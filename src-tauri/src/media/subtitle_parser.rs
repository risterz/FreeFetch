use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubtitleCue {
    pub index: Option<usize>,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SubtitleDocument {
    pub cues: Vec<SubtitleCue>,
}

/// Parses timestamp string `HH:MM:SS.mmm` or `MM:SS.mmm` into milliseconds.
pub fn parse_timestamp_ms(ts: &str) -> Option<u64> {
    let clean = ts.trim();
    let parts: Vec<&str> = clean.split(':').collect();
    match parts.len() {
        2 => {
            // MM:SS.mmm
            let min: u64 = parts[0].parse().ok()?;
            let sec_parts: Vec<&str> = parts[1].split('.').collect();
            if sec_parts.len() != 2 {
                return None;
            }
            let sec: u64 = sec_parts[0].parse().ok()?;
            let mut ms_str = sec_parts[1].to_string();
            while ms_str.len() < 3 {
                ms_str.push('0');
            }
            ms_str.truncate(3);
            let ms: u64 = ms_str.parse().ok()?;
            Some(min * 60_000 + sec * 1_000 + ms)
        }
        3 => {
            // HH:MM:SS.mmm
            let hour: u64 = parts[0].parse().ok()?;
            let min: u64 = parts[1].parse().ok()?;
            let sec_parts: Vec<&str> = parts[2].split('.').collect();
            if sec_parts.len() != 2 {
                return None;
            }
            let sec: u64 = sec_parts[0].parse().ok()?;
            let mut ms_str = sec_parts[1].to_string();
            while ms_str.len() < 3 {
                ms_str.push('0');
            }
            ms_str.truncate(3);
            let ms: u64 = ms_str.parse().ok()?;
            Some(hour * 3_600_000 + min * 60_000 + sec * 1_000 + ms)
        }
        _ => None,
    }
}

/// Formats milliseconds into WebVTT timestamp `HH:MM:SS.mmm`.
pub fn format_timestamp_vtt(ms: u64) -> String {
    let hours = ms / 3_600_000;
    let rem = ms % 3_600_000;
    let mins = rem / 60_000;
    let rem = rem % 60_000;
    let secs = rem / 1_000;
    let millis = rem % 1_000;
    format!("{hours:02}:{mins:02}:{secs:02}.{millis:03}")
}

/// Formats milliseconds into SRT timestamp `HH:MM:SS,mmm` (comma separator).
pub fn format_timestamp_srt(ms: u64) -> String {
    let hours = ms / 3_600_000;
    let rem = ms % 3_600_000;
    let mins = rem / 60_000;
    let rem = rem % 60_000;
    let secs = rem / 1_000;
    let millis = rem % 1_000;
    format!("{hours:02}:{mins:02}:{secs:02},{millis:03}")
}

/// Strips WebVTT inline tags such as `<c.color>`, `<v Name>`, `<b>`, `<i>`, `</u>`, `<00:00:01.000>`
pub fn strip_vtt_tags(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut inside_tag = false;

    for ch in input.chars() {
        if ch == '<' {
            inside_tag = true;
        } else if ch == '>' {
            inside_tag = false;
        } else if !inside_tag {
            out.push(ch);
        }
    }

    // Decode standard HTML entities commonly found in timedtext
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

/// Parses raw WebVTT content safely into a `SubtitleDocument`.
pub fn parse_vtt(input: &str) -> SubtitleDocument {
    // 1. Strip UTF-8 BOM if present
    let raw = input.strip_prefix('\u{feff}').unwrap_or(input);

    // 2. Normalize CRLF to LF
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");

    let lines: Vec<&str> = normalized.lines().collect();
    let mut cues = Vec::new();
    let mut i = 0;

    // Skip WEBVTT header and header comments
    while i < lines.len() {
        let line = lines[i].trim();
        if line.starts_with("WEBVTT") || line.is_empty() {
            i += 1;
            continue;
        }
        // Check for blocks like NOTE, STYLE, REGION
        if line.starts_with("NOTE") || line.starts_with("STYLE") || line.starts_with("REGION") {
            i += 1;
            while i < lines.len() && !lines[i].trim().is_empty() {
                i += 1;
            }
            continue;
        }
        break;
    }

    // Parse cues
    while i < lines.len() {
        let line = lines[i].trim();
        if line.is_empty() {
            i += 1;
            continue;
        }

        // Check if line is block to skip (NOTE, STYLE, REGION)
        if line.starts_with("NOTE") || line.starts_with("STYLE") || line.starts_with("REGION") {
            i += 1;
            while i < lines.len() && !lines[i].trim().is_empty() {
                i += 1;
            }
            continue;
        }

        // Could be a cue identifier OR the timestamp line directly
        let mut identifier = None;
        let timing_line;

        if line.contains("-->") {
            timing_line = line;
        } else {
            identifier = Some(line.to_string());
            i += 1;
            if i >= lines.len() {
                break;
            }
            timing_line = lines[i].trim();
            if !timing_line.contains("-->") {
                // Not a valid cue timing line, advance and continue
                i += 1;
                continue;
            }
        }

        // Parse timing: `00:00:01.000 --> 00:00:04.000 [settings]`
        let arrow_parts: Vec<&str> = timing_line.split("-->").collect();
        if arrow_parts.len() != 2 {
            i += 1;
            continue;
        }

        let start_str = arrow_parts[0].trim();
        let end_part = arrow_parts[1].trim();
        let end_str = end_part.split_whitespace().next().unwrap_or(end_part);

        let start_ms = match parse_timestamp_ms(start_str) {
            Some(ms) => ms,
            None => {
                i += 1;
                continue;
            }
        };

        let end_ms = match parse_timestamp_ms(end_str) {
            Some(ms) => ms,
            None => {
                i += 1;
                continue;
            }
        };

        // Collect cue text lines until empty line
        i += 1;
        let mut text_lines = Vec::new();
        while i < lines.len() && !lines[i].trim().is_empty() {
            let stripped = strip_vtt_tags(lines[i]);
            text_lines.push(stripped);
            i += 1;
        }

        let cue_text = text_lines.join("\n").trim().to_string();
        if !cue_text.is_empty() {
            let index = identifier.and_then(|id| id.parse::<usize>().ok());
            cues.push(SubtitleCue {
                index,
                start_ms,
                end_ms,
                text: cue_text,
            });
        }
    }

    SubtitleDocument { cues }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_timestamp_ms() {
        assert_eq!(parse_timestamp_ms("00:01.000"), Some(1_000));
        assert_eq!(parse_timestamp_ms("01:23.456"), Some(83_456));
        assert_eq!(parse_timestamp_ms("01:02:03.456"), Some(3_723_456));
        assert_eq!(parse_timestamp_ms("invalid"), None);
    }

    #[test]
    fn test_parse_basic_vtt() {
        let vtt = "WEBVTT\n\n00:00:01.000 --> 00:00:04.000\nHello World\n\n00:00:05.500 --> 00:00:08.000\nSecond line\n";
        let doc = parse_vtt(vtt);
        assert_eq!(doc.cues.len(), 2);
        assert_eq!(doc.cues[0].start_ms, 1_000);
        assert_eq!(doc.cues[0].end_ms, 4_000);
        assert_eq!(doc.cues[0].text, "Hello World");
        assert_eq!(doc.cues[1].start_ms, 5_500);
        assert_eq!(doc.cues[1].end_ms, 8_000);
        assert_eq!(doc.cues[1].text, "Second line");
    }

    #[test]
    fn test_parse_vtt_with_bom_and_crlf() {
        let vtt = "\u{feff}WEBVTT\r\n\r\n1\r\n00:00:01.000 --> 00:00:03.000\r\nLine 1\r\n\r\n2\r\n00:00:04.000 --> 00:00:06.000\r\nLine 2\r\n";
        let doc = parse_vtt(vtt);
        assert_eq!(doc.cues.len(), 2);
        assert_eq!(doc.cues[0].index, Some(1));
        assert_eq!(doc.cues[0].text, "Line 1");
        assert_eq!(doc.cues[1].index, Some(2));
        assert_eq!(doc.cues[1].text, "Line 2");
    }

    #[test]
    fn test_parse_multiline_cues_and_styling() {
        let vtt = "WEBVTT\n\n00:00:01.000 --> 00:00:05.000 line:0%\n<v Speaker 1>First line of dialogue.</v>\n<c.yellow>Second line</c> &amp; text\n";
        let doc = parse_vtt(vtt);
        assert_eq!(doc.cues.len(), 1);
        assert_eq!(
            doc.cues[0].text,
            "First line of dialogue.\nSecond line & text"
        );
    }

    #[test]
    fn test_parse_malformed_vtt_resilience() {
        let vtt = "WEBVTT\n\nNOTE comment here\n\nSTYLE\n::cue { color: red; }\n\ncorrupt timestamp\nHello\n\n00:00:10.000 --> 00:00:12.000\nValid cue\n";
        let doc = parse_vtt(vtt);
        assert_eq!(doc.cues.len(), 1);
        assert_eq!(doc.cues[0].text, "Valid cue");
        assert_eq!(doc.cues[0].start_ms, 10_000);
    }
}
