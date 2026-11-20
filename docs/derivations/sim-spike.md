# S8 feasibility spike: deterministic simulation of the engine

Branch `sim-spike` (from `main` 54198a2a), throwaway. Nothing here goes to main. Plan: `test-detection-plan.md` S8, ADR-0082 (Proposed). Host: Windows, `test` profile (optimized + debuginfo). Linux was not run.

## Verdict

**Feasible, with the changes listed below.** Both perf peers run in one process on a current-thread tokio runtime with paused time, and quinn runs on an in-memory socket. A 2 MB transfer completes. The per-event socket trace and the engine's readout log are byte-identical across separate processes with the same seed: 20/20 runs on one path, 32/32 on two paths, 10/10 on three. A different seed gives a different trace. The lost-tail wedge on the evicting seat reproduces deterministically (3/3 identical runs). A no-blackout control resolves k. Every listed blocker was either needed or confirmed harmless by an ablation, and none needed more than a mechanical change.

## What was built

- `src/sim.rs` (~310 lines). `SimRuntime` wraps quinn's tokio runtime. Its `wrap_udp_socket` keeps the real bound socket alive but never polls it, and returns a `SimSocket` on a deterministic virtual address: the configured server port, or 127.0.0.1:50000+n for each `:0` client bind. Each direction is one FIFO link with 40 Mbit/s serialization, a fixed 10 ms one-way delay, a 500 kB drop-tail queue and seeded Bernoulli loss (SplitMix64, per direction). It can also add a blackout. `poll_recv` waits on one stored tokio `Sleep` per socket. Every TX (with its fate) and every RX goes into the trace as `virt_us TX|RX dir src>dst ord len fate`.
- The hook sits in `transport/io_owner.rs` `bind_endpoint`: under the `sim` feature, with a net installed, it wraps the runtime and sets `EndpointConfig::rng_seed`.
- `src/clock.rs`: one engine clock (`now()`, the `SimElapsed::sim_elapsed()` trait, `wall_us()`), one RNG (`random_f64()`) and `PathMap`. Without `sim` these are `std::time::Instant::now()`, `rand::random` and `HashMap`.
- `tests/sim_spike.rs` (~125 lines, needs `--features sim,test-seams`). One run per process. It builds the paused runtime with `rng_seed`, installs the net, and runs `perf::server` + `perf::client`, controlled by environment variables (seed, scenario, bytes, path count, ablation switches). A std-thread watchdog prints virtual versus wall time and aborts once a wall-time limit is passed.
- The S06 test seams from `wedge-diag` bc383f29 are ported (src only, separate commit), with `t_us` on the engine clock.
- `.cargo/config.toml` sets `--cfg tokio_unstable`, which `Builder::rng_seed` needs.

The driver runs the test binary as separate processes and compares traces with `md5sum` and `diff`. Reproduce with:
`SIM_SEED=7 SIM_LOSS=0.025 RWM_DIAG=1 target/debug/deps/sim_spike-*.exe --exact sim_run --nocapture`.

## Evidence

| run | outcome | virtual | wall | determinism |
|---|---|---|---|---|
| 2 MB, Auto, 1 path, 1–2.5 % loss | acked | 0.55–0.60 s | 0.2–0.5 s | 20/20 identical trace and log (seed 5); seeds 1 and 2 differ (8709 diff lines) |
| 140 MB, Auto, 1 path, 1 % loss | acked, 36.4 Mbit/s | 30.9 s | 16.5 s | 2/2 identical (398 906 events) |
| 2 MB, 2 paths | acked | 0.59 s | 0.25 s | 32/32 identical after the hash fix (before: 2 classes, 14/11 of 25) |
| 2 MB, 3 paths | acked | 0.56 s | 0.28 s | 10/10 identical |
| lost tail, Realtime, ρ < 1, 30 s | stalled (as expected) | 30.0 s | 0.7 s | 3/3 identical |

For the log comparison, the filter drops only the `[RCVBUF]` line (it echoes the real ephemeral port), the `wall_s` line and libtest's "finished in" line.

**Wall cost.** Idle virtual time is free: a 30 s stall takes 0.7 s. Busy virtual time costs the engine's real CPU per packet: a saturated 40 Mbit/s link runs only about 1.9× faster than real time. The plan's sentence that "a 30 s scenario runs in milliseconds" is true for tail and stall scenarios, but not for bulk throughput.

## Blockers: what bit and the size of each fix

Each fix was ablated: one switch turns it off, and two same-seed runs are diffed.

| # | blocker | bit? | evidence | fix (spike) |
|---|---|---|---|---|
| 1 | std clock reads | **yes** | With the reads on the std clock (`SIM_STD_CLOCK=1`), the transfer still completes, but the runtime is never idle, so virtual time tracks wall time: 3.7 s against 0.55 s, and 13 110 diff lines between two same-seed runs. | 114 `Instant::now()` and 44 `.elapsed()` sites in 26 files (perl, mechanical), plus one import per file. The `now_us` wall base is pinned (1 site). The seams' `t_us` (1 site). `.elapsed()` was not in the plan's count of 121; it is a second hidden std read. |
| 1b | tokio `select!` branch order | **yes** (not on the plan's list) | Without `rng_seed`: 12 diff lines. | `Builder::rng_seed`, which needs `--cfg tokio_unstable` build-wide. The alternative is `biased;` on all 9 `select!` sites, which is a behaviour change. |
| 2 | std threads / `spawn_blocking` | no | The `runtime_obs` sampler is off unless `RWM_RTOBS`; the Windows TUN is not on the perf path. | none |
| 3 | hash order | **yes, N ≥ 2 only** | On 2 paths, the first control datagrams went to different paths in 2 classes (14/11 of 25). | 4 sites: `scheduler` `paths` and `assignments`, `scheduler/rx` `paths` (BTreeMap under `sim` via `PathMap`), and `QuicTransport::start_readers` (sort the DashMap ids before spawning; unconditional on this branch). After the fix: 32/32 on N=2 and 10/10 on N=3. The other ~74 hash maps did not show up. |
| 3b | engine `rand::random` | **yes** | Without it: 16 diff lines. | 3 sites (`emit_source` 1, `scheduler/place` 2). `bbr_rs::from_entropy` is not on the default CC path and was not exercised. |
| 4 | TLS / connection-ID randomness | **yes for quinn's RNG, no for rustls** | Without quinn's `rng_seed`: 8633 diff lines (packet-number skips and connection-ID lengths change datagram sizes). rustls and ring randomness changes ciphertext only: no length or timing difference in 62 runs. Residual risk: an ECDSA DER signature varies by a byte. It was not seen, but a fixed key or Ed25519 under sim would close it. | 1 site (`EndpointConfig::rng_seed`). |
| 5 | quinn `work_limiter` | no | It takes `runtime.now()`, which is tokio's paused clock, so it follows virtual time. | none |
| – | tokio `test-util` reach | works | A `sim = ["tokio/test-util"]` feature unifies into the lib and into quinn. quinn's `TokioRuntime::now/new_timer` are tokio-time. | – |
| – | process-global statics | cost | Both peers share `now_us`'s base, gates, `set_side` and the seam statics. The seams' `dead` mode also swallows the server's data sends; the server sends only the perf ack, so this is harmless here. | Note it; run one scenario per process. |

## Lost tail at ρ < 1 (evicting seat)

Setup: Realtime, `window_reliable=false`, 6 MB, k = 11800, `RWM_TEST_TS_DROP=dead`, `RWM_TEST_TS_DEAD_MS=1500` (the engine-level seam, as in `lost_tail_longer_than_window_strands_k_evicting`), 2.5 % link loss, 30 s virtual.

- **Wedge (3/3 identical):** k is sent once (`dropped=1`). It gets 0 resends and 0 sheds, there is no arrival above k, and it is neither passed nor filled. `sweeps=298`. Every tail sweep to the end clamps out the gap `[11800,11800]` against the window `[11947,12146]` (`gap_clamped n=3734`). This is the signature the original test asserts, and it now holds over 30 s rather than 6 s.
- **Control (2/2 identical):** the same config with a clean link and no drop. k is sent once, the first arrival above k comes at 0.868 s, and k passes at 0.956 s.
- **Fidelity note.** With free CPU, the sender hands every source symbol to the transport by about 64 ms, and the wire drains for about 0.8 s after that, so k's "send" (the seam's arming point) happens long before k reaches the wire. The wedge mechanism is the same. Both runs also end in DNF, because the realtime seat with passed holes never completes the object; that is why the verdict reads per-seq events, not completion. A socket-level blackout (`SIM_SCEN=tail` without the seam) is wired up but was not used for the verdict.

## Cost to make this real

- The clock routing as above (about 160 call sites plus imports, mechanical). It can stay on std without `sim`; check release bytes with the `.text` hash.
- The `tokio_unstable` cfg for the seeded `select!` (a build-config decision), or `biased` everywhere.
- A sim-ordered `PathMap` for decisions over paths, plus a deterministic spawn order.
- The sim net (about 300 lines) and the harness (about 150).
- An oracle beyond "did it finish": the S06 seams already give the per-seq signal, and S4's counters would generalise it.
- Builds: the first cold build was 31 min at `-j1`; later incremental ones took 1–3 min.

## Open

- Linux was not run.
- Jitter, Gilbert–Elliott and Weibull links are not built; only a fixed delay with Bernoulli loss exists.
- The sim does not model kernel effects (GSO, receive buffer, pacing).
- `bbr_rs` was not exercised.
- Wider hash-order coverage (other seats, N > 3) is not tested.
