use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, ToSocketAddrs};
use url::Url;

use crate::core::error::FreeFetchError;

/// Returns true if the IP address is publicly routable and safe from SSRF.
pub fn is_safe_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_safe_ipv4(v4),
        IpAddr::V6(v6) => is_safe_ipv6(v6),
    }
}

fn is_safe_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    // 0.0.0.0/8 (Current network)
    if octets[0] == 0 {
        return false;
    }
    // 10.0.0.0/8 (Private)
    if octets[0] == 10 {
        return false;
    }
    // 127.0.0.0/8 (Loopback)
    if octets[0] == 127 {
        return false;
    }
    // 169.254.0.0/16 (Link-local)
    if octets[0] == 169 && octets[1] == 254 {
        return false;
    }
    // 172.16.0.0/12 (Private: 172.16.0.0 - 172.31.255.255)
    if octets[0] == 172 && (16..=31).contains(&octets[1]) {
        return false;
    }
    // 192.168.0.0/16 (Private)
    if octets[0] == 192 && octets[1] == 168 {
        return false;
    }
    // 192.0.0.0/24 (IETF Protocol Assignments)
    if octets[0] == 192 && octets[1] == 0 && octets[2] == 0 {
        return false;
    }
    // 192.0.2.0/24 (TEST-NET-1)
    if octets[0] == 192 && octets[1] == 0 && octets[2] == 2 {
        return false;
    }
    // 198.51.100.0/24 (TEST-NET-2)
    if octets[0] == 198 && octets[1] == 51 && octets[2] == 100 {
        return false;
    }
    // 203.0.113.0/24 (TEST-NET-3)
    if octets[0] == 203 && octets[1] == 0 && octets[2] == 113 {
        return false;
    }
    // 224.0.0.0/4 (Multicast: 224 - 239)
    if (224..=239).contains(&octets[0]) {
        return false;
    }
    // 240.0.0.0/4 (Reserved for future use: 240 - 255)
    if octets[0] >= 240 {
        return false;
    }
    // 255.255.255.255 (Broadcast)
    if ip == Ipv4Addr::BROADCAST {
        return false;
    }
    true
}

fn is_safe_ipv6(ip: Ipv6Addr) -> bool {
    // Unspecified ::
    if ip.is_unspecified() {
        return false;
    }
    // Loopback ::1
    if ip.is_loopback() {
        return false;
    }
    // IPv4-mapped IPv6 address ::ffff:x.x.x.x
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_safe_ipv4(v4);
    }
    let segments = ip.segments();
    // Unique local address fc00::/7
    if (segments[0] & 0xfe00) == 0xfc00 {
        return false;
    }
    // Link-local unicast fe80::/10
    if (segments[0] & 0xffc0) == 0xfe80 {
        return false;
    }
    // Multicast ff00::/8
    if (segments[0] & 0xff00) == 0xff00 {
        return false;
    }
    true
}

/// Validates that a URL is safe from SSRF attacks, has valid HTTP/HTTPS scheme,
/// and does not target loopback, private, or prohibited network destinations.
pub fn validate_safe_url(url: &Url) -> Result<(), FreeFetchError> {
    let scheme = url.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(FreeFetchError::InvalidUrl(format!(
            "Only HTTP and HTTPS URLs are permitted. Got scheme: '{scheme}'"
        )));
    }

    let host = url
        .host_str()
        .ok_or_else(|| FreeFetchError::InvalidUrl("Missing host in URL".to_string()))?;

    let lower_host = host.to_lowercase();
    if lower_host == "localhost"
        || lower_host.ends_with(".localhost")
        || lower_host.ends_with(".local")
        || lower_host.ends_with(".internal")
        || lower_host == "127.0.0.1"
        || lower_host == "0.0.0.0"
        || lower_host == "::1"
    {
        return Err(FreeFetchError::InvalidUrl(format!(
            "Access to local and private addresses is forbidden: {host}"
        )));
    }

    // If host is an IP literal
    if let Ok(ip) = host.parse::<IpAddr>() {
        if !is_safe_ip(ip) {
            return Err(FreeFetchError::InvalidUrl(format!(
                "Access to private or reserved IP address is forbidden: {ip}"
            )));
        }
        return Ok(());
    }

    // Resolve DNS to verify all destination IP addresses
    let port = url.port_or_known_default().unwrap_or(80);
    let socket_str = format!("{}:{}", host, port);
    if let Ok(addrs) = socket_str.to_socket_addrs() {
        for addr in addrs {
            if !is_safe_ip(addr.ip()) {
                return Err(FreeFetchError::InvalidUrl(format!(
                    "Hostname '{host}' resolves to prohibited private IP address: {}",
                    addr.ip()
                )));
            }
        }
    }

    Ok(())
}

/// Builds an SSRF-hardened reqwest Client with strict redirect checks and timeout.
pub fn build_ssrf_safe_client(
    timeout: std::time::Duration,
    user_agent: &str,
    proxy: Option<&str>,
) -> Result<reqwest::Client, FreeFetchError> {
    let mut builder = reqwest::Client::builder()
        .timeout(timeout)
        .user_agent(user_agent)
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 10 {
                return attempt.error("Too many redirects (maximum 10)");
            }
            let next_url = attempt.url();
            if let Err(e) = validate_safe_url(next_url) {
                return attempt.error(format!("SSRF redirect blocked: {e}"));
            }
            attempt.follow()
        }));

    if let Some(p) = proxy {
        if !p.trim().is_empty() {
            let proxy_obj = reqwest::Proxy::all(p).map_err(|e| {
                FreeFetchError::Generic(format!("Invalid proxy configuration: {e}"))
            })?;
            builder = builder.proxy(proxy_obj);
        }
    }

    builder
        .build()
        .map_err(|e| FreeFetchError::Generic(format!("Failed to build HTTP client: {e}")))
}

/// Redacts query parameters and headers containing credentials, keys, or tokens.
pub fn sanitize_sensitive_text(text: &str) -> String {
    let re_query = regex::Regex::new(
        r"(?i)(token|auth|key|signature|sig|session|cookie|secret|pass|credential|access_token|refresh_token)=([^\s&]+)",
    )
    .unwrap();
    let redacted = re_query.replace_all(text, "$1=[REDACTED]");

    let re_auth_header =
        regex::Regex::new(r"(?i)(authorization:\s*(bearer|basic)\s+)[^\r\n]+").unwrap();
    let redacted = re_auth_header.replace_all(&redacted, "$1[REDACTED]");

    let re_cookie_header = regex::Regex::new(r"(?i)(cookie:\s*)[^\r\n]+").unwrap();
    let redacted = re_cookie_header.replace_all(&redacted, "$1[REDACTED]");

    redacted.into_owned()
}
