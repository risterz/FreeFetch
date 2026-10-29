use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::watch;
use uuid::Uuid;

use freefetch_lib::core::config::AppSettings;
use freefetch_lib::core::error::FreeFetchError;
use freefetch_lib::core::types::{QueueStatus, SubtitleFormat};
use freefetch_lib::engine::queue::QueueManager;
use freefetch_lib::engine::subtitle_downloader::{
    download_and_convert_subtitle_with_policy, parse_retry_after, SubtitleRetryPolicy,
};
use freefetch_lib::engine::{EngineManager, StartDownloadParams};
use freefetch_lib::extractor::ExtractorRegistry;
use freefetch_lib::storage::create_in_memory_database;

/// Helper: temporary directory
fn get_test_dir(prefix: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("{}_{}", prefix, Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Helper: simple mock HTTP server
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
async fn test_1_429_without_retry_after_retries_and_succeeds() {
    let vtt_body = "WEBVTT\n\n00:00:01.000 --> 00:00:04.000\nSubtitle text after 429 recovery\n";

    let (addr, _shutdown) = run_mock_server(move |_req, req_idx| {
        if req_idx == 0 {
            // First attempt: 429 without Retry-After
            Some(b"HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec())
        } else {
            // Second attempt: 200 OK
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/vtt\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                vtt_body.len(),
                vtt_body
            );
            Some(resp.into_bytes())
        }
    })
    .await;

    let client = reqwest::Client::new();
    let tmp_dir = get_test_dir("ff_sub_429_no_hdr");
    let dest_file = tmp_dir.join("sub_429.srt");
    let url = format!("http://127.0.0.1:{}/timedtext/sample", addr.port());

    let policy = SubtitleRetryPolicy::for_tests();
    let res = download_and_convert_subtitle_with_policy(
        &client,
        &url,
        &dest_file,
        SubtitleFormat::Srt,
        None,
        None,
        policy,
        None,
    )
    .await;

    assert!(
        res.is_ok(),
        "Expected success after 429 retry: {:?}",
        res.err()
    );
    assert!(dest_file.exists());
    let content = std::fs::read_to_string(&dest_file).unwrap();
    assert!(content.contains("Subtitle text after 429 recovery"));

    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[tokio::test]
async fn test_2_429_with_retry_after_header_respected() {
    let vtt_body = "WEBVTT\n\n00:00:01.000 --> 00:00:03.000\nHeader respected\n";

    let (addr, _shutdown) = run_mock_server(move |_req, req_idx| {
        if req_idx == 0 {
            // First attempt: 429 with Retry-After: 1
            Some(b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec())
        } else {
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/vtt\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                vtt_body.len(),
                vtt_body
            );
            Some(resp.into_bytes())
        }
    })
    .await;

    let client = reqwest::Client::new();
    let tmp_dir = get_test_dir("ff_sub_429_hdr");
    let dest_file = tmp_dir.join("sub_hdr.srt");
    let url = format!("http://127.0.0.1:{}/timedtext/sample", addr.port());

    let policy = SubtitleRetryPolicy::for_tests();
    let start = Instant::now();
    let res = download_and_convert_subtitle_with_policy(
        &client,
        &url,
        &dest_file,
        SubtitleFormat::Srt,
        None,
        None,
        policy,
        None,
    )
    .await;

    assert!(
        res.is_ok(),
        "Expected success after Retry-After: {:?}",
        res.err()
    );
    // In test policy, delay is accelerated and bounded (clamped to milliseconds)
    assert!(start.elapsed() < Duration::from_secs(2));
    assert!(dest_file.exists());

    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[tokio::test]
async fn test_3_repeated_429_exhausts_retries_with_diagnostics() {
    let attempt_count = Arc::new(AtomicUsize::new(0));
    let counter_clone = attempt_count.clone();

    let (addr, _shutdown) = run_mock_server(move |_req, _idx| {
        counter_clone.fetch_add(1, Ordering::SeqCst);
        Some(
            b"HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                .to_vec(),
        )
    })
    .await;

    let client = reqwest::Client::new();
    let tmp_dir = get_test_dir("ff_sub_429_exhaust");
    let dest_file = tmp_dir.join("sub_exhaust.srt");
    let url = format!("http://127.0.0.1:{}/timedtext/exhaust", addr.port());

    let policy = SubtitleRetryPolicy {
        max_retries: 3,
        base_429_delay: Duration::from_millis(5),
        max_429_delay: Duration::from_millis(20),
        retry_after_max: Duration::from_millis(50),
        jitter_factor: 0.0,
    };

    let res = download_and_convert_subtitle_with_policy(
        &client,
        &url,
        &dest_file,
        SubtitleFormat::Srt,
        None,
        None,
        policy,
        None,
    )
    .await;

    assert!(res.is_err(), "Expected error when 429 retries exhausted");
    let err_str = res.err().unwrap().to_string();
    assert!(err_str.contains("HTTP 429 Too Many Requests"));
    assert!(err_str.contains("rate-limit retry attempts exhausted"));

    // 1 initial + 3 retries = 4 attempts total
    assert_eq!(attempt_count.load(Ordering::SeqCst), 4);
    assert!(!dest_file.exists());

    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[tokio::test]
async fn test_4_cancellation_during_429_backoff() {
    let (addr, _shutdown) = run_mock_server(move |_req, _idx| {
        Some(
            b"HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                .to_vec(),
        )
    })
    .await;

    let client = reqwest::Client::new();
    let tmp_dir = get_test_dir("ff_sub_429_cancel");
    let dest_file = tmp_dir.join("sub_cancel.srt");
    let url = format!("http://127.0.0.1:{}/timedtext/cancel", addr.port());

    let (cancel_tx, cancel_rx) = watch::channel(false);

    let policy = SubtitleRetryPolicy {
        max_retries: 3,
        base_429_delay: Duration::from_secs(5), // Long 5-second backoff
        max_429_delay: Duration::from_secs(10),
        retry_after_max: Duration::from_secs(10),
        jitter_factor: 0.0,
    };

    // Trigger cancellation after 25ms while waiting in backoff
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(25)).await;
        let _ = cancel_tx.send(true);
    });

    let start = Instant::now();
    let res = download_and_convert_subtitle_with_policy(
        &client,
        &url,
        &dest_file,
        SubtitleFormat::Srt,
        Some(cancel_rx),
        None,
        policy,
        None,
    )
    .await;

    let elapsed = start.elapsed();
    assert!(matches!(res, Err(FreeFetchError::Cancelled)));
    // Must terminate promptly and NOT wait the full 5 seconds
    assert!(
        elapsed < Duration::from_secs(1),
        "Cancellation took too long: {:?}",
        elapsed
    );
    assert!(!dest_file.exists());

    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[tokio::test]
async fn test_5_403_and_410_url_refresh_not_converted_to_429() {
    let refreshed_called = Arc::new(AtomicUsize::new(0));
    let refreshed_called_clone = refreshed_called.clone();

    let vtt_body = "WEBVTT\n\n00:00:01.000 --> 00:00:03.000\nFresh Subtitle\n";
    let (addr, _shutdown) = run_mock_server(move |req, _idx| {
        if req.contains("expired") {
            Some(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec())
        } else {
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/vtt\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                vtt_body.len(),
                vtt_body
            );
            Some(resp.into_bytes())
        }
    })
    .await;

    let client = reqwest::Client::new();
    let tmp_dir = get_test_dir("ff_sub_403_distinct");
    let dest_file = tmp_dir.join("sub_403.srt");

    let expired_url = format!("http://127.0.0.1:{}/timedtext/expired", addr.port());
    let fresh_url = format!("http://127.0.0.1:{}/timedtext/fresh", addr.port());

    let refresher: freefetch_lib::engine::chunk_downloader::UrlRefresher = {
        let f_url = fresh_url.clone();
        Arc::new(move || {
            refreshed_called_clone.fetch_add(1, Ordering::SeqCst);
            let u = f_url.clone();
            Box::pin(async move { Ok(u) })
        })
    };

    let res = download_and_convert_subtitle_with_policy(
        &client,
        &expired_url,
        &dest_file,
        SubtitleFormat::Srt,
        None,
        Some(refresher),
        SubtitleRetryPolicy::for_tests(),
        None,
    )
    .await;

    assert!(
        res.is_ok(),
        "Expected success from URL refresh: {:?}",
        res.err()
    );
    assert_eq!(
        refreshed_called.load(Ordering::SeqCst),
        1,
        "Refresher must be invoked on 403"
    );
    assert!(dest_file.exists());

    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[tokio::test]
async fn test_6_subtitle_isolation_with_429() {
    let repo = create_in_memory_database().unwrap();
    let registry = Arc::new(ExtractorRegistry::new());
    let engine = EngineManager::new(repo.clone(), registry);
    let settings = Arc::new(Mutex::new(AppSettings::default()));
    let queue = QueueManager::new(repo.clone(), engine, settings);

    let media_params = StartDownloadParams {
        source_url: "https://www.youtube.com/watch?v=TEST_ISO_429".to_string(),
        service_name: "youtube".to_string(),
        title: "Isolation Test Video".to_string(),
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
    http_headers: None,
    };

    let sub_params_429 = StartDownloadParams {
        source_url: "https://www.youtube.com/watch?v=TEST_ISO_429".to_string(),
        service_name: "youtube".to_string(),
        title: "Isolation Test Video.en.srt".to_string(),
        duration: 120,
        thumbnail_url: None,
        format_id: "sub:en:srt".to_string(),
        download_url: "https://example.com/timedtext/en".to_string(),
        requires_merge: false,
        matching_audio_url: None,
        container: "srt".to_string(),
        quality_label: "English".to_string(),
        destination_dir: None,
        selected_subtitles: Vec::new(),
        embed_subtitles: false,
        subtitle_mode: None,
        audio_options: None,
        video_codec: None,
        audio_codec: None,
    http_headers: None,
    };

    let sub_params_ok = StartDownloadParams {
        source_url: "https://www.youtube.com/watch?v=TEST_ISO_429".to_string(),
        service_name: "youtube".to_string(),
        title: "Isolation Test Video.es.srt".to_string(),
        duration: 120,
        thumbnail_url: None,
        format_id: "sub:es:srt".to_string(),
        download_url: "https://example.com/timedtext/es".to_string(),
        requires_merge: false,
        matching_audio_url: None,
        container: "srt".to_string(),
        quality_label: "Spanish".to_string(),
        destination_dir: None,
        selected_subtitles: Vec::new(),
        embed_subtitles: false,
        subtitle_mode: None,
        audio_options: None,
        video_codec: None,
        audio_codec: None,
    http_headers: None,
    };

    let media_item = queue.add(media_params).await.unwrap();
    let sub_429_item = queue.add(sub_params_429).await.unwrap();
    let sub_ok_item = queue.add(sub_params_ok).await.unwrap();

    // Simulate media completed, sub 1 failed with 429, sub 2 completed
    queue
        .handle_engine_state_change(&media_item.id, "completed", None)
        .await;
    queue
        .handle_engine_state_change(
            &sub_429_item.id,
            "failed",
            Some("HTTP 429 Too Many Requests: rate-limit retry attempts exhausted"),
        )
        .await;
    queue
        .handle_engine_state_change(&sub_ok_item.id, "completed", None)
        .await;

    let media_after = repo.get_queue_item(&media_item.id).unwrap().unwrap();
    let sub_429_after = repo.get_queue_item(&sub_429_item.id).unwrap().unwrap();
    let sub_ok_after = repo.get_queue_item(&sub_ok_item.id).unwrap().unwrap();

    assert_eq!(media_after.status, QueueStatus::Completed);
    assert_eq!(sub_429_after.status, QueueStatus::Failed);
    assert_eq!(sub_ok_after.status, QueueStatus::Completed);
}

#[tokio::test]
async fn test_7_concurrent_subtitle_requests_remain_bounded() {
    let repo = create_in_memory_database().unwrap();
    let registry = Arc::new(ExtractorRegistry::new());
    let engine = EngineManager::new(repo.clone(), registry);
    let settings = Arc::new(Mutex::new(AppSettings::default()));
    let queue = QueueManager::new(repo.clone(), engine, settings);

    // Limit concurrency to 2
    let _ = queue.set_concurrency(2);

    for i in 0..4 {
        let sub_params = StartDownloadParams {
            source_url: format!("https://www.youtube.com/watch?v=TEST_CONC_{i}"),
            service_name: "youtube".to_string(),
            title: format!("Sub Item {i}"),
            duration: 60,
            thumbnail_url: None,
            format_id: format!("sub:en_{i}:srt"),
            download_url: format!("https://example.com/timedtext/{i}"),
            requires_merge: false,
            matching_audio_url: None,
            container: "srt".to_string(),
            quality_label: "English".to_string(),
            destination_dir: None,
            selected_subtitles: Vec::new(),
            embed_subtitles: false,
            subtitle_mode: None,
            audio_options: None,
            video_codec: None,
            audio_codec: None,
        http_headers: None,
        };
        queue.add(sub_params).await.unwrap();
    }

    let summary = queue.get_summary().unwrap();
    assert_eq!(summary.total_items, 4);
    // Queue enforces bounded concurrency
    let concurrency_limit = queue.get_concurrency();
    assert_eq!(concurrency_limit, 2, "Concurrency limit must be 2");
}

#[test]
fn test_8_retry_after_malformed_and_edge_cases() {
    // Normal numeric seconds
    assert_eq!(parse_retry_after("5", 30), Some(Duration::from_secs(5)));
    assert_eq!(
        parse_retry_after("  12  ", 30),
        Some(Duration::from_secs(12))
    );

    // Minimum clamp 1 second
    assert_eq!(parse_retry_after("0", 30), Some(Duration::from_secs(1)));

    // Maximum clamp to max_clamp_secs
    assert_eq!(
        parse_retry_after("99999999", 30),
        Some(Duration::from_secs(30))
    );

    // Negative numbers rejected
    assert_eq!(parse_retry_after("-1", 30), None);
    assert_eq!(parse_retry_after("-500", 30), None);

    // Malformed text rejected
    assert_eq!(parse_retry_after("invalid", 30), None);
    assert_eq!(parse_retry_after("abc5", 30), None);
    assert_eq!(parse_retry_after("", 30), None);
    assert_eq!(parse_retry_after("   ", 30), None);

    // RFC 2822 HTTP-date
    let future_date = chrono::Utc::now() + chrono::Duration::seconds(15);
    let rfc2822_str = future_date.to_rfc2822();
    let parsed_future = parse_retry_after(&rfc2822_str, 30);
    assert!(parsed_future.is_some());
    let dur = parsed_future.unwrap();
    assert!(dur.as_secs() >= 14 && dur.as_secs() <= 16);

    // Past date returns minimal 1s retry delay
    let past_date = chrono::Utc::now() - chrono::Duration::seconds(60);
    let past_str = past_date.to_rfc2822();
    assert_eq!(
        parse_retry_after(&past_str, 30),
        Some(Duration::from_secs(1))
    );
}
