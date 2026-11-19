# M1 plan: four RTT-dump rows and their gauge twins (tools-only, no build)

Status: PLAN, for the coordinator to register (as the next free status section; written here as `66`). Docs only.
No VM contact. It serves `docs/derivations/sampler-design.md` §4.1 (the registration sketch, whose rows, reading
rules and predictions this plan takes verbatim) and its owner question O1, and through it `store-law-state.md` §5
item B. It runs first once the owner clears the Ceph cluster.

**Answer.**
- Eight rows (four `D` dump rows, each with its `G` twin) plus one smoke row, Bulk, static pools, one replicate.
- The binary is prebuilt `e410d640` (the §60–§62 gauge binary). **Main's binary (`8104fa02`) cannot run it.**
  Main has the dump switch `RWM_RTT_DUMP` (`net/rttdump.rs`) but no `RWM_SLAW` gate and no `[SLAW]` window line.
  The P60/P61/P62 gate family preflights both and aborts (`loopdiag_battery.sh` :257–261, `ABORT-SHA ... not the
  gauge build`).
- **The existing harness cannot run it without plan code.**
  - `plan62.py` has the dump arm but one cell (`CELLS = (C3J0,)`) and a fixed rehearsal table.
  - `plan61.py` has c3-25 and c2-100 but no dump arm.
  - The battery writes `UNKNOWN-CELL` for any other cell under P62.
  - §B gives the exact spec for a Sonnet worker: one new plan module, and P66 branches beside P62's in the battery
    and run_all.
- **Budget:** about 1 h typical (tools tests ≈ 15 min, rows ≈ 15–20 min, staging and scoring), 1 h worst on the
  rows' guard.

## A. The plan (registrable text)

**Why.** sampler-design §2.2 proposes the window *median* as the law's location, with S = the floor IQR, conditional
on two facts no committed row shows:
- the median of raw echo samples is count-free under jitter;
- g at the Realtime knee is shape-free.

M1 reads both from raw samples, with no build.

**Rows** (`LD_PLAN=66`; every row Bulk, `--window-reliable`, one live path, pool = W):

| slot | cell | arm | seed / rep | bytes | timeout | reads |
|---|---|---|---|---|---|---|
| smoke | c2-100 | G16 | 7 / 9 | 100 MB | 150 s | liveness only, not scored |
| 1 / 2 | c3-25 (J = 5) | G12 / D12 | 42 / 1 | 15 MB (plan61 `formula_mb(12)`) | 120 s | the median and IQR under jitter against the count; IQR_F at c3 |
| 3 / 4 | c3-25 | G108 / D108 | 42 / 1 | 120 MB (plan61 `BYTES_KNEE_MB`) | 120 s | the knee: the send gap before each window-minimum sample; the knee-count median |
| 5 / 6 | c2-100 | G16 / D16 | 42 / 1 | 100 MB (plan60 `BYTES_FLOOR[(c2, 16)]`) | 150 s | IQR_F at c2 |
| 7 / 8 | c3j0-25 (J = 0) | G112 / D112 | 42 / 1 | 120 MB (plan62 `BYTES_KNEE_MB`) | 120 s | the J = 0 knee (reported only) |

- **Twins.** Each `D` row has its `G` twin adjacent in the same session, so the dump's perturbation is read as s62's
  D12 against its floor pair.
- **Order:** the table order above. The rows run one at a time and the battery re-applies netem per row.

**Environment** (per row, from `plan66.py armspec`):
- `RWM_SLAW=1` first, then `RWM_RTOBS=1` and `RWM_STORE_PATH_POOL=<W>`;
- `D` rows add `RWM_RTT_DUMP=1`, with `RWM_RTT_DUMP_MAX` left at its shipped 400 000;
- launch-wide: `RWM_DIAG=1`, `RWM_RDIAG=1`, `RWM_PERF_TIMEOUT_S=<timeout>`, `RWM_NETEM_WITNESS=1` (as P62's
  `INSTR_ENV`);
- the plan-62 scrub list, plus `-u RWM_RTT_DUMP -u RWM_SLAW -u RWM_STORE_PATH_POOL`.

**Binary:**
- `LD_BIN_PREBUILT` = the staged gauge binary, checked against `LD_BIN_SHA_EXPECT` =
  `e410d64053ab0a143e813aded2e4023176b41edf09d287e795252736b0e5f25d` (the run_all default for P60/P62).
- No build. Bulk is untouched by F2 (F2 hits Realtime receivers), and every frozen input below was measured on this
  binary.
- If the staged copy is gone with the Ceph trouble, rebuild `s60-tree` `cb88df3a` on the VM by the decision-9
  precedent, and require the same sha.

**Witnesses (a row is VOID on any, replaced once):**
1. `bin_sha` on every row equals `e410d640…`.
2. `[GATES]` on both ends: `RWM_SLAW=1`, `RWM_DIAG=1`, `RWM_THREE_TERM` off, the pool echo = W, and `RWM_RTT_DUMP=1`
   on `D` / `=0` on `G`.
3. **Live netem per cell**, before and after each row (`[NETEM] tag=pre|post`):
   - c2-100 reads `delay 5ms 3ms`;
   - c3-25 reads `delay 20ms 5ms`;
   - c3j0-25 reads `delay 20ms` with no jitter term.
   - A wrong read is ABORT-NETEM, as P62.
4. **Dump integrity on `D` rows:**
   - `[RTTDUMP]` lines are present, and there is no `[RTTDUMP-CAP]`;
   - coverage (dumped ÷ the last `[DIAG]` sig `n`) ≥ 0.95 (s62_score `DUMP_COVERAGE_MIN`);
   - the count test against the `[SLAW]` path-0 `nk` sum is in [0.95, 1.05] (s62 rule 9).
   - `G` rows carry no `[RTTDUMP]`.
   - Expected sizes are well under the cap: c3 W 108 ≈ 1 920 samples/s × ≈ 55 s ≈ 100 k; c2 W 16 ≈ 1 130/s × ≈ 75 s
     ≈ 85 k.
5. LIVE (no DNF; a DNF is re-run once), with the store bind fraction reported.

**Launch preconditions (run_all's, as P62):**
- the owner's Ceph clearance;
- host cargo paused under the coordinator's host lock;
- `procs_running` and io pressure at the existing precondition;
- `/dev/shm` ≥ 1 GB free;
- `LD_HARD` = launch + 1.0 h.

**Rehearsal:** `LD_REHEARSE=1` runs the smoke row plus slot 2 (c3 D12) and stops. It is GO iff both rows pass every
witness.

**Readouts, registered verbatim from sampler-design §4.1** (computed post hoc from the committed client logs by
`store-law/crdump.py`'s §4 construction, extended to three cells; the harness's scorer prints integrity only):
- **Count-freeness, per `D` row:** the pooled raw-sample minimum, median and IQR over native, 4 and 36 windows.
  Prediction for D12: median drift ≤ 0.3 ms, minimum drift ≥ 7 ms, IQR within 10 %. D112 is reported only.
- **IQR_F,c:** the median over LATE native windows of p75 − p25 of the raw samples, nearest rank. Predicted 2.77–3.49 ms
  (c2) and 4.22–5.33 ms (c3).
- **g_R,c = (need₅₀,c − 0.05·F̂₅₀,c) / IQR_F,c**, with need₅₀ / F̂₅₀ frozen: c2 3.61 / 13.70, c3 6.92 / 48.73 (61c),
  and 61a's 7.17 / 48.47 reported beside it. Registered prediction: g_R ∈ [0.84, 1.06] at both cells.
- **Outcome meanings,** as sampler-design §4.1:
  - inside at both cells: one g_R ≈ 1 places Realtime at the knee on both leg shapes, under (b) with S = IQR;
  - outside: the IQR band or need/IQR's shape-freeness failed, and IQR_F is reported so the two can be told apart.
- **Twin perturbation (reported):** D − G for goodput and the median `[SLAW]` window T̂min and p50, per pair.
- **Also reported, for `store-law-state.md` §7:** the release (ack-wake) interval per row (d `wake[ack]`/dt).

**Budget:**
- **Rows:** 9 × (≤ 150 s + ≈ 15 s bring-up) + one replacement ≈ 1 650 s worst. The guard is `BUDGET_S = 2 400` (as
  62r0), strictly under the `LD_HARD` offset.
- **Typical rows:** ≈ 75 s each, so ≈ 15 min.
- **With tools tests (≈ 15 min) and staging:** ≈ 40–60 min. The rehearsal is ≈ 20 min including tests.

## B. Worker spec (Sonnet, tools only; branch `harness66` off main; no `raptorpath/src` change)

Pattern: `tools/l1/plan62.py` and its P62 branches. Every change sits beside P62's, never inside it, so plan 62's
tests stay byte-equal.

1. **`tools/l1/plan66.py`**, copied from `plan62.py`:
   - `CELLS = ("c3-25", "c2-100", "c3j0-25")`, `NLIVE = 1` each, `LATE_T = 15.0` each. Register them in
     `plan60.LATE_T` / `plan61.LATE_T` with `setdefault`, as plan62 :38–39.
   - `ARMS = {c3-25: (12, 108), c2-100: (16,), c3j0-25: (112,)}`, kinds `G<W>` / `D<W>`; `arm_parse` as plan62.
   - `arm_bytes`: c3-25 W 12 → `plan61.formula_mb(12)`·10⁶ (= 15 MB); c3-25 W 108 → `plan61.BYTES_KNEE_MB`·10⁶;
     c2-100 W 16 → `plan60.BYTES_FLOOR[("c2-100", 16)]`; c3j0-25 W 112 → `plan62.BYTES_KNEE_MB`·10⁶.
   - `arm_timeout`: 150 s at c2-100, 120 s otherwise. `arm_hint` → `bulk`.
   - `arm_env`: plan62's `arm_env` unchanged (`RWM_SLAW=1`, `RWM_RTOBS=1`, the pool unless 2048, `RWM_RTT_DUMP=1` on D).
   - `plan("66")`: the 8 slots in §A's order, `cell def arm seed rep`. `plan("66", smoke=True)` → the smoke row.
     `plan("66r0")` → smoke plus slot 2. `count` / `check` as plan62.
   - `row_problems`: plan62's witnesses, with the cell taken from the row.
   - `replace_reason`: plan62's (echo-void, or a DNF → once).
   - `netem_check(path, cell)`: plan62's, but the expected qdisc comes from the cell. Expected (delay ms, jitter ms):
     c2-100 (5, 3), c3-25 (20, 5), c3j0-25 (20, 0), from `lib.sh scenario_params`. Extend `l1common.netem_witness`
     with an `expect=(delay, jitter)` keyword defaulting to plan 62's (20, 0), so plan 62 is unchanged.
   - `LD_HARD_S = {"66r0": 3600, "66": 3600}`, `BUDGET_S = {"66r0": 1200, "66": 2400}`.
   - CLI: `plan|count|armspec|replace|check|netem`, as plan62.
2. **`loopdiag_battery.sh`:**
   - `P66=0; case "$PLAN" in 66r0|66) P66=1 ;; esac`, added to the plan guard at :98.
   - `INSTR_ENV` as P62's (:210), with `KEEP_PAT` adding `\[NETEM\]`.
   - The preflight at :257–261 takes P66 too, with `RWM_RTT_DUMP` required.
   - The armspec dispatch as :382–388 from `plan66.py`; the per-row netem check as :586–588 calling
     `plan66.py netem "$TMPD/ld-drv.out" "$cell"`.
   - A `plan66()` driver as `plan62()` (:1309–1360), with no pilot and no re-probe.
   - Add plan66.py, s66_score.py and test_harness66.py to the file list at :84.
3. **`loopdiag_run_all.sh`:**
   - Accept `66r0|66` at :164/:168, with `REHEARSE=1` for 66r0.
   - `P66` sets `P60=1 P46=1 P57=1`, as P62 (:224), with `LD_BIN_PREBUILT` required (:553) and `P57_NEED` from
     `plan66.BUDGET_S`.
   - No `LD_CTL`/`LD_KNEE` checks.
   - Tools tests `test_harness66.py` (as :492); the crlf_guard list at :125; smoke and score through `s66_score.py`.
4. **`tools/l1/s66_score.py`** (integrity only, stdlib):
   - `smoke66 <plan> <smoke.log>` prints `LABEL66 GO` iff every row is LIVE with witnesses 1–5.
   - `score66 <plan> <ld.log>` prints, per slot: LIVE/VOID, the witnesses, dump n / coverage / cap / count-test
     ratio, LATE goodput, and the release interval.
   - It imports s62_score's `_DUMP` parser and its coverage function rather than copying them.
   - The scientific readouts stay out of the harness: the post-hoc `store-law/m1read.py` is written after the rows,
     on crdump.py's construction.
5. **Tests:**
   - `tools/l1/test_harness66.py`: armspec for every slot (bytes, timeout, hint, env tokens and order); netem
     expectations for all three cells, plus plan 62's default unchanged; replace and check on synthetic ledgers; the
     table counts (8, 1 smoke, 2 rehearsal).
   - `test_stubrun.py`: plan-66 paths in the fast mode, all paths covered.
   - Plan 62's tests pass unchanged.
6. **Done when:**
   - `python -I tools/l1/test_harness66.py` passes, as do the full fast unfiltered `test_stubrun.py` on an LF export,
     plan 62's tests unchanged, and the bash tests that run on the host.
   - Report the branch sha and the tools-manifest sha.
   - No VM contact and no engine file touched.
