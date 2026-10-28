use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DownloadStatus {
    Queued,
    Analyzing,
    Downloading,
    Retrying,
    Processing,
    Completed,
    Failed,
    Paused,
    Cancelled,
}

impl DownloadStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            DownloadStatus::Queued => "queued",
            DownloadStatus::Analyzing => "analyzing",
            DownloadStatus::Downloading => "downloading",
            DownloadStatus::Retrying => "retrying",
            DownloadStatus::Processing => "processing",
            DownloadStatus::Completed => "completed",
            DownloadStatus::Failed => "failed",
            DownloadStatus::Paused => "paused",
            DownloadStatus::Cancelled => "cancelled",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "analyzing" => DownloadStatus::Analyzing,
            "downloading" => DownloadStatus::Downloading,
            "retrying" => DownloadStatus::Retrying,
            "processing" | "remuxing" => DownloadStatus::Processing,
            "completed" => DownloadStatus::Completed,
            "failed" => DownloadStatus::Failed,
            "paused" => DownloadStatus::Paused,
            "cancelled" => DownloadStatus::Cancelled,
            _ => DownloadStatus::Queued,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadRecord {
    pub id: String,
    pub source_url: String,
    pub service_name: String,
    pub title: String,
    pub duration: u64,
    pub thumbnail_url: Option<String>,
    pub destination_path: String,
    pub container: String,
    pub quality_label: String,
    pub total_bytes: u64,
    pub downloaded_bytes: u64,
    pub status: DownloadStatus,
    pub error_message: Option<String>,
    pub created_at: i64,
    pub completed_at: Option<i64>,
}

impl DownloadRecord {
    pub fn media_type(&self) -> &'static str {
        match self.container.to_ascii_lowercase().as_str() {
            "mp3" | "m4a" | "flac" | "opus" | "aac" | "ogg" | "wav" => "audio",
            _ => "video",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HistoryFilter {
    pub search: Option<String>,
    pub status: Option<String>,
    pub media_type: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryPage {
    pub items: Vec<DownloadRecord>,
    pub total_count: usize,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DownloadStatistics {
    pub total_downloads: u64,
    pub completed_downloads: u64,
    pub failed_downloads: u64,
    pub cancelled_downloads: u64,
    pub total_downloaded_bytes: u64,
    pub audio_downloads: u64,
    pub video_downloads: u64,
    pub total_duration_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamRecord {
    pub id: String,
    pub download_id: String,
    pub stream_type: String, // 'video', 'audio', 'subtitle'
    pub format_id: String,
    pub codec: Option<String>,
    pub resolution: Option<String>,
    pub bitrate: Option<u64>,
    pub temp_file_path: String,
    pub total_bytes: u64,
    pub downloaded_bytes: u64,
    pub is_finished: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QueueStatus {
    Queued,
    Preparing,
    Downloading,
    Paused,
    Processing,
    Completed,
    Failed,
    Cancelled,
    Retrying,
}

impl QueueStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            QueueStatus::Queued => "queued",
            QueueStatus::Preparing => "preparing",
            QueueStatus::Downloading => "downloading",
            QueueStatus::Paused => "paused",
            QueueStatus::Processing => "processing",
            QueueStatus::Completed => "completed",
            QueueStatus::Failed => "failed",
            QueueStatus::Cancelled => "cancelled",
            QueueStatus::Retrying => "retrying",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "queued" => QueueStatus::Queued,
            "preparing" => QueueStatus::Preparing,
            "downloading" => QueueStatus::Downloading,
            "paused" => QueueStatus::Paused,
            "processing" | "remuxing" => QueueStatus::Processing,
            "completed" => QueueStatus::Completed,
            "failed" => QueueStatus::Failed,
            "cancelled" => QueueStatus::Cancelled,
            "retrying" => QueueStatus::Retrying,
            _ => QueueStatus::Queued,
        }
    }

    pub fn can_transition_to(&self, next: &QueueStatus) -> bool {
        match (self, next) {
            (a, b) if a == b => true,
            (QueueStatus::Queued, QueueStatus::Preparing) => true,
            (QueueStatus::Queued, QueueStatus::Downloading) => true,
            (QueueStatus::Queued, QueueStatus::Paused) => true,
            (QueueStatus::Queued, QueueStatus::Cancelled) => true,
            (QueueStatus::Preparing, QueueStatus::Downloading) => true,
            (QueueStatus::Preparing, QueueStatus::Processing) => true,
            (QueueStatus::Preparing, QueueStatus::Completed) => true,
            (QueueStatus::Preparing, QueueStatus::Paused) => true,
            (QueueStatus::Preparing, QueueStatus::Cancelled) => true,
            (QueueStatus::Preparing, QueueStatus::Failed) => true,
            (QueueStatus::Downloading, QueueStatus::Paused) => true,
            (QueueStatus::Downloading, QueueStatus::Retrying) => true,
            (QueueStatus::Downloading, QueueStatus::Processing) => true,
            (QueueStatus::Downloading, QueueStatus::Completed) => true,
            (QueueStatus::Downloading, QueueStatus::Failed) => true,
            (QueueStatus::Downloading, QueueStatus::Cancelled) => true,
            (QueueStatus::Retrying, QueueStatus::Downloading) => true,
            (QueueStatus::Retrying, QueueStatus::Paused) => true,
            (QueueStatus::Retrying, QueueStatus::Cancelled) => true,
            (QueueStatus::Retrying, QueueStatus::Failed) => true,
            (QueueStatus::Processing, QueueStatus::Completed) => true,
            (QueueStatus::Processing, QueueStatus::Failed) => true,
            (QueueStatus::Processing, QueueStatus::Cancelled) => true,
            (QueueStatus::Paused, QueueStatus::Queued) => true,
            (QueueStatus::Paused, QueueStatus::Preparing) => true,
            (QueueStatus::Paused, QueueStatus::Downloading) => true,
            (QueueStatus::Paused, QueueStatus::Cancelled) => true,
            (QueueStatus::Failed, QueueStatus::Queued) => true,
            (QueueStatus::Cancelled, QueueStatus::Queued) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueItem {
    pub id: String,
    pub url: String,
    pub canonical_url: String,
    pub title: String,
    pub thumbnail: Option<String>,
    pub status: QueueStatus,
    pub selected_format: String,
    pub format_id: String,
    pub output_path: String,
    pub file_size: u64,
    pub downloaded_bytes: u64,
    pub progress: f64,
    pub speed: u64,
    pub eta: u64,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub retry_count: u32,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub updated_at: i64,
    pub position: i32,
    pub params_json: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QueueSummary {
    pub total_items: usize,
    pub queued_count: usize,
    pub downloading_count: usize,
    pub paused_count: usize,
    pub completed_count: usize,
    pub failed_count: usize,
    pub total_bytes: u64,
    pub downloaded_bytes: u64,
    pub overall_progress_percent: f64,
    pub total_speed_bytes_per_sec: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ItemAvailability {
    #[default]
    Available,
    Unavailable,
    Private,
    Deleted,
}

impl ItemAvailability {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistItem {
    pub id: String,
    pub index: u32,
    pub url: String,
    pub canonical_url: String,
    pub title: String,
    pub thumbnail: Option<String>,
    pub duration: u64,
    pub availability: ItemAvailability,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistInfo {
    pub id: String,
    pub canonical_url: String,
    pub title: String,
    pub thumbnail: Option<String>,
    pub uploader: Option<String>,
    pub item_count: usize,
    pub items: Vec<PlaylistItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlaylistDetection {
    pub playlist_id: String,
    pub is_pure_playlist: bool,
    pub video_id: Option<String>,
    pub canonical_playlist_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BatchItemStatus {
    Pending,
    Analyzing,
    Ready,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchAnalysisItem {
    pub original_url: String,
    pub canonical_url: String,
    pub status: BatchItemStatus,
    pub metadata: Option<crate::extractor::MediaMetadata>,
    pub formats: Vec<crate::extractor::MediaFormat>,
    #[serde(default)]
    pub subtitles: Vec<SubtitleTrack>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchAnalysisResult {
    pub total: usize,
    pub ready_count: usize,
    pub failed_count: usize,
    pub items: Vec<BatchAnalysisItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchQueueResult {
    pub added_count: usize,
    pub duplicate_count: usize,
    pub failed_count: usize,
    pub unavailable_count: usize,
    pub added_ids: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistItemSelection {
    pub id: String,
    pub url: String,
    pub title: String,
    pub duration: u64,
    pub thumbnail: Option<String>,
    pub format_id: Option<String>,
    #[serde(default)]
    pub selected_subtitles: Vec<SubtitleSelection>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SubtitleMode {
    #[default]
    None,
    Embed,
    Hardcode,
}

impl SubtitleMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            SubtitleMode::None => "none",
            SubtitleMode::Embed => "embed",
            SubtitleMode::Hardcode => "hardcode",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SubtitleFormat {
    #[default]
    Srt,
    Vtt,
}

impl SubtitleFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            SubtitleFormat::Srt => "srt",
            SubtitleFormat::Vtt => "vtt",
        }
    }

    pub fn from_ext(ext: &str) -> Self {
        if ext.eq_ignore_ascii_case("vtt") {
            SubtitleFormat::Vtt
        } else {
            SubtitleFormat::Srt
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubtitleTrack {
    pub id: String,
    pub language_code: String,
    pub language_name: String,
    pub format: SubtitleFormat,
    pub is_auto_generated: bool,
    pub is_default: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubtitleSelection {
    pub language_code: String,
    pub language_name: String,
    pub target_format: SubtitleFormat,
    pub is_auto_generated: bool,
    #[serde(default)]
    pub download_url: Option<String>,
}

pub use crate::media::audio::{
    AudioConversionOptions, AudioQualityPreset, AudioTargetFormat, BitrateMode,
};
