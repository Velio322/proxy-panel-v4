use crate::api::common::{ok, valid_id, AppJson};
use crate::auth::extract::AuthUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::NaiveDateTime;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_templates).post(create_template))
        .route("/srr", get(list_srr).post(create_srr))
        .route("/srr/reorder", post(reorder_srr))
        .route("/srr/{id}", get(get_srr).put(update_srr).delete(delete_srr))
        .route("/{id}/preview", post(preview_template))
        .route("/{id}", get(get_template).put(update_template).delete(delete_template))
}

pub fn srr_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_srr).post(create_srr))
        .route("/reorder", post(reorder_srr))
        .route("/{id}", get(get_srr).put(update_srr).delete(delete_srr))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReorderSrrReq {
    rule_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpsertTemplateReq {
    name: String,
    template_type: String,
    content: String,
    #[serde(default)]
    is_default: bool,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpsertSrrReq {
    name: String,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    priority: i32,
    #[serde(default = "default_header")]
    match_header: String,
    #[serde(default = "default_op")]
    match_operator: String,
    #[serde(default)]
    match_pattern: String,
    #[serde(default)]
    negate: bool,
    #[serde(default)]
    conditions: Value,
    #[serde(default = "default_action")]
    action: String,
    template_id: Option<String>,
    override_format: Option<String>,
    custom_remarks: Option<String>,
    age_public_key: Option<String>,
    age_config: Option<Value>,
}

fn default_true() -> bool {
    true
}
fn default_header() -> String {
    "user-agent".into()
}
fn default_op() -> String {
    "CONTAINS".into()
}
fn default_action() -> String {
    "ALLOW".into()
}

fn tpl_json(r: &sqlx::postgres::PgRow) -> AppResult<Value> {
    let ca: NaiveDateTime = r.try_get("createdAt")?;
    let ua: NaiveDateTime = r.try_get("updatedAt")?;
    Ok(json!({
        "id": r.try_get::<String, _>("id")?,
        "name": r.try_get::<String, _>("name")?,
        "templateType": r.try_get::<String, _>("templateType")?,
        "content": r.try_get::<String, _>("content")?,
        "isDefault": r.try_get::<bool, _>("isDefault")?,
        "description": r.try_get::<Option<String>, _>("description")?,
        "createdAt": crate::db::ts::format_iso(&ca),
        "updatedAt": crate::db::ts::format_iso(&ua),
    }))
}

fn srr_json(r: &sqlx::postgres::PgRow) -> AppResult<Value> {
    let ca: NaiveDateTime = r.try_get("createdAt")?;
    let ua: NaiveDateTime = r.try_get("updatedAt")?;
    Ok(json!({
        "id": r.try_get::<String, _>("id")?,
        "name": r.try_get::<String, _>("name")?,
        "enabled": r.try_get::<bool, _>("enabled")?,
        "priority": r.try_get::<i32, _>("priority")?,
        "matchHeader": r.try_get::<String, _>("matchHeader")?,
        "matchOperator": r.try_get::<String, _>("matchOperator")?,
        "matchPattern": r.try_get::<String, _>("matchPattern")?,
        "negate": r.try_get::<bool, _>("negate")?,
        "conditions": r.try_get::<Value, _>("conditions")?,
        "action": r.try_get::<String, _>("action")?,
        "templateId": r.try_get::<Option<String>, _>("templateId")?,
        "overrideFormat": r.try_get::<Option<String>, _>("overrideFormat")?,
        "customRemarks": r.try_get::<Option<String>, _>("customRemarks")?,
        "agePublicKey": r.try_get::<Option<String>, _>("agePublicKey")?,
        "ageConfig": r.try_get::<Option<Value>, _>("ageConfig")?,
        "createdAt": crate::db::ts::format_iso(&ca),
        "updatedAt": crate::db::ts::format_iso(&ua),
    }))
}

async fn list_templates(State(state): State<AppState>, _user: AuthUser) -> AppResult<Json<Vec<Value>>> {
    let rows = sqlx::query(
        r#"SELECT id, name, "templateType", content, "isDefault", description, "createdAt", "updatedAt"
           FROM "SubscriptionTemplate" ORDER BY "createdAt" DESC"#,
    )
    .fetch_all(&state.db)
    .await?;
    rows.iter().map(tpl_json).collect::<AppResult<Vec<_>>>().map(Json)
}

async fn get_template(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    valid_id(&id)?;
    let r = sqlx::query(
        r#"SELECT id, name, "templateType", content, "isDefault", description, "createdAt", "updatedAt"
           FROM "SubscriptionTemplate" WHERE id = $1"#,
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("Template not found"))?;
    Ok(Json(tpl_json(&r)?))
}

async fn create_template(
    State(state): State<AppState>,
    user: AuthUser,
    AppJson(body): AppJson<UpsertTemplateReq>,
) -> AppResult<Response> {
    user.require_admin()?;
    let id = crate::db::new_id();
    let now = crate::db::now();
    let r = sqlx::query(
        r#"INSERT INTO "SubscriptionTemplate" (id, name, "templateType", content, "isDefault", description, "createdAt", "updatedAt")
           VALUES ($1, $2, $3, $4, $5, $6, $7, $7)
           RETURNING id, name, "templateType", content, "isDefault", description, "createdAt", "updatedAt""#,
    )
    .bind(&id)
    .bind(body.name.trim())
    .bind(body.template_type.trim())
    .bind(&body.content)
    .bind(body.is_default)
    .bind(&body.description)
    .bind(now)
    .fetch_one(&state.db)
    .await?;
    state.invalidate_derived();
    Ok((StatusCode::CREATED, Json(tpl_json(&r)?)).into_response())
}

async fn update_template(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    AppJson(body): AppJson<UpsertTemplateReq>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;
    let r = sqlx::query(
        r#"UPDATE "SubscriptionTemplate"
           SET name = $2, "templateType" = $3, content = $4, "isDefault" = $5, description = $6, "updatedAt" = $7
           WHERE id = $1
           RETURNING id, name, "templateType", content, "isDefault", description, "createdAt", "updatedAt""#,
    )
    .bind(&id)
    .bind(body.name.trim())
    .bind(body.template_type.trim())
    .bind(&body.content)
    .bind(body.is_default)
    .bind(&body.description)
    .bind(crate::db::now())
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("Template not found"))?;
    state.invalidate_derived();
    Ok(Json(tpl_json(&r)?))
}

async fn delete_template(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;
    let res = sqlx::query(r#"DELETE FROM "SubscriptionTemplate" WHERE id = $1"#)
        .bind(&id)
        .execute(&state.db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::not_found("Template not found"));
    }
    state.invalidate_derived();
    Ok(ok())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreviewTemplateReq {
    #[serde(default)]
    client_id: Option<String>,
}

fn parse_template_type_str(s: &str) -> mypanel_proto::subscription::TemplateType {
    use mypanel_proto::subscription::TemplateType;
    match s.trim().to_ascii_uppercase().as_str() {
        "XRAY_JSON" | "ORIGINAL" => TemplateType::XrayJson,
        "XRAY_BASE64" | "BASE64" | "HAPP" => TemplateType::XrayBase64,
        "CLASH" => TemplateType::Clash,
        "STASH" => TemplateType::Stash,
        "SINGBOX" | "SING_BOX" | "SING-BOX" => TemplateType::Singbox,
        _ => TemplateType::Mihomo,
    }
}

async fn preview_template(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<String>,
    body: Option<Json<PreviewTemplateReq>>,
) -> AppResult<Json<Value>> {
    valid_id(&id)?;
    let r = sqlx::query(
        r#"SELECT id, name, "templateType", content FROM "SubscriptionTemplate" WHERE id = $1"#,
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("Template not found"))?;

    let tpl_name: String = r.try_get("name")?;
    let tpl_type_str: String = r.try_get("templateType")?;
    let content: String = r.try_get("content")?;

    let tpl = mypanel_proto::subscription::SubscriptionTemplate {
        id,
        name: tpl_name,
        template_type: parse_template_type_str(&tpl_type_str),
        content,
    };

    let (username, uuid): (String, String) = if let Some(ref cid) = body.as_ref().and_then(|b| b.client_id.as_ref()) {
        if let Ok(Some(crow)) =
            sqlx::query(r#"SELECT username, uuid FROM "Client" WHERE id = $1"#)
                .bind(cid)
                .fetch_optional(&state.db)
                .await
        {
            (
                crow.try_get("username").unwrap_or_else(|_| "preview-user".into()),
                crow.try_get("uuid")
                    .unwrap_or_else(|_| "550e8400-e29b-41d4-a716-446655440000".into()),
            )
        } else {
            (
                "preview-user".into(),
                "550e8400-e29b-41d4-a716-446655440000".into(),
            )
        }
    } else {
        (
            "preview-user".into(),
            "550e8400-e29b-41d4-a716-446655440000".into(),
        )
    };

    let ctx = mypanel_proto::links::EntryContext {
        node_host: "edge.example.com".into(),
        port: 443,
        client_uuid: uuid.clone(),
        client_id: username.clone(),
        secret: "preview-secret".into(),
    };

    let mut entries = Vec::new();
    if let Ok(Some(e1)) = mypanel_proto::links::build_entry(
        mypanel_proto::Protocol::Vless,
        "DE-XHTTP-Reality",
        &json!({ "decryption": "none" }),
        &json!({
            "network": "xhttp",
            "security": "reality",
            "sni": "www.microsoft.com",
            "fingerprint": "chrome",
            "publicKey": "example-public-key",
            "shortId": "abcd1234",
            "xhttpSettings": { "path": "/xh", "mode": "packet-up" }
        }),
        &ctx,
    ) {
        entries.push(e1);
    }
    if let Ok(Some(e2)) = mypanel_proto::links::build_entry(
        mypanel_proto::Protocol::Hysteria2,
        "FI-Hysteria2",
        &json!({ "sni": "hy2.example.com", "portRange": "20000-20063" }),
        &json!({}),
        &ctx,
    ) {
        entries.push(e2);
    }

    let info = mypanel_proto::subscription::UserInfo {
        username,
        uuid,
        upload: 1024 * 1024 * 1024,
        download: 4 * 1024 * 1024 * 1024,
        total: 100 * 1024 * 1024 * 1024,
        expire: 1_900_000_000,
        ..Default::default()
    };

    let rendered = mypanel_proto::subscription::apply_template(&tpl, &entries, &info)
        .map_err(|e| AppError::bad_request(e.to_string()))?;

    Ok(Json(json!({
        "rendered": rendered.body,
        "contentType": rendered.content_type
    })))
}

async fn list_srr(State(state): State<AppState>, _user: AuthUser) -> AppResult<Json<Vec<Value>>> {
    let rows = sqlx::query(
        r#"SELECT id, name, enabled, priority, "matchHeader", "matchOperator", "matchPattern", negate,
                  conditions, action, "templateId", "overrideFormat", "customRemarks", "agePublicKey",
                  "ageConfig", "createdAt", "updatedAt"
           FROM "SrrRule" ORDER BY priority ASC, "createdAt" ASC"#,
    )
    .fetch_all(&state.db)
    .await?;
    rows.iter().map(srr_json).collect::<AppResult<Vec<_>>>().map(Json)
}

async fn get_srr(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    valid_id(&id)?;
    let r = sqlx::query(
        r#"SELECT id, name, enabled, priority, "matchHeader", "matchOperator", "matchPattern", negate,
                  conditions, action, "templateId", "overrideFormat", "customRemarks", "agePublicKey",
                  "ageConfig", "createdAt", "updatedAt"
           FROM "SrrRule" WHERE id = $1"#,
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("SRR rule not found"))?;
    Ok(Json(srr_json(&r)?))
}

async fn create_srr(
    State(state): State<AppState>,
    user: AuthUser,
    AppJson(body): AppJson<UpsertSrrReq>,
) -> AppResult<Response> {
    user.require_admin()?;
    let id = crate::db::new_id();
    let now = crate::db::now();
    let conds = if body.conditions.is_array() {
        body.conditions
    } else {
        json!([])
    };
    let r = sqlx::query(
        r#"INSERT INTO "SrrRule" (id, name, enabled, priority, "matchHeader", "matchOperator", "matchPattern", negate,
                                  conditions, action, "templateId", "overrideFormat", "customRemarks", "agePublicKey",
                                  "ageConfig", "createdAt", "updatedAt")
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $16)
           RETURNING id, name, enabled, priority, "matchHeader", "matchOperator", "matchPattern", negate,
                     conditions, action, "templateId", "overrideFormat", "customRemarks", "agePublicKey",
                     "ageConfig", "createdAt", "updatedAt""#,
    )
    .bind(&id)
    .bind(body.name.trim())
    .bind(body.enabled)
    .bind(body.priority)
    .bind(&body.match_header)
    .bind(&body.match_operator)
    .bind(&body.match_pattern)
    .bind(body.negate)
    .bind(conds)
    .bind(&body.action)
    .bind(&body.template_id)
    .bind(&body.override_format)
    .bind(&body.custom_remarks)
    .bind(&body.age_public_key)
    .bind(&body.age_config)
    .bind(now)
    .fetch_one(&state.db)
    .await?;
    state.invalidate_derived();
    Ok((StatusCode::CREATED, Json(srr_json(&r)?)).into_response())
}

async fn update_srr(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    AppJson(body): AppJson<UpsertSrrReq>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;
    let conds = if body.conditions.is_array() {
        body.conditions
    } else {
        json!([])
    };
    let r = sqlx::query(
        r#"UPDATE "SrrRule"
           SET name = $2, enabled = $3, priority = $4, "matchHeader" = $5, "matchOperator" = $6,
               "matchPattern" = $7, negate = $8, conditions = $9, action = $10, "templateId" = $11,
               "overrideFormat" = $12, "customRemarks" = $13, "agePublicKey" = $14, "ageConfig" = $15,
               "updatedAt" = $16
           WHERE id = $1
           RETURNING id, name, enabled, priority, "matchHeader", "matchOperator", "matchPattern", negate,
                     conditions, action, "templateId", "overrideFormat", "customRemarks", "agePublicKey",
                     "ageConfig", "createdAt", "updatedAt""#,
    )
    .bind(&id)
    .bind(body.name.trim())
    .bind(body.enabled)
    .bind(body.priority)
    .bind(&body.match_header)
    .bind(&body.match_operator)
    .bind(&body.match_pattern)
    .bind(body.negate)
    .bind(conds)
    .bind(&body.action)
    .bind(&body.template_id)
    .bind(&body.override_format)
    .bind(&body.custom_remarks)
    .bind(&body.age_public_key)
    .bind(&body.age_config)
    .bind(crate::db::now())
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("SRR rule not found"))?;
    state.invalidate_derived();
    Ok(Json(srr_json(&r)?))
}

async fn delete_srr(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;
    let res = sqlx::query(r#"DELETE FROM "SrrRule" WHERE id = $1"#)
        .bind(&id)
        .execute(&state.db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::not_found("SRR rule not found"));
    }
    state.invalidate_derived();
    Ok(ok())
}

async fn reorder_srr(
    State(state): State<AppState>,
    user: AuthUser,
    AppJson(body): AppJson<ReorderSrrReq>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    let now = crate::db::now();
    let mut tx = state.db.begin().await?;
    for (idx, rid) in body.rule_ids.iter().enumerate() {
        valid_id(rid)?;
        let prio = i32::try_from(idx + 1).unwrap_or(1);
        sqlx::query(r#"UPDATE "SrrRule" SET priority = $2, "updatedAt" = $3 WHERE id = $1"#)
            .bind(rid)
            .bind(prio)
            .bind(now)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    state.invalidate_derived();
    Ok(ok())
}
