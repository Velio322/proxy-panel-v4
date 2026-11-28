use crate::api::common::{ok, valid_id, AppJson};
use crate::auth::extract::AuthUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use sqlx::Row;
use std::collections::HashMap;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(get_all).put(batch_update))
        .route("/{key}", get(get_one).put(upsert_one).delete(remove_one))
}

#[derive(Debug, Deserialize)]
struct UpsertSettingReq {
    value: Value,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BatchSettingsReq {
    settings: HashMap<String, Value>,
}

const SECRET_MASK: &str = "********";

fn is_secret_setting_key(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    k.contains("token")
        || k.contains("secret")
        || k.contains("password")
        || k.ends_with("_key")
        || k == "smtp_pass"
}

fn mask_if_secret(key: &str, val: Value) -> Value {
    if !is_secret_setting_key(key) {
        return val;
    }
    match &val {
        Value::String(s) if !s.trim().is_empty() => Value::String(SECRET_MASK.to_string()),
        _ => val,
    }
}

async fn get_all(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<Value>> {
    user.require_super_admin()?;
    let rows = sqlx::query(r#"SELECT key, value FROM "SystemSetting""#)
        .fetch_all(&state.db)
        .await?;
    let mut map = Map::new();
    for r in rows {
        let k: String = r.try_get("key")?;
        let v: Value = r.try_get("value")?;
        let masked = mask_if_secret(&k, v);
        map.insert(k, masked);
    }
    Ok(Json(Value::Object(map)))
}

async fn get_one(
    State(state): State<AppState>,
    user: AuthUser,
    Path(key): Path<String>,
) -> AppResult<Json<Value>> {
    user.require_super_admin()?;
    valid_id(&key)?;
    let r = sqlx::query(r#"SELECT key, value, description FROM "SystemSetting" WHERE key = $1"#)
        .bind(&key)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("Setting not found"))?;

    let k: String = r.try_get("key")?;
    let v: Value = r.try_get("value")?;
    Ok(Json(json!({
        "key": k,
        "value": mask_if_secret(&key, v),
        "description": r.try_get::<Option<String>, _>("description")?,
    })))
}

async fn upsert_one(
    State(state): State<AppState>,
    user: AuthUser,
    Path(key): Path<String>,
    AppJson(body): AppJson<UpsertSettingReq>,
) -> AppResult<Json<Value>> {
    user.require_super_admin()?;
    valid_id(&key)?;
    if is_secret_setting_key(&key) && body.value.as_str() == Some(SECRET_MASK) {
        return Ok(Json(json!({ "key": key, "value": SECRET_MASK })));
    }
    let now = crate::db::now();
    sqlx::query(
        r#"INSERT INTO "SystemSetting" (id, key, value, description, "updatedAt")
           VALUES ($1, $2, $3, $4, $5)
           ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, description = COALESCE(EXCLUDED.description, "SystemSetting".description), "updatedAt" = EXCLUDED."updatedAt""#,
    )
    .bind(crate::db::new_id())
    .bind(&key)
    .bind(&body.value)
    .bind(&body.description)
    .bind(now)
    .execute(&state.db)
    .await?;

    state.cache.del_prefix("settings:");
    state.invalidate_derived();
    Ok(Json(json!({ "key": key, "value": body.value })))
}

async fn remove_one(
    State(state): State<AppState>,
    user: AuthUser,
    Path(key): Path<String>,
) -> AppResult<Json<Value>> {
    user.require_super_admin()?;
    valid_id(&key)?;
    let res = sqlx::query(r#"DELETE FROM "SystemSetting" WHERE key = $1"#)
        .bind(&key)
        .execute(&state.db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::not_found("Setting not found"));
    }
    state.cache.del_prefix("settings:");
    state.invalidate_derived();
    Ok(ok())
}

async fn batch_update(
    State(state): State<AppState>,
    user: AuthUser,
    AppJson(body): AppJson<BatchSettingsReq>,
) -> AppResult<Json<Value>> {
    user.require_super_admin()?;
    let now = crate::db::now();
    for (k, v) in body.settings {
        valid_id(&k)?;
        if is_secret_setting_key(&k) && v.as_str() == Some(SECRET_MASK) {
            continue;
        }
        sqlx::query(
            r#"INSERT INTO "SystemSetting" (id, key, value, "updatedAt")
               VALUES ($1, $2, $3, $4)
               ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, "updatedAt" = EXCLUDED."updatedAt""#,
        )
        .bind(crate::db::new_id())
        .bind(&k)
        .bind(&v)
        .bind(now)
        .execute(&state.db)
        .await?;
    }
    state.cache.del_prefix("settings:");
    state.invalidate_derived();
    Ok(ok())
}
