use crate::core::error::FreeFetchError;
use std::path::{Path, PathBuf};
use url::Url;

pub fn sanitize_filename(name: &str) -> String {
    let mut clean = String::with_capacity(name.len());
    for c in name.chars() {
        match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' | '\0'..='\x1F' => {
                clean.push('_');
            }
            _ => clean.push(c),
        }
    }
    let trimmed = clean.trim().trim_matches('.');
    let mut safe = trimmed.to_string();
    while safe.contains("..") {
        safe = safe.replace("..", "_");
    }
    let trimmed_final = safe.trim_matches('.');
    if trimmed_final.is_empty() {
        return "download".to_string();
    }

    const WINDOWS_RESERVED: &[&str] = &[
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];

    let upper = trimmed_final.to_uppercase();
    let safe_reserved = if WINDOWS_RESERVED.contains(&upper.as_str()) {
        format!("_{trimmed_final}")
    } else {
        trimmed_final.to_string()
    };

    if safe_reserved.len() > 180 {
        safe_reserved[..180].to_string()
    } else {
        safe_reserved
    }
}

pub fn validate_download_url(url_str: &str) -> Result<Url, FreeFetchError> {
    let parsed = Url::parse(url_str)?;
    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(FreeFetchError::InvalidUrl(format!(
            "Only HTTP and HTTPS URLs are permitted. Got: {scheme}"
        )));
    }
    if parsed.host_str().is_none() {
        return Err(FreeFetchError::InvalidUrl(
            "Missing host in URL".to_string(),
        ));
    }
    Ok(parsed)
}

pub fn ensure_safe_destination_path<P: AsRef<Path>>(dir: P, filename: &str, ext: &str) -> PathBuf {
    ensure_safe_destination_path_with_reserved(dir, filename, ext, &[])
}

pub fn ensure_safe_destination_path_with_reserved<P: AsRef<Path>>(
    dir: P,
    filename: &str,
    ext: &str,
    reserved_paths: &[String],
) -> PathBuf {
    let safe_stem = sanitize_filename(filename);
    let mut candidate = dir.as_ref().join(format!("{safe_stem}.{ext}"));
    let mut counter = 1;
    while candidate.exists()
        || candidate.with_extension(format!("{ext}.part")).exists()
        || candidate
            .with_extension(format!("{ext}.part.meta"))
            .exists()
        || candidate.with_extension("video.part").exists()
        || candidate.with_extension("video.part.meta").exists()
        || candidate.with_extension("audio.part").exists()
        || candidate.with_extension("audio.part.meta").exists()
        || candidate.with_extension(format!("{ext}.tmp")).exists()
        || reserved_paths.iter().any(|p| Path::new(p) == candidate)
    {
        candidate = dir.as_ref().join(format!("{safe_stem} ({counter}).{ext}"));
        counter += 1;
    }
    candidate
}
