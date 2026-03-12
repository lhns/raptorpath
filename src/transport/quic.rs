//! QUIC transport implementation using quinn.
//!
//! Each path gets its own QUIC connection. We use:
//! - DATAGRAM frames for symbol data (unreliable, low overhead)
//! - A bidirectional stream for control messages (reliable)

use super::protocol::{ControlMessage, Handshake, PROTOCOL_VERSION, SymbolBatch, WireMessage};
use crate::scheduler::PathId;
use quinn::{ClientConfig, Endpoint, ServerConfig};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// A QUIC-based multipath transport.
pub struct QuicTransport {
    /// Local endpoints (one per bind address / path)
    endpoints: HashMap<PathId, Endpoint>,
    /// Active connections per path
    connections: HashMap<PathId, quinn::Connection>,
}

impl QuicTransport {
    /// Create a new transport with endpoints bound to the given addresses.
    pub async fn new(bind_addrs: &[SocketAddr], is_server: bool) -> anyhow::Result<Self> {
        let mut endpoints = HashMap::new();

        for (i, addr) in bind_addrs.iter().enumerate() {
            let endpoint = if is_server {
                let (server_config, _cert) = Self::generate_self_signed_config()?;
                let ep = Endpoint::server(server_config, *addr)?;
                info!(%addr, path_id = i, "server endpoint bound");
                ep
            } else {
                let mut ep = Endpoint::client(*addr)?;
                let client_config = Self::insecure_client_config();
                ep.set_default_client_config(client_config);
                info!(%addr, path_id = i, "client endpoint bound");
                ep
            };
            endpoints.insert(i as PathId, endpoint);
        }

        Ok(Self {
            endpoints,
            connections: HashMap::new(),
        })
    }

    /// Connect to a peer on a specific path.
    pub async fn connect(&mut self, path_id: PathId, peer_addr: SocketAddr) -> anyhow::Result<()> {
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
    pub async fn accept(&mut self, path_id: PathId) -> anyhow::Result<()> {
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

    /// Perform handshake on a connection (client side). Returns the peer's handshake.
    async fn perform_handshake(
        conn: &quinn::Connection,
        local: &Handshake,
    ) -> anyhow::Result<Handshake> {
        // Open a bidirectional stream for handshake
        let (mut send, mut recv) = conn.open_bi().await?;

        // Send our handshake
        let data = local.serialize();
        send.write_all(&(data.len() as u32).to_be_bytes()).await?;
        send.write_all(&data).await?;
        send.finish()?;

        // Read peer's handshake
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
        // Accept the bidirectional stream
        let (mut send, mut recv) = conn.accept_bi().await?;

        // Read peer's handshake first (peer initiated)
        let mut len_buf = [0u8; 4];
        recv.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf) as usize;
        if len > 10_000 {
            anyhow::bail!("handshake too large: {len} bytes");
        }
        let mut buf = vec![0u8; len];
        recv.read_exact(&mut buf).await?;

        let peer = Handshake::deserialize(&buf)?;

        // Send our handshake back
        let data = local.serialize();
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
        let data = msg.serialize();

        // Use QUIC datagrams for unreliable delivery
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
        let data = wire.serialize();
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
        let data = wire.serialize();

        // Length-prefix the message
        send.write_all(&(data.len() as u32).to_be_bytes()).await?;
        send.write_all(&data).await?;
        send.finish()?;
        Ok(())
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
    ) -> Vec<tokio::task::JoinHandle<()>> {
        let mut handles = vec![];

        for (&path_id, conn) in &self.connections {
            let conn = conn.clone();
            let tx = tx.clone();

            // Clone for uni-stream receiver before datagram task moves them
            let conn_uni = conn.clone();
            let tx_uni = tx.clone();

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
                            // Read length-prefixed message
                            let mut len_buf = [0u8; 4];
                            if recv.read_exact(&mut len_buf).await.is_err() {
                                continue;
                            }
                            let len = u32::from_be_bytes(len_buf) as usize;
                            if len > 1_000_000 { continue; } // sanity limit
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
        transport.datagram_receive_buffer_size(Some(65536));

        Ok((server_config, vec![cert_der]))
    }

    fn insecure_client_config() -> ClientConfig {
        let crypto = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(SkipCertVerification))
            .with_no_client_auth();

        let mut config = ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(crypto)
                .expect("rustls config should be valid"),
        ));

        let mut transport = quinn::TransportConfig::default();
        transport.datagram_receive_buffer_size(Some(65536));
        config.transport_config(Arc::new(transport));
        config
    }
}

/// Skip certificate verification (for self-signed certs in testing/dev).
/// In production, use proper certificate validation.
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
