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

## Amendment 2: harness decisions

The registered second amendment. It lists the decisions the plan-67 harness takes that this registration does not state. Rows 1-135 were first read at github/main `9bc656bf` and synced at harness67c `3092dc96`; the regeneration of 2026-11-27 read every row at `harness67d` `5826606a`. **Every row is now read at `harness67e` `1d26c8cd8e2c51b15e61ea19f6578fe6f25a8356`** (main `6d1c959ddf73ec10bc1eb1374c222c6e3824b6ea`; `harness67e` is harness67d plus the third review's eight fixes, merged to main as `5ea4dd11`; see `git diff 5826606a 1d26c8cd -- tools/l1`). Line references were carried from `5826606a` to `1d26c8cd` by a line-level diff of each file, and each carried line was checked (the text of the cited line is the same at both commits); every row whose cited lines the diff changed, and every row whose claim it changes without moving its lines, was re-read against the diff hunk by hunk. Edited at harness67e: #24-#26, #39-#41, #49, #58, #77, #78, #80, #90, #98, #100, #122, #124, #126, #127, #129, #131, #137, #138, #149, #150, #154, #155, the defaults for #77/#78, #90 and #122/#124, and the Proposed texts; new rows #157-#161. The tools witness (#12-#14) compares each seat's `tools/l1` with the harness that runs the session; the final pins are in the seat table below. Each row gives the decision, where the code takes it, and a verdict: MATCHES (stated, but the code is more specific), EXTENDS (the registration is silent and the code picks), CONTRADICTS (the code differs from the quoted text), or FIXED on harness67c / harness67e (a former CONTRADICTS that now matches the registration text, with the line). Paths are under `tools/l1/`. `run_all` is `loopdiag_run_all.sh`, `battery` is `loopdiag_battery.sh`, `p67` is `loopdiag_p67.sh`, `p67pre` is `loopdiag_p67pre.sh`. Scoring choices that belong to one stage sit in that stage's table; the last table holds the ones that cut across stages. Rows 136-153 were added at `eb976857`, row 154 at `3092dc96`, rows 157-161 at `1d26c8cd`; each sits in its area's table.

### Seats, build and pins

| # | decision | where | verdict |
|---|---|---|---|
| 1 | No seat has a default commit pin. Every seat's `LD_PIN_<SEAT>` is required in every session, 67pre included; an unset pin is ABORT-SEAT. (At `9bc656bf`, S1 and S2 defaulted to the old heads.) | plan67.py:16, 34-45; p67:53-61 | MATCHES |
| 2 | P1T and P2T reuse P1's and P2's pin (`LD_PIN_P1`, `LD_PIN_P2`) and source tree. | plan67.py:44-45 | EXTENDS |
| 3 | The pacer commit `4ee1df9b` is informational. Nothing checks that P1 and P2 contain it, except the binary strings in #10. | plan67.py:52 | EXTENDS |
| 4 | A pin is a prefix match: `$ROOT/seats/<tree>/COMMIT` must start with the pin, so an 8-hex pin accepts any commit that shares those characters. | p67:59-60 | EXTENDS |
| 5 | Paths: trees at `$LD_ROOT/seats/<B0,S1,S2,P1,P2>/src`, targets at `$ROOT/target-<seat>` (must be fresh), build logs at `$ROOT/build-<seat>.log`, installed binaries at `$LD_SHM/bin-<seat>/raptorpath` (root, 0755), kept copies at `$RUN/bins67/<seat>-raptorpath`, shas in `$RUN/BINSHA.txt`. | p67:13, 59, 70-94 | EXTENDS |
| 6 | The rehearsal builds the seven seats one after another, with `CARGO_BUILD_JOBS=4` and `cargo build --release --bin raptorpath`, adding `--features test-seams` for P1T and P2T. | p67:71-87; plan67.py:44-45 | EXTENDS |
| 7 | The rehearsal also accepts `LD_BIN_PREBUILT_<SEAT>` and then skips that seat's build. §6.1 says "five release builds". | p67:77-80 | EXTENDS |
| 8 | A launch (67g, 67x) never builds. Every seat it uses needs both `LD_BIN_PREBUILT_<SEAT>` and `LD_BIN_SHA_<SEAT>`, else ABORT-BUILD. In the rehearsal a given `LD_BIN_SHA_<SEAT>` is checked, and an absent one is recorded. | p67:77-100 | MATCHES |
| 9 | Any two seats with the same binary sha are ABORT-BUILD. | p67:104-106 | EXTENDS |
| 10 | Identity strings (ABORT-SEAT before any row, the rehearsal included). Beyond §3.1 and §4.1: B0 also forbids `pfec=`, `pslot=` and `[ACCT-RX]`. Every non-twin seat forbids `[ACCT-RX]`. The P seats need `RWM_PERF_RATE_MBIT` and `offered-load pacing ACTIVE`. The twins need `[ACCT-RX]`. P1 and P1T forbid `pslot=`. | plan67.py:36-45; p67:108-116 | EXTENDS |
| 11 | Each stage's battery checks the identity strings and the sha again (`preflight_binary ... RWM_STORE`, `--help` runs), and exits 5 on a failure. The envelope then marks the stage not ok and goes on to the next stage. | battery:293-306; tun67_battery.sh:40-51; p67:300-311 | EXTENDS |
| 12 | The tools-diff witness compares each seat tree's `tools/l1` file manifest with the harness's own `tools/l1`. §1.3 says `git diff B0 S1 -- tools` and `git diff S1 S2 -- tools`. The rest of `tools/` (e.g. `tools/ci`, `tools/roadmap`) is not compared. | p67:119-137 | CONTRADICTS |
| 13 | The witness reference is the harness tree running the session, not B0. It runs on the VM over the archives, not as a `git diff`, and skips `*.pyc` and `__pycache__`. | p67:120-121 | EXTENDS |
| 14 | The pacer exemption: P1's and P2's `tools/l1` may differ in `lib.sh` only, and `lib.sh` must equal the harness's once the token ` RWM_PERF_RATE_MBIT` is deleted (a token in the forward list, not a whole line). | p67:129-133 | EXTENDS |
| 15 | Suite reuse is checked for S1 and S2 only, against `LD_REF_MANIFEST_S1` and `LD_REF_MANIFEST_S2` (required, else ABORT-SUITES). B0, P1 and P2 have no suite record and no reuse check. | p67:139-152 | EXTENDS |
| 16 | The reuse manifest (`src_manifest67`, one definition for session 0 and the comparison) covers the Cargo files, the engine crates' src, `raptorpath/tests`, `gf256/src`, `vendor/`, and also `tools/ci/flake_rule.py`. Session 0 writes it on the VM over the archive files (#146). `ref_manifest67.sh <rev>` is the host-side equivalent over git blobs. | p67:44-50; p67pre:43-47; ref_manifest67.sh:11-13 | EXTENDS |
| 17 | The session refuses to start unless the time left before `LD_HARD` covers `plan67.BUDGET_S`: 17 040 s (67pre), 7 500 s (67), 10 000 s (67g), 15 600 s (67x). Against §6's offsets of 4.9 h, 2.25 h, 3 h and 4.5 h, that leaves 600 s of slack at launch for 67pre, 67 and 67x and 800 s for 67g. `LD_HARD_MIN_S` overrides it; it is a test seam (#140). | plan67.py:76; run_all:391-397 | EXTENDS |
| 18 | Inherited plan-46 gates: the session waits without a bound (30 s polls) until both locks are free. It then needs 5 samples of `procs_running` at most 2 and no cargo or rustc, else ABORT-PRECONDITION. | run_all:259, 351-366 | EXTENDS |

### Session 0, suites, repeat and binid

| # | decision | where | verdict |
|---|---|---|---|
| 19 | Plan 67 never runs the engine suites in sessions 1-3 (`ENGINE_SUITES=0`). The reference-manifest HIT stands for them. | p67:155 | MATCHES |
| 20 | Before the seats are built, every plan-67 session (67pre included) runs these tools tests from the harness tree: `test_loopdiag_parse`, `test_harness46`, `test_harness47`, `test_l1common`, `test_pool54`, `test_harness64`, `test_guard_v2`, `test_bbr48_parse`, `test_place38_parse`, `test_s67`, `test_harness67`. A launch runs them even when it reuses the rehearsal's tools result. In 67pre this pre-build gate is the only tools-test run (#21). | run_all:124-125, 455-462, 476-480 | EXTENDS |
| 21 | The tools tests in `run_suites` are every `tools/l1/test_*.py` except `test_stubrun.py`, plus `test_lib_battery.sh` and `test_abort_witness.sh`. They run from the harness tree, not from each seat's tree. Session 0's suites skip them (`SUITES_NO_TOOLS=1`), so session 0 runs only the pre-build gate of #20; the full set runs in session 1. | run_all:810-820; p67pre:89 | EXTENDS |
| 22 | A launch reuses the tools result only when `LD_PRIOR_TOOLS` holds `TOOLS-TESTS-OK` and a byte-identical `tools/l1` manifest. On a MISS it runs the tests; that is not an abort. | p67:156-160; run_all:880-892 | EXTENDS |
| 23 | ADR-0080's rule in `run_suites`: a failure fails unless the test is on `raptorpath/tests/quarantine.txt` (checked by `tools/ci/flake_rule.py --check`). The solo re-run is `-- <t> --exact --test-threads=1` and only classifies the failure. A failure with no named test is REAL. The target dir (`$ROOT/target-tests`, or session 0's `SUITES_TT`) must not exist, else the suites fail. | run_all:745-754, 768-795 | MATCHES |
| 24 | Repeat defaults: `R67_N=30`, `R67_BUDGET_S=3600`, seats S1 then S2 (the arguments are sorted, so S1 runs first whatever the call order; harness67e), one seat after another, a fresh `CARGO_TARGET_DIR=$R67_TGT/tgt-<seat>` (session 0 passes a disk path under `$ROOT`) removed before and after, and for the sim runs `<that>-sim`, removed after them. `R67_N`, `R67_BUDGET_S`, `R67_SIM` and `R67_T1_S` are test seams (#140). | repeat67_run.sh:28-33, 78-79, 138, 146; p67pre:122-124 | EXTENDS |
| 25 | The repeat budget (`R67_BUDGET_S`, §2.2: 60 min total) is shared in run order, S1 first (the seats are sorted, line 33). A seat's share is fixed at its start: what is left of the budget over the seats still to run. So S1 gets half (1 800 s) and S2 gets the budget minus S1's elapsed time, S1's build and runs included (`T_START` at line 51, the share at line 75); a seat that runs alone gets all of it. The first run of the seat's whole command is timed, and the cut is `floor(share / t1)` (lines 89, 106-111); `REPEAT-CUT` names the share. The cut applies to all of the seat's tests at once, not per test. A seat's own build and sim runs are outside its share (it is set before them), so S2's build and sim runs come on top of the 60 min. Before harness67e the budget was split equally and S1's elapsed time did not move S2's share. | repeat67_run.sh:33, 51, 75, 89, 106-111 | FIXED on harness67e (repeat67_run.sh:33, 51, 75) |
| 26 | If the seat's share (#25) does not fit one run, that is ABORT-REPEAT. A cut is written as `REPEAT-CUT seat=<S> reached=<n>`. | repeat67_run.sh:106-110 | EXTENDS |
| 27 | The repeat commands use `--release`: S1 `cargo test -p raptorpath --release --lib -- config::`; S2 `cargo test -p raptorpath --release --test s06_no_copy --test holeclass_reachability --no-fail-fast`. §2.2 names "every non-ignored test in `s06_no_copy` and `s06_reactive`", but every test in `s06_reactive` is `#[ignore]` "records, does not gate" (T2 gates as `sim_s06_nack`, T3 records only; §2.1 item 3), so its share is empty and it is not named (VM 2026-11-27: `0 passed; 0 failed; 6 ignored`). Before `harness67d`, S2 named `s06_reactive` without `--features test-seams`; the VM check of 2026-11-27 confirmed cargo's refusal ("requires the features: `test-seams`", rc 101), so the S2 build step would have read ABORT-REPEAT seat=S2 without running a test. | repeat67_run.sh:81-83 | EXTENDS (fixed on harness67d) |
| 28 | A failing run is a named `FAILED` test, a missing `test result` line, or a non-zero exit; and (harness67d) a run that exits cleanly but, for any named target, lacks its `Running` line (`TARGET-MISSING(<t>)`; S1's target is `unittests src/lib.rs`) or has no passed test in the `test result` lines after that `Running` line (`ZERO-TESTS(<t>)`). The rule is per target: a target whose tests are all `#[ignore]` fails the run even when another target passes. No quarantine applies. | repeat67_run.sh:54-56, 60-72, 96-103 | EXTENDS |
| 29 | The sim tests run on S2 only. The default list (`R67_SIM`) is `sim_shed_reach sim_lost_tail sim_s06_nack sim_nack_test place_arms_reachability`: `sim_s06_nack` is T2's port (§2.1 item 3). The list is split by the seat's own `raptorpath/Cargo.toml`: a target whose `[[test]]` stanza requires `sim` (sim_shed_reach, sim_lost_tail, sim_s06_nack) runs as ci.yml's sim job runs it, `RUSTFLAGS="--cfg tokio_unstable" cargo test -p raptorpath --features sim,test-seams` (debug profile), in its own target dir (`<seat target>-sim`, removed after); the others (sim_nack_test, place_arms_reachability) run `--release` with no feature, as before. Each of the two runs must exit 0 (`SIM-RC<k>`, `DET-RC<k>`), print `test result` (`SIM-NO-RESULT<k>`), show every target's `Running` line and pass at least one test (`SIM<k>:TARGET-MISSING(<t>)/ZERO-TESTS(<t>)`, per target as #28); the digests must be equal. Before harness67d the list lacked `sim_s06_nack` and every target ran `--release` with no feature. The sim build is a separate debug build, outside the repeat budget: §6.4's session 0 gains an estimated 10 min for it (not measured). | repeat67_run.sh:30, 57-59, 82, 116-139 | EXTENDS (fixed on harness67d) |
| 30 | The repeat takes both VM locks (inherited from session 0, #142), refuses with ABORT-PRECONDITION if cargo or rustc is running, and exits 5 on any failure. | repeat67_run.sh:37-46, 140-151 | EXTENDS |
| 31 | binid packs each given seat's `src` plus its `COMMIT` file into a tarball and builds it with `binid_run.sh` at `B67_WORK`. Session 0 passes B0, S1, S2 (B0, S1 under `LD_DROP_S2=1`). The shas must be pairwise different over the given seats, else `BINID67 FAIL`. | binid67_run.sh:16, 20-29, 41-45; p67pre:165 | EXTENDS |
| 32 | `repeat67_run.sh` and `binid67_run.sh` are called by session 0's envelope, `loopdiag_p67pre.sh`, which also runs `cargo check`, the gates on B0, the S1 and S2 suites and the visualizer (#141-#147). | p67:63; p67pre:60-171 | MATCHES |
| 141 | Session 0 is `LD_PLAN=67pre`, run as `loopdiag_run_all.sh` → `loopdiag_p67.sh` (the pins of B0, S1, S2) → `loopdiag_p67pre.sh`, with no build and no battery. It refuses `LD_REHEARSE=1` (ABORT-REHEARSE) and needs 17 040 s before `LD_HARD` (#17). Its only tools tests are run_all's pre-build gate from the harness tree (#20, #21); §2.2 runs the tools tests in session 1. §6.0 says "`suites_run.sh` envelope". Steps in order: 0 the locks, 1 `cargo check --all-targets` on S1 and S2, 2 the gates on B0, 3 the suites on S1 then S2, 4 the repeats, 5 the visualizer and the golden test on S2, 6 binid then BINTEST, 7 the record (`SESSION0.txt`, `LD-ALL-SESSION0-DONE`). A step starts only with 600 s left (else `ABORT-BUDGET plan 67pre step <name>`) and runs under `timeout` to `LD_HARD` (rc 124 is the same abort). Every target dir must be fresh (`ABORT-PREFLIGHT plan 67pre: <dir> exists ...`) and is removed after its step. `LD_DROP_S2=1` runs no S2 step. | run_all:124-125, 141, 190-191, 476; p67:30, 63; p67pre:1-41, 60-212, 89; plan67.py:76, 81 | CONTRADICTS |
| 142 | Step 0, the locks: a trivial child (`lockcheck67.sh`), started the way the real children are, must log `LOCK-INHERITED` for both locks within 30 s, else `ABORT-PREFLIGHT plan 67pre step locks: a child cannot inherit the session's locks`. The children (`lockcheck67.sh`, `repeat67_run.sh`, `binid67_run.sh` through `binid_run.sh`) get `LB_LOCK_PARENT=<session pid>` and both lock paths. `take_lock` treats a lock whose holder field is that live pid as inherited: logged, never taken, never released. | p67pre:38-40, 49-58; lockcheck67.sh:1-16; lib_battery.sh:97-108; repeat67_run.sh:44-45; binid_run.sh:31-32 | EXTENDS |
| 143 | Steps 1-3. Check: `ABORT-PREFLIGHT plan 67pre seat <S>: cargo check --all-targets failed`. Gates on B0: no `test result` line is `ABORT-PREFLIGHT plan 67pre seat B0: the gates reference did not run`; B0's failing `gates::` tests are GATES-B0-FINDING, not an abort. Suites (`run_suites`: main, doc, wasm without `GOLDEN_CAPTURE`, core, debug, and three f2 test-seams targets): the failures are the REAL-FAILURE and FLAKE-FAILURE lines (QUARANTINED is not one). A `gates::` failure B0 does not have is `ABORT-PREFLIGHT plan 67pre seat <S>: gates failure not equal on B0`; any other failure is `ABORT-TESTS plan 67pre seat <S>`; with both, both lines print and the abort is ABORT-PREFLIGHT. A `gates::` failure B0 shares is GATES-FINDING-ON-MAIN, and the suites count as passed. A suite killed at `LD_HARD` is ABORT-BUDGET. The touched-target list is reported (`TARGETS seen / not-seen`), not gated. | p67pre:60-118; run_all:796-806 | MATCHES |
| 144 | Steps 4-6. Repeat: `ABORT-REPEAT plan 67pre seat <S>`, or `ABORT-REPEAT plan 67pre: repeat67_run.sh rc=<n>` for a precondition or lock failure. Visualizer: `build_visualizer.sh` runs in a scratch copy of S2's tree with a fresh wasm (`ABORT-PREFLIGHT plan 67pre seat S2: visualizer ...`); then `cargo test -p raptorpath-wasm golden` without `GOLDEN_CAPTURE`, which must pass at least one test (`... seat S2: golden test failed` / `golden test ran no test`). Binid: no `BINID67 PASS` is `ABORT-PREFLIGHT plan 67pre binid: <reason>`. | p67pre:120-172 | EXTENDS |
| 145 | The binid inner-test rule. `binid_run.sh` runs the math and wasm tests (BINTEST) on each seat's tree and does not fail on them. Session 0 applies the gates rule: a failing BINTEST test on S1 or S2 that B0 does not fail is `ABORT-PREFLIGHT plan 67pre binid: BINTEST <S> <names> not equal on B0`; one B0 also fails is BINTEST-FINDING-ON-MAIN, and the session goes on. A BINTEST with no BINTEST line or no `test result` line is `ABORT-PREFLIGHT plan 67pre binid: BINTEST <S> ran no test`, on any seat; on B0 it stops the whole session. §2.2 gives binid no test rule. | p67pre:173-193; binid_run.sh:60-66 | EXTENDS |
| 146 | REF-MANIFEST per seat: `$RUN/REF-MANIFEST-<S>.sha256` (`src_manifest67`) is written as soon as that seat's own steps pass: S1 after its repeat reads zero failures, S2 after its golden test. Both come before binid and BINTEST, which run last for all seats, so a later BINTEST abort on S1 leaves S1's manifest in the persisted run. Step 7 only checks that they exist, then writes `SESSION0.txt` with the `LD_REF_MANIFEST_<S>` lines. | p67pre:42-47, 127-131, 159, 195-209 | EXTENDS |
| 147 | An S2 failure in session 0 ends the session at that step: `seat_fail` prints `... (seat S2 is in the S2 family: guard 2 stops; relaunch with LD_DROP_S2=1 to run guard 1 alone)`, then FAILED-ALL, exit 5, the run persisted, no `SESSION0.txt`. No later step runs, S1's included. Guard 1 needs session 0 relaunched with `LD_DROP_S2=1`, which re-runs S1's steps and binid on B0 and S1. The failed run holds REF-MANIFEST-S1 only if S2 failed at the repeat or later, and binid shas only if it failed at BINTEST. A B0 or S1 failure ends the session the same way, with no relaunch hint. §5.2: "On S2, it stops guard 2. Guard 1 still launches, with S2 dropped." | p67:33-39; p67pre:68, 106-108, 133-136, 143-157, 180, 191 | EXTENDS |
| 155 | (harness67d) The repeat refuses to read a run that ran nothing as zero failures: TARGET-MISSING / ZERO-TESTS (#28) on the gating runs and the sim runs (#29). The stub repo of plan 67 carries the four `[[test]]` stanzas raptorpath/Cargo.toml declares, so the split of #29 runs on the stubs as on a seat. Stub paths: `repeat67/repeat-zero-tests` (every S2 run passes 0 tests: ABORT-REPEAT seat=S2, S1 not aborted), `repeat67/repeat-all-ignored` (one gating target all `#[ignore]`, the other passing: ZERO-TESTS(<t>) on every run, ABORT-REPEAT seat=S2; red on the per-run rule of `ba0b112b`); the stub prints one `Running` and one `test result` line per target, as cargo does; Stub path `repeat67/repeat-s2-share` (harness67e): a 1 000 s first run (seam) on a 3 900 s budget: S1's share is half (one run, `REPEAT-CUT` on record), S2's is the budget minus S1's real elapsed time (2 or 3 runs; an equal split would give 1), rc 0 (#25); `repeat-cut` now uses a 2 400 s budget and a 500 s first run (S1's share of 1 200 s fits 2 runs); the stub cargo now refuses a `--test` target whose required features are off and a sim target without `--cfg tokio_unstable`, as cargo and the sim build do, so the pre-fix script reads red (`repeat-pass`: ABORT-REPEAT seat=S2 at build). | repeat67_run.sh:60-72; test_stubrun.py | EXTENDS |
| 156 | Where session 0 runs the touched targets. §2.2 lists `s06_reactive`, `sim_shed_reach` and `sim_lost_tail` among the touched integration tests "inside the suites run". `run_suites` runs the engine suites with no feature beyond the three f2 `test-seams` targets, and cargo skips a target whose required features are off when it is not named, so none of the three runs there. `sim_shed_reach` and `sim_lost_tail` (and `sim_s06_nack`) run only in the repeat's sim step (#29). `s06_reactive` (all `#[ignore]`, record only) is built and run nowhere in the batch, and no ci.yml job builds it either (its ignore reasons start with "records", not a roadmap node id, so the defects job does not select it). Session 0's touched-target report (`TARGETS ... not-seen=`, reported, no gate) will list the three as not seen on a real S2 tree; the stub's `STUB_TARGETS` prints them as seen, so the stub runs do not show this. | run_all:796-806; p67pre:111-117; repeat67_run.sh:116-139 | CONTRADICTS |

### Stages G1 and G2

| # | decision | where | verdict |
|---|---|---|---|
| 33 | G runs plan 41d's battery with `LD_WITNESS67=g1` or `g2`. CAND and BASE are rebound to (S1, B0) and (S2, S1). `LD_BASE_SHA` is the BASE seat's commit pin. `LD_WITNESS54=0` and `LD_WITNESS64=0` are forced. | p67:173-187; plan67.py:82 | MATCHES |
| 34 | A G stage is ok only if the battery exits 0 and `ld.log` holds exactly 96 `P38ROW` lines, else ABORT-ROWS. | p67:22, 248-251 | EXTENDS |
| 35 | Replacements are run at the end of the stage (`place38_parse replacements41c`, cap 4), as plan 41d does. More than 4 voided slots means NO VERDICT and no replacement run. The replacement run needs 900 s left before `LD_HARD`, else ABORT-BUDGET. 41d's replaceable statuses are WITNESS-FAIL, CONTAMINATED, VOID-COTENANT, VOID-ONDISK and VOID-SHORT. | p67:230-245; place38_parse.py:1470-1471, 1509-1518 | MATCHES |
| 36 | G1 witnesses: S1 rows need `scw=` and `plx=` on `[DIAG]` and `rlaw=` on `[SPAN]`; B0 rows carry none of them. `rsat=` and `pfec=` are not required on G rows. | s67_score.py:1325-1330 | MATCHES |
| 37 | W-ROUTE (rr = 0) is checked on G1 rows too, not only on G2. It reads the maximum rr over all `[SPAN]` ticks, not the median. | s67_score.py:1336-1337 | EXTENDS |
| 38 | A failed G witness voids the row (it is replaced through 41d's rule), unless the row is a DNF. | s67_score.py:1340 | EXTENDS |
| 39 | `GATE-67G1` / `GATE-67G2` read one of five classes (harness67e; the class is verdict text, nothing in the shell reads it). PASS needs GATE-41d PASS, no S06-FAIL row, no unreplaced witness failure, no G2-TAPER-NONZERO row outside S06-FAIL (#149) and no abort token. FAIL only for a finding: GATE-41d FAIL, an S06-FAIL row (the DNF rule's included, #41) or a G2-TAPER-NONZERO row. UNSCOREABLE ("re-run", never FAIL): an abort token, `S06-DNF-RULE NOT EVALUATED <cell>` (#41), a cell with no S2 row or S2 dropped (it also names score41d's NO VERDICT), an unreplaced witness failure, or a GATE-41d line that is none of PASS, FAIL and NO VERDICT. NO VERDICT: score41d's NO VERDICT with no other cause (e.g. too many voided slots). TRUNCATED: a cut ledger with none of the above. Order: FAIL, UNSCOREABLE, NO VERDICT, TRUNCATED, PASS. A FAIL line also names the UNSCOREABLE and NO VERDICT causes on the same ledger (#157). So only a finding can start §3.4's bisection or `LD_G2_FAIL67`; a void or an abort means re-run. §3.1 says "G1 PASS iff p > 0.05", with the witnesses "outside the min-p family". The old reading, "an abort token makes it FAIL", is struck (Proposed text #39). | s67_score.py:1440-1478 | EXTENDS (changed on harness67e) |
| 40 | In G2, an S2 (CAND) row of the right binary that prints `pslot=` and reads `taper > 0` or `plost > 0` is S06-FAIL, whatever its 41d label (infra, crash, WITNESS-FAIL) and whatever rr, and is never replaced. Stage A's mechanism precondition (pslot > 0, rr > 0) is not applied: G2 is Bulk, rr = 0 by design. A row with `taper > 0` and no `pslot=` fails the seat witness (WITNESS-FAIL, replaced once) and is G2-TAPER-NONZERO (#149). §4.1 registers `taper=0`; `plost` is added. Either reading is a finding: GATE-67G2 FAIL (#39). | s67_score.py:1333, 1343-1361; place38_parse.py:1135 | EXTENDS |
| 41 | The "DNF where every S1 row completes" S06-FAIL rule, registered for Stage A, is also applied to G2 (over BASE rows that are LIVE), and only to G2. A cell with no live S1 row prints `S06-DNF-RULE NOT EVALUATED <cell>` and makes the gate UNSCOREABLE (#39: nothing was found, re-run); a cell where an S1 row DNFs prints `S06-DNF-RULE not applicable`. A G2 ledger with no S2 rows in a cell, or with S2 dropped, reads `GATE-67G2 UNSCOREABLE (...)`, and names score41d's NO VERDICT beside it (round 3). | s67_score.py:1401-1459 | EXTENDS |
| 42 | An S06-FAIL row in G enters score41d under the 41d label it would otherwise have had. A label in 41d's replaceable list reads as VOID-RC. | s67_score.py:1364-1373 | EXTENDS |
| 43 | Resolution41d is not computed on the VM. It is computed on the host from the pulled `ld.log`. | p67:258 | EXTENDS |
| 148 | G block 1: after the first 8 rows of plan 41c's order (one group: BASE and CAND at each of the four cells), the first row of each of the 8 (cell, arm) pairs must be LIVE with no G witness failure (not WITNESS-FAIL, a void or S06-FAIL). Else `FAILED-STAGE67`, battery exit 6, `FAILED-<stage>`, and the next stage runs. Not on the smoke or the replacement run. A DNF first row with a witness failure also fails it. §5.2 reads "every arm's first real row". | battery:879-892; s67_score.py:1091-1111; p67:229 | EXTENDS |
| 149 | G2-TAPER-NONZERO: every G2 S2 (CAND) row of the right binary that the stage ran, voided originals and their replacements alike, with `taper > 0` or `plost > 0` is printed, whatever its label. One that is not S06-FAIL (no `pslot=`) fails GATE-67G2, so a clean replacement never clears it. It is a finding: FAIL class (#39). Stages A, R1, R2, the pool block and the dump twins read the same property on every S2-family row (#159). | s67_score.py:1343-1350, 1432-1435, 1458-1460 | EXTENDS |

### Stages A and A1

| # | decision | where | verdict |
|---|---|---|---|
| 44 | Row order: for each of 6 block indices b, the seed is 42 when b is even and 7 when b is odd. Inside a block index, the cells alternate (c2, then c8), each with (S1, S2, S1, S2, B0). S1 and S2 get reps 2k+1 and 2k+2, B0 gets rep k+1, with k = b // 2. | plan67.py:140-150 | EXTENDS |
| 45 | Instrument env on every plan-67 battery row: `RWM_RTOBS=1 RWM_RDIAG=1 RWM_DIAG=1 RWM_GEN=0`, `RWM_C_PIPELINE=window`. `RWM_DELTA`, `RWM_COPA_*`, `RWM_COMPLETION_EXPOSURE`, `RWM_MIN_R`, `RWM_CC_PACE` and `RWM_PERF_RATE_MBIT` are scrubbed before the arm env. | battery:178, 218, 570-577 | EXTENDS |
| 46 | ρ = 1 at Auto comes from the scrubbed `RWM_C_RELIABLE` (`perf_rwm_c.sh` passes `--window-reliable`). No row of A, the pool block or the dump twins checks `window_reliable: true`. | plan67.py:200-205; s67_score.py:407 | EXTENDS |
| 47 | A row runs once per attempt (`TRIES=1`): no bring-up retry. | battery:160 | EXTENDS |
| 48 | A void is replaced right after its row (inline), not at the end of the stage. The replacement's logs are `a11`/`a12`. | battery:1231-1241 | EXTENDS |
| 49 | Replaceable statuses: VOID-WITNESS, VOID-RHO, CONTAMINATED, VOID-COTENANT, VOID-ONDISK, WITNESS-FAIL, VOID-SHORT and NO_DATA; except that NO_DATA is replaceable on the reference arms only. On every arm of the S2 family (S2, P2, P2T, `S2P<pool>`, `S2D`) a NO_DATA row is never replaced: it stays a crash (VOID-RC counted apart, CANDIDATE-CRASH, #152), as plan 41d treats a candidate arm (harness67e; `plan67.replaceable`, used by the scorer's replacement pass and by the battery's inline `replace` helper). S1, A1's candidate arm (#152), is not in the S2 family, so its NO_DATA is still replaced. A crash (VOID-RC) is not replaced, and VOID-SEAT never is. §4.2 registers the replacement for a precondition-witness failure only. | plan67.py:233-238, 255; s67_score.py:451 | EXTENDS (changed on harness67e) |
| 50 | More than 4 voided slots in A (replaceable voids, infra ones included, and NO_DATA on the reference arms; a crash is not counted, and an S2-family NO_DATA is a crash, #49) reads AS and A1 NO VERDICT. §4.2 states the cap but not the outcome above it. | s67_score.py:463, 757-758 | EXTENDS |
| 51 | Status precedence: a sha problem (VOID-SEAT), then S06-FAIL, then infra and crash voids, then VOID-WITNESS, then VOID-SHORT, then VOID-RHO (R1), then LIVE. An S2 row whose slot preconditions hold and that reads `taper > 0` or `plost > 0` is S06-FAIL whatever the infra or crash label, and is never replaced. | s67_score.py:14-16, 391-409 | FIXED on harness67c (s67_score.py:396-398) |
| 52 | The S06-FAIL outcome is read once `pslot > 0` holds, and in A also the `[SPAN]` median rr > 0. The other precondition witnesses (e.g. the gauge prints) are not required for it. | s67_score.py:342-353 | EXTENDS |
| 53 | A DNF row is never VOID-WITNESS for a precondition: those failures are dropped and it stays LIVE at its worst rank. A pool-echo failure still voids a DNF row. | s67_score.py:401-403 | EXTENDS |
| 54 | VOID-SHORT applies to A, pool and dump rows that completed with their last `[DIAG]` tick under 30 s (`place41b_score.MIN_TICK_S`). | s67_score.py:404-406; place41b_score.py:47 | EXTENDS |
| 55 | A missing `LD_SHA_<SEAT>` in the parser's env makes the row VOID-SEAT (`bin-sha-uncheckable`). | s67_score.py:293-299, 391-393 | EXTENDS |
| 56 | The precondition witnesses also void a row for `pslot=` printed on a seat outside the S2 family. | s67_score.py:311-312 | EXTENDS |
| 57 | Row readings: counters come from the last `[DIAG]` line (`cum=` gives src and cod), below-floor is the maximum `below_floor` over the receiver's `[RDIAG]` lines, and `fill_coded` comes from the `[RFA]` line with the largest `src_n`. | s67_score.py:190-203, 244-251 | EXTENDS |
| 58 | The UNSCOREABLE minimum counts LIVE rows only (a LIVE DNF counts; an S06-FAIL row counts through its underlying label). Crash rows (VOID-RC; NO_DATA after replacement, or at once on the S2 family, #49) and other voids do not count; they are counted apart (#151, #152). | s67_score.py:517-520, 731-736 | FIXED on harness67c (s67_score.py:517-520) |
| 59 | A1's minimum reads only the arms of its test: at least 10 live S1 rows and 4 live B0 rows (`MIN_LIVE_B0`) per cell. An S2 shortfall makes AS UNSCOREABLE and leaves A1 scored; A1's rank test reads B0 against S1, stratified within seed per cell. §4.2: "fewer than 10 live rows on S1 or S2 after replacement is UNSCOREABLE for AS and A1". The B0 minimum is not registered. | plan67.py:60; s67_score.py:20-22, 722-736 | CONTRADICTS |
| 60 | Verdict precedence for AS and A1: an abort token, then the minimum (CANDIDATE-CRASH first, #152), then the replacement cap, then the test, then truncation (#137). Any 41d abort token (ABORT-LOCK, -CRLF, -SHA, -BIN, -SENTINEL-UNWRITABLE, -SMOKE) in the ledger reads UNSCOREABLE. | s67_score.py:747-759, 775, 796; place38_parse.py:88 | EXTENDS |
| 61 | An S06-FAIL row is scored in AS and A1 under the label it would otherwise have had. A replaceable or infra label reads as a crash (VOID-RC, worst rank). | s67_score.py:466-474, 710 | EXTENDS |
| 62 | A crash row (VOID-RC, NO_DATA) fills a worst rank in the rank statistic (`with_crash=True`). | s67_score.py:570 | EXTENDS |
| 63 | AS's null is the guard's stratified Monte Carlo (10 000 relabellings, seed 4141). §9 says "exact null at small n". | s67_score.py:761-774 | EXTENDS |
| 64 | A1's exact null is stratified within seed (per cell C(9,3)² = 7 056 labellings) and combines the cells as 1 − Π(1 − F_c). It is not the unstratified C(18,6). It falls back to Monte Carlo above 200 000 labellings per cell. | s67_score.py:51, 566-597, 779-795 | EXTENDS |
| 65 | AS prints its realised d80 as the q95 of the S1 half-splits per seed. | s67_score.py:769-771 | EXTENDS |
| 66 | Below-floor flag: H-B's level is 854 at c2 (registered) and 122 at c8, taken from `docs/l1-raw/s51b/recopy/acct-c8.txt`. The cell flag compares the S2 median with 0.25 × the S1 median and with the median `fill_coded` + 2 × H-B. The per-row flag uses the row's own `fill_coded`. | s67_score.py:47, 606-621 | EXTENDS |
| 67 | The CPU owner flag: "worse" is the relative difference of the per-GB medians (DNF rows excluded) above d80, AND a one-clause rank p ≤ 0.05 for that cell from the guard's Monte Carlo. | s67_score.py:662-672 | EXTENDS |
| 68 | The c8 in-order delay is printed as the relative median difference with no test. §4.2 says "two-sided". The c2 in-order delay is printed per arm only, with no S2-against-S1 line. | s67_score.py:649-650, 657-661 | EXTENDS |
| 69 | `[SUCC]`: the last `[SUCC]` line on the server; `rep_*` is read as decode-resolved and `orig_*` as resend-resolved. A row has "both close times" if `rep_p50_us` and `orig_p50_us` are present (p90 is not required). | s67_score.py:236-241 | EXTENDS |
| 70 | `[DIAG] retx` and `[RFA] fill_coded` are printed as raw per-arm medians, not per source. | s67_score.py:652 | EXTENDS |
| 71 | Rate readouts: the shares are pooled over all `rlaw=` ticks. The rsat/rr/pfec quantiles pool at most 400 strided samples per row. β is the median of the per-row medians. The r_sat bind fraction is the share of ticks whose anchor-term bound is `sat`. | s67_score.py:74-84, 216-233, 624-639 | EXTENDS |
| 72 | Cost split: consumed r = (cod + taper) / src, coded share = cod / (src + cod), taper share = taper / (taper + cod), each from the last `[DIAG]` cum and taper. | s67_score.py:197-200, 678-681 | EXTENDS |
| 73 | t_tail is printed as the constant 1·10⁻⁵, labelled DERIVED. | s67_score.py:50, 639 | MATCHES |
| 151 | MINIMUM-COUNTS: when any S1, S2 or B0 cell is under its minimum, Stage A prints one `MINIMUM-COUNTS` line with the live count (a LIVE DNF counts) and the crash count (VOID-RC and NO_DATA after replacement) per arm and cell (S1, S2, B0; S1 and B0 under the drop). Each UNSCOREABLE-minimum verdict names its short cells as `<cell> <arm> <live><<min> (crashes <k>)`. | s67_score.py:492-507, 731-740, 756 | EXTENDS |
| 152 | The candidate-crash rule. AS's candidate arm is S2, A1's is S1, R2-HOLD's is P2T. When a candidate cell (R2: the twin) is under its minimum and holds any crash row, the verdict reads `<AS or A1> UNSCOREABLE (CANDIDATE-CRASH <arm> n=<k> in <cell>: a finding to investigate, not cleared by a plain re-run)`, plus ` (live + crashes <n> < minimum <m>: other voids also short)` when the crashes alone do not account for the shortfall (round 3: before, the rule needed live + crashes to reach the minimum), with any other short cells after `; also under the minimum:`. Crashes in a candidate cell that meets its minimum are printed as `CANDIDATE-CRASHES (reported; worst rank in the test, not a shortfall): <arm> <cell> n=<k>`. R2: `R2-HOLD UNSCOREABLE (CANDIDATE-CRASH P2T n=<k> in twins: ...)`, after an abort token, R2-BLIND and the replacement cap; a P1T shortfall alone is plain UNSCOREABLE naming the live and crash counts. R1 has no live minimum, so the rule does not apply there. | s67_score.py:18-23, 497-503, 738-770, 924-940 | EXTENDS |

### Stage R1

| # | decision | where | verdict |
|---|---|---|---|
| 74 | Order and arms as A at c2 only (30 rows), Realtime, 100 MB, 150 s, ρ = 1 witnessed by `window_reliable: true` on both ends (VOID-RHO). A Realtime row that lacks the reliable-window line is not voided for it (plan 59's rule). | plan67.py:68, 183-184, 206-207; s67_score.py:364-368, 407-409 | MATCHES |
| 75 | The R1 replacement cap is 2 slots (plan 64's stage R). More than that reads NO VERDICT. | plan67.py:63, 228-229; s67_score.py:874 | EXTENDS |
| 76 | A B0 R1 row carrying a gauge print is not voided: it stays LIVE and fails clause A1-R ("no B0 row does"). | s67_score.py:306, 835 | FIXED on harness67c (s67_score.py:306, 835) |
| 77 | "Completes" means status LIVE or S06-FAIL and no DNF. An unreplaced VOID-* row (VOID-WITNESS, VOID-RHO, CONTAMINATED, ...) is no reading: it leaves the completion sets of A1-R and R1 and is not a non-completion (harness67e). A crash (VOID-RC, NO_DATA) is a row that did not complete. So an S1 crash row can fail A1-R, and an S2 crash row with its preconditions met can fail R1; an S2 void can no longer fail R1. Too few S1 rows with a reading is UNSCOREABLE (#158). The default of 2026-11-27 ("an S2 VOID-RHO or crash row can fail R1") is replaced. | s67_score.py:830-848 | EXTENDS (changed on harness67e) |
| 78 | §4.3 registers the S1 `taper > 0` witness, but the code never voids an S1 row for it. R1-BLIND is `taper = 0` (a missing value counts as 0) on more than half of the LIVE S1 rows (harness67e): voids and crashes are not in the count. The default of 2026-11-27 ("void S1 rows count toward R1-BLIND") is replaced. | s67_score.py:842-844 | EXTENDS (changed on harness67e) |
| 79 | An S2 row with `pslot = 0` or no `pslot=` is VOID-WITNESS. An S2 row with `pslot > 0` and `taper > 0` or `plost > 0` is S06-FAIL (`plost` is added to §4.3's `taper = 0`). | s67_score.py:309-310, 323-324, 342-353 | EXTENDS |
| 80 | Verdict precedence: an abort token, no S1 or S2 row, INCOMPLETE (#138) (each UNSCOREABLE), the replacement cap (NO VERDICT), then R1 FAIL (or A1-R FAIL), then the S1 shortfall (UNSCOREABLE, #158), R1-BLIND, R1 PASS, then truncation (#137). A blind stage reads R1-BLIND even when the clause holds, unless the S1 shortfall applies. There is no live minimum. | s67_score.py:867-882 | EXTENDS |
| 81 | R1 reports per arm goodput, `cpu_cli/GB`, `cpu_srv/GB`, `lag_E_cli`, `lag_E_srv` and the in-order delay mean and p95, marked under the 30 s tick floor. | s67_score.py:854-861 | FIXED on harness67c (s67_score.py:854-861) |
| 138 | INCOMPLETE: A1-R and R1 read `UNSCOREABLE (INCOMPLETE: <k> of <n> registered slots hold no row: ...)` when a registered slot has no row. The registered slots follow the ledger's drop state (#153). It comes after an abort token and the no-row check, before the replacement cap. Stage A has no such check: its missing slots fall to the minimum and to the stage's `check` (#113). Stage T reads a ledger with no `T67DONE` the same way (`T UNSCOREABLE (INCOMPLETE: no T67DONE)`, #100). | s67_score.py:825-827, 867-875 | EXTENDS |
| 158 | The S1 shortfall rule of R1 and A1-R (harness67e). "Registered N" is the registered S1 slots of the stage (12; `n_s1`, line 833). A crash row (VOID-RC, NO_DATA) is a row with a reading; an unreplaced VOID-* row is not (lines 831-832). If fewer than N S1 rows have a reading and the clause would otherwise pass, the verdict reads `UNSCOREABLE (unreplaced void rows leave <k> of the registered 12 S1 rows with a reading: re-run)`: for R1 (before R1-BLIND is read) and for A1-R alike. A failing clause still reads FAIL first (#157). B0 voids have no shortfall rule: they leave A1-R's B0 set, and "every B0 row completes" is read over the rest (false when none is left, which skips the comparison). S2 voids leave R1's hold set with no shortfall rule. Slots with no row at all are INCOMPLETE (#138), which comes first. | s67_score.py:831-837, 846-848, 877-879 | EXTENDS |

### Stage R2 and the twins

| # | decision | where | verdict |
|---|---|---|---|
| 82 | R2 rows take plan 51e's PM spec: 262 500 000 bytes, timeout 75 s, `RWM_SLAW=1 RWM_C_RELIABLE=0 RWM_PERF_RATE_MBIT=35`, Realtime. | plan67.py:208-210; plan51e.py:34-37, 89-110 | EXTENDS |
| 83 | Order: 6 groups; group g has seed 42 when g is even and 7 when odd, rep g // 2 + 1. Each group runs (P1, P2, P1T, P2T), reversed on odd groups to (P2, P1, P2T, P1T). | plan67.py:153-160 | EXTENDS |
| 84 | The pacer seats' rows run `perf_rwm_c.sh` from that seat's own `tools/l1` (`LD_TOOLS_<SEAT>`). | p67:101, 190; battery:422, 569 | EXTENDS |
| 85 | Every R2 row needs the pacer echo, the client gate at 35, the server gate unset or 35, and a `"pace"` JSON line, else VOID-WITNESS. P1 `taper > 0` and P2 `pslot > 0` with rr > 0 are checked on the smoke row and, as R2-BLIND (#139), on the scored rows; they never void a row. | s67_score.py:325-338, 902-909, 1056-1059 | EXTENDS |
| 86 | Twin witness: §4.4 says "a row with neither is VOID". The code voids a twin row when either line is missing. | s67_score.py:328-329 | EXTENDS |
| 87 | The R2 replacement cap is 4 slots. More than that reads R2-HOLD UNSCOREABLE, naming the live and crash twin counts and the cap. | plan67.py:228-229; s67_score.py:938-939 | EXTENDS |
| 88 | The battery keeps one `ACCT-KEPT` line per kind in the ledger: the last periodic `[ACCT]`, the last `final=1` `[ACCT]` and the last `[ACCT-RX]`. The scorer reads the account from the raw endpoint log at row time; the kept lines serve only the rehearsal's kept-log check. | battery:590-595; s67_score.py:164-174, 257-268; plan67.py:280-303 | EXTENDS |
| 89 | U = O_serv + O_strand. A missing field leaves U unread (None), never 0. | s67_score.py:261-262 | EXTENDS |
| 90 | A twin row counts toward the minimum of 5 (`plan67.MIN_TWIN`) only if it is LIVE and has an `[ACCT]` reading. The `LIVE TWIN ROWS` line prints each twin's live and crash counts. Mixed labels (harness67e): the mix is judged across all counted twin rows (P1T and P2T together). When `final=1` and `ACCT-LAST` readings are both present, U is read on the `final=1` rows only, and only with at least `MIN_TWIN` (5) such rows per seat; with fewer, U is unread and R2-HOLD reads UNSCOREABLE ("mixed account labels") unless (ii) fired. `oldest_unchanged_max` is read on every counted row, whatever its label. A FIRED outranks an unread U. A `MIXED account labels` line and the earlier NOTE are printed. The default of 2026-11-27 ("one rank test over both labels") is replaced. | s67_score.py:911-925, 944-968; plan67.py:61 | EXTENDS (changed on harness67e) |
| 91 | If a counted twin row lacks U or `oldest_unchanged_max`, R2-HOLD reads UNSCOREABLE and guard 2 NOT ELIGIBLE ("never read as 'did not fire'"). | s67_score.py:929, 940-941 | MATCHES |
| 92 | `shed_total(P2) − shed_total(P1)` is computed per P2 row against the median of the P1 rows of the same seed (and the same for the twins). | s67_score.py:977-984 | EXTENDS |
| 139 | R2-BLIND: on the scored R2 rows, P1 needs `taper > 0`, and P2 needs `pslot > 0` with the `[SPAN]` median rr > 0 (§4.4's smoke witness). More than half of a seat's rows failing it prints `R2-BLIND <seat>`, and R2-HOLD reads `UNSCOREABLE (R2 VOID: the mechanism did not run; guard 2 waits for an R2 re-run; blind seat <S>); guard 2 NOT ELIGIBLE`. It comes after an abort token and before the twin minimum. | s67_score.py:902-909, 930-933 | EXTENDS |

### Stage T

| # | decision | where | verdict |
|---|---|---|---|
| 93 | Transfer order: hint auto then realtime; inside a hint, B0 then S1. Each invocation uses `tun_bulk.sh c2 <hint> 100000000 5`, seed 42, `TB_RUST_LOG=info`, and a comment-only default TOML. | tun67_battery.sh:37, 56-75 | MATCHES |
| 94 | Inherited `tun_bulk.sh` defaults: 3 bring-up attempts, each gated by up to 20 pings (1 s wait plus 1 s sleep, so about 40 s), a 90 s timeout per transfer (`RWM_TB_TIMEOUT_S`). | tun_bulk.sh:37, 70-93 | EXTENDS |
| 95 | Refusal order: S1 first, then B0; for each key, `check` then `run`. `check` has a 20 s timeout. `run` is a throwaway server on `127.0.0.1:7999`, TUN `rptref0` `10.98.0.1/24`, `--protocol-hint auto`, killed after 8 s. Both run with `RUST_LOG=info`. | tun67_battery.sh:79-92 | EXTENDS |
| 96 | Parsing: `cfg` is the key's value on the last `resolved configuration` line; `err` is the first non-empty stderr line, ANSI-stripped. The refusal text is compared with that one line, after an optional leading `Error: ` is removed. | tun67_battery.sh:93-94; s67_score.py:1208-1212 | EXTENDS |
| 97 | B0's four `check` commands are run and printed, never scored. | tun67_battery.sh:9; s67_score.py:1199 | EXTENDS |
| 98 | A B0 `run` that prints the guard text stops the stage at once (the battery writes `ABORT-SEAT ...` to the ledger, exit 5). The scorer matches that ledger line as an abort token, so the stage reads `T UNSCOREABLE (abort ABORT-SEAT: B0 run <key>: stderr carries the guard text)` (#100). The text `T ABORT-SEAT (B0 is not main's ...)` is reachable only on a ledger that holds the guard text in a `T67REF` line but no `ABORT-SEAT` line. | tun67_battery.sh:96-99; s67_score.py:1182-1188, 1198, 1261-1264 | MATCHES |
| 99 | A transfer "completes" if its `TUNREP` JSON has `mbps` or `seconds` and no `nodata`. | s67_score.py:1162 | EXTENDS |
| 100 | Verdict precedence (harness67e): an abort token (`ABORT-SHA`, `ABORT-SEAT`, `BUSY`, `ABORT-SENTINEL`, `ABORT-LIB` at a line start: `T UNSCOREABLE (abort ...)`), then B0's own refusal reading (`T ABORT-SEAT`), then no `T67DONE` (`T TRUNCATED` if the ledger holds the `LD_HARD` sentinel, else `T UNSCOREABLE (INCOMPLETE: no T67DONE)`), then clause 1 (`T FAIL`), then a bring-up failure (ABORT-BRINGUP-T, #150), then clauses 3 and 4 (`T FAIL`), then a missing S1 refusal line (UNSCOREABLE), then clause 2 (`T VOID`), then PASS (TRUNCATED on a cut ledger). `T FAIL` needs a refusal line that is present and wrong, or a failing clause 3 or 4; a missing line is no reading. A finding outranks a void; an abort or a missing stage outranks both. In the real battery `ABORT-LIB` and `ABORT-SENTINEL-UNWRITABLE` are echoed to stdout, outside the ledger, so in practice they read INCOMPLETE. | s67_score.py:1182-1188, 1198-1213, 1259-1280 | EXTENDS (changed on harness67e) |
| 101 | `TUN-BRINGUP-FAIL` is read: a bring-up failure on either seat gives `ABORT-BRINGUP-T seat=<S> hint=<h>` as the T result, and the envelope does not count the stage ok. | s67_score.py:1125-1171, 1194-1196, 1270-1271; p67:253; tun67_battery.sh:14-16 | FIXED on harness67c (s67_score.py:1194-1196, 1270-1271; p67:253) |
| 102 | T has no replacement. It is ok if the battery exits 0, the ledger has `T67DONE` and no `TUN-BRINGUP-FAIL`. | p67:252-253 | EXTENDS |
| 150 | Bring-up attribution: a `TUN-BRINGUP-FAIL` line is attributed by its own `label=<seat>-<hint>`, else to the invocation of the last `T67SEAT` line, else read as `ABORT-BRINGUP-T unattributed (...)`; it is never dropped. A ledger without `T67DONE` reads `T UNSCOREABLE (INCOMPLETE: no T67DONE)`, or `T TRUNCATED` on a cut ledger, unless an abort token or B0's ABORT-SEAT comes first; it outranks clause 1 and the bring-up abort (#100). | s67_score.py:1125-1171, 1185-1196, 1265-1267; tun_bulk.sh:92 | EXTENDS |

### Pool block and dump twins

| # | decision | where | verdict |
|---|---|---|---|
| 103 | Pool order: rep 1 to 3; inside a rep, pools 170, 256, 640; inside a pool, (S1, S2) on odd reps and (S2, S1) on even reps. Arms are named `S1P<pool>` and `S2P<pool>`. | plan67.py:163-171 | EXTENDS |
| 104 | Pool and dump rows take A's precondition witnesses (the four gauge prints on S1 and S2, none on B0, `pslot=` only on S2) and are replaced at cap 4. Pool rows also need `RWM_STORE_PATH_POOL` echoed on both ends. | s67_score.py:305-314, 380-386; plan67.py:228-229 | EXTENDS |
| 105 | Pool readouts: completion is the row's wall time, or 200 s for a DNF; duplicates are the `[C8CONV-R]` `dup=` sum and `[RFA] dup_src`. | s67_score.py:252-253, 993-1013 | EXTENDS |
| 106 | Dump twins: 8 rows ordered cell, then seed, then seat (arms `S1D`, `S2D`), A's spec plus `RWM_SUCC_DUMP=1`. They report the `[SUCCDUMP]` record counts, with no minimum record count. | plan67.py:174-176, 213-214; s67_score.py:1016-1024 | EXTENDS |
| 107 | The dump twins run only with `LD_DUMP67=1`, set by hand. The rehearsal writes its advice (`DUMP67=1` or `0`) into `AMENDMENT2.txt`, from `DUMP-TWINS-NEEDED` in `sa/smoke-check.txt`. | plan67.py:104-105, 115; p67:288 | MATCHES |
| 108 | `LD_CUT67` accepts only `dump`; any other value is ABORT-CUT. | p67:27 | MATCHES |

### Stage control and the block-1 rule

| # | decision | where | verdict |
|---|---|---|---|
| 109 | Block-1 sizes: G = 8 rows (#148), A = 10 rows (block index 0, both cells), R1 = 5, R2 = 4 (group 0), pool = 6 (rep 1). The dump twins have none. | plan67.py:222-225; s67_score.py:1091 | EXTENDS |
| 110 | Block-1 (A, R1, R2, pool) takes each arm's first row in the ledger. A first row that was void and then replaced (inline, #48) still fails the rule, and so does a first-row S06-FAIL. For A, the c8 cell's first rows are not checked. (G reads each (cell, arm) pair, #148.) | s67_score.py:1069-1088 | EXTENDS |
| 111 | G1 and G2 now run block 1 (#148). T and the dump twins have none. §5.2: "after each stage's first block, every arm's first real row must be LIVE". | battery:879-892, 1249-1258; tun67_battery.sh:65-102 | CONTRADICTS |
| 112 | A stage battery starts only with 1 800 s left before `LD_HARD`; otherwise it is not started: `ABORT-BUDGET stage <s>: ... not started`, a `NOT-STARTED` marker, stage return 8 (#154). Its timeout is the time left to `LD_HARD`. A battery killed at `LD_HARD` gets the truncation sentinel (#136) and is scored as far as it got. | p67:220-229 | EXTENDS |
| 113 | A, R1, R2, pool and dump are ok only if `LD-BATTERY-DONE` is present and the set of slots equals the table. | p67:254-255; plan67.py:269-277 | EXTENDS |
| 114 | Every stage runs even after an earlier one failed. A stage that is not ok, other than cut or not started, ends the session `FAILED-ALL` ("stages not run to the end", naming any cut stages too), after the summary `score.txt` is written. Cut or not-started stages alone end it TRUNCATED-ALL (#154). | p67:300-324 | EXTENDS |
| 115 | `LD_G2_FAIL67=1` sets `LD_DROP_S2=1` and removes P1, P2, P1T and P2T, R2, the pool block and the dump twins; A, R1 and T keep their S1 and B0 arms. §5.2 drops "Stage A's S2 arm, R1's S2 arm and R2". The pool block and the dump twins also lose their S1 arms. | p67:24-25, 32; plan67.py:116-117 | EXTENDS |
| 116 | A seat failure in the S2 family (S2, P2, P2T) aborts the whole session with "relaunch with LD_DROP_S2=1", session 0 included (#147). Guard 1 alone then needs a relaunch; it does not go on in the same session. | p67:33-39 | EXTENDS |

### Rehearsal and stub runs

| # | decision | where | verdict |
|---|---|---|---|
| 117 | Plan 67 needs `LD_REHEARSE=1`, and only plan 67 accepts it (67pre, 67g and 67x refuse it). | p67:23; run_all:190-191 | EXTENDS |
| 118 | Rehearsal stages: g1, g2, a, r1, t, r2. The pool block and the dump twins get no smoke. | plan67.py:81 | MATCHES |
| 119 | G's smoke runs twice (g1, g2), each with plan 41d's three smoke rows (c1s BASE, c1s CAND, c8 600 MB CAND). That is six rows: c1s B0, c1s S1, c8 S1, c1s S1, c1s S2, c8 S2. §6.1 lists three. | s67_score.py:1492; place38_parse.py:73 | EXTENDS |
| 120 | The smoke rows are A: one c8 row per seat; R1: one c2 row per seat; R2: one row per pacer seat and twin; all at seed 42, rep 1, in the order S1, S2, B0 (A, R1) and P1, P2, P1T, P2T (R2). | plan67.py:179-186 | MATCHES |
| 121 | A rehearsal smoke runs under `timeout` at min(1 800 s, the time left to `LD_HARD`). No time left, or a timeout, is ABORT-SMOKE with the reason. | p67:202-207 | EXTENDS |
| 122 | A smoke row fails unless it is LIVE with its preconditions met, so a failed S2 outcome witness (S06-FAIL) fails that smoke. In g2, A and R1, a failure that names the S2 family alone makes guard 2 VOID instead of stopping the rehearsal (#160, harness67e); any other smoke failure is ABORT-SMOKE and stops it (#124). §6.1 reads a failed S2 outcome witness in the rehearsal as "a finding"; the rows are kept and the ABORT-SMOKE marker stays either way. The default of 2026-11-27 ("it aborts the rehearsal") is replaced. | s67_score.py:1047; plan67.py:306-319 | EXTENDS (changed on harness67e) |
| 123 | The `[SUCC]` close-time check in the A smoke is INFO: missing close times print DUMP-TWINS-NEEDED, and the smoke does not fail. | s67_score.py:1049-1051 | EXTENDS |
| 124 | The rehearsal stops at the first smoke that fails (`LD-ALL FAILED`): no later smoke runs, and no `AMENDMENT2.txt` is written. Two failures do not stop it: an R2 smoke failure (#126) and an S2-family-only failure in g2, A or R1 (#160). Each marks its guard VOID, lets the other smokes run, writes `AMENDMENT2.txt` and ends with exit 7. A failure that names S1 or B0 still stops it. | p67:267-284 | EXTENDS (changed on harness67e) |
| 125 | The R2 smoke adds the kept-log check: every twin row needs `ACCT-KEPT side=cli` with `[ACCT]` and `side=srv` with `[ACCT-RX]` in the ledger. | p67:212; plan67.py:280-303 | MATCHES |
| 126 | §4.4: "A failure [of the R2 smoke witness] makes R2 VOID; it goes back to the coordinator". In the rehearsal an R2 smoke failure now keeps ABORT-SMOKE in the stage dir, writes `R2-VOID` and `REHEARSAL-R2-VOID`, lets the other stages' smokes run, writes `R2 VOID` into AMENDMENT2.txt and ends `LD-ALL-REHEARSAL-R2-VOID`, exit 7 (`P67_EXIT_NONPASS`: neither DONE nor FAILED-ALL; guard 1 may launch). Every other smoke failure is still ABORT-SMOKE and FAILED-ALL, except an S2-family-only failure in g2, A or R1 (#160). 67x reads the same witness on the scored rows (R2-BLIND, #139). | p67:167, 270-275, 291, 296-297; s67_score.py:902-909, 932-933 | FIXED on harness67c round 3 (p67:270-275, 296-297) |
| 127 | `AMENDMENT2.txt` holds the seven seats' binary shas with their commits, the dump-twin advice, the S1/S2 suite-reuse HIT list, the sha of `TOOLS-MANIFEST.sha256` and, after an R2 smoke failure, the line `R2 VOID (rehearsal smoke failed: guard 2 waits for an R2 re-run)` (round 3, #126) or, after an S2-family-only smoke failure in g2, A or R1, the line `REHEARSAL-G2-VOID (an S2-family smoke failed in g2 / A / R1: guard 2 is VOID until the S2 seat is fixed and the rehearsal re-run; guard 1 may launch)` (harness67e, #160). The binid shas and repeat counts (§6.1) come from session 0's `SESSION0.txt`. | p67:285-294; p67pre:200-209 | EXTENDS |
| 128 | Stub fast mode (`test_stubrun.py`) shrinks G_ROWS67 to 16, N_BLOCKS to 2, N_TWIN_REPS to 2, N_POOL_REPS to 1, MIN_LIVE_A to 2, MIN_LIVE_B0 to 1 and MIN_TWIN to 2. `STUBRUN_FULL=1` keeps the registered values. Other test seams: `LD_HARD_MIN_S`, `R67_T1_S`, `LD_ROW_BYTES`, `LD_BLOCK_ROW_S`, `R67_N`, `R67_BUDGET_S`, `R67_SIM`. Every one of them is ABORT-SEAM without `LD_TEST_SEAMS=1` (#140). | test_stubrun.py:298-304; plan67.py:71-73, 261-266; repeat67_run.sh:95 | EXTENDS |
| 160 | The S2-family smoke failure (harness67e), as #126 for R2. After a smoke failure (stage rc 1, `ABORT-SMOKE` kept), `plan67.py s2-smoke-fail <stage> smoke-check.txt` decides by a string match on the `SMOKE-FAIL` lines. In g2: the lines name `CAND` and no `BASE`. In A and R1: the items (split at `;`, the text before the first `:`) name `S2` and neither `S1` nor `B0`. Other stages (g1, r2, t) never match. On a match the stage gets a `G2-VOID` marker (the `ABORT-SMOKE` stays), the other smokes run, `AMENDMENT2.txt` gets `REHEARSAL-G2-VOID`, and the session ends `LD-ALL-REHEARSAL-G2-VOID`, exit 7, with the sentinel `REHEARSAL-G2-VOID` (no DONE, no FAILED-ALL; guard 1 may launch). A BASE (S1) failure in g2 still stops the rehearsal, whatever else it holds. Both VOIDs in one rehearsal print both lines and exit 7 once. The match is on text, so any `CAND` item without a `BASE` item (a missing row, a wrong binary) voids guard 2, not only a taper finding. §6.1: "a failed S2 outcome witness in the rehearsal is a finding"; §5.2: "On S2, it stops guard 2. Guard 1 still launches". | plan67.py:306-319, 371-372; p67:265, 276-282, 292, 295-297; run_all:336 | EXTENDS |
| 161 | Stub paths after the verdict classes (harness67e). The `plan 67g` path `g1-fail` (S1 rows without the gauge prints on every draw, so replacements fail too) reads `GATE-67G1 UNSCOREABLE` (re-run), no longer FAIL; G2 still runs. A genuine "S1 slower" G1 FAIL has no end-to-end stub path: the stub cannot make S1 slower than B0, and nothing in the shell keys on FAIL. It is covered by the scorer unit test `g_class_green` (G2) only (`test_g_verdict_classes` in `test_s67.py`); `g_class_red` pins UNSCOREABLE and NO VERDICT. The rehearsal path `s2-taper-nonzero` now ends `LD-ALL-REHEARSAL-G2-VOID` (#160); the 67g path of that name is still GATE-67G2 FAIL. | test_stubrun.py:3271-3278, 3327-3328, 3337; harness_paths.py:176, 179; test_s67.py:1214-1240 | EXTENDS |

### Aborts and exit strings

| # | decision | where | verdict |
|---|---|---|---|
| 129 | Tokens beyond §5.4's list: ABORT-SEAM, ABORT-REHEARSE, ABORT-CUT, ABORT-SHM, ABORT-SUITES, ABORT-BUDGET, ABORT-ROWS, ABORT-SHA, ABORT-BIN, ABORT-PLAN67, ABORT-PRECONDITION, ABORT-BRINGUP-T, `FAILED-<stage>` / FAILED-STAGE67, BUSY, BINID67 FAIL; the session outcome TRUNCATED-ALL and the stage marker NOT-STARTED; and the verdict tokens TRUNCATED, INCOMPLETE, R2-BLIND, CANDIDATE-CRASH, G2-TAPER-NONZERO, `S06-DNF-RULE NOT EVALUATED`. Round 3 adds the rehearsal outcome REHEARSAL-R2-VOID (with the stage marker R2-VOID; exit 7), the reported line CANDIDATE-CRASHES (not a verdict) and the session lines NONPASS-IN-CUT-STAGE; harness67d adds the repeat failure causes TARGET-MISSING(<t>), ZERO-TESTS(<t>), SIM-RC<k>, DET-RC<k>, SIM-NO-RESULT<k> (all inside ABORT-REPEAT). Harness67e adds the verdict classes `GATE-67G UNSCOREABLE` and `NO VERDICT` (#39), `T UNSCOREABLE` (#100), the lines `S2-TAPER-NONZERO` and `GUARD2-ELIGIBILITY NOT ELIGIBLE` (#159), and the rehearsal outcome REHEARSAL-G2-VOID with the stage marker G2-VOID (#160, exit 7). Findings that do not abort: GATES-B0-FINDING, GATES-FINDING-ON-MAIN, BINTEST-FINDING-ON-MAIN. ABORT-PREFLIGHT, listed in §5.4, is now emitted by session 0 (#141-#145). | run_all:146; p67:23-27, 66, 144, 221, 251, 323; p67pre:27, 31, 81, 109, 185, 192; battery:296, 305, 888, 1248, 1256; tun67_battery.sh:52-53; s67_score.py:754, 873, 909, 935, 1194-1196, 1434 | EXTENDS |
| 130 | Order in practice: ABORT-SEAM fires first, before the locks. Then ABORT-PRECONDITION, the session budget (ABORT-BUDGET), and ABORT-TESTS from the tools-test gate (#20), before any seat check. The seat checks then go commit pin (ABORT-SEAT); in 67pre the session-0 steps follow (#141); otherwise build and sha (ABORT-BUILD), identity strings and the tools-diff witness (ABORT-SEAT), suite reuse (ABORT-SUITES), then the tools tests again (ABORT-TESTS). §5.4 orders ABORT-SEAT before ABORT-BUILD before ABORT-TESTS. | run_all:144-147, 351-366, 391-397, 455-480; p67:51-161 | EXTENDS |
| 131 | Exit strings: `LD-ALL-SESSION0-DONE` (exit 0), `LD-ALL-REHEARSAL-DONE` (exit 0), `LD-ALL-DONE` with `DONE-ALL` (exit 0), `LD-ALL-TRUNCATED` with `TRUNCATED-ALL` (exit 7, #154), `LD-ALL-REHEARSAL-R2-VOID` with `REHEARSAL-R2-VOID` (exit 7, #126), `LD-ALL-REHEARSAL-G2-VOID` with `REHEARSAL-G2-VOID` (exit 7, #160), `LD-ALL FAILED: <cause>` with `FAILED-ALL` (exit 5). Stage markers: `ABORT-SMOKE`, `G2-VOID`, `R2-VOID`, `SMOKE-PASS`, `REHEARSAL-PASS`, `TRUNCATED`, `NOT-STARTED`, `FAILED-<stage>`, `DONE`. A battery's exit 6 is the block-1 stop. | p67:203-219, 221, 227-229, 259-260, 298, 321-324; p67pre:211-212; run_all:336, 343 | EXTENDS |

### Scoring, parsers and env (across stages)

| # | decision | where | verdict |
|---|---|---|---|
| 132 | Stage T is parsed by `s67_score.py`'s own `tun_parse`, not by `verify4_parse.py tun`. §9 item 5: "parsed with `verify4_parse.py tun`". The parser now reads `TUN-BRINGUP-FAIL` (#101), and a test checks that it and `verify4_parse.tun_arms` agree on a clean, a no-data and a bring-up-failure ledger. | s67_score.py:1125-1171; test_s67.py:805-820 | CONTRADICTS |
| 133 | The copy and slot counters and the below-floor count are re-implemented in `readings()`. §9 item 2 says "as `s51_score.py` reads them" and "as `acct.py` reads it"; neither module is imported. In-order delay does use `threadp1_parse.inorder_delay_stats`, as §9 says. | s67_score.py:39, 177-285, 369 | EXTENDS |
| 134 | Envelope to battery env (`sudo env`): `LD_OUTDIR`, `LD_HARD`, `LD_REHEARSE`, `LD_DROP_S2`, `LD_DUMP67`, `LD_WITNESS54=0`, `LD_WITNESS64=0`, plus `LD_BIN`/`LD_SHA` (S1's for every non-G stage), `LD_PLAN`, `LD_SEATS67`, `LD_BIN_<S>`, `LD_SHA_<S>` and `LD_TOOLS_<S>`. `LD_BLOCK_ROW_S` and `LD_ROW_BYTES` pass only under `LD_TEST_SEAMS=1`. For G: CAND and BASE binaries and shas, `LD_SHA_G_CAND`, `LD_SHA_G_BASE`, `LD_BASE_SHA`, `LD_PLAN=41d`, `LD_WITNESS67`. T gets a reduced list: `LD_OUTDIR`, B0's and S1's binary and sha, `LD_HARD`, `LD_DROP_S2`. | p67:183-199 | EXTENDS |
| 135 | Launches (67g, 67x) run no smoke before a stage's battery. Only the rehearsal smokes, so each launch rests on the rehearsal's smoke of the same binaries. | p67:200-219 | EXTENDS |
| 136 | The truncation sentinel: a battery, or G's replacement run, killed by `timeout` at `LD_HARD` (rc 124) gets `LD-BATTERY-TRUNCATED at LD_HARD <epoch>` appended to its ledger and a `TRUNCATED` marker file. The stage is still scored, then returns 7 (#154). A G replacement run with under 900 s left is not started and only logged (ABORT-BUDGET); it gives no 7 or 8. | p67:226-228, 236-244, 259; s67_score.py:91-93 | EXTENDS |
| 137 | Truncation rewrites verdicts: a verdict whose text before ` (` carries PASS, CLEAR or ELIGIBLE (not `NOT ELIGIBLE`) reads `<name> TRUNCATED (...)`; a finding or a void keeps its label. Applied to AS, A1, A1-R, R1, R2-HOLD and T. `R1-BLIND` on a cut ledger reads `R1 TRUNCATED (R1-BLIND on a battery cut at LD_HARD ...)`. T on a cut ledger with no `T67DONE` reads `T TRUNCATED` (an abort token or B0's ABORT-SEAT still comes first, #100). G prints TRUNCATED on score41d's GATE-41d verdict lines and on `GATE-67G<n>`, and `score67g`'s returned result carries `truncated` and gate TRUNCATED; on G, FAIL, UNSCOREABLE and NO VERDICT outrank TRUNCATED (#39). The pool block and the dump twins print `VERDICT-67O` / `VERDICT-67D TRUNCATED`. A truncated stage never reads PASS. | s67_score.py:96-108, 775, 796, 880-882, 986, 1013, 1024, 1265-1267, 1279, 1380-1385, 1472-1478 | EXTENDS |
| 140 | ABORT-SEAM: a plan-67 session (67pre, 67, 67g, 67x) with an active test seam and no `LD_TEST_SEAMS=1` prints `ABORT-SEAM plan <p>: test seam(s) active without LD_TEST_SEAMS=1: <list>; NOTHING WAS RUN` and exits 3, before the locks (no FAILED-ALL). The seams are any non-empty variable of `SEAM_ENV` (`LD_HARD_MIN_S`, `R67_T1_S`, `LD_ROW_BYTES`, `LD_BLOCK_ROW_S`, `R67_N`, `R67_BUDGET_S`, `R67_SIM`, and the stub-sandbox paths `LD_DEVSHM`, `LD_PROCFS`, `LD_LOCK_VM`, `LD_LOCK_RP`, `LD_TMPD`), and any of N_BLOCKS, N_TWIN_REPS, N_POOL_REPS, MIN_LIVE_A, MIN_LIVE_B0, MIN_TWIN or G_ROWS67 (read from `loopdiag_p67.sh`) off its registered value. Without the marker, session 0 also unsets the `R67_*` seams for its children. | plan67.py:72-75, 261-266; run_all:141-147, 185; battery:93-95; tun67_battery.sh:26-27; p67pre:38-40 | EXTENDS |
| 153 | The S2-drop state rides in the ledger. The battery writes `LD-PLAN67 plan=<p> drop_s2=<0 or 1>` on the A, R1, R2, pool and dump ledgers and `LD-PLAN67 plan=41d stage=<g1 or g2> drop_s2=<0 or 1>` on G's; T's battery writes `LD-PLAN67 plan=t drop_s2=<0 or 1>`, and the envelope forwards `LD_DROP_S2` to T. There is no `dump=` field. The scorer reads `drop_s2` from the ledger, and falls back to `LD_DROP_S2` in the environment when no ledger carries the line. It sets R1's registered slots (INCOMPLETE, #138), the arms of the MINIMUM-COUNTS line and (round 3) G2's verdict: G2 with S2 dropped reads UNSCOREABLE. Every scorer prints `LD-PLAN67 STATE drop_s2=<0|1|unknown>`; T's prints it and nothing reads it. `plan67.py check` (through its tables) still reads the environment. | battery:98, 162, 648-649; tun67_battery.sh:62; p67:199; s67_score.py:24, 510-514, 727-740, 818, 825-827; plan67.py:85-101 | EXTENDS |
| 154 | TRUNCATED-ALL: a stage cut at `LD_HARD` returns 7 (TRUNCATED, scored); a stage not started for lack of time returns 8 (ABORT-BUDGET, NOT-STARTED). Neither counts as a failure. If they are the only not-ok stages, the session ends TRUNCATED-ALL: a `TRUNCATED-ALL` sentinel (proved writable at launch), the line `LD-ALL-TRUNCATED`, exit 7 (`P67_EXIT_NONPASS`; default taken, 2026-11-27), no DONE-ALL and no FAILED-ALL; the era line and the session's `score.txt` list the cut stages' non-pass verdict lines (NONPASS-IN-CUT-STAGE, round 3; `GUARD2-ELIGIBILITY` lines and the `GATE-67G` UNSCOREABLE / NO VERDICT lines count as non-pass lines, s67_score.py:111, and `score.txt` greps `GUARD2-ELIGIBILITY` too, p67:316). Otherwise it ends FAILED-ALL, naming the cut stages too. A block-1 stop (6) is a failure. Session 0 and the rehearsal have no such path: their budget aborts stay ABORT-BUDGET / ABORT-SMOKE through FAILED-ALL. | p67:164-166, 221, 259, 300-324; run_all:336 | EXTENDS |
| 157 | A finding outranks a void (harness67e). A failing R1 or A1-R reads FAIL before the S1-shortfall UNSCOREABLE (#158). A GATE-41d FAIL, S06-FAIL or G2-TAPER-NONZERO finding on a G ledger that also holds an abort token or a void reads `GATE-67G<n> FAIL` and still names the abort or the void (`UNSCOREABLE: ...`, `NO VERDICT: ...`, #39). The limits: R1 and A1-R still read an abort token, no row, INCOMPLETE and the replacement cap before any FAIL (#80); AS and A1 read an abort before the test (#60); T reads an abort, B0's ABORT-SEAT and a missing T67DONE before its clauses (#100). | s67_score.py:876-879, 1440-1478, 1259-1280 | EXTENDS |
| 159 | The S2-TAPER-NONZERO lines and `GUARD2-ELIGIBILITY NOT ELIGIBLE` (harness67e). Every S2-family row (S2, P2, P2T, `S2P<pool>`, `S2D`) read in A, R1, R2, the pool block and the dump twins with `taper > 0` or `plost > 0` prints `S2-TAPER-NONZERO <stage> <cell> <arm> s<seed> rep<rep> (<status>): taper=<n>; plost=<n>`, whatever its status or preconditions, originals and replacements alike; a row with a `bin-sha` problem (wrong binary) is not counted. Any hit prints one `GUARD2-ELIGIBILITY NOT ELIGIBLE (S2-TAPER-NONZERO on <n> S2-family row(s) of stage <s>: ...)` line, and on A, R1 and R2 the `VERDICT-67A` / `VERDICT-67R` / `VERDICT-67P` line ends `; S2-TAPER-NONZERO rows <n>; guard 2 NOT ELIGIBLE`. The pool and dump verdict lines (`VERDICT-67O` / `-67D`) carry no suffix. G reads the same property as G2-TAPER-NONZERO (#149) and prints no GUARD2-ELIGIBILITY line. A replacement never clears it. §4.5's eligibility list does not name it yet (Proposed text §4.5). | s67_score.py:111, 126-143, 806-808, 883-886, 985-988, 1012, 1023; p67:316 | EXTENDS |

### Contradictions

**Still contradicting at `harness67e`.** Each is resolved by a proposed text change, not a code fix:
- **T2's red (Amendment 1 item 2(b), §2.1 item 3; third review).** Registered as "copies of k sent through a proactive slot"; the record is a taper red. It is the same `taper = 0` property that `s06_no_copy` and Stage A's outcome witness assert; T2's own subject (k recovered by NACK within the bound, at most one resend per SRTT) is not evaluated on S1, because the taper panic comes first. The ported T2 therefore adds no red-first evidence on recovery timing; on S2 it gates that timing as a green test. Amended: Proposed text §2.1 item 3.
- **#63, AS's null.** §9 says an exact null; the code uses a Monte Carlo null. Amended: "AS's p-value is a Monte Carlo permutation estimate (the code's draw count), reported with its count."
- **#68, c8 in-order delay.** §4.2 says two-sided; the code tests nothing. Amended: "reported as a relative median difference, untested."
- **#110, block-1.** Its Proposed text calls the coded rule stricter; in Stage A it skips the c8 first rows, which is looser than "every arm's first real row". Amended: "stricter except in Stage A, where c8's first rows are not block-1 rows."
0. **#156, where the touched targets run.** §2.2 says inside the suites run; three of them run only in the repeat's sim step or nowhere. Amended: Proposed text §2.2 (the touched-target list).
1. **#12, tools-diff scope.** §1.3: "`git diff B0 S1 -- tools` and `git diff S1 S2 -- tools` are empty". The code compares `tools/l1` only. Amended: Proposed text #12 (§1.3).
2. **#111, block-1 coverage.** G1 and G2 now run block 1 (battery:879-892). T and the dump twins still have none. Amended: Proposed text #111 (§5.2).
3. **#132, the T parser.** §9 item 5: "parsed with `verify4_parse.py tun`". The code uses its own parser; it now reads the bring-up lines, and test_s67.py:805-820 checks it against `verify4_parse`. Amended: Proposed text #132 (§9 item 5).
4. **#59, A1's minimum.** §4.2: "fewer than 10 live rows on S1 or S2 after replacement is UNSCOREABLE for AS and A1". A1 now reads only S1's and B0's minimums (s67_score.py:736). Amended: Proposed text #59 (§4.2).
5. **#141, session 0's envelope.** §6.0: "`suites_run.sh` envelope". Session 0 runs as `LD_PLAN=67pre` through `loopdiag_run_all.sh`. Amended: Proposed text #141 (§2.2, §6.0).

**Former contradictions, fixed on harness67c:**
- **#126, R2 smoke failure (round 3):** a rehearsal R2 smoke failure makes R2 VOID and ends REHEARSAL-R2-VOID, exit 7; the other smokes run (p67:270-275, 296-297).
- **#51, S06-FAIL precedence:** S06-FAIL now sits above the infra and crash labels (s67_score.py:396-398).
- **#58, the live-row minimum:** LIVE rows only (s67_score.py:517-520); crashes are counted apart (#151) and read through the candidate-crash rule (#152).
- **#76, B0 gauge prints in R1:** the row stays LIVE and fails A1-R (s67_score.py:306, 835).
- **#81, R1 readouts:** lag and `cpu_srv/GB` are printed (s67_score.py:854-861).
- **#101, T bring-up:** ABORT-BRINGUP-T, and the stage is not ok (s67_score.py:1194-1196, 1270-1271; p67:253).

**Former contradictions, fixed on harness67e:**
- **#25, the repeat budget:** S1 runs first with half the budget and S2 gets the budget minus S1's elapsed time, as the third review reads §2.2 (repeat67_run.sh:33, 51, 75).

### Defaults taken, and what stays open for the owner

**Coordinator decision (2026-11-27):** each procedural open item takes the default that aborts and keeps the data. Each line says what the default is; none changes a registered clause. Owner-only items are listed last.

Defaults taken:
- **Pin strength (#4): default taken.** Every pin in the second amendment is a full 40-hex sha (the seat table below), so the prefix match accepts exactly one commit.
- **Mixed account labels (#90): changed on harness67e.** The mix is judged across all counted twin rows. U is read on the `final=1` rows only, with at least 5 per seat; with fewer, U is unread and R2-HOLD is UNSCOREABLE unless (ii) fired. `oldest_unchanged_max` is read on every counted row. A fired hold outranks an unread U. A mixed set is flagged in the Result with its per-label counts (the `MIXED account labels` line). R2-HOLD firing still holds the merge.
- **R1 void rows (#77, #78): changed on harness67e.** Unreplaced VOID-* rows leave A1-R's and R1's completion sets and R1-BLIND's count; crashes still count as not completing. Fewer S1 rows with a reading than the registered 12 reads UNSCOREABLE (#158), but a failing clause reads FAIL first (#157). Both err toward not passing R1.
- **Block-1 on a replaced first row (#110): default taken.** As coded: a void or S06-FAIL first row stops the stage even when its replacement is LIVE (stricter than §5.2's wording; the rows are kept).
- **S06-FAIL in the rehearsal (#122, #124): changed on harness67e.** An S2-family-only smoke failure in g2, A or R1 makes guard 2 VOID (REHEARSAL-G2-VOID, exit 7, #160); the other smokes run and the rows are persisted. A failure that names S1 or B0 aborts the rehearsal (ABORT-SMOKE). §6.1's "a finding" is read as "a finding that voids guard 2".
- **Verdict classes (#39, #100, #157): coordinator decision, third review item 1-2.** A finding reads FAIL; a void, an abort or a missing stage reads UNSCOREABLE (re-run) and never starts a FAIL follow-up (§3.4's bisection, `LD_G2_FAIL67`). A finding on a ledger that also holds a void or an abort still reads FAIL and names it.
- **G2 FAIL drops the pool block (#115): default taken.** As coded: `LD_G2_FAIL67=1` drops the pool block's S1 arms too; lever B's trigger then waits for its own session (§5.2).
- **T2's sim port (#29): fixed on harness67d.** `sim_s06_nack` is in `R67_SIM` and runs twice with the digest check.
- **Parser equivalence (#133): changed (advisor review 2).** Before session 2, `readings()` is compared with `s51_score.py`'s counters and `acct.py`'s below-floor count on a committed s51b ledger on the host, and again on a rehearsal ledger for `pslot=`. A mismatch blocks session 2. (Coordinator level.)
- **No launch smoke (#135): open for the owner, as a waiver request.** Protocol item 4 asks for a short smoke on the same binary that witnesses every required gauge and echo on both endpoints before a launch. The rehearsal's smoke meets "same binary" (the launches run its sha-checked binaries) and "every gauge and echo" for each stage's witnesses, but in an earlier session, not at the launch. The block-1 rule meets "at the launch" for G1, G2, A, R1, R2 and the pool block, after one block of scored rows. Gaps: T and the dump twins have no block-1, and a block-1 stop halts one stage, not the launch.
- **The twin "neither" wording (#86): default taken.** As coded, "not both" is VOID (Proposed text #86).
- **Session 0 after an S2 failure (#147): default taken.** As coded: session 0 stops and is relaunched with `LD_DROP_S2=1`; §6.4's worst case gains about 2.4 h for it.
- **REF-MANIFEST before binid (#146): default taken.** As coded; a later BINTEST abort ends session 0 FAILED-ALL, so no launch can use that manifest without a fresh session 0.
- **TRUNCATED-ALL with an outcome label, and exit 7 (#154): default taken.** The session exit reports completeness (exit 7 for TRUNCATED-ALL and for REHEARSAL-R2-VOID, `P67_EXIT_NONPASS`); the non-pass verdict lines of the cut stages are listed on the `LD-ALL TRUNCATED` era line and as `NONPASS-IN-CUT-STAGE` in the session's `score.txt` (round 3, p67:301-308, 318, 323).
- **Tools tests in session 0 (#20, #21, #141): default taken.** As Proposed text #141.
- **§2.1 item 2's line reference:** the fixed rule is at `loopdiag_run_all.sh:768-795` (at `harness67d`).
- **§6.4's "10 smoke rows":** the code runs 16 (#119, #120); timing unchanged within the 25 min line.
- **No seats or token for the G1 bisection or a pool-only session: default chosen (§5.2 says "pre-pinned at the second amendment"; coordinator to confirm).** They are cut only if G1 fails or session 3 runs long, under their own amendment; no row runs on an unpinned seat (Proposed text, G1 bisection). Coordinator level (advisor review 2).
- **A1 and S2's minimum:** resolved at `3092dc96` (Proposed text #59).

Owner answers (2026-11-27, verbatim; the owner delegated the S06 judgement to Fable):
- **P0, Ceph: cleared.** "i dont think i have any ceph instability." Sessions may launch. No stability stop rule is added: plan 67 has no A/A rows, and infra aborts are read as registered (UNSCOREABLE, re-run), never as a verdict.
- **#135, the launch smoke: waiver granted.** "i accept the rehearsals smoke rows." The launches rest on the rehearsal's smoke of the same sha-checked binaries plus the block-1 rule; the gaps listed above (T and the dump twins have no block-1) are accepted with it.
- **S06's merge on its own guard-2 PASS: stands, conditionally.** "the merge approval does stand if fable is confident on the fix." Fable's conditions (2026-11-27, revised after the third review); if all three hold, S06 merges without a further question:
  1. T2 red on S1 in both invocations with matching output lines (S1 panics before the test's own trace comparison, so the sim traces were not compared), and green on S2 with the slot witness (`pslot=Some(n)`, n > 0): met at the VM pre-pin check below, as amended in §2.1 item 3. The record was taken at S1 `507fb836` / S2 `90a13d59`; it carries to the final pins only if the S1→S2 engine patch-ids (`git diff S1 S2 -- raptorpath | git patch-id`) are unchanged at the pin, else T2 is re-run on the pinned trees;
  2. the harness67d S2 gating command, run once on the VM, prints a `Running` line and a non-zero passed count for every target: met (VM pre-pin check 1 below);
  3. guard 2 reads ELIGIBLE TO MERGE under §4.5, every bullet (suites and repeats, G2, AS, R1, the below-floor hold-flag read, R2-HOLD not fired or cleared, binid, the ADR-0084 Amendment 1 rewrite before the merge). R1-BLIND is accepted only with A's and R2's execution witnesses met, so the merge never rests on Bulk rows alone. A fired R2-HOLD or a below-floor flag is cleared only by a written read (Fable's, owner-delegated), recorded in the Result before the merge.

### Seat heads at the 2026-11-28 main merge (final pins, full shas)

`harness67e` `1d26c8cd` (harness67d plus the third review's fixes) merged to main as `5ea4dd11`; main is `6d1c959ddf73ec10bc1eb1374c222c6e3824b6ea` with the roadmap note. Every seat took it by a clean `git merge --no-ff`, tools only:

| seat | branch | pin (`LD_PIN_<SEAT>`) |
|---|---|---|
| B0 | none | `6d1c959ddf73ec10bc1eb1374c222c6e3824b6ea` (github/main) |
| S1 | `vmb-s1` | `1ecc5b8305ebe2886c205f8669eee3d03f157c0e` |
| S2 | `vmb-s2` | `e74d4645f893bbac0b515ca03ed1efdc34385427` |
| P1 (P1T) | `vmb-p1` | `fc9d696ec27f42e29d01410e415eb71f4e56a993` |
| P2 (P2T) | `vmb-p2` | `cd93566c6402f50cf07f8f76e2b6ca04f4e6a69a` |

Witnesses at these heads:
- `git diff B0 S1 -- tools` is empty; `git diff S1 S2 -- tools` is `tools/ci/coverage_sites.json` only (#12). P1 / P2 against S1 / S2: `perf.rs`, `gates/tests.rs`, `lib.sh` (one line) and the pacer's fourth file, as before (#14).
- `git diff 507fb836 S1 -- raptorpath` is empty: S1's engine is byte-for-byte the tree T2's red and the binid runs were taken on.
- T2's carry rule (S06 condition 1): `git diff S1 S2 -- raptorpath | git patch-id --stable` = `32a5afdc…` at the old and the new heads; the pacer diff's patch-id is unchanged too (`1d8c086e…`). The T2 record carries; no re-run.
- CI on the four heads: see the roadmap note of the pin push.

### Seat heads at the 2026-11-27 main merge (pins, full shas)

Main moved from `9bc656bf` to `39c41f2a4d8597f770db17db9984dd4c081e92a1`: the wasm model (knobs, consumer, link vectors), docs, `tools/toy`, the roadmap, harness67c round 3 (tools only), and test-only engine-path files (`raptorpath/src/net/tests.rs` +91, `raptorpath/tests/sim_d_frozen.rs` and its `[[test]]` stanza, the ci.yml sim line). No non-test `src/` of `raptorpath`, `raptorpath-math` or `raptorpath-core`, no root `Cargo.*` and no `vendor` changed, so §1.3's "if main moves" needs no engine re-pin. Engine patch-ids (`git patch-id --stable` over §1.3's set `raptorpath raptorpath-math Cargo.toml Cargo.lock`): S1 against B0 `ac12e9b5`, as at the 2026-11-21 heads; S2 against B0 `24602b1e` and S1 to S2 `9cbbd742`, equal to those of `1ea78c26` (the 2026-11-21 S2 head plus its `sim_s06_nack` `[[test]]` stanza, which is why they differ from the draft's `3d06984e` reading). Main's new source pins in `net/tests.rs` (`emit_source.rs` text) hold on S2.

| seat | branch | pin (`LD_PIN_<SEAT>`) | how |
|---|---|---|---|
| B0 | none | `39c41f2a4d8597f770db17db9984dd4c081e92a1` | github/main, the main commit inside S1 |
| S1 | `vmb-s1` | `507fb836ae1afa62a7cbcbb6679e0aca0ea0e712` | `git merge --no-ff github/main` (2026-11-27, twice: `0069b792`, then this), clean |
| S2 | `vmb-s2` | `90a13d59eafb7b8cd3abc5782b2ac7b49d9e5b8f` | `git merge --no-ff` new S1, twice; the first had one conflict in `.github/workflows/ci.yml` (sim job), resolved as the union (`--test sim_s06_nack` and `--test sim_d_frozen`) |
| P1 (P1T) | `vmb-p1` | `7ab0cf9d487297f21dad6e997865cd986e3c5fcc` | S1 + `4ee1df9b` applied without its commit (`git cherry-pick -n`): `perf.rs`, `gates.rs` (the `[GATES]` echo token), `gates/tests.rs` (the pinned echo) and `RWM_PERF_RATE_MBIT` in `lib.sh`'s `RWM_FORWARD`. P1T is this tree built `--release --features test-seams` (#2) |
| P2 (P2T) | `vmb-p2` | `4664086538df8d1daace6d49e684ec99a87eae2b` | S2 + the same diff; P2T as P1T |
| vmb-c8, vmb-tun | none | not cut | only after a G1 FAIL (defaults taken) |

**These pins move once more** when `harness67d` (with the third review's fixes, `harness67e`) merges to main: B0 becomes the new main, S1 and S2 take main again (tools only), and P1 and P2 are rebuilt on the new heads. B0 moves too, because the tools witness compares its `tools/l1` with the session harness. The second amendment pins the final shas.

Witnesses at these heads (§1.3):
- `git diff B0 S1 -- tools` is empty; `git diff S1 S2 -- tools` is `tools/ci/coverage_sites.json` only (#12).
- P1 and P2: `lib.sh` with ` RWM_PERF_RATE_MBIT` deleted is byte-identical to S1's (#14); the pacer diff is the only change against S1 / S2.
- CI `ci` green on all four heads. `binid.yml` runs only on pushes touching `clock.rs`, `sim.rs`, `test_seams.rs`, `net/account.rs`, `raptorpath/Cargo.toml` or the workflow, so it did not run on these heads; its last green runs are at S1 `0069b792` and S2 `1b27a1b6`, and the engine diff from those to these heads is empty.

### Proposed text changes

Each item replaces or extends the registration text named. Since `9bc656bf`, harness67c fixed the code for #1, #49, #51, #58, #76, #81, #85 (R2-BLIND), #89, #91, #100, #101, #111 (G), #112, #121 and #128/#134 (ABORT-SEAM) at `eb976857`, and for #40/#149 (G2), #137 (R1-BLIND truncated), #152 (R2) and #154 (TRUNCATED-ALL) at `3092dc96`. Where an item below names a fixed row (#58, #111), it writes the fixed behaviour into the text. Harness67e changed #25, #39, #49, #77/#78, #90, #100 and #122/#124: the #39 item is replaced, and items for the others follow the #126 item.

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
- **#39 (§3.1, §4.1).** Add: "GATE-67G1 / GATE-67G2 read FAIL only for a finding: GATE-41d FAIL, an `S06-FAIL` row (G2) or a `G2-TAPER-NONZERO` row outside `S06-FAIL`. PASS needs none of these, no unreplaced witness failure and no abort token. An abort token, an unreplaced witness failure, `S06-DNF-RULE NOT EVALUATED` and, on G2, a cell with no S2 row (or S2 dropped) read UNSCOREABLE; score41d's NO VERDICT reads NO VERDICT. Both mean re-run, and neither starts §3.4's bisection or `LD_G2_FAIL67`. A finding on a ledger that also holds an abort or a void reads FAIL and names it. A ledger cut at `LD_HARD` reads TRUNCATED, unless one of these applies."
- **#40, #149 (§4.1, taper on S2).** Replace "`taper=0` on every S2 row" with: "`taper = 0` and `plost = 0` on every S2 row. A nonzero reading on an S2 row that prints `pslot=` is `S06-FAIL`, whatever its other labels, and is never replaced. A nonzero reading on an S2 row without `pslot=` fails the seat witness: the row is replaced once, and `G2-TAPER-NONZERO` fails GATE-67G2 whatever the replacement shows."
- **#79 (§4.3).** The S2 outcome witness is `taper = 0` and `plost = 0` (§4.2's rule), not `taper = 0` alone. An S2 row with `pslot = 0` is VOID-WITNESS.
- **#41 (§4.1).** Add: "§4.2's rule 'a DNF where every S1 row completes is `S06-FAIL`' applies to G2 too, over S1's LIVE rows."
- **#58, #152 (§4.2, the minimum; §4.4, the twin minimum).** In §4.2, replace "a cell with fewer than 10 live rows on S1 or S2 after replacement is UNSCOREABLE for AS and A1" with: "a cell with fewer than 10 live rows on S1 or S2 after replacement is UNSCOREABLE for AS and A1. A LIVE DNF counts; a crash (VOID-RC, or NO_DATA after its replacement) does not, and the per-arm live and crash counts are printed. An UNSCOREABLE caused by candidate-arm crashes is a finding to investigate, not something a plain re-run clears." In §4.4, after "Unscoreable is never read as 'did not fire'", add: "The candidate arm is P2T: when P2T is under the minimum and holds any crash row, R2-HOLD reads UNSCOREABLE (CANDIDATE-CRASH P2T), and says so when live plus crash rows are still under the minimum; crashes in a candidate arm that meets its minimum are reported (CANDIDATE-CRASHES), not a shortfall. An UNSCOREABLE caused by candidate-arm crashes is a finding to investigate, not something a plain re-run clears. A P1T shortfall is a plain UNSCOREABLE with the live and crash counts."
- **#59 (§4.2, A1).** Replace "a cell with fewer than 10 live rows on S1 or S2 after replacement is UNSCOREABLE for AS and A1" (as worded in the #58 item above) with: "an S2 shortfall makes AS UNSCOREABLE; A1 is UNSCOREABLE only on an S1 or B0 shortfall (fewer than 10 live S1 rows, or fewer than 4 live B0 rows, in a cell)".
- **#86 (§4.4, twins).** Replace "a row with neither is VOID" with "a twin row missing either `[ACCT]` (client) or `[ACCT-RX]` (server) is VOID".
- **#126 (§4.4, R2 smoke).** The registration is kept, and the code now follows it (harness67c round 3, on main: an R2 smoke failure marks R2 VOID, the other smokes run, AMENDMENT2.txt records it, the rehearsal ends REHEARSAL-R2-VOID with exit 7; guard 1 is not held, as §5.2 requires). Add to §4.4: "The R2 smoke runs in the rehearsal (§6.1). Session 3 runs no second R2 smoke; it reads the same witness on the scored rows, and more than half of a seat's rows failing it (`R2-BLIND`) makes R2 VOID, so guard 2 waits for an R2 re-run."
- **#160 (§6.1, rehearsal).** Replace "A failed S2 outcome witness in the rehearsal is a finding." with "A smoke failure that names the S2 family alone (g2, A or R1) marks guard 2 VOID (REHEARSAL-G2-VOID, exit 7): the other smokes run and guard 1 may launch. A failure that names S1 or B0 stops the rehearsal."
- **§4.5, #159 (S2-TAPER-NONZERO).** Add to ELIGIBLE TO MERGE's list: "no `S2-TAPER-NONZERO` row on any S2-family row of A, R1, R2, the pool block or the dump twins, originals and replacements alike (`GUARD2-ELIGIBILITY NOT ELIGIBLE`)". The third review adds it to §4.5; the body does not hold it yet.
- **#90 (§4.4, mixed account labels).** Add: "With `final=1` and `ACCT-LAST` twin rows together, U is read on the `final=1` rows only (at least 5 per seat), else R2-HOLD is UNSCOREABLE unless (ii) fired; `oldest_unchanged_max` is read on every counted row."
- **#77, #78, #158 (§4.3, R1 voids).** Add: "An unreplaced void row is no reading and leaves A1-R's and R1's completion sets; R1-BLIND counts the LIVE S1 rows; fewer than the registered 12 S1 rows with a reading is UNSCOREABLE for A1-R and R1 unless a clause fails (FAIL first)."
- **#100 (§3.3, §3.4, Stage T).** Add: "T reads UNSCOREABLE for an abort token, a ledger with no `T67DONE` (TRUNCATED if cut at `LD_HARD`) and a missing S1 refusal line; T FAIL needs a refusal line that is present and wrong, or a failing transfer clause (3 or 4)."
- **#49 (§4.2, replacement).** Add: "NO_DATA is replaceable on the reference arms only; on the S2 family it stays a crash."
- **#25 (§2.2, repeat budget).** Add: "The 60 min are shared in run order: S1 gets half, S2 gets the budget minus S1's elapsed time (S1's build included). A seat's own build and sim runs are outside its share."
- **#110 (§5.2, block-1).** Add: "Block-1 reads each arm's first ledger row as written: a first row that was void and then replaced still stops the stage, and so does a first-row `S06-FAIL`. In Stage A only the c2 cell's first rows are read. In G1 and G2 the first row of each of the 8 (cell, arm) pairs of block 1 is read." (As the code does; stricter than §5.2's wording, so it can only stop a stage more often.)
- **#111 (§5.2, block-1 scope).** Replace "after each stage's first block" with "after the first block of G1, G2, A, R1, R2 and the pool block". G1 and G2 run block 1 over all 8 (cell, arm) first rows. Stage T has no block-1 rule: it is four invocations with no blocks, and its bring-up abort (ABORT-BRINGUP-T, #101) covers it. The dump twins have none: they are eight reported rows with no verdict, so a failed first row is reported, not a stop.
- **#136, #137, #154 (§5.2, §5.4, truncation).** Add to §5.2: "A stage cut at `LD_HARD` is TRUNCATED: it is scored at the n it reached, and none of its verdicts reads PASS, CLEAR or ELIGIBLE (R1-BLIND reads R1 TRUNCATED). A stage with under 1 800 s left before `LD_HARD` is not started (ABORT-BUDGET, NOT-STARTED). A session whose only not-ok stages are cut or not started ends TRUNCATED-ALL (`LD-ALL-TRUNCATED`, exit 7; default taken), which is neither a pass nor a failure; the non-pass verdict lines of its cut stages are listed (NONPASS-IN-CUT-STAGE). Any other not-ok stage ends the session FAILED-ALL." The session exit reports run completeness, not verdicts: a cut stage returns 7 whatever its verdicts, and the TRUNCATED-ALL line lists every non-pass verdict of the cut stages, as DONE-ALL leaves verdicts to the score lines. Add TRUNCATED-ALL and NOT-STARTED to §5.4 beside TRUNCATED (#129). Two limits, stated as registered: G's replacement run, when fewer than 900 s are left before `LD_HARD`, is skipped and only logged (`ABORT-BUDGET stage <g> replacement`, `loopdiag_p67.sh:243`); the stage gets no NOT-STARTED marker and no return 8, and its voided slots stay void, so 41d's cap rule reads them. Session 0 (`67pre`) and the rehearsal have no truncation path: a budget shortfall there is ABORT-BUDGET (session 0, FAILED-ALL) or ABORT-SMOKE (the rehearsal's bounded smoke, `loopdiag_p67.sh:203-207`), never TRUNCATED.
- **#129 (§5.4, abort classes).** Replace "bring-up exhausted (Stage T)" with ABORT-BRINGUP-T, and add the harness's further tokens: ABORT-SEAM (first, before the locks), ABORT-REHEARSE, ABORT-CUT, ABORT-SHM, ABORT-SUITES, ABORT-BUDGET, ABORT-ROWS, ABORT-SHA, ABORT-BIN, ABORT-PLAN67, ABORT-PRECONDITION, `FAILED-<stage>` (FAILED-STAGE67), BUSY, BINID67 FAIL. Add the verdict tokens that are not aborts: TRUNCATED (the battery was cut at `LD_HARD`), INCOMPLETE (UNSCOREABLE: a registered slot holds no row), R2-BLIND (R2 VOID), CANDIDATE-CRASH, G2-TAPER-NONZERO, `S06-DNF-RULE NOT EVALUATED`. Add the outcomes REHEARSAL-R2-VOID (exit 7) and TRUNCATED-ALL (exit 7), and, as readouts that decide nothing, CANDIDATE-CRASHES and NONPASS-IN-CUT-STAGE. Under ABORT-REPEAT, its causes: TARGET-MISSING(<t>), ZERO-TESTS(<t>), SIM-RC, DET-RC, SIM-NO-RESULT, SIM-DIGEST-DIFFERS, SIM-FAILED. ABORT-PREFLIGHT stays as listed; session 0 now emits it (#141). The verdict taxonomy of `docs/measurement-discipline.md` records no verdict outside the registered set, so these tokens need this text before any scored result is read.
- **#130 (§5.4, order).** Add: "In practice ABORT-SEAM fires first, then ABORT-PRECONDITION, the session budget (ABORT-BUDGET) and the tools-test gate (ABORT-TESTS); then commit pin (ABORT-SEAT); in session 0 its steps follow (inside step 4, ABORT-REPEAT with its cause); otherwise build and sha (ABORT-BUILD), identity strings and tools witness (ABORT-SEAT), suite reuse (ABORT-SUITES), tools tests (ABORT-TESTS). The Result lists the first one that fired."
- **#132 (§9 item 5).** Replace "parsed with `verify4_parse.py tun`" with "parsed by `s67_score.py`'s T parser, which reads the bring-up lines; `test_s67.py` checks that it and `verify4_parse.py tun` agree on a fixture".
- **G1 bisection seats (§5.2).** Replace "These seats, `vmb-c8` and `vmb-tun`, are pre-pinned at the second amendment" with "These seats, `vmb-c8` and `vmb-tun`, are cut and pinned by their own amendment after a G1 FAIL, before the bisection session, on this batch's pinned B0 (not on main at that time). The harness has no bisection token yet either, so the bisection runs in the first session after both exist, not necessarily the next one."
- **§2.1 item 3 (T2's red, Fable 2026-11-27).** Replace "It must be shown red on S1's engine (copies of k sent through a proactive slot) and green on S2." with "It must be shown red on S1's engine and green on S2. The red is the run-wide `taper` assertion (`taper = 0`): S1's copy branch sends proactive-slot copies in T2's scenario (`taper > 0`), S2 sends none while taking slots (`pslot > 0`). The k-specific assertion (no copy of k through a proactive slot) is kept, but it is not a discriminating red: at the 2026-11-27 check it passed on S1 too, so the record claims a taper red only, not that k is slot-copied on S1."
- **§2.2, the S2 repeat.** Replace "S2: every non-ignored test in `s06_no_copy` and `s06_reactive` (T2 runs as a sim test, §2.1 item 3); `holeclass_reachability`" with "S2: every non-ignored test in `s06_no_copy` and `holeclass_reachability`. `s06_reactive` holds no non-ignored test (T2 runs as the sim test `sim_s06_nack`, T3 records only, §2.1 item 3), so it is not repeated. Each run must pass at least one test in each named target."
- **§2.2, the touched-target list (#156).** Replace "inside the suites run; listed so a seat-specific failure is named" with "`holeclass_reachability`, `recovery_bench`, `sim_nack_test`, `place_arms_reachability` and `s06_no_copy` inside the suites run; `sim_shed_reach`, `sim_lost_tail` and `sim_s06_nack` in the repeat's sim step (#29); `s06_reactive` holds only record-only `#[ignore]` tests and runs nowhere in this batch. Session 0's TARGETS line reports what the suites saw, and gates nothing."

### Third review (2026-11-27, fresh Opus reviewer, at `a9fd5aab` / harness67d `5826606a`)

Verdict: ready after fixes. Text fixed here: the P0 stop rule (no A/A rows exist), Fable's S06 conditions aligned with §4.5, the T2 red listed as a contradiction, B0 in the pin move, #63, #68, #110. Code fixed on `harness67e` (from harness67d; each with a red-first test), all decided by the coordinator:
1. GATE-67G verdict classes: FAIL only for GATE-41d FAIL, S06-FAIL or G2-TAPER-NONZERO; NO VERDICT and UNSCOREABLE (aborts, NOT EVALUATED, no S2 row, unreplaced witness failures) mean re-run, and never trigger §3.4's bisection or `LD_G2_FAIL67`. This replaces Proposed text #39's "an abort reads FAIL".
2. Stage T: aborts read `T UNSCOREABLE`, a cut battery `T TRUNCATED`; T FAIL only on refusal lines present and wrong.
3. R1 and A1-R: unreplaced VOID-* rows leave the completion sets (crashes still count as not completing); R1-BLIND counts LIVE S1 rows; too few live rows read UNSCOREABLE. This replaces the #77/#78 defaults.
4. The repeat budget: S2 gets the 60 min minus S1's elapsed time, as §2.2 registers (#25).
5. `S2-TAPER-NONZERO` on every S2-family row in A, R1, pool, dump twins and R2, originals and replacements alike; a hit makes guard 2 NOT ELIGIBLE (added to §4.5).
6. NO_DATA is not replaceable for candidate arms (it stays a crash, as in 41d; #49).
7. R2-HOLD with mixed account labels runs on `final=1` rows only, else UNSCOREABLE; `oldest_unchanged_max` is read regardless (replaces the #90 default).
8. A rehearsal S2-family smoke failure marks guard 2 VOID and lets the other smokes run, as #126 does for R2 (replaces the #122/#124 defaults).
Not changed: the sim digest compares sorted ok/FAILED lines only (adds nothing beyond "both runs passed"; reported as such); T clause 3 is vacuous if B0 completes nothing (as registered); the pool block has no own session after a G2 FAIL (its own amendment, as before).

### VM pre-pin checks (2026-11-27, coordinator; output on the VM in `~/vmb1-in/check.out`)

Trees: git archives of S1 `507fb836ae1afa62a7cbcbb6679e0aca0ea0e712` and S2 `90a13d59eafb7b8cd3abc5782b2ac7b49d9e5b8f`. No scored row; no session envelope.

**1. The S2 repeat commands.**
- Main's harness (`39c41f2a`, `repeat67_run.sh` before harness67d): both commands rc 101, "requires the features: `test-seams`". Session 0 would have read ABORT-REPEAT seat=S2 at the build step (#27).
- harness67d `ba0b112b`'s commands: every command rc 0. Gating (with `--features test-seams`): `s06_no_copy` 12 passed, `holeclass_reachability` 3 passed, `s06_reactive` 0 passed, 6 ignored (all `#[ignore]`; see #27, #28: the per-run rule of `ba0b112b` would have passed it, and `96587526` drops the target and checks per target). Sim (`--features sim,test-seams`, `--cfg tokio_unstable`): `sim_shed_reach` 8 (+3 ignored), `sim_lost_tail` 4 (+1 ignored), `sim_s06_nack` 2. Plain (`--release`): `place_arms_reachability` 4, `sim_nack_test` 4.
- **The final gating command of `96587526`, run on the VM (2026-11-27, S2 `90a13d59` tree, fresh target; log `~/vmb1-in/gate-final.log`):** `cargo test -p raptorpath --release --test s06_no_copy --test holeclass_reachability --no-fail-fast`, without `--features test-seams`: rc 0; `Running tests/holeclass_reachability.rs`, 3 passed; `Running tests/s06_no_copy.rs`, 12 passed; 0 ignored in both. The counts equal the `--features test-seams` run's. Fable's S06 condition 2 is met.

**2. T2 red on S1, green on S2 (§2.1 item 3).** `sim_s06_nack.rs` from S2, with its `[[test]]` stanza, on an S1 tree; each tree run twice.
- **S1 (red, on the run-wide assertion only):** both runs rc 101; `[s06 T2 sim] sends of k: 3 (copies via slot path 0, NACK/sweep resends before fill 1), taper=127 pslot=None, stall 37000 us`, then the panic at `sim_s06_nack.rs:120:5`, which is `assert_eq!(taper, 0, "taper={taper}")` (taper = 127). The k-specific assertion before it (`copies == 0`: no copy of k through a proactive slot) PASSED on S1: k was not slot-copied; the copy branch sent 127 copies of other seqs. The slot witness held (taper > 0; S1 has no `pslot=`). The two invocations agree line for line (the test panics inside its first scenario, so this is its two-run digest).
- **S2 (green):** both runs rc 0; `taper=0 pslot=Some(192)` (slots taken, none a copy); the test's own two-seed-run trace equality holds.
- **Ruling (Fable, 2026-11-27, owner-delegated): the taper-only red counts, with the claim narrowed.** The red shows that S1's copy branch acts in T2's scenario (127 proactive-slot copies, `taper=127`) and S2's does not (`taper=0` with 192 slots taken). It does not show that the stalled blocker k itself is slot-copied on S1: the k-specific assertion (`copies == 0`) passed on both seats, so it did not discriminate. The record claims "taper red" only. §2.1 item 3 is amended to match (Proposed text, §2.1 item 3).

### B0 and the binid runs (2026-11-21)

- **B0** is the merge base of `vmb-s1` and github/main (the main commit inside S1), not a moving main. Its sha is filled in at pin time: it moves at the seat re-merge. After the 2026-11-21 merge it was `9bc656bf`.
- **What the push binid runs compared.** On the pushes of `vmb-s1` `6628e93b` and `vmb-s2` `3d06984e`, `binid.yml` compared main `9bc656bf` (base, base2: identical) against each seat head. Both passed only through the `binid: expected-change` line of `2d590cdc`. They did not compare old seat head against new. Old against new, dispatched with no opt-out: identical release binaries (archive and binary sha) for S1 `2d590cdc` → `6628e93b` (run 44938909897) and S2 `1953abd1` → `3d06984e` (run 44938912735).
