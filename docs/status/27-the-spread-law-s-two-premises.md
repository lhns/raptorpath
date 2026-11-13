## 27. The spread law's two premises — the L source and a cap-flat spread — diagnosis — pre-registration

§24 found no input for the store law and left one candidate outside its set:

```text
   cap  =  (1 + q(δ)) · Σᵢ Lᵢ · ( RTpropᵢ + (Tp50ᵢ − Tminᵢ) )
```

with `Tp50 − Tmin` the spread of the per-window loop-time distribution
(cap-flat at c2 across a 16× cap range in §24, while Tmin moved 10×). It
rests on two premises, both untested: (1) some non-circular source reads the
link rate Lᵢ under the store cap; (2) the spread stays flat across the cap at
every cell, so the law has a stable interior fixed point. This
instrument-only session tests both and the reproduction they imply. Nothing
ships or flips; no law is built.

**The L source.** The in-tree burst-robust BBR substrate controller
(`src/transport/bbr_rs.rs`, ADR-0054/0061: quinn's BBR with its bandwidth
estimator replaced by the interval-guarded per-flight sampler,
max(send, ack) elapsed and ≥ RTprop intervals). It is switched on by the
substrate CC selection `RWM_QUIC_CC=bbr_rs`, so it runs **only as a
measurement arm (`BRS`)**: it changes the substrate controller, not the
store law; the store cap is the static `RWM_STORE=<n>` of the sweep in both
arms. Its rate is read as `[LOOPDIAG]` `q_pace_bps` (its
`metrics().pacing_rate` = pacing_gain·max_bandwidth; the window median
reads the gain-1 phase). Stated risks: it is app-limited by our store cap;
and like upstream it never lowers the pacing rate before it declares full
bandwidth, so a startup over-shoot can persist.

**Instrument and binary.** The `[LOOPDIAG]` instrument (`f5c200d`, the §24
tree re-applied onto main `de3e11a`) and nothing else; one binary, built
fresh on disk, run from `/dev/shm`.

**Design.** Arms `DEF<n>` (shipped substrate BBR, `RWM_STORE=<n>`) and
`BRS<n>` (`RWM_QUIC_CC=bbr_rs RWM_STORE=<n>`). Everything else as §24
(Bulk, `--window-reliable`, `RWM_GEN=0 RWM_DIAG=1 RWM_ACKDIAG=1
RWM_LOOPDIAG=1`, §24's withholding plus `RWM_QUIC_CC`). Grids:
`c1s-400` {192 … 2048} and `c2-100` {64 … 1024} as §23/§24; `c1d-400`
{384, 512, 768, 1024, 1536, 2048, 3072, 4096}; `c8-100` {128, 192, 256,
384, 512, 768, 1024, 2048}. Two blocks (seed 42, then 7); per block the
cells c2, c1s, c8, c1d, each DEF then BRS (BRS first on the odd block), each
sweep ascending on the even block and descending on the odd one. 128 rows
after a 2-row smoke (`c2-100` BRS192, `c8-100` DEF512). Harness
`tools/l1/loopdiag_*` plan 27, waiter `loopdiag_launch27.sh`.

*Rules (shared file):* precondition = both locks free (`test -e` only), no
cargo/rustc (`pgrep -x`), `procs_running` ≤ 2 in each of 5 samples 2 s
apart; run dir and binary on `/dev/shm`, build on disk; I/O `full avg10`
before/after each row recorded and flagged at ≥ 20, never voided; a row off
tmpfs is `VOID-ONDISK`.

*Witnesses (row void otherwise):* `[GATES]`; ≥ 4 post-warm-up `[LOOPDIAG]`
windows with samples for every path of the cell, and a numeric `q_pace_bps`
on each; `[SUMCAP] eng` absent or 0 and every post-warm-up `win=`
denominator equal to n; DEF: the stock-BBR echo and no `burst-robust BBR`
echo; BRS: the `burst-robust BBR` echo.

*Abort table:* as §23/§24 (`ABORT-LOCK`, `-CRLF`, `-SHA`,
`-SENTINEL-UNWRITABLE`, `-SMOKE`, `-BUILD`, `-RC`, `-BRINGUP`,
`VOID-COTENANT`, `VOID-ONDISK`), filled in the result.

**Readouts and outcomes (fixed now).** Path links: c1 legs 125·10⁶ B/s,
c2 12.5·10⁶, c3 (c8's p1) 2.5·10⁶; W = 1 279 B per datagram; Lₚ = link/W;
q = 0.1. Per cell, arm, path and n: the mean over live rows of each row's
median over post-warm-up windows. Knee per arm: n* = the smallest n with
goodput ≥ 0.95·Gmax; band [n*/1.5, 1.5·n*].
- **Q1, the L source (BRS rows).** Q(p, n) = bbr_rs rate ÷ link.
  `FLAT-AT-LINK` iff every Q(p, n) ∈ [0.8, 1.25]; else `CIRCULAR` iff some
  path has Spearman(Q, n) ≥ 0.8 and Q(n_min) < 0.8; else `OVER-READ` iff the
  median over (p, n) > 1.25; else `APP-LIMITED` iff that median < 0.8; else
  `NEITHER`. The same classification of stock BBR on DEF rows is printed as
  a reference (§24 read it ×7.6 at c2).
- **Q2, the spread (DEF rows).** S(p, n) = Tp50 − Tmin. A path is `FLAT`
  iff max_n S / min_n S ≤ 1.5, `INFLATES` iff S(n_max)/S(n_min) ≥ 2.0,
  `AMBIGUOUS` otherwise; a cell is `FLAT` iff every path is, `INFLATES` iff
  any path does. Fixed point on the true link: ask(n) = 1.1·Σₚ Lₚ·(RTpropₚ(n)
  + Sₚ(n)); `FLOOR` if ask(n_min) < n_min, `CEILING` if ask(n_max) ≥ n_max,
  else n_fp by linear interpolation of ask − n across the first sign change
  and the local map slope = Δask/Δn there: `FRAGILE` iff slope ≥ 0.9, else
  `STABLE`.
- **Q3, reproduction (BRS rows).** The same fixed point with Lₚ replaced
  by bbr_rs's measured rate Q·link/W and S, RTprop from the BRS rows.
  `REPRODUCES` iff the interior n_fp lies in the BRS knee band (`FLOOR` and
  `CEILING` miss); the DEF band is reported.

**Decision rule (ships nothing).** `PREMISES-HOLD` iff at all four cells
Q1 = `FLAT-AT-LINK`, Q2 = `FLAT` and `STABLE`, and Q3 = `REPRODUCES`;
then the law can be written in the paper and pre-registered as a build.
Otherwise `PREMISES-FAIL`, naming the failed cells and clauses: a failed
Q1 means the law has no rate input; a failed Q2 means its time term is not
a property of the path; the report names what that leaves.

**Predictions (before any number).** Q1: `APP-LIMITED` or `CIRCULAR` at
c1s/c2 at the small caps (bbr_rs sees only what our cap lets through),
`FLAT-AT-LINK` unlikely below the knee. Q2: `FLAT` at c2 (as §24) and c8's
fast leg; `INFLATES` at c1s (§24: 1.1 → 11.4 ms) and c1d (receiver-bound).
Q3: `MISSES` wherever Q1 fails. So `PREMISES-FAIL` is the expected verdict.

**Suites in the same session** (the instrument tree, so it can merge):
after the battery and its ledger are persisted, on the same on-disk tree
with a fresh target: release `cargo test -p raptorpath -p raptorpath-math
--release --no-fail-fast -- --test-threads=2`, `cargo test --doc -p
raptorpath --release`, `cargo test -p raptorpath-wasm`, debug `cargo test
-p raptorpath --no-fail-fast -- --test-threads=2` (with the identity-panic
count), and the parser tests; a failing test is re-run solo (`FLAKE` if it
passes). A suite failure is reported; it does not void the battery.

**Duration.** Build ≈ 6–30 min; 130 invocations ≈ 25 min; suites ≈ 2 h
(§21: release 34 min, debug 77 min); under the 5 h session cap, inside the
4 h launch deadline for the battery.

### 27. Amendment 1 (before any row ran): the first launch is `ABORT-TESTS`; relaunch

The first launch met the precondition at 23:25:48Z, took both locks at 23:25:57Z and stopped one second later on `ABORT-TESTS`: the parser test (`test_loopdiag_parse.py`, 18 of its checks) failed because the shipped source archive excluded `docs/l1-raw/`, and the test reads its base line shapes from saved logs in `docs/l1-raw/store-law-n1/diag-sl/` and `docs/l1-raw/anchor-diag/diag-ad/` (the same test passes on the host). A packaging fault, not a code or parser fault: no build, no smoke, no row ran, nothing is scored. The ledger of the abort is kept in the persisted run dir. Fix: the two fixture directories are shipped with the tree; nothing in the harness, the plan or the outcomes changes. Relaunched under the same precondition.

### 27. The spread law's two premises — result

Scored 2026-10-27 against the pre-registration and amendment 1, literally.
Diagnostic: nothing ships. **`PREMISES-FAIL`.** Neither premise holds at
all four cells: bbr_rs reads the delivered rate, not the link (Q1 never
`FLAT-AT-LINK`; `CIRCULAR` at c2), and the spread inflates with the cap
everywhere except c2 (Q2). Q3 misses at every cell.

*Session.* Relaunch after amendment 1: the waiter met the precondition at
08:57:32Z (load 0.36, io `full avg10` 7.8 %); locks held from 08:57:33Z;
build 314 s; smoke 09:03:11Z (`SMOKE-PASS`); battery 09:03:11–09:26:01Z
(1 370 s, 128 rows); ledger persisted 09:26:01Z; the suites ran after it
in the same session (below). Binary `10c0fa64…d7ba68` (engine = main +
the `[LOOPDIAG]` instrument). Ledgers: `docs/l1-raw/loop-diag/p27/`.

*Abort table.* First launch: `ABORT-TESTS` (amendment 1; nothing ran).
Relaunch: no token fired; 0 `ABORT-RC`, 0 bring-up retries, 0
`VOID-ONDISK`; no row io-flagged (`full avg10` < 20 throughout).

*Rows.* 115 of 128 LIVE; 13 `WITNESS-FAIL`: 12 at `c8-100` (DEF 128, 256,
256, 384, 512, 768; BRS 192, 192, 256, 256, 384, 384), every one because
the slow path p1 carried no RTT samples in ≥ 4 post-warm-up windows (the
placement sent it nothing at those caps), and one at `c1d-400` (DEF768 s42,
p0 had 2 windows). They are void per the witness rule; c8 is scored on the
remaining points (DEF n = 256 and BRS n = 192–384 have no live row). The
score file's `ROWS` and `VOID` tables list them (an earlier note here called
them empty; that was a filter in the reader's extract, not the parser).

*Q1, the L source (bbr_rs rate ÷ link, BRS rows).*

| cell | Q over the grid | Spearman(Q, n) | class |
|---|---|---|---|
| c2-100 | 0.40, 0.55, 0.67, 0.80 at n ≥ 192 | 0.91 | **`CIRCULAR`** |
| c1s-400 | 0.74–1.02 (median 0.87) | 0.48 | `NEITHER` |
| c8-100 | p0 0.66–0.81; p1 0.78–2.07 | 0.40 / −1.0 | `NEITHER` |
| c1d-400 | 0.50–0.97 (median 0.86) | 0.64 / 0.64 | `NEITHER` |

bbr_rs does not over-read (stock BBR reads ×7.3 at c2 and ×9.1 at c8, as in
§24); it reads what is delivered. At c2 it tracks the cap below the knee and
sits at 0.80 above it, which is the delivered 87 Mbit/s, not the 100 Mbit/s
link. An honest sampler of an app-limited flow cannot see the headroom the
cap withholds — the §23 circularity, one layer down.

*Q2, the spread (DEF rows), with the fixed point on the true link.*

| cell | S = Tp50 − Tmin over the grid | class | ask fixed point | slope | DEF band |
|---|---|---|---|---|---|
| c2-100 | 4.9–6.8 ms (max/min 1.39) | **`FLAT`** | 164 `STABLE` | 0.25 | [128, 288] |
| c1s-400 | 1.3–10.2 ms (×7.9) | `INFLATES` | 701 `STABLE` | 0.82 | [683, 1 536] |
| c8-100 | p0 5.3–27.8 ms (×5.3), p1 9.3–118 ms (×12.7) | `INFLATES` | 324 `STABLE` | 0.52 | [256, 576] |
| c1d-400 | 2.2–17.4 ms (×7.8 / ×7.4) | `INFLATES` | `CEILING` (ask > n to 4 096) | – | [512, 1 152] |

*Q3, reproduction with bbr_rs's rate (BRS rows).* c2 `FLOOR` (ask 55–152,
below n everywhere: the circular rate starves it); c1s n_fp = 500 (band
[683, 1 536]); c8 n_fp = 336 (band [341, 768]; inside the DEF band, reported
only); c1d n_fp = 2 957 (band [683, 1 536]). **`MISSES` at all four.**

*Predictions.* Q1 `APP-LIMITED` or `CIRCULAR` below the knee: MET in
substance (`CIRCULAR` at c2; elsewhere `NEITHER` — it reads the delivered
rate, below the link). Q2 `FLAT` at c2: MET; `INFLATES` at c1s and c1d: MET;
`FLAT` on c8's fast leg: MISSED (×5.3). Q3 misses wherever Q1 fails: MET.
`PREMISES-FAIL`: MET.

*What it implies for the law (outside the set, no verdict).* (1) The rate
premise is the binding one. Three samplers now read the same thing — ours
(§23), quinn's stock BBR (§24, over-reads under jitter) and bbr_rs (this
section, reads the delivered rate) — and none sees a link the store cap
does not let the sender reach. A store law of the form `rate × time` cannot
take its rate from the loop it closes; it needs an exploration term, i.e. a
probe. (2) The time premise holds only at c2. Elsewhere the spread is
the queue's own variance: it grows with the cap, receiver-bound at c1s/c1d
and per leg at c8. With the true link the map is still stable at three cells
(slopes 0.25–0.82) and lands in the knee band at c2, c1s and c8, so the
spread term is not useless; but c1d runs to `CEILING`, and c1s's slope 0.82
is near the 0.9 fragility line. (3) Together: there is no measured,
non-circular input for this law in the current engine; the next candidate
must either probe (bounded, continuous in δ — a gain on the cap that is
always present, not a phase) or close the loop on delay rather than rate
(grow the cap while the queue delay `Tmin − RTprop` stays under the dial's
q·RTprop). The data here can pre-check the second offline: at the knees,
Tmin/RTprop reads 1.32 (c2), 1.56–1.98 (c1s 768–1024) and 1.21–2.19 (c1d
768–1024), so one fixed ratio does not mark every knee.

*Suites (the instrument tree, same session, after the battery; `TESTS.txt` and the logs in `docs/l1-raw/loop-diag/p27/`).* Release `cargo test -p raptorpath -p raptorpath-math` **1 111 passed, 0 failed**, 50 ignored (1 522 s); `--doc` rc 0; `raptorpath-wasm` **35 passed**; debug `cargo test -p raptorpath` 969 passed, 1 failed, 49 ignored (2 782 s), `IDENTITY-PANICS 0`. The one failure, `place_arms_reachability::the_derived_temperature_arm_resolves_a_finite_t_eff_on_the_dual`, passed on the immediate solo re-run: `FLAKE` by the rule (the same timing-dependent place-arm test §21 recorded; no store-cap or `[LOOPDIAG]` code on its path). Parser tests rc 0. `TESTS-OK`, `DONE-ALL` at 10:40Z; VM dirs removed.

> Coordinator note: the `[LOOPDIAG]` instrument and the power law (D) are not on main; they are kept at tag `archive/store-power`.
