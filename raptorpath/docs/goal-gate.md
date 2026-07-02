# Goal Gate — surpass the TCP-style baseline + model reacts correctly

Executable form of the project goal, at fidelity level **L0** (in-process
simulation per ADR-0051). Run:

```
cargo test --test gate_suite -p raptorpath --release -- --test-threads 1
```

**Status: GREEN** (2026-07-02, 12/12 tests, 10 trials/cell, fixed seeds,
95%-CI separation required for every win).

## G1 — surpass the SimRetx baseline (ADR-0051 win conditions)

Baseline = **SimRetx**, NOT "real TCP": reliable ARQ transport model with
slow-start + AIMD congestion window, min-SRTT multipath scheduling, and
TCP in-order delivery semantics. Channels are the paper §2.4 GE
parameterizations (h_B = 1, so ε = p/(p+q) exactly).

| Cell | Win condition | raptorpath | SimRetx | Verdict |
|------|---------------|-----------:|--------:|---------|
| C1 DC (tie cell) | completion within 2% (+1 RTT allowance), overhead ≤ 1% | 0.025 s, 0.6% | 0.027 s, 0.1% | tie (actually faster) |
| C2 WiFi | compl ≤ 0.9×, p99 ≤ 0.7× | 0.185 s / 28 ms | 0.818 s / 53 ms | 0.23× / 0.53× |
| C3 LTE | same | 0.97 s / 84 ms | 3.76 s / 272 ms | 0.26× / 0.31× |
| C4 Satellite | same | 1.60 s / 356 ms | 23.0 s / 1565 ms | 0.07× / 0.23× |
| C5 Bad WiFi | same | 0.41 s / 34 ms | 2.26 s / 81 ms | 0.18× / 0.42× |
| C7 dual sym | beat dual min-RTT AND best single | 0.110 s | 0.405 s (single 0.867) | 0.27× |
| C8 dual asym RTT | same | 0.177 s | 0.695 s (single 0.816) | 0.25× |
| C9 outage | goodput ≥ 90% steady within 3×RTT of path recovery | 8/10 trials ≤ ~34 ms | — | pass |

## G2 — the model reacts correctly

- Estimator converges to each channel's true (ε, q) (per-symbol GE feed).
- Controller rate re-converges within 25 batches of a 1%→10% regime change
  (measured: 1 batch, BOCD).
- Spare-capacity gate clamps FEC monotonically to spare; zero spare → zero FEC.
- Outage: ε̂ saturates ≤ 10 batches, P_lost → >0.99, recovery ≤ 30 batches.

## What building the gate found and fixed

Production bugs (fixed in src/):
1. **σ²_burst sentinel blow-up** (`fec_rate.rs`, `raptorpath-math`): on very
   clean channels the GE estimator's decayed Bad-state counters empty out and
   `p_bg()` returns its 0.0 no-data sentinel; treating it as a measurement
   made σ² ≈ 2/p̂ ≈ 4000 and over-provisioned DC links ~50×. No data now
   maps to iid (σ² = 1). Paper §8.3 note added.
2. **Reorder-buffer stranding** (`net/reorder.rs`): `drain_expired` advanced
   `next_deliver_seq` past still-pending younger entries — a hole filled by
   FEC just after a later entry expired sat for a full extra timeout.
   Expiring seq k now releases everything pending up to k, in order.
3. **Estimator burst-bias API** (`control/estimator.rs`): `record_batch`
   lumps losses (overestimates σ²_burst ~2×). Added `record_counts` +
   `record_symbol` so SACK-informed callers feed the true loss pattern
   (paper §7.5).

Model/driver findings (documented in the paper):
4. **Continuous r\*** (§8.4): quantile at 1 − δ/ε — rate glides to 0 when
   pure ARQ meets the tail target; no cutoff branch; hint enters only via z.
5. **Steady-state taper shape invariance** (§4.2): aggregate correction
   rate = r regardless of shape; a global-offset τ(t) accumulator (latent
   bench_suite bug) decays to zero and starves repair generation.
6. **Jitter-horizon encoder lag** (§14.24, new): repairs covering
   not-yet-arrived symbols park as deep pivots; lagging the encoder by
   L = J × send_rate symbols made hole-fill ~10× faster.
7. **Pacing**: windowed-count pacing degenerates into RTT-synchronized
   mega-bursts; token-bucket at Copa's cwnd/SRTT keeps the send process
   smooth (and a delay-based CC keeps queues empty — that, not just FEC,
   is half the p99 win vs the AIMD baseline).
8. **Fast path-recovery signal**: blending the GE state-conditional loss
   rate (paper C.6 ε_burst) into path selection detects outage recovery
   within one delivery instead of ~6 estimator batches.

## Honest scope

- L0's baseline is our own simulation model. It now includes slow-start,
  AIMD, and in-order semantics, but it is not CUBIC/BBR. **The claim this
  gate supports is "surpasses the SimRetx model under ADR-0051 conditions",
  not "surpasses real TCP".**
- The loss feedback timing is per-RTT-batch with sender-side knowledge of
  wire outcomes (oracle timing, same convention as bench_suite).
- Next fidelity level (L1, ADR-0051): real CUBIC/BBR/quinn/MPTCP stacks
  over netns + netem — requires Linux/WSL2; the win conditions transfer
  unchanged.
