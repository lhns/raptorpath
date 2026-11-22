# The lost tail and the floor hold: two owner objections (derivation, no code)

**Objections (owner, 2026-11-22).**
1. **"Fix the lost tail first."** Limit L-TAIL (`adr84-guard-registration.md` §3.1, §5.5b): at a matched ρ < 1 pair in the sender-first build, the receiver's old arm passes holes on its own; those passes enter `G_rx` and so the sender's allowance `Ab`; once they reach `(1 − ρ)·S_src`, every sender verdict is refused, and a lost tail waits for a resend to get through, with no time bound. The owner wants this fixed before the sender build ships.
2. **"This fixed delay sounds completely wrong."** The floor hold H of `floor-hold.md`: the windowed maximum over W = 10 s of RFC 4737's late time offset. W was borrowed by analogy, not derived.

**Short answer.**
- **Objection 1: count only the sender's own floor passes against the allowance (option (c), §1.4).** The receiver already splits `G_rx` by writer (brief step 4). One more `WindowAck` field, `passed_by_floor` (8 bytes, riding v11, which is not built yet), lets the sender run its budget on `G_floor`. The old arm's passes stay on the wire in `passed_undelivered`, printed against the budget, as today. Realtime latency does not change, no ρ-keyed site is added, and Amendment 3's order stands. L-TAIL then reduces to the ρ contract (§1.1) in this build, as it does under design U. The same operand stays correct after U, where `G_floor ≡ G_rx`.
- **Objection 2: replace the statistical hold with an event rule (§2.3).** On a FIFO path, any datagram sent after a copy arrives after that copy. So the receiver needs no wait for a path that keeps delivering. It only needs to know, per path, when the last copy of a covered hole was sent, and that is a sender fact. With that on the floor (one stamp per path), the receiver passes as soon as every copy-carrying path has delivered something sent later. A wait remains only for a path that falls silent (dead or stalled). No event can tell "slow" from "dead" there, so that wait cannot be removed. Its bound is the path's own loss timeout, `srtt_p + 4·rttvar_p` (RFC 9002 §6.2.1 / RFC 6298 form), anchored on send stamps, so it needs no clock offset. W, the record deque and the 10 s memory go. H = 0 at N = 1 as before. At N = 2 the wait is 0 unless a copy rode the slower path just before the floor, and it is never longer than what that copy itself needs.
- **Records (§3).**
  - The age half of the verdict is degenerate at N = 1 at Realtime and Auto in the simulation. In the engine, whether it refuses depends on how far the EWMA RTT sits above the report's round trip; the refusal condition is in §3.1.
  - D's estimator (EWMA against RTprop) is open, and so is its two-path input: the engine reads the highest-loss path, the paper the largest RTprop. A bounding test is proposed.

**Inputs read** (`github/main` `344f3510`; `raptorpath/src` read with `git grep` only):
- ADR-0084 and Amendments 1–3;
- `adr84-engine-brief.md` (all);
- `rho-step-sites.md` §0, §1, §2 (C9–C12), §4, §5 and §8;
- `floor-hold.md` (all);
- `ordering-hold.md` (all);
- `skip-rule-efficiency.md` §1–§4;
- paper `docs/fec-arq-model.md` §5.1 and §5.6;
- branch `adr84-guard-registration` `a727535a`, `adr84-guard-registration.md` §2, §3.1, §4, §5.4–§5.5b, §8, §10–§12;
- `tools/toy/results-stage1.md` and roadmap node `model-toy-stage1`.

Code:
- `net/emit_source.rs:505` (the one `retransmit_buffer.insert`, at first send, stamped `src_send_ts_us`);
- `:575` (`worst_eps_estimator`), `:617–634` (D refreshed under `pol.shed_on`, per source emission only);
- `:761–772` (the slot site's age; S06 deletes it);
- `net/sender_phases.rs:849–870` (the NACK site's age, the same stamp; this site survives);
- `net/shed.rs:21–77`, `:389`;
- `net/channel_set.rs:37–73` (`worst_eps_channel_path`, ties to the last maximum; `p_lost_inputs` with `rttvar = 0.1·srtt`);
- `control/estimator.rs:523–531` (`rtt()` is `ewma_rtt`, which may still be the 50 ms seed);
- `net/receiver.rs:697`, `:1740` (the receiver's SRTT is the maximum over live paths), `:775–795`, `:1750–1765` (the old arm's hold);
- `net/store_cap.rs:299` and `raptorpath-math/src/lib.rs:183` (`b(δ)`: 0.5 at Realtime, 1 at Auto, 2 at Bulk);
- `net/mod.rs:236` (`GAP_ACK_MIN_INTERVAL` = 2 ms), `:498` (`DEAD_PATH_TIMEOUT` = 6 s);
- `transport/protocol.rs:105` (`PROTOCOL_VERSION = 10`: v11 is not built);
- `scheduler/mod.rs:53` (`PathId = u32`).

No ADR-0084 engine branch exists on `github` (only `adr84-amend` and `adr84-guard-registration`), so every wire change below can still ride v11.

**Words.** *Tail*: the source seqs at the end of a transfer with nothing sent above them. *Allowance*: `A(t) = ⌊(1 − ρ)·S_src(t)⌋ − Ab(t)` (skip-rule §2). *Round*: one serve-and-report cycle, about RTT plus the sampler interval, or the tail-sweep period `clamp(2·SRTT, 25, 100) ms` for a tail.

## 1. Objection 1: the lost tail

### 1.1 The frame: what L-TAIL is and what no design can remove

- **A tail longer than the remaining allowance waits for its fills in every design.** That is the ρ contract: deliver at least ρ of every prefix. Brief T1 shows it: a 347-seq tail against an allowance of ⌊0.01·12 147⌋ = 121 gets at most 121 verdicts, and the rest are served until filled. A tail of up to B = N·2048 seqs fits only once `S_src ≥ B/(1 − ρ)`, which is 204 800 source symbols at N = 1 and ρ = 0.99.
- **A dead link is not the case to fix.** A floor crosses a link exactly as a resend does. On a link that delivers nothing, no design passes the tail; when the link returns, the tail resolves in one round either way (by fill, or by floor). The sim's `RWM_TEST_TS_DROP=dead` seam is different: it kills every copy of one seq while the link carries everything else. "Forever" holds only under that seam, or a seam without end. It is a test adversary, not a link.
- **On a live link the cost of L-TAIL is extra serve rounds, not forever.** A served tail seq fails each round with probability ε_c, the loss of one copy:

```text
   E[extra rounds]  =  ε_c / (1 − ε_c)              P(tail open after k rounds)  =  ε_c^k
```

  A verdict sends one last copy and the floor together (Amendment 2), so it resolves in the first round whatever the copy's fate. The verdict therefore saves only the rounds after the first: at XE (ε ≈ 10 %, a 100 ms round), about 11 ms in mean and 2 rounds (≈ 0.2 s) at p99 under independent loss. GE bursts raise ε_c inside a bad state. XL measures that tail (registration §5.5b).
- **So the defect in L-TAIL is not the wait itself.** It is *whose* spending makes the sender refuse. In the sender-first build, the receiver's old arm, an uncounted, untold, ρ-keyed exception (Amendment 3), spends the sender's allowance first. Under any one-decider design the allowance is spent only by the sender's own verdicts. The fix is judged by whether it reaches that state.

### 1.2 Option (a): Amendment 3's fallback now (floor-only R1)

**In plain words.** The receiver drops its own give-up and passes a hole only below the sender's floor. Only the sender's verdicts spend the allowance.

**What changes at the receiver** (`rho-step-sites.md` §2):
- **C9:** one reorder-buffer constructor. No expiry and no force-drain at 500. The floor pass is the only frontier writer besides delivery. `reorder_timeout_ms` and `reorder_max_size` become tombstones.
- **C10:** a full consumer channel holds at every ρ (today it drops below 1, uncounted).
- **C11:** `recv_shed_on`, `shed_recv_budget_ok`, `recv_shed_holes` and `shed_recv_hold`'s legacy arm are deleted.
- **C12:** the expiry term leaves the deadline; it becomes `min(t_nack + refresh, t_req, marker)`.
- **Also:**
  - `rho1_debt_bit` and every row of Amendment 3's exception block go, except C8's allowlisted refusal;
  - `rho1_debt_is_gone` is un-ignored;
  - the reverse pairing (`rec-mixed-rho-stall`) turns green: a ρ < 1 receiver no longer passes holes that a ρ = 1 sender promised.

**Latency, isolated hole, priced term by term** (not as one sum):
- **Today:** pass at `t_buf(next entry) + h_rx`. h_rx is `SRTT_rx/2` with `SRTT_rx` the maximum over live paths (`receiver.rs:697`) while the receiver's ε̂_recv budget is open, and `clamp(4·SRTT_rx, 60, 300) ms` once it is spent.
- **Under R1**, the hole resolves at the first copy's fill, about `t_detect + s + RTT_r`:
  - s ≤ 2 ms is the gap-report sampler;
  - RTT_r is the report's leg plus the copy's leg.
- **The age gate adds nothing when the copy arrives.** A hole refused by `age ≤ D` is *served*, so its resend leaves at the same report. D delays resolution only when that resend is lost: then the verdict comes one report later.
- **The tell adds nothing when the copy arrives.** The first floor leaves with the verdict's copy (Amendment 2 decision 1), so it arrives one owd after the verdict. The sweep clock (25–100 ms) governs only its *repeats*, after a lost floor datagram. Brief step 8's "floor cadence: tail-sweep clock only" must say that; it is a wording fix, owed.
- **H adds** 0 at N = 1, and about the skew at N = 2 under `floor-hold.md` (§2 below changes that).
- **The one new cost: a spent budget serializes.** A refused hole is served until filled: `ε_c/(1 − ε_c)` extra rounds in mean, against today's old-arm pass at 60–300 ms. The allowance is spent at about `ε_c·ĥ` per source symbol, where ĥ is the hole rate after FEC.

| cell | today: loss → pass | R1: loss → fill (copy arrives, 1 − ε_c) | R1: copy also lost | spend `ε_c·ĥ` vs 1 − ρ = 1 % |
|---|---|---|---|---|
| c2 (≈ 10 ms RTT, ε ≈ 2.6 %) | `SRTT/2` ≈ 5 ms | ≈ 10–12 ms (**+5–7 ms**) | ≈ 10–12 ms + H (floor pass, H = 0) | ≈ 0.026·ĥ: well under 1 % for any ĥ ≤ 38 % |
| c8 (5 / 20 ms one way, 2.7 / 4.1 %) | `SRTT_rx/2` = max-path 40 ms / 2 ≈ 20 ms | fast-leg loss: resend on the report, ≈ 12–17 ms (**−3 to −8 ms**); slow-leg loss: ≈ 25–32 ms once C3's hole law ripens (**+5 to +12 ms**) | + H (≈ 15 ms skew under `floor-hold.md`; 0–15 ms under §2.3) | ≈ 0.03–0.04·ĥ: under 1 % |
| XE `c2r100l10` (100 ms RTT, ε ≈ 10 %, GE) | `SRTT/2` ≈ 50 ms (budget open) | ≈ 100–102 ms (**+50 ms**) | + 0 (N = 1) | ≈ 0.1·ĥ, higher in GE bursts: **at the edge** (ĥ ≳ 10 % reaches 1 %), so R1 may serialize mid-stream here, rounds of 100 ms |

The c8 figures are first-order estimates from the cell's declared delays; the engine's D there reads the c3 leg's EWMA RTT (§3.2), so D ≈ 20 ms.

**Cost.**
- **Build:** four or five site commits (C9, C10, C11, C12's expiry, the tombstones), each already specified in `rho-step-sites.md` §2.
- **VM:** the guard registration is re-cut, not grown much.
  - The matched cells become floor-only. R-ONLY, R-BI and R-H then apply in X1, X2, XK and XE, and the frontier delay and stall at XE become the owner's latency readout.
  - The reverse pairing gains a cell, about 30 rows or 30 min.
  - XL becomes a scored gate.
  - Estimate: +0.5–1 h on the registered ≈ 11.4 h, plus a re-registration and a rehearsal re-run (≈ 1.3 h).
- **Later:** the release-rule build changes the Realtime receiver a second time.

**Against the owner's decisions.** (a) is exactly what the owner declined twice on 2026-11-21. Sender-only R1 is not an interim Realtime receiver ("rewriting C9–C12 twice … would ship a Realtime receiver whose latency changes twice"), and Amendment 3 Q-a accepted the exception with this fallback, timed to the ω measurement. (a) reverses Amendment 3's order.

### 1.3 Option (b): the full receiver build (design U, release `H_o = ω·d/δ`)

**When L-TAIL goes.** With the release-rule build (`model-ordering-hold`), after `meas-disorder-price`'s Result and at most 8 weeks after it (Amendment 3's fallback clause). Until then L-TAIL is on main.

**Does the receiver-side build remove it? The claim checked.** Under U the receiver's release moves only its *delivery* frontier. The ack frontier A_rx moves only on fills and floor passes, so `G_rx = G_floor` and the allowance is spent only by the sender's own verdicts. A tail has no arrival above it, so `t_detect` is undefined and H_o never fires: **the tail behaves exactly as under R1.** So once the sender has spent `(1 − ρ)·S_src` on its own verdicts, the tail waits for its fills in every design, and that is the contract (§1.1). The claim holds for the defect. **It does not hold as "nothing remains".** Three things remain, none of them the other decider:
1. **First-come spending.** The allowance goes to the first past-D holes, including a lower part of a long open run (skip-rule F2). Run-atomic spending and T_ρ were dropped by toy stage 1 (`model-toy-stage1`), and the owner accepted the cumulative, first-come count (O2). So "legitimately given up" means first-come. That is the accepted contract, not a residual defect, but the owner should know that the tail competes with every earlier hole on equal terms.
2. **The pending term over-counts above a stuck hole.** `Ab = G_rx + |{v ∈ V : v ≥ A_rx}|`. A verdict seq that was *filled* still counts while a lower refused hole holds A_rx below it. At the tail that can refuse the tail's verdict while the true abandonment is under budget. It lasts until the lower hole fills, which is geometric in ε_c rounds. A cheap fix needs no wire: subtract the verdict seqs the sender already knows are received (SACKed, C2 runs at every ρ):

   ```text
      Ab  =  G_floor  +  |{ v ∈ V : v ≥ A_rx,  v ∉ SACKed }|
   ```

   Every symbol is already at the sender. This is recommended in §1.6 as an owed refinement, not part of Amendment 4.
3. **Floor-pass misses.** A fill that arrives after its floor pass holds one unit until the ack carries the fill (`fh_miss`). This is transient, and gauged.

The allowance is 0 below 1/(1 − ρ) = 100 source symbols, which is the contract too.

So L-TAIL under U is **the contract case plus a bounded over-count (item 2)**. That is not exactly the contract case, but nothing remains that another decider spends.

### 1.4 Option (c): count only the sender's own give-ups against the allowance

**In plain words.** The sender budgets only the holes its floor passed. The old arm's passes stay uncounted, as they already effectively are at Realtime, where the 1 % promise is not kept in the interim (Amendment 3 Consequences).

**Does v11 carry enough? No.** v11 as accepted (decision 4) carries one level, `WindowAck.passed_undelivered = G_rx = G_expiry + G_force + G_floor`. The split exists only at the receiver (brief step 4) and on the test-seams `[ACCT-RX]` line. The sender cannot separate G_floor from the sum.

**Wire change.** One field, `WindowAck.passed_by_floor: u64` = G_floor, the holes passed by the floor pass net of their own late fills. It rides the same v11 bump (no v12), because v11 is not built. Amendment 4 amends decision 4.

**The operand** (amends decision 5):

```text
   Ab_c   =  G_floor  +  |{ v ∈ V : v ≥ A_rx }|                 verdict  ⇔  age > D  ∧  Ab_c + 1  ≤  (1 − ρ)·S_src
   printed:  G_rx  against  (1 − ρ)·S_src                        ([BUDGET] breach=, unchanged)
```

| symbol | provenance |
|---|---|
| G_floor | measured at the receiver: frontier advances by the floor pass minus delivered, minus late fills of holes that writer passed |
| G_rx | measured, as v11 accepted |
| V, A_rx, S_src, ρ, D | as ADR-0084 decision 1 and paper §5.6 |

**Why this is the right operand, not a carve-out.** A sender verdict on s can lose s only if the receiver would otherwise have waited for s.
- A hole the old arm passed is dropped from the sender's service by the cumulative ack whether or not a verdict was taken: Amendment 3 item 4 books it as acked. So a verdict on such a hole costs nothing that the old arm did not already cost.
- G_floor is exactly the abandonment the sender's verdicts caused.
- Under U, and under R1, the floor pass is the only abandoning frontier writer, so `G_floor ≡ G_rx`: the operand means the same before and after the release rule. The field then becomes an identity gauge, `G_rx − G_floor = 0`, which is the routing witness that no other writer exists. A later version may drop it.

**The points that must hold:**
- **Attribution.** A verdict seq the old arm passes before its floor lands in G_expiry, not in G_floor, so Ab_c does not count it. That causes no extra loss: the sends for that hole are the same with or without the admission. A refused verdict is served once at that report; an admitted one sends its last copy once; and the old arm's pass then ends service either way.
- **Late fills** subtract from the writer that passed the hole. The receiver keeps the writer per passed hole (one tag in `abandoned_seqs`). A fill after the decode-floor prune is gauged, not subtracted (brief step 4, unchanged).
- **The pair merge** (amends Amendment 2 decision 3): keep `(next_expected, G_rx, G_floor)` from the ack with the greatest `next_expected`; on a tie, the larger G_floor, then the larger G_rx. At a fixed frontier both levels only fall (by late fills), and every pass moves the frontier. So the tie rule picks the older, more conservative ack in both components at once.
- **Price: total abandonment against the Amendment 3 interim.** Before (c), once G_expiry exceeded the budget, no verdict was admitted, so G_floor stopped growing. Under (c), G_floor grows up to the budget on its own. So:

  ```text
     G_rx(c)  −  G_rx(A3)   ≤   G_floor(c)   ≤   (1 − ρ)·S_src
  ```

  The extra comes only from holes the old arm does *not* pass first:
  - lost tails, which is the point of the fix;
  - holes whose old-arm hold has switched to the 60–300 ms clamp (ε̂_recv budget spent), where the floor, at about RTT + H, now comes first;
  - at N ≥ 2, holes whose floor beats `SRTT_rx/2`.

  In each of these the pass comes *earlier* than under Amendment 3, never later, because the old arm still runs and the earlier writer wins. So (c) cannot raise Realtime latency.
- **NO-MODE-SWITCH.** No dial is read, and no comparison or branch is added. The operand is one counter at every ρ, δ and hint. At ρ = 1 the budget is 0 and nothing is admitted. No `rho1_debt` identifier is touched: the exception block of Amendment 3 is unchanged, and the deny test's ratchet is unaffected. The old arm itself stays the accepted, named, ρ-keyed exception, and (c) neither widens nor relies on it beyond what it is.
- **The 1 % promise.** At a matched Realtime pair it is still not kept in the interim, exactly as Amendment 3 states, and the breach is still printed. What (c) adds is that the sender's own part of the promise, `G_floor ≤ (1 − ρ)·S_src`, now holds at every prefix in every cell. The registration's clause BI already scores exactly this (§5.4: "`G_floor ≤ budget` … the only share that the sender's verdict decides").

**L-TAIL under (c).** The old arm can no longer exhaust the sender's allowance. A tail is refused only when the sender's own floor passes plus its pending verdicts have reached `(1 − ρ)·S_src`. That is §1.3's state (the contract case plus the pending over-count), reached in this build and not months later.

**Cost.**
- **Build:**
  - one field in the v11 codec, written beside `passed_undelivered` in brief step 4;
  - the writer tag on passed holes, which step 4 already owes per writer;
  - `abandon_count(g_floor, pending)` in step 7;
  - the three-component pair merge, with its test extended.
- **VM: none added.** The same cells run, but the registration's predictions are re-registered (§5):
  - §5.4's admitted-verdict table: the matched cells now admit verdicts at roughly XR's rate, so W4 and W6 become witnessable there;
  - EX is redefined on G_floor, so XE is predicted `XE-NOT-EXHAUSTED`, and that state must no longer hold the merge;
  - XL's prediction becomes "the tail is passed by its floor within one round when its copy is lost", and XL can become a scored gate;
  - §3.1's L-TAIL record turns into a test with a forced budget seam (the old arm can no longer pre-spend the allowance).
- **Wire:** 8 bytes per `WindowAck`.

**Variant (c1), not recommended:** redefine `passed_undelivered` as G_floor and add no field. The sender could then no longer print the total against the budget (the receiver knows neither S_src nor the sender's ρ), so decision 3's "the breach … is printed" would be lost at the sender.

### 1.5 Other options weighed

- **Exempt the tail** (a verdict on a seq with nothing sent above it spends nothing). That is a branch on a seq's position, and it breaks the contract it exempts. Rejected.
- **The sender counts every verdict, `Ab = |V|`.** It needs no wire field, but it gives up Amendment 2's saving: it spends about ĥ instead of ε·ĥ, 40× more in the skip-rule example. It also still mixes in verdicts the old arm made moot. Rejected.
- **Report per-seq pass and fill information** (the toy's observer channel). That needs an unbounded wire field. (c) needs one level.
- **Bound the old arm by the sender's ρ** (R2, `rho-step-sites.md` §1). That needs ρ in the handshake and two deciders on one budget. It was rejected there, and it is not simpler.

### 1.6 Recommendation for objection 1

**Option (c), with the SACK-aware pending term (§1.3 item 2) as an owed refinement.**
- **Price:** one 8-byte ack field riding v11; no latency change (a pass can only come earlier); no VM time added; total interim abandonment up by at most `(1 − ρ)·S_src`, and only on holes the old arm did not pass first.
- **Effect on earlier decisions:**
  - Amendment 3's order stands: no reversal, the receiver still lands once, with U;
  - decisions 4 and 5 are amended (one field; the operand);
  - Amendment 2 decision 3 is amended (the pair becomes a triple);
  - Amendment 3 item 4's "they spend the sender's allowance first" is superseded;
  - the fallback clause is unchanged.

Option (a) is the right fallback if the owner prefers to keep the 1 % promise *in total* at Realtime now, at the latency of §1.2.

## 2. Objection 2: the floor hold H

### 2.1 What is fixed in the current H

H is not a fixed delay: it is a measured statistic, 0 on a FIFO single path and about the skew at N = 2. Two things in it are fixed, though, and the objection lands on both:
- **W = 10 s**, borrowed from Copa's minimum-RTT window, a different role;
- **its kind:** every floor pass waits the worst lateness seen in the last 10 s, whether or not anything covered by *this* floor can still be in flight.

The second is the deeper fault. H asks "how late has any packet been", when the question is "can a fill of *these* holes still arrive".

### 2.2 Is any wait needed? Per-path order

**The premise** (the same premise as `floor-hold.md` §1, H = 0 at N = 1): each path is FIFO from staging to the receiver loop, and the floor on a path is staged after every copy staged before it on that path (`io_owner.rs:1499–1530`; brief step 8).

**What per-path order gives:**
- A copy of s sent on path p at τ_c has arrived or is lost once the receiver has taken from p any datagram staged after it.
- So a hole s below the floor F can still be filled only by a copy on a path that has not yet delivered anything sent after that copy.
- If every path that carried a copy of s has done so, s can be passed at once, with no wait.

**What the receiver cannot know alone:**
- It cannot know which paths carried a copy of s (the original's path, the resends' paths, the last copy's path);
- nor when each copy was sent: a lost copy never arrives, so its stamp is never seen.

These are sender facts. The sender knows them: the path map holds originals to `ack + 1` (A7), and every resend chooses its path at the serve site.

**Why floor-hold's "the last copy's path" was not enough, and what is.** Earlier copies and the original may still be in flight on other paths (`floor-hold.md` §3), so one path is not enough. What is enough is **the latest send stamp of any copy of any covered hole, per path**:

```text
   σ_p(F)  =  max { τ(c) :  c a copy (original, resend or last copy) of a seq in [A_rx, F),  sent on path p }        absent if p carried none
```

Every seq in `[A_rx, F)` is a verdict seq, because F is the first non-verdict seq at or above A_rx (paper §5.1).

### 2.3 The event rule

"Pass the holes below F once every path that carried a copy of one of them has delivered something sent after that copy, or has stayed silent past its own loss timeout."

```text
   p resolved for F   ⇔   τ_max_rx(p) > σ_p                       (a datagram sent after p's last copy has arrived on p)
                      ∨   floor_rx(p) ≥ F                          (this floor, or a higher one, has arrived on p: staged after the copy)
                      ∨   τ_max_rx(any) ≥ σ_p + PTO_p              (silence bound: anything sent PTO_p after the copy has arrived, on any path)

   pass [next_deliver, F)   ⇔   ∀ p with σ_p present :  p resolved for F

   PTO_p  =  srtt_p  +  max( 4·rttvar_p ,  1 ms )
```

| symbol | meaning | provenance |
|---|---|---|
| σ_p | latest send stamp of a copy of a covered hole on p, carried on `WindowFloor` | sender state (measured stamps); the stamps are fresh at every send (`floor-hold.md` §1) |
| τ_max_rx(p) | largest `send_timestamp_us` of a data batch received on p | measured at the receiver |
| floor_rx(p) | largest floor received on p | measured at the receiver |
| τ_max_rx(any) | largest send stamp received on any path, data or floor (the floor carries its own `sent_us`) | measured at the receiver |
| srtt_p | the sender's per-path smoothed RTT | measured |
| rttvar_p | per-path RTT mean deviation | measured, RFC 6298 §2. **Owed:** the engine has none per path (`p_lost_inputs` uses `0.1·srtt`, a placeholder with no provenance) |
| 4, 1 ms | K and the clock granularity | cited, RFC 6298 §2 and RFC 9002 §6.2.1 (kGranularity). RFC 9002's `max_ack_delay` is omitted: no ack is awaited |

**Why it is safe, given the premise.**
- The first two terms are FIFO witnesses: nothing staged before them on p is still in flight.
- The third is offset-free, because it compares send stamps on one clock. A datagram sent at `σ_p + PTO_p` arrives at `σ_p + PTO_p + T_q`. A live copy on p arrives at `σ_p + T_p`, and `T_p ≤ srtt_p ≤ PTO_p`, since the one-way transit is part of the round trip. So the bound under-covers only a path whose one-way transit jumps above its own `srtt + 4·rttvar`, a queue ramp faster than the estimator. That is a miss, which is recoverable (§2.6).

**Why a wait cannot be removed entirely.** A path that carried a copy and then delivers nothing gives no event. "The copy is still in flight on a slow path" and "the path died with the copy" produce the same silence.
- Waiting for an event there can stall without bound on a dead path.
- Passing at once misses a slow path's copy.
- So some time bound on silence is forced. Its exact value is a price, rent (δ/d per stalled second) against a miss (ω): the same rent-or-buy as design U's `H_o = ω·d/δ`. ω is not measured.
- Until it is, the path's own loss timeout is the measured, cited bound. It binds only on a silent path, and its bind fraction is gauged (§2.6). When U lands, `min(PTO-bound, t_detect + H_o)` composes as `ordering-hold.md` §8 already states for H.

**The receiver needs no timer.** Every term changes only on an arrival, so the rule is evaluated on arrivals. If nothing arrives on any path, nothing can be filled either. This retires brief step 8's "the marker arms the drain timer".

**Several floors.** Each floor rise carries its own σ vector. A higher floor's σ is elementwise at least the lower one's, and its floor term is stronger, so "F₂ resolved ⇒ F₁ resolved". Passing in order needs no extra rule. Paper §5.1's newest floor `F_rx` stays the value; the event rule only decides when it acts.

### 2.4 Behaviour and numbers

| case | `floor-hold.md` H | event rule |
|---|---|---|
| N = 1, FIFO (c2, XE, every sim test T1, T2, T1′) | 0 | 0: the floor on the one path is its own witness. H_max = 0 is unchanged in the step-1 bounds |
| N = 1, floor datagram lost | 0, then the pass at the next floor | the next data batch sent after σ (mid-stream), or the next floor repeat (tail, 25–100 ms): the same as today |
| N = 2, c8 (skew 15 ms) / XK (skew 20 ms), last copy on the fast path | ≈ 15–18 / 20–23 ms on **every** pass (skew + spread, held for W) | **0**, if the slow path has delivered something sent after any older copy on it. An original sent about one RTT before the verdict is normally witnessed already: the slow leg delivers its successors before the floor arrives |
| N = 2, last copy on the slow path | as above | `≈ (owd_slow − owd_fast) − (τ_F − σ_slow)`, at most the skew: exactly the copy's own remaining transit |
| N = 2, a copy-carrying path dies | ≈ skew; a copy reviving later is a miss | the silence bound, ≈ `σ_p + PTO_p` (≈ 45–60 ms after the copy on XK's slow leg) |
| a skew spike 5 s ago | still held (W = 10 s) | forgotten: the rule has no memory |
| over-driven N = 2 (T5 class) | ≤ J + q + d ≈ 943 ms (`floor-hold.md` §7) | the true transit difference of the copy's path, ≤ the same bound; the silence bound carries the queue inside srtt_p |

**What that does to Realtime at N = 2.** The floor-only cells (XRK) and, after U, every floor pass wait only for copies actually in flight. Where the last copy goes is a placement question. On the fastest live path the wait is 0; that is `floor-hold.md`'s "recorded, not decided" item, now with no ω trade inside H, because nothing on the fast path can be passed over. It stays a placement-law question (§5.7) and is not proposed here.

### 2.5 The rejected alternatives, re-read

| alternative (`floor-hold.md` §3, paper §5.1) | rejected because | against the event rule |
|---|---|---|
| (i) `shed_recv_hold` as is | bit-selected, underived | still holds. Not re-proposed |
| (ii) `SRTT_rx/2` | wrong kind (a δ horizon) | still holds |
| `ordering-hold.md` §8 path support | (1) the receiver cannot tell which paths carry a copy; (2) the observed maximum never comes down | **no longer holds.** (1) is answered by σ_p on the wire; (2) there is no estimator to come down |
| per-path transit forecast `max(ĝ + Kv) − min(ĝ − Kv)` | > 0 on FIFO; queue ramps inflate it (7.5 s in T1); needs a live set; freezes on idle | **no longer holds for the role used here.** The PTO term is only a silence bound: on FIFO a witness ends the wait first; a ramp only lengthens a bound that binds on a silent path; the "live set" is the sender's σ list; an idle live path gets floor repeats on the sweep clock, which are witnesses |
| sender info, "the last copy's path" | earlier copies and the original are on other paths | **no longer holds:** σ_p covers every copy on every path |
| sender info, "the floor's send stamp" | not needed (the floor's arrival is the reference) | **now needed,** for the silence term when only floors arrive (a tail at N = 2 with the copy's path dead) |
| last copy on every path | ε^N against N − 1 copies; a placement question | still holds; not proposed |
| quantile; ρ-quantile | a constant with no provenance; the receiver has no ρ | moot: no statistic remains |
| paper §5.1 "the newest floor, not the minimum over paths (a frozen leg holds the minimum)" | the frozen leg | **still holds for the value.** A frozen leg blocks the event rule only if it carried a recent copy, and then the silence term bounds it |

### 2.6 What breaks it, and the gauges

- **In-path reordering** (Wi-Fi, netem jitter without `rate`) breaks the FIFO premise. A later-sent datagram can overtake the copy, the witness fires early, and that is a miss: the late fill is delivered at once, out of order, and subtracted from G_rx (or G_floor). The miss costs ω, and is the recoverable kind. `floor-hold.md`'s H covered in-path reorder by measuring it, at the price of W. The only cover for it is a lateness statistic with a memory, whose price is ω, so it belongs to U, not here.
- **A lost floor datagram** on an idle path: the next repeat (sweep clock), or the silence term.
- **Path ids must agree.** σ_p names paths in the sender's `PathId` (u32) space, and the receiver keys arrivals by the `path_id` each message carries (`receiver.rs:808`). That the two spaces are the same ids is **unverified**: an owed check before the build.
- **Gauges** on `[ACCT-RX]` (FORMULA-FIRST):
  - per pass, which term resolved each path: `fe_data`, `fe_floor`, `fe_pto`. `fe_pto` is the silence bound's bind fraction;
  - `fh_us` sum and max, the wait `t_pass − t_F`;
  - `fh_miss`, `fh_miss_us_max` and `fh_miss_refused`, kept from `floor-hold.md` §6 with the same bars;
  - `fifo_inv` per path: data batches with a stamp below that path's `τ_max_rx`. It must read 0 for the premise; it replaces `fl_late` as the N = 1 premise witness.
- **Retired:** the ℓ record deque, W, `fh_age_us`, `fl_trunc`, `fl_n`, `fl_late` and `fl_max_us`.

### 2.7 Cost and NO-MODE-SWITCH

- **Wire** (rides v11; amends decision 4): `WindowFloor { floor: u64, sent_us: u64, copies: [(path: u32, last_copy_us: u64, pto_us: u32)] }`. That is 16 + 16·N bytes, 48 bytes at N = 2. It is sent with each verdict and repeated at most `|channel_paths|` times per sweep.
- **Sender:**
  - per store entry, the latest send stamp per path (N·8 bytes; at most 64 KB at N = 2, B = 4 096);
  - σ_p maintained per floor value as verdicts extend the run, kept until `A_rx ≥ F`;
  - a per-path RFC 6298 rttvar.
- **Receiver:** per path, `τ_max_rx` and `floor_rx`; per pending floor, one σ vector. That is far less than the 10⁴-entry record deque.
- **No δ, ρ or hint** is read. Every term is computed on every arrival. At ρ = 1 no floor exists, so nothing is evaluated. The mixed pairings are unaffected: the rule is one function at every receiver.
- **Tests owed** (they replace `floor-hold.md` §7):
  - absolute tests at N = 1 (the floor witness gives 0) and at N = 2: a fast-path copy gives 0; a slow-path copy gives exactly its remaining transit; a dead slow path passes at `σ + PTO`; a lost floor passes on the next data or repeat;
  - "F₂ resolved ⇒ F₁ resolved";
  - the signature test (no hint, δ or ρ);
  - `floor_never_overtakes_its_copy_real_io` (kept);
  - `sigma_covers_every_copy` (every send site updates the per-path stamp);
  - a `fifo_inv = 0` witness in T1, T2 and T1′.

### 2.8 Recommendation for objection 2

**Adopt the event rule and drop W.** There is a correct option without a fixed wait on every path that keeps delivering. A wait remains only for a silent copy-carrying path, where none can be removed (§2.3); its bound is the path's own loss timeout, measured and cited, never a borrowed memory.

**Simpler fallback, E0**, if the per-path stamp is judged too heavy: the floor carries only the *set* of copy-carrying paths, and the witness is a floor ≥ F on each. That saves the per-entry stamps. It waits about the skew whenever the slow path carried any copy, including an old original (with C1 striping, almost always), and it still needs `sent_us` and PTO for the silence term.

## 3. Records

### 3.1 The age half of the verdict: degenerate at N = 1, Realtime and Auto, in the simulation

**The facts.**
- The verdict's age runs from the original send. `emit_source.rs:505` is the only `retransmit_buffer.insert`, made at first send with `src_send_ts_us`, and no resend re-stamps it.
- The NACK site reads that stamp (`sender_phases.rs:851–852`), and so does the slot site (`emit_source.rs:761–772`, deleted by S06).

**The age at a hole's first report.** Take a lost original sent at τ₀. The next datagram is sent `Δs` later on path j and arrives after `T_j`; the gap report leaves within the sampler wait `s ≤ 2 ms` and returns after `T_back`. Then:

```text
   a_r  =  RTT_r  +  g          RTT_r = T_j + T_back  (the report's round trip),   g = Δs + s
   refused by age   ⇔   a_r  ≤  D  =  min(b, 2)·SRTT_ewma
                    ⇔   SRTT_ewma − RTT_r   ≥   (1 − b)·SRTT_ewma  +  g
```

- **In the simulation, where D = b·RTprop exactly** (the toy, stage 1): `RTT_r ≥ RTprop`, so `a_r > RTprop ≥ b·RTprop` for b ≤ 1. **The gate never refuses at N = 1 at Realtime (b = ½) or Auto (b = 1).** The toy confirms it: `D-fail = 0` in every cell, N = 2 included (`results-stage1.md` finding 1).
- **In the engine this is not shown.** D reads the EWMA RTT, which sits above the fastest round trips and carries queueing.
  - **Auto (b = 1):** a refusal needs `SRTT_ewma − RTT_r ≥ g`, a report that comes back faster than the average by more than the send gap and the sampler wait. That is plausible under jitter and draining queues.
  - **Realtime (b = ½):** a refusal needs `RTT_r ≤ SRTT_ewma/2 − g`, a report round trip under half the EWMA. That happens only right after a large queue drains.
  - **Bulk (b = 2, only with an explicit ρ < 1):** first reports are mostly refused, so the gate is live there.
- **At N = 2 the gate is live in principle,** as a reorder guard: a fast leg reports a slow-leg seq early. The toy did not see it refuse there either, because its N = 2 cells judged ripe holes.

**Ledger verdict, owed (MEASUREMENT DISCIPLINE 17/18).** "Degenerate at N = 1 for b(δ) ≤ 1 in the simulation; engine refusal rate unknown, condition as above". This is a defect finding, not a footnote. The guard's W-AGE readout (registration §5.4, scored at XA) is where the engine's rate is read. It should print `SRTT_ewma − RTT_r` per report beside the refusals, so that a refusal can be attributed to estimator lag rather than to δ.

**Should D's role change? Yes, as a derivation owed, not decided here.**
- D's stated reason, "a retransmit older than D lands after the receiver's δ-horizon give-up" (`shed.rs:27–30`), presumes the old receiver arm. Under R1 or U the receiver never gives up on its own, so that reason is void.
- What survives is the reorder guard: "do not judge a seq that may still be in flight". That is a path quantity, and C3's per-flight hole law, which runs at every ρ in this build, already computes it per path: the RFC 9002 time threshold on the copy's path.
- The candidate is therefore a verdict that requires the hole law's "declared lost", with δ entering through D only if a δ term is derived. At Realtime and Auto that changes no N = 1 decision (§3.1 above), and at N = 2 it replaces b(δ)·RTprop by the copy's own path clock.
- **Open:** the estimator itself, EWMA RTT against windowed RTprop (decision 7 left it to a battery, both computed and their ratio logged).

### 3.2 D at two paths: highest-loss path against largest RTprop

**The divergence.**
- **Paper §5.6:** `RTprop = the per-path windowed-min RTT, the largest over live paths, no seed (D = 0 before the first sample)`.
- **Engine:** `shed_deadline_us(b, est.rtt())` (`emit_source.rs:631`) with `est = worst_eps_estimator(..)` (`:575`; `channel_set.rs:40–53`), the EWMA RTT of the path with the highest loss estimate, ties to the last maximum.

Three differences:
1. **Which path.** At c8 the c3 leg is lossier and slower, so the two agree by accident. At XK both legs read about 2.6 %, so the pick flips with estimator noise and D jumps by the RTT ratio, 10 against 50 ms (D = 5 or 25 ms at Realtime). With D = 5 ms, a slow-leg seq first reported by the fast leg at age ≈ 10 ms passes the age gate while it is still in flight. It gets a spurious verdict, a spurious last copy and a floor, and only H (or §2.3's rule) keeps it from becoming a miss.
2. **The estimator:** EWMA against windowed minimum (§3.1).
3. **The seed.** Before the first sample `ewma_rtt` is the 50 ms constructor seed (`estimator.rs:527–531`), so D = 25 ms at Realtime, not 0.

Also: D is refreshed only per source emission (`emit_source.rs:630`, under `pol.shed_on`), so it is frozen during a tail. Age only grows, so this matters only if the RTT changes during the tail.

**Bounding tests, owed** (MEASUREMENT DISCIPLINE: a divergence carries a test that bounds it):
- `d_path_input_divergence_is_bounded`, a unit test on the D input over a hand-built path set:
  - **N = 1, absolute:** engine D = paper D.
  - **N = 2, absolute,** with the fast leg *lossier* (fast: 10 ms RTT, ε 4 %; slow: 50 ms, ε 1 %). Assert the engine's `D/D_paper = 10/50 = 0.2` exactly, and in general `D_engine/D_paper ∈ [min_p RTT_p / max_p RTT_p, 1]`, printing both.
  - Red-first once the paper form is adopted; a bound until then.
- `d_is_zero_before_the_first_sample`: no RTT sample ⇒ D = 0 (paper). Red today (25 ms).
- **A sim record at N = 2 with equal-loss legs and 20 ms skew:** verdicts whose seq later arrives by its own original (spurious), and the D values, per run. It bounds the effect, not only the input.

## 4. ADR-0084 Amendment 4 (Proposed): the sender's allowance counts its own passes; the floor pass waits for events, not for a statistic

*Draft text for the ADR. It is not written into the ADR file until the owner decides.*

### Status

Proposed (2026-11-22), answering the owner's objections of 2026-11-22 to limit L-TAIL and to the floor hold H. Source: `docs/derivations/tail-and-hold.md`. Supersedes, where it says so: decision 4 (wire v11), decision 5 (the budget operand), Amendment 2 decision 3 (the pair merge), Amendment 3 item 3 (the derived H, and W pending) and item 4 (the old arm's passes spend the sender's allowance), and `floor-hold.md`'s law. Amendment 3's order of work, its exception list and its fallback clause are unchanged.

### Context

- **L-TAIL.** In the sender-first build the receiver's old arm (Amendment 3's exception) passes holes that enter `G_rx` and so `Ab`. Once they reach `(1 − ρ)·S_src`, the sender refuses every verdict, and a lost tail is served until it fills. A hole the old arm passes is dropped from service by the cumulative ack, verdict or not, so the sender's verdicts lose only what the floor passes. v11 carries only the sum `G_rx`.
- **H.** The derived H waits the worst late time offset seen in 10 s on every floor pass, with W borrowed. Per-path FIFO makes the wait unnecessary on every path that keeps delivering, given the sender's per-path stamp of the covered holes' latest copy.

### Decision

1. **The operand.** `Ab = G_floor + |{v ∈ V : v ≥ A_rx}|`, where G_floor is the holes the floor pass passed, net of their late fills. The verdict reads `Ab + 1 ≤ (1 − ρ)·S_src` as before. `G_rx` (every writer) is still carried and printed against the budget. Under R1 or U, `G_floor ≡ G_rx`, which is gauged.
2. **Wire v11** (one bump, still 10 → 11; amends decision 4):
   - `WindowAck.passed_by_floor: u64`, beside `passed_undelivered`;
   - `WindowFloor { floor: u64, sent_us: u64, copies: [(path: u32, last_copy_us: u64, pto_us: u32)] }`.
3. **The pair merge** (amends Amendment 2 decision 3): keep `(next_expected, G_rx, G_floor)` from the ack with the greatest `next_expected`; on a tie, the larger `G_floor`, then the larger `G_rx`.
4. **The floor pass** (replaces Amendment 3 item 3's H): pass `[next_deliver, F)` once every path p named in the floor's `copies` is resolved: `τ_max_rx(p) > σ_p ∨ floor_rx(p) ≥ F ∨ τ_max_rx(any) ≥ σ_p + PTO_p`, with `PTO_p = srtt_p + max(4·rttvar_p, 1 ms)` (RFC 6298 §2, RFC 9002 §6.2.1). It is evaluated on arrivals; there is no timer and no W. It reads no δ, ρ or hint. The receiver's writer per passed hole is kept, so that late fills subtract from the writer that passed the hole.
5. **No exception changes.** Amendment 3's `rho1-debt-exception` block, its removal trigger and its fallback stand.

### Consequences

- L-TAIL reduces to the ρ contract in this build: a tail is refused only by the sender's own floor passes and pending verdicts.
- Realtime latency is unchanged or earlier: the old arm still runs, and the earlier writer wins.
- Total interim abandonment can rise by at most `(1 − ρ)·S_src`, on holes the old arm does not pass first. The 1 % promise in total stays unkept at Realtime until the release rule, as Amendment 3 states. The sender's share of it, `G_floor ≤ (1 − ρ)·S_src`, holds at every prefix.
- H = 0 at N = 1 as before. At N ≥ 2 the floor pass waits only for copies in flight; a silent path waits its PTO.
- In-path reordering, which `floor-hold.md`'s H covered by measurement, becomes a gauged, recoverable miss (`fifo_inv`, `fh_miss`), until U's release prices it.
- Wire: 8 more bytes per `WindowAck`; 16 + 16·N bytes per `WindowFloor`.
- The guard registration's predictions are re-registered: §3.1's L-TAIL record, §5.4's admitted-verdict table and EX, §5.4b–c's R-H clauses, and §5.5b's XL. No VM time is added.
- Owed:
  - a per-path RFC 6298 rttvar;
  - a check that the sender's and receiver's path ids agree;
  - the SACK-aware pending term (`tail-and-hold.md` §1.3), as its own derivation step;
  - D's role and inputs (`tail-and-hold.md` §3).

## 5. Owed edits (not made here; they move with the owner's decision)

- **Paper:**
  - §5.6: the operand, the two v11 fields, the triple pair rule;
  - §5.1: the floor pass's event rule with its provenance table, replacing the `H_hold` formula; `floor-hold.md` marked superseded;
  - §11.2/§11.3: W's register row deleted, a row added for rttvar's owed estimator, and the two D divergences of §3.2 (path choice, cold seed) as §11.3 rows with their bounding tests.
- **Engine brief:**
  - step 4 (the `passed_by_floor` field, the writer tag, the triple merge and its test);
  - step 7 (`abandon_count(g_floor, pending)`);
  - step 8 (the event rule; the §2.6 gauges; §2.7's tests; the first floor leaves with the verdict's copy, and the sweep clock governs repeats);
  - steps 1–2's H_max = 0 is unchanged.
- **Guard registration** (`adr84-guard-registration`):
  - §2 tokens;
  - §3.1 (the L-TAIL record becomes a test with a forced budget seam, red on Amendment 3 as written);
  - §5.4 (the admitted-verdict table, EX on G_floor, `XE-NOT-EXHAUSTED` no longer holding the merge, BI unchanged);
  - §5.4b–c (R-H/R-H2 re-stated on `fh_miss` with `fifo_inv` and `fe_pto`);
  - §5.5b (XL as a scored gate);
  - §11 Q1 (W gone) and Q3 (L-TAIL answered).
- **`rho-step-sites.md` §6:** C11's row (the floor pass's hold becomes the event rule).
- **Roadmap** (`docs/roadmap/nodes.yaml`): a node for this derivation (owner decision pending), with `model-floor-hold` qualified by it and `rec-adr84-guard`'s registration marked for re-registration. `tools/roadmap/build.py` and `check.py` are run with that edit.
- **Ledger:** the age gate's verdict (§3.1) and the D path-input divergence (§3.2).

## 6. For the owner

**The lost tail.**
- At the end of a transfer, a lost packet waits for a resend only when the 1 % allowance is used up. Today the receiver's own old give-up, the temporary exception you accepted, uses it up first.
- The proposed fix is that the sender counts only the packets *its own* skip caused to be lost, which is one number more in each acknowledgement. Then the tail gets its skip whenever the sender's own allowance is left, and waits only when the sender has really used its 1 %. That wait is the promise itself, and the full receiver build would leave it too.
- Nothing gets slower, and the receiver still changes only once, with design U.
- The cost: at Realtime, total loss in the interim can grow by up to 1 % on packets the old give-up did not already skip.
- The alternative is to switch the receiver to floor-only now (the fallback you have). It keeps the 1 % promise in total, but it makes Realtime about half a round trip slower per lost packet (≈ +5 ms at 10 ms RTT, +50 ms at 100 ms), and it changes the receiver twice.

**The floor hold.**
- The receiver should not wait "as long as packets have ever been late". It should wait until it *knows* nothing more can come. On each path packets arrive in order, so once a path has delivered something sent after the last copy, nothing older is still on its way there. The sender tells the receiver, per path, when it sent the last copy.
- On one path the wait stays zero. On two paths it is zero unless the last copy went on the slower path, and then it is exactly that copy's remaining travel time.
- Only when a path goes silent does the receiver wait a time-out: that path's normal round trip plus four times its variation, the standard loss timeout.
- The 10 s memory goes, and the floor message grows by a few bytes per path.

**Your decisions:**
1. Amendment 4 item 1 (the sender's allowance counts its own passes), or Amendment 3's fallback now (floor-only receiver).
2. Amendment 4 item 4 (the event rule), or its smaller form E0 (§2.8), or keep `floor-hold.md` with W confirmed.

**Open, not owner calls:**
- the per-path rttvar estimator;
- the path-id agreement check;
- the SACK-aware pending term;
- D's role (a reorder guard from the hole law) and its estimator;
- the two D divergences' bounding tests.
