# DRAFT: the hand-off gate on `bbr_rs` against main, in two steps (pre-registration draft)

**Status.** A draft for the coordinator to register as status sections. No row has run, and nothing in it is built.
- It replaces the §48d draft (`reg-48d-draft.md` on `w1r2`), which does not run as a scored session (`sender-queue-law.md` §1.4, §6).
- Rulings (coordinator and Fable, 2026-11-26):
  - The promotion candidate is `bbrrs-r4` plus the hand-off gate.
  - The session runs in two steps: a reported-only diagnostic first, then a scored two-arm session.
  - The δ seat stays open, so the gate is δ-free and the store law is unchanged.
  - Reading cwnd and in-flight is allowed for the diagnostic, behind a default-off environment arm (Q-S10).
  - Promotion at ρ < 1 is blocked on the three-knob lifetime (Q-S11).
- The law is paper §6.8, revision 4. L uses realized timer waits, and the repair clamp is the last-turn predicate. Its derivation and the §48b evidence are in `docs/derivations/sender-queue-law.md`.

## 1. Arms and trees

| arm | tree | env | role |
|---|---|---|---|
| **BASE** | step 1: main at launch plus the gauge commit (§2 item 1, `RWM_DIAG`-gated, no law input), as a session tree pinned by amendment with both shas. Step 2: **unmodified main** at launch, so the verdict's BASE carries no new code | default (quinn's BBR) | baseline |
| **R4** | `bbrrs-r4` (`fa9947dd`: §48 window law, `[QCC]`, one-round ProbeRtt, send-record ring), merged with BASE, plus the gate commit (§2 item 2) | `RWM_QUIC_CC=bbr_rs`, the gate's arm **off** | `bbr_rs` alone. Step 1 only |
| **GATE** | the same binary as R4 | `RWM_QUIC_CC=bbr_rs`, the gate's arm **on** | the candidate |

- R4 and GATE share one binary, so the gate's effect is one environment arm wide.
  - **Behind the arm:** the gate, the owner's republish timer, the poll arming on the gate's pause, the `RouteProbe` cwnd atomic (the law's input) and the predicate clamp.
  - With the arm off, R4 runs `bbrrs-r4`'s code paths plus one branch on the arm per producer turn and per owner poll. Its CPU differs from `bbrrs-r4` only by that branch, which the host ns bench bounds before launch.
  - The `[HOFF]` gauges are `RWM_DIAG`-gated, as every row runs with `RWM_DIAG=1`. They cost equally in every step-1 arm.
- **Pre-launch diff checks.**
  - `git diff --stat BASE R4tree -- raptorpath/src` lists §48b's file list plus the gate's files.
  - With the arm off, R4's decision traces equal `bbrrs-r4`'s, byte for byte, on the host sims. That is the arm's identity check.
- **No pacing seam** in any arm. `RWM_BBR_PROBERTT_ROUNDS` is unset.

## 2. What must be built first (each its own commit, red first where marked)

1. **Gauges, reported only, in every arm**, on a new `[HOFF]` line at the `[DIAG]` cadence.
   - **The hand-off backlog B_i:** the producer's staged count minus quinn's `tx`. Printed as p50, p95 and max, beside quinn's queue depth (`hand − tx`).
   - **The gate's inputs:** β_i, R_i, L_i and its four terms: τ̂_o and τ̂_p, the realized timer waits as p99, with their floor binds; ℓ̂_o p99, which is new, since `[IOWN]` has only a mean; and ℓ̂_p p99.
   - **`owner_timer_wakes`,** and how each producer turn for a path ended (dry, or gate closed): the predicate's input.
   - **Quinn's in-flight F_i and window cwnd_i** (for `bbr_rs`, min(cwnd, w_rec)), as gauges from the `RouteProbe` wrapper's `on_end_acks` and the inner `window()`. They exist under quinn's BBR too. A read counter shows that `congestion_state()` is not used.
   - **Its `app_limited` share,** also from `RouteProbe`.
   - **`infl_vs_out`:** the engine's `in_flight` against Σ(F_i/s + B_i).
   - **The rest:** `fifo_wait_resend_i`, `reopen_i`, `starve_i`, `batch_i`, `hand_bind_i`, `spare_bind_i`, `store_pin`.
   - **A `gate=on/off` token,** the execution witness.
2. **The gate commit** (paper §6.8), behind its default-off environment arm, which is an experiment arm and not a dial. It contains:
   - first-send admission while B_i < β_i, at every hint and ρ. Control, resends and repair are exempt from the bound and count in B_i;
   - the 1 ms backpressure poll armed on the gate's pause at every ρ, not only under `tx_paused = reliable && …`;
   - the owner's republish every VIEW_REFRESH while its last published queue was non-empty;
   - the clamp's predicate at the existing `min(r, spare)` seats (`emit_source.rs` cadence, `fec_rate.rs` taper grant). spare_i is 0 if path i refused a first send in the last completed turn, else +∞. It is held through the turn, and store-full turns leave it unchanged. `Scheduler::spare_capacity` changes from the min to the max over active paths, and repair placement reads spare_i.
   - **Not in it:** placement's load term, any store-law or δ change, any ρ < 1 law.
3. **Red-first tests,** with the absolute anchors of `sender-queue-law.md` §7:
   - gate decision: β = 53.83 (hand at 53, not at 54); second leg 12.29 (hand at 12, not at 13); cold start 27.25;
   - no dial is read;
   - the predicate: 250 repairs per 1 000 dry turns, 0 when the gate closes; at N = 2, 250 repairs, all on the dry leg;
   - the two-leg burst;
   - the resend's wait;
   - **β bounded across a 1.5 s silence:** [48.4, 59.2], against ≈ 19 000 on the measured-interval form;
   - **reopen without inbound traffic:** within 2.2 ms of virtual time, against ≥ 100 ms on today's owner. This is a logic test; the realized L is read on the VM;
   - **repair at β < 1:** exactly 250 repairs per 1 000 sources;
   - **the post-outage drain witness** in the L0 netem loopback: `spare_bind` ≥ 0.5 and drain coded share ≤ 0.2 × pre-outage;
   - **the arm-off identity.**
4. **Suites** on the R4/GATE binary, as §48b item 5. The host gate `GATE-HO PASS` requires every item-3 test green, and every red-first test red on the pre-gate tree, before any VM contact.

## 3. Step 1: diagnostic (reported only, no label)

| cell | hint | size | rows per arm |
|---|---|---|---|
| c2-100 | Bulk | 600 MB | 3 |
| c8-100 | Bulk | 600 MB | 3 |
| c2-100 | Auto | 600 MB | 3 |
| c2-100, low rate | Realtime, ρ = 1 (`window_reliable: true` witnessed), the source paced at a call rate (≈ 36 datagrams/s) with §51 Block E's pacing mechanism | 60 s | 3 |
| c2-100, paced (headroom) | Auto, the source paced at ≈ 70 % of the link, with the same mechanism | 600 MB or 60 s | 3 |
| c2-100, bursty (headroom) | Realtime, ρ = 1, frames of ≈ 10 datagrams every 33 ms with the same mechanism (or `UNBUILT`) | 60 s | 3 |

- **Rows.** 6 cells × 3 arms × 3 rows (seed 42 × 3) = 54, interleaved.
  - If the harness has no source pacing, the three paced cells read `UNBUILT`. The host tests of §2 (repair at β < 1, and the predicate) are then the only witnesses, and step 2 waits for the owner.
- **Cost.** About 50 min of rows plus builds and suites.

**What step 1 reads, with a rule before step 2 for each.**

| item | reading | before step 2 |
|---|---|---|
| **c8 server CPU** (predicted failure) | `cpu_srv/GB` GATE/BASE and R4/BASE; `batch_i` and tx batches per leg; `[IOWN]` per-leg polls and drains | GATE/BASE > 1.03, or `batch_i` < 0.9 × R4's on a leg: diagnose (finer I/O from admission, or the window's pacer bursts as in §48b), then fix or register the cost as priced, with its own number |
| **c2 goodput** (predicted failure) | goodput GATE/BASE and R4/BASE; `starve_i`; `reopen_i` (p99 ≤ L); τ̂_o measured against its assumed 2 ms; ProbeRtt counters (`qrn`, `qrus`, `qrb`, `qab`); `app_limited_i` | GATE/BASE < 0.995: attribute it to starvation, reopen latency, ProbeRtt or the unattributed rest, then fix or price it |
| **Gate executes** | `gate=on` on GATE rows; `hand_bind` > 0.5 at c2 Bulk; B p95 ≤ β + one batch; RouteProbe reads > 0 and `congestion_state()` reads = 0 | any failure: harness or build fault; stop |
| **Clamp at Auto** | `spare_bind`; coded share GATE against R4 | reported; a coded share on GATE ≥ R4's while `hand_bind` > 0.5 means the clamp is inert, so stop |
| **Clamp with headroom** (paced cell) | `spare_bind` on GATE; coded share GATE against R4 | `spare_bind` > 0.05, or a coded share outside R4's ± A/A: the clamp binds with headroom, so stop |
| **Bursts with headroom** (bursty cell; the registered limit of §6.8) | frame completion time GATE against R4; repairs per source; `hand_bind` | completion + L·(frame/β − 1) or more, or repairs per source < 0.9 r: the limit binds. β then takes quinn's bucket term (re-derived and re-reviewed) before step 2 |
| **Low rate** | repairs per source against the rate law's r (from the `[DIAG]` rate echo); B max; first-send delay against R4 | repairs per source outside r ± 5 % or B max > 2: the β < 1 handling fails, so stop |
| **§8.4 step in time** | β and spare across ProbeRtt episodes (`qm = R`) | reported, for the amendment decision |

**Step 1 predictions (ratios against BASE).**

| cell | readout | R4 / BASE | GATE / BASE |
|---|---|---|---|
| c2 Bulk | goodput | 0.987–0.995 | 0.985–0.998 |
| c2 Bulk | A_x | 0.97–1.04 (≈ 206 ms) | 0.13–0.18 (≈ 28–37 ms) |
| c2 Bulk | in-order delay mean | 0.98–1.06 | 0.15–0.40 |
| c2 Bulk | cpu_cli/GB, cpu_srv/GB | 0.96–1.00, 0.94–1.00 | 0.96–1.04, 0.94–1.04 |
| c8 Bulk | goodput | 0.974–0.988 | 0.985–1.05 |
| c8 Bulk | A_x fast; slow | 0.55–0.80; 0.90–1.15 | 0.13–0.20; 0.36–0.47 |
| c8 Bulk | in-order delay mean | 1.30–1.60 | 0.30–0.90 |
| c8 Bulk | cpu_srv/GB | 1.035–1.065 | 1.03–1.07 (β ≥ 2.3 × batch, so admission is not expected to make I/O finer) |
| c2 Auto | coded share | as BASE ± 30 % | ≤ 0.3 × R4's |
| low rate (Realtime ρ = 1) | repairs per source | = r | = r (±5 %); B max ≤ 2 |
| paced (headroom) | `spare_bind`; coded share | — | ≤ 0.05; within A/A of R4's |
| bursty (headroom) | frame completion; repairs per source | — | the limit is predicted to bind in ProbeBw (β ≈ 3 against a 10-datagram frame): + 2 L to + 3 L per frame and repairs cut on most frames |

With n = 3 these are directional readings, not tests.

## 4. Step 2: the scored session, two arms (GATE against BASE)

Step 2 is registered only after step 1's rules are cleared, and with any priced cost written in.

| stage | cells | rows | role |
|---|---|---|---|
| **G, the guard** | plan 41d exactly: c1s and c1d at guard size, c2 and c8 at 600 MB, Bulk, `--window-reliable`, 12 + 12, rank-minp v2, 10 000 relabellings, seed 4141, PASS iff p > 0.05 | 96 | scored |
| **A, Auto** | c2 and c8 at 600 MB | 12 + 12 per cell, 48 | reported. Witnesses: cod > 0 on BASE (`VOID-NOREPAIR` otherwise); `hand_bind` > 0 on GATE (`VOID-INERT` otherwise) |
| **R, Realtime** | c2 at 100 MB, ρ = 1 witnessed by `window_reliable: true` on both ends (plan 64's stage R pattern) | 12 + 12, 24 | reported |
| **L, low rate** | step 1's low-rate cell (Realtime ρ = 1) | 6 + 6, 12 | reported. A witness of the β < 1 handling |
| **P, paced headroom** | step 1's paced cell | 6 + 6, 12 | reported. A witness that the clamp does not bind with headroom |
| **B, bursty** | step 1's bursty cell, if the burst limit is resolved | 6 + 6, 12 | reported |

- **ρ < 1 is not run.** Promotion there is blocked on the three-knob lifetime (Q-S11).
- **Post-outage drain.** The VM harness cannot cut a link, so the three-knob verdict's witness (`spare_bind` in a post-outage drain with repair on) is the host L0 netem test of §2, re-run on the pinned binary. A VM stage is added if the harness gains a link cut.
- **Cost.** About 3.2 h of rows. Set `LD_HARD` at launch + 5 h.

**Step 2 predictions.**

| cell | scored clause (GATE / BASE) | band |
|---|---|---|
| c1s | goodput; cpu_cli/GB; cpu_srv/GB | 0.97–1.03; 0.95–1.01; 0.96–1.02 (the gate rarely binds at c1s) |
| c1d | goodput | 0.98–1.06 |
| c2 | goodput; cpu_cli/GB; cpu_srv/GB; in-order delay mean | 0.985–0.998; 0.96–1.04; 0.94–1.04; 0.15–0.40 |
| c8 | goodput; cpu_cli/GB; cpu_srv/GB; in-order delay mean | 0.985–1.05; 1.00–1.035; 1.03–1.07; 0.30–0.90 |

- **Reported:** A_x per leg as in step 1 (c1s ≈ R4's, since β ≈ 920 exceeds its queue of 715 there); wire RTT 0.17–0.29 at c2, 0.17–0.28 and 0.35–0.45 on c8's legs, 1.15–1.60 at c1s; stage A's coded share ≤ 0.3 × BASE's under saturation; stage R's A_x 0.12–0.20.
- **The label, as predicted now:** PASS ≈ 0.35, through c8 `cpu_srv/GB` or c2 goodput. Step 1 exists to move this before scoring.
- **Witnesses and bind gauges** are as in step 1, with the same stop rules.
- **Saturating transfers off Bulk run without proactive repair under the predicate.** Stage A's coded share ≤ 0.3 × BASE's is that effect, and it is a deliberate change, not a fault.

## 5. Decision rule (step 2)

1. **NO VERDICT** if `GATE-HO` failed, a stage is incomplete, or more than 4 witness-voided slots occur in a stage.
2. **Stage G PASS**, with no step-1 stop rule firing in step 2 and stage L's witness holding: the merge question goes to the owner. The question is GATE as the new default (`bbr_rs` with the gate on), by an ADR superseding ADR-0054's default and amending paper §8.4. Promotion holds at ρ = 1 only.
3. **Anything else** goes to the owner, with step 1's R4 readings beside it.
4. **Nothing merges automatically,** and nothing merges on stage G alone (`meas-guard-bulk-blind`).

## 6. Open before registration

- **Harness work:** the plan number, the `[HOFF]` parser, step 1's three-arm interleave, stage L's pacing, and step 2's stage A and L witnesses.
- **The owner's acceptance of the §8.4 amendment** before step 2.
- **The A/A resolution** of the new readouts (A_x, `[HOFF]`).

## 7. Step 1 result and Amendment 1 (2026-11-27, post-data)

**Step 1 ran** as plan 68 on the VM: 27 of 54 rows (K2, K8, A2 at 3 rows per arm; LR, PC, BU unbuilt, because the pacer was not in either tree). Ledger in `docs/l1-raw/hoff68/`. Reported only. Readouts, GATE against BASE (medians, n = 3, one seed):

| cell | in-order delay | goodput | client CPU/GB | server CPU/GB | coded share |
|---|---|---|---|---|---|
| c2 Bulk | 344 → 41 ms | −1.0 % | +7 % | +11 % | 0.004 → 0.004 |
| c8 Bulk | 4.23 → 0.12 s | +10.1 % | +7 % | −16 % | 0.005 → 0.004 |
| c2 Auto | 392 → 45 ms | +17.5 % | −23 % | −20 % | 0.064 → 0.005 |

- **R4 alone** (`bbr_rs`, gate off) is within ±1.5 % of BASE on goodput at every cell; c8 in-order delay is +45 %.
- **Auto's goodput gain is repair switched off.** The predicate cuts proactive repair on a saturated path (§6.8, the registered "saturating transfers lose proactive repair"). So GATE at Auto equals GATE at Bulk (89.6 against 89.4 Mbit).
- **Execution witness:** 721 620 first-send refusals on GATE rows; `gate=on` and the `[GATES]` echo are 1 on both ends.
- **Reopen latency:** the maximum is 17–56 ms against L ≈ 6.2 ms, not within L as §6.8 claims. Open.
- **DIAG68 items** (registered "diagnose before step 2"):
  - c2 Bulk goodput GATE/BASE 0.990 < 0.995;
  - c8 leg-0 batch 16.7 < 0.9 × R4's 25.7.

**The stop rule "B p95 ≤ β + one batch" fired on all 9 GATE rows.** The excess is 75–170 datagrams, in 16–30 % of 0.25 s windows. The worst windows sit near t ≈ x0.3 s, where β (one end-of-window sample) is back near 100, quinn's queue `dgq` holds about 90, and B p95 reads up to 285. The current gauge cannot separate the four candidate causes:
- exempt traffic;
- β moving within the window;
- first sends past the bound (the bypass paths, or `admits` at B = 0 or an undefined β);
- an owner stall (staged but not yet handed to quinn).

Step 2 does not run on this.

**Amendment 1** (post-data, so labelled):
- **(a)** The B-over-β line is reported, not a stop. As worded, it compares a window p95 against one end sample of a moving bound, and would fire on every step-2 row.
- **(b)** The law's own witness replaces it: `fs_over`, first sends staged while B ≥ β, must be 0 on every GATE row (`STOP68` otherwise). The bypass paths are counted separately (`fs_bypass`). The exempt part of the excess, β p5/p50/p95, the owner's longest drain gap and the staged-but-unhanded maximum are reported beside it.
- **(c)** A rerun, plan 68r, carries these counters on the 9 GATE rows plus LR and PC at three arms, with the pacer restored in both trees (4ee1df9b ported).
- **(d)** Step 2 waits for 68r. If 68r shows `fs_over` > 0, the gate is at fault and is fixed before step 2. If the excess is exempt traffic or an owner stall, it goes to the derivation (§6.8's bound on exempt traffic) before step 2.

## 8. Plan 68r result (2026-11-27, post-data, reported only)

**Ran** on the VM: 36 of 36 rows, n = 3, seed 42, `stops=20`. CAND `5fd66615` (gate tree `269c11fe`, witness counters + pacer), BASE `ac46d5f1` (`4b245251` + pacer). Pre-flight: gate-tree build, release lib, loopback, `sim_determinism` and `sim_cross_os` green; gate-off sim traces identical to `f80715eb` (0 of 14 differ). Ledger and report in `docs/l1-raw/hoff68r/`.

**New arm Q** ("gate alone": the CAND binary on quinn's BBR, `RWM_HOFF_GATE=1`) separates the two changes that GATE against BASE bundles. On the K cells, BASE and R4 come from the plan-68 ledger (older binaries, cross-session); GATE's repeat is the drift witness (goodput within 1 %, K2 delay 41 → 49 ms). GATE against Q is the clean within-session pair.

| cell | in-order mean, BASE / Q / GATE | GATE p99 / max | goodput Q/BASE, GATE/BASE |
|---|---|---|---|
| c2 Bulk (K2) | 344 / 239 / 49 ms | 118 / 217 ms | 0.998, 0.994 |
| c8 Bulk (K8) | 4 233 / 1 491 / 123 ms | 293 / 445 ms | 1.073, 1.103 |
| c2 Auto (A2) | 392 / 199 / 46 ms | 110 / 248 ms | 1.151, 1.177 |

- **Attribution:** the gate alone removes 30–65 % of the delay and most of the goodput gain; the delay reaches 40–120 ms only with `bbr_rs` and the gate together. "The gain comes from the gate" stays withdrawn; both are needed. K2 goodput differences are inside cross-session drift.
- **CPU:** GATE and Q both pay +5–13 % CPU per GB on K2 and K8 client. Part of it is the new witness (a view read per first send), so it is not booked to the gate law.

**Auto with headroom (PC, Auto paced at 70 % of the link):** repair comes back. Coded share GATE 0.133 against BASE 0.136, `spare_bind` 0.018, goodput equal, in-order p99 40.5 against 41.8 ms, max 70 against 182 ms. The report's own label for the coded-share reading is A/A-UNRESOLVED. 70–100 % load is unmeasured.
- **Ledger verdict (CLAUDE.md, pinned clamp):** on a saturated path the predicate pins repair off (`spare_bind` 0.99), and GATE at Auto equals GATE at Bulk on every readout (A2 and K2 both 89.8 Mbit). Measured range: the clamp is pinned at 100 % offered load and nearly free at 70 %. With the gate, δ has no effect between Auto and Bulk on a full link. That is the predicate's registered behaviour (§6.8), recorded here as a finding with its range, not a footnote.
- **Worst-case delay:** BA2 has no percentiles (old ledger); GA2's max (248 ms) is below BA2's mean (392 ms).
- **Bursty Realtime (BU):** still unbuilt (no bursty-frame source), so unproven.

**Stops:**
- **`fs_over` (14 stops, on GATE and Q rows):** 1–13 first sends per row staged with B ≥ β through no tagged site, out of about 1.5 M first sends; `fs_bypass` (deferred intake) 0–22. Amendment 1(b) makes this the stop, so step 2 waits. The witness reads the view again after the gate's own check, so the count may be read skew; the data cannot tell skew from an untagged path.
- **Low rate (6 stops, GLR):** repairs per source 0.023–0.026 against the law's r ≈ 0.44–0.47, and B max 3 > 2. BASE and R4 read the same (0.022–0.027, B max 3–4), so it is not the gate. It is a separate finding: at 36 datagrams/s Realtime the engine sends about 2.5 % repair where its own r reads about 0.45 (or the rule compares different quantities). Open; the B ≤ 2 premise is wrong on every arm.

**What lets B exceed β (the read B-over-β line, 14 rows):** first sends are ruled out (≤ 13 + 22 per 1.5 M). Exempt traffic is 0 of the worst excess at K2 and K8 leg 0, but a real part at A2 (39 of 81) and K8 leg 1 (55 of 68). The unbooked rest lumps "β dipped" (β p5 down to 14 against p50 80) with "the owner had not drained" (`pend_max` 122–168, `own_gap_max` 43–166 ms). Narrowed to those two; a per-window join (end-sample β against p50, beside `pend_max` and `own_gap_max`) separates them.

**Other open:** Q K8 leg 1's `own_gap_max` reaches 2.0 s (an owner stall on quinn's BBR); reopen latency max 18.9 ms against L 6.3 ms; the c8 leg-0 batch DIAG68 item stands (16.8 against 25.7).

**Next:** the per-window join; decide whether `fs_over` is read skew (red-first test in `hoff_gate_tests.rs` with the check and the witness read at the same instant); then step 2's registration.

## 9. 68r diagnosis (post-data)

Read from the 68r raw `[HOFF]`, `[QCC]`, `[DIAG]` and `[SPAN]` lines in `docs/l1-raw/hoff68r/`. Engine lines cite the gate tree `269c11fe`. Nothing was rerun.

### 9.1 What lets B exceed β

**Rule, fixed before any count was taken.**
- *Population:* every gate=on window at t ≥ 2 s, per path, with E = B p95 − β(end sample) > batch. "Worst" means the top 3 by E per (row, path).
- *Flags:* all fields except `ex_max` are window extremes, so each flag means "consistent with".
  - **A**: B p95 − β p50 ≤ batch.
  - **U**: B max ≤ max(β p95 of this window, β p95 of the previous one) + batch. This means the backlog was admitted under a higher β that then fell.
  - **O**: `pend_max` ≥ 3 batches and `own_gap_max` (this window or the next) ≥ L.
  - **X**: the exempt part of `ex_max` ≥ half its total.
- *Primary class:* A or U gives "β moved"; else O gives "owner"; else X gives "exempt"; else "unexplained".

**Counts.** Over-batch windows on the GATE rows, by primary class:

| GATE rows | over-batch windows | β moved | owner | exempt | unexplained | worst 3 per row |
|---|---|---|---|---|---|---|
| A2, K2, K8 leg 0 | 436 | 412 | 18 | 1 | 5 | β moved 22, owner 5 |
| K8 leg 1 | 113 | 40 | 10 | 62 | 1 | exempt 5, β moved 3, owner 1 |

- The 18 "owner" windows on leg 0 fail U by only 1.0–3.2 batches. They are measured against β's p95, not its maximum.
- Q rows (quinn's BBR) read the same: 145 of 172 over-batch windows are "β moved".

**Answer: mostly cause (a), β moving.**
- In the worst windows β p95 is about 1.5–3 times its p50 (e.g. 257 against 87). The gate admits up to that β, then β falls back while B drains at R.
- Flag split on leg 0: U without A in 291 windows, A and U in 121, A alone in 0. So β spiked up and fell back; it did not dip below its median.
- In 412 of 436 windows B max stayed within one batch of the highest β p95 of the window (or the one before), so the gate held its bound at admission. The 18 "owner" windows exceed it by 1.0–3.2 batches.
- **Part of the excess is real.** `ex_max` total, the one same-instant reading (B − live β at a turn end), is over the 436 windows:
  - p50 16 (about 1 batch), p90 44, max 92 (5 batches), against a median E of about 1.7 times that;
  - booked at that turn end: first sends of the open turn 8 587, exempt 3 148, against a total of 9 554;
  - unbooked carry-in: p50 0.
- **So roughly 60 % of E is B standing above the live β at a turn end**, built from that turn's own first sends and exempt datagrams. Each first send passed `B < β` (`fs_over` ≈ 0), so the excess comes either from β falling within the turn or from the read skew still open for `fs_over`. The rest of E is the p95-against-end-sample artifact Amendment 1(a) named.
- **This agrees with §8's exempt split** (A2: 39 of 81). X is set on many leg-0 windows, but the rule ranks U above X.
- **The owner is a co-factor, not a cause.** O is set in 150 of 436 over-batch windows, against about 17 % of all windows. But B counts a datagram from its staging, and admission reads that B (`io_owner.rs` `stage`). An undrained owner keeps B high longer; it cannot carry B past β.
- **Exempt traffic is the cause on K8 leg 1.** That leg's batch is about 6 and its β about 15–20, so 50 resends or repairs are 3 β.
- **ProbeRtt carries much of it.**
  - GATE leg 0: 34 of the 83 windows with E > 50 lie within −0.3…+1.0 s of a `[QCC]` ProbeRtt step, against 14 % of all windows.
  - Q rows: 75 of 78 lie within 1 s after the 10 s ProbeRtt cadence. The Q rows have no `[QCC]` line, so the 10 s cadence is a proxy.
- What makes β spike is open. β = R·L/s, with R = window/srtt; window and srtt are only end samples, and L's terms refresh every turn.

**Reopen misses share these windows.**
- On GATE leg 0, 29 of 1 747 windows read reopen max > L, and 25 of the 29 are over-batch windows.
- 22 of the 29 show a wide β spread (p5 < p50/2), against about 2 % of all windows.
- 26 of the 29 carry O. That is partly by construction: a closed gate stages nothing, so the owner does not drain.
- **The reopen gauge is the shadow one.** On a gate=on line, `reopen` tests B against the previous line's shadow β (τ_o assumed 2 ms), held for 0.25 s (`hoff.rs` `turn`, `beta_prev`), not against the gate's live β. In a window where β moves, a "slow reopen" cannot be told apart from the live gate correctly still closed.
- The 19 ms maximum is therefore not yet evidence against §6.8. The gauge should time reopen on the gate's own `admits` at the live β.

**Q K8 leg 1, the 2.0 s gap (r3, t ≈ 44.3–46.2 s): the end-of-run tail, not an owner stall.**
- The source finished at 44.1 s: `[DIAG]` cum src stays at 507 616 from 44.1 to 46.2 s, and 46.2 s is the row's last `[HOFF]` line.
- Both legs then read `fs_n` 0, B 0, `pend_max` 0 and `app_limited` 1. The 2 s is idle time while the sender waits out its acks.
- **The 0.7 and 0.9 s gaps on r1 and r2 are different.** They follow a collapse of quinn's window on leg 1 (r2: 2.3 MB → 89 kB, β 0.8).
  - B stuck at 11–23, the same value as `dgq`, and the gate stayed closed (`hand_bind` 1) with `pend_max` 0.
  - This reads as quinn holding datagrams it did not send, while the owner had nothing to drain. The source of `dgq` was not checked, so this is an inference.

### 9.2 Low rate (LR): repair per source 0.024 against rr 0.45

**Answer: real, not a units mismatch.**
- `cod/src` is Δ`cum` cod / Δ`cum` src from `[DIAG]` (`mbar59_score`).
- `rr` is the median `[SPAN]` `repair_rate`, the per-source debt accrual, printed before the gate's `r_eff` (`emit_source.rs:727`, `:749`).
- Both count datagrams per source.
- The gate is not the cut. `spare_bind` is 0, and BASE, R4 and GATE all read the same.

**Where r is cut: ack cancellation of the debt.**
- Each source adds r to the debt (`emit_source.rs:767`), and a repair goes out only while the debt is ≥ 1 (`:770`).
- Each cumulative ack advance subtracts `newly_acked × r` (`sender_phases.rs:1328–1329`).
- At 36 datagrams/s a source goes out every 27.8 ms, against an RTT of about 11 ms (`[DIAG]` rtt). Each source is acked before the next is sent, so each advance cancels the r that the one source added.
- So the debt never reaches 1: `[SPAN]` debt p50 is 0.03 and its max 0.21–0.33 on all 9 rows. The W1 lower bracket, max(0, 1 − 1/(r·m̄)) with m̄ ≥ 0.33, is 0 and excludes nothing; the debt readings carry the claim.
- **The 2.4 % that is sent is consistent with reactive repair.**
  - Δcod is 44–54 per row. On the three r1 rows it matches the gap reports (`mpr rep` 51–53) and the gap-served resends (`retx` 52–55).
  - The repair margin `ceil(retransmitted × ε)` (`sender_phases.rs:1113`) rounds ε ≈ 0.001 up to one coded repair per served gap. That ceil always binds here.

**Ledger: defect finding.** Over this operating range (Realtime, ρ = 1, m̄ < 1/r) the proactive repair law is degenerate: its r is accrued and then cancelled entirely. It needs a ledger verdict (CLAUDE.md, formula-first laws). The registered LR rule "repairs per source = r ± 5 %" compares the accrual with what is emitted after cancellation, so as worded it cannot pass on any arm.

### 9.3 Open

- What drives the β spikes (window against srtt), and whether β should be smoothed or the reading taken against β at admission.
- A live-β reopen gauge.
- A verdict on the low-rate debt cancellation and on the margin's ceil.
- Whether `fs_over` is read skew (the §8 next step is unchanged).
