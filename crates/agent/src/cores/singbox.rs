use crate::config::DEFAULT_SINGBOX_API_ADDR;
use crate::cores::{extract_protocol_users, str_or_empty, validate_inbound};
use mypanel_proto::links::parse_port_range;
use mypanel_proto::{InboundConfig, Protocol, RoutingRuleConfig};
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;

/// Returns `true` if `protocol` is natively served by the Sing-box process.
pub fn is_singbox_protocol(protocol: Protocol) -> bool {
    matches!(
        protocol,
        Protocol::Hysteria2 | Protocol::Tuic | Protocol::Anytls | Protocol::Shadowtls
    )
}

/// Generates a Sing-box v1.14+ configuration without global routing rules (`DEFAULT_SINGBOX_API_ADDR`).
pub fn generate_config(inbounds: &[InboundConfig]) -> Result<Value, String> {
    generate_config_with_routing(inbounds, &[], DEFAULT_SINGBOX_API_ADDR)
}

/// Generates a complete Sing-box v1.14+ JSON configuration (`<CONFIG_DIR>/singbox.json`).
///
/// Implements:
/// - **``**: Numeric `port` (`Vec<u16>`) / `port_range` (`Vec<String>` `"start:end"`),
/// `domain:`/`full:`/`keyword:`/`regexp:` prefix mapping, `geosite:`/`geoip:` stripping into `.srs` `rule_set`,
/// `{"action": "sniff"}` prepended in `route.rules`, modern DNS without `"detour": "direct"` on local server,
/// and `"default_domain_resolver": "dns-direct"`.
/// - **Sing-box 1.14+**: `.srs` remote binary `route.rule_set`, DNS `1.14` (`"action": "evaluate"`,
/// `"match_response": true`), `AnyTLS` (`type: "anytls"`, `users`, `padding_scheme`),
/// Hysteria2 port hopping (`listen_ports` + multi-listener expansion `<= 64` ports),
/// `obfs` (`salamander` / `gecko`), `bbr_profile`, `tls.ech`, `disable_chrome_parrot`,
/// and ShadowTLS v3 (`type: "shadowtls"`, `version: 3`).
pub fn generate_config_with_routing(
    inbounds: &[InboundConfig],
    global_rules: &[RoutingRuleConfig],
    clash_api_addr: &str,
) -> Result<Value, String> {
    let mut built_inbounds = Vec::new();
    let mut rule_sets: BTreeSet<String> = BTreeSet::new();
    let mut route_rules = vec![
        json!({ "action": "sniff" }),
        json!({ "ip_is_private": true, "action": "reject" }),
    ];

    for inbound in inbounds {
        if !inbound.enable || !is_singbox_protocol(inbound.protocol) {
            continue;
        }
        validate_inbound(inbound)?;
        let expanded = build_singbox_inbounds(inbound)?;
        built_inbounds.extend(expanded);
        append_inbound_routing(&mut route_rules, &mut rule_sets, &inbound.routing);
    }

    append_route_rules(&mut route_rules, &mut rule_sets, global_rules);

    let rule_set_entries: Vec<Value> = rule_sets
        .iter()
        .map(|tag| build_srs_rule_set_entry(tag))
        .collect();

    let api_addr = if clash_api_addr.trim().is_empty() {
        DEFAULT_SINGBOX_API_ADDR
    } else {
        clash_api_addr.trim()
    };

    Ok(json!({
        "log": {
            "level": "warn",
            "timestamp": true
        },
        "dns": {
            "servers": [
                {
                    "type": "tls",
                    "tag": "google",
                    "server": "8.8.8.8"
                },
                {
                    "type": "local",
                    "tag": "dns-direct"
                },
                {
                    "type": "local",
                    "tag": "local"
                }
            ],
            "rules": [
                {
                    "domain_suffix": [".ir", ".local"],
                    "action": "route",
                    "server": "dns-direct"
                },
                {
                    "action": "evaluate",
                    "match_response": true,
                    "server": "google"
                }
            ],
            "final": "google"
        },
        "inbounds": built_inbounds,
        "outbounds": [
            { "type": "direct", "tag": "direct" }
        ],
        "route": {
            "rules": route_rules,
            "rule_set": rule_set_entries,
            "auto_detect_interface": true,
            "default_domain_resolver": "dns-direct"
        },
        "experimental": {
            "clash_api": {
                "external_controller": api_addr
            }
        }
    }))
}

fn build_srs_rule_set_entry(tag: &str) -> Value {
    let url = if let Some(name) = tag.strip_prefix("geosite-") {
        format!(
            "https://raw.githubusercontent.com/SagerNet/sing-geosite/rule-set/geosite-{name}.srs"
        )
    } else if let Some(name) = tag.strip_prefix("geoip-") {
        format!(
            "https://raw.githubusercontent.com/SagerNet/sing-geoip/rule-set/geoip-{name}.srs"
        )
    } else {
        format!(
            "https://raw.githubusercontent.com/SagerNet/sing-geosite/rule-set/{tag}.srs"
        )
    };
    json!({
        "type": "remote",
        "tag": tag,
        "format": "binary",
        "url": url,
        "download_detour": "direct"
    })
}

fn append_inbound_routing(
    route_rules: &mut Vec<Value>,
    rule_sets: &mut BTreeSet<String>,
    routing: &Value,
) {
    let Some(obj) = routing.as_object() else {
        return;
    };
    if obj.get("blockTorrent").and_then(Value::as_bool) == Some(true) {
        route_rules.push(json!({
            "protocol": ["bittorrent"],
            "action": "reject"
        }));
    }
    if obj.get("blockAds").and_then(Value::as_bool) == Some(true) {
        let tag = "geosite-category-ads-all".to_string();
        rule_sets.insert(tag.clone());
        route_rules.push(json!({
            "rule_set": [tag],
            "action": "reject"
        }));
    }
}

/// Translates global [`RoutingRuleConfig`] items into Sing-box 1.14+ `route.rules`.
pub fn append_route_rules(
    route_rules: &mut Vec<Value>,
    rule_sets: &mut BTreeSet<String>,
    global: &[RoutingRuleConfig],
) {
    for rule in global {
        let mut r = Map::new();
        let mut exact = Vec::new();
        let mut suffix = Vec::new();
        let mut keyword = Vec::new();
        let mut regex = Vec::new();
        let mut rule_srs = Vec::new();

        for d in &rule.domain {
            let trimmed = d.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("domain:") {
                if !rest.trim().is_empty() {
                    suffix.push(rest.trim().to_string());
                }
            } else if let Some(rest) = trimmed.strip_prefix("full:") {
                if !rest.trim().is_empty() {
                    exact.push(rest.trim().to_string());
                }
            } else if let Some(rest) = trimmed.strip_prefix("keyword:") {
                if !rest.trim().is_empty() {
                    keyword.push(rest.trim().to_string());
                }
            } else if let Some(rest) = trimmed.strip_prefix("regexp:") {
                if !rest.trim().is_empty() {
                    regex.push(rest.trim().to_string());
                }
            } else if let Some(rest) = trimmed.strip_prefix("geosite:") {
                let clean = rest.trim().to_ascii_lowercase();
                if !clean.is_empty() {
                    let srs_tag = format!("geosite-{clean}");
                    rule_sets.insert(srs_tag.clone());
                    rule_srs.push(srs_tag);
                }
            } else if !trimmed.starts_with("ext:") {
                exact.push(trimmed.to_string());
            }
        }

        if !exact.is_empty() {
            r.insert("domain".into(), json!(exact));
        }
        if !suffix.is_empty() {
            r.insert("domain_suffix".into(), json!(suffix));
        }
        if !keyword.is_empty() {
            r.insert("domain_keyword".into(), json!(keyword));
        }
        if !regex.is_empty() {
            r.insert("domain_regex".into(), json!(regex));
        }

        let mut ip_cidrs = Vec::new();
        let mut ip_is_private = false;
        for ip in &rule.ip {
            let trimmed = ip.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.eq_ignore_ascii_case("geoip:private") {
                ip_is_private = true;
            } else if let Some(rest) = trimmed.strip_prefix("geoip:") {
                let clean = rest.trim().to_ascii_lowercase();
                if !clean.is_empty() {
                    let srs_tag = format!("geoip-{clean}");
                    rule_sets.insert(srs_tag.clone());
                    rule_srs.push(srs_tag);
                }
            } else if !trimmed.starts_with("ext:") {
                ip_cidrs.push(trimmed.to_string());
            }
        }

        if ip_is_private {
            r.insert("ip_is_private".into(), json!(true));
        }
        if !ip_cidrs.is_empty() {
            r.insert("ip_cidr".into(), json!(ip_cidrs));
        }
        if !rule_srs.is_empty() {
            r.insert("rule_set".into(), json!(rule_srs));
        }

        // numeric `port` (Vec<u16>) or `port_range` (Vec<String> "start:end")
        if let Some(p_spec) = rule.port.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
            let (ports, ranges) = parse_singbox_ports(p_spec);
            if !ports.is_empty() {
                r.insert("port".into(), json!(ports));
            }
            if !ranges.is_empty() {
                r.insert("port_range".into(), json!(ranges));
            }
        }

        if let Some(sp_spec) = rule
            .source_port
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            let (ports, ranges) = parse_singbox_ports(sp_spec);
            if !ports.is_empty() {
                r.insert("source_port".into(), json!(ports));
            }
            if !ranges.is_empty() {
                r.insert("source_port_range".into(), json!(ranges));
            }
        }

        if !rule.source.is_empty() {
            let src_cidrs: Vec<String> = rule
                .source
                .iter()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty() && !s.starts_with("geoip:") && !s.starts_with("ext:"))
                .map(ToString::to_string)
                .collect();
            if !src_cidrs.is_empty() {
                r.insert("source_ip_cidr".into(), json!(src_cidrs));
            }
        }

        if !rule.protocol.is_empty() {
            r.insert("protocol".into(), json!(rule.protocol));
        }
        if !rule.inbound_tag.is_empty() {
            r.insert("inbound".into(), json!(rule.inbound_tag));
        }

        // Only emit the rule if it actually has at least one match condition
        if r.is_empty() {
            continue;
        }

        if rule.outbound_tag.eq_ignore_ascii_case("block")
            || rule.outbound_tag.eq_ignore_ascii_case("reject")
        {
            r.insert("action".into(), json!("reject"));
        } else {
            let target = if rule.outbound_tag.trim().is_empty()
                || rule.outbound_tag.eq_ignore_ascii_case("proxy")
            {
                "direct"
            } else {
                rule.outbound_tag.trim()
            };
            r.insert("action".into(), json!("route"));
            r.insert("outbound".into(), json!(target));
        }

        route_rules.push(Value::Object(r));
    }
}

fn parse_singbox_ports(spec: &str) -> (Vec<u16>, Vec<String>) {
    let mut ports = Vec::new();
    let mut ranges = Vec::new();
    for part in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if let Some((a, b)) = part.split_once('-').or_else(|| part.split_once(':')) {
            if let (Ok(sa), Ok(sb)) = (a.trim().parse::<u16>(), b.trim().parse::<u16>()) {
                if sa > 0 && sb >= sa {
                    ranges.push(format!("{sa}:{sb}"));
                }
            }
        } else if let Ok(num) = part.parse::<u16>() {
            if num > 0 {
                ports.push(num);
            }
        }
    }
    (ports, ranges)
}

/// Builds one or more Sing-box inbound objects from an [`InboundConfig`].
/// For Hysteria2 with `portRange` (`<= 64` ports), expands into hop listeners or populates `listen_ports`.
pub fn build_singbox_inbounds(inbound: &InboundConfig) -> Result<Vec<Value>, String> {
    let listen = if inbound.listen.trim().is_empty() {
        "::"
    } else {
        inbound.listen.trim()
    };

    match inbound.protocol {
        Protocol::Hysteria2 => build_hysteria2(inbound, listen),
        Protocol::Tuic => Ok(vec![build_tuic(inbound, listen)?]),
        Protocol::Anytls => Ok(vec![build_anytls(inbound, listen)?]),
        Protocol::Shadowtls => Ok(vec![build_shadowtls(inbound, listen)?]),
        other => Err(format!("Protocol {other} is not supported by Sing-box")),
    }
}

fn build_hysteria2(inbound: &InboundConfig, listen: &str) -> Result<Vec<Value>, String> {
    let s = &inbound.settings;
    let users = extract_protocol_users(&inbound.users, s, true)?;
    let user_vals: Vec<Value> = users
        .iter()
        .map(|u| {
            json!({
                "name": u.name,
                "password": u.password
            })
        })
        .collect();

    let mut base = Map::new();
    base.insert("type".into(), json!("hysteria2"));
    base.insert("tag".into(), json!(inbound.tag));
    base.insert("listen".into(), json!(listen));
    base.insert("listen_port".into(), json!(inbound.port));
    base.insert("users".into(), Value::Array(user_vals));

    // Obfuscation: salamander or gecko
    if let Some(obfs) = s.get("obfs").and_then(Value::as_object) {
        let otype = obfs
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        if !otype.is_empty() && otype != "none" {
            let mut obfs_out = Map::new();
            obfs_out.insert("type".into(), json!(otype));
            let pw = obfs
                .get("password")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            if !pw.is_empty() {
                obfs_out.insert("password".into(), json!(pw));
            }
            if otype == "gecko" {
                if let Some(min_pkt) = obfs
                    .get("minPacketSize")
                    .or_else(|| obfs.get("min_packet_size"))
                    .and_then(Value::as_u64)
                {
                    obfs_out.insert("min_packet_size".into(), json!(min_pkt));
                }
                if let Some(max_pkt) = obfs
                    .get("maxPacketSize")
                    .or_else(|| obfs.get("max_packet_size"))
                    .and_then(Value::as_u64)
                {
                    obfs_out.insert("max_packet_size".into(), json!(max_pkt));
                }
            }
            base.insert("obfs".into(), Value::Object(obfs_out));
        }
    }

    if let Some(up) = num_value(s.get("uploadMbps").or_else(|| s.get("maxUploadSpeed"))) {
        base.insert("up_mbps".into(), up);
    }
    if let Some(down) = num_value(s.get("downloadMbps").or_else(|| s.get("maxDownloadSpeed"))) {
        base.insert("down_mbps".into(), down);
    }
    if let Some(masq) = s.get("masquerade").filter(|v| !v.is_null()) {
        base.insert("masquerade".into(), masq.clone());
    }
    if let Some(icb) = s
        .get("ignoreClientBandwidth")
        .or_else(|| s.get("ignore_client_bandwidth"))
        .and_then(Value::as_bool)
    {
        base.insert("ignore_client_bandwidth".into(), json!(icb));
    }

    // `bbr_profile` & `disable_chrome_parrot`
    if let Some(bbr) = s
        .get("bbrProfile")
        .or_else(|| s.get("bbr_profile"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        base.insert("bbr_profile".into(), json!(bbr));
    }
    if let Some(dcp) = s
        .get("disableChromeParrot")
        .or_else(|| s.get("disable_chrome_parrot"))
        .and_then(Value::as_bool)
    {
        base.insert("disable_chrome_parrot".into(), json!(dcp));
    }

    let tls = build_singbox_tls(s, &inbound.stream, Some(vec!["h3".into()]));
    base.insert("tls".into(), tls);

    // Hysteria2 port hopping (`portRange` / `port_range` / `listen_ports` <= 64 ports)
    // Generate distinct listener inbounds cleanly per hop port (without invalid/colliding `listen_ports` integer arrays).
    let port_range_spec = s
        .get("portRange")
        .or_else(|| s.get("port_range"))
        .or_else(|| s.get("listen_ports"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty());

    if let Some(spec) = port_range_spec {
        let hop_ports = parse_port_range(spec).map_err(|e| e.to_string())?;
        let mut results = Vec::with_capacity(hop_ports.len() + 1);
        let mut seen_primary = false;
        for p in &hop_ports {
            if *p == inbound.port {
                seen_primary = true;
            }
        }
        results.push(Value::Object(base.clone()));
        for p in hop_ports {
            if p == inbound.port && seen_primary {
                continue;
            }
            let mut hop_ib = base.clone();
            hop_ib.insert("tag".into(), json!(format!("{}#{}", inbound.tag, p)));
            hop_ib.insert("listen_port".into(), json!(p));
            results.push(Value::Object(hop_ib));
        }
        return Ok(results);
    }

    Ok(vec![Value::Object(base)])
}

fn build_tuic(inbound: &InboundConfig, listen: &str) -> Result<Value, String> {
    let s = &inbound.settings;
    let users = extract_protocol_users(&inbound.users, s, true)?;
    let mut user_vals = Vec::with_capacity(users.len());
    for u in users {
        if u.id.is_empty() {
            return Err("Missing TUIC client UUID".into());
        }
        user_vals.push(json!({
            "name": u.name,
            "uuid": u.id,
            "password": u.password
        }));
    }

    let cc = {
        let c = str_or_empty(s, "congestion_control");
        let c = if c.is_empty() {
            str_or_empty(s, "congestionControl")
        } else {
            c
        };
        if c.is_empty() {
            "bbr"
        } else {
            c
        }
    };
    let zero_rtt = s
        .get("zero_rtt")
        .or_else(|| s.get("zeroRtt"))
        .or_else(|| s.get("zero_rtt_handshake"))
        .or_else(|| s.get("zeroRttHandshake"))
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let tls = build_singbox_tls(s, &inbound.stream, Some(vec!["h3".into()]));

    let mut out = Map::new();
    out.insert("type".into(), json!("tuic"));
    out.insert("tag".into(), json!(inbound.tag));
    out.insert("listen".into(), json!(listen));
    out.insert("listen_port".into(), json!(inbound.port));
    out.insert("users".into(), Value::Array(user_vals));
    out.insert("congestion_control".into(), json!(cc));
    out.insert("zero_rtt_handshake".into(), json!(zero_rtt));
    let auth_timeout = str_or_empty(s, "authTimeout").if_empty(str_or_empty(s, "auth_timeout"));
    if !auth_timeout.is_empty() {
        out.insert("auth_timeout".into(), json!(auth_timeout));
    }
    let heartbeat = str_or_empty(s, "heartbeat");
    if !heartbeat.is_empty() {
        out.insert("heartbeat".into(), json!(heartbeat));
    }
    out.insert("tls".into(), tls);

    Ok(Value::Object(out))
}

/// AnyTLS inbound (`Sing-box 1.12+`: `type: "anytls"`, `users`, `padding_scheme`, `tls`).
fn build_anytls(inbound: &InboundConfig, listen: &str) -> Result<Value, String> {
    let s = &inbound.settings;
    let users = extract_protocol_users(&inbound.users, s, true)?;
    let user_vals: Vec<Value> = users
        .iter()
        .map(|u| {
            json!({
                "name": u.name,
                "password": u.password
            })
        })
        .collect();

    let mut out = Map::new();
    out.insert("type".into(), json!("anytls"));
    out.insert("tag".into(), json!(inbound.tag));
    out.insert("listen".into(), json!(listen));
    out.insert("listen_port".into(), json!(inbound.port));
    out.insert("users".into(), Value::Array(user_vals));

    if let Some(padding) = s
        .get("paddingScheme")
        .or_else(|| s.get("padding_scheme"))
        .filter(|v| !v.is_null())
    {
        match padding {
            Value::Array(arr) => {
                out.insert("padding_scheme".into(), Value::Array(arr.clone()));
            }
            Value::String(st) if !st.trim().is_empty() => {
                let lines: Vec<String> = st
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(ToString::to_string)
                    .collect();
                out.insert("padding_scheme".into(), json!(lines));
            }
            _ => {}
        }
    }

    let tls = build_singbox_tls(s, &inbound.stream, None);
    out.insert("tls".into(), tls);

    Ok(Value::Object(out))
}

/// ShadowTLS v3 inbound (`type: "shadowtls"`, `version: 3`, `users`, `handshake`, `detour`).
fn build_shadowtls(inbound: &InboundConfig, listen: &str) -> Result<Value, String> {
    let s = &inbound.settings;
    let users = extract_protocol_users(&inbound.users, s, true)?;
    let user_vals: Vec<Value> = users
        .iter()
        .map(|u| {
            json!({
                "name": u.name,
                "password": u.password
            })
        })
        .collect();

    let version = s
        .get("version")
        .or_else(|| s.get("shadowtlsVersion"))
        .and_then(Value::as_u64)
        .unwrap_or(3);
    let handshake_server = {
        let hs = str_or_empty(s, "handshakeServer");
        let hs = if hs.is_empty() {
            str_or_empty(s, "sni")
        } else {
            hs
        };
        let hs = if hs.is_empty() {
            str_or_empty(&inbound.stream, "sni")
        } else {
            hs
        };
        if hs.is_empty() {
            "www.microsoft.com"
        } else {
            hs
        }
    };
    let handshake_port = s
        .get("handshakePort")
        .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|st| st.trim().parse().ok())))
        .unwrap_or(443);
    let strict_mode = s
        .get("strictMode")
        .or_else(|| s.get("strict_mode"))
        .and_then(Value::as_bool)
        .unwrap_or(true);

    let mut out = Map::new();
    out.insert("type".into(), json!("shadowtls"));
    out.insert("tag".into(), json!(inbound.tag));
    out.insert("listen".into(), json!(listen));
    out.insert("listen_port".into(), json!(inbound.port));
    out.insert("version".into(), json!(version));
    out.insert("users".into(), Value::Array(user_vals));
    out.insert(
        "handshake".into(),
        json!({
            "server": handshake_server,
            "server_port": handshake_port
        }),
    );
    out.insert("strict_mode".into(), json!(strict_mode));
    if let Some(detour) = s
        .get("detour")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        out.insert("detour".into(), json!(detour));
    }
    if let Some(map) = s
        .get("handshake_for_server_name")
        .or_else(|| s.get("handshakeForServerName"))
        .filter(|v| v.is_object())
    {
        out.insert("handshake_for_server_name".into(), map.clone());
    }

    Ok(Value::Object(out))
}

trait StrExt<'a> {
    fn if_empty(self, fallback: &'a str) -> &'a str;
}

impl<'a> StrExt<'a> for &'a str {
    fn if_empty(self, fallback: &'a str) -> &'a str {
        if self.trim().is_empty() {
            fallback.trim()
        } else {
            self.trim()
        }
    }
}

fn build_singbox_tls(
    settings: &Value,
    stream: &Value,
    default_alpn: Option<Vec<String>>,
) -> Value {
    let mut tls = Map::new();
    tls.insert("enabled".into(), json!(true));

    let sni = {
        let s1 = str_or_empty(settings, "sni");
        if !s1.is_empty() {
            s1
        } else {
            str_or_empty(stream, "sni")
        }
    };
    tls.insert("server_name".into(), json!(sni));

    let custom_alpn = settings
        .get("alpn")
        .or_else(|| stream.get("alpn"))
        .or_else(|| stream.pointer("/tlsSettings/alpn"))
        .and_then(|v| match v {
            Value::Array(arr) => {
                let items: Vec<String> = arr
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(ToString::to_string)
                    .collect();
                if items.is_empty() {
                    None
                } else {
                    Some(items)
                }
            }
            Value::String(s) if !s.trim().is_empty() => Some(
                s.split(',')
                    .map(str::trim)
                    .filter(|p| !p.is_empty())
                    .map(ToString::to_string)
                    .collect(),
            ),
            _ => None,
        });

    if let Some(alpn) = custom_alpn.or(default_alpn) {
        tls.insert("alpn".into(), json!(alpn));
    }

    if let Some(min_v) = settings
        .get("minVersion")
        .or_else(|| settings.get("min_version"))
        .or_else(|| stream.pointer("/tlsSettings/minVersion"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        tls.insert("min_version".into(), json!(min_v));
    }
    if let Some(max_v) = settings
        .get("maxVersion")
        .or_else(|| settings.get("max_version"))
        .or_else(|| stream.pointer("/tlsSettings/maxVersion"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        tls.insert("max_version".into(), json!(max_v));
    }

    let cert = {
        let c = str_or_empty(settings, "certFile");
        let c = if c.is_empty() {
            str_or_empty(settings, "certificate_path")
        } else {
            c
        };
        if c.is_empty() {
            str_or_empty(stream, "certFile")
        } else {
            c
        }
    };
    if !cert.is_empty() {
        tls.insert("certificate_path".into(), json!(cert));
    }

    let key = {
        let k = str_or_empty(settings, "keyFile");
        let k = if k.is_empty() {
            str_or_empty(settings, "key_path")
        } else {
            k
        };
        if k.is_empty() {
            str_or_empty(stream, "keyFile")
        } else {
            k
        }
    };
    if !key.is_empty() {
        tls.insert("key_path".into(), json!(key));
    }

    // ECH support (`tls.ech`)
    let ech_key_path = {
        let ek = str_or_empty(settings, "echKeyPath");
        if ek.is_empty() {
            str_or_empty(settings, "ech_key_path")
        } else {
            ek
        }
    };
    let ech_key_inline = {
        let ek = str_or_empty(settings, "echKey");
        if ek.is_empty() {
            str_or_empty(stream, "echKey")
        } else {
            ek
        }
    };
    if !ech_key_path.is_empty() || !ech_key_inline.is_empty() {
        let mut ech = Map::new();
        ech.insert("enabled".into(), json!(true));
        if !ech_key_path.is_empty() {
            ech.insert("key_path".into(), json!(ech_key_path));
        }
        if !ech_key_inline.is_empty() {
            ech.insert("key".into(), json!([ech_key_inline]));
        }
        tls.insert("ech".into(), Value::Object(ech));
    }

    Value::Object(tls)
}

fn num_value(v: Option<&Value>) -> Option<Value> {
    let val = v?;
    if val.is_number() {
        return Some(val.clone());
    }
    if let Some(s) = val.as_str() {
        if let Ok(n) = s.trim().parse::<u64>() {
            return Some(json!(n));
        }
        if let Ok(f) = s.trim().parse::<f64>() {
            if f.is_finite() && f >= 0.0 {
                return Some(json!(f));
            }
        }
    }
    None
}
