# W3.2 — one per-leg state that every law reads (design, no code)

Plan W3.2 / W3.4; status §44, §45 (Stage 3.1 and 3.2, and the §45 addendum's
correction). Every file:line is at `github/main` `fdad664e`, paths relative
to `raptorpath/src/`. Design only: no engine change, no VM.

**Summary.**
- *Duplicates.* The engine keeps two app-echo smoothed RTTs on one sample
  stream (Copa `srtt`, `estimator.ewma_rtt`). It keeps one RTprop filter,
  read under two names (`min_rtt()`, plus `est.rtt()` bound to a variable
  called `rtprop`). It runs two delivery-rate samplers into one max filter.
  It holds about six dispersion statistics, five of them read only by
  gauges.
- *Copa's cwnd.* It has four default-on readers and six default-off ones.
  The four default-on readers can each read one δ-continuous per-leg window
  instead:

  ```text
     Ŵ_i(δ) = (1 + q(δ)) · b̂_i · m̂_i
  ```

  This is the store law's own per-leg term, so the store cap is Σ Ŵ_i.
  `queue_target_mult` then has no default-path reader. Its continuous
  replacement is q(δ) itself, so no third map is needed.
- *Migration.* Phase A relocates the estimators. Its steps are
  bit-identical or carry a stated bound. Phase B moves one consumer per
  guard step.
- *A premise gap.* Main has no srtt-equality test, and the two copies are
  not bit-identical (§1.2). Step A1 writes that test.

## 1. Inventory

### 1.1 Feed sites (the only writers)

App-echo RTT samples are written at three handlers. Each writes the loss
estimator and Copa from the same sample, in the same order, under the same
guard:

| site | estimator | Copa | guard |
|---|---|---|---|
| per-batch `Ack` | `net/control_msg.rs:298` | `:308` | `!gap_q` |
| `PathReport` | `:435` | `:442` | `!mstar_anchor && !gap_q` (`RWM_MSTAR_ANCHOR` on by default, so this site is off) |
| `WindowAck` | `:572` | `:582` | `echo_send_timestamp_us > 0 && !gap_q` |

- **Copa's input switches under `RWM_COPA_WIRE`.** Copa takes
  `wire.rtt.unwrap_or(echo)`, read at `:303`, `:437` and `:577`. The
  estimator always takes the echo.
- **The wire RTT ŵ** is mirrored for every leg at each of the three
  handlers (`control_msg.rs:236`, `:419`, `:510`) through
  `Scheduler::mirror_wire_rtts` (`scheduler/mod.rs:234`) from quinn's
  `PathView.rtt_us` (`transport/quic.rs:491`).
- **Delivery samples** reach `copa.record_delivery` (ack-interval Δt,
  `scheduler/copa.rs:746`) by this chain: `sched.ack` (`scheduler/mod.rs:400`)
  → `PathState::on_ack` (`scheduler/path.rs:484`), called from
  `control_msg.rs:276` and `:565`. Under `RWM_COPA_FEED` (default off) the
  send-interval sampler `rs_on_sent` / `rs_on_delivered` (`copa.rs:815`,
  `:850`) feeds the same max filter instead.
- **Throughput:** `estimator.record_throughput` (`net/tasks/report.rs:155`)
  takes the report task's local send rate.

### 1.2 Per-leg estimators, grouped by quantity

**D** marks a duplicate: the same quantity in two estimators, or under two
names.

| # | quantity | estimator (state, update) | readers | dup |
|---|---|---|---|---|
| R1 | smoothed echo RTT s | Copa `srtt` (`copa.rs:408`; EWMA 1/8 in `Duration::mul_f64`, `copa.rs:946–949`; seeds from the first sample; cleared by `copa.reset()` on revival, `scheduler/mod.rs:441`) → `PathState::srtt()` (`path.rs:690`, falls back to R2 while `None`) | placement s_i (`place.rs:332–337`); in-flight expiry horizon (`path.rs:1174`, `:1184`); D1 per-leg clock (`net/sender_phases.rs:700`); TX view to the receiver's hole refresh (`scheduler/mod.rs:510` → `net/receiver.rs:747`); `channel_max_rtprop_s` fallback and `channel_max_srtt_us` (`net/channel_set.rs:91`, `:106`); Copa's cadence and targets; off arms: cc_pace (`net/mod.rs:2678`), THREE_TERM (`sender_phases.rs:152`) | **D with R2** |
| R2 | smoothed echo RTT s | `LossEstimator.ewma_rtt` (`control/estimator.rs:465–481`; α = 0.125 at `:282`; f64-seconds blend then `from_secs_f64`; seeds from the first sample only under `mstar_anchor` (`:284`), else blends from 50 ms; never reset) → `est.rtt()` | recovery clocks: pooled max (`net/mod.rs:2727`, `net/recovery_laws.rs:289`, `sender_phases.rs:701`, `:711`); rate law (`control/fec_rate.rs:466`, `:533`); A*, the span deadline and the shed deadline (`net/emit_source.rs:562–584`, bound to a variable named `rtprop` at `:567`); P_lost (`channel_set.rs:68`); the ρ<1 path cost `effective_delivery_time` (`path.rs:410`); `PathReport.avg_rtt_us` (`report.rs:205`) | **D with R1** |
| R3 | wire RTT ŵ | quinn's packet-timed SRTT, mirrored (`path.rs:34`, `:679`) | placement base (`place.rs:363`, `:382`); `[DIAG]` | distinct clock, not a duplicate |
| R4 | RTprop m̂ | Copa 10 s windowed min (`copa.rs:399–403`; monotone deque `:1000–1007`; window `copa.rs:641`; evicted in `expire_old_samples` `:1489`, called only from `record_rtt` and `record_delivery`) → `min_rtt()` (`path.rs:1070`) | store pool through `bdp_anchor` (`copa.rs:1458` → `channel_set.rs:125`); the rate sampler's interval floor (`copa.rs:882`); `[ETA]` (`control_msg.rs:595`); NACK congestion (`channel_set.rs:79`); `channel_max_rtprop_s` (`:89`); TX view (`mod.rs:511`); Copa's queue floor | — |
| R5 | "RTprop" | R2 read as RTprop at `emit_source.rs:567` (§44 item 3, rename pending) | A* and D(δ) | **D: a misnamed R2** |
| R6 | quinn lifetime min | `RttEstimator::min` | nothing (status §42 (b)) | — |
| J1 | consecutive-difference jitter | Copa `jitter_est` and `win_jitter_est` (`copa.rs:417`, `:509`; the second is fed only inside Copa's per-SRTT update) → `rtt_jitter_us` (`path.rs:1053`) | derived sweep / hole refresh (Group C, off: `net/holddown.rs:549`); `[QCLK]`; Copa's backoff headroom | — |
| J2 | σ (second moment) | Copa `rtt_var_sq` (`copa.rs:432`, β = ¼) → `rtt_sigma_us` (`path.rs:718`) | `[QCLK]` and `[DIAG]` only | — |
| J3 | dispersion candidates | `rtt_mdev`; `rtt_win` (256-sample FIFO, `copa.rs:477`); `rtt_tlag` (decimated at RTprop/8, `:491`) → `qsp`, `msd`, `tlag`, `rvar` (`path.rs:798–1038`) | `[DIAG]` only (`net/diag.rs:662–674`) | unread hot-path work (§45 Stage 1) |
| J4 | interarrival jitter | `estimator.jitter_us` (RFC 3550, receiver side, mirrored by `sync_rx`) | A*'s Δ (`emit_source.rs:574`); `PathReport` | different direction and clock |
| S | app-echo loop spread S = p50 − min | **no engine estimator.** A grep of `src` for `LOOPDIAG`, `STORE_LAW` and `ORACLE_BRAKE` finds nothing at `fdad664e`. The only 250 ms literal (`diag.rs:469`) is the `[DIAG]` print cadence, not a window minimum, so §44's 250 ms Tmin is not on main. `qsp` is P90 − P50, not S | — | gap (§3) |
| B1 | delivery rate b̂ | Copa `max_bw` over `bw_samples` / `bw_mono` (`copa.rs:384–405`), ack-interval sampler (`:746`), refreshed only in `record_delivery`; gated at `ANCHOR_MIN_SAMPLES` = 8 (`effective_btlbw` `:1472`). **Units on the default feed:** wire symbols per second, repairs included. The count is the receiver's per-leg `PathBatchTracker.total_received` delta (`net/mod.rs:3665`, `:3738`; `control_msg.rs:276`, `:565`), the same unit as I_i. Only the B2 sampler counts source symbols | store cap Σ b̂·m̂ (`channel_set.rs:125–129`); cwnd anchor floor and recovery pull (`copa.rs:1483`, `:1327–1333`); `btlbw_sym_per_s` (THREE_TERM, `[DIAG]`); INFL_CAP per-leg cap (`net/mod.rs:2346`, off) | — |
| B2 | delivery rate | Copa send-interval rate sampler into the **same** filter (`copa.rs:815–930`, `RWM_COPA_FEED` only) | as B1 | **D feed** (gated) |
| B3 | send rate | `estimator.throughput` (EWMA of the report task's local send rate) | rate law t_sym (`fec_rate.rs:465`); A* when the A* anchor is off | different quantity (offered, not delivered) |
| B4 | implied rate W/s | Copa cwnd / s | placement queue term (`path.rs:441–445`); cc_pace (off) | **D with B1** (the anchor floor ties W to b̂·m̂) |
| K | echo ratio K_i | `EchoRatioMin` (`net/store_cap.rs:208`) in `percap_k` | `three_term_terms` only (`store_cap.rs:399–411`, THREE_TERM, Group B, approved for deletion); `[DIAG]` (`diag.rs:615`) | **leaves with Group B; not moved** |

§44 counts six delivery-rate estimators. Two of them are aggregate, not per
leg, so they stay out of `LegState`: `SendRateAnchor` (`emit_source.rs:146`,
`control/anchor.rs:185`) and `gen_rate_ewma` (`sender_phases.rs:370`).

Notes on the duplicates:
- **R1 and R2 are not bit-identical.** They differ in four ways:
  - *Arithmetic.* R1 rounds each of two `mul_f64` terms to ns. R2 rounds
    once. With an 0.875 decay the gap stays bounded at about 8 ns per
    rounding unit (to be bounded by the A1 test, not assumed).
  - *Revival.* `touch_path_at` resets Copa and not the estimator, so R1
    reseeds from the first post-revival sample while R2 blends from its
    stale pre-death value. They differ by milliseconds and the gap decays
    as 0.875ⁿ.
  - *Seed.* With `RWM_MSTAR_ANCHOR=0`, R2 blends from 50 ms while R1 seeds
    from the first sample.
  - *Feed.* Under `RWM_COPA_WIRE`, R1 is fed wire samples.

  The D1 clock `max(SRTT_p, EWMA_p)` (`recovery_laws.rs:159`, fed
  `(srtt(), est.rtt())` at `sender_phases.rs:700`) is therefore a no-op
  only up to these divergences.
- **A latent inconsistency in R4.** `record_delivery` evicts the RTT deque
  (`expire_old_samples`) but leaves the cached `min_rtt` alone until the
  next `record_rtt`. Phase A keeps that bit-identical and flags it.
- **Idle behaviour today.** R1, R2, R4, J1–J3 and B1 freeze when a leg
  gets no echo and no delivery: eviction runs only on a feed, so the
  windows hold their last envelope. R3 refreshes at idle, 2–3 samples per
  2 s (status §46). A revival clears R1, R4, J1–J3 and B1 through
  `copa.reset()`.

### 1.3 Readers of Copa's cwnd

| reader | site | default | fate in this design |
|---|---|---|---|
| placement queue term (I_i/W_i)·s_i, and the `[ETA]` stamp through `place_e` | `path.rs:441–445`, `place.rs:380–384` | **on** | Ŵ_i (step B4) |
| `spare_capacity` (W − I)/I: the min over active legs clamps repair emission | `path.rs:467`, `scheduler/mod.rs:606`, `emit_source.rs:526`; `place.rs:182` (XPATH_REPAIR, Group A) | **on** | Ŵ_i (B2) |
| `available() > 0` filter: ρ<1 picks and the Realtime duplicate | `scheduler/mod.rs:531`, `:552`, `:567`, `:593` via `net/mod.rs:1380–1393`; callers `emit_source.rs:313`, `:464`, `:823`, `sender_phases.rs:983`, `:1139`, all on the `!reliable` arm; `active_paths()` `:267` feeds only the `[SF]` gauge and `[DIAG]` | **on** at ρ<1 | Ŵ_i (B3, needs the Realtime cells) |
| in-flight leak backstop `I > W ⇒ I −= I/4` | `net/tasks/report.rs:183` | **on** | Ŵ_i, or delete (B1) |
| `schedule()` | `scheduler/mod.rs:304` | dead (no caller outside tests) | delete (§45 batch 1) |
| pass-through quinn window | `control_msg.rs:373`, `:668`; `net/copa_feed.rs:134` | off (`RWM_QUIC_CC=passthrough`) | stays on Copa |
| Σcwnd store law | `sender_phases.rs:105` | off (`RWM_COPA_FEED`) | stays on Copa |
| LATE_BRAKE per-leg cap | `net/mod.rs:2342` | off | stays (no verdict) |
| cc_pace Σ cwnd/s | `net/mod.rs:2680` | off (`= RWM_COPA_WIRE`) | stays on Copa |
| PLACE_HOL `r_ref` | `place.rs:485` | off | removed on `simplify-3` (`57955ea4`) |
| monitoring `ps.cwnd`, `[DIAG]` | `control_msg.rs:363`, `diag.rs:541` | gauge | report Ŵ_i beside cwnd |

After B1–B4, Copa's dynamics have no default-path reader. They are the
ramp, backoff, anchor pull, queue target, wire and compete modes, called
from `on_delivery_signal` and `on_loss` (`path.rs:496`, `:583`). Their only
remaining readers are the off arms, which go to the user under ADR-0073
(§4, B5).

## 2. The struct

`LegState` replaces the estimator fields of `CopaState` and the RTT half of
`LossEstimator`. `PathState` owns it. Loss estimation (ε̂, GE, BOCD) and
the in-flight accounting (I_i and its charge log) stay where they are. Both
are counted elsewhere (§44: three loss estimators), and I_i is accounting,
not an estimator.

```text
LegState {
  // clock E: app echo of our own symbols (dwell-inclusive), fed at the three handlers of §1.1
  s:        Option<Duration>   // smoothed echo RTT, EWMA 1/8; ONE copy (R1 = R2)
  m:        MinFilter          // RTprop m̂: 10 s windowed min (R4), evicted on feed only
  ring:     TimedRing<u32 µs>  // raw echo samples, time-decimated at m̂/8, 256 entries (today's rtt_tlag)
  var_sq:   (f64, n)           // σ² EWMA (J2), gauge
  jit:      f64                // consecutive-difference jitter (J1, raw half)
  e_at:     Option<Instant>    // last echo sample
  // clock W: quinn's packet-timed SRTT (dwell-free; refreshes at idle)
  w:        Option<Duration>   // ŵ (R3)
  w_at:     Option<Instant>
  // rate
  b:        MaxFilter          // b̂: 10 s windowed max of delivery samples (B1), ack-interval feed today
  rs:       RateSampler        // the send-interval sampler (B2), with its app-limited marks
  b_at:     Option<Instant>
}
```

Units: durations in ns `Duration`. Rates and windows are in **wire
symbols** (source and repair), per second and in symbols respectively. That
is b̂'s unit on the default feed (§1.2 B1), and it matches I_i, so Ŵ_i − I_i
is unit-consistent.
- Today's store cap compares this wire-unit Σ b̂·m̂ against an outstanding
  count in source units, a factor of (1 + r) apart. That mismatch already
  ships and is noted for W3.3.
- Switching L to the source-counting send-interval sampler (the §3 L rule)
  would move Ŵ into source units against I_i's coded units, a (1 + r)
  mismatch exactly where B2 and B3 operate. Whichever sampler feeds L, Ŵ's
  readout converts to I_i's unit.

**Readouts.** Each is a pure function of the state and the dial. None
holds state of its own.

| readout | definition | readers after migration |
|---|---|---|
| `srtt()` / `srtt_measured()` | `s` (seed ⇒ `None` for `measured`) | placement s_i, expiry, recovery clocks, rate law, A*/D(δ) (renamed from `rtprop`), PathReport, TX view |
| `rtprop()` | `m.min()` | store law, rate sampler floor, `[ETA]`, NACK congestion, TX view |
| `wire()` | `w` | placement base ŵ |
| `rate()` (L) | `b.max()`, gated at ANCHOR_MIN_SAMPLES, plus the idle rule (§3) | store law, Ŵ |
| `bdp()` | `rate() · rtprop()` | store pool |
| `window(δ)` = Ŵ_i | `(1 + q(δ)) · bdp()`, with the cold chain below | the four cwnd consumers; the store cap = clamp(Σ Ŵ_i) (today `pooled_store_cap` with `sum_cap` and `delta_cap` on) |
| `spread()` (S) | `p50(ring) − min(ring)` over the ring's own span (§3) | store law time term (W3.3); W1 τ candidate |
| `tau_lag()` | today's `tlag` σ̂_Δ(τ = m̂) on the ring | W1 τ candidate; `[DIAG]` |
| `sigma()`, `jitter()` | J2, J1 | `[QCLK]`, `[DIAG]` |

**Cold chains: two, by consumer.** Both are availability fallbacks, not
modes (the `RWM_COLD_PLACE` pattern). Optimism is safe for a price and
unsafe for a cap. An optimistic price gets a leg sampled. An optimistic cap
lets a cold or slow leg take on the fast leg's window.

```text
price (B4, placement):          Ŵᵖ_i = window_i(δ)  ?:  max_{j warm} window_j(δ)  ?:  STORE_CAP_FLOOR
caps (B1 backstop, B2 spare,    Ŵᶜ_i = window_i(δ)  ?:  STORE_CAP_FLOOR
      B3 ρ<1 filter):
```

- STORE_CAP_FLOOR = 10 is the derived floor (`net/sender_policy.rs:67–79`:
  the max of ANCHOR_MIN_SAMPLES·1 and RFC 6928's IW 10). It equals Copa's
  INITIAL_CWND, so a cold leg's cap starts where Copa starts.
- Unlike Copa, it does not ramp ×1.5 per SRTT before the anchor warms. The
  anchor warms after 8 delivery samples, so this is a warm-up-only
  difference. A4's gauge measures how long it lasts.
- The price chain's middle link is hygiene rule 1: an unmeasured leg takes
  the best measured leg's value, so it gets sampled.
- `window_i` is "warm" only when L_i is warm under §3's rule. So under that
  rule an app-limited-only leg falls to the floor for caps and to the best
  leg for the price.

**Why Ŵ is the store law's term.**
- The shipped pool is already (1 + q(δ))·Σ b̂_i·m̂_i
  (`store_cap.rs:76–200`).
- Copa's cwnd is floored at 0.85·b̂·m̂ (`copa.rs:301`, `:1483`). Under the
  ack-interval over-read, that floor is what binds, so cwnd and Ŵ already
  share their input. Expected ratio where the floor binds:
  Ŵ/cwnd = (1+q)/0.85 = 1.24 (Realtime), 1.25 (Auto), 1.29 (Bulk).
- One window means the store law, repair headroom, the ρ<1 filter, the
  backstop and the placement drain all read one quantity with one δ seat.
- When W3.3 changes the store term (L̂, a time term, δ's seat), Ŵ follows
  by definition. Ŵ_i is defined as "the store law's per-leg term", not as
  this formula.

**`queue_target_mult` (W3.4).** It is a three-arm table, 1.08 / 1.125 /
1.25 (`copa.rs:314–320`), read only inside Copa's app-echo queue test
(`copa.rs:1227–1250`). No existing map reproduces it:

| form | Realtime | Auto | Bulk | change in headroom (mult − 1) against the table |
|---|---|---|---|---|
| table (today) | 1.080 | 1.125 | 1.250 | — |
| 1 + q(δ) = 1 + (b+1)/30 | 1.050 | 1.067 | 1.100 | −38 %, −47 %, −60 % |
| 1 + b(δ)/8 | 1.0625 | 1.125 | 1.250 | −22 %, 0, 0 |
| 1 + 0.08 + 0.17·β(δ) | 1.080 | 1.080 | 1.250 | 0, −36 %, 0 (flat below Auto) |

Recommendation:
- Once B1–B4 land, the table has no default-path reader, and the permitted
  standing-queue fraction is q(δ) by construction (Ŵ = (1+q)·BDP). That
  discharges the NO-MODE-SWITCH debt without a new map.
- If the user keeps a Copa arm (passthrough, feed or wire), it should read
  `1 + q(δ)`, so the arm and the store law share one map.
- `1 + b/8` would carry the fitted 1/8, a constant without provenance.
- **Open question (user):** whether a kept arm takes the 1 + q change.
  Whichever form is chosen gets the law-continuity test pattern: absolute
  values at the three presets, plus continuity and monotonicity at ±2 %
  around each (the `delta_budget_b_is_the_dial_not_a_mode` pattern).

## 3. The idle rule, field by field

Paper §5.7's constraint: when I_i = 0, a law may read only state that
refreshes without our symbols, or state that is never pessimistic.
Load-dependent delay enters only multiplied by a live load counter.

| field | at idle today | rule |
|---|---|---|
| ŵ | refreshes (quinn's 2 s report and ping) | none needed; placement's base |
| s | frozen at the dwell that idled the leg (pessimistic) | read only times I_i (the placement queue term: already so) or by clocks where pessimism costs time, not exclusion (recovery clocks). **Hazard to note, not fix here:** the recovery clocks pool `max_i s_i`, so a leg frozen at a spike inflates every leg's clock until it carries again |
| m̂ | frozen low envelope (never pessimistic for a price) | keep: evict on feed only (bit-identical) |
| L = b̂ | frozen max; pessimistic when the window held only app-limited samples, because the ack-interval sampler has no app-limited mark | **proposed:** `L_i = max over non-app-limited samples ?: unmeasured`. A leg whose window holds only app-limited samples counts as unmeasured, and the consumer's own cold chain then applies (§2): the best leg's value for the price, STORE_CAP_FLOOR for caps. The only inputs are BBR's app-limited rule and hygiene rule 1, with no κ/ν and no decay constant. Today the app-limited mark exists only on the send-interval sampler (`RWM_COPA_FEED`), so the rule needs that sampler on the default path, which changes b̂ for the store law (a W3.3-class change). |
| S | no estimator; paper §5.7's "keep S when idle" keeps the dwell spike (127–140 ms on the locked rows, Table 5.7a) | placement never reads S. For the store law, **proposed:** S_i is read over the ring entries within one m̂-window horizon (10 s, `copa.rs:641`, an existing constant). An empty window means unmeasured, so S takes `min_j S_j ?: 0`, the optimistic chain. S must not keep the last window. |

**Open question (W3.3).** The ring's span is 32·m̂ (decimation m̂/8 × 256).
That is not m̂'s 10 s. A spread whose minimum sits on a different horizon
from m̂ is the §44-erratum brake bias (b₀). Which pair the time term reads
is W3.3's call:
- `p50(ring) − min(ring)`, both on the ring's horizon, cancels the bias;
- `p50(ring) − m̂` keeps it.

LegState exposes `min(ring)`, `p50(ring)` and m̂ separately.

**Revival.** `copa.reset()` clears m̂, b̂ and s while the loss estimator is
kept. Once s is a single field, A1 picks one semantics: reseed on revival
(today's R1). It is pre-registered as a behaviour change for the R2 readers
after a revival only.

## 4. Migration

There are two phases. In Phase A, Copa's controller still runs and reads
the moved fields. Every step there is bit-identical or bounded, and each
has an equality test that is red if the step changes a value. In Phase B
each step moves one consumer behind one §41c guard run, with an absolute
red-first test and a routing test (`goal-gate.md` rule 1: prove the
mechanism under test executes).

**Phase A — relocation (suites per step; one guard at the end, expected
WITHIN)**

- **A0. Pin the trace.** `leg_state_trace_pin` (scheduler tests): a fixed
  script through `PathState`.
  - Inputs: echo samples, delivery batches, wire mirrors, a 12 s idle gap,
    a dead-path timeout and a revival.
  - At every step it records `srtt()`, `est.rtt()`, `min_rtt()`,
    `btlbw_sym_per_s()`, `copa_bdp_anchor()`, `rtt_sigma_us()`,
    `rtt_jitter_us()`, `wire_rtt()` and `cwnd`, against a table captured at
    main.
  - Every A step must leave it green.
  - Also add a bind gauge on the leak backstop: fires, and the I/cwnd at
    each fire.
- **A1. One s.**
  - First: `echo_srtt_copies_bounded` drives R1 and R2 from one sample
    script under default gates. It asserts |R1 − R2| ≤ a bound read off
    the rounding analysis (≈ 16 ns).
  - Each known divergence then gets its own test that **bounds** it
    (CLAUDE.md: every documented divergence carries a bounding test):
    - revival: post-revival |R1 − R2| ≤ 0.875ⁿ·|pre − first|;
    - `RWM_MSTAR_ANCHOR=0`: the seed blend;
    - `RWM_COPA_WIRE`: different feeds, so the arm keeps a controller-local
      sample path.
  - Then merge into `LegState.s`. Both accessors read it, and
    `mp_time_threshold_split` drops its second argument.
  - The trace pin changes only in the ≤ 16 ns column and the post-revival
    R2 column, and those changes are pinned. This is the §45 Stage 1 item.
- **A2. Move m̂, b̂ and the rate sampler, J1–J3 and ŵ into `LegState`.**
  The Copa controller receives `&LegState`. It is a pure move: the trace
  pin stays unchanged, the R4 cache quirk included.
- **A3. Rename `rtprop` → `srtt` at `emit_source.rs:567` (§44 item 3).**
  Text only.
- **A4. Add readouts and gauges (observation only).**
  - Readouts: `window(δ)`, `spread()`, `rate()`, and the `*_at` stamps.
  - Gauge: a `[LEG]` line per leg every 2 s, under `RWM_DIAG`. It prints
    Ŵ_i beside cwnd, the fraction of time I > Ŵ against I > cwnd, and the
    fraction of time `available() > 0` disagrees with I < Ŵ.
  - Test: the `[GATES]` pin is unchanged, and the gauge is absent with the
    gate off.
  - This gives offline evidence (on the guard's ledgers) for every Phase B
    step before it is built.
- **A5 (separate decision, §45 Stage 1).** Delete the unread J3 candidates,
  or keep them as gauges. Measure host ns per sample first.

**Phase B — consumers (one guard each, in this order: lowest risk, and
evidence from A4, first)**

- **B1. Leak backstop** (`report.rs:183`): `I > cwnd` → `I > Ŵ_i`.
  - Red-first: a leg with Ŵ = 11 and I = 12 decays to 9.
  - Routing: with cwnd poisoned to `MAX_CWND`, the decay still fires.
  - If A4's fire fraction is ≈ 0 under both W's, propose deletion instead
    (an ADR-0073 route), since `expire_in_flight` is the primary release.
- **B2. `spare_capacity`** (`path.rs:467`): (Ŵ_i − I_i)/I_i.
  - Absolute: b̂ = 1000 sym/s, m̂ = 10 ms, Bulk ⇒ Ŵ = 11; at I = 5,
    spare = 1.2.
  - Continuity and monotonicity of Ŵ through every preset at ±2 % (through
    q).
  - Routing: with cwnd poisoned, the repair clamp at `emit_source.rs:526`
    moves.
- **B3. The ρ<1 filter.** At `scheduler/mod.rs:531`, `:552`, `:567` and
  `:593`, `available() > 0` becomes `I_i < Ŵ_i`.
  - It needs the Realtime cells (W5.1), because only the `!reliable` seat
    runs it.
  - Routing test: with cwnd = 0, the leg is still picked while I < Ŵ.
- **B4. The placement queue term** (`path.rs:441–445`): (I_i/Ŵ_i)·s_i. The
  `[ETA]` stamp follows through `place_e`.
  - This is the highest-risk step (c8 water-filling).
  - **Precondition:** L's app-limited rule (§3). Without it an under-placed
    slow leg reads its offered rate as capacity. Then I/Ŵ·s ≈ (s/m̂)·s/(1+q):
    the term prices delay only and stops water-filling by capacity. Copa's
    cwnd avoided this by probing up without delivery evidence. So B4 waits
    on the L rule, or on W3.3's L̂.
  - Tests: the existing placement-law tests (water-filling, spillover,
    T → 0 argmin) must stay green. Absolute: b̂ in a 10:1 ratio at equal m̂,
    s and I gives queue terms in a 1:10 ratio. Routing: cwnd poisoned.
- **B5. Copa's controller.** After B4, nothing on the default path reads
  its dynamics, so removing the calls is wire-identical: host ns before and
  after, and the trace pin minus the cwnd column.
  - **Precondition, re-grepped at that time.** At `fdad664e` the facts are:
    - Copa's dynamics state is read only by monitoring stats
      (`control_msg.rs:365`, `:661`). That covers `in_slow_start` and
      `ssthresh`, which are written at `scheduler/mod.rs:439–440`.
    - `Scheduler::on_loss` (`scheduler/mod.rs:412`) has no caller outside
      tests, so `PathState::on_loss` is already dead on the default path.
    - The live call is `on_delivery_signal` inside `on_ack`.
  - The b̂ feed (`record_delivery`) stays: it is `LegState`'s.
  - Its arms go to the user as one ADR-0073 question: passthrough,
    `COPA_FEED`, `COPA_WIRE` / compete, `CC_PACE`, and `LATE_BRAKE`'s cwnd
    face.
  - `queue_target_mult` is deleted with the controller, or set to 1 + q(δ)
    if an arm is kept (§2).

**Guard attribution.** Each Phase B step changes one reader, and A4's
gauge names that reader's input on the same `[DIAG]` lines. A §41c change
is therefore attributable to one step. The end of Phase A must read WITHIN,
or the trace pin missed something.

## 5. Interfaces for W1 and W3.3

**W1 (a τ per leg for `bbr_rs`).**
- *Candidates.*
  - `tau_lag()`: σ̂_Δ(τ = m̂) on the echo ring. Rate-invariant, but it
    missed its stability bar (paper §7.4).
  - `spread()`: p50 − min.
  - Both are **echo-clocked**, so they include the sender's own dwell. That
    is a loop: window → store occupancy → dwell → τ → window.
  - A dwell-free τ must come from the wire clock. quinn exposes only the
    smoothed RTT (status §42 (b): `latest` and RTTVAR are private). The
    smoothed RTT attenuates c2's jitter ×0.3, so the wire-clock τ is
    computed inside `bbr_rs` from the `RttEstimator` it receives on each
    ack.
- *This design does not pick between the clocks.* W1.2 owns that choice,
  and the loop gain must be shown below 1 if an echo τ is chosen.
- *Seam.*
  - Engine to quinn: a per-leg atomic slot on `QuicTransport`, the
    `set_cc_window_bytes` pattern (`transport/quic.rs:465`, `cc_windows`),
    e.g. `set_leg_tau_us(path_id, µs)`, written where `LegState` updates
    and read by `bbr_rs` on ack.
  - Receiver: if it needs τ, a field in `TxPublished`
    (`scheduler/mod.rs:505–516`).

**W3.3 (the store law).**
- *What `LegState` supplies:* `rate()` (L, with its idle rule; the
  receiver-timestamp L̂ replaces the b̂ feed behind the same readout),
  `rtprop()`, `spread()` with its horizon pair (§3), and `window(δ)` as the
  per-leg term.
- *What W3.3 decides:* the term's form (today (1+q)·L·m̂; the candidate
  g·L·(m̂ + S − b₀)), δ's seat (q(δ) or a readout of u), and which S
  horizon.
- *What follows automatically:* the store cap stays clamp(Σ_i Ŵ_i), and
  B1–B4's consumers follow the term.

## 6. Open questions

1. `queue_target_mult` for a kept Copa arm: 1 + q(δ) (recommended) or
   another form (§2 table). This is the user's call (δ's seat).
2. L's idle and app-limited rule (§3) needs the send-interval sampler on
   the default path, which changes the store law's b̂. Is it done here, as
   a B4 precondition, or in W3.3 with L̂?
3. S's horizon (ring span 32·m̂ against m̂'s 10 s), and whether the
   empty-window chain is `min_j S_j ?: 0`. This is W3.3's.
4. Revival semantics for s: reseed (recommended, today's R1) or continue
   (today's R2).
5. B1: retarget the leak backstop or delete it. A4's gauge decides.
6. The recovery clocks' `max_i s_i` pool over idle-frozen legs (§3). It is
   noted here and belongs to W3.6.
7. The cap consumers' cold chain ends at STORE_CAP_FLOOR, with no ×1.5
   warm-up ramp (§2). Is a warm-up-only difference against Copa acceptable,
   or must the cap chain reproduce a ramp? A4 measures the warm-up length.
8. Units (§2): the shipped store cap already mixes wire-unit Σ b̂·m̂ with a
   source-unit outstanding count, a factor of (1 + r). Whether W3.3
   converts it.

**NO-MODE-SWITCH debt seen and not fixed here** (for Stage 3.4):
- `emit_source.rs:461`: `protocol_hint == Realtime` selects the duplicate.
- The `if reliable { place_symbol } else { select_*_path }` splits at
  `emit_source.rs:308`, `:815`, and `sender_phases.rs:980` and `:1131`.
  They are a ρ-keyed choice between two path laws.
- The ρ<1 cost `effective_delivery_time` (R2-based) is a second path price
  beside `place_e`.
