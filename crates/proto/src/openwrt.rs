use crate::links::{encode_uri_component, form_urlencode, js_hash_code as links_hash_code, SubError};
use crate::wire::Protocol;
use base64::{engine::general_purpose, Engine as _};
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

const STANDARD: general_purpose::GeneralPurpose = general_purpose::STANDARD;

/// Computes the exact 32-bit UTF-16 JS `String.prototype.hashCode` unsigned magnitude.
pub fn js_hash_code(s: &str) -> u32 {
    links_hash_code(s)
}

/// Alias matching `01-data-and-services.md` §8d naming (`hashCode`).
pub fn hash_code(s: &str) -> u32 {
    links_hash_code(s)
}

/// Input descriptor for OpenWrt / router config builders (`01-data-and-services.md` §8d).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InboundExport {
    pub protocol: Protocol,
    pub tag: String,
    pub host: String,
    pub port: u16,
    #[serde(default = "default_obj")]
    pub settings: Value,
    #[serde(default = "default_obj")]
    pub stream: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_ip: Option<String>,
}

fn default_obj() -> Value {
    json!({})
}

/// UCI command operation type.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum UciCmdType {
    Set,
    Add,
    Delete,
    Commit,
}

impl UciCmdType {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Set => "set",
            Self::Add => "add",
            Self::Delete => "delete",
            Self::Commit => "commit",
        }
    }
}

/// Single OpenWrt UCI command emitted by [`generate_openclash_uci`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UciCommand {
    #[serde(rename = "type")]
    pub cmd_type: UciCmdType,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

/// Helper implementing JS `||` semantics (treats empty strings / null / non-strings as absent).
fn js_str<'a>(val: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut cur = val;
    for key in path {
        cur = cur.get(*key)?;
    }
    cur.as_str().filter(|s| !s.is_empty())
}

fn js_or<'a>(val: &'a Value, path: &[&str], fallback: &'a str) -> &'a str {
    js_str(val, path).unwrap_or(fallback)
}

fn is_tls(stream: &Value) -> bool {
    matches!(js_str(stream, &["security"]), Some("tls" | "reality"))
}

/// Generates OpenClash UCI commands for an [`InboundExport`].
pub fn generate_openclash_uci(inbound: &InboundExport) -> Vec<UciCommand> {
    let section = format!("proxy_{}", hash_code(&inbound.tag));
    let prefix = format!("openclash.{section}");
    let mut cmds = vec![
        UciCommand {
            cmd_type: UciCmdType::Delete,
            path: prefix.clone(),
            value: None,
        },
        UciCommand {
            cmd_type: UciCmdType::Add,
            path: "openclash".into(),
            value: Some("proxy".into()),
        },
    ];

    let mut set_opt = |opt: &str, val: &str| {
        cmds.push(UciCommand {
            cmd_type: UciCmdType::Set,
            path: format!("{prefix}.{opt}"),
            value: Some(val.to_string()),
        });
    };

    let s = &inbound.settings;
    let st = &inbound.stream;

    match inbound.protocol {
        Protocol::Vless => {
            let network = js_or(st, &["network"], "tcp");
            let sec = js_str(st, &["security"]);
            set_opt("type", "vless");
            set_opt("name", &inbound.tag);
            set_opt("server", &inbound.host);
            set_opt("port", &inbound.port.to_string());
            set_opt("uuid", js_or(s, &["id"], ""));
            set_opt("network", network);
            set_opt("tls", if is_tls(st) { "1" } else { "0" });

            if let Some(sni) = js_str(st, &["sni"]) {
                set_opt("servername", sni);
            }
            if let Some(fp) = js_str(st, &["fingerprint"]) {
                set_opt("client-fingerprint", fp);
            }
            if let Some(flow) = js_str(s, &["flow"]) {
                set_opt("flow", flow);
            }
            if sec == Some("reality") {
                set_opt("reality-public-key", js_or(st, &["publicKey"], ""));
                set_opt("reality-short-id", js_or(st, &["shortId"], ""));
            }
            if network == "ws" {
                set_opt("ws-opts.path", js_or(st, &["wsSettings", "path"], "/"));
                if let Some(ws_host) = js_str(st, &["wsSettings", "host"]) {
                    set_opt("ws-opts.headers.Host", ws_host);
                }
            } else if network == "grpc" {
                set_opt(
                    "grpc-opts.grpc-service-name",
                    js_or(st, &["grpcSettings", "serviceName"], ""),
                );
            }
        }
        Protocol::Hysteria2 => {
            set_opt("type", "hysteria2");
            set_opt("name", &inbound.tag);
            set_opt("server", &inbound.host);
            set_opt("port", &inbound.port.to_string());
            set_opt("password", js_or(s, &["password"], ""));
            if let Some(sni) = js_str(s, &["sni"]) {
                set_opt("sni", sni);
            }
            if let Some(obfs_type) = js_str(s, &["obfs", "type"]) {
                if obfs_type != "none" {
                    set_opt("obfs", obfs_type);
                    set_opt("obfs-password", js_or(s, &["obfs", "password"], ""));
                }
            }
        }
        Protocol::Trojan => {
            set_opt("type", "trojan");
            set_opt("name", &inbound.tag);
            set_opt("server", &inbound.host);
            set_opt("port", &inbound.port.to_string());
            set_opt("password", js_or(s, &["password"], ""));
            set_opt("tls", "1");
            if let Some(sni) = js_str(s, &["sni"]) {
                set_opt("servername", sni);
            }
            if let Some(fp) = js_str(st, &["fingerprint"]) {
                set_opt("client-fingerprint", fp);
            }
        }
        _ => {}
    }

    cmds.push(UciCommand {
        cmd_type: UciCmdType::Commit,
        path: "openclash".into(),
        value: None,
    });
    cmds
}

/// Generates a single OpenWrt Passwall JSON object preserving exact key insertion order.
pub fn generate_passwall_config(inbound: &InboundExport) -> Value {
    let mut obj = Map::new();
    obj.insert("name".into(), json!(inbound.tag));
    obj.insert(
        "type".into(),
        json!(inbound.protocol.as_str().to_ascii_lowercase()),
    );
    obj.insert("server".into(), json!(inbound.host));
    obj.insert("port".into(), json!(inbound.port));

    let s = &inbound.settings;
    let st = &inbound.stream;

    match inbound.protocol {
        Protocol::Vless => {
            let network = js_or(st, &["network"], "tcp");
            obj.insert("uuid".into(), json!(js_or(s, &["id"], "")));
            obj.insert("flow".into(), json!(js_or(s, &["flow"], "")));
            obj.insert("tls".into(), json!(if is_tls(st) { "1" } else { "0" }));
            obj.insert("tls_host".into(), json!(js_or(st, &["sni"], "")));
            obj.insert(
                "fingerprint".into(),
                json!(js_or(st, &["fingerprint"], "chrome")),
            );
            obj.insert(
                "reality_public_key".into(),
                json!(js_or(st, &["publicKey"], "")),
            );
            obj.insert("reality_short_id".into(), json!(js_or(st, &["shortId"], "")));
            obj.insert("network".into(), json!(network));
            if network == "ws" {
                obj.insert(
                    "network_path".into(),
                    json!(js_or(st, &["wsSettings", "path"], "/")),
                );
                obj.insert(
                    "network_host".into(),
                    json!(js_or(st, &["wsSettings", "host"], "")),
                );
            } else if network == "grpc" {
                obj.insert(
                    "network_serviceName".into(),
                    json!(js_or(st, &["grpcSettings", "serviceName"], "")),
                );
            }
        }
        Protocol::Hysteria2 => {
            obj.insert("password".into(), json!(js_or(s, &["password"], "")));
            obj.insert("sni".into(), json!(js_or(s, &["sni"], "")));
            obj.insert("obfs".into(), json!(js_or(s, &["obfs", "type"], "none")));
            obj.insert(
                "obfs_password".into(),
                json!(js_or(s, &["obfs", "password"], "")),
            );
        }
        Protocol::Trojan => {
            obj.insert("password".into(), json!(js_or(s, &["password"], "")));
            obj.insert("tls".into(), json!("1"));
            obj.insert("tls_host".into(), json!(js_or(st, &["sni"], "")));
            obj.insert(
                "fingerprint".into(),
                json!(js_or(st, &["fingerprint"], "chrome")),
            );
        }
        Protocol::Shadowsocks => {
            obj.insert("cipher".into(), json!(js_or(s, &["method"], "aes-256-gcm")));
            obj.insert("password".into(), json!(js_or(s, &["password"], "")));
        }
        _ => {}
    }

    Value::Object(obj)
}

/// Generates a 2-space pretty JSON array of Passwall node configurations.
pub fn generate_passwall_subscription(inbounds: &[InboundExport]) -> Result<String, SubError> {
    let arr: Vec<Value> = inbounds.iter().map(generate_passwall_config).collect();
    serde_json::to_string_pretty(&arr)
        .map_err(|e| SubError::Invalid(format!("Passwall JSON serialization failed: {e}")))
}

fn sanitize_yaml_str(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace(['\r', '\n'], " ")
}

fn to_openclash_proxy(inbound: &InboundExport) -> Option<Value> {
    let s = &inbound.settings;
    let st = &inbound.stream;
    let mut obj = Map::new();

    match inbound.protocol {
        Protocol::Vless => {
            let network = js_or(st, &["network"], "tcp");
            let sec = js_str(st, &["security"]);
            obj.insert("name".into(), json!(inbound.tag));
            obj.insert("type".into(), json!("vless"));
            obj.insert("server".into(), json!(inbound.host));
            obj.insert("port".into(), json!(inbound.port));
            obj.insert("uuid".into(), json!(js_or(s, &["id"], "")));
            obj.insert("flow".into(), json!(js_or(s, &["flow"], "")));
            obj.insert("tls".into(), json!(is_tls(st)));
            obj.insert("servername".into(), json!(js_or(st, &["sni"], "")));
            obj.insert(
                "client-fingerprint".into(),
                json!(js_or(st, &["fingerprint"], "chrome")),
            );
            if sec == Some("reality") {
                obj.insert(
                    "reality-public-key".into(),
                    json!(js_or(st, &["publicKey"], "")),
                );
                obj.insert("reality-short-id".into(), json!(js_or(st, &["shortId"], "")));
            }
            obj.insert("network".into(), json!(network));
            if network == "ws" {
                let mut ws_opts = Map::new();
                ws_opts.insert("path".into(), json!(js_or(st, &["wsSettings", "path"], "/")));
                let mut headers = Map::new();
                if let Some(host) = js_str(st, &["wsSettings", "host"]) {
                    headers.insert("Host".into(), json!(host));
                }
                ws_opts.insert("headers".into(), Value::Object(headers));
                obj.insert("ws-opts".into(), Value::Object(ws_opts));
            } else if network == "grpc" {
                let mut grpc_opts = Map::new();
                grpc_opts.insert(
                    "grpc-service-name".into(),
                    json!(js_or(st, &["grpcSettings", "serviceName"], "")),
                );
                obj.insert("grpc-opts".into(), Value::Object(grpc_opts));
            }
            Some(Value::Object(obj))
        }
        Protocol::Hysteria2 => {
            obj.insert("name".into(), json!(inbound.tag));
            obj.insert("type".into(), json!("hysteria2"));
            obj.insert("server".into(), json!(inbound.host));
            obj.insert("port".into(), json!(inbound.port));
            obj.insert("password".into(), json!(js_or(s, &["password"], "")));
            obj.insert("sni".into(), json!(js_or(s, &["sni"], "")));
            if let Some(obfs_type) = js_str(s, &["obfs", "type"]) {
                if obfs_type != "none" {
                    obj.insert("obfs".into(), json!(obfs_type));
                    obj.insert(
                        "obfs-password".into(),
                        json!(js_or(s, &["obfs", "password"], "")),
                    );
                }
            }
            Some(Value::Object(obj))
        }
        Protocol::Trojan => {
            obj.insert("name".into(), json!(inbound.tag));
            obj.insert("type".into(), json!("trojan"));
            obj.insert("server".into(), json!(inbound.host));
            obj.insert("port".into(), json!(inbound.port));
            obj.insert("password".into(), json!(js_or(s, &["password"], "")));
            obj.insert("sni".into(), json!(js_or(st, &["sni"], "")));
            obj.insert(
                "client-fingerprint".into(),
                json!(js_or(st, &["fingerprint"], "chrome")),
            );
            Some(Value::Object(obj))
        }
        _ => None,
    }
}

fn object_to_yaml(val: &Value, indent: usize) -> String {
    let pad = " ".repeat(indent);
    let mut yaml = String::new();
    if let Some(map) = val.as_object() {
        for (key, value) in map {
            match value {
                Value::Bool(b) => {
                    yaml.push_str(&format!("{pad}{key}: {b}\n"));
                }
                Value::Number(n) => {
                    yaml.push_str(&format!("{pad}{key}: {n}\n"));
                }
                Value::Object(_) => {
                    yaml.push_str(&format!("{pad}{key}:\n"));
                    yaml.push_str(&object_to_yaml(value, indent + 2));
                    yaml.push('\n');
                }
                Value::Array(arr) => {
                    yaml.push_str(&format!("{pad}{key}:\n"));
                    for item in arr {
                        match item {
                            Value::String(s) => {
                                yaml.push_str(&format!(
                                    "{pad}  - \"{}\"\n",
                                    sanitize_yaml_str(s)
                                ));
                            }
                            Value::Bool(b) => yaml.push_str(&format!("{pad}  - {b}\n")),
                            Value::Number(n) => yaml.push_str(&format!("{pad}  - {n}\n")),
                            Value::Null => yaml.push_str(&format!("{pad}  - null\n")),
                            _ => {
                                yaml.push_str(&format!(
                                    "{pad}  - \"{}\"\n",
                                    sanitize_yaml_str(&item.to_string())
                                ));
                            }
                        }
                    }
                }
                Value::String(s) => {
                    yaml.push_str(&format!("{pad}{key}: \"{}\"\n", sanitize_yaml_str(s)));
                }
                Value::Null => {
                    yaml.push_str(&format!("{pad}{key}: \"null\"\n"));
                }
            }
        }
    }
    yaml
}

/// Generates an OpenClash YAML profile using an explicit ISO-8601 timestamp (for deterministic tests).
pub fn generate_openclash_yaml_with_ts(
    inbounds: &[InboundExport],
    generated_iso: &str,
) -> Result<String, SubError> {
    let mut proxies_yaml = String::new();
    for inbound in inbounds {
        if let Some(node) = to_openclash_proxy(inbound) {
            let block = object_to_yaml(&node, 4);
            let stripped = block.strip_prefix("    ").unwrap_or(&block);
            proxies_yaml.push_str("  - ");
            proxies_yaml.push_str(stripped);
            proxies_yaml.push('\n');
        }
    }

    let group_lines = if inbounds.is_empty() {
        "      - DIRECT".to_string()
    } else {
        inbounds
            .iter()
            .map(|i| format!("      - {}", i.tag.replace(['\r', '\n'], " ")))
            .collect::<Vec<_>>()
            .join("\n")
    };

    Ok(format!(
        "# ProxPanel \u{2192} OpenClash Config\n\
         # Generated: {generated_iso}\n\
         # Compatible with: OpenClash / Clash Meta (mihomo)\n\
         \n\
         mixed-port: 7890\n\
         allow-lan: true\n\
         bind-address: '*'\n\
         mode: rule\n\
         log-level: info\n\
         \n\
         dns:\n  \
         enable: true\n  \
         listen: 0.0.0.0:53\n  \
         enhanced-mode: fake-ip\n  \
         fake-ip-range: 198.18.0.1/16\n  \
         nameserver:\n    \
         - https://dns.google/dns-query\n    \
         - https://1.1.1.1/dns-query\n  \
         fallback:\n    \
         - tls://8.8.8.8\n    \
         - https://dns.google/dns-query\n  \
         fallback-filter:\n    \
         geoip: true\n    \
         geoip-code: CN\n\
         \n\
         proxies:\n\
         {proxies_yaml}\n\
         proxy-groups:\n  \
         - name: PROXY\n    \
         type: select\n    \
         proxies:\n\
         {group_lines}\n\
         \n  \
         - name: AUTO\n    \
         type: url-test\n    \
         proxies:\n\
         {group_lines}\n    \
         url: https://www.gstatic.com/generate_204\n    \
         interval: 300\n\
         \n\
         rules:\n  \
         - GEOIP,CN,DIRECT\n  \
         - GEOSITE,category-ads-all,REJECT\n  \
         - GEOSITE,category-malware,REJECT\n  \
         - GEOSITE,telegram,PROXY\n  \
         - GEOSITE,google,PROXY\n  \
         - MATCH,PROXY\n"
    ))
}

/// Generates an OpenClash YAML profile stamped with the current UTC timestamp.
pub fn generate_openclash_yaml(inbounds: &[InboundExport]) -> Result<String, SubError> {
    let now_iso = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    generate_openclash_yaml_with_ts(inbounds, &now_iso)
}

fn to_singbox_outbound(inbound: &InboundExport) -> Option<Value> {
    let s = &inbound.settings;
    let ctx = crate::links::EntryContext {
        node_host: inbound.host.clone(),
        port: inbound.port,
        client_uuid: js_or(s, &["id"], "").to_string(),
        client_id: js_or(s, &["username"], "").to_string(),
        secret: js_or(s, &["password"], "").to_string(),
    };
    let mut entry = crate::links::build_entry(
        inbound.protocol,
        &inbound.tag,
        &inbound.settings,
        &inbound.stream,
        &ctx,
    )
    .ok()??;
    entry.resolved_ip = inbound.resolved_ip.clone();
    crate::subscription::singbox_outbound(&entry, &inbound.tag)
        .ok()
        .flatten()
}

/// Generates an OpenWrt Sing-box JSON `{"outbounds": [...]}` subscription.
pub fn generate_singbox_subscription(inbounds: &[InboundExport]) -> Result<String, SubError> {
    let outbounds: Vec<Value> = inbounds.iter().filter_map(to_singbox_outbound).collect();
    serde_json::to_string_pretty(&json!({ "outbounds": outbounds }))
        .map_err(|e| SubError::Invalid(format!("Sing-box JSON serialization failed: {e}")))
}

fn to_openwrt_uri(inbound: &InboundExport) -> Option<String> {
    let s = &inbound.settings;
    let st = &inbound.stream;

    match inbound.protocol {
        Protocol::Vless => {
            let network = js_or(st, &["network"], "tcp");
            let sec = js_or(st, &["security"], "none");
            let mut params: Vec<(&'static str, String)> = vec![
                ("security", sec.to_string()),
                ("type", network.to_string()),
            ];
            if let Some(sni) = js_str(st, &["sni"]) {
                params.push(("sni", sni.to_string()));
            }
            if let Some(fp) = js_str(st, &["fingerprint"]) {
                params.push(("fp", fp.to_string()));
            }
            if let Some(pbk) = js_str(st, &["publicKey"]) {
                params.push(("pbk", pbk.to_string()));
            }
            if let Some(sid) = js_str(st, &["shortId"]) {
                params.push(("sid", sid.to_string()));
            }
            if let Some(spx) = js_str(st, &["spiderX"]) {
                params.push(("spx", spx.to_string()));
            }
            if let Some(flow) = js_str(s, &["flow"]) {
                params.push(("flow", flow.to_string()));
            }
            if network == "ws" {
                if let Some(path) = js_str(st, &["wsSettings", "path"]) {
                    params.push(("path", path.to_string()));
                }
            } else if network == "grpc" {
                if let Some(svc) = js_str(st, &["grpcSettings", "serviceName"]) {
                    params.push(("serviceName", svc.to_string()));
                }
            }
            Some(format!(
                "vless://{}@{}:{}?{}#{}",
                js_or(s, &["id"], ""),
                inbound.host,
                inbound.port,
                form_urlencode(&params),
                encode_uri_component(&inbound.tag)
            ))
        }
        Protocol::Hysteria2 => {
            let mut params: Vec<(&'static str, String)> = Vec::new();
            if let Some(sni) = js_str(s, &["sni"]) {
                params.push(("sni", sni.to_string()));
            }
            if let Some(obfs_type) = js_str(s, &["obfs", "type"]) {
                if obfs_type != "none" {
                    params.push(("obfs", obfs_type.to_string()));
                    if let Some(obfs_pw) = js_str(s, &["obfs", "password"]) {
                        params.push(("obfs-password", obfs_pw.to_string()));
                    }
                }
            }
            Some(format!(
                "hy2://{}@{}:{}?{}#{}",
                js_or(s, &["password"], ""),
                inbound.host,
                inbound.port,
                form_urlencode(&params),
                encode_uri_component(&inbound.tag)
            ))
        }
        Protocol::Trojan => {
            let mut params: Vec<(&'static str, String)> = Vec::new();
            if let Some(sni) = js_str(st, &["sni"]) {
                params.push(("sni", sni.to_string()));
            }
            if let Some(fp) = js_str(st, &["fingerprint"]) {
                params.push(("fp", fp.to_string()));
            }
            Some(format!(
                "trojan://{}@{}:{}?{}#{}",
                js_or(s, &["password"], ""),
                inbound.host,
                inbound.port,
                form_urlencode(&params),
                encode_uri_component(&inbound.tag)
            ))
        }
        Protocol::Shadowsocks => {
            let method = js_or(s, &["method"], "aes-256-gcm");
            let pw = js_or(s, &["password"], "");
            let auth = STANDARD.encode(format!("{method}:{pw}").as_bytes());
            Some(format!(
                "ss://{auth}@{}:{}#{}",
                inbound.host,
                inbound.port,
                encode_uri_component(&inbound.tag)
            ))
        }
        _ => None,
    }
}

/// Generates a standard-base64 subscription of URI lines matching `01-data-and-services.md` §8d.
pub fn generate_base64_subscription(inbounds: &[InboundExport]) -> Result<String, SubError> {
    let lines: Vec<String> = inbounds.iter().filter_map(to_openwrt_uri).collect();
    Ok(STANDARD.encode(lines.join("\n").as_bytes()))
}

/// Generates a double-base64 SSR subscription for Shadowsocks inbounds matching `01-data-and-services.md` §8d.
pub fn generate_ssr_subscription(inbounds: &[InboundExport]) -> Result<String, SubError> {
    let lines: Vec<String> = inbounds
        .iter()
        .filter(|i| i.protocol == Protocol::Shadowsocks)
        .map(|i| {
            let method = js_or(&i.settings, &["method"], "aes-256-gcm");
            let pw = js_or(&i.settings, &["password"], "");
            let proto = js_or(&i.settings, &["proto"], "origin");
            let raw = format!("{}:{}:{method}:{pw}:{proto}", i.host, i.port);
            format!("ssr://{}", STANDARD.encode(raw.as_bytes()))
        })
        .collect();
    Ok(STANDARD.encode(lines.join("\n").as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_singbox_subscription_supports_vless_xhttp_reality() {
        let inbound = InboundExport {
            protocol: Protocol::Vless,
            tag: "Finland Rostislav".into(),
            host: "finland.example.com".into(),
            port: 443,
            settings: json!({
                "id": "682e693a-33b4-4563-9ca4-f11f54fae2da",
                "flow": "none"
            }),
            stream: json!({
                "network": "xhttp",
                "security": "reality",
                "sni": "finland.example.com",
                "fingerprint": "firefox",
                "realitySettings": {
                    "dest": "www.twitch.tv:443",
                    "serverNames": ["www.twitch.tv"],
                    "publicKey": "YSia7kbg9eXUatxAQ5e2nbmKkFNgormAPs27SsWeJXg",
                    "shortIds": [""]
                },
                "xhttpSettings": {
                    "path": "/",
                    "mode": "auto",
                    "host": "finland.example.com"
                }
            }),
            resolved_ip: Some("178.104.166.22".into()),
        };

        let rendered = generate_singbox_subscription(&[inbound]).unwrap();
        let parsed: Value = serde_json::from_str(&rendered).unwrap();
        let ob = &parsed["outbounds"][0];
        assert_eq!(ob["server"], json!("178.104.166.22"));
        assert_eq!(ob["tls"]["server_name"], json!("www.twitch.tv"));
        assert_eq!(ob["tls"]["alpn"], json!(["h2", "http/1.1"]));
        assert_eq!(ob["tls"]["utls"]["fingerprint"], json!("edge"));
        assert_eq!(ob["transport"]["type"], json!("xhttp"));
        assert_eq!(ob["transport"]["mode"], json!("packet-up"));
        assert_eq!(ob["transport"]["x_padding_bytes"], json!("100-1000"));
        assert_eq!(ob["transport"]["host"], json!("www.twitch.tv"));
        assert!(ob.get("flow").is_none());
        assert!(ob.get("packet_encoding").is_none());
    }
}
