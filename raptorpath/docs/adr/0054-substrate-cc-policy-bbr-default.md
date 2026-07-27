# ADR-0054: Substrate Congestion Control Is Policy — BBR Default (`RWM_QUIC_CC`)

## Status: Accepted (default flipped 2026-07-21; Cubic retained as explicit opt-out)

**Date**: 2026-07-13 (policy surface), 2026-07-21 (default flip)

## Context

Wall #1 of the substrate chain: every datagram send is gated on quinn's
own congestion window, and quinn's default is loss-reactive Cubic. On GE
loss cells the transport's "15–17 Mbit link ceiling" — cited for a month
of verdicts — was Cubic collapsing under loss beneath the FEC transport's
own control loops (plain 17.5 → plain+BBR 74.5 pooled, ×4.3; gen-mode's
~10 Mbit/s per-path wall was the same controller amplified by
generation-mode standing-queue RTT inflation). A loss-tolerant FEC
transport must not ride a hidden loss-reactive CC underneath its own CC.
Additionally the engine's own per-path Copa-lite can BE quinn's window
(`passthrough`), giving a delay-based/queue-tight alternative (ADR-0062).

## Decision

1. **Substrate CC is an explicit policy surface**, not a fixed property:
   `RWM_QUIC_CC = bbr | cubic | newreno | passthrough` selects quinn's
   congestion controller (transport/quic.rs).
2. **BBR is the shipped default** (2026-07-21, roadmap Item 0): env unset
   ⇒ BBR. The A/B inverts — the legacy wire is the explicit
   `RWM_QUIC_CC=cubic` opt-out arm. Unrecognized values warn and keep
   BBR. Cubic is retained (dead as a performance choice; alive as the
   fairness-conservative and control arm).
3. **The endstate policy** (paper §17.2): the hint's declared price
   chooses the controller — bulk → bbr-under; latency-priced →
   passthrough+Copa. Policy, not a mode switch.

### Amendment 2026-07-22 — the mode switch is a MEASURED TRADEOFF, not a wish (goal-gate "Copa-Sole on Clean Substrate")

The pre-registered clean-substrate re-measure asked whether the
consolidated stack (SACK-release + recovery suppression + path pool +
anchor hygiene, all default ON) closed Copa-sole's bulk gap — i.e.
whether the two-value surface could collapse to ONE δ-parameterized
controller (`RWM_QUIC_CC` default → passthrough). It does NOT: the walls
lifted BBR-under's aggregation (c7 → 166, c8 → 82) while Copa's
δ-equilibrium caps its cwnd near BDP + 1/δ regardless of freed pipe, so
the gap GREW — copa/bbr 0.89× sc2, 0.73× c7, 0.57× c8, 0.66× dc1, ≫σ
both seeds (0.97× sc3 the lone parity cell). Copa's #82 C8 domination
was a broken-substrate artifact and inverted. **NO FLIP.** The surface
stays two-valued, honestly documented as a measured queue/tail-vs-bulk
tradeoff (Copa holds the network standing queue ×18/×16/×6–7 tighter at
sc2/sc3/c7, re-confirmed on this substrate) — NOT a mode switch to be
collapsed by wishing, and NOT flipped on a hope. The fusion (ADR-0068)
inherits the bulk gap as its target; this battery strengthens its
motivation (a rate-model feed-forward is exactly what would let a
δ-priced controller convert the freed pipe Copa leaves on the table).

## Consequences

- Every measured best arm since §16.17 already set `bbr` explicitly; the
  flip fixes the shipped binary and the local suites, which were still
  exercising the condemned Cubic path. L1 identity check passed (the
  default binary reproduces the measured bbr arms; sc2 ~80, sc3 ~15.5).
- Fairness caveat documented at the flip site: BBR takes 0.95–0.96 share
  vs one Cubic flow at the lossy c2 cell (Cubic is Mathis-bound there)
  and 0.24 share on a clean shared bottleneck (BBRv1 yields under
  bufferbloat) — within the deployed-BBRv1 envelope, measured in
  ADR-0062's cross-traffic battery.
- BBR-under's costs stay on the record: standing queue (38 ms at sc2,
  88–124 ms C8 slow path) and a c3/C8 bimodal collapse mode (partly the
  MTU wedge, fixed in ADR-0055; partly BBR's own). Copa-sole holds the
  network standing queue ×18/×16/×6–7 tighter (sc2/sc3/c7) at a bulk
  cost that GREW on the consolidated substrate (the 2026-07-22
  amendment); the #82 "Copa dominates at C8" claim is superseded — on
  the fixed substrate BBR-under leads C8 throughput 0.57× (Copa keeps
  only the queue).
- Note: this ADR concerns the SUBSTRATE (quinn) controller. ADR-0019
  (the engine's own BBR-style delay CC) is unaffected.

## Evidence

- Ledger: goal-gate.md "Gen Substrate Ceiling (2026-07-13)" (the wall
  named + raised; ×3.4 alone), "Default CC Flip (2026-07-21)" (flip +
  identity check), "Copa-Sole on Clean Substrate (2026-07-22)" (the
  no-flip tradeoff verdict, both seeds), "CONSOLIDATED VERDICT" wall #1
  row.
- Paper: §12.11, §16.17, §17.2.
- Commits: 0d9f26e (`RWM_QUIC_CC` lever + wall diagnosis), 519467e
  (default flip), 7145fcc (identity-check binary).

## References

- ADR-0019 (engine delay-based CC), ADR-0062 (Copa wire-signal +
  competitive mode), ADR-0067 (the composed default stack), ADR-0068
  (the Copa/BBR fusion — inherits this battery's bulk gap as its target).
