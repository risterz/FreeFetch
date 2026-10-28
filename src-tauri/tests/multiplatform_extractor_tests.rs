use freefetch_lib::core::security::{
    is_safe_ip, sanitize_sensitive_text, validate_safe_url,
};
use freefetch_lib::extractor::implementations::dropbox::convert_to_direct_dropbox_url;
use freefetch_lib::extractor::implementations::google_drive::{
    extract_gdrive_file_id, parse_content_disposition_filename,
};
use freefetch_lib::extractor::ExtractorRegistry;
use std::net::IpAddr;
use url::Url;

#[test]
fn test_ssrf_ip_validation() {
    // Prohibited IPv4 loopback & private ranges
    assert!(!is_safe_ip("127.0.0.1".parse::<IpAddr>().unwrap()));
    assert!(!is_safe_ip("127.255.0.1".parse::<IpAddr>().unwrap()));
    assert!(!is_safe_ip("10.0.0.1".parse::<IpAddr>().unwrap()));
    assert!(!is_safe_ip("10.254.12.34".parse::<IpAddr>().unwrap()));
    assert!(!is_safe_ip("172.16.0.1".parse::<IpAddr>().unwrap()));
    assert!(!is_safe_ip("172.31.255.255".parse::<IpAddr>().unwrap()));
    assert!(!is_safe_ip("192.168.1.1".parse::<IpAddr>().unwrap()));
    assert!(!is_safe_ip("169.254.169.254".parse::<IpAddr>().unwrap())); // AWS metadata
    assert!(!is_safe_ip("0.0.0.0".parse::<IpAddr>().unwrap()));
    assert!(!is_safe_ip("255.255.255.255".parse::<IpAddr>().unwrap()));

    // Prohibited IPv6
    assert!(!is_safe_ip("::1".parse::<IpAddr>().unwrap())); // loopback
    assert!(!is_safe_ip("::".parse::<IpAddr>().unwrap())); // unspecified
    assert!(!is_safe_ip("fe80::1".parse::<IpAddr>().unwrap())); // link-local
    assert!(!is_safe_ip("fc00::1".parse::<IpAddr>().unwrap())); // unique local

    // Allowed public IPs
    assert!(is_safe_ip("8.8.8.8".parse::<IpAddr>().unwrap()));
    assert!(is_safe_ip("1.1.1.1".parse::<IpAddr>().unwrap()));
    assert!(is_safe_ip("142.250.190.46".parse::<IpAddr>().unwrap())); // Google
    assert!(is_safe_ip("151.101.1.140".parse::<IpAddr>().unwrap())); // Reddit
}

#[test]
fn test_ssrf_url_validation() {
    // Dangerous schemes
    let file_url = Url::parse("file:///etc/passwd").unwrap();
    assert!(validate_safe_url(&file_url).is_err());

    let ftp_url = Url::parse("ftp://example.com/file.mp4").unwrap();
    assert!(validate_safe_url(&ftp_url).is_err());

    // Localhost hostnames
    let local_url1 = Url::parse("http://localhost:8080/video.mp4").unwrap();
    assert!(validate_safe_url(&local_url1).is_err());

    let local_url2 = Url::parse("http://myhost.local/stream").unwrap();
    assert!(validate_safe_url(&local_url2).is_err());

    let local_url3 = Url::parse("http://server.internal/asset.mp3").unwrap();
    assert!(validate_safe_url(&local_url3).is_err());

    // Private IP literals
    let priv_ip_url = Url::parse("http://192.168.1.100/video.mp4").unwrap();
    assert!(validate_safe_url(&priv_ip_url).is_err());

    let meta_url = Url::parse("http://169.254.169.254/latest/meta-data").unwrap();
    assert!(validate_safe_url(&meta_url).is_err());

    // Valid public URLs
    let yt_url = Url::parse("https://www.youtube.com/watch?v=dQw4w9WgXcQ").unwrap();
    assert!(validate_safe_url(&yt_url).is_ok());

    let gdrive_url =
        Url::parse("https://drive.google.com/file/d/12345abcdef/view?usp=sharing").unwrap();
    assert!(validate_safe_url(&gdrive_url).is_ok());
}

#[test]
fn test_sensitive_data_sanitization() {
    let raw_error = "Failed to download: https://cdn.example.com/stream?token=secret12345&auth=pass999&sig=abcdef";
    let sanitized = sanitize_sensitive_text(raw_error);
    assert!(!sanitized.contains("secret12345"));
    assert!(!sanitized.contains("pass999"));
    assert!(!sanitized.contains("abcdef"));
    assert!(sanitized.contains("token=[REDACTED]"));
    assert!(sanitized.contains("auth=[REDACTED]"));
    assert!(sanitized.contains("sig=[REDACTED]"));

    let raw_header = "Error with header Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.xyz in request";
    let sanitized_header = sanitize_sensitive_text(raw_header);
    assert!(!sanitized_header.contains("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"));
    assert!(sanitized_header.contains("Authorization: Bearer [REDACTED]"));
}

#[test]
fn test_google_drive_file_id_extraction() {
    let url1 =
        Url::parse("https://drive.google.com/file/d/1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms/view?usp=sharing")
            .unwrap();
    assert_eq!(
        extract_gdrive_file_id(&url1).as_deref(),
        Some("1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms")
    );

    let url2 = Url::parse("https://drive.google.com/open?id=1AbC_dEf-1234567890").unwrap();
    assert_eq!(
        extract_gdrive_file_id(&url2).as_deref(),
        Some("1AbC_dEf-1234567890")
    );

    let url3 = Url::parse("https://docs.google.com/file/d/987654321_abc/edit").unwrap();
    assert_eq!(
        extract_gdrive_file_id(&url3).as_deref(),
        Some("987654321_abc")
    );

    let url4 = Url::parse("https://drive.google.com/uc?id=XYZ123&export=download").unwrap();
    assert_eq!(extract_gdrive_file_id(&url4).as_deref(), Some("XYZ123"));
}

#[test]
fn test_google_drive_content_disposition_parsing() {
    let cd1 = r#"attachment; filename="vacation_clip.mp4""#;
    assert_eq!(
        parse_content_disposition_filename(cd1).as_deref(),
        Some("vacation_clip.mp4")
    );

    let cd2 = "attachment; filename*=UTF-8''holiday%20in%20tokyo.mp4";
    assert_eq!(
        parse_content_disposition_filename(cd2).as_deref(),
        Some("holiday in tokyo.mp4")
    );

    let cd3 = r#"attachment; filename="fallback.mkv"; filename*=UTF-8''special%20video.mkv"#;
    assert_eq!(
        parse_content_disposition_filename(cd3).as_deref(),
        Some("special video.mkv")
    );
}

#[test]
fn test_dropbox_url_conversion() {
    let url1 = Url::parse("https://www.dropbox.com/s/abcdef123456789/video.mp4?dl=0").unwrap();
    let direct1 = convert_to_direct_dropbox_url(&url1);
    assert!(direct1.contains("dl=1"));
    assert!(!direct1.contains("dl=0"));

    let url2 =
        Url::parse("https://www.dropbox.com/scl/fi/xyz123/track.mp3?rlkey=secretkey&dl=0").unwrap();
    let direct2 = convert_to_direct_dropbox_url(&url2);
    assert!(direct2.contains("dl=1"));
    assert!(direct2.contains("rlkey=secretkey"));
}

#[test]
fn test_extractor_registry_routing() {
    let registry = ExtractorRegistry::new();

    // 1. YouTube routing
    let yt_url = Url::parse("https://www.youtube.com/watch?v=dQw4w9WgXcQ").unwrap();
    let ext = registry.find_extractor(&yt_url).expect("Must find extractor for YouTube");
    assert_eq!(ext.name(), "youtube");

    // 2. Google Drive routing
    let gdrive_url = Url::parse("https://drive.google.com/file/d/123456789/view").unwrap();
    let ext = registry.find_extractor(&gdrive_url).expect("Must find extractor for Google Drive");
    assert_eq!(ext.name(), "google_drive");

    // 3. Dropbox routing
    let dropbox_url = Url::parse("https://www.dropbox.com/s/abcdef123/video.mp4?dl=0").unwrap();
    let ext = registry.find_extractor(&dropbox_url).expect("Must find extractor for Dropbox");
    assert_eq!(ext.name(), "dropbox");

    // 4. TikTok routing
    let tiktok_url = Url::parse("https://www.tiktok.com/@user/video/7123456789012345678").unwrap();
    let ext = registry.find_extractor(&tiktok_url).expect("Must find extractor for TikTok");
    assert_eq!(ext.name(), "ytdlp");

    // 5. Instagram routing
    let insta_url = Url::parse("https://www.instagram.com/reel/C123456789/").unwrap();
    let ext = registry.find_extractor(&insta_url).expect("Must find extractor for Instagram");
    assert_eq!(ext.name(), "ytdlp");

    // 6. X (Twitter) routing
    let twitter_url = Url::parse("https://x.com/user/status/1234567890123456789").unwrap();
    let ext = registry.find_extractor(&twitter_url).expect("Must find extractor for X");
    assert_eq!(ext.name(), "ytdlp");

    // 7. Reddit routing
    let reddit_url =
        Url::parse("https://www.reddit.com/r/videos/comments/abc123/cool_video/").unwrap();
    let ext = registry.find_extractor(&reddit_url).expect("Must find extractor for Reddit");
    assert_eq!(ext.name(), "ytdlp");

    // 8. Vimeo routing
    let vimeo_url = Url::parse("https://vimeo.com/123456789").unwrap();
    let ext = registry.find_extractor(&vimeo_url).expect("Must find extractor for Vimeo");
    assert_eq!(ext.name(), "ytdlp");

    // 9. Twitch routing
    let twitch_url = Url::parse("https://www.twitch.tv/videos/123456789").unwrap();
    let ext = registry.find_extractor(&twitch_url).expect("Must find extractor for Twitch");
    assert_eq!(ext.name(), "ytdlp");

    // 10. Generic direct media link
    let direct_url = Url::parse("https://example.com/videos/awesome_movie.mp4").unwrap();
    let ext = registry.find_extractor(&direct_url).expect("Must find extractor for direct link");
    assert_eq!(ext.name(), "generic");
}

#[tokio::test]
async fn test_real_world_tiktok_extraction() {
    let registry = ExtractorRegistry::new();
    let tiktok_url = Url::parse("https://www.tiktok.com/@hansenvendiagus14/video/7692418604607868180")
        .expect("Valid URL");

    let ext = registry
        .find_extractor(&tiktok_url)
        .expect("Must find extractor for TikTok");
    assert_eq!(ext.name(), "ytdlp");

    let ctx = freefetch_lib::extractor::ExtractionContext::default();

    let result = ext.extract(&tiktok_url, &ctx).await;
    match result {
        Ok((meta, manifest)) => {
            println!(
                "TikTok extracted successfully: title='{}', formats={}",
                meta.title,
                manifest.formats.len()
            );
            assert!(!meta.title.is_empty(), "Title should not be empty");
            assert!(!manifest.formats.is_empty(), "Formats should not be empty");
            assert_eq!(meta.service_name, "tiktok");
        }
        Err(e) => {
            panic!("TikTok extraction failed: {:?}", e);
        }
    }
}
