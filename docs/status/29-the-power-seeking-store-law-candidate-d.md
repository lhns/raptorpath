## 29. The power-seeking store law (candidate D) against main and the static knee — diagnosis — pre-registration

The user asked for both candidate laws of status §27 to be explored. Paper
§6.1 ("Two candidate laws") derives both to their fixed points first.
**(P), the always-on probe gain, is closed by the derivation**: with the
window as its only lever, the Little identity makes it a constant
delay-ratio setpoint (runaway with a loop-time τ, T(n*) = (1 + q + g)·RTprop
with RTprop), and the knee ratios it would have to hit read 1.85–4.1 across
the cells, with c2's unloaded ratio (2.1) above its own knee ratio. It is
not built and has no arm here; measuring a law the paper has already shown
to run away or stop at a constant ratio would measure the derivation. **(D),
the power-seeking law**, is built behind a default-off flag and is the
subject of this section. Diagnostic: nothing ships or flips, whatever the
result; a shipped law change needs its own ADR and battery.

**The law under test** (paper §6.1): the store cap seeks the maximum of
Kleinrock power Π = D^α/T, α = b(δ), by a dithered elasticity: per period,
two halves at fixed n̄ with cap n̄·(1 ± q/2), each half two loop rounds (one
settling, one measured), ê = Δ ln Π / ln((1 + q/2)/(1 − q/2)), n̄' =
clamp(n̄·(1 + q)^clamp(ê, −1, 1), floor, N·knee); plus the ρ hole term
Σ ε̂ᵢ·Dᵢ·stall(δ, ρ, i). Selected per run by `RWM_STORE_LAW=power`; the
shipped pooled law is unchanged and is the default. Starts from the shipped
law's ask.

**Predicted operating points** (paper §6.1, transcribed from the §27 DEF
ledger, Bulk α = 2): c2 192, c8 384, c1s 384, c1d ≤ 384 (the §27 grid's
lower edge, so for c1d the prediction is a bound, < 512). The static
knees (0.95 rule, §27 DEF): c2 192, c1s 1 024, c1d 768, c8 384.

**Design.** One binary (this branch; engine = main + `[LOOPDIAG]` + the
power law, default off), built fresh on disk, run from `/dev/shm`. Arms:
`MAIN` (shipped), `PWR` (`RWM_STORE_LAW=power`), `KNEE` (`RWM_STORE` = the
cell's §27 static knee above). Bulk, `--window-reliable`, `RWM_GEN=0
RWM_DIAG=1 RWM_ACKDIAG=1 RWM_LOOPDIAG=1 RWM_PERF_TIMEOUT_S=150`, §27's env
withholding plus `RWM_STORE_LAW`. Cells `c2-100`, `c1s-400`, `c8-100`,
`c1d-400`; three blocks (42, 1), (7, 1), (42, 2), arm order rotated per
block; n = 3 per (cell, arm); 36 rows after a 2-row smoke (c2 PWR, c1s PWR).
Harness `tools/l1/loopdiag_*` plan 29, waiter `loopdiag_launch29.sh`.

*Rules (shared file):* precondition = both locks free (`test -e` only), no
cargo/rustc (`pgrep -x`), `procs_running` ≤ 2 in 5 samples 2 s apart; run
dir and binary on tmpfs; I/O `full avg10` per row a flagged covariate;
`VOID-ONDISK` off tmpfs.

*Witnesses (row void otherwise):* `[GATES]`; ≥ 1 `[ACKDIAG]`; ≥ 4
post-warm-up `[LOOPDIAG]` windows on every path that delivered anything (an
idle c8 slow leg is recorded `p1-idle`, not voided — §27 lost 12 c8 rows to
the stricter rule); MAIN `[SUMCAP] eng>0` and no power echo; PWR the
"power store law ACTIVE" echo and ≥ 3 `[PCAP]` periods; KNEE `[SUMCAP]` off
and every post-warm-up `win=` denominator equal to the knee.

*Abort table:* as §27, filled in the result.

**Outcomes per cell for PWR (fixed now; σ_pooled from the two arms' sample
sd; means over live rows; minimum 2 live rows per arm, else
`UNSCOREABLE`).**
- `GOODPUT`: `BETTER` / `WORSE` if PWR − MAIN exceeds ±2σ_pooled, else
  `WITHIN`.
- `LATENCY`: `A_x` p50 (whole transfer) against MAIN and against KNEE, each
  `BETTER` / `WITHIN` / `WORSE` by 2σ_pooled; the post-convergence loop-time
  p50 (from `[LOOPDIAG]` windows after PWR's convergence time) is reported
  beside it.
- `FIXED-POINT`: the time-average of n̄ over post-convergence periods,
  `IN-BAND` if inside [n*/1.5, 1.5·n*] with n* the cell's KNEE value,
  `BELOW` / `ABOVE` otherwise, `NO-CONVERGENCE` if no convergence period was
  reached; its distance to the paper's predicted point is reported.
- `STABILITY`: `FLOOR-COLLAPSE` if any post-convergence period sits at the
  floor, `RUNAWAY` if the knee pin fraction ≥ 0.25, `NOISE-DOMINATED` if the
  converged suffix (the periods after convergence) covers less than a third
  of the row's periods, else `STABLE`. The sign agreement of successive ê is
  reported only: in a converged limit cycle successive ê alternate, so it
  reads near 0 for a healthy seeker and cannot mark noise.
MAIN and KNEE are reported per cell on the same columns.

**Decision rule (ships nothing).** The candidate is `VIABLE` iff at every
cell `GOODPUT` is `WITHIN` or `BETTER`, `LATENCY` against MAIN is `BETTER`
or `WITHIN`, `FIXED-POINT` is `IN-BAND` or `BELOW` with the paper's predicted
point inside 1.5× of the measured n̄, and `STABILITY` is `STABLE`; then the
law goes to an ADR and its own ship battery (the coordinator's decision).
Otherwise `NOT-VIABLE`, naming the failed cells and clauses.

**What n = 3 can and cannot resolve.** c1s goodput is noisy at this n
(§27: one static cap read 45–441 Mbit/s across rows; MAIN's own c1s spread
in §19 was 328–683), so a c1s `GOODPUT` `WITHIN` is weak evidence and a
`WORSE` there needs the per-row values beside it; the latency columns move by
×3–10 between arms (§23/§27) and are resolved.

**Predictions (before any number).** c2 and c8: `GOODPUT WITHIN`,
`LATENCY BETTER` (×5–15 against MAIN), `FIXED-POINT IN-BAND`, `STABLE`.
c1s and c1d: `FIXED-POINT BELOW` (the power criterion declines the receiver
tail), `GOODPUT WITHIN` against MAIN, `LATENCY BETTER`; c1s
`NOISE-DOMINATED` is the stated risk.

**Duration.** Build ≈ 6 min; 38 invocations ≈ 8 min; well inside the
deadlines.

### 29. The power-seeking store law (candidate D) — result

Scored 2026-10-27 against the pre-registration, literally. Diagnostic:
nothing ships. **`NOT-VIABLE`** by the decision rule: `STABILITY` is
`NOISE-DOMINATED` at all four cells, `GOODPUT` is `WORSE` at c2 and c1s, and
`FIXED-POINT` is `IN-BAND` only at c8. The latency columns are what the law
promised (A_x p50 `BETTER` than MAIN at every cell, by ×5–12); the estimator
that is meant to hold the operating point is not.

*Session.* Binary `cef7ecf8…086729` (tree `2fdc5fe`: main 92934a7 +
`[LOOPDIAG]` + the power law, default off), built 313 s; waiter met the
precondition at 11:17:13Z (load 0.03, io `full avg10` 2.3 %); smoke
`SMOKE-PASS` 11:22:59Z; battery 11:22:59–11:28:46Z (347 s, 36 rows); exit
0 `raptorpath`, 0 `rp-*`; run dirs removed. Ledgers:
`docs/l1-raw/loop-diag/p29/`. A first waiter (11:16:32Z) met the
precondition, took the locks, and stopped at `ABORT-TESTS` before any build
(the parser fixtures were missing from the shipped archive, as in §27
amendment 1); nothing ran. Its stale `FAILED-ALL` was carried into the
persisted run dir and is kept, renamed `FAILED-ALL.first-launch-abort-tests`.

*Abort table.* First waiter: `ABORT-TESTS` (nothing ran). The session: no
token fired; 35 of 36 rows LIVE; 1 `WITNESS-FAIL` (c8 PWR s42 r1:
`no-q-pace-bps-p1`); notes `p1-idle` on two c8 KNEE rows (recorded, not
void); no row io-flagged.

| cell | goodput MAIN / PWR / KNEE (Mbit/s) | `GOODPUT` (PWR vs MAIN) | A_x p50 MAIN / PWR / KNEE (ms) | `LATENCY` (vs MAIN, vs KNEE) | PWR n̄ post-conv · all-period (predicted) | `FIXED-POINT` | `STABILITY` |
|---|---|---|---|---|---|---|---|
| c2-100 | 90.2 / 80.7 / 87.5 | **WORSE** (−9.5, 2σ 4.7) | 56.7 / 6.8 / 8.2 | BETTER, WITHIN | 153.5 · 153–170 (192) | `NO-CONVERGENCE` (1/3 rows) | `NOISE-DOMINATED` |
| c1s-400 | 748.3 / 517.9 / 712.7 | **WORSE** (−230, 2σ 92) | 13.3 / 1.5 / 6.3 | BETTER, BETTER | 390.8 · 213–313 (384) | `BELOW` (band [683, 1 536]) | `NOISE-DOMINATED` |
| c8-100 | 96.6 / 96.8 / 93.9 | WITHIN (+0.1, 2σ 12.9) | 76.5 / 14.8 / 20.5 | BETTER, WITHIN | 370.9 · 258–359 (384) | `IN-BAND` | `NOISE-DOMINATED` |
| c1d-400 | 602.9 / 499.8 / 622.1 | WITHIN (−103, 2σ 110) | 18.8 / 1.5 / 5.8 | BETTER, BETTER | 443.0 · 208–312 (< 512) | `BELOW` (band [512, 1 152]) | `NOISE-DOMINATED` |

The post-convergence loop-time p50 is resolved only at c8 (PWR 41.1 against
MAIN 153.1 ms, `BETTER`; against KNEE 39.5, `WITHIN`); elsewhere too few
rows reached the cut.

*Why it fails (the gauges, per PWR row).* The ê clamp binds in 51–87 % of
periods (|ê| > 1): a single measured round's power is so noisy that the
elasticity estimate is mostly its sign, and that sign agrees with the
previous period's 34–55 % of the time — chance. n̄ therefore moves by a full
(1 + q) step almost every period, in a random walk. The "converged" suffix
is 1–9 periods out of 32–279; the time-average of n̄ over all periods sits
below the predicted points (c2 153–170 against 192, c1s 213–313 against 384,
c1d 208–312 against < 512, c8 258–359 against 384), and the goodput loss at
c2 and c1s follows from that low, wandering operating point (§27's static
curve at those caps reads 69–89 at c2 and 529–632 at c1s). No floor
collapse and no runaway: floor and knee pins are 0 in every row.

*Predictions.* c2/c8 `GOODPUT WITHIN`: MET at c8, MISSED at c2. `LATENCY
BETTER`: MET at all four. c2/c8 `IN-BAND`: MET at c8, `NO-CONVERGENCE` at c2.
c1s/c1d `BELOW`: MET. c1s `NOISE-DOMINATED` as the stated risk: it fired at
every cell.

*What the evidence says (outside the set, no verdict).* The power
criterion's *location* is borne out where the seeker settled at all (c8 371
against 384; c1s's post-convergence 391 against 384; c1d 443 against < 512),
and so is the latency it buys; the online *estimator* is not. As built, D is
dead. What would change it is the estimator, not the law: the per-half
measurement is one loop round, and its power estimate has a standard error
far above the dither's ln((1 + q/2)/(1 − q/2)) ≈ q signal. A derived window
would size the rounds per half from the measured variance of ln Π so that
its standard error falls below q/2, at the cost of convergence time (which
already ran a third or more of a c2 transfer at one round). Whether that
trade closes is a new derivation and a new pre-registration, not a re-run.
