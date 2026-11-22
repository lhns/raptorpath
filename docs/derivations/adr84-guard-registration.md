# The ADR-0084 sender-side build: merge guard, pre-registration draft

Draft for the coordinator to register as its own status section. No row has run, and nothing in it can be pinned yet: BASE needs S06 merged (VM batch 1, guard 2), and CAND needs the build head. A pin amendment (§1.3) fixes both before any VM contact. Base text: github main `4832cf3c`. Spec: ADR-0084 with Amendments 1 and 2, and Amendment 3 (Accepted 2026-11-21: Q-a yes with its fallback, Q-b answered (iii)); `docs/derivations/floor-hold.md` (the derived H, its gauges §6 and fail levels §7–§8); `docs/derivations/adr84-engine-brief.md` as re-cut for Amendment 3 (§4 steps, §5 done criteria); `docs/derivations/rho-step-sites.md` §2, §5 (the re-cut) and the owner answers of 2026-11-21. House format and reused machinery: `docs/derivations/vm-batch-1-registration.md` (plan 67) and `docs/derivations/f2-guard-registration.md` (plan 64). Process rules: CLAUDE.md "No engine merge on a Bulk-only guard", the binary-identity rule (ADR-0082) and `docs/measurement-discipline.md`.

**For the owner, in plain words.**
- **What is guarded.** The sender side of ADR-0084, as the owner scoped it on 2026-11-21 (Amendment 3; the brief's re-cut):
  - a store at every ρ;
  - the rate bound (the source pauses on `O ≥ B ∨ cwnd_full`);
  - the counted verdict against the `(1 − ρ)·S_src` budget;
  - Amendment 2's last copy with every verdict, and the floor alone on a forced verdict;
  - the ack pair merge;
  - protocol v11 (`WindowFloor`, `WindowAck.passed_undelivered`), and the receiver honouring the floor (the seat-free floor pass, with the **derived H** of `floor-hold.md`: the windowed maximum over W = 10 s of RFC 4737's reordering late time offset, H = 0 on a FIFO single path, about the skew at N ≥ 2);
  - the sender (C) sites: C2 (SACK release at every ρ), C5, C12's refresh half, C3, C4, C1 (the Realtime N ≥ 2 laws the owner accepted), C7 and C6, then the sender scaffold commit.

  The receiver's own give-up (C9's expiry and force-drain, C11), the C10 drop and C12's expiry stay as today, keyed on the receiver's own ρ through Amendment 3's named exception. So this build has **two abandonment deciders**: the sender's verdict and the receiver's old arm. The old arm's passes are counted in `G_rx` but neither told nor bounded, and C10's drops are not counted (Amendment 3 item 4). The predictions below follow from that.
- **One guard for the whole head.** rho-step-sites §5's re-cut names three cut points inside this build (its guards 2, 3 and 4). This registration follows the brief's §5 instead: BASE is main with S06, and CAND is the build head. A failure is therefore attributed to the whole build, and §8.3 pre-pins the bisection seats.
- **Bulk alone cannot see this change.** At ρ = 1 the budget is 0, so no verdict, no last copy and no floor exist. The change acts at ρ < 1, so the guard has three parts:
  - a ρ = 1 identity part (Bulk, Auto, Realtime at ρ = 1);
  - a ρ < 1 part: Realtime at the declared 0.99, at N = 1 and N = 2, including a 20 ms skew cell, a budget-exhaust cell, a lost-tail cell after the allowance is spent (limit L-TAIL, named), and two floor-only pairings (sender 0.99, receiver 1; one single-path, one on the 20 ms skew link) where floor passes, last copies and the derived H's miss count are required witnesses, plus Auto at 0.99 on CAND;
  - simulation gates before any VM contact (the lost tail, the stalled blocker, the mixed pairing).
- **The binary changes on purpose.** The merge carries a `binid: expected-change` line (§6).
- **Cost:** five VM sessions. Typical ≈ 3.3 + 1.3 + 2.4 + 2.9 + 1.5 ≈ 11.4 h, worst ≈ 17 h. Each session stays under the five-hour cap.
- **Owner answers now in (Amendment 3, Accepted):** Q-a yes, with its fallback; Q-b (iii), H derived (`floor-hold.md`); C1 stays in this build (option (a)), and the interim XK delay rise is accepted and reported.
- **Your word is still needed (§11):**
  - **W = 10 s**, H's one borrowed constant, which is a default awaiting your confirmation before the floor-pass step is built;
  - whether one guard on the head is acceptable, or the three cut points of rho-step-sites §5 each get a guard.

## 1. Seats and pins

### 1.1 The seats

| seat | tree | role |
|---|---|---|
| **B84** | main after the S06 merge (VM batch 1 guard 2 PASS) + `harness84` (tools only) | BASE |
| **C84** | B84 + `git merge --no-ff <build branch>` at the build head | CAND; merges if the guard passes |
| **B84T**, **C84T** | B84 and C84 built `--release --features test-seams` | `[ACCT]` / `[ACCT-RX]` twins (§5.6); never merge |
| **PB**, **PC** (optional) | B84 and C84 + the pacer `4ee1df9b` (cherry-pick) + its `RWM_PERF_RATE_MBIT` forward line, as batch 1's P1/P2 | Stage XP only (§5.7); never merge. Built only if the cherry-pick applies cleanly to both, which the pin amendment records |

- BASE is main with S06, not S06's seat: a failure belongs to ADR-0084 alone (brief §2, Amendment 1 item 3).
- If S06 has not merged, nothing here pins. The build does not stack on an unmerged `s06-delete` (brief §2).

### 1.2 The CLI mapping per tree (ADR-0084 Amendment 1 item 8)

| row's retention | B84 (`--window-reliable` spelling) | C84 (`--window-rho` spelling) |
|---|---|---|
| ρ = 1, any hint | `--window-reliable` (today's `RWM_C_PIPELINE=window` default) | `--window-rho 1` |
| Realtime ρ < 1 | no flag (the evicting preset) | `--window-rho 0.99`, explicit (rule 15d: never rely on the default) |
| Auto ρ < 1 | **no spelling.** `perf` builds its config from the CLI only, and its CLI can set only `window_reliable = true` (`main.rs` `cmd_perf`) | `--window-rho 0.99` |

- The harness detects the spelling from the binary (`perf --help | grep -q -- --window-rho`), not from the tree name, and records it in the row's provenance (§9 item 1).
- Auto ρ < 1 therefore runs on C84 only, against C84 at ρ = 1 (Stage XA, §5.5).

### 1.3 The pin amendment (before VM contact)

It records:
- B84's sha (main after the S06 merge) and C84's build head;
- the engine patch-id of `git diff B84 C84 -- raptorpath raptorpath-math raptorpath-core Cargo.toml Cargo.lock`, equal to the build branch's own diff over the same paths;
- `git diff B84 C84 -- tools` empty, so both seats run one harness;
- CI green on C84, `binid.yml` included (with the expected-change line, §6);
- **which owed tokens the build prints, and where** (release under `RWM_DIAG`, or test-seams only; §2). A token that is test-seams only moves its clause to the twins (§5.6), and the amendment says so;
- which sender site commits the head holds (C2, C5, C12's refresh half, C3, C4, C1, C7, C6, the scaffold), with the ratchet table equal to Amendment 3's `rho1-debt-exception` block. A missing site selects the matching prediction branch below (XK's branch L if C1 is missing), and none adds a row after the pin;
- the bisection seats of §8.3 (CAND2 = steps 1–8 + C2 + C5; CAND3 = CAND2 + C12's refresh half, C3, C4, C1), with their shas;
- the release shas of every seat, pinned by a second amendment after the rehearsal (the batch-1 pattern).

**If main moves** before the launch: docs, roadmap and ledger commits change nothing. An engine commit rebuilds and re-pins the seats.

## 2. The instruments

Every BASE-against-CAND readout uses a line that main already prints. B84 is main and gains no gauge. CAND-only tokens are the brief's names, and their exact strings are fixed at the pin.

| token | on main? | gated by | endpoint | used for |
|---|---|---|---|---|
| `[DIAG] paused=`, `cum=src/…/ack`, `retx=`, `sweeps=`, `cwnd=`, `infl=` | yes | `RWM_DIAG` | sender | pause share (any disjunct), the frontier series (§4), resends |
| `[DIAG] shed=V_adm/denied bud= D=` | yes, only where `pol.shed_on` (Realtime ρ < 1 on main) | `RWM_DIAG` | sender | verdict visits and budget. **On C84 it prints at every ρ** (B1 arms the check; budget 0 at ρ = 1) |
| `[SPAN] rr=` | yes | `RWM_DIAG` | sender | route witness: the repair path ran |
| `[SUMCAP] on= eng= pin=` | yes; emitted only where `sum_cap && plain_dyn_cap`, and `plain_dyn_cap` needs `reliable` on main (`sender_policy.rs:579`), so absent at ρ < 1 on B84 | `RWM_DIAG` | sender | the pooled cap's scope (at every ρ on C84, A6) and its bind fraction (rule 18) |
| `[SHED-R] holes= frontier= budget_open=` | yes | `RWM_DIAG` (receiver gates) | receiver | the receiver's own give-ups on both seats |
| `[SUCC]` summary | yes | on | receiver | hole-close times (batch 1 §4.2) |
| `[TRUTH] loss= rcvbuf_drops=` | harness | — | per leg | loss truth (rule 19) |
| `perf configuration` (`info!(?peer_config)`) | yes: `window_reliable: <bool>` | info | both | the ρ echo (VOID-RHO). **Owed on C84:** `window_rho: <f64>` in the same line |
| `[ACCT] … O_serv= O_strand= O_max= …` | yes | **test-seams / debug only** (`net/mod.rs:48`) | sender | account identity, stranded blockers (twins) |
| `[ACCT-RX] frontier= holes_given_up= final=1` | yes | **test-seams / debug only** | receiver | receiver frontier (twins) |
| verdict echo: admitted verdicts `V_adm` (the brief's and `[ACCT]`'s `H`, renamed here so it cannot be confused with the floor hold H), forced `E`, last copies, `floor_sent` | **owed** (brief steps 6–8) | must print under `RWM_DIAG` in release | sender | X witnesses W3–W6 |
| **verdict visits `vv`**: every seq the per-seq rule judged (`judge_seq` calls), so that **age refusals = vv − V_adm − denied** | **owed, new here** (P3); main's `Account::shed_visit_nack` counts NACK-site visits on the test-seams account only | must print under `RWM_DIAG` in release | sender | the age half of the verdict (W-AGE) |
| `G_rx` and its split `G_expiry`, `G_force`, `G_floor`; `floor_rx`; `floor_passes` | **owed** (brief step 4, rho-step-sites §2 "all-seq pass count") | release `RWM_DIAG`, or `[ACCT-RX]` (twins) | receiver; `G_rx` also at the sender from the ack | W6, BI, EX |
| `[BUDGET] G_rx= budget= breach=` | **owed** (brief step 5, T7b) | not stated by the brief | sender | BI, EX |
| pause share on the `O ≥ B` disjunct | **owed** (brief step 5, T7a "the paused-iteration share on the `O ≥ B` disjunct") | not stated by the brief | sender | PW |
| `[ACCT] rho=` | owed (brief D3) | test-seams | sender | twin ρ echo |
| `[DIAG] wnd2= relgap=` | yes, gated on `store_sack_release_on` (brief D4) | `RWM_DIAG` | sender | C2's witness: present at ρ < 1 on C84 once SACK release runs at every ρ, absent at ρ < 1 on B84 |
| `released` (the step-5 bound's `\|released\|`) | owed (T7c) | release `RWM_DIAG`, or `[ACCT]` | sender | W-C2 |
| `[DIAG]` recovery trace `supp c/a/l` | yes | `RWM_DIAG` | sender | C3's witness at N = 2: `l` (suppressed by the multipath law) |
| per-path source counts | **unverified**: `mpr[…]` is per retransmit path, and no per-path source token was found on main. Fixed at the pin amendment, or owed by the build (P3) | `RWM_DIAG` | sender | C1's witness at N = 2: the slow leg's source share |
| `[QCLK] site=receiver` | yes: printed at a 1 s cadence when `recv_gates.diag` and the clock has evaluated (`receiver.rs`, the `[QCLK]` readout) | `RWM_DIAG` | receiver | C12's refresh half at ρ < 1 |
| `fh_n`, `fh_us_sum`, `fh_us_max`, `fh_zero`, `fl_n`, `fl_late`, `fl_max_us`, `fh_age_us`, `fl_trunc`, `fh_miss`, `fh_miss_us_max`, `fh_miss_refused`, `fh_fill_first` | owed (`floor-hold.md` §6) | `[ACCT-RX]`, test-seams only, so they are read on twins | receiver | the derived H, its window's age, its record clamp, and its empirical test (`fh_miss`) |

**Owed by the build (pin precondition P3, print-only, behaviour-inert):** every "owed" row above must print in the release binary under `RWM_DIAG`, or the pin amendment moves its clause to the twins. The ABORT-SEAT string check (§7) tests each string on C84 and its absence on B84.

## 3. Pre-flight gates (host, before the pin; repeated in session 0)

### 3.1 The simulation gates (`RUSTFLAGS="--cfg tokio_unstable" --features sim,test-seams`)

Each must pass on C84. Each that the brief makes red-first must also be shown failing on B84 (the step 1 record).

| gate | file | the condition |
|---|---|---|
| **T1** lost tail | `sim_lost_tail.rs` `lost_tail_ack_reaches_s_src_at_rho_below_1` | the ack reaches S_src within the brief's bound; red on B84 (ack stays at 11 800). **It also asserts, inside the test, that the tail's verdict is admitted:** `Ab + 1 ≤ budget` at that verdict (coordinator decision 2026-11-22). So it cannot pass only because its `dead` window ends. The pre-spent-allowance variant is the record below (L-TAIL), and it becomes a gate in the release-rule build |
| **T1′** last seq | `last_seq_lost_is_passed_by_a_floor` | k = S_src − 1 shed and passed by a floor; `floor_sent ≥ 1`, `floor_passes ≥ 1` |
| **T2** stalled blocker | `sim_shed_reach.rs` `shed_blocker_ack_reaches_s_src` | ack 12 147; red on B84 (12 087) |
| **the ablation** | `ablation_without_the_floor_t1prime_and_t2_stall` | under `RWM_TEST_TS_NO_FLOOR=1`, T1′, T2 and T1 at 0.97 stop short; T1 at 0.99 stays live |
| **mixed pairing, forward** (`rec-mixed-rho-stall`) | `sim_mixed_rho::mixed_realtime_sender_bulk_receiver_ack_reaches_src`, plus the same-hint variant (rho-step-sites §4) | the ack reaches S_src. Red on B84 (ack 49 of 12 147) |
| T3 | `rho1_controls` | ρ = 1: `client_ok`, V = ∅, `floor_sent = 0`, `G_rx = 0` |
| T4, T5, T5′ | floor loss, ack loss, never-ack, dead peer, shutdown; N = 2 frozen leg and slow-leg fills; N = 1 overtaking | as the brief |
| T6 | `in_capacity_abandonment_within_budget` | `G_rx ≤ (1 − ρ)·S_src`; refusals rise when `Ab + 1 > budget` |
| T7a, T7b, T7c | the pause executes; `O ≤ B + 1`; `[BUDGET]` present; C2's step bounded | as the brief |
| Amendment 2 | `last_copy_sent_with_each_verdict`, `forced_verdict_sends_floor_alone`, `ack_pair_merge_survives_reordered_acks` (three cases) | as the brief |
| B1 at ρ = 1 | `shed_arm_at_rho_1_sheds_nothing` | `shed_visit_nack > 0`, `D > 0`, `shed_denied > 0`, `V_adm = 0` |
| the deny test | `rho_deny_list` | red on its fixture, green on the tree |
| the decision pin | `decision_trace_pin` | unchanged, except the `shed_allowed` digest re-captured at step 7 (named in that commit) |
| the sender sites | `c01_placement_law_at_every_rho`, the C2 property test with a verdicted tail and T7c inverted, `c03_hole_law_at_every_rho`, `c04_recov_sp_scope_reads_no_rho`, `c05_burst_stops_at_the_pause_below_1`, `c06_request_is_judged_like_a_gap`, `c07_feed_never_credits_a_verdict` (R1 cell), `c12_refresh_runs_below_1` (ε̂_recv budget spent) | as rho-step-sites §2 |
| the derived H (`floor-hold.md` §7–§8) | the absolute H tests (N = 1 FIFO gives 0; a 3 ms late batch gives 3 ms and is back to 0 after W; N = 2 with 20 ms skew gives [20 ms, 20 ms + J]; an idle slow path gives 0; an empty window gives 0; a later floor never delays a covered hole); the signature test (no hint, δ or ρ); `last_copy_carries_a_fresh_stamp`; `fh_us_max = 0` asserted in T1, T2 and T1′; the loopback `floor_never_overtakes_its_copy_real_io` at N = 1 (`fh_miss = 0`) and N = 2 (within §6's bar) | as `floor-hold.md` |
| the receiver's debt | `rho1_debt_bounds.rs` for C9, C10, C11; the deny test's ratchet table equals Amendment 3's `rho1-debt-exception` block | as the brief's step 9 |
| expected failures | `python tools/ci/expected_failure.py --list` | every still-ignored test fails. That includes `sim_rho_dial` (re-prefixed `model-ordering-hold:`), `rho1_debt_is_gone` and the reverse pairing (`mixed_bulk_sender_rho_below_1_receiver_delivers_everything`, `rec-mixed-rho-stall:`). **All three stay red in this build**, because the receiver keeps its old arm (Amendment 3) |

**Which cell each assertion runs in** (brief step 1, Amendment 3 Consequences):
- **R1 cell** (sender ρ < 1, receiver ρ = 1, same hint): floor-only by construction, because the ρ = 1 buffer passes nothing on its own. The budget, pass-count and last-copy assertions run here: T2's shed witness, T6, T1′, the two Amendment 2 tests, the ablation, T5 and `c07`.
- **Matched cell** (both ends at ρ < 1): liveness and the account only (`assert_frontier_live` items 1–3), with `G_expiry`, `G_force`, `G_floor` and the `[BUDGET]` breach line printed. T1, T2 and T7a run here.

Two-run digest equality holds for every sim test above.

**Record (owed, host, not a gate): the lost tail after the allowance is spent, matched cell.** `sim_lost_tail`'s scenario at ρ = 0.99 on both ends, with the allowance pre-spent by the old arm's passes. A forced budget seam (`RWM_TEST_TS_BUDGET_FRAC`) at a value the scenario's early expiry passes exhaust is one way, recorded with its value. Two variants:
- k lost once: resent and filled;
- k under `dead`.

Recorded: whether the ack reaches S_src, by which writer (fill or `G_floor`), and when.

The derivation predicts:
- **Not stranded.** The store holds k at every ρ, so a refused verdict means "served until filled" (ADR-0084 decision 1). The tail resolves by a fill, not a floor.
- **Under `dead` it stalls for good.** No fill ever comes, the budget refuses the verdict, and the old arm cannot pass a hole with nothing above it. This is ρ = 1's behaviour, reached at ρ = 0.99 because the old arm's passes spent the allowance first.

That is **limit L-TAIL of this build**. It is derived here, not yet run: at a matched ρ < 1 pair, once the old arm has spent `(1 − ρ)·S_src`, a lost tail waits for its fill with no time bound. It is named here, carried to the owner beside Amendment 3's fallback, and read at L1 in Stage XL (§5.5b). The record becomes a test in the release-rule build, where the old arm leaves.

### 3.2 The ρ = 1 simulation record (host; record-only)

The same seed on B84 and C84 at ρ = 1, in the four cells of rho-step-sites §2: Bulk and Realtime hints, N = 1 and N = 2, the `sim_lost_tail` scenario, seed 42.
- **Asserted on C84:** V = ∅, `V_adm = E = 0`, `floor_sent = 0`, last copies = 0, `G_rx = 0`.
- **Recorded:** the per-seq data sends and resends of each tree, and their diff.
- **Prediction:** identical per-seq sends at N = 1. The trace is not byte-identical, because v11 adds 8 bytes to every `WindowAck`, which moves ack timing. At N = 2 any difference is named. Candidates:
  - the pair merge, if B84's sender ever let a reordered ack lower its frontier;
  - the 8-byte timing shift.

  A difference with no named cause is a finding, not a pass.

### 3.3 Session 0 on the VM (no battery; fresh target dir per tree)

| step | trees | rule |
|---|---|---|
| `cargo check --all-targets` | C84 | an error is ABORT-PREFLIGHT |
| `cargo test -p raptorpath --lib gates` | B84 | a failure on C84 that is not equal on B84 is ABORT-PREFLIGHT |
| full suites (`run_suites` with ADR-0080's rule): release, doc, wasm (`env -u GOLDEN_CAPTURE`), debug, core, seams | C84 | REAL or non-quarantined FLAKE is ABORT-TESTS |
| the §3.1 sim gates, named | C84, and the red-first ones on B84 | any departure is ABORT-PREFLIGHT |
| repeats of the touched loopback tests | C84: every non-ignored test in the files the build touches that are not `sim_*` | **N = 30, zero failures**, with the 60 min cut rule of batch 1 §2.2 |
| `build_visualizer.sh` and the native golden test without capture | C84 | as batch 1. If the build includes step 10, the re-capture is checked against its commit message |
| `binid_run.sh` | B84, C84 | the crate-directory shas, the reference for §6 |

## 4. Definitions used at ρ < 1

**Completion and goodput are not clauses at ρ < 1.** A perf object needs every chunk, so a row whose receiver passes a single hole is DNF by design (§63, batch 1 §4.4). Rows run a **timed window**: 100 MB at `RWM_PERF_TIMEOUT_S=40`. A row that ends at the timeout is labelled `WINDOW-END`, which is not a failure. A row that completes inside the window is `COMPLETE`. Its readouts are computed over its own ticks and marked short; this is expected for XA's ρ = 1 arm (about 12 s) and possibly for XL's 20 MB object. **VOID-SHORT applies only to a `WINDOW-END` row with under 30 s of ticks**, a process that died early. A `COMPLETE` row is never VOID-SHORT.

**The warm-up at ρ < 1.** `perf` sends a one-chunk warm-up object first, under the same `run_timeout`, and bails with "warm-up object never acked" on a DNF (`perf.rs`). A passed warm-up chunk therefore ends the row after one full window with no timed transfer.
- Such a row is `WARMUP-DEAD`: a reading per arm, k of n. It is never VOID and never replaced, and it does not count toward the 4-slot cap or the block-1 rule.
- Its expected rate is about the chunk's loss-and-pass probability: small at c2, higher at XE and XL, and possibly high on XR's B84 arm, where the mixed stall is the predicted defect.
- **WARMUP-HOLD:** the counts are compared per cell between the seats. If C84 has more `WARMUP-DEAD` rows than B84 at any cell, the merge is held until the rows are read. The counts are small, so this is a strict count comparison, not a test, and it is stated as such.
- A cell with more than 4 `WARMUP-DEAD` rows on one seat is UNSCOREABLE, with k printed, and **an UNSCOREABLE cell makes the guard not eligible** (§7). The one exception is XR's and XRK's B84 arm: there a dead warm-up is the predicted mixed-stall defect, B84 carries no scored clause, and the count is reported as the defect reading.
- Each such row costs a full 40 s window, which §8.2 budgets at the worst case (no saving). A SIGKILLed process writes no `final=1`: twins read the last periodic line, labelled `ACCT-LAST` (batch 1 §4.4).

**Frontier delay.** The in-order delay of §41d is undefined at ρ < 1, so this guard defines its own readout. On every `[DIAG]` tick:

```text
   d(t)  =  t − t_src(A(t))
```

- A(t) is the third `cum=` field, the receiver's cumulative ack as the sender holds it, and t_src inverts the cumulative source-sent curve (`threadp1_parse.inorder_delay`, unchanged).
- A passed hole advances A, so d(t) is the delay of the *frontier*, deliveries and give-ups together. It is never read without its pass count beside it:
  - B84: `[SHED-R] holes=` (passes, with no late-fill subtraction);
  - C84: `G_rx` (passes minus late fills).

  The two are defined differently, and that is printed.
- Read per row: p50 and p99 over ticks, plus the **frontier stall**, the longest run of ticks with A unchanged while `src > A`.
- **Resolution:** one `[DIAG]` tick; the rehearsal measures the cadence. A stall or delay change shorter than one tick is invisible (§10). There is no Realtime A/A ledger, so every relative readout prints an **in-session resolution**: the d80 of each arm's own half-splits (relabelled within the arm, 10 000 draws, seed 4141), as batch 1 printed for its S1 rows.

**Budget at time t:** `budget(t) = ⌊(1 − ρ)·S_src(t)⌋`, with `S_src(t)` the first `cum=` field (source symbols sent).

## 5. Stages

### 5.1 Stage G (scored, Bulk, ρ = 1): plan 41d unchanged

- **Rows:** 96 in plan 41c's order:
  - c1s-400 and c1d-400 at the guard size;
  - c2-100 and c8-100 at 600 MB, `RWM_PERF_TIMEOUT_S=200`;
  - 12 + 12 per cell, seeds 42 and 7, `RWM_DIAG=1`, mapped per §1.2.
- **Clauses (v2, §41d):**
  - goodput, `cpu_cli/GB`, `cpu_srv/GB`, `lag_E_cli` and `lag_E_srv` per cell;
  - the in-order delay mean at c2 and c8.

  Rank-minp, one-sided, 10 000 joint relabellings, seed 4141. **G84 PASS iff p > 0.05.** Voids and replacement as §41d (VOID-SHORT re-run, a DNF keeps its worst rank, at most 4 replaced slots, more is NO VERDICT).
- **Expected resolution (d80, the latest v2 ledger):**
  - goodput: c1s 5 %, c1d 8 %, c2 1 %, c8 2 %;
  - `cpu_cli/GB`: c1s 2.4 %, c1d 8 %, c2 5 %, c8 5 %;
  - in-order delay: c2 8 %, c8 20 %.
- **Identity witnesses on every C84 row (`ID-FAIL` on first sight; never VOID, never replaced):**
  - `[DIAG] shed=0/<d> bud=0.0000` on the last tick: `V_adm = 0` at budget 0;
  - the verdict echo reads `V_adm = E = 0`, last copies 0, `floor_sent = 0`;
  - the receiver's `G_rx = 0` and `floor_rx = 0`.
- **Seat and spelling witnesses:**
  - B84's config line reads `window_reliable: true` on both ends, and C84's reads `window_rho: 1.0`. Otherwise the row is VOID-RHO.
  - B84 rows print no `shed=` at Bulk, because B84's arm is off at ρ = 1. That is a seat check, not a clause.
- **Reported:** `shed_denied` per C84 row. It is now non-zero at Bulk wherever a NACK-resent hole ages past D (brief §1 B). The plan 41d parser must read these rows unchanged (stub `bulk-shed-field`, §9).
- **Prediction: PASS.** Bulk sends no repair slots, the store and the pause are identical by data at ρ = 1, and the budget is 0. What remains:
  - one shed check per past-D NACK visit, a few ns each;
  - 8 more bytes per `WindowAck`.

  The registered risk is the 5 % false-fail rate.

### 5.2 Stage A (scored, Auto, ρ = 1): the off-Bulk no-regression check

Auto sends proactive repair and NACK resends, so the repair path runs, but at ρ = 1 the budget is 0.
- **Cells:** c2-100 and c8-100, Auto, ρ = 1, 600 MB, `RWM_PERF_TIMEOUT_S=200`, `RWM_DIAG=1`.
- **Arms:** B84 12 and C84 12 per cell, seeds 42 and 7, interleaved (B, C, B, C). That gives 48 rows.
- **Precondition witnesses** (a failing row is VOID-WITNESS and re-run once, at most 4 replaced slots in the stage):
  - the `[SPAN]` median rr > 0 on both seats: the repair path ran;
  - the ρ echo (§5.1).
- **Identity witnesses on C84:** as §5.1 (`ID-FAIL`).
- **Minimum:** fewer than 10 live rows per seat at a cell makes it UNSCOREABLE, and the guard not eligible (§7).
- **Clause AS84 (scored):** C84 against B84, one-sided (C84 worse), on goodput, `cpu_cli/GB` and `cpu_srv/GB` at c2 and c8. Rank-minp over the six, 10 000 relabellings, seed 4141. **AS84 PASS iff p > 0.05.**
  - **Resolution:** §51's in-session A/A at 6 v 6 was c2 goodput 1 % and CPU 8 %, and c8 goodput 3 % and CPU 12 %. 12 v 12 is tighter; the realised d80 is printed from the half-splits.
  - **Prediction: PASS.**
- **Reported:**
  - c8 in-order delay, two-sided;
  - `shed_denied` per C84 row;
  - `paused=`;
  - `[SUMCAP] pin=`, the bind fraction (F01 PINNED: expected ≥ 0.9; reported as "the bound operates as a constant");
  - `retx=`, `[SUCC]` close times.

### 5.3 Stage R1 (scored, Realtime, ρ = 1): completion

- c2-100, Realtime, ρ = 1 per §1.2, 100 MB, `RWM_PERF_TIMEOUT_S=150`, `RWM_DIAG=1`. B84 12, C84 12, interleaved. 24 rows.
- **Clause R1-84:** every C84 row completes inside its timeout wherever every B84 row completes (plan 64 item 5's rule), with zero `ID-FAIL`. **R1-84 PASS** iff both hold.
- **Reported:** goodput, CPU/GB, lag, in-order delay (marked: under the 30 s tick floor).
  - No Realtime A/A exists. **Owner flag:** C84 worse than c2's Bulk guard-size d80 (goodput 2.4 %, CPU 8 %) with rank p ≤ 0.05.
- **Prediction: PASS.** At ρ = 1 each sender (C) site's one law reduces to today's ρ = 1 arm (rho-step-sites §2's pins), the receiver keeps its ρ = 1 arm, and the budget is 0.

### 5.4 Stage X (Realtime ρ = 0.99): witnesses and invariants scored, readouts reported

**Cells.** All are Realtime, 100 MB, a timed window at `RWM_PERF_TIMEOUT_S=40`, `RWM_DIAG=1`. B84 12 and C84 12 per cell, seeds 42 and 7, interleaved.

| cell | shape | N | regime | why |
|---|---|---|---|---|
| **X1** | `c2-100`: c2 single | 1 | unpaced, so the source is bounded only by cwnd and B | the verdict, the last copy and the floor on a lossy short-RTT path |
| **X2** | `c8-100`: c2 + c3 dual (5 ms and 20 ms one way, 100 and 20 Mbit/s) | 2 | unpaced | N = 2 with asymmetric legs; the floor on every live path; reordered acks |
| **XK** | `c2k20-100` (**new cell row**): c2 + `c2r50` dual (5 ms + 3 ms jitter against 25 ms, no jitter; both 100 Mbit/s, GE 1.3/50) | 2 | unpaced | **the two-path delivered-delay readout with 20 ms skew** |
| **XE** | `c2r100l10-100` (**new cell row**): `c2r100l10` single (100 Mbit/s, 50 ms one way, GE 5.56/50, ε ≈ 10 %) | 1 | unpaced | **the budget-exhaust cell** (below) |

The two new cell rows reuse existing scenarios: only a `cell_spec` row each is new, no new netem line.

**Why XE exhausts, and who spends the allowance (prediction, not a premise).**
- At ρ = 0.99 the budget is 1 % of S_src.
- The receiver keeps its old arm (Amendment 3 item 4). It passes a hole with an entry above it `SRTT/2 ≈ 50 ms` after that entry is buffered, while its ε̂_recv budget is open, and after `(4·SRTT).clamp(60, 300) ms` once that budget is spent. A resend needs at least one RTT, 100 ms. So most holes are passed by the receiver's own expiry before they can be filled.
- **Those passes spend the sender's allowance first.** They arrive in `G_rx = G_expiry + G_force + G_floor` (net of late fills) on every ack, and so in `Ab`. `G_expiry` grows at roughly ε minus the late fills, an order of magnitude above 1 %. So `Ab + 1 > budget` early in each row, and from then on almost every verdict is refused.
- Predicted shares at the row's end: `G_expiry` ≫ `G_floor`; `G_floor` small, set by the verdicts admitted before the old arm filled the allowance; `G_rx > budget` with `[BUDGET] breach > 0` on most C84 rows. This is the printed breach Amendment 3's Consequences names, not a fail.
- **After exhaustion, a hole with an entry above it still passes on the old arm's hold**, so no serialization is predicted mid-stream. A hole with nothing above it (a lost tail) cannot; that is Stage XL (§5.5b).
- **Witness EX:** on a C84 row, the denied count rises across ticks on which `Ab + 1 > budget(t)` (read from `[BUDGET]`).
- **EX-REACHED** iff EX holds on at least 6 of 12 C84 rows.
- **Otherwise XE reads `XE-NOT-EXHAUSTED`, which holds the merge until it is read.** It would mean that the prediction that the receiver's old arm spends the allowance is wrong, and every matched-cell reading in §5.4 (the admitted-verdict table, `NO-VERDICT-ADMITTED`, W3 on denials) rests on that prediction. The read decides whether the matched-cell readings stand. If they do not, they are re-scoped in an amendment before any of them is reported. The exhaust reading itself then becomes `NEEDS-MORE-blackout` (a blackout path event, §10, which this registration does not build).

**Precondition witnesses (both seats; a failure is VOID-WITNESS, re-run once, at most 4 replaced slots per stage):**
- the ρ echo: B84 `window_reliable: false` and C84 `window_rho: 0.99`, on both ends (VOID-RHO);
- `[DIAG]` carries `shed=` (the arm is on at ρ < 1 on both seats);
- 30 s of ticks on a `WINDOW-END` row (VOID-SHORT, §4).

**Execution witnesses on every C84 row (scored; `X-FAIL <item>` on first sight; never VOID, never replaced):**
- **W1 the pooled cap at every ρ:** `[SUMCAP]` is present with `eng > 0` (A6 scopes it at ρ < 1). B84 prints no `[SUMCAP]` at ρ < 1. The store fill itself is read on the twins (`[ACCT]` `O_serv`, §5.6).
- **W3 verdicts visited:** `V_adm + denied > 0` on the last tick, required on at least 10 of 12 C84 rows per cell (a quiet row is not a failure). Realtime's `D = RTprop/2` makes past-D holes common at every cell.
- **W4 last copy:** last copies = `V_adm − E` **at the row's last tick** (Amendment 2 decision 1: one copy per verdict, none on a forced one). The brief does not guarantee that the verdict count and the copy count move in one step before a `[DIAG]` tick prints (step 7 sends the copy "at the same instant", not under one counter update), so a mid-row tick may sit between the two; per-tick equality is not asserted. If the pin amendment finds both counters updated in different loop iterations, the last-tick tolerance is 1.
- **W-AGE, the age half of the verdict (printed in every X cell):** age refusals = `vv − V_adm − denied`, per row. `denied` counts budget refusals only, so this is the only reading of `age > D` refusing. The toy's stage 1 never saw the age gate refuse in any cell (roadmap, toy stage 1). At Realtime, D = RTprop/2 is short, and a refusal needs a gap report younger than D, so 0 is likely and is not a failure here. It is scored at XA (§5.5).
- **W5 forced verdicts:** `E = 0`. A non-zero reading is `FORCED-NONZERO`. It is a finding (F1's full penalty), not a void, and it holds the merge until it is read.
- **W6 the floor told, on both ends** (rule 6):
  - sender: `floor_sent > 0` on every row with `V_adm > 0`;
  - receiver: `floor_rx > 0` on the same rows;
  - `floor_passes` and `G_floor` are reported here, with no lower bound. In this matched cell the old arm usually passes a hole before its floor arrives, so a floor pass is not expected. **The floor pass is a required witness in Stage XR (§5.4b)**, the floor-only pairing, where nothing else can pass a hole.
- **BI the sender's budget:** at the row's last `[BUDGET]` line, `G_floor ≤ budget`. `G_floor` is the only share that the sender's verdict decides. `G_expiry` and `G_force` are the receiver's own, and they are printed, not bounded. A total `G_rx > budget` is the printed breach ADR-0084 decision 3 allows, and it is reported (prediction: at XE on most rows).
- **PW the pause:** the share of paused iterations on the `O ≥ B` disjunct, with the cwnd share beside it.
  - Scored at XE: PW > 0 on at least 6 of 12 C84 rows. XE's BDP is about 2 400 symbols of 512 B, above B = 2 048, so O can reach B before cwnd binds.
  - **At XE, PW below 6 of 12 is `X-FAIL PW`.** `INERT-AT-CELL` does not apply there, because the derivation puts XE's BDP above B.
  - In the other cells it is reported. Where cwnd binds first, PW = 0 reads `INERT-AT-CELL`; T7a/T7b carry the executes proof.
  - B is PINNED at N·2048 (ledger F01, ADR-0084 Consequences). Under rule 18 the bound "operates as a constant" here, and no law verdict on B is drawn from this stage.
- **W-C2, SACK release at every ρ:** `wnd2=` is present at ρ < 1 on every C84 row (the routing, which is deterministic). `released > 0` on at least one tick is required on at least 8 of 12 rows per cell (a stochastic counter, pooled). B84 prints no `wnd2=` at ρ < 1 (seat check).
- **W-C3, at N = 2 (X2, XK):** the recovery trace's `supp … l` (suppressed by the multipath law) is > 0 on the last tick of at least 8 of 12 C84 rows per cell (pooled). B84's `l` at ρ < 1 is printed; it is predicted 0, because main runs the age gate there.
- **W-C1, at N = 2 (X2, XK):** the slow leg's source share is > 0 on at least 10 of 12 C84 rows per cell (the law replaced the best-path pick). B84's share is printed, predicted ≈ 0. Its token is fixed at the pin (§2). If the pinned head lacks C1, XK takes branch L below, and the pin amendment drops this witness.
- **W-C12, the refresh half (XE):** on C84 receiver rows whose `[SHED-R]` shows `budget_open=false`, `[QCLK] site=receiver` re-advertises at ρ < 1. Where the ε̂_recv budget stays open and SRTT < 200 ms, the old hold expires first and W-C12 reads `INERT-AT-CELL` (brief step 9, C12 timing).

**Admitted verdicts in the matched cells: predicted, and what each cell can witness.** A verdict is admitted only while `Ab + 1 ≤ budget(t)`. In a matched cell, the old arm's passes enter `Ab` through `G_rx` from the start of the row. Holes pass about `SRTT/2` after the next entry, before any resend (≥ 1 RTT) can fill them. A late fill subtracts, but the sender sees a passed hole as acked and sends no resend for it, so few late fills come.
- So `G_rx` grows at roughly the link's loss rate net of fills in flight. Wherever that exceeds 1 − ρ = 1 %, `Ab` crosses `budget(t)` almost at once, and only the first verdicts of a row are admitted.

| cell | ε (truth) | predicted admitted verdicts `V_adm` per row | what the cell witnesses |
|---|---|---|---|
| X1 | ≈ 2.6 % | 0 to tens: only before `G_rx` overtakes 1 % of S_src | **the refusal path**: W3 passes on `denied` alone. W4 and W6 are likely empty |
| X2 | c2 leg 2.7 %, c3 leg 4.1 % | 0 to tens; the old arm also passes slow-leg packets (C1 striping) | as X1 |
| XK | ≈ 2.6 % per leg | 0 to tens | as X1 |
| XE | ≈ 10 % | 0 to a few | the refusal path (EX) |
| XA (Auto) | ≈ 2.6 % | 0 to tens; Auto's longer D admits fewer | as X1 |

- **Consequence, stated:** the matched cells do not witness the verdict's last copy or its floor. W4 and W6 are scored only on rows with `V_adm ≥ 1`. A cell where every C84 row has `V_adm = 0` reads `NO-VERDICT-ADMITTED` for W4 and W6: it witnesses nothing for them, and that is recorded, not failed.
- **W3 is then witnessed by `denied` only**, which shows that the budget conjunct was reached and refused, not that a verdict executed.
- The verdict, the last copy, the floor and H are witnessed in **XR and XRK** (§5.4b, §5.4c), where the receiver's old arm cannot spend the allowance.
- If the matched cells admit more verdicts than this table predicts (`V_adm` in the hundreds), that is reported as a finding: `G_rx` then grows more slowly than the derivation says.

**Seat absence on B84 (ABORT-SEAT before any row):** B84's binary holds none of the owed strings, and C84's holds every one.

**Minimum:** fewer than 10 live rows per seat at a cell makes that cell UNSCOREABLE, and the guard not eligible (§7). It is never a dropped cell. **Stage X PASS** iff no cell is UNSCOREABLE, no cell has an `X-FAIL`, and EX-REACHED or `XE-NOT-EXHAUSTED` is recorded.

**Reported readouts (two-sided rank p, in-session resolution, owner flag at p ≤ 0.05 beyond the resolution).**

| readout | X1 | X2 | XK | XE |
|---|---|---|---|---|
| frontier delay p50 / p99 (§4) | C84 ≤ B84 or within resolution (N = 1: C1 is the one path) | **C84 > B84**: striping onto the 20 ms leg (C1), as XK's branch C1, with up to the c2–c3 skew of 15 ms plus the hold | **the skew readout**, below | within resolution: no mid-stream serialization in this build |
| frontier stall (longest frozen run) | C84 ≤ B84: unclamped gaps are served, not stranded | unknown: serving lowers it, striping raises it | as X2 | **within resolution** (below) |
| A/src at the window end | C84 ≥ B84 | as X1 | as X1 | unknown direction |
| `[TRUTH]` loss per leg | C84 < B84: the pause stops overflow loss that B84's unbounded ρ < 1 source causes | as X1 | as X1 | C84 ≤ B84 |
| `retx=` / source | C84 > B84: the whole gap is served (A3) | as X1 | as X1 | C84 > B84 |
| passes: B84 `[SHED-R] holes=` against C84 `G_rx` (with `G_expiry`, `G_force`, `G_floor`) | printed side by side, defined differently (§4) | | | `G_rx > budget` on most C84 rows |
| `V_adm`, `denied`, age refusals (`vv − V_adm − denied`), last copies, `floor_sent`, `floor_passes` | printed | | | denied ≫ `V_adm` |
| `[SUMCAP] pin=`, `paused=` | printed | | | |
| `[SUCC]` close times p50 / p90 | printed | | | |

**XE's mid-stream stall prediction, stated so it can fail.** With the old arm in place, a hole with an entry above it still passes after 60–300 ms once the budget is spent. So no mid-stream serialization (R1's cost, rho-step-sites §1.2) is predicted. **A C84 stall p99 beyond B84's half-split resolution at XE is a finding**: something serializes that this build was not meant to change. It is reported to the owner with the merge question.

**C3's interim effect (X2, XK).** C3's suppression can delay a resend past the old expiry at N ≥ 2 (rho-step-sites §8). Predicted `G_expiry` C84 ≥ B84's `[SHED-R] holes=`; it is printed with its rank p (rho-step-sites §5 reads "`G_expiry` against CAND2").

**XK, the delivered-delay readout with 20 ms skew.** The owner accepted the Realtime N ≥ 2 law changes (C1 placement, C3 per-flight hole law, gap-report coalescing at every N). The re-cut brief (main `2d203c19`, step 9) puts C3, C4 and C1 **in this build**, after C12's refresh half. So branch C1 is the registered prediction; branch L applies only if the pin amendment finds C1 missing from the head (§1.3).
- **Branch C1 (registered).** W-C1 is scored: the routing witness that the law replaced the pick.
  - Predicted: frontier delay p50 and p99 rise on C84, by up to the 20 ms skew plus the receiver's hold. Striped packets on the 25 ms leg arrive behind the fast leg's (`ordering-hold.md` §5, the C1 coupling), and the in-order frontier waits for them. With the old arm, a slow-leg packet that is still in flight when the hold expires is passed and filled late, so `G_expiry` and the late-fill count rise too. Both are printed.
  - The rise is reported to the owner in ms, beside `G_expiry`. There is no scored no-regression clause, because the owner accepted the change; an owner flag fires if the p50 rise exceeds the 20 ms skew plus B84's half-split resolution.
  - `mpd_supp_law` (W-C3), the per-flight fire counts and the per-leg source shares are printed.
  - The reordered acks from the 25 ms leg engage the pair merge, which no gauge shows (§10).
- **Branch L (fallback: C1 not in the pinned head).** C84 keeps the best-path pick, so the source goes to the cheaper leg on both seats. Predicted: frontier delay p50 within resolution, and p99 C84 ≤ B84, because blockers are served, not stranded.

### 5.4b Stage XR (scored): the floor-only pairing, sender ρ = 0.99 and receiver ρ = 1

**Why.** In the matched cells the old arm passes most holes before their floor arrives, so a floor pass cannot be required there. With the receiver at ρ = 1, its buffer never passes a hole on its own (`G_expiry = G_force = 0` by construction; brief step 1's R1 cell). Every abandoned seq is then passed by a floor or not at all. This is also `rec-mixed-rho-stall`'s forward pairing, at L1.

**Cell.** c2, Realtime, **300 MB**, a timed window of **45 s** (`RWM_PERF_TIMEOUT_S=45`; a new cell row `c2-300`), `RWM_DIAG=1`. The object and window are sized for power, below. Per-end ρ (a new harness path, §9 item 1):

| seat | client (sender) | server (receiver) |
|---|---|---|
| B84 | no flag (evicting) | `--window-reliable` |
| C84 | `--window-rho 0.99` | `--window-rho 1` |

12 + 12 rows, seeds 42 and 7, interleaved. The ρ echo is read per end (VOID-RHO).

**Binaries.** The H gauges are on `[ACCT-RX]`, which is test-seams only (`floor-hold.md` §6). So the scored CAND arm is **C84T**, 12 rows. **C84**, the release binary, adds 6 rows interleaved with it. They carry the release-visible witnesses (`floor_sent`, last copies, `[BUDGET]` where printed) and R-LIVE, and show that the release binary behaves as its twin does. B84 runs 12 rows. That gives 30 rows.

**Required witnesses** (`X-FAIL <item>`; never VOID). These are the old-arm-free execution witnesses that §5.4 cannot require. They are read on C84T, and on C84 where the token is release-visible:
**Power, sized before the launch (per row, C84T).**
- **Source symbols:** Realtime chunks carry 494 B. c2 at ρ = 1 Realtime ran about 69 Mbit/s (§64's committed rows), so 300 MB, about 607 k source symbols, is sent in about 35 s, inside the 45 s window.
- **Admitted verdicts:** at Realtime, `D = RTprop/2 ≈ 5 ms`. A lost seq is reported by the receiver at the next arrival, and it reaches the sender about one owd later, past D. So nearly every loss that coded repair has not filled is judged on its first report.
  - In this cell the allowance is spent only by `G_floor` (about ε·V_adm, ≈ 0.07 % of S_src) plus the pending verdicts, well under 1 %, so the budget does not bind.
  - So `V_adm` ≈ losses not filled by repair: ε ≈ 2.6 % of 607 k ≈ 16 k losses, of which repair fills an unknown share. **Predicted `V_adm` ∈ [3 000, 15 000] per row.**
- **Floor passes:** a verdict seq is floor-passed when its last copy is lost too, probability ε_copy ≥ ε ≈ 2.6 % (a copy sent in a GE bad state is lost more often, which only raises it). **Predicted 80–390 per row; pooled over 12 rows, 940–4 700.**
- **Against the thresholds:** R-W6's counted-row threshold (`V_adm ≥ 300`) is cleared about 10× at the low end, and XRK's 300 pooled floor passes about 3×. At the earlier 100 MB, the low end was about 1 000 verdicts per row, so the pooled floor passes would have sat at the 300 bar. That is why the object and window were raised.
- The rehearsal checks this power (§8.4).

**Required witnesses:**
- **R-W3:** `V_adm > 0` on at least 10 of 12 rows. The verdicts are admitted, because only the sender's own unfilled verdicts spend the allowance.
- **R-W4 last copy:** last copies = `V_adm − E` at the last tick (as W4), and last copies > 0.
- **R-W6 floor passes:** `floor_sent > 0`, `floor_rx > 0` and **`floor_passes > 0`**, with `G_floor > 0`, on every counted row.
  - **A counted row is one with `V_adm ≥ 300`.** The threshold is set by the false-fail rate. With λ = ε·V_adm ≥ 0.026 × 300 = 7.8, a correct row shows zero floor passes with probability e^(−7.8) ≈ 4·10⁻⁴, so **over 12 rows the clause fails a correct build with probability ≈ 0.5 %** (under 1 %). The earlier threshold, ε·V_adm = 5, gave about 8 %, and a fail is never replaced.
  - Rows with `V_adm < 300` are printed and are not counted against the clause.
  - **R-W6 holds** iff it holds on every counted row, and at least 8 of 12 rows are counted. With fewer than 8, XR reads `NEEDS-MORE-verdicts`, and the guard is not eligible.
- **R-ONLY the floor passes:** at the row's last receiver line, `G_expiry = G_force = 0` and `G_rx = G_floor`.
- **R-BI the 1 − ρ promise, kept in this cell, at every prefix:** `G_rx ≤ budget(t)` at **every** `[BUDGET]` line of the row, not only the last. The promise holds per prefix, and `G_rx` can fall on a late fill, so a final reading can hide an earlier breach.
- **R-H the derived H at N = 1** (`floor-hold.md` §6–§8):
  - `fh_miss = 0` on every row. **Any miss fails the stage at N = 1** (`X-FAIL R-H`);
  - `fh_miss_refused = 0` and `fl_trunc = 0` on every row;
  - predicted: `fh_us_max = 0`, `fh_zero = 1` and `fl_late = 0`. One owner forwards each path's datagrams in order, and the floor is processed in the loop's `Control` arm after earlier data (§8 there).
  - **The toy's warning** (stage 1, carried to this registration): with the floor sent as its own datagram at H = 0, the floor overtook its copy about half the time there. The engine's premise against that is `floor-hold.md` §8's staging order: the copy is staged before the floor on the same path's FIFO lane. `floor_never_overtakes_its_copy_real_io` gates it before the pin (§3.1), and R-H fails here if the premise breaks on the real link.

    A non-zero `fl_late` with `fh_miss = 0` is a premise finding: some N = 1 reordering exists on the real I/O path. It is reported with `fl_max_us`, and is not a fail.
  - `fh_age_us` (W's bind gauge), p50 and max, is reported.
- **R-LIVE:** the frontier stall is at most 1 s on every row.
  - The bound restated from the derived H: a passed hole moves the frontier at `t_F + H`, with H = 0 here. It waits at most one tail-sweep fire to be told, plus owd ≈ 5 ms, so well under 100 ms.
  - 1 s is that, plus one to two `[DIAG]` ticks of reading resolution, plus the gap-report and D margins. The pin amendment restates it in ticks once the rehearsal measures the cadence.

**On B84 (reported, the defect):** A/src at the window end. Predicted far below C84's, because the frontier stalls as in the sim (ack 49 of 12 147); the stall is printed. **B84's arm is outside the live-row minimum:** it is predicted to stall, carries no scored clause, and its rows are never replaced for stalling or dying.

**Prediction:** every C84T and C84 witness holds; B84 stalls.

### 5.4c Stage XRK (scored): the floor-only pairing on two paths with 20 ms skew

**Why.** At N = 2 the derived H is largest (about the skew plus the jitter spread, `floor-hold.md` §7). The ack pair rule runs there, on acks reordered across the legs, and C1 stripes onto the slow leg. A floor-only receiver makes every pass a floor pass, so H's miss bar can be scored where it matters.

**Cell.** XK's link (c2 + `c2r50` dual, 5 ms + 3 ms jitter against 25 ms; a new cell row `c2k20-300`), Realtime, **300 MB**, a timed window of **45 s**, per-end ρ as XR. C84T 12, C84 6 and B84 12, interleaved. That gives 30 rows. **Power:** as XR, per leg. With two 100 Mbit/s legs the source rate is at least XR's, so the predicted floor passes are 80–390 per row, 940–4 700 pooled, against R-H2's 300.

**Required witnesses** (`X-FAIL <item>`), as XR:
- R-W3, R-W4, R-W6 with the same counted-row rule (`V_adm ≥ 300`, false-fail ≈ 0.5 %), R-ONLY, and R-BI at every `[BUDGET]` line;
- W-C1 (the slow leg's source share > 0 on at least 10 of 12 rows);
- **R-H2, the derived H at N = 2** (`floor-hold.md` §6):
  - the stage fails if the one-sided 95 % upper bound (Clopper–Pearson, pooled over the stage's C84T rows) of `fh_miss / floor-passed holes` exceeds the cell's 1 − ρ = 0.01;
  - `fh_miss_refused = 0` and `fl_trunc = 0` on every row;
  - **power:** the bound can pass only if the pooled count of floor-passed holes is large enough. With zero misses the bound is ≈ 3/n, so n ≥ 300 floor-passed holes are needed. Below 300 pooled, R-H2 reads `NEEDS-MORE-floor-passes`, and the guard is not eligible;
  - predicted: `fh_us_max` in [20 ms, 20 ms + J + q] with J ≈ 3 ms (the c2 leg's jitter) and q the standing queue difference; `fh_zero` < 1; `fl_late` > 0; `fh_miss` = 0 or a few, well inside the bar;
  - `fh_age_us`, `fh_fill_first` and `fh_miss_us_max` are reported.
- **R-LIVE2:** the frontier stall is at most 1 s on every row. The derived H (≈ 20–30 ms) adds to XR's margin and stays inside it.

**Reported:** the frontier delay p50 / p99 against XK's matched cell, the C1 shares, and `G_floor` against `budget(t)` over time. On B84: the stall (the defect).

**B84's arm is outside the live-row minimum**, as in XR.

**Prediction:** every C84T and C84 witness holds; B84 stalls.

### 5.5 Stage XA (C84 only, Auto, ρ = 0.99 against ρ = 1)

- c2-100, Auto, 100 MB, a timed window of 40 s. C84 at `--window-rho 0.99` 6 rows and C84 at `--window-rho 1` 6 rows, one binary, interleaved (rule 3), seeds 42 and 7.
- **Witnesses:**
  - at 0.99: `bud=0.0100` on every row; `V_adm + denied > 0` on at least 4 of 6 rows (Auto's longer D makes past-D holes rarer); W4 and W5 as §5.4;
  - at 1: `bud=0.0000`, `V_adm = 0` and `floor_sent = 0` (`ID-FAIL`).
- **W-AGE at XA (scored):** at 0.99, the pooled age refusals over the six rows (`vv − V_adm − denied`) are > 0. Auto's D(δ = 0.5) is the longest D in this guard, so XA is the only cell where a gap report can routinely be younger than D.
  - **0 reads `AGE-GATE-INERT`.** That is a defect finding under MEASUREMENT DISCIPLINE 17 and 18: half of the verdict's law is never seen to act over the operating range. It needs a law-ledger verdict on the verdict rule's age conjunct, recorded before the merge (it holds the merge until then). It is never an explanatory footnote, and no reading of D's effect is drawn from this guard.
  - Printed beside it: D per row, the gap-report age distribution against D where the pin amendment finds it printed, and `V_adm` and `denied`.
- **Reported:** everything of §5.4, 0.99 against 1. Auto's D(δ = 0.5) is longer than Realtime's, so fewer holes age past D: δ and ρ compose through D and the budget.
- **What its difference also contains.** The sender sites carry no bit in this build. The receiver's exception sites still step at ρ = 1: C9's expiry and force-drain, C10's drop and C11's arm. The stage reads "the step at ρ = 1 is the budget plus the receiver's old arm" and prints `G_expiry` and `G_force` at 0.99, both 0 at 1. It decides nothing. `sim_rho_dial` carries it, in the release-rule build.

### 5.5b Stage XL (scored witness, reported outcome): the lost tail after the allowance is spent, limit L-TAIL

**Why.** The §3.1 record predicts that, at a matched ρ < 1 pair, once the old arm's passes have spent the allowance, a lost tail is served until it is filled, with no time bound. A timed window on a 100 MB object rarely reaches the object's end, so this cell sends a short object whose tail falls inside the window.

**Cell.** XE's link (`c2r100l10`, single), Realtime, matched ρ < 1 (B84 no flag; C84 `--window-rho 0.99` on both ends), **20 MB** object, a timed window of 40 s (a new cell row `c2r100l10-20`). 12 + 12 rows, seeds 42 and 7, interleaved.

**Precondition, per C84 row (never an X-FAIL):** EX before the tail, i.e. `Ab + 1 > budget` on the last tick before the last source symbol is first sent. A row without it is `XL-NOT-EXHAUSTED`: recorded, not replaced, and not read for L-TAIL. If at least 6 of 12 C84 rows are `XL-NOT-EXHAUSTED`, the cell reads `XL-NOT-EXHAUSTED`. Its L-TAIL reading is then empty, which is reported, and the cell is re-run once by amendment with the loss or object size changed so that exhaustion comes before the tail.

**Witnesses on C84 (scored, `X-FAIL <item>`):** W4, W5 and BI as §5.4.

**Readouts (reported; the limit's size):**
- **tail time:** from the last source symbol's first send (the `cum=` src reaching S_src) to A = S_src, per row;
- whether A reaches S_src inside the window, and by which writer the last hole closed (fill, or `G_floor` if a verdict was still admitted);
- `denied` during the tail.

**Prediction:**
- C84 reaches A = S_src on every row by a fill. Under 10 % loss, a resend round is lost with probability ε, so the tail time is a few RTTs at p50, about 0.1–0.5 s, with a geometric tail.
- B84 strands some tails: A stays below S_src at the window end on some rows (the `rec-evict-blocker-unresolved` defect).
- A C84 row that does not reach S_src inside the window is **L-TAIL observed**. It is reported with its denied count and tail age, not as a fail: the limit is named (§3.1, §11), and it ends with the release-rule build. More than 2 of 12 such rows go to the owner as the limit's measured size, with Amendment 3's fallback clause.

### 5.6 Stage XT (scored twins: the account, both seats)

- B84T and C84T, 6 + 6 rows at each of X1 and XE, the same rows as §5.4. That gives 24 rows. `[ACCT]` and `[ACCT-RX]` go into the kept-line filter (the §64 `[SYMSZ]` lesson).
- **Twin row witness:** an `[ACCT]` line (final, else `ACCT-LAST`) on the client and an `[ACCT-RX]` line on the server. A row with neither is VOID and re-run once. Below 5 live twin rows per seat at a cell, XT there is UNSCOREABLE, and the guard is not eligible until the twins re-run.
- **Clause ACCT-ID (scored, C84T):** `O_strand = 0` on every live row, and `S_sent = A + V_adm + E + |U|` (ADR-0081 as amended; `V_adm` is `[ACCT]`'s `H=` field) on its last line. A violation is `X-FAIL acct`.
- **Clause XT-HOLD (registered, holds the merge, batch 1's R2-HOLD form):** an exact one-sided Mann–Whitney test, C84T worse, on `oldest_unchanged_max` at each cell. It fires iff p ≤ 0.05. At 6 v 6 the smallest p is 1/924. It is read and diagnosed before the merge; it is not a FAIL label.
- **Reported:** B84T's `O_strand` and `O_max` (predicted > 0 on most rows: the defect), C84T's G split, and `[BUDGET]` if it is test-seams only (then BI and EX are scored here at n = 6, which the pin amendment records).
- **The derived H on the matched cells (reported):** `floor-hold.md` §6's gauges on the twins at X1 and XE: `fh_n`, `fh_us_max`, `fh_zero`, `fl_late`, `fh_age_us`, `fh_fill_first`; `fl_trunc = 0`, `fh_miss`, `fh_miss_refused = 0`.
  - Floor passes are rare there (§5.4), so `fh_n` may be 0, which prints `-`.
  - A non-zero `fl_trunc` or `fh_miss_refused` is `X-FAIL R-H` on any stage: both must read 0 by the law's own statement.
  - The old arm's expiry hold stays `shed_recv_hold` under the exception (its row in Amendment 3's block), and is not this guard's law.

### 5.7 Stage XP (reported only; optional; cut first under the cap)

- PB and PC, c2, Realtime 0.99, paced at 35 Mbit/s (`RWM_PERF_RATE_MBIT`), 6 + 6, seeds 42 and 7, a timed window of 40 s.
- It reads the in-capacity regime, where the pause is idle and only the verdict and the floor act: the readouts of §5.4. The sim's T6 is the gate; XP only reports.
- It runs only if §1.1's cherry-pick condition holds.

## 6. Binary identity (ADR-0082)

- **This build changes the release binary deliberately:** the binary, `.text` and `.rodata` (new strings). Each commit in the range that touches a `binid.yml` path (the build touches `net/account.rs` and `test_seams.rs`) carries:

  ```text
  binid: expected-change ADR-0084 sender side: protocol v11 (WindowFloor, WindowAck.passed_undelivered); window_rho replaces window_reliable (tombstones); store and pooled rate bound at every rho; verdict rule with the (1 - rho)*S_src budget; last copy with every verdict, forced verdict floor alone; ack pair merge; receiver floor pass with the derived H (floor-hold.md); sender sites C2, C5, C12 refresh half, C3, C4, C1, C7, C6; receiver old arm keyed per the Amendment 3 exception; release gauges
  ```

  The merge commit carries the same line.
- **The line does not cover test-seam code reaching the release binary.** ABORT-SEAT checks that C84's release binary holds no `[ACCT]` or `[ACCT-RX]` string, as main's holds none.
- **After the merge**, batch 1 §5.3 applies unchanged: `binid_run.sh` builds merged main and the guarded seat C84 at the same path, and the two shas must be equal. They are equal when the merged tree equals C84 outside `docs/`, `tools/` and the roadmap; that check is recorded too. A mismatch is a hold.

## 7. Abort classes, labels, precedence and the harness rules

**Harness rules adopted by name from batch 1's fixed harness (`harness67c`).** `harness84` derives from its merged head and does not start before it merges (§9).
- **Truncation:** a truncated stage (`TRUNCATED-<stage>`) never reads PASS. A guard with a truncated scored stage is not eligible.
- **Every slot filled:** every registered slot must hold a row. Otherwise the stage reads `UNSCOREABLE (INCOMPLETE)`.
- **Candidate crashes:** a C84 or C84T crash (VOID-RC, NO_DATA) makes its cell `UNSCOREABLE (CANDIDATE-CRASH)`. It is a finding against the build, and a plain re-run does not clear it. Only a diagnosed cause, written into an amendment, re-opens the cell.
- **No default pins:** every `LD_PIN_*` (and `LD_BIN_SHA_*`) is required. A missing one is ABORT-PIN.
- **The test-seam marker:** a row whose environment carries an engine test seam that the gates use (`RWM_TEST_TS_NO_FLOOR`, `RWM_TEST_TS_BUDGET_FRAC`, or any `RWM_TEST_TS_*`), or a release seat whose binary carries the seam marker, is ABORT-SEAM. The twins are built with `test-seams`, but their rows set no seam variable.
- **The session-0 envelope:** `suites_run.sh` with ABORT-PREFLIGHT, the lock-inheritance probe (a child of the session holds both locks, and nothing else can take them), and binid's inner-test rule (`binid_run.sh` runs its own inner test before a sha is trusted).

**Abort classes**, filled in the Result in this order:
1. ABORT-LOCK;
2. ABORT-CRLF;
3. ABORT-PIN;
4. ABORT-SEAT: missing or present strings (§2, §6), or a sha mismatch;
5. ABORT-SEAM;
6. ABORT-SPELL: `perf --help`'s spelling disagrees with the seat's expected one, or a row requests a ρ that its seat cannot spell (Auto ρ < 1 on B84);
7. ABORT-BUILD;
8. ABORT-PREFLIGHT;
9. ABORT-TESTS;
10. ABORT-REPEAT;
11. ABORT-SMOKE;
12. a non-zero stage exit;
13. the cap stopper's `TRUNCATED-<stage>` (not a failure, but never a PASS; §8).

**Outcome labels.** These are never VOID and never replaced, whatever else the row carries:
- `ID-FAIL`: a ρ = 1 identity witness violated on C84;
- `X-FAIL <item>`: a violation of W1–W6, W-C1/C2/C3, PW at XE, BI, the XR and XRK witnesses (R-W3, R-W4, R-W6, R-ONLY, R-BI, R-H, R-H2, R-LIVE, R-LIVE2), XL's W4/W5/BI, or ACCT-ID;
- `FORCED-NONZERO`: a finding that holds the merge.

**Infrastructure voids** (replaced once, at most 4 slots per stage, more is NO VERDICT): VOID-COTENANT, VOID-ONDISK, CONTAMINATED.

**Crash labels** (never replaced): VOID-RC (the process exited non-zero or was killed outside the window), NO_DATA (no summary line).
- On C84 or C84T they give `UNSCOREABLE (CANDIDATE-CRASH)` (above).
- On B84 or B84T the slot stays empty, so the stage reads `UNSCOREABLE (INCOMPLETE)` until a diagnosed re-run.
- The one exception is the B84 arm of XR and XRK, which is outside the live-row minimum.

**Precondition voids** (replaced once, within the same 4-slot cap): VOID-RHO, VOID-SHORT (only on a `WINDOW-END` row, §4), VOID-WITNESS.

**Readings and stage labels** (recorded, never replaced):
- `WINDOW-END`, `COMPLETE`;
- `WARMUP-DEAD`, outside the slot cap (§4);
- `NO-VERDICT-ADMITTED` (§5.4);
- `L-TAIL observed`;
- `XL-NOT-EXHAUSTED` (a precondition reading, never an X-FAIL).

**Merge holds** (the guard is not eligible until each is read and cleared):
- `WARMUP-HOLD`;
- `XE-NOT-EXHAUSTED`;
- `AGE-GATE-INERT`, which needs a ledger verdict;
- XT-HOLD.

**Not eligible:**
- `NEEDS-MORE-verdicts` (fewer than 8 counted XR or XRK rows);
- `NEEDS-MORE-floor-passes` (under 300 pooled floor-passed holes in XRK);
- **UNSCOREABLE in any stage or cell**, for any reason (under its live-row minimum, `INCOMPLETE`, `CANDIDATE-CRASH`, or a seat lost to `WARMUP-DEAD`). A cell is never dropped from the verdict. The exception is the B84 arm of XR and XRK, as above.

**Precedence on a row** (the first that applies is the row's label):
1. a seat or sha problem (ABORT-SEAT, ABORT-PIN, ABORT-SEAM, a row sha that does not match its arm's pin): the row is not a row of its seat;
2. **an outcome label** (`ID-FAIL`, `X-FAIL`, `FORCED-NONZERO`). A row that also carries an infrastructure void keeps its outcome label and is never replaced: **a cotenant-voided `ID-FAIL` or `X-FAIL` row is still a fail**;
3. an infrastructure void (VOID-COTENANT, VOID-ONDISK, CONTAMINATED);
4. a crash label (VOID-RC, NO_DATA);
5. a precondition void (VOID-RHO, VOID-SHORT, VOID-WITNESS);
6. then the readings above, and §41d's own labels on the ρ = 1 stages.

## 8. n, sessions and VM time

### 8.1 n per arm

| stage | cell(s) | arms × n | rows |
|---|---|---|---|
| G | c1s, c1d, c2, c8 | 12 + 12 per cell | 96 |
| A | c2, c8 Auto, 600 MB | 12 + 12 per cell | 48 |
| R1 | c2-100 Realtime ρ = 1 | 12 + 12 | 24 |
| X | X1, X2, XK, XE | 12 + 12 per cell | 96 |
| XA | c2-100 Auto, C84 only | 6 (0.99) + 6 (1) | 12 |
| XR | c2-300, sender 0.99 / receiver 1 | B84 12, C84T 12, C84 6 | 30 |
| XRK | c2k20-300, sender 0.99 / receiver 1 | B84 12, C84T 12, C84 6 | 30 |
| XL | c2r100l10-20, matched 0.99 | 12 + 12 | 24 |
| XT | X1, XE | 6 + 6 per cell | 24 |
| XP (optional) | c2 paced | 6 + 6 | 12 |

### 8.2 Sessions (one at a time; host cargo paused during each)

| session | content | typical | basis |
|---|---|---|---|
| 0, pre-flight (`suites_run.sh` envelope, `LD_HARD` = launch + 4.9 h) | check ≈ 10 min; gates on B84 ≈ 10 min; suites on C84 ≈ 55 min; the §3.1 gates on both trees ≈ 30 min; repeats ≤ 60 min; visualizer ≈ 10 min; binid × 2 ≈ 35 min | **≈ 3.3 h** | batch 1 §6.4 |
| 1, rehearsal (`LD_PLAN=84 LD_REHEARSE=1`, `LD_HARD` = launch + 2 h) | 4 builds (6 with PB/PC) ≈ 30–45 min; tools tests ≈ 8 min; the stub-run check; one smoke row per (stage, seat) ≈ 16 rows ≈ 20 min | **≈ 1.3 h** | §64, batch 1 |
| 2, `LD_PLAN=84g` (`LD_HARD` = launch + 3 h) | G 96 ≈ 53 min; A 48 × ≈ 80 s ≈ 64 min; R1 24 ≈ 10 min; replacements and scoring ≈ 15 min | **≈ 2.4 h** | §64 Result; block 51 |
| 3a, `LD_PLAN=84x` (`LD_HARD` = launch + 4.2 h) | X 96 at ≈ 55 s per row (a 40 s window plus bring-up) ≈ 88 min; XR 30 and XRK 30 at ≈ 60 s (a 45 s window) ≈ 60 min; scoring ≈ 15 min; smoke re-check ≈ 10 min | **≈ 2.9 h** | block E rows ≈ 62 s |
| 3b, `LD_PLAN=84y` (`LD_HARD` = launch + 2.5 h) | XL 24, XA 12, XT 24 ≈ 55 min; XP 12 ≈ 11 min; scoring ≈ 15 min; smoke ≈ 10 min | **≈ 1.5 h** | as 3a |
| **total** | | **≈ 11.4 h** (≈ 11.2 h without XP) | |

**Worst case, about 17 h:** a rehearsal re-run (+1.3 h), a session-0 re-run after a fix (+3.3 h, only if a seat changes), and a NO VERDICT re-run of Stage G (+1 h).

**Cap rule:** sessions 3a and 3b carry the detached stopper (a balanced rep boundary or `LD_HARD`). Cuts, in order: XP, then XA, then XL to 6 + 6. X, XR, XRK and XT are not cut, because they carry the merge. Each cut is written into an amendment before the session.

### 8.3 Stop rules

- **P0:** the owner's Ceph clearance, as batch 1.
- **Block-1 rule** (§51 Amendment 6): every arm's first real row of each stage must be LIVE with its precondition witnesses. Otherwise that stage stops with `FAILED-<stage>`, and the next stage runs.
- **Any `ID-FAIL` in session 2:** session 3 still runs. Its reading goes to the diagnosis, and the guard is not eligible.
- **Bisection on a FAIL** (one guard for the whole head): the next session runs the failing stage on the pre-pinned seats CAND2 (steps 1–8 + C2 + C5) and CAND3 (+ C12's refresh half, C3, C4, C1), each against its predecessor, ≈ 1–2 h per seat. These are rho-step-sites §5's cut points.

### 8.4 The rehearsal must show

| check | NO-GO consequence |
|---|---|
| the spelling detection reads `reliable` on B84 and `rho` on C84, written to the provenance | ABORT-SPELL: fix `harness84` and re-rehearse |
| every owed string on C84 and none on B84; no `[ACCT]` in either release binary; both strings on the twins | ABORT-SEAT |
| one G row per seat (c1s), one A row per seat (c8) with rr > 0 and the ρ echo, one R1 row per seat, completed and LIVE | a harness fault: fix and re-rehearse |
| **the power check:** one XR and one XRK row per seat (B84, C84T, C84) with the per-end ρ echo, R-W4/R-W6 and the fh gauges computed on C84T, and `[BUDGET]` lines parsed in sequence. On each C84T row, `V_adm ≥ 300` (R-W6's counted-row threshold), and the row's floor passes × 12 ≥ 300 (XRK's pooled bar). **XR's C84T row must print `fl_late = 0`**, because R-H's "any miss fails" rests on it | a shortfall in either power number is a NO-GO: the object, window or n is resized by amendment and re-rehearsed. A non-zero `fl_late` on XR is a NO-GO: N = 1 reordering exists on the real link, and R-H's clause is reconsidered before any launch |
|  one XL row per seat with the tail time computed | as above |
| one X1 and one XE row per seat: the ρ echo, `shed=` present, 30 s of ticks, `WINDOW-END` labelled, W1–W6 on C84 computed (not None) | a missing line is a harness fault. A failed W item is a finding against the build: no launch |
| one XA pair on C84 with both echoes | as above |
| one XT row per twin at X1, with `[ACCT]` and `[ACCT-RX]` in the kept log, not only in the raw log | as above |
| the frontier series, stall and half-split resolution computed on the X rows; the `[DIAG]` cadence recorded | as above |
| tools tests and the stub runs (§9) green; `TOOLS-TESTS-OK` and the manifest written | ABORT-TESTS |

A rehearsal row is not re-run, and a failed outcome witness in the rehearsal is a finding.

## 9. Harness change (`harness84`, tools only, a Sonnet worker; it goes into both seats before the pin)

**It derives from the merged `harness67c` head** (batch 1's fixed harness: `plan67.py`, `s67_score.py` and the rules adopted in §7). It does not start before `harness67c` merges. Plan tokens: `84`, `84g`, `84x`, `84y`.
1. **Spelling detection.** `perf --help | grep -q -- --window-rho` sets the row's spelling, mapped per §1.2:
   - `RWM_C_RHO=<f>` replaces `RWM_C_RELIABLE` for these plans; the old knob stays for older plans;
   - a `[SPELL] sha=<…> spelling=<rho|reliable> rho=<f> argv=<…>` provenance line goes in every row;
   - Auto ρ < 1 on a `reliable` binary is refused (ABORT-SPELL), so it never runs as ρ = 1 silently;
   - **per-end ρ** (`RWM_C_RHO_SRV=<f>`, default `RWM_C_RHO`) for Stage XR: `perf_rwm_c.sh` today passes one `$WR_FLAG` to both ends, so the server gets its own mapped flag.

   Every `tools/l1` script that passes `--window-reliable` and is reached by these plans goes through the one mapping (Amendment 1 item 8: about 30 scripts; the others stay as they are and are listed).
2. **The ρ echo parser:** `window_reliable: <bool>` (B84) and `window_rho: <f>` (C84) from `perf configuration` on both ends, giving VOID-RHO.
3. **Cell rows:** `c2-300 → "c2 c2 single 300000000"`, `c2k20-300 → "c2 c2r50 dual 300000000"`, `c2k20-100 → "c2 c2r50 dual 100000000"`, `c2r100l10-100 → "c2r100l10 c2r100l10 single 100000000"` and `c2r100l10-20 → "c2r100l10 c2r100l10 single 20000000"` in `loopdiag_battery.sh`'s `cell_spec`, with a test that each resolves to its scenarios' `netem_args`.
4. **Timed-window rows:** `RWM_PERF_TIMEOUT_S=40` for the ρ < 1 stages; `WINDOW-END`, `COMPLETE` and `WARMUP-DEAD` labels instead of DNF; VOID-SHORT only on a `WINDOW-END` row under 30 s of ticks.
5. **Parsers:**
   - the owed tokens (§2), with names from the pin amendment;
   - the frontier series, p50 / p99, stall and half-split resolution (§4);
   - the plan 41d parser made tolerant of `shed=` at Bulk;
   - the twins: plan 67's `_acct_of`, final else last, plus `[ACCT-RX]` and the kept-line filter;
   - `[BUDGET]`;
   - the slow leg's source share (W-C1), `supp … l` (W-C3), `wnd2=` and `released` (W-C2), `[QCLK] site=receiver` (W-C12);
   - XL's tail time (the src curve reaching S_src to A = S_src);
   - `floor-hold.md` §6's gauges from `[ACCT-RX]`, and the pooled Clopper–Pearson bound for R-H2;
   - every `[BUDGET]` line in sequence for R-BI, not only the last.
6. **Scorers:**
   - `score41d` reused for G;
   - plan 67's `scoreAS` for AS84 (6 clauses, 12 v 12);
   - plan 64's completion rule for R1-84;
   - **new `score84x`:** W1–W6, W-C1/C2/C3/C12, BI, PW, EX, the XR witnesses with R-W6's counted-row rule, XL and the labels of §7;
   - XT-HOLD as plan 67's R2-HOLD;
   - the readout tables of §5.4, with the two-sided rank p and the resolution.
7. **Stub paths, red first where new, plus `STUBRUN_FULL=1` per token:**
   - `spell-mismatch` → ABORT-SPELL;
   - `auto-rho-base` → ABORT-SPELL;
   - `rho-echo-missing` → VOID-RHO;
   - `bulk-shed-field` (plan 41d's parse unchanged);
   - `cand-shed-nonzero-rho1` → `ID-FAIL`, not replaced;
   - `cand-floor-at-rho1` → `ID-FAIL`;
   - `x-no-verdict` → `X-FAIL W3`;
   - `x-lastcopy-mismatch` → `X-FAIL W4`;
   - `x-forced` → `FORCED-NONZERO`;
   - `x-floor-not-received` → `X-FAIL W6`;
   - `x-gfloor-over-budget` → `X-FAIL BI`;
   - `x-grx-over-budget-expiry` (a breach from `G_expiry`) → reported, not a failure;
   - `xe-not-exhausted` → `XE-NOT-EXHAUSTED`;
   - `pw-zero-at-xe` → `X-FAIL PW` (never `INERT-AT-CELL`);
   - `window-end` → WINDOW-END, not DNF;
   - `x-short` → VOID-SHORT;
   - `x-complete-short` → `COMPLETE`, not VOID-SHORT;
   - `warmup-dead` → `WARMUP-DEAD`, not replaced, outside the slot cap;
   - `warmup-dead-5` → that seat's cell is UNSCOREABLE, and the guard is not eligible;
   - `age-gate-inert` (XA with `vv − V_adm − denied = 0` pooled) → `AGE-GATE-INERT`, holds the merge;
   - `xe-not-exhausted-hold` → `XE-NOT-EXHAUSTED`, holds the merge until read;
   - `xl-row-not-exhausted` → `XL-NOT-EXHAUSTED`, never `X-FAIL`;
   - `xr-w6-low-vadm` (a row with `V_adm` < 300 and no floor pass) → not counted, no failure;
   - `cotenant-on-xfail` (an `X-FAIL` row also cotenant-flagged) → stays `X-FAIL`, not replaced;
   - `cand-crash` → `UNSCOREABLE (CANDIDATE-CRASH)`, not cleared by a re-run;
   - `slot-empty` → `UNSCOREABLE (INCOMPLETE)`;
   - `truncated-stage` → never PASS, not eligible;
   - `pin-missing` → ABORT-PIN;
   - `seam-env-in-row` (`RWM_TEST_TS_*` in a row's environment) → ABORT-SEAM;
   - `x-quiet-row` (one row with `l = 0`, the rest > 0) → no failure;
   - `twin-no-final` → `ACCT-LAST`;
   - `twin-under-min` → UNSCOREABLE, not eligible;
   - `twin-strand` → `X-FAIL acct`;
   - `twin-worse` → XT-HOLD fires;
   - `a-under-min` → UNSCOREABLE;
   - `prebuilt-sha-mismatch` → ABORT-BUILD;
   - `acct-in-release` → ABORT-SEAT;
   - `xr-no-floor-pass` → `X-FAIL R-W6`;
   - `xr-expiry-pass` (`G_expiry > 0` at a ρ = 1 receiver) → `X-FAIL R-ONLY`;
   - `xr-few-verdicts` → `NEEDS-MORE-verdicts`;
   - `xr-server-same-flag` (both ends got one flag) → VOID-RHO;
   - `xl-tail-stuck` → `L-TAIL observed`, not a fail;
   - `fh-miss-n1` → `X-FAIL R-H`;
   - `fh-miss-n2-over-bar` → `X-FAIL R-H2`;
   - `fh-few-passes` → `NEEDS-MORE-floor-passes`;
   - `fl-trunc` → `X-FAIL R-H`;
   - `budget-midrow-breach` (one mid-row `[BUDGET]` line over budget, the last under) → `X-FAIL R-BI`;
   - `warmup-more-on-cand` → `WARMUP-HOLD`;
   - `matched-no-verdict` → `NO-VERDICT-ADMITTED`, not a fail;
   - `x-cell-unscoreable` → not eligible;
   - `x-no-mp-supp` → `X-FAIL W-C3`;
   - `x-no-slow-share` → `X-FAIL W-C1`.

## 10. What this guard cannot see

- **The lost tail under `dead`, and the stalled blocker, at L1.** The release binary has no `dead` seam. T1, T1′, T2 and the ablation carry them (§3.1). XL reads only a natural tail.
- **A floor pass, a last copy or H in the matched cells.** There the old arm spends the allowance first, so few or no verdicts are admitted (§5.4's table). These are required only in XR and XRK.
- **C10's full-channel drops.** They are neither told nor counted (Amendment 3 item 4). Perf's consumer is fast, so they are expected to be 0, but no row can show it.
- **The pair merge's effect.** No gauge counts acks the rule set aside. `ack_pair_merge_survives_reordered_acks` carries it; X2 and XK only exercise reordered acks.
- **The forced verdict path.** It is 0 by data with a store at every ρ. W5 only shows that it stays 0; `forced_verdict_sends_floor_alone` carries the path.
- **The reverse pairing** (ρ = 1 sender, ρ < 1 receiver). It stays a defect in this build, with its sim test red (`rec-mixed-rho-stall`), and no row runs it.
- **The release-rule build's costs:** design U, the new H, floor-on-raise and R1-like serialization. None is in this build.
- **H's W.** W = 10 s is borrowed, not derived, and awaits the owner. `fh_age_us` shows whether W binds; the W sweep is the toy simulation's (`skip-rule-efficiency.md` §6.4).
- **H at N = 1 on a link that reorders.** Every cell sets `rate`, so netem never reorders; R-H's `fh_miss = 0` holds on FIFO paths only.
- **C4, C6 and C7.** C4's gate is off by default, C6's arms are off, and C7's feed is opt-in. They are inert at every row; their sim tests carry them.
- **The pause's law shape.** B is pinned (F01). Where cwnd binds first, PW reads `INERT-AT-CELL`.
- **The in-capacity regime**, unless XP runs. The scored X cells are unpaced.
- **Auto ρ < 1 on BASE:** `perf` has no spelling for it, so XA is within C84 only.
- **The receiver exception's step at ρ = 1, site by site.** XA reads only its sum.
- **Anything shorter than one `[DIAG]` tick** in the frontier readouts.
- **Reordering within one leg.** Every cell sets `rate`, so netem is FIFO per leg (rule 19). At N = 2, arrivals do interleave across the legs.
- **A blackout.** `event_inject.py` steps delay only. If XE does not exhaust, the exhaust reading is `NEEDS-MORE-blackout`: a loss-step verb in `topo_dual.sh`, with its own stub and tools test.
- **The tunnel path, v10 refusal, the tombstones and C8's refusal.** They are suite tests (session 0), not rows.
- **Resident memory** of the store at ρ < 1, up to B. It is not read.

## 11. Open questions (for the coordinator and the owner)

1. **Answered:** Q-a accepted with its fallback; Q-b (iii), H derived in `floor-hold.md`; C1 stays in this build (option (a)), and the interim XK delay rise is accepted and reported. **Still open: W = 10 s, with the max** (`floor-hold.md`), is a default awaiting owner confirmation before the floor-pass step is built. If the owner picks another W, the pin amendment restates R-H's and R-H2's predictions; the fail levels do not depend on W.
2. **One guard or three** (coordinator, then owner): **default, awaiting confirmation: one guard on the head, with the pre-pinned bisection seats (§8.3).** This registration guards the head against main, as the brief's §5 says. rho-step-sites §5 lists three cut points; a FAIL is bisected on them (§8.3).
3. **L-TAIL** (owner): **a default awaiting confirmation, as an accepted interim.** At a matched ρ < 1 pair, once the old arm has spent the allowance, a lost tail waits for its fill with no time bound. It is named and measured here (§3.1 record, XL), and it ends with the release-rule build.
4. **T1 against L-TAIL: closed (coordinator decision, 2026-11-22).** T1 stays a gate in its current form and asserts that the tail's verdict is admitted (`Ab + 1 ≤ budget` at that verdict), in the brief's T1 done criterion and §3.1. The pre-spent-allowance variant stays the §3.1 record (L-TAIL) and becomes a gate in the release-rule build.
5. **Owed release gauges (§2, P3).** If the build prints them only under test-seams, the twins grow to 12 + 12 per X cell and XR (+≈ 50 min in session 3, which then splits into two sessions).
6. **The pacer seats (XP):** whether `4ee1df9b` applies to both trees.
7. **Fable's review of this draft**, before the pin.

## 12. The decision rule

- **ELIGIBLE TO MERGE** iff all of these hold:
  - the §3.1 gates are green on C84, with their red-first records on B84;
  - session 0's suites and repeats are green;
  - G84 PASS, AS84 PASS and R1-84 PASS, with zero `ID-FAIL`;
  - Stage X PASS: no `X-FAIL`, and EX-REACHED, or `XE-NOT-EXHAUSTED` read and cleared (§5.4: the matched-cell readings stand or are re-scoped first);
  - W-AGE at XA is > 0, or `AGE-GATE-INERT` carries its ledger verdict;
  - no scored stage is `TRUNCATED`, every registered slot holds a row, and no cell is `UNSCOREABLE (CANDIDATE-CRASH)`;
  - Stages XR and XRK: every witness holds, including R-H / R-H2 and R-BI at every `[BUDGET]` line, with at least 8 counted rows each and at least 300 pooled floor passes in XRK;
  - no cell of any stage is UNSCOREABLE (except the B84 arm of XR and XRK), and no `WARMUP-HOLD` is unread;
  - Stage XL: its witnesses hold. `L-TAIL observed` and `XL-NOT-EXHAUSTED` rows are reported, not counted against the merge;
  - PW at XE passes (it has no `INERT-AT-CELL` exit);
  - ACCT-ID passes, XT-HOLD did not fire (or fired and was read with a verdict that clears it), and no `FORCED-NONZERO` is unread;
  - the deny test's ratchet equals Amendment 3's `rho1-debt-exception` block (CI; Q-a accepted), and the floor pass uses the derived H (Q-b (iii)) with W as the owner confirmed;
  - §6's binid check holds after the merge.
- **Then** the build merges `--no-ff` with the `binid: expected-change` line. ADR-0084's Status names the merge commit. The roadmap node `rec-adr84-guard` and `rec-evict-blocker-unresolved` move in the same push.
- **Any `ID-FAIL`:** no merge. It is a finding that ρ = 1 is not identical, diagnosed first.
- **Any `X-FAIL`:** no merge. It is a finding against the build.
- **G84 or AS84 FAIL:** no merge. The failing clause is diagnosed.
- **The XE stall finding, the XK delay readout, XL's tail times, the matched cells' H gauges and every reported readout decide nothing.** They go to the owner with the merge.
