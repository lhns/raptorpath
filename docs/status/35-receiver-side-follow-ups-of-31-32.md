## 35. Receiver-side follow-ups of §31/§32 — the `recv_alloc_bound` spread (diagnostic) and the test sessions — pre-registration

Five open items from §31 and §32 (dead-peer fail-fast, the `[LAT]`
in-order assert under ρ < 1, the legacy decoders' prune, the size of one
prune, the allocation gate's spread). Items 1–3 are code with red-first
unit and loopback tests; item 4 is a paper decision (§5.2); none of them
changes the default path's cost (item 1 adds no work to any per-message
path; item 2 is debug-only; item 3 touches only `RWM_UNIFIED=0`), so no
performance battery is planned. This section pre-registers the one VM
measurement (item 5's diagnostic) and the test sessions. Committed before
any VM contact. Nothing below is a result.

**Item 5 — why `recv_alloc_bound` spreads (diagnostic; ships nothing).**
§32 Amendment 1 read 23.0–23.5 allocations per source symbol on one tree
(the gate is 23.5). The gate counts process-wide allocations over a 2 MB
and a 12 MB in-process loopback on ONE runtime and divides the difference
by the extra source symbols (8 347).

- *Hypothesis H-leak.* `in_process::run` aborts only the perf server
  task; the engines both ends spawned keep running on the runtime. So the
  2 MB count's two engines are alive during the 12 MB count and allocate
  on their timers (the 1 ms loop wake, the 2 s report tick and pings) at
  some rate `r_idle`. That share, `r_idle · T_large / 8 347` per symbol,
  scales with the 12 MB run's wall time, not its symbols, and its
  run-to-run variance is `T_large`'s.
- *Alternative H-batch.* The spread is in-run: per-batch allocations
  (owner polls, `recv_many` drains, ack batches, quinn's packet handling)
  whose count per symbol depends on scheduling.
- *Instrument.* `tests/recv_alloc_spread_diag.rs` (`#[ignore]`d, asserts
  nothing): per pair, the per-symbol figure by the gate's method
  (`shared`: one runtime) with each count's wall time and the leftover
  engines' allocation rate in a 1 s idle window after each count, and by
  `fresh` (each count on its own runtime, shut down after it, so no engine
  outlives its count); plus the per-symbol difference per log2 size class.
- *Session (tests, not a battery).* Both locks for the whole session
  (`take_lock`), no other cargo/rustc; builds and tests have no I/O gate.
  Tree = this commit, `git archive` (LF), fresh target dir on disk,
  release. (1) `recv_alloc_spread_diag -- --ignored --nocapture` once with
  `RAS_PAIRS=6`; (2) `recv_alloc_bound` five times (reference). ≈ 40 min.
- *Readings and decision rule.* Per variant, the per-symbol range
  (max − min) and median over the 6 pairs; per `shared` pair the leak
  estimate `L = idle_after_small · t_large / 8 347`.
  - **H-leak SUPPORTED** iff `fresh`'s range ≤ ½ of `shared`'s range AND
    the median `L` ≥ ½ of `shared`'s range. Then the gate moves to the
    `fresh` method (no engine outlives its count) and its bound is
    re-derived for that statistic in an amendment, with calibration on
    main and on ce60079 (the gate's red tree; the file builds on both), so
    the R1 discrimination the bound exists for is kept. The number is not
    raised to absorb the spread.
  - **H-leak REFUTED** otherwise. The size classes whose per-symbol
    difference varies most across pairs name the in-run source; the fix
    (or an estimator such as min-of-k for one-sided noise) is decided in
    an amendment before any change to the gate.
- *What it does not show.* Process-wide counts: a class is a size, not a
  call site.

**Test sessions (not measurements).** On the VM under both locks, fresh
target dirs, `git archive` trees: the new tests of items 1–3 red at their
test commits and green at their fix commits (named in the result), then
the full suites (`tools/l1/suites_run.sh`: release, doc, wasm, debug,
parsers) on the branch head. A REAL failure is reported, not retried
away. VM dirs removed at the end; 0 `raptorpath`, 0 `rp-*` confirmed.

### 35. Item 5 diagnostic — result: H-leak REFUTED; the spread is the run's own speed

Session 2026-11-01 22:55–23:01Z, both locks, release, fresh target, tree
`08cfda9`. Ledger: the `RAS` lines below (VM dir removed).

| variant | per-symbol, 6 pairs | range | median |
|---|---|---|---|
| `shared` (the gate's method) | 23.327, 23.236, 23.061, 23.132, 23.419, 23.089 | 0.358 | 23.184 |
| `fresh` (one runtime per count) | 23.264, 23.121, 23.340, 23.013, 23.107, 23.014 | 0.327 | 23.114 |
| gate, 5 runs | 23.029, 23.288, 22.958, 22.974, 23.110 | 0.330 | 23.029 |

- Leftover engines idle at 20–36 allocations/s, so `L` = 0.001–0.002 per
  symbol, two orders below the 0.36 range; `fresh` is not tighter (0.327 vs
  0.358). **H-leak REFUTED** by both clauses.
- The figure tracks the 12 MB run's own wall time: over the 12 pairs,
  r = 0.87 between `t_large` (0.38–0.57 s) and the per-symbol figure,
  slope ≈ 1.6 per symbol per second, i.e. ≈ 13 000 allocations per extra
  second of an ACTIVE transfer (an idle engine makes ~25/s). So a part of
  the engine's allocations is per unit of active time (wakes, polls,
  batches), not per symbol; a slower run spends more of it per symbol.
  Size classes: the varying share sits mostly in 17–32 B (3.71–3.96 per
  symbol), with 257–512 B, 1–2 KiB and 4–8 KiB smaller; 8–16 B, 16 B,
  64 B and the 1 KiB payload class are flat to 0.01.
- The noise is one-sided in that sense: the fast runs form a floor
  (22.96–23.03 in 5 of 17 runs); slowness only adds.

### 35. Amendment 1 — the gate's estimator (before the change)

The gate keeps its bound (23.5, unchanged) and its method per count, and
changes its statistic from one pair to the **minimum over k = 5 pairs** in
one test process. Derivation: the figure is `a + (time-coupled
allocations)/symbols`, where the second term is ≥ 0 and grows with the
run's wall time; the minimum over pairs estimates `a`, the per-symbol cost
the gate exists to bound (the R1 set, worth 2.4), and a regression in `a`
moves every pair, so the minimum keeps the gate's sensitivity to it. k = 5
because a pair costs ≈ 1 s (the 12 MB run is 0.4–0.6 s) and 5 of the 17
runs above sat at the floor, so the chance that 5 pairs all miss it is
small; k is not tuned to a target false-alarm rate. Every pair's value is
printed, so the spread stays visible. Not taken: the `fresh` method (no
gain, measured), a time-regression correction (it would turn the bound
into a fitted model). Check: the gate five times in the test session (each
reports its 5 pairs and the minimum).

### 35. Items 1–4 — result (tests; ships nothing by itself)

*Item 1 — dead peer.* Cause: the per-path I/O owner (`transport/io_owner.rs`)
logged a terminal quinn `ConnectionError` from `read_datagram` and kept
serving; nothing told the engine, and `perf::client` did not watch its
engine task. After a SIGKILL'd server the client's connection does close
at quinn's idle timeout (30 s, quinn 0.11's default), but the run waited
its full timeout. Fix (`01f6841`, `e301bba`): the owner records the loss
on its `PathView` and wakes `QuicTransport::all_connections_lost`, which
resolves when every current path's connection is lost (one lost path
while another lives is not lost); `run_impl` ends with `peer lost: …` on
the dialing side; `perf::client` watches its engine. The idle timeout is
now explicit (`QUIC_IDLE_TIMEOUT`, unchanged value). Declared: the arm is
gated on the dialing side (`!is_server`), because a perf server must
outlive its client leaving (the harness stops it by signal and reads its
exit flush); a server whose peer is gone stays up, as before. Loss is
noticed by a reading owner only. Test `tests/dead_peer_fail_fast.rs`
(1 and 2 paths, SIGKILL after warm-up, bound `QUIC_IDLE_TIMEOUT +
REPORT_INTERVAL` = 32 s, non-zero exit, `peer lost` in the output): red at
`d6b3828` (client still running 90 s after the kill, both tests), green at
`81f0f8c`, `5ff8030`, `e301bba` (≈ 30 s each).

*Item 2 — `[LAT]` assert.* The receiver now declares a late fill (`seq <
next_deliver_seq()` before the push with a reorder buffer; `dseq <
next_expected` without one; never ρ or the hint), and debug builds check
exactly-once: an in-order delivery must be ≥ the floor; a late one must
fill a hole the frontier skipped, once (a hole record capped at
`PENDING_SPAN_MAX`). Found while building it: the hold-expiry deliveries
(`drain_expired`) are not offered to `[LAT]`, so its floor can lag the
buffer's frontier; a late fill at or above the lagging floor is accepted
once and a repeat fires; a duplicate of an expiry-delivered seq cannot be
seen by the gauge. 7 new `net::lat` tests red at `81f0f8c`, 24/24 green at
`5ff8030` and `e301bba`. Audit: no non-ignored debug test runs the evict
window with loss (every subprocess loopback passes `--window-reliable`;
`auto` resolves to reliable); the `#[ignore]`d L0 instruments
(`unified_l0`, `unified_stream_l0`, `taper_emission_l0`) are the ones that
can produce late fills. Not changed, reported: `Pending::insert` refuses
`seq < floor`, so a late fill's arrival record is lost and counted twice
in `over`.

*Item 3 — legacy decoders.* RLC, Generation and the reference decoder
prune by popping ordered sets from the front; `seen` is `SeqBits` for
sources plus an ordered repair set (the unified pattern); the generation
prune visits only anchors below the frontier and keeps an ordered set of
Solving slots for its live floor; RLC's per-row coefficient trim is
deleted (dead work: a row's columns all lie above its pivot). Source gate
`tests/legacy_decoders_prune_by_removal.rs` red at `5ff8030`, green at
`e301bba`; the contract tests in `fec::prune_contract_tests` (retained
keys ≥ floor, exactly-once, below-floor re-feed) green at both (guards).
Non-default path (`RWM_UNIFIED=0`): no battery.

*Item 4 — size of one prune.* Left as is; the reasoning is in the paper,
§5.2 "The size of one prune" (`0ef8f29`).

*Item 5 — the gate after Amendment 1* (`e301bba`, five runs, each the
minimum of 5 pairs): 22.815, 22.954, 23.009, 22.811, 22.995 (pairs
22.81–23.30). Bound 23.5 unchanged.

*Session hygiene.* A first red/green pass shared one target dir across the
archived trees; their commit-time mtimes made cargo reuse the first
tree's binaries, so those t2/t3 readings were void and were re-run with a
fresh target dir per tree (the numbers above).

*Not covered by a test, declared.* No non-ignored test drives the
receiver's `late` line itself; the composition test proves the formula
over the real `ReorderBuffer` only. The unordered branch (`dseq <
next_expected`) is reached only on the evict window without a reorder
buffer, where nothing is held, so `next_expected` is the gauge's floor;
the out-of-order reliable arm `continue`s before `[LAT]` and never reaches
it.

*Full suites on `e301bba`* (2026-11-02 01:56–02:43Z, both locks, fresh
target): release 1146/0 (52 ignored), doc rc 0, wasm 35/0, debug 1009/0
(51 ignored), parsers rc 0, identity panics 0. `SUITES OK`. END: 0
`raptorpath`, 0 `rp-*`; the VM dir is removed. Ledgers:
`docs/l1-raw/rx-follow/` (VM user scrubbed; the run-1 red/green era file
is kept and marked VOID).

*Commit names.* The SHAs in §35 are the tested trees before the rebase onto
`3939d4f` (no engine change on main in between). After it: `d6b3828` →
`7383494`, `01f6841` → `fb7783b`, `81f0f8c` → `3114907`, `8a00599` →
`0b3a094`, `5ff8030` → `b099242`, `567d26a` → `a7b08a8`, `e301bba` →
`33a4ed8`, `0ef8f29` → `6115c81`, `08cfda9` → `5e1b74b`.
