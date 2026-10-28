use reqwest::Client;
use std::path::Path;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::sync::watch;

use crate::core::error::FreeFetchError;
use crate::core::types::SubtitleFormat;
use crate::engine::chunk_downloader::UrlRefresher;
use crate::engine::rate_limiter::RateLimiter;
use crate::media::{document_to_srt, document_to_vtt, parse_vtt};

const MAX_SUBTITLE_BYTES: usize = 5 * 1024 * 1024; // 5 MB ceiling
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// Retry policy for subtitle downloads under HTTP 429 and transient errors.
#[derive(Debug, Clone)]
pub struct SubtitleRetryPolicy {
    pub max_retries: u32,
    pub base_429_delay: Duration,
    pub max_429_delay: Duration,
    pub retry_after_max: Duration,
    pub jitter_factor: f64,
}

impl Default for SubtitleRetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 4,
            base_429_delay: Duration::from_secs(2), // 2s -> 4s -> 8s -> 16s
            max_429_delay: Duration::from_secs(30),
            retry_after_max: Duration::from_secs(30),
            jitter_factor: 0.25,
        }
    }
}

impl SubtitleRetryPolicy {
    /// Accelerated configuration for unit/integration tests
    pub fn for_tests() -> Self {
        Self {
            max_retries: 4,
            base_429_delay: Duration::from_millis(5),
            max_429_delay: Duration::from_millis(50),
            retry_after_max: Duration::from_millis(100),
            jitter_factor: 0.0,
        }
    }

    /// Calculate backoff duration for HTTP 429 response
    pub fn calculate_429_delay(&self, attempt: u32, retry_after_hdr: Option<&str>) -> Duration {
        if let Some(hdr) = retry_after_hdr {
            let max_secs = self.retry_after_max.as_secs().max(1);
            if let Some(parsed) = parse_retry_after(hdr, max_secs) {
                if self.retry_after_max < Duration::from_secs(1) {
                    return self.retry_after_max;
                }
                return parsed.min(self.retry_after_max);
            }
        }

        let exp = 1u64 << (attempt.saturating_sub(1).min(5));
        let raw_ms = self.base_429_delay.as_millis() as u64 * exp;
        let clamped_ms = raw_ms.min(self.max_429_delay.as_millis() as u64);

        if self.jitter_factor > 0.0 {
            let jitter_range = (clamped_ms as f64 * self.jitter_factor) as u64;
            let jitter = (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_micros() as u64)
                % jitter_range.max(1);
            Duration::from_millis((clamped_ms + jitter).min(self.max_429_delay.as_millis() as u64))
        } else {
            Duration::from_millis(clamped_ms)
        }
    }
}

/// Parses the Retry-After header per RFC 7231 (seconds or HTTP-date).
/// Clamps parsed value to safe boundaries [1, max_clamp_secs].
pub fn parse_retry_after(header_val: &str, max_clamp_secs: u64) -> Option<Duration> {
    let trimmed = header_val.trim();
    if trimmed.is_empty() {
        return None;
    }

    // 1. Numeric seconds: "5", "30", etc.
    if let Ok(secs) = trimmed.parse::<u64>() {
        let clamped = secs.clamp(1, max_clamp_secs);
        return Some(Duration::from_secs(clamped));
    }

    // 2. HTTP-Date: "Wed, 21 Oct 2026 07:28:00 GMT" (RFC 2822 / IMF-fixdate)
    if let Ok(parsed_date) = chrono::DateTime::parse_from_rfc2822(trimmed) {
        let now = chrono::Utc::now();
        let diff = parsed_date.signed_duration_since(now).num_seconds();
        if diff > 0 {
            let clamped = (diff as u64).clamp(1, max_clamp_secs);
            return Some(Duration::from_secs(clamped));
        } else {
            // Already elapsed date -> minimal 1s retry delay
            return Some(Duration::from_secs(1));
        }
    }

    None
}

/// Cancellable sleep that wakes immediately when cancel_rx triggers true.
pub async fn cancellable_sleep(
    duration: Duration,
    cancel_rx: &mut Option<watch::Receiver<bool>>,
) -> Result<(), FreeFetchError> {
    if let Some(ref mut rx) = cancel_rx {
        if *rx.borrow() {
            return Err(FreeFetchError::Cancelled);
        }
        let sleep_fut = tokio::time::sleep(duration);
        tokio::pin!(sleep_fut);

        loop {
            tokio::select! {
                () = &mut sleep_fut => return Ok(()),
                change = rx.changed() => {
                    if change.is_err() || *rx.borrow() {
                        return Err(FreeFetchError::Cancelled);
                    }
                }
            }
        }
    } else {
        tokio::time::sleep(duration).await;
        Ok(())
    }
}

/// Downloads a subtitle file, parses its cues, converts to target format (SRT or VTT),
/// and saves atomically to destination path.
pub async fn download_and_convert_subtitle(
    client: &Client,
    initial_url: &str,
    dest_path: &Path,
    target_format: SubtitleFormat,
    cancel_rx: Option<watch::Receiver<bool>>,
    url_refresher: Option<UrlRefresher>,
) -> Result<u64, FreeFetchError> {
    download_and_convert_subtitle_with_policy(
        client,
        initial_url,
        dest_path,
        target_format,
        cancel_rx,
        url_refresher,
        SubtitleRetryPolicy::default(),
        None,
    )
    .await
}

/// Downloads a subtitle file with shared rate-limiter support.
pub async fn download_and_convert_subtitle_with_limiter(
    client: &Client,
    initial_url: &str,
    dest_path: &Path,
    target_format: SubtitleFormat,
    cancel_rx: Option<watch::Receiver<bool>>,
    url_refresher: Option<UrlRefresher>,
    rate_limiter: Option<&RateLimiter>,
) -> Result<u64, FreeFetchError> {
    download_and_convert_subtitle_with_policy(
        client,
        initial_url,
        dest_path,
        target_format,
        cancel_rx,
        url_refresher,
        SubtitleRetryPolicy::default(),
        rate_limiter,
    )
    .await
}

/// Downloads a subtitle file with explicit retry policy and rate-limiter integration.
#[allow(clippy::too_many_arguments)]
pub async fn download_and_convert_subtitle_with_policy(
    client: &Client,
    initial_url: &str,
    dest_path: &Path,
    target_format: SubtitleFormat,
    mut cancel_rx: Option<watch::Receiver<bool>>,
    url_refresher: Option<UrlRefresher>,
    policy: SubtitleRetryPolicy,
    rate_limiter: Option<&RateLimiter>,
) -> Result<u64, FreeFetchError> {
    let mut current_url = initial_url.to_string();
    let mut attempts = 0;

    let tmp_path = dest_path.with_extension(format!("{}.tmp", target_format.extension()));

    loop {
        if let Some(ref mut rx) = cancel_rx {
            if *rx.borrow() {
                let _ = tokio::fs::remove_file(&tmp_path).await;
                return Err(FreeFetchError::Cancelled);
            }
        }

        attempts += 1;

        // Ensure YouTube timedtext requests WebVTT if it doesn't already specify a format
        let req_url = if current_url.contains("timedtext") && !current_url.contains("fmt=") {
            if current_url.contains('?') {
                format!("{current_url}&fmt=vtt")
            } else {
                format!("{current_url}?fmt=vtt")
            }
        } else {
            current_url.clone()
        };

        let request = client
            .get(&req_url)
            .timeout(REQUEST_TIMEOUT)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36");

        let response_result = request.send().await;

        match response_result {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    // Check Content-Length if present
                    if let Some(len) = resp.content_length() {
                        if len as usize > MAX_SUBTITLE_BYTES {
                            let _ = tokio::fs::remove_file(&tmp_path).await;
                            return Err(FreeFetchError::Generic(format!(
                                "Subtitle payload exceeded 5MB ceiling: {len} bytes"
                            )));
                        }
                    }

                    let bytes = resp
                        .bytes()
                        .await
                        .map_err(|e| FreeFetchError::Network(e.to_string()))?;

                    if bytes.len() > MAX_SUBTITLE_BYTES {
                        let _ = tokio::fs::remove_file(&tmp_path).await;
                        return Err(FreeFetchError::Generic(format!(
                            "Subtitle payload exceeded 5MB ceiling: {} bytes",
                            bytes.len()
                        )));
                    }

                    // Throttle through shared rate limiter if configured
                    if let Some(limiter) = rate_limiter {
                        limiter
                            .throttle(bytes.len(), Duration::from_millis(10))
                            .await;
                    }

                    let content_str = String::from_utf8_lossy(&bytes).to_string();

                    // Parse cues safely
                    let doc = parse_vtt(&content_str);

                    // Convert to requested output format
                    let final_text = match target_format {
                        SubtitleFormat::Srt => document_to_srt(&doc),
                        SubtitleFormat::Vtt => document_to_vtt(&doc),
                    };

                    // Atomic write
                    if let Some(parent) = dest_path.parent() {
                        tokio::fs::create_dir_all(parent).await?;
                    }

                    {
                        let mut file = tokio::fs::File::create(&tmp_path).await?;
                        file.write_all(final_text.as_bytes()).await?;
                        file.flush().await?;
                    }

                    if dest_path.exists() {
                        let _ = tokio::fs::remove_file(dest_path).await;
                    }
                    tokio::fs::rename(&tmp_path, dest_path).await?;

                    return Ok(final_text.len() as u64);
                } else if status.as_u16() == 404 {
                    // Non-retryable
                    let _ = tokio::fs::remove_file(&tmp_path).await;
                    return Err(FreeFetchError::NotFound(
                        "Subtitle track not found or deleted (HTTP 404)".to_string(),
                    ));
                } else if status.as_u16() == 429 {
                    // Explicit HTTP 429 Rate Limiting with Retry-After and cancellable backoff
                    let retry_after_hdr = resp
                        .headers()
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|h| h.to_str().ok());

                    let backoff = policy.calculate_429_delay(attempts, retry_after_hdr);

                    if attempts <= policy.max_retries {
                        if let Err(e) = cancellable_sleep(backoff, &mut cancel_rx).await {
                            let _ = tokio::fs::remove_file(&tmp_path).await;
                            return Err(e);
                        }
                        continue;
                    } else {
                        let _ = tokio::fs::remove_file(&tmp_path).await;
                        return Err(FreeFetchError::Network(format!(
                            "HTTP 429 Too Many Requests: rate-limit retry attempts exhausted (attempt {}/{}, last wait {:.1?})",
                            attempts, policy.max_retries, backoff
                        )));
                    }
                } else if (status.as_u16() == 403 || status.as_u16() == 410)
                    && url_refresher.is_some()
                    && attempts <= policy.max_retries
                {
                    // URL expired, refresh signed URL (do NOT treat 403/410 as 429)
                    if let Some(ref refresher) = url_refresher {
                        if let Ok(fresh) = refresher().await {
                            current_url = fresh;
                            if let Err(e) =
                                cancellable_sleep(Duration::from_millis(500), &mut cancel_rx).await
                            {
                                let _ = tokio::fs::remove_file(&tmp_path).await;
                                return Err(e);
                            }
                            continue;
                        }
                    }
                    let _ = tokio::fs::remove_file(&tmp_path).await;
                    return Err(FreeFetchError::Network(format!(
                        "Subtitle URL access forbidden (HTTP {status})"
                    )));
                } else if (status.as_u16() == 408 || status.is_server_error())
                    && attempts <= policy.max_retries
                {
                    // Transient 5xx / 408 server errors
                    let backoff = Duration::from_millis(500 * (1 << (attempts - 1)));
                    if let Err(e) = cancellable_sleep(backoff, &mut cancel_rx).await {
                        let _ = tokio::fs::remove_file(&tmp_path).await;
                        return Err(e);
                    }
                    continue;
                } else {
                    let _ = tokio::fs::remove_file(&tmp_path).await;
                    return Err(FreeFetchError::Network(format!(
                        "Subtitle download failed with HTTP status: {status}"
                    )));
                }
            }
            Err(e) => {
                if attempts <= policy.max_retries && (e.is_timeout() || e.is_connect()) {
                    let backoff = Duration::from_millis(500 * (1 << (attempts - 1)));
                    if let Err(c_err) = cancellable_sleep(backoff, &mut cancel_rx).await {
                        let _ = tokio::fs::remove_file(&tmp_path).await;
                        return Err(c_err);
                    }
                    continue;
                }
                let _ = tokio::fs::remove_file(&tmp_path).await;
                return Err(FreeFetchError::Network(e.to_string()));
            }
        }
    }
}
