use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use url::Url;
use uuid::Uuid;

use crate::core::config::AppSettings;
use crate::core::error::FreeFetchError;
use crate::core::types::{QueueItem, QueueStatus, QueueSummary};
use crate::engine::state::ensure_safe_destination_path_with_reserved;
use crate::engine::template::{FilenameTemplateEngine, TemplateContext};
use crate::engine::{EngineManager, StartDownloadParams};
use crate::extractor::implementations::youtube::extract_youtube_id;
use crate::media::audio_metadata::AudioMetadata;
use crate::storage::Repository;

pub type QueueEventSink = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;

#[derive(Clone)]
pub struct QueueManager {
    repo: Repository,
    engine: EngineManager,
    max_concurrency: Arc<AtomicUsize>,
    is_running: Arc<AtomicBool>,
    active_jobs: Arc<Mutex<HashSet<String>>>,
    event_sink: Arc<Mutex<Option<QueueEventSink>>>,
    settings: Arc<Mutex<AppSettings>>,
}

impl QueueManager {
    pub fn new(repo: Repository, engine: EngineManager, settings: Arc<Mutex<AppSettings>>) -> Self {
        let initial_concurrency = {
            let s = settings.lock().unwrap();
            s.max_concurrent_downloads.clamp(1, 5)
        };

        let qm = Self {
            repo,
            engine: engine.clone(),
            max_concurrency: Arc::new(AtomicUsize::new(initial_concurrency)),
            is_running: Arc::new(AtomicBool::new(true)),
            active_jobs: Arc::new(Mutex::new(HashSet::new())),
            event_sink: Arc::new(Mutex::new(None)),
            settings,
        };

        // Wire up state listener from EngineManager
        let qm_clone = qm.clone();
        engine.add_state_listener(Arc::new(move |id, status, err_msg| {
            let qm_inner = qm_clone.clone();
            let job_id = id.to_string();
            let st = status.to_string();
            let err = err_msg.map(|s| s.to_string());

            qm_clone.engine.spawn_task(async move {
                qm_inner
                    .handle_engine_state_change(&job_id, &st, err.as_deref())
                    .await;
            });
        }));

        qm
    }

    pub fn set_event_sink(&self, sink: QueueEventSink) {
        let mut s = self.event_sink.lock().unwrap();
        *s = Some(sink);
    }

    fn emit_event<T: serde::Serialize>(&self, event: &str, payload: &T) {
        let sink_opt = {
            let s = self.event_sink.lock().unwrap();
            s.clone()
        };
        if let Some(sink) = sink_opt {
            if let Ok(val) = serde_json::to_value(payload) {
                sink(event, val);
            }
        }
    }

    pub fn canonicalize_url(raw_url: &str) -> String {
        if let Ok(parsed) = Url::parse(raw_url) {
            if let Some(yt_id) = extract_youtube_id(&parsed) {
                return format!("https://www.youtube.com/watch?v={yt_id}");
            }
            let mut clean = parsed.clone();
            clean.set_fragment(None);
            clean.to_string()
        } else {
            raw_url.trim().to_string()
        }
    }

    pub async fn add(&self, params: StartDownloadParams) -> Result<QueueItem, FreeFetchError> {
        let canonical_url = Self::canonicalize_url(&params.source_url);

        let effective_format_id = if let Some(ref opts) = params.audio_options {
            if params.format_id.contains(':') {
                params.format_id.clone()
            } else {
                format!("{}:{}", params.format_id, opts.target_format.extension())
            }
        } else {
            params.format_id.clone()
        };

        // Duplicate detection: check if existing item with same canonical URL and format is active/completed
        if let Ok(Some(existing)) = self
            .repo
            .get_queue_item_by_canonical(&canonical_url, &effective_format_id)
        {
            if matches!(
                existing.status,
                QueueStatus::Queued
                    | QueueStatus::Preparing
                    | QueueStatus::Downloading
                    | QueueStatus::Paused
                    | QueueStatus::Processing
                    | QueueStatus::Completed
            ) {
                return Err(FreeFetchError::Generic(format!(
                    "Item is already in queue or completed: {} ({}) with status {:?}",
                    existing.title, existing.format_id, existing.status
                )));
            }
        }

        let max_pos = self.repo.get_max_queue_position().unwrap_or(-1);
        let next_pos = max_pos + 1;
        let id = Uuid::new_v4().to_string();

        let dest_dir = if let Some(ref d) = params.destination_dir {
            PathBuf::from(d)
        } else {
            let s = self.settings.lock().unwrap();
            PathBuf::from(&s.download_directory)
        };
        let reserved = self
            .repo
            .get_active_destination_paths(None)
            .unwrap_or_default();
        let final_dest = if let Some(ref opts) = params.audio_options {
            let (clean_title, artist) = AudioMetadata::parse_artist_and_title(&params.title, None);
            let ctx = TemplateContext {
                title: clean_title,
                artist,
                album: None,
                date: None,
                id: None,
                uploader: None,
                ext: opts.target_format.extension().to_string(),
            };
            FilenameTemplateEngine::render_destination_with_reserved(
                &dest_dir,
                opts.filename_template.as_deref(),
                &ctx,
                &reserved,
            )
        } else {
            ensure_safe_destination_path_with_reserved(
                &dest_dir,
                &params.title,
                &params.container,
                &reserved,
            )
        };
        let mut final_params = params.clone();
        final_params.format_id = effective_format_id.clone();
        if final_params.format_id.starts_with("sub:") {
            // NEVER persist signed/ephemeral YouTube timedtext URLs in SQLite
            final_params.download_url = String::new();
        }
        let params_json = serde_json::to_string(&final_params)
            .map_err(|e| FreeFetchError::Generic(e.to_string()))?;

        let now = chrono::Utc::now().timestamp();
        let item = QueueItem {
            id: id.clone(),
            url: params.source_url.clone(),
            canonical_url,
            title: params.title.clone(),
            thumbnail: params.thumbnail_url.clone(),
            status: QueueStatus::Queued,
            selected_format: if params.format_id.starts_with("sub:") {
                format!(
                    "{} Subtitle ({})",
                    params.container.to_uppercase(),
                    params.quality_label
                )
            } else {
                format!(
                    "{} ({})",
                    params.quality_label,
                    params.container.to_uppercase()
                )
            },
            format_id: effective_format_id,
            output_path: final_dest.to_string_lossy().to_string(),
            file_size: 0,
            downloaded_bytes: 0,
            progress: 0.0,
            speed: 0,
            eta: 0,
            error_code: None,
            error_message: None,
            retry_count: 0,
            created_at: now,
            started_at: None,
            completed_at: None,
            updated_at: now,
            position: next_pos,
            params_json,
        };

        self.repo.insert_queue_item(&item)?;

        self.emit_event("queue:item-added", &item);
        self.emit_event("queue:updated", &self.get_items().unwrap_or_default());
        self.emit_event("queue:summary", &self.get_summary().unwrap_or_default());

        self.process_next().await;

        Ok(item)
    }

    pub async fn add_with_subtitles(
        &self,
        media_params: StartDownloadParams,
        subtitles: Vec<crate::core::types::SubtitleSelection>,
    ) -> Result<Vec<QueueItem>, FreeFetchError> {
        let mut created = Vec::new();

        // 1. Add main media item
        let media_item = self.add(media_params.clone()).await?;
        created.push(media_item);

        // 2. Add each selected subtitle track if not embedding or hardcoding
        if media_params.resolved_subtitle_mode() == crate::core::types::SubtitleMode::None {
            for sub in subtitles {
                let sub_format_id = format!(
                    "sub:{}:{}{}",
                    sub.language_code,
                    if sub.is_auto_generated { "auto:" } else { "" },
                    sub.target_format.extension()
                );

                let sub_title = format!(
                    "{}.{}{}",
                    media_params.title,
                    sub.language_code,
                    if sub.is_auto_generated { ".auto" } else { "" }
                );

                let sub_params = StartDownloadParams {
                    source_url: media_params.source_url.clone(),
                    service_name: media_params.service_name.clone(),
                    title: sub_title,
                    duration: media_params.duration,
                    thumbnail_url: media_params.thumbnail_url.clone(),
                    format_id: sub_format_id,
                    download_url: String::new(), // Ephemeral: do NOT persist temporary timedtext URL
                    requires_merge: false,
                    matching_audio_url: None,
                    container: sub.target_format.extension().to_string(),
                    quality_label: sub.language_name,
                    destination_dir: media_params.destination_dir.clone(),
                    selected_subtitles: Vec::new(),
                    embed_subtitles: false,
                    subtitle_mode: None,
                    audio_options: None,
                    video_codec: None,
                    audio_codec: None,
                    http_headers: None,
                };

                // Individual subtitle failure does not stop adding other subtitles or media
                if let Ok(sub_item) = self.add(sub_params).await {
                    created.push(sub_item);
                }
            }
        }

        Ok(created)
    }

    pub async fn remove(&self, id: &str) -> Result<(), FreeFetchError> {
        let is_active = {
            let mut active = self.active_jobs.lock().unwrap();
            active.remove(id)
        };

        if is_active {
            let _ = self.engine.cancel_download(id);
        }

        self.repo.delete_queue_item(id)?;
        self.recompact_positions()?;

        self.emit_event("queue:item-removed", &serde_json::json!({ "id": id }));
        self.emit_event("queue:updated", &self.get_items().unwrap_or_default());
        self.emit_event("queue:summary", &self.get_summary().unwrap_or_default());

        self.process_next().await;

        Ok(())
    }

    pub async fn start_all(&self) -> Result<(), FreeFetchError> {
        self.is_running.store(true, Ordering::SeqCst);
        self.process_next().await;
        Ok(())
    }

    pub async fn pause_all(&self) -> Result<(), FreeFetchError> {
        self.is_running.store(false, Ordering::SeqCst);
        let active_ids: Vec<String> = {
            let active = self.active_jobs.lock().unwrap();
            active.iter().cloned().collect()
        };

        for id in active_ids {
            let _ = self.engine.pause_download(&id);
            let _ = self.repo.update_queue_item_status(
                &id,
                &QueueStatus::Paused,
                None,
                None,
                None,
                None,
            );
        }

        {
            let mut active = self.active_jobs.lock().unwrap();
            active.clear();
        }

        self.emit_event("queue:updated", &self.get_items().unwrap_or_default());
        self.emit_event("queue:summary", &self.get_summary().unwrap_or_default());

        Ok(())
    }

    pub async fn resume_all(&self) -> Result<(), FreeFetchError> {
        self.is_running.store(true, Ordering::SeqCst);
        let items = self.repo.list_queue_items()?;
        for item in items {
            if item.status == QueueStatus::Paused {
                let _ = self.repo.update_queue_item_status(
                    &item.id,
                    &QueueStatus::Queued,
                    None,
                    None,
                    None,
                    None,
                );
            }
        }

        self.emit_event("queue:updated", &self.get_items().unwrap_or_default());
        self.emit_event("queue:summary", &self.get_summary().unwrap_or_default());

        self.process_next().await;
        Ok(())
    }

    pub async fn cancel_all(&self) -> Result<(), FreeFetchError> {
        self.is_running.store(false, Ordering::SeqCst);
        let active_ids: Vec<String> = {
            let active = self.active_jobs.lock().unwrap();
            active.iter().cloned().collect()
        };

        for id in active_ids {
            let _ = self.engine.cancel_download(&id);
            let _ = self.repo.update_queue_item_status(
                &id,
                &QueueStatus::Cancelled,
                None,
                None,
                None,
                None,
            );
        }

        {
            let mut active = self.active_jobs.lock().unwrap();
            active.clear();
        }

        let items = self.repo.list_queue_items()?;
        for item in items {
            if matches!(
                item.status,
                QueueStatus::Queued | QueueStatus::Preparing | QueueStatus::Paused
            ) {
                let _ = self.repo.update_queue_item_status(
                    &item.id,
                    &QueueStatus::Cancelled,
                    None,
                    None,
                    None,
                    None,
                );
            }
        }

        self.emit_event("queue:updated", &self.get_items().unwrap_or_default());
        self.emit_event("queue:summary", &self.get_summary().unwrap_or_default());

        Ok(())
    }

    pub async fn pause_item(&self, id: &str) -> Result<(), FreeFetchError> {
        let is_active = {
            let mut active = self.active_jobs.lock().unwrap();
            active.remove(id)
        };

        if is_active {
            let _ = self.engine.pause_download(id);
        }

        self.repo
            .update_queue_item_status(id, &QueueStatus::Paused, None, None, None, None)?;

        if let Ok(Some(item)) = self.repo.get_queue_item(id) {
            self.emit_event("queue:item-updated", &item);
        }
        self.emit_event("queue:summary", &self.get_summary().unwrap_or_default());

        self.process_next().await;
        Ok(())
    }

    pub async fn resume_item(&self, id: &str) -> Result<(), FreeFetchError> {
        let item = self
            .repo
            .get_queue_item(id)?
            .ok_or_else(|| FreeFetchError::NotFound(id.to_string()))?;

        if !item.status.can_transition_to(&QueueStatus::Queued) {
            return Err(FreeFetchError::Generic(format!(
                "Cannot resume item in state {:?}",
                item.status
            )));
        }

        self.repo
            .update_queue_item_status(id, &QueueStatus::Queued, None, None, None, None)?;

        self.is_running.store(true, Ordering::SeqCst);

        if let Ok(Some(updated)) = self.repo.get_queue_item(id) {
            self.emit_event("queue:item-updated", &updated);
        }
        self.emit_event("queue:summary", &self.get_summary().unwrap_or_default());

        self.process_next().await;
        Ok(())
    }

    pub async fn cancel_item(&self, id: &str) -> Result<(), FreeFetchError> {
        let is_active = {
            let mut active = self.active_jobs.lock().unwrap();
            active.remove(id)
        };

        if is_active {
            let _ = self.engine.cancel_download(id);
        }

        self.repo
            .update_queue_item_status(id, &QueueStatus::Cancelled, None, None, None, None)?;

        if let Ok(Some(updated)) = self.repo.get_queue_item(id) {
            self.emit_event("queue:item-updated", &updated);
        }
        self.emit_event("queue:summary", &self.get_summary().unwrap_or_default());

        self.process_next().await;
        Ok(())
    }

    pub async fn retry_item(&self, id: &str) -> Result<(), FreeFetchError> {
        let item = self
            .repo
            .get_queue_item(id)?
            .ok_or_else(|| FreeFetchError::NotFound(id.to_string()))?;

        if !matches!(item.status, QueueStatus::Failed | QueueStatus::Cancelled) {
            return Err(FreeFetchError::Generic(format!(
                "Cannot retry item with status {:?}",
                item.status
            )));
        }

        let mut updated = item.clone();
        updated.status = QueueStatus::Queued;
        updated.retry_count += 1;
        updated.error_code = None;
        updated.error_message = None;
        updated.downloaded_bytes = 0;
        updated.progress = 0.0;
        updated.speed = 0;
        updated.eta = 0;
        updated.updated_at = chrono::Utc::now().timestamp();

        self.repo.update_queue_item(&updated)?;
        self.is_running.store(true, Ordering::SeqCst);

        self.emit_event("queue:item-updated", &updated);
        self.emit_event("queue:summary", &self.get_summary().unwrap_or_default());

        self.process_next().await;
        Ok(())
    }

    pub async fn clear_completed(&self) -> Result<(), FreeFetchError> {
        self.repo.clear_completed_queue_items()?;
        self.recompact_positions()?;

        self.emit_event("queue:updated", &self.get_items().unwrap_or_default());
        self.emit_event("queue:summary", &self.get_summary().unwrap_or_default());
        Ok(())
    }

    pub async fn clear_failed(&self) -> Result<(), FreeFetchError> {
        self.repo.clear_failed_queue_items()?;
        self.recompact_positions()?;

        self.emit_event("queue:updated", &self.get_items().unwrap_or_default());
        self.emit_event("queue:summary", &self.get_summary().unwrap_or_default());
        Ok(())
    }

    pub async fn move_up(&self, id: &str) -> Result<(), FreeFetchError> {
        let items = self.repo.list_queue_items()?;
        let idx = items
            .iter()
            .position(|i| i.id == id)
            .ok_or_else(|| FreeFetchError::NotFound(format!("Queue item {id} not found")))?;

        if idx == 0 {
            return Ok(()); // Already at top
        }

        let curr = &items[idx];
        let prev = &items[idx - 1];

        // Do not allow reordering actively running items
        if curr.status == QueueStatus::Downloading || prev.status == QueueStatus::Downloading {
            return Err(FreeFetchError::Generic(
                "Cannot reorder active downloading items".to_string(),
            ));
        }

        let positions = vec![
            (curr.id.clone(), prev.position),
            (prev.id.clone(), curr.position),
        ];

        self.repo.update_queue_positions(&positions)?;
        self.recompact_positions()?;

        self.emit_event("queue:updated", &self.get_items().unwrap_or_default());

        Ok(())
    }

    pub async fn move_down(&self, id: &str) -> Result<(), FreeFetchError> {
        let items = self.repo.list_queue_items()?;
        let idx = items
            .iter()
            .position(|i| i.id == id)
            .ok_or_else(|| FreeFetchError::NotFound(format!("Queue item {id} not found")))?;

        if idx + 1 >= items.len() {
            return Ok(()); // Already at bottom
        }

        let curr = &items[idx];
        let next = &items[idx + 1];

        // Do not allow reordering actively running items
        if curr.status == QueueStatus::Downloading || next.status == QueueStatus::Downloading {
            return Err(FreeFetchError::Generic(
                "Cannot reorder active downloading items".to_string(),
            ));
        }

        let positions = vec![
            (curr.id.clone(), next.position),
            (next.id.clone(), curr.position),
        ];

        self.repo.update_queue_positions(&positions)?;
        self.recompact_positions()?;

        self.emit_event("queue:updated", &self.get_items().unwrap_or_default());

        Ok(())
    }

    pub async fn reorder(&self, id: &str, target_position: i32) -> Result<(), FreeFetchError> {
        let mut items = self.repo.list_queue_items()?;
        let idx = items
            .iter()
            .position(|i| i.id == id)
            .ok_or_else(|| FreeFetchError::NotFound(format!("Queue item {id} not found")))?;

        let item = items.remove(idx);
        if item.status == QueueStatus::Downloading {
            return Err(FreeFetchError::Generic(
                "Cannot reorder actively downloading item".to_string(),
            ));
        }

        let clamped_pos = (target_position as usize).min(items.len());
        items.insert(clamped_pos, item);

        let positions: Vec<(String, i32)> = items
            .iter()
            .enumerate()
            .map(|(pos, i)| (i.id.clone(), pos as i32))
            .collect();

        self.repo.update_queue_positions(&positions)?;

        self.emit_event("queue:updated", &self.get_items().unwrap_or_default());

        Ok(())
    }

    fn recompact_positions(&self) -> Result<(), FreeFetchError> {
        let items = self.repo.list_queue_items()?;
        let positions: Vec<(String, i32)> = items
            .iter()
            .enumerate()
            .map(|(pos, i)| (i.id.clone(), pos as i32))
            .collect();
        self.repo.update_queue_positions(&positions)?;
        Ok(())
    }

    pub fn set_concurrency(&self, limit: usize) -> Result<(), FreeFetchError> {
        let clamped = limit.clamp(1, 5);
        self.max_concurrency.store(clamped, Ordering::SeqCst);

        // Update settings
        {
            let mut s = self.settings.lock().unwrap();
            s.max_concurrent_downloads = clamped;
            let serialized =
                serde_json::to_string(&*s).map_err(|e| FreeFetchError::Generic(e.to_string()))?;
            let _ = self.repo.set_setting("app_settings", &serialized);
        }

        let qm = self.clone();
        self.engine.spawn_task(async move {
            qm.process_next().await;
        });

        Ok(())
    }

    pub fn get_concurrency(&self) -> usize {
        self.max_concurrency.load(Ordering::SeqCst)
    }

    pub fn get_items(&self) -> Result<Vec<QueueItem>, FreeFetchError> {
        self.repo.list_queue_items()
    }

    pub fn get_summary(&self) -> Result<QueueSummary, FreeFetchError> {
        let items = self.repo.list_queue_items()?;
        let total_items = items.len();
        let mut queued_count = 0;
        let mut downloading_count = 0;
        let mut paused_count = 0;
        let mut completed_count = 0;
        let mut failed_count = 0;
        let mut total_bytes = 0u64;
        let mut downloaded_bytes = 0u64;
        let mut total_speed = 0u64;

        for item in &items {
            match item.status {
                QueueStatus::Queued | QueueStatus::Preparing => queued_count += 1,
                QueueStatus::Downloading | QueueStatus::Processing | QueueStatus::Retrying => {
                    downloading_count += 1;
                    total_speed += item.speed;
                }
                QueueStatus::Paused => paused_count += 1,
                QueueStatus::Completed => completed_count += 1,
                QueueStatus::Failed | QueueStatus::Cancelled => failed_count += 1,
            }

            total_bytes += item.file_size;
            downloaded_bytes += item.downloaded_bytes;
        }

        let overall_progress_percent = if total_bytes > 0 {
            ((downloaded_bytes as f64 / total_bytes as f64) * 100.0).clamp(0.0, 100.0)
        } else if total_items > 0 {
            ((completed_count as f64 / total_items as f64) * 100.0).clamp(0.0, 100.0)
        } else {
            0.0
        };

        Ok(QueueSummary {
            total_items,
            queued_count,
            downloading_count,
            paused_count,
            completed_count,
            failed_count,
            total_bytes,
            downloaded_bytes,
            overall_progress_percent,
            total_speed_bytes_per_sec: total_speed,
        })
    }

    pub async fn process_next(&self) {
        if !self.is_running.load(Ordering::SeqCst) {
            return;
        }

        let max = self.max_concurrency.load(Ordering::SeqCst);
        let active_count = {
            let active = self.active_jobs.lock().unwrap();
            active.len()
        };

        if active_count >= max {
            return;
        }

        let available_slots = max - active_count;
        let next_items = match self.repo.get_next_queued_items(available_slots) {
            Ok(items) => items,
            Err(_) => return,
        };

        if next_items.is_empty() {
            return;
        }

        let settings = {
            let s = self.settings.lock().unwrap();
            s.clone()
        };

        for item in next_items {
            {
                let mut active = self.active_jobs.lock().unwrap();
                if active.len() >= max {
                    break;
                }
                active.insert(item.id.clone());
            }

            let start_time = chrono::Utc::now().timestamp();
            let _ = self.repo.update_queue_item_status(
                &item.id,
                &QueueStatus::Preparing,
                None,
                None,
                Some(start_time),
                None,
            );

            if let Ok(Some(up)) = self.repo.get_queue_item(&item.id) {
                self.emit_event("queue:item-updated", &up);
            }
            self.emit_event("queue:summary", &self.get_summary().unwrap_or_default());

            let params: StartDownloadParams = match serde_json::from_str(&item.params_json) {
                Ok(p) => p,
                Err(e) => {
                    let _ = self.repo.update_queue_item_status(
                        &item.id,
                        &QueueStatus::Failed,
                        Some("INVALID_PARAMS"),
                        Some(&e.to_string()),
                        None,
                        None,
                    );
                    {
                        let mut active = self.active_jobs.lock().unwrap();
                        active.remove(&item.id);
                    }
                    continue;
                }
            };

            let qm = self.clone();
            let engine = self.engine.clone();
            let item_id = item.id.clone();
            let settings_clone = settings.clone();

            self.engine.spawn_task(async move {
                if let Err(e) = engine
                    .start_download_with_id(item_id.clone(), params, &settings_clone)
                    .await
                {
                    qm.handle_init_failure(&item_id, &e.to_string());
                }
            });
        }
    }

    pub fn handle_init_failure(&self, item_id: &str, err_msg: &str) {
        let now = chrono::Utc::now().timestamp();
        let _ = self.repo.update_queue_item_status(
            item_id,
            &QueueStatus::Failed,
            Some("INIT_FAILED"),
            Some(err_msg),
            None,
            Some(now),
        );

        {
            let mut active = self.active_jobs.lock().unwrap();
            active.remove(item_id);
        }

        if let Ok(Some(updated)) = self.repo.get_queue_item(item_id) {
            self.emit_event("queue:item-updated", &updated);
        }
        self.emit_event("queue:summary", &self.get_summary().unwrap_or_default());

        let qm = self.clone();
        self.engine.spawn_task(async move {
            qm.process_next().await;
        });
    }

    pub async fn handle_engine_state_change(
        &self,
        job_id: &str,
        status: &str,
        error_message: Option<&str>,
    ) {
        let existing = match self.repo.get_queue_item(job_id) {
            Ok(Some(item)) => item,
            _ => return, // Not a queue-managed job
        };

        let target_status = match status {
            "completed" => QueueStatus::Completed,
            "paused" => QueueStatus::Paused,
            "cancelled" => QueueStatus::Cancelled,
            "failed" => QueueStatus::Failed,
            "downloading" => QueueStatus::Downloading,
            "processing" | "remuxing" => QueueStatus::Processing,
            "retrying" => QueueStatus::Retrying,
            _ => return,
        };

        // Enforce valid state transitions (P1-3)
        // Late worker network errors must not overwrite Cancelled or Completed.
        // Stale worker progress must not overwrite Paused.
        if !existing.status.can_transition_to(&target_status)
            || (existing.status == QueueStatus::Paused
                && matches!(
                    target_status,
                    QueueStatus::Downloading | QueueStatus::Failed | QueueStatus::Retrying
                ))
        {
            return;
        }

        let now = chrono::Utc::now().timestamp();
        let completed_time = if target_status == QueueStatus::Completed {
            Some(now)
        } else {
            existing.completed_at
        };

        let is_terminal = matches!(
            target_status,
            QueueStatus::Completed
                | QueueStatus::Failed
                | QueueStatus::Cancelled
                | QueueStatus::Paused
        );

        if is_terminal {
            let mut active = self.active_jobs.lock().unwrap();
            active.remove(job_id);
        }

        let mut updated = existing.clone();
        updated.status = target_status;
        updated.error_message = error_message.map(|s| s.to_string());
        if updated.status == QueueStatus::Completed {
            updated.progress = 100.0;
            updated.completed_at = completed_time;
        }
        updated.updated_at = now;

        let _ = self.repo.update_queue_item(&updated);

        self.emit_event("queue:item-updated", &updated);
        self.emit_event("queue:summary", &self.get_summary().unwrap_or_default());

        // When a job completes, fails, pauses, or cancels, process the next item
        if is_terminal {
            self.process_next().await;
        }
    }

    pub fn handle_progress_update(
        &self,
        id: &str,
        downloaded: u64,
        total: u64,
        progress: f64,
        speed: u64,
        eta: u64,
    ) {
        if let Ok(Some(item)) = self.repo.get_queue_item(id) {
            if matches!(
                item.status,
                QueueStatus::Downloading | QueueStatus::Retrying | QueueStatus::Processing
            ) {
                let _ = self
                    .repo
                    .update_queue_item_progress(id, downloaded, total, progress, speed, eta);
            }
        }
    }
}
