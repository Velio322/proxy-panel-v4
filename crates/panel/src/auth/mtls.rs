use crate::auth::crypto::sha256_bytes_hex;
use crate::error::{AppError, AppResult};
use chrono::{Datelike, Duration as ChronoDuration, NaiveDateTime, Utc};
use rcgen::{
    BasicConstraints, CertificateParams, CertificateSigningRequestParams, DistinguishedName, DnType,
    ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair, KeyUsagePurpose, SanType,
};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use x509_parser::prelude::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssuedNodeCert {
    pub node_id: String,
    pub cert_pem: String,
    pub key_pem: String,
    pub ca_pem: String,
    pub serial: String,
    pub fingerprint: String,
    pub subject: String,
    pub expires_at: NaiveDateTime,
}

#[derive(Clone)]
pub struct MtlsCa {
    ca_cert_pem: String,
    ca_key_pem: String,
    ca_fingerprint: String,
    issuer: Arc<Issuer<'static, KeyPair>>,
}

impl std::fmt::Debug for MtlsCa {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MtlsCa")
            .field("ca_fingerprint", &self.ca_fingerprint)
            .finish()
    }
}

fn write_secret_file(path: &Path, content: &str) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        f.write_all(content.as_bytes())?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, content)
    }
}

fn set_validity(params: &mut CertificateParams, days: i64) {
    let today = Utc::now().date_naive();
    let start = today - ChronoDuration::days(1);
    let end = today + ChronoDuration::days(days);
    params.not_before = rcgen::date_time_ymd(start.year(), start.month() as u8, start.day() as u8);
    params.not_after = rcgen::date_time_ymd(end.year(), end.month() as u8, end.day() as u8);
}

impl MtlsCa {
    /// Generates a new internal Root CA in memory (or loads/persists to `data_dir/mtls-ca.{crt,key}` when possible).
    pub fn load_or_generate(data_dir: Option<&Path>) -> AppResult<Self> {
        if let Some(dir) = data_dir {
            let cert_path = dir.join("mtls-ca.crt");
            let key_path = dir.join("mtls-ca.key");
            if let (Ok(cert_pem), Ok(key_pem)) = (
                std::fs::read_to_string(&cert_path),
                std::fs::read_to_string(&key_path),
            ) {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(
                        &key_path,
                        std::fs::Permissions::from_mode(0o600),
                    );
                }
                if let Ok(loaded) = Self::from_pem(&cert_pem, &key_pem) {
                    return Ok(loaded);
                }
            }
        }

        let generated = Self::generate_ephemeral()?;
        if let Some(dir) = data_dir {
            if std::fs::create_dir_all(dir).is_ok() {
                let _ = std::fs::write(dir.join("mtls-ca.crt"), &generated.ca_cert_pem);
                let _ = write_secret_file(&dir.join("mtls-ca.key"), &generated.ca_key_pem);
            }
        }
        Ok(generated)
    }

    pub fn generate_ephemeral() -> AppResult<Self> {
        let key_pair = KeyPair::generate()
            .map_err(|e| AppError::internal(format!("mTLS CA keygen failed: {e}")))?;
        let mut params = CertificateParams::default();
        set_validity(&mut params, 3650);
        let mut dn = DistinguishedName::new();
        dn.push(DnType::OrganizationName, "ProxyPanel v4 Internal CA");
        dn.push(DnType::CommonName, "mypanel3-internal-root-ca");
        params.distinguished_name = dn;
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
            KeyUsagePurpose::DigitalSignature,
        ];

        let ca_cert = params
            .self_signed(&key_pair)
            .map_err(|e| AppError::internal(format!("mTLS CA self-sign failed: {e}")))?;
        let ca_cert_pem = ca_cert.pem();
        let ca_key_pem = key_pair.serialize_pem();
        let ca_fingerprint = sha256_bytes_hex(ca_cert.der());
        let issuer = Arc::new(Issuer::new(params, key_pair));

        Ok(Self {
            ca_cert_pem,
            ca_key_pem,
            ca_fingerprint,
            issuer,
        })
    }

    pub fn from_pem(ca_cert_pem: &str, ca_key_pem: &str) -> AppResult<Self> {
        let key_pair = KeyPair::from_pem(ca_key_pem)
            .map_err(|e| AppError::internal(format!("invalid CA key PEM: {e}")))?;
        let issuer = Issuer::from_ca_cert_pem(ca_cert_pem, key_pair)
            .map_err(|e| AppError::internal(format!("invalid CA cert PEM: {e}")))?;
        let der = pem_to_der(ca_cert_pem)?;
        let ca_fingerprint = sha256_bytes_hex(&der);
        Ok(Self {
            ca_cert_pem: ca_cert_pem.to_string(),
            ca_key_pem: ca_key_pem.to_string(),
            ca_fingerprint,
            issuer: Arc::new(issuer),
        })
    }

    pub fn ca_pem(&self) -> &str {
        &self.ca_cert_pem
    }

    pub fn ca_fingerprint(&self) -> &str {
        &self.ca_fingerprint
    }

    /// Issues a signed X.509 client/server leaf certificate for a node.
    pub fn issue_node_cert(&self, node_id: &str, node_host: &str) -> AppResult<IssuedNodeCert> {
        let leaf_key = KeyPair::generate()
            .map_err(|e| AppError::internal(format!("leaf keygen failed: {e}")))?;
        let mut params = CertificateParams::default();
        set_validity(&mut params, 365);
        let subject_cn = format!("node:{node_id}");
        let mut dn = DistinguishedName::new();
        dn.push(DnType::OrganizationName, "ProxyPanel v4 Node");
        dn.push(DnType::CommonName, subject_cn.clone());
        params.distinguished_name = dn;
        params.is_ca = IsCa::NoCa;
        params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyEncipherment,
        ];
        params.extended_key_usages = vec![
            ExtendedKeyUsagePurpose::ClientAuth,
            ExtendedKeyUsagePurpose::ServerAuth,
        ];

        let clean_host = node_host
            .trim()
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .split('/')
            .next()
            .unwrap_or("")
            .trim();
        if !clean_host.is_empty() {
            if let Ok(ip) = clean_host.parse::<std::net::IpAddr>() {
                params.subject_alt_names.push(SanType::IpAddress(ip));
            } else if let Ok(ia5) = clean_host.to_string().try_into() {
                params.subject_alt_names.push(SanType::DnsName(ia5));
            }
        }

        let cert = params
            .signed_by(&leaf_key, &self.issuer)
            .map_err(|e| AppError::internal(format!("leaf cert sign failed: {e}")))?;
        let cert_pem = cert.pem();
        let key_pem = leaf_key.serialize_pem();
        let fingerprint = sha256_bytes_hex(cert.der());
        let serial = hex::encode(&cert.der()[..16.min(cert.der().len())]);
        let expires_at = crate::db::now() + ChronoDuration::days(365);

        Ok(IssuedNodeCert {
            node_id: node_id.to_string(),
            cert_pem,
            key_pem,
            ca_pem: self.ca_cert_pem.clone(),
            serial,
            fingerprint,
            subject: format!("CN={subject_cn},O=ProxyPanel v4 Node"),
            expires_at,
        })
    }

    /// Signs a node-generated CSR as a **server-only** certificate (pull mode).
    ///
    /// The node keeps its private key. Subject and extensions come from the panel, not from the
    /// CSR, so a node cannot request a CA/client certificate or an arbitrary identity; only the
    /// public key is taken from the request.
    pub fn sign_node_csr(
        &self,
        csr_pem: &str,
        node_id: &str,
        node_host: &str,
    ) -> AppResult<IssuedNodeCert> {
        let mut csr = CertificateSigningRequestParams::from_pem(csr_pem)
            .map_err(|e| AppError::bad_request(format!("invalid CSR: {e}")))?;

        set_validity(&mut csr.params, 365);
        let subject_cn = format!("node:{node_id}");
        let mut dn = DistinguishedName::new();
        dn.push(DnType::OrganizationName, "ProxyPanel v4 Node");
        dn.push(DnType::CommonName, subject_cn.clone());
        csr.params.distinguished_name = dn;
        csr.params.is_ca = IsCa::NoCa;
        csr.params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyEncipherment,
        ];
        csr.params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        csr.params.custom_extensions.clear();
        csr.params.subject_alt_names = node_sans(node_host);

        let cert = csr
            .signed_by(&self.issuer)
            .map_err(|e| AppError::internal(format!("node cert sign failed: {e}")))?;
        let der = cert.der();
        Ok(IssuedNodeCert {
            node_id: node_id.to_string(),
            cert_pem: cert.pem(),
            key_pem: String::new(),
            ca_pem: self.ca_cert_pem.clone(),
            serial: hex::encode(&der[..16.min(der.len())]),
            fingerprint: sha256_bytes_hex(der),
            subject: format!("CN={subject_cn},O=ProxyPanel v4 Node"),
            expires_at: crate::db::now() + ChronoDuration::days(365),
        })
    }

    /// Issues the panel's own **client-only** identity used to call node APIs over mTLS.
    /// Returns `(cert_pem, key_pem)`; the node accepts exactly this CA-signed client certificate.
    pub fn issue_panel_client_identity(&self) -> AppResult<(String, String)> {
        let key = KeyPair::generate()
            .map_err(|e| AppError::internal(format!("panel client keygen failed: {e}")))?;
        let mut params = CertificateParams::default();
        set_validity(&mut params, 365);
        let mut dn = DistinguishedName::new();
        dn.push(DnType::OrganizationName, "ProxyPanel v4 Panel");
        dn.push(DnType::CommonName, "panel-client");
        params.distinguished_name = dn;
        params.is_ca = IsCa::NoCa;
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
        let cert = params
            .signed_by(&key, &self.issuer)
            .map_err(|e| AppError::internal(format!("panel client cert sign failed: {e}")))?;
        Ok((cert.pem(), key.serialize_pem()))
    }

    /// HTTPS client for pull-mode nodes: trusts only this CA for the server certificate and
    /// presents a CA-signed, client-only identity. Hostname verification stays on, so a node's
    /// certificate is only valid for the host it was issued for.
    pub fn build_node_client(&self) -> AppResult<reqwest::Client> {
        let (cert, key) = self.issue_panel_client_identity()?;
        let ca = reqwest::Certificate::from_pem(self.ca_cert_pem.as_bytes())
            .map_err(|e| AppError::internal(format!("invalid CA for node client: {e}")))?;
        let identity = reqwest::Identity::from_pem(format!("{cert}\n{key}").as_bytes())
            .map_err(|e| AppError::internal(format!("invalid panel client identity: {e}")))?;
        reqwest::Client::builder()
            .tls_built_in_root_certs(false)
            .add_root_certificate(ca)
            .identity(identity)
            .timeout(std::time::Duration::from_secs(15))
            .connect_timeout(std::time::Duration::from_secs(5))
            .build()
            .map_err(|e| AppError::internal(format!("node mTLS client build failed: {e}")))
    }

    /// Verifies a presented client certificate PEM (or base64 DER) against a stored fingerprint or CA.
    pub fn verify_presented_cert(&self, raw_header: &str, expected_fp: Option<&str>) -> bool {
        let trimmed = raw_header.trim();
        if trimmed.is_empty() {
            return false;
        }
        // Accept either raw hex fingerprint (`x-mtls-fingerprint`), URL-encoded PEM, or raw PEM
        if let Some(exp) = expected_fp.filter(|s| !s.is_empty()) {
            if crate::auth::crypto::ct_eq(&trimmed.to_ascii_lowercase(), &exp.to_ascii_lowercase())
            {
                return true;
            }
        }
        let unescaped = percent_encoding::percent_decode_str(trimmed)
            .decode_utf8()
            .map(|c| c.into_owned())
            .unwrap_or_else(|_| trimmed.to_string());

        let Ok(der) = pem_to_der(&unescaped) else {
            return false;
        };
        let fp = sha256_bytes_hex(&der);
        if let Some(exp) = expected_fp.filter(|s| !s.is_empty()) {
            if !crate::auth::crypto::ct_eq(&fp, &exp.to_ascii_lowercase()) {
                return false;
            }
        }
        let Ok((_, parsed)) = X509Certificate::from_der(&der) else {
            return false;
        };
        if !parsed.validity().is_valid() {
            return false;
        }
        let Ok(ca_der) = pem_to_der(&self.ca_cert_pem) else {
            return false;
        };
        let Ok((_, ca_x509)) = X509Certificate::from_der(&ca_der) else {
            return false;
        };
        parsed
            .verify_signature(Some(ca_x509.public_key()))
            .is_ok()
    }
}

/// Subject alternative names for a node host (IP or DNS name); `localhost`/loopback always included
/// so the panel can also reach a node on the same machine.
fn node_sans(node_host: &str) -> Vec<SanType> {
    let clean = node_host
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or("")
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']');
    let mut sans = Vec::new();
    if !clean.is_empty() {
        if let Ok(ip) = clean.parse::<std::net::IpAddr>() {
            sans.push(SanType::IpAddress(ip));
        } else if let Ok(ia5) = clean.to_string().try_into() {
            sans.push(SanType::DnsName(ia5));
        }
    }
    sans.push(SanType::IpAddress(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)));
    if let Ok(ia5) = "localhost".to_string().try_into() {
        sans.push(SanType::DnsName(ia5));
    }
    sans
}

pub fn pem_to_der(pem_str: &str) -> AppResult<Vec<u8>> {
    let mut reader = std::io::Cursor::new(pem_str.as_bytes());
    if let Some(Ok(cert)) = rustls_pemfile::certs(&mut reader).next() {
        return Ok(cert.to_vec());
    }
    Err(AppError::bad_request("Invalid X.509 PEM certificate"))
}

#[cfg(test)]
mod pull_tests {
    use super::*;

    fn csr_for(key: &KeyPair) -> String {
        let mut p = CertificateParams::default();
        // The node tries to smuggle in a CA identity and client usage; the panel must ignore both.
        p.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        p.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, "i-am-root");
        p.distinguished_name = dn;
        p.serialize_request(key).unwrap().pem().unwrap()
    }

    #[test]
    fn csr_signing_forces_server_only_identity_chosen_by_panel() {
        let ca = MtlsCa::generate_ephemeral().unwrap();
        let key = KeyPair::generate().unwrap();
        let issued = ca.sign_node_csr(&csr_for(&key), "node-1", "203.0.113.7").unwrap();
        assert!(issued.key_pem.is_empty(), "panel must never hold the node key");

        let der = pem_to_der(&issued.cert_pem).unwrap();
        let (_, x) = X509Certificate::from_der(&der).unwrap();
        assert!(x.subject().to_string().contains("node:node-1"));
        assert!(!x.subject().to_string().contains("i-am-root"));
        assert!(!x.is_ca(), "CSR must not yield a CA certificate");
        let eku = x.extended_key_usage().unwrap().unwrap().value;
        assert!(eku.server_auth && !eku.client_auth);
        let sans = x.subject_alternative_name().unwrap().unwrap().value.general_names.len();
        assert!(sans >= 3, "host + loopback SANs expected, got {sans}");
        assert!(ca.verify_presented_cert(&issued.cert_pem, Some(&issued.fingerprint)));
    }

    #[test]
    fn node_client_builds() {
        let ca = MtlsCa::generate_ephemeral().unwrap();
        assert!(ca.build_node_client().is_ok());
    }

    #[test]
    fn invalid_csr_is_rejected() {
        let ca = MtlsCa::generate_ephemeral().unwrap();
        assert!(ca.sign_node_csr("not a csr", "n", "h").is_err());
    }

    #[test]
    fn panel_identity_is_client_only() {
        let ca = MtlsCa::generate_ephemeral().unwrap();
        let (cert, key) = ca.issue_panel_client_identity().unwrap();
        assert!(key.contains("PRIVATE KEY"));
        let der = pem_to_der(&cert).unwrap();
        let (_, x) = X509Certificate::from_der(&der).unwrap();
        let eku = x.extended_key_usage().unwrap().unwrap().value;
        assert!(eku.client_auth && !eku.server_auth);
        assert!(!x.is_ca());
    }

    #[test]
    fn cert_from_other_ca_is_rejected_and_validity_is_bounded() {
        let ca1 = MtlsCa::generate_ephemeral().unwrap();
        let ca2 = MtlsCa::generate_ephemeral().unwrap();
        let issued = ca1.issue_node_cert("node-1", "1.1.1.1").unwrap();
        assert!(ca1.verify_presented_cert(&issued.cert_pem, Some(&issued.fingerprint)));
        assert!(!ca2.verify_presented_cert(&issued.cert_pem, None));

        let der = pem_to_der(&issued.cert_pem).unwrap();
        let (_, x) = X509Certificate::from_der(&der).unwrap();
        let not_after = x.validity().not_after.to_datetime();
        assert!(not_after.year() <= Utc::now().year() + 2);
    }
}
