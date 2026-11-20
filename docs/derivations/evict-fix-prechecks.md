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
- **[E] After the source is exhausted** the sender emits only what the
  repair and recovery paths produce: the tail sweep's synthesized gap (clamped
  away when `first()` is below the window, item 1) and whatever proactive
  repair the window still funds. None of it names a seq the receiver may skip.
  `Shutdown` is sent only on engine shutdown (`net/mod.rs:3046–`) and the
  receiver does not flush its reorder buffer on it.

**Answer.** Today the receiver learns of an abandoned seq only by inference
from later arrivals. A lost tail with nothing above it is therefore never
passed, at any budget, and neither a shed nor an eviction changes that.
"Tell the receiver" needs a new sender→receiver fact (a floor below which the
sender has abandoned every seq), and it must be sent when no data follows,
so it cannot ride on data symbols alone.

## 3. Why the lost tail stalls at the natural budget

(traces below)

## 4. What this implies for the rule's derivation

(below)
