use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Semaphore;
use url::Url;

use crate::core::error::FreeFetchError;
use crate::core::types::{
    BatchAnalysisItem, BatchAnalysisResult, BatchItemStatus, BatchQueueResult,
};
use crate::engine::queue::QueueManager;
use crate::engine::StartDownloadParams;
use crate::extractor::{ExtractionContext, ExtractorRegistry};

/// Parses multi-line pasted text into a clean, deduplicated list of valid URLs.
///
/// Handles:
/// - Windows CRLF (`\r\n`) and Unix LF (`\n`)
/// - Whitespace trimming
/// - Blank lines and non-URL garbage removal
/// - Canonical URL deduplication (e.g., `youtu.be/ID` vs `youtube.com/watch?v=ID`)
/// - Preserves first-seen input ordering
pub fn parse_batch_urls(raw_text: &str) -> Vec<String> {
    let mut seen_canonical = HashSet::new();
    let mut result = Vec::new();

    for line in raw_text.lines() {
        // Split on whitespace in case multiple URLs are on one line
        for token in line.split_whitespace() {
            let trimmed = token.trim();
            if trimmed.is_empty() {
                continue;
            }

            // Must look like an HTTP/HTTPS URL
            if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
                continue;
            }

            if let Ok(parsed) = Url::parse(trimmed) {
                if parsed.scheme() != "http" && parsed.scheme() != "https" {
                    continue;
                }
                if parsed.host_str().is_none() {
                    continue;
                }

                // Canonicalize URL to eliminate duplicates
                let canonical = QueueManager::canonicalize_url(trimmed);
                if seen_canonical.insert(canonical) {
                    result.push(trimmed.to_string());
                }
            }
        }
    }

    result
}

/// Analyzes multiple URLs asynchronously with bounded concurrency.
///
/// A single failed URL will never fail or abort other URLs in the batch.
pub async fn analyze_batch_urls(
    registry: &Arc<ExtractorRegistry>,
    urls: &[String],
    max_concurrency: usize,
) -> BatchAnalysisResult {
    let concurrency = max_concurrency.clamp(1, 5);
    let semaphore = Arc::new(Semaphore::new(concurrency));

    let mut tasks = Vec::with_capacity(urls.len());

    for url_str in urls {
        let sem = semaphore.clone();
        let reg = registry.clone();
        let raw_url = url_str.clone();

        let task = tokio::spawn(async move {
            let _permit = match sem.acquire().await {
                Ok(p) => p,
                Err(_) => {
                    return BatchAnalysisItem {
                        original_url: raw_url.clone(),
                        canonical_url: QueueManager::canonicalize_url(&raw_url),
                        status: BatchItemStatus::Failed,
                        metadata: None,
                        formats: Vec::new(),
                        subtitles: Vec::new(),
                        error_message: Some("Analysis permit acquisition failed".to_string()),
                    }
                }
            };

            let canonical = QueueManager::canonicalize_url(&raw_url);

            let parsed_url = match Url::parse(&raw_url) {
                Ok(u) => u,
                Err(e) => {
                    return BatchAnalysisItem {
                        original_url: raw_url,
                        canonical_url: canonical,
                        status: BatchItemStatus::Failed,
                        metadata: None,
                        formats: Vec::new(),
                        subtitles: Vec::new(),
                        error_message: Some(format!("Invalid URL syntax: {e}")),
                    };
                }
            };

            let extractor = match reg.find_extractor(&parsed_url) {
                Some(ext) => ext,
                None => {
                    return BatchAnalysisItem {
                        original_url: raw_url,
                        canonical_url: canonical,
                        status: BatchItemStatus::Failed,
                        metadata: None,
                        formats: Vec::new(),
                        subtitles: Vec::new(),
                        error_message: Some(
                            "No compatible extractor found for this URL".to_string(),
                        ),
                    };
                }
            };

            let ctx = ExtractionContext::default();
            match extractor.extract(&parsed_url, &ctx).await {
                Ok((meta, manifest)) => BatchAnalysisItem {
                    original_url: raw_url,
                    canonical_url: canonical,
                    status: BatchItemStatus::Ready,
                    metadata: Some(meta),
                    formats: manifest.formats,
                    subtitles: manifest.subtitles,
                    error_message: None,
                },
                Err(e) => BatchAnalysisItem {
                    original_url: raw_url,
                    canonical_url: canonical,
                    status: BatchItemStatus::Failed,
                    metadata: None,
                    formats: Vec::new(),
                    subtitles: Vec::new(),
                    error_message: Some(e.to_string()),
                },
            }
        });

        tasks.push(task);
    }

    let mut items = Vec::with_capacity(tasks.len());
    let mut ready_count = 0;
    let mut failed_count = 0;

    for task in tasks {
        match task.await {
            Ok(item) => {
                if item.status == BatchItemStatus::Ready {
                    ready_count += 1;
                } else {
                    failed_count += 1;
                }
                items.push(item);
            }
            Err(e) => {
                failed_count += 1;
                items.push(BatchAnalysisItem {
                    original_url: "unknown".to_string(),
                    canonical_url: "unknown".to_string(),
                    status: BatchItemStatus::Failed,
                    metadata: None,
                    formats: Vec::new(),
                    subtitles: Vec::new(),
                    error_message: Some(format!("Task panic: {e}")),
                });
            }
        }
    }

    BatchAnalysisResult {
        total: items.len(),
        ready_count,
        failed_count,
        items,
    }
}

/// Adds multiple analyzed items to the QueueManager preserving input order.
pub async fn add_batch_to_queue(
    queue: &QueueManager,
    items: Vec<StartDownloadParams>,
) -> Result<BatchQueueResult, FreeFetchError> {
    let mut added_count = 0;
    let mut duplicate_count = 0;
    let mut failed_count = 0;
    let mut added_ids = Vec::new();
    let mut errors = Vec::new();

    for params in items {
        let subs = params.selected_subtitles.clone();
        if !subs.is_empty()
            && params.resolved_subtitle_mode() == crate::core::types::SubtitleMode::None
        {
            match queue.add_with_subtitles(params, subs).await {
                Ok(queue_items) => {
                    for qi in queue_items {
                        added_count += 1;
                        added_ids.push(qi.id);
                    }
                }
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("already in queue") || msg.contains("Duplicate") {
                        duplicate_count += 1;
                    } else {
                        failed_count += 1;
                        errors.push(msg);
                    }
                }
            }
        } else {
            match queue.add(params).await {
                Ok(queue_item) => {
                    added_count += 1;
                    added_ids.push(queue_item.id);
                }
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("already in queue") || msg.contains("Duplicate") {
                        duplicate_count += 1;
                    } else {
                        failed_count += 1;
                        errors.push(msg);
                    }
                }
            }
        }
    }

    Ok(BatchQueueResult {
        added_count,
        duplicate_count,
        failed_count,
        unavailable_count: 0,
        added_ids,
        errors,
    })
}
