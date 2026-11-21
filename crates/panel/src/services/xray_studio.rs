use crate::api::inbounds::InboundDto;
use crate::error::{AppError, AppResult};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::Utc;
use mypanel_proto::{Protocol, RoutingRuleConfig};
use rand::{rngs::OsRng, RngCore};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::collections::HashSet;
use std::time::Duration;
use tokio::process::Command;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationResult {
    pub valid: bool,
    pub timestamp: String,
    pub message: String,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub engine: String,
    pub xray_version: Option<String>,
}

fn now_hms() -> String {
    Utc::now().format("%H:%M:%S").to_string()
}

pub fn parse_x25519_output(raw: &str) -> Option<(String, String, Option<String>)> {
    let mut priv_key = None;
    let mut pub_key = None;
    let mut hash32 = None;
    for line in raw.lines() {
        let trimmed = line.trim();
        if let Some((k, v)) = trimmed.split_once(':') {
            let key = k.trim().to_ascii_lowercase();
            let val = v.trim().to_string();
            if val.is_empty() {
                continue;
            }
            if key.contains("private") {
                priv_key = Some(val);
            } else if key.contains("public") || key.contains("password") {
                pub_key = Some(val);
            } else if key.contains("hash32") {
                hash32 = Some(val);
            }
        }
    }
    Some((priv_key?, pub_key?, hash32))
}

pub fn parse_mldsa65_output(raw: &str) -> Option<(String, String)> {
    let mut seed = None;
    let mut verify = None;
    for line in raw.lines() {
        let trimmed = line.trim();
        if let Some((k, v)) = trimmed.split_once(':') {
            let key = k.trim().to_ascii_lowercase();
            let val = v.trim().to_string();
            if val.is_empty() {
                continue;
            }
            if key == "seed" {
                seed = Some(val);
            } else if key == "verify" {
                verify = Some(val);
            }
        }
    }
    Some((seed?, verify?))
}

pub fn parse_vlessenc_output(raw: &str) -> Value {
    let mut section = "";
    let mut x25519_dec: Option<String> = None;
    let mut x25519_enc: Option<String> = None;
    let mut mlkem_dec: Option<String> = None;
    let mut mlkem_enc: Option<String> = None;

    for line in raw.lines() {
        let t = line.trim();
        let lower = t.to_ascii_lowercase();
        if lower.contains("authentication: x25519") {
            section = "x25519";
            continue;
        }
        if lower.contains("authentication: ml-kem-768") || lower.contains("authentication: mlkem768") {
            section = "mlkem768";
            continue;
        }
        if let Some(rest) = t.strip_prefix("\"decryption\"") {
            if let Some((_, rhs)) = rest.split_once(':') {
                let val = rhs.trim().trim_matches('"').to_string();
                match section {
                    "x25519" => x25519_dec = Some(val),
                    "mlkem768" => mlkem_dec = Some(val),
                    _ => {}
                }
            }
        } else if let Some(rest) = t.strip_prefix("\"encryption\"") {
            if let Some((_, rhs)) = rest.split_once(':') {
                let val = rhs.trim().trim_matches('"').to_string();
                match section {
                    "x25519" => x25519_enc = Some(val),
                    "mlkem768" => mlkem_enc = Some(val),
                    _ => {}
                }
            }
        }
    }

    json!({
        "x25519": x25519_dec.zip(x25519_enc).map(|(d, e)| json!({ "decryption": d, "encryption": e })),
        "mlkem768": mlkem_dec.zip(mlkem_enc).map(|(d, e)| json!({ "decryption": d, "encryption": e })),
    })
}

pub async fn find_xray_binary() -> Option<String> {
    for candidate in ["/usr/local/bin/xray", "/usr/bin/xray", "xray"] {
        if candidate.starts_with('/') && !tokio::fs::try_exists(candidate).await.unwrap_or(false) {
            continue;
        }
        if let Ok(Ok(out)) = tokio::time::timeout(
            Duration::from_secs(2),
            Command::new(candidate).arg("version").output(),
        )
        .await
        {
            if out.status.success() {
                return Some(candidate.to_string());
            }
        }
    }
    None
}

pub async fn generate_protocol_keys(kind: &str, method: Option<&str>) -> AppResult<Value> {
    match kind.trim().to_ascii_lowercase().as_str() {
        "uuid" => Ok(json!({ "uuid": uuid::Uuid::new_v4().to_string() })),
        "shortid" | "short_id" => {
            let mut buf = [0u8; 8];
            OsRng.fill_bytes(&mut buf);
            Ok(json!({ "shortId": hex::encode(buf) }))
        }
        "ss2022" | "ss-2022" => {
            let is_128 = method.is_some_and(|m| m.trim() == "2022-blake3-aes-128-gcm");
            let mut key = vec![0u8; if is_128 { 16 } else { 32 }];
            OsRng.fill_bytes(&mut key);
            Ok(json!({ "key": STANDARD.encode(&key), "bytes": key.len() }))
        }
        "reality" | "x25519" => {
            let mut sid = [0u8; 8];
            OsRng.fill_bytes(&mut sid);
            let short_id = hex::encode(sid);
            if let Some(bin) = find_xray_binary().await {
                let out = tokio::time::timeout(
                    Duration::from_secs(5),
                    Command::new(&bin).arg("x25519").output(),
                )
                .await
                .map_err(|_| AppError::service_unavailable("xray x25519 timed out"))?
                .map_err(|e| AppError::service_unavailable(format!("xray x25519 failed: {e}")))?;
                let txt = format!(
                    "{}\n{}",
                    String::from_utf8_lossy(&out.stdout),
                    String::from_utf8_lossy(&out.stderr)
                );
                if let Some((priv_k, pub_k, hash32)) = parse_x25519_output(&txt) {
                    return Ok(json!({
                        "privateKey": priv_k,
                        "publicKey": pub_k,
                        "hash32": hash32,
                        "shortId": short_id,
                        "source": "xray-cli"
                    }));
                }
            }
            Err(AppError::service_unavailable(
                "Xray binary is not installed on panel host; client-side fallback available in browser",
            ))
        }
        "mldsa65" => {
            let bin = find_xray_binary().await.ok_or_else(|| {
                AppError::service_unavailable("Xray 25+ binary is required to generate ML-DSA-65 keys")
            })?;
            let out = tokio::time::timeout(
                Duration::from_secs(5),
                Command::new(&bin).arg("mldsa65").output(),
            )
            .await
            .map_err(|_| AppError::service_unavailable("xray mldsa65 timed out"))?
            .map_err(|e| AppError::service_unavailable(format!("xray mldsa65 failed: {e}")))?;
            let txt = String::from_utf8_lossy(&out.stdout);
            let (seed, verify) = parse_mldsa65_output(&txt).ok_or_else(|| {
                AppError::service_unavailable("Unexpected xray mldsa65 output format")
            })?;
            Ok(json!({ "seed": seed, "verify": verify, "source": "xray-cli" }))
        }
        "vlessenc" => {
            let bin = find_xray_binary().await.ok_or_else(|| {
                AppError::service_unavailable("Xray 25+ binary is required to generate VLESS Encryption PQ pairs")
            })?;
            let out = tokio::time::timeout(
                Duration::from_secs(5),
                Command::new(&bin).arg("vlessenc").output(),
            )
            .await
            .map_err(|_| AppError::service_unavailable("xray vlessenc timed out"))?
            .map_err(|e| AppError::service_unavailable(format!("xray vlessenc failed: {e}")))?;
            let txt = String::from_utf8_lossy(&out.stdout);
            let mut parsed = parse_vlessenc_output(&txt);
            if let Some(obj) = parsed.as_object_mut() {
                obj.insert("source".into(), json!("xray-cli"));
            }
            Ok(parsed)
        }
        other => Err(AppError::validation(
            "kind",
            format!("Unsupported keygen kind: {other}"),
        )),
    }
}

pub fn validate_xray_config_value(config: &Value) -> ValidationResult {
    let ts = now_hms();
    let mut errors: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    let Some(root) = config.as_object() else {
        return ValidationResult {
            valid: false,
            timestamp: ts,
            message: "Configuration must be a JSON object.".into(),
            errors: vec!["Root value must be a JSON object".into()],
            warnings,
            engine: "schema-validator".into(),
            xray_version: None,
        };
    };

    if let Some(inbounds) = root.get("inbounds") {
        let Some(arr) = inbounds.as_array() else {
            errors.push("'inbounds' must be an array".into());
            return ValidationResult {
                valid: false,
                timestamp: ts,
                message: errors[0].clone(),
                errors,
                warnings,
                engine: "schema-validator".into(),
                xray_version: None,
            };
        };
        if arr.is_empty() {
            warnings.push("'inbounds' array is empty".into());
        }
        let mut seen_tags = HashSet::new();
        for (idx, ib) in arr.iter().enumerate() {
            validate_single_inbound(ib, Some(idx), Some(&mut seen_tags), &mut errors, &mut warnings);
        }
    } else if root.contains_key("protocol") || root.contains_key("streamSettings") || root.contains_key("stream") {
        validate_single_inbound(config, None, None, &mut errors, &mut warnings);
    } else {
        errors.push("JSON object must contain an 'inbounds' array or an inbound 'protocol'".into());
    }

    // Validate outbounds & routing tags if present
    let mut outbound_tags: HashSet<String> =
        ["direct", "block", "proxy", "api", "fragment"]
            .into_iter()
            .map(ToString::to_string)
            .collect();

    if let Some(outs) = root.get("outbounds").and_then(Value::as_array) {
        for (idx, ob) in outs.iter().enumerate() {
            if let Some(tag) = ob.get("tag").and_then(Value::as_str).map(str::trim) {
                if !tag.is_empty() {
                    outbound_tags.insert(tag.to_string());
                }
            } else {
                warnings.push(format!("outbounds[{idx}] has no 'tag'"));
            }
            if ob.get("protocol").and_then(Value::as_str).unwrap_or("").is_empty() {
                errors.push(format!("outbounds[{idx}] is missing 'protocol'"));
            }
        }
    }

    if let Some(rules) = config.pointer("/routing/rules").and_then(Value::as_array) {
        for (idx, rule) in rules.iter().enumerate() {
            if let Some(otag) = rule.get("outboundTag").and_then(Value::as_str).map(str::trim) {
                if !otag.is_empty() && !outbound_tags.contains(otag) {
                    warnings.push(format!("routing.rules[{idx}] references unknown outboundTag '{otag}'"));
                }
            }
        }
    }

    let valid = errors.is_empty();
    let message = if valid {
        "Xray config is valid.".into()
    } else {
        errors.first().cloned().unwrap_or_else(|| "Validation failed".into())
    };

    ValidationResult {
        valid,
        timestamp: ts,
        message,
        errors,
        warnings,
        engine: "schema-validator".into(),
        xray_version: None,
    }
}

fn validate_single_inbound(
    ib: &Value,
    idx: Option<usize>,
    mut seen_tags: Option<&mut HashSet<String>>,
    errors: &mut Vec<String>,
    warnings: &mut Vec<String>,
) {
    let prefix = idx.map(|i| format!("inbounds[{i}]")).unwrap_or_else(|| "inbound".into());
    let Some(obj) = ib.as_object() else {
        errors.push(format!("{prefix}: must be a JSON object"));
        return;
    };

    if let Some(tag) = obj.get("tag").and_then(Value::as_str).map(str::trim) {
        if tag.is_empty() {
            errors.push(format!("{prefix}: 'tag' cannot be empty"));
        } else if let Some(ref mut set) = seen_tags {
            if !set.insert(tag.to_string()) {
                errors.push(format!("{prefix}: duplicate tag '{tag}'"));
            }
        }
    } else if idx.is_some() {
        errors.push(format!("{prefix}: missing 'tag'"));
    }

    if let Some(port) = obj.get("port").and_then(Value::as_i64) {
        if !(1..=65535).contains(&port) {
            errors.push(format!("{prefix}: port {port} must be 1..=65535"));
        }
    }

    let proto = obj
        .get("protocol")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    if proto.is_empty() {
        errors.push(format!("{prefix}: missing 'protocol'"));
    }

    let settings = obj.get("settings").unwrap_or(&Value::Null);
    let stream = obj
        .get("streamSettings")
        .or_else(|| obj.get("stream"))
        .unwrap_or(&Value::Null);

    let net = stream
        .get("network")
        .and_then(Value::as_str)
        .unwrap_or("tcp")
        .to_ascii_lowercase();
    let sec = stream
        .get("security")
        .and_then(Value::as_str)
        .unwrap_or("none")
        .to_ascii_lowercase();

    if !matches!(
        net.as_str(),
        "tcp" | "raw" | "ws" | "grpc" | "httpupgrade" | "xhttp" | "h2" | "http" | "kcp" | "splithttp"
    ) && matches!(proto.as_str(), "vless" | "vmess" | "trojan")
    {
        warnings.push(format!("{prefix}: unusual Xray transport network '{net}'"));
    }

    if !matches!(sec.as_str(), "none" | "tls" | "reality" | "") {
        errors.push(format!("{prefix}: unsupported security '{sec}'"));
    }

    if sec == "reality" && !matches!(net.as_str(), "tcp" | "raw" | "grpc" | "xhttp" | "h2" | "http") {
        errors.push(format!("{prefix}: Reality requires tcp/raw, xhttp, grpc, or h2 transport"));
    }

    if proto == "vless" {
        let mut flow_str = settings.get("flow").and_then(Value::as_str).unwrap_or("");
        if flow_str.is_empty() {
            if let Some(first_flow) = settings
                .pointer("/clients/0/flow")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            {
                flow_str = first_flow;
            }
        }
        if !flow_str.is_empty() {
            if !matches!(flow_str, "xtls-rprx-vision" | "xtls-rprx-vision-udp443") {
                errors.push(format!("{prefix}: unsupported VLESS flow '{flow_str}'"));
            } else if !matches!(net.as_str(), "tcp" | "raw") || !matches!(sec.as_str(), "tls" | "reality") {
                errors.push(format!("{prefix}: VLESS Vision flow requires TCP/raw + TLS or Reality"));
            }
        }
        if sec == "reality" {
            let r = stream.get("realitySettings").unwrap_or(&Value::Null);
            let priv_k = r.get("privateKey").and_then(Value::as_str).unwrap_or("");
            if priv_k.trim().is_empty() {
                warnings.push(format!("{prefix}: realitySettings.privateKey is empty"));
            }
            let target = r
                .get("target")
                .or_else(|| r.get("dest"))
                .and_then(Value::as_str)
                .unwrap_or("");
            if target.trim().is_empty() {
                warnings.push(format!("{prefix}: realitySettings.target/dest is empty"));
            }
        }
    }

    if net == "xhttp" {
        if let Some(mode) = stream.pointer("/xhttpSettings/mode").and_then(Value::as_str) {
            if !mode.is_empty() && !matches!(mode, "auto" | "packet-up" | "stream-up" | "stream-one") {
                errors.push(format!("{prefix}: invalid xhttp mode '{mode}'"));
            }
        }
    }

    if let Some(dest_ov) = ib.pointer("/sniffing/destOverride").and_then(Value::as_array) {
        for item in dest_ov {
            if let Some(s) = item.as_str() {
                if !matches!(s, "http" | "tls" | "quic" | "fakedns" | "fakedns+others") {
                    errors.push(format!("{prefix}: invalid sniffing.destOverride token '{s}'"));
                }
            }
        }
    }
}

pub async fn validate_xray_config_live(config: &Value) -> ValidationResult {
    let mut res = validate_xray_config_value(config);
    if !res.valid {
        return res;
    }

    let is_full_xray = config.get("inbounds").and_then(Value::as_array).is_some();
    if !is_full_xray {
        return res;
    }

    let Some(bin) = find_xray_binary().await else {
        return res;
    };

    let xray_ver = tokio::time::timeout(
        Duration::from_secs(2),
        Command::new(&bin).arg("version").output(),
    )
    .await
    .ok()
    .and_then(Result::ok)
    .and_then(|o| {
        String::from_utf8_lossy(&o.stdout)
            .lines()
            .next()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
    });

    let mut cfg = config.clone();
    if let Some(obj) = cfg.as_object_mut() {
        if let Some(log_obj) = obj.get_mut("log").and_then(Value::as_object_mut) {
            log_obj.remove("error");
            log_obj.remove("access");
        }
        if !obj.contains_key("outbounds") {
            obj.insert(
                "outbounds".into(),
                json!([
                    { "tag": "direct", "protocol": "freedom" },
                    { "tag": "block", "protocol": "blackhole" }
                ]),
            );
        }
    }

    let tmp_res = tempfile::Builder::new()
        .prefix("mypanel-xray-val-")
        .suffix(".json")
        .tempfile();
    let Ok(tmp_file) = tmp_res else {
        return res;
    };
    let Ok(bytes) = serde_json::to_vec_pretty(&cfg) else {
        return res;
    };
    if tokio::fs::write(tmp_file.path(), &bytes).await.is_err() {
        return res;
    }

    let out_res = tokio::time::timeout(
        Duration::from_secs(5),
        Command::new(&bin)
            .args(["run", "-test", "-c"])
            .arg(tmp_file.path())
            .output(),
    )
    .await;

    res.engine = "xray-cli".into();
    res.xray_version = xray_ver;
    res.timestamp = now_hms();

    match out_res {
        Ok(Ok(out)) if out.status.success() => {
            res.valid = true;
            res.message = "Xray config is valid.".into();
        }
        Ok(Ok(out)) => {
            let combined = format!(
                "{}\n{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            let lines: Vec<String> = combined
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with("Xray "))
                .map(ToString::to_string)
                .collect();
            res.valid = false;
            res.errors = if lines.is_empty() {
                vec![format!("xray run -test exited with {}", out.status)]
            } else {
                lines
            };
            res.message = res.errors[0].clone();
        }
        _ => {}
    }

    res
}

pub fn build_studio_xray_config(
    inbounds: &[InboundDto],
    rules: &[RoutingRuleConfig],
    saved_profile: Option<&Value>,
) -> Value {
    let profile_map = saved_profile.and_then(Value::as_object);
    let mut ib_list = Vec::new();

    for ib in inbounds {
        if !matches!(
            ib.protocol,
            Protocol::Vless | Protocol::Vmess | Protocol::Trojan | Protocol::Shadowsocks
        ) {
            continue;
        }
        let ov = profile_map
            .and_then(|pm| pm.get("inboundOverrides"))
            .and_then(|m| m.get(&ib.tag))
            .and_then(Value::as_object);

        let mut stream_obj = ib.stream.as_object().cloned().unwrap_or_default();
        if let Some(ov_so) = ov.and_then(|o| o.get("sockopt")).filter(|v| v.is_object()) {
            stream_obj.insert("sockopt".into(), ov_so.clone());
        } else if !stream_obj.contains_key("sockopt") {
            stream_obj.insert(
                "sockopt".into(),
                json!({
                    "mark": 0,
                    "V6Only": false,
                    "tproxy": "off",
                    "tcpMptcp": false,
                    "interface": ""
                }),
            );
        }

        let mut settings_obj = ib.settings.as_object().cloned().unwrap_or_default();
        settings_obj.entry("clients").or_insert_with(|| json!([]));
        if ib.protocol == Protocol::Vless {
            let dec = settings_obj
                .get("decryption")
                .or_else(|| settings_obj.get("vlessDecryption"))
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .unwrap_or("none")
                .to_string();
            settings_obj.insert("decryption".into(), json!(dec));
        }

        let sniffing_val = ov
            .and_then(|o| o.get("sniffing"))
            .filter(|v| v.is_object())
            .cloned()
            .or_else(|| ib.settings.get("sniffing").filter(|v| v.is_object()).cloned())
            .unwrap_or_else(|| {
                json!({
                    "enabled": ib.sniffing,
                    "destOverride": ["http", "tls", "quic", "fakedns"],
                    "routeOnly": true
                })
            });

        let mut ib_val = Map::new();
        ib_val.insert("tag".into(), json!(ib.tag));
        ib_val.insert("port".into(), json!(ib.port));
        ib_val.insert("listen".into(), json!(ib.listen));
        ib_val.insert("protocol".into(), json!(ib.protocol.as_str().to_ascii_lowercase()));
        ib_val.insert("settings".into(), Value::Object(settings_obj));
        ib_val.insert("sniffing".into(), sniffing_val);
        ib_val.insert("streamSettings".into(), Value::Object(stream_obj));
        if let Some(alloc) = ov
            .and_then(|o| o.get("allocate"))
            .or_else(|| ib.settings.get("allocate"))
            .filter(|v| v.is_object())
        {
            ib_val.insert("allocate".into(), alloc.clone());
        }
        ib_list.push(Value::Object(ib_val));
    }

    let log_val = profile_map
        .and_then(|pm| pm.get("log"))
        .filter(|v| v.is_object())
        .cloned()
        .unwrap_or_else(|| json!({ "loglevel": "warning" }));

    let outbounds_val = profile_map
        .and_then(|pm| pm.get("outbounds"))
        .filter(|v| v.is_array())
        .cloned()
        .unwrap_or_else(|| {
            json!([
                { "tag": "direct", "protocol": "freedom" },
                { "tag": "block", "protocol": "blackhole" }
            ])
        });

    let mut rule_vals = Vec::new();
    for r in rules {
        let mut entry = Map::new();
        entry.insert("type".into(), json!(r.rule_type));
        if !r.domain.is_empty() {
            entry.insert("domain".into(), json!(r.domain));
        }
        if !r.ip.is_empty() {
            entry.insert("ip".into(), json!(r.ip));
        }
        if let Some(ref p) = r.port {
            entry.insert("port".into(), json!(p));
        }
        if !r.protocol.is_empty() {
            entry.insert("protocol".into(), json!(r.protocol));
        }
        if !r.outbound_tag.is_empty() {
            entry.insert("outboundTag".into(), json!(r.outbound_tag));
        }
        rule_vals.push(Value::Object(entry));
    }
    let routing_val = profile_map
        .and_then(|pm| pm.get("routing"))
        .filter(|v| v.is_object())
        .cloned()
        .unwrap_or_else(|| {
            json!({
                "domainStrategy": "IPIfNonMatch",
                "rules": rule_vals
            })
        });

    let mut root = Map::new();
    root.insert("log".into(), log_val);
    if let Some(dns) = profile_map.and_then(|pm| pm.get("dns")).filter(|v| v.is_object()) {
        root.insert("dns".into(), dns.clone());
    }
    if let Some(fakedns) = profile_map
        .and_then(|pm| pm.get("fakedns"))
        .filter(|v| v.is_object() || v.is_array())
    {
        root.insert("fakedns".into(), fakedns.clone());
    }
    root.insert("inbounds".into(), Value::Array(ib_list));
    root.insert("outbounds".into(), outbounds_val);
    root.insert("routing".into(), routing_val);
    if let Some(pol) = profile_map.and_then(|pm| pm.get("policy")).filter(|v| v.is_object()) {
        root.insert("policy".into(), pol.clone());
    }
    Value::Object(root)
}
