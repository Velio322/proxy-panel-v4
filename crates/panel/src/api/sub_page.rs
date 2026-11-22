use crate::services::telegram::esc;
use aes_gcm::{aead::Aead, Aes256Gcm, KeyInit, Nonce};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::NaiveDateTime;
use mypanel_proto::links::{encode_uri_component, share_link, SubEntry};
use mypanel_proto::subscription::{self, UserInfo};
use qrcode::{render::svg, EcLevel, QrCode};
use rand::{rngs::OsRng, RngCore};
use serde_json::Value;
use std::collections::BTreeMap;

/// Public K1 key from `@incy/link-encoder` v1.3.0 (`incy://crypt1/` obfuscation scheme;
/// SHA-256 fingerprint verified by unit test `incy_k1_fingerprint_matches_official_spec`).
const INCY_K1: [u8; 32] = [
    0xf6, 0xd4, 0x0e, 0xa0, 0xc8, 0xa8, 0x89, 0x9d, 0x7c, 0x68, 0x2d, 0x09, 0xba, 0x0d, 0x41, 0x65,
    0xdf, 0xe2, 0xb3, 0xdd, 0x45, 0xe6, 0xbb, 0x3e, 0x25, 0xcb, 0x23, 0x3c, 0xf0, 0x0c, 0x24, 0x62,
];

/// Encodes a subscription URL into `incy://crypt1/<payload>` compatible with INCY iOS/Android/Desktop.
pub fn encode_incy_crypt1(url: &str, name: Option<&str>) -> Option<String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut payload_map: BTreeMap<&'static str, Value> = BTreeMap::new();
    if let Some(n) = name.map(str::trim).filter(|s| !s.is_empty()) {
        let truncated: String = n.chars().take(128).collect();
        payload_map.insert("n", Value::String(truncated));
    }
    payload_map.insert("url", Value::String(trimmed.to_string()));
    payload_map.insert("v", Value::Number(1.into()));

    let plaintext = serde_json::to_string(&payload_map).ok()?;
    let cipher = Aes256Gcm::new_from_slice(&INCY_K1).ok()?;
    let mut iv = [0u8; 12];
    OsRng.fill_bytes(&mut iv);
    let ct = cipher
        .encrypt(Nonce::from_slice(&iv), plaintext.as_bytes())
        .ok()?;

    let mut wire = Vec::with_capacity(iv.len() + ct.len());
    wire.extend_from_slice(&iv);
    wire.extend_from_slice(&ct);
    Some(format!("incy://crypt1/{}", URL_SAFE_NO_PAD.encode(&wire)))
}

/// Renders an inline crisp SVG QR code for the provided payload.
pub fn render_qr_svg(data: &str, size_px: u32) -> String {
    let Ok(code) = QrCode::with_error_correction_level(data.as_bytes(), EcLevel::M) else {
        return String::new();
    };
    code.render::<svg::Color<'_>>()
        .min_dimensions(size_px, size_px)
        .dark_color(svg::Color("#0b1120"))
        .light_color(svg::Color("#ffffff"))
        .quiet_zone(true)
        .build()
}

pub fn format_human_bytes(bytes: i64) -> String {
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

/// Renders the standalone browser subscription landing page in ProxyPanel v4 visual style,
/// featuring visually separated Mobile (iPhone/Android) and Desktop (Windows/macOS/Linux)
/// app quick-connect cards, inline SVG QR codes, and per-node config drawers.
pub fn render_landing_html(
    username: &str,
    sub_base_url: &str,
    sub_token: &str,
    used_traffic: i64,
    traffic_limit: i64,
    expire_at: Option<NaiveDateTime>,
    entries: &[SubEntry],
    info: &UserInfo,
) -> String {
    let base = sub_base_url.trim_end_matches('/');
    let sub_link = format!("{base}/sub/{sub_token}");
    let encoded_sub = encode_uri_component(&sub_link);
    let encoded_name = encode_uri_component(username);

    let happ_deep_link = format!("happ://add/{sub_link}");
    let incy_deep_link =
        encode_incy_crypt1(&sub_link, Some(username)).unwrap_or_else(|| sub_link.clone());
    let karing_deep_link =
        format!("karing://install-config?url={encoded_sub}&name={encoded_name}");
    let nekobox_deep_link = format!("sn://subscription?url={encoded_sub}&name={encoded_name}");
    let throne_deep_link = format!("throne://addsub/?url={encoded_sub}&name={encoded_name}");

    let main_qr_svg = render_qr_svg(&sub_link, 168);

    let traffic_pct: u32 = if traffic_limit > 0 {
        ((used_traffic.max(0) as f64 / traffic_limit as f64) * 100.0).clamp(0.0, 100.0) as u32
    } else {
        0
    };
    let traffic_str = if traffic_limit > 0 {
        format!(
            "{} / {}",
            format_human_bytes(used_traffic),
            format_human_bytes(traffic_limit)
        )
    } else {
        format!("{} / ∞", format_human_bytes(used_traffic))
    };
    let expire_str = expire_at
        .map(|d| d.format("%d.%m.%Y").to_string())
        .unwrap_or_else(|| "Бессрочно".to_string());
    let upload_str = format_human_bytes(i64::try_from(info.upload).unwrap_or(i64::MAX));
    let download_str = format_human_bytes(i64::try_from(info.download).unwrap_or(i64::MAX));

    let parsed_entries = subscription::render_json(entries, info)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .and_then(|v| v.get("entries").and_then(Value::as_array).cloned())
        .unwrap_or_default();

    let node_count = if !parsed_entries.is_empty() {
        parsed_entries.len()
    } else {
        entries.len()
    };

    let mut cards_html = String::new();
    if parsed_entries.is_empty() {
        cards_html.push_str(
            "<div class=\"blade empty-state\">\
               <p data-i18n=\"no_nodes\">Нет активных серверов для этой подписки.</p>\
             </div>",
        );
    } else {
        for (idx, item) in parsed_entries.iter().enumerate() {
            let proto = item.get("protocol").and_then(Value::as_str).unwrap_or("PROXY");
            let tag = item.get("tag").and_then(Value::as_str).unwrap_or("");
            let host = item.get("host").and_then(Value::as_str).unwrap_or("");
            let port = item.get("port").and_then(Value::as_u64).unwrap_or(0);
            let link = item
                .get("link")
                .and_then(Value::as_str)
                .map(ToString::to_string)
                .or_else(|| entries.get(idx).and_then(|e| share_link(e).ok()))
                .unwrap_or_default();

            let badge_cls = match proto {
                "VLESS" => "badge-vless",
                "HYSTERIA2" => "badge-hy2",
                "NAIVEPROXY" => "badge-naive",
                "MIERU" => "badge-mieru",
                "TROJAN" => "badge-trojan",
                _ => "badge-default",
            };

            let node_qr_svg = render_qr_svg(&link, 148);
            let cfg_block = match item.get("config").filter(|c| !c.is_null()) {
                Some(cfg) => {
                    let pretty = serde_json::to_string_pretty(cfg).unwrap_or_default();
                    format!(
                        "<details class=\"drawer\">\
                           <summary data-i18n=\"native_json\">Нативный JSON конфиг</summary>\
                           <pre>{}</pre>\
                         </details>",
                        esc(&pretty)
                    )
                }
                None => String::new(),
            };

            cards_html.push_str(&format!(
                "<article class=\"blade server-card\">\
                   <div class=\"server-head\">\
                     <div class=\"server-title\">\
                       <span class=\"badge {badge_cls}\">{proto_esc}</span>\
                       <strong class=\"server-name\">{tag_esc}</strong>\
                     </div>\
                     <span class=\"mono endpoint\">{host_esc}:{port}</span>\
                   </div>\
                   <div class=\"copy-row\">\
                     <input type=\"text\" readonly class=\"mono key-input\" value=\"{link_esc}\" aria-label=\"Share link for {tag_esc}\">\
                     <button type=\"button\" class=\"btn btn-outline copy-btn\" data-copy=\"{link_esc}\">\
                       <span class=\"btn-icon\" aria-hidden=\"true\">&#x29C9;</span>\
                       <span data-i18n=\"copy\">Копировать</span>\
                     </button>\
                   </div>\
                   <div class=\"drawer-row\">\
                     <details class=\"drawer\">\
                       <summary data-i18n=\"show_qr\">Показать QR-код</summary>\
                       <div class=\"qr-box qr-sm\">{node_qr_svg}</div>\
                     </details>\
                     {cfg_block}\
                   </div>\
                 </article>",
                badge_cls = badge_cls,
                proto_esc = esc(proto),
                tag_esc = esc(tag),
                host_esc = esc(host),
                port = port,
                link_esc = esc(&link),
                node_qr_svg = node_qr_svg,
                cfg_block = cfg_block,
            ));
        }
    }

    // Inline SVG brand glyphs for client applications
    let make_icon_happ = |gid: &str| {
        format!(
            "<svg viewBox=\"0 0 36 36\" fill=\"none\" aria-hidden=\"true\">\
                <rect x=\"2\" y=\"2\" width=\"32\" height=\"32\" rx=\"9\" fill=\"url(#{gid})\"/>\
                <path d=\"M11 11v14M25 11v14M11 18h14\" stroke=\"#fff\" stroke-width=\"3\" stroke-linecap=\"round\"/>\
                <defs><linearGradient id=\"{gid}\" x1=\"2\" y1=\"2\" x2=\"34\" y2=\"34\"><stop stop-color=\"#10b981\"/><stop offset=\"1\" stop-color=\"#047857\"/></linearGradient></defs>\
            </svg>"
        )
    };
    let icon_happ = make_icon_happ("gHappMob");
    let icon_happ_pc = make_icon_happ("gHappPc");

    let icon_incy = "<svg viewBox=\"0 0 36 36\" fill=\"none\" aria-hidden=\"true\">\
        <rect x=\"2\" y=\"2\" width=\"32\" height=\"32\" rx=\"9\" fill=\"url(#gIncy)\"/>\
        <circle cx=\"18\" cy=\"18\" r=\"7\" stroke=\"#fff\" stroke-width=\"2.6\"/>\
        <path d=\"M18 11v14M13.5 15.5L22.5 20.5\" stroke=\"#fff\" stroke-width=\"2.4\" stroke-linecap=\"round\"/>\
        <defs><linearGradient id=\"gIncy\" x1=\"2\" y1=\"2\" x2=\"34\" y2=\"34\"><stop stop-color=\"#38bdf8\"/><stop offset=\"1\" stop-color=\"#2563eb\"/></linearGradient></defs>\
    </svg>";

    let icon_karing = "<svg viewBox=\"0 0 36 36\" fill=\"none\" aria-hidden=\"true\">\
        <rect x=\"2\" y=\"2\" width=\"32\" height=\"32\" rx=\"9\" fill=\"url(#gKaring)\"/>\
        <path d=\"M12 10v16M12 18l10-7M14 18l10 8\" stroke=\"#fff\" stroke-width=\"2.8\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>\
        <defs><linearGradient id=\"gKaring\" x1=\"2\" y1=\"2\" x2=\"34\" y2=\"34\"><stop stop-color=\"#a855f7\"/><stop offset=\"1\" stop-color=\"#6366f1\"/></linearGradient></defs>\
    </svg>";

    let icon_nekobox = "<svg viewBox=\"0 0 36 36\" fill=\"none\" aria-hidden=\"true\">\
        <rect x=\"2\" y=\"2\" width=\"32\" height=\"32\" rx=\"9\" fill=\"url(#gNeko)\"/>\
        <path d=\"M10 14l4-4 4 3 4-3 4 4v9a3 3 0 0 1-3 3H13a3 3 0 0 1-3-3v-9z\" stroke=\"#fff\" stroke-width=\"2.3\" stroke-linejoin=\"round\"/>\
        <circle cx=\"15\" cy=\"19\" r=\"1.4\" fill=\"#fff\"/><circle cx=\"21\" cy=\"19\" r=\"1.4\" fill=\"#fff\"/>\
        <defs><linearGradient id=\"gNeko\" x1=\"2\" y1=\"2\" x2=\"34\" y2=\"34\"><stop stop-color=\"#f59e0b\"/><stop offset=\"1\" stop-color=\"#ea580c\"/></linearGradient></defs>\
    </svg>";

    let icon_throne = "<svg viewBox=\"0 0 36 36\" fill=\"none\" aria-hidden=\"true\">\
        <rect x=\"2\" y=\"2\" width=\"32\" height=\"32\" rx=\"9\" fill=\"url(#gThrone)\"/>\
        <path d=\"M9 24h18M10 21l2-9 6 4 6-4 2 9H10z\" stroke=\"#fff\" stroke-width=\"2.4\" stroke-linejoin=\"round\" stroke-linecap=\"round\"/>\
        <defs><linearGradient id=\"gThrone\" x1=\"2\" y1=\"2\" x2=\"34\" y2=\"34\"><stop stop-color=\"#14b8a6\"/><stop offset=\"1\" stop-color=\"#0f766e\"/></linearGradient></defs>\
    </svg>";

    let logo_svg = "<svg class=\"brand-mark\" viewBox=\"0 0 32 32\" fill=\"none\" aria-hidden=\"true\">\
        <rect width=\"32\" height=\"32\" rx=\"8\" fill=\"rgba(18,201,148,0.14)\" stroke=\"rgba(18,201,148,0.4)\"/>\
        <path d=\"M16 6l8 3.5v6.2c0 5-3.4 9.1-8 10.3-4.6-1.2-8-5.3-8-10.3V9.5L16 6z\" stroke=\"#12c994\" stroke-width=\"2\" stroke-linejoin=\"round\"/>\
        <circle cx=\"16\" cy=\"15\" r=\"2.4\" fill=\"#12c994\"/>\
    </svg>";

    format!(
        "<!doctype html>\
<html lang=\"ru\" class=\"dark\">\
<head>\
  <meta charset=\"utf-8\">\
  <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
  <title>{user_esc} — ProxyPanel Subscription</title>\
  <style>\
    :root {{\
      --bg: #f4f7fa;\
      --bg-grad: radial-gradient(ellipse 70% 40% at 50% -10%, rgba(16,185,129,0.10), transparent 70%);\
      --card: #ffffff;\
      --card-grad: linear-gradient(180deg, #ffffff 0%, #f8fafc 100%);\
      --inset: #f1f5f9;\
      --fg: #0f172a;\
      --muted: #64748b;\
      --border: #e2e8f0;\
      --border-strong: #cbd5e1;\
      --primary: #0ea574;\
      --primary-soft: rgba(14,165,116,0.12);\
      --primary-glow: rgba(14,165,116,0.22);\
      --shadow: 0 12px 32px -18px rgba(15,23,42,0.16);\
    }}\
    html.dark {{\
      --bg: #07090d;\
      --bg-grad: radial-gradient(ellipse 68% 36% at 50% -8%, rgba(18,201,148,0.12), transparent 70%);\
      --card: #0e1118;\
      --card-grad: linear-gradient(180deg, #11151d 0%, #0b0e14 100%);\
      --inset: #080b10;\
      --fg: #eef2f7;\
      --muted: #8b95a7;\
      --border: #1d222d;\
      --border-strong: #2a3140;\
      --primary: #12c994;\
      --primary-soft: rgba(18,201,148,0.13);\
      --primary-glow: rgba(18,201,148,0.26);\
      --shadow: inset 0 1px 0 0 rgba(255,255,255,0.045), 0 14px 36px -18px rgba(0,0,0,0.85);\
    }}\
    * {{ box-sizing: border-box; }}\
    html {{ font-size: 16px; }}\
    body {{\
      margin: 0;\
      min-height: 100vh;\
      font-family: Inter, -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;\
      background-color: var(--bg);\
      background-image: var(--bg-grad);\
      color: var(--fg);\
      line-height: 1.5;\
      -webkit-font-smoothing: antialiased;\
    }}\
    .mono {{ font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace; }}\
    .shell {{ max-width: 980px; margin: 0 auto; padding: 20px 16px 56px; }}\
    .topbar {{ display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-bottom: 20px; }}\
    .brand {{ display: flex; align-items: center; gap: 10px; font-weight: 700; letter-spacing: -0.02em; font-size: 1.05rem; }}\
    .brand-mark {{ width: 32px; height: 32px; flex-shrink: 0; }}\
    .brand-tag {{ font-size: 0.72rem; font-weight: 600; color: var(--primary); background: var(--primary-soft); border: 1px solid var(--primary-glow); border-radius: 999px; padding: 2px 8px; }}\
    .topbar-controls {{ display: flex; align-items: center; gap: 8px; }}\
    .ctrl-btn {{\
      appearance: none; background: var(--card); color: var(--fg); border: 1px solid var(--border);\
      border-radius: 8px; padding: 6px 10px; font-size: 0.78rem; font-weight: 600; cursor: pointer;\
      transition: border-color .15s, background-color .15s;\
    }}\
    .ctrl-btn:hover {{ border-color: var(--primary); }}\
    .blade {{\
      background: var(--card-grad); border: 1px solid var(--border); border-radius: 16px;\
      box-shadow: var(--shadow); padding: 20px;\
    }}\
    .hero-card {{\
      background-image:\
        radial-gradient(ellipse 55% 80% at 8% 0%, rgba(18,201,148,0.12), transparent 65%),\
        radial-gradient(var(--border) 1px, transparent 1px),\
        var(--card-grad);\
      background-size: 100% 100%, 18px 18px, 100% 100%;\
      margin-bottom: 22px;\
    }}\
    .hero-grid {{ display: grid; grid-template-columns: 1fr auto; gap: 20px; align-items: center; }}\
    @media (max-width: 720px) {{ .hero-grid {{ grid-template-columns: 1fr; }} }}\
    .user-row {{ display: flex; flex-wrap: wrap; align-items: center; gap: 10px; margin-bottom: 12px; }}\
    .user-name {{ margin: 0; font-size: 1.55rem; font-weight: 700; letter-spacing: -0.025em; }}\
    .status-pill {{\
      display: inline-flex; align-items: center; gap: 6px; font-size: 0.76rem; font-weight: 600;\
      color: var(--primary); background: var(--primary-soft); border: 1px solid var(--primary-glow);\
      border-radius: 999px; padding: 3px 10px;\
    }}\
    .dot {{ width: 7px; height: 7px; border-radius: 50%; background: var(--primary); box-shadow: 0 0 8px var(--primary); }}\
    .stats-grid {{ display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 10px; margin-bottom: 14px; }}\
    @media (max-width: 640px) {{ .stats-grid {{ grid-template-columns: repeat(2, minmax(0, 1fr)); }} }}\
    .stat-tile {{ background: var(--inset); border: 1px solid var(--border); border-radius: 10px; padding: 9px 11px; }}\
    .stat-label {{ font-size: 0.72rem; color: var(--muted); display: block; }}\
    .stat-val {{ font-size: 0.92rem; font-weight: 600; margin-top: 2px; display: block; }}\
    .progress-track {{ height: 6px; border-radius: 999px; background: var(--inset); border: 1px solid var(--border); overflow: hidden; margin-bottom: 14px; }}\
    .progress-fill {{ height: 100%; background: linear-gradient(90deg, var(--primary), #34d399); border-radius: 999px; }}\
    .copy-row {{ display: flex; gap: 8px; align-items: stretch; }}\
    @media (max-width: 500px) {{ .copy-row {{ flex-direction: column; }} }}\
    .key-input {{\
      flex: 1; min-width: 0; background: var(--inset); color: var(--fg); border: 1px solid var(--border);\
      border-radius: 10px; padding: 9px 12px; font-size: 0.8rem; outline: none;\
    }}\
    .key-input:focus {{ border-color: var(--primary); }}\
    .btn {{\
      display: inline-flex; align-items: center; justify-content: center; gap: 6px; border-radius: 10px;\
      padding: 9px 14px; font-size: 0.82rem; font-weight: 600; cursor: pointer; text-decoration: none;\
      transition: transform .12s, border-color .15s, background-color .15s; white-space: nowrap;\
    }}\
    .btn:active {{ transform: scale(0.98); }}\
    .btn-primary {{ background: var(--primary); color: #041b13; border: 1px solid var(--primary); }}\
    .btn-primary:hover {{ filter: brightness(1.06); }}\
    .btn-outline {{ background: var(--inset); color: var(--fg); border: 1px solid var(--border-strong); }}\
    .btn-outline:hover {{ border-color: var(--primary); color: var(--primary); }}\
    .qr-panel {{ display: flex; flex-direction: column; align-items: center; gap: 6px; }}\
    .qr-box {{ background: #ffffff; padding: 10px; border-radius: 12px; border: 1px solid var(--border-strong); line-height: 0; box-shadow: 0 8px 20px -12px rgba(0,0,0,0.45); }}\
    .qr-box svg {{ display: block; width: 148px; height: 148px; }}\
    .qr-sm {{ margin-top: 8px; display: inline-block; }}\
    .qr-sm svg {{ width: 136px; height: 136px; }}\
    .qr-caption {{ font-size: 0.72rem; color: var(--muted); }}\
    .section-title {{ display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 10px; margin: 24px 0 12px; }}\
    .section-title h2 {{ margin: 0; font-size: 1.08rem; font-weight: 700; letter-spacing: -0.015em; display: flex; align-items: center; gap: 8px; }}\
    .device-split {{ display: grid; grid-template-columns: 1fr; gap: 18px; margin-bottom: 24px; }}\
    .device-zone {{ position: relative; }}\
    .zone-header {{ display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 10px; margin-bottom: 14px; padding-bottom: 12px; border-bottom: 1px solid var(--border); }}\
    .zone-label {{ display: flex; align-items: center; gap: 10px; }}\
    .zone-icon {{\
      width: 36px; height: 36px; border-radius: 10px; display: grid; place-items: center;\
      background: var(--primary-soft); border: 1px solid var(--primary-glow); color: var(--primary);\
    }}\
    .zone-icon-desktop {{ background: rgba(56,189,248,0.12); border-color: rgba(56,189,248,0.3); color: #38bdf8; }}\
    .zone-name {{ font-size: 1rem; font-weight: 700; margin: 0; }}\
    .zone-sub {{ font-size: 0.76rem; color: var(--muted); margin: 0; }}\
    .os-tabs {{ display: inline-flex; background: var(--inset); border: 1px solid var(--border); border-radius: 9px; padding: 3px; gap: 3px; }}\
    .os-tab {{\
      border: 0; background: transparent; color: var(--muted); font-size: 0.75rem; font-weight: 600;\
      padding: 5px 10px; border-radius: 6px; cursor: pointer;\
    }}\
    .os-tab.active {{ background: var(--card); color: var(--fg); box-shadow: 0 1px 4px rgba(0,0,0,0.2); }}\
    .platform-divider {{\
      display: flex; align-items: center; gap: 12px; margin: 2px 0;\
      color: var(--muted); font-size: 0.74rem; font-weight: 700; text-transform: uppercase; letter-spacing: 0.08em;\
    }}\
    .platform-divider::before, .platform-divider::after {{ content: ''; flex: 1; height: 1px; background: linear-gradient(90deg, transparent, var(--border-strong), transparent); }}\
    .apps-grid {{ display: grid; grid-template-columns: repeat(auto-fill, minmax(205px, 1fr)); gap: 12px; }}\
    .app-card {{\
      display: flex; align-items: center; gap: 12px; padding: 13px 14px; border-radius: 12px;\
      background: var(--inset); border: 1px solid var(--border); color: var(--fg); text-decoration: none;\
      transition: border-color .16s, transform .14s, box-shadow .16s;\
    }}\
    .app-card:hover {{ border-color: var(--primary); transform: translateY(-2px); box-shadow: 0 10px 24px -14px var(--primary-glow); }}\
    .app-card.hidden {{ display: none; }}\
    .app-ico {{ width: 40px; height: 40px; flex-shrink: 0; }}\
    .app-ico svg {{ width: 40px; height: 40px; display: block; }}\
    .app-meta {{ min-width: 0; flex: 1; }}\
    .app-name {{ font-size: 0.94rem; font-weight: 700; display: flex; align-items: center; justify-content: space-between; gap: 6px; }}\
    .app-arrow {{ color: var(--muted); font-size: 0.9rem; }}\
    .app-card:hover .app-arrow {{ color: var(--primary); }}\
    .app-desc {{ font-size: 0.74rem; color: var(--muted); margin-top: 2px; }}\
    .os-badges {{ display: flex; flex-wrap: wrap; gap: 4px; margin-top: 5px; }}\
    .os-pill {{ font-size: 0.66rem; font-weight: 600; padding: 1px 6px; border-radius: 5px; background: var(--card); border: 1px solid var(--border); color: var(--muted); }}\
    .format-pills {{ display: flex; flex-wrap: wrap; gap: 8px; }}\
    .format-pill {{\
      display: inline-flex; align-items: center; padding: 5px 11px; border-radius: 8px;\
      background: var(--card); border: 1px solid var(--border); color: var(--fg);\
      text-decoration: none; font-size: 0.78rem; font-weight: 600;\
    }}\
    .format-pill:hover {{ border-color: var(--primary); color: var(--primary); }}\
    .servers-list {{ display: grid; grid-template-columns: 1fr; gap: 12px; }}\
    .server-card {{ padding: 15px 16px; border-radius: 13px; }}\
    .server-head {{ display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 8px; margin-bottom: 10px; }}\
    .server-title {{ display: flex; align-items: center; gap: 8px; }}\
    .server-name {{ font-size: 0.95rem; }}\
    .endpoint {{ font-size: 0.76rem; color: var(--muted); }}\
    .badge {{ border-radius: 6px; padding: 2px 8px; font-size: 0.7rem; font-weight: 700; letter-spacing: 0.03em; }}\
    .badge-vless {{ background: rgba(16,185,129,0.14); color: #34d399; border: 1px solid rgba(16,185,129,0.3); }}\
    .badge-hy2 {{ background: rgba(168,85,247,0.14); color: #c084fc; border: 1px solid rgba(168,85,247,0.3); }}\
    .badge-naive {{ background: rgba(56,189,248,0.14); color: #7dd3fc; border: 1px solid rgba(56,189,248,0.3); }}\
    .badge-mieru {{ background: rgba(245,158,11,0.14); color: #fbbf24; border: 1px solid rgba(245,158,11,0.3); }}\
    .badge-trojan {{ background: rgba(244,63,94,0.14); color: #fb7185; border: 1px solid rgba(244,63,94,0.3); }}\
    .badge-default {{ background: var(--primary-soft); color: var(--primary); border: 1px solid var(--primary-glow); }}\
    .drawer-row {{ display: flex; flex-wrap: wrap; gap: 14px; margin-top: 10px; }}\
    .drawer summary {{ cursor: pointer; font-size: 0.78rem; font-weight: 600; color: var(--primary); user-select: none; }}\
    .drawer pre {{ margin: 8px 0 0; padding: 10px; border-radius: 9px; background: var(--inset); border: 1px solid var(--border); overflow-x: auto; font-size: 0.76rem; max-width: 100%; }}\
    .toast {{\
      position: fixed; bottom: 20px; right: 20px; z-index: 50;\
      background: var(--card); color: var(--fg); border: 1px solid var(--primary);\
      border-radius: 10px; padding: 10px 14px; font-size: 0.82rem; font-weight: 600;\
      box-shadow: 0 12px 28px rgba(0,0,0,0.45); opacity: 0; pointer-events: none;\
      transform: translateY(8px); transition: opacity .18s, transform .18s;\
    }}\
    .toast.show {{ opacity: 1; transform: translateY(0); }}\
  </style>\
</head>\
<body>\
  <div class=\"shell\">\
    <header class=\"topbar\">\
      <div class=\"brand\">\
        {logo_svg}\
        <span>ProxyPanel</span>\
        <span class=\"brand-tag\" data-i18n=\"sub_badge\">Подписка</span>\
      </div>\
      <div class=\"topbar-controls\">\
        <button type=\"button\" class=\"ctrl-btn\" id=\"langToggle\">EN</button>\
        <button type=\"button\" class=\"ctrl-btn\" id=\"themeToggle\" data-i18n=\"theme_btn\">Тема</button>\
      </div>\
    </header>\
\
    <section class=\"blade hero-card\">\
      <div class=\"hero-grid\">\
        <div>\
          <div class=\"user-row\">\
            <h1 class=\"user-name\">{user_esc}</h1>\
            <span class=\"status-pill\"><span class=\"dot\"></span><span data-i18n=\"status_active\">Активна</span></span>\
          </div>\
          <div class=\"stats-grid\">\
            <div class=\"stat-tile\">\
              <span class=\"stat-label\" data-i18n=\"lbl_traffic\">Трафик</span>\
              <span class=\"stat-val\">{traffic_esc}</span>\
            </div>\
            <div class=\"stat-tile\">\
              <span class=\"stat-label\" data-i18n=\"lbl_ul_dl\">Отдача / Загрузка</span>\
              <span class=\"stat-val\">{ul_esc} / {dl_esc}</span>\
            </div>\
            <div class=\"stat-tile\">\
              <span class=\"stat-label\" data-i18n=\"lbl_expire\">Действует до</span>\
              <span class=\"stat-val\">{expire_esc}</span>\
            </div>\
            <div class=\"stat-tile\">\
              <span class=\"stat-label\" data-i18n=\"lbl_servers\">Серверы</span>\
              <span class=\"stat-val\">{node_count}</span>\
            </div>\
          </div>\
          <div class=\"progress-track\" aria-hidden=\"true\">\
            <div class=\"progress-fill\" style=\"width:{traffic_pct}%\"></div>\
          </div>\
          <div class=\"copy-row\">\
            <input type=\"text\" readonly class=\"mono key-input\" id=\"subUrlInput\" value=\"{sub_link_esc}\" aria-label=\"Subscription URL\">\
            <button type=\"button\" class=\"btn btn-primary copy-btn\" data-copy=\"{sub_link_esc}\">\
              <span data-i18n=\"copy_sub\">Скопировать ссылку</span>\
            </button>\
          </div>\
        </div>\
        <div class=\"qr-panel\">\
          <div class=\"qr-box\">{main_qr_svg}</div>\
          <span class=\"qr-caption\" data-i18n=\"scan_qr\">Сканируйте в клиенте</span>\
        </div>\
      </div>\
    </section>\
\
    <section class=\"device-split\">\
      <div class=\"blade device-zone\">\
        <div class=\"zone-header\">\
          <div class=\"zone-label\">\
            <div class=\"zone-icon\" aria-hidden=\"true\">\
              <svg width=\"18\" height=\"18\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><rect x=\"6\" y=\"2\" width=\"12\" height=\"20\" rx=\"2.5\"/><path d=\"M11 18h2\"/></svg>\
            </div>\
            <div>\
              <h2 class=\"zone-name\" data-i18n=\"mobile_title\">Смартфоны и планшеты (iPhone / Android)</h2>\
              <p class=\"zone-sub\" data-i18n=\"mobile_sub\">Нажмите на значок приложения — откроется клиент с автоматическим импортом подписки</p>\
            </div>\
          </div>\
          <div class=\"os-tabs\" role=\"tablist\" aria-label=\"Mobile OS filter\">\
            <button type=\"button\" class=\"os-tab active\" data-os-filter=\"all\" data-i18n=\"os_all\">Все</button>\
            <button type=\"button\" class=\"os-tab\" data-os-filter=\"ios\">iPhone (iOS)</button>\
            <button type=\"button\" class=\"os-tab\" data-os-filter=\"android\">Android</button>\
          </div>\
        </div>\
        <div class=\"apps-grid\" id=\"mobileAppsGrid\">\
          <a class=\"app-card app-launch\" data-os=\"ios android\" data-app=\"Happ\" data-copy=\"{sub_link_esc}\" href=\"{happ_href}\">\
            <div class=\"app-ico\">{icon_happ}</div>\
            <div class=\"app-meta\">\
              <div class=\"app-name\"><span>Happ</span><span class=\"app-arrow\">&#x2197;</span></div>\
              <div class=\"app-desc\" data-i18n=\"app_happ_mob\">Быстрый импорт в 1 касание</div>\
              <div class=\"os-badges\"><span class=\"os-pill\">iPhone</span><span class=\"os-pill\">Android</span></div>\
            </div>\
          </a>\
          <a class=\"app-card app-launch\" data-os=\"ios android\" data-app=\"Incy\" data-copy=\"{sub_link_esc}\" href=\"{incy_href}\">\
            <div class=\"app-ico\">{icon_incy}</div>\
            <div class=\"app-meta\">\
              <div class=\"app-name\"><span>Incy</span><span class=\"app-arrow\">&#x2197;</span></div>\
              <div class=\"app-desc\" data-i18n=\"app_incy_mob\">Защищённый импорт crypt1</div>\
              <div class=\"os-badges\"><span class=\"os-pill\">iPhone</span><span class=\"os-pill\">Android</span></div>\
            </div>\
          </a>\
          <a class=\"app-card app-launch\" data-os=\"ios android\" data-app=\"Karing\" data-copy=\"{sub_link_esc}\" href=\"{karing_href}\">\
            <div class=\"app-ico\">{icon_karing}</div>\
            <div class=\"app-meta\">\
              <div class=\"app-name\"><span>Karing</span><span class=\"app-arrow\">&#x2197;</span></div>\
              <div class=\"app-desc\" data-i18n=\"app_karing_mob\">Профиль Sing-box / Clash</div>\
              <div class=\"os-badges\"><span class=\"os-pill\">iPhone</span><span class=\"os-pill\">Android</span></div>\
            </div>\
          </a>\
          <a class=\"app-card app-launch\" data-os=\"android\" data-app=\"NekoBox\" data-copy=\"{sub_link_esc}\" href=\"{nekobox_href}\">\
            <div class=\"app-ico\">{icon_nekobox}</div>\
            <div class=\"app-meta\">\
              <div class=\"app-name\"><span>NekoBox</span><span class=\"app-arrow\">&#x2197;</span></div>\
              <div class=\"app-desc\" data-i18n=\"app_neko_mob\">Sing-box клиент для Android</div>\
              <div class=\"os-badges\"><span class=\"os-pill\">Android</span></div>\
            </div>\
          </a>\
        </div>\
      </div>\
\
      <div class=\"platform-divider\"><span data-i18n=\"pc_divider\">Компьютеры · ПК (Windows / macOS / Linux)</span></div>\
\
      <div class=\"blade device-zone\">\
        <div class=\"zone-header\">\
          <div class=\"zone-label\">\
            <div class=\"zone-icon zone-icon-desktop\" aria-hidden=\"true\">\
              <svg width=\"18\" height=\"18\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><rect x=\"2\" y=\"3\" width=\"20\" height=\"14\" rx=\"2.5\"/><path d=\"M8 21h8M12 17v4\"/></svg>\
            </div>\
            <div>\
              <h2 class=\"zone-name\" data-i18n=\"pc_title\">Десктопные клиенты для ПК</h2>\
              <p class=\"zone-sub\" data-i18n=\"pc_sub\">Нажмите на приложение для перехода в клиент (ссылка также копируется в буфер обмена)</p>\
            </div>\
          </div>\
        </div>\
        <div class=\"apps-grid\">\
          <a class=\"app-card app-launch\" data-app=\"Throne\" data-copy=\"{sub_link_esc}\" href=\"{throne_href}\">\
            <div class=\"app-ico\">{icon_throne}</div>\
            <div class=\"app-meta\">\
              <div class=\"app-name\"><span>Throne</span><span class=\"app-arrow\">&#x2197;</span></div>\
              <div class=\"app-desc\" data-i18n=\"app_throne_pc\">Наследник NekoRay на ядре Sing-box</div>\
              <div class=\"os-badges\"><span class=\"os-pill\">Windows</span><span class=\"os-pill\">macOS</span><span class=\"os-pill\">Linux</span></div>\
            </div>\
          </a>\
          <a class=\"app-card app-launch\" data-app=\"Happ Desktop\" data-copy=\"{sub_link_esc}\" href=\"{happ_href}\">\
            <div class=\"app-ico\">{icon_happ_pc}</div>\
            <div class=\"app-meta\">\
              <div class=\"app-name\"><span>Happ</span><span class=\"app-arrow\">&#x2197;</span></div>\
              <div class=\"app-desc\" data-i18n=\"app_happ_pc\">Десктопная версия Happ</div>\
              <div class=\"os-badges\"><span class=\"os-pill\">Windows</span><span class=\"os-pill\">macOS</span></div>\
            </div>\
          </a>\
        </div>\
      </div>\
    </section>\
\
    <div class=\"section-title\">\
      <h2 data-i18n=\"servers_title\">Доступные серверы и форматы</h2>\
      <div class=\"format-pills\">\
        <a class=\"format-pill\" href=\"{sub_link_esc}?flag=base64\">Base64</a>\
        <a class=\"format-pill\" href=\"{sub_link_esc}?flag=clash\">Clash / Mihomo</a>\
        <a class=\"format-pill\" href=\"{sub_link_esc}?flag=singbox\">Sing-box</a>\
        <a class=\"format-pill\" href=\"{sub_link_esc}?flag=json\">JSON (Native)</a>\
        <a class=\"format-pill\" href=\"{sub_link_esc}?flag=original\">Xray JSON</a>\
      </div>\
    </div>\
\
    <section class=\"servers-list\">\
      {cards_html}\
    </section>\
  </div>\
\
  <div class=\"toast\" id=\"toast\" role=\"status\" aria-live=\"polite\"></div>\
\
  <script>\
    (function() {{\
      var toast = document.getElementById('toast');\
      var timer = null;\
      function showToast(msg) {{\
        if (!toast) return;\
        toast.textContent = msg;\
        toast.classList.add('show');\
        if (timer) clearTimeout(timer);\
        timer = setTimeout(function() {{ toast.classList.remove('show'); }}, 2200);\
      }}\
      function copyText(text) {{\
        if (!text) return;\
        if (navigator.clipboard && navigator.clipboard.writeText) {{\
          navigator.clipboard.writeText(text).catch(function() {{}});\
        }} else {{\
          var ta = document.createElement('textarea');\
          ta.value = text; ta.style.position = 'fixed'; ta.style.opacity = '0';\
          document.body.appendChild(ta); ta.select();\
          try {{ document.execCommand('copy'); }} catch (e) {{}}\
          document.body.removeChild(ta);\
        }}\
      }}\
      document.querySelectorAll('.copy-btn').forEach(function(btn) {{\
        btn.addEventListener('click', function() {{\
          var val = btn.getAttribute('data-copy') || '';\
          copyText(val);\
          showToast(currentLang === 'en' ? 'Copied to clipboard' : 'Скопировано в буфер обмена');\
        }});\
      }});\
      document.querySelectorAll('.app-launch').forEach(function(link) {{\
        link.addEventListener('click', function() {{\
          var sub = link.getAttribute('data-copy') || '';\
          var app = link.getAttribute('data-app') || 'Client';\
          copyText(sub);\
          showToast((currentLang === 'en' ? 'Opening ' : 'Открываем ') + app + '...');\
        }});\
      }});\
      document.querySelectorAll('[data-os-filter]').forEach(function(tab) {{\
        tab.addEventListener('click', function() {{\
          document.querySelectorAll('[data-os-filter]').forEach(function(t) {{ t.classList.remove('active'); }});\
          tab.classList.add('active');\
          var f = tab.getAttribute('data-os-filter');\
          document.querySelectorAll('#mobileAppsGrid .app-card').forEach(function(card) {{\
            var osList = (card.getAttribute('data-os') || '').split(' ');\
            if (f === 'all' || osList.indexOf(f) !== -1) card.classList.remove('hidden');\
            else card.classList.add('hidden');\
          }});\
        }});\
      }});\
      var themeBtn = document.getElementById('themeToggle');\
      var savedTheme = localStorage.getItem('pp_sub_theme');\
      if (savedTheme === 'light') document.documentElement.classList.remove('dark');\
      if (themeBtn) {{\
        themeBtn.addEventListener('click', function() {{\
          var isDark = document.documentElement.classList.toggle('dark');\
          localStorage.setItem('pp_sub_theme', isDark ? 'dark' : 'light');\
        }});\
      }}\
      var dict = {{\
        ru: {{\
          sub_badge: 'Подписка', theme_btn: 'Тема', status_active: 'Активна',\
          lbl_traffic: 'Трафик', lbl_ul_dl: 'Отдача / Загрузка', lbl_expire: 'Действует до', lbl_servers: 'Серверы',\
          copy_sub: 'Скопировать ссылку', scan_qr: 'Сканируйте в клиенте',\
          mobile_title: 'Смартфоны и планшеты (iPhone / Android)',\
          mobile_sub: 'Нажмите на значок приложения — откроется клиент с автоматическим импортом подписки',\
          os_all: 'Все', app_happ_mob: 'Быстрый импорт в 1 касание', app_incy_mob: 'Защищённый импорт crypt1',\
          app_karing_mob: 'Профиль Sing-box / Clash', app_neko_mob: 'Sing-box клиент для Android',\
          pc_divider: 'Компьютеры · ПК (Windows / macOS / Linux)',\
          pc_title: 'Десктопные клиенты для ПК',\
          pc_sub: 'Нажмите на приложение для перехода в клиент (ссылка также копируется в буфер обмена)',\
          app_throne_pc: 'Наследник NekoRay на ядре Sing-box', app_happ_pc: 'Десктопная версия Happ',\
          servers_title: 'Доступные серверы и форматы', copy: 'Копировать', show_qr: 'Показать QR-код',\
          native_json: 'Нативный JSON конфиг', no_nodes: 'Нет активных серверов для этой подписки.'\
        }},\
        en: {{\
          sub_badge: 'Subscription', theme_btn: 'Theme', status_active: 'Active',\
          lbl_traffic: 'Traffic', lbl_ul_dl: 'Upload / Download', lbl_expire: 'Expires', lbl_servers: 'Servers',\
          copy_sub: 'Copy Subscription URL', scan_qr: 'Scan in client app',\
          mobile_title: 'Mobile Devices (iPhone / Android)',\
          mobile_sub: 'Tap an app card to launch it with automatic subscription import',\
          os_all: 'All', app_happ_mob: 'One-tap import', app_incy_mob: 'Encrypted crypt1 deep link',\
          app_karing_mob: 'Sing-box / Clash profile', app_neko_mob: 'Sing-box client for Android',\
          pc_divider: 'Desktop · PC (Windows / macOS / Linux)',\
          pc_title: 'Desktop Clients for PC',\
          pc_sub: 'Click to open the desktop app (URL is also copied to clipboard)',\
          app_throne_pc: 'NekoRay successor on Sing-box', app_happ_pc: 'Happ Desktop edition',\
          servers_title: 'Available Servers & Formats', copy: 'Copy', show_qr: 'Show QR Code',\
          native_json: 'Native JSON Config', no_nodes: 'No active nodes available for this subscription.'\
        }}\
      }};\
      var currentLang = localStorage.getItem('pp_sub_lang') === 'en' ? 'en' : 'ru';\
      var langBtn = document.getElementById('langToggle');\
      function applyLang(l) {{\
        currentLang = l;\
        var map = dict[l] || dict.ru;\
        document.querySelectorAll('[data-i18n]').forEach(function(el) {{\
          var k = el.getAttribute('data-i18n');\
          if (map[k]) el.textContent = map[k];\
        }});\
        if (langBtn) langBtn.textContent = l === 'ru' ? 'EN' : 'RU';\
      }}\
      applyLang(currentLang);\
      if (langBtn) {{\
        langBtn.addEventListener('click', function() {{\
          var next = currentLang === 'ru' ? 'en' : 'ru';\
          localStorage.setItem('pp_sub_lang', next);\
          applyLang(next);\
        }});\
      }}\
    }})();\
  </script>\
</body>\
</html>",
        user_esc = esc(username),
        logo_svg = logo_svg,
        traffic_esc = esc(&traffic_str),
        ul_esc = esc(&upload_str),
        dl_esc = esc(&download_str),
        expire_esc = esc(&expire_str),
        node_count = node_count,
        traffic_pct = traffic_pct,
        sub_link_esc = esc(&sub_link),
        main_qr_svg = main_qr_svg,
        happ_href = esc(&happ_deep_link),
        incy_href = esc(&incy_deep_link),
        karing_href = esc(&karing_deep_link),
        nekobox_href = esc(&nekobox_deep_link),
        throne_href = esc(&throne_deep_link),
        icon_happ = icon_happ,
        icon_happ_pc = icon_happ_pc,
        icon_incy = icon_incy,
        icon_karing = icon_karing,
        icon_nekobox = icon_nekobox,
        icon_throne = icon_throne,
        cards_html = cards_html,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use mypanel_proto::links::EntryContext;
    use mypanel_proto::wire::Protocol;
    use serde_json::json;
    use sha2::{Digest, Sha256};

    #[test]
    fn incy_k1_fingerprint_matches_official_spec() {
        let fp = hex::encode(Sha256::digest(INCY_K1));
        assert_eq!(
            fp,
            "b6bf708471cc90043232967660aade86a50b4e57929db2e53c5fa34db624c08c"
        );
    }

    #[test]
    fn incy_crypt1_round_trip_decrypts_sorted_json() {
        let link =
            encode_incy_crypt1("https://subpanel.example.com/sub/tok-123", Some("Velio")).unwrap();
        let encoded = link.strip_prefix("incy://crypt1/").unwrap();
        let wire = URL_SAFE_NO_PAD.decode(encoded).unwrap();
        assert!(wire.len() > 28);
        let (iv, ct) = wire.split_at(12);
        let cipher = Aes256Gcm::new_from_slice(&INCY_K1).unwrap();
        let pt = cipher.decrypt(Nonce::from_slice(iv), ct).unwrap();
        let text = String::from_utf8(pt).unwrap();
        assert_eq!(
            text,
            r#"{"n":"Velio","url":"https://subpanel.example.com/sub/tok-123","v":1}"#
        );
    }

    #[test]
    fn landing_html_contains_all_apps_and_qr_svg() {
        let ctx = EntryContext {
            node_host: "node.example.com".into(),
            port: 443,
            client_uuid: "682e693a-33b4-4563-9ca4-f11f54fae2da".into(),
            client_id: "u1".into(),
            secret: String::new(),
        };
        let entry = mypanel_proto::links::build_entry(
            Protocol::Vless,
            "EU-1",
            &json!({ "decryption": "none" }),
            &json!({
                "network": "xhttp",
                "security": "reality",
                "realitySettings": {
                    "dest": "127.0.0.1:9443",
                    "serverNames": ["node.example.com"],
                    "publicKey": "PUBKEY",
                    "shortIds": ["aa11"]
                },
                "xhttpSettings": { "path": "/xh", "mode": "packet-up" }
            }),
            &ctx,
        )
        .unwrap()
        .unwrap();

        let info = UserInfo {
            username: "Velio".into(),
            uuid: "682e693a-33b4-4563-9ca4-f11f54fae2da".into(),
            ..Default::default()
        };

        let html = render_landing_html(
            "Velio",
            "https://subpanel.fenrir322.pro",
            "tok-abc",
            1024 * 1024,
            0,
            None,
            &[entry],
            &info,
        );

        assert!(html.contains("happ://add/https://subpanel.fenrir322.pro/sub/tok-abc"));
        assert!(html.contains("incy://crypt1/"));
        assert!(html.contains("karing://install-config?url="));
        assert!(html.contains("sn://subscription?url="));
        assert!(html.contains("throne://addsub/?url="));
        assert!(html.contains("iPhone / Android"));
        assert!(html.contains("Windows / macOS / Linux"));
        assert!(html.contains("<svg"));
    }
}
