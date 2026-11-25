use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

/// Maximum number of per-user traffic records accepted in a single [`TrafficReport`].
pub const MAX_TRAFFIC_ENTRIES: usize = 5000;
/// Maximum byte length of a traffic stat key (client id / email / username).
pub const MAX_TRAFFIC_KEY_LEN: usize = 320;
/// Maximum safe integer byte counter (`2^53 - 1`) compatible with JSON/JS and PostgreSQL `BIGINT`.
pub const MAX_SAFE_BYTES: u64 = 9_007_199_254_740_991;

/// Wire contract and validation errors.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum WireError {
    #[error("Unknown protocol: {0}")]
    UnknownProtocol(String),
    #[error("{0}")]
    Validation(String),
}

/// Proxy protocols supported across panel, agent, and subscription generators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Protocol {
    #[serde(rename = "VLESS")]
    Vless,
    #[serde(rename = "VMESS")]
    Vmess,
    #[serde(rename = "TROJAN")]
    Trojan,
    #[serde(rename = "SHADOWSOCKS")]
    Shadowsocks,
    #[serde(rename = "HYSTERIA2")]
    Hysteria2,
    #[serde(rename = "NAIVEPROXY")]
    Naiveproxy,
    #[serde(rename = "MIERU")]
    Mieru,
    #[serde(rename = "TUIC")]
    Tuic,
    #[serde(rename = "ANYTLS")]
    Anytls,
    #[serde(rename = "SHADOWTLS")]
    Shadowtls,
    /// Telegram WEB proxy (`tg://webproxy`) served by `telemt` behind a TLS terminator.
    #[serde(rename = "TGWEB")]
    Tgweb,
}

impl Protocol {
    /// Canonical uppercase string matching PostgreSQL `"Protocol"` enum literals.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Vless => "VLESS",
            Self::Vmess => "VMESS",
            Self::Trojan => "TROJAN",
            Self::Shadowsocks => "SHADOWSOCKS",
            Self::Hysteria2 => "HYSTERIA2",
            Self::Naiveproxy => "NAIVEPROXY",
            Self::Mieru => "MIERU",
            Self::Tuic => "TUIC",
            Self::Anytls => "ANYTLS",
            Self::Shadowtls => "SHADOWTLS",
            Self::Tgweb => "TGWEB",
        }
    }

    /// Human-readable protocol display label.
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Vless => "VLESS",
            Self::Vmess => "VMess",
            Self::Trojan => "Trojan",
            Self::Shadowsocks => "Shadowsocks",
            Self::Hysteria2 => "Hysteria2",
            Self::Naiveproxy => "NaiveProxy",
            Self::Mieru => "Mieru",
            Self::Tuic => "TUIC",
            Self::Anytls => "AnyTLS",
            Self::Shadowtls => "ShadowTLS",
            Self::Tgweb => "Telegram WEB",
        }
    }

    /// Returns `true` for protocols authenticated directly via client UUID (`VLESS`, `VMESS`).
    pub const fn is_uuid_auth(&self) -> bool {
        matches!(self, Self::Vless | Self::Vmess)
    }

    /// Identity conversion helper for callers migrating from `EntryProtocol::from_db`.
    pub const fn from_db(protocol: Self) -> Self {
        protocol
    }

    /// All 11 protocol variants in canonical order.
    pub const ALL: [Self; 11] = [
        Self::Vless,
        Self::Vmess,
        Self::Trojan,
        Self::Shadowsocks,
        Self::Hysteria2,
        Self::Naiveproxy,
        Self::Mieru,
        Self::Tuic,
        Self::Anytls,
        Self::Shadowtls,
        Self::Tgweb,
    ];
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Protocol {
    type Err = WireError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let normalized = s.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "vless" => Ok(Self::Vless),
            "vmess" => Ok(Self::Vmess),
            "trojan" => Ok(Self::Trojan),
            "shadowsocks" | "ss" => Ok(Self::Shadowsocks),
            "hysteria2" | "hy2" | "hysteria" => Ok(Self::Hysteria2),
            "naiveproxy" | "naive" => Ok(Self::Naiveproxy),
            "mieru" => Ok(Self::Mieru),
            "tuic" => Ok(Self::Tuic),
            "anytls" | "any-tls" => Ok(Self::Anytls),
            "shadowtls" | "shadow-tls" => Ok(Self::Shadowtls),
            "tgweb" | "telegram-web" | "telegramweb" => Ok(Self::Tgweb),
            _ => Err(WireError::UnknownProtocol(s.to_string())),
        }
    }
}

/// Empty object payload used for parameterless control messages (`config_request`, `heartbeat`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmptyPayload {}

/// Tagged WebSocket envelope exchanged between Master (`mypanel-panel`) and Worker (`mypanel-agent`).
///
/// Serialized shape: `{"event": "<snake_case>", "payload": <object>}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", content = "payload", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum Envelope {
    // Master -> Worker
    ConfigUpdate(ConfigUpdate),
    TrafficAck(TrafficAck),
    #[serde(alias = "drop_session")]
    DropSessionCmd(DropSessionCmd),
    #[serde(alias = "core_update")]
    CoreUpdateCmd(CoreUpdateCmd),
    ApplyConfigRequest(ApplyConfigRequest),
    /// Restart / stop / metrics request delivered over the already-open WebSocket.
    NodeControlCmd(NodeControlCmd),

    // Worker -> Master
    ConfigRequest(EmptyPayload),
    CommandAck(CommandAck),
    StatusReport(StatusReport),
    TrafficReport(TrafficReport),
    Heartbeat(EmptyPayload),
    LogChunk(LogChunk),
    SessionsList(SessionsList),
}

/// Master -> Worker configuration synchronization command.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigUpdate {
    pub command_id: String,
    #[serde(default)]
    pub revision: i32,
    #[serde(default)]
    pub inbounds: Vec<InboundConfig>,
    #[serde(default)]
    pub routing_rules: Vec<RoutingRuleConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xray_profile: Option<Value>,
    #[serde(default)]
    pub timestamp: i64,
}

/// Direct HTTP/WS configuration apply request (`POST /api/config` & cold-start `desired_config.json`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyConfigRequest {
    #[serde(default)]
    pub inbounds: Vec<InboundConfig>,
    #[serde(default)]
    pub routing_rules: Vec<RoutingRuleConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xray_profile: Option<Value>,
}

/// Response returned by the node agent's `POST /api/config`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ApplyConfigResponse {
    pub success: bool,
    pub timestamp: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Applied, but some inbound did not come up (missing binary, port not listening, ...).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// User entry materialized inside an inbound or port-share configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NodeUser {
    pub id: String,
    pub username: String,
    pub uuid: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flow: Option<String>,
    #[serde(default)]
    pub level: u32,
}

/// SNI-multiplexed port-share configuration attached to a primary TLS/TCP inbound.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortShareConfig {
    pub id: String,
    pub tag: String,
    pub protocol: Protocol,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default = "default_json_obj")]
    pub settings: Value,
    #[serde(default = "default_json_obj")]
    pub stream: Value,
    #[serde(default = "default_true")]
    pub enable: bool,
    #[serde(default)]
    pub users: Vec<NodeUser>,
}

/// Per-inbound subscription host override configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HostOverrideConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inbound_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remark: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sni: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alpn: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xhttp_extra: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_mask: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vless_route_id: Option<String>,
    #[serde(default)]
    pub shuffle_host: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fragment: Option<String>,
    #[serde(default)]
    pub is_cdn: bool,
    #[serde(default = "default_true")]
    pub enable: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_insecure: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned_peer_cert_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify_peer_cert_by_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mihomo_ip_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_description: Option<String>,
}

impl Default for HostOverrideConfig {
    fn default() -> Self {
        Self {
            id: None,
            inbound_id: None,
            remark: None,
            address: None,
            port: None,
            sni: None,
            host: None,
            path: None,
            alpn: None,
            fingerprint: None,
            xhttp_extra: None,
            final_mask: None,
            vless_route_id: None,
            shuffle_host: false,
            fragment: None,
            is_cdn: false,
            enable: true,
            tags: Vec::new(),
            allow_insecure: None,
            pinned_peer_cert_sha256: None,
            verify_peer_cert_by_name: None,
            mihomo_ip_version: None,
            server_description: None,
        }
    }
}

/// Complete inbound definition pushed to a node agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InboundConfig {
    pub id: String,
    pub tag: String,
    pub protocol: Protocol,
    #[serde(default = "default_listen")]
    pub listen: String,
    pub port: u16,
    #[serde(default = "default_json_obj")]
    pub settings: Value,
    #[serde(default = "default_json_obj")]
    pub stream: Value,
    #[serde(default = "default_json_obj")]
    pub routing: Value,
    #[serde(default = "default_true", deserialize_with = "deserialize_sniffing_bool")]
    pub sniffing: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remark: Option<String>,
    #[serde(default = "default_true")]
    pub enable: bool,
    #[serde(default)]
    pub users: Vec<NodeUser>,
    #[serde(default)]
    pub port_shares: Vec<PortShareConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hosts: Vec<HostOverrideConfig>,
}

/// Routing rule transmitted to Xray / Sing-box generators on the node agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RoutingRuleConfig {
    pub id: String,
    #[serde(rename = "type", alias = "ruleType", default = "default_rule_type")]
    pub rule_type: String,
    #[serde(default)]
    pub domain: Vec<String>,
    #[serde(default)]
    pub ip: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_port: Option<String>,
    #[serde(default)]
    pub source: Vec<String>,
    #[serde(default)]
    pub protocol: Vec<String>,
    #[serde(default)]
    pub inbound_tag: Vec<String>,
    #[serde(default = "default_outbound")]
    pub outbound_tag: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub balancer_tag: Option<String>,
}

impl Default for RoutingRuleConfig {
    fn default() -> Self {
        Self {
            id: String::new(),
            rule_type: default_rule_type(),
            domain: Vec::new(),
            ip: Vec::new(),
            port: None,
            source_port: None,
            source: Vec::new(),
            protocol: Vec::new(),
            inbound_tag: Vec::new(),
            outbound_tag: default_outbound(),
            balancer_tag: None,
        }
    }
}

/// Per-user upload/download delta counters.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TrafficCounter {
    #[serde(default)]
    pub upload: u64,
    #[serde(default)]
    pub download: u64,
    #[serde(default, skip_serializing_if = "is_false")]
    pub unmetered: bool,
}

/// Batched traffic report emitted by a node agent.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrafficReport {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch_id: Option<String>,
    #[serde(default)]
    pub stats: HashMap<String, TrafficCounter>,
}

impl TrafficReport {
    /// Validates entry count, key lengths, and byte bounds (`<= 2^53 - 1`).
    pub fn validate(&self) -> Result<(), WireError> {
        if self.stats.len() > MAX_TRAFFIC_ENTRIES {
            return Err(WireError::Validation("Too many traffic records".into()));
        }
        for (key, counter) in &self.stats {
            if key.is_empty() || key.len() > MAX_TRAFFIC_KEY_LEN {
                return Err(WireError::Validation("Invalid traffic key".into()));
            }
            if counter.upload > MAX_SAFE_BYTES || counter.download > MAX_SAFE_BYTES {
                return Err(WireError::Validation("Invalid traffic bytes".into()));
            }
        }
        Ok(())
    }
}

/// Acknowledgement from Master confirming receipt of a [`TrafficReport`] batch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TrafficAck {
    pub batch_id: String,
}

/// Worker acknowledgement for a Master command (`config_update`, `drop_session_cmd`, `core_update_cmd`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CommandAck {
    pub command_id: String,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Accounting mode supported by a proxy core on a node.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrafficMode {
    Metered,
    Unmetered,
}

/// Per-core traffic accounting & session termination capability descriptor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TrafficCapability {
    pub core: String,
    pub source: String,
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<String>,
    pub session_termination: bool,
    #[serde(default = "default_metered")]
    pub mode: TrafficMode,
}

impl TrafficCapability {
    /// Constructs a metered traffic capability entry.
    pub fn metered(
        core: impl Into<String>,
        source: impl Into<String>,
        bytes: impl Into<String>,
        session_termination: bool,
    ) -> Self {
        Self {
            core: core.into(),
            source: source.into(),
            available: true,
            bytes: Some(bytes.into()),
            session_termination,
            mode: TrafficMode::Metered,
        }
    }

    /// Constructs an unmetered traffic capability entry.
    pub fn unmetered(core: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            core: core.into(),
            source: source.into(),
            available: false,
            bytes: None,
            session_termination: false,
            mode: TrafficMode::Unmetered,
        }
    }

    /// Default capabilities where all proxy cores support metered per-user accounting.
    pub fn default_v4_capabilities() -> Vec<Self> {
        vec![
            Self::metered("xray", "grpc_stats", "tunnel", true),
            Self::metered("sing-box", "clash_api", "tunnel", true),
            Self::metered("hysteria2", "singbox_stats", "tunnel", true),
            Self::metered("mieru", "mita_metrics", "tunnel", false),
            Self::metered("naive", "caddy_access_log", "access_log", false),
            Self::metered("telemt", "prometheus_metrics", "tunnel", false),
        ]
    }
}

/// Detailed host & agent process telemetry reported inside [`StatusReport`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HostTelemetry {
    pub sampled_at: String,
    pub scope: String,
    pub cpu: CpuTelemetry,
    pub memory: MemoryTelemetry,
    pub uptime: UptimeTelemetry,
    pub connections: ConnectionsTelemetry,
    pub network: NetworkTelemetry,
    pub process: ProcessTelemetry,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CpuTelemetry {
    pub count: usize,
    pub usage_percent: Option<f64>,
    pub load_average: [f64; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MemoryTelemetry {
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub used_bytes: u64,
    pub usage_percent: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UptimeTelemetry {
    pub host_seconds: f64,
    pub process_seconds: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionsTelemetry {
    pub established_tcp: Option<u64>,
    pub udp_sockets: Option<u64>,
    pub scope: String,
    pub proxy_user_count: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NetworkTelemetry {
    pub scope: String,
    pub received_bytes: Option<u64>,
    pub transmitted_bytes: Option<u64>,
    pub upload_bps: Option<f64>,
    pub download_bps: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProcessTelemetry {
    pub memory: ProcessMemoryTelemetry,
    pub pid: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProcessMemoryTelemetry {
    pub rss: u64,
    #[serde(default)]
    pub heap_total: u64,
    #[serde(default)]
    pub heap_used: u64,
    #[serde(default)]
    pub external: u64,
    #[serde(default)]
    pub array_buffers: u64,
}

/// Summary of total & per-user traffic counters attached to HTTP status responses.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StatusTrafficSummary {
    pub total: TrafficCounter,
    #[serde(default)]
    pub per_user: HashMap<String, TrafficCounter>,
}

/// Worker heartbeat / status report sent over WS and returned by `GET /api/status`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusReport {
    pub status: String,
    pub xray_running: bool,
    pub singbox_running: bool,
    pub naive_running: bool,
    pub mieru_running: bool,
    #[serde(default)]
    pub xray_pid: Option<u32>,
    #[serde(default)]
    pub singbox_pid: Option<u32>,
    #[serde(default)]
    pub naive_pid: Option<u32>,
    #[serde(default)]
    pub mieru_pid: Option<u32>,
    pub uptime: u64,
    pub version: String,
    #[serde(default)]
    pub cpu_usage: Option<f64>,
    #[serde(default)]
    pub mem_usage: Option<f64>,
    #[serde(default)]
    pub connections: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_telemetry: Option<HostTelemetry>,
    #[serde(default)]
    pub traffic_capabilities: Vec<TrafficCapability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cores: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub traffic: Option<StatusTrafficSummary>,
}

/// System release metadata included during node self-registration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegisterSystemInfo {
    pub release: String,
}

/// Node self-registration payload (`POST /api/v1/nodes/self/register`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegisterRequest {
    pub token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<RegisterSystemInfo>,
    /// PEM PKCS#10 request for the node's TLS server certificate (pull mode). The private key
    /// never leaves the node; the panel signs the CSR with its internal CA.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub csr_pem: Option<String>,
    /// `"pull"` when the node serves an mTLS HTTPS API the panel polls; absent/`"ws"` otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_mode: Option<String>,
}

/// Node self-registration response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegisterResponse {
    pub node_id: String,
    pub status: String,
    pub message: String,
}

/// Single log line captured from a core or agent process.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub time: String,
    pub stream: String,
    pub line: String,
}

/// Live log chunk streamed from a node agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LogChunk {
    pub core: String,
    #[serde(default)]
    pub entries: Vec<LogEntry>,
}

/// Active proxy connection entry reported by a node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ActiveSession {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default)]
    pub inbound_tag: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol: Option<Protocol>,
    pub source_ip: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destination: Option<String>,
    #[serde(default)]
    pub uptime_secs: u64,
    #[serde(default)]
    pub upload: u64,
    #[serde(default)]
    pub download: u64,
}

/// Batch of active sessions reported by a node agent.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionsList {
    #[serde(default)]
    pub sessions: Vec<ActiveSession>,
}

/// Command to terminate matching active sessions on a node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DropSessionCmd {
    pub command_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_ip: Option<String>,
}

/// Command instructing a node agent to download, SHA-256 verify, and replace a proxy core binary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CoreUpdateCmd {
    pub command_id: String,
    pub core: String,
    pub version: String,
    pub download_url: String,
    pub sha256: String,
}

/// Response of `POST /api/traffic/collect` (pull mode): every unacknowledged batch, newest included.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrafficCollectResponse {
    #[serde(default)]
    pub reports: Vec<TrafficReport>,
}

/// Request of `POST /api/traffic/ack` (pull mode): batches the panel has durably recorded.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrafficAckRequest {
    #[serde(default)]
    pub batch_ids: Vec<String>,
}

/// Control action carried by [`NodeControlCmd`].
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeControlAction {
    Restart,
    Stop,
    /// Ask for a fresh `StatusReport` (used for on-demand metrics).
    Report,
}

/// Master -> Worker control command (answered with a [`CommandAck`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeControlCmd {
    pub command_id: String,
    pub action: NodeControlAction,
}

/// Repositories core binaries may be downloaded from (`/api/core-update`, `CoreUpdateCmd`).
const CORE_DOWNLOAD_ALLOWED: [(&str, &str); 5] = [
    ("github.com", "/XTLS/Xray-core/releases/download/"),
    ("github.com", "/SagerNet/sing-box/releases/download/"),
    ("github.com", "/klzgrad/forwardproxy/releases/download/"),
    ("github.com", "/enfein/mieru/releases/download/"),
    ("github.com", "/telemt/telemt/releases/download/"),
];

/// Accepts only `https://github.com/<allowed repo>/releases/download/...` URLs without credentials
/// or non-default ports, so a leaked node secret cannot be used to install an arbitrary binary.
pub fn is_allowed_core_download_url(raw: &str) -> bool {
    let Ok(u) = url::Url::parse(raw) else {
        return false;
    };
    if u.scheme() != "https"
        || !u.username().is_empty()
        || u.password().is_some()
        || u.port().is_some()
    {
        return false;
    }
    let Some(host) = u.host_str() else {
        return false;
    };
    let path = u.path();
    if path.contains("..") || path.contains("//") {
        return false;
    }
    CORE_DOWNLOAD_ALLOWED
        .iter()
        .any(|(h, prefix)| host.eq_ignore_ascii_case(h) && path.starts_with(prefix))
}

fn default_true() -> bool {
    true
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn default_listen() -> String {
    "0.0.0.0".to_string()
}

fn default_json_obj() -> Value {
    json!({})
}

fn default_rule_type() -> String {
    "field".to_string()
}

fn default_outbound() -> String {
    "direct".to_string()
}

const fn default_metered() -> TrafficMode {
    TrafficMode::Metered
}

/// Accepts either a JSON boolean (`true`/`false`) or legacy `{"enabled": bool}` object.
fn deserialize_sniffing_bool<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    struct SniffingVisitor;

    impl<'de> Visitor<'de> for SniffingVisitor {
        type Value = bool;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a boolean or an object like {\"enabled\": true}")
        }

        fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E> {
            Ok(v)
        }

        fn visit_none<E>(self) -> Result<Self::Value, E> {
            Ok(true)
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E> {
            Ok(true)
        }

        fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
        where
            A: de::MapAccess<'de>,
        {
            let mut enabled: Option<bool> = None;
            while let Some(key) = map.next_key::<String>()? {
                if key == "enabled" {
                    enabled = Some(map.next_value()?);
                } else {
                    let _: de::IgnoredAny = map.next_value()?;
                }
            }
            Ok(enabled.unwrap_or(true))
        }
    }

    deserializer.deserialize_any(SniffingVisitor)
}

#[cfg(test)]
mod control_tests {
    use super::*;

    #[test]
    fn register_request_is_backward_compatible() {
        // Old agents send neither field; both must default to None.
        let old: RegisterRequest = serde_json::from_str(r#"{"token":"x"}"#).unwrap();
        assert!(old.csr_pem.is_none() && old.api_mode.is_none());
        let json = serde_json::to_string(&old).unwrap();
        assert!(!json.contains("csrPem") && !json.contains("apiMode"));
        let new: RegisterRequest =
            serde_json::from_str(r#"{"token":"x","csrPem":"PEM","apiMode":"pull"}"#).unwrap();
        assert_eq!(new.api_mode.as_deref(), Some("pull"));
    }

    #[test]
    fn apply_response_omits_empty_warnings() {
        let r = ApplyConfigResponse { success: true, timestamp: 1, error: None, warnings: vec![] };
        assert!(!serde_json::to_string(&r).unwrap().contains("warnings"));
        let back: ApplyConfigResponse =
            serde_json::from_str(r#"{"success":true,"timestamp":1,"warnings":["a"]}"#).unwrap();
        assert_eq!(back.warnings, vec!["a".to_string()]);
    }

    #[test]
    fn core_download_allow_list() {
        let ok = [
            "https://github.com/enfein/mieru/releases/download/v3.39.0/mita_3.39.0_linux_amd64.tar.gz",
            "https://github.com/XTLS/Xray-core/releases/download/v26.11.9/Xray-linux-64.zip",
            "https://github.com/klzgrad/forwardproxy/releases/download/v2.11.7-naive2/caddy-forwardproxy-naive.tar.xz",
            "https://GitHub.com/SagerNet/sing-box/releases/download/v1.14.4/x.tar.gz",
        ];
        for u in ok {
            assert!(is_allowed_core_download_url(u), "{u}");
        }
        let bad = [
            "http://github.com/enfein/mieru/releases/download/v1/x.tar.gz",
            "https://evil.com/enfein/mieru/releases/download/v1/x.tar.gz",
            "https://github.com.evil.com/enfein/mieru/releases/download/v1/x",
            "https://github.com/evil/mieru/releases/download/v1/x",
            "https://github.com/enfein/mieru/releases/download/../../../evil/x",
            "https://user:pw@github.com/enfein/mieru/releases/download/v1/x",
            "https://github.com:8443/enfein/mieru/releases/download/v1/x",
            "file:///etc/passwd",
            "not a url",
        ];
        for u in bad {
            assert!(!is_allowed_core_download_url(u), "{u}");
        }
    }

    #[test]
    fn control_cmd_roundtrip() {
        let env = Envelope::NodeControlCmd(NodeControlCmd {
            command_id: "c1".into(),
            action: NodeControlAction::Restart,
        });
        let json = serde_json::to_string(&env).unwrap();
        assert!(json.contains("node_control_cmd") && json.contains("restart"));
        let back: Envelope = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, Envelope::NodeControlCmd(c) if c.action == NodeControlAction::Restart));
    }
}
