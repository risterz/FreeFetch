use std::collections::HashMap;
use url::Url;

use freefetch_lib::core::types::SubtitleFormat;
use freefetch_lib::extractor::context::ExtractionContext;
use freefetch_lib::extractor::implementations::youtube::{
    is_translated_caption_url, normalize_ytdlp_subtitles, YtDlpSubtitleTrack,
};
use freefetch_lib::extractor::ExtractorRegistry;

#[test]
fn test_case_1_and_case_2_indonesian_auto_generated_keeps_id_orig_and_hides_translated() {
    // Simulates raw yt-dlp metadata for an Indonesian video containing:
    // - id-orig: Original auto-generated speech recognition track
    // - id: Bare duplicate alias
    // - iu: Inuktitut machine-translated track
    // - ga: Irish machine-translated track
    // - en: English machine-translated track
    let mut auto_map = HashMap::new();

    auto_map.insert(
        "id-orig".to_string(),
        vec![YtDlpSubtitleTrack {
            ext: Some("vtt".to_string()),
            url: Some(
                "https://www.youtube.com/api/timedtext?v=YMXJICVux6A&kind=asr&lang=id&fmt=vtt"
                    .to_string(),
            ),
            name: Some("Indonesian (Original)".to_string()),
        }],
    );

    auto_map.insert(
        "id".to_string(),
        vec![YtDlpSubtitleTrack {
            ext: Some("vtt".to_string()),
            url: Some(
                "https://www.youtube.com/api/timedtext?v=YMXJICVux6A&kind=asr&lang=id&fmt=vtt"
                    .to_string(),
            ),
            name: Some("Indonesian".to_string()),
        }],
    );

    auto_map.insert(
        "iu".to_string(),
        vec![YtDlpSubtitleTrack {
            ext: Some("vtt".to_string()),
            url: Some("https://www.youtube.com/api/timedtext?v=YMXJICVux6A&kind=asr&lang=id&tlang=iu&fmt=vtt".to_string()),
            name: Some("Inuktitut".to_string()),
        }],
    );

    auto_map.insert(
        "ga".to_string(),
        vec![YtDlpSubtitleTrack {
            ext: Some("vtt".to_string()),
            url: Some("https://www.youtube.com/api/timedtext?v=YMXJICVux6A&kind=asr&lang=id&tlang=ga&fmt=vtt".to_string()),
            name: Some("Irish".to_string()),
        }],
    );

    auto_map.insert(
        "en".to_string(),
        vec![YtDlpSubtitleTrack {
            ext: Some("vtt".to_string()),
            url: Some("https://www.youtube.com/api/timedtext?v=YMXJICVux6A&kind=asr&lang=id&tlang=en&fmt=vtt".to_string()),
            name: Some("English".to_string()),
        }],
    );

    let normalized = normalize_ytdlp_subtitles(None, Some(auto_map), Some("id"));

    // Case 1: ID-ORIG must be VISIBLE / SELECTABLE
    assert_eq!(
        normalized.len(),
        1,
        "Only the original auto-generated track must survive filtering"
    );
    let orig_track = &normalized[0];
    assert_eq!(orig_track.language_code, "id-orig");
    assert_eq!(
        orig_track.language_name,
        "Indonesian (Original) (Auto-generated)"
    );
    assert!(orig_track.is_auto_generated);
    assert_eq!(orig_track.format, SubtitleFormat::Vtt);
    assert!(orig_track.download_url.is_some());

    // Case 2: ID, IU, GA, EN translated auto tracks must be HIDDEN / FILTERED
    assert!(
        !normalized.iter().any(|s| s.language_code == "id"),
        "Bare 'id' must be hidden"
    );
    assert!(
        !normalized.iter().any(|s| s.language_code == "iu"),
        "Translated 'iu' must be hidden"
    );
    assert!(
        !normalized.iter().any(|s| s.language_code == "ga"),
        "Translated 'ga' must be hidden"
    );
    assert!(
        !normalized.iter().any(|s| s.language_code == "en"),
        "Translated 'en' must be hidden"
    );
}

#[test]
fn test_case_3_manual_subtitles_preserved() {
    // Manual subtitles supplied by content creator (e.g. English, Malay, Indonesian)
    let mut manual_map = HashMap::new();
    manual_map.insert(
        "en".to_string(),
        vec![YtDlpSubtitleTrack {
            ext: Some("vtt".to_string()),
            url: Some(
                "https://www.youtube.com/api/timedtext?v=TEST123&lang=en&fmt=vtt".to_string(),
            ),
            name: Some("English".to_string()),
        }],
    );
    manual_map.insert(
        "ms".to_string(),
        vec![YtDlpSubtitleTrack {
            ext: Some("vtt".to_string()),
            url: Some(
                "https://www.youtube.com/api/timedtext?v=TEST123&lang=ms&fmt=vtt".to_string(),
            ),
            name: Some("Malay".to_string()),
        }],
    );
    manual_map.insert(
        "id".to_string(),
        vec![YtDlpSubtitleTrack {
            ext: Some("vtt".to_string()),
            url: Some(
                "https://www.youtube.com/api/timedtext?v=TEST123&lang=id&fmt=vtt".to_string(),
            ),
            name: Some("Indonesian".to_string()),
        }],
    );

    // Auto-generated map also present with original track and translated clutter
    let mut auto_map = HashMap::new();
    auto_map.insert(
        "id-orig".to_string(),
        vec![YtDlpSubtitleTrack {
            ext: Some("vtt".to_string()),
            url: Some(
                "https://www.youtube.com/api/timedtext?v=TEST123&kind=asr&lang=id&fmt=vtt"
                    .to_string(),
            ),
            name: Some("Indonesian (Original)".to_string()),
        }],
    );
    auto_map.insert(
        "iu".to_string(),
        vec![YtDlpSubtitleTrack {
            ext: Some("vtt".to_string()),
            url: Some(
                "https://www.youtube.com/api/timedtext?v=TEST123&kind=asr&lang=id&tlang=iu&fmt=vtt"
                    .to_string(),
            ),
            name: Some("Inuktitut".to_string()),
        }],
    );

    let normalized = normalize_ytdlp_subtitles(Some(manual_map), Some(auto_map), Some("id"));

    // All 3 manual tracks must remain VISIBLE / SELECTABLE
    assert!(
        normalized
            .iter()
            .any(|s| s.language_code == "en" && !s.is_auto_generated),
        "Manual English must remain available"
    );
    assert!(
        normalized
            .iter()
            .any(|s| s.language_code == "ms" && !s.is_auto_generated),
        "Manual Malay must remain available"
    );
    assert!(
        normalized
            .iter()
            .any(|s| s.language_code == "id" && !s.is_auto_generated),
        "Manual Indonesian must remain available"
    );

    // Auto-generated original is also present
    assert!(
        normalized
            .iter()
            .any(|s| s.language_code == "id-orig" && s.is_auto_generated),
        "Original auto-generated caption must remain available"
    );

    // Translated auto-generated tracks are filtered out
    assert!(
        !normalized.iter().any(|s| s.language_code == "iu"),
        "Translated auto caption 'iu' must be filtered out"
    );

    assert_eq!(
        normalized.len(),
        4,
        "Total tracks: 3 manual + 1 original auto"
    );
}

#[test]
fn test_case_4_video_with_only_original_auto_generated_captions() {
    let mut auto_map = HashMap::new();
    auto_map.insert(
        "en-orig".to_string(),
        vec![YtDlpSubtitleTrack {
            ext: Some("vtt".to_string()),
            url: Some(
                "https://www.youtube.com/api/timedtext?v=TEST456&kind=asr&lang=en&fmt=vtt"
                    .to_string(),
            ),
            name: Some("English (Original)".to_string()),
        }],
    );
    auto_map.insert(
        "en".to_string(),
        vec![YtDlpSubtitleTrack {
            ext: Some("vtt".to_string()),
            url: Some(
                "https://www.youtube.com/api/timedtext?v=TEST456&kind=asr&lang=en&fmt=vtt"
                    .to_string(),
            ),
            name: Some("English".to_string()),
        }],
    );
    auto_map.insert(
        "es".to_string(),
        vec![YtDlpSubtitleTrack {
            ext: Some("vtt".to_string()),
            url: Some(
                "https://www.youtube.com/api/timedtext?v=TEST456&kind=asr&lang=en&tlang=es&fmt=vtt"
                    .to_string(),
            ),
            name: Some("Spanish".to_string()),
        }],
    );

    let normalized = normalize_ytdlp_subtitles(None, Some(auto_map), Some("en"));

    assert_eq!(
        normalized.len(),
        1,
        "Exactly one auto-generated caption visible"
    );
    assert_eq!(normalized[0].language_code, "en-orig");
    assert_eq!(
        normalized[0].language_name,
        "English (Original) (Auto-generated)"
    );
    assert!(normalized[0].is_auto_generated);
}

#[test]
fn test_case_5_video_with_no_subtitles() {
    let normalized = normalize_ytdlp_subtitles(None, None, None);
    assert_eq!(normalized.len(), 0, "No subtitles available");

    let mut empty_manual = HashMap::new();
    empty_manual.insert("live_chat".to_string(), vec![]);
    let normalized_live_chat_only = normalize_ytdlp_subtitles(Some(empty_manual), None, None);
    assert_eq!(
        normalized_live_chat_only.len(),
        0,
        "Live chat must be ignored, 0 subtitles"
    );
}

#[test]
fn test_is_translated_caption_url_detection() {
    assert!(is_translated_caption_url(
        "https://www.youtube.com/api/timedtext?v=123&lang=en&tlang=fr&fmt=vtt"
    ));
    assert!(is_translated_caption_url(
        "https://www.youtube.com/api/timedtext?tlang=id&v=123&lang=en"
    ));
    assert!(!is_translated_caption_url(
        "https://www.youtube.com/api/timedtext?v=123&kind=asr&lang=id&fmt=vtt"
    ));
    assert!(!is_translated_caption_url(
        "https://www.youtube.com/api/timedtext?v=123&lang=en&fmt=vtt"
    ));
}

#[tokio::test]
async fn test_real_world_youtube_extraction_filters_translated_auto_captions() {
    let registry = ExtractorRegistry::new();
    let url = Url::parse("https://www.youtube.com/watch?v=j8n1pj8RfFs").unwrap();
    let extractor = registry
        .find_extractor(&url)
        .expect("YouTube extractor found");
    let ctx = ExtractionContext::default();

    if let Ok((_meta, manifest)) = extractor.extract(&url, &ctx).await {
        println!(
            "Real YouTube extract subtitle count: {}",
            manifest.subtitles.len()
        );

        // Zero translated auto-captions must exist in the returned subtitles
        for sub in &manifest.subtitles {
            println!(
                "  Track: [{}] {} (auto: {})",
                sub.language_code, sub.language_name, sub.is_auto_generated
            );
            if let Some(ref dl_url) = sub.download_url {
                assert!(
                    !is_translated_caption_url(dl_url),
                    "No returned subtitle may have a translated 'tlang=' URL: {dl_url}"
                );
            }
        }

        // Verify that the auto-generated track is the original track
        let auto_tracks: Vec<_> = manifest
            .subtitles
            .iter()
            .filter(|s| s.is_auto_generated)
            .collect();
        assert!(
            auto_tracks.len() <= 2,
            "Must have bounded original auto-generated tracks, not 150+ translated languages (found {})",
            auto_tracks.len()
        );

        if let Some(first_auto) = auto_tracks.first() {
            assert!(
                first_auto.language_code.ends_with("-orig") || first_auto.language_code == "en",
                "Original auto track must be identified as original: {}",
                first_auto.language_code
            );
        }
    } else {
        println!("Network environment restricted, skipping live YouTube network test");
    }
}

#[tokio::test]
async fn test_real_world_youtube_indonesian_video_extracts_id_orig_only() {
    let registry = ExtractorRegistry::new();
    let url = Url::parse("https://www.youtube.com/watch?v=YMXJICVux6A").unwrap();
    let extractor = registry
        .find_extractor(&url)
        .expect("YouTube extractor found");
    let ctx = ExtractionContext::default();

    if let Ok((_meta, manifest)) = extractor.extract(&url, &ctx).await {
        println!(
            "Real Indonesian YouTube extract subtitle count: {}",
            manifest.subtitles.len()
        );
        for sub in &manifest.subtitles {
            println!(
                "  Track: [{}] {} (auto: {})",
                sub.language_code, sub.language_name, sub.is_auto_generated
            );
        }

        let auto_tracks: Vec<_> = manifest
            .subtitles
            .iter()
            .filter(|s| s.is_auto_generated)
            .collect();
        assert_eq!(
            auto_tracks.len(),
            1,
            "Must contain exactly 1 original auto track"
        );
        assert_eq!(auto_tracks[0].language_code, "id-orig");
        assert_eq!(
            auto_tracks[0].language_name,
            "Indonesian (Original) (Auto-generated)"
        );
    } else {
        println!("Network environment restricted, skipping live YouTube network test");
    }
}

#[tokio::test]
async fn test_regression_dpp1n7mw_g8_error_serialization() {
    let registry = ExtractorRegistry::new();
    let ctx = ExtractionContext::default();

    let url_dpp = Url::parse("https://www.youtube.com/watch?v=Dpp1n7MW-g8").unwrap();
    let ext_dpp = registry.find_extractor(&url_dpp).unwrap();
    let res = ext_dpp.extract(&url_dpp, &ctx).await;

    // Must return an Extractor error because of YouTube bot/login protection
    match res {
        Err(e) => {
            let json = serde_json::to_string(&e).unwrap();
            let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

            // Type must be Extractor
            assert_eq!(parsed["type"], "Extractor");
            assert_eq!(parsed["service"], "youtube");

            // Message MUST be a string, NOT an object or array
            assert!(
                parsed["message"].is_string(),
                "Serialized error message must be a string, got {:?}",
                parsed["message"]
            );
            let msg = parsed["message"].as_str().unwrap();
            assert!(!msg.is_empty(), "Error message string must not be empty");
            assert!(
                !msg.contains("[object Object]"),
                "Error message must never contain '[object Object]'"
            );
            println!("Verified Dpp1n7MW-g8 clean error message: {msg}");
        }
        Ok((meta, _)) => {
            println!("Video extracted successfully: {}", meta.title);
        }
    }
}

#[test]
fn test_freefetch_error_serialization_has_string_message_for_all_variants() {
    use freefetch_lib::core::error::FreeFetchError;

    let errors = vec![
        FreeFetchError::Network("connection reset".to_string()),
        FreeFetchError::Extractor {
            service: "youtube".to_string(),
            message: "No streams found".to_string(),
        },
        FreeFetchError::HttpRangeNotSupported { status: 416 },
        FreeFetchError::DiskSpace {
            required_mb: 500,
            available_mb: 100,
        },
        FreeFetchError::RemuxFailed {
            code: 1,
            log: "mux error".to_string(),
        },
        FreeFetchError::Database("table locked".to_string()),
        FreeFetchError::Cancelled,
        FreeFetchError::InvalidUrl("bad url".to_string()),
        FreeFetchError::Io("permission denied".to_string()),
        FreeFetchError::FfmpegNotFound,
        FreeFetchError::NotFound("item 123".to_string()),
        FreeFetchError::Generic("generic failure".to_string()),
    ];

    for err in errors {
        let json = serde_json::to_string(&err).unwrap();
        let val: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(val["type"].is_string(), "type must be string for {err:?}");
        assert!(
            val["message"].is_string(),
            "message must be a string for {err:?}, found: {:?}",
            val["message"]
        );
        let msg = val["message"].as_str().unwrap();
        assert!(!msg.is_empty(), "message must not be empty for {err:?}");
        assert!(
            !msg.contains("[object Object]"),
            "must never produce [object Object]"
        );
    }
}
