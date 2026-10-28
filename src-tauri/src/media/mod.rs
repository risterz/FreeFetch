pub mod artwork;
pub mod audio;
pub mod audio_metadata;
pub mod container;
pub mod ffmpeg;
pub mod probe;
pub mod subtitle_converter;
pub mod subtitle_parser;

pub use artwork::ArtworkDownloader;
pub use audio::{AudioConversionOptions, AudioQualityPreset, AudioTargetFormat, BitrateMode};
pub use audio_metadata::AudioMetadata;
pub use container::{
    is_audio_codec_compatible, is_subtitle_codec_compatible, is_video_codec_compatible,
    normalize_audio_codec, normalize_container, normalize_video_codec, validate_stream_combination,
};
pub use ffmpeg::FfmpegController;
pub use probe::ProbeController;
pub use subtitle_converter::{document_to_srt, document_to_vtt, vtt_to_srt};
pub use subtitle_parser::{parse_vtt, SubtitleCue, SubtitleDocument};
