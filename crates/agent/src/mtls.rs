use crate::config::AgentConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::WebPkiClientVerifier;
use rustls::{RootCertStore, ServerConfig};
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// Loads PEM bytes from either an inline PEM string (`-----BEGIN ...`) or a file path.
pub async fn load_pem_bytes(path_or_pem: &str) -> Result<Vec<u8>, String> {
    let trimmed = path_or_pem.trim();
    if trimmed.is_empty() {
        return Err("Empty PEM path or content".into());
    }
    if trimmed.contains("-----BEGIN ") {
        return Ok(trimmed.as_bytes().to_vec());
    }
    tokio::fs::read(Path::new(trimmed))
        .await
        .map_err(|e| format!("Failed to read PEM file {trimmed}: {e}"))
}

/// Parses PEM-encoded X.509 certificates.
pub fn parse_certs(pem: &[u8]) -> Result<Vec<CertificateDer<'static>>, String> {
    let mut reader = BufReader::new(pem);
    let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to parse PEM certificates: {e}"))?;
    if certs.is_empty() {
        return Err("No certificates found in PEM".into());
    }
    Ok(certs)
}

/// Parses the first PEM-encoded private key (PKCS#8, PKCS#1 RSA, or SEC1 EC).
pub fn parse_private_key(pem: &[u8]) -> Result<PrivateKeyDer<'static>, String> {
    let mut reader = BufReader::new(pem);
    rustls_pemfile::private_key(&mut reader)
        .map_err(|e| format!("Failed to parse PEM private key: {e}"))?
        .ok_or_else(|| "No private key found in PEM".to_string())
}

/// Builds a `rustls::ServerConfig` with optional mTLS client certificate verification.
pub fn build_server_tls_config(
    ca_pem: Option<&[u8]>,
    server_cert_pem: &[u8],
    server_key_pem: &[u8],
    require_client_cert: bool,
) -> Result<ServerConfig, String> {
    let certs = parse_certs(server_cert_pem)?;
    let key = parse_private_key(server_key_pem)?;

    let builder = ServerConfig::builder();
    let cfg = if require_client_cert {
        let ca = ca_pem.ok_or_else(|| "mTLS requires a CA certificate".to_string())?;
        let ca_certs = parse_certs(ca)?;
        let mut roots = RootCertStore::empty();
        for c in ca_certs {
            roots
                .add(c)
                .map_err(|e| format!("Failed to add CA cert to RootCertStore: {e}"))?;
        }
        let verifier = WebPkiClientVerifier::builder(Arc::new(roots))
            .build()
            .map_err(|e| format!("Failed to build mTLS client verifier: {e}"))?;
        builder
            .with_client_cert_verifier(verifier)
            .with_single_cert(certs, key)
            .map_err(|e| format!("Failed to build mTLS ServerConfig: {e}"))?
    } else {
        builder
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|e| format!("Failed to build TLS ServerConfig: {e}"))?
    };

    Ok(cfg)
}

/// Extracts the X.509 subject CommonName / string from a PEM certificate using `x509-parser`.
pub fn inspect_cert_subject(cert_pem: &[u8]) -> Result<String, String> {
    let certs = parse_certs(cert_pem)?;
    let (_, x509) = x509_parser::parse_x509_certificate(certs[0].as_ref())
        .map_err(|e| format!("Invalid X.509 certificate: {e}"))?;
    Ok(x509.subject().to_string())
}

/// Builds a `reqwest::Client` configured with optional mTLS client identity & CA trust.
pub async fn build_reqwest_client(cfg: &AgentConfig) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder().timeout(Duration::from_secs(10));

    if cfg.has_mtls() {
        if let Some(ref ca) = cfg.mtls_ca_cert {
            let ca_bytes = load_pem_bytes(ca).await?;
            let cert = reqwest::Certificate::from_pem(&ca_bytes)
                .map_err(|e| format!("Invalid MTLS_CA_CERT: {e}"))?;
            builder = builder.add_root_certificate(cert);
        }
        if let (Some(ref cert_src), Some(ref key_src)) =
            (&cfg.mtls_client_cert, &cfg.mtls_client_key)
        {
            let mut combined = load_pem_bytes(cert_src).await?;
            combined.push(b'\n');
            combined.extend_from_slice(&load_pem_bytes(key_src).await?);
            let identity = reqwest::Identity::from_pem(&combined)
                .map_err(|e| format!("Invalid MTLS client identity: {e}"))?;
            builder = builder.identity(identity);
        }
    }

    builder
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))
}

/// Builds a `tokio_tungstenite::Connector` with the configured mTLS CA and client identity.
pub async fn build_ws_tls_connector(
    cfg: &AgentConfig,
) -> Result<Option<tokio_tungstenite::Connector>, String> {
    if !cfg.has_mtls() {
        return Ok(None);
    }
    let mut roots = RootCertStore::empty();
    if let Some(ref ca) = cfg.mtls_ca_cert {
        let ca_bytes = load_pem_bytes(ca).await?;
        for cert in parse_certs(&ca_bytes)? {
            roots
                .add(cert)
                .map_err(|e| format!("Invalid MTLS_CA_CERT for WS: {e}"))?;
        }
    }
    let client_cfg = if let (Some(ref cert_src), Some(ref key_src)) =
        (&cfg.mtls_client_cert, &cfg.mtls_client_key)
    {
        let certs = parse_certs(&load_pem_bytes(cert_src).await?)?;
        let key = parse_private_key(&load_pem_bytes(key_src).await?)?;
        rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_client_auth_cert(certs, key)
            .map_err(|e| format!("Invalid MTLS client identity for WS: {e}"))?
    } else {
        rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth()
    };
    Ok(Some(tokio_tungstenite::Connector::Rustls(Arc::new(
        client_cfg,
    ))))
}
