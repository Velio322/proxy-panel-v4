use crate::config::DEFAULT_XRAY_API_PORT;
use crate::cores::{
    as_obj_or_empty, extract_protocol_users, str_or_empty, validate_inbound, CoreUser,
};
use mypanel_proto::links::{apply_vless_route, derive_ss2022_psk, is_ss2022};
use mypanel_proto::{InboundConfig, Protocol, RoutingRuleConfig};
use serde_json::{json, Map, Value};

/// Returns `true` if `protocol` is natively served by the Xray-core process.
pub fn is_xray_protocol(protocol: Protocol) -> bool {
    matches!(
        protocol,
        Protocol::Vless | Protocol::Vmess | Protocol::Trojan | Protocol::Shadowsocks
    )
}

/// Generates a complete Xray-core JSON configuration with default `XRAY_API_PORT = 10085`.
pub fn generate_config(
    inbounds: &[InboundConfig],
    global_rules: &[RoutingRuleConfig],
) -> Result<Value, String> {
    generate_config_with_api_port(inbounds, global_rules, DEFAULT_XRAY_API_PORT)
}

/// Generates a complete Xray-core JSON configuration (`<CONFIG_DIR>/xray.json`).
///
/// Includes:
/// - `stats: {}` + `api` (`StatsService`, `HandlerService`) + `policy.levels["0"]` (`statsUserUplink`/`statsUserDownlink`)
/// - `dokodemo-door` `"api"` inbound on `127.0.0.1:<api_port>`
/// - `"direct"` (`freedom`), `"block"` (`blackhole`), and `"proxy"` (`freedom`, ``) outbounds
/// - Per-inbound and global routing rules
pub fn generate_config_with_api_port(
    inbounds: &[InboundConfig],
    global_rules: &[RoutingRuleConfig],
    api_port: u16,
) -> Result<Value, String> {
    generate_config_with_profile(inbounds, global_rules, api_port, None)
}

/// Generates a complete Xray-core JSON configuration with optional custom profile overrides
/// (`log`, `dns`, `fakedns`, custom Cascade/Kaskad/WARP `outbounds`, `routing`, `policy`, `inboundOverrides`).
pub fn generate_config_with_profile(
    inbounds: &[InboundConfig],
    global_rules: &[RoutingRuleConfig],
    api_port: u16,
    xray_profile: Option<&Value>,
) -> Result<Value, String> {
    let mut built = Vec::new();
    let mut routing_rules = vec![
        json!({
            "type": "field",
            "inboundTag": ["api"],
            "outboundTag": "api"
        }),
        json!({
            "type": "field",
            "ip": ["geoip:private"],
            "outboundTag": "block"
        }),
    ];

    for inbound in inbounds {
        if !inbound.enable || !is_xray_protocol(inbound.protocol) {
            continue;
        }
        validate_inbound(inbound)?;
        let mut ib_val = build_inbound(inbound)?;
        if let Some(ov) = xray_profile
            .and_then(|p| p.get("inboundOverrides"))
            .and_then(|m| m.get(&inbound.tag))
            .and_then(Value::as_object)
        {
            if let Some(ib_obj) = ib_val.as_object_mut() {
                if let Some(sn) = ov.get("sniffing").filter(|v| v.is_object()) {
                    ib_obj.insert("sniffing".into(), sn.clone());
                }
                if let Some(alloc) = ov.get("allocate").filter(|v| v.is_object()) {
                    ib_obj.insert("allocate".into(), alloc.clone());
                }
                if let Some(so) = ov.get("sockopt").filter(|v| v.is_object()) {
                    if let Some(ss) = ib_obj
                        .get_mut("streamSettings")
                        .and_then(Value::as_object_mut)
                    {
                        ss.insert("sockopt".into(), so.clone());
                    }
                }
            }
        }
        built.push(ib_val);
        append_inbound_routing(&mut routing_rules, &inbound.routing);
    }

    append_global_routing(&mut routing_rules, global_rules);

    built.push(json!({
        "tag": "api",
        "listen": "127.0.0.1",
        "port": api_port,
        "protocol": "dokodemo-door",
        "settings": {
            "address": "127.0.0.1"
        }
    }));

    let mut outbounds = vec![
        json!({ "protocol": "freedom", "tag": "direct" }),
        json!({ "protocol": "blackhole", "tag": "block" }),
        json!({ "protocol": "freedom", "tag": "proxy" }),
    ];

    let mut log_obj = json!({
        "loglevel": "warning",
        "access": "none",
        "error": "/var/log/xray/error.log"
    });
    let mut policy_obj = json!({
        "system": {
            "statsInboundUplink": true,
            "statsInboundDownlink": true,
            "statsOutboundUplink": true,
            "statsOutboundDownlink": true
        },
        "levels": {
            "0": {
                "handshake": 4,
                "connIdle": 300,
                "uplinkOnly": 2,
                "downlinkOnly": 5,
                "statsUserUplink": true,
                "statsUserDownlink": true,
                "statsUserOnline": true
            }
        }
    });
    let mut domain_strategy = "IPIfNonMatch".to_string();
    let mut domain_matcher: Option<Value> = None;
    let mut balancers: Option<Value> = None;

    if let Some(profile) = xray_profile {
        if let Some(l) = profile.get("log").filter(|v| v.is_object()) {
            log_obj = l.clone();
        }
        if let Some(p_map) = profile.get("policy").and_then(Value::as_object) {
            if let Some(base_map) = policy_obj.as_object_mut() {
                for (k, v) in p_map {
                    if k == "levels" {
                        if let (Some(base_levels), Some(ov_levels)) = (
                            base_map.get_mut("levels").and_then(Value::as_object_mut),
                            v.as_object(),
                        ) {
                            for (lvl_key, lvl_val) in ov_levels {
                                if lvl_key == "0" {
                                    if let (Some(base_l0), Some(ov_l0)) = (
                                        base_levels.get_mut("0").and_then(Value::as_object_mut),
                                        lvl_val.as_object(),
                                    ) {
                                        for (lk, lv) in ov_l0 {
                                            base_l0.insert(lk.clone(), lv.clone());
                                        }
                                        continue;
                                    }
                                }
                                base_levels.insert(lvl_key.clone(), lvl_val.clone());
                            }
                            continue;
                        }
                    } else if k == "system" {
                        if let (Some(base_sys), Some(ov_sys)) = (
                            base_map.get_mut("system").and_then(Value::as_object_mut),
                            v.as_object(),
                        ) {
                            for (sk, sv) in ov_sys {
                                base_sys.insert(sk.clone(), sv.clone());
                            }
                            continue;
                        }
                    }
                    base_map.insert(k.clone(), v.clone());
                }
            }
        }
        // Per-user Xray accounting requires statsUserUplink/statsUserDownlink on level 0.
        if let Some(l0) = policy_obj
            .get_mut("levels")
            .and_then(Value::as_object_mut)
            .and_then(|lv| lv.get_mut("0"))
            .and_then(Value::as_object_mut)
        {
            l0.insert("statsUserUplink".into(), json!(true));
            l0.insert("statsUserDownlink".into(), json!(true));
        }
        if let Some(custom_outs) = profile.get("outbounds").and_then(Value::as_array) {
            for co in custom_outs {
                let Some(tag) = co.get("tag").and_then(Value::as_str).map(str::trim) else {
                    outbounds.push(co.clone());
                    continue;
                };
                if tag == "api" {
                    continue;
                }
                if let Some(idx) = outbounds
                    .iter()
                    .position(|o| o.get("tag").and_then(Value::as_str) == Some(tag))
                {
                    outbounds[idx] = co.clone();
                } else {
                    outbounds.push(co.clone());
                }
            }
        }
        if let Some(rt) = profile.get("routing").and_then(Value::as_object) {
            if let Some(ds) = rt.get("domainStrategy").and_then(Value::as_str).map(str::trim) {
                if !ds.is_empty() {
                    domain_strategy = ds.to_string();
                }
            }
            if let Some(dm) = rt.get("domainMatcher").filter(|v| !v.is_null()) {
                domain_matcher = Some(dm.clone());
            }
            if let Some(bs) = rt.get("balancers").filter(|v| v.is_array()) {
                balancers = Some(bs.clone());
            }
            if let Some(extra_rules) = rt.get("rules").and_then(Value::as_array) {
                for er in extra_rules {
                    let is_api_rule = er
                        .get("inboundTag")
                        .and_then(Value::as_array)
                        .is_some_and(|arr| arr.iter().any(|x| x.as_str() == Some("api")));
                    if !is_api_rule && er.is_object() {
                        routing_rules.push(er.clone());
                    }
                }
            }
        }
    }

    let mut routing_obj = Map::new();
    routing_obj.insert("domainStrategy".into(), json!(domain_strategy));
    if let Some(dm) = domain_matcher {
        routing_obj.insert("domainMatcher".into(), dm);
    }
    if let Some(bs) = balancers {
        routing_obj.insert("balancers".into(), bs);
    }
    routing_obj.insert("rules".into(), Value::Array(routing_rules));

    let mut root = Map::new();
    root.insert("log".into(), log_obj);
    root.insert("stats".into(), json!({}));
    root.insert(
        "api".into(),
        json!({
            "tag": "api",
            "services": ["StatsService", "HandlerService"]
        }),
    );
    root.insert("policy".into(), policy_obj);
    if let Some(dns) = xray_profile.and_then(|p| p.get("dns")).filter(|v| v.is_object()) {
        root.insert("dns".into(), dns.clone());
    }
    if let Some(fakedns) = xray_profile
        .and_then(|p| p.get("fakedns"))
        .filter(|v| v.is_object() || v.is_array())
    {
        root.insert("fakedns".into(), fakedns.clone());
    }
    root.insert("inbounds".into(), Value::Array(built));
    root.insert("outbounds".into(), Value::Array(outbounds));
    root.insert("routing".into(), Value::Object(routing_obj));

    Ok(Value::Object(root))
}

fn append_inbound_routing(rules: &mut Vec<Value>, routing: &Value) {
    let Some(obj) = routing.as_object() else {
        return;
    };
    if obj.get("blockTorrent").and_then(Value::as_bool) == Some(true) {
        rules.push(json!({
            "type": "field",
            "protocol": ["bittorrent"],
            "outboundTag": "block"
        }));
    }
    if obj.get("blockAds").and_then(Value::as_bool) == Some(true) {
        rules.push(json!({
            "type": "field",
            "domain": ["geosite:category-ads-all"],
            "outboundTag": "block"
        }));
    }
    if let Some(custom) = obj.get("customRules").and_then(Value::as_array) {
        for r in custom {
            if r.is_object() {
                rules.push(r.clone());
            }
        }
    }
}

fn append_global_routing(rules: &mut Vec<Value>, global: &[RoutingRuleConfig]) {
    for r in global {
        let mut entry = Map::new();
        entry.insert(
            "type".into(),
            json!(if r.rule_type.trim().is_empty() {
                "field"
            } else {
                r.rule_type.trim()
            }),
        );
        if !r.domain.is_empty() {
            entry.insert("domain".into(), json!(r.domain));
        }
        if !r.ip.is_empty() {
            entry.insert("ip".into(), json!(r.ip));
        }
        if let Some(ref p) = r.port.as_ref().filter(|s| !s.trim().is_empty()) {
            entry.insert("port".into(), json!(p.trim()));
        }
        if let Some(ref sp) = r.source_port.as_ref().filter(|s| !s.trim().is_empty()) {
            entry.insert("sourcePort".into(), json!(sp.trim()));
        }
        if !r.source.is_empty() {
            entry.insert("source".into(), json!(r.source));
        }
        if !r.protocol.is_empty() {
            entry.insert("protocol".into(), json!(r.protocol));
        }
        if !r.inbound_tag.is_empty() {
            entry.insert("inboundTag".into(), json!(r.inbound_tag));
        }
        if let Some(ref bt) = r.balancer_tag.as_ref().filter(|s| !s.trim().is_empty()) {
            entry.insert("balancerTag".into(), json!(bt.trim()));
        } else {
            let out = if r.outbound_tag.trim().is_empty() {
                "direct"
            } else {
                r.outbound_tag.trim()
            };
            entry.insert("outboundTag".into(), json!(out));
        }
        rules.push(Value::Object(entry));
    }
}

/// Builds a single Xray inbound JSON object from an [`InboundConfig`].
pub fn build_inbound(inbound: &InboundConfig) -> Result<Value, String> {
    validate_inbound(inbound)?;
    let listen = if inbound.listen.trim().is_empty() {
        "0.0.0.0"
    } else {
        inbound.listen.trim()
    };

    let settings = match inbound.protocol {
        Protocol::Vless => build_vless(inbound)?,
        Protocol::Vmess => build_vmess(inbound)?,
        Protocol::Trojan => build_trojan(inbound)?,
        Protocol::Shadowsocks => build_shadowsocks(inbound)?,
        other => return Err(format!("Protocol {other} is not supported by Xray")),
    };

    let stream_settings = build_stream_settings(&inbound.stream, &inbound.settings);

    let sn_cfg = inbound
        .settings
        .get("sniffing")
        .or_else(|| inbound.stream.get("sniffing"))
        .and_then(Value::as_object);
    let sn_enabled = sn_cfg
        .and_then(|o| o.get("enabled").and_then(Value::as_bool))
        .unwrap_or(inbound.sniffing);
    let sn_dest = sn_cfg
        .and_then(|o| o.get("destOverride").and_then(Value::as_array))
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| vec!["http".into(), "tls".into(), "quic".into(), "fakedns".into()]);
    let sn_route_only = sn_cfg
        .and_then(|o| o.get("routeOnly").and_then(Value::as_bool))
        .unwrap_or(true);

    let mut sniffing_obj = Map::new();
    sniffing_obj.insert("enabled".into(), json!(sn_enabled));
    sniffing_obj.insert("destOverride".into(), json!(sn_dest));
    sniffing_obj.insert("routeOnly".into(), json!(sn_route_only));
    if let Some(meta_only) = sn_cfg.and_then(|o| o.get("metadataOnly").and_then(Value::as_bool)) {
        if meta_only {
            sniffing_obj.insert("metadataOnly".into(), json!(true));
        }
    }
    if let Some(excl) = sn_cfg
        .and_then(|o| o.get("domainsExcluded").and_then(Value::as_array))
        .filter(|a| !a.is_empty())
    {
        sniffing_obj.insert("domainsExcluded".into(), Value::Array(excl.clone()));
    }

    let mut ib_out = Map::new();
    ib_out.insert("tag".into(), json!(inbound.tag));
    ib_out.insert("port".into(), json!(inbound.port));
    ib_out.insert("listen".into(), json!(listen));
    ib_out.insert(
        "protocol".into(),
        json!(inbound.protocol.as_str().to_ascii_lowercase()),
    );
    ib_out.insert("settings".into(), settings);
    ib_out.insert("streamSettings".into(), stream_settings);
    ib_out.insert("sniffing".into(), Value::Object(sniffing_obj));
    if let Some(alloc) = inbound.settings.get("allocate").filter(|v| v.is_object()) {
        ib_out.insert("allocate".into(), alloc.clone());
    }

    Ok(Value::Object(ib_out))
}

fn build_vless(inbound: &InboundConfig) -> Result<Value, String> {
    let s = &inbound.settings;
    let default_flow = str_or_empty(s, "flow");
    let default_flow = if default_flow.is_empty() {
        str_or_empty(&inbound.stream, "flow")
    } else {
        default_flow
    };
    let vless_route = str_or_empty(s, "vlessRouteId");
    let vless_route = if vless_route.is_empty() {
        str_or_empty(s, "vlessRoute")
    } else {
        vless_route
    };

    let users = extract_protocol_users(&inbound.users, s, false)?;

    let mut clients = Vec::with_capacity(users.len());
    for u in users {
        if u.id.is_empty() {
            return Err("Missing client UUID".into());
        }
        let client_uuid = if !vless_route.is_empty() {
            apply_vless_route(&u.id, vless_route).map_err(|e| e.to_string())?
        } else {
            u.id
        };
        let flow = if !u.flow.is_empty() {
            u.flow
        } else {
            default_flow.to_string()
        };
        clients.push(json!({
            "id": client_uuid,
            "flow": flow,
            "email": u.email,
            "level": u.level
        }));
    }

    // Xray 2026: support `vlessenc` decryption/encryption configuration
    let decryption = s
        .get("decryption")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .or_else(|| {
            s.get("vlessDecryption")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|v| !v.is_empty())
        })
        .unwrap_or("none");

    let mut out = Map::new();
    out.insert("clients".into(), Value::Array(clients));
    out.insert("decryption".into(), json!(decryption));
    if let Some(enc) = s
        .get("encryption")
        .or_else(|| s.get("vlessEnc"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty() && *v != "none")
    {
        out.insert("encryption".into(), json!(enc));
    }
    if !vless_route.is_empty() {
        out.insert("vlessRoute".into(), json!(vless_route));
    }
    if let Some(fallbacks) = s.get("fallbacks").and_then(Value::as_array) {
        out.insert("fallbacks".into(), Value::Array(fallbacks.clone()));
    }

    Ok(Value::Object(out))
}

fn build_vmess(inbound: &InboundConfig) -> Result<Value, String> {
    let s = &inbound.settings;
    let alter_id = s
        .get("alterId")
        .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|st| st.parse().ok())))
        .unwrap_or(0);
    let users = extract_protocol_users(&inbound.users, s, false)?;

    let mut clients = Vec::with_capacity(users.len());
    for u in users {
        if u.id.is_empty() {
            return Err("Missing client UUID".into());
        }
        clients.push(json!({
            "id": u.id,
            "alterId": alter_id,
            "email": u.email,
            "level": u.level
        }));
    }

    Ok(json!({
        "clients": clients
    }))
}

fn build_trojan(inbound: &InboundConfig) -> Result<Value, String> {
    let s = &inbound.settings;
    let users = extract_protocol_users(&inbound.users, s, false)?;

    let mut clients = Vec::with_capacity(users.len());
    for u in users {
        if u.password.is_empty() {
            return Err("Missing persisted Trojan credential".into());
        }
        clients.push(json!({
            "password": u.password,
            "email": u.email,
            "level": u.level
        }));
    }

    let mut out = Map::new();
    out.insert("clients".into(), Value::Array(clients));
    if let Some(fallbacks) = s.get("fallbacks").and_then(Value::as_array) {
        out.insert("fallbacks".into(), Value::Array(fallbacks.clone()));
    }
    Ok(Value::Object(out))
}

/// Builds Shadowsocks / Shadowsocks-2022 settings:
/// - Single-user legacy Shadowsocks emits `"email"` and `"level": 0` for `StatsService` accounting.
/// - Multi-user / `2022-blake3-*` includes server `"password"` (`iPSK`, normalized to 16B/32B standard Base64)
/// plus `"clients"` array with per-user `"password"` (`uPSK`), `"email"`, and `"level": 0`.
pub fn build_shadowsocks(inbound: &InboundConfig) -> Result<Value, String> {
    let s = &inbound.settings;
    let method = {
        let m = str_or_empty(s, "method");
        if m.is_empty() {
            "aes-256-gcm"
        } else {
            m
        }
    };
    let network = {
        let n = str_or_empty(s, "network");
        if n.is_empty() {
            "tcp,udp"
        } else {
            n
        }
    };

    let users: Vec<CoreUser> = extract_protocol_users(&inbound.users, s, false)?;
    if users.is_empty() {
        return Err("Missing Shadowsocks credential".into());
    }

    let ss2022 = is_ss2022(method);

    // Single-user non-2022 emits top-level password + email + level: 0
    if users.len() == 1 && !ss2022 {
        let u = &users[0];
        if u.password.is_empty() {
            return Err("Missing Shadowsocks credential".into());
        }
        return Ok(json!({
            "method": method,
            "password": u.password,
            "network": network,
            "email": u.email,
            "level": u.level
        }));
    }

    let mut clients = Vec::with_capacity(users.len());
    for u in &users {
        if u.password.is_empty() {
            return Err("Missing Shadowsocks credential".into());
        }
        let user_psk = if ss2022 {
            derive_ss2022_psk(&u.password, method)
        } else {
            u.password.clone()
        };
        let mut entry = Map::new();
        entry.insert("password".into(), json!(user_psk));
        if !u.email.is_empty() {
            entry.insert("email".into(), json!(u.email));
        }
        entry.insert("level".into(), json!(u.level));
        if !ss2022 {
            entry.insert("method".into(), json!(method));
        }
        clients.push(Value::Object(entry));
    }

    let mut out = Map::new();
    out.insert("method".into(), json!(method));
    out.insert("network".into(), json!(network));

    // Server iPSK at `settings.password`
    let raw_server_psk = str_or_empty(s, "password");
    let server_psk = if !raw_server_psk.is_empty() {
        if ss2022 {
            derive_ss2022_psk(raw_server_psk, method)
        } else {
            raw_server_psk.to_string()
        }
    } else if ss2022 {
        // Match crates/proto/src/links.rs fallback so server iPSK stays stable across client changes.
        derive_ss2022_psk(&format!("ss2022-ipsk:{}", inbound.tag), method)
    } else if let Some(first_pw) = clients
        .first()
        .and_then(|c| c.get("password"))
        .and_then(Value::as_str)
    {
        first_pw.to_string()
    } else {
        String::new()
    };

    if !server_psk.is_empty() {
        out.insert("password".into(), json!(server_psk));
    }
    out.insert("clients".into(), Value::Array(clients));
    Ok(Value::Object(out))
}

fn build_stream_settings(stream: &Value, settings: &Value) -> Value {
    let network = {
        let n = str_or_empty(stream, "network");
        if n.is_empty() {
            "tcp"
        } else {
            n
        }
    };
    let mut stream_settings = Map::new();
    stream_settings.insert("network".into(), json!(network));

    let security = str_or_empty(stream, "security").to_ascii_lowercase();
    match security.as_str() {
        "tls" => {
            stream_settings.insert("security".into(), json!("tls"));
            let mut tls = as_obj_or_empty(stream.get("tlsSettings").unwrap_or(&Value::Null));
            let sni = {
                let s = str_or_empty(stream, "sni");
                if !s.is_empty() {
                    s.to_string()
                } else {
                    tls.get("serverName")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .trim()
                        .to_string()
                }
            };
            tls.insert("serverName".into(), json!(sni));

            let fp = {
                let f = str_or_empty(stream, "fingerprint");
                if !f.is_empty() {
                    f.to_string()
                } else {
                    tls.get("fingerprint")
                        .and_then(Value::as_str)
                        .filter(|s| !s.trim().is_empty())
                        .unwrap_or("chrome")
                        .to_string()
                }
            };
            tls.insert("fingerprint".into(), json!(fp));

            let alpn = parse_alpn(stream.get("alpn").or_else(|| tls.get("alpn")));
            tls.insert("alpn".into(), json!(alpn));
            tls.entry("minVersion").or_insert_with(|| json!("1.2"));
            tls.entry("maxVersion").or_insert_with(|| json!("1.3"));
            tls.entry("rejectUnknownSni")
                .or_insert_with(|| json!(true));

            // Normalize certificateFile / keyFile if provided at top-level stream or settings
            if !tls.contains_key("certificates") {
                let cert_file = str_or_empty(stream, "certFile")
                    .or_not_empty(str_or_empty(settings, "certFile"))
                    .or_not_empty(str_or_empty(settings, "certificate_path"));
                let key_file = str_or_empty(stream, "keyFile")
                    .or_not_empty(str_or_empty(settings, "keyFile"))
                    .or_not_empty(str_or_empty(settings, "key_path"));
                if !cert_file.is_empty() && !key_file.is_empty() {
                    tls.insert(
                        "certificates".into(),
                        json!([{
                            "certificateFile": cert_file,
                            "keyFile": key_file
                        }]),
                    );
                }
            }

            // Xray 2026: `pinnedPeerCertSha256` & `verifyPeerCertByName` replace deprecated `allowInsecure`
            let pinned = str_or_empty(stream, "pinnedPeerCertSha256")
                .or_not_empty(
                    tls.get("pinnedPeerCertSha256")
                        .and_then(Value::as_str)
                        .unwrap_or(""),
                )
                .or_not_empty(str_or_empty(settings, "pinnedPeerCertSha256"))
                .or_not_empty(str_or_empty(settings, "pinSHA256"))
                .to_string();
            let verify_name = str_or_empty(stream, "verifyPeerCertByName")
                .or_not_empty(
                    tls.get("verifyPeerCertByName")
                        .and_then(Value::as_str)
                        .unwrap_or(""),
                )
                .or_not_empty(str_or_empty(settings, "verifyPeerCertByName"))
                .to_string();

            if !pinned.is_empty() || !verify_name.is_empty() {
                tls.remove("allowInsecure");
                if !pinned.is_empty() {
                    tls.insert("pinnedPeerCertSha256".into(), json!(pinned));
                }
                if !verify_name.is_empty() {
                    tls.insert("verifyPeerCertByName".into(), json!(verify_name));
                }
            } else {
                let allow_insecure = stream
                    .get("allowInsecure")
                    .and_then(Value::as_bool)
                    .or_else(|| tls.get("allowInsecure").and_then(Value::as_bool))
                    .unwrap_or(false);
                tls.insert("allowInsecure".into(), json!(allow_insecure));
            }

            if let Some(ech) = stream
                .get("echServerKeys")
                .or_else(|| tls.get("echServerKeys"))
                .or_else(|| settings.get("echServerKeys"))
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
            {
                tls.insert("echServerKeys".into(), json!(ech.trim()));
            }
            for k in [
                "cipherSuites",
                "curvePreferences",
                "ocspStapling",
                "disableSystemRoot",
                "rejectUnknownSni",
            ] {
                if let Some(v) = stream
                    .get(k)
                    .or_else(|| tls.get(k))
                    .or_else(|| settings.get(k))
                    .filter(|v| !v.is_null())
                {
                    let empty_str = v.as_str().is_some_and(|s| s.trim().is_empty());
                    if !empty_str {
                        tls.insert(k.into(), v.clone());
                    }
                }
            }

            stream_settings.insert("tlsSettings".into(), Value::Object(tls));
        }
        "reality" => {
            stream_settings.insert("security".into(), json!("reality"));
            let r_in = stream.get("realitySettings").unwrap_or(&Value::Null);
            let mut r = as_obj_or_empty(r_in);

            let dest = str_or_empty(r_in, "target")
                .or_not_empty(str_or_empty(r_in, "dest"))
                .or_not_empty(str_or_empty(stream, "target"))
                .or_not_empty(str_or_empty(stream, "dest"))
                .or_not_empty("www.google.com:443")
                .to_string();
            r.insert("dest".into(), json!(dest));
            r.insert("target".into(), json!(dest));

            let priv_key = str_or_empty(r_in, "privateKey")
                .or_not_empty(str_or_empty(stream, "privateKey"))
                .to_string();
            r.insert("privateKey".into(), json!(priv_key));

            if let Some(pub_key) = r_in
                .get("publicKey")
                .or_else(|| stream.get("publicKey"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                r.insert("publicKey".into(), json!(pub_key));
            }

            // filter out empty strings in `realitySettings.serverNames`; `[]` when empty, NEVER `[""]`
            let server_names = extract_non_empty_str_vec(
                r_in.get("serverNames")
                    .or_else(|| stream.get("serverNames")),
                str_or_empty(stream, "sni"),
            );
            r.insert("serverNames".into(), json!(server_names));

            let short_ids = extract_short_ids(
                r_in.get("shortIds").or_else(|| stream.get("shortIds")),
                str_or_empty(stream, "shortId"),
            );
            r.insert("shortIds".into(), json!(short_ids));

            // Pass through additional Reality v25/v26 tuning parameters
            for k in [
                "xver",
                "show",
                "maxTimeDiff",
                "minClientVer",
                "maxClientVer",
                "limitFallbackUpload",
                "limitFallbackDownload",
            ] {
                if let Some(v) = r_in
                    .get(k)
                    .or_else(|| stream.get(k))
                    .filter(|v| !v.is_null())
                {
                    let empty_str = v.as_str().is_some_and(|s| s.trim().is_empty());
                    if !empty_str {
                        r.insert(k.into(), v.clone());
                    }
                }
            }

            // Xray 2026: Post-Quantum ML-DSA-65 seed & verification key
            if let Some(seed) = r_in
                .get("mldsa65Seed")
                .or_else(|| stream.get("mldsa65Seed"))
                .or_else(|| settings.get("mldsa65Seed"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                r.insert("mldsa65Seed".into(), json!(seed));
            }
            if let Some(verify) = r_in
                .get("mldsa65Verify")
                .or_else(|| stream.get("mldsa65Verify"))
                .or_else(|| settings.get("mldsa65Verify"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                r.insert("mldsa65Verify".into(), json!(verify));
            }

            stream_settings.insert("realitySettings".into(), Value::Object(r));
        }
        _ => {}
    }

    match network {
        "tcp" | "raw" => {
            let key = if network == "raw" {
                "rawSettings"
            } else {
                "tcpSettings"
            };
            let t = stream
                .get(key)
                .or_else(|| stream.get("tcpSettings"))
                .or_else(|| stream.get("rawSettings"));
            if let Some(t_val) = t.filter(|v| v.is_object()) {
                let mut out = Map::new();
                if let Some(pp) = t_val.get("acceptProxyProtocol").and_then(Value::as_bool) {
                    out.insert("acceptProxyProtocol".into(), json!(pp));
                }
                if let Some(hdr) = t_val.get("header").filter(|v| v.is_object()) {
                    out.insert("header".into(), hdr.clone());
                }
                if !out.is_empty() {
                    stream_settings.insert(key.into(), Value::Object(out));
                }
            }
        }
        "ws" => {
            let ws = stream.get("wsSettings").unwrap_or(&Value::Null);
            let path = {
                let p = str_or_empty(ws, "path").or_not_empty(str_or_empty(stream, "path"));
                if p.is_empty() {
                    "/"
                } else {
                    p
                }
            };
            let host = str_or_empty(ws, "host")
                .or_not_empty(
                    ws.get("headers")
                        .and_then(|h| h.get("Host"))
                        .and_then(Value::as_str)
                        .unwrap_or(""),
                )
                .or_not_empty(str_or_empty(stream, "host"));
            let mut out = Map::new();
            out.insert("path".into(), json!(path));
            // populate `wsSettings.host` when non-empty
            if !host.is_empty() {
                out.insert("host".into(), json!(host));
            }
            let headers = ws
                .get("headers")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            out.insert("headers".into(), Value::Object(headers));
            if let Some(pp) = ws.get("acceptProxyProtocol").and_then(Value::as_bool) {
                if pp {
                    out.insert("acceptProxyProtocol".into(), json!(true));
                }
            }
            if let Some(hb) = ws.get("heartbeatPeriod").and_then(Value::as_u64) {
                if hb > 0 {
                    out.insert("heartbeatPeriod".into(), json!(hb));
                }
            }
            stream_settings.insert("wsSettings".into(), Value::Object(out));
        }
        "grpc" => {
            let g = stream.get("grpcSettings").unwrap_or(&Value::Null);
            let service_name = str_or_empty(g, "serviceName")
                .or_not_empty(str_or_empty(stream, "serviceName"))
                .or_not_empty(str_or_empty(stream, "path"));
            let multi_mode = g
                .get("multiMode")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let mut out = Map::new();
            out.insert("serviceName".into(), json!(service_name));
            out.insert("multiMode".into(), json!(multi_mode));
            for k in [
                "authority",
                "idle_timeout",
                "health_check_timeout",
                "permit_without_stream",
                "initial_windows_size",
            ] {
                if let Some(v) = g.get(k).filter(|v| !v.is_null()) {
                    let empty_str = v.as_str().is_some_and(|s| s.trim().is_empty());
                    if !empty_str {
                        out.insert(k.into(), v.clone());
                    }
                }
            }
            stream_settings.insert("grpcSettings".into(), Value::Object(out));
        }
        "httpupgrade" => {
            let h = stream.get("httpupgradeSettings").unwrap_or(&Value::Null);
            let path = {
                let p = str_or_empty(h, "path").or_not_empty(str_or_empty(stream, "path"));
                if p.is_empty() {
                    "/"
                } else {
                    p
                }
            };
            let host = str_or_empty(h, "host").or_not_empty(str_or_empty(stream, "host"));
            let accept_pp = h
                .get("acceptProxyProtocol")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            stream_settings.insert(
                "httpupgradeSettings".into(),
                json!({
                    "path": path,
                    "host": host,
                    "acceptProxyProtocol": accept_pp
                }),
            );
        }
        "xhttp" => {
            let x = stream.get("xhttpSettings").unwrap_or(&Value::Null);
            let path = str_or_empty(x, "path")
                .or_not_empty(str_or_empty(stream, "path"))
                .or_not_empty("/");
            let host = str_or_empty(x, "host").or_not_empty(str_or_empty(stream, "host"));
            let mode = {
                let m = str_or_empty(x, "mode");
                if m.is_empty() {
                    "auto"
                } else {
                    m
                }
            };
            let mut extra = as_obj_or_empty(x.get("extra").unwrap_or(&Value::Null));
            if let Some(xmux) = x
                .get("xmux")
                .or_else(|| stream.get("xmux"))
                .filter(|v| !v.is_null())
            {
                extra.entry("xmux").or_insert_with(|| xmux.clone());
            }
            if let Some(dl) = x
                .get("downloadSettings")
                .or_else(|| stream.get("downloadSettings"))
                .filter(|v| !v.is_null())
            {
                extra
                    .entry("downloadSettings")
                    .or_insert_with(|| dl.clone());
            }
            let mut x_out = Map::new();
            // populate `xhttpSettings.host`
            x_out.insert("host".into(), json!(host));
            x_out.insert("path".into(), json!(path));
            x_out.insert("mode".into(), json!(mode));
            for k in [
                "scMaxEachPostBytes",
                "scMinPostsIntervalMs",
                "scMaxBufferedPosts",
                "scStreamUpServerSecs",
                "xPaddingBytes",
                "noGRPCHeader",
                "noSSEHeader",
                "headers",
            ] {
                if let Some(v) = x.get(k).filter(|v| !v.is_null()) {
                    let empty_str = v.as_str().is_some_and(|s| s.trim().is_empty());
                    if !empty_str {
                        x_out.insert(k.into(), v.clone());
                        extra.entry(k).or_insert_with(|| v.clone());
                    }
                }
            }
            if !extra.is_empty() {
                x_out.insert("extra".into(), Value::Object(extra));
            }
            if let Some(xmux) = x.get("xmux").filter(|v| !v.is_null()) {
                x_out.insert("xmux".into(), xmux.clone());
            }
            if let Some(dl) = x.get("downloadSettings").filter(|v| !v.is_null()) {
                x_out.insert("downloadSettings".into(), dl.clone());
            }
            stream_settings.insert("xhttpSettings".into(), Value::Object(x_out));
        }
        "h2" => {
            let h = stream.get("httpSettings").unwrap_or(&Value::Null);
            let path = {
                let p = str_or_empty(h, "path").or_not_empty(str_or_empty(stream, "path"));
                if p.is_empty() {
                    "/"
                } else {
                    p
                }
            };
            let host_vec: Vec<String> = match h.get("host") {
                Some(Value::Array(arr)) => arr
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(ToString::to_string)
                    .collect(),
                Some(Value::String(s)) if !s.trim().is_empty() => s
                    .split(',')
                    .map(str::trim)
                    .filter(|p| !p.is_empty())
                    .map(ToString::to_string)
                    .collect(),
                _ => vec![String::new()],
            };
            let pass_through = h
                .get("passThroughUri")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            stream_settings.insert(
                "httpSettings".into(),
                json!({
                    "path": path,
                    "host": host_vec,
                    "passThroughUri": pass_through
                }),
            );
        }
        "kcp" => {
            let k = stream.get("kcpSettings").unwrap_or(&Value::Null);
            let header_type = {
                let ht = str_or_empty(k, "headerType").or_not_empty(
                    k.get("header")
                        .and_then(|h| h.get("type"))
                        .and_then(Value::as_str)
                        .unwrap_or(""),
                );
                if ht.is_empty() {
                    "none"
                } else {
                    ht
                }
            };
            let seed = str_or_empty(k, "seed");
            let mut out = Map::new();
            out.insert("header".into(), json!({ "type": header_type }));
            out.insert("seed".into(), json!(seed));
            for num_k in [
                "mtu",
                "tti",
                "uplinkCapacity",
                "downlinkCapacity",
                "readBufferSize",
                "writeBufferSize",
            ] {
                if let Some(v) = k.get(num_k).and_then(|x| x.as_u64()) {
                    out.insert(num_k.into(), json!(v));
                }
            }
            if let Some(cong) = k.get("congestion").and_then(Value::as_bool) {
                out.insert("congestion".into(), json!(cong));
            }
            stream_settings.insert("kcpSettings".into(), Value::Object(out));
        }
        _ => {}
    }

    // Xray 2026: `finalmask` / `finalMask` (Sudoku, header-custom, TCP fragment, UDP noise/hop)
    if let Some(fm) = stream
        .get("finalmask")
        .or_else(|| stream.get("finalMask"))
        .or_else(|| settings.get("finalmask"))
        .or_else(|| settings.get("finalMask"))
        .filter(|v| !v.is_null())
    {
        stream_settings.insert("finalmask".into(), fm.clone());
    }

    if let Some(sockopt) = stream
        .get("sockopt")
        .or_else(|| settings.get("sockopt"))
        .filter(|v| v.is_object())
    {
        stream_settings.insert("sockopt".into(), sockopt.clone());
    }

    Value::Object(stream_settings)
}

trait OrNotEmpty<'a> {
    fn or_not_empty(self, fallback: &'a str) -> &'a str;
}

impl<'a> OrNotEmpty<'a> for &'a str {
    fn or_not_empty(self, fallback: &'a str) -> &'a str {
        if !self.trim().is_empty() {
            self.trim()
        } else {
            fallback.trim()
        }
    }
}

fn parse_alpn(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Array(arr)) => {
            let list: Vec<String> = arr
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToString::to_string)
                .collect();
            if list.is_empty() {
                vec!["h2".into(), "http/1.1".into()]
            } else {
                list
            }
        }
        Some(Value::String(s)) if !s.trim().is_empty() => s
            .split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(ToString::to_string)
            .collect(),
        _ => vec!["h2".into(), "http/1.1".into()],
    }
}

fn extract_non_empty_str_vec(v: Option<&Value>, fallback_sni: &str) -> Vec<String> {
    if let Some(Value::Array(arr)) = v {
        let filtered: Vec<String> = arr
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
            .collect();
        if !filtered.is_empty() {
            return filtered;
        }
    } else if let Some(Value::String(s)) = v {
        let filtered: Vec<String> = s
            .split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(ToString::to_string)
            .collect();
        if !filtered.is_empty() {
            return filtered;
        }
    }
    let fb = fallback_sni.trim();
    if fb.is_empty() {
        Vec::new()
    } else {
        vec![fb.to_string()]
    }
}

fn extract_short_ids(v: Option<&Value>, fallback_short_id: &str) -> Vec<String> {
    if let Some(Value::Array(arr)) = v {
        if !arr.is_empty() {
            return arr
                .iter()
                .filter_map(Value::as_str)
                .map(|s| s.trim().to_string())
                .collect();
        }
    } else if let Some(Value::String(s)) = v {
        if !s.trim().is_empty() {
            return s.split(',').map(|p| p.trim().to_string()).collect();
        }
    }
    vec![fallback_short_id.trim().to_string()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use mypanel_proto::NodeUser;

    /// Exact `VLESS + XHTTP + Reality` inbound `streamSettings` from the reference
    /// Remnawave node config (`VLESS_XHTTP_REALITY`).
    fn reference_stream() -> Value {
        json!({
            "network": "xhttp",
            "sockopt": {
                "mark": 0,
                "V6Only": false,
                "tproxy": "off",
                "tcpMptcp": false,
                "interface": "",
                "penetrate": false,
                "tcpMaxSeg": 1440,
                "dialerProxy": "",
                "tcpFastOpen": true,
                "tcpcongestion": "bbr",
                "domainStrategy": "UseIP",
                "tcpUserTimeout": 10000,
                "tcpWindowClamp": 600,
                "tcpKeepAliveIdle": 300,
                "acceptProxyProtocol": false,
                "tcpKeepAliveInterval": 0
            },
            "security": "reality",
            "xhttpSettings": {
                "host": "www.twitch.tv",
                "mode": "packet-up",
                "path": "/"
            },
            "realitySettings": {
                "show": false,
                "xver": 0,
                "target": "www.twitch.tv:443",
                "shortIds": [""],
                "privateKey": "L-y1ZTWHuRgg09Bv0JJOYqQkly9jMcsmD7D97O2093o",
                "publicKey": "YSia7kbg9eXUatxAQ5e2nbmKkFNgormAPs27SsWeJXg",
                "fingerprint": "firefox",
                "serverNames": ["www.twitch.tv"]
            }
        })
    }

    fn reference_inbound() -> InboundConfig {
        InboundConfig {
            id: "in-ref".into(),
            tag: "VLESS_XHTTP_REALITY".into(),
            protocol: Protocol::Vless,
            listen: "0.0.0.0".into(),
            port: 443,
            settings: json!({ "decryption": "none" }),
            stream: reference_stream(),
            routing: json!({}),
            sniffing: true,
            remark: None,
            enable: true,
            users: vec![NodeUser {
                id: "3a8a4cc5-c756-41a3-9961-3789079074a7".into(),
                username: "finland-user".into(),
                uuid: "3a8a4cc5-c756-41a3-9961-3789079074a7".into(),
                email: "finland-user".into(),
                name: "finland-user".into(),
                password: Some("3a8a4cc5-c756-41a3-9961-3789079074a7".into()),
                auth: None,
                flow: Some(String::new()),
                level: 0,
            }],
            port_shares: Vec::new(),
            hosts: Vec::new(),
        }
    }

    /// The generated Xray `config.json` inbound must carry the reference network &
    /// TLS/Reality parameters 1:1 (plus the legacy `dest` alias of `target`).
    #[test]
    fn generated_inbound_matches_remnawave_reference() {
        let cfg = generate_config_with_api_port(&[reference_inbound()], &[], 10085).unwrap();
        let ib = &cfg["inbounds"][0];

        assert_eq!(ib["tag"], json!("VLESS_XHTTP_REALITY"));
        assert_eq!(ib["port"], json!(443));
        assert_eq!(ib["listen"], json!("0.0.0.0"));
        assert_eq!(ib["protocol"], json!("vless"));
        assert_eq!(ib["settings"]["decryption"], json!("none"));

        // Clients are substituted from `InboundConfig.users` and never wiped.
        assert_eq!(ib["settings"]["clients"].as_array().unwrap().len(), 1);
        assert_eq!(
            ib["settings"]["clients"][0],
            json!({
                "id": "3a8a4cc5-c756-41a3-9961-3789079074a7",
                "flow": "",
                "email": "finland-user",
                "level": 0
            })
        );

        let ss = &ib["streamSettings"];
        assert_eq!(ss["network"], json!("xhttp"));
        assert_eq!(ss["security"], json!("reality"));
        // xhttpSettings identical to the reference (no synthetic `extra`).
        assert_eq!(
            ss["xhttpSettings"],
            json!({ "host": "www.twitch.tv", "mode": "packet-up", "path": "/" })
        );
        // realitySettings: reference keys verbatim + legacy `dest` alias of `target`.
        assert_eq!(
            ss["realitySettings"],
            json!({
                "show": false,
                "xver": 0,
                "target": "www.twitch.tv:443",
                "dest": "www.twitch.tv:443",
                "shortIds": [""],
                "privateKey": "L-y1ZTWHuRgg09Bv0JJOYqQkly9jMcsmD7D97O2093o",
                "publicKey": "YSia7kbg9eXUatxAQ5e2nbmKkFNgormAPs27SsWeJXg",
                "fingerprint": "firefox",
                "serverNames": ["www.twitch.tv"]
            })
        );
        // sockopt passes through untouched (tcpWindowClamp, tcpFastOpen, ...).
        assert_eq!(ss["sockopt"], reference_stream()["sockopt"]);

        // sniffing matches the reference destOverride set.
        assert_eq!(ib["sniffing"]["enabled"], json!(true));
        assert_eq!(
            ib["sniffing"]["destOverride"],
            json!(["http", "tls", "quic", "fakedns"])
        );

        // Outbound order: `freedom` first = default outbound when no rule matches.
        assert_eq!(cfg["outbounds"][0]["tag"], json!("direct"));
        assert_eq!(cfg["outbounds"][0]["protocol"], json!("freedom"));
        assert_eq!(cfg["outbounds"][1]["protocol"], json!("blackhole"));

        println!(
            "=== generated node config inbounds[0] ===\n{}",
            serde_json::to_string_pretty(ib).unwrap()
        );
    }

    /// An inbound without eligible users (e.g. port-share only) must render an
    /// empty `clients` array instead of failing the whole node config.
    #[test]
    fn empty_clients_render_empty_array_instead_of_error() {
        let mut inbound = reference_inbound();
        inbound.users.clear();
        inbound.settings = json!({ "decryption": "none", "clients": [] });
        let cfg = generate_config_with_api_port(&[inbound], &[], 10085).unwrap();
        assert_eq!(cfg["inbounds"][0]["settings"]["clients"], json!([]));
    }
}
