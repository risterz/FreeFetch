use freefetch_lib::core::config::AppSettings;
use freefetch_lib::core::types::{
    AudioConversionOptions, AudioQualityPreset, AudioTargetFormat, BitrateMode, QueueStatus,
};
use freefetch_lib::engine::queue::QueueManager;
use freefetch_lib::engine::template::{FilenameTemplateEngine, TemplateContext};
use freefetch_lib::engine::{EngineManager, StartDownloadParams};
use freefetch_lib::extractor::ExtractorRegistry;
use freefetch_lib::media::artwork::ArtworkDownloader;
use freefetch_lib::media::audio_metadata::AudioMetadata;
use freefetch_lib::media::ffmpeg::FfmpegController;
use freefetch_lib::storage::create_in_memory_database;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

fn get_test_dir(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("ff_audio_test_{}_{}", name, std::process::id()));
    let _ = std::fs::create_dir_all(&p);
    p
}

async fn run_mock_server<F>(handler: F) -> (std::net::SocketAddr, oneshot::Sender<()>)
where
    F: Fn(String, usize) -> Option<Vec<u8>> + Send + Sync + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
    let handler = Arc::new(handler);
    let req_counter = Arc::new(AtomicUsize::new(0));

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut shutdown_rx => break,
                accept_res = listener.accept() => {
                    if let Ok((mut socket, _)) = accept_res {
                        let h = handler.clone();
                        let count = req_counter.clone();
                        tokio::spawn(async move {
                            let mut buf = [0u8; 4096];
                            let n = match socket.read(&mut buf).await {
                                Ok(n) if n > 0 => n,
                                _ => return,
                            };
                            let req_str = String::from_utf8_lossy(&buf[..n]).to_string();
                            let req_idx = count.fetch_add(1, Ordering::SeqCst);

                            if let Some(resp_bytes) = h(req_str, req_idx) {
                                let _ = socket.write_all(&resp_bytes).await;
                                let _ = socket.flush().await;
                            }
                        });
                    }
                }
            }
        }
    });

    (addr, shutdown_tx)
}

// ---------------------------------------------------------------------------
// 1. Audio Models Tests
// ---------------------------------------------------------------------------

#[test]
fn test_audio_models_roundtrip_and_defaults() {
    let default_opts = AudioConversionOptions::default();
    assert_eq!(default_opts.target_format, AudioTargetFormat::Mp3);
    assert_eq!(default_opts.quality_preset, AudioQualityPreset::Best);
    assert!(default_opts.embed_metadata);
    assert!(default_opts.embed_artwork);

    let serialized = serde_json::to_string(&default_opts).unwrap();
    let deserialized: AudioConversionOptions = serde_json::from_str(&serialized).unwrap();
    assert_eq!(default_opts, deserialized);
}

#[test]
fn test_audio_models_invalid_combinations_rejected() {
    let flac_with_bitrate = AudioConversionOptions {
        target_format: AudioTargetFormat::Flac,
        custom_bitrate_kbps: Some(320),
        ..AudioConversionOptions::default()
    };
    assert!(flac_with_bitrate.validate().is_err());

    let crazy_sample_rate = AudioConversionOptions {
        sample_rate: Some(999999),
        ..AudioConversionOptions::default()
    };
    assert!(crazy_sample_rate.validate().is_err());
}

// ---------------------------------------------------------------------------
// 2. Filename Template Engine Tests
// ---------------------------------------------------------------------------

#[test]
fn test_filename_template_tokens_and_fallbacks() {
    let ctx = TemplateContext {
        title: "Song Title".to_string(),
        artist: Some("The Artist".to_string()),
        album: Some("Greatest Hits".to_string()),
        date: Some("2023".to_string()),
        id: Some("XYZ12345678".to_string()),
        uploader: Some("OfficialChannel".to_string()),
        ext: "mp3".to_string(),
    };

    let stem = FilenameTemplateEngine::render_stem(Some("{artist} - {title}"), &ctx);
    assert_eq!(stem, "The Artist - Song Title");

    let stem_album = FilenameTemplateEngine::render_stem(Some("{album}/{title}"), &ctx);
    // Traversal '/' replaced with '_'
    assert_eq!(stem_album, "Greatest Hits_Song Title");
}

#[test]
fn test_filename_template_reserved_windows_names() {
    let ctx = TemplateContext {
        title: "NUL".to_string(),
        artist: None,
        ext: "mp3".to_string(),
        ..TemplateContext::default()
    };

    let stem = FilenameTemplateEngine::render_stem(Some("{title}"), &ctx);
    assert_eq!(stem, "_NUL");
}

// ---------------------------------------------------------------------------
// 3. Audio Metadata Parser Tests
// ---------------------------------------------------------------------------

#[test]
fn test_audio_metadata_heuristic_and_noise_removal() {
    let (title, artist) = AudioMetadata::parse_artist_and_title(
        "Michael Jackson - Billie Jean [Official Music Video] HD",
        Some("MJVEVO"),
    );
    assert_eq!(title, "Billie Jean");
    assert_eq!(artist.as_deref(), Some("Michael Jackson"));

    let (title_fallback, artist_fallback) = AudioMetadata::parse_artist_and_title(
        "Standalone Instrumental Track (Audio)",
        Some("ComposerName"),
    );
    assert_eq!(title_fallback, "Standalone Instrumental Track");
    assert_eq!(artist_fallback.as_deref(), Some("ComposerName"));
}

// ---------------------------------------------------------------------------
// 4. Artwork Downloader Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_artwork_magic_bytes_and_isolation() {
    let (addr, shutdown) = run_mock_server(|req, _idx| {
        if req.contains("valid.jpg") {
            let body = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46];
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let mut v = resp.into_bytes();
            v.extend_from_slice(&body);
            Some(v)
        } else if req.contains("corrupt.jpg") {
            let body = b"not an image file at all";
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let mut v = resp.into_bytes();
            v.extend_from_slice(body);
            Some(v)
        } else {
            Some(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec())
        }
    })
    .await;

    let client = reqwest::Client::new();
    let tmp = get_test_dir("artwork_suite");

    // 1. Valid image
    let valid_url = format!("http://127.0.0.1:{}/valid.jpg", addr.port());
    let res_valid =
        ArtworkDownloader::download_cover_art(&client, &valid_url, &tmp, "job_valid").await;
    assert!(res_valid.is_some());
    let path = res_valid.unwrap();
    assert!(path.exists());
    ArtworkDownloader::cleanup_cover_art(&path).await;
    assert!(!path.exists());

    // 2. Corrupt image (fails magic bytes cleanly without panic)
    let corrupt_url = format!("http://127.0.0.1:{}/corrupt.jpg", addr.port());
    let res_corrupt =
        ArtworkDownloader::download_cover_art(&client, &corrupt_url, &tmp, "job_corrupt").await;
    assert!(res_corrupt.is_none());

    // 3. 404 Not Found
    let missing_url = format!("http://127.0.0.1:{}/missing.jpg", addr.port());
    let res_missing =
        ArtworkDownloader::download_cover_art(&client, &missing_url, &tmp, "job_missing").await;
    assert!(res_missing.is_none());

    let _ = shutdown.send(());
    let _ = std::fs::remove_dir_all(&tmp);
}

// ---------------------------------------------------------------------------
// 5. FFmpeg Audio Transcoding Tests (MP3, M4A, FLAC, Opus)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ffmpeg_audio_transcoding_all_formats() {
    let controller = match FfmpegController::discover(None) {
        Ok(c) => c,
        Err(_) => return, // Skip if ffmpeg not installed on host
    };

    let tmp = get_test_dir("ffmpeg_audio_suite");
    let input_wav = tmp.join("input.wav");
    let cover_jpg = tmp.join("cover.jpg");

    // Generate 1-second sine wave input
    let status_wav = std::process::Command::new(controller.binary_path())
        .arg("-y")
        .arg("-f")
        .arg("lavfi")
        .arg("-i")
        .arg("sine=frequency=1000:duration=1")
        .arg(&input_wav)
        .status();
    if status_wav.is_err() || !status_wav.unwrap().success() {
        return;
    }

    // Generate test cover JPEG image
    let _ = std::process::Command::new(controller.binary_path())
        .arg("-y")
        .arg("-f")
        .arg("lavfi")
        .arg("-i")
        .arg("color=c=blue:s=32x32:d=1")
        .arg("-vframes")
        .arg("1")
        .arg(&cover_jpg)
        .status();

    let meta = AudioMetadata {
        title: "Test Track".to_string(),
        artist: Some("Test Artist".to_string()),
        album: Some("Test Album".to_string()),
        album_artist: Some("Test Artist".to_string()),
        date: Some("2024".to_string()),
        genre: Some("Electronic".to_string()),
        track_number: Some(1),
        comment: Some("FreeFetch Test".to_string()),
    };

    // 1. MP3 conversion
    let mp3_out = tmp.join("output.mp3");
    let mp3_opts = AudioConversionOptions {
        target_format: AudioTargetFormat::Mp3,
        quality_preset: AudioQualityPreset::Best,
        bitrate_mode: BitrateMode::Cbr,
        custom_bitrate_kbps: None,
        sample_rate: None,
        embed_metadata: true,
        embed_artwork: true,
        filename_template: None,
    };
    let mp3_res = controller
        .convert_audio(
            &input_wav,
            &mp3_out,
            &mp3_opts,
            Some(&meta),
            Some(&cover_jpg),
        )
        .await;
    assert!(
        mp3_res.is_ok(),
        "MP3 conversion must succeed: {:?}",
        mp3_res
    );
    assert!(mp3_out.exists() && std::fs::metadata(&mp3_out).unwrap().len() > 0);

    // 2. M4A conversion
    let m4a_out = tmp.join("output.m4a");
    let m4a_opts = AudioConversionOptions {
        target_format: AudioTargetFormat::M4a,
        quality_preset: AudioQualityPreset::High,
        bitrate_mode: BitrateMode::Cbr,
        custom_bitrate_kbps: None,
        sample_rate: None,
        embed_metadata: true,
        embed_artwork: true,
        filename_template: None,
    };
    let m4a_res = controller
        .convert_audio(
            &input_wav,
            &m4a_out,
            &m4a_opts,
            Some(&meta),
            Some(&cover_jpg),
        )
        .await;
    assert!(
        m4a_res.is_ok(),
        "M4A conversion must succeed: {:?}",
        m4a_res
    );
    assert!(m4a_out.exists() && std::fs::metadata(&m4a_out).unwrap().len() > 0);

    // 3. FLAC conversion
    let flac_out = tmp.join("output.flac");
    let flac_opts = AudioConversionOptions {
        target_format: AudioTargetFormat::Flac,
        quality_preset: AudioQualityPreset::Best,
        bitrate_mode: BitrateMode::Cbr,
        custom_bitrate_kbps: None,
        sample_rate: None,
        embed_metadata: true,
        embed_artwork: true,
        filename_template: None,
    };
    let flac_res = controller
        .convert_audio(
            &input_wav,
            &flac_out,
            &flac_opts,
            Some(&meta),
            Some(&cover_jpg),
        )
        .await;
    assert!(
        flac_res.is_ok(),
        "FLAC conversion must succeed: {:?}",
        flac_res
    );
    assert!(flac_out.exists() && std::fs::metadata(&flac_out).unwrap().len() > 0);

    // 4. Opus conversion
    let opus_out = tmp.join("output.opus");
    let opus_opts = AudioConversionOptions {
        target_format: AudioTargetFormat::Opus,
        quality_preset: AudioQualityPreset::Best,
        bitrate_mode: BitrateMode::Cbr,
        custom_bitrate_kbps: None,
        sample_rate: None,
        embed_metadata: true,
        embed_artwork: false,
        filename_template: None,
    };
    let opus_res = controller
        .convert_audio(&input_wav, &opus_out, &opus_opts, Some(&meta), None)
        .await;
    assert!(
        opus_res.is_ok(),
        "Opus conversion must succeed: {:?}",
        opus_res
    );
    assert!(opus_out.exists() && std::fs::metadata(&opus_out).unwrap().len() > 0);

    let _ = std::fs::remove_dir_all(&tmp);
}

// ---------------------------------------------------------------------------
// 6. Queue Lifecycle: Queued -> Downloading -> Processing -> Completed
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_audio_queue_full_lifecycle_and_conversion() {
    let controller = match FfmpegController::discover(None) {
        Ok(c) => c,
        Err(_) => return,
    };

    let tmp = get_test_dir("audio_queue_lifecycle");
    let audio_wav = tmp.join("audio_source.wav");

    // Generate valid 1s sine wave WAV file
    let _ = std::process::Command::new(controller.binary_path())
        .arg("-y")
        .arg("-f")
        .arg("lavfi")
        .arg("-i")
        .arg("sine=frequency=440:duration=1")
        .arg(&audio_wav)
        .status();

    let wav_bytes = std::fs::read(&audio_wav).unwrap();

    let (addr, shutdown) = run_mock_server(move |req, _idx| {
        if req.contains("GET /stream.wav") {
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: audio/wav\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                wav_bytes.len()
            );
            let mut v = resp.into_bytes();
            v.extend_from_slice(&wav_bytes);
            Some(v)
        } else {
            None
        }
    })
    .await;

    let repo = create_in_memory_database().unwrap();
    let registry = Arc::new(ExtractorRegistry::new());
    let engine = EngineManager::new(repo.clone(), registry);
    let settings = Arc::new(Mutex::new(AppSettings {
        max_concurrent_downloads: 1,
        download_directory: tmp.to_string_lossy().to_string(),
        ffmpeg_path: Some(controller.binary_path().to_string_lossy().to_string()),
        ..AppSettings::default()
    }));
    let queue = QueueManager::new(repo.clone(), engine, settings);

    let params = StartDownloadParams {
        source_url: format!("http://127.0.0.1:{}/stream.wav", addr.port()),
        service_name: "generic".to_string(),
        title: "Coldplay - Fix You".to_string(),
        duration: 60,
        thumbnail_url: None,
        format_id: "raw-audio".to_string(),
        download_url: format!("http://127.0.0.1:{}/stream.wav", addr.port()),
        requires_merge: false,
        matching_audio_url: None,
        container: "mp3".to_string(),
        quality_label: "320 kbps".to_string(),
        destination_dir: Some(tmp.to_string_lossy().to_string()),
        selected_subtitles: Vec::new(),
        embed_subtitles: false,
        subtitle_mode: None,
        audio_options: Some(AudioConversionOptions {
            target_format: AudioTargetFormat::Mp3,
            quality_preset: AudioQualityPreset::Best,
            bitrate_mode: BitrateMode::Cbr,
            custom_bitrate_kbps: None,
            sample_rate: None,
            embed_metadata: true,
            embed_artwork: false,
            filename_template: Some("{artist} - {title}.{ext}".to_string()),
        }),
        video_codec: None,
        audio_codec: None,
        http_headers: None,
    };

    let item = queue.add(params).await.unwrap();

    // Allow background worker time to download and convert
    tokio::time::sleep(Duration::from_millis(1500)).await;

    let item_after = repo.get_queue_item(&item.id).unwrap().unwrap();
    assert_eq!(
        item_after.status,
        QueueStatus::Completed,
        "Audio download + conversion must complete successfully, got status {:?}",
        item_after.status
    );

    // Verify converted MP3 output exists with template filename
    let expected_file = tmp.join("Coldplay - Fix You.mp3");
    assert!(
        expected_file.exists(),
        "Target MP3 file must exist on disk with template filename"
    );
    assert!(std::fs::metadata(&expected_file).unwrap().len() > 0);

    let _ = shutdown.send(());
    let _ = std::fs::remove_dir_all(&tmp);
}

// ---------------------------------------------------------------------------
// 7. Duplicate Isolation: MP4 Video vs MP3 Audio of Same Source Allowed
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_audio_and_video_duplicate_isolation() {
    let repo = create_in_memory_database().unwrap();
    let registry = Arc::new(ExtractorRegistry::new());
    let engine = EngineManager::new(repo.clone(), registry);
    let settings = Arc::new(Mutex::new(AppSettings::default()));
    let queue = QueueManager::new(repo.clone(), engine, settings);

    let video_params = StartDownloadParams {
        source_url: "https://example.com/video123".to_string(),
        service_name: "generic".to_string(),
        title: "Test Video".to_string(),
        duration: 100,
        thumbnail_url: None,
        format_id: "fmt-1080p".to_string(),
        download_url: "https://example.com/video.mp4".to_string(),
        requires_merge: false,
        matching_audio_url: None,
        container: "mp4".to_string(),
        quality_label: "1080p".to_string(),
        destination_dir: None,
        selected_subtitles: Vec::new(),
        embed_subtitles: false,
        subtitle_mode: None,
        audio_options: None,
        video_codec: None,
        audio_codec: None,
        http_headers: None,
    };

    let audio_params = StartDownloadParams {
        source_url: "https://example.com/video123".to_string(),
        service_name: "generic".to_string(),
        title: "Test Video".to_string(),
        duration: 100,
        thumbnail_url: None,
        format_id: "fmt-1080p".to_string(),
        download_url: "https://example.com/audio.m4a".to_string(),
        requires_merge: false,
        matching_audio_url: None,
        container: "mp3".to_string(),
        quality_label: "320 kbps".to_string(),
        destination_dir: None,
        selected_subtitles: Vec::new(),
        embed_subtitles: false,
        subtitle_mode: None,
        audio_options: Some(AudioConversionOptions {
            target_format: AudioTargetFormat::Mp3,
            ..AudioConversionOptions::default()
        }),
        video_codec: None,
        audio_codec: None,
        http_headers: None,
    };

    // Queuing video first succeeds
    let v_item = queue.add(video_params).await;
    assert!(v_item.is_ok(), "Video queue item must succeed");

    // Queuing audio of the exact same source URL MUST also succeed (not blocked as duplicate)
    let a_item = queue.add(audio_params).await;
    assert!(
        a_item.is_ok(),
        "Audio item for the same source must be allowed: {:?}",
        a_item.err()
    );
}
