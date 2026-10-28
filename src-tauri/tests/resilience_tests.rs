use reqwest::Client;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::watch;
use uuid::Uuid;

use freefetch_lib::core::types::{DownloadRecord, DownloadStatus, StreamRecord};
use freefetch_lib::engine::chunk_downloader::{
    download_stream_resilient, parse_content_range, sanitize_url_for_logging, UrlRefresher,
};
use freefetch_lib::engine::rate_limiter::RateLimiter;
use freefetch_lib::engine::EngineManager;
use freefetch_lib::extractor::ExtractorRegistry;
use freefetch_lib::storage::{initialize_database, migrations::run_migrations};

/// Simple mock HTTP server that processes requests with a custom handler
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
                            } else {
                                // Simulate connection reset by dropping socket immediately
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

#[tokio::test]
async fn test_regression_transient_timeout_retry_success() {
    // Attempt 0: sleep 1.5s (triggers client timeout of 500ms)
    // Attempt 1: return 200 OK with 10 bytes
    let (addr, _shutdown) = run_mock_server(|_req, idx| {
        if idx == 0 {
            std::thread::sleep(Duration::from_millis(800));
            None
        } else {
            let body = b"0123456789";
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let mut v = resp.into_bytes();
            v.extend_from_slice(body);
            Some(v)
        }
    })
    .await;

    let client = Client::builder()
        .timeout(Duration::from_millis(400))
        .build()
        .unwrap();

    let temp_dir = std::env::temp_dir().join(format!("ff_test_timeout_{}", Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let part_path = temp_dir.join("test.part");

    let (_tx, rx) = watch::channel(false);
    let rate_limiter = RateLimiter::default();
    let url = format!("http://{addr}/test_timeout");

    let res = download_stream_resilient(
        &client,
        &url,
        &part_path,
        rx,
        rate_limiter,
        None,
        |_| {},
        |_, _| {},
    )
    .await;

    assert!(res.is_ok(), "Should succeed after retrying on timeout");
    assert_eq!(res.unwrap(), 10);
    assert_eq!(std::fs::read(&part_path).unwrap(), b"0123456789");

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_regression_connection_reset_retry_success() {
    // Attempt 0: abrupt disconnect
    // Attempt 1: 200 OK
    let (addr, _shutdown) = run_mock_server(|_req, idx| {
        if idx == 0 {
            None // Connection reset
        } else {
            let body = b"abcdefghij";
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let mut v = resp.into_bytes();
            v.extend_from_slice(body);
            Some(v)
        }
    })
    .await;

    let client = Client::new();
    let temp_dir = std::env::temp_dir().join(format!("ff_test_reset_{}", Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let part_path = temp_dir.join("test.part");

    let (_tx, rx) = watch::channel(false);
    let rate_limiter = RateLimiter::default();
    let url = format!("http://{addr}/test_reset");

    let res = download_stream_resilient(
        &client,
        &url,
        &part_path,
        rx,
        rate_limiter,
        None,
        |_| {},
        |_, _| {},
    )
    .await;

    assert!(res.is_ok(), "Should succeed after connection reset");
    assert_eq!(res.unwrap(), 10);
    assert_eq!(std::fs::read(&part_path).unwrap(), b"abcdefghij");

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_regression_http_503_retry_success() {
    let (addr, _shutdown) = run_mock_server(|_req, idx| {
        if idx == 0 {
            Some(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec())
        } else {
            let body = b"recovered503";
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let mut v = resp.into_bytes();
            v.extend_from_slice(body);
            Some(v)
        }
    }).await;

    let client = Client::new();
    let temp_dir = std::env::temp_dir().join(format!("ff_test_503_{}", Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let part_path = temp_dir.join("test.part");

    let (_tx, rx) = watch::channel(false);
    let rate_limiter = RateLimiter::default();
    let url = format!("http://{addr}/test_503");

    let res = download_stream_resilient(
        &client,
        &url,
        &part_path,
        rx,
        rate_limiter,
        None,
        |_| {},
        |_, _| {},
    )
    .await;

    assert!(res.is_ok());
    assert_eq!(std::fs::read(&part_path).unwrap(), b"recovered503");
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_regression_http_429_retry_success() {
    let (addr, _shutdown) = run_mock_server(|_req, idx| {
        if idx == 0 {
            Some(
                b"HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    .to_vec(),
            )
        } else {
            let body = b"recovered429";
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let mut v = resp.into_bytes();
            v.extend_from_slice(body);
            Some(v)
        }
    })
    .await;

    let client = Client::new();
    let temp_dir = std::env::temp_dir().join(format!("ff_test_429_{}", Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let part_path = temp_dir.join("test.part");

    let (_tx, rx) = watch::channel(false);
    let rate_limiter = RateLimiter::default();
    let url = format!("http://{addr}/test_429");

    let res = download_stream_resilient(
        &client,
        &url,
        &part_path,
        rx,
        rate_limiter,
        None,
        |_| {},
        |_, _| {},
    )
    .await;

    assert!(res.is_ok());
    assert_eq!(std::fs::read(&part_path).unwrap(), b"recovered429");
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_regression_http_403_youtube_refresh_resume() {
    let (addr, _shutdown) = run_mock_server(|req, _idx| {
        if req.contains("stream_expired") {
            Some(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec())
        } else if req.contains("stream_fresh") {
            // Must support range request from offset 5
            if req.contains("bytes=5-") {
                let body = b"56789";
                let resp = "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 5-9/10\r\nContent-Length: 5\r\nConnection: close\r\n\r\n".to_string();
                let mut v = resp.into_bytes();
                v.extend_from_slice(body);
                Some(v)
            } else {
                let body = b"0123456789";
                let resp = "HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\n".to_string();
                let mut v = resp.into_bytes();
                v.extend_from_slice(body);
                Some(v)
            }
        } else {
            None
        }
    }).await;

    let client = Client::new();
    let temp_dir = std::env::temp_dir().join(format!("ff_test_refresh_{}", Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let part_path = temp_dir.join("test.part");

    // Pre-populate 5 bytes in .part
    std::fs::write(&part_path, b"01234").unwrap();

    let fresh_url = format!("http://{addr}/googlevideo.com/stream_fresh");
    let refreshed_url_clone = fresh_url.clone();
    let refresher: UrlRefresher = Arc::new(move || {
        let u = refreshed_url_clone.clone();
        Box::pin(async move { Ok(u) })
    });

    let expired_url = format!("http://{addr}/googlevideo.com/stream_expired");
    let (_tx, rx) = watch::channel(false);
    let rate_limiter = RateLimiter::default();

    let res = download_stream_resilient(
        &client,
        &expired_url,
        &part_path,
        rx,
        rate_limiter,
        Some(refresher),
        |_| {},
        |_, _| {},
    )
    .await;

    assert!(
        res.is_ok(),
        "Should refresh URL and resume download: {:?}",
        res.err()
    );
    assert_eq!(std::fs::read(&part_path).unwrap(), b"0123456789");
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_regression_http_416_handling() {
    // Test 6A: File is already complete (10 bytes), server returns 416 with */10
    let (addr, _shutdown) = run_mock_server(|_req, _idx| {
        Some(b"HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */10\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec())
    }).await;

    let client = Client::new();
    let temp_dir = std::env::temp_dir().join(format!("ff_test_416_{}", Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let part_path = temp_dir.join("test.part");
    std::fs::write(&part_path, b"0123456789").unwrap();

    let (_tx, rx) = watch::channel(false);
    let rate_limiter = RateLimiter::default();
    let url = format!("http://{addr}/test_416");

    let res = download_stream_resilient(
        &client,
        &url,
        &part_path,
        rx,
        rate_limiter,
        None,
        |_| {},
        |_, _| {},
    )
    .await;

    assert!(
        res.is_ok(),
        "Should recognize file is already complete on 416"
    );
    assert_eq!(res.unwrap(), 10);
    assert_eq!(std::fs::read(&part_path).unwrap(), b"0123456789");
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_regression_range_mismatch_protection() {
    // Client requests bytes=5-, server returns bytes 0-9/10 (offset mismatch)
    let (addr, _shutdown) = run_mock_server(|_req, _idx| {
        let body = b"0123456789";
        let resp = "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-9/10\r\nContent-Length: 10\r\nConnection: close\r\n\r\n";
        let mut v = resp.as_bytes().to_vec();
        v.extend_from_slice(body);
        Some(v)
    }).await;

    let client = Client::new();
    let temp_dir = std::env::temp_dir().join(format!("ff_test_mismatch_{}", Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let part_path = temp_dir.join("test.part");
    std::fs::write(&part_path, b"01234").unwrap(); // local offset 5

    let (_tx, rx) = watch::channel(false);
    let rate_limiter = RateLimiter::default();
    let url = format!("http://{addr}/test_mismatch");

    let res = download_stream_resilient(
        &client,
        &url,
        &part_path,
        rx,
        rate_limiter,
        None,
        |_| {},
        |_, _| {},
    )
    .await;

    assert!(
        res.is_err(),
        "Must reject server offset mismatch to avoid file corruption"
    );
    let err_msg = res.unwrap_err().to_string();
    assert!(err_msg.contains("RANGE_MISMATCH") || err_msg.contains("expected 5"));
    assert_eq!(
        std::fs::read(&part_path).unwrap(),
        b"01234",
        "Existing bytes must not be corrupted"
    );
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_regression_partial_part_file_recovery() {
    // 6 bytes on disk. Server sends remaining 4 bytes for bytes=6-
    let (addr, _shutdown) = run_mock_server(|req, _idx| {
        if req.contains("bytes=6-") {
            let body = b"6789";
            let resp = "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 6-9/10\r\nContent-Length: 4\r\nConnection: close\r\n\r\n";
            let mut v = resp.as_bytes().to_vec();
            v.extend_from_slice(body);
            Some(v)
        } else {
            None
        }
    }).await;

    let client = Client::new();
    let temp_dir = std::env::temp_dir().join(format!("ff_test_part_rec_{}", Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let part_path = temp_dir.join("test.part");
    std::fs::write(&part_path, b"012345").unwrap(); // 6 bytes

    let (_tx, rx) = watch::channel(false);
    let rate_limiter = RateLimiter::default();
    let url = format!("http://{addr}/test_part");

    let res = download_stream_resilient(
        &client,
        &url,
        &part_path,
        rx,
        rate_limiter,
        None,
        |_| {},
        |_, _| {},
    )
    .await;

    assert!(res.is_ok());
    assert_eq!(std::fs::read(&part_path).unwrap(), b"0123456789");
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_regression_stream_independence_and_persistence() {
    let temp_dir = std::env::temp_dir().join(format!("ff_test_db_{}", Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let db_path = temp_dir.join("history.db");
    let repo = initialize_database(&db_path).unwrap();

    let job_id = "test-job-independence";

    let rec = DownloadRecord {
        id: job_id.to_string(),
        source_url: "https://example.com/test".to_string(),
        service_name: "youtube".to_string(),
        title: "Test Video".to_string(),
        duration: 120,
        thumbnail_url: None,
        destination_path: temp_dir.join("test.mp4").to_string_lossy().to_string(),
        container: "mp4".to_string(),
        quality_label: "720p".to_string(),
        total_bytes: 20,
        downloaded_bytes: 0,
        status: DownloadStatus::Downloading,
        error_message: None,
        created_at: 1000,
        completed_at: None,
    };
    repo.insert_download(&rec).unwrap();

    let video_part = temp_dir.join("v.part");
    let audio_part = temp_dir.join("a.part");

    std::fs::write(&video_part, b"videobytes").unwrap();
    std::fs::write(&audio_part, b"audiobytes").unwrap();

    // Mark video as finished
    repo.upsert_stream(&StreamRecord {
        id: format!("{job_id}-video"),
        download_id: job_id.to_string(),
        stream_type: "video".to_string(),
        format_id: "136".to_string(),
        codec: Some("avc1".to_string()),
        resolution: Some("720p".to_string()),
        bitrate: Some(1000000),
        temp_file_path: video_part.to_string_lossy().to_string(),
        total_bytes: 10,
        downloaded_bytes: 10,
        is_finished: true,
    })
    .unwrap();

    let v_stream = repo.get_stream(job_id, "video").unwrap().unwrap();
    assert!(v_stream.is_finished);
    assert_eq!(v_stream.downloaded_bytes, 10);

    // Audio is not finished yet
    let a_stream = repo.get_stream(job_id, "audio").unwrap();
    assert!(a_stream.is_none());

    // Now finish audio
    repo.upsert_stream(&StreamRecord {
        id: format!("{job_id}-audio"),
        download_id: job_id.to_string(),
        stream_type: "audio".to_string(),
        format_id: "140".to_string(),
        codec: Some("mp4a".to_string()),
        resolution: None,
        bitrate: Some(128000),
        temp_file_path: audio_part.to_string_lossy().to_string(),
        total_bytes: 10,
        downloaded_bytes: 10,
        is_finished: true,
    })
    .unwrap();

    let a_stream = repo.get_stream(job_id, "audio").unwrap().unwrap();
    assert!(a_stream.is_finished);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_regression_crash_recovery_on_restart() {
    let temp_dir = std::env::temp_dir().join(format!("ff_test_crash_{}", Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let db_path = temp_dir.join("history.db");
    let repo = initialize_database(&db_path).unwrap();

    let record = DownloadRecord {
        id: "crashed-job".to_string(),
        source_url: "https://example.com/test".to_string(),
        service_name: "generic".to_string(),
        title: "Test Video".to_string(),
        duration: 60,
        thumbnail_url: None,
        destination_path: temp_dir.join("test.mp4").to_string_lossy().to_string(),
        container: "mp4".to_string(),
        quality_label: "720p".to_string(),
        total_bytes: 1000,
        downloaded_bytes: 500,
        status: DownloadStatus::Downloading,
        error_message: None,
        created_at: 1000,
        completed_at: None,
    };
    repo.insert_download(&record).unwrap();

    // Simulate crash and restart: run migrations again
    let mut conn = rusqlite::Connection::open(&db_path).unwrap();
    run_migrations(&mut conn).unwrap();

    let recovered = repo.get_download("crashed-job").unwrap().unwrap();
    assert_eq!(
        recovered.status,
        DownloadStatus::Paused,
        "Unfinished downloads should safely reset to paused on restart"
    );
    assert_eq!(recovered.downloaded_bytes, 500);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_regression_webview2_callback_non_tokio_thread_safety() {
    // Spawns a native OS thread with NO Tokio runtime attached
    let handle = std::thread::spawn(|| {
        // Assert no Tokio reactor is active
        assert!(
            tokio::runtime::Handle::try_current().is_err(),
            "Must be running outside Tokio"
        );

        let temp_dir = std::env::temp_dir().join(format!("ff_test_webview_{}", Uuid::new_v4()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let db_path = temp_dir.join("history.db");
        let repo = initialize_database(&db_path).unwrap();
        let registry = Arc::new(ExtractorRegistry::new());
        let engine = EngineManager::new(repo, registry);

        // spawn_task should NOT panic even when invoked outside Tokio
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        engine.spawn_task(async move {
            let _ = done_tx.send(true);
        });

        // Verify task executed safely on background runtime
        let res = done_rx.recv_timeout(Duration::from_secs(3));
        assert!(
            res.is_ok(),
            "EngineManager::spawn_task must safely execute from non-Tokio WebView2 threads"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    });

    handle.join().unwrap();
}

#[test]
fn test_safe_url_sanitizer() {
    let signed_url = "https://rr3---sn-4g5edn6s.googlevideo.com/videoplayback?expire=1700000000&ei=AbCdEf&ip=1.2.3.4&id=o-AAABBB&itag=136&source=youtube&requiressl=yes&ratebypass=yes&signature=1234567890ABCDEF";
    let (safe, host) = sanitize_url_for_logging(signed_url);
    assert_eq!(host, "rr3---sn-4g5edn6s.googlevideo.com");
    assert!(!safe.contains("expire="));
    assert!(!safe.contains("signature="));
    assert!(!safe.contains("1.2.3.4"));
    assert!(safe.contains("[params redacted]"));
}

#[test]
fn test_content_range_parser() {
    let cr1 = parse_content_range("bytes 100-199/1000").unwrap();
    assert_eq!(cr1.start, Some(100));
    assert_eq!(cr1.end, Some(199));
    assert_eq!(cr1.total, Some(1000));

    let cr2 = parse_content_range("bytes */500").unwrap();
    assert_eq!(cr2.start, None);
    assert_eq!(cr2.total, Some(500));

    let cr3 = parse_content_range("invalid range header");
    assert!(cr3.is_none());
}
