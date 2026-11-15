## 49. Simplify batch 1 (`simplify-2`) against main — the guard session — pre-registration (before any VM contact)

**What is tested.** `simplify-1` (from old main `4926822`) deletes the default-off arms `RWM_PLAIN_RS`, `RWM_RS_ATTR`, `RWM_HONEST_K`, `RWM_HONEST_CAP`, `RWM_POOL_ANCHOR`, `RWM_REPAIR_WAIT`, `RWM_PROACTIVE_PACER`, `RWM_REASM_BDP` and `RWM_COMPOSED_CAP` plus dead code; each arm is tagged `archive/RWM_*`. `simplify-2` is that work with current main merged in. On the default path every deleted gate resolved off or inert, so the expected behaviour change is none. **The guard asks whether the deletions are free.**

**Trees.**
- **BASE = main at launch** (`LD_BASE_SHA`, full sha, recorded by amendment).
- **CAND = `simplify-2`** (main merged in, so `git diff BASE CAND -- raptorpath/src` is the deletions alone), pinned by amendment (`LD_CAND_SHA`).

**Session (`LD_PLAN=41c`, `LD_P46_SUITES=1`).** The suites on CAND's tree first (release, doc, wasm, debug-witness, every `tools/l1` test; a REAL failure is `ABORT-TESTS`), then §41c's guard exactly: `rank-minp`, N_PERM 10 000, seed 4141, one-sided, PASS iff p > 0.05; 26 clauses; BASE n = 12 and CAND n = 12 at c1s-400, c1d-400, c2-100 and c8-100 default (96 rows plus the 2-row smoke); §41c's DNF rule and replacement rule (≤ 4 witness-voided slots, re-run once); an incomplete design reads NO VERDICT. No §46 stage.

**Readouts.** The guard's verdict block, the §21 readings, the tail line, QCWND-BELOW-BDP; on the host afterwards the resolution table (`resolution41c`, reported only).

**Prediction.** PASS. Every deleted arm is off on the default path; the risk is a merge error in the shared files (`net/diag.rs`, `net/control_msg.rs`, `scheduler/*`), which the suites and the guard both see.

**Decision rule.**
- **PASS:** `simplify-2` merges to main (`--no-ff`), and status §1's on-list drops `RWM_RS_ATTR` and the `RWM_POOL_ANCHOR` / `RWM_HONEST_CAP` / `RWM_PLAIN_RS` sentences in the same merge, with the `[GATES]` byte pin.
- **FAIL:** no merge; the failing clause, its size and the resolution table go to the user. Bisection is by arm (one commit per arm on `simplify-1`).
- **NO VERDICT:** re-run.
- **ABORT-TESTS:** fixed on the branch and re-run.

§48b's BASE is pinned only after this verdict, to main as it then is.

**§49 amendment 1 (before VM contact): the trees.**
- **BASE = main `b3000701f440d11b1b5ad946c8b36c3623c770cd`** (`LD_BASE_SHA`). Later main commits are status-only.
- **CAND = `simplify-2` `c634f3326254767d0fd03a99efb8078ca5e52fa3`** (`LD_CAND_SHA`): main merged in, so `git diff BASE CAND -- raptorpath/src` is the deletions alone (20 files, +142 / −2 665).
- **Host checks at CAND:** `cargo check --all-targets` rc 0; all 28 `tools/l1` python tests pass. The host lib suites were not run: the host disk filled (os error 112). The session's suites are the test of record.
- **Merge review:** `[DIAG]` per path loses `kraw=` and `sr=…/g…d…` (the deleted `RWM_HONEST_K` and `RWM_POOL_ANCHOR` gauges). No `tools/l1` parser reads either field; they appear only in test fixtures.

**§49 session 1: `ABORT-TESTS` before any test ran** (2026-11-12T04:16:47Z). A harness defect: plan 41c's `LD_P46_SUITES` path called `run_suites` above its definition in `loopdiag_run_all.sh` (the path had never run; plan 47 calls the suites further down). Both binaries built (BASE `a3399061…`, identical to §41c session 2's CAND binary, as `raptorpath/src` is). Fixed on main `baa92d5` (the call moved below the definition, and that path now runs every `tools/l1` python test, as §49 registered), with two harness tests.

**§49 amendment 2 (before VM contact): re-pinned CAND.** CAND = `simplify-2` `cd84a7f6a2feecc47cc522a2d0fbd3b900d505f4` (main `baa92d5` merged in; `raptorpath/src` unchanged from amendment 1's CAND). BASE stays `b3000701` (main's later commits touch no `raptorpath/src`).

**§49 session 2: `ABORT-TESTS`** (CAND `cd84a7f6`, 2026-11-12T05:22:10Z). Release 1131 passed / 2 failed, debug-witness 994 / 2, doc ok, wasm 35, every `tools/l1` python test ok. The two failures, in both profiles: `the_diag_line_reports_the_rtt_sigma_the_recovery_clock_needs` (no `[DIAG]` tick with a cwnd-full path) and `the_frontier_arm_runs_and_moves_the_argmin_on_the_dual` (`hol_mv=0`; release passed on the solo re-run).
- **Not a default-path change.** Both tests set `RWM_PLAIN_RS=1`. On main that ran the plain-RS store with `RWM_HONEST_CAP` (default on) live, and that arm is what drove the path to its window. Bisect (fresh target per commit): first bad `c8aaff29`, the `RWM_HONEST_CAP` removal. Control: main `4926822` with the tests' `RWM_PLAIN_RS=0` (the default path) fails the same way: sigma 0 of 13 saturated ticks in 2 of 2 runs, frontier 1 of 2.
- **Discarded evidence:** an earlier sweep shared one target dir across 13 source trees, so every test ran a stale pre-simplify binary (`RWM_POOL_ANCHOR` strings present) and read "pass". Its results are void.
- **Fix** (`simplify-2` `b1673419`, tests only): both tests set `RWM_STORE=4096` in place of the dead `RWM_PLAIN_RS=1`. Sigma passes 3 of 3; place_arms' other four tests pass. The frontier assertion is `#[ignore]` with its evidence: on the default path the frontier term moves the argmin in only some runs (2 of 3 at 4096, 5 of 6 at 16384, 1 of 2 on main). The arm `RWM_PLACE_HOL` is default off; INERT-AS-DERIVED is its reading.

**§49 amendment 3 (before VM contact): re-pinned CAND** = `simplify-2` `b16734195caf5de56c3cc6b895aa9f77289705c7` (`raptorpath/src` unchanged; two test files differ from amendment 2's CAND). BASE stays `b3000701`.

**§49 session 3 (CAND `b1673419`): suites PASS, then `ABORT-SMOKE`** (2026-11-12T07:37:54Z).
- **Suites:** release 1132 passed / 0 failed, debug-witness 995 / 0, doc ok, wasm 35, every `tools/l1` python test ok.
- **Smoke:** CAND LIVE (835.6 Mbit/s). BASE `WITNESS-FAIL` on `wpl_us-numeric-on-BASE wire-gauges-on-BASE`: a third harness assumption that BASE is the unpromoted `4926822`. The new main carries ADR-0073's promotion and prints those gauges, as CAND does. Fixed on main (`place38_parse.base_promoted()`: BASE counts as promoted when `LD_BASE_SHA` names a later main), with two tests. Dry run on this session's real smoke logs: both rows LIVE with `LD_BASE_SHA` set; the original failure reproduces with it unset.

**§49 amendment 4 (before VM contact).** CAND = `simplify-2` `91625f3a7a8aced20d078f8d0ebe6be9c4fd448a` (`raptorpath/src` and `raptorpath/tests` byte-identical to session 3's CAND). Session 3's suites bind it, as in §41c amendment 6, so session 4 runs `LD_PLAN=41c` without `LD_P46_SUITES`: the smoke and the guard. BASE stays `b3000701`.

**§49 session 4: `ABORT-TESTS` at the parser gate** (2026-11-12T07:43:26Z, before any build). `test_place38_parse.py`'s historical-BASE checks ran with the session's `LD_BASE_SHA` in their environment and read the promoted BASE. The test now clears the variable first (main), and all 28 `tools/l1` tests pass with it set.

**§49 amendment 5 (before VM contact).** CAND = `simplify-2` `b1e6bfb7c3ac79597380c05ead6da3619e49b2d0` (`raptorpath/src` and `raptorpath/tests` byte-identical to session 3's). Session 3's suites carry; smoke and guard only. BASE stays `b3000701`.

**§49 session 5: `ABORT-SMOKE`** (2026-11-12T08:17:28Z). BASE again `WITNESS-FAIL` on the wire gauges: `loopdiag_run_all.sh` runs the battery under `sudo -n env`, which resets the environment, so the row parser never saw `LD_BASE_SHA`. Verified on the VM (a plain `sudo -n env` drops it; the explicit pass-through delivers it). Fixed on main: both battery env arrays pass `LD_BASE_SHA`, and an empty value reads as unset. CAND LIVE (798.1 Mbit/s).

**§49 amendment 6 (before VM contact).** CAND = `simplify-2` `54600e5d75b7dfb772e2acdfdbede0ddbb647304` (`raptorpath/src` and `raptorpath/tests` byte-identical to session 3's). Session 3's suites carry; smoke and guard only. BASE stays `b3000701`.

**§49 Result (session 6, CAND `54600e5d`, BASE `b3000701`, launched 2026-11-12T08:32Z, `LD-ALL-DONE` 08:46:51Z): `GATE-41c PASS`, p = 0.9566.** Suites: session 3's (release 1132 / 0, debug-witness 995 / 0, doc, wasm 35, every `tools/l1` test), on byte-identical `raptorpath/src` and `raptorpath/tests`.
- **Design complete:** 96 / 96 rows LIVE, no witness voids, 0 replaced slots, no DNF or crash rows, ABORT-TOKENS none.
- **Min-p (true)** 0.158 at c8 `cpu_srv/GB` (x = +5.7 %). No clause has p < 0.15. Worse-side readings above 3 %: c2 `rtp_floor_p0` +10.6 % (p 0.35), c1s `lag_E_cli` +10.4 % (p 0.68), c1d `lag_E_cli` +6.1 % (p 0.43), c8 `cpu_srv/GB` +5.7 %, c8 `lag_E_srv` +3.3 % (p 0.23). Goodput: c1s −0.8 %, c1d −2.4 %, c2 −0.1 %, c8 +1.4 %.
- **§21 readings:** every clause WITHIN (c1s goodput WITHIN with the CAND range [148.5–849.3]).
- **Slow row (reported only): 1 of 12 CAND rows against 0 of 12 BASE rows, unresolved at this n.** c1s CAND s42 r6 at 148.5 Mbit/s (0.178 × the cell median). It is §41b's quinn congestion-window collapse: from 2.5 s quinn's window sits at 11–16 kB (BDP 250 kB) on 89 % of lines, with ≈ 2 050 datagrams held in quinn (`hand − tx`), wire RTT 2 ms and app-echo RTT 435 ms. Simplify touches no `transport/` file. The row's `[DIAG]` lines are kept (`docs/l1-raw/simplify49/c1s-CAND-s42-r6-diag.log`). This is the defect §48b tests.
- **Tail:** apart from that row, every arm's lowest row is ≥ 0.889 × its cell median.
- **QCWND-BELOW-BDP:** c1s CAND range [0–0.890] (that row), BASE 0.
- **Resolution (`resolution41c`, seed 0, reported only):** goodput d80 c1s 5 %, c1d 8 %, c2 2 %, c8 12 %; CPU/GB d80 3–12 % (power at +2.4 %: c1s client 0.73, server 0.70; c2 client 0.63; elsewhere ≤ 0.27).
- **Decision (§49 rule): PASS → `simplify-2` merges to main**, with status §1's on-list edit. Ledgers: `docs/l1-raw/simplify49/` (scrubbed).
- **Debt carried:** `RWM_PLACE_HOL`'s execution witness is `#[ignore]`d (archive the arm, or build a deterministic fixture: the user's call); 16 other tests still set the dead `RWM_PLAIN_RS=1` and need classifying (those with a regime assertion may now pass without reaching their case); status §2's verdict table is stale.

**§48b amendment 1 (before VM contact): trees, harness, scoring choices, rehearsal.**
- **BASE = main `0a1ddb88b9445d7dca16959dca07c66d48af57c3`** (after §49's merge; `LD_BASE_SHA`). **CAND = `bbr48cand` `e5d7ffbd3a6701cdab924ed9937c5a86877ad65c`** (`LD_CAND_SHA`): `bbrfix48b` + `harness48` + that main. `git diff --stat BASE CAND -- raptorpath/src raptorpath/tests` lists exactly the seven registered files (`net/diag.rs`, `transport/{bbr_rs,cc_sim,io_owner,mod,quic}.rs`, `tests/qcc_gauge_loopback.rs`).
- **Host checks:** on `bbrfix48b` (`ab712e65`) debug lib 630 / 0 / 16, `cc_sim` 7 / 0 / 6 ignored by design, `qcc_gauge_loopback` 1 / 0. `recv_alloc_bound` reads 24.47 on this host, but unmodified main reads 24.57 here too: a host artifact; the VM suites decide. The last merge (main with §49) auto-merged `net/diag.rs` and was not compiled on the host (out of memory); the rehearsal builds it.
- **Harness (`harness48`, plan 48, `bbr48_score.py`, 101 checks):** built without a red-first demonstration. Dry run on §41c session 2's real ledger reproduces its GATE-41c p = 0.2256 and min-p 0.01160 exactly; p_rest there is 0.2098. (Item 6's wording "as both arms' pool" is replaced by this: the scorer is fed the real 96-row ledger.)
- **Scoring choices the registration left open, now fixed:**
  1. A row's R2b is the max over its active legs; NOT-CONTROLLED reads the median over CAND rows of that, and the 0.25 row limit on it. The per-leg alternative is printed (`R2B-LEG-READING`), not scored.
  2. A leg is active on a line when its `qlp` sent counter advanced since the previous `[DIAG]` line; R2 and R3 read lines at t ≥ 1 s.
  3. R3's arm figures are the mean and p50 of per-row medians.
  4. The tail clause's median is over completed CAND rows; a CAND DNF or crash counts as goodput 0 and fails it.
  5. The ladder: NO VERDICT, then NOT-CONTROLLED, then the rest as registered. R1 PASS with a failed tail clause reads REGRESSION (PROMOTE-ELIGIBLE needs the tail; CONTROLLED-WITH-COST needs R1 to fail).
  6. c8's slow leg is leg 1 (c3); its wrtt p50 is the median over rows of each row's median.
- **Rehearsal first (`LD_REHEARSE=1`, unscored).** Every plan-48 path is unexecuted, and five of §49's six launches died on such paths. So a short session runs the two builds, the CAND and BASE smoke rows at c8 and `smoke48`, then stops (`LD-ALL-REHEARSAL-DONE`). Its rows are never scored. Only after `REHEARSAL-PASS` does the full session (suites, smoke, 96 rows) launch, on the same pins.

### 48b. Result — `VERDICT48 REGRESSION` by the ladder; the collapse is gone, the cost is c8 CPU and 1.5–2.6 % goodput at c2/c8

Session `LD_PLAN=48`, CAND `bbr48cand` `e5d7ffbd` (`RWM_QUIC_CC=bbr_rs`), BASE main `0a1ddb88`, launched 2026-11-12T09:19:53Z, `LD-ALL-DONE` 11:50:20Z. Rehearsal first (`REHEARSAL-PASS`, unscored). Ledgers: `docs/l1-raw/bbr48b/` (scrubbed).
- **Suites:** release 1143 passed / 1 failed, the failure `two_path_client_fails_fast_when_server_is_killed` a FLAKE (passed on the immediate solo re-run); debug-witness 1007 / 0; doc ok; wasm 35; every `tools/l1` test ok.
- **Design complete:** 96/96 LIVE, no DNF or crash rows, no replaced slots, ABORT-TOKENS none.
- **R1 (§41c guard): FAIL, p = 0.0020** (min-p 0.0001 at c2 goodput). **p_rest = 0.0101** (min-p 0.0006 at c8 `cpu_cli/GB`), so `CONTROLLED-WITH-COST` does not apply. **Ladder: 1 NOT-CONTROLLED no; 2 PROMOTE-ELIGIBLE no (R1 fails; the tail clause holds); 3 CONTROLLED-WITH-COST no (p_rest ≤ 0.05); 4 `REGRESSION`.** Registered prediction was `CONTROLLED-WITH-COST`: wrong on p_rest, through c8 CPU.
- **Worse-side clauses** (x > 0 = CAND worse): c2 goodput +1.48 % (p 0.0001), c8 goodput +2.56 % (p 0.0001), c8 `cpu_cli/GB` +3.51 % (p 0.0006), c8 `cpu_srv/GB` +5.32 % (p 0.0059), c1s `lag_E_srv` +16.4 % (p 0.40), c1d `lag_E_cli` +11.7 % (p 0.08). Better side: c1s goodput −2.3 %, c1d goodput −4.9 %, c1s/c1d/c2 CPU −1.4 to −3.0 %.
- **The collapse (R2b, the §47 witness):** CAND median R2b ≤ 0.012 at every cell, no row above 0.25: **controlled**. BASE R2b reaches 0.898 (c1s s42 r3) and 0.708 (c1d s42 r5). Tail: BASE c1s lowest row 265.5 Mbit/s (0.307 × median, 3 of 12 below 0.5 ×), c1d lowest 556.2 (0.630 ×); CAND c1s lowest 847.4 (0.980 ×), c1d 840.0 (0.952 ×).
- **R3 (authority and delay), means of row medians:** wire RTT at c2 112.4 → 21.8 ms (×0.19); c8 fast leg 119.7 → 21.6 ms, slow leg 232.9 → 88.1 ms. App-echo A_x at c2 206.2 → 206.8 ms (unchanged: the queue moved into quinn, B 840 → 1804); c8 fast leg 187.8 → 122.5 ms, slow leg 247.7 → 251.5 ms. c1s B 1335 → 715, A_x 37.4 → 17.6 ms.
- **R4 (`bbr_rs` gauges):** qm ≈ 98 % ProbeBW, ProbeRtt 1.2–2.0 % of blocks. Median m̂: c1 2.15–2.20 ms, c2 7.81 ms, c8 8.32 / 39.4 ms. **Loop margin 2·m̂/R̄: c1s 0.84, c1d 1.62, c2 0.72, c8 0.73 / 0.87.** Below 1 means W = 2·b̂·m̂ < b̂·R̄: the window sits below the loop's in-flight.
- **Cost attribution (plan W1.1, fixed before the result, reported only; `attribution.txt`, `attr48.py`):** on CAND's lowest-goodput third of rows, active-leg lines at t ≥ 1 s are QUINN-LIMITED (B ≥ 98 and qm ∈ {B,S,D}) **97.3 % at c2, 83.0 % at c8**; STORE-LIMITED 0 % / 6.6 %, UNDER-READ 0 % / 6.5 %, PROBE-RTT 2.1 % / 1.9 %. **The class naming the cost is QUINN-LIMITED.** Read with R4: the window binds because m̂ (the RTT floor) is well below the loop time R̄; b̂ is not under-read at c2.
- **Not scored, noted:** H1 (§48b derivation) is not read here; `[SUMCAP]` was not part of plan 48's readouts.
- **Decision (§48b item 7):** `REGRESSION` goes to the user: archive `bbr_rs`, or §47's options 1/3, or the W1.2 route (QUINN-LIMITED → the δ-free window fix first: a loop-time allowance from the measured ê = R̄ − m̂ on the window, then δ on the store).

### 48b. Erratum and supplement (same day, advisor review) — the label and numbers stand; two readings are withdrawn

**Withdrawn 1: "loop margin below 1 means the window sits below the loop's in-flight".** Quinn ignores `pacing_rate`, so the whole window is in flight and R̄ = W/x. Then 2·m̂/R̄ = (x/b̂)/(1 + A/(2·b̂·m̂)), which is below 1 whenever the window binds. So a margin below 1 reads *window-limited*, not *window too small*: c2's `qsr` (21.7 ms) is the 21.8 ms wire RTT the window itself produced, and c1s reads 0.84 with better goodput than BASE. The fix the Result proposed (an allowance from ê = R̄ − m̂ on the window) feeds the window's own queue back into the window: the form W1.2 rejected (candidate B).

**Withdrawn 2: "QUINN-LIMITED names the cost".** The registered attribution counts lines, not lost goodput, and its B ≥ 98 threshold does not discriminate: BASE also holds a large quinn backlog. The registered output (`attribution.txt`) stands as printed; the reading is withdrawn.

**Post-hoc supplement (not registered; `supplement.txt`, `supp48.py`, from the kept c2/c8 logs):**
- c2: BASE B p50 808, `paused` p50 72 %, joint (`paused` ≥ 50 % and B ≥ 98) 0.828; CAND B p50 1803, `paused` 71 %, joint 0.997. c8: BASE 600 / 79 % / 0.761; CAND 986 / 79 % / 0.904. **In both arms the store caps the in-flight; with `bbr_rs` its queue sits in quinn instead of on the link.**
- **ProbeRtt's own goodput cost:** c2, 2.0 % of lines at mean `good` 52.4 against 90.7 Mbit/s → **≈ 0.84 % of goodput** (of the 1.48 % deficit); c8, 3.3 % at 75.8 against 99.7 → **≈ 0.78 %** (of 2.56 %). The rest (≈ 0.6 % at c2, ≈ 1.8 % at c8) is unattributed.
- **c8 CPU** (the clause that decides the label): the placement split is unchanged (leg shares 0.823/0.177 vs 0.828/0.172), coded share 0.0058 vs 0.0064, loss per leg equal; CPU totals cli 20.78 → 21.51 s, srv 27.27 → 28.71 s on runs 2.6 % longer (48.1 → 49.4 s). CPU/GB rises only at the two-leg cell. **Unexplained.** Hypothesis, untested: `bbr_rs`'s per-packet send-record bookkeeping (up to 8 192 tracked records per path). A host ns bench decides it.

**Registered readouts missing from the Result:**
- **Quinn's `rtt.min()` (`qmin_us`), the §47 witness in vivo:** median 1 772 µs at c1s (lows 0), **84 µs** on c1d leg 0 and 704 µs on leg 1 (lows 0), against `bbr_rs`'s m̂ 2.15–2.20 ms on the same rows. **§47's inferred cause (a latched-low quinn min-RTT) is confirmed in vivo.**
- **Datagram evictions:** `dgq full` 0 in essentially every row (BASE c1d up to 246; CAND c1d up to 3); the `dgev` column is the hand − tx backlog, not evictions.
- **R2a (reported):** CAND medians 0.014–0.083 per leg; BASE c1s/c1d ranges reach 0.94 / 0.92.
- **`qcns` (`congestion_state()` cost per read):** mean 4.3–14.6 µs per read (one read per DIAG tick).
- **Predictions, line by line:**
  - Hit: c1 tail improves; R2b ≤ 0.02 (c2 0.005, c8 0.005); c2 `wrtt` ≤ 30 ms (21.8); c2 A_x within ±15 % (+0.3 %); c8 slow-leg `wrtt` ≤ 0.6 × BASE (0.38 ×); c2 goodput in [0.92, 1.00] × BASE (0.985); c8 in [0.90, 1.00] (0.974).
  - Missed: the loop-margin predictions (≥ 1.3 at c1 → c1s 0.84; 1.1–1.8 at c2 → 0.72). This was a prediction error, given the identity above.
  - Missed: the label (`CONTROLLED-WITH-COST` → `REGRESSION`, through p_rest at c8 CPU).
- **The resolution table** (`resolution41c`) is computed on the host and appended when done.
- **Suites note:** the dead-peer test's FLAKE is its first this session, on a tree that edits `io_owner.rs`. Recorded, not explained.

**What doing nothing costs (main, BASE rows of this session):** R2b > 0.25 in 4 of 12 c1s rows and 5 of 12 c1d rows; 3 of 12 c1s rows below 0.5 × the cell median (lowest 265.5 Mbit/s). Delivered delay at c2 is ≈ 206 ms in both arms.

**§48b user decision (2026-11-12): continue `bbr_rs`.** A new registration with the §48 ladder unchanged. First the δ-free fixes: ProbeRtt's goodput cost (≈ 0.8 % at c2/c8) and the unexplained c8 CPU cost (a host/VM ns bench of `bbr_rs`'s per-packet path against quinn's BBR decides it). Then δ on the store law (W1.2 candidate A). The `bbr_rs` code does not reach main before a passing registration.

**§48b resolution readout** (`resolution41c`, seed 0, reported only, a lower bound): goodput d80 c1s **100 %** (power at 5 %: 0.07), c1d 8 %, c2 1 %, c8 2.4 %; CPU/GB d80 c1s 50 %, c1d 5 %, c2 5–8 %, c8 8 %. The c1s goodput and CPU clauses are blind in this session: BASE's own collapsed rows (3 of 12 below 0.5 × median) inflate the A/A spread. The c2 and c8 goodput clauses resolve 1–2.4 %, so their +1.5 % / +2.6 % readings are inside what this design can see.

**§48b c8 diagnosis (post hoc, not registered; scripts and outputs in the session scratchpad `s48b/diag-c8/`, advisor-reviewed brief).** Supersedes the erratum's CPU hypothesis (per-packet controller bookkeeping): CPU/GB *fell* at c1s/c1d/c2 (c1d is two-leg), and the server, which runs no controller on data, rose more, so a per-packet controller cost is unlikely.
- **Q1, c8 CPU (cli +0.75 s, srv +1.33 s per run): partly located, not priced.**
  - **Leading candidate: finer I/O on c8's slow leg (20 Mbit),** the only leg in any cell whose batching gets finer under CAND: client tx datagrams per batch 6.80 → 5.18 (tx batches +34 %), server rx 4.46 → 3.77 (+21 %), off-owner `on_sent` share 0.343 → 0.526. Server tokio parks +6.7 % (parks follow server CPU's sign in all four cells); client sender wake cycles +3.1 % at flat cost per cycle (≈ 45 % of the client delta).
  - **Reading (consistent, not proven):** quinn's pacer burst is W·2 ms/srtt clamped to 10–256 MTU. BASE's over-read window (≈ 12–14 × BDP) kept bursts at ≈ 25; `bbr_rs`'s correct window (≈ 2.2 × BDP) drops the slow leg to the 10-datagram floor.
  - **Price:** the server's per-batch price fitted out of sample covers 25–58 % of its delta (unstable); the client's is not identified.
  - **Ruled out:** decoder window and reorder as the carrier (≈ 9 %), coded share, retransmits, GSO (wrong sign), wall time, run order.
  - **Unattributed by `RWM_RTOBS=1`:** ≈ 0.9 s server, ≈ 0.48 s client. The c8 `cpu_srv` A/A q95 is 4.7 % against the 5.3 % effect, so these fractions are soft.
- **Q2, c8 goodput: not UNDER-READ, not the window.**
  - **Main phase:** UNDER-READ ≈ 0 (qbw/link p10 1.01–1.02). The 0.72 points of deficit there match ProbeRtt's cost.
  - **Tail:** **70 % of c8's deficit (1.66 of 2.39 points) sits in the completion tail**, which lasts 3.91 s in BASE and 4.73 s in CAND (≤ 0.23 s elsewhere, c2 0.15 s). The sender is idle there (in-flight ≈ 0, send buffer ≈ 1 datagram), waiting on recovery. UNDER-READ and R2a lines are almost all tail lines and follow the tail, not cause it.
- **Consequences:**
  - **c8's completion tail (≈ 3.9 s even on main, ≈ 8 % of a c8 run) is a model item in its own right** (plan W3): find what bounds it (likely last-symbol recovery on the slow leg) before any change.
  - **No CPU engine change is justified yet.** Next: one instrumented c8 rerun (`RWM_RTOBS=2`, `RWM_CPUPROF=1`). Any later burst fix would be a continuous rate × RTT law in quinn's pacer (the W2 seam), never a threshold.

**c8 completion tail diagnosis (post hoc, not registered; session scratchpad `s48b/tail-c8/`, advisor-reviewed).** The guess above ("likely last-symbol recovery on the slow leg") is **refuted**.
- **Tail law:** tail ≈ H0/μ. H0 = unrecovered holes at source end: BASE 1 890, CAND 2 246 (frontier lag 73k / 83k source symbols, ≈ 2.5 % density, 76–77 % of tail retransmits originally on leg 0). μ ≈ 500/s in both arms. The arm gap is H0 (+356 holes ≈ +0.71 s).
- **What paces the drain:** a stop-and-wait quantum of 10 retransmits per processed report (`MAX_NACK_REPAIRS_PER_NACK` and the budget floor of 10, both literal), ack-clocked at ≈ 50 reports/s. Holes are recovered almost entirely by SACK-gap retransmits (`[FCAUSE]` gap_data ≈ 180k per arm; timer ≈ 90).
- **Why the backlog builds (the root):** the sender acts only on the first `MAX_NACK_GAPS` = 20 gaps above the frontier. In the main phase 97 % of visited sequences are cooldown-suppressed, so each slot is held for the pooled recovery clock C (closed-by-retransmit p50 ≈ 175 ms). Service ≈ K/C with K ≈ 63–103 per 4 s bin. The backlog grows whenever C exceeds ≈ 0.26 s, against ≈ 330 holes/s arriving.
  - At c8, leg 1 (40 ms RTT, 5 % loss) sets the pooled max clock on 95–100 % of lines in 0–16 s: BASE 221–370 ms, CAND 303–566 ms. Reports carry 63.3 ranges at c8, 4 at c2, 25–30 at c1d, 1.1 at c1s. First-report hole age p50 at c8: 3.1 s BASE / 5.2 s CAND (c2 197 ms).
  - The frozen-idle-leg clock hazard is real but minor here (≤ ≈ 8 % of μ).
  - Under `bbr_rs`, leg 1's app-echo clock is 1.5–1.7 × BASE's in 4–16 s, so H0 is larger. Open: whether that clock includes the dwell in quinn's send buffer (CAND leg 1 B ≈ 327 against 1.2 on BASE).
- **Next (plan W3.6b), two levers, each one formula:**
  - **Lever B (root), §52:** the sender's gap horizon becomes `MAX_SACK_RANGES` (64, set by the control datagram) instead of the literal 20. It changes behaviour wherever reports carry more than 20 ranges, i.e. **c8 and c1d** (not c1s or c2). Its predicted tail on main (≈ 0.2–0.4 s) rests on an unmeasured premise (K scales with the horizon), which §52 registers and tests. At CAND's clock the 64-range wire cap binds next (≈ 85 needed).
  - **Lever A (the tail quantum Q = max(10, Σ available)):** **parked.** Recovery sends bypass the in-flight charge and Copa's cwnd grows without bound while app-limited, so the term is not self-limiting (it would remove the ADR-0050 throttle by another door), and it adds a reader W3.4 retires. Revisit only if B leaves a tail.

**§48b ns bench (reported only; `nsbench48` `transport::cc_sim::tests::nsbench48_controller_ns_per_packet`, release, VM, `taskset -c 2-9`, n = 2 000 000, 5 alternating pairs; two paths, rtt 2.1 ms, 1 000 in flight per path, 2.5 % loss, acks in pairs).** ns per sent packet, median [min–max]: quinn BBR 136.05 [128.49–141.12], `bbr_rs` 553.50 [537.49–581.49], no-op driver 29.45 [28.49–33.95]. **Net of the driver: quinn 106.6, `bbr_rs` 524.1 ns, Δ = +417 ns per packet (× 4.9).** The `bbr_rs` sampler generated 1 071 401 samples with 1 001 records tracked.
- At c8's ≈ 10k datagrams/s that is ≈ 4 ms CPU per second, about a quarter of the c8 client delta. At c1s's rate it would be ≈ 29 ms/s, yet CPU/GB fell there, so other effects (fewer wakes once the collapse is gone) dominate it. **A contributor, not the whole c8 cost; and a cost worth removing on its own** (plan W1 round 2: the per-packet send-record bookkeeping).

**§48b evidence, now in the repo** (it was cited above from the session scratchpad): `docs/l1-raw/bbr48b/diag-ld-full.tgz` (every row's client/server log and driver output, 12.9 MB compressed; contains no VM address, user or home path), the c8 CPU diagnosis `diag-c8/` and the completion-tail diagnosis `tail-c8/` (scripts and their outputs; run with `python -I <script> <dir with run/ and full/diag-ld/ unpacked>`). §52's premise rests on `tail-c8/`.

**§48b ns bench at a realistic in-flight count** (`NSBENCH48_INFLIGHT`, `nsbench48` `a0`-tip; same VM, same settings, back to back): at 200 in flight per path (c8's fast leg holds ≈ 230 in vivo) net quinn 128.0, `bbr_rs` 466.7 ns, **Δ = +339 ns (× 3.6)**; at 1 000, +385 ns (× 4.5; the first run read +417). The cost hardly scales with tracked records, so it is a fixed per-packet cost in `bbr_rs`'s path (sampler and bookkeeping), not the record map's size. The fix target moves from "map → ring" to profiling the per-packet path; the gate (≤ 2 × quinn) stands. The "≈ a quarter of the c8 client delta" estimate holds at ≈ +3.4 ms of CPU per second at c8's rate.

**Placement sibling arms (read-only diagnosis, advisor-reviewed; 2026-11-12).**
- **Correction:** the earlier claim that both siblings share `RWM_PLACE_HOL`'s measured verdict was wrong. `RWM_PLACE_WDIV_DERIVED` was **never in a battery** (Track A ran it `=0` on all 166 rows; 5e6e3f5). The goodput regressions belong to the temperature arms (TSIG/HOLTSIG); HOL alone read 160.7 against CTL 153.1 at c1. The Track-A ledger lives at `faa54afd^:raptorpath/docs/l1-raw/place/` (deleted from main by `faa54afd`).
- **`RWM_PLACE_T_DERIVED`:** not refuted, expensive as built. σ̂_e is re-sorted per leg per placement; c1 is single-leg (P = 1), so its 0.78–0.85 × goodput and +26 % CPU/GB there are the estimator's cost alone (all 16 arm rows below all 16 CTL+HOL rows). The formula has no floor and reaches the argmin as σ̂_e → 0 (argmin cost 0.35× at c7). The ETA error is a persistent per-leg bias with per-leg scale, against the law's i.i.d. Gumbel premise. **User ruling: archive the code, keep the formula open** (tag, §10 wording with a re-ask clause; on `simplify-5`, rides §50).
- **`RWM_PLACE_WDIV_DERIVED`: kept** (no verdict to archive on). SACK-gap retransmits carry V = w_div·fate = 1.0 on the original leg (`sender_phases.rs:981`), 786 : 1 against it at T = 0.15; on main's binary at c8 ≈ 82 % of fast-leg holes are re-sent on the slow leg. **Whether that drives the c8 backlog is not established** (the tail law has no leg-choice term); a post-hoc check of time-to-close by send-leg is running. Either way §52 registers the retransmit leg share as a readout.
- **The constants:** T = 0.15 and w_div = 1.0 are in units of ref (the fastest leg's echo srtt, including send-buffer dwell), so in seconds they are load-dependent. Arm-off σ̂_e/ref read 0.22/0.18/0.45 (old predictor) and 0.06–0.13 on the current echo ref (0.13–0.94 on a wire ref). The constant is not pinned either way until ref's clock is settled. A sensitivity bounding test (T over [0.03, 0.68], ref ×0.5–×2) rides `simplify-5`.

**`bbrrs-r2` (one-round ProbeRtt, `da3d0320`; derivation `w1r2`):** red 11 failures (`docs/l1-raw/bbrrs-r2/red.txt`), green `transport::` 55/0/7. In cc_sim at c2: ProbeRtt ≈ 44–49 ms per episode, 0.44–0.49 % of time (shipped 2.1 %); b̂/L inside ProbeRtt ≥ 1.03 (shipped 0.588); m̂ at exit 8.0–12.35 ms (shortfall u −0.01…+0.52, above the 7.8–9.2 ms prediction). **The derivation's ratchet claim failed in sim:** a mutated gain of 1.25 plateaus (≈ 12.6 ms) instead of growing ×1.3 per period, so T6 is a no-growth gate (mean of last 3 exits ≤ first 3 + 1 ms) and the mutation check is `#[ignore]`d with its measurement. The round count is a diagnostic arm `RWM_BBR_PROBERTT_ROUNDS` (default 1).

**Retransmit leg choice at c8 (post hoc, not registered; `docs/l1-raw/bbr48b/rtxleg/`, advisor-reviewed).** **Leg choice matters, through hold time, not loss.**
- **Fit:** per main-phase bin, horizon hold time T_h ≈ a·(s1·c1 + (1−s1)·c0), with s1 the share of fires sent on the slow leg (R² 0.68 BASE / 0.93 CAND, against 0.53 / 0.90 for the pooled clock). The within-row slope on s1·(c1−c0) is 0.86 ± 0.07 / 1.06 ± 0.03, positive in 12/12 rows: a slow-leg retransmit holds its slot for the slow leg's clock. Implied hold: leg 0 ≈ 185 / 116 ms, leg 1 ≈ 262 / 314 ms.
- **Measured** (`[HOLD]` closed_retx by original leg, row medians): p90 377 vs 246 ms (BASE, 11/12 rows), 426 vs 147 ms (CAND, 12/12). c1d reads null.
- **Not the lever:** re-fire from lost retransmits (≈ 2 % of fires; one fire per real loss at c8).
- **The logs carry no per-hole send-leg record** (send and original leg are nearly aliased: 80–88 % of leg-0 holes go to leg 1); items 1, 3 and 4 are derived or proxy.
- **Counterfactual (model, a knife edge since λ ≈ μ; relative to the measured mix):** retransmits kept on their original leg → BASE H0 1 890 → 770–940, tail 3.91 → 1.5–1.9 s; CAND H0 2 246 → ≈ 90–135, tail 4.73 → ≈ 0.3 s. This supersedes `price.py`'s variant-O line (H0 = 0). The interaction with lever B is not modelled (B raises the horizon's occupancy N_h; leg choice lowers the hold T_h).
- **The leg choice is set by the placement diversity term** (w_div = 1.0 on the original leg, 786 : 1 against it at T = 0.15), so `RWM_PLACE_WDIV_DERIVED` (never measured) and w_div are levers on the c8 backlog, beside lever B.
- **Next:** §52 registers the retransmit send-leg share as a readout, plus reported-only arms on the retransmit diversity weight (the derived form, and a fixed lower weight via a diagnostic env) at c8 600 MB, against the same BASE, before lever B merges (after B, main's c8 backlog may be gone). An instrument: a send-leg field on `[HOLD]`'s closed_retx histogram, or `RWM_SUCC_DUMP=1` for those rows.
