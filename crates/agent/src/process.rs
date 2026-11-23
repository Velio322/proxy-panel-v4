use crate::config::{AgentConfig, AGENT_VERSION};
use crate::cores::xray_grpc::{
    diff_configs, ConfigDiff, TonicXrayClient, UserOp, XrayHandlerClient,
};
use crate::cores::{mieru, naive, singbox, validate_inbound, write_atomic_0600, xray};
use crate::port_share::{remap_inbounds, PortShareRuntime};
use crate::telemetry::TelemetryCollector;
use crate::traffic::{
    drop_singbox_connection, fetch_singbox_connections_json, merge_traffic_maps,
    parse_mieru_metrics_json, parse_xray_stats, parse_xray_stats_json, NaiveAccessLogReader,
    SingboxConnectionTracker,
};
use chrono::Utc;
use mypanel_proto::{
    ActiveSession, DropSessionCmd, InboundConfig, LogEntry, Protocol, RoutingRuleConfig,
    StatusReport, StatusTrafficSummary, TrafficCapability, TrafficCounter,
};
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

/// Maximum number of log lines retained in the agent ring buffer (`/api/logs`).
pub const MAX_LOG_LINES: usize = 2000;

/// Outcome of [`ProcessManager::apply_config`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyOutcome {
    NoChange,
    HotReload(usize),
    FullRestart,
}

#[derive(Debug, Clone)]
pub struct LogBuffer {
    entries: Arc<Mutex<VecDeque<(String, LogEntry)>>>,
}

impl Default for LogBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl LogBuffer {
    pub fn new() -> Self {
        Self {
            entries: Arc::new(Mutex::new(VecDeque::with_capacity(MAX_LOG_LINES))),
        }
    }

    pub async fn push(&self, core: impl Into<String>, stream: impl Into<String>, line: impl Into<String>) {
        let mut guard = self.entries.lock().await;
        if guard.len() >= MAX_LOG_LINES {
            guard.pop_front();
        }
        guard.push_back((
            core.into(),
            LogEntry {
                time: Utc::now().to_rfc3339(),
                stream: stream.into(),
                line: line.into(),
            },
        ));
    }

    pub async fn recent(&self, core_filter: Option<&str>, limit: usize) -> Vec<LogEntry> {
        let guard = self.entries.lock().await;
        let cap = limit.min(MAX_LOG_LINES);
        let filtered: Vec<LogEntry> = guard
            .iter()
            .filter(|(c, _)| {
                core_filter
                    .map(|f| f.trim().is_empty() || c.eq_ignore_ascii_case(f.trim()))
                    .unwrap_or(true)
            })
            .map(|(_, entry)| entry.clone())
            .collect();
        if filtered.len() <= cap {
            filtered
        } else {
            filtered[filtered.len() - cap..].to_vec()
        }
    }
}

struct ManagedChild {
    child: Child,
    pid: Option<u32>,
}

/// Manages the lifecycle of all 4 proxy cores (`xray`, `sing-box`, `naive`, `mieru`),
/// native Tokio SNI port-sharing, ring-buffer logs, active sessions, and per-user traffic collection.
pub struct ProcessManager {
    pub config: AgentConfig,
    http_client: reqwest::Client,
    xray_handler: Arc<dyn XrayHandlerClient>,
    xray_tonic: TonicXrayClient,

    current_inbounds: Vec<InboundConfig>,
    current_routing_rules: Vec<RoutingRuleConfig>,
    current_xray_profile: Option<serde_json::Value>,

    xray_proc: Option<ManagedChild>,
    singbox_proc: Option<ManagedChild>,
    naive_procs: HashMap<String, ManagedChild>,
    mieru_procs: HashMap<String, ManagedChild>,

    port_share: PortShareRuntime,
    telemetry: TelemetryCollector,
    singbox_tracker: SingboxConnectionTracker,
    naive_log_reader: NaiveAccessLogReader,
    pub logs: LogBuffer,
    restart_counts: HashMap<String, u32>,
    core_versions: HashMap<String, String>,
    core_versions_probed: bool,
}

impl ProcessManager {
    pub fn new(config: AgentConfig, http_client: reqwest::Client) -> Self {
        let xray_tonic = TonicXrayClient::new(config.xray_api_port, &config.xray_bin);
        let xray_handler: Arc<dyn XrayHandlerClient> = Arc::new(xray_tonic.clone());
        Self {
            config,
            http_client,
            xray_handler,
            xray_tonic,
            current_inbounds: Vec::new(),
            current_routing_rules: Vec::new(),
            current_xray_profile: None,
            xray_proc: None,
            singbox_proc: None,
            naive_procs: HashMap::new(),
            mieru_procs: HashMap::new(),
            port_share: PortShareRuntime::new(),
            telemetry: TelemetryCollector::new(),
            singbox_tracker: SingboxConnectionTracker::new(),
            naive_log_reader: NaiveAccessLogReader::new(),
            logs: LogBuffer::new(),
            restart_counts: HashMap::new(),
            core_versions: HashMap::new(),
            core_versions_probed: false,
        }
    }

    /// Replaces the Xray gRPC `HandlerService` client (used in unit tests to mock hot-reload).
    pub fn set_xray_handler(&mut self, handler: Arc<dyn XrayHandlerClient>) {
        self.xray_handler = handler;
    }

    pub fn current_inbounds(&self) -> &[InboundConfig] {
        &self.current_inbounds
    }

    pub fn current_routing_rules(&self) -> &[RoutingRuleConfig] {
        &self.current_routing_rules
    }

    pub fn reset_restart_counters(&mut self) {
        self.restart_counts.clear();
    }

    pub fn invalidate_core_versions(&mut self) {
        self.core_versions_probed = false;
    }

    pub async fn refresh_core_versions(&mut self) -> HashMap<String, String> {
        self.core_versions = detect_core_versions(&self.config).await;
        self.core_versions_probed = true;
        self.core_versions.clone()
    }

    pub fn is_core_running(&mut self, canonical_core: &str) -> bool {
        match canonical_core {
            "xray" => self.xray_running(),
            "singbox" => self.singbox_running(),
            "naive" => self.naive_running(),
            "mieru" => self.mieru_running(),
            _ => false,
        }
    }

    pub fn core_is_in_use(&mut self, canonical_core: &str) -> bool {
        if self.is_core_running(canonical_core) {
            return true;
        }
        self.current_inbounds.iter().filter(|i| i.enable).any(|i| {
            let direct = match canonical_core {
                "xray" => xray::is_xray_protocol(i.protocol),
                "singbox" => singbox::is_singbox_protocol(i.protocol),
                "naive" => i.protocol == Protocol::Naiveproxy,
                "mieru" => i.protocol == Protocol::Mieru,
                _ => false,
            };
            let in_share = i.port_shares.iter().filter(|s| s.enable).any(|s| {
                match canonical_core {
                    "xray" => xray::is_xray_protocol(s.protocol),
                    "singbox" => singbox::is_singbox_protocol(s.protocol),
                    "naive" => s.protocol == Protocol::Naiveproxy,
                    "mieru" => s.protocol == Protocol::Mieru,
                    _ => false,
                }
            });
            direct || in_share
        })
    }

    /// Validates all inbounds, renders candidate configs into a temporary directory,
    /// and runs core CLI config checks (`xray run -test`, `sing-box check`, etc.) when binaries exist
    /// BEFORE stopping any currently running core process.
    pub async fn prevalidate_config(
        &self,
        inbounds: &[InboundConfig],
        routing_rules: &[RoutingRuleConfig],
    ) -> Result<(), String> {
        self.prevalidate_config_with_profile(inbounds, routing_rules, None)
            .await
    }

    pub async fn prevalidate_config_with_profile(
        &self,
        inbounds: &[InboundConfig],
        routing_rules: &[RoutingRuleConfig],
        xray_profile: Option<&serde_json::Value>,
    ) -> Result<(), String> {
        for ib in inbounds {
            validate_inbound(ib)?;
        }

        let remapped = remap_inbounds(inbounds)?;
        let tmp_path = std::env::temp_dir().join(format!("mypanel-preval-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&tmp_path)
            .await
            .map_err(|e| format!("Failed to create prevalidation temp dir: {e}"))?;
        struct CleanupDir(std::path::PathBuf);
        impl Drop for CleanupDir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = CleanupDir(tmp_path.clone());
        let tmp_dir_path = tmp_path.as_path();

        let xray_inbounds: Vec<InboundConfig> = remapped
            .inbounds
            .iter()
            .filter(|i| i.enable && xray::is_xray_protocol(i.protocol))
            .cloned()
            .collect();
        if !xray_inbounds.is_empty() {
            let mut cfg_val = xray::generate_config_with_profile(
                &xray_inbounds,
                routing_rules,
                self.config.xray_api_port,
                xray_profile,
            )?;
            // Avoid requiring root-writable /var/log/xray during dry-run -test prevalidation
            if let Some(log_obj) = cfg_val.get_mut("log").and_then(|v| v.as_object_mut()) {
                log_obj.remove("error");
            }
            let tmp_xray = tmp_dir_path.join("xray.json");
            let bytes = serde_json::to_vec_pretty(&cfg_val)
                .map_err(|e| format!("Failed to serialize xray config: {e}"))?;
            tokio::fs::write(&tmp_xray, &bytes)
                .await
                .map_err(|e| format!("Failed to write temp xray.json: {e}"))?;

            if binary_exists(&self.config.xray_bin).await {
                run_check_command(
                    &self.config.xray_bin,
                    &["run", "-test", "-c", &tmp_xray.to_string_lossy()],
                    self.xray_env(),
                    "xray",
                )
                .await?;
            }
        }

        let singbox_inbounds: Vec<InboundConfig> = remapped
            .inbounds
            .iter()
            .filter(|i| i.enable && singbox::is_singbox_protocol(i.protocol))
            .cloned()
            .collect();
        if !singbox_inbounds.is_empty() {
            let cfg_val = singbox::generate_config_with_routing(
                &singbox_inbounds,
                routing_rules,
                &self.config.singbox_api_addr,
            )?;
            let tmp_sb = tmp_dir_path.join("singbox.json");
            let bytes = serde_json::to_vec_pretty(&cfg_val)
                .map_err(|e| format!("Failed to serialize singbox config: {e}"))?;
            tokio::fs::write(&tmp_sb, &bytes)
                .await
                .map_err(|e| format!("Failed to write temp singbox.json: {e}"))?;

            if binary_exists(&self.config.singbox_bin).await {
                run_check_command(
                    &self.config.singbox_bin,
                    &["check", "-c", &tmp_sb.to_string_lossy()],
                    Vec::new(),
                    "sing-box",
                )
                .await?;
            }
        }

        for ib in remapped
            .inbounds
            .iter()
            .filter(|i| i.enable && i.protocol == Protocol::Naiveproxy)
        {
            let caddyfile = naive::generate_caddyfile(ib, &self.config.naive_access_log_path())?;
            let tmp_naive = naive::caddyfile_path(tmp_dir_path, &ib.id)?;
            tokio::fs::write(&tmp_naive, caddyfile.as_bytes())
                .await
                .map_err(|e| format!("Failed to write temp Caddyfile: {e}"))?;

            if binary_exists(&self.config.naive_bin).await {
                let tmp_cfg = tmp_dir_path.join(".config");
                let tmp_data = tmp_dir_path.join(".local/share");
                let _ = tokio::fs::create_dir_all(&tmp_cfg).await;
                let _ = tokio::fs::create_dir_all(&tmp_data).await;
                let naive_env = vec![
                    ("HOME".to_string(), tmp_dir_path.to_string_lossy().to_string()),
                    ("XDG_CONFIG_HOME".to_string(), tmp_cfg.to_string_lossy().to_string()),
                    ("XDG_DATA_HOME".to_string(), tmp_data.to_string_lossy().to_string()),
                ];
                run_check_command(
                    &self.config.naive_bin,
                    &[
                        "validate",
                        "--config",
                        &tmp_naive.to_string_lossy(),
                        "--adapter",
                        "caddyfile",
                    ],
                    naive_env,
                    "naive",
                )
                .await?;
            }
        }

        for ib in remapped
            .inbounds
            .iter()
            .filter(|i| i.enable && i.protocol == Protocol::Mieru)
        {
            let cfg_val = mieru::generate_config(ib)?;
            let tmp_mieru = mieru::mieru_config_path(tmp_dir_path, &ib.id)?;
            let bytes = serde_json::to_vec_pretty(&cfg_val)
                .map_err(|e| format!("Failed to serialize mieru config: {e}"))?;
            tokio::fs::write(&tmp_mieru, &bytes)
                .await
                .map_err(|e| format!("Failed to write temp mieru config: {e}"))?;
        }

        Ok(())
    }

    /// Applies a new desired configuration.
    ///
    /// - If only Xray user lists changed and Xray is running (or `force_hot_reload` via mocked handler),
    /// performs an incremental `HandlerService/AlterInbound` hot-reload without restarting any core.
    /// - Otherwise, runs `prevalidate_config` BEFORE stopping existing cores, then writes
    /// `0600` configs, starts the SNI port-share runtime, and spawns enabled cores.
    pub async fn apply_config(
        &mut self,
        inbounds: Vec<InboundConfig>,
        routing_rules: Vec<RoutingRuleConfig>,
    ) -> Result<ApplyOutcome, String> {
        self.apply_config_with_profile(inbounds, routing_rules, None)
            .await
    }

    pub async fn apply_config_with_profile(
        &mut self,
        inbounds: Vec<InboundConfig>,
        routing_rules: Vec<RoutingRuleConfig>,
        xray_profile: Option<serde_json::Value>,
    ) -> Result<ApplyOutcome, String> {
        self.apply_config_inner(inbounds, routing_rules, xray_profile, false)
            .await
    }

    async fn apply_config_inner(
        &mut self,
        inbounds: Vec<InboundConfig>,
        routing_rules: Vec<RoutingRuleConfig>,
        xray_profile: Option<serde_json::Value>,
        force_restart: bool,
    ) -> Result<ApplyOutcome, String> {
        let profile_changed = xray_profile != self.current_xray_profile;
        let has_applied_before = !self.current_inbounds.is_empty() || self.any_core_running();
        if !force_restart && has_applied_before && !profile_changed {
            match diff_configs(
                &self.current_inbounds,
                &self.current_routing_rules,
                &inbounds,
                &routing_rules,
            )? {
                ConfigDiff::NoChange => return Ok(ApplyOutcome::NoChange),
                ConfigDiff::HotReload(ops) if !ops.is_empty() => {
                    if self.xray_running() || !binary_exists(&self.config.xray_bin).await {
                        if let Err(err) = self
                            .apply_xray_hot_reload(&ops, &inbounds, &routing_rules, xray_profile.as_ref())
                            .await
                        {
                            self.logs
                                .push("agent", "warn", format!("Xray hot-reload failed ({err}), falling back to full restart"))
                                .await;
                        } else {
                            self.current_inbounds = inbounds;
                            self.current_routing_rules = routing_rules;
                            return Ok(ApplyOutcome::HotReload(ops.len()));
                        }
                    }
                }
                _ => {}
            }
        }

        // Validate everything BEFORE stopping existing cores
        self.prevalidate_config_with_profile(&inbounds, &routing_rules, xray_profile.as_ref())
            .await?;

        let remapped = remap_inbounds(&inbounds)?;
        self.stop_all().await;

        let xray_inbounds: Vec<InboundConfig> = remapped
            .inbounds
            .iter()
            .filter(|i| i.enable && xray::is_xray_protocol(i.protocol))
            .cloned()
            .collect();
        if !xray_inbounds.is_empty() {
            let cfg_val = xray::generate_config_with_profile(
                &xray_inbounds,
                &routing_rules,
                self.config.xray_api_port,
                xray_profile.as_ref(),
            )?;
            let bytes = serde_json::to_vec_pretty(&cfg_val)
                .map_err(|e| format!("Failed to serialize xray.json: {e}"))?;
            write_atomic_0600(&self.config.xray_config_path(), &bytes).await?;

            let _ = tokio::fs::create_dir_all("/var/log/xray").await;
            if binary_exists(&self.config.xray_bin).await {
                let child = self
                    .spawn_core(
                        "xray",
                        &self.config.xray_bin.clone(),
                        &["run", "-c", &self.config.xray_config_path().to_string_lossy()],
                        self.xray_env(),
                    )
                    .await?;
                self.xray_proc = Some(child);
            }
        }

        let singbox_inbounds: Vec<InboundConfig> = remapped
            .inbounds
            .iter()
            .filter(|i| i.enable && singbox::is_singbox_protocol(i.protocol))
            .cloned()
            .collect();
        if !singbox_inbounds.is_empty() {
            let cfg_val = singbox::generate_config_with_routing(
                &singbox_inbounds,
                &routing_rules,
                &self.config.singbox_api_addr,
            )?;
            let bytes = serde_json::to_vec_pretty(&cfg_val)
                .map_err(|e| format!("Failed to serialize singbox.json: {e}"))?;
            write_atomic_0600(&self.config.singbox_config_path(), &bytes).await?;

            if binary_exists(&self.config.singbox_bin).await {
                let child = self
                    .spawn_core(
                        "sing-box",
                        &self.config.singbox_bin.clone(),
                        &[
                            "run",
                            "-c",
                            &self.config.singbox_config_path().to_string_lossy(),
                        ],
                        Vec::new(),
                    )
                    .await?;
                self.singbox_proc = Some(child);
            }
        }

        for ib in remapped
            .inbounds
            .iter()
            .filter(|i| i.enable && i.protocol == Protocol::Naiveproxy)
        {
            let caddyfile = naive::generate_caddyfile(ib, &self.config.naive_access_log_path())?;
            let path = naive::caddyfile_path(&self.config.config_dir, &ib.id)?;
            write_atomic_0600(&path, caddyfile.as_bytes()).await?;

            let fallback_root = ib
                .settings
                .get("fallbackRoot")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("/var/www/html");
            let _ = naive::ensure_fallback_index(Path::new(fallback_root)).await;

            if binary_exists(&self.config.naive_bin).await {
                let naive_env = self.naive_env().await;
                let child = self
                    .spawn_core(
                        "naive",
                        &self.config.naive_bin.clone(),
                        &[
                            "run",
                            "--config",
                            &path.to_string_lossy(),
                            "--adapter",
                            "caddyfile",
                        ],
                        naive_env,
                    )
                    .await?;
                self.naive_procs.insert(ib.id.clone(), child);
            } else {
                self.warn_missing_core("naive", &self.config.naive_bin.clone(), &ib.tag)
                    .await;
            }
        }

        for ib in remapped
            .inbounds
            .iter()
            .filter(|i| i.enable && i.protocol == Protocol::Mieru)
        {
            let cfg_val = mieru::generate_config(ib)?;
            let path = mieru::mieru_config_path(&self.config.config_dir, &ib.id)?;
            let bytes = serde_json::to_vec_pretty(&cfg_val)
                .map_err(|e| format!("Failed to serialize mieru config: {e}"))?;
            write_atomic_0600(&path, &bytes).await?;

            if binary_exists(&self.config.mieru_bin).await {
                let _ = tokio::fs::create_dir_all("/var/run/mita").await;
                let path_str = path.to_string_lossy().to_string();
                let child = self
                    .spawn_core(
                        "mieru",
                        &self.config.mieru_bin.clone(),
                        &["run"],
                        vec![
                            ("MITA_CONFIG_JSON_FILE".to_string(), path_str.clone()),
                            ("MITA_LOG_NO_TIMESTAMP".to_string(), "true".to_string()),
                            ("MIERU_CONF_FILE".to_string(), path_str.clone()),
                        ],
                    )
                    .await?;
                self.mieru_procs.insert(ib.id.clone(), child);

                // Wait briefly for /var/run/mita/mita.sock to be ready, then run
                // `mita replace config <path>` and `mita reload` so Mieru hashes user passwords
                // into `hashedPassword` and removes any deleted users from its state.
                let sock_path = Path::new("/var/run/mita/mita.sock");
                for _ in 0..40 {
                    if tokio::fs::metadata(sock_path).await.is_ok() {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                let mut replace_cmd = Command::new(&self.config.mieru_bin);
                replace_cmd
                    .args(["replace", "config", &path_str])
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());
                if let Ok(Ok(out)) =
                    tokio::time::timeout(Duration::from_secs(5), replace_cmd.output()).await
                {
                    if !out.status.success() {
                        let err_msg = String::from_utf8_lossy(&out.stderr);
                        self.logs
                            .push(
                                "mieru",
                                "warn",
                                format!("mita replace config exited with {}: {}", out.status, err_msg.trim()),
                            )
                            .await;
                    }
                }
                let mut reload_cmd = Command::new(&self.config.mieru_bin);
                reload_cmd
                    .args(["reload"])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                let _ = tokio::time::timeout(Duration::from_secs(5), reload_cmd.output()).await;
            } else {
                self.warn_missing_core("mieru", &self.config.mieru_bin.clone(), &ib.tag)
                    .await;
            }
        }

        if let Err(err) = self.port_share.apply(&remapped.groups).await {
            self.logs
                .push("port-share", "error", format!("PortShare apply failed: {err}"))
                .await;
            return Err(err);
        }

        for warning in crate::firewall::open_ports_for_inbounds(&inbounds).await {
            self.logs.push("firewall", "warn", warning).await;
        }

        self.current_inbounds = inbounds;
        self.current_routing_rules = routing_rules;
        self.current_xray_profile = xray_profile;
        Ok(ApplyOutcome::FullRestart)
    }

    async fn apply_xray_hot_reload(
        &self,
        ops: &[UserOp],
        new_inbounds: &[InboundConfig],
        new_rules: &[RoutingRuleConfig],
        xray_profile: Option<&serde_json::Value>,
    ) -> Result<(), String> {
        self.xray_handler.apply_ops(ops).await?;

        // Keep xray.json on disk synchronized with hot-reloaded users
        let remapped = remap_inbounds(new_inbounds)?;
        let xray_inbounds: Vec<InboundConfig> = remapped
            .inbounds
            .iter()
            .filter(|i| i.enable && xray::is_xray_protocol(i.protocol))
            .cloned()
            .collect();
        if !xray_inbounds.is_empty() {
            let cfg_val = xray::generate_config_with_profile(
                &xray_inbounds,
                new_rules,
                self.config.xray_api_port,
                xray_profile,
            )?;
            let bytes = serde_json::to_vec_pretty(&cfg_val)
                .map_err(|e| format!("Failed to serialize xray.json: {e}"))?;
            write_atomic_0600(&self.config.xray_config_path(), &bytes).await?;
        }
        self.logs
            .push(
                "xray",
                "info",
                format!("Applied {} incremental HandlerService user ops", ops.len()),
            )
            .await;
        Ok(())
    }

    /// Re-applies the stored `current_inbounds`, `current_routing_rules`, and `current_xray_profile` after resetting restart counters.
    pub async fn restart(&mut self) -> Result<ApplyOutcome, String> {
        self.reset_restart_counters();
        let inbounds = self.current_inbounds.clone();
        let rules = self.current_routing_rules.clone();
        let profile = self.current_xray_profile.clone();
        if inbounds.is_empty() {
            self.stop_all().await;
            return Ok(ApplyOutcome::NoChange);
        }
        self.apply_config_inner(inbounds, rules, profile, true).await
    }

    /// Stops all running proxy cores and SNI port-share listeners, retaining `current_inbounds`.
    pub async fn stop_all(&mut self) {
        self.port_share.stop().await;

        if let Some(mut mc) = self.xray_proc.take() {
            terminate_child(&mut mc.child).await;
        }
        if let Some(mut mc) = self.singbox_proc.take() {
            terminate_child(&mut mc.child).await;
        }
        for (_, mut mc) in self.naive_procs.drain() {
            terminate_child(&mut mc.child).await;
        }
        for (_, mut mc) in self.mieru_procs.drain() {
            terminate_child(&mut mc.child).await;
        }
    }

    fn xray_env(&self) -> Vec<(String, String)> {
        let mut env = Vec::new();
        if let Some(ref asset_dir) = self.config.xray_asset_dir {
            env.push((
                "XRAY_LOCATION_ASSET".to_string(),
                asset_dir.to_string_lossy().to_string(),
            ));
        }
        env
    }

    async fn naive_env(&self) -> Vec<(String, String)> {
        let cfg_home = self.config.config_dir.join(".config");
        let data_home = self.config.config_dir.join(".local/share");
        let _ = tokio::fs::create_dir_all(&cfg_home).await;
        let _ = tokio::fs::create_dir_all(&data_home).await;
        vec![
            (
                "HOME".to_string(),
                self.config.config_dir.to_string_lossy().to_string(),
            ),
            (
                "XDG_CONFIG_HOME".to_string(),
                cfg_home.to_string_lossy().to_string(),
            ),
            (
                "XDG_DATA_HOME".to_string(),
                data_home.to_string_lossy().to_string(),
            ),
        ]
    }

    async fn spawn_core(
        &self,
        core_name: &str,
        bin: &Path,
        args: &[&str],
        extra_env: Vec<(String, String)>,
    ) -> Result<ManagedChild, String> {
        let mut cmd = Command::new(bin);
        cmd.args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env_clear();

        if let Ok(path) = std::env::var("PATH") {
            cmd.env("PATH", path);
        }
        for (k, v) in extra_env {
            cmd.env(k, v);
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn {core_name} ({}): {e}", bin.display()))?;
        let pid = child.id();

        if let Some(stdout) = child.stdout.take() {
            let logs = self.logs.clone();
            let name = core_name.to_string();
            tokio::spawn(async move {
                let mut reader = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        logs.push(name.clone(), "stdout", trimmed.to_string()).await;
                    }
                }
            });
        }

        if let Some(stderr) = child.stderr.take() {
            let logs = self.logs.clone();
            let name = core_name.to_string();
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        logs.push(name.clone(), "stderr", trimmed.to_string()).await;
                    }
                }
            });
        }

        self.logs
            .push(
                core_name,
                "info",
                format!("Started {core_name} (pid={:?})", pid),
            )
            .await;

        Ok(ManagedChild { child, pid })
    }

    pub fn xray_running(&mut self) -> bool {
        check_child_alive(&mut self.xray_proc)
    }

    pub fn singbox_running(&mut self) -> bool {
        check_child_alive(&mut self.singbox_proc)
    }

    pub fn naive_running(&mut self) -> bool {
        prune_dead_children(&mut self.naive_procs);
        !self.naive_procs.is_empty()
    }

    pub fn mieru_running(&mut self) -> bool {
        prune_dead_children(&mut self.mieru_procs);
        !self.mieru_procs.is_empty()
    }

    pub fn any_core_running(&mut self) -> bool {
        self.xray_running()
            || self.singbox_running()
            || self.naive_running()
            || self.mieru_running()
    }

    /// Collects cumulative per-user traffic counters across all 4 proxy cores (`unmetered: false`).
    pub async fn collect_traffic(&mut self) -> HashMap<String, TrafficCounter> {
        let xray_map = if self.xray_running() {
            self.collect_xray_traffic().await
        } else {
            HashMap::new()
        };

        let singbox_map = if self.singbox_running() {
            if let Ok(body) =
                fetch_singbox_connections_json(&self.http_client, &self.config.singbox_api_addr)
                    .await
            {
                match self.singbox_tracker.ingest_connections_json(&body) {
                    Ok((totals, _)) => totals,
                    Err(_) => self.singbox_tracker.current_totals(),
                }
            } else {
                self.singbox_tracker.current_totals()
            }
        } else {
            HashMap::new()
        };

        let mieru_map = if self.mieru_running() {
            self.collect_mieru_traffic().await
        } else {
            HashMap::new()
        };

        let naive_map = self
            .naive_log_reader
            .poll(&self.config.naive_access_log_path())
            .await;

        merge_traffic_maps(&[&xray_map, &singbox_map, &mieru_map, &naive_map])
    }

    async fn collect_xray_traffic(&self) -> HashMap<String, TrafficCounter> {
        if let Ok(entries) = self.xray_tonic.query_user_stats(false).await {
            return parse_xray_stats(&entries);
        }
        // CLI fallback when gRPC tonic fails
        let server_arg = format!("--server=127.0.0.1:{}", self.config.xray_api_port);
        let mut cmd = Command::new(&self.config.xray_bin);
        cmd.args(["api", "statsquery", &server_arg])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .env_clear();
        if let Ok(path) = std::env::var("PATH") {
            cmd.env("PATH", path);
        }
        if let Ok(Ok(out)) = tokio::time::timeout(Duration::from_secs(3), cmd.output()).await {
            if out.status.success() {
                if let Ok(stdout) = String::from_utf8(out.stdout) {
                    if let Ok(parsed) = parse_xray_stats_json(&stdout) {
                        return parsed;
                    }
                }
            }
        }
        HashMap::new()
    }

    async fn collect_mieru_traffic(&self) -> HashMap<String, TrafficCounter> {
        let mut merged = HashMap::new();
        for id in self.mieru_procs.keys() {
            let Ok(conf_path) = mieru::mieru_config_path(&self.config.config_dir, id) else {
                continue;
            };
            let mut cmd = Command::new(&self.config.mieru_bin);
            cmd.args(["get", "metrics"])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .env_clear()
                .env("MITA_CONFIG_JSON_FILE", &conf_path)
                .env("MIERU_CONF_FILE", &conf_path);
            if let Ok(path) = std::env::var("PATH") {
                cmd.env("PATH", path);
            }
            if let Ok(Ok(out)) = tokio::time::timeout(Duration::from_secs(3), cmd.output()).await {
                if out.status.success() {
                    if let Ok(stdout) = String::from_utf8(out.stdout) {
                        if let Ok(m) = parse_mieru_metrics_json(&stdout) {
                            merged = merge_traffic_maps(&[&merged, &m]);
                        }
                    }
                }
            }
        }
        merged
    }

    /// Returns currently active proxy sessions across Sing-box Clash API & Xray online users.
    pub async fn get_sessions(&mut self) -> Vec<ActiveSession> {
        let mut sessions = Vec::new();

        if self.singbox_running() {
            if let Ok(body) =
                fetch_singbox_connections_json(&self.http_client, &self.config.singbox_api_addr)
                    .await
            {
                if let Ok((_, sb_sessions)) = self.singbox_tracker.ingest_connections_json(&body) {
                    sessions.extend(sb_sessions);
                }
            }
        }

        if self.xray_running() {
            if let Ok(online_users) = self.xray_tonic.get_all_online_users().await {
                for user in online_users {
                    sessions.push(ActiveSession {
                        id: format!("xray:{user}"),
                        client_id: Some(user.clone()),
                        username: Some(user),
                        inbound_tag: "xray".to_string(),
                        protocol: None,
                        source_ip: String::new(),
                        destination: None,
                        uptime_secs: 0,
                        upload: 0,
                        download: 0,
                    });
                }
            }
        }

        sessions
    }

    /// Terminates matching active sessions (`session_id`, `client_id`, or `source_ip`).
    pub async fn drop_session(&mut self, cmd: &DropSessionCmd) -> Result<usize, String> {
        let mut dropped = 0usize;
        if !self.singbox_running() {
            return Ok(0);
        }

        if let Some(ref sid) = cmd.session_id.as_ref().filter(|s| !s.trim().is_empty()) {
            drop_singbox_connection(&self.http_client, &self.config.singbox_api_addr, sid).await?;
            return Ok(1);
        }

        let sessions = self.get_sessions().await;
        for s in sessions {
            if s.id.starts_with("xray:") {
                continue;
            }
            let match_client = cmd
                .client_id
                .as_deref()
                .is_some_and(|cid| s.client_id.as_deref() == Some(cid));
            let match_ip = cmd
                .source_ip
                .as_deref()
                .is_some_and(|ip| s.source_ip == ip);
            if match_client || match_ip {
                if drop_singbox_connection(&self.http_client, &self.config.singbox_api_addr, &s.id)
                    .await
                    .is_ok()
                {
                    dropped += 1;
                }
            }
        }

        Ok(dropped)
    }

    /// Records a loud warning when an enabled inbound cannot start because its core binary is absent.
    async fn warn_missing_core(&self, core: &str, bin: &Path, tag: &str) {
        let msg = format!(
            "{core} binary not found at {}: inbound '{tag}' is NOT running. \
             Install the core (panel: Nodes -> Core update, or re-run install.sh)",
            bin.display()
        );
        tracing::warn!(core, inbound = tag, bin = %bin.display(), "core binary missing; inbound not started");
        self.logs.push(core, "warn", msg).await;
    }

    /// Builds a complete [`StatusReport`] including host telemetry, core PIDs, traffic capabilities, and traffic summary.
    pub async fn status_report(&mut self, include_traffic: bool) -> StatusReport {
        let xray_running = self.xray_running();
        let singbox_running = self.singbox_running();
        let naive_running = self.naive_running();
        let mieru_running = self.mieru_running();

        let xray_pid = if xray_running {
            self.xray_proc.as_ref().and_then(|p| p.pid)
        } else {
            None
        };
        let singbox_pid = if singbox_running {
            self.singbox_proc.as_ref().and_then(|p| p.pid)
        } else {
            None
        };
        let naive_pid = if naive_running {
            self.naive_procs.values().find_map(|p| p.pid)
        } else {
            None
        };
        let mieru_pid = if mieru_running {
            self.mieru_procs.values().find_map(|p| p.pid)
        } else {
            None
        };

        let traffic_summary = if include_traffic {
            let per_user = self.collect_traffic().await;
            let mut total = TrafficCounter {
                upload: 0,
                download: 0,
                unmetered: false,
            };
            for c in per_user.values() {
                total.upload = total.upload.saturating_add(c.upload);
                total.download = total.download.saturating_add(c.download);
            }
            Some(StatusTrafficSummary { total, per_user })
        } else {
            None
        };

        let proxy_user_count = traffic_summary
            .as_ref()
            .map(|t| t.per_user.len() as u64);
        if !self.core_versions_probed {
            self.core_versions = detect_core_versions(&self.config).await;
            self.core_versions_probed = true;
        }

        let host_telemetry = self.telemetry.sample(proxy_user_count).await;
        let connections = host_telemetry.connections.established_tcp.unwrap_or(0);
        let cpu_usage = host_telemetry.cpu.usage_percent;
        let mem_usage = host_telemetry.memory.usage_percent;

        StatusReport {
            status: "ONLINE".to_string(),
            xray_running,
            singbox_running,
            naive_running,
            mieru_running,
            xray_pid,
            singbox_pid,
            naive_pid,
            mieru_pid,
            uptime: self.telemetry.uptime_seconds(),
            version: AGENT_VERSION.to_string(),
            cpu_usage,
            mem_usage,
            connections,
            host_telemetry: Some(host_telemetry),
            traffic_capabilities: TrafficCapability::default_v4_capabilities(),
            cores: Some(serde_json::json!({
                "arch": std::env::consts::ARCH,
                "xray": {
                    "running": xray_running,
                    "version": self.core_versions.get("xray").cloned(),
                },
                "singbox": {
                    "running": singbox_running,
                    "version": self.core_versions.get("singbox").cloned(),
                },
                "naive": {
                    "running": naive_running,
                    "installed": binary_exists(&self.config.naive_bin).await,
                    "version": self.core_versions.get("naive").cloned(),
                },
                "mieru": {
                    "running": mieru_running,
                    "installed": binary_exists(&self.config.mieru_bin).await,
                    "version": self.core_versions.get("mieru").cloned(),
                },
            })),
            traffic: traffic_summary,
        }
    }
}

async fn probe_binary_version(bin: &Path, args: &[&str]) -> Option<String> {
    if !binary_exists(bin).await {
        return None;
    }
    let mut cmd = Command::new(bin);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear();
    if let Ok(path) = std::env::var("PATH") {
        cmd.env("PATH", path);
    }
    let out = tokio::time::timeout(Duration::from_secs(5), cmd.output())
        .await
        .ok()?
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    Some(combined)
}

async fn detect_core_versions(cfg: &AgentConfig) -> HashMap<String, String> {
    use crate::updater::{parse_version_output, CoreKind};

    let mut versions = HashMap::new();
    let (xv, sbv, nv, mv) = tokio::join!(
        probe_binary_version(&cfg.xray_bin, &["version"]),
        probe_binary_version(&cfg.singbox_bin, &["version"]),
        probe_binary_version(&cfg.naive_bin, &["version"]),
        probe_binary_version(&cfg.mieru_bin, &["version"]),
    );

    for (kind, out_opt) in [
        (CoreKind::Xray, xv),
        (CoreKind::Singbox, sbv),
        (CoreKind::Naive, nv),
        (CoreKind::Mieru, mv),
    ] {
        if let Some(combined) = out_opt {
            if let Some(ver) = parse_version_output(kind, &combined) {
                versions.insert(kind.canonical_name().to_string(), ver);
            }
        }
    }
    versions
}

async fn binary_exists(path: &Path) -> bool {
    tokio::fs::metadata(path).await.is_ok()
}

async fn run_check_command(
    bin: &Path,
    args: &[&str],
    extra_env: Vec<(String, String)>,
    core_name: &str,
) -> Result<(), String> {
    let mut cmd = Command::new(bin);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear();
    if let Ok(path) = std::env::var("PATH") {
        cmd.env("PATH", path);
    }
    for (k, v) in extra_env {
        cmd.env(k, v);
    }

    let out = tokio::time::timeout(Duration::from_secs(10), cmd.output())
        .await
        .map_err(|_| format!("{core_name} config validation timed out"))?
        .map_err(|e| format!("Failed to run {core_name} validation: {e}"))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        return Err(format!(
            "{core_name} config validation failed (exit {:?}): {} {}",
            out.status.code(),
            stderr.trim(),
            stdout.trim()
        ));
    }
    Ok(())
}

fn check_child_alive(slot: &mut Option<ManagedChild>) -> bool {
    let Some(mc) = slot.as_mut() else {
        return false;
    };
    match mc.child.try_wait() {
        Ok(None) => true,
        _ => {
            *slot = None;
            false
        }
    }
}

fn prune_dead_children(map: &mut HashMap<String, ManagedChild>) {
    map.retain(|_, mc| matches!(mc.child.try_wait(), Ok(None)));
}

async fn terminate_child(child: &mut Child) {
    #[cfg(unix)]
    if let Some(pid) = child.id() {
        unsafe {
            libc::kill(pid as i32, libc::SIGTERM);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = child.start_kill();
    }
    if tokio::time::timeout(Duration::from_secs(3), child.wait())
        .await
        .is_err()
    {
        let _ = child.kill().await;
    }
}
