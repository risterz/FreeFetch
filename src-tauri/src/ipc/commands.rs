use std::path::Path;
use std::sync::{Arc, Mutex};
use tauri::State;
use url::Url;

use crate::core::config::AppSettings;
use crate::core::error::FreeFetchError;
use crate::core::types::{
    AudioConversionOptions, AudioQualityPreset, AudioTargetFormat, BatchAnalysisResult,
    BatchQueueResult, BitrateMode, DownloadRecord, DownloadStatistics, HistoryFilter, HistoryPage,
    PlaylistDetection, PlaylistInfo, PlaylistItemSelection, QueueItem, QueueSummary,
};
use crate::engine::queue::QueueManager;
use crate::engine::{EngineManager, StartDownloadParams};
use crate::extractor::{ExtractionContext, ExtractorRegistry, ParseResult};
use crate::media::ffmpeg::FfmpegController;
use crate::service::batch::{
    add_batch_to_queue as service_add_batch_to_queue,
    analyze_batch_urls as service_analyze_batch_urls, parse_batch_urls,
};
use crate::service::playlist::{
    detect_playlist_url as service_detect_playlist_url, discover_playlist, queue_playlist_items,
};
use crate::storage::Repository;

pub struct AppState {
    pub repo: Repository,
    pub engine: EngineManager,
    pub queue: QueueManager,
    pub registry: Arc<ExtractorRegistry>,
    pub settings: Arc<Mutex<AppSettings>>,
}

#[tauri::command]
pub async fn parse_url(
    url: String,
    state: State<'_, AppState>,
) -> Result<ParseResult, FreeFetchError> {
    let parsed_url = Url::parse(&url).map_err(|e| FreeFetchError::InvalidUrl(e.to_string()))?;
    let extractor =
        state
            .registry
            .find_extractor(&parsed_url)
            .ok_or_else(|| FreeFetchError::Extractor {
                service: "unknown".to_string(),
                message: "No compatible extractor found for this URL".to_string(),
            })?;

    let ctx = ExtractionContext::default();
    let (metadata, manifest) = extractor.extract(&parsed_url, &ctx).await?;

    Ok(ParseResult {
        metadata,
        formats: manifest.formats,
        subtitles: manifest.subtitles,
    })
}

#[tauri::command]
pub async fn start_download(
    params: StartDownloadParams,
    state: State<'_, AppState>,
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
    let settings = {
        let s = state.settings.lock().unwrap();
        s.clone()
    };
    state.engine.start_download(params, &settings).await
}

#[tauri::command]
pub async fn pause_download(id: String, state: State<'_, AppState>) -> Result<(), FreeFetchError> {
    state.engine.pause_download(&id)
}

#[tauri::command]
pub async fn resume_download(id: String, state: State<'_, AppState>) -> Result<(), FreeFetchError> {
    let settings = {
        let s = state.settings.lock().unwrap();
        s.clone()
    };
    state.engine.resume_download(&id, &settings).await
}

#[tauri::command]
pub async fn cancel_download(id: String, state: State<'_, AppState>) -> Result<(), FreeFetchError> {
    state.engine.cancel_download(&id)
}

#[tauri::command]
pub async fn delete_download(
    id: String,
    delete_files: bool,
    state: State<'_, AppState>,
) -> Result<(), FreeFetchError> {
    state.engine.delete_download(&id, delete_files)
}

#[tauri::command]
pub fn get_downloads(state: State<'_, AppState>) -> Result<Vec<DownloadRecord>, FreeFetchError> {
    state.repo.list_downloads()
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, FreeFetchError> {
    let s = state.settings.lock().unwrap();
    Ok(s.clone())
}

#[tauri::command]
pub fn update_settings(
    mut settings: AppSettings,
    state: State<'_, AppState>,
) -> Result<(), FreeFetchError> {
    settings.sanitize();

    // Validate FFmpeg path if custom specified
    if let Some(ref path_str) = settings.ffmpeg_path {
        let p = Path::new(path_str);
        if !p.exists() || !p.is_file() {
            return Err(FreeFetchError::Generic(format!(
                "Specified FFmpeg executable does not exist: {}",
                path_str
            )));
        }
        FfmpegController::discover(Some(path_str))
            .map_err(|e| FreeFetchError::Generic(format!("Invalid FFmpeg binary: {}", e)))?;
    }

    // Validate and prepare download directory
    let dl_path = Path::new(&settings.download_directory);
    if !dl_path.exists() {
        if let Err(e) = std::fs::create_dir_all(dl_path) {
            return Err(FreeFetchError::Generic(format!(
                "Cannot create download directory: {}",
                e
            )));
        }
    }

    let serialized =
        serde_json::to_string(&settings).map_err(|e| FreeFetchError::Generic(e.to_string()))?;
    state.repo.set_setting("app_settings", &serialized)?;
    let mut s = state.settings.lock().unwrap();
    *s = settings.clone();
    let _ = state
        .queue
        .set_concurrency(settings.max_concurrent_downloads);
    Ok(())
}

#[tauri::command]
pub fn check_ffmpeg_status(state: State<'_, AppState>) -> Result<String, FreeFetchError> {
    let settings = state.settings.lock().unwrap();
    let controller = FfmpegController::discover(settings.ffmpeg_path.as_deref())?;
    Ok(controller.binary_path().to_string_lossy().to_string())
}

#[tauri::command]
pub fn open_downloaded_file(path: String) -> Result<(), FreeFetchError> {
    let p = Path::new(&path);
    if !p.exists() {
        return Err(FreeFetchError::NotFound(format!(
            "File does not exist: {path}"
        )));
    }

    #[cfg(target_os = "windows")]
    {
        crate::core::hidden_std_command("rundll32.exe")
            .arg("url.dll,FileProtocolHandler")
            .arg(&path)
            .spawn()
            .map_err(|e| FreeFetchError::Io(e.to_string()))?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| FreeFetchError::Io(e.to_string()))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| FreeFetchError::Io(e.to_string()))?;
    }

    Ok(())
}

#[tauri::command]
pub fn open_containing_folder(path: String) -> Result<(), FreeFetchError> {
    let p = Path::new(&path);

    #[cfg(target_os = "windows")]
    {
        if p.is_file() {
            std::process::Command::new("explorer.exe")
                .arg(format!("/select,\"{}\"", path.replace('/', "\\")))
                .spawn()
                .map_err(|e| FreeFetchError::Io(e.to_string()))?;
        } else if p.is_dir() {
            std::process::Command::new("explorer.exe")
                .arg(path.replace('/', "\\"))
                .spawn()
                .map_err(|e| FreeFetchError::Io(e.to_string()))?;
        } else if let Some(parent) = p.parent() {
            std::process::Command::new("explorer.exe")
                .arg(parent.to_string_lossy().replace('/', "\\"))
                .spawn()
                .map_err(|e| FreeFetchError::Io(e.to_string()))?;
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(parent) = p.parent() {
            std::process::Command::new("open")
                .arg(parent)
                .spawn()
                .map_err(|e| FreeFetchError::Io(e.to_string()))?;
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(parent) = p.parent() {
            std::process::Command::new("xdg-open")
                .arg(parent)
                .spawn()
                .map_err(|e| FreeFetchError::Io(e.to_string()))?;
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn queue_add(
    params: StartDownloadParams,
    state: State<'_, AppState>,
) -> Result<QueueItem, FreeFetchError> {
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
    let subs = params.selected_subtitles.clone();
    if !subs.is_empty() && params.resolved_subtitle_mode() == crate::core::types::SubtitleMode::None
    {
        let items = state.queue.add_with_subtitles(params, subs).await?;
        items
            .into_iter()
            .next()
            .ok_or_else(|| FreeFetchError::Generic("No queue item created".to_string()))
    } else {
        state.queue.add(params).await
    }
}

#[tauri::command]
pub async fn queue_remove(id: String, state: State<'_, AppState>) -> Result<(), FreeFetchError> {
    state.queue.remove(&id).await
}

#[tauri::command]
pub async fn queue_start(state: State<'_, AppState>) -> Result<(), FreeFetchError> {
    state.queue.start_all().await
}

#[tauri::command]
pub async fn queue_pause(
    id: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), FreeFetchError> {
    if let Some(item_id) = id {
        state.queue.pause_item(&item_id).await
    } else {
        state.queue.pause_all().await
    }
}

#[tauri::command]
pub async fn queue_resume(
    id: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), FreeFetchError> {
    if let Some(item_id) = id {
        state.queue.resume_item(&item_id).await
    } else {
        state.queue.resume_all().await
    }
}

#[tauri::command]
pub async fn queue_cancel(
    id: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), FreeFetchError> {
    if let Some(item_id) = id {
        state.queue.cancel_item(&item_id).await
    } else {
        state.queue.cancel_all().await
    }
}

#[tauri::command]
pub async fn queue_retry(id: String, state: State<'_, AppState>) -> Result<(), FreeFetchError> {
    state.queue.retry_item(&id).await
}

#[tauri::command]
pub async fn queue_reorder(
    id: String,
    target_position: i32,
    state: State<'_, AppState>,
) -> Result<(), FreeFetchError> {
    state.queue.reorder(&id, target_position).await
}

#[tauri::command]
pub async fn queue_move_up(id: String, state: State<'_, AppState>) -> Result<(), FreeFetchError> {
    state.queue.move_up(&id).await
}

#[tauri::command]
pub async fn queue_move_down(id: String, state: State<'_, AppState>) -> Result<(), FreeFetchError> {
    state.queue.move_down(&id).await
}

#[tauri::command]
pub fn queue_get(state: State<'_, AppState>) -> Result<Vec<QueueItem>, FreeFetchError> {
    state.queue.get_items()
}

#[tauri::command]
pub async fn queue_clear_completed(state: State<'_, AppState>) -> Result<(), FreeFetchError> {
    state.queue.clear_completed().await
}

#[tauri::command]
pub async fn queue_clear_failed(state: State<'_, AppState>) -> Result<(), FreeFetchError> {
    state.queue.clear_failed().await
}

#[tauri::command]
pub fn queue_set_concurrency(
    concurrency: usize,
    state: State<'_, AppState>,
) -> Result<(), FreeFetchError> {
    state.queue.set_concurrency(concurrency)
}

#[tauri::command]
pub fn get_queue_summary(state: State<'_, AppState>) -> Result<QueueSummary, FreeFetchError> {
    state.queue.get_summary()
}

#[tauri::command]
pub fn parse_batch_urls_text(raw_text: String) -> Result<Vec<String>, FreeFetchError> {
    Ok(parse_batch_urls(&raw_text))
}

#[tauri::command]
pub async fn analyze_batch_urls(
    urls: Vec<String>,
    state: State<'_, AppState>,
) -> Result<BatchAnalysisResult, FreeFetchError> {
    Ok(service_analyze_batch_urls(&state.registry, &urls, 3).await)
}

#[tauri::command]
pub async fn add_batch_to_queue(
    items: Vec<StartDownloadParams>,
    state: State<'_, AppState>,
) -> Result<BatchQueueResult, FreeFetchError> {
    service_add_batch_to_queue(&state.queue, items).await
}

#[tauri::command]
pub fn detect_playlist_url(url: String) -> Result<Option<PlaylistDetection>, FreeFetchError> {
    Ok(service_detect_playlist_url(&url))
}

#[tauri::command]
pub async fn analyze_playlist(url: String) -> Result<PlaylistInfo, FreeFetchError> {
    discover_playlist(&url).await
}

#[tauri::command]
pub async fn add_playlist_to_queue(
    items: Vec<PlaylistItemSelection>,
    quality: String,
    state: State<'_, AppState>,
) -> Result<BatchQueueResult, FreeFetchError> {
    let settings = {
        let s = state.settings.lock().unwrap();
        s.clone()
    };
    queue_playlist_items(
        &state.registry,
        &state.queue,
        items,
        quality,
        Some(settings.download_directory),
    )
    .await
}

#[tauri::command]
pub fn get_download_history(
    filter: HistoryFilter,
    state: State<'_, AppState>,
) -> Result<HistoryPage, FreeFetchError> {
    state.repo.get_history(&filter)
}

#[tauri::command]
pub fn get_download_history_item(
    id: String,
    state: State<'_, AppState>,
) -> Result<Option<DownloadRecord>, FreeFetchError> {
    state.repo.get_download(&id)
}

#[tauri::command]
pub fn delete_download_history_item(
    id: String,
    delete_file: Option<bool>,
    state: State<'_, AppState>,
) -> Result<(), FreeFetchError> {
    if delete_file.unwrap_or(false) {
        if let Ok(Some(record)) = state.repo.get_download(&id) {
            let path = std::path::Path::new(&record.destination_path);
            if path.exists() {
                let _ = std::fs::remove_file(path);
            }
        }
    }
    state.repo.delete_download(&id)
}

#[tauri::command]
pub fn clear_download_history(
    status_filter: Option<String>,
    state: State<'_, AppState>,
) -> Result<usize, FreeFetchError> {
    state.repo.clear_history(status_filter.as_deref())
}

#[tauri::command]
pub fn get_download_statistics(
    state: State<'_, AppState>,
) -> Result<DownloadStatistics, FreeFetchError> {
    state.repo.get_statistics()
}

#[tauri::command]
pub fn export_download_history(
    format: String,
    state: State<'_, AppState>,
) -> Result<String, FreeFetchError> {
    state.repo.export_history(&format)
}

#[tauri::command]
pub async fn retry_history_download(
    id: String,
    state: State<'_, AppState>,
) -> Result<String, FreeFetchError> {
    // 1. If item exists in queue and is failed or cancelled, use queue_retry
    if let Ok(Some(item)) = state.repo.get_queue_item(&id) {
        if matches!(
            item.status,
            crate::core::types::QueueStatus::Failed | crate::core::types::QueueStatus::Cancelled
        ) {
            state.queue.retry_item(&id).await?;
            return Ok(id);
        }
        // If completed or other status, reuse stored params_json to create new queue item
        if let Ok(params) = serde_json::from_str::<StartDownloadParams>(&item.params_json) {
            let new_item = state.queue.add(params).await?;
            return Ok(new_item.id);
        }
    }

    // 2. Fallback: look up in downloads table and re-resolve via extractor
    let record = state
        .repo
        .get_download(&id)?
        .ok_or_else(|| FreeFetchError::NotFound(id.clone()))?;

    let parsed_url =
        Url::parse(&record.source_url).map_err(|e| FreeFetchError::InvalidUrl(e.to_string()))?;
    let extractor =
        state
            .registry
            .find_extractor(&parsed_url)
            .ok_or_else(|| FreeFetchError::Extractor {
                service: record.service_name.clone(),
                message: "No compatible extractor found for this URL".to_string(),
            })?;
    let (_metadata, manifest) = extractor
        .extract(&parsed_url, &ExtractionContext::default())
        .await?;

    let is_audio = record.media_type() == "audio";
    let selected_format = if is_audio {
        manifest
            .formats
            .iter()
            .find(|f| f.format_type == "audio")
            .or_else(|| manifest.formats.iter().find(|f| !f.download_url.is_empty()))
            .ok_or_else(|| FreeFetchError::Extractor {
                service: record.service_name.clone(),
                message: "No audio stream found".to_string(),
            })?
    } else {
        manifest
            .formats
            .iter()
            .find(|f| {
                f.quality_label == record.quality_label || f.format_id == record.quality_label
            })
            .or_else(|| {
                manifest
                    .formats
                    .iter()
                    .find(|f| f.format_type == "video" || f.format_type == "muxed")
            })
            .or_else(|| manifest.formats.first())
            .ok_or_else(|| FreeFetchError::Extractor {
                service: record.service_name.clone(),
                message: "No suitable format found".to_string(),
            })?
    };

    let audio_options = if is_audio {
        let target_format =
            AudioTargetFormat::from_str_loose(&record.container).unwrap_or(AudioTargetFormat::Mp3);
        Some(AudioConversionOptions {
            target_format,
            quality_preset: AudioQualityPreset::Best,
            bitrate_mode: BitrateMode::Vbr,
            custom_bitrate_kbps: None,
            sample_rate: None,
            embed_metadata: true,
            embed_artwork: true,
            filename_template: None,
        })
    } else {
        None
    };

    let settings = {
        let s = state.settings.lock().unwrap();
        s.clone()
    };

    let params = StartDownloadParams {
        source_url: record.source_url.clone(),
        service_name: record.service_name.clone(),
        title: record.title.clone(),
        duration: record.duration,
        thumbnail_url: record.thumbnail_url.clone(),
        format_id: selected_format.format_id.clone(),
        download_url: selected_format.download_url.clone(),
        requires_merge: selected_format.requires_merge,
        matching_audio_url: selected_format.matching_audio_url.clone(),
        container: record.container.clone(),
        quality_label: record.quality_label.clone(),
        destination_dir: Some(settings.download_directory),
        selected_subtitles: Vec::new(),
        embed_subtitles: false,
        subtitle_mode: None,
        audio_options,
        video_codec: selected_format.video_codec.clone(),
        audio_codec: selected_format.audio_codec.clone(),
    };

    let new_item = state.queue.add(params).await?;
    Ok(new_item.id)
}
