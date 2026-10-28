use url::Url;

pub fn is_potential_media_url(text: &str) -> bool {
    let trimmed = text.trim();
    if let Ok(parsed) = Url::parse(trimmed) {
        let scheme = parsed.scheme();
        if (scheme == "http" || scheme == "https") && parsed.host_str().is_some() {
            return true;
        }
    }
    false
}
