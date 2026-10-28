use reqwest::Client;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::watch;
use uuid::Uuid;

use freefetch_lib::engine::parallel_downloader::{
    download_stream_parallel, PartMetadata, RangeState,
};
use freefetch_lib::engine::rate_limiter::RateLimiter;
use freefetch_lib::engine::speed_tracker::SpeedTracker;

/// Deterministic mock server for Range requests
async fn run_range_mock_server(
    data: Arc<Vec<u8>>,
    fail_range_index: Option<usize>,
) -> (
    SocketAddr,
    tokio::sync::oneshot::Sender<()>,
    Arc<AtomicUsize>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let request_count = Arc::new(AtomicUsize::new(0));

    let data_clone = data.clone();
    let count_clone = request_count.clone();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut shutdown_rx => break,
                accept_res = listener.accept() => {
                    if let Ok((mut socket, _)) = accept_res {
                        let data = data_clone.clone();
                        let count = count_clone.clone();
                        tokio::spawn(async move {
                            let mut buf = [0u8; 2048];
                            let n = match socket.read(&mut buf).await {
                                Ok(n) if n > 0 => n,
                                _ => return,
                            };
                            let req = String::from_utf8_lossy(&buf[..n]).to_string();
                            let req_idx = count.fetch_add(1, Ordering::SeqCst);

                            // Handle Range probing: bytes=0-0
                            if req.contains("bytes=0-0") {
                                let total = data.len();
                                let resp = format!(
                                    "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-0/{total}\r\nContent-Length: 1\r\nConnection: close\r\n\r\n"
                                );
                                let mut bytes = resp.into_bytes();
                                bytes.push(data[0]);
                                let _ = socket.write_all(&bytes).await;
                                return;
                            }

                            // Extract Range: bytes=START-END
                            if let Some(range_line) = req.lines().find(|l| l.to_lowercase().starts_with("range:")) {
                                if let Some(spec) = range_line.split('=').nth(1) {
                                    let parts: Vec<&str> = spec.trim().split('-').collect();
                                    if parts.len() == 2 {
                                        let start: usize = parts[0].parse().unwrap_or(0);
                                        let end: usize = parts[1].parse().unwrap_or(data.len() - 1);

                                        // Simulate transient failure on a specific range
                                        if let Some(fail_idx) = fail_range_index {
                                            if req_idx == fail_idx {
                                                // Abruptly drop connection
                                                return;
                                            }
                                        }

                                        let slice = &data[start..=end.min(data.len() - 1)];
                                        let total = data.len();
                                        let resp = format!(
                                            "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {start}-{end}/{total}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                            slice.len()
                                        );
                                        let mut bytes = resp.into_bytes();
                                        bytes.extend_from_slice(slice);
                                        let _ = socket.write_all(&bytes).await;
                                        return;
                                    }
                                }
                            }

                            // Default 200 OK
                            let resp = format!(
                                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                data.len()
                            );
                            let mut bytes = resp.into_bytes();
                            bytes.extend_from_slice(&data);
                            let _ = socket.write_all(&bytes).await;
                        });
                    }
                }
            }
        }
    });

    (addr, shutdown_tx, request_count)
}

fn generate_deterministic_payload(size: usize) -> Vec<u8> {
    let mut payload = Vec::with_capacity(size);
    for i in 0..size {
        payload.push(((i * 31 + 17) % 256) as u8);
    }
    payload
}

#[tokio::test]
async fn test_parallel_range_download_correctness() {
    let size = 8 * 1024 * 1024; // 8 MB
    let payload = Arc::new(generate_deterministic_payload(size));
    let (addr, _shutdown, req_count) = run_range_mock_server(payload.clone(), None).await;

    let client = Client::new();
    let temp_dir = std::env::temp_dir().join(format!("ff_speed_corr_{}", Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let part_path = temp_dir.join("test.mp4.part");

    let (_tx, rx) = watch::channel(false);
    let rate_limiter = RateLimiter::default();
    let url = format!("http://{addr}/video.mp4");

    let start = Instant::now();
    let downloaded = download_stream_parallel(
        &client,
        &url,
        &part_path,
        rx,
        rate_limiter,
        4, // 4 workers
        None,
        |_| {},
        |_, _| {},
    )
    .await
    .expect("Parallel download must succeed");

    let duration = start.elapsed();
    println!("8 MB parallel download completed in {:?}", duration);

    assert_eq!(downloaded, size as u64);
    assert!(part_path.exists());
    let written = std::fs::read(&part_path).unwrap();
    assert_eq!(written.len(), size);
    assert_eq!(
        written, *payload,
        "Downloaded file must match original payload byte-for-byte"
    );

    // Multiple range requests must have been issued
    assert!(
        req_count.load(Ordering::SeqCst) >= 4,
        "Must issue parallel range requests"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_parallel_range_worker_failure_and_retry() {
    let size = 6 * 1024 * 1024; // 6 MB
    let payload = Arc::new(generate_deterministic_payload(size));
    // Fail request index 2 (one worker connection reset)
    let (addr, _shutdown, _) = run_range_mock_server(payload.clone(), Some(2)).await;

    let client = Client::new();
    let temp_dir = std::env::temp_dir().join(format!("ff_speed_fail_{}", Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let part_path = temp_dir.join("test_fail.mp4.part");

    let (_tx, rx) = watch::channel(false);
    let rate_limiter = RateLimiter::default();
    let url = format!("http://{addr}/video_fail.mp4");

    let downloaded = download_stream_parallel(
        &client,
        &url,
        &part_path,
        rx,
        rate_limiter,
        4,
        None,
        |_| {},
        |_, _| {},
    )
    .await
    .expect("Must recover and succeed even if a worker fails");

    assert_eq!(downloaded, size as u64);
    let written = std::fs::read(&part_path).unwrap();
    assert_eq!(
        written, *payload,
        "File integrity must remain 100% intact after worker retry"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_parallel_range_resume_missing_ranges_only() {
    let size = 8 * 1024 * 1024; // 8 MB
    let payload = Arc::new(generate_deterministic_payload(size));
    let (addr, _shutdown, req_count) = run_range_mock_server(payload.clone(), None).await;

    let temp_dir = std::env::temp_dir().join(format!("ff_speed_res_{}", Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let part_path = temp_dir.join("test_resume.part");
    let meta_path = temp_dir.join("test_resume.part.meta");

    // Pre-create file and mark Range 0 and 1 as ALREADY complete (4 MB)
    let chunk_size = 2 * 1024 * 1024;
    let initial_ranges = vec![
        RangeState {
            index: 0,
            start_byte: 0,
            end_byte: chunk_size - 1,
            downloaded_bytes: chunk_size,
            is_finished: true,
        },
        RangeState {
            index: 1,
            start_byte: chunk_size,
            end_byte: 2 * chunk_size - 1,
            downloaded_bytes: chunk_size,
            is_finished: true,
        },
        RangeState {
            index: 2,
            start_byte: 2 * chunk_size,
            end_byte: 3 * chunk_size - 1,
            downloaded_bytes: 0,
            is_finished: false,
        },
        RangeState {
            index: 3,
            start_byte: 3 * chunk_size,
            end_byte: size as u64 - 1,
            downloaded_bytes: 0,
            is_finished: false,
        },
    ];

    let mut pre_file = payload[..2 * chunk_size as usize].to_vec();
    pre_file.resize(size, 0u8);
    std::fs::write(&part_path, pre_file).unwrap();

    let meta = PartMetadata {
        total_bytes: size as u64,
        ranges: initial_ranges,
    };
    std::fs::write(&meta_path, serde_json::to_string(&meta).unwrap()).unwrap();

    let client = Client::new();
    let (_tx, rx) = watch::channel(false);
    let rate_limiter = RateLimiter::default();
    let url = format!("http://{addr}/video_resume.mp4");

    req_count.store(0, Ordering::SeqCst);

    let downloaded = download_stream_parallel(
        &client,
        &url,
        &part_path,
        rx,
        rate_limiter,
        4,
        None,
        |_| {},
        |_, _| {},
    )
    .await
    .expect("Resume must succeed");

    assert_eq!(downloaded, size as u64);
    let written = std::fs::read(&part_path).unwrap();
    assert_eq!(written, *payload);

    // Range 0 and Range 1 should NOT have been requested again
    // Probing request (1) + Range 2 (1) + Range 3 (1) = ~3 requests
    let total_reqs = req_count.load(Ordering::SeqCst);
    assert!(
        total_reqs <= 3,
        "Only missing ranges should be requested, got {total_reqs} requests"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_speed_tracker_smooth_moving_average() {
    let mut tracker = SpeedTracker::new(Duration::from_secs(2));
    let t0 = Instant::now();
    tracker.update(0, t0);

    // 1 MB transferred after 0.5s -> 2 MB/s
    let s1 = tracker.update(1_000_000, t0 + Duration::from_millis(500));
    assert!(
        s1 > 1_500_000 && s1 < 2_500_000,
        "Expected ~2 MB/s, got {s1}"
    );

    // Another 1 MB after next 0.5s (2 MB total in 1.0s)
    let s2 = tracker.update(2_000_000, t0 + Duration::from_millis(1000));
    assert!(
        s2 > 1_800_000 && s2 < 2_200_000,
        "Expected stable ~2 MB/s, got {s2}"
    );

    // ETA for 10 MB total
    let eta = tracker.calculate_eta(2_000_000, 10_000_000);
    assert_eq!(eta, 4, "8 MB remaining at 2 MB/s should give 4s ETA");
}

#[tokio::test]
async fn test_unlimited_rate_limiter_zero_latency() {
    let limiter = RateLimiter::new(None);
    let start = Instant::now();
    for _ in 0..500 {
        limiter
            .throttle(1024 * 1024, Duration::from_millis(1))
            .await;
    }
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(30),
        "Unlimited rate limiter must have zero latency delay"
    );
}

#[tokio::test]
async fn test_concurrency_scaling_benchmark() {
    let size = 4 * 1024 * 1024; // 4 MB
    let payload = Arc::new(generate_deterministic_payload(size));
    let (addr, _shutdown, _) = run_range_mock_server(payload.clone(), None).await;

    let client = Client::new();
    let temp_dir = std::env::temp_dir().join(format!("ff_bench_{}", Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();

    // Benchmark 1 worker
    let part1 = temp_dir.join("test_1.part");
    let (_tx1, rx1) = watch::channel(false);
    let start1 = Instant::now();
    let _ = download_stream_parallel(
        &client,
        &format!("http://{addr}/b1"),
        &part1,
        rx1,
        RateLimiter::default(),
        1,
        None,
        |_| {},
        |_, _| {},
    )
    .await
    .unwrap();
    let dur1 = start1.elapsed();

    // Benchmark 4 workers
    let part4 = temp_dir.join("test_4.part");
    let (_tx4, rx4) = watch::channel(false);
    let start4 = Instant::now();
    let _ = download_stream_parallel(
        &client,
        &format!("http://{addr}/b4"),
        &part4,
        rx4,
        RateLimiter::default(),
        4,
        None,
        |_| {},
        |_, _| {},
    )
    .await
    .unwrap();
    let dur4 = start4.elapsed();

    println!(
        "Benchmark: 1 worker took {:?}, 4 workers took {:?}",
        dur1, dur4
    );
    assert_eq!(std::fs::read(&part1).unwrap(), *payload);
    assert_eq!(std::fs::read(&part4).unwrap(), *payload);

    let _ = std::fs::remove_dir_all(&temp_dir);
}
