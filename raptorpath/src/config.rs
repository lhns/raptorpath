//! Configuration: TOML file loading, profile presets, CLI overlay.

use crate::control::fec_rate::ProtocolHint;
use crate::fec::FecBackend;
use crate::net::PeerConfig;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::Path;

/// TOML-serializable configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct RaptorpathConfig {
    pub server: Option<bool>,
    pub bind: Option<Vec<String>>,
    pub peer: Option<Vec<String>>,
    pub tun_name: Option<String>,
    pub tun_addr: Option<String>,
    pub target_tail_loss: Option<f64>,
    pub max_fec_overhead: Option<f64>,
    pub protocol_hint: Option<String>,
    pub status_addr: Option<String>,
    /// Additional routes to add through the tunnel (CIDR notation)
    pub route: Option<Vec<String>>,
    /// DNS server to configure on the tunnel interface
    pub dns: Option<String>,
    /// Block interleaving depth (1 = disabled, 2+ = spread burst loss across N blocks)
    pub interleave_depth: Option<u32>,
    /// Path to a pinned TLS certificate (DER or PEM) for server verification
    pub pin_cert: Option<String>,
    /// FEC backend: "raptorq" (default) or "mettle"
    pub fec_backend: Option<String>,
    /// Low threshold for FEC backend switching (below → RaptorQ), default 0.01
    pub fec_switch_threshold_low: Option<f64>,
    /// High threshold for FEC backend switching (above → Mettle), default 0.10
    pub fec_switch_threshold_high: Option<f64>,
    /// Minimum seconds between FEC backend switches, default 5
    pub fec_switch_interval: Option<u64>,
    /// Enable automatic FEC backend switching (default: true unless fec_backend is set)
    pub fec_auto_switch: Option<bool>,
    /// Enable PI feedback loop in FEC rate controller (default: true)
    pub enable_pi_feedback: Option<bool>,
    /// GE burst scaling multiplier; 0.0 = disabled (default: 0.10)
    pub ge_burst_factor: Option<f64>,
    /// Extra FEC % during bursts in realtime mode; 0.0 = disabled (default: 0.10)
    pub realtime_burst_extra: Option<f64>,
    /// Reorder buffer timeout in ms; 0 = disabled (default: 20)
    pub reorder_timeout_ms: Option<u64>,
    /// Reorder buffer max capacity (default: 500)
    pub reorder_max_size: Option<usize>,
}

/// Named configuration profiles with sensible defaults.
#[derive(Debug, Clone, Copy)]
pub enum Profile {
    /// Home network: WiFi + LTE, moderate loss, latency-sensitive
    Home,
    /// Datacenter: low loss, high throughput, stricter tail loss target
    Datacenter,
}

impl Profile {
    pub fn defaults(&self) -> RaptorpathConfig {
        match self {
            Profile::Home => RaptorpathConfig {
                target_tail_loss: Some(1e-4),
                max_fec_overhead: Some(0.3),
                protocol_hint: Some("auto".to_string()),
                ..Default::default()
            },
            Profile::Datacenter => RaptorpathConfig {
                target_tail_loss: Some(1e-6),
                max_fec_overhead: Some(0.5),
                protocol_hint: Some("bulk".to_string()),
                ..Default::default()
            },
        }
    }
}

impl std::str::FromStr for Profile {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "home" => Ok(Profile::Home),
            "datacenter" | "dc" => Ok(Profile::Datacenter),
            other => anyhow::bail!("unknown profile '{other}'. Available: home, datacenter"),
        }
    }
}

/// Load config from TOML file.
pub fn load_config(path: &Path) -> anyhow::Result<RaptorpathConfig> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("failed to read config file '{}': {e}", path.display()))?;
    let config: RaptorpathConfig = toml::from_str(&content)
        .map_err(|e| anyhow::anyhow!("failed to parse config file '{}': {e}", path.display()))?;
    Ok(config)
}

/// Merge two configs: `overlay` values take precedence over `base`.
pub fn merge(base: RaptorpathConfig, overlay: RaptorpathConfig) -> RaptorpathConfig {
    RaptorpathConfig {
        server: overlay.server.or(base.server),
        bind: overlay.bind.or(base.bind),
        peer: overlay.peer.or(base.peer),
        tun_name: overlay.tun_name.or(base.tun_name),
        tun_addr: overlay.tun_addr.or(base.tun_addr),
        target_tail_loss: overlay.target_tail_loss.or(base.target_tail_loss),
        max_fec_overhead: overlay.max_fec_overhead.or(base.max_fec_overhead),
        protocol_hint: overlay.protocol_hint.or(base.protocol_hint),
        status_addr: overlay.status_addr.or(base.status_addr),
        route: overlay.route.or(base.route),
        dns: overlay.dns.or(base.dns),
        interleave_depth: overlay.interleave_depth.or(base.interleave_depth),
        pin_cert: overlay.pin_cert.or(base.pin_cert),
        fec_backend: overlay.fec_backend.or(base.fec_backend),
        fec_switch_threshold_low: overlay.fec_switch_threshold_low.or(base.fec_switch_threshold_low),
        fec_switch_threshold_high: overlay.fec_switch_threshold_high.or(base.fec_switch_threshold_high),
        fec_switch_interval: overlay.fec_switch_interval.or(base.fec_switch_interval),
        fec_auto_switch: overlay.fec_auto_switch.or(base.fec_auto_switch),
        enable_pi_feedback: overlay.enable_pi_feedback.or(base.enable_pi_feedback),
        ge_burst_factor: overlay.ge_burst_factor.or(base.ge_burst_factor),
        realtime_burst_extra: overlay.realtime_burst_extra.or(base.realtime_burst_extra),
        reorder_timeout_ms: overlay.reorder_timeout_ms.or(base.reorder_timeout_ms),
        reorder_max_size: overlay.reorder_max_size.or(base.reorder_max_size),
    }
}

/// Convert resolved config into PeerConfig + optional status address.
pub fn resolve(config: &RaptorpathConfig) -> anyhow::Result<(PeerConfig, Option<SocketAddr>)> {
    let bind_addrs: Vec<SocketAddr> = config
        .bind
        .as_ref()
        .unwrap_or(&vec![])
        .iter()
        .map(|s| s.parse())
        .collect::<Result<_, _>>()
        .map_err(|e| anyhow::anyhow!("invalid bind address: {e}"))?;

    let peer_addrs: Vec<SocketAddr> = config
        .peer
        .as_ref()
        .unwrap_or(&vec![])
        .iter()
        .map(|s| s.parse())
        .collect::<Result<_, _>>()
        .map_err(|e| anyhow::anyhow!("invalid peer address: {e}"))?;

    let protocol_hint: ProtocolHint = config
        .protocol_hint
        .as_deref()
        .unwrap_or("auto")
        .parse()?;

    let status_addr: Option<SocketAddr> = config
        .status_addr
        .as_ref()
        .map(|s| s.parse())
        .transpose()
        .map_err(|e| anyhow::anyhow!("invalid status address: {e}"))?;

    let dns: Option<std::net::IpAddr> = config
        .dns
        .as_ref()
        .map(|s| s.parse())
        .transpose()
        .map_err(|e| anyhow::anyhow!("invalid DNS address: {e}"))?;

    // Default interleave depth based on protocol hint.
    //
    // Bulk was 4 and is now 1 (L1 C2 measurement): interleaving delays
    // every block's completion by (depth-1) block serialization times, and
    // for TCP-in-tunnel that inflates the inner RTT in a closed loop
    // (slower inner TCP → lower rate → longer block serialization → higher
    // latency). With block-mode ARQ (P8) + in-order block delivery
    // handling burst loss reactively, the burst-spreading insurance no
    // longer pays its latency cost: median 1.8MB completion 1.38s
    // (depth 1) vs 1.63s (depth 4) at C2, same inner-TCP retransmits.
    let default_interleave = match protocol_hint {
        ProtocolHint::Realtime => 2,
        ProtocolHint::Bulk => 1,
        ProtocolHint::Auto => 3,
    };

    let fec_backend = match config.fec_backend.as_deref() {
        Some("mettle") => FecBackend::Mettle,
        Some("reed-solomon") | Some("rs") => FecBackend::ReedSolomon,
        Some("rlc") => FecBackend::Rlc,
        Some("streaming") => FecBackend::Streaming,
        Some("raptorq") | None => FecBackend::RaptorQ,
        Some(other) => anyhow::bail!("unknown fec_backend '{other}'. Available: raptorq, mettle, rs, rlc, streaming"),
    };

    let fec_backend_explicit = config.fec_backend.is_some();

    // Auto-switching: disabled if fec_backend is explicitly set and fec_auto_switch is not explicitly true
    let fec_auto_switch = config.fec_auto_switch.unwrap_or(!fec_backend_explicit);

    let peer_config = PeerConfig {
        bind_addrs,
        peer_addrs,
        tun_name: config.tun_name.clone().unwrap_or_else(|| "rpath0".into()),
        tun_addr: config.tun_addr.clone().unwrap_or_else(|| "10.99.0.1/24".into()),
        target_tail_loss: config.target_tail_loss.unwrap_or(1e-5),
        max_fec_overhead: config.max_fec_overhead.unwrap_or(0.5),
        protocol_hint,
        is_server: config.server.unwrap_or(false),
        status_addr,
        routes: config.route.clone().unwrap_or_default(),
        dns,
        interleave_depth: config.interleave_depth.unwrap_or(default_interleave),
        pin_cert: config.pin_cert.as_ref().map(std::path::PathBuf::from),
        fec_backend,
        fec_backend_explicit,
        fec_switch_threshold_low: config.fec_switch_threshold_low.unwrap_or(0.01),
        fec_switch_threshold_high: config.fec_switch_threshold_high.unwrap_or(0.12),
        fec_switch_interval: config.fec_switch_interval.unwrap_or(5),
        fec_auto_switch,
        enable_pi_feedback: config.enable_pi_feedback.unwrap_or(true),
        symbol_size_override: 0, // use profile default
        reorder_timeout_ms: config.reorder_timeout_ms.unwrap_or(20),
        reorder_max_size: config.reorder_max_size.unwrap_or(500),
    };

    Ok((peer_config, status_addr))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_defaults() {
        let home = Profile::Home.defaults();
        assert_eq!(home.target_tail_loss, Some(1e-4));
        assert_eq!(home.max_fec_overhead, Some(0.3));

        let dc = Profile::Datacenter.defaults();
        assert_eq!(dc.target_tail_loss, Some(1e-6));
        assert_eq!(dc.max_fec_overhead, Some(0.5));
    }

    #[test]
    fn test_merge_overlay_wins() {
        let base = RaptorpathConfig {
            tun_name: Some("base".into()),
            target_tail_loss: Some(1e-5),
            ..Default::default()
        };
        let overlay = RaptorpathConfig {
            tun_name: Some("overlay".into()),
            ..Default::default()
        };
        let merged = merge(base, overlay);
        assert_eq!(merged.tun_name.as_deref(), Some("overlay"));
        assert_eq!(merged.target_tail_loss, Some(1e-5)); // from base
    }

    #[test]
    fn test_parse_profile() {
        assert!(matches!("home".parse::<Profile>().unwrap(), Profile::Home));
        assert!(matches!("datacenter".parse::<Profile>().unwrap(), Profile::Datacenter));
        assert!(matches!("dc".parse::<Profile>().unwrap(), Profile::Datacenter));
        assert!("unknown".parse::<Profile>().is_err());
    }

    #[test]
    fn test_toml_roundtrip() {
        let config = RaptorpathConfig {
            server: Some(true),
            bind: Some(vec!["0.0.0.0:4433".into()]),
            peer: Some(vec!["1.2.3.4:4433".into()]),
            tun_name: Some("rpath0".into()),
            tun_addr: Some("10.99.0.1/24".into()),
            target_tail_loss: Some(1e-5),
            max_fec_overhead: Some(0.5),
            protocol_hint: Some("auto".into()),
            status_addr: Some("127.0.0.1:9820".into()),
            route: Some(vec!["192.168.50.0/24".into()]),
            dns: Some("10.99.0.1".into()),
            interleave_depth: Some(3),
            pin_cert: None,
            fec_backend: Some("mettle".into()),
            fec_switch_threshold_low: Some(0.01),
            fec_switch_threshold_high: Some(0.10),
            fec_switch_interval: Some(5),
            fec_auto_switch: Some(true),
            enable_pi_feedback: Some(false),
            ge_burst_factor: Some(0.0),
            realtime_burst_extra: Some(0.05),
            reorder_timeout_ms: Some(0),
            reorder_max_size: Some(200),
        };
        let toml_str = toml::to_string(&config).unwrap();
        let parsed: RaptorpathConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.server, Some(true));
        assert_eq!(parsed.tun_name.as_deref(), Some("rpath0"));
        assert_eq!(parsed.fec_backend.as_deref(), Some("mettle"));
        assert_eq!(parsed.fec_switch_threshold_low, Some(0.01));
        assert_eq!(parsed.fec_auto_switch, Some(true));
        assert_eq!(parsed.enable_pi_feedback, Some(false));
        assert_eq!(parsed.ge_burst_factor, Some(0.0));
        assert_eq!(parsed.realtime_burst_extra, Some(0.05));
        assert_eq!(parsed.reorder_timeout_ms, Some(0));
        assert_eq!(parsed.reorder_max_size, Some(200));
    }

    #[test]
    fn test_resolve_defaults() {
        let config = RaptorpathConfig::default();
        let (peer_config, status_addr) = resolve(&config).unwrap();
        assert_eq!(peer_config.tun_name, "rpath0");
        assert_eq!(peer_config.tun_addr, "10.99.0.1/24");
        assert!(!peer_config.is_server);
        assert!(status_addr.is_none());
    }
}
