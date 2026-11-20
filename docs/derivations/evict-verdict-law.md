# The verdict law for the evicting seat (derivation, no code)

Roadmap node `rec-evict-blocker-unresolved`. Owner decision of 2026-11-20:
fix the defect with one rule, "every abandoned seq gets a verdict and the
receiver is told". Inputs: `evict-fix-prechecks.md`, `core-extraction-recovery.md`,
ADR-0064, ADR-0079 with Amendment 1, ADR-0081 (and its draft on
`recovery-counters` e6355bec, which adds the `E` term), plan S4,
`liveness-audit.md` item 4, and paper §4.10, §5.1, §5.6, §6.1 and §6.4.
Code read on github/main `1ef0f634`. Paths are under `raptorpath/src/`
unless they start with `tests/` or `docs/`.

The precheck numbers are one seed, run on Windows before the port rekey, in
a simulation with no CPU cost. Here they show direction, not size, and no
test below pins any of them.

FORMULA-FIRST: each law appears on its own line next to the sentence it
implements, and each symbol has a provenance line. Section 8 holds the paper
text. Nothing here changes code.

**Revision 2** (same branch) answers the coordinator's follow-up. Its changes:
- §9 now opens with two owner decisions. The first is the store fork (§3.6),
  with both branches spelled out. The second is the rate bound together
  with the overload answer.
- D's RTT is traced to ADR-0064's derivation, RTprop (§1.3). The EWMA keeps
  shipping until a battery reads the RTprop form (question 7).
- The termination rule is bounded (§2.2).
- The receiver now takes the newest floor from **any** path, not the minimum
  over paths, because a frozen idle leg would lock the minimum out (§2.3).
- An ablation shows which test needs the wire change (§2.5).
- The store-cap reading is reconciled with the store-law record (§3.2 (b)).
- The supersessions in `core-extraction-recovery.md` are marked (§10).

## 0. Two words, one partition

- **Verdict** (sender side). The sender declares "I will not serve this seq
  again". A seq gets a verdict in one of two ways:
  - a **shed** (`H`), which is discretionary and can be refused by the budget;
  - a **forced verdict** (`E`), taken when the last servable copy is gone.
    The coding window evicts the seq while it is unresolved and no store
    holds it, so this verdict cannot be refused.
- **Abandoned** (receiver side). The receiver's in-order frontier passed the
  seq without delivering it. Only the receiver knows this. At ρ < 1 the
  cumulative ack also passes holes the receiver gave up on. So from the ack
  alone the sender cannot tell a delivered seq from an abandoned one. On
  `recovery-counters` the lost tail reads `A = 1`, `E = 11 799`, and nearly
  all of those seqs were delivered.

**The 1 − ρ budget counts abandonment, not verdicts.** A verdict is only an
upper bound on it.

The sender's partition of the source seqs it has sent, U being the
*unresolved* set (sent, not acked, no verdict):

```text
   S_sent  =  A  +  V  +  |U|          V = H + E,  each seq in exactly one term
```

- A seq with a verdict stays in `V`, even when a later cumulative ack covers
  it. Its delivery or abandonment is the receiver's fact (§4).
- ADR-0081's `O_serv + O_strand = |U|`. Part 2 below makes `O_strand ≡ 0` by
  construction, because no unresolved seq can lie below the servable span.
- At ρ = 1, `V = 0` by data. The store holds every unresolved seq, so `E = 0`,
  and the budget is 0, so `H = 0`.

## 1. The verdict rule (parts 1 and 2 are one change)

Parts 1 and 2 ship together. A shed judged on the unclamped gap that never
tells the receiver turns "the sender stalls" into "the receiver waits". The
precheck shows it: in `sr_evict_b05` the untold shed left the ack at 12 087,
while refuse-and-resend in `sr_evict` reached 12 147.

### 1.1 Forced verdict at the eviction sites

"A seq whose last servable copy leaves the sender gets a verdict at that
instant."

```text
   evict(s)  ∧  s ∈ U  ∧  s ∉ store   ⇒   U ← U ∖ {s},  E ← E + 1,  record s ∈ V
```

- The sites are the two window advances: the size cap at `net/mod.rs:3594–3597`
  and the ack-relative slide at `net/sender_phases.rs:1374`.
- The slide cannot reach an unresolved seq: it advances to `ack − keep_behind`,
  which is below the ack. So it is a witness only.
- A forced verdict removes `s` everywhere a seq is pruned today at a shed:
  `retransmit_buffer`, `nack_retx_at`, and `source_path_map` (already pruned
  at `:3601`).
- At ρ = 1 the store holds `s`, so the antecedent is false by data. The
  formula reads no seat bit.

### 1.2 Judging a reported gap

"Every unresolved seq in a reported gap is judged on the unclamped gap. The
clamp only bounds what is served."

```text
   for s ∈ [g_s, g_e] ∩ U :                                     (no clamp here)
      verdict(s) = SHED     if  age(s) > D  ∧  Ab + 1 ≤ (1 − ρ)·S_src
                 = SERVE    else if  s ∈ [f, c]                 (clamp_gap's servable span)
                 = FORCED   otherwise                           (counted, never refused)
```

- Seqs in the gap that are not in U (acked or already given a verdict) are
  skipped. The floor covers them (§2).
- After §1.1, `U ⊆ [f, c]` holds at every instant, so the FORCED arm is empty
  by construction at the NACK and sweep sites. It is kept as the rule's
  closure, with a gauge that must read 0 (§7).
- The clamp `clamp_gap(s, e, f, c)` (core-extraction §1.1) is unchanged. It
  now runs after the shed check, on the seqs the shed check left to serve.
- The shed arm is ADR-0064's `shed_allowed`. Only its budget operand changes
  (§4): `Ab` and `(1 − ρ)` replace `shed_total` and `ε̂·(1 − P_fec)`.

**Where today's per-gap bookkeeping sits.** Today, after the clamp,
`nacked_count += e′ − s′ + 1` is counted (`net/sender_phases.rs:803`; it
reduces `repair_debt` at `:1171`). Then the per-seq loop stamps every seq
first (the `[HOLD]` echo `hold_echo.on_reported`, `:812–830`), before any
`continue`, and only then reaches the shed and the store miss (`mpd_stale`).
The new order keeps both in place:
- **`nacked_count`** stays on the clamped span, at the same line.
- **The per-seq order** is: stamp (as today), then skip if `s ∉ U` (where
  today's store miss and `mpd_stale` sit), then `judge_seq`, then serve.
- **At ρ = 1 both are identical in both branches.** The clamp is the
  identity there, so the span, the count and the stamps are today's. The
  `s ∉ U` skip falls where today's store-miss skip already is.
- **At ρ < 1, branch W:** `nacked_count` is unchanged (clamped). The stamps
  now also cover below-window gap seqs, which are all `∉ U` and skipped.
  This is a gauge change only.
- **At ρ < 1, branch S:** the clamp bounds are `(0, u64::MAX)`, so
  `nacked_count` counts the whole gap. That is a behaviour change: a larger
  `repair_debt` reduction at ρ < 1. It is part of S's guard battery.

The tail sweep target is unchanged: `tail_sweep_gap(min U)` (core-extraction
§1.2). After §1.1, `min U` is always servable, so the D1c defect (a sweep
aimed below the window) cannot occur. Its gap goes through §1.2.

### 1.3 The deadline D: refreshed where verdicts are taken (operand: owner question 7)

"D is the span law's own deadline, read when the verdict is taken."

```text
   D  =  min( b(δ)·T , 2·T )        evaluated once per judged batch and per sweep fire
   T  =  RTprop   (the derivation's operand)        T = EWMA RTT  (what ships under question 7's default, until a battery reads RTprop)
```

**What ships under the default:** the refresh at the verdict sites, with the
EWMA operand the engine uses today. The `D_rtprop / D_ewma` gauge logs
beside it. The RTprop operand moves in its own change after a battery. The
bullets below give the reasons.

- **Refresh.** Today D is refreshed only inside `emit_source`
  (`net/emit_source.rs:630–633`), so it freezes at the last source emission.
  The prechecks show `D = 13 ms` beside a live `rtt = 844 ms`.
- **Which RTT the derivation used.** ADR-0064's shed commit `6568822c`
  (2026-07-21) states the deadline as "age > D = b(hint)*RTprop, the span
  law's D". It names the function's parameter `rtprop_us`, and its test
  comment reads "The shed deadline IS the span law's D (§16.20.3): b·RTprop,
  capped at the 2·RTprop deficit-round limit". The call site in the same
  commit passed `est.rtt()`, the EWMA RTT. So the code has diverged from its
  derivation since the day it shipped. Paper §11.3's first row already
  records the divergence (model `min(b·RTprop, 2·RTprop)`, engine EWMA).
- **The EWMA under a self-inflicted queue.** The EWMA tracks RTprop plus the
  standing queue. When the sender's own backlog builds (the unpaced source
  of the prechecks), the queue delay enters D. D grows with the backlog:
  `b·844 ms ≈ 422 ms` at Realtime instead of `b·RTprop`. That has three
  consequences:
  - fewer holes count as late;
  - fewer holes are shed;
  - more resends go into the same queue.

  The deadline thereby stops measuring the δ horizon and starts measuring
  the machine's own congestion. That is positive feedback: a larger queue
  gives a longer deadline, more serialization and a larger queue. RTprop,
  the windowed-min RTT, excludes the standing queue by construction, which
  is why the span law uses it.
- **The derivation's form is D on RTprop** (T = RTprop). D reads the per-path
  windowed-min RTT that the §6.1 cap law reads, taking the largest over live
  paths. The resend of a hole can go on any live path (the recovery plane
  uses `live_paths()`, §11.3). The deadline is when the slowest such resend
  stops being useful, and the largest RTprop is the choice that sheds the
  least. It is not settled by editing the comment.
- **But the anchor is not clean either.** The store-law record
  (store-law-state, quoted in §3.2 (b)) shows the logged `rtp` over-reading
  the netem floor 5–25× on the VM. So "RTprop excludes the queue by
  construction" holds for the estimator's definition, not for its measured
  value. A D on that anchor would read high, toward the EWMA. And ADR-0064's
  flip battery ran the EWMA call site: **the derived form has never been
  measured.** That is owner question 7.
- **If and when D moves to RTprop:**
  - it is an engine behaviour change on the evicting seat: D shrinks under
    queueing;
  - with rate bound (b) in place, the standing queue is bounded near
    q(δ)·RTprop, so the two forms differ by at most a factor (1 + q) ≤ 1.1
    in steady state. They diverge mainly in the overload transient, where
    the derivation's form is the δ-honest one.
  - **Caveat:** that bound holds only while the cap B tracks the link. On
    real links B is pinned at its ceiling (§3.2 (b)), and the standing queue
    is then bounded by the ceiling, not by q(δ).
  - The receiver hold stays on SRTT (`shed_recv_hold`). The sender's D and
    the receiver's H then disagree by the standing queue. That is the
    existing "receiver shed hold" row of §11.3, not a new divergence.
- **Cold start.** Before RTprop has a sample there is no anchor, so `D = 0`,
  and `shed_allowed`'s `deadline_us > 0` makes nothing sheddable: the seq is
  served. A seed (the estimator's 50 ms constructor value,
  `control/estimator.rs:527–533`) is not an anchor (MEASUREMENT DISCIPLINE
  rule 1).
- **D does not bound the count.** On the RTprop operand, every queued seq is
  past `D = b·RTprop` under queueing. On the EWMA, the queue enters D
  instead. Either way, only the budget term bounds how many are shed.

### 1.4 Provenance

| symbol | provenance |
|---|---|
| U, A, H, E, S_src | measured state (sender counters; ADR-0081 sites) |
| age(s) | measured: `now − send_ts(s)` from `retransmit_buffer` |
| b(δ), the min with 2·RTT | derived, paper §5.3/§5.4 (ADR-0064 decision 2) |
| RTprop | measured: per-path windowed-min RTT (the §6.1 anchor), max over live paths; no seed. **Known defect:** the logged `rtp` over-reads the netem floor 5–25× on the VM (store-law-state); gauge `D_rtprop / D_ewma` and `rtp / netem floor` |
| [f, c] | measured state: `servable_span(window, store)` (core-extraction §1.1) |
| ρ | declared dial (§3.4 says what must supply it) |
| Ab | measured (§4) |

## 2. The receiver floor

### 2.1 What the sender asserts

"Every seq below the floor is acked or has a verdict."

```text
   F  =  min { s ≥ A_rx : s ∉ V }          A_rx = the receiver's last advertised next_expected
```

F is `A_rx` advanced through the run of verdict seqs that starts at it. It is
defined from the verdict set and the ack, not from `retransmit_buffer`. The
reason is that `retransmit_buffer` is not filled on every ρ = 1 arm: the
insert at `net/emit_source.rs:498–506` is skipped in generation mode
(`if !pol.generation`). There, `min U` would read `hi + 1` with data
outstanding, so a floor would be sent at ρ = 1.

On the plain arm at ρ < 1 the two forms agree. Every seq in
`[A_rx, min U)` is in V, because `retransmit_buffer` is pruned only by the
cumulative ack and by verdicts. So `F = min(U ∪ {hi + 1})` there. The
verdict set below `hi + 1` and at or above `A_rx` is `shed_seqs` extended to
forced verdicts. `shed_seqs` is already pruned on the cumulative twin
(`net/sender_phases.rs:1404–1407`).

**At ρ = 1, `V = ∅` on every arm**, because:
- shedding is unarmed (`shed_armed` needs `!reliable`, and the budget is 0);
- the store holds every unresolved seq on the plain arm;
- generation mode is exempt from the size-cap eviction (`net/mod.rs:3594`);
- coded-only and OOO retain (config refuses them at ρ < 1, `config.rs:304`).

So `F = A_rx` identically, and the floor is never sent at ρ = 1. This is by
data, not by a seat check, and T3 witnesses it on the plain, generation and
coded-only arms.

### 2.2 When the sender sends it, and the ack

"While the floor is above the receiver's acked frontier, the sender repeats
it on the tail-sweep clock. The receiver's cumulative ack acknowledges it."

```text
   send WindowFloor{F} on every live path   at each tail-sweep fire  while  F > A_rx
   sweep armed  ⇔  U ≠ ∅  ∨  F > A_rx                      (today: U ≠ ∅)
   acked  ⇔  WindowAck.next_expected ≥ F
   repeats end  ⇔  acked  ∨  P_live = ∅  ∨  engine shutdown       (shutdown sends one last floor on the reliable stream)
```

- **The arming change is required.** A verdict on the last unresolved seq
  empties `retransmit_buffer`, which today disarms the sweep
  (`net/mod.rs:2788–2813`). That reproduces the 12 087 stall. With `F > A_rx`
  in the arming predicate, one condition covers both cases: data follows, and
  no data follows. At ρ = 1 the new disjunct is false by §2.1, so arming is
  unchanged there.
- **Termination is bounded in rate, not in count.**
  - **Liveness of a path rides the reliable stream.** A path leaves the live
    set when no `PathReport` arrives for `DEAD_PATH_TIMEOUT = 6 s`
    (`net/tasks/report.rs:161`, `scheduler/mod.rs:412–420`). The report task
    sends reports on the reliable control stream precisely so that liveness
    does not share fate with the datagram flood (`report.rs:105–115`).
  - **So a peer that is alive but whose acks never arrive keeps the path
    live, and the repeat continues.** That is correct: the floor is still
    unacknowledged. The claimed bound is a rate:
    `≤ |channel_paths| datagrams per TAIL_SWEEP_MIN_US`
    (`control_broadcast_paths` = `channel_paths`, `net/channel_set.rs:114`).
    The count is bounded only by the ack, the path's death (its reports
    stop: 6 s), or engine shutdown.
  - **The revision-1 text "sender exit held while F > A_rx" is withdrawn.**
    The sender loop ends only on engine shutdown (`net/mod.rs:3046–3065`).
  - **Shutdown.** One unreliable floor at shutdown can be lost, and that
    would leave a finite transfer's receiver on the hole for good: the
    receiver only logs `Shutdown`. So the final floor at shutdown goes on
    the **reliable** control stream (`send_control`, bounded by
    `io_owner::CONTROL_SEND_TIMEOUT`), the existing mechanism the reports
    use, before `Shutdown`. **[I]**: that a `WindowFloor` arriving on the
    reliable stream reaches the same receive arm as a datagram one was not
    read. The control-stream reader is the place to confirm it.
    - The stream is not FIFO with the datagram lane. Seqs below F still in
      flight then arrive as late fills, as in §2.3's N ≥ 2 case. That is
      stated, not hidden.
    - If the control send itself times out, the receiver stays on the hole.
      That is reported (a `floor_final_failed` count), not prevented.
- **Cadence.** The cadence is `tail_sweep_timeout_us(srtt) = clamp(2·SRTT,
  TAIL_SWEEP_MIN_US, TAIL_SWEEP_MAX_US)` (`net/recovery_laws.rs:331`). It is
  the existing law, so no new constant is introduced. A raise of F does not
  trigger an immediate send. Raises can come in bursts of thousands (every
  forced verdict raises F), and the receiver already passes holes that have
  data above them on its own hold.
- **Delivery after n repeats** has probability `1 − p_ctrl^n`, with `p_ctrl`
  the control datagram loss on the worst live path.
- **The ack already exists.** It is the bare `WindowAck` the receiver sends
  when its frontier moves with no arrival (`net/receiver.rs:1026–1047`). That
  ack is sent once and can be lost. So the receiver answers **every**
  `WindowFloor` with a bare `WindowAck` once its `next_expected ≥ F`. The
  sender's repeat then also repairs a lost ack.

### 2.3 The receiver's rule for passing a hole below the floor

"The newest floor heard on any path is treated as a payload-free arrival at
seq F, under the same hold and the same expiry as a data arrival."

```text
   F_rx  =  max floor received on any path          (0 before the first)
   on F_rx rising above next_deliver:  insert marker (F_rx, t = now) into the reorder hold
   drain_expired: the marker expires at t + H, exactly like a pending entry;
                  it passes the holes below F_rx and delivers the pending entries below it in order;
                  the frontier moves to F_rx (not F_rx + 1: the marker carries no payload)
```

- **Why the newest floor and not the minimum over paths** (revision 1 took
  the minimum).
  - **The minimum locks out.** At N = 2 a live but frozen idle leg that
    delivers nothing never raises its floor. The minimum then holds `F_rx`
    at 0 until that leg leaves the live set, which takes `DEAD_PATH_TIMEOUT`
    or QUIC's idle timeout. That is the known lock-out hazard class: one
    stuck member of a set gates the whole set.
  - **The newest floor's exposure equals today's.** A floor arriving on path
    p exposes the receiver to exactly what a data arrival at seq F on p
    already exposes it to today. The receiver's existing hold passes every
    hole below a pending entry after H, whichever path carried it. Seqs
    below F still in flight on a slower path then arrive as late fills.
    They are delivered at once, out of order (`net/reorder.rs:76–82`), and
    `G_rx` subtracts them (§4).
  - So the floor adds no exposure beyond what a data arrival at F already
    causes, and it cannot lock out. Per-path FIFO still protects the
    single-path case, which is the precheck's 8 000-in-flight case.
- **The in-flight bound is per-path FIFO.** A `WindowFloor` on path p is a
  datagram on the same connection and the same bottleneck queue as earlier
  data on p. So when it arrives, every seq below F sent on p has either
  arrived or been lost, up to reordering within the path.
  - **[E] The control lane does not overtake staged data on the sender's
    side.** `TxBatch` hands each path's slots over Data first, then Ctrl
    (`transport/io_owner.rs:1509`). The I/O owner then sends the commands in
    the order received, each command's datagrams in order
    (`:1088–1106`).
  - **[I] quinn's datagram send queue and the links are FIFO** (not read
    here). T5′ (single-path overtaking) tests it.
- The hold H covers that reordering. It is the receiver's existing hold
  (`shed_recv_hold`: `SRTT/2` while the receiver's budget is open,
  `(4·SRTT).clamp(60, 300) ms` otherwise). No new constant is introduced.
- On one path, the exposure is no larger than what a data arrival at seq F
  already causes today. This answers the precheck's contradiction: an
  eviction while 8 000 symbols were in flight on its one path. Those symbols
  precede the floor on that path, so they arrive before it. On N ≥ 2 the
  bound is the late fills above.
- **The marker arms the drain timer itself.** Today no timer runs without a
  pending entry (`net/reorder.rs:181–189`).
- **The pass reads no seat bit.** `drain_expired` returns early when
  `reliable` (`net/reorder.rs:117–119`). The floor pass therefore goes in its
  own function, which runs on both seats. At ρ = 1 it never fires, because
  floors are never sent (§2.1).
- **Edge.** A blackholed path cannot hold `F_rx` down, because the newest
  floor on any path is used. Its in-flight seqs are lost or arrive late, as
  they would today. Tested in §6, T5.

### 2.4 The wire change: yes, v11

The coordinator asked whether repair symbols already carry the coding
window's start. They do, but it cannot serve as the floor:
- **The full-window repair** stamps the window front:
  `[window_start(8)][window_count(2)][repair_index(4)]`
  (`fec/rlc_window.rs:11`, `:113–128`).
- **The trailing-span repair** (`generate_repair_range`, `:142`) stamps the
  span start. That is not the window start.
- **At ρ = 1 the window start is not a floor at all.** The store serves
  below it.
- **Using the header as the floor** would make the receiver's rule depend on
  the seat. It would also be silent exactly when no data follows, which is
  the case that stalls.

So the floor rides its own message:

```rust
ControlMessage::WindowFloor { floor: u64 }   // appended after RepairRequest (tag = declaration index)
WindowAck { .., passed_undelivered: u64 }     // §4: the receiver's abandonment count
```

- Both changes go in one bump, `PROTOCOL_VERSION` 10 → 11, with a v11 entry
  in the version-history comment (`transport/protocol.rs:25–104`).
- The change extends **ADR-0010**, the version header and the hard refusal
  of a mismatch.
- The rule itself, a new control message and a new ack field, is a
  hard-to-reverse protocol decision. It should get its own ADR, proposed as
  **ADR-0084, "Every verdict reaches the receiver"**. That ADR also amends
  ADR-0081's identity (§0), and its field supersedes the ack counter
  `cumulative_received` for this purpose.
- **`cumulative_received` cannot carry the count.** On in-order streams it is
  the per-path received count; on OOO streams it is the delivered-set size
  (`net/receiver.rs:2022–2028`). It is 0 on timer acks. So the sender cannot
  derive passed-undelivered from it.

### 2.5 Ablation: which test needs the wire change

The question: would a receiver drain timer alone pass the hole, with no
floor message?

- **The receiver has no evidence without an arrival above the hole.** Its
  frontier moves past a hole only on a pending entry above it, through
  expiry or force-drain (prechecks §2). Repair headers are evidence only if
  a repair arrives.
- **The lost tail (k = 11 800) had nothing above it.** `RWM_TEST_TS_DROP=dead`
  swallows every data send for 1.5 s from k's first send
  (`tests/sim_lost_tail.rs:1–4`, `src/test_seams.rs:78–104`). All of
  11 801..12 146 went out in that interval (the source ends at about 79 ms).
  The link trace's last data datagram is at 0.822 s. **[E]**: prechecks §3,
  "k has nothing above it at the receiver".
  - If the coordinator's note that "the 11 800 hole had arrivals above it"
    rests on another run, the two disagree. These are the precheck's
    numbers, on one seed.
- **But part 1 alone can create arrivals above k.** In the lost tail, k = 11 800
and the window is `[11947, 12146]` (prechecks §3). So 346 seqs lie above k:
146 below the window and 200 inside it. With the verdict judged before the
clamp, every one of them that is not shed is resent, and the first resend
above k is a pending entry above k. The receiver's own hold then passes k,
with no floor.

So the floor is needed exactly when every seq above the blocker gets a
verdict with no resend. Counted per branch at the candidate ρ = 0.99, where
the budget is `(1 − ρ)·S_src ≈ 121`:
- **Branch W.**
  - k and the 146 below-window seqs take forced verdicts. They count in
    `Ab`'s pending term at once: `Ab ≥ 147 > 121`.
  - The budget is therefore spent, all 200 in-window seqs are served, and
    the receiver passes k on its own hold.
  - T1 is green without the floor at any ρ whose budget is below 347.
- **Branch S.**
  - Nothing is forced. Up to 121 seqs are shed (k first, since the sweep
    targets `min U`), and the other ≥ 226 are served from the store.
  - The first resend above k gives the receiver its pending entry, so T1 is
    again green without the floor.
- **Either branch.** T1 needs the floor only when `(1 − ρ)·S_src ≥ 347`, that
  is ρ ≤ 0.971.

| test | budget | needs the floor? | why |
|---|---|---|---|
| T1 lost tail, ρ = 0.99 or budget 0 | ≤ 121 | no | some seqs above k are resent, and the receiver's hold passes k |
| T1 lost tail, ρ ≤ 0.971 | ≥ 347 | **yes** | every seq above k is shed or forced; nothing arrives above k |
| T2 stalled blocker, forced budget 0.05 (a seam) | ≈ 607 | **yes** | all 60 tail seqs are shed untold, which is `sr_evict_b05`'s 12 087 stall |
| T1′ the blocker is the object's last seq, lost under `dead` | natural (ρ = 0.99) | **yes** | nothing exists above it. Once shed (age > D, budget open) it can only be passed by a floor. If the budget refuses, it is resent and filled |

**The wire change's red evidence at a natural budget is T1′.** T2 needs a
forced-budget seam, and T1 needs ρ ≤ 0.971.

**The ablation to run.** Run the same suite under a seam that suppresses
`WindowFloor`, with the verdict rule on:
- T1 at ρ = 0.99 and at budget 0 is green (control);
- T1′, T2 and T1 at ρ = 0.97 are red;
- with the floor, all are green.

## 3. The source-rate bound

### 3.1 Why `reliable &&` is there (git history)

- **The gate came with the store.** `4c901536` (2026-07-06, "RWM Phase A")
  introduced it as `store_backpressure(reliable, store_len) = reliable &&
  store_len >= RELIABLE_STORE_MAX`. The doc comment reads: "EVICT mode never
  backpressures on retention (Realtime's correct spend of the budget)". It
  was a *store-full* pause, and the store existed only at ρ = 1. The gate
  records where the bytes were. It was not a flow-control decision for
  ρ < 1.
- **`cwnd_full` was added later without a reason for its scope.** `92e2f780`
  (2026-07-07, a generation-mode diagnosis) put it inside the same
  parenthesis, giving `reliable && (store_len >= store_max || cwnd_full)`.
  The commit says nothing about restricting it to the reliable seat.
- **The present form** is blamed to `ae83022e` (2026-09-27), the
  `effective_store_cap` rename.

So no derivation ever made the ρ < 1 source unbounded. It is a leftover of
where the bytes live.

### 3.2 The candidates, against four checks

The four checks:
1. **(i)** The form is exactly today's at ρ = 1 under the default gates.
2. **(ii)** It binds at ρ < 1 on the shipped arm.
3. **(iii)** It bounds unresolved seqs, not only bytes in flight.
4. **(iv)** Every constant in it has a source.

**(a) The congestion window gating intake at every ρ.** This is the simplest
form, with no ρ in it:

```text
   P  =  (O_store ≥ B)  ∨  F_cwnd ,      F_cwnd = ∀ i ∈ live: in_flightᵢ ≥ cwndᵢ      (no reliable &&)
```

- (i) It is exact only if the brake stays as armed as today, and today it is
  unarmed by default (`RWM_LATE_BRAKE` off, `RWM_INFL_CAP = 0`).
  - Left unarmed, it binds nothing at ρ < 1 on the shipped arm:
    `O_store = 0` (no store) and `F_cwnd = false`. That is core-extraction
    §1.3's redundancy-by-data.
  - Arming it by default at every ρ changes ρ = 1. The brake alone was never
    measured as a default; its only verdict came inside the composed cap,
    which was refuted in 2026-08 (paper §10).
  - Arming it at ρ < 1 only is a seat switch.
- (ii) It binds if armed. `in_flightᵢ` is maintained by `expire_in_flight` on
  every path (`net/mod.rs:2421`). That it is maintained on the evicting
  seat's send paths is **[I]**, and needs a witness.
- (iii) It fails. A lost seq leaves `in_flight` when it expires but stays in
  U, so the brake bounds the pipe and not the unresolved set.
- (iv) It passes: cwnd is derived by the CC.

**(b) The count of seqs not yet passed by the receiver's frontier, against the pooled cap at every ρ (recommended):**

```text
   P  =  (O ≥ B)  ∨  F_cwnd ,      O = (hi + 1 − A_rx) − |released| ,      B = §6.1's pooled cap, at every ρ
```

"The sender stops reading source while the seqs it has sent and the
receiver's frontier has not yet passed reach the pooled cap."

- **Why not |U|.** §1.1 removes every evicted seq from U, so at ρ < 1
  `|U| ≤ win_cap = 200` always. Work it on the precheck link: 40 Mbit at 512 B
  is about 9 800 sym/s, and at a 26 ms RTT the BDP is about 250 symbols, so
  `B ≈ (1 + q)·250 > 200`. `|U| ≥ B` would never be true. The source would
  stay unpaced, and a `|U| ≤ B` assertion would pass without the pause ever
  executing. Forced verdicts drain U, but they do not drain O: only the
  receiver's frontier does. O equals `|U| + |V_pending|` on the plain arm,
  where `V_pending = {v ∈ V : v ≥ A_rx}`, the term §4 already uses.
- (i) It is exact by data, with one pin. At ρ = 1 on the plain arm the store
  holds exactly `(ack, hi]` (core-extraction §1.1). Insertion is in seq
  order and pruning is by prefix on the cumulative twin. So
  `sent_store.len() = hi + 1 − A_rx` and `O = sack_release_outstanding(sent_store.len(), released)`,
  today's `store_len`.
  - **[I]**: this needs a per-tick equality pin at ρ = 1 before it ships,
    because `ack` and `A_rx` are read at different instants of the loop.
  - The generation and coded-only arms keep their own O. Both require
    ρ = 1 (`config.rs:304`).
  - `F_cwnd` and its arming are untouched.
- (ii) It binds. O grows whenever emission outruns the receiver's frontier,
  which is exactly the 44 ms unpaced emission of the precheck.
  - B is today's `effective_store_cap`. At ρ < 1 that needs `plain_dyn_cap`
    without its `reliable &&` (`net/sender_policy.rs:521`).
  - `plain_dyn_cap` has these readers, all on main `1ef0f634`. Each one then
    also runs at ρ < 1:
    - the cap select (`net/mod.rs:2464`);
    - the 5 ms cap refresh `refresh_store_cap` (`net/sender_phases.rs:62`);
    - the path-scaled pool echo (`net/mod.rs:1784`);
    - `three_term_on`, `sum_cap` and `delta_cap` (`net/sender_policy.rs:564`, `:579`, `:592`), of which three-term is off by default;
    - `percap_track` (`:686`, DIAG only).

    All of them belong to the one §6.1 law, which has no ρ input. None of them
    reads the store's bytes. **[I]**: this was read at the definitions, not
    traced through every consumer.
- (iii) It passes, and more than |U| would. O counts unresolved seqs plus
  verdict seqs the receiver has not resolved.
- (iv) Partly.
  - The law's terms are cited or derived (§6.1 table).
  - `knee` is a resource bound whose value is stale.
  - `RELIABLE_STORE_MAX = 1024` has no provenance ("≈ 10× the C2 BDP",
    `4c901536`). It does not ship as a law term: it is a memory ceiling, and
    it gets a bind gauge (§7).

It has no ρ and no δ beyond B's existing q(δ), so it is continuous by
construction. `released` is empty at ρ < 1 today, because SACK release
requires `reliable` (`net/sender_policy.rs:628`). O then over-counts the
seqs received above the frontier. That errs on the safe side; owner
question 8.

**The cap B on real links, and "about 250" against "1024".**
- **1024 is today's value at ρ < 1.** `plain_dyn_cap` is false there, so
  `effective_store_cap` falls through to `pol.store_max = RELIABLE_STORE_MAX = 1024`
  (`net/mod.rs:2464–2472`, `net/sender_policy.rs:503–509`). That is the
  "store cap 1024" the counters branch prints at ρ < 1. **[I]**: this
  attribution was not read on `recovery-counters` e6355bec. It is a constant
  with no provenance, and no law produces it.
- **"About 250" is the §6.1 law on honest inputs** (the link's rate and
  RTT). It is not what the engine computes.
- **On real links the pooled cap acts as a constant.** The store-law record
  (`docs/derivations/store-law-state.md` on branch `store-law-state`
  d1b703c9) shows the cap sitting at its ceiling N·2048 on 97–100 % of LATE
  ticks at every δ in the 152 sweep rows. The ask is 3–34× above the
  ceiling, because `btlbw` over-reads the link 6–19× and `rtp` the floor
  5–25×.
- **So at ρ < 1, (b) would read `O ≥ 2048·N` on the VM**, a constant
  outstanding bound of about 1 MiB per path at 512 B. It binds the precheck's
  unpaced burst (12 147 sources) but leaves a standing queue of up to
  about 2048 − BDP symbols. That is a reduction, not the law's
  q(δ)·RTprop. The sim's own anchors are not read here, so the sim's value
  of B is **[I]**.
- **The store law owns the fix**: a derived time law whose inputs have no
  engine source yet (store-law-state §1.2). This bound inherits that debt
  and names it. It does not fix it.
- **Bind gauge:** `[SUMCAP] pin=` (the N·knee ceiling) and `floor=` are
  reported at ρ < 1 as well, beside the paused share by disjunct (§7). A pin
  fraction near 1 is a MEASUREMENT DISCIPLINE 17 finding (a degenerate law),
  already open in the store-law record.

At an evicting receiver, A_rx also advances by hold expiry over holes. So O
is drained by the receiver's give-ups as well as by delivery, and the bound
cannot deadlock on a lost seq. At ρ = 1 a lost seq holds A_rx until it is
recovered, exactly as the store does today.

**(c) Pause before a forced verdict:**

```text
   P  =  (b)  ∨  [ evict would force a verdict on s_front  ∧  ¬(age > D ∧ Ab + 1 ≤ (1 − ρ)·S_src) ]
```

- It puts ρ into the bound and makes forced verdicts refusable.
- It fails on §0's finding: a forced verdict on an in-flight seq is not
  abandonment, so it throttles intake for a non-loss.
- It inherits the cumulative budget's burst. After a long clean run,
  `(1 − ρ − R)·S_src` of headroom lets the window run ahead by thousands.
  That is the precheck's queue overflow again.
- It is not recommended. It is the right form only if the owner wants forced
  verdicts refusable without a store at ρ < 1 (decision 1, branch S, makes
  that unnecessary).

**(d) A store at every ρ (paper §5.1).** `sent_store` fills on both seats and
an entry leaves on ack or on verdict. Then:
- every unresolved seq stays servable;
- `E ≡ 0`, and the FORCED arm is empty everywhere;
- (b)'s O equals the store's outstanding count on both seats;
- the `reliable` bool leaves the store, the clamp and the intake pause.

It is the form that removes the ρ = 1 step (§5). It is a larger behaviour
change: a per-source store insert at Realtime and store memory up to B. It is
decision 1 (§9).

**Recommendation: (b), together with decision 1 (default branch S, (d) in the
same change).** Under branch S, §1.1's forced arm stays as the rule's
closure and reads 0.

### 3.3 Which part of the budget the rate bound enforces

The rate bound reads no ρ, so it enforces no part of the 1 − ρ budget
directly. What it bounds is the *self-inflicted* term:
- `O ≤ B` would keep the standing queue near q(δ)·RTprop (Little's law,
  §6.1) if B tracked the link. On the VM, B is pinned at N·2048
  (§3.2 (b)), so the queue is bounded by that ceiling instead.
- That removes most queue-overflow losses: 1 110 against 91 random losses in
  the precheck, direction only.
- Those losses would otherwise become receiver abandonment (`G_rx`, §4) or
  forced verdicts.

The budget is enforced by the shed arm on `H` alone. Receiver give-ups and
forced verdicts on truly lost seqs cannot be refused. That is the overload
region of §3.5.

### 3.4 ρ as a number: what must supply it

- **Where ρ is missing.** The engine has no numeric ρ on this seat:
  `contract_rho: f64 = 1.0` is a literal (`net/sender_policy.rs:616`), read by
  no budget site. `window_reliable` is a bool (`config.rs:293–295`).
- **The source.** A declared config field, `window_rho: Option<f64>`, a dial
  next to `window_reliable`. It is carried in `SenderPolicy` as
  `contract_rho`, and its only reader in this law is the shed arm's
  `(1 − ρ)·S_src`.
- **Presets.** Bulk and Auto: ρ = 1. Realtime: **no value has a source in
  the repository.** Paper §4.10's 99.98 % is an achievable value computed for
  one cell, not a contract. No ρ < 1 preset ships until the owner declares
  one (owner question 4).
- **`reliable` is not derived from ρ ≥ 1.** That would be the forbidden
  threshold. The seat bool stays the existing debt (`w3-per-leg-state.md`)
  until branch S removes it from the sender (decision 1). The consistency of the two config values is checked
  at config time, not in a runtime branch (owner question 11).
- **The receiver** needs no ρ under this law: the sender does the counting
  (§4). Supplying ρ to the receiver's own give-up budget would need a
  handshake field, a second wire change (owner question 5).

### 3.5 The overload region (decision 2, not chosen here)

Let C be the source rate the live paths carry after the rate law's
correction share, `C = Σᵢ bwᵢ·(1 − ε̂ᵢ) / (1 + r)`. Let λ be the offered
source rate. Abandoning a share `a` of the offered seqs leaves a carried rate
`λ·(1 − a)`. The contract allows abandonment `a ≤ 1 − ρ`, and FEC residue is
predicted to use R of it (§4). So the sender can keep up with λ without
delaying the source and without breaking the contract only while

```text
   λ · ( 1 − (1 − ρ − R)⁺ )  ≤  C
```

Above that line the offered load exceeds what the dial can absorb. Exactly
one of these happens:
1. **The source is delayed.** The ρ contract holds and δ is violated: intake
   backpressure, which is (b)'s behaviour once the shed budget is spent. This
   is ADR-0064's "ρ wins over δ".
2. **The contract is broken.** δ holds and abandonment exceeds 1 − ρ: today's
   ρ < 1 behaviour, unbounded and unreported.
3. **Thin first, then delay.** Abandon at the source up to `(1 − ρ − R)⁺`
   (shed before the first send, which costs no wire), then delay. This needs
   a new source-side shed site.
4. **Break, but report.** As option 2, with the breach as a measured,
   printed reading.

**The rate bound and this answer are one decision.** (b) *is* option 1:
pausing intake when O ≥ B means delaying the source under overload. It
reverses `4c901536`'s stated intent ("EVICT mode never backpressures on
retention (Realtime's correct spend of the budget)"). That commit's
Realtime spent the abandonment budget, unbounded, instead of delaying the
source. The owner decides both together (§9, decision 2).

(b) ships option 1 for the sender-side part. Receiver give-ups and forced
verdicts can still push abandonment past 1 − ρ inside the region. With (b),
that is reported (`G_rx` against the budget, §7), not prevented.

### 3.6 The store fork: what a store at every ρ costs, and what the storeless stage buys

Forced verdicts exist only because the ρ < 1 seat keeps no retention store
(`net/emit_source.rs:346`: `if pol.reliable && !pol.generation`). The fork:

**Branch S (a store at every ρ).** The edits:
- Drop `pol.reliable &&` from the store fill.
- The serve reads the store first, then the window
  (`net/sender_phases.rs:985–996`, today `if reliable`).
- The clamp needs **no** step-4 store-span bound. With every unresolved
  seq in the store, the seat map's ρ = 1 bounds `(0, u64::MAX)` (core-
  extraction step 2, zero-change at ρ = 1) are correct on both seats. Gap
  seqs that are not in U are skipped before the serve (§1.2), so no
  already-acked or verdicted seq reaches the store lookup. Step 4, which is
  pinned and not zero-change at ρ = 1, is not needed.

Consequences:
- Every unresolved seq stays servable.
- The FORCED arm and `E` are empty by construction.
- O in (b) equals the store's outstanding count on both seats.
- **The sender-side behaviour step at ρ = 1 disappears.** At ρ = 1 − ε with
  `ε·S_src < 1` the sender sheds nothing and resends everything, exactly as
  at ρ = 1.

Cost per symbol at ρ < 1, read from the code:
- **The copy: none.** `sent_store.insert(seq, wire_sym.clone())` clones a
  `WireSymbol` whose payload is `Bytes`, "shared, not copied, by the encoder
  window, the sender's retention store and the wire send"
  (`fec/traits.rs:32–35`). Per symbol that is one atomic refcount
  increment, a struct move of about 50 bytes, and one `SeqRing` slot insert
  (O(1), insertion in seq order). The prune runs on the cumulative twin,
  which the slide already walks.
- **Memory: the residency beyond the window.** A symbol's bytes are already
  held while the 200-symbol window holds it (100 KB at 512 B). The store
  extends that until ack or verdict, so the extra resident bytes are at most
  `(O − 200)⁺ · 512 B`, bounded by B.
  - On the VM the cap is pinned at N·2048: at most about 0.9 MiB extra per
    path.
  - Under the law on honest inputs (about 250): about 25 KB.
  - At ρ = 1 the same store already runs, up to 2048 × 1200 B ≈ 2.4 MiB per
    path.
- **The cap's bind: no new clamp.** The store has no cap of its own beyond B
  (backpressure, never eviction). B already binds O in (b) on both
  branches, so branch S adds no bind that (b) does not already have. The
  memory ceiling `RELIABLE_STORE_MAX` binds only where the dyn cap is off
  (`RWM_STORE` set). Its bind gauge is in §7.
- **Behaviour change.** A below-window hole that the budget refuses to shed
  is resent from the store, instead of taking a forced verdict. This
  intends more resends at Realtime when the budget is spent: ρ wins over δ,
  as ADR-0064 says.

**Branch W (the storeless stage, this document's §1.1 as written).** It keeps
the forced arm and ships the sender-side ρ = 1 step, which CLAUDE.md calls a
defect. It buys, beyond sequencing:
- **A smaller engine diff.** The serve path, the clamp bounds and the
  path-map floor keep their `reliable` reads.
- **No per-symbol store insert at Realtime.** The cost is the one refcount
  increment and ring insert above, which are not measured in the ns bench.
  This is the one cost S adds on the hot path.
- **No extra memory beyond the window**: at most about 0.9 MiB per path on
  the VM.

It costs:
- forced verdicts on in-flight seqs that over-charge `Ab` (§0);
- **shedding switched off mid-stream.** The pending term holds about
  `(O − 200)⁺` forced verdicts while the source runs ahead, about 1 850 at
  the pinned B. At ρ = 0.99 the budget covers that only after
  `S_src ≈ 185 000`. Until then every shed is refused (§9 decision 1, T6);
- a forced arm with its own gauges and tests (T8);
- a step at ρ = 1 that every later ρ test must bound.

**Assessment.** Branch W buys a smaller first diff and avoids one atomic
increment per source symbol at Realtime. It buys nothing in correctness,
and it ships a known step.

**A step remains on the receiver in both branches.** The reorder buffer's
`reliable` never passes a hole at ρ = 1 (`net/reorder.rs:117–119`), while at
ρ = 1 − ε the receiver still gives up on its ε̂_recv budget. Removing that
step needs a receiver law that never passes a hole at ρ = 1. A spent
budget today only lengthens the hold, and the hold still passes. So that
law is a separate derivation, likely with ρ in the handshake (owner
question 5, default: not in this change).

## 4. The 1 − ρ budget law

"Abandonment, counted where it happens plus the verdicts not yet resolved,
stays within the declared 1 − ρ of the source. The FEC residue R is the
predicted use of that budget, not its size."

```text
   G_rx    =  |passed holes|  −  |arrivals that fill a passed hole|      (not every arrival below the frontier)
   Ab      =  G_rx  +  |{ v ∈ V : v ≥ A_rx }|          (verdicts the receiver has not yet passed or acked)
   shed admissible  ⇔  age > D  ∧  Ab + 1 ≤ (1 − ρ) · S_src
   forced verdicts:  counted in Ab, never refused
   predicted use:   û  =  R  =  ε̂ · (1 − P_fec(r_live, ε̂, A*, σ²_burst))
   contract (end of run):  G_rx  ≤  (1 − ρ) · S_src
```

- **Ab is an upper bound that converges to the truth.** A pending verdict
  either becomes a hole (and moves into `G_rx`) or is delivered (and leaves
  Ab) once the receiver's frontier passes it.
- **No double count.** One `WindowAck` carries both `next_expected` and
  `passed_undelivered`. A verdict seq the receiver passes leaves the pending
  term (it falls below `A_rx`) in the same update that adds it to `G_rx`.
  Between acks it sits in the pending term only. An ack lost in transit
  delays the move without duplicating it, because both fields are
  cumulative.
- **What this replaces.** It supersedes `budget = min(1 − ρ, R)` (paper §5.6;
  model option (ii), owner decision 2026-11-14, `giveup-law-model.md`). The
  engine evaluates that law today as budget = R on the evicting seat:
  - cold (R = 0 before the first ε̂);
  - frozen (refreshed only on source emission);
  - ρ-blind (liveness audit item 4).
  The new law is never cold: 1 − ρ is declared, and S_src and Ab are
  counters.
- **The model must move too.** The cartoon and wasm give-up law change to
  budget = 1 − ρ. That is a deliberate model change with a
  `GOLDEN_CAPTURE=1` re-capture. The visualizer anchor "= R when 1 − ρ ≥ R"
  (`test_visualizer_ui.mjs:238–241`) becomes "= 1 − ρ". This is owner
  question 3, raised and not assumed.

**The receiver counter exists but is incomplete.** `recv_shed_holes`
(`net/receiver.rs:195`, `:980`, `:1767`):
- counts the frontier's advance minus the entries delivered, at the two
  `drain_expired` sites only;
- misses `force_drain_oldest` (`net/reorder.rs:149–163`) and the new floor
  pass;
- never subtracts late fills below the frontier (`net/reorder.rs:76–82`);
- is gated on `recv_shed_on`.

`G_rx` is that counter extended:
- to all three frontier writers plus the floor pass;
- with fills of passed holes subtracted. That needs the set of passed holes
  (a seq set, pruned at the decode-inert floor, `net/receiver.rs:2236`). An
  arrival below the frontier that fills no passed hole is not subtracted:
  for example the Realtime duplicate source on a second path
  (`net/emit_source.rs:508`) of a seq already delivered. A fill arriving
  after its hole was pruned is not subtracted. That errs on the safe side,
  and a gauge counts it;
- with no gate.

It is carried on every `WindowAck` as `passed_undelivered`.

`undeliverable_symbols` (`net/mod.rs:1452`, the F2 counter) counts symbols
that fail framing. It is unrelated.

**The receiver's own give-up budget stays.** It is `shed_recv_budget_ok`:
`holes < ε̂_recv × frontier` (`net/shed.rs:389`). It has no ρ (owner
question 5), and it is an existing law selection by budget state inside
`shed_recv_hold`.

| symbol | provenance |
|---|---|
| G_rx | measured at the receiver; reported on the ack (v11) |
| V, A_rx, S_src | measured sender state |
| 1 − ρ | declared dial |
| R | derived, §5.6. It inherits W22's `p_fec_normal` cut and is provisional. Here it is a gauge, not an operand |

### 4.1 The formulas as core functions

| formula | core function (`raptorpath-core/src/hole.rs`) | relation to `core-extraction-recovery.md` |
|---|---|---|
| §1.2 verdict | `judge_seq(age_us, d_us, ab, s_src, budget_frac, servable: bool) -> Verdict {Shed, Serve, Forced}` | new. **Breaks decision 5** (clamp-then-shed order stays at the call site): the shed is judged before the clamp |
| §1.2 serve bound | `clamp_gap(s, e, f, c)` unchanged | unchanged signature; it now runs after `judge_seq` |
| §1.1 forced verdict | none; it is an engine event at the advance sites, counted | — |
| §1.3 D | `shed_deadline_us(b, rtt_us)` (moves with ADR-0076 phase 6) | refresh site changes only |
| §2.1 floor | `floor(acked: u64, verdicts_from_acked: impl Iterator<u64>) -> u64` | new |
| §1.2/§2.2 sweep | `tail_sweep_gap(min U)` unchanged; new `sweep_armed(lowest_unresolved: Option<u64>, floor, acked) -> bool` | arming was inline (`net/mod.rs:2788`); it now needs the floor |
| §2.3 receiver | `floor_rx(per_path: &[u64]) -> u64` (max, the newest floor; 0 for an empty slice) | new, receiver side; N = 1 and N = 2 absolute tests |
| §3.2 (b) | `intake_paused(O, B, F)` unchanged signature | **breaks §1.3's map and its redundancy pin**: at ρ < 1 the engine passes (O, B, F) instead of (0, ∞, false), so the §3.3 pin is retired. The model's `intake_paused(O_retained, B, false)` call must pass the frontier-covered O |
| §4 budget | `shed_budget(rho, s_src) -> f64 = (1 − ρ)·s_src` and `abandon_count(g_rx, pending) -> u64` | new; replaces the R operand of `shed_allowed` |

Class A holds for all of them: no hint, no δ/ρ- or N-selected branch, no env
read. `judge_seq` reads ρ only as the number `budget_frac`.

## 5. Continuity

The formulas:
- **The verdict rule** reads ρ only through `(1 − ρ)·S_src`, which is linear
  in ρ with slope `−S_src`. It reads δ only through D's `b(δ)`, which is
  continuous (§5.4).
- **The floor, F_rx and the rate bound (b)** read neither ρ nor δ. The only
  exception is B's `q(δ)`, which already has its preset tests (§6.1).
- **The budget** `1 − ρ` is Lipschitz-1 and is 0 at ρ = 1.
- No term selects a law by a threshold on δ or ρ.

**The behaviour still steps at ρ = 1, and this is a defect, not shown
continuity.** Retention is a bool. At ρ = 1 − ε:
- the budget admits fewer than one shed while `S_src < 1/ε`;
- yet the seat has no store, so forced verdicts still happen.

At ρ = 1 the store prevents them. In the limit ρ → 1⁻ the evicting seat
therefore abandons seqs that the ρ = 1 seat delivers. Branch S
(decision 1) removes the sender-side step. Branch W keeps it, and a test
bounds it. The receiver-side step (§3.6) remains in both branches, and the
same test bounds it
(T8).

The ±2 % tests to write. Each is absolute, at each preset, in
`raptorpath-core` where the function lives:
- **`shed_budget(ρ, S)`** at ρ ∈ {ρ_p, ρ_p·0.98} and, for ρ_p < 1,
  ρ_p·1.02 clipped to 1. Assert `|Δ budget| = S·|Δρ|` exactly, and that
  `budget(1) = 0`. The nudge at ρ = 1 is one-sided.
- **`D(δ)`** at δ_p·(1 ± 0.02) for Bulk, Auto and Realtime through
  `delta_budget_b_of`. Assert `|D(δ_p·(1 ± 0.02)) / D(δ_p) − 1|` is within
  the tolerance of the existing b(δ) preset test.
  - Note: the engine reads b at the hint's named point (`delta_budget_b(hint)`,
    `net/emit_source.rs:619`). The test therefore runs on the δ-numeric form
    the core takes.
- **`judge_seq`** at fixed state across the same nudges. The verdict may
  change only where `age − D` or `(1 − ρ)·S − (Ab + 1)` changes sign. That
  is a state comparison, not a dial threshold: assert no other flip.
- **`floor_rx(per_path)`** (max) at N = 1 and N = 2: absolute values, including a
  path that has received no floor yet (F_p = 0).
- **`floor(acked, verdicts)`**: absolute values. `floor(a, ∅) = a` (the ρ = 1 identity), a run of verdicts from a gives `a + run`, and a gap in the run stops it.

## 6. Red-first tests

General rules for every test below:
- Liveness at ρ < 1 is asserted on the **frontier and the ack**, never on
  object completion. At ρ < 1 every abandoned seq fails `perf`'s
  `got.len() == total` (`perf.rs:144`), so `client_ok` stays a ρ = 1 oracle
  only.
- The frontier is read from the sender's received `WindowAck.next_expected`
  (a new test-seams hook, or `[ACCT]`). It is not read from `ev=frontier`,
  which prints only on arrival and falls silent when arrivals stop.
- **Order.** Each test asserts the frontier and ack bound **first**, then the
  account. Completion is asserted only in the ρ = 1 control. That way the
  frontier assertion executes (the current `assert_live` fails earlier on
  completion, so its frontier loop never ran).

Execution witnesses carried by every test:
- verdict-site visits (shed, serve, forced);
- `floor_sent`;
- `floor_passes` at the receiver;
- `G_rx`.

The tests:
- **T1 lost tail** (`sim_lost_tail`, k = 11 800, ρ < 1).
  - **k is passed** by `t_dead + 2·T_sweep_max + owd_max + H_max`. That is
    one sweep for k's verdict and one for the first resend or floor above
    it. t_dead is when the seam stops dropping, and
    `T_sweep_max = TAIL_SWEEP_MAX_US`.
  - **The ack reaches `S_src`** by
    `t_dead + (n_tail + 1 + n_drop)·T_sweep_max + owd_max + H_max`, where:
    - `n_tail = 346`, the seqs above k, fixed by the scenario;
    - n_drop is the number of floors the scenario's seam drops.

    The sweep resends one seq per fire in the worst case (precheck
    `sr_evict`'s walk). NACK batches only make it faster.
  - Every term is fixed by the scenario; none is read from the run.
  - **The scenario's limit must move.** `TAIL_SWEEP_MAX_US = 100 ms`
    (`net/mod.rs:251`), so `347 × 100 ms = 34.7 s` exceeds today's
    `limit_s: 30`, and the ack bound could never bind. T1 raises `limit_s`
    to 40.
  - **T1 needs the floor for the 2-sweep bound, not for liveness.** In the
    floorless ablation control at ρ = 0.99 under branch S, the sweep sheds
    k and then one seq per fire until about 121 are shed. The first resend
    above k comes only after that. The control's own bound is therefore
    `t_dead + 122·T_sweep_max + owd_max + H_max` for k's pass (about
    12.2 s). Under branch W the forced verdicts spend the budget at once.
    The first in-window seq is resent at the first sweep, and k passes in 2
    sweeps without the floor.
  - k has a verdict (shed, or forced), or is filled.
  - `floor_sent ≥ 1` and `floor_passes ≥ 1`.
  - Red on main: the ack stays at 11 800.
- **T2 stalled blocker** (`sim_shed_reach`, forced budget 0.05).
  - The ack reaches 12 147.
  - Every shed seq is below the final ack by `t_s + (1 + n_drop)·T_sweep_max + owd_max + H_max`. The shed tail is passed by one floor.
  - Red on main: the ack stays at 12 087.
- **The replacement for `assert_live`.** `assert_frontier_live(r)`, in this
  order:
  1. ack = S_src within the bound above;
  2. for each verdict seq s: frontier > s by `t_v(s) + (1 + n_drop)·T_sweep_max + owd_max + H_max`;
  3. the account closes, `S_sent = A + V + |U|`, with `|U| = 0`;
  4. `G_rx ≤ Ab` at the end;
  5. `G_rx` and `(1 − ρ)·S_src` are printed. They are asserted only where a
     budget was declared.
- **T3 ρ = 1 control.** The same scenarios with `SIM_RELIABLE=true`, on the
  plain arm and on the generation and coded-only arms (F must not read
  `retransmit_buffer`, §2.1):
  - `V = 0`, `floor_sent = 0`, `G_rx = 0`;
  - the arming disjunct `F > A_rx` is never true;
  - `client_ok`.
- **T4 floor loss and ack loss.**
  - A seam drops the first n `WindowFloor` datagrams: T1 stays green with
    n + 1 sweeps.
  - A seam drops the bare `WindowAck`: the sender repeats, the receiver
    re-acks, and repeats stop at the ack.
  - **Never-ack variant** (acks dropped, reports still flowing): the path
    stays live. Assert the repeat rate
    `≤ |paths| per TAIL_SWEEP_MIN_US` and that repeats continue. This is
    the bound that is reachable here.
  - **Dead-peer variant** (a second seam stops `PathReport` too): repeats
    stop within `DEAD_PATH_TIMEOUT + T_sweep_max`.
  - **Shutdown variant:** exactly one final floor per run, on the reliable
    stream, before `Shutdown`. With the datagram lane dropped entirely, the
    receiver still passes the hole after the final floor plus H.
- **T5 multipath.** All cases run at N = 2.
  - **(a) The frozen idle leg (lock-out hazard).** One leg is live but
    delivers nothing: it is idle, or blackholed with its connection open.
    - The receiver passes every hole below F within
      `H + owd_fast + T_sweep_max` of the verdict.
    - The pass does **not** wait for that leg's `DEAD_PATH_TIMEOUT`.
    - Red against the revision-1 minimum rule; green against the newest-floor
      rule.
  - **(b) A slow leg in flight.** The slow leg still carries seqs below F
    when the fast leg's floor lands. Those seqs arrive as late fills.
    - Assert `G_rx` excludes them.
    - Assert their count is at most the slow leg's in-flight count at the
      floor's arrival.
    - The count is reported, not pinned.
- **T5′ single-path overtaking** (N = 1). A verdict raises F while seqs
  below F are still queued on the one path. The receiver passes none of
  them, which tests §2.3's FIFO premise.
- **T6 budget.** At a declared ρ < 1:
  - sheds stop when `Ab + 1 > (1 − ρ)·S_src`, and further candidates are
    served (the refusal counter rises);
  - R and `G_rx / S_src` are printed side by side;
  - under branch W, the pending term's size and the share of shed checks
    refused while it alone exceeds the budget are reported. That is the
    mid-stream shed-off cost of decision 1.
- **T7 rate bound.**
  - At ρ < 1 with the unpaced source: the pause **executes**, as a
    paused-iteration share > 0 on the `O ≥ B` disjunct (an execution
    witness, read first). Then `O ≤ B + 1` at every DIAG tick
    (`O_hwm ≤ B + 1`), and `drop_queue` falls against main. The direction is
    asserted, not the size.
  - At ρ = 1: the per-tick pin `O == sack_release_outstanding(sent_store.len(), released)`.
- **T8 the ρ = 1 step.** The same seed at ρ = 1 and at ρ = 1 − ε with
  `ε·S_src < 1`.
  - **Sender observables**: verdicts, per-seq sends and resends.
    - Under branch S: asserted identical (always-on).
    - Under branch W: recorded, with only `H = 0` asserted.
  - **Receiver observables** (passes, `G_rx`): record-only in both branches.
    The receiver's `reliable` step remains (§3.6).

## 7. Bind gauges

| clamp or min | gauge | expected |
|---|---|---|
| serve clamp `clamp_gap` on U members | gap ∩ U seqs clamped out per call, with call count | 0 after §1.1 (`O_strand ≡ 0`); non-zero is a defect |
| FORCED arm at the NACK and sweep sites | visits vs forced | 0 forced |
| forced verdicts at the eviction sites | `E`, per site, with visit count | branch S: 0 at every ρ; branch W: > 0 at ρ < 1, 0 at ρ = 1 |
| D's `min(b·RTprop, 2·RTprop)` | share of judged batches where 2·RTprop binds | 0 at b ≤ 2 (all presets); reported |
| D's anchor | `D_rtprop / D_ewma` per judged batch, both computed, one used | reported; the ratio is the queue share D drops |
| D cold (`D = 0`, no measured RTT) | count of judged batches with D = 0 | early-run only |
| shed admission | refusals split by the deadline conjunct and by the budget conjunct | reported per run |
| intake pause (b) | paused-iteration share split by `O ≥ B` and by `F_cwnd` | reported (`[DIAG] paused=`); > 0 at ρ < 1 under overload |
| B's clamp | `[SUMCAP] floor=` / `pin=` (existing), plus `RELIABLE_STORE_MAX` binds | the memory ceiling should not bind |
| newest floor over paths | late fills of floor-passed seqs per pass, by path | reported (the N ≥ 2 exposure) |
| floor repeat | sends per `F` raise, until acked | ≈ 1 + p_ctrl/(1 − p_ctrl) |
| receiver hold H | `SHEDH` at_floor / at_cap / interior (existing) | reported |
| Ab pending term | verdict seqs at or above `A_rx`, against `G_rx` | reported |
| floor at ρ = 1 | `floor_sent`, and sweep fires armed only by `F > A_rx` | 0 |

## 8. Paper text

The exact paragraphs follow. Each is placed after the named anchor. The
coordinator applies them with the engine change, not before.

**§5.1, appended after the paragraph ending "ρ is not a runtime input
anywhere in the engine (Section 11.1).":**

> *Verdicts.* A sent source seq leaves the sender's unresolved set U in one
> of three ways: acknowledgement, a shed (Section 5.6), or a forced verdict
> when its last servable copy is gone. Where no store retains (the evicting
> seat before the store runs at every ρ), the coding window
> is that copy, so a window eviction of an unresolved seq is a verdict at
> that instant.
>
> ```text
>    S_sent  =  A  +  H  +  E  +  |U|            each seq in exactly one term
> ```
>
> A verdict is a sender fact ("I will not serve it"). Abandonment is a
> receiver fact ("the frontier passed it undelivered"). The contract counts
> the second (Section 5.6). At ρ = 1 the store holds every unresolved seq, so
> E = 0 by data. With a store at every ρ, E = 0 on every seat. Without it,
> the seat is a step at ρ → 1⁻ (Section 11.3).
>
> *The floor.* Every verdict reaches the receiver as a floor below which
> every seq is acknowledged or given up:
>
> ```text
>    F  =  min { s ≥ A_rx : s ∉ V }          A_rx = the acknowledged frontier
> ```
>
> The sender repeats `WindowFloor{F}` on every live path at the tail-sweep
> cadence while F exceeds the receiver's acknowledged frontier. The
> receiver's cumulative ack is its acknowledgement. The receiver takes the
> newest floor heard on any path and treats it as a payload-free arrival at
> F under its ordinary hold, so it exposes no more than a data arrival at F
> does. On one path, per-path FIFO makes every earlier seq arrive or be lost
> first. On several paths, late seqs are late fills, as for data today.
> At ρ = 1 no verdict exists on any arm, so F equals the acknowledged
> frontier and the floor is never sent.

**§5.6, replacing the law block and the paragraph after it ("R is derived;
… (Section 11.3)."):**

> ```text
>    D(δ)    =  min( b(δ)·RTprop , 2·RTprop )            read at the verdict; the engine reads the EWMA RTT until a battery measures RTprop (Section 11.3)
>    G_rx    =  seqs the receiver passed undelivered     (measured at the receiver, on the ack)
>    Ab      =  G_rx  +  #verdicts at or above the acknowledged frontier
>    shed  ⇔  age > D(δ)   ∧   Ab + 1  ≤  (1 − ρ)·src_total
>    R       =  ε̂ · (1 − P_fec(r_live, ε̂, A*, σ²_burst))    predicted use of the budget
> ```
>
> The budget is the declared 1 − ρ. It counts abandonment where it happens,
> at the receiver, plus the verdicts the receiver has not yet resolved. R no
> longer sizes it: R predicts how much of it FEC's residue will use, and
> R > 1 − ρ marks an operating point where the contract can be met only by
> delaying the source. A forced verdict is counted and never refused, and
> past-budget sheds are served: ρ wins over δ. This supersedes
> budget = min(1 − ρ, R) (owner decision 2026-11-14). At ρ = 1 the budget is
> 0 at every δ.

**§6.1, replacing the sentence "The engine also has no ρ < 1 seat for it:
… never evicts above its frontier.":**

> The source bound runs at every ρ on the count of sent seqs that the
> receiver's frontier has not yet passed, net of SACK-released ones:
>
> ```text
>    pause  ⇔  O ≥ cap  ∨  cwnd_full ,      O = (hi + 1 − A_rx) − |released|
> ```
>
> At ρ = 1, O equals the store's outstanding count (pinned), so the bound
> is the one above. At ρ < 1 it is the first outstanding bound the evicting
> seat has had. It has no ρ input. Past the offered load
> λ·(1 − (1 − ρ − R)⁺) > C, the source is delayed or the contract breaks
> (Section 11.1).

**§11.1, a new open question:**

> *The overload region.* When λ·(1 − (1 − ρ − R)⁺) exceeds the carried rate
> C, the dial cannot hold both δ and ρ. The engine delays the source once the
> shed budget is spent. Receiver give-ups and forced verdicts can still
> exceed 1 − ρ there, and they are reported, not prevented. Thinning at the
> source within 1 − ρ − R is not implemented.

**§11.3, two new divergence rows** (the table's columns are item | model |
engine):

> | retention at ρ → 1⁻ | a store at every ρ (Section 5.1): no forced verdicts, continuous through ρ = 1 | branch S: the same, so no divergence on the sender. Branch W: the evicting seat has no store, so forced verdicts occur at ρ → 1⁻ and not at ρ = 1, bounded by the step test |
> | receiver give-up at ρ → 1⁻ | (not modelled) | the reorder buffer never passes at ρ = 1 and passes on its ε̂_recv budget at ρ < 1: a step, bounded by the step test's receiver observables (record-only) |
> | the shed budget | `1 − ρ`, with ρ the declared dial | `1 − ρ` from `window_rho`; no ρ < 1 preset is declared at Realtime, so its budget is 0 until one is |

**§11.1, row 2 ("Is ρ a runtime dial?"), appended to its "what would
decide it" cell:**

> The shed budget now reads ρ as a number (Section 5.6); the store, the
> clamp and the receiver hold still read the seat.

**Sentences this text makes false, to rewrite in the same commit:**
- §5.1: "ρ is not a runtime input anywhere in the engine (Section 11.1)".
  ρ now enters the shed budget.
- §4.10, last paragraph: "In the engine ρ < 1 exists only as the evicting
  Realtime seat (Section 5.1); T_cut is not a runtime input". The first
  half stays true of the seat, and ρ is now an input of the budget.
- §5.6: "The engine evaluates the law at two seats only: the evicting seat
  runs budget = R, the retain seat budget = 0". The paragraph is replaced
  above.
- §5.6: "inside the probabilistic retransmit branch". That branch was
  deleted with ADR-0079, so the budget is evaluated at the verdict sites.
- §5.6: "The retain (ρ = 1) seat never sheds; the law runs only on the
  evicting seat". It becomes "at ρ = 1 the budget is 0, so nothing is shed".
- §6.1's deleted sentence also carried the §6.3 caveat ("the release bound
  of Section 6.3 assumes a receiver that never evicts above its frontier").
  It must move to §6.3, since O is drained by the receiver's give-ups at
  ρ < 1.
- The §11.3 row "receiver shed hold" is unchanged. The §11.3 row "span
  deadline D and A*" changes if owner question 7 takes RTprop: its engine
  cell becomes "D on the windowed-min RTprop at the verdict sites; A* still
  on the EWMA" (A* is not part of this change).

**Appendix A.4, replacing the shed line:**

> `shed ⇔ age > D(δ) ∧ G_rx + V_pending + 1 ≤ (1 − ρ)·src_total`;
> `F = min{ s ≥ A_rx : s ∉ V }`; `pause ⇔ (hi + 1 − A_rx − released) ≥ cap ∨ cwnd_full` (every ρ).

## 9. Open questions for the owner

**Answered 2026-11-20 (owner): every recommended default below is accepted.** This means:
- decision 1 is branch S, a store at every ρ, in the same change;
- decision 2 is to delay the source under overload, with the breach printed;
- question 3 supersedes `min(1 − ρ, R)` with `1 − ρ` (model change, golden re-capture in its own commit);
- the Realtime preset declares ρ = 0.99 as a named point (declared, not derived);
- the protocol v11 `WindowFloor` message is approved, recorded in ADR-0084;
- questions 5–12 take their defaults.

Each question has a recommended default. The first two are decisions that
shape the engine change. The rest are smaller.

**Decision 1: the store fork (§3.6).**
- **Branch S, a store at every ρ.**
  - What changes: drop `reliable` from the store fill, the serve and the
    clamp bounds.
  - What it removes: the forced arm (E ≡ 0) and the sender-side ρ = 1 step.
    T8's sender observables become always-on. The receiver-side step
    stays (question 5).
  - Costs at ρ < 1: no byte copy (shared `Bytes`); one refcount increment
    and ring insert per symbol; extra memory at most `(O − 200)⁺·512 B`,
    about 0.9 MiB per path at the VM's pinned cap; no new clamp.
  - Behaviour change: below-window holes are resent instead of given up
    once the budget is spent.
- **Branch W, the storeless stage, as §1.1 is written.**
  - What it keeps: forced verdicts, with `E` over-charging `Ab` on in-flight
    seqs. It ships the ρ = 1 step as a defect bounded by T8.
  - **Hidden cost: it switches shedding off mid-stream.** While the source
    runs ahead, every in-flight seq the window passes takes a forced verdict
    and sits in `Ab`'s pending term until the receiver passes it. With O up
    to the pinned B = 2048 and a 200-symbol window, that is about
    `(O − 200)⁺ ≈ 1 850` pending at steady state. At ρ = 0.99 the budget
    `(1 − ρ)·S_src` reaches that only at `S_src ≈ 185 000` symbols, about
    95 MB at 512 B. Until then every shed is refused, so δ-honest shedding
    is effectively off on a W stream of that size.
  - What it buys: a smaller first diff and no per-symbol insert at Realtime.
- **Default: branch S, in the same change.** The storeless stage buys only
  sequencing and one atomic increment per symbol, while it ships a defect
  the standing rules forbid. Under S, §1.1 and the FORCED arm remain as the
  rule's closure, with gauges that must read 0.

**Decision 2: the rate bound and the overload answer (§3.2 (b), §3.5).**
- **The bound:** `pause ⇔ (hi + 1 − A_rx − released) ≥ B ∨ cwnd_full` at
  every ρ, with B the pooled cap at every ρ. It has no ρ.
- **What it means:** delaying the source under overload, option 1 of §3.5.
  It reverses `4c901536`'s stated intent ("EVICT mode never backpressures on
  retention").
- **On real links B is pinned** at N·2048 (store-law-state). So the bound
  acts as a constant outstanding cap until the store law's time form lands.
  Its bind gauge is `[SUMCAP] pin=` at ρ < 1.
- **The alternatives:**
  - break the contract (today);
  - thin at the source within `1 − ρ − R`, then delay (needs a new
    source-side shed site);
  - break and report.
- **Default: delay**, plus a printed breach reading for the abandonment
  that receiver give-ups and forced verdicts still cause.

**Smaller questions:**
3. **Supersede `min(1 − ρ, R)` with `1 − ρ`, R as predicted use.** **Default:
   yes.** It is a deliberate model change: wasm give-up law, visualizer
   anchors, and a `GOLDEN_CAPTURE=1` re-capture, in its own commit.
4. **The Realtime preset ρ.** The repository has no source for it.
   **Default:** the owner declares a named point. 0.99 is a candidate: it
   leaves room above the measured 0.36 % shed share (liveness audit item 4)
   and is labelled declared, not derived. **The consequence of no
   declaration:** the budget at Realtime is 0, which means serve and never
   shed. That switches off ADR-0064's shipped δ-honest shedding, which its
   flip battery rested on (0 collapse reps, 0.25 % shed). So "undeclared"
   is not a neutral default: Realtime would ship without shedding until a ρ
   is declared.
5. **ρ at the receiver.** The receiver's give-up budget reads ε̂_recv, and its
   reorder `reliable` is a second ρ = 1 step (§3.6). Making the reorder
   `reliable` redundant by data needs a receiver law that never passes a
   hole at ρ = 1. Today a spent budget only lengthens the hold
   (`shed_recv_hold`'s 4·SRTT fallback) and still passes. So a ρ-budgeted
   receiver is a separate derivation, not a one-line change. **Default: no,
   in this change.** The sender counts `G_rx` and reports breaches, and the
   receiver law is derived next, with ρ in the handshake if it needs it.
6. **Floor cadence.** Send on the tail-sweep clock only, or also immediately
   on a raise? **Default: the sweep clock only.** Raises can come in bursts,
   and holes with data above them pass on the receiver's own hold.
7. **D's RTT (§1.3).** ADR-0064's derivation (`6568822c`) used RTprop, the
   span law's D. The engine passes the EWMA RTT, which carries the
   self-inflicted queue into D (positive feedback under overload). The
   derived form has **never been measured**: the flip battery ran the EWMA,
   and the RTprop anchor itself over-reads on the VM.
   **Default:** compute both at the verdict sites and ship the measured
   EWMA. Log `D_rtprop / D_ewma` and move D to RTprop in its own change,
   after a battery reads it. The EWMA stays only until that reading. It is
   a known divergence from the derivation, carried in §11.3, not a second
   law.
8. **SACK release at every ρ.** It would make O exclude seqs received above
   the frontier. **Default: no for now.** Over-counting errs on the safe
   side; revisit with T7's `O_hwm` reading.
9. **Arm the cwnd brake by default at every ρ, (a).** **Default: no.** It
   changes ρ = 1, the brake alone has no verdict as a default, and (b) binds
   without it.
10. **A cumulative or a windowed budget.** The shed law's cumulative count
    admits bursts after long clean runs. **Default: keep it cumulative**
    (ADR-0064's form, no new constant) and gauge the largest shed burst.
11. **The config pair (`window_reliable = false`, ρ = 1).**
    - **Under branch S** the sender is harmless at that pair: the store
      retains and the budget is 0. The receiver's reorder still gives up on
      its ε̂_recv budget, so the contract can still break on the receiver
      side.
    - **Under branch W** forced verdicts break it on the sender side too.
    - **Default: refuse the pair at config time** in both branches. That is
      a validation, not a runtime path.
12. **ADR.** **Default:** a new ADR-0084, "Every verdict reaches the
    receiver", for the v11 message, the ack field and the verdict rule. It
    extends ADR-0010 (version) and amends ADR-0081's identity.

## 10. Cross-references and supersessions

Superseded, pending the new ADR (proposed ADR-0084):
- **`core-extraction-recovery.md` decision 5** ("the clamp → shed order
  stays at the call site … No core function composes clamp, shed and
  serve"). **Superseded pending ADR-0084**: the verdict rule judges the shed
  before the clamp (§1.2), and `judge_seq` composes shed and serve (§4.1).
- **`core-extraction-recovery.md` decision 3 and §1.3's redundancy pin**
  ("on the shipped arm the map is redundant by data", the engine mapping
  ρ < 1 to (0, ∞, false), §3.3's pin). **Superseded pending ADR-0084**: the
  intake pause reads (O, B, F) on both seats (§3.2 (b)). The model's
  `intake_paused(O_retained, B, false)` call must pass the frontier-covered O.
- **`giveup-law-model.md`** (budget = min(1 − ρ, R), owner decision
  2026-11-14). Superseded only if owner question 3 is answered yes.
- **ADR-0081's identity** gains the partition of §0 (`V = H + E`, a verdict
  seq stays in V after a later ack). Amended by ADR-0084.
- **ADR-0010**: extended by the v11 bump (§2.4).
- **`core-extraction-recovery.md` step 4** (the store-span clamp bound): not
  needed under branch S, which uses step 2's ρ = 1 bounds on both seats
  (§3.6). It stays an option under branch W only.

Not superseded: ADR-0064's shed law keeps its form (age > D within a 1 − ρ
budget). Its operands change: `Ab` replaces the cumulative shed count, and
1 − ρ replaces R as the budget. D's derivation reads RTprop. The engine
keeps the EWMA until question 7's battery reads the derived form (§1.3).

