# The twelve ρ-step sites: does each split have a reason? (derivation, no code)

**The owner's decision (2026-11-20).** The `window_reliable` flag goes ("there is no reliable; it is just ρ = 1"). All 12 sites that behave differently at ρ = 1 and below it are rewritten, none grandfathered (option B). The question for each: *"Do the 12 things, as they are now, have a reason to be that way? If not, they are more or less useless anyway. We have to work on correct things we can understand."*

**Inputs.**
- The site list C1–C12 is ADR-0084 Amendment 1 item 4, `docs/derivations/adr84-engine-brief.md` §1, ledger rows G01–G12 and paper §11.3, all on branch `adr84-amend`.
- Line numbers are on `github/main` `0c1692c2`. Its `raptorpath/src` is byte-identical to `060db8af`, the commit the brief uses.
- "The ADR-0084 change" means the brief's steps 0–8: a store at every ρ, the rate bound, the verdict rule with the `(1 − ρ)·S_src` budget, protocol v11 and the floor. Everything below sits on top of it.
- A paced-source run (branch `paced-lost-tail`, CI 44759961432): at ρ < 1 the lost tail still stalls, with zero queue drops, at 20 and 35 Mbit/s on a 40 Mbit link, 3 seeds each. So the stall does not need an over-driven source. Its cause is the below-window handling (the clamp, and no store), and the rate bound alone does not fix it. That fits §1.1: the receiver has no entry above a lost tail, so no receiver-side law can pass it, and only the sender's verdict plus the floor can.

**Shapes.** For each site the answer at ρ = 0.999 takes one of four shapes:
- (i) keep one arm at every ρ and delete the branch;
- (ii) the term is gated by a (1 − ρ) quantity, so it is inert at ρ = 1 by arithmetic;
- (iii) a feature that has to be made valid at every ρ;
- (iv) no clear answer: an owner question.

## 0. Summary

Every site has a formula with no ρ comparison. Each formula reproduces today's ρ = 1 decisions exactly, because at ρ = 1 it *is* today's ρ = 1 arm, or because its (1 − ρ) term is 0 there.

| # | site (main) | today: ρ = 1 / ρ < 1 | reason for the split found? | correct at ρ = 0.999 | shape | formula (§2) | pin | red-first test |
|---|---|---|---|---|---|---|---|---|
| C1 | placement: `emit_source.rs:356`, `:863`; `sender_phases.rs:973`, `:1118` | §5.7 softmax law / best-path pick | **no**: rollout scope, "wire it into run_window_sender (reliable mode only)" (`4cf6b9b1`) | the law | (i) | §5.7 `P(i)` at every ρ | N = 1 absolute: both arms pick the one path | `c01_placement_law_at_every_rho` |
| C2 | SACK release: `sender_policy.rs:628`, `mod.rs:1103` | release on / `released = ∅` | **no**: copies the store's scope ("the gate arithmetic is exactly the shipped store_len", `ff7acb4c`); the store was ρ = 1-only | release, with H the highest seq *sent* | (iii) | `\|released\| ≤ \|S ∩ (received ∪ passed)\|` at every ρ | `SenderPolicy` ρ = 1 digest | property test with a verdicted tail (red today) plus T7c inverted |
| C3 | `recov_mp`: `sender_policy.rs:692` | per-flight hole law / legacy age gate | **no**: the stated reason excludes generation only ("generation mode has no per-seq ARQ", `8a345200`) | the law | (i), after C12 | `fire ⇔ time_ripe ∨ packet_threshold` at every ρ | N = 1 path of the law; ρ = 1 digest | `c03_hole_law_at_every_rho` |
| C4 | `recov_sp`: `sender_policy.rs:705` (gate off) | as C3, at N = 1 | **no**: same text (`982b1a07`) | as C3 | (i), after C12 | as C3, time channel | gate-off identity | `c04_recov_sp_scope_reads_no_rho` |
| C5 | `emit_batch_on`: `sender_policy.rs:737` | batched / per-symbol | **yes, now void**: "inside the flow-control store headroom" (`75cbd012`); the pause was ρ = 1-only, and ADR-0084 A5 puts it at every ρ | batched | (i) | `gate ∧ ¬coded_wire ∧ ¬use_packing` | ρ = 1 and Realtime digests unchanged | `c05_burst_stops_at_the_pause_below_1` |
| C6 | request arms: `mod.rs:1049`, `:1058` (`sack.rs:36`), `:1752` (gates off) | armed / never | **no**: the stated reason is generation ("no per-seq layer to suppress", `b15a8e8c`) | **owner** | **(iv)** | default: requests judged by the same verdict rule | gate-off identity | `c06_request_is_judged_like_a_gap` |
| C7 | Copa plain feed: `mod.rs:938` (opt-in) | fed / unfed | **no**: the reason covers out-of-order and generation only (`a895205e`) | fed, minus verdicts | (ii) | `newly = ([c, A_rx) ∪ SACK) \ attributed \ V` | V = ∅ at ρ = 1 | `c07_feed_never_credits_a_verdict` |
| C8 | corner refusal: `config.rs:304` | accepted / refused | **yes, now void**: "the reliable store is the backstop for every hole they leave behind" (`dacfd7cc`). The store exists at every ρ after ADR-0084; generation never had one | accepted | (i) generation, systematic; (iii) out-of-order, coded-only | refusal deleted; floor pass on the out-of-order frontier | config tests at 1 and 0.99 | `c08_*` (§2.8) |
| C9 | reorder buffer: `receiver.rs:163`; `reorder.rs:88`, `:118`, `:182` | never passes a hole / expiry, force-drain at 500 | **a reason for the expiry, none for the split**: "force-delivered to avoid head-of-line blocking. Max 500 buffered entries" (`628fdb1c`, March 2026, before any retention existed). The ρ = 1 arm was added next to it (`4c901536`). After ADR-0084, HOL avoidance is the sender's δ verdict, and every verdict is told | passes only below the floor | (i) | `next' = max(next, F_rx)` at `t_F + H` | `ReorderBuffer` script digest, no floor | `c09_receiver_passes_only_below_a_floor` |
| C10 | full consumer channel: `receiver.rs:172`, `delivery.rs:66` | hold / drop | **hold has a reason valid at every ρ; drop has none** ("as it always has", `780be2b1`) | hold | (i) | hold at every ρ | `delivery.rs` ρ = 1 tests unchanged | `c10_full_channel_holds_below_1` |
| C11 | receiver shed arm: `receiver.rs:183` | off / `SRTT/2` under ε̂_recv | **no**: "compiled OUT on the reliable (ρ = 1) contract" (ADR-0064 decision 3), by fiat, not by a 0 budget; "this path exists only for the Realtime hint" (`dacfd7cc`) | no local give-up; H = the floor marker's hold | (i) | `H = min(b(δ_rx), 2)·SRTT_rx` | inert at ρ = 1 (no floor) | `c11_no_local_give_up_below_1` |
| C12 | hole timer: `receiver.rs:734`, `:860` | re-advertise / expire | **as C9**: the expiry from `628fdb1c`; the refresh was added only to the arm that never gives up (`4c901536`) | refresh, plus the floor marker | (i) | `deadline = min(t_nack + refresh, t_req, t_F + H)` | ρ = 1 term set unchanged | `c12_refresh_runs_below_1` |

**Count:**
- (i): 8 sites (C1, C3, C4, C5, C9, C10, C11, C12), plus C8's generation and systematic corners;
- (ii): 1 (C7);
- (iii): 1 (C2), plus C8's out-of-order and coded-only corners;
- (iv): 1 (C6).

For two sites the split had a written reason about retention: C5 and C8. Both said "ρ < 1 has no store or no pause", and ADR-0084 removes it. C9 and C12's expiry has a reason that is not about ρ: head-of-line avoidance, from a tunnel that had no ARQ. ADR-0084's verdict now carries it. C10's hold has a reason that holds at every ρ. In every other site the bool was at hand, or the store's scope test was copied. The original Phase A commit claimed the continuous form and shipped a bool: "Retention policy (rho-parameterized, T_cut(rho); rho=1 = ack-only)" (`4c901536`), while the code holds no ρ.

**Bulk and Auto defaults:** none move. At ρ = 1 every formula is today's ρ = 1 arm, or has its (1 − ρ) term at 0. (Owner, 2026-11-21: the later release rule of design U may change Bulk after a long outage; see §8.)

Shape (i) here means "keep one arm at every ρ". That is wider than "redundant by data": six of the (i) sites (C1, C3, C9, C10, C11, C12) change Realtime behaviour.

**Realtime defaults that move** (Realtime is ρ = 0.99; the owner accepted ADR-0084's Realtime change, but these come on top of it):
- C9, C11, C12: a hole is passed on the sender's verdict and floor instead of the receiver's own `SRTT/2` hold (§1);
- C10: a full consumer channel backpressures instead of dropping;
- C1: at N ≥ 2, the placement law replaces the best-path pick;
- C3: at N ≥ 2, the per-flight hole law replaces the age gate. At every N, gap reports are coalesced to the newest snapshot (`take_gap_batch(.., pol.recov_mp_law, ..)`, `sender_phases.rs:657`);
- C2: O becomes net of SACKed seqs, so the pause opens earlier.

## 1. The receiver question

Under option B, "no ρ at the receiver" is open again. The bit that Amendment 1 item 5 derived from the receiver's own config is gone, so C9–C12 need a law. There are two options:
- **(R1) floor-only.** The receiver passes a hole only below the sender's floor. It keeps no give-up law, no budget and no ρ.
- **(R2) local budget.** The receiver gives up on its own hold, within a budget that uses the sender's ρ as a number. That needs ρ in the handshake, and a budget shared by two deciders.

### 1.1 What the code says

- **A local give-up needs a pending entry above the hole.** The receiver timer arms only on `waiting && pending` (`receiver.rs:723–725`), and `drain_expired` takes its max over pending entries (`reorder.rs:123–130`). A lost tail has nothing above it, so no receiver law, under any budget, can pass it. The floor is needed in both options. That is the paced-source result: no over-driven source, and still a stall.
- **Today's ρ < 1 receiver budget never stops a give-up.** `shed_recv_hold` (`shed.rs:58`) returns `SRTT/2` while `holes < ε̂_recv·frontier`, and otherwise `(4·SRTT).clamp(60, 300) ms`. That hold still expires. Its test calls the second arm "budget spent ⇒ serialize" (`net/tests.rs:263`), but it is a slower give-up, not a serialize. So no existing receiver bound survives at 0.999. R2 would have to be built from nothing.
- **The ρ = 1 buffer's reason is valid at every ρ once ADR-0084 lands.** "Holes are recovered by NACK/repair (the sender retains sent bytes in its ARQ store and retransmits until delivered)" (`reorder.rs:21–26`). After ADR-0084 that is true at every ρ, up to a verdict, and every verdict is told by the floor.

### 1.2 Recommendation: R1, floor-only

- **One decider, one budget.** The sender's verdict is the only abandonment: `age > D ∧ Ab + 1 ≤ (1 − ρ)·S_src`. The receiver passes exactly the told verdicts, so `G_rx` counts the sender's decisions, and `Ab ≤ (1 − ρ)·S_src` holds by construction. Under R2, two deciders spend one budget across a feedback delay. The overshoot is bounded only by the decisions in flight in one RTT, and it needs its own derivation.
- **No ρ on the wire and none at the receiver.** C9–C12 then have no ρ in any form, which is the simplest way to satisfy the deny test.
- **The mixed pairings are fixed by construction.** A Realtime sender to a Bulk receiver: the receiver passes the floors it is told (the floor pass is already seat-free, `evict-verdict-law.md` §2.3). A Bulk sender to a Realtime receiver: no floor is ever sent, so nothing is passed (§4).
- **The receiver's memory bound moves to the sender**, as at ρ = 1 today: the outstanding cap B at every ρ (ADR-0084 decision 3). The 500-entry force-drain goes. *Open, and not new:* under SACK release, the receiver's pending set above a stuck hole is bounded at ρ = 1 by nothing derived (O excludes released seqs). R1 gives ρ < 1 exactly the ρ = 1 bound. A derivation is owed at both.

**The cost, a Realtime default change: give-up latency.** Today the receiver passes a hole `SRTT/2` after the next entry is buffered, about `owd + SRTT/2` after the loss. Under R1 the path is: gap report → verdict at age > D (`D = RTprop/2` at Realtime) → floor → receiver → H. That is about `RTT + owd + H`, plus the floor's cadence. To keep that cadence out of the latency, R1 reopens `evict-verdict-law.md` question 6. Its default ("sweep clock only") rested on "holes with data above them pass on the receiver's own hold", and R1 removes that premise. So:

```text
   send WindowFloor{F}  at the end of every serve_gaps pass that raised F   (and on the tail-sweep clock while F > A_rx, as before)
```

The rate is at most one floor per path per gap report. Gap reports are already rate-limited by `GAP_ACK_MIN_INTERVAL`, and forced verdicts are 0 by data once there is a store at every ρ, so the "bursts of thousands" concern is gone. No new constant. The guard's ρ < 1 stage measures the latency change (the frontier-stall p99 at Realtime, N = 1 and N = 2, against CAND1).

**The larger cost, a Realtime default change: a spent budget serializes.**
- Today a spent receiver budget still passes the hole after `(4·SRTT).clamp(60, 300) ms`.
- Under R1, once `Ab + 1 > (1 − ρ)·S_src`, a past-D hole is served until it is filled, with no time bound: ρ wins over δ. That is ADR-0084 decision 1's rule, but today's Realtime receiver never obeyed it.
- On a link whose residual loss exceeds 1 − ρ, Realtime then stalls where it used to skip.
- Guard 3 carries a cell for it: Realtime 0.99 at a loss and blackout pattern that exhausts the budget. It reads the frontier-stall p99 against CAND2 and `shed_denied`.

**What R1 changes, site by site:** C9 and C11 lose their give-up arm; C12's expiry becomes the floor-marker term while its refresh runs at every ρ; C10 holds at every ρ.

## 2. The sites

Each formula is on its own line, next to the sentence it implements. Symbol provenance follows the formula. "Pin" says how today's ρ = 1 decisions are shown unchanged.

**The pin extension.** `net/decision_trace_pin.rs` gains a `rho_sites` digest, captured at brief step 2 on the tree that still has the bit:
- the boolean fields of `SenderPolicy::resolve` over a grid of hint × {generation, coded_only, systematic, out-of-order} × every gate in the sites' predicates, at ρ = 1;
- `ReorderBuffer` and `WindowDelivery` driven by a scripted push, drain and full-channel sequence, with no floor.

Every site commit must keep it. A commit that changes it is not a site commit.

The digest cannot reach the inline sites: C1's four call sites, the hole law in `serve_gaps`, C12's timer and C7's feed. So there is a second, universal pin. **Each site commit keeps the ρ = 1 sim trace byte-identical to the previous commit.** The trace is `ChildRun.trace`, every link event with its virtual time. It runs in four ρ = 1 cells: the Bulk and Realtime hints at N = 1 and N = 2, seed 42, the `sim_lost_tail` scenario.

**The all-seq pass count.** The `pass` and `pass_force` tseam events print only for the seam's seq k (`test_seams.rs:167–172`, `:230–235`), so they cannot show "no pass". Brief step 4 already counts `G_rx` at every frontier writer. It must count each writer separately and ungated, `G_rx = G_expiry + G_force + G_floor`, each being frontier advance minus delivered at that writer, carried on `[ACCT-RX]`. "No local pass" then means `G_expiry = G_force = 0`.

### C1: placement (4 sites)

- **Today.** `emit_source.rs:356` (source), `:863` (correction), `sender_phases.rs:973` (NACK resend) and `:1118` (repair margin). At ρ = 1 each runs `sched.place_symbol(..)`: the §5.7 softmax, with the fate term on repairs. At ρ < 1 each runs `best_source_path` or `best_repair_path[_avoiding]`: an argmin of `path_cost` over paths with `available() > 0`.
- **Reason.** None for ρ. `4cf6b9b1` (2026-07-06): "wire it into run_window_sender (reliable mode only; single-path is byte-identical — the law with N=1 is that path always)". That is a rollout scope. The paper says "Every source and repair symbol is placed by one continuous marginal-cost rule" (§5.7), with no ρ. The law already composes with δ through `w_bw(δ)`.
- **At 0.999:** the law. The pick has no derivation at any ρ (status §3.3 lists it with the ρ-keyed debts).
- **Shape:** (i).
- **Formula.** "Every symbol is placed by one continuous marginal-cost rule":

  ```text
     P(i)  ∝  exp( −(c_i − min_j c_j) / T ),   c_i = L_i + B_i + V_i        at every ρ
  ```

  The symbols are those of §5.7, unchanged. T is an open constant (status §3.4), w_bw(δ) is the dial, and L, B, V are measured.
- **Pin.** The ρ = 1 arm is the kept arm. At N = 1, an absolute test: for any scheduler state, `place_symbol` and `best_*` return the one path. `best_*` filters `available() > 0` and falls back to `unwrap_or(0)`; the law has no capacity filter. Both give the single path, so the test asserts equality, including at `available() == 0`.
- **Default change:** Realtime at N ≥ 2, which goes from best-path to striping. The Realtime duplicate send (`emit_source.rs:508`, hint-keyed, status §3.3) picks its alt path relative to `source_path`, so it changes with C1. It stays a hint debt and is not touched here.
- **Red-first test.** `c01_placement_law_at_every_rho`: at ρ = 0.99, N = 2, asymmetric legs, the source split follows the law's fixed point (a share on the slow leg greater than 0). Red today: everything goes to the best path.

### C2: SACK release

- **Today.**
  - At ρ = 1, `store_sack_release_on = reliable ∧ ¬generation ∧ ¬coded_only ∧ gate` (`sender_policy.rs:628`), and the SACK channel exists (`mod.rs:1101–1107`), so SACKed seqs leave O.
  - At ρ < 1 there is no channel and `released ≡ 0`, so O counts them.
- **Reason.** None. `ff7acb4c` scoped it to the store it releases from ("sent_store holds the only payload copy"), and the store existed only at ρ = 1. ADR-0060 has no ρ. ADR-0084 question 8 then deferred release at every ρ ("no for now"), pending a proof that the release stays a lower bound when the frontier also moves by give-ups.
- **At 0.999:** release on.
- **Shape:** (iii). The lower bound has to be re-derived, and the re-derivation finds a defect that release at ρ < 1 would expose.
- **The code.**
  - `store_released_for_gate = min(|S|, max(marks, above))` (`sack.rs:387`).
  - `above = |S ∩ [0, F)| + (A − not_retained)⁺`, with `not_retained = (H + 1 − F) − |S ∩ [F, H]|` (`received_above_released`, `sack.rs:361`).
  - `store_gate_released` takes H as **the store's last key** (`sack.rs:403`).
- **The derivation's ρ = 1 premises** (doc comment, `sack.rs:342–357`):
  1. "every seq `< F` was delivered";
  2. "the reliable receiver never evicts above its frontier";
  3. "S is pruned only as a prefix", so "its last key is the highest seq ever sent" (`sack.rs:391–394`).
- **Each premise at ρ < 1, after ADR-0084:**
  1. Below F every seq is received **or passed**. A released passed seq is one the receiver no longer needs, so the bound to keep is against `received ∪ passed`. Under R1, `passed ⊆ V` and a verdict leaves S, so it is again exactly `|S ∩ received|`.
  2. Under R1 the receiver passes only below F, so nothing above F leaves the received set.
  3. **False.** A verdict removes an entry from S anywhere, the tail included (T1′: the object's last seq is shed). Then S's last key h is below the true highest sent seq H. `not_retained` is under-counted by `H − h`, and `above` over-counts by up to `H − h`, which is an over-release. The fix is to read H from the sender's highest-sent counter, not from S.
- **Formula.** "A SACKed seq leaves O; its payload stays until the cumulative frontier passes it" (ADR-0060):

  ```text
     |released|  =  min( |S|,  max( marks,  |S ∩ [0, F)| + ( A − ((H_sent + 1 − F) − |S ∩ [F, H_sent]|) )⁺ ) )   ≤   |S ∩ (received ∪ passed)|        at every ρ
  ```

  - **Symbols.** S is the store, which keeps an entry until ack or verdict (ADR-0084 decision 2). F and A are the newest report's `next_expected` and `received_above`, measured. `marks` are the per-seq SACK marks. `H_sent` is the highest seq sent, a counter.
  - **At ρ = 1** `H_sent` equals S's last key, so `|released|` is today's.
  - **The claim is on the count, not on a set.**
- **Pin.** The `SenderPolicy` digest (at ρ = 1 the deleted conjunct was true), plus the per-commit ρ = 1 trace identity.
- **Default change.** At Realtime the pause opens earlier (O net of received-above), and the pause still binds at B. At Bulk and Auto, none.
- **Red-first tests.**
  - `store_gate_released_bound_survives_a_verdicted_tail`: the property test of `net/tests.rs:1690`, with entries removed from S at random and at the tail (verdicts) and H_sent above S's last key. It asserts `released ≤ |S ∩ received|`. Red today, through the last-key H.
  - The brief's T7c, inverted: at ρ = 0.99, `released > 0` on at least one tick (the routing witness; today `released ≡ 0`), and `released ≤ received_above` at every tick at both ρ.

### C3: the multipath hole law (`recov_mp`)

- **Today.** `recov_mp = gate ∧ reliable ∧ ¬generation` (`sender_policy.rs:692`), with `RWM_RECOV_MP` on by default.
  - At ρ = 1 and N ≥ 2, a gap seq fires on the RFC 9002 per-path time or packet threshold (`sender_phases.rs:892–930`).
  - At ρ < 1 it fires on the legacy age gate (`:952–965`).
  - The same flag also gates gap-batch coalescing at every N (`take_gap_batch(.., pol.recov_mp_law, ..)`, `:657`) and the evidence prune (`:1437`).
- **Reason.** None for ρ. `8a345200`: "plain window reliable mode only — generation mode has no per-seq ARQ to suppress". In July the evicting seat had per-seq ARQ too (the NACK site served from the encoder).
- **At 0.999:** the law. Its inputs (`nack_retx_at`, `retransmit_buffer`, `source_path_map`) exist at every ρ once A7 keeps the path map to `ack + 1`.
- **Shape:** (i), **after C12**. The law is suppression-only, and its safety argument is the receiver's hole refresh: "the receiver's hole-refresh re-advertises until a channel fires, so real holes still recover" (`:903–905`). Below 1 that timer is today the expiry, not the refresh.
- **Formula.** "A gap seq is a hole once its live flight is older than 9/8 of its own path's clock, or 3 later same-path symbols are delivered" (ADR-0059):

  ```text
     fire(s)  ⇔  age_flight(s) ≥ 9/8·max(srtt_p, ewma_p)  ∨  delivered_after_p(s) ≥ 3        at every ρ, N ≥ 2
  ```

  The 9/8 and 3 are cited (RFC 9002 §6.1.1–6.1.2; open constants, status §3.4). The ages are measured.
- **Pin.** The `SenderPolicy` digest at ρ = 1. The verdict comes before the hole law (`sender_phases.rs:847–870` precedes `:892`), so a suppressed seq has already been judged. C3 changes only serving, never a verdict.
- **Default change.** Realtime at N ≥ 2 (law instead of age gate), and the gap-report coalescing at every N.
- **Red-first test.** `c03_hole_law_at_every_rho`: ρ = 0.99, N = 2, skewed legs. `mpd_supp_law > 0`, and `retx` against scheduler-created gaps falls against CAND (direction, read after the witness). Red today: `mpd_supp_law = 0`.

### C4: the single-path hole law (`recov_sp`)

- **Today.** `recov_sp = gate ∧ reliable ∧ ¬generation` (`sender_policy.rs:705`), gate off by default.
- **Reason.** None (`982b1a07`, the same text as C3).
- **Shape:** (i), after C12.
- **Formula.** "At N = 1 a gap seq with a live flight fires once the flight is 9/8 of the clock old":

  ```text
     fire(s)  ⇔  age_flight(s) ≥ 9/8·max(srtt, ewma)        at every ρ, N = 1, when the gate is on
  ```
- **Pin.** Gate-off identity at every ρ (the default), plus the digest.
- **Default change:** none.
- **Red-first test.** `c04_recov_sp_scope_reads_no_rho`: with the gate on at ρ = 0.99, the "single-path hole-law suppression ACTIVE" echo is present. Red today.

### C5: emission batching

- **Today.** `emit_batch_on = gate ∧ reliable ∧ ¬coded_wire ∧ ¬use_packing` (`sender_policy.rs:737`), with the gate on by default.
- **Reason. Yes, and now void.** `75cbd012`: "≤ emit_burst symbols per loop iteration, ~64 KB — inside the flow-control store headroom and the cc_pace token bucket, checked per symbol". The headroom check was the ρ = 1-only pause. ADR-0084 A5 puts that pause in the burst loop at every ρ. The Realtime exclusion is `use_packing` (a hint debt, ledger S09), not ρ.
- **Shape:** (i).
- **Formula.** "Batch wherever the pause bounds the burst":

  ```text
     emit_batch_on  =  gate ∧ ¬coded_wire ∧ ¬use_packing
  ```
- **Pin.** The digest at ρ = 1 and at Realtime: Realtime stays excluded by packing.
- **Default change: none at any preset.** It changes only explicit ρ < 1 at Bulk and Auto.
- **Red-first test.** `c05_burst_stops_at_the_pause_below_1`: Bulk hint, ρ = 0.99. Assert `eb_bursts > 0` and `O ≤ B + emit_burst` at every DIAG tick. Red today: no bursts.

### C6: the request arms (owner question)

- **Today.** `request_law_armed(reliable, generation, gate) = reliable ∧ ¬generation ∧ gate` (`sack.rs:35`), read at `mod.rs:1048–1060`. The sender's `request_arm` (`mod.rs:1752`) is the same. Both gates are off by default.
- **Reason.** None for ρ. `b15a8e8c`, and the doc comment at `sack.rs:30`: "generation coding has no per-seq layer to suppress".
- **Why it is (iv).** Arm (A)'s collision seam switches off the per-seq gap producer (`mod.rs:1061–1067`), and the gap reports are the input that ADR-0084's verdict rule judges. At ρ < 1 with arm (A) on, the verdict would see only requests. That is defined only if the request server runs the same verdict. Today it serves blindly. So there are two correct answers:
  - **(a), the default.** The request server runs the per-seq order of ADR-0084 §1.2 (stamp, skip `s ∉ U`, `judge_seq`, serve) on every requested seq, and the scope loses `reliable`:

    ```text
       request_law_armed  =  ¬generation ∧ gate;     serve_request(s)  =  serve_gap(s)   (one per-seq rule)
    ```

    The receiver's l*_recv is measured (`LateGauge::request_lateness`). Under R1 a passed hole leaves the open map (`succ.abandon_below`), so the receiver never requests it.
  - **(b)** Retire the default-off arms (A)/(B). They are experiment arms, 16.83's loop (C) is "NOT BUILT and NOT LICENSED", and nobody runs them.
- **Recommendation: (a).** It is one rule and a short commit. (b) is the owner's to choose, because the arms were his licence.
- **Pin.** Gate-off identity (the default) and the digest.
- **Red-first test.** `c06_request_is_judged_like_a_gap`: ρ = 0.99 with arm (A) on, and a past-D hole requested. Assert `shed_visit_nack` (or the request twin) > 0 and the hole is shed within budget. Red today: the arm is absent below 1.

### C7: the Copa plain feed

- **Today.** `plain_inorder = reliable ∧ ¬generation ∧ ¬coded_only ∧ ¬out_of_order` (`mod.rs:938`). The feed is opt-in.
- **Reason.** None for ρ. `a895205e`: "In-order plain mode ONLY: the OOO/generation modes deliver out of order (the in-order frontier is not their delivery signal)". It names no reason for ρ. The real hazard below 1 is that a frontier jump over a verdicted hole would be credited as delivery. `CopaFeed::newly_delivered` credits every seq in `[cursor, next_expected)` (`copa_feed.rs:66–80`).
- **Shape:** (ii). The correction term is the verdict set, and V = ∅ at ρ = 1, because the budget `(1 − ρ)·S_src = 0` admits no shed and forced verdicts are 0 by data.
- **Formula.** "Credit as delivered only what the receiver delivered":

  ```text
     newly(ack)  =  ( [c, A_rx) ∪ SACK(ack) ) \ attributed \ V
  ```

  - **Symbols.** c is the feed cursor and A_rx the ack's `next_expected`, both measured. V is the sender's verdict set, exact per seq (`G_rx` would only give a count).
  - **Mechanism.** `newly_delivered` runs in `copa_feed_attribute`, on the ack path, where `shed_seqs` is not in reach. So the verdict site calls `feed.on_verdict(s)`, which drops the seq's commitment and marks it attributed. `newly` then skips it.
  - **A late fill of a shed seq is not credited.** That is a conservative under-count, gauged.
  - **Correction (2026-11-21, ADR-0084 Amendment 2).** Every verdict now sends a last copy, so most verdict seqs end delivered. The correction term is the *passed* verdicts, `V ∩ passed` (known from the floor's ack and `G_rx`), or a seq leaves the exclusion when it is filled. Subtracting all of V would under-credit every filled verdict.
- **Pin.** At ρ = 1, `V = ∅` (asserted), so `newly` is today's set.
- **Default change:** none (opt-in).
- **Red-first test.** `c07_feed_never_credits_a_verdict`: ρ = 0.99 with the feed on and the T2 forced-budget scenario. No shed seq appears in the feed's attributed set. Red today: the feed is absent below 1. (2026-11-21, Amendment 2: read "no *passed* verdict seq"; a verdict seq filled by its last copy is credited.)

### C8: the object corners

- **Today.** `config.rs:304` refuses out-of-order, coded-only, generation and systematic when `!window_reliable`. On top of that, the runtime conjuncts `window_reliable ∧ corner` (`mod.rs:632`, `:646`, `:647`, `:1207`; brief A9–A12) are deleted in brief step 2.
- **Reason. Yes, and now void.** "The object-delivery corners presuppose retention (the reliable store is the backstop for every hole they leave behind)" (`config.rs:296–297`, `dacfd7cc`). After ADR-0084 the store exists at every ρ. For generation and systematic the reason was never true: they keep no store at any ρ (`emit_source.rs:347`, `!pol.generation`) and run no per-seq ARQ.
- **At 0.999.**
  - **Generation and systematic: accept.** ρ is inert there. With no NACK site there is no verdict, so `Ab = 0 ≤ (1 − ρ)·S_src`, and the contract holds by construction. Shape (i).
  - **Out-of-order and coded-only: accept, once the floor pass moves the out-of-order frontier.** They receive through `ooo_frontier` over `received_seqs`, not through `ReorderBuffer`, and brief step 8 specifies the floor pass only on `ReorderBuffer`. Shape (iii).
- **Formula.** "The floor passes holes below F on whichever frontier the stream has":

  ```text
     A_rx^ooo  =  min{ s ≥ A_rx^ooo : s ∉ received ∪ [0, F_rx) }     at t_F + H
  ```
- **Pin.** Config tests: accepted at 1 before and after. The generation decision trace at 0.99 equals the one at 1 (no ρ reader on that pipeline).
- **Default change:** none, since the corners are opt-in.
- **Red-first tests.**
  - `c08_generation_at_rho_below_1_is_accepted_and_rho_inert`: red today (refused).
  - `c08_ooo_floor_moves_the_ooo_frontier`: out-of-order at 0.99, T1′'s last-seq-lost scenario; the ack reaches S_src. Red today: refused, and the pass is only on `ReorderBuffer`.

### C9: the reorder buffer

- **Today.**
  - At ρ = 1, `new_reliable()` (`receiver.rs:163–164`): no expiry (`reorder.rs:118`), no force-drain (`:88`), no deadline (`:182`).
  - At ρ < 1, `new(timeout, max)` with `reorder_max_size = 500` (`config.rs:349`): it expires at the hold and force-drains above 500.
  - There is also a third arm: `reorder_timeout_ms == 0` gives no buffer at all (unordered), and is reachable only below 1 (`receiver.rs:165–168`).
- **Reason. Yes, and now void** (§1.1). The ρ = 1 comment gives it: holes are recovered because "the sender retains sent bytes in its ARQ store and retransmits until delivered" (`reorder.rs:21–26`, `4c901536`). Below 1 there was no store, so a forgotten seq could only be timed out locally.
- **Shape:** (i), under R1.
- **Formula.** "The buffer passes a hole only below the newest floor, after the hold":

  ```text
     next_deliver'  =  max(next_deliver, F_rx)    at  t ≥ t_F + H,    delivering the pending entries below F_rx in order
  ```

  - **Symbols.** `F_rx = max` floor heard on any path (`evict-verdict-law.md` §2.3); t_F is its arrival, H the hold (C11).
  - **Inert at ρ = 1 by arithmetic:** no floor is ever sent, because V = ∅ there.
  - **One constructor.** Ordering is chosen only by `window_out_of_order` (the declared ordering dial). `reorder_timeout_ms` and `reorder_max_size` configured the deleted local give-up. They become tombstones that warn when set (the `fec_auto_switch` pattern, `config.rs:315–320`). Default 20 and 500, so a bare config changes nothing beyond R1.
- **Pin.** The `ReorderBuffer` script digest (no floor), equal to `new_reliable`'s today.
- **Default change.** Realtime (§1.2): give-up latency, and the 500-entry force-drain is gone.
- **Red-first test.** `c09_receiver_passes_only_below_a_floor`:
  - the matched Realtime scenario of `sim_mixed_rho` at 0.99;
  - `G_expiry = G_force = 0` (the all-seq count, §2 intro), and `G_floor = G_rx`;
  - the unit tests run the floor pass on the one constructor.
  - Red today: expiry passes.

### C10: a full consumer channel

- **Today.** `WindowDelivery::new(recv_window_reliable)` (`receiver.rs:172`). On `Full` (`delivery.rs:66`) it holds at ρ = 1 and drops below.
- **Reason. The hold has one; the drop has none.**
  - Hold: "Under ρ = 1 the sender discards its only copy once the cumulative ack passes a seq, so a dropped packet would be a permanent loss" (`delivery.rs:20–28`). With a store at every ρ that is true at every ρ.
  - Drop: "ρ < 1 (EVICT) drops the packet, as it always has" (`780be2b1`, which fixed only the ρ = 1 side). Below 1 the drop is an untold, uncounted abandonment, which ADR-0084 decision 1 forbids. It never reaches `G_rx`.
- **Shape:** (i), the hold at every ρ. It is ADR-0084 decision 3's rule, "under overload ρ wins over δ": a full consumer is local overload.
- **Formula.** "A full channel holds, and the ack stays below the lowest held seq":

  ```text
     on Full:  hold;     A_rx  ≤  lowest_held          at every ρ
  ```
- **Pin.** The `delivery.rs` ρ = 1 unit tests, unchanged.
- **Default change and owner confirmation.** A Realtime receiver with a slow consumer backpressures instead of dropping. The alternative, a budgeted drop, needs a receiver budget and so a receiver ρ, which R1 rejects.
- **Red-first test.** `c10_full_channel_holds_below_1`: the existing `reliable_*` tests in `delivery.rs`, run at ρ = 0.99. Red today.

### C11: the receiver shed arm

- **Today.** `recv_shed_on = ¬reliable ∧ ¬ooo ∧ buffer ∧ shed_armed(unified, false, gate)` (`receiver.rs:183`). Below 1 the hold is `SRTT/2` while `holes < ε̂_recv·frontier`, and otherwise the 4·SRTT clamp. At 1 the arm is off.
- **Reason.** None for ρ:
  - ADR-0064 decision 3: "The law is compiled OUT on the reliable (ρ=1) contract". A 0 budget would have done that by formula.
  - The comment: "b(Realtime) = ½ — this path exists only for the Realtime hint" (`receiver.rs:175`). That is why `SRTT/2` is hard-coded (§11.3 "receiver shed hold").
- **Shape:** (i), under R1. With no local give-up there is nothing to arm or budget. `recv_shed_on`, `shed_recv_budget_ok`, `recv_shed_holes`, `REORDER_MIN_HOLD`/`MAX_HOLD` and the legacy arm of `shed_recv_hold` are deleted. One quantity survives: the floor marker's hold.
- **Formula.** "After the floor, hold for the in-flight reorder on the paths":

  ```text
     H  =  min(b(δ_rx), 2) · SRTT_rx
  ```

  - **Symbols.** b(δ) is the declared dial `delta_budget_b` read at the *receiver's* own hint. SRTT_rx is the max over live paths, measured, as today.
  - **Realtime reproduces today's matched hold:** b = ½, so H = `SRTT/2`. This also closes §11.3's "receiver shed hold" divergence.
  - **Inert at ρ = 1:** no floor arrives.
  - *Open, ledger UNDERIVED:* H covers the per-path reorder of seqs below F (§2.3), which is a path quantity, not a δ quantity. Its kind is not derived. This keeps the value ADR-0084 already chose.
- **Pin.** `SHEDH evals = 0` at ρ = 1, now by formula.
- **Default change.** Realtime: R1. The hold value is the same.
- **Red-first test.** `c11_no_local_give_up_below_1`: ρ = 0.99, a hole with entries above it, and the sender silenced by the floor-suppression seam. Assert no pass after any multiple of `SRTT/2` while no floor arrives. Red today: it passes at `SRTT/2`.

### C12: the hole timer

- **Today.** At ρ = 1 the timer re-advertises the gap at `hole_nack_refresh` = `(2·SRTT).clamp(25, 100) ms` (`receiver.rs:734`, `:860`), with arm (A)'s request term. Below 1 the same timer is the hold expiry, and no gap is re-advertised.
- **Reason.** As C9 (`4c901536`): the refresh was added for the side that never gives up.
- **Shape:** (i). The refresh runs at every ρ. The expiry becomes the floor-marker term.
- **Formula.** "Re-advertise a stalled hole on the refresh clock; pass below a floor after H":

  ```text
     deadline  =  min( t_nack + refresh(SRTT),  t_req,  t_F + H )          every term at every ρ; absent = ∞
  ```

  - **Symbols.** `refresh` is an open constant (status §3.4). t_req is arm (A)'s `A_hat + l*`. t_F + H is the marker (C9, C11).
  - **The marker arms the timer even with no pending entry** (brief step 8). That is the lost-tail case.
- **Pin.** At ρ = 1, t_F is absent, so the expression is today's.
- **Default change.** Realtime receivers now re-advertise stalled holes: up to one control datagram per live path per refresh, as Bulk does.
- **Red-first test.** `c12_refresh_runs_below_1`: ρ = 0.99, a hole with an entry above it. Assert a refresh WindowAck (SACK-bearing, `cum_expected = 0`) within `refresh + ε`. Red today.

## 3. The acceptance test for the whole change (replaces T8)

`tests/sim_rho_dial.rs::rho_dial_is_one_machine`.
- **Scenario.**
  - The `sim_lost_tail` scenario: seed 42, N = 1, 6 MB, 2.5 % loss, jitter 4 ms, the `RWM_TEST_TS_DROP=dead` seam on one late seq k for 1.5 s;
  - fed by the paced source `SIM_PACE_MBIT=20`, so the source is not over-driven;
  - at the Bulk hint, so δ is fixed and ρ is the only dial (S_src ≈ 5 017);
  - ρ ∈ {1, 0.9999, 0.999, 0.99}, giving budgets ⌊(1 − ρ)·S_src⌋ = {0, 0, 5, 50};
  - a second cell at the Realtime hint, N = 2 (S_src = 12 147, budgets {0, 1, 12, 121}).
- **Assertions, execution witnesses first:**
  1. **ρ = 1 control.** `client_ok`, `V = ∅`, `floor_sent = 0`, `G_rx = 0`.
  2. **Witness.** At 0.99: `shed ≥ 1` (counted over every seq with `RWM_TEST_TS_SHED_ALL=1`, `test_seams.rs:58`, `:175–179`) and `G_floor ≥ 1`. At 0.999, if the candidates exceed 5, `shed_denied > 0`, so the budget, not an arm, decided.
  3. **Liveness.** At every ρ the ack reaches S_src within T1's bound, and the account closes with `|U| = 0`.
  4. **Only give-ups.**
     - `G_rx ≤ ⌊(1 − ρ)·S_src⌋`.
     - Every pass is a floor pass: `G_expiry = G_force = 0` and `G_floor = G_rx` (the all-seq counts).
     - The delivered set satisfies `|S_all \ D(ρ)| ≤ ⌊(1 − ρ)·S_src⌋`, with `D(1) = S_all`.
  5. **Identity where the budget is 0.** If ⌊(1 − ρ)·S_src⌋ = 0, the sim trace equals the ρ = 1 run's byte for byte (`ChildRun.trace`). After the change, ρ enters only through the budget. The three-term law reads it too, but it is off by default.
  6. **Identity up to the first give-up.** Otherwise, the sim trace equals the ρ = 1 run's up to the virtual time of the first `ev=shed`. Trace lines start with their virtual µs (`sim.rs:280–282`). "Differ only by give-ups" cannot be a whole-trace diff, because the first shed shifts later timing.
- **Red on main.** Expressed through brief step 2's mapping, where ρ < 1 is the evicting seat:
  - at 0.9999, clause 5 fails: the trace differs from the start;
  - clause 3 fails: the lost tail stalls;
  - clause 4 fails: the receiver passes by expiry, not by floor.
- **Lifecycle.**
  - It is written in the first commit, ignored with a `model-rho-step-sites:` reason (the roadmap node `0c1692c2` added; `tools/ci/expected_failure.py` rejects a prefix that is not a node).
  - It is un-ignored in the commit that turns it green, which the expected-failure job enforces. By its cells that is C1 at the latest (commit 9): C6, C7 and C8 are not in them.
  - *2026-11-21 (ADR-0084 Amendment 3, §5 re-cut): with the receiver's old arm kept, clauses 4–6 cannot hold in the sender build. The test is re-prefixed `model-ordering-hold:` and un-ignored in the release-rule build.*

## 4. The reverse pairing

`sim_mixed_rho::mixed_bulk_sender_rho_below_1_receiver_delivers_everything`:
- **Scenario.** A Bulk sender (ρ = 1) to a receiver at ρ = 0.99, same hint (`SIM_SRV_RELIABLE=false` on `adr84-amend`, or `rho` on the server after brief step 2); seed 42, 6 MB, 2.5 % loss, one path.
- **Assertions.**
  - `client_ok`: the perf server needs every chunk, and a ρ = 1 sender promises every chunk;
  - ack = S_src;
  - receiver passes = 0: `[SHED-R] holes=0` before v11, `G_rx = 0` after.
- **Red on `adr84-amend` (`381ab508`), run locally on Windows.** Scratch tests, not committed: seed 42, 6 MB, 2.5 % loss, one path, `RWM_DIAG=1`.
  - **Reverse:** Bulk hint at both ends, server `SIM_SRV_RELIABLE=false`. Result: `outcome=virtual_limit`, `src=5078 ack=5078`, and the receiver's last `[SHED-R]` line reads `holes=72 frontier=3204 budget_open=true`.
  - **What it shows:** the ack reaches S_src, so a test on the ack alone would be green. But the receiver passed at least 72 holes undelivered, and the ρ = 1 sender dropped each one from its store on the cumulative ack. The perf transfer therefore never completes.
  - **So the red assertions are `client_ok` and passes = 0, not the ack.** That is a loss nothing counts, against a sender whose contract forbids it (Q11 in `evict-verdict-law.md` §9: "the contract can still break on the receiver").
- **Green under R1:** no floor, no pass.
- **Isolating the forward pairing.** The existing forward test (`mixed_realtime_sender_bulk_receiver_ack_reaches_src`, ack 49 of 12 147) changes the server's *hint* as well as its retention. The same-hint variant (Realtime at both ends, server `SIM_SRV_RELIABLE=true`, same run as above) reads `outcome=virtual_limit src=12147 ack=49`, the same stall. So retention alone causes it, and the ADR's claim, "each end reads its own retention flag", holds. Add the variant beside the existing test.

## 5. The implementation plan

**Re-cut (owner, 2026-11-21; ADR-0084 Amendment 3).** The sender side is built now, right after S06 merges through VM batch 1; the receiver side lands once, with design U's release rule, after ω is measured (roadmap `model-ordering-hold`). Sender-only R1 is not shipped as an interim Realtime receiver. What this changes below:
- **Stays in this build:** steps 1–8, including the floor and its seat-free pass on both buffers with the marker `(F_rx, t_F + H)` arming the timer (T1 and T2 need it); C2, C5, C12's refresh half, C3, C4, C1, C7 and C6.
- **Moves to the release-rule build:** removing the receiver's own give-up (C9's expiry and force-drain, C11's local arm), the C10 slow-consumer hold, C12's expiry rewrite, the floor-on-raise cadence (§1.2), the two reorder tombstones, and C8: every corner at ρ < 1 would reach the old arm (drop on a full channel for all corners, the evicting buffer for the in-order ones, `receiver.rs:161–172`), so the refusal stays in `check_window_rho` until then. Under U, C9 and C11 keep a release arm keyed by H_o(δ) instead of losing their give-up (`skip-rule-efficiency.md` §4.1).
- **The interim receiver below ρ = 1** is today's arm (expiry at `shed_recv_hold`, force-drain at 500, drop on a full channel) plus the floor pass. It stays keyed on `rho1_debt_bit` of the receiver's own configured `window_rho`, at the sites in Amendment 3's `rho1-debt-exception` block, a named temporary exception the deny test would pin. Amendment 3 is **Proposed**: the exception (Q-a) and H (Q-b) await the owner. Its passes are counted in `G_rx = G_expiry + G_force + G_floor` and so in the sender's Ab, but not told and not bounded by `(1 − ρ)·S_src`; C10's drops are not counted.
- **H** in the floor pass awaits the owner (Amendment 3 item 3, Q-b): (i) `shed_recv_hold` with the expiry's arguments, bit-selected (60 ms at a ρ = 1 receiver, 5 ms at a matched Realtime one, 10 ms RTT); (ii) `SRTT_rx/2` at every receiver, recommended, under which the step-1 bounds still hold; (iii) derive H first.
- **C7 and C6 with the old arm.** An old-arm pass is not in V, so C7's feed over-credits by at most `G_expiry + G_force`; `c07` runs in the R1 cell. C6's premise holds: the expiry path also prunes the open map (`succ.abandon_below`, `receiver.rs:1001`).
- **C2 with the old arm.** Premise 1 reads "received or passed by the old arm or a floor"; the old arm's passes are below F, so the cumulative ack prunes them from S, and the bound against `|S ∩ received|` holds. Under R1 it was tight (`passed ⊆ V`); with the old arm it is only `≤`. Premise 2 holds: the old arm moves the frontier, it never evicts above it. Premise 3's fix (`H_sent`) is unchanged.
- **Tests.** Budget and pass-count assertions run in an R1 cell (sender ρ < 1, receiver ρ = 1, same hint), floor-only by construction; matched cells assert liveness and record the breach (brief step 1). §3's acceptance test and `rho1_debt_is_gone` stay ignored with a `model-ordering-hold:` reason past this build's merge, and §4's reverse test with its `rec-mixed-rho-stall:` reason: with the old arm the ρ < 1 receiver still passes holes a ρ = 1 sender promised.
- **Merge condition of this build:** the ratchet table equals the exception block exactly, and C9–C11 carry their bounding tests (`rho1_debt_bounds.rs`, brief step 9).

The text below is the plan as derived (R1); read it through the re-cut above. In particular, under **Base**: `rho1_debt_bounds.rs` exists for C9–C11; §4 is un-ignored in the release-rule build, not the C9–C12 group; commit 13 is replaced by the sender scaffold commit and the release-rule build.

**Base.**
1. CAND1: `s06-delete` merged up to main (brief §2).
2. ADR-0084, brief steps 1–8, with these changes:
   - **Step 1** also adds §3's acceptance test (`model-rho-step-sites:`) and §4's reverse test (`rec-mixed-rho-stall:`), red and ignored. Each is un-ignored in the commit that turns it green: §4 in the C9–C12 group, and §3 by C1.
   - **Step 2's bit is scaffolding.** `rho1_debt_bit` and its readers exist only inside this stack. Its reader list is the brief's (C) list plus D1, pinned by the deny test's table.
   - **Step 3's ratchet becomes the merge gate.** Each site commit lowers the table. The test `rho1_debt_is_gone` asserts that the table is empty and `net/rho_debt.rs` is absent. It is ignored with a `model-rho-step-sites:` reason, so `expected_failure.py` requires it to fail until the last site commit, which un-ignores it. Merge condition: it runs and passes.
   - **Step 9 is dropped.** There is no `rho1_debt_bounds.rs` and no T8: the debt does not merge. §3 replaces T8. Its bookkeeping moves to commit 13: ledger F02 done, rows G01–G12 and S09, the paper's tense flip, and the `law-ledger/render.py` re-render.
   - **Step 10** (the model's give-up law, `GOLDEN_CAPTURE=1`) stays last.

**Site commits.** One per site, red first, each lowering the table. Each is Sonnet-implementable from its §2 entry. The dependencies set the order:

| order | commit | after | why there |
|---|---|---|---|
| 1 | C2 SACK release | step 6 (store at every ρ) | the step-5 bound reads `\|released\|`. Release needs a store to release from, and its lower bound needs no R1 (§2 C2). T7c inverts here |
| 2 | C5 batching | step 5 (pause in the burst loop) | inert at every preset; cheap |
| — | steps 7, 8 | | verdict, budget, floor |
| 3 | C12 refresh half | step 8 | **this build (Amendment 3).** `deadline = min(t_nack + refresh, t_req, oldest_deadline, t_F + H)`: the refresh at every ρ, the expiry from C9's buffer. Red-first with the ε̂_recv budget spent (brief step 9) |
| 4 | C3 `recov_mp` | C12 refresh | its safety net is the refresh. At Realtime the old expiry still passes first (owner statement, §8) |
| 5 | C4 `recov_sp` | C12 refresh | as C3 |
| 6 | C1 placement | step 6 (path map to `ack + 1`) | biggest Realtime change, so last of the behaviour commits |
| 7 | C7 Copa feed | step 7 (V exists) | opt-in |
| 8 | C6 request arms | step 7, owner answer | gates off |
| 9 | sender scaffold | all above | delete the sender's `rho1_debt_bit` call and readers; D1's echo prints `rho=` at every ρ (with `tools/stage3_parse.py` WIN_LINE updated). The ratchet table equals Amendment 3's exception block. Merge |
| — | **release-rule build** (`model-ordering-hold`) | ω measured, H_o adopted | C9 (release arm by H_o(δ); floor-on-raise cadence; tombstones), C11, C10, C12's expiry, C8; the new H; delete `rho_debt.rs`; un-ignore `rho1_debt_is_gone`, §3 and §4 |

**Guard cut points.** Each guard runs against the previous candidate. Each has a Bulk stage, which must read identical at ρ = 1, and a ρ < 1 stage per Amendment 1 item 8, with execution witnesses before outcomes:

| guard | BASE → CAND | what it attributes | ρ < 1 stage |
|---|---|---|---|
| 1 | main → CAND1 | ADR-0079 (`s06-delete`) | as registered |
| 2 | CAND1 → CAND2 = steps 1–8 + C2 + C5 | ADR-0084 core | Realtime 0.99, N = 1, 2: store fill, pause share, verdict visits, `floor_sent > 0` where a verdict happened, `released > 0`; `floor_passes`, `G_expiry`/`G_force`/`G_floor` and `G_rx` against budget recorded (the old arm may breach) |
| 3 | CAND2 → CAND3 = C12 refresh + C3, C4, C1 | the Realtime N ≥ 2 sender laws | Realtime 0.99, N = 2: split shares, `mpd_supp_law`, retx/repair against CAND2, `G_expiry` against CAND2 |
| 4 | CAND3 → CAND4 = C7, C6, sender scaffold, model law | none at presets (opt-in, config) | short smoke only. The digest carries it |
| later | release-rule build | design U (C9–C12 receiver, C8) | Realtime 0.99, N = 1, 2: frontier-stall p99, no local pass, release-before-fill; both mixed pairings; Bulk long-outage cell |

## 6. Paper text and ledger verdicts

**Paper §11.3.**
- **Rows C1–C12:** each is deleted when its commit lands; the divergence is gone. The commit adds the sentence below to the section the row cites, in present tense, with the commit hash.
- **The "retention at ρ → 1⁻" row** becomes: "a store at every ρ and no ρ comparison anywhere in the engine (deny test). At ⌊(1 − ρ)·S_src⌋ = 0 the run is identical to ρ = 1 (`sim_rho_dial`)".
- **The "receiver shed hold" row** is deleted with C11.

| site | sentence for the paper | section | ledger row: verdict after |
|---|---|---|---|
| C1 | The placement law runs at every ρ; there is no best-path pick. | §5.7 | G01: OK |
| C2 | SACK release runs at every ρ; `released ⊆ S ∩ (received ∪ passed)`, tight under the floor-only receiver. | §6.3 | G02: OK |
| C3 | The per-flight hole law runs at every ρ; its suppression is safe because the receiver's refresh runs at every ρ. | §7.1 | G03: OK |
| C4 | As C3 at N = 1, behind its gate. | §7.1 | G04: OK |
| C5 | Batching runs wherever the pause bounds the burst, which is every ρ; Realtime stays excluded by packing (S09). | §6.5 | S09: the ρ part OK; the hint part stays MODE-SWITCH(live) |
| C6 | per owner answer (a): a requested seq is judged by the same per-seq rule as a reported gap. | §7.6 | G06: OK under (a) |
| C7 | The plain Copa feed credits delivery net of the verdict set. | §8.2 | G07: OK |
| C8 | The object corners are defined at every ρ; generation is ρ-inert; the floor moves the out-of-order frontier. | §5.8, §5.1 | G08: OK |
| C9 | The receiver passes a hole only below the newest floor, after H; it has no give-up law of its own. | §5.1, §5.6 | G09: OK |
| C10 | A full consumer channel holds at every ρ. | §5.1 | G10: OK |
| C11 | The floor marker's hold is `H = min(b(δ_rx), 2)·SRTT_rx`. | §5.6 | G11: OK for the arm; UNDERIVED for H's kind (reorder cover against δ) |
| C12 | The hole timer re-advertises at every ρ; the floor marker is its second term. | §5.1 | G12: OK |

While the stack is open, every row reads `MODE-SWITCH(scaffold)` with its commit pending. None merges in that state.

## 7. Amendment 1 rewrite

*2026-11-21: superseded in part by ADR-0084 Amendment 3. "The bit never reaches main" and "no exception" hold for the sender only; the receiver's rows ship as a named temporary exception until the release-rule build.*

Proposed replacement for item 2's exception and enforcement (b), and for item 5 (below).

These other parts go stale and are rewritten in the same amendment commit:
- **ADR-0084 decision 7**, "SACK release stays reliable-only". It is an accepted decision, so it is not edited. The amendment supersedes it: release runs at every ρ (C2).
- **The Consequences bullet** "A step at ρ = 1 remains on the receiver …". No step remains; R1.
- **Item 4:**
  - its table's "what makes it continuous" column becomes a pointer to this document;
  - its paragraph "C2 and the rate bound" and its "Correction to decision 2" become: the sender-side step at ρ = 1 is gone, for all of C1–C7.
- **Item 7**, the bullet "(C) takes the ρ = 1 arm through the bit". There is no bit, and every site has one arm.
- **Item 8:** the guard plan becomes §5's five guards.
- **Item 2 (a)'s allowlist:** after C8, `check_window_rho` holds the domain check only, with no corner refusal. The allowlist is then `check_window_rho` alone; `rho1_debt_bit` leaves at commit 13.
- **The Consequences bullet** "23 counted `rho1_debt` reader lines": zero at merge.
- **The engine brief:** §1 group (C) points here; §3 "SACK release stays on the ρ = 1 arm" and "the receiver's give-up budget stays ε̂_recv" are deleted; §4 steps 3 and 9 change as in §5 above.

> **2. The trap rule.** In engine code, nothing compares, matches or thresholds the retention ρ to select a code path, a law, a constructor or an argument. ρ enters only as a number in a formula continuous in ρ. Today that formula is the shed budget `(1 − ρ)·S_src`, plus the opt-in three-term law. There is no exception.
>
> **Scaffolding during the build.** While the engine change is being built, `rho1_debt_bit` may exist on the branch so that each of the 12 sites (item 4) can be rewritten in its own commit. Its readers are pinned by the deny test's table, which may only fall. The test `rho1_debt_is_gone` (table empty, `net/rho_debt.rs` absent) is the merge gate: the change merges only with it running and green. The bit never reaches main.
>
> **Enforcement** (a) and (c) are as before. (b): the ratchet counts `rho1_debt` identifiers against the table during the build, and its final state is zero. The `rho1-debt-readers` block is deleted.
>
> **4.** The (C) list is rewritten, not grandfathered. Each site's formula and test are in `docs/derivations/rho-step-sites.md`.
>
> **5. The receiver's ρ: none.** The receiver passes a hole only below the sender's floor, after the hold H. It keeps no give-up law, no budget and no ρ. The one abandonment decider is the sender's verdict against `(1 − ρ)·S_src`, and the floor tells every verdict. The floor is also sent at the end of every gap pass that raised it (reversing `evict-verdict-law.md` question 6, whose premise, the receiver's own hold, is gone). Both mixed pairings are tested (`sim_mixed_rho`).

## 8. For the owner

- **(iv) C6, the request arms at ρ < 1.**
  - (a) the request server judges each requested seq by the same verdict rule;
  - (b) retire the default-off arms.
  - **Recommended: (a).**
- **Confirm the receiver law R1, floor-only.** The receiver passes only on the sender's floor and keeps no ρ. Cost: Realtime give-up latency goes from about `owd + SRTT/2` to about `RTT + owd + H`. Guard 3 measures it.
- **Confirm C10's hold at Realtime.** A slow consumer backpressures instead of dropping. A budgeted drop would need a receiver ρ.
- **Confirm the floor-cadence reversal.** This reverses the owner-accepted default of `evict-verdict-law.md` question 6, "sweep clock only". The floor is also sent at the end of every gap pass that raised it, because R1 removes that default's premise (holes with data above pass on the receiver's own hold).
- **Confirm the serialize cost of R1** (§1.2). With the budget spent, a Realtime hole waits for its fill instead of passing at 60–300 ms.
- **Realtime N ≥ 2 changes (C1, C3).** These are by the paper's own law, but they change the Realtime default. Guard 4 measures them.
- **Bulk and Auto:** no default changes from the twelve sites. The owner has accepted one later Bulk change, which ships only with design U's release rule (see the owner answers below).

**Open, not owner calls:**
- the receiver's reorder memory under SACK release has no derived bound at ρ = 1. R1 gives ρ < 1 the same bound, and a derivation is owed at both;
- H's kind, a reorder cover or δ (C11, UNDERIVED); the floor pass's H is now being derived first (owner, Q-b (iii); `docs/derivations/floor-hold.md`, roadmap `model-floor-hold`);
- the Realtime duplicate send and `use_packing` stay hint debts (status §3.3); C1 changes the duplicate send's alt path.

### Owner answers (2026-11-21)

**Decided:**
- **C6: option (a).** The request server judges each requested seq by the same per-seq verdict rule as a gap; the scope loses `reliable`.
- **Realtime N ≥ 2 changes: accepted.** C1 (the §5.7 placement law replaces the best-path pick), C3 (the per-flight hole law replaces the age gate) and the gap-report coalescing at every N. `ordering-hold.md` §5 couples C1 to design U: with U, every slow-path packet is released past and reordered unless ω is large. That coupling stands and is read with the ω measurement.
- **Design U, not RX** (`skip-rule-efficiency.md` §4.1, `ordering-hold.md`). A late out-of-order fill counts as delivered (O3 = yes). RX (§4.3 there) is not pursued, so O4 is moot.
- **The cumulative count (O2): accepted,** with no run-length window. The largest single-episode spend is gauged.
- **Bulk after a long outage.** Late, out-of-order delivery instead of a stall is acceptable once the release rule ships. That is the first Bulk default change, and it waits on the rule. No Bulk or Auto default changes now. Reordering at Bulk or Auto in normal operation (a small measured ω) is not decided: ω is measured on a TCP-in-tunnel test and its result is followed (roadmap `meas-disorder-price`).
- **ADR-0084 Amendment 2:** every verdict sends a last copy with the floor; the sender merges acks by pair. C7's correction above follows from it.

**Not decided (open):**
- the receiver law R1 (floor-only) as final. Sender-only R1 is the staging point only;
- the floor-cadence reversal;
- the C10 hold at Realtime;
- the serialize cost of R1 (§1.2), and O1.

**Build order: sender side first (owner, 2026-11-21: yes; ADR-0084 Amendment 3).** The sender side of the ADR-0084 fix is built right after S06 merges through VM batch 1, without waiting for ω. The receiver side lands once, with design U's release rule, after ω is measured. Sender-only R1 is not shipped as an interim Realtime receiver. §5's re-cut gives the split. Stated for the owner: **Realtime latency is unchanged in the interim only because the receiver's old arm stays, so the 1 % promise is still not kept at Realtime until the release rule lands.** The old arm's passes are counted (in `G_rx`, and so in Ab and the `[BUDGET]` breach line) but not told and not bounded, and they spend the sender's allowance first; C10's full-channel drops are not counted at all. A ρ = 1 sender to a ρ < 1 receiver still loses packets (§4). Two small Realtime changes do land in the interim: C12's refresh re-advertises a stalled hole once the receiver's ε̂_recv budget is spent, or at SRTT ≥ 200 ms, and C3's suppression can delay a resend past the old expiry at N ≥ 2. **Two owner questions followed (Amendment 3), answered 2026-11-21:** Q-a **accepted**: the receiver's old arm is a named, temporary ρ-keyed exception the deny test pins (sites in Amendment 3, removal at the release-rule build), with its fallback (R1 lands if the release-rule build is not in within 8 weeks of the ω Result, or that Result is `OMEGA-VARIES-*` or `RENT-UNDERPRICED-*`, unless the owner re-decides); the release-rule build removes the exception list and the fallback clause together, so the fallback expires with the exception. Q-b answered **(iii), derive H first** (reviewed with Fable): the build's floor-pass step waits on the H derivation (`docs/derivations/floor-hold.md`, roadmap `model-floor-hold`); (i) as is, bit-selected, and (ii) `SRTT_rx/2` at every receiver are not adopted. C8 (object corners below ρ = 1) also waits for the release-rule build.

**C1 stays in the sender build (owner, 2026-11-21: option (a)).** Asked after the guard registration predicted it: with C1 in the first build and the receiver's old arm still in place, Realtime at N ≥ 2 frontier delay rises by up to the path skew (about 20 ms in the guard's skew cell) until the release rule lands. The owner keeps C1 here and accepts that interim rise, rather than moving C1 to the release-rule build, which would add three ρ-keyed sender rows (`emit_source.rs emit_source`, `sender_phases.rs serve_gaps`, the sender's `rho1_debt_bit` call in `run_impl`) to the closed exception list. The guard measures the rise and reports it in ms (`adr84-guard-registration.md` §5.4, XK). C3 stays in this build too.

**What waits.** The receiver sites C9, C11 and C12 get their release arm once, together with the release rule H_o(δ), after ω is measured. Run-atomic spending, the time threshold T_ρ, the release arm, the new H and retiring `window_out_of_order` wait on the toy simulation or the measurement, and none is decided.
