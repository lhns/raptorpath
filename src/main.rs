mod control;
mod fec;
mod net;
mod scheduler;
mod transport;
mod tun;

use clap::Parser;
use std::net::SocketAddr;
use tracing::info;

#[derive(Parser, Debug)]
#[command(name = "raptorpath", about = "Multipath transport with RaptorQ FEC")]
struct Args {
    /// Run as server (listener)
    #[arg(long)]
    server: bool,

    /// Local bind addresses (one per path)
    #[arg(long, value_delimiter = ',')]
    bind: Vec<SocketAddr>,

    /// Remote peer addresses (one per path, client mode)
    #[arg(long, value_delimiter = ',')]
    peer: Vec<SocketAddr>,

    /// TUN interface name
    #[arg(long, default_value = "rpath0")]
    tun_name: String,

    /// TUN interface IP (CIDR notation)
    #[arg(long, default_value = "10.99.0.1/24")]
    tun_addr: String,

    /// Target tail loss probability (e.g. 1e-6 for one-in-a-million)
    #[arg(long, default_value = "1e-5")]
    target_tail_loss: f64,

    /// Maximum FEC overhead ratio (safety cap)
    #[arg(long, default_value = "0.5")]
    max_fec_overhead: f64,

    /// Protocol hint for FEC aggressiveness: "realtime", "bulk", "auto"
    #[arg(long, default_value = "auto")]
    protocol_hint: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("raptorpath=info".parse()?),
        )
        .init();

    let args = Args::parse();
    info!(?args, "starting raptorpath");

    let config = net::PeerConfig {
        bind_addrs: args.bind,
        peer_addrs: args.peer,
        tun_name: args.tun_name,
        tun_addr: args.tun_addr,
        target_tail_loss: args.target_tail_loss,
        max_fec_overhead: args.max_fec_overhead,
        protocol_hint: args.protocol_hint.parse()?,
        is_server: args.server,
    };

    net::run(config).await
}
