use crate::extractor::MediaMetadata;
use crate::media::audio::AudioTargetFormat;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioMetadata {
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub date: Option<String>,
    pub genre: Option<String>,
    pub track_number: Option<u32>,
    pub comment: Option<String>,
}

impl AudioMetadata {
    /// Sanitizes metadata field: strips control characters (\0..\x1F) and trims whitespace
    pub fn sanitize_tag(value: &str) -> String {
        value
            .chars()
            .filter(|c| !c.is_control())
            .collect::<String>()
            .trim()
            .to_string()
    }

    /// Strips common promotional noise/suffixes from music video titles
    pub fn clean_title_noise(raw: &str) -> String {
        let mut text = raw.to_string();

        let patterns = [
            "[Official Video]",
            "[Official Music Video]",
            "(Official Video)",
            "(Official Music Video)",
            "[Official Audio]",
            "(Official Audio)",
            "[Lyric Video]",
            "(Lyric Video)",
            "[Official Lyric Video]",
            "(Official Lyric Video)",
            "[Visualizer]",
            "(Visualizer)",
            "[Audio]",
            "(Audio)",
            "[Lyrics]",
            "(Lyrics)",
            "[HD]",
            "(HD)",
            "[4K]",
            "(4K)",
            "[HQ]",
            "(HQ)",
            "| Official Music Video",
            "| Official Video",
            " 4K",
            " 4k",
            " HD",
            " hd",
        ];

        for pattern in patterns {
            let lower_pat = pattern.to_lowercase();
            while let Some(pos) = text.to_lowercase().find(&lower_pat) {
                text.replace_range(pos..pos + pattern.len(), "");
            }
        }

        // Collapse multiple spaces
        while text.contains("  ") {
            text = text.replace("  ", " ");
        }

        // Clean up redundant punctuation or trailing dashes
        let cleaned = text.trim();
        let cleaned = cleaned.trim_end_matches(['-', '|', '/', '\\', ' ']);
        cleaned.trim().to_string()
    }

    /// Heuristic: splits "Artist - Title" if present, with noise cleanup
    pub fn parse_artist_and_title(
        raw_title: &str,
        fallback_artist: Option<&str>,
    ) -> (String, Option<String>) {
        let cleaned = Self::clean_title_noise(raw_title);

        // Check for " - " or " – " (en-dash) or " — " (em-dash) separator
        let separators = [" - ", " – ", " — "];
        for sep in separators {
            if let Some(idx) = cleaned.find(sep) {
                let left = cleaned[..idx]
                    .trim()
                    .trim_matches(['"', '\'', '“', '”', '`', ' ']);
                let right = cleaned[idx + sep.len()..]
                    .trim()
                    .trim_matches(['"', '\'', '“', '”', '`', ' ']);
                if !left.is_empty() && !right.is_empty() {
                    return (Self::sanitize_tag(right), Some(Self::sanitize_tag(left)));
                }
            }
        }

        let trimmed = cleaned.trim().trim_matches(['"', '\'', '“', '”', '`', ' ']);
        (
            Self::sanitize_tag(trimmed),
            fallback_artist
                .map(|a| a.trim().trim_matches(['"', '\'', '“', '”', '`', ' ']))
                .map(Self::sanitize_tag)
                .filter(|s| !s.is_empty()),
        )
    }

    /// Builds audio metadata from standard MediaMetadata with optional overrides
    pub fn from_media_metadata(
        meta: &MediaMetadata,
        playlist_title: Option<&str>,
        track_number: Option<u32>,
    ) -> Self {
        let (title, artist) = Self::parse_artist_and_title(&meta.title, meta.uploader.as_deref());

        let album = playlist_title
            .map(Self::sanitize_tag)
            .filter(|s| !s.is_empty())
            .or_else(|| meta.uploader.as_deref().map(Self::sanitize_tag));

        let album_artist = meta.uploader.as_deref().map(Self::sanitize_tag);

        // Extract 4-digit year from upload_date if available (e.g. "20210515" -> "2021")
        let date = meta.upload_date.as_ref().and_then(|d| {
            if d.len() >= 4 && d[..4].chars().all(|c| c.is_ascii_digit()) {
                Some(d[..4].to_string())
            } else {
                None
            }
        });

        Self {
            title,
            artist,
            album,
            album_artist,
            date,
            genre: Some("Music".to_string()),
            track_number,
            comment: Some(format!("Downloaded via FreeFetch: {}", meta.url)),
        }
    }

    /// Formats metadata key-value arguments for FFmpeg
    pub fn to_ffmpeg_args(&self, format: AudioTargetFormat) -> Vec<String> {
        let mut args = Vec::new();

        if format == AudioTargetFormat::Mp3 {
            // ID3v2.3 tag standard ensures maximum compatibility with Windows Explorer, iTunes, and cars
            args.push("-id3v2_version".to_string());
            args.push("3".to_string());
            args.push("-write_id3v1".to_string());
            args.push("1".to_string());
        }

        if !self.title.is_empty() {
            args.push("-metadata".to_string());
            args.push(format!("title={}", self.title));
        }

        if let Some(ref artist) = self.artist {
            args.push("-metadata".to_string());
            args.push(format!("artist={artist}"));
        }

        if let Some(ref album) = self.album {
            args.push("-metadata".to_string());
            args.push(format!("album={album}"));
        }

        if let Some(ref aa) = self.album_artist {
            args.push("-metadata".to_string());
            args.push(format!("album_artist={aa}"));
        }

        if let Some(ref date) = self.date {
            args.push("-metadata".to_string());
            args.push(format!("date={date}"));
        }

        if let Some(ref genre) = self.genre {
            args.push("-metadata".to_string());
            args.push(format!("genre={genre}"));
        }

        if let Some(track) = self.track_number {
            args.push("-metadata".to_string());
            args.push(format!("track={track}"));
        }

        if let Some(ref comment) = self.comment {
            args.push("-metadata".to_string());
            args.push(format!("comment={comment}"));
        }

        args
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_promotional_noise_stripping() {
        assert_eq!(
            AudioMetadata::clean_title_noise("Dua Lipa - Levitating [Official Music Video]"),
            "Dua Lipa - Levitating"
        );
        assert_eq!(
            AudioMetadata::clean_title_noise("Adele - Easy On Me (Official Lyric Video) 4K"),
            "Adele - Easy On Me"
        );
        assert_eq!(
            AudioMetadata::clean_title_noise("The Weeknd - Blinding Lights (Audio) [HQ]"),
            "The Weeknd - Blinding Lights"
        );
    }

    #[test]
    fn test_artist_title_splitting() {
        let (title, artist) = AudioMetadata::parse_artist_and_title(
            "Queen - Bohemian Rhapsody [Official Video]",
            Some("Queen Official"),
        );
        assert_eq!(title, "Bohemian Rhapsody");
        assert_eq!(artist.as_deref(), Some("Queen"));

        // Fallback when no hyphen separator
        let (title2, artist2) =
            AudioMetadata::parse_artist_and_title("Bohemian Rhapsody", Some("Queen Official"));
        assert_eq!(title2, "Bohemian Rhapsody");
        assert_eq!(artist2.as_deref(), Some("Queen Official"));
    }

    #[test]
    fn test_sanitize_tag_removes_controls() {
        let dirty = "Clean\0Artist\n\rName\t";
        assert_eq!(AudioMetadata::sanitize_tag(dirty), "CleanArtistName");
    }

    #[test]
    fn test_ffmpeg_args_generation() {
        let meta = AudioMetadata {
            title: "Song".to_string(),
            artist: Some("Band".to_string()),
            album: Some("Record".to_string()),
            album_artist: Some("Band".to_string()),
            date: Some("2022".to_string()),
            genre: Some("Rock".to_string()),
            track_number: Some(1),
            comment: Some("Test".to_string()),
        };

        let mp3_args = meta.to_ffmpeg_args(AudioTargetFormat::Mp3);
        assert!(mp3_args.contains(&"-id3v2_version".to_string()));
        assert!(mp3_args.contains(&"title=Song".to_string()));
        assert!(mp3_args.contains(&"artist=Band".to_string()));
        assert!(mp3_args.contains(&"track=1".to_string()));

        let m4a_args = meta.to_ffmpeg_args(AudioTargetFormat::M4a);
        assert!(!m4a_args.contains(&"-id3v2_version".to_string()));
        assert!(m4a_args.contains(&"title=Song".to_string()));
    }
}
