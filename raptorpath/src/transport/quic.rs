//! QUIC transport implementation using quinn.
//!
//! Each path gets its own QUIC connection. We use:
//! - DATAGRAM frames for symbol data (unreliable, low overhead)
//! - A bidirectional stream for control messages (reliable)
//!
//! TLS modes:
//! - Default: self-signed cert, skip verification (dev/testing)
//! - Pinned: verify server cert matches a pinned DER/PEM file (production)

use super::protocol::{ControlMessage, Handshake, PROTOCOL_VERSION, SymbolBatch, WireMessage};
use crate::scheduler::PathId;
use dashmap::DashMap;
use quinn::{ClientConfig, Endpoint, ServerConfig};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use sha2::{Digest, Sha256};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

/// A QUIC-based multipath transport.
///
/// Uses DashMap for connections so paths can be added/removed at runtime.
pub struct QuicTransport {
    /// Local endpoints (one per bind address / path)
    endpoints: DashMap<PathId, Endpoint>,
    /// Active connections per path
    connections: DashMap<PathId, quinn::Connection>,
    /// Whether this transport is a server
    is_server: bool,
    /// Optional pinned certificate for client-side verification.
    /// When set, the client verifies the server's cert matches this fingerprint.
    pinned_cert_hash: Option<[u8; 32]>,
}

impl QuicTransport {
    /// Create a new transport with endpoints bound to the given addresses.
    ///
    /// `pin_cert_path`: optional path to a DER or PEM certificate file.
    /// When provided, the client will verify that the server's certificate
    /// matches this pinned cert (SHA-256 fingerprint comparison).
    pub async fn new(
        bind_addrs: &[SocketAddr],
        is_server: bool,
        pin_cert_path: Option<&Path>,
    ) -> anyhow::Result<Self> {
        let pinned_cert_hash = pin_cert_path
            .map(|p| load_pinned_cert_hash(p))
            .transpose()?;

        if let Some(hash) = &pinned_cert_hash {
            info!(fingerprint = %hex::encode(hash), "TLS cert pinning enabled");
        }

        let endpoints = DashMap::new();

        for (i, addr) in bind_addrs.iter().enumerate() {
            let endpoint = if is_server {
                let (server_config, cert_der) = Self::generate_self_signed_config()?;
                // Log the server cert fingerprint so the user can pin it on the client
                let fingerprint = sha256_fingerprint(&cert_der[0]);
                info!(%addr, path_id = i, fingerprint = %hex::encode(fingerprint),
                    "server endpoint bound — use this fingerprint for --pin-cert");
                Endpoint::server(server_config, *addr)?
            } else {
                let mut ep = Endpoint::client(*addr)?;
                let client_config = Self::make_client_config(pinned_cert_hash);
                ep.set_default_client_config(client_config);
                info!(%addr, path_id = i, "client endpoint bound");
                ep
            };
            endpoints.insert(i as PathId, endpoint);
        }

        Ok(Self {
            endpoints,
            connections: DashMap::new(),
            is_server,
            pinned_cert_hash,
        })
    }

    /// Connect to a peer on a specific path.
    pub async fn connect(&self, path_id: PathId, peer_addr: SocketAddr) -> anyhow::Result<()> {
        let endpoint = self
            .endpoints
            .get(&path_id)
            .ok_or_else(|| anyhow::anyhow!("no endpoint for path {path_id}"))?;

        let connection = endpoint.connect(peer_addr, "raptorpath")?.await?;

        // ADR-0010: perform handshake
        let local_hs = Handshake {
            version: PROTOCOL_VERSION,
            max_block_size: 64 * 1024,
            symbol_size: 1200,
            path_id,
        };
        let _peer_hs = Self::perform_handshake(&connection, &local_hs).await?;

        info!(path_id, %peer_addr, "connected and handshake complete");
        self.connections.insert(path_id, connection);
        Ok(())
    }

    /// Accept an incoming connection on a specific path.
    pub async fn accept(&self, path_id: PathId) -> anyhow::Result<()> {
        let endpoint = self
            .endpoints
            .get(&path_id)
            .ok_or_else(|| anyhow::anyhow!("no endpoint for path {path_id}"))?;

        let incoming = endpoint
            .accept()
            .await
            .ok_or_else(|| anyhow::anyhow!("endpoint closed"))?;
        let connection = incoming.await?;

        // ADR-0010: accept handshake from peer
        let local_hs = Handshake {
            version: PROTOCOL_VERSION,
            max_block_size: 64 * 1024,
            symbol_size: 1200,
            path_id,
        };
        let _peer_hs = Self::accept_handshake(&connection, &local_hs).await?;

        info!(path_id, remote = %connection.remote_address(), "accepted with handshake");
        self.connections.insert(path_id, connection);
        Ok(())
    }

    /// Add a new path at runtime. Binds a new endpoint, connects or accepts,
    /// and returns the connection for receiver spawning.
    pub async fn add_path(
        &self,
        path_id: PathId,
        bind_addr: SocketAddr,
        peer_addr: Option<SocketAddr>,
    ) -> anyhow::Result<quinn::Connection> {
        // Create and bind new endpoint
        let endpoint = if self.is_server {
            let (server_config, _cert) = Self::generate_self_signed_config()?;
            Endpoint::server(server_config, bind_addr)?
        } else {
            let mut ep = Endpoint::client(bind_addr)?;
            ep.set_default_client_config(Self::make_client_config(self.pinned_cert_hash));
            ep
        };
        self.endpoints.insert(path_id, endpoint);

        // Connect or accept
        if let Some(peer) = peer_addr {
            self.connect(path_id, peer).await?;
        } else {
            self.accept(path_id).await?;
        }

        let conn = self
            .connections
            .get(&path_id)
            .ok_or_else(|| anyhow::anyhow!("connection not found after setup"))?
            .clone();
        Ok(conn)
    }

    /// Remove a path at runtime.
    pub fn remove_path(&self, path_id: PathId) {
        if let Some((_, conn)) = self.connections.remove(&path_id) {
            conn.close(0u32.into(), b"path removed");
        }
        self.endpoints.remove(&path_id);
        info!(path_id, "path removed");
    }

    /// Spawn receive loops for a single path, feeding into a channel.
    pub fn spawn_receiver_for_path(
        &self,
        path_id: PathId,
        conn: quinn::Connection,
        tx: mpsc::Sender<(PathId, WireMessage)>,
        ctrl_tx: mpsc::Sender<(PathId, WireMessage)>,
    ) -> Vec<tokio::task::JoinHandle<()>> {
        let mut handles = vec![];

        let conn_uni = conn.clone();
        // Stream-origin control messages go to a DEDICATED channel: the
        // data channel backs up under symbol floods, and liveness
        // (PathReport/Ping) queued behind it starved the dead-path check
        // (L1 finding: bulk transfers killed the tunnel in ~6 s).
        let tx_uni = ctrl_tx;

        // Datagram receiver
        let handle = tokio::spawn(async move {
            loop {
                match conn.read_datagram().await {
                    Ok(data) => match WireMessage::deserialize(&data) {
                        Ok(msg) => {
                            if tx.send((path_id, msg)).await.is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            warn!(path_id, ?e, "failed to deserialize datagram");
                        }
                    },
                    Err(e) => {
                        error!(path_id, ?e, "datagram receive error");
                        break;
                    }
                }
            }
        });
        handles.push(handle);

        // Uni-stream receiver (for reliable control messages)
        let uni_handle = tokio::spawn(async move {
            loop {
                match conn_uni.accept_uni().await {
                    Ok(mut recv) => {
                        tracing::debug!(path_id, "uni stream accepted");
                        let mut len_buf = [0u8; 4];
                        if let Err(e) = recv.read_exact(&mut len_buf).await {
                            tracing::debug!(path_id, ?e, "uni stream length read failed");
                            continue;
                        }
                        let len = u32::from_be_bytes(len_buf) as usize;
                        if len > 1_000_000 { continue; }
                        let mut data = vec![0u8; len];
                        if recv.read_exact(&mut data).await.is_err() {
                            continue;
                        }
                        match WireMessage::deserialize(&data) {
                            Ok(msg) => {
                                if tx_uni.send((path_id, msg)).await.is_err() {
                                    break;
                                }
                            }
                            Err(e) => {
                                warn!(path_id, ?e, "failed to deserialize uni stream message");
                            }
                        }
                    }
                    Err(e) => {
                        error!(path_id, ?e, "uni stream accept error");
                        break;
                    }
                }
            }
        });
        handles.push(uni_handle);

        handles
    }

    /// Perform handshake on a connection (client side). Returns the peer's handshake.
    async fn perform_handshake(
        conn: &quinn::Connection,
        local: &Handshake,
    ) -> anyhow::Result<Handshake> {
        let (mut send, mut recv) = conn.open_bi().await?;

        let data = local.serialize()?;
        send.write_all(&(data.len() as u32).to_be_bytes()).await?;
        send.write_all(&data).await?;
        send.finish()?;

        let mut len_buf = [0u8; 4];
        recv.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf) as usize;
        if len > 10_000 {
            anyhow::bail!("handshake too large: {len} bytes");
        }
        let mut buf = vec![0u8; len];
        recv.read_exact(&mut buf).await?;

        let peer = Handshake::deserialize(&buf)?;
        info!(
            local_version = local.version,
            peer_version = peer.version,
            peer_path_id = peer.path_id,
            "handshake complete"
        );
        Ok(peer)
    }

    /// Accept a handshake from a peer (server side).
    async fn accept_handshake(
        conn: &quinn::Connection,
        local: &Handshake,
    ) -> anyhow::Result<Handshake> {
        let (mut send, mut recv) = conn.accept_bi().await?;

        let mut len_buf = [0u8; 4];
        recv.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf) as usize;
        if len > 10_000 {
            anyhow::bail!("handshake too large: {len} bytes");
        }
        let mut buf = vec![0u8; len];
        recv.read_exact(&mut buf).await?;

        let peer = Handshake::deserialize(&buf)?;

        let data = local.serialize()?;
        send.write_all(&(data.len() as u32).to_be_bytes()).await?;
        send.write_all(&data).await?;
        send.finish()?;

        info!(
            local_version = local.version,
            peer_version = peer.version,
            peer_path_id = peer.path_id,
            "handshake complete (server)"
        );
        Ok(peer)
    }

    /// Send a symbol batch over a path using QUIC datagrams.
    pub fn send_symbols(&self, path_id: PathId, batch: SymbolBatch) -> anyhow::Result<()> {
        let conn = self
            .connections
            .get(&path_id)
            .ok_or_else(|| anyhow::anyhow!("no connection on path {path_id}"))?;

        let msg = WireMessage::Data(batch);
        let data = msg.serialize()?;

        conn.send_datagram(data.into())?;
        Ok(())
    }

    /// Send a control message as a datagram (best-effort, low latency).
    pub fn send_control_datagram(&self, path_id: PathId, msg: ControlMessage) -> anyhow::Result<()> {
        let conn = self
            .connections
            .get(&path_id)
            .ok_or_else(|| anyhow::anyhow!("no connection on path {path_id}"))?;
        let wire = WireMessage::Control(msg);
        let data = wire.serialize()?;
        conn.send_datagram(data.into())?;
        Ok(())
    }

    /// Send a control message over a path's reliable stream.
    pub async fn send_control(
        &self,
        path_id: PathId,
        msg: ControlMessage,
    ) -> anyhow::Result<()> {
        let conn = self
            .connections
            .get(&path_id)
            .ok_or_else(|| anyhow::anyhow!("no connection on path {path_id}"))?;

        let mut send = conn.open_uni().await?;
        let wire = WireMessage::Control(msg);
        let data = wire.serialize()?;

        send.write_all(&(data.len() as u32).to_be_bytes()).await?;
        send.write_all(&data).await?;
        send.finish()?;
        Ok(())
    }

    /// Query the max datagram size for a path (PMTU-based).
    pub fn max_datagram_size(&self, path_id: PathId) -> Option<usize> {
        self.connections
            .get(&path_id)
            .and_then(|conn| conn.max_datagram_size())
    }

    /// Receive datagrams from a path.
    pub async fn recv_datagram(&self, path_id: PathId) -> anyhow::Result<WireMessage> {
        let conn = self
            .connections
            .get(&path_id)
            .ok_or_else(|| anyhow::anyhow!("no connection on path {path_id}"))?;

        let data = conn.read_datagram().await?;
        let msg = WireMessage::deserialize(&data)?;
        Ok(msg)
    }

    /// Spawn receive loops for all paths, feeding into a channel.
    pub fn spawn_receivers(
        &self,
        tx: mpsc::Sender<(PathId, WireMessage)>,
        ctrl_tx: mpsc::Sender<(PathId, WireMessage)>,
    ) -> Vec<tokio::task::JoinHandle<()>> {
        let mut handles = vec![];

        for entry in self.connections.iter() {
            let path_id = *entry.key();
            let conn = entry.value().clone();
            handles.extend(self.spawn_receiver_for_path(
                path_id,
                conn,
                tx.clone(),
                ctrl_tx.clone(),
            ));
        }

        handles
    }

    fn generate_self_signed_config(
    ) -> anyhow::Result<(ServerConfig, Vec<CertificateDer<'static>>)> {
        let cert = rcgen::generate_simple_self_signed(vec!["raptorpath".into()])?;
        let cert_der = CertificateDer::from(cert.cert);
        let key_der = PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der());

        let mut server_config = ServerConfig::with_single_cert(
            vec![cert_der.clone()],
            key_der.into(),
        )?;

        // Enable datagrams
        let transport = Arc::get_mut(&mut server_config.transport).unwrap();
        transport.max_concurrent_bidi_streams(100u32.into());
        transport.max_concurrent_uni_streams(100u32.into());
        transport.datagram_receive_buffer_size(Some(4 * 1024 * 1024));
        transport.datagram_send_buffer_size(4 * 1024 * 1024);

        Ok((server_config, vec![cert_der]))
    }

    /// Build a client config with either pinned cert verification or
    /// insecure mode (skip verification) for dev/testing.
    fn make_client_config(pinned_hash: Option<[u8; 32]>) -> ClientConfig {
        let verifier: Arc<dyn rustls::client::danger::ServerCertVerifier> = match pinned_hash {
            Some(hash) => Arc::new(PinnedCertVerifier { expected_hash: hash }),
            None => Arc::new(SkipCertVerification),
        };

        let crypto = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(verifier)
            .with_no_client_auth();

        let mut config = ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(crypto)
                .expect("rustls config should be valid"),
        ));

        let mut transport = quinn::TransportConfig::default();
        transport.datagram_receive_buffer_size(Some(4 * 1024 * 1024));
        transport.datagram_send_buffer_size(4 * 1024 * 1024);
        config.transport_config(Arc::new(transport));
        config
    }
}

/// Compute SHA-256 fingerprint of a DER-encoded certificate.
fn sha256_fingerprint(cert: &CertificateDer<'_>) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(cert.as_ref());
    hasher.finalize().into()
}

/// Load a pinned certificate from a DER or PEM file and return its SHA-256 hash.
fn load_pinned_cert_hash(path: &Path) -> anyhow::Result<[u8; 32]> {
    let data = std::fs::read(path)
        .map_err(|e| anyhow::anyhow!("failed to read pinned cert '{}': {e}", path.display()))?;

    // Try PEM first, fall back to DER
    let cert_der = if data.starts_with(b"-----BEGIN") {
        let pem = pem::parse(&data)
            .map_err(|e| anyhow::anyhow!("failed to parse PEM cert '{}': {e}", path.display()))?;
        CertificateDer::from(pem.into_contents())
    } else {
        CertificateDer::from(data)
    };

    Ok(sha256_fingerprint(&cert_der))
}

/// Certificate verifier that pins to a specific certificate's SHA-256 fingerprint.
#[derive(Debug)]
struct PinnedCertVerifier {
    expected_hash: [u8; 32],
}

impl rustls::client::danger::ServerCertVerifier for PinnedCertVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        let actual_hash = sha256_fingerprint(end_entity);
        if actual_hash == self.expected_hash {
            Ok(rustls::client::danger::ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General(format!(
                "certificate fingerprint mismatch: expected {}, got {}",
                hex::encode(self.expected_hash),
                hex::encode(actual_hash),
            )))
        }
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::ED25519,
        ]
    }
}

/// Skip certificate verification (for self-signed certs in testing/dev).
#[derive(Debug)]
struct SkipCertVerification;

impl rustls::client::danger::ServerCertVerifier for SkipCertVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::ED25519,
        ]
    }
}
