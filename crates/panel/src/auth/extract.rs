use crate::db::enums::UserRole;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::{header, HeaderMap, Method};
use chrono::NaiveDateTime;
use serde_json::Value;
use sqlx::Row;

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: String,
    pub username: String,
    pub role: UserRole,
    pub reseller_id: Option<String>,
    pub token: String,
    pub is_api_key: bool,
    pub permissions: Vec<String>,
    pub from_cookie: bool,
}

impl AuthUser {
    pub fn is_super_admin(&self) -> bool {
        self.role == UserRole::SuperAdmin
    }

    pub fn is_admin(&self) -> bool {
        matches!(self.role, UserRole::SuperAdmin | UserRole::Admin)
    }

    pub fn is_reseller(&self) -> bool {
        self.role == UserRole::Reseller
    }

    pub fn require_super_admin(&self) -> AppResult<()> {
        if self.is_super_admin() {
            Ok(())
        } else {
            Err(AppError::forbidden("Forbidden: insufficient permissions"))
        }
    }

    pub fn require_admin(&self) -> AppResult<()> {
        if self.is_admin() {
            Ok(())
        } else {
            Err(AppError::forbidden("Forbidden: insufficient permissions"))
        }
    }

    pub fn require_reseller(&self) -> AppResult<()> {
        if matches!(
            self.role,
            UserRole::SuperAdmin | UserRole::Admin | UserRole::Reseller
        ) {
            Ok(())
        } else {
            Err(AppError::forbidden("Forbidden: insufficient permissions"))
        }
    }

    /// Returns `Some(reseller_id)` when caller is a `RESELLER` (failing with 403 if no tenant is assigned),
    /// or `None` for non-reseller roles.
    pub fn tenant_scope(&self) -> AppResult<Option<String>> {
        if self.role == UserRole::Reseller {
            let rid = self
                .reseller_id
                .as_deref()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| AppError::forbidden("Reseller account has no tenant assigned"))?;
            Ok(Some(rid.to_string()))
        } else {
            Ok(None)
        }
    }
}

pub fn bearer_from_headers(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let tok = raw.strip_prefix("Bearer ")?.trim();
    (!tok.is_empty()).then(|| tok.to_string())
}

pub fn cookie_token_from_headers(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .find_map(|c| c.trim().strip_prefix("auth_token=").map(str::to_string))
        .filter(|t| !t.is_empty())
}

fn api_key_from_headers(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
}

/// Checks if an API key's `permissions` array allows the request method and path.
///
/// An empty permissions array grants nothing. `["*"]` or `["admin"]` or `["write"]` permits all actions;
/// `["read"]` / `["readonly"]` / `["*:read"]` only permits `GET`/`HEAD`/`OPTIONS`.
/// Resource-scoped permissions such as `"clients:write"`, `"clients:read"`, `"nodes:*"` are also matched.
pub fn check_api_key_permissions(
    perms: &[String],
    method: &Method,
    path: &str,
) -> AppResult<()> {
    if perms.is_empty() {
        return Err(AppError::forbidden(
            "Forbidden: API key has no permissions assigned",
        ));
    }
    let is_read = matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS);
    let resource = path
        .trim_start_matches("/api/v1/")
        .split('/')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();

    for raw_p in perms {
        let p = raw_p.trim().to_ascii_lowercase();
        if matches!(p.as_str(), "*" | "all" | "admin" | "full" | "read_write" | "rw") {
            return Ok(());
        }
        if matches!(p.as_str(), "write" | "*:write" | "*:*") {
            return Ok(());
        }
        if is_read && matches!(p.as_str(), "read" | "readonly" | "read_only" | "ro" | "*:read") {
            return Ok(());
        }
        if !resource.is_empty() {
            if p == resource || p == format!("{resource}:*") || p == format!("{resource}:write") {
                return Ok(());
            }
            if is_read && p == format!("{resource}:read") {
                return Ok(());
            }
        }
    }

    Err(AppError::forbidden(
        "Forbidden: API key lacks required permission",
    ))
}

pub async fn resolve(
    state: &AppState,
    token: String,
    from_cookie: bool,
    method: &Method,
    path: &str,
) -> AppResult<AuthUser> {
    // ApiKey authentication when token has no '.' (e.g. `pk_...`)
    if !token.contains('.') {
        let hash = crate::auth::crypto::sha256_hex(&token);
        let row = sqlx::query(
            r#"SELECT k.id AS key_id, k.permissions, k."expiresAt" AS expires_at,
                      u.id AS user_id, u.username, u.role AS "role",
                      u."resellerId" AS reseller_id, u.banned
               FROM "ApiKey" k JOIN "User" u ON u.id = k."userId"
               WHERE k.key = $1"#,
        )
        .bind(&hash)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "api key lookup failed");
            AppError::service_unavailable("Unable to validate API key")
        })?;

        let Some(row) = row else {
            return Err(AppError::unauthorized("Invalid or expired token"));
        };

        let expires_at: Option<NaiveDateTime> = row.try_get("expires_at").ok().flatten();
        if let Some(exp) = expires_at {
            if exp <= crate::db::now() {
                return Err(AppError::unauthorized("API key expired"));
            }
        }
        let banned: bool = row.try_get("banned").unwrap_or(false);
        if banned {
            return Err(AppError::forbidden("Account is banned"));
        }

        let perms_val: Value = row
            .try_get("permissions")
            .unwrap_or_else(|_| Value::Array(Vec::new()));
        let permissions: Vec<String> = perms_val
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .map(ToString::to_string)
                    .collect()
            })
            .unwrap_or_default();

        check_api_key_permissions(&permissions, method, path)?;

        let key_id: String = row.try_get("key_id").unwrap_or_default();
        let user_id: String = row.try_get("user_id").unwrap_or_default();
        let username: String = row.try_get("username").unwrap_or_default();
        let role: UserRole = row.try_get("role").unwrap_or(UserRole::Operator);
        let reseller_id: Option<String> = row.try_get("reseller_id").ok().flatten();

        let db = state.db.clone();
        tokio::spawn(async move {
            let _ = sqlx::query(r#"UPDATE "ApiKey" SET "lastUsedAt" = $2 WHERE id = $1"#)
                .bind(key_id)
                .bind(crate::db::now())
                .execute(&db)
                .await;
        });

        return Ok(AuthUser {
            id: user_id,
            username,
            role,
            reseller_id,
            token,
            is_api_key: true,
            permissions,
            from_cookie: false,
        });
    }

    // JWT + UserSession lookup
    let claims = state.jwt.verify(&token)?;
    let hash = crate::auth::crypto::sha256_hex(&token);
    let row = sqlx::query(
        r#"SELECT s."userId" AS user_id, s."expiresAt" AS expires_at,
                  u.username, u.role AS "role",
                  u."resellerId" AS reseller_id, u.banned
           FROM "UserSession" s JOIN "User" u ON u.id = s."userId"
           WHERE s.token = $1 OR s.token = $2"#,
    )
    .bind(&token)
    .bind(&hash)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "session lookup failed");
        AppError::service_unavailable("Unable to validate session")
    })?;

    let Some(row) = row else {
        return Err(AppError::unauthorized("Session expired or revoked"));
    };
    let user_id: String = row.try_get("user_id").unwrap_or_default();
    let expires_at: NaiveDateTime = row
        .try_get("expires_at")
        .unwrap_or_else(|_| crate::db::now());
    if user_id != claims.id || expires_at <= crate::db::now() {
        return Err(AppError::unauthorized("Session expired or revoked"));
    }
    let banned: bool = row.try_get("banned").unwrap_or(false);
    if banned {
        return Err(AppError::forbidden("Account is banned"));
    }

    let username: String = row.try_get("username").unwrap_or_default();
    let role: UserRole = row.try_get("role").unwrap_or(UserRole::Operator);
    let reseller_id: Option<String> = row.try_get("reseller_id").ok().flatten();

    Ok(AuthUser {
        id: user_id,
        username,
        role,
        reseller_id,
        token,
        is_api_key: false,
        permissions: Vec::new(),
        from_cookie,
    })
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let full_path = parts
            .extensions
            .get::<axum::extract::OriginalUri>()
            .map(|u| u.0.path())
            .unwrap_or_else(|| parts.uri.path());
        if let Some(tok) = bearer_from_headers(&parts.headers) {
            return resolve(state, tok, false, &parts.method, full_path).await;
        }
        if let Some(tok) = api_key_from_headers(&parts.headers) {
            return resolve(state, tok, false, &parts.method, full_path).await;
        }
        if let Some(tok) = cookie_token_from_headers(&parts.headers) {
            return resolve(state, tok, true, &parts.method, full_path).await;
        }
        Err(AppError::unauthorized("Unauthorized"))
    }
}

#[cfg(test)]
mod tests {
    use super::check_api_key_permissions;
    use axum::extract::OriginalUri;
    use axum::http::Method;
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    #[test]
    fn resource_scoped_api_key_permissions_match_full_paths() {
        let read_clients = vec!["clients:read".to_string()];
        assert!(check_api_key_permissions(&read_clients, &Method::GET, "/api/v1/clients").is_ok());
        assert!(check_api_key_permissions(&read_clients, &Method::GET, "/api/v1/clients/123").is_ok());
        assert!(check_api_key_permissions(&read_clients, &Method::POST, "/api/v1/clients").is_err());
        assert!(check_api_key_permissions(&read_clients, &Method::GET, "/api/v1/nodes").is_err());

        // Stripped nested URI ("/" or "/123") must not pass if OriginalUri were lost
        assert!(check_api_key_permissions(&read_clients, &Method::GET, "/").is_err());
        assert!(check_api_key_permissions(&read_clients, &Method::GET, "/123").is_err());

        let write_clients = vec!["clients:write".to_string()];
        assert!(check_api_key_permissions(&write_clients, &Method::POST, "/api/v1/clients").is_ok());
        assert!(check_api_key_permissions(&write_clients, &Method::GET, "/api/v1/clients").is_ok());
        assert!(check_api_key_permissions(&write_clients, &Method::DELETE, "/api/v1/nodes/1").is_err());

        let wild_nodes = vec!["nodes:*".to_string()];
        assert!(check_api_key_permissions(&wild_nodes, &Method::POST, "/api/v1/nodes/n1/restart").is_ok());
        assert!(check_api_key_permissions(&wild_nodes, &Method::GET, "/api/v1/clients").is_err());
    }

    #[tokio::test]
    async fn nested_router_preserves_original_uri_for_api_key_scope() {
        async fn probe(req: axum::extract::Request) -> axum::http::StatusCode {
            let full_path = req
                .extensions()
                .get::<OriginalUri>()
                .map(|u| u.0.path())
                .unwrap_or_else(|| req.uri().path());
            let perms = vec!["clients:read".to_string()];
            if check_api_key_permissions(&perms, req.method(), full_path).is_ok() {
                axum::http::StatusCode::OK
            } else {
                axum::http::StatusCode::FORBIDDEN
            }
        }

        let clients_router = Router::new().route("/", get(probe)).route("/{id}", get(probe));
        let v1 = Router::new().nest("/clients", clients_router);
        let app = Router::new().nest("/api/v1", v1);

        let res_list = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/clients")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_list.status(), axum::http::StatusCode::OK);

        let res_one = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/v1/clients/abc")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res_one.status(), axum::http::StatusCode::OK);
    }
}
