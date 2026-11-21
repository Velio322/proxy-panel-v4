use crate::auth::extract::cookie_token_from_headers;
use crate::net::extract_ip;
use crate::state::AppState;
use axum::extract::{ConnectInfo, MatchedPath, Request, State};
use axum::http::{header, HeaderName, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use std::net::SocketAddr;
use std::time::Instant;

/// Applies Helmet-equivalent security headers (``: HSTS only on HTTPS or trusted proxy).
pub async fn security_headers(
    State(st): State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    let is_https = st.cfg.production
        || (st.cfg.trust_proxy
            && req
                .headers()
                .get("x-forwarded-proto")
                .and_then(|v| v.to_str().ok())
                .is_some_and(|p| p.eq_ignore_ascii_case("https")));

    let mut res = next.run(req).await;
    let headers = res.headers_mut();
    let static_headers = [
        ("cross-origin-opener-policy", "same-origin"),
        ("cross-origin-resource-policy", "same-origin"),
        ("origin-agent-cluster", "?1"),
        ("referrer-policy", "no-referrer"),
        ("x-content-type-options", "nosniff"),
        ("x-dns-prefetch-control", "off"),
        ("x-download-options", "noopen"),
        ("x-frame-options", "SAMEORIGIN"),
        ("x-permitted-cross-domain-policies", "none"),
        ("x-xss-protection", "0"),
    ];
    for (k, v) in static_headers {
        if let (Ok(name), Ok(val)) = (
            HeaderName::from_bytes(k.as_bytes()),
            HeaderValue::from_str(v),
        ) {
            headers.insert(name, val);
        }
    }
    if is_https {
        headers.insert(
            header::STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static("max-age=31536000; includeSubDomains"),
        );
    }
    res
}

/// CSRF Origin/Referer check for cookie-authenticated mutating requests.
pub async fn csrf_origin_check(
    State(st): State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    let is_mutating = matches!(
        *req.method(),
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    );
    if is_mutating {
        let headers = req.headers();
        let has_bearer_or_key =
            headers.contains_key(header::AUTHORIZATION) || headers.contains_key("x-api-key");
        let has_cookie = cookie_token_from_headers(headers).is_some();

        if !has_bearer_or_key && has_cookie {
            let origin_hdr = headers
                .get(header::ORIGIN)
                .and_then(|v| v.to_str().ok())
                .map(str::trim)
                .filter(|s| !s.is_empty());
            let referer_hdr = headers
                .get(header::REFERER)
                .and_then(|v| v.to_str().ok())
                .map(str::trim)
                .filter(|s| !s.is_empty());

            let source_origin = origin_hdr
                .and_then(extract_origin)
                .or_else(|| referer_hdr.and_then(extract_origin));

            let allowed = source_origin.is_some_and(|orig| is_allowed_origin(&st, &orig));
            if !allowed {
                return (
                    StatusCode::FORBIDDEN,
                    Json(json!({ "error": "Forbidden: CSRF origin check failed" })),
                )
                    .into_response();
            }
        }
    }
    next.run(req).await
}

fn extract_origin(raw: &str) -> Option<String> {
    let parsed = url::Url::parse(raw).ok()?;
    let host = parsed.host_str()?;
    let scheme = parsed.scheme();
    match parsed.port() {
        Some(p) => Some(format!("{scheme}://{host}:{p}").to_ascii_lowercase()),
        None => Some(format!("{scheme}://{host}").to_ascii_lowercase()),
    }
}

fn is_allowed_origin(st: &AppState, origin: &str) -> bool {
    let mut candidates: Vec<&str> = st.cfg.cors_origins.iter().map(String::as_str).collect();
    candidates.push(&st.cfg.api_url);
    candidates.push(&st.cfg.frontend_url);
    candidates.push(&st.cfg.sub_url);
    candidates.iter().any(|c| {
        extract_origin(c)
            .map(|co| co == origin)
            .unwrap_or_else(|| c.trim_end_matches('/').eq_ignore_ascii_case(origin))
    })
}

/// Global IP rate limiter (`` + `` pre-auth protection on `/nodes/self/register` & `/ws/worker`).
pub async fn global_rate_limit(
    State(st): State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    let path = req.uri().path();
    let has_node_cred = req
        .headers()
        .get("x-node-secret")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .is_some_and(|s| (16..=256).contains(&s.len()))
        || req
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(str::trim)
            .is_some_and(|s| (16..=256).contains(&s.len()));

    let exempt = path == "/api/health"
        || path.starts_with("/assets/")
        || (path.starts_with("/api/v1/nodes/self/")
            && path != "/api/v1/nodes/self/register"
            && has_node_cred)
        || (path == "/ws/worker" && has_node_cred);

    if exempt {
        return next.run(req).await;
    }

    let ip = extract_ip(
        req.headers(),
        req.extensions().get::<ConnectInfo<SocketAddr>>(),
        st.cfg.trust_proxy,
    );
    let outcome = st.global_limiter.check(&ip);
    if !outcome.allowed {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({ "error": "Too many requests, please try again later" })),
        )
            .into_response();
    }

    let mut res = next.run(req).await;
    let headers = res.headers_mut();
    if let Ok(v) = HeaderValue::from_str(&outcome.limit.to_string()) {
        headers.insert("RateLimit-Limit", v);
    }
    if let Ok(v) = HeaderValue::from_str(&outcome.remaining.to_string()) {
        headers.insert("RateLimit-Remaining", v);
    }
    if let Ok(v) = HeaderValue::from_str(&outcome.reset_secs.to_string()) {
        headers.insert("RateLimit-Reset", v);
    }
    res
}

pub async fn auth_rate_limit(
    State(st): State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    let ip = extract_ip(
        req.headers(),
        req.extensions().get::<ConnectInfo<SocketAddr>>(),
        st.cfg.trust_proxy,
    );
    let outcome = st.auth_limiter.check(&ip);
    if !outcome.allowed {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({ "error": "Too many login attempts" })),
        )
            .into_response();
    }
    next.run(req).await
}

pub async fn transfer_rate_limit(
    State(st): State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    let ip = extract_ip(
        req.headers(),
        req.extensions().get::<ConnectInfo<SocketAddr>>(),
        st.cfg.trust_proxy,
    );
    let outcome = st.transfer_limiter.check(&ip);
    if !outcome.allowed {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            "Too many requests, please try again later.",
        )
            .into_response();
    }
    next.run(req).await
}

pub async fn metrics_middleware(
    State(st): State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    let method = req.method().as_str().to_string();
    let route = req
        .extensions()
        .get::<MatchedPath>()
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| req.uri().path().to_string());
    let start = Instant::now();
    let res = next.run(req).await;
    st.metrics.observe_http(
        &method,
        &route,
        res.status().as_u16(),
        start.elapsed().as_secs_f64(),
    );
    res
}
