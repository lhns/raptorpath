## 48b. The `bbr_rs` battery on the VM — pre-registration (before any VM contact; advisor review)

§48's Step 0 read `STOP-NOT-REPRODUCED` on its registered 50 µs simulator. The 10 µs result (quinn collapses, `bbr_rs` holds at every phase, the pre-fix sweep red everywhere) came after that verdict, so it is post hoc. **This battery is the only blind test of the fix.** §48 Step 1's design, readouts, outcomes and predictions stand, except where they are amended here.

**1. Trees.**
- **BASE = main at launch,** pinned by amendment after the `simplify-2` verdict (§49), with its sha recorded. Quinn's BBR is the default; the binary has no `qm=` and no `[QCC]` line.
- **CAND = BASE plus the §48 commits only** (branch `bbrfix48b`: `bbrfix48-impl` with current main merged), run with `RWM_QUIC_CC=bbr_rs`. Pinned by amendment.
- **Pre-launch check:** `git diff --stat BASE CAND -- raptorpath/src` lists only `transport/bbr_rs.rs`, `transport/cc_sim.rs`, `transport/io_owner.rs`, `transport/mod.rs`, `transport/quic.rs`, `net/diag.rs`, plus `tests/qcc_gauge_loopback.rs`.
- **No merge before the verdict.** Quinn's BBR plus the §48 seams is not measured, and it reaches main under no outcome: promotion deletes quinn's path (ADR-0073), and the other outcomes archive.

**2. The guard is §41c, not §41b.** §41c's `rank-minp` statistic exactly, with N_PERM 10 000, seed 4141, one-sided, PASS iff p > 0.05: 26 clauses over c1s, c1d, c2 and c8, with §41c's DNF rule (a DNF or crash row takes the worst rank on every clause of its cell).
- **p_rest** (the cost outcome): the same statistic and relabellings over the 24 clauses without the goodput clauses of c2 and c8.
- §48's tail clause (CAND's lowest row ≥ 0.8 × CAND's cell median) and the BASE-collapse line (BASE DNF rows and R2b per BASE row) are reported beside the guard.
- The outcome ladder is §48's, with R1 = this guard and "p_rest > 0.05" read on this p_rest.

**3. Design: 12 + 12 per cell** at §48's long sizes (c1s and c1d 5 GB, c2 and c8 600 MB), seeds 42 and 7 × 6 reps per arm: 96 rows. This is §41c's mandatory design, so its acceptance table and replacement rule (≤ 4 witness-voided slots, re-run once) carry over unchanged. An incomplete design reads NO VERDICT.
- Est. ≈ 96 × 65 s ≈ 105 min plus builds and suites, inside `LD_HARD` = launch + 4 h.

**4. Smoke and check row: one CAND row at c8,** then one BASE row at c8. The CAND row is the parser's first real `qm=` / `[QCC]` log on a two-leg cell, so it is unscored and must show `qm=` in {S, D, B, R} on every path block and a `[QCC]` line per leg; otherwise `ABORT-SMOKE`.

**5. Suites** run on CAND's tree first (release, doc, wasm, debug-witness, every `tools/l1` test). A REAL failure is `ABORT-TESTS`.

**6. Before VM contact:** the harness (plan 48 in `loopdiag_*`, `tools/l1/bbr48_score.py`, `test_bbr48_parse.py`) is built and tested; the scorer dry-runs on §41c session 2's BASE rows and reproduces its GATE-41c verdict and resolution table bit-for-bit when given the same rows as both arms' pool; the trees are pinned by amendment.

**7. After the verdict (unchanged from §48).** `PROMOTE-ELIGIBLE` or `CONTROLLED-WITH-COST` goes to the user, who rules on flipping `RWM_QUIC_CC` (a new ADR superseding ADR-0054's default, and quinn's path deleted unless kept as a reference arm). `NOT-CONTROLLED` or `REGRESSION` goes to the user to choose: archive (`archive/bbr-rs`) or §47's option 1 or 3. Nothing merges to main automatically. **The registered prediction stays `CONTROLLED-WITH-COST`** (failing R1 on c2/c8 goodput, wrtt at c2 ≤ 0.3 × BASE).
