use async_trait::async_trait;
use url::Url;

use crate::core::error::FreeFetchError;
use crate::extractor::{
    context::ExtractionContext,
    models::{MediaFormat, MediaMetadata, StreamManifest},
    Extractor,
};
use crate::media::ytdlp::{YtDlpController, YtDlpDump};

pub struct YtDlpExtractor;

const KNOWN_PLATFORMS: &[&str] = &[
    "tiktok.com",
    "instagram.com",
    "twitter.com",
    "x.com",
    "facebook.com",
    "fb.watch",
    "vimeo.com",
    "reddit.com",
    "v.redd.it",
    "twitch.tv",
    "bilibili.com",
    "dailymotion.com",
    "soundcloud.com",
    "threads.net",
    "pinterest.com",
    "streamable.com",
    "rumble.com",
    "vk.com",
    "loom.com",
];

#[async_trait]
impl Extractor for YtDlpExtractor {
    fn name(&self) -> &'static str {
        "ytdlp"
    }

    fn can_handle(&self, url: &Url) -> bool {
        let host = url.host_str().unwrap_or("").to_lowercase();
        if host.contains("youtube.com")
            || host.contains("youtu.be")
            || host.contains("drive.google.com")
            || host.contains("docs.google.com")
            || host.contains("dropbox.com")
            || host.contains("dropboxusercontent.com")
        {
            return false;
        }

        KNOWN_PLATFORMS
            .iter()
            .any(|k| host == *k || host.ends_with(&format!(".{k}")))
    }

    async fn extract(
        &self,
        url: &Url,
        ctx: &ExtractionContext,
    ) -> Result<(MediaMetadata, StreamManifest), FreeFetchError> {
        let host = url.host_str().unwrap_or("").to_lowercase();
        let platform_label = detect_platform_display_name(&host);

        // Discover yt-dlp executable
        let controller = match YtDlpController::discover(ctx.ytdlp_path.as_deref()) {
            Ok(c) => c,
            Err(_) => {
                return Err(FreeFetchError::Extractor {
                    service: platform_label.to_lowercase(),
                    message: format!(
                        "yt-dlp is required to extract and download media from {platform_label}. Please install yt-dlp or configure its location in Settings."
                    ),
                });
            }
        };

        let canonical_url = url.as_str();
        let dump = controller
            .extract_json(canonical_url, ctx.proxy.as_deref())
            .await?;

        normalize_universal_ytdlp_dump(dump, canonical_url, &platform_label)
    }
}

pub fn detect_platform_display_name(host: &str) -> String {
    let lower = host.to_lowercase();
    if lower.contains("tiktok.com") {
        "TikTok".to_string()
    } else if lower.contains("instagram.com") {
        "Instagram".to_string()
    } else if lower.contains("twitter.com") || lower.contains("x.com") {
        "X (Twitter)".to_string()
    } else if lower.contains("facebook.com") || lower.contains("fb.watch") {
        "Facebook".to_string()
    } else if lower.contains("vimeo.com") {
        "Vimeo".to_string()
    } else if lower.contains("reddit.com") || lower.contains("v.redd.it") {
        "Reddit".to_string()
    } else if lower.contains("twitch.tv") {
        "Twitch".to_string()
    } else if lower.contains("bilibili.com") {
        "Bilibili".to_string()
    } else if lower.contains("dailymotion.com") {
        "Dailymotion".to_string()
    } else if lower.contains("soundcloud.com") {
        "SoundCloud".to_string()
    } else {
        "Web Media".to_string()
    }
}

fn normalize_universal_ytdlp_dump(
    dump: YtDlpDump,
    canonical_url: &str,
    platform_name: &str,
) -> Result<(MediaMetadata, StreamManifest), FreeFetchError> {
    let metadata = MediaMetadata {
        id: dump.id.clone(),
        url: canonical_url.to_string(),
        title: dump.title.clone(),
        duration: dump.duration.unwrap_or(0.0) as u64,
        thumbnail_url: dump.thumbnail,
        uploader: dump.artist.or(dump.uploader).or(dump.channel),
        service_name: platform_name.to_lowercase().replace(' ', "_"),
        description: dump.description,
        upload_date: dump.release_date.or(dump.upload_date),
    };

    let raw_formats = dump.formats.unwrap_or_default();
    let mut parsed_formats: Vec<MediaFormat> = Vec::new();

    // Check best audio stream for possible multiplexing if needed
    let best_audio = raw_formats
        .iter()
        .filter(|f| {
            let vcodec = f.vcodec.as_deref().unwrap_or("none");
            let acodec = f.acodec.as_deref().unwrap_or("none");
            (vcodec == "none" || vcodec.is_empty()) && acodec != "none" && !acodec.is_empty()
        })
        .max_by_key(|f| f.abr.unwrap_or(0.0) as u64);

    for f in &raw_formats {
        let vcodec = f.vcodec.as_deref().unwrap_or("none");
        let acodec = f.acodec.as_deref().unwrap_or("none");
        let has_video = vcodec != "none" && !vcodec.is_empty();
        let has_audio = acodec != "none" && !acodec.is_empty();

        let ext = f.ext.clone().unwrap_or_else(|| "mp4".to_string());
        if ext == "mhtml" {
            continue;
        }

        // Platforms extracted through yt-dlp require extractor-managed downloading
        // to maintain anti-bot challenge cookies, TLS impersonation, and proper remuxing.
        let download_url = format!("ytdlp://{}#{}", canonical_url, f.format_id);
        let format_headers = f.http_headers.clone().or_else(|| dump.http_headers.clone());

        if has_video && has_audio {
            // Progressive muxed stream
            let height_label = f.height.map(|h| format!("{h}p")).unwrap_or_else(|| "Source".to_string());
            parsed_formats.push(MediaFormat {
                format_id: f.format_id.clone(),
                format_type: "video".to_string(),
                quality_label: height_label.clone(),
                resolution: Some(height_label),
                width: f.width,
                height: f.height,
                fps: f.fps.map(|fps| fps as u32),
                video_codec: Some(vcodec.to_string()),
                audio_codec: Some(acodec.to_string()),
                mime_type: None,
                bitrate: f.tbr.map(|t| (t * 1000.0) as u64),
                container: ext,
                filesize: f.filesize.or(f.filesize_approx),
                download_url,
                requires_merge: false,
                matching_audio_url: None,
                http_headers: format_headers,
            });
        } else if has_video && !has_audio {
            // Video-only adaptive stream: check if best audio can be paired
            let height_label = f.height.map(|h| format!("{h}p")).unwrap_or_else(|| "Source".to_string());
            let (requires_merge, matching_audio_url) = if let Some(a) = best_audio {
                if let Some(ref a_url) = a.url {
                    (true, Some(a_url.clone()))
                } else {
                    (false, None)
                }
            } else {
                (false, None)
            };

            parsed_formats.push(MediaFormat {
                format_id: f.format_id.clone(),
                format_type: "video".to_string(),
                quality_label: height_label.clone(),
                resolution: Some(height_label),
                width: f.width,
                height: f.height,
                fps: f.fps.map(|fps| fps as u32),
                video_codec: Some(vcodec.to_string()),
                audio_codec: best_audio.and_then(|a| a.acodec.clone()),
                mime_type: None,
                bitrate: f.tbr.map(|t| (t * 1000.0) as u64),
                container: ext,
                filesize: f.filesize.or(f.filesize_approx),
                download_url,
                requires_merge,
                matching_audio_url,
                http_headers: format_headers,
            });
        } else if has_audio && !has_video {
            // Pure audio stream
            let abr_label = f.abr.map(|a| format!("{} kbps", a as u32)).unwrap_or_else(|| "Audio Stream".to_string());
            parsed_formats.push(MediaFormat {
                format_id: f.format_id.clone(),
                format_type: "audio".to_string(),
                quality_label: abr_label,
                resolution: None,
                width: None,
                height: None,
                fps: None,
                video_codec: None,
                audio_codec: Some(acodec.to_string()),
                mime_type: None,
                bitrate: f.abr.map(|a| (a * 1000.0) as u64),
                container: ext,
                filesize: f.filesize.or(f.filesize_approx),
                download_url,
                requires_merge: false,
                matching_audio_url: None,
                http_headers: format_headers,
            });
        }
    }

    // Deduplicate formats by resolution / container
    parsed_formats.sort_by(|a, b| {
        b.height.unwrap_or(0).cmp(&a.height.unwrap_or(0))
            .then_with(|| b.bitrate.unwrap_or(0).cmp(&a.bitrate.unwrap_or(0)))
    });

    if parsed_formats.is_empty() {
        // Fallback: create single original quality format
        parsed_formats.push(MediaFormat {
            format_id: "best".to_string(),
            format_type: "video".to_string(),
            quality_label: "Best Quality".to_string(),
            resolution: Some("Source".to_string()),
            width: None,
            height: None,
            fps: None,
            video_codec: Some("auto".to_string()),
            audio_codec: Some("auto".to_string()),
            mime_type: None,
            bitrate: None,
            container: "mp4".to_string(),
            filesize: None,
            download_url: format!("ytdlp://{canonical_url}#best"),
            requires_merge: false,
            matching_audio_url: None,
            http_headers: dump.http_headers,
        });
    }

    Ok((
        metadata,
        StreamManifest {
            formats: parsed_formats,
            subtitles: Vec::new(),
        },
    ))
}
