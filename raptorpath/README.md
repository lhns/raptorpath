# RaptorPath

A multipath transport that combines forward error correction with ARQ. It bonds
several network paths (for example WiFi + LTE) into one TUN interface. Each path
is its own QUIC connection, and packets are protected by a sliding-window random
linear code with SACK-driven retransmission for what the code does not cover.

The design goal is one machine tuned by three continuous dials rather than
separate modes:

- **δ**: the latency price. How much delay a flow will trade for throughput.
- **ρ**: the retention contract. The fraction of data that must eventually arrive.
- **r**: the proactive repair rate, derived from the measured loss process.

The protocol hints `realtime`, `auto` and `bulk` are named points on these dials.
[DESIGN.md](DESIGN.md) maps the dials to the code, and the paper derives them.

## Status

Research code, measured on an emulated L1 rig (netem/tc cells, see
[`docs/l1-harness-plan.md`](docs/l1-harness-plan.md)). Wire protocol version
**v8** (`PROTOCOL_VERSION` in `src/transport/protocol.rs`). Both peers must run
the same version; mismatches are refused at handshake.

[`docs/status.md`](docs/status.md) is the authoritative record of the default
stack, the most recent verdicts and the open debts. In short:

- Realtime runs the window pipeline: the unified RLC span machine with EVICT
  retention (ρ < 1) and δ-honest shedding.
- Bulk and Auto still default to the legacy **block pipeline** (RaptorQ + block
  ARQ). `--window-reliable` moves them onto the window pipeline with
  retain-until-acked (ρ = 1). The default flips only if the pre-registered
  re-test in ADR-0069 and `docs/status.md` §4 passes.
- The QUIC congestion controller underneath is quinn BBR (`RWM_QUIC_CC`
  overrides it).
- Experiment arms and instruments are `RWM_*` environment gates, resolved once
  at start (`src/gates.rs`). Most are off by default.

## Prerequisites

- A Rust toolchain ([rustup](https://rustup.rs)).
- Administrator or root rights for the TUN interface (`perf` does not need them).
- **Linux:** root or `CAP_NET_ADMIN`, with the TUN module loaded.
- **Windows:** Visual Studio Build Tools (C++ workload), the Windows SDK and
  [WinTUN](https://www.wintun.net/). `raptorpath setup` installs `wintun.dll`.

## Build and test

```bash
cargo build --release
cargo test -p raptorpath -p raptorpath-math --release -- --test-threads=2
```

## Usage

Subcommands: `run` (the tunnel, the default), `check`, `status`, `perf` and
`setup`. `--config <file.toml>` is global. Run `raptorpath <cmd> --help` for
every flag.

### Tunnel

```bash
# server
sudo raptorpath run --server \
  --bind 0.0.0.0:4433,0.0.0.0:4434 \
  --tun-name rpath0 --tun-addr 10.99.0.1/24

# client: one --bind and one --peer per path
sudo raptorpath run \
  --bind 0.0.0.0:4433,0.0.0.0:4434 \
  --peer 203.0.113.1:4433,203.0.113.1:4434 \
  --tun-name rpath0 --tun-addr 10.99.0.2/24 \
  --route 192.168.50.0/24 --dns 10.99.0.1 \
  --protocol-hint auto --window-reliable
```

| flag | default | meaning |
|---|---|---|
| `--server` | off | listen instead of connecting |
| `--bind`, `--peer` | — | local and remote addresses, one per path |
| `--tun-name`, `--tun-addr` | `rpath0`, `10.99.0.1/24` | the virtual interface |
| `--protocol-hint` | `auto` | `realtime`, `auto` or `bulk` (a point on the δ dial) |
| `--window-reliable` | off | Bulk/Auto on the window pipeline, retain-until-acked |
| `--fec-backend` | `raptorq` | `raptorq`, `rs` or `rlc`; the window pipeline needs `rlc` (selected automatically when unset) |
| `--target-tail-loss`, `--max-fec-overhead` | `1e-5`, `0.5` | repair contract inputs |
| `--profile` | none | `home` or `datacenter` presets |
| `--route`, `--dns` | none | routes and DNS through the tunnel |
| `--pin-cert` | none | pin the server certificate (DER or PEM), ADR-0020 |
| `--status-addr` | none | enable the HTTP endpoint (`/status`, `/health`, `/paths`) |

Configuration is layered: profile, then TOML file, then CLI. TOML keys use the
flag names with underscores (`bind = ["0.0.0.0:4433"]`, `protocol_hint =
"realtime"`, ...).

### Other subcommands

```bash
raptorpath check                      # preflight: privileges, TUN driver, config
raptorpath status [--addr 127.0.0.1:9820] [--json]
raptorpath setup                      # Windows: install wintun.dll
```

### `perf`: object benchmark without a kernel TUN

`perf` sends fixed-size objects over the real engine through a memory TUN. The
L1 harness uses it.

```bash
raptorpath perf --server --bind 0.0.0.0:4433,0.0.0.0:4434
raptorpath perf --client --peer 10.0.0.1:4433,10.0.0.1:4434 \
  --bytes 1800000 --runs 10 --protocol-hint bulk --window-reliable
```

The flags `--window-out-of-order`, `--window-coded-only`,
`--window-generation-coding` and `--window-systematic-repair` select opt-in
experiment arms, and each needs `--window-reliable`. `docs/status.md` covers
their standing.

### Logging

`RUST_LOG=raptorpath=debug` (or `trace`). The diagnostic readouts (`[DIAG]`,
`[GATES]`, ...) are described where they are defined in `src/net/`.

## Repository layout

| path | contents |
|---|---|
| `raptorpath/` | the engine crate; module map in [DESIGN.md](DESIGN.md) |
| `raptorpath-math/` | closed-form laws shared by the engine and the model |
| `gf256/` | GF(2^8) arithmetic with SSSE3/AVX2 kernels |
| `raptorpath-wasm/`, `raptorpath-visualizer/` | the L0 model and interactive visualizer |
| `raptorpath/tools/l1/` | the L1 measurement harness |

## Documentation

| document | what it is |
|---|---|
| [`docs/fec-arq-model.md`](docs/fec-arq-model.md) | the paper: channel model, rate law, span machine, flow control, recovery, CC |
| [`docs/status.md`](docs/status.md) | the default stack, recent verdicts, open debts, the pending block re-test |
| [`docs/measurement-discipline.md`](docs/measurement-discipline.md) | binding rules for any L1 verdict, the verdict taxonomy, the VM protocol |
| [`docs/adr/README.md`](docs/adr/README.md) | the architecture decision index |
| [`docs/benchmark-methodology.md`](docs/benchmark-methodology.md) | the in-process benchmark suite (`tests/bench_suite.rs`) and how to read it |
| [`docs/l1-harness-plan.md`](docs/l1-harness-plan.md) | the emulated-network harness |
| [`DESIGN.md`](DESIGN.md) | the architecture mapped onto the code |

## License

MIT OR Apache-2.0
