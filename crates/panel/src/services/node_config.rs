use crate::db::enums::{DbProtocol, Protocol};
use crate::error::AppResult;
use crate::services::enforcement::is_client_allowed;
use crate::services::protocol_credentials::{
    get_protocol_secret_raw, get_protocol_secrets_bulk, normalize_ss_secret,
};
use crate::state::AppState;
use chrono::NaiveDateTime;
use mypanel_proto::{InboundConfig, NodeUser, PortShareConfig, RoutingRuleConfig};
use serde_json::{Map, Value};
use sqlx::Row;
use std::collections::HashMap;

struct LinkRow {
    inbound_id: String,
    client_id: String,
    username: String,
    uuid: String,
    banned: bool,
    on_hold: bool,
    expire_at: Option<NaiveDateTime>,
    traffic_limit: i64,
    used_traffic: i64,
    restriction_reason: Option<String>,
    protocols: Value,
}

struct ShareRow {
    id: String,
    inbound_id: String,
    protocol: Protocol,
    tag: String,
    host: Option<String>,
    path: Option<String>,
    settings: Value,
    stream: Value,
    enable: bool,
}

/// Builds the full inbound configuration for a node (`` + `` bulk queries + `PortShare` `c.protocols` filter).
pub async fn build(state: &AppState, node_id: &str) -> AppResult<Vec<InboundConfig>> {
    let now = crate::db::now();
    let inbounds = sqlx::query(
        r#"SELECT id, protocol, tag, port, listen, settings, stream, routing, sniffing, remark, enable
           FROM "Inbound"
           WHERE "nodeId" = $1 AND enable = true
           ORDER BY port ASC, id ASC"#,
    )
    .bind(node_id)
    .fetch_all(&state.db)
    .await?;

    if inbounds.is_empty() {
        return Ok(Vec::new());
    }

    let inbound_ids: Vec<String> = inbounds
        .iter()
        .filter_map(|r| r.try_get::<String, _>("id").ok())
        .collect();

    // Bulk fetch all ClientInbound + Client rows (guarded by ClientNode / allNodes)
    let link_rows = sqlx::query(
        r#"SELECT ci."inboundId" AS inbound_id, ci."clientId" AS client_id,
                  c.username, c.uuid, c.banned, COALESCE(c."onHold", false) AS on_hold,
                  c."expireAt" AS expire_at,
                  c."trafficLimit" AS traffic_limit, c."usedTraffic" AS used_traffic,
                  c."restrictionReason" AS restriction_reason, c.protocols
           FROM "ClientInbound" ci
           JOIN "Client" c ON c.id = ci."clientId"
           WHERE ci."inboundId" = ANY($1::text[]) AND ci.enable = true
             AND (
               c."allNodes" = true
               OR EXISTS (
                 SELECT 1 FROM "ClientNode" cn
                 WHERE cn."clientId" = c.id AND cn."nodeId" = $2
               )
             )
           ORDER BY ci."createdAt" ASC, ci.id ASC"#,
    )
    .bind(&inbound_ids)
    .bind(node_id)
    .fetch_all(&state.db)
    .await?;

    let mut all_links = Vec::with_capacity(link_rows.len());
    let mut client_ids = Vec::with_capacity(link_rows.len());
    for r in link_rows {
        let client_id: String = r.try_get("client_id")?;
        client_ids.push(client_id.clone());
        all_links.push(LinkRow {
            inbound_id: r.try_get("inbound_id")?,
            client_id,
            username: r.try_get("username")?,
            uuid: r.try_get("uuid")?,
            banned: r.try_get("banned")?,
            on_hold: r.try_get("on_hold")?,
            expire_at: r.try_get("expire_at")?,
            traffic_limit: r.try_get("traffic_limit")?,
            used_traffic: r.try_get("used_traffic")?,
            restriction_reason: r.try_get("restriction_reason")?,
            protocols: r.try_get("protocols")?,
        });
    }
    client_ids.sort_unstable();
    client_ids.dedup();

    // Bulk fetch all PortShares
    let share_rows = sqlx::query(
        r#"SELECT id, "inboundId" AS inbound_id, protocol, tag, host, path, settings, stream, enable
           FROM "PortShare"
           WHERE "inboundId" = ANY($1::text[]) AND enable = true
           ORDER BY "createdAt" ASC, id ASC"#,
    )
    .bind(&inbound_ids)
    .fetch_all(&state.db)
    .await?;

    let mut all_shares = Vec::with_capacity(share_rows.len());
    for r in share_rows {
        all_shares.push(ShareRow {
            id: r.try_get("id")?,
            inbound_id: r.try_get("inbound_id")?,
            protocol: r.try_get::<DbProtocol, _>("protocol")?.into(),
            tag: r.try_get("tag")?,
            host: r.try_get("host")?,
            path: r.try_get("path")?,
            settings: r.try_get("settings")?,
            stream: r.try_get("stream")?,
            enable: r.try_get("enable")?,
        });
    }

    // Bulk load ProtocolCredential secrets
    let mut secret_cache: HashMap<(String, Protocol), String> =
        get_protocol_secrets_bulk(state, &client_ids).await?;

    let mut result = Vec::new();

    for inbound in inbounds {
        let inbound_id: String = inbound.try_get("id")?;
        let protocol: Protocol = inbound.try_get::<DbProtocol, _>("protocol")?.into();
        let tag: String = inbound.try_get("tag")?;
        let port: i32 = inbound.try_get("port")?;
        let listen: String = inbound.try_get("listen")?;
        let raw_settings: Value = inbound.try_get("settings")?;
        let stream: Value = inbound.try_get("stream")?;
        let routing: Value = inbound.try_get("routing")?;
        let sniffing: bool = inbound.try_get("sniffing")?;
        let remark: Option<String> = inbound.try_get("remark")?;
        let enable: bool = inbound.try_get("enable")?;

        let ss_method = raw_settings.get("method").and_then(Value::as_str);
        let flow = raw_settings
            .get("flow")
            .and_then(Value::as_str)
            .or_else(|| stream.get("flow").and_then(Value::as_str))
            .map(ToString::to_string);

        let links: Vec<&LinkRow> = all_links
            .iter()
            .filter(|l| l.inbound_id == inbound_id)
            .collect();

        let mut users = Vec::new();
        for link in &links {
            if !is_client_allowed(
                link.banned,
                link.on_hold,
                link.expire_at,
                link.traffic_limit,
                link.used_traffic,
                link.restriction_reason.as_deref(),
                now,
            ) {
                continue;
            }
            if !protocol_allowed(&link.protocols, protocol) {
                continue;
            }
            let Some(secret) = resolve_user_secret(
                state,
                &mut secret_cache,
                &link.client_id,
                &link.uuid,
                protocol,
                ss_method,
            )
            .await?
            else {
                continue;
            };

            users.push(NodeUser {
                id: link.uuid.clone(),
                username: link.username.clone(),
                uuid: link.uuid.clone(),
                email: link.client_id.clone(),
                name: link.client_id.clone(),
                password: Some(secret.clone()),
                auth: Some(secret),
                flow: flow.clone(),
                level: 0,
            });
        }

        let shares: Vec<&ShareRow> = all_shares
            .iter()
            .filter(|s| s.inbound_id == inbound_id)
            .collect();

        let mut port_shares = Vec::new();
        for share in shares {
            let share_ss_method = share.settings.get("method").and_then(Value::as_str);
            let share_flow = share
                .settings
                .get("flow")
                .and_then(Value::as_str)
                .or_else(|| share.stream.get("flow").and_then(Value::as_str))
                .map(ToString::to_string);

            let mut share_users = Vec::new();
            for link in &links {
                // Enforce Client.protocols on PortShare!
                if !protocol_allowed(&link.protocols, share.protocol) {
                    continue;
                }
                if !is_client_allowed(
                    link.banned,
                    link.on_hold,
                    link.expire_at,
                    link.traffic_limit,
                    link.used_traffic,
                    link.restriction_reason.as_deref(),
                    now,
                ) {
                    continue;
                }
                let Some(secret) = resolve_user_secret(
                    state,
                    &mut secret_cache,
                    &link.client_id,
                    &link.uuid,
                    share.protocol,
                    share_ss_method,
                )
                .await?
                else {
                    continue;
                };
                share_users.push(NodeUser {
                    id: link.uuid.clone(),
                    username: link.username.clone(),
                    uuid: link.uuid.clone(),
                    email: link.client_id.clone(),
                    name: link.client_id.clone(),
                    password: Some(secret.clone()),
                    auth: Some(secret),
                    flow: share_flow.clone(),
                    level: 0,
                });
            }

            if share_users.is_empty() {
                continue;
            }

            let share_settings = clean_settings_with_users(&share.settings, &share_users);
            port_shares.push(PortShareConfig {
                id: share.id.clone(),
                tag: share.tag.clone(),
                protocol: share.protocol,
                host: share.host.clone(),
                path: share.path.clone(),
                settings: share_settings,
                stream: share.stream.clone(),
                enable: share.enable,
                users: share_users,
            });
        }

        if users.is_empty() && port_shares.is_empty() {
            continue;
        }

        let clean_settings = clean_settings_with_users(&raw_settings, &users);

        result.push(InboundConfig {
            id: inbound_id,
            protocol,
            tag,
            port: u16::try_from(port).unwrap_or(443),
            listen,
            settings: clean_settings,
            stream,
            routing,
            sniffing,
            remark,
            enable,
            users,
            port_shares,
            hosts: Vec::new(),
        });
    }

    Ok(result)
}

pub async fn build_xray_profile(
    state: &AppState,
    node_id: &str,
) -> AppResult<Option<Value>> {
    let row = sqlx::query(r#"SELECT "xrayProfile" FROM "Node" WHERE id = $1"#)
        .bind(node_id)
        .fetch_optional(&state.db)
        .await?;
    let Some(r) = row else {
        return Ok(None);
    };
    let prof: Option<Value> = r.try_get("xrayProfile").ok().flatten();
    Ok(prof.filter(|v| v.is_object() && !v.as_object().is_some_and(|m| m.is_empty())))
}

pub async fn build_routing_rules(
    state: &AppState,
    node_id: &str,
) -> AppResult<Vec<RoutingRuleConfig>> {
    let rows = sqlx::query(
        r#"SELECT id, "type", domain, ip, port, "sourcePort" AS source_port,
                  source, protocol, "inboundTag" AS inbound_tag, "outboundTag" AS outbound_tag,
                  "balancerTag" AS balancer_tag
           FROM "RoutingRule"
           WHERE enabled = true AND ("nodeScope" IS NULL OR "nodeScope" = $1)
           ORDER BY priority DESC, "createdAt" ASC"#,
    )
    .bind(node_id)
    .fetch_all(&state.db)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        out.push(RoutingRuleConfig {
            id: r.try_get("id")?,
            rule_type: r.try_get("type")?,
            domain: r.try_get::<Option<Vec<String>>, _>("domain")?.unwrap_or_default(),
            ip: r.try_get::<Option<Vec<String>>, _>("ip")?.unwrap_or_default(),
            port: r.try_get("port")?,
            source_port: r.try_get("source_port")?,
            source: r.try_get::<Option<Vec<String>>, _>("source")?.unwrap_or_default(),
            protocol: r.try_get::<Option<Vec<String>>, _>("protocol")?.unwrap_or_default(),
            inbound_tag: r.try_get::<Option<Vec<String>>, _>("inbound_tag")?.unwrap_or_default(),
            outbound_tag: r.try_get("outbound_tag")?,
            balancer_tag: r.try_get("balancer_tag")?,
        });
    }
    Ok(out)
}

fn protocol_allowed(protocols_json: &Value, target: Protocol) -> bool {
    match protocols_json.as_array() {
        Some(arr) => arr.iter().any(|v| {
            v.as_str()
                .is_some_and(|s| s.eq_ignore_ascii_case(target.as_str()))
        }),
        None => false,
    }
}

/// Returns the credential a node must accept for `client` on `protocol`.
///
/// UUID-auth protocols use the client UUID. Every other protocol needs its own stored secret;
/// when `ENCRYPTION_KEY` is not configured there is none, and the user is skipped (`None`) instead
/// of reusing the UUID, which would leak the VLESS identity as a Naive/Mieru/SS/Trojan password.
async fn resolve_user_secret(
    state: &AppState,
    cache: &mut HashMap<(String, Protocol), String>,
    client_id: &str,
    client_uuid: &str,
    protocol: Protocol,
    ss_method: Option<&str>,
) -> AppResult<Option<String>> {
    if protocol.is_uuid_auth() {
        return Ok(Some(client_uuid.to_string()));
    }
    if let Some(cached) = cache.get(&(client_id.to_string(), protocol)) {
        return Ok(Some(normalize_ss_secret(cached, protocol, ss_method)));
    }
    if state.encryption_key().is_some() {
        let raw = get_protocol_secret_raw(state, client_id, protocol, ss_method).await?;
        cache.insert((client_id.to_string(), protocol), raw.clone());
        return Ok(Some(normalize_ss_secret(&raw, protocol, ss_method)));
    }
    tracing::warn!(
        client_id,
        protocol = protocol.as_str(),
        "ENCRYPTION_KEY is not configured: skipping user for non-UUID protocol"
    );
    Ok(None)
}

fn clean_settings_with_users(raw: &Value, users: &[NodeUser]) -> Value {
    let mut map = match raw.as_object() {
        Some(m) => m.clone(),
        None => Map::new(),
    };
    for key in ["id", "uuid", "email", "username", "auth"] {
        map.remove(key);
    }
    let users_val = serde_json::to_value(users).unwrap_or_else(|_| Value::Array(Vec::new()));
    map.insert("clients".to_string(), users_val.clone());
    map.insert("users".to_string(), users_val);
    Value::Object(map)
}
