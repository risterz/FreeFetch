pub mod chunk_downloader;
pub mod parallel_downloader;
pub mod queue;
pub mod rate_limiter;
pub mod speed_tracker;
pub mod state;
pub mod subtitle_downloader;
pub mod template;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;
use tokio::sync::watch;
use url::Url;
use uuid::Uuid;

static FALLBACK_RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

fn get_fallback_runtime() -> &'static tokio::runtime::Runtime {
    FALLBACK_RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .thread_name("freefetch-bg")
            .build()
            .expect("Failed to initialize background Tokio runtime")
    })
}

use crate::core::config::AppSettings;
use crate::core::error::FreeFetchError;
use crate::core::types::{
    AudioConversionOptions, AudioQualityPreset, AudioTargetFormat, BitrateMode, DownloadRecord,
    DownloadStatus, StreamRecord, SubtitleSelection,
};
use crate::extractor::{ExtractionContext, ExtractorRegistry, MediaMetadata};
use crate::media::artwork::ArtworkDownloader;
use crate::media::audio_metadata::AudioMetadata;
use crate::media::ffmpeg::FfmpegController;
use crate::storage::Repository;
use chunk_downloader::{ProgressUpdate, UrlRefresher};
use rate_limiter::RateLimiter;
use speed_tracker::SpeedTracker;
use state::{ensure_safe_destination_path_with_reserved, validate_download_url};
use template::{FilenameTemplateEngine, TemplateContext};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartDownloadParams {
    pub source_url: String,
    pub service_name: String,
    pub title: String,
    pub duration: u64,
    pub thumbnail_url: Option<String>,
    pub format_id: String,
    pub download_url: String,
    pub requires_merge: bool,
    pub matching_audio_url: Option<String>,
    pub container: String,
    pub quality_label: String,
    pub destination_dir: Option<String>,
    #[serde(default)]
    pub selected_subtitles: Vec<SubtitleSelection>,
    #[serde(default)]
    pub embed_subtitles: bool,
    #[serde(default)]
    pub subtitle_mode: Option<crate::core::types::SubtitleMode>,
    #[serde(default)]
    pub audio_options: Option<AudioConversionOptions>,
    #[serde(default)]
    pub video_codec: Option<String>,
    #[serde(default)]
    pub audio_codec: Option<String>,
    #[serde(default)]
    pub http_headers: Option<std::collections::HashMap<String, String>>,
}

impl StartDownloadParams {
    pub fn resolved_subtitle_mode(&self) -> crate::core::types::SubtitleMode {
        if let Some(mode) = self.subtitle_mode {
            mode
        } else if self.embed_subtitles {
            crate::core::types::SubtitleMode::Embed
        } else {
            crate::core::types::SubtitleMode::None
        }
    }
}

impl Default for StartDownloadParams {
    fn default() -> Self {
        Self {
            source_url: String::new(),
            service_name: String::new(),
            title: String::new(),
            duration: 0,
            thumbnail_url: None,
            format_id: String::new(),
            download_url: String::new(),
            requires_merge: false,
            matching_audio_url: None,
            container: "mp4".to_string(),
            quality_label: String::new(),
            destination_dir: None,
            selected_subtitles: Vec::new(),
            embed_subtitles: false,
            subtitle_mode: None,
            audio_options: None,
            video_codec: None,
            audio_codec: None,
            http_headers: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgressPayload {
    pub id: String,
    #[serde(rename = "downloadedBytes")]
    pub downloaded_bytes: u64,
    #[serde(rename = "totalBytes")]
    pub total_bytes: u64,
    #[serde(rename = "speedBytesPerSec")]
    pub speed_bytes_per_sec: u64,
    #[serde(rename = "etaSeconds")]
    pub eta_seconds: u64,
    #[serde(rename = "progressPercent")]
    pub progress_percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadStateChangedPayload {
    pub id: String,
    pub status: String,
    #[serde(rename = "errorMessage")]
    pub error_message: Option<String>,
}

pub type StateChangeListener = Arc<dyn Fn(&str, &str, Option<&str>) + Send + Sync>;
pub type EngineEventSink = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;

#[derive(Clone)]
pub struct EngineManager {
    client: Client,
    repo: Repository,
    extractor_registry: Arc<ExtractorRegistry>,
    active_cancel_tokens: Arc<Mutex<HashMap<String, watch::Sender<bool>>>>,
    active_params: Arc<Mutex<HashMap<String, StartDownloadParams>>>,
    state_listeners: Arc<Mutex<Vec<StateChangeListener>>>,
    event_sink: Arc<Mutex<Option<EngineEventSink>>>,
}

impl EngineManager {
    pub fn new(repo: Repository, extractor_registry: Arc<ExtractorRegistry>) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .pool_idle_timeout(std::time::Duration::from_secs(90))
            .pool_max_idle_per_host(10)
            .build()
            .unwrap_or_default();

        Self {
            client,
            repo,
            extractor_registry,
            active_cancel_tokens: Arc::new(Mutex::new(HashMap::new())),
            active_params: Arc::new(Mutex::new(HashMap::new())),
            state_listeners: Arc::new(Mutex::new(Vec::new())),
            event_sink: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_event_sink(&self, sink: EngineEventSink) {
        let mut s = self.event_sink.lock().unwrap();
        *s = Some(sink);
    }

    pub fn emit_event<T: Serialize>(&self, event: &str, payload: &T) {
        let sink = {
            let s = self.event_sink.lock().unwrap();
            s.clone()
        };
        if let Some(s) = sink {
            if let Ok(val) = serde_json::to_value(payload) {
                s(event, val);
            }
        }
    }

    pub fn add_state_listener(&self, listener: StateChangeListener) {
        let mut listeners = self.state_listeners.lock().unwrap();
        listeners.push(listener);
    }

    pub fn notify_state_changed(&self, id: &str, status: &str, err_msg: Option<&str>) {
        let listeners = {
            let l = self.state_listeners.lock().unwrap();
            l.clone()
        };
        for listener in listeners {
            listener(id, status, err_msg);
        }
    }

    pub fn spawn_task<F>(&self, future: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(future);
        } else {
            get_fallback_runtime().spawn(future);
        }
    }

    pub async fn start_download(
        &self,
        params: StartDownloadParams,
        settings: &AppSettings,
    ) -> Result<String, FreeFetchError> {
        let download_id = Uuid::new_v4().to_string();
        self.start_download_with_id(download_id, params, settings)
            .await
    }

    pub async fn start_download_with_id(
        &self,
        download_id: String,
        params: StartDownloadParams,
        settings: &AppSettings,
    ) -> Result<String, FreeFetchError> {
        let is_sub = params.format_id.starts_with("sub:");
        if params.requires_merge || !is_sub {
            crate::media::validate_stream_combination(
                &params.container,
                params.video_codec.as_deref(),
                params.audio_codec.as_deref(),
                params.subtitle_mode.map(|m| m.as_str()),
                None,
            )?;
        }
        if (!is_sub || !params.download_url.is_empty())
            && !params.download_url.starts_with("ytdlp://")
        {
            let _ = validate_download_url(&params.download_url)?;
        }
        if let Some(ref a_url) = params.matching_audio_url {
            if !a_url.starts_with("ytdlp://") {
                let _ = validate_download_url(a_url)?;
            }
        }

        let dest_dir = if let Some(ref d) = params.destination_dir {
            PathBuf::from(d)
        } else {
            PathBuf::from(&settings.download_directory)
        };

        if !dest_dir.exists() {
            let _ = std::fs::create_dir_all(&dest_dir);
        }

        let reserved = self
            .repo
            .get_active_destination_paths(Some(&download_id))
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
        let record = DownloadRecord {
            id: download_id.clone(),
            source_url: params.source_url.clone(),
            service_name: params.service_name.clone(),
            title: params.title.clone(),
            duration: params.duration,
            thumbnail_url: params.thumbnail_url.clone(),
            destination_path: final_dest.to_string_lossy().to_string(),
            container: params.container.clone(),
            quality_label: params.quality_label.clone(),
            total_bytes: 0,
            downloaded_bytes: 0,
            status: DownloadStatus::Downloading,
            error_message: None,
            created_at: chrono::Utc::now().timestamp(),
            completed_at: None,
        };

        if let Ok(Some(_)) = self.repo.get_download(&download_id) {
            let _ = self.repo.update_download_status(
                &download_id,
                &DownloadStatus::Downloading,
                None,
                None,
            );
        } else {
            self.repo.insert_download(&record)?;
        }

        self.spawn_job(download_id.clone(), params, final_dest, settings.clone());

        Ok(download_id)
    }

    pub async fn resume_download(
        &self,
        id: &str,
        settings: &AppSettings,
    ) -> Result<(), FreeFetchError> {
        let record = self
            .repo
            .get_download(id)?
            .ok_or_else(|| FreeFetchError::NotFound(id.to_string()))?;

        let params_opt = {
            let p = self.active_params.lock().unwrap();
            p.get(id).cloned()
        };

        let params = if let Some(p) = params_opt {
            p
        } else if record.service_name == "youtube" {
            let mut resolved = None;
            if let Ok(parsed_url) = Url::parse(&record.source_url) {
                if let Some(ext) = self.extractor_registry.find_extractor(&parsed_url) {
                    let ctx = ExtractionContext::default();
                    if let Ok((_meta, manifest)) = ext.extract(&parsed_url, &ctx).await {
                        let matched = manifest
                            .formats
                            .iter()
                            .find(|f| {
                                f.quality_label == record.quality_label
                                    && f.container == record.container
                            })
                            .or_else(|| {
                                manifest
                                    .formats
                                    .iter()
                                    .find(|f| f.quality_label == record.quality_label)
                            })
                            .or_else(|| manifest.formats.first());

                        if let Some(fmt) = matched {
                            resolved = Some(StartDownloadParams {
                                source_url: record.source_url.clone(),
                                service_name: record.service_name.clone(),
                                title: record.title.clone(),
                                duration: record.duration,
                                thumbnail_url: record.thumbnail_url.clone(),
                                format_id: fmt.format_id.clone(),
                                download_url: fmt.download_url.clone(),
                                requires_merge: fmt.requires_merge,
                                matching_audio_url: fmt.matching_audio_url.clone(),
                                container: record.container.clone(),
                                quality_label: record.quality_label.clone(),
                                destination_dir: None,
                                selected_subtitles: Vec::new(),
                                embed_subtitles: false,
                                subtitle_mode: None,
                                audio_options: None,
                                video_codec: fmt.video_codec.clone(),
                                audio_codec: fmt.audio_codec.clone(),
                                http_headers: fmt.http_headers.clone(),
                            });
                        }
                    }
                }
            }

            resolved.unwrap_or_else(|| StartDownloadParams {
                source_url: record.source_url.clone(),
                service_name: record.service_name.clone(),
                title: record.title.clone(),
                duration: record.duration,
                thumbnail_url: record.thumbnail_url.clone(),
                format_id: "resumed".to_string(),
                download_url: record.source_url.clone(),
                requires_merge: false,
                matching_audio_url: None,
                container: record.container.clone(),
                quality_label: record.quality_label.clone(),
                destination_dir: None,
                selected_subtitles: Vec::new(),
                embed_subtitles: false,
                subtitle_mode: None,
                audio_options: None,
                video_codec: None,
                audio_codec: None,
                http_headers: None,
            })
        } else {
            StartDownloadParams {
                source_url: record.source_url.clone(),
                service_name: record.service_name.clone(),
                title: record.title.clone(),
                duration: record.duration,
                thumbnail_url: record.thumbnail_url.clone(),
                format_id: "resumed".to_string(),
                download_url: record.source_url.clone(),
                requires_merge: false,
                matching_audio_url: None,
                container: record.container.clone(),
                quality_label: record.quality_label.clone(),
                destination_dir: None,
                selected_subtitles: Vec::new(),
                embed_subtitles: false,
                subtitle_mode: None,
                audio_options: None,
                video_codec: None,
                audio_codec: None,
                http_headers: None,
            }
        };

        let dest = PathBuf::from(&record.destination_path);
        let _ = self
            .repo
            .update_download_status(id, &DownloadStatus::Downloading, None, None);
        self.spawn_job(id.to_string(), params, dest, settings.clone());
        Ok(())
    }

    pub fn pause_download(&self, id: &str) -> Result<(), FreeFetchError> {
        let mut tokens = self.active_cancel_tokens.lock().unwrap();
        if let Some(tx) = tokens.remove(id) {
            let _ = tx.send(true);
        }
        self.repo
            .update_download_status(id, &DownloadStatus::Paused, None, None)?;
        self.notify_state_changed(id, "paused", None);
        Ok(())
    }

    pub fn cancel_download(&self, id: &str) -> Result<(), FreeFetchError> {
        let mut tokens = self.active_cancel_tokens.lock().unwrap();
        if let Some(tx) = tokens.remove(id) {
            let _ = tx.send(true);
        }
        self.repo
            .update_download_status(id, &DownloadStatus::Cancelled, None, None)?;

        if let Ok(Some(rec)) = self.repo.get_download(id) {
            let dest = PathBuf::from(rec.destination_path);
            let part1 = dest.with_extension(format!("{}.part", rec.container));
            let _ = std::fs::remove_file(part1);
            let meta1 = dest.with_extension(format!("{}.part.meta", rec.container));
            let _ = std::fs::remove_file(meta1);
            let part_v = dest.with_extension("video.part");
            let _ = std::fs::remove_file(part_v);
            let meta_v = dest.with_extension("video.part.meta");
            let _ = std::fs::remove_file(meta_v);
            let part_a = dest.with_extension("audio.part");
            let _ = std::fs::remove_file(part_a);
            let meta_a = dest.with_extension("audio.part.meta");
            let _ = std::fs::remove_file(meta_a);
            let embed_tmp = dest.with_extension(format!("embed.tmp.{}", rec.container));
            let _ = std::fs::remove_file(embed_tmp);
            let hardcode_tmp = dest.with_extension(format!("hardcode.tmp.{}", rec.container));
            let _ = std::fs::remove_file(hardcode_tmp);
        }

        let mut p_map = self.active_params.lock().unwrap();
        p_map.remove(id);

        self.notify_state_changed(id, "cancelled", None);

        Ok(())
    }

    pub fn delete_download(&self, id: &str, delete_files: bool) -> Result<(), FreeFetchError> {
        let _ = self.cancel_download(id);

        if delete_files {
            if let Ok(Some(rec)) = self.repo.get_download(id) {
                let dest = PathBuf::from(rec.destination_path);
                let _ = std::fs::remove_file(&dest);
                let _ =
                    std::fs::remove_file(dest.with_extension(format!("{}.part", rec.container)));
                let _ = std::fs::remove_file(
                    dest.with_extension(format!("{}.part.meta", rec.container)),
                );
                let _ = std::fs::remove_file(dest.with_extension("video.part"));
                let _ = std::fs::remove_file(dest.with_extension("video.part.meta"));
                let _ = std::fs::remove_file(dest.with_extension("audio.part"));
                let _ = std::fs::remove_file(dest.with_extension("audio.part.meta"));
                let _ = std::fs::remove_file(
                    dest.with_extension(format!("embed.tmp.{}", rec.container)),
                );
                let _ = std::fs::remove_file(
                    dest.with_extension(format!("hardcode.tmp.{}", rec.container)),
                );
            }
        }

        self.repo.delete_download(id)?;
        Ok(())
    }

    pub async fn clean_cancelled_files(dest: &Path, container: &str) {
        Self::clean_cancelled_files_with_id(dest, container, None).await;
    }

    pub async fn clean_cancelled_files_with_id(dest: &Path, container: &str, job_id: Option<&str>) {
        let mut files_to_delete = vec![
            dest.with_extension(format!("{container}.part")),
            dest.with_extension(format!("{container}.part.meta")),
            dest.with_extension("video.part"),
            dest.with_extension("video.part.meta"),
            dest.with_extension("audio.part"),
            dest.with_extension("audio.part.meta"),
            dest.with_extension(format!("{container}.tmp")),
            dest.with_extension(format!("embed.tmp.{container}")),
            dest.with_extension("embedded.tmp"),
            dest.with_extension("cover.tmp.jpg"),
            dest.to_path_buf(),
        ];

        if let Some(id) = job_id {
            if let Some(parent) = dest.parent() {
                files_to_delete.push(parent.join(format!("{id}.cover.tmp.jpg")));
            }
        }

        for file in files_to_delete {
            if file.exists() {
                for attempt in 0..5 {
                    if tokio::fs::remove_file(&file).await.is_ok() || !file.exists() {
                        break;
                    }
                    if attempt < 4 {
                        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                    }
                }
            }
        }
    }

    fn spawn_job(
        &self,
        id: String,
        params: StartDownloadParams,
        dest: PathBuf,
        settings: AppSettings,
    ) {
        let (cancel_tx, cancel_rx) = watch::channel(false);
        {
            let mut tokens = self.active_cancel_tokens.lock().unwrap();
            tokens.insert(id.clone(), cancel_tx);
            let mut p_map = self.active_params.lock().unwrap();
            p_map.insert(id.clone(), params.clone());
        }

        let client = self.client.clone();
        let repo = self.repo.clone();
        let registry = self.extractor_registry.clone();
        let active_tokens = self.active_cancel_tokens.clone();
        let active_p = self.active_params.clone();
        let state_listeners = self.state_listeners.clone();
        let job_id = id.clone();
        let engine_clone = self.clone();

        self.spawn_task(async move {
            let rate_limiter = RateLimiter::new(settings.speed_limit_bytes_per_sec);
            let result = run_job(
                &engine_clone,
                &client,
                &repo,
                &registry,
                &job_id,
                &params,
                &dest,
                cancel_rx,
                rate_limiter,
                &settings,
            )
            .await;

            // Remove cancel token when job completes
            {
                let mut tokens = active_tokens.lock().unwrap();
                tokens.remove(&job_id);
            }

            match result {
                Ok(_) => {
                    {
                        let mut p_map = active_p.lock().unwrap();
                        p_map.remove(&job_id);
                    }
                    let _ = repo.update_download_status(
                        &job_id,
                        &DownloadStatus::Completed,
                        None,
                        Some(chrono::Utc::now().timestamp()),
                    );
                    engine_clone.emit_event(
                        "download-state-changed",
                        &DownloadStateChangedPayload {
                            id: job_id.clone(),
                            status: "completed".to_string(),
                            error_message: None,
                        },
                    );
                    let listeners = {
                        let l = state_listeners.lock().unwrap();
                        l.clone()
                    };
                    for cb in listeners {
                        cb(&job_id, "completed", None);
                    }
                }
                Err(FreeFetchError::Cancelled) => {
                    let is_paused = repo
                        .get_download(&job_id)
                        .ok()
                        .flatten()
                        .map(|r| r.status == DownloadStatus::Paused)
                        .unwrap_or(false);

                    let target_status = if is_paused { "paused" } else { "cancelled" };

                    if is_paused {
                        engine_clone.emit_event(
                            "download-state-changed",
                            &DownloadStateChangedPayload {
                                id: job_id.clone(),
                                status: "paused".to_string(),
                                error_message: None,
                            },
                        );
                    } else {
                        {
                            let mut p_map = active_p.lock().unwrap();
                            p_map.remove(&job_id);
                        }
                        let _ = repo.update_download_status(
                            &job_id,
                            &DownloadStatus::Cancelled,
                            None,
                            None,
                        );
                        // Clean up all .part, .part.meta, .tmp, and cover art files on cancellation
                        Self::clean_cancelled_files_with_id(
                            &dest,
                            &params.container,
                            Some(&job_id),
                        )
                        .await;

                        engine_clone.emit_event(
                            "download-state-changed",
                            &DownloadStateChangedPayload {
                                id: job_id.clone(),
                                status: "cancelled".to_string(),
                                error_message: None,
                            },
                        );
                    }

                    let listeners = {
                        let l = state_listeners.lock().unwrap();
                        l.clone()
                    };
                    for cb in listeners {
                        cb(&job_id, target_status, None);
                    }
                }
                Err(err) => {
                    let err_msg = err.to_string();
                    let _ = repo.update_download_status(
                        &job_id,
                        &DownloadStatus::Failed,
                        Some(&err_msg),
                        None,
                    );
                    engine_clone.emit_event(
                        "download-state-changed",
                        &DownloadStateChangedPayload {
                            id: job_id.clone(),
                            status: "failed".to_string(),
                            error_message: Some(err_msg.clone()),
                        },
                    );
                    let listeners = {
                        let l = state_listeners.lock().unwrap();
                        l.clone()
                    };
                    for cb in listeners {
                        cb(&job_id, "failed", Some(&err_msg));
                    }
                }
            }
        });
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_job(
    engine: &EngineManager,
    client: &Client,
    repo: &Repository,
    registry: &Arc<ExtractorRegistry>,
    job_id: &str,
    params: &StartDownloadParams,
    dest: &Path,
    cancel_rx: watch::Receiver<bool>,
    rate_limiter: RateLimiter,
    settings: &AppSettings,
) -> Result<(), FreeFetchError> {
    engine.emit_event(
        "download-state-changed",
        &DownloadStateChangedPayload {
            id: job_id.to_string(),
            status: "downloading".to_string(),
            error_message: None,
        },
    );
    engine.notify_state_changed(job_id, "downloading", None);

    if params.format_id.starts_with("sub:") {
        // Standalone subtitle download job
        let target_format = if params.container.eq_ignore_ascii_case("vtt") {
            crate::core::types::SubtitleFormat::Vtt
        } else {
            crate::core::types::SubtitleFormat::Srt
        };

        let parts: Vec<&str> = params.format_id.split(':').collect();
        let lang_code = parts.get(1).unwrap_or(&"en").to_string();
        let is_auto_requested = parts.len() > 3 && parts[2] == "auto";

        let url_refresher: Option<UrlRefresher> = if params.service_name == "youtube" {
            let reg = registry.clone();
            let src_url = params.source_url.clone();
            let lang = lang_code.clone();
            Some(Arc::new(move || {
                let reg = reg.clone();
                let src_url = src_url.clone();
                let lang = lang.clone();
                Box::pin(async move {
                    let u = Url::parse(&src_url)?;
                    let ext = reg.find_extractor(&u).ok_or_else(|| {
                        FreeFetchError::Generic("No compatible extractor found".to_string())
                    })?;
                    let ctx = ExtractionContext::default();
                    let (_meta, manifest) = ext.extract(&u, &ctx).await?;
                    let track = manifest
                        .subtitles
                        .iter()
                        .find(|s| {
                            s.language_code == lang && s.is_auto_generated == is_auto_requested
                        })
                        .or_else(|| manifest.subtitles.iter().find(|s| s.language_code == lang))
                        .or_else(|| {
                            manifest.subtitles.iter().find(|s| {
                                s.language_code.starts_with(lang.as_str())
                                    || lang.starts_with(&s.language_code)
                            })
                        })
                        .or_else(|| manifest.subtitles.first())
                        .ok_or_else(|| {
                            FreeFetchError::NotFound(format!(
                                "Subtitle track for language '{lang}' no longer available"
                            ))
                        })?;
                    track.download_url.clone().ok_or_else(|| {
                        FreeFetchError::Generic("No download URL for subtitle".to_string())
                    })
                })
            }))
        } else {
            None
        };

        let mut current_sub_url = params.download_url.clone();
        if current_sub_url.is_empty() {
            if let Some(ref refresher) = url_refresher {
                current_sub_url = refresher().await?;
            } else {
                return Err(FreeFetchError::Generic(
                    "Missing download URL for subtitle and no refresher available".to_string(),
                ));
            }
        }
        let _ = validate_download_url(&current_sub_url)?;

        let j_id = job_id.to_string();
        let engine_prog = engine.clone();
        let rep = repo.clone();

        let downloaded_bytes = subtitle_downloader::download_and_convert_subtitle_with_limiter(
            client,
            &current_sub_url,
            dest,
            target_format,
            Some(cancel_rx),
            url_refresher,
            Some(&rate_limiter),
        )
        .await?;

        let _ = rep.update_download_progress(&j_id, downloaded_bytes, downloaded_bytes);
        engine_prog.emit_event(
            "download-progress",
            &DownloadProgressPayload {
                id: j_id.clone(),
                downloaded_bytes,
                total_bytes: downloaded_bytes,
                speed_bytes_per_sec: downloaded_bytes,
                eta_seconds: 0,
                progress_percent: 100.0,
            },
        );

        return Ok(());
    }

    if params.download_url.starts_with("ytdlp://") {
        let controller =
            crate::media::YtDlpController::discover(settings.ytdlp_path.as_deref())?;
        let engine_prog = engine.clone();
        let j_id_prog = job_id.to_string();
        let rep_prog = repo.clone();

        let format_selector = if params.format_id == "best" {
            None
        } else {
            Some(params.format_id.as_str())
        };

        let is_audio_job = params.audio_options.is_some()
            || matches!(
                params.container.to_lowercase().as_str(),
                "mp3" | "m4a" | "flac" | "opus"
            );

        let dl_dest = if is_audio_job {
            dest.with_extension(format!("{}.part", params.container))
        } else {
            dest.to_path_buf()
        };

        controller
            .download_with_progress(
                &params.source_url,
                format_selector,
                &dl_dest,
                None,
                Some(cancel_rx.clone()),
                move |percent, downloaded_bytes, total_bytes, speed_bytes_per_sec| {
                    let _ = rep_prog.update_download_progress(
                        &j_id_prog,
                        downloaded_bytes,
                        total_bytes,
                    );
                    engine_prog.emit_event(
                        "download-progress",
                        &DownloadProgressPayload {
                            id: j_id_prog.clone(),
                            downloaded_bytes,
                            total_bytes,
                            speed_bytes_per_sec,
                            eta_seconds: 0,
                            progress_percent: percent,
                        },
                    );
                },
            )
            .await?;

        if is_audio_job {
            let resolved_opts = params.audio_options.clone().unwrap_or_else(|| {
                let fmt = AudioTargetFormat::from_str_loose(&params.container)
                    .unwrap_or(AudioTargetFormat::Mp3);
                AudioConversionOptions {
                    target_format: fmt,
                    quality_preset: AudioQualityPreset::Best,
                    bitrate_mode: BitrateMode::Cbr,
                    custom_bitrate_kbps: None,
                    sample_rate: None,
                    embed_metadata: true,
                    embed_artwork: true,
                    filename_template: None,
                }
            });

            let ffmpeg = FfmpegController::discover(settings.ffmpeg_path.as_deref())?;
            engine.emit_event(
                "download-state-changed",
                &DownloadStateChangedPayload {
                    id: job_id.to_string(),
                    status: "processing".to_string(),
                    error_message: None,
                },
            );
            engine.notify_state_changed(job_id, "processing", None);
            let _ = repo.update_download_status(job_id, &DownloadStatus::Processing, None, None);

            let dest_dir = dest.parent().unwrap_or_else(|| Path::new("."));
            let artwork_temp =
                if resolved_opts.embed_artwork && resolved_opts.target_format.supports_artwork() {
                    if let Some(ref thumb) = params.thumbnail_url {
                        ArtworkDownloader::download_cover_art(client, thumb, dest_dir, job_id).await
                    } else {
                        None
                    }
                } else {
                    None
                };

            let audio_meta = if resolved_opts.embed_metadata {
                let m = MediaMetadata {
                    id: params.format_id.clone(),
                    url: params.source_url.clone(),
                    title: params.title.clone(),
                    duration: params.duration,
                    thumbnail_url: params.thumbnail_url.clone(),
                    uploader: None,
                    service_name: params.service_name.clone(),
                    description: None,
                    upload_date: None,
                };
                Some(AudioMetadata::from_media_metadata(&m, None, None))
            } else {
                None
            };

            let conv_res = ffmpeg
                .convert_audio_cancellable(
                    &dl_dest,
                    dest,
                    &resolved_opts,
                    audio_meta.as_ref(),
                    artwork_temp.as_deref(),
                    Some(cancel_rx.clone()),
                )
                .await;

            if let Some(ref art) = artwork_temp {
                ArtworkDownloader::cleanup_cover_art(art).await;
            }

            conv_res?;
            let _ = tokio::fs::remove_file(&dl_dest).await;
        }

        return Ok(());
    }

    if !params.requires_merge {
        // Direct single stream download with parallel range workers
        let part_path = dest.with_extension(format!("{}.part", params.container));
        let j_id = job_id.to_string();
        let rep = repo.clone();

        let engine_status = engine.clone();
        let j_id_status = job_id.to_string();
        let rep_status = repo.clone();
        let on_status = move |status: DownloadStatus, msg: &str| {
            let err_opt = if matches!(status, DownloadStatus::Failed | DownloadStatus::Retrying) {
                Some(msg)
            } else {
                None
            };
            let _ = rep_status.update_download_status(&j_id_status, &status, err_opt, None);
            engine_status.emit_event(
                "download-state-changed",
                &DownloadStateChangedPayload {
                    id: j_id_status.clone(),
                    status: status.as_str().to_string(),
                    error_message: err_opt.map(|s| s.to_string()),
                },
            );
        };

        let url_refresher: Option<UrlRefresher> = {
            let reg = registry.clone();
            let src_url = params.source_url.clone();
            let fmt_id = params.format_id.clone();
            let qual = params.quality_label.clone();
            Some(Arc::new(move || {
                let reg = reg.clone();
                let src_url = src_url.clone();
                let fmt_id = fmt_id.clone();
                let qual = qual.clone();
                Box::pin(async move {
                    let u = Url::parse(&src_url)?;
                    let ext = reg.find_extractor(&u).ok_or_else(|| {
                        FreeFetchError::Generic("No compatible extractor found".to_string())
                    })?;
                    let ctx = ExtractionContext::default();
                    let (_meta, manifest) = ext.extract(&u, &ctx).await?;
                    let found = manifest
                        .formats
                        .iter()
                        .find(|f| f.format_id == fmt_id || f.quality_label == qual)
                        .or_else(|| manifest.formats.first())
                        .ok_or_else(|| {
                            FreeFetchError::Generic("Stream format no longer available".to_string())
                        })?;
                    Ok(found.download_url.clone())
                })
            }))
        };

        let headers_arc = params.http_headers.as_ref().map(|h| Arc::new(h.clone()));
        let engine_prog = engine.clone();
        let j_id_prog = j_id.clone();
        let rep_prog = rep.clone();
        let dl_res = parallel_downloader::download_stream_parallel_with_headers(
            client,
            &params.download_url,
            &part_path,
            cancel_rx.clone(),
            rate_limiter,
            4,
            url_refresher,
            headers_arc,
            move |p| {
                let percent = if p.total_bytes > 0 {
                    (p.downloaded_bytes as f64 / p.total_bytes as f64) * 100.0
                } else {
                    0.0
                };
                let _ = rep_prog.update_download_progress(
                    &j_id_prog,
                    p.downloaded_bytes,
                    p.total_bytes,
                );
                engine_prog.emit_event(
                    "download-progress",
                    &DownloadProgressPayload {
                        id: j_id_prog.clone(),
                        downloaded_bytes: p.downloaded_bytes,
                        total_bytes: p.total_bytes,
                        speed_bytes_per_sec: p.speed_bytes_per_sec,
                        eta_seconds: p.eta_seconds,
                        progress_percent: percent,
                    },
                );
            },
            on_status,
        )
        .await;

        if let Err(FreeFetchError::Network(ref err_msg)) = dl_res {
            if err_msg.contains("403") {
                // If it's a URL supported by yt-dlp, fallback to yt-dlp managed download
                if let Ok(src_u) = Url::parse(&params.source_url) {
                    if let Some(ext) = registry.find_extractor(&src_u) {
                        if ext.name() == "ytdlp" || ext.can_handle(&src_u) {
                            if let Ok(controller) =
                                crate::media::YtDlpController::discover(settings.ytdlp_path.as_deref())
                            {
                                let engine_fb = engine.clone();
                                let j_id_fb = job_id.to_string();
                                let rep_fb = repo.clone();
                                let format_selector = if params.format_id == "best" {
                                    None
                                } else {
                                    Some(params.format_id.as_str())
                                };
                                return controller
                                    .download_with_progress(
                                        &params.source_url,
                                        format_selector,
                                        dest,
                                        None,
                                        Some(cancel_rx.clone()),
                                        move |percent, downloaded_bytes, total_bytes, speed_bytes_per_sec| {
                                            let _ = rep_fb.update_download_progress(
                                                &j_id_fb,
                                                downloaded_bytes,
                                                total_bytes,
                                            );
                                            engine_fb.emit_event(
                                                "download-progress",
                                                &DownloadProgressPayload {
                                                    id: j_id_fb.clone(),
                                                    downloaded_bytes,
                                                    total_bytes,
                                                    speed_bytes_per_sec,
                                                    eta_seconds: 0,
                                                    progress_percent: percent,
                                                },
                                            );
                                        },
                                    )
                                    .await;
                            }
                        }
                    }
                }
            }
            return Err(dl_res.unwrap_err());
        } else {
            dl_res?;
        }

        // Check if audio conversion is requested (either via explicit audio_options or audio container)
        let is_audio_job = params.audio_options.is_some()
            || matches!(
                params.container.to_lowercase().as_str(),
                "mp3" | "m4a" | "flac" | "opus"
            );

        if is_audio_job {
            let resolved_opts = params.audio_options.clone().unwrap_or_else(|| {
                let fmt = AudioTargetFormat::from_str_loose(&params.container)
                    .unwrap_or(AudioTargetFormat::Mp3);
                AudioConversionOptions {
                    target_format: fmt,
                    quality_preset: AudioQualityPreset::Best,
                    bitrate_mode: BitrateMode::Cbr,
                    custom_bitrate_kbps: None,
                    sample_rate: None,
                    embed_metadata: true,
                    embed_artwork: true,
                    filename_template: None,
                }
            });

            let ffmpeg = FfmpegController::discover(settings.ffmpeg_path.as_deref())?;
            engine.emit_event(
                "download-state-changed",
                &DownloadStateChangedPayload {
                    id: job_id.to_string(),
                    status: "processing".to_string(),
                    error_message: None,
                },
            );
            engine.notify_state_changed(job_id, "processing", None);
            let _ = rep.update_download_status(&j_id, &DownloadStatus::Processing, None, None);

            // Resolve artwork if requested
            let dest_dir = dest.parent().unwrap_or_else(|| Path::new("."));
            let artwork_temp =
                if resolved_opts.embed_artwork && resolved_opts.target_format.supports_artwork() {
                    if let Some(ref thumb) = params.thumbnail_url {
                        ArtworkDownloader::download_cover_art(client, thumb, dest_dir, job_id).await
                    } else {
                        None
                    }
                } else {
                    None
                };

            // Resolve metadata if requested
            let audio_meta = if resolved_opts.embed_metadata {
                let m = MediaMetadata {
                    id: params.format_id.clone(),
                    url: params.source_url.clone(),
                    title: params.title.clone(),
                    duration: params.duration,
                    thumbnail_url: params.thumbnail_url.clone(),
                    uploader: None,
                    service_name: params.service_name.clone(),
                    description: None,
                    upload_date: None,
                };
                Some(AudioMetadata::from_media_metadata(&m, None, None))
            } else {
                None
            };

            let conv_res = ffmpeg
                .convert_audio_cancellable(
                    &part_path,
                    dest,
                    &resolved_opts,
                    audio_meta.as_ref(),
                    artwork_temp.as_deref(),
                    Some(cancel_rx.clone()),
                )
                .await;

            if let Some(ref art) = artwork_temp {
                ArtworkDownloader::cleanup_cover_art(art).await;
            }

            conv_res?;
            let _ = tokio::fs::remove_file(&part_path).await;
        } else {
            if dest.exists() {
                let _ = tokio::fs::remove_file(dest).await;
            }
            tokio::fs::rename(&part_path, dest).await?;
        }

        // Embed subtitles if requested
        maybe_embed_subtitles(
            client,
            engine,
            job_id,
            params,
            dest,
            settings,
            Some(cancel_rx.clone()),
        )
        .await?;
    } else {
        // Separate video and audio streams: validate codec combination and download concurrently
        crate::media::validate_stream_combination(
            &params.container,
            params.video_codec.as_deref(),
            params.audio_codec.as_deref(),
            params.subtitle_mode.map(|m| m.as_str()),
            None,
        )?;

        let ffmpeg = FfmpegController::discover(settings.ffmpeg_path.as_deref())?;
        let video_part = dest.with_extension("video.part");
        let audio_part = dest.with_extension("audio.part");

        let audio_url = params.matching_audio_url.as_ref().ok_or_else(|| {
            FreeFetchError::Generic("Missing matching audio stream URL".to_string())
        })?;

        let video_finished = repo
            .get_stream(job_id, "video")
            .map(|opt| {
                opt.map(|s| s.is_finished && video_part.exists())
                    .unwrap_or(false)
            })
            .unwrap_or(false);

        let audio_finished = repo
            .get_stream(job_id, "audio")
            .map(|opt| {
                opt.map(|s| s.is_finished && audio_part.exists())
                    .unwrap_or(false)
            })
            .unwrap_or(false);

        // Progress aggregation
        let v_downloaded = Arc::new(AtomicU64::new(0));
        let v_total = Arc::new(AtomicU64::new(0));
        let a_downloaded = Arc::new(AtomicU64::new(0));
        let a_total = Arc::new(AtomicU64::new(0));

        let engine_prog = engine.clone();
        let j_id_prog = job_id.to_string();
        let rep_prog = repo.clone();
        let tracker_arc = Arc::new(tokio::sync::Mutex::new(SpeedTracker::default()));

        let emit_combined_progress = {
            let v_dl = v_downloaded.clone();
            let v_tot = v_total.clone();
            let a_dl = a_downloaded.clone();
            let a_tot = a_total.clone();
            let engine = engine_prog.clone();
            let j_id = j_id_prog.clone();
            let rep = rep_prog.clone();
            let tr = tracker_arc.clone();

            Arc::new(move || {
                let d = v_dl.load(Ordering::Relaxed) + a_dl.load(Ordering::Relaxed);
                let t = v_tot.load(Ordering::Relaxed) + a_tot.load(Ordering::Relaxed);
                let percent = if t > 0 {
                    (d as f64 / t as f64) * 100.0
                } else {
                    0.0
                };
                let tr = tr.clone();
                let engine = engine.clone();
                let j_id = j_id.clone();
                let rep = rep.clone();

                tokio::spawn(async move {
                    let (speed, eta) = {
                        let mut t_lock = tr.lock().await;
                        let s = t_lock.update(d, Instant::now());
                        let e = t_lock.calculate_eta(d, t);
                        (s, e)
                    };
                    let _ = rep.update_download_progress(&j_id, d, t);
                    engine.emit_event(
                        "download-progress",
                        &DownloadProgressPayload {
                            id: j_id,
                            downloaded_bytes: d,
                            total_bytes: t,
                            speed_bytes_per_sec: speed,
                            eta_seconds: eta,
                            progress_percent: percent,
                        },
                    );
                });
            })
        };

        // 1. Video Download Task
        let client_v = client.clone();
        let video_url = params.download_url.clone();
        let video_part_clone = video_part.clone();
        let rx_v = cancel_rx.clone();
        let rate_limiter_v = rate_limiter.clone();
        let repo_v = repo.clone();
        let job_id_v = job_id.to_string();
        let format_id_v = params.format_id.clone();

        let engine_status_v = engine.clone();
        let j_id_status_v = job_id.to_string();
        let rep_status_v = repo.clone();
        let on_status_v = move |status: DownloadStatus, msg: &str| {
            let err_opt = if matches!(status, DownloadStatus::Failed | DownloadStatus::Retrying) {
                Some(msg)
            } else {
                None
            };
            let _ = rep_status_v.update_download_status(&j_id_status_v, &status, err_opt, None);
            engine_status_v.emit_event(
                "download-state-changed",
                &DownloadStateChangedPayload {
                    id: j_id_status_v.clone(),
                    status: status.as_str().to_string(),
                    error_message: err_opt.map(|s| s.to_string()),
                },
            );
        };

        let headers_arc = params.http_headers.as_ref().map(|h| Arc::new(h.clone()));

        let video_refresher: Option<UrlRefresher> = {
            let reg = registry.clone();
            let src_url = params.source_url.clone();
            let fmt_id = params.format_id.clone();
            let qual = params.quality_label.clone();
            Some(Arc::new(move || {
                let reg = reg.clone();
                let src_url = src_url.clone();
                let fmt_id = fmt_id.clone();
                let qual = qual.clone();
                Box::pin(async move {
                    let u = Url::parse(&src_url)?;
                    let ext = reg.find_extractor(&u).ok_or_else(|| {
                        FreeFetchError::Generic("No compatible extractor found".to_string())
                    })?;
                    let ctx = ExtractionContext::default();
                    let (_meta, manifest) = ext.extract(&u, &ctx).await?;
                    let found = manifest
                        .formats
                        .iter()
                        .find(|f| f.format_id == fmt_id || f.quality_label == qual)
                        .or_else(|| manifest.formats.first())
                        .ok_or_else(|| {
                            FreeFetchError::Generic("Video stream disappeared".to_string())
                        })?;
                    Ok(found.download_url.clone())
                })
            }))
        };

        let emit_prog_v = emit_combined_progress.clone();
        let v_dl_copy = v_downloaded.clone();
        let v_tot_copy = v_total.clone();
        let on_v_prog = move |p: ProgressUpdate| {
            v_dl_copy.store(p.downloaded_bytes, Ordering::Relaxed);
            v_tot_copy.store(p.total_bytes, Ordering::Relaxed);
            emit_prog_v();
        };

        let headers_v = headers_arc.clone();
        let download_video = async move {
            if !video_finished {
                let v_bytes = parallel_downloader::download_stream_parallel_with_headers(
                    &client_v,
                    &video_url,
                    &video_part_clone,
                    rx_v,
                    rate_limiter_v,
                    8, // 8 parallel workers for video stream
                    video_refresher,
                    headers_v,
                    on_v_prog,
                    on_status_v,
                )
                .await?;

                repo_v.upsert_stream(&StreamRecord {
                    id: format!("{job_id_v}-video"),
                    download_id: job_id_v.clone(),
                    stream_type: "video".to_string(),
                    format_id: format_id_v,
                    codec: None,
                    resolution: None,
                    bitrate: None,
                    temp_file_path: video_part_clone.to_string_lossy().to_string(),
                    total_bytes: v_bytes,
                    downloaded_bytes: v_bytes,
                    is_finished: true,
                })?;
            }
            Ok::<(), FreeFetchError>(())
        };

        // 2. Audio Download Task
        let client_a = client.clone();
        let audio_url_clone = audio_url.clone();
        let audio_part_clone = audio_part.clone();
        let rx_a = cancel_rx.clone();
        let rate_limiter_a = rate_limiter.clone();
        let repo_a = repo.clone();
        let job_id_a = job_id.to_string();

        let engine_status_a = engine.clone();
        let j_id_status_a = job_id.to_string();
        let rep_status_a = repo.clone();
        let on_status_a = move |status: DownloadStatus, msg: &str| {
            let err_opt = if matches!(status, DownloadStatus::Failed | DownloadStatus::Retrying) {
                Some(msg)
            } else {
                None
            };
            let _ = rep_status_a.update_download_status(&j_id_status_a, &status, err_opt, None);
            engine_status_a.emit_event(
                "download-state-changed",
                &DownloadStateChangedPayload {
                    id: j_id_status_a.clone(),
                    status: status.as_str().to_string(),
                    error_message: err_opt.map(|s| s.to_string()),
                },
            );
        };

        let audio_refresher: Option<UrlRefresher> = {
            let reg = registry.clone();
            let src_url = params.source_url.clone();
            Some(Arc::new(move || {
                let reg = reg.clone();
                let src_url = src_url.clone();
                Box::pin(async move {
                    let u = Url::parse(&src_url)?;
                    let ext = reg.find_extractor(&u).ok_or_else(|| {
                        FreeFetchError::Generic("No compatible extractor found".to_string())
                    })?;
                    let ctx = ExtractionContext::default();
                    let (_meta, manifest) = ext.extract(&u, &ctx).await?;
                    let found = manifest
                        .formats
                        .iter()
                        .find(|f| f.format_type == "audio")
                        .or_else(|| {
                            manifest
                                .formats
                                .iter()
                                .find_map(|f| f.matching_audio_url.as_ref().map(|_| f))
                        })
                        .ok_or_else(|| {
                            FreeFetchError::Generic("Audio stream disappeared".to_string())
                        })?;
                    let a_url = if found.format_type == "audio" {
                        found.download_url.clone()
                    } else {
                        found.matching_audio_url.clone().unwrap()
                    };
                    Ok(a_url)
                })
            }))
        };

        let emit_prog_a = emit_combined_progress.clone();
        let a_dl_copy = a_downloaded.clone();
        let a_tot_copy = a_total.clone();
        let on_a_prog = move |p: ProgressUpdate| {
            a_dl_copy.store(p.downloaded_bytes, Ordering::Relaxed);
            a_tot_copy.store(p.total_bytes, Ordering::Relaxed);
            emit_prog_a();
        };

        let headers_a = headers_arc.clone();
        let download_audio = async move {
            if !audio_finished {
                let a_bytes = parallel_downloader::download_stream_parallel_with_headers(
                    &client_a,
                    &audio_url_clone,
                    &audio_part_clone,
                    rx_a,
                    rate_limiter_a,
                    2, // 2 parallel workers for audio
                    audio_refresher,
                    headers_a,
                    on_a_prog,
                    on_status_a,
                )
                .await?;

                repo_a.upsert_stream(&StreamRecord {
                    id: format!("{job_id_a}-audio"),
                    download_id: job_id_a.clone(),
                    stream_type: "audio".to_string(),
                    format_id: "audio".to_string(),
                    codec: None,
                    resolution: None,
                    bitrate: None,
                    temp_file_path: audio_part_clone.to_string_lossy().to_string(),
                    total_bytes: a_bytes,
                    downloaded_bytes: a_bytes,
                    is_finished: true,
                })?;
            }
            Ok::<(), FreeFetchError>(())
        };

        // Run video and audio downloads concurrently!
        tokio::try_join!(download_video, download_audio)?;

        // 3. Remux streams using FFmpeg
        let _ = repo.update_download_status(job_id, &DownloadStatus::Processing, None, None);
        engine.emit_event(
            "download-state-changed",
            &DownloadStateChangedPayload {
                id: job_id.to_string(),
                status: "processing".to_string(),
                error_message: None,
            },
        );
        engine.notify_state_changed(job_id, "processing", None);

        ffmpeg
            .merge_streams_cancellable(&video_part, &audio_part, dest, Some(cancel_rx.clone()))
            .await?;

        // Clean up partial chunks and metadata
        let _ = tokio::fs::remove_file(&video_part).await;
        let _ = tokio::fs::remove_file(dest.with_extension("video.part.meta")).await;
        let _ = tokio::fs::remove_file(&audio_part).await;
        let _ = tokio::fs::remove_file(dest.with_extension("audio.part.meta")).await;

        // Embed subtitles if requested
        maybe_embed_subtitles(
            client,
            engine,
            job_id,
            params,
            dest,
            settings,
            Some(cancel_rx.clone()),
        )
        .await?;
    }

    Ok(())
}

async fn maybe_embed_subtitles(
    client: &Client,
    engine: &EngineManager,
    job_id: &str,
    params: &StartDownloadParams,
    dest: &Path,
    settings: &AppSettings,
    cancel_rx: Option<watch::Receiver<bool>>,
) -> Result<(), FreeFetchError> {
    let mode = params.resolved_subtitle_mode();
    if mode == crate::core::types::SubtitleMode::None
        || params.selected_subtitles.is_empty()
        || params.audio_options.is_some()
    {
        return Ok(());
    }

    let ffmpeg = match FfmpegController::discover(settings.ffmpeg_path.as_deref()) {
        Ok(f) => f,
        Err(_) => return Ok(()),
    };

    engine.emit_event(
        "download-state-changed",
        &DownloadStateChangedPayload {
            id: job_id.to_string(),
            status: "processing".to_string(),
            error_message: None,
        },
    );
    engine.notify_state_changed(job_id, "processing", None);

    let parent = dest.parent().unwrap_or_else(|| Path::new("."));
    let ext = dest.extension().and_then(|e| e.to_str()).unwrap_or("mp4");

    match mode {
        crate::core::types::SubtitleMode::Hardcode => {
            // Hardcode mode burns exactly one selected subtitle track into video frames
            let sub = params.selected_subtitles.first().ok_or_else(|| {
                FreeFetchError::Generic("No subtitle selected for hardcoding".to_string())
            })?;
            let dl_url = sub.download_url.as_ref().ok_or_else(|| {
                FreeFetchError::Generic("Selected subtitle missing download URL".to_string())
            })?;

            let sub_path = parent.join(format!("{}.hardcode.{}.srt", job_id, sub.language_code));
            subtitle_downloader::download_and_convert_subtitle(
                client,
                dl_url,
                &sub_path,
                crate::core::types::SubtitleFormat::Srt,
                cancel_rx.clone(),
                None,
            )
            .await?;

            let hardcode_tmp = dest.with_extension(format!("hardcode.tmp.{ext}"));
            let hc_result = ffmpeg
                .hardcode_subtitles_cancellable(dest, &sub_path, &hardcode_tmp, cancel_rx)
                .await;

            let _ = tokio::fs::remove_file(&sub_path).await;

            match hc_result {
                Ok(_) => {
                    let _ = tokio::fs::remove_file(dest).await;
                    tokio::fs::rename(&hardcode_tmp, dest).await?;
                }
                Err(FreeFetchError::Cancelled) => {
                    let _ = tokio::fs::remove_file(&hardcode_tmp).await;
                    return Err(FreeFetchError::Cancelled);
                }
                Err(e) => {
                    let _ = tokio::fs::remove_file(&hardcode_tmp).await;
                    return Err(e);
                }
            }
        }
        crate::core::types::SubtitleMode::Embed => {
            let mut temp_subs: Vec<(PathBuf, String, String)> = Vec::new();

            for sub in &params.selected_subtitles {
                if let Some(ref dl_url) = sub.download_url {
                    let sub_path =
                        parent.join(format!("{}.embed.{}.srt", job_id, sub.language_code));
                    if subtitle_downloader::download_and_convert_subtitle(
                        client,
                        dl_url,
                        &sub_path,
                        crate::core::types::SubtitleFormat::Srt,
                        cancel_rx.clone(),
                        None,
                    )
                    .await
                    .is_ok()
                    {
                        temp_subs.push((
                            sub_path,
                            sub.language_code.clone(),
                            sub.language_name.clone(),
                        ));
                    }
                }
            }

            if temp_subs.is_empty() {
                return Ok(());
            }

            let sub_refs: Vec<(&Path, &str, &str)> = temp_subs
                .iter()
                .map(|(p, code, title)| (p.as_path(), code.as_str(), title.as_str()))
                .collect();

            let embedded_tmp = dest.with_extension(format!("embed.tmp.{ext}"));
            let embed_result = ffmpeg
                .embed_subtitles_cancellable(dest, &sub_refs, &embedded_tmp, cancel_rx)
                .await;

            // Clean up temporary downloaded subtitle files
            for (p, _, _) in &temp_subs {
                let _ = tokio::fs::remove_file(p).await;
            }

            match embed_result {
                Ok(_) => {
                    let _ = tokio::fs::remove_file(dest).await;
                    tokio::fs::rename(&embedded_tmp, dest).await?;
                }
                Err(FreeFetchError::Cancelled) => {
                    let _ = tokio::fs::remove_file(&embedded_tmp).await;
                    return Err(FreeFetchError::Cancelled);
                }
                Err(e) => {
                    let _ = tokio::fs::remove_file(&embedded_tmp).await;
                    return Err(e);
                }
            }
        }
        crate::core::types::SubtitleMode::None => {}
    }

    Ok(())
}
