# ADR-0043: Information-Theoretic FEC Rate Controller

## Status
Accepted

## Context

The bench suite (ADR-0042) revealed three problems with the FEC rate controller:

1. **Over-provisioning.** Table 2 showed 12.5% repair overhead for 0.1% DC loss (125x
   the loss rate) and hit the 20% cap even at WiFi (2.5%). Root cause: five stacked
   safety margins multiplied together — (a) 95th-percentile Beta upper bound,
   (b) z*sigma*0.1 safety, (c) Realtime x1.2 + burst_extra, (d) GE burst
   x(1+ln(b)*0.10), (e) PI correction.

2. **RTT ignored.** Higher RTT means more symbols in flight and slower NACK feedback;
   the controller should provision more burst protection. Lower RTT means NACK can fill
   gaps cheaply. `compute_repair_rate()` did not use RTT at all.

3. **Table 3 undifferentiated.** The WiFi-bursty scenario at 20% FEC gave 100% recovery
   for all configs — even `no_nack` and `no_ge_burst` showed 0pp delta. The FEC budget
   was too generous for feature ablation to matter.

4. **Production ignored controller.** `net/mod.rs` used a hardcoded `REPAIR_FACTOR = 4.0`
   instead of `FecRateController::compute_repair_rate()`.

## Decision

Replace the stacked-multiplier formula with an information-theoretic optimal formula.

### Optimal formula

For an i.i.d. erasure channel with loss rate p, the information-theoretic minimum
overhead is `p/(1-p)`.

For a burst erasure channel with max burst length B and delay constraint T symbols
(Badr et al. 2017), the delay-constrained capacity is `C(T,B) = T/(T+B)`, giving
minimum overhead `B/T`.

T incorporates RTT naturally:
```
T = (RTT * throughput) / symbol_size
```

The optimal repair rate is:
```
base_rate = max(p/(1-p) + codec_overhead, B/T)
```

A single safety margin from estimation uncertainty replaces all five stacked margins:
```
margin = (z * uncertainty * 0.25).clamp(0.0, 1.0)
```

Protocol hint is an additive offset (+0.05 Realtime, -0.05 Bulk, 0 Auto) instead of
a multiplicative factor.

Final formula:
```
rate = base_rate * (1 + margin) + pi_correction + hint_offset
```

### Other changes

- **`compute_repair_count`** now delegates to `compute_repair_rate` (rate * k) instead
  of using a separate Newton solver. The Newton solver was principled but redundant —
  `p/(1-p)` is the closed-form limit of the same binomial constraint.

- **PI gains reduced** from kp=2.0/ki=0.5 to kp=0.5/ki=0.1. The old high gains
  compensated for an inaccurate base rate.

- **`ge_burst_factor` and `realtime_burst_extra` fields removed** from
  `FecRateController`. These are replaced by the B/T term and hint_offset respectively.

- **`symbol_size: u16` added** to `FecRateController` (needed to compute T).

- **Production send loop** (`net/mod.rs`) now uses `compute_repair_rate()` instead of
  `REPAIR_FACTOR = 4.0`.

- **Backend selector `threshold_high`** changed from 0.08/0.10 to 0.12 based on Table 1
  bench data showing block codes cliff at ~12-15% loss.

- **Table 3 FEC budget** reduced from 20% to 8% to force feature differentiation.

## Consequences

- DC overhead drops from ~12.5% to ~0.5-1.5% (near information-theoretic minimum)
- WiFi overhead drops from 20% (capped) to ~5-8%
- Production repair rate at 0.1% loss drops from ~12% to ~0.5%
- Table 3 ablation can now differentiate features at the tighter 8% budget
- B/T term only activates when GE estimator is valid and throughput > 0, preventing
  spurious burst protection during warmup
