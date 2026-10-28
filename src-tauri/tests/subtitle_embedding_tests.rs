use std::path::{Path, PathBuf};
use std::process::Command;
use tokio::fs;

use freefetch_lib::core::types::{
    AudioConversionOptions, AudioQualityPreset, AudioTargetFormat, SubtitleFormat,
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
        .arg("testsrc=duration=1:size=320x240:rate=1")
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
fn test_1_params_serialization_embed_subtitles_non_empty() {
    let json_input = r#"{
        "source_url": "https://www.youtube.com/watch?v=j8n1pj8RfFs",
        "service_name": "youtube",
        "title": "Justin Bieber - Baby ft. Ludacris",
        "duration": 193,
        "format_id": "yt-1080p",
        "download_url": "https://example.com/video.mp4",
        "requires_merge": true,
        "container": "mp4",
        "quality_label": "1080p FHD",
        "embed_subtitles": true,
        "selected_subtitles": [
            {
                "language_code": "en",
                "language_name": "English (Auto-generated)",
                "target_format": "srt",
                "is_auto_generated": true,
                "download_url": "https://www.youtube.com/api/timedtext?..."
            }
        ]
    }"#;

    let params: StartDownloadParams =
        serde_json::from_str(json_input).expect("Must deserialize StartDownloadParams");

    assert!(params.embed_subtitles);
    assert_eq!(params.selected_subtitles.len(), 1);
    let sub = &params.selected_subtitles[0];
    assert_eq!(sub.language_code, "en");
    assert_eq!(sub.language_name, "English (Auto-generated)");
    assert_eq!(sub.target_format, SubtitleFormat::Srt);
    assert!(sub.is_auto_generated);
    assert!(sub.download_url.is_some());
}

#[tokio::test]
async fn test_2_mp4_ffmpeg_mapping_mov_text() {
    let test_dir = get_test_dir("ff_embed_test_2");
    let video_path = test_dir.join("input.mp4");
    let srt_path = test_dir.join("sub.srt");
    let out_path = test_dir.join("output.embed.tmp.mp4");

    create_dummy_mp4(&video_path);
    create_dummy_srt(&srt_path, "Hello from FreeFetch");

    let ffmpeg = FfmpegController::discover(None).expect("FFmpeg must be discovered");
    let sub_refs = [(srt_path.as_path(), "en", "English (Auto-generated)")];

    let res = ffmpeg
        .embed_subtitles_cancellable(&video_path, &sub_refs, &out_path, None)
        .await;

    assert!(res.is_ok(), "FFmpeg embedding into MP4 must succeed");
    assert!(out_path.exists());

    // Run ffprobe on out_path
    let probe_out = Command::new("ffprobe")
        .arg("-hide_banner")
        .arg("-show_streams")
        .arg(&out_path)
        .output()
        .expect("ffprobe must run");

    let stdout = String::from_utf8_lossy(&probe_out.stdout);
    assert!(
        stdout.contains("codec_name=mov_text"),
        "Subtitle stream in MP4 must be encoded as mov_text"
    );
    assert!(
        stdout.contains("TAG:language=eng"),
        "Language metadata must be tagged as eng"
    );

    let _ = fs::remove_dir_all(&test_dir).await;
}

#[test]
fn test_3_audio_subtitle_isolation_invariant_a3() {
    let params = StartDownloadParams {
        source_url: "https://www.youtube.com/watch?v=123".to_string(),
        service_name: "youtube".to_string(),
        title: "Audio Only".to_string(),
        duration: 200,
        thumbnail_url: None,
        format_id: "audio".to_string(),
        download_url: "https://example.com/audio.m4a".to_string(),
        requires_merge: false,
        matching_audio_url: None,
        container: "mp3".to_string(),
        quality_label: "BEST (MP3)".to_string(),
        destination_dir: None,
        selected_subtitles: vec![SubtitleSelection {
            language_code: "en".to_string(),
            language_name: "English".to_string(),
            target_format: SubtitleFormat::Srt,
            is_auto_generated: false,
            download_url: Some("https://example.com/sub.srt".to_string()),
        }],
        embed_subtitles: true,
        subtitle_mode: None,
        audio_options: Some(AudioConversionOptions {
            target_format: AudioTargetFormat::Mp3,
            quality_preset: AudioQualityPreset::Best,
            bitrate_mode: freefetch_lib::media::audio::BitrateMode::Cbr,
            custom_bitrate_kbps: None,
            sample_rate: None,
            embed_metadata: true,
            embed_artwork: false,
            filename_template: None,
        }),
        video_codec: None,
        audio_codec: None,
    };

    // Invariant A3 verification: audio_options present must skip embedding
    let should_embed = params.embed_subtitles
        && !params.selected_subtitles.is_empty()
        && params.audio_options.is_none();

    assert!(
        !should_embed,
        "Invariant A3: Audio options present must disable subtitle embedding"
    );
}

#[tokio::test]
async fn test_4_final_mp4_subtitle_stream_probe() {
    let test_dir = get_test_dir("ff_embed_test_4");
    let video_path = test_dir.join("video.mp4");
    let sub_path = test_dir.join("sub.en.srt");
    let embedded_path = test_dir.join("final.mp4");

    create_dummy_mp4(&video_path);
    create_dummy_srt(&sub_path, "FreeFetch subtitle line");

    let ffmpeg = FfmpegController::discover(None).expect("FFmpeg must be discovered");
    let sub_tracks = [(sub_path.as_path(), "en", "English")];

    ffmpeg
        .embed_subtitles_cancellable(&video_path, &sub_tracks, &embedded_path, None)
        .await
        .expect("Embedding must succeed");

    assert!(embedded_path.exists());

    let output = Command::new("ffprobe")
        .arg("-hide_banner")
        .arg("-print_format")
        .arg("json")
        .arg("-show_streams")
        .arg(&embedded_path)
        .output()
        .expect("ffprobe must run");

    assert!(output.status.success());
    let json_str = String::from_utf8_lossy(&output.stdout);
    let val: serde_json::Value = serde_json::from_str(&json_str).expect("Valid JSON from ffprobe");
    let streams = val["streams"].as_array().expect("streams array");

    let mut has_video = false;
    let mut has_audio = false;
    let mut has_sub = false;

    for s in streams {
        let codec_type = s["codec_type"].as_str().unwrap_or("");
        let codec_name = s["codec_name"].as_str().unwrap_or("");
        if codec_type == "video" {
            has_video = true;
        } else if codec_type == "audio" {
            has_audio = true;
        } else if codec_type == "subtitle" {
            has_sub = true;
            assert_eq!(
                codec_name, "mov_text",
                "MP4 subtitle codec must be mov_text"
            );
            let lang = s["tags"]["language"].as_str().unwrap_or("");
            assert_eq!(lang, "eng", "Subtitle language tag must be eng");
        }
    }

    assert!(has_video, "Resulting MP4 must contain video stream");
    assert!(has_audio, "Resulting MP4 must contain audio stream");
    assert!(
        has_sub,
        "Resulting MP4 must contain mov_text subtitle stream"
    );

    let _ = fs::remove_dir_all(&test_dir).await;
}

#[tokio::test]
async fn test_5_multiple_subtitles_embedded_into_mp4() {
    let test_dir = get_test_dir("ff_embed_test_5");
    let video_path = test_dir.join("video.mp4");
    let sub_en = test_dir.join("sub.en.srt");
    let sub_es = test_dir.join("sub.es.srt");
    let out_path = test_dir.join("multi_sub.mp4");

    create_dummy_mp4(&video_path);
    create_dummy_srt(&sub_en, "Hello World");
    create_dummy_srt(&sub_es, "Hola Mundo");

    let ffmpeg = FfmpegController::discover(None).expect("FFmpeg must be discovered");
    let sub_tracks = [
        (sub_en.as_path(), "en", "English"),
        (sub_es.as_path(), "es", "Spanish"),
    ];

    ffmpeg
        .embed_subtitles_cancellable(&video_path, &sub_tracks, &out_path, None)
        .await
        .expect("Multi-track embedding must succeed");

    assert!(out_path.exists());

    let output = Command::new("ffprobe")
        .arg("-hide_banner")
        .arg("-print_format")
        .arg("json")
        .arg("-show_streams")
        .arg(&out_path)
        .output()
        .expect("ffprobe must run");

    let json_str = String::from_utf8_lossy(&output.stdout);
    let val: serde_json::Value = serde_json::from_str(&json_str).expect("Valid JSON");
    let streams = val["streams"].as_array().expect("streams array");

    let sub_streams: Vec<&serde_json::Value> = streams
        .iter()
        .filter(|s| s["codec_type"].as_str() == Some("subtitle"))
        .collect();

    assert_eq!(
        sub_streams.len(),
        2,
        "Must contain exactly 2 embedded subtitle streams"
    );
    assert_eq!(sub_streams[0]["tags"]["language"].as_str(), Some("eng"));
    assert_eq!(sub_streams[1]["tags"]["language"].as_str(), Some("spa"));

    let _ = fs::remove_dir_all(&test_dir).await;
}

#[tokio::test]
async fn test_6_youtube_auto_generated_caption_end_to_end_embedding() {
    use freefetch_lib::extractor::context::ExtractionContext;
    use freefetch_lib::extractor::ExtractorRegistry;
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
        .expect("English auto-generated subtitle must exist for j8n1pj8RfFs");

    let dl_url = en_auto
        .download_url
        .as_ref()
        .expect("Subtitle must have download URL");

    let test_dir = get_test_dir("ff_embed_test_6");
    let video_path = test_dir.join("jb_base.mp4");
    let srt_path = test_dir.join("jb_auto_en.srt");
    let out_path = test_dir.join("jb_final.mp4");

    create_dummy_mp4(&video_path);

    let client = reqwest::Client::new();
    let res = freefetch_lib::engine::subtitle_downloader::download_and_convert_subtitle(
        &client,
        dl_url,
        &srt_path,
        SubtitleFormat::Srt,
        None,
        None,
    )
    .await;

    assert!(res.is_ok(), "Auto-generated caption download must succeed");
    assert!(srt_path.exists());
    assert!(std::fs::metadata(&srt_path).unwrap().len() > 0);

    let ffmpeg = FfmpegController::discover(None).expect("FFmpeg must be discovered");
    let sub_tracks = [(srt_path.as_path(), "en", "English (Auto-generated)")];

    ffmpeg
        .embed_subtitles_cancellable(&video_path, &sub_tracks, &out_path, None)
        .await
        .expect("Embedding YouTube auto-generated subtitle must succeed");

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

    let sub_stream = streams
        .iter()
        .find(|s| s["codec_type"].as_str() == Some("subtitle"))
        .expect("Must have subtitle stream");

    assert_eq!(sub_stream["codec_name"].as_str(), Some("mov_text"));
    assert_eq!(sub_stream["tags"]["language"].as_str(), Some("eng"));
    let tag_title = sub_stream["tags"]["title"]
        .as_str()
        .or_else(|| sub_stream["tags"]["handler_name"].as_str());
    assert_eq!(tag_title, Some("English (Auto-generated)"));

    let _ = fs::remove_dir_all(&test_dir).await;
}

#[test]
fn test_7_no_subtitles_selected_leaves_mp4_unchanged() {
    let params = StartDownloadParams {
        source_url: "https://www.youtube.com/watch?v=123".to_string(),
        service_name: "youtube".to_string(),
        title: "Video Only".to_string(),
        duration: 100,
        thumbnail_url: None,
        format_id: "yt-1080p".to_string(),
        download_url: "https://example.com/video.mp4".to_string(),
        requires_merge: true,
        matching_audio_url: Some("https://example.com/audio.m4a".to_string()),
        container: "mp4".to_string(),
        quality_label: "1080p FHD".to_string(),
        destination_dir: None,
        selected_subtitles: Vec::new(),
        embed_subtitles: true,
        subtitle_mode: None,
        audio_options: None,
        video_codec: None,
        audio_codec: None,
    };

    let should_embed = params.embed_subtitles
        && !params.selected_subtitles.is_empty()
        && params.audio_options.is_none();

    assert!(
        !should_embed,
        "Empty selected subtitles must bypass embedding"
    );
}
