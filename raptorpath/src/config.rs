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
    /// DEPRECATED (parsed, warned, ignored): mid-stream FEC backend
    /// auto-switching was removed (paper §16.4). A switch strands every
    /// in-flight symbol of the old code (no cross-code algebra), discards
    /// estimator/ARQ state, and the hard 0.01/0.12 loss thresholds violated
    /// the no-hard-cutoffs convention. The codec is chosen once at startup
    /// (per config/hint) and never changes mid-stream.
    pub fec_switch_threshold_low: Option<f64>,
    /// DEPRECATED (parsed, warned, ignored) — see fec_switch_threshold_low.
    pub fec_switch_threshold_high: Option<f64>,
    /// DEPRECATED (parsed, warned, ignored) — see fec_switch_threshold_low.
    pub fec_switch_interval: Option<u64>,
    /// DEPRECATED (parsed, warned, ignored) — see fec_switch_threshold_low.
    pub fec_auto_switch: Option<bool>,
    /// RWM Phase A (paper §15.7/§16.3): run the sliding-window pipeline
    /// with the RETAIN-UNTIL-ACKED policy for this stream. Retention is the
    /// ARQ layer's contract: sent source bytes are retained in a store
    /// until the peer acks them (removal by ack only), aged SACK-confirmed
    /// holes are recovered by targeted retransmits from the store, and a
    /// full store becomes TUN-read backpressure — never data loss. The
    /// coding window keeps sliding freely (it is only the FEC horizon).
    /// The receiver holds delivery at holes until they are recovered
    /// (NACK/repair), never force-delivering past them. Also routes
    /// Bulk/Auto hints onto the window pipeline (RLC codec unless
    /// fec_backend says otherwise). Default false: Bulk/Auto stay on block
    /// mode, Realtime keeps its lossy EVICT window (correct for its δ).
    pub window_reliable: Option<bool>,
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
    /// Inner-feedback weight in [0,1] (paper 14.28): mid-stream repair
    /// floor for payloads whose delivery latency feeds back into their own
    /// throughput (TCP-in-tunnel). Default 0.0 (L1-measured: neutral at
    /// C2, regressive at C3); set 1.0 to enable the floor.
    pub inner_feedback_weight: Option<f64>,
    /// Block-granular multipath source affinity (paper 13.8 in-order
    /// coupling refinement, L2 ws1): a whole block's source symbols ride
    /// one path; blocks are WRR-distributed by capacity share. Default
    /// true; false restores legacy per-symbol striping (ablation).
    pub mp_block_affinity: Option<bool>,
    /// RWM Phase C (paper §16.2, H→∞ corner): out-of-order OBJECT delivery
    /// on the reliable sliding window. When set (object/perf path only,
    /// requires `window_reliable`), the receiver hands each decoded source
    /// symbol to the consumer the instant it decodes — in ANY order — and
    /// the sender's retention backpressure is relaxed so a stalled in-order
    /// frontier no longer throttles the fast path. The native object API
    /// reassembles by offset and completes on total-decoded, so no in-order
    /// frontier is needed. Default false: the TCP-in-tunnel path keeps its
    /// in-order delivery contract (a live inner stream DOES need the
    /// frontier). Not a codec/rate change — just the delivery latency
    /// budget H raised to ∞ for a bounded object.
    pub window_out_of_order: Option<bool>,
    /// Fungible frontier (paper §16.3 "empty quadrant", coded-object mode):
    /// on the reliable sliding window, emit ONLY coded (random-linear-
    /// combination) symbols over the window — no raw systematic source. Any
    /// K linearly independent coded symbols from ANY path reconstruct the K
    /// window sources (GF(256), MDS-tight), so no symbol is a fixed in-order
    /// position a slow path can long-pole (the §16.7 systematic-window cap).
    /// Bulk-object / loose-δ ONLY: pays a window-fill decode latency before
    /// any delivery, so it implies out-of-order delivery and requires
    /// `window_reliable`. Realtime / in-order streams stay systematic.
    /// Default false.
    pub window_coded_only: Option<bool>,
    /// Generation-based cross-path fungible coding (paper §16.3, the
    /// oracle-validated fix for the coded-*sliding*-window drag). Partitions
    /// the object's source symbols into FIXED generations of ~W_mp (384–512 at
    /// C8) and emits RANDOM-LINEAR-COMBINATION symbols WITHIN each generation
    /// (a STABLE coding anchor, unlike the moving sliding window). Any K_G
    /// independent coded symbols from ANY path reconstruct generation g, which
    /// decodes out-of-order the instant K_G arrive; recovery is generation-level
    /// (more coded symbols for a short generation) with NO per-seq targeted ARQ
    /// beneath the code — the per-seq layer is exactly what made the moving
    /// window path-affine and invoked the ADR-0046 throttle (measured ×0.26 at
    /// C8). Implies coded-only wire symbols + out-of-order delivery; requires
    /// `window_reliable`. Bulk-object / loose-δ ONLY. `RWM_GEN` (default 384)
    /// and `RWM_PIPELINE` (default 2) tune G and M. Default false.
    pub window_generation_coding: Option<bool>,
    /// Systematic + deficit-driven cross-path REPAIR (paper §16.3 oracle — the
    /// cheaper realization of generation coding that reaches ×1.19 at C8 without
    /// coded-only's two L1-killers). Reuses the generation machinery but sends
    /// the RAW systematic source as primary (delivered on arrival, ZERO decode);
    /// coded symbols are windowed REPAIR only (`ceil(len·r)` proactive per
    /// generation of ~W_mp + deficit-driven top-up), so decode is O(deficit)
    /// (the holes) not O(G) and nothing waits for K_G. NO per-seq ARQ; implies
    /// out-of-order delivery; requires `window_reliable`. `RWM_GEN` (~480 at C8)
    /// sets the repair window / fungibility horizon, `RWM_GEN_R` (default 0.15)
    /// the proactive overhead. Bulk-object / loose-δ ONLY. Default false.
    pub window_systematic_repair: Option<bool>,
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
        window_reliable: overlay.window_reliable.or(base.window_reliable),
        enable_pi_feedback: overlay.enable_pi_feedback.or(base.enable_pi_feedback),
        ge_burst_factor: overlay.ge_burst_factor.or(base.ge_burst_factor),
        realtime_burst_extra: overlay.realtime_burst_extra.or(base.realtime_burst_extra),
        reorder_timeout_ms: overlay.reorder_timeout_ms.or(base.reorder_timeout_ms),
        reorder_max_size: overlay.reorder_max_size.or(base.reorder_max_size),
        inner_feedback_weight: overlay.inner_feedback_weight.or(base.inner_feedback_weight),
        mp_block_affinity: overlay.mp_block_affinity.or(base.mp_block_affinity),
        window_out_of_order: overlay.window_out_of_order.or(base.window_out_of_order),
        window_coded_only: overlay.window_coded_only.or(base.window_coded_only),
        window_generation_coding: overlay
            .window_generation_coding
            .or(base.window_generation_coding),
        window_systematic_repair: overlay
            .window_systematic_repair
            .or(base.window_systematic_repair),
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

    // Mid-stream FEC backend auto-switching was REMOVED (paper §16.4): the
    // codec is pinned at startup. The old knobs are still parsed so existing
    // configs keep loading, but they are ignored — warn when set.
    if config.fec_auto_switch == Some(true) {
        tracing::warn!(
            "config: fec_auto_switch is deprecated and ignored — mid-stream FEC \
             backend switching was removed (codec is pinned at startup; paper §16.4)"
        );
    }
    if config.fec_switch_threshold_low.is_some()
        || config.fec_switch_threshold_high.is_some()
        || config.fec_switch_interval.is_some()
    {
        tracing::warn!(
            "config: fec_switch_threshold_low/high and fec_switch_interval are \
             deprecated and ignored — mid-stream FEC backend switching was removed \
             (paper §16.4)"
        );
    }

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
        window_reliable: config.window_reliable.unwrap_or(false),
        enable_pi_feedback: config.enable_pi_feedback.unwrap_or(true),
        symbol_size_override: 0, // use profile default
        reorder_timeout_ms: config.reorder_timeout_ms.unwrap_or(20),
        reorder_max_size: config.reorder_max_size.unwrap_or(500),
        // Paper 14.28 (P10a): mid-stream repair floor for inner-feedback
        // payloads (TCP-in-tunnel). Default 0.0 — the L1 ablation MEASURED
        // the floor active (client FEC volume 2.5% -> 4.7% at C2) with NO
        // completion or inner-retransmit improvement at C2 and a 28%
        // median REGRESSION at C3: post-P8/P9b the inner flow absorbs the
        // residual ARQ stalls, and the floor's repair volume displaces
        // source symbols inside the same inner-limited closed loop. The
        // knob is kept for payload semantics that measure differently; see
        // docs/fec-arq-model.md 14.28 and docs/goal-gate.md P10a.
        inner_feedback_weight: config
            .inner_feedback_weight
            .unwrap_or(0.0)
            .clamp(0.0, 1.0),
        mp_block_affinity: config.mp_block_affinity.unwrap_or(true),
        // RWM Phase C: out-of-order object delivery (H→∞). Default false —
        // set only by the perf/native-object path (which is bounded and
        // reassembles by offset). The run() tunnel path keeps in-order.
        window_out_of_order: config.window_out_of_order.unwrap_or(false),
        // Fungible frontier (§16.3 coded-object mode). Default false — set
        // only by the native object / perf path (bulk, loose-δ). Coded-only
        // implies out-of-order delivery (it pays window-fill decode latency).
        window_coded_only: config.window_coded_only.unwrap_or(false),
        // Generation-based fungible coding (§16.3 stable anchor). Default
        // false — set only by the native object / perf path (bulk, loose-δ).
        // Implies coded-only wire symbols + out-of-order delivery.
        window_generation_coding: config.window_generation_coding.unwrap_or(false),
        // Systematic + deficit-repair (§16.3 oracle). Default false — set only by
        // the native object / perf path (bulk, loose-δ). A submode of generation
        // coding: source rides the wire as primary, coded is windowed repair only.
        window_systematic_repair: config.window_systematic_repair.unwrap_or(false),
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
            window_reliable: Some(false),
            enable_pi_feedback: Some(false),
            ge_burst_factor: Some(0.0),
            realtime_burst_extra: Some(0.05),
            reorder_timeout_ms: Some(0),
            reorder_max_size: Some(200),
            inner_feedback_weight: Some(0.0),
            mp_block_affinity: Some(true),
            window_out_of_order: Some(false),
            window_coded_only: Some(false),
            window_generation_coding: Some(false),
            window_systematic_repair: Some(false),
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
        assert_eq!(parsed.inner_feedback_weight, Some(0.0));
    }

    #[test]
    fn test_inner_feedback_weight_defaults() {
        // Default OFF (paper 14.28: the L1 ablation measured the floor
        // active but completion-neutral at C2 and regressive at C3).
        let bulk = RaptorpathConfig {
            protocol_hint: Some("bulk".into()),
            ..Default::default()
        };
        let (pc, _) = resolve(&bulk).unwrap();
        assert_eq!(pc.inner_feedback_weight, 0.0);
        // Explicit opt-in wins (the L1 ablation arm / future payloads).
        let opt_in = RaptorpathConfig {
            protocol_hint: Some("bulk".into()),
            inner_feedback_weight: Some(1.0),
            ..Default::default()
        };
        let (pc, _) = resolve(&opt_in).unwrap();
        assert_eq!(pc.inner_feedback_weight, 1.0);
        // Out-of-range values clamp.
        let clamped = RaptorpathConfig {
            inner_feedback_weight: Some(3.0),
            ..Default::default()
        };
        let (pc, _) = resolve(&clamped).unwrap();
        assert_eq!(pc.inner_feedback_weight, 1.0);
    }

    #[test]
    fn test_window_reliable_default_off_and_opt_in() {
        // Default OFF: bulk stays on block mode (no big-bang switch).
        let bulk = RaptorpathConfig {
            protocol_hint: Some("bulk".into()),
            ..Default::default()
        };
        let (pc, _) = resolve(&bulk).unwrap();
        assert!(!pc.window_reliable);
        // Explicit opt-in (the RWM Phase A A/B arm).
        let opt_in = RaptorpathConfig {
            protocol_hint: Some("bulk".into()),
            window_reliable: Some(true),
            ..Default::default()
        };
        let (pc, _) = resolve(&opt_in).unwrap();
        assert!(pc.window_reliable);
    }

    #[test]
    fn test_deprecated_switch_fields_still_parse() {
        // Old configs with auto-switch knobs must keep loading (warned,
        // ignored) — paper §16.4 removal is not allowed to break configs.
        let cfg: RaptorpathConfig = toml::from_str(
            "fec_auto_switch = true\nfec_switch_threshold_low = 0.01\n\
             fec_switch_threshold_high = 0.12\nfec_switch_interval = 5\n",
        )
        .unwrap();
        assert_eq!(cfg.fec_auto_switch, Some(true));
        assert!(resolve(&cfg).is_ok());
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
