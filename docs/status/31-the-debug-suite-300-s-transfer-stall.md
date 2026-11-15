## 31. The debug-suite 300 s transfer stall — decoder re-delivery below the prune floor — stall-rate check — pre-registration

**Cause (from the d6bf4e7 suite's DNF tails).** In the 3 MB stall the server's
receiver task panicked: `rp-w-3 panicked at src/net/lat.rs:252 — [LAT]
delivery 3247 below the floor 3659` (a `debug_assert`). The receiver calls
`WindowDecoder::advance(prune_before)` with `prune_before = delivered_last −
2·recv_win_cap` (cap 200 here, so ≈ 3258 > 3247); `advance` prunes the
decoder's dedup state (`seen`, `recovered`, `delivered`) below it. A late
source copy of an already-delivered seq below that point then passes the
dedup as new, `ReorderBuffer::push` returns it (`seq < next_deliver_seq`),
and it is delivered a second time. Debug: the `[LAT]` assert kills the
receiver task, the client sees a silent peer (`np_act=0`, `paused=100%`,
`cum` frozen at 3659) until the 300 s run timeout. Release: a duplicate
delivery to the consumer. The 8 MB stall's tail has no server log
(`assert_clean_rate_executed` passes none); its client side is consistent
with the same cause, not proven.

**Fix.** Every `WindowDecoder` keeps its advance floor and never returns a
seq below it (the dedup for pruned seqs is the floor itself). No wire, law
or dial change. Red/green: a cross-decoder unit test (re-feed below the
floor after `advance` → nothing returned; nothing returned twice).

**Stall-rate check (tests, not a measurement — no verdict ships).** Debug
build on disk, both locks held, no cargo/rustc of others. Arms `BASE`
(d6bf4e7) and `FIX` (this branch), each its own fresh target dir. One
iteration = `cargo test -p raptorpath --test chi_reachability --test
sigma_candidates_reachability -- --test-threads=2`. N = 40 iterations per
arm, wall cap 2.5 h per arm (record the N reached). Count per arm: DNF
panics (`did not finish within the perf client's run timeout`), server
`below the floor` panics, other failures. Outcomes: `STALL-FIXED` if BASE
DNF ≥ 1 and FIX DNF = 0; `RATE-INCONCLUSIVE` if BASE DNF = 0 (the unit
red/green carries the proof); `NOT-FIXED` if any FIX DNF (its tails are
read before anything else). Then the full suites (`tools/l1/suites_run.sh`:
release, doc, wasm, debug, parsers) on FIX.

### §31 result — STALL-FIXED (coordinator)

- BASE d6bf4e7: 15 of 20 iterations stalled (606–607 s each), every one with the server's `[LAT] delivery … below the floor` panic.
- FIX 17824f7: 0 of 40 iterations stalled, 0 floor panics (8–11 s each).
- Full suites on 17824f7: release 1130/0, doc rc 0, wasm 35/0, debug 989/0, parsers rc 0, identity panics 0. The two "flaky" debug tests (§27, cleanup pass) were this bug.
- Release builds had no panic but silently re-delivered a pruned seq; the fix is a correctness fix there too.
- Open: a dead server receiver does not fail the client fast (each run waits its 300 s timeout); the `[LAT]` in-order assert is unaudited for evict-mode (ρ<1) debug tests with loss.
