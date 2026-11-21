use dashmap::DashMap;
use std::sync::atomic::{AtomicU64, Ordering};

const BUCKETS: [f64; 6] = [0.01, 0.05, 0.1, 0.5, 1.0, 5.0];

#[derive(Debug, Default)]
struct Hist {
    buckets: [AtomicU64; 6],
    count: AtomicU64,
    sum_micros: AtomicU64,
}

#[derive(Debug, Default)]
pub struct MetricsRegistry {
    requests_total: DashMap<(String, String, u16), u64>,
    durations: DashMap<(String, String), Hist>,
    node_status: DashMap<(String, String), i64>,
    node_cpu: DashMap<String, f64>,
    node_mem: DashMap<String, f64>,
}

impl MetricsRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn observe_http(&self, method: &str, route: &str, status: u16, elapsed_secs: f64) {
        let mut count_entry = self
            .requests_total
            .entry((method.to_string(), route.to_string(), status))
            .or_insert(0);
        *count_entry = count_entry.saturating_add(1);
        drop(count_entry);

        let hist = self
            .durations
            .entry((method.to_string(), route.to_string()))
            .or_default();
        for (i, b) in BUCKETS.iter().enumerate() {
            if elapsed_secs <= *b {
                hist.buckets[i].fetch_add(1, Ordering::Relaxed);
            }
        }
        hist.count.fetch_add(1, Ordering::Relaxed);
        let micros = (elapsed_secs.max(0.0) * 1_000_000.0) as u64;
        hist.sum_micros.fetch_add(micros, Ordering::Relaxed);
    }

    pub fn set_node_status(&self, node_id: &str, node_name: &str, online: bool) {
        self.node_status.insert(
            (node_id.to_string(), node_name.to_string()),
            if online { 1 } else { 0 },
        );
    }

    pub fn set_node_telemetry(&self, node_id: &str, cpu: f64, mem: f64) {
        self.node_cpu.insert(node_id.to_string(), cpu);
        self.node_mem.insert(node_id.to_string(), mem);
    }

    pub fn render(&self, uptime_secs: f64) -> String {
        let mut out = String::with_capacity(4096);
        out.push_str("# HELP proxpanel_process_uptime_seconds Process uptime in seconds\n");
        out.push_str("# TYPE proxpanel_process_uptime_seconds gauge\n");
        out.push_str(&format!("proxpanel_process_uptime_seconds {uptime_secs:.3}\n"));

        out.push_str("# HELP proxpanel_http_requests_total Total HTTP requests\n");
        out.push_str("# TYPE proxpanel_http_requests_total counter\n");
        for item in self.requests_total.iter() {
            let (method, route, status) = item.key();
            let val = *item.value();
            out.push_str(&format!(
                "proxpanel_http_requests_total{{method=\"{}\",route=\"{}\",status=\"{}\"}} {}\n",
                esc_label(method),
                esc_label(route),
                status,
                val
            ));
        }

        out.push_str("# HELP proxpanel_http_request_duration_seconds HTTP request duration\n");
        out.push_str("# TYPE proxpanel_http_request_duration_seconds histogram\n");
        for item in self.durations.iter() {
            let (method, route) = item.key();
            let h = item.value();
            let m = esc_label(method);
            let r = esc_label(route);
            for (i, b) in BUCKETS.iter().enumerate() {
                let c = h.buckets[i].load(Ordering::Relaxed);
                out.push_str(&format!(
                    "proxpanel_http_request_duration_seconds_bucket{{method=\"{m}\",route=\"{r}\",le=\"{b}\"}} {c}\n"
                ));
            }
            let total = h.count.load(Ordering::Relaxed);
            let sum_secs = (h.sum_micros.load(Ordering::Relaxed) as f64) / 1_000_000.0;
            out.push_str(&format!(
                "proxpanel_http_request_duration_seconds_bucket{{method=\"{m}\",route=\"{r}\",le=\"+Inf\"}} {total}\n"
            ));
            out.push_str(&format!(
                "proxpanel_http_request_duration_seconds_sum{{method=\"{m}\",route=\"{r}\"}} {sum_secs:.6}\n"
            ));
            out.push_str(&format!(
                "proxpanel_http_request_duration_seconds_count{{method=\"{m}\",route=\"{r}\"}} {total}\n"
            ));
        }

        out.push_str("# HELP proxpanel_node_status Node status (1=online, 0=offline)\n");
        out.push_str("# TYPE proxpanel_node_status gauge\n");
        for item in self.node_status.iter() {
            let (nid, nname) = item.key();
            let v = *item.value();
            out.push_str(&format!(
                "proxpanel_node_status{{node_id=\"{}\",node_name=\"{}\"}} {}\n",
                esc_label(nid),
                esc_label(nname),
                v
            ));
        }

        out.push_str("# HELP proxpanel_node_cpu_usage Node CPU usage percentage\n");
        out.push_str("# TYPE proxpanel_node_cpu_usage gauge\n");
        for item in self.node_cpu.iter() {
            out.push_str(&format!(
                "proxpanel_node_cpu_usage{{node_id=\"{}\"}} {:.2}\n",
                esc_label(item.key()),
                *item.value()
            ));
        }

        out.push_str("# HELP proxpanel_node_memory_usage Node memory usage percentage\n");
        out.push_str("# TYPE proxpanel_node_memory_usage gauge\n");
        for item in self.node_mem.iter() {
            out.push_str(&format!(
                "proxpanel_node_memory_usage{{node_id=\"{}\"}} {:.2}\n",
                esc_label(item.key()),
                *item.value()
            ));
        }

        // Declared compatibility metrics (01-data-and-services.md §9)
        out.push_str("# HELP proxpanel_active_connections Active proxy connections\n");
        out.push_str("# TYPE proxpanel_active_connections gauge\n");
        out.push_str("# HELP proxpanel_traffic_bytes_total Total traffic in bytes\n");
        out.push_str("# TYPE proxpanel_traffic_bytes_total counter\n");
        out.push_str("# HELP proxpanel_clients_total Total active clients\n");
        out.push_str("# TYPE proxpanel_clients_total gauge\n");
        out.push_str("# HELP proxpanel_subscriptions_active Active subscriptions\n");
        out.push_str("# TYPE proxpanel_subscriptions_active gauge\n");
        out.push_str("# HELP proxpanel_revenue_total Total revenue\n");
        out.push_str("# TYPE proxpanel_revenue_total counter\n");
        out
    }
}

fn esc_label(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}
