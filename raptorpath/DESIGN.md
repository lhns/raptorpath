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
| δ | latency price: how much delay the flow will trade for throughput | `net/store_cap.rs` `delta_price` (hint → δ via `hint_delta_price`, `scheduler/copa.rs`; `RWM_DELTA` sets a number); `delta_budget_b_of` → `raptorpath_math::span_horizon_b` |
| ρ | retention contract: the fraction of data that must eventually arrive | ρ = 1 retain-until-acked (`--window-reliable`); ρ < 1 EVICT retention with δ-honest shedding (`shed_allowed`, `shed_deadline_us`, `shed_recv_budget_ok` in `net/shed.rs`) |
| r | proactive repair rate | `control/fec_rate.rs`: r* from the BOCD loss quantile and the window loss-mass tail; the rate mix r(β) = (1−β)·r_anchor + β·r_late-is-fine with β = `bulkness_of_delta(δ)`; `TaperBudget` spends it on the wire |

The shared closed forms (`compute_r_star`, `r_star_mass`, `span_horizon_b`,
`bulkness_of_delta`, `zeta_of_delta`, `p_lost`, ...) and the shared estimators
(BOCD changepoint, Gilbert-Elliott, the normal helpers) live in the
`raptorpath-math` crate, which the engine and the visualizer
(`raptorpath-wasm`, `raptorpath-visualizer/`) both call, so the model and the
engine evaluate the same law.

## Data path (window pipeline)

```
TUN / perf object
  └─ run_window_sender (net/mod.rs; loop phases in net/sender_phases.rs)
       └─ emit_source (net/emit_source.rs)         one packet → one source symbol
            ├─ encoder intake (fec/rlc_window.rs)   sliding-window RLC over GF(256)
            ├─ Scheduler::place_symbol → place_costs (scheduler/place.rs)
            ├─ retention store + store-cap gate     (flow control, net/store_cap.rs)
            └─ proactive repair (TaperBudget, control/fec_rate.rs)
  └─ QuicTransport (transport/quic.rs)              QUIC DATAGRAM per path
                                                    wire format: transport/protocol.rs
peer ─ run_receiver (net/receiver.rs)
       ├─ UnifiedDecoder (fec/unified.rs)           one decoder for every RLC wire
       ├─ ReorderBuffer (net/reorder.rs)            in-order frontier / hold-at-hole
       └─ WindowAck (SACK) back to the sender
sender ─ on_window_ack (net/control_msg.rs)         store release (net/sack.rs), hole detection, retransmit
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
engine-side state (`PathState`, `scheduler/path.rs`, with a Copa-lite
`CopaState`, `scheduler/copa.rs`) whose samples feed
the anchors. The congestion controller under each QUIC connection is quinn BBR
by default (`RWM_QUIC_CC`, ADR-0054).

**Flow control.** Sent symbols live in a retention store until the cumulative
ACK passes them. SACKed symbols release their store slot at once but keep their
payload for recovery (ADR-0060). The store cap is a derived law over path count,
per-path pipe and the δ price (`path_scaled_store_cap`, `pooled_store_cap`,
`pool_value_multiplier` and the honest-cap terms in `net/store_cap.rs`). Paper
§6 derives it.

**Recovery.** The receiver acknowledges with SACK ranges (`WindowAck`). The
sender turns gaps into holes (`sack_to_gaps`, `net/sack.rs`) and decides per
hole, on that flight's own path clock (RFC 9002 time and packet thresholds
generalized per path, `mp_hole_ripe` in `net/recovery_laws.rs`, ADR-0059),
whether to retransmit. The recovery clocks (tail sweep, refresh, cooldown)
pool their SRTT over the live paths (`recovery_clock_paths`). At ρ < 1 a hole whose
projected delivery misses the δ deadline may be shed, within the 1 − ρ budget.
`RepairRequest` exists on the v8 wire for receiver-driven repair; it is sent
only under the off-by-default `RWM_RECV_REQUEST_LAW` arm, and otherwise an
arriving one is counted and dropped.

**Decoding.** `UnifiedDecoder` is the global sparse-aware closure (ADR-0056,
ADR-0064): known source columns stay payload-only, coded rows reduce only over
their spans, and unit rows deliver per arrival. `RWM_UNIFIED=0` falls back to the
legacy `RlcWindowDecoder`.

## One pipeline

Every hint runs the window pipeline above. The block pipeline (RaptorQ /
Reed-Solomon / block RLC, interleaving, block ARQ) was the last architectural
mode bit; ADR-0069 deleted it in `dacfd7c` after the Stage-3 re-test
(`docs/status.md` §5, `WINDOW-NOT-WORSE`). `net::pipeline_backend` accepts only
the streaming RLC codec; a block-only setting is a startup error naming the
ADR. ρ (`window_reliable`) is the named point's preset — retain-until-acked
at Bulk/Auto, EVICT at Realtime — and composes with δ; it selects no pipeline.

## Module map (`raptorpath/src`)

| module | role |
|---|---|
| `main.rs` | CLI: `run`, `check`, `status`, `perf`, `setup` (links the `raptorpath` library) |
| `config.rs` | TOML + profile + CLI layering; `resolve` → `PeerConfig` |
| `gates.rs`, `gates/scheduler_gates.rs` | the `RWM_*` environment surface, resolved once into a `OnceLock<RuntimeGates>` read everywhere through `gates::get()`; one strict boolean dialect (`config::parse_bool`), an unrecognised value is a startup error |
| `perf.rs` | `raptorpath perf`: objects over the real engine through a memory TUN |
| `preflight.rs`, `routing.rs` | environment checks; route and DNS setup and cleanup |
| `net/mod.rs` | orchestration (`run_impl`), pipeline routing (`is_window_mode`), the window sender loop (`run_window_sender`), the monotonic engine clock `now_us` |
| `net/sender_phases.rs` | the window sender's loop phases: store-cap refresh, generation emission, gap serving, ack advance |
| `net/recovery_laws.rs` | per-path loss detection (`mp_time_threshold_split`, `mp_hole_ripe`), refresh and tail-sweep clocks, `recovery_clock_paths` |
| `net/holddown.rs`, `net/recovery_clock.rs` | the hold-down arm (`RWM_HOLDDOWN_Q`, off) and its quantile window law; the `[HOLD]` and `[QCLK]` gauges |
| `net/store_cap.rs` | flow control: path-scaled, pooled, honest and three-term caps, the δ-cap setpoint, `delta_price`, `contract_stall_s` |
| `net/shed.rs` | δ-honest shedding (sender deadline and budget, receiver hold) and the completion feed |
| `net/sack.rs` | SACK gap inversion, per-path outstanding accounts, SACK-clocked release, window-ack emission |
| `net/copa_feed.rs` | the plain-mode Copa delivery feed (opt-in) |
| `net/report.rs` | recovery-plane report lines: `[RACK]`, `[RFA]`, `[FCAUSE]`, `[REQS]` |
| `net/emit_source.rs`, `net/sender_policy.rs` | the per-symbol emission step and its resolve-once policy |
| `net/receiver.rs`, `net/control_msg.rs`, `net/reorder.rs` | receiver task, control-message dispatch, in-order frontier |
| `net/framing.rs` | length-prefix packet framing and symbol packing |
| `net/tasks/` | background tasks: path add/remove, periodic report, control fast path |
| `net/{diag,ackdiag,cpuprof,eta,lat,late,succ,walldiag,rttdump,recv_block}.rs` | measurement instruments (`[DIAG]`, `[ETA]`, `[LAT]`, ...), mostly off by default |
| `fec/` | codecs: `rlc_window.rs`, `unified.rs`, `generation.rs` (opt-in generation coding; `generation/reference.rs` is the test oracle), the window traits and the wire `WireSymbol`/`FecBackend` types |
| `control/` | `estimator.rs` (Beta + BOCD loss estimate), `fec_rate.rs` (r*, rate mix, taper), `anchor.rs` (anchor hygiene); `changepoint`, `gilbert_elliott` and `p_lost` are re-exported from `raptorpath-math` |
| `scheduler/` | `mod.rs` (`Scheduler`, the `live_paths`/`active_paths` sets, weights), `path.rs` (`PathState`), `copa.rs` (Copa-lite, `CopaState`), `place.rs` (placement, `place_costs`), `clock.rs` (injectable clock) |
| `transport/` | `quic.rs` (quinn, per-path connections, substrate CC choice), `protocol.rs` (wire, `PROTOCOL_VERSION = 8`), `bbr_rs.rs` (gated reference BBR), `l0_netem.rs` (in-process netem shim for loopback tests) |
| `monitor/` | `SharedStats` and the axum endpoint (`/status`, `/health`, `/paths`); `quantile.rs` (the one nearest-rank quantile every gauge uses) |
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

The paper sections:

- **§2 System and channel model**: the Gilbert-Elliott channel and the (δ, ρ, r) contract.
- **§3 Recovery fundamentals**: FEC versus ARQ, P_lost, the taper.
- **§4 The rate law**: r*, the window loss-mass tail, r(β), b(δ).
- **§5 The span machine and multipath**: the unified decoder, the span law, placement.
- **§6 Flow control**: the pooled store cap and the δ-cap.
- **§7 The recovery decision**: the hole law and the receiver seat.
- **§8 Congestion control and substrate**: Copa-lite, quinn BBR, `RWM_QUIC_CC`.
- **§10 Refuted and superseded designs**: what was tried and removed, and why.

Decisions are indexed in [`docs/adr/README.md`](docs/adr/README.md). Measurement
rules are in [`docs/measurement-discipline.md`](docs/measurement-discipline.md).
