use reqwest::Client;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::engine::state::validate_download_url;

pub struct ArtworkDownloader;

impl ArtworkDownloader {
    /// Checks magic bytes to ensure response payload is a valid JPEG, PNG, or WebP image
    pub fn is_valid_image_bytes(bytes: &[u8]) -> bool {
        if bytes.len() < 8 {
            return false;
        }

        // JPEG: FF D8 FF
        let is_jpeg = bytes.starts_with(&[0xFF, 0xD8, 0xFF]);

        // PNG: 89 50 4E 47 0D 0A 1A 0A
        let is_png = bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A])
            || bytes.starts_with(&[0x89, b'P', b'N', b'G']);

        // WebP: RIFF .... WEBP
        let is_webp = bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP";

        is_jpeg || is_png || is_webp
    }

    /// Downloads and validates a thumbnail image to a temporary file.
    /// Returns Ok(Some(path)) on success, or Ok(None) if download fails non-fatally.
    pub async fn download_cover_art<P: AsRef<Path>>(
        client: &Client,
        thumbnail_url: &str,
        dest_dir: P,
        job_id: &str,
    ) -> Option<PathBuf> {
        let clean_url = thumbnail_url.trim();
        if clean_url.is_empty() {
            return None;
        }

        if validate_download_url(clean_url).is_err() {
            eprintln!("[FreeFetch] Invalid thumbnail URL: {clean_url}");
            return None;
        }

        let resp = match client
            .get(clean_url)
            .timeout(Duration::from_secs(15))
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => {
                eprintln!(
                    "[FreeFetch] Thumbnail download returned HTTP status {}",
                    r.status()
                );
                return None;
            }
            Err(e) => {
                eprintln!("[FreeFetch] Thumbnail download request failed: {e}");
                return None;
            }
        };

        let bytes = match resp.bytes().await {
            Ok(b) => b,
            Err(e) => {
                eprintln!("[FreeFetch] Failed to read thumbnail response bytes: {e}");
                return None;
            }
        };

        if !Self::is_valid_image_bytes(&bytes) {
            eprintln!("[FreeFetch] Downloaded thumbnail data failed magic byte validation");
            return None;
        }

        let temp_path = dest_dir.as_ref().join(format!("{job_id}.cover.tmp.jpg"));
        if let Err(e) = tokio::fs::write(&temp_path, &bytes).await {
            eprintln!("[FreeFetch] Failed to write temporary cover art file: {e}");
            return None;
        }

        Some(temp_path)
    }

    /// Cleans up a temporary cover art file if it exists.
    pub async fn cleanup_cover_art<P: AsRef<Path>>(path: P) {
        let p = path.as_ref();
        if p.exists() {
            let _ = tokio::fs::remove_file(p).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_magic_byte_validation() {
        let jpeg_bytes = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46];
        assert!(ArtworkDownloader::is_valid_image_bytes(&jpeg_bytes));

        let png_bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        assert!(ArtworkDownloader::is_valid_image_bytes(&png_bytes));

        let webp_bytes = b"RIFF____WEBP".to_vec();
        assert!(ArtworkDownloader::is_valid_image_bytes(&webp_bytes));

        let html_bytes = b"<!DOCTYPE html><html><body>Error</body></html>".to_vec();
        assert!(!ArtworkDownloader::is_valid_image_bytes(&html_bytes));

        let empty_bytes = vec![0u8; 4];
        assert!(!ArtworkDownloader::is_valid_image_bytes(&empty_bytes));
    }

    #[tokio::test]
    async fn test_download_cover_art_success_and_cleanup() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = [0u8; 1024];
                let _ = socket.read(&mut buf).await;
                let jpeg_content = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46];
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    jpeg_content.len()
                );
                let _ = socket.write_all(resp.as_bytes()).await;
                let _ = socket.write_all(&jpeg_content).await;
            }
        });

        let client = Client::new();
        let tmp = std::env::temp_dir().join(format!("ff_art_test_{}", std::process::id()));
        let _ = tokio::fs::create_dir_all(&tmp).await;

        let url = format!("http://127.0.0.1:{}/thumb.jpg", addr.port());
        let res = ArtworkDownloader::download_cover_art(&client, &url, &tmp, "job123").await;
        assert!(res.is_some());
        let cover_path = res.unwrap();
        assert!(cover_path.exists());

        // Cleanup test
        ArtworkDownloader::cleanup_cover_art(&cover_path).await;
        assert!(!cover_path.exists());

        let _ = tokio::fs::remove_dir_all(&tmp).await;
    }

    #[tokio::test]
    async fn test_download_cover_art_404_failure_isolation() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = [0u8; 1024];
                let _ = socket.read(&mut buf).await;
                let resp =
                    "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                let _ = socket.write_all(resp.as_bytes()).await;
            }
        });

        let client = Client::new();
        let tmp = std::env::temp_dir();
        let url = format!("http://127.0.0.1:{}/missing.jpg", addr.port());

        // 404 must return None without panic or error
        let res = ArtworkDownloader::download_cover_art(&client, &url, &tmp, "job404").await;
        assert!(res.is_none());
    }

    #[tokio::test]
    async fn test_download_cover_art_invalid_bytes_isolation() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = [0u8; 1024];
                let _ = socket.read(&mut buf).await;
                let resp = "HTTP/1.1 200 OK\r\nContent-Length: 12\r\nConnection: close\r\n\r\nHello World!";
                let _ = socket.write_all(resp.as_bytes()).await;
            }
        });

        let client = Client::new();
        let tmp = std::env::temp_dir();
        let url = format!("http://127.0.0.1:{}/fake.jpg", addr.port());

        // Invalid payload must return None without panic or error
        let res = ArtworkDownloader::download_cover_art(&client, &url, &tmp, "job_fake").await;
        assert!(res.is_none());
    }
}
