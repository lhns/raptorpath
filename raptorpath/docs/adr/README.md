# Architecture Decision Records

One row per retained ADR. Numbers are stable: gaps are ADRs that were superseded
or that only narrated experiments. They were deleted in 94bf58d; git history
keeps them.
ADR-0071 moved to [`../research/successor-candidates.md`](../research/successor-candidates.md).

Conventions in the ADRs below:

- "Ledger" citations refer to the measurement ledger that `docs/status.md` and
  `docs/measurement-discipline.md` replaced (in git history; ledger at ac1aed1).
- `§` numbers refer to the current paper, `docs/fec-arq-model.md`.
- For what ships today, [`../status.md`](../status.md) is authoritative.

| # | title | status | summary |
|---|---|---|---|
| [0002](0002-packet-framing-after-decode.md) | Packet framing after FEC decode | Accepted | Length-prefix framing (`net/framing.rs`) recovers IP packet boundaries after decode. |
| [0003](0003-loss-estimation-is-broken.md) | Loss estimation is broken | Accepted | Loss comes from batch-sequence gaps and ACK echo, not from received-only counts. |
| [0004](0004-decoder-memory-leak.md) | Decoder map grows without bound | Accepted | Completed decoders are dropped at once; stale ones are evicted after `DECODER_TIMEOUT`. |
| [0005](0005-ack-mechanism-missing.md) | No ACK/feedback loop | Accepted | Receiver→sender ACK, result, report and ping/pong messages close the control loop. |
| [0007](0007-rtt-calculation-broken.md) | RTT depends on clock sync | Accepted | RTT is measured by echoing the sender's own timestamp. |
| [0010](0010-handshake-and-versioning.md) | Handshake and protocol versioning | Accepted | `RPTQ` magic + `PROTOCOL_VERSION` (now 8) on every message; a mismatch is refused at handshake. |
| [0011](0011-channel-backpressure.md) | Channels stall under load | Accepted | Larger bounded channels; TUN inject drops with `try_send` rather than blocking. |
| [0012](0012-platform-setup-ux.md) | Platform setup UX | Accepted | TOML config, profiles, preflight `check`, the `setup` subcommand. |
| [0013](0013-monitoring-and-observability.md) | Runtime monitoring | Accepted | `SharedStats` atomics, an axum `/status` endpoint and the `status` subcommand. |
| [0014](0014-duplicate-symbol-handling.md) | Duplicate symbol detection | Accepted | Decoders skip already-seen symbol ids. |
| [0015](0015-graceful-shutdown.md) | Graceful shutdown | Accepted | Ctrl+C flushes, notifies the peer with `Shutdown` and closes cleanly. |
| [0017](0017-mtu-aware-symbol-sizing.md) | MTU-aware symbol sizing | Accepted | Symbol size follows the smallest path `max_datagram_size`. |
| [0018](0018-connection-migration.md) | Runtime connection migration | Accepted | Paths are added and removed at runtime through `POST/DELETE /paths`. |
| [0020](0020-tls-cert-pinning.md) | TLS certificate pinning | Accepted | Optional SHA-256 pinning of the server certificate (`--pin-cert`). |
| [0023](0023-gilbert-elliott-loss-model.md) | Gilbert-Elliott loss model | Accepted | A GE burst estimator feeds the rate controller; its burst multiplier was superseded by 0050. |
| [0031](0031-network-simulation-harness.md) | Network simulation harness | Accepted | Deterministic `SimChannel` + `MockClock` test harness in `tests/common`. |
| [0041](0041-simd-gf256.md) | SIMD GF(2^8) multiply-accumulate | Accepted | SSSE3/AVX2 split-table kernels behind the unchanged `gf256` API. |
| [0042](0042-bench-suite-consolidation.md) | Benchmark suite consolidation | Accepted | One `bench_suite.rs` replaces the ad-hoc benchmarks; see `benchmark-methodology.md`. |
| [0050](0050-fec-rate-control-redesign.md) | FEC rate control redesign | Accepted | BOCD loss quantile + spare-capacity budget replace the PI controller. |
| [0051](0051-canonical-evaluation-scenarios.md) | Canonical evaluation scenarios | Accepted | A scenario suite with explicit, falsifiable win conditions and a fidelity ladder (L0–L2). |
| [0052](0052-measurement-discipline.md) | L1 measurement discipline | Accepted | `measurement-discipline.md` is binding for every L1 verdict; refuted mechanisms retire in two stages. |
| [0054](0054-substrate-cc-policy-bbr-default.md) | Substrate CC is policy; BBR default | Accepted | `RWM_QUIC_CC` selects quinn's controller; BBR is the default and Cubic an opt-out. |
| [0055](0055-mtu-floor-1350.md) | MTU floor 1350 | Accepted | `min_mtu = initial_mtu = 1350` defeats quinn's PMTU black-hole false positive. |
| [0056](0056-systematic-wire-sparse-decoder.md) | Systematic wire + sparse-aware decoding | Accepted | Known sources never enter the matrix; decode cost scales with the deficit. |
| [0059](0059-per-path-recovery-clocks.md) | Per-path recovery clocks | Accepted | RFC 9002 loss detection generalized per path (`RWM_RECOV_MP`). |
| [0060](0060-sack-clocked-store-release.md) | SACK-clocked store release | Accepted | SACKed symbols free their store slot but keep their payload until the cumulative ACK. |
| [0061](0061-anchor-hygiene.md) | Anchor hygiene | Accepted | Anchors are measured-seed, discard clock gaps, and let floors expire. |
| [0062](0062-copa-wire-signal-competitive-mode.md) | Copa wire signal + competitive mode | Accepted | Copa-sole is the queue/tail arm of the CC surface; competitive mode is built but off. |
| [0063](0063-rstar-window-mass-provisioning.md) | r* window-mass provisioning | Accepted | r* provisions the window loss-mass quantile (`RWM_RSTAR_TAIL`); realized through the span machine. |
| [0064](0064-unified-span-machine.md) | Unified span machine + δ-honest shedding | Accepted | One decoder and one continuous span law across δ; shedding within the 1 − ρ budget. |
| [0068](0068-copa-bbr-fusion.md) | Copa/BBR fusion | Proposed | One δ-priced controller over a measured rate model; targets measured, nothing built. |
| [0069](0069-block-mode-disposition.md) | Block mode is legacy | Accepted | The block/window fork is the last mode bit; the default stays until the re-test in `status.md` §4. |
