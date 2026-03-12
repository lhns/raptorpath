# RaptorPath

Multipath network tunnel with RaptorQ fountain code FEC. Bonds multiple network paths (WiFi + LTE + Ethernet) into a single virtual interface, using forward error correction to tolerate per-path loss without the head-of-line blocking problem of traditional MPTCP.

## Why not regular MPTCP?

Standard MPTCP with round-robin scheduling degrades to the speed of the **worst** link — one lossy path stalls the entire connection waiting for retransmissions. RaptorPath takes a fundamentally different approach:

- **Fountain codes instead of retransmission**: send original packets + computed repair packets. The receiver can reconstruct from *any* sufficient subset of symbols, regardless of which specific packets were lost or which path they arrived on.
- **Source-first transmission**: original data is sent first with zero encoding latency. Repair symbols follow. If nothing is lost, there's no decoding overhead at all.
- **Loss-aware scheduling**: source symbols go to low-RTT paths for minimum latency; repair symbols go to high-goodput paths for maximum reliability.
- **Constant tail loss targeting**: instead of a fixed FEC percentage, the system computes the exact redundancy needed for a target tail loss probability (e.g., 1-in-100,000 block failure rate), using binomial statistics + a PI feedback controller.

## Project Status

**Prototype / proof of concept.** The core architecture compiles and the data path is wired up, but several critical issues remain before end-to-end operation. See [docs/adr/](docs/adr/) for the full list.

### What works
- RaptorQ encoding/decoding with source-first symbol emission
- Bayesian loss estimation with EWMA and burst detection
- Feedforward FEC rate computation (binomial model) + PI feedback controller
- Multipath scheduler (RTT-aware for source, goodput-aware for repair)
- QUIC transport with per-path connections and datagram framing
- TUN interface creation on Linux and Windows
- CLI with configurable tail loss target, FEC overhead cap, protocol hints

### What doesn't work yet
- **Packet framing** — decoded blocks don't preserve IP packet boundaries ([ADR-0002](docs/adr/0002-packet-framing-after-decode.md))
- **Loss feedback** — estimator is fed incorrect data, FEC rate control is effectively disabled ([ADR-0003](docs/adr/0003-loss-estimation-is-broken.md))
- **Block metadata** — receiver never learns encoding params for blocks ([ADR-0008](docs/adr/0008-blockstart-not-handled.md))
- **ACK mechanism** — no receiver→sender feedback at all ([ADR-0005](docs/adr/0005-ack-mechanism-missing.md))
- **Block flush** — partial blocks wait indefinitely for more data ([ADR-0001](docs/adr/0001-block-assembly-timeout.md))
- **Congestion control** — none; will flood the network ([ADR-0009](docs/adr/0009-no-congestion-control.md))

See [docs/adr/README.md](docs/adr/README.md) for the prioritized issue list.

## Architecture

See [DESIGN.md](DESIGN.md) for the full architecture diagram and design decisions.

```
App ──▶ TUN ──▶ Block Assembly ──▶ RaptorQ Encode ──▶ Scheduler ──▶ QUIC paths
                                                                        │
App ◀── TUN ◀── Packet Extract ◀── RaptorQ Decode  ◀──────────────────┘
```

## Prerequisites

### Both platforms
- Rust toolchain (install via [rustup](https://rustup.rs))
- Administrator / root privileges (required for TUN interface)

### Windows
- Visual Studio Build Tools with C++ workload
- Windows SDK (for kernel32.lib etc.)
- [WinTUN driver](https://www.wintun.net/) — download `wintun.dll` and place in your PATH or next to the binary

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
sudo raptorpath --server \
  --bind 0.0.0.0:4433,0.0.0.0:4434 \
  --tun-name rpath0 \
  --tun-addr 10.99.0.1/24
```

### Client
```bash
sudo raptorpath \
  --bind 0.0.0.0:4433,0.0.0.0:4434 \
  --peer 203.0.113.1:4433,203.0.113.1:4434 \
  --tun-name rpath0 \
  --tun-addr 10.99.0.2/24
```

### Options

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

### Environment

| Variable | Effect |
|----------|--------|
| `RUST_LOG=raptorpath=debug` | Verbose logging |
| `RUST_LOG=raptorpath=trace` | Very verbose (per-symbol) |

## Project Structure

```
src/
├── main.rs              CLI entry point
├── lib.rs               Library re-exports
├── net/mod.rs           Orchestration (TUN ↔ FEC ↔ transport)
├── fec/
│   ├── codec.rs         RaptorQ encoder/decoder wrapper
│   └── stream.rs        Streaming FEC interface
├── control/
│   ├── estimator.rs     Bayesian loss rate estimation
│   └── fec_rate.rs      FEC rate controller (feedforward + PI)
├── scheduler/mod.rs     Multipath symbol scheduling
├── transport/
│   ├── protocol.rs      Wire protocol definitions
│   └── quic.rs          QUIC transport (quinn)
└── tun/
    ├── mod.rs           Platform-agnostic TUN interface
    ├── linux/mod.rs     Linux TUN (kernel driver)
    └── windows/mod.rs   Windows TUN (wintun)
```

## License

MIT OR Apache-2.0
