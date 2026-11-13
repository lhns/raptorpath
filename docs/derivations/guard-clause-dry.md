# guard-clause-dry: two replacements for the `rtp_floor` guard clause, dry-run on the guard ledgers

Status: DRY RUN from committed logs only. No engine change, no VM contact, nothing wired into a gate: the candidates are functions next to `rtp_floor` in `tools/l1/threadp1_parse.py` (tests: `tools/l1/test_guard_clause_dry.py`). Feeds: status §3.10 item 7 and §53 Erratum 1 (`docs/status.md`). This note states facts; it chooses nothing.

## 1. Provenance of `rtp_floor` as a guard clause

`rtp_floor` first appears as a scored quantity in the V-P1 battery of threading P1 (status §11): the harness commit `a557afcf` (2026-10-05, `threadp1_parse.rtp_floor`, "the minimum non-zero `rtp_us` over the client's `[DIAG]` lines") and the pre-registration `c765e3f3` list "per leg the RTprop floor" among the scored quantities, with a tolerance of max(5 %, MAIN's half-range / median) and no committed MDE. The pre-registration states no purpose beyond the list. The only written reading is the P1 result `ea8fe1e0` (§11 item 4): the c8 fast-leg floor read 9.8-10.8 ms against MAIN's 7.8-9.8 ms (ranges touching), with the unmeasured hypothesis "a sender woken sooner while paused hands quinn datagrams that queue behind the previous burst", that is, the clause was used as a probe for sender-induced queueing delay on a path. Later batteries copied it as part of §21's pass rule (pre-registrations P2a `39a97f3e` and Q1 `272d0012`; the D9, PD, R and SK parsers). Plan 38's harness (`8b958155`, 2026-11-10) listed "RTprop per leg" among its clauses; plan 41b (`ff965d04`) made it one of the 13 clauses of the A/A-calibrated guard (rtp floor resolution 1.6 % at c1s, 2.3-3.1 % at c1d, status §41b), and the §41c rank-minp guard (status `docs/status/41c-the-merge-guard-rank-minp-12-12.md`, code in `place41b_score.py`) kept it per leg: 26 clauses, 6 of them `rtp_floor_p0/p1`. No earlier commit scores `rtp_us` (`git log -S rtp_us`), and no document states a design rationale for it as a merge-guard clause; what it was added to protect is, as written, "the per-path RTprop did not rise" (§21 pass rule). The defect finding is §52 Result (`f7a4ff5a`, `2d3b2069`), §53 Result (`a9090e99`) and §53 Erratum 1 (`a4e5ef52`): at 600 MB the minimum is the drained tail on 20-gap rows and the start-up sample on lever-B rows, and at guard size it is the start-up sample on about half of the rows and the end-of-run sample on the rest.

## 2. The three clauses

Notation: `[DIAG]` is the client log line every 250 ms (printed to 0.1 s); `t` its time stamp, `src` its `src=` field (sym/s, the source's current rate), `rtp_i(t)` the `rtp_us=` of leg i (0 = no sample yet), `S(t)` and `K(t)` the first and third `cum=` fields (cumulative source-sent and `window_ack_seq`, the receiver's cumulative in-order ack).

- **(C) current** `rtp_floor`: `C_i = min { rtp_i(t) : rtp_i(t) > 0 }` over all ticks. Per leg.
- **(A) source-phase floor** `rtp_floor_source_phase`: `A_i = min { rtp_i(t) : rtp_i(t) > 0, t >= 1.0 s, src(t) > 0 }`. Per leg; `t_min = 1.0 s` is the task's parameter (`SRC_PHASE_T_MIN`), a leg without such a tick is absent (no substitute). Provenance of the symbols: `rtp_us` is the app-echo RTT windowed minimum measured by the sender's Copa `min_rtt` (a 10 s window; status §3.10 item 9), `src` the `[DIAG]` field, 1.0 s a declared dial.
- **(B) delivered in-order delay** `inorder_delay` / `inorder_delay_stats`: `delay(t) = t - t_src(K(t))`, with `t_src(a)` the inverse of the cumulative sent curve `S`, linearly interpolated between ticks (`t_src(a) = T[0]` for `a <= S[0]`; ticks with `K(t) = 0` or `K(t) > S(last)` are skipped). Reported as the mean and the p95 `sorted[int(0.95 (n-1) + 0.5)]` over all ticks with `K > 0` (whole run, drained tail included). One scalar per row (leg 0 stands for it in the guard's clause slot). Logic reused from `docs/derivations/p0-dwell-diag/inorder.py` on branch `github/p0-dwell-diag` (same `t_src`, same p95 index; that script also splits a "PIN" phase on the receive window, which is not needed here). **(B')**, reported beside B, is the same with only ticks `src > 0` (drained tail removed).

Defined on every row: on all 330 rows scored (96 + 96 + 30 + 60 + 48) C, A, B and B' have a value on every leg (no None, no neutral rank fill); the check is in `rowvals-*.json`.

## 3. Ledger coverage

Scored (client `[DIAG]` logs present, all rows LIVE, no replacement slots):

| ledger | archive | rows | arms |
|---|---|---|---|
| §52 guard | `docs/l1-raw/s52/guard-diag-ld.tgz` + `guard-ld.log` | 96 | BASE 12, CAND 12 at c1s-400, c1d-400, c2-100, c8-100 (last `[DIAG]` tick 3.5-4.3 s at c1, 8.8-9.0 s at c2, 7.5-9.1 s at c8) |
| §48b | `docs/l1-raw/bbr48b/diag-ld-full.tgz` + `run/bbr48/ld.log` | 96 | BASE 12, CAND 12 (the bbr_rs candidate) at the same four cells; rows run 43-151 s, longer than the 10 s window |
| §52 rep | `docs/l1-raw/s52/rep52-diag-ld.tgz` | 30 | c8 600 MB: BASE, CAND, W0, W05, WDER, 6 each |
| §53 | `docs/l1-raw/s53/rep53-diag-ld.tgz` | 60 | c8 600 MB: BASE, CAND, BASES, CANDS, CW0, 6 each; c1d 5 GB: BASE 12, CAND 12, CANDS 6 |
| §41b | `docs/l1-raw/place41b/diag-ld/*.gz` | 48 | BASE 12, MAIN 6, WIRE 6 at c1s, c1d (a gate-off A/A ledger, not a merge guard; extra A/A data) |

Not scored: the §47 guard ledgers (`mergecand47/*`), §49 (`simplify49`), §50 (`simplify50`) keep `ld.log`/`score.txt` only, with no client `[DIAG]` logs, so no candidate can be computed from them. `bbr48b/c2c8-diag-qcc.tgz` is a diagnostic re-run of §48b's c2/c8 rows with no ledger rows next to it (no status, seed/rep slot or `P38ROW`), so it is not a guard ledger. `place38` (96 rows) and `place41` (108 rows) are plan-38/41 batteries of MAIN/WIRE (and BASE) at store sizes, with 3 rows per arm per seed and not the 12 + 12 guard design; their diag logs were extracted but not scored.

## 4. Method

- Rows come from each ledger's `ld.log` (`place38_parse.rows_of`, `apply_replacements`); a row's `rtp_floor_p<i>` slot is replaced by the candidate's value and the guard's own `place41b_score.statistic_rank` (N_PERM 10 000, RNG seed 4141, stratified by seed, ties extreme, undefined = neutral rank) is run, BASE as reference and the named arm as test. The printed p is that clause's one-sided p (`p_true`): the share of relabellings whose worse-side rank sum is at least the true one. The cells are passed exactly as the ledger's guard ran them. **Check:** with candidate (C) the six `rtp_floor` p values of the §52 guard reproduce `docs/l1-raw/s52/guard-score.txt` exactly (0.7665, 0.0564, 0.9354, 0.6136, 0.0809, 0.5563).
- Worse side is the high side for all three (a higher floor, a longer delay). `test/ref` is the ratio of arm medians. Spread is (max - min) / median of the arm's rows (the full range; it is not `l1ab.spread_rel`, which is the half-range). Units: C and A in µs, B in ms. Row values: `res-*.tsv` (one line per row and clause).
- **A/A resolution (d80):** the guard's own recipe (`place41b_score.session_resolution`: BASE rows resampled with replacement to the design, balanced per seed, "CAND" rows shifted worse by delta on the clause, 30 draws, 1 000 relabellings, grid 1 %..100 %, d80 = smallest delta caught at p <= 0.05 in >= 80 % of draws), restricted to the rtp clauses (`resolution.py`; the other clauses keep their real BASE values, so the min-p runs over the ledger's other clauses too). A d80 of ">100 %" means no grid point reaches 80 %. Not comparable between rows of different design: the four-cell ledgers use 12 + 12 and 26 clauses; the c8-only 600 MB ledgers (6 + 6, 7 clauses) and the c1s/c1d-only §41b ledger (12 + 6) have fewer clauses, so their min-p floor is lower. **Check:** `validate_d80.py` runs the guard's unmodified `session_resolution` on the §52 guard ledger; its six rtp d80 values equal `resolution.py`'s candidate (C) entries exactly (0.05, 0.05, 0.08, 0.5, 0.2, 0.12; `d80-validate-s52-C.json`).
- Where a minimum sits (`where.py`): C "start-up" = C's first minimum at t <= 0.5 s; "drained tail" = after the last tick with `src` > 0.

## 5. Results

BASE is the reference arm; its range and spread are the A/A readings. "ref arm" and "test arm" are named in the bold row where a ledger has two pairs.


#### s52 guard (BASE 12 vs CAND 12; the §52 merge-candidate guard, the ledger of §53 Erratum 1)

| cell | leg | clause (unit) | ref arm: median [min-max] spread | test arm: median [min-max] spread | test/ref | p (1-sided) | A/A d80 |
|---|---|---|---|---|---|---|---|
| c1s-400 | p0 | C rtp_floor (us) | 2531 [2344-2636] 12% | 2514 [2332-2698] 15% | 0.993 | 0.7665 | 5 % |
| c1s-400 | p0 | A src-phase floor (us) | 2531 [2344-2636] 12% | 2514 [2332-2698] 15% | 0.993 | 0.7665 | 5 % |
| c1s-400 | p0 | B mean delay (ms) | 18.2 [16.3-21.0] 26% | 18.1 [15.3-23.8] 47% | 0.999 | 0.3124 | 20 % |
| c1s-400 | p0 | B p95 delay (ms) | 25.9 [22.9-48.2] 98% | 30.1 [24.7-51.6] 89% | 1.163 | 0.1434 | 100 % |
| c1s-400 | p0 | B' mean (src phase) (ms) | 18.2 [16.3-21.0] 26% | 18.1 [15.3-23.8] 47% | 0.999 | 0.3124 | 20 % |
| c1s-400 | p0 | B' p95 (src phase) (ms) | 25.9 [22.9-48.2] 98% | 30.1 [24.7-51.6] 89% | 1.163 | 0.1434 | 100 % |
| c1d-400 | p0 | C rtp_floor (us) | 2456 [2376-2629] 10% | 2554 [2331-2936] 24% | 1.040 | 0.0564 | 5 % |
| c1d-400 | p0 | A src-phase floor (us) | 2456 [2376-2629] 10% | 2554 [2331-2936] 24% | 1.040 | 0.0564 | 5 % |
| c1d-400 | p0 | B mean delay (ms) | 18.0 [10.4-32.6] 123% | 17.2 [10.2-28.0] 103% | 0.956 | 0.7705 | 100 % |
| c1d-400 | p0 | B p95 delay (ms) | 36.9 [18.6-106] 236% | 29.0 [17.1-54.7] 130% | 0.784 | 0.9395 | >100 % |
| c1d-400 | p0 | B' mean (src phase) (ms) | 18.0 [10.4-32.6] 123% | 17.2 [10.2-28.0] 103% | 0.956 | 0.7705 | 100 % |
| c1d-400 | p0 | B' p95 (src phase) (ms) | 36.9 [18.6-106] 236% | 29.0 [17.1-54.7] 130% | 0.784 | 0.9395 | >100 % |
| c1d-400 | p1 | C rtp_floor (us) | 2503 [2370-2725] 14% | 2443 [2302-2747] 18% | 0.976 | 0.9354 | 8 % |
| c1d-400 | p1 | A src-phase floor (us) | 2503 [2370-2725] 14% | 2443 [2302-2747] 18% | 0.976 | 0.9354 | 8 % |
| c2-100 | p0 | C rtp_floor (us) | 11240 [8386-14967] 59% | 11194 [7280-14728] 67% | 0.996 | 0.6136 | 50 % |
| c2-100 | p0 | A src-phase floor (us) | 11240 [8386-14967] 59% | 11194 [7280-14728] 67% | 0.996 | 0.6136 | 50 % |
| c2-100 | p0 | B mean delay (ms) | 83.2 [61.1-151] 109% | 84.2 [58.4-135] 91% | 1.011 | 0.6161 | 50 % |
| c2-100 | p0 | B p95 delay (ms) | 132 [95.4-292] 149% | 146 [97.1-295] 136% | 1.107 | 0.4817 | 100 % |
| c2-100 | p0 | B' mean (src phase) (ms) | 83.2 [61.1-151] 109% | 84.2 [58.4-135] 91% | 1.011 | 0.6161 | 50 % |
| c2-100 | p0 | B' p95 (src phase) (ms) | 132 [95.4-292] 149% | 146 [97.1-295] 136% | 1.107 | 0.4817 | 100 % |
| c8-100 | p0 | C rtp_floor (us) | 9530 [7553-10909] 35% | 10708 [7415-11799] 41% | 1.124 | 0.0809 | 20 % |
| c8-100 | p0 | A src-phase floor (us) | 10800 [8233-12577] 40% | 11032 [7441-11799] 40% | 1.021 | 0.7716 | 50 % |
| c8-100 | p0 | B mean delay (ms) | 880 [384-1276] 101% | 654 [462-941] 73% | 0.743 | 0.9696 | >100 % |
| c8-100 | p0 | B p95 delay (ms) | 1554 [624-2424] 116% | 1182 [685-1600] 77% | 0.761 | 0.9363 | >100 % |
| c8-100 | p0 | B' mean (src phase) (ms) | 871 [384-1157] 89% | 641 [457-910] 71% | 0.736 | 0.9688 | >100 % |
| c8-100 | p0 | B' p95 (src phase) (ms) | 1591 [624-2554] 121% | 1182 [685-1600] 77% | 0.743 | 0.9490 | >100 % |
| c8-100 | p1 | C rtp_floor (us) | 40724 [36755-43667] 17% | 39132 [34480-47658] 34% | 0.961 | 0.5563 | 12 % |
| c8-100 | p1 | A src-phase floor (us) | 40724 [36755-43667] 17% | 39132 [34480-47658] 34% | 0.961 | 0.5563 | 12 % |

#### bbr48b (BASE 12 vs CAND 12; the §48b bbr_rs battery, a different candidate on the same four cells)

| cell | leg | clause (unit) | ref arm: median [min-max] spread | test arm: median [min-max] spread | test/ref | p (1-sided) | A/A d80 |
|---|---|---|---|---|---|---|---|
| c1s-400 | p0 | C rtp_floor (us) | 2522 [2466-3029] 22% | 2526 [2372-3023] 26% | 1.002 | 0.5596 | 12 % |
| c1s-400 | p0 | A src-phase floor (us) | 2522 [2466-3029] 22% | 2526 [2372-3023] 26% | 1.002 | 0.5596 | 12 % |
| c1s-400 | p0 | B mean delay (ms) | 22.0 [19.5-96.2] 349% | 19.7 [17.3-21.5] 22% | 0.895 | 0.9987 | >100 % |
| c1s-400 | p0 | B p95 delay (ms) | 41.9 [35.6-167] 314% | 40.0 [35.3-53.9] 46% | 0.956 | 0.9010 | >100 % |
| c1s-400 | p0 | B' mean (src phase) (ms) | 22.0 [19.5-96.2] 349% | 19.7 [17.3-21.5] 22% | 0.895 | 0.9987 | >100 % |
| c1s-400 | p0 | B' p95 (src phase) (ms) | 41.9 [35.6-167] 314% | 40.0 [35.3-53.9] 46% | 0.956 | 0.9010 | >100 % |
| c1d-400 | p0 | C rtp_floor (us) | 2456 [2296-2595] 12% | 2393 [2339-2543] 9% | 0.974 | 0.8296 | 8 % |
| c1d-400 | p0 | A src-phase floor (us) | 2456 [2296-2595] 12% | 2393 [2339-2543] 9% | 0.974 | 0.8296 | 8 % |
| c1d-400 | p0 | B mean delay (ms) | 26.7 [22.7-68.0] 170% | 26.8 [22.8-30.3] 28% | 1.005 | 0.6519 | 100 % |
| c1d-400 | p0 | B p95 delay (ms) | 65.1 [48.6-233] 282% | 57.3 [46.4-77.4] 54% | 0.880 | 0.9360 | >100 % |
| c1d-400 | p0 | B' mean (src phase) (ms) | 26.7 [22.7-68.0] 170% | 26.8 [22.8-30.3] 28% | 1.005 | 0.6519 | 100 % |
| c1d-400 | p0 | B' p95 (src phase) (ms) | 65.1 [48.6-233] 282% | 57.3 [46.4-77.4] 54% | 0.880 | 0.9360 | >100 % |
| c1d-400 | p1 | C rtp_floor (us) | 2482 [2326-2823] 20% | 2372 [2301-2745] 19% | 0.956 | 0.9771 | 12 % |
| c1d-400 | p1 | A src-phase floor (us) | 2482 [2326-2823] 20% | 2372 [2301-2745] 19% | 0.956 | 0.9771 | 12 % |
| c2-100 | p0 | C rtp_floor (us) | 11450 [6230-13801] 66% | 10602 [8846-15126] 59% | 0.926 | 0.9371 | 100 % |
| c2-100 | p0 | A src-phase floor (us) | 11450 [6230-13801] 66% | 11065 [8846-15126] 57% | 0.966 | 0.8338 | 100 % |
| c2-100 | p0 | B mean delay (ms) | 344 [309-357] 14% | 350 [341-366] 7% | 1.019 | 0.0169 | 5 % |
| c2-100 | p0 | B p95 delay (ms) | 568 [542-588] 8% | 566 [531-599] 12% | 0.996 | 0.5644 | 5 % |
| c2-100 | p0 | B' mean (src phase) (ms) | 344 [309-357] 14% | 350 [341-366] 7% | 1.019 | 0.0169 | 5 % |
| c2-100 | p0 | B' p95 (src phase) (ms) | 568 [542-588] 8% | 566 [531-585] 10% | 0.996 | 0.6066 | 5 % |
| c8-100 | p0 | C rtp_floor (us) | 8648 [5406-9837] 51% | 7044 [5122-8723] 51% | 0.814 | 0.9953 | 20 % |
| c8-100 | p0 | A src-phase floor (us) | 10933 [6484-12419] 54% | 12145 [8378-13746] 44% | 1.111 | 0.0396 | 50 % |
| c8-100 | p0 | B mean delay (ms) | 4203 [3756-4714] 23% | 6141 [5456-6587] 18% | 1.461 | 0.0001 | 20 % |
| c8-100 | p0 | B p95 delay (ms) | 7076 [5887-7652] 25% | 8451 [7890-8910] 12% | 1.194 | 0.0001 | 20 % |
| c8-100 | p0 | B' mean (src phase) (ms) | 4144 [3595-4627] 25% | 6118 [5346-6645] 21% | 1.476 | 0.0001 | 20 % |
| c8-100 | p0 | B' p95 (src phase) (ms) | 7062 [5801-7652] 26% | 8460 [7913-8935] 12% | 1.198 | 0.0001 | 20 % |
| c8-100 | p1 | C rtp_floor (us) | 34472 [32791-37202] 13% | 35798 [33680-37246] 10% | 1.038 | 0.1480 | 8 % |
| c8-100 | p1 | A src-phase floor (us) | 36148 [32791-37991] 14% | 37450 [33857-40948] 19% | 1.036 | 0.0165 | 12 % |

#### s53 c8 600 MB (BASE vs CAND, then BASES vs CANDS; n = 6 per arm)

| cell | leg | clause (unit) | ref arm: median [min-max] spread | test arm: median [min-max] spread | test/ref | p (1-sided) | A/A d80 |
|---|---|---|---|---|---|---|---|
| **BASE vs CAND** | | | | | | | |
| c8-100 | p0 | C rtp_floor (us) | 8698 [7863-10235] 27% | 10217 [8395-12028] 36% | 1.175 | 0.0873 | 20 % |
| c8-100 | p0 | A src-phase floor (us) | 10857 [10239-11925] 16% | 10217 [8395-12028] 36% | 0.941 | 0.9530 | 8 % |
| c8-100 | p0 | B mean delay (ms) | 4408 [4020-4788] 17% | 956 [874-1288] 43% | 0.217 | 1.0000 | 12 % |
| c8-100 | p0 | B p95 delay (ms) | 6764 [6384-7352] 14% | 1463 [1439-2124] 47% | 0.216 | 1.0000 | 12 % |
| c8-100 | p0 | B' mean (src phase) (ms) | 4351 [3819-4795] 22% | 957 [876-1289] 43% | 0.220 | 1.0000 | 20 % |
| c8-100 | p0 | B' p95 (src phase) (ms) | 6667 [6288-7352] 16% | 1463 [1439-2124] 47% | 0.219 | 1.0000 | 12 % |
| c8-100 | p1 | C rtp_floor (us) | 36936 [35218-38284] 8% | 38714 [36497-46917] 27% | 1.048 | 0.0232 | 8 % |
| c8-100 | p1 | A src-phase floor (us) | 37295 [35218-38284] 8% | 38973 [36497-47762] 29% | 1.045 | 0.0498 | 8 % |
| **BASES vs CANDS** | | | | | | | |
| c8-100 | p0 | C rtp_floor (us) | 7896 [7039-10174] 40% | 11778 [9750-12656] 25% | 1.492 | 0.0090 | - |
| c8-100 | p0 | A src-phase floor (us) | 10782 [9176-14176] 46% | 11786 [9750-13100] 28% | 1.093 | 0.5605 | - |
| c8-100 | p0 | B mean delay (ms) | 4498 [4135-4943] 18% | 1016 [891-1355] 46% | 0.226 | 1.0000 | - |
| c8-100 | p0 | B p95 delay (ms) | 6752 [6318-7740] 21% | 1766 [1430-2269] 48% | 0.262 | 1.0000 | - |
| c8-100 | p0 | B' mean (src phase) (ms) | 4420 [4067-4790] 16% | 1015 [893-1361] 46% | 0.230 | 1.0000 | - |
| c8-100 | p0 | B' p95 (src phase) (ms) | 6744 [6318-7804] 22% | 1766 [1430-2269] 48% | 0.262 | 1.0000 | - |
| c8-100 | p1 | C rtp_floor (us) | 37776 [35082-39174] 11% | 39022 [36856-42397] 14% | 1.033 | 0.1519 | - |
| c8-100 | p1 | A src-phase floor (us) | 38045 [36780-39226] 6% | 39022 [36856-42397] 14% | 1.026 | 0.3275 | - |

Other arms in the same ledger (median [min-max] spread):

| cell | leg | clause | arm | value |
|---|---|---|---|---|
| c8-100 | p0 | C rtp_floor | CW0 | 10872 [9060-12590] 32% |
| c8-100 | p1 | C rtp_floor | CW0 | 39501 [36756-42722] 15% |
| c8-100 | p0 | A src-phase floor | CW0 | 10872 [9060-12590] 32% |
| c8-100 | p1 | A src-phase floor | CW0 | 40318 [36756-42722] 15% |
| c8-100 | p0 | B mean delay | CW0 | 811 [770-854] 10% |
| c8-100 | p0 | B p95 delay | CW0 | 1269 [1189-1336] 12% |
| c8-100 | p0 | B' mean (src phase) | CW0 | 810 [770-854] 10% |
| c8-100 | p0 | B' p95 (src phase) | CW0 | 1269 [1189-1336] 12% |

#### s53 c1d 5 GB (BASE 12 vs CAND 12)

| cell | leg | clause (unit) | ref arm: median [min-max] spread | test arm: median [min-max] spread | test/ref | p (1-sided) | A/A d80 |
|---|---|---|---|---|---|---|---|
| c1d-400 | p0 | C rtp_floor (us) | 2504 [2319-2794] 19% | 2531 [2290-2857] 22% | 1.011 | 0.2054 | - |
| c1d-400 | p0 | A src-phase floor (us) | 2504 [2319-2794] 19% | 2531 [2290-2857] 22% | 1.011 | 0.2054 | - |
| c1d-400 | p0 | B mean delay (ms) | 28.4 [24.6-645] 2182% | 26.4 [21.4-390] 1397% | 0.929 | 0.9481 | - |
| c1d-400 | p0 | B p95 delay (ms) | 64.0 [44.4-1497] 2268% | 62.1 [49.4-1108] 1706% | 0.969 | 0.6578 | - |
| c1d-400 | p0 | B' mean (src phase) (ms) | 28.4 [24.6-645] 2184% | 26.4 [21.4-387] 1383% | 0.929 | 0.9481 | - |
| c1d-400 | p0 | B' p95 (src phase) (ms) | 64.0 [44.4-1497] 2268% | 62.1 [49.4-1071] 1646% | 0.969 | 0.6578 | - |
| c1d-400 | p1 | C rtp_floor (us) | 2546 [2422-2669] 10% | 2482 [2373-2753] 15% | 0.975 | 0.9593 | - |
| c1d-400 | p1 | A src-phase floor (us) | 2546 [2422-2669] 10% | 2482 [2373-2753] 15% | 0.975 | 0.9593 | - |

Other arms in the same ledger (median [min-max] spread):

| cell | leg | clause | arm | value |
|---|---|---|---|---|
| c1d-400 | p0 | C rtp_floor | CANDS | 2624 [2416-2881] 18% |
| c1d-400 | p1 | C rtp_floor | CANDS | 2458 [2413-2667] 10% |
| c1d-400 | p0 | A src-phase floor | CANDS | 2624 [2416-2881] 18% |
| c1d-400 | p1 | A src-phase floor | CANDS | 2458 [2413-2667] 10% |
| c1d-400 | p0 | B mean delay | CANDS | 24.7 [22.6-46.0] 94% |
| c1d-400 | p0 | B p95 delay | CANDS | 68.9 [52.3-139] 126% |
| c1d-400 | p0 | B' mean (src phase) | CANDS | 24.7 [22.6-46.0] 94% |
| c1d-400 | p0 | B' p95 (src phase) | CANDS | 68.9 [52.3-139] 126% |

#### rep52 c8 600 MB (BASE vs CAND; n = 6 per arm)

| cell | leg | clause (unit) | ref arm: median [min-max] spread | test arm: median [min-max] spread | test/ref | p (1-sided) | A/A d80 |
|---|---|---|---|---|---|---|---|
| c8-100 | p0 | C rtp_floor (us) | 7608 [5444-8327] 38% | 10596 [10079-12647] 24% | 1.393 | 0.0030 | 50 % |
| c8-100 | p0 | A src-phase floor (us) | 11403 [10458-12407] 17% | 11816 [10079-13817] 32% | 1.036 | 0.2237 | 12 % |
| c8-100 | p0 | B mean delay (ms) | 4305 [4111-4571] 11% | 961 [879-1131] 26% | 0.223 | 1.0000 | 12 % |
| c8-100 | p0 | B p95 delay (ms) | 7080 [6350-7740] 20% | 1641 [1435-1874] 27% | 0.232 | 1.0000 | 50 % |
| c8-100 | p0 | B' mean (src phase) (ms) | 4248 [3940-4388] 11% | 963 [882-1133] 26% | 0.227 | 1.0000 | 8 % |
| c8-100 | p0 | B' p95 (src phase) (ms) | 7120 [6062-7740] 24% | 1641 [1435-1874] 27% | 0.230 | 1.0000 | 50 % |
| c8-100 | p1 | C rtp_floor (us) | 34044 [33659-37028] 10% | 39466 [36626-42474] 15% | 1.159 | 0.0030 | 8 % |
| c8-100 | p1 | A src-phase floor (us) | 36902 [34014-40037] 16% | 39466 [36626-42474] 15% | 1.069 | 0.0512 | 20 % |

Other arms in the same ledger (median [min-max] spread):

| cell | leg | clause | arm | value |
|---|---|---|---|---|
| c8-100 | p0 | C rtp_floor | W0 | 8394 [6050-9030] 35% |
| c8-100 | p0 | C rtp_floor | W05 | 7981 [6474-9737] 41% |
| c8-100 | p0 | C rtp_floor | WDER | 8249 [6786-10725] 48% |
| c8-100 | p1 | C rtp_floor | W0 | 34902 [32207-38438] 18% |
| c8-100 | p1 | C rtp_floor | W05 | 35755 [32949-38473] 15% |
| c8-100 | p1 | C rtp_floor | WDER | 37628 [32770-37943] 14% |
| c8-100 | p0 | A src-phase floor | W0 | 8974 [8122-11096] 33% |
| c8-100 | p0 | A src-phase floor | W05 | 10862 [10129-11820] 16% |
| c8-100 | p0 | A src-phase floor | WDER | 11023 [8202-13565] 49% |
| c8-100 | p1 | A src-phase floor | W0 | 35378 [32207-38438] 18% |
| c8-100 | p1 | A src-phase floor | W05 | 35933 [32949-38473] 15% |
| c8-100 | p1 | A src-phase floor | WDER | 37628 [32770-37943] 14% |
| c8-100 | p0 | B mean delay | W0 | 4259 [4183-4754] 13% |
| c8-100 | p0 | B mean delay | W05 | 4594 [4086-4837] 16% |
| c8-100 | p0 | B mean delay | WDER | 4448 [3929-4732] 18% |
| c8-100 | p0 | B p95 delay | W0 | 6999 [6805-7914] 16% |
| c8-100 | p0 | B p95 delay | W05 | 7309 [6890-7930] 14% |
| c8-100 | p0 | B p95 delay | WDER | 6938 [6614-7293] 10% |
| c8-100 | p0 | B' mean (src phase) | W0 | 4144 [4055-4608] 13% |
| c8-100 | p0 | B' mean (src phase) | W05 | 4451 [3960-4795] 19% |
| c8-100 | p0 | B' mean (src phase) | WDER | 4403 [3791-4616] 19% |
| c8-100 | p0 | B' p95 (src phase) | W0 | 7035 [6815-7894] 15% |
| c8-100 | p0 | B' p95 (src phase) | W05 | 7234 [6804-7959] 16% |
| c8-100 | p0 | B' p95 (src phase) | WDER | 6897 [6226-7293] 15% |

#### place41b (BASE 12 vs MAIN 6, gate off; c1s and c1d)

| cell | leg | clause (unit) | ref arm: median [min-max] spread | test arm: median [min-max] spread | test/ref | p (1-sided) | A/A d80 |
|---|---|---|---|---|---|---|---|
| c1s-400 | p0 | C rtp_floor (us) | 2570 [2385-2684] 12% | 2562 [2363-2649] 11% | 0.997 | 0.6828 | 8 % |
| c1s-400 | p0 | A src-phase floor (us) | 2570 [2385-2684] 12% | 2562 [2363-2649] 11% | 0.997 | 0.6828 | 8 % |
| c1s-400 | p0 | B mean delay (ms) | 19.1 [13.6-29.7] 84% | 20.0 [16.5-543] 2631% | 1.046 | 0.3315 | 100 % |
| c1s-400 | p0 | B p95 delay (ms) | 26.2 [22.1-65.1] 164% | 34.9 [21.4-1028] 2884% | 1.334 | 0.4016 | >100 % |
| c1s-400 | p0 | B' mean (src phase) (ms) | 19.1 [13.6-29.7] 84% | 20.0 [16.5-542] 2628% | 1.046 | 0.3315 | 100 % |
| c1s-400 | p0 | B' p95 (src phase) (ms) | 26.2 [22.1-65.1] 164% | 34.9 [21.4-1028] 2884% | 1.334 | 0.4016 | >100 % |
| c1d-400 | p0 | C rtp_floor (us) | 2476 [2343-2600] 10% | 2423 [2328-2528] 8% | 0.979 | 0.8749 | 8 % |
| c1d-400 | p0 | A src-phase floor (us) | 2476 [2343-2600] 10% | 2423 [2328-2528] 8% | 0.979 | 0.8749 | 8 % |
| c1d-400 | p0 | B mean delay (ms) | 26.5 [13.3-124] 419% | 17.1 [13.5-26.4] 76% | 0.645 | 0.9704 | >100 % |
| c1d-400 | p0 | B p95 delay (ms) | 46.0 [20.5-330] 671% | 35.1 [24.5-44.6] 57% | 0.761 | 0.9075 | >100 % |
| c1d-400 | p0 | B' mean (src phase) (ms) | 26.5 [13.3-102] 336% | 17.1 [13.5-26.4] 76% | 0.645 | 0.9704 | >100 % |
| c1d-400 | p0 | B' p95 (src phase) (ms) | 46.0 [20.5-281] 566% | 35.1 [24.5-44.6] 57% | 0.761 | 0.9075 | >100 % |
| c1d-400 | p1 | C rtp_floor (us) | 2484 [2388-2629] 10% | 2390 [2312-2523] 9% | 0.962 | 0.9589 | 8 % |
| c1d-400 | p1 | A src-phase floor (us) | 2484 [2388-2629] 10% | 2390 [2312-2523] 9% | 0.962 | 0.9589 | 8 % |

Other arms in the same ledger (median [min-max] spread):

| cell | leg | clause | arm | value |
|---|---|---|---|---|
| c1s-400 | p0 | C rtp_floor | WIRE | 2485 [2404-2655] 10% |
| c1d-400 | p0 | C rtp_floor | WIRE | 2442 [2392-2514] 5% |
| c1d-400 | p1 | C rtp_floor | WIRE | 2392 [2264-3032] 32% |
| c1s-400 | p0 | A src-phase floor | WIRE | 2485 [2404-2655] 10% |
| c1d-400 | p0 | A src-phase floor | WIRE | 2442 [2392-2514] 5% |
| c1d-400 | p1 | A src-phase floor | WIRE | 2392 [2264-3032] 32% |
| c1s-400 | p0 | B mean delay | WIRE | 18.0 [17.1-19.6] 14% |
| c1d-400 | p0 | B mean delay | WIRE | 17.4 [14.2-20.0] 34% |
| c1s-400 | p0 | B p95 delay | WIRE | 30.3 [25.4-46.3] 69% |
| c1d-400 | p0 | B p95 delay | WIRE | 28.2 [20.7-50.7] 107% |
| c1s-400 | p0 | B' mean (src phase) | WIRE | 18.0 [17.1-19.6] 14% |
| c1d-400 | p0 | B' mean (src phase) | WIRE | 17.4 [14.2-20.0] 34% |
| c1s-400 | p0 | B' p95 (src phase) | WIRE | 30.3 [25.4-46.3] 69% |
| c1d-400 | p0 | B' p95 (src phase) | WIRE | 28.2 [20.7-50.7] 107% |


## 6. Does (A) still sit on a start-up reading?

`rtp_us` is a 10 s windowed minimum, so the tick at t = 1.0 s already carries the start-up sample from t < 1 s, and on a row shorter than the window (every §52 guard row: last tick 3.5-9.1 s) nothing can raise it afterwards; (A) can then only equal the t = 1.0 reading. Counts over (row, leg) pairs of the BASE and CAND arms:

| ledger, cells | (row, leg) pairs | A = reading at its first window tick | A = the run's first non-zero `rtp_us` | A = C |
|---|---|---|---|---|
| §52 guard, c1s/c1d/c2 | 96 | 95 | 89 | 96 |
| §52 guard, c8 | 48 | 45 | 45 | 37 |
| §48b, c1s/c1d/c2 | 96 | 91 | 88 | 95 |
| §48b, c8 | 48 | 33 | 32 | 10 |
| §53, c8 600 MB (BASE, CAND, BASES, CANDS) | 48 | 30 | 30 | 30 |
| §53, c1d 5 GB (BASE, CAND, CANDS) | 60 | 47 | 45 | 60 |
| §52 rep, c8 600 MB (BASE, CAND) | 24 | 18 | 18 | 12 |

Per cell and arm (§52 guard; `where.py`):

| cell | arm | leg | rows | C: start-up (t<=0.5) | C: drained tail | A defined | A: = 1st window tick | A == C | A == run's first non-zero rtp_us | A: at last source ticks (t >= last_src-0.5) |
|---|---|---|---|---|---|---|---|---|---|---|
| c1d-400 | BASE | p0 | 12 | 10 | 0 | 12 | 11 | 12 | 9 | 0 |
| c1d-400 | BASE | p1 | 12 | 12 | 0 | 12 | 12 | 12 | 10 | 0 |
| c1d-400 | CAND | p0 | 12 | 12 | 0 | 12 | 12 | 12 | 12 | 0 |
| c1d-400 | CAND | p1 | 12 | 12 | 0 | 12 | 12 | 12 | 11 | 0 |
| c1s-400 | BASE | p0 | 12 | 12 | 0 | 12 | 12 | 12 | 12 | 0 |
| c1s-400 | CAND | p0 | 12 | 12 | 0 | 12 | 12 | 12 | 11 | 0 |
| c2-100 | BASE | p0 | 12 | 12 | 0 | 12 | 12 | 12 | 12 | 0 |
| c2-100 | CAND | p0 | 12 | 12 | 0 | 12 | 12 | 12 | 12 | 0 |
| c8-100 | BASE | p0 | 12 | 6 | 6 | 12 | 11 | 6 | 11 | 1 |
| c8-100 | BASE | p1 | 12 | 12 | 0 | 12 | 12 | 12 | 12 | 0 |
| c8-100 | CAND | p0 | 12 | 5 | 5 | 12 | 10 | 7 | 10 | 2 |
| c8-100 | CAND | p1 | 12 | 12 | 0 | 12 | 12 | 12 | 12 | 0 |

§53 c8 600 MB (A = first window tick on 30 of 48 pairs; p0 only on BASE and BASES: 5 of 6 each, where A = C on 0 of 6):

| cell | arm | leg | rows | C: start-up (t<=0.5) | C: drained tail | A defined | A: = 1st window tick | A == C | A == run's first non-zero rtp_us | A: at last source ticks (t >= last_src-0.5) |
|---|---|---|---|---|---|---|---|---|---|---|
| c8-100 | BASE | p0 | 6 | 0 | 6 | 6 | 5 | 0 | 5 | 1 |
| c8-100 | BASE | p1 | 6 | 0 | 1 | 6 | 0 | 5 | 0 | 0 |
| c8-100 | BASES | p0 | 6 | 0 | 6 | 6 | 5 | 0 | 5 | 1 |
| c8-100 | BASES | p1 | 6 | 0 | 2 | 6 | 0 | 4 | 0 | 0 |
| c8-100 | CAND | p0 | 6 | 6 | 0 | 6 | 6 | 6 | 6 | 0 |
| c8-100 | CAND | p1 | 6 | 4 | 2 | 6 | 5 | 4 | 5 | 0 |
| c8-100 | CANDS | p0 | 6 | 5 | 1 | 6 | 6 | 5 | 6 | 0 |
| c8-100 | CANDS | p1 | 6 | 3 | 0 | 6 | 3 | 6 | 3 | 0 |

§48b:

| cell | arm | leg | rows | C: start-up (t<=0.5) | C: drained tail | A defined | A: = 1st window tick | A == C | A == run's first non-zero rtp_us | A: at last source ticks (t >= last_src-0.5) |
|---|---|---|---|---|---|---|---|---|---|---|
| c1d-400 | BASE | p0 | 12 | 9 | 0 | 12 | 9 | 12 | 9 | 0 |
| c1d-400 | BASE | p1 | 12 | 11 | 0 | 12 | 11 | 12 | 9 | 0 |
| c1d-400 | CAND | p0 | 12 | 12 | 0 | 12 | 12 | 12 | 12 | 0 |
| c1d-400 | CAND | p1 | 12 | 11 | 0 | 12 | 12 | 12 | 11 | 0 |
| c1s-400 | BASE | p0 | 12 | 12 | 0 | 12 | 12 | 12 | 12 | 0 |
| c1s-400 | CAND | p0 | 12 | 11 | 0 | 12 | 11 | 12 | 11 | 0 |
| c2-100 | BASE | p0 | 12 | 12 | 0 | 12 | 12 | 12 | 12 | 0 |
| c2-100 | CAND | p0 | 12 | 11 | 1 | 12 | 12 | 11 | 12 | 0 |
| c8-100 | BASE | p0 | 12 | 1 | 11 | 12 | 11 | 1 | 11 | 1 |
| c8-100 | BASE | p1 | 12 | 2 | 4 | 12 | 4 | 8 | 4 | 1 |
| c8-100 | CAND | p0 | 12 | 0 | 12 | 12 | 12 | 0 | 12 | 0 |
| c8-100 | CAND | p1 | 12 | 1 | 11 | 12 | 6 | 1 | 5 | 1 |

§52 rep (c8 600 MB):

| cell | arm | leg | rows | C: start-up (t<=0.5) | C: drained tail | A defined | A: = 1st window tick | A == C | A == run's first non-zero rtp_us | A: at last source ticks (t >= last_src-0.5) |
|---|---|---|---|---|---|---|---|---|---|---|
| c8-100 | BASE | p0 | 6 | 0 | 6 | 6 | 6 | 0 | 6 | 0 |
| c8-100 | BASE | p1 | 6 | 0 | 3 | 6 | 0 | 3 | 0 | 0 |
| c8-100 | CAND | p0 | 6 | 3 | 3 | 6 | 6 | 3 | 6 | 0 |
| c8-100 | CAND | p1 | 6 | 6 | 0 | 6 | 6 | 6 | 6 | 0 |

## 7. What each clause measures and how they compare (facts)

Spread and resolution. The 12-row arms of the §52 guard have relative range for C and A of 12-15 % at c1s, 10-24 % at c1d, 59-67 % at c2 and 35-41 % at c8 p0 (17-34 % at c8 p1); for B mean 26-47 % (c1s), 103-123 % (c1d), 91-109 % (c2) and 73-101 % (c8), with B p95 wider (77-236 %). B has heavy single-row outliers at c1 (BASE row 96 ms against a 22 ms median at c1s in §48b; one row of 645 ms mean at c1d 5 GB), which set the spread there. A/A d80 (§52 guard): C 5 % (c1s), 5 % and 8 % (c1d p0, p1), 50 % (c2), 20 % and 12 % (c8 p0, p1); A is identical to C everywhere except c8 p0 (20 % to 50 %); B mean 20 % (c1s), 100 % (c1d), 50 % (c2), >100 % (c8); B p95 100 %, >100 %, 100 %, >100 %. On the 600 MB c8 ledgers (6 + 6, fewer clauses) d80 is C 20 % / 8 % (§53) and 50 % / 8 % (rep52) at p0 / p1, A 8 % / 8 % and 12 % / 20 %, B mean 12 % and 12 %, B p95 12 % and 50 %. In the §48b ledger B mean has d80 5 % at c2 and 20 % at c8, >100 % at c1s and 100 % at c1d.

What each reads. C is the minimum over the whole run; on short rows (c1s, c1d, c2 and most c8 guard rows) it equals the start-up sample, the minimum of a window that already holds it, and on c8 it is the drained-tail sample on 6 of 12 BASE and 5 of 12 CAND rows (status item 7; counts above reproduce it). A equals C on every c1s, c1d and c2 row of the §52 guard and on 37 of 48 c8 pairs; it equals the run's first non-zero reading on 89 of 96 and 45 of 48 pairs, so on rows shorter than the 10 s window it carries the same start-up sample as C, and at 600 MB it moves c8 p0 BASE from 8.7 ms (C, the tail) to 10.9 ms (A, the t = 1.0 tick, which carries the start-up window) while CAND stays at 10.2 ms. B is not an RTT: it is the horizontal distance between the sent curve and the in-order ack curve, so it contains receiver reordering and hole waiting (the p0 backlog and the tail), and it is the only one of the three that sees the 600 MB tail change (BASE 4.4 s to CAND 0.96 s mean, 6.8 s to 1.5 s p95; CW0 0.81 s). Ticks are 250 ms apart and `t` is printed to 0.1 s, so at c1s/c1d (medians 17-28 ms) and c2 (83 ms) the delay is below or near the tick quantum and only its mean over many ticks is meaningful; the p95 there is quantised. At c8 (0.4-1.6 s at guard size, 4 s at 600 MB) it is resolved directly. B' (source phase only) differs from B by at most a few percent of the median except on rows with a long drained tail (c1 outliers).

BASE to CAND across the tail change at c8 p0 (ratio test/ref, one-sided p):

| ledger | C | A | B mean |
|---|---|---|---|
| §52 guard (7.5-9.1 s rows) | 1.124, p 0.081 | 1.021, p 0.77 | 0.743, p 0.97 |
| §52 rep, 600 MB | 1.393, p 0.0030 | 1.036, p 0.22 | 0.223, p 1.00 |
| §53, 600 MB BASE vs CAND | 1.175, p 0.087 | 0.941, p 0.95 | 0.217, p 1.00 |
| §53, 600 MB BASES vs CANDS | 1.492, p 0.0090 | 1.093, p 0.56 | 0.226, p 1.00 |
| §48b (a different candidate; 47-50 s rows) | 0.814, p 0.995 | 1.111, p 0.040 | 1.461, p 0.0001 |

At c8 p1 the three-way readings are C 1.048 (p 0.023) / A 1.045 (p 0.050) at §53 600 MB, C 1.159 (p 0.0030) / A 1.069 (p 0.051) at rep52, and C 0.961 / A 0.961 (identical, p 0.556) in the §52 guard. In §48b the bbr_rs candidate reads B mean 1.461 and B p95 1.194 at c8 (p 0.0001 each) and B mean 1.019 at c2 (p 0.017), where C reads c8 p0 0.814 (p 0.995) and A reads 1.111 (p 0.040). At c1s, c1d and c2 of the §52 guard C and A give identical p; B gives p between 0.14 and 0.94.

## 8. Reproduce

`python -I rowvals.py out.json ld.log diag-dir ...` (per-row values), `python -I score.py ...` (statistics, `res-*.json/.tsv`), `python -I resolution.py ...` (`d80-*.json`; `d80s-*.json` for B'), `python -X utf8 -I make_tables.py`, `python -I where.py rowvals-*.json BASE,CAND`; the directory `docs/derivations/guard-clause-dry/` holds the scripts, per-row values and results. Archives were extracted into fresh directories outside the repository and treated as data.
