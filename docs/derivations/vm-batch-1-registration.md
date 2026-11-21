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
