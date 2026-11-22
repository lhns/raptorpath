# The lost tail and the floor hold: two owner objections (derivation, no code)

**Objections (owner, 2026-11-22).**
1. **"Fix the lost tail first."** Limit L-TAIL (`adr84-guard-registration.md` §3.1, §5.5b): at a matched ρ < 1 pair in the sender-first build, the receiver's old arm passes holes on its own; those passes enter `G_rx` and so the sender's allowance `Ab`; once they reach `(1 − ρ)·S_src`, every sender verdict is refused, and a lost tail waits for a resend to get through, with no time bound. The owner wants this fixed before the sender build ships.
2. **"This fixed delay sounds completely wrong."** The floor hold H of `floor-hold.md`: the windowed maximum over W = 10 s of RFC 4737's late time offset. W was borrowed by analogy, not derived.

**Short answer.**
- **Objection 1: count only the sender's own floor passes against the allowance (option (c), §1.4).** The receiver already splits `G_rx` by writer (brief step 4). One more `WindowAck` field, `passed_by_floor` (8 bytes, riding v11, which is not built yet), lets the sender run its budget on `G_floor`. The old arm's passes stay on the wire in `passed_undelivered`, printed against the budget, as today. Realtime latency does not change, no ρ-keyed site is added, and Amendment 3's order stands. L-TAIL then reduces to the ρ contract (§1.1) in this build, as it does under design U. The same operand stays correct after U, where `G_floor ≡ G_rx`.
- **Objection 2: replace the statistical hold with an event rule, form E0 (§2.3), as a default awaiting owner confirmation.**
  - **The idea.** The floor is sent on every live path at one instant, and on each path it is staged after every copy and repair sent before it. On a FIFO path, its arrival therefore proves that nothing older is still on its way there. So the receiver passes the holes below F once the floor, or data sent after it, has arrived on as many distinct paths as the floor was sent on (a count, not a path list, so the sender's and receiver's path ids never need to agree).
  - **The remaining wait.** A wait remains only for a path that falls silent: no event tells "slow" from "dead" there, so it cannot be removed. Its bound is a marker timer `t_F + max_p(srtt_p + 4·σ_rtt,p)`, read from the per-path RTT statistics the engine already keeps (`PathState::srtt_measured`, `rtt_sigma_us`, paper §7.4), with RFC 6298's K and RFC 9002's granularity.
  - **What goes:** W, the record deque and the 10 s memory. The floor message grows to 21 bytes.
  - **What it does not change.** H = 0 at N = 1 as before. At N = 2 the wait stays about the skew, because repair keeps covering the holes up to the floor; it is exact and memoryless, never a remembered spike.
  - **The price.** It is not correct on a path that reorders internally. There the copy and its floor swap about half the time, and about half the skips whose copy got through are delivered out of order (§2.6).
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
- `net/recovery_laws.rs:311` and `net/mod.rs:2795–2807` (the tail sweep's pooled SRTT, the maximum over paths); `scheduler/path.rs:700–723` (`srtt_measured`, `rtt_sigma_us`);
- quinn-proto 0.11.19 (crates.io; no vendored copy in the repo), `src/connection/paths.rs:297`, `:313`, `:326`, `:331` (`var` and `pto_base` crate-private; `get` and `min` public).

No ADR-0084 engine branch exists on `github` (only `adr84-amend` and `adr84-guard-registration`), so every wire change below can still ride v11.

**Words.** *Tail*: the source seqs at the end of a transfer with nothing sent above them. *Allowance*: `A(t) = ⌊(1 − ρ)·S_src(t)⌋ − Ab(t)` (skip-rule §2). *Round*: one serve-and-report cycle, about RTT plus the sampler interval, or the tail-sweep period `clamp(2·SRTT, 25, 100) ms` for a tail.

## 1. Objection 1: the lost tail

### 1.1 The frame: what L-TAIL is and what no design can remove

- **A tail longer than the remaining allowance waits for its fills in every design.** That is the ρ contract: deliver at least ρ of every prefix. Brief T1 shows it: a 347-seq tail against an allowance of ⌊0.01·12 147⌋ = 121 gets at most 121 verdicts, and the rest are served until filled. A tail of up to B = N·2048 seqs fits only once `S_src ≥ B/(1 − ρ)`, which is 204 800 source symbols at N = 1 and ρ = 0.99.
- **A dead link is not the case to fix.** A floor crosses a link exactly as a resend does. On a link that delivers nothing, no design passes the tail; when the link returns, the tail resolves in one round either way (by fill, or by floor). The sim's `RWM_TEST_TS_DROP=dead` seam is different: it kills every copy of one seq while the link carries everything else. "Forever" holds only under that seam, or a seam without end. It is a test adversary, not a link.
- **On a live link the cost of L-TAIL is extra serve rounds, not forever.** A tail has nothing above it, so no gap report covers it. Its serve round is the tail-sweep period, not the RTT:

```text
   T_sweep  =  clamp( 2·SRTT_pooled , 25 ms , 100 ms )          SRTT_pooled = max over the recovery paths' smoothed RTT (net/recovery_laws.rs:311, net/mod.rs:2799–2807)
   E[extra rounds]  =  ε_c / (1 − ε_c)                            P(≥ k extra rounds)  =  ε_c^k        (per tail seq, independent loss)
```

  - ε_c is the loss of one copy. For an n-seq tail, the union bound gives `P(tail still open after k extra rounds) ≤ n·ε_c^k`.
  - A verdict sends one last copy and the floor together (Amendment 2), so it resolves in the first round unless both are lost. The floor is its own datagram with its own loss draw (toy stage 1, finding 2), so that happens with probability about `ε_c·ε_floor`, and then the next floor repeat resolves it, one T_sweep later.
  - The verdict therefore saves only the rounds after the first. Per cell, per tail seq:

  | cell | SRTT_pooled | T_sweep | ε_c | mean extra (served) | p99 extra (served) | verdict instead |
  |---|---|---|---|---|---|---|
  | c2 | 10 ms | 25 ms (floor of the clamp) | ≈ 2.6 % | 0.7 ms | 1 round = 25 ms | first round, + T_sweep with prob ≈ ε_c·ε_floor ≈ 7·10⁻⁴ |
  | c8 | 40 ms (c3 leg) | 80 ms | 2.7–4.1 % (by leg) | 2.2–3.4 ms | 1 round = 80 ms | as above |
  | XK | 50 ms (`c2r50` leg) | 100 ms (cap) | ≈ 2.6 % | 2.7 ms | 1 round = 100 ms | as above |
  | XE | 100 ms | 100 ms (cap; 2·SRTT = 200) | ≈ 10 % | 11 ms | 1–2 rounds = 100–200 ms (P(≥ 2) = 1 %) | first round, + T_sweep with prob ≈ 1 % |

  On the 100 ms link the sweep period and the RTT coincide; on the 10 ms links the round is the sweep's 25 ms floor, 2.5 RTTs. GE bursts raise ε_c inside a bad state. XL measures the tail (registration §5.5b).
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

**Why this operand is correct.** A sender verdict on s can lose s only if the receiver would otherwise have waited for s.
- A hole the old arm passed is dropped from the sender's service by the cumulative ack whether or not a verdict was taken: Amendment 3 item 4 books it as acked. So a verdict on such a hole costs nothing that the old arm did not already cost.
- G_floor is exactly the abandonment the sender's verdicts caused.
- Under U, and under R1, the floor pass is the only abandoning frontier writer, so `G_floor ≡ G_rx`: the operand means the same before and after the release rule. The field then becomes an identity gauge, `G_rx − G_floor = 0`, the routing witness that no other writer exists. **It goes on Amendment 3's removal list:** the release-rule build deletes `passed_by_floor` (with its protocol bump) together with the exception rows, once its identity has read 0 in that build's guard.

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

**What (c) sends in the matched cells.** With verdicts admitted at scale, two send streams change. Let r_src be the source rate and ĥ the share of source seqs that reach a report as holes past D.
- **Last copies:** one per admitted verdict, `≈ ĥ·r_src`. Each replaces the one serve resend that a refused verdict would have sent at the same report (the old arm's pass then ends service either way), so data sends are unchanged to first order. Where the floor now beats the old arm's clamped hold, the hole gets one copy instead of one serve per round until that hold, which is slightly fewer.
- **Floor datagrams:** the first floor goes with the verdict's copy. Coalesced to one floor per `serve_gaps` pass (the newest F, staged after the pass's copies; Amendment 2's "same instant" holds per pass), the rate is at most N per gap report, so `≤ N/GAP_ACK_MIN_INTERVAL = 500·N /s`. Repeats add at most N per T_sweep while `F > A_rx`, which in matched cells is short, because the old arm usually moves A_rx first. Each floor is 24–32 bytes (E0, §2.3).

| cell | r_src (source symbols/s) | ĥ (from XR's prediction) | last copies / s | floor datagrams / s | floor bytes / data bytes |
|---|---|---|---|---|---|
| X1 c2 (N = 1) | ≈ 17 500 (69 Mbit/s, 494 B) | 0.5–2.5 % | ≈ 90–440 | ≈ 90–440 (cap 500), + ≤ 40 repeats | ≤ 0.15 % |
| X2 c8 (N = 2) | ≈ 17 500–25 000 | 0.5–2.5 % | ≈ 90–630 | ≤ 2 × min(passes with a verdict, 500) ≈ 180–1 000 | ≤ 0.3 % |
| XK (N = 2) | as X2 | as X2, with the D-input precondition below | ≈ 90–630 | ≈ 180–1 000 | ≤ 0.3 % |
| XE (N = 1) | link- and cwnd-bound, not predicted | up to ≈ 10 % | ≈ 0.1·r_src | ≤ 500, + ≤ 10 repeats | ≤ 0.6 % at r_src ≥ 5 000 |

The r_src and ĥ figures are the registration's XR power numbers, not measurements. The guard prints both rates per row.

**L-TAIL under (c).** The old arm can no longer exhaust the sender's allowance. A tail is refused only when the sender's own floor passes plus its pending verdicts have reached `(1 − ρ)·S_src`. That is §1.3's state (the contract case plus the pending over-count), reached in this build and not months later.

**Cost.**
- **Build:**
  - one field in the v11 codec, written beside `passed_undelivered` in brief step 4;
  - the writer tag on passed holes, which step 4 already owes per writer;
  - `abandon_count(g_floor, pending)` in step 7;
  - the three-component pair merge, with its test extended.
- **VM: no new session; two cells re-pointed; no test seam on VM rows.** The registration's predictions are re-registered (§5):
  - §5.4's admitted-verdict table: the matched cells now admit verdicts at roughly XR's rate, so W4 and W6 become witnessable there;
  - EX is redefined on G_floor. At ρ = 0.99 no cell then exhausts the allowance, because the sender's own spend is about `ε_c·ĥ` (≈ 0.013–0.065 % at c2, at most ≈ 1 % at XE). So XE is predicted `XE-NOT-EXHAUSTED`, and that state no longer holds the merge;
  - **the refusal witness, without a seam:** it needs a cell where `1 − ρ < ε_c·ĥ`, so that the sender's own floor passes spend the allowance. ρ is a dial, so a point closer to 1 does it. **At ρ = 0.9999** the budget is 10⁻⁴·S_src. At c2 the predicted spend of 1.3–6.5·10⁻⁴ exceeds it: XR predicts 80–390 floor passes per 607 k-symbol row, against a budget of 61. At XE (≈ 0.1·ĥ) it exceeds it by far. The allowance opens only after `1/(1 − ρ) = 10 000` source symbols;
  - **XL's power: ρ = 0.9999 on a 20 MB object is marginal, so XL moves to ρ = 0.99998 and a 99 999-symbol object.**
    - **Why 0.9999 is marginal.** `Ab_c = G_floor + pending`. A unit is freed when a grant arrives, at every 1/(1 − ρ) source symbols. An admitted verdict holds it as pending for one round (≈ 100 ms at XE), and only becomes G_floor with probability ε_c ≈ 10 % (its last copy lost). Otherwise the copy fills, the unit is free again, and the next verdict takes it. So a granted unit becomes G_floor only after `m ≈ 1/ε_c ≈ 10` rounds (≈ 1 s) on average; it is still free, or merely pending, after m rounds with probability `(1 − ε_c)^m`.
    - At 0.9999 and 20 MB (≈ 40 500 source symbols), the 4th unit is granted at 40 000, about 500 symbols before the end. The tail therefore nearly always finds a free or only-pending unit, and is admitted once that verdict clears. The refusals seen there would be the pending over-count, not the contract.
    - **The sizing rule.** The last grant must come at least `m* = ln(0.01)/ln(1 − ε_c) ≈ 44` rounds before the tail, so that the unit is G_floor at the tail with probability ≥ 99 %. At XE that is `44 × 0.1 s = 4.4 s`, i.e. `4.4·r_src` source symbols.
    - **XL therefore runs at `--window-rho 0.99998`, with an object of 99 999 source symbols (≈ 49.4 MB at 494 B).** The budget `⌊2·10⁻⁵·S_src⌋` is 0 until 50 000 and 1 from then to the end. The single unit is granted 49 999 symbols before the tail, which is ≥ 44 rounds whenever `r_src ≤ 11 000` symbols/s. The rehearsal (§8.4 there) measures r_src at XE. If it exceeds 11 000, the object grows to `2·G − 1` symbols with `G = ⌈4.4 s·r_src⌉`, and `1 − ρ = 1/G`, which keeps exactly one grant, at the midpoint. This is recorded by amendment before the launch. At r_src ≥ 1 250 symbols/s the object is sent inside the 40 s window.
    - **The predicted floor-pass count before the tail is exactly 1**, the budget. Admission needs `G_floor + pending + 1 ≤ budget`, so G_floor cannot pass the budget. It is reached at about `S_src ≈ 50 000 + r_src·T_round/ε_c` (≈ 60 000 at 10 000 symbols/s). A late fill of that hole would free the unit again; at N = 1 on a FIFO link no earlier copy can arrive after the floor, so none is predicted.
    - **The exhaust witness, split by cause, at every judged verdict, and in particular at the tail's first judgment:**
      - `EX-FLOOR`: `G_floor + 1 > budget`. The sender's own floor passes have spent the allowance. This is the contract case;
      - `EX-PENDING`: `G_floor + 1 ≤ budget < G_floor + pending + 1`. The allowance is held by verdicts in flight. This is the pending over-count (§1.3 item 2), and it is reported, not counted as the contract.
    - **Is the contract-case tail reached?** It is reached on a row iff `EX-FLOOR` holds at the tail's first judgment. Predicted on ≥ 11 of 12 C84 rows (per-row ≥ 99 % at the sized object). XL reads `CONTRACT-TAIL-REACHED` at ≥ 10 of 12, else `CONTRACT-TAIL-MARGINAL`, which re-sizes the cell by amendment. The tail time on those rows is the contract's cost: served until filled, `ε_c/(1 − ε_c)` extra sweeps in mean.
    - **The rows and the time:** 24 rows. A 49.4 MB object at XE takes up to ≈ 10–40 s, against 20 MB before, so the session grows by ≈ 5–10 min.
  - optionally, X1 adds 6 C84 rows (≈ 6 min) for EX at a mid-stream point, sized by the same rule (at c2, `m* ≈ 175` rounds of ≈ 10 ms, so 1.75 s before the end of the row's last grant). If neither cell exhausts, the simulation carries the refusal path (T6, and §3.1's record turned into a test with a forced budget seam), and the guard lists it under "cannot see".
- **At N ≥ 2, §3.2's D-input fix is a precondition of (c), not an option.** The verdict is judged on every reported gap, whether or not C3's hole law has ripened the seq. Where D reads the fast leg (§3.2, XK when the worst-ε pick flips), a slow-leg seq still in flight passes the age gate. Under Amendment 3 as written, the exhausted allowance refused those verdicts in matched cells; under (c) they are admitted. Each sends a spurious last copy and a floor. The original then fills the hole, so nothing is lost, but there is up to one extra copy per slow-leg source seq while D reads the fast leg. Counting those copies would not remove them. The D defects are on main anyway, so the fix (the largest RTprop over live paths, with D = 0 before the first sample) lands first, and §3.2's bounding tests become its tests.
- **Wire:** 8 bytes per `WindowAck`.

**Variant (c1), not recommended:** redefine `passed_undelivered` as G_floor and add no field. The sender could then no longer print the total against the budget (the receiver knows neither S_src nor the sender's ρ), so decision 3's "the breach … is printed" would be lost at the sender.

### 1.5 Other options weighed

- **Exempt the tail** (a verdict on a seq with nothing sent above it spends nothing). That is a branch on a seq's position, and it breaks the contract it exempts. Rejected.
- **The sender counts every verdict, `Ab = |V|`.** It needs no wire field, but it gives up Amendment 2's saving: it spends about ĥ instead of ε·ĥ, 40× more in the skip-rule example. It also still mixes in verdicts the old arm made moot. Rejected.
- **Report per-seq pass and fill information** (the toy's observer channel). That needs an unbounded wire field. (c) needs one level.
- **Bound the old arm by the sender's ρ** (R2, `rho-step-sites.md` §1). That needs ρ in the handshake and two deciders on one budget. It was rejected there, and it is not simpler.

### 1.6 Recommendation for objection 1

**Option (c), with the SACK-aware pending term (§1.3 item 2) as an owed refinement.**
- **Price:**
  - one 8-byte ack field riding v11;
  - no latency change (a pass can only come earlier);
  - no new VM session: XL runs at ρ = 0.99998 with a 99 999-symbol object on C84, as the seam-free witness for the refusal (split `EX-FLOOR` / `EX-PENDING`) and for the contract-case tail; ≈ +5–10 min;
  - total interim abandonment up by at most `(1 − ρ)·S_src`, and only on holes the old arm did not pass first;
  - at N ≥ 2, §3.2's D-input fix is a precondition;
  - `passed_by_floor` goes on Amendment 3's removal list.
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
   σ_p(F)  =  max { τ(c) :  c sent on path p,  and c is a copy (original, resend, last copy) of a seq in [A_rx, F)
                                              or a coded repair whose window contains a seq in [A_rx, F) }        absent if p carried none
```

- Every seq in `[A_rx, F)` is a verdict seq, because F is the first non-verdict seq at or above A_rx (paper §5.1).
- **Repair counts, as in `floor-hold.md` §2 ("a fill can be either").** The encoder keeps its window at W* behind the ack (`sender_phases.rs:1372–1374`: `advance(ack − keep_behind)`), and evicts from the oldest only above `win_cap` (`net/mod.rs:3594–3596`). So the holes at A_rx stay inside the coding window, and **every repair sent on p up to the floor can still decode them.** On any path that carries repair, σ_p is therefore about τ_F.

### 2.3 The event rule, form E0

"Pass the holes below F once every path the floor was sent on has delivered that floor, or something sent after it, or once the paths' loss timeout has run out since the floor arrived."

The floor is sent on every live path at one instant τ_F. On each path it is staged after every copy and every repair sent before it. So **the floor's own arrival on a path is that path's FIFO witness.** Under E0 the latest covering send on each path is taken as τ_F itself (σ_p = τ_F), which is conservative: repair keeps covering the holes up to the floor (§2.2), so on a leg that carries repair σ_p ≈ τ_F anyway.

```text
   witness on path p for F   ⇔   a floor ≥ F sent at or after τ_F arrives on p
                             ∨   a data batch with send stamp τ > τ_F arrives on p,  and p delivered something stamped ≤ τ_F before

   pass [next_deliver, F)    ⇔   |{ p : witness on p for F }|  ≥  n_F          (count of distinct arrival paths)
                             ∨   now  ≥  t_F + PTO_F                           (marker timer)

   PTO_F  =  max over the paths the floor was sent on of ( srtt_p  +  max( 4·σ_rtt,p ,  1 ms ) )          computed by the sender at τ_F
```

| symbol | meaning | provenance |
|---|---|---|
| τ_F | the floor's send stamp, carried as `sent_us` | measured (sender clock, fresh at every send) |
| n_F | the number of paths the floor was sent on (the sender's live set at τ_F), carried as `n_paths: u8` | sender state |
| t_F | receiver arrival of the first floor ≥ F, on any datagram path or the reliable control stream | measured |
| srtt_p | the path's smoothed RTT, `PathState::srtt_measured()` (Copa's EWMA; `None` before a sample, so no constructor seed enters) | measured |
| σ_rtt,p | the path's RTT standard deviation, `PathState::rtt_sigma_us()` = `√EWMA[(rtt − srtt)²]` (`scheduler/path.rs:714–723`) | measured, paper §7.4 |
| 4 | K, applied to σ | cited, RFC 6298 §2 (K = 4 on the mean deviation). The standard deviation is never below the mean deviation, so 4σ covers at least as much as RFC 6298's 4·rttvar. Cantelli at k = 4 bounds `P(RTT > srtt + 4σ) ≤ 1/17` for any distribution (paper §7.4's own bound) |
| 1 ms | the timer granularity | cited, RFC 9002 §6.2.1 (kGranularity). RFC 9002's `max_ack_delay` is omitted: no ack is awaited |

**The PTO source.** quinn-proto 0.11.19 keeps an RFC 9002 `var` and `pto_base()` per connection, one connection per path (`src/connection/paths.rs:297`, `:331`). Both are crate-private, so the engine cannot read them; it can read only `rtt()` and `min()`. The engine already has its own per-path deviation, `rtt_sigma_us()`, with paper provenance (§7.4), and E0 reads that. **No new estimator is owed.** The `0.1·srtt` placeholder in `p_lost_inputs` is unrelated, and stays a separate finding.

**`pto_us` is the maximum over the paths the floor was sent on.** The sender computes `PTO_F = max_p (srtt_p + max(4·σ_rtt,p, 1 ms))` over exactly the n_F paths it sends that floor on, at τ_F. One value suffices, because the timer must cover the slowest path still unresolved, and the receiver does not know which one that is. A path without an RTT sample contributes nothing, and the marker waits for its first sample (below).

**Why it is safe, given the premise.**
- **A floor witness on p:** the floor was staged after every send on p before τ_F, so all of them have arrived or are lost.
- **A data witness on p:** a batch stamped after τ_F was staged after the floor on p. The extra clause, "p delivered something stamped ≤ τ_F before", keeps a path that only started delivering after τ_F from being counted in place of a silent old one.
- **The count needs no shared path ids.** Witnesses arrive only on paths the sender sends on, and at τ_F it sends the floor on exactly n_F of them. So n_F distinct arrival paths with a witness means every one of them has resolved. The receiver distinguishes paths by its own arrival `path_id`. **The path-id agreement precondition of the first draft therefore disappears.**
- **The count's one exposure is path churn:** a path that joins after τ_F and also delivered something stamped ≤ τ_F. That is not possible for a new path, so the exposure is limited to a path that left and rejoined inside the wait. It shows up as a miss, gauged.
- **The timer:** the floor was sent at `τ_F ≥` every covering send and arrived at `t_F ≥ τ_F + T_q`. A live covering send on p arrives by `τ_F + T_p`. So `t_F + PTO_F > τ_F + T_p` whenever `PTO_F ≥ T_p`, and `T_p ≤ srtt_p` because the one-way transit is part of the round trip. The timer under-covers only a path whose transit jumps above `srtt + 4σ`, a queue ramp faster than the estimator (Cantelli: at most 1/17 of RTTs exceed it). That is a miss, which is recoverable (§2.6).

**The send-anchored silence term of the first draft is dropped.** Under E0, `σ_p = τ_F`. A datagram stamped `≥ τ_F + PTO_F` arrives on path q at `τ_F + PTO_F + T_q′`, and the timer fires at `t_F + PTO_F = τ_F + PTO_F + T_q`, where T_q is the transit of the first floor's arrival. The send-anchored term can fire first only if T_q′ < T_q, that is, only by the transit change between the floor and that datagram (a draining queue), never by more. And the rule cannot run at all before some floor ≥ F has arrived, because the receiver does not know F before then. So the term adds nothing beyond a transit change. It is dropped, and with it any need for the receiver to compare stamps across paths.

**Why a wait cannot be removed entirely.** A path that carried a covering send and then delivers nothing gives no event. "The copy is still in flight on a slow path" and "the path died with the copy" produce the same silence.
- Waiting for an event there can stall without bound on a dead path.
- Passing at once misses a slow path's copy.
- So some time bound on silence is forced. Its exact value is a price, rent (δ/d per stalled second) against a miss (ω): the same rent-or-buy as design U's `H_o = ω·d/δ`. ω is not measured.
- Until it is, the paths' own loss timeout is the measured, cited bound. It binds only on a silent path, and its bind fraction is gauged (§2.6). When U lands, it composes with `t_detect + H_o` as `ordering-hold.md` §8 already states for H.

**The timer handles shutdown and a dead peer.** At shutdown the last floor comes on the reliable control stream: it sets t_F, and it is never a per-path witness. With a dead peer at N = 2, the floor may have arrived on only one path. In both cases arrivals stop, and the marker `(F, t_F + PTO_F)` (brief step 8's marker, kept) passes the buffered data. That keeps `floor-hold.md`'s property that "a dead or silent peer cannot stall a floor pass" (T4).

**With no RTT sample** on some path, `PTO_F` is not defined. The marker then waits for the witnesses alone, and the first sample arms it. A verdict needs `D > 0` first, which is D's own information-availability condition (once §3.2's cold-seed fix lands, D = 0 before the first sample), so a verdict, and hence a floor, cannot come before any RTT sample.

**Several floors.** A later floor ≥ F is itself a witness for F, so "F₂ resolved ⇒ F₁ resolved", and passing in order needs no extra rule. Paper §5.1's newest floor `F_rx` stays the value; the rule only decides when it acts.

**Form E1 (the refinement, not the default).** The floor carries, per path, σ_p, the latest send stamp of a copy or covering repair of the covered holes (§2.2), with the path's id. The witness on p is then any datagram on p stamped after σ_p. That is 0 on a leg that carried nothing covering since the originals, where E0 waits for the floor on that leg. E1 needs per-seq, per-path stamps at the sender, the repair-window tracking, a path list on the wire, and **the sender's and receiver's path ids to agree**, which is unverified. It is worth building only if the gauges show legs that carry no covering traffic.

### 2.4 Behaviour and numbers (E0)

| case | `floor-hold.md` H | E0 |
|---|---|---|
| N = 1, FIFO (c2, XE, every sim test T1, T2, T1′) | 0 | 0: the floor on the one path is its own witness. H_max = 0 is unchanged in the step-1 bounds |
| N = 1, floor datagram lost | 0, then the pass at the next floor | the next data batch (mid-stream), the next floor repeat (tail, one T_sweep), or the timer `t_F + PTO_F` if an earlier floor ≥ F had arrived |
| N = 2, c8 (skew 15 ms) / XK (skew 20 ms) | ≈ 15–18 / 20–23 ms on **every** pass (skew + spread, held for W) | ≈ 15 / 20 ms: until the slow leg delivers the floor or later data. The same size as H, but exact: never longer than the skew at that moment, never shorter (no miss on FIFO legs) |
| N = 2, slow leg idle | ≈ 0 (no sample) | ≈ the skew: the floor is sent on the idle leg too. E1 gives 0 here |
| N = 2, a path dies | ≈ skew; a copy reviving later is a miss | the timer, `PTO_F` after t_F (≈ 50 + 4·σ ms on XK's slow leg) |
| a skew spike 5 s ago | still held (W = 10 s) | forgotten: the rule has no memory |
| over-driven N = 2 (T5 class) | ≤ J + q + d ≈ 943 ms (`floor-hold.md` §7) | the true transit difference at that moment, ≤ the same bound; the timer carries the queue inside srtt_p |

**What that does to Realtime at N = 2.** The size of the wait does not fall: it stays about the skew. What changes is its kind. It is the wait until the slow leg shows that nothing older is still on its way, read per floor, with no 10 s memory of an old spike and no miss when a new lateness exceeds the window's maximum. Where the repair and the last copy go is a placement question (§5.7); not proposed.

### 2.5 The rejected alternatives, re-read

| alternative (`floor-hold.md` §3, paper §5.1) | rejected because | against E0 |
|---|---|---|
| (i) `shed_recv_hold` as is | bit-selected, underived | still holds. Not re-proposed |
| (ii) `SRTT_rx/2` | wrong kind (a δ horizon) | still holds |
| `ordering-hold.md` §8 path support | (1) the receiver cannot tell which paths carry a copy; (2) the observed maximum never comes down | **no longer holds.** (1): E0 needs no copy map, because the floor itself is the witness on every path it is sent on; (2): there is no estimator to come down |
| per-path transit forecast `max(ĝ + Kv) − min(ĝ − Kv)` | > 0 on FIFO; queue ramps inflate it (7.5 s in T1); needs a live set; freezes on idle | **no longer holds for the role used here.** The PTO term is only the timer: on FIFO a witness ends the wait first; a ramp only lengthens a bound that binds on a silent path; the "live set" is n_F; an idle live path gets the floor, which is its witness |
| sender info, "the last copy's path" | earlier copies and the original are on other paths | **no longer holds:** E0 waits for every path the floor went on |
| sender info, "the floor's send stamp" | not needed (the floor's arrival is the reference) | **now needed,** for the data witness (a batch stamped after τ_F) |
| last copy on every path | ε^N against N − 1 copies; a placement question | still holds; not proposed |
| quantile; ρ-quantile | a constant with no provenance; the receiver has no ρ | moot: no statistic remains |
| paper §5.1 "the newest floor, not the minimum over paths (a frozen leg holds the minimum)" | the frozen leg | **still holds for the value.** A frozen leg blocks E0 only until the timer |

### 2.6 What breaks it, and the gauges

**Reordering inside one path.** It breaks the FIFO premise, and the copy and its floor are the pair most exposed, because they are sent back to back.
- **The mechanism.** On a path with independent per-packet jitter and no rate limiter (netem `delay D J` without `rate`, some Wi-Fi), two packets sent g apart swap with probability `P_swap(g) = (J − g)²/(2J²)` for uniform jitter on [0, J], and 1/2 at g → 0 for any continuous jitter. The copy and the floor leave microseconds apart, so they swap about half the time. That is the toy's finding: the floor beat its copy in about half the verdicts (`results-stage1.md` finding 2).
- **In user terms.** On such a path, about half of the skipped packets whose last copy does get through, `(1 − ε_c)/2 ≈ 49 %` of skips at ε_c = 2.6 %, are delivered **out of order**: just after the packets behind them, instead of in order. Nothing is lost; the late fill is delivered at once and subtracted from G_rx (and G_floor), and it costs ω, the disorder price. The data witness has the same exposure (the next data batch is µs behind the copy at Realtime rates).
- **The bar.** `fh_miss / floor-passed holes ≤ 1 − ρ` per path, the miss bar of `floor-hold.md` §6, now applied at N = 1 too.
  - On a FIFO path (every guard cell sets `rate`, so netem is FIFO) the prediction is 0.
  - On a jittered path without `rate` the prediction is ≈ 49 %, which fails the bar by far. **E0, like every FIFO-witness rule, is not correct on a path that reorders internally.**
- **Detection:** `fifo_inv` per path, data batches stamped below that path's newest stamp. It is 0 on FIFO paths, and it names the link class where E0 misses.
- **The options for that class, owner's call later, not part of Amendment 4:**
  - (r1) shift the witness on a path with `fifo_inv > 0` by that path's own measured in-path late offset (`floor-hold.md`'s ℓ restricted to one path). That is 0 on FIFO paths, but it brings back a memory length W, for reordering paths only;
  - (r2) carry the first floor inside the copy's datagram, so the copy cannot be overtaken by its own floor. A lost copy then loses its floor, and the hole waits one repeat. The toy measured this shared-fate cost as a refresh round, and the data witness would still have to be disabled there;
  - (r3) accept the misses and leave their price to ω, under U.
- **Owed test:** `copy_floor_swap_on_reordering_path`, loopback with netem jitter 4 ms and no `rate`, recording `fh_miss` per verdict against the 49 % prediction, beside `fifo_inv`.

**The silence term's bind fraction, and the miss rate it implies.** Cantelli's 1/17 is above the 1 % bar, so the bar rests on the timer binding rarely. Per cell:
- **Mid-stream**, every path carries data within an inter-packet gap of the floor, so a lost floor is replaced by a data witness at once. The timer binds only on path death: `fe_pto ≈ 0`.
- **At a tail** (no data follows), a path whose floor datagram is lost has no witness until the next repeat, one T_sweep later. `T_sweep = clamp(2·SRTT, 25, 100) ms` is at least `PTO_F` whenever `4·σ_rtt < SRTT`, so the timer fires first. So `fe_pto ≈ 1 − (1 − ε)^N` of tail passes.
- **The miss it implies.** A timer-resolved pass misses only if a covering send on the silent path is still in flight past `PTO_F`. Under Cantelli that is at most 1/17 of those passes; on these cells' bounded netem jitter (J ≤ 4 ms, far below srtt) it is ≈ 0.

| cell | N | ε per leg | `fe_pto`, mid-stream | `fe_pto`, tail passes | miss bound `fe_pto/17` (tail) | predicted miss |
|---|---|---|---|---|---|---|
| c2 (X1, XR) | 1 | 2.6 % | ≈ 0 | ≈ 2.6 % | ≤ 0.15 % | ≈ 0 |
| c8 (X2) | 2 | 2.7 / 4.1 % | ≈ 0 | ≈ 6.7 % | ≤ 0.4 % | ≈ 0 |
| XK, XRK | 2 | 2.6 / 2.6 % | ≈ 0 | ≈ 5.1 % | ≤ 0.3 % | ≈ 0 |
| XE, XL | 1 | ≈ 10 % | ≈ 0 | ≈ 10 % | ≤ 0.6 % | ≈ 0 |

So the distribution-free bound stays under the 1 − ρ = 1 % bar in every cell, even at the tail. A measured `fe_pto` far above these values (for example, from silent paths mid-stream) is a finding, because there the 1/17 bound would no longer clear the bar.

**A miss is disorder, not loss.** A late fill below the frontier is delivered at once, out of order: `ReorderBuffer::push_with_time` hands out any seq below `next_deliver_seq` immediately (`net/reorder.rs:75–82`), pinned by the unit test at `:267–275` ("a late fill below the frontier is delivered immediately"). The fill is also subtracted from G_rx and G_floor. So a miss of the floor pass costs ω (one out-of-order delivery) and a transient allowance unit, never a packet. The exception is a fill older than the decode floor, which the decoder refuses (`fh_miss_refused`, must read 0; `floor-hold.md` §4).

**Other cases.**
- **A lost floor datagram:** the next data on that path, the next repeat, or the timer.
- **Gauges** on `[ACCT-RX]` (FORMULA-FIRST):
  - per pass, which term resolved it: `fe_count` (all n_F witnesses) or `fe_timer`. `fe_timer` is the timer's bind fraction;
  - `fh_us` sum and max, the wait `t_pass − t_F`;
  - `fh_miss`, `fh_miss_us_max` and `fh_miss_refused`, kept from `floor-hold.md` §6, with the bar above;
  - `fifo_inv` per path; it replaces `fl_late` as the N = 1 premise witness.
- **Retired:** the ℓ record deque, W, `fh_age_us`, `fl_trunc`, `fl_n`, `fl_late` and `fl_max_us`.

### 2.7 Cost and NO-MODE-SWITCH

- **Wire** (rides v11; amends decision 4): `WindowFloor { floor: u64, sent_us: u64, n_paths: u8, pto_us: u32 }`, 21 bytes. It is sent with each `serve_gaps` pass that raised F, and repeated at most `|channel_paths|` times per sweep.
- **Sender:** nothing per seq. At each floor send: the live-path count and `PTO_F` from `srtt_measured()` and `rtt_sigma_us()`, both already per path.
- **Receiver:** per path, the newest data stamp and whether it delivered anything stamped ≤ τ_F; per pending floor, `(F, τ_F, n_F, PTO_F, t_F)` and a witness set. That is far less than the 10⁴-entry record deque.
- **No δ, ρ or hint** is read. Every term is computed on every arrival and at the marker. At ρ = 1 no floor exists, so nothing is evaluated. The mixed pairings are unaffected: the rule is one function at every receiver.
- **Tests owed** (they replace `floor-hold.md` §7):
  - absolute tests at N = 1 (the floor witness gives 0) and at N = 2 (the pass waits for the slow leg's floor or later data; a dead slow leg passes at `t_F + PTO_F`; a lost floor passes on the next data, repeat or timer; a path that joins after τ_F is not counted);
  - "F₂ resolved ⇒ F₁ resolved";
  - the signature test (no hint, δ or ρ);
  - `floor_never_overtakes_its_copy_real_io` (kept), and `copy_floor_swap_on_reordering_path` (§2.6);
  - a `fifo_inv = 0` witness in T1, T2 and T1′.

### 2.8 Recommendation for objection 2

**Adopt E0 and drop W, as the default awaiting owner confirmation.**
- On every FIFO path that keeps delivering there is a correct option without a fixed wait: the floor's own arrival on each path.
- A wait remains only for a silent path, where none can be removed (§2.3). Its bound is the paths' loss timeout, measured (`srtt`, `σ_rtt` per path, already in the engine) and cited (RFC 6298's K, RFC 9002's granularity), never a borrowed memory.
- It does not shorten the two-path wait (still about the skew), and it is not correct on a path that reorders internally (§2.6), where `floor-hold.md`'s measured H was.
- E1 is the refinement if legs without covering traffic turn out to matter.

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

- **In the simulation, where D = b·RTprop exactly** (the toy, stage 1): `RTT_r ≥ RTprop`, so `a_r > RTprop ≥ b·RTprop` for b ≤ 1. **The gate never refuses at N = 1 at Realtime (b = ½) or Auto (b = 1).** For Realtime the toy confirms it: `D-fail = 0` in every cell, N = 2 included (`results-stage1.md` finding 1). The toy fixed D = RTprop/2 (its line 65), so it tested b = ½ only. The Auto statement rests on the inequality alone.
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
   - `WindowFloor { floor: u64, sent_us: u64, n_paths: u8, pto_us: u32 }` (form E0).
3. **The pair merge** (amends Amendment 2 decision 3): keep `(next_expected, G_rx, G_floor)` from the ack with the greatest `next_expected`; on a tie, the larger `G_floor`, then the larger `G_rx`.
4. **The floor pass** (replaces Amendment 3 item 3's H). **Form E0 is the default awaiting owner confirmation.** Pass `[next_deliver, F)` once:
   - a witness has arrived on `n_paths` distinct paths, a witness being a floor ≥ F sent at or after `sent_us`, or data stamped after `sent_us` on a path that delivered something stamped ≤ `sent_us` before;
   - or the marker `t_F + pto_us` has expired, with `pto_us = max_p(srtt_p + max(4·σ_rtt,p, 1 ms))` from `PathState::srtt_measured()` and `rtt_sigma_us()` (paper §7.4; RFC 6298 §2, RFC 9002 §6.2.1).

   There is no W. It reads no δ, ρ or hint. The receiver keeps the writer per passed hole, so late fills subtract from the writer that passed the hole. On a path that reorders internally the rule misses (`fifo_inv`, `fh_miss`), and the remedy is open.
5. **No exception changes.** Amendment 3's `rho1-debt-exception` block, its removal trigger and its fallback stand. The removal list gains `WindowAck.passed_by_floor`: the release-rule build deletes it, with its protocol bump, once `G_rx − G_floor = 0` has read 0 in that build's guard.
6. **Precondition at N ≥ 2:** D's input is the largest RTprop over live paths, with D = 0 before the first sample (`tail-and-hold.md` §3.2), before (c) is built.

### Consequences

- L-TAIL reduces to the ρ contract in this build: a tail is refused only by the sender's own floor passes and pending verdicts.
- Realtime latency is unchanged or earlier: the old arm still runs, and the earlier writer wins.
- Total interim abandonment can rise by at most `(1 − ρ)·S_src`, on holes the old arm does not pass first. The 1 % promise in total stays unkept at Realtime until the release rule, as Amendment 3 states. The sender's share of it, `G_floor ≤ (1 − ρ)·S_src`, holds at every prefix.
- H = 0 at N = 1 as before. At N ≥ 2 the floor pass waits until every leg the floor went on has delivered it, about the skew; a silent leg waits the marker.
- In-path reordering, which `floor-hold.md`'s H covered by measurement, becomes a miss: about half the skips on such a path are delivered out of order. It is gauged (`fifo_inv`, `fh_miss`, bar 1 − ρ) and recoverable, and its remedy is an open owner choice (§2.6).
- Wire: 8 more bytes per `WindowAck`; `WindowFloor` grows from 8 to 21 bytes.
- The guard registration's predictions are re-registered: §3.1's L-TAIL record, §5.4's admitted-verdict table and EX, §5.4b–c's R-H clauses, and §5.5b's XL. No VM time is added.
- Owed:
  - the SACK-aware pending term (`tail-and-hold.md` §1.3), as its own derivation step;
  - D's role and inputs (`tail-and-hold.md` §3).

## 5. Owed edits (not made here; they move with the owner's decision)

- **`floor-hold.md`:** untouched here. It is marked superseded only once the owner confirms E0.
- **Paper:**
  - §5.6: the operand, the two v11 fields, the triple pair rule;
  - §5.1: the floor pass's event rule with its provenance table, replacing the `H_hold` formula; `floor-hold.md` marked superseded;
  - §11.2/§11.3: W's register row deleted, and the two D divergences of §3.2 (path choice, cold seed) as §11.3 rows with their bounding tests.
- **Engine brief:**
  - step 4 (the `passed_by_floor` field, the writer tag, the triple merge and its test);
  - step 7 (`abandon_count(g_floor, pending)`);
  - step 8 (the event rule; the §2.6 gauges; §2.7's tests; the first floor leaves with the verdict's copy, and the sweep clock governs repeats);
  - steps 1–2's H_max = 0 is unchanged.
- **Guard registration** (`adr84-guard-registration`; untouched here, and left untouched until the owner decides):
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
- The receiver should not wait "as long as packets have ever been late". It should wait until it *knows* nothing more can come.
- On each path packets arrive in order, and the sender sends its "skip below here" message on every path right after everything else. So once that message, or anything sent after it, has arrived on every path, nothing older is still on its way.
- On one path the wait stays zero. On two paths it is still about the difference in the paths' delays, but it is exactly the time needed: it never remembers an old spike, and it never gives up too early.
- Only when a path goes silent does the receiver wait a time-out: the paths' normal round trip plus four times its variation, the standard loss timeout, from measurements the engine already keeps.
- The 10 s memory goes, and the message grows by 13 bytes.
- **The catch:** on a link that reorders packets within one path, about half of the skipped packets whose last copy did arrive would come out of order. Nothing is lost. All tested links are in-order; the gauge shows the case if it occurs.

**Your decisions:**
1. Amendment 4 item 1 (the sender's allowance counts its own passes), or Amendment 3's fallback now (floor-only receiver).
2. Amendment 4 item 4 (E0, the default awaiting your confirmation; E1 the refinement, §2.8), or keep `floor-hold.md` with W confirmed. Later, if a reordering link class matters: r1, r2 or r3 (§2.6).

**Open, not owner calls:**
- the SACK-aware pending term;
- D's role (a reorder guard from the hole law) and its estimator;
- the two D divergences' bounding tests.
