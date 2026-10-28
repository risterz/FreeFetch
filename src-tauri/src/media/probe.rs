use crate::core::error::FreeFetchError;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct ProbeController {
    ffprobe_path: PathBuf,
}

impl ProbeController {
    pub fn discover(custom_path: Option<&str>) -> Result<Self, FreeFetchError> {
        if let Some(cp) = custom_path {
            let p = PathBuf::from(cp);
            if p.is_file() {
                return Ok(Self { ffprobe_path: p });
            }
        }

        let choco_candidate = PathBuf::from("C:\\ProgramData\\chocolatey\\bin\\ffprobe.exe");
        if choco_candidate.is_file() {
            return Ok(Self {
                ffprobe_path: choco_candidate,
            });
        }

        if let Ok(output) = crate::core::hidden_std_command("ffprobe")
            .arg("-version")
            .output()
        {
            if output.status.success() {
                return Ok(Self {
                    ffprobe_path: PathBuf::from("ffprobe"),
                });
            }
        }

        Err(FreeFetchError::FfmpegNotFound)
    }

    pub fn binary_path(&self) -> &Path {
        &self.ffprobe_path
    }

    /// Probes the elementary codec name for the primary stream (e.g. "vp9", "opus", "aac", "h264")
    pub async fn probe_codec(&self, path: &Path) -> Result<String, FreeFetchError> {
        self.probe_codec_for_stream(path, "0").await
    }

    /// Probes the video codec name for the first video stream
    pub async fn probe_video_codec(&self, path: &Path) -> Result<String, FreeFetchError> {
        self.probe_codec_for_stream(path, "v:0").await
    }

    /// Probes the audio codec name for the first audio stream
    pub async fn probe_audio_codec(&self, path: &Path) -> Result<String, FreeFetchError> {
        self.probe_codec_for_stream(path, "a:0").await
    }

    /// Core stream codec probing using ffprobe with specified stream selector
    pub async fn probe_codec_for_stream(
        &self,
        path: &Path,
        stream_specifier: &str,
    ) -> Result<String, FreeFetchError> {
        let mut cmd = crate::core::hidden_tokio_command(&self.ffprobe_path);
        cmd.arg("-v")
            .arg("error")
            .arg("-select_streams")
            .arg(stream_specifier)
            .arg("-show_entries")
            .arg("stream=codec_name")
            .arg("-of")
            .arg("default=noprint_wrappers=1:nokey=1")
            .arg(path);

        let output = cmd
            .output()
            .await
            .map_err(|e| FreeFetchError::Io(e.to_string()))?;
        if output.status.success() {
            let codec = String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_lowercase();
            if codec.is_empty() {
                Err(FreeFetchError::Generic(format!(
                    "No stream found matching specifier '{stream_specifier}'"
                )))
            } else {
                Ok(codec)
            }
        } else {
            let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
            Err(FreeFetchError::Generic(format!("ffprobe failed: {err}")))
        }
    }
}
