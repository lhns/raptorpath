## 19. Same-session control for §18 — pre-registration

§18 scored `REFUTED` by its own rule, through `CONTROL-MOVED` at `c1d-400`
and `c2-100` against the §15 MAIN ranges. That reference is a different
session and a different binary (main at `88476a4`, `RWM_RTOBS=1`), so the
check could not separate "the change moved N ≥ 2" from "the VM or the era
moved". The project lead's ruling: §18 stands as scored; the weakness is the
design's; this section tests it. Committed before any VM contact of this
battery (the VM was used for the §18 session only; this section's
harness and its witness fix are committed first, with the red-first test).

**Question.** Does LAW (this branch HEAD) differ from main at N ≥ 2, where
the change is a no-op by construction? And at c1s, repeat the §18 contrast
with main itself as the reference (not `RWM_STORE_PATHS=0`).

**Design.** Two binaries built fresh, each in its own fresh target, in the
same session under both VM locks; both `sha256` values are recorded in
`BINSHA.txt` and checked before every invocation:

- `MAIN` = `ce60079` (git archive; the base of this branch's parent chain);
- `LAW` = the branch HEAD at launch (engine identical to `6323db5`).

| leg | cells | arms | n | hint |
|---|---|---|---|---|
| contrast | `c1s-400`, `c1d-400`, `c2-100`, `c8-100` | MAIN vs LAW, interleaved; arm order rotated per (rep, seed) block | 6 per arm (seeds 42, 7; 3 reps each) | Bulk |
| δ rerun | `c1s-400`, `c2-100` | LAW | 3 per cell, seed 42 | Realtime |

Invocations: 4 cells × 2 arms × 6 = 48, plus 6 = 54, preceded by a
5-invocation smoke (c1s MAIN and LAW, c2 MAIN and LAW, c1s LAW Realtime).
Harness: `storelaw19_run_all.sh` / `storelaw_battery.sh` (`SL_PLAN=19`) /
`storelaw_parse.py score19`, derived from §18's. All arms: shipped defaults,
`RWM_GEN=0 RWM_DIAG=1`, window pipeline, one invocation per fresh topology,
the same env withholding as §18; no gate is set on either arm.

**The Realtime witness fix** (its own commits, red-first against one saved
§18 Realtime log, `tools/l1/test_storelaw_parse.py`): a named hint is
accepted from `[PIPE] pipeline=window hint=<h>` plus `[DCAP] q=`; the
"auto-selecting RLC windowed backend" line is still required for auto. A
named-hint row without `[DCAP] q=` stays `WITNESS-FAIL`.

**Witnesses** (as §18; a row without them is void, not null): the `[GATES]`
echo and the `[PIPE]` echo; LAW `[SUMCAP] eng>0` and a non-constant `win=`;
MAIN `[SUMCAP] eng=0/0` and `win=` the constant 1024 at `c1s-400`, and
`eng>0` at every N ≥ 2 cell; the two binaries' `sha256` distinct and
checked per invocation.

**Rule** (σ_pooled, means, minimum 4 live rows per (cell, arm) and the
outcome vocabulary as in §18; amended only as stated here).

- **N ≥ 2 cells (`c1d-400`, `c2-100`, `c8-100`): `REFUTED-NOOP`** if, at any
  of them, |mean goodput(LAW) − mean goodput(MAIN)| > 2σ_pooled, in either
  direction. Otherwise §18's `CONTROL-MOVED` is attributed to the
  cross-session reference. Fewer than 4 live rows in an arm at a cell:
  `UNSCOREABLE` at that cell.
- **`c1s-400`: LAW vs MAIN** with §18's outcome list and order, unchanged:
  `REFUTED` (goodput below MAIN by more than 2σ_pooled), `PINNED` (LAW
  `[SUMCAP]` pin ≥ 0.25), `LATENCY-COST` (goodput within 2σ_pooled and `A_x`
  p50 up by more than 2σ_pooled), `GOODPUT-UP`, `INERT-AS-DERIVED`; the first
  that holds in that order is the verdict, the others that hold are listed.
- **DECISION RULE: the law ships iff there is no `REFUTED-NOOP` and c1s is
  not `REFUTED`.** `PINNED` and `LATENCY-COST` do not block (the N = 1 step
  is a no-mode-switch defect regardless; the c1 anchor over-read, ×7–9 in
  §16 and ×12–29 on LAW in §18, is the next defect item).
- Anything else is `OUTSIDE THE PRE-REGISTERED SET`, recorded with no
  verdict.

**Reported, not scored** (the VM-drift reading): the c1d LAW and MAIN
goodput medians against §15 MAIN (677.9–755.1) and §16 (688–729), and the
same for c2 and c8, so a session-wide shift shows as both arms moving
together. Gauges per row as §18: `pin=`, `floor=`, per-window btlbw
over-read, rtt, `[LAT]` `A_x` p50/p95, paused %, occupancy. The δ rerun is
reported (LAW at Realtime: `[DCAP] q=0.05`, goodput, pin); it carries no
verdict.

**Abort table** (tokens fixed now; filled in the result): as §18's, plus
`ABORT-TESTS` (the parser test, `test_storelaw_parse.py`, fails at
launch) and `ABORT-BUILD` for either binary.

**Duration.** 54 invocations at ≈ 11–12 s each (§18: 42 in 483 s) ≈ 11
minutes of measurement; two builds ≈ 20 minutes; with lock waits well
under the 5 h cap.

### 19. Amendment (before any scored measurement): c2 is a single-path cell

The §19 smoke aborted (`ABORT-SMOKE`, nothing scored, nothing in it is a
result) on the first MAIN witness at `c2-100`, and the abort exposed a
factual error in the §18 amendment and in §19 as first committed: **`c2-100`
is a single-path cell** (`stage3_parse.CELL_SPEC`: `c2 / c2 / single`; the
ADR-0072 consequences name it as an N = 1 cell, "c2/sc2"), not N = 2. The
smoke rows (binaries `MAIN` `c9630606…f8f8cf` = `ce60079`, `LAW`
`17418fd8…a69ff4`; `docs/l1-raw/store-law-n1/same-session/aborted-smoke/`):
MAIN at c2 reads `[SUMCAP] eng=0/0` with `win=` 1024 and below (main's
`clamp(2·bdp, 10, 1024)` chain), LAW at c2 `eng=1556/1556`, `win=` 1211
median.

Consequences, stated without relabelling any scored result:

- The §18 result at `c2-100` is data of the N = 1 law, not of an N ≥ 2
  no-op; the §18 `CONTROL-MOVED` clause applied to `c2-100` compared an
  N = 1 cell, where the change applies, against §15's pre-change main. §18's
  recorded verdict is unchanged (the lead's ruling stands); `c1d-400` is the
  §18 cell whose CONTROL-MOVED bears on the N ≥ 2 no-op claim.
- §19, as amended here (replacing the matching text above):
  - **Single-path cells (`c1s-400`, `c2-100`): LAW vs MAIN** with §18's
    outcome list and order at each cell (`REFUTED`, `PINNED`,
    `LATENCY-COST`, `GOODPUT-UP`, `INERT-AS-DERIVED`). `REFUTED` at either
    cell is c1s's `REFUTED` rule: goodput below MAIN by more than
    2σ_pooled. This restores the original brief's c2 treatment (goodput a
    guard at a cell with under 5 % headroom, rule 16; `A_x` p50 and
    occupancy scored; expected: the cap falls from ≈ 989–1024 to ≈ 544–850,
    so queueing falls or holds).
  - **N ≥ 2 no-op cells: `c1d-400` and `c8-100` only**: `REFUTED-NOOP` if
    |mean goodput(LAW) − mean goodput(MAIN)| > 2σ_pooled at either.
  - **DECISION RULE: the law ships iff there is no `REFUTED-NOOP` (c1d, c8)
    and neither `c1s-400` nor `c2-100` is `REFUTED`.** `PINNED` and
    `LATENCY-COST` do not block.
  - Witnesses: MAIN `[SUMCAP] eng=0/0` at both single-path cells with the
    window cap never above 1024 (the constant 1024 at c1s; at c2 it
    follows the anchor up to 1024); MAIN `eng>0` at c1d and c8; LAW as in
    §18.
  - The VM-drift reading (c1d, c8 medians against §15/§16) is unchanged and
    unscored; c2 MAIN is reported beside §15's c2 range as a drift reading
    too (§15's main is the same N = 1 chain).
- Plan, sizes and invocation count are unchanged (the same 54 invocations
  measure the same four cells); only the cell-to-leg assignment and the
  witnesses above change. The harness commit that follows carries the
  witness change and its red-first test on the saved MAIN-at-c2 log.

### 19. Same-session control for §18 — result

Scored 2026-10-26 against the pre-registration and its amendment above,
literally; no threshold was touched after the data was read. **Verdict:
`SHIPS` — no `REFUTED-NOOP` at the N ≥ 2 no-op cells (c1d, c8), and neither
single-path cell is `REFUTED`: `c1s-400` reads `PINNED`, `c2-100` reads
`INERT-AS-DERIVED`.** §18's `CONTROL-MOVED` at `c1d-400` is attributed to the
cross-session reference: with MAIN and LAW in one session both arms sit
~24 % under §15 (LAW 541, MAIN 525 Mbit/s; §15 678–755) and within 3 % of
each other. §18's recorded verdict is not relabelled. Nothing is flipped or
merged; the ship/flip is the lead's separate, reviewed commit.

*Binaries and session.* `MAIN` = `ce60079`, `sha256 c9630606…f8f8cf`; `LAW` =
branch tree `83cc0b2` (engine identical to `6323db5`), `sha256
17418fd8…a69ff4`; each built fresh in its own target (LAW 709 s, MAIN 458
s), both checked before every invocation. Envelope
`tools/l1/storelaw19_run_all.sh`; parser `storelaw_parse.py score19`. Locks
held from 07:31:38Z (the first attempt waited 25 min and the second 0, both
behind the other agent), smoke 07:52:15Z, battery 07:52:15–08:04:52Z (757 s,
54 invocations), locks released 08:04:52Z, exit state 0 `raptorpath`, 0
`rp-*`. The first launch aborted at its smoke (`ABORT-SMOKE`, see the
amendment: the MAIN witness assumed c2 was an N ≥ 2 cell); its smoke logs are
kept in `same-session/aborted-smoke/`. Ledgers:
`docs/l1-raw/store-law-n1/same-session/final/` (`sl.log`, `score.txt`,
`BINSHA.txt`, `smoke*`, `all-era.txt`, per-invocation logs).

*Abort table (filled).*

| cause | fired? |
|---|---|
| `ABORT-LOCK` / `ABORT-CRLF` / `ABORT-SHA` / `ABORT-SENTINEL-UNWRITABLE` / `ABORT-BUILD` / `ABORT-TESTS` | no (parser test ok: 11 checks) |
| `ABORT-SMOKE` | once, in the first launch (amended away, above); no on the second (`SMOKE-PASS`, 5 rows) |
| `ABORT-RC` | 1 of 54: `c2-100 LAW-realtime` rep 3 (rc 1 at the 150 s driver limit, no summary, never left the boot cap; row void, excluded) |
| `ABORT-BRINGUP` / `VOID-COTENANT` | 0 / 0 |

*Rows.* 54; LIVE 51, `WITNESS-FAIL` 2 (`c1s-400` LAW seed 42 rep 2 and seed
7 rep 2: `law-win-constant`, the cap sat at the 2048 knee in every window),
`VOID-RC` 1.

| cell, arm | live / total |
|---|---|
| c1s-400 MAIN / LAW | 6/6 / 4/6 |
| c2-100 MAIN / LAW | 6/6 / 6/6 |
| c1d-400 MAIN / LAW | 6/6 / 6/6 |
| c8-100 MAIN / LAW | 6/6 / 6/6 |
| LAW Realtime c1s / c2 | 3/3 / 2/3 |

*Single-path cells (LAW vs MAIN; means, σ_pooled, 2σ band).*

| | c1s-400 (LAW n = 4, MAIN n = 6) | c2-100 (6 vs 6) |
|---|---|---|
| goodput Mbit/s | LAW 640.4 (606.5–721.6), MAIN 563.4 (327.8–683.3); diff +76.9, 2σ = 189.0 | LAW 88.7 (86.9–90.3), MAIN 89.2 (88.0–90.2); diff −0.4, 2σ = 2.35 |
| `[SUMCAP]` | pin 0.588, floor 0; MAIN `eng=0/0`, `win=` 1024 | pin 0.038, floor 0, cap 993 mean; MAIN `eng=0/0`, `win=` ≤ 1024 |
| `A_x` p50 | 15.4 vs 10.5 ms (diff +4.9, σ_pooled 4.9) | 81.6 vs 87.4 ms (diff −5.8, σ_pooled 24.1) |
| rtt ms / paused % / occupancy | 24.0 vs 17.0 / 24 vs 33 / 0.92 vs 0.95 | 97.0 vs 102.4 / 66 vs 68 / 1.00 vs 1.00 |
| anchor over-read | ×11.7 vs ×12.7 | ×8.2 vs ×10.8 |
| outcome | **`PINNED`** (nothing else holds) | **`INERT-AS-DERIVED`** |

c1s reads differently from §18's LAW-vs-`RWM_STORE_PATHS=0` contrast in two
ways, both visible above: MAIN's own goodput this session is 563 (one rep at
327.8 Mbit/s, rtt 35.6 ms, which is MAIN with the 1024 constant, not a LAW
effect) against §18's CTL 671, and LAW's `A_x` p50 excess over MAIN is 4.9
ms (not significant) against §18's +18.9 ms over CTL. The two LAW reps
voided by `law-win-constant` (570.2 and 670.1 Mbit/s, pin 0.87 and 0.89)
are the most-pinned; with all six LAW reps the mean is 633.7, so excluding
them does not manufacture the sign. `c2-100` does not show the
expected fall of the cap or of queueing: the LAW cap averages 993 against
MAIN's window of at most 1024, it varies with the anchor from rep to rep, and
`A_x` p50 and rtt do not move beyond the noise (σ_pooled 24 ms on `A_x`),
so the cell is inert at this n.

*N ≥ 2 no-op cells (LAW vs MAIN goodput; REFUTED-NOOP if |diff| > 2σ_pooled).*

| cell | LAW | MAIN | diff | 2σ_pooled | result |
|---|---|---|---|---|---|
| `c1d-400` | 540.8 (519.5–559.7) | 526.5 (485.3–567.3) | +14.3 | 44.4 | no-op holds |
| `c8-100` | 94.2 (80.4–102.0) | 93.1 (87.3–97.4) | +1.0 | 12.7 | no-op holds |

Their `[SUMCAP]` caps agree (c1d 1826 vs 1828; c8 1936 vs 2022), both
`INERT-AS-DERIVED`.

*VM-drift reading (reported, not scored).* Goodput medians, LAW / MAIN:
`c1d-400` 541 / 525 against §15 MAIN 678–755 and §16 688–729; `c2-100` 88.8 /
89.4 against §15 90.08–90.32; `c8-100` 95.7 / 95.0 against §15 82.9–102.9.
Both arms move together at c1d (≈ −24 % against the old sessions): a
session-wide (VM or binary-era) shift, not a property of the change.
§18's c1d `CONTROL-MOVED` is therefore attributed to the cross-session
reference, as the pre-registration says.

*δ rerun (LAW, Realtime, seed 42; unscored).* `[DCAP] q = 0.050` on all six
rows; `c1s-400` 138.3, 133.1, 124.5 Mbit/s (pin 0, all LIVE: the named-hint
witness fix works on live rows); `c2-100` 67.4 and 68.8 Mbit/s (pin 0.90 and
0.31); the third c2 row is the `ABORT-RC` above. Realtime at one path runs a
regime of its own (goodput ≈ 1/5 of Bulk at c1s); no MAIN was run there, so
no contrast is claimed.

*What this settles, and what it does not.* The change is a no-op at N ≥ 2
as designed (c1d, c8) and §18's c1d shift was the session. At c1s the law
engages and the knee binds in about half the windows (the c1 anchor
over-read, ×12 here), with goodput not separable from MAIN and no
significant latency cost at this n; at c2 it is inert. By §19's rule the
law ships; `PINNED` is the recorded next defect item (the anchor), not a
block. Open: n = 6 is small against MAIN's own c1s spread (327.8–683.3).
