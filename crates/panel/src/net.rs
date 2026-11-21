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
        if let Some(xff) = headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
        {
            if let Some(first) = xff.split(',').next().map(str::trim).filter(|s| !s.is_empty()) {
                return first.to_string();
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
    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_documentation()
        || (ip.octets()[0] == 100 && (ip.octets()[1] & 0b1100_0000) == 64) // CGNAT 100.64.0.0/10
}

fn is_private_v6(ip: &Ipv6Addr) -> bool {
    ip.is_loopback()
        || ip.is_unspecified()
        || (ip.segments()[0] & 0xfe00) == 0xfc00 // Unique local fc00::/7
        || (ip.segments()[0] & 0xffc0) == 0xfe80 // Link-local fe80::/10
}

pub fn is_loopback_host(host: &str) -> bool {
    let h = host
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or("")
        .trim();
    let host_only = if h.starts_with('[') {
        h.split(']').next().unwrap_or(h).trim_start_matches('[')
    } else {
        h.split(':').next().unwrap_or(h)
    };
    matches!(
        host_only.to_ascii_lowercase().as_str(),
        "127.0.0.1" | "::1" | "localhost" | "0.0.0.0"
    )
}

/// Validates node host against SSRF rules.
pub fn validate_node_host(raw_host: &str, allow_private: bool) -> AppResult<()> {
    let trimmed = raw_host.trim();
    if trimmed.is_empty() {
        return Err(AppError::bad_request("Node host is empty"));
    }
    let host_part = if trimmed.contains("://") {
        let parsed = url::Url::parse(trimmed)
            .map_err(|_| AppError::bad_request("Invalid node URL"))?;
        match parsed.scheme() {
            "http" | "https" => {}
            _ => return Err(AppError::bad_request("Unsupported node protocol")),
        }
        if !parsed.username().is_empty() || parsed.password().is_some() {
            return Err(AppError::bad_request(
                "Node URL must not include credentials",
            ));
        }
        parsed
            .host_str()
            .ok_or_else(|| AppError::bad_request("Node URL has no host"))?
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_string()
    } else if trimmed.starts_with('[') {
        trimmed
            .split(']')
            .next()
            .unwrap_or(trimmed)
            .trim_start_matches('[')
            .to_string()
    } else {
        trimmed.split(':').next().unwrap_or(trimmed).to_string()
    };

    if !allow_private {
        if matches!(
            host_part.to_ascii_lowercase().as_str(),
            "localhost" | "0.0.0.0"
        ) {
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
