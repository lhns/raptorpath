# ADR-0062: Copa Wire-Signal + Competitive Mode; the CC-Independence Finding

## Status: Accepted (Copa-sole = the queue/tail arm of the CC policy surface; `RWM_COPA_COMPETE` built, default OFF; CC-flip gate moved to contention recovery)

**Date**: 2026-07-13 … 2026-07-19

## Context

With substrate CC a policy surface (ADR-0054), the engine's own per-path
Copa-lite could BE quinn's window (`RWM_QUIC_CC=passthrough`) — fed for
the first time with clean plain-mode delivery samples. Copa-sole v1 held
a 3–6× tighter standing queue everywhere and killed plain-BBR's c3
collapse mode, but earned only 0.4–0.6× BBR's bulk throughput. Separately,
Copa-lite had no TCP-competitive mode (Copa §4/§2.2 unbuilt) and no
cross-traffic cell had ever been measured — gating any CC default flip
(it also carried BBR's unevaluated fairness).

## Decision

1. **Copa wire-signal** (`feat/copa-wire-signal`): wire-clocked delay
   term (the standing wire sample, not the window min), hint→δ mapping
   with no new constants (δ = 0.5/ζ; measured knee AT the mapped Bulk δ),
   Copa's real velocity/update law, aggregate-correct pace rate.
   Result: 0.86–0.89× BBR at single-c2 and PARITY at C8 (1.01×/0.95×)
   with a ×18–25 tighter slow-path queue and σ collapsed — at C8
   Copa-sole strictly dominates BBR-under.
2. **Copa §2.2 competitive mode built** (`RWM_COPA_COMPETE`, default
   OFF): loss/delay-regime detection + AIMD on 1/δ composed with the
   δ(hint) base — faithful, unit- and liveness-proven.
3. **The finding that moved the gate (CC-independence):** in the first
   shared-bottleneck battery, at the lossy c2 cell Copa-sole is
   cross-traffic-safe (0.88–0.90 share vs Cubic — Cubic-friendlier than
   BBR's 0.95–0.96). At the CLEAN bottleneck Copa-sole starves (share
   0.023) and competitive mode does NOT restore share — δ-null probes
   prove δ is not the binder: the starvation is the plain ARQ/1024-pool
   pipeline under contention tail-drop (Little's law — wall #7 at a
   shared bottleneck), a CC-INDEPENDENT blocker. BBR-under: 0.24 share at
   a 305–316 ms standing queue.
4. **Therefore: no Copa default flip; the CC-flip gate MOVES** from
   "no competitive mode" to the shared-bottleneck contention-recovery
   pipeline (contention-scaled pool / loss-burst NACK cadence /
   FEC-protected blocker retransmit — named successor, not built).

## Consequences

- The CC policy surface has two measured arms: BBR-under (bulk champion,
  default — ADR-0054) and Copa-sole (queue/tail champion, the δ-capable
  controller for latency-priced traffic). The endstate is hint-priced
  controller selection — policy, not a mode switch.
- Competitive mode is retained gated: correct mechanism awaiting the
  contention-recovery successor that would make its cell winnable.
- The battery is the system's fairness record for BOTH controllers; the
  BBR caveat documented at the ADR-0054 flip site comes from here.

## Evidence

- Ledger: goal-gate.md "Copa-Sole Substrate CC (2026-07-13)", "Copa
  Wire-Signal (2026-07-13)", "Copa Competitive Mode + Cross-Traffic
  (2026-07-19)"; CONSOLIDATED VERDICT §2.
- Paper: §12.11 (+ §12.4 addendum), §17.2.
- Commits: a895205 (passthrough shim + feed), f203d6e…386979d (wire-signal
  chain + measurement), 0f9bb2b + 0f0828b (competitive mode + battery).

## References

- ADR-0054 (policy surface + BBR default), ADR-0058 (the pool law that
  owns the contention blocker), ADR-0009/0019 (the engine CC lineage).
