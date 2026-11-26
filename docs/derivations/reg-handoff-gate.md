# DRAFT: the hand-off gate on `bbr_rs` against main: guard v2 with off-Bulk stages (pre-registration draft)

**Status.** A draft for the coordinator to register as its own status section. No row has run, and nothing in it has been built.
- It replaces the §48d draft (`reg-48d-draft.md` on `w1r2`), which will not run as a scored session: it is predicted `REGRESSION`, its in-order delay fails guard v2 by itself, and its host gate is stale (`sender-queue-law.md` §1.4, §6).
- Ruling (coordinator and Fable, 2026-11-26): the promotion candidate is `bbrrs-r4` plus the hand-off gate. `bbrrs-r4` alone runs in the same session as a reported-only arm.
- The law is paper §6.8. Its derivation and the §48b evidence are in `docs/derivations/sender-queue-law.md`.
- **The δ seat stays open** (Q-S4, Q-S5). The gate is built δ-free, and the store law is unchanged.

## 1. Arms

| arm | tree | env | role |
|---|---|---|---|
| **BASE** | main at launch, pinned by amendment with its sha | default (quinn's BBR) | the baseline |
| **R4** | `bbrrs-r4` (`fa9947dd`: §48 window law, `[QCC]`, one-round ProbeRtt, send-record ring) merged with BASE | `RWM_QUIC_CC=bbr_rs`; `RWM_BBR_PROBERTT_ROUNDS` and `RWM_BBR_PACE` unset | **reported only**. It attributes what the controller does alone |
| **GATE** | R4 plus the build items of §2 | as R4 | **the promotion candidate, scored** |

- **Pre-launch diff checks.**
  - `git diff --stat BASE R4 -- raptorpath/src` lists only §48b's file list.
  - `git diff --stat R4 GATE -- raptorpath/src` lists only the gate's files (§2).
  - No pacing seam in any arm.
- **Release binary.** GATE changes it deliberately: `binid: expected-change hand-off reserve (paper §6.8)`. R4 changes it as §48b's CAND did.

## 2. What must be built first (in this order, each its own commit, red first where marked)

1. **Gauges, reported only, on all three trees** (so BASE carries them). They go on a new `[HOFF]` line at the `[DIAG]` cadence.
   - **The hand-off backlog B_i:** the producer's own count of datagrams staged for path i, minus quinn's `tx`. That includes the owner's command channel. Printed as p50, p95 and max per tick window.
   - **Quinn's queue depth:** `hand − tx`, the existing `dgq`, beside B_i, so the channel part is visible.
   - **A stamp on the owner's published view.** The snapshot age a_i at each producer read, as p50 and max.
   - **The cadence and the pacer rate:** g_i (hand-off interval), the pacer rate R_i = 1.25·cwnd_i/srtt_i, and β_i.
   - **In-flight against outstanding:** `infl_vs_out`, the engine's `in_flight` beside Σ(quinn bytes in flight/s + B_i).
   - **Each resend's wait:** `fifo_wait_resend_i`, B_i·s_i/R_i at each resend's hand-off.
   - **The witnesses and bind gauges** of §4, computed in every arm. On BASE and R4 they read what the gate would have done.
   - **Witness of absence:** BASE and R4 print `gate=off`, GATE prints `gate=on`. The line is the execution witness (MD rule 1).
2. **The gate**, paper §6.8:
   - hand to path i only while B_i < β_i, β_i = R_i·(g_i + a_i)/s_i, at every hint and ρ;
   - pause the source when no path has room, or when the store is full (the store law unchanged);
   - the repair clamp's spare becomes (β_i − B_i)⁺/β_i.
   - **Not in this build:** placement's load term substitution (paper §6.8 names it; deferred so the gate's effect is attributable) and any change to the store law or a δ input.
3. **The red-first tests** (`sender-queue-law.md` §7). The anchors are the absolute numbers given there.
   - **Gate decision at N = 1 and N = 2:** β = 42.58 → hand at 42, not 43; β = 10.23 → hand at 10, not 11; at r = 12.5 MB/s, β = 33.45.
   - **No dial read:** byte-identical decision traces across hints, ±2 % δ and ρ < 1.
   - **Clamp values** at B = 0, B = β and B > β; repair-off traces identical.
   - **The two-leg burst in the engine sim:** without the gate B1 > 400 for ≥ 2 s, which is the red. With it B1 ≤ β1 + one batch on ≥ 99 % of polls. The mutation β × 100 must turn it red.
   - **The resend's wait** ≤ β1·s/R1 + g.
   - **T-free:** the gate reads no T.
4. **Suites** on GATE and R4, as in §48b item 5. Then the host gate `GATE-HO PASS` (all of item 3 green on GATE, and the reds red on R4) before any VM contact.

## 3. Stages (guard v2 plus the off-Bulk stages; plan to be numbered)

### Stage G, the guard (scored): plan 41d unchanged
- **Cells.** c1s and c1d at guard size, c2 and c8 at 600 MB, Bulk, `--window-reliable`.
- **Rows.** 12 per arm and cell (seeds 42 and 7 × 6), three arms interleaved rep-major: 144 rows.
- **Scored:** GATE against BASE, by §41d's rank-minp (v2 clause set, 10 000 relabellings, seed 4141, PASS iff p > 0.05, the DNF rule, the VOID-SHORT rule).
- **Reported:** the same statistic for R4 against BASE and for GATE against R4, with no label.

### Stage A, Auto (reported; off-Bulk, so the repair clamp executes)
- **Cells.** c2 and c8 at 600 MB, Auto hint, 12 per arm and cell: 72 rows.
- **Repair witness:** cod > 0 on every BASE and R4 row (proactive repair is sent). A cell without it reads `VOID-NOREPAIR`.
- **Clamp witness on GATE:** `spare_bind` is reported per row. A GATE row whose `[HOFF]` line shows `hand_bind` = 0 while the path is window-limited reads `VOID-INERT`.

### Stage R, Realtime (reported; its own ledger after G and A, plan 64's stage R pattern)
- **Cell.** c2 at 100 MB, Realtime hint, ρ = 1 witnessed by `window_reliable: true` on both ends, `RWM_PERF_TIMEOUT_S=150`, 12 per arm: 36 rows.
- ρ < 1 is not run: evicting rows do not complete (§63).

**Cost.** G ≈ 144 × 65 s ≈ 2.6 h, A ≈ 72 × 60 s ≈ 1.2 h, R ≈ 36 × 20 s ≈ 12 min. Plus builds, suites, smoke and a rehearsal first (the CAND-then-BASE check row at c8 for each new arm). About 4.5 h of rows; set `LD_HARD` at launch + 6 h.

## 4. Witnesses and bind gauges (GATE; the same quantities printed on BASE and R4)

| gauge | GATE prediction | void or flag rule |
|---|---|---|
| `hand_bind_i` (share of turns with B_i ≥ β_i) | c2 0.6–1.0; c8 legs 0.5–1.0; c1s 0.1–0.6 (store rarely full there) | 0 on a window-limited path: `VOID-INERT` |
| B_i p95 against β_i + one batch | ≤ on ≥ 99 % of ticks | above: `GATE-LEAK` (reported, read before the label) |
| `starve_i` (B_i = 0 with units waiting and quinn's window open) | ≤ 2 % of time | above 5 %: `STARVE` flag |
| `app_limited_i` | ≤ 10 % of quinn's transmit polls in steady state, with spikes at ProbeRtt exits | reported |
| `batch_i` (datagrams per hand-off) | ≥ 0.9 × R4's per path | below: `FINER-IO` flag, the c8 CPU mechanism |
| `spare_bind_i` | Stage G ≈ the `hand_bind` share (no repair at Bulk, so moot); Stage A 0.5–1.0 under saturation | reported |
| `store_pin` | c2 ≈ 0: occupancy ≈ handed + holes ≈ 300, against a cap of ≈ 1 700–2 048. Copa's RTprop falls with the FIFO, but its BtlBw over-read (7–14×) keeps the cap high. c8: 0–0.3, from holes during recovery backlogs | GATE store-limited lines (paused, B < β) > 10 % at c2: `STORE-LIMITED` flag (W1.2's H1) |
| `fifo_wait_resend_i` | ≤ 10 ms p95 on every path | reported |
| `infl_vs_out` | ≈ 60× low at c2 in every arm (the defect, unchanged) | reported |

## 5. Predictions (before any row). Ratios against BASE; the R4 column is the §48d prediction of `sender-queue-law.md` §6

**Stage G (Bulk).**

| cell | readout | R4 / BASE (reported) | GATE / BASE (scored clauses in bold) | mechanism |
|---|---|---|---|---|
| c1s | **goodput** | 0.97–1.03 at guard size | **0.97–1.03** | no collapse at guard size on main (§64) |
| c1s | **cpu_cli/GB, cpu_srv/GB** | 0.95–0.99; 0.96–1.00 | **0.95–1.01; 0.96–1.02** | ring; the gate adds a per-turn comparison |
| c1s | wire RTT | 1.15–1.60 | 1.15–1.60 | the controller's window, unchanged by the gate |
| c1s | delivered delay A_x | 0.80–1.05 | **0.35–0.65** (≈ 7–11 ms) | wrtt + β/x, with β ≈ 2.6 ms at c1s |
| c1d | **goodput** | 0.98–1.06 | **0.98–1.06** | as §48b (CAND better through BASE's tail rows) |
| c2 | **goodput** | 0.987–0.995 | **0.985–0.998** | the residual is the controller's (ProbeRtt 0.13–0.23 %, unattributed ≈ 0.6 %); the gate's refill after window jumps costs ≤ 0.1 % |
| c2 | **cpu_cli/GB, cpu_srv/GB** | 0.96–1.00; 0.94–1.00 | **0.96–1.03; 0.94–1.03** | fewer queued datagrams against more producer comparisons |
| c2 | wire RTT | 0.17–0.29 | 0.17–0.29 | unchanged |
| c2 | delivered delay A_x | 0.97–1.04 (≈ 206 ms) | **0.12–0.18** (≈ 25–37 ms) | the 1 800-datagram FIFO is gone: wrtt + β/x |
| c2 | **in-order delay mean** | 0.98–1.06 (≈ 345 ms) | **0.15–0.40** (≈ 50–140 ms) | the sojourn falls ≈ 180 ms, and holes recover on a ≈ 30 ms loop instead of ≈ 210 ms |
| c8 | **goodput** | 0.974–0.988 | **0.985–1.05** | the leg-1 hole clock falls from ≈ 0.57 s to ≈ 0.1 s, so H0 and the 3.9 s main tail shrink (§48b tail law tail ≈ H0/μ) |
| c8 | **cpu_cli/GB, cpu_srv/GB** | 1.015–1.035; 1.035–1.065 | **1.00–1.035; 1.03–1.065** | the server's finer slow-leg I/O is the window's, so it stays |
| c8 | wire RTT, fast; slow | 0.17–0.28; 0.35–0.45 | the same | unchanged |
| c8 | A_x, fast; slow | 0.55–0.80; 0.90–1.15 | **0.13–0.25; 0.35–0.50** | B0 and B1 → β |
| c8 | **in-order delay mean** | 1.30–1.60 | **0.30–0.90** | resends no longer wait 0.24–0.49 s in leg 1's FIFO; the 20-gap horizon's hold falls with the clock |
| c8 | B1, median over 4–16 s | R4 450–1 100; BASE ≤ 10 | GATE ≤ 15 | the cap-step bursts wait in the intake (back-pressure) instead |

- **Delay readout definition, stated.** The in-order delay is measured from the source-sent curve. Under the gate, emission happens at hand-off, so the time a unit spends held back before hand-off is not counted.
  - For the perf source that time is zero: it is back-pressured and holds no queued data, just as BASE's store pause blocks it before emission. So the arms compare fairly here.
  - That would not hold for a source that queues ahead of the sender, such as a TUN channel. No row here has one.
- **Label (stage G).**
  - **PASS ≈ 0.35.**
  - **FAIL ≈ 0.65,** most likely through c8 `cpu_srv/GB` (+3 to +6.5 % against an A/A q95 of 4.7 %) or c2 goodput (−0.2 to −1.5 % against d80 1 %).
  - A FAIL with the delay clauses better goes to the owner as a trade, not as a regression of the gate. GATE against R4 isolates the gate's own effect.

**Stage A (Auto, reported).**
- **GATE against R4:**
  - coded share ≤ 0.3 × R4's at both cells (the clamp takes repair to ≈ 0 while saturated);
  - goodput 1.00–1.05;
  - A_x as in stage G;
  - in-order delay: no sign predicted (less FEC means more holes for ARQ, but a much shorter loop).
- **R4 against BASE:** as in stage G within A/A.
- **Witnesses:** cod > 0 on BASE and R4 rows; `spare_bind` 0.5–1.0 on GATE rows.

**Stage R (Realtime ρ = 1, reported).**
- **GATE against BASE:** c2 A_x 0.12–0.20; goodput 0.97–1.03; completion on every row where BASE completes.
- **R4 against BASE:** A_x 0.95–1.05.

## 6. Decision rule

1. **NO VERDICT** if a stage is incomplete or `GATE-HO` failed.
2. **VOID-INERT or VOID-NOREPAIR** rows are replaced once, at most 4 per stage. More than that reads NO VERDICT for the stage.
3. **Stage G PASS, with no `GATE-LEAK` or `STORE-LIMITED` flag:** the merge question goes to the owner, with stage A and R readings beside it. The question is GATE as the new default, an ADR superseding ADR-0054's default and deleting quinn's BBR path unless kept as a reference arm (ADR-0073).
4. **Anything else** goes to the owner with GATE against R4 printed, so the gate's own effect is separated from the controller's.
5. **Nothing merges to main automatically,** and the gate does not merge on stage G alone (`meas-guard-bulk-blind`).

## 7. Open before registration

- **Plan number and harness work.** Three-arm interleave, the `[HOFF]` parser, stage A's witnesses, and the R4 and GATE builds. Sonnet harness brief, as for §64.
- **The A/A resolution for the new readouts:** A_x is not a v2 clause; the in-order delay is.
- **Q-S10:** the gate reads quinn's cwnd and srtt (its pacer's inputs). The owner rules whether that is admissible under paper §8.4.
- **Q-S2 is settled by this draft as v2.** The c1s row size is therefore guard size.
