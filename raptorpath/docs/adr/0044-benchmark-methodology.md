# ADR-0044: Benchmark Methodology Audit

## Status
Accepted

## Context

An audit of the benchmark suite revealed several methodology issues that undermined the validity of reported results:

- **MPTCP min-RTT bug**: The MPTCP scheduler selected the same path for every packet because it compared raw RTT constants rather than tracking smoothed RTT. All traffic went to the primary path, making MPTCP results identical to QUIC single-path. Table 5 comparisons were therefore meaningless.
- **Table 3 cold-start contamination**: The LossEstimator started from a Beta(1,1) uniform prior instead of a pre-warmed state. Early iterations blended cold-start transient behavior with steady-state measurements, distorting ablation conclusions.
- **50ms clock step**: All simulation ticks were 50ms apart, which quantized every latency measurement to 50ms multiples. The reorder buffer (25ms timeout) could never fire between ticks -- it always expired within a single step, masking its actual behavior.
- **Datacenter GE model is uniform**: The datacenter Gilbert-Elliott channel used p_gb=0.0, meaning the bad state was never entered. This reduced the GE model to plain uniform loss, defeating the purpose of having a burst-loss channel model.
- **No bursty loss sweep**: Table 1 swept only uniform loss rates. There was no coverage of burst-loss behavior, so backends that handle correlated loss differently from independent loss were never distinguished.
- **Inconsistent satellite channel**: SimChannel and ReliableSimChannel defined the satellite preset with different RTT and loss parameters, producing incomparable results across tests that used different channel types.
- **No congestion/queuing model**: All channels had infinite capacity and no buffer limits. FEC overhead could not cause queuing or tail-drop, so the cost of redundancy under load was never measured.
- **No cross-path loss correlation**: Path failures were always independent. Real wireless environments exhibit correlated fading (e.g., device entering a tunnel affects all radios), which was never tested.

## Decision

Eight code changes were applied to fix the identified issues:

1. **LinkModel**: Added a bottleneck link abstraction with finite capacity (bytes/sec) and a finite buffer (bytes). Excess packets are tail-dropped. This lets benchmarks measure FEC overhead interaction with queuing.

2. **CorrelatedFading**: Added a per-tick probability of forcing both paths into the GE bad state simultaneously. This models shared environmental fading (e.g., device shielding, tunnel entry) that affects all radios at once.

3. **GE helper and satellite preset**: Added a `ge_for_target_loss` helper that computes GE transition probabilities for a target loss rate and mean burst length. Added `SimChannel::satellite()` as a canonical preset with consistent parameters. Added doc comments explaining each GE parameter.

4. **Table 1b -- bursty loss sweep**: Added a GE bursty loss sweep (Table 1b) alongside the existing uniform sweep (Table 1a). Both use the same loss rates but Table 1b uses mean burst length of ~3 packets, directly revealing which backends degrade under correlated loss.

5. **Clock granularity**: Reduced simulation tick interval from 50ms to 2ms in all timed tables. Latency measurements now have sub-RTT resolution. The reorder buffer timeout (25ms) spans multiple ticks and can function as designed.

6. **MPTCP min-RTT fix**: The MPTCP scheduler now tracks per-path SRTT using EWMA (alpha=0.125) and selects the path with the lowest current SRTT for each packet. SRTT updates on every ACK, so path selection responds to changing conditions.

7. **Pre-warm Table 3 estimator**: Table 3 initializes the LossEstimator via `make_estimator_for_loss(0.025)` followed by 10 PI feedback cycles before measurement begins. This eliminates cold-start transient from ablation results.

8. **Table 3 congestion channels**: Table 3 now uses `wifi_congested` and `lte_congested` channel presets with finite link capacity, correlated fading enabled, and tail-drop counting. The table reports tail-drop counts alongside latency and goodput.

## Consequences

- Table 1b directly shows which backends handle burst loss differently from uniform loss.
- Table 3 p99 latency varies continuously rather than appearing as 50ms multiples.
- Table 3 reorder buffer timeout now functions across multiple ticks, revealing its true contribution to latency.
- Table 5 MPTCP minRTT now differs from QUIC single-path because SRTT-based selection distributes load across paths.
- Table 3 reports tail-drop counts, which are non-zero under congestion and quantify the cost of FEC overhead.
- Satellite channel parameters are consistent across SimChannel and ReliableSimChannel, making cross-test comparisons valid.
- All feature-ablation conclusions are now steady-state measurements with no cold-start contamination.
- The new congestion model enables reasoning about FEC overhead interaction with queuing delay and packet loss.
