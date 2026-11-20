# The hole-lifecycle model (test-detection plan S7)

Crate `raptorpath-model/` (ADR-0083, Proposed). It is a hand-written [stateright](https://crates.io/crates/stateright) model of what happens to each seq over a lossy link. It runs under `cargo test --manifest-path raptorpath-model/Cargo.toml --release`.

**The model is checked; its match to the engine is assumed.** No engine code runs in it. Each step below copies the engine's order of decisions as read on github/main 54198a2a. If the engine changes, the model does not follow. Plan step S9 removes this caveat for the steps it moves into `raptorpath-core`.

The crate is its own workspace root, through an empty `[workspace]` table, with its own `Cargo.lock`. So stateright never enters the engine's lock file, and the root `Cargo.toml` is untouched. The engine suites (`tools/l1` `src_manifest`) do not build or cover it, so it is not added there.

## What is modelled

- **One model, with the seat as a value.** `Seat { reliable, shed budget 1 − ρ }`:
  - ρ = 1 is reliable with budget 0;
  - ρ = 3/4 is evicting with budget 1/4.
- **The model branches on `reliable` only where the engine does.** Each such site is cited in the code.
- **Bounds:**
  - coding window W = 3;
  - an object of 8 source packets;
  - at the object boundary, the sender either stops or keeps sending 2 packets of a next object (a nondeterministic choice);
  - every loss pattern with at most L drops per seq, counted over the original send and all resends; after that the link delivers;
  - L + 1 timer chances per hole;
  - reliable store cap 4;
  - evicting reorder capacity 4.
- **Every run ends.** Every budget is finite and the counters only grow, so the properties are safety properties checked on terminal states (states with no enabled action). There is no liveness check.

| Model step | Engine (github/main 54198a2a) |
|---|---|
| emit a source; it enters the retransmit buffer | `net/emit_source.rs:505` |
| correction slot on emission: `first()` → shed if allowed, else copy if servable (reliable: sent store `:346`; evicting: window), else coded repair (no effect here) | `net/emit_source.rs:757–826` |
| intake pause, reliable seat only: `reliable && store_len >= cap` | `net/mod.rs:2473` |
| window eviction to the last W seqs | `net/mod.rs:3594` (`MAX_WINDOW_SIZE` `:205`) |
| report processing: cumulative ack prunes the retransmit buffer and the shed set | `net/sender_phases.rs:1403`, `:1407` |
| gap: clamp to the window on the evicting seat, none on the reliable seat | `net/sender_phases.rs:795–802` |
| then per seq: already shed → skip; `shed_allowed` → shed | `net/sender_phases.rs:847–869`, `net/shed.rs:39` |
| then serve (resend) | `net/sender_phases.rs:871–` (gates abstracted, below) |
| shed armed only off the reliable seat | `net/shed.rs:23`, `net/sender_policy.rs:667` |
| tail sweep targets `retransmit_buffer.first()` and synthesizes `(seq, seq)` through the same judgement | `net/mod.rs:2789`, `:3035–3042` |
| receiver push: a late fill below the frontier is delivered at once; capacity force-drain (evicting only); contiguous drain | `net/reorder.rs:75`, `:88`, `:149`, `:97` |
| an arrival sends a WindowAck with SACK gaps | `net/receiver.rs:2007` |
| receiver timer armed only with an entry pending above the hole | `net/receiver.rs:725` |
| reliable seat: the timer re-advertises the gap (hole refresh) | `net/receiver.rs:860–968` |
| evicting seat: the timer is hold expiry, which gives up through the largest expired entry and then sends the unwedge ack | `net/reorder.rs:112`, `net/receiver.rs:973`, `:1031` |

**Properties.** A sender shed alone does not resolve the receiver's hole, so the two terminal properties are separate:

- *terminal: sender holds no outstanding seq.* Every seq is sent, and the retransmit buffer is empty.
- *terminal: receiver frontier passed every seq.* Every seq was delivered, or skipped by force-drain or hold expiry.
- *terminal, frontier moved: sender holds no outstanding seq.* This is the first property restricted to runs where something was delivered, so its counterexample is a tail lost behind a delivered prefix.
- *terminal, sender clean: receiver frontier passed every seq.* This is the second property restricted to runs where the sender is clean, so its counterexample is a hole the sender released that the receiver never passes.
- *always: reliable outstanding ≤ store cap.* This is the `O` ≤ store cap check.
- *sometimes: a run completes cleanly with a drop.* This guards against a vacuous model.

## What is abstracted, and why

- **FEC is off.** No seq is filled by decode, and a coded repair has no effect. FEC only adds fills, and under the plan's lost-tail condition the repairs die with the tail. A trace this model calls clean can only be cleaner with FEC. A counterexample here may be one that FEC rescues in a real run; the wedge test on the engine is the check for that.
- **One source per loop iteration.** The correction slot sees the window before this iteration's eviction, so it holds W + 1 seqs.
- **The ack path is lossless and coalesced.** The sender reads the receiver's latest snapshot. The snapshot-order filter (`judged_gaps`, `net/snapver.rs:120`) is therefore always true.
- **Timer gates are taken as ripe.** The per-seq cooldown, the age gate and the RFC 9002 time and packet thresholds count as ripe whenever a chance is taken. They only delay a serve, and a timer budget spent on suppressed fires would make the control fail for a reason that is not the protocol's. Repair budgets (`cached_max_repairs`, the NACK budget) are not modelled.
- **A timer fires only after its target's last copy has resolved.** This covers both the tail sweep and the refresh. The sweep deadline is last activity + 2×SRTT, more than one flight.
- **The link is one FIFO path.** Each copy is lost (within its budget) or arrives, in send order. A copy of a seq already queued coalesces with it. Multipath reordering is not modelled. An arbitrary-order link was tried first and did not finish under a 40 M-state cap.
- **The shed deadline.** D = min(b·RTprop, 2·RTprop), with b(Realtime) = ½ (`net/shed.rs:31`), measured from the original send.
  - A receiver gap report about a seq needs a later arrival and then an ack flight, so it is judged at least one RTprop after the send.
  - A tail sweep fires at last activity + 2×SRTT.
  - Both are therefore past D, so the model judges them past the deadline.
  - Only the correction slot at emission can see a younger seq, so there "past" is a choice. The seq it has just emitted is never past D.
  - On the reliable seat the deadline is never derived (`emit_source.rs:630`), and the budget is 0 anyway.
- **Hold expiry gives up through the lowest pending entry.** That is the first one to expire under in-order sending, and it skips the fewest seqs.
- **State canonicalization.** These are sound, because no later step reads them:
  - a seq that is neither outstanding nor in flight is never sent again, so its drop count and resend witness are cleared;
  - arrivals below the frontier are cleared.
- **The window's ack-driven advance** (`sender_phases.rs:1374`) only moves the start to at most the ack, so it never changes the clamp of an un-acked seq. It is not modelled.
- **Multipath, the store-cap law and cwnd** are not modelled. The cap is the constant 4.

## Results

L = 1, release build, one thread, on the host (2026-11-20). A debug `cargo test` runs both tests in 45 s.

| Seat | Unique states | States | Time | Result |
|---|---|---|---|---|
| reliable, ρ = 1 | 830 285 | 3 075 163 | 4.8 s | every property holds (full exploration) |
| evicting, ρ = 3/4 | 1 715 116 | 6 414 525 | 9.6 s | counterexamples (BFS stops once every failure property has one) |

The evicting seat's shortest counterexample to *sender holds no outstanding seq* is also its lost-tail one:

```
Emit ×8, Boundary{stop}, Deliver(0), Drop(1) … Drop(7), SenderReport, TailSweep, TailSweep
```

- seq 0 is delivered and acked, and seqs 1–7 are lost once each;
- the window is [5, 7] and the cumulative blocker is 1;
- both sweeps synthesize the gap (1, 1), and the clamp empties it before the shed check;
- seq 1 is never resent and never shed, and the receiver holds nothing above it.

- **Evicting seat: a counterexample exists.** It is the lost tail:
  - the tail is lost and the sender stops;
  - the window moves past the cumulative blocker;
  - every sweep of the blocker is clamped out before the shed judgement;
  - nothing above the blocker arrives, so neither hold expiry nor force-drain can pass it.

  This is the known defect diagnosed on branch `wedge-diag` (bc383f29, `lost_tail_longer_than_window_strands_k_evicting`). The test asserts that the counterexample exists and has this shape, both for the whole object and for a tail behind a delivered prefix (with the tail longer than W). Flip it when the defect is fixed.
- **Evicting seat, reported but not asserted.** A second shape is a receiver hole that the sender released:
  - seq 6 is lost and then shed by the correction slot while seq 7 is emitted;
  - seq 7 is lost, and its sweep sheds it (past D, within the budget 2/8);
  - the sender is clean, but the receiver frontier stays at 6, because no entry above it arrives to expire or drain.

  Whether a stalled frontier with nothing behind it matters to the application is an owner question. It is not one of the defects this step targets.
- **Reliable seat, the control:** passes every property at the same bounds.
