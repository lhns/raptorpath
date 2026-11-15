# raptorpath

A multipath transport that combines forward error correction with ARQ. It
bonds several network paths (for example WiFi + LTE) into one TUN interface;
each path is its own QUIC connection, and packets are protected by a
sliding-window random linear code with SACK-driven retransmission for what the
code does not cover. Instead of a realtime mode and a bulk mode it is one
machine parameterised by the triangle (δ, ρ, r) — a latency price δ, a
retention contract ρ and a correction rate r — on measured channel anchors,
continuous in δ with no mode bit. The protocol hints `realtime`, `auto` and
`bulk` are named points on those dials. Research code, measured on an
emulated L1 rig.

## Workspace

| path | what it is |
|---|---|
| [`raptorpath/`](raptorpath/) | the engine crate (binary + library); usage in [`raptorpath/README.md`](raptorpath/README.md), module map in [`raptorpath/DESIGN.md`](raptorpath/DESIGN.md) |
| [`raptorpath-math/`](raptorpath-math/) | closed-form laws and estimators shared by the engine and the model |
| [`gf256/`](gf256/) | GF(2^8) arithmetic with SSSE3/AVX2 kernels |
| [`raptorpath-wasm/`](raptorpath-wasm/) | WASM bindings and the L0 model simulator for the visualizer |
| [`raptorpath-visualizer/`](raptorpath-visualizer/) | the interactive visualizer (one self-contained HTML file) and its continuity gates |

## Where things live

| what | where |
|---|---|
| the paper (derivations of every law) | [`docs/fec-arq-model.md`](docs/fec-arq-model.md) |
| what ships, verdicts, open debts, pre-registered batteries | [`docs/status.md`](docs/status.md) (authoritative; cited by `§` number) |
| architecture decisions | [`docs/adr/`](docs/adr/README.md) |
| measurement discipline (binding for any L1 verdict) | [`docs/measurement-discipline.md`](docs/measurement-discipline.md) |
| the L1 harness (netem cells on a VM) | [`tools/l1/`](tools/l1/), plan in [`docs/l1-harness-plan.md`](docs/l1-harness-plan.md) |
| raw L1 ledgers (evidence cited by status and the paper) | [`docs/l1-raw/`](docs/l1-raw/) |
| in-process benchmark suite | [`docs/benchmark-methodology.md`](docs/benchmark-methodology.md) |
| research notes | [`docs/research/`](docs/research/) |

## Build and test

From the workspace root:

```bash
cargo build --release
cargo test -p raptorpath -p raptorpath-math --release -- --test-threads=2
cargo test --doc -p raptorpath --release
cargo test -p raptorpath-wasm
```

The L1 harness's python tests (`test_stubrun.py` re-runs the session driver
end to end against stubs, about four minutes per launch path; the VM suites
skip it):

```bash
cd tools/l1 && for t in test_*.py; do python3 "$t"; done
```

The visualizer (needs `wasm-pack` and `node`; runs both continuity gates and
fails the build on a gate failure), and the paper as PDF (needs Pandoc and
Typst):

```bash
bash raptorpath-visualizer/build_visualizer.sh
bash tools/build_paper.sh
```

Running the tunnel, `perf` and the `RWM_*` gates: see
[`raptorpath/README.md`](raptorpath/README.md).

## Standing rules

[`CLAUDE.md`](CLAUDE.md) holds the rules every change must keep: the
no-mode-switch invariant, the testing discipline, formula-first laws and the
scope rule that keeps the visualizer a self-contained model.

## License

MIT OR Apache-2.0
