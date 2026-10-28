use super::subtitle_parser::{
    format_timestamp_srt, format_timestamp_vtt, parse_vtt, SubtitleDocument,
};

/// Converts a `SubtitleDocument` into standard SubRip (.srt) format.
pub fn document_to_srt(doc: &SubtitleDocument) -> String {
    let mut srt = String::new();

    for (i, cue) in doc.cues.iter().enumerate() {
        let cue_number = i + 1;
        let start_ts = format_timestamp_srt(cue.start_ms);
        let end_ts = format_timestamp_srt(cue.end_ms);

        srt.push_str(&format!("{cue_number}\n"));
        srt.push_str(&format!("{start_ts} --> {end_ts}\n"));
        srt.push_str(&cue.text);
        srt.push_str("\n\n");
    }

    srt
}

/// Converts raw WebVTT content directly into standard SubRip (.srt) string.
pub fn vtt_to_srt(vtt_content: &str) -> String {
    let doc = parse_vtt(vtt_content);
    document_to_srt(&doc)
}

/// Normalizes a `SubtitleDocument` into clean standard WebVTT format.
pub fn document_to_vtt(doc: &SubtitleDocument) -> String {
    let mut vtt = String::from("WEBVTT\n\n");

    for (i, cue) in doc.cues.iter().enumerate() {
        let cue_number = i + 1;
        let start_ts = format_timestamp_vtt(cue.start_ms);
        let end_ts = format_timestamp_vtt(cue.end_ms);

        vtt.push_str(&format!("{cue_number}\n"));
        vtt.push_str(&format!("{start_ts} --> {end_ts}\n"));
        vtt.push_str(&cue.text);
        vtt.push_str("\n\n");
    }

    vtt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vtt_to_srt_conversion() {
        let vtt = "WEBVTT\n\n00:00:01.250 --> 00:00:03.750\nHello world!\n\n00:00:04.000 --> 00:00:07.123\nSecond line of subtitles.\n";
        let srt = vtt_to_srt(vtt);

        let expected = "1\n00:00:01,250 --> 00:00:03,750\nHello world!\n\n2\n00:00:04,000 --> 00:00:07,123\nSecond line of subtitles.\n\n";
        assert_eq!(srt, expected);
    }

    #[test]
    fn test_vtt_to_srt_multiline_and_formatting() {
        let vtt = "WEBVTT\n\n01:05.100 --> 01:08.500\n<b>Alice:</b> Hi Bob!\n<i>Bob:</i> Hello!\n";
        let srt = vtt_to_srt(vtt);

        let expected = "1\n00:01:05,100 --> 00:01:08,500\nAlice: Hi Bob!\nBob: Hello!\n\n";
        assert_eq!(srt, expected);
    }

    #[test]
    fn test_document_to_vtt_roundtrip() {
        let vtt = "WEBVTT\n\n00:00:01.000 --> 00:00:02.000\nTest\n";
        let doc = parse_vtt(vtt);
        let normalized = document_to_vtt(&doc);
        assert!(normalized.starts_with("WEBVTT\n\n"));
        assert!(normalized.contains("00:00:01.000 --> 00:00:02.000\nTest\n\n"));
    }
}
