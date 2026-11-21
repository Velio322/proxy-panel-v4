use chrono::Utc;
use mypanel_proto::{
    ConnectionsTelemetry, CpuTelemetry, HostTelemetry, MemoryTelemetry, NetworkTelemetry,
    ProcessMemoryTelemetry, ProcessTelemetry, UptimeTelemetry,
};
use std::time::Instant;

/// Samples host & agent process telemetry (`HostTelemetry`) from `/proc` on Linux
/// with graceful fallbacks on non-Linux hosts.
#[derive(Debug)]
pub struct TelemetryCollector {
    started_at: Instant,
    prev_cpu: Option<(u64, u64)>,
    prev_net: Option<(Instant, u64, u64)>,
}

impl Default for TelemetryCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl TelemetryCollector {
    pub fn new() -> Self {
        Self {
            started_at: Instant::now(),
            prev_cpu: None,
            prev_net: None,
        }
    }

    pub fn uptime_seconds(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }

    /// Collects a full [`HostTelemetry`] snapshot.
    pub async fn sample(&mut self, proxy_user_count: Option<u64>) -> HostTelemetry {
        let cpu_count = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);

        let cpu_usage = match tokio::fs::read_to_string("/proc/stat").await {
            Ok(content) => {
                if let Some((idle, total)) = parse_proc_stat_cpu(&content) {
                    let pct = self.prev_cpu.and_then(|(prev_idle, prev_total)| {
                        let d_total = total.saturating_sub(prev_total);
                        let d_idle = idle.saturating_sub(prev_idle);
                        if d_total == 0 {
                            None
                        } else {
                            let used_ratio = 1.0 - (d_idle as f64 / d_total as f64);
                            Some((used_ratio * 100.0).clamp(0.0, 100.0))
                        }
                    });
                    self.prev_cpu = Some((idle, total));
                    pct
                } else {
                    None
                }
            }
            Err(_) => None,
        };

        let load_average = match tokio::fs::read_to_string("/proc/loadavg").await {
            Ok(content) => parse_proc_loadavg(&content),
            Err(_) => [0.0, 0.0, 0.0],
        };

        let memory = match tokio::fs::read_to_string("/proc/meminfo").await {
            Ok(content) => parse_proc_meminfo(&content),
            Err(_) => MemoryTelemetry {
                total_bytes: 0,
                free_bytes: 0,
                used_bytes: 0,
                usage_percent: None,
            },
        };

        let host_seconds = match tokio::fs::read_to_string("/proc/uptime").await {
            Ok(content) => parse_proc_uptime(&content),
            Err(_) => 0.0,
        };
        let process_seconds = self.started_at.elapsed().as_secs_f64();

        let tcp_established = read_tcp_established_count().await;
        let udp_sockets = read_udp_sockets_count().await;

        let network = self.sample_network().await;

        let rss = match tokio::fs::read_to_string("/proc/self/statm").await {
            Ok(content) => parse_proc_self_statm_rss(&content),
            Err(_) => 0,
        };

        HostTelemetry {
            sampled_at: Utc::now().to_rfc3339(),
            scope: "host-or-container-namespace".to_string(),
            cpu: CpuTelemetry {
                count: cpu_count,
                usage_percent: cpu_usage,
                load_average,
            },
            memory,
            uptime: UptimeTelemetry {
                host_seconds,
                process_seconds,
            },
            connections: ConnectionsTelemetry {
                established_tcp: tcp_established,
                udp_sockets,
                scope: "network-namespace".to_string(),
                proxy_user_count,
            },
            network,
            process: ProcessTelemetry {
                memory: ProcessMemoryTelemetry {
                    rss,
                    heap_total: 0,
                    heap_used: 0,
                    external: 0,
                    array_buffers: 0,
                },
                pid: std::process::id(),
            },
        }
    }

    async fn sample_network(&mut self) -> NetworkTelemetry {
        let Ok(content) = tokio::fs::read_to_string("/proc/net/dev").await else {
            self.prev_net = None;
            return NetworkTelemetry {
                scope: "network-namespace; all non-loopback interfaces".into(),
                received_bytes: None,
                transmitted_bytes: None,
                upload_bps: None,
                download_bps: None,
            };
        };

        let Some((rx, tx)) = parse_proc_net_dev(&content) else {
            self.prev_net = None;
            return NetworkTelemetry {
                scope: "network-namespace; all non-loopback interfaces".into(),
                received_bytes: None,
                transmitted_bytes: None,
                upload_bps: None,
                download_bps: None,
            };
        };

        let now = Instant::now();
        let (upload_bps, download_bps) = match self.prev_net {
            Some((prev_t, prev_rx, prev_tx)) => {
                let dt = now.duration_since(prev_t).as_secs_f64();
                if dt > 0.0 && dt <= 120.0 && rx >= prev_rx && tx >= prev_tx {
                    let up = (tx - prev_tx) as f64 / dt;
                    let down = (rx - prev_rx) as f64 / dt;
                    (Some(up), Some(down))
                } else {
                    (None, None)
                }
            }
            None => (None, None),
        };
        self.prev_net = Some((now, rx, tx));

        NetworkTelemetry {
            scope: "network-namespace; all non-loopback interfaces".into(),
            received_bytes: Some(rx),
            transmitted_bytes: Some(tx),
            upload_bps,
            download_bps,
        }
    }
}

async fn read_tcp_established_count() -> Option<u64> {
    let mut total = 0u64;
    let mut any_ok = false;
    for path in ["/proc/net/tcp", "/proc/net/tcp6"] {
        if let Ok(content) = tokio::fs::read_to_string(path).await {
            total = total.saturating_add(parse_proc_net_tcp_established(&content));
            any_ok = true;
        }
    }
    if any_ok {
        Some(total)
    } else {
        None
    }
}

async fn read_udp_sockets_count() -> Option<u64> {
    let mut total = 0u64;
    let mut any_ok = false;
    for path in ["/proc/net/udp", "/proc/net/udp6"] {
        if let Ok(content) = tokio::fs::read_to_string(path).await {
            total = total.saturating_add(parse_proc_net_udp_count(&content));
            any_ok = true;
        }
    }
    if any_ok {
        Some(total)
    } else {
        None
    }
}

/// Parses the aggregate `cpu` line from `/proc/stat`, returning `(idle_ticks, total_ticks)`.
pub fn parse_proc_stat_cpu(content: &str) -> Option<(u64, u64)> {
    let line = content.lines().find(|l| l.starts_with("cpu "))?;
    let nums: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|s| s.parse::<u64>().ok())
        .collect();
    if nums.len() < 4 {
        return None;
    }
    let idle = nums[3].saturating_add(nums.get(4).copied().unwrap_or(0));
    let total: u64 = nums.iter().copied().sum();
    Some((idle, total))
}

/// Parses 1m, 5m, 15m load averages from `/proc/loadavg`.
pub fn parse_proc_loadavg(content: &str) -> [f64; 3] {
    let mut parts = content.split_whitespace();
    let l1 = parts.next().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
    let l5 = parts.next().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
    let l15 = parts.next().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
    [l1, l5, l15]
}

/// Parses `/proc/meminfo` into [`MemoryTelemetry`].
pub fn parse_proc_meminfo(content: &str) -> MemoryTelemetry {
    let mut mem_total_kb = None;
    let mut mem_avail_kb = None;
    let mut mem_free_kb = None;

    for line in content.lines() {
        let mut parts = line.split_whitespace();
        let Some(key) = parts.next() else {
            continue;
        };
        let Some(val) = parts.next().and_then(|s| s.parse::<u64>().ok()) else {
            continue;
        };
        match key {
            "MemTotal:" => mem_total_kb = Some(val),
            "MemAvailable:" => mem_avail_kb = Some(val),
            "MemFree:" => mem_free_kb = Some(val),
            _ => {}
        }
    }

    let total_bytes = mem_total_kb.unwrap_or(0).saturating_mul(1024);
    let free_bytes = mem_avail_kb
        .or(mem_free_kb)
        .unwrap_or(0)
        .saturating_mul(1024);
    let used_bytes = total_bytes.saturating_sub(free_bytes);
    let usage_percent = if total_bytes > 0 {
        Some((used_bytes as f64 / total_bytes as f64) * 100.0)
    } else {
        None
    };

    MemoryTelemetry {
        total_bytes,
        free_bytes,
        used_bytes,
        usage_percent,
    }
}

/// Parses host uptime seconds from `/proc/uptime`.
pub fn parse_proc_uptime(content: &str) -> f64 {
    content
        .split_whitespace()
        .next()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0)
}

/// Counts `ESTABLISHED` (`01`) TCP sockets in `/proc/net/tcp` or `/proc/net/tcp6`.
pub fn parse_proc_net_tcp_established(content: &str) -> u64 {
    content
        .lines()
        .skip(1)
        .filter(|line| line.split_whitespace().nth(3) == Some("01"))
        .count() as u64
}

/// Counts UDP sockets in `/proc/net/udp` or `/proc/net/udp6`.
pub fn parse_proc_net_udp_count(content: &str) -> u64 {
    content
        .lines()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .count() as u64
}

/// Sums `(rx_bytes, tx_bytes)` across all non-loopback interfaces in `/proc/net/dev`.
pub fn parse_proc_net_dev(content: &str) -> Option<(u64, u64)> {
    let mut rx_total = 0u64;
    let mut tx_total = 0u64;
    let mut saw_any = false;

    for line in content.lines().skip(2) {
        let Some((iface, rest)) = line.split_once(':') else {
            continue;
        };
        if iface.trim() == "lo" {
            continue;
        }
        let fields: Vec<u64> = rest
            .split_whitespace()
            .filter_map(|s| s.parse::<u64>().ok())
            .collect();
        if fields.len() < 16 {
            continue;
        }
        rx_total = rx_total.saturating_add(fields[0]);
        tx_total = tx_total.saturating_add(fields[8]);
        saw_any = true;
    }

    if saw_any {
        Some((rx_total, tx_total))
    } else {
        Some((0, 0))
    }
}

fn parse_proc_self_statm_rss(content: &str) -> u64 {
    let pages = content
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    pages.saturating_mul(4096)
}
