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
  MTU wedge, fixed in ADR-0055; partly BBR's own). Copa-sole strictly
  dominates BBR-under at C8 on queue/tail metrics (ADR-0062).
- Note: this ADR concerns the SUBSTRATE (quinn) controller. ADR-0019
  (the engine's own BBR-style delay CC) is unaffected.

## Evidence

- Ledger: goal-gate.md "Gen Substrate Ceiling (2026-07-13)" (the wall
  named + raised; ×3.4 alone), "Default CC Flip (2026-07-21)" (flip +
  identity check), "CONSOLIDATED VERDICT" wall #1 row.
- Paper: §12.11, §16.17, §17.2.
- Commits: 0d9f26e (`RWM_QUIC_CC` lever + wall diagnosis), 519467e
  (default flip), 7145fcc (identity-check binary).

## References

- ADR-0019 (engine delay-based CC), ADR-0062 (Copa wire-signal +
  competitive mode), ADR-0067 (the composed default stack).
