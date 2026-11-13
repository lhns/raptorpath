## 40. The p90 dispersion estimator, fixed in advance, with aggregation and policer cells — capture diagnostic — pre-registration

§36 failed both registered estimators. Its exploratory addendum found a lead:
the per-window p90 of k = 4 train rates never exceeded 1.032 of the link,
read it from a cap crossover up, and read 1.6–3.9× the delivered rate below
that crossover, which keeps a store-law map out of the floor. That reading
was post hoc. This section tests it fresh, with the estimator fixed now.
It adds the two real-link failure modes the literature names: aggregation
(netem `slot`) and a token-bucket policer (`tbf`). Diagnostic: no engine
diff, nothing ships, no law is built.

**Tree, capture, rules.** As §36 after amendment 6: main's engine (the
diff outside `docs/` and `tools/` is empty); unfiltered `-Q in` capture
with drain and settle; windowed `[SNAP]` witnesses at 1 %; root-owned
tmpfs and the per-row `ABORT-BIN` sha check; the §36 launch precondition,
locks and abort table. Static cap `RWM_STORE=<n>`, Bulk,
`--window-reliable`, `RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=300`.

**Estimator (fixed; it uses no L).** Per leg, all skbs in arrival order, b
as §36, after a 1 s warm-up. For each 250 ms window, take the p90 of
`r_4(j) = Σ_{i=j+1..j+4} bᵢ / (t_{j+4} − t_j)` over trains starting in
it. The row value L̂ is the median over windows.

**Cells.**

| cell | caps | sustained rate (B/s) | role |
|---|---|---|---|
| `c1s-400` | {16, 32, 64, 128, 256, 512, 1024, 2048} | 125·10⁶ | scored |
| `c2-100` | {16, 32, 64, 128, 256, 512} | 12.5·10⁶ | scored |
| `c8-100` | {16, 32, 64, 128, 256, 512, 1024} | 12.5·10⁶ / 2.5·10⁶ | scored, both legs |
| `c2slot` | {32, 128, 512} | 12.5·10⁶ | aggregation, reported |
| `c2tbf` | {32, 128, 512} | 6.25·10⁶ | policer, reported |

- `c2slot` is c2's netem plus `slot 1ms 3ms packets 16`: release in bursts
  of ≤ 16 packets at 1–3 ms intervals.
- `c2tbf` is c2's netem (100 Mbit/s) plus a child `tbf rate 50mbit burst
  125000 latency 50ms`: a policer whose burst drains at the netem rate.
- Each cell runs seeds 42 and 7, two reps each: 4 rows per cap, 108 rows,
  plus a 2-row smoke (c2 CAP128, c2tbf CAP128).
- Block order as §36: ascending, then descending, cells alternated.

**Witnesses.** As §36, amendment 6. For `c2slot` and `c2tbf`, the
skb/byte witnesses read the clsact counter against the root qdisc's
`Sent` (the qdisc tree), with the tolerance unchanged.

**Readouts and outcomes.** Q(p, n) = mean over the cap's live rows of
L̂ ÷ sustained rate. D(p, n) = delivered bytes ÷ time from the same capture.
Crossover caps, fixed from §36: 128 on c1s, c2 and c8 fast; 256 on c8
slow.

- **E1, above the crossover.** Q ∈ [0.90, 1.10] at every cap ≥ the
  crossover.
- **E2, never over.** Q ≤ 1.10 at every cap.
- **E3, floor escape.** (1 + q)·L̂/D ≥ 1.2 at every cap below the
  crossover, with q = 0.1. Per row; the cap's minimum is reported.
- **E4, row agreement.** At every cap ≥ the crossover, the 4 rows' L̂ agree
  within max/min ≤ 1.05.
- **E5 (reported).** Within-row cv over windows, the in-run share, and the
  same readouts for the window median.
- **E6 (reported).** Medians of the `p<i>:` µs fields `sig_us`, `rvar_us`,
  `qsp_us`, `msd_us` and `tlag_us` per cap, toward the open question of
  dwell in S (§36 addendum).
- **Aggregation and policer.**
  - `c2slot` and `c2tbf` get E1 and E2 against their sustained rates,
    reported.
  - `AGG-SAFE` iff both hold E2. Otherwise `AGG-OVER-READ`, with the
    factor.

**Decision rule (ships nothing).**
- `P90-READS-LINK` iff E1–E4 hold at c1s, c2 and both c8 legs. The store
  law can then take L̂ from dispersion between the floor and the knee, and
  the next step is the joint derivation with it. A dwell-free S is still
  missing, so no build follows.
- If, in addition, `AGG-OVER-READ`, the law also needs an aggregation bound
  (a windowed excess, as BBR's `extra_acked`) before any build.
- Otherwise `NOT-YET`, naming the failed cells and clauses.

**Predictions (before any number).**
- E1, E2 and E3 hold at c2 and c8 (§36: p90 1.00–1.01 above the crossover,
  L̂/D ≥ 1.6 below it).
- At c1s, E1 and E2 hold (§36 max 1.032). E4 is the stated risk there,
  because the batched release spreads the rows.
- `c2slot` over-reads (bursts of 16 at line rate): `AGG-OVER-READ`,
  ×1.5–3.
- `c2tbf` reads the netem rate inside bursts: ≈ ×2 over its sustained rate.
- So the expected verdict is `P90-READS-LINK` with `AGG-OVER-READ`.

**Duration.** Build ≈ 6 min; 110 invocations ≈ 60–75 min (c8 at 16 ≈
100 s). Inside the 4 h launch deadline and the 5 h cap.

### 40. Result

Scored 2026-11-10 against the pre-registration, literally
(`docs/l1-raw/disp40/score.txt`). Diagnostic: nothing ships.
**`NOT-YET`**, failed on one leg only: c8's slow leg, where E3 fails at cap
32 and E4 is unscoreable at 256. **E1–E4 all pass at c1s, c2 and c8's fast
leg. `AGG-OVER-READ`, ×2.05 (`c2slot`); `c2tbf` does not over-read.**

*Session.* 08:31:04–09:36:11Z. Precondition met, both locks, gate ok,
build 338 s (binary `79572edd…edfff5`, the §36 engine = main), smoke ok,
battery `DONE-ALL`. 108/108 rows LIVE, all rc 0, io `full avg10` ≤ 0.49.
Abort table: none. tc: both scenarios valid as written (`c2tbf` uses netem
`root handle 1:` with the tbf as child `10:`). The root `Sent` equals the
child's, so the windowed witnesses held unchanged.

*Readings (Q = mean L̂ ÷ sustained rate; 4 rows per cap).*

| cell / leg | Q below the crossover | Q at and above the crossover | E3 min (1+q)·L̂/D | E4 max/min ≥ crossover | verdict |
|---|---|---|---|---|---|
| c1s | 0.09, 0.30, 0.50 (16–64) | 1.001–1.038 (128–2 048) | 1.93 | ≤ 1.013 | E1–E4 PASS |
| c2 | 0.17, 0.54 (16–32); 1.000 at 64 | 1.009 (128–512) | 1.65 | ≤ 1.001 | E1–E4 PASS |
| c8 fast | 0.17, 0.50; 1.00 at 64 | 1.009–1.013 | 1.12 | ≤ 1.005 | E1–E4 PASS |
| c8 slow | 0.008–0.20 (16–128) | 0.995–1.011 (256–1 024) | 0.81 at 32 | 256: 3 rows only | E3 FAIL, E4 UNSCOREABLE |
| c2slot | 0.51 (32) | 1.62 (128), 2.05 (512) | 3.07 | ≤ 1.023 | E1, E2 FAIL |
| c2tbf | 1.09 (32) | 1.01 (128, 512) | 1.36 | ≤ 1.004 | E1–E4 PASS |

- **c8 slow leg.** Three rows had the slow leg idle (CAP128 s42 ×2, CAP256
  s7). At cap 32 one row's L̂/D gives 0.81. That leg carries ≈ 2 % of the
  traffic below the crossover by the placement law (§36 D7/D8), so its
  floor escape is set by placement, not by the store.
- **E6** (the µs fields at c1s; meaning per `src/net/diag.rs`, not yet
  read):
  - `sig_us` and `rvar_us` fall from 64 to 15 µs as the cap grows;
  - `qsp_us` rises from 139 to 738 µs up to 512 and then falls;
  - `tlag_us` rises from 91 µs to 2.24 ms;
  - `msd_us` stays at 2.

*Predictions.*
- E1–E3 at c2 and c8, and E1/E2 at c1s: MET, except E3 on c8's slow leg.
- E4 the risk at c1s: MISSED (it passes at ≤ 1.013).
- `c2slot` over-reads ×1.5–3: MET (×1.62–2.05).
- `c2tbf` ≈ ×2: MISSED (1.01). netem releases the policer's burst into the
  tbf child at the netem rate and the tbf passes it on at its own token
  rate, so no burst reaches the wire faster than the sustained rate here. A
  policer upstream of a faster link was not tested.
- Expected verdict `P90-READS-LINK` with `AGG-OVER-READ`: MISSED, on the
  slow leg only.

*What it means (outside the decision rule, no verdict).* The p90 estimator,
fixed in advance, reads the link within +4 % on every leg that carries its
share of traffic. That holds from a cap crossover up, with no over-read and
row agreement within 1.3 %. Below the crossover it reads ≥ 1.5 × the
delivered rate on those legs, so a store law fed by it expands out of the
floor.

What fails is a placement-starved leg, which forms no trains to read: the
§38 lock-out and the placement law's own low share. Aggregation (`slot`)
over-reads ×2, so a law built on L̂ needs an aggregation bound before any
build, as §40's decision rule names. The store law still lacks a
dwell-free S (§37 addendum). Next: the joint law with L̂, an aggregation
bound and a placement-aware rule for a leg with no trains, derived on paper.

### 40. Result — addendum (offline, exploratory; the verdict stands)

The registered rule licenses no next step. What follows is offline reading
of the ledger, and it is labelled as such.

- **c8's slow leg and the law.** The joint derivation (§37) found that the
  slow leg's credit in the pooled Σ funds the queue that activates it, so
  its failure is not only placement's.
  - §36's D5 rerun offline with §40's p90 Q on both legs: Q interpolated in
    n, RTprop and S from p27 (cross-session).
  - Both legs: STABLE at 329, slope 0.53 (oracle 324).
  - With the slow leg's credit removed: STABLE at 160 (the fast-leg-only
    point; goodput there ≈ 65–79 against 103).
  - The p90 map still lands at the oracle point, because at 128 its ask
    (169) is above n and the slow leg reads its link from 256.
  - This holds on the static grid only. It assumes Q between 128 and 256 as
    interpolated and no lock-out (§38). Three of the 28 c8 rows had the slow
    leg idle.
  - The rule for a leg with no trains stays the open piece.
- **E5 (registered, omitted from the result): the window median is the
  aggregation-robust candidate.**
  - At `c2slot` it reads 0.20 / 0.78 / 1.00 where p90 reads 0.51 / 1.62 /
    2.05.
  - Its cost: a crossover one grid step higher, and a weaker floor escape
    (§36: L̂/D 1.06–1.45).
- **The policer is not cleared.** `c2tbf` stays within 1.10 only at the
  three caps run. At cap 32, 36 % of pairs sit at the netem rate (twice
  sustained) and one row reads 1.114. Caps between 32 and 128, where load
  sits just under the token rate, were not run. The result's mechanism
  sentence (no burst faster than sustained) is withdrawn.
- **Errata (none changes the verdict).**
  - *E3 column:* it took the minimum over all caps. Below-crossover minima
    are c1s 1.93, c2 1.79, c8 fast 1.81 and `c2tbf` 2.95. The 1.12 shown for
    c8 fast is at cap 256, above its crossover.
  - *D:* defined over the whole post-warm-up capture, D counts idle capture
    time (§36's explore reads 0.33 of link at c1s 256, where goodput was
    599 Mbit/s). The E3 verdicts survive because the minima sit at cap 16,
    where transfers run about a minute. L̂/D at caps ≥ 64 is not to be
    carried into a derivation until D is recomputed over the data span.
  - *E6:* `sig_us`, `rvar_us`, `qsp_us`, `msd_us` and `tlag_us` are the RTT
    dispersion (σ) candidates of paper §7.4 on the app-echo samples
    (`src/net/diag.rs` ~630–690). They are not dwell measures and do not
    settle dwell in S.

### 40. Standing decision — δ's authority on the store law (user, 2026-11-10)

The store law's δ dial moves its fixed point by only ≈ 5–10 % across the
named points: q(δ) is its only δ term, while the power argmax moves ×2
between α = 1 and 2 (§37). The user accepted this **for now, to be
revisited**. It is reopened when a store law with a measured input is
derived (after §40), or when the δ → latency trade-off is measured on the
wire. The revisit is to decide whether δ needs authority beyond q, for
example a δ-indexed quantile in S, as the joint derivation suggested.
