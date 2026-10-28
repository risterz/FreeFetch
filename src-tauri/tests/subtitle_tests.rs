use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use url::Url;
use uuid::Uuid;

use freefetch_lib::core::config::AppSettings;
use freefetch_lib::core::types::{QueueStatus, SubtitleFormat, SubtitleSelection, SubtitleTrack};
use freefetch_lib::engine::queue::QueueManager;
use freefetch_lib::engine::{EngineManager, StartDownloadParams};
use freefetch_lib::extractor::{ExtractionContext, ExtractorRegistry};
use freefetch_lib::media::subtitle_converter::vtt_to_srt;
use freefetch_lib::media::subtitle_parser::{
    format_timestamp_srt, format_timestamp_vtt, parse_timestamp_ms, parse_vtt, strip_vtt_tags,
};
use freefetch_lib::storage::create_in_memory_database;

fn get_test_dir(prefix: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("{}_{}", prefix, Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

#[test]
fn test_parse_timestamp_ms_and_formatting() {
    assert_eq!(parse_timestamp_ms("00:00:01.500"), Some(1500));
    assert_eq!(parse_timestamp_ms("01:23.456"), Some(83456));
    assert_eq!(parse_timestamp_ms("01:02:03.456"), Some(3723456));

    assert_eq!(format_timestamp_vtt(1500), "00:00:01.500");
    assert_eq!(format_timestamp_srt(1500), "00:00:01,500");
}

#[test]
fn test_strip_vtt_styling_tags() {
    let input = "<v Narrator>Hello <b>World</b> &amp; <c.yellow>Friends</c>!</v>";
    let stripped = strip_vtt_tags(input);
    assert_eq!(stripped, "Hello World & Friends!");
}

#[test]
fn test_vtt_bom_and_crlf_parsing() {
    let raw = "\u{feff}WEBVTT\r\n\r\n1\r\n00:00:01.000 --> 00:00:03.000\r\nLine 1\r\n\r\n2\r\n00:00:04.000 --> 00:00:06.000\r\nLine 2\r\n";
    let doc = parse_vtt(raw);
    assert_eq!(doc.cues.len(), 2);
    assert_eq!(doc.cues[0].text, "Line 1");
    assert_eq!(doc.cues[1].text, "Line 2");
}

#[test]
fn test_vtt_multiline_cues_and_srt_conversion() {
    let vtt = "WEBVTT\n\n00:00:01.000 --> 00:00:04.000\nFirst line of dialogue.\nSecond line of dialogue.\n\n00:00:05.500 --> 00:00:08.000\nThird line.\n";
    let srt = vtt_to_srt(vtt);

    let expected = "1\n00:00:01,000 --> 00:00:04,000\nFirst line of dialogue.\nSecond line of dialogue.\n\n2\n00:00:05,500 --> 00:00:08,000\nThird line.\n\n";
    assert_eq!(srt, expected);
}

#[test]
fn test_malformed_vtt_resilience() {
    let vtt = "WEBVTT\n\nNOTE commentary block\n\nSTYLE\n::cue { color: red; }\n\ncorrupt header\nHello\n\n00:00:10.000 --> 00:00:12.000\nValid cue\n";
    let doc = parse_vtt(vtt);
    assert_eq!(doc.cues.len(), 1);
    assert_eq!(doc.cues[0].text, "Valid cue");
    assert_eq!(doc.cues[0].start_ms, 10000);
}

#[test]
fn test_subtitle_filename_sanitization() {
    let tmp_dir = get_test_dir("ff_sub_safe");
    let safe_path = freefetch_lib::engine::state::ensure_safe_destination_path(
        &tmp_dir,
        "../../My Video.en",
        "srt",
    );

    assert_eq!(safe_path.extension().unwrap(), "srt");
    assert!(
        safe_path.starts_with(&tmp_dir),
        "Destination must remain inside designated download directory"
    );
    let fname = safe_path.file_name().unwrap().to_str().unwrap();
    assert!(!fname.contains(".."));
    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[tokio::test]
async fn test_subtitle_queue_format_and_deduplication() {
    let repo = create_in_memory_database().unwrap();
    let registry = Arc::new(ExtractorRegistry::new());
    let engine = EngineManager::new(repo.clone(), registry);
    let settings = Arc::new(Mutex::new(AppSettings::default()));
    let queue = QueueManager::new(repo.clone(), engine, settings);

    let media_params = StartDownloadParams {
        source_url: "https://www.youtube.com/watch?v=TEST_SUB_123".to_string(),
        service_name: "youtube".to_string(),
        title: "Test Video".to_string(),
        duration: 120,
        thumbnail_url: None,
        format_id: "yt-1080p".to_string(),
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
    };

    let subs = vec![
        SubtitleSelection {
            language_code: "en".to_string(),
            language_name: "English".to_string(),
            target_format: SubtitleFormat::Srt,
            is_auto_generated: false,
            download_url: Some("https://example.com/timedtext_en".to_string()),
        },
        SubtitleSelection {
            language_code: "de".to_string(),
            language_name: "German".to_string(),
            target_format: SubtitleFormat::Vtt,
            is_auto_generated: false,
            download_url: Some("https://example.com/timedtext_de".to_string()),
        },
    ];

    let items = queue.add_with_subtitles(media_params, subs).await.unwrap();
    assert_eq!(
        items.len(),
        3,
        "Media + 2 subtitles should yield 3 queue items"
    );

    assert_eq!(items[0].format_id, "yt-1080p");
    assert_eq!(items[1].format_id, "sub:en:srt");
    assert_eq!(items[2].format_id, "sub:de:vtt");

    // Check duplicate prevention: re-adding sub:en:srt fails duplicate check
    let dup_sub = StartDownloadParams {
        source_url: "https://www.youtube.com/watch?v=TEST_SUB_123".to_string(),
        service_name: "youtube".to_string(),
        title: "Test Video.en".to_string(),
        duration: 120,
        thumbnail_url: None,
        format_id: "sub:en:srt".to_string(),
        download_url: "https://example.com/timedtext_en".to_string(),
        requires_merge: false,
        matching_audio_url: None,
        container: "srt".to_string(),
        quality_label: "English".to_string(),
        destination_dir: None,
        selected_subtitles: Vec::new(),
        embed_subtitles: false,
        subtitle_mode: None,
        audio_options: None,
        video_codec: None,
        audio_codec: None,
    };

    let res = queue.add(dup_sub).await;
    assert!(res.is_err(), "Duplicate subtitle item must be rejected");
}

#[tokio::test]
async fn test_subtitle_failure_isolation() {
    let repo = create_in_memory_database().unwrap();
    let registry = Arc::new(ExtractorRegistry::new());
    let engine = EngineManager::new(repo.clone(), registry);
    let settings = Arc::new(Mutex::new(AppSettings::default()));
    let queue = QueueManager::new(repo.clone(), engine, settings);

    let media_params = StartDownloadParams {
        source_url: "https://www.youtube.com/watch?v=TEST_ISOLATION_456".to_string(),
        service_name: "youtube".to_string(),
        title: "Media Job".to_string(),
        duration: 60,
        thumbnail_url: None,
        format_id: "yt-720p".to_string(),
        download_url: "https://example.com/video.mp4".to_string(),
        requires_merge: false,
        matching_audio_url: None,
        container: "mp4".to_string(),
        quality_label: "720p".to_string(),
        destination_dir: None,
        selected_subtitles: Vec::new(),
        embed_subtitles: false,
        subtitle_mode: None,
        audio_options: None,
        video_codec: None,
        audio_codec: None,
    };

    let sub_params = StartDownloadParams {
        source_url: "https://www.youtube.com/watch?v=TEST_ISOLATION_456".to_string(),
        service_name: "youtube".to_string(),
        title: "Media Job.ms".to_string(),
        duration: 60,
        thumbnail_url: None,
        format_id: "sub:ms:srt".to_string(),
        download_url: "https://example.com/invalid_sub".to_string(),
        requires_merge: false,
        matching_audio_url: None,
        container: "srt".to_string(),
        quality_label: "Malay".to_string(),
        destination_dir: None,
        selected_subtitles: Vec::new(),
        embed_subtitles: false,
        subtitle_mode: None,
        audio_options: None,
        video_codec: None,
        audio_codec: None,
    };

    let media_item = queue.add(media_params).await.unwrap();
    let sub_item = queue.add(sub_params).await.unwrap();

    // Simulate media completed and subtitle failed
    queue
        .handle_engine_state_change(&media_item.id, "completed", None)
        .await;
    queue
        .handle_engine_state_change(&sub_item.id, "failed", Some("HTTP 404 Subtitle not found"))
        .await;

    let media_after = repo.get_queue_item(&media_item.id).unwrap().unwrap();
    let sub_after = repo.get_queue_item(&sub_item.id).unwrap().unwrap();

    assert_eq!(
        media_after.status,
        QueueStatus::Completed,
        "Media must complete successfully"
    );
    assert_eq!(
        sub_after.status,
        QueueStatus::Failed,
        "Subtitle failure is isolated"
    );
}

#[tokio::test]
async fn test_real_youtube_video_with_no_subtitles() {
    let registry = ExtractorRegistry::new();
    let url = Url::parse("https://www.youtube.com/watch?v=FyS5dAywkEo").unwrap();
    let extractor = registry
        .find_extractor(&url)
        .expect("YouTube extractor found");

    let ctx = ExtractionContext::default();
    let res = extractor.extract(&url, &ctx).await;

    if let Ok((meta, manifest)) = res {
        println!("Title: {}", meta.title);
        println!("Discovered subtitles count: {}", manifest.subtitles.len());
        assert_eq!(
            manifest.subtitles.len(),
            0,
            "FyS5dAywkEo has no captions; extractor must return empty vec gracefully"
        );
    } else {
        println!("Video unavailable in network environment, skipped");
    }
}

#[tokio::test]
async fn test_real_youtube_video_with_captions() {
    let registry = ExtractorRegistry::new();
    let url = Url::parse("https://www.youtube.com/watch?v=jNQXAC9IVRw").unwrap();
    let extractor = registry
        .find_extractor(&url)
        .expect("YouTube extractor found");

    let ctx = ExtractionContext::default();
    let res = extractor.extract(&url, &ctx).await;

    if let Ok((meta, manifest)) = res {
        println!("Title: {}", meta.title);
        println!("Subtitles count: {}", manifest.subtitles.len());

        assert!(
            !manifest.subtitles.is_empty(),
            "jNQXAC9IVRw should have captions"
        );

        let manual_tracks: Vec<&SubtitleTrack> = manifest
            .subtitles
            .iter()
            .filter(|s| !s.is_auto_generated)
            .collect();
        println!("Manual tracks count: {}", manual_tracks.len());

        for track in manifest.subtitles.iter().take(5) {
            println!(
                "  - [{}] {} (auto: {})",
                track.language_code, track.language_name, track.is_auto_generated
            );
        }

        // Test downloading one track if URL is present
        if let Some(track) = manifest.subtitles.first() {
            if let Some(ref dl_url) = track.download_url {
                let tmp_dir = get_test_dir("ff_real_sub");
                let sub_path = tmp_dir.join("me_at_the_zoo.en.srt");
                let client = reqwest::Client::new();

                let res =
                    freefetch_lib::engine::subtitle_downloader::download_and_convert_subtitle(
                        &client,
                        dl_url,
                        &sub_path,
                        SubtitleFormat::Srt,
                        None,
                        None,
                    )
                    .await;

                if let Ok(bytes) = res {
                    assert!(bytes > 0);
                    assert!(sub_path.exists());
                    let content = tokio::fs::read_to_string(&sub_path).await.unwrap();
                    assert!(content.contains("-->"));
                    println!("Successfully downloaded and verified SRT subtitle! Bytes: {bytes}");
                } else {
                    println!("Timedtext direct fetch timed out or restricted in this test run");
                }
                let _ = std::fs::remove_dir_all(&tmp_dir);
            }
        }
    } else {
        println!("Video unavailable in network environment, skipped");
    }
}
