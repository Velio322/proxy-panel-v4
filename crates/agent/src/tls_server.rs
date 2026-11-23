//! HTTPS listener with mutual TLS for `API_MODE=pull`.
//!
//! The panel calls the node over short HTTPS requests authenticated by a client certificate
//! signed by the panel CA (plus the node secret). There is no persistent session, so a panel
//! restart changes nothing on the node.
//!
//! Handshakes run in their own tasks and completed streams are handed to `axum::serve` through a
//! channel, so one slow or hostile client cannot stall `accept`.

use crate::config::AgentConfig;
use crate::mtls::{build_server_tls_config, load_pem_bytes};
use axum::serve::Listener;
use rustls::ServerConfig;
use std::io;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Semaphore};
use tokio_rustls::server::TlsStream;
use tokio_rustls::TlsAcceptor;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_PENDING_HANDSHAKES: usize = 256;

/// Current TLS configuration; `None` until the node has obtained its certificate from the panel.
pub type SharedTlsConfig = Arc<RwLock<Option<Arc<ServerConfig>>>>;

pub fn new_shared_config() -> SharedTlsConfig {
    Arc::new(RwLock::new(None))
}

/// Builds the server config from the files written at registration. Client certificates are
/// mandatory and must chain to the panel CA. Only HTTP/1.1 is offered (axum is built without h2).
pub async fn load_server_config(cfg: &AgentConfig) -> Result<Arc<ServerConfig>, String> {
    let cert = load_pem_bytes(&cfg.tls_cert_path().to_string_lossy()).await?;
    let key = load_pem_bytes(&cfg.tls_key_path().to_string_lossy()).await?;
    let ca = load_pem_bytes(&cfg.panel_ca_path().to_string_lossy()).await?;
    let mut sc = build_server_tls_config(Some(&ca), &cert, &key, true)?;
    sc.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Arc::new(sc))
}

pub fn install_config(slot: &SharedTlsConfig, cfg: Arc<ServerConfig>) {
    if let Ok(mut guard) = slot.write() {
        *guard = Some(cfg);
    }
}

pub struct TlsListener {
    rx: mpsc::Receiver<(TlsStream<TcpStream>, SocketAddr)>,
    local: SocketAddr,
}

impl TlsListener {
    pub fn new(tcp: TcpListener, config: SharedTlsConfig) -> io::Result<Self> {
        let local = tcp.local_addr()?;
        let (tx, rx) = mpsc::channel(64);
        let limit = Arc::new(Semaphore::new(MAX_PENDING_HANDSHAKES));
        tokio::spawn(async move {
            loop {
                let (sock, peer) = match tcp.accept().await {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::warn!(error = %e, "TLS listener accept failed");
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        continue;
                    }
                };
                if tx.is_closed() {
                    break;
                }
                // No certificate yet (first boot, panel unreachable): refuse instead of hanging.
                let Some(server_cfg) = config.read().ok().and_then(|g| g.clone()) else {
                    drop(sock);
                    continue;
                };
                let Ok(permit) = limit.clone().try_acquire_owned() else {
                    continue;
                };
                let tx = tx.clone();
                tokio::spawn(async move {
                    let _permit = permit;
                    let acceptor = TlsAcceptor::from(server_cfg);
                    match tokio::time::timeout(HANDSHAKE_TIMEOUT, acceptor.accept(sock)).await {
                        Ok(Ok(stream)) => {
                            let _ = tx.send((stream, peer)).await;
                        }
                        Ok(Err(e)) => {
                            tracing::debug!(peer = %peer, error = %e, "TLS handshake rejected")
                        }
                        Err(_) => tracing::debug!(peer = %peer, "TLS handshake timed out"),
                    }
                });
            }
        });
        Ok(Self { rx, local })
    }
}

impl Listener for TlsListener {
    type Io = TlsStream<TcpStream>;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        match self.rx.recv().await {
            Some(v) => v,
            None => std::future::pending().await,
        }
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        Ok(self.local)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::get;
    use axum::serve::ListenerExt;
    use rcgen::{
        BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose,
        IsCa, Issuer, KeyPair, KeyUsagePurpose, SanType,
    };

    struct Ca {
        pem: String,
        issuer: Issuer<'static, KeyPair>,
    }

    fn make_ca(cn: &str) -> Ca {
        let key = KeyPair::generate().unwrap();
        let mut p = CertificateParams::default();
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, cn);
        p.distinguished_name = dn;
        p.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        p.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::DigitalSignature];
        let cert = p.self_signed(&key).unwrap();
        Ca { pem: cert.pem(), issuer: Issuer::new(p, key) }
    }

    /// Returns `(cert_pem, key_pem)` signed by `ca` with the given usage.
    fn leaf(ca: &Ca, usage: ExtendedKeyUsagePurpose, san_ip: bool) -> (String, String) {
        let key = KeyPair::generate().unwrap();
        let mut p = CertificateParams::default();
        p.is_ca = IsCa::NoCa;
        p.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        p.extended_key_usages = vec![usage];
        if san_ip {
            p.subject_alt_names = vec![SanType::IpAddress("127.0.0.1".parse().unwrap())];
        }
        let cert = p.signed_by(&key, &ca.issuer).unwrap();
        (cert.pem(), key.serialize_pem())
    }

    async fn start(ca: &Ca) -> (SocketAddr, tokio::task::JoinHandle<()>) {
        let (cert, key) = leaf(ca, ExtendedKeyUsagePurpose::ServerAuth, true);
        let mut sc = build_server_tls_config(Some(ca.pem.as_bytes()), cert.as_bytes(), key.as_bytes(), true).unwrap();
        sc.alpn_protocols = vec![b"http/1.1".to_vec()];
        let slot = new_shared_config();
        install_config(&slot, Arc::new(sc));
        let tcp = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let listener = TlsListener::new(tcp, slot).unwrap();
        let addr = listener.local_addr().unwrap();
        let app = axum::Router::new().route("/ping", get(|| async { "pong" }));
        let handle = tokio::spawn(async move {
            let _ = axum::serve(
                listener.tap_io(|_| {}),
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await;
        });
        (addr, handle)
    }

    fn client(ca: &Ca, identity: Option<(String, String)>) -> reqwest::Client {
        let mut b = reqwest::Client::builder()
            .tls_built_in_root_certs(false)
            .add_root_certificate(reqwest::Certificate::from_pem(ca.pem.as_bytes()).unwrap())
            .timeout(Duration::from_secs(5));
        if let Some((c, k)) = identity {
            b = b.identity(reqwest::Identity::from_pem(format!("{c}\n{k}").as_bytes()).unwrap());
        }
        b.build().unwrap()
    }

    #[tokio::test]
    async fn only_panel_ca_client_certificates_are_accepted() {
        let panel_ca = make_ca("panel-ca");
        let other_ca = make_ca("evil-ca");
        let (addr, server) = start(&panel_ca).await;
        let url = format!("https://127.0.0.1:{}/ping", addr.port());

        // valid panel client certificate
        let ok = client(&panel_ca, Some(leaf(&panel_ca, ExtendedKeyUsagePurpose::ClientAuth, false)));
        let resp = ok.get(&url).send().await.expect("panel client must connect");
        assert_eq!(resp.text().await.unwrap(), "pong");

        // no client certificate
        assert!(client(&panel_ca, None).get(&url).send().await.is_err(), "no cert must fail");

        // client certificate from a different CA
        let evil = client(&panel_ca, Some(leaf(&other_ca, ExtendedKeyUsagePurpose::ClientAuth, false)));
        assert!(evil.get(&url).send().await.is_err(), "foreign CA must fail");

        // a node's own (server-only) certificate cannot be used to impersonate the panel
        let node_cert = client(&panel_ca, Some(leaf(&panel_ca, ExtendedKeyUsagePurpose::ServerAuth, true)));
        assert!(node_cert.get(&url).send().await.is_err(), "server-only cert must fail as client cert");

        server.abort();
    }

    #[tokio::test]
    async fn connections_are_refused_until_a_certificate_is_installed() {
        let tcp = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = tcp.local_addr().unwrap();
        let listener = TlsListener::new(tcp, new_shared_config()).unwrap();
        let app = axum::Router::new().route("/ping", get(|| async { "pong" }));
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener.tap_io(|_| {}), app.into_make_service_with_connect_info::<SocketAddr>()).await;
        });
        let ca = make_ca("ca");
        let res = client(&ca, None).get(format!("https://127.0.0.1:{}/ping", addr.port())).send().await;
        assert!(res.is_err());
        task.abort();
    }
}
