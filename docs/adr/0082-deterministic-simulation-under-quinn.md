# ADR-0082: A deterministic simulation of both peers, with the virtual link under quinn

## Status: Accepted (owner decision 2026-11-20, on Fable's review; conditioned on main's CI green with all four sim merges in, at 680d5aa7 and 8276ee39). Phase 1 implemented; phase 2 (Gilbert-Elliott and Weibull links, a CPU-cost model) not implemented. Plan: `docs/derivations/test-detection-plan.md` S8.

## Context

- Recovery-path tests spawn the real binary over loopback UDP and wait on wall time. On a loaded host they are flaky: one passed 4/10 on one tree and 1/10 on another.
- A lost tail with no later traffic needs tens of seconds of real time to show that it is never resolved.
- There is no transport trait to simulate below: the engine holds a concrete `QuicTransport`. The L0 netem shim sits above quinn, so quinn's own controller sees a clean link.
- quinn's `Runtime` trait already lets the engine supply timers, the clock and the UDP socket (`task_obs.rs` `TimedRuntime` wraps it). Its tokio runtime reads tokio's clock, which pauses.
- The engine reads `std::time::Instant::now()` directly 121 times outside test files, and that clock does not pause.

## Decision

1. **One engine clock function** returning `tokio::time::Instant::now().into_std()` replaces every direct `Instant::now()` read and the `now_us()` base. Without tokio's `test-util` feature that call is `std::time::Instant::now()`, so release behaviour is unchanged. The proof ladder applies: hashes, then the decision pins, then the ns bench, then the guard unless waived.
2. **The simulator runs both peers in one process** (`perf::server`, `perf::client`) on a current-thread tokio runtime with paused time, with an in-memory `AsyncUdpSocket` given to quinn through a `Runtime` wrapper. The L0 shim's link model (rate, delay, jitter, GE, Weibull bursts) moves under the socket, seeded.
3. **The oracle** is the recovery account of ADR-0081, as amended by ADR-0084, plus the per-seq `test-seams` trace. The account is not on main yet (branch `recovery-counters`).
4. **Not turmoil, not `scheduler::Clock`.** turmoil has no quinn socket and does not own time for quinn. `MockClock` moves reads but not tokio's timers. The wasm sim is a model and drives no engine code.
5. **Simulator code is behind the `sim` cargo feature**, never in the release binary. In the release binary there are only pass-through seams. These are the clock function, the `clock.rs` hooks called from `transport/quic.rs` (certificate and client config), and `shutdown_signal`'s cfg split, whose sim-only body is not compiled into release.
6. **Determinism rules.**
   - One seeded RNG for the engine and the link.
   - Ordered maps where iteration order reaches behaviour under `sim`.
   - `--cfg tokio_unstable` in sim builds only, for a seeded `select!` order.
   - quinn's connection ids, stateless-reset key and initial dcid seeded through its public config hooks, and a fixed certificate key.
   - No port or OS value as an RNG input.

## Consequences

- Recovery scenarios run in milliseconds of real time and give the same event order for the same seed, so timing tests stop being flaky and move here.
- The clock change touches most of `net/`. It lands as one chunk at a quiet point between engine batches.
- **What a seed fixes (measured 2026-11-20).** For a given tokio version, one seed gives the same trace and log on Linux and Windows. The `sim-cross-os` CI job compares 7 cells byte for byte, with a red control. Two causes were found and fixed. First, the server port was hashed into the link RNG (fixed in `sim.rs`). Second, `shutdown_signal` drew tokio's per-thread RNG differently on unix (one body under sim on every OS). Connection ids, the reset key, the initial dcid and the certificate are seeded under sim. Still unseeded: rustls randomness and the NEW_TOKEN key (content only, never a length), and 3 diagnostic `SystemTime` reads. Platform float differences are not ruled out beyond those cells. Any new platform-specific `select!` or `watch` reached under sim breaks cross-OS equality.
- **What the simulation cannot show.**
  - Sim builds may order maps differently from the product, so product iteration-order bugs are invisible to it.
  - It has no CPU or backpressure cost model yet, so an unpaced source runs at infinite speed.
  - It is fast for stalls only; bulk transfers run at about 1.9 x real time.
- **The release binary stays byte-identical to a tree without the sim.** Today it is proven per merge with the `binid` workflow. A standing gate, running on every push that touches the sim, clock, seam or account code or the crate manifest, is owed on branch `ci-gates`.
- **A green simulation test does not replace the VM guard** for anything timing- or load-dependent.
- Fidelity: no kernel stack (GSO, receive buffer, real pacing). Claims about those stay L1's.
