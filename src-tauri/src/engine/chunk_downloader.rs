use futures_util::StreamExt;
use reqwest::header::{CONTENT_LENGTH, RANGE};
use reqwest::Client;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::sync::watch;
use url::Url;

use crate::core::error::FreeFetchError;
use crate::core::types::DownloadStatus;
use crate::engine::rate_limiter::RateLimiter;

pub type UrlRefresher = Arc<
    dyn Fn() -> Pin<Box<dyn Future<Output = Result<String, FreeFetchError>> + Send>> + Send + Sync,
>;

pub struct ProgressUpdate {
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub speed_bytes_per_sec: u64,
    pub eta_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkErrorKind {
    NetworkTimeout,
    ConnectionReset,
    ConnectionAborted,
    IncompleteResponse,
    Http403,
    Http408,
    Http416,
    Http429,
    Http5xx(u16),
    StreamUrlExpired,
    RangeMismatch,
    DiskError,
    UnknownNetworkError,
}

impl NetworkErrorKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NetworkTimeout => "NETWORK_TIMEOUT",
            Self::ConnectionReset => "CONNECTION_RESET",
            Self::ConnectionAborted => "CONNECTION_ABORTED",
            Self::IncompleteResponse => "INCOMPLETE_RESPONSE",
            Self::Http403 => "HTTP_403",
            Self::Http408 => "HTTP_408",
            Self::Http416 => "HTTP_416",
            Self::Http429 => "HTTP_429",
            Self::Http5xx(_) => "HTTP_5XX",
            Self::StreamUrlExpired => "STREAM_URL_EXPIRED",
            Self::RangeMismatch => "RANGE_MISMATCH",
            Self::DiskError => "DISK_ERROR",
            Self::UnknownNetworkError => "UNKNOWN_NETWORK_ERROR",
        }
    }

    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::NetworkTimeout
                | Self::ConnectionReset
                | Self::ConnectionAborted
                | Self::IncompleteResponse
                | Self::Http408
                | Self::Http429
                | Self::Http5xx(_)
        )
    }
}

#[derive(Debug, Clone)]
pub struct DownloadDiagnostics {
    pub safe_url: String,
    pub host: String,
    pub status: Option<u16>,
    pub requested_range: Option<String>,
    pub current_offset: u64,
    pub total_bytes: u64,
    pub downloaded_bytes: u64,
    pub retry_count: u32,
    pub error_kind: NetworkErrorKind,
    pub details: String,
}

impl std::fmt::Display for DownloadDiagnostics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}] host='{}', offset={}/{}, retries={}, status={:?}: {}",
            self.error_kind.as_str(),
            self.host,
            self.current_offset,
            self.total_bytes,
            self.retry_count,
            self.status,
            self.details
        )
    }
}

pub fn sanitize_url_for_logging(url_str: &str) -> (String, String) {
    if let Ok(mut u) = Url::parse(url_str) {
        let host = u.host_str().unwrap_or("unknown").to_string();
        u.set_query(None);
        (
            format!("{}://{}/... [params redacted]", u.scheme(), host),
            host,
        )
    } else {
        ("[invalid url]".to_string(), "unknown".to_string())
    }
}

pub fn classify_reqwest_error(err: &reqwest::Error) -> NetworkErrorKind {
    if err.is_timeout() {
        NetworkErrorKind::NetworkTimeout
    } else if err.is_connect() {
        let s = err.to_string().to_lowercase();
        if s.contains("connection reset")
            || s.contains("forcibly closed")
            || s.contains("reset by peer")
        {
            NetworkErrorKind::ConnectionReset
        } else if s.contains("abort") || s.contains("broken pipe") {
            NetworkErrorKind::ConnectionAborted
        } else {
            NetworkErrorKind::ConnectionReset
        }
    } else if err.is_body() || err.is_decode() {
        NetworkErrorKind::IncompleteResponse
    } else {
        let s = err.to_string().to_lowercase();
        if s.contains("timeout") {
            NetworkErrorKind::NetworkTimeout
        } else if s.contains("reset") {
            NetworkErrorKind::ConnectionReset
        } else if s.contains("abort") {
            NetworkErrorKind::ConnectionAborted
        } else if s.contains("closed")
            || s.contains("broken pipe")
            || s.contains("connection")
            || err.is_request()
        {
            NetworkErrorKind::ConnectionReset
        } else {
            NetworkErrorKind::UnknownNetworkError
        }
    }
}

pub fn classify_http_status(status: reqwest::StatusCode, host: &str) -> NetworkErrorKind {
    match status.as_u16() {
        403 => {
            if host.contains("googlevideo.com") || host.contains("youtube.com") {
                NetworkErrorKind::StreamUrlExpired
            } else {
                NetworkErrorKind::Http403
            }
        }
        408 => NetworkErrorKind::Http408,
        416 => NetworkErrorKind::Http416,
        429 => NetworkErrorKind::Http429,
        500..=599 => NetworkErrorKind::Http5xx(status.as_u16()),
        _ => NetworkErrorKind::UnknownNetworkError,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedContentRange {
    pub start: Option<u64>,
    pub end: Option<u64>,
    pub total: Option<u64>,
}

pub fn parse_content_range(header_val: &str) -> Option<ParsedContentRange> {
    let header_val = header_val.trim();
    if !header_val.starts_with("bytes ") {
        return None;
    }
    let rest = header_val[6..].trim();
    let parts: Vec<&str> = rest.split('/').collect();
    if parts.len() != 2 {
        return None;
    }

    let range_part = parts[0].trim();
    let total_part = parts[1].trim();

    let total = total_part.parse::<u64>().ok();

    if range_part == "*" {
        return Some(ParsedContentRange {
            start: None,
            end: None,
            total,
        });
    }

    let range_split: Vec<&str> = range_part.split('-').collect();
    if range_split.len() == 2 {
        let start = range_split[0].parse::<u64>().ok();
        let end = range_split[1].parse::<u64>().ok();
        Some(ParsedContentRange { start, end, total })
    } else {
        None
    }
}

pub async fn download_stream_to_file<F>(
    client: &Client,
    url: &str,
    part_path: &Path,
    cancel_rx: watch::Receiver<bool>,
    rate_limiter: RateLimiter,
    on_progress: F,
) -> Result<u64, FreeFetchError>
where
    F: FnMut(ProgressUpdate) + Send + 'static,
{
    download_stream_resilient(
        client,
        url,
        part_path,
        cancel_rx,
        rate_limiter,
        None,
        on_progress,
        |_, _| {},
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn download_stream_resilient<F, S>(
    client: &Client,
    initial_url: &str,
    part_path: &Path,
    cancel_rx: watch::Receiver<bool>,
    rate_limiter: RateLimiter,
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

    let max_retries = 5;
    let mut attempt = 0;
    let mut current_url = initial_url.to_string();

    loop {
        if *cancel_rx.borrow() {
            return Err(FreeFetchError::Cancelled);
        }

        let (safe_url, host) = sanitize_url_for_logging(&current_url);

        let local_offset = if part_path.exists() {
            tokio::fs::metadata(part_path)
                .await
                .map(|m| m.len())
                .unwrap_or(0)
        } else {
            0
        };

        let mut req = client
            .get(&current_url)
            .header("User-Agent", "Mozilla/5.0 FreeFetch/0.1");
        if local_offset > 0 {
            req = req.header(RANGE, format!("bytes={local_offset}-"));
        }

        let resp_res = req.send().await;

        let resp = match resp_res {
            Ok(r) => r,
            Err(e) => {
                let err_kind = classify_reqwest_error(&e);
                if err_kind.is_transient() && attempt < max_retries {
                    attempt += 1;
                    let delay_ms =
                        (1000u64 << (attempt - 1)).min(16000) + (attempt as u64 * 73 % 200);
                    on_status_change(
                        DownloadStatus::Retrying,
                        &format!(
                            "Connection interrupted ({}) — retrying (attempt {attempt}/{max_retries})...",
                            err_kind.as_str()
                        ),
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                    continue;
                } else {
                    let diag = DownloadDiagnostics {
                        safe_url,
                        host,
                        status: None,
                        requested_range: if local_offset > 0 {
                            Some(format!("bytes={local_offset}-"))
                        } else {
                            None
                        },
                        current_offset: local_offset,
                        total_bytes: 0,
                        downloaded_bytes: local_offset,
                        retry_count: attempt,
                        error_kind: err_kind,
                        details: e.to_string(),
                    };
                    return Err(FreeFetchError::Network(diag.to_string()));
                }
            }
        };

        let status = resp.status();

        if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::GONE {
            if url_refresher.is_some() && attempt < max_retries {
                attempt += 1;
                on_status_change(
                    DownloadStatus::Retrying,
                    "Stream URL expired — refreshing connection...",
                );
                if let Some(ref refresher) = url_refresher {
                    match refresher().await {
                        Ok(new_url) => {
                            current_url = new_url;
                            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                            continue;
                        }
                        Err(e) => {
                            return Err(FreeFetchError::Extractor {
                                service: "youtube".to_string(),
                                message: format!("Failed to refresh expired stream URL: {e}"),
                            });
                        }
                    }
                }
            } else {
                let diag = DownloadDiagnostics {
                    safe_url,
                    host,
                    status: Some(status.as_u16()),
                    requested_range: if local_offset > 0 {
                        Some(format!("bytes={local_offset}-"))
                    } else {
                        None
                    },
                    current_offset: local_offset,
                    total_bytes: 0,
                    downloaded_bytes: local_offset,
                    retry_count: attempt,
                    error_kind: NetworkErrorKind::Http403,
                    details: "Access forbidden (URL may be expired or signature invalid)"
                        .to_string(),
                };
                return Err(FreeFetchError::Network(diag.to_string()));
            }
        } else if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
            let content_range = resp
                .headers()
                .get(reqwest::header::CONTENT_RANGE)
                .and_then(|v| v.to_str().ok())
                .and_then(parse_content_range);

            if let Some(cr) = content_range {
                if let Some(total) = cr.total {
                    if local_offset >= total && total > 0 {
                        on_progress(ProgressUpdate {
                            downloaded_bytes: total,
                            total_bytes: total,
                            speed_bytes_per_sec: 0,
                            eta_seconds: 0,
                        });
                        return Ok(total);
                    }
                }
            }

            if attempt < max_retries {
                attempt += 1;
                let _ = tokio::fs::remove_file(part_path).await;
                on_status_change(
                    DownloadStatus::Retrying,
                    "Invalid range — restarting stream from offset 0...",
                );
                continue;
            } else {
                let diag = DownloadDiagnostics {
                    safe_url,
                    host,
                    status: Some(416),
                    requested_range: Some(format!("bytes={local_offset}-")),
                    current_offset: local_offset,
                    total_bytes: 0,
                    downloaded_bytes: local_offset,
                    retry_count: attempt,
                    error_kind: NetworkErrorKind::Http416,
                    details: "Range not satisfiable".to_string(),
                };
                return Err(FreeFetchError::Network(diag.to_string()));
            }
        } else if status == reqwest::StatusCode::TOO_MANY_REQUESTS
            || status == reqwest::StatusCode::REQUEST_TIMEOUT
            || status.is_server_error()
        {
            if attempt < max_retries {
                attempt += 1;
                let delay_ms = (1000u64 << (attempt - 1)).min(16000) + (attempt as u64 * 97 % 200);
                on_status_change(
                    DownloadStatus::Retrying,
                    &format!(
                        "Server busy/error ({}) — retrying (attempt {attempt}/{max_retries})...",
                        status.as_u16()
                    ),
                );
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                continue;
            } else {
                let diag = DownloadDiagnostics {
                    safe_url,
                    host: host.clone(),
                    status: Some(status.as_u16()),
                    requested_range: if local_offset > 0 {
                        Some(format!("bytes={local_offset}-"))
                    } else {
                        None
                    },
                    current_offset: local_offset,
                    total_bytes: 0,
                    downloaded_bytes: local_offset,
                    retry_count: attempt,
                    error_kind: classify_http_status(status, &host),
                    details: format!("Server returned error status {status}"),
                };
                return Err(FreeFetchError::Network(diag.to_string()));
            }
        } else if !status.is_success() && status != reqwest::StatusCode::PARTIAL_CONTENT {
            let diag = DownloadDiagnostics {
                safe_url,
                host: host.clone(),
                status: Some(status.as_u16()),
                requested_range: if local_offset > 0 {
                    Some(format!("bytes={local_offset}-"))
                } else {
                    None
                },
                current_offset: local_offset,
                total_bytes: 0,
                downloaded_bytes: local_offset,
                retry_count: attempt,
                error_kind: classify_http_status(status, &host),
                details: format!("HTTP error status {status}"),
            };
            return Err(FreeFetchError::Network(diag.to_string()));
        }

        let (file, mut current_downloaded, total_bytes) =
            if status == reqwest::StatusCode::PARTIAL_CONTENT {
                let cr_header = resp
                    .headers()
                    .get(reqwest::header::CONTENT_RANGE)
                    .and_then(|v| v.to_str().ok());
                let parsed_cr = cr_header.and_then(parse_content_range);

                if let Some(ref cr) = parsed_cr {
                    if let Some(start) = cr.start {
                        if start != local_offset {
                            let diag = DownloadDiagnostics {
                                safe_url,
                                host,
                                status: Some(206),
                                requested_range: Some(format!("bytes={local_offset}-")),
                                current_offset: local_offset,
                                total_bytes: cr.total.unwrap_or(0),
                                downloaded_bytes: local_offset,
                                retry_count: attempt,
                                error_kind: NetworkErrorKind::RangeMismatch,
                                details: format!(
                                "Server returned starting offset {start}, expected {local_offset}"
                            ),
                            };
                            return Err(FreeFetchError::Network(diag.to_string()));
                        }
                    }
                }

                let total = parsed_cr.and_then(|cr| cr.total).unwrap_or_else(|| {
                    let content_length = resp
                        .headers()
                        .get(CONTENT_LENGTH)
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.parse::<u64>().ok())
                        .unwrap_or(0);
                    local_offset + content_length
                });

                let f = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(part_path)
                    .await?;

                (f, local_offset, total)
            } else {
                let content_length = resp
                    .headers()
                    .get(CONTENT_LENGTH)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(0);

                let f = OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .open(part_path)
                    .await?;

                (f, 0u64, content_length)
            };

        on_status_change(DownloadStatus::Downloading, "Downloading");

        let mut writer = tokio::io::BufWriter::new(file);
        let mut stream = resp.bytes_stream();
        let start_time = Instant::now();
        let mut last_progress_report = Instant::now();
        let mut stream_failed = false;
        let mut last_stream_err = None;

        while let Some(chunk_res) = stream.next().await {
            if *cancel_rx.borrow() {
                let _ = writer.flush().await;
                return Err(FreeFetchError::Cancelled);
            }

            match chunk_res {
                Ok(chunk) => {
                    let chunk_len = chunk.len();
                    if let Err(e) = writer.write_all(&chunk).await {
                        let _ = writer.flush().await;
                        return Err(FreeFetchError::Io(e.to_string()));
                    }

                    current_downloaded += chunk_len as u64;

                    rate_limiter
                        .throttle(chunk_len, last_progress_report.elapsed())
                        .await;

                    if last_progress_report.elapsed().as_millis() >= 250 {
                        let elapsed_secs = start_time.elapsed().as_secs_f64();
                        let speed = if elapsed_secs > 0.05 {
                            (current_downloaded.saturating_sub(local_offset) as f64 / elapsed_secs)
                                as u64
                        } else {
                            0
                        };

                        let eta = if speed > 0 && total_bytes > current_downloaded {
                            (total_bytes - current_downloaded) / speed
                        } else {
                            0
                        };

                        on_progress(ProgressUpdate {
                            downloaded_bytes: current_downloaded,
                            total_bytes,
                            speed_bytes_per_sec: speed,
                            eta_seconds: eta,
                        });

                        last_progress_report = Instant::now();
                    }
                }
                Err(e) => {
                    stream_failed = true;
                    last_stream_err = Some(e);
                    break;
                }
            }
        }

        let _ = writer.flush().await;
        drop(writer);

        if stream_failed {
            let e = last_stream_err.unwrap();
            let err_kind = classify_reqwest_error(&e);

            if err_kind.is_transient() && attempt < max_retries {
                attempt += 1;
                let delay_ms = (1000u64 << (attempt - 1)).min(16000) + (attempt as u64 * 83 % 200);
                on_status_change(
                    DownloadStatus::Retrying,
                    &format!(
                        "Stream interrupted ({}) — retrying from {} MB (attempt {attempt}/{max_retries})...",
                        err_kind.as_str(),
                        current_downloaded / (1024 * 1024)
                    ),
                );
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                continue;
            } else {
                let diag = DownloadDiagnostics {
                    safe_url,
                    host,
                    status: None,
                    requested_range: Some(format!("bytes={current_downloaded}-")),
                    current_offset: current_downloaded,
                    total_bytes,
                    downloaded_bytes: current_downloaded,
                    retry_count: attempt,
                    error_kind: err_kind,
                    details: format!("Stream interrupted: {e}"),
                };
                return Err(FreeFetchError::Network(diag.to_string()));
            }
        }

        on_progress(ProgressUpdate {
            downloaded_bytes: current_downloaded,
            total_bytes: if total_bytes == 0 {
                current_downloaded
            } else {
                total_bytes
            },
            speed_bytes_per_sec: 0,
            eta_seconds: 0,
        });

        return Ok(current_downloaded);
    }
}
