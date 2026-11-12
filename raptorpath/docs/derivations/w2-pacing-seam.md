# W2 pacing-seam audit: what a fused Copa/BBR controller needs to own pacing

Plan W2.2, input to ADR-0068 Addendum 3. Code reading only: no cargo, no VM.

**Pins.** Engine citations are `github/main` at `fdad664e`. `bbr_rs` citations
are `github/bbr48cand` at `e5d7ffbd` (the fixed window law, 1544 lines; main's
`bbr_rs.rs` is the older 1187-line port). quinn citations are quinn-proto
0.11.14, the version in `Cargo.lock`.

## 1. What exists today

### quinn's `Controller` trait: one actuator

- The trait (`congestion.rs:16-84`) receives `on_sent`, `on_ack`
  (with the packet's send `Instant`, quinn's `app_limited` and its RTT
  estimator), `on_end_acks` (in_flight) and `on_congestion_event`. It
  returns only `window()`.
- `pacing_rate` exists only in `ControllerMetrics` (`congestion.rs:96`). Its
  only reader is qlog (`connection/paths.rs:198`).
- The pacer refills at `1.25 · W / srtt` (`connection/pacing.rs:92`). Its
  burst capacity is `W · 2 ms / srtt`, clamped to 10–256 MTU
  (`pacing.rs:129-151`).
- In `poll_transmit` (`connection/mod.rs:606-628`), window-blocked and
  pacing-blocked both set `congestion_blocked`. Then
  `app_limited = buf.is_empty() && !congestion_blocked` (`mod.rs:940`).
  So quinn's own pacer never makes a sample app-limited. A sender that paces
  above quinn does: quinn sees an empty buffer.

### `RWM_QUIC_CC=passthrough`: the mode and the mechanism

- **The mode** installs `PassthroughController` (`transport/quic.rs:119-167`).
  - `window()` reads a per-path `AtomicU64`, floored at 2 MTU (`:146-150`).
  - Every congestion signal is a recorded no-op into `PassthroughCcStats`
    (`:111-115`, `:126-140`).
  - Nothing about pacing changes: quinn still paces at 1.25·W/srtt from the
    engine-written W.
- **The mechanism** is `cc_factory_for_path` (`quic.rs:441-460`). It builds one
  factory per path that shares an `Arc` handle with the engine. The write side
  is `set_cc_window_bytes` (`:466-473`); the read side is
  `cc_passthrough_stats` (`:537`).
- `RouteProbe` (`transport/io_owner.rs:440-500`) is the same pattern as a
  wrapper: a per-path controller decorator holding an `Arc<PathView>`. A
  shared per-path handle between a controller inside quinn and the engine
  already exists twice.

### How Copa-sole drives the wire

- **Feed.** `CopaFeed` (`net/copa_feed.rs`) records seq→(path, send time) at
  source send (`net/emit_source.rs:410-414`). It derives the newly delivered
  seqs from each WindowAck frontier and SACK, and runs the ADR-0061
  send-interval sampler and `on_delivery_signal` per path. It then writes
  `cwnd_symbols × 1250 B` into the passthrough atomic
  (`copa_feed.rs:104-145`).
- **Scope.** The feed exists only in plain in-order window mode
  (`net/mod.rs:915-920`). Generation, OOO and coded-only modes get none.
- **Law.** Copa's update law is wire-clocked: `d_q` comes from quinn's packet
  RTT (`scheduler/copa.rs:1191-1208`). The ±v/δ dither drains each update
  (`:1388-1430`), and a coupling cap holds `cwnd ≤ BDP̂ + 2/δ` (`:1369-1386`).
- **Pacing (`cc_pace`, `RWM_CC_PACE`, default = `copa_wire`, which defaults
  on under passthrough):**
  - It is a token bucket on TUN intake, before placement
    (`net/mod.rs:2658-2697`). The rate is
    `max(goodput EWMA, Σ_i cwnd_i/srtt_i) × 1.1`, clamped to
    `[floor, gen_rate · n_live]`; the burst is 4 ms of rate, clamped to 8–64
    symbols.
  - The coded bucket shares it (`net/sender_phases.rs:387-413`).
  - Its rate is **derived from the window**, not an independent rate. It is
    **aggregate** over paths, not per path.
  - Per-path rate on the wire is still quinn's window-derived pacer.
- **Why this works on shal8.** Copa-sole holds 75–82 Mbit there (ADR-0068
  B1 table, Addendum 2). The intake bucket at 1.1× sits under quinn's 1.25×,
  so arrivals at quinn are already metered, and quinn's ≥ 10-MTU bursts
  rarely form. This is an inference from the rates, not a measured
  attribution.
- **No per-path pacer.** The I/O owner hands every queued datagram to
  `send_datagram` at once (`io_owner.rs:1069-1085`). Anything queued below
  placement waits in quinn's 4 MB datagram buffer, which silently evicts the
  oldest on overflow (`io_owner.rs:129-133`, quinn `datagrams.rs:38-48`).

### `bbr_rs` (bbr48cand): the probe cycle has no actuator

- **The dead rate.** `pacing_gain` (the 1.25/0.75 cycle, `K_PACING_GAIN`
  `bbr_rs.rs:1132`, and Drain's `1/2.885`, `:602`) feeds only:
  - `calculate_pacing_rate` (`:673-696`), which is read only by `metrics()`
    (`:916-922`);
  - the phase-advance tests (`:567-580`).
- **The window.** In ProbeBw, `calculate_cwnd` sets
  `W = min(T(2) + A, W + acked)` for every phase (`:698-712`). The ProbeBw
  window is constant across the gain cycle.
- **Consequence.** Behind the trait, `bbr_rs` probes with a standing
  `T(2) − BDP` excess instead of a 1.25-up / 0.75-down cycle. That is the
  mechanism of Addendum 2's "cannot both probe its filter and avoid the drop
  storm". The paper's §8.4 states it as "the window alone probes".
- **What does work behind the trait:**
  - the burst-robust sampler (`:180-330`, which keys snapshots on
    `on_ack`'s send `Instant`);
  - the own ack clock a_e;
  - the 10 s min filter;
  - the window law.
  All four need packet-level signals that only exist inside quinn.

### ε̂

ε̂ is `PathState.estimator: LossEstimator` (`scheduler/path.rs:10`). Its point
estimate is the TX loss EWMA (`control/estimator.rs:498`), and it is per path,
engine-side. The worst-ε reader is `net/channel_set.rs:37-45`. It measures
**all** loss on the path, congestion loss included. It has no channel-only
component.

## 2. What the four amendment steps need

| step | actuator needed | works behind window-only trait? |
|---|---|---|
| (a) δ-continuous time term in W (plan W1) | W | yes (W1 is designed for it) |
| (b) δ seat: probe amplitude from δ | W (amplitude = gain on T) | amplitude yes; a probe that ends in a drain no (§1: the cycle is unactuated) |
| (b) δ seat: probe dwell from δ | rate | no |
| (c) ε̂-referenced loss discrimination | W (bounded in-flight) + ε̂ input | the response yes; ε̂ must cross engine→controller |
| (d) Copa-style drain replacing ProbeRtt | rate below bŵ for a bounded time | no: a window cut is ProbeRtt itself |

So pacing ownership binds the second half of (b) as well as (c) and (d).
(c)'s hard part is not the actuator. It is a channel-only ε̂ reference: §1
shows the estimator includes congestion loss, so referencing loss to itself
is circular.

## 3. Seam options

**S1: controller in quinn, exports a rate, the engine paces.**
- **How.** `bbr_rs` publishes `pacing_rate` into a per-path shared handle, built
  the way `cc_factory_for_path` builds one. `cc_pace` sums the per-path rates
  instead of `cwnd_i/srtt_i`. ε̂ crosses the other way on the same handle.
- **Defect.** The engine pacing below the window makes quinn's buffer empty,
  so `app_limited` is true (`mod.rs:940`). `bbr_rs` then admits samples
  raise-only, and Startup cannot exit while app-limited. The model loses its
  decay. The fix is an engine→controller backlog bit that overrides quinn's
  flag: a second channel and a second notion of app-limited.
- **Multipath.** Pacing stays aggregate at intake. A true per-path pacer below
  placement queues into the evicting datagram buffer (§1). It must
  backpressure placement.
- **Size.** About 400–600 lines with tests: handle, publish and read,
  backlog bit, `cc_pace` source swap, per-path backpressure.

**S2: controller in the engine, under the passthrough mode (Copa-sole's
pattern).**
- **How.** Port the `bbr_rs` model into the scheduler, fed by `CopaFeed`.
  Pacing is `cc_pace`.
- **Losses.** The packet clock (send `Instant`s, quinn's in_flight and loss
  events) is gone. The feed is plain in-order only. Two estimators, the
  ADR-0061 sampler and `bbr_rs`'s, have to merge. This is "build a second
  controller", which the direction rejects.
- **Size.** 800+ lines.

**S3: make quinn's pacer honour the controller's rate (patched quinn-proto).**
- **How.** A `[patch.crates-io]` quinn-proto 0.11.14 fork:
  - add `fn pacing_rate(&self) -> Option<u64> { None }` to `Controller`, so
    stock Bbr, Cubic, NewReno and passthrough are unchanged;
  - have `Pacer::delay` refill at that rate, with capacity = rate × 2 ms, when
    it is `Some` (`pacing.rs:48-100`, the call at `connection/mod.rs:617`).
- **What it fixes.** `bbr_rs` overrides `pacing_rate`, and its existing gain
  cycle and Drain become live with no engine change.
  - Pacing-blocked stays `congestion_blocked`, so `app_limited` stays honest.
  - Pacing is per path by construction (one connection per path).
  - cc_sim drives real quinn-proto, including the pacer (`cc_sim.rs:7`), so
    the patched pacer is under test immediately.
- **Size.** About 40–60 lines in the fork, a one-method override in `bbr_rs`,
  and pacer tests.
- **Cost.**
  - A maintained fork of a dependency (an ADR of its own) and drift risk on
    quinn upgrades.
  - Queueing below placement is today's quinn-default situation, so the
    store cap still governs intake.
- **History.** Status §48 already names "quinn honours `pacing_rate`" as the
  re-ask condition for `bbr_rs`.

**Recommendation.**
- **S3 if the user accepts a quinn-proto fork.** It is the only seam where one
  controller owns window and rate with honest app-limited semantics and
  per-path pacing.
- **Otherwise S1 with the backlog bit.**
- **Not S2.** The passthrough *mode* cannot host `bbr_rs`: it replaces quinn's
  controller with a no-op. What Addendum 2 calls "the passthrough seam" is
  usable only as its *mechanism*, the per-path shared handle, which S1 and
  (for ε̂) S3 both use.

## 4. Risks common to every seam

- **Mutual masking** (Addendum 2). An honest estimator strips the standing
  queue that today hides store-dwell recovery latency. The GE cells lose
  4–14 % until recovery latency is queue-independent.
- **Burst floor.** quinn's 10-MTU minimum burst exceeds the 8-packet shal8
  buffer. S3's capacity must derive from the rate, not keep the 10-MTU
  floor. S1 relies on intake metering, as Copa-sole does today.
- **NO-MODE-SWITCH.** `bbr_rs`'s Startup/Drain/ProbeBw/ProbeRtt are BBR
  states, not hint modes. Every δ-derived quantity added in (a)–(d) must be
  one formula continuous in δ, with no δ, ρ or hint threshold. (d) removes a
  state; it must not add one.
- **ε̂ reference.** (c) needs a channel-loss reference that excludes
  congestion loss: for example loss sampled while `d_q ≈ 0`, or the BOCD
  regime. No such estimator exists today (§1).
