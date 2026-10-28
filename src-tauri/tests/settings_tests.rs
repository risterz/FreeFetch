use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use freefetch_lib::core::config::AppSettings;
use freefetch_lib::core::error::FreeFetchError;
use freefetch_lib::core::types::{DownloadRecord, DownloadStatus, QueueItem, QueueStatus};
use freefetch_lib::media::ffmpeg::FfmpegController;
use freefetch_lib::storage::initialize_database;

struct TestContext {
    repo: freefetch_lib::storage::Repository,
    settings: Arc<Mutex<AppSettings>>,
    temp_dir: PathBuf,
}

fn create_test_context() -> TestContext {
    let temp_dir = std::env::temp_dir().join(format!("ff_settings_test_{}", Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let db_path = temp_dir.join("test.db");
    let repo = initialize_database(&db_path).unwrap();

    let settings = AppSettings::default();
    let settings_arc = Arc::new(Mutex::new(settings));

    TestContext {
        repo,
        settings: settings_arc,
        temp_dir,
    }
}

// Simulates update_settings logic for testing validation and state mutation
fn execute_update_settings(
    ctx: &TestContext,
    mut settings: AppSettings,
) -> Result<(), FreeFetchError> {
    settings.sanitize();

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
    ctx.repo.set_setting("app_settings", &serialized)?;
    let mut s = ctx.settings.lock().unwrap();
    *s = settings;
    Ok(())
}

#[test]
fn test_settings_defaults() {
    let s = AppSettings::default();
    assert_eq!(s.max_concurrent_downloads, 3);
    assert_eq!(s.speed_limit_bytes_per_sec, None);
    assert_eq!(s.ffmpeg_path, None);
    assert_eq!(s.default_container, "mp4");
    assert_eq!(s.default_audio_format, "mp3");
    assert_eq!(s.default_audio_quality, "high");
    assert!(s.embed_artwork);
    assert_eq!(s.default_subtitle_mode, "none");
    assert_eq!(s.theme, "system");
    assert!(s.notifications_enabled);
    assert!(!s.auto_clear_completed);
    assert!(!s.download_directory.is_empty());
}

#[test]
fn test_settings_sanitization() {
    let mut s = AppSettings {
        download_directory: "   ".to_string(),
        max_concurrent_downloads: 100,      // Excessive
        speed_limit_bytes_per_sec: Some(0), // 0 should become None
        ffmpeg_path: Some("   ".to_string()),
        default_container: "AVI".to_string(),       // Invalid
        default_audio_format: "WAV".to_string(),    // Invalid
        default_audio_quality: "ULTRA".to_string(), // Invalid
        embed_artwork: false,
        default_subtitle_mode: "BURN_IT".to_string(), // Invalid
        theme: "NEON".to_string(),                    // Invalid
        notifications_enabled: false,
        auto_clear_completed: true,
    };

    s.sanitize();

    assert_eq!(s.max_concurrent_downloads, 5, "Clamped to max 5");
    assert_eq!(
        s.speed_limit_bytes_per_sec, None,
        "0 becomes None (unlimited)"
    );
    assert_eq!(s.ffmpeg_path, None, "Whitespace path becomes None");
    assert_eq!(s.default_container, "mp4", "Fallback to mp4");
    assert_eq!(s.default_audio_format, "mp3", "Fallback to mp3");
    assert_eq!(s.default_audio_quality, "high", "Fallback to high");
    assert_eq!(s.default_subtitle_mode, "none", "Fallback to none");
    assert_eq!(s.theme, "system", "Fallback to system");
    assert!(
        !s.download_directory.trim().is_empty(),
        "Fallback to dirs_fallback"
    );
}

#[test]
fn test_settings_serialization_backwards_compatibility() {
    // Legacy JSON containing only the original 5 fields
    let legacy_json = r#"{
        "download_directory": "C:\\Downloads\\Old",
        "max_concurrent_downloads": 4,
        "speed_limit_bytes_per_sec": 1048576,
        "ffmpeg_path": null,
        "default_container": "mkv"
    }"#;

    let parsed: AppSettings = serde_json::from_str(legacy_json).expect("Must deserialize cleanly");
    assert_eq!(parsed.download_directory, "C:\\Downloads\\Old");
    assert_eq!(parsed.max_concurrent_downloads, 4);
    assert_eq!(parsed.speed_limit_bytes_per_sec, Some(1048576));
    assert_eq!(parsed.default_container, "mkv");
    // New fields must automatically have default values
    assert_eq!(parsed.default_audio_format, "mp3");
    assert_eq!(parsed.default_audio_quality, "high");
    assert!(parsed.embed_artwork);
    assert_eq!(parsed.default_subtitle_mode, "none");
    assert_eq!(parsed.theme, "system");
    assert!(parsed.notifications_enabled);
    assert!(!parsed.auto_clear_completed);
}

#[test]
fn test_settings_sqlite_persistence_roundtrip() {
    let ctx = create_test_context();

    let mut custom = AppSettings {
        download_directory: ctx.temp_dir.join("custom_dl").to_string_lossy().to_string(),
        max_concurrent_downloads: 4,
        speed_limit_bytes_per_sec: Some(5_000_000),
        ffmpeg_path: None,
        default_container: "mkv".to_string(),
        default_audio_format: "flac".to_string(),
        default_audio_quality: "best".to_string(),
        embed_artwork: true,
        default_subtitle_mode: "hardcode".to_string(),
        theme: "dark".to_string(),
        notifications_enabled: true,
        auto_clear_completed: true,
    };
    custom.sanitize();

    let res = execute_update_settings(&ctx, custom.clone());
    assert!(res.is_ok());

    let loaded_json = ctx.repo.get_setting("app_settings").unwrap().unwrap();
    let loaded: AppSettings = serde_json::from_str(&loaded_json).unwrap();

    assert_eq!(loaded, custom);

    let in_memory = ctx.settings.lock().unwrap().clone();
    assert_eq!(in_memory, custom);

    let _ = std::fs::remove_dir_all(&ctx.temp_dir);
}

#[test]
fn test_settings_validation_invalid_ffmpeg_path() {
    let ctx = create_test_context();

    let invalid_settings = AppSettings {
        download_directory: ctx.temp_dir.to_string_lossy().to_string(),
        max_concurrent_downloads: 3,
        speed_limit_bytes_per_sec: None,
        ffmpeg_path: Some("C:\\NonExistentPath\\definitely_not_ffmpeg.exe".to_string()),
        ..AppSettings::default()
    };

    let result = execute_update_settings(&ctx, invalid_settings);

    assert!(result.is_err());
    if let Err(FreeFetchError::Generic(msg)) = result {
        assert!(msg.contains("does not exist"));
    } else {
        panic!("Expected FreeFetchError::Generic with missing file message");
    }

    let _ = std::fs::remove_dir_all(&ctx.temp_dir);
}

#[test]
fn test_reset_to_defaults_preserves_history_and_queue() {
    let ctx = create_test_context();

    // Insert a download record into history
    let record = DownloadRecord {
        id: "hist-1".to_string(),
        source_url: "https://example.com/video".to_string(),
        service_name: "generic".to_string(),
        title: "Test Video".to_string(),
        duration: 120,
        thumbnail_url: None,
        destination_path: ctx.temp_dir.join("test.mp4").to_string_lossy().to_string(),
        container: "mp4".to_string(),
        quality_label: "720p".to_string(),
        total_bytes: 1000,
        downloaded_bytes: 1000,
        status: DownloadStatus::Completed,
        error_message: None,
        created_at: 1000,
        completed_at: Some(1050),
    };
    ctx.repo.insert_download(&record).unwrap();

    // Insert a queue item
    let q_item = QueueItem {
        id: "q-1".to_string(),
        url: "https://example.com/queue".to_string(),
        canonical_url: "https://example.com/queue".to_string(),
        title: "Queue Item".to_string(),
        thumbnail: None,
        status: QueueStatus::Queued,
        selected_format: "1080p".to_string(),
        format_id: "fmt-1".to_string(),
        output_path: ctx.temp_dir.join("q.mp4").to_string_lossy().to_string(),
        file_size: 2000,
        downloaded_bytes: 0,
        progress: 0.0,
        speed: 0,
        eta: 0,
        error_code: None,
        error_message: None,
        retry_count: 0,
        created_at: 2000,
        started_at: None,
        completed_at: None,
        updated_at: 2000,
        position: 0,
        params_json: "{}".to_string(),
    };
    ctx.repo.insert_queue_item(&q_item).unwrap();

    // Reset settings to default
    let default_settings = AppSettings::default();
    let update_res = execute_update_settings(&ctx, default_settings.clone());
    assert!(update_res.is_ok());

    // Verify settings updated in memory
    let current = ctx.settings.lock().unwrap().clone();
    assert_eq!(current, default_settings);

    // Verify history record is still intact
    let hist_after = ctx.repo.get_download("hist-1").unwrap().unwrap();
    assert_eq!(hist_after.status, DownloadStatus::Completed);
    assert_eq!(hist_after.title, "Test Video");

    // Verify queue item is still intact
    let q_after = ctx.repo.get_queue_item("q-1").unwrap().unwrap();
    assert_eq!(q_after.status, QueueStatus::Queued);

    let _ = std::fs::remove_dir_all(&ctx.temp_dir);
}
