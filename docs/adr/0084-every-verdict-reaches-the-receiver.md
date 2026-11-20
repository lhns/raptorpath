# ADR-0084: Every verdict reaches the receiver

## Status: Accepted (owner decision 2026-11-20: every default of `docs/derivations/evict-verdict-law.md` §9). Engine change pending. Paper: §5.1, §5.6, §6.1, §6.3, §11.1, §11.3, A.4, A.5. Supersedes in part ADR-0064, ADR-0079 Amendment 1 and ADR-0081, and the owner's 2026-11-14 `budget = min(1 − ρ, R)` decision (`giveup-law-model.md`); extends ADR-0010. **Amended 2026-11-20 (Amendment 1): the `window_reliable` flag leaves; ρ is the only retention dial; the engine change is stacked on `s06-delete`.**

## Context

Roadmap node `rec-evict-blocker-unresolved`. On the evicting seat (ρ < 1) a seq can leave the sender with no decision and the receiver is never told:
- the coding window evicts unresolved seqs, and no store holds them;
- a gap below the window is clamped away before the shed check, so it is neither served nor shed;
- a shed sends nothing, so a lost tail with nothing above it stalls the receiver's frontier (precheck `sr_evict_b05`: ack stuck at 12 087 of 12 147).

The shed budget does not read ρ: the engine runs `budget = R`, which is cold, frozen between source emissions, and ρ-blind (liveness audit item 4). At ρ < 1 the source has no outstanding bound at all (`tx_paused` requires `reliable`). That gate came with the store in `4c901536`, whose comment reads "EVICT mode never backpressures on retention"; no derivation ever made the ρ < 1 source unbounded (derivation §3.1).

## Decision

1. **Verdict and tell.** Every unresolved seq in a reported gap is judged on the unclamped gap: shed if `age > D ∧ Ab + 1 ≤ (1 − ρ)·S_src`, otherwise served; the window clamp only bounds what is served. A seq whose last servable copy leaves the sender gets a forced verdict at that instant, counted and never refused. The sender then tells the receiver a floor `F = min{ s ≥ A_rx : s ∉ V }`, repeated on the tail-sweep clock on every live path while `F > A_rx`; the last floor at shutdown goes on the reliable control stream. The receiver takes the newest floor from any path as a payload-free arrival at F under its existing hold.
2. **A retention store at every ρ.** `sent_store` fills on both seats; an entry leaves on ack or on verdict. Every unresolved seq stays servable, so forced verdicts are 0 by data at every ρ, and the sender-side step at ρ = 1 is gone. The forced arm stays as the rule's closure, with a gauge that must read 0.
3. **The source-rate bound at every ρ:** `pause ⇔ (hi + 1 − A_rx − |released|) ≥ B ∨ cwnd_full`, B the pooled cap of paper §6.1, with no ρ input. Under overload the source is delayed (ρ wins over δ), and the abandonment receiver give-ups still cause beyond 1 − ρ is printed, not prevented. This reverses `4c901536`'s intent.
4. **Protocol v11.** `ControlMessage::WindowFloor { floor: u64 }` and `WindowAck.passed_undelivered: u64` (the receiver's abandonment count) in one bump, `PROTOCOL_VERSION` 10 → 11, refused on mismatch as ADR-0010 requires.
5. **The 1 − ρ budget counts abandonment at the receiver.** `Ab = G_rx + |{v ∈ V : v ≥ A_rx}|`, with `G_rx` the holes the receiver passed undelivered minus their late fills, carried on every ack. The budget is the declared 1 − ρ; R becomes its predicted use, a gauge, not its size. The model's give-up law moves to 1 − ρ in its own commit with a `GOLDEN_CAPTURE=1` re-capture.
6. **ρ as a declared number.** A config field `window_rho: Option<f64>`, carried as `SenderPolicy.contract_rho` and read only by the shed budget. Bulk and Auto are ρ = 1; Realtime is declared at **ρ = 0.99**, a named point, declared and not derived. `reliable` is not derived from ρ; the pair (`window_reliable = false`, ρ = 1) is refused at config time.

7. **Smaller defaults.** D keeps the EWMA RTT until a battery reads the derived RTprop form; both are computed and their ratio logged (derivation question 7). SACK release stays reliable-only, the cwnd brake stays unarmed, and the budget stays cumulative (questions 8–10).
8. **Superseded in part:**
   - **ADR-0064 decision 3:** the budget operand `ε̂·(1 − P_fec)` with the cumulative shed count becomes `Ab` against 1 − ρ, and at ρ = 1 nothing is shed because the budget is 0; whether `shed_armed`'s seat read goes is left to the engine change. The law's form (age > D(δ) within a 1 − ρ budget, past-budget candidates served) stays.
   - **ADR-0079 Amendment 1:** a below-window blocker is no longer "judged by no sender path"; a shed now reaches the receiver; the shared budget now reads ρ. Its merge hold on `s06-delete` is unchanged.
   - **ADR-0081's identity** becomes `S_sent = A + H + E + |U|` (`O_strand ≡ 0` by construction), with a verdict seq staying in its term after a later ack, and abandonment counted at the receiver, not as `H + E`. This also supersedes the `E` term of `test-detection-plan.md` §7 answer 5.
   - **The 2026-11-14 owner decision** `budget = min(1 − ρ, R)`.
   - **`core-extraction-recovery.md`** decisions 3 and 5, the intake redundancy pin and step 4.

## Consequences

- The lost tail resolves: every abandoned seq is passed by the receiver within one sweep plus the hold per delivered floor (the repeat is bounded in rate, not in count), and the account closes with `|U| = 0` at every ρ.
- Realtime keeps δ-honest shedding through its declared ρ; without the declaration its budget would be 0.
- Costs at ρ < 1: one refcount increment and ring insert per source symbol (payloads are shared `Bytes`), resident memory up to the cap B, and floor repeats at most `|channel_paths|` datagrams per `TAIL_SWEEP_MIN_US`.
- On real links B sits at its N·2048 ceiling (ledger F01, PINNED), so the bound acts as a constant outstanding cap until the store law's time form lands. Its bind gauge is reported at ρ < 1.
- A step at ρ = 1 remains on the receiver: the reorder buffer never passes a hole at ρ = 1 and gives up on its ε̂_recv budget below it. A receiver law with ρ is a separate derivation (question 5), bounded meanwhile by a record-only step test.
- ADR-0078 and the receiver-timestamp plans (status §42, §43; roadmap `rec-framing-followups`) gave v11 to the receiver-timestamp field. It needs a later version; which one is the coordinator's to confirm.
- The engine change touches the store fill, the serve, the clamp bounds, the intake pause, the sweep arming, the receiver reorder buffer and the wire; it is a behaviour change at ρ < 1 and rides a guard batch. Its tests are the derivation's §6 (T1–T8), red first.

## Amendment 1 (2026-11-20): ρ is the only retention dial; the flag leaves

### Status

Accepted for the owner's two decisions of 2026-11-20 (items 1 and 3 below). Two parts are the owner's call and stand as recommendations until the owner rules: how the (C) sites read ρ (item 2, "owner's call") and the receiver's ρ (item 5). The reader inventory and the implementation order are in `docs/derivations/adr84-engine-brief.md`. The decisions above are unchanged; this amendment supersedes them where it says so.

### Context

The owner's first decision reads: "There is no such thing as reliable; it is just ρ = 1." Decision 6 above kept the `window_reliable` bool next to `window_rho` and refused the pair (`window_reliable = false`, ρ = 1). Decision 7 kept SACK release reliable-only. Both assume the flag survives.

Read on main `060db8af`, the flag reaches 48 reader lines (brief §1). They fall into four groups:
- (A) 16 lines in 12 sites that ADR-0084's engine change removes;
- (B) 2 lines, one site, that become zero by formula once the budget is (1 − ρ)·S_src;
- (C) 24 lines in 12 sites that have no ρ-continuous form yet;
- (D) 6 log, diagnostic and test-seam lines.

Removing the flag leaves the (C) sites only one source for their bit: ρ itself. The hint cannot supply it, because an explicit `window_rho` wins at any hint. So any surviving (C) branch is `ρ ≥ 1`, which is the NO-MODE-SWITCH trap (CLAUDE.md) under another name.

### Decision

1. **The flag leaves.** These go:
   - `window_reliable` in the config and in `PeerConfig`;
   - `--window-reliable` on `run` and `perf`;
   - `SenderPolicy.reliable`, the `run_window_sender` and `ServeGapsCtx`/`AckAdvanceCtx` `reliable` arguments, `recv_window_reliable`, `ReorderBuffer::reliable`, `WindowDelivery::hold_on_full` and `Account::reliable`.

   ρ becomes the only retention dial:
   - `window_rho: Option<f64>` in the config, and `--window-rho <f64>` on `run` and `perf`;
   - resolved by `config::resolve` to Bulk and Auto **1**, Realtime **0.99** (declared, not derived). An explicit `window_rho` wins at any hint;
   - the domain is 0 < ρ ≤ 1 and finite; anything else is a config error;
   - carried as `PeerConfig.window_rho: f64` and `SenderPolicy.contract_rho` (today a literal 1.0, `net/sender_policy.rs:616`).

   This supersedes two parts of decision 6: "`reliable` is not derived from ρ" and the pair refusal (no pair exists). Decision 6's "read only by the shed budget" was already false on main: `contract_rho` also feeds the three-term store law (`net/sender_phases.rs:182`, `RWM_THREE_TERM`, default off) and two log lines. With ρ declared, that opt-in arm reads the declared ρ through its own continuous `stall(δ, ρ)` term. The default cap (decision 3) still has no ρ input.

2. **The trap rule.** In engine code, nothing compares, matches or thresholds the retention ρ to select a code path, a law, a constructor or an argument. `reliable = (ρ ≥ 1)` is forbidden under any name, and so is any alias of a ρ value that feeds such a test. ρ enters the engine only as a number in a formula continuous in ρ, such as the shed budget `(1 − ρ)·S_src`.

   **One closed exception: the grandfathered ρ = 1 bit.** It is the trap itself, kept only for the (C) list in item 4:
   - derived at exactly one function, `rho1_debt_bit(rho: f64) -> bool { rho >= 1.0 }` in `net/rho_debt.rs`;
   - called once per end;
   - carried only under identifiers containing `rho1_debt`;
   - read only at the (C) sites and the counted (D) log lines.

   A new reader is forbidden. Removing one updates the deny test's table, the law-ledger row and the paper §11.3 row in the same commit. This has the same standing as the known `raptorpath-math` debts W22 and W14 in CLAUDE.md: a counted defect with a bounding test, not a mechanism.

   **Owner's call.** The alternative is to collapse every (C) site to one arm in this change, with no bit. That is 12 behaviour changes at ρ < 1, each needing its own Realtime battery. The recommendation is to grandfather the bit and retire sites one commit each.

   **Enforcement:** `raptorpath/tests/rho_deny_list.rs`, a source walk patterned on `raptorpath-core/tests/deny_list.rs`, run in the normal CI test job. It fails on any of the following:
   - (a) a comparison operator, `match`, `partial_cmp` or `total_cmp` on a retention-ρ identifier (`rho`, `window_rho`, `contract_rho`, `recv_rho`, `peer_rho`), anywhere in the non-test code of `raptorpath/src` (test files and `#[cfg(test)]` modules are skipped). Also any binding of a ρ value to a name outside that list, so `contract_rho: cfg.window_rho` passes and `let r = window_rho` fails. The allowlist is two function bodies, each defined exactly once: `rho1_debt_bit`, and `config::check_window_rho`, which holds the domain check and the corner refusal (C8). The retention ρ already travels as a bare `rho` parameter in `net/store_cap.rs` (`contract_stall_s`, `three_term_store_cap`). `net/late.rs` uses a local `rho` for the healing fraction ρ̂_heal, which is renamed `rho_heal` (zero-behaviour), so bare `rho` is denied with no exception;
   - (b) a per-file count of `rho1_debt` identifiers that differs from the (C)+(D) table, whether it rises or falls without a recorded change;
   - (c) any occurrence of the identifiers `reliable`, `window_reliable`, `recv_window_reliable`, `sender_window_reliable`, `hold_on_full` or `new_reliable` in code outside the two tombstones of item 6. Comments and string literals are exempt, so the byte-stable log echo survives.

   A planted fixture proves each form red, and the tree is green. The walk asserts that it visited files. The test is a tripwire, not a proof: review still owns aliases the scanner cannot see.

3. **Stacked on `s06-delete`.** The engine change is built on `s06-delete` merged up to main. That lifts ADR-0079 Amendment 1's merge hold, which decision 8 above kept, for this purpose only: `s06-delete` reaches main beneath this change, not on its own.

   There are two candidates and two guards, so a failure can be attributed:
   - guard 1: BASE = main, CAND1 = `s06-delete` merged up. This is ADR-0079's own guard, as registered.
   - guard 2: BASE = CAND1, CAND2 = this change on top.

   A guard-2 failure belongs to ADR-0084 and a guard-1 failure to ADR-0079. Merge-up facts for the worker:
   - the slot-site counters main added (`shed_visit_slot`, `on_shed_slot`, `on_denied_slot`) lose their site;
   - `sim_shed_reach::witness_slot_site_sheds_paced` is red on that tree by design and is inverted;
   - the `shed-slot` and `shed-slot-denied` entries in `tools/ci/coverage_sites.json` are stale and are removed.

4. **The (C) list is named debt.** Each item below gets a law-ledger row (G01–G12; C5 extends S09 instead), a paper §11.3 row and a bounding test in `raptorpath/tests/rho1_debt_bounds.rs`.

   Every bounding test has the same shape:
   - a routing witness: the arm taken at ρ = 1, and the same arm at ρ = 0.99 and ρ = 0.97, so the step is at one point only;
   - the step's observable recorded at ρ = 1 against 1 − ε with ε·S_src < 1. There the budget admits no shed on either side, and (A) is identical by data, so any difference belongs to the bit;
   - an absolute bound where one exists.

   | # | site (main `060db8af`) | ρ = 1 arm / ρ < 1 arm | what makes it continuous |
   |---|---|---|---|
   | C1 | placement split: `net/emit_source.rs:356` (source), `:863` (correction), `net/sender_phases.rs:973` (NACK resend), `:1118` (repair margin) | `place_symbol` law with the ρ_fate penalty / best-goodput pick (`select_source_path`, `select_repair_path[_avoiding]`) | the law at every ρ (it already composes with δ through its weights), after a Realtime N = 2 battery reads it no worse than the pick |
   | C2 | SACK release: `net/sender_policy.rs:628`, `net/mod.rs:1103` | SACKed seqs leave O / `released = ∅`, so O over-counts | release at every ρ, once a derivation shows `store_gate_released` stays a lower bound when the frontier also moves by give-ups (question 8), read against T7's `O_hwm` |
   | C3 | multipath hole law `recov_mp`: `net/sender_policy.rs:692` | per-flight suppression / global clock | it reads no ρ: run it at every ρ after a Realtime N = 2 battery |
   | C4 | single-path hole law `recov_sp`: `:705` (gate default off) | as C3 | as C3, at N = 1 |
   | C5 | emission batching scope `emit_batch_on`: `:737` (ledger S09, with the hint-keyed `use_packing`) | batched / per-symbol | drop the bit: the burst already stops at the pause at every ρ (A5); the Realtime packing exclusion stays S09's hint debt |
   | C6 | receiver request arms: `net/mod.rs:1049`, `:1058` through `net/sack.rs:36`; sender `request_arm` `net/mod.rs:1752` (gates default off) | armed / never armed | a request law that does not ask for a hole the ρ < 1 receiver has already passed |
   | C7 | Copa plain feed `plain_inorder`: `net/mod.rs:938` (opt-in) | fed / unfed | attribution that does not credit a give-up advance as delivery (subtract `G_rx`'s passes) |
   | C8 | corner domain refusal: `config.rs:304` (out-of-order, coded-only, generation, systematic) | accepted / refused at config time | show each corner defined at ρ < 1: generation keeps no store and no per-seq ARQ (ρ inert); coded-only would arm the shed; the out-of-order frontier needs the floor. Then drop the refusal. The runtime `window_reliable &&` conjuncts it makes redundant (`net/mod.rs:632`, `:646`, `:647`, `:1207`) are deleted now, zero-behaviour |
   | C9 | receiver reorder buffer: `net/receiver.rs:163`; `net/reorder.rs:88`, `:118`, `:182` | never passes a hole, never force-drains, never expires / hold, expiry, force-drain at capacity | a receiver give-up budget `(1 − ρ)·frontier` that passes nothing at ρ = 1 by formula, with the memory bound carried by the sender's pooled B at every ρ (decision 3) instead of the force-drain (question 5, a derivation) |
   | C10 | delivery on a full consumer channel: `net/receiver.rs:172`, `net/delivery.rs:66` | hold / drop | a drop is an abandonment: count it in `G_rx` (today it is not) and admit it only within the receiver's 1 − ρ budget, else hold |
   | C11 | receiver shed hold arm `recv_shed_on`: `net/receiver.rs:183` | off / `SRTT/2` under the ε̂_recv budget | follows C9, with `SRTT/2` becoming `b(δ)·SRTT` (§11.3 "receiver shed hold") |
   | C12 | receiver hole timer: `net/receiver.rs:734`, `:860` | refresh (re-advertise the gap) / hold expiry (give up) | both computed at every ρ: the refresh runs everywhere, and under C9's law the expiry passes nothing at ρ = 1 |

   Count: 12 sites, 24 reader lines. C8 reads ρ in `check_window_rho`, through the allowlisted refusal, not through the bit. The other 11 read `rho1_debt`. `check_window_rho` runs in `config::resolve` and again at the top of `net::run_impl`. `PeerConfig` is public and `perf::server`/`client` take one, so a hand-built config with a corner below ρ = 1 is refused at start instead of having its corner silently dropped. That makes deleting the conjuncts zero-behaviour for every config `resolve` accepts.

   **Correction to decision 2 and the paper's "retention at ρ → 1⁻" row.** "The sender-side step at ρ = 1 is gone" holds only for the store (group A) and the shed budget (group B). Seven sender sites still step at ρ = 1 through the bit, C1–C7. Of these:
   - C2 (SACK release) is on by default and changes O, and so the pause, at every N;
   - C5 is live at Bulk and Auto;
   - C1 and C3 are live at N ≥ 2.

   T8 therefore asserts identity only on what no live (C) site touches: the verdict set (V = ∅ on both sides, so H = E = 0 and `floor_sent = 0`). It records per-seq sends and resends, names the (C) sites live in its cell, and runs after the last behavioural step.

5. **The receiver's ρ (owner's call).** With no flag, the receiver needs ρ for its (C) bit (C9–C12). Today it reads its own local flag (`net/mod.rs:1195`, `net/receiver.rs:86`). Either option below reverses default 5's "no ρ at the receiver" for this purpose; neither gives the receiver's give-up budget a ρ, which keeps ε̂_recv until C9's derivation lands.
   - **(a) Each end reads its own configured ρ.** No new information flows, the same as today's flag. It carries a latent mismatch. Each direction's symbol size belongs to its sender (v10 `Handshake.symbol_size`), so a Realtime end can talk to a Bulk end. The Bulk end's receiver then runs the never-pass buffer on the peer's ρ < 1 stream. That buffer's `drain_expired` returns nothing, so a `WindowFloor` marker never passes a hole, and a lost tail stalls again (T1′).
   - **(b) ρ in the handshake (recommended).** `Handshake.retention_rho` carries the sender's ρ as IEEE-754 bits, in the same v11 bump as decision 4, so no second wire change is needed. Every path must carry the same value, reconciled and refused on mismatch like `reconcile_peer_symbol_size`, and the receiver derives its bit from the peer's ρ. The receiver reads the contract of the stream it receives.

6. **Old keys fail loudly.** `RaptorpathConfig` is `#[serde(default)]` without `deny_unknown_fields`, so deleting the field would silently ignore an old key. The removed names stay as tombstones, following the `mp_block_affinity` pattern (`config.rs:77–80`, `main.rs:105–107`):
   - the TOML key `window_reliable` stays as a parsed `Option<bool>`;
   - `--window-reliable` stays as a hidden flag.

   Setting either is a startup error: "`window_reliable` was removed by ADR-0084 Amendment 1: set `window_rho` (1.0 = retain until acknowledged, the Bulk/Auto preset; Realtime's preset is 0.99)". Neither is read anywhere else; deny test (c) pins that.

7. **Defaults.** No default ρ changes: Bulk and Auto stay ρ = 1 and Realtime stays the evicting preset, now the declared 0.99.
   - A bare Bulk or Auto run keeps its behaviour:
     - (A) is identical by data at ρ = 1;
     - (C) takes the ρ = 1 arm through the bit;
     - (B) is the one ρ = 1 code change: the shed check is now armed, D is refreshed and `shed_denied` counts past-D holes, all with budget 0, so there are zero sheds.
   - It is not binary-identical. Invariance at ρ = 1 rests on the decision pins, T3 and the guards, not on an identical-binary waiver. T8 only records the step at ρ → 1⁻.
   - The wire changes at every ρ (v11: `WindowAck.passed_undelivered`, `WindowFloor`, and `Handshake.retention_rho` under 5(b)), so a v10 peer is refused (ADR-0010).
   - A bare Realtime run takes ADR-0084's accepted change.
   - Today's `window_reliable = false` at Bulk or Auto has no numeric counterpart. Tests and harnesses map it to the declared 0.99, and `true` to 1.0.

8. **The VM guard harness maps per tree.** A guard across this change runs two binaries that spell retention differently:
   - BASE (main or CAND1) takes `--window-reliable`;
   - CAND takes `--window-rho 1`.

   Where neither is passed, both use their defaults: ρ = 1 at Bulk and Auto, the evicting preset at Realtime. The harness detects the spelling from the binary (`perf --help` lists `--window-rho`), not from the tree name, and records the mapping in the run's provenance. A cell that passed `--window-reliable` to BASE passes `--window-rho 1` to CAND. About 30 scripts under `tools/l1/` and 32 test files under `raptorpath/tests/` pass the flag today. The Rust test harnesses (`tests/common/loopback.rs`, `tests/sim_support`) move in the flag-removal commit; the L1 scripts move before the guard runs.

### Consequences

- The NO-MODE-SWITCH invariant gains a source deny test on the engine. Before this amendment, it had one only on the core and the visualizer.
- The trap is no longer hidden behind a config bool. It is one function and 23 counted `rho1_debt` reader lines (C1–C7, C9–C12) plus the log lines D1–D3 (brief §1), each (C) site with a ledger row and a §11.3 row.
- Decision 7's "SACK release stays reliable-only" now reads "stays on the ρ = 1 arm of the grandfathered bit (C2)".
- ADR-0079's merge hold is lifted only for the stacked build.
- The receiver either learns its peer's ρ on the wire (5(b), wire v11) or keeps today's local reading and its mismatch (5(a)).
- The tests that assert the defect invert with the fix: `sim_lost_tail::known_defect_lost_tail_strands_k_at_rho_below_1` and the defect assertions in `sim_shed_reach::witness_nack_site_sheds_at_rho_below_1`. So do the red-first T1, T1′ and T2 (brief §4).
