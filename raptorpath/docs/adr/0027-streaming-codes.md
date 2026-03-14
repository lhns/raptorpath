# ADR-0027: Streaming Codes (Badr/Martinian Delay-Optimal)

## Status

Resolved

## Context

raptorpath's sliding-window FEC backends (RLC, METTLE) use random linear combinations or
peeling decoders without awareness of the channel's burst structure. On bursty wireless
channels (WiFi, LTE), a burst of B consecutive losses can overwhelm random codes that are
only provisioned for independent losses.

Streaming codes (Badr et al. 2017, Martinian & Sundberg 2004) are the theoretical optimum
for burst+random erasure channels: they achieve the streaming capacity C(T,B) = T/(T+B)
with guaranteed recovery within delay T, using a two-layer construction that separately
handles burst and random losses.

## Decision

### Two-layer encoder/decoder

Implement `StreamingEncoder` and `StreamingDecoder` as a new `WindowEncoder`/`WindowDecoder`
backend (`FecBackend::Streaming`):

**Burst layer** — diagonal interleaving with stride T:
- Source symbol at position i is XOR'd with symbols at {i, i-T, i-2T, ...}
- Creates T independent diagonals; a burst of B hits at most ⌈B/T⌉ per diagonal
- Recovery: if one symbol is missing from a diagonal, XOR the others to recover it
- Pure XOR (GF(2)) — zero multiplication cost

**Random layer** — GF(256) random linear combinations:
- Identical to RLC repair symbols: random coefficients from the same PRNG
- Rate = ε/(1-ε) where ε is the random loss rate
- Recovery: incremental Gaussian elimination (same as `RlcWindowDecoder`)
- Handles residual random loss not caught by the burst layer

### Parameter derivation

Parameters are computed from the Gilbert-Elliott HMM and loss estimator:
- **B**: `ge.mean_burst_length() × safety_factor` (over-provisioned)
- **T**: set to B (delay = burst tolerance). Caller can override for multipath RTT.
- **ε**: `estimator.loss_rate_upper(0.95)` (95th percentile upper bound)
- **Safety factor**: 15% for Realtime, 10% otherwise

`FecRateController::compute_streaming_params()` computes these from the live estimator.

### Integration

- `FecBackend::Streaming` variant added to the backend enum
- Selected via `--fec-backend streaming` or `fec_backend = "streaming"` in TOML
- Window sender creates `StreamingEncoder` with params from the channel estimator
- Receiver creates `StreamingDecoder` with default params (refined as channel info arrives)
- Block-mode fallback uses RaptorQ (streaming codes are window-only)

## Consequences

- Burst recovery is structurally optimal: T independent diagonals vs random hope in RLC
- Random layer provides the same random-loss correction as RLC
- GE estimator (ADR-0023) directly feeds B and ε into the code parameters
- Delay constraint T is explicitly tracked — symbols are recoverable within T positions
- Two recovery paths (burst XOR + random GE) cascade: burst recovery can unblock random pivots

## References

- Badr et al., "Layered Constructions for Low-Delay Streaming Codes," IEEE Trans. IT, 2017
- Martinian & Sundberg, "Burst Erasure Correction Codes with Low Decoding Delay," 2004
- Fong et al., "Optimal Streaming Codes for Channels with Burst and Arbitrary Erasures," 2019
