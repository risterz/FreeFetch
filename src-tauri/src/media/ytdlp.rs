use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::watch;

use crate::core::error::FreeFetchError;
use crate::core::security::sanitize_sensitive_text;

#[derive(Deserialize, Debug, Clone)]
#[allow(dead_code)]
pub struct YtDlpDump {
    pub id: String,
    pub title: String,
    pub uploader: Option<String>,
    pub channel: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub track: Option<String>,
    pub release_date: Option<String>,
    pub release_year: Option<u32>,
    pub duration: Option<f64>,
    pub thumbnail: Option<String>,
    pub description: Option<String>,
    pub upload_date: Option<String>,
    pub language: Option<String>,
    pub formats: Option<Vec<YtDlpFormat>>,
    pub subtitles: Option<HashMap<String, Vec<YtDlpSubtitleTrack>>>,
    pub automatic_captions: Option<HashMap<String, Vec<YtDlpSubtitleTrack>>>,
    pub http_headers: Option<HashMap<String, String>>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct YtDlpSubtitleTrack {
    pub ext: Option<String>,
    pub url: Option<String>,
    pub name: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct YtDlpFormat {
    pub format_id: String,
    pub url: Option<String>,
    pub ext: Option<String>,
    pub protocol: Option<String>,
    pub vcodec: Option<String>,
    pub acodec: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<f64>,
    pub tbr: Option<f64>,
    pub vbr: Option<f64>,
    pub abr: Option<f64>,
    pub filesize: Option<u64>,
    pub filesize_approx: Option<u64>,
    pub format_note: Option<String>,
    pub resolution: Option<String>,
    pub http_headers: Option<HashMap<String, String>>,
    pub cookies: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct YtDlpDiagnostics {
    pub executable_path: String,
    pub version: String,
    pub has_impersonation: bool,
    pub available_impersonate_targets: Vec<String>,
    pub verbose_log: Option<String>,
    pub status: String,
    pub recommendations: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct YtDlpController {
    binary_path: PathBuf,
}

fn verify_ytdlp_path(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    match crate::core::hidden_std_command(path)
        .arg("--version")
        .output()
    {
        Ok(output) => output.status.success() && !output.stdout.is_empty(),
        Err(_) => false,
    }
}

impl YtDlpController {
    pub fn discover(custom_path: Option<&str>) -> Result<Self, FreeFetchError> {
        if let Some(cp) = custom_path {
            let p = PathBuf::from(cp);
            if verify_ytdlp_path(&p) {
                return Ok(Self { binary_path: p });
            }
        }

        // 1. Check local sidecars directory or next to running executable
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let candidates = [
                    dir.join("yt-dlp.exe"),
                    dir.join("sidecars").join("yt-dlp.exe"),
                    dir.join("sidecars")
                        .join("yt-dlp-x86_64-pc-windows-msvc.exe"),
                    dir.join("resources").join("sidecars").join("yt-dlp.exe"),
                    dir.join("resources").join("yt-dlp.exe"),
                    dir.join("yt-dlp"),
                ];
                for candidate in &candidates {
                    if verify_ytdlp_path(candidate) {
                        return Ok(Self {
                            binary_path: candidate.clone(),
                        });
                    }
                }
            }
        }

        // 2. Check current working directory or relative sidecars
        let rel_candidates = [
            PathBuf::from("sidecars").join("yt-dlp.exe"),
            PathBuf::from("src-tauri")
                .join("sidecars")
                .join("yt-dlp.exe"),
        ];
        for candidate in &rel_candidates {
            if verify_ytdlp_path(candidate) {
                if let Ok(abs) = candidate.canonicalize() {
                    return Ok(Self { binary_path: abs });
                }
                return Ok(Self {
                    binary_path: candidate.clone(),
                });
            }
        }

        // 3. Check common Windows package manager paths
        let common_paths = [
            "C:\\ProgramData\\chocolatey\\bin\\yt-dlp.exe",
            "C:\\Users\\Default\\AppData\\Local\\Microsoft\\WinGet\\Links\\yt-dlp.exe",
        ];
        for path_str in &common_paths {
            let p = PathBuf::from(path_str);
            if verify_ytdlp_path(&p) {
                return Ok(Self { binary_path: p });
            }
        }

        // 4. Check user local bin / scoop / winget / Python
        if let Some(userprofile) = std::env::var_os("USERPROFILE") {
            let user_dir = PathBuf::from(userprofile);
            let user_candidates = [
                user_dir.join("scoop").join("shims").join("yt-dlp.exe"),
                user_dir
                    .join("AppData")
                    .join("Local")
                    .join("Microsoft")
                    .join("WinGet")
                    .join("Links")
                    .join("yt-dlp.exe"),
                user_dir
                    .join("AppData")
                    .join("Local")
                    .join("Programs")
                    .join("Python")
                    .join("Python313")
                    .join("Scripts")
                    .join("yt-dlp.exe"),
            ];
            for candidate in &user_candidates {
                if verify_ytdlp_path(candidate) {
                    return Ok(Self {
                        binary_path: candidate.clone(),
                    });
                }
            }
        }

        // 5. Check system PATH and resolve to canonical absolute path
        if let Ok(output) = crate::core::hidden_std_command("yt-dlp")
            .arg("--version")
            .output()
        {
            if output.status.success() {
                let resolved = crate::core::resolve_executable_in_path("yt-dlp")
                    .unwrap_or_else(|| PathBuf::from("yt-dlp"));
                return Ok(Self {
                    binary_path: resolved,
                });
            }
        }

        Err(FreeFetchError::NotFound(
            "yt-dlp executable could not be found or executed. Please install yt-dlp (e.g. 'winget install yt-dlp'), place yt-dlp.exe in the application folder, or configure its location in Settings > Advanced.".to_string(),
        ))
    }

    pub fn binary_path(&self) -> &Path {
        &self.binary_path
    }

    pub fn get_version(&self) -> Result<String, FreeFetchError> {
        let output = crate::core::hidden_std_command(&self.binary_path)
            .arg("--version")
            .output()
            .map_err(|e| FreeFetchError::Io(e.to_string()))?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(FreeFetchError::Generic(format!(
                "yt-dlp --version failed: {err}"
            )));
        }

        let ver = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(ver)
    }

    /// Checks available impersonation targets from yt-dlp (e.g. curl_cffi for browser TLS).
    pub fn check_impersonation(&self) -> (bool, Vec<String>) {
        let mut available = Vec::new();
        if let Ok(out) = crate::core::hidden_std_command(&self.binary_path)
            .arg("--list-impersonate-targets")
            .output()
        {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout);
                for line in text.lines() {
                    let trimmed = line.trim();
                    if trimmed.contains("curl_cffi") && !trimmed.contains("(unavailable)") {
                        if let Some(client) = trimmed.split_whitespace().next() {
                            if !client.is_empty()
                                && client != "Client"
                                && !available.contains(&client.to_string())
                            {
                                available.push(client.to_string());
                            }
                        }
                    }
                }
            }
        }
        let has = !available.is_empty();
        (has, available)
    }

    /// Runs comprehensive diagnostics and verbose extraction without exposing sensitive tokens/cookies.
    pub async fn run_diagnostics(
        &self,
        target_url: Option<&str>,
    ) -> Result<YtDlpDiagnostics, FreeFetchError> {
        let version = self.get_version().unwrap_or_else(|e| format!("Error: {e}"));
        let (has_impersonation, targets) = self.check_impersonation();

        let mut recommendations = Vec::new();
        if !has_impersonation {
            recommendations.push(
                "Browser impersonation (curl_cffi) is not available. Platforms with anti-bot verification (such as TikTok) will reject requests with 'Unexpected response from webpage request'. Fix by installing curl-cffi (e.g. pip install \"curl-cffi<=0.16.0\") or configuring a yt-dlp binary with impersonation support."
                    .to_string(),
            );
        }

        let mut verbose_log = None;
        let mut overall_status = if has_impersonation {
            "Ready with Impersonation"
        } else {
            "Ready (Missing Impersonation)"
        }
        .to_string();

        if let Some(url) = target_url {
            let mut cmd = crate::core::hidden_tokio_command(&self.binary_path);
            cmd.arg("-v")
                .arg("--dump-json")
                .arg("--no-download")
                .arg("--skip-download")
                .arg("--no-call-home")
                .arg(url)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());

            if let Ok(out) = cmd.output().await {
                let stderr = String::from_utf8_lossy(&out.stderr);
                let stdout = String::from_utf8_lossy(&out.stdout);

                let summary = truncate_and_sanitize_stdout(&stdout);
                let combined = format!("=== STDERR ===\n{}\n\n=== STDOUT ===\n{}", stderr, summary);
                let sanitized = sanitize_sensitive_text(&combined);
                verbose_log = Some(sanitized);

                if out.status.success() {
                    overall_status = "Extraction Succeeded".to_string();
                } else {
                    overall_status = "Extraction Failed".to_string();
                    if stderr.contains("Unexpected response from webpage request") {
                        recommendations.push(
                            "TikTok returned an anti-bot challenge page. Ensure yt-dlp is updated and curl-cffi<=0.16.0 is installed."
                                .to_string(),
                        );
                    }
                }
            }
        }

        Ok(YtDlpDiagnostics {
            executable_path: self.binary_path.to_string_lossy().to_string(),
            version,
            has_impersonation,
            available_impersonate_targets: targets,
            verbose_log,
            status: overall_status,
            recommendations,
        })
    }

    /// Triggers yt-dlp self-update or informs how to upgrade.
    pub async fn update(&self) -> Result<String, FreeFetchError> {
        let mut cmd = crate::core::hidden_tokio_command(&self.binary_path);
        cmd.arg("-U").stdout(Stdio::piped()).stderr(Stdio::piped());

        let out = cmd
            .output()
            .await
            .map_err(|e| FreeFetchError::Io(e.to_string()))?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        let raw = format!("{stdout}\n{stderr}");
        let sanitized = sanitize_sensitive_text(&raw);

        if out.status.success() {
            return Ok(sanitized.trim().to_string());
        }

        if raw.contains("pip") {
            let pip_cmd = crate::core::hidden_tokio_command("python")
                .arg("-m")
                .arg("pip")
                .arg("install")
                .arg("--upgrade")
                .arg("yt-dlp")
                .arg("curl-cffi<=0.16.0")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .await;

            if let Ok(p_out) = pip_cmd {
                let p_stdout = String::from_utf8_lossy(&p_out.stdout);
                let p_stderr = String::from_utf8_lossy(&p_out.stderr);
                let p_combined = format!("{p_stdout}\n{p_stderr}");
                return Ok(sanitize_sensitive_text(p_combined.trim()));
            }
        }

        Ok(sanitized.trim().to_string())
    }

    pub async fn extract_json(
        &self,
        url: &str,
        proxy: Option<&str>,
    ) -> Result<YtDlpDump, FreeFetchError> {
        let mut cmd = crate::core::hidden_tokio_command(&self.binary_path);
        cmd.arg("--dump-json")
            .arg("--no-download")
            .arg("--no-warnings")
            .arg("--skip-download")
            .arg("--no-call-home");

        if let Some(p) = proxy {
            if !p.trim().is_empty() {
                cmd.arg("--proxy").arg(p.trim());
            }
        }

        cmd.arg(url).stdout(Stdio::piped()).stderr(Stdio::piped());

        let output = cmd
            .output()
            .await
            .map_err(|e| FreeFetchError::Io(format!("Failed to execute yt-dlp: {e}")))?;

        if !output.status.success() {
            let raw_err = String::from_utf8_lossy(&output.stderr).to_string();
            let sanitized = sanitize_sensitive_text(&raw_err);

            // Handle TikTok and anti-bot challenge errors with clear actionable guidance
            if sanitized.contains("Unexpected response from webpage request")
                || sanitized
                    .contains("attempting impersonation, but no impersonate target is available")
            {
                let (has_imp, _) = self.check_impersonation();
                let help_msg = if !has_imp {
                    "TikTok blocked the extraction request because browser impersonation (curl_cffi) is not installed. Please run `pip install \"curl-cffi<=0.16.0\"` or configure a yt-dlp binary with impersonation support in Settings > Advanced.".to_string()
                } else {
                    "TikTok anti-bot verification blocked the request. Please update yt-dlp in Settings > Advanced or try again shortly.".to_string()
                };
                return Err(FreeFetchError::Extractor {
                    service: "tiktok".to_string(),
                    message: help_msg,
                });
            }

            let cleaned = if let Some(line) = sanitized
                .lines()
                .find(|l| l.contains("ERROR:") || l.contains("Sign in"))
            {
                line.trim().to_string()
            } else {
                sanitized.trim().to_string()
            };

            return Err(FreeFetchError::Extractor {
                service: "yt-dlp".to_string(),
                message: cleaned,
            });
        }

        let dump: YtDlpDump =
            serde_json::from_slice(&output.stdout).map_err(|e| FreeFetchError::Extractor {
                service: "yt-dlp".to_string(),
                message: format!("Failed to parse metadata JSON: {e}"),
            })?;

        Ok(dump)
    }

    /// Downloads a media stream using yt-dlp with realtime progress reporting and cancellation.
    pub async fn download_with_progress<F>(
        &self,
        url: &str,
        format_selector: Option<&str>,
        dest: &Path,
        proxy: Option<&str>,
        mut cancel_rx: Option<watch::Receiver<bool>>,
        on_progress: F,
    ) -> Result<(), FreeFetchError>
    where
        F: Fn(f64, u64, u64, u64) + Send + Sync + 'static,
    {
        let mut cmd = crate::core::hidden_tokio_command(&self.binary_path);
        cmd.arg("--newline").arg("--no-warnings");

        if let Some(fmt) = format_selector {
            let trimmed = fmt.trim();
            if !trimmed.is_empty() {
                if trimmed == "best" {
                    cmd.arg("-f").arg("bestvideo+bestaudio/best");
                } else {
                    cmd.arg("-f")
                        .arg(format!("{trimmed}/bestvideo+bestaudio/best"));
                }
            }
        }

        if let Some(p) = proxy {
            if !p.trim().is_empty() {
                cmd.arg("--proxy").arg(p.trim());
            }
        }

        // Set output template to exact destination
        cmd.arg("-o").arg(dest.to_string_lossy().to_string());
        cmd.arg(url);

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| FreeFetchError::Io(format!("Failed to spawn yt-dlp: {e}")))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| FreeFetchError::Io("Failed to capture yt-dlp stdout".to_string()))?;

        let stderr = child.stderr.take();

        // Spawn background task to collect stderr in case of error
        let stderr_handle = tokio::spawn(async move {
            let mut err_lines = Vec::new();
            if let Some(err) = stderr {
                let mut reader = BufReader::new(err).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    if !line.trim().is_empty() {
                        err_lines.push(line);
                    }
                }
            }
            err_lines
        });

        let mut reader = BufReader::new(stdout).lines();

        // Progress regex: [download]  45.2% of 10.00MiB at 2.50MiB/s ETA 00:02
        let progress_re = regex::Regex::new(
            r"\[download\]\s+([0-9\.]+)%\s+of\s+([0-9\.]+[A-Za-z]+|\S+)(?:\s+at\s+([0-9\.]+[A-Za-z]+/\S+))?(?:\s+ETA\s+(\S+))?",
        )
        .unwrap();

        loop {
            tokio::select! {
                line_res = reader.next_line() => {
                    match line_res {
                        Ok(Some(line)) => {
                            if let Some(caps) = progress_re.captures(&line) {
                                let percent: f64 = caps.get(1).and_then(|m| m.as_str().parse().ok()).unwrap_or(0.0);
                                let total_bytes = caps.get(2).map(|m| parse_byte_size(m.as_str())).unwrap_or(0);
                                let downloaded_bytes = if total_bytes > 0 {
                                    ((percent / 100.0) * total_bytes as f64) as u64
                                } else {
                                    0
                                };
                                let speed_bytes = caps.get(3).map(|m| parse_byte_rate(m.as_str())).unwrap_or(0);
                                on_progress(percent, downloaded_bytes, total_bytes, speed_bytes);
                            }
                        }
                        Ok(None) => break,
                        Err(_) => break,
                    }
                }
                _ = async {
                    if let Some(rx) = cancel_rx.as_mut() {
                        let _ = rx.changed().await;
                    } else {
                        futures_util::future::pending::<()>().await;
                    }
                } => {
                    let _ = child.kill().await;
                    return Err(FreeFetchError::Cancelled);
                }
            }
        }

        let status = child
            .wait()
            .await
            .map_err(|e| FreeFetchError::Io(format!("Failed to wait on yt-dlp process: {e}")))?;

        if !status.success() {
            let captured_err = stderr_handle.await.unwrap_or_default().join("\n");
            let sanitized = sanitize_sensitive_text(&captured_err);
            return Err(FreeFetchError::Extractor {
                service: "yt-dlp".to_string(),
                message: if sanitized.trim().is_empty() {
                    "yt-dlp download process exited with error status".to_string()
                } else {
                    sanitized
                },
            });
        }

        // If dest does not exist directly, check if yt-dlp appended/changed extension
        if !dest.exists() {
            if let Some(parent) = dest.parent() {
                if let Some(stem) = dest.file_stem().and_then(|s| s.to_str()) {
                    if let Ok(mut entries) = tokio::fs::read_dir(parent).await {
                        while let Ok(Some(entry)) = entries.next_entry().await {
                            let path = entry.path();
                            if path.is_file() {
                                if let Some(entry_stem) = path.file_stem().and_then(|s| s.to_str())
                                {
                                    if entry_stem == stem && path != dest {
                                        let _ = tokio::fs::rename(&path, dest).await;
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

fn truncate_and_sanitize_stdout(stdout: &str) -> String {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(stdout) {
        let mut summary = serde_json::Map::new();
        if let Some(id) = val.get("id") {
            summary.insert("id".to_string(), id.clone());
        }
        if let Some(title) = val.get("title") {
            summary.insert("title".to_string(), title.clone());
        }
        if let Some(uploader) = val.get("uploader") {
            summary.insert("uploader".to_string(), uploader.clone());
        }
        if let Some(extractor) = val.get("extractor") {
            summary.insert("extractor".to_string(), extractor.clone());
        }
        if let Some(formats) = val.get("formats").and_then(|f| f.as_array()) {
            summary.insert(
                "format_count".to_string(),
                serde_json::Value::from(formats.len()),
            );
        }
        serde_json::to_string_pretty(&summary)
            .unwrap_or_else(|_| "Valid JSON metadata payload".to_string())
    } else {
        let lines: Vec<&str> = stdout.lines().take(15).collect();
        lines.join("\n")
    }
}

fn parse_byte_size(s: &str) -> u64 {
    let lower = s.to_lowercase();
    let num_str: String = lower
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let num: f64 = num_str.parse().unwrap_or(0.0);

    if lower.contains("gib") || lower.contains("gb") {
        (num * 1024.0 * 1024.0 * 1024.0) as u64
    } else if lower.contains("mib") || lower.contains("mb") {
        (num * 1024.0 * 1024.0) as u64
    } else if lower.contains("kib") || lower.contains("kb") {
        (num * 1024.0) as u64
    } else {
        num as u64
    }
}

fn parse_byte_rate(s: &str) -> u64 {
    let lower = s.to_lowercase();
    let num_str: String = lower
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let num: f64 = num_str.parse().unwrap_or(0.0);

    if lower.contains("gib/s") || lower.contains("gb/s") {
        (num * 1024.0 * 1024.0 * 1024.0) as u64
    } else if lower.contains("mib/s") || lower.contains("mb/s") {
        (num * 1024.0 * 1024.0) as u64
    } else if lower.contains("kib/s") || lower.contains("kb/s") {
        (num * 1024.0) as u64
    } else {
        num as u64
    }
}
