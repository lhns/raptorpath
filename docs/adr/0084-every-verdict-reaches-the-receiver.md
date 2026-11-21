# ADR-0084: Every verdict reaches the receiver

## Status: Accepted (owner decision 2026-11-20: every default of `docs/derivations/evict-verdict-law.md` §9). Engine change pending. Paper: §5.1, §5.6, §6.1, §6.3, §11.1, §11.3, A.4, A.5. Supersedes in part ADR-0064, ADR-0079 Amendment 1 and ADR-0081, and the owner's 2026-11-14 `budget = min(1 − ρ, R)` decision (`giveup-law-model.md`); extends ADR-0010. **Amended 2026-11-20 (Amendment 1): the `window_reliable` flag leaves; ρ is the only retention dial; S06 merges first on its own guard; all twelve (C) sites are rewritten (option B). Amended 2026-11-21 (Amendment 2): every verdict sends one last copy with the floor (a forced verdict sends the floor alone); the sender takes (next_expected, G_rx) as a pair from the ack with the greatest next_expected.**

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

Accepted. The owner's decisions of 2026-11-20 are recorded here:
- the flag removal (item 1);
- S06 merging on its own guard-2 PASS, with this change built on main after that merge (item 3);
- option B for the (C) sites: all twelve are rewritten and none is grandfathered (items 2 and 4).

**Pending:** the receiver's ρ (item 5). The derivation `docs/derivations/rho-step-sites.md` (branch `rho-step-sites`) decides it, together with the form of each (C) site. The reader inventory and the implementation order are in `docs/derivations/adr84-engine-brief.md`. The decisions above are unchanged; this amendment supersedes them where it says so.

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
   - the domain is 0 < ρ ≤ 1. NaN, ±∞, ρ ≤ 0 and ρ > 1 are config errors, each with a test (the precedent is the `RWM_DELTA` NaN defect, F1, merged in `9b8b42ca`);
   - carried as `PeerConfig.window_rho: f64` and `SenderPolicy.contract_rho` (today a literal 1.0, `net/sender_policy.rs:616`).

   This supersedes two parts of decision 6: "`reliable` is not derived from ρ" and the pair refusal (no pair exists). Decision 6's "read only by the shed budget" was already false on main: `contract_rho` also feeds the three-term store law (`net/sender_phases.rs:182`, `RWM_THREE_TERM`, default off) and two log lines. With ρ declared, that opt-in arm reads the declared ρ through its own continuous `stall(δ, ρ)` term. The default cap (decision 3) still has no ρ input.

2. **The trap rule.** In engine code, nothing compares, matches or thresholds the retention ρ to select a code path, a law, a constructor or an argument. `reliable = (ρ ≥ 1)` is forbidden under any name, and so is any alias of a ρ value that feeds such a test. ρ enters the engine only as a number in a formula continuous in ρ, such as the shed budget `(1 − ρ)·S_src`.

   **No exception (owner, 2026-11-20: option B).** Each of the twelve (C) sites is rewritten. The owner's framing is to ask whether each split has a reason, and to work only on behaviour that can be derived. `docs/derivations/rho-step-sites.md` gives each site its form: one law continuous in ρ, or the split deleted because it has no reason. Nothing merges with a ρ-selected branch.

   **Mid-stack scaffolding only.** While the change is built, the old bool may survive under one name so that each commit stays zero-behaviour where it should:
   - it is derived at exactly one function, `rho1_debt_bit(rho: f64) -> bool { rho >= 1.0 }` in `net/rho_debt.rs`;
   - it is carried only under identifiers containing `rho1_debt`;
   - its readers are pinned by the list below, which only shrinks.

   It is the trap itself and is never a mechanism. **The merge condition is a reader count of 0, with the function and every `rho1_debt` field and identifier deleted.** The deny test's final form asserts that no identifier containing `rho1_debt` exists anywhere in `raptorpath/src`. A branch head that still holds one does not merge.

   **Enforcement:** `raptorpath/tests/rho_deny_list.rs`, a source walk patterned on `raptorpath-core/tests/deny_list.rs`, run in the normal CI test job. It fails on any of the following:
   - (a) a comparison operator, `match`, `partial_cmp` or `total_cmp` on a retention-ρ identifier (`rho`, `window_rho`, `contract_rho`, `recv_rho`, `peer_rho`), anywhere in the non-test code of `raptorpath/src` (test files and `#[cfg(test)]` modules are skipped). Also any binding of a ρ value to a name outside that list, so `contract_rho: cfg.window_rho` passes and `let r = window_rho` fails. The allowlist is `config::check_window_rho`, whose body holds the domain check and nothing else at merge, plus `rho1_debt_bit` while the scaffolding exists. The corner refusal (C8) stays in that body only until its site is rewritten. The retention ρ already travels as a bare `rho` parameter in `net/store_cap.rs` (`contract_stall_s`, `three_term_store_cap`). `net/late.rs` uses a local `rho` for the healing fraction ρ̂_heal, which is renamed `rho_heal` (zero-behaviour), so bare `rho` is denied with no exception;
   - (b) **the ratchet on the scaffolding.** The test finds every occurrence of an identifier containing `rho1_debt` and attributes it to its file and enclosing `fn`. It compares the resulting set of `(file, fn, count)` with the list in the block `rho1-debt-readers` below, which the test parses from this file. The match must be exact:
     - a reader in a function not on the list fails;
     - a count above the listed one fails;
     - a count below it also fails, until the table is lowered in the same commit.

     The list only shrinks, and its end state is empty. The merge test asserts zero occurrences;
   - (c) any occurrence of the identifiers `reliable`, `window_reliable`, `recv_window_reliable`, `sender_window_reliable`, `hold_on_full` or `new_reliable` in code outside the two tombstones of item 6. Comments and string literals are exempt, so the byte-stable log echo survives.

   A planted fixture proves each form red, and the tree is green. The walk asserts that it visited files. The test is a tripwire, not a proof: review still owns aliases the scanner cannot see.

   The reader list: the functions allowed to hold the scaffolding bit mid-stack, on main `060db8af`, once the (A) and (B) readers have gone (they leave in their own steps). Each (C) site's rewrite removes its rows, and the list is empty at merge. The test's companion table pins the identifier count per function, plumbing included. The engine change records those counts at its step 3 and lowers them with every step that removes a reader; a count may only fall.

   <!-- rho1-debt-readers (file | fn | sites) -->
   ```text
   net/rho_debt.rs      | rho1_debt_bit      | the derivation (the one comparison)
   net/mod.rs           | run_impl           | the two rho1_debt_bit calls; C2, C6, C7, D1
   net/mod.rs           | run_window_sender  | C6
   net/sender_policy.rs | resolve            | C2, C3, C4, C5
   net/emit_source.rs   | emit_source        | C1 (source, correction)
   net/sender_phases.rs | serve_gaps         | C1 (NACK resend, repair margin)
   net/sack.rs          | request_law_armed  | C6
   net/receiver.rs      | run_receiver       | C9, C10, C11, C12
   net/reorder.rs       | new_rho1_debt      | C9 (constructor)
   net/reorder.rs       | push_with_time     | C9
   net/reorder.rs       | drain_expired      | C9
   net/reorder.rs       | oldest_deadline    | C9
   net/delivery.rs      | new                | C10 (constructor)
   net/delivery.rs      | offer              | C10
   ```
   <!-- /rho1-debt-readers -->

   Struct field declarations carrying the bit (`SenderPolicy`, `ReorderBuffer`, `WindowDelivery`) are attributed to their struct and listed by the test in the same way. At merge, those fields are deleted.

3. **S06 first, on its own guard; this change on main after it (owner, 2026-11-20).** S06 merges on its own guard-2 PASS in VM batch 1 (`docs/derivations/vm-batch-1-registration.md`):
   - BASE is S1 = main + `c8-gauges` + `tunnel-inorder-guard` (guard 1's candidate);
   - CAND is S2 = S1 + `s06-delete`, merged up.

   It does not wait beneath this change, because a guard pass goes stale as main moves. That lifts ADR-0079 Amendment 1's merge hold, which decision 8 above kept: S06 merges when its guard passes.

   The ADR-0084 engine change then branches from main after the S06 merge. Its own guard has main (with S06) as BASE and this change as CAND, so a failure belongs to ADR-0084 alone. If S06 has not merged when the engine work starts, the worker waits; the change is not stacked on an unmerged S06. Merge-up facts for S06 (VM batch 1 §1 uses them):
   - the slot-site counters main added (`shed_visit_slot`, `on_shed_slot`, `on_denied_slot`) lose their site;
   - `sim_shed_reach::witness_slot_site_sheds_paced` is red on that tree by design and is inverted;
   - the `shed-slot` and `shed-slot-denied` entries in `tools/ci/coverage_sites.json` are stale and are removed.

4. **The (C) list: all twelve are rewritten (option B).** Each item below has a law-ledger row (G01–G12; C5 extends S09 instead) and a paper §11.3 row until its rewrite lands. The table's last column records this amendment's first reading of what would make each site continuous. `rho-step-sites.md` supersedes it site by site: one law, or the split deleted where it has no reason. A site with no derivable behaviour is not rewritten by guesswork; it holds the merge.

   While a site still reads the scaffolding bit mid-stack, its bounding test in `raptorpath/tests/rho1_debt_bounds.rs` keeps the step visible. After the rewrite, the test becomes the site's ±2 % continuity test through ρ = 1. Every bounding test has the same shape:
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

   **C2 and the rate bound.** Decision 3's pause subtracts `|released|`. That term is identically 0 below ρ = 1. The sender's SACK channel is `None` there (`net/mod.rs:1103`). And `store_sack_release_on` is false (`net/sender_policy.rs:628`), so the loop at `net/mod.rs:2239–2267` neither marks `sack_released` nor folds `above_report`. `store_gate_released` therefore returns 0.

   So the bound's formula reads no ρ, but its input does. At ρ = 1, O is net of the seqs received above the frontier; just below ρ = 1 it counts them. The step is

   ```text
      Δ O  =  |released|   at ρ = 1   ∈  [0, received_above]  ⊆  [0, H + 1 − F]
   ```

   It is bounded by the release law's own lower-bound property (`store_gate_released_never_exceeds_the_retained_received_truth`). C2 does not retire in this change: the owner accepted question 8's default ("no for now"). Retiring it needs a derivation that the release stays a lower bound on `|S ∩ received|` when the ρ < 1 receiver's frontier also moves by give-ups. Instead:
   - the paper states the step (§6.1);
   - T7 bounds it: `released ≡ 0` at ρ = 0.99 (the routing witness), and at ρ = 1 the per-tick `released ≤ received_above` (an absolute bound);
   - the step's size at ρ = 1 against 1 − ε is recorded.

   **Correction to decision 2 and the paper's "retention at ρ → 1⁻" row.** "The sender-side step at ρ = 1 is gone" holds only for the store (group A) and the shed budget (group B). Seven sender sites still step at ρ = 1 through the bit, C1–C7. Of these:
   - C2 (SACK release) is on by default and changes O, and so the pause, at every N;
   - C5 is live at Bulk and Auto;
   - C1 and C3 are live at N ≥ 2.

   T8 therefore asserts identity only on what no live (C) site touches: the verdict set (V = ∅ on both sides, so H = E = 0 and `floor_sent = 0`). It records per-seq sends and resends, names the (C) sites live in its cell, and runs after the last behavioural step.

5. **The receiver's ρ (pending `rho-step-sites.md`).** The fix needs no ρ at the receiver, so decision 5's accepted default ("no ρ at the receiver") stands. Read in `net/reorder.rs`, the expiry path today honours nothing on the ρ = 1 buffer:
   - `drain_expired` returns early when `reliable` (`:117–119`);
   - `oldest_deadline` returns `None` (`:182`);
   - `new_reliable` sets `timeout = 0` and `max_buffered = usize::MAX` (`:43–51`).

   A floor marker pushed through that path would never pass a hole on the ρ = 1 buffer. The derivation does not use that path. Its floor pass (`evict-verdict-law.md` §2.3, "the pass reads no seat bit") is its own function, run on both buffers:
   - with its own marker deadline, `now + H`, where H is the receiver's existing `shed_recv_hold`;
   - with its own timer arm, because the marker arms the drain timer itself.

   Built that way, every buffer honours a floor. No floor is sent at ρ = 1, so the pass never fires there.

   Whether the receiver needs ρ at all after its sites C9–C12 are rewritten is pending. `rho-step-sites.md` decides it:
   - if a receiver law reads ρ (for example a give-up budget `(1 − ρ)·frontier`), the derivation also says whose ρ it is: the receiver's own config, or the sender's on the wire;
   - if no receiver law reads ρ, the receiver has none.

   Mid-stack, the scaffolding bit is derived from the receiver's own configured `window_rho`, the same information as today's local flag. There is no handshake field in this amendment.

   **The mixed configuration.** Each end reading its own retention is a defect on main today. A Realtime sender evicts and sheds; against a Bulk receiver, whose buffer never passes a hole, the frontier stalls. `tests/sim_mixed_rho.rs` shows it (proposed roadmap node `rec-mixed-rho-stall`). ADR-0084's store at every ρ and its seat-free floor pass fix it without a receiver ρ: every unresolved seq is served until it gets a verdict, and every verdict is told. The test is un-ignored in the floor step.

6. **Old keys fail loudly.** `RaptorpathConfig` is `#[serde(default)]` without `deny_unknown_fields`, so deleting the field would silently ignore an old key. The removed names stay as tombstones, following the `mp_block_affinity` pattern (`config.rs:77–80`, `main.rs:105–107`):
   - the TOML key `window_reliable` stays as a parsed `Option<bool>`;
   - `--window-reliable` stays as a hidden flag.

   Setting either is a startup error: "`window_reliable` was removed by ADR-0084 Amendment 1: set `window_rho` (1.0 = retain until acknowledged, the Bulk/Auto preset; Realtime's preset is 0.99)". Neither is read anywhere else; deny test (c) pins that.

7. **Defaults.** No default ρ changes: Bulk and Auto stay ρ = 1 and Realtime stays the evicting preset, now the declared 0.99.
   - A bare Bulk or Auto run keeps its behaviour:
     - (A) is identical by data at ρ = 1;
     - each (C) site's rewrite must reduce to today's ρ = 1 arm at ρ = 1, or say, with its derivation, why it does not;
     - (B) is the one ρ = 1 code change: the shed check is now armed, D is refreshed and `shed_denied` counts past-D holes, all with budget 0, so there are zero sheds.
   - It is not binary-identical. Invariance at ρ = 1 rests on the decision pins, T3 and the guards, not on an identical-binary waiver. T8 only records the step at ρ → 1⁻.
   - The wire changes at every ρ (v11: `WindowAck.passed_undelivered` and `WindowFloor`), so a v10 peer is refused (ADR-0010).
   - A bare Realtime run takes ADR-0084's accepted change.
   - Today's `window_reliable = false` at Bulk or Auto has no numeric counterpart. Tests and harnesses map it to the declared 0.99, and `true` to 1.0.

8. **The VM guard harness maps per tree.** A guard across this change runs two binaries that spell retention differently:
   - BASE (main, with S06 merged) takes `--window-reliable`;
   - CAND takes `--window-rho 1`.

   Where neither is passed, both use their defaults: ρ = 1 at Bulk and Auto, the evicting preset at Realtime.

   A Bulk-only guard cannot see this change, because at ρ = 1 every (A) site is identical by data and every rewritten (C) site is meant to reduce to its ρ = 1 arm. Before merge, guard 2 therefore needs a **ρ < 1 stage**: Realtime cells at the declared 0.99, N = 1 and N = 2. Each cell must show its execution witnesses before any outcome is read:
   - the store fills at ρ < 1;
   - the pause executes, a paused share above 0 on the `O ≥ B` disjunct;
   - the verdict sites are visited;
   - `floor_sent` and `floor_passes` (above 0 wherever a verdict happened);
   - `G_rx` beside `(1 − ρ)·S_src`.

   The harness detects the spelling from the binary (`perf --help` lists `--window-rho`), not from the tree name, and records the mapping in the run's provenance. A cell that passed `--window-reliable` to BASE passes `--window-rho 1` to CAND. About 30 scripts under `tools/l1/` and 32 test files under `raptorpath/tests/` pass the flag today. The Rust test harnesses (`tests/common/loopback.rs`, `tests/sim_support`) move in the flag-removal commit; the L1 scripts move before the guard runs.

### Consequences

- The NO-MODE-SWITCH invariant gains a source deny test on the engine. Before this amendment, it had one only on the core and the visualizer.
- No ρ-selected branch merges. Mid-stack the old bool survives as scaffolding under one pinned name, with a reader list that only shrinks. Its merge state is zero readers, with the field deleted. That is 23 reader lines (C1–C7, C9–C12) plus the log echo D1 at its widest; D2–D4 move off it.
- Decision 7's "SACK release stays reliable-only" is superseded: C2 is rewritten under `rho-step-sites.md`. Until then, the step it puts into the pause through `|released|` is stated and bounded (item 4).
- ADR-0079's merge hold is lifted: S06 merges on its own guard-2 PASS, and this change follows it on main.
- The receiver's ρ is pending the derivation. The mixed-config stall on main is fixed by the store and the floor, not by a wire field.
- The tests that assert the defect invert with the fix: `sim_lost_tail::known_defect_lost_tail_strands_k_at_rho_below_1` and the defect assertions in `sim_shed_reach::witness_nack_site_sheds_at_rho_below_1`. So do the red-first T1, T1′ and T2 (brief §4).

## Amendment 2 (2026-11-21): every verdict sends a last copy; the sender merges acks by pair

### Status

Accepted (owner, 2026-11-21). It amends decision 1 (the verdict and its tell) and the ack handling behind decision 5 (the count). Source: `docs/derivations/skip-rule-efficiency.md` §3.1 (finding F1 and the merge rule). Paper: §5.6, §5.1. The decisions above keep their text; this amendment adds to them where it says so. Wire v11 is unchanged.

### Context

- **F1.** Decision 1 sends the floor and no copy on a shed. The copy and the floor travel together, and the floor then waits H at the receiver. So a copy-less shed resolves an isolated hole H later than a plain resend whenever the resend would have arrived, and it spends one unit of the 1 − ρ allowance per hole. With one last copy sent beside the floor, the hole resolves at `min(fill, owd + H)`: never later than serving, and a unit is spent only when the copy is lost too (about ε·ĥ instead of ĥ).
- **G_rx is a level, not a counter.** It falls when a passed hole is filled late. At N ≥ 2 acks arrive reordered. Taking G_rx from the latest-arriving ack can pair a stale, lower G_rx with a newer frontier; that under-counts Ab and overshoots the budget.

### Decision

1. **Every verdict sends one last copy of the seq together with the floor**, at the same instant (amends decision 1):

   ```text
      verdict(s)  ⇒  send one copy of s  and the floor F,   at the same instant
   ```

   The verdict still stops retrying; it no longer withholds the last copy. There is no condition on δ, ρ or the hint: the copy goes with every verdict, and bandwidth is that of serving once.

2. **A forced verdict sends the floor alone.** A verdict forced by eviction (the last servable copy left the sender) has no data to copy, so it sends the floor only and always consumes one unit. This is the absence of data, not a branch: the rule above sends a copy of whatever the store still holds. With a store at every ρ (decision 2) forced verdicts are 0 by data, and their gauge must read 0; a nonzero reading costs F1's full penalty and is a finding.

3. **The ack pair-merge rule** (amends the ack handling behind decision 5). The sender keeps `(next_expected, G_rx)` as a pair, taken from one ack:

   ```text
      keep (A_rx, G_rx) from the ack with the greatest next_expected;   on a tie, the larger G_rx
   ```

   This is conservative (it never pairs a lower G_rx with a newer frontier) and converges as the frontier moves. `Ab = G_rx + |{v ∈ V : v ≥ A_rx}|` reads the merged pair.

### Consequences

- **Most verdict seqs now end delivered, not passed.** A filled verdict seq is delivered in order and leaves Ab's pending term without entering G_rx.
- **C7's feed** (`rho-step-sites.md` C7) subtracts the *passed* verdicts, not V: `V ∩ passed`, known from the floor's ack and G_rx, or a seq leaves the exclusion when it is filled. Subtracting all of V would under-credit every filled verdict.
- **Tests assert on G_rx, not on `ev=shed`.** "Verdict ⇒ passed" no longer holds. Tests whose `dead` seam kills every copy of the seq (T1′, T2, the rho-step-sites §3 acceptance test) still see a pass; any other test that counts passes asserts on G_rx.
- **Owed tests**, red first, in `docs/derivations/adr84-engine-brief.md`: the last copy is sent with each verdict; a forced verdict sends the floor alone; G_rx-based assertions; the pair merge at N = 2 with reordered acks.
- **Not part of this amendment** (they wait on the toy simulation or the ω measurement): run-atomic spending, the time threshold T_ρ, the receiver's release arm (design U), the new H, and retiring `window_out_of_order`. The receiver law R1 (floor-only) is the staging point only, not accepted as final; Amendment 1 item 7's "a bare Bulk or Auto run keeps its behaviour" is unchanged by this amendment (for the later release rule see `rho-step-sites.md` §8).
