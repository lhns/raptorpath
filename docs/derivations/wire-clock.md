# wire-clock: which per-path clock the recovery laws need

Status: DERIVATION. No engine change and no VM contact. Feeds the roadmap chain `rec-fix-wire-clock` →
`rec-fix-pooled-clock`, `rec-fix-refresh-clock`, `rec-fix-cooldown`, `rec-fix-time-threshold`,
`rec-fix-nack-throttle`, and the law ledger (status §55) rows E01, E11, E10, C07, C05, C01, C04 and C09.

Code read on `github/main` `7968c870`. Line numbers are on that commit.

Evidence:
- `docs/l1-raw/s53/rep53-diag-ld.tgz`: c8-100 at 600 MB (BASE, BASES, CAND, CANDS, CW0, 6 rows each) and
  c1d-400 (BASE 12, CAND 12, CANDS 6). Here CAND is lever B, the 64-gap horizon.
- `docs/l1-raw/bbr48b/diag-ld-full.tgz`: c1s-400, c1d-400, c2-100 and c8-100, 12 rows per arm. Here
  **"CAND" is `bbr_rs`**, not lever B, so this document writes it as BRS.
- Windows: client `[DIAG]` ticks with src > 0, from t ≥ 10.5 s on s53 (after the start-up sample leaves
  Copa's 10 s window, p0-dwell-diag §2.1) and from t ≥ 2 s on §48b.
- Script: `wire-clock/clocks.py`, stdlib Python. It takes an extracted `diag-ld` directory and a cell.
  The `[HOLD]`, `[FCAUSE]`, `pf=` and `supp=` tallies are grep one-liners over the same logs.

## 0. Answer

**The ledger's proposed fix is the wrong measurand for the two laws that matter most.**
- The time threshold (C01) and the cooldown (C05) need the time a non-lost flight on its own path
  takes to show up in a gap report, measured from the stamp their age runs from. That is the
  **per-path evidence loop**.
- The per-path app-echo RTT (E01) is the engine's estimator of that loop: the echo and the gap report
  ride the same `WindowAck` on the same path. quinn's wire RTT is not.
- At c1d the loop is 18–25 ms on a 2–3 ms wire RTT. A wire-clocked threshold would sit at its 10 ms
  floor, under the age at which holes are first reported (p50 16–33 ms), while 98 % of those holes
  self-heal within about 1–2 ms of that report. That predicts a retransmit flood (§4.2).
- Between §48b's two substrates, the c2 wire RTT falls 5× (116 → 22 ms) while the loop stays at
  206–207 ms. A wire clock would have moved 5× with the thing it must predict unchanged.

**The real recovery-clock defects are three, and none of them is the measurand.**
1. **E11, the pool.** The cooldown, the N = 1 age gate and every fallback read the max over legs, so
   a retransmit on the fast leg is clocked by the slow leg.
2. **C04, the N-step.** N = 1 runs a different law: the `srtt/2` age gate, no packet threshold, no
   time threshold on the live flight.
3. **E10, the receiver's seed.** The receiver's refresh reads a 50 ms constructor seed.

The fix is one law at every N on a per-path loop clock, with no pool and no cooldown:

```text
   L_j      =  max( s_j , l_j )                                (per-path evidence loop)
   ripe(s)  ⇔  now − t_live(s)  ≥  max( 9/8 · L_p(s) , g )      (time channel)
   lost(s)  ⇔  ripe(s)  ∨  fast(s)                              (at every N; unknown flight ⇒ lost)
```

**The wire clock W_j (quinn's smoothed RTT) has two consumers in this chain.**
- The receiver: it is the only per-path clock the receiving endpoint owns.
- C09's congestion input: a network-queue signal. Do not swap that input before C09 has its gauge
  (§3.6).
- The store law's RTprop is a third statistic, a windowed minimum of wire samples. It is not this
  clock (§6.3).

**Predicted effects at the measured cells (§4).**
- **Time threshold:** unchanged by construction at every cell. It is today's per-path form, except
  that RFC 9002's `latest` term replaces a duplicate EWMA.
- **Re-fire interval:** shorter for retransmits on the fast c8 leg: −64 ms BASE, −17 ms CAND,
  −30 ms CANDS. CW0's p1 retransmits are −134 ms.
- **N = 1:** re-fires are 12 % longer (c2 206 → 232 ms).
- **N = 1, first fires:** they now depend on a fast channel that has **never executed at N = 1**
  (`fast=0` on 48/48 single-path rows). The c2 prediction is therefore two-branch, and it hinges on
  one wiring prerequisite (§3.3).
- **Refresh:** its clamps bind with or without the honest input, on every default-substrate cell.
  E10 is an input fix with no output effect; C07's defect is its clamps (§3.5).

**Owner questions:** §7.

## 1. What each law needs, from its own sentence

The age every sender-side law compares is `now − t`. Here `t` is the engine's stamp: the placement
lock for an original (`emit_source.rs:457`, `src_send_ts_us`), or the retransmit's staging time
(`sender_phases.rs:1095`, `mod.rs:3413`). Both precede the owner channel, quinn's datagram send
buffer, quinn's pacer and the kernel. **A clock compared against that age must start at the same
origin.** This is the shape check that decides the measurand.

| law (ledger row) | its sentence | what the sentence needs |
|---|---|---|
| time threshold (C01) | RFC 9002 §6.1.2: declare a packet lost once it was sent `kTimeThreshold · max(smoothed_rtt, latest_rtt)` ago and a later packet has been acknowledged | **RFC's RTT is time_sent → ACK received**, so it includes the ACK's return. The engine's "later packet acknowledged" is the gap report. The analog is placement stamp → gap report ingested, on the live flight's path. RFC 9002 §6.1.2 has **no RTTVAR term**: its jitter guard is the `latest` term and the 9/8 margin. Variance belongs to PTO (§6.2.1), whose analog here is the tail sweep (C06, §3.7) |
| packet threshold (C02 channel) | RFC 9002 §6.1.1: lost once 3 later packets in the same number space are acknowledged | no clock. Same-path FIFO evidence; the number space is the path |
| cooldown (C05) | "repeated gap acks for the same hole must not resend more than once per SRTT" (`sender_phases.rs:880`) | the evidence loop of the **retransmit's** path, from the retransmit's stamp: the earliest time a report can reflect that copy's arrival |
| N = 1 age gate (C04) | "only repair symbols old enough that an in-flight copy would already have been sacked" (`sender_phases.rs:959`) | the same evidence loop as C01, at N = 1. The `/2` has no derivation (§3.4 of the status: pre-RFC) |
| receiver refresh (C07, input E10) | "this cadence re-presents a stalled hole to the sender" (`recovery_laws.rs:294`) | the receiver's round trip to a repair, report → sender → repair → arrival. The receiver cannot see the sender's dwell |
| pooled clock (E11) | none for the max. It is the input of C05, C04, C06 and two fallbacks | per-seq laws need **their own flight's path**, never a cross-leg aggregate. Only a law that waits on *every* outstanding flight (the tail sweep) has a reason for a max, and only over the legs holding those flights |
| NACK throttle (C09) | "halve on rising loss AND rising RTT" (`mod.rs:287–320`) | a *network* congestion signal: queue growth on the wire. Not the sender's own buffer growth, and not a 10 s minimum |

**The loop and its parts.** For a flight of seq s on path j, with its report returning on path b:

```text
   loop_j,b(s)  =  τ_j(s)   +   F_j       +   B_b
                   pre-wire     forward       report generation + return + sender ingest
                   dwell        (wire + receiver ingest)
```

The app-echo sample on path j measures `τ_j(echoed seq) + F_j + B_j`. Under ack-merge, the default
(`receiver.rs:1948`), the echo rides the gap-bearing `WindowAck`, sent on the data's own path. **So
for b = j the echo measures the loop, up to the dwell difference between the echoed seq and s.**
- At N = 1, b = j always.
- At N > 1 the return-leg mismatch `B_b − B_j` is what makes a report stale. The snapshot-order law
  (C03) owns that term (§6.1); the time threshold does not.

## 2. The candidate sources, as the code has them

### 2.1 quinn's per-path RTT (W_j)

- **What it measures.** quinn-proto 0.11.14 `RttEstimator` (`connection/paths.rs:285–354`):
  - the sample is `now − time_sent(largest acked)`, where `time_sent` is stamped at packet build
    (`packet_builder.rs:208`);
  - `smoothed` is the RFC 6298 EWMA, 7/8, with `var` at 3/4. It is ack-delay-adjusted, and only
    when `min + ack_delay ≤ latest`;
  - `min` is a lifetime minimum that ignores ack delay.
- **What it includes.** Packetization happens after quinn's datagram send buffer and after its pacer,
  so **W excludes the sender's quinn buffer dwell and pacing**. It includes the kernel, netem's queue
  (c8 p0: 103–159 ms on a 10 ms leg, p0-dwell-diag §2.3) and the peer's ACK generation minus the
  reported ack delay. It excludes everything before packetization:
  - the engine's staging (`TxBatch`, the owner channel);
  - the receiver's application ingest;
  - the `WindowAck` path back.
- **How the engine reads it.**
  - `PathView.rtt_us` (`io_owner.rs:262`) is written by the owner from `conn.stats().path.rtt` on
    every full snapshot, at most every `VIEW_REFRESH` = 1 ms of owner activity. Under
    `RWM_COPA_WIRE` it is written from `conn.rtt()` on every owner poll (`io_owner.rs:1352–1369`).
  - `Scheduler::mirror_wire_rtts` copies it into `PathState.wire_rtt` unconditionally on every
    control message (`scheduler/mod.rs:196`; callers `control_msg.rs:236/419/510`).
  - **Cost to a new reader: zero.** One atomic load, already paid.
  - **Freshness:** ≤ 1 ms behind quinn while the owner is busy. quinn samples at idle too: the 2 s
    report and keep-alives (w3-per-leg-state §3).
- **What is not readable.**
  - `var` is private, and `pto_base` is `pub(crate)`.
  - `latest` is reachable only as `conservative() = max(smoothed, latest)`, inside a congestion
    controller's `on_ack`. That is the `RouteProbe` wrapper (`io_owner.rs:474–482`), installed only
    under `route_probe`.
  - `on_ack` runs before `rtt.update` for the same ACK (`connection/mod.rs:1592` vs `:1522`), so the
    estimator it sees is one ACK stale. The paper's §8.4 records the same fact.
  - `congestion_state()` exists only on `github/bbrrs-r4`. It clones the controller under the
    connection lock, and `[QCC]` pays that once per DIAG tick (`bbrrs-r4` `io_owner.rs:274`, `:699`).
    It is not a per-decision source.
- **Failure modes.**
  - **`min()` latches low.** It is a lifetime minimum, and quinn's `now` is a per-poll stamp that can
    precede the arrival (paper §8.4; §47's c1s window collapse). **No recovery clock may read
    `min()`.**
  - **The smoothed value can lag a rising RTT,** which is why RFC 9002 adds `latest`, and the
    engine cannot read `latest` off the published view.
- **Receiver side.** The receiving endpoint's quinn measures the same path on its own ack-eliciting
  datagrams (its `WindowAck`s). Its `[DIAG]` prints it. Agreement with the sender's W, medians of
  per-row medians:

  | cell | W (sender) | W^rx (receiver) |
  |---|---|---|
  | c8 p0 / p1 (s53, five arms) | 111–160 / 207–351 ms | 111–160 / 214–343 ms |
  | c2 (BASE / BRS) | 116 / 22 ms | 109 / 21 ms |
  | c1s, c1d | 2–5 ms | 2–5 ms |

### 2.2 The engine's app-echo samples (e_j)

- **What it measures.** `now − echo_send_timestamp_us` (`control_msg.rs:290`, `:572`). The echo is
  the triggering batch's stamp, taken at the placement lock. A sample is therefore
  `τ + F_j + B_j` (§1), sampled once per `WindowAck` on the data's own path.
- **Smoothing.** EWMA 1/8 (`estimator.rs:465`). The first sample replaces the 50 ms seed under
  `RWM_MSTAR_ANCHOR`. Copa's `srtt` is a second EWMA of the same stream: E02, and the two copies are
  what `max(srtt, ewma)` takes the max of (w3-per-leg-state R1/R2).
- **What it includes.** Everything the loop includes. Decomposition, from medians of per-row medians
  (ms; τ is quinn buffer dwell by Little, `(hand − tx)/(Δtx/Δt)`, p0-dwell-diag §1):

  | cell, arm, path | e | W | τ | residual e − W − τ |
  |---|---|---|---|---|
  | c8 BASE p0 / p1 | 204 / 269 | 126 / 245 | 68 / 0 | 10 / 24 |
  | c8 CAND p0 / p1 | 351 / 320 | 126 / 309 | 222 / 1 | 3 / 10 |
  | c2 BASE | 206 | 116 | 83 | 7 |
  | c2 BRS | 207 | 22 | 152 | 33 |
  | c1s BASE | 19.5 | 3.5 | 11.5 | 4.5 |
  | c1d BASE p0 / p1 | 21 / 22.5 | 3 / 2.5 | 0.9 / 1.0 | 17 / 19 |

  **The c1d residual (17–19 ms) is unlocated.**
  - Its forward part is real latency: receiver `[LAT]` `ax_p50` is 15–16 ms, which is arrival minus
    stamp, above the path floor.
  - `[LAG]` p50 is 0.5–0.7 ms on both ends, and quinn's buffer holds about 1 ms. So it sits in engine
    staging, in receive-side buffering before the arrival stamp, or in both. Not diagnosed here.
  - The echo form does not need it located: it measures it.
- **Cadence.** One sample per data `WindowAck`. Retransmits, repairs and the tail sweep carry the 0
  sentinel and **never sample** (`control_msg.rs:569`; p0-dwell-diag §1's CW0 gap).
- **Failure modes.**
  - **Lag.** A sample reports the dwell of a seq placed one loop earlier. The 9/8 margin covers the
    dwell change over one loop (§3.1).
  - **Idle legs.** The clock freezes at the value that idled the leg. That is pessimistic: it costs
    time, never exclusion (w3-per-leg-state §3).
  - **Cold legs.** Before the first echo the value is the 50 ms seed (§3.2 gives the rule).
  - **Report-path asymmetry** at N > 1 (§6.1).
  - **The receiver has no echo at all** (E10).

### 2.3 `bbr_rs`'s m̂ (branch `github/bbrrs-r4`)

- **What it measures.** The 10 s windowed minimum of `r_e = a_e − max time_sent`, on the controller's
  own ack clock. The `bbr_rs` header (`bbrrs-r4` `transport/bbr_rs.rs`, "Window law") fixes the
  early-stamp latch this way. ProbeRtt re-installs it from one clean round. The `[QCC]` gauge exports
  it with a smoothed `qsr_us` beside it.
- **What it includes.** Wire time only, like W, and its minimum over 10 s.
- **Where it exists.** Only when `bbr_rs` is the substrate (K09 is pending), and readable today only
  through the `congestion_state()` clone.
- **Use.** It is an RTprop estimator, a propagation floor. **It is not a recovery clock.** A
  threshold at the loop's floor fires on every flight that meets any queue. It is the store law's
  candidate input (§6.3).

## 3. The formulas

### 3.1 The per-path evidence loop

```text
   L_j  =  max( s_j , l_j )
```

| symbol | meaning | provenance |
|---|---|---|
| `s_j` | smoothed app-echo RTT of path j, EWMA 1/8. **One copy**: w3-per-leg-state's `LegState.s`, which retires the E02 duplicate | measured (echo samples on j); gain cited (RFC 6298) |
| `l_j` | path j's latest echo sample (`LegState.ring`'s newest entry) | measured |
| `max(·, l_j)` | RFC 9002 §6.1.2's `max(smoothed_rtt, latest_rtt)` | cited |

Today's `max(srtt_j, ewma_j)` is the max of two EWMAs of the same stream. It has the RFC's shape but
not its second term. **The logs cannot evaluate `l_j`:** `[DIAG]` prints only the EWMA. The term can
only raise L_j, by about the echo's dispersion (`sig_us` 1.6–3.4 ms at c2/c8).

### 3.2 The unmeasured leg

A recovery clock may not read the 50 ms constructor seed. That is E10's defect class, and with the
pool gone (§3.4) nothing would hide it.

```text
   L_j  :=  max( W_j , max_{i measured} L_i )      while path j has no echo sample yet
```

- **Provenance:** w3-per-leg-state §3's idle rule. Pessimism is allowed where it costs time, not
  exclusion. W_j is measured: quinn has an RTT from the handshake on, so the chain never ends in a
  constant.
- **The step at a leg's first sample is information availability**, like `[HOLD]`'s "reported"
  condition. It reads no dial and no N.
- **Retransmit-only legs** keep their last measured `L_j`, frozen and pessimistic, because sentinel
  echoes never sample. A `[DIAG]` counter of judgements made on a frozen or cold clock is owed (§5).

### 3.3 The loss law, at every N

```text
   ripe(s)  ⇔  now − t_live(s)  ≥  max( κ · L_p(s) , g )
   lost(s)  ⇔  t_live(s) unknown  ∨  ripe(s)  ∨  fast(s)
   fast(s)  ⇔  s never retransmitted  ∧  #{ q > s delivered : path(q) = path_orig(s) }  ≥  3
```

| symbol | meaning | provenance |
|---|---|---|
| `t_live(s)` | stamp of the live flight: the last retransmit's staging if any, else the original's placement | measured (`nack_retx_at`, `retransmit_buffer`) |
| `p(s)` | the live flight's path | measured |
| `κ` | kTimeThreshold = 9/8 | cited (RFC 9002 §6.1.2; empirical there) |
| `g` | the floor | **kept at today's `NACK_RETX_COOLDOWN_FLOOR_US` = 10 ms** in this change (UNDERIVED: "10× kGranularity by choice"). The FORMULA-FIRST value is kGranularity = `TIMER_GRANULARITY_US` = 1 ms (derived, `recovery_laws.rs:23`). The floor binds 3.7–17 % of evaluations at c1d and 0–7 % at c1s (`pf=`), 0 at c2/c8, so its value is live at c1. Change it separately (§7 Q4) |
| `3` | kPacketThreshold | cited (RFC 9002 §6.1.1) |
| "delivered above s" | from the report's implied intervals (`mp_delivered_intervals`), **plus the extent above the top gap** (below) | measured |

**What changes against main.**
- The `mp_n_paths > 1` gates are removed: the branch at `sender_phases.rs:898`, the evidence ingest
  at `:773`, and `mp_hole_ripe`'s N ≤ 1 bypass (`recovery_laws.rs:187`).
- `legacy_age_ripe` and the `RWM_RECOV_SP` arm (`:938`) are deleted. The arm is the time channel
  alone at N = 1, which the paper (§7.1) records as costing throughput.
- The flight's path with no snapshot (`mp_thr_of`'s `None => retx_cooldown_us`) becomes "ripe". Such
  a flight is on a dead path, and the rule is never to suppress what cannot be clocked.

**The wiring prerequisite: the extent above the top gap.**
- Today the evidence proves only **one** delivered seq above the top gap (`mp_delivered_intervals`, `recovery_laws.rs:148`).
  A new hole is usually the top gap of its first report.
- The v9 `WindowAck` carries `received_above`. When the gap list is not truncated (fewer than
  `MAX_NACK_GAPS` runs), the seqs above the top gap are contiguous and number exactly
  `received_above − Σ(listed intervals)`. The extent is then known, and its paths follow from
  `source_path_map` at every N.
- `received_above` reaches `serve_gaps` on `github/snapver-law` (C03's V), not on main.
- When the list is truncated, the extent stays 1, as today. That too is information availability.

**Shape check (sentence against expression).**
- **Units:** µs against µs.
- **Monotonicity:** the threshold rises with L, and L rises with s and with l.
- **Order in N:** N⁰. The threshold for a flight on j reads path j only, so adding, removing or
  idling another path changes no term of it.
- **N = 1 → 2:** no term steps. This removes C04's step.
- **Origin:** both the age and L run from the engine stamp (§1).
- **Return leg:** L carries B_j and not B_b; C03 owns B_b − B_j (§6.1).

### 3.4 The cooldown and the pool

```text
   cooldown_k  =  L_k   ≤   κ · L_k        ⇒  cooldown_elapsed  ⇐  ripe   (same stamp nack_retx_at)
```

- **C05 is dominated** by the time channel on the live flight at every N, so it is **deleted**. If
  it is kept for a release, its bind fraction is predicted at exactly 0 and is gaugeable (§5).
- **E11's pool** has no per-seq reader left. Every reader of `cooldown_elapsed`, `retx_cooldown_us`
  and `pooled_recovery_srtt_*`:

| reader | today | disposition |
|---|---|---|
| gap loop cooldown (`sender_phases.rs:882`) | `max(pool, 10 ms)` | deleted (dominated) |
| N = 1 age gate (`:964`) | `pool/2` | deleted (§3.3) |
| `mp_thr_of` / `mp_thr_pure` fallback (`:737`, `:750`) | the pooled cooldown | "ripe" (dead path) |
| tail sweep (`mod.rs:2719`, C06) | `clamp(2·pool, 25, 100 ms)` | **kept** until C06's derivation (§3.7). Its fires go through `serve_gaps` (`mod.rs:2705`), so they are gated by the live-flight time channel after C05's deletion. Deleting C05 does not un-gate them |
| `[HOLD]` classifier `half_srtt_of` fallback (`:755`) | `pool/2` | gauge only, gates nothing; repoint to `L_p/2` when touched |
| receiver-requested repair (`mod.rs:3413`) | stamps `nack_retx_at`, reads no cooldown | unchanged |
| `expire_in_flight` under `RWM_RELEASE_1TO1` (`path.rs:1179`, off) | shares the 10 ms constant | unchanged; it shares `g` if g changes |

### 3.5 The receiver's refresh (E10, C07)

```text
   R  =  clamp( 2 · max_j W^rx_j ,  25 ms ,  100 ms )       (100 ms before any W^rx)
```

| symbol | provenance |
|---|---|
| `W^rx_j` | measured: the receiving endpoint's own quinn smoothed RTT on path j, read from its `PathView`. It agrees with the sender's W within the table in §2.1. No wire-format change |
| `max_j` | today's aggregate over `live_tx_views`, kept. It is continuous in N |
| `2`, `25 ms`, `100 ms` | **UNDERIVED** (status §3.4 refresh row) and unchanged here |

- **W^rx is a lower bound of the repair loop.** It misses the sender's dwell and the residual. Under-
  reading only re-presents early, and the sender's time channel on the live flight suppresses an
  early re-presentation. The cost is report traffic, never a spurious retransmit.
- **The finding (rule 18):** under the honest input the clamps bind on every default-substrate cell.

  | cell | 2·max W^rx | refresh |
  |---|---|---|
  | c8 (all five s53 arms) | 428–686 ms | ceiling, 100 ms |
  | c2 BASE | 218 ms | ceiling, 100 ms |
  | c1s, c1d | 4–10 ms | floor, 25 ms |
  | c2 BRS | 42 ms | 42 ms, the only unclamped cell |

  E10's input fix changes C07 from DEGENERATE to PINNED. The output changes only at c1 (100 → 25 ms,
  where `gap_refresh` fires are 0–28 per run) and under `bbr_rs`. **C07's open defect is its clamps
  and its factor 2.** Those belong to `rec-fix-refresh-clock`, not to this input.
- **Rejected alternative:** publishing the sender's L_j to the receiver. That needs a wire-format
  change for an output the clamps discard today.

### 3.6 The NACK throttle's RTT input (C09)

- **What it reads today.** The input is `copa_min_rtt()` of the worst-ε path (`channel_set.rs:79`):
  a 10 s windowed minimum of the echo. "Rising by more than 1 ms in two consecutive 5 ms periods"
  almost never fires on a windowed minimum. **The halving branch is therefore probably dormant**; no
  gauge says so.
- **The honest signal** for "congestion" is the wire queue on that path, W_j. But W_j moves by
  milliseconds every period under c2's 3 ms jitter, so swapping the input would probably **wake** a
  halving rule whose seven constants have no provenance.
- **Disposition: do not change C09's input in this chain.** Gauge first: the halving count, the
  multiplier distribution and the per-report cap bind fraction. Then a paper entry or deletion, as
  the ledger says. When it is derived, its RTT symbol is `W_j` of the path it judges.

### 3.7 The tail sweep (C06, out of this chain's scope)

For the record, the shape its sentence implies, as the PTO analog (RFC 9002 §6.2.1):

```text
   T  =  max_{j : j holds an outstanding tail flight} ( L_j + max(4·v_j, g) )
```

- `v_j` is a measured per-leg dispersion (E09: four already exist, gauge-only).
- The max is legitimate here: the sweep waits on every outstanding flight. It is a max over the legs
  holding them, not over the live set.
- It is pinned at 100 ms today. It is not derived here.

## 4. Expected effect at the measured cells

Medians of per-row medians, row range in brackets, ms. "today" is the shipped value and "new" is §3.
Time windows as in the header.

### 4.1 Time threshold (C01): unchanged by construction

Today `9/8·max(srtt_j, ewma_j)`; new `9/8·max(s_j, l_j)`. With equal EWMAs they differ only by the
`l_j` term, which the logs cannot evaluate. `[HOLD] thr_p50_us` cross-checks 9/8·rtt:
- c2: 229 ms against 9/8 × 206 = 232 ms;
- c1d: 18–37 ms against 9/8 × 21 ≈ 24 ms.

| cell | p0 | p1 |
|---|---|---|
| c8 BASE | 230 [215–255] | 303 [246–356] |
| c8 CAND | 394 [377–407] | 360 [289–441] |
| c8 CANDS | 381 [361–403] | 412 [284–507] |
| c8 CW0 | 419 [406–425] | 240 [219–329] |
| c1d BASE | 24 [18–570] | 25 [20–70] |
| c2 BASE / BRS | 232 / 233 | — |
| c1s BASE | 22 [20–107] | — |

**Predicted spurious and late retransmits from C01:** unchanged, apart from the `l_j` term, which
only delays.

**The rejected wire form,** `max(τ_j + 9/8·W_j, 10 ms)` with τ by Little:

| cell | wire-form threshold | today | first-report age, `[HOLD]` p50 |
|---|---|---|---|
| c1d BASE, CAND, CANDS | **10 [10–10]**, floor at 100 % | 21–25 | 16–33 |
| c2 BRS | 177 [176–179] | 233 | — |
| c8 p0 | −14 to −43 against today | — | — |

At c1d `[HOLD]` reads 98 % of holes self-healing (`hn` / `fed` = 122 476 / 125 319 on BASE s42 r1),
`hn_p50` 0.9–2.3 ms after the first report, and `ripe_frac` 0.4–3.5 % at today's threshold. Under
the wire form essentially every reported hole (`fed` 54 k–355 k per path per run) is ripe at its
first report. Today a run fires 11–28 k. **The prediction is a several-fold rise in spurious
retransmits at c1d.**

### 4.2 Re-fire interval (C05 + E11): the change

Today N > 1 uses `max(max_i e_i, 9/8·e_k)` for a retransmit on k: the pool and the live-flight
threshold both apply. N = 1 uses `max(e, 10 ms)`, the cooldown alone. New is `9/8·L_k`.

| cell | retx on p0: today → new | retx on p1: today → new | retx leg-1 share (§53) |
|---|---|---|---|
| c8 BASE | 294 → 230 (**−64**) | 303 → 303 | 0.59 |
| c8 BASES | 277 → 234 (−43) | 278 → 275 | 0.60 |
| c8 CAND | 411 → 394 (−17) | 379 → 360 (−19) | 0.74 |
| c8 CANDS | 412 → 381 (−30) | 412 → 412 | 0.73 |
| c8 CW0 | 419 → 419 | 374 → 240 (**−134**) | 0.17 |
| c1d BASE / CAND | 26 → 24, 22.5 → 21 | 26 → 25, 23 → 22 | — |
| c2 BASE (N = 1) | 206 → 232 (**+26, +12.5 %**) | — | — |
| c1s BASE (N = 1) | 19.5 → 21.9 (+12 %) | — | — |

**Predicted effect.**
- A re-fire happens only after the first copy is lost or the report is stale. The legs are lossy at
  2.5 % (c2-shaped p0) and 4.8 % (c3-shaped p1), so re-fires are a few percent of fires.
- **c8:** shorter re-fire on the fast leg lowers the latency of the holes whose first retransmit was
  lost. The spurious risk per re-fire is the same 1/8 margin as a first fire. Predicted: duplicates
  and retransmits/GB inside BASE's row range on every arm, and in-order delay inside noise. **c8 is a
  no-regression row, not a confirming one.**
- **N = 1:** re-fire is 12 % later. At ε ≈ 2.5 % that adds about 0.025 × 26 ms ≈ 0.7 ms to the mean
  repair latency at c2: inert.
- **C05 bind fraction** if kept: predicted 0 at every cell. Today it is the dominant gate. The
  share of gap-seq evaluations it suppresses (`supp=` first field over `seqs=`, per row) is:
  - 98.3–98.5 % at c2;
  - 92–97 % at c8 on main (33–42 % under lever B);
  - 67–95 % at c1s;
  - 0.4–26 % at c1d.

  After deletion those evaluations meet the live-flight time channel. On the fast c8 leg that
  channel passes them earlier, which is the −64 ms row above.

### 4.3 N = 1 first fires (C04 → the unified law): two-branch prediction

- **C04's "never gauged" is settled post hoc: it suppresses nothing.** `supp=…/0/…` (age = 0) on
  48 of 48 single-path rows (c2 and c1s, BASE and BRS, §48b). The N = 1 law today is "fire at first
  report, then the cooldown".
- **Under the unified law, 94 % of c2 fires (`y`/`fired` = 11 550–13 329 / 13 191–14 338) and
  30–70 % of c1s fires are younger than the threshold.** They fire at first report only if the fast
  channel fires. `fast=0` on all 48 rows: the channel has never run at N = 1.

| branch | condition | c2 prediction | c1s prediction |
|---|---|---|---|
| A | the extent above the top gap is wired (§3.3) | a new hole has ≥ 3 delivered same-path seqs above it at its first report (FIFO, 2 ms report cadence ≈ 19 seqs): **fast fires at first report as today**; retransmits/GB, duplicates and goodput unchanged within the v2 resolution (c2 goodput d80 1 %); `fast/fired` ≈ `y/fired` ≈ 0.94 | as c2; `fast/fired` ≈ 0.3–0.7 |
| B | not wired | the top hole of each first report waits for a later report showing a new gap above it, bounded by θ − age ≈ 35 ms. At ε ≈ 2.5 % with GE bursts of about 2, a new gap arrives about every 80 seqs ≈ 8 ms: about +8–10 ms on most first fires, against a 206 ms loop. **c2 goodput at risk at the 1 % clause**; do not register B as no-regression | the top-hole wait is about one inter-loss interval (~1 000 seqs ≈ 12 ms at 85 k/s), bounded by θ − age; a large share is already ripe at first report |

- **Spurious at N = 1:** unchanged in both branches. Netem is FIFO, so the fast channel cannot fire
  on a non-lost original, and π₀ ≈ 0.005 (paper §7.2).
- **CPU at N = 1:** the evidence ingest pushes one entry per delivered seq inside a gap-bounded
  interval, pruned at the frontier (`sender_phases.rs:1442`). c1d already pays it. The c1s CPU/GB
  clause should be read for it.

### 4.4 Refresh (E10/C07)

See §3.5's table. c8 and c2 are unchanged (100 ms). c1s and c1d go 100 → 25 ms, where
`[FCAUSE] gap_refresh` is 0–28 per run (0 on most rows), so the predicted fire change is about 0.
c2 BRS goes to 42 ms.

## 5. What must be measured to confirm, and the guard

**Host tests first, always on, absolute.** These are the formula's invariants, and each proves its
wiring executes (MD rule 1).
1. **N-continuity:** the loss decision for a flight on path j is bit-identical with and without a
   second, idle path. This is the C04 step's regression test.
2. **Path locality:** perturbing any path i ≠ j's clock leaves j's threshold unchanged. This is
   E11's test.
3. **Anchor:** threshold = `max(9/8·L, g)` exactly at L ∈ {g·8/9, 10 ms, 200 ms}, and `L = max(s, l)`
   picks `l` when `l > s`.
4. **Dominance (if C05 is kept for a release):** `ripe ⇒ cooldown_elapsed` for every reachable state.
5. **N = 1 fast-channel wiring:** a single-path `serve_gaps` drive where the top gap has ≥ 3
   delivered seqs above it fires through `fast` on its first report. This is red on main. It needs
   the §3.3 extent.
6. **Cold leg:** a retransmit on a leg with no echo is clocked by the §3.2 chain, never by the
   50 ms seed.
7. **Receiver:** the refresh reads the receiving endpoint's own `PathView` RTT. A one-way transfer
   whose own sender has no samples gets `clamp(2·W^rx)`, not 100 ms.

**Gauges, each with its bind fraction reported.**

| clamp or gate | gauge | predicted |
|---|---|---|
| floor g on the threshold | `pf=` (exists) | 0 at c2/c8; 3.7–17 % c1d, 0–7 % c1s (unchanged while g = 10 ms) |
| C05, if kept | new: cooldown-suppressed seqs the time channel would pass | 0 |
| refresh clamps [25, 100] | new receiver counters `rf_floor` / `rf_ceil` | ceiling 100 % c2/c8; floor 100 % c1s/c1d; unclamped c2 BRS |
| tail sweep [25, 100] | `[QCLK] w_us` (exists) | 100 ms at p05 = p95 on c8 (unchanged) |
| cold or frozen clock judgements | new `[DIAG]` count per path | ≈ 0 after warm-up |
| fast vs time at N = 1 | `fast=`, `y=`, `r=` (exist) | branch A: `fast/fired` ≈ 0.94 at c2 |
| time fires on stale reports | `[SNAPV] tt_st` (exists) | unchanged on main; 0 with C03 |
| C09 halving and the per-report cap | new | measured before any C09 change |

**Guard v2 plan (§41d: c2/c8 at 600 MB, the in-order delay clause).**
- **Prerequisite:** host tests 1–7 green, and branch A wired. Without the §3.3 extent, do not run the
  guard: branch B predicts a c2 goodput risk at the 1 % clause.
- **Scored: CAND-wire against main,** with registered predictions:
  - c2 and c1s: goodput, retransmits/GB and duplicates inside main's row range; c1s CPU/GB inside
    its range, or the ingest cost reported;
  - c8: in-order delay mean and duplicates inside main's range; p0-retransmit re-fire −40 to −70 ms
    on the `[HOLD]` retransmit-closed timing;
  - c1d: unchanged.
- **Reported (discriminating for the mechanism, not for the merge):**
  - the N = 1 `fast/fired` share;
  - `pf=` at c1d;
  - the receiver's refresh bind counters;
  - C05's dominated count.
- **Falsifiers:**
  - any c1d rise in `y` above main's row range (the clock under-reads the loop);
  - `fast=0` at N = 1 on CAND-wire (wiring);
  - a non-zero C05 dominated count;
  - c2 goodput below main's range with `fast/fired` < 0.8 (branch B in effect).
- **The floor change** (g = 10 → 1 ms) is a separate arm with its own c1d prediction. It is not
  part of this guard (§7 Q4).

## 6. Interactions

### 6.1 The snapshot-order law (`github/snapver-law`, paper §7.1 on the branch)

- **It owns the return leg.** L_j carries `B_j`. A report on path b ≠ j that is older than the newest
  report already dequeued fails `V ≥ V*` and is never judged. On main the time channel fires on
  stale reports 999 (BASE) and 2 481 (CAND) times per c8 run (`tt_st`); CANDS reads 0. The clock
  change does not touch these.
- **Residual exposure with C03:** a report on b that is the newest one, while j sends no fresher
  reports. Gauge it as `tt_st` under C03.
- **Dependency:** the branch also carries `received_above` into `serve_gaps`, which is §3.3's wiring
  prerequisite. **The unified N = 1 law depends on C03's plumbing.**

### 6.2 Lever B, the gap horizon (C02)

- **Under the 20-gap horizon the time channel is mostly irrelevant at c8.** p0 holes reach the law at
  about 3.1–3.5 s of age (`[HOLD]` `age_p50` 3.1 s, `ripe_frac` 0.70 on BASE p0), far past any
  threshold.
- **Under lever B holes are judged young,** so the threshold and the re-fire interval govern. That
  is where §4.2's p0 changes act.
- **The horizon also truncates the gap list.** That sets when §3.3's top-extent is available: the
  20-run list truncates more often than the 64-run one.

### 6.3 The store law's RTprop (E03, F01)

**It is not the same source.**
- **The store wants propagation:** `bdp = rate × RTprop`, a windowed minimum of wire-timed samples.
  Its candidates are `bbr_rs`'s m̂, or a windowed minimum over quinn's per-packet samples taken in
  the controller hook. **Not quinn's `min()`**, which latches (§2.1).
- **The recovery laws want the loop,** a smoothed value plus latest, of the echo.
- **They share only the per-path structure.** Merging them would put a propagation floor under a
  threshold that must cover queue and dwell.
- **E01 stays DISHONEST for the store** (and for S01's span deadline, E12 and R07), which read the
  echo as RTprop or a path RTT.

### 6.4 The per-leg state design (`github/perleg-design`, `docs/derivations/w3-per-leg-state.md`)

- **L_j is a readout of `LegState`:** `s` (one EWMA, R1 = R2) and the newest `ring` entry for `l`.
  The receiver's W^rx is its own `LegState.w`. No new state is needed, except that the ring's newest
  sample must be read undecimated: the ring is time-decimated at m̂/8, so `l` needs the newest raw
  sample.
- **It closes w3 open question 6:** "the recovery clocks' `max_i s_i` pool over idle-frozen legs".
  The per-seq laws no longer pool, and the remaining max (the tail sweep) ranges only over legs with
  outstanding flights.
- **w3 §3's idle rule supplies §3.2's provenance.**
- **Ordering:** this change is small enough to land on today's `est.rtt()`. It reads the same value
  as `LegState.s`, because E02 equals E01 bit for bit while `RWM_COPA_WIRE` is off. It can land before
  or after the `LegState` migration.

## 7. Open questions for the owner

1. **Verdict narrowing (ledger, not edited here).** Should E01 become **OK for C01/C05** (the
   per-path echo is the evidence-loop estimator) and stay DISHONEST-INPUT for E03, S01, E12 and R07?
   Should C01's DISHONEST-INPUT become "duplicate EWMA instead of RFC `latest`"?
   - That moves the recovery chain's root from E01 to **E11 + C04**.
   - `rec-fix-wire-clock` itself dissolves into: no new clock for C01/C05; W^rx for E10; W_j for
     C09 only when C09 is derived.
   - The roadmap chain would read: `rec-fix-pooled-clock` (E11, C05 deletion) ═
     `rec-fix-time-threshold` (C04 unification), both on the C03 plumbing.
2. **Delete C05 outright, or keep it one release behind a dominance gauge?**
3. **The N = 1 unification depends on `received_above` reaching `serve_gaps`,** which ships with C03
   on `snapver-law`. Land C04's unification after C03, or plumb the field separately?
4. **The floor g:** keep 10 ms (UNDERIVED, live at c1) or move to the derived 1 ms kGranularity as
   its own arm with a c1d prediction?
5. **The c1d residual** (17–19 ms of the echo is neither quinn buffer nor wire). The echo form does
   not need it located, but placement and the store do. Diagnose it, with `RWM_RTOBS=2` at c1d
   (owner channel depth, receiver inbound)?
6. **Echo-referenced ripeness (idea only).** The `WindowAck` echoes `batch_send_ts`. A report can
   therefore be dated on the sender's own clock, as "the receiver had seen a batch placed at
   T_e", and ripeness could be stated against T_e instead of `now`. That would remove `B_b` from the
   comparison altogether, but it needs the forward skew F_j − F_b, which is not separable from echoes
   when B_j ≠ B_b. Not derived.
