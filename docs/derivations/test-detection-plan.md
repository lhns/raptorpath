# Test detection plan: catching this week's defect classes

Status: plan, nothing built. Base: github/main `e304921b`. ADR drafts: 0080–0083 (Proposed).

This week's defects passed every test we have. This plan adds seven detectors, one per defect class, and orders them by what can land first. All seven are in scope. The order only says what goes first given one host build slot, and what waits on what.

## 0. Terms

- **Seat:** a point on the (δ, ρ) dials that a test or battery runs at. The **reliable seat** is ρ = 1: the sent-data store keeps every un-acked seq. The **evicting seat** is ρ < 1: only the 200-symbol coding window can serve a gap.
- **Merge guard:** the 12 + 12-row A/B run on the benchmark VM (status §41c). ADR-0076 requires it for any change to the release binary's bytes, together with the host ns bench. A change whose release `.text`/`.rodata` hashes are unchanged may be waived (owner question 9 in `shared-core-design.md`, still open).
- **Host slot:** the one cargo job the host allows at a time (lock dir, `CARGO_BUILD_JOBS=1`). Anything that runs on CI instead does not use it.
- **CI:** GitHub Actions on the public `github` remote. Nothing runs there today.

## 1. The defects

| id | defect | where |
|---|---|---|
| D1a | On the evicting seat the NACK site clamps every gap to the coding window before the shed check, so a gap below the window is neither served nor shed | `net/sender_phases.rs:790–805` |
| D1b | The source rate has no bound on the evicting seat: the intake pause is `reliable && …` | `net/mod.rs:2473` |
| D1c | The tail sweep re-targets `retransmit_buffer.first()` even when that seq is below the window, so the gap it builds is clamped away (D1a) and the sweep does nothing | `net/mod.rs:2789`, `:3035` |
| D1d | A lost tail with no later traffic is never resolved | follows from D1a + D1c; reproduced on `wedge-diag` `bc383f29` (`tests/s06_reactive.rs`) |
| D2 | A non-ignored timing test passed 4/10 on one tree and 1/10 on the other, and was recorded green on 1–2 samples | `s06_reactive.rs` T3 (`stall_budget_open`, `a91e23f1` on `s06-delete`) |
| D3 | Unwired code and ignored config: ADR-0050 Phase 3 `proactive_rate` and `set_protocol_hint` are called only from tests; `enable_pi_feedback` is passed and ignored (`_enable_pi_feedback`); the CLI overlay sets tunnel fields to `None` and `merge` takes `overlay.or(base)`; perf drops `--config` and the tail target; a third of the ledger's cells never execute | `fec_rate.rs:317`, `:832`; `scheduler/mod.rs:596`; `main.rs:378–395`; `config.rs:195–212`; `liveness-audit.md` |
| D4 | Pinned laws: the store cap sits at N·2048 at every δ, and the NACK budget at its floor of 10 on 70–97 % of ticks. Every test checked that the code computes the model. None checked that the law output moves | `law-ledger.md` F01, C09 |
| D5 | Zero readings with no witness: shed counters read 0 because the code never ran | `liveness-audit.md` S04 |
| D6a | main failed its own gate tests after a "tools only" merge dropped an engine half | fixed in `e304921b` |
| D6b | A stale binary from a shared `CARGO_TARGET_DIR` was used in comparisons without anyone noticing | host practice |

## 2. Premises checked against this repo

What the brief assumed, and what the tree says.

- **CI: none.** There is no `.github/`. The repo on `github` is public, so standard runners are free with no minute limit. They have 4 vCPU, 16 GB and 14 GB SSD, and a job may run up to 6 h (GitHub docs, checked 2026-11-20). The account allows 40 concurrent jobs (search result, not checked on the account).
- **Loopback tests need no root and no netns.** `tests/common/loopback.rs` binds `127.0.0.1:0` and spawns the real binary (`CARGO_BIN_EXE_raptorpath`). Loss and delay come from the in-process L0 netem shim (`transport/l0_netem.rs`). No test file uses `netns`, `sudo` or `/dev/net/tun`. So they run on a hosted runner. They will be noisier there, because the runner is shared and has 4 vCPU.
- **Suite size and time.** The release suite has about 1150 tests and the debug suite about 1000. On the VM they took 44–54 min, plus about 12 min of release build (`docs/l1-raw/build-timings`). `--lib gates` is 15 tests.
- **The flake rule today.** `tools/l1/suites_run.sh` `tst()` calls a failure that passes one immediate solo re-run `FLAKE` and returns 0. That rule is how a test that passes 40 % of the time can still count as green.
- **`cc_sim` is not on main.** It exists only in the `nsbench48` commits (`db2cb4f4` and earlier).
- **There is no transport trait.** The engine holds a concrete `Arc<QuicTransport>`. The seam below the transport is quinn's own `Runtime` (`new_timer`, `spawn`, `wrap_udp_socket`, `now`). The engine already wraps it once: `TimedRuntime` in `task_obs.rs:576–640`.
- **quinn follows tokio's clock.** quinn 0.11.9's tokio runtime builds timers with `sleep_until` and reads `tokio::time::Instant::now()`. The `Instant::now()` reads in quinn-proto 0.11.14 are all in `#[cfg(test)]` code. `work_limiter.rs` reads the std clock, but only to size work batches.
- **tokio's mock clock is already in the test build.** The dev-dependency `tokio-test` 0.4.5 enables tokio's `test-util` feature. Under resolver 2 that reaches test targets only, not the release binary. Whether the `CARGO_BIN_EXE` binary that the tests spawn also gets it is **unverified**.
- **The engine reads the clock directly.** Outside test files `raptorpath/src` has 121 `Instant::now()` reads, 44 of them in `net/receiver.rs`. It also has 72 `now_us()` and 5 `now_pair()` calls, which go through one function (`net/mod.rs:516`), 18 `tokio::time::Instant::now()` reads and 4 `SystemTime::now()` reads. The binary builds a multi-thread runtime (`runtime_obs.rs:65`), and tokio's paused time needs a current-thread runtime. `perf::server` and `perf::client` are `pub async fn`s (`perf.rs:107`, `:268`), so both peers can run in one process.
- **The `pub` surface.** `raptorpath/src` has 890 `pub` items and 341 `pub(crate)` ones, and all 15 top-level modules are `pub mod`. rustc's dead-code lint skips anything reachable from a `pub` path, which is why `proactive_rate` and `set_protocol_hint` never warned. Integration tests import from at least 25 module paths (`raptorpath::fec`, `::net`, `::scheduler` …), so narrowing has to keep those paths.
- **`raptorpath-core` is empty.** Its 14-line `lib.rs` exists only to hold the boundary. Its deny-list test forbids I/O, clocks and hint types.
- **Seam mechanism.** The `test-seams` cargo feature exists (`raptorpath/Cargo.toml:78`), is off by default and is never in the release binary. `wedge-diag` uses it for per-seq drop injection and the `[TSEAM]` trace.
- **`raptorpath-math`.** With `gf256` it has about 11.8k lines, 239 functions and 169 tests. It carries platform-specific bit-pin tables (`rstar_bitpin_table_{linux,windows}.in`), so a Linux CI run uses the Linux table.
- **Tools.** Kani documents Linux and macOS only, not Windows. stateright is at 0.31.0 on docs.rs. cargo-mutants (27.x) has `--shard k/n`, `--file`, `--re` and `--test-tool nextest`. cargo-llvm-cov has supported Windows since 0.1. `cargo-nextest` is already installed on the host. None of cargo-mutants, cargo-llvm-cov or Kani is installed there.

## 3. Which detector catches which defect

The seven detectors from the brief, keyed to the steps in §4.

| | D1a clamp | D1b rate | D1c sweep | D1d tail | D2 flaky | D3 unwired | D4 pinned | D5 zero | D6a gate | D6b stale bin |
|---|---|---|---|---|---|---|---|---|---|---|
| 1 CI + flake rule (S1) | no | no | no | no | **yes** | no | no | no | **yes** | no |
| 3a/b static audits, config round trip (S2) | no | no | no | no | no | **yes** (4 of 6) | no | no | no | no |
| 7 pinned-law check on logs + law-moves sweep (S3) | no | no | no | no | no | part (NR cells) | **yes** | part | no | no |
| 2 conservation + stuck counters (S4) | **yes** | part | **yes** | **yes** | no | no | no | **yes** | no | no |
| 3c per-seat coverage (S5) | part | no | part | no | no | part (NR cells) | no | **yes** | no | no |
| 6 mutation testing (S6) | no | no | no | no | no | part | part | no | no | no |
| 5a hand-written hole model + Kani (S7) | yes* | part* | yes* | yes* | no | no | no | no | no | no |
| 4 deterministic simulation (S8) | **yes** | **yes** | **yes** | **yes** | **yes** | no | part | part | no | no |
| 5b model check over the real decisions (S9) | **yes** | part | **yes** | **yes** | no | no | no | no | no | no |

`yes*`: only if the hand-written model copies the code's order (clamp, then shed). The model is checked; its match to the engine is assumed. Kani alone catches none of this week's defects (§4 S7).

No detector catches D6b. §6 has a small fix for it.

## 4. The steps, in order

Every step lists: what it catches and misses, what it needs, where it runs, its cost, whether it changes engine bytes (and so needs the merge guard), what the owner has to decide, its prerequisites, and whether it can run beside the current engine work (the wedge fix, the shared-core moves, the taper deletion).

### S1. CI on every push, and a flake rule that fails (detector 1)

- **What.** A GitHub Actions workflow with two tiers.
  - **Tier A, on every push to `main` and every PR** (about 25–35 min cold, estimated, less with a cargo cache):
    - `cargo test -p raptorpath --lib gates`, `-p raptorpath-math`, `-p raptorpath-core` (including its deny-list test);
    - `cargo test -p raptorpath --lib` in release;
    - the `tools/l1` Python and shell tests;
    - the roadmap `check.py`;
    - the two visualizer `.mjs` gates (needs Node and wasm-pack).
  - **Tier B, nightly and on demand:** the full `suites_run.sh` set (release, debug, seams, wasm, core), plus a repeat job (below). Estimated 1.5–2.5 h on one runner; it can be split by test binary over parallel jobs.
- **The flake rule, replaced.** A test failure that passes on re-run is a defect, not a pass. `tst()` keeps recording `FLAKE`, but the suite's exit code counts it as a failure unless the test is on `tests/quarantine.txt`, with an owner and a reason. A quarantined test is `#[ignore]`d with that reason.
- **Getting out of quarantine.** A timing test (one that waits on wall time or a deadline) becomes non-ignored only after N consecutive passes under the tier-B repeat job. One failure puts it back. Proposed N = 30. The arithmetic, P(pass N in a row) = p^N:
  - a 90 %-pass test clears 10 runs with probability 0.9^10 = 0.35, and 30 runs with 0.9^30 = 0.04;
  - the D2 test, at 40 %, clears 4 runs with 0.4^4 = 0.026 and 30 runs with about 1e-12;
  - so 30 clean runs show a failure rate ≤ 10 % at 95 % confidence. Showing ≤ 5 % takes 59 runs (0.95^59 = 0.05).
  - The owner picks N. A timing test that cannot reach it moves to the simulator (S8), where it is deterministic.
- **Catches.** D6a (main red on the push that broke it) and D2 (a 40 % test fails the repeat job at once).
- **Misses.** On its own, every engine logic defect. CI only carries the other detectors. On push it finds a broken main after the fact; a required PR check catches it before.
- **Needs.** `.github/workflows/ci.yml` and `nightly.yml`, a `tests/quarantine.txt` reader in `suites_run.sh`, and a small runner wrapper, because `suites_run.sh` takes the VM locks today. No code changes.
- **Runs on:** CI. **Host slot:** none. **Engine bytes:** none. **Guard:** no.
- **Risk.** Loopback timing tests on a shared 4-vCPU runner will fail more often than on the VM. That is the point of the repeat job, but tier A must hold only tests that pass under load, or the gate gets ignored. The first week's job is sorting tests into tier A or quarantine.
- **Owner.** ADR-0080. Is `github` the canonical remote (Actions sees only pushes there; `origin` is a separate gitea)? Push-only, or a required PR check? N? Who owns the quarantine list?
- **Prerequisites:** none. **Parallel:** yes, with everything.

### S2. Static audits and a config round trip (detector 3, parts a and b)

All three are `#[cfg(test)]` source-walk or resolution tests, in the style of `gates.rs` `forwarding_audit`. None changes engine bytes.

- **S2a. Production-caller audit.** Walk `raptorpath/src`. For every `pub`/`pub(crate)` `fn` defined outside test code, require at least one call outside `#[cfg(test)]`, test files and `tests/`; or an entry in an allowlist with a reason (trait impls, `main.rs` entry points, `test-seams` hooks).
  - Catches `proactive_rate` and `set_protocol_hint` today: their only callers are `control/fec_rate/tests.rs:629–650` and `scheduler/tests.rs:585`.
  - It is text-based, so method-name collisions give false passes. It reports a candidate list and does not prove anything.
- **S2b. Ignored-parameter audit.** Flag every parameter named `_x` in a non-test function whose caller passes a non-constant value. rustc never warns on `_`-prefixed names, which is why `_enable_pi_feedback` (`fec_rate.rs:317`) stayed silent. An allowlist with a reason covers trait signatures.
- **S2c. Config round trip, per entry point.** For each key in `Config`, resolve a TOML that sets only that key to a non-default value through each entry point: tunnel `run`, perf client, perf server. Each must change the resolved `PeerConfig` or a named consumer field. A key that reaches none is either marked "not read by this entry point" in a table the test owns, or it fails.
  - Catches: the CLI overlay fields set to `None` (`main.rs:378–395`) shadowing a file value, and perf dropping `--config` and the tail target.
  - It catches `enable_pi_feedback` only if the assertion sits at the consumer (the controller), not at `PeerConfig`, because the value reaches `PeerConfig` and dies one call later. So the test asserts one level past the struct for every key the controller takes.
  - It needs the entry points' resolution to be callable without starting the engine. If `main.rs` builds `PeerConfig` inline, that code moves into a lib function first: a pure refactor with the same values.
- **S2d. The NR third of the ledger** is not caught here. It needs S3 (logs) or S5 (coverage).
- **Catches:** D3 (4 of the 6 named items). **Misses:** D3's never-executed ledger cells, and everything else.
- **Runs on:** host once to write the tests (one incremental `--lib` test build in the host slot), then CI tier A. **Engine bytes:** none (`cfg(test)`). The S2c refactor in `main.rs`, if needed, changes the binary's code layout: run the `.text`/`.rodata` hash check, and if the hashes differ it rides the next guard batch.
- **Owner:** no ADR. The allowlists are reviewed like code.
- **Prerequisites:** none (S1 to keep it running). **Parallel:** yes. It touches only test modules and, for S2c, `main.rs`/`config.rs`, which no current engine work touches.

### S3. Pinned-law check from committed logs, and a law-moves sweep (detector 7)

- **What, part 1: the check over committed logs (Python only, no cargo).** `docs/derivations/liveness-audit/` already turns the committed `[DIAG]`/`[SPAN]`/`[SUMCAP]`/`[QCLK]` lines into per-tick shares (`live.py`, `plib.py`, `count.py`).
  - Make it a test in `tools/l1` that recomputes the classification over `docs/l1-raw` and fails when a law cell reads PIN or NR (as defined in `liveness-audit.md`) and the ledger has no verdict for that row and seat.
  - It also fails on a counter that reads 0 at a seat with no execution witness in the same log (MEASUREMENT DISCIPLINE 1).
  - Each new battery's logs then go through the same check when they are committed.
- **What, part 2: true replay.** This works only for laws whose inputs are logged. Most `[DIAG]` fields are outputs (`nbud`, `win`, `wsize`), not inputs. The store cap's inputs (b̂, m̂, N, δ) and the NACK budget's input (`current_loss`) are not all printed.
  - So: for each ledger row, list whether its inputs are logged.
  - Where they are, feed the logged inputs to the law function (a `raptorpath-math` function, or an engine function via a small test binary) and fail if the output's range over the logged operating range is one value.
  - Where they are not, the part-1 check over output shares stands in, and the row gets an "inputs not logged" note, which is an NG cell in the audit's terms.
- **What, part 3: law-moves sweep (MEASUREMENT DISCIPLINE 17, made a gate).** For each law the ledger lists, a property test sweeps each dial over the operating range read from the logs: δ over the three presets ± 2 %, N = 1…8, ε from the logged range.
  - It asserts the unclamped output is not constant, and it reports the clamp's bind fraction.
  - The rule-17 tests that exist today run at N ∈ {1, 2} and synthetic ranges. This step puts the logged range into them.
  - The store cap at N·2048 for every δ fails here: its δ-derivative is 0 at every point the cells reach.
- **Catches.** D4 (store cap, NACK budget) and D5 (a zero without a witness). Part of D3's NR cells, read from data.
- **Misses.** It only sees seats that a battery ran: the Bulk-only blind spot, if the logs are Bulk-only. And it only sees laws with a gauge (72 NG cells today).
- **Runs on:** CI tier A for parts 1 and 2 (Python, seconds to minutes over 384 MB of logs; keep the parsed summary committed so CI does not unpack every archive), and the host for writing part 3 (a `--lib` test build in the host slot). **Engine bytes:** none.
- **Owner.** The threshold: ledger `pinned_threshold` is ≥ 0.5 of the range; the audit uses ≥ 90 % of ticks. Which one fails the test? No ADR: this applies rules 1, 17 and 18 as they already stand.
- **Prerequisites:** none. **Parallel:** yes. Part 3 adds tests to `net::tests::law_shape` and the math crate. Coordinate with the shared-core moves, which re-home those laws, so each test is written once against the post-move path.

### S4. Conservation and stuck counters (detector 2)

- **The identity.** One line per sender, at every DIAG tick and at the `final=1` exit flush:

  ```
  S_sent = A + H + O_serv + O_strand
  ```

  - `S_sent` source seqs sent;
  - `A` acked (cumulative or SACK-released);
  - `H` shed;
  - `O_serv` outstanding seqs that the gap server would serve;
  - `O_strand` outstanding seqs that the gap server's own clamp excludes, that is, below `clamped_start` as `serve_gaps` computes it.

  On the reliable seat the clamp is the identity, so `O_strand` = 0 by construction. The counter reads the same bound the server uses and adds no seat branch (NO-MODE-SWITCH): it is one formula on both seats, and its second term is zero at ρ = 1.
- **Stuck witnesses.**
  - `inert_sweeps`: tail-sweep fires whose gap produced zero sends.
  - `oldest_unchanged`: how many sweeps the oldest un-acked seq has gone unchanged while sweeps fire.
  - `O_hwm`: the high-water mark of outstanding seqs, against the effective store cap.
- **Execution witnesses.** Each counter comes with a site-visit count: shed-check visits next to `H`, clamp evaluations next to `O_strand`. A reading of 0 then shows whether the code ran.
- **The test-side check.** One assertion in `tests/common/loopback.rs`, called by the helpers every loopback test already uses (`assert_no_dnf`, `transfer`, `lossy_run`). At the end of a completed transfer:
  - the identity closes;
  - `O_strand` = 0;
  - `O` = 0;
  - `inert_sweeps` = 0.

  Unit tests in `net/tests.rs` cover the counter arithmetic.
- **Catches.**
  - D1a: `O_strand` > 0.
  - D1c: `inert_sweeps` > 0.
  - D1d: `O` > 0 at the end with `oldest_unchanged` growing.
  - D5: visit counts.
  - D1b partly: `O_hwm` above the store cap shows unbounded intake, but only if a test runs the evicting seat long enough.
- **Needs a test that runs the seat.** Today's loopback tests are mostly ρ = 1. The S06 tests on `wedge-diag` (per-seq drop of the tail seq through `test-seams`) are the first that reach D1d. The counters turn their prose verdicts into assertions.
- **Misses.** D2, D3, D4 and D6. Also any stuck state on a seat no test runs.
- **Three ways to ship it; the owner picks one.**
  - **(A) In the release binary:** an end-of-run line plus DIAG fields. The L1 batteries then carry the witness too, which is where the zero readings happened. It changes engine bytes, so it needs the merge guard and the host ns bench.
    - Hot-path cost: one integer increment at sites that already increment `shed_total`, and an O(1) check per DIAG tick, not per symbol.
    - `O_strand` needs a count of outstanding seqs below a bound. `retransmit_buffer` is an ordered map, so that is a range count on the DIAG tick only, at O(log n), behind `pol.diag_on` like the other DIAG work.
  - **(B) Under `#[cfg(debug_assertions)]`.** The release bytes are expected unchanged; prove it with the `.text`/`.rodata` hash, then no guard. The debug suite (about 1000 tests, whose child processes run the debug binary) carries it. The release suite and L1 do not.
  - **(C) Under `test-seams`.** The same, but only the seams runs carry it.
  - **Recommended:** (B) now, so it lands without a guard on the next debug suite, and (A) on the next scheduled guard batch.
- **Owner question on semantics.** Is a seq that leaves the coding window on the evicting seat without a verdict a legal outcome? If yes, it needs its own term (`E` evicted) and the retention contract has to bound it by 1 − ρ. If no, `O_strand` > 0 is always a defect. The plan assumes no, because ADR-0064's shed law is the only sanctioned loss.
- **Runs on:** host for building, then CI tier B (debug suite). **Engine bytes:** none for (B), proven by the hash check; yes for (A), with the guard. ADR-0081.
- **Prerequisites:** none to land (B). For red-first value it pairs with the wedge fix: land the counters on `wedge-diag`'s tests first (red on main), and the fix turns them green. **Parallel:** it touches `net/mod.rs`, `sender_phases.rs`, `emit_source.rs` and `diag.rs`, the same files as the wedge fix and the taper deletion (ADR-0079). It runs in sequence with those, in the same chunk as the wedge fix.

### S5. Coverage per seat (detector 3, part c)

- **What.** `cargo llvm-cov` over the loopback and unit tests on CI, one run per seat group: B/A/R at ρ = 1, the evicting seat, N = 1 and N = 2. The result is a table, line ranges per ledger row against the seat, committed next to the liveness audit: the code-level twin of its NR column.
  - A ledger row whose decision lines are never hit at a seat a test claims to cover fails.
  - The table also shows which tests run the evicting seat at all.
- **Catches.** D5 (the shed branch never ran in the test that read 0), part of D3's never-executed cells, and D1a/D1c partly (the clamp `continue` and the sweep's clamped branch light up; that they do nothing is S4's job).
- **Misses.** Anything that runs but is wrong. Coverage shows execution, not effect.
- **Needs.** The perf server must write its profile on exit. `PerfServer::stop_with` sends a signal on unix and waits for a clean exit, so the profile is written (expected; unverified per test). On non-unix it kills the child, so the profile is lost: Linux only. A mapping from ledger rows to line ranges, kept in `law-ledger.yaml` `loc` (it already holds `file:line`).
- **Runs on:** CI tier B (an instrumented build is about 1.5–2× the release build, estimated). **Host slot:** none. **Engine bytes:** none. **Owner:** no ADR.
- **Prerequisites:** S1. **Parallel:** yes.

### S6. Mutation testing (detector 6)

- **What.** `cargo mutants` on `raptorpath-math` and `raptorpath-core` (empty today), sharded on CI. Later, also on the engine law files only, with `--file`: `net/store_cap.rs`, `net/recovery_laws.rs`, `net/shed.rs`, `control/fec_rate.rs`.
  - A surviving mutant inside a law means no test pins that part.
  - A surviving mutant inside a clamped expression means the clamp binds in every test: the D4 class, seen from the test side.
- **Cost estimate.** 239 functions at about 4–6 mutants each, so roughly 1000–1500 mutants; `cargo mutants --list` gives the real count without a build. Each needs an incremental rebuild of the math crate and its test binaries, then the tests. The math tests include Monte Carlo validations; their time is not measured.
  - At an assumed 30–90 s per mutant on a 4-vCPU runner: 8–35 runner-hours.
  - Over 16 shards: 0.5–2.5 h of wall time per weekly run.
  - The engine law files cost minutes per mutant (each rebuilds the `raptorpath` lib tests), so they run monthly or on demand, with `--re` limited to the law functions.
  - On the host this is not feasible inside one cargo slot.
- **Catches.** D4 partly (a clamp that always binds, in tests) and D3 partly (a mutant in an unwired function survives, because nothing calls it).
- **Misses.** D1, D2, D5 and D6: mutation measures the tests, not the protocol.
- **Risk.** Floating-point laws produce equivalent mutants (`<` against `<=` at a measure-zero boundary). Triage costs reviewer time, so keep a committed `mutants.out` baseline and fail only on new survivors.
- **Runs on:** CI (weekly). **Engine bytes:** none. **Owner:** no ADR (a CI tool, no `Cargo.toml` dependency).
- **Prerequisites:** S1; S3 part 3, so the law-moves tests exist to kill the clamp mutants. **Parallel:** yes.

### S7. A hand-written model of the hole lifecycle, plus Kani (detector 5, part a)

- **What, the model.** A stateright model at tiny bounds of every seq's state: `sent → {acked, shed, outstanding}` over a lossy link.
  - Bounds: window W = 3, 8 source packets, every loss pattern, a sender that keeps sending or stops, a bounded number of timer fires.
  - It copies the shipped order: gap report → clamp to the window (evicting seat) → shed check → serve; the tail sweep on `first()`; the intake pause on the reliable seat only.
- **The property.** At every terminal state, each seq is acked, filled, skipped by decode, or shed, and `O` ≤ store cap.
  - The time steps are bounded so that every run ends. Then this is a safety check on terminal states, not a liveness check. That matters because a sweep that fires forever without effect is a cycle, not a deadlock, and whether stateright's `eventually` sees cycles is **unverified**. The docs say a counterexample runs "through to a terminal state".
  - This should find D1a, D1c and D1d in seconds (expected, not run): seq 0 lost, 4 more sent, window moves past 0, and nothing ever judges seq 0.
- **Honest limit.** The model is checked; its match to the engine is assumed. It repeats this project's known failure (the model ≠ the engine) unless S9 replaces it. It is worth doing first because it is cheap and it checks the protocol design: it would have shown that the clamp-before-shed order cannot be live on the evicting seat, whatever the code does.
- **TLA+ or stateright.** TLC checks liveness with fairness properly, but it needs Java and a second language, and it cannot call Rust. Stateright runs under `cargo test`, and S9 can make the same model call the real core functions. So stateright, with the bounded-time trick above.
- **Kani, honestly.** Kani is Linux/macOS only, so it runs on CI. Its targets are the pure integer functions: seq-wrap arithmetic (`net/seq_bits.rs`, `seq_ring.rs`), the window clamp and span bounds, and `SeqRing` indexing.
  - It verifies: no panic, no overflow, and stated bounds such as `clamped_start ≥ win_start`, for every input of the given width.
  - It assumes: loop bounds (unwind limits) and that the floating-point laws stay out of scope (Kani's float support is slow, and those laws are covered by S3 and S6).
  - It would have caught **none** of this week's defects, which are protocol and wiring defects, not arithmetic ones. It is a guard for a later wrap bug.
- **Needs.** A new workspace member `raptorpath-model` (`publish = false`) with a `stateright = "0.31"` dependency. It sits outside `raptorpath-core`, whose boundary allows only `raptorpath-math`. It must be added to `src_manifest` in `tools/l1/loopdiag_run_all.sh` and to the harness path tests, as `shared-core-design.md` step 0.1 did for core. Kani harnesses go under `#[cfg(kani)]` in the crates they check.
- **Runs on:** host (a small crate, one short build) and CI tier A for the model; CI weekly for Kani. **Engine bytes:** none. **Owner:** ADR-0083 (a new dependency).
- **Prerequisites:** none. **Parallel:** yes.

### S8. Deterministic simulation of the recovery path (detector 4)

- **The layer: under quinn, at the socket.** There is no transport trait to cut below, and the L0 shim sits above quinn, so quinn's own CC sees a clean link. Both peers run in one process on a current-thread tokio runtime with paused time, through `perf::server`/`perf::client`. quinn gets an in-memory `AsyncUdpSocket` through a `quinn::Runtime` wrapper, the pattern `TimedRuntime` already uses. The link model (rate, delay, jitter, GE, Weibull bursts) is the L0 shim's, moved under the socket and seeded. Time advances only when every task is idle, so a 30 s tail-loss scenario runs in milliseconds and gives the same event order for the same seed.
- **Blockers, all found in this tree:**
  1. **121 direct `Instant::now()` reads** in engine code, plus the `now_us()` base. tokio's paused clock does not move `std::time::Instant`, so each read goes through one engine clock function that returns `tokio::time::Instant::now().into_std()`.
     - Without tokio's `test-util` feature, that call is `std::time::Instant::now()` (tokio 1.50 `time/instant.rs`, `variant::now`), so the release behaviour is the same and the bytes may be too.
     - The 4 `SystemTime` reads (the `now_us` epoch base, the trace stamps) get the same treatment.
     - It is a mechanical change, but it touches most of `net/`.
  2. **The runtime.** The binary builds a multi-thread runtime (`runtime_obs.rs:65`). The simulator does not use `main`; it calls the lib entry points on its own current-thread runtime. Anything that spawns a std thread or `spawn_blocking` on the perf path breaks paused time. Found so far: the `runtime_obs` sampler thread, which is diagnostics and can stay off in the sim, and the Windows TUN, which is not on the perf path.
  3. **Hash order.** `HashMap` and `DashMap` iteration order is randomized per process. Any decision that iterates a map gives a different event order on each run. The simulator needs a fixed hasher under a sim build, or proof that no decision depends on that order. Unknown until the first double run diffs its traces.
  4. **TLS and connection-ID randomness** in quinn and rustls changes packet bytes, not timing. Expected harmless; check it with the same double-run diff.
  5. **quinn's `work_limiter`** reads the std clock to size work batches. It may change batch boundaries between runs. Check it with the same diff.
- **Oracle.** The S4 counters (`O_strand`, `inert_sweeps`, the identity) and the S06 per-seq trace. Without S4 the simulator can only say "the transfer did not finish".
- **Catches.**
  - D1a–d, with D1b seen as `O_hwm` growing while time is virtual and free.
  - D2: timing tests move here and stop being flaky, because the outcome no longer depends on host load.
  - D4 partly: δ and N sweeps of the real engine are cheap, so a law output that never moves over a δ sweep shows up.
- **Misses.** D3 and D6. Anything that depends on the real kernel stack (GSO, the receive buffer, real pacing). Fidelity is bounded the way the L0 note says.
- **Cost.** The clock change is about 200 call sites, mechanical. The sim socket and runtime wrapper are about 300–500 lines. The in-process two-peer harness is about 200 lines. The determinism shakedown is open-ended (blocker 3). Several host builds in the slot. Once built, a scenario runs in seconds on CI.
- **Engine bytes.** The clock change touches release code. Proof ladder: `.text`/`.rodata` hashes, then the decision pins, then the ns bench. If the hashes are unchanged, the guard can be waived under owner question 9; otherwise it rides a guard batch. The sim socket and harness are test-only (`test-seams` or `cfg(test)`). ADR-0082.
- **Prerequisites:** S4 for the oracle; the wedge fix in, so `net/mod.rs` is quiet; ideally after shared-core batch α, so moved code is converted once. **Parallel:** the clock change conflicts with every engine edit in `net/`. Land it as one chunk at a quiet point, between engine batches. The sim socket and harness can be written in parallel, against a branch.

### S9. Model check over the real decisions (detector 5, part b)

- **What.** S7's model, with the hand-written steps replaced by calls into `raptorpath-core`: the gap clamp, the shed check, the channel verdicts, the tail-sweep target choice and the intake pause. The model then checks the code's own decisions, and the "match assumed" caveat goes away for those steps.
- **The prerequisite that makes or breaks it.** The shared-core plan does not move the parts that hold this week's defects:
  - D9's row keeps "the channel selection (N split, `recov_sp`, `reliable` arms)", including the clamp, in the engine "until owner question 1";
  - the tail-sweep target (`net/mod.rs:2789`, `:3035`) and the intake pause (`net/mod.rs:2473`) sit in the run loop and are in no step.

  So a checker over today's planned core functions would never run the D1 code. **The extraction scope has to grow**: clamp, sweep target and intake pause become core functions taking the window bounds and the store state as arguments. That is an engine-side move (zero behaviour change, the proof ladder, a guard batch), and it needs owner question 1. The same extraction is what lets S8 test decisions without the full engine, so it is one shared prerequisite.
- **Class A.** These functions take the window bounds and ρ as numbers. `reliable` becomes a bound (the clamp floor at ρ = 1 is the store floor), not a bool, which also removes a mode bit (NO-MODE-SWITCH). Whether that change keeps behaviour identical needs its own pin. It is a design question for the owner, not a refactor.
- **Catches:** D1a, D1c and D1d with no "assumed" caveat, and D1b as far as the intake pause moves.
- **Misses:** everything outside recovery.
- **Runs on:** host and CI tier A. **Engine bytes:** yes (the extraction), so the guard. **Owner:** ADR-0083 (scope), plus shared-core owner question 1.
- **Prerequisites:** S7; shared-core steps 2.1, 2.5 and 3.1; the extended extraction above. **Parallel:** it runs in sequence with the shared-core moves, as part of them.

### S10. Narrow `pub` to `pub(crate)`, so rustc finds dead code (detector 3, the rest of part a)

- **What.**
  - Change `lib.rs` to `pub(crate) mod` for each module that no integration test or `main.rs` imports.
  - For the rest, narrow the items to `pub(crate)` one by one, and re-export the test-used ones through one `#[doc(hidden)] pub mod testing` path.
  - Then build the lib without `cfg(test)` under `-D dead_code`.
  - S2a's text check stays as the cheap first pass; this one is exact.
- **Cost.** 890 items, mechanical, but it touches every module. It conflicts with every open engine branch (rebase churn), so it is done at a quiet point.
- **Engine bytes:** expected unchanged (visibility does not change codegen under LTO); prove it with the hash check.
- **Owner:** when to do it. No ADR: there is no outside consumer of the lib API.
- **Prerequisites:** S2a (its allowlist seeds this). **Parallel:** no. It is a quiet-point step, in the same window as S8's clock change.

## 5. Order and parallelism

| order | step | host slot | engine bytes / guard | waits on | runs beside engine work |
|---|---|---|---|---|---|
| 1 | S1 CI + flake rule | none | no | — | yes |
| 2 | S2 static audits + config round trip | 1 test build | no (S2c: hash check) | — | yes |
| 3 | S3 pinned-law check + law-moves sweep | part 3 only | no | — | yes (part 3 written against the post-move paths) |
| 4 | S7 hand-written model + Kani | 1 small build | no | — | yes |
| 5 | S4 counters, option B | debug builds | no (hash check) | pairs with the wedge fix | no: same files as the wedge fix and ADR-0079; same chunk |
| 6 | S5 coverage per seat | none | no | S1 | yes |
| 7 | S6 mutation testing | none | no | S1, S3 part 3 | yes |
| 8 | S4 counters, option A | release build | **guard** (next batch) | S4 option B | rides a guard batch |
| 9 | S8 deterministic simulation | several | clock change: hash check, else guard | S4, the wedge fix, ideally shared-core batch α | the socket and harness yes; the clock change at a quiet point |
| 10 | S9 model check over the real decisions | yes | **guard** (extraction) | S7, shared-core 2.1/2.5/3.1, extended extraction, owner question 1 | in sequence, part of the shared-core moves |
| 11 | S10 `pub` narrowing | 1–2 builds | no (hash check) | S2a | quiet point (with S8's clock change) |

S1–S3 and S7 can start at once, in parallel, as four independent workers. Only S2 and S7 need the host slot, briefly. S5 and S6 follow as soon as S1 runs. S4 goes in with the wedge fix.

## 6. What none of these catch

- **Rate over-provisioning and estimator over-read.** These are measurement questions: is r too high for the loss the link really has; does ε̂ read above the truth? They need ground truth: the `[TRUTH] loss=` egress counter (MEASUREMENT DISCIPLINE 19) against the estimator, on L1 cells. No unit, model or simulation test can say what the real link's loss is. S3 can check that ε̂ and `[TRUTH]` are both logged and compared when a battery reports a rate verdict. The verdict itself stays an L1 battery's.
- **D6b, a stale binary from a shared `CARGO_TARGET_DIR`.** None of the seven. Small fix, outside the seven: every comparison script records the binary's `sha256` next to `git rev-parse HEAD` at build time (`tools/l1/binid_run.sh` does this on the VM), and refuses a binary older than its tree's last engine commit. On the host the rule stays "one `CARGO_TARGET_DIR` per worktree". CI builds fresh by construction.
- **Defects on a seat no test runs.** Each detector sees only what its tests drive. S5's table is how that gap shows up.
- **A wrong law that moves.** S3 and S6 catch a law that does not move. Whether it moves correctly is FORMULA-FIRST's paper derivation, read by a reviewer.

## 7. Owner decisions

1. CI: is `github` canonical? Push-only or a required PR check? (ADR-0080)
2. The flake rule: N (proposed 30) and who owns the quarantine list. (ADR-0080)
3. Pinned-law threshold for the S3 test: the ledger's 0.5 of range, or the audit's 90 % of ticks.
4. Counters: ship option B now and A on the next guard batch, or only one of them? (ADR-0081)
5. Is a seq evicted without a verdict on the evicting seat legal? If yes, the identity gets an `E` term bounded by 1 − ρ. (ADR-0081)
6. Simulation: accept the engine clock function across `net/`, and when (quiet point). (ADR-0082)
7. Model check: the extended core extraction (clamp, sweep target, intake pause), and `reliable` as a clamp bound instead of a bool: shared-core owner question 1. (ADR-0083)
8. The new dependency `stateright` in a separate `raptorpath-model` crate. (ADR-0083)
9. When to schedule the `pub` narrowing.

## 8. Not verified

- The CI timings (tier A 25–35 min, tier B 1.5–2.5 h) and the instrumented-build factor are estimates from the VM's suite times, not runs.
- The account's 40 concurrent jobs came from a search result for the Pro plan, not from the account.
- Whether the `CARGO_BIN_EXE` binary built for integration tests gets tokio's `test-util` through `tokio-test`.
- That the perf server writes an llvm-cov profile on its signal exit.
- The mutant count and per-mutant time (run `cargo mutants --list` on CI first).
- How stateright's `eventually` treats cycles. The plan avoids depending on it with bounded time.
- That S7's model finds D1a/c/d at W = 3 and 8 packets. The trace is argued, not run.
- That the clock change leaves the release `.text` unchanged.
- Whether simulation determinism survives hash-map order, TLS randomness and quinn's work limiter.
