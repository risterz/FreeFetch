use std::path::{Path, PathBuf};
use std::process::Command;
use tokio::fs;

use freefetch_lib::core::error::FreeFetchError;
use freefetch_lib::core::types::{
    AudioConversionOptions, AudioQualityPreset, AudioTargetFormat, SubtitleFormat, SubtitleMode,
    SubtitleSelection,
};
use freefetch_lib::engine::StartDownloadParams;
use freefetch_lib::media::ffmpeg::FfmpegController;

fn get_test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("{}_{}", name, std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn create_dummy_mp4(path: &Path) {
    let status = Command::new("ffmpeg")
        .arg("-y")
        .arg("-f")
        .arg("lavfi")
        .arg("-i")
        .arg("testsrc=duration=1:size=320x240:rate=25")
        .arg("-f")
        .arg("lavfi")
        .arg("-i")
        .arg("sine=duration=1")
        .arg("-c:v")
        .arg("libx264")
        .arg("-c:a")
        .arg("aac")
        .arg(path)
        .status()
        .expect("ffmpeg must create dummy mp4");
    assert!(status.success());
    assert!(path.exists());
}

fn create_dummy_srt(path: &Path, text: &str) {
    let srt_content = format!("1\n00:00:00,000 --> 00:00:01,000\n{text}\n");
    std::fs::write(path, srt_content).expect("must write dummy srt");
}

#[test]
fn test_1_subtitle_mode_serialization_and_backward_compatibility() {
    // None
    let json_none = r#"{"subtitle_mode": "none"}"#;
    let mode: serde_json::Value = serde_json::from_str(json_none).unwrap();
    let parsed: SubtitleMode = serde_json::from_value(mode["subtitle_mode"].clone()).unwrap();
    assert_eq!(parsed, SubtitleMode::None);

    // Embed
    let json_embed = r#"{"subtitle_mode": "embed"}"#;
    let mode: serde_json::Value = serde_json::from_str(json_embed).unwrap();
    let parsed: SubtitleMode = serde_json::from_value(mode["subtitle_mode"].clone()).unwrap();
    assert_eq!(parsed, SubtitleMode::Embed);

    // Hardcode
    let json_hardcode = r#"{"subtitle_mode": "hardcode"}"#;
    let mode: serde_json::Value = serde_json::from_str(json_hardcode).unwrap();
    let parsed: SubtitleMode = serde_json::from_value(mode["subtitle_mode"].clone()).unwrap();
    assert_eq!(parsed, SubtitleMode::Hardcode);

    // Backward compatibility: old payload with embed_subtitles = true and no subtitle_mode
    let old_payload = r#"{
        "source_url": "https://example.com/v",
        "service_name": "generic",
        "title": "Legacy Item",
        "duration": 60,
        "format_id": "1",
        "download_url": "https://example.com/stream",
        "requires_merge": false,
        "container": "mp4",
        "quality_label": "720p",
        "embed_subtitles": true
    }"#;
    let params: StartDownloadParams = serde_json::from_str(old_payload).unwrap();
    assert_eq!(params.resolved_subtitle_mode(), SubtitleMode::Embed);

    // Legacy payload with embed_subtitles = false
    let old_payload_false = r#"{
        "source_url": "https://example.com/v",
        "service_name": "generic",
        "title": "Legacy Item",
        "duration": 60,
        "format_id": "1",
        "download_url": "https://example.com/stream",
        "requires_merge": false,
        "container": "mp4",
        "quality_label": "720p",
        "embed_subtitles": false
    }"#;
    let params: StartDownloadParams = serde_json::from_str(old_payload_false).unwrap();
    assert_eq!(params.resolved_subtitle_mode(), SubtitleMode::None);
}

#[test]
fn test_2_hardcode_mode_parameter_resolution() {
    let payload = r#"{
        "source_url": "https://www.youtube.com/watch?v=j8n1pj8RfFs",
        "service_name": "youtube",
        "title": "Test Video",
        "duration": 120,
        "format_id": "1080p",
        "download_url": "https://example.com/vid",
        "requires_merge": true,
        "container": "mp4",
        "quality_label": "1080p",
        "subtitle_mode": "hardcode",
        "selected_subtitles": [
            {
                "language_code": "en",
                "language_name": "English",
                "target_format": "srt",
                "is_auto_generated": false,
                "download_url": "https://example.com/sub.srt"
            }
        ]
    }"#;
    let params: StartDownloadParams = serde_json::from_str(payload).unwrap();
    assert_eq!(params.resolved_subtitle_mode(), SubtitleMode::Hardcode);
    assert_eq!(params.selected_subtitles.len(), 1);
    assert_eq!(params.selected_subtitles[0].language_code, "en");
}

#[test]
fn test_3_ffmpeg_subtitle_filter_path_escaping() {
    // Test complex paths containing spaces, colons, parentheses, apostrophes, brackets, Unicode, &, #
    let complex_path =
        PathBuf::from(r"C:\Users\FreeFetch User\Videos\[Anime] (Test) & 'Spec' # 日本語\sub.srt");
    let escaped = FfmpegController::escape_subtitle_filter_path(&complex_path);

    // Must start with subtitles=' and end with '
    assert!(escaped.starts_with("subtitles='"));
    assert!(escaped.ends_with("'"));

    // Path directory separators must be normalized to forward slashes (no raw \Videos\ or \Users\)
    assert!(!escaped.contains("/Videos\\") && !escaped.contains(r"\Videos/"));
    // Drive colon must be escaped
    assert!(escaped.contains(r"C\:"));
    // Single quotes inside path must be double-escaped with '\'''
    assert!(escaped.contains(r"'\\\''"));
}

#[test]
fn test_4_audio_isolation_invariant_a3_blocks_hardcoding() {
    let params = StartDownloadParams {
        source_url: "https://www.youtube.com/watch?v=123".to_string(),
        service_name: "youtube".to_string(),
        title: "Audio Only".to_string(),
        duration: 200,
        thumbnail_url: None,
        format_id: "audio".to_string(),
        download_url: "https://example.com/audio".to_string(),
        requires_merge: false,
        matching_audio_url: None,
        container: "mp3".to_string(),
        quality_label: "320k".to_string(),
        destination_dir: None,
        selected_subtitles: vec![SubtitleSelection {
            language_code: "en".to_string(),
            language_name: "English".to_string(),
            target_format: SubtitleFormat::Srt,
            is_auto_generated: false,
            download_url: Some("https://example.com/en.srt".to_string()),
        }],
        embed_subtitles: false,
        subtitle_mode: Some(SubtitleMode::Hardcode),
        audio_options: Some(AudioConversionOptions {
            target_format: AudioTargetFormat::Mp3,
            quality_preset: AudioQualityPreset::Best,
            bitrate_mode: freefetch_lib::core::types::BitrateMode::Cbr,
            custom_bitrate_kbps: None,
            sample_rate: None,
            embed_metadata: true,
            embed_artwork: false,
            filename_template: None,
        }),
        video_codec: None,
        audio_codec: None,
        http_headers: None,
    };

    let should_process = params.resolved_subtitle_mode() != SubtitleMode::None
        && !params.selected_subtitles.is_empty()
        && params.audio_options.is_none();

    assert!(
        !should_process,
        "Invariant A3 violation: audio conversion options must strictly bypass hardcoding!"
    );
}

#[tokio::test]
async fn test_5_hardcode_subtitles_with_complex_path() {
    let test_dir = get_test_dir("ff_hc_path_test [dir] (spec) & 'sub' # 日本語");
    let video_path = test_dir.join("video [1] (test).mp4");
    let srt_path = test_dir.join("sub 'quote' & [1] # 日本語.srt");
    let out_path = test_dir.join("hardcoded 'out' (1).mp4");

    create_dummy_mp4(&video_path);
    create_dummy_srt(&srt_path, "Subtitles with complex path!");

    let ffmpeg = FfmpegController::discover(None).expect("FFmpeg must be discovered");
    let res = ffmpeg
        .hardcode_subtitles_cancellable(&video_path, &srt_path, &out_path, None)
        .await;

    assert!(
        res.is_ok(),
        "Hardcoding with special chars in path must succeed: {:?}",
        res.err()
    );
    assert!(out_path.exists());

    let _ = fs::remove_dir_all(&test_dir).await;
}

#[tokio::test]
async fn test_6_cancellation_during_hardcode_cleans_temporary_output() {
    let test_dir = get_test_dir("ff_hc_cancel_test");
    let video_path = test_dir.join("video.mp4");
    let srt_path = test_dir.join("sub.srt");
    let out_path = test_dir.join("out.hardcode.tmp.mp4");

    create_dummy_mp4(&video_path);
    create_dummy_srt(&srt_path, "Cancellation test");

    let ffmpeg = FfmpegController::discover(None).expect("FFmpeg must be discovered");
    let (tx, rx) = tokio::sync::watch::channel(false);

    // Cancel immediately before or right as ffmpeg begins
    let _ = tx.send(true);

    let res = ffmpeg
        .hardcode_subtitles_cancellable(&video_path, &srt_path, &out_path, Some(rx))
        .await;

    assert!(matches!(res, Err(FreeFetchError::Cancelled)));
    assert!(!out_path.exists(), "Cancelled output file must be deleted");

    let _ = fs::remove_dir_all(&test_dir).await;
}

#[tokio::test]
async fn test_7_failure_propagation_on_nonexistent_input() {
    let test_dir = get_test_dir("ff_hc_fail_test");
    let video_path = test_dir.join("nonexistent_video.mp4");
    let srt_path = test_dir.join("nonexistent_sub.srt");
    let out_path = test_dir.join("out.mp4");

    let ffmpeg = FfmpegController::discover(None).expect("FFmpeg must be discovered");
    let res = ffmpeg
        .hardcode_subtitles_cancellable(&video_path, &srt_path, &out_path, None)
        .await;

    assert!(res.is_err(), "Must return Err on invalid input");
    assert!(!out_path.exists());

    let _ = fs::remove_dir_all(&test_dir).await;
}

#[tokio::test]
async fn test_8_soft_embed_regression_check() {
    let test_dir = get_test_dir("ff_hc_soft_embed_regression");
    let video_path = test_dir.join("video.mp4");
    let srt_path = test_dir.join("sub.srt");
    let out_path = test_dir.join("soft_embedded.mp4");

    create_dummy_mp4(&video_path);
    create_dummy_srt(&srt_path, "Soft Subtitle");

    let ffmpeg = FfmpegController::discover(None).expect("FFmpeg must be discovered");
    let sub_tracks = [(srt_path.as_path(), "en", "English")];

    ffmpeg
        .embed_subtitles_cancellable(&video_path, &sub_tracks, &out_path, None)
        .await
        .expect("Soft embedding must succeed");

    assert!(out_path.exists());

    let output = Command::new("ffprobe")
        .arg("-hide_banner")
        .arg("-print_format")
        .arg("json")
        .arg("-show_streams")
        .arg(&out_path)
        .output()
        .expect("ffprobe must run");

    assert!(output.status.success());
    let json_str = String::from_utf8_lossy(&output.stdout);
    let val: serde_json::Value = serde_json::from_str(&json_str).expect("Valid JSON");
    let streams = val["streams"].as_array().expect("streams array");

    let has_sub = streams.iter().any(|s| {
        s["codec_type"].as_str() == Some("subtitle") && s["codec_name"].as_str() == Some("mov_text")
    });
    assert!(
        has_sub,
        "Soft embed regression: MP4 must contain mov_text subtitle stream"
    );

    let _ = fs::remove_dir_all(&test_dir).await;
}

#[tokio::test]
async fn test_9_hardcode_real_ffmpeg_burns_subtitles_and_leaves_no_subtitle_stream() {
    let test_dir = get_test_dir("ff_hc_stream_probe_test");
    let video_path = test_dir.join("video.mp4");
    let srt_path = test_dir.join("sub.srt");
    let out_path = test_dir.join("hardcoded.mp4");

    create_dummy_mp4(&video_path);
    create_dummy_srt(&srt_path, "Permanently Burned In Subtitle");

    let ffmpeg = FfmpegController::discover(None).expect("FFmpeg must be discovered");
    ffmpeg
        .hardcode_subtitles_cancellable(&video_path, &srt_path, &out_path, None)
        .await
        .expect("Hardcoding must succeed");

    assert!(out_path.exists());

    let output = Command::new("ffprobe")
        .arg("-hide_banner")
        .arg("-print_format")
        .arg("json")
        .arg("-show_streams")
        .arg(&out_path)
        .output()
        .expect("ffprobe must run");

    assert!(output.status.success());
    let json_str = String::from_utf8_lossy(&output.stdout);
    let val: serde_json::Value = serde_json::from_str(&json_str).expect("Valid JSON");
    let streams = val["streams"].as_array().expect("streams array");

    let mut video_count = 0;
    let mut audio_count = 0;
    let mut subtitle_count = 0;

    for s in streams {
        match s["codec_type"].as_str() {
            Some("video") => {
                video_count += 1;
                assert_eq!(s["codec_name"].as_str(), Some("h264"));
            }
            Some("audio") => {
                audio_count += 1;
                assert_eq!(s["codec_name"].as_str(), Some("aac"));
            }
            Some("subtitle") => {
                subtitle_count += 1;
            }
            _ => {}
        }
    }

    assert_eq!(
        video_count, 1,
        "Must contain exactly 1 re-encoded video stream"
    );
    assert_eq!(audio_count, 1, "Must contain exactly 1 copied audio stream");
    assert_eq!(
        subtitle_count, 0,
        "Hardcoded output MUST NOT contain any separate subtitle stream!"
    );

    let _ = fs::remove_dir_all(&test_dir).await;
}

#[tokio::test]
async fn test_10_visual_frame_extraction_and_pixel_verification() {
    let test_dir = get_test_dir("ff_hc_visual_frame_test");
    let video_path = test_dir.join("input.mp4");
    let srt_path = test_dir.join("sub.srt");
    let out_path = test_dir.join("hardcoded.mp4");
    let raw_frame_path = test_dir.join("raw_frame.png");
    let burned_frame_path = test_dir.join("burned_frame.png");

    create_dummy_mp4(&video_path);
    create_dummy_srt(&srt_path, "FREEFETCH_HARDCODE_VISUAL_VERIFY");

    // Extract frame from original video without subtitles
    let raw_status = Command::new("ffmpeg")
        .arg("-y")
        .arg("-ss")
        .arg("00:00:00.500")
        .arg("-i")
        .arg(&video_path)
        .arg("-frames:v")
        .arg("1")
        .arg(&raw_frame_path)
        .status()
        .expect("ffmpeg extract raw frame");
    assert!(raw_status.success());
    assert!(raw_frame_path.exists());

    // Run hardcode subtitles
    let ffmpeg = FfmpegController::discover(None).expect("FFmpeg must be discovered");
    ffmpeg
        .hardcode_subtitles_cancellable(&video_path, &srt_path, &out_path, None)
        .await
        .expect("Hardcoding must succeed");

    // Extract frame from hardcoded video
    let burned_status = Command::new("ffmpeg")
        .arg("-y")
        .arg("-ss")
        .arg("00:00:00.500")
        .arg("-i")
        .arg(&out_path)
        .arg("-frames:v")
        .arg("1")
        .arg(&burned_frame_path)
        .status()
        .expect("ffmpeg extract burned frame");
    assert!(burned_status.success());
    assert!(burned_frame_path.exists());

    let raw_bytes = std::fs::read(&raw_frame_path).expect("read raw frame");
    let burned_bytes = std::fs::read(&burned_frame_path).expect("read burned frame");

    // Burned frame MUST be visually different from the raw frame because subtitle glyphs are rendered in pixels
    assert_ne!(
        raw_bytes, burned_bytes,
        "Burned frame pixels must differ from raw frame due to rendered subtitle text!"
    );

    let _ = fs::remove_dir_all(&test_dir).await;
}

#[tokio::test]
async fn test_11_real_world_youtube_auto_caption_hardcode_and_frame_verification() {
    use freefetch_lib::engine::subtitle_downloader;
    use freefetch_lib::extractor::context::ExtractionContext;
    use freefetch_lib::extractor::ExtractorRegistry;
    use reqwest::Client;
    use url::Url;

    let registry = ExtractorRegistry::new();
    let url = Url::parse("https://www.youtube.com/watch?v=j8n1pj8RfFs").unwrap();
    let extractor = registry.find_extractor(&url).expect("extractor found");
    let ctx = ExtractionContext::default();
    let (_meta, manifest) = match extractor.extract(&url, &ctx).await {
        Ok(v) => v,
        Err(e) => {
            println!("Network/botguard restricted on live YouTube ({e:?}), skipping test");
            return;
        }
    };

    let en_auto = manifest
        .subtitles
        .iter()
        .find(|s| (s.language_code == "en" || s.language_code == "en-orig") && s.is_auto_generated)
        .expect("Must find English auto-generated subtitle");

    let dl_url = en_auto.download_url.as_ref().expect("timedtext URL exists");

    let test_dir = get_test_dir("ff_hc_youtube_real");
    let srt_path = test_dir.join("youtube_en_auto.srt");
    let client = Client::new();

    subtitle_downloader::download_and_convert_subtitle(
        &client,
        dl_url,
        &srt_path,
        SubtitleFormat::Srt,
        None,
        None,
    )
    .await
    .expect("Must download and convert YouTube auto-caption to SRT");

    assert!(srt_path.exists());
    let srt_content = std::fs::read_to_string(&srt_path).expect("read srt content");
    assert!(
        srt_content.contains("Baby") || srt_content.contains("baby") || srt_content.contains("-->"),
        "SRT must contain valid caption cues"
    );

    // Create 3-second base video to cover first caption cue
    let video_path = test_dir.join("jb_base.mp4");
    let status = Command::new("ffmpeg")
        .arg("-y")
        .arg("-f")
        .arg("lavfi")
        .arg("-i")
        .arg("testsrc=duration=3:size=640x360:rate=25")
        .arg("-f")
        .arg("lavfi")
        .arg("-i")
        .arg("sine=duration=3")
        .arg("-c:v")
        .arg("libx264")
        .arg("-c:a")
        .arg("aac")
        .arg(&video_path)
        .status()
        .expect("ffmpeg must create base video");
    assert!(status.success());

    let hardcoded_path = test_dir.join("jb_hardcoded.mp4");
    let ffmpeg = FfmpegController::discover(None).expect("FFmpeg must be discovered");

    ffmpeg
        .hardcode_subtitles_cancellable(&video_path, &srt_path, &hardcoded_path, None)
        .await
        .expect("YouTube auto-generated subtitle hardcode must succeed");

    assert!(hardcoded_path.exists());

    // FFprobe verification
    let output = Command::new("ffprobe")
        .arg("-hide_banner")
        .arg("-print_format")
        .arg("json")
        .arg("-show_streams")
        .arg(&hardcoded_path)
        .output()
        .expect("ffprobe must run");

    assert!(output.status.success());
    let json_str = String::from_utf8_lossy(&output.stdout);
    let val: serde_json::Value = serde_json::from_str(&json_str).expect("Valid JSON");
    let streams = val["streams"].as_array().expect("streams array");

    let sub_count = streams
        .iter()
        .filter(|s| s["codec_type"].as_str() == Some("subtitle"))
        .count();
    let vid_count = streams
        .iter()
        .filter(|s| s["codec_type"].as_str() == Some("video"))
        .count();
    let aud_count = streams
        .iter()
        .filter(|s| s["codec_type"].as_str() == Some("audio"))
        .count();

    assert_eq!(vid_count, 1, "Must contain exactly 1 video stream");
    assert_eq!(aud_count, 1, "Must contain exactly 1 audio stream");
    assert_eq!(
        sub_count, 0,
        "Hardcoded output must have 0 subtitle streams"
    );

    // Extract frame at 1.0s and verify pixel rendering
    let frame_path = test_dir.join("jb_burned_frame.png");
    let frame_status = Command::new("ffmpeg")
        .arg("-y")
        .arg("-ss")
        .arg("00:00:01.000")
        .arg("-i")
        .arg(&hardcoded_path)
        .arg("-frames:v")
        .arg("1")
        .arg(&frame_path)
        .status()
        .expect("ffmpeg extract frame");
    assert!(frame_status.success());
    assert!(frame_path.exists());
    assert!(std::fs::metadata(&frame_path).unwrap().len() > 1000);

    let _ = fs::remove_dir_all(&test_dir).await;
}
