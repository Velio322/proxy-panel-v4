//! Pure, I/O-free shared protocol contracts, URI/link builders, subscription renderers,
//! and OpenWrt configuration exporters for `mypanel3` (`mypanel-panel` and `mypanel-agent`).
//!
//! # Public API Surface
//! - [`wire`]: Master <-> Worker JSON wire contracts (`Envelope` with `{"event": "...", "payload": ...}`,
//! 10-variant [`Protocol`], [`InboundConfig`], [`PortShareConfig`], [`HostOverrideConfig`],
//! [`NodeUser`], [`RoutingRuleConfig`], [`TrafficReport::validate`], [`TrafficCapability`],
//! [`StatusReport`], [`HostTelemetry`], [`LogChunk`], [`ActiveSession`], [`SessionsList`],
//! [`DropSessionCmd`], [`CoreUpdateCmd`], [`RegisterRequest`]/[`RegisterResponse`],
//! [`ApplyConfigRequest`]/[`ApplyConfigResponse`]).
//! - [`links`]: Normalized [`SubEntry`] extraction (`build_entry`, `build_port_share_entry`),
//! [`HostOverrideConfig`] application (`apply_host_override`, `expand_with_hosts`),
//! Hysteria2 port-range validation (`parse_port_range` <= 64 ports), Xray v26 / Sing-box 1.14 /
//! SS-2022 / VMess URI builders (`build_uri`, `vless_uri`, `vmess_uri`, `trojan_uri`,
//! `ss_uri`, `hysteria2_uri`, `tuic_uri`, `naive_uri`, `naive_link`, `mieru_config`,
//! `mieru_link`, `anytls_uri`, `shadowtls_uri`), and JS-compatible URL helpers
//! (`encode_uri_component`, `form_urlencode`, `bracket_host`, `js_hash_code`).
//! - [`subscription`]: [`SubFormat`] (`Base64`, `Json`, `Clash`, `Singbox`, `Original`, `Happ`),
//! [`ClientProfile`], UA & flag detection (`SubFormat::detect`, `detect_client_from_ua`),
//! RFC 5987 headers (`headers`), tag deduplication & `DIRECT` fallback, Sing-box 1.14 DNS & `.srs`
//! rule sets, Happ `happ://crypt4`/`5` encryption (`build_happ_crypt4`, `build_happ_crypt5`),
//! template substitution (`SubscriptionTemplate`, `apply_template`), SRR rules (`evaluate_srr`),
//! `age` encryption (`encrypt_age`), and unified [`SubscriptionRenderer::render`].
//! - [`openwrt`]: OpenWrt / router exporters (`generate_openclash_uci`, `generate_passwall_config`,
//! `generate_passwall_subscription`, `generate_openclash_yaml`, `generate_openclash_yaml_with_ts`,
//! `generate_singbox_subscription`, `generate_base64_subscription`, `generate_ssr_subscription`).
//!
//! # Wire `Envelope` Event Tag Names
//! - Master -> Worker: `config_update`, `traffic_ack`, `drop_session_cmd` (alias `drop_session`),
//! `core_update_cmd` (alias `core_update`), `apply_config_request`.
//! - Worker -> Master: `config_request`, `command_ack`, `status_report`, `traffic_report`,
//! `heartbeat`, `log_chunk`, `sessions_list`.

pub mod links;
pub mod openwrt;
pub mod subscription;
pub mod wire;

pub use wire::{
    ActiveSession, ApplyConfigRequest, ApplyConfigResponse, CommandAck, ConfigUpdate,
    ConnectionsTelemetry, CoreUpdateCmd, CpuTelemetry, DropSessionCmd, EmptyPayload, Envelope,
    HostOverrideConfig, HostTelemetry, InboundConfig, LogChunk, LogEntry, MemoryTelemetry,
    NetworkTelemetry, NodeUser, PortShareConfig, ProcessMemoryTelemetry, ProcessTelemetry,
    Protocol, RegisterRequest, RegisterResponse, RegisterSystemInfo, RoutingRuleConfig,
    SessionsList, StatusReport, StatusTrafficSummary, TrafficAck, TrafficCapability,
    TrafficCounter, TrafficMode, TrafficReport, UptimeTelemetry, WireError, MAX_SAFE_BYTES,
    is_allowed_core_download_url, NodeControlAction, NodeControlCmd, TrafficAckRequest,
    TrafficCollectResponse,
    MAX_TRAFFIC_ENTRIES, MAX_TRAFFIC_KEY_LEN,
};
