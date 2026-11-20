# VM batch 1: the c8 gauges, the tunnel in-order guard and the S06 deletion, guarded in the audit's order: pre-registration draft

Draft for the coordinator to register as its own status section. No row has run. Base text: github main `7123c754`. Order source: `docs/derivations/liveness-audit.md` §3 (rank 1 = S06, rank 2 = the c8 gap horizon, rank 3 = the tunnel guard). Design inputs: `docs/derivations/s06-delete-plan.md` §6, `docs/derivations/f2-guard-registration.md` (the house format), status §41d, §58, §64, the 2026-11-19 VM queue and the 2026-11-20 S06 review. Process rules: CLAUDE.md "No engine merge on a Bulk-only guard", the binary-identity rule, ADR-0080's flake rule and `docs/measurement-discipline.md`.

**For the owner, in plain words.**
- **Nothing in this batch can be pinned today.** All three candidates are 77 commits behind main. `s06-delete` conflicts with main (in `emit_source.rs`, `sender_phases.rs` and the paper) and with `c8-gauges` (in `emit_source.rs`). A worker builds merged-up seat branches first, and an amendment pins them (§1). That is host work, with no VM.
- **The batch runs two guards, so the two parts that are ready can merge without waiting on S06:**
  - **guard 1:** the c8 gauges plus the tunnel guard, against main;
  - **guard 2:** the S06 deletion on top of guard 1's candidate.
  
  Each guard has a Bulk stage and stages off Bulk, with witnesses that show each change actually ran.
- **S06 cannot merge on its own under the current text.** ADR-0084 Amendment 1 lifts the S06 merge hold "for this purpose only": `s06-delete` reaches main beneath the ADR-0084 engine change, not alone. That amendment is itself unmerged, and some of its marks wait on you. **The question for you:** if guard 2 passes, does S06 merge now, or wait under ADR-0084? (§7.) The guard runs either way, because it is ADR-0079's own guard (ADR-0084 Amendment 1, item 3, guard 1).
- **The c8 gap horizon (lever B, audit rank 2) is not in this batch.** You parked it: decision 6, re-affirmed on 2026-11-19 ("the fix is the store law"). Its branch is also 628 commits behind. §8 lists what it would need.
- **Ceph.** Status still says no scored VM session runs until you say the Ceph instability is over. This batch waits for that word (§6, precondition P0).
- **Cost:** three VM sessions. Typical ≈ 3.8 h + 2.0 h + 2.1 h ≈ 8 h, worst ≈ 13 h. Each session stays under the five-hour cap.

## 1. Seats and pins

### 1.1 Sources (at the time of writing)

| item | branch @ sha | state against main `7123c754` |
|---|---|---|
| main (BASE) | `github/main` `7123c754` | — |
| c8 gauges | `c8-gauges` `697eeb73` | 77 behind, 12 ahead; trial merge with main clean |
| tunnel guard | `tunnel-inorder-guard` `df5ffa08` | 77 behind, 3 ahead; trial merge with main clean; diff in `config.rs` and `main.rs` (`cmd_run`, `cmd_check`) only |
| S06 deletion | `s06-delete` `a930a72b` (holds `s06-delete-paper-present` `f29a0db2` and `c8-gauges` up to `a68ba449`) | 77 behind, 26 ahead; **conflicts with main** in `docs/fec-arq-model.md`, `net/emit_source.rs`, `net/sender_phases.rs`; **conflicts with `c8-gauges`** in `net/emit_source.rs` |
| the pacer (Stage R2 only) | `4ee1df9b` (`RWM_PERF_RATE_MBIT`) | its engine code is not in main's tree; the forward-list line is (status, 2026-11-20 review) |
| the round trip (tunnel red/green) | `static-audits` `05a94a06`; commits `4bbb2f96` (`resolve_tunnel` extraction), `41e37116` + `235994ac` (the round-trip test) | 0 behind main; it already contains the tunnel commits `4d5c53d2` and `987b0360` |

### 1.2 The seats (cumulative, so each merge state is a guarded tree)

The binary embeds location data (§58 Part B), so merged main equals a guarded binary only if the merge reproduces the guarded source tree exactly. The seats are therefore stacked in merge order:

| seat | tree | role |
|---|---|---|
| **B0** | `vmb-base` = main at the pin + `harness67` (tools only) | BASE of guard 1; reference arm in Stage A |
| **S1** | `vmb-s1` = B0 + `git merge --no-ff c8-gauges` + `git merge --no-ff tunnel-inorder-guard` | CAND of guard 1; BASE of guard 2. Merges to main if guard 1 passes |
| **S2** | `vmb-s2` = S1 + `git merge --no-ff s06-delete`, merged up (below) | CAND of guard 2. Merges to main if guard 2 passes and the owner says yes (§7) |
| **P1** | S1 + `4ee1df9b` (cherry-pick) | Stage R2 only, never merges |
| **P2** | S2 + `4ee1df9b` (cherry-pick) | Stage R2 only, never merges |

**Why the tunnel sits with the gauges, not on top of S06.** Neither change is on the perf data path. The tunnel diff runs only in `raptorpath run` and `raptorpath check`, and the gauges are print sites. With the tunnel stacked above S06, the owner's S06 question would hold the tunnel too. Guard 1's FAIL bisection is cheap (§5.2).

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
   - **T2 (`stalled_blocker_recovered_by_nack`)** still gates, and it reads 9/10 on the candidate (status, 2026-11-20). **Required before the guard, one of:**
     - (a) ported to the deterministic sim (`tests/sim_support`), with the decode-fill witness kept;
     - (b) put on `quarantine.txt` and `#[ignore]`d, with its diagnosed reason (a decode fill of k, not a NACK) and an owner.
     
     The default is (a). Under (b), the S06 red-first evidence loses its gating NACK test, so that must be said in the merge question.
4. **The tunnel's red-first round trip** (`config_round_trip.rs`, `static-audits`). It exists only on `static-audits`, which already contains the guard. The trees:
   - **red tree:** main at the pin + cherry-picks of `4bbb2f96`, `41e37116` and `235994ac` only, so `resolve_tunnel` resolves `merge(base, cli)` as main's `cmd_run` does. The four `ForcedOff` cells (`window_out_of_order`, `window_coded_only`, `window_generation_coding`, `window_systematic_repair`) must fail with "the entry point's forced value moved: the file leaks through". Any other failure, or a pass, means the tree is wrong.
   - **green tree:** `static-audits` `05a94a06` (it contains `4d5c53d2` and `987b0360`). Every cell passes.
   - The command is `cargo test -p raptorpath --lib config_round_trip`, on each tree's own target dir, under the host mutex. The red output, the green output and both shas go into the pin amendment.
5. **The `harness67` worker** (tools only, §9), with its stub runs green.
6. **Fable's review of this registration** (§10).

### 2.2 On the VM, in the rehearsal (fresh target dir per tree)

| step | trees | rule |
|---|---|---|
| `cargo check --all-targets` | S1, S2 | any error is ABORT-PREFLIGHT for that seat's guard |
| `cargo test -p raptorpath --lib gates` | B0, S1, S2 | a failure on S1 or S2 that is not equal on B0 is ABORT-PREFLIGHT; a failure equal on B0 is a finding on main, reported, and the guard runs |
| full suites (`run_suites`, fixed rule): release, doc, wasm (`env -u GOLDEN_CAPTURE`), debug, core, seams | S2 (contains S1) | REAL or non-quarantined FLAKE is ABORT-TESTS: no row runs |
| touched integration tests, once | S2: `holeclass_reachability`, `recovery_bench`, `sim_nack_test`, `place_arms_reachability`, `s06_no_copy`, `s06_reactive`, `sim_shed_reach`, `sim_lost_tail`; S1: `--lib config::` | inside the suites run; listed so a seat-specific failure is named |
| **repeat run of the gating loopback tests** | S2: every non-ignored test in `s06_no_copy` and `s06_reactive` (T2 if it stays loopback); `holeclass_reachability`. S1: `--lib config::` | **N = 30, zero failures** (a 10 % flake survives 30 runs with p ≈ 0.04). Sim tests (`sim_*`) are deterministic and run once plus a two-run digest equality. If 30 runs of a test do not fit the repeat budget (60 min total, timed per run in the first pass), the cut is written in the second amendment before the guard launch, with the reached N. Any failure: that seat's guard does not launch |
| `build_visualizer.sh` (fresh wasm) and the native golden test without capture | S2 | as plan §6 item 1 |
| `binid_run.sh` | B0, S1, S2 | records the crate-directory sha of each seat, the reference for §5.3 |
| tools tests (`tools/l1`, python and bash) | the shared harness | ABORT-TESTS |

**`[ACCT]` (the coordinator's question).** No execution witness in this batch needs the sender's `[ACCT]` account line. It comes from `net/account.rs`, compiled only under `debug_assertions` or `test-seams` (`net/mod.rs:48`), so it is absent from the release binary. The witnesses use:
- `[DIAG] mpr[… plost= taper= pslot= …]` (`pslot=` added by `s06-delete`'s red-first commit; the others exist on main);
- `cod=`, the `[SPAN]` rr and the `[DIAG]` shed gauge with `c8-gauges`' `shed_slot=` / `shed_nack=`;
- the receiver's below-floor count, as Addendum 2's `acct.py` reads it;
- the gauge prints `scw=`, `rsat=`, `rlaw=`, `plx=`;
- the perf summary and `[TRUTH]`.

The account's invariants are covered by the debug suite (`recovery_bench` and the account tests) in §2.2, not by rows. So no `--features test-seams` release row is registered, and the shipped binary stays the only one the rows run.

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
  - B0: `check` does not print that text (the defect, shown on the shipped binary).
  - That is 16 commands.
- **Clause T (scored). T PASS iff:**
  1. all 8 S1 refusal commands refuse with the exact text;
  2. none of the 8 B0 commands prints it (otherwise the seat is wrong: ABORT-SEAT);
  3. every S1 transfer completes (`TUNREP` with data) wherever the B0 transfer of the same (hint, rep) completes;
  4. both ends of every S1 invocation print TUNPIPE with the hint it was given and a `resolved configuration` line whose four object-mode fields read false.
- **Reported only:** per-hint goodput and completion time, S1 against B0 (n = 5, no A/A; never scored).
- **Prediction: PASS.** The default file holds no object-mode key, so the transfer path is unchanged.

### 3.4 Guard 1's decision rule

- **MERGE guard 1** (both `c8-gauges` and `tunnel-inorder-guard`, `--no-ff`, in that order, as S1 was built) iff G1 PASS, A1 PASS, T PASS, the §2.1 item 4 red/green record exists, and the binid check of §5.3 holds.
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
- **Execution witnesses per row (scored: a row failing one is VOID-WITNESS and re-run once; a second failure on an S2 row is `S06-FAIL`, never replaced):**
  - S1 and B0: `taper=` > 0 and `plost=` = `taper=` on the last `[DIAG]` (the copy branch ran);
  - S2: `taper=` = 0, `plost=` = 0, `pslot=` > 0, and the `[SPAN]` median rr > 0 (slots were taken, so the deletion had something to act on);
  - S2: below-floor arrivals ≤ 5 000 per row. H-B, which has no copies, read 854 (c2) and 122 (c8), against 57–97 k on Auto rows with copies (Addendum 2). A higher count means copies still reach the receiver by some path: `S06-FAIL`.
  - Every S2 row reaches LIVE with completion (a DNF on S2 where every S1 row of the cell completes is `S06-FAIL`).
- **Clause AS (scored):** S2 against S1, one-sided (S2 worse), on **goodput** at c2 and c8, rank-minp over the two, 12 v 12. **AS PASS iff p > 0.05.**
  - **Prediction: PASS, goodput unchanged.** Each slot sends one symbol either way; only its content changes from a copy to coded repair (Addendum 2, "the same slots spent on coded repair").
  - Resolution: §51's in-session A/A at 6 v 6 gave c2 1 % and c8 3 %; 12 v 12 is tighter. Its realised d80 is printed from the S1 rows' half-splits.
- **Reported only (every one goes to the owner):**
  - **c8 in-order delay, two-sided,** with §41d's definition at ρ = 1. No direction is predicted (plan §6 item 3). c8's resolution is 50 % at 6 v 6 (§51), so a move below that is reported as unresolved, not as "no effect". It is confounded by the fixed store (status, "Consequence for open registrations"), so it is read beside hole-close time;
  - **hole-close time**, decode-resolved and resend-resolved, p50 and p90, from the receiver's `[SUCC]` dump (`RWM_SUCC_DUMP=1`, as in `rate-overprovision.md`: 9 ms against 229 ms on c2 HA s42 r1). It runs on dump twins, one per (seat, seed, cell) for S1 and S2, 8 rows, outside the scored rows. Prediction at c2: more holes close by decoding on S2, since the 64 % of slots that were copies are now coded; the share is predicted in 63–83 % of source losses (derivation §4.1.1). c8: unknown within [6 %, 100 %];
  - c2 in-order delay (d80 12 % at 6 v 6);
  - `cpu_cli/GB` and `cpu_srv/GB`. Coded repair costs an encode where a copy cost a store read, so the sender's CPU may rise. **Owner flag:** S2 worse than §51's d80 (8 % c2, 12 % c8) with rank p ≤ 0.05;
  - `[DIAG] retx=` per source, `[RFA] fill_coded` per source, `cod=` / `pslot=`, and the gauge prints.
- **Pool block (reported only; the first cut under the cap, §6):** c8-100 Auto, `RWM_STORE_PATH_POOL` = 170, 256 and 640, S1 and S2, n = 3 each, seed 42. That gives 18 rows, with in-order delay and hole-close time read per arm. It answers the store-law confound the status names. It decides nothing here.

### 4.3 Stage R1 (Realtime, ρ = 1, unpaced): completion scored

- c2-100, Realtime hint, `--window-reliable` on both ends, 100 MB, `RWM_PERF_TIMEOUT_S=150`, `RWM_DIAG=1`, S1 12 + S2 12, plan 64's stage R mechanism (VOID-RHO on a missing `window_reliable: true`).
- **Witness:** S1 rows `taper=` > 0, and S2 rows `pslot=` > 0 with `taper=` = 0.
  - If S1 reads `taper=0` on more than 6 of 12 rows, R1 is `R1-BLIND`: the copy branch did not act there. That is reported, and the merge then rests on Stage A and R2.
- **Clause R1 (scored):** every S2 row whose witnesses hold completes inside its timeout, wherever every S1 row completes (plan 64 item 5's rule). **R1 PASS** iff that holds and no S2 row is `S06-FAIL` (the witness rule above).
- **Reported:** goodput, CPU/GB, lag, in-order delay (marked: under the 30 s tick floor).

### 4.4 Stage R2 (the evicting seat, paced; reported only)

- **Why paced seats:** an unpaced evicting row takes almost no slots (§60: rr = 0 on 46 of 53 windows), and the pacer is not on main (plan §6 item 4). So R2 runs on P1 and P2, which never merge. It shows the deletion on the seat where the slot site used to shed; the merging binaries are the subject of Stages A and R1.
- Block E's PM shape: c2, Realtime, evicting (no `--window-reliable`), paced at 35 Mbit/s with `RWM_PERF_RATE_MBIT`. P1 6 + P2 6, seeds 42 and 7.
- **Smoke witness before the stage:** P1 `taper=` > 0; P2 `pslot=` > 0 with `[SPAN]` median rr > 0; both print the pacer echo. A failure makes R2 VOID; it goes back to the coordinator and does not hold guard 2.
- **Reported, read under ADR-0079 Amendment 1:** `shed_slot` (P2 = 0 by construction), `shed_nack`, `shed_denied`, `shed_total`, and the delivered fraction.
  - Conservation is no longer the expectation: the 2026-11-20 review measured the NACK site absorbing the slot sheds (245/90 vs 250/0 at a forced budget).
  - **Printed:** `shed_total(P2) − shed_total(P1)` with its per-seed range, and how often the stalled blocker is left unresolved (`rec-evict-blocker-unresolved`, a defect on main).
  - In-order delay is undefined at ρ < 1 (§41d item 3), so it is not read.

### 4.5 Guard 2's decision rule

- **ELIGIBLE TO MERGE** iff all of: G2 PASS, zero `S06-FAIL` rows in G2, A, R1, AS PASS, R1 PASS or `R1-BLIND`, §2.2's suites and repeats green, and §5.3's binid check.
- **Then the owner's question (§7).** On a yes, `s06-delete` merges (merged up, as S2) with `--no-ff`. The paper's present tense comes in the same merge (it is already inside `s06-delete`). ADR-0079's Status and `docs/adr/README.md` row name the merge commit, and the paper's §10 row names the deletion hash.
- **G2 FAIL:** no merge. The failing Bulk clause is diagnosed. Bulk does not touch the deleted branch, so a FAIL points at the merge-up or the §1.3 cleanup (the ε slot removal).
- **Any `S06-FAIL` or AS FAIL:** no merge. A finding against the deletion, diagnosed first.
- **Stage A's delay and hole-close readings decide nothing.** They travel with the owner's question.

## 5. Statistics, n, stop rules and binary identity

### 5.1 n per arm

| stage | cell(s) | arms × n | rows |
|---|---|---|---|
| G1, G2 | c1s, c1d, c2, c8 | 12 + 12 per cell | 96 each |
| A (shared with A1) | c2, c8 at 600 MB | S1 12, S2 12, B0 6 per cell | 60 |
| A dump twins | c2, c8 | S1, S2 × seeds 42, 7 | 8 |
| pool block | c8 | 3 pools × S1, S2 × 3 | 18 |
| R1 | c2-100 Realtime ρ = 1 | 12 + 12 | 24 |
| R2 | c2 paced evicting | 6 + 6 | 12 |
| T | c2 tunnel | 2 seats × 2 hints × 5 reps, + 16 refusal commands | 4 invocations |

### 5.2 Stop rules

- **Precondition P0:** the owner has said the Ceph instability is over. No session launches before that.
- **Session 1 (rehearsal):** any ABORT-PREFLIGHT, ABORT-TESTS, ABORT-SEAT or repeat failure on S1 stops guard 1. On S2 it stops guard 2. Guard 1 can still launch with S2 dropped.
- **Session 2, block-1 rule** (§51 Amendment 6): after each stage's first block, every arm's first real row must be LIVE with its witnesses. Otherwise the stage stops with `FAILED-<stage>`, and the next stage still runs.
- **G1 FAIL:** G2 still runs (its BASE is S1, so it is still ADR-0079's comparison). The next session bisects: a Stage G on B0 + `c8-gauges` alone and one on B0 + `tunnel-inorder-guard` alone (pre-pinned at the second amendment as fallback seats `vmb-c8` and `vmb-tun`, about 55 min each).
- **G2 FAIL, or an `S06-FAIL` in G2:** session 3 drops Stage A's S2 arm, R1 and R2, and keeps A1 (B0 against S1) and T. The S06 rows would then be for diagnosis only, and the diagnosis gets its own registration.
- **Cap cuts, in this order:** the pool block, then the dump twins, then R2. Each cut is written into an amendment before the session.

### 5.3 Binary identity after each merge (the coordinator's rule)

After each merge, before anything else lands on main:
- `binid_run.sh` on an archive of the merged main and on an archive of the guarded seat (S1, then S2), at the same path with a fresh target;
- **the two shas must be equal.** They are, by construction, when the merged tree equals the seat outside `docs/`, `tools/` and the roadmap. That source-tree check is recorded too.
- A mismatch is a hold: the merge is reverted, or the merged tree gets its own guard.

The harness's `LD_BIN_SHA` values are not used for this, because the harness builds at its own path and the binary embeds location data (§58).

### 5.4 Abort classes (filled in the Result, in this order)

ABORT-LOCK, ABORT-CRLF, ABORT-SEAT, ABORT-BUILD (sha mismatch against a pin), ABORT-PREFLIGHT, ABORT-TESTS, ABORT-REPEAT, ABORT-SMOKE, a non-zero stage exit, and bring-up exhausted (Stage T).

## 6. Rehearsal and run order

### 6.1 Session 1, `LD_PLAN=67 LD_REHEARSE=1` (`LD_HARD` = launch + 4.75 h)

The rehearsal is §2.2 in full: five builds, the suites on S2, gates on three seats, the repeat runs, the visualizer, binid on three seats, and the tools tests. Then one smoke per stage, each of which must show the stage's witnesses:
- G: plan 41d's c1s B0 and S1 rows, and a 600 MB c8 S2 row;
- A: one c8 Auto row per seat with the witnesses of §4.2, plus one dump twin with `[SUCC]` lines;
- R1: one S1 and one S2 row, with `window_reliable: true` on both ends;
- R2: one P1 and one P2 paced row, with the R2 smoke witness;
- T: one invocation per seat at `auto` with REPS = 1, plus the 16 refusal commands.

A rehearsal row is not re-run, and a failed S2 witness in the rehearsal is a finding. The second amendment then pins the five release shas, the binid shas, the repeat counts and any cap cut.

### 6.2 Session 2, `LD_PLAN=67g` (`LD_HARD` = launch + 3 h)

G1, then G2, using the rehearsal's binaries (`LD_BIN_PREBUILT_*`, sha-checked) and its suites and tools results (manifest reuse, as plan 64).

### 6.3 Session 3, `LD_PLAN=67x` (`LD_HARD` = launch + 3.5 h)

A (with A1), the dump twins, R1, R2, T, then the pool block.

### 6.4 Time estimate (4 cores, 11 GB, one session at a time)

| session | content | typical | basis |
|---|---|---|---|
| 1 | 5 builds ≈ 40 min; suites on S2 ≈ 55 min; gates × 3 and check × 2 ≈ 30 min; repeats ≤ 60 min; visualizer, binid × 3, tools tests ≈ 25 min; 9 smoke rows plus T ≈ 20 min | **≈ 3.8 h** | builds 15 min per two (§64); suites 45–60 min (§58 Part A, 60-pre); smoke as §64 |
| 2 | G1 96 rows ≈ 53 min; G2 ≈ 53 min; replacements and scoring ≈ 15 min | **≈ 2.0 h** | §64 Result: 96 rows in 53 min |
| 3 | A 60 rows × ≈ 80 s ≈ 80 min; twins 8 ≈ 11 min; R1 24 ≈ 11 min; R2 12 ≈ 16 min; T ≈ 20 min; pool 18 ≈ 25 min; scoring ≈ 10 min | **≈ 2.9 h** (≈ 2.1 h without the pool block and twins) | block 51: 76 rows of 600 MB in 1 h 44 min; §64 stage R; block E rows ≈ 62 s |
| **total** | | **≈ 8–8.7 h** | host cargo paused during each session |

**Worst case, about 13 h:** a rehearsal re-run (+3.8 h), a NO VERDICT re-run of one Stage G (+1 h) and the G1 bisection (+2 h). Every session stays under the five-hour cap.

## 7. What goes back to the owner (asked once, with the Result)

1. **S06 standalone.** If guard 2 is ELIGIBLE: merge `s06-delete` now, as S2, or hold it to land beneath the ADR-0084 engine change, as ADR-0084 Amendment 1 item 3 now reads ("not on its own")? A yes needs that sentence amended. With the question go Stage A's c8 delay (two-sided), the hole-close times, the CPU owner flag, the pool block, R2's shed table, and T2's form (sim port, or quarantined).
2. **Guard 1:** no owner question beyond the Result, unless A1 shows a print cost above its resolution.
3. **Ceph clearance** (P0) before session 1.
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
   - the dump twins with a `[SUCC]` hole-close parser;
   - the pool block (`RWM_STORE_PATH_POOL` per row, as plan 54b).
3. **R1:** plan 64's stage R row mechanism, with §4.3's witness and clause.
4. **R2:** plan 51e's paced PM rows on P1 and P2, with `RWM_PERF_RATE_MBIT` forwarded (main's `lib.sh` keeps it), and §4.4's readouts.
5. **T:** `tun_bulk.sh` invocations, the refusal commands and `scoreT` (§3.3), parsed with `verify4_parse.py tun`.
6. **Suites:** `run_suites` with ADR-0080's rule (§2.1 item 2) and `test_harness47.py` updated; the repeat stage of §2.2; `binid_run.sh` on three seats.
7. **Reuse:** prebuilt binaries and the suite/tools manifests, as plan 64.
8. **Stub paths, red first where new:**
   - `flake-not-quarantined` → ABORT-TESTS;
   - `s2-taper-nonzero` → `S06-FAIL`, not replaced;
   - `s2-belowfloor-high` → `S06-FAIL`;
   - `s1-no-gauge` → VOID-WITNESS;
   - `r1-blind`;
   - `t-refusal-missing` → T FAIL;
   - `t-base-refuses` → ABORT-SEAT;
   - `repeat-one-failure` → ABORT-REPEAT;
   - `prebuilt-sha-mismatch` → ABORT-BUILD;
   - plus `STUBRUN_FULL=1` for each token.

## 10. Review

Fable reviewed the orientation before drafting and gave the structure: cumulative seats, the S06 hold, lever B out, Ceph, the flake rule, T2 rather than T3, N = 30, hole-close time and pool arms, separate pacer seats, and vm_order as a proposal. The final review is recorded in the coordinator's register commit.

## 11. Roadmap: proposed `vm_order` (proposal only; not edited here)

| vm_order | node | lane change needed | reason |
|---|---|---|---|
| 1 | `meas-s51-c8-gauges` | `host-cargo` → `vm` | guard 1 (S1), lowest in the stack |
| 2 | `infra-tunnel-inorder-guard` | `host-cargo` → `vm` | guard 1 (S1); Stage T |
| 3 | `rate-s06-plost-derivation` | `no-cargo` → `vm` | guard 2 (S2); merge on the owner's answer (§7) |

- `rec-leverb-guard` gets no order (parked, §8).
- `meas-guard-bulk-blind` stays `no-cargo`. This batch is its first per-change use. Its "reusable plan" done criterion is met only if `harness67`'s off-Bulk stages are kept as a plan.
- `check.py` warns on a vm_order outside the vm lane, hence the lane changes. M1's place relative to these is the owner's call (§7 item 4).
