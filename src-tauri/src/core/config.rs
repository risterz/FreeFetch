use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct AppSettings {
    pub download_directory: String,
    pub max_concurrent_downloads: usize,
    pub speed_limit_bytes_per_sec: Option<u64>,
    pub ffmpeg_path: Option<String>,
    pub ytdlp_path: Option<String>,
    pub default_container: String,
    pub default_audio_format: String,
    pub default_audio_quality: String,
    pub embed_artwork: bool,
    pub default_subtitle_mode: String,
    pub theme: String,
    pub notifications_enabled: bool,
    pub auto_clear_completed: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        let download_dir = dirs_fallback();
        Self {
            download_directory: download_dir.to_string_lossy().to_string(),
            max_concurrent_downloads: 3,
            speed_limit_bytes_per_sec: None,
            ffmpeg_path: None,
            ytdlp_path: None,
            default_container: "mp4".to_string(),
            default_audio_format: "mp3".to_string(),
            default_audio_quality: "high".to_string(),
            embed_artwork: true,
            default_subtitle_mode: "none".to_string(),
            theme: "system".to_string(),
            notifications_enabled: true,
            auto_clear_completed: false,
        }
    }
}

impl AppSettings {
    pub fn sanitize(&mut self) {
        // Concurrency clamp between 1 and 5
        self.max_concurrent_downloads = self.max_concurrent_downloads.clamp(1, 5);

        // Speed limit: 0 means unlimited (None)
        if let Some(0) = self.speed_limit_bytes_per_sec {
            self.speed_limit_bytes_per_sec = None;
        }

        // Download directory: fallback if empty
        if self.download_directory.trim().is_empty() {
            self.download_directory = dirs_fallback().to_string_lossy().to_string();
        }

        // Container: ensure lowercase and valid
        self.default_container = self.default_container.trim().to_lowercase();
        if !["mp4", "mkv", "webm"].contains(&self.default_container.as_str()) {
            self.default_container = "mp4".to_string();
        }

        // Audio format: ensure lowercase and valid
        self.default_audio_format = self.default_audio_format.trim().to_lowercase();
        if !["mp3", "m4a", "flac", "opus"].contains(&self.default_audio_format.as_str()) {
            self.default_audio_format = "mp3".to_string();
        }

        // Audio quality preset: low, medium, high, best
        self.default_audio_quality = self.default_audio_quality.trim().to_lowercase();
        if !["low", "medium", "high", "best"].contains(&self.default_audio_quality.as_str()) {
            self.default_audio_quality = "high".to_string();
        }

        // Subtitle mode: none, embed, hardcode
        self.default_subtitle_mode = self.default_subtitle_mode.trim().to_lowercase();
        if !["none", "embed", "hardcode"].contains(&self.default_subtitle_mode.as_str()) {
            self.default_subtitle_mode = "none".to_string();
        }

        // Theme: system, light, dark
        self.theme = self.theme.trim().to_lowercase();
        if !["system", "light", "dark"].contains(&self.theme.as_str()) {
            self.theme = "system".to_string();
        }

        // FFmpeg path: trim or None if empty
        if let Some(ref p) = self.ffmpeg_path {
            let trimmed = p.trim().to_string();
            if trimmed.is_empty() {
                self.ffmpeg_path = None;
            } else {
                self.ffmpeg_path = Some(trimmed);
            }
        }

        // yt-dlp path: trim or None if empty
        if let Some(ref p) = self.ytdlp_path {
            let trimmed = p.trim().to_string();
            if trimmed.is_empty() {
                self.ytdlp_path = None;
            } else {
                self.ytdlp_path = Some(trimmed);
            }
        }
    }
}

fn dirs_fallback() -> PathBuf {
    if let Some(user_dirs) = std::env::var_os("USERPROFILE") {
        let path = PathBuf::from(user_dirs).join("Downloads").join("FreeFetch");
        return path;
    }
    if let Some(home) = std::env::var_os("HOME") {
        let path = PathBuf::from(home).join("Downloads").join("FreeFetch");
        return path;
    }
    PathBuf::from("./downloads")
}
