## 46. The path event — does the promoted placement price bring a leg back? — pre-registration

§41b's ruling promoted the wire-clocked base: ŵ_i is quinn's SRTT per leg,
mirrored at every ack batch. The old base priced a leg on its own app-echo
srtt and froze it while the leg carried nothing. Every lock-out seen so far
was a start-up artefact at static caps. The default law showed no benefit in
§38, §41 and §41b. The trigger a real link meets is a **path event**: one
leg's delay steps up briefly, placement leaves it, and then the link
recovers. This section asks whether the old price keeps that leg out and the
new one brings it back. Design only: no engine code, no VM contact.

**What refreshes each price on a leg that carries nothing (read from code
at `diag36` `5ba4d07`).**
- *The old price (Copa's app-echo srtt) has no refresh path at idle.*
  - The srtt is fed at three sites: the per-batch `Ack` (`control_msg.rs:308`
    and `:318`), the `WindowAck` (`:599` and `:609`, guarded by
    `echo_send_timestamp_us > 0` at `:596`), and the peer's `PathReport`
    (`:457`–`:465`).
  - The report arm is off: `RWM_MSTAR_ANCHOR` resolves on (`gates.rs:355`),
    and `if !mstar_anchor` skips it (`control_msg.rs:457`).
  - A data-triggered `WindowAck` carries the batch's echo and goes out on the
    leg the data arrived on (`receiver.rs:2074`–`2104`; the legacy per-batch
    `Ack` at `:2253` likewise). An idle leg gets none.
  - The receiver's broadcasts to every live leg carry echo 0: the hole
    re-advertisement (`receiver.rs:1032`, sent `:1043`–`1046`), the
    hold-expiry unwedge (`:1124`, sent `:1132`–`1135`) and the timer ack
    (`:1168`, sent `:1176`–`1179`). The `:596` guard drops them.
    `GenerationDeficit` (`:577`–`581`) carries no timestamp.
  - So an idle leg's srtt changes only when placement puts a symbol on it.
    It heals in only two ways:
    - the softmax tail places a symbol there, and its echo moves srtt 1/8 of
      the way to the true RTT (a cascade once it starts);
    - the surviving legs' queue term `(I/W)·s` climbs past the frozen base
      (§38's c8 192 role swap). That needs a survivor that cannot carry the
      load.
- *The new price (quinn's SRTT) refreshes at idle, slowly.* On the sender,
  quinn takes an RTT sample only when an ack-eliciting packet of its own is
  newly acked (quinn-proto 0.11.14 `connection/mod.rs:1512`–`1522`, EWMA
  7/8 in `paths.rs:329`–`349`). On an idle leg these come from:
  - its own `PathReport` and `Ping`, sent every `REPORT_INTERVAL` = 2 s
    (`net/mod.rs:187`) on the leg's reliable control stream to every live
    path (`tasks/report.rs:113`–`134`, `report_tick_tx`'s `live_paths()`);
  - its `Pong` datagram, answering the peer's own 2 s `Ping`
    (`control_msg.rs:183`–`186`). Both peers run the report task
    (`net/mod.rs:1397`, inside `run_impl`), so this arrives at the peer's
    phase.
- *Nothing faster.*
  - No keep-alive is configured (`transport/quic.rs:909`–`914` server,
    `:953`–`958` client), and quinn's default is `None`
    (`config/transport.rs:381`).
  - The client's ACKs of the peer's traffic are not ack-eliciting, so they
    give no sample.
  - `MAX_STREAMS` goes out once more than 1/8 of the 100-stream credit is
    used (`streams/state.rs:813`–`816`). At two streams per 2 s that is
    about every 14 s.
  - MTU probes run at start-up and then every 600 s.
  - So an idle leg gets **2–3 samples per 2 s** (derived): one or two from
    the report pair (two stream packets, acked together or apart), and one
    from the Pong.
  - *Measured floor.* On §38's locked c8 MAIN rows (`place38/diag-ld`,
    @256 s42 r1–r3, s7 r3, @192 s42 r3), the frozen leg's `dgq1 tx` rises
    by exactly 1 every 2 s and is otherwise flat (4–5 such windows per row,
    0 elsewhere bar start and end). That is the Pong, so the peer's Ping
    reaches an idle leg in `perf` mode. The report pair rides the stream,
    which `dgq` does not count; it is not measured. **≥ 1 sample per 2 s is
    measured, and 2–3 is derived.** This session measures the rate itself
    (the `wrtt` step count below).
- *ŵ's recovery after a step Δ ends on an idle leg.* The excess after k
  samples is Δ·(7/8)^k. It halves in 5.2 samples and falls to 10 % in 17:
  - at 2–3 samples per 2 s: ≈ 3.5–5 s and ≈ 11–17 s;
  - at the measured floor of 1 per 2 s: ≈ 10 s and ≈ 34 s.

  On a carrying leg every data ack is a sample, so ŵ follows within an RTT
  or two.
- *Repairs are placed by the same softmax, with a fate term* (paper §5.7's
  V_i = w_div·fate_i, `place.rs:550`–`571`). This covers coded corrections
  (`emit_source.rs:816`–`824`), SACK-gap retransmits
  (`sender_phases.rs:1162`) and margins (`:1320`).
  - With the hit leg idle, every covered symbol sits on the survivor, so
    fate = 1 there and 0 on the idle leg. A repair therefore sees the gap
    less 1/T ≈ 6.7 nats.
  - At c8 default these run at ≈ 450/s (`retx` ≈ 370/s, `cod` 50–90/s,
    §38 MAIN s42 r1), against ≈ 12 000 source placements/s.
  - Repairs carry a live `send_timestamp_us` (`emit_source.rs:833`, the
    `[ETA]` "0 sentinel" is `eta_rel`, not the echo). So a landed repair
    refreshes the old price as well as quinn's.
  - The spare-capacity repair rule, which would send every repair to an idle
    leg (`spare_capacity` = ∞ at `infl` = 0, `path.rs:517`–`522`;
    `place.rs:201`–`230`), runs only under `RWM_XPATH_REPAIR`, which is off
    and being deleted.

**The event must exceed the leg's standing queue (why not +100 ms).**
- Every c8 cell sets `rate`, so netem schedules a packet no earlier than the
  queue's tail (`lib.sh`'s FIFO note): t_send = max(now + delay, t_tail) +
  t_pkt. A delay step smaller than the standing wait is absorbed.
- c8 default rows (§38, `place38/diag-ld`, MAIN and WIRE s42 r1, 1–7 s):
  - fast leg (c2, about 10 ms floor): `wrtt` 72–145 ms, a wait of 60–135 ms;
  - slow leg (c3, about 40 ms floor): `wrtt` 170–316 ms, a wait of
    130–280 ms.
- That formula is the documented FIFO behaviour, not read from kernel
  source here. The event witness below checks that the step bites.
- So +100 ms would read as a null that the injection made, not the law.
  **Δ = +1 500 ms, D = 2.0 s**, on the data direction only. The ACK
  direction's netem is untouched, so the RTT step is Δ. Why Δ = 1 500 ms:
  - Δ exceeds both legs' largest standing wait by ≥ 5×, so it bites
    whatever the pre-event queue.
  - It is the smallest round Δ at which the old price stays frozen against
    the repair path at the derivation's worst corner (below). At 1 000 ms
    that corner would let repairs heal BASE within seconds. Then the test
    could not tell a self-healing old price from the new one.
  - D ≥ Δ + floor, so the +Δ echoes and acks return inside the hold, and
    both prices have absorbed the step before restore.
  - Bounds. During the hold a leg holds at most quinn's cwnd in netem
    (`qcwnd` ≈ 0.4–0.5 MB ≈ 350–420 datagrams, fewer skbs with GSO),
    against `limit 1000` skbs. The check rows measure this. Arrivals pause
    ≤ 1.54 s, against `DEAD_PATH_TIMEOUT` = 6 s (`net/mod.rs:496`) and the
    QUIC idle timeout of 30 s.

**The derivation (Bulk, T = 0.15, ref = the fast leg's s when it is the
smaller).** At restore, with the hit leg idle (I_hit = 0) and the survivor
carrying, the gap in nats is g = (c_hit − c_surv)/T:
- old: g = [(½+ε̂_h)·s_h*/s_F − I_F/W_F − (½+ε̂_F) + r_h − r_F] / T;
- new: g = [(½+ε̂_h)·ŵ_h/s_F − I_F/W_F − (½+ε̂_F)·ŵ_F/s_F + r_h − r_F] / T.

Inputs, from the c8 default rows and §38:
- with the fast leg carrying everything, s_F ≈ 150–200 ms (the window of
  about 1 600–1 800 symbols over 9 800 symbols/s);
- ŵ_F/s_F ≈ 0.6–0.9 (WIRE rows' fast leg: `rtt` 126–130 against `wrtt`
  71–75);
- I_F/W_F ≈ 0.3–0.7 (needed to reproduce the 0.18 share at fresh prices);
- ε̂ ≈ 0.03–0.04;
- about 12 000 placements per second.

What follows from them:
- *Re-entry.* Repairs see g − 6.7 at ≈ 450/s, and sources see g at
  ≈ 12 000/s. So a leg gets about one landing per second once g ≲ 13 nats.
  Each landing's echo moves both prices 1/8 of the way down, which raises
  the odds of the next (a cascade).
- *Frozen.* With g ≳ 19 nats, the expected landings over 25 s stay ≤ 0.05.
- *Slow-leg hit.*
  - Old: s_h* ≈ 1.54–1.9 s (Δ, the floor, and any local dwell at the
    freeze), so g ≈ 20–32 nats at restore. Nothing refreshes it. The fast
    leg carries the whole window at 100 Mbit/s, so its own price never
    reaches the frozen one: no role swap.
    - At Δ = 1 000 ms the same corner would give g ≈ 11 nats, several
      repair landings per second, and a BASE that heals by itself.
  - New: ŵ_h* ≈ 1.54 s and g ≈ 20–33 nats at restore, but ŵ_h decays. It
    falls below g = 13 nats at ŵ_h ≈ 710–1 150 ms, after 3–7 idle samples.
    That takes ≈ 2–7 s at 2–3 samples per 2 s, and ≈ 6–14 s at the
    measured floor.
- *Fast-leg hit.* The slow leg cannot carry the window: at 20 Mbit/s its
  echo srtt and its queue term climb to the order of the frozen fast-leg
  price. So the fast leg re-enters in both arms, during the hold or within
  about 1 s of restore. Its netem queue drains its +Δ backlog at line rate
  after restore.

**Cell and arms.**
- `c8-100` (c2/c3), default store law, Bulk, `--window-reliable`, 500 MB
  (≈ 42 s), env as §38 (`RWM_RTOBS=1 RWM_RDIAG=1 RWM_GEN=0 RWM_DIAG=1
  RWM_PERF_TIMEOUT_S=150`, the standard `env -u` list).
  - 500 MB, not the 400 MB first proposed: a one-leg BASE row at 88–90
    Mbit/s would end ≈ 29 s after restore, too close to the 25 s horizon
    below. The extra ≈ 7 s per row is cheap.
- Two event variants:
  - **SLOW-HIT:** leg B, `cli1`, c3;
  - **FAST-HIT:** leg A, `cli0`, c2.
- **BASE** = main `4926822` (old price). **CAND** = `place-promote` (new
  price, gate gone). Each is built fresh in its own `git archive` tree with
  its own target dir.
  - `place-promote` currently equals `diag36`'s tip `5ba4d07`, with no
    engine change yet.
  - CAND is pinned as a SHA, recorded here by amendment before launch. That
    SHA must contain the promotion: no `RWM_PLACE_WIRE` token on `[GATES]`,
    and the old base deleted. Otherwise the session does not launch.
  - The arms are told apart by binary sha, not by `[GATES]`.
  - It runs in the merge-candidate session, after that tree's suites.
- n = 4 per arm per variant: seeds 42 and 7 × 2 reps, 16 scored rows.
  - Order: rep-major; within a rep, seed 42 then 7; within a seed,
    SLOW-HIT then FAST-HIT. Each (rep, seed, variant) block runs both arms,
    CAND first on odd blocks.
  - The order is fixed by the harness and pinned by a test.
- **Check rows (unscored, first; they double as the smoke):** one per
  variant, CAND binary. Each captures `tc -s qdisc show dev <leg>` before
  onset, every 100 ms during the hold, and after restore.
  - **Pass:** apart from counters, the "during" line differs from "before"
    only in `delay` (+1500ms), and "after" equals "before".
  - **Pass:** the backlog peaks below 900 packets.
  - A fail stops the session for an amendment.
- **c1d, reported only:** c1/c1, 3.5 GB (≈ 36 s), leg 1 hit with the same
  event, n = 1 per arm (s42), run last, dropped if the session's budget is
  short. That is 20 invocations in all.

**Injection (harness, not engine).**
- `topo_dual.sh`'s `shape` builds its netem argument list inline and runs
  `tc qdisc add`. The harness factors that list into one builder,
  `netem_args <scenario> <seed> [delay_ms]`, used by `shape` (`add`, plus
  `truth_counter_install` as now) and by a new `reshape <dev> <scenario>
  <seed> <delay_ms>`.
- `reshape` re-issues the FULL line as `tc qdisc change dev <dev> root
  [handle 1:] netem <args>`, with only `delay` overridden.
  - It never runs `add` or `replace`, which would drop the queue and reset
    the qdisc's counters that `[TRUTH]` and the QDISC tail read.
  - It never reinstalls the counter.
- A test pins `netem_args c3 1042` byte-equal to the line `shape` issued
  before the refactor. It also pins `netem_args c3 1042 1520` as that line
  with `delay 1520ms`.
- `change` may re-seed netem's PRNG and reset the GE chain. The check row
  shows which. Either way it happens at the same instants in both arms.
- **Timing, keyed to transfer start by the harness.**
  - An injector starts with the client and polls `/tmp/rwm-c.log` every
    20 ms. Transfer start is the wall time when the warm-up JSON line
    appears (the measured transfer starts after it).
  - Onset is at transfer start + 5.0 s, restore at onset + 2.0 s. Wall
    stamps are taken before and after each `tc` call, in
    `<row>-evt.txt`.
  - The injector also stamps when each `[DIAG] t=` line appears. The
    parser maps onset and restore onto the `[DIAG]` axis with offset =
    median(wall_seen − t_printed). The tolerance is ±0.15 s, and the
    registered readouts are coarser.

**Readouts (fixed now; client `[DIAG]` lines, one per ≈ 250 ms).**
- *Egress per leg and window.* The per-line delta of `dgq<i>[… tx=…]`:
  DATAGRAM frames quinn transmitted on leg i, cumulative
  (`net/diag.rs:770`–`806`). It is printed by both binaries.
  - Hit-leg share = Δtx_hit / (Δtx_A + Δtx_B).
  - σ_pre = the pooled share over lines with t ∈ [2.0 s, t_on).
  - A line is *idle* on the hit leg when Δtx_hit ≤ 2 (control only) and
    `infl` = 0.
  - `[TRUTH] egress_dgrams` (end of row) is the row-level cross-check:
    Σ Δtx within 1 %, reported.
- *R, return time (scored).* Take the first 4 consecutive lines that start
  after t_off and whose pooled hit-leg share is ≥ ½·σ_pre. R is the last of
  those lines' t minus t_off.
  - R is right-censored at the horizon H = 25 s (R = ">25"), which ends
    before the end-of-transfer drain.
  - A censored value ranks above every finite one.
  - In a median, a censored value enters as H. That understates a
    difference and never inflates one.
- *G, goodput deficit (scored).* G = 1 − ḡ[t_off, t_off + 25 s] /
  ḡ[2.0 s, t_on).
  - ḡ is the time-weighted mean of `good=` (the cumulative-ack advance per
    window, `diag.rs:481`–`483`), with weights equal to the gap in t between
    consecutive lines.
  - The intervals are absolute on the `[DIAG]` axis.
- *Reported beside them:*
  - R0, the first line after t_off with Δtx_hit > 2 (data egress resumed).
    It separates "the price let it back" from Copa's cwnd regrowth. W_i is
    in both arms' queue term, and Copa shrinks it under the +Δ standing
    delay.
  - Goodput during the hold.
  - The hit leg's `rtt`, `wrtt` (and `wpl_us`) at t_off and at R0.
- *Event witness (per row; void otherwise).*
  - The hit leg's max `wrtt` over [t_on, t_off + 1.5 s] is ≥ its pre-event
    median + 750 ms (½·Δ).
  - The injector's two `tc` calls returned 0.
- *Exit witness (per row, for NOT-PROVOKED).* Some 4-line window inside
  [t_on, t_off + 1 s] has pooled hit-leg share < ½·σ_pre.
- *Mechanism witnesses (reported; they explain R, they do not void).*
  - BASE: on idle hit-leg lines after t_off, `rtt` is constant (±1 ms)
    while `wrtt` falls. That shows the frozen old price beside a refreshing
    wire RTT. The values at t_off and at the end of the idle run are given.
  - CAND:
    - `wpl_us` is numeric on both legs. If the promotion dropped it, `wrtt`
      is the witness, and that is stated.
    - On idle hit-leg lines after t_off, `wpl_us` falls in ≥ 2 steps before
      R0 (unless R0 ≤ 1 s).
    - The final `[ETA]` line has `w_mv` > 0.
  - Both arms: idle-leg refresh rate, i.e. the count of `wrtt` changes per
    2 s on idle hit-leg lines. Predicted 2–3, with ≥ 1 measured (finding
    above). Steps are ≈ 190 ms at first, so they are visible at 1 ms
    resolution.
- §38's row-level lock-out classifier is not used. It would call a leg that
  recovers late LOCKED.

**Noise rule (fixed now): disjoint ranges plus a magnitude floor.**
- Per variant and readout, CAND is BETTER iff all 4 CAND values are on the
  better side of all 4 BASE values, and the medians differ by more than the
  floor. WORSE is the mirror image.
  - The floor for R is 1 s, the readout's window.
  - The floor for G is 4.0 points, c8's registered goodput MDE (§38).
  - Ties, including two censored values, break disjointness.
- *Why not §41b's relabelling.* Its statistic scales each clause by an A/A
  spread from a pool of one binary. No A/A event data exists, and R is
  censored (BASE is expected at ">25"), so its sd is undefined.
- At 4 against 4, disjointness is the extreme of the exact permutation test:
  p = 1/70 ≈ 0.014 one-sided. It holds under censoring, because only ranks
  enter.
- The rule runs on live rows (those passing the event witness). With 3
  against 4 it is p = 1/35, and with 3 against 3 p = 1/20. That p is printed
  beside each call, and the rule is unchanged.
- Family-wise:
  - CAND-WORSE has 4 one-sided chances (R and G × 2 variants), so its false
    alarm is ≤ 4/70 ≈ 5.7 %;
  - FASTER has 2 chances, ≤ 2/70 ≈ 2.9 %.

**Outcomes (one verdict, in precedence order).**
1. `UNSCOREABLE`: an abort cause fired, a check row failed, or a variant has
   < 3 rows passing the event witness in either arm.
2. `CAND-WORSE`: at either variant, R reads WORSE (CAND slower) or G reads
   WORSE (CAND loses more).
3. `NOT-PROVOKED`: SLOW-HIT is not provoked, and no provoked variant reads R
   BETTER.
   - A variant is *provoked* iff ≥ 3 of its BASE live rows pass the exit
     witness. Otherwise the event did not make placement leave the leg, and
     that variant did not test the price.
4. `CAND-RECOVERS-FASTER`: R reads BETTER at a provoked variant, and nothing
   in 2 fired.
5. `NO-DIFFERENCE`: none of the above.

**Decision rule (approved in substance by the user).**
- `CAND-WORSE` blocks the promotion's merge.
- `NO-DIFFERENCE` means the promotion's case rests on fixed-cap robustness
  alone (§38, §41: 9 of 12 c1d 1536 rows and 3 c8 256 rows unlocked). The
  call goes back to the user.
- `NOT-PROVOKED` also goes back to the user, labelled as such: a null of
  the injection, not of the price.
- `CAND-RECOVERS-FASTER` supports the merge.
- No tuning in this session: no second Δ, D or horizon. A follow-up is a new
  registration.

**Named as untested.**
- A late-joining leg. The old price seeds it at `DEFAULT_SRTT` = 50 ms, so
  it is optimistic at c8 and pessimistic at c1d (paper §5.7, cold price).
- A rate drop or an outage, as distinct from a delay step.
- An event on both legs at once.
- Off Bulk, and at four paths.
- The ACK direction.

**Predictions (from the derivation above, before any number).**
- *SLOW-HIT (σ_pre ≈ 0.17–0.19, so ½·σ_pre ≈ 0.09).* Both arms: the
  event witness passes (`wrtt` ≈ 1.5–1.6 s), and the hit leg's share falls
  below 0.02 within ≈ 0.5 s of onset. The queue term acts before any echo
  returns.
  - BASE:
    - `rtt` freezes at 1.54–1.9 s with `infl` = 0, while `wrtt` decays
      ≈ 1/8 of its excess per idle sample. At 2–3 samples per 2 s that is
      ≈ 250–450 ms after 10 s and below 100 ms after 25 s. At the floor it
      is ≈ 800 ms after 10 s.
    - R is censored (">25") in all 4 rows (expected landings ≤ 0.05).
    - G ≈ 8–14 %, the one-leg rate (§38's locked c8 rows ran 88–90 against
      102–103).
  - CAND:
    - `wpl_us` falls in steps of ≈ 190 ms at first, 2–3 per 2 s.
    - R0 ≈ 2–7 s and R ≈ 3–10 s (median ≈ 6). At the measured floor: R0
      6–14 s and R ≤ 17 s, still inside H.
    - G ≈ 2–7 %.
  - Reading: R BETTER, G not WORSE (and likely BETTER).
- *FAST-HIT (σ_pre ≈ 0.81–0.83, so ½·σ_pre ≈ 0.41).* Both arms: the
  window stalls behind the fast leg's undelivered symbols during the hold,
  and goodput collapses then.
  - The slow leg cannot carry the load, so the fast leg re-enters in both
    arms. R ≈ 0.5–3 s, with overlapping ranges, and G is similar.
  - Reading: NO-DIFFERENCE at this variant.
  - The stated risk: if the store window shrinks during the hold, the slow
    leg's price stays low, and BASE could hold the fast leg out. That would
    read BETTER for CAND, not WORSE.
- *Verdict:* `CAND-RECOVERS-FASTER`.
- *c1d (reported).*
  - BASE: leg 1 freezes at ≈ 1.5 s against s ≈ 20–45 ms on the other leg
    (g ≫ 30 nats), so it stays out.
  - CAND: re-entry needs ŵ ≲ 90–200 ms, i.e. ≈ 15–20 idle samples, so R ≈
    12–20 s at 2–3 samples per 2 s. It censors at the floor.
  - G may be negative in BASE: one-leg c1d rows ran faster, because the
    receiver runs ×1.35 μ_R (§38).

**Abort table.** §41's (`ABORT-LOCK`, `-CRLF`, `-SHA`, `-SMOKE`, `-BUILD`,
`-RC`, `-BRINGUP`, `-BUDGET`, `-ONDISK`, `-BIN`, `-PRECONDITION`,
`VOID-COTENANT`), plus `ABORT-INJECT`: a `tc` call fails, or a check row
fails. The precondition, locks, tmpfs and `LD_HARD` = launch + 4 h are as
§41b.

**Duration.** Two builds (≈ 11 min). Then 18 c8 invocations at ≈ 65 s
each, and 2 c1d at ≈ 70 s (≈ 22 min). Scoring ≈ 5 min.

### 46. Amendment 1 (before build)

A coordinator review asked for nine checks before the harness is built. All
are settled here from code and the committed ledgers. No engine code, no VM
contact.

**1. The dead-path timeout cannot fire; the test measures the price, not a
revival.**
- *The rule.* `DEAD_PATH_TIMEOUT` = 6 s (`net/mod.rs:496`). At every 2 s
  report tick, `report_tick_tx` calls `check_dead_paths` (`report.rs:161`).
  That marks a leg inactive when now − `last_report` > 6 s
  (`scheduler/mod.rs:571`–`582`).
- *What moves `last_report`.* `touch_path_at` (`scheduler/mod.rs:546`–`550`)
  moves it, from these sources:
  - the peer's `Ping` (`control_msg.rs:185`);
  - the per-batch `Ack` (`:236`);
  - the peer's `PathReport` (`:441`);
  - every `WindowAck`, including the echo-0 broadcasts (`:533`);
  - the receiver-side arrival stamps (`sync_rx`, `scheduler/mod.rs:592`–`608`).
  So it keys on anything heard on the leg, in either direction.
- *Silence on the client (the sender, whose price is under test).* The
  event delays only the data direction, the client's egress `cli<i>`. The
  peer's `PathReport` and `Ping` reach the client on the undelayed `srv<i>`
  egress every 2 s, and so do its echo-0 broadcasts. Worst-case silence is
  therefore ≈ 2 s plus the 500 ms send deadline, which is ≤ 2.5 s, against
  6 s.
- *Silence on the server.* The server hears the hit leg only through the
  delayed direction.
  - The client's reports leave every 2 s. Shifting their arrivals by Δ at
    onset opens one gap of at most 2 s + Δ + 0.5 s = 4.0 s at Δ = 1.5 s.
  - After restore, packets leave in send order behind the delayed tail
    (item 2). So arrival gaps shrink, never grow.
  - 4.0 s < 6 s, so no leg goes inactive in either arm. Δ would have to
    exceed ≈ 3.5 s for this to bind.
- *Witness, added.* Count the `path timed out — marking inactive` and `path
  recovered` lines (`scheduler/mod.rs:576`, `:552`) in both logs of every
  row. Any non-zero count voids the row. A revival resets Copa and the cwnd
  (`:557`–`560`), which would be a second mechanism.

**2. The event does not end at restore.**
- *Netem's release rule.* A packet sent at t leaves at max(t + delay,
  t_tail) + t_pkt.
  - A packet sent just before restore leaves ≈ Δ later.
  - Packets sent after restore queue behind it, so the leg's true RTT falls
    linearly from ≈ Δ + floor back to its floor over [t_off, t_off + Δ].
  - It then drains whatever queued behind at the leg's rate:
    - SLOW-HIT: the leg is idle, so there is nothing behind it beyond a few
      control packets;
    - FAST-HIT: up to quinn's cwnd, ≈ 400 datagrams ≈ 40 ms at 100 Mbit/s.
- *Consequence.* R and R0, measured from the `tc` restore call, have a
  **floor near Δ = 1.5 s in both arms**. The price cannot see a recovered
  link before the link has recovered.
- *Quinn's samples inside the tail.* Idle-leg samples taken in [t_off,
  t_off + Δ] read the residual delay, so ŵ's decay toward the floor starts
  between t_off and t_off + Δ.
- *Readout added (reported).* T_w is the time from t_off until the hit
  leg's `wrtt` is back to ≤ 1.1 × its pre-event median (the median over
  [2.0 s, t_on)). It is censored at H.
- *Predictions redone with the tail (they replace §46's where they differ).*
  - **SLOW-HIT, BASE.** `rtt` frozen at 1.54–1.9 s. R censored (">25") in
    all 4 rows. T_w ≈ 11–17 s at 2–3 samples per 2 s, and censored at the
    floor rate. The leg is idle, so its `wrtt` must decay from ≈ 1.5 s to
    ≈ 240 ms by quinn's idle samples: 15 samples.
  - **SLOW-HIT, CAND.**
    - R0 ≈ 2.5–8.5 s, which is ≈ 0–1.5 s of tail plus 3–7 samples.
    - R ≈ 4–12 s, median ≈ 7.
    - At the measured floor of 1 sample per 2 s: R0 ≤ 15.5 s and R ≤ 19 s,
      still inside H = 25 s.
    - T_w ≈ R0 + 1 s, because a carrying leg's `wrtt` follows within an RTT
      or two.
  - **FAST-HIT, both arms.** R ≈ 1.5–4 s, overlapping. T_w ≈ 1.5–2 s.

**3. A second step size, reported only: SLOW-HIT at +500 ms.**
- *Design.* 2 rows per arm (s42, s7). They run after the 16 scored rows and
  before c1d, with the same event, readouts and witnesses. They are never
  scored and never enter the verdict.
- *What each step represents.*
  - The scored Δ = 1.5 s is an **outage-class event on a lossy link**:
    roaming, a stall, a scan. The old price freezes there, and repairs do
    not heal it.
  - +500 ms is the **ordinary hiccup**.
- *Prediction.*
  - 500 ms exceeds the slow leg's standing wait (130–280 ms), so the step
    bites, by ≈ 220–370 ms of RTT on a loaded leg and ≈ 500 ms once it
    idles.
  - BASE: s* ≈ 0.54–0.9 s, so at restore g ≈ 2–16 nats (the upper end only
    if the local dwell at the freeze is large).
  - At g ≲ 8, sources still land at ≥ 4/s and repairs heal the rest, so
    the leg is sampled again within about 1 s in both arms.
  - Predicted R ≈ 1–3.5 s in both arms (≈ 0.5 s floor from the tail),
    ranges overlapping: no difference.
  - The stated risk: a BASE row whose s* includes ≳ 300 ms of local dwell
    can hold out for ≈ 5–20 s. That would be reported as the hiccup-scale
    tail of the same defect.

**4. c1d is read on R, not goodput (reported only).**
- c1d is the clean-link case: c1's loss is ≈ 0.1 %, so there are few
  repairs to heal the old price. Its goodput comparison is confounded by
  the receiver running faster on one leg (§38, ×1.35 μ_R).
- Predicted:
  - BASE R censored;
  - CAND R ≈ 13.5–21.5 s (1.5 s tail plus ≈ 18 idle samples), which may
    censor at the floor rate.

**5. Scoring decides on absolute goodput; G becomes reported only.**
- G's baseline is the 3 s window [2.0 s, t_on), just after start-up, while
  the store window is still growing (§38 rows: `win` 1 400 → 1 600 over
  2–5 s). It is too noisy a denominator to decide on.
- **Scored instead:** ḡ_post, the time-weighted mean of `good=` over
  [t_off, t_off + 25 s], CAND against BASE. It uses the same disjoint rule.
  The floor is 4.0 % of BASE's median (c8's goodput MDE, §38).
- Outcome 2's goodput clause now reads "ḡ_post reads WORSE (CAND lower)".
  G stays printed beside it.
- Predicted:
  - SLOW-HIT: BASE ≈ 86–92 Mbit/s (one leg), CAND ≈ 94–102. Likely BETTER
    (disjointness at n = 4 is not assured).
  - FAST-HIT: within the floor.

**6. Check rows: one per binary.**
- BASE runs SLOW-HIT and CAND runs FAST-HIT, so each binary and each
  variant's injection path is exercised once.
- Each check row keeps §46's `tc -s qdisc show` captures (the delay-only
  diff and the backlog < 900 packets).
- Each also prints the row's witnesses:
  - the hit leg's peak `rtt` and peak `wpl_us` (or `wrtt` on BASE);
  - the inactive and recovered line counts (item 1);
  - the number of idle-leg samples, i.e. `wrtt` step changes per 2 s on
    idle lines;
  - the event and exit witnesses.
- A dead injection stops the session before the 16 scored rows. A dead
  injection is a failed event witness, a failed exit witness at SLOW-HIT on
  BASE, a non-zero inactive count, or a failed qdisc diff.
- The fix is then a new amendment, never an in-session re-tune of Δ or D.

**7. Netem state.** Each `tc qdisc change` that carries the full line
re-parses every option, and likely restarts netem's Gilbert–Elliott chain
and its random sequence, re-seeded from the line's `seed`. That happens at
onset and at restore. It is the same in both arms, by seed and at the same
instants relative to transfer start. The check rows' qdisc captures show
what changed.

**8. CAND's identity.**
- CAND is the **merge-candidate build**: `diag36` + the promotion +
  simplification batch 1. Its SHA is recorded by a further amendment
  before launch, and must contain the promotion (§46's conditions).
- If the result is `CAND-WORSE`, the cause is bisected against
  `place-promote` alone (`diag36` + the promotion) under a new
  registration with this design. That separates the price from batch 1.
- No other verdict needs the bisection.

**9. The guard, run in the same session (not part of §46).** §41b cleared
only the gate-off binary, at c1s and c1d. So the promoted build also runs
§41b's guard (BASE n = 12 per cell against the candidate) at c2 default
and c8 default in this session, under §41b's statistic and decision rule.
It is noted here for the session plan. It neither enters nor depends on
§46's verdict.

**Row count and duration.** 2 check rows, then 16 scored rows, 4 rows at
+500 ms and 2 c1d rows: 24 invocations, ≈ 27 min, plus two builds
(≈ 11 min) and the guard.
