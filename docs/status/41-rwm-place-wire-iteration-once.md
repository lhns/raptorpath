## 41. `RWM_PLACE_WIRE`, iteration once — an allocation-free witness — host gate, then battery — pre-registration

§38 read `REGRESSES-AT(c1s@def)` on one clause: client CPU per GB +2.5 %
(+1.9 % at matched speed). The ledger attributes it to the armed
`place_costs` call (≈ 3.4·10⁵ calls per c1s row, ≈ 360 ns each). That call
fills two extra `Vec`s and runs two argmin scans for the witnesses. The
mirror runs 29× less often and is left unchanged. By ADR-0052 a failed
prediction is iterated only when the failure names a mechanism. This one
does, so §41 is the single iteration. **Stop rule, fixed now:** a second
`REGRESSES` result, or a failed host gate, means the arm is tagged
`archive/place-wire` and reverted from `diag36`. The docs stay.

**Change (engine, arm-only).** The armed witness computes, in one pass and
with no heap allocation, both the shipped and the wire-based argmin, plus
the `ref` floor bind. Nothing in the law, the mirror or the gauge
definitions changes. The gate-off path must do no additional work against
main: no `Vec`, no extra pass, and no armed gauge update. Its printed lines
stay as §38 shipped them.

**Host gate (before any VM contact).**
- A red-first test asserts that an armed `place_costs` call on a two-leg
  scheduler allocates zero heap blocks (a counting allocator in the test
  binary), and likewise with the arm off.
- A host micro-measurement reports ns per call, armed against off, on the
  same scheduler state (release build, median of ≥ 10⁵ calls; reported, not
  scored).
- **Gate:** if the armed call still costs ≥ 100 ns more than off, the stop
  rule fires and no VM time is spent.

**Battery.** As §38 with amendments 1–4, unchanged: cells, n = 6 per arm,
seeds, block order, the ladder with amendment 4's exemption, every clause
and tolerance (client CPU/GB rel 0.024). Nothing is loosened. Added:
- **Arm BASE:** main's own binary (`4926822`, built fresh from that tree),
  at c1s default and c1d default, n = 6. **BASE gate:** MAIN (the new
  binary with the gate off) against BASE must read WITHIN on every §21
  clause at both cells. Otherwise the arm cannot reach main even switched
  off, whatever the ladder says. That is 24 more rows.
- **Reported beside the clause:** client CPU/GB at matched speed (WIRE and
  MAIN rows within ±3 % of the pooled median goodput), and per-row
  contention (`procs_running` and io `full avg10` before/after) for every
  c1s row. §38's slow WIRE rows (575, 684 Mbit/s) are not yet explained.

**Predictions.** The host gate passes, with the armed overhead ≤ 50 ns. The
c1s client CPU/GB is WITHIN (≤ +1 %), and the ladder reads `CURED`, since
§38's other clauses all passed. BASE against MAIN reads WITHIN.

**Duration.** Two fresh builds (≈ 12 min) and 120 rows plus the smoke
(≈ 25 min), inside the 4 h launch deadline.


### 41. Host gate — result (before VM contact)

**PASS.** The change is `9354111`, merged on `diag36`.
- **Allocations:** the armed path now makes 0 extra heap blocks per
  `place_costs` call. Off and armed make 1 each (the result `Vec`, as on
  main), against 2 extra before. `armed_place_costs_allocates_exactly_what_off_does`
  was red on the old code ("left: 2, right: 0").
- **Cost:** the armed − off overhead is 43 / 45 / 40 ns, against 206 / 209 /
  190 ns before. Method: a release host bench, median of 2·10⁵ calls in
  batches of 100 (the Windows `Instant` tick is 100 ns), three runs.
- **Result:** below the 100 ns gate and the 50 ns prediction. What remains is
  the second load term, the gauge updates and the running-min tracking.
- **Unchanged:** the witnesses are now running minima in the single pass,
  with first-minimum tie-break by id, identical to the old sorted `min_by`
  for finite costs. The law, the mirror and the printed fields are
  unchanged. Release `scheduler::`/`gates::`/`net::eta` tests give 121/0.
- **Next:** the full suites and the §41 battery (with the BASE arm) on the
  VM.

### 41. Amendment 1 — the row count and the harness's readings (before VM contact)

- **Rows.** BASE runs at c1s and c1d default with n = 6 each: **12 rows**,
  108 in all. "24" and "120" above were miscounts. Duration falls with it.
- **Arm order.** MAIN and WIRE keep plan 38's order exactly (MAIN first on
  even blocks). BASE is inserted at position (block mod 3), so each block
  position gets it twice. A test pins plan 41 minus BASE = plan 38.
- **Smoke.** Plan 38's two rows plus one BASE row (c1s default).
- **No N = 1 check row.** WIRE rows already need `w_calls > 0` to be LIVE,
  and §41 changed no printed field.
- **Contention.** One `/proc/stat` `procs_running` sample and io `full
  avg10`, just before and just after each invocation. Covariates only;
  they never void a row.
- **BASE gate.** It passes only if every §21 clause reads exactly WITHIN,
  the feed reads SAME, and there is no DNF. TREND-* and BETTER fail it; a
  missing cell reads UNREADABLE. The BASE witnesses: no `RWM_PLACE_WIRE`
  token, no numeric `wpl_us`, no `w_*` gauges.

### 41. Result

Scored 2026-11-10 (`docs/l1-raw/place41/score.txt`). **Ladder `CURED`
(WIRE against MAIN); BASE gate `FAIL`.** By the gate's own rule, the arm
cannot reach main as registered.

*Session.* One launch at 13:17Z: precondition met, both locks, gate, two
fresh builds (322 s and 319 s), suites, smoke, then the battery
14:13–14:29Z, `DONE-ALL`.
- Suites on the diag36 tree: release 1 163/0, doc, wasm 35, debug 1 026/0,
  tools/l1 python tests. No flake.
- 108 rows: 106 LIVE, plus two WIRE rows `WITNESS-FAIL` on the ≥ 95 %
  `wpl`-within-1 ms witness (92 % and 94 %), left void.
- No abort token fired.

*Ladder.*
- **c1s default:** all clauses WITHIN. Client CPU/GB is +0.8 % (WIRE 15.76
  against MAIN 15.64; tolerance 2.4 %), and +1.7 % at matched speed (4 rows
  per arm; §38 read +1.9 %). The overhead is reduced, not gone.
- **c1d default:** goodput +0.1 %, so `CURED`, not `-AT-COST`.
- **c1d 1536:** LOCKED 4/6 under MAIN, 0/6 under WIRE.
- **c8:** static caps 192, 256 and 512 were NOT reproduced this time (MAIN
  locked 1/6 at each). The share is preserved.
- The cure therefore rests on c1d 1536 in two sessions, and on c8 256 in
  §38 only.

*BASE gate.*
- **c1s:** PASS, every clause WITHIN.
- **c1d default:** FAIL, on one clause. `lag_E_cli` reads TREND-BETTER:
  MAIN 1 290 against BASE 1 814 µs median, −28.9 % against a 28.5 %
  tolerance. All other clauses are WITHIN.
- **Per-row (ledger):** BASE's six c1d rows read 1 519–2 552 µs. The new
  binary's MAIN and WIRE rows read 858–2 823 µs, mostly 900–1 700.
- **Reading.** Both new-binary arms sit lower than BASE, so the difference
  is the binary or the run order, not the arm switch. The new binary
  differs from main only by the gate-off arm code, its printed fields and
  the test hook. A better lag is not harm.
- **But the gate was never sized.** How often two identical binaries trip a
  13-clause all-WITHIN gate was not measured (an A/A pair). That gap is the
  coordinator's.

*Decision for the user (§43 amendment 1 allows two routes).*
- **Recommended: archive.** Tag `archive/place-wire`, revert the arm's
  engine merges from `diag36`, and keep the docs, harness and ledgers.
  - No default-law benefit showed in two sessions.
  - §45's rule is promote or archive at the verdict, and Stage 3 redesigns
    the price inputs anyway.
  - `diag36`'s engine then equals main, which clears the merge and gives
    §43 a clean comparator.
- **Alternative:** a calibrated re-test as a new registration, ≈ 10 min,
  with arms BASE, BASE′ (relabelled) and MAIN. MAIN-against-BASE must not
  exceed BASE-against-BASE′.
- Either way, §45's guard needs an A/A pair in its first run.
