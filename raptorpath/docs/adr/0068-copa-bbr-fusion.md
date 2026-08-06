# ADR-0068: A better Copa — δ-priced probing over a measured rate model with ε̂-referenced loss discrimination

## Status: Proposed (prerequisite battery COMPLETE — see MEASURED BASELINE addendum, 2026-08-06; targets are now numbers)

**Date**: 2026-07-21 (design conversation recorded; no build scheduled)

## Context

The substrate-CC policy surface (ADR-0054) carries two measured
controllers, and each one's advantage over the other is STRUCTURAL, not
incidental:

- **BBR's structural advantages over Copa are exactly its explicit rate
  model** (BtlBw/RTprop as first-class measured quantities): robustness
  to delay-noise (WiFi/LTE frame aggregation makes RTT a poor congestion
  signal — a delay-based law backs off against link-layer artifacts),
  fast bandwidth discovery (the 1.25× ProbeBW gain finds new capacity in
  ~one probe cycle where Copa climbs at v/δ), shallow-buffer viability
  (a rate-model controller does not NEED a queue excursion to find the
  rate; Copa's dither must be able to park 1/δ packets), and policer
  survival (a token-bucket policer presents loss at a rate ceiling with
  no queue buildup — invisible to a pure delay signal, modeled explicitly
  by BBRv2-class controllers).
- **Copa's structural advantage is δ**: the latency price as a
  first-class parameter of the utility U = log(tput) − δ·log(delay),
  giving a continuous throughput↔queue dial (measured: the δ frontier's
  knee sits AT the hint-mapped value, goal-gate "Copa Wire-Signal") and
  the natural ~5-RTT queue drain that needs no ProbeRTT-class forced
  stall (no FEC protection gap, paper §12.3) and no ProbeBW-class
  overshoot (the measured ×18–25 tighter C8 slow-path queue).

Today these live as TWO controllers behind `RWM_QUIC_CC`, selected by
the hint's declared price — a hint-selected mode switch on the policy
surface. That violates the vision axiom (VISION-TRIAGE-2026-07): ONE
continuous mechanism parameterized by the contract, no mode switches.
The δ knob already spans the continuum in principle; what Copa-lite
lacks is the rate model that makes BBR robust where Copa is fragile.

## Decision (proposed)

Fuse the two into ONE controller — "a better Copa" rather than a Copa/BBR
switch:

1. **Copa's δ-priced delay control** stays the outer law: the target rate
   1/(δ·d_q), the velocity dynamics, the wire-clocked d_q
   (paper §12.4 addendum), δ(hint) = 0.5/ζ with `RWM_COPA_DELTA` as the
   continuous override. δ remains the ONLY latency knob.
2. **A BBR-style rate model as the feed-forward baseline**: cwnd is
   anchored on measured BtlBw·RTprop (the per-path send-interval anchors
   that Anchor Hygiene (ADR-0061) already made honest — the fusion needs
   no new estimator), so delay-noise cannot talk the controller below the
   measured pipe, shallow buffers do not starve discovery, and the
   δ-term prices only the queue the model says we added.
3. **Probe amplitude and dwell DERIVED from δ** (no new constants): probe
   above the model baseline until the MEASURED queue excursion reaches
   the δ budget (1/δ packets / d_q* = 1/(δ·μ̂)), then drain — δ→0
   recovers BBR-class 1.25×-style discovery (large excursions allowed,
   bandwidth found fast), δ large recovers Copa's gentleness (the dither
   IS the probe). Probe frequency derives from the max-filter age (probe
   when the BtlBw sample window is going stale), the same law BBR's
   10-round cycle approximates with a constant.
4. **ε̂-referenced loss discrimination** (the card generic CCs lack: this
   transport carries a live channel-loss estimator): measured loss ≤ ε̂
   is channel noise — FEC's job, no rate response (paper §12.1); measured
   PERSISTENT loss > ε̂ is congestion or a policer — respond with a
   bounded-inflight regime, BBRv2-style. The channel estimator turns
   BBRv2's fixed loss-threshold constant into a measured reference.

## Consequences / prerequisites (why this is Proposed, not Accepted)

- **NOT buildable-falsifiable on the current rig.** The L1 cells are
  clean-delay netem paths with deep buffers and no policers — exactly
  the regime where the fusion's advantages over plain Copa CANNOT show
  (measured: Copa-sole already reaches its documented bulk class there,
  and the 2026-07-22 "Copa-Sole on Clean Substrate" battery is the
  clean-rig endgame either way). Building the fusion against the current
  cells would measure noise and invite motivated tuning.
- **Prerequisite: adversarial cells + measured Copa breakage.** Before
  any build: (i) add delay-jitter (aggregation-class), shallow-buffer,
  and policer cells to the L1 harness; (ii) MEASURE Copa-sole's breakage
  on them (the pre-registered baseline — predicted: delay-noise
  under-utilization, shallow-buffer discovery failure, policer
  starvation); (iii) pre-register the fusion's predicted recovery per
  cell per ADR-0052 item 11. If Copa-sole does NOT break on the
  adversarial cells, the fusion has no falsifiable target and stays
  unbuilt.
- **Literature verification required before build.** The design leans on
  three published mechanism families — the BBRv2 bounded/loss-aware
  model, PCC-Vivace-class priced utility optimization, and
  Nimbus-class elasticity/cross-traffic detection. Verify authors,
  venues, and the actual mechanisms from the sources at build time; no
  guessed citations enter the paper or the code comments.
- The bulk target the fusion must meet is set by the measured record:
  whatever the "Copa-Sole on Clean Substrate" battery (goal-gate) leaves
  as the standing bulk class — parity if the flip landed, the residual
  gap if it did not — plus the queue/tail class Copa already owns.
- Until then the policy surface stands as shipped: the δ-parameterized
  controller and the explicit reference arms under `RWM_QUIC_CC`
  (ADR-0054, as amended by the 2026-07-22 battery's flip decision).

## Evidence (for the context claims)

- Ledger: goal-gate.md "Copa-Sole Substrate CC" (#80), "Copa Wire-Signal"
  (#82: 0.86–0.89× bulk, δ-knee at the mapped value, C8 domination,
  wireQ ×18–25), "Copa Competitive Mode + Cross-Traffic" (clean-cell
  contention starvation is CC-independent), "Copa-Sole on Clean
  Substrate" (2026-07-22).
- Paper: §12.4 (+ wire addendum), §12.11, §17.2.
- BBR-side structural claims are the deployed-BBR literature's; they are
  design rationale here, to be source-verified at build time (above).

## References

- ADR-0054 (substrate CC policy), ADR-0061 (the anchors the rate model
  reuses), ADR-0062 (Copa wire signal + competitive mode), ADR-0052
  (pre-registration discipline the prerequisite battery must follow).
- VISION-TRIAGE-2026-07 §5 (the follow-ups register).

## MEASURED BASELINE addendum (2026-08-06, branch `meas/adversarial-cells`) — the prerequisite battery ran; the fusion's targets are now numbers, and the analysis above is partly WRONG

The "adversarial cells + measured Copa breakage" prerequisite (items
(i)+(ii) of the Consequences section) is DONE: goal-gate "Adversarial
Cells (B1)" — delay-jitter dose-response (20 ms base, {0,5,15,25} ms
correlated jitter, c2-class rate/loss), 8-packet shallow buffer
(tbf + child netem), and a 100 mbit token-bucket policer (ingress
police, drop-without-queue), each cell mechanism-validated before
measurement, seeds 42+7, interleaved, shipped BBR-under vs Copa-sole
(+compete arms). Measured baseline (Mbit/s, s42 · s7):

| cell | BBR-under | Copa-sole | Copa/BBR |
|---|---|---|---|
| c2ctl (clean) | 81.3 · 76.9 | 74.1 · 71.4 | 0.91 · 0.93 |
| jit0 (40 ms RTprop, no jitter) | 79.2 · 75.7 | 29.9 · 29.0 | 0.38 |
| jit25 | 66.6 · 69.7 | 20.5 · 20.0 | 0.31 · 0.29 |
| shal8 | **9.8 · 10.0** | **75.3 · 78.8** | **×7.7–7.9 INVERTED** |
| pol100 | 8.1 · 8.0 | 8.0 · 8.0 | 0.99 · 1.00 |

What this does to the Context section's structural-advantage claims:

1. **Delay-noise: breakage CONFIRMED in shape, re-attributed in
   mechanism.** Copa's goodput decays strictly monotonically with
   jitter (both seeds), but the dominant collapse (0.38×) is already
   present at ZERO jitter — the binder is the 1024-slot store's
   Little's-law dwell ceiling under Copa's empty-pipe hole recovery at
   40 ms RTprop (gauges: sinfl=sout=1024 pinned; dwell ~250–350 ms vs
   BBR's ~68 ms). A rate-model feed-forward ALONE is therefore not
   predicted sufficient for this cell: the fusion must keep the pipe
   fed across recovery rounds WITHOUT BBR's 42–66 ms standing queue.
2. **Shallow-buffer viability: the claim was BACKWARDS for the
   BBR we actually ship.** quinn's BBRv1-class controller COLLAPSES at
   the 8-packet buffer (0.12× its clean class, 7.3% sustained loss;
   the engine's max-filter delivery anchor reads ~10× the link under
   token-bucket burst-quantized delivery — measured poisoning), while
   Copa holds its FULL clean class there. The δ outer law ALREADY owns
   this cell; the fusion's rate model must be burst-robust or it will
   regress the fusion's own motivating cell.
3. **Policer survival: CC-INDEPENDENT starvation — the cell does not
   discriminate controllers.** Both arms pin at 8 Mbit (0.10–0.12× the
   clean class) at identical ~3.8% police-drop fractions with zero
   queue: the token-exhaustion loss BURSTS stall the recovery/frontier
   pipeline behind either controller (the 2026-07-19 contention-binder
   family). The ε̂-referenced bounded-loss regime cannot show value here
   until that pipeline survives burst drops — a named PREREQUISITE,
   ahead of any CC build.
4. **Competitive mode buys nothing on these cells** (pre-registered and
   confirmed): the §2.2 detector structurally cannot fire on a policer
   (no queue signal) and did not fire on the shallow cell; C/B
   0.97–1.01 everywhere.

**The fusion's falsifiable targets (fixed by this battery):** c2ctl
≥ ~0.95× BBR-under with Copa's 5–6 ms wireQ class; jit0–jit25 ≥ 0.9×
BBR-under across the dose-response without the standing queue; shal8
≥ Copa's own 75–79 Mbit (no regression); pol100 carries NO CC-side
target until the burst-loss recovery prerequisite is fixed. The
realtime crown survives jit15 (p99 medians 92–96 ms vs 36–39 clean,
wire-class inflation only) — the fusion inherits that bar too.
Evidence: goal-gate "Adversarial Cells (B1)" (2026-08-06), paper
§16.33.
