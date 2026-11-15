## 32. The decoder prune per drain (the measured R3 subset) — ship battery — pre-registration

§22 left the c1d receiver task as the ceiling (decode ≈ 0.31–0.32 core,
`[RDIAG]` busy 95–100 %) and measured two costs of the 2 s prune cadence:
an in-order source `add_symbol` costs 2.15× more than with a prune every
1 000 symbols (H-alloc (b)), and the prune itself is a 63–90 ms stall
(H-burst). Its deciding witness for the full in-place R3 (H-alloc (a)) was
never read, so the full R3 stays shelved at `archive/thread-r3`. This
section takes only the part §22 measured: evaluate the existing prune law
at every receiver wait instead of on the report tick. Committed before any
VM contact. **The battery is a ship battery and is stopped at the gate
until the operator says go.** No number below is a result.

**The change (`PD`, branch `worktree-agent-a1973fd76fa746af3`: main 71aa3ee
+ §31's re-delivery fix 17824f7 (cherry-picked) + PD; paper §5.2 "Retained
state: the decode-inert floor, applied per drain").**
- *Law, unchanged.* F = (next_expected − 1) − 2·W_cap, saturating, with
  the existing `RWM_REASM_BDP` clamp. No new constant, no mode, nothing
  reads δ, ρ or N.
- *When.* At every wait of the receiver loop (once per drained `recv_many`
  batch), before the staged-ack flush; the 2 s block keeps its other duties.
- *Decoder floor.* §31 gave every decoder an advance floor (no seq returned
  below it; a source below it refused). PD adds, on the unified decoder,
  the refusal of a repair whose span ends at or below it and the count of
  both refusals (`below_floor`). The prune is by range, O(pruned): the wire dedup set
  becomes an ordered set with the same key, so no call scans the retained
  set.
- *Behaviour change, declared.* On §31's base the floor already refuses a
  source below it; what moves is the floor. On the 2 s tick it lagged the
  frontier by 2·W_cap plus up to 2 s of seqs (a sawtooth); per drain it
  lags by 2·W_cap plus at most one drain. So a symbol more than 2·W_cap
  seqs behind the frontier is now refused at once: at ρ = 1 such a source
  is a duplicate of a delivered seq either way; at ρ < 1 it can be a late
  fill of a hole the frontier gave up on, which was accepted until the
  next tick and is now refused. A repair whose span ends at or below F is
  refused (before: assimilated as a row of pruned columns and dropped at
  the next prune). `[RFA]` reads a refused source arrival as a fill; that
  count is bounded by `below_floor`.
- *Not taken from R3.* Zero-copy payloads, the delivered set folded into the
  payload store, the per-entry source flag, bounded covering-row lookups,
  the reused worklist.
- *Witness* (`RWM_RDIAG` only): the server `[RDIAG]` line appends
  `dec[… recovered= … floor= below_floor=] wcap= prune_n= prune_max_us=`
  (prunes since the last line; the run's longest prune). Under
  `RWM_RDIAG=1` NEW pays two `Instant::now()` per wait for this witness
  that MAIN does not: a between-arm instrument difference, small and
  against NEW. `prune_max_us` is a wall-clock maximum, so a preemption
  inside one prune can set it; the reading that matters is its order of
  magnitude against §22's 63–90 ms, not the 5 000 µs bar below.

**Tests (red first; run on the VM under both locks before this battery,
reported in the gate report, not results).**
- `fec::unified` units: a source below the floor is refused and counted; a
  repair whose span ends at or below the floor is refused and counted, one
  that straddles it is not; the retained sets never hold a key below the
  floor and stay within 2·W_cap + 1 + one drain (+ holes above the
  frontier) under per-drain advances with late duplicates; delivery is
  exactly-once with the source bytes under loss, repairs, immediate and
  late duplicates and reordering (seeded). On the base (71aa3ee + 17824f7
  + the witness commit) the two counting tests are red (`below_floor=0`);
  the retained-state and exactly-once tests are green there, because
  17824f7 already guarantees them (they were red on 71aa3ee alone: a key
  below the floor retained; seq 2 delivered twice). They stay as guards of
  that contract under per-drain advances.
- `tests/receiver_prunes_per_drain.rs` (routing gate, rule 1): one decoder
  prune site, at the wait point, guarded by `waiting`, none in the
  REPORT_INTERVAL block; the `[RDIAG]` witness fields exist.
- `tests/prune_per_drain_loopback.rs`: a clean loopback transfer; every
  server `[RDIAG]` line has `recovered ≤ 2·wcap + 4 096` (one drain ≤
  `MSG_CHANNEL_DEPTH`) and some line has `prune_n ≥ 2` (the 2 s tick gives
  at most 1 per 500 ms window). Red on the witness-only commit (prune
  still on the tick), green on PD.
- Full suites on PD: release, doc, wasm, debug, parsers.

**Ship battery.** `MAIN` = main at the merge of §31's fix (it carries
17824f7; the per-drain change is then the only engine difference) vs
`NEW` = PD rebased onto it. If §31 has not merged at launch, the battery
does not launch; an amendment names the trees first.
- *Cells and n.* `c1s-400`, `c1d-400`, `c2-100`, `c8-100`; n = 6 per arm
  and cell (3 reps × seeds 42 and 7); arm order rotated per block.
- *Env.* §28's (`RWM_RTOBS=1 RWM_RDIAG=1 RWM_GEN=0 RWM_DIAG=1
  RWM_PERF_TIMEOUT_S=150`, the standard `env -u` list), bulk,
  `--window-reliable`, shipped defaults.
- *Harness.* `tools/l1/threadpd_run_all.sh`, `threadpd_battery.sh`,
  `threadpd_parse.py` (tested by `test_threadpd_parse.py`), derived from
  §28's `threadsk_*`, scoring through `l1ab.py`.
  - Session order: NEW build and full suites (release, doc, wasm, debug,
    parsers; a REAL failure is `ABORT-TESTS`), the decoder cadence bench
    once (`bench_in_order_add_symbol_against_the_prune_cadence`, cadences
    140 k / 1 k / 32; reported; a failed exactly-once assert is
    `ABORT-TESTS`), MAIN build (fresh target), smoke, budget, battery,
    score.
  - Launch precondition (agent rules): both locks free (`test -e` only),
    no `cargo`/`rustc` (`pgrep -x`), `procs_running` ≤ 2 in each of 5
    samples 2 s apart; loadavg not used. Both locks for the whole session.
  - Measurement from `/dev/shm/tpd` (binaries and run dir) with the
    `ABORT-ONDISK` guard; ledgers copied to disk after the last row.
  - io `full avg10` recorded before and after every invocation as a
    covariate; rows that are goodput or wall outliers while it is high are
    flagged, not voided.
- *Binary witness (two-sided, at launch).* `below_floor=` and
  `prune_max_us=` in NEW's binary; `below_floor=` absent from MAIN's
  (`REFUSED` otherwise).
- *Row witness (rule 1).* A NEW row is LIVE only if its server log carries
  an in-transfer `[RDIAG]` line with `dec[` and `prune_n ≥ 2`; a MAIN
  server log with `dec[` is a witness failure.
- *Pass rule per cell.* §28's, unchanged: §21's clauses and min–max rule,
  the `[LAG]` clauses (client and server) on E = p99 − `min_us`, and at
  `c8-100` the `lag_E_spread_cli` clause (×1.5).
- *Decision rule.* **`SHIP`** iff NEW is PASS-EVERYWHERE;
  **`REFUTED-WITH-RECORD`** if it fails anywhere; **`UNSCOREABLE-AT(…)`**
  as §21.
- *Predictions (reported, not gating).*
  - **c1d:** goodput up; server CPU per GB and the server `[RDIAG]` busy
    median down; NEW `prune_max_us` < 5 000 µs (§22's H-burst bar) in every
    row, against §22's 63 196–89 740 µs on the 2 s tick (cross-session,
    report-only; MAIN has no in-binary reading of it). Size, stated
    against §22's bar: the bench difference (3 553 → 1 656 ns per source)
    is ≈ 1.9 µs of a 15–18 µs message, ≈ 11–13 %, about half of the
    ≈ 25 % handler cut §22 named for a goodput BETTER beyond the 5.6 %
    MDE. So the predicted c1d goodput clause is TREND-BETTER or BETTER, and
    the `[RDIAG]` busy median may stay ≥ 90 %.
  - c1s: the same direction, smaller (the receiver is not the only limit
    there); c2, c8: WITHIN.
  - NEW `recovered=` max ≤ 2·wcap + 4 096 at c1s and c1d (lossless enough
    that the span above the frontier is small); at c2/c8 it also holds the
    span above a hole, reported only. `below_floor` is reported per row; no
    value predicted.
  - Server `[LAG]` `max_us`, both arms: reported (the prune stall may or
    may not land on the lag probe's worker).
  - The bench: the 32 cadence costs no more per source than the 1 k
    cadence (the prune is O(pruned)).
  - *Receiver batching tail (for §30's open question, report-only).* §30
    left open whether the c1 store-law noise is receiver-side (the
    receiver's batching tail). Per cell and arm: server `[RDIAG]` `q_avg`
    and `q_max` (median, min–max), the per-row coefficient of variation of
    `q_avg` over the in-transfer lines, goodput max/min across reps, and
    the acks/dgram spread. No direction is predicted; a NEW reduction in
    the c1 `q_avg` CV and goodput spread would be evidence that the 2 s
    prune stall feeds that noise. This is not a test of §30's law.
- *Abort causes.* §21's table plus `ABORT-ONDISK`. Hard deadline launch +
  4 h; planned ≈ 1.5–2 h (§28's session: 85 min, battery 436 s).

### 32. Amendment 1 — the trees, the allocation gate, and its attribution (before VM contact)

*Tests run so far (not results).* Session 2026-10-28 03:49–05:42Z on the
pre-rebase trees (witness `bc1a52a`, PD `b2e257f`), both locks.
- Red at the witness tree (prune still on the tick): the two counting
  tests (`below_floor=0`), the routing gate, and the loopback bound, which
  failed on the bound itself: `recovered 6065 > 2*W_cap + one drain = 4496`
  with `prune_n=0`. Green at PD: 12 passed / 0 failed.
- Suites on PD: doc, wasm, parsers pass; release 1136/1 and debug 995/1,
  both `recv_alloc_bound::allocations_per_source_symbol_stay_under_the_bound`
  (release: 23.512 allocations per source symbol against the gate's 23.5).

*Cause, derived (to be checked below).* PD changed the decoder's wire
dedup set from a `HashSet` to a `BTreeSet` so the per-drain prune could pop
a prefix instead of scanning. A `HashSet` keeps its capacity across
`retain`, so in steady state it allocates nothing per symbol; a B-tree
allocates a new leaf every ≈ 5–6 ascending inserts (leaf capacity 11, split
near the middle), ≈ +0.2 allocations per source symbol — the tree node per
source symbol R1 removed from the receiver (`SeqBits`). It is a regression
by the gate's own derivation (23.5 = R1's worst run + 0.56), not a bound to
re-derive, so the bound is not touched.

*Fix (`4dcc893`).* Source seqs (sources carry `payload_id` 0) are deduped
in a `SeqBits` ring bitmap (prefix prune, no per-seq allocation); repairs
keep an ordered set `(block_id, payload_id)`. Membership semantics and
`seq_probe` are unchanged; the prune stays O(pruned). The "wire dedup set
becomes an ordered set" line of the change description above is replaced
by this.

*Trees.* `MAIN` = main `b30044b` (§31 merged). `NEW` = this branch rebased
onto `b30044b`; its engine tree is `4dcc893` (later commits are docs and
tools only). The cherry-pick of 17824f7 is gone (it is in MAIN).

*Attribution check (tests, not a battery; ships nothing by itself).* One
session under both locks (launch precondition as above), release, fresh
target dirs: `cargo test -p raptorpath --release --test recv_alloc_bound
-- --nocapture` five times on each of `BASE` = `b30044b`, `PD` = the
rebased change before the fix (`285cfe5`'s engine tree), `FIX` = `4dcc893`;
the `RECV_ALLOC allocs/symbol` values reported per run. Prediction: PD −
BASE ≈ +0.15 to +0.25 (median of 5); FIX − BASE ≤ +0.05 (the repair set
only); FIX ≤ 23.5 in every run. If FIX − BASE > +0.05, the remaining delta
is attributed before the battery (no bound change). Then the full suites
(`suites_run.sh`: release, doc, wasm, debug, parsers) on NEW.

### 32. Amendment 1 — result (tests; ships nothing)

Session 2026-10-28 09:37–10:45Z, both locks, release, fresh target dirs.
`recv_alloc_bound` allocations per source symbol, 5 runs each:

| tree | runs | median |
|---|---|---|
| BASE `b30044b` | 23.121, 23.123, 23.273, 23.046, 23.271 | 23.123 |
| PD `285cfe5` (BTreeSet dedup) | 23.420, 23.252, 23.457, 23.474, 23.271 | 23.420 |
| FIX `4dcc893` (SeqBits + repair set) | 23.494, 23.108, 23.106, 23.036, 22.999 | 23.106 |

PD − BASE = +0.30 (predicted +0.15 to +0.25: the sign and the cause hold,
the size is a little above the range). FIX − BASE = −0.02 (predicted
≤ +0.05): **MET**; FIX ≤ 23.5 in every run: **MET**. The gate is not
touched. Recorded: BASE already reads up to 23.27 and FIX's worst run
23.494, so the gate sits within about one run-spread of current main;
it can flake on main itself, independent of this change.

Full suites on NEW (`3596b0b`, engine `4dcc893`): release 1137/0
(51 ignored), doc rc 0, wasm 35/0, debug 996/0 (50 ignored), parsers rc 0,
identity panics 0. `SUITES OK`. END: 0 `raptorpath`, 0 `rp-*`; the VM dirs
are removed. Ledgers: `docs/l1-raw/thread-pd/alloc/` and (the first
session) `docs/l1-raw/thread-pd/tests1/`.

### 32. Ship battery — result

Scored 2026-10-29 against the pre-registration and Amendment 1, literally.
**`SHIP`: NEW (the decoder prune per drain) is PASS-EVERYWHERE.** It is
BETTER in goodput and in server CPU per GB at c1s and c1d, and WITHIN or
better everywhere else. Shipping is the operator's merge.

*Session.* Operator go 2026-10-29; github/main checked at `b30044b` before
launch. The on-VM waiter (`threadpd_wait.sh`) found the precondition met at
once and launched at 02:25:08Z (hard deadline + 4 h); the session ended
03:37:10Z under both locks.
- Binaries: NEW `7cab9723…f0fe86` (tree `5f31bb6`, engine `4dcc893`), MAIN
  `2842127b…c93e75` (`b30044b`), both built fresh on disk and run from
  `/dev/shm/tpd`.
- Full suites on NEW: release 1137/0 (51 ignored), doc rc 0, wasm 35,
  debug 996/0 (50 ignored), parsers rc 0, identity panics 0. `TESTS-OK`.
- Bench (`[DECBENCH]`, release, ns per source including the prune):
  140 k cadence 3 750 (`adv_max_us` 63 830), 1 k cadence 1 765 (615),
  32 cadence 1 376 (139).
- Smoke passed (43 s). Battery 03:29:57–03:37:09Z (432 s): 48 invocations,
  all LIVE, 0 witness failures, no abort token, no truncation.
- io `full avg10` ≤ 4.97 before and after every invocation, so no row is
  flagged. END: 0 `raptorpath`, 0 `rp-*`; the VM dirs are removed.
- Ledgers: `docs/l1-raw/thread-pd/battery/` (`score.txt` with every
  per-rep value, `tpd.log` with the `IOP` lines, `TESTS.txt`, `bench.log`,
  `diag-tpd/`; VM user and host scrubbed).

*Abort table.* None fired (`ABORT-LOCK`, `-PRECONDITION`, `-CRLF`,
`-BUILD`, `-TESTS`, `-SHA`, `-SENTINEL-UNWRITABLE`, `-SMOKE`, `-BUDGET`,
`-ONDISK` all *no*); `ABORT-RC` / `ABORT-BRINGUP` / `VOID-COTENANT` 0 / 0 / 0
of 48.

*Per cell* (NEW vs MAIN, median [min–max], n = 6; E = `[LAG]` xfer p99 −
`min_us`, µs; busy = server `[RDIAG]` busy median, %):

| cell | goodput Mbit/s | CPUCLI s/GB | CPUSRV s/GB | client E | server E | NEW `prune_max_us` | `[RDIAG]` busy | verdict |
|---|---|---|---|---|---|---|---|---|
| `c1s-400` | **826.3 [802.6–836.5] vs 736.5 [709.3–752.7] (+12.2 %, BETTER)** | 13.64 vs 13.99 (TREND-BETTER) | **16.52 vs 19.84 (−16.7 %, BETTER)** | 2096 vs 2129 (=) | 1971 vs 1570 (+25.5 %, =) | 2820 [1669–3372] | 67.5 vs 90.2 | **PASS** |
| `c1d-400` | **751.4 [734.6–785.9] vs 628.4 [612.9–685.4] (+19.6 %, BETTER)** | 18.56 vs 19.99 (TREND-BETTER) | **20.45 vs 23.69 (−13.7 %, BETTER)** | 2521 vs 1926 (+30.9 %, =) | 1678 vs 1622 (=) | 4298 [2861–10600] | 96.0 vs 99.0 | **PASS** (RTprop p1 TREND-BETTER) |
| `c2-100` | 90.01 vs 89.81 (=) | 31.25 vs 31.90 (=) | 35.40 vs 39.45 (−10.3 %, TREND-BETTER) | 1174 vs 1476 (=) | 908 vs 1061 (=) | 778 [563–5393] | 10.0 vs 11.8 | **PASS** |
| `c8-100` | 95.16 vs 96.91 (=) | 36.10 vs 36.80 (=) | 44.75 vs 46.55 (=) | 1328 vs 1616 (=) | 1059 vs 1119 (=) | 2986 [2317–9720] | 16.2 vs 18.0 | **PASS** (E spread 2.22 vs 1.74, =) |

Feed SAME at every cell and path. Arm verdict `PASS-EVERYWHERE`, so the
decision is **`SHIP`**.

*Predictions.*
- c1d goodput up: **MET**, and larger than predicted: BETTER at +19.6 %
  (ranges disjoint), where the bench-based size (≈ 11–13 % of the handler)
  predicted TREND-BETTER or BETTER. The bench reads why: per source the
  32 cadence costs 1 376 ns against 3 750 at the 2 s cadence, a 2.4 µs
  saving (§22 had 1.9 µs), and the change also removed the SipHash dedup
  for sources (Amendment 1's `SeqBits`).
- c1d server CPU per GB down: **MET** (−13.7 %). `[RDIAG]` busy down:
  **MET in direction** (99 → 96); it stays ≥ 90 % as allowed.
- NEW `prune_max_us` < 5 000 in every c1d row: **MISSED** in two of six
  (5 103, 10 600). Against §22's 63 196–89 740 µs on the tick it is one
  order of magnitude lower in every row, which is the reading the
  pre-registration named as the one that matters. Rows above 5 000 also
  occur at c2 (5 393) and c8 (9 720). Reading: the prune is O(pruned),
  and when a filled hole moves the frontier by thousands of seqs at once,
  one prune removes that whole jump (c8 NEW `recovered=` max 9 434–39 260,
  the span held above a hole). The bound 2·W_cap + one drain holds below
  the frontier, not for the size of one frontier jump.
- c1s same direction, smaller: **MET** (+12.2 %, below c1d's +19.6 %).
  c2, c8 WITHIN: **MET**.
- NEW `recovered=` max ≤ 2·wcap + 4 096 = 4 496 at c1s and c1d: **MET**
  (c1s 401–4 108, c1d 487–3 481). `below_floor` per row: c1s 33–62,
  c1d 1 290–1 952, c2 76–127, c8 3–52.
- Bench: the 32 cadence costs no more per source than the 1 k cadence:
  **MET** (1 376 vs 1 765 ns).
- Server `[LAG]` `max_us` (report): c1s 4 728 vs 4 090, c1d 3 318 vs
  4 421, c2 3 582 vs 5 088, c8 4 677 vs 3 942 (NEW vs MAIN medians).

*Receiver batching tail (§30's open question, report-only).* The `[RDIAG]`
`q_avg` reads 0 in almost every window on both arms (median 0, `q_max`
1–13), so its CV is undefined in most rows: this instrument is too coarse
to see a batching tail at this sampling. Goodput max/min across reps is
lower on NEW at every cell: c1s 1.042 vs 1.061, c1d 1.070 vs 1.118, c2
1.013 vs 1.026, c8 1.218 vs 1.249; acks/dgram spread c1s 0.999–1.000 vs
0.993–0.999, elsewhere equal. That is weak evidence, in direction only,
that the 2 s prune stall added to the c1 run-to-run spread. It does not
test §30's law and does not explain its window-level noise.

*What it does not show.*
- c1d client E +30.9 % and c1s server E +25.5 % sit inside their min–max
  bands; leans, not readings. The NEW arm also moves ≈ 12–20 % more data
  per second at c1, which loads the same runtimes more.
- Nothing here measures the legacy decoders (`RWM_UNIFIED=0`), whose
  prune still scans and now runs once per drain.
