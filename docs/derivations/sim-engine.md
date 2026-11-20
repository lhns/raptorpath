# S8 phase 1: deterministic simulation of the engine (branch `sim-engine`)

Plan: `test-detection-plan.md` S8, ADR-0082 (Proposed). Feasibility: `sim-spike.md` (branch `sim-spike` ed8405fb). This is the mergeable rebuild.

## What it is

- `raptorpath/src/clock.rs`: the engine's one clock and one RNG. Every engine `Instant::now()` (84), std `.elapsed()` (40), `rand::random` (3), the OS-seeded `bbr_rs` `StdRng` (1), the `now_us` wall base (1) and quinn's endpoint RNG (1) go through it. Without `sim`, each function is `#[inline(always)]` and its body is the expression it replaced. Not routed: 3 `SystemTime` reads in `monitor/stats.rs` and `task_obs.rs` (diagnostic stamps).
- Under `sim` only: per-path maps are ordered (`clock::PathMap`, BTreeMap) and the reader spawn order is sorted (`clock::PathIds`, BTreeSet). Without `sim` they are the original HashMap and Vec.
- `raptorpath/src/sim.rs` (`--features sim`): in-memory network under quinn on tokio's paused clock. Per direction: rate, fixed delay, seeded jitter (uniform or normal, its own RNG stream, FIFO kept so it never reorders), drop-tail queue, seeded Bernoulli loss, optional blackout. Time resolution is 1 ms (tokio's timer wheel): jitter needs several ms to show as RTT variance.
- `raptorpath/src/test_seams.rs` (`--features test-seams`): the S06 per-seq seams, the canonical copy (see below).
- Tests: `tests/sim_determinism.rs`, `tests/sim_lost_tail.rs`, harness `tests/sim_support/mod.rs`. One scenario per child process (process-global statics).

Run:

    RUSTFLAGS="--cfg tokio_unstable" cargo test -p raptorpath --features sim,test-seams --test sim_determinism --test sim_lost_tail -- --nocapture

`tokio_unstable` (tokio's seeded `select!` order) is set only in the CI `sim` job's RUSTFLAGS, never in `.cargo/config.toml` or the release build. The sim tests refuse to compile without it, and their `[[test]]` entries need `sim` and `test-seams`, so a plain `cargo test` never builds them.

## Release-binary identity

No existing source line moves: call sites are rewritten on the same line, imports and module declarations are appended at the end of their files, and every cfg'd seam hook sits on an existing line. A moved line changes `.rodata` even when the code is the same: tracing callsite names carry `file:line` ("event raptorpath/src/net/receiver.rs:1538"), and panic locations carry line and column.

Gate: `.github/workflows/binid.yml` (workflow_dispatch). It builds the base twice as a control, then every engine commit since the base, and compares sha256 of the binary, `.text` and `.rodata`. Toolchain of the identity run: `rustc 1.100.0 (0ef1bc386 2026-11-09)`, LLVM 23.1.5, x86_64-unknown-linux-gnu, ubuntu-24.04. Build command, from the crate dir as `tools/l1/binid_run.sh` does:

    cd src/raptorpath && CARGO_TARGET_DIR="$GITHUB_WORKSPACE/tgt" cargo build --release --locked --bin raptorpath

| commit | binary | `.text` | `.rodata` |
|---|---|---|---|
| main 5977317c (twice, control) | 099e2dee | 5d46ade0 | f8350152 |
| d596e702 clock/RNG routing | same | same | same |
| c9cf08f2 seams on new lines | differs (76 B) | differs (8 B) | differs (23 B: tracing `file:line`) |
| 813befb0 seams folded onto existing lines | differs | differs (8 B) | same |
| a27453b5 sorted ids as a type, not a call | same | same | same |

Runs 44674646726, 44675108300, 44675848279, 44676399042. The last 8 `.text` bytes went away when `path_order(Vec)` in the async `start_readers` became the `PathIds` type; the ordered-map commit alone was never built (it did not compile), so that attribution is by elimination.

## Results (Linux CI)

- Same seed twice: identical socket trace and filtered engine log, 1 path and 2 paths; a different seed differs. 30 consecutive repeats of both, on CI, before the repeat step was removed.
- Lost tail at ρ < 1 (Realtime, `window_reliable=false`, 6 MB, k = 11800, seam `dead` 1.5 s, 2.5 % loss): KNOWN DEFECT `rec-evict-blocker-unresolved` reproduced. k is sent once (swallowed) and never resent, shed, filled or passed in 30 s virtual; the evicting NACK site clamps the gap 299 times (logged every 100 ms). The test asserts the stall exists; flip it when the defect is fixed.
- Control, the same lost tail at ρ = 1: k resent and filled, transfer complete at 12.2 s virtual.
- Jitter witness: recovery-clock sigma 4799 µs with 4 ms uniform jitter, 757 µs with jitter off (queueing and retransmits, not 0).
- Wall: under 0.3 s per child run on CI.

## What a seed fixes

Measured 2026-11-20 (Windows 11 host, `windows-latest` and `ubuntu-24.04` runners, rustc 1.95.0 and 1.100.0), comparing socket traces line by line:

- **Same seed, same port, same OS: identical**, including the 6 MB blackout and 30 MB 5 % scenarios of `sim_shed_reach`.
- **The server port was an input** until the link's loss and jitter streams were keyed by server index (`sim.rs`, `SimNet::send`); before, the first c>s datagram already arrived at a different virtual time. Now pinned by `sim_determinism::same_seed_two_ports_*`.
- **The OS is an input.** The traces agree for a while, then the same events at the same virtual millisecond run in a different task order (first seen as a 24-byte DATAGRAM frame coalesced into a different packet). Cause: tokio's `select!` start branch and its `watch` channel draw from one per-thread RNG, and `net::shutdown_signal` draws a different number of times per OS (unix: `select!` over `ctrl_c` and SIGTERM, two `watch` listeners; Windows: one `ctrl_c`), so every later draw is shifted. Running the Windows body on unix under `sim` (experiment only, not merged) made every compared trace and filtered log identical across the two OSes. Compiler version (1.95.0 vs 1.100.0) and machine speed changed nothing.
- **Not fixed by the seed, masked so far:** quinn-proto's connection IDs (`rand::rng()` in `cid_generator.rs`) and the per-run certificate (Handshake CRYPTO length 655 vs 656 bytes between two runs, absorbed by padding). Neither changed a trace in any run compared.

## Hash order in production

On two or more paths, which path the first control datagrams take depends on `HashMap`/`DashMap` iteration order (`sim-spike.md` blocker 3: 2 classes, 14/11 of 25 runs before the fix). The sim fixes that order only under `sim`. In production (no `sim`) it stays per-process random: the 2-path first-control-datagram path is hash-order dependent, and a test or measurement that depends on it is not repeatable on the real engine.

## Test seams: the canonical copy

`sim-engine`'s `test_seams.rs` is meant to replace the copies on `s06-delete` and `wedge-diag`. Against `s06-delete` bf20df24 it differs only in:
- `t_us` is `net::now_us()` (wall-anchored once per process, then monotonic, virtual under `sim`) instead of `SystemTime`. `tests/s06_reactive.rs` subtracts times across the two processes; both are wall-anchored, so the differences hold.
- It prints through `crate::readout!`. The `eprintln!` copy fails `readout::tests::no_eprintln_in_engine_source`, which walks every file under `src/`.

The function set and event formats are the same. `s06-delete` does not call `on_shed_slot`/`on_denied_slot` (it deletes the slot-site branch); they stay for trees that still have it.

## Phase 2

Gilbert–Elliott and Weibull links; a CPU or feed-rate model for senders without backpressure (free CPU makes the Realtime sender overdrive any link); porting flaky timing tests into the sim; the 3 unrouted `SystemTime` reads; a fixed key or Ed25519 under `sim` (ECDSA signature length is a residual risk, unseen in 60 Linux pairs); hash-order coverage at N > 2; sub-ms time.
