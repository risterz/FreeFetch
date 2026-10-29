use serde::{Deserialize, Serialize};

use crate::core::types::SubtitleTrack;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaMetadata {
    pub id: String,
    pub url: String,
    pub title: String,
    pub duration: u64,
    pub thumbnail_url: Option<String>,
    pub uploader: Option<String>,
    pub service_name: String,
    pub description: Option<String>,
    pub upload_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaFormat {
    pub format_id: String,
    pub format_type: String,   // "video", "audio", "muxed"
    pub quality_label: String, // "1080p", "720p", "320kbps", etc.
    pub resolution: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<u32>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub mime_type: Option<String>,
    pub bitrate: Option<u64>,
    pub container: String,
    pub filesize: Option<u64>,
    pub download_url: String,
    pub requires_merge: bool,
    pub matching_audio_url: Option<String>,
    #[serde(default)]
    pub http_headers: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StreamManifest {
    pub formats: Vec<MediaFormat>,
    #[serde(default)]
    pub subtitles: Vec<SubtitleTrack>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParseResult {
    pub metadata: MediaMetadata,
    pub formats: Vec<MediaFormat>,
    #[serde(default)]
    pub subtitles: Vec<SubtitleTrack>,
}
