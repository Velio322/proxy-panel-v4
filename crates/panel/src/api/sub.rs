use crate::api::common::valid_id;
use crate::auth::extract::AuthUser;
use crate::cache::CachedSubscription;
use crate::db::enums::{DbProtocol, Protocol};
use crate::error::{AppError, AppResult};
use crate::net::{user_agent, ClientIp};
use crate::services::{enforcement, protocol_credentials};
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::NaiveDateTime;
use mypanel_proto::links::{
    build_entry, build_port_share_entry, expand_with_hosts, EntryContext, PortShareInput, SubEntry,
};
use mypanel_proto::subscription::{
    self, build_happ_crypt4, build_happ_crypt5, detect_client_from_ua, AgeEncryptionConfig,
    ClientProfile, SrrAction, SrrCondition, SrrContext, SrrOperator, SrrRule, SubFormat,
    SubscriptionRenderer, SubscriptionTemplate, UserInfo,
};
use mypanel_proto::HostOverrideConfig;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/{sub_token}/sub", get(get_subscription))
        .route("/{sub_token}/info", get(get_info))
        .route("/{sub_token}/regenerate", post(regenerate))
        .route("/{sub_token}/happ", get(get_happ_encrypted))
}

#[derive(Debug, Deserialize)]
pub(crate) struct SubQuery {
    flag: Option<String>,
    #[serde(default)]
    format: Option<String>,
}

fn plain_err(status: StatusCode, msg: &str) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        msg.to_string(),
    )
        .into_response()
}

fn apply_sub_headers(
    mut res: Response,
    info: &UserInfo,
    format: SubFormat,
    profile: ClientProfile,
    hwid_headers: Option<(i32, i64)>,
) -> Response {
    for (k, v) in subscription::headers(info, format, profile) {
        if let (Ok(name), Ok(val)) = (
            HeaderName::from_bytes(k.as_bytes()),
            HeaderValue::from_str(&v),
        ) {
            res.headers_mut().insert(name, val);
        }
    }
    if let Some((limit, count)) = hwid_headers {
        if let Ok(v) = HeaderValue::from_str(&limit.to_string()) {
            res.headers_mut().insert("x-hwid-limit", v);
        }
        if let Ok(v) = HeaderValue::from_str(&count.to_string()) {
            res.headers_mut().insert("x-hwid-count", v);
        }
    }
    res
}

async fn enforce_hwid_device(
    state: &AppState,
    client_id: &str,
    device_limit: Option<i32>,
    headers: &HeaderMap,
    ip: &str,
    ua: Option<&str>,
) -> AppResult<Option<(i32, i64)>> {
    let hwid = headers
        .get("x-hwid")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let Some(hwid) = hwid else {
        return Ok(None);
    };

    let os = headers
        .get("x-device-os")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let os_ver = headers
        .get("x-ver-os")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let model = headers
        .get("x-device-model")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    let now = crate::db::now();
    let mut tx = state.db.begin().await?;
    // Advisory transaction lock keyed on client_id to prevent HWID race conditions
    sqlx::query(r#"SELECT pg_advisory_xact_lock(hashtext($1))"#)
        .bind(client_id)
        .execute(&mut *tx)
        .await?;

    let existing: Option<String> = sqlx::query_scalar(
        r#"SELECT id FROM "Device" WHERE "clientId" = $1 AND hwid = $2"#,
    )
    .bind(client_id)
    .bind(hwid)
    .fetch_optional(&mut *tx)
    .await?;

    if let Some(dev_id) = existing {
        sqlx::query(
            r#"UPDATE "Device" SET os = COALESCE($2, os), "osVersion" = COALESCE($3, "osVersion"),
                      model = COALESCE($4, model), "userAgent" = COALESCE($5, "userAgent"),
                      ip = $6, "lastSeenAt" = $7, "updatedAt" = $7
               WHERE id = $1"#,
        )
        .bind(dev_id)
        .bind(&os)
        .bind(&os_ver)
        .bind(&model)
        .bind(ua)
        .bind(ip)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    } else {
        let count: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Device" WHERE "clientId" = $1"#)
                .bind(client_id)
                .fetch_one(&mut *tx)
                .await?;
        if let Some(limit) = device_limit.filter(|&l| l > 0) {
            if count >= i64::from(limit) {
                tx.commit().await?;
                return Err(AppError::forbidden("Device HWID limit reached"));
            }
        }
        sqlx::query(
            r#"INSERT INTO "Device" (id, "clientId", hwid, os, "osVersion", model, "userAgent", ip, "lastSeenAt", "createdAt", "updatedAt")
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9, $9)"#,
        )
        .bind(crate::db::new_id())
        .bind(client_id)
        .bind(hwid)
        .bind(&os)
        .bind(&os_ver)
        .bind(&model)
        .bind(ua)
        .bind(ip)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }

    let final_count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Device" WHERE "clientId" = $1"#)
            .bind(client_id)
            .fetch_one(&mut *tx)
            .await?;

    tx.commit().await?;
    Ok(device_limit.map(|l| (l, final_count)))
}

fn map_template_type(raw: &str) -> Option<subscription::TemplateType> {
    use subscription::TemplateType;
    match raw.trim().to_ascii_uppercase().as_str() {
        "XRAY_JSON" | "ORIGINAL" => Some(TemplateType::XrayJson),
        "XRAY_BASE64" | "BASE64" | "HAPP" => Some(TemplateType::XrayBase64),
        "MIHOMO" => Some(TemplateType::Mihomo),
        "CLASH" => Some(TemplateType::Clash),
        "STASH" => Some(TemplateType::Stash),
        "SINGBOX" | "SING_BOX" | "SING-BOX" => Some(TemplateType::Singbox),
        _ => None,
    }
}

async fn load_subscription_template_by_id(
    state: &AppState,
    id: &str,
) -> Option<SubscriptionTemplate> {
    let row = sqlx::query(
        r#"SELECT id, name, "templateType", content FROM "SubscriptionTemplate" WHERE id = $1"#,
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .ok()??;
    let tt_str: String = row.try_get("templateType").ok()?;
    Some(SubscriptionTemplate {
        id: row.try_get("id").ok()?,
        name: row.try_get("name").ok()?,
        template_type: map_template_type(&tt_str)?,
        content: row.try_get("content").ok()?,
    })
}

async fn load_default_subscription_template(
    state: &AppState,
    format: SubFormat,
) -> Option<SubscriptionTemplate> {
    let candidates: &[&str] = match format {
        SubFormat::Original => &["XRAY_JSON"],
        SubFormat::Base64 | SubFormat::Happ => &["XRAY_BASE64"],
        SubFormat::Clash => &["MIHOMO", "CLASH", "STASH"],
        SubFormat::Singbox => &["SINGBOX"],
        SubFormat::Json => return None,
    };
    let rows = sqlx::query(
        r#"SELECT id, name, "templateType", content
           FROM "SubscriptionTemplate"
           WHERE "isDefault" = true
           ORDER BY "updatedAt" DESC"#,
    )
    .fetch_all(&state.db)
    .await
    .ok()?;
    for row in rows {
        let Ok(tt_str) = row.try_get::<String, _>("templateType") else {
            continue;
        };
        let norm = tt_str.trim().to_ascii_uppercase();
        if candidates.iter().any(|c| *c == norm) {
            if let Some(tt) = map_template_type(&norm) {
                return Some(SubscriptionTemplate {
                    id: row.try_get("id").ok()?,
                    name: row.try_get("name").ok()?,
                    template_type: tt,
                    content: row.try_get("content").ok()?,
                });
            }
        }
    }
    None
}

async fn load_srr_rules(state: &AppState) -> Vec<SrrRule> {
    let Ok(rows) = sqlx::query(
        r#"SELECT id, name, enabled, priority, "matchHeader", "matchOperator", "matchPattern",
                  negate, conditions, action, "templateId", "overrideFormat", "customRemarks",
                  "agePublicKey", "ageConfig"
           FROM "SrrRule" WHERE enabled = true ORDER BY priority ASC"#,
    )
    .fetch_all(&state.db)
    .await
    else {
        return Vec::new();
    };

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let id: String = r.try_get("id").unwrap_or_default();
        let name: String = r.try_get("name").unwrap_or_default();
        let enabled: bool = r.try_get("enabled").unwrap_or(true);
        let priority: i32 = r.try_get("priority").unwrap_or(0);
        let match_header: String = r.try_get("matchHeader").unwrap_or_else(|_| "user-agent".into());
        let match_op_str: String = r.try_get("matchOperator").unwrap_or_else(|_| "CONTAINS".into());
        let match_pattern: String = r.try_get("matchPattern").unwrap_or_default();
        let negate: bool = r.try_get("negate").unwrap_or(false);
        let action_str: String = r.try_get("action").unwrap_or_else(|_| "ALLOW".into());
        let template_id: Option<String> = r.try_get("templateId").ok().flatten();
        let override_fmt: Option<String> = r.try_get("overrideFormat").ok().flatten();
        let custom_remarks: Option<String> = r.try_get("customRemarks").ok().flatten();
        let age_pub: Option<String> = r.try_get("agePublicKey").ok().flatten();
        let conds_val: Value = r.try_get("conditions").unwrap_or_else(|_| json!([]));

        let op = match match_op_str.to_ascii_uppercase().as_str() {
            "EQUALS" => SrrOperator::Equals,
            "REGEX" => SrrOperator::Regex,
            _ => SrrOperator::Contains,
        };
        let mut conditions: Vec<SrrCondition> = serde_json::from_value(conds_val).unwrap_or_default();
        if conditions.is_empty() && !match_pattern.is_empty() {
            conditions.push(SrrCondition {
                header: match_header,
                operator: op,
                pattern: match_pattern,
                negate,
            });
        }

        let action = match action_str.to_ascii_uppercase().as_str() {
            "ALLOW" => continue,
            "SOCKET_DROP" => SrrAction::SocketDrop,
            "REJECT_404" | "BLOCK_404" | "404" => SrrAction::RejectStatus(404),
            "REJECT_451" | "BLOCK_451" | "451" => SrrAction::RejectStatus(451),
            "RESPOND_WITH_REMARKS" => SrrAction::RespondWithRemarks(
                custom_remarks
                    .unwrap_or_default()
                    .lines()
                    .map(ToString::to_string)
                    .collect(),
            ),
            "SELECT_TEMPLATE" | "USE_TEMPLATE" => {
                SrrAction::SelectTemplate(template_id.clone().unwrap_or_default())
            }
            "OVERRIDE_FORMAT" => SrrAction::OverrideFormat(
                override_fmt
                    .as_deref()
                    .and_then(|f| SubFormat::from_flag(f).ok())
                    .unwrap_or(SubFormat::Base64),
            ),
            "AGE_ENCRYPT" | "ENCRYPT_AGE" => SrrAction::AgeEncrypt,
            _ => continue,
        };

        let age_config = age_pub.filter(|s| !s.is_empty()).map(|pk| {
            AgeEncryptionConfig::Recipients(vec![pk])
        });

        out.push(SrrRule {
            id,
            name,
            enabled,
            priority,
            conditions,
            action,
            age_config,
        });
    }
    out
}

fn record_srh(
    state: &AppState,
    client_id: Option<String>,
    username: Option<String>,
    ip: String,
    ua: String,
    profile: ClientProfile,
    format: SubFormat,
    status: i32,
    hwid: Option<String>,
) {
    let db = state.db.clone();
    tokio::spawn(async move {
        let _ = sqlx::query(
            r#"INSERT INTO "SubRequestHistory" (id, "clientId", username, ip, "userAgent", "detectedApp", format, status, hwid, "createdAt")
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"#,
        )
        .bind(crate::db::new_id())
        .bind(client_id)
        .bind(username)
        .bind(ip)
        .bind(ua)
        .bind(format!("{profile:?}"))
        .bind(format!("{format:?}"))
        .bind(status)
        .bind(hwid)
        .bind(crate::db::now())
        .execute(&db)
        .await;
    });
}

fn format_human_bytes(bytes: i64) -> String {
    let b = bytes.max(0) as f64;
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if b < 1024.0 {
        return format!("{} B", bytes.max(0));
    }
    let mut val = b;
    let mut idx = 0;
    while val >= 1024.0 && idx < UNITS.len() - 1 {
        val /= 1024.0;
        idx += 1;
    }
    format!("{val:.2} {}", UNITS[idx])
}

fn render_landing_html(
    username: &str,
    sub_base_url: &str,
    sub_token: &str,
    used_traffic: i64,
    traffic_limit: i64,
    entries: &[SubEntry],
    info: &UserInfo,
) -> String {
    use crate::services::telegram::esc;

    let base = sub_base_url.trim_end_matches('/');
    let sub_link = format!("{base}/sub/{sub_token}");
    let traffic_str = if traffic_limit > 0 {
        format!(
            "{} / {}",
            format_human_bytes(used_traffic),
            format_human_bytes(traffic_limit)
        )
    } else {
        format!("{} / ∞", format_human_bytes(used_traffic))
    };

    let mut cards_html = String::new();
    let parsed_entries = subscription::render_json(entries, info)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .and_then(|v| v.get("entries").and_then(Value::as_array).cloned())
        .unwrap_or_default();

    if parsed_entries.is_empty() {
        cards_html.push_str(
            "<div class=\"card\"><p class=\"muted\">No active proxy nodes available for this subscription.</p></div>",
        );
    } else {
        for item in &parsed_entries {
            let proto = item.get("protocol").and_then(Value::as_str).unwrap_or("PROXY");
            let tag = item.get("tag").and_then(Value::as_str).unwrap_or("");
            let host = item.get("host").and_then(Value::as_str).unwrap_or("");
            let port = item.get("port").and_then(Value::as_u64).unwrap_or(0);
            let link = item.get("link").and_then(Value::as_str).unwrap_or("");
            let cfg_block = match item.get("config").filter(|c| !c.is_null()) {
                Some(cfg) => {
                    let pretty = serde_json::to_string_pretty(cfg).unwrap_or_default();
                    format!(
                        "<details open><summary>Native JSON Config</summary><pre>{}</pre></details>",
                        esc(&pretty)
                    )
                }
                None => String::new(),
            };
            cards_html.push_str(&format!(
                "<div class=\"card\">\
                 <div class=\"card-head\"><span class=\"badge\">{}</span><strong>{}</strong>\
                 <span class=\"mono muted\">{}:{}</span></div>\
                 <label class=\"label\">Share Link</label>\
                 <textarea readonly rows=\"2\" class=\"mono\">{}</textarea>\
                 {}\
                 </div>",
                esc(proto),
                esc(tag),
                esc(host),
                port,
                esc(link),
                cfg_block,
            ));
        }
    }

    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <title>{}</title>\
         <style>\
         body{{font-family:system-ui,-apple-system,sans-serif;background:#0b1120;color:#e2e8f0;max-width:780px;margin:32px auto;padding:0 16px;line-height:1.5}}\
         h1{{margin:0 0 8px;font-size:1.5rem}}\
         .muted{{color:#94a3b8;font-size:0.875rem}}\
         .mono{{font-family:ui-monospace,SFMono-Regular,Menlo,monospace;font-size:0.8rem}}\
         .card{{background:#111827;border:1px solid #1e293b;border-radius:12px;padding:16px;margin:14px 0}}\
         .card-head{{display:flex;flex-wrap:wrap;align-items:center;gap:8px;margin-bottom:10px}}\
         .badge{{background:#1d4ed8;color:#eff6ff;border-radius:6px;padding:2px 8px;font-size:0.75rem;font-weight:600}}\
         .label{{display:block;font-size:0.75rem;color:#94a3b8;margin-bottom:4px}}\
         textarea{{width:100%;box-sizing:border-box;background:#020617;color:#e2e8f0;border:1px solid #334155;border-radius:8px;padding:8px;resize:vertical}}\
         pre{{background:#020617;border:1px solid #334155;border-radius:8px;padding:10px;overflow-x:auto;font-size:0.78rem;margin:8px 0 0}}\
         details{{margin-top:10px}}\
         summary{{cursor:pointer;font-size:0.8rem;color:#93c5fd}}\
         .links{{display:flex;flex-wrap:wrap;gap:8px;margin-top:10px}}\
         .pill{{display:inline-block;padding:4px 10px;border:1px solid #334155;border-radius:6px;color:#93c5fd;text-decoration:none;font-size:0.8rem}}\
         .pill:hover{{background:#1e293b}}\
         </style></head>\
         <body>\
         <h1>{}</h1>\
         <p class=\"muted\">Traffic: {}</p>\
         <div class=\"card\">\
           <label class=\"label\">Subscription URL</label>\
           <textarea readonly rows=\"1\" class=\"mono\">{}</textarea>\
           <div class=\"links\">\
             <a class=\"pill\" href=\"{}?flag=base64\">Base64</a>\
             <a class=\"pill\" href=\"{}?flag=clash\">Clash / Mihomo</a>\
             <a class=\"pill\" href=\"{}?flag=singbox\">Sing-box</a>\
             <a class=\"pill\" href=\"{}?flag=json\">JSON (All / Native)</a>\
             <a class=\"pill\" href=\"{}?flag=original\">Original JSON</a>\
           </div>\
         </div>\
         {}\
         </body></html>",
        esc(username),
        esc(username),
        esc(&traffic_str),
        esc(&sub_link),
        esc(&sub_link),
        esc(&sub_link),
        esc(&sub_link),
        esc(&sub_link),
        esc(&sub_link),
        cards_html,
    )
}

pub(crate) async fn load_client_entries(
    state: &AppState,
    client_id: &str,
    uuid: &str,
    protocols_val: &Value,
) -> AppResult<Vec<SubEntry>> {
    let allowed_protocols: Vec<String> = protocols_val
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(|s| s.to_ascii_uppercase())
                .collect()
        })
        .unwrap_or_else(|| vec!["VLESS".into(), "HYSTERIA2".into()]);

    let inbound_rows = sqlx::query(
        r#"SELECT i.id, i.protocol, i.tag, i.port, i.settings, i.stream, n.host
           FROM "ClientInbound" ci
           JOIN "Client" c ON c.id = ci."clientId"
           JOIN "Inbound" i ON i.id = ci."inboundId"
           JOIN "Node" n ON n.id = i."nodeId"
           WHERE ci."clientId" = $1 AND ci.enable = true AND i.enable = true
             AND n.active = true AND n.status = 'ONLINE'
             AND (
               c."allNodes" = true
               OR EXISTS (
                 SELECT 1 FROM "ClientNode" cn
                 WHERE cn."clientId" = ci."clientId" AND cn."nodeId" = i."nodeId"
               )
             )
           ORDER BY i."createdAt" ASC, i.id ASC"#,
    )
    .bind(client_id)
    .fetch_all(&state.db)
    .await?;

    let inbound_ids: Vec<String> = inbound_rows
        .iter()
        .filter_map(|r| r.try_get::<String, _>("id").ok())
        .collect();

    let mut shares_map: HashMap<String, Vec<sqlx::postgres::PgRow>> = HashMap::new();
    let mut hosts_map: HashMap<String, Vec<HostOverrideConfig>> = HashMap::new();

    if !inbound_ids.is_empty() {
        if let Ok(srows) = sqlx::query(
            r#"SELECT "inboundId", protocol, tag, host, path, settings, stream
               FROM "PortShare" WHERE "inboundId" = ANY($1::text[]) AND enable = true
               ORDER BY "createdAt" ASC"#,
        )
        .bind(&inbound_ids)
        .fetch_all(&state.db)
        .await
        {
            for sr in srows {
                if let Ok(iid) = sr.try_get::<String, _>("inboundId") {
                    shares_map.entry(iid).or_default().push(sr);
                }
            }
        }

        if let Ok(hrows) = sqlx::query(
            r#"SELECT id, "inboundId", remark, address, port, sni, host, path, alpn, fingerprint,
                      "xhttpExtra", "finalMask", "vlessRouteId", "shuffleHost", fragment, "isCdn",
                      COALESCE(tags, ARRAY[]::TEXT[]) AS tags, "allowInsecure", "pinnedPeerCertSha256",
                      "verifyPeerCertByName", "mihomoIpVersion", "serverDescription", enable
               FROM "Host"
               WHERE ("inboundId" = ANY($1::text[]) OR "inboundId" IS NULL) AND enable = true
               ORDER BY "sortOrder" ASC, "createdAt" ASC"#,
        )
        .bind(&inbound_ids)
        .fetch_all(&state.db)
        .await
        {
            for hr in hrows {
                let iid: Option<String> = hr.try_get("inboundId").ok().flatten();
                let key = iid.unwrap_or_default();
                let frag_val: Option<Value> = hr.try_get("fragment").ok().flatten();
                let frag_str = frag_val.and_then(|fv| match fv {
                    Value::String(s) => {
                        let t = s.trim();
                        if t.is_empty() { None } else { Some(t.to_string()) }
                    }
                    Value::Object(ref map) => {
                        let packets = map.get("packets").and_then(Value::as_str).unwrap_or("tlshello").trim();
                        let length = map.get("length").and_then(Value::as_str).unwrap_or("10-20").trim();
                        let interval = map.get("interval").and_then(Value::as_str).unwrap_or("10-20").trim();
                        Some(format!("{packets},{length},{interval}"))
                    }
                    _ => None,
                });
                hosts_map.entry(key).or_default().push(HostOverrideConfig {
                    id: hr.try_get("id").ok(),
                    inbound_id: hr.try_get("inboundId").ok().flatten(),
                    remark: hr.try_get("remark").ok(),
                    address: hr.try_get("address").ok(),
                    port: hr.try_get::<Option<i32>, _>("port").ok().flatten().and_then(|p| u16::try_from(p).ok()),
                    sni: hr.try_get("sni").ok().flatten(),
                    host: hr.try_get("host").ok().flatten(),
                    path: hr.try_get("path").ok().flatten(),
                    alpn: hr.try_get("alpn").ok().flatten(),
                    fingerprint: hr.try_get("fingerprint").ok().flatten(),
                    xhttp_extra: hr.try_get("xhttpExtra").ok().flatten(),
                    final_mask: hr.try_get("finalMask").ok().flatten(),
                    vless_route_id: hr.try_get("vlessRouteId").ok().flatten(),
                    shuffle_host: hr.try_get("shuffleHost").unwrap_or(false),
                    fragment: frag_str,
                    is_cdn: hr.try_get("isCdn").unwrap_or(false),
                    enable: hr.try_get("enable").unwrap_or(true),
                    tags: hr.try_get::<Option<Vec<String>>, _>("tags").ok().flatten().unwrap_or_default(),
                    allow_insecure: hr.try_get::<Option<bool>, _>("allowInsecure").ok().flatten(),
                    pinned_peer_cert_sha256: hr.try_get::<Option<String>, _>("pinnedPeerCertSha256").ok().flatten(),
                    verify_peer_cert_by_name: hr.try_get::<Option<String>, _>("verifyPeerCertByName").ok().flatten(),
                    mihomo_ip_version: hr.try_get::<Option<String>, _>("mihomoIpVersion").ok().flatten(),
                    server_description: hr.try_get::<Option<String>, _>("serverDescription").ok().flatten(),
                });
            }
        }
    }

    let mut entries: Vec<SubEntry> = Vec::new();
    for ir in inbound_rows {
        let iid: String = ir.try_get("id").unwrap_or_default();
        let db_proto: DbProtocol = match ir.try_get("protocol") {
            Ok(p) => p,
            Err(_) => continue,
        };
        let proto: Protocol = db_proto.into();
        if !allowed_protocols.iter().any(|p| p == proto.as_str()) {
            continue;
        }
        let tag: String = ir.try_get("tag").unwrap_or_default();
        let port: i32 = ir.try_get("port").unwrap_or(443);
        let settings: Value = ir.try_get("settings").unwrap_or_else(|_| json!({}));
        let stream: Value = ir.try_get("stream").unwrap_or_else(|_| json!({}));
        let host: String = ir.try_get("host").unwrap_or_default();
        let ss_method = settings.get("method").and_then(Value::as_str);

        let secret = if proto.is_uuid_auth() {
            String::new()
        } else {
            match protocol_credentials::get_protocol_secret(state, client_id, proto, ss_method).await {
                Ok(s) => s,
                Err(_) => continue,
            }
        };

        let ctx = EntryContext {
            node_host: host.clone(),
            port: u16::try_from(port).unwrap_or(443),
            client_uuid: uuid.to_string(),
            client_id: client_id.to_string(),
            secret,
        };

        if let Ok(Some(mut entry)) = build_entry(proto, &tag, &settings, &stream, &ctx) {
            entry.inbound_id = Some(iid.clone());
            let mut overrides = hosts_map.get(&iid).cloned().unwrap_or_default();
            if let Some(global) = hosts_map.get("") {
                overrides.extend(global.iter().cloned());
            }
            if let Ok(expanded) = expand_with_hosts(&[entry], &overrides) {
                entries.extend(expanded);
            }
        }

        if let Some(shares) = shares_map.get(&iid) {
            for sr in shares {
                let s_db_proto: DbProtocol = match sr.try_get("protocol") {
                    Ok(p) => p,
                    Err(_) => continue,
                };
                let s_proto: Protocol = s_db_proto.into();
                if !allowed_protocols.iter().any(|p| p == s_proto.as_str()) {
                    continue;
                }
                let s_tag: String = sr.try_get("tag").unwrap_or_default();
                let s_host: Option<String> = sr.try_get("host").ok().flatten();
                let s_path: Option<String> = sr.try_get("path").ok().flatten();
                let s_settings: Value = sr.try_get("settings").unwrap_or_else(|_| json!({}));
                let s_stream: Value = sr.try_get("stream").unwrap_or_else(|_| json!({}));
                let s_ss_method = s_settings.get("method").and_then(Value::as_str);
                let s_secret = if s_proto.is_uuid_auth() {
                    String::new()
                } else {
                    match protocol_credentials::get_protocol_secret(state, client_id, s_proto, s_ss_method).await {
                        Ok(s) => s,
                        Err(_) => continue,
                    }
                };
                let s_ctx = EntryContext {
                    node_host: host.clone(),
                    port: u16::try_from(port).unwrap_or(443),
                    client_uuid: uuid.to_string(),
                    client_id: client_id.to_string(),
                    secret: s_secret,
                };
                let ps_in = PortShareInput {
                    protocol: s_proto,
                    tag: &s_tag,
                    host: s_host.as_deref(),
                    path: s_path.as_deref(),
                    settings: &s_settings,
                    stream: &s_stream,
                };
                if let Ok(Some(entry)) = build_port_share_entry(&settings, &stream, &ps_in, &s_ctx) {
                    entries.push(entry);
                }
            }
        }
    }

    Ok(entries)
}

pub(crate) async fn get_subscription(
    State(state): State<AppState>,
    Path(sub_token): Path<String>,
    Query(q): Query<SubQuery>,
    ip: ClientIp,
    headers: HeaderMap,
) -> Response {
    let raw_flag = q.flag.as_deref().or(q.format.as_deref());
    let ua_opt = user_agent(&headers);
    let ua_str = ua_opt.as_deref().unwrap_or("");

    // HTML subscription landing page when requested directly from a browser without ?flag=
    let accepts_html = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|a| a.contains("text/html"));
    let is_browser = ua_str.contains("Mozilla/5.0") && !ua_str.to_ascii_lowercase().contains("clash");

    let format = match SubFormat::detect(raw_flag, ua_opt.as_deref()) {
        Ok(f) => f,
        Err(_) => return plain_err(StatusCode::BAD_REQUEST, "Invalid subscription format"),
    };
    let (_, profile) = detect_client_from_ua(ua_str);

    let row = match sqlx::query(
        r#"SELECT c.id, c.uuid, c.username, c."trafficLimit", c."usedTraffic", c."uploadTraffic",
                  c."downloadTraffic", c."expireAt", c.banned, c."restrictionReason",
                  c.protocols, c."deviceLimit", c."onHold", c."planId",
                  p."hwidLimit" AS plan_hwid_limit
           FROM "Client" c
           LEFT JOIN "Plan" p ON p.id = c."planId"
           WHERE c."subToken" = $1"#,
    )
    .bind(&sub_token)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(r)) => r,
        Ok(None) => return plain_err(StatusCode::NOT_FOUND, "Client not found"),
        Err(_) => return plain_err(StatusCode::SERVICE_UNAVAILABLE, "Database unavailable"),
    };

    let client_id: String = row.try_get("id").unwrap_or_default();
    let uuid: String = row.try_get("uuid").unwrap_or_default();
    let username: String = row.try_get("username").unwrap_or_default();
    let traffic_limit: i64 = row.try_get("trafficLimit").unwrap_or(0);
    let used_traffic: i64 = row.try_get("usedTraffic").unwrap_or(0);
    let upload_traffic: i64 = row.try_get("uploadTraffic").unwrap_or(0);
    let download_traffic: i64 = row.try_get("downloadTraffic").unwrap_or(0);
    let mut expire_at: Option<NaiveDateTime> = row.try_get("expireAt").ok().flatten();
    let banned: bool = row.try_get("banned").unwrap_or(false);
    let restriction_reason: Option<String> = row.try_get("restrictionReason").ok().flatten();
    let protocols_val: Value = row.try_get("protocols").unwrap_or_else(|_| json!([]));
    let device_limit: Option<i32> = row
        .try_get::<Option<i32>, _>("deviceLimit")
        .ok()
        .flatten()
        .or_else(|| row.try_get::<Option<i32>, _>("plan_hwid_limit").ok().flatten());
    let on_hold: bool = row.try_get::<Option<bool>, _>("onHold").ok().flatten().unwrap_or(false);

    if on_hold {
        if enforcement::activate_on_hold_if_needed(&state, &client_id)
            .await
            .unwrap_or(false)
        {
            if let Ok(Some(new_exp)) = sqlx::query_scalar::<_, Option<NaiveDateTime>>(
                r#"SELECT "expireAt" FROM "Client" WHERE id = $1"#,
            )
            .bind(&client_id)
            .fetch_one(&state.db)
            .await
            {
                expire_at = Some(new_exp);
            }
        }
    }

    let now = crate::db::now();
    if banned {
        return plain_err(StatusCode::FORBIDDEN, "Client is banned");
    }
    if expire_at.is_some_and(|e| e <= now) {
        return plain_err(StatusCode::FORBIDDEN, "Subscription expired");
    }
    if traffic_limit > 0 && used_traffic >= traffic_limit {
        return plain_err(StatusCode::FORBIDDEN, "Traffic limit exceeded");
    }
    if restriction_reason.is_some() {
        return plain_err(StatusCode::FORBIDDEN, "Subscription restricted");
    }

    let hwid_str = headers
        .get("x-hwid")
        .and_then(|v| v.to_str().ok())
        .map(ToString::to_string);
    let hwid_headers = match enforce_hwid_device(
        &state,
        &client_id,
        device_limit,
        &headers,
        &ip,
        ua_opt.as_deref(),
    )
    .await
    {
        Ok(h) => h,
        Err(e) => {
            record_srh(
                &state,
                Some(client_id),
                Some(username),
                ip.0,
                ua_str.to_string(),
                profile,
                format,
                403,
                hwid_str,
            );
            return plain_err(StatusCode::FORBIDDEN, &e.to_string());
        }
    };

    let info = UserInfo {
        username: username.clone(),
        uuid: uuid.clone(),
        upload: u64::try_from(upload_traffic.max(0)).unwrap_or(0),
        download: u64::try_from(download_traffic.max(0)).unwrap_or(0),
        total: u64::try_from(traffic_limit.max(0)).unwrap_or(0),
        expire: expire_at.map_or(0, |d| d.and_utc().timestamp()),
        profile_title: Some(username.clone()),
        update_interval: Some(12),
        support_url: None,
        profile_web_page_url: Some(state.cfg.sub_url.clone()),
        announce: None,
        routing_url: None,
    };

    if raw_flag.is_none() && accepts_html && is_browser {
        let entries = match load_client_entries(&state, &client_id, &uuid, &protocols_val).await {
            Ok(e) => e,
            Err(_) => return plain_err(StatusCode::SERVICE_UNAVAILABLE, "Database unavailable"),
        };
        let html = render_landing_html(
            &username,
            &state.cfg.sub_url,
            &sub_token,
            used_traffic,
            traffic_limit,
            &entries,
            &info,
        );
        return (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            html,
        )
            .into_response();
    }

    let cache_key = format!("sub:{sub_token}:{format:?}:{profile:?}");
    if let Some(cached) = state.cache.get_sub(&cache_key) {
        let body = if format == SubFormat::Json {
            if let Ok(mut parsed) = serde_json::from_str::<Value>(&cached.body) {
                if let Some(obj) = parsed.as_object_mut() {
                    obj.insert(
                        "client".into(),
                        json!({
                            "username": info.username,
                            "uuid": info.uuid,
                            "upload": info.upload,
                            "download": info.download,
                            "total": info.total,
                            "expire": info.expire
                        }),
                    );
                }
                serde_json::to_string_pretty(&parsed).unwrap_or(cached.body)
            } else {
                cached.body
            }
        } else {
            cached.body
        };

        // Preserve cached content_type + headers AND apply fresh Userinfo/HWID headers
        let mut res = (StatusCode::OK, body).into_response();
        if let Ok(ct) = HeaderValue::from_str(&cached.content_type) {
            res.headers_mut().insert(header::CONTENT_TYPE, ct);
        }
        for (k, v) in &cached.headers {
            if let (Ok(name), Ok(val)) = (
                HeaderName::from_bytes(k.as_bytes()),
                HeaderValue::from_str(v),
            ) {
                res.headers_mut().insert(name, val);
            }
        }
        record_srh(
            &state,
            Some(client_id),
            Some(username),
            ip.0,
            ua_str.to_string(),
            profile,
            format,
            200,
            hwid_str,
        );
        return apply_sub_headers(res, &info, format, profile, hwid_headers);
    }

    let entries = match load_client_entries(&state, &client_id, &uuid, &protocols_val).await {
        Ok(e) => e,
        Err(_) => return plain_err(StatusCode::SERVICE_UNAVAILABLE, "Database unavailable"),
    };

    if entries.is_empty() {
        record_srh(
            &state,
            Some(client_id),
            Some(username),
            ip.0,
            ua_str.to_string(),
            profile,
            format,
            503,
            hwid_str,
        );
        let res = (StatusCode::SERVICE_UNAVAILABLE, "No online nodes available").into_response();
        return apply_sub_headers(res, &info, format, profile, hwid_headers);
    }

    let srr_rules = load_srr_rules(&state).await;
    let mut hdr_map = BTreeMap::new();
    for (k, v) in &headers {
        if let Ok(vs) = v.to_str() {
            hdr_map.insert(k.as_str().to_ascii_lowercase(), vs.to_string());
        }
    }
    let srr_ctx = SrrContext {
        user_agent: ua_str.to_string(),
        headers: hdr_map,
        client_ip: Some(ip.0.clone()),
        hwid: hwid_str.clone(),
    };

    let decision = subscription::evaluate_srr(&srr_ctx, &srr_rules);
    let effective_fmt = decision.override_format.unwrap_or(format);
    let tpl_opt: Option<SubscriptionTemplate> = if let Some(ref tid) = decision.template_id.as_deref().filter(|s| !s.trim().is_empty()) {
        load_subscription_template_by_id(&state, tid).await
    } else {
        load_default_subscription_template(&state, effective_fmt).await
    };

    let rendered = match SubscriptionRenderer::render(
        &entries,
        &info,
        format,
        tpl_opt.as_ref(),
        Some((&srr_ctx, &srr_rules)),
    ) {
        Ok(r) => r,
        Err(e) => {
            return plain_err(StatusCode::UNPROCESSABLE_ENTITY, &e.to_string());
        }
    };

    if let Some(early) = rendered.early_status {
        let code = StatusCode::from_u16(early).unwrap_or(StatusCode::FORBIDDEN);
        return (code, "").into_response();
    }

    state.cache.set_sub(
        cache_key,
        CachedSubscription {
            body: rendered.body.clone(),
            content_type: rendered.content_type.to_string(),
            headers: rendered.headers.clone(),
        },
        Duration::from_secs(300),
    );

    let db = state.db.clone();
    let cid = client_id.clone();
    tokio::spawn(async move {
        let _ = sqlx::query(r#"UPDATE "Client" SET "lastActiveAt" = $2 WHERE id = $1"#)
            .bind(cid)
            .bind(crate::db::now())
            .execute(&db)
            .await;
    });

    record_srh(
        &state,
        Some(client_id),
        Some(username),
        ip.0,
        ua_str.to_string(),
        profile,
        rendered.format,
        200,
        hwid_str,
    );

    let mut res = (StatusCode::OK, rendered.body).into_response();
    if let Ok(ct) = HeaderValue::from_str(rendered.content_type) {
        res.headers_mut().insert(header::CONTENT_TYPE, ct);
    }
    for (k, v) in &rendered.headers {
        if let (Ok(name), Ok(val)) = (
            HeaderName::from_bytes(k.as_bytes()),
            HeaderValue::from_str(v),
        ) {
            res.headers_mut().insert(name, val);
        }
    }
    if let Some((limit, count)) = hwid_headers {
        if let Ok(v) = HeaderValue::from_str(&limit.to_string()) {
            res.headers_mut().insert("x-hwid-limit", v);
        }
        if let Ok(v) = HeaderValue::from_str(&count.to_string()) {
            res.headers_mut().insert("x-hwid-count", v);
        }
    }
    res
}

async fn get_info(
    State(state): State<AppState>,
    Path(sub_token): Path<String>,
) -> AppResult<Json<Value>> {
    valid_id(&sub_token)?;
    let r = sqlx::query(
        r#"SELECT username, uuid, "trafficLimit", "usedTraffic", "uploadTraffic", "downloadTraffic",
                  "expireAt", banned, "lastActiveAt", "createdAt", protocols, "subToken"
           FROM "Client" WHERE "subToken" = $1"#,
    )
    .bind(&sub_token)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("Client not found"))?;

    let limit: i64 = r.try_get("trafficLimit")?;
    let used: i64 = r.try_get("usedTraffic")?;
    let pct = if limit > 0 {
        ((used as f64 / limit as f64) * 10000.0).round() / 100.0
    } else {
        0.0
    };
    let online_nodes: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "Node" WHERE status = 'ONLINE' AND active = true"#,
    )
    .fetch_one(&state.db)
    .await?;

    let expire_at: Option<NaiveDateTime> = r.try_get("expireAt")?;
    let last_active: Option<NaiveDateTime> = r.try_get("lastActiveAt")?;
    let created_at: NaiveDateTime = r.try_get("createdAt")?;

    Ok(Json(json!({
        "username": r.try_get::<String, _>("username")?,
        "uuid": r.try_get::<String, _>("uuid")?,
        "trafficLimit": limit,
        "usedTraffic": used,
        "uploadTraffic": r.try_get::<i64, _>("uploadTraffic")?,
        "downloadTraffic": r.try_get::<i64, _>("downloadTraffic")?,
        "expireAt": expire_at.map(|d| crate::db::ts::format_iso(&d)),
        "banned": r.try_get::<bool, _>("banned")?,
        "lastActiveAt": last_active.map(|d| crate::db::ts::format_iso(&d)),
        "createdAt": crate::db::ts::format_iso(&created_at),
        "protocols": r.try_get::<Value, _>("protocols")?,
        "subToken": r.try_get::<String, _>("subToken")?,
        "subscriptionUrl": format!("{}/api/v1/client/{}/sub", state.cfg.sub_url, sub_token),
        "onlineNodes": online_nodes,
        "trafficPercent": pct
    })))
}

/// Requires authenticated user (admin or owning reseller) to rotate `subToken`.
async fn regenerate(
    State(state): State<AppState>,
    user: AuthUser,
    Path(sub_token): Path<String>,
) -> AppResult<Json<Value>> {
    user.require_reseller()?;
    valid_id(&sub_token)?;

    let client_reseller: Option<Option<String>> = sqlx::query_scalar(
        r#"SELECT "resellerId" FROM "Client" WHERE "subToken" = $1"#,
    )
    .bind(&sub_token)
    .fetch_optional(&state.db)
    .await?;

    let Some(rid_opt) = client_reseller else {
        return Err(AppError::not_found("Client not found"));
    };
    if let Some(tenant_rid) = user.tenant_scope()? {
        if rid_opt.as_deref() != Some(tenant_rid.as_str()) {
            return Err(AppError::not_found("Client not found"));
        }
    }

    let new_token = crate::db::new_id();
    sqlx::query(r#"UPDATE "Client" SET "subToken" = $2, "updatedAt" = $3 WHERE "subToken" = $1"#)
        .bind(&sub_token)
        .bind(&new_token)
        .bind(crate::db::now())
        .execute(&state.db)
        .await?;

    state.cache.del_sub_token(&sub_token);
    Ok(Json(json!({ "subToken": new_token })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HappQuery {
    rsa_pub_pem: Option<String>,
    version: Option<String>,
}

async fn get_happ_encrypted(
    State(state): State<AppState>,
    Path(sub_token): Path<String>,
    Query(q): Query<HappQuery>,
) -> AppResult<Json<Value>> {
    valid_id(&sub_token)?;
    let sub_url = format!("{}/api/v1/client/{}/sub?flag=happ", state.cfg.sub_url, sub_token);
    let Some(pem) = q.rsa_pub_pem.as_deref().filter(|s| !s.is_empty()) else {
        return Ok(Json(json!({ "url": sub_url })));
    };
    let ver = q.version.as_deref().unwrap_or("5");
    let encrypted = if ver == "4" {
        build_happ_crypt4(&sub_url, pem)
    } else {
        build_happ_crypt5(&sub_url, pem)
    }
    .map_err(|e| AppError::bad_request(e.to_string()))?;

    Ok(Json(json!({ "url": encrypted })))
}
