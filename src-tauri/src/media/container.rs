use crate::core::error::FreeFetchError;

/// Normalizes a video codec string to a canonical token (e.g., "vp9", "vp8", "av1", "h264", "hevc").
pub fn normalize_video_codec(vcodec: &str) -> String {
    let lower = vcodec.trim().to_lowercase();
    if lower.is_empty() || lower == "none" || lower == "null" {
        return "none".to_string();
    }
    // Handle yt-dlp / MIME style codec designations
    if lower.starts_with("vp09") || lower == "vp9" {
        "vp9".to_string()
    } else if lower.starts_with("vp08") || lower == "vp8" {
        "vp8".to_string()
    } else if lower.starts_with("av01") || lower == "av1" {
        "av1".to_string()
    } else if lower.starts_with("avc1")
        || lower.starts_with("avc3")
        || lower == "h264"
        || lower == "x264"
    {
        "h264".to_string()
    } else if lower.starts_with("hev1")
        || lower.starts_with("hvc1")
        || lower == "h265"
        || lower == "hevc"
        || lower == "x265"
    {
        "hevc".to_string()
    } else {
        // Strip everything after first dot or space
        lower.split('.').next().unwrap_or(&lower).trim().to_string()
    }
}

/// Normalizes an audio codec string to a canonical token (e.g., "opus", "vorbis", "aac", "mp3", "flac").
pub fn normalize_audio_codec(acodec: &str) -> String {
    let lower = acodec.trim().to_lowercase();
    if lower.is_empty() || lower == "none" || lower == "null" {
        return "none".to_string();
    }
    if lower.starts_with("mp4a") || lower == "aac" {
        "aac".to_string()
    } else if lower == "opus" || lower.starts_with("opus") {
        "opus".to_string()
    } else if lower == "vorbis" || lower.starts_with("vorbis") {
        "vorbis".to_string()
    } else if lower == "mp3" || lower == "mp3lame" || lower.starts_with("mp3") {
        "mp3".to_string()
    } else if lower == "flac" || lower.starts_with("flac") {
        "flac".to_string()
    } else if lower == "alac" || lower.starts_with("alac") {
        "alac".to_string()
    } else if lower == "ac3" || lower == "ac-3" {
        "ac3".to_string()
    } else if lower == "eac3" || lower == "ec-3" {
        "eac3".to_string()
    } else {
        lower.split('.').next().unwrap_or(&lower).trim().to_string()
    }
}

/// Normalizes container extension (e.g., ".webm" -> "webm", "MP4" -> "mp4").
pub fn normalize_container(container: &str) -> String {
    container.trim().trim_start_matches('.').to_lowercase()
}

/// Checks whether a normalized video codec is supported by the target container format.
pub fn is_video_codec_compatible(container: &str, vcodec: &str) -> bool {
    let c = normalize_container(container);
    let v = normalize_video_codec(vcodec);
    if v == "none" || v == "auto" {
        return true;
    }

    match c.as_str() {
        "webm" => matches!(v.as_str(), "vp8" | "vp9" | "av1"),
        "mp4" | "m4v" | "mov" => matches!(v.as_str(), "h264" | "hevc" | "av1" | "vp9"),
        "mkv" => true, // Matroska accepts virtually all video formats
        _ => true,
    }
}

/// Checks whether a normalized audio codec is supported by the target container format.
pub fn is_audio_codec_compatible(container: &str, acodec: &str) -> bool {
    let c = normalize_container(container);
    let a = normalize_audio_codec(acodec);
    if a == "none" || a == "auto" {
        return true;
    }

    match c.as_str() {
        "webm" => matches!(a.as_str(), "opus" | "vorbis"),
        "mp4" | "m4v" | "mov" => {
            matches!(a.as_str(), "aac" | "mp3" | "opus" | "alac" | "ac3" | "eac3")
        }
        "mkv" => true, // Matroska accepts virtually all audio formats
        "mp3" => a == "mp3",
        "m4a" => matches!(a.as_str(), "aac" | "alac"),
        "flac" => a == "flac",
        "opus" => a == "opus",
        _ => true,
    }
}

/// Checks whether a subtitle track format is compatible for embedding into the target container.
pub fn is_subtitle_codec_compatible(
    container: &str,
    sub_mode: &str,
    sub_format: Option<&str>,
) -> bool {
    let mode = sub_mode.trim().to_lowercase();
    if mode == "none" || mode == "hardcode" {
        return true;
    }
    if mode != "embed" {
        return true;
    }

    let c = normalize_container(container);
    let s = sub_format.map(|fmt| fmt.trim().trim_start_matches('.').to_lowercase());

    match c.as_str() {
        "webm" => match s.as_deref() {
            Some(fmt) => matches!(fmt, "vtt" | "webvtt"),
            None => true,
        },
        "mp4" | "m4v" | "mov" => true, // FFmpeg handles conversion to mov_text
        "mkv" => true,                 // MKV accepts srt, vtt, ass, etc.
        _ => true,
    }
}

/// Authoritative multi-tier stream combination validator.
///
/// Ensures no incompatible video, audio, or subtitle stream combination reaches FFmpeg muxing.
pub fn validate_stream_combination(
    container: &str,
    vcodec: Option<&str>,
    acodec: Option<&str>,
    sub_mode: Option<&str>,
    sub_format: Option<&str>,
) -> Result<(), FreeFetchError> {
    let c = normalize_container(container);

    if let Some(vc) = vcodec {
        let normalized_v = normalize_video_codec(vc);
        if normalized_v != "none"
            && normalized_v != "auto"
            && !is_video_codec_compatible(&c, &normalized_v)
        {
            return Err(FreeFetchError::IncompatibleContainerCodec {
                container: c.clone(),
                message: format!(
                    "Video codec '{vc}' ({normalized_v}) is not supported in {c} container. {c} requires VP8, VP9, or AV1 video."
                ),
            });
        }
    }

    if let Some(ac) = acodec {
        let normalized_a = normalize_audio_codec(ac);
        if normalized_a != "none"
            && normalized_a != "auto"
            && !is_audio_codec_compatible(&c, &normalized_a)
        {
            return Err(FreeFetchError::IncompatibleContainerCodec {
                container: c.clone(),
                message: format!(
                    "Audio codec '{ac}' ({normalized_a}) is not supported in {c} container. {c} requires Opus or Vorbis audio."
                ),
            });
        }
    }

    if let Some(sm) = sub_mode {
        if !is_subtitle_codec_compatible(&c, sm, sub_format) {
            let fmt_str = sub_format.unwrap_or("unknown");
            return Err(FreeFetchError::IncompatibleContainerCodec {
                container: c.clone(),
                message: format!(
                    "Subtitle format '{fmt_str}' cannot be embedded directly into {c} container. WebM only supports WebVTT."
                ),
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalizers() {
        assert_eq!(normalize_video_codec("vp09.00.41.08"), "vp9");
        assert_eq!(normalize_video_codec("av01.0.08M.08"), "av1");
        assert_eq!(normalize_video_codec("avc1.640028"), "h264");
        assert_eq!(normalize_video_codec("hev1.1.6.L93.B0"), "hevc");
        assert_eq!(normalize_video_codec("none"), "none");

        assert_eq!(normalize_audio_codec("mp4a.40.2"), "aac");
        assert_eq!(normalize_audio_codec("opus"), "opus");
        assert_eq!(normalize_audio_codec("vorbis"), "vorbis");
        assert_eq!(normalize_audio_codec("mp3"), "mp3");
        assert_eq!(normalize_audio_codec("flac"), "flac");
    }

    #[test]
    fn test_webm_compatibility() {
        assert!(is_video_codec_compatible("webm", "vp9"));
        assert!(is_video_codec_compatible("webm", "vp09.00.41.08"));
        assert!(is_video_codec_compatible("webm", "vp8"));
        assert!(is_video_codec_compatible("webm", "av1"));
        assert!(!is_video_codec_compatible("webm", "avc1.640028"));
        assert!(!is_video_codec_compatible("webm", "h264"));
        assert!(!is_video_codec_compatible("webm", "hevc"));

        assert!(is_audio_codec_compatible("webm", "opus"));
        assert!(is_audio_codec_compatible("webm", "vorbis"));
        assert!(!is_audio_codec_compatible("webm", "mp4a.40.2"));
        assert!(!is_audio_codec_compatible("webm", "aac"));
        assert!(!is_audio_codec_compatible("webm", "mp3"));
        assert!(!is_audio_codec_compatible("webm", "flac"));
    }

    #[test]
    fn test_mp4_and_mkv_compatibility() {
        assert!(is_video_codec_compatible("mp4", "h264"));
        assert!(is_video_codec_compatible("mp4", "av1"));
        assert!(is_audio_codec_compatible("mp4", "aac"));
        assert!(is_audio_codec_compatible("mp4", "mp3"));
        assert!(is_audio_codec_compatible("mp4", "opus"));

        assert!(is_video_codec_compatible("mkv", "vp9"));
        assert!(is_video_codec_compatible("mkv", "h264"));
        assert!(is_audio_codec_compatible("mkv", "opus"));
        assert!(is_audio_codec_compatible("mkv", "aac"));
    }

    #[test]
    fn test_stream_combination_validation() {
        // WebM valid
        assert!(validate_stream_combination("webm", Some("vp9"), Some("opus"), None, None).is_ok());
        assert!(validate_stream_combination("webm", Some("av1"), Some("opus"), None, None).is_ok());

        // WebM invalid video
        assert!(
            validate_stream_combination("webm", Some("h264"), Some("opus"), None, None).is_err()
        );

        // WebM invalid audio (The exact reported production defect!)
        let err = validate_stream_combination("webm", Some("vp9"), Some("aac"), None, None);
        assert!(err.is_err());
        match err.unwrap_err() {
            FreeFetchError::IncompatibleContainerCodec { container, message } => {
                assert_eq!(container, "webm");
                assert!(message.contains("Opus or Vorbis"));
            }
            other => panic!("Expected IncompatibleContainerCodec, got {:?}", other),
        }

        // MP4 valid
        assert!(validate_stream_combination("mp4", Some("h264"), Some("aac"), None, None).is_ok());
    }
}
