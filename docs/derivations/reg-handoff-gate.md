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
