//! `API_MODE=pull`: the panel polls this node over mutual-TLS HTTPS, like Remnawave does.
//!
//! The agent has no persistent session to the panel. At start it registers once (and every
//! [`RENEW_EVERY`] afterwards) to obtain a server certificate for its own key, then only serves
//! requests. A panel restart therefore changes nothing on the node.

use crate::config::{AgentConfig, AGENT_VERSION};
use crate::cores::write_atomic_0600;
use crate::tls_server::{install_config, load_server_config, SharedTlsConfig};
use mypanel_proto::{RegisterRequest, RegisterSystemInfo};
use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair, SanType};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Certificates are valid 365 days; refreshing twice a day keeps a wide safety margin.
const RENEW_EVERY: Duration = Duration::from_secs(12 * 3600);
const BACKOFF: [Duration; 4] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(15),
];

/// Loads the node TLS key, generating and persisting a new one (mode `0600`) on first run.
pub async fn load_or_create_key(cfg: &AgentConfig) -> Result<KeyPair, String> {
    let path = cfg.tls_key_path();
    if let Ok(pem) = tokio::fs::read_to_string(&path).await {
        if let Ok(key) = KeyPair::from_pem(&pem) {
            return Ok(key);
        }
        tracing::warn!(path = %path.display(), "unreadable node TLS key, generating a new one");
    }
    let key = KeyPair::generate().map_err(|e| format!("TLS key generation failed: {e}"))?;
    write_atomic_0600(&path, key.serialize_pem().as_bytes()).await?;
    Ok(key)
}

/// Builds a PKCS#10 request for `key`. Only the public key matters: the panel chooses the subject
/// and usages itself.
pub fn build_csr(cfg: &AgentConfig, key: &KeyPair) -> Result<String, String> {
    let mut params = CertificateParams::default();
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, cfg.node_name.as_str());
    params.distinguished_name = dn;
    let host = cfg.node_host.trim();
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        params.subject_alt_names.push(SanType::IpAddress(ip));
    } else if let Ok(ia5) = host.to_string().try_into() {
        params.subject_alt_names.push(SanType::DnsName(ia5));
    }
    params
        .serialize_request(key)
        .map_err(|e| format!("CSR creation failed: {e}"))?
        .pem()
        .map_err(|e| format!("CSR PEM encoding failed: {e}"))
}

/// Registers with the panel in pull mode and stores the returned certificate and panel CA.
pub async fn register_and_store(
    client: &reqwest::Client,
    cfg: &AgentConfig,
) -> Result<(), String> {
    let key = load_or_create_key(cfg).await?;
    let payload = RegisterRequest {
        token: cfg.node_secret.clone(),
        name: Some(cfg.node_name.clone()),
        host: Some(cfg.node_host.clone()),
        port: Some(443),
        api_port: Some(cfg.worker_port),
        system: Some(RegisterSystemInfo {
            release: AGENT_VERSION.to_string(),
        }),
        csr_pem: Some(build_csr(cfg, &key)?),
        api_mode: Some("pull".to_string()),
    };
    let url = format!("{}/api/v1/nodes/self/register", cfg.master_url);
    let resp = client
        .post(&url)
        .json(&payload)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("register request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("register failed with HTTP {}", resp.status()));
    }
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("register response is not JSON: {e}"))?;
    let cert = body
        .get("tlsCertPem")
        .and_then(|v| v.as_str())
        .filter(|s| s.contains("BEGIN CERTIFICATE"))
        .ok_or("panel did not return a TLS certificate (panel too old for API_MODE=pull?)")?;
    let ca = body
        .get("caPem")
        .and_then(|v| v.as_str())
        .filter(|s| s.contains("BEGIN CERTIFICATE"))
        .ok_or("panel did not return its CA certificate")?;
    write_atomic_0600(&cfg.tls_cert_path(), cert.as_bytes()).await?;
    write_atomic_0600(&cfg.panel_ca_path(), ca.as_bytes()).await?;
    Ok(())
}

/// Obtains and installs the certificate, retrying with backoff, then renews it periodically.
/// A cached certificate from a previous run is used immediately so the node serves right away
/// even when the panel is down at boot.
pub async fn run_identity_loop(
    cfg: AgentConfig,
    client: reqwest::Client,
    slot: SharedTlsConfig,
    cancel: CancellationToken,
) {
    if let Ok(cached) = load_server_config(&cfg).await {
        install_config(&slot, cached);
        tracing::info!("loaded cached node TLS certificate");
    }

    let mut failures = 0usize;
    loop {
        let wait = match register_and_store(&client, &cfg).await {
            Ok(()) => match load_server_config(&cfg).await {
                Ok(sc) => {
                    install_config(&slot, sc);
                    tracing::info!("node TLS certificate installed (pull mode)");
                    failures = 0;
                    RENEW_EVERY
                }
                Err(e) => {
                    tracing::warn!(error = %e, "stored TLS material is unusable");
                    BACKOFF[failures.min(BACKOFF.len() - 1)]
                }
            },
            Err(e) => {
                tracing::warn!(error = %e, "pull-mode registration failed; retrying");
                let d = BACKOFF[failures.min(BACKOFF.len() - 1)];
                failures = failures.saturating_add(1);
                d
            }
        };
        tokio::select! {
            _ = cancel.cancelled() => return,
            _ = tokio::time::sleep(wait) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn key_is_created_once_and_reused() {
        let dir = std::env::temp_dir().join(format!("mypanel3-pull-{}", uuid::Uuid::new_v4()));
        let cfg = AgentConfig::for_tests(&dir);
        let a = load_or_create_key(&cfg).await.unwrap();
        let b = load_or_create_key(&cfg).await.unwrap();
        assert_eq!(a.serialize_pem(), b.serialize_pem());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(cfg.tls_key_path()).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn csr_contains_our_public_key_and_host_san() {
        let dir = std::env::temp_dir().join(format!("mypanel3-pull-{}", uuid::Uuid::new_v4()));
        let mut cfg = AgentConfig::for_tests(&dir);
        cfg.node_host = "node.example.com".into();
        let key = load_or_create_key(&cfg).await.unwrap();
        let csr = build_csr(&cfg, &key).unwrap();
        assert!(csr.contains("BEGIN CERTIFICATE REQUEST"));
        let parsed = rcgen::CertificateSigningRequestParams::from_pem(&csr).unwrap();
        assert!(parsed
            .params
            .subject_alt_names
            .iter()
            .any(|s| matches!(s, SanType::DnsName(d) if d.as_str() == "node.example.com")));
        let _ = std::fs::remove_dir_all(dir);
    }
}
