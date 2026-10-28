use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::watch;
use url::Url;
use uuid::Uuid;

use freefetch_lib::core::config::AppSettings;
use freefetch_lib::core::error::FreeFetchError;
use freefetch_lib::core::types::{QueueStatus, SubtitleFormat, SubtitleSelection};
use freefetch_lib::engine::queue::QueueManager;
use freefetch_lib::engine::{EngineManager, StartDownloadParams};
use freefetch_lib::extractor::implementations::youtube::extract_youtube_id;
use freefetch_lib::extractor::models::{MediaMetadata, StreamManifest};
use freefetch_lib::extractor::{ExtractionContext, Extractor, ExtractorRegistry};
use freefetch_lib::media::ffmpeg::FfmpegController;
use freefetch_lib::storage::create_in_memory_database;

/// Helper: temporary directory
fn get_test_dir(prefix: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("{}_{}", prefix, Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Helper: Simple mock HTTP server
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
                                let _ = socket.flush().await;
                            }
                        });
                    }
                }
            }
        }
    });

    (addr, shutdown_tx)
}

// ---------------------------------------------------------------------------
// P0-1: Queue Stall on Initialization Error
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_regression_queue_init_failure_frees_slot_and_advances_queue() {
    let repo = create_in_memory_database().unwrap();
    let registry = Arc::new(ExtractorRegistry::new());
    let engine = EngineManager::new(repo.clone(), registry);
    let settings = Arc::new(Mutex::new(AppSettings {
        max_concurrent_downloads: 1, // Concurrency clamped to 1
        ..AppSettings::default()
    }));
    let queue = QueueManager::new(repo.clone(), engine, settings);

    // Item 1: Has an invalid URL that fails start_download_with_id immediately
    let bad_params = StartDownloadParams {
        source_url: "https://example.com/item1.mp4".to_string(),
        service_name: "generic".to_string(),
        title: "Bad Item".to_string(),
        duration: 10,
        thumbnail_url: None,
        format_id: "fmt-1".to_string(),
        download_url: "ftp://invalid-scheme-not-http-or-https/test.mp4".to_string(),
        requires_merge: false,
        matching_audio_url: None,
        container: "mp4".to_string(),
        quality_label: "720p".to_string(),
        destination_dir: None,
        selected_subtitles: Vec::new(),
        embed_subtitles: false,
        subtitle_mode: None,
        audio_options: None,
        video_codec: None,
        audio_codec: None,
    };

    // Item 2: Queued behind Item 1
    let good_params = StartDownloadParams {
        source_url: "https://example.com/item2.mp4".to_string(),
        service_name: "generic".to_string(),
        title: "Good Item".to_string(),
        duration: 10,
        thumbnail_url: None,
        format_id: "fmt-2".to_string(),
        download_url: "https://example.com/item2.mp4".to_string(),
        requires_merge: false,
        matching_audio_url: None,
        container: "mp4".to_string(),
        quality_label: "720p".to_string(),
        destination_dir: None,
        selected_subtitles: Vec::new(),
        embed_subtitles: false,
        subtitle_mode: None,
        audio_options: None,
        video_codec: None,
        audio_codec: None,
    };

    let item1 = queue.add(bad_params).await.unwrap();
    let item2 = queue.add(good_params).await.unwrap();

    // Give background async tasks time to execute process_next
    tokio::time::sleep(Duration::from_millis(500)).await;

    let item1_after = repo.get_queue_item(&item1.id).unwrap().unwrap();
    assert_eq!(
        item1_after.status,
        QueueStatus::Failed,
        "Failed item must transition to Failed, never stay stuck in Preparing"
    );
    assert_eq!(
        item1_after.error_code.as_deref(),
        Some("INIT_FAILED"),
        "Must specify INIT_FAILED error code"
    );
    assert!(
        item1_after.error_message.is_some(),
        "Must preserve underlying actionable error message"
    );

    // Verify Item 2 was unblocked and allowed to start
    let item2_after = repo.get_queue_item(&item2.id).unwrap().unwrap();
    assert!(
        matches!(
            item2_after.status,
            QueueStatus::Preparing | QueueStatus::Downloading | QueueStatus::Failed
        ),
        "Item 2 must have started after Item 1 failed. Actual status: {:?}",
        item2_after.status
    );
}

// ---------------------------------------------------------------------------
// P0-2: Ephemeral Subtitle URL Resolution
// ---------------------------------------------------------------------------

struct MockSubtitleExtractor {
    timedtext_url: String,
}

#[async_trait::async_trait]
impl Extractor for MockSubtitleExtractor {
    fn name(&self) -> &'static str {
        "youtube"
    }

    fn can_handle(&self, url: &Url) -> bool {
        url.host_str().unwrap_or("").contains("youtube.com") && extract_youtube_id(url).is_some()
    }

    async fn extract(
        &self,
        url: &Url,
        _ctx: &ExtractionContext,
    ) -> Result<(MediaMetadata, StreamManifest), FreeFetchError> {
        let meta = MediaMetadata {
            id: "MOCK_SUB_VID".to_string(),
            url: url.to_string(),
            title: "Mock Subtitle Video".to_string(),
            duration: 100,
            thumbnail_url: None,
            uploader: Some("Mock Channel".to_string()),
            service_name: "youtube".to_string(),
            description: None,
            upload_date: None,
        };

        let tracks = vec![freefetch_lib::core::types::SubtitleTrack {
            id: "yt-sub-en-manual".to_string(),
            language_code: "en".to_string(),
            language_name: "English".to_string(),
            format: SubtitleFormat::Vtt,
            is_auto_generated: false,
            is_default: true,
            download_url: Some(self.timedtext_url.clone()),
        }];

        Ok((
            meta,
            StreamManifest {
                formats: Vec::new(),
                subtitles: tracks,
            },
        ))
    }
}

#[tokio::test]
async fn test_regression_subtitle_no_signed_url_persisted_in_sqlite() {
    let repo = create_in_memory_database().unwrap();
    let registry = Arc::new(ExtractorRegistry::new());
    let engine = EngineManager::new(repo.clone(), registry);
    let settings = Arc::new(Mutex::new(AppSettings::default()));
    let queue = QueueManager::new(repo.clone(), engine, settings);

    let media_params = StartDownloadParams {
        source_url: "https://www.youtube.com/watch?v=TEST_NO_SIGN".to_string(),
        service_name: "youtube".to_string(),
        title: "Test Video".to_string(),
        duration: 120,
        thumbnail_url: None,
        format_id: "yt-1080p".to_string(),
        download_url: "https://example.com/video.mp4".to_string(),
        requires_merge: false,
        matching_audio_url: None,
        container: "mp4".to_string(),
        quality_label: "1080p".to_string(),
        destination_dir: None,
        selected_subtitles: Vec::new(),
        embed_subtitles: false,
        subtitle_mode: None,
        audio_options: None,
        video_codec: None,
        audio_codec: None,
    };

    let sub_selection = vec![SubtitleSelection {
        language_code: "en".to_string(),
        language_name: "English".to_string(),
        target_format: SubtitleFormat::Srt,
        is_auto_generated: false,
        download_url: Some(
            "https://www.youtube.com/api/timedtext?expire=12345&sig=abcdef".to_string(),
        ),
    }];

    let items = queue
        .add_with_subtitles(media_params, sub_selection)
        .await
        .unwrap();
    assert_eq!(items.len(), 2);

    let sub_item = &items[1];
    assert!(sub_item.format_id.starts_with("sub:en:"));

    // Check stored SQLite record: signed timedtext URL MUST NOT be in params_json
    let stored_sub = repo.get_queue_item(&sub_item.id).unwrap().unwrap();
    assert!(
        !stored_sub.params_json.contains("api/timedtext"),
        "Ephemeral/signed timedtext URL must NOT be persisted in SQLite"
    );
    assert!(
        !stored_sub.params_json.contains("sig=abcdef"),
        "Signature parameters must NOT be persisted in SQLite"
    );
}

#[tokio::test]
async fn test_regression_subtitle_url_absent_deferred_resolution() {
    let vtt_content = "WEBVTT\n\n00:00:01.000 --> 00:00:03.000\nDeferred subtitle text.\n";
    let (addr, shutdown) = run_mock_server(move |_req, _idx| {
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/vtt\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            vtt_content.len(),
            vtt_content
        );
        Some(resp.into_bytes())
    })
    .await;

    let timedtext_url = format!("http://127.0.0.1:{}/timedtext?fmt=vtt", addr.port());

    let mut reg = ExtractorRegistry::new();
    reg.register(Arc::new(MockSubtitleExtractor { timedtext_url }));
    let registry = Arc::new(reg);

    let repo = create_in_memory_database().unwrap();
    let engine = EngineManager::new(repo.clone(), registry);
    let tmp_dir = get_test_dir("ff_sub_deferred");
    let settings = Arc::new(Mutex::new(AppSettings {
        download_directory: tmp_dir.to_string_lossy().to_string(),
        ..AppSettings::default()
    }));
    let queue = QueueManager::new(repo.clone(), engine, settings);

    // Subtitle item created with EMPTY download_url
    let sub_params = StartDownloadParams {
        source_url: "https://www.youtube.com/watch?v=TEST_DEF_12".to_string(),
        service_name: "youtube".to_string(),
        title: "Test Deferred Video.en".to_string(),
        duration: 100,
        thumbnail_url: None,
        format_id: "sub:en:srt".to_string(),
        download_url: String::new(), // NO URL provided upfront
        requires_merge: false,
        matching_audio_url: None,
        container: "srt".to_string(),
        quality_label: "English".to_string(),
        destination_dir: Some(tmp_dir.to_string_lossy().to_string()),
        selected_subtitles: Vec::new(),
        embed_subtitles: false,
        subtitle_mode: None,
        audio_options: None,
        video_codec: None,
        audio_codec: None,
    };

    let item = queue.add(sub_params).await.unwrap();

    // Await execution
    tokio::time::sleep(Duration::from_millis(800)).await;

    let item_after = repo.get_queue_item(&item.id).unwrap().unwrap();
    assert_eq!(
        item_after.status,
        QueueStatus::Completed,
        "Deferred subtitle resolution must download and complete successfully"
    );

    let srt_file = tmp_dir.join("Test Deferred Video.en.srt");
    assert!(srt_file.exists(), "Target SRT file must be created on disk");
    let content = std::fs::read_to_string(&srt_file).unwrap();
    assert!(content.contains("Deferred subtitle text."));

    let _ = shutdown.send(());
    let _ = std::fs::remove_dir_all(&tmp_dir);
}

// ---------------------------------------------------------------------------
// P1-1: FFmpeg Cancellation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_regression_ffmpeg_cancellation_and_child_termination() {
    let controller = match FfmpegController::discover(None) {
        Ok(c) => c,
        Err(_) => return, // Skip if FFmpeg not available on machine
    };

    let tmp_dir = get_test_dir("ff_ffmpeg_cancel");
    let video_input = tmp_dir.join("sample_v.mp4");
    let audio_input = tmp_dir.join("sample_a.m4a");
    let out_file = tmp_dir.join("cancelled_output.mp4");

    // 1. Generate sample video input
    let _ = std::process::Command::new(controller.binary_path())
        .arg("-y")
        .arg("-f")
        .arg("lavfi")
        .arg("-i")
        .arg("testsrc=duration=2:size=320x240:rate=10")
        .arg(&video_input)
        .output();

    // 2. Generate sample audio input
    let _ = std::process::Command::new(controller.binary_path())
        .arg("-y")
        .arg("-f")
        .arg("lavfi")
        .arg("-i")
        .arg("sine=frequency=1000:duration=2")
        .arg(&audio_input)
        .output();

    // Test A: Pre-cancelled receiver
    let (_cancel_tx1, cancel_rx1) = watch::channel(true);
    let res1 = controller
        .merge_streams_cancellable(&video_input, &audio_input, &out_file, Some(cancel_rx1))
        .await;
    assert!(
        matches!(res1, Err(FreeFetchError::Cancelled)),
        "Pre-cancelled FFmpeg call must return FreeFetchError::Cancelled"
    );

    // Test B: Active cancellation signal during execution
    let (cancel_tx2, cancel_rx2) = watch::channel(false);
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let _ = cancel_tx2.send(true);
    });

    let res2 = controller
        .merge_streams_cancellable(&video_input, &audio_input, &out_file, Some(cancel_rx2))
        .await;

    assert!(
        matches!(res2, Err(FreeFetchError::Cancelled)),
        "FFmpeg operation must terminate on cancel signal and return FreeFetchError::Cancelled"
    );

    assert!(
        !out_file.exists(),
        "Incomplete output file must be cleaned up on cancellation"
    );

    let _ = std::fs::remove_dir_all(&tmp_dir);
}

// ---------------------------------------------------------------------------
// P1-2: Windows .part and .part.meta Cleanup
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_regression_windows_cancellation_cleans_part_and_meta() {
    let tmp_dir = get_test_dir("ff_cleanup_cancel");
    let dest = tmp_dir.join("test_download.mp4");

    // Create dummy .part and .part.meta files
    let part_file = tmp_dir.join("test_download.mp4.part");
    let meta_file = tmp_dir.join("test_download.mp4.part.meta");
    let video_part = tmp_dir.join("test_download.video.part");
    let video_meta = tmp_dir.join("test_download.video.part.meta");

    std::fs::write(&part_file, b"partial data").unwrap();
    std::fs::write(&meta_file, b"metadata").unwrap();
    std::fs::write(&video_part, b"video data").unwrap();
    std::fs::write(&video_meta, b"video meta").unwrap();

    assert!(part_file.exists());
    assert!(meta_file.exists());
    assert!(video_part.exists());
    assert!(video_meta.exists());

    // Execute cancellation cleanup
    EngineManager::clean_cancelled_files(&dest, "mp4").await;

    assert!(!part_file.exists(), ".part file must be removed on cancel");
    assert!(
        !meta_file.exists(),
        ".part.meta file must be removed on cancel"
    );
    assert!(!video_part.exists(), "video.part must be removed on cancel");
    assert!(
        !video_meta.exists(),
        "video.part.meta must be removed on cancel"
    );

    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[test]
fn test_regression_paused_download_preserves_part_and_meta() {
    let tmp_dir = get_test_dir("ff_cleanup_pause");
    let part_file = tmp_dir.join("test_download.mp4.part");
    let meta_file = tmp_dir.join("test_download.mp4.part.meta");

    std::fs::write(&part_file, b"partial data").unwrap();
    std::fs::write(&meta_file, b"metadata").unwrap();

    // Pausing a download must NEVER delete .part or .part.meta!
    assert!(part_file.exists());
    assert!(meta_file.exists());

    let _ = std::fs::remove_dir_all(&tmp_dir);
}

// ---------------------------------------------------------------------------
// P1-3: Late Worker State Overwriting Cancelled / Completed / Paused
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_regression_late_worker_error_cannot_overwrite_cancelled() {
    let repo = create_in_memory_database().unwrap();
    let registry = Arc::new(ExtractorRegistry::new());
    let engine = EngineManager::new(repo.clone(), registry);
    let settings = Arc::new(Mutex::new(AppSettings::default()));
    let queue = QueueManager::new(repo.clone(), engine, settings);

    let params = StartDownloadParams {
        source_url: "https://example.com/cancelled.mp4".to_string(),
        service_name: "generic".to_string(),
        title: "Cancelled Video".to_string(),
        duration: 100,
        thumbnail_url: None,
        format_id: "fmt-1".to_string(),
        download_url: "https://example.com/cancelled.mp4".to_string(),
        requires_merge: false,
        matching_audio_url: None,
        container: "mp4".to_string(),
        quality_label: "720p".to_string(),
        destination_dir: None,
        selected_subtitles: Vec::new(),
        embed_subtitles: false,
        subtitle_mode: None,
        audio_options: None,
        video_codec: None,
        audio_codec: None,
    };

    let item = queue.add(params).await.unwrap();

    // User cancels the item
    queue.cancel_item(&item.id).await.unwrap();

    let state_before = repo.get_queue_item(&item.id).unwrap().unwrap();
    assert_eq!(state_before.status, QueueStatus::Cancelled);

    // Stale worker emits a late failure
    queue
        .handle_engine_state_change(&item.id, "failed", Some("Network timeout while aborting"))
        .await;

    let state_after = repo.get_queue_item(&item.id).unwrap().unwrap();
    assert_eq!(
        state_after.status,
        QueueStatus::Cancelled,
        "Cancelled item MUST NOT be overwritten to Failed by a late worker event"
    );

    // Stale worker emits downloading
    queue
        .handle_engine_state_change(&item.id, "downloading", None)
        .await;

    let state_after2 = repo.get_queue_item(&item.id).unwrap().unwrap();
    assert_eq!(
        state_after2.status,
        QueueStatus::Cancelled,
        "Cancelled item MUST NOT be overwritten to Downloading"
    );
}

#[tokio::test]
async fn test_regression_late_worker_error_cannot_overwrite_completed() {
    let repo = create_in_memory_database().unwrap();
    let registry = Arc::new(ExtractorRegistry::new());
    let engine = EngineManager::new(repo.clone(), registry);
    let settings = Arc::new(Mutex::new(AppSettings::default()));
    let queue = QueueManager::new(repo.clone(), engine, settings);

    let params = StartDownloadParams {
        source_url: "https://example.com/completed.mp4".to_string(),
        service_name: "generic".to_string(),
        title: "Completed Video".to_string(),
        duration: 100,
        thumbnail_url: None,
        format_id: "fmt-1".to_string(),
        download_url: "https://example.com/completed.mp4".to_string(),
        requires_merge: false,
        matching_audio_url: None,
        container: "mp4".to_string(),
        quality_label: "720p".to_string(),
        destination_dir: None,
        selected_subtitles: Vec::new(),
        embed_subtitles: false,
        subtitle_mode: None,
        audio_options: None,
        video_codec: None,
        audio_codec: None,
    };

    let item = queue.add(params).await.unwrap();

    // Simulate completion
    queue
        .handle_engine_state_change(&item.id, "completed", None)
        .await;

    let state_before = repo.get_queue_item(&item.id).unwrap().unwrap();
    assert_eq!(state_before.status, QueueStatus::Completed);

    // Late worker error arrives
    queue
        .handle_engine_state_change(&item.id, "failed", Some("Worker error"))
        .await;

    let state_after = repo.get_queue_item(&item.id).unwrap().unwrap();
    assert_eq!(
        state_after.status,
        QueueStatus::Completed,
        "Completed item MUST NOT be overwritten to Failed"
    );
}

#[tokio::test]
async fn test_regression_stale_worker_progress_cannot_overwrite_paused() {
    let repo = create_in_memory_database().unwrap();
    let registry = Arc::new(ExtractorRegistry::new());
    let engine = EngineManager::new(repo.clone(), registry);
    let settings = Arc::new(Mutex::new(AppSettings::default()));
    let queue = QueueManager::new(repo.clone(), engine, settings);

    let params = StartDownloadParams {
        source_url: "https://example.com/paused.mp4".to_string(),
        service_name: "generic".to_string(),
        title: "Paused Video".to_string(),
        duration: 100,
        thumbnail_url: None,
        format_id: "fmt-1".to_string(),
        download_url: "https://example.com/paused.mp4".to_string(),
        requires_merge: false,
        matching_audio_url: None,
        container: "mp4".to_string(),
        quality_label: "720p".to_string(),
        destination_dir: None,
        selected_subtitles: Vec::new(),
        embed_subtitles: false,
        subtitle_mode: None,
        audio_options: None,
        video_codec: None,
        audio_codec: None,
    };

    let item = queue.add(params).await.unwrap();

    // User pauses item
    queue.pause_item(&item.id).await.unwrap();

    let state_before = repo.get_queue_item(&item.id).unwrap().unwrap();
    assert_eq!(state_before.status, QueueStatus::Paused);

    // Stale worker emits downloading
    queue
        .handle_engine_state_change(&item.id, "downloading", None)
        .await;

    let state_after = repo.get_queue_item(&item.id).unwrap().unwrap();
    assert_eq!(
        state_after.status,
        QueueStatus::Paused,
        "Paused item MUST NOT be overwritten to Downloading by stale worker event"
    );

    // Stale worker emits failed
    queue
        .handle_engine_state_change(&item.id, "failed", Some("Worker aborted"))
        .await;

    let state_after2 = repo.get_queue_item(&item.id).unwrap().unwrap();
    assert_eq!(
        state_after2.status,
        QueueStatus::Paused,
        "Paused item MUST NOT be overwritten to Failed by stale worker event"
    );
}

#[tokio::test]
async fn test_regression_subtitle_403_and_410_url_refresh() {
    let vtt_body = "WEBVTT\n\n00:00:01.000 --> 00:00:04.000\nRefreshed subtitle line.\n";
    let (addr, shutdown) = run_mock_server(move |req, _idx| {
        if req.contains("stream_expired") {
            Some(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec())
        } else if req.contains("stream_fresh") {
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/vtt\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                vtt_body.len(),
                vtt_body
            );
            Some(resp.into_bytes())
        } else {
            None
        }
    })
    .await;

    let client = reqwest::Client::new();
    let tmp_dir = get_test_dir("ff_sub_refresh_403");
    let dest_file = tmp_dir.join("sub_refreshed.srt");

    let expired_url = format!("http://127.0.0.1:{}/timedtext/stream_expired", addr.port());
    let fresh_url = format!("http://127.0.0.1:{}/timedtext/stream_fresh", addr.port());

    let refresher: freefetch_lib::engine::chunk_downloader::UrlRefresher = {
        let f_url = fresh_url.clone();
        Arc::new(move || {
            let u = f_url.clone();
            Box::pin(async move { Ok(u) })
        })
    };

    let res = freefetch_lib::engine::subtitle_downloader::download_and_convert_subtitle(
        &client,
        &expired_url,
        &dest_file,
        SubtitleFormat::Srt,
        None,
        Some(refresher),
    )
    .await;

    assert!(
        res.is_ok(),
        "Subtitle downloader must refresh expired URL on 403 and succeed: {:?}",
        res.err()
    );

    assert!(dest_file.exists());
    let content = std::fs::read_to_string(&dest_file).unwrap();
    assert!(content.contains("Refreshed subtitle line."));

    let _ = shutdown.send(());
    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[tokio::test]
async fn test_regression_subtitle_resolution_failure_clean_state() {
    let (addr, shutdown) = run_mock_server(|req, _idx| {
        if req.contains("GET /media.mp4") {
            let body = b"fake video content for testing";
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: video/mp4\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let mut v = resp.into_bytes();
            v.extend_from_slice(body);
            Some(v)
        } else {
            None
        }
    })
    .await;

    let timedtext_url = format!("http://127.0.0.1:{}/timedtext?fmt=vtt", addr.port());
    let mut reg = ExtractorRegistry::new();
    reg.register(Arc::new(MockSubtitleExtractor { timedtext_url }));
    let registry = Arc::new(reg);

    let tmp_dir = get_test_dir("sub_failure_clean_state");
    let repo = create_in_memory_database().unwrap();
    let engine = EngineManager::new(repo.clone(), registry);
    let settings = Arc::new(Mutex::new(AppSettings {
        max_concurrent_downloads: 2,
        download_directory: tmp_dir.to_string_lossy().to_string(),
        ..AppSettings::default()
    }));
    let queue = QueueManager::new(repo.clone(), engine, settings);

    // Main media item
    let media_params = StartDownloadParams {
        source_url: format!("http://127.0.0.1:{}/media.mp4", addr.port()),
        service_name: "generic".to_string(),
        title: "Media Job".to_string(),
        duration: 60,
        thumbnail_url: None,
        format_id: "fmt-media".to_string(),
        download_url: format!("http://127.0.0.1:{}/media.mp4", addr.port()),
        requires_merge: false,
        matching_audio_url: None,
        container: "mp4".to_string(),
        quality_label: "720p".to_string(),
        destination_dir: Some(tmp_dir.to_string_lossy().to_string()),
        selected_subtitles: Vec::new(),
        embed_subtitles: false,
        subtitle_mode: None,
        audio_options: None,
        video_codec: None,
        audio_codec: None,
    };

    // Subtitle item with non-existent language
    let sub_params = StartDownloadParams {
        source_url: "https://www.youtube.com/watch?v=TEST_ISO_12".to_string(),
        service_name: "youtube".to_string(),
        title: "Media Job.nonexistent".to_string(),
        duration: 60,
        thumbnail_url: None,
        format_id: "sub:zz_fake:srt".to_string(),
        download_url: String::new(),
        requires_merge: false,
        matching_audio_url: None,
        container: "srt".to_string(),
        quality_label: "NonExistentLang".to_string(),
        destination_dir: Some(tmp_dir.to_string_lossy().to_string()),
        selected_subtitles: Vec::new(),
        embed_subtitles: false,
        subtitle_mode: None,
        audio_options: None,
        video_codec: None,
        audio_codec: None,
    };

    let media_item = queue.add(media_params).await.unwrap();
    let sub_item = queue.add(sub_params).await.unwrap();

    // Give background tasks time to process
    tokio::time::sleep(Duration::from_millis(600)).await;

    // Subtitle failure must be isolated
    let sub_after = repo.get_queue_item(&sub_item.id).unwrap().unwrap();
    assert_eq!(
        sub_after.status,
        QueueStatus::Failed,
        "Failed subtitle resolution must be cleanly marked Failed"
    );

    // Media item is unaffected
    let media_after = repo.get_queue_item(&media_item.id).unwrap().unwrap();
    assert!(
        media_after.status != QueueStatus::Failed,
        "Media item must remain unaffected by isolated subtitle failure, got {:?}",
        media_after.status
    );

    let _ = shutdown.send(());
    let _ = std::fs::remove_dir_all(&tmp_dir);
}
