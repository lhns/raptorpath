# ADR-0079: Delete the taper copy branch: every proactive slot carries a coded symbol

## Status: Accepted (owner decision 2026-11-19, status "§51 block 51 Result, Addendum 3"). Engine change pending: `docs/derivations/s06-delete-plan.md`. Derivation: `docs/derivations/s06-plost-resend.md` on branch `s06-derivation` @ `5ebf41a5` (§4.1, §4.1.1, §4.1.2, Q1).

## Context

Paper §3.2 let each proactive correction slot copy a source symbol with probability P_lost instead of sending a coded repair, as a "smooth transition from FEC to ARQ" (`net/emit_source.rs:739–822` @ `d1e952cf`). Law ledger S06 measured that branch on all 152 §51 sweep rows (status Addendum 2):
- It reads `retransmit_buffer.first()`, the cumulative-ack blocker, on its original send age, and never counts its own resends in flight. Between two cumulative advances it copies the same seq on every slot.
- Copies run at 4.0× (c2 Auto) and 6.3× (c8 Auto) the wire losses. At least about 70 % of them land below the receiver's decode floor, already received.
- At c8 the copy share of slots is 0.98, so no proactive coded repair is sent on two paths at any measured δ above Bulk.
- NACK resends stay at about 1 per source loss on every arm, with or without copies, and with or without decode closes (derivation §4.1.1).

The copy candidates are holes that the NACK path already resends, once per SRTT, from the same gap reports. So at ρ = 1 a copy adds no degree of freedom over the NACK resend. It is only a second, earlier trigger for the same correction.

No accepted ADR decides §3.2's FEC-to-ARQ transition. `docs/adr` has no hit for P_lost, taper copy or probabilistic retransmit. The near hits decide neighbouring things, and none of them decides the copy branch:
- ADR-0059 decides the NACK path's per-path recovery clocks;
- ADR-0063 decides how many proactive slots there are (`TaperBudget`), not what they carry;
- ADR-0064 decides the shed law, which runs on both call sites.

## Decision

1. **The copy branch is deleted.** Every proactive debt slot carries a coded symbol, at the trailing span when the window allows it (S10), otherwise over the leading window. The copy count is 0 at every δ, ρ and N. There is no hint, threshold or seat input, so the change is trivially continuous through every preset.
2. **Retransmission is reactive only.** A named hole is recovered by the NACK resend (ADR-0059's clocks, once per SRTT), the receiver's stalled-hole refresh (ρ = 1), the sender's tail sweep, or decoding.
3. **The slot count does not change.** r(β(δ)) and `TaperBudget` (ADR-0063) are untouched. The slots the copies took become coded repair.
4. **ADR-0064's shed law is kept.** It keeps the same `shed_allowed` function, deadline D(δ) and 1 − ρ budget at its NACK call site (`sender_phases.rs:846–869`). That site judges every seq of every judged gap batch, a superset of the slot path's `first()`. The slot call site goes with the branch.
5. **The rejected alternative is S06′:** a posterior over resends in flight, with one draw per symbol per send epoch (derivation §2–§5). It keeps "copy with probability π" from §3.2, which the derivation calls UNDERIVED. It would therefore need a FORMULA-FIRST waiver, new per-seq state and per-path inputs (E01, E09) that the engine does not have.

## Consequences

- **The copy cost is gone at ρ = 1.** By arithmetic, not measurement (derivation §7): at c2 Auto, coded symbols rise from 0.072 to 0.198 per source, and at c8 Auto from 0.004 to 0.242. Goodput is unchanged, also by arithmetic (status Addendum 2), because the slot count is the consumed r. Getting goodput back needs the rate law (R02) to come down, and R02's size now waits for a reading without copies.
- **Lost at ρ < 1: the slot-time shed trigger between gap batches.** A past-deadline blocker that no gap batch names, before the tail sweep fires, is no longer shed at slot time. On the evicting seat the receiver's hold expiry (60–300 ms) force-delivers past it. That interval is the only difference in the shed set (derivation §4.1.2).
- **Unknown: in-order delay at c8.** NACK fires there at a run-mean live-flight age of 3.0–3.6 s (law ledger C02), and today the first copy per stall may be the only early trigger. Coded repair cannot reach the blocker, which is outside the trailing span. The merge battery (plan §6, Stage A) registers c8 in-order delay two-sided, with no direction predicted:
  - **a rise** would mean the copy was the only early trigger at c8, so C02's 3 s NACK age dominates once it is gone;
  - **a fall** would mean the redundant copies were congesting p1.
  - **Resolution:** the c8 in-order d80 was 50 % at 6 v 6 (status §51). Stage A runs 12 v 12 and does not raise n further. It reports the reading only, and a move below 50 % counts as unresolved. The reading goes to the owner.
- **P_lost stays in the paper (§3.2, A.2) as the single-send posterior.** After the deletion, no engine decision reads it. The wasm model's ARQ trigger, the test harnesses and the math pins still read it (paper §11.3).
- **Ledger S06 closes by deletion (fix class delete).** The status, ledger and roadmap updates are the coordinator's, made with the engine merge.
- **Re-ask:** the decision record names no condition for bringing a copy rule back. That is an open owner question.
