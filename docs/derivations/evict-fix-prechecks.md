# Prechecks for the evicting-seat verdict rule

Roadmap node `rec-evict-blocker-unresolved`; ADR-0079 Amendment 1;
`test-detection-plan.md` §1 D1a–D1d. Code read on github/main `0d885bbd`.
Paths are under `raptorpath/src/` unless they start with `tests/`.
Each answer is marked **[E]** established (read in code or seen in a trace)
or **[I]** inferred.

The question behind all three: the owner's rule says "every abandoned symbol
gets a verdict, and the receiver is told". Before deriving it, is the clamp a
bug that hides a resendable seq, does any message tell the receiver today, and
why does the natural-budget tail stall?

## 1. A seq below the coding window is not resendable at ρ < 1

- **[E] The bytes are gone.** At ρ < 1 the only payload copy is the encoder
  window. `sent_store` is filled only when `pol.reliable`
  (`net/emit_source.rs:347–349`); `retransmit_buffer` holds metadata only,
  `(send_ts, ε, path)`, no bytes (`net/emit_source.rs:87–89`, insert `:505`).
  The window drops its front on two paths: the size cap
  (`net/mod.rs:3594–3597`, `win_cap`) and the ack-relative slide
  (`net/sender_phases.rs:1371–1374`, `ack − keep_behind`). `advance` pops the
  bytes (`fec/rlc_window.rs:199–207`).
- **[E] The resend lookup cannot serve below the window.** The evicting arm
  is `st.encoder.get_source(seq).unwrap_or_else(|| st.encoder.generate_repair())`
  (`net/sender_phases.rs:998`). `get_source` returns `None` for
  `seq < window start` (`fec/rlc_window.rs:213–217`), and the fallback is a
  repair over the *current* window (`fec/rlc_window.rs:98–`), which contains
  no equation for the lost seq. So without the clamp the site would emit a
  useless repair per below-window seq, not a resend.
- **[E] Provenance of the clamp.** Added with exact-source NACK retransmit,
  ADR-0037, commit `d8925b4f` (2026-03-18), when the encoder window was the
  only data source in every mode: "if `None` is returned (source evicted from
  window), fall back to" repair. The retain branch (no clamp, serve from
  `sent_store`) came with retention, `4c901536` (2026-07-06, "RWM Phase A").
  The comment at `net/sender_phases.rs:791–794` states the reason: "EVICT:
  only the coding window can serve a gap — older seqs are gone". It is a
  statement of where the data is, not a bound chosen for cost.
- **[E] But the clamp also gates the verdict.** The shed check sits *inside*
  the clamped loop (`net/sender_phases.rs:847–872`), so a below-window seq is
  neither served nor shed; and `retransmit_buffer` is pruned only by the
  cumulative ack (`net/sender_phases.rs:1403`) and by a shed (`:860`), never
  by eviction. A below-window seq therefore stays at `retransmit_buffer.first()`
  forever, and the tail sweep keeps aiming its `(first, first)` gap at it
  (`net/mod.rs:2789`, `:3035`), which the clamp drops
  (`test_seams::on_gap_clamped`, `:800`).

**Answer.** The clamp is correct for the *serve* question and wrong only as
the gate of the *verdict* question. Removing it would not make the seq
resendable. Eviction of an un-acked seq is already an abandonment that the
code never records: eviction itself must be the verdict.

## 2. Nothing tells the receiver; it moves past a hole only on its own

- **[E] The receiver's frontier has exactly three writers**, all in
  `net/reorder.rs`: the contiguous drain (`:96–101`), hold expiry
  (`drain_expired`, `:112–143`) and the capacity force-drain
  (`force_drain_oldest`, `:149–163`, called from `push` at `:87–90`). Expiry
  and force-drain both need a pending entry *above* the hole: expiry takes the
  largest expired pending seq `k` and moves the frontier to `k + 1`; the
  force-drain runs only when `pending.len() > max_buffered`. With nothing
  above the hole, `oldest_deadline()` is `None` (`:181–189`) and no timer is
  armed (`net/receiver.rs:715–795`).
- **[E] No field carries the sender's window start.** The control messages
  are `BlockStart, Ack, BlockResult, PathReport, Ping, Pong, Shutdown,
  WindowStart, WindowAck, WindowSwitch, GenerationDeficit, RepairRequest`
  (`transport/protocol.rs:400–570`). `WindowStart { backend }` is a one-shot
  announcement of the codec at stream start, log only since ADR-0078
  (`:454–457`); it carries no seq. `tests/f2_window_start_lost.rs` is about
  that message being lost, not about a window floor. The receiver's
  `decode_inert_floor` (`net/receiver.rs:2236`) is derived from its *own*
  `next_expected`, not from the sender.
- **[E] A shed sends nothing.** The NACK-site shed removes the seq from
  `retransmit_buffer` and `nack_retx_at`, adds it to `shed_seqs`, counts it
  (`net/sender_phases.rs:860–863`) and `continue`s. The slot-site shed
  (`net/emit_source.rs:757–785`) is the same. ADR-0079 Amendment 1 already
  states it ("A shed sends nothing to the receiver").
- **[E] An eviction sends nothing.** `encoder.advance` plus a
  `source_path_map` prune (`net/mod.rs:3594–3602`, `net/sender_phases.rs:1374`,
  `:1399–1401`).
- **[E] After the source is exhausted, nothing is sent at all** when
  `first()` is below the window. In the lost-tail run below (`lt_evict`), the
  sim's link trace shows the last data-sized client→server datagram at
  t = 0.822 s (`ord=3965 len=564`). After that only datagrams of at most a few
  hundred bytes (control) cross, until t = 28.1 s. The sender's `[DIAG]` reads
  `src=0 cod=0 retx=0` for the rest of the 30 s, and all 298 sweeps are
  clamped. When `first()` is inside the window the sweep does resend
  (section 3, `sr_evict`), but a resend is data, not a verdict.
- **[E] `Shutdown` is no substitute.** The sender sends it only on engine
  shutdown (`net/mod.rs:3046–3061`). The receiver only logs it
  (`net/control_msg.rs:119–121`).
- **[E] The reverse direction exists.** When a hold expiry moves the frontier
  with no arrival, the receiver sends a bare `WindowAck` (`net/receiver.rs`
  `:1026–1047`). So a receiver-side pass does reach the sender, as the
  cumulative ack, and prunes `retransmit_buffer` (`net/sender_phases.rs:1403`).

**Answer.** Today the receiver learns that a seq was abandoned only by
inference from later arrivals. A lost tail with nothing above it is never
passed, at any budget. Neither a shed nor an eviction produces a message, and
once the source is exhausted with `first()` below the window the sender is
silent. "Tell the receiver" needs a new sender→receiver fact: a floor below
which the sender can serve nothing. It must be sent when no data follows, so
it cannot ride on data symbols alone, and it needs a repeat or ack rule
because it travels on the unreliable control lane.

## 3. Why the lost tail stalls at the natural budget

**Setup.** The `sim_lost_tail` test binary (`--features sim,test-seams`,
`RUSTFLAGS="--cfg tokio_unstable"`, built from `0d885bbd`) is run in its child
mode directly, so env can be added. Every run uses seed 42, 6 MB (12 147
source symbols), Realtime, one path, 2.5 % loss, 4 ms jitter,
`RWM_TEST_TS_DROP=dead`, `RWM_TEST_TS_DEAD_MS=1500` and
`RWM_TEST_TS_SHED_ALL=1`. One port per scenario: 47 200 for k = 11 800
(`sim_lost_tail`), 47 400 for k = 12 087 (`sim_shed_reach`). The script and
logs are not committed. The command is the one `tests/sim_support/mod.rs:81–97`
builds, with `SIM_RELIABLE` set per row.

| run | ρ | k | budget | outcome | sender `cum=src/cod/ack` | sweeps | retx | shed/denied |
|---|---|---|---|---|---|---|---|---|
| lt_retain | 1 | 11 800 | n/a | client_ok 12.4 s | n/a | n/a | k sent twice, filled | – |
| lt_evict | <1 | 11 800 | natural (0.0000) | limit 30 s | 12147/2/**11800** | 298 | **0** | 0/0 |
| lt_evict_b05 | <1 | 11 800 | forced 0.05 | limit 30 s | 12147/2/**11800** | 298 | **0** | 0/0 |
| sr_retain | 1 | 12 087 | n/a | client_ok 4.55 s | n/a | n/a | k sent twice, filled | – |
| sr_evict | <1 | 12 087 | natural (0.0000) | limit 30 s | 12147/64/**12147** | 80 | 62 | 0/70 |
| sr_evict_b05 | <1 | 12 087 | forced 0.05 | limit 30 s | 12147/2/**12087** | 70 | 0 | 60/0 |

**[E] The divergence starts at the source, not at the resend.** At ρ < 1 the
whole object goes through the encoder before any loss is judged. k = 11 800's
first send is at t = 44 ms. At ρ = 1 it is at t = 1.535 s, so the source is
bounded there; which brake binds was not separated. The first gap report, at
t = 72–79 ms, already shows the window frozen at `win=[11947,12146]`, while the
receiver's frontier is at 3 953 (t = 130 ms). The link pays for this. The sim's
40 Mbit link drops **1 110 client→server datagrams on queue overflow**
(`drop_queue`, t = 0.574–0.820 s) against 91 random losses. In the other ρ < 1
scenario it drops 1 158 against 108, and at ρ = 1 none against 170. This is
D1b: there is no source-rate bound at ρ < 1 (`net/mod.rs:2473`).

**[E] The lost tail (lt_evict): resends never start.** k is below the window
from its first judgment. The first sweep line naming k is
`gap_clamped n=2896 gap=[11800,11800] win=[11947,12146] t_us=…1044000`, and
every later sweep is the same line. The ρ = 1 counterpart at the same point
is `send seq=11800 n=2 site=batch dropped=0` (t = 3.180 s), then `fill`. k has
nothing above it at the receiver (every later send was swallowed), so no
expiry passes it. The receiver's ack stays at 11 800, `retransmit_buffer.first()`
stays at k, and the sweep stays aimed at a below-window seq (D1a + D1c = D1d).
**The budget plays no part:** with the budget forced to 0.05 the run is
identical (`lt_evict_b05`: no shed, no resend), because the shed check sits
after the clamp.

**[E] The in-window tail (sr_evict) at the natural budget does not stall.**
The shed refusal (`denied`, 70 times, on every tail seq 12 087..12 146) falls
through to the resend (`net/sender_phases.rs:866–1091`). The sweep resends
one seq per sweep, the receiver's frontier walks 12 088 → 12 146 at about two
seqs per 100 ms, and the sender's ack reaches 12 147. Why `first()` is in the
window here: the earlier below-window holes (e.g. 11 482, clamped at
t = 0.94 s) had data above them, so the receiver passed them and its ack
pruned them [E for the ack; whether by hold expiry or by force-drain was not
separated]. **The transfer still never completes.** The perf server completes
an object only when every chunk was delivered (`perf.rs:144`,
`got.len() == total`). Those passed holes are missing chunks. Their count
was not totalled: the only `[SHED-R]` sample, `holes=4212` at frontier
11 508, is rate-limited and counts expiry passes only. [I] Most of them are
the queue-overflow drops above, which the sender never sees as such: they
were evicted from the window before any report named them. So at ρ < 1
`client_ok` is not a liveness oracle: any abandoned seq fails it.

**[E] The forced-budget shape (sr_evict_b05).** All 60 tail seqs are shed, no
copy is sent, the ack stays at 12 087 and nothing tells the receiver. The node's
"receiver frontier stays at 10856" is a stale sample: `ev=frontier` prints
only from `push` (`net/reorder.rs:92`), so it falls silent when arrivals
stop. The receiver's own ack shows its frontier reached 12 087.

**Why the budget reads 0.**
- **[E]** It is refreshed only inside `emit_source` (`net/emit_source.rs:620–641`,
  in `fn emit_source` at `:315`), so it freezes at the last source emission,
  here t ≈ 0.05 s. `bud=0.0000` is printed on every `[DIAG]` line of `sr_evict`.
  The shed deadline `shed_deadline_us_live` is refreshed in the same block and
  freezes with it: every client `[DIAG]` line reads `D=13ms` (32 of 32) while
  the same lines read `rtt=844.4ms` once the queue has filled.
- **[E]** The law is `residual_loss_after_fec(ε̂, r, A*, σ²) = ε̂·(1 − P_fec)`,
  which returns 0 for ε̂ = 0 (`control/fec_rate.rs:784–790`). It has no ρ
  argument, and `contract_rho` is the literal 1.0 (`net/sender_policy.rs:616`),
  read by no budget site.
- **[I]** ε̂ was 0 at that instant because the first ack round had barely
  started (the loss estimator was not sampled here).

**Answer.** At the natural budget the *lost tail* stalls because it is below
the window from its first judgment. The source has no rate bound, so the
window passed k within 35 ms of k's send (sent at 44 ms, window at the end of
the object by 79 ms). The clamp removes every sweep and
NACK before either the shed or the resend is reached, and nothing above k
ever reaches the receiver. The 0 budget is a separate defect (cold, ρ-blind,
frozen after the last source). It decides shed versus resend only for
in-window seqs. In-window, a refusal means "resend", which works but is slow
(one seq per sweep).

## 4. Input to the rule's derivation

The owner's rule (node `rec-evict-blocker-unresolved`, main `13fe8637`) has
three parts plus the budget law. For each: what the answers establish, what
the derivation must settle, and any contradiction.

**Sufficiency, in one line.** Shedding below the window and telling the
receiver should end the stall [I]: the floor lets the receiver pass k, and
`first()` moves to a servable seq. But it does not bound how much is
abandoned (part 3 does), and it cannot turn a completion-based test green.

**(1) The verdict is judged on the unclamped gap; the clamp bounds only what
is served.**
- Established: the clamp is correct as a serve bound. Below the window there
  are no bytes, and the fallback is a useless repair (section 1). Moving the
  verdict before the clamp is the right split. It is the identity at ρ = 1,
  where there is no clamp.
- Established: a below-window seq can get only one verdict, "abandoned". A
  budget refusal there means "resend", which cannot be served, so for these
  seqs the budget can only *count*, not *decide*.
- To settle: what the tail sweep targets. It names only `first()`
  (`net/mod.rs:3035`). With part 2 removing abandoned seqs, `first()` moves to
  the oldest servable seq, which settles it, but the derivation should say so.
- To settle, and a hazard [E for the freeze]: judging before the clamp uses
  `shed_deadline_us_live`, which freezes with the budget (`D=13ms` all run,
  beside `rtt=844.4ms`). Judged against a stale 13 ms, every queued seq is
  past its deadline. The verdict's D and budget must be refreshed where
  verdicts are taken, not on source emission.

**(2) Every verdict removes the seq and reaches the receiver, even when no
data follows.**
- Established: no message exists today (section 2). The new fact is a floor,
  and it must be repeated or acked because the control lane is unreliable.
- **Contradiction, in its naive form "eviction is the verdict, tell the
  receiver the window start".** The size-cap eviction (`net/mod.rs:3594–3597`)
  does not read the ack. In these runs the window start was 11 947 at
  t = 79 ms, while the receiver's frontier was about 4 000 and about 8 000
  symbols were still in flight on the link (section 3, [E]). A floor equal to
  the window start would have told the receiver to skip data that then
  arrived. Late fills below the frontier are delivered at once, out of order
  (`net/reorder.rs:76–82`). So an eviction verdict means "the sender can no
  longer serve it", not "lost". The receiver may pass a hole below the floor
  only when its own in-flight bound (hold H or the deadline D) has also
  expired, and the derivation must state that bound.
- To settle: whether eviction of an un-acked seq should exist at all at
  ρ < 1, or be made rare by (3). Every eviction verdict is also an
  abandonment that 1 − ρ must account for (below).
- Not sufficient alone [E for the oracle]: at ρ < 1 any abandoned seq fails
  the perf object (`perf.rs:144`). So "shed below the window" plus "tell the
  receiver" can make `sim_lost_tail` resolve k, but cannot make
  `sim_shed_reach`'s `assert_live` green, which asserts `client_ok`. The red-first
  test of the rule must assert the frontier passing every verdict seq (and the
  sender's ack reaching the end), not object completion.

**(3) A ρ-continuous source-rate bound replaces the reliable-only intake
pause.**
- Established: this is where the stall starts. The unpaced source put the
  whole object through the 200-symbol window in about 44 ms. The link then
  dropped 1 110 datagrams on queue overflow against 91 random losses, and
  those symbols were evicted before any report named them. Parts 1–2 turn
  these into prompt verdicts but do not reduce their number. Only (3) does.
  The overflow is a third kind of abandonment, next to sheds and evictions,
  and today nothing charges it.
- To settle: the bound's form. At ρ = 1 it must reduce to today's
  `store_len ≥ cap ∨ cwnd_full` (`net/mod.rs:2473`). At ρ < 1 it must keep
  abandonment ≤ (1 − ρ) of the source. A natural candidate is
  "un-acked seqs evicted per source ≤ 1 − ρ", which makes eviction of un-acked
  data itself the paced quantity. That is inferred, not measured.
- Note: the engine has no numeric ρ on this seat. `reliable` is a bool that
  selects the store, the clamp and `shed_on` (`net/sender_policy.rs:667`,
  `net/emit_source.rs:347`, `net/sender_phases.rs:795`). A ρ-continuous bound
  needs ρ as a number at these sites, which is the NO-MODE-SWITCH debt this
  rule inherits.

**The 1 − ρ budget law.**
- Established: today it is ε̂·(1 − P_fec). It reads no ρ, is 0 on a cold
  estimator, is refreshed only on source emission (so frozen once the source
  is exhausted), and charges only NACK and slot sheds. Evictions are never
  charged, and neither are receiver give-ups (thousands here, the
  `[SHED-R]` sample reads 4 212, against 0 sender sheds).
- What it must become: a bound on all abandonment (sender sheds plus evicted
  un-acked seqs, the set part 2 makes visible) against the *declared* 1 − ρ,
  refreshed where verdicts are taken. At ρ = 1 it is 0, which is the identity
  with retention. ε̂·(1 − P_fec) is then a predicted *use* of that budget (what
  FEC leaves), not the budget itself.
- To settle: whether the budget is enforced on the source (part 3) and only
  accounted at the verdict, since a below-window verdict cannot be refused.
  The section-3 traces say it must be: no budget can refuse an abandonment
  that has already happened.
