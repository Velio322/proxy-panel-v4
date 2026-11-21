use crate::api::common::{ok, valid_id, AppJson};
use crate::auth::extract::AuthUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::NaiveDateTime;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{QueryBuilder, Row};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/bulk", post(bulk_action))
        .route("/{id}", get(get_one).put(update).delete(remove))
        .route("/{id}/toggle", post(toggle))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListQuery {
    inbound_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BulkHostsReq {
    action: String,
    host_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpsertHostReq {
    inbound_id: Option<String>,
    remark: String,
    address: String,
    port: Option<i32>,
    sni: Option<String>,
    host: Option<String>,
    path: Option<String>,
    alpn: Option<String>,
    fingerprint: Option<String>,
    xhttp_extra: Option<Value>,
    final_mask: Option<Value>,
    vless_route_id: Option<String>,
    #[serde(default)]
    shuffle_host: bool,
    fragment: Option<Value>,
    #[serde(default)]
    is_cdn: bool,
    #[serde(default)]
    sort_order: i32,
    enable: Option<bool>,
    #[serde(default)]
    tags: Vec<String>,
    allow_insecure: Option<bool>,
    pinned_peer_cert_sha256: Option<String>,
    verify_peer_cert_by_name: Option<String>,
    mihomo_ip_version: Option<String>,
    server_description: Option<String>,
}

fn host_json(r: &sqlx::postgres::PgRow) -> AppResult<Value> {
    let created: NaiveDateTime = r.try_get("createdAt")?;
    let updated: NaiveDateTime = r.try_get("updatedAt")?;
    Ok(json!({
        "id": r.try_get::<String, _>("id")?,
        "inboundId": r.try_get::<Option<String>, _>("inboundId")?,
        "remark": r.try_get::<String, _>("remark")?,
        "address": r.try_get::<String, _>("address")?,
        "port": r.try_get::<Option<i32>, _>("port")?,
        "sni": r.try_get::<Option<String>, _>("sni")?,
        "host": r.try_get::<Option<String>, _>("host")?,
        "path": r.try_get::<Option<String>, _>("path")?,
        "alpn": r.try_get::<Option<String>, _>("alpn")?,
        "fingerprint": r.try_get::<Option<String>, _>("fingerprint")?,
        "xhttpExtra": r.try_get::<Option<Value>, _>("xhttpExtra")?,
        "finalMask": r.try_get::<Option<Value>, _>("finalMask")?,
        "vlessRouteId": r.try_get::<Option<String>, _>("vlessRouteId")?,
        "shuffleHost": r.try_get::<bool, _>("shuffleHost")?,
        "fragment": r.try_get::<Option<Value>, _>("fragment")?,
        "isCdn": r.try_get::<bool, _>("isCdn")?,
        "sortOrder": r.try_get::<i32, _>("sortOrder")?,
        "enable": r.try_get::<bool, _>("enable")?,
        "tags": r.try_get::<Option<Vec<String>>, _>("tags").ok().flatten().unwrap_or_default(),
        "allowInsecure": r.try_get::<Option<bool>, _>("allowInsecure").ok().flatten(),
        "pinnedPeerCertSha256": r.try_get::<Option<String>, _>("pinnedPeerCertSha256").ok().flatten(),
        "verifyPeerCertByName": r.try_get::<Option<String>, _>("verifyPeerCertByName").ok().flatten(),
        "mihomoIpVersion": r.try_get::<Option<String>, _>("mihomoIpVersion").ok().flatten(),
        "serverDescription": r.try_get::<Option<String>, _>("serverDescription").ok().flatten(),
        "createdAt": crate::db::ts::format_iso(&created),
        "updatedAt": crate::db::ts::format_iso(&updated),
    }))
}

async fn list(
    State(state): State<AppState>,
    _user: AuthUser,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<Value>>> {
    let mut qb = QueryBuilder::new(
        r#"SELECT id, "inboundId", remark, address, port, sni, host, path, alpn, fingerprint,
                  "xhttpExtra", "finalMask", "vlessRouteId", "shuffleHost", fragment, "isCdn",
                  "sortOrder", enable, COALESCE(tags, ARRAY[]::TEXT[]) AS tags,
                  "allowInsecure", "pinnedPeerCertSha256", "verifyPeerCertByName",
                  "mihomoIpVersion", "serverDescription", "createdAt", "updatedAt"
           FROM "Host" WHERE 1=1"#,
    );
    if let Some(ref iid) = q.inbound_id.as_deref().filter(|s| !s.is_empty()) {
        qb.push(r#" AND "inboundId" = "#).push_bind(iid.to_string());
    }
    qb.push(r#" ORDER BY "sortOrder" ASC, "createdAt" DESC"#);

    let rows = qb.build().fetch_all(&state.db).await?;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        out.push(host_json(&r)?);
    }
    Ok(Json(out))
}

async fn get_one(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    valid_id(&id)?;
    let r = sqlx::query(
        r#"SELECT id, "inboundId", remark, address, port, sni, host, path, alpn, fingerprint,
                  "xhttpExtra", "finalMask", "vlessRouteId", "shuffleHost", fragment, "isCdn",
                  "sortOrder", enable, COALESCE(tags, ARRAY[]::TEXT[]) AS tags,
                  "allowInsecure", "pinnedPeerCertSha256", "verifyPeerCertByName",
                  "mihomoIpVersion", "serverDescription", "createdAt", "updatedAt"
           FROM "Host" WHERE id = $1"#,
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("Host not found"))?;
    Ok(Json(host_json(&r)?))
}

async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    AppJson(body): AppJson<UpsertHostReq>,
) -> AppResult<Response> {
    user.require_admin()?;
    if body.remark.trim().is_empty() || body.address.trim().is_empty() {
        return Err(AppError::validation("address", "remark and address are required"));
    }
    let id = crate::db::new_id();
    let now = crate::db::now();
    let r = sqlx::query(
        r#"INSERT INTO "Host" (id, "inboundId", remark, address, port, sni, host, path, alpn, fingerprint,
                               "xhttpExtra", "finalMask", "vlessRouteId", "shuffleHost", fragment, "isCdn",
                               "sortOrder", enable, tags, "allowInsecure", "pinnedPeerCertSha256",
                               "verifyPeerCertByName", "mihomoIpVersion", "serverDescription", "createdAt", "updatedAt")
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $25)
           RETURNING id, "inboundId", remark, address, port, sni, host, path, alpn, fingerprint,
                     "xhttpExtra", "finalMask", "vlessRouteId", "shuffleHost", fragment, "isCdn",
                     "sortOrder", enable, COALESCE(tags, ARRAY[]::TEXT[]) AS tags,
                     "allowInsecure", "pinnedPeerCertSha256", "verifyPeerCertByName",
                     "mihomoIpVersion", "serverDescription", "createdAt", "updatedAt""#,
    )
    .bind(&id)
    .bind(&body.inbound_id)
    .bind(body.remark.trim())
    .bind(body.address.trim())
    .bind(body.port)
    .bind(&body.sni)
    .bind(&body.host)
    .bind(&body.path)
    .bind(&body.alpn)
    .bind(&body.fingerprint)
    .bind(&body.xhttp_extra)
    .bind(&body.final_mask)
    .bind(&body.vless_route_id)
    .bind(body.shuffle_host)
    .bind(&body.fragment)
    .bind(body.is_cdn)
    .bind(body.sort_order)
    .bind(body.enable.unwrap_or(true))
    .bind(&body.tags)
    .bind(body.allow_insecure)
    .bind(&body.pinned_peer_cert_sha256)
    .bind(&body.verify_peer_cert_by_name)
    .bind(&body.mihomo_ip_version)
    .bind(&body.server_description)
    .bind(now)
    .fetch_one(&state.db)
    .await?;

    state.invalidate_derived();
    Ok((StatusCode::CREATED, Json(host_json(&r)?)).into_response())
}

async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    AppJson(body): AppJson<UpsertHostReq>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;
    let now = crate::db::now();
    let r = sqlx::query(
        r#"UPDATE "Host"
           SET "inboundId" = $2, remark = $3, address = $4, port = $5, sni = $6, host = $7,
               path = $8, alpn = $9, fingerprint = $10, "xhttpExtra" = $11, "finalMask" = $12,
               "vlessRouteId" = $13, "shuffleHost" = $14, fragment = $15, "isCdn" = $16,
               "sortOrder" = $17, enable = $18, tags = $19, "allowInsecure" = $20,
               "pinnedPeerCertSha256" = $21, "verifyPeerCertByName" = $22,
               "mihomoIpVersion" = $23, "serverDescription" = $24, "updatedAt" = $25
           WHERE id = $1
           RETURNING id, "inboundId", remark, address, port, sni, host, path, alpn, fingerprint,
                     "xhttpExtra", "finalMask", "vlessRouteId", "shuffleHost", fragment, "isCdn",
                     "sortOrder", enable, COALESCE(tags, ARRAY[]::TEXT[]) AS tags,
                     "allowInsecure", "pinnedPeerCertSha256", "verifyPeerCertByName",
                     "mihomoIpVersion", "serverDescription", "createdAt", "updatedAt""#,
    )
    .bind(&id)
    .bind(&body.inbound_id)
    .bind(body.remark.trim())
    .bind(body.address.trim())
    .bind(body.port)
    .bind(&body.sni)
    .bind(&body.host)
    .bind(&body.path)
    .bind(&body.alpn)
    .bind(&body.fingerprint)
    .bind(&body.xhttp_extra)
    .bind(&body.final_mask)
    .bind(&body.vless_route_id)
    .bind(body.shuffle_host)
    .bind(&body.fragment)
    .bind(body.is_cdn)
    .bind(body.sort_order)
    .bind(body.enable.unwrap_or(true))
    .bind(&body.tags)
    .bind(body.allow_insecure)
    .bind(&body.pinned_peer_cert_sha256)
    .bind(&body.verify_peer_cert_by_name)
    .bind(&body.mihomo_ip_version)
    .bind(&body.server_description)
    .bind(now)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("Host not found"))?;

    state.invalidate_derived();
    Ok(Json(host_json(&r)?))
}

async fn remove(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;
    let res = sqlx::query(r#"DELETE FROM "Host" WHERE id = $1"#)
        .bind(&id)
        .execute(&state.db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::not_found("Host not found"));
    }
    state.invalidate_derived();
    Ok(ok())
}

async fn toggle(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;
    let r = sqlx::query(
        r#"UPDATE "Host"
           SET enable = NOT enable, "updatedAt" = $2
           WHERE id = $1
           RETURNING id, "inboundId", remark, address, port, sni, host, path, alpn, fingerprint,
                     "xhttpExtra", "finalMask", "vlessRouteId", "shuffleHost", fragment, "isCdn",
                     "sortOrder", enable, COALESCE(tags, ARRAY[]::TEXT[]) AS tags,
                     "allowInsecure", "pinnedPeerCertSha256", "verifyPeerCertByName",
                     "mihomoIpVersion", "serverDescription", "createdAt", "updatedAt""#,
    )
    .bind(&id)
    .bind(crate::db::now())
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("Host not found"))?;

    state.invalidate_derived();
    Ok(Json(host_json(&r)?))
}

async fn bulk_action(
    State(state): State<AppState>,
    user: AuthUser,
    AppJson(body): AppJson<BulkHostsReq>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    if body.host_ids.is_empty() {
        return Ok(json!({ "affected": 0 }).into());
    }
    for hid in &body.host_ids {
        valid_id(hid)?;
    }
    let now = crate::db::now();
    let affected = match body.action.as_str() {
        "enable" => {
            sqlx::query(r#"UPDATE "Host" SET enable = true, "updatedAt" = $2 WHERE id = ANY($1::text[])"#)
                .bind(&body.host_ids)
                .bind(now)
                .execute(&state.db)
                .await?
                .rows_affected()
        }
        "disable" => {
            sqlx::query(r#"UPDATE "Host" SET enable = false, "updatedAt" = $2 WHERE id = ANY($1::text[])"#)
                .bind(&body.host_ids)
                .bind(now)
                .execute(&state.db)
                .await?
                .rows_affected()
        }
        "delete" => {
            sqlx::query(r#"DELETE FROM "Host" WHERE id = ANY($1::text[])"#)
                .bind(&body.host_ids)
                .execute(&state.db)
                .await?
                .rows_affected()
        }
        _ => return Err(AppError::validation("action", "Unsupported bulk action")),
    };
    state.invalidate_derived();
    Ok(Json(json!({ "affected": affected })))
}
