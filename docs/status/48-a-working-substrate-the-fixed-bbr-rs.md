## 48. A working substrate: the fixed `bbr_rs` against quinn's BBR — host gate, then battery — pre-registration

The user chose §47's option 2: a patched in-tree `bbr_rs` with its own
windowed min-RTT. The law, its provenance and the derivations are in paper
§8.4, "The substrate's window law, fixed". This section fixes the tests, the
battery, the outcomes and the decision rule before any code is written and
before any VM contact. It changes no default: flipping `RWM_QUIC_CC` to
`bbr_rs` is the user's call after the verdict (ADR-0073).

*§47's claims, re-checked from source (no VM).*

| # | §47 claim | reading here |
|---|---|---|
| 1 | Startup growth unbounded (gain vs bytes) | **Verified.** quinn `bbr/mod.rs:326` and `bbr_rs.rs:632`: `cwnd_gain < target_window as f32`. v1 (`tcp_bbr.c` `bbr_set_cwnd`) and Chromium test `cwnd < target`. |
| 2 | min_rtt is the lifetime `rtt.min()`, taken against a per-poll stamp, so it can read below the path | **Verified, with the mechanism made exact.** The endpoint driver stamps `now` once per poll before `recvmmsg` (quinn `endpoint.rs:372`, `:774–799`, budget `RECV_TIME_BOUND` = 50 µs). A datagram that arrives after the stamp reads early by the gap. The gap is unbounded under preemption. The send stamp can only read late (`connection.rs:996`). One early sample latches for the connection (`paths.rs:329–333`; `bbr_rs.rs:733–738`). Also, `on_ack` and `on_end_acks` run before `rtt.update` (`connection/mod.rs` 1505 vs 1522). |
| 3 | The bw sample shares one `now` per poll; is bbr_rs's sampler sound? | **Upstream: worse than stated.** Within one ACK frame every `on_ack` after the first gets `now − prev = 0` → `None` → `ack_rate` 0, so only the frame's first packet is sampled (1 MTU ÷ inter-frame gap), and the max filter is raise-only, i.e. a ratchet. **bbr_rs: sound.** It samples per flight against a send-time snapshot; one shared stamp errs by ≤ the stamp gap over an interval ≥ the guard; the 10-round max decays because every admitted sample reaches `update_max`. Its one weak input is the guard's `rtt.min()`, which inherits claim 2 (floored at 1 ms). Minor: the pipe-restart branch is dead after the first ACK (`split_off` keeps the key, `:246`); this is safe for a max filter and recorded. |
| 4 | quinn paces at window/srtt and ignores `pacing_rate` | **Verified.** The refill is 1.25·W/srtt (`pacing.rs:92`), with bursts of 10–256 MTU. `pacing_rate` reaches only qlog metrics (`paths.rs:198`). |
| 5 | The Drain exit needs in_flight ≤ BDP, which never happens under a backlog | **Partly.** Drain's window keeps gain 2.885 (`bbr_rs.rs:511`), so under a backlog in_flight ≈ 2.885·BDP̂ and the exit is unreachable. It is left at the next ProbeRtt, ≤ 10 s later, whose exit goes to ProbeBw: stuck for at most 10 s, not forever. |
| + | not in §47 | ProbeRtt entry is "10 s since the last ProbeRtt start", with `None` read as expired (`bbr_rs.rs:518–524`, upstream the same). It fires on the first non-app-limited ACK and every 10 s after, which is §47's 10.0 / 20.1 / 30.1 s fingerprint, and it re-reads the same lifetime min. Startup also never exits while app-limited (`check_if_full_bw_reached`, v1's): rows under the store's brake stay in Startup, so §47's "short healthy rows stay in Startup" may mean "app-limited rows". The battery reads this (`qm=`). |

*The design (paper §8.4).*
1. **Ack clock.** a_e = clk() once per ACK event, with r_e = a_e − max s_k.
   Proven ≥ RTprop. clk() is quinn's runtime clock, injectable via
   `BbrRsConfig` for tests.
2. **v1 min filter.** m̂ is the 10 s windowed min of r_e. ProbeRtt is entered
   on the filter's age; on exit, t_m ← a_e.
3. **v1 Startup test:** W < T(2.885) + e.
4. **Drain window:** T(1).
5. **Sampler guard:** max(m̂, 1 ms).

ProbeBw keeps v1's W = T(2) + A; the loop map shows that the window alone
probes. The diag gauges are `qm=`, `qrt_us=`, `qsr_us=` and `qbw=`.

### Step 0 — the host gate (before any VM contact)

*Red first.* The tests are committed on `bbrfix48` before the fix, against
the unchanged controller. The only additions are two test seams that
both trees carry unchanged: an inert clock in `BbrRsConfig`, which the
pre-fix code does not read, and `#[cfg(test)] plant_min_rtt(d)`, which sets
the `min_rtt` field pre-fix and seeds the filter post-fix. Tests 6–9 are
therefore byte-identical across the fix. Their red output is
saved to `docs/l1-raw/bbr48/host-red.txt`. The fix then follows in its own
commit, and the green output goes to `host-green.txt`.

*The simulator* (`raptorpath/src/transport/cc_sim.rs`, `#[cfg(test)]`):
- **Real connections.** A quinn-proto client/server `Endpoint` pair on a
  synthetic `Instant` timeline, using the engine's own config builders
  (`transport/quic.rs` `generate_self_signed_config`,
  `make_client_config`, MTU floor 1350). Both controllers therefore see
  quinn's real `RttEstimator`, ACK framing and pacer.
- **The client source has two phases.**
  - Phase A, 0–1.0 s, is bulk: ≥ 1 MB of 1 279 B datagrams stays queued, so
    the flow is not app-limited and both controllers leave Startup.
  - Phase B, 1.0–3.0 s, supplies 0.9·L in 1 ms chunks. This is the
    store-braked regime: the wire queue drains to ≈ 0, matching the 2 ms
    `wrtt` that §41b read in vivo at c1s.
- **The link (c1):** 1 Gbit/s serialisation, 1 ms each way, a FIFO of 1 000
  packets (netem's default limit), no jitter.
- **Loss burst:** 28 consecutive client datagrams dropped at t = 1.8 s
  (§41b's collapsed row: lost +28 at 1.8 s), inside phase B.
- **Polls.** The client endpoint polls every 50 µs (`RECV_TIME_BOUND`). A
  poll stamped τ hands every datagram that arrived by τ to `handle(τ, …)`.
- **Stale-stamp events.** At t = 1.2 + 0.1·j s, j = 0…17 (18 events in
  phase B, wire queue ≈ 0), one poll's drain instead covers (τ, τ + L_j]
  with stamp τ.
  - L_j cycles through {0.5, 0.8, 1.1, 1.4, 1.7, 1.9, 1.95} ms, all below
    the 2 ms RTT, so no sample saturates to 0.
  - The top values come from §47's fingerprint, which implies a skew of
    1.92–2.0 ms (paper §8.4). The skew distribution and the 10/s rate are
    **declared, unprovenanced**: no record measures the stamp skew, and
    paper §8.5's drain pauses bound stall length, not skew.
  - During such a drain the injected clock reads τ + L_j, the drain's end,
    which is the proof's tight edge.
- **Mechanism witness.** The harness knows every datagram's true arrival
  time. For each stale event it counts ACK-bearing datagrams that arrived
  after τ: their quinn sample, τ − send, reads below the true RTT and
  above 0. A run with zero such datagrams is `VOID-SIM`.
- **Run length:** 3.0 s simulated. The window is sampled every 1 ms;
  BDP_true = 250 000 B.
- **Expected failure path for quinn.** One event at 1.9–1.95 ms latches a
  min of 50–100 µs, giving T(2) ≈ 2 × 125 MB/s × 0.1 ms ≈ 25 KB + A against
  the ≈ 225 KB that phase B carries in flight. That is the §47 collapse.

*Tests and assertions (absolute).*
1. **`cc_sim::bbr_rs_holds_the_pipe_after_a_loss_burst_under_stale_stamps`.**
   Over t ∈ [1.3, 3.0] s:
   - share(W < BDP_true) ≤ 0.05 (the §47 witness, bounded by the ProbeRtt
     and Drain duty cycle);
   - min W ≥ 0.5·BDP_true.

   Witnesses: the controller left Startup in phase A (`qm` ≠ S); the link
   dropped exactly 28; 18 stale events were applied; the mechanism witness
   is > 0.
   **Predicted RED pre-fix:** m̂ = latched `rtt.min()`.
2. **`cc_sim::bbr_rs_min_rtt_never_reads_below_the_path`.** The same run;
   at every sample after the first event, m̂ ≥ 2.000 ms. Predicted RED
   pre-fix. Phase A's bulk queue only raises samples, so it cannot help
   pre-fix.
3. **`cc_sim::bbr_rs_min_rtt_follows_a_path_rtt_rise`.** 10 Mbit/s;
   one-way delay 5 ms, stepped to 20 ms at t = 2 s; 14 s simulated; no stale
   events.
   - m̂ ≥ 36 ms by t = 12.5 s (2 s + W_m + T_R + margin);
   - share(W < 50 000 B) over [13, 14] s = 0.

   Predicted RED pre-fix (the lifetime min holds 10 ms).
4. **`cc_sim::quinn_bbr_window_collapses_under_stale_stamps`.** quinn's
   `BbrConfig` on test 1's scenario; asserts share(W < BDP_true) > 0.5.
   This pins the upstream defect: if a quinn bump turns it red, ask again.
   It also prints, reported only, the same run without stale events and
   the latched `min_rtt` from quinn's `Debug`. That separates the min-RTT
   term from the bandwidth term, which is §47's open item.
5. **`cc_sim::quinn_bbr_min_rtt_under_reads_under_stale_stamps`.** quinn's
   `min_rtt` (from `Debug`) ∈ (0, 1.0 ms) after the events. This is the
   mechanism realised in quinn's own estimator.
6. **`bbr_rs::tests::startup_window_is_bounded_by_the_model_target`
   (unit).** b̂ = 12 MB/s and m̂ = 10 ms planted; 50 rounds of ACK events
   flagged app-limited, so the full-bandwidth check is skipped and the
   controller stays in Startup (the store-braked regime). Asserts
   W ≤ T(2.885) + max e + one event's acked bytes. Predicted RED pre-fix
   (W ≈ the accumulated acked bytes).
7. **`bbr_rs::tests::drain_exits_under_a_backlog` (unit).** Full bandwidth
   is reached with in_flight held at W (backlog). The test reaches ProbeBw
   within 3 m̂ of entering Drain, with no ProbeRtt in between. Predicted RED
   pre-fix (no exit until the ProbeRtt at ≤ 10 s).
8. **`bbr_rs::tests::probe_rtt_fires_only_on_an_expired_min_filter`
   (unit).**
   - no ProbeRtt on the first non-app-limited ACK event;
   - none while r_e ≤ m̂ refreshes t_m every second for 30 s;
   - exactly one at t_m + 10 s once refreshes stop.

   Predicted RED pre-fix (ProbeRtt at the first event).
9. **The kept tests stay green, edited only to plant m̂ through the filter:**
   the estimator law tests and
   `sustained_loss_cannot_clamp_recovery_below_the_pipe`.
10. **Gauge wiring (loopback, `RWM_QUIC_CC=bbr_rs`):** the `[DIAG]` path
    block carries `qm=` in {S, D, B, R}, and numeric `qrt_us=`, `qsr_us=`
    and `qbw=`. Under the default substrate it carries `qm=-`. This proves
    the gauges read the running controller (measurement discipline rule 1).

*Gate rule.*
- **`GATE-0 PASS`** iff:
  - tests 1, 2, 3, 6, 7 and 8 are red pre-fix and green post-fix;
  - tests 4 and 5 are green on both trees;
  - tests 9 and 10 are green post-fix;
  - the full suites are green post-fix: release
    `-p raptorpath -p raptorpath-math`, `--doc`, `raptorpath-wasm`, debug
    `-p raptorpath`, and the parser tests (`FLAKE` by a solo re-run, as
    §27).
- **`VOID-SIM`** if the mechanism witness is 0 or test 5 is red: the
  simulator did not realise a stamp that reads early without saturating.
  This is a harness fault and says nothing about §47. Fix the harness
  (never the L_j grid toward red) and re-run.
- **`STOP-NOT-REPRODUCED`** if the mechanism is realised (witness > 0,
  test 5 green) but test 4 is red or test 1 or 2 is green pre-fix. An early
  stamp then does not collapse the window, and §47's inferred cause is
  refuted in simulation. Report; no VM.
- **`STOP-FIX`** if any post-fix test is red after one focused fix attempt.
- Local cargo only under the host mutex.

### Step 1 — the battery (only after `GATE-0 PASS`)

*Question.* As the substrate under the default store law, does the fixed
`bbr_rs` keep quinn's window at or above the true BDP (the §47 witness)
without being worse than main on any §21 clause beyond what two runs of
main differ by?

*Trees and arms.*
- **BASE:** main's binary at launch. Its sha is recorded, and quinn's BBR
  is the default.
- **CAND:** the same main plus the §48 commits only (`bbr_rs.rs`,
  `cc_sim.rs`, the clock seam, the `[DIAG]` gauges), run with
  `RWM_QUIC_CC=bbr_rs`. The default is not flipped in the measured tree.
- **Pre-launch check:** `git diff --stat BASE CAND -- raptorpath/src` lists
  only those files. Each tree is built fresh in its own `git archive`
  (`core.autocrlf=false`) with its own target dir. The binaries run from
  `/dev/shm`.

*Design.*
- **Cells:** c1s-400, c1d-400, c2-100 and c8-100, with the default store
  law, Bulk, `--window-reliable` and the env of §41b. Sizes are chosen so
  that every row runs ≥ 30 s; each such row contains ≥ 2 ProbeRtt
  opportunities and, where its loss process allows, a non-app-limited
  round:
  - c1s and c1d: 5 GB (≈ 48–62 s);
  - c2 and c8: 600 MB (≈ 53–60 s).
- **Rows:**
  - BASE n = 12 per cell (seeds 42 and 7 × 6 reps);
  - CAND n = 6 per cell (seeds 42 and 7 × 3);
  - 72 rows plus a 2-row smoke (one per arm, c1s).
- **Interleave:** arms interleave per block, rotated as in §41b's plan and
  pinned by the harness test. Cell order per block: c2, c1s, c8, c1d.
- **Harness:** plan 48 in `loopdiag_*`; scorer `tools/l1/bbr48_score.py`
  (§41b's `place41b_score.py` statistic generalised to four cells); parser
  test `test_bbr48_parse.py`, covering both `[DIAG]` shapes (with and
  without the `q*` gauges).
- **Dry run before launch:** the scorer runs on §41b's BASE rows and must
  reproduce §41b's c1s and c1d resolution table.
- **Session rules, carried from §41b:**
  - precondition and both locks (`test -e`);
  - root-owned tmpfs, `ABORT-BIN`;
  - `LD_HARD` = launch + 4 h; session ≤ 5 h.
- **Suites:** the CAND tree's suites run in Step 0, and Step 1 reuses them
  only if `raptorpath/src` is byte-identical (sha256 of every file);
  otherwise they are re-run before the battery.

*Witnesses (row void otherwise).*
- `[GATES]` and the arm's substrate echo:
  - BASE: the stock-BBR echo, and no `qm=` field (main's binary has none);
  - CAND: the `burst-robust BBR` echo, and `qm=` in {S, D, B, R} on every
    path block.
- ≥ 20 s of post-warm-up `[DIAG]` lines.
- The row ran (rc 0, bring-up ok). **A DNF (`RWM_PERF_TIMEOUT_S` = 200)
  is not void; it is scored per arm:**
  - a CAND DNF fails the tail clause, and its lines enter R2;
  - a BASE DNF is reported as collapse evidence.

  DNF rows are excluded from R1's medians only, as in §41b, and are also
  classified as in the §43 note (`ENGINE-STALL`).

*Readouts (fixed now).*
- **R1, the guard.** §41b's statistic exactly, with the RNG seed 4141 and
  10 000 relabellings:
  - **Clauses,** per cell: goodput, client CPU/GB, server CPU/GB,
    `lag_E_cli`, `lag_E_srv`, and `rtp_floor` per leg.
  - **Pooling:** 12 BASE + 6 CAND rows per cell; each relabelling takes
    3 + 3 per seed.
  - **Statistic:** T = max z over every clause of all four cells;
    **PASS iff p > 0.05.**
  - **Second statistic,** registered now for the cost outcome: T_rest,
    which excludes the goodput clauses of c2 and c8, with its own p_rest.
  - **Printed first,** from BASE alone: the resolution table (q95 of |x|
    per clause, beside §21's tolerance).
  - **Also reported:** the tail line (each arm's lowest row against its own
    cell median) and every clause's z.
- **R2, the §47 witness.** Per row and active leg, where a leg is active if
  its `qlp` sent counter advanced since the previous line. BDP_leg =
  250 000 B for a c1 leg, 125 000 B for c2 and 100 000 B for c3.
  - **R2a, reported for both arms (§47's form):** the share of `[DIAG]`
    lines at t ≥ 1 s with `qcwnd` < BDP_leg. The fixed law's window is
    g·b̂·m̂, so R2a ≈ 0 is not implied where b̂/L or m̂/R_nom is below 1:
    §27 read b̂/L 0.66–0.81 at c2/c8 and from 0.50 at c1d. R2a is not
    scored.
  - **R2b, scored (the collapse):** the share of lines at t ≥ 1 s with
    `qcwnd` < 0.5·BDP_leg **and** the leg's B = `hand − tx` ≥ 98 datagrams.
    That is a window far below the link while quinn holds a backlog. A
    trickle leg (app-limited, B ≈ 0) or a window above the carried
    in-flight does not count. §47's collapsed row reads `qcwnd` 0.023·BDP
    with B ≈ 2 000.
- **R3, authority and delay.** Per leg, the row medians of:
  - B = `hand − tx`;
  - `wrtt` (quinn's wire srtt);
  - A_x, the app-echo `rtt`.

  Means over rows per arm and cell.
- **R4, CAND bind gauges.**
  - `qm` occupancy (S, D, B, R share of lines);
  - median `qrt_us` against the netem base (2 ms c1, 10 ms c2 with a
    measured floor ≈ 7.0 ms, 40 ms c3); the paper's claim requires
    qrt_us ≥ the base minus jitter;
  - the loop margin 2·qrt_us/qsr_us, the paper's stability term.

*Outcomes and decision rule (applied in this order).*
1. **`NOT-CONTROLLED`** if, at any cell, the median over CAND rows of R2b
   exceeds 0.05 (ProbeRtt's duty, ≤ 0.03, sits below it), or if any CAND row
   has R2b > 0.25. The fix does not hold the pipe in vivo. Stop; no flip;
   diagnose from R4.
2. **`PROMOTE-ELIGIBLE`** if R1 PASSes and, at every cell, CAND's lowest
   row is ≥ 0.8 × CAND's cell median.
3. **`CONTROLLED-WITH-COST`** if R1 fails but p_rest > 0.05, and all of the
   following hold. It is a measured trade, to be ruled on by the user:
   - CAND's goodput median is ≥ 0.90 × BASE's at c2 and at c8;
   - CAND's `wrtt` p50 is ≤ 0.7 × BASE's at c2 and on c8's slow leg;
   - the tail clause of 2 holds.
4. **`REGRESSION`** otherwise.

*What follows.*
- **After `PROMOTE-ELIGIBLE` or `CONTROLLED-WITH-COST`,** the user makes the
  ADR-0073 call on `RWM_QUIC_CC`. Promotion means:
  - the default flips to `bbr_rs`, through a new ADR that supersedes
    ADR-0054's default;
  - by ADR-0073, quinn's `bbr` path is deleted in the same merge, unless
    the user rules to keep it as a reference arm.
- **After `REGRESSION` or `NOT-CONTROLLED`,** the user chooses: archive
  (`archive/bbr-rs`, a paper §10 row, re-ask "when quinn's trait passes a
  receive timestamp or honours `pacing_rate`") or §47's option 1 or 3.
- **§43b's c1s τ test** stays blocked unless the substrate is promoted.

*Predictions (before any number).* Each prediction comes from the paper's
loop map and authority rule. The numbers are model estimates, not
measurements.

| cell | substrate window W_q = 2·b̂·m̂ + A | authority | goodput | delay gauges | §47 witness (CAND) |
|---|---|---|---|---|---|
| c1s | ≈ 2 × 103 MB/s × 2.0–2.3 ms ≈ 410–470 KB, above the wire in-flight (≈ 210–260 KB at 2–2.5 ms) | store; ΔB ≤ 98 | Δ median within 3.8 %; the tail improves (no row < 0.8× median) | `wrtt`, A_x: same | R2b ≤ 0.02; R2a ≤ 0.05; `qm` mostly S (app-limited Startup) |
| c1d | the same per active leg; an idle leg keeps `init_cwnd` as BASE does | store | within 6.4 % | same | R2b ≤ 0.02; R2a reported (b̂/L from 0.5 on a light leg) |
| c2 | ≈ 2 × (11.3–12.2 MB/s) × m̂ + A, with m̂ between 7.0 ms and the wire floor of 10–12 ms (§43b SL rows `rtp_us` p50 10.2–11.8 ms): ≈ 170–300 KB (1.4–2.4 BDP); margin 1.1–1.8 | **quinn takes the wire** (the shipped law pins 2 048): B_CAND ≫ B_BASE, > 500 | [0.92, 1.00] × BASE: masking risk (ADR-0054 addendum −4…−14 % at GE cells), recovery conservation at 2.6 % GE, and the thin margin at the low end of m̂ | `wrtt` p50 ≤ 30 ms against BASE 95–137 ms (§43b MAIN rows' p50); A_x within ±15 % (≈ n/x; the dwell moves into B) | R2b ≤ 0.03; R2a reported |
| c8 | p0 as c2; p1 ≈ 2 × 2.4 MB/s × 30–40 ms + A ≈ 150–200 KB; margin 1.3–2 | quinn takes the slow leg's wire | [0.90, 1.00] × BASE | p1 `wrtt` p50 ≤ 0.6 × BASE (BASE over-reads ×9.1, §24; its slow-path queue 88–124 ms, ADR-0054); `lag_E` same or better | R2b ≤ 0.03; R2a reported |

- **Expected label:** `CONTROLLED-WITH-COST`, failing R1 on c2/c8 goodput,
  with `PROMOTE-ELIGIBLE` second.
- **R4:** median `qrt_us` 2.0–2.5 ms at c1, 7–12 ms at c2, 30–40 ms on c3.
  Loop margin ≥ 1.3 at c1 and on c3, and 1.1–1.8 at c2; this gauge settles
  which end holds.
- **BASE witness:** a few percent on healthy rows, most lines on any
  collapsed row (§47).

*Stated risks.*
- **Host delay at c1s.** m̂ and qsr include the connection task's
  processing delay. If the typical delay exceeds about 2 ms at c1s, the
  margin falls below 1 and CAND contracts there. R4 reads this directly.
- **c2 is thin.** Its margin (1.1–1.8) is the thinnest, because its m̂ is
  a jitter floor. At the low end, recovery rounds can under-fill the
  link.
- **Placement at c8** reads quinn's srtt (`RWM_PLACE_WIRE`, promoted), so
  CAND's lower p1 wire RTT may shift the split. That is reported, not
  scored.

*Duration.*
- Step 0: host builds plus suites ≈ 2–3 h under the mutex.
- Step 1: two builds (≈ 11 min) and 74 invocations (≈ 80 min), inside
  launch + 4 h.


### 48. Amendment 2 — the guard, the cost label, the trees and the host gate (before any VM contact; advisor review)

- **Guard.** §41b's statistic misses +15 % CPU at 26 clauses (its z
  saturates near 3; see the guard rework under way). §48 uses whichever
  guard statistic passes the acceptance table (one separated clause gives
  p ≤ 0.01; A/A false-fail ≈ 5 %; a per-clause power table), registered as
  §41c. §41b's statistic is not used.
- **The cost label.** `CONTROLLED-WITH-COST` also reports A_x
  (delivered-data delay) beside wire RTT. The rule states what that outcome
  buys and what it does not:
  - the c1 collapse is gone, and the substrate actually limits;
  - delivered delay at c2 is predicted unchanged (the queue moves from the
    link into quinn's send buffer);
  - goodput is 0–10 % lower at c2 and c8.
- **Trees.** The battery runs after the `diag36` merge decision. BASE =
  main at that time; CAND = the §48 commits rebased on it. Both are pinned
  by SHA. The promoted placement price reads quinn's SRTT, which this fix
  moves.
- **Added to the host gate.**
  - A **jitter scenario** in `cc_sim`: netem's release rule with c2's delays
    (5 ms ± 3 ms per direction) across one ProbeRtt. By the loop map, once
    2·m̂ < R̄₀ the estimate settles at A/(R̄₀ − 2·m̂). If b̂ decays there,
    the law needs a jitter allowance before any VM time. It is the same
    defect as the store law's brake bias.
  - **Quinn's `rtt.min()`** is logged beside m̂ on CAND rows. A reading
    below 2 ms at c1s confirms §47's inferred cause.
  - **The new gauges go on their own log line** (`[QCC]`), not the pinned
    per-path `[DIAG]` format. BASE rows simply lack it.
  - **`congestion_state()` is read only at the DIAG tick.** Its per-call
    cost is measured on the host (§38 failed on 360 ns per call).
  - **Datagram evictions per row** (`dgq full=`, `dgev`) are reported. With
    quinn limiting, the store's backlog sits in its 4 MiB buffer, and an
    eviction reads as loss.
- **ProbeRtt is a periodic drain** of up to 200 ms every 10 s. It sits in
  the substrate, not in our laws, and the user is told so.
- **After §48, if it controls:** three reported rows at c2 with §43b's law
  over the fixed substrate, where the two loops first meet (≈ 150 KB in
  flight against a 170–300 KB window).
