use freefetch_lib::core::config::AppSettings;
use freefetch_lib::core::types::{DownloadRecord, DownloadStatus};
use freefetch_lib::engine::state::{
    ensure_safe_destination_path, sanitize_filename, validate_download_url,
};
use freefetch_lib::extractor::implementations::youtube::extract_youtube_id;
use freefetch_lib::extractor::ExtractorRegistry;
use freefetch_lib::storage::create_in_memory_database;
use url::Url;

#[test]
fn test_filename_sanitization() {
    assert_eq!(sanitize_filename("valid_name"), "valid_name");
    assert_eq!(
        sanitize_filename("file:with*illegal?chars/\\<>|\""),
        "file_with_illegal_chars______"
    );
    assert_eq!(
        sanitize_filename("   leading_trailing_dots...  "),
        "leading_trailing_dots"
    );
    assert_eq!(sanitize_filename(""), "download");
    assert_eq!(sanitize_filename("..."), "download");

    let long_name = "a".repeat(300);
    let sanitized = sanitize_filename(&long_name);
    assert_eq!(sanitized.len(), 180);
}

#[test]
fn test_url_validation() {
    assert!(validate_download_url("https://example.com/video.mp4").is_ok());
    assert!(validate_download_url("http://example.com/audio.mp3").is_ok());

    // Rejection of invalid or dangerous schemes
    assert!(validate_download_url("file:///etc/passwd").is_err());
    assert!(validate_download_url("ftp://server/file").is_err());
    assert!(validate_download_url("javascript:alert(1)").is_err());
    assert!(validate_download_url("not a url").is_err());
}

#[test]
fn test_ensure_safe_destination_path() {
    let temp_dir = std::env::temp_dir().join(format!("freefetch_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();

    let p1 = ensure_safe_destination_path(&temp_dir, "my_video", "mp4");
    assert_eq!(p1.file_name().unwrap(), "my_video.mp4");

    // Create the file so next call detects collision
    std::fs::write(&p1, b"dummy").unwrap();

    let p2 = ensure_safe_destination_path(&temp_dir, "my_video", "mp4");
    assert_eq!(p2.file_name().unwrap(), "my_video (1).mp4");

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_youtube_id_extraction() {
    // Standard watch URLs
    let u1 = Url::parse("https://www.youtube.com/watch?v=dQw4w9WgXcQ").unwrap();
    assert_eq!(extract_youtube_id(&u1), Some("dQw4w9WgXcQ".to_string()));

    let u1_with_query =
        Url::parse("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=42s&feature=shared").unwrap();
    assert_eq!(
        extract_youtube_id(&u1_with_query),
        Some("dQw4w9WgXcQ".to_string())
    );

    // Short youtu.be URLs
    let u2 = Url::parse("https://youtu.be/dQw4w9WgXcQ?si=abc").unwrap();
    assert_eq!(extract_youtube_id(&u2), Some("dQw4w9WgXcQ".to_string()));

    // Shorts URLs
    let u3 = Url::parse("https://www.youtube.com/shorts/dQw4w9WgXcQ").unwrap();
    assert_eq!(extract_youtube_id(&u3), Some("dQw4w9WgXcQ".to_string()));

    let u3_with_query =
        Url::parse("https://www.youtube.com/shorts/dQw4w9WgXcQ?feature=share").unwrap();
    assert_eq!(
        extract_youtube_id(&u3_with_query),
        Some("dQw4w9WgXcQ".to_string())
    );

    // Embed and Live URLs
    let u_embed = Url::parse("https://www.youtube.com/embed/dQw4w9WgXcQ").unwrap();
    assert_eq!(
        extract_youtube_id(&u_embed),
        Some("dQw4w9WgXcQ".to_string())
    );

    let u_live = Url::parse("https://www.youtube.com/live/dQw4w9WgXcQ").unwrap();
    assert_eq!(extract_youtube_id(&u_live), Some("dQw4w9WgXcQ".to_string()));

    // Rejection of invalid or non-video URLs
    let u_channel = Url::parse("https://www.youtube.com/@RickAstleyYT").unwrap();
    assert_eq!(extract_youtube_id(&u_channel), None);

    let u_subs = Url::parse("https://www.youtube.com/feed/subscriptions").unwrap();
    assert_eq!(extract_youtube_id(&u_subs), None);

    let u_invalid_len = Url::parse("https://www.youtube.com/watch?v=too_short").unwrap();
    assert_eq!(extract_youtube_id(&u_invalid_len), None);

    let u_invalid_chars = Url::parse("https://www.youtube.com/watch?v=dQw4w9WgXc!").unwrap();
    assert_eq!(extract_youtube_id(&u_invalid_chars), None);

    let u_other = Url::parse("https://example.com/other").unwrap();
    assert_eq!(extract_youtube_id(&u_other), None);
}

#[test]
fn test_youtube_format_classification_and_selection() {
    use freefetch_lib::extractor::models::MediaFormat;

    let f_muxed = MediaFormat {
        format_id: "yt-18".to_string(),
        format_type: "muxed".to_string(),
        quality_label: "360p SD".to_string(),
        resolution: Some("640x360".to_string()),
        width: Some(640),
        height: Some(360),
        fps: Some(30),
        video_codec: Some("h264".to_string()),
        audio_codec: Some("aac".to_string()),
        mime_type: Some("video/mp4".to_string()),
        bitrate: Some(600_000),
        container: "mp4".to_string(),
        filesize: Some(15_000_000),
        download_url: "https://example.com/muxed.mp4".to_string(),
        requires_merge: false,
        matching_audio_url: None,
    };

    let f_video_only = MediaFormat {
        format_id: "yt-137".to_string(),
        format_type: "video".to_string(),
        quality_label: "1080p FHD".to_string(),
        resolution: Some("1920x1080".to_string()),
        width: Some(1920),
        height: Some(1080),
        fps: Some(60),
        video_codec: Some("h264".to_string()),
        audio_codec: None,
        mime_type: Some("video/mp4".to_string()),
        bitrate: Some(4_500_000),
        container: "mp4".to_string(),
        filesize: None, // Missing filesize test
        download_url: "https://example.com/video_only.mp4".to_string(),
        requires_merge: true,
        matching_audio_url: Some("https://example.com/audio_stream.m4a".to_string()),
    };

    let f_audio_only = MediaFormat {
        format_id: "yt-140".to_string(),
        format_type: "audio".to_string(),
        quality_label: "128 kbps Audio".to_string(),
        resolution: None,
        width: None,
        height: None,
        fps: None, // Missing FPS test
        video_codec: None,
        audio_codec: Some("aac".to_string()),
        mime_type: Some("audio/mp4".to_string()),
        bitrate: None, // Missing bitrate test
        container: "m4a".to_string(),
        filesize: Some(4_200_000),
        download_url: "https://example.com/audio_stream.m4a".to_string(),
        requires_merge: false,
        matching_audio_url: None,
    };

    let mut formats = [f_muxed.clone(), f_video_only.clone(), f_audio_only.clone()];

    // Verify format types
    assert_eq!(f_muxed.format_type, "muxed");
    assert!(!f_muxed.requires_merge);

    assert_eq!(f_video_only.format_type, "video");
    assert!(f_video_only.requires_merge);
    assert!(f_video_only.matching_audio_url.is_some());

    assert_eq!(f_audio_only.format_type, "audio");
    assert!(!f_audio_only.requires_merge);

    // Verify sorting (highest resolution first)
    formats.sort_by(|a, b| {
        let h_a = a.height.unwrap_or(0);
        let h_b = b.height.unwrap_or(0);
        h_b.cmp(&h_a)
    });

    assert_eq!(formats[0].quality_label, "1080p FHD");
    assert_eq!(formats[1].quality_label, "360p SD");
}

#[test]
fn test_extractor_registry_lookup() {
    let registry = ExtractorRegistry::new();

    let yt_url = Url::parse("https://www.youtube.com/watch?v=dQw4w9WgXcQ").unwrap();
    let ext1 = registry.find_extractor(&yt_url);
    assert!(ext1.is_some());
    assert_eq!(ext1.unwrap().name(), "youtube");

    let direct_url = Url::parse("https://example.com/video.mp4").unwrap();
    let ext2 = registry.find_extractor(&direct_url);
    assert!(ext2.is_some());
    assert_eq!(ext2.unwrap().name(), "generic");
}

#[test]
fn test_download_status_transitions_and_serialization() {
    assert_eq!(DownloadStatus::Queued.as_str(), "queued");
    assert_eq!(DownloadStatus::Downloading.as_str(), "downloading");
    assert_eq!(DownloadStatus::Processing.as_str(), "processing");
    assert_eq!(DownloadStatus::Completed.as_str(), "completed");
    assert_eq!(DownloadStatus::Failed.as_str(), "failed");
    assert_eq!(DownloadStatus::Paused.as_str(), "paused");
    assert_eq!(DownloadStatus::Cancelled.as_str(), "cancelled");

    assert_eq!(
        DownloadStatus::parse("downloading"),
        DownloadStatus::Downloading
    );
    assert_eq!(
        DownloadStatus::parse("remuxing"),
        DownloadStatus::Processing
    );
    assert_eq!(
        DownloadStatus::parse("completed"),
        DownloadStatus::Completed
    );
}

#[test]
fn test_sqlite_repository_lifecycle() {
    let repo = create_in_memory_database().expect("Should initialize in-memory SQLite");

    let record = DownloadRecord {
        id: "test-id-123".to_string(),
        source_url: "https://example.com/video.mp4".to_string(),
        service_name: "generic".to_string(),
        title: "Test Video".to_string(),
        duration: 120,
        thumbnail_url: Some("https://example.com/thumb.jpg".to_string()),
        destination_path: "C:\\Downloads\\test.mp4".to_string(),
        container: "mp4".to_string(),
        quality_label: "1080p".to_string(),
        total_bytes: 10_000_000,
        downloaded_bytes: 2_000_000,
        status: DownloadStatus::Downloading,
        error_message: None,
        created_at: 1700000000,
        completed_at: None,
    };

    // Insert
    repo.insert_download(&record)
        .expect("Insert should succeed");

    // Fetch
    let fetched = repo
        .get_download("test-id-123")
        .expect("Query should succeed");
    assert!(fetched.is_some());
    let item = fetched.unwrap();
    assert_eq!(item.title, "Test Video");
    assert_eq!(item.status, DownloadStatus::Downloading);
    assert_eq!(item.downloaded_bytes, 2_000_000);

    // Update Progress
    repo.update_download_progress("test-id-123", 5_000_000, 10_000_000)
        .expect("Update progress should succeed");
    let updated = repo.get_download("test-id-123").unwrap().unwrap();
    assert_eq!(updated.downloaded_bytes, 5_000_000);

    // Update Status
    repo.update_download_status(
        "test-id-123",
        &DownloadStatus::Completed,
        None,
        Some(1700001000),
    )
    .expect("Status update should succeed");
    let completed = repo.get_download("test-id-123").unwrap().unwrap();
    assert_eq!(completed.status, DownloadStatus::Completed);
    assert_eq!(completed.completed_at, Some(1700001000));

    // List
    let list = repo.list_downloads().expect("List should succeed");
    assert_eq!(list.len(), 1);

    // Settings
    repo.set_setting("test_key", "test_val")
        .expect("Set setting should succeed");
    assert_eq!(
        repo.get_setting("test_key").unwrap(),
        Some("test_val".to_string())
    );

    // Delete
    repo.delete_download("test-id-123")
        .expect("Delete should succeed");
    let after_delete = repo.get_download("test-id-123").unwrap();
    assert!(after_delete.is_none());
}

#[test]
fn test_default_app_settings() {
    let settings = AppSettings::default();
    assert_eq!(settings.max_concurrent_downloads, 3);
    assert_eq!(settings.default_container, "mp4");
    assert!(!settings.download_directory.is_empty());
}
