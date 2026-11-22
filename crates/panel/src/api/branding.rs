use crate::api::common::AppJson;
use crate::audit::{self, AuditEntry};
use crate::auth::extract::AuthUser;
use crate::error::{AppError, AppResult};
use crate::net::{user_agent, ClientIp};
use crate::state::AppState;
use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::path::PathBuf;
use std::time::Duration;

pub const DEFAULT_PANEL_NAME: &str = "Keeper";
const SETTING_KEY: &str = "panel_branding";
const CACHE_KEY: &str = "branding:meta";
pub const MAX_LOGO_BYTES: usize = 1_048_576; // 1 MiB

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrandingMeta {
    pub panel_name: String,
    pub has_custom_logo: bool,
    pub logo_mime: Option<String>,
    pub logo_file: Option<String>,
    pub logo_version: Option<String>,
}

impl Default for BrandingMeta {
    fn default() -> Self {
        Self {
            panel_name: DEFAULT_PANEL_NAME.to_string(),
            has_custom_logo: false,
            logo_mime: None,
            logo_file: None,
            logo_version: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrandingPublic {
    pub panel_name: String,
    pub has_custom_logo: bool,
    pub logo_mime: Option<String>,
    pub logo_version: Option<String>,
    pub logo_url: Option<String>,
}

impl BrandingPublic {
    fn from_meta(meta: &BrandingMeta, logo_exists: bool) -> Self {
        let name = clean_panel_name(&meta.panel_name).unwrap_or_else(|_| DEFAULT_PANEL_NAME.to_string());
        let has_logo = meta.has_custom_logo && logo_exists && meta.logo_file.is_some();
        let version = if has_logo {
            meta.logo_version.clone()
        } else {
            None
        };
        let logo_url = if has_logo {
            match &version {
                Some(v) if !v.is_empty() => Some(format!("/api/v1/branding/logo?v={v}")),
                _ => Some("/api/v1/branding/logo".to_string()),
            }
        } else {
            None
        };
        Self {
            panel_name: name,
            has_custom_logo: has_logo,
            logo_mime: if has_logo { meta.logo_mime.clone() } else { None },
            logo_version: version,
            logo_url,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateBrandingReq {
    panel_name: Option<String>,
    logo_data_url: Option<String>,
    svg_code: Option<String>,
    #[serde(default)]
    clear_logo: bool,
}

#[derive(Debug, Deserialize)]
struct LogoQuery {
    v: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/",
            get(get_branding).put(update_branding).delete(reset_branding),
        )
        .route("/logo", get(get_logo))
}

fn clean_panel_name(raw: &str) -> AppResult<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(DEFAULT_PANEL_NAME.to_string());
    }
    if trimmed.chars().count() > 64 {
        return Err(AppError::validation(
            "panelName",
            "Panel name must be at most 64 characters",
        ));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(AppError::validation(
            "panelName",
            "Panel name cannot contain control characters",
        ));
    }
    Ok(trimmed.to_string())
}

fn is_allowed_logo_filename(name: &str) -> bool {
    matches!(name, "logo.svg" | "logo.png" | "logo.webp")
}

fn branding_dir(state: &AppState) -> PathBuf {
    state.cfg.data_dir.join("branding")
}

/// Validates SVG markup to ensure it is well-formed and free from script/XSS vectors.
pub fn validate_safe_svg(raw: &str) -> AppResult<()> {
    let trimmed = raw.trim().trim_start_matches('\u{feff}').trim();
    if trimmed.is_empty() {
        return Err(AppError::validation("logo", "SVG content cannot be empty"));
    }
    if trimmed.len() > MAX_LOGO_BYTES {
        return Err(AppError::validation(
            "logo",
            "Logo exceeds maximum allowed size of 1 MiB",
        ));
    }
    let lower = trimmed.to_ascii_lowercase();
    if !(lower.starts_with("<svg") || lower.starts_with("<?xml")) {
        return Err(AppError::validation(
            "logo",
            "Invalid SVG document: must start with <svg or <?xml",
        ));
    }
    if !lower.contains("<svg") || !lower.contains("</svg>") {
        return Err(AppError::validation(
            "logo",
            "Invalid SVG document: missing <svg>...</svg> element",
        ));
    }

    const FORBIDDEN_PATTERNS: &[&str] = &[
        "<script",
        "</script",
        "<foreignobject",
        "<iframe",
        "<object",
        "<embed",
        "<applet",
        "<!doctype",
        "<!entity",
        "javascript:",
        "vbscript:",
        "data:text/html",
    ];
    for pat in FORBIDDEN_PATTERNS {
        if lower.contains(pat) {
            return Err(AppError::validation(
                "logo",
                format!("Unsafe SVG construct rejected: {pat}"),
            ));
        }
    }

    // Reject inline event handlers like onclick= / onload= / onerror=
    let bytes = lower.as_bytes();
    let mut i = 0usize;
    while i + 3 < bytes.len() {
        let prev_ok = i > 0
            && (bytes[i - 1].is_ascii_whitespace()
                || matches!(bytes[i - 1], b'/' | b'"' | b'\'' | b'>'));
        if prev_ok && bytes[i] == b'o' && bytes[i + 1] == b'n' {
            let mut j = i + 2;
            while j < bytes.len() && bytes[j].is_ascii_alphabetic() {
                j += 1;
            }
            if j > i + 2 {
                let mut k = j;
                while k < bytes.len() && bytes[k].is_ascii_whitespace() {
                    k += 1;
                }
                if k < bytes.len() && bytes[k] == b'=' {
                    return Err(AppError::validation(
                        "logo",
                        "Unsafe SVG attribute rejected: inline event handler",
                    ));
                }
            }
        }
        i += 1;
    }

    Ok(())
}

/// Decodes and validates a custom logo provided as SVG source code or a base64 Data URL
/// (`image/svg+xml`, `image/png`, `image/webp`) up to 1 MiB.
pub fn validate_and_decode_logo(
    logo_data_url: Option<&str>,
    svg_code: Option<&str>,
) -> AppResult<(Vec<u8>, &'static str, &'static str)> {
    if let Some(svg_raw) = svg_code.map(str::trim).filter(|s| !s.is_empty()) {
        validate_safe_svg(svg_raw)?;
        return Ok((svg_raw.as_bytes().to_vec(), "image/svg+xml", "svg"));
    }

    let data_url = logo_data_url
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::validation("logo", "Logo payload is empty"))?;

    // Guard encoded size before allocating decoded buffer (~4/3 overhead + header margin)
    if data_url.len() > (MAX_LOGO_BYTES * 4) / 3 + 256 {
        return Err(AppError::validation(
            "logo",
            "Logo exceeds maximum allowed size of 1 MiB",
        ));
    }

    let Some((header, b64_raw)) = data_url.split_once(',') else {
        return Err(AppError::validation(
            "logo",
            "Logo must be a valid base64 data URL",
        ));
    };

    let header_lower = header.trim().to_ascii_lowercase();
    if !header_lower.starts_with("data:") || !header_lower.ends_with(";base64") {
        return Err(AppError::validation(
            "logo",
            "Unsupported data URL format: expected data:<mime>;base64,...",
        ));
    }
    let mime = header_lower
        .trim_start_matches("data:")
        .split(';')
        .next()
        .unwrap_or("")
        .trim();

    let compact: String = b64_raw.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    let bytes = BASE64
        .decode(compact.as_bytes())
        .map_err(|_| AppError::validation("logo", "Invalid base64 image payload"))?;

    if bytes.is_empty() {
        return Err(AppError::validation("logo", "Uploaded logo is empty"));
    }
    if bytes.len() > MAX_LOGO_BYTES {
        return Err(AppError::validation(
            "logo",
            "Decoded logo exceeds maximum allowed size of 1 MiB",
        ));
    }

    match mime {
        "image/png" => {
            if !bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
                return Err(AppError::validation("logo", "Invalid PNG file signature"));
            }
            Ok((bytes, "image/png", "png"))
        }
        "image/webp" => {
            if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WEBP" {
                return Err(AppError::validation("logo", "Invalid WebP file signature"));
            }
            Ok((bytes, "image/webp", "webp"))
        }
        "image/svg+xml" => {
            let text = std::str::from_utf8(&bytes)
                .map_err(|_| AppError::validation("logo", "SVG must be valid UTF-8"))?;
            validate_safe_svg(text)?;
            Ok((bytes, "image/svg+xml", "svg"))
        }
        _ => Err(AppError::validation(
            "logo",
            "Unsupported logo format: allowed .svg, .png, .webp",
        )),
    }
}

async fn read_meta_from_db(state: &AppState) -> BrandingMeta {
    let row = sqlx::query(r#"SELECT value FROM "SystemSetting" WHERE key = $1"#)
        .bind(SETTING_KEY)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();

    let Some(r) = row else {
        return BrandingMeta::default();
    };

    r.try_get::<Value, _>("value")
        .ok()
        .and_then(|v| serde_json::from_value::<BrandingMeta>(v).ok())
        .unwrap_or_default()
}

pub async fn load_branding(state: &AppState) -> BrandingPublic {
    if let Some(cached) = state.cache.get_json(CACHE_KEY) {
        if let Ok(pub_meta) = serde_json::from_value::<BrandingPublic>(cached) {
            return pub_meta;
        }
    }

    let meta = read_meta_from_db(state).await;
    let logo_exists = match meta.logo_file.as_deref() {
        Some(f) if meta.has_custom_logo && is_allowed_logo_filename(f) => {
            tokio::fs::try_exists(branding_dir(state).join(f))
                .await
                .unwrap_or(false)
        }
        _ => false,
    };

    let public = BrandingPublic::from_meta(&meta, logo_exists);
    if let Ok(val) = serde_json::to_value(&public) {
        state.cache.set_json(CACHE_KEY, val, Duration::from_secs(60));
    }
    public
}

async fn remove_all_logo_files(dir: &PathBuf) {
    for fname in ["logo.svg", "logo.png", "logo.webp"] {
        let _ = tokio::fs::remove_file(dir.join(fname)).await;
    }
}

async fn write_logo_file(dir: &PathBuf, ext: &str, bytes: &[u8]) -> AppResult<String> {
    tokio::fs::create_dir_all(dir)
        .await
        .map_err(|e| AppError::internal(format!("Failed to create branding directory: {e}")))?;

    let target_name = format!("logo.{ext}");
    let tmp_path = dir.join(format!("{target_name}.tmp"));
    let final_path = dir.join(&target_name);

    tokio::fs::write(&tmp_path, bytes)
        .await
        .map_err(|e| AppError::internal(format!("Failed to write logo temp file: {e}")))?;
    tokio::fs::rename(&tmp_path, &final_path)
        .await
        .map_err(|e| AppError::internal(format!("Failed to save logo file: {e}")))?;

    for other_ext in ["svg", "png", "webp"] {
        if other_ext != ext {
            let _ = tokio::fs::remove_file(dir.join(format!("logo.{other_ext}"))).await;
        }
    }

    Ok(target_name)
}

async fn get_branding(State(state): State<AppState>) -> Json<BrandingPublic> {
    Json(load_branding(&state).await)
}

async fn get_logo(
    State(state): State<AppState>,
    Query(q): Query<LogoQuery>,
) -> AppResult<Response> {
    let meta = read_meta_from_db(&state).await;
    let Some(filename) = meta.logo_file.as_deref() else {
        return Err(AppError::not_found("Custom logo not configured"));
    };
    if !meta.has_custom_logo || !is_allowed_logo_filename(filename) {
        return Err(AppError::not_found("Custom logo not configured"));
    }

    let path = branding_dir(&state).join(filename);
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|_| AppError::not_found("Custom logo file not found"))?;

    let mime = meta.logo_mime.as_deref().unwrap_or(match filename {
        "logo.png" => "image/png",
        "logo.webp" => "image/webp",
        _ => "image/svg+xml",
    });

    let cache_ctrl = if q.v.as_deref().is_some_and(|v| !v.is_empty()) {
        HeaderValue::from_static("public, max-age=31536000, immutable")
    } else {
        HeaderValue::from_static("no-cache")
    };

    let mut res = (StatusCode::OK, [(header::CONTENT_TYPE, mime)], Body::from(bytes)).into_response();
    let headers = res.headers_mut();
    headers.insert(header::CACHE_CONTROL, cache_ctrl);
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    if mime == "image/svg+xml" {
        headers.insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("default-src 'none'; style-src 'unsafe-inline'; sandbox"),
        );
    }
    Ok(res)
}

async fn update_branding(
    State(state): State<AppState>,
    user: AuthUser,
    ip: ClientIp,
    headers: HeaderMap,
    AppJson(body): AppJson<UpdateBrandingReq>,
) -> AppResult<Json<BrandingPublic>> {
    user.require_admin()?;

    let mut meta = read_meta_from_db(&state).await;
    if let Some(name_raw) = body.panel_name {
        meta.panel_name = clean_panel_name(&name_raw)?;
    }

    let dir = branding_dir(&state);
    let has_new_logo = body.svg_code.as_deref().is_some_and(|s| !s.trim().is_empty())
        || body
            .logo_data_url
            .as_deref()
            .is_some_and(|s| !s.trim().is_empty());

    if body.clear_logo {
        remove_all_logo_files(&dir).await;
        meta.has_custom_logo = false;
        meta.logo_mime = None;
        meta.logo_file = None;
        meta.logo_version = None;
    } else if has_new_logo {
        let (bytes, mime, ext) =
            validate_and_decode_logo(body.logo_data_url.as_deref(), body.svg_code.as_deref())?;
        let filename = write_logo_file(&dir, ext, &bytes).await?;
        let digest = hex::encode(Sha256::digest(&bytes));
        meta.has_custom_logo = true;
        meta.logo_mime = Some(mime.to_string());
        meta.logo_file = Some(filename);
        meta.logo_version = Some(digest.chars().take(12).collect());
    }

    let val = serde_json::to_value(&meta)
        .map_err(|e| AppError::internal(format!("Failed to serialize branding meta: {e}")))?;
    let now = crate::db::now();
    sqlx::query(
        r#"INSERT INTO "SystemSetting" (id, key, value, description, "updatedAt")
           VALUES ($1, $2, $3, 'Panel name and custom logo branding', $4)
           ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, "updatedAt" = EXCLUDED."updatedAt""#,
    )
    .bind(crate::db::new_id())
    .bind(SETTING_KEY)
    .bind(&val)
    .bind(now)
    .execute(&state.db)
    .await?;

    state.cache.del_prefix("branding:");
    state.cache.del_prefix("settings:");
    state.invalidate_derived();

    audit::spawn(
        &state.db,
        AuditEntry::new("UPDATE", "branding")
            .user(&user.id)
            .resource_id(SETTING_KEY)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );

    Ok(Json(load_branding(&state).await))
}

async fn reset_branding(
    State(state): State<AppState>,
    user: AuthUser,
    ip: ClientIp,
    headers: HeaderMap,
) -> AppResult<Json<BrandingPublic>> {
    user.require_admin()?;

    remove_all_logo_files(&branding_dir(&state)).await;
    sqlx::query(r#"DELETE FROM "SystemSetting" WHERE key = $1"#)
        .bind(SETTING_KEY)
        .execute(&state.db)
        .await?;

    state.cache.del_prefix("branding:");
    state.cache.del_prefix("settings:");
    state.invalidate_derived();

    audit::spawn(
        &state.db,
        AuditEntry::new("RESET", "branding")
            .user(&user.id)
            .resource_id(SETTING_KEY)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );

    Ok(Json(BrandingPublic::from_meta(&BrandingMeta::default(), false)))
}

#[cfg(test)]
mod tests {
    use super::{validate_and_decode_logo, MAX_LOGO_BYTES};
    use base64::engine::general_purpose::STANDARD as BASE64;
    use base64::Engine as _;

    #[test]
    fn accepts_valid_svg_png_and_webp() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><circle cx="16" cy="16" r="10"/></svg>"#;
        let (bytes, mime, ext) = validate_and_decode_logo(None, Some(svg)).unwrap();
        assert_eq!(mime, "image/svg+xml");
        assert_eq!(ext, "svg");
        assert_eq!(bytes, svg.as_bytes());

        let svg_url = format!("data:image/svg+xml;base64,{}", BASE64.encode(svg.as_bytes()));
        let (_, mime_u, ext_u) = validate_and_decode_logo(Some(&svg_url), None).unwrap();
        assert_eq!(mime_u, "image/svg+xml");
        assert_eq!(ext_u, "svg");

        let png = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x01];
        let png_url = format!("data:image/png;base64,{}", BASE64.encode(png));
        let (p_bytes, p_mime, p_ext) = validate_and_decode_logo(Some(&png_url), None).unwrap();
        assert_eq!(p_mime, "image/png");
        assert_eq!(p_ext, "png");
        assert_eq!(p_bytes, png);

        let webp = b"RIFF\x20\x00\x00\x00WEBPVP8 ";
        let webp_url = format!("data:image/webp;base64,{}", BASE64.encode(webp));
        let (w_bytes, w_mime, w_ext) = validate_and_decode_logo(Some(&webp_url), None).unwrap();
        assert_eq!(w_mime, "image/webp");
        assert_eq!(w_ext, "webp");
        assert_eq!(w_bytes, webp);
    }

    #[test]
    fn rejects_xss_svg_vectors() {
        let vectors = [
            r#"<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>"#,
            r#"<svg xmlns="http://www.w3.org/2000/svg" onload="alert(1)"><rect width="10" height="10"/></svg>"#,
            r#"<svg xmlns="http://www.w3.org/2000/svg"><a href="javascript:alert(1)"><rect width="10" height="10"/></a></svg>"#,
            r#"<svg xmlns="http://www.w3.org/2000/svg"><foreignObject><body>hi</body></foreignObject></svg>"#,
        ];
        for v in vectors {
            assert!(validate_and_decode_logo(None, Some(v)).is_err(), "accepted unsafe SVG: {v}");
        }
    }

    #[test]
    fn rejects_oversized_and_mismatched_magic_bytes() {
        let mut huge = b"RIFF\x20\x00\x00\x00WEBP".to_vec();
        huge.resize(MAX_LOGO_BYTES + 1, 0u8);
        let huge_url = format!("data:image/webp;base64,{}", BASE64.encode(&huge));
        assert!(validate_and_decode_logo(Some(&huge_url), None).is_err());

        let fake_png = format!("data:image/png;base64,{}", BASE64.encode(b"not a png"));
        assert!(validate_and_decode_logo(Some(&fake_png), None).is_err());
    }
}
