# RaptorPath design

RaptorPath is a multipath FEC + ARQ transport. It carries IP packets from a TUN
interface (or objects, in `raptorpath perf`) over one QUIC connection per path,
protecting them with a sliding-window random linear code (RLC) and repairing
what the code does not cover with SACK-driven retransmission.

The theory lives in the paper, [`docs/fec-arq-model.md`](docs/fec-arq-model.md).
This file maps it onto the code. What ships by default, and the open debts, are
in [`docs/status.md`](docs/status.md), which is authoritative where the two
differ.

## One machine, three dials

The architecture's central claim is **one machine parameterized by the
(δ, ρ, r) triangle on measured anchors**: continuous in δ, with no mode bit.
The protocol hints (`realtime`, `auto`, `bulk`) are named points on the dials,
never modes (see the root `CLAUDE.md`).

| dial | meaning | where it is computed |
|---|---|---|
| δ | latency price: how much delay the flow will trade for throughput | `net/mod.rs` `delta_price` (hint → δ via `scheduler::hint_delta_price`; `RWM_DELTA` sets a number); `delta_budget_b_of` → `raptorpath_math::span_horizon_b` |
| ρ | retention contract: the fraction of data that must eventually arrive | ρ = 1 retain-until-acked (`--window-reliable`); ρ < 1 EVICT retention with δ-honest shedding (`shed_allowed`, `shed_deadline_us`, `shed_recv_budget_ok` in `net/mod.rs`) |
| r | proactive repair rate | `control/fec_rate.rs`: r* from the BOCD loss quantile and the window loss-mass tail; the rate mix r(β) = (1−β)·r_anchor + β·r_late-is-fine with β = `bulkness_of_delta(δ)`; `TaperBudget` spends it on the wire |

The shared closed forms (`compute_r_star`, `r_star_mass`, `span_horizon_b`,
`bulkness_of_delta`, `zeta_of_delta`, `p_lost`, ...) live in the
`raptorpath-math` crate, which the engine and the visualizer
(`raptorpath-wasm`, `raptorpath-visualizer/`) both call, so the model and the
engine evaluate the same law.

## Data path (window pipeline)

```
TUN / perf object
  └─ run_window_sender (net/mod.rs)
       └─ emit_source (net/emit_source.rs)         one packet → one source symbol
            ├─ encoder intake (fec/rlc_window.rs)   sliding-window RLC over GF(256)
            ├─ Scheduler::place_symbol → place_costs (scheduler/mod.rs)
            ├─ retention store + store-cap gate     (flow control, net/mod.rs)
            └─ proactive repair (TaperBudget, control/fec_rate.rs)
  └─ QuicTransport (transport/quic.rs)              QUIC DATAGRAM per path
                                                    wire format: transport/protocol.rs
peer ─ run_receiver (net/receiver.rs)
       ├─ UnifiedDecoder (fec/unified.rs)           one decoder for every RLC wire
       ├─ ReorderBuffer (net/reorder.rs)            in-order frontier / hold-at-hole
       └─ WindowAck (SACK) back to the sender
sender ─ on_window_ack (net/control_msg.rs)         store release, hole detection, retransmit
```

**Emission.** Source symbols go out first and unencoded, so a loss-free path
never decodes. Repair symbols are random linear combinations over the trailing
span. The span law (ADR-0064) sets width A*, depth M* and trailing offset from
(δ, ρ, r) and measured anchors (`control/anchor.rs`, ADR-0061).

**Placement.** `Scheduler::place_symbol` samples a path from a softmax over
`place_costs`: expected delivery time under the path's current load
(normalized by the fastest SRTT), its loss burden, and, for repair symbols,
how much of the covered data the path already carried. There is no hard
capacity cutoff; a saturated path's weight falls continuously. Each path keeps
engine-side state (`PathState`, with a Copa-lite `CopaState`) whose samples feed
the anchors. The congestion controller under each QUIC connection is quinn BBR
by default (`RWM_QUIC_CC`, ADR-0054).

**Flow control.** Sent symbols live in a retention store until the cumulative
ACK passes them. SACKed symbols release their store slot at once but keep their
payload for recovery (ADR-0060). The store cap is a derived law over path count,
per-path pipe and the δ price (`path_scaled_store_cap`, `pooled_store_cap`,
`pool_value_multiplier` and the honest-cap terms in `net/mod.rs`). The paper
section "Flow control: the store/δ-cap law" derives it.

**Recovery.** The receiver acknowledges with SACK ranges (`WindowAck`). The
sender turns gaps into holes (`sack_to_gaps`) and decides per hole, on that
flight's own path clock (RFC 9002 time and packet thresholds generalized per
path, `mp_hole_ripe`, ADR-0059), whether to retransmit. At ρ < 1 a hole whose
projected delivery misses the δ deadline may be shed, within the 1 − ρ budget.
`RepairRequest` exists on the wire for future receiver-driven repair and is not
sent in v8.

**Decoding.** `UnifiedDecoder` is the global sparse-aware closure (ADR-0056,
ADR-0064): known source columns stay payload-only, coded rows reduce only over
their spans, and unit rows deliver per arrival. `RWM_UNIFIED=0` falls back to the
legacy `RlcWindowDecoder`.

## The block pipeline (legacy, still the Bulk/Auto default)

`net/mod.rs` `is_window_mode` routes by
`(hint == Realtime || window_reliable) && backend.is_streaming()`. With the
defaults (`window_reliable = false`, backend RaptorQ) **Bulk and Auto still run
the block pipeline**: `run_block_sender` (`net/block_sender.rs`) assembles
packets into blocks, encodes them with RaptorQ / Reed-Solomon / block RLC
(`fec/*_backend.rs`), interleaves them (`net/interleave.rs`), and repairs with
block ARQ (`net/block_arq.rs`, `net/tasks/arq_sweep.rs`). Realtime, and any hint
with `--window-reliable`, runs the window pipeline above.

This fork is the last architectural mode bit and violates the no-mode-switch
invariant. ADR-0069 records it as legacy and pins the routing with a test.
Flipping the default and deleting block mode wait on the pre-registered re-test
in `docs/status.md` §4.

## Module map (`raptorpath/src`)

| module | role |
|---|---|
| `main.rs` | CLI: `run`, `check`, `status`, `perf`, `setup` |
| `config.rs` | TOML + profile + CLI layering; `resolve` → `PeerConfig` |
| `gates.rs` | the `RWM_*` environment surface, resolved once (`RuntimeGates::resolve`) |
| `perf.rs` | `raptorpath perf`: objects over the real engine through a memory TUN |
| `preflight.rs`, `routing.rs` | environment checks; route and DNS setup and cleanup |
| `net/mod.rs` | orchestration (`run_impl`), window sender, recovery and store laws |
| `net/emit_source.rs`, `net/sender_policy.rs` | the per-symbol emission step and its resolve-once policy |
| `net/receiver.rs`, `net/control_msg.rs`, `net/reorder.rs` | receiver task, control-message dispatch, in-order frontier |
| `net/framing.rs` | length-prefix packet framing and symbol packing |
| `net/block_sender.rs`, `block_arq.rs`, `interleave.rs` | block pipeline (legacy) |
| `net/tasks/` | background tasks: decoder GC, block ARQ sweep, path add/remove, periodic report, control fast path |
| `net/{diag,ackdiag,cpuprof,eta,lat,late,succ,walldiag,rttdump,recv_block}.rs` | measurement instruments (`[DIAG]`, `[ETA]`, `[LAT]`, ...), mostly off by default |
| `fec/` | codecs: `rlc_window.rs`, `unified.rs`, `generation.rs` (opt-in generation coding), block backends, traits |
| `control/` | `estimator.rs` (Beta + BOCD loss estimate), `changepoint.rs`, `gilbert_elliott.rs`, `fec_rate.rs` (r*, rate mix, taper), `anchor.rs` (anchor hygiene) |
| `scheduler/` | per-path state, Copa-lite, placement (`place_costs`), injectable clock |
| `transport/` | `quic.rs` (quinn, per-path connections, substrate CC choice), `protocol.rs` (wire, `PROTOCOL_VERSION = 8`), `bbr_rs.rs` (gated reference BBR) |
| `monitor/` | `SharedStats` and the axum endpoint (`/status`, `/health`, `/paths`) |
| `tun/` | Linux TUN and Windows wintun |

The workspace also holds `gf256/` (SIMD GF(2^8) arithmetic, ADR-0041),
`raptorpath-math/` (the shared laws) and the L0 model (`raptorpath-wasm/`,
`raptorpath-visualizer/`).

## Default stack

`docs/status.md` §1 lists the defaults as the code resolves them: routing per
hint, wire v8 with compact DATA framing, quinn BBR underneath, and the `RWM_*`
gates that are on by default (unified machine and shedding, taper, anchor
hygiene, SACK-clocked release, path-scaled and δ-priced store caps, per-path
recovery clocks, ACK merge). Every other gate is an experiment arm or an
instrument and is off.

## Where to read more

The paper sections (named, since the paper is being renumbered):

- **System and channel model**: the Gilbert-Elliott channel and the (δ, ρ, r) contract.
- **Recovery fundamentals**: FEC versus ARQ, P_lost, the taper.
- **The rate law**: r*, the window loss-mass tail, r(β), b(δ).
- **The sliding-window span machine and multipath**: the unified decoder, the span law, placement.
- **Flow control: the store/δ-cap law**.
- **The recovery decision**: the hole law and the receiver seat.
- **Congestion control and substrate**: Copa-lite, quinn BBR, `RWM_QUIC_CC`.
- **Refuted and superseded designs**: what was tried and removed, and why.

Decisions are indexed in [`docs/adr/README.md`](docs/adr/README.md). Measurement
rules are in [`docs/measurement-discipline.md`](docs/measurement-discipline.md).
