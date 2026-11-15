## 34. The store law's missing input after the receiver change — derivation, no VM — verdict, and a diagnostic pre-registration (not run)

§30 left the store law without its one input: the knee location, or the rate
of the bottleneck that sets it. §32 then made the receiver cheaper at c1. This
section asks, on paper from the committed ledgers, (a) whether §32 reopens
D-W at c1, and (b) whether any source that needs no new sender-side rate
sampler supplies the input. Derivations, provenance and tables: paper §6.1,
"Candidate inputs for the knee after the receiver change". Scripts and
outputs: `docs/l1-raw/thread-pd/battery/derive34/` (`diagwin.py` window noise
from `[DIAG]` with its calibration against `[LOOPDIAG]`; `qcwnd.py` quinn's
window on the §27 grid; `authority.py` store occupancy and quinn's backlog;
`rcap.py` receiver capacity from `[RDIAG]`). No code, no build, no VM.

**Verdict: `NONE-CLOSES` on the committed ledgers.**

*(a) §32 against §30.* The §32 ledgers carry no `[LOOPDIAG]` and no static
cap, so they cannot give σ²_win at n*; they give a same-session MAIN/NEW
contrast at the shipped law's pin through a `[DIAG]` proxy (within ×1–5 of
§30's statistic at c1, reproducible across sessions at one cap, useless at
c2/c8). c1s: 0.205 → 0.069 (×2 median, ×3 mean), mostly in D (×4); c1d:
0.304 → 0.328, no change. Even the favourable ×3 at n* leaves W* ≥ 250
rounds, one decision per ≈ 2.4 s of a ≈ 3.9 s transfer; reaching §6.1's c1s
point from the pin within a third of a transfer needs a ×166 cut.
**`NOT-REOPENED`.** Open, and only measurable: post-§32 c1s is link-bound at
the pin (receiver busy 69 %, capacity 1.38 L, delivered 0.96 L), so its knee
and the nature of its noise may both have moved.

*(b) Per candidate (fixed point · noise · circularity).*

| candidate | fixed point | noise | circularity | verdict |
|---|---|---|---|---|
| (b1) quinn's BBR window, cap = (1 + q)·K, K = 2·max_bw·min_rtt + ack height | c1d ≈ 1 050 (in band), c1s ≈ 545 (below band, G ≈ 0.91–0.94 Gmax), c2 ≥ 1 024 and c8 at the clamp (×5–15 above the knee) | cv(K) 0.08–1.32 in a row; c1s at cap 2048 reads 575–2 680 across sessions (a max filter holding non-app-limited reads) | Spearman(K, n) 0.90 at c1s and c2 (K ≈ n at c1s 192); 0.17 c1d, 0.52 c8 | does not close; the gain 2 returns as BBR's `cwnd_gain` |
| (b2) cwnd-binding share (ADR-0068 q12) | the authority crossover: ≈ 2048 at c1s, > 1 024 at c2/c8, above every knee | as (b1) | reads K through a threshold | does not close; survives as the authority gauge (below) |
| (b3) path memory (RFC 9040) | D's argmax (right in §29) | first transfer pays D-W: ≈ 5.5 s per unit ln n at c2, ≈ 40 s–2 min at c1s | none in rate; adds an unmeasured stationarity premise (§32 itself moved the c1 map) | a seed, not an input: conditional at c2/c8 from the second transfer, minutes at c1 |
| (b4i) receiver delivered rate | cap/T (Little) | — | circular by identity (§27 one layer down) | closed by derivation |
| (b4ii) receiver capacity μ_R = msgs ÷ busy, as a receiver-wide ceiling | c1d ≈ 430 (pre-§32 μ_R, below band) or ≈ 670 (NEW μ_R with §27's spread, in band; cross-session); over-asks ×1.4–4.7 where the receiver is not the bottleneck (c1s post-§32, c2, c8) | cv 0.04–0.05 per row on NEW | not the sender's delivery; load dependence (batching) unmeasured, since no static-cap ledger has `[RDIAG]` | open at c1d only; needs a receiver → sender field and an ADR |
| (b4iii) receiver-side dispersion | the link rate, in principle | skb-unit arrivals (GSO g ≈ 2.7–4.9), batch-drained receiver | non-circular in principle | named; no ledger carries arrival times |

*The authority finding (ADR-0068 q12).* At every static cap the store sits at
0.88–1.00 of its cap, and quinn's own backlog (`dgq[hand − tx]`) is ≤ 92
datagrams (≤ 1 ms of L) at every cap up to 1.5·n* in every cell: the store
cap, not quinn's window, holds authority at and around every knee, and the
c1 knee is not quinn's. Quinn takes part of the authority only at the pin
(c1s: 228 at §27's 2048, 584 ≈ 6 ms of L on §32 NEW). §30's K < c shares
(4–32 % at c1s, 18–24 % at c2) count a different thing — the store holds a
symbol until SACK release, quinn until its QUIC ACK — and overstate quinn's
authority; any nested store law's authority gauge is the quinn backlog B.

*What it leaves.* The law stays as shipped (ADR-0072). The link rate at c1s
(post-§32), c2 and c8 still has no source; a dispersion instrument is the
only named non-circular candidate and is not built. One question is still
live and measurable: at the receiver-bound cell (c1d), does the receiver
capacity, as a ceiling with RTprop plus the loop spread, land on the
post-§32 knee with a stable fixed point — and, on the same grid, has §32
moved c1s's knee and noise enough to reopen D-W? The diagnostic below
answers both. **It is pre-registered here and NOT run: stopped before any VM
contact, for the coordinator's decision** (it needs the `[LOOPDIAG]`
instrument re-applied to main, an instrument-only engine diff).

### 34. Diagnostic — the post-§32 c1 static grid: knee, noise at the knee, receiver capacity, authority — pre-registration

Diagnostic: nothing ships or flips, whatever the result; no law is built.

**Tree.** github/main at launch (≥ `baadb32`) plus the `[LOOPDIAG]`
instrument from `archive/store-power` re-applied as its own commit
(observation only, default off; the power law itself is not taken). One
binary, built fresh on disk, measured from tmpfs.

**Design.** Arm `DEF<n>` = `RWM_STORE=<n>`, shipped substrate BBR. Cells and
grids: `c1s-400` {256, 384, 512, 768, 1024, 1536, 2048}; `c1d-400` {384,
512, 768, 1024, 1536, 2048}. Env: Bulk, `--window-reliable`, `RWM_GEN=0
RWM_DIAG=1 RWM_ACKDIAG=1 RWM_LOOPDIAG=1 RWM_RDIAG=1 RWM_PERF_TIMEOUT_S=150`,
§27's withholding list. Three blocks (42, 1), (7, 1), (42, 2), sweep
ascending on even blocks and descending on odd, cells alternated; 39 rows
after a 2-row smoke (c1s DEF384, c1d DEF768). Harness `tools/l1/loopdiag_*`
as §27/§29, a plan 34; parser tests run in the session.

*Rules (shared file):* launch precondition (both locks free by `test -e`
only, no cargo/rustc by `pgrep -x`, `procs_running` ≤ 2 in 5 samples 2 s
apart); run dir and binary on `/dev/shm`; io `full avg10` before/after every
row, flagged at ≥ 20, never voided; `VOID-ONDISK` off tmpfs.

*Witnesses (row void otherwise):* `[GATES]`; every post-warm-up `win=`
denominator equal to n and `[SUMCAP] eng` absent or 0; ≥ 8 post-warm-up
`[LOOPDIAG]` windows on every path; a `dgq` field on every `[DIAG]` line;
≥ 3 server `[RDIAG]` lines with busy ≥ 20 % (else the row is void for R3
only).

*Abort table:* as §27/§29 (`ABORT-LOCK`, `-CRLF`, `-SHA`,
`-SENTINEL-UNWRITABLE`, `-SMOKE`, `-BUILD`, `-TESTS`, `-RC`, `-BRINGUP`,
`VOID-COTENANT`, `VOID-ONDISK`), filled in the result.

**Readouts and outcomes (fixed now; L = 97 700 datagrams/s per c1 leg,
W = 1 279 B, q = 0.1, α = 2).** Per cell and n, means over live rows.
- **R1, the knee.** n* = smallest n with goodput ≥ 0.95·Gmax; band
  [n*/1.5, 1.5·n*]; the Bulk argmax of Π₂ = goodput²/T (T = max-path Tp50)
  beside it. Reported, no outcome.
- **R2, D-W at c1 (question (a)).** σ²_win at the grid cap nearest the Π₂
  argmax, by §30's `win.py` statistic; γ from the ratio of the variance of
  1 s block means to the 250 ms window variance (log-log slope over the 4×
  span); W*_white = W_win·σ²_win/(q/2)² and W*_γ = W_win·(σ²_win/(q/2)²)^(1/γ);
  reach = (decisions in ⅓ of the row's transfer)·ln(1 + q)·0.72. Per cell:
  **`REOPENS`** iff reach ≥ ln(2048/n_argmax) at both W*; **`CLOSED`** iff it
  fails at W*_white (the lower bound); else `AMBIGUOUS`.
- **R3, the receiver capacity as a ceiling (question (b4ii); c1d scored,
  c1s reported).** μ_R(n) = median over the row's `[RDIAG]` lines with
  busy ≥ 20 % of msgs ÷ busy. `FLAT` iff max_n μ_R / min_n μ_R ≤ 1.25,
  `LOAD-DEPENDENT` iff ≥ 1.5, else `AMBIGUOUS`. Fixed point of
  ask(n) = (1 + q)·μ_R(n)·(RTprop̄(n) + S̄(n)) (S = Tp50 − Tmin; both means
  over legs) by §27's interpolation rule: `FLOOR` / `CEILING` / interior
  n_fp with its slope; **`REPRODUCES`** iff interior, inside the R1 band, and
  slope < 0.9.
- **R4, authority.** B(n) = median of `dgq[hand − tx]` summed over paths;
  **`STORE-HOLDS`** iff B ≤ 98 datagrams (1 ms of one leg's L) at every grid
  n ≤ 1.5·n*; else `QUINN-SHARES`, naming the caps. The K < c share is
  reported beside it.

**Decision rule (ships nothing).** If R2 `REOPENS` at a cell, D-W is
re-derived there with the measured σ²_win and γ and a build is proposed to
the coordinator (no build follows from this section). If at c1d R3 is
`FLAT` and `REPRODUCES` and R4 `STORE-HOLDS`, the receiver-capacity ceiling
goes to a law derivation in paper §6.1 and an ADR (the new receiver → sender
field). Otherwise the input stays open, and the report names what remains:
the link rate at c1s/c2/c8, for which only a dispersion instrument is named.

**Predictions (before any number).** R1: c1s's knee moves below §27's 1 024
(the receiver no longer holds the c1s curve up); c1d's stays near 768. R2:
`CLOSED` at c1d (§32 left its noise unchanged); c1s `AMBIGUOUS` or `CLOSED`
— `REOPENS` there would need a ×10 or larger cut from §30's 0.036. R3:
`LOAD-DEPENDENT` is the stated risk (batching); if `FLAT`, an interior n_fp
near 600–700. R4: `STORE-HOLDS` in both cells.

**Duration.** Instrument re-apply and fresh build ≈ 10 min; parser tests;
41 invocations ≈ 10–12 min (§27: 128 rows in 1 370 s); inside the 4 h launch
deadline and the 5 h session cap. No suites: the instrument tree is not a
ship tree.

**Gate.** STOPPED before VM contact. Re-applying the instrument is engine
code (default-off, observation only) and is the coordinator's call.

### 34. Amendment 1 — trees, session order and the scorer's arithmetic (before VM contact)

Coordinator go 2026-11-02. Nothing in the readouts, outcomes or decision
rule above changes; this fixes what the pre-registration left to the
harness.

*Trees.* Docs and harness branch: main `a722513` + the plan-34 harness
`7c77de3` (`tools/l1/loopdiag_*`: plan 34 in `loopdiag_battery.sh`,
`loopdiag_run_all.sh`, `loopdiag_parse.py` with its tests, and the waiter
`loopdiag_wait34.sh`; the plan 27/29 harness restored from
`archive/store-power`, tools only). Run branch `run34` = that + the
`[LOOPDIAG]` instrument `a48f25f` (bd74aa0 re-applied, unchanged; the power
law is not taken). The instrument commit is not for merge.

*Session order (one session, both locks).* The on-VM waiter launches
`loopdiag_run_all.sh` with `LD_PLAN=34` and `LD_HARD` = launch + 4 h. The
envelope waits for both locks (`test -e` only), checks the launch
precondition (`procs_running` ≤ 2 in 5 samples 2 s apart, no cargo/rustc;
exit 3 and nothing run otherwise, the waiter retries every 10 min), takes
the locks, runs the parser tests, builds the release binary fresh on disk
and copies it to `/dev/shm/ld34`, then the suites on the same source tree
in a fresh target (release `-p raptorpath -p raptorpath-math`, doc, wasm,
debug, parsers; a failure that passes its immediate solo re-run is
`FLAKE`, any other is `REAL` and stops the session as `ABORT-TESTS`), then
the smoke and the battery from tmpfs. The battery does not start with less
than 30 min to `LD_HARD` (`ABORT-BUDGET`) and its timeout ends there.

*Row witnesses as implemented.* The §27 DEF witnesses (`[GATES]`, the BBR
echo, every post-warm-up `win=` denominator = n, `[SUMCAP] eng` 0, ≥ 1
`[ACKDIAG]`) plus the plan-34 ones: ≥ 8 post-warm-up `[LOOPDIAG]` windows
with samples on every path, `dgq` on every post-warm-up `[DIAG]` line; a
row with < 3 server `[RDIAG]` lines at busy ≥ 20 % is LIVE but carries the
note `r3-void` and is left out of R3 only.

*Scorer arithmetic (`loopdiag_parse.py score34`).*
- ln Π per window = 2·ln(Σ_paths `dlv_rate`) − ln(max_paths `rtt_p50_us`),
  windows where every path reports, D ≥ ½ the row's median D (as §30's
  `win.py`). σ²_win at a cap = mean over its live rows of var(Δ ln Π)/2;
  T = median over rows of the row's median max-path Tp50; W_win = 0.25 s / T.
- γ = ln(σ²_250ms / σ²_1s)/ln 4, both statistics pooled over the cap's rows,
  the 1 s values the means of consecutive blocks of four windows.
- Π₂ per cap = mean goodput² / mean max-path Tp50 (ms); its argmax is R2's cap.
- R2's transfer = 400 MB·8 / mean goodput at that cap; a decision period
  = 2·(max(W*, 1) + 1)·T; reach = (transfer/3)/period·ln(1.1)·0.72; the
  descent needed is ln(2048/n_argmax). If σ²_win is already ≤ (q/2)², W* is
  below one window and reach is computed at W* = 1.
- R3: μ_R per cap = mean over its R3-valid rows of the row median of
  msgs/(busy/100); RTprop̄ and S̄ = means over legs of the row medians,
  averaged over rows; the fixed point by §27's interpolation
  (`fixed_point27`), `REPRODUCES` iff interior, inside R1's band, slope < 0.9.
- R4: B per cap = median over rows of the row median of Σ (hand − tx).

*Dry run (no VM).* The scorer run on §27's c1s/c1d DEF logs (no `[RDIAG]`
there, so R3 is unscoreable) reproduces §30: c1s σ²_win 0.0364 at 384,
T 4.79 ms, W*_white 760 (§30: 765), `CLOSED`; c1d 0.0333 at 384, `CLOSED`;
R4 `STORE-HOLDS` at both (B 16–66). Two §27 c1d rows would fail the new
≥ 8-window witness (5 and 2 windows), so c1d may lose rows to it.

*Duration.* Build ≈ 6 min, suites ≈ 1.5–2 h (§27: release 25 min, debug
46 min, plus builds), smoke and battery ≈ 15 min; inside `LD_HARD`.

### 34. Amendment 2 — an idle c1d leg, and the relaunch (before any scored row)

*First launch (2026-11-01 23:30Z):* `ABORT-TESTS` before any build: the
parser tests inherited the envelope's `LD_PLAN=34`, which the plan-34
witnesses key on, so 17 tests of the other plans failed. Fixed in the test
(it drops an inherited `LD_PLAN`, `3d75442`); nothing ran.

*Second launch (23:33Z–00:21Z, both locks):* build 330 s (binary
`920a92f2…a91ff`, tree `5020f80`), suites green on the instrumented tree —
release 1 143 passed / 0 failed (51 ignored), doc rc 0, wasm 35, debug
1 002 / 0 (50 ignored), parsers rc 0 (`TESTS-OK`). Then `ABORT-SMOKE`: the
c1d DEF768 smoke row delivered almost everything on leg 1 (leg 0: 4 289
egress datagrams of ≈ 341 000) and carried no post-warm-up `[LOOPDIAG]`
sample on leg 0, so the every-path window witness voided it. §27 lost the
same row the same way (c1d DEF768 s42, leg 0 with 2 windows). The c1s smoke
row was LIVE (16 windows, μ_R 124 k msgs/s, B 64). No battery row ran.

*Change (nothing in the readouts, outcomes or decision rule moves).* As
§29's c8 rule: under plan 34 a leg with no post-warm-up `[LOOPDIAG]` window
carrying samples is noted `p<i>-idle` and is not witnessed; the row is void
only if every leg is idle. A leg that carried some samples but fewer than 8
windows still voids the row. ln Π sums D and takes the max T over the legs
that carry samples. Re-checked on the two saved smoke rows: both LIVE (c1d
with the note `p0-idle`). An idle leg means the cell ran that row on one
path; the score lists those rows, so R1–R4 at c1d can be read with and
without them.

*Relaunch.* Same engine tree; harness only. The suites are not re-run: the
envelope takes `LD_PRIOR_TESTS` (the second launch's run dir, which holds
`TESTS-OK`) and copies its `TESTS.txt` in as `TESTS-prior.txt`. Fresh target
dirs (the old ones are removed while both locks are free and nothing
measures). New `LD_HARD` = relaunch + 4 h.

### 34. Diagnostic — result

Scored 2026-11-02 against the pre-registration and amendments 1–2,
literally (`score.txt` is the parser's output, unedited). Diagnostic:
nothing ships. **R1: the knee is 1 024 at both cells. R2 `CLOSED` at both,
so D-W is not reopened. R3 at c1d: μ_R `AMBIGUOUS` (max/min 1.27) and its
fixed point `MISSES` (457, band [683, 1 536]). R4 `QUINN-SHARES` at both
(c1s 768 and 1 536, c1d 1 024).** By the decision rule no branch fires:
the store law's input stays open.

*Session.* Host `cargo check -p raptorpath --all-targets` on the run tree:
clean (warnings only). Launches: first `ABORT-TESTS` (parser env, nothing
run), second suites green then `ABORT-SMOKE` (amendment 2), third
2026-11-02 01:00:59–01:11:55Z (VM clock): precondition met
(`procs_running` [1,1,1,1,1], io `full avg10` 0.00), both locks, parser
tests ok, build 341 s (binary `920a92f2…a91ff`, the second launch's binary
bit for bit; tree `d2dfa45`), suites reused from the second launch
(`TESTS-prior.txt`: release 1 143/0, doc, wasm 35, debug 1 002/0, parsers),
smoke `SMOKE-PASS`, battery 281 s, 39 rows, `DONE-ALL`; exit 0
`raptorpath`, 0 `rp-*`. Ledgers: `docs/l1-raw/loop-diag/p34/` (`run/`
the scored session, `run-abort1/` and `run-abort2/` the two stopped
launches with the second's suite logs; VM user and host scrubbed).

*Abort table.* First launch `ABORT-TESTS`, second `ABORT-SMOKE` (both
before any battery row; amendments 1–2). Third: none (`ABORT-LOCK`, `-CRLF`,
`-SHA`, `-SENTINEL-UNWRITABLE`, `-SMOKE`, `-BUILD`, `-TESTS`, `-BUDGET`,
`-RC`, `-BRINGUP`, `VOID-COTENANT`, `VOID-ONDISK` all *no*); no row
io-flagged (`full avg10` ≤ 0.18).

*Rows.* 38 of 39 LIVE; 1 `WITNESS-FAIL` (c1d DEF512 s7: leg 1 with 2
windows, neither idle nor witnessed). **7 of the 17 live c1d rows ran on
one leg** (`p0-idle` or `p1-idle`, at caps 768–2048): on post-§32 main a
static cap at c1d often leaves one leg without traffic.

*Per cap* (means over live rows; B = quinn's backlog Σ `dgq[hand − tx]`,
row medians; μ_R in msgs/s):

| c1s n | 256 | 384 | 512 | 768 | 1024 | 1536 | 2048 |
|---|---|---|---|---|---|---|---|
| goodput | 526 | 538 [277–681] | 687 | 773 | 792 | 816 | 825 |
| A_x p50 ms | 1.0 | 1.7 | 2.1 | 3.4 | 5.1 | 8.9 | 12.3 |
| σ²_win | 0.019 | 0.072 | 0.021 | 0.018 | 0.010 | 0.021 | 0.021 |
| μ_R k | 119 | 123 | 116 | 121 | 118 | 116 | 120 |
| B | 20 | 25 | 50 | 128 | 64 | 133 | 563 |

| c1d n | 384 | 512 | 768 | 1024 | 1536 | 2048 |
|---|---|---|---|---|---|---|
| goodput | 642 | 706 | 759 | 780 | 813 | 759 |
| A_x p50 ms | 1.3 | 2.2 | 9.3 | 5.3 | 31.1 | 70.7 |
| σ²_win | 0.025 | 0.028 | 0.028 | 0.026 | 0.027 | 0.030 |
| μ_R k | 86.8 | 86.9 | 108.1 | 107.2 | 110.3 | 94.2 |
| B | 29 | 22 | 69 | 192 | 83 | 228 |

- **R1.** c1s Gmax 824.5 (at 2048), n* = 1 024, band [683, 1 536], Π₂
  argmax 512 (§27: n* 1 024, argmax 384, Gmax 746). c1d Gmax 813.3, n* =
  1 024 (§27: 768), argmax 384. The receiver change raised the c1s curve by
  +2 to +14 % at the caps ≥ 512 but did not move the 0.95 knee (one c1s
  DEF384 row read 277 Mbit/s and pulls that cap down).
- **R2 `CLOSED` at both.** c1s at 512: σ²_win 0.0205 (§27 at 512: 0.0277;
  §30's 0.036 was at 384), T 5.77 ms, γ 0.53 (still red), W*_white 355,
  W*_γ 2 374; reach in a third of the 4.7 s transfer 0.026 against the
  1.386 needed. c1d at 384: 0.0246 (§30 0.029), γ 1.17, W* 358–502, reach
  0.02–0.03 against 1.674. §32 cut the c1 window noise by ×1.2–1.8 at the
  argmax; D-W needed ×100 or more.
- **R3 (c1d scored).** μ_R 86.8 k at 384–512, 107–110 k at 768–1 536,
  94 k at 2048: max/min 1.27, `AMBIGUOUS` (`LOAD-DEPENDENT` at ≥ 1.5), the
  step sitting where the idle-leg rows begin. The ceiling ask
  (1 + q)·μ_R·(RTprop̄ + S̄) has a stable fixed point at 457 (slope 0.54),
  below the band: `MISSES`. Reported only, c1s: μ_R flat (116–123 k,
  max/min 1.06, 1.2 L — the receiver is not c1s's bottleneck) and the same
  form lands at 1 020 (slope 0.62), inside the band, because μ_R·(RTprop +
  S) happens to equal L·(RTprop + S) within 20 % there; with L itself §27
  read 701.
- **R4 `QUINN-SHARES`.** quinn's own backlog stays ≤ 50 datagrams (≤ 0.5 ms
  of L) at caps ≤ 512 at both cells, and reaches 128–192 (1.3–2 ms of L) at
  the knee caps (c1s 768 and 1 536, c1d 1 024). On the post-§32 receiver the
  store cap is the only brake below ≈ 512 and shares authority with quinn's
  window from the knee up; the K < c share reads 0.35–0.90 from 384 up at c1s.

*Predictions.* R1: c1s's knee moves below 1 024: MISSED (it stays at
1 024; the curve rose instead); c1d stays near 768: MISSED (1 024). R2
`CLOSED` at c1d: MET; c1s `AMBIGUOUS` or `CLOSED`: MET (`CLOSED`). R3
stated risk `LOAD-DEPENDENT`: MISSED narrowly (`AMBIGUOUS`, 1.27); the
interior n_fp near 600–700: MISSED (457). R4 `STORE-HOLDS`: MISSED at both.

*What it means for the store law (outside the decision rule, no
verdict).* (1) D-W is closed at c1 on post-§32 main as well; the noise fell
a little and stayed red at c1s. (2) The receiver capacity is not a usable
c1d ceiling: it is load- and leg-dependent and its fixed point sits below
the knee. (3) §34's committed-ledger reading that the store cap holds
authority at every knee (from §27/§29, pre-§32) no longer holds at c1: from
the knee up quinn's window shares it, so a nested store law there would
need the authority gauge B and the rejection rule of paper §6.1. (4) c1d
often runs on one leg at a static cap, which no store law can fix and
which makes c1d's knee partly a placement question. What remains is the
link rate at c1s, c2 and c8, for which only a dispersion instrument is
named. The `[LOOPDIAG]` instrument stays off main (run branch only).
