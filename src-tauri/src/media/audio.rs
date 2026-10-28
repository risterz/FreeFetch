use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AudioTargetFormat {
    #[default]
    Mp3,
    M4a,
    Flac,
    Opus,
}

impl AudioTargetFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            AudioTargetFormat::Mp3 => "mp3",
            AudioTargetFormat::M4a => "m4a",
            AudioTargetFormat::Flac => "flac",
            AudioTargetFormat::Opus => "opus",
        }
    }

    pub fn default_codec(&self) -> &'static str {
        match self {
            AudioTargetFormat::Mp3 => "libmp3lame",
            AudioTargetFormat::M4a => "aac",
            AudioTargetFormat::Flac => "flac",
            AudioTargetFormat::Opus => "libopus",
        }
    }

    pub fn supports_artwork(&self) -> bool {
        match self {
            AudioTargetFormat::Mp3 | AudioTargetFormat::M4a | AudioTargetFormat::Flac => true,
            AudioTargetFormat::Opus => false,
        }
    }

    pub fn from_str_loose(s: &str) -> Option<Self> {
        let clean = s.trim().to_lowercase();
        match clean.as_str() {
            "mp3" | "audio/mp3" | "audio/mpeg" => Some(AudioTargetFormat::Mp3),
            "m4a" | "aac" | "audio/m4a" | "audio/aac" | "audio/mp4" => Some(AudioTargetFormat::M4a),
            "flac" | "audio/flac" => Some(AudioTargetFormat::Flac),
            "opus" | "ogg" | "audio/opus" | "audio/ogg" => Some(AudioTargetFormat::Opus),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AudioQualityPreset {
    #[default]
    Best,
    High,
    Medium,
    Low,
    Custom,
}

impl AudioQualityPreset {
    pub fn resolve_bitrate_kbps(
        &self,
        format: AudioTargetFormat,
        custom: Option<u32>,
    ) -> Option<u32> {
        if format == AudioTargetFormat::Flac {
            return None;
        }

        match self {
            AudioQualityPreset::Custom => custom.or(Some(match format {
                AudioTargetFormat::Mp3 => 320,
                AudioTargetFormat::M4a => 256,
                AudioTargetFormat::Opus => 128,
                AudioTargetFormat::Flac => 0,
            })),
            AudioQualityPreset::Best => Some(match format {
                AudioTargetFormat::Mp3 => 320,
                AudioTargetFormat::M4a => 256,
                AudioTargetFormat::Opus => 160,
                AudioTargetFormat::Flac => 0,
            }),
            AudioQualityPreset::High => Some(match format {
                AudioTargetFormat::Mp3 => 256,
                AudioTargetFormat::M4a => 192,
                AudioTargetFormat::Opus => 128,
                AudioTargetFormat::Flac => 0,
            }),
            AudioQualityPreset::Medium => Some(match format {
                AudioTargetFormat::Mp3 => 192,
                AudioTargetFormat::M4a => 160,
                AudioTargetFormat::Opus => 96,
                AudioTargetFormat::Flac => 0,
            }),
            AudioQualityPreset::Low => Some(match format {
                AudioTargetFormat::Mp3 => 128,
                AudioTargetFormat::M4a => 128,
                AudioTargetFormat::Opus => 64,
                AudioTargetFormat::Flac => 0,
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum BitrateMode {
    #[default]
    Cbr,
    Vbr,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioConversionOptions {
    #[serde(default)]
    pub target_format: AudioTargetFormat,
    #[serde(default)]
    pub quality_preset: AudioQualityPreset,
    #[serde(default)]
    pub bitrate_mode: BitrateMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_bitrate_kbps: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<u32>,
    #[serde(default = "default_true")]
    pub embed_metadata: bool,
    #[serde(default = "default_true")]
    pub embed_artwork: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename_template: Option<String>,
}

fn default_true() -> bool {
    true
}

impl Default for AudioConversionOptions {
    fn default() -> Self {
        Self {
            target_format: AudioTargetFormat::Mp3,
            quality_preset: AudioQualityPreset::Best,
            bitrate_mode: BitrateMode::Cbr,
            custom_bitrate_kbps: None,
            sample_rate: None,
            embed_metadata: true,
            embed_artwork: true,
            filename_template: None,
        }
    }
}

impl AudioConversionOptions {
    pub fn validate(&self) -> Result<(), String> {
        if self.target_format == AudioTargetFormat::Flac && self.custom_bitrate_kbps.is_some() {
            return Err("Bitrate cannot be configured for lossless FLAC encoding".to_string());
        }

        if let Some(rate) = self.sample_rate {
            if !(8000..=192000).contains(&rate) {
                return Err(format!(
                    "Sample rate {rate}Hz is outside supported range (8000-192000Hz)"
                ));
            }
        }

        if let Some(kbps) = self.custom_bitrate_kbps {
            if !(32..=512).contains(&kbps) {
                return Err(format!(
                    "Custom bitrate {kbps}kbps is outside safe range (32-512kbps)"
                ));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_format_extensions_and_codecs() {
        assert_eq!(AudioTargetFormat::Mp3.extension(), "mp3");
        assert_eq!(AudioTargetFormat::M4a.extension(), "m4a");
        assert_eq!(AudioTargetFormat::Flac.extension(), "flac");
        assert_eq!(AudioTargetFormat::Opus.extension(), "opus");

        assert_eq!(AudioTargetFormat::Mp3.default_codec(), "libmp3lame");
        assert_eq!(AudioTargetFormat::M4a.default_codec(), "aac");
        assert_eq!(AudioTargetFormat::Flac.default_codec(), "flac");
        assert_eq!(AudioTargetFormat::Opus.default_codec(), "libopus");

        assert!(AudioTargetFormat::Mp3.supports_artwork());
        assert!(AudioTargetFormat::M4a.supports_artwork());
        assert!(AudioTargetFormat::Flac.supports_artwork());
        assert!(!AudioTargetFormat::Opus.supports_artwork());
    }

    #[test]
    fn test_audio_format_parsing() {
        assert_eq!(
            AudioTargetFormat::from_str_loose("mp3"),
            Some(AudioTargetFormat::Mp3)
        );
        assert_eq!(
            AudioTargetFormat::from_str_loose("M4A"),
            Some(AudioTargetFormat::M4a)
        );
        assert_eq!(
            AudioTargetFormat::from_str_loose("aac"),
            Some(AudioTargetFormat::M4a)
        );
        assert_eq!(
            AudioTargetFormat::from_str_loose("flac"),
            Some(AudioTargetFormat::Flac)
        );
        assert_eq!(
            AudioTargetFormat::from_str_loose("opus"),
            Some(AudioTargetFormat::Opus)
        );
        assert_eq!(AudioTargetFormat::from_str_loose("invalid"), None);
    }

    #[test]
    fn test_quality_preset_bitrate_resolution() {
        let best_mp3 = AudioQualityPreset::Best.resolve_bitrate_kbps(AudioTargetFormat::Mp3, None);
        assert_eq!(best_mp3, Some(320));

        let high_mp3 = AudioQualityPreset::High.resolve_bitrate_kbps(AudioTargetFormat::Mp3, None);
        assert_eq!(high_mp3, Some(256));

        let best_m4a = AudioQualityPreset::Best.resolve_bitrate_kbps(AudioTargetFormat::M4a, None);
        assert_eq!(best_m4a, Some(256));

        let best_opus =
            AudioQualityPreset::Best.resolve_bitrate_kbps(AudioTargetFormat::Opus, None);
        assert_eq!(best_opus, Some(160));

        let flac_res = AudioQualityPreset::Best.resolve_bitrate_kbps(AudioTargetFormat::Flac, None);
        assert_eq!(flac_res, None);

        let custom_res =
            AudioQualityPreset::Custom.resolve_bitrate_kbps(AudioTargetFormat::Mp3, Some(224));
        assert_eq!(custom_res, Some(224));
    }

    #[test]
    fn test_serialization_and_defaults() {
        let default_opts = AudioConversionOptions::default();
        let json = serde_json::to_string(&default_opts).unwrap();
        let deserialized: AudioConversionOptions = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.target_format, AudioTargetFormat::Mp3);
        assert_eq!(deserialized.quality_preset, AudioQualityPreset::Best);
        assert!(deserialized.embed_metadata);
        assert!(deserialized.embed_artwork);
    }

    #[test]
    fn test_validation_logic() {
        let valid = AudioConversionOptions::default();
        assert!(valid.validate().is_ok());

        let invalid_flac = AudioConversionOptions {
            target_format: AudioTargetFormat::Flac,
            custom_bitrate_kbps: Some(320),
            ..AudioConversionOptions::default()
        };
        assert!(invalid_flac.validate().is_err());

        let invalid_rate = AudioConversionOptions {
            sample_rate: Some(4000),
            ..AudioConversionOptions::default()
        };
        assert!(invalid_rate.validate().is_err());

        let invalid_bitrate = AudioConversionOptions {
            custom_bitrate_kbps: Some(10),
            ..AudioConversionOptions::default()
        };
        assert!(invalid_bitrate.validate().is_err());
    }
}
