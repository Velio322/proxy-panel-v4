use serde_json::{json, Value};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

#[derive(Debug)]
struct TelemetryState {
    started_at: Instant,
    prev_cpu: Option<(u64, u64)>,
    prev_net: Option<(Instant, u64, u64)>,
}

fn state() -> &'static Mutex<TelemetryState> {
    static STATE: OnceLock<Mutex<TelemetryState>> = OnceLock::new();
    STATE.get_or_init(|| {
        Mutex::new(TelemetryState {
            started_at: Instant::now(),
            prev_cpu: None,
            prev_net: None,
        })
    })
}

fn round2(val: f64) -> f64 {
    if !val.is_finite() {
        return 0.0;
    }
    (val * 100.0).round() / 100.0
}

/// Samples local panel host telemetry in the `HostTelemetry` contract expected by the UI.
pub async fn get_host_telemetry() -> Value {
    let cpu_count = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);

    let (started_at, had_prev_cpu) = {
        let guard = state().lock().unwrap_or_else(|e| e.into_inner());
        (guard.started_at, guard.prev_cpu.is_some())
    };

    if !had_prev_cpu {
        if let Some(first) = read_proc_stat_cpu().await {
            let mut guard = state().lock().unwrap_or_else(|e| e.into_inner());
            if guard.prev_cpu.is_none() {
                guard.prev_cpu = Some(first);
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let loadavg = read_loadavg().await;
    let cur_cpu = read_proc_stat_cpu().await;
    let cpu_usage = {
        let mut guard = state().lock().unwrap_or_else(|e| e.into_inner());
        let pct = match (guard.prev_cpu, cur_cpu) {
            (Some((prev_idle, prev_total)), Some((idle, total))) => {
                let d_total = total.saturating_sub(prev_total);
                let d_idle = idle.saturating_sub(prev_idle);
                if d_total > 0 {
                    Some(((1.0 - (d_idle as f64 / d_total as f64)) * 100.0).clamp(0.0, 100.0))
                } else {
                    None
                }
            }
            _ => None,
        };
        if let Some(sample) = cur_cpu {
            guard.prev_cpu = Some(sample);
        }
        pct.unwrap_or_else(|| ((loadavg[0] / cpu_count.max(1) as f64) * 100.0).clamp(0.0, 100.0))
    };

    let mem = read_meminfo().await;
    let host_uptime = read_uptime().await;
    let process_uptime = started_at.elapsed().as_secs_f64();
    let established_tcp = read_tcp_established_count().await;
    let udp_sockets = read_udp_sockets_count().await;
    let (rx_bytes, tx_bytes) = read_netdev().await;

    let (upload_bps, download_bps) = {
        let now = Instant::now();
        let mut guard = state().lock().unwrap_or_else(|e| e.into_inner());
        let rates = match guard.prev_net {
            Some((prev_t, prev_rx, prev_tx)) => {
                let dt = now.duration_since(prev_t).as_secs_f64();
                if dt > 0.0 && dt <= 120.0 && rx_bytes >= prev_rx && tx_bytes >= prev_tx {
                    (
                        round2((tx_bytes - prev_tx) as f64 / dt),
                        round2((rx_bytes - prev_rx) as f64 / dt),
                    )
                } else {
                    (0.0, 0.0)
                }
            }
            None => (0.0, 0.0),
        };
        guard.prev_net = Some((now, rx_bytes, tx_bytes));
        rates
    };

    let now_iso = crate::db::ts::format_iso(&crate::db::now());

    json!({
        "sampledAt": now_iso,
        "timestamp": now_iso,
        "scope": "host-or-container-namespace",
        "platform": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "uptimeSeconds": round2(host_uptime),
        "loadAvg": loadavg,
        "cpu": {
            "count": cpu_count,
            "usagePercent": round2(cpu_usage),
            "loadAverage": loadavg
        },
        "memory": mem,
        "uptime": {
            "hostSeconds": round2(host_uptime),
            "processSeconds": round2(process_uptime)
        },
        "connections": {
            "establishedTcp": established_tcp,
            "udpSockets": udp_sockets,
            "scope": "network-namespace"
        },
        "network": {
            "scope": "network-namespace; all non-loopback interfaces",
            "rxBytes": rx_bytes,
            "txBytes": tx_bytes,
            "receivedBytes": rx_bytes,
            "transmittedBytes": tx_bytes,
            "uploadBps": upload_bps,
            "downloadBps": download_bps
        }
    })
}

async fn read_proc_stat_cpu() -> Option<(u64, u64)> {
    let raw = tokio::fs::read_to_string("/proc/stat").await.ok()?;
    let line = raw.lines().find(|l| l.starts_with("cpu "))?;
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

async fn read_uptime() -> f64 {
    let Ok(raw) = tokio::fs::read_to_string("/proc/uptime").await else {
        return 0.0;
    };
    raw.split_whitespace()
        .next()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0)
}

async fn read_loadavg() -> [f64; 3] {
    let Ok(raw) = tokio::fs::read_to_string("/proc/loadavg").await else {
        return [0.0, 0.0, 0.0];
    };
    let mut parts = raw.split_whitespace();
    let l1 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let l5 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let l15 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0.0);
    [l1, l5, l15]
}

async fn read_meminfo() -> Value {
    let Ok(raw) = tokio::fs::read_to_string("/proc/meminfo").await else {
        return json!({
            "totalBytes": 0,
            "freeBytes": 0,
            "availableBytes": 0,
            "usedBytes": 0,
            "usagePercent": 0.0,
            "usedPercent": 0.0
        });
    };
    let mut total_kb: Option<u64> = None;
    let mut avail_kb: Option<u64> = None;
    let mut free_kb: Option<u64> = None;
    for line in raw.lines() {
        let mut parts = line.split_whitespace();
        let Some(key) = parts.next() else {
            continue;
        };
        let Some(val) = parts.next().and_then(|s| s.parse::<u64>().ok()) else {
            continue;
        };
        match key {
            "MemTotal:" => total_kb = Some(val),
            "MemAvailable:" => avail_kb = Some(val),
            "MemFree:" => free_kb = Some(val),
            _ => {}
        }
    }
    let total_bytes = total_kb.unwrap_or(0).saturating_mul(1024);
    let avail_bytes = avail_kb.or(free_kb).unwrap_or(0).saturating_mul(1024);
    let used_bytes = total_bytes.saturating_sub(avail_bytes);
    let used_pct = if total_bytes > 0 {
        round2((used_bytes as f64 / total_bytes as f64) * 100.0)
    } else {
        0.0
    };
    json!({
        "totalBytes": total_bytes,
        "freeBytes": avail_bytes,
        "availableBytes": avail_bytes,
        "usedBytes": used_bytes,
        "usagePercent": used_pct,
        "usedPercent": used_pct
    })
}

async fn read_tcp_established_count() -> u64 {
    let mut total = 0u64;
    for path in ["/proc/net/tcp", "/proc/net/tcp6"] {
        if let Ok(raw) = tokio::fs::read_to_string(path).await {
            total = total.saturating_add(
                raw.lines()
                    .skip(1)
                    .filter(|line| line.split_whitespace().nth(3) == Some("01"))
                    .count() as u64,
            );
        }
    }
    total
}

async fn read_udp_sockets_count() -> u64 {
    let mut total = 0u64;
    for path in ["/proc/net/udp", "/proc/net/udp6"] {
        if let Ok(raw) = tokio::fs::read_to_string(path).await {
            total = total.saturating_add(
                raw.lines()
                    .skip(1)
                    .filter(|line| !line.trim().is_empty())
                    .count() as u64,
            );
        }
    }
    total
}

async fn read_netdev() -> (u64, u64) {
    let Ok(raw) = tokio::fs::read_to_string("/proc/net/dev").await else {
        return (0, 0);
    };
    let mut rx_total = 0u64;
    let mut tx_total = 0u64;
    for line in raw.lines().skip(2) {
        let Some((iface, stats)) = line.split_once(':') else {
            continue;
        };
        if iface.trim() == "lo" {
            continue;
        }
        let nums: Vec<u64> = stats
            .split_whitespace()
            .filter_map(|s| s.parse::<u64>().ok())
            .collect();
        if nums.len() >= 9 {
            rx_total = rx_total.saturating_add(nums[0]);
            tx_total = tx_total.saturating_add(nums[8]);
        }
    }
    (rx_total, tx_total)
}
