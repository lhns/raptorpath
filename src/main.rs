mod config;
mod control;
mod fec;
mod monitor;
mod net;
mod preflight;
mod scheduler;
mod transport;
mod tun;

use clap::{Parser, Subcommand};
use std::net::SocketAddr;
use std::path::PathBuf;
use tracing::info;

#[derive(Parser, Debug)]
#[command(name = "raptorpath", about = "Multipath transport with RaptorQ FEC")]
struct Cli {
    /// Config file path (TOML)
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run the multipath tunnel (default)
    Run(RunArgs),
    /// Validate environment and configuration
    Check,
    /// Query status of a running raptorpath instance
    Status(StatusArgs),
}

#[derive(Parser, Debug)]
struct RunArgs {
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
    #[arg(long)]
    tun_name: Option<String>,

    /// TUN interface IP (CIDR notation)
    #[arg(long)]
    tun_addr: Option<String>,

    /// Target tail loss probability (e.g. 1e-6)
    #[arg(long)]
    target_tail_loss: Option<f64>,

    /// Maximum FEC overhead ratio
    #[arg(long)]
    max_fec_overhead: Option<f64>,

    /// Protocol hint: "realtime", "bulk", "auto"
    #[arg(long)]
    protocol_hint: Option<String>,

    /// Configuration profile: "home", "datacenter"
    #[arg(long)]
    profile: Option<String>,

    /// Status endpoint address (e.g. 127.0.0.1:9820)
    #[arg(long)]
    status_addr: Option<String>,
}

#[derive(Parser, Debug)]
struct StatusArgs {
    /// Address of the running raptorpath status endpoint
    #[arg(long, default_value = "127.0.0.1:9820")]
    addr: String,

    /// Output raw JSON instead of formatted table
    #[arg(long)]
    json: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("raptorpath=info".parse()?),
        )
        .init();

    let cli = Cli::parse();

    match cli.command.unwrap_or(Commands::Run(RunArgs {
        server: false,
        bind: vec![],
        peer: vec![],
        tun_name: None,
        tun_addr: None,
        target_tail_loss: None,
        max_fec_overhead: None,
        protocol_hint: None,
        profile: None,
        status_addr: None,
    })) {
        Commands::Run(args) => cmd_run(cli.config, args).await,
        Commands::Check => cmd_check(cli.config).await,
        Commands::Status(args) => cmd_status(args).await,
    }
}

async fn cmd_run(config_path: Option<PathBuf>, args: RunArgs) -> anyhow::Result<()> {
    // Build config: profile defaults -> TOML file -> CLI args
    let mut base_config = config::RaptorpathConfig::default();

    // Apply profile defaults if specified
    if let Some(ref profile_name) = args.profile {
        let profile: config::Profile = profile_name.parse()?;
        base_config = config::merge(base_config, profile.defaults());
    }

    // Load TOML config if specified
    if let Some(ref path) = config_path {
        let file_config = config::load_config(path)?;
        base_config = config::merge(base_config, file_config);
    }

    // Overlay CLI args
    let cli_overlay = config::RaptorpathConfig {
        server: if args.server { Some(true) } else { None },
        bind: if args.bind.is_empty() {
            None
        } else {
            Some(args.bind.iter().map(|a| a.to_string()).collect())
        },
        peer: if args.peer.is_empty() {
            None
        } else {
            Some(args.peer.iter().map(|a| a.to_string()).collect())
        },
        tun_name: args.tun_name,
        tun_addr: args.tun_addr,
        target_tail_loss: args.target_tail_loss,
        max_fec_overhead: args.max_fec_overhead,
        protocol_hint: args.protocol_hint,
        status_addr: args.status_addr,
    };
    let final_config = config::merge(base_config, cli_overlay);
    let (peer_config, status_addr) = config::resolve(&final_config)?;

    info!(?peer_config, "resolved configuration");

    // Run preflight checks
    println!("Preflight checks:");
    let checks = preflight::run_checks(&peer_config.bind_addrs, peer_config.is_server, status_addr);
    if preflight::print_and_check(&checks) {
        anyhow::bail!("Preflight checks failed. Fix the issues above and try again.");
    }
    println!();

    // Build the full PeerConfig with status_addr
    let config = net::PeerConfig {
        bind_addrs: peer_config.bind_addrs,
        peer_addrs: peer_config.peer_addrs,
        tun_name: peer_config.tun_name,
        tun_addr: peer_config.tun_addr,
        target_tail_loss: peer_config.target_tail_loss,
        max_fec_overhead: peer_config.max_fec_overhead,
        protocol_hint: peer_config.protocol_hint,
        is_server: peer_config.is_server,
        status_addr,
    };

    net::run(config).await
}

async fn cmd_check(config_path: Option<PathBuf>) -> anyhow::Result<()> {
    let mut base_config = config::RaptorpathConfig::default();
    if let Some(ref path) = config_path {
        let file_config = config::load_config(path)?;
        base_config = config::merge(base_config, file_config);
    }
    let (peer_config, status_addr) = config::resolve(&base_config)?;

    println!("Preflight checks:");
    let checks = preflight::run_checks(&peer_config.bind_addrs, peer_config.is_server, status_addr);
    let has_failure = preflight::print_and_check(&checks);

    if has_failure {
        println!("\nSome checks failed.");
        std::process::exit(1);
    } else {
        println!("\nAll checks passed.");
        Ok(())
    }
}

async fn cmd_status(args: StatusArgs) -> anyhow::Result<()> {
    let addr: SocketAddr = args.addr.parse()
        .map_err(|e| anyhow::anyhow!("invalid status address '{}': {e}", args.addr))?;

    // Simple HTTP GET via raw TCP
    let mut stream = tokio::net::TcpStream::connect(addr).await
        .map_err(|e| anyhow::anyhow!(
            "cannot connect to raptorpath at {addr}: {e}\n\
             Make sure raptorpath is running with --status-addr {addr}"
        ))?;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let request = format!("GET /status HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await?;

    let mut response = String::new();
    stream.read_to_string(&mut response).await?;

    // Skip HTTP headers (find \r\n\r\n)
    let body = response
        .split("\r\n\r\n")
        .nth(1)
        .unwrap_or(&response);

    if args.json {
        println!("{body}");
        return Ok(());
    }

    // Parse and pretty-print
    let snap: serde_json::Value = serde_json::from_str(body)
        .map_err(|e| anyhow::anyhow!("failed to parse status response: {e}"))?;

    println!("raptorpath status (connected to {addr})\n");

    // Uptime
    if let Some(uptime) = snap.get("uptime_secs").and_then(|v| v.as_f64()) {
        let mins = uptime as u64 / 60;
        let secs = uptime as u64 % 60;
        println!("Uptime: {}m {}s\n", mins, secs);
    }

    // Paths
    if let Some(paths) = snap.get("paths").and_then(|v| v.as_array()) {
        println!("Paths:");
        for p in paths {
            let id = p.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
            let active = p.get("active").and_then(|v| v.as_bool()).unwrap_or(false);
            let state = if active { "active" } else { "inactive" };
            let rtt = p.get("rtt_ms").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let loss = p.get("loss_rate").and_then(|v| v.as_f64()).unwrap_or(0.0) * 100.0;
            let tp = p.get("throughput_mbps").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let cwnd = p.get("cwnd").and_then(|v| v.as_u64()).unwrap_or(0);
            let in_flight = p.get("in_flight").and_then(|v| v.as_u64()).unwrap_or(0);
            println!(
                "  Path {id:<3} {state:<8}  RTT: {rtt:>6.1}ms  Loss: {loss:>5.1}%  \
                 Throughput: {tp:>7.1} Mbps  cwnd: {cwnd}  in_flight: {in_flight}"
            );
        }
        println!();
    }

    // FEC
    if let Some(fec) = snap.get("fec") {
        let target = fec.get("target_tail_loss").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let actual = fec.get("actual_failure_rate").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let overhead = fec.get("overhead_ratio").and_then(|v| v.as_f64()).unwrap_or(0.0) * 100.0;
        let pi = fec.get("pi_correction").and_then(|v| v.as_f64()).unwrap_or(0.0);
        println!("FEC:");
        println!("  Target tail loss: {target:.1e}  Actual failure rate: {actual:.1e}  Overhead: {overhead:.1}%");
        println!("  PI correction: {pi:+.3}");
        println!();
    }

    // Blocks
    if let Some(blocks) = snap.get("blocks") {
        let enc = blocks.get("encoded").and_then(|v| v.as_u64()).unwrap_or(0);
        let ok = blocks.get("decoded_ok").and_then(|v| v.as_u64()).unwrap_or(0);
        let fail = blocks.get("decoded_fail").and_then(|v| v.as_u64()).unwrap_or(0);
        let pending = blocks.get("pending").and_then(|v| v.as_u64()).unwrap_or(0);
        println!("Blocks:");
        println!("  Encoded: {enc}  Decoded OK: {ok}  Failed: {fail}  Pending: {pending}");
    }

    Ok(())
}
