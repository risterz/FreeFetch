use async_trait::async_trait;
use reqwest::header::{CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_TYPE, SET_COOKIE};
use url::Url;

use crate::core::error::FreeFetchError;
use crate::extractor::{
    context::ExtractionContext,
    models::{MediaFormat, MediaMetadata, StreamManifest},
    Extractor,
};

pub struct GoogleDriveExtractor;

#[async_trait]
impl Extractor for GoogleDriveExtractor {
    fn name(&self) -> &'static str {
        "google_drive"
    }

    fn can_handle(&self, url: &Url) -> bool {
        let host = url.host_str().unwrap_or("").to_lowercase();
        if host != "drive.google.com" && host != "docs.google.com" {
            return false;
        }
        extract_gdrive_file_id(url).is_some()
    }

    async fn extract(
        &self,
        url: &Url,
        ctx: &ExtractionContext,
    ) -> Result<(MediaMetadata, StreamManifest), FreeFetchError> {
        let file_id = extract_gdrive_file_id(url).ok_or_else(|| {
            FreeFetchError::InvalidUrl(
                "Could not extract a valid Google Drive file ID from URL".to_string(),
            )
        })?;

        // 1. Initial request to Google Drive export download endpoint
        let initial_url = format!("https://drive.google.com/uc?id={file_id}&export=download");

        let res = ctx
            .client
            .get(&initial_url)
            .header("User-Agent", &ctx.user_agent)
            .send()
            .await
            .map_err(|e| FreeFetchError::Extractor {
                service: "google_drive".to_string(),
                message: format!("Failed to connect to Google Drive: {e}"),
            })?;

        let status = res.status();
        if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(FreeFetchError::Extractor {
                service: "google_drive".to_string(),
                message: "Google Drive access denied. The file is private or requires authorized account access. Ensure sharing is set to 'Anyone with the link'.".to_string(),
            });
        }
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(FreeFetchError::Extractor {
                service: "google_drive".to_string(),
                message: "Google Drive file not found. The link may be invalid or the file has been deleted.".to_string(),
            });
        }

        let headers = res.headers().clone();
        let content_type = headers
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_lowercase();

        let final_download_url: String;
        let mut resolved_filename: Option<String> = None;
        let resolved_size: Option<u64>;

        // Check if direct binary response (small file)
        if !content_type.contains("text/html") {
            final_download_url = res.url().to_string();
            resolved_size = headers
                .get(CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok());

            if let Some(cd) = headers.get(CONTENT_DISPOSITION).and_then(|v| v.to_str().ok()) {
                resolved_filename = parse_content_disposition_filename(cd);
            }
        } else {
            // HTML response: either a large file virus-scan warning or access denied page
            let cookies: Vec<String> = headers
                .get_all(SET_COOKIE)
                .iter()
                .filter_map(|v| v.to_str().ok().map(|s| s.to_string()))
                .collect();

            let cookie_header = cookies
                .iter()
                .filter_map(|c| c.split(';').next())
                .collect::<Vec<_>>()
                .join("; ");

            let html = res.text().await.map_err(|e| FreeFetchError::Extractor {
                service: "google_drive".to_string(),
                message: format!("Failed to read Google Drive page: {e}"),
            })?;

            if html.contains("You need access") || html.contains("Request access") || html.contains("Sign in") {
                return Err(FreeFetchError::Extractor {
                    service: "google_drive".to_string(),
                    message: "Google Drive file is private. Please ensure 'Anyone with the link' can view this file.".to_string(),
                });
            }

            if html.contains("File not found") || html.contains("file does not exist") {
                return Err(FreeFetchError::Extractor {
                    service: "google_drive".to_string(),
                    message: "Google Drive file does not exist or has been removed.".to_string(),
                });
            }

            // Extract filename from HTML title if available: "<title>filename.mp4 - Google Drive</title>"
            resolved_filename = extract_title_filename(&html);

            // Look for large-file confirmation tokens
            let (confirm_token, uuid_token) = extract_confirmation_tokens(&html);

            let mut confirm_url = format!(
                "https://drive.google.com/uc?export=download&id={file_id}&confirm={confirm_token}"
            );
            if let Some(uuid) = uuid_token {
                confirm_url.push_str(&format!("&uuid={uuid}"));
            }

            // Send confirmation request to get final redirect URL and headers
            let mut req = ctx.client.get(&confirm_url).header("User-Agent", &ctx.user_agent);
            if !cookie_header.is_empty() {
                req = req.header("Cookie", cookie_header);
            }

            let confirm_res = req.send().await.map_err(|e| FreeFetchError::Extractor {
                service: "google_drive".to_string(),
                message: format!("Failed to follow Google Drive confirmation: {e}"),
            })?;

            if confirm_res.status() == reqwest::StatusCode::FORBIDDEN {
                return Err(FreeFetchError::Extractor {
                    service: "google_drive".to_string(),
                    message: "Google Drive download limit reached or access forbidden for this file.".to_string(),
                });
            }

            let c_headers = confirm_res.headers();
            if let Some(cd) = c_headers.get(CONTENT_DISPOSITION).and_then(|v| v.to_str().ok()) {
                if let Some(fn_extracted) = parse_content_disposition_filename(cd) {
                    resolved_filename = Some(fn_extracted);
                }
            }

            resolved_size = c_headers
                .get(CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok());

            final_download_url = confirm_res.url().to_string();
        }

        let fallback_title = format!("Google_Drive_File_{file_id}");
        let filename = resolved_filename.unwrap_or(fallback_title);
        let container = infer_container(&filename, &content_type);
        let is_audio = container == "mp3" || container == "m4a" || container == "flac" || container == "wav";

        let format = MediaFormat {
            format_id: format!("gdrive-{file_id}"),
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
            filesize: resolved_size,
            download_url: final_download_url,
            requires_merge: false,
            matching_audio_url: None,
        };

        let metadata = MediaMetadata {
            id: file_id.clone(),
            url: url.as_str().to_string(),
            title: filename,
            duration: 0,
            thumbnail_url: None,
            uploader: Some("Google Drive".to_string()),
            service_name: "google_drive".to_string(),
            description: Some(format!("Google Drive file ID: {file_id}")),
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

pub fn extract_gdrive_file_id(url: &Url) -> Option<String> {
    let path = url.path();

    // Check path: /file/d/{id}/...
    if let Some(idx) = path.find("/file/d/") {
        let remainder = &path[idx + 8..];
        let file_id: String = remainder.chars().take_while(|c| *c != '/' && *c != '?').collect();
        if !file_id.is_empty() {
            return Some(file_id);
        }
    }

    // Check path: /d/{id}/...
    if let Some(idx) = path.find("/d/") {
        let remainder = &path[idx + 3..];
        let file_id: String = remainder.chars().take_while(|c| *c != '/' && *c != '?').collect();
        if !file_id.is_empty() {
            return Some(file_id);
        }
    }

    // Check query param: ?id={id}
    for (k, v) in url.query_pairs() {
        if k == "id" && !v.is_empty() {
            return Some(v.to_string());
        }
    }

    None
}

pub fn parse_content_disposition_filename(cd: &str) -> Option<String> {
    // 1. Check RFC 5987 / RFC 6266 filename*=UTF-8''filename.ext
    if let Some(idx) = cd.to_lowercase().find("filename*=") {
        let remainder = &cd[idx + 10..];
        let raw_val = remainder.split(';').next().unwrap_or("").trim();
        let stripped = raw_val.trim_matches('"').trim_matches('\'');
        if let Some(utf8_idx) = stripped.to_lowercase().find("utf-8''") {
            let encoded = &stripped[utf8_idx + 7..];
            if let Ok(decoded) = urlencoding_decode(encoded) {
                return Some(decoded);
            }
        }
    }

    // 2. Standard filename="filename.ext" or filename=filename.ext
    if let Some(idx) = cd.to_lowercase().find("filename=") {
        let remainder = &cd[idx + 9..];
        let raw_val = remainder.split(';').next().unwrap_or("").trim();
        let clean = raw_val.trim_matches('"').trim_matches('\'');
        if !clean.is_empty() {
            return Some(clean.to_string());
        }
    }

    None
}

fn urlencoding_decode(s: &str) -> Result<String, FreeFetchError> {
    let mut bytes = Vec::new();
    let mut chars = s.bytes();
    while let Some(b) = chars.next() {
        if b == b'%' {
            let h1 = chars.next().ok_or_else(|| FreeFetchError::Generic("Invalid percent encoding".to_string()))?;
            let h2 = chars.next().ok_or_else(|| FreeFetchError::Generic("Invalid percent encoding".to_string()))?;
            let hex_bytes = [h1, h2];
            let hex_str = std::str::from_utf8(&hex_bytes)
                .map_err(|_| FreeFetchError::Generic("Invalid utf8 hex in encoding".to_string()))?;
            let byte = u8::from_str_radix(hex_str, 16)
                .map_err(|_| FreeFetchError::Generic("Invalid hex digit".to_string()))?;
            bytes.push(byte);
        } else {
            bytes.push(b);
        }
    }
    String::from_utf8(bytes).map_err(|e| FreeFetchError::Generic(e.to_string()))
}

fn extract_title_filename(html: &str) -> Option<String> {
    if let Some(start) = html.find("<title>") {
        if let Some(end) = html[start + 7..].find("</title>") {
            let title = &html[start + 7..start + 7 + end];
            let clean = title.replace(" - Google Drive", "").trim().to_string();
            if !clean.is_empty() {
                return Some(clean);
            }
        }
    }
    None
}

fn extract_confirmation_tokens(html: &str) -> (String, Option<String>) {
    // 1. Check confirm=([a-zA-Z0-9_-]+)
    let mut confirm_token = "t".to_string();
    if let Some(idx) = html.find("confirm=") {
        let rem = &html[idx + 8..];
        let token: String = rem
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
            .collect();
        if !token.is_empty() {
            confirm_token = token;
        }
    } else if let Some(idx) = html.find("name=\"confirm\" value=\"") {
        let rem = &html[idx + 22..];
        let token: String = rem.chars().take_while(|c| *c != '"').collect();
        if !token.is_empty() {
            confirm_token = token;
        }
    }

    // 2. Check uuid=([a-zA-Z0-9_-]+)
    let mut uuid_token = None;
    if let Some(idx) = html.find("uuid=") {
        let rem = &html[idx + 5..];
        let token: String = rem
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
            .collect();
        if !token.is_empty() {
            uuid_token = Some(token);
        }
    } else if let Some(idx) = html.find("name=\"uuid\" value=\"") {
        let rem = &html[idx + 19..];
        let token: String = rem.chars().take_while(|c| *c != '"').collect();
        if !token.is_empty() {
            uuid_token = Some(token);
        }
    }

    (confirm_token, uuid_token)
}

fn infer_container(filename: &str, mime: &str) -> String {
    let lower_file = filename.to_lowercase();
    if lower_file.ends_with(".mp4") || mime.contains("video/mp4") {
        "mp4".to_string()
    } else if lower_file.ends_with(".mkv") || mime.contains("video/x-matroska") {
        "mkv".to_string()
    } else if lower_file.ends_with(".webm") || mime.contains("video/webm") {
        "webm".to_string()
    } else if lower_file.ends_with(".mp3") || mime.contains("audio/mpeg") {
        "mp3".to_string()
    } else if lower_file.ends_with(".m4a") || mime.contains("audio/mp4") {
        "m4a".to_string()
    } else if lower_file.ends_with(".flac") || mime.contains("audio/flac") {
        "flac".to_string()
    } else if lower_file.ends_with(".wav") || mime.contains("audio/wav") {
        "wav".to_string()
    } else if let Some(dot_idx) = lower_file.rfind('.') {
        let ext = &lower_file[dot_idx + 1..];
        if ext.len() <= 5 && ext.chars().all(|c| c.is_ascii_alphanumeric()) {
            ext.to_string()
        } else {
            "mp4".to_string()
        }
    } else {
        "mp4".to_string()
    }
}
