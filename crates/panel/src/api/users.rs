use crate::api::common::{check_manage, like_pattern, ok, req_str, valid_id, AppJson, OptionalJson};
use crate::audit::{self, AuditEntry};
use crate::auth::crypto::{generate_user_password, sha256_hex};
use crate::auth::extract::AuthUser;
use crate::auth::password::hash_password;
use crate::db::enums::UserRole;
use crate::error::{AppError, AppResult};
use crate::net::{user_agent, ClientIp};
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::NaiveDateTime;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{QueryBuilder, Row};
use std::str::FromStr;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/create-reseller", post(create_reseller))
        .route("/{id}", get(get_one).put(update).delete(remove))
        .route("/{id}/reset-password", post(reset_password))
        .route("/{id}/toggle-ban", post(toggle_ban))
        .route("/{id}/api-keys", get(list_api_keys).post(create_api_key))
        .route("/{user_id}/api-keys/{key_id}", delete(delete_api_key))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListQuery {
    search: Option<String>,
    role: Option<String>,
    reseller_id: Option<String>,
    banned: Option<String>,
    page: Option<String>,
    limit: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateUserReq {
    username: String,
    email: Option<String>,
    password: Option<String>,
    role: Option<UserRole>,
    reseller_id: Option<String>,
    language: Option<String>,
    #[serde(default)]
    generate_password: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateUserReq {
    email: Option<String>,
    password: Option<String>,
    role: Option<UserRole>,
    reseller_id: Option<Option<String>>,
    language: Option<String>,
    banned: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateResellerBundleReq {
    reseller: ResellerInput,
    user: ResellerUserInput,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResellerInput {
    name: String,
    contact_email: Option<String>,
    contact_telegram: Option<String>,
    max_clients: Option<i32>,
    traffic_limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResellerUserInput {
    username: String,
    email: Option<String>,
    password: Option<String>,
    language: Option<String>,
}

fn validate_username(u: &str) -> AppResult<&str> {
    let s = u.trim();
    if s.len() < 3
        || s.len() > 50
        || !s
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(AppError::validation(
            "username",
            "Only letters, numbers, - and _ (3..=50 chars)",
        ));
    }
    Ok(s)
}

async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    let page = q
        .page
        .as_deref()
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(1)
        .max(1);
    let raw_limit = q
        .limit
        .as_deref()
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(20)
        .max(1);
    let limit = raw_limit.min(100);
    let offset = (page - 1).saturating_mul(limit);

    let mut count_qb = QueryBuilder::new(r#"SELECT COUNT(*) FROM "User" WHERE 1=1"#);
    let mut data_qb = QueryBuilder::new(
        r#"SELECT id, username, email, role, "resellerId", language, banned, "lastLoginAt", "lastLoginIp", "createdAt"
           FROM "User" WHERE 1=1"#,
    );

    if let Some(ref s) = q.search.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let pat = like_pattern(s);
        for qb in [&mut count_qb, &mut data_qb] {
            qb.push(r#" AND (username ILIKE "#)
                .push_bind(pat.clone())
                .push(r#" OR email ILIKE "#)
                .push_bind(pat.clone())
                .push(")");
        }
    }
    if let Some(ref role_s) = q.role.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let role = UserRole::from_str(role_s).map_err(AppError::bad_request)?;
        for qb in [&mut count_qb, &mut data_qb] {
            qb.push(r#" AND role = "#).push_bind(role);
        }
    }
    if let Some(ref rid) = q
        .reseller_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        for qb in [&mut count_qb, &mut data_qb] {
            qb.push(r#" AND "resellerId" = "#).push_bind(rid.to_string());
        }
    }
    if let Some(ref b) = q.banned {
        let bv = b == "true";
        for qb in [&mut count_qb, &mut data_qb] {
            qb.push(r#" AND banned = "#).push_bind(bv);
        }
    }

    let total: i64 = count_qb.build_query_scalar().fetch_one(&state.db).await?;
    data_qb
        .push(r#" ORDER BY "createdAt" DESC LIMIT "#)
        .push_bind(limit)
        .push(" OFFSET ")
        .push_bind(offset);

    let rows = data_qb.build().fetch_all(&state.db).await?;
    let mut data = Vec::with_capacity(rows.len());
    for r in rows {
        let created_at: NaiveDateTime = r.try_get("createdAt")?;
        let last_login: Option<NaiveDateTime> = r.try_get("lastLoginAt")?;
        data.push(json!({
            "id": r.try_get::<String, _>("id")?,
            "username": r.try_get::<String, _>("username")?,
            "email": r.try_get::<Option<String>, _>("email")?,
            "role": r.try_get::<UserRole, _>("role")?,
            "resellerId": r.try_get::<Option<String>, _>("resellerId")?,
            "language": r.try_get::<String, _>("language")?,
            "banned": r.try_get::<bool, _>("banned")?,
            "lastLoginAt": last_login.map(|d| crate::db::ts::format_iso(&d)),
            "lastLoginIp": r.try_get::<Option<String>, _>("lastLoginIp")?,
            "createdAt": crate::db::ts::format_iso(&created_at),
        }));
    }

    let pages = ((total + limit - 1) / limit).max(1);
    Ok(Json(json!({
        "data": data,
        "total": total,
        "page": page,
        "pages": pages,
        "limit": limit
    })))
}

async fn get_one(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;

    let r = sqlx::query(
        r#"SELECT id, username, email, role, "resellerId", language, banned, "lastLoginAt", "lastLoginIp", "createdAt", "updatedAt"
           FROM "User" WHERE id = $1"#,
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("User not found"))?;

    let created_at: NaiveDateTime = r.try_get("createdAt")?;
    let updated_at: NaiveDateTime = r.try_get("updatedAt")?;
    let last_login: Option<NaiveDateTime> = r.try_get("lastLoginAt")?;
    Ok(Json(json!({
        "id": r.try_get::<String, _>("id")?,
        "username": r.try_get::<String, _>("username")?,
        "email": r.try_get::<Option<String>, _>("email")?,
        "role": r.try_get::<UserRole, _>("role")?,
        "resellerId": r.try_get::<Option<String>, _>("resellerId")?,
        "language": r.try_get::<String, _>("language")?,
        "banned": r.try_get::<bool, _>("banned")?,
        "lastLoginAt": last_login.map(|d| crate::db::ts::format_iso(&d)),
        "lastLoginIp": r.try_get::<Option<String>, _>("lastLoginIp")?,
        "createdAt": crate::db::ts::format_iso(&created_at),
        "updatedAt": crate::db::ts::format_iso(&updated_at),
    })))
}

async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    ip: ClientIp,
    headers: HeaderMap,
    AppJson(body): AppJson<CreateUserReq>,
) -> AppResult<Response> {
    user.require_admin()?;
    let username = validate_username(&body.username)?;
    let target_role = body.role.unwrap_or(UserRole::Operator);
    if !user.role.can_manage(target_role) {
        return Err(AppError::forbidden("Cannot create user with this role"));
    }

    let exists_u: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "User" WHERE username = $1"#)
        .bind(username)
        .fetch_one(&state.db)
        .await?;
    if exists_u > 0 {
        return Err(AppError::conflict("Username already exists"));
    }

    let email = body
        .email
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string);
    if let Some(ref em) = email {
        let exists_e: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "User" WHERE email = $1"#)
            .bind(em)
            .fetch_one(&state.db)
            .await?;
        if exists_e > 0 {
            return Err(AppError::conflict("Email already exists"));
        }
    }

    let plain_pw = if body.generate_password || body.password.as_deref().is_none_or(str::is_empty) {
        generate_user_password(16)
    } else {
        let p = body.password.unwrap_or_default();
        if p.len() < 8 {
            return Err(AppError::validation("password", "Password must be at least 8 characters"));
        }
        p
    };

    let pw_hash = hash_password(&plain_pw, 12).await?;
    let id = crate::db::new_id();
    let now = crate::db::now();
    let language = body.language.unwrap_or_else(|| "en".to_string());

    sqlx::query(
        r#"INSERT INTO "User" (id, username, email, password, role, "resellerId", language, banned, "createdAt", "updatedAt")
           VALUES ($1, $2, $3, $4, $5, $6, $7, false, $8, $8)"#,
    )
    .bind(&id)
    .bind(username)
    .bind(&email)
    .bind(&pw_hash)
    .bind(target_role)
    .bind(&body.reseller_id)
    .bind(&language)
    .bind(now)
    .execute(&state.db)
    .await?;

    audit::spawn(
        &state.db,
        AuditEntry::new("CREATE", "user")
            .user(&user.id)
            .resource_id(&id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": id,
            "username": username,
            "email": email,
            "role": target_role,
            "resellerId": body.reseller_id,
            "language": language,
            "createdAt": crate::db::ts::format_iso(&now),
            "password": plain_pw
        })),
    )
        .into_response())
}

async fn create_reseller(
    State(state): State<AppState>,
    user: AuthUser,
    ip: ClientIp,
    headers: HeaderMap,
    AppJson(body): AppJson<CreateResellerBundleReq>,
) -> AppResult<Response> {
    user.require_super_admin()?;
    if body.reseller.name.trim().is_empty() {
        return Err(AppError::validation("reseller.name", "Reseller name is required"));
    }
    let username = validate_username(&body.user.username)?;
    let plain_pw = body
        .user
        .password
        .as_deref()
        .filter(|s| s.len() >= 8)
        .map(ToString::to_string)
        .unwrap_or_else(|| generate_user_password(16));
    let pw_hash = hash_password(&plain_pw, 12).await?;

    let mut tx = state.db.begin().await?;
    let now = crate::db::now();
    let reseller_id = crate::db::new_id();
    let user_id = crate::db::new_id();
    let max_clients = body.reseller.max_clients.unwrap_or(100).max(1);
    let traffic_limit = body.reseller.traffic_limit.unwrap_or(107_374_182_400).max(0);
    let language = body.user.language.unwrap_or_else(|| "en".to_string());

    sqlx::query(
        r#"INSERT INTO "Reseller" (id, name, "contactEmail", "contactTelegram", "maxClients", "trafficLimit", "createdAt", "updatedAt")
           VALUES ($1, $2, $3, $4, $5, $6, $7, $7)"#,
    )
    .bind(&reseller_id)
    .bind(body.reseller.name.trim())
    .bind(&body.reseller.contact_email)
    .bind(&body.reseller.contact_telegram)
    .bind(max_clients)
    .bind(traffic_limit)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"INSERT INTO "User" (id, username, email, password, role, "resellerId", language, banned, "createdAt", "updatedAt")
           VALUES ($1, $2, $3, $4, $5, $6, $7, false, $8, $8)"#,
    )
    .bind(&user_id)
    .bind(username)
    .bind(&body.user.email)
    .bind(&pw_hash)
    .bind(UserRole::Reseller)
    .bind(&reseller_id)
    .bind(&language)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    audit::spawn(
        &state.db,
        AuditEntry::new("CREATE", "reseller+user")
            .user(&user.id)
            .resource_id(&reseller_id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "reseller": {
                "id": reseller_id,
                "name": body.reseller.name.trim(),
                "contactEmail": body.reseller.contact_email,
                "contactTelegram": body.reseller.contact_telegram,
                "maxClients": max_clients,
                "maxNodes": 5,
                "trafficLimit": traffic_limit,
                "usedTraffic": 0,
                "balance": "0.00",
                "active": true,
                "createdAt": crate::db::ts::format_iso(&now),
                "updatedAt": crate::db::ts::format_iso(&now)
            },
            "user": {
                "id": user_id,
                "username": username,
                "email": body.user.email,
                "role": UserRole::Reseller,
                "resellerId": reseller_id,
                "language": language,
                "createdAt": crate::db::ts::format_iso(&now),
                "password": plain_pw
            }
        })),
    )
        .into_response())
}

async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    ip: ClientIp,
    headers: HeaderMap,
    AppJson(body): AppJson<UpdateUserReq>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;

    let existing = sqlx::query(
        r#"SELECT id, username, email, role, "resellerId", language, banned, "createdAt"
           FROM "User" WHERE id = $1"#,
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("User not found"))?;

    if id == user.id {
        return Err(AppError::bad_request("Cannot edit your own account here"));
    }

    let target_role: UserRole = existing.try_get("role")?;
    check_manage(user.role, target_role, "edit")?;

    if let Some(new_role) = body.role {
        if !user.role.can_manage(new_role) {
            return Err(AppError::forbidden("Cannot assign this role"));
        }
    }

    let new_pw_hash = match body.password.as_deref() {
        Some(p) if !p.is_empty() => {
            if p.len() < 8 {
                return Err(AppError::validation("password", "Password must be at least 8 characters"));
            }
            Some(hash_password(p, 12).await?)
        }
        _ => None,
    };

    let now = crate::db::now();
    let email = body.email.or_else(|| existing.try_get("email").ok().flatten());
    let role = body.role.unwrap_or(target_role);
    let reseller_id = match body.reseller_id {
        Some(v) => v,
        None => existing.try_get("resellerId")?,
    };
    let language = body
        .language
        .unwrap_or_else(|| existing.try_get("language").unwrap_or_else(|_| "en".into()));
    let banned = body
        .banned
        .unwrap_or_else(|| existing.try_get("banned").unwrap_or(false));

    if let Some(ref hash) = new_pw_hash {
        sqlx::query(
            r#"UPDATE "User" SET email = $2, password = $3, role = $4, "resellerId" = $5, language = $6, banned = $7, "updatedAt" = $8 WHERE id = $1"#,
        )
        .bind(&id)
        .bind(&email)
        .bind(hash)
        .bind(role)
        .bind(&reseller_id)
        .bind(&language)
        .bind(banned)
        .bind(now)
        .execute(&state.db)
        .await?;
    } else {
        sqlx::query(
            r#"UPDATE "User" SET email = $2, role = $3, "resellerId" = $4, language = $5, banned = $6, "updatedAt" = $7 WHERE id = $1"#,
        )
        .bind(&id)
        .bind(&email)
        .bind(role)
        .bind(&reseller_id)
        .bind(&language)
        .bind(banned)
        .bind(now)
        .execute(&state.db)
        .await?;
    }

    if banned || new_pw_hash.is_some() {
        let _ = sqlx::query(r#"DELETE FROM "UserSession" WHERE "userId" = $1"#)
            .bind(&id)
            .execute(&state.db)
            .await;
    }

    audit::spawn(
        &state.db,
        AuditEntry::new("UPDATE", "user")
            .user(&user.id)
            .resource_id(&id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );

    let created_at: NaiveDateTime = existing.try_get("createdAt")?;
    Ok(Json(json!({
        "id": id,
        "username": existing.try_get::<String, _>("username")?,
        "email": email,
        "role": role,
        "resellerId": reseller_id,
        "language": language,
        "banned": banned,
        "createdAt": crate::db::ts::format_iso(&created_at)
    })))
}

async fn remove(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    ip: ClientIp,
    headers: HeaderMap,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;
    if id == user.id {
        return Err(AppError::bad_request("Cannot delete yourself"));
    }

    let target_role: UserRole =
        sqlx::query_scalar(r#"SELECT role FROM "User" WHERE id = $1"#)
            .bind(&id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| AppError::not_found("User not found"))?;

    if target_role == UserRole::SuperAdmin && !user.is_super_admin() {
        return Err(AppError::forbidden("Cannot delete SUPER_ADMIN"));
    }
    check_manage(user.role, target_role, "delete")?;

    sqlx::query(r#"DELETE FROM "UserSession" WHERE "userId" = $1"#)
        .bind(&id)
        .execute(&state.db)
        .await?;
    sqlx::query(r#"DELETE FROM "ApiKey" WHERE "userId" = $1"#)
        .bind(&id)
        .execute(&state.db)
        .await?;
    sqlx::query(r#"DELETE FROM "User" WHERE id = $1"#)
        .bind(&id)
        .execute(&state.db)
        .await?;

    audit::spawn(
        &state.db,
        AuditEntry::new("DELETE", "user")
            .user(&user.id)
            .resource_id(&id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );
    Ok(ok())
}

async fn reset_password(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    ip: ClientIp,
    headers: HeaderMap,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;

    let target_role: UserRole =
        sqlx::query_scalar(r#"SELECT role FROM "User" WHERE id = $1"#)
            .bind(&id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| AppError::not_found("User not found"))?;

    if target_role == UserRole::SuperAdmin && !user.is_super_admin() {
        return Err(AppError::forbidden("Cannot reset SUPER_ADMIN password"));
    }

    let plain = generate_user_password(16);
    let hash = hash_password(&plain, 12).await?;
    sqlx::query(r#"UPDATE "User" SET password = $2, "updatedAt" = $3 WHERE id = $1"#)
        .bind(&id)
        .bind(&hash)
        .bind(crate::db::now())
        .execute(&state.db)
        .await?;

    sqlx::query(r#"DELETE FROM "UserSession" WHERE "userId" = $1"#)
        .bind(&id)
        .execute(&state.db)
        .await?;

    audit::spawn(
        &state.db,
        AuditEntry::new("RESET_PASSWORD", "user")
            .user(&user.id)
            .resource_id(&id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );

    Ok(Json(json!({ "password": plain })))
}

async fn toggle_ban(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    ip: ClientIp,
    headers: HeaderMap,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;
    if id == user.id {
        return Err(AppError::bad_request("Cannot ban yourself"));
    }

    let row = sqlx::query(r#"SELECT role, banned FROM "User" WHERE id = $1"#)
        .bind(&id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("User not found"))?;

    let target_role: UserRole = row.try_get("role")?;
    if target_role == UserRole::SuperAdmin {
        return Err(AppError::forbidden("Cannot ban SUPER_ADMIN"));
    }
    check_manage(user.role, target_role, "ban")?;

    let new_banned = !row.try_get::<bool, _>("banned")?;
    sqlx::query(r#"UPDATE "User" SET banned = $2, "updatedAt" = $3 WHERE id = $1"#)
        .bind(&id)
        .bind(new_banned)
        .bind(crate::db::now())
        .execute(&state.db)
        .await?;

    if new_banned {
        sqlx::query(r#"DELETE FROM "UserSession" WHERE "userId" = $1"#)
            .bind(&id)
            .execute(&state.db)
            .await?;
    }

    audit::spawn(
        &state.db,
        AuditEntry::new("TOGGLE_BAN", "user")
            .user(&user.id)
            .resource_id(&id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );

    Ok(Json(json!({ "id": id, "banned": new_banned })))
}

// Enforce check_manage on API key endpoints when targeting another user
async fn ensure_api_key_target_access(
    state: &AppState,
    actor: &AuthUser,
    target_user_id: &str,
    action: &str,
) -> AppResult<()> {
    if target_user_id == actor.id {
        return Ok(());
    }
    let target_role: UserRole =
        sqlx::query_scalar(r#"SELECT role FROM "User" WHERE id = $1"#)
            .bind(target_user_id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| AppError::not_found("User not found"))?;
    check_manage(actor.role, target_role, action)
}

async fn list_api_keys(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Vec<Value>>> {
    user.require_admin()?;
    valid_id(&id)?;
    ensure_api_key_target_access(&state, &user, &id, "manage API keys for").await?;

    let rows = sqlx::query(
        r#"SELECT id, name, permissions, "expiresAt", "lastUsedAt", "createdAt"
           FROM "ApiKey" WHERE "userId" = $1 ORDER BY "createdAt" DESC"#,
    )
    .bind(&id)
    .fetch_all(&state.db)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let exp: Option<NaiveDateTime> = r.try_get("expiresAt")?;
        let last: Option<NaiveDateTime> = r.try_get("lastUsedAt")?;
        let created: NaiveDateTime = r.try_get("createdAt")?;
        out.push(json!({
            "id": r.try_get::<String, _>("id")?,
            "name": r.try_get::<String, _>("name")?,
            "permissions": r.try_get::<Value, _>("permissions")?,
            "expiresAt": exp.map(|d| crate::db::ts::format_iso(&d)),
            "lastUsedAt": last.map(|d| crate::db::ts::format_iso(&d)),
            "createdAt": crate::db::ts::format_iso(&created),
        }));
    }
    Ok(Json(out))
}

async fn create_api_key(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    ip: ClientIp,
    headers: HeaderMap,
    OptionalJson(body): OptionalJson<Value>,
) -> AppResult<Response> {
    user.require_admin()?;
    valid_id(&id)?;
    ensure_api_key_target_access(&state, &user, &id, "create API keys for").await?;

    let name = req_str(&body, "name")?;
    let permissions = body
        .get("permissions")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let expires_at: Option<NaiveDateTime> = body
        .get("expiresAt")
        .and_then(Value::as_str)
        .map(crate::db::ts::parse_iso)
        .transpose()
        .map_err(AppError::bad_request)?;

    let raw_key = format!("pk_{}", crate::db::new_id().replace('-', ""));
    let key_hash = sha256_hex(&raw_key);
    let key_id = crate::db::new_id();
    let now = crate::db::now();

    sqlx::query(
        r#"INSERT INTO "ApiKey" (id, "userId", key, name, permissions, "expiresAt", "createdAt")
           VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
    )
    .bind(&key_id)
    .bind(&id)
    .bind(&key_hash)
    .bind(name)
    .bind(&permissions)
    .bind(expires_at)
    .bind(now)
    .execute(&state.db)
    .await?;

    audit::spawn(
        &state.db,
        AuditEntry::new("CREATE", "apiKey")
            .user(&user.id)
            .resource_id(&key_id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": key_id,
            "name": name,
            "key": raw_key,
            "permissions": permissions,
            "expiresAt": expires_at.map(|d| crate::db::ts::format_iso(&d)),
            "createdAt": crate::db::ts::format_iso(&now),
        })),
    )
        .into_response())
}

async fn delete_api_key(
    State(state): State<AppState>,
    user: AuthUser,
    Path((user_id, key_id)): Path<(String, String)>,
    ip: ClientIp,
    headers: HeaderMap,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&user_id)?;
    valid_id(&key_id)?;
    ensure_api_key_target_access(&state, &user, &user_id, "revoke API keys for").await?;

    let res = sqlx::query(r#"DELETE FROM "ApiKey" WHERE id = $1 AND "userId" = $2"#)
        .bind(&key_id)
        .bind(&user_id)
        .execute(&state.db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::not_found("API key not found"));
    }

    audit::spawn(
        &state.db,
        AuditEntry::new("DELETE", "apiKey")
            .user(&user.id)
            .resource_id(&key_id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );
    Ok(ok())
}
