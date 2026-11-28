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

    let mut prepared_nodes = Vec::with_capacity(doc.nodes.len());
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
        prepared_nodes.push((n, secret, secret_hash));
    }

    let mut prepared_users = Vec::with_capacity(doc.users.len());
    for u in &doc.users {
        let pw = credentials
            .get(&u.username)
            .cloned()
            .unwrap_or_else(|| crate::auth::crypto::generate_user_password(16));
        let pw_hash = hash_password(&pw, 12).await?;
        prepared_users.push((u, pw_hash));
    }

    let mut tx = state.db.begin().await?;
    let now = crate::db::now();
    let mut created_nodes = 0usize;
    let mut created_users = 0usize;

    for (n, secret, secret_hash) in prepared_nodes {
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

    for (u, pw_hash) in prepared_users {
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

    let client_ids: Vec<String> = clients
        .iter()
        .filter_map(|c| c.try_get::<String, _>("id").ok())
        .collect();
    let bulk_creds = if include_secrets && state.encryption_key().is_some() {
        crate::services::protocol_credentials::get_protocol_secrets_bulk(state, &client_ids).await?
    } else {
        HashMap::new()
    };

    let mut by_client: HashMap<String, Vec<(Protocol, String)>> = HashMap::new();
    for ((cid, proto), plain) in bulk_creds {
        by_client.entry(cid).or_default().push((proto, plain));
    }

    let mut client_list = Vec::with_capacity(clients.len());
    for c in clients {
        let cid: String = c.try_get("id")?;
        let mut secrets_obj = serde_json::Map::new();
        if include_secrets {
            if let (Some(enc_key), Some(creds)) = (state.encryption_key(), by_client.get(&cid)) {
                for (proto, plain) in creds {
                    if let Ok(re_enc) = encrypt_str(Some(enc_key), plain, AAD_CONFIG_TRANSFER_V2) {
                        secrets_obj.insert(proto.as_str().to_string(), json!(re_enc));
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
    let nodes: Vec<TransferNodeV1> = match doc.get("nodes") {
        Some(v) if !v.is_null() => serde_json::from_value(v.clone())
            .map_err(|_| AppError::bad_request("Invalid nodes section in transfer document"))?,
        _ => Vec::new(),
    };
    let users: Vec<TransferUserV1> = match doc.get("users") {
        Some(v) if !v.is_null() => serde_json::from_value(v.clone())
            .map_err(|_| AppError::bad_request("Invalid users section in transfer document"))?,
        _ => Vec::new(),
    };
    let clients_arr = doc.get("clients").and_then(Value::as_array).cloned().unwrap_or_default();
    if nodes.len() + users.len() + clients_arr.len() > MAX_TRANSFER_ITEMS {
        return Err(AppError::bad_request("Transfer document exceeds item limit"));
    }
    for n in &nodes {
        if n.name.is_empty()
            || n.host.is_empty()
            || !(1..=65535).contains(&n.port)
            || !(1..=65535).contains(&n.api_port)
        {
            return Err(AppError::bad_request("Invalid transfer document or credentials"));
        }
        crate::net::validate_node_host(&n.host, state.cfg.allow_private_nodes)?;
    }
    for u in &users {
        if u.username.len() < 3 || u.username.len() > 64 {
            return Err(AppError::bad_request("Invalid transfer document or credentials"));
        }
    }

    if !apply {
        return Ok(json!({
            "applied": false,
            "mode": mode,
            "nodes": nodes.len(),
            "users": users.len(),
            "clients": clients_arr.len(),
            "dryRun": true
        }));
    }

    // Hash all passwords before starting the PostgreSQL transaction.
    let mut prepared_users = Vec::with_capacity(users.len());
    for u in users {
        let pw_hash = hash_password(&crate::auth::crypto::generate_user_password(16), 12).await?;
        prepared_users.push((u, pw_hash));
    }

    struct PreparedClient<'a> {
        cid: String,
        uuid: String,
        username: &'a str,
        email: Option<&'a str>,
        pw_hash: String,
        traffic_limit: i64,
        expire_at: Option<chrono::NaiveDateTime>,
        banned: bool,
        protocols: Value,
        sub_token: String,
        device_limit: Option<i32>,
        reset_strategy: &'a str,
        secrets: Option<&'a serde_json::Map<String, Value>>,
    }

    let mut prepared_clients = Vec::with_capacity(clients_arr.len());
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
        let email = c
            .get("email")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let expire_at = c
            .get("expireAt")
            .and_then(Value::as_str)
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.naive_utc());
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
        let device_limit = c
            .get("deviceLimit")
            .and_then(Value::as_i64)
            .and_then(|v| i32::try_from(v).ok());
        let cid = c
            .get("id")
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_else(crate::db::new_id);
        let pw_hash = hash_password(&crate::auth::crypto::random_hex(12), 10).await?;
        let secrets = c.get("secrets").and_then(Value::as_object);
        prepared_clients.push(PreparedClient {
            cid,
            uuid,
            username,
            email,
            pw_hash,
            traffic_limit,
            expire_at,
            banned,
            protocols,
            sub_token,
            device_limit,
            reset_strategy,
            secrets,
        });
    }

    let upsert = mode == "upsert";
    let mut created_nodes = 0usize;
    let mut created_users = 0usize;
    let mut imported_clients = 0usize;
    let now = crate::db::now();
    let mut tx = state.db.begin().await?;

    for n in &nodes {
        let existing_id: Option<String> = sqlx::query_scalar(
            r#"SELECT id FROM "Node" WHERE host = $1 AND "apiPort" = $2 LIMIT 1"#,
        )
        .bind(&n.host)
        .bind(n.api_port)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(eid) = existing_id {
            if upsert {
                sqlx::query(
                    r#"UPDATE "Node"
                       SET name = $2, port = $3, country = $4, city = $5, isp = $6,
                           tags = $7, active = $8, "updatedAt" = $9
                       WHERE id = $1"#,
                )
                .bind(&eid)
                .bind(&n.name)
                .bind(n.port)
                .bind(&n.country)
                .bind(&n.city)
                .bind(&n.isp)
                .bind(&n.tags)
                .bind(n.active)
                .bind(now)
                .execute(&mut *tx)
                .await?;
                created_nodes += 1;
            }
            continue;
        }
        let secret = crate::auth::crypto::random_hex(24);
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

    for (u, pw_hash) in &prepared_users {
        let user_sql = if upsert {
            r#"INSERT INTO "User" (id, username, email, password, role, language, banned, "createdAt", "updatedAt")
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8)
               ON CONFLICT (username) DO UPDATE SET
                 email = COALESCE(EXCLUDED.email, "User".email),
                 role = EXCLUDED.role,
                 language = EXCLUDED.language,
                 banned = EXCLUDED.banned,
                 "updatedAt" = EXCLUDED."updatedAt""#
        } else {
            r#"INSERT INTO "User" (id, username, email, password, role, language, banned, "createdAt", "updatedAt")
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8)
               ON CONFLICT DO NOTHING"#
        };
        let res = sqlx::query(user_sql)
            .bind(crate::db::new_id())
            .bind(&u.username)
            .bind(&u.email)
            .bind(pw_hash)
            .bind(u.role)
            .bind(&u.language)
            .bind(u.banned)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        created_users += usize::try_from(res.rows_affected()).unwrap_or(0);
    }

    for pc in prepared_clients {
        let sql = if upsert {
            r#"INSERT INTO "Client" (id, uuid, username, email, password, "trafficLimit", "expireAt", banned, protocols, "subToken", "deviceLimit", "resetStrategy", "createdAt", "updatedAt")
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $13)
               ON CONFLICT (uuid) DO UPDATE SET
                 email = COALESCE(EXCLUDED.email, "Client".email),
                 "trafficLimit" = EXCLUDED."trafficLimit",
                 "expireAt" = COALESCE(EXCLUDED."expireAt", "Client"."expireAt"),
                 banned = EXCLUDED.banned,
                 protocols = EXCLUDED.protocols,
                 "deviceLimit" = EXCLUDED."deviceLimit",
                 "resetStrategy" = EXCLUDED."resetStrategy",
                 "updatedAt" = EXCLUDED."updatedAt"
               RETURNING id"#
        } else {
            r#"INSERT INTO "Client" (id, uuid, username, email, password, "trafficLimit", "expireAt", banned, protocols, "subToken", "deviceLimit", "resetStrategy", "createdAt", "updatedAt")
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $13)
               ON CONFLICT DO NOTHING
               RETURNING id"#
        };
        let persisted_cid: Option<String> = sqlx::query_scalar(sql)
            .bind(&pc.cid)
            .bind(&pc.uuid)
            .bind(pc.username)
            .bind(pc.email)
            .bind(&pc.pw_hash)
            .bind(pc.traffic_limit)
            .bind(pc.expire_at)
            .bind(pc.banned)
            .bind(&pc.protocols)
            .bind(&pc.sub_token)
            .bind(pc.device_limit)
            .bind(pc.reset_strategy)
            .bind(now)
            .fetch_optional(&mut *tx)
            .await?;

        let Some(actual_cid) = persisted_cid else {
            continue;
        };
        imported_clients += 1;

        if let (Some(enc_key), Some(secrets)) = (state.encryption_key(), pc.secrets) {
            let cred_sql = if upsert {
                r#"INSERT INTO "ProtocolCredential" (id, "clientId", protocol, "encryptedSecret", "createdAt", "updatedAt")
                   VALUES ($1, $2, $3, $4, $5, $5)
                   ON CONFLICT ("clientId", protocol) DO UPDATE SET
                     "encryptedSecret" = EXCLUDED."encryptedSecret",
                     "updatedAt" = EXCLUDED."updatedAt""#
            } else {
                r#"INSERT INTO "ProtocolCredential" (id, "clientId", protocol, "encryptedSecret", "createdAt", "updatedAt")
                   VALUES ($1, $2, $3, $4, $5, $5)
                   ON CONFLICT ("clientId", protocol) DO NOTHING"#
            };
            for (proto_str, enc_val) in secrets {
                if let (Ok(proto), Some(enc_str)) = (Protocol::from_str(proto_str), enc_val.as_str()) {
                    if let Ok(plain) = decrypt_str(Some(enc_key), enc_str, AAD_CONFIG_TRANSFER_V2) {
                        if let Ok(re_enc) = encrypt_str(Some(enc_key), &plain, crate::auth::crypto::AAD_PROTOCOL_CREDENTIAL) {
                            sqlx::query(cred_sql)
                                .bind(crate::db::new_id())
                                .bind(&actual_cid)
                                .bind(DbProtocol::from(proto))
                                .bind(re_enc)
                                .bind(now)
                                .execute(&mut *tx)
                                .await?;
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
        "createdNodes": created_nodes,
        "createdUsers": created_users,
        "importedClients": imported_clients
    }))
}
