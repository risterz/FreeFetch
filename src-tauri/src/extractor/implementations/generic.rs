use crate::core::error::FreeFetchError;
use crate::extractor::{
    context::ExtractionContext,
    models::{MediaFormat, MediaMetadata, StreamManifest},
    Extractor,
};
use async_trait::async_trait;
use reqwest::header::{CONTENT_LENGTH, CONTENT_TYPE};
use url::Url;

pub struct GenericExtractor;

#[async_trait]
impl Extractor for GenericExtractor {
    fn name(&self) -> &'static str {
        "generic"
    }

    fn can_handle(&self, url: &Url) -> bool {
        url.scheme() == "http" || url.scheme() == "https"
    }

    async fn extract(
        &self,
        url: &Url,
        ctx: &ExtractionContext,
    ) -> Result<(MediaMetadata, StreamManifest), FreeFetchError> {
        let url_str = url.as_str();

        // 1. Try HEAD request first to inspect Content-Type
        let head_res = ctx
            .client
            .head(url_str)
            .header("User-Agent", &ctx.user_agent)
            .send()
            .await;

        if let Ok(resp) = head_res {
            let content_type = resp
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_lowercase();

            let content_length = resp
                .headers()
                .get(CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok());

            if content_type.starts_with("video/")
                || content_type.starts_with("audio/")
                || is_media_url_path(url)
            {
                let filename = url
                    .path_segments()
                    .and_then(|mut s| s.next_back())
                    .filter(|s| !s.is_empty())
                    .unwrap_or("media");

                let (container, is_audio) =
                    parse_container_from_mime_or_ext(&content_type, filename);
                let quality_label = if is_audio {
                    "Audio Stream".to_string()
                } else {
                    "Original Quality".to_string()
                };

                let format = MediaFormat {
                    format_id: "direct-1".to_string(),
                    format_type: if is_audio {
                        "audio".to_string()
                    } else {
                        "video".to_string()
                    },
                    quality_label,
                    resolution: if is_audio {
                        None
                    } else {
                        Some("Original".to_string())
                    },
                    width: None,
                    height: None,
                    fps: None,
                    video_codec: if is_audio {
                        None
                    } else {
                        Some("auto".to_string())
                    },
                    audio_codec: Some("auto".to_string()),
                    mime_type: Some(content_type),
                    bitrate: None,
                    container,
                    filesize: content_length,
                    download_url: url_str.to_string(),
                    requires_merge: false,
                    matching_audio_url: None,
                    http_headers: None,
                };

                let meta = MediaMetadata {
                    id: sanitize_id_from_url(url),
                    url: url_str.to_string(),
                    title: filename.to_string(),
                    duration: 0,
                    thumbnail_url: None,
                    uploader: Some(url.host_str().unwrap_or("Direct").to_string()),
                    service_name: "generic".to_string(),
                    description: None,
                    upload_date: None,
                };

                return Ok((
                    meta,
                    StreamManifest {
                        formats: vec![format],
                        subtitles: Vec::new(),
                    },
                ));
            }
        }

        // 2. Fetch page HTML to check OpenGraph / HTML5 video tags
        let get_resp = ctx
            .client
            .get(url_str)
            .header("User-Agent", &ctx.user_agent)
            .send()
            .await
            .map_err(|e| FreeFetchError::Extractor {
                service: "generic".to_string(),
                message: format!("Failed to fetch URL: {e}"),
            })?;

        let html = get_resp
            .text()
            .await
            .map_err(|e| FreeFetchError::Extractor {
                service: "generic".to_string(),
                message: format!("Failed to read page content: {e}"),
            })?;

        let title = extract_meta_tag(&html, "og:title")
            .or_else(|| extract_html_tag(&html, "title"))
            .unwrap_or_else(|| url.host_str().unwrap_or("Web Media").to_string());

        let video_url = extract_meta_tag(&html, "og:video")
            .or_else(|| extract_meta_tag(&html, "og:video:url"))
            .or_else(|| extract_meta_tag(&html, "og:video:secure_url"))
            .or_else(|| extract_video_tag_src(&html));

        let thumbnail_url = extract_meta_tag(&html, "og:image");

        if let Some(v_url) = video_url {
            let full_video_url = if v_url.starts_with("http") {
                v_url
            } else if let Ok(base) = url.join(&v_url) {
                base.to_string()
            } else {
                v_url
            };

            let format = MediaFormat {
                format_id: "html-video-1".to_string(),
                format_type: "video".to_string(),
                quality_label: "Original Quality".to_string(),
                resolution: Some("Source".to_string()),
                width: None,
                height: None,
                fps: None,
                video_codec: Some("h264".to_string()),
                audio_codec: Some("aac".to_string()),
                mime_type: Some("video/mp4".to_string()),
                bitrate: None,
                container: "mp4".to_string(),
                filesize: None,
                download_url: full_video_url,
                requires_merge: false,
                matching_audio_url: None,
                http_headers: None,
            };

            let meta = MediaMetadata {
                id: sanitize_id_from_url(url),
                url: url_str.to_string(),
                title,
                duration: 0,
                thumbnail_url,
                uploader: Some(url.host_str().unwrap_or("Web").to_string()),
                service_name: "generic".to_string(),
                description: extract_meta_tag(&html, "og:description"),
                upload_date: None,
            };

            return Ok((
                meta,
                StreamManifest {
                    formats: vec![format],
                    subtitles: Vec::new(),
                },
            ));
        }

        Err(FreeFetchError::Extractor {
            service: "generic".to_string(),
            message: "No public media stream found at this URL".to_string(),
        })
    }
}

fn is_media_url_path(url: &Url) -> bool {
    let path = url.path().to_lowercase();
    path.ends_with(".mp4")
        || path.ends_with(".m4v")
        || path.ends_with(".webm")
        || path.ends_with(".mkv")
        || path.ends_with(".mp3")
        || path.ends_with(".m4a")
        || path.ends_with(".wav")
        || path.ends_with(".flac")
}

fn parse_container_from_mime_or_ext(mime: &str, filename: &str) -> (String, bool) {
    let lower_file = filename.to_lowercase();
    if mime.contains("mp3") || lower_file.ends_with(".mp3") {
        ("mp3".to_string(), true)
    } else if mime.contains("audio/mp4") || lower_file.ends_with(".m4a") {
        ("m4a".to_string(), true)
    } else if mime.contains("webm") || lower_file.ends_with(".webm") {
        ("webm".to_string(), false)
    } else if mime.contains("mkv") || lower_file.ends_with(".mkv") {
        ("mkv".to_string(), false)
    } else {
        ("mp4".to_string(), false)
    }
}

fn sanitize_id_from_url(url: &Url) -> String {
    let mut clean = format!("gen_{}_{}", url.host_str().unwrap_or("unknown"), url.path());
    clean.retain(|c| c.is_ascii_alphanumeric() || c == '_');
    if clean.len() > 32 {
        clean.truncate(32);
    }
    clean
}

fn extract_meta_tag(html: &str, property: &str) -> Option<String> {
    let pattern1 = format!("property=\"{}\" content=\"", property);
    let pattern2 = format!("property='{}' content='", property);

    if let Some(pos) = html.find(&pattern1) {
        let rest = &html[pos + pattern1.len()..];
        if let Some(end) = rest.find('\"') {
            return Some(rest[..end].trim().to_string());
        }
    }
    if let Some(pos) = html.find(&pattern2) {
        let rest = &html[pos + pattern2.len()..];
        if let Some(end) = rest.find('\'') {
            return Some(rest[..end].trim().to_string());
        }
    }
    None
}

fn extract_html_tag(html: &str, tag: &str) -> Option<String> {
    let open_tag = format!("<{}>", tag);
    let close_tag = format!("</{}>", tag);
    if let Some(start) = html.find(&open_tag) {
        let rest = &html[start + open_tag.len()..];
        if let Some(end) = rest.find(&close_tag) {
            return Some(rest[..end].trim().to_string());
        }
    }
    None
}

fn extract_video_tag_src(html: &str) -> Option<String> {
    if let Some(start) = html.find("<video") {
        let rest = &html[start..];
        if let Some(src_pos) = rest.find("src=\"") {
            let after_src = &rest[src_pos + 5..];
            if let Some(end) = after_src.find('\"') {
                return Some(after_src[..end].to_string());
            }
        }
    }
    None
}
