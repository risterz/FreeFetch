use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;
use url::Url;

use crate::core::error::FreeFetchError;
use crate::core::types::{SubtitleFormat, SubtitleTrack};
use crate::extractor::{
    context::ExtractionContext,
    models::{MediaFormat, MediaMetadata, StreamManifest},
    Extractor,
};

pub struct YoutubeExtractor;

#[derive(Deserialize, Debug)]
struct OEmbedResponse {
    title: String,
    _author_name: Option<String>,
    _thumbnail_url: Option<String>,
}

pub use crate::media::ytdlp::{YtDlpController, YtDlpDump, YtDlpFormat, YtDlpSubtitleTrack};

#[async_trait]
impl Extractor for YoutubeExtractor {
    fn name(&self) -> &'static str {
        "youtube"
    }

    fn can_handle(&self, url: &Url) -> bool {
        let host = url.host_str().unwrap_or("").to_lowercase();
        if !host.contains("youtube.com") && !host.contains("youtu.be") {
            return false;
        }
        extract_youtube_id(url).is_some()
    }

    async fn extract(
        &self,
        url: &Url,
        ctx: &ExtractionContext,
    ) -> Result<(MediaMetadata, StreamManifest), FreeFetchError> {
        let video_id = extract_youtube_id(url).ok_or_else(|| {
            FreeFetchError::InvalidUrl(
                "Could not extract a valid 11-character video ID from YouTube URL".to_string(),
            )
        })?;

        let canonical_url = format!("https://www.youtube.com/watch?v={video_id}");
        let mut last_failure_reason = None;

        // Attempt Method 1: yt-dlp discovery and execution
        match try_extract_with_ytdlp(
            &canonical_url,
            ctx.ytdlp_path.as_deref(),
            ctx.proxy.as_deref(),
        )
        .await
        {
            Ok(dump) => return normalize_ytdlp_dump(dump, &canonical_url),
            Err(e) => {
                if let FreeFetchError::Extractor { message, .. } = e {
                    let cleaned = if let Some(line) = message
                        .lines()
                        .find(|l| l.contains("ERROR:") || l.contains("Sign in"))
                    {
                        line.trim().to_string()
                    } else {
                        message.trim().to_string()
                    };
                    last_failure_reason = Some(cleaned);
                }
            }
        }

        // Attempt Method 2: Innertube ANDROID_VR client direct extraction
        match try_extract_with_innertube(ctx, &video_id, &canonical_url).await {
            Ok((meta, manifest)) => {
                if !manifest.formats.is_empty() {
                    return Ok((meta, manifest));
                }
            }
            Err(e) => {
                if let FreeFetchError::Extractor { message, .. } = e {
                    let cleaned = if let Some(line) = message
                        .lines()
                        .find(|l| l.contains("returned:") || l.contains("Sign in"))
                    {
                        line.trim().to_string()
                    } else {
                        message.trim().to_string()
                    };
                    if last_failure_reason.is_none() || cleaned.contains("Sign in") {
                        last_failure_reason = Some(cleaned);
                    }
                }
            }
        }

        // Attempt Method 3: Public oEmbed metadata
        let oembed_url = format!(
            "https://www.youtube.com/oembed?url=https://www.youtube.com/watch?v={video_id}&format=json"
        );
        let resp = ctx
            .client
            .get(&oembed_url)
            .header("User-Agent", &ctx.user_agent)
            .send()
            .await
            .map_err(|e| FreeFetchError::Network(e.to_string()))?;

        if resp.status().is_success() {
            if let Ok(data) = resp.json::<OEmbedResponse>().await {
                let reason_suffix = if let Some(ref r) = last_failure_reason {
                    format!(": {r}")
                } else {
                    " without authentication/cipher deciphering.".to_string()
                };
                return Err(FreeFetchError::Extractor {
                    service: "youtube".to_string(),
                    message: format!(
                        "Metadata resolved for '{}', but no downloadable streams could be resolved{}",
                        data.title, reason_suffix
                    ),
                });
            }
        } else if resp.status().as_u16() == 404 {
            return Err(FreeFetchError::Extractor {
                service: "youtube".to_string(),
                message: "Video is private, removed, or unavailable".to_string(),
            });
        }

        Err(FreeFetchError::Extractor {
            service: "youtube".to_string(),
            message: last_failure_reason
                .unwrap_or_else(|| "Failed to extract streams for YouTube video".to_string()),
        })
    }
}

pub fn extract_youtube_id(url: &Url) -> Option<String> {
    let host = url.host_str()?.to_lowercase();
    if host.contains("youtu.be") {
        let segments: Vec<&str> = url.path_segments()?.collect();
        if let Some(first) = segments.first() {
            let candidate = first.trim();
            if is_valid_youtube_id(candidate) {
                return Some(candidate.to_string());
            }
        }
        return None;
    }

    if host.contains("youtube.com") {
        let path = url.path();
        if path.starts_with("/watch") {
            if let Some((_, v)) = url.query_pairs().find(|(k, _)| k == "v") {
                let candidate = v.trim();
                if is_valid_youtube_id(candidate) {
                    return Some(candidate.to_string());
                }
            }
            return None;
        }

        let segments: Vec<&str> = url.path_segments()?.collect();
        if segments.len() >= 2
            && (segments[0] == "shorts" || segments[0] == "embed" || segments[0] == "live")
        {
            let candidate = segments[1].trim();
            if is_valid_youtube_id(candidate) {
                return Some(candidate.to_string());
            }
        }
    }
    None
}

pub fn is_valid_youtube_id(id: &str) -> bool {
    if id.len() != 11 {
        return false;
    }
    id.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

async fn try_extract_with_ytdlp(
    url: &str,
    custom_path: Option<&str>,
    proxy: Option<&str>,
) -> Result<YtDlpDump, FreeFetchError> {
    let controller = YtDlpController::discover(custom_path)?;
    controller.extract_json(url, proxy).await
}

fn normalize_ytdlp_dump(
    dump: YtDlpDump,
    canonical_url: &str,
) -> Result<(MediaMetadata, StreamManifest), FreeFetchError> {
    let metadata = MediaMetadata {
        id: dump.id.clone(),
        url: canonical_url.to_string(),
        title: dump.title,
        duration: dump.duration.unwrap_or(0.0) as u64,
        thumbnail_url: dump.thumbnail,
        uploader: dump.artist.or(dump.uploader).or(dump.channel),
        service_name: "youtube".to_string(),
        description: dump.description,
        upload_date: dump.release_date.or(dump.upload_date),
    };

    let raw_formats = dump.formats.unwrap_or_default();

    let is_direct_stream = |f: &YtDlpFormat| {
        let url_ok = f
            .url
            .as_ref()
            .map(|u| u.starts_with("http") && !u.contains(".m3u8"))
            .unwrap_or(false);
        let proto_ok = match f.protocol.as_deref() {
            Some(p) => (p == "https" || p == "http") && !p.contains("m3u8"),
            None => true,
        };
        let ext_ok = match f.ext.as_deref() {
            Some(e) => e != "mhtml" && e != "m3u8",
            None => true,
        };
        url_ok && proto_ok && ext_ok
    };

    // 1. Find best compatible audio streams for multiplexing based on container format
    let (best_m4a_audio_url, best_m4a_acodec) = raw_formats
        .iter()
        .filter(|f| {
            let vcodec = f.vcodec.as_deref().unwrap_or("none");
            let acodec = f.acodec.as_deref().unwrap_or("none");
            let ext = f.ext.as_deref().unwrap_or("");
            is_direct_stream(f)
                && vcodec == "none"
                && acodec != "none"
                && (ext == "m4a" || acodec.starts_with("mp4a") || acodec == "aac")
        })
        .max_by_key(|f| f.abr.or(f.tbr).unwrap_or(0.0) as u64)
        .map(|f| (f.url.clone(), f.acodec.clone()))
        .unwrap_or((None, None));

    let (best_webm_audio_url, best_webm_acodec) = raw_formats
        .iter()
        .filter(|f| {
            let vcodec = f.vcodec.as_deref().unwrap_or("none");
            let acodec = f.acodec.as_deref().unwrap_or("none");
            let ext = f.ext.as_deref().unwrap_or("");
            is_direct_stream(f)
                && vcodec == "none"
                && acodec != "none"
                && (ext == "webm" || acodec.starts_with("opus") || acodec.starts_with("vorbis"))
        })
        .max_by_key(|f| f.abr.or(f.tbr).unwrap_or(0.0) as u64)
        .map(|f| (f.url.clone(), f.acodec.clone()))
        .unwrap_or((None, None));

    let (best_any_audio_url, best_any_acodec) = raw_formats
        .iter()
        .filter(|f| {
            let vcodec = f.vcodec.as_deref().unwrap_or("none");
            let acodec = f.acodec.as_deref().unwrap_or("none");
            is_direct_stream(f) && vcodec == "none" && acodec != "none"
        })
        .max_by_key(|f| f.abr.or(f.tbr).unwrap_or(0.0) as u64)
        .map(|f| (f.url.clone(), f.acodec.clone()))
        .unwrap_or((None, None));

    let mut normalized_formats = Vec::new();

    for f in raw_formats {
        if !is_direct_stream(&f) {
            continue;
        }

        let download_url = match f.url {
            Some(ref u) => u.clone(),
            _ => continue,
        };

        let ext = f.ext.unwrap_or_else(|| "mp4".to_string());
        if ext == "mhtml" {
            continue; // Ignore storyboards
        }

        let vcodec = f.vcodec.unwrap_or_else(|| "none".to_string());
        let acodec = f.acodec.unwrap_or_else(|| "none".to_string());

        if vcodec == "none" && acodec == "none" {
            continue;
        }

        let is_video_only = vcodec != "none" && acodec == "none";
        let is_audio_only = acodec != "none" && vcodec == "none";
        let is_muxed = vcodec != "none" && acodec != "none";

        let format_type = if is_muxed {
            "muxed".to_string()
        } else if is_video_only {
            "video".to_string()
        } else {
            "audio".to_string()
        };

        let height = f.height;
        let width = f.width;
        let fps = f.fps.map(|fps| fps.round() as u32);
        let bitrate = f.tbr.or(f.vbr).or(f.abr).map(|k| (k * 1000.0) as u64);
        let filesize = f.filesize.or(f.filesize_approx);

        let quality_label = if let Some(h) = height {
            if h >= 2160 {
                "2160p 4K".to_string()
            } else if h >= 1440 {
                "1440p 2K".to_string()
            } else if h >= 1080 {
                "1080p FHD".to_string()
            } else if h >= 720 {
                "720p HD".to_string()
            } else if h >= 480 {
                "480p SD".to_string()
            } else {
                format!("{h}p")
            }
        } else if is_audio_only {
            if let Some(abr) = f.abr {
                format!("{:.0} kbps Audio", abr)
            } else {
                "Audio Stream".to_string()
            }
        } else {
            f.format_note
                .or(f.resolution)
                .unwrap_or_else(|| "Standard Quality".to_string())
        };

        let resolution = height.map(|h| {
            if let Some(w) = width {
                format!("{w}x{h}")
            } else {
                format!("{h}p")
            }
        });

        let requires_merge = is_video_only;
        let is_webm = ext.eq_ignore_ascii_case("webm");
        let (matching_audio_url, paired_acodec) = if requires_merge {
            if is_webm {
                (
                    best_webm_audio_url
                        .clone()
                        .or_else(|| best_any_audio_url.clone()),
                    best_webm_acodec.clone().or_else(|| best_any_acodec.clone()),
                )
            } else {
                (
                    best_m4a_audio_url
                        .clone()
                        .or_else(|| best_any_audio_url.clone()),
                    best_m4a_acodec.clone().or_else(|| best_any_acodec.clone()),
                )
            }
        } else {
            (None, None)
        };

        normalized_formats.push(MediaFormat {
            format_id: f.format_id,
            format_type,
            quality_label,
            resolution,
            width,
            height,
            fps,
            video_codec: if vcodec != "none" { Some(vcodec) } else { None },
            audio_codec: if acodec != "none" {
                Some(acodec)
            } else {
                paired_acodec
            },
            mime_type: None,
            bitrate,
            container: ext,
            filesize,
            download_url,
            requires_merge,
            matching_audio_url,
        });
    }

    // Sort: Video/Muxed by height descending then bitrate descending. Audio by bitrate descending.
    normalized_formats.sort_by(|a, b| {
        let h_a = a.height.unwrap_or(0);
        let h_b = b.height.unwrap_or(0);
        if h_a != h_b {
            return h_b.cmp(&h_a);
        }
        let br_a = a.bitrate.unwrap_or(0);
        let br_b = b.bitrate.unwrap_or(0);
        br_b.cmp(&br_a)
    });

    if normalized_formats.is_empty() {
        return Err(FreeFetchError::Extractor {
            service: "youtube".to_string(),
            message: "No usable stream formats discovered for this video".to_string(),
        });
    }

    let normalized_subtitles = normalize_ytdlp_subtitles(
        dump.subtitles,
        dump.automatic_captions,
        dump.language.as_deref(),
    );

    Ok((
        metadata,
        StreamManifest {
            formats: normalized_formats,
            subtitles: normalized_subtitles,
        },
    ))
}

pub fn language_code_to_name(code: &str) -> String {
    let lower = code.to_lowercase();
    let base = lower.split('-').next().unwrap_or(&lower);
    match base {
        "en" => "English".to_string(),
        "de" => "German".to_string(),
        "es" => "Spanish".to_string(),
        "fr" => "French".to_string(),
        "it" => "Italian".to_string(),
        "ja" => "Japanese".to_string(),
        "ko" => "Korean".to_string(),
        "zh" => {
            if lower.contains("hant") || lower.contains("tw") || lower.contains("hk") {
                "Chinese (Traditional)".to_string()
            } else {
                "Chinese (Simplified)".to_string()
            }
        }
        "ru" => "Russian".to_string(),
        "pt" => "Portuguese".to_string(),
        "ms" => "Malay".to_string(),
        "id" => "Indonesian".to_string(),
        "hi" => "Hindi".to_string(),
        "ar" => "Arabic".to_string(),
        "th" => "Thai".to_string(),
        "vi" => "Vietnamese".to_string(),
        "tr" => "Turkish".to_string(),
        "nl" => "Dutch".to_string(),
        "pl" => "Polish".to_string(),
        "sv" => "Swedish".to_string(),
        "no" => "Norwegian".to_string(),
        "fi" => "Finnish".to_string(),
        "da" => "Danish".to_string(),
        "el" => "Greek".to_string(),
        "he" => "Hebrew".to_string(),
        "uk" => "Ukrainian".to_string(),
        other => {
            let mut c = other.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        }
    }
}

pub fn is_translated_caption_url(url: &str) -> bool {
    if let Ok(parsed) = url::Url::parse(url) {
        parsed.query_pairs().any(|(k, _)| k == "tlang")
    } else {
        url.contains("tlang=")
    }
}

pub fn normalize_ytdlp_subtitles(
    subtitles: Option<HashMap<String, Vec<YtDlpSubtitleTrack>>>,
    automatic_captions: Option<HashMap<String, Vec<YtDlpSubtitleTrack>>>,
    video_language: Option<&str>,
) -> Vec<SubtitleTrack> {
    let mut out = Vec::new();
    let mut seen_manual_langs = std::collections::HashSet::new();

    // 1. Manual captions first (preserve all legitimate manual tracks)
    if let Some(manual_map) = subtitles {
        for (lang_code, formats) in manual_map {
            if lang_code == "live_chat" {
                continue;
            }
            let chosen = formats
                .iter()
                .find(|f| f.ext.as_deref() == Some("vtt"))
                .or_else(|| formats.iter().find(|f| f.ext.as_deref() == Some("srt")))
                .or_else(|| formats.first());

            if let Some(track) = chosen {
                if let Some(ref url) = track.url {
                    let base_name = track
                        .name
                        .clone()
                        .unwrap_or_else(|| language_code_to_name(&lang_code));
                    let is_default = lang_code.starts_with("en");
                    seen_manual_langs.insert(lang_code.clone());

                    out.push(SubtitleTrack {
                        id: format!("yt-sub-{lang_code}-manual"),
                        language_code: lang_code.clone(),
                        language_name: base_name,
                        format: if track.ext.as_deref() == Some("srt") {
                            SubtitleFormat::Srt
                        } else {
                            SubtitleFormat::Vtt
                        },
                        is_auto_generated: false,
                        is_default,
                        download_url: Some(url.clone()),
                    });
                }
            }
        }
    }

    // 2. Auto-generated captions
    // In YouTube timedtext, only the original language ASR track is reliable.
    // Derived/translated auto-captions contain `tlang=` in their URL and fail with HTTP 429.
    // Provider tags original ASR with `<lang>-orig`.
    if let Some(auto_map) = automatic_captions {
        let has_orig_key = auto_map.keys().any(|k| k.to_lowercase().ends_with("-orig"));

        for (lang_code, formats) in auto_map {
            let is_orig_code = lang_code.to_lowercase().ends_with("-orig");

            // If any `-orig` key exists, ONLY accept `-orig` keys.
            // If no `-orig` key exists, accept tracks whose URL has NO `tlang=`.
            if has_orig_key {
                if !is_orig_code {
                    continue; // Skip bare duplicate (e.g. `id`) and all translated tracks (e.g. `iu`, `ga`)
                }
            } else {
                // If no `-orig` key was provided by yt-dlp, check video_language and URL
                if let Some(vlang) = video_language {
                    let base_code = lang_code.split('-').next().unwrap_or(&lang_code);
                    let base_vlang = vlang.split('-').next().unwrap_or(vlang);
                    if !base_code.eq_ignore_ascii_case(base_vlang) {
                        continue;
                    }
                }
            }

            let chosen = formats
                .iter()
                .find(|f| f.ext.as_deref() == Some("vtt"))
                .or_else(|| formats.iter().find(|f| f.ext.as_deref() == Some("srt")))
                .or_else(|| formats.first());

            if let Some(track) = chosen {
                if let Some(ref url) = track.url {
                    // Strictly filter out any track that has `tlang=` (machine-translated)
                    if is_translated_caption_url(url) {
                        continue;
                    }

                    let mut base_name = track
                        .name
                        .clone()
                        .unwrap_or_else(|| language_code_to_name(&lang_code));

                    if is_orig_code && !base_name.contains("(Original)") {
                        base_name = format!("{base_name} (Original)");
                    }

                    let display_name = if base_name.contains("(Auto-generated)") {
                        base_name
                    } else {
                        format!("{base_name} (Auto-generated)")
                    };

                    let is_default = out.is_empty() && lang_code.starts_with("en");

                    out.push(SubtitleTrack {
                        id: format!("yt-sub-{lang_code}-auto"),
                        language_code: lang_code.clone(),
                        language_name: display_name,
                        format: if track.ext.as_deref() == Some("srt") {
                            SubtitleFormat::Srt
                        } else {
                            SubtitleFormat::Vtt
                        },
                        is_auto_generated: true,
                        is_default,
                        download_url: Some(url.clone()),
                    });
                }
            }
        }
    }

    // Sort: default track first, then manual by language name, then auto-generated
    out.sort_by(|a, b| {
        if a.is_default != b.is_default {
            return b.is_default.cmp(&a.is_default);
        }
        if a.is_auto_generated != b.is_auto_generated {
            return a.is_auto_generated.cmp(&b.is_auto_generated);
        }
        a.language_name.cmp(&b.language_name)
    });

    out
}

async fn try_extract_with_innertube(
    ctx: &ExtractionContext,
    video_id: &str,
    canonical_url: &str,
) -> Result<(MediaMetadata, StreamManifest), FreeFetchError> {
    let body = serde_json::json!({
        "videoId": video_id,
        "context": {
            "client": {
                "clientName": "ANDROID_VR",
                "clientVersion": "1.60.19",
                "deviceMake": "Oculus",
                "deviceModel": "Quest 3",
                "osName": "Android",
                "osVersion": "12",
                "hl": "en",
                "gl": "US"
            }
        }
    });

    let resp = ctx
        .client
        .post("https://www.youtube.com/youtubei/v1/player")
        .header("Content-Type", "application/json")
        .header(
            "User-Agent",
            "Mozilla/5.0 (Android 12; Mobile VR; rv:105.0) Gecko/105.0 Firefox/105.0",
        )
        .json(&body)
        .send()
        .await
        .map_err(|e| FreeFetchError::Network(e.to_string()))?;

    let json_val: serde_json::Value = resp.json().await.map_err(|e| FreeFetchError::Extractor {
        service: "youtube".to_string(),
        message: format!("Failed to parse Innertube JSON: {e}"),
    })?;

    let playability = json_val
        .get("playabilityStatus")
        .and_then(|p| p.get("status"))
        .and_then(|s| s.as_str())
        .unwrap_or("");

    if playability != "OK" {
        let reason = json_val
            .get("playabilityStatus")
            .and_then(|p| p.get("reason"))
            .and_then(|r| r.as_str())
            .unwrap_or(playability);
        return Err(FreeFetchError::Extractor {
            service: "youtube".to_string(),
            message: format!("Video playability check returned: {reason}"),
        });
    }

    let video_details = json_val
        .get("videoDetails")
        .ok_or_else(|| FreeFetchError::Extractor {
            service: "youtube".to_string(),
            message: "Missing videoDetails in Innertube response".to_string(),
        })?;

    let title = video_details
        .get("title")
        .and_then(|t| t.as_str())
        .unwrap_or("YouTube Video")
        .to_string();

    let author = video_details
        .get("author")
        .and_then(|a| a.as_str())
        .map(|s| s.to_string());

    let duration_sec = video_details
        .get("lengthSeconds")
        .and_then(|l| l.as_str())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    let thumbnail_url = format!("https://i.ytimg.com/vi/{video_id}/hqdefault.jpg");

    let metadata = MediaMetadata {
        id: video_id.to_string(),
        url: canonical_url.to_string(),
        title,
        duration: duration_sec,
        thumbnail_url: Some(thumbnail_url),
        uploader: author,
        service_name: "youtube".to_string(),
        description: video_details
            .get("shortDescription")
            .and_then(|d| d.as_str())
            .map(|s| s.to_string()),
        upload_date: None,
    };

    let streaming_data =
        json_val
            .get("streamingData")
            .ok_or_else(|| FreeFetchError::Extractor {
                service: "youtube".to_string(),
                message: "No streamingData in Innertube response".to_string(),
            })?;

    let mut formats = Vec::new();
    let empty_vec = Vec::new();
    let muxed_raw = streaming_data
        .get("formats")
        .and_then(|f| f.as_array())
        .unwrap_or(&empty_vec);
    let adaptive_raw = streaming_data
        .get("adaptiveFormats")
        .and_then(|a| a.as_array())
        .unwrap_or(&empty_vec);

    // Audio streams for merging based on container format
    let best_mp4_audio_url = adaptive_raw
        .iter()
        .filter(|f| {
            let mime = f.get("mimeType").and_then(|m| m.as_str()).unwrap_or("");
            let has_url = f.get("url").is_some();
            has_url && mime.contains("audio/mp4")
        })
        .max_by_key(|f| f.get("bitrate").and_then(|b| b.as_u64()).unwrap_or(0))
        .and_then(|f| f.get("url").and_then(|u| u.as_str()).map(|s| s.to_string()));

    let best_webm_audio_url = adaptive_raw
        .iter()
        .filter(|f| {
            let mime = f.get("mimeType").and_then(|m| m.as_str()).unwrap_or("");
            let has_url = f.get("url").is_some();
            has_url && mime.contains("audio/webm")
        })
        .max_by_key(|f| f.get("bitrate").and_then(|b| b.as_u64()).unwrap_or(0))
        .and_then(|f| f.get("url").and_then(|u| u.as_str()).map(|s| s.to_string()));

    let best_any_audio_url = adaptive_raw
        .iter()
        .filter(|f| {
            let mime = f.get("mimeType").and_then(|m| m.as_str()).unwrap_or("");
            let has_url = f.get("url").is_some();
            has_url && mime.starts_with("audio/")
        })
        .max_by_key(|f| f.get("bitrate").and_then(|b| b.as_u64()).unwrap_or(0))
        .and_then(|f| f.get("url").and_then(|u| u.as_str()).map(|s| s.to_string()));

    for raw in muxed_raw.iter().chain(adaptive_raw.iter()) {
        let url_str = match raw.get("url").and_then(|u| u.as_str()) {
            Some(u) if u.starts_with("http") => u.to_string(),
            _ => continue,
        };

        let itag = raw.get("itag").and_then(|i| i.as_u64()).unwrap_or(0);
        let mime = raw.get("mimeType").and_then(|m| m.as_str()).unwrap_or("");
        let is_video = mime.starts_with("video/");
        let is_audio = mime.starts_with("audio/");

        let container = if mime.contains("webm") {
            "webm".to_string()
        } else if is_audio && mime.contains("mp4") {
            "m4a".to_string()
        } else {
            "mp4".to_string()
        };

        let quality_label = if let Some(q) = raw.get("qualityLabel").and_then(|q| q.as_str()) {
            if q.starts_with("2160") {
                "2160p 4K".to_string()
            } else if q.starts_with("1440") {
                "1440p 2K".to_string()
            } else if q.starts_with("1080") {
                "1080p FHD".to_string()
            } else if q.starts_with("720") {
                "720p HD".to_string()
            } else if q.starts_with("480") {
                "480p SD".to_string()
            } else {
                q.to_string()
            }
        } else if is_audio {
            let br = raw
                .get("bitrate")
                .and_then(|b| b.as_u64())
                .unwrap_or(128000);
            format!("{} kbps Audio", br / 1000)
        } else {
            "Standard".to_string()
        };

        let width = raw.get("width").and_then(|w| w.as_u64()).map(|w| w as u32);
        let height = raw.get("height").and_then(|h| h.as_u64()).map(|h| h as u32);
        let fps = raw.get("fps").and_then(|f| f.as_u64()).map(|f| f as u32);
        let bitrate = raw.get("bitrate").and_then(|b| b.as_u64());
        let filesize = raw
            .get("contentLength")
            .and_then(|c| c.as_str())
            .and_then(|s| s.parse::<u64>().ok());

        let format_type = if is_video && is_audio {
            "muxed".to_string()
        } else if is_video {
            "video".to_string()
        } else {
            "audio".to_string()
        };

        let is_mux = format_type == "muxed";
        let is_vid = format_type == "video";
        let requires_merge = is_vid;
        let is_webm = container == "webm";
        let matching_audio_url = if requires_merge {
            if is_webm {
                best_webm_audio_url
                    .clone()
                    .or_else(|| best_any_audio_url.clone())
            } else {
                best_mp4_audio_url
                    .clone()
                    .or_else(|| best_any_audio_url.clone())
            }
        } else {
            None
        };
        let paired_acodec = if requires_merge {
            if is_webm {
                Some("opus".to_string())
            } else {
                Some("aac".to_string())
            }
        } else {
            None
        };

        formats.push(MediaFormat {
            format_id: format!("yt-{itag}"),
            format_type,
            quality_label,
            resolution: height.map(|h| format!("{h}p")),
            width,
            height,
            fps,
            video_codec: if is_video {
                if is_webm {
                    Some("vp9".to_string())
                } else {
                    Some("h264".to_string())
                }
            } else {
                None
            },
            audio_codec: if is_audio || is_mux {
                if is_webm {
                    Some("opus".to_string())
                } else {
                    Some("aac".to_string())
                }
            } else {
                paired_acodec
            },
            mime_type: Some(mime.to_string()),
            bitrate,
            container,
            filesize,
            download_url: url_str,
            requires_merge,
            matching_audio_url,
        });
    }

    formats.sort_by(|a, b| {
        let h_a = a.height.unwrap_or(0);
        let h_b = b.height.unwrap_or(0);
        if h_a != h_b {
            return h_b.cmp(&h_a);
        }
        b.bitrate.unwrap_or(0).cmp(&a.bitrate.unwrap_or(0))
    });

    // Extract captions from Innertube response
    let mut normalized_subtitles = Vec::new();
    if let Some(caption_tracks) = json_val
        .get("captions")
        .and_then(|c| c.get("playerCaptionsTracklistRenderer"))
        .and_then(|r| r.get("captionTracks"))
        .and_then(|t| t.as_array())
    {
        for ct in caption_tracks {
            let mut base_url = match ct.get("baseUrl").and_then(|u| u.as_str()) {
                Some(u) => u.to_string(),
                None => continue,
            };
            if !base_url.contains("fmt=") {
                base_url.push_str("&fmt=vtt");
            }
            if is_translated_caption_url(&base_url) {
                continue;
            }
            let lang_code = ct
                .get("languageCode")
                .and_then(|l| l.as_str())
                .unwrap_or("und")
                .to_string();
            let vss_id = ct.get("vssId").and_then(|v| v.as_str()).unwrap_or("");
            let kind = ct.get("kind").and_then(|k| k.as_str()).unwrap_or("");
            let is_auto = kind == "asr" || vss_id.starts_with("a.");

            let name = ct
                .get("name")
                .and_then(|n| {
                    n.get("runs")
                        .and_then(|r| r.get(0))
                        .and_then(|r0| r0.get("text"))
                        .and_then(|t| t.as_str())
                        .or_else(|| n.get("simpleText").and_then(|t| t.as_str()))
                })
                .map(|s| s.to_string())
                .unwrap_or_else(|| language_code_to_name(&lang_code));

            let display_name = if is_auto {
                let mut d = name;
                if !d.contains("(Original)") {
                    d = format!("{d} (Original)");
                }
                if !d.contains("(Auto-generated)") {
                    d = format!("{d} (Auto-generated)");
                }
                d
            } else {
                name
            };

            let effective_lang_code = if is_auto && !lang_code.ends_with("-orig") {
                format!("{lang_code}-orig")
            } else {
                lang_code.clone()
            };

            let is_default = lang_code == "en" && !is_auto;

            normalized_subtitles.push(SubtitleTrack {
                id: format!(
                    "yt-sub-{effective_lang_code}-{}",
                    if is_auto { "auto" } else { "manual" }
                ),
                language_code: effective_lang_code,
                language_name: display_name,
                format: SubtitleFormat::Vtt,
                is_auto_generated: is_auto,
                is_default,
                download_url: Some(base_url),
            });
        }
    }

    normalized_subtitles.sort_by(|a, b| {
        if a.is_default != b.is_default {
            return b.is_default.cmp(&a.is_default);
        }
        if a.is_auto_generated != b.is_auto_generated {
            return a.is_auto_generated.cmp(&b.is_auto_generated);
        }
        a.language_name.cmp(&b.language_name)
    });

    Ok((
        metadata,
        StreamManifest {
            formats,
            subtitles: normalized_subtitles,
        },
    ))
}
