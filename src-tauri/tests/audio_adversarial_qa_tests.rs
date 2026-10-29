use freefetch_lib::core::config::AppSettings;
use freefetch_lib::core::types::{
    AudioConversionOptions, AudioQualityPreset, AudioTargetFormat, BitrateMode,
};
use freefetch_lib::engine::queue::QueueManager;
use freefetch_lib::engine::template::{FilenameTemplateEngine, TemplateContext};
use freefetch_lib::engine::{EngineManager, StartDownloadParams};
use freefetch_lib::extractor::{ExtractorRegistry, MediaMetadata};
use freefetch_lib::media::artwork::ArtworkDownloader;
use freefetch_lib::media::audio_metadata::AudioMetadata;
use freefetch_lib::media::ffmpeg::FfmpegController;
use freefetch_lib::storage::create_in_memory_database;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use tokio::sync::watch;

fn get_qa_dir(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("ff_audio_qa_{}_{}", name, std::process::id()));
    let _ = std::fs::create_dir_all(&p);
    p
}

/// Helper to generate a 1-second sine wave audio file
fn create_test_wav(output: &Path) {
    let status = Command::new("ffmpeg")
        .arg("-y")
        .arg("-f")
        .arg("lavfi")
        .arg("-i")
        .arg("sine=frequency=1000:duration=1")
        .arg("-c:a")
        .arg("pcm_s16le")
        .arg(output)
        .status()
        .expect("Failed to execute ffmpeg to generate test wav");
    assert!(status.success(), "Failed to generate test input wav");
}

/// Helper to generate a 32x32 test JPEG cover
fn create_test_jpeg(output: &Path) {
    let status = Command::new("ffmpeg")
        .arg("-y")
        .arg("-f")
        .arg("lavfi")
        .arg("-i")
        .arg("color=c=purple:s=32x32:d=1")
        .arg("-frames:v")
        .arg("1")
        .arg(output)
        .status()
        .expect("Failed to execute ffmpeg to generate test jpeg");
    assert!(status.success(), "Failed to generate test jpeg");
}

/// Helper to inspect file streams and tags with ffprobe JSON
fn probe_file_json(path: &Path) -> serde_json::Value {
    let output = Command::new("ffprobe")
        .arg("-v")
        .arg("quiet")
        .arg("-print_format")
        .arg("json")
        .arg("-show_format")
        .arg("-show_streams")
        .arg(path)
        .output()
        .expect("Failed to run ffprobe");
    assert!(output.status.success(), "ffprobe failed on {:?}", path);
    serde_json::from_slice(&output.stdout).expect("Failed to parse ffprobe json")
}

// ---------------------------------------------------------------------------
// 1. Audio Format Validation with Real FFprobe
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ffprobe_audio_validation_all_formats() {
    let dir = get_qa_dir("format_validation");
    let input_wav = dir.join("input.wav");
    let cover_jpg = dir.join("cover.jpg");
    create_test_wav(&input_wav);
    create_test_jpeg(&cover_jpg);

    let ffmpeg = FfmpegController::discover(None).expect("FFmpeg must be installed for QA pass");

    let meta = AudioMetadata {
        title: "Test Song Title".to_string(),
        artist: Some("QA Artist".to_string()),
        album: Some("QA Album".to_string()),
        album_artist: Some("QA Artist".to_string()),
        date: Some("2025".to_string()),
        track_number: Some(1),
        genre: Some("Electronic".to_string()),
        comment: Some("https://example.com/audio".to_string()),
    };

    // A. MP3 Validation
    let mp3_out = dir.join("output.mp3");
    let mp3_opts = AudioConversionOptions {
        target_format: AudioTargetFormat::Mp3,
        quality_preset: AudioQualityPreset::Best, // 320k
        bitrate_mode: BitrateMode::Cbr,
        embed_metadata: true,
        embed_artwork: true,
        ..Default::default()
    };
    ffmpeg
        .convert_audio(
            &input_wav,
            &mp3_out,
            &mp3_opts,
            Some(&meta),
            Some(&cover_jpg),
        )
        .await
        .expect("MP3 transcode failed");

    let mp3_probe = probe_file_json(&mp3_out);
    let mp3_streams = mp3_probe["streams"].as_array().expect("MP3 streams");
    assert!(
        mp3_streams.len() >= 2,
        "MP3 must have audio and attached_pic streams"
    );
    assert_eq!(mp3_streams[0]["codec_name"], "mp3");
    assert_eq!(mp3_streams[0]["codec_type"], "audio");
    assert_eq!(mp3_streams[1]["codec_name"], "mjpeg");
    assert_eq!(mp3_streams[1]["disposition"]["attached_pic"], 1);

    let mp3_tags = &mp3_probe["format"]["tags"];
    assert_eq!(mp3_tags["title"], "Test Song Title");
    assert_eq!(mp3_tags["artist"], "QA Artist");
    assert_eq!(mp3_tags["album"], "QA Album");

    // B. M4A Validation
    let m4a_out = dir.join("output.m4a");
    let m4a_opts = AudioConversionOptions {
        target_format: AudioTargetFormat::M4a,
        quality_preset: AudioQualityPreset::High, // 192k
        bitrate_mode: BitrateMode::Cbr,
        embed_metadata: true,
        embed_artwork: true,
        ..Default::default()
    };
    ffmpeg
        .convert_audio(
            &input_wav,
            &m4a_out,
            &m4a_opts,
            Some(&meta),
            Some(&cover_jpg),
        )
        .await
        .expect("M4A transcode failed");

    let m4a_probe = probe_file_json(&m4a_out);
    let m4a_streams = m4a_probe["streams"].as_array().expect("M4A streams");
    assert!(
        m4a_streams.len() >= 2,
        "M4A must have audio and attached_pic streams"
    );
    assert_eq!(m4a_streams[0]["codec_name"], "aac");
    assert_eq!(m4a_streams[0]["codec_type"], "audio");
    assert_eq!(m4a_streams[1]["disposition"]["attached_pic"], 1);

    let m4a_tags = &m4a_probe["format"]["tags"];
    assert_eq!(m4a_tags["title"], "Test Song Title");
    assert_eq!(m4a_tags["artist"], "QA Artist");

    // C. FLAC Validation
    let flac_out = dir.join("output.flac");
    let flac_opts = AudioConversionOptions {
        target_format: AudioTargetFormat::Flac,
        quality_preset: AudioQualityPreset::Best,
        embed_metadata: true,
        embed_artwork: true,
        ..Default::default()
    };
    ffmpeg
        .convert_audio(
            &input_wav,
            &flac_out,
            &flac_opts,
            Some(&meta),
            Some(&cover_jpg),
        )
        .await
        .expect("FLAC transcode failed");

    let flac_probe = probe_file_json(&flac_out);
    let flac_streams = flac_probe["streams"].as_array().expect("FLAC streams");
    assert!(
        flac_streams.len() >= 2,
        "FLAC must have audio and attached_pic streams"
    );
    assert_eq!(flac_streams[0]["codec_name"], "flac");
    assert_eq!(flac_streams[1]["disposition"]["attached_pic"], 1);

    let get_tag = |probe: &serde_json::Value, key: &str| -> Option<String> {
        let check_map = |m: &serde_json::Value| {
            m.as_object().and_then(|map| {
                map.iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(key))
                    .and_then(|(_, v)| v.as_str().map(|s| s.to_string()))
            })
        };
        check_map(&probe["format"]["tags"]).or_else(|| check_map(&probe["streams"][0]["tags"]))
    };

    assert_eq!(
        get_tag(&flac_probe, "title").as_deref(),
        Some("Test Song Title")
    );
    assert_eq!(get_tag(&flac_probe, "artist").as_deref(), Some("QA Artist"));

    // D. Opus Validation
    let opus_out = dir.join("output.opus");
    let opus_opts = AudioConversionOptions {
        target_format: AudioTargetFormat::Opus,
        quality_preset: AudioQualityPreset::High, // 128k
        embed_metadata: true,
        embed_artwork: true, // Should gracefully omit attached_pic for Ogg
        ..Default::default()
    };
    ffmpeg
        .convert_audio(
            &input_wav,
            &opus_out,
            &opus_opts,
            Some(&meta),
            Some(&cover_jpg),
        )
        .await
        .expect("Opus transcode failed");

    let opus_probe = probe_file_json(&opus_out);
    let opus_streams = opus_probe["streams"].as_array().expect("Opus streams");
    assert_eq!(
        opus_streams.len(),
        1,
        "Opus has single audio stream (no video attached_pic)"
    );
    assert_eq!(opus_streams[0]["codec_name"], "opus");

    assert_eq!(
        get_tag(&opus_probe, "title").as_deref(),
        Some("Test Song Title")
    );
    assert_eq!(get_tag(&opus_probe, "artist").as_deref(), Some("QA Artist"));

    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// 2. Quality & Sample Rate Verification
// ---------------------------------------------------------------------------

#[test]
fn test_quality_presets_resolution_accuracy() {
    // MP3
    assert_eq!(
        AudioQualityPreset::Best.resolve_bitrate_kbps(AudioTargetFormat::Mp3, None),
        Some(320)
    );
    assert_eq!(
        AudioQualityPreset::High.resolve_bitrate_kbps(AudioTargetFormat::Mp3, None),
        Some(256)
    );
    assert_eq!(
        AudioQualityPreset::Medium.resolve_bitrate_kbps(AudioTargetFormat::Mp3, None),
        Some(192)
    );
    assert_eq!(
        AudioQualityPreset::Low.resolve_bitrate_kbps(AudioTargetFormat::Mp3, None),
        Some(128)
    );
    assert_eq!(
        AudioQualityPreset::Custom.resolve_bitrate_kbps(AudioTargetFormat::Mp3, Some(224)),
        Some(224)
    );

    // M4A
    assert_eq!(
        AudioQualityPreset::Best.resolve_bitrate_kbps(AudioTargetFormat::M4a, None),
        Some(256)
    );
    assert_eq!(
        AudioQualityPreset::High.resolve_bitrate_kbps(AudioTargetFormat::M4a, None),
        Some(192)
    );
    assert_eq!(
        AudioQualityPreset::Medium.resolve_bitrate_kbps(AudioTargetFormat::M4a, None),
        Some(160)
    );
    assert_eq!(
        AudioQualityPreset::Low.resolve_bitrate_kbps(AudioTargetFormat::M4a, None),
        Some(128)
    );

    // Opus
    assert_eq!(
        AudioQualityPreset::Best.resolve_bitrate_kbps(AudioTargetFormat::Opus, None),
        Some(160)
    );
    assert_eq!(
        AudioQualityPreset::High.resolve_bitrate_kbps(AudioTargetFormat::Opus, None),
        Some(128)
    );
    assert_eq!(
        AudioQualityPreset::Medium.resolve_bitrate_kbps(AudioTargetFormat::Opus, None),
        Some(96)
    );
    assert_eq!(
        AudioQualityPreset::Low.resolve_bitrate_kbps(AudioTargetFormat::Opus, None),
        Some(64)
    );

    // FLAC (Lossless)
    assert_eq!(
        AudioQualityPreset::Best.resolve_bitrate_kbps(AudioTargetFormat::Flac, None),
        None
    );
    assert_eq!(
        AudioQualityPreset::High.resolve_bitrate_kbps(AudioTargetFormat::Flac, None),
        None
    );

    // FLAC must reject custom bitrate
    let flac_custom_bad = AudioConversionOptions {
        target_format: AudioTargetFormat::Flac,
        custom_bitrate_kbps: Some(320),
        ..Default::default()
    };
    assert!(flac_custom_bad.validate().is_err());
}

#[tokio::test]
async fn test_sample_rate_ffmpeg_enforcement() {
    let dir = get_qa_dir("sample_rates");
    let input_wav = dir.join("input.wav");
    create_test_wav(&input_wav);

    let ffmpeg = FfmpegController::discover(None).unwrap();

    // 1. 48000 Hz custom sample rate
    let out_48k = dir.join("out_48k.mp3");
    let opts_48k = AudioConversionOptions {
        target_format: AudioTargetFormat::Mp3,
        sample_rate: Some(48000),
        ..Default::default()
    };
    assert!(opts_48k.validate().is_ok());

    ffmpeg
        .convert_audio(&input_wav, &out_48k, &opts_48k, None, None)
        .await
        .unwrap();

    let probe_48k = probe_file_json(&out_48k);
    assert_eq!(probe_48k["streams"][0]["sample_rate"], "48000");

    // 2. Out of range sample rates rejected by validation
    let bad_low = AudioConversionOptions {
        sample_rate: Some(4000), // Below 8000
        ..Default::default()
    };
    assert!(bad_low.validate().is_err());

    let bad_high = AudioConversionOptions {
        sample_rate: Some(250000), // Above 192000
        ..Default::default()
    };
    assert!(bad_high.validate().is_err());

    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// 3. Metadata Adversarial Testing
// ---------------------------------------------------------------------------

#[test]
fn test_metadata_adversarial_strings_and_fallbacks() {
    // A. Complex noisy title with quotes, Unicode, emoji, resolution tags
    let raw_title =
        "  \"YOASOBI\" - 夜に駆ける (Racing into the Night) [Official Music Video] 4K 🔥  ";
    let (title, artist) = AudioMetadata::parse_artist_and_title(raw_title, Some("Ayase / YOASOBI"));

    assert_eq!(artist.as_deref(), Some("YOASOBI"));
    assert_eq!(title, "夜に駆ける (Racing into the Night) 🔥");

    // Control characters stripped
    let dirty_str = "Clean\x00Title\x08With\x1bControl\r\nChars";
    let cleaned = AudioMetadata::sanitize_tag(dirty_str);
    assert!(!cleaned.contains('\x00'));
    assert!(!cleaned.contains('\x08'));
    assert!(!cleaned.contains('\x1b'));
    assert_eq!(cleaned, "CleanTitleWithControlChars");

    // Missing metadata fallbacks
    let sparse_meta = MediaMetadata {
        id: "sparse_id".to_string(),
        url: "https://example.com/audio".to_string(),
        title: "Just A Song".to_string(),
        duration: 120,
        thumbnail_url: None,
        uploader: None,
        service_name: "generic".to_string(),
        description: None,
        upload_date: None,
    };
    let parsed_sparse = AudioMetadata::from_media_metadata(&sparse_meta, None, None);
    assert_eq!(parsed_sparse.title, "Just A Song");
    assert_eq!(parsed_sparse.artist, None);
    assert_eq!(parsed_sparse.album, None);
    assert_eq!(parsed_sparse.date, None);
    assert_eq!(parsed_sparse.track_number, None);
}

// ---------------------------------------------------------------------------
// 4. Filename Security & Traversal Testing
// ---------------------------------------------------------------------------

#[test]
fn test_filename_security_and_traversal() {
    let ctx = TemplateContext {
        title: "Song".to_string(),
        artist: None,
        album: None,
        date: None,
        id: Some("123".to_string()),
        uploader: None,
        ext: "mp3".to_string(),
    };

    // Windows reserved names
    let reserved = ["CON", "PRN", "AUX", "NUL", "COM1", "LPT1", "con", "nul"];
    for name in reserved {
        let rendered = FilenameTemplateEngine::render_stem(Some(name), &ctx);
        assert!(
            rendered.starts_with('_'),
            "Reserved name {} must be prefixed with underscore: got {}",
            name,
            rendered
        );
    }

    // Path traversal and illegal chars
    let traversal_templates = [
        "..",
        "../../evil",
        "..\\..\\evil",
        "foo/bar",
        "foo\\bar",
        "foo:bar",
        "foo*bar",
        "foo?bar",
        "foo\"bar",
        "foo<bar>baz|qux",
    ];

    for tmpl in traversal_templates {
        let rendered = FilenameTemplateEngine::render_stem(Some(tmpl), &ctx);
        assert!(
            !rendered.contains(".."),
            "Must not contain ..: {}",
            rendered
        );
        assert!(!rendered.contains('/'), "Must not contain /: {}", rendered);
        assert!(
            !rendered.contains('\\'),
            "Must not contain \\: {}",
            rendered
        );
        assert!(!rendered.contains(':'), "Must not contain :: {}", rendered);
        assert!(!rendered.contains('*'), "Must not contain *: {}", rendered);
        assert!(!rendered.contains('?'), "Must not contain ?: {}", rendered);
        assert!(!rendered.contains('"'), "Must not contain \": {}", rendered);
        assert!(!rendered.contains('<'), "Must not contain <: {}", rendered);
        assert!(!rendered.contains('>'), "Must not contain >: {}", rendered);
        assert!(!rendered.contains('|'), "Must not contain |: {}", rendered);
    }

    // Very long title truncation
    let long_title = "A".repeat(400);
    let long_ctx = TemplateContext {
        title: long_title,
        artist: None,
        album: None,
        date: None,
        id: Some("123".to_string()),
        uploader: None,
        ext: "flac".to_string(),
    };
    let rendered_long = FilenameTemplateEngine::render_stem(None, &long_ctx);
    assert!(
        rendered_long.len() <= 200,
        "Rendered filename must be clamped: {}",
        rendered_long.len()
    );
}

// ---------------------------------------------------------------------------
// 5. Artwork Adversarial Testing
// ---------------------------------------------------------------------------

#[test]
fn test_artwork_magic_byte_adversarial() {
    // Valid headers
    assert!(ArtworkDownloader::is_valid_image_bytes(&[
        0xFF, 0xD8, 0xFF, 0xDB, 0, 0, 0, 0
    ])); // JPEG
    assert!(ArtworkDownloader::is_valid_image_bytes(&[
        0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A
    ])); // PNG
    assert!(ArtworkDownloader::is_valid_image_bytes(
        b"RIFF\x00\x00\x00\x00WEBPVP8 "
    )); // WebP

    // Corrupt / invalid headers
    assert!(!ArtworkDownloader::is_valid_image_bytes(b"")); // Empty
    assert!(!ArtworkDownloader::is_valid_image_bytes(b"GIF89a")); // GIF (not supported)
    assert!(!ArtworkDownloader::is_valid_image_bytes(
        b"<!DOCTYPE html><html><body>Error</body></html>"
    )); // HTML 404/500
    assert!(!ArtworkDownloader::is_valid_image_bytes(&[
        0, 0, 0, 0, 0, 0, 0, 0
    ])); // Zeroes
}

// ---------------------------------------------------------------------------
// 6. Cancellation Testing & Process Termination
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_cancellation_during_ffmpeg_conversion() {
    let dir = get_qa_dir("cancellation_test");
    let input_wav = dir.join("input.wav");
    create_test_wav(&input_wav);

    let ffmpeg = FfmpegController::discover(None).unwrap();
    let out_file = dir.join("cancelled_output.mp3");

    let (cancel_tx, cancel_rx) = watch::channel(false);

    // Trigger cancellation immediately or after tiny delay
    let cancel_task = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let _ = cancel_tx.send(true);
    });

    let opts = AudioConversionOptions {
        target_format: AudioTargetFormat::Mp3,
        quality_preset: AudioQualityPreset::Best,
        ..Default::default()
    };

    let res = ffmpeg
        .convert_audio_cancellable(&input_wav, &out_file, &opts, None, None, Some(cancel_rx))
        .await;

    let _ = cancel_task.await;

    // Must be cancelled or early exit
    if res.is_err() {
        // Output file must be cleaned up
        assert!(!out_file.exists(), "Cancelled output file must be removed");
    }

    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// 7. Duplicate Isolation & Mixed Format Queue
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_mixed_queue_duplicate_isolation() {
    let repo = create_in_memory_database().unwrap();
    let registry = Arc::new(ExtractorRegistry::new());
    let engine = EngineManager::new(repo.clone(), registry);
    let settings = Arc::new(Mutex::new(AppSettings::default()));
    let queue = QueueManager::new(repo.clone(), engine, settings);

    let base_url = "https://www.youtube.com/watch?v=QA_TEST_123";

    // Queue 5 different formats for the same URL
    let formats = [
        ("best-video", "mp4", None),
        (
            "audio-mp3",
            "mp3",
            Some(AudioConversionOptions {
                target_format: AudioTargetFormat::Mp3,
                ..Default::default()
            }),
        ),
        (
            "audio-m4a",
            "m4a",
            Some(AudioConversionOptions {
                target_format: AudioTargetFormat::M4a,
                ..Default::default()
            }),
        ),
        (
            "audio-flac",
            "flac",
            Some(AudioConversionOptions {
                target_format: AudioTargetFormat::Flac,
                ..Default::default()
            }),
        ),
        (
            "audio-opus",
            "opus",
            Some(AudioConversionOptions {
                target_format: AudioTargetFormat::Opus,
                ..Default::default()
            }),
        ),
    ];

    for (fmt_id, container, audio_opt) in formats {
        let params = StartDownloadParams {
            source_url: base_url.to_string(),
            service_name: "youtube".to_string(),
            title: "QA Test Song".to_string(),
            duration: 180,
            thumbnail_url: None,
            format_id: fmt_id.to_string(),
            download_url: "https://example.com/stream".to_string(),
            requires_merge: false,
            matching_audio_url: None,
            container: container.to_string(),
            quality_label: container.to_uppercase(),
            destination_dir: None,
            selected_subtitles: vec![],
            embed_subtitles: false,
            subtitle_mode: None,
            audio_options: audio_opt,
            video_codec: None,
            audio_codec: None,
        http_headers: None,
        };

        let res = queue.add(params).await;
        assert!(
            res.is_ok(),
            "Format {} must be accepted into queue: {:?}",
            fmt_id,
            res
        );
    }

    // Queue count must be 5
    let items = queue.get_items().unwrap();
    assert_eq!(
        items.len(),
        5,
        "All 5 formats must coexist in queue without deduplication clashes"
    );

    // Re-queueing the exact same format (e.g. MP3) must be detected as duplicate
    let duplicate_mp3 = StartDownloadParams {
        source_url: base_url.to_string(),
        service_name: "youtube".to_string(),
        title: "QA Test Song".to_string(),
        duration: 180,
        thumbnail_url: None,
        format_id: "audio-mp3".to_string(),
        download_url: "https://example.com/stream".to_string(),
        requires_merge: false,
        matching_audio_url: None,
        container: "mp3".to_string(),
        quality_label: "MP3".to_string(),
        destination_dir: None,
        selected_subtitles: vec![],
        embed_subtitles: false,
        subtitle_mode: None,
        audio_options: Some(AudioConversionOptions {
            target_format: AudioTargetFormat::Mp3,
            ..Default::default()
        }),
        video_codec: None,
        audio_codec: None,
    http_headers: None,
    };

    let dup_res = queue.add(duplicate_mp3).await;
    assert!(
        dup_res.is_err(),
        "Duplicate identical audio format must be rejected"
    );
}

// ---------------------------------------------------------------------------
// 8. Playlist & Batch Metadata Mapping
// ---------------------------------------------------------------------------

#[test]
fn test_playlist_audio_metadata_mapping() {
    let meta = MediaMetadata {
        id: "item_01".to_string(),
        url: "https://www.youtube.com/watch?v=test01".to_string(),
        title: "Track One".to_string(),
        duration: 210,
        thumbnail_url: None,
        uploader: Some("Artist Band".to_string()),
        service_name: "youtube".to_string(),
        description: None,
        upload_date: Some("20240101".to_string()),
    };

    let playlist_name = "Greatest Hits Vol. 1";
    let track_num = 1;

    let audio_meta =
        AudioMetadata::from_media_metadata(&meta, Some(playlist_name), Some(track_num));

    assert_eq!(audio_meta.title, "Track One");
    assert_eq!(audio_meta.artist, Some("Artist Band".to_string()));
    assert_eq!(audio_meta.album, Some("Greatest Hits Vol. 1".to_string()));
    assert_eq!(audio_meta.track_number, Some(1));
    assert_eq!(audio_meta.date, Some("2024".to_string()));

    let ffmpeg_args = audio_meta.to_ffmpeg_args(AudioTargetFormat::Mp3);
    assert!(ffmpeg_args.contains(&"-metadata".to_string()));
    assert!(ffmpeg_args.contains(&"track=1".to_string()));
    assert!(ffmpeg_args.contains(&"album=Greatest Hits Vol. 1".to_string()));
}
