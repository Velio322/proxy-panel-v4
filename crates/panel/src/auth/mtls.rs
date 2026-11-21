use crate::auth::crypto::sha256_bytes_hex;
use crate::error::{AppError, AppResult};
use chrono::{Duration as ChronoDuration, NaiveDateTime};
use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa,
    Issuer, KeyPair, KeyUsagePurpose, SanType,
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
                if let Ok(loaded) = Self::from_pem(&cert_pem, &key_pem) {
                    return Ok(loaded);
                }
            }
        }

        let generated = Self::generate_ephemeral()?;
        if let Some(dir) = data_dir {
            if std::fs::create_dir_all(dir).is_ok() {
                let _ = std::fs::write(dir.join("mtls-ca.crt"), &generated.ca_cert_pem);
                let _ = std::fs::write(dir.join("mtls-ca.key"), &generated.ca_key_pem);
            }
        }
        Ok(generated)
    }

    pub fn generate_ephemeral() -> AppResult<Self> {
        let key_pair = KeyPair::generate()
            .map_err(|e| AppError::internal(format!("mTLS CA keygen failed: {e}")))?;
        let mut params = CertificateParams::default();
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
        parsed.validity().is_valid()
    }
}

pub fn pem_to_der(pem_str: &str) -> AppResult<Vec<u8>> {
    let mut reader = std::io::Cursor::new(pem_str.as_bytes());
    if let Some(Ok(cert)) = rustls_pemfile::certs(&mut reader).next() {
        return Ok(cert.to_vec());
    }
    Err(AppError::bad_request("Invalid X.509 PEM certificate"))
}
