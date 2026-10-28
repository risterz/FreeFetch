use crate::core::error::FreeFetchError;
use crate::media::audio::{
    AudioConversionOptions, AudioQualityPreset, AudioTargetFormat, BitrateMode,
};
use crate::media::audio_metadata::AudioMetadata;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;
use tokio::sync::watch;

#[derive(Clone, Debug)]
pub struct FfmpegController {
    ffmpeg_path: PathBuf,
}

impl FfmpegController {
    pub fn discover(custom_path: Option<&str>) -> Result<Self, FreeFetchError> {
        if let Some(cp) = custom_path {
            let p = PathBuf::from(cp);
            if p.is_file() {
                return Ok(Self { ffmpeg_path: p });
            }
        }

        // Check local sidecars directory or next to executable
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let candidate1 = dir.join("ffmpeg.exe");
                let candidate2 = dir
                    .join("sidecars")
                    .join("ffmpeg-x86_64-pc-windows-msvc.exe");
                if candidate1.is_file() {
                    return Ok(Self {
                        ffmpeg_path: candidate1,
                    });
                }
                if candidate2.is_file() {
                    return Ok(Self {
                        ffmpeg_path: candidate2,
                    });
                }
            }
        }

        // Check Chocolatey standard bin
        let choco_candidate = PathBuf::from("C:\\ProgramData\\chocolatey\\bin\\ffmpeg.exe");
        if choco_candidate.is_file() {
            return Ok(Self {
                ffmpeg_path: choco_candidate,
            });
        }

        // Check system PATH
        if let Ok(output) = crate::core::hidden_std_command("ffmpeg")
            .arg("-version")
            .output()
        {
            if output.status.success() {
                return Ok(Self {
                    ffmpeg_path: PathBuf::from("ffmpeg"),
                });
            }
        }

        Err(FreeFetchError::FfmpegNotFound)
    }

    pub fn binary_path(&self) -> &Path {
        &self.ffmpeg_path
    }

    /// Core cancellable executor for FFmpeg commands
    async fn run_cancellable_command(
        mut cmd: Command,
        output_path: &Path,
        mut cancel_rx: Option<watch::Receiver<bool>>,
    ) -> Result<(), FreeFetchError> {
        crate::core::configure_hidden_tokio_command(&mut cmd);
        cmd.kill_on_drop(true);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        // Pre-cancellation check
        if let Some(ref rx) = cancel_rx {
            if *rx.borrow() {
                if output_path.exists() {
                    let _ = tokio::fs::remove_file(output_path).await;
                }
                return Err(FreeFetchError::Cancelled);
            }
        }

        let mut child = cmd.spawn().map_err(|e| FreeFetchError::Io(e.to_string()))?;
        let stderr = child.stderr.take();

        let cancel_fut = async {
            if let Some(ref mut rx) = cancel_rx {
                while rx.changed().await.is_ok() {
                    if *rx.borrow() {
                        return;
                    }
                }
            }
            std::future::pending::<()>().await
        };

        tokio::select! {
            status_res = child.wait() => {
                match status_res {
                    Ok(status) => {
                        if !status.success() {
                            let mut err_bytes = Vec::new();
                            if let Some(mut err_reader) = stderr {
                                use tokio::io::AsyncReadExt;
                                let _ = err_reader.read_to_end(&mut err_bytes).await;
                            }
                            let raw_log = String::from_utf8_lossy(&err_bytes).to_string();
                            let error_lines: Vec<&str> = raw_log
                                .lines()
                                .rev()
                                .filter(|l| {
                                    let t = l.trim();
                                    !t.is_empty()
                                        && !t.starts_with("frame=")
                                        && !t.starts_with("size=")
                                })
                                .take(4)
                                .collect();
                            let log = if !error_lines.is_empty() {
                                let summary = error_lines
                                    .into_iter()
                                    .rev()
                                    .collect::<Vec<_>>()
                                    .join(" | ");
                                format!("{summary}\n{raw_log}")
                            } else {
                                raw_log
                            };
                            if output_path.exists() {
                                let _ = tokio::fs::remove_file(output_path).await;
                            }
                            return Err(FreeFetchError::RemuxFailed {
                                code: status.code().unwrap_or(-1),
                                log,
                            });
                        }
                        Ok(())
                    }
                    Err(e) => {
                        if output_path.exists() {
                            let _ = tokio::fs::remove_file(output_path).await;
                        }
                        Err(FreeFetchError::Io(e.to_string()))
                    }
                }
            }
            _ = cancel_fut => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                if output_path.exists() {
                    let _ = tokio::fs::remove_file(output_path).await;
                }
                Err(FreeFetchError::Cancelled)
            }
        }
    }

    /// Merges separate video and audio elementary streams into a final MP4 or MKV container
    pub async fn merge_streams(
        &self,
        video_path: &Path,
        audio_path: &Path,
        output_path: &Path,
    ) -> Result<(), FreeFetchError> {
        self.merge_streams_cancellable(video_path, audio_path, output_path, None)
            .await
    }

    /// Merges separate video and audio elementary streams with active cancellation support
    pub async fn merge_streams_cancellable(
        &self,
        video_path: &Path,
        audio_path: &Path,
        output_path: &Path,
        cancel_rx: Option<watch::Receiver<bool>>,
    ) -> Result<(), FreeFetchError> {
        let is_webm = output_path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|s| s.eq_ignore_ascii_case("webm"))
            .unwrap_or(false);

        // Pre-validation: probe input elementary streams if WebM container
        if is_webm {
            if let Ok(probe) = crate::media::ProbeController::discover(None) {
                if let Ok(vcodec) = probe.probe_codec(video_path).await {
                    if !crate::media::is_video_codec_compatible("webm", &vcodec) {
                        return Err(FreeFetchError::IncompatibleContainerCodec {
                            container: "webm".to_string(),
                            message: format!(
                                "Video stream codec '{vcodec}' is not supported in WebM container. WebM requires VP8, VP9, or AV1."
                            ),
                        });
                    }
                }
                if let Ok(acodec) = probe.probe_codec(audio_path).await {
                    if !crate::media::is_audio_codec_compatible("webm", &acodec) {
                        return Err(FreeFetchError::IncompatibleContainerCodec {
                            container: "webm".to_string(),
                            message: format!(
                                "Audio stream codec '{acodec}' is not supported in WebM container. WebM requires Opus or Vorbis."
                            ),
                        });
                    }
                }
            }
        }

        let mut cmd = crate::core::hidden_tokio_command(&self.ffmpeg_path);
        cmd.arg("-y")
            .arg("-i")
            .arg(video_path)
            .arg("-i")
            .arg(audio_path)
            .arg("-c")
            .arg("copy")
            .arg("-map")
            .arg("0:v:0")
            .arg("-map")
            .arg("1:a:0");

        let is_mov_mp4 = output_path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|s| {
                s.eq_ignore_ascii_case("mp4")
                    || s.eq_ignore_ascii_case("m4a")
                    || s.eq_ignore_ascii_case("mov")
            })
            .unwrap_or(false);

        if is_mov_mp4 {
            cmd.arg("-movflags").arg("+faststart");
        }

        cmd.arg(output_path);

        match Self::run_cancellable_command(cmd, output_path, cancel_rx).await {
            Err(FreeFetchError::RemuxFailed { code, log })
                if log.contains("Only VP8 or VP9 or AV1") || log.contains("Vorbis or Opus") =>
            {
                Err(FreeFetchError::IncompatibleContainerCodec {
                    container: "webm".to_string(),
                    message: format!(
                        "Incompatible codec detected for WebM container (exit code {code}). FFmpeg requires VP8/VP9/AV1 video and Opus/Vorbis audio."
                    ),
                })
            }
            res => res,
        }
    }

    /// Remuxes a single elementary file to another container with lossless stream copying
    pub async fn remux_to_container(
        &self,
        input_path: &Path,
        output_path: &Path,
    ) -> Result<(), FreeFetchError> {
        self.remux_to_container_cancellable(input_path, output_path, None)
            .await
    }

    /// Remuxes a single elementary file to another container with active cancellation support
    pub async fn remux_to_container_cancellable(
        &self,
        input_path: &Path,
        output_path: &Path,
        cancel_rx: Option<watch::Receiver<bool>>,
    ) -> Result<(), FreeFetchError> {
        let mut cmd = crate::core::hidden_tokio_command(&self.ffmpeg_path);
        cmd.arg("-y")
            .arg("-i")
            .arg(input_path)
            .arg("-c")
            .arg("copy");

        let is_mov_mp4 = output_path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|s| {
                s.eq_ignore_ascii_case("mp4")
                    || s.eq_ignore_ascii_case("m4a")
                    || s.eq_ignore_ascii_case("mov")
            })
            .unwrap_or(false);

        if is_mov_mp4 {
            cmd.arg("-movflags").arg("+faststart");
        }

        cmd.arg(output_path);

        Self::run_cancellable_command(cmd, output_path, cancel_rx).await
    }

    /// Extracts and transcodes audio to MP3 using libmp3lame
    pub async fn extract_to_mp3(
        &self,
        input_path: &Path,
        output_path: &Path,
    ) -> Result<(), FreeFetchError> {
        self.extract_to_mp3_cancellable(input_path, output_path, None)
            .await
    }

    /// Extracts and transcodes audio to MP3 with active cancellation support
    pub async fn extract_to_mp3_cancellable(
        &self,
        input_path: &Path,
        output_path: &Path,
        cancel_rx: Option<watch::Receiver<bool>>,
    ) -> Result<(), FreeFetchError> {
        let mut cmd = crate::core::hidden_tokio_command(&self.ffmpeg_path);
        cmd.arg("-y")
            .arg("-i")
            .arg(input_path)
            .arg("-vn")
            .arg("-c:a")
            .arg("libmp3lame")
            .arg("-q:a")
            .arg("2")
            .arg(output_path);

        Self::run_cancellable_command(cmd, output_path, cancel_rx).await
    }

    /// Converts raw audio input to target format (MP3, M4A, FLAC, Opus) with metadata and artwork
    pub async fn convert_audio(
        &self,
        input_path: &Path,
        output_path: &Path,
        options: &AudioConversionOptions,
        metadata: Option<&AudioMetadata>,
        artwork_path: Option<&Path>,
    ) -> Result<(), FreeFetchError> {
        self.convert_audio_cancellable(
            input_path,
            output_path,
            options,
            metadata,
            artwork_path,
            None,
        )
        .await
    }

    /// Converts raw audio input to target format with cancellation support
    pub async fn convert_audio_cancellable(
        &self,
        input_path: &Path,
        output_path: &Path,
        options: &AudioConversionOptions,
        metadata: Option<&AudioMetadata>,
        artwork_path: Option<&Path>,
        cancel_rx: Option<watch::Receiver<bool>>,
    ) -> Result<(), FreeFetchError> {
        let mut cmd = crate::core::hidden_tokio_command(&self.ffmpeg_path);
        cmd.arg("-y").arg("-i").arg(input_path);

        let has_artwork = artwork_path
            .map(|p| {
                p.exists() && options.embed_artwork && options.target_format.supports_artwork()
            })
            .unwrap_or(false);

        if has_artwork {
            if let Some(art) = artwork_path {
                cmd.arg("-i").arg(art);
            }
        }

        // Map audio stream from first input
        cmd.arg("-map").arg("0:a:0");

        // Codec and Bitrate configuration based on target_format
        match options.target_format {
            AudioTargetFormat::Mp3 => {
                cmd.arg("-c:a").arg("libmp3lame");
                match options.bitrate_mode {
                    BitrateMode::Vbr => {
                        let q = match options.quality_preset {
                            AudioQualityPreset::Best => "0",
                            AudioQualityPreset::High => "2",
                            AudioQualityPreset::Medium => "4",
                            AudioQualityPreset::Low => "6",
                            AudioQualityPreset::Custom => "2",
                        };
                        cmd.arg("-q:a").arg(q);
                    }
                    BitrateMode::Cbr => {
                        let kbps = options
                            .quality_preset
                            .resolve_bitrate_kbps(
                                AudioTargetFormat::Mp3,
                                options.custom_bitrate_kbps,
                            )
                            .unwrap_or(320);
                        cmd.arg("-b:a").arg(format!("{kbps}k"));
                    }
                }
            }
            AudioTargetFormat::M4a => {
                cmd.arg("-c:a").arg("aac");
                let kbps = options
                    .quality_preset
                    .resolve_bitrate_kbps(AudioTargetFormat::M4a, options.custom_bitrate_kbps)
                    .unwrap_or(256);
                cmd.arg("-b:a").arg(format!("{kbps}k"));
                cmd.arg("-movflags").arg("+faststart");
            }
            AudioTargetFormat::Flac => {
                cmd.arg("-c:a").arg("flac");
                cmd.arg("-compression_level").arg("5");
            }
            AudioTargetFormat::Opus => {
                cmd.arg("-c:a").arg("libopus");
                let kbps = options
                    .quality_preset
                    .resolve_bitrate_kbps(AudioTargetFormat::Opus, options.custom_bitrate_kbps)
                    .unwrap_or(128);
                cmd.arg("-b:a").arg(format!("{kbps}k"));
                cmd.arg("-vbr").arg("on");
            }
        }

        // Sample rate override if provided
        if let Some(rate) = options.sample_rate {
            cmd.arg("-ar").arg(rate.to_string());
        }

        // Map and format artwork stream
        if has_artwork {
            match options.target_format {
                AudioTargetFormat::Mp3 => {
                    cmd.arg("-map").arg("1:v:0");
                    cmd.arg("-c:v:0").arg("mjpeg");
                    cmd.arg("-disposition:v:0").arg("attached_pic");
                    cmd.arg("-metadata:s:v").arg("comment=Cover (front)");
                }
                AudioTargetFormat::M4a => {
                    cmd.arg("-map").arg("1:v:0");
                    cmd.arg("-c:v:0").arg("copy");
                    cmd.arg("-disposition:v:0").arg("attached_pic");
                }
                AudioTargetFormat::Flac => {
                    cmd.arg("-map").arg("1:v:0");
                    cmd.arg("-c:v:0").arg("copy");
                    cmd.arg("-disposition:v:0").arg("attached_pic");
                }
                AudioTargetFormat::Opus => {
                    // Ogg Opus does not support attached video pictures directly
                }
            }
        }

        // Embed metadata tags
        if options.embed_metadata {
            if let Some(meta) = metadata {
                for arg in meta.to_ffmpeg_args(options.target_format) {
                    cmd.arg(arg);
                }
            }
        }

        cmd.arg(output_path);

        Self::run_cancellable_command(cmd, output_path, cancel_rx).await
    }

    /// Embeds subtitle files as native subtitle streams into MP4 or MKV containers
    pub async fn embed_subtitles(
        &self,
        video_input: &Path,
        subtitle_tracks: &[(&Path, &str, &str)],
        output_path: &Path,
    ) -> Result<(), FreeFetchError> {
        self.embed_subtitles_cancellable(video_input, subtitle_tracks, output_path, None)
            .await
    }

    /// Embeds subtitle files as native subtitle streams with active cancellation support
    pub async fn embed_subtitles_cancellable(
        &self,
        video_input: &Path,
        subtitle_tracks: &[(&Path, &str, &str)],
        output_path: &Path,
        cancel_rx: Option<watch::Receiver<bool>>,
    ) -> Result<(), FreeFetchError> {
        let mut cmd = crate::core::hidden_tokio_command(&self.ffmpeg_path);
        cmd.arg("-y").arg("-i").arg(video_input);

        for (sub_path, _, _) in subtitle_tracks {
            cmd.arg("-i").arg(sub_path);
        }

        // Map video and audio from input 0
        cmd.arg("-map").arg("0:v");
        cmd.arg("-map").arg("0:a?");

        // Map subtitle inputs starting from index 1
        for (idx, _) in subtitle_tracks.iter().enumerate() {
            cmd.arg("-map").arg(format!("{}:s", idx + 1));
        }

        // Copy video and audio codecs
        cmd.arg("-c:v").arg("copy");
        cmd.arg("-c:a").arg("copy");

        let out_ext = output_path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("mp4")
            .to_lowercase();

        let ext = if out_ext == "tmp" || out_ext == "part" {
            video_input
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("mp4")
                .to_lowercase()
        } else {
            out_ext
        };

        if ext == "mp4" || ext == "m4v" || ext == "mov" {
            // MP4 uses mov_text codec for subtitles
            cmd.arg("-c:s").arg("mov_text");
            cmd.arg("-movflags").arg("+faststart");
            cmd.arg("-f").arg("mp4");
        } else if ext == "mkv" {
            // MKV uses srt
            cmd.arg("-c:s").arg("srt");
            cmd.arg("-f").arg("matroska");
        } else if ext == "webm" {
            cmd.arg("-c:s").arg("webvtt");
            cmd.arg("-f").arg("webm");
        } else {
            cmd.arg("-c:s").arg("srt");
        }

        // Metadata tags
        for (idx, (_, lang_code, title)) in subtitle_tracks.iter().enumerate() {
            let lower = lang_code.to_lowercase();
            let iso3 = match lower.as_str() {
                "en" => "eng",
                "de" => "deu",
                "es" => "spa",
                "fr" => "fra",
                "it" => "ita",
                "ja" => "jpn",
                "ko" => "kor",
                "zh" => "zho",
                "ru" => "rus",
                "pt" => "por",
                "ms" => "msa",
                "id" => "ind",
                "ar" => "ara",
                "hi" => "hin",
                other => other,
            };
            cmd.arg(format!("-metadata:s:s:{idx}"))
                .arg(format!("language={iso3}"));
            cmd.arg(format!("-metadata:s:s:{idx}"))
                .arg(format!("title={title}"));
            if ext == "mp4" {
                cmd.arg(format!("-metadata:s:s:{idx}"))
                    .arg(format!("handler_name={title}"));
            }
        }

        cmd.arg(output_path);

        Self::run_cancellable_command(cmd, output_path, cancel_rx).await
    }

    /// Safely escapes a path for FFmpeg's `subtitles` filter option string on all platforms.
    ///
    /// FFmpeg filtergraph syntax parses special characters (`\`, `:`, `'`, `[`, `]`, `,`, `;`).
    /// This function normalizes separators to forward slashes, escapes drive colons,
    /// escapes single quotes via double-escaping (`'\\\'`), and wraps the path in single quotes.
    pub fn escape_subtitle_filter_path(path: &Path) -> String {
        let p_str = path.to_string_lossy();
        let normalized = p_str.replace('\\', "/");
        let escaped_colons = normalized.replace(':', r"\:");
        let escaped_quotes = escaped_colons.replace('\'', r"'\\\''");
        format!("subtitles='{escaped_quotes}'")
    }

    /// Hardcodes (burns) a subtitle file into video frames using FFmpeg's subtitles filter
    pub async fn hardcode_subtitles(
        &self,
        video_input: &Path,
        subtitle_input: &Path,
        output_path: &Path,
    ) -> Result<(), FreeFetchError> {
        self.hardcode_subtitles_cancellable(video_input, subtitle_input, output_path, None)
            .await
    }

    /// Hardcodes (burns) a subtitle file into video frames with active cancellation support
    pub async fn hardcode_subtitles_cancellable(
        &self,
        video_input: &Path,
        subtitle_input: &Path,
        output_path: &Path,
        cancel_rx: Option<watch::Receiver<bool>>,
    ) -> Result<(), FreeFetchError> {
        let mut cmd = crate::core::hidden_tokio_command(&self.ffmpeg_path);
        let filter_arg = Self::escape_subtitle_filter_path(subtitle_input);

        let out_ext = output_path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("mp4")
            .to_lowercase();

        let ext = if out_ext == "tmp" || out_ext == "part" {
            video_input
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("mp4")
                .to_lowercase()
        } else {
            out_ext
        };

        cmd.arg("-y")
            .arg("-i")
            .arg(video_input)
            .arg("-vf")
            .arg(filter_arg)
            .arg("-map")
            .arg("0:v:0")
            .arg("-map")
            .arg("0:a?");

        if ext == "webm" {
            cmd.arg("-c:v")
                .arg("libvpx-vp9")
                .arg("-crf")
                .arg("30")
                .arg("-b:v")
                .arg("0")
                .arg("-c:a")
                .arg("copy")
                .arg("-f")
                .arg("webm");
        } else {
            cmd.arg("-c:v")
                .arg("libx264")
                .arg("-crf")
                .arg("20")
                .arg("-preset")
                .arg("medium")
                .arg("-c:a")
                .arg("copy");

            if ext == "mp4" || ext == "m4v" || ext == "mov" {
                cmd.arg("-movflags").arg("+faststart").arg("-f").arg("mp4");
            } else if ext == "mkv" {
                cmd.arg("-f").arg("matroska");
            }
        }

        cmd.arg(output_path);

        Self::run_cancellable_command(cmd, output_path, cancel_rx).await
    }
}
