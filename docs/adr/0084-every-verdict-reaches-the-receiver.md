# ADR-0084: Every verdict reaches the receiver

## Status: Accepted (owner decision 2026-11-20: every default of `docs/derivations/evict-verdict-law.md` §9). Engine change pending. Paper: §5.1, §5.6, §6.1, §6.3, §11.1, §11.3, A.4, A.5. Supersedes in part ADR-0064, ADR-0079 Amendment 1 and ADR-0081, and the owner's 2026-11-14 `budget = min(1 − ρ, R)` decision (`giveup-law-model.md`); extends ADR-0010.

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
   - **ADR-0064 decision 3:** the budget operand `ε̂·(1 − P_fec)` with the cumulative shed count becomes `Ab` against 1 − ρ, and "compiled out at ρ = 1" becomes "budget 0 at ρ = 1". The law's form (age > D(δ) within a 1 − ρ budget, past-budget candidates served) stays.
   - **ADR-0079 Amendment 1:** a below-window blocker is no longer "judged by no sender path"; a shed now reaches the receiver; the shared budget now reads ρ. Its merge hold on `s06-delete` is unchanged.
   - **ADR-0081's identity** becomes `S_sent = A + H + E + |U|` (`O_strand ≡ 0` by construction), with a verdict seq staying in its term after a later ack, and abandonment counted at the receiver, not as `H + E`. This also supersedes the `E` term of `test-detection-plan.md` §7 answer 5.
   - **The 2026-11-14 owner decision** `budget = min(1 − ρ, R)`.
   - **`core-extraction-recovery.md`** decisions 3 and 5, the intake redundancy pin and step 4.

## Consequences

- The lost tail resolves: every abandoned seq is passed by the receiver within a sweep-bounded time, and the account closes with `|U| = 0` at every ρ.
- Realtime keeps δ-honest shedding through its declared ρ; without the declaration its budget would be 0.
- Costs at ρ < 1: one refcount increment and ring insert per source symbol (payloads are shared `Bytes`), resident memory up to the cap B, and floor repeats at most `|channel_paths|` datagrams per `TAIL_SWEEP_MIN_US`.
- On real links B sits at its N·2048 ceiling (ledger F01, PINNED), so the bound acts as a constant outstanding cap until the store law's time form lands. Its bind gauge is reported at ρ < 1.
- A step at ρ = 1 remains on the receiver: the reorder buffer never passes a hole at ρ = 1 and gives up on its ε̂_recv budget below it. A receiver law with ρ is a separate derivation (question 5), bounded meanwhile by a record-only step test.
- ADR-0078 and the receiver-timestamp plans (status §42, §43; roadmap `rec-framing-followups`) gave v11 to the receiver-timestamp field. That field moves to the next bump after this one.
- The engine change touches the store fill, the serve, the clamp bounds, the intake pause, the sweep arming, the receiver reorder buffer and the wire; it is a behaviour change at ρ < 1 and rides a guard batch. Its tests are the derivation's §6 (T1–T8), red first.
