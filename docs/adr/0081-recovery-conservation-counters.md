# ADR-0081: The sender accounts for every source seq, and every test checks the account

## Status: Accepted, option B (owner, 2026-11-20). Plan: `docs/derivations/test-detection-plan.md` S4.

## Context

On the evicting seat (ρ < 1):
- the NACK site clamps a gap to the coding window before the shed check, so a gap below the window is neither served nor shed (`net/sender_phases.rs:790–805`);
- the tail sweep re-targets `retransmit_buffer.first()` even below the window, so it does nothing (`net/mod.rs:2789`, `:3035`);
- a lost tail with no later traffic is never resolved.

Shed counters read 0 because the code never ran, and no test could tell that from "nothing needed shedding". Every one of these states is visible as a seq that is outstanding and that no recovery channel can serve.

## Decision

1. **One identity per sender**, printed at every DIAG tick and at the `final=1` exit flush:

   ```
   S_sent = A + H + E + O_serv + O_strand
   ```

   - `S_sent`: source seqs sent;
   - `A`: acked while still inside the coding window;
   - `H`: shed;
   - `E`: evicted without a verdict: left the coding window while outstanding, then released by the cumulative ack. Legal; `H + E` is bounded by the 1 − ρ budget. 0 at ρ = 1 by construction;
   - `O_serv`: outstanding seqs the gap server would serve;
   - `O_strand`: outstanding seqs below the gap server's own clamp bound.

   The counters read the bound `serve_gaps` computes (`account::clamp_lo`, pinned against `gap_clamp` by a unit test), so on the reliable seat `E` and `O_strand` are 0 by construction. It adds no seat branch (NO-MODE-SWITCH). Each term is measured at its own site (insert, cumulative prune, shed, buffer count), so the identity is a check and not a definition.
2. **Stuck witnesses:** `inert_sweeps` (sweep fires whose gap sent nothing), `oldest_unchanged` (sweeps with the same oldest un-acked seq), and `O_hwm` against the effective store cap.
3. **Every counter carries an execution count:** shed-check visits, clamp evaluations.
4. **`tests/common/loopback.rs` asserts the account at the end of every completed transfer:** the identity closes, `O_strand` = 0, `O` = 0, `inert_sweeps` = 0, `H + E` within the budget. `assert_no_dnf` (so `transfer` and `lossy_run`) runs it on the client log, which must carry an exit line.
5. **Shipping, in two steps.**
   - First under `#[cfg(any(debug_assertions, feature = "test-seams"))]`, so the debug suite and the seams tests (which run in release and are the ones that reach the lost tail) both carry it. The release bytes are expected unchanged, which the `.text`/`.rodata` hash check proves; then no merge guard is needed.
   - The exit line prints whatever `RWM_DIAG` says, so every loopback test can assert it. Only the per-tick fields stay behind the DIAG gate.
   - Then in the release binary, on the next scheduled guard batch with the host ns bench, so L1 batteries carry the witness too. Hot-path cost: integer increments at existing counter sites, and an O(log n) range count on the DIAG tick only.

## Consequences

- The four stuck states become assertion failures in any test that runs the evicting seat with tail loss. The S06 tests on `wedge-diag` are the first such tests. A seat no test runs stays unseen.
- A zero counter reading now comes with its visit count (MEASUREMENT DISCIPLINE 1).
- The counters touch `net/mod.rs`, `sender_phases.rs`, `emit_source.rs` and `diag.rs`, so they land in the same chunk as the wedge fix, red first.
- Answered by the owner (2026-11-20): a seq evicted from the window without a verdict is legal at ρ < 1 and counts in `E`; `E` above the budget, or `O_strand` > 0 at the end of a completed transfer, is a defect.
- `E` is an upper bound on abandonment, not abandonment: a seq that left the coding window before its ack arrived counts in `E` even when the receiver delivered it (lost tail at ρ < 1: `A` = 1, `E` = 11799, nearly all delivered). The loopback helper reports `H`, `E` and the budget and gates only `H` where no budget was ever open. The contract check proper is the receiver's own count of holes it passed undelivered against the 1 − ρ budget; the receiver keeps that count only when shedding is armed (`recv_shed_holes`) and `[ACCT-RX]` prints it as `holes_given_up`.
- The end-of-transfer check on `O` is judged against the receiver's delivered frontier (`[ACCT-RX] frontier=`, printed under the same cfg every report interval and at exit): an outstanding seq below it is an ack in flight, not a stuck seq. The client does not wait for its last ack, so the release shutdown is what is tested.
- "Inert" today means: the sweep sent nothing and the clamp excluded a gap of its batch. Once the verdict fix lands, a sweep may legitimately send nothing because it issued a verdict (a shed), so "inert" must then mean no send AND no verdict.
