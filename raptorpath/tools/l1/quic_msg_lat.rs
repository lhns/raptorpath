//! Fixed-rate QUIC message-latency benchmark for the L1 harness.
//!
//! Built as a quinn *example* (lives in quinn/quinn/examples/msg_lat.rs on the
//! build VM) so it inherits the exact, proven quinn version + deps of the
//! quinn-perf binary. Mirrors raptorpath/tools/l1/transfer_bench.py's
//! `stream-server` / `stream-client` modes, but over an ordered, reliable QUIC
//! stream instead of TCP.
//!
//! Wire framing (identical to transfer_bench.py): each message is a 4-byte
//! big-endian length prefix `n`, followed by `n` bytes whose first 8 bytes are
//! a big-endian CLOCK_REALTIME send timestamp in nanoseconds (SystemTime since
//! UNIX_EPOCH — the same clock python's time.time_ns() reads). The two netns
//! share the kernel clock, so one-way latency = recv_time - send_time is
//! directly measurable. Percentile JSON matches transfer_bench.py's format.
//!
//! Server: msg_lat server --listen 10.77.0.2:9920
//!   accepts ONE connection, accepts a unidirectional stream, reads framed
//!   messages, and on stream/connection close prints the latency percentile
//!   JSON, then exits.
//! Client: msg_lat client --server-name raptorpath --ip 10.77.0.2:9920 \
//!           --rate 50 --size 1200 --duration 30
//!   connects (skips cert verification), opens a uni stream, and sends framed
//!   [send_ns u64][payload] messages at --rate/sec for --duration seconds.
//!
//! ---------------------------------------------------------------------------
//! BUILDING & RUNNING (source reference — this file is the record of what was
//! measured; it is compiled ON THE L1 VM, not in this repo):
//!
//!   # On the VM, drop this file in as a quinn example so it links the exact
//!   # proven quinn version/deps that quinn-perf was built from, then:
//!   cp quic_msg_lat.rs ~/quinn/quinn/examples/msg_lat.rs
//!   # register it in ~/quinn/quinn/Cargo.toml:
//!   #   [[example]]
//!   #   name = "msg_lat"
//!   #   required-features = ["rustls-ring"]
//!   cd ~/quinn && cargo build --release --example msg_lat -p quinn
//!   # binary: ~/quinn/target/release/examples/msg_lat
//!
//! The L1 sweep is driven by tools/l1/quic_stream_bench.sh (server in rp-srv
//! bound to 10.77.0.2, client in rp-cli, direct QUIC over the netem veth — the
//! same geometry as the kernel-TCP cubic/bbr stream runs in stream_bench.sh).
//!
//! Shutdown handshake: the client keeps the connection open after finish()
//! until the server drains the whole reliable stream (incl. FIN + tail
//! retransmits under loss) and closes it. keep_alive_interval(1s) +
//! max_idle_timeout(60s) keep the connection up through GE loss bursts so the
//! full 1500-message stream is delivered and its true in-order tail measured
//! (at c5's 5.3% burst loss that tail is a ~45 s head-of-line cascade — see
//! docs/goal-gate.md "quinn message-tail vs raptorpath").

use std::{
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use quinn::{
    crypto::rustls::{QuicClientConfig, QuicServerConfig},
    ClientConfig, Endpoint, ServerConfig,
};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};

const ALPN: &[u8] = b"msg-lat";

#[derive(Parser)]
#[clap(name = "msg_lat")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Receive a fixed-rate message stream; report one-way latency percentiles.
    Server(ServerOpt),
    /// Send fixed-size messages at a fixed rate for a duration.
    Client(ClientOpt),
}

#[derive(Parser)]
struct ServerOpt {
    /// Address to listen on (bind to the namespace IP, e.g. 10.77.0.2:9920)
    #[clap(long, default_value = "0.0.0.0:9920")]
    listen: SocketAddr,
}

#[derive(Parser)]
struct ClientOpt {
    /// TLS server name (cert is not verified; any name works)
    #[clap(long, default_value = "raptorpath")]
    server_name: String,
    /// Server socket address, host:port (e.g. 10.77.0.2:9920)
    #[clap(long)]
    ip: SocketAddr,
    /// Messages per second
    #[clap(long, default_value = "50")]
    rate: f64,
    /// Total message size in bytes (>= 8; first 8 bytes are the send timestamp)
    #[clap(long, default_value = "1200")]
    size: usize,
    /// Duration to send for, in seconds
    #[clap(long, default_value = "30")]
    duration: f64,
}

/// Shared transport config: keep-alive so a long GE loss burst can't trip the
/// idle timeout mid-stream, and generous idle timeout / uni-stream allowance.
fn transport_config() -> quinn::TransportConfig {
    let mut t = quinn::TransportConfig::default();
    t.max_concurrent_uni_streams(8u8.into());
    t.max_idle_timeout(Some(Duration::from_secs(60).try_into().unwrap()));
    t.keep_alive_interval(Some(Duration::from_secs(1)));
    t
}

fn now_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Server(opt) => run_server(opt).await,
        Cmd::Client(opt) => run_client(opt).await,
    }
}

async fn run_server(opt: ServerOpt) -> Result<()> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());

    // Self-signed cert (same pattern as quinn-perf's server).
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let key: PrivateKeyDer = PrivatePkcs8KeyDer::from(cert.signing_key.serialize_der()).into();
    let certs = vec![CertificateDer::from(cert.cert)];

    let mut crypto = rustls::ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .context("building server crypto")?;
    crypto.alpn_protocols = vec![ALPN.to_vec()];

    let mut server_config =
        ServerConfig::with_crypto(Arc::new(QuicServerConfig::try_from(crypto)?));
    server_config.transport = Arc::new(transport_config());

    let endpoint = Endpoint::server(server_config, opt.listen)?;
    eprintln!("msg_lat server listening on {}", endpoint.local_addr()?);

    let incoming = endpoint.accept().await.context("endpoint closed")?;
    let connection = incoming.await.context("handshake failed")?;
    eprintln!("connection from {}", connection.remote_address());

    let mut recv = connection.accept_uni().await.context("accept_uni")?;

    let mut buf: Vec<u8> = Vec::new();
    let mut tmp = vec![0u8; 65536];
    let mut lats: Vec<f64> = Vec::new();

    loop {
        match recv.read(&mut tmp).await {
            Ok(Some(k)) if k > 0 => {
                buf.extend_from_slice(&tmp[..k]);
                let mut pos = 0usize;
                while buf.len() - pos >= 4 {
                    let n = u32::from_be_bytes([
                        buf[pos],
                        buf[pos + 1],
                        buf[pos + 2],
                        buf[pos + 3],
                    ]) as usize;
                    if buf.len() - pos < 4 + n || n < 8 {
                        if n < 8 {
                            // malformed / truncated frame; stop parsing
                            break;
                        }
                        break;
                    }
                    let recv_ns = now_ns();
                    let msg = &buf[pos + 4..pos + 4 + n];
                    let send_ns = u64::from_be_bytes(msg[..8].try_into().unwrap());
                    lats.push(recv_ns.wrapping_sub(send_ns) as f64 / 1e6);
                    pos += 4 + n;
                }
                if pos > 0 {
                    buf.drain(..pos);
                }
            }
            Ok(Some(_)) => {}
            Ok(None) => break,
            Err(_) => break,
        }
    }

    print_percentiles(&mut lats);
    // Close the connection so the client's `closed()` wait returns promptly,
    // then flush before exit. One connection is measured per invocation.
    connection.close(0u32.into(), b"done");
    endpoint.wait_idle().await;
    Ok(())
}

fn print_percentiles(lats: &mut [f64]) {
    if lats.is_empty() {
        println!("{{\"summary\": true, \"mode\": \"stream\", \"protocol\": \"quic\", \"count\": 0}}");
        return;
    }
    lats.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let len = lats.len();
    let q = |p: f64| lats[std::cmp::min(len - 1, (len as f64 * p) as usize)];
    let sum: f64 = lats.iter().sum();
    println!(
        "{{\"summary\": true, \"mode\": \"stream\", \"protocol\": \"quic\", \"count\": {}, \
         \"p50_ms\": {:.3}, \"p90_ms\": {:.3}, \"p99_ms\": {:.3}, \"p999_ms\": {:.3}, \
         \"max_ms\": {:.3}, \"mean_ms\": {:.3}}}",
        len,
        q(0.50),
        q(0.90),
        q(0.99),
        q(0.999),
        lats[len - 1],
        sum / len as f64,
    );
}

async fn run_client(opt: ClientOpt) -> Result<()> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());

    let mut crypto = rustls::ClientConfig::builder_with_provider(provider.clone())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .unwrap()
        .dangerous()
        .with_custom_certificate_verifier(SkipServerVerification::new(provider))
        .with_no_client_auth();
    crypto.alpn_protocols = vec![ALPN.to_vec()];

    let mut client_config = ClientConfig::new(Arc::new(QuicClientConfig::try_from(crypto)?));
    client_config.transport_config(Arc::new(transport_config()));

    let mut endpoint = Endpoint::client(SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 0))?;
    endpoint.set_default_client_config(client_config);

    eprintln!("connecting to {} ({})", opt.ip, opt.server_name);
    let connection = endpoint
        .connect(opt.ip, &opt.server_name)?
        .await
        .context("connecting")?;
    eprintln!("connected");

    let mut send = connection.open_uni().await.context("open_uni")?;

    let payload = vec![b'Z'; opt.size.saturating_sub(8)];
    let n = 8 + payload.len();
    let interval_s = 1.0 / opt.rate;
    let start = tokio::time::Instant::now();
    let end = start + Duration::from_secs_f64(opt.duration);
    let mut i: u64 = 0;

    while tokio::time::Instant::now() < end {
        let send_ns = now_ns();
        let mut frame = Vec::with_capacity(4 + n);
        frame.extend_from_slice(&(n as u32).to_be_bytes());
        frame.extend_from_slice(&send_ns.to_be_bytes());
        frame.extend_from_slice(&payload);
        send.write_all(&frame).await.context("write_all")?;
        i += 1;
        let next = start + Duration::from_secs_f64(interval_s * i as f64);
        tokio::time::sleep_until(next).await;
    }

    send.finish().context("finish")?;
    // Keep the connection alive until the server has drained the whole reliable
    // stream (including FIN + tail retransmits under loss) and closes it — this
    // guarantees no tail messages are dropped by an early teardown. Bounded so a
    // genuinely broken link cannot hang the run forever.
    let _ = tokio::time::timeout(Duration::from_secs(120), connection.closed()).await;
    endpoint.wait_idle().await;

    println!("{{\"stream_client_done\": true, \"sent\": {}}}", i);
    Ok(())
}

/// Dummy verifier that accepts any server certificate (matches quinn-perf's
/// SkipServerVerification / raptorpath's quic.rs SkipServerVerification).
#[derive(Debug)]
struct SkipServerVerification(Arc<rustls::crypto::CryptoProvider>);

impl SkipServerVerification {
    fn new(provider: Arc<rustls::crypto::CryptoProvider>) -> Arc<Self> {
        Arc::new(Self(provider))
    }
}

impl rustls::client::danger::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}
