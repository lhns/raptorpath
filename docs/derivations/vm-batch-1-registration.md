# VM batch 1: the c8 gauges, the tunnel in-order guard and the S06 deletion: pre-registration draft

Draft for the coordinator to register as its own status section. No row has run. Base text: github main `7123c754`. Order source: `docs/derivations/liveness-audit.md` §3 (rank 1 = S06, rank 2 = the c8 gap horizon, rank 3 = the tunnel guard). Design inputs: `docs/derivations/s06-delete-plan.md` §6, `docs/derivations/f2-guard-registration.md` (the house format), status §41d, §58, §64, the 2026-11-19 VM queue and the 2026-11-20 S06 review. Process rules: CLAUDE.md "No engine merge on a Bulk-only guard", the binary-identity rule, ADR-0080's flake rule and `docs/measurement-discipline.md`.

**For the owner, in plain words.**
- **Order against the audit.** The audit ranks S06 first, lever B second and the tunnel third. Lever B is out (below). The merge order is the tunnel and the gauges first, then S06. Those two cannot change behaviour on the data path. Putting them first means a guard-2 FAIL, or an R2 hold, cannot hold them back. The guard still runs S06 in the same batch.
- **Nothing in this batch can be pinned today.** All three candidates are 77 commits behind main. `s06-delete` conflicts with main (in `emit_source.rs`, `sender_phases.rs` and the paper) and with `c8-gauges` (in `emit_source.rs`). A worker builds merged-up seat branches first, and an amendment pins them (§1). That is host work, with no VM.
- **The batch runs two guards, so the two parts that are ready can merge without waiting on S06:**
  - **guard 1:** the c8 gauges plus the tunnel guard, against main;
  - **guard 2:** the S06 deletion on top of guard 1's candidate.
  
  Each guard has a Bulk stage and stages off Bulk, with witnesses that show each change actually ran.
- **S06 merges on its own guard-2 PASS (owner, 2026-11-20; Amendment 1 below).** It does not wait beneath the ADR-0084 engine change: a guard pass goes stale as main moves, and holding it would mean guarding it again. ADR-0084 Amendment 1's "not on its own" clause (item 3, and its Consequences line "lifted only for the stacked build") must be rewritten to match before S06 merges. The coordinator is doing that on `adr84-amend`.
- **The c8 gap horizon (lever B, audit rank 2) is not in this batch.** You parked it: decision 6, re-affirmed on 2026-11-19 ("the fix is the store law"). Its branch is also 628 commits behind. §8 lists what it would need.
- **Ceph.** Status still says no scored VM session runs until you say the Ceph instability is over. This batch waits for that word (§6, precondition P0).
- **Cost:** four VM sessions: pre-flight, rehearsal, the two Bulk guards, the off-Bulk stages. Typical ≈ 4.3 + 1.5 + 2.0 + 3.1 ≈ 10.9 h, worst ≈ 15.5 h. Each session stays under the five-hour cap.

## 1. Seats and pins

### 1.1 Sources (at the time of writing)

| item | branch @ sha | state against main `7123c754` |
|---|---|---|
| main (BASE) | `github/main` `7123c754` | — |
| c8 gauges | `c8-gauges` `697eeb73`, plus `c8-gauges-pfec` (one commit: `[SPAN] pfec=`) | 77 behind, 12 ahead; trial merge with main clean |
| tunnel guard | `tunnel-inorder-guard` `df5ffa08` | 77 behind, 3 ahead; trial merge with main clean; diff in `config.rs` and `main.rs` (`cmd_run`, `cmd_check`) only |
| S06 deletion | `s06-delete` `a930a72b` (holds `s06-delete-paper-present` `f29a0db2` and `c8-gauges` up to `a68ba449`) | 77 behind, 26 ahead; **conflicts with main** in `docs/fec-arq-model.md`, `net/emit_source.rs`, `net/sender_phases.rs`; **conflicts with `c8-gauges`** in `net/emit_source.rs` |
| the pacer (Stage R2 only) | `4ee1df9b` (`RWM_PERF_RATE_MBIT`) | neither its engine code nor its `lib.sh` forward line is in main's tree (forward line removed by `e304921b`) |
| the round trip (tunnel red/green) | `static-audits` `05a94a06`; commits `4bbb2f96` (`resolve_tunnel` extraction), `41e37116` + `235994ac` (the round-trip test) | 0 behind main; it already contains the tunnel commits `4d5c53d2` and `987b0360` |

### 1.2 The seats (cumulative, so each merge state is a guarded tree)

The binary embeds location data (§58 Part B), so merged main equals a guarded binary only if the merge reproduces the guarded source tree exactly. The seats are therefore stacked in merge order:

| seat | tree | role |
|---|---|---|
| **B0** | `vmb-base` = main at the pin + `harness67` (tools only) | BASE of guard 1; reference arm in Stage A |
| **S1** | `vmb-s1` = B0 + `git merge --no-ff c8-gauges` + `git merge --no-ff tunnel-inorder-guard` | CAND of guard 1; BASE of guard 2. Merges to main if guard 1 passes |
| **S2** | `vmb-s2` = S1 + `git merge --no-ff s06-delete`, merged up (below) | CAND of guard 2. Merges to main on its own guard-2 PASS (owner, 2026-11-20; §4.5) |
| **P1** | S1 + `4ee1df9b` (cherry-pick) + the `RWM_PERF_RATE_MBIT` line in `tools/l1/lib.sh`'s `RWM_FORWARD` | Stage R2 only, never merges |
| **P2** | S2 + the same two | Stage R2 only, never merges |
| **P1T**, **P2T** | P1 and P2 built `--release --features test-seams` | R2's `[ACCT]` twins (Amendment 1), never merge |

Main's `lib.sh` no longer forwards `RWM_PERF_RATE_MBIT`: `e304921b` removed the line, because the engine code that reads it is missing. So the pacer seats carry the line, and it stays out of `harness67`; otherwise `gates::forwarding_audit` goes red on B0, S1 and S2. The tools-diff witness of §1.3 exempts that one line on P1 and P2.

**Why the tunnel sits with the gauges, not on top of S06.** Neither change is on the perf data path. The tunnel diff runs only in `raptorpath run` and `raptorpath check`, and the gauges are print sites. With the tunnel stacked above S06, a guard-2 FAIL or an R2 hold would hold the tunnel too. Guard 1's FAIL bisection is cheap (§5.2).

**The S06 merge-up (worker job, host, before the pin).** Resolve the three conflicts. The merge-up facts are those of ADR-0084 Amendment 1, item 3 (branch `adr84-amend` `df49d0c0`):
- the slot-site counters main added (`shed_visit_slot`, `on_shed_slot`, `on_denied_slot`) lose their site;
- `sim_shed_reach::witness_slot_site_sheds_paced` is red on that tree by design, and is inverted;
- the stale `shed-slot` and `shed-slot-denied` entries leave `tools/ci/coverage_sites.json`.

The `c8-gauges` side is taken as on `697eeb73`: `ptgt=` and `pmax=` were reverted there, and `shed_slot=` keeps printing at 0 (plan §1.2).

### 1.3 Identity witnesses, recorded in the pin amendment

- S1 carries exactly the two branches. The engine patch-id (`git patch-id --stable`) of `git diff B0 S1 -- raptorpath raptorpath-math Cargo.toml Cargo.lock` equals that of the two branch diffs `github/main...c8-gauges` plus `github/main...tunnel-inorder-guard` over the same paths.
- The S2 engine patch-id is recorded, and every conflict hunk is listed with its resolution.
- `git diff B0 S1 -- tools` and `git diff S1 S2 -- tools` are empty, so all seats run one harness.
- CI is green on `vmb-s1` and `vmb-s2`, including `binid.yml`.
- The release shas `LD_BIN_SHA_{B0,S1,S2,P1,P2}` are recorded at the rehearsal and pinned by a second amendment (the §64 Amendment 2 pattern).

**If main moves** before the launch: docs, roadmap and ledger commits change nothing here. An engine commit means the seats are rebuilt and re-pinned.

## 2. Pre-flight

### 2.1 Before VM contact (host or coordinator); each item blocks the pin

1. **Seats** built as in §1.2 and pinned by amendment.
2. **The flake rule in the suites runner.**
   - This batch's suites run in `run_suites` in `tools/l1/loopdiag_run_all.sh`, because plan 67 is a plan-41d derivative.
   - Its rule (`:736–751`) still treats "passes on the immediate solo re-run" as ok.
   - **Required, on `harness67`:** ADR-0080's rule, as `suites_run.sh` has it. A failure fails the suite unless the test is on `raptorpath/tests/quarantine.txt`; the solo re-run only classifies it (FLAKE or REAL). `test_harness47.py`, which asserts on the loopdiag string, is updated. A stub path `flake-not-quarantined` must read ABORT-TESTS.
   - `threadp1_run_all.sh`, `threadpd_run_all.sh` and `threadq1_run_all.sh` carry the same old rule. This batch does not use them, so they are listed for a separate fix.
3. **The S06 timing tests.**
   - **T3:** all of `s06_reactive.rs`'s T3 variants are `#[ignore]` with a "records, does not gate" reason, so they record only (checked at `a930a72b`: `arrivals_continue`, `stall_budget_open`, `stall_budget_closed`, `mid_object_stall_evicting`, `mid_object_no_blackout_evicting`). Nothing more is needed for them.
   - **T2 (`stalled_blocker_recovered_by_nack`)** still gates, and it reads 9/10 on the candidate (status, 2026-11-20). **Hard precondition (Amendment 1): T2 is ported to the deterministic sim (`tests/sim_support`), with its decode-fill witness kept.**
     - It must be shown red on S1's engine (copies of k sent through a proactive slot) and green on S2.
     - It keeps T2's `pslot > 0` assertion. That needs the paced sim client, because an unpaced sim client emits the whole object at one instant and takes no slot.
     - The two-run digest equality must hold.
     - Quarantine is not an alternative. Without the port, guard 2 does not pin.
4. **The tunnel's red-first round trip** (`config_round_trip.rs`, `static-audits`). It exists only on `static-audits`, which already contains the guard. The trees:
   - **red tree:** main at the pin + cherry-picks of `4bbb2f96`, `41e37116` and `235994ac` only, so `resolve_tunnel` resolves `merge(base, cli)` as main's `cmd_run` does. The four `ForcedOff` cells (`window_out_of_order`, `window_coded_only`, `window_generation_coding`, `window_systematic_repair`) must fail with "the entry point's forced value moved: the file leaks through". Any other failure, or a pass, means the tree is wrong.
   - **green tree:** `static-audits` `05a94a06` (it contains `4d5c53d2` and `987b0360`). Every cell passes.
   - The command is `cargo test -p raptorpath --lib config_round_trip`, on each tree's own target dir, under the host mutex. The red output, the green output and both shas go into the pin amendment.
5. **The `harness67` worker** (tools only, §9), with its stub runs green.
6. **Fable's review of this registration** (§10).

### 2.2 On the VM, in session 0 (pre-flight, no battery; fresh target dir per tree)

| step | trees | rule |
|---|---|---|
| `cargo check --all-targets` | S1, S2 | any error is ABORT-PREFLIGHT for that seat's guard |
| `cargo test -p raptorpath --lib gates` | B0 only (S1 and S2 run it inside their suites) | a failure on S1 or S2 that is not equal on B0 is ABORT-PREFLIGHT; a failure equal on B0 is a finding on main, reported, and the guard runs |
| full suites (`run_suites`, fixed rule): release, doc, wasm (`env -u GOLDEN_CAPTURE`), debug, core, seams | **S1 and S2, each.** S1 may merge alone, and S2's run does not cover the tests S06 deletes or inverts (`sim_shed_reach`, `holeclass_reachability` clause 6, the `p_lost` pins) | REAL or non-quarantined FLAKE is ABORT-TESTS for that seat: none of its rows run |
| touched integration tests, once | S2: `holeclass_reachability`, `recovery_bench`, `sim_nack_test`, `place_arms_reachability`, `s06_no_copy`, `s06_reactive`, `sim_shed_reach`, `sim_lost_tail`; S1: `--lib config::` | inside the suites run; listed so a seat-specific failure is named |
| **repeat run of the gating loopback tests** | S2: every non-ignored test in `s06_no_copy` and `s06_reactive` (T2 runs as a sim test, §2.1 item 3); `holeclass_reachability`. S1: `--lib config::` | **N = 30, zero failures** (a 10 % flake survives 30 runs with p ≈ 0.04). Sim tests (`sim_*`) are deterministic and run once plus a two-run digest equality. If 30 runs of a test do not fit the repeat budget (60 min total, timed per run in the first pass), the cut is written in the second amendment before the guard launch, with the reached N. Any failure: that seat's guard does not launch |
| `build_visualizer.sh` (fresh wasm) and the native golden test without capture | S2 | as plan §6 item 1 |
| `binid_run.sh` | B0, S1, S2 | records the crate-directory sha of each seat, the reference for §5.3 |

The tools tests (`tools/l1`, python and bash; ABORT-TESTS) run in session 1, the rehearsal (§6.1).

**`[ACCT]` (the coordinator's question).** No execution witness in this batch needs the sender's `[ACCT]` account line. It comes from `net/account.rs`, compiled only under `debug_assertions` or `test-seams` (`net/mod.rs:48`), so it is absent from the release binary. The witnesses use:
- `[DIAG] mpr[… plost= taper= pslot= …]` (`pslot=` added by `s06-delete`'s red-first commit; the others exist on main);
- `cod=`, the `[SPAN]` rr and the `[DIAG]` shed gauge with `c8-gauges`' `shed_slot=` / `shed_nack=`;
- the receiver's below-floor count, as Addendum 2's `acct.py` reads it;
- the gauge prints `scw=`, `rsat=`, `rlaw=`, `plx=`;
- the perf summary and `[TRUTH]`.

The account's invariants are covered by the debug suite (`recovery_bench` and the account tests) in §2.2, not by rows. So no `--features test-seams` release row is registered, and the shipped binary stays the only one the rows run.

The one readout that needs it is R2's unresolved-blocker count. It comes from `--features test-seams` twins of the pacer seats (P1T, P2T; Amendment 1, §4.4). They never merge, so no shipped binary changes.

## 3. Guard 1: B0 against S1 (c8 gauges + tunnel guard)

### 3.1 Stage G1 (scored, Bulk): plan 41d unchanged

- 96 rows: c1s-400, c1d-400 at the guard size; c2-100 and c8-100 at 600 MB, `RWM_PERF_TIMEOUT_S=200`; 12 + 12 per cell, seeds 42 and 7, `RWM_DIAG=1`, plan 41c's interleave.
- Clauses v2 (§41d): goodput, `cpu_cli/GB`, `cpu_srv/GB`, `lag_E_cli`, `lag_E_srv` per cell, plus the in-order delay mean at c2 and c8. Scored with `score41d`: rank-minp, one-sided, 10 000 relabellings, seed 4141. **G1 PASS iff p > 0.05.**
- Voids as §41d: VOID-SHORT re-run, DNF keeps its worst rank, at most 4 replaced slots, more is NO VERDICT.
- **Row witnesses (outside the min-p family):** every S1 row's client log carries `scw=` and `plx=` on `[DIAG]` and `rlaw=` on `[SPAN]`; no B0 row carries them. Seat check before any row: the S1 binary holds the strings `rlaw=` and `is a perf/native-object mode`, and B0's holds neither (ABORT-SEAT).
- **Prediction: PASS.** The gauges are print sites under `RWM_DIAG`. The tunnel code is not executed by `perf`. The registered risk is the 5 % false-fail rate.
- Expected resolution (§64 Result, the latest v2 ledger): goodput c1s 5 %, c1d 8 %, c2 1 %, c8 2 %; CPU 2.4–8 %; in-order c2 8 %, c8 20 %.

### 3.2 Stage A1 (scored, Auto, off Bulk): the gauges at the default hint

Shares the rows of Stage A (§4.2): B0 6 rows and S1 12 rows per cell, at c2-100 and c8-100 Auto, 600 MB.
- **Witness per row:** S1 rows carry the four gauge prints, and `rr` > 0 on the `[SPAN]` median (so the repair path ran); B0 rows carry none of the prints. A failing row is VOID-WITNESS and re-run once.
- **Clause A1 (scored):** S1 against B0, one-sided (S1 worse), on goodput, `cpu_cli/GB` and `cpu_srv/GB` at c2 and c8. Rank-minp over the six, exact relabelling null at 6 v 12. **A1 PASS iff p > 0.05.**
- **Resolution, stated:** §51's in-session A/A at 6 v 6 (c2 goodput 1 %, CPU 8 %; c8 goodput 3 %, CPU 12 %). A PASS bounds a gauge cost only above those sizes.
- **Prediction: PASS.**

### 3.3 Stage T (scored, the tunnel itself, Auto and Realtime)

Perf rows never run `cmd_run` or `cmd_check`, so a perf-only guard is blind to the tunnel change at every hint. Stage T runs the binary as a tunnel, with `tools/l1/tun_bulk.sh` (kernel TCP inside the tunnel, `raptorpath run` on both ends, no `--window-reliable`, TUNPIPE / TUNMTU / TUNSHA witnesses).
- **Transfer rows:** cell c2. Hints `auto` and `realtime`. Seats B0 and S1. One tunnel bring-up per (seat, hint), with REPS = 5 transfers of 100 MB and seed 42. That gives 4 invocations and 20 transfers.
- **Refusal rows** (each a single command, no TUN brought up beyond what `run` needs): for each of the four keys, a TOML file setting only that key to `true` (otherwise the transfer rows' file).
  - S1: `raptorpath --config <f> check` and `raptorpath --config <f> run` both exit non-zero, and stderr holds exactly `<key> is a perf/native-object mode; the tunnel always delivers in order: remove it from the config file`.
  - B0: `run` starts, and its `resolved configuration` line (`info!(?peer_config)`; `PeerConfig` carries `window_out_of_order` and the other three) shows that key `true`. This is the defect, shown on the shipped binary. The harness keeps both endpoint logs at info level, and the smoke checks the line is there.
  - That is 16 commands.
- **Clause T (scored). T PASS iff:**
  1. all 8 S1 refusal commands refuse with the exact text;
  2. on each of the 4 B0 `run` commands, the key reads `true` in `resolved configuration` (the red side). If it does not, the file never reached the tunnel, and the stage is VOID, not a pass;
  3. every S1 transfer completes (`TUNREP` with data) wherever the B0 transfer of the same (hint, rep) completes;
  4. both ends of every S1 invocation print TUNPIPE with the hint it was given and a `resolved configuration` line whose four object-mode fields read false.
- **Reported only:** per-hint goodput and completion time, S1 against B0 (n = 5, no A/A; never scored).
- **Prediction: PASS.** The default file holds no object-mode key, so the transfer path is unchanged.

### 3.4 Guard 1's decision rule

- **MERGE guard 1** (both `c8-gauges` and `tunnel-inorder-guard`, `--no-ff`, in that order, as S1 was built) iff all of these hold:
  - S1's suites and repeats are green (§2.2);
  - G1 PASS, A1 PASS, A1-R PASS (§4.3) and T PASS;
  - the §2.1 item 4 red/green record exists;
  - the binid check of §5.3 holds.
  
  The merge commit carries a `binid: expected-change <reason>` line (ADR-0082).
- **G1 FAIL:** no merge. Bisection next session (§5.2).
- **A1 FAIL:** the gauges do not merge. It is a finding about print cost under `RWM_DIAG`. The tunnel merges only after its own guard on B0 + tunnel (§5.2).
- **T FAIL:** the tunnel does not merge. It is a finding against the fix. The gauges merge only after their own guard on B0 + `c8-gauges`.
- **NO VERDICT:** re-run the stage.

## 4. Guard 2: S1 against S2 (the S06 deletion)

This is a deliberate behaviour change (plan §6). Its guard is a no-regression guard at Bulk plus off-Bulk stages, where the change acts.

**Deviation from ADR-0084 Amendment 1's "guard 1: BASE = main".** BASE here is S1, which is main + the gauges + the tunnel. Its engine differs from main only by print sites, and `taper=` reads the same on both. S1 is also what main becomes if guard 1 passes. If guard 1 fails and S1 does not merge, guard 2's verdict still stands as ADR-0079's guard, but the binid rule (§5.3) then needs S06 re-seated on main, with a new Stage G2.

### 4.1 Stage G2 (scored, Bulk): plan 41d unchanged, as G1

- The same 96-row design, rank-minp, **G2 PASS iff p > 0.05.**
- **Read as a check that Bulk did not regress, nothing more.** Bulk sends no proactive slots (rr = 0), so both seats take the same path (plan §6 item 2; roadmap `meas-guard-bulk-blind`). The off-Bulk stages carry the change.
- **Row witnesses:** `pslot=` prints on every S2 row (the seat witness) and `taper=0` on every S2 row. W-ROUTE is rr = 0 on every row of both seats. A seat check before any row: S2's binary holds `pslot=` and S1's does not (ABORT-SEAT).
- **Prediction: PASS.** The deleted branch draws only on repair debt, which Bulk never accrues.

### 4.2 Stage A (Auto, ρ = 1): witnesses scored, one goodput clause scored, delay reported

- **Cells:** c2-100 and c8-100, Auto hint, ρ = 1, 600 MB, `RWM_PERF_TIMEOUT_S=200`, `RWM_DIAG=1`.
- **Arms per cell:** S1 12, S2 12, B0 6 (B0 serves A1, §3.2). Seeds 42 and 7, interleaved in blocks of (S1, S2, S1, S2, B0). That gives 60 rows.
- **Precondition witnesses (a row failing one is VOID-WITNESS and re-run once; at most 4 replaced slots in the stage):**
  - S1 and B0: `taper=` > 0 and `plost=` = `taper=` on the last `[DIAG]` (the copy branch ran);
  - S2: `pslot=` > 0 and the `[SPAN]` median rr > 0 (slots were taken, so the deletion had something to act on).
- **Outcome witnesses on S2 (`S06-FAIL` on first sight: never VOID, never replaced):**
  - `taper=` > 0 or `plost=` > 0 on a row whose preconditions hold;
  - a DNF where every S1 row of the cell completes.
- **Below-floor arrivals: reported, with a hold flag, not a failure.**
  - A correct S2 is not near zero here. H-B's 854 (c2) is about its decode closes (≈ 0.0013 × 507 k ≈ 660), because a decode close does not cancel the NACK resend (audit item 1.iii). S2 decodes more (about 0.011 × 507 k ≈ 5.4 k closes per c2 row at Auto's measured yield), so its late resends land below the floor in the thousands.
  - Copies read 57–65 k (c2) and 90–97 k (c8) on S1-like rows (Addendum 2, `acct-c2.txt`, `acct-c8.txt`).
  - **Flag:** an S2 cell median above 0.25 × the S1 cell median, or above S2's own `fill_coded` count + 2 × H-B's level (≈ 2 k), means copies may still reach the receiver by another path. The merge waits for a read of those rows. The ratio to `fill_coded` is printed per row.
- **Minimum:** a cell with fewer than 10 live rows on S1 or S2 after replacement is UNSCOREABLE for AS and A1.
- **Clause AS (scored):** S2 against S1, one-sided (S2 worse), on **goodput** at c2 and c8, rank-minp over the two, 12 v 12. **AS PASS iff p > 0.05.**
  - **Prediction: PASS, goodput unchanged.** Each slot sends one symbol either way; only its content changes from a copy to coded repair (Addendum 2, "the same slots spent on coded repair").
  - Resolution: §51's in-session A/A at 6 v 6 gave c2 1 % and c8 3 %; 12 v 12 is tighter. Its realised d80 is printed from the S1 rows' half-splits.
- **Reported only (every one goes to the owner):**
  - **c8 in-order delay, two-sided,** with §41d's definition at ρ = 1. No direction is predicted (plan §6 item 3). c8's resolution is 50 % at 6 v 6 (§51), so a move below that is reported as unresolved, not as "no effect". It is confounded by the fixed store (status, "Consequence for open registrations"), so it is read beside hole-close time;
  - **hole-close time**, decode-resolved and resend-resolved, p50 and p90, on every row, from the receiver's `[SUCC]` summary line (`net/succ.rs`; `rate-overprovision.md` read 9 ms against 229 ms on c2 HA s42 r1 from it). The rehearsal smoke must show that the line carries both close times without `RWM_SUCC_DUMP`. If it does not, dump twins run (`RWM_SUCC_DUMP=1`; one per seat, seed and cell for S1 and S2, so 8 rows outside the scored rows), and that is written into the second amendment. Prediction at c2: more holes close by decoding on S2, since the 64 % of slots that were copies are now coded; the share is predicted in 63–83 % of source losses (derivation §4.1.1). c8: unknown within [6 %, 100 %];
  - c2 in-order delay (d80 12 % at 6 v 6);
  - `cpu_cli/GB` and `cpu_srv/GB`. Coded repair costs an encode where a copy cost a store read, so the sender's CPU may rise. **Owner flag:** S2 worse than §51's d80 (8 % c2, 12 % c8) with rank p ≤ 0.05;
  - `[DIAG] retx=` per source, `[RFA] fill_coded` per source, `cod=` / `pslot=`, and the gauge prints.
  - **The repair-rate readouts (Amendment 1), on S1 and S2, every row.** The audit's largest cost is the repair rate r at ρ = 1 (item 1: c2 −15 % goodput, +39 % client CPU/GB). S2 is the first copy-free reading of it, so this stage is where that cost gets split. From `c8-gauges`' prints, per tick, pooled over rows as the shares and p10 / p50 / p90:
    - **which term wins:** `[SPAN] rlaw=a:<core|burst|mass>/<sat|cap|free>,l:<…>/<…>,b=<β>`. The anchor term's winner and its binding bound are read as shares of ticks; the late-is-fine term and β are printed beside them (β = 0 at Auto);
    - **the live r_sat:** `[SPAN] rsat=` (1.0 means no cap), beside the granted `rr=` and the consumed r (`(cod + taper) / src`, `s51_score.py`'s r_consumed). The share of ticks with `rlaw` bound `sat` is r_sat's bind fraction;
    - **t_tail:** not printed. It is evaluated from the declared formula t_tail = 10⁻⁵·0.5/δ (Addendum 3) at the preset δ = 0.5 of the named Auto hint (these rows set no `RWM_DELTA`, so there is no δ echo to read): 1.0·10⁻⁵. It is labelled DERIVED, not measured;
    - **P_fec:** `[SPAN] pfec=`, printed beside `rsat=` and `rlaw=` (diag only, no decision) by `c8-gauges-pfec` (on top of `c8-gauges` `697eeb73`, carried by `vmb-s1` and `vmb-s2`). It is the normal P_fec of paper §4.2 at r = the rate law's own output for the worst-ε path, with the ε̂ (`p_upper`), window and σ² the law and `r_saturation` use (`RateSnapshot::p_fec`). Read per tick, pooled as p10 / p50 / p90 like `rsat=`. A row of a seat whose binary lacks the string `pfec=` is `NOT-AVAILABLE` for P_fec, and the ABORT-SEAT string check on S1 and S2 includes `pfec=`;
    - **the cost split**, printed per cell: S2's consumed r against S1's, with S2's coded share; S1's taper share (copies per slot, expected ≈ 0.64 at c2 and 0.98 at c8); goodput and CPU/GB per arm.

      This says how much of item 1's cost is the copies (cause i) and how much is r itself (causes ii–iv). It is reported and decides no clause.
- **Pool block (reported only; not cut, amendment 2026-11-20):** c8-100 Auto, `RWM_STORE_PATH_POOL` = 170, 256 and 640, S1 and S2, n = 3 each, seed 42. That gives 18 rows, with in-order delay, hole-close time, completion tail and receiver duplicates read per arm: the same readouts lever B was measured on. It decides nothing here, but it is lever B's revisit trigger (roadmap `rec-leverb-guard`): if a smaller pool closes the c8 tail without lever B's 2.1× duplicates, lever B is dropped.

### 4.3 Stage R1 (Realtime, ρ = 1, unpaced): completion scored

- c2-100, Realtime hint, `--window-reliable` on both ends, 100 MB, `RWM_PERF_TIMEOUT_S=150`, `RWM_DIAG=1`, plan 64's stage R mechanism (VOID-RHO on a missing `window_reliable: true`).
- **Arms:** S1 12, S2 12, and B0 6, the gauges' Realtime row. Interleaved (S1, S2, S1, S2, B0).
- **Clause A1-R (guard 1, scored):**
  - every S1 row carries the four gauge prints, and no B0 row does;
  - every S1 row completes wherever every B0 row completes.
  
  **A1-R PASS** iff both hold. Goodput and CPU for S1 against B0 are reported only (no Realtime A/A).
- **Witness:** S1 rows `taper=` > 0, and S2 rows `pslot=` > 0 with `taper=` = 0.
  - If S1 reads `taper=0` on more than 6 of 12 rows, R1 is `R1-BLIND`: the copy branch did not act there. That is reported, and the merge then rests on Stage A and R2.
- **Clause R1 (scored):** every S2 row whose witnesses hold completes inside its timeout, wherever every S1 row completes (plan 64 item 5's rule). **R1 PASS** iff that holds and no S2 row is `S06-FAIL` (the witness rule above).
- **Reported:** goodput, CPU/GB, lag, in-order delay (marked: under the 30 s tick floor).

### 4.4 Stage R2 (the evicting seat, paced; reported, plus one merge condition: R2-HOLD)

- **Why paced seats:** an unpaced evicting row takes almost no slots (§60: rr = 0 on 46 of 53 windows), and the pacer is not on main (plan §6 item 4). So R2 runs on P1 and P2, which never merge. It shows the deletion on the seat where the slot site used to shed; the merging binaries are the subject of Stages A and R1.
- Block E's PM shape: c2, Realtime, evicting (no `--window-reliable`), paced at 35 Mbit/s with `RWM_PERF_RATE_MBIT`. P1 6 + P2 6, seeds 42 and 7.
- **Smoke witness before the stage:** P1 `taper=` > 0; P2 `pslot=` > 0 with `[SPAN]` median rr > 0; both print the pacer echo. A failure makes R2 VOID; it goes back to the coordinator, and guard 2 then waits for an R2 re-run (the HOLD flag below needs R2).
- **Test-seams twins (Amendment 1, condition a):** P1T and P2T are P1 and P2 built `--release --features test-seams`, each with its own sha recorded in the pin amendment. They run the same PM rows, 6 + 6, with seeds 42 and 7, interleaved with R2.
  - **Where the line comes from** (checked at main):
    - the sender prints `[ACCT] … final=1` from `SenderState`'s `Drop` (`net/emit_source.rs`, "on every way the sender can end", independent of `RWM_DIAG`);
    - it prints a periodic `[ACCT]` line without the marker at the `[DIAG]` cadence (`net/diag.rs`);
    - the receiver prints `[ACCT-RX] frontier=… holes_given_up=… final=1` at its end.

    A process killed by the row timeout (SIGKILL) runs no `Drop`. So the reading is the sender's line marked `final=1` if present, else its **last periodic `[ACCT]` line** (labelled `ACCT-LAST`, never mixed into the `final=1` medians without the label).
  - **Twin row witness:** an `[ACCT]` line (final or periodic) on the client and an `[ACCT-RX]` line on the server. A row with neither is VOID and re-run once.
  - **Unresolved blockers per row:** `U = O_serv + O_strand` on that line.
    - `O_serv` includes ordinary in-flight seqs if the line is read before the last acks return. So printed beside U are `O_strand`, `O_max`, `oldest_unchanged_max`, `inert_sweeps` and the receiver's `holes_given_up`.
    - `oldest_unchanged_max` (the longest time one blocker stayed oldest) is the stall reading that in-flight data cannot inflate.
  - **Minimum:** at least 5 live twin rows per seat. Below it, R2-HOLD is UNSCOREABLE, and guard 2 is **not eligible** until the twins are re-run. "Unscoreable" is never read as "did not fire".
  - **R2-HOLD** (registered, holds the S06 merge) fires iff either holds:
    - **(i)** an exact one-sided rank-sum test (Mann–Whitney, all relabellings, P2T worse) on U gives p ≤ 0.05;
    - **(ii)** the same test on `oldest_unchanged_max` gives p ≤ 0.05.

    At 6 v 6 the smallest attainable p is 1/924, so the test can fire. It resolves only a shift that puts most P2T rows above most P1T rows; smaller differences are not resolved, and that is stated with the reading. A bare median or count comparison is not used: with U > 0 common on both trees (loopback 19/30 and 15/30), it would fire on noise about half the time.

    R2-HOLD is read and diagnosed before S06 merges. It is not a FAIL label, because the unresolved blocker is a defect on main too (`rec-evict-blocker-unresolved`).
- **Reported, read under ADR-0079 Amendment 1:** `shed_slot` (P2 = 0 by construction), `shed_nack`, `shed_denied`, `shed_total`, and the delivered fraction.
  - Conservation is no longer the expectation: the 2026-11-20 review measured the NACK site absorbing the slot sheds (245/90 vs 250/0 at a forced budget).
  - **Printed:** `shed_total(P2) − shed_total(P1)` with its per-seed range, and the twins' account identity per row (`S_sent = A + H + E + O_serv + O_strand`).
  - In-order delay is undefined at ρ < 1 (§41d item 3), so it is not read.

### 4.5 Guard 2's decision rule

- **ELIGIBLE TO MERGE** iff all of these hold:
  - S2's suites and repeats are green (§2.2);
  - G2 PASS, AS PASS, and R1 PASS or `R1-BLIND`;
  - there are zero `S06-FAIL` rows in G2, A and R1;
  - no below-floor hold flag is unread, and R2-HOLD did not fire, or fired and was read with a verdict that clears it;
  - T2's sim port is red on S1 and green on S2 (§2.1 item 3);
  - §5.3's binid check holds.
- **Then S06 merges on its own (owner, 2026-11-20).** Two things come first: ADR-0084 Amendment 1's "not on its own" clause is rewritten on `adr84-amend`, and the coordinator records the merge. Then `s06-delete` merges (merged up, as S2) with `--no-ff` and a `binid: expected-change` line (ADR-0082). The paper's present tense comes in the same merge, because it is already inside `s06-delete`. ADR-0079's Status and its `docs/adr/README.md` row name the merge commit, and the paper's §10 row names the deletion hash.
- **G2 FAIL:** no merge. The failing Bulk clause is diagnosed. Bulk does not reach the deleted branch, so a FAIL points at the merge-up or the §1.3 cleanup (the ε slot removal).
- **Any `S06-FAIL` or AS FAIL:** no merge. It is a finding against the deletion and is diagnosed first.
- **Stage A's delay, hole-close and rate readings decide nothing.** They are reported with the merge.

## 5. Statistics, n, stop rules and binary identity

### 5.1 n per arm

| stage | cell(s) | arms × n | rows |
|---|---|---|---|
| G1, G2 | c1s, c1d, c2, c8 | 12 + 12 per cell | 96 each |
| A (shared with A1) | c2, c8 at 600 MB | S1 12, S2 12, B0 6 per cell | 60 |
| A dump twins (only if `[SUCC]` lacks the close times) | c2, c8 | S1, S2 × seeds 42, 7 | 8 |
| pool block | c8 | 3 pools × S1, S2 × 3 | 18 |
| R1 (shared with A1-R) | c2-100 Realtime ρ = 1 | S1 12, S2 12, B0 6 | 30 |
| R2 | c2 paced evicting | 6 + 6 | 12 |
| T | c2 tunnel | 2 seats × 2 hints × 5 reps, + 16 refusal commands | 4 invocations |

### 5.2 Stop rules

- **Precondition P0:** the owner has said the Ceph instability is over. No session launches before that.
- **Session 0 (pre-flight) and session 1 (rehearsal):** any ABORT-PREFLIGHT, ABORT-TESTS, ABORT-REPEAT or ABORT-SEAT stops that seat's guard.
  - On S1, it stops guard 1. Guard 2 then has no valid BASE, so it waits for re-seating on B0.
  - On S2, it stops guard 2. Guard 1 still launches, with S2 dropped.
- **Sessions 2 and 3, block-1 rule** (§51 Amendment 6): after each stage's first block, every arm's first real row must be LIVE with its precondition witnesses. Otherwise the stage stops with `FAILED-<stage>`, and the next stage still runs.
- **G1 FAIL:**
  - G2 still runs; its BASE is S1, so it is still ADR-0079's comparison.
  - The next session bisects: one Stage G on B0 + `c8-gauges` alone and one on B0 + `tunnel-inorder-guard` alone. These seats, `vmb-c8` and `vmb-tun`, are pre-pinned at the second amendment. About 55 min each.
- **G2 FAIL, or an `S06-FAIL` in G2:** session 3 drops Stage A's S2 arm, R1's S2 arm and R2, and keeps A1, A1-R and T. Any S06 rows run from then on are for diagnosis, under their own registration.
- **Cap cuts:** the dump twins only. The pool block is lever B's revisit trigger and is not cut; if session 3 runs long, it moves to its own session. R2 and its test-seams twins are not cut: R2-HOLD is a merge condition. Each cut is written into an amendment before the session.

### 5.3 Binary identity after each merge (the coordinator's rule)

After each merge, before anything else lands on main:
- `binid_run.sh` builds an archive of the merged main and an archive of the guarded seat (S1, then S2), at the same path with a fresh target.
- **The two shas must be equal.** They are equal by construction when the merged tree equals the seat outside `docs/`, `tools/` and the roadmap. That source-tree check is recorded too.
- A mismatch is a hold: the merge is reverted, or the merged tree gets its own guard.

The harness's `LD_BIN_SHA` values are not used for this. The harness builds at its own path, and the binary embeds location data (§58).

### 5.4 Abort classes (filled in the Result, in this order)

ABORT-LOCK, ABORT-CRLF, ABORT-SEAT, ABORT-BUILD (sha mismatch against a pin), ABORT-PREFLIGHT, ABORT-TESTS, ABORT-REPEAT, ABORT-SMOKE, a non-zero stage exit, and bring-up exhausted (Stage T).

## 6. Sessions, rehearsal and run order

### 6.0 Session 0, pre-flight with no battery (`suites_run.sh` envelope with `run_suites`' fixed rule; `LD_HARD` = launch + 4.9 h)

All of §2.2: the check, the gates on B0, the suites on S1 and on S2, the repeat runs, the visualizer and binid on three seats. No `raptorpath` row runs. The results are reused by the later sessions through the reference manifest (plan 64 §1.3).

### 6.1 Session 1, rehearsal (`LD_PLAN=67 LD_REHEARSE=1`; `LD_HARD` = launch + 2.25 h)

- Five release builds, with shas recorded.
- The tools tests, with `TOOLS-MANIFEST`.
- The suite-reuse HIT check against session 0.
- One smoke per stage, each of which must show its stage's witnesses:
  - **G:** plan 41d's c1s B0 and S1 rows, and a 600 MB c8 S2 row;
  - **A:** one c8 Auto row per seat with §4.2's witnesses, and the `[SUCC]` close-time check;
  - **R1:** one row each of S1, S2 and B0, with `window_reliable: true` on both ends;
  - **R2:** one P1 and one P2 paced row, with the R2 smoke witness, and one P1T and one P2T row. Each twin row must show an `[ACCT]` line on the client and `[ACCT-RX]` on the server in the battery's kept log, not only in the raw log;
  - **T:** one invocation per seat at `auto` with REPS = 1, and the 16 refusal commands.
- A rehearsal row is not re-run. A failed S2 outcome witness in the rehearsal is a finding.
- The second amendment then pins the five release shas, the binid shas, the repeat counts, the twin decision and any cap cut.

### 6.2 Session 2, `LD_PLAN=67g` (`LD_HARD` = launch + 3 h)

G1, then G2. Both use the rehearsal's binaries (`LD_BIN_PREBUILT_*`, sha-checked) and its suites and tools results.

### 6.3 Session 3, `LD_PLAN=67x` (`LD_HARD` = launch + 4.5 h)

In order: A (with A1), R1 (with A1-R), T, R2, the dump twins if needed, then the pool block.

### 6.4 Time estimate (4 cores, 11 GB, one session at a time)

| session | content | typical | basis |
|---|---|---|---|
| 0 | check × 2 ≈ 15 min; gates on B0 ≈ 10 min; suites on S1 and S2 ≈ 2 × 55 min; repeats ≤ 60 min; visualizer ≈ 10 min; binid × 3 ≈ 50 min | **≈ 4.3 h** | suites 45–60 min (§58 Part A, 60-pre); binid ≈ 15–20 min per tree (§58 Part B) |
| 1 | 7 builds (5 seats + 2 test-seams twins) ≈ 55 min; tools tests ≈ 8 min; 10 smoke rows plus T ≈ 25 min | **≈ 1.5 h** | builds 15 min per two (§64); smoke as §64 |
| 2 | G1 96 rows ≈ 53 min; G2 ≈ 53 min; replacements and scoring ≈ 15 min | **≈ 2.0 h** | §64 Result: 96 rows in 53 min |
| 3 | A 60 rows × ≈ 80 s ≈ 80 min; R1 30 ≈ 14 min; T ≈ 20 min; R2 12 + twins 12 ≈ 32 min; pool 18 ≈ 25 min; scoring ≈ 10 min; twins if needed + 11 min | **≈ 3.1 h** (≈ 2.5 h without the pool block) | block 51: 76 rows of 600 MB in 1 h 44 min; §64 stage R; block E rows ≈ 62 s |
| **total** | | **≈ 10.9 h** (≈ 10.3 h with the pool block cut) | host cargo paused during each session |

**Worst case, about 15.5 h:** a rehearsal re-run (+1.5 h), a session-0 re-run after a fix (+4.3 h, only if a seat changes), and in session 2 a NO VERDICT re-run of one Stage G (+1 h) or the G1 bisection (+2 h). Every session stays under the five-hour cap.

## 7. What goes back to the owner (with the Result)

1. **S06 (decided: merges on its own PASS).** The Result reports these with the merge: Stage A's c8 delay (two-sided), the hole-close times, the CPU owner flag, the repair-rate split, the pool block, R2's shed table, and R2-HOLD with its reading.
2. **Guard 1:** no owner question beyond the Result, unless A1 shows a print cost above its resolution.
3. **Ceph clearance** (P0) before session 0.
4. **The VM queue order** (§11): this batch goes before M1, unlike the 2026-11-19 queue. That is the owner's call.

## 8. Lever B (the c8 gap horizon, `rec-gap-horizon` / `rec-leverb-guard`): not in this batch

**Current state.** `gaphorizon` is at `98ecf038` (2026-11-12), 628 commits behind main, two commits ahead: the red test and `MAX_SACK_RANGES` as the horizon. The superseded guards are `cand52` / `base52` / `cand53`. `rec-leverb-guard` is **parked by the owner** (decision 6; reversed and re-affirmed on 2026-11-19: "lever B and a smaller pool constant are both workarounds; the fix is the store law").

**What it would need to enter a batch:**
1. the owner's reversal of decision 6;
2. a merge-up onto main, with `net/mod.rs`, `sack.rs` and `tests.rs` re-read against the merged recovery account and the gap clamp macro;
3. the C03 snapshot-order filter on main (C02 "ships with C03": `rec-s54-session` is done, so verify the filter is the merged `snapver-law`);
4. its own guard with off-Bulk stages, carrying §52's c1d CPU and duplicate cost (receiver duplicates ×2.1) as a clause or an owner-flag readout;
5. the horizon bind gauge (status, store-law round 2: a ripe-overflow fraction on the sender and a SACK-prefix bind fraction on the receiver), so the 20 → 64 change is read through a gauge, not pinned.

## 9. Harness change (`harness67`, Sonnet worker, tools only)

The plan tokens are `67` (rehearsal), `67g` and `67x`. It goes into all seats, not main, before the Result.
1. **Stage G:** plan 41d's guard for G1 and G2, with `score41d` output byte for byte, plus per-stage row witnesses (§3.1, §4.1) and the ABORT-SEAT string checks.
2. **Stage A and A1:**
   - Auto rows at 600 MB with three seats;
   - the witnesses and `S06-FAIL` status of §4.2;
   - `scoreA1` and `scoreAS` (rank-minp from `place38_parse`, exact null at small n);
   - the reported readouts: in-order delay through `threadp1_parse.inorder_delay_stats`, the copy and slot counters as `s51_score.py` reads them, and below-floor as `acct.py` reads it;
   - a `[SUCC]` hole-close parser on every row, and the dump twins only if the summary line lacks the close times;
   - the pool block (`RWM_STORE_PATH_POOL` per row, as plan 54b).
3. **R1:** plan 64's stage R row mechanism, with three arms (S1, S2, B0) and §4.3's witnesses and clauses (R1, A1-R).
4. **R2:** plan 51e's paced PM rows on P1 and P2, plus the test-seams twins P1T and P2T: built `--release --features test-seams` with their own shas, `[ACCT]` and `[ACCT-RX]` added to the battery's kept-line filter (the §64 `[SYMSZ]` lesson), an `[ACCT]` parser (final, else last periodic, labelled), and the R2-HOLD rule with its exact rank test and its minimum. Stub paths: `twin-no-final` (reads `ACCT-LAST`), `twin-under-min` (UNSCOREABLE, not eligible), `twin-worse` (R2-HOLD fires). `RWM_PERF_RATE_MBIT` is forwarded only through the pacer seats' own `lib.sh` line (§1.2). It carries §4.4's readouts.
   - **The rate readouts:** an `rlaw=` / `rsat=` parser, shares and quantiles per arm, t_tail as DERIVED, `pfec=` read with them, and the cost-split table (§4.2).
5. **T:** `tun_bulk.sh` invocations, the refusal commands and `scoreT` (§3.3), parsed with `verify4_parse.py tun`.
6. **Suites (session 0's envelope):** `run_suites` with ADR-0080's rule (§2.1 item 2) and `test_harness47.py` updated; the repeat stage of §2.2; `binid_run.sh` on three seats.
7. **Reuse:** prebuilt binaries and the suite/tools manifests, as plan 64.
8. **Stub paths, red first where new:**
   - `flake-not-quarantined` → ABORT-TESTS;
   - `s2-taper-nonzero` → `S06-FAIL`, not replaced;
   - `s2-no-pslot` → VOID-WITNESS, replaced once;
   - `s2-belowfloor-high` → hold flag, not a fail;
   - `a-under-min` → UNSCOREABLE;
   - `b0-run-key-false` → T VOID;
   - `s1-no-gauge` → VOID-WITNESS;
   - `r1-blind`;
   - `t-refusal-missing` → T FAIL;
   - `t-base-refuses` → ABORT-SEAT;
   - `repeat-one-failure` → ABORT-REPEAT;
   - `prebuilt-sha-mismatch` → ABORT-BUILD;
   - plus `STUBRUN_FULL=1` for each token.

## 10. Review

Fable reviewed twice.
- **Before drafting**, Fable gave the structure: cumulative seats, the S06 hold, lever B out, Ceph, the flake rule, T2 rather than T3, N = 30, hole-close time and pool arms, separate pacer seats, and vm_order as a proposal.
- **On the draft (`f5959d0a`)**, Fable's verdict was "accept once items 1–8 are in":
  1. the below-floor cap would fail a correct S2, so it became a hold flag scaled to `fill_coded`;
  2. precondition and outcome witnesses are split;
  3. the pacer's forward line;
  4. suites on S1;
  5. session 1 broke the cap, so session 0 was split off;
  6. `[ACCT]` against R2;
  7. a Realtime row for the gauges (A1-R);
  8. the order against the audit is stated.
  
  Smaller items, all taken: T's red side read from B0's `resolved configuration`, `[SUCC]` read on every row, S1 suites in §3.4, a replacement cap and an UNSCOREABLE minimum in Stage A, and the `binid: expected-change` line.
  
  This revision takes all of them.

## 11. Roadmap: proposed `queue_order` (proposal only; not edited here)

| queue_order | node | lane change needed | proposed `order_reason` (with the estimated gain) |
|---|---|---|---|
| 1 | `meas-s51-c8-gauges` | `host-cargo` → `vm` | VM batch 1, guard 1, at the bottom of the stack. No product gain of its own (print sites only). It makes the repair rate readable: which term wins, and the live r_sat. That readout splits the audit's top cost (c2 Auto −15 % goodput, +39 % client CPU/GB) between the copies and r itself. |
| 2 | `infra-tunnel-inorder-guard` | `host-cargo` → `vm` | VM batch 1, guard 1 (with the gauges). A correctness fix on the product path: a config file can no longer switch the tunnel to out-of-order or object delivery. Gain: every byte stays in order for anyone who sets one of the four keys; nothing changes for the default file. |
| 3 | `rate-s06-plost-derivation` | `no-cargo` → `vm` | VM batch 1, guard 2; merges on its own PASS (owner, 2026-11-20). It turns the 64 % (c2) and 98 % (c8) of proactive slots that are copies into coded repair. Expected: goodput unchanged (same slot count), copy arrivals at the receiver from 57–97 k per row to roughly a decode-close level, decode closes at c2 from 42–46 % toward 63–83 % of source losses, and the first copy-free reading of r. It is the prerequisite for the rate fix that can recover up to the 15 % goodput and 39 % CPU. |

- `rec-leverb-guard` gets no order (parked, §8).
- `meas-guard-bulk-blind` stays `no-cargo`. This batch is its first per-change use. Its "reusable plan" done criterion is met only if `harness67`'s off-Bulk stages are kept as a plan.
- `check.py` warns on a queue_order outside the vm lane, hence the lane changes. M1's place relative to these is the owner's call (§7 item 4).

## Amendment 1 (2026-11-20, before the pin; owner decision and Fable's review)

1. **S06 merges on its own guard-2 PASS (owner).** It is not held beneath ADR-0084: a pass goes stale as main moves, and holding it would mean guarding it again. §4.5, §7 and the owner block are updated.
   - **Owed before the merge:** ADR-0084 Amendment 1 rewritten on `adr84-amend`. That covers item 3 ("lifts it for this purpose only … not on its own"), the Consequences line ("ADR-0079's merge hold is lifted only for the stacked build"), and ADR-0079's Status sentence that quotes it.
   - The same item 3 names guard 1's BASE as main. Here it is S1 (§4, the deviation note), so that sentence is rewritten too.
   - Guard 2 of ADR-0084 then takes the merged S2 (main) as its BASE.
2. **Conditions on that merge, registered here:**
   - (a) **R2-HOLD:** the unresolved-blocker count from the test-seams twins P1T and P2T (§1.2, §4.4). S2 must be no worse than S1.
   - (b) **T2 ported to the sim** is a hard precondition of guard 2's pin, not a choice (§2.1 item 3).
3. **The repair-rate readouts in Stage A** (§4.2), reported only, on S1 and S2:
   - `rlaw=` (the winning term and binding bound, plus β);
   - `rsat=` (the live r_sat);
   - t_tail, as DERIVED from its declared formula;
   - P_fec (`pfec=`, added to `c8-gauges` before the pin);
   - the copy-against-r cost split.
4. **Order reasons** in §11.
5. **Time:** two more builds and 12 more paced rows, so ≈ +0.6 h (typical ≈ 10.9 h). `LD_HARD`: session 1 at launch + 2.25 h, session 3 at launch + 4.5 h.
6. **Fable's review of this amendment:** "accept once items 1–5 are in". All five are in:
   1. R2-HOLD uses an exact rank test, not a median comparison;
   2. the `[ACCT]` source is checked, with a fallback to the last periodic line, a minimum of live twin rows, twin smoke rows and the kept-line filter;
   3. R2 is no longer cut under the cap, and `LD_HARD` is raised;
   4. text from before the owner's decision is removed;
   5. guard 1's BASE is added to the ADR rewrite list.

   Also taken: t_tail at the preset δ, and the paced sim client for T2's `pslot` assertion. Main moved to `917e069e` and later with test-only engine commits (the decision pin and `sim_lost_tail_paced`); `s06-delete`'s conflict set against main is unchanged (the same three files).

## Amendment 2 (draft, for review): harness decisions

Draft. Not accepted. It lists the decisions the plan-67 harness takes that this registration does not state. Rows 1-135 were first read at github/main `9bc656bf`; every row is read at harness67c `3092dc96`. Each row gives the decision, where the code takes it, and a verdict: MATCHES (stated, but the code is more specific), EXTENDS (the registration is silent and the code picks), CONTRADICTS (the code differs from the quoted text), or FIXED on harness67c (a former CONTRADICTS that now matches the registration text, with the line). Paths are under `tools/l1/`. `run_all` is `loopdiag_run_all.sh`, `battery` is `loopdiag_battery.sh`, `p67` is `loopdiag_p67.sh`, `p67pre` is `loopdiag_p67pre.sh`. Scoring choices that belong to one stage sit in that stage's table; the last table holds the ones that cut across stages. Rows 136-153 were added at `eb976857` and row 154 at `3092dc96`; each sits in its area's table.

### Seats, build and pins

| # | decision | where | verdict |
|---|---|---|---|
| 1 | No seat has a default commit pin. Every seat's `LD_PIN_<SEAT>` is required in every session, 67pre included; an unset pin is ABORT-SEAT. (At `9bc656bf`, S1 and S2 defaulted to the old heads.) | plan67.py:16, 33-44; p67:53-61 | MATCHES |
| 2 | P1T and P2T reuse P1's and P2's pin (`LD_PIN_P1`, `LD_PIN_P2`) and source tree. | plan67.py:43-44 | EXTENDS |
| 3 | The pacer commit `4ee1df9b` is informational. Nothing checks that P1 and P2 contain it, except the binary strings in #10. | plan67.py:51 | EXTENDS |
| 4 | A pin is a prefix match: `$ROOT/seats/<tree>/COMMIT` must start with the pin, so an 8-hex pin accepts any commit that shares those characters. | p67:59-60 | EXTENDS |
| 5 | Paths: trees at `$LD_ROOT/seats/<B0,S1,S2,P1,P2>/src`, targets at `$ROOT/target-<seat>` (must be fresh), build logs at `$ROOT/build-<seat>.log`, installed binaries at `$LD_SHM/bin-<seat>/raptorpath` (root, 0755), kept copies at `$RUN/bins67/<seat>-raptorpath`, shas in `$RUN/BINSHA.txt`. | p67:13, 59, 70-94 | EXTENDS |
| 6 | The rehearsal builds the seven seats one after another, with `CARGO_BUILD_JOBS=4` and `cargo build --release --bin raptorpath`, adding `--features test-seams` for P1T and P2T. | p67:71-87; plan67.py:43-44 | EXTENDS |
| 7 | The rehearsal also accepts `LD_BIN_PREBUILT_<SEAT>` and then skips that seat's build. §6.1 says "five release builds". | p67:77-80 | EXTENDS |
| 8 | A launch (67g, 67x) never builds. Every seat it uses needs both `LD_BIN_PREBUILT_<SEAT>` and `LD_BIN_SHA_<SEAT>`, else ABORT-BUILD. In the rehearsal a given `LD_BIN_SHA_<SEAT>` is checked, and an absent one is recorded. | p67:77-100 | MATCHES |
| 9 | Any two seats with the same binary sha are ABORT-BUILD. | p67:104-106 | EXTENDS |
| 10 | Identity strings (ABORT-SEAT before any row, the rehearsal included). Beyond §3.1 and §4.1: B0 also forbids `pfec=`, `pslot=` and `[ACCT-RX]`. Every non-twin seat forbids `[ACCT-RX]`. The P seats need `RWM_PERF_RATE_MBIT` and `offered-load pacing ACTIVE`. The twins need `[ACCT-RX]`. P1 and P1T forbid `pslot=`. | plan67.py:35-44; p67:108-116 | EXTENDS |
| 11 | Each stage's battery checks the identity strings and the sha again (`preflight_binary ... RWM_STORE`, `--help` runs), and exits 5 on a failure. The envelope then marks the stage not ok and goes on to the next stage. | battery:293-306; tun67_battery.sh:40-51; p67:280-284 | EXTENDS |
| 12 | The tools-diff witness compares each seat tree's `tools/l1` file manifest with the harness's own `tools/l1`. §1.3 says `git diff B0 S1 -- tools` and `git diff S1 S2 -- tools`. The rest of `tools/` (e.g. `tools/ci`, `tools/roadmap`) is not compared. | p67:119-137 | CONTRADICTS |
| 13 | The witness reference is the harness tree running the session, not B0. It runs on the VM over the archives, not as a `git diff`, and skips `*.pyc` and `__pycache__`. | p67:120-121 | EXTENDS |
| 14 | The pacer exemption: P1's and P2's `tools/l1` may differ in `lib.sh` only, and `lib.sh` must equal the harness's once the token ` RWM_PERF_RATE_MBIT` is deleted (a token in the forward list, not a whole line). | p67:129-133 | EXTENDS |
| 15 | Suite reuse is checked for S1 and S2 only, against `LD_REF_MANIFEST_S1` and `LD_REF_MANIFEST_S2` (required, else ABORT-SUITES). B0, P1 and P2 have no suite record and no reuse check. | p67:139-152 | EXTENDS |
| 16 | The reuse manifest (`src_manifest67`, one definition for session 0 and the comparison) covers the Cargo files, the engine crates' src, `raptorpath/tests`, `gf256/src`, `vendor/`, and also `tools/ci/flake_rule.py`. Session 0 writes it on the VM over the archive files (#146). `ref_manifest67.sh <rev>` is the host-side equivalent over git blobs. | p67:44-50; p67pre:43-47; ref_manifest67.sh:11-13 | EXTENDS |
| 17 | The session refuses to start unless the time left before `LD_HARD` covers `plan67.BUDGET_S`: 17 040 s (67pre), 7 500 s (67), 10 000 s (67g), 15 600 s (67x). Against §6's offsets of 4.9 h, 2.25 h, 3 h and 4.5 h, that leaves 600 s of slack at launch for 67pre, 67 and 67x and 800 s for 67g. `LD_HARD_MIN_S` overrides it; it is a test seam (#140). | plan67.py:75; run_all:391-397 | EXTENDS |
| 18 | Inherited plan-46 gates: the session waits without a bound (30 s polls) until both locks are free. It then needs 5 samples of `procs_running` at most 2 and no cargo or rustc, else ABORT-PRECONDITION. | run_all:259, 351-366 | EXTENDS |

### Session 0, suites, repeat and binid

| # | decision | where | verdict |
|---|---|---|---|
| 19 | Plan 67 never runs the engine suites in sessions 1-3 (`ENGINE_SUITES=0`). The reference-manifest HIT stands for them. | p67:155 | MATCHES |
| 20 | Before the seats are built, every plan-67 session (67pre included) runs these tools tests from the harness tree: `test_loopdiag_parse`, `test_harness46`, `test_harness47`, `test_l1common`, `test_pool54`, `test_harness64`, `test_guard_v2`, `test_bbr48_parse`, `test_place38_parse`, `test_s67`, `test_harness67`. A launch runs them even when it reuses the rehearsal's tools result. In 67pre this pre-build gate is the only tools-test run (#21). | run_all:124-125, 455-462, 476-480 | EXTENDS |
| 21 | The tools tests in `run_suites` are every `tools/l1/test_*.py` except `test_stubrun.py`, plus `test_lib_battery.sh` and `test_abort_witness.sh`. They run from the harness tree, not from each seat's tree. Session 0's suites skip them (`SUITES_NO_TOOLS=1`), so session 0 runs only the pre-build gate of #20; the full set runs in session 1. | run_all:810-820; p67pre:89 | EXTENDS |
| 22 | A launch reuses the tools result only when `LD_PRIOR_TOOLS` holds `TOOLS-TESTS-OK` and a byte-identical `tools/l1` manifest. On a MISS it runs the tests; that is not an abort. | p67:156-160; run_all:880-892 | EXTENDS |
| 23 | ADR-0080's rule in `run_suites`: a failure fails unless the test is on `raptorpath/tests/quarantine.txt` (checked by `tools/ci/flake_rule.py --check`). The solo re-run is `-- <t> --exact --test-threads=1` and only classifies the failure. A failure with no named test is REAL. The target dir (`$ROOT/target-tests`, or session 0's `SUITES_TT`) must not exist, else the suites fail. | run_all:745-754, 768-795 | MATCHES |
| 24 | Repeat defaults: `R67_N=30`, `R67_BUDGET_S=3600`, seats S1 then S2, one seat after another, a fresh `CARGO_TARGET_DIR=$R67_TGT/tgt-<seat>` (session 0 passes a disk path under `$ROOT`) removed before and after. `R67_N`, `R67_BUDGET_S`, `R67_SIM` and `R67_T1_S` are test seams (#140). | repeat67_run.sh:22-26, 50-51, 99; p67pre:122-124 | EXTENDS |
| 25 | The repeat budget is split equally between the seats (1 800 s each). The first run of the seat's whole command is timed, and the cut is `floor(share / t1)`. The cut applies to all of the seat's tests at once, not per test. The build and the sim runs are outside the budget. | repeat67_run.sh:57, 59-82 | EXTENDS |
| 26 | If the budget does not fit one run, that is ABORT-REPEAT. A cut is written as `REPEAT-CUT seat=<S> reached=<n>`. | repeat67_run.sh:73-77 | EXTENDS |
| 27 | The repeat commands use `--release`: S1 `cargo test -p raptorpath --release --lib -- config::`; S2 `--release --test s06_no_copy --test s06_reactive --test holeclass_reachability --no-fail-fast`. | repeat67_run.sh:53-54 | EXTENDS |
| 28 | A failing run is a named `FAILED` test, a missing `test result` line, or a non-zero exit. No quarantine applies. | repeat67_run.sh:68-70 | EXTENDS |
| 29 | The sim tests run on S2 only. The default list (`R67_SIM`) is `sim_shed_reach sim_lost_tail sim_nack_test place_arms_reachability`, run twice, one command per run. The digest is the sorted `test ... ok/FAILED/ignored` lines. Only the first run is checked for a `test result` line. | repeat67_run.sh:24, 54, 84-92 | EXTENDS |
| 30 | The repeat takes both VM locks (inherited from session 0, #142), refuses with ABORT-PRECONDITION if cargo or rustc is running, and exits 5 on any failure. | repeat67_run.sh:30-39, 95-104 | EXTENDS |
| 31 | binid packs each given seat's `src` plus its `COMMIT` file into a tarball and builds it with `binid_run.sh` at `B67_WORK`. Session 0 passes B0, S1, S2 (B0, S1 under `LD_DROP_S2=1`). The shas must be pairwise different over the given seats, else `BINID67 FAIL`. | binid67_run.sh:16, 20-29, 41-45; p67pre:165 | EXTENDS |
| 32 | `repeat67_run.sh` and `binid67_run.sh` are called by session 0's envelope, `loopdiag_p67pre.sh`, which also runs `cargo check`, the gates on B0, the S1 and S2 suites and the visualizer (#141-#147). | p67:63; p67pre:60-171 | MATCHES |
| 141 | Session 0 is `LD_PLAN=67pre`, run as `loopdiag_run_all.sh` → `loopdiag_p67.sh` (the pins of B0, S1, S2) → `loopdiag_p67pre.sh`, with no build and no battery. It refuses `LD_REHEARSE=1` (ABORT-REHEARSE) and needs 17 040 s before `LD_HARD` (#17). Its only tools tests are run_all's pre-build gate from the harness tree (#20, #21); §2.2 runs the tools tests in session 1. §6.0 says "`suites_run.sh` envelope". Steps in order: 0 the locks, 1 `cargo check --all-targets` on S1 and S2, 2 the gates on B0, 3 the suites on S1 then S2, 4 the repeats, 5 the visualizer and the golden test on S2, 6 binid then BINTEST, 7 the record (`SESSION0.txt`, `LD-ALL-SESSION0-DONE`). A step starts only with 600 s left (else `ABORT-BUDGET plan 67pre step <name>`) and runs under `timeout` to `LD_HARD` (rc 124 is the same abort). Every target dir must be fresh (`ABORT-PREFLIGHT plan 67pre: <dir> exists ...`) and is removed after its step. `LD_DROP_S2=1` runs no S2 step. | run_all:124-125, 141, 190-191, 476; p67:30, 63; p67pre:1-41, 60-212, 89; plan67.py:75, 80 | CONTRADICTS |
| 142 | Step 0, the locks: a trivial child (`lockcheck67.sh`), started the way the real children are, must log `LOCK-INHERITED` for both locks within 30 s, else `ABORT-PREFLIGHT plan 67pre step locks: a child cannot inherit the session's locks`. The children (`lockcheck67.sh`, `repeat67_run.sh`, `binid67_run.sh` through `binid_run.sh`) get `LB_LOCK_PARENT=<session pid>` and both lock paths. `take_lock` treats a lock whose holder field is that live pid as inherited: logged, never taken, never released. | p67pre:38-40, 49-58; lockcheck67.sh:1-16; lib_battery.sh:97-108; repeat67_run.sh:37-38; binid_run.sh:31-32 | EXTENDS |
| 143 | Steps 1-3. Check: `ABORT-PREFLIGHT plan 67pre seat <S>: cargo check --all-targets failed`. Gates on B0: no `test result` line is `ABORT-PREFLIGHT plan 67pre seat B0: the gates reference did not run`; B0's failing `gates::` tests are GATES-B0-FINDING, not an abort. Suites (`run_suites`: main, doc, wasm without `GOLDEN_CAPTURE`, core, debug, and three f2 test-seams targets): the failures are the REAL-FAILURE and FLAKE-FAILURE lines (QUARANTINED is not one). A `gates::` failure B0 does not have is `ABORT-PREFLIGHT plan 67pre seat <S>: gates failure not equal on B0`; any other failure is `ABORT-TESTS plan 67pre seat <S>`; with both, both lines print and the abort is ABORT-PREFLIGHT. A `gates::` failure B0 shares is GATES-FINDING-ON-MAIN, and the suites count as passed. A suite killed at `LD_HARD` is ABORT-BUDGET. The touched-target list is reported (`TARGETS seen / not-seen`), not gated. | p67pre:60-118; run_all:796-806 | MATCHES |
| 144 | Steps 4-6. Repeat: `ABORT-REPEAT plan 67pre seat <S>`, or `ABORT-REPEAT plan 67pre: repeat67_run.sh rc=<n>` for a precondition or lock failure. Visualizer: `build_visualizer.sh` runs in a scratch copy of S2's tree with a fresh wasm (`ABORT-PREFLIGHT plan 67pre seat S2: visualizer ...`); then `cargo test -p raptorpath-wasm golden` without `GOLDEN_CAPTURE`, which must pass at least one test (`... seat S2: golden test failed` / `golden test ran no test`). Binid: no `BINID67 PASS` is `ABORT-PREFLIGHT plan 67pre binid: <reason>`. | p67pre:120-172 | EXTENDS |
| 145 | The binid inner-test rule. `binid_run.sh` runs the math and wasm tests (BINTEST) on each seat's tree and does not fail on them. Session 0 applies the gates rule: a failing BINTEST test on S1 or S2 that B0 does not fail is `ABORT-PREFLIGHT plan 67pre binid: BINTEST <S> <names> not equal on B0`; one B0 also fails is BINTEST-FINDING-ON-MAIN, and the session goes on. A BINTEST with no BINTEST line or no `test result` line is `ABORT-PREFLIGHT plan 67pre binid: BINTEST <S> ran no test`, on any seat; on B0 it stops the whole session. §2.2 gives binid no test rule. | p67pre:173-193; binid_run.sh:60-66 | EXTENDS |
| 146 | REF-MANIFEST per seat: `$RUN/REF-MANIFEST-<S>.sha256` (`src_manifest67`) is written as soon as that seat's own steps pass: S1 after its repeat reads zero failures, S2 after its golden test. Both come before binid and BINTEST, which run last for all seats, so a later BINTEST abort on S1 leaves S1's manifest in the persisted run. Step 7 only checks that they exist, then writes `SESSION0.txt` with the `LD_REF_MANIFEST_<S>` lines. | p67pre:42-47, 127-131, 159, 195-209 | EXTENDS |
| 147 | An S2 failure in session 0 ends the session at that step: `seat_fail` prints `... (seat S2 is in the S2 family: guard 2 stops; relaunch with LD_DROP_S2=1 to run guard 1 alone)`, then FAILED-ALL, exit 5, the run persisted, no `SESSION0.txt`. No later step runs, S1's included. Guard 1 needs session 0 relaunched with `LD_DROP_S2=1`, which re-runs S1's steps and binid on B0 and S1. The failed run holds REF-MANIFEST-S1 only if S2 failed at the repeat or later, and binid shas only if it failed at BINTEST. A B0 or S1 failure ends the session the same way, with no relaunch hint. §5.2: "On S2, it stops guard 2. Guard 1 still launches, with S2 dropped." | p67:33-39; p67pre:68, 106-108, 133-136, 143-157, 180, 191 | EXTENDS |

### Stages G1 and G2

| # | decision | where | verdict |
|---|---|---|---|
| 33 | G runs plan 41d's battery with `LD_WITNESS67=g1` or `g2`. CAND and BASE are rebound to (S1, B0) and (S2, S1). `LD_BASE_SHA` is the BASE seat's commit pin. `LD_WITNESS54=0` and `LD_WITNESS64=0` are forced. | p67:172-186; plan67.py:81 | MATCHES |
| 34 | A G stage is ok only if the battery exits 0 and `ld.log` holds exactly 96 `P38ROW` lines, else ABORT-ROWS. | p67:22, 247-250 | EXTENDS |
| 35 | Replacements are run at the end of the stage (`place38_parse replacements41c`, cap 4), as plan 41d does. More than 4 voided slots means NO VERDICT and no replacement run. The replacement run needs 900 s left before `LD_HARD`, else ABORT-BUDGET. 41d's replaceable statuses are WITNESS-FAIL, CONTAMINATED, VOID-COTENANT, VOID-ONDISK and VOID-SHORT. | p67:229-244; place38_parse.py:1470-1471, 1509-1518 | MATCHES |
| 36 | G1 witnesses: S1 rows need `scw=` and `plx=` on `[DIAG]` and `rlaw=` on `[SPAN]`; B0 rows carry none of them. `rsat=` and `pfec=` are not required on G rows. | s67_score.py:1234-1239 | MATCHES |
| 37 | W-ROUTE (rr = 0) is checked on G1 rows too, not only on G2. It reads the maximum rr over all `[SPAN]` ticks, not the median. | s67_score.py:1245-1246 | EXTENDS |
| 38 | A failed G witness voids the row (it is replaced through 41d's rule), unless the row is a DNF. | s67_score.py:1249 | EXTENDS |
| 39 | `GATE-67G1`/`GATE-67G2` PASS needs GATE-41d PASS, no S06-FAIL row, no unreplaced witness failure and no G2-TAPER-NONZERO row outside S06-FAIL (#149). An abort token makes it FAIL ("UNSCOREABLE"). A truncated ledger reads TRUNCATED (#137). §3.1 says "G1 PASS iff p > 0.05", with the witnesses "outside the min-p family". | s67_score.py:1330-1348 | EXTENDS |
| 40 | In G2, an S2 (CAND) row of the right binary that prints `pslot=` and reads `taper > 0` or `plost > 0` is S06-FAIL, whatever its 41d label (infra, crash, WITNESS-FAIL) and whatever rr, and is never replaced. Stage A's mechanism precondition (pslot > 0, rr > 0) is not applied: G2 is Bulk, rr = 0 by design. A row with `taper > 0` and no `pslot=` fails the seat witness (WITNESS-FAIL, replaced once) and is G2-TAPER-NONZERO (#149). §4.1 registers `taper=0`; `plost` is added. | s67_score.py:1242, 1252-1270; place38_parse.py:1135 | EXTENDS |
| 41 | The "DNF where every S1 row completes" S06-FAIL rule, registered for Stage A, is also applied to G2 (over BASE rows that are LIVE). | s67_score.py:1308-1316 | EXTENDS |
| 42 | An S06-FAIL row in G enters score41d under the 41d label it would otherwise have had. A label in 41d's replaceable list reads as VOID-RC. | s67_score.py:1273-1282 | EXTENDS |
| 43 | Resolution41d is not computed on the VM. It is computed on the host from the pulled `ld.log`. | p67:257 | EXTENDS |
| 148 | G block 1: after the first 8 rows of plan 41c's order (one group: BASE and CAND at each of the four cells), the first row of each of the 8 (cell, arm) pairs must be LIVE with no G witness failure (not WITNESS-FAIL, a void or S06-FAIL). Else `FAILED-STAGE67`, battery exit 6, `FAILED-<stage>`, and the next stage runs. Not on the smoke or the replacement run. A DNF first row with a witness failure also fails it. §5.2 reads "every arm's first real row". | battery:879-892; s67_score.py:1012-1032; p67:228 | EXTENDS |
| 149 | G2-TAPER-NONZERO: every G2 S2 (CAND) row of the right binary that the stage ran, voided originals and their replacements alike, with `taper > 0` or `plost > 0` is printed, whatever its label. One that is not S06-FAIL (no `pslot=`) fails GATE-67G2, so a clean replacement never clears it. | s67_score.py:1252-1259, 1322-1325, 1340-1342 | EXTENDS |

### Stages A and A1

| # | decision | where | verdict |
|---|---|---|---|
| 44 | Row order: for each of 6 block indices b, the seed is 42 when b is even and 7 when b is odd. Inside a block index, the cells alternate (c2, then c8), each with (S1, S2, S1, S2, B0). S1 and S2 get reps 2k+1 and 2k+2, B0 gets rep k+1, with k = b // 2. | plan67.py:139-149 | EXTENDS |
| 45 | Instrument env on every plan-67 battery row: `RWM_RTOBS=1 RWM_RDIAG=1 RWM_DIAG=1 RWM_GEN=0`, `RWM_C_PIPELINE=window`. `RWM_DELTA`, `RWM_COPA_*`, `RWM_COMPLETION_EXPOSURE`, `RWM_MIN_R`, `RWM_CC_PACE` and `RWM_PERF_RATE_MBIT` are scrubbed before the arm env. | battery:178, 218, 570-577 | EXTENDS |
| 46 | ρ = 1 at Auto comes from the scrubbed `RWM_C_RELIABLE` (`perf_rwm_c.sh` passes `--window-reliable`). No row of A, the pool block or the dump twins checks `window_reliable: true`. | plan67.py:199-204; s67_score.py:372 | EXTENDS |
| 47 | A row runs once per attempt (`TRIES=1`): no bring-up retry. | battery:160 | EXTENDS |
| 48 | A void is replaced right after its row (inline), not at the end of the stage. The replacement's logs are `a11`/`a12`. | battery:1231-1241 | EXTENDS |
| 49 | Replaceable statuses: VOID-WITNESS, VOID-RHO, CONTAMINATED, VOID-COTENANT, VOID-ONDISK, WITNESS-FAIL, VOID-SHORT and NO_DATA. A crash (VOID-RC) is not replaced, and VOID-SEAT never is. §4.2 registers the replacement for a precondition-witness failure only. | plan67.py:232; s67_score.py:403-428 | EXTENDS |
| 50 | More than 4 voided slots in A (crashes and infra voids included) reads AS and A1 NO VERDICT. §4.2 states the cap but not the outcome above it. | s67_score.py:428, 709-710 | EXTENDS |
| 51 | Status precedence: a sha problem (VOID-SEAT), then S06-FAIL, then infra and crash voids, then VOID-WITNESS, then VOID-SHORT, then VOID-RHO (R1), then LIVE. An S2 row whose slot preconditions hold and that reads `taper > 0` or `plost > 0` is S06-FAIL whatever the infra or crash label, and is never replaced. | s67_score.py:14-16, 356-374 | FIXED on harness67c (s67_score.py:361-363) |
| 52 | The S06-FAIL outcome is read once `pslot > 0` holds, and in A also the `[SPAN]` median rr > 0. The other precondition witnesses (e.g. the gauge prints) are not required for it. | s67_score.py:307-318 | EXTENDS |
| 53 | A DNF row is never VOID-WITNESS for a precondition: those failures are dropped and it stays LIVE at its worst rank. A pool-echo failure still voids a DNF row. | s67_score.py:366-368 | EXTENDS |
| 54 | VOID-SHORT applies to A, pool and dump rows that completed with their last `[DIAG]` tick under 30 s (`place41b_score.MIN_TICK_S`). | s67_score.py:369-371; place41b_score.py:47 | EXTENDS |
| 55 | A missing `LD_SHA_<SEAT>` in the parser's env makes the row VOID-SEAT (`bin-sha-uncheckable`). | s67_score.py:258-264, 356-358 | EXTENDS |
| 56 | The precondition witnesses also void a row for `pslot=` printed on a seat outside the S2 family. | s67_score.py:276-277 | EXTENDS |
| 57 | Row readings: counters come from the last `[DIAG]` line (`cum=` gives src and cod), below-floor is the maximum `below_floor` over the receiver's `[RDIAG]` lines, and `fill_coded` comes from the `[RFA]` line with the largest `src_n`. | s67_score.py:155-168, 209-216 | EXTENDS |
| 58 | The UNSCOREABLE minimum counts LIVE rows only (a LIVE DNF counts; an S06-FAIL row counts through its underlying label). Crash rows (VOID-RC, NO_DATA after replacement) and other voids do not count; they are counted apart (#151, #152). | s67_score.py:474-477, 688-693 | FIXED on harness67c (s67_score.py:474-477) |
| 59 | A1's minimum reads only the arms of its test: at least 10 live S1 rows and 4 live B0 rows (`MIN_LIVE_B0`) per cell. An S2 shortfall makes AS UNSCOREABLE and leaves A1 scored; A1's rank test reads B0 against S1, stratified within seed per cell. §4.2: "fewer than 10 live rows on S1 or S2 after replacement is UNSCOREABLE for AS and A1". The B0 minimum is not registered. | plan67.py:59; s67_score.py:20-22, 679-693 | CONTRADICTS |
| 60 | Verdict precedence for AS and A1: an abort token, then the minimum (CANDIDATE-CRASH first, #152), then the replacement cap, then the test, then truncation (#137). Any 41d abort token (ABORT-LOCK, -CRLF, -SHA, -BIN, -SENTINEL-UNWRITABLE, -SMOKE) in the ledger reads UNSCOREABLE. | s67_score.py:699-711, 727, 748; place38_parse.py:88 | EXTENDS |
| 61 | An S06-FAIL row is scored in AS and A1 under the label it would otherwise have had. A replaceable or infra label reads as a crash (VOID-RC, worst rank). | s67_score.py:431-439, 667 | EXTENDS |
| 62 | A crash row (VOID-RC, NO_DATA) fills a worst rank in the rank statistic (`with_crash=True`). | s67_score.py:527 | EXTENDS |
| 63 | AS's null is the guard's stratified Monte Carlo (10 000 relabellings, seed 4141). §9 says "exact null at small n". | s67_score.py:713-726 | EXTENDS |
| 64 | A1's exact null is stratified within seed (per cell C(9,3)² = 7 056 labellings) and combines the cells as 1 − Π(1 − F_c). It is not the unstratified C(18,6). It falls back to Monte Carlo above 200 000 labellings per cell. | s67_score.py:51, 523-554, 731-747 | EXTENDS |
| 65 | AS prints its realised d80 as the q95 of the S1 half-splits per seed. | s67_score.py:721-723 | EXTENDS |
| 66 | Below-floor flag: H-B's level is 854 at c2 (registered) and 122 at c8, taken from `docs/l1-raw/s51b/recopy/acct-c8.txt`. The cell flag compares the S2 median with 0.25 × the S1 median and with the median `fill_coded` + 2 × H-B. The per-row flag uses the row's own `fill_coded`. | s67_score.py:47, 563-578 | EXTENDS |
| 67 | The CPU owner flag: "worse" is the relative difference of the per-GB medians (DNF rows excluded) above d80, AND a one-clause rank p ≤ 0.05 for that cell from the guard's Monte Carlo. | s67_score.py:619-629 | EXTENDS |
| 68 | The c8 in-order delay is printed as the relative median difference with no test. §4.2 says "two-sided". The c2 in-order delay is printed per arm only, with no S2-against-S1 line. | s67_score.py:606-607, 614-618 | EXTENDS |
| 69 | `[SUCC]`: the last `[SUCC]` line on the server; `rep_*` is read as decode-resolved and `orig_*` as resend-resolved. A row has "both close times" if `rep_p50_us` and `orig_p50_us` are present (p90 is not required). | s67_score.py:201-206 | EXTENDS |
| 70 | `[DIAG] retx` and `[RFA] fill_coded` are printed as raw per-arm medians, not per source. | s67_score.py:609 | EXTENDS |
| 71 | Rate readouts: the shares are pooled over all `rlaw=` ticks. The rsat/rr/pfec quantiles pool at most 400 strided samples per row. β is the median of the per-row medians. The r_sat bind fraction is the share of ticks whose anchor-term bound is `sat`. | s67_score.py:74-84, 181-198, 581-596 | EXTENDS |
| 72 | Cost split: consumed r = (cod + taper) / src, coded share = cod / (src + cod), taper share = taper / (taper + cod), each from the last `[DIAG]` cum and taper. | s67_score.py:162-165, 635-638 | EXTENDS |
| 73 | t_tail is printed as the constant 1·10⁻⁵, labelled DERIVED. | s67_score.py:50, 596 | MATCHES |
| 151 | MINIMUM-COUNTS: when any S1, S2 or B0 cell is under its minimum, Stage A prints one `MINIMUM-COUNTS` line with the live count (a LIVE DNF counts) and the crash count (VOID-RC and NO_DATA after replacement) per arm and cell (S1, S2, B0; S1 and B0 under the drop). Each UNSCOREABLE-minimum verdict names its short cells as `<cell> <arm> <live><<min> (crashes <k>)`. | s67_score.py:457-464, 688-697, 708 | EXTENDS |
| 152 | The candidate-crash rule. AS's candidate arm is S2, A1's is S1, R2-HOLD's is P2T. When a candidate cell (R2: the twin) is under its minimum but its live plus crash rows reach it, the verdict reads `<AS or A1> UNSCOREABLE (CANDIDATE-CRASH <arm> n=<k> in <cell>: a finding to investigate, not cleared by a plain re-run)`, with any other short cells after `; also under the minimum:`. R2: `R2-HOLD UNSCOREABLE (CANDIDATE-CRASH P2T n=<k>: a finding to investigate, not cleared by a plain re-run); guard 2 NOT ELIGIBLE`, after an abort token, R2-BLIND and the replacement cap; a P1T shortfall alone is plain UNSCOREABLE naming the live and crash counts. The code's docstring carries the sentence "An UNSCOREABLE caused by candidate-arm crashes is a finding to investigate, not something a plain re-run clears." R1 has no live minimum, so the rule does not apply there. | s67_score.py:18-23, 679-683, 699-708, 868-881 | EXTENDS |

### Stage R1

| # | decision | where | verdict |
|---|---|---|---|
| 74 | Order and arms as A at c2 only (30 rows), Realtime, 100 MB, 150 s, ρ = 1 witnessed by `window_reliable: true` on both ends (VOID-RHO). A Realtime row that lacks the reliable-window line is not voided for it (plan 59's rule). | plan67.py:67, 182-183, 205-206; s67_score.py:329-333, 372-374 | MATCHES |
| 75 | The R1 replacement cap is 2 slots (plan 64's stage R). More than that reads NO VERDICT. | plan67.py:62, 227-228; s67_score.py:821 | EXTENDS |
| 76 | A B0 R1 row carrying a gauge print is not voided: it stays LIVE and fails clause A1-R ("no B0 row does"). | s67_score.py:271, 783 | FIXED on harness67c (s67_score.py:271, 783) |
| 77 | "Completes" means status LIVE or S06-FAIL and no DNF. Unreplaced voids (VOID-RHO, crashes) count as not completing. So an S1 void can fail A1-R, and an S2 VOID-RHO or crash row with its preconditions met can fail R1. | s67_score.py:779-796 | EXTENDS |
| 78 | §4.3 registers the S1 `taper > 0` witness, but the code never voids an S1 row for it. R1-BLIND is `taper = 0` (a missing value counts as 0) on more than half of all S1 rows, voids included. | s67_score.py:790-791 | EXTENDS |
| 79 | An S2 row with `pslot = 0` or no `pslot=` is VOID-WITNESS. An S2 row with `pslot > 0` and `taper > 0` or `plost > 0` is S06-FAIL (`plost` is added to §4.3's `taper = 0`). | s67_score.py:274-275, 288-289, 307-318 | EXTENDS |
| 80 | Verdict precedence: an abort token, no S1 or S2 row, INCOMPLETE (#138) (each UNSCOREABLE), the replacement cap (NO VERDICT), then R1 FAIL, R1-BLIND, R1 PASS, then truncation (#137). A blind stage reads R1-BLIND even when the clause holds. There is no live minimum. | s67_score.py:810-825 | EXTENDS |
| 81 | R1 reports per arm goodput, `cpu_cli/GB`, `cpu_srv/GB`, `lag_E_cli`, `lag_E_srv` and the in-order delay mean and p95, marked under the 30 s tick floor. | s67_score.py:801-808 | FIXED on harness67c (s67_score.py:801-808) |
| 138 | INCOMPLETE: A1-R and R1 read `UNSCOREABLE (INCOMPLETE: <k> of <n> registered slots hold no row: ...)` when a registered slot has no row. The registered slots follow the ledger's drop state (#153). It comes after an abort token and the no-row check, before the replacement cap. Stage A has no such check: its missing slots fall to the minimum and to the stage's `check` (#113). | s67_score.py:776-778, 814-822 | EXTENDS |

### Stage R2 and the twins

| # | decision | where | verdict |
|---|---|---|---|
| 82 | R2 rows take plan 51e's PM spec: 262 500 000 bytes, timeout 75 s, `RWM_SLAW=1 RWM_C_RELIABLE=0 RWM_PERF_RATE_MBIT=35`, Realtime. | plan67.py:207-209; plan51e.py:34-37, 89-110 | EXTENDS |
| 83 | Order: 6 groups; group g has seed 42 when g is even and 7 when odd, rep g // 2 + 1. Each group runs (P1, P2, P1T, P2T), reversed on odd groups to (P2, P1, P2T, P1T). | plan67.py:152-159 | EXTENDS |
| 84 | The pacer seats' rows run `perf_rwm_c.sh` from that seat's own `tools/l1` (`LD_TOOLS_<SEAT>`). | p67:101, 189; battery:422, 569 | EXTENDS |
| 85 | Every R2 row needs the pacer echo, the client gate at 35, the server gate unset or 35, and a `"pace"` JSON line, else VOID-WITNESS. P1 `taper > 0` and P2 `pslot > 0` with rr > 0 are checked on the smoke row and, as R2-BLIND (#139), on the scored rows; they never void a row. | s67_score.py:290-303, 846-853, 977-980 | EXTENDS |
| 86 | Twin witness: §4.4 says "a row with neither is VOID". The code voids a twin row when either line is missing. | s67_score.py:293-294 | EXTENDS |
| 87 | The R2 replacement cap is 4 slots. More than that reads R2-HOLD UNSCOREABLE, naming the live and crash twin counts and the cap. | plan67.py:227-228; s67_score.py:879-880 | EXTENDS |
| 88 | The battery keeps one `ACCT-KEPT` line per kind in the ledger: the last periodic `[ACCT]`, the last `final=1` `[ACCT]` and the last `[ACCT-RX]`. The scorer reads the account from the raw endpoint log at row time; the kept lines serve only the rehearsal's kept-log check. | battery:590-595; s67_score.py:129-139, 222-233; plan67.py:274-297 | EXTENDS |
| 89 | U = O_serv + O_strand. A missing field leaves U unread (None), never 0. | s67_score.py:226-227 | EXTENDS |
| 90 | A twin row counts toward the minimum of 5 only if it is LIVE and has an `[ACCT]` reading. The `LIVE TWIN ROWS` line prints each twin's live and crash counts. `final=1` and `ACCT-LAST` readings go into one rank test; a NOTE is printed when they are mixed. | s67_score.py:856-869 | EXTENDS |
| 91 | If a counted twin row lacks U or `oldest_unchanged_max`, R2-HOLD reads UNSCOREABLE and guard 2 NOT ELIGIBLE ("never read as 'did not fire'"). | s67_score.py:871, 881-882 | MATCHES |
| 92 | `shed_total(P2) − shed_total(P1)` is computed per P2 row against the median of the P1 rows of the same seed (and the same for the twins). | s67_score.py:901-908 | EXTENDS |
| 139 | R2-BLIND: on the scored R2 rows, P1 needs `taper > 0`, and P2 needs `pslot > 0` with the `[SPAN]` median rr > 0 (§4.4's smoke witness). More than half of a seat's rows failing it prints `R2-BLIND <seat>`, and R2-HOLD reads `UNSCOREABLE (R2 VOID: the mechanism did not run; guard 2 waits for an R2 re-run; blind seat <S>); guard 2 NOT ELIGIBLE`. It comes after an abort token and before the twin minimum. | s67_score.py:846-853, 872-875 | EXTENDS |

### Stage T

| # | decision | where | verdict |
|---|---|---|---|
| 93 | Transfer order: hint auto then realtime; inside a hint, B0 then S1. Each invocation uses `tun_bulk.sh c2 <hint> 100000000 5`, seed 42, `TB_RUST_LOG=info`, and a comment-only default TOML. | tun67_battery.sh:37, 56-75 | MATCHES |
| 94 | Inherited `tun_bulk.sh` defaults: 3 bring-up attempts, each gated by up to 20 pings (1 s wait plus 1 s sleep, so about 40 s), a 90 s timeout per transfer (`RWM_TB_TIMEOUT_S`). | tun_bulk.sh:37, 70-93 | EXTENDS |
| 95 | Refusal order: S1 first, then B0; for each key, `check` then `run`. `check` has a 20 s timeout. `run` is a throwaway server on `127.0.0.1:7999`, TUN `rptref0` `10.98.0.1/24`, `--protocol-hint auto`, killed after 8 s. Both run with `RUST_LOG=info`. | tun67_battery.sh:79-92 | EXTENDS |
| 96 | Parsing: `cfg` is the key's value on the last `resolved configuration` line; `err` is the first non-empty stderr line, ANSI-stripped. The refusal text is compared with that one line, after an optional leading `Error: ` is removed. | tun67_battery.sh:93-94; s67_score.py:1123-1127 | EXTENDS |
| 97 | B0's four `check` commands are run and printed, never scored. | tun67_battery.sh:9; s67_score.py:1114 | EXTENDS |
| 98 | A B0 `run` that prints the guard text stops the stage at once (ABORT-SEAT, exit 5). | tun67_battery.sh:96-99; s67_score.py:1113, 1175-1176 | MATCHES |
| 99 | A transfer "completes" if its `TUNREP` JSON has `mbps` or `seconds` and no `nodata`. | s67_score.py:1083 | EXTENDS |
| 100 | Verdict precedence: ABORT-SEAT, then clause 1 (T FAIL), then ABORT-BRINGUP-T (#150), then no `T67DONE` (T FAIL), then clauses 3 and 4 (T FAIL), then clause 2 (T VOID), then PASS (TRUNCATED on a cut ledger). A finding outranks a void. | s67_score.py:1174-1188 | EXTENDS |
| 101 | `TUN-BRINGUP-FAIL` is read: a bring-up failure on either seat gives `ABORT-BRINGUP-T seat=<S> hint=<h>` as the T result, and the envelope does not count the stage ok. | s67_score.py:1046-1092, 1109-1111, 1179-1180; p67:252; tun67_battery.sh:14-16 | FIXED on harness67c (s67_score.py:1109-1111, 1179-1180; p67:252) |
| 102 | T has no replacement. It is ok if the battery exits 0, the ledger has `T67DONE` and no `TUN-BRINGUP-FAIL`. | p67:251-252 | EXTENDS |
| 150 | Bring-up attribution: a `TUN-BRINGUP-FAIL` line is attributed by its own `label=<seat>-<hint>`, else to the invocation of the last `T67SEAT` line, else read as `ABORT-BRINGUP-T unattributed (...)`; it is never dropped. A ledger without `T67DONE` reads `T FAIL (no T67DONE)` unless ABORT-SEAT, clause 1 or a bring-up abort comes first (#100). | s67_score.py:1046-1092, 1103-1111, 1181-1182; tun_bulk.sh:92 | EXTENDS |

### Pool block and dump twins

| # | decision | where | verdict |
|---|---|---|---|
| 103 | Pool order: rep 1 to 3; inside a rep, pools 170, 256, 640; inside a pool, (S1, S2) on odd reps and (S2, S1) on even reps. Arms are named `S1P<pool>` and `S2P<pool>`. | plan67.py:162-170 | EXTENDS |
| 104 | Pool and dump rows take A's precondition witnesses (the four gauge prints on S1 and S2, none on B0, `pslot=` only on S2) and are replaced at cap 4. Pool rows also need `RWM_STORE_PATH_POOL` echoed on both ends. | s67_score.py:270-279, 345-351; plan67.py:227-228 | EXTENDS |
| 105 | Pool readouts: completion is the row's wall time, or 200 s for a DNF; duplicates are the `[C8CONV-R]` `dup=` sum and `[RFA] dup_src`. | s67_score.py:217-218, 916-935 | EXTENDS |
| 106 | Dump twins: 8 rows ordered cell, then seed, then seat (arms `S1D`, `S2D`), A's spec plus `RWM_SUCC_DUMP=1`. They report the `[SUCCDUMP]` record counts, with no minimum record count. | plan67.py:173-175, 212-213; s67_score.py:938-945 | EXTENDS |
| 107 | The dump twins run only with `LD_DUMP67=1`, set by hand. The rehearsal writes its advice (`DUMP67=1` or `0`) into `AMENDMENT2.txt`, from `DUMP-TWINS-NEEDED` in `sa/smoke-check.txt`. | plan67.py:103-104, 114; p67:273 | MATCHES |
| 108 | `LD_CUT67` accepts only `dump`; any other value is ABORT-CUT. | p67:27 | MATCHES |

### Stage control and the block-1 rule

| # | decision | where | verdict |
|---|---|---|---|
| 109 | Block-1 sizes: G = 8 rows (#148), A = 10 rows (block index 0, both cells), R1 = 5, R2 = 4 (group 0), pool = 6 (rep 1). The dump twins have none. | plan67.py:221-224; s67_score.py:1012 | EXTENDS |
| 110 | Block-1 (A, R1, R2, pool) takes each arm's first row in the ledger. A first row that was void and then replaced (inline, #48) still fails the rule, and so does a first-row S06-FAIL. For A, the c8 cell's first rows are not checked. (G reads each (cell, arm) pair, #148.) | s67_score.py:990-1009 | EXTENDS |
| 111 | G1 and G2 now run block 1 (#148). T and the dump twins have none. §5.2: "after each stage's first block, every arm's first real row must be LIVE". | battery:879-892, 1249-1258; tun67_battery.sh:65-102 | CONTRADICTS |
| 112 | A stage battery starts only with 1 800 s left before `LD_HARD`; otherwise it is not started: `ABORT-BUDGET stage <s>: ... not started`, a `NOT-STARTED` marker, stage return 8 (#154). Its timeout is the time left to `LD_HARD`. A battery killed at `LD_HARD` gets the truncation sentinel (#136) and is scored as far as it got. | p67:219-228 | EXTENDS |
| 113 | A, R1, R2, pool and dump are ok only if `LD-BATTERY-DONE` is present and the set of slots equals the table. | p67:253-254; plan67.py:263-271 | EXTENDS |
| 114 | Every stage runs even after an earlier one failed. A stage that is not ok, other than cut or not started, ends the session `FAILED-ALL` ("stages not run to the end", naming any cut stages too), after the summary `score.txt` is written. Cut or not-started stages alone end it TRUNCATED-ALL (#154). | p67:280-296 | EXTENDS |
| 115 | `LD_G2_FAIL67=1` sets `LD_DROP_S2=1` and removes P1, P2, P1T and P2T, R2, the pool block and the dump twins; A, R1 and T keep their S1 and B0 arms. §5.2 drops "Stage A's S2 arm, R1's S2 arm and R2". The pool block and the dump twins also lose their S1 arms. | p67:24-25, 32; plan67.py:115-116 | EXTENDS |
| 116 | A seat failure in the S2 family (S2, P2, P2T) aborts the whole session with "relaunch with LD_DROP_S2=1", session 0 included (#147). Guard 1 alone then needs a relaunch; it does not go on in the same session. | p67:33-39 | EXTENDS |

### Rehearsal and stub runs

| # | decision | where | verdict |
|---|---|---|---|
| 117 | Plan 67 needs `LD_REHEARSE=1`, and only plan 67 accepts it (67pre, 67g and 67x refuse it). | p67:23; run_all:190-191 | EXTENDS |
| 118 | Rehearsal stages: g1, g2, a, r1, t, r2. The pool block and the dump twins get no smoke. | plan67.py:80 | MATCHES |
| 119 | G's smoke runs twice (g1, g2), each with plan 41d's three smoke rows (c1s BASE, c1s CAND, c8 600 MB CAND). That is six rows: c1s B0, c1s S1, c8 S1, c1s S1, c1s S2, c8 S2. §6.1 lists three. | s67_score.py:1362; place38_parse.py:73 | EXTENDS |
| 120 | The smoke rows are A: one c8 row per seat; R1: one c2 row per seat; R2: one row per pacer seat and twin; all at seed 42, rep 1, in the order S1, S2, B0 (A, R1) and P1, P2, P1T, P2T (R2). | plan67.py:178-185 | MATCHES |
| 121 | A rehearsal smoke runs under `timeout` at min(1 800 s, the time left to `LD_HARD`). No time left, or a timeout, is ABORT-SMOKE with the reason. | p67:201-206 | EXTENDS |
| 122 | A smoke row fails unless it is LIVE with its preconditions met. So a failed S2 outcome witness (S06-FAIL) fails the smoke and gives ABORT-SMOKE. | s67_score.py:968 | EXTENDS |
| 123 | The `[SUCC]` close-time check in the A smoke is INFO: missing close times print DUMP-TWINS-NEEDED, and the smoke does not fail. | s67_score.py:970-972 | EXTENDS |
| 124 | The rehearsal stops at the first smoke that fails (`LD-ALL FAILED`). No later smoke runs, and no `AMENDMENT2.txt` is written. | p67:265-269 | EXTENDS |
| 125 | The R2 smoke adds the kept-log check: every twin row needs `ACCT-KEPT side=cli` with `[ACCT]` and `side=srv` with `[ACCT-RX]` in the ledger. | p67:211; plan67.py:274-297 | MATCHES |
| 126 | §4.4: "A failure [of the R2 smoke witness] makes R2 VOID; it goes back to the coordinator". In the code, the R2 smoke runs only in the rehearsal; a failure there is ABORT-SMOKE and fails the whole rehearsal. 67x runs R2 with no smoke before it; it reads the same witness on the scored rows afterwards (R2-BLIND, #139), which makes R2 VOID. | p67:199-218, 265-269; s67_score.py:846-853, 874-875 | CONTRADICTS |
| 127 | `AMENDMENT2.txt` holds the seven seats' binary shas with their commits, the dump-twin advice, the S1/S2 suite-reuse HIT list and the sha of `TOOLS-MANIFEST.sha256`. The binid shas and repeat counts (§6.1) come from session 0's `SESSION0.txt`. | p67:270-276; p67pre:200-209 | EXTENDS |
| 128 | Stub fast mode (`test_stubrun.py`) shrinks G_ROWS67 to 16, N_BLOCKS to 2, N_TWIN_REPS to 2, N_POOL_REPS to 1, MIN_LIVE_A to 2, MIN_LIVE_B0 to 1 and MIN_TWIN to 2. `STUBRUN_FULL=1` keeps the registered values. Other test seams: `LD_HARD_MIN_S`, `R67_T1_S`, `LD_ROW_BYTES`, `LD_BLOCK_ROW_S`, `R67_N`, `R67_BUDGET_S`, `R67_SIM`. Every one of them is ABORT-SEAM without `LD_TEST_SEAMS=1` (#140). | test_stubrun.py:268-274; plan67.py:70-72, 255-260; repeat67_run.sh:66 | EXTENDS |

### Aborts and exit strings

| # | decision | where | verdict |
|---|---|---|---|
| 129 | Tokens beyond §5.4's list: ABORT-SEAM, ABORT-REHEARSE, ABORT-CUT, ABORT-SHM, ABORT-SUITES, ABORT-BUDGET, ABORT-ROWS, ABORT-SHA, ABORT-BIN, ABORT-PLAN67, ABORT-PRECONDITION, ABORT-BRINGUP-T, `FAILED-<stage>` / FAILED-STAGE67, BUSY, BINID67 FAIL; the session outcome TRUNCATED-ALL and the stage marker NOT-STARTED; and the verdict tokens TRUNCATED, INCOMPLETE, R2-BLIND, CANDIDATE-CRASH, G2-TAPER-NONZERO. Findings that do not abort: GATES-B0-FINDING, GATES-FINDING-ON-MAIN, BINTEST-FINDING-ON-MAIN. ABORT-PREFLIGHT, listed in §5.4, is now emitted by session 0 (#141-#145). | run_all:146; p67:23-27, 66, 144, 220, 250, 295; p67pre:27, 31, 81, 109, 185, 192; battery:296, 305, 888, 1248, 1256; tun67_battery.sh:52-53; s67_score.py:706, 820, 853, 877, 1109-1111, 1324 | EXTENDS |
| 130 | Order in practice: ABORT-SEAM fires first, before the locks. Then ABORT-PRECONDITION, the session budget (ABORT-BUDGET), and ABORT-TESTS from the tools-test gate (#20), before any seat check. The seat checks then go commit pin (ABORT-SEAT); in 67pre the session-0 steps follow (#141); otherwise build and sha (ABORT-BUILD), identity strings and the tools-diff witness (ABORT-SEAT), suite reuse (ABORT-SUITES), then the tools tests again (ABORT-TESTS). §5.4 orders ABORT-SEAT before ABORT-BUILD before ABORT-TESTS. | run_all:144-147, 351-366, 391-397, 455-480; p67:51-161 | EXTENDS |
| 131 | Exit strings: `LD-ALL-SESSION0-DONE` (exit 0), `LD-ALL-REHEARSAL-DONE` (exit 0), `LD-ALL-DONE` with `DONE-ALL` (exit 0), `LD-ALL-TRUNCATED` with `TRUNCATED-ALL` (exit 7, #154), `LD-ALL FAILED: <cause>` with `FAILED-ALL` (exit 5). Stage markers: `ABORT-SMOKE`, `SMOKE-PASS`, `REHEARSAL-PASS`, `TRUNCATED`, `NOT-STARTED`, `FAILED-<stage>`, `DONE`. A battery's exit 6 is the block-1 stop. | p67:202-218, 220, 226-228, 258-259, 278, 293-296; p67pre:211-212; run_all:336, 343 | EXTENDS |

### Scoring, parsers and env (across stages)

| # | decision | where | verdict |
|---|---|---|---|
| 132 | Stage T is parsed by `s67_score.py`'s own `tun_parse`, not by `verify4_parse.py tun`. §9 item 5: "parsed with `verify4_parse.py tun`". The parser now reads `TUN-BRINGUP-FAIL` (#101), and a test checks that it and `verify4_parse.tun_arms` agree on a clean, a no-data and a bring-up-failure ledger. | s67_score.py:1046-1092; test_s67.py:804-819 | CONTRADICTS |
| 133 | The copy and slot counters and the below-floor count are re-implemented in `readings()`. §9 item 2 says "as `s51_score.py` reads them" and "as `acct.py` reads it"; neither module is imported. In-order delay does use `threadp1_parse.inorder_delay_stats`, as §9 says. | s67_score.py:39, 142-250, 334 | EXTENDS |
| 134 | Envelope to battery env (`sudo env`): `LD_OUTDIR`, `LD_HARD`, `LD_REHEARSE`, `LD_DROP_S2`, `LD_DUMP67`, `LD_WITNESS54=0`, `LD_WITNESS64=0`, plus `LD_BIN`/`LD_SHA` (S1's for every non-G stage), `LD_PLAN`, `LD_SEATS67`, `LD_BIN_<S>`, `LD_SHA_<S>` and `LD_TOOLS_<S>`. `LD_BLOCK_ROW_S` and `LD_ROW_BYTES` pass only under `LD_TEST_SEAMS=1`. For G: CAND and BASE binaries and shas, `LD_SHA_G_CAND`, `LD_SHA_G_BASE`, `LD_BASE_SHA`, `LD_PLAN=41d`, `LD_WITNESS67`. T gets a reduced list: `LD_OUTDIR`, B0's and S1's binary and sha, `LD_HARD`, `LD_DROP_S2`. | p67:182-198 | EXTENDS |
| 135 | Launches (67g, 67x) run no smoke before a stage's battery. Only the rehearsal smokes, so each launch rests on the rehearsal's smoke of the same binaries. | p67:199-218 | EXTENDS |
| 136 | The truncation sentinel: a battery, or G's replacement run, killed by `timeout` at `LD_HARD` (rc 124) gets `LD-BATTERY-TRUNCATED at LD_HARD <epoch>` appended to its ledger and a `TRUNCATED` marker file. The stage is still scored, then returns 7 (#154). A G replacement run with under 900 s left is not started and only logged (ABORT-BUDGET); it gives no 7 or 8. | p67:225-227, 235-243, 258; s67_score.py:91-93 | EXTENDS |
| 137 | Truncation rewrites verdicts: a verdict whose text before ` (` carries PASS, CLEAR or ELIGIBLE (not `NOT ELIGIBLE`) reads `<name> TRUNCATED (...)`; a finding or a void keeps its label. Applied to AS, A1, A1-R, R1, R2-HOLD and T. `R1-BLIND` on a cut ledger reads `R1 TRUNCATED (R1-BLIND on a battery cut at LD_HARD ...)`. G prints TRUNCATED on score41d's GATE-41d verdict lines and on `GATE-67G<n>`, and `score67g`'s returned result carries `truncated` and gate TRUNCATED. The pool block and the dump twins print `VERDICT-67O` / `VERDICT-67D TRUNCATED`. A truncated stage never reads PASS. | s67_score.py:96-108, 727, 748, 825-827, 909, 935, 945, 1188, 1287-1292, 1344-1348 | EXTENDS |
| 140 | ABORT-SEAM: a plan-67 session (67pre, 67, 67g, 67x) with an active test seam and no `LD_TEST_SEAMS=1` prints `ABORT-SEAM plan <p>: test seam(s) active without LD_TEST_SEAMS=1: <list>; NOTHING WAS RUN` and exits 3, before the locks (no FAILED-ALL). The seams are any non-empty variable of `SEAM_ENV` (`LD_HARD_MIN_S`, `R67_T1_S`, `LD_ROW_BYTES`, `LD_BLOCK_ROW_S`, `R67_N`, `R67_BUDGET_S`, `R67_SIM`, and the stub-sandbox paths `LD_DEVSHM`, `LD_PROCFS`, `LD_LOCK_VM`, `LD_LOCK_RP`, `LD_TMPD`), and any of N_BLOCKS, N_TWIN_REPS, N_POOL_REPS, MIN_LIVE_A, MIN_LIVE_B0, MIN_TWIN or G_ROWS67 (read from `loopdiag_p67.sh`) off its registered value. Without the marker, session 0 also unsets the `R67_*` seams for its children. | plan67.py:71-74, 255-260; run_all:141-147, 185; battery:93-95; tun67_battery.sh:26-27; p67pre:38-40 | EXTENDS |
| 153 | The S2-drop state rides in the ledger. The battery writes `LD-PLAN67 plan=<p> drop_s2=<0 or 1>` on the A, R1, R2, pool and dump ledgers and `LD-PLAN67 plan=41d stage=<g1 or g2> drop_s2=<0 or 1>` on G's; T's battery writes `LD-PLAN67 plan=t drop_s2=<0 or 1>`, and the envelope forwards `LD_DROP_S2` to T. There is no `dump=` field. The scorer reads `drop_s2` from the ledger, and falls back to `LD_DROP_S2` in the environment when no ledger carries the line. It sets R1's registered slots (INCOMPLETE, #138) and the arms of the MINIMUM-COUNTS line; the G and T scorers do not read it. `plan67.py check` (through its tables) still reads the environment. | battery:98, 162, 648-649; tun67_battery.sh:62; p67:198; s67_score.py:24, 467-471, 684-697, 769, 776-778; plan67.py:84-100 | EXTENDS |
| 154 | TRUNCATED-ALL: a stage cut at `LD_HARD` returns 7 (TRUNCATED, scored); a stage not started for lack of time returns 8 (ABORT-BUDGET, NOT-STARTED). Neither counts as a failure. If they are the only not-ok stages, the session ends TRUNCATED-ALL: a `TRUNCATED-ALL` sentinel (proved writable at launch), the line `LD-ALL-TRUNCATED`, exit 7 (default, awaiting owner confirmation), no DONE-ALL and no FAILED-ALL. Otherwise it ends FAILED-ALL, naming the cut stages too. A block-1 stop (6) is a failure. Session 0 and the rehearsal have no such path: their budget aborts stay ABORT-BUDGET / ABORT-SMOKE through FAILED-ALL. | p67:164-166, 220, 258, 280-296; run_all:336 | EXTENDS |

### Contradictions

**Still contradicting at `3092dc96`.** Each is resolved by a proposed text change, not a code fix:
1. **#12, tools-diff scope.** §1.3: "`git diff B0 S1 -- tools` and `git diff S1 S2 -- tools` are empty". The code compares `tools/l1` only. Amended: Proposed text #12 (§1.3).
2. **#111, block-1 coverage.** G1 and G2 now run block 1 (battery:879-892). T and the dump twins still have none. Amended: Proposed text #111 (§5.2).
3. **#126, R2 smoke failure.** §4.4: "A failure makes R2 VOID; it goes back to the coordinator". The rehearsal's R2 smoke failure is ABORT-SMOKE; in 67x the witness is read on the scored rows (R2-BLIND). Amended: Proposed text #126 (§4.4).
4. **#132, the T parser.** §9 item 5: "parsed with `verify4_parse.py tun`". The code uses its own parser; it now reads the bring-up lines, and test_s67.py:804-819 checks it against `verify4_parse`. Amended: Proposed text #132 (§9 item 5).
5. **#59, A1's minimum.** §4.2: "fewer than 10 live rows on S1 or S2 after replacement is UNSCOREABLE for AS and A1". A1 now reads only S1's and B0's minimums (s67_score.py:693). Amended: Proposed text #59 (§4.2).
6. **#141, session 0's envelope.** §6.0: "`suites_run.sh` envelope". Session 0 runs as `LD_PLAN=67pre` through `loopdiag_run_all.sh`. Amended: Proposed text #141 (§2.2, §6.0).

**Former contradictions, fixed on harness67c:**
- **#51, S06-FAIL precedence:** S06-FAIL now sits above the infra and crash labels (s67_score.py:361-363).
- **#58, the live-row minimum:** LIVE rows only (s67_score.py:474-477); crashes are counted apart (#151) and read through the candidate-crash rule (#152).
- **#76, B0 gauge prints in R1:** the row stays LIVE and fails A1-R (s67_score.py:271, 783).
- **#81, R1 readouts:** lag and `cpu_srv/GB` are printed (s67_score.py:801-808).
- **#101, T bring-up:** ABORT-BRINGUP-T, and the stage is not ok (s67_score.py:1109-1111, 1179-1180; p67:252).

### Open for the owner and the coordinator

- **A1 and S2's minimum: resolved at `3092dc96`.** A1 is UNSCOREABLE only on an S1 or B0 shortfall (s67_score.py:693); the text follows in Proposed text #59.
- **Pin strength (#4).** Pins are a prefix match, so an 8-hex pin is weak; full shas would be safer. The stale defaults are gone (#1).
- **Mixed account labels (#90).** `final=1` and `ACCT-LAST` readings share one rank test. Should a mixed set be tested apart or flagged?
- **R1 void rows (#77, #78).** An S2 VOID-RHO or crash row can fail R1, and void S1 rows count toward R1-BLIND. Should "rows whose witnesses hold" exclude every void?
- **Block-1 on a replaced first row (#110).** It stops the stage even when the replacement is LIVE. Is "first real row" the replacement? A first-row S06-FAIL also stops stage A, which drops the rest of A1.
- **S06-FAIL in the rehearsal (#122, #124).** §6.1 calls it "a finding", but it aborts the rehearsal. Should the smoke record it and go on?
- **G2 FAIL drops the pool block (#115).** Its S1 arms are lever B's trigger too, and §5.2 does not drop them.
- **T2's sim port (#29).** `R67_SIM` does not name the T2 port's target. If the port lives in `s06_reactive`, it runs N times with no two-run digest check (§2.1 item 3, §2.2).
- **Parser equivalence (#133).** The T parser is now checked against `verify4_parse` (#132). Nothing shows that `readings()` matches `s51_score.py`'s counters and `acct.py`'s below-floor count. A fixture comparison on one pulled ledger would settle it.
- **No launch smoke (#135).** Measurement-discipline protocol item 4: "A short smoke on the same binary must witness every required gauge and echo on both endpoints; if it fails, nothing is launched." The block-1 rule is the only in-launch check. Is the rehearsal's smoke enough?
- **The twin "neither" wording (#86).** §4.4 says "a row with neither is VOID", but its witness needs both lines. The code reads it as "not both". The text should say which.
- **Session 0 after an S2 failure (#147).** The session stops at the failing step, and guard 1 needs a full session-0 relaunch with `LD_DROP_S2=1`, about half the session again. §6.4's worst case does not count it. Is that the intent, or should the session drop S2 and go on?
- **REF-MANIFEST before binid (#146).** S1's manifest is written before its BINTEST. A later BINTEST abort on S1 leaves a manifest for a seat that failed session 0. Should it wait for binid?
- **TRUNCATED-ALL with an outcome label (#154).** A cut stage returns 7 whatever its verdicts (`loopdiag_p67.sh:258` runs before the ok check), so a cut session whose scored rows hold an S06-FAIL or a candidate crash ends TRUNCATED-ALL, exit 7. The verdict lines still read FAIL / S06-FAIL / UNSCOREABLE: truncation rewrites only PASS, CLEAR and ELIGIBLE (s67_score.py:1344). The session exit never carries verdicts in this harness: a complete stage with an S06-FAIL returns 0 and the session ends DONE-ALL (`loopdiag_p67.sh:249, 259, 293`). Ending a cut session FAILED-ALL on an outcome label would make the cut session stricter than a complete one. Options: (i) keep the exits and have the TRUNCATED-ALL line list every non-pass verdict (being added on `harness67c`); (ii) give both DONE-ALL and TRUNCATED-ALL a verdict-aware exit. Open for the coordinator.
- **Exit 7 for TRUNCATED-ALL (#154).** The code now follows the five-hour cap rule of `docs/measurement-discipline.md`: the truncation sentinel is not a failure, the stage is scored at the n it reached, and no truncated verdict reads PASS (R1-BLIND included). Exit 7 for TRUNCATED-ALL is a default awaiting owner confirmation.
- **Tools tests in session 0 (#20, #21, #141).** Session 0 runs only run_all's pre-build tools gate (the plan-67 parser and harness tests) from the harness tree (`loopdiag_run_all.sh:124-125`, `:476`); `run_suites` skips the full set (`loopdiag_p67pre.sh:89`), which runs in session 1. §2.2 says the tools tests run in session 1. Proposed text #141 registers the gate in session 0 and the full set in session 1.
- **§2.1 item 2's line reference** (`:736–751`) is stale. The fixed rule is now at `loopdiag_run_all.sh:768-795`.
- **§6.4's "10 smoke rows"** no longer matches the code's 16 (#119, #120).
- **No seats or token exist for the G1 bisection** (`vmb-c8`, `vmb-tun`, §5.2) **or for a pool-only session** (§5.2, "it moves to its own session").

### Seat heads after the 2026-11-21 main merge (draft)

Main moved from the seats' merge base `e02ee2d4` to `9bc656bf` with docs, tools and roadmap commits plus test-only engine-path files: `raptorpath/tests/sim_mixed_rho.rs` (its one test `#[ignore]`d), `raptorpath/tests/sim_support/mod.rs`, a `[[test]]` stanza in `raptorpath/Cargo.toml`, `raptorpath-math/tests/mutation_triage_rate.rs` and the new crate `raptorpath-model` (not a workspace member). No `src/` of a workspace crate, root `Cargo.*` or `vendor` changed, so §1.3 "If main moves" needs no rebuild-and-re-pin of engine content; the seat commits move and the binary shas are recorded at the rehearsal as planned. Main later gained docs-only commits (to `91fa38f0`), not merged into the seats.

| seat | branch | 2026-11-21 merge (history) | pin | how |
|---|---|---|---|---|
| B0 | none (`vmb-base` does not exist) | github/main `9bc656bf` | filled in at pin time (B0 moves at the seat re-merge) | B0 = the main commit inside S1 |
| S1 | `vmb-s1` | `2d590cdc` → `6628e93b` | filled in at pin time | `git merge --no-ff github/main`, clean |
| S2 | `vmb-s2` | `1953abd1` → `3d06984e` | filled in at pin time | `git merge --no-ff` new S1; one conflict in `.github/workflows/ci.yml` (sim job): both `--test sim_s06_nack` and `--test sim_mixed_rho` kept |
| P1, P2 (P1T, P2T) | none | — | filled in at pin time | not built yet: `4ee1df9b`'s engine diff applies cleanly to both 2026-11-21 heads (`git apply --check`); the `lib.sh` forward line still to add |
| vmb-c8, vmb-tun | none | — | filled in at pin time (§5.2) | not built |

Witnesses at the 2026-11-21 heads (§1.3):
- `git diff B0 S1 -- tools` is empty; `git diff B0 S1 -- tools/l1` and `git diff B0 S2 -- tools/l1` are empty.
- `git diff S1 S2 -- tools` is NOT empty: `tools/ci/coverage_sites.json` loses 16 lines on S2, the stale `shed-slot` / `shed-slot-denied` entries that §1.2's merge-up removes by design. §1.3's witness and §1.2's merge-up contradict each other there; the harness's own witness compares `tools/l1` only (#12 above). Proposed: §1.3 reads `-- tools/l1`, and the `coverage_sites.json` difference is listed as S2 seat content.
- Engine untouched: `git diff <old> <new>` over `raptorpath/src raptorpath-math/src raptorpath-core raptorpath-wasm gf256 Cargo.toml Cargo.lock vendor` is empty for S1 and S2. Over `raptorpath*` it lists exactly main's nine files above. The seat-against-base engine patch-id is unchanged: S1 `ac12e9b5` (with `.github`), S2 `008dc9da` (without `.github`; with it, the ci.yml merge line differs).
- CI (`ci`, `binid`) green on both heads; nightly dispatched on both.
- Every seat's `LD_PIN_<SEAT>` is filled in at pin time. plan67.py has no defaults (#1).

### Proposed text changes (draft, for review)

Each item replaces or extends the registration text named. Since `9bc656bf`, harness67c fixed the code for #1, #49, #51, #58, #76, #81, #85 (R2-BLIND), #89, #91, #100, #101, #111 (G), #112, #121 and #128/#134 (ABORT-SEAM) at `eb976857`, and for #40/#149 (G2), #137 (R1-BLIND truncated), #152 (R2) and #154 (TRUNCATED-ALL) at `3092dc96`. Where an item below names a fixed row (#58, #111), it writes the fixed behaviour into the text.

- **#12 (§1.3, tools witness).** Replace the third bullet with: "`git diff B0 S1 -- tools` is empty. `git diff S1 S2 -- tools` lists one file, `tools/ci/coverage_sites.json`, where S2 drops the stale `shed-slot` and `shed-slot-denied` entries (§1.2's merge-up); that is the one sanctioned tools difference besides the pacer seats' `lib.sh` line. The harness checks `tools/l1` byte for byte on every seat (the pacer seats by that one line)."
- **#141 (§2.2, §6.0, session 0).** In §6.0's heading, replace "(`suites_run.sh` envelope with `run_suites`' fixed rule; `LD_HARD` = launch + 4.9 h)" with "(`LD_PLAN=67pre` through `loopdiag_run_all.sh`, `run_suites`' fixed rule; `LD_HARD` = launch + 4.9 h)". Add to §2.2: "Session 0 runs as `LD_PLAN=67pre`. Its steps, in order:
  0. the locks: a child must inherit both session locks, else `ABORT-PREFLIGHT plan 67pre step locks`;
  1. `cargo check --all-targets` on S1 and S2: `ABORT-PREFLIGHT plan 67pre seat <S>: cargo check --all-targets failed`;
  2. the gates on B0: no test result is `ABORT-PREFLIGHT plan 67pre seat B0: the gates reference did not run`;
  3. the suites on S1, then S2: `ABORT-PREFLIGHT plan 67pre seat <S>: gates failure not equal on B0`, or `ABORT-TESTS plan 67pre seat <S>`;
  4. the repeats: `ABORT-REPEAT plan 67pre seat <S>`;
  5. the visualizer and the golden test on S2: `ABORT-PREFLIGHT plan 67pre seat S2: visualizer ...` or `... golden test ...`;
  6. binid on B0, S1 and S2 (`ABORT-PREFLIGHT plan 67pre binid: <reason>`), then each seat's BINTEST under the gates rule: `ABORT-PREFLIGHT plan 67pre binid: BINTEST <S> <names> not equal on B0`, or BINTEST-FINDING-ON-MAIN when B0 fails the same test.

  Each seat's reference manifest is written as soon as that seat's own steps pass. Any abort ends session 0. After an S2 abort, guard 1 needs session 0 re-run with `LD_DROP_S2=1`." For the tools tests, replace §2.2's "The tools tests (`tools/l1`, python and bash; ABORT-TESTS) run in session 1, the rehearsal (§6.1)" with "Session 0 runs the pre-build tools gate from the harness tree (the plan-67 parser and harness tests; a failure is ABORT-TESTS and session 0 stops). The full tools tests run in session 1, the rehearsal (§6.1)." Chosen over keeping the old sentence with an addition, because the gate can stop session 0, and an abort that can fire there must be registered for that session, not only for session 1.
- **#39 (§3.1, §4.1).** Add: "GATE-67G1 / GATE-67G2 PASS iff GATE-41d PASS, no `S06-FAIL` row (G2), no unreplaced witness failure and no `G2-TAPER-NONZERO` row outside `S06-FAIL`. An abort reads FAIL (UNSCOREABLE). A ledger cut at `LD_HARD` reads TRUNCATED."
- **#40, #149 (§4.1, taper on S2).** Replace "`taper=0` on every S2 row" with: "`taper = 0` and `plost = 0` on every S2 row. A nonzero reading on an S2 row that prints `pslot=` is `S06-FAIL`, whatever its other labels, and is never replaced. A nonzero reading on an S2 row without `pslot=` fails the seat witness: the row is replaced once, and `G2-TAPER-NONZERO` fails GATE-67G2 whatever the replacement shows."
- **#79 (§4.3).** The S2 outcome witness is `taper = 0` and `plost = 0` (§4.2's rule), not `taper = 0` alone. An S2 row with `pslot = 0` is VOID-WITNESS.
- **#41 (§4.1).** Add: "§4.2's rule 'a DNF where every S1 row completes is `S06-FAIL`' applies to G2 too, over S1's LIVE rows."
- **#58, #152 (§4.2, the minimum; §4.4, the twin minimum).** In §4.2, replace "a cell with fewer than 10 live rows on S1 or S2 after replacement is UNSCOREABLE for AS and A1" with: "a cell with fewer than 10 live rows on S1 or S2 after replacement is UNSCOREABLE for AS and A1. A LIVE DNF counts; a crash (VOID-RC, or NO_DATA after its replacement) does not, and the per-arm live and crash counts are printed. An UNSCOREABLE caused by candidate-arm crashes is a finding to investigate, not something a plain re-run clears." In §4.4, after "Unscoreable is never read as 'did not fire'", add: "The candidate arm is P2T: when P2T's live plus crash rows reach the minimum, R2-HOLD reads UNSCOREABLE (CANDIDATE-CRASH P2T). An UNSCOREABLE caused by candidate-arm crashes is a finding to investigate, not something a plain re-run clears. A P1T shortfall is a plain UNSCOREABLE with the live and crash counts."
- **#59 (§4.2, A1).** Replace "a cell with fewer than 10 live rows on S1 or S2 after replacement is UNSCOREABLE for AS and A1" (as worded in the #58 item above) with: "an S2 shortfall makes AS UNSCOREABLE; A1 is UNSCOREABLE only on an S1 or B0 shortfall (fewer than 10 live S1 rows, or fewer than 4 live B0 rows, in a cell)".
- **#86 (§4.4, twins).** Replace "a row with neither is VOID" with "a twin row missing either `[ACCT]` (client) or `[ACCT-RX]` (server) is VOID".
- **#126 (§4.4, R2 smoke).** The registration is kept and the code is being brought to it on `harness67c` (an R2 smoke failure marks R2 VOID, the other smokes run, AMENDMENT2.txt records it; guard 1 is not held, as §5.2 requires). Add to §4.4: "The R2 smoke runs in the rehearsal (§6.1). Session 3 runs no second R2 smoke; it reads the same witness on the scored rows, and more than half of a seat's rows failing it (`R2-BLIND`) makes R2 VOID, so guard 2 waits for an R2 re-run."
- **#110 (§5.2, block-1).** Add: "Block-1 reads each arm's first ledger row as written: a first row that was void and then replaced still stops the stage, and so does a first-row `S06-FAIL`. In Stage A only the c2 cell's first rows are read. In G1 and G2 the first row of each of the 8 (cell, arm) pairs of block 1 is read." (As the code does; stricter than §5.2's wording, so it can only stop a stage more often.)
- **#111 (§5.2, block-1 scope).** Replace "after each stage's first block" with "after the first block of G1, G2, A, R1, R2 and the pool block". G1 and G2 run block 1 over all 8 (cell, arm) first rows. Stage T has no block-1 rule: it is four invocations with no blocks, and its bring-up abort (ABORT-BRINGUP-T, #101) covers it. The dump twins have none: they are eight reported rows with no verdict, so a failed first row is reported, not a stop.
- **#136, #137, #154 (§5.2, §5.4, truncation).** Add to §5.2: "A stage cut at `LD_HARD` is TRUNCATED: it is scored at the n it reached, and none of its verdicts reads PASS, CLEAR or ELIGIBLE (R1-BLIND reads R1 TRUNCATED). A stage with under 1 800 s left before `LD_HARD` is not started (ABORT-BUDGET, NOT-STARTED). A session whose only not-ok stages are cut or not started ends TRUNCATED-ALL (`LD-ALL-TRUNCATED`, exit 7; a default awaiting the owner), which is neither a pass nor a failure. Any other not-ok stage ends the session FAILED-ALL." Add TRUNCATED-ALL and NOT-STARTED to §5.4 beside TRUNCATED (#129). Two limits, stated as registered: G's replacement run, when fewer than 900 s are left before `LD_HARD`, is skipped and only logged (`ABORT-BUDGET stage <g> replacement`, `loopdiag_p67.sh:242`); the stage gets no NOT-STARTED marker and no return 8, and its voided slots stay void, so 41d's cap rule reads them. Session 0 (`67pre`) and the rehearsal have no truncation path: a budget shortfall there is ABORT-BUDGET (session 0, FAILED-ALL) or ABORT-SMOKE (the rehearsal's bounded smoke, `loopdiag_p67.sh:202-206`), never TRUNCATED.
- **#129 (§5.4, abort classes).** Replace "bring-up exhausted (Stage T)" with ABORT-BRINGUP-T, and add the harness's further tokens: ABORT-SEAM (first, before the locks), ABORT-REHEARSE, ABORT-CUT, ABORT-SHM, ABORT-SUITES, ABORT-BUDGET, ABORT-ROWS, ABORT-SHA, ABORT-BIN, ABORT-PLAN67, ABORT-PRECONDITION, `FAILED-<stage>` (FAILED-STAGE67), BUSY, BINID67 FAIL. Add the verdict tokens that are not aborts: TRUNCATED (the battery was cut at `LD_HARD`), INCOMPLETE (UNSCOREABLE: a registered slot holds no row), R2-BLIND (R2 VOID). ABORT-PREFLIGHT stays as listed; session 0 now emits it (#141). The verdict taxonomy of `docs/measurement-discipline.md` records no verdict outside the registered set, so these tokens need this text before any scored result is read.
- **#130 (§5.4, order).** Add: "In practice ABORT-SEAM fires first, then ABORT-PRECONDITION, the session budget (ABORT-BUDGET) and the tools-test gate (ABORT-TESTS); then commit pin (ABORT-SEAT); in session 0 its steps follow; otherwise build and sha (ABORT-BUILD), identity strings and tools witness (ABORT-SEAT), suite reuse (ABORT-SUITES), tools tests (ABORT-TESTS). The Result lists the first one that fired."
- **#132 (§9 item 5).** Replace "parsed with `verify4_parse.py tun`" with "parsed by `s67_score.py`'s T parser, which reads the bring-up lines; `test_s67.py` checks that it and `verify4_parse.py tun` agree on a fixture".

### B0 and the binid runs (2026-11-21)

- **B0** is the merge base of `vmb-s1` and github/main (the main commit inside S1), not a moving main. Its sha is filled in at pin time: it moves at the seat re-merge. After the 2026-11-21 merge it was `9bc656bf`.
- **What the push binid runs compared.** On the pushes of `vmb-s1` `6628e93b` and `vmb-s2` `3d06984e`, `binid.yml` compared main `9bc656bf` (base, base2: identical) against each seat head. Both passed only through the `binid: expected-change` line of `2d590cdc`. They did not compare old seat head against new. Old against new, dispatched with no opt-out: identical release binaries (archive and binary sha) for S1 `2d590cdc` → `6628e93b` (run 44938909897) and S2 `1953abd1` → `3d06984e` (run 44938912735).
