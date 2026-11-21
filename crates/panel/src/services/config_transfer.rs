use crate::auth::crypto::{decrypt_str, encrypt_str, sha256_hex, AAD_CONFIG_TRANSFER_V2};
use crate::auth::password::hash_password;
use crate::db::enums::{DbProtocol, Protocol, UserRole};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;

pub const MAX_TRANSFER_BYTES: usize = 1024 * 1024;
pub const MAX_TRANSFER_ITEMS: usize = 500;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferNodeV1 {
    pub name: String,
    pub host: String,
    pub port: i32,
    pub api_port: i32,
    pub country: Option<String>,
    pub city: Option<String>,
    pub isp: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "default_true")]
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferUserV1 {
    pub username: String,
    pub email: Option<String>,
    pub role: UserRole,
    pub language: String,
    #[serde(default)]
    pub banned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferDocumentV1 {
    pub version: u32,
    pub kind: String,
    #[serde(default)]
    pub nodes: Vec<TransferNodeV1>,
    #[serde(default)]
    pub users: Vec<TransferUserV1>,
}

fn default_true() -> bool {
    true
}

pub async fn export_v1(state: &AppState) -> AppResult<TransferDocumentV1> {
    let node_rows = sqlx::query(
        r#"SELECT name, host, port, "apiPort", country, city, isp, COALESCE(tags, '{}') AS tags, active
           FROM "Node" ORDER BY "createdAt" ASC"#,
    )
    .fetch_all(&state.db)
    .await?;

    let mut nodes = Vec::with_capacity(node_rows.len());
    for r in node_rows {
        nodes.push(TransferNodeV1 {
            name: r.try_get("name")?,
            host: r.try_get("host")?,
            port: r.try_get("port")?,
            api_port: r.try_get("apiPort")?,
            country: r.try_get("country")?,
            city: r.try_get("city")?,
            isp: r.try_get("isp")?,
            tags: r.try_get("tags")?,
            active: r.try_get("active")?,
        });
    }

    let user_rows = sqlx::query(
        r#"SELECT username, email, role, language, banned FROM "User" ORDER BY "createdAt" ASC"#,
    )
    .fetch_all(&state.db)
    .await?;

    let mut users = Vec::with_capacity(user_rows.len());
    for r in user_rows {
        users.push(TransferUserV1 {
            username: r.try_get("username")?,
            email: r.try_get("email")?,
            role: r.try_get("role")?,
            language: r.try_get("language")?,
            banned: r.try_get("banned")?,
        });
    }

    Ok(TransferDocumentV1 {
        version: 1,
        kind: "proxpanel-config".to_string(),
        nodes,
        users,
    })
}

pub fn validate_v1(doc: &TransferDocumentV1) -> AppResult<()> {
    if doc.version != 1 || doc.kind != "proxpanel-config" {
        return Err(AppError::bad_request("Invalid transfer document or credentials"));
    }
    if doc.nodes.len() + doc.users.len() > MAX_TRANSFER_ITEMS {
        return Err(AppError::bad_request("Transfer document exceeds item limit"));
    }
    let mut seen_users = HashSet::new();
    let mut seen_emails = HashSet::new();
    for u in &doc.users {
        if u.username.len() < 3 || u.username.len() > 64 {
            return Err(AppError::bad_request("Invalid transfer document or credentials"));
        }
        if !seen_users.insert(u.username.as_str()) {
            return Err(AppError::conflict("Duplicate username in transfer document"));
        }
        if let Some(em) = u.email.as_deref().filter(|s| !s.is_empty()) {
            if !seen_emails.insert(em) {
                return Err(AppError::conflict("Duplicate email in transfer document"));
            }
        }
    }
    let mut seen_nodes = HashSet::new();
    for n in &doc.nodes {
        if n.name.is_empty() || n.host.is_empty() || !(1..=65535).contains(&n.port) || !(1..=65535).contains(&n.api_port) {
            return Err(AppError::bad_request("Invalid transfer document or credentials"));
        }
        if !seen_nodes.insert((n.host.as_str(), n.api_port)) {
            return Err(AppError::conflict("Duplicate node host:apiPort in transfer document"));
        }
    }
    Ok(())
}

pub async fn import_v1(
    state: &AppState,
    doc: TransferDocumentV1,
    credentials: &HashMap<String, String>,
    apply: bool,
) -> AppResult<Value> {
    validate_v1(&doc)?;
    if !apply {
        return Ok(json!({
            "applied": false,
            "nodes": doc.nodes.len(),
            "users": doc.users.len(),
            "dryRun": true
        }));
    }

    let mut tx = state.db.begin().await?;
    let now = crate::db::now();
    let mut created_nodes = 0usize;
    let mut created_users = 0usize;

    for n in &doc.nodes {
        let secret = credentials
            .get(&n.name)
            .or_else(|| credentials.get(&n.host))
            .cloned()
            .unwrap_or_else(|| crate::auth::crypto::random_hex(24));
        if secret.len() < 12 || secret.len() > 72 {
            return Err(AppError::bad_request("Invalid transfer document or credentials"));
        }
        let secret_hash = sha256_hex(&secret);
        let res = sqlx::query(
            r#"INSERT INTO "Node" (id, name, host, port, "apiPort", secret, "secretHash", country, city, isp, tags, active, "createdAt", "updatedAt")
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $13)
               ON CONFLICT DO NOTHING"#,
        )
        .bind(crate::db::new_id())
        .bind(&n.name)
        .bind(&n.host)
        .bind(n.port)
        .bind(n.api_port)
        .bind(&secret)
        .bind(&secret_hash)
        .bind(&n.country)
        .bind(&n.city)
        .bind(&n.isp)
        .bind(&n.tags)
        .bind(n.active)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        created_nodes += usize::try_from(res.rows_affected()).unwrap_or(0);
    }

    for u in &doc.users {
        let pw = credentials
            .get(&u.username)
            .cloned()
            .unwrap_or_else(|| crate::auth::crypto::generate_user_password(16));
        let pw_hash = hash_password(&pw, 12).await?;
        let res = sqlx::query(
            r#"INSERT INTO "User" (id, username, email, password, role, language, banned, "createdAt", "updatedAt")
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8)
               ON CONFLICT (username) DO NOTHING"#,
        )
        .bind(crate::db::new_id())
        .bind(&u.username)
        .bind(&u.email)
        .bind(&pw_hash)
        .bind(u.role)
        .bind(&u.language)
        .bind(u.banned)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        created_users += usize::try_from(res.rows_affected()).unwrap_or(0);
    }

    tx.commit().await?;
    state.invalidate_derived();
    Ok(json!({
        "applied": true,
        "createdNodes": created_nodes,
        "createdUsers": created_users
    }))
}

pub async fn export_v2(state: &AppState, include_secrets: bool) -> AppResult<Value> {
    let v1 = export_v1(state).await?;
    let clients = sqlx::query(
        r#"SELECT id, uuid, username, email, "trafficLimit", "expireAt", banned, protocols, "subToken", "deviceLimit", "resetStrategy"
           FROM "Client" ORDER BY "createdAt" ASC LIMIT 500"#,
    )
    .fetch_all(&state.db)
    .await?;

    let mut client_list = Vec::with_capacity(clients.len());
    for c in clients {
        let cid: String = c.try_get("id")?;
        let mut secrets_obj = serde_json::Map::new();
        if include_secrets {
            if let Some(enc_key) = state.encryption_key() {
                let creds = sqlx::query(
                    r#"SELECT protocol, "encryptedSecret" FROM "ProtocolCredential" WHERE "clientId" = $1"#,
                )
                .bind(&cid)
                .fetch_all(&state.db)
                .await?;
                for cr in creds {
                    let proto: Protocol = cr.try_get::<DbProtocol, _>("protocol")?.into();
                    let enc: String = cr.try_get("encryptedSecret")?;
                    if let Ok(plain) = decrypt_str(Some(enc_key), &enc, AAD_CONFIG_TRANSFER_V2)
                        .or_else(|_| decrypt_str(Some(enc_key), &enc, crate::auth::crypto::AAD_PROTOCOL_CREDENTIAL))
                    {
                        if let Ok(re_enc) = encrypt_str(Some(enc_key), &plain, AAD_CONFIG_TRANSFER_V2) {
                            secrets_obj.insert(proto.as_str().to_string(), json!(re_enc));
                        }
                    }
                }
            }
        }
        client_list.push(json!({
            "id": cid,
            "uuid": c.try_get::<String, _>("uuid")?,
            "username": c.try_get::<String, _>("username")?,
            "email": c.try_get::<Option<String>, _>("email")?,
            "trafficLimit": c.try_get::<i64, _>("trafficLimit")?,
            "expireAt": c.try_get::<Option<chrono::NaiveDateTime>, _>("expireAt")?.map(|d| crate::db::ts::format_iso(&d)),
            "banned": c.try_get::<bool, _>("banned")?,
            "protocols": c.try_get::<Value, _>("protocols")?,
            "subToken": c.try_get::<String, _>("subToken")?,
            "deviceLimit": c.try_get::<Option<i32>, _>("deviceLimit")?,
            "resetStrategy": c.try_get::<String, _>("resetStrategy")?,
            "secrets": Value::Object(secrets_obj)
        }));
    }

    Ok(json!({
        "version": 2,
        "kind": "proxpanel-config-v2",
        "nodes": v1.nodes,
        "users": v1.users,
        "clients": client_list
    }))
}

pub async fn import_v2(
    state: &AppState,
    doc: Value,
    mode: &str,
    apply: bool,
) -> AppResult<Value> {
    let version = doc.get("version").and_then(Value::as_u64).unwrap_or(0);
    if version != 2 {
        return Err(AppError::bad_request("Invalid transfer document or credentials"));
    }
    let nodes_count = doc.get("nodes").and_then(Value::as_array).map_or(0, Vec::len);
    let users_count = doc.get("users").and_then(Value::as_array).map_or(0, Vec::len);
    let clients_arr = doc.get("clients").and_then(Value::as_array).cloned().unwrap_or_default();
    if nodes_count + users_count + clients_arr.len() > MAX_TRANSFER_ITEMS {
        return Err(AppError::bad_request("Transfer document exceeds item limit"));
    }
    if !apply {
        return Ok(json!({
            "applied": false,
            "mode": mode,
            "nodes": nodes_count,
            "users": users_count,
            "clients": clients_arr.len(),
            "dryRun": true
        }));
    }

    let upsert = mode == "upsert";
    let mut imported_clients = 0usize;
    let now = crate::db::now();
    let mut tx = state.db.begin().await?;

    for c in &clients_arr {
        let Some(username) = c.get("username").and_then(Value::as_str) else {
            continue;
        };
        let uuid = c
            .get("uuid")
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_else(crate::db::new_id);
        let sub_token = c
            .get("subToken")
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_else(crate::db::new_id);
        let traffic_limit = c.get("trafficLimit").and_then(Value::as_i64).unwrap_or(0);
        let banned = c.get("banned").and_then(Value::as_bool).unwrap_or(false);
        let protocols = c
            .get("protocols")
            .cloned()
            .unwrap_or_else(|| json!(["VLESS", "HYSTERIA2"]));
        let reset_strategy = c
            .get("resetStrategy")
            .and_then(Value::as_str)
            .unwrap_or("NO_RESET");
        let device_limit = c.get("deviceLimit").and_then(Value::as_i64).and_then(|v| i32::try_from(v).ok());
        let cid = c
            .get("id")
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_else(crate::db::new_id);
        let pw_hash = hash_password(&crate::auth::crypto::random_hex(12), 10).await?;

        let sql = if upsert {
            r#"INSERT INTO "Client" (id, uuid, username, password, "trafficLimit", banned, protocols, "subToken", "deviceLimit", "resetStrategy", "createdAt", "updatedAt")
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $11)
               ON CONFLICT (uuid) DO UPDATE SET "trafficLimit" = EXCLUDED."trafficLimit", banned = EXCLUDED.banned, protocols = EXCLUDED.protocols, "updatedAt" = EXCLUDED."updatedAt""#
        } else {
            r#"INSERT INTO "Client" (id, uuid, username, password, "trafficLimit", banned, protocols, "subToken", "deviceLimit", "resetStrategy", "createdAt", "updatedAt")
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $11)
               ON CONFLICT DO NOTHING"#
        };
        let res = sqlx::query(sql)
            .bind(&cid)
            .bind(&uuid)
            .bind(username)
            .bind(&pw_hash)
            .bind(traffic_limit)
            .bind(banned)
            .bind(&protocols)
            .bind(&sub_token)
            .bind(device_limit)
            .bind(reset_strategy)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        if res.rows_affected() > 0 {
            imported_clients += 1;
        }

        if let (Some(enc_key), Some(secrets)) = (
            state.encryption_key(),
            c.get("secrets").and_then(Value::as_object),
        ) {
            for (proto_str, enc_val) in secrets {
                if let (Ok(proto), Some(enc_str)) = (Protocol::from_str(proto_str), enc_val.as_str()) {
                    if let Ok(plain) = decrypt_str(Some(enc_key), enc_str, AAD_CONFIG_TRANSFER_V2) {
                        if let Ok(re_enc) = encrypt_str(Some(enc_key), &plain, crate::auth::crypto::AAD_PROTOCOL_CREDENTIAL) {
                            let _ = sqlx::query(
                                r#"INSERT INTO "ProtocolCredential" (id, "clientId", protocol, "encryptedSecret", "createdAt", "updatedAt")
                                   VALUES ($1, $2, $3, $4, $5, $5)
                                   ON CONFLICT ("clientId", protocol) DO NOTHING"#,
                            )
                            .bind(crate::db::new_id())
                            .bind(&cid)
                            .bind(DbProtocol::from(proto))
                            .bind(re_enc)
                            .bind(now)
                            .execute(&mut *tx)
                            .await;
                        }
                    }
                }
            }
        }
    }

    tx.commit().await?;
    state.invalidate_derived();
    Ok(json!({
        "applied": true,
        "mode": mode,
        "importedClients": imported_clients
    }))
}
