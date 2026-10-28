use freefetch_lib::core::error::FreeFetchError;
use freefetch_lib::extractor::{ExtractionContext, ExtractorRegistry};
use freefetch_lib::media::container::{
    is_audio_codec_compatible, is_subtitle_codec_compatible, is_video_codec_compatible,
    normalize_audio_codec, normalize_container, normalize_video_codec, validate_stream_combination,
};
use url::Url;

#[test]
fn test_video_codec_normalization() {
    assert_eq!(normalize_video_codec("vp09.00.41.08"), "vp9");
    assert_eq!(normalize_video_codec("vp9"), "vp9");
    assert_eq!(normalize_video_codec("vp08.00.41.08"), "vp8");
    assert_eq!(normalize_video_codec("vp8"), "vp8");
    assert_eq!(normalize_video_codec("av01.0.08M.08"), "av1");
    assert_eq!(normalize_video_codec("av1"), "av1");
    assert_eq!(normalize_video_codec("avc1.640028"), "h264");
    assert_eq!(normalize_video_codec("h264"), "h264");
    assert_eq!(normalize_video_codec("hev1.1.6.L93.B0"), "hevc");
    assert_eq!(normalize_video_codec("hvc1.1.6.L93.B0"), "hevc");
    assert_eq!(normalize_video_codec("h265"), "hevc");
    assert_eq!(normalize_video_codec("none"), "none");
    assert_eq!(normalize_video_codec(""), "none");
}

#[test]
fn test_audio_codec_normalization() {
    assert_eq!(normalize_audio_codec("mp4a.40.2"), "aac");
    assert_eq!(normalize_audio_codec("aac"), "aac");
    assert_eq!(normalize_audio_codec("opus"), "opus");
    assert_eq!(normalize_audio_codec("vorbis"), "vorbis");
    assert_eq!(normalize_audio_codec("mp3"), "mp3");
    assert_eq!(normalize_audio_codec("flac"), "flac");
    assert_eq!(normalize_audio_codec("alac"), "alac");
    assert_eq!(normalize_audio_codec("ac3"), "ac3");
    assert_eq!(normalize_audio_codec("eac3"), "eac3");
    assert_eq!(normalize_audio_codec("none"), "none");
}

#[test]
fn test_container_normalization() {
    assert_eq!(normalize_container(".webm"), "webm");
    assert_eq!(normalize_container("WEBM"), "webm");
    assert_eq!(normalize_container(".mp4"), "mp4");
    assert_eq!(normalize_container("mkv"), "mkv");
}

#[test]
fn test_webm_strict_codec_matrix() {
    // Valid WebM video
    assert!(is_video_codec_compatible("webm", "vp9"));
    assert!(is_video_codec_compatible("webm", "vp09.00.41.08"));
    assert!(is_video_codec_compatible("webm", "vp8"));
    assert!(is_video_codec_compatible("webm", "av1"));
    assert!(is_video_codec_compatible("webm", "av01.0.08M.08"));

    // Invalid WebM video
    assert!(!is_video_codec_compatible("webm", "avc1.640028"));
    assert!(!is_video_codec_compatible("webm", "h264"));
    assert!(!is_video_codec_compatible("webm", "hevc"));
    assert!(!is_video_codec_compatible("webm", "h265"));

    // Valid WebM audio
    assert!(is_audio_codec_compatible("webm", "opus"));
    assert!(is_audio_codec_compatible("webm", "vorbis"));

    // Invalid WebM audio (The exact reported production defect!)
    assert!(!is_audio_codec_compatible("webm", "mp4a.40.2"));
    assert!(!is_audio_codec_compatible("webm", "aac"));
    assert!(!is_audio_codec_compatible("webm", "mp3"));
    assert!(!is_audio_codec_compatible("webm", "flac"));
    assert!(!is_audio_codec_compatible("webm", "alac"));
}

#[test]
fn test_mp4_and_mkv_codec_matrix() {
    // MP4
    assert!(is_video_codec_compatible("mp4", "h264"));
    assert!(is_video_codec_compatible("mp4", "hevc"));
    assert!(is_video_codec_compatible("mp4", "av1"));
    assert!(is_audio_codec_compatible("mp4", "aac"));
    assert!(is_audio_codec_compatible("mp4", "mp3"));
    assert!(is_audio_codec_compatible("mp4", "opus"));

    // MKV accepts all
    assert!(is_video_codec_compatible("mkv", "vp9"));
    assert!(is_video_codec_compatible("mkv", "h264"));
    assert!(is_video_codec_compatible("mkv", "hevc"));
    assert!(is_audio_codec_compatible("mkv", "opus"));
    assert!(is_audio_codec_compatible("mkv", "vorbis"));
    assert!(is_audio_codec_compatible("mkv", "aac"));
    assert!(is_audio_codec_compatible("mkv", "flac"));
}

#[test]
fn test_subtitle_embedding_compatibility() {
    // WebM only supports WebVTT for embedded subtitle tracks
    assert!(is_subtitle_codec_compatible("webm", "embed", Some("vtt")));
    assert!(is_subtitle_codec_compatible(
        "webm",
        "embed",
        Some("webvtt")
    ));
    assert!(!is_subtitle_codec_compatible("webm", "embed", Some("srt")));
    assert!(!is_subtitle_codec_compatible("webm", "embed", Some("ass")));

    // When mode is none or hardcode, container embedding rule doesn't restrict
    assert!(is_subtitle_codec_compatible("webm", "none", Some("srt")));
    assert!(is_subtitle_codec_compatible(
        "webm",
        "hardcode",
        Some("srt")
    ));

    // MP4 and MKV support embedding
    assert!(is_subtitle_codec_compatible("mp4", "embed", Some("srt")));
    assert!(is_subtitle_codec_compatible("mp4", "embed", Some("vtt")));
    assert!(is_subtitle_codec_compatible("mkv", "embed", Some("srt")));
    assert!(is_subtitle_codec_compatible("mkv", "embed", Some("vtt")));
}

#[test]
fn test_validate_stream_combination_rejects_webm_aac() {
    // The exact bug report case: VP9 + AAC in WebM container
    let res = validate_stream_combination("webm", Some("vp9"), Some("aac"), None, None);
    assert!(res.is_err());
    match res.unwrap_err() {
        FreeFetchError::IncompatibleContainerCodec { container, message } => {
            assert_eq!(container, "webm");
            assert!(
                message.contains("Opus or Vorbis"),
                "Expected message to mention Opus/Vorbis, got: {message}"
            );
        }
        other => panic!(
            "Expected IncompatibleContainerCodec error, got: {:?}",
            other
        ),
    }

    // Also with yt-dlp codec designations
    let res2 =
        validate_stream_combination("webm", Some("vp09.00.41.08"), Some("mp4a.40.2"), None, None);
    assert!(res2.is_err());
}

#[test]
fn test_validate_stream_combination_accepts_valid_pairs() {
    // WebM + VP9 + Opus
    assert!(validate_stream_combination("webm", Some("vp9"), Some("opus"), None, None).is_ok());

    // WebM + AV1 + Opus
    assert!(validate_stream_combination("webm", Some("av1"), Some("opus"), None, None).is_ok());

    // WebM + VP8 + Vorbis
    assert!(validate_stream_combination("webm", Some("vp8"), Some("vorbis"), None, None).is_ok());

    // MP4 + H264 + AAC
    assert!(validate_stream_combination("mp4", Some("h264"), Some("aac"), None, None).is_ok());

    // MKV + VP9 + AAC
    assert!(validate_stream_combination("mkv", Some("vp9"), Some("aac"), None, None).is_ok());
}

#[tokio::test]
async fn test_real_world_youtube_webm_pairs_with_opus() {
    // Target video from production defect report: "10 Jajan Anak SD Paling Menjijikan di Indonesia !"
    let url = Url::parse("https://www.youtube.com/watch?v=kTn-k0aM-4I").unwrap();
    let reg = ExtractorRegistry::new();
    let extractor = reg.find_extractor(&url).expect("YouTube extractor found");

    let ctx = ExtractionContext::default();
    let (_meta, manifest) = extractor
        .extract(&url, &ctx)
        .await
        .expect("Extraction succeeded");

    assert!(!manifest.formats.is_empty(), "Manifest formats not empty");

    // Inspect all WebM formats that require merging
    let webm_merge_formats: Vec<_> = manifest
        .formats
        .iter()
        .filter(|f| f.container == "webm" && f.requires_merge)
        .collect();

    assert!(
        !webm_merge_formats.is_empty(),
        "Video has WebM adaptive formats requiring merge"
    );

    for fmt in webm_merge_formats {
        // 1. matching_audio_url must be present
        assert!(
            fmt.matching_audio_url.is_some(),
            "WebM format {} must have matching_audio_url",
            fmt.format_id
        );

        // 2. audio_codec must be Opus (never AAC)
        let acodec = fmt.audio_codec.as_deref().unwrap_or("");
        assert!(
            acodec.contains("opus") || acodec == "opus",
            "WebM format {} paired audio_codec must be opus, got '{}'",
            fmt.format_id,
            acodec
        );

        // 3. validate_stream_combination must succeed
        let val_res = validate_stream_combination(
            &fmt.container,
            fmt.video_codec.as_deref(),
            fmt.audio_codec.as_deref(),
            None,
            None,
        );
        assert!(
            val_res.is_ok(),
            "WebM format {} combination must be valid: {:?}",
            fmt.format_id,
            val_res.err()
        );
    }

    // Inspect MP4 formats requiring merge: must pair with AAC
    let mp4_merge_formats: Vec<_> = manifest
        .formats
        .iter()
        .filter(|f| f.container == "mp4" && f.requires_merge)
        .collect();

    for fmt in mp4_merge_formats {
        let acodec = fmt.audio_codec.as_deref().unwrap_or("");
        assert!(
            acodec.contains("mp4a") || acodec.contains("aac"),
            "MP4 format {} paired audio_codec must be AAC/mp4a, got '{}'",
            fmt.format_id,
            acodec
        );
    }
}

#[tokio::test]
async fn test_ffmpeg_merge_webm_vp9_and_opus_succeeds() {
    use freefetch_lib::media::ffmpeg::FfmpegController;
    use freefetch_lib::media::probe::ProbeController;

    let ffmpeg = match FfmpegController::discover(None) {
        Ok(f) => f,
        Err(_) => return, // Skip test if ffmpeg not installed
    };
    let probe = match ProbeController::discover(None) {
        Ok(p) => p,
        Err(_) => return,
    };

    let tmp = std::env::temp_dir().join(format!("ff_test_webm_{}", uuid::Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&tmp);

    let v_path = tmp.join("video_vp9.webm");
    let a_path = tmp.join("audio_opus.webm");
    let out_path = tmp.join("merged_output.webm");

    // Generate 1-second VP9 video stream
    let status_v = std::process::Command::new(ffmpeg.binary_path())
        .arg("-y")
        .arg("-f")
        .arg("lavfi")
        .arg("-i")
        .arg("testsrc=duration=1:size=320x240:rate=25")
        .arg("-c:v")
        .arg("libvpx-vp9")
        .arg(&v_path)
        .status()
        .unwrap();
    assert!(status_v.success());

    // Generate 1-second Opus audio stream
    let status_a = std::process::Command::new(ffmpeg.binary_path())
        .arg("-y")
        .arg("-f")
        .arg("lavfi")
        .arg("-i")
        .arg("sine=frequency=1000:duration=1")
        .arg("-c:a")
        .arg("libopus")
        .arg(&a_path)
        .status()
        .unwrap();
    assert!(status_a.success());

    // Merge VP9 + Opus into WebM container using FreeFetch engine controller
    let merge_res = ffmpeg
        .merge_streams_cancellable(&v_path, &a_path, &out_path, None)
        .await;
    assert!(
        merge_res.is_ok(),
        "FFmpeg merge VP9 + Opus into WebM must succeed: {:?}",
        merge_res.err()
    );
    assert!(out_path.exists(), "Merged WebM file must exist on disk");

    // Probe resulting file
    let probed_v = probe.probe_video_codec(&out_path).await.unwrap();
    assert_eq!(probed_v, "vp9");
    let probed_a = probe.probe_audio_codec(&out_path).await.unwrap();
    assert_eq!(probed_a, "opus");

    let _ = std::fs::remove_dir_all(&tmp);
}

#[tokio::test]
async fn test_ffmpeg_merge_webm_vp9_and_aac_blocked_with_clean_error() {
    use freefetch_lib::media::ffmpeg::FfmpegController;

    let ffmpeg = match FfmpegController::discover(None) {
        Ok(f) => f,
        Err(_) => return, // Skip test if ffmpeg not installed
    };

    let tmp = std::env::temp_dir().join(format!("ff_test_webm_aac_{}", uuid::Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&tmp);

    let v_path = tmp.join("video_vp9.webm");
    let a_path = tmp.join("audio_aac.m4a");
    let out_path = tmp.join("invalid_output.webm");

    // Generate 1-second VP9 video stream
    let _ = std::process::Command::new(ffmpeg.binary_path())
        .arg("-y")
        .arg("-f")
        .arg("lavfi")
        .arg("-i")
        .arg("testsrc=duration=1:size=320x240:rate=25")
        .arg("-c:v")
        .arg("libvpx-vp9")
        .arg(&v_path)
        .status();

    // Generate 1-second AAC audio stream
    let _ = std::process::Command::new(ffmpeg.binary_path())
        .arg("-y")
        .arg("-f")
        .arg("lavfi")
        .arg("-i")
        .arg("sine=frequency=1000:duration=1")
        .arg("-c:a")
        .arg("aac")
        .arg(&a_path)
        .status();

    // Attempting to merge VP9 + AAC into WebM container MUST be blocked with IncompatibleContainerCodec
    let merge_res = ffmpeg
        .merge_streams_cancellable(&v_path, &a_path, &out_path, None)
        .await;

    assert!(
        merge_res.is_err(),
        "FFmpeg merge VP9 + AAC into WebM container must fail"
    );
    match merge_res.unwrap_err() {
        FreeFetchError::IncompatibleContainerCodec { container, message } => {
            assert_eq!(container, "webm");
            assert!(
                message.contains("Opus or Vorbis") || message.contains("Opus/Vorbis"),
                "Error message must specify audio incompatibility: {message}"
            );
        }
        other => panic!(
            "Expected IncompatibleContainerCodec, got other error: {:?}",
            other
        ),
    }

    let _ = std::fs::remove_dir_all(&tmp);
}
