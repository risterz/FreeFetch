use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use uuid::Uuid;

use freefetch_lib::core::config::AppSettings;
use freefetch_lib::core::types::{
    BatchItemStatus, ItemAvailability, PlaylistInfo, PlaylistItem, PlaylistItemSelection,
};
use freefetch_lib::engine::queue::QueueManager;
use freefetch_lib::engine::{EngineManager, StartDownloadParams};
use freefetch_lib::extractor::{ExtractorRegistry, MediaFormat};
use freefetch_lib::service::batch::{add_batch_to_queue, analyze_batch_urls, parse_batch_urls};
use freefetch_lib::service::playlist::{
    detect_playlist_url, queue_playlist_items, select_format_for_quality,
};
use freefetch_lib::storage::create_in_memory_database;

/// Simple mock HTTP server for testing batch & playlist download jobs
async fn run_mock_server<F>(handler: F) -> (SocketAddr, tokio::sync::oneshot::Sender<()>)
where
    F: Fn(String, usize) -> Option<Vec<u8>> + Send + Sync + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let handler = Arc::new(handler);
    let req_counter = Arc::new(AtomicUsize::new(0));

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut shutdown_rx => break,
                accept_res = listener.accept() => {
                    if let Ok((mut socket, _)) = accept_res {
                        let h = handler.clone();
                        let count = req_counter.clone();
                        tokio::spawn(async move {
                            let mut buf = [0u8; 4096];
                            let n = match socket.read(&mut buf).await {
                                Ok(n) if n > 0 => n,
                                _ => return,
                            };
                            let req_str = String::from_utf8_lossy(&buf[..n]).to_string();
                            let req_idx = count.fetch_add(1, Ordering::SeqCst);

                            if let Some(resp_bytes) = h(req_str, req_idx) {
                                let _ = socket.write_all(&resp_bytes).await;
                                let _ = socket.shutdown().await;
                            }
                        });
                    }
                }
            }
        }
    });

    (addr, shutdown_tx)
}

fn sample_params(url: &str, format_id: &str, title: &str, dest_dir: &str) -> StartDownloadParams {
    StartDownloadParams {
        source_url: url.to_string(),
        service_name: "generic".to_string(),
        title: title.to_string(),
        duration: 120,
        thumbnail_url: None,
        format_id: format_id.to_string(),
        download_url: url.to_string(),
        requires_merge: false,
        matching_audio_url: None,
        container: "mp4".to_string(),
        quality_label: "720p".to_string(),
        destination_dir: Some(dest_dir.to_string()),
        selected_subtitles: Vec::new(),
        embed_subtitles: false,
        subtitle_mode: None,
        audio_options: None,
        video_codec: None,
        audio_codec: None,
    http_headers: None,
    }
}

// ---------------------------------------------------------------------------
// 1. Batch URL Parsing & Deduplication Tests
// ---------------------------------------------------------------------------
#[test]
fn test_batch_url_parsing_and_deduplication() {
    let input = r#"
        https://www.youtube.com/watch?v=dQw4w9WgXcQ
        
        https://youtu.be/dQw4w9WgXcQ
        
        not a valid url
        https://example.com/video1.mp4   https://example.com/video2.mp4
        
        ftp://invalid-scheme.com/file
        https://www.youtube.com/watch?v=dQw4w9WgXcQ&feature=share
        https://example.com/video1.mp4
        
        Just some random text
        https://example.com/video3.mp4
    "#;

    let parsed = parse_batch_urls(input);

    // 1. YouTube canonicalization deduplicates youtu.be, watch?v=, and share params to 1 unique entry
    // 2. example.com/video1.mp4 is deduplicated
    // 3. example.com/video2.mp4 is kept
    // 4. example.com/video3.mp4 is kept
    // 5. invalid URLs / schemes / text are omitted
    assert_eq!(parsed.len(), 4);
    assert_eq!(parsed[0], "https://www.youtube.com/watch?v=dQw4w9WgXcQ");
    assert_eq!(parsed[1], "https://example.com/video1.mp4");
    assert_eq!(parsed[2], "https://example.com/video2.mp4");
    assert_eq!(parsed[3], "https://example.com/video3.mp4");
}

#[test]
fn test_batch_url_parsing_windows_crlf_and_whitespace() {
    let input = "https://example.com/a.mp4\r\n\r\n   https://example.com/b.mp4  \r\n\r\n";
    let parsed = parse_batch_urls(input);
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0], "https://example.com/a.mp4");
    assert_eq!(parsed[1], "https://example.com/b.mp4");
}

#[test]
fn test_batch_url_empty_input() {
    assert!(parse_batch_urls("").is_empty());
    assert!(parse_batch_urls("   \n\r\n   \t  ").is_empty());
    assert!(parse_batch_urls("hello world foo bar").is_empty());
}

// ---------------------------------------------------------------------------
// 2. Playlist URL Detection Tests
// ---------------------------------------------------------------------------
#[test]
fn test_playlist_url_detection() {
    // Pure playlist
    let pure = "https://www.youtube.com/playlist?list=PLrEnWoR732-BHrPp_Pm8_VleD68f9n14-";
    let det_pure = detect_playlist_url(pure).expect("Should detect pure playlist");
    assert_eq!(det_pure.playlist_id, "PLrEnWoR732-BHrPp_Pm8_VleD68f9n14-");
    assert!(det_pure.is_pure_playlist);
    assert!(det_pure.video_id.is_none());
    assert_eq!(
        det_pure.canonical_playlist_url,
        "https://www.youtube.com/playlist?list=PLrEnWoR732-BHrPp_Pm8_VleD68f9n14-"
    );

    // Watch URL with playlist context
    let watch_pl = "https://www.youtube.com/watch?v=dQw4w9WgXcQ&list=PLrEnWoR732-BHrPp_Pm8_VleD68f9n14-&index=2";
    let det_watch = detect_playlist_url(watch_pl).expect("Should detect watch with playlist");
    assert_eq!(det_watch.playlist_id, "PLrEnWoR732-BHrPp_Pm8_VleD68f9n14-");
    assert!(!det_watch.is_pure_playlist);
    assert_eq!(det_watch.video_id.as_deref(), Some("dQw4w9WgXcQ"));

    // Non-playlist YouTube video
    let single = "https://www.youtube.com/watch?v=dQw4w9WgXcQ";
    assert!(detect_playlist_url(single).is_none());

    // Non-YouTube URL with 'list' param
    let other = "https://example.com/items?list=123";
    assert!(detect_playlist_url(other).is_none());
}

// ---------------------------------------------------------------------------
// 3. Format Selection Policy Tests
// ---------------------------------------------------------------------------
#[test]
fn test_format_selection_policy() {
    let formats = vec![
        MediaFormat {
            format_id: "audio_128".to_string(),
            format_type: "audio".to_string(),
            quality_label: "128kbps".to_string(),
            resolution: None,
            width: None,
            height: None,
            fps: None,
            video_codec: None,
            audio_codec: Some("aac".to_string()),
            mime_type: Some("audio/mp4".to_string()),
            bitrate: Some(128000),
            container: "m4a".to_string(),
            filesize: Some(1_000_000),
            download_url: "https://example.com/audio.m4a".to_string(),
            requires_merge: false,
            matching_audio_url: None,
        http_headers: None,
        },
        MediaFormat {
            format_id: "vid_480p".to_string(),
            format_type: "video".to_string(),
            quality_label: "480p".to_string(),
            resolution: Some("854x480".to_string()),
            width: Some(854),
            height: Some(480),
            fps: Some(30),
            video_codec: Some("h264".to_string()),
            audio_codec: None,
            mime_type: Some("video/mp4".to_string()),
            bitrate: Some(800000),
            container: "mp4".to_string(),
            filesize: Some(5_000_000),
            download_url: "https://example.com/480.mp4".to_string(),
            requires_merge: true,
            matching_audio_url: Some("https://example.com/audio.m4a".to_string()),
        http_headers: None,
        },
        MediaFormat {
            format_id: "vid_720p".to_string(),
            format_type: "video".to_string(),
            quality_label: "720p".to_string(),
            resolution: Some("1280x720".to_string()),
            width: Some(1280),
            height: Some(720),
            fps: Some(60),
            video_codec: Some("h264".to_string()),
            audio_codec: None,
            mime_type: Some("video/mp4".to_string()),
            bitrate: Some(2000000),
            container: "mp4".to_string(),
            filesize: Some(15_000_000),
            download_url: "https://example.com/720.mp4".to_string(),
            requires_merge: true,
            matching_audio_url: Some("https://example.com/audio.m4a".to_string()),
        http_headers: None,
        },
    ];

    // Audio preference
    let f_audio = select_format_for_quality(&formats, "Audio Only").unwrap();
    assert_eq!(f_audio.format_id, "audio_128");

    // Exact 720p
    let f_720 = select_format_for_quality(&formats, "720p").unwrap();
    assert_eq!(f_720.format_id, "vid_720p");

    // Exact 480p
    let f_480 = select_format_for_quality(&formats, "480p").unwrap();
    assert_eq!(f_480.format_id, "vid_480p");

    // Best quality -> selects 720p (highest available)
    let f_best = select_format_for_quality(&formats, "Best Quality").unwrap();
    assert_eq!(f_best.format_id, "vid_720p");

    // 1080p requested but unavailable -> falls back to closest <= 1080p (720p)
    let f_1080_fallback = select_format_for_quality(&formats, "1080p").unwrap();
    assert_eq!(f_1080_fallback.format_id, "vid_720p");
}

// ---------------------------------------------------------------------------
// 4. Batch Analysis with Bounded Concurrency & Failure Isolation
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_batch_analysis_with_bounded_concurrency() {
    let payload = b"<html><head><title>Test Media</title><meta property=\"og:video\" content=\"http://example.com/v.mp4\"/></head><body></body></html>";

    let (addr, shutdown_tx) = run_mock_server(move |req, _idx| {
        if req.contains("/bad") {
            let resp = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";
            return Some(resp.as_bytes().to_vec());
        }

        let resp_header = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n",
            payload.len()
        );
        let mut resp = resp_header.into_bytes();
        resp.extend_from_slice(payload);
        Some(resp)
    })
    .await;

    let registry = Arc::new(ExtractorRegistry::new());

    let urls = vec![
        format!("http://{addr}/good1.html"),
        format!("http://{addr}/bad1.html"),
        format!("http://{addr}/good2.html"),
        format!("http://{addr}/bad2.html"),
        format!("http://{addr}/good3.html"),
    ];

    // Concurrency limit of 3
    let res = analyze_batch_urls(&registry, &urls, 3).await;

    assert_eq!(res.total, 5);
    assert_eq!(res.ready_count, 3, "3 good URLs should succeed");
    assert_eq!(
        res.failed_count, 2,
        "2 bad URLs should fail without aborting"
    );

    assert_eq!(res.items[0].status, BatchItemStatus::Ready);
    assert_eq!(res.items[1].status, BatchItemStatus::Failed);
    assert_eq!(res.items[2].status, BatchItemStatus::Ready);
    assert_eq!(res.items[3].status, BatchItemStatus::Failed);
    assert_eq!(res.items[4].status, BatchItemStatus::Ready);

    let _ = shutdown_tx.send(());
}

// ---------------------------------------------------------------------------
// 5. Batch Addition to Queue & Duplicate Protection
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_batch_add_to_queue_and_duplicates() {
    let repo = create_in_memory_database().unwrap();
    let registry = Arc::new(ExtractorRegistry::new());
    let engine = EngineManager::new(repo.clone(), registry);
    let settings = Arc::new(Mutex::new(AppSettings::default()));
    let qm = QueueManager::new(repo, engine, settings);

    let temp_dir = std::env::temp_dir().join(format!("ff_batch_q_{}", Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&temp_dir);
    let dest = temp_dir.to_string_lossy().to_string();

    let items = vec![
        sample_params("https://example.com/v1", "720p", "Video 1", &dest),
        sample_params("https://example.com/v2", "720p", "Video 2", &dest),
        sample_params("https://example.com/v1", "720p", "Video 1 Duplicate", &dest),
        sample_params("https://example.com/v3", "720p", "Video 3", &dest),
    ];

    let res = add_batch_to_queue(&qm, items).await.unwrap();

    assert_eq!(res.added_count, 3, "v1, v2, v3 should be added");
    assert_eq!(res.duplicate_count, 1, "Duplicate v1 should be skipped");
    assert_eq!(res.failed_count, 0);

    let queue_items = qm.get_items().unwrap();
    assert_eq!(queue_items.len(), 3);
    assert_eq!(queue_items[0].title, "Video 1");
    assert_eq!(queue_items[0].position, 0);
    assert_eq!(queue_items[1].title, "Video 2");
    assert_eq!(queue_items[1].position, 1);
    assert_eq!(queue_items[2].title, "Video 3");
    assert_eq!(queue_items[2].position, 2);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

// ---------------------------------------------------------------------------
// 6. Playlist Selection to Queue Integration
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_playlist_selection_to_queue_preserves_order() {
    let payload = b"<html><head><title>Playlist Track</title><meta property=\"og:video\" content=\"http://example.com/track.mp4\"/></head><body></body></html>";

    let (addr, shutdown_tx) = run_mock_server(move |_req, _idx| {
        let resp_header = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n",
            payload.len()
        );
        let mut resp = resp_header.into_bytes();
        resp.extend_from_slice(payload);
        Some(resp)
    })
    .await;

    let repo = create_in_memory_database().unwrap();
    let registry = Arc::new(ExtractorRegistry::new());
    let engine = EngineManager::new(repo.clone(), registry.clone());
    let settings = Arc::new(Mutex::new(AppSettings::default()));
    let qm = QueueManager::new(repo, engine, settings);

    let temp_dir = std::env::temp_dir().join(format!("ff_pl_q_{}", Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&temp_dir);
    let dest = temp_dir.to_string_lossy().to_string();

    // User selected tracks 1, 3, 4 from playlist
    let selected_items = vec![
        PlaylistItemSelection {
            id: "t1".to_string(),
            url: format!("http://{addr}/t1.html"),
            title: "Track 01".to_string(),
            duration: 180,
            thumbnail: None,
            format_id: None,
            selected_subtitles: Vec::new(),
        },
        PlaylistItemSelection {
            id: "t3".to_string(),
            url: format!("http://{addr}/t3.html"),
            title: "Track 03".to_string(),
            duration: 210,
            thumbnail: None,
            format_id: None,
            selected_subtitles: Vec::new(),
        },
        PlaylistItemSelection {
            id: "t4".to_string(),
            url: format!("http://{addr}/t4.html"),
            title: "Track 04".to_string(),
            duration: 240,
            thumbnail: None,
            format_id: None,
            selected_subtitles: Vec::new(),
        },
    ];

    let res = queue_playlist_items(
        &registry,
        &qm,
        selected_items,
        "720p".to_string(),
        Some(dest),
    )
    .await
    .unwrap();

    assert_eq!(res.added_count, 3);
    assert_eq!(res.duplicate_count, 0);
    assert_eq!(res.failed_count, 0);

    let q_items = qm.get_items().unwrap();
    assert_eq!(q_items.len(), 3);
    assert_eq!(q_items[0].title, "Playlist Track");
    assert_eq!(q_items[0].position, 0);
    assert_eq!(q_items[1].position, 1);
    assert_eq!(q_items[2].position, 2);

    let _ = shutdown_tx.send(());
    let _ = std::fs::remove_dir_all(&temp_dir);
}

// ---------------------------------------------------------------------------
// 7. Playlist Item Availability & Large Scale Handling
// ---------------------------------------------------------------------------
#[test]
fn test_playlist_availability_and_filtering() {
    let raw_entries = vec![
        ("v1", "Normal Video 1", ItemAvailability::Available),
        ("v2", "[Private video]", ItemAvailability::Private),
        ("v3", "Normal Video 2", ItemAvailability::Available),
        ("v4", "[Deleted video]", ItemAvailability::Deleted),
        ("v5", "[Unavailable video]", ItemAvailability::Unavailable),
    ];

    let items: Vec<PlaylistItem> = raw_entries
        .into_iter()
        .enumerate()
        .map(|(idx, (id, title, avail))| PlaylistItem {
            id: id.to_string(),
            index: (idx + 1) as u32,
            url: format!("https://www.youtube.com/watch?v={id}"),
            canonical_url: format!("https://www.youtube.com/watch?v={id}"),
            title: title.to_string(),
            thumbnail: None,
            duration: 120,
            availability: avail,
        })
        .collect();

    let playlist = PlaylistInfo {
        id: "PL123".to_string(),
        canonical_url: "https://www.youtube.com/playlist?list=PL123".to_string(),
        title: "Test Playlist".to_string(),
        thumbnail: None,
        uploader: Some("Tester".to_string()),
        item_count: items.len(),
        items,
    };

    assert_eq!(playlist.item_count, 5);
    let available_count = playlist
        .items
        .iter()
        .filter(|i| i.availability == ItemAvailability::Available)
        .count();
    let private_count = playlist
        .items
        .iter()
        .filter(|i| i.availability == ItemAvailability::Private)
        .count();
    let deleted_count = playlist
        .items
        .iter()
        .filter(|i| i.availability == ItemAvailability::Deleted)
        .count();
    let unavailable_count = playlist
        .items
        .iter()
        .filter(|i| i.availability == ItemAvailability::Unavailable)
        .count();

    assert_eq!(available_count, 2);
    assert_eq!(private_count, 1);
    assert_eq!(deleted_count, 1);
    assert_eq!(unavailable_count, 1);
}
