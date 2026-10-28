use freefetch_lib::engine::state::{
    ensure_safe_destination_path, ensure_safe_destination_path_with_reserved,
};
use freefetch_lib::engine::template::{FilenameTemplateEngine, TemplateContext};
use uuid::Uuid;

#[test]
fn test_part_file_collision_avoids_overwriting_in_flight_download() {
    let temp_dir = std::env::temp_dir().join(format!("ff_collision_test_{}", Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();

    // 1. Initial destination for video
    let p1 = ensure_safe_destination_path(&temp_dir, "GILA ADA ADVANTURE TIME", "mp4");
    assert_eq!(p1.file_name().unwrap(), "GILA ADA ADVANTURE TIME.mp4");

    // Simulate in-flight download of Job 1 creating video.part and audio.part
    let video_part = p1.with_extension("video.part");
    let audio_part = p1.with_extension("audio.part");
    std::fs::write(&video_part, b"dummy video stream").unwrap();
    std::fs::write(&audio_part, b"dummy audio stream").unwrap();

    // 2. Second download with identical title while Job 1 is downloading
    // Crucial: p1 (.mp4) does not exist yet, but its .video.part and .audio.part DO exist!
    let p2 = ensure_safe_destination_path(&temp_dir, "GILA ADA ADVANTURE TIME", "mp4");
    assert_eq!(
        p2.file_name().unwrap(),
        "GILA ADA ADVANTURE TIME (1).mp4",
        "Must increment counter to prevent colliding with in-flight .part files"
    );

    // Verify p2 uses independent part files
    let p2_video = p2.with_extension("video.part");
    assert!(!p2_video.exists());
    assert_ne!(video_part, p2_video);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_reserved_paths_avoid_queue_collision_before_files_are_created() {
    let temp_dir = std::env::temp_dir().join(format!("ff_reserved_test_{}", Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();

    let target_str = temp_dir.join("My Video.mp4").to_string_lossy().to_string();
    let reserved = vec![target_str];

    // Even if no file exists on disk yet, reserved path from SQLite queue must be avoided
    let p = ensure_safe_destination_path_with_reserved(&temp_dir, "My Video", "mp4", &reserved);
    assert_eq!(p.file_name().unwrap(), "My Video (1).mp4");

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_template_render_with_reserved_paths() {
    let temp_dir = std::env::temp_dir().join(format!("ff_tmpl_res_{}", Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();

    let target_str = temp_dir
        .join("Coldplay - Yellow.mp3")
        .to_string_lossy()
        .to_string();
    let reserved = vec![target_str];

    let ctx = TemplateContext {
        title: "Yellow".to_string(),
        artist: Some("Coldplay".to_string()),
        ext: "mp3".to_string(),
        ..TemplateContext::default()
    };

    let p =
        FilenameTemplateEngine::render_destination_with_reserved(&temp_dir, None, &ctx, &reserved);
    assert_eq!(p.file_name().unwrap(), "Coldplay - Yellow (1).mp3");

    let _ = std::fs::remove_dir_all(&temp_dir);
}
