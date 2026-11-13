## 30. A noise-sized measurement window for the power-seeking law (D) — derivation, no VM — verdict

§29 left one question: can D be rescued by measuring each dither half over
W rounds instead of one, W sized from the measured variance of ln Π so its
standard error falls below q/2, and does the convergence time that W costs
fit inside a transfer? Answered on paper from the existing ledgers; no
code, no build, no VM contact. Derivation, provenance and the alternatives:
paper §6.1, "Candidate (D-W)". Scripts and their outputs:
`docs/l1-raw/loop-diag/p29/derive30/` (`pcap.py` one-round noise from
`[PCAP]`; `win.py` 250 ms static-cap noise from `[LOOPDIAG]` of the §27 DEF
and §29 KNEE rows; `alpha.py`/`dial.py` the same per α across the dial;
`sim.py` mean-field convergence and goodput; `checks.py` the D/T split, the
block variance and quinn's cwnd against the cap).

**Verdict: `DOES-NOT-CLOSE`.**

*Per-cell numbers (Bulk, q = 0.1, target σ ≤ q/2 = 0.05).* σ₁² =
var(Δ ln Π)/2 over the p29 PWR periods (one round per half); σ²_win the
same over 250 ms windows at the static cap nearest n*, with no controller;
γ the log-log slope between them; T at n*; transfer = p29 MAIN's.

| cell | var(Δ ln Π) one round | σ²_win (rounds) | γ | W* rounds | one decision (period) | MAIN transfer | reach in ⅓ transfer |
|---|---|---|---|---|---|---|---|
| c2-100 | 0.128 | 0.0014 (13) | 1.48 | 9 | 0.38 s | 8.9 s | ×1.71 |
| c8-100 | 0.078 | 0.0061 (5) | 1.14 | 11 | 1.2 s | 8.3 s | ×1.17 |
| c1s-400 | 0.71 | 0.036 (52) | 0.58 | ≥ 765 | ≥ 7.3 s | 4.3 s | ×1.01 |
| c1d-400 | 0.89 | 0.029 (49) | 0.70 | ≥ 579 | ≥ 6.0 s | 5.3 s | ×1.02 |

At c1s/c1d W* is a lower bound (white extrapolation beyond the data; the
measured γ < 1 makes the true value 2–7× larger), and it already exceeds
the transfer: the seeker gets less than one decision per transfer and
cannot leave its seed. Across the dial W* reads 7–9 at c2, 11–27 at c8,
≥ 424–765 at c1s and ≥ 494–579 at c1d (Realtime to Bulk); no dial position
changes the verdict.

*Convergence (mean-field, favouring D).* From the p29 seeds (the shipped
law's ask at its first refresh, 42–335) the c2 seeker converges in 11–38 %
of its transfer but loses −7.7 / −5.2 / −0.9 Mbit/s against the ±4.7 band
(two of three outside); c8 converges from one seed of three (30 %), the
other two lose −13 and −16 against ±12.9; c1s and c1d never converge and
lose hundreds of Mbit/s. From MAIN's settled cap the goodput holds at every
cell, but only because the seeker never reaches the knee within the
transfer (c2 70 % to never; elsewhere never), so it runs at MAIN's cap and
latency — the thing D was for is not delivered. D-W closes only from a
seed within ×1.7 of n* at c2, ×1.17 at c8, ×1.01 at c1, which is the knee
location the law was meant to find without a link-rate input. Separately,
the c1s fixed point fails the band by itself: §27's static G(384) = 632
against p29 MAIN 748 ± 92 (cross-session; the p29 KNEE row at 1 024 reads
713 against §27's 741, so the p29 binary is not faster at a static cap).

*Why.* The noise is not one thing. At c2 one round's ln D carries a
granularity term that averages faster than 1/W (γ_D ≈ 1.4) and ln T is
white, so 9 rounds suffice. At c1s/c1d both D and T are red (γ ≈ 0.5–0.7,
correlated only −0.2 to −0.3): 250 ms at a fixed cap cuts the variance
×10–15, not ×50, and leaves σ = 0.17–0.19 against 0.05. It is also not
stationary: c1d reads σ²_win 0.48–0.49 at §27's 512 and §29's 768 KNEE.
The §29 clamp-bind gauge conflated two things: below the knee ê ≈ α = 2
saturates the clamp legitimately, so `bind` must be read against the
predicted |ê| on each side; the sign agreement at chance is the noise
witness.

*Alternatives, each sized the same way (paper §6.1):* averaging ê across
periods (twice the time of a held window, plus a loop delay that is
unstable at c2: bound 0.35 against a gain of 0.48); a step decaying with
sign agreement (Kesten; agreement far from n* is 0.504–0.518 at c1, so
telling it from ½ takes ≈ 60 000 periods — the rule freezes the walk); a
median half (×1.4–2.5 at c1, needs ×10+); a sequential (Wald) half (≈ ×2);
a larger dither (A ≥ 0.36 at c1s, seven times the q/2 band, and wider than
c1s's flat Π top); a noise-adaptive W (under-estimates the half-mean
variance by W^(1−γ), ×16 at c1s). None closes c1; all are continuous in δ
with ρ's hole term additive as before, so the no-mode-switch rule is not
what kills them. Not derived, named: path memory across transfers
(RFC 9040 temporal sharing), which moves the decision budget from one
transfer to the path's lifetime (≈ 5.5 s of transfer per unit ln n at c2,
≈ 2 min at c1s) and would need its own derivation and ADR.

*ADR-0068 open question 12 (nested loops), for D.* quinn's cwnd sat below
the store cap in bytes in 18–24 % of post-warm-up windows at c2, 4–32 % at
c1s, 0–14 % at c1d, 0–3 % at c8 (p29 PWR rows, an end-of-window snapshot).
While the cwnd binds, Π is flat in n̄ and ê is zero-mean noise: a plateau
random walk with no restoring force. Where the cap binds, the seeker is a
sampled integrator with per-period gain 0.16–0.48, locally stable. BBR's
ProbeBW cycle averages out inside a W*-round half; its 10 s ProbeRTT and
the c1 red noise sit at the half's own scale. The question stays open,
with an addition: any nested store law needs an authority gauge (share of
time the cwnd rather than the cap binds) beside its bind gauges.

*What this leaves for the store law.* D's power criterion is right about
*where* the knee is (c8 371 vs 384, c1s 391 vs 384 in §29) and about the
latency it buys; no online estimator of it fits inside one transfer at c1,
and at c2/c8 it fits only from a seed that already knows the knee. The
shipped pooled law stays. What a store law still lacks is the one input
§23–§27 could not find: the knee location (or the link rate L) under the
cap. The c1 noise floor that defeats D is plausibly receiver-side (the
batching tail of §6.1; the c1d ceiling is the server's single receiver
task, status §16 finding 4 — an attribution, not measured here), in which
case receiver work changes this arithmetic before any store law can. No pre-registration follows; nothing is built.
