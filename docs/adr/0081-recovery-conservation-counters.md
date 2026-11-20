# ADR-0081: The sender accounts for every source seq, and every test checks the account

## Status: Proposed (2026-11-20). Plan: `docs/derivations/test-detection-plan.md` S4. Identity superseded in part by ADR-0084: `S_sent = A + H + E + |U|`, abandonment counted at the receiver.

## Context

On the evicting seat (ρ < 1):
- the NACK site clamps a gap to the coding window before the shed check, so a gap below the window is neither served nor shed (`net/sender_phases.rs:790–805`);
- the tail sweep re-targets `retransmit_buffer.first()` even below the window, so it does nothing (`net/mod.rs:2789`, `:3035`);
- a lost tail with no later traffic is never resolved.

Shed counters read 0 because the code never ran, and no test could tell that from "nothing needed shedding". Every one of these states is visible as a seq that is outstanding and that no recovery channel can serve.

## Decision

1. **One identity per sender**, printed at every DIAG tick and at the `final=1` exit flush:

   ```
   S_sent = A + H + O_serv + O_strand
   ```

   - `S_sent`: source seqs sent;
   - `A`: acked;
   - `H`: shed;
   - `O_serv`: outstanding seqs the gap server would serve;
   - `O_strand`: outstanding seqs below the gap server's own clamp bound.

   The counter reads the bound `serve_gaps` computes, so on the reliable seat `O_strand` is 0 by construction. It adds no seat branch (NO-MODE-SWITCH).
2. **Stuck witnesses:** `inert_sweeps` (sweep fires whose gap sent nothing), `oldest_unchanged` (sweeps with the same oldest un-acked seq), and `O_hwm` against the effective store cap.
3. **Every counter carries an execution count:** shed-check visits, clamp evaluations.
4. **`tests/common/loopback.rs` asserts the account at the end of every completed transfer:** the identity closes, `O_strand` = 0, `O` = 0, `inert_sweeps` = 0. The helpers every loopback test calls run it.
5. **Shipping, in two steps.**
   - First under `#[cfg(any(debug_assertions, feature = "test-seams"))]`, so the debug suite and the seams tests (which run in release and are the ones that reach the lost tail) both carry it. The release bytes are expected unchanged, which the `.text`/`.rodata` hash check proves; then no merge guard is needed.
   - The exit line prints whatever `RWM_DIAG` says, so every loopback test can assert it. Only the per-tick fields stay behind the DIAG gate.
   - Then in the release binary, on the next scheduled guard batch with the host ns bench, so L1 batteries carry the witness too. Hot-path cost: integer increments at existing counter sites, and an O(log n) range count on the DIAG tick only.

## Consequences

- The four stuck states become assertion failures in any test that runs the evicting seat with tail loss. The S06 tests on `wedge-diag` are the first such tests. A seat no test runs stays unseen.
- A zero counter reading now comes with its visit count (MEASUREMENT DISCIPLINE 1).
- The counters touch `net/mod.rs`, `sender_phases.rs`, `emit_source.rs` and `diag.rs`, so they land in the same chunk as the wedge fix, red first.
- Open for the owner: is a seq evicted from the window without a verdict a legal outcome at ρ < 1? If yes, the identity gains an `E` term bounded by 1 − ρ. This draft assumes no: ADR-0064's shed is the only sanctioned loss.
