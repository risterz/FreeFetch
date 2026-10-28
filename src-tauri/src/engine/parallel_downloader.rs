use futures_util::StreamExt;
use reqwest::header::{CONTENT_RANGE, RANGE};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{watch, Mutex};

use crate::core::error::FreeFetchError;
use crate::core::types::DownloadStatus;
use crate::engine::chunk_downloader::{
    classify_http_status, classify_reqwest_error, parse_content_range, ProgressUpdate, UrlRefresher,
};
use crate::engine::rate_limiter::RateLimiter;
use crate::engine::speed_tracker::SpeedTracker;

pub const DEFAULT_STALL_THRESHOLD_BYTES_PER_SEC: u64 = 100 * 1024; // 100 KB/s
pub const DEFAULT_STALL_DURATION: Duration = Duration::from_secs(3); // 3 seconds
pub const MAX_STALL_RECOVERIES_PER_RANGE: usize = 3;
pub const STALL_MIN_REMAINING_BYTES: u64 = 64 * 1024; // 64 KB
pub const YOUTUBE_SUBRANGE_SIZE: u64 = 2 * 1024 * 1024; // 2 MiB

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RangeState {
    pub index: usize,
    pub start_byte: u64,
    pub end_byte: u64,
    pub downloaded_bytes: u64,
    pub is_finished: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartMetadata {
    pub total_bytes: u64,
    pub ranges: Vec<RangeState>,
}

#[derive(Debug, Clone)]
pub struct RangeWorkQueue {
    pub ranges: Vec<RangeState>,
    pub claimed: Vec<bool>,
    pub stall_counts: Vec<usize>,
}

impl RangeWorkQueue {
    pub fn new(ranges: Vec<RangeState>) -> Self {
        let n = ranges.len();
        Self {
            ranges,
            claimed: vec![false; n],
            stall_counts: vec![0; n],
        }
    }

    /// Claim next unfinished and unclaimed range
    pub fn claim_next(&mut self) -> Option<(usize, RangeState)> {
        for (i, r) in self.ranges.iter().enumerate() {
            if !r.is_finished && !self.claimed[i] {
                self.claimed[i] = true;
                return Some((i, r.clone()));
            }
        }
        None
    }

    /// Mark range finished
    pub fn finish_range(&mut self, index: usize) {
        if index < self.ranges.len() {
            self.ranges[index].is_finished = true;
            self.ranges[index].downloaded_bytes =
                self.ranges[index].end_byte - self.ranges[index].start_byte + 1;
            self.claimed[index] = false;
        }
    }

    /// Update progress of an active range
    pub fn update_progress(&mut self, index: usize, downloaded: u64) {
        if index < self.ranges.len() {
            self.ranges[index].downloaded_bytes = downloaded;
        }
    }

    /// Unclaim range (e.g. if worker is requeueing or cancelled)
    pub fn unclaim(&mut self, index: usize) {
        if index < self.claimed.len() {
            self.claimed[index] = false;
        }
    }

    /// Record a stall recovery, returning new count
    pub fn record_stall(&mut self, index: usize) -> usize {
        if index < self.stall_counts.len() {
            self.stall_counts[index] += 1;
            self.stall_counts[index]
        } else {
            0
        }
    }

    /// Check if all ranges are finished
    pub fn is_all_finished(&self) -> bool {
        self.ranges.iter().all(|r| r.is_finished)
    }
}

/// Dynamic range partitioning logic: uniform 2 MiB sub-ranges for YouTube,
/// monolithic partitions for other CDNs.
pub fn partition_ranges(total_bytes: u64, is_youtube: bool, concurrency: usize) -> Vec<RangeState> {
    let mut ranges = Vec::new();
    if is_youtube {
        let subrange_size = YOUTUBE_SUBRANGE_SIZE;
        let count = total_bytes.div_ceil(subrange_size);
        for i in 0..count {
            let start = i * subrange_size;
            let end = ((i + 1) * subrange_size - 1).min(total_bytes - 1);
            if start <= end {
                ranges.push(RangeState {
                    index: i as usize,
                    start_byte: start,
                    end_byte: end,
                    downloaded_bytes: 0,
                    is_finished: false,
                });
            }
        }
    } else {
        let chunk_size = total_bytes.div_ceil(concurrency as u64);
        for i in 0..concurrency {
            let start = i as u64 * chunk_size;
            let end = ((i as u64 + 1) * chunk_size - 1).min(total_bytes - 1);
            if start <= end {
                ranges.push(RangeState {
                    index: i,
                    start_byte: start,
                    end_byte: end,
                    downloaded_bytes: 0,
                    is_finished: false,
                });
            }
        }
    }
    ranges
}

#[derive(Debug, Clone)]
pub struct StallWatchdog {
    pub threshold_bytes_per_sec: u64,
    pub duration: Duration,
    pub min_remaining_bytes: u64,
    pub max_recoveries: usize,
    stall_timer: Option<Instant>,
    window_start: Instant,
    window_bytes: u64,
}

impl StallWatchdog {
    pub fn new(threshold_bytes_per_sec: u64, duration: Duration) -> Self {
        Self {
            threshold_bytes_per_sec,
            duration,
            min_remaining_bytes: STALL_MIN_REMAINING_BYTES,
            max_recoveries: MAX_STALL_RECOVERIES_PER_RANGE,
            stall_timer: None,
            window_start: Instant::now(),
            window_bytes: 0,
        }
    }

    pub fn record_bytes(&mut self, bytes: u64) {
        self.window_bytes += bytes;
    }

    /// Evaluates if stalled based on current throughput window and remaining bytes.
    /// Returns true if stall condition met (> duration below threshold).
    pub fn evaluate(&mut self, now: Instant, remaining_bytes: u64, recovery_count: usize) -> bool {
        if remaining_bytes <= self.min_remaining_bytes || recovery_count >= self.max_recoveries {
            self.stall_timer = None;
            return false;
        }

        let elapsed = now.duration_since(self.window_start).as_secs_f64();
        if elapsed < 1.0 {
            // Need at least 1 second sampling window
            return false;
        }

        let current_rate = (self.window_bytes as f64 / elapsed) as u64;

        let is_stalled = if current_rate < self.threshold_bytes_per_sec {
            match self.stall_timer {
                Some(st) => now.duration_since(st) >= self.duration,
                None => {
                    self.stall_timer = Some(now);
                    false
                }
            }
        } else {
            // Throughput healthy; reset timer
            self.stall_timer = None;
            false
        };

        // Reset rolling window
        self.window_start = now;
        self.window_bytes = 0;

        is_stalled
    }

    pub fn reset_stall_timer(&mut self) {
        self.stall_timer = None;
    }

    pub fn is_timing(&self) -> bool {
        self.stall_timer.is_some()
    }
}

#[derive(Clone)]
pub struct PositionalFileWriter {
    file: Arc<std::fs::File>,
}

impl PositionalFileWriter {
    pub fn open(path: &Path, total_size: u64) -> Result<Self, FreeFetchError> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(path)?;

        let _ = file.set_len(total_size);

        Ok(Self {
            file: Arc::new(file),
        })
    }

    pub fn write_at(&self, offset: u64, buf: &[u8]) -> Result<(), FreeFetchError> {
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::fs::FileExt;
            let mut written = 0;
            while written < buf.len() {
                let n = self
                    .file
                    .seek_write(&buf[written..], offset + written as u64)?;
                if n == 0 {
                    return Err(FreeFetchError::Io(
                        "Zero bytes written to positional file writer".to_string(),
                    ));
                }
                written += n;
            }
            Ok(())
        }

        #[cfg(not(target_os = "windows"))]
        {
            use std::os::unix::fs::FileExt;
            self.file
                .write_all_at(buf, offset)
                .map_err(FreeFetchError::Io)
        }
    }

    pub fn sync_all(&self) -> Result<(), FreeFetchError> {
        self.file.sync_all()?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct StreamProbeInfo {
    pub total_bytes: Option<u64>,
    pub supports_range: bool,
}

pub async fn probe_stream(client: &Client, url: &str) -> Result<StreamProbeInfo, FreeFetchError> {
    let resp = client
        .get(url)
        .header("User-Agent", "Mozilla/5.0 FreeFetch/0.1")
        .header(RANGE, "bytes=0-0")
        .send()
        .await
        .map_err(|e| FreeFetchError::Network(format!("Stream probe failed: {e}")))?;

    let status = resp.status();
    if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::GONE {
        return Err(FreeFetchError::Network(format!(
            "[{}] Stream probe rejected: status {status}",
            classify_http_status(status, "cdn").as_str()
        )));
    }

    let mut total_bytes = None;
    let mut supports_range = false;

    if status == reqwest::StatusCode::PARTIAL_CONTENT {
        supports_range = true;
        if let Some(cr) = resp.headers().get(CONTENT_RANGE) {
            if let Ok(cr_str) = cr.to_str() {
                if let Some(parsed) = parse_content_range(cr_str) {
                    total_bytes = parsed.total;
                }
            }
        }
    } else if status.is_success() {
        total_bytes = resp.content_length();
    }

    Ok(StreamProbeInfo {
        total_bytes,
        supports_range,
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn download_stream_parallel<F, S>(
    client: &Client,
    initial_url: &str,
    part_path: &Path,
    cancel_rx: watch::Receiver<bool>,
    rate_limiter: RateLimiter,
    max_concurrency: usize,
    url_refresher: Option<UrlRefresher>,
    mut on_progress: F,
    mut on_status_change: S,
) -> Result<u64, FreeFetchError>
where
    F: FnMut(ProgressUpdate) + Send + 'static,
    S: FnMut(DownloadStatus, &str) + Send + 'static,
{
    if let Some(parent) = part_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    // Step 1: Probe the stream URL for size and Range capability
    let probe = probe_stream(client, initial_url).await?;

    let total_bytes = match probe.total_bytes {
        Some(t) if t > 0 => t,
        _ => {
            // Cannot parallelize without total length; fallback to resilient single stream
            return crate::engine::chunk_downloader::download_stream_resilient(
                client,
                initial_url,
                part_path,
                cancel_rx,
                rate_limiter,
                url_refresher,
                on_progress,
                on_status_change,
            )
            .await;
        }
    };

    if !probe.supports_range || total_bytes < 2 * 1024 * 1024 {
        // Fallback to single stream if range not supported or file is very small (< 2 MB)
        return crate::engine::chunk_downloader::download_stream_resilient(
            client,
            initial_url,
            part_path,
            cancel_rx,
            rate_limiter,
            url_refresher,
            on_progress,
            on_status_change,
        )
        .await;
    }

    let meta_path = part_path.with_extension("part.meta");

    // Calculate number of workers: adaptive for YouTube, conservative for other CDNs
    let is_youtube = initial_url.contains("googlevideo.com") || initial_url.contains("youtube.com");
    let concurrency = if is_youtube {
        if total_bytes > 64 * 1024 * 1024 {
            max_concurrency.clamp(6, 10)
        } else if total_bytes > 8 * 1024 * 1024 {
            max_concurrency.clamp(4, 8)
        } else {
            max_concurrency.clamp(2, 6)
        }
    } else if total_bytes > 64 * 1024 * 1024 {
        max_concurrency.clamp(2, 8)
    } else if total_bytes > 8 * 1024 * 1024 {
        max_concurrency.clamp(2, 6)
    } else {
        max_concurrency.clamp(2, 4)
    };

    // Step 2: Initialize or load ranges
    let mut ranges = Vec::new();
    let mut loaded_from_meta = false;

    if meta_path.exists() && part_path.exists() {
        if let Ok(meta_str) = tokio::fs::read_to_string(&meta_path).await {
            if let Ok(meta) = serde_json::from_str::<PartMetadata>(&meta_str) {
                if meta.total_bytes == total_bytes && !meta.ranges.is_empty() {
                    ranges = meta.ranges;
                    loaded_from_meta = true;
                }
            }
        }
    }

    if !loaded_from_meta {
        ranges = partition_ranges(total_bytes, is_youtube, concurrency);

        let init_meta = PartMetadata {
            total_bytes,
            ranges: ranges.clone(),
        };
        if let Ok(json) = serde_json::to_string(&init_meta) {
            let _ = tokio::fs::write(&meta_path, json).await;
        }
    }

    // Step 3: Open positional writer
    let writer = PositionalFileWriter::open(part_path, total_bytes)?;

    // Shared state
    let initial_downloaded: u64 = ranges.iter().map(|r| r.downloaded_bytes).sum();
    let atomic_downloaded = Arc::new(AtomicU64::new(initial_downloaded));
    let shared_url = Arc::new(tokio::sync::RwLock::new(initial_url.to_string()));
    let shared_queue = Arc::new(Mutex::new(RangeWorkQueue::new(ranges)));
    let is_cancelled = Arc::new(AtomicBool::new(false));
    let meta_path_arc = Arc::new(meta_path.clone());
    let active_workers = Arc::new(AtomicUsize::new(0));
    let notify_work = Arc::new(tokio::sync::Notify::new());

    on_status_change(DownloadStatus::Downloading, "Downloading in parallel");

    // Spawn progress reporting task (approx 6-7 updates/sec every 150ms)
    let progress_stop = Arc::new(AtomicBool::new(false));
    let p_stop = progress_stop.clone();
    let p_downloaded = atomic_downloaded.clone();
    let p_cancel = cancel_rx.clone();

    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel::<ProgressUpdate>(32);

    tokio::spawn(async move {
        let mut tracker = SpeedTracker::default();
        let mut interval = tokio::time::interval(Duration::from_millis(150));

        while !p_stop.load(Ordering::Relaxed) {
            interval.tick().await;
            if *p_cancel.borrow() {
                break;
            }

            let cur = p_downloaded.load(Ordering::Relaxed);
            let speed = tracker.update(cur, Instant::now());
            let eta = tracker.calculate_eta(cur, total_bytes);

            let _ = progress_tx
                .send(ProgressUpdate {
                    downloaded_bytes: cur,
                    total_bytes,
                    speed_bytes_per_sec: speed,
                    eta_seconds: eta,
                })
                .await;
        }
    });

    // Step 4: Worker execution
    let mut worker_handles = Vec::new();

    for _ in 0..concurrency {
        let client = client.clone();
        let writer = writer.clone();
        let shared_url = shared_url.clone();
        let shared_queue = shared_queue.clone();
        let atomic_downloaded = atomic_downloaded.clone();
        let cancel_rx = cancel_rx.clone();
        let rate_limiter = rate_limiter.clone();
        let url_refresher = url_refresher.clone();
        let meta_path = meta_path_arc.clone();
        let active_workers = active_workers.clone();
        let is_cancelled = is_cancelled.clone();
        let notify_work = notify_work.clone();

        let handle = tokio::spawn(async move {
            active_workers.fetch_add(1, Ordering::SeqCst);

            loop {
                if *cancel_rx.borrow() || is_cancelled.load(Ordering::Relaxed) {
                    active_workers.fetch_sub(1, Ordering::SeqCst);
                    return Err(FreeFetchError::Cancelled);
                }

                let next_task = {
                    let mut q = shared_queue.lock().await;
                    q.claim_next()
                };

                let (range_idx, mut range) = match next_task {
                    Some(t) => t,
                    None => {
                        let q = shared_queue.lock().await;
                        if q.is_all_finished() {
                            break;
                        }
                        drop(q);
                        tokio::select! {
                            _ = notify_work.notified() => {}
                            _ = tokio::time::sleep(Duration::from_millis(50)) => {}
                        }
                        continue;
                    }
                };

                let mut stall_watchdog = StallWatchdog::new(
                    DEFAULT_STALL_THRESHOLD_BYTES_PER_SEC,
                    DEFAULT_STALL_DURATION,
                );

                let max_retries = 5;
                let mut attempt = 0;

                while !range.is_finished {
                    if *cancel_rx.borrow() || is_cancelled.load(Ordering::Relaxed) {
                        {
                            let mut q = shared_queue.lock().await;
                            q.unclaim(range_idx);
                        }
                        active_workers.fetch_sub(1, Ordering::SeqCst);
                        return Err(FreeFetchError::Cancelled);
                    }

                    let current_url = {
                        let u = shared_url.read().await;
                        u.clone()
                    };

                    let request_start = range.start_byte + range.downloaded_bytes;
                    if request_start > range.end_byte {
                        {
                            let mut q = shared_queue.lock().await;
                            q.finish_range(range_idx);
                            if let Ok(json) = serde_json::to_string(&PartMetadata {
                                total_bytes,
                                ranges: q.ranges.clone(),
                            }) {
                                let _ = tokio::fs::write(&*meta_path, json).await;
                            }
                        }
                        notify_work.notify_waiters();
                        break;
                    }

                    let req_range = format!("bytes={request_start}-{}", range.end_byte);

                    let resp_res = client
                        .get(&current_url)
                        .header("User-Agent", "Mozilla/5.0 FreeFetch/0.1")
                        .header(RANGE, req_range.clone())
                        .send()
                        .await;

                    let resp = match resp_res {
                        Ok(r) => r,
                        Err(e) => {
                            let err_kind = classify_reqwest_error(&e);
                            if err_kind.is_transient() && attempt < max_retries {
                                attempt += 1;
                                let delay_ms = (1000u64 << (attempt - 1)).min(16000)
                                    + (attempt as u64 * 37 % 200);
                                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                                continue;
                            } else {
                                {
                                    let mut q = shared_queue.lock().await;
                                    q.unclaim(range_idx);
                                }
                                active_workers.fetch_sub(1, Ordering::SeqCst);
                                return Err(FreeFetchError::Network(format!(
                                    "[{}] Range {range_idx} connection failed: {e}",
                                    err_kind.as_str()
                                )));
                            }
                        }
                    };

                    let status = resp.status();

                    if status == reqwest::StatusCode::FORBIDDEN
                        || status == reqwest::StatusCode::GONE
                    {
                        if let Some(ref refresher) = url_refresher {
                            if attempt < max_retries {
                                attempt += 1;
                                match refresher().await {
                                    Ok(new_url) => {
                                        {
                                            let mut u = shared_url.write().await;
                                            *u = new_url;
                                        }
                                        tokio::time::sleep(Duration::from_millis(400)).await;
                                        continue;
                                    }
                                    Err(e) => {
                                        {
                                            let mut q = shared_queue.lock().await;
                                            q.unclaim(range_idx);
                                        }
                                        active_workers.fetch_sub(1, Ordering::SeqCst);
                                        return Err(FreeFetchError::Extractor {
                                            service: "youtube".to_string(),
                                            message: format!("Failed to refresh stream URL: {e}"),
                                        });
                                    }
                                }
                            }
                        }

                        {
                            let mut q = shared_queue.lock().await;
                            q.unclaim(range_idx);
                        }
                        active_workers.fetch_sub(1, Ordering::SeqCst);
                        return Err(FreeFetchError::Network(format!(
                            "[HTTP_403] Range {range_idx} access forbidden"
                        )));
                    } else if status == reqwest::StatusCode::TOO_MANY_REQUESTS
                        || status == reqwest::StatusCode::REQUEST_TIMEOUT
                        || status.is_server_error()
                    {
                        if attempt < max_retries {
                            attempt += 1;
                            let delay_ms =
                                (1000u64 << (attempt - 1)).min(16000) + (attempt as u64 * 53 % 200);
                            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                            continue;
                        } else {
                            {
                                let mut q = shared_queue.lock().await;
                                q.unclaim(range_idx);
                            }
                            active_workers.fetch_sub(1, Ordering::SeqCst);
                            return Err(FreeFetchError::Network(format!(
                                "[{}] Range {range_idx} server error status {status}",
                                classify_http_status(status, "cdn").as_str()
                            )));
                        }
                    } else if status != reqwest::StatusCode::PARTIAL_CONTENT
                        && status != reqwest::StatusCode::OK
                    {
                        {
                            let mut q = shared_queue.lock().await;
                            q.unclaim(range_idx);
                        }
                        active_workers.fetch_sub(1, Ordering::SeqCst);
                        return Err(FreeFetchError::Network(format!(
                            "HTTP status {status} on Range {range_idx}"
                        )));
                    }

                    // Streaming body chunks directly to positional file writer
                    let mut stream = resp.bytes_stream();
                    let mut stream_interrupted = false;
                    let mut stalled = false;

                    loop {
                        if *cancel_rx.borrow() || is_cancelled.load(Ordering::Relaxed) {
                            {
                                let mut q = shared_queue.lock().await;
                                q.unclaim(range_idx);
                            }
                            active_workers.fetch_sub(1, Ordering::SeqCst);
                            return Err(FreeFetchError::Cancelled);
                        }

                        enum StreamEvent<T> {
                            Chunk(T),
                            EndOfStream,
                            WatchdogTick,
                        }

                        let event = tokio::select! {
                            item = stream.next() => {
                                match item {
                                    Some(chunk) => StreamEvent::Chunk(chunk),
                                    None => StreamEvent::EndOfStream,
                                }
                            }
                            _ = tokio::time::sleep(Duration::from_millis(500)) => {
                                StreamEvent::WatchdogTick
                            }
                        };

                        match event {
                            StreamEvent::Chunk(Ok(chunk)) => {
                                let chunk_len = chunk.len();
                                let write_pos = range.start_byte + range.downloaded_bytes;

                                if let Err(e) = writer.write_at(write_pos, &chunk) {
                                    {
                                        let mut q = shared_queue.lock().await;
                                        q.unclaim(range_idx);
                                    }
                                    active_workers.fetch_sub(1, Ordering::SeqCst);
                                    return Err(e);
                                }

                                range.downloaded_bytes += chunk_len as u64;
                                atomic_downloaded.fetch_add(chunk_len as u64, Ordering::Relaxed);
                                stall_watchdog.record_bytes(chunk_len as u64);

                                rate_limiter
                                    .throttle(chunk_len, Duration::from_millis(10))
                                    .await;

                                if range.start_byte + range.downloaded_bytes > range.end_byte {
                                    range.is_finished = true;
                                    break;
                                }
                            }
                            StreamEvent::Chunk(Err(_e)) => {
                                stream_interrupted = true;
                                break;
                            }
                            StreamEvent::EndOfStream => {
                                if range.start_byte + range.downloaded_bytes <= range.end_byte {
                                    stream_interrupted = true;
                                }
                                break;
                            }
                            StreamEvent::WatchdogTick => {}
                        }

                        // Evaluate stall watchdog if YouTube
                        if is_youtube {
                            let remaining = (range.end_byte + 1)
                                .saturating_sub(range.start_byte + range.downloaded_bytes);
                            let rec_count = {
                                let q = shared_queue.lock().await;
                                q.stall_counts.get(range_idx).copied().unwrap_or(0)
                            };

                            if stall_watchdog.evaluate(Instant::now(), remaining, rec_count) {
                                stalled = true;
                                break;
                            }
                        }
                    }

                    // Update shared queue progress & persist .part.meta
                    {
                        let mut q = shared_queue.lock().await;
                        q.update_progress(range_idx, range.downloaded_bytes);
                        if range.is_finished {
                            q.finish_range(range_idx);
                        }
                        if let Ok(json) = serde_json::to_string(&PartMetadata {
                            total_bytes,
                            ranges: q.ranges.clone(),
                        }) {
                            let _ = tokio::fs::write(&*meta_path, json).await;
                        }
                    }

                    if stalled {
                        let _ = {
                            let mut q = shared_queue.lock().await;
                            q.record_stall(range_idx)
                        };
                        attempt = 0;
                        stall_watchdog.reset_stall_timer();
                        continue;
                    }

                    if stream_interrupted && !range.is_finished {
                        if attempt < max_retries {
                            attempt += 1;
                            let delay_ms = (1000u64 << (attempt - 1)).min(16000);
                            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                            continue;
                        } else {
                            {
                                let mut q = shared_queue.lock().await;
                                q.unclaim(range_idx);
                            }
                            active_workers.fetch_sub(1, Ordering::SeqCst);
                            return Err(FreeFetchError::Network(format!(
                                "Stream disconnected on Range {range_idx}"
                            )));
                        }
                    } else if !stream_interrupted {
                        attempt = 0;
                    }
                }

                notify_work.notify_waiters();
            }

            active_workers.fetch_sub(1, Ordering::SeqCst);
            Ok(())
        });

        worker_handles.push(handle);
    }

    // Step 5: Coordinate progress receiver and worker completions
    let worker_future = async {
        for handle in worker_handles {
            handle
                .await
                .map_err(|e| FreeFetchError::Generic(e.to_string()))??;
        }
        Ok::<(), FreeFetchError>(())
    };

    tokio::pin!(worker_future);

    loop {
        tokio::select! {
            res = &mut worker_future => {
                progress_stop.store(true, Ordering::Relaxed);
                res?;
                break;
            }
            Some(update) = progress_rx.recv() => {
                on_progress(update);
            }
        }
    }

    // Flush any remaining progress updates
    while let Ok(update) = progress_rx.try_recv() {
        on_progress(update);
    }

    writer.sync_all()?;
    let _ = tokio::fs::remove_file(&meta_path).await;

    let final_bytes = atomic_downloaded.load(Ordering::Relaxed);
    on_progress(ProgressUpdate {
        downloaded_bytes: final_bytes,
        total_bytes,
        speed_bytes_per_sec: 0,
        eta_seconds: 0,
    });

    Ok(final_bytes)
}
