use std::io::Write;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::watch;
use url::Url;

use freefetch_lib::core::types::{DownloadRecord, DownloadStatus};
use freefetch_lib::engine::chunk_downloader::download_stream_to_file;
use freefetch_lib::engine::parallel_downloader::download_stream_parallel;
use freefetch_lib::engine::rate_limiter::RateLimiter;
use freefetch_lib::extractor::implementations::generic::GenericExtractor;
use freefetch_lib::extractor::implementations::youtube::{extract_youtube_id, YoutubeExtractor};
use freefetch_lib::extractor::{ExtractionContext, Extractor};
use freefetch_lib::media::ffmpeg::FfmpegController;
use freefetch_lib::storage::initialize_database;

/// Sets up a local test HTTP server supporting HTTP Range requests
async fn spawn_range_test_server(
    payload: Vec<u8>,
) -> (SocketAddr, tokio::sync::oneshot::Sender<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    tokio::spawn(async move {
        let payload = Arc::new(payload);
        loop {
            tokio::select! {
                _ = &mut shutdown_rx => break,
                accept_res = listener.accept() => {
                    if let Ok((mut socket, _)) = accept_res {
                        let payload_clone = payload.clone();
                        tokio::spawn(async move {
                            let mut buf = [0u8; 2048];
                            if let Ok(n) = socket.read(&mut buf).await {
                                let req = String::from_utf8_lossy(&buf[..n]);
                                let total_len = payload_clone.len();

                                // Check for Range header: "Range: bytes=start-"
                                if let Some(range_pos) = req.find("Range: bytes=") {
                                    let rest = &req[range_pos + 13..];
                                    let end_of_range = rest.find("\r\n").unwrap_or(rest.len());
                                    let range_val = &rest[..end_of_range];

                                    let start_offset: usize = if let Some(dash) = range_val.find('-') {
                                        range_val[..dash].parse().unwrap_or(0)
                                    } else {
                                        0
                                    };

                                    if start_offset < total_len {
                                        let slice = &payload_clone[start_offset..];
                                        let resp = format!(
                                            "HTTP/1.1 206 Partial Content\r\n\
                                             Content-Range: bytes {}-{}/{}\r\n\
                                             Content-Length: {}\r\n\
                                             Content-Type: video/mp4\r\n\
                                             Connection: close\r\n\r\n",
                                            start_offset,
                                            total_len - 1,
                                            total_len,
                                            slice.len()
                                        );
                                        let _ = socket.write_all(resp.as_bytes()).await;
                                        let _ = socket.write_all(slice).await;
                                    }
                                } else {
                                    // Full content response
                                    let resp = format!(
                                        "HTTP/1.1 200 OK\r\n\
                                         Accept-Ranges: bytes\r\n\
                                         Content-Length: {}\r\n\
                                         Content-Type: video/mp4\r\n\
                                         Connection: close\r\n\r\n",
                                        total_len
                                    );
                                    let _ = socket.write_all(resp.as_bytes()).await;
                                    let _ = socket.write_all(&payload_clone).await;
                                }
                            }
                        });
                    }
                }
            }
        }
    });

    (addr, shutdown_tx)
}

#[tokio::test]
async fn test_e2e_sqlite_lifecycle_and_crash_recovery() {
    let temp_dir = std::env::temp_dir().join(format!("ff_db_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let db_path = temp_dir.join("test.db");

    // 1. Initial creation and migrations
    let repo = initialize_database(&db_path).expect("Initial database migration failed");

    // 2. Insert test download records
    let rec1 = DownloadRecord {
        id: "job-downloading-1".to_string(),
        source_url: "https://example.com/video1.mp4".to_string(),
        service_name: "generic".to_string(),
        title: "Active Video".to_string(),
        duration: 60,
        thumbnail_url: None,
        destination_path: "C:\\tmp\\video1.mp4".to_string(),
        container: "mp4".to_string(),
        quality_label: "1080p".to_string(),
        total_bytes: 100_000,
        downloaded_bytes: 40_000,
        status: DownloadStatus::Downloading,
        error_message: None,
        created_at: 1700000000,
        completed_at: None,
    };
    repo.insert_download(&rec1).unwrap();

    let rec2 = DownloadRecord {
        id: "job-completed-2".to_string(),
        source_url: "https://example.com/video2.mp4".to_string(),
        service_name: "generic".to_string(),
        title: "Finished Video".to_string(),
        duration: 120,
        thumbnail_url: None,
        destination_path: "C:\\tmp\\video2.mp4".to_string(),
        container: "mp4".to_string(),
        quality_label: "720p".to_string(),
        total_bytes: 50_000,
        downloaded_bytes: 50_000,
        status: DownloadStatus::Completed,
        error_message: None,
        created_at: 1700000000,
        completed_at: Some(1700000060),
    };
    repo.insert_download(&rec2).unwrap();

    // 3. Simulate Application Restart: Re-open the database and run migrations
    let repo_restarted = initialize_database(&db_path).expect("Restart database open failed");

    // 4. Verify crash recovery: In-progress job MUST transition to 'paused'
    let job1 = repo_restarted
        .get_download("job-downloading-1")
        .unwrap()
        .unwrap();
    assert_eq!(job1.status, DownloadStatus::Paused);
    assert_eq!(job1.downloaded_bytes, 40_000);

    // Completed job MUST remain 'completed'
    let job2 = repo_restarted
        .get_download("job-completed-2")
        .unwrap()
        .unwrap();
    assert_eq!(job2.status, DownloadStatus::Completed);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_e2e_http_range_download_and_resume() {
    // Generate a 128 KB test payload
    let mut payload = Vec::with_capacity(128 * 1024);
    for i in 0..(128 * 1024) {
        payload.push((i % 256) as u8);
    }

    let (server_addr, _shutdown) = spawn_range_test_server(payload.clone()).await;
    let url = format!("http://{server_addr}/test_stream.mp4");

    let temp_dir = std::env::temp_dir().join(format!("ff_stream_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let part_path = temp_dir.join("test.mp4.part");

    let client = reqwest::Client::new();
    let rate_limiter = RateLimiter::default();

    // Phase 1: Simulate interrupted partial download (write first 40 KB)
    {
        let mut f = std::fs::File::create(&part_path).unwrap();
        f.write_all(&payload[..40960]).unwrap();
        f.flush().unwrap();
    }
    assert_eq!(part_path.metadata().unwrap().len(), 40960);

    // Phase 2: Resume download with Range request
    let (cancel_tx, cancel_rx) = watch::channel(false);
    let mut progress_reports = Vec::new();

    let total_downloaded = download_stream_to_file(
        &client,
        &url,
        &part_path,
        cancel_rx,
        rate_limiter,
        move |p| {
            progress_reports.push(p.downloaded_bytes);
        },
    )
    .await
    .expect("Resume download should succeed");

    assert_eq!(total_downloaded, payload.len() as u64);
    assert_eq!(part_path.metadata().unwrap().len(), payload.len() as u64);

    // Verify byte content integrity
    let downloaded_bytes = std::fs::read(&part_path).unwrap();
    assert_eq!(downloaded_bytes, payload);

    let _ = cancel_tx.send(true);
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_e2e_ffmpeg_stream_merging_and_transcoding() {
    let controller =
        FfmpegController::discover(None).expect("FFmpeg must be installed for e2e test");
    assert!(
        controller.binary_path().exists() || controller.binary_path().to_str() == Some("ffmpeg")
    );

    let temp_dir = std::env::temp_dir().join(format!("ff_ffmpeg_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();

    let video_input = temp_dir.join("input_v.mp4");
    let audio_input = temp_dir.join("input_a.m4a");
    let merged_output = temp_dir.join("merged.mp4");
    let mp3_output = temp_dir.join("extracted.mp3");

    // 1. Generate 1-second sample video using ffmpeg testsrc
    let status_v = std::process::Command::new(controller.binary_path())
        .arg("-y")
        .arg("-f")
        .arg("lavfi")
        .arg("-i")
        .arg("testsrc=duration=1:size=320x240:rate=30")
        .arg("-c:v")
        .arg("libx264")
        .arg(&video_input)
        .output()
        .expect("Failed to execute FFmpeg to create test video");
    assert!(status_v.status.success());

    // 2. Generate 1-second sample audio using ffmpeg sine
    let status_a = std::process::Command::new(controller.binary_path())
        .arg("-y")
        .arg("-f")
        .arg("lavfi")
        .arg("-i")
        .arg("sine=frequency=1000:duration=1")
        .arg("-c:a")
        .arg("aac")
        .arg(&audio_input)
        .output()
        .expect("Failed to execute FFmpeg to create test audio");
    assert!(status_a.status.success());

    // 3. Test merge_streams
    controller
        .merge_streams(&video_input, &audio_input, &merged_output)
        .await
        .expect("Stream merging failed");
    assert!(merged_output.exists());
    assert!(merged_output.metadata().unwrap().len() > 1000);

    // 4. Test extract_to_mp3
    controller
        .extract_to_mp3(&audio_input, &mp3_output)
        .await
        .expect("Audio extraction to MP3 failed");
    assert!(mp3_output.exists());
    assert!(mp3_output.metadata().unwrap().len() > 500);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_e2e_generic_extractor_url_and_opengraph() {
    let extractor = GenericExtractor;
    let ctx = ExtractionContext::default();

    // 1. Direct media URL path
    let direct_url = Url::parse(
        "https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/BigBuckBunny.mp4",
    )
    .unwrap();
    assert!(extractor.can_handle(&direct_url));

    let (meta, manifest) = extractor
        .extract(&direct_url, &ctx)
        .await
        .expect("Extraction failed");
    assert_eq!(meta.service_name, "generic");
    assert!(!manifest.formats.is_empty());
    assert_eq!(manifest.formats[0].container, "mp4");
}

#[tokio::test]
async fn test_e2e_youtube_extractor_oembed_and_formats() {
    let extractor = YoutubeExtractor;
    let ctx = ExtractionContext::default();

    let yt_url = Url::parse("https://www.youtube.com/watch?v=dQw4w9WgXcQ").unwrap();
    assert!(extractor.can_handle(&yt_url));
    assert_eq!(extract_youtube_id(&yt_url), Some("dQw4w9WgXcQ".to_string()));

    let (meta, manifest) = extractor
        .extract(&yt_url, &ctx)
        .await
        .expect("YouTube extraction failed");
    assert_eq!(meta.service_name, "youtube");
    assert_eq!(meta.id, "dQw4w9WgXcQ");
    assert!(!meta.title.is_empty());
    assert!(!manifest.formats.is_empty());

    // Verify format quality labels and downloadable URLs
    let f1 = &manifest.formats[0];
    assert!(f1.quality_label.contains("p") || f1.quality_label.contains("Audio"));
    assert!(f1.download_url.starts_with("http"));
}

#[tokio::test]
async fn test_e2e_real_youtube_download_merge_and_persistence() {
    let extractor = YoutubeExtractor;
    let ctx = ExtractionContext::default();
    let client = reqwest::Client::new();
    let rate_limiter = RateLimiter::default();
    let ffmpeg = FfmpegController::discover(None).expect("FFmpeg required for test");

    let yt_url = Url::parse("https://www.youtube.com/watch?v=dQw4w9WgXcQ").unwrap();
    let (meta, manifest) = extractor
        .extract(&yt_url, &ctx)
        .await
        .expect("Extraction failed");

    assert!(!meta.title.is_empty(), "Title must not be empty");
    assert!(meta.duration > 0, "Duration should be positive");
    assert!(
        !manifest.formats.is_empty(),
        "Formats list must not be empty"
    );

    let temp_dir = std::env::temp_dir().join(format!("ff_yt_e2e_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let db_path = temp_dir.join("history.db");
    let repo = initialize_database(&db_path).unwrap();

    // Prefer 720p or 360p or first available video/muxed format
    let target_format = manifest
        .formats
        .iter()
        .find(|f| {
            f.format_type != "audio"
                && (f.quality_label.starts_with("720") || f.quality_label.starts_with("360"))
        })
        .unwrap_or(&manifest.formats[0]);

    println!(
        "Selected format: id={}, label={}, type={}, container={}, merge={}",
        target_format.format_id,
        target_format.quality_label,
        target_format.format_type,
        target_format.container,
        target_format.requires_merge
    );

    let final_mp4 = temp_dir.join("rick_astley.mp4");

    if target_format.requires_merge {
        // Video + Audio stream download and merge with parallel workers
        let v_part = temp_dir.join("v.part");
        let a_part = temp_dir.join("a.part");

        let start_dl = std::time::Instant::now();
        let peak_speed = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let ps = peak_speed.clone();

        let (_tx_v, rx_v) = watch::channel(false);
        let v_bytes = download_stream_parallel(
            &client,
            &target_format.download_url,
            &v_part,
            rx_v,
            rate_limiter.clone(),
            6, // 6 parallel workers for video
            None,
            move |p| {
                let cur = ps.load(std::sync::atomic::Ordering::Relaxed);
                if p.speed_bytes_per_sec > cur {
                    ps.store(p.speed_bytes_per_sec, std::sync::atomic::Ordering::Relaxed);
                }
            },
            |_, _| {},
        )
        .await
        .unwrap();

        let audio_url = target_format
            .matching_audio_url
            .as_ref()
            .expect("Matching audio URL required");
        let (_tx_a, rx_a) = watch::channel(false);
        let a_bytes = download_stream_parallel(
            &client,
            audio_url,
            &a_part,
            rx_a,
            rate_limiter,
            2, // 2 workers for audio
            None,
            |_| {},
            |_, _| {},
        )
        .await
        .unwrap();

        let elapsed = start_dl.elapsed();
        let total_bytes = v_bytes + a_bytes;
        let avg_mbps = (total_bytes as f64 / 1_048_576.0) / elapsed.as_secs_f64();
        let peak_mbps = peak_speed.load(std::sync::atomic::Ordering::Relaxed) as f64 / 1_048_576.0;

        println!(
            "YouTube Benchmark: {} MB in {:.1}s (avg: {:.2} MB/s, peak: {:.2} MB/s)",
            total_bytes / 1_048_576,
            elapsed.as_secs_f64(),
            avg_mbps,
            peak_mbps
        );

        assert!(v_part.exists() && v_part.metadata().unwrap().len() > 10_000);
        assert!(a_part.exists() && a_part.metadata().unwrap().len() > 5_000);

        ffmpeg
            .merge_streams(&v_part, &a_part, &final_mp4)
            .await
            .expect("FFmpeg remux must succeed");
    } else {
        // Muxed stream download
        let (_tx, rx) = watch::channel(false);
        let part = temp_dir.join("direct.mp4.part");
        let downloaded = download_stream_to_file(
            &client,
            &target_format.download_url,
            &part,
            rx,
            rate_limiter,
            |_| {},
        )
        .await
        .unwrap();
        assert!(
            downloaded > 50_000,
            "Downloaded bytes should be substantial"
        );
        std::fs::rename(&part, &final_mp4).unwrap();
    }

    // Verify final MP4 exists and has valid size
    assert!(final_mp4.exists());
    let file_size = final_mp4.metadata().unwrap().len();
    assert!(file_size > 100_000, "Final MP4 file must be > 100 KB");

    // Validate media file integrity with FFmpeg
    let verify_status = std::process::Command::new(ffmpeg.binary_path())
        .arg("-v")
        .arg("error")
        .arg("-i")
        .arg(&final_mp4)
        .arg("-f")
        .arg("null")
        .arg("-")
        .output()
        .expect("Failed to run FFmpeg validator");
    assert!(
        verify_status.status.success(),
        "FFmpeg validation on final MP4 failed"
    );

    // Record completed download in SQLite
    let record = DownloadRecord {
        id: "yt-e2e-completed".to_string(),
        source_url: yt_url.to_string(),
        service_name: "youtube".to_string(),
        title: meta.title,
        duration: meta.duration,
        thumbnail_url: meta.thumbnail_url,
        destination_path: final_mp4.to_string_lossy().to_string(),
        container: "mp4".to_string(),
        quality_label: target_format.quality_label.clone(),
        total_bytes: file_size,
        downloaded_bytes: file_size,
        status: DownloadStatus::Completed,
        error_message: None,
        created_at: 1700000000,
        completed_at: Some(1700000050),
    };
    repo.insert_download(&record).unwrap();

    let fetched = repo
        .get_download("yt-e2e-completed")
        .unwrap()
        .expect("Record must be in SQLite");
    assert_eq!(fetched.status, DownloadStatus::Completed);
    assert_eq!(fetched.service_name, "youtube");
    assert_eq!(fetched.total_bytes, file_size);

    let _ = std::fs::remove_dir_all(&temp_dir);
}
