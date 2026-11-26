use anyhow::{bail, Result};
use std::env;
use std::path::{Path, PathBuf};

/// Default HTTP port for the worker node agent API (`127.0.0.1:2222`).
pub const DEFAULT_WORKER_PORT: u16 = 2222;
/// Default listen address for the worker node agent HTTP API.
pub const DEFAULT_WORKER_HOST: &str = "127.0.0.1";
/// Default directory for rendered core configuration and runtime state files.
pub const DEFAULT_CONFIG_DIR: &str = "/var/lib/mypanel3-node";
/// Default Xray-core binary path.
pub const DEFAULT_XRAY_BIN: &str = "/usr/local/bin/xray";
/// Default Sing-box binary path.
pub const DEFAULT_SINGBOX_BIN: &str = "/usr/local/bin/sing-box";
/// Default NaiveProxy (Caddy with `forward_proxy` plugin) binary path.
pub const DEFAULT_NAIVE_BIN: &str = "/usr/local/bin/caddy-naive";
/// Default Mieru server (`mita` / `mieru`) binary path.
pub const DEFAULT_MIERU_BIN: &str = "/usr/local/bin/mita";
/// Default telemt (Telegram WEB proxy) binary path.
pub const DEFAULT_TELEMT_BIN: &str = "/usr/local/bin/telemt";
/// Default local Xray gRPC `StatsService` & `HandlerService` port (`127.0.0.1:10085`).
pub const DEFAULT_XRAY_API_PORT: u16 = 10085;
/// Default local Sing-box Clash API address (`127.0.0.1:9091`).
pub const DEFAULT_SINGBOX_API_ADDR: &str = "127.0.0.1:9091";
/// Canonical worker agent version string.
pub const AGENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Configuration for the ProxyPanel v4 node worker daemon (`mypanel-agent`).
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Master panel base URL (e.g. `https://panel.example.com`). Empty in standalone mode.
    pub master_url: String,
    /// Shared secret authenticating this node with the Master (`NODE_RPC_SECRET`).
    pub node_secret: String,
    /// Human-readable node name reported during self-registration (`NODE_NAME`).
    pub node_name: String,
    /// Public host or domain of this node (`NODE_HOST`).
    pub node_host: String,
    /// Bind address for the worker HTTP server (`WORKER_HOST`, default `"127.0.0.1"`).
    pub worker_host: String,
    /// Bind port for the worker HTTP server (`WORKER_PORT`, default `2222`).
    pub worker_port: u16,
    /// Directory where core configurations, access logs, and state files are persisted (`CONFIG_DIR`).
    pub config_dir: PathBuf,
    /// Path to the `xray` binary (`XRAY_BIN`).
    pub xray_bin: PathBuf,
    /// Path to the `sing-box` binary (`SINGBOX_BIN`).
    pub singbox_bin: PathBuf,
    /// Path to the `caddy-naive` / `naive` binary (`NAIVE_BIN`).
    pub naive_bin: PathBuf,
    /// Path to the `mita` / `mieru` binary (`MIERU_BIN`).
    pub mieru_bin: PathBuf,
    /// Path to the `telemt` binary (`TELEMT_BIN`).
    pub telemt_bin: PathBuf,
    /// Optional `XRAY_LOCATION_ASSET` directory passed to child Xray processes.
    pub xray_asset_dir: Option<PathBuf>,
    /// Local loopback port for Xray gRPC `StatsService` and `HandlerService` (`XRAY_API_PORT`, default `10085`).
    pub xray_api_port: u16,
    /// Local loopback address for Sing-box Clash API (`SINGBOX_API_ADDR`, default `"127.0.0.1:9091"`).
    pub singbox_api_addr: String,
    /// Optional PEM-encoded CA certificate path or inline PEM for mTLS (`MTLS_CA_CERT`).
    pub mtls_ca_cert: Option<String>,
    /// Optional PEM-encoded client certificate path or inline PEM for mTLS (`MTLS_CLIENT_CERT`).
    pub mtls_client_cert: Option<String>,
    /// Optional PEM-encoded client private key path or inline PEM for mTLS (`MTLS_CLIENT_KEY`).
    pub mtls_client_key: Option<String>,
    /// Optional panel IP / CIDR allow-list (`ALLOWED_PANEL_IPS` or `PANEL_IP`). Empty allows any source IP.
    pub allowed_panel_ips: Vec<ipnet::IpNet>,
    /// How the panel talks to this node (`API_MODE`): `ws` (agent keeps a WebSocket to the panel,
    /// default) or `pull` (agent serves an mTLS HTTPS API the panel polls; no persistent session).
    pub api_mode: ApiMode,
}

/// Control channel between panel and node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ApiMode {
    /// Agent dials the panel and holds a WebSocket (works behind NAT without an open port).
    #[default]
    Ws,
    /// Panel dials the node over HTTPS with mutual TLS (like Remnawave); a panel restart changes nothing.
    Pull,
}

impl Default for AgentConfig {
    fn default() -> Self {
        let host = default_hostname();
        Self {
            master_url: String::new(),
            node_secret: String::new(),
            node_name: host.clone(),
            node_host: host,
            worker_host: DEFAULT_WORKER_HOST.to_string(),
            worker_port: DEFAULT_WORKER_PORT,
            config_dir: PathBuf::from(DEFAULT_CONFIG_DIR),
            xray_bin: PathBuf::from(DEFAULT_XRAY_BIN),
            singbox_bin: PathBuf::from(DEFAULT_SINGBOX_BIN),
            naive_bin: PathBuf::from(DEFAULT_NAIVE_BIN),
            mieru_bin: PathBuf::from(DEFAULT_MIERU_BIN),
            telemt_bin: PathBuf::from(DEFAULT_TELEMT_BIN),
            xray_asset_dir: None,
            xray_api_port: DEFAULT_XRAY_API_PORT,
            singbox_api_addr: DEFAULT_SINGBOX_API_ADDR.to_string(),
            mtls_ca_cert: None,
            mtls_client_cert: None,
            mtls_client_key: None,
            allowed_panel_ips: Vec::new(),
            api_mode: ApiMode::Ws,
        }
    }
}

impl AgentConfig {
    /// Loads [`AgentConfig`] from environment variables.
    pub fn from_env() -> Result<Self> {
        let pairs: Vec<(String, String)> = env::vars().collect();
        Self::from_pairs(&pairs)
    }

    /// Parses [`AgentConfig`] from an explicit slice of `(key, value)` environment pairs.
    pub fn from_pairs<K, V>(vars: &[(K, V)]) -> Result<Self>
    where
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let get = |name: &str| -> Option<String> {
            vars.iter()
                .find(|(k, _)| k.as_ref() == name)
                .map(|(_, v)| v.as_ref().trim().to_string())
                .filter(|v| !v.is_empty())
        };

        let mut cfg = Self::default();

        if let Some(url) = get("MASTER_URL") {
            cfg.master_url = url.trim_end_matches('/').to_string();
        }
        if let Some(sec) = get("NODE_RPC_SECRET").or_else(|| get("NODE_SECRET")) {
            if sec.len() < 16 || sec.len() > 256 {
                bail!("NODE_RPC_SECRET must be between 16 and 256 characters");
            }
            if sec.chars().any(|c| c.is_whitespace() || c.is_control()) {
                bail!("NODE_RPC_SECRET must not contain whitespace or control characters");
            }
            cfg.node_secret = sec;
        }
        if let Some(host) = get("NODE_HOST") {
            cfg.node_host = host;
        }
        if let Some(name) = get("NODE_NAME") {
            cfg.node_name = name;
        } else {
            cfg.node_name = cfg.node_host.clone();
        }
        if let Some(wh) = get("WORKER_HOST") {
            cfg.worker_host = wh;
        }
        if let Some(wp) = get("WORKER_PORT").or_else(|| get("NODE_API_PORT")) {
            let port: u16 = wp
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid WORKER_PORT: {wp}"))?;
            if port == 0 {
                bail!("WORKER_PORT must be in 1..=65535");
            }
            cfg.worker_port = port;
        }
        if let Some(dir) = get("CONFIG_DIR") {
            cfg.config_dir = PathBuf::from(dir);
        }
        if let Some(xb) = get("XRAY_BIN") {
            cfg.xray_bin = PathBuf::from(xb);
        }
        if let Some(sb) = get("SINGBOX_BIN") {
            cfg.singbox_bin = PathBuf::from(sb);
        }
        if let Some(nb) = get("NAIVE_BIN") {
            cfg.naive_bin = PathBuf::from(nb);
        }
        if let Some(mb) = get("MIERU_BIN") {
            cfg.mieru_bin = PathBuf::from(mb);
        }
        if let Some(tb) = get("TELEMT_BIN") {
            cfg.telemt_bin = PathBuf::from(tb);
        }
        if let Some(ad) = get("XRAY_LOCATION_ASSET") {
            cfg.xray_asset_dir = Some(PathBuf::from(ad));
        }
        if let Some(ap) = get("XRAY_API_PORT") {
            let port: u16 = ap
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid XRAY_API_PORT: {ap}"))?;
            if port == 0 {
                bail!("XRAY_API_PORT must be in 1..=65535");
            }
            cfg.xray_api_port = port;
        }
        if let Some(sa) = get("SINGBOX_API_ADDR") {
            cfg.singbox_api_addr = sa;
        }
        if let Some(mode) = get("API_MODE") {
            cfg.api_mode = match mode.to_ascii_lowercase().as_str() {
                "ws" | "websocket" => ApiMode::Ws,
                "pull" | "mtls" => ApiMode::Pull,
                other => bail!("Invalid API_MODE '{other}' (expected ws or pull)"),
            };
        }
        cfg.mtls_ca_cert = get("MTLS_CA_CERT");
        cfg.mtls_client_cert = get("MTLS_CLIENT_CERT");
        cfg.mtls_client_key = get("MTLS_CLIENT_KEY");

        if let Some(raw_ips) = get("ALLOWED_PANEL_IPS").or_else(|| get("PANEL_IP")) {
            let mut nets = Vec::new();
            for item in raw_ips.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                if item.eq_ignore_ascii_case("any") || item == "*" {
                    nets.clear();
                    break;
                }
                if let Ok(net) = item.parse::<ipnet::IpNet>() {
                    nets.push(net);
                } else if let Ok(ip) = item.parse::<std::net::IpAddr>() {
                    let normalized = normalize_ip_addr(ip);
                    nets.push(ipnet::IpNet::from(normalized));
                } else {
                    bail!("Invalid ALLOWED_PANEL_IPS / PANEL_IP entry: {item}");
                }
            }
            cfg.allowed_panel_ips = nets;
        }

        Ok(cfg)
    }

    /// Creates a test configuration pointing to `config_dir` with a valid 24-character node secret.
    pub fn for_tests(config_dir: impl AsRef<Path>) -> Self {
        Self {
            master_url: String::new(),
            node_secret: "test-node-secret-0123456789".to_string(),
            node_name: "test-node".to_string(),
            node_host: "127.0.0.1".to_string(),
            worker_host: "127.0.0.1".to_string(),
            worker_port: DEFAULT_WORKER_PORT,
            config_dir: config_dir.as_ref().to_path_buf(),
            ..Self::default()
        }
    }

    /// Path to `<CONFIG_DIR>/desired_config.json` used for cold-start restoration.
    pub fn desired_config_path(&self) -> PathBuf {
        self.config_dir.join("desired_config.json")
    }

    /// Path to `<CONFIG_DIR>/xray.json`.
    pub fn xray_config_path(&self) -> PathBuf {
        self.config_dir.join("xray.json")
    }

    /// Path to `<CONFIG_DIR>/singbox.json`.
    pub fn singbox_config_path(&self) -> PathBuf {
        self.config_dir.join("singbox.json")
    }

    /// `<CONFIG_DIR>/node-tls.key` - node TLS private key (generated on the node, never sent anywhere).
    pub fn tls_key_path(&self) -> PathBuf {
        self.config_dir.join("node-tls.key")
    }

    /// `<CONFIG_DIR>/node-tls.crt` - node TLS certificate signed by the panel CA.
    pub fn tls_cert_path(&self) -> PathBuf {
        self.config_dir.join("node-tls.crt")
    }

    /// `<CONFIG_DIR>/panel-ca.crt` - panel CA used to authenticate the panel's client certificate.
    pub fn panel_ca_path(&self) -> PathBuf {
        self.config_dir.join("panel-ca.crt")
    }

    /// Path to `<CONFIG_DIR>/traffic-pending.json`.
    pub fn traffic_pending_path(&self) -> PathBuf {
        self.config_dir.join("traffic-pending.json")
    }

    /// Path to `<CONFIG_DIR>/naive-reader.json` (persisted naive log position and totals).
    pub fn naive_reader_state_path(&self) -> PathBuf {
        self.config_dir.join("naive-reader.json")
    }

    /// Path to `<CONFIG_DIR>/naive-access.log`.
    pub fn naive_access_log_path(&self) -> PathBuf {
        self.config_dir.join("naive-access.log")
    }

    /// Returns `true` when mTLS client credentials and CA certificate are configured.
    pub fn has_mtls(&self) -> bool {
        self.mtls_ca_cert.is_some()
            && self.mtls_client_cert.is_some()
            && self.mtls_client_key.is_some()
    }
}

fn normalize_ip_addr(ip: std::net::IpAddr) -> std::net::IpAddr {
    match ip {
        std::net::IpAddr::V6(v6) => v6
            .to_ipv4_mapped()
            .map(std::net::IpAddr::V4)
            .unwrap_or(std::net::IpAddr::V6(v6)),
        other => other,
    }
}

/// Returns `true` when `peer` is allowed to access the node worker HTTP API.
/// Empty allow-lists and loopback addresses (`127.0.0.1`, `::1`) are always permitted.
pub fn is_panel_ip_allowed(allowed: &[ipnet::IpNet], peer: std::net::IpAddr) -> bool {
    if allowed.is_empty() {
        return true;
    }
    let normalized = normalize_ip_addr(peer);
    if normalized.is_loopback() {
        return true;
    }
    allowed.iter().any(|net| net.contains(&normalized))
}

fn default_hostname() -> String {
    env::var("HOSTNAME")
        .or_else(|_| env::var("COMPUTERNAME"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "proxypanel-node".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    #[test]
    fn api_mode_parses_and_rejects_garbage() {
        let pairs = |m: &str| vec![("API_MODE".to_string(), m.to_string())];
        assert_eq!(AgentConfig::from_pairs(&pairs("pull")).unwrap().api_mode, ApiMode::Pull);
        assert_eq!(AgentConfig::from_pairs(&pairs("WS")).unwrap().api_mode, ApiMode::Ws);
        assert!(AgentConfig::from_pairs(&pairs("carrier-pigeon")).is_err());
        let none: Vec<(String, String)> = vec![];
        assert_eq!(AgentConfig::from_pairs(&none).unwrap().api_mode, ApiMode::Ws);
    }

    #[test]
    fn default_worker_port_is_2222() {
        assert_eq!(DEFAULT_WORKER_PORT, 2222);
        let cfg = AgentConfig::default();
        assert_eq!(cfg.worker_port, 2222);
        assert!(cfg.allowed_panel_ips.is_empty());
    }

    #[test]
    fn from_pairs_supports_aliases_for_secret_and_port() {
        let cfg = AgentConfig::from_pairs(&[
            ("NODE_SECRET", "super-secret-token-123456"),
            ("NODE_API_PORT", "2222"),
        ])
        .expect("valid alias pairs");
        assert_eq!(cfg.node_secret, "super-secret-token-123456");
        assert_eq!(cfg.worker_port, 2222);

        let cfg2 = AgentConfig::from_pairs(&[
            ("NODE_RPC_SECRET", "primary-rpc-secret-123456"),
            ("WORKER_PORT", "2223"),
        ])
        .expect("valid primary pairs");
        assert_eq!(cfg2.node_secret, "primary-rpc-secret-123456");
        assert_eq!(cfg2.worker_port, 2223);
    }

    #[test]
    fn from_pairs_parses_panel_ip_and_allowed_panel_ips() {
        let single = AgentConfig::from_pairs(&[("PANEL_IP", "203.0.113.10")])
            .expect("single PANEL_IP");
        assert_eq!(single.allowed_panel_ips.len(), 1);
        assert!(is_panel_ip_allowed(
            &single.allowed_panel_ips,
            IpAddr::V4(Ipv4Addr::new(203, 0, 113, 10))
        ));
        assert!(is_panel_ip_allowed(
            &single.allowed_panel_ips,
            IpAddr::V6(Ipv4Addr::new(203, 0, 113, 10).to_ipv6_mapped())
        ));
        assert!(is_panel_ip_allowed(
            &single.allowed_panel_ips,
            IpAddr::V4(Ipv4Addr::LOCALHOST)
        ));
        assert!(is_panel_ip_allowed(
            &single.allowed_panel_ips,
            IpAddr::V6(Ipv6Addr::LOCALHOST)
        ));
        assert!(!is_panel_ip_allowed(
            &single.allowed_panel_ips,
            IpAddr::V4(Ipv4Addr::new(198, 51, 100, 77))
        ));

        let multi = AgentConfig::from_pairs(&[(
            "ALLOWED_PANEL_IPS",
            "203.0.113.10, 10.10.0.0/24, 2001:db8::1",
        )])
        .expect("comma-separated IP + CIDR");
        assert_eq!(multi.allowed_panel_ips.len(), 3);
        assert!(is_panel_ip_allowed(
            &multi.allowed_panel_ips,
            IpAddr::V4(Ipv4Addr::new(10, 10, 0, 42))
        ));
        assert!(is_panel_ip_allowed(
            &multi.allowed_panel_ips,
            "2001:db8::1".parse().unwrap()
        ));
        assert!(!is_panel_ip_allowed(
            &multi.allowed_panel_ips,
            IpAddr::V4(Ipv4Addr::new(10, 10, 1, 42))
        ));
    }

    #[test]
    fn from_pairs_rejects_invalid_panel_ip() {
        let err = AgentConfig::from_pairs(&[("PANEL_IP", "not-an-ip")])
            .expect_err("malformed PANEL_IP must fail");
        assert!(err.to_string().contains("ALLOWED_PANEL_IPS"));
    }
}
