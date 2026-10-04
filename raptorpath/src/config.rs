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
    /// FEC backend: "raptorq" (default), "rs", or "rlc"
    pub fec_backend: Option<String>,
    /// Deprecated (parsed, warned, ignored). The codec is chosen once at
    /// startup (per config/hint) and never changes mid-stream: a switch
    /// would strand every in-flight symbol of the old code (no cross-code
    /// algebra, paper §5.10).
    pub fec_switch_threshold_low: Option<f64>,
    /// Deprecated (parsed, warned, ignored) — see fec_switch_threshold_low.
    pub fec_switch_threshold_high: Option<f64>,
    /// Deprecated (parsed, warned, ignored) — see fec_switch_threshold_low.
    pub fec_switch_interval: Option<u64>,
    /// Deprecated (parsed, warned, ignored) — see fec_switch_threshold_low.
    pub fec_auto_switch: Option<bool>,
    /// Run the sliding-window pipeline with the retain-until-acked policy
    /// for this stream (paper §5.1). Retention is the
    /// ARQ layer's contract: sent source bytes are retained in a store
    /// until the peer acks them (removal by ack only), aged SACK-confirmed
    /// holes are recovered by targeted retransmits from the store, and a
    /// full store becomes TUN-read backpressure — never data loss. The
    /// coding window keeps sliding freely (it is only the FEC horizon).
    /// The receiver holds delivery at holes until they are recovered
    /// (NACK/repair), never force-delivering past them. Also routes
    /// Bulk/Auto hints onto the window pipeline (RLC codec unless
    /// fec_backend says otherwise). Default false: Bulk/Auto stay on block
    /// mode, Realtime keeps its lossy evict window (correct for its δ).
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
    /// Inner-feedback weight in [0,1] (paper §4.4): mid-stream repair
    /// floor for payloads whose delivery latency feeds back into their own
    /// throughput (TCP-in-tunnel). Default 0.0 (measured neutral at c2,
    /// regressive at c3); set 1.0 to enable the floor.
    pub inner_feedback_weight: Option<f64>,
    /// Block-granular multipath source affinity (paper §5.5): a whole
    /// block's source symbols ride one path; blocks are WRR-distributed by
    /// capacity share. Default true; false restores per-symbol striping
    /// (ablation).
    pub mp_block_affinity: Option<bool>,
    /// Out-of-order object delivery on the reliable sliding window (the
    /// H → ∞ corner, paper §4.11). When set (object/perf path only,
    /// requires `window_reliable`), the receiver hands each decoded source
    /// symbol to the consumer the instant it decodes — in any order — and
    /// the sender's retention backpressure is relaxed so a stalled in-order
    /// frontier no longer throttles the fast path. The native object API
    /// reassembles by offset and completes on total-decoded, so no in-order
    /// frontier is needed. Default false: the TCP-in-tunnel path keeps its
    /// in-order delivery contract (a live inner stream does need the
    /// frontier). Not a codec/rate change — just the delivery latency
    /// budget H raised to ∞ for a bounded object.
    pub window_out_of_order: Option<bool>,
    /// Coded-only window (coded-object mode): on the reliable sliding
    /// window, emit only coded (random-linear-combination) symbols over the
    /// window — no raw systematic source. Any K linearly independent coded
    /// symbols from any path reconstruct the K window sources (GF(256),
    /// MDS-tight), so no symbol is a fixed in-order position a slow path can
    /// long-pole. Bulk-object / loose-δ only: pays a window-fill decode
    /// latency before any delivery, so it implies out-of-order delivery and
    /// requires `window_reliable`. Realtime / in-order streams stay
    /// systematic. Default false (paper §10).
    pub window_coded_only: Option<bool>,
    /// Generation-based cross-path coding (the generation seat, paper
    /// §5.8). Partitions the object's source symbols into fixed generations
    /// of ~W_mp and emits random-linear-combination symbols within each
    /// generation (a stable coding anchor, unlike the moving sliding
    /// window). Any K_G independent coded symbols from any path reconstruct
    /// generation g, which decodes out-of-order the instant K_G arrive;
    /// recovery is generation-level (more coded symbols for a short
    /// generation) with no per-seq targeted ARQ beneath the code. Implies
    /// coded-only wire symbols + out-of-order delivery; requires
    /// `window_reliable`. Bulk-object / loose-δ only. `RWM_GEN` (default
    /// 384) and `RWM_PIPELINE` (default 2) tune G and M. Default false.
    pub window_generation_coding: Option<bool>,
    /// Systematic + deficit-driven cross-path repair (paper §5.8): the
    /// generation machinery with the raw systematic source as primary
    /// (delivered on arrival, zero decode); coded symbols are windowed
    /// repair only (`ceil(len·r)` proactive per generation of ~W_mp +
    /// deficit-driven top-up), so decode is O(deficit) (the holes) not O(G)
    /// and nothing waits for K_G. No per-seq ARQ; implies out-of-order
    /// delivery; requires `window_reliable`. `RWM_GEN` sets the repair
    /// window / fungibility horizon, `RWM_GEN_R` (default 0.15) the
    /// proactive overhead. Bulk-object / loose-δ only. Default false.
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

    // Default interleave depth based on protocol hint (a declared
    // hint-keyed corner, paper §11.1). Bulk uses 1: interleaving delays
    // every block's completion by (depth-1) block serialization times, and
    // for TCP-in-tunnel that inflates the inner RTT in a closed loop
    // (slower inner TCP → lower rate → longer block serialization → higher
    // latency). Block-mode ARQ + in-order block delivery handle burst loss
    // reactively, so the burst-spreading insurance does not pay its
    // latency cost there.
    let default_interleave = match protocol_hint {
        ProtocolHint::Realtime => 2,
        ProtocolHint::Bulk => 1,
        ProtocolHint::Auto => 3,
    };

    let fec_backend = match config.fec_backend.as_deref() {
        Some("reed-solomon") | Some("rs") => FecBackend::ReedSolomon,
        Some("rlc") => FecBackend::Rlc,
        // The streaming machine is gone; the unified span machine
        // (ADR-0064) replaces it.
        Some("streaming") => anyhow::bail!(
            "fec_backend 'streaming' was removed: the streaming two-layer machine \
             was retired in favour of the unified span machine (ADR-0064). \
             Realtime rides the unified RLC span machine (default); RWM_UNIFIED=0 selects the \
             legacy-RLC windowed machine. Available: raptorq, rs, rlc"
        ),
        Some("raptorq") | None => FecBackend::RaptorQ,
        Some(other) => anyhow::bail!("unknown fec_backend '{other}'. Available: raptorq, rs, rlc"),
    };

    let fec_backend_explicit = config.fec_backend.is_some();

    // The codec is pinned at startup (paper §5.10). The old auto-switch
    // knobs are still parsed so existing
    // configs keep loading, but they are ignored — warn when set.
    if config.fec_auto_switch == Some(true) {
        tracing::warn!(
            "config: fec_auto_switch is deprecated and ignored — mid-stream FEC \
             backend switching was removed (codec is pinned at startup; paper §5.10)"
        );
    }
    if config.fec_switch_threshold_low.is_some()
        || config.fec_switch_threshold_high.is_some()
        || config.fec_switch_interval.is_some()
    {
        tracing::warn!(
            "config: fec_switch_threshold_low/high and fec_switch_interval are \
             deprecated and ignored — mid-stream FEC backend switching was removed \
             (paper §5.10)"
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
        reorder_timeout_ms: config.reorder_timeout_ms.unwrap_or(20),
        reorder_max_size: config.reorder_max_size.unwrap_or(500),
        // Mid-stream repair floor for inner-feedback payloads
        // (TCP-in-tunnel), paper §4.4. Default 0.0: the inner flow absorbs
        // the residual ARQ stalls, and the floor's repair volume displaces
        // source symbols inside the same inner-limited closed loop. The
        // knob is kept for payload semantics that measure differently.
        inner_feedback_weight: config
            .inner_feedback_weight
            .unwrap_or(0.0)
            .clamp(0.0, 1.0),
        // The completion feed (paper §4.6). Always `None`
        // here — the tunnel is an endless stream and has no `T_rem` to
        // publish. Only a driver that knows the size of what it is sending
        // (the perf client, under `RWM_COMPLETION_EXPOSURE`) sets it.
        completion_feed: None,
        mp_block_affinity: config.mp_block_affinity.unwrap_or(true),
        // Out-of-order object delivery (H→∞). Default false —
        // set only by the perf/native-object path (which is bounded and
        // reassembles by offset). The run() tunnel path keeps in-order.
        window_out_of_order: config.window_out_of_order.unwrap_or(false),
        // Coded-only window (coded-object mode). Default false — set
        // only by the native object / perf path (bulk, loose-δ). Coded-only
        // implies out-of-order delivery (it pays window-fill decode latency).
        window_coded_only: config.window_coded_only.unwrap_or(false),
        // Generation coding (stable anchor, paper §5.8). Default
        // false — set only by the native object / perf path (bulk, loose-δ).
        // Implies coded-only wire symbols + out-of-order delivery.
        window_generation_coding: config.window_generation_coding.unwrap_or(false),
        // Systematic + deficit-repair. Default false — set only by
        // the native object / perf path (bulk, loose-δ). A submode of generation
        // coding: source rides the wire as primary, coded is windowed repair only.
        window_systematic_repair: config.window_systematic_repair.unwrap_or(false),
    };

    Ok((peer_config, status_addr))
}

/// Per-fix anchor-hygiene gate (ADR-0061) with the `RWM_ANCHOR_HYGIENE`
/// umbrella as its default — `RWM_ANCHOR_HYGIENE=1` turns the whole anchor-repair
/// family on for a battery, while each fix stays individually A/B-able
/// (`RWM_ASTAR_ANCHOR`, `RWM_MSTAR_ANCHOR`, `RWM_PLAIN_RS`, `RWM_CLOCK_GAP`).
/// Everything defaults off: the shipped path is byte-identical unset.
pub fn anchor_gate(name: &str) -> bool {
    anchor_gate_default(name, false)
}

/// `anchor_gate` with a per-gate shipped default: a
/// member of the anchor-hygiene family can flip its own default while the
/// umbrella semantics are preserved — `RWM_ANCHOR_HYGIENE`, when set, overrides the
/// family default in either direction (`=1` all on, `=0` all off), the
/// individual gate env always wins, and unset-everything yields `default`.
pub fn anchor_gate_default(name: &str, default: bool) -> bool {
    env_flag(name, env_flag("RWM_ANCHOR_HYGIENE", default))
}

/// The boolean dialect for every env gate (trimmed, case-insensitive):
/// `1`/`true`/`on`/`yes` → on; `0`/`false`/`off`/`no`/empty → off; anything
/// else → `None`.
pub fn parse_bool(v: &str) -> Option<bool> {
    match v.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "on" | "yes" => Some(true),
        "" | "0" | "false" | "off" | "no" => Some(false),
        _ => None,
    }
}

/// Boolean `RWM_*` env gate parser — the one way to read an on/off gate.
///
///   - unset → `default` (shipped default preserved)
///   - set → [`parse_bool`]; an unrecognised value (a typo such as `of`, or
///     a number) is a hard error naming the variable, never a silent on or
///     off. The engine resolves its gates at startup (`RuntimeGates::resolve`
///     from `main`), so the error surfaces before any transfer.
///
/// Numeric-value knobs (e.g. `RWM_GEN_R=0.03`, `RWM_STORE=..`) do not use
/// this — they parse their value, and 0 may be a legitimate value there.
pub fn env_flag(name: &str, default: bool) -> bool {
    match std::env::var(name) {
        Err(_) => default,
        Ok(v) => parse_bool(&v).unwrap_or_else(|| {
            panic!(
                "invalid boolean {v:?} for env gate {name}: expected one of \
                 1/0/true/false/on/off/yes/no (case-insensitive)"
            )
        }),
    }
}

#[cfg(test)]
mod env_flag_tests {
    use super::env_flag;

    // Each test uses unique var names so parallel test threads never race.

    #[test]
    fn unset_returns_the_default() {
        std::env::remove_var("RWM_TEST_EF_UNSET");
        assert!(!env_flag("RWM_TEST_EF_UNSET", false));
        assert!(env_flag("RWM_TEST_EF_UNSET", true));
    }

    #[test]
    fn zero_false_and_empty_are_off_even_when_default_on() {
        for (var, val) in [
            ("RWM_TEST_EF_ZERO", "0"),
            ("RWM_TEST_EF_FALSE", "false"),
            ("RWM_TEST_EF_FALSE_UC", "FALSE"),
            ("RWM_TEST_EF_FALSE_MC", "False"),
            ("RWM_TEST_EF_EMPTY", ""),
            ("RWM_TEST_EF_WS", "  0  "),
        ] {
            std::env::set_var(var, val);
            assert!(!env_flag(var, false), "{var}={val:?} must be OFF");
            assert!(!env_flag(var, true), "{var}={val:?} must be OFF");
            std::env::remove_var(var);
        }
    }

    #[test]
    fn off_and_no_are_off_on_and_yes_are_on_case_insensitive() {
        for (var, val, want) in [
            ("RWM_TEST_EF_OFF", "off", false),
            ("RWM_TEST_EF_OFF_UC", "OFF", false),
            ("RWM_TEST_EF_NO", "no", false),
            ("RWM_TEST_EF_NO_MC", "No", false),
            ("RWM_TEST_EF_ON", "on", true),
            ("RWM_TEST_EF_YES_UC", "YES", true),
            ("RWM_TEST_EF_TRUE_WS", " true ", true),
        ] {
            std::env::set_var(var, val);
            assert_eq!(env_flag(var, !want), want, "{var}={val:?}");
            std::env::remove_var(var);
        }
    }

    #[test]
    #[should_panic(expected = "RWM_TEST_EF_GARBAGE")]
    fn an_unrecognised_boolean_is_an_error_naming_the_variable() {
        std::env::set_var("RWM_TEST_EF_GARBAGE", "of");
        let _ = env_flag("RWM_TEST_EF_GARBAGE", false);
    }

    #[test]
    #[should_panic(expected = "RWM_TEST_EF_NUMBER")]
    fn a_number_is_not_a_boolean() {
        std::env::set_var("RWM_TEST_EF_NUMBER", "16");
        let _ = env_flag("RWM_TEST_EF_NUMBER", false);
    }

    #[test]
    fn one_true_and_yes_are_on_even_when_default_off() {
        for (var, val) in [
            ("RWM_TEST_EF_ONE", "1"),
            ("RWM_TEST_EF_TRUE", "true"),
            ("RWM_TEST_EF_YES", "yes"),
        ] {
            std::env::set_var(var, val);
            assert!(env_flag(var, false), "{var}={val:?} must be ON");
            assert!(env_flag(var, true), "{var}={val:?} must be ON");
            std::env::remove_var(var);
        }
    }
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
            fec_backend: Some("rs".into()),
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
        assert_eq!(parsed.fec_backend.as_deref(), Some("rs"));
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
        // Default off (paper §4.4: measured completion-neutral at c2 and
        // regressive at c3).
        let bulk = RaptorpathConfig {
            protocol_hint: Some("bulk".into()),
            ..Default::default()
        };
        let (pc, _) = resolve(&bulk).unwrap();
        assert_eq!(pc.inner_feedback_weight, 0.0);
        // Explicit opt-in wins.
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
        // Default off: bulk stays on block mode.
        let bulk = RaptorpathConfig {
            protocol_hint: Some("bulk".into()),
            ..Default::default()
        };
        let (pc, _) = resolve(&bulk).unwrap();
        assert!(!pc.window_reliable);
        // Explicit opt-in.
        let opt_in = RaptorpathConfig {
            protocol_hint: Some("bulk".into()),
            window_reliable: Some(true),
            ..Default::default()
        };
        let (pc, _) = resolve(&opt_in).unwrap();
        assert!(pc.window_reliable);
    }

    /// ADR-0069: there is no block pipeline to fall back to, so every
    /// config that would have selected it is a startup error naming the ADR
    /// — never a silent re-route.
    #[test]
    fn block_only_config_is_an_error_naming_adr_0069() {
        let cases: Vec<(&str, RaptorpathConfig)> = vec![
            ("raptorq", RaptorpathConfig { fec_backend: Some("raptorq".into()), ..Default::default() }),
            ("rs", RaptorpathConfig { fec_backend: Some("rs".into()), ..Default::default() }),
            ("reed-solomon", RaptorpathConfig { fec_backend: Some("reed-solomon".into()), ..Default::default() }),
            ("interleave_depth", RaptorpathConfig { interleave_depth: Some(3), ..Default::default() }),
            ("mp_block_affinity", RaptorpathConfig { mp_block_affinity: Some(false), ..Default::default() }),
        ];
        for (what, cfg) in cases {
            let err = match resolve(&cfg) {
                Ok(_) => panic!("{what}: a block-only setting must not resolve"),
                Err(e) => e.to_string(),
            };
            assert!(err.contains("ADR-0069"), "{what}: the error names ADR-0069: {err}");
        }
        // `rlc`, the window pipeline's codec, still resolves explicitly.
        let rlc = RaptorpathConfig { fec_backend: Some("rlc".into()), ..Default::default() };
        assert!(resolve(&rlc).is_ok());
    }

    #[test]
    fn test_deprecated_switch_fields_still_parse() {
        // Old configs with auto-switch knobs must keep loading (warned,
        // ignored): pinning the codec is not allowed to break configs.
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
