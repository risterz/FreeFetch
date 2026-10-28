//! Cross-platform hidden subprocess execution helper.
//!
//! Guarantees that internal child processes (yt-dlp, FFmpeg, ffprobe, etc.)
//! run invisibly on Windows without allocating a visible console window,
//! while preserving direct-vector execution, piped stdout/stderr,
//! cancellation, and exit status.

#[cfg(target_os = "windows")]
pub const CREATE_NO_WINDOW: u32 = 0x08000000;

/// Configures a `tokio::process::Command` to hide any console window on Windows.
pub fn configure_hidden_tokio_command(
    cmd: &mut tokio::process::Command,
) -> &mut tokio::process::Command {
    #[cfg(target_os = "windows")]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Creates a new `tokio::process::Command` configured to execute without a visible console window.
pub fn hidden_tokio_command<P: AsRef<std::ffi::OsStr>>(program: P) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(program);
    configure_hidden_tokio_command(&mut cmd);
    cmd
}

/// Configures a `std::process::Command` to hide any console window on Windows.
pub fn configure_hidden_std_command(cmd: &mut std::process::Command) -> &mut std::process::Command {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Creates a new `std::process::Command` configured to execute without a visible console window.
pub fn hidden_std_command<P: AsRef<std::ffi::OsStr>>(program: P) -> std::process::Command {
    let mut cmd = std::process::Command::new(program);
    configure_hidden_std_command(&mut cmd);
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hidden_std_command_execution() {
        #[cfg(target_os = "windows")]
        {
            let output = hidden_std_command("cmd")
                .arg("/C")
                .arg("echo test_hidden_std")
                .output()
                .expect("Failed to execute hidden std command");
            assert!(output.status.success());
            let text = String::from_utf8_lossy(&output.stdout);
            assert!(text.contains("test_hidden_std"));
        }
        #[cfg(not(target_os = "windows"))]
        {
            let output = hidden_std_command("echo")
                .arg("test_hidden_std")
                .output()
                .expect("Failed to execute hidden std command");
            assert!(output.status.success());
        }
    }

    #[tokio::test]
    async fn test_hidden_tokio_command_execution() {
        #[cfg(target_os = "windows")]
        {
            let output = hidden_tokio_command("cmd")
                .arg("/C")
                .arg("echo test_hidden_tokio")
                .output()
                .await
                .expect("Failed to execute hidden tokio command");
            assert!(output.status.success());
            let text = String::from_utf8_lossy(&output.stdout);
            assert!(text.contains("test_hidden_tokio"));
        }
        #[cfg(not(target_os = "windows"))]
        {
            let output = hidden_tokio_command("echo")
                .arg("test_hidden_tokio")
                .output()
                .await
                .expect("Failed to execute hidden tokio command");
            assert!(output.status.success());
        }
    }
}
