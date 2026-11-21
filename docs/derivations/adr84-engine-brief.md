# ADR-0084 engine change: implementation brief

This brief is for the worker who builds the engine change. **The spec is ADR-0084 plus its Amendments 1 and 2, plus the paper sections they cite** (`docs/fec-arq-model.md` §5.1, §5.6, §6.1, §6.3, §11.1, §11.3, A.4, A.5). Where the ADR is silent, `evict-verdict-law.md` §1–§7 fills in, subject to its erratum. If this brief disagrees with any of them, they win: report the disagreement, don't resolve it.

Owner decisions of 2026-11-20 are recorded in Amendment 1:
- S06 merges first, on its own guard; this work branches from main after it;
- **option B**: all twelve (C) sites are rewritten, and none is grandfathered.

Owner decisions of 2026-11-21 are recorded in Amendment 2 and in `rho-step-sites.md` §8:
- every verdict sends one last copy of the seq with the floor; a forced verdict sends the floor alone (steps 6–8);
- the sender takes `(next_expected, G_rx)` as a pair from the ack with the greatest `next_expected`, the larger `G_rx` on a tie (step 4);
- C6 is option (a); the Realtime N ≥ 2 changes (C1, C3, gap-report coalescing at every N) are accepted.
- Still open: the receiver law R1 as final, the floor-cadence reversal, the C10 hold at Realtime. R1 is the staging point only. The receiver's release arm (C9, C11, C12, design U) lands once, with the release rule, after the disorder price ω is measured.

**What is still open.** The form of each (C) site, and whether the receiver reads ρ at all, come from `docs/derivations/rho-step-sites.md` (branch `rho-step-sites`). That derivation is still running.
- Steps 0–8 below do not depend on it.
- Step 9 (the (C) rewrites) does not start until it is accepted.
- The merge condition is zero `rho1_debt` readers, with the scaffolding function and every `rho1_debt` field deleted.

**Working rules.** The coordinator's worker rules apply:
- your own worktree and branch;
- host cargo under the mutex, one targeted test binary at a time, with your own target dir;
- push only to the named branch, never to main, no force-push;
- no attribution lines.

**One commit per step.** Every commit leaves CI green. A test that asserts the defect is flipped in the commit that fixes the defect, not later. Line numbers below are on main `060db8af`. On main after S06, `net/emit_source.rs` and `net/sender_phases.rs` shift, so find each site by its expression.

## 1. Inventory: every reader of the flag

This section counts readers, not mentions. A reader is a line whose value depends on the flag; a derived bool counts at the line that derives it. Plumbing (fields, parameters, struct-literal passes) and comments are listed after the table and not counted. 48 reader lines in total.

**(A) Removed by ADR-0084's engine change.** 12 sites, 16 lines. At ρ = 1 each is identical by data.

| # | site | expression | after the change |
|---|---|---|---|
| A1 | `net/emit_source.rs:347` store fill | `if pol.reliable && !pol.generation` | `if !pol.generation` |
| A2 | `net/sender_phases.rs:985` serve | `if reliable { store } else { window or repair }` | the store at every ρ; a miss means acked or verdicted, so skip |
| A3 | `net/sender_phases.rs:796` clamp, through `gap_clamp!` `:1460–1461`; `net/account.rs:35` `clamp_lo`, read at `:123` and `:179` | clamp to the window when `!reliable` | bounds `(0, u64::MAX)` at every ρ (derivation §3.6: no step-4 store-span bound). The macro and `clamp_lo` go, and the account's floor is 0 |
| A4 | `net/mod.rs:2473` intake pause | `reliable && (store_len >= cap \|\| cwnd_full)` | `O ≥ cap ∨ cwnd_full` with `O = (H + 1 − F) − \|released\|` at every ρ (A.5) |
| A5 | `net/mod.rs:3219` the burst loop's copy of the pause | `if reliable && sack_release_outstanding(..) >= cap` | the same O test as A4, through one helper, so the two copies cannot drift |
| A6 | `net/sender_policy.rs:522` pooled cap scope | `plain_dyn_cap = reliable && !generation && !coded_only && !store_env_set` | drop `reliable` (decision 3: B at every ρ). `three_term_on` and `percap_track` follow |
| A7 | `net/sender_phases.rs:1400` path-map floor | `if reliable { ack + 1 } else { win_start }` | `ack + 1`: attribution is kept while the store holds the seq |
| A8 | `net/mod.rs:3599` eviction prune | `if !reliable { source_path_map.prune_below(win_start) }` | deleted (follows A7) |
| A9–A12 | `net/mod.rs:632`, `:646`, `:647`, `:1207` | `window_reliable && corner` | `corner`: redundant, because `config.rs` refuses every corner below ρ = 1 (C8). Zero-behaviour |

**(B) Zero by formula.** 1 site, 2 lines.

| # | site | expression | formula after the change |
|---|---|---|---|
| B1 | `net/shed.rs:24`, called at `net/sender_policy.rs:667` | `shed_armed = unified && !reliable && gate` | `shed_armed = unified && gate`, at every ρ |

The budget becomes:

```text
   shed admissible  ⇔  D > 0  ∧  age > D  ∧  Ab + 1 ≤ (1 − ρ)·S_src
```

At ρ = 1 the last conjunct reads `Ab + 1 ≤ 0`, which is always false, so the site sheds nothing by formula.

**The ρ = 1 test** `shed_arm_at_rho_1_sheds_nothing`. Use the blackout scenario of `sim_shed_reach` at ρ = 1, so holes older than D are guaranteed. A plain lossy run may never age a hole past D, and then zero sheds proves nothing. Assert:
- `shed_visit_nack > 0`: the check is visited. The site is reached at ρ = 1 because `retransmit_buffer` fills for every non-generation send (`emit_source.rs:499–506`).
- `D > 0` at the visit: `shed_deadline_us_live` is refreshed under `if pol.shed_on` inside the span block (`emit_source.rs:630`). With the arm on at ρ = 1 it refreshes there too.
- `shed_denied > 0`: a past-D hole reached the budget conjunct and was refused. This is the witness that the formula, not the arming, kept H at 0. `shed_denied` is a gauge only, now non-zero at Bulk, so any L1 parser that reads `[DIAG] shed` at Bulk sees it move.
- `H = 0` and no `ev=shed` line.

The receiver calls `shed_armed(.., false, ..)` at `net/receiver.rs:186`; after the change that call simply loses the argument.

**(C) No ρ-continuous form yet.** 12 sites, 24 lines. All are rewritten under option B in step 9. Until then they read the mid-stack scaffolding bit `rho1_debt` (Amendment 1, items 2 and 4); C8 reads ρ at config time instead.

| # | site and lines | test (`raptorpath/tests/rho1_debt_bounds.rs`) | ledger |
|---|---|---|---|
| C1 | placement split: `emit_source.rs:356`, `:863`; `sender_phases.rs:973`, `:1118` | `c01_placement_split_steps_only_at_rho_1` | G01 |
| C2 | SACK release: `sender_policy.rs:628`; `mod.rs:1103` | `c02_sack_release_steps_only_at_rho_1` | G02 |
| C3 | `recov_mp`: `sender_policy.rs:692` | `c03_recov_mp_steps_only_at_rho_1` | G03 |
| C4 | `recov_sp`: `sender_policy.rs:705` | `c04_recov_sp_steps_only_at_rho_1` | G04 |
| C5 | `emit_batch_on`: `sender_policy.rs:737` | `c05_emit_batch_scope_steps_only_at_rho_1` | S09 |
| C6 | request arms: `mod.rs:1049`, `:1058` (through `sack.rs:36`); `mod.rs:1752` | `c06_request_arms_step_only_at_rho_1` | G06 |
| C7 | Copa plain feed `plain_inorder`: `mod.rs:938` | `c07_copa_plain_feed_steps_only_at_rho_1` | G07 |
| C8 | corner refusal: `config.rs:304` | `c08_corner_refusal_is_config_only` | G08 |
| C9 | reorder buffer: `receiver.rs:163`; `reorder.rs:88`, `:118`, `:182` | `c09_reorder_buffer_steps_only_at_rho_1` | G09 |
| C10 | delivery on a full channel: `receiver.rs:172`; `delivery.rs:66` | `c10_delivery_drop_steps_only_at_rho_1` | G10 |
| C11 | `recv_shed_on`: `receiver.rs:183` | `c11_receiver_shed_hold_steps_only_at_rho_1` | G11 |
| C12 | hole timer: `receiver.rs:734`, `:860` | `c12_hole_timer_steps_only_at_rho_1` | G12 |

What each site would need to become continuous is in Amendment 1, item 4, and in the paper §11.3 rows C1–C12.

**(D) Log, diagnostic and test-seam readers.** 4 sites, 6 lines. They read `rho1_debt` and are counted with (C) in the deny test.

| # | site | note |
|---|---|---|
| D1 | `net/mod.rs:616–619` | the echo "reliable window mode (retain until acked): auto-selecting RLC windowed backend", matched byte-for-byte by `stage3_parse.py` WIN_LINE. Keep the string; print it when `rho1_debt` |
| D2 | `net/mod.rs:2007` | `reliable =` field of the emission-batching scope log; rename it `rho =` and print ρ |
| D3 | `net/mod.rs:2040` → `net/account.rs:199` | `Account::new(reliable)` and the `[ACCT] reliable=` field. After A3 the account needs no seat: print `rho=` with ρ, and update any parser that reads `reliable=` |
| D4 | `net/mod.rs:2478`, `net/diag.rs:808` | the wnd2/relgap DIAG gauges gated on `reliable && !generation`. They measure the SACK-release frontier, so gate them on `pol.store_sack_release_on` (C2's derived name), not the bit. D4 then reads no `rho1_debt` directly |

D2, D3 and D4 move off the bit: D2 and D3 print ρ, and D4 reads C2's derived name. Only D1 stays in the ratchet.

**Plumbing.** These carry the value but are not readers. Each disappears or is renamed to `rho1_debt` or `window_rho`:
- `config.rs:61`, `:196`, `:293–294`, `:346`;
- `main.rs:114`, `:152`, `:258`, `:295`, `:386`;
- `net/mod.rs:123`, `:627`, `:1124`, `:1168`, `:1195`, `:1268`, `:1533`, `:1594`, `:2529`, `:3530`, `:3563`;
- `net/sender_phases.rs:587`, `:617`, `:1191`, `:1231`;
- `net/sender_policy.rs:138`, `:308`, `:789`;
- `net/receiver.rs:86`;
- `net/account.rs:43`, `:104–105`;
- `net/reorder.rs:27`, `:37`, `:49` (`new_reliable` becomes `new_rho1_debt`);
- `net/delivery.rs:30`, `:38`, `:40` (`hold_on_full` becomes `rho1_debt`);
- `net/diag.rs:406`;
- `net/shed.rs:23`;
- `net/sack.rs:35`.

**Tests** that read the flag:
- in `src`: `config.rs:572`, `:628–642`, the `reorder.rs` and `delivery.rs` unit tests, and `net/tests.rs` (20 mentions);
- 32 files under `raptorpath/tests/` (through `Scenario.reliable`, `in_process::cfgs(.., window_reliable)`, and `--window-reliable` in `tests/common/loopback.rs:427`, `:730`);
- 30 files under `tools/` (`tools/l1/*` and `tools/ci/seat_groups.json`).

## 2. The base: main, after S06 merges (step 0)

S06 merges on its own guard-2 PASS in VM batch 1 (`docs/derivations/vm-batch-1-registration.md`; Amendment 1 item 3). The S06 merge-up is that batch's job, not yours: the conflict resolutions, the inverted `witness_slot_site_sheds_paced`, and the removal of the stale `shed-slot` entries from `tools/ci/coverage_sites.json`.

**Branch from `github/main` after that merge.** If S06 has not merged, stop and report. Do not stack on an unmerged `s06-delete`.

On that main, check the following before step 1:
- `net/emit_source.rs` has no slot-site shed. Either `Account::shed_visit_slot` reads 0 at every run, or the batch deleted it; either way, record which.
- `sim_shed_reach` has the inverted slot witness and no `witness_slot_site_sheds_paced`.
- `coverage_sites.json` has no `shed-slot*` entries.

If any of these is missing, report it; do not fix S06's merge-up yourself.

Your guard has BASE = that main and CAND = your head, so a failure belongs to ADR-0084 alone.

## 3. What does not change

- Hints stay named points. No step adds an `if hint ==`.
- The receiver's give-up budget stays ε̂_recv (C9, C11).
- D keeps the EWMA RTT. Compute and log `D_rtprop / D_ewma` (decision 7).
- The budget stays cumulative.
- The cwnd brake stays unarmed.
- SACK release stays on the ρ = 1 arm (C2).

## 4. Steps, in order, one commit each

The derivation's tests (§6) keep its names. The scenario parameters (seed 42, 6 MB, 2.5 % loss, `RWM_TEST_TS_DROP=dead`, 1.5 s) are those of `tests/sim_lost_tail.rs` and `tests/sim_shed_reach.rs`. Run the sim tests with `RUSTFLAGS="--cfg tokio_unstable" --features sim,test-seams`.

**Step 1: red-first defect tests.** Use the flag spelling still on the tree: `reliable: Some(false)`. Each test gets `#[ignore = "rec-evict-blocker-unresolved: <why it fails today>"]`, so `tools/ci/expected_failure.py` picks it up and requires it to FAIL. Show each one failing on the base with `python tools/ci/expected_failure.py --list` and the cargo output.
- **New helper `assert_frontier_live(r)`** in `tests/sim_support`, in the order of derivation §6. It reads the frontier from the sender's received `WindowAck.next_expected` through a new test-seam hook (`ev=ack next=`), never from `ev=frontier`. It checks:
  1. the ack equals S_src within the bound;
  2. every verdict seq is passed within its bound;
  3. the account closes, with `|U| = 0`;
  4. `G_rx ≤ Ab`, once that exists.

  It asserts the frontier first. It asserts completion only in ρ = 1 controls.
- **T1** `lost_tail_ack_reaches_s_src_at_rho_below_1` (`sim_lost_tail.rs`, k = 11 800, `limit_s: 40`). The ack reaches S_src by `t_dead + (n_tail + 1 + n_drop)·T_sweep_max + owd_max + H_max`, with `n_tail = 346`. k is passed by `t_dead + 2·T_sweep_max + owd_max + H_max`. Red on the base: the ack stays at 11 800.
- **T1′ is not here.** It is an ablation, not a defect on main. On the base the derived budget reads 0 in the sim (`sim_shed_reach.rs:9–14`), so k is resent and filled. It is red there only under a forced budget, which would make the expected-failure job lock in a seam artefact. T1′ is written in step 8.
- **`sim_mixed_rho::mixed_realtime_sender_bulk_receiver_ack_reaches_src`** is already on the branch: Amendment 1 item 5, the mixed-config stall on main. Keep it ignored until step 8.
- **T2** `shed_blocker_ack_reaches_s_src` (`sim_shed_reach.rs`, forced budget 0.05). The ack reaches 12 147, and every shed seq is below the final ack by `t_s + (1 + n_drop)·T_sweep_max + owd_max + H_max`. Red on the base: the ack stays at 12 087.
- **T7a** `intake_bounded_at_rho_below_1` (`sim_lost_tail.rs`). First the pause executes: the paused-iteration share on the `O ≥ B` disjunct is greater than 0, read first. Then `O_hwm ≤ cap_at_hwm + 1` from `[ACCT]`. Red on the base: `O_hwm > 4·cap_at_hwm`. The existing defect test already reads this.
- **T3** (not ignored) `rho1_controls`: the same scenarios at ρ = 1 on the plain, generation and coded-only arms. Green on the base and on every later commit. Asserts `client_ok`, `V = 0` and, once those counters exist, `floor_sent = 0` and `G_rx = 0`.

**Step 2: `window_rho` replaces the flag.** Zero behaviour at every mapped config.
- **Config:** `RaptorpathConfig.window_rho: Option<f64>`. `config::check_window_rho` enforces the domain `0 < ρ ≤ 1` and refuses the object corners below ρ = 1 (C8). Add one test per refused value, each failing with a message that names `window_rho`: NaN, +∞, −∞, 0, −0.5 and 1.0000001. NaN fails every comparison, so write the check as `!(rho > 0.0 && rho <= 1.0)`, which also refuses NaN. The precedent is the `RWM_DELTA` NaN defect F1 (`9b8b42ca`), where a NaN slid through a clamp and pinned the cap. `resolve` gives Bulk and Auto 1.0, Realtime 0.99, and an explicit value wins. `check_window_rho` is also called at the top of `net::run_impl`: `PeerConfig` is public, and `perf::server` and `perf::client` take one. Delete the redundant conjuncts A9–A12 in the same commit. Add a test that a hand-built `PeerConfig` with a corner at ρ = 0.99 is refused at start.
- **The wire to the engine:** `PeerConfig.window_rho: f64`. `SenderPolicy.contract_rho` takes the declared ρ instead of the literal `1.0` (`sender_policy.rs:616`). It is read by the opt-in three-term law only. That law is off unless `RWM_THREE_TERM` is set and is scoped to `plain_dyn_cap`, which is ρ = 1 until step 5, so nothing changes here.
- **CLI:** `--window-rho <f64>` on `run` and `perf`.
- **Tombstones**, so old spellings fail loudly. The TOML key `window_reliable: Option<bool>` and a hidden `--window-reliable` flag both error at startup with the message in Amendment 1 item 6, following the `mp_block_affinity` pattern (`config.rs:77–80`, `main.rs:105–107`). Add tests that a TOML with `window_reliable = true`, and a CLI with `--window-reliable`, both fail with "window_rho" in the message.
- **The bit:** `raptorpath/src/net/rho_debt.rs` holds `pub(crate) fn rho1_debt_bit(rho: f64) -> bool { rho >= 1.0 }`, documented as the NO-MODE-SWITCH trap kept as counted debt (Amendment 1). It is called exactly twice:
  - the sender's own ρ in `run_impl`;
  - the receiver's own configured ρ, for its (C) sites C9–C12. There is no handshake field (Amendment 1 item 5).
- **Renames:**
  - every reader in §1 reads `rho1_debt`, and the plumbing is renamed;
  - the bare `rho` parameters of `net/store_cap.rs` (`contract_stall_s`, `three_term_store_cap`) become `contract_rho`;
  - `net/late.rs`'s local `rho` (ρ̂_heal) becomes `rho_heal`.
- **The harness mapping:** `tests/sim_support` `Scenario.reliable: Option<bool>` becomes `rho: Option<f64>`, with `Some(true)` → 1.0 and `Some(false)` → 0.99. `tests/common/loopback.rs` gets `cfgs(.., rho: f64)` and `--window-rho`. Make the same change in every test file in §1.
- **Zero-behaviour proof:**
  - `decision_trace_pin` unchanged;
  - T3 green;
  - every existing sim fingerprint unchanged.

**Step 3: the deny test** `raptorpath/tests/rho_deny_list.rs`. Copy the structure of `raptorpath-core/tests/deny_list.rs`, including the walk-visited assertion and the planted fixture under `CARGO_TARGET_TMPDIR` at `:144`. Strip comments and string literals, skip test files (`**/tests.rs`, `#[cfg(test)]` modules), and walk the non-test code of `raptorpath/src`. It fails on:
- **(a) Comparing a retention-ρ identifier** (`rho`, `window_rho`, `contract_rho`, `recv_rho`, `peer_rho`). That means a comparison operator on either side, a `match`, `.partial_cmp(` or `.total_cmp(` on one. It also fails when a ρ value is bound to a name outside that list: `contract_rho: cfg.window_rho` passes and `let r = window_rho;` fails. The allowlist is two function bodies, `rho1_debt_bit` and `check_window_rho`, each defined exactly once (pin the definition count).
- **(b) The ratchet.** Attribute every identifier containing `rho1_debt` to its `(file, enclosing fn)`; a struct field counts under its struct.
  - The set of functions must equal the `rho1-debt-readers` block in Amendment 1, which the test parses from `docs/adr/0084-every-verdict-reaches-the-receiver.md`. A reader in any other function fails.
  - The per-function counts must equal a const table in the test. A rise fails, and so does a fall the table does not record.
  - The table starts at step 2's counts, including the (A) and (B) readers still present. Every later step that removes a reader lowers its entry in the same commit, and at the end the table matches the ADR block exactly.
  - The test also fails if the table lists a function the ADR block does not. So the list can only shrink, and growing it needs a new ADR.
- **(c) An old identifier.** These must not occur: `reliable`, `window_reliable`, `recv_window_reliable`, `sender_window_reliable`, `hold_on_full`, `new_reliable`. The exception is the two tombstones, `config.rs` (field plus check) and `main.rs` (hidden arg plus check), with their counts pinned.

Show the test red on the fixture: each planted form is flagged, including `let reliable = contract_rho >= 1.0;`, an aliased `let r = window_rho; if r < 1.0 {}`, and `fn f(rho: f64) -> bool { rho == 1.0 }`. Show it green on the tree.

It runs in the normal CI test job (`cargo test -p raptorpath --test rho_deny_list`), not in `build_visualizer.sh`, which would compile the engine in the wasm job. Do not edit `CLAUDE.md`; the coordinator handles that line.

**Step 4: protocol v11.**
- **Wire changes**, one `PROTOCOL_VERSION` bump from 10 to 11, refused on mismatch (ADR-0010):
  - `ControlMessage::WindowFloor { floor: u64 }`, appended so no tag renumbers;
  - `WindowAck.passed_undelivered: u64`.
- **The receiver's ρ:** none on the wire. The handshake is unchanged except for the version.
- **`G_rx` at the receiver:** it counts at all three frontier writers (`drain_expired` sites, `force_drain_oldest`, the future floor pass) and subtracts fills of passed holes. The passed-hole set is pruned at the decode-inert floor (`receiver.rs:2236`). A fill after the prune is gauged, not subtracted. It is ungated (paper §5.6) and carried on every `WindowAck`.
- **The pair merge at the sender** (Amendment 2 decision 3, paper §5.6). `G_rx` is a level that falls on a late fill, and at N ≥ 2 acks arrive reordered. Keep `(A_rx, G_rx)` from the ack with the greatest `next_expected`, and on a tie the larger `passed_undelivered`. Never take `G_rx` from the latest-arriving ack alone.
- **Test (owed, red first):** `ack_pair_merge_survives_reordered_acks`, N = 2: deliver two acks out of order, the older with the lower `next_expected` and the higher `G_rx`; assert the merged pair is the newer ack's. Second case, the one that motivates the rule: the older ack (lower `next_expected`, lower `G_rx`) arrives last; assert it does not replace the pair. Third: a tie keeps the larger `G_rx`.
- **Tests:** codec round-trips and v10-to-v11 refusal.

**Step 5: the rate bound (A4, A5, A6).** This comes before the store. `SeqRing` is unbounded (`net/seq_ring.rs`), so a store at ρ < 1 with no pause would grow to the object size: the lost-tail run reads `O_hwm = 12 146` against a cap of 1 024.
- **The pause:** `pause ⇔ O ≥ B ∨ cwnd_full`, with `O = (H + 1 − F) − |released|`, through one helper for both copies, and B the pooled cap at every ρ (`plain_dyn_cap` loses the bit).
- **The ρ = 1 pin:** `O == sack_release_outstanding(sent_store.len(), released)` at every tick (A.5's owed equality).
- **Un-ignore T7a.** Invert the `O_hwm > 4·cap_at_hwm` assertion ("D1b") in `known_defect_lost_tail_strands_k_at_rho_below_1`.
- **T7b** `overload_breach_is_printed`. Unpaced source at ρ = 0.99. First the pause executes, read first. Then `O ≤ B + 1` at every DIAG tick, and `drop_queue` falls against the base (direction only). The line `[BUDGET] G_rx=… budget=… breach=…` is asserted present, not asserted within budget.
- **The bind gauge:** `[SUMCAP] pin=` at ρ < 1 is reported.
- **T7c** `sack_release_step_is_bounded` (Amendment 1 item 4, "C2 and the rate bound"). Decision 3's formula has no ρ, but its input `|released|` reads the bit through C2. Assert:
  - at ρ = 0.99, `released = 0` at every tick, and the sender's SACK channel is absent (the routing witness);
  - at ρ = 1, `released ≤ received_above` at every tick (the absolute bound on the step).

  Record the step's size at ρ = 1 against 1 − ε.

**Step 6: a store at every ρ (A1, A2, A3, A7, A8).**
- **The clamp:** removing it (A3) makes the gap unclamped at every ρ. So `nacked_count` covers the whole gap here, a deliberate behaviour change at ρ < 1 that the guard battery reads.
- **The forced verdict:** at both window advances (`mod.rs:3594–3597` and `sender_phases.rs:1374`), `evict(s) ∧ s ∈ U ∧ s ∉ store ⇒ E += 1`, pruned like a shed, with a visits-and-forced gauge that must read 0. It has no data to copy, so it sends the floor alone and always consumes one unit (Amendment 2 decision 2). Test (owed, red first, through a test seam that evicts a seq from the store): `forced_verdict_sends_floor_alone`, asserting no data send for that seq, `floor_sent ≥ 1` and one unit in `Ab`.
- **The account identity:** `S_sent = A + H + E + |U|` (ADR-0081 as amended).
- **Flips:** these defect assertions stop holding here; invert them in this commit:
  - in `known_defect_lost_tail_strands_k_at_rho_below_1`, `O_strand > 0`, `inert_sweeps > 0`, `clamp_excl > 0` and "k never resent". Assert instead `O_strand = 0`, `clamp_excl = 0`, and that k is resent or shed;
  - in `sim_shed_reach::witness_nack_site_sheds_at_rho_below_1`, the `inert_sweeps` and `clamp_excl` assertions.

  Any red-first test that now passes is un-ignored here.

**Step 7: the verdict rule and the budget (B1).**
- **The per-seq order** in a reported gap (§1.2):
  1. the stamp;
  2. skip if `s ∉ U`;
  3. `judge_seq`;
  4. serve.
- **Pure functions**, with absolute tests at the anchor points:
  - `judge_seq(age, d, ab, s_src, budget_frac, servable) -> Verdict`;
  - `shed_budget(rho, s_src) = (1 − ρ)·s_src`;
  - `abandon_count(g_rx, pending) = g_rx + pending`.

  `Ab = G_rx + |{v ∈ V : v ≥ A_rx}|` replaces `shed_total`, and `1 − ρ` replaces `residual_loss_after_fec` as `budget_frac`. R stays as a gauge printed beside `G_rx / S_src`.
- **B1:** `shed_armed` loses `reliable`. This is where the ρ = 1 test from §1 B lives.
- **The ±2 % tests** (derivation §5): `shed_budget` at ρ_p and ρ_p·0.98, plus ρ_p·1.02 clipped to 1 for ρ_p < 1. Assert `|Δ| = S·|Δρ|` exactly and `budget(1) = 0`. `judge_seq` may flip only where `age − D` or `(1 − ρ)·S − (Ab + 1)` changes sign.
- **`decision_trace_pin`:** its `shed_allowed` digest changes by design. Re-capture that digest only and say so in the commit message.
- **T6** `in_capacity_abandonment_within_budget`. ρ = 0.99, paced source below the §3.5 line (`SIM_PACE_MBIT=20`). Asserts:
  - `G_rx ≤ (1 − ρ)·S_src` at the end;
  - sheds stop when `Ab + 1 > (1 − ρ)·S_src`, and the refusal counter rises.

  It prints R and `G_rx / S_src` side by side.
- **The last copy** (Amendment 2 decision 1, paper §5.6): `verdict(s) ⇒` one copy of s sent with the floor, at the same instant, on every verdict, with no δ, ρ or hint condition. The copy is the stored payload; it is sent in this step and travels with the floor once step 8 lands. Test (owed, red first): `last_copy_sent_with_each_verdict`, ρ = 0.99, a past-D hole with no `dead` seam: for each verdict seq exactly one data send after the verdict, at the floor's instant, and the seq ends delivered, with `G_rx` not counting it.
- **Assert on `G_rx`, not on `ev=shed`.** With the last copy a verdict no longer implies a pass. T6, T1′, T2 and any other test that counts passes assert on `G_rx` (and `floor_passes`). T1′ and T2 still see a pass only because their `dead` seam kills every copy of k; say so in the test.
- **C7's feed** (after this step) subtracts the passed verdicts, `V ∩ passed`, not V (Amendment 2 consequences).
- **Flips:** the `H+E` budget-breach assertions of the two defect-asserting tests flip here or in step 8. Invert them where they flip.

**Step 8: the floor.**
- **Sender:** `F = min{s ≥ A_rx : s ∉ V}`. While `F > A_rx`, send it on every live path at each tail-sweep fire. Arm the sweep on `U ≠ ∅ ∨ F > A_rx` (`sweep_armed`). Send the last floor on the reliable control stream before `Shutdown`.
- **Receiver:** take the newest floor over paths, `floor_rx = max`, 0 before the first. The pass is its own function on `ReorderBuffer`, and it reads no `rho1_debt`; derivation §2.3 says "the pass reads no seat bit". The marker is `(F_rx, now + H)`, with H from `shed_recv_hold`, and the marker's own deadline arms the drain timer. The pass does not go through `drain_expired` or `oldest_deadline`, which return nothing on the ρ = 1 buffer (`reorder.rs:117–119`, `:182`). When the marker expires it delivers the pending entries below F in order, counts the passes in `G_rx` and moves the frontier to F. Unit tests run it on both constructors: the ρ = 1 buffer passes a hole below a floor exactly like the evicting one.
- **Absolute tests:** `floor(a, ∅) = a`; a run of verdicts from a gives `a + run`; a gap stops it. `floor_rx` at N = 1 and N = 2, including a path with no floor yet.
- **T4** (floor loss, ack loss, never-ack, dead peer, shutdown), **T5** (N = 2: the frozen idle leg; a slow leg's late fills excluded from `G_rx`) and **T5′** (N = 1 overtaking), as in derivation §6.
- **Un-ignore T1, T2 and `sim_mixed_rho::mixed_realtime_sender_bulk_receiver_ack_reaches_src`** (green here: the Bulk receiver's floor pass honours the Realtime sender's floors).
- **T1′** `last_seq_lost_is_passed_by_a_floor`. k is the object's last seq, `S_src − 1`, under `dead`, with nothing above it. Run at the declared ρ = 0.99 with no budget seam. Assert:
  - k is shed and then passed by `t_shed + 2·T_sweep_max + owd_max + H_max`;
  - `floor_sent ≥ 1`;
  - `floor_passes ≥ 1`.

  It is green on the finished tree. Its witness is the ablation below.
- **The ablation** is the evidence that the floor is what fixes these: a test seam `RWM_TEST_TS_NO_FLOOR=1` that suppresses `WindowFloor` on the finished tree. `ablation_without_the_floor_t1prime_and_t2_stall` (not ignored) runs T1′, T2, and T1 at ρ = 0.97 under the seam, and asserts that each one's ack stops short. T1 at 0.99 under the seam is the control that stays live (derivation §2.5). Record the outputs in the commit message.

**Step 9: the (C) rewrites (option B), T8 and the ledger.** This step starts only once `rho-step-sites.md` is accepted. Each (C) site is one commit, in the order the derivation gives, and each takes one of two forms:
- the derived law, continuous in ρ, with its ±2 % test through ρ = 1 and its ρ = 1 reduction test (it must equal today's ρ = 1 arm at ρ = 1, unless the derivation says why not);
- the split deleted, where the derivation finds no reason for it.

Each commit removes its rows from the ratchet table and from Amendment 1's `rho1-debt-readers` block, marks its ledger G row done, and updates its §11.3 row. The **last** commit deletes `rho1_debt_bit`, `net/rho_debt.rs` and every `rho1_debt` field, and switches the deny test to assert zero `rho1_debt` identifiers. That is the merge condition. The bounding tests below cover each site while it still reads the bit; they are removed or turned into continuity tests by its rewrite.
- **T8** `rho1_step_is_confined_to_the_c_sites`. The same seed at ρ = 1 and at ρ = 1 − ε with ε·S_src < 1 (for 6 MB, ε = 5·10⁻⁵ gives 0.6). It runs after every behavioural step, so the pause, the store and the budget are all in.
  - Assert on what no live (C) site touches: `V = ∅` on both sides, so `H = E = 0` and `floor_sent = 0`.
  - Record per-seq sends and resends, and the receiver's passes and `G_rx`.
  - Name the (C) sites live in its cell. At Realtime, N = 1 these are C2 (default on: it changes O and so the pause) and C9–C12; C3 is listed if `recov_mp` acts at N = 1.

  This replaces the derivation's "sender observables identical" (Amendment 1, the correction to decision 2).
- **`raptorpath/tests/rho1_debt_bounds.rs`**, one test per C row. Each asserts that the site routes by the bit:
  - the ρ = 1 arm executes at ρ = 1, through an existing counter or echo, or a test-seam hook;
  - the other arm executes at ρ = 0.99 and at ρ = 0.97, the same arm at both, so the step is at one point;
  - at ρ = 1 against 1 − ε, it prints the site's observable; that line is record-only.

  The absolute bounds to assert:
  - C9: at ρ = 1, `pass = pass_force = 0`;
  - C10: at ρ < 1, the drop count is printed beside `G_rx`, the known gap;
  - C11: at ρ = 1, `SHEDH evals = 0`;
  - C12: at ρ = 1, the `[QCLK] site=receiver` echo appears and there is no expiry.

  C8 is a config test: refused at 0.99, accepted at 1, and no runtime reader (the deny test).
- **Bookkeeping:**
  - the ratchet table ends empty: D1 prints ρ instead of the bit, and every (C) row has gone;
  - update the ledger rows F02 (done), G01–G12 and S09 with the test names;
  - flip the paper's "engine change pending" tense to the present for the parts that landed, and re-render `law-ledger/render.py`.

**Step 10: the model's give-up law (its own commit).** This is a deliberate model change (ADR-0084 decision 5). The wasm and cartoon give-up law moves to `1 − ρ`, and the visualizer anchor "= R when 1 − ρ ≥ R" (`test_visualizer_ui.mjs:238–241`) becomes "= 1 − ρ". Re-capture with `GOLDEN_CAPTURE=1` and say in the commit message that it is a deliberate model change.

## 5. Done criteria and the report

Done means all of these hold:
- each step's tests are red before it and green after it, with commands and outputs in the report;
- `expected_failure.py` lists nothing left from `rec-evict-blocker-unresolved` that passes;
- the deny test is red on its fixture and green on the tree;
- T3 and T8 are green;
- the wasm and visualizer gates pass after step 10.

Report the branch and the commit per step, every number the tests printed (`G_rx / S_src` against 1 − ρ, R, `O_hwm` against B, the refusal shares), the decisions you made locally, and anything in this brief that the spec contradicted.

The guards are not yours to run, and there is no VM. Three things for whoever runs them:
- the guard's BASE is main (with S06 merged) and its CAND is your head;
- the guard needs a ρ < 1 stage before merge (Amendment 1 item 8), because a Bulk-only guard cannot see this change: Realtime at 0.99, N = 1 and N = 2, with execution witnesses read before outcomes (store fill, pause share on `O ≥ B`, verdict visits, `floor_sent`/`floor_passes`, `G_rx` against `(1 − ρ)·S_src`);
- the harness takes `--window-reliable` on BASE and `--window-rho 1` on CAND, as Amendment 1 item 8 requires. The `tools/l1` scripts must detect the spelling from the binary (`perf --help | grep -q -- --window-rho`) before that guard runs. That change is a separate commit in `tools/`, listed in your report.
