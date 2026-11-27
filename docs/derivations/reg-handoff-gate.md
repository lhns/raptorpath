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

## 10. Plan 68s result (2026-11-27, post-data, reported only)

**Ran** on the VM: 30 of 30 rows, n = 3, seed 42. CAND harness `96ad6910` (gate tree `9b3b30b3`: token witness for `fs_over`, the `RWM_PERF_BURST` source), BASE `2f884be4`. Pre-flight green (all 11 steps; gate-off sim traces identical to `f80715eb`, 0 of 14). Ledger and report in `docs/l1-raw/hoff68s/`. K-cell BASE and R4 are the plan-68 rows (cross-session).

**`fs_over` with the token witness: still 1–11 per row** (GK2, GK8, GA2, GPH, TPH; 0 on BU). So it was not read skew. The cause, found in the code before this run finished (notes in the session scratchpad, `fsover/notes.md`): `emit_source` re-reads the gate (`open_live`) after the decision that admitted the packet. When that second read finds no open path (β drops by a datagram as `qwin` moves), placement falls back to an unrestricted pick (`place.rs:116`, `scheduler/mod.rs:533`) and stages past the bound. A second route: the `RWM_EMIT_BATCH` burst loop decides only when `tun.has_queued()`. A real gate leak of one datagram per event. Fix in progress (placement obeys the admitting decision; the burst loop always decides); the next run must read `fs_over` = 0.

**Auto at 85 % of the link (PH):**
- GATE carries the offered 85 Mbit (84.9); BASE and R4 saturate at 76 Mbit (as at full load), so for them PH is a full link.
- In-order delay: GATE mean 41 ms, p99 98 ms against BASE 399 / 782 ms.
- Repair: GATE coded share 0.042 against R4 0.081, `spare_bind` 0.47. **The registered stop "clamp binds with headroom" fired**: at 85 % offered load the predicate halves repair. 85 Mbit of source plus overhead sits near the 89.8 Mbit the link carries, so the headroom is about 5 %. The clamp's range is now: nearly free at 70 %, binding half the time at 85 %, pinned at 100 %. Ledger verdict on that shape owed with the 70–85 % curve (a defect finding if the predicate cuts repair the path has room for).

**Bursty Realtime (BU, 10-datagram frames every 33 ms, ρ = 1):**
- The gate never binds (`hand_bind` 0); repairs per source 0.49 on all arms; goodput equal.
- In-order p99 37.5 ms on 8 of 9 rows. One GATE row (GBU r1) reads mean 8.1, p99 100, max 600 ms and a B-over-β excess of 52, against max 37–60 ms on every other row. One row in three; cause open. Frame completion time has no readout (UNWITNESSED68).

**A/A (GATE against its twin T on PH, same binary):** goodput 1.000, coded share 1.014, CPU 0.94–0.95, in-order delay 1.11–1.18. So at n = 3 differences under about 6 % in CPU and about 20 % in in-order delay are not readable.

**Unchanged from 68r:** B over β on most GATE rows (diagnosed in §9, β moving); c2 Bulk goodput DIAG68 0.994; c8 leg-0 batch DIAG68.

**Next:** the `fs_over` fix and a rehearsal reading 0; GBU r1's stall; the clamp's 70–85 % curve and its verdict; then step 2's registration.

**§10 correction (2026-11-27, arithmetic, no new data):**
- **PH:** at 70 % the repair rate is about 0.15 per source (coded share 0.133). Full repair at 85 Mbit of source would need about 98 Mbit of wire, against about 90 Mbit the link carries. The room allows a coded share of about 0.055; GATE sent 0.042. So the "clamp binds with headroom" stop rests on a wrong premise (as LR's did): PH has no headroom for full repair. R4 is no reference there (it carried 76 Mbit, not 85). Predicted: the clamp starts to bind near 78 Mbit offered. To check with one load on each side (75 and 80 Mbit), not a sweep; until then the 85 % reading is not a defect finding.
- **BU** runs at 1.2 Mbit with 10-datagram bursts against β ≈ 500, so the gate cannot bind there. It shows nothing about bursts near or above β, or bursts on a loaded link; a fair bursty cell needs frames larger than β.

## 11. β spikes: cause and fix (derivation)

Read from the 68s raw `[HOFF]`, `[QCC]` and `[DIAG]` lines (`docs/l1-raw/hoff68s/rep68s/diag-ld/`). The scripts and their output are in `docs/l1-raw/hoff68s/diag11/` (`out_diag11.txt`; run as `python -I <script> ../rep68s/diag-ld` from that directory). Engine lines cite the gate tree `9b3b30b3`. Nothing was rerun.

The law change this section proposes is written out in `sender-queue-law.md` §10 (Revision 5, proposed). Paper §6.8 stays the authority until the owner accepts it.

### 11.1 What the logs can and cannot show

- `[HOFF]` gives one end sample per 0.25 s window of `win` (the published `qwin`), R, L and β. It also gives β's p5/p50/p95 over the window's **producer turns** (`beta_q`). srtt is recovered from the end sample as 1.25·win/R.
- **Within a window, motion cannot be split between window and srtt.**
- **Turn quantiles over-weight open-gate time.** A closed turn sleeps on the 1 ms poll (τ̂_p ≈ 2.7 ms realized); an open one does not. So a β p95 above p50 lasts at most 5 % of the window (12.5 ms), and probably less.
- **Across windows, the end samples are random instants, so their spread decomposes.** Over t ≥ 2 s on the GATE Bulk rows (`decomp.py`):

| leg | share of var(log β): window / srtt / L | corr(log win, log srtt) | (win/srtt)/b̂, p50 and p95 | F/win, p50 |
|---|---|---|---|---|
| c2 leg 0 (A2, K2, PH, K8) | 0.20–0.83 / −0.14–0.22 / 0.14–0.63 | 0.50–0.86 | 0.94–0.96 / 1.06–1.27 | 0.94–0.96 |
| K8 leg 1 | 0.51–0.72 / 0.11–0.27 / 0.17–0.28 | 0.30–0.58 | 0.97–0.98 / 1.15–1.20 | 0.97 |

Window and srtt move together, as Little's law says they should when F ≈ W. β moves where they do not, and with L.

### 11.2 Cause

**(a) The mechanism the shape supports for the large spikes: R over-reads quinn's release whenever srtt is below the RTT of the bytes now in flight.**
- **Not proven at this resolution (§11.1).** The gauge that would settle it is the tuple (W, srtt, L) at the turn of each window's largest β, a `beta_max` field in `[HOFF]` (owed).
- **Why R over-reads.** R = 1.25·W/srtt is quinn's pacer refill (`pacing.rs:110`). On these rows the window binds (`hand_bind` ≈ 0.65, F/W ≈ 0.95), so quinn releases what the window lets out: the room W − F, plus what acks free at the ack clock ẋ. W/srtt = ẋ only while srtt is the RTT of the bytes in flight.
- **Where it breaks.** After a window restore, the queue has just drained, so srtt sits near m̂ while W is back at full size. The new RTT reaches srtt one round later. Until then, R reads up to W/(m̂·ẋ) ≈ srtt_ss/m̂ times the ack clock.
- **ProbeRtt exit is the clean case.** `window()` returns the ProbeRtt window, then the full cwnd (`bbr_rs.rs:1078`).
  - There are 136 windows at or just after a `[QCC]` ProbeRtt step (112 on leg 0, 24 on K8 leg 1; `prtt.py`). On leg 0, β p95/p50 reads 1.02–2.22, every one within its bound srtt_before/m̂ (1.5–4.1).
  - Two windows exceed their bound, both on K8 leg 1, where β ≈ 15–40: r3 at 21.3 s (3.91 against 1.88) and r2 at 31.1 s (1.61 against 1.50).
  - Over the rows, the largest spread is 2.0–2.8 against srtt/m̂ 2.5–2.8 on leg 0 (`spikes.py`).
  - B follows: K2 r1 at 10.3 s reads β p95 214 and B p95 213.
- **ProbeRtt is not the whole of it.**
  - ProbeRtt steps come every 10 s. The windows within −0.3…+1 s of one are 14 % of windows, but carry 20–37 % of leg-0 excess (21–73 % on K8 leg 1).
  - Bulk rows read 5–6 quinn congestion events per window (`recjoin.py`). Spike windows read no more than calm ones (median 5–7 against 5–6), and their end-sample F/W is the same (0.96). So recovery exits cannot be told apart from ordinary motion at this resolution.
  - By band (`spikes.py`): windows with β p95/p50 ≥ 1.5 (5–7 % of windows) carry 30–50 % of leg-0 excess, and those at 1.2–1.5 carry 26–65 %.
- **The ack-clock reading supports (a)** (`ackclock.py`; ẋ = Δ`qab`/Δt from `[QCC]`, the window's mean ack rate):
  - end-sample (W/srtt)/ẋ reads 1.09–1.17 at p50 and 1.34–1.37 at p90;
  - β p95 over the window's ack clock 1.25·ẋ·L/s reads 1.23–1.30 at p50, 1.54–1.67 at p90, and up to 4.6.

**(b) L moves in slow steps, and part of it is a quantizer.**
- **L steps are smaller than R's.** Between consecutive end samples, |Δlog L| reads p50 0.01 and p90 0.07–0.18; |Δlog R| reads p50 0.05–0.06 and p90 0.19–0.23 (`steps.py`).
- **τ̂_p moves most.** It is the p99 of the producer's poll wait over a 1 s roll. It is the most-moved term in 36 % of windows, and those windows carry 36–55 % of the excess (`recjoin.py`).
- **τ̂_o takes only the values 2.97, 4.19 and 5.93 ms.** These are the upper edges of the owner histogram's √2-spaced buckets (`hist_p99_s`, `WorkHist::upper_ns`).
  - So L can read high by up to 41 % of τ̂_o (≈ 1.2 ms, ≈ 19 % of L), and it steps by that much for at least the 1 s roll.
  - That is a quantizer inside the law with no provenance.
  - **Defect finding (MD18): the L term's τ̂_o is quantized at √2.** The fix is interpolation within the bucket. It is a gauge-resolution fix, not a law change.

**(c) `qwin` is fresh in Bulk and stale under an ack blackout.**
- `RouteProbe::on_end_acks` stores `window()` once per ACK event, before the controller's own update of that event (`io_owner.rs:628`). At bulk ack rates that lag is sub-millisecond.
- With no ACKs, `qwin` and `qinfl` freeze. On GBU r1 at 53.2–54.0 s, the gate read `win` 18.2 kB while quinn's window (`[DIAG]` `qcwnd`) was 5.8 kB (§11.5).
- That staleness errs loose: β reads too high on a dead path.
- Publishing both atomics from `on_sent` and `on_congestion_event` as well closes it. It is a wiring fix, with no law.

**(d) Most "B over β" is β's ordinary spread, tracked by B and read against one end sample.**
- β p95/p50 has a median of 1.13–1.18 per window (`lsplit.py`). B sits at β (B p50 ≈ β p50) and follows its p95.
- So E = B p95 − β_end is about β's own spread plus a batch. The registered "B p95 ≤ β + batch" fires whenever that spread exceeds batch/β ≈ 0.2.
- §9's U flag (B max within a batch of the window's highest β) holds on 412 of 436 windows. **The gate keeps its bound at admission**; what the readout calls excess is β falling after admission.

### 11.3 Is "admit up to the instantaneous β" the right law?

**The inequality is right. The rate in β is wrong when the window binds.**
- `sender-queue-law.md` §2.2's argument sizes B by what quinn **releases** before the next look, and that is the lesser of two caps:
  - the pacer refill, 1.25·(W/srtt)·L;
  - what the window lets out, (W − F)⁺ plus the bytes acks free over L.
- Revision 4 reads only the first. Under a binding window it over-reads exactly as in (a), and the over-read becomes delay for everything behind it.
- **So the fix is the second cap, not a filter:**

```text
   β_i = min( R_i·L_i ,  (W_i − F_i)⁺ + R̃_i·L_i ) / s_i ,   R_i = N·W_i/srtt_i ,   R̃_i = N·ẋ_i ,   N = 1.25
   ẋ_i = (bytes acked on path i over the last srtt_i) / srtt_i
```

- **Provenance** (in full in `sender-queue-law.md` §10). No new constant.
  - W_i and F_i are the `RouteProbe` atomics already in the view.
    - F_i is meant live: also published from `on_sent`.
    - Today `qinfl` is stored in `on_end_acks`, after the ACK's bytes leave flight and before quinn refills them. So the logged W − F (p50 ≈ 13 datagrams) is mostly the last ACK event's freed bytes, not standing room.
  - ẋ_i is measured, from an acked-bytes counter in `RouteProbe::on_ack`. Its horizon is one srtt, BBR's delivery-rate interval (one packet's send-to-ack), so it reads one window's delivery rather than one ack burst.
  - **Argued, not cited; the owner's call:**
    - N on ẋ. N is RFC 9002 §7.7's slack, reused on the ack clock for its stated reason ("variations in RTT"). That reuse is an argument.
    - Leaving out the window's growth over L. The alternative is a factor 2 on ẋ, from "growth ≤ acked" (slow start: cwnd += acked; `bbr_rs` ProbeBw: min(target, W + acked)). That factor would cap the restore spikes at ≈ 1.6× instead of ≈ 1×.
- **Shape.**
  - Units: both terms are bytes, so β is in datagrams.
  - Continuous: a min of two continuous terms, both always computed.
  - No dial: it reads no hint, no δ/ρ and no mode. The second term is +∞ while srtt or ẋ is undefined (cold start, as today).
  - Monotone: β rises with W, ẋ and L, and falls with srtt and F.
  - Per leg: N = 1 and N = 2 are each one leg's own term.
  - It is a clamp, so it carries a bind-fraction gauge: `ack_bind_i`, the share of turns where the second term is the smaller.
- **What it keeps.**
  - **App-limited bursts (GBU):** room ≈ W, so the first term binds and β is unchanged.
    - `capcheck.py`: GBU end samples read cap < β in 3 of 228 windows (r1), 0 of 227 (r2) and 12 of 226 (r3).
    - The r3 binds are where β (≈ 500) exceeds the whole window (≈ 240 datagrams), so the cap still leaves ≥ 100 datagrams against 10-datagram frames.
  - **Startup:** the room term is read live, so window growth is admitted at the next look.
    - Unlike Revision 2's ν̂, no rate term is built only from past sends, so growth has no ×1.25 ceiling.
    - The worst case is one look gap L with up to ẋ·L of unused room.
    - **But the cap does bind in Startup** (`capcheck.py`, 41 Startup end samples on the Bulk rows):
      - **The first Startup line** of K2 r2, PH r2 and PH r3 binds at cap/β 0.92, 0.89 and 0.74 with the logged room, and at 0.79, 0.84 and 0.57 with room 0.
      - **K2 r3** stays in Startup until 10.3 s. W sits at 478 kB with the link full and the gate open (`hand_bind` 0, B p50 ≈ 10). Its 38 windows never bind with the logged room (min 1.07), and bind in 19 of 38 with room 0 (min 0.60). B ≪ β there, so the bind costs nothing.
    - So Startup β falls by up to 26 % (logged room) or 43 % (room 0) on the first line.
    - **That is Revision 2's hazard, here bounded by the live room term rather than structural. It is a risk to witness, not one ruled out.** Witnesses: time to leave Startup, and `starve` in Startup.
  - **Cruise:** β at the end of each window, over one L of the ack clock (β_fix·s/(ẋ·L)), reads p5 0.97–1.02 and p50 1.25–1.44. So it covers the next L of release.
- **Rejected.**
  - **A cap at the controller's pacing rate or at b̂.** On GBU r1 (ProbeBw), `qbw` is 0.35–0.66 MB/s. With s = 536 B (R·L/β from the 53.5 s line), that gives β ≈ 4–7 against today's 19, so a 15-datagram frame plus repair would wait two to three reopens.
  - **A running min over L or srtt.** The spikes last up to 2 L, and a min over srtt reads the low srtt that causes them.
  - **A slew limit (×2 per srtt upward).** It reads the same low srtt, and it slows the refill after every ProbeRtt.

### 11.4 Predicted effect on the 68s rows (counterfactual replay)

**Model** (`replay2.py`; outputs in `out_diag11.txt` and `out_diag11b.txt`).
- **The cap.** Per window, the cap is room + 1.25·ẋ·L/s, with ẋ the window's mean ack rate. Room is taken two ways, as a bracket:
  - **logged room:** the end-sample W − F, which over-reads (§11.3);
  - **room 0:** F live, which is what Revision 5 means. On Bulk rows this is the realistic end.
- **B follows β as it does now.** `track.py`: per window, B p50 − β p50 reads −1 at the median (p10 −2 to −4) on A2, K2 and K8. B p95 − β p95 reads 0 (PH: −8 and −5). So a B quantile drops by what the cap removes from the same β quantile.
- **The readout.** E_fix = B_fix p95 − min(β_end, cap).
- **ẋ noise.** The engine's ẋ over one srtt is noisier than a 0.25 s mean, so these numbers are the optimistic end.

| row, leg (t ≥ 2 s) | E p50 / p90 / max: now → logged room → room 0 | windows with E > batch | E > 50 | B p95 median | B max p99 | β p50 median |
|---|---|---|---|---|---|---|
| A2 leg 0 | 11/37/332 → 4/24/88 → 0/16/85 | 183 → 99 → 39 | 34 → 7 → 4 | 95 → 88 → 81 | 202 → 152 → 135 | 82 → 80 → 76 |
| K2 leg 0 | 11/29/119 → 3/21/81 → 0/16/81 | 171 → 85 → 38 | 21 → 3 → 2 | 93 → 85 → 72 | 183 → 137 → 124 | 81 → 79 → 76 |
| K8 leg 0 | 12/34/137 → 4/23/69 → 1/17/69 | 159 → 84 → 56 | 26 → 3 → 3 | 96 → 88 → 81 | 189 → 152 → 146 | 82 → 80 → 76 |
| PH leg 0 | 7/23/137 → −1/14/67 → −2/10/61 | 197 → 89 → 48 | 17 → 3 → 2 | 92 → 82 → 76 | 217 → 183 → 139 | 82 → 80 → 77 |
| K8 leg 1 | 3/9/56 → 2/8/56 → 1/6/50 | 115 → 91 → 62 | 1 → 1 → 0 | 19 → 19 → 17 | 45 → 44 → 39 | 16 → 16 → 15 |

**The first model, and why it is not used** (`replay.py`, in `out_diag11b.txt`).
- It assumed B does not move with the cap: B_fix p95 = min(B p95, cap + batch), with β_end lowered.
- It reads the readout getting worse. Over-batch windows go 183 → 235 (logged room) or → 396 (room 0) on A2, and similarly on the others, while B max barely moves (p90 135 → 131).
- **Its premise contradicts the data.** B sits within one or two datagrams of β's median and p95 in every window (`track.py`), so a lower β is a lower B.
- **Its one real warning stands.** Against an end sample, a β lowered by the cap reads as more "excess" wherever B lags the cap downward. That is the same artifact as §11.2(d).

**Predicted readouts on leg 0 (logged room → room 0):**
- β median −2 → −5 to −6 datagrams (end-sample β_fix/β p50 1.00 → 0.89–0.91).
- B p95 median −7 to −10 → −14 to −21 datagrams, about −1 to −2 ms at x ≈ 9.5 k datagrams/s.
- Over-batch windows cut by about half → by 65–79 %. Windows with E > 50 cut by 79–88 % → 88–90 %.
- **`ack_bind`:**
  - leg 0: between 0.32–0.37 (windows with the cap below β p50) and 0.48–0.53 (below β p95) with logged room, and between 0.50–0.52 and 0.67–0.69 with room 0;
  - K8 leg 1: 0.11–0.22 → 0.52–0.61.
  - It binds about half the time, not always, so the law's shape stays visible.
- **GBU:** ≤ 0.05. There the room is real (F ≈ 1 kB against W ≈ 18 kB), so the room-0 bracket does not apply.
- **Goodput and `starve` unchanged.** The window term covers one L of the ack clock (p5 0.97–1.02 in both brackets).

**`ex_max` (same instant).** The windows the cap cuts hold 70–75 % of leg-0 over-batch `ex_max` and 60–80 % of its first-send booking (`exmax.py`). That is the most the fix can remove. The exempt booking (979–1 612 per arm) is untouched.

**What stays.** "B p95 ≤ β + batch" still fires on 13–16 % (logged room) or 6–11 % (room 0) of leg-0 windows, so it stays reported, not a stop. The residual is:
- one batch of granularity;
- exempt traffic: K8 leg 1 unchanged, and about a third of the booking on A2 and K2;
- L's down-steps (p90 7–18 %, so 6–15 datagrams) until the τ̂_o quantizer is fixed;
- W's real drops (recovery entry, ProbeRtt entry), which no admission law can undo;
- the p95-against-end-sample artifact.

The gate's own witness should be the U form, the law's invariant: B max ≤ the highest β of this window and the one before, plus one batch.

### 11.5 GBU r1: 600 ms in-order max and 52 of excess

**What happened: a path stall in quinn and `bbr_rs`. Exempt datagrams piled up behind a correctly closed gate. No gate mechanism was found; there is no control, because no R4 row reached ProbeBw.**

**r1 is the only BU row whose `bbr_rs` left Startup** (S → B at 15.4 s). r2 and r3 stay in S, and the RBU rows only go S → R → S.
- After that, the window is 2·b̂·m̂ + A from app-limited samples: 8.2 / 14.1 / 16.7 kB (p5 / p50 / p95), about 15–31 datagrams of 536 B. A frame plus its repair (r = 0.5) is 15 datagrams, and β is ≈ 16, so a frame meets the gate near its tail.
- The gate starts binding: `hand_bind` > 0 in 129 of 172 windows (mean 0.038), against 1 of 58 before 15.4 s.
- β falls from ≈ 440 to ≈ 16.
- That corrects §10's correction ("BU cannot bind the gate") for one row in three.

**The stall at 53.2–54.0 s:**
- `qcwnd` falls 18 238 → 5 808 (4 × 1 452, `bbr_rs`'s min window), with `qce` +1.
- `qab` (acked bytes) and `qage` freeze from 53.5 to 54.0 s: no ACK arrives.
- quinn's `tx` advances 4 per window, while `hand − tx` grows 20 → 47 → 71.
- `src` is 0, because the gate is closed (`hand_bind` 1.0) with B ≥ β.
- `retx` (444 → 520) and repair keep staging: `ex_max` 79, of which 81 exempt and 2 first sends.
- `relgap` max is 685 ms.

**Reading:** a Gilbert-Elliott loss run took the whole small flight.
- With no ACKs, quinn's sends are probe-only (≈ 2 per PTO) under PTO backoff. Four backed-off PTOs from ≈ 40 ms sum to ≈ 0.6 s.
- PTO is inferred from the frozen acks and the probe-sized `tx` steps; the logs carry no PTO counter.
- The netem loss is Gilbert-Elliott (p 1.3 %, r 50 %), so a run of ≥ 8 losses has probability ≈ 0.8 % per burst, about one per row.

**Gate attribution:**
- The gate stayed closed, and was right to. Its stale `qwin` (§11.2 c) gave β ≈ 19, where the true window gave ≈ 6.
- Its 52 of B over β is exempt traffic staged into a dead path. That is §6.8's exempt term, not a leak (`fs_over` 0).
- The trigger is `bbr_rs` leaving Startup on a bursty app-limited flow, with a window of one to two frames. RBU r1 and r3 touch the min window for one window each, but never leave Startup for ProbeBw.
- Whether the gate made that exit likelier cannot be read at n = 3. It barely binds in Startup (1 of 58 windows), so it does not shape the bursts there.

### 11.6 A fair bursty test: cell spec for the harness

As run, BU tests nothing about the gate in 2 of 3 rows: in Startup β ≈ 440–550, and no 10-datagram frame exceeds it. A fair test puts frames above β, or puts bursts on a loaded path.

| cell | source | link | condition it must reach (execution witness) | arms, n |
|---|---|---|---|---|
| **BW** (frames above β, headroom) | Realtime ρ = 1. A 5 s saturating lead-in, so that `bbr_rs` reaches ProbeBw by full-bandwidth detection, then `RWM_PERF_BURST=k/33` with k ∈ {10, 20, 40} | c2 | `[QCC]` qm = B on ≥ 90 % of burst-phase lines; ≥ 50 % of frames meet ≥ 1 refusal at k = 20 (`hand_bind` > 0 at the frame's first datagram) | BASE, R4, GATE; n ≥ 5 (r1's stall is a one-in-a-row event) |
| **BL** (bursts on a loaded path) | the same burst source plus a Bulk transfer on the same path (one connection, two objects), k = 10 | c2 | the path saturated (`hand_bind` ≥ 0.5 on GATE); ≥ 50 % of frames arrive while the gate is closed | BASE, R4, GATE; n ≥ 5 |

**Readouts (new, all owed):**
- Frame completion time per frame, from the first datagram's intake to the last datagram's delivery: p50, p99 and max, by k.
- At each frame's arrival: β, W/s, k/β and k/(W/s), so that a window-limited frame is not booked to the gate.
- Controller mode share (qm).
- An ack-blackout gauge: the longest interval without an ACK, and quinn's PTO count.
- `ack_bind` (Revision 5) and the U witness.

**Predictions to register with it:**
- **BL:** GATE's frame p99 is below BASE's by the FIFO wait (BASE c2 in-order mean 344 ms).
- **BW:** frame time on GATE rises with k/β, by about ⌈k/β⌉ − 1 reopens of at most L each.
- **k/(W/s) > 1:** the frame is window-limited, on R4 and GATE alike.
