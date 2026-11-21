use crate::api::common::{like_pattern, ok, valid_id, AppJson};
use crate::audit::{self, AuditEntry};
use crate::auth::extract::AuthUser;
use crate::db::enums::{DbProtocol, Protocol};
use crate::error::{AppError, AppResult};
use crate::net::{user_agent, ClientIp};
use crate::services::{node_client, node_config, outbox, xray_studio};
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{QueryBuilder, Row};
use std::collections::HashMap;
use std::str::FromStr;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/validate", post(validate_xray_config))
        .route("/generate-keys", post(generate_protocol_keys))
        .route(
            "/xray-profile/{node_id}",
            get(get_node_xray_profile).put(save_node_xray_profile),
        )
        .route("/{id}", get(get_one).put(update).delete(remove))
        .route("/{id}/toggle", post(toggle))
        .route("/{id}/port-share", post(create_port_share))
        .route(
            "/{inbound_id}/port-share/{id}",
            put(update_port_share).delete(delete_port_share),
        )
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListQuery {
    node_id: Option<String>,
    protocol: Option<String>,
    search: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateInboundReq {
    node_id: String,
    protocol: Protocol,
    tag: String,
    port: i32,
    listen: Option<String>,
    settings: Value,
    stream: Option<Value>,
    routing: Option<Value>,
    sniffing: Option<bool>,
    remark: Option<String>,
    enable: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateInboundReq {
    protocol: Option<Protocol>,
    tag: Option<String>,
    port: Option<i32>,
    listen: Option<String>,
    settings: Option<Value>,
    stream: Option<Value>,
    routing: Option<Value>,
    sniffing: Option<bool>,
    remark: Option<Option<String>>,
    enable: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreatePortShareReq {
    protocol: Protocol,
    tag: String,
    host: Option<String>,
    path: Option<String>,
    settings: Option<Value>,
    stream: Option<Value>,
    enable: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdatePortShareReq {
    protocol: Option<Protocol>,
    tag: Option<String>,
    host: Option<Option<String>>,
    path: Option<Option<String>>,
    settings: Option<Value>,
    stream: Option<Value>,
    enable: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InboundDto {
    pub id: String,
    pub node_id: String,
    pub protocol: Protocol,
    pub tag: String,
    pub port: i32,
    pub listen: String,
    pub settings: Value,
    pub stream: Value,
    pub routing: Value,
    pub sniffing: bool,
    pub remark: Option<String>,
    pub enable: bool,
    #[serde(with = "crate::db::ts")]
    pub created_at: NaiveDateTime,
    #[serde(with = "crate::db::ts")]
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortShareDto {
    pub id: String,
    pub inbound_id: String,
    pub protocol: Protocol,
    pub tag: String,
    pub settings: Value,
    pub stream: Value,
    pub host: Option<String>,
    pub path: Option<String>,
    pub enable: bool,
    #[serde(with = "crate::db::ts")]
    pub created_at: NaiveDateTime,
    #[serde(with = "crate::db::ts")]
    pub updated_at: NaiveDateTime,
}

pub(crate) fn to_value<T: Serialize>(v: &T) -> AppResult<Value> {
    serde_json::to_value(v).map_err(|e| AppError::internal(format!("serialize: {e}")))
}

/// Scrubs private keys, TLS key files, and server passwords for non-admin callers.
pub(crate) fn redact_inbound_secrets(v: &mut Value, is_admin: bool) {
    if is_admin {
        return;
    }
    fn scrub(node: &mut Value) {
        match node {
            Value::Object(map) => {
                for k in ["privateKey", "keyFile", "key_path", "echKeyPath", "password"] {
                    map.remove(k);
                }
                for val in map.values_mut() {
                    scrub(val);
                }
            }
            Value::Array(arr) => arr.iter_mut().for_each(scrub),
            _ => {}
        }
    }
    scrub(v);
}

fn validate_inbound_config(
    protocol: Protocol,
    tag: &str,
    port: i32,
    settings: &Value,
    stream: &Value,
) -> AppResult<()> {
    let t = tag.trim();
    if t.is_empty() || t.chars().any(|c| c == '/' || c == '\\' || c.is_control()) {
        return Err(AppError::validation("tag", "Unsafe tag"));
    }
    if !(1..=65535).contains(&port) {
        return Err(AppError::validation("port", "Port must be between 1 and 65535"));
    }
    if !settings.is_object() {
        return Err(AppError::validation("settings", "settings must be an object"));
    }
    let network = stream
        .get("network")
        .and_then(Value::as_str)
        .unwrap_or("tcp")
        .to_ascii_lowercase();
    let security = stream
        .get("security")
        .and_then(Value::as_str)
        .unwrap_or("none")
        .to_ascii_lowercase();
    let flow = settings
        .get("flow")
        .and_then(Value::as_str)
        .or_else(|| stream.get("flow").and_then(Value::as_str))
        .unwrap_or("");

    if protocol == Protocol::Vless && !flow.is_empty() {
        let valid_flow = matches!(flow, "xtls-rprx-vision" | "xtls-rprx-vision-udp443");
        let valid_net = matches!(network.as_str(), "tcp" | "raw");
        let valid_sec = matches!(security.as_str(), "tls" | "reality");
        if !valid_flow || !valid_net || !valid_sec {
            return Err(AppError::validation(
                "settings.flow",
                "Vision requires TCP/raw with TLS or Reality",
            ));
        }
    }

    if security == "reality" && !matches!(network.as_str(), "tcp" | "raw" | "grpc" | "xhttp" | "h2" | "http") {
        return Err(AppError::validation(
            "stream.network",
            "Unsupported Reality transport; use TCP/raw, gRPC, HTTP/2 or XHTTP",
        ));
    }

    if protocol == Protocol::Hysteria2 {
        if let Some(pr) = settings
            .get("portRange")
            .or_else(|| settings.get("port_range"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            mypanel_proto::links::parse_port_range(pr)
                .map_err(|e| AppError::validation("settings.portRange", &e.to_string()))?;
        }
    }

    if protocol == Protocol::Shadowtls {
        let ver = settings
            .get("version")
            .or_else(|| settings.get("shadowtlsVersion"))
            .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok())))
            .unwrap_or(3);
        if !(1..=3).contains(&ver) {
            return Err(AppError::validation(
                "settings.version",
                "ShadowTLS version must be 1, 2, or 3",
            ));
        }
        if let Some(hp) = settings
            .get("handshakePort")
            .and_then(|v| v.as_i64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok())))
        {
            if !(1..=65535).contains(&hp) {
                return Err(AppError::validation(
                    "settings.handshakePort",
                    "Handshake port must be between 1 and 65535",
                ));
            }
        }
    }

    if let Some(obfs) = settings.get("obfs") {
        if obfs.get("type").and_then(Value::as_str) == Some("salamander")
            && obfs
                .get("password")
                .and_then(Value::as_str)
                .is_none_or(|p| p.is_empty())
        {
            return Err(AppError::validation(
                "settings.obfs.password",
                "Salamander requires a password",
            ));
        }
    }

    if protocol == Protocol::Mieru {
        if let Some(tr) = settings
            .get("transport")
            .or_else(|| settings.get("protocol"))
            .and_then(Value::as_str)
        {
            let lower = tr.to_ascii_lowercase();
            if !matches!(lower.as_str(), "tcp" | "udp" | "both" | "tcp,udp") {
                return Err(AppError::validation(
                    "settings.transport",
                    "Invalid Mieru transport",
                ));
            }
        }
    }

    Ok(())
}

fn row_to_inbound(r: &sqlx::postgres::PgRow) -> AppResult<InboundDto> {
    Ok(InboundDto {
        id: r.try_get("id")?,
        node_id: r.try_get("nodeId")?,
        protocol: r.try_get::<DbProtocol, _>("protocol")?.into(),
        tag: r.try_get("tag")?,
        port: r.try_get("port")?,
        listen: r.try_get("listen")?,
        settings: r.try_get("settings")?,
        stream: r.try_get("stream")?,
        routing: r.try_get("routing")?,
        sniffing: r.try_get("sniffing")?,
        remark: r.try_get("remark")?,
        enable: r.try_get("enable")?,
        created_at: r.try_get("createdAt")?,
        updated_at: r.try_get("updatedAt")?,
    })
}

fn row_to_share(r: &sqlx::postgres::PgRow) -> AppResult<PortShareDto> {
    Ok(PortShareDto {
        id: r.try_get("id")?,
        inbound_id: r.try_get("inboundId")?,
        protocol: r.try_get::<DbProtocol, _>("protocol")?.into(),
        tag: r.try_get("tag")?,
        settings: r.try_get("settings")?,
        stream: r.try_get("stream")?,
        host: r.try_get("host")?,
        path: r.try_get("path")?,
        enable: r.try_get("enable")?,
        created_at: r.try_get("createdAt")?,
        updated_at: r.try_get("updatedAt")?,
    })
}

pub(crate) async fn fetch_inbounds_of_node(
    pool: &sqlx::PgPool,
    node_id: &str,
) -> AppResult<Vec<InboundDto>> {
    let rows = sqlx::query(
        r#"SELECT id, "nodeId", protocol, tag, port, listen, settings, stream, routing, sniffing, remark, enable, "createdAt", "updatedAt"
           FROM "Inbound" WHERE "nodeId" = $1 ORDER BY port ASC, id ASC"#,
    )
    .bind(node_id)
    .fetch_all(pool)
    .await?;
    rows.iter().map(row_to_inbound).collect()
}

pub(crate) async fn fetch_port_shares_bulk(
    pool: &sqlx::PgPool,
    inbound_ids: &[String],
) -> AppResult<HashMap<String, Vec<Value>>> {
    let mut map: HashMap<String, Vec<Value>> = HashMap::new();
    if inbound_ids.is_empty() {
        return Ok(map);
    }
    let rows = sqlx::query(
        r#"SELECT id, "inboundId", protocol, tag, settings, stream, host, path, enable, "createdAt", "updatedAt"
           FROM "PortShare" WHERE "inboundId" = ANY($1::text[]) ORDER BY "createdAt" ASC, id ASC"#,
    )
    .bind(inbound_ids)
    .fetch_all(pool)
    .await?;

    for r in rows {
        let dto = row_to_share(&r)?;
        let iid = dto.inbound_id.clone();
        map.entry(iid).or_default().push(to_value(&dto)?);
    }
    Ok(map)
}

async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<Value>>> {
    let mut qb = QueryBuilder::new(
        r#"SELECT id, "nodeId", protocol, tag, port, listen, settings, stream, routing, sniffing, remark, enable, "createdAt", "updatedAt"
           FROM "Inbound" WHERE 1=1"#,
    );
    if let Some(ref nid) = q.node_id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        qb.push(r#" AND "nodeId" = "#).push_bind(nid.to_string());
    }
    if let Some(ref ps) = q.protocol.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        if let Ok(proto) = Protocol::from_str(ps) {
            qb.push(r#" AND protocol = "#).push_bind(DbProtocol::from(proto));
        }
    }
    if let Some(ref s) = q.search.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let pat = like_pattern(s);
        qb.push(r#" AND (tag ILIKE "#)
            .push_bind(pat.clone())
            .push(r#" OR remark ILIKE "#)
            .push_bind(pat)
            .push(")");
    }
    qb.push(r#" ORDER BY port ASC"#);

    let rows = qb.build().fetch_all(&state.db).await?;
    let inbounds: Vec<InboundDto> = rows.iter().map(row_to_inbound).collect::<AppResult<_>>()?;
    let inbound_ids: Vec<String> = inbounds.iter().map(|i| i.id.clone()).collect();
    let node_ids: Vec<String> = inbounds.iter().map(|i| i.node_id.clone()).collect();

    let shares = fetch_port_shares_bulk(&state.db, &inbound_ids).await?;
    let mut refs = HashMap::new();
    if !node_ids.is_empty() {
        let n_rows = sqlx::query(r#"SELECT id, name, host FROM "Node" WHERE id = ANY($1::text[])"#)
            .bind(&node_ids)
            .fetch_all(&state.db)
            .await?;
        for nr in n_rows {
            let nid: String = nr.try_get("id")?;
            refs.insert(
                nid.clone(),
                json!({
                    "id": nid,
                    "name": nr.try_get::<String, _>("name")?,
                    "host": nr.try_get::<String, _>("host")?,
                }),
            );
        }
    }

    let mut out = Vec::with_capacity(inbounds.len());
    for row in inbounds {
        let mut v = to_value(&row)?;
        redact_inbound_secrets(&mut v, user.is_admin());
        if let Some(obj) = v.as_object_mut() {
            obj.insert(
                "node".into(),
                refs.get(&row.node_id).cloned().unwrap_or(Value::Null),
            );
            let mut svs = shares.get(&row.id).cloned().unwrap_or_default();
            svs.iter_mut()
                .for_each(|sv| redact_inbound_secrets(sv, user.is_admin()));
            obj.insert("portShares".into(), Value::Array(svs));
        }
        out.push(v);
    }
    Ok(Json(out))
}

async fn get_one(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    valid_id(&id)?;
    let r = sqlx::query(
        r#"SELECT id, "nodeId", protocol, tag, port, listen, settings, stream, routing, sniffing, remark, enable, "createdAt", "updatedAt"
           FROM "Inbound" WHERE id = $1"#,
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("Inbound not found"))?;
    let dto = row_to_inbound(&r)?;
    let mut v = to_value(&dto)?;
    redact_inbound_secrets(&mut v, user.is_admin());

    let shares = fetch_port_shares_bulk(&state.db, &[id.clone()]).await?;
    let mut share_vals = shares.get(&id).cloned().unwrap_or_default();
    share_vals
        .iter_mut()
        .for_each(|sv| redact_inbound_secrets(sv, user.is_admin()));

    let node_row = sqlx::query(r#"SELECT id, name, host FROM "Node" WHERE id = $1"#)
        .bind(&dto.node_id)
        .fetch_optional(&state.db)
        .await?;
    let node_val = match node_row {
        Some(nr) => json!({
            "id": nr.try_get::<String, _>("id")?,
            "name": nr.try_get::<String, _>("name")?,
            "host": nr.try_get::<String, _>("host")?
        }),
        None => Value::Null,
    };

    let ci_rows = sqlx::query(
        r#"SELECT id, "clientId", "inboundId", uuid, enable, "createdAt" FROM "ClientInbound" WHERE "inboundId" = $1"#,
    )
    .bind(&id)
    .fetch_all(&state.db)
    .await?;
    let mut ci_vals = Vec::with_capacity(ci_rows.len());
    for cr in ci_rows {
        let ca: NaiveDateTime = cr.try_get("createdAt")?;
        ci_vals.push(json!({
            "id": cr.try_get::<String, _>("id")?,
            "clientId": cr.try_get::<String, _>("clientId")?,
            "inboundId": cr.try_get::<String, _>("inboundId")?,
            "uuid": cr.try_get::<String, _>("uuid")?,
            "enable": cr.try_get::<bool, _>("enable")?,
            "createdAt": crate::db::ts::format_iso(&ca),
        }));
    }

    if let Some(obj) = v.as_object_mut() {
        obj.insert("node".into(), node_val);
        obj.insert("portShares".into(), Value::Array(share_vals));
        obj.insert("clientInbounds".into(), Value::Array(ci_vals));
    }
    Ok(Json(v))
}

async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    ip: ClientIp,
    headers: HeaderMap,
    AppJson(body): AppJson<CreateInboundReq>,
) -> AppResult<Response> {
    user.require_admin()?;
    valid_id(&body.node_id)?;
    let stream = body.stream.unwrap_or_else(|| json!({}));
    let routing = body.routing.unwrap_or_else(|| json!({}));
    validate_inbound_config(body.protocol, &body.tag, body.port, &body.settings, &stream)?;

    let dup: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "Inbound" WHERE "nodeId" = $1 AND tag = $2"#,
    )
    .bind(&body.node_id)
    .bind(body.tag.trim())
    .fetch_one(&state.db)
    .await?;
    if dup > 0 {
        return Err(AppError::bad_request("Tag already exists on this node"));
    }

    let id = crate::db::new_id();
    let now = crate::db::now();
    let listen = body.listen.unwrap_or_else(|| "0.0.0.0".into());
    let sniffing = body.sniffing.unwrap_or(true);
    let enable = body.enable.unwrap_or(true);

    let r = sqlx::query(
        r#"INSERT INTO "Inbound" (id, "nodeId", protocol, tag, port, listen, settings, stream, routing, sniffing, remark, enable, "createdAt", "updatedAt")
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $13)
           RETURNING id, "nodeId", protocol, tag, port, listen, settings, stream, routing, sniffing, remark, enable, "createdAt", "updatedAt""#,
    )
    .bind(&id)
    .bind(&body.node_id)
    .bind(DbProtocol::from(body.protocol))
    .bind(body.tag.trim())
    .bind(body.port)
    .bind(&listen)
    .bind(&body.settings)
    .bind(&stream)
    .bind(&routing)
    .bind(sniffing)
    .bind(&body.remark)
    .bind(enable)
    .bind(now)
    .fetch_one(&state.db)
    .await?;

    // Auto-bind existing clients whose `protocols` include this inbound's protocol
    // and whose node scope allows this inbound's node.
    let _ = sqlx::query(
        r#"INSERT INTO "ClientInbound" (id, "clientId", "inboundId", uuid, enable, "createdAt")
           SELECT gen_random_uuid()::text, c.id, $1, c.uuid, true, $2
           FROM "Client" c
           WHERE c.protocols ? $3
             AND (
               c."allNodes" = true
               OR EXISTS (
                 SELECT 1 FROM "ClientNode" cn
                 WHERE cn."clientId" = c.id AND cn."nodeId" = $4
               )
             )
           ON CONFLICT ("clientId", "inboundId") DO NOTHING"#,
    )
    .bind(&id)
    .bind(now)
    .bind(body.protocol.as_str())
    .bind(&body.node_id)
    .execute(&state.db)
    .await;

    state.invalidate_derived();
    let st = state.clone();
    let nid = body.node_id.clone();
    tokio::spawn(async move {
        let _ = outbox::push_config(&st, &nid).await;
    });

    audit::spawn(
        &state.db,
        AuditEntry::new("CREATE", "inbound")
            .user(&user.id)
            .resource_id(&id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );

    let dto = row_to_inbound(&r)?;
    Ok((StatusCode::CREATED, Json(to_value(&dto)?)).into_response())
}

async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    ip: ClientIp,
    headers: HeaderMap,
    AppJson(body): AppJson<UpdateInboundReq>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;
    let ex_row = sqlx::query(
        r#"SELECT id, "nodeId", protocol, tag, port, listen, settings, stream, routing, sniffing, remark, enable, "createdAt", "updatedAt"
           FROM "Inbound" WHERE id = $1"#,
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("Inbound not found"))?;
    let ex = row_to_inbound(&ex_row)?;

    let protocol = body.protocol.unwrap_or(ex.protocol);
    let tag = body.tag.unwrap_or(ex.tag);
    let port = body.port.unwrap_or(ex.port);
    let listen = body.listen.unwrap_or(ex.listen);
    let settings = body.settings.unwrap_or(ex.settings);
    let stream = body.stream.unwrap_or(ex.stream);
    let routing = body.routing.unwrap_or(ex.routing);
    let sniffing = body.sniffing.unwrap_or(ex.sniffing);
    let remark = body.remark.unwrap_or(ex.remark);
    let enable = body.enable.unwrap_or(ex.enable);

    validate_inbound_config(protocol, &tag, port, &settings, &stream)?;
    let now = crate::db::now();

    let r = sqlx::query(
        r#"UPDATE "Inbound"
           SET protocol = $2, tag = $3, port = $4, listen = $5, settings = $6, stream = $7,
               routing = $8, sniffing = $9, remark = $10, enable = $11, "updatedAt" = $12
           WHERE id = $1
           RETURNING id, "nodeId", protocol, tag, port, listen, settings, stream, routing, sniffing, remark, enable, "createdAt", "updatedAt""#,
    )
    .bind(&id)
    .bind(DbProtocol::from(protocol))
    .bind(tag.trim())
    .bind(port)
    .bind(listen)
    .bind(settings)
    .bind(stream)
    .bind(routing)
    .bind(sniffing)
    .bind(remark)
    .bind(enable)
    .bind(now)
    .fetch_one(&state.db)
    .await?;

    state.invalidate_derived();
    let st = state.clone();
    let nid = ex.node_id.clone();
    tokio::spawn(async move {
        let _ = outbox::push_config(&st, &nid).await;
    });

    audit::spawn(
        &state.db,
        AuditEntry::new("UPDATE", "inbound")
            .user(&user.id)
            .resource_id(&id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );

    Ok(Json(to_value(&row_to_inbound(&r)?)?))
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
    let node_id: Option<String> =
        sqlx::query_scalar(r#"DELETE FROM "Inbound" WHERE id = $1 RETURNING "nodeId""#)
            .bind(&id)
            .fetch_optional(&state.db)
            .await?;
    let Some(nid) = node_id else {
        return Err(AppError::not_found("Inbound not found"));
    };

    state.invalidate_derived();
    let st = state.clone();
    tokio::spawn(async move {
        let _ = outbox::push_config(&st, &nid).await;
    });

    audit::spawn(
        &state.db,
        AuditEntry::new("DELETE", "inbound")
            .user(&user.id)
            .resource_id(&id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );
    Ok(ok())
}

async fn toggle(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    ip: ClientIp,
    headers: HeaderMap,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&id)?;
    let r = sqlx::query(
        r#"UPDATE "Inbound" SET enable = NOT enable, "updatedAt" = $2 WHERE id = $1 RETURNING enable, "nodeId""#,
    )
    .bind(&id)
    .bind(crate::db::now())
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("Inbound not found"))?;

    let enable: bool = r.try_get("enable")?;
    let nid: String = r.try_get("nodeId")?;

    state.invalidate_derived();
    let st = state.clone();
    tokio::spawn(async move {
        let _ = outbox::push_config(&st, &nid).await;
    });

    audit::spawn(
        &state.db,
        AuditEntry::new("TOGGLE", "inbound")
            .user(&user.id)
            .resource_id(&id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );
    Ok(Json(json!({ "enable": enable })))
}

async fn create_port_share(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    ip: ClientIp,
    headers: HeaderMap,
    AppJson(body): AppJson<CreatePortShareReq>,
) -> AppResult<Response> {
    user.require_admin()?;
    valid_id(&id)?;

    let inbound_row = sqlx::query(r#"SELECT "nodeId", port FROM "Inbound" WHERE id = $1"#)
        .bind(&id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("Inbound not found"))?;
    let node_id: String = inbound_row.try_get("nodeId")?;
    let port: i32 = inbound_row.try_get("port")?;

    let settings = body.settings.unwrap_or_else(|| json!({}));
    let stream = body.stream.unwrap_or_else(|| json!({}));
    validate_inbound_config(body.protocol, &body.tag, port, &settings, &stream)?;

    let ps_id = crate::db::new_id();
    let now = crate::db::now();
    let r = sqlx::query(
        r#"INSERT INTO "PortShare" (id, "inboundId", protocol, tag, settings, stream, host, path, enable, "createdAt", "updatedAt")
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10)
           RETURNING id, "inboundId", protocol, tag, settings, stream, host, path, enable, "createdAt", "updatedAt""#,
    )
    .bind(&ps_id)
    .bind(&id)
    .bind(DbProtocol::from(body.protocol))
    .bind(body.tag.trim())
    .bind(settings)
    .bind(stream)
    .bind(&body.host)
    .bind(&body.path)
    .bind(body.enable.unwrap_or(true))
    .bind(now)
    .fetch_one(&state.db)
    .await?;

    state.invalidate_derived();
    let st = state.clone();
    tokio::spawn(async move {
        let _ = outbox::push_config(&st, &node_id).await;
    });

    audit::spawn(
        &state.db,
        AuditEntry::new("CREATE", "portShare")
            .user(&user.id)
            .resource_id(&ps_id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );

    Ok((StatusCode::CREATED, Json(to_value(&row_to_share(&r)?)?)).into_response())
}

async fn update_port_share(
    State(state): State<AppState>,
    user: AuthUser,
    Path((inbound_id, id)): Path<(String, String)>,
    ip: ClientIp,
    headers: HeaderMap,
    AppJson(body): AppJson<UpdatePortShareReq>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&inbound_id)?;
    valid_id(&id)?;

    let inbound_row = sqlx::query(r#"SELECT "nodeId", port FROM "Inbound" WHERE id = $1"#)
        .bind(&inbound_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("Inbound not found"))?;
    let node_id: String = inbound_row.try_get("nodeId")?;
    let port: i32 = inbound_row.try_get("port")?;

    let ex_row = sqlx::query(
        r#"SELECT id, "inboundId", protocol, tag, settings, stream, host, path, enable, "createdAt", "updatedAt"
           FROM "PortShare" WHERE id = $1 AND "inboundId" = $2"#,
    )
    .bind(&id)
    .bind(&inbound_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("PortShare not found"))?;
    let ex = row_to_share(&ex_row)?;

    let protocol = body.protocol.unwrap_or(ex.protocol);
    let tag = body.tag.unwrap_or(ex.tag);
    let host = body.host.unwrap_or(ex.host);
    let path = body.path.unwrap_or(ex.path);
    let settings = body.settings.unwrap_or(ex.settings);
    let stream = body.stream.unwrap_or(ex.stream);
    let enable = body.enable.unwrap_or(ex.enable);

    validate_inbound_config(protocol, &tag, port, &settings, &stream)?;
    let now = crate::db::now();

    let r = sqlx::query(
        r#"UPDATE "PortShare"
           SET protocol = $3, tag = $4, host = $5, path = $6, settings = $7, stream = $8, enable = $9, "updatedAt" = $10
           WHERE id = $1 AND "inboundId" = $2
           RETURNING id, "inboundId", protocol, tag, settings, stream, host, path, enable, "createdAt", "updatedAt""#,
    )
    .bind(&id)
    .bind(&inbound_id)
    .bind(DbProtocol::from(protocol))
    .bind(tag.trim())
    .bind(&host)
    .bind(&path)
    .bind(&settings)
    .bind(&stream)
    .bind(enable)
    .bind(now)
    .fetch_one(&state.db)
    .await?;

    state.invalidate_derived();
    let st = state.clone();
    tokio::spawn(async move {
        let _ = outbox::push_config(&st, &node_id).await;
    });

    audit::spawn(
        &state.db,
        AuditEntry::new("UPDATE", "portShare")
            .user(&user.id)
            .resource_id(&id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );

    Ok(Json(to_value(&row_to_share(&r)?)?))
}

async fn delete_port_share(
    State(state): State<AppState>,
    user: AuthUser,
    Path((inbound_id, id)): Path<(String, String)>,
    ip: ClientIp,
    headers: HeaderMap,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&inbound_id)?;
    valid_id(&id)?;

    let res = sqlx::query(r#"DELETE FROM "PortShare" WHERE id = $1 AND "inboundId" = $2"#)
        .bind(&id)
        .bind(&inbound_id)
        .execute(&state.db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::not_found("PortShare not found"));
    }

    state.invalidate_derived();
    audit::spawn(
        &state.db,
        AuditEntry::new("DELETE", "portShare")
            .user(&user.id)
            .resource_id(&id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );
    Ok(ok())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ValidateXrayReq {
    config: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GenerateKeysReq {
    kind: String,
    #[serde(default)]
    method: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveXrayProfileReq {
    #[serde(default)]
    profile_name: Option<String>,
    config: Value,
    #[serde(default)]
    push: bool,
    #[serde(default)]
    sync_inbounds: bool,
}

async fn validate_xray_config(
    user: AuthUser,
    AppJson(body): AppJson<ValidateXrayReq>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    let res = xray_studio::validate_xray_config_live(&body.config).await;
    Ok(Json(to_value(&res)?))
}

async fn generate_protocol_keys(
    user: AuthUser,
    AppJson(body): AppJson<GenerateKeysReq>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    let out = xray_studio::generate_protocol_keys(&body.kind, body.method.as_deref()).await?;
    Ok(Json(out))
}

async fn get_node_xray_profile(
    State(state): State<AppState>,
    user: AuthUser,
    Path(node_id): Path<String>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&node_id)?;
    let row = sqlx::query(
        r#"SELECT id, name, "xrayProfile", "updatedAt" FROM "Node" WHERE id = $1"#,
    )
    .bind(&node_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("Node not found"))?;

    let node_name: String = row.try_get("name")?;
    let saved_profile: Option<Value> = row.try_get("xrayProfile").ok().flatten();
    let updated_at: NaiveDateTime = row.try_get("updatedAt")?;

    let inbounds = fetch_inbounds_of_node(&state.db, &node_id).await?;
    let rules = node_config::build_routing_rules(&state, &node_id).await?;
    let config = xray_studio::build_studio_xray_config(&inbounds, &rules, saved_profile.as_ref());

    let profile_name = saved_profile
        .as_ref()
        .and_then(|p| p.get("profileName"))
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| format!("{node_name} Xray Profile"));

    Ok(Json(json!({
        "nodeId": node_id,
        "nodeName": node_name,
        "profileName": profile_name,
        "config": config,
        "savedProfile": saved_profile,
        "updatedAt": crate::db::ts::format_iso(&updated_at),
    })))
}

async fn save_node_xray_profile(
    State(state): State<AppState>,
    user: AuthUser,
    Path(node_id): Path<String>,
    ip: ClientIp,
    headers: HeaderMap,
    AppJson(body): AppJson<SaveXrayProfileReq>,
) -> AppResult<Json<Value>> {
    user.require_admin()?;
    valid_id(&node_id)?;

    let validation = xray_studio::validate_xray_config_live(&body.config).await;
    if !validation.valid {
        return Err(AppError::bad_request(validation.message));
    }

    let exists: Option<String> = sqlx::query_scalar(r#"SELECT id FROM "Node" WHERE id = $1"#)
        .bind(&node_id)
        .fetch_optional(&state.db)
        .await?;
    if exists.is_none() {
        return Err(AppError::not_found("Node not found"));
    }

    let Some(cfg_obj) = body.config.as_object() else {
        return Err(AppError::validation("config", "Xray profile config must be a JSON object"));
    };

    let mut inbound_overrides = serde_json::Map::new();
    if let Some(inbounds_arr) = cfg_obj.get("inbounds").and_then(Value::as_array) {
        for ib_val in inbounds_arr {
            let Some(ib_obj) = ib_val.as_object() else {
                continue;
            };
            let Some(tag) = ib_obj.get("tag").and_then(Value::as_str).map(str::trim) else {
                continue;
            };
            if tag.is_empty() || tag == "api" {
                continue;
            }
            let mut ov = serde_json::Map::new();
            if let Some(sn) = ib_obj.get("sniffing").filter(|v| v.is_object()) {
                ov.insert("sniffing".into(), sn.clone());
            }
            if let Some(alloc) = ib_obj.get("allocate").filter(|v| v.is_object()) {
                ov.insert("allocate".into(), alloc.clone());
            }
            if let Some(so) = ib_val.pointer("/streamSettings/sockopt").filter(|v| v.is_object()) {
                ov.insert("sockopt".into(), so.clone());
            }
            if !ov.is_empty() {
                inbound_overrides.insert(tag.to_string(), Value::Object(ov));
            }

            if body.sync_inbounds {
                let port = ib_obj
                    .get("port")
                    .and_then(Value::as_i64)
                    .and_then(|p| i32::try_from(p).ok());
                let listen = ib_obj.get("listen").and_then(Value::as_str).map(ToString::to_string);
                let stream_val = ib_obj.get("streamSettings").filter(|v| v.is_object()).cloned();
                let mut settings_sync = ib_obj.get("settings").and_then(Value::as_object).cloned();
                if let Some(ref mut s_map) = settings_sync {
                    if let Some(sn) = ib_obj.get("sniffing").filter(|v| v.is_object()) {
                        s_map.insert("sniffing".into(), sn.clone());
                    }
                    if let Some(alloc) = ib_obj.get("allocate").filter(|v| v.is_object()) {
                        s_map.insert("allocate".into(), alloc.clone());
                    }
                }
                if let (Some(p), Some(l), Some(sv), Some(st)) = (port, listen, stream_val, settings_sync) {
                    let _ = sqlx::query(
                        r#"UPDATE "Inbound"
                           SET port = $3, listen = $4, settings = $5, stream = $6, "updatedAt" = $7
                           WHERE "nodeId" = $1 AND tag = $2"#,
                    )
                    .bind(&node_id)
                    .bind(tag)
                    .bind(p)
                    .bind(l)
                    .bind(Value::Object(st))
                    .bind(sv)
                    .bind(crate::db::now())
                    .execute(&state.db)
                    .await;
                }
            }
        }
    }

    let mut stored = serde_json::Map::new();
    if let Some(ref pname) = body.profile_name.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        stored.insert("profileName".into(), json!(pname));
    }
    for key in ["log", "outbounds", "routing", "dns", "fakedns", "policy"] {
        if let Some(val) = cfg_obj.get(key).filter(|v| !v.is_null()) {
            stored.insert(key.into(), val.clone());
        }
    }
    if !inbound_overrides.is_empty() {
        stored.insert("inboundOverrides".into(), Value::Object(inbound_overrides));
    }

    let stored_val = Value::Object(stored);
    sqlx::query(r#"UPDATE "Node" SET "xrayProfile" = $2, "updatedAt" = $3 WHERE id = $1"#)
        .bind(&node_id)
        .bind(&stored_val)
        .bind(crate::db::now())
        .execute(&state.db)
        .await?;

    state.invalidate_derived();

    let mut pushed = false;
    if body.push {
        if state.hub.is_connected(&node_id) {
            pushed = outbox::push_config(&state, &node_id).await.is_ok();
        } else if let Ok(target) = node_client::NodeTarget::fetch(&state, &node_id).await {
            pushed = node_client::push_config_http(&state, &target)
                .await
                .unwrap_or(false);
        }
    }

    audit::spawn(
        &state.db,
        AuditEntry::new("UPDATE", "nodeXrayProfile")
            .user(&user.id)
            .resource_id(&node_id)
            .ip(&*ip)
            .user_agent(user_agent(&headers)),
    );

    Ok(Json(json!({
        "saved": true,
        "pushed": pushed,
        "validation": validation,
        "savedProfile": stored_val,
    })))
}
