use async_trait::async_trait;
use reqwest::header::{CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_TYPE};
use url::Url;

use crate::core::error::FreeFetchError;
use crate::extractor::{
    context::ExtractionContext,
    implementations::google_drive::parse_content_disposition_filename,
    models::{MediaFormat, MediaMetadata, StreamManifest},
    Extractor,
};

pub struct DropboxExtractor;

#[async_trait]
impl Extractor for DropboxExtractor {
    fn name(&self) -> &'static str {
        "dropbox"
    }

    fn can_handle(&self, url: &Url) -> bool {
        let host = url.host_str().unwrap_or("").to_lowercase();
        host.contains("dropbox.com") || host.contains("dropboxusercontent.com")
    }

    async fn extract(
        &self,
        url: &Url,
        ctx: &ExtractionContext,
    ) -> Result<(MediaMetadata, StreamManifest), FreeFetchError> {
        let direct_url_str = convert_to_direct_dropbox_url(url);

        // Send HEAD request to probe metadata and verify direct stream
        let res = ctx
            .client
            .head(&direct_url_str)
            .header("User-Agent", &ctx.user_agent)
            .send()
            .await
            .map_err(|e| FreeFetchError::Extractor {
                service: "dropbox".to_string(),
                message: format!("Failed to reach Dropbox: {e}"),
            })?;

        let status = res.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(FreeFetchError::Extractor {
                service: "dropbox".to_string(),
                message: "Dropbox file not found. The link may have expired or been deleted.".to_string(),
            });
        }
        if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(FreeFetchError::Extractor {
                service: "dropbox".to_string(),
                message: "Dropbox access denied. The file is private or requires authorization.".to_string(),
            });
        }

        let headers = res.headers();
        let content_type = headers
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_lowercase();

        let content_length = headers
            .get(CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok());

        let mut filename = headers
            .get(CONTENT_DISPOSITION)
            .and_then(|v| v.to_str().ok())
            .and_then(parse_content_disposition_filename);

        if filename.is_none() {
            filename = url
                .path_segments()
                .and_then(|mut s| s.next_back())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
        }

        let resolved_filename = filename.unwrap_or_else(|| "dropbox_file".to_string());
        let container = infer_dropbox_container(&resolved_filename, &content_type);
        let is_audio = container == "mp3" || container == "m4a" || container == "flac" || container == "wav";

        let format = MediaFormat {
            format_id: "dropbox-direct".to_string(),
            format_type: if is_audio { "audio".to_string() } else { "video".to_string() },
            quality_label: "Original File".to_string(),
            resolution: Some("Source".to_string()),
            width: None,
            height: None,
            fps: None,
            video_codec: if is_audio { None } else { Some("auto".to_string()) },
            audio_codec: Some("auto".to_string()),
            mime_type: Some(content_type),
            bitrate: None,
            container,
            filesize: content_length,
            download_url: direct_url_str.clone(),
            requires_merge: false,
            matching_audio_url: None,
            http_headers: None,
        };

        let metadata = MediaMetadata {
            id: format!("db_{}", resolved_filename),
            url: url.as_str().to_string(),
            title: resolved_filename,
            duration: 0,
            thumbnail_url: None,
            uploader: Some("Dropbox".to_string()),
            service_name: "dropbox".to_string(),
            description: None,
            upload_date: None,
        };

        Ok((
            metadata,
            StreamManifest {
                formats: vec![format],
                subtitles: Vec::new(),
            },
        ))
    }
}

pub fn convert_to_direct_dropbox_url(url: &Url) -> String {
    let mut cloned = url.clone();
    let host = cloned.host_str().unwrap_or("").to_lowercase();

    if host.contains("dropbox.com") && !host.contains("dl.dropboxusercontent.com") {
        let mut pairs: Vec<(String, String)> = cloned
            .query_pairs()
            .filter(|(k, _)| k != "dl" && k != "raw")
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        pairs.push(("dl".to_string(), "1".to_string()));

        cloned.query_pairs_mut().clear().extend_pairs(pairs).finish();
        return cloned.to_string();
    }

    url.to_string()
}

fn infer_dropbox_container(filename: &str, mime: &str) -> String {
    let lower = filename.to_lowercase();
    if lower.ends_with(".mp4") || mime.contains("video/mp4") {
        "mp4".to_string()
    } else if lower.ends_with(".mkv") || mime.contains("video/x-matroska") {
        "mkv".to_string()
    } else if lower.ends_with(".webm") || mime.contains("video/webm") {
        "webm".to_string()
    } else if lower.ends_with(".mp3") || mime.contains("audio/mpeg") {
        "mp3".to_string()
    } else if lower.ends_with(".m4a") || mime.contains("audio/mp4") {
        "m4a".to_string()
    } else if lower.ends_with(".wav") || mime.contains("audio/wav") {
        "wav".to_string()
    } else if lower.ends_with(".flac") || mime.contains("audio/flac") {
        "flac".to_string()
    } else if let Some(idx) = lower.rfind('.') {
        let ext = &lower[idx + 1..];
        if ext.len() <= 5 && ext.chars().all(|c| c.is_ascii_alphanumeric()) {
            ext.to_string()
        } else {
            "mp4".to_string()
        }
    } else {
        "mp4".to_string()
    }
}
