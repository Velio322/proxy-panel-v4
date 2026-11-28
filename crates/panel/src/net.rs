use crate::error::{AppError, AppResult};
use crate::state::AppState;
use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;
use axum::http::{header, HeaderMap};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

#[derive(Debug, Clone)]
pub struct ClientIp(pub String);

impl std::ops::Deref for ClientIp {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromRequestParts<AppState> for ClientIp {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        Ok(Self(extract_ip(&parts.headers, parts.extensions.get::<ConnectInfo<SocketAddr>>(), state.cfg.trust_proxy)))
    }
}

pub fn extract_ip(
    headers: &HeaderMap,
    connect_info: Option<&ConnectInfo<SocketAddr>>,
    trust_proxy: bool,
) -> String {
    if trust_proxy {
        // The rightmost X-Forwarded-For hop is the one our own reverse proxy appended (or set),
        // so a client cannot forge it; earlier entries are client-controlled and ignored.
        // X-Real-IP is only a fallback: it is trustworthy solely if the proxy overwrites it.
        if let Some(xff) = headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
        {
            if let Some(last) = xff.rsplit(',').next().map(str::trim).filter(|s| !s.is_empty()) {
                return last.to_string();
            }
        }
        if let Some(real_ip) = headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
            let trimmed = real_ip.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }
    connect_info
        .map(|ci| ci.0.ip().to_string())
        .unwrap_or_else(|| "127.0.0.1".to_string())
}

pub fn user_agent(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
}

/// Checks if an IP address belongs to loopback, private, link-local, or unspecified ranges.
pub fn is_private_or_loopback_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_private_v4(v4),
        IpAddr::V6(v6) => {
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return is_private_v4(&mapped);
            }
            is_private_v6(v6)
        }
    }
}

fn is_private_v4(ip: &Ipv4Addr) -> bool {
    let octets = ip.octets();
    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_multicast()
        || octets[0] == 0 // 0.0.0.0/8 ("this host on this network")
        || octets[0] >= 240 // 240.0.0.0/4 reserved/future use
        || (octets[0] == 100 && (octets[1] & 0b1100_0000) == 64) // CGNAT 100.64.0.0/10
}

fn is_private_v6(ip: &Ipv6Addr) -> bool {
    let segs = ip.segments();
    let is_nat64 = segs[0] == 0x0064 && segs[1] == 0xff9b && segs[2..6] == [0, 0, 0, 0];
    let nat64_private = if is_nat64 {
        let embedded = Ipv4Addr::from(((segs[6] as u32) << 16) | (segs[7] as u32));
        is_private_v4(&embedded)
    } else {
        false
    };
    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || (segs[0] & 0xfe00) == 0xfc00 // Unique local fc00::/7
        || (segs[0] & 0xffc0) == 0xfe80 // Link-local fe80::/10
        || nat64_private
}

fn is_forbidden_hostname(host: &str) -> bool {
    let h = host.trim().trim_end_matches('.').to_ascii_lowercase();
    matches!(h.as_str(), "localhost" | "0.0.0.0")
        || h.ends_with(".localhost")
        || h.ends_with(".local")
}

fn parse_host_part(raw_host: &str) -> AppResult<String> {
    let trimmed = raw_host.trim();
    if trimmed.is_empty() {
        return Err(AppError::bad_request("Node host is empty"));
    }
    let candidate_url = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        if trimmed.chars().any(char::is_whitespace)
            || trimmed.contains('/')
            || trimmed.contains('@')
            || trimmed.contains('?')
            || trimmed.contains('#')
        {
            return Err(AppError::bad_request("Invalid node host"));
        }
        if trimmed.parse::<Ipv6Addr>().is_ok() {
            format!("https://[{trimmed}]")
        } else {
            format!("https://{trimmed}")
        }
    };
    let parsed =
        url::Url::parse(&candidate_url).map_err(|_| AppError::bad_request("Invalid node URL"))?;
    match parsed.scheme() {
        "http" | "https" => {}
        _ => return Err(AppError::bad_request("Unsupported node protocol")),
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(AppError::bad_request(
            "Node URL must not include credentials",
        ));
    }
    let host_part = parsed
        .host_str()
        .ok_or_else(|| AppError::bad_request("Node URL has no host"))?
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim_end_matches('.')
        .to_string();
    if host_part.is_empty() {
        return Err(AppError::bad_request("Node host is empty"));
    }
    Ok(host_part)
}

pub fn is_loopback_host(host: &str) -> bool {
    let Ok(host_only) = parse_host_part(host) else {
        return false;
    };
    let lower = host_only.to_ascii_lowercase();
    if matches!(lower.as_str(), "127.0.0.1" | "::1" | "localhost" | "0.0.0.0") {
        return true;
    }
    if let Ok(ip) = lower.parse::<IpAddr>() {
        return match ip {
            IpAddr::V4(v4) => v4.is_loopback() || v4.is_unspecified(),
            IpAddr::V6(v6) => {
                v6.is_loopback()
                    || v6.is_unspecified()
                    || v6.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback() || v4.is_unspecified())
            }
        };
    }
    false
}

/// Validates node host against SSRF rules.
pub fn validate_node_host(raw_host: &str, allow_private: bool) -> AppResult<()> {
    let host_part = parse_host_part(raw_host)?;
    if !allow_private {
        if is_forbidden_hostname(&host_part) {
            return Err(AppError::bad_request(
                "Private or loopback node addresses are disabled (ALLOW_PRIVATE_NODES=false)",
            ));
        }
        if let Ok(ip) = host_part.parse::<IpAddr>() {
            if is_private_or_loopback_ip(&ip) {
                return Err(AppError::bad_request(
                    "Private or loopback node addresses are disabled (ALLOW_PRIVATE_NODES=false)",
                ));
            }
        }
    }
    Ok(())
}

/// Validates node host against SSRF rules, including DNS resolution when `allow_private` is false.
pub async fn validate_node_host_async(raw_host: &str, allow_private: bool) -> AppResult<()> {
    validate_node_host(raw_host, allow_private)?;
    if allow_private {
        return Ok(());
    }
    let host_part = parse_host_part(raw_host)?;
    if host_part.parse::<IpAddr>().is_ok() {
        return Ok(());
    }
    let addrs = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        tokio::net::lookup_host((host_part.as_str(), 443)),
    )
    .await
    .map_err(|_| AppError::bad_request("DNS lookup timed out for node host"))?
    .map_err(|_| AppError::bad_request("Unable to resolve node host"))?;
    let mut count = 0usize;
    for addr in addrs {
        count += 1;
        if is_private_or_loopback_ip(&addr.ip()) {
            return Err(AppError::bad_request(
                "Node host resolves to a private or loopback IP address (ALLOW_PRIVATE_NODES=false)",
            ));
        }
    }
    if count == 0 {
        return Err(AppError::bad_request("Node host resolved to no addresses"));
    }
    Ok(())
}

/// Validates an outbound HTTP/HTTPS URL (e.g. webhooks) against SSRF rules including DNS resolution.
pub async fn validate_outbound_url(raw_url: &str, allow_private: bool) -> AppResult<url::Url> {
    let (parsed, _) = validate_outbound_url_with_addrs(raw_url, allow_private).await?;
    Ok(parsed)
}

/// Validates an outbound HTTP/HTTPS URL against SSRF rules and returns its validated socket addresses
/// so callers can pin DNS resolution and prevent DNS-rebinding TOCTOU.
pub async fn validate_outbound_url_with_addrs(
    raw_url: &str,
    allow_private: bool,
) -> AppResult<(url::Url, Vec<SocketAddr>)> {
    let parsed = url::Url::parse(raw_url.trim())
        .map_err(|_| AppError::validation("url", "Invalid URL"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(AppError::validation("url", "URL must use http or https"));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(AppError::validation(
            "url",
            "URL must not include credentials",
        ));
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| AppError::validation("url", "URL has no host"))?
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim_end_matches('.');
    if host.is_empty() {
        return Err(AppError::validation("url", "URL has no host"));
    }
    let port = parsed.port_or_known_default().unwrap_or(443);
    let mut resolved = Vec::new();
    if !allow_private {
        if is_forbidden_hostname(host) {
            return Err(AppError::validation(
                "url",
                "Private or loopback URL targets are not allowed",
            ));
        }
        if let Ok(ip) = host.parse::<IpAddr>() {
            if is_private_or_loopback_ip(&ip) {
                return Err(AppError::validation(
                    "url",
                    "Private or loopback URL targets are not allowed",
                ));
            }
            resolved.push(SocketAddr::new(ip, port));
            return Ok((parsed, resolved));
        }
        let addrs = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            tokio::net::lookup_host((host, port)),
        )
        .await
        .map_err(|_| AppError::validation("url", "DNS lookup timed out for URL host"))?
        .map_err(|_| AppError::validation("url", "Unable to resolve URL host"))?;
        for addr in addrs {
            if is_private_or_loopback_ip(&addr.ip()) {
                return Err(AppError::validation(
                    "url",
                    "URL host resolves to a private or loopback IP address",
                ));
            }
            resolved.push(addr);
        }
        if resolved.is_empty() {
            return Err(AppError::validation("url", "URL host resolved to no addresses"));
        }
    }
    Ok((parsed, resolved))
}

#[cfg(test)]
mod client_ip_tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(*k, HeaderValue::from_str(v).unwrap());
        }
        h
    }

    #[test]
    fn rightmost_xff_wins_and_spoofed_prefix_or_real_ip_is_ignored() {
        let h = headers(&[("x-real-ip", "9.9.9.9"), ("x-forwarded-for", "1.1.1.1, 203.0.113.9")]);
        assert_eq!(extract_ip(&h, None, true), "203.0.113.9");
        let h = headers(&[("x-forwarded-for", "6.6.6.6, 198.51.100.4")]);
        assert_eq!(extract_ip(&h, None, true), "198.51.100.4");
        let h = headers(&[("x-real-ip", "203.0.113.9")]);
        assert_eq!(extract_ip(&h, None, true), "203.0.113.9");
    }

    #[test]
    fn untrusted_proxy_headers_are_ignored() {
        let h = headers(&[("x-real-ip", "203.0.113.9")]);
        assert_eq!(extract_ip(&h, None, false), "127.0.0.1");
    }

    #[test]
    fn node_host_validation_blocks_ssrf_variants() {
        for bad in [
            "::1",
            "fe80::1",
            "fc00::1",
            "::ffff:127.0.0.1",
            "127.1",
            "2130706433",
            "0.1.2.3",
            "localhost.",
            "https://169.254.169.254/latest",
            "http://[::1]:2222",
        ] {
            assert!(
                validate_node_host(bad, false).is_err(),
                "expected {bad} to be rejected when allow_private=false"
            );
        }
        assert!(validate_node_host("1.1.1.1", false).is_ok());
        assert!(validate_node_host("node1.example.com", false).is_ok());
        assert!(is_loopback_host("::1"));
        assert!(is_loopback_host("https://[::1]:2222"));
    }
}
