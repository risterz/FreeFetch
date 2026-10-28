use serde::Deserialize;
use std::process::Stdio;
use std::sync::Arc;
use tokio::sync::Semaphore;
use url::Url;

use crate::core::error::FreeFetchError;
use crate::core::types::{
    BatchQueueResult, ItemAvailability, PlaylistDetection, PlaylistInfo, PlaylistItem,
    PlaylistItemSelection,
};
use crate::engine::queue::QueueManager;
use crate::engine::StartDownloadParams;
use crate::extractor::implementations::youtube::extract_youtube_id;
use crate::extractor::{ExtractionContext, ExtractorRegistry, MediaFormat};
use crate::media::audio::{
    AudioConversionOptions, AudioQualityPreset, AudioTargetFormat, BitrateMode,
};

/// Checks if a raw URL represents or contains a YouTube playlist.
pub fn detect_playlist_url(raw_url: &str) -> Option<PlaylistDetection> {
    let parsed = Url::parse(raw_url.trim()).ok()?;
    let host = parsed.host_str()?.to_lowercase();
    if !host.contains("youtube.com") && !host.contains("youtu.be") {
        return None;
    }

    let mut list_id = None;
    for (k, v) in parsed.query_pairs() {
        if k == "list" && !v.trim().is_empty() {
            let candidate = v.trim();
            // Validate playlist ID characters
            if candidate
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            {
                list_id = Some(candidate.to_string());
                break;
            }
        }
    }

    let playlist_id = list_id?;
    let path = parsed.path().to_lowercase();
    let is_pure = path == "/playlist" || path == "/playlist/";
    let video_id = extract_youtube_id(&parsed);

    Some(PlaylistDetection {
        playlist_id: playlist_id.clone(),
        is_pure_playlist: is_pure,
        video_id,
        canonical_playlist_url: format!("https://www.youtube.com/playlist?list={playlist_id}"),
    })
}

// ---------------------------------------------------------------------------
// yt-dlp Flat Playlist JSON Model
// ---------------------------------------------------------------------------
#[derive(Deserialize, Debug)]
struct YtDlpPlaylistDump {
    id: String,
    title: Option<String>,
    uploader: Option<String>,
    thumbnails: Option<Vec<YtDlpThumbnail>>,
    entries: Option<Vec<YtDlpPlaylistEntry>>,
}

#[derive(Deserialize, Debug)]
struct YtDlpThumbnail {
    url: Option<String>,
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
struct YtDlpPlaylistEntry {
    id: Option<String>,
    title: Option<String>,
    duration: Option<f64>,
    thumbnails: Option<Vec<YtDlpThumbnail>>,
    url: Option<String>,
}

/// Discovers playlist metadata and items using yt-dlp flat-playlist or clean-room fallback.
pub async fn discover_playlist(raw_url: &str) -> Result<PlaylistInfo, FreeFetchError> {
    let detection = detect_playlist_url(raw_url).ok_or_else(|| {
        FreeFetchError::InvalidUrl(format!("Not a valid playlist URL: {raw_url}"))
    })?;

    let canonical = detection.canonical_playlist_url.clone();

    // Method 1: yt-dlp flat-playlist discovery
    if let Ok(info) = try_discover_with_ytdlp(&canonical, &detection.playlist_id).await {
        if !info.items.is_empty() {
            return Ok(info);
        }
    }

    // Method 2: Clean-room YouTube Innertube browse API fallback
    if let Ok(info) = try_discover_with_innertube(&detection.playlist_id, &canonical).await {
        return Ok(info);
    }

    Err(FreeFetchError::Extractor {
        service: "youtube".to_string(),
        message: format!("Unable to discover playlist for {}", detection.playlist_id),
    })
}

async fn try_discover_with_ytdlp(
    playlist_url: &str,
    playlist_id: &str,
) -> Result<PlaylistInfo, FreeFetchError> {
    let mut cmd = crate::core::hidden_tokio_command("yt-dlp");
    cmd.arg("--flat-playlist")
        .arg("--dump-single-json")
        .arg("--no-warnings")
        .arg("--skip-download")
        .arg(playlist_url)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let output = cmd
        .output()
        .await
        .map_err(|e| FreeFetchError::Io(e.to_string()))?;

    if !output.status.success() {
        let err_log = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(FreeFetchError::Extractor {
            service: "youtube".to_string(),
            message: err_log,
        });
    }

    let dump: YtDlpPlaylistDump =
        serde_json::from_slice(&output.stdout).map_err(|e| FreeFetchError::Extractor {
            service: "youtube".to_string(),
            message: format!("Failed to parse playlist yt-dlp JSON: {e}"),
        })?;

    let mut items = Vec::new();
    let raw_entries = dump.entries.unwrap_or_default();

    for (idx, entry) in raw_entries.into_iter().enumerate() {
        let video_id = match entry.id {
            Some(id) if !id.trim().is_empty() => id,
            _ => continue,
        };

        let raw_title = entry.title.unwrap_or_else(|| format!("Video {idx}"));
        let availability = if raw_title.contains("[Private video]") {
            ItemAvailability::Private
        } else if raw_title.contains("[Deleted video]") {
            ItemAvailability::Deleted
        } else if raw_title.contains("[Unavailable video]") {
            ItemAvailability::Unavailable
        } else {
            ItemAvailability::Available
        };

        let thumb = entry
            .thumbnails
            .as_ref()
            .and_then(|t| t.last())
            .and_then(|t| t.url.clone())
            .or_else(|| Some(format!("https://i.ytimg.com/vi/{video_id}/hqdefault.jpg")));

        items.push(PlaylistItem {
            id: video_id.clone(),
            index: (idx + 1) as u32,
            url: format!("https://www.youtube.com/watch?v={video_id}"),
            canonical_url: format!("https://www.youtube.com/watch?v={video_id}"),
            title: raw_title,
            thumbnail: thumb,
            duration: entry.duration.unwrap_or(0.0) as u64,
            availability,
        });
    }

    let playlist_thumb = dump
        .thumbnails
        .as_ref()
        .and_then(|t| t.last())
        .and_then(|t| t.url.clone())
        .or_else(|| items.first().and_then(|i| i.thumbnail.clone()));

    Ok(PlaylistInfo {
        id: dump.id,
        canonical_url: format!("https://www.youtube.com/playlist?list={playlist_id}"),
        title: dump.title.unwrap_or_else(|| "YouTube Playlist".to_string()),
        thumbnail: playlist_thumb,
        uploader: dump.uploader,
        item_count: items.len(),
        items,
    })
}

/// Clean-room Innertube WEB client fallback for YouTube playlists.
async fn try_discover_with_innertube(
    playlist_id: &str,
    canonical_url: &str,
) -> Result<PlaylistInfo, FreeFetchError> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .unwrap_or_default();

    let browse_id = if playlist_id.starts_with("VL") {
        playlist_id.to_string()
    } else {
        format!("VL{playlist_id}")
    };

    let payload = serde_json::json!({
        "context": {
            "client": {
                "clientName": "WEB",
                "clientVersion": "2.20240101.00.00",
                "hl": "en",
                "gl": "US"
            }
        },
        "browseId": browse_id
    });

    let resp = client
        .post("https://www.youtube.com/youtubei/v1/browse")
        .header("Content-Type", "application/json")
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
        .json(&payload)
        .send()
        .await
        .map_err(|e| FreeFetchError::Network(format!("Innertube browse failed: {e}")))?;

    if !resp.status().is_success() {
        return Err(FreeFetchError::Network(format!(
            "Innertube browse returned HTTP {}",
            resp.status()
        )));
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| FreeFetchError::Generic(format!("Failed to parse browse JSON: {e}")))?;

    // Extract title from header or metadata
    let title = json
        .pointer("/header/playlistHeaderRenderer/title/simpleText")
        .or_else(|| json.pointer("/metadata/playlistMetadataRenderer/title"))
        .and_then(|v| v.as_str())
        .unwrap_or("YouTube Playlist")
        .to_string();

    let uploader = json
        .pointer("/header/playlistHeaderRenderer/ownerText/runs/0/text")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let playlist_thumb = json
        .pointer("/header/playlistHeaderRenderer/playlistHeaderBanner/thumbnails/0/url")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let mut items = Vec::new();

    // Navigate to playlistVideoListRenderer contents
    let empty_vec = Vec::new();
    let contents = json
        .pointer("/contents/twoColumnBrowseResultsRenderer/tabs/0/tabRenderer/content/sectionListRenderer/contents/0/itemSectionRenderer/contents/0/playlistVideoListRenderer/contents")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty_vec);

    for (idx, entry) in contents.iter().enumerate() {
        let renderer = match entry.get("playlistVideoRenderer") {
            Some(r) => r,
            None => continue,
        };

        let video_id = match renderer.get("videoId").and_then(|v| v.as_str()) {
            Some(id) => id.to_string(),
            None => continue,
        };

        let raw_title = renderer
            .pointer("/title/runs/0/text")
            .and_then(|v| v.as_str())
            .unwrap_or("Untitled")
            .to_string();

        let is_playable = renderer
            .get("isPlayable")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let availability = if raw_title.contains("[Private video]") {
            ItemAvailability::Private
        } else if raw_title.contains("[Deleted video]") {
            ItemAvailability::Deleted
        } else if !is_playable {
            ItemAvailability::Unavailable
        } else {
            ItemAvailability::Available
        };

        let duration: u64 = renderer
            .get("lengthSeconds")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        let thumb = renderer
            .pointer("/thumbnail/thumbnails/0/url")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| Some(format!("https://i.ytimg.com/vi/{video_id}/hqdefault.jpg")));

        items.push(PlaylistItem {
            id: video_id.clone(),
            index: (idx + 1) as u32,
            url: format!("https://www.youtube.com/watch?v={video_id}"),
            canonical_url: format!("https://www.youtube.com/watch?v={video_id}"),
            title: raw_title,
            thumbnail: thumb,
            duration,
            availability,
        });
    }

    Ok(PlaylistInfo {
        id: playlist_id.to_string(),
        canonical_url: canonical_url.to_string(),
        title,
        thumbnail: playlist_thumb.or_else(|| items.first().and_then(|i| i.thumbnail.clone())),
        uploader,
        item_count: items.len(),
        items,
    })
}

/// Selects the best matching format given a quality preference.
///
/// Supported quality preferences:
/// - "audio" / "Audio Only": chooses audio-only stream
/// - "best" / "Best Quality": highest resolution video stream
/// - "1080p": exactly 1080p if available, else highest available <= 1080p
/// - "720p": exactly 720p if available, else highest available <= 720p
/// - "480p": exactly 480p if available, else highest available <= 480p
pub fn select_format_for_quality(
    formats: &[MediaFormat],
    quality_pref: &str,
) -> Option<MediaFormat> {
    if formats.is_empty() {
        return None;
    }

    let pref = quality_pref.trim().to_lowercase();

    if pref.starts_with("audio")
        || pref.contains("mp3")
        || pref.contains("m4a")
        || pref.contains("flac")
        || pref.contains("opus")
    {
        return formats
            .iter()
            .filter(|f| f.format_type == "audio")
            .max_by_key(|f| f.bitrate.unwrap_or(0))
            .cloned()
            .or_else(|| formats.iter().find(|f| f.format_type == "audio").cloned())
            .or_else(|| {
                formats
                    .iter()
                    .find(|f| f.matching_audio_url.is_some())
                    .cloned()
            })
            .or_else(|| formats.first().cloned());
    }

    if pref == "best" || pref == "best quality" {
        // Video streams sorted by height descending
        let mut video_fmts: Vec<&MediaFormat> = formats
            .iter()
            .filter(|f| f.format_type != "audio")
            .collect();
        video_fmts.sort_by_key(|f| f.height.unwrap_or(0));
        return video_fmts
            .last()
            .cloned()
            .cloned()
            .or_else(|| formats.first().cloned());
    }

    let target_height: u32 = match pref.as_str() {
        "1080p" | "1080" => 1080,
        "720p" | "720" => 720,
        "480p" | "480" => 480,
        "360p" | "360" => 360,
        _ => 720,
    };

    // 1. Try exact quality label match
    if let Some(f) = formats.iter().find(|f| {
        f.format_type != "audio"
            && f.quality_label
                .to_lowercase()
                .starts_with(&format!("{target_height}p"))
    }) {
        return Some(f.clone());
    }

    // 2. Try height match
    if let Some(f) = formats
        .iter()
        .find(|f| f.format_type != "audio" && f.height.map(|h| h == target_height).unwrap_or(false))
    {
        return Some(f.clone());
    }

    // 3. Fallback to closest available <= target_height
    let mut candidates: Vec<&MediaFormat> = formats
        .iter()
        .filter(|f| f.format_type != "audio" && f.height.unwrap_or(0) <= target_height)
        .collect();
    candidates.sort_by_key(|f| f.height.unwrap_or(0));
    if let Some(closest) = candidates.last() {
        return Some((*closest).clone());
    }

    // 4. Fallback to any video format or first format
    formats
        .iter()
        .find(|f| f.format_type != "audio")
        .cloned()
        .or_else(|| formats.first().cloned())
}

/// Resolves selected playlist items with bounded concurrency and enqueues them preserving order.
pub async fn queue_playlist_items(
    registry: &Arc<ExtractorRegistry>,
    queue: &QueueManager,
    items: Vec<PlaylistItemSelection>,
    quality_pref: String,
    destination_dir: Option<String>,
) -> Result<BatchQueueResult, FreeFetchError> {
    let semaphore = Arc::new(Semaphore::new(3)); // Bounded concurrency of 3
    let mut resolved_params = Vec::with_capacity(items.len());

    let mut tasks = Vec::with_capacity(items.len());

    for item in items {
        let sem = semaphore.clone();
        let reg = registry.clone();
        let q_pref = quality_pref.clone();
        let dest_dir = destination_dir.clone();

        let task = tokio::spawn(async move {
            let _permit = match sem.acquire().await {
                Ok(p) => p,
                Err(_) => {
                    return Err(FreeFetchError::Generic(
                        "Semaphore permit error".to_string(),
                    ))
                }
            };

            let parsed_url = Url::parse(&item.url).map_err(|e| {
                FreeFetchError::InvalidUrl(format!("Invalid URL for item {}: {e}", item.id))
            })?;

            let extractor =
                reg.find_extractor(&parsed_url)
                    .ok_or_else(|| FreeFetchError::Extractor {
                        service: "generic".to_string(),
                        message: "No extractor found".to_string(),
                    })?;

            let ctx = ExtractionContext::default();
            let (meta, manifest) = extractor.extract(&parsed_url, &ctx).await?;

            // Choose format matching preferred quality or explicitly selected format_id
            let selected_fmt = if let Some(ref f_id) = item.format_id {
                manifest
                    .formats
                    .iter()
                    .find(|f| &f.format_id == f_id)
                    .cloned()
                    .or_else(|| select_format_for_quality(&manifest.formats, &q_pref))
            } else {
                select_format_for_quality(&manifest.formats, &q_pref)
            }
            .ok_or_else(|| {
                FreeFetchError::Generic(format!("No usable format found for {}", meta.title))
            })?;

            let mut resolved_subs = Vec::new();
            for req_sub in &item.selected_subtitles {
                if let Some(found_track) = manifest
                    .subtitles
                    .iter()
                    .find(|s| s.language_code == req_sub.language_code)
                {
                    resolved_subs.push(crate::core::types::SubtitleSelection {
                        language_code: found_track.language_code.clone(),
                        language_name: found_track.language_name.clone(),
                        target_format: req_sub.target_format,
                        is_auto_generated: found_track.is_auto_generated,
                        download_url: found_track.download_url.clone(),
                    });
                }
            }

            let audio_opts = if let Some(target_fmt) = AudioTargetFormat::from_str_loose(&q_pref) {
                Some(AudioConversionOptions {
                    target_format: target_fmt,
                    quality_preset: AudioQualityPreset::Best,
                    bitrate_mode: BitrateMode::Cbr,
                    custom_bitrate_kbps: None,
                    sample_rate: None,
                    embed_metadata: true,
                    embed_artwork: true,
                    filename_template: Some("{artist} - {title}.{ext}".to_string()),
                })
            } else if q_pref.to_lowercase().starts_with("audio") {
                Some(AudioConversionOptions {
                    target_format: AudioTargetFormat::Mp3,
                    quality_preset: AudioQualityPreset::Best,
                    bitrate_mode: BitrateMode::Cbr,
                    custom_bitrate_kbps: None,
                    sample_rate: None,
                    embed_metadata: true,
                    embed_artwork: true,
                    filename_template: Some("{artist} - {title}.{ext}".to_string()),
                })
            } else {
                None
            };

            let container = if let Some(ref opts) = audio_opts {
                opts.target_format.extension().to_string()
            } else {
                selected_fmt.container.clone()
            };

            let quality_label = if let Some(ref opts) = audio_opts {
                format!("{} (Audio)", opts.target_format.extension().to_uppercase())
            } else {
                selected_fmt.quality_label.clone()
            };

            Ok(StartDownloadParams {
                source_url: meta.url,
                service_name: meta.service_name,
                title: meta.title,
                duration: meta.duration,
                thumbnail_url: meta.thumbnail_url,
                format_id: selected_fmt.format_id.clone(),
                download_url: selected_fmt.download_url.clone(),
                requires_merge: selected_fmt.requires_merge,
                matching_audio_url: selected_fmt.matching_audio_url.clone(),
                container,
                quality_label,
                destination_dir: dest_dir,
                selected_subtitles: resolved_subs,
                embed_subtitles: false,
                subtitle_mode: None,
                audio_options: audio_opts,
                video_codec: selected_fmt.video_codec.clone(),
                audio_codec: selected_fmt.audio_codec.clone(),
            })
        });

        tasks.push(task);
    }

    let mut failed_count = 0;
    let mut errors = Vec::new();

    // Await all tasks in order to preserve playlist positions
    for task in tasks {
        match task.await {
            Ok(Ok(params)) => resolved_params.push(params),
            Ok(Err(e)) => {
                failed_count += 1;
                errors.push(e.to_string());
            }
            Err(e) => {
                failed_count += 1;
                errors.push(format!("Task panic: {e}"));
            }
        }
    }

    // Now add resolved items to QueueManager
    let mut batch_res = crate::service::batch::add_batch_to_queue(queue, resolved_params).await?;
    batch_res.failed_count += failed_count;
    batch_res.errors.extend(errors);

    Ok(batch_res)
}
