# RaptorPath

Multipath network tunnel with fountain code FEC. Bonds multiple network paths (WiFi + LTE + Ethernet) into a single virtual interface, using forward error correction to tolerate per-path loss without the head-of-line blocking problem of traditional MPTCP.

## Why not regular MPTCP?

Standard MPTCP with round-robin scheduling degrades to the speed of the **worst** link — one lossy path stalls the entire connection waiting for retransmissions. RaptorPath takes a fundamentally different approach:

- **Fountain codes instead of retransmission**: send original packets + computed repair packets. The receiver can reconstruct from *any* sufficient subset of symbols, regardless of which specific packets were lost or which path they arrived on.
- **Source-first transmission**: original data is sent first with zero encoding latency. Repair symbols follow. If nothing is lost, there's no decoding overhead at all.
- **Loss-aware scheduling**: source symbols go to low-RTT paths for minimum latency; repair symbols go to high-goodput paths for maximum reliability.
- **Constant tail loss targeting**: instead of a fixed FEC percentage, the system computes the exact redundancy needed for a target tail loss probability (e.g., 1-in-100,000 block failure rate), using binomial statistics + a PI feedback controller.

## Project Status

**30 ADRs resolved.** The core data path is fully wired with five FEC backends, sliding-window FEC, Gilbert-Elliott burst modeling, runtime backend auto-switching, BBR congestion control, tapered interleaving, and multipath window scheduling. See [docs/adr/](docs/adr/) for the full decision log.

### Features
- Five swappable FEC backends: RaptorQ, METTLE, Reed-Solomon, RLC, Streaming
- Sliding-window FEC pipeline (RLC, METTLE-window, Streaming backends)
- Runtime FEC backend auto-switching with loss-based heuristic
- Source-first symbol emission with on-demand repair generation
- Bayesian loss estimation with EWMA, Beta-Binomial, and burst detection
- Gilbert-Elliott HMM for correlated loss / burst modeling
- Feedforward FEC rate computation (binomial model) + PI feedback controller
- Tapered block interleaving with repair distribution for burst resilience
- WindowNack sender-initiated repair for window-mode backends
- Multipath scheduler (RTT-aware for source, goodput-aware for repair)
- Multipath window symbol scheduling for sliding-window backends
- BBR-inspired delay-based congestion control with ProbeRTT phase
- QUIC transport with per-path connections and datagram framing
- TUN interface creation on Linux and Windows
- Length-prefixed packet framing (block + window mode) preserving IP boundaries
- ACK/feedback loop with echo-based RTT measurement
- Protocol versioning with 8-byte magic+version header and handshake (v3)
- Block profiles tuned by protocol hint (realtime/bulk/auto)
- Channel backpressure with bounded queues and drop-on-full
- TOML config with layered merging (profile → file → CLI)
- Preflight environment checks (`raptorpath check`)
- HTTP monitoring endpoint with runtime stats (`/status`, `/health`)
- Graceful shutdown with partial block flush and peer notification
- Automatic route and DNS management with cleanup on shutdown
- `setup` command for automated wintun.dll installation on Windows

For the full algorithm inventory and modularity matrix, see [docs/FEATURES.md](docs/FEATURES.md).

## Architecture

See [DESIGN.md](DESIGN.md) for the full architecture diagram and design decisions.

```
App ──▶ TUN ──▶ Block Assembly ──▶ FEC Encode ──▶ Scheduler ──▶ QUIC paths
                                                                      │
App ◀── TUN ◀── Packet Extract ◀── FEC Decode  ◀────────────────────┘
```

## Prerequisites

### Both platforms
- Rust toolchain (install via [rustup](https://rustup.rs))
- Administrator / root privileges (required for TUN interface)

### Windows
- Visual Studio Build Tools with C++ workload
- Windows SDK (for kernel32.lib etc.)
- [WinTUN driver](https://www.wintun.net/) — run `raptorpath setup` to install automatically, or download manually

### Linux
- Root or `CAP_NET_ADMIN` capability
- TUN kernel module (usually loaded by default)

## Build

```bash
cargo build --release
```

## Usage

### Server (listener)
```bash
sudo raptorpath run --server \
  --bind 0.0.0.0:4433,0.0.0.0:4434 \
  --tun-name rpath0 \
  --tun-addr 10.99.0.1/24
```

### Client
```bash
sudo raptorpath run \
  --bind 0.0.0.0:4433,0.0.0.0:4434 \
  --peer 203.0.113.1:4433,203.0.113.1:4434 \
  --tun-name rpath0 \
  --tun-addr 10.99.0.2/24 \
  --route 192.168.50.0/24,10.0.0.0/8 \
  --dns 10.99.0.1
```

### Setup (Windows)
```bash
# Download and install wintun.dll automatically
raptorpath setup
```

### Preflight checks
```bash
raptorpath check --bind 0.0.0.0:4433 --server
```

### Runtime monitoring
```bash
# Start with monitoring enabled
sudo raptorpath run --status-addr 127.0.0.1:9820 ...

# Query stats from another terminal
raptorpath status
raptorpath status --json
```

### Configuration file
```bash
sudo raptorpath run --config raptorpath.toml
```

Example `raptorpath.toml`:
```toml
server = false
bind = ["0.0.0.0:4433", "0.0.0.0:4434"]
peer = ["203.0.113.1:4433", "203.0.113.1:4434"]
tun_name = "rpath0"
tun_addr = "10.99.0.2/24"
protocol_hint = "realtime"
fec_backend = "mettle"         # "raptorq" (default), "mettle", "rs", "rlc", or "streaming"
route = ["192.168.50.0/24"]
dns = "10.99.0.1"
status_addr = "127.0.0.1:9820"

# Runtime FEC backend switching (config-only, no CLI flags)
fec_auto_switch = true         # auto-switch between backends based on loss (default: true)
fec_switch_threshold_low = 0.01   # below this loss rate → RaptorQ
fec_switch_threshold_high = 0.10  # above this loss rate → Mettle
fec_switch_interval = 5           # minimum seconds between switches
```

CLI flags override config file values. See `--help` for all available fields.

### Options (run subcommand)

| Flag | Default | Description |
|------|---------|-------------|
| `--server` | false | Run as listener |
| `--bind` | required | Local addresses, one per path (comma-separated) |
| `--peer` | required (client) | Remote addresses, one per path |
| `--tun-name` | `rpath0` | Virtual interface name |
| `--tun-addr` | `10.99.0.1/24` | Interface IP in CIDR notation |
| `--target-tail-loss` | `1e-5` | Target probability of block decode failure |
| `--max-fec-overhead` | `0.5` | Max repair symbols as fraction of source (50%) |
| `--protocol-hint` | `auto` | `realtime`, `bulk`, or `auto` |
| `--profile` | none | Config profile: `home` or `datacenter` |
| `--config` | none | Path to TOML config file |
| `--status-addr` | none | Address for HTTP monitoring endpoint |
| `--route` | none | Routes to add through tunnel (CIDR, comma-separated) |
| `--dns` | none | DNS server to configure on tunnel interface |
| `--interleave-depth` | auto | Block interleaving depth (1=disabled, 2+=burst resilience) |
| `--pin-cert` | none | Path to pinned TLS certificate (DER or PEM) |
| `--fec-backend` | `raptorq` | FEC backend: `raptorq`, `mettle`, `rs`, `rlc`, or `streaming` |

### Environment

| Variable | Effect |
|----------|--------|
| `RUST_LOG=raptorpath=debug` | Verbose logging |
| `RUST_LOG=raptorpath=trace` | Very verbose (per-symbol) |

## Project Structure

```
raptorpath/                      (workspace root)
├── raptorpath/                  Main crate
│   └── src/
│       ├── main.rs              CLI entry point (run/check/status/setup)
│       ├── lib.rs               Library re-exports
│       ├── config.rs            TOML config with profile/layered merging
│       ├── preflight.rs         Pre-run environment checks
│       ├── routing.rs           Route and DNS management
│       ├── net/
│       │   ├── mod.rs           Orchestration (TUN ↔ FEC ↔ transport, window pipeline)
│       │   ├── framing.rs       Length-prefixed packet framing (block + window)
│       │   └── interleave.rs    Block interleaving with tapered repair
│       ├── fec/
│       │   ├── traits.rs        FecEncoder/FecDecoder traits, FecBackend enum
│       │   ├── window_traits.rs WindowEncoder/WindowDecoder traits
│       │   ├── raptorq_backend.rs  RaptorQ implementation (RFC 6330)
│       │   ├── mettle_backend.rs   METTLE block-mode adapter
│       │   ├── mettle_window.rs    METTLE window-mode adapter
│       │   ├── rs_backend.rs       Reed-Solomon (GF(256) MDS)
│       │   ├── rlc_backend.rs      RLC block-mode (RFC 8681)
│       │   ├── rlc_window.rs       RLC window-mode (sliding window)
│       │   ├── streaming.rs        Streaming code (Badr/Martinian layered)
│       │   ├── gf256.rs            GF(256) field arithmetic
│       │   └── stream.rs           Streaming FEC interface
│       ├── control/
│       │   ├── estimator.rs     Bayesian loss estimation (EWMA + Beta-Binomial)
│       │   ├── gilbert_elliott.rs  Gilbert-Elliott HMM (burst modeling)
│       │   ├── fec_rate.rs      FEC rate controller (feedforward + PI)
│       │   └── backend_selector.rs Runtime FEC backend switching
│       ├── scheduler/
│       │   ├── mod.rs           Multipath scheduler + BBR congestion control
│       │   └── clock.rs         Testable time source
│       ├── transport/
│       │   ├── protocol.rs      Wire protocol (versioned framing, handshake)
│       │   └── quic.rs          QUIC transport (quinn)
│       ├── monitor/
│       │   ├── stats.rs         Lock-free SharedStats with atomics
│       │   └── http.rs          Axum HTTP endpoint (/status, /health)
│       └── tun/
│           ├── mod.rs           Platform-agnostic TUN interface
│           ├── linux/mod.rs     Linux TUN (kernel driver)
│           └── windows/mod.rs   Windows TUN (wintun)
└── mettle/                      Standalone METTLE erasure code crate
    └── src/
        ├── lib.rs               Public API and MettleConfig
        ├── encoder.rs           Streaming encoder with bin accumulation
        ├── decoder.rs           Peeling decoder
        ├── graph.rs             Tanner graph / hash-based edge generation
        └── gf2.rs               GF(2) XOR packet operations
```

## License

MIT OR Apache-2.0
