# ADR-0069: Block mode is LEGACY — the last mode bit, deprecated with a re-test clause; the default does NOT flip without measurement

## Status: Accepted (2026-08-10) — deprecation recorded; the shipped default is UNCHANGED and now PINNED by a test; the flip and the deletion are both gated on the pre-registered Block Default Re-Test below

**Date**: 2026-08-10

## Context

### The traced position (not assumed — read)

The transport has two pipelines and one bit selects between them:

```
net/mod.rs:789-790
fn is_window_mode(hint, backend, window_reliable) -> bool {
    (hint == ProtocolHint::Realtime || window_reliable) && backend.is_streaming()
}
```

`is_streaming()` is `matches!(self, Self::Rlc)` (`fec/traits.rs:113-114`). The
shipped default resolves to `window_reliable = false` (`config.rs:328`), hint
`auto` (`config.rs:237-241`), codec `RaptorQ` (`config.rs:286`, also
`FecBackend::default()`, `fec/traits.rs:101-104`). RaptorQ is block-only, so
`is_window_mode` is FALSE and `run_impl` falls through to `run_block_sender`
(`net/mod.rs:1862`). **The shipped default for Bulk/Auto is block mode, and the
`false` is not vestigial — it is load-bearing at every entry point:**

- CLI/TOML tunnel (`run`): `main.rs:288` sets `Some(true)` only under
  `--window-reliable`; unset ⇒ block.
- `perf` / native-object (`main.rs:387`): the same opt-in. `rp perf` without the
  flag is block mode too.
- Library API: `PeerConfig` has NO `Default` impl (`net/mod.rs:52-53`); every
  embedder sets the field explicitly, so there is no third path that quietly
  reaches window mode.
- Profiles: neither `Home` nor `Datacenter` sets it (`config.rs:141-152`).

One correction to the loose phrasing "the transport ships block mode": **Realtime
already rides the window pipeline at the default** — it auto-selects the RLC span
machine (`net/mod.rs:1390`, §16.20) — but with the lossy EVICT retention, ρ < 1,
not the reliable window. The block default is a **Bulk/Auto fact only**.

There is also a soft edge: setting `window_reliable` with a non-streaming backend
does not error, it warns and falls back to block (`net/mod.rs:1429-1435`).

### The contradiction

ADR-0067 is the standing rule: *the shipped default IS the best-measured
configuration*, because "users of the default were running the condemned arms"
is a scandal this project already had once. Against that rule:

- **Last battery in which block mode was the arm under test: 2026-07-08**
  ("Full Benchmark Re-Run", goal-gate.md:7970), cells C1–C5 via
  `tools/l1/perf_native.sh`. Headline: block is recovery-bound and unmoved by the
  CPU-era fixes (C2 0.86 s, C3 9.9 s) and **C4 DNF 6/6 at 600 s — "Flagged"**
  (goal-gate.md:7997-8010). That flag has never been discharged.
- **Last time block-mode code executed in ANY L1 run: 2026-07-27**, the crown
  battery's bulk sanity spot (cell S, goal-gate.md:14197/14311) — an
  explicitly INERT tail-latency spot, not a pipeline measurement; its A/B partner
  arm was retired the next day (`tools/l1/tail_matrix.sh:228-230`).
- **Every battery from 2026-07-12 to 2026-08-10** — DAPS, Competitive Baseline,
  Copa-Sole, C8 pool, Lossy-Single, Adversarial B1, Window Decoupling, C8
  conversion, Ship The Wins 1/1b/2, Unlock The Default 1/2, Ack-Merge Flip,
  Store-Cap, Three-Term Law — runs through `perf_rwm_c.sh`, which hardcodes
  `--window-reliable` (`tools/l1/perf_rwm_c.sh:112,128`). **No driver in the tree
  runs a block control arm.** The 2026-07-08 datum was measured with W1 (quinn
  Cubic), W2 (MTU black-hole wedge), W7 (1024 pool law), W8 (global recovery
  clocks) and PRE-DIV all ACTIVE.
- **ADR-0066 has no row for it.** The register is env-gate shaped
  (`config::deprecated_env_flag`); block mode is the *unset state of a CLI flag*,
  so it fell through the net. The only mention is the `RWM_ACK_MERGE` row's
  parenthetical (goal-gate.md:846), which *protects* block mode ("block mode
  keeps it — `block_arq`'s dup-ack ledger depends on the 1:1 cadence").

So the default is a configuration nobody has measured in a month, on a substrate
that no longer exists, carrying an undischarged DNF.

### Why this is not a free deletion either

Three facts cut the other way, and honesty requires naming them:

1. **Block mode WON the only head-to-head at C2**: 0.884 s vs the reliable
   window's 1.092 s, 1.23× (goal-gate.md:5069-5072, 2026-07-06 — the same section
   that wrote "Default remains block mode", :5049). Window won C3 by 1.9×.
2. **The window pipeline's bulk arm DNF'd 0/10 as recently as 2026-07-05**
   (goal-gate.md:4954) — the reliable-retention contract is what fixed that, and
   it is one month old.
3. **In-order delivery for an inner TCP was originally a block property**
   ("cross-block reordering broke the inner TCP", goal-gate.md:4226); the window
   path's frontier now provides it, but that equivalence was never re-argued at
   the tunnel cells.

Deleting a mechanism whose only head-to-head it WON, on a pre-wall substrate, is
the ADR-0066 error in reverse. The register's own rule — "a refutation is only as
good as the substrate it was measured on" — cuts both ways: a *displacement* is
only as good as its substrate too.

### Why it cannot just sit there either

The block/window fork is a `bool` in a config struct that selects a different
code path, a different codec, a different sender, a different receiver, and a
different ARQ. That is **the last architectural mode bit in the tree** — exactly
the defect CLAUDE.md's no-mode-switch invariant and §16.20 exist to forbid. And
the paper already says it should not be a separate machine at all: "Block mode is
therefore not a different mechanism; it is streaming with the repair kernel's
width taken to zero and its period set to W" (§15.3, fec-arq-model.md:5899-5901).
The tree nevertheless carries **~3,530 lines of block-only production code and
~3,500 lines of block-only tests** implementing that degenerate case separately.

## Decision

**Block mode is LEGACY as of 2026-08-10.** It is not a supported shipped
configuration and it is not a measured one; it is a frozen implementation kept
alive solely to be re-tested. Five binding parts:

1. **Register row (ADR-0066), the register's first non-env member.** Block mode
   joins as a Class-C row with a re-test clause OWED (walls active at its
   refutation-grade datum: W1, W2, W7, W8, PRE-DIV — the same profile that made
   `RWM_FMTCP` "the strongest re-test case"). Its enforcement hook cannot be
   `deprecated_env_flag` — there is no env gate to warn on. The hook is the pin
   in part 3.

2. **The default does NOT flip in this ADR.** Flipping a shipped default on
   inference is the precise failure mode ADR-0052/ADR-0067 exist to prevent, and
   the only head-to-head on record says block wins the cell that matters most for
   a bulk default. The default stays block for Bulk/Auto until the battery in
   part 4 discharges the clause.

3. **The contradiction is PINNED so it cannot drift further.**
   `net::tests::default_config_routes_bulk_and_auto_to_the_block_pipeline`
   (`net/mod.rs`) asserts the ROUTING consequence, not the flag:
   default-resolved config ⇒ `is_window_mode == false` for Auto and Bulk,
   `== true` for Bulk/Auto + `--window-reliable`, and `== true` for Realtime at
   the default. `is_window_mode` had **no test at all** before this ADR — the
   flag was pinned (`config.rs:592`), the pipeline it selects was not. Any
   default move now fails a test and must land with its measurement.

4. **Block mode is FROZEN and owned by one named battery.** Owner: the
   **Block Default Re-Test** (pre-registered here, ADR-0052 shape). Frozen means:
   bug fixes only; no new features on the block path; no block-mode number is
   admissible as evidence for a shipped claim. Restoring a runnable block control
   arm is part of the battery's setup cost — `tools/l1/perf_native.sh` is the
   surviving driver and takes no env, so the arm is glue, not new machinery.

   **Cells** (deliberately the 2026-07-08 cells, so the result is
   apples-to-apples with the last block datum): C1, C2, C3, C4, C5, single-path
   object completion, hints `bulk` and `auto`, seeds 42 + 7, arms interleaved,
   10 reps. Arm A = `perf_native.sh` (block, RaptorQ + P8 block-ARQ). Arm B =
   `perf_rwm_c.sh` with `RWM_GEN=0` (plain reliable window), both on the current
   consolidated default stack (ADR-0067 + subsequent flips).

   **Flip rule, fixed BEFORE the battery** — the default flips to window iff, on
   both seeds:
   - (a) window completes 10/10 at every cell (no DNF), **and**
   - (b) median window completion ≤ **1.0×** median block completion at **C2** —
     the cell block won; the flip must actually REVERSE that result, not merely
     land "within parity", **and**
   - (c) median window ≤ **1.3×** median block at C1, C3, C5 (the standing
     parity gate, goal-gate.md:5072), with no cell regressed ≫σ, **and**
   - (d) C4 completes 10/10 in the window arm (the 2026-07-08 DNF is retired by
     measurement, in whichever arm).

   **Pre-committed branches** (no iteration to manufacture an outcome):
   - **Flip rule met** ⇒ default flips (see part 5 for what the flip entails) and
     block mode is DELETED per Appendix A, register row closed
     `REMOVED <commit> (<date>)`.
   - **Block wins any cell ≫σ on both seeds** ⇒ block mode is RETAINED, gains a
     PERMANENT control arm in every bulk battery, and §16.20's one-machine claim
     is amended to name the exception with its cell and number. The exception
     gets said out loud, not buried in a default.
   - **Mixed / within noise** ⇒ the default does NOT move, the row stays open
     with the per-cell split recorded, and the next consolidation pass decides on
     the structural argument (§15.3) with the split in hand.

5. **The deletion is NOT executed here.** Appendix A is the reviewable removal
   list; executing it is a separate task, and only after part 4. Whoever executes
   it inherits three constraints that are decisions in themselves:
   - **Wire compatibility.** `ControlMessage` is a bincode enum whose *variant
     order is the encoding* (`transport/protocol.rs:240`): `BlockStart`/`Ack`/
     `BlockResult` are variants 0/1/2. Deleting them renumbers every survivor.
     Follow the `FecBackend::Streaming` precedent (`fec/traits.rs:95-98`) —
     delete the handlers, keep the indices stable — or bump `PROTOCOL_VERSION`
     7→8. Note `ControlMessage::Ack` is NOT block-only on the wire (window mode
     emits it too); only `on_ack`'s ARQ tail goes.
   - **The flip is two changes, not one.** `window_reliable = true` alone routes
     nowhere: `is_window_mode` also requires a streaming backend, so the default
     `fec_backend` must move `RaptorQ → Rlc` in the same commit
     (`config.rs:286` + `FecBackend::default()`).
   - **The soft edge becomes hard.** `net/mod.rs:1429-1435`'s warn-and-fall-back
     to block must become a config ERROR once there is no block pipeline to fall
     back to.

## Consequences

- **The default remains, knowingly and dated, not-best-measured.** ADR-0067
  named one such place (the c8 WATCH); this is the second, and it is bigger
  because it is the pipeline, not a pool constant. It is now written down with a
  discharge path instead of living implicitly in a `unwrap_or(false)`.
- **The last mode bit stays in the tree for now.** CLAUDE.md's invariant is not
  weakened — it is recorded as violated at ONE named place with a scheduled
  close. Any new `if window_mode` branch is a defect against this ADR.
- **Carrying cost, quantified**: ~3,530 lines of block-only production code
  (~8.5% of `raptorpath/src`) and ~3,500 lines of block-only tests, plus the
  `raptorq` and `reed-solomon-erasure` dependencies, kept compiling and green for
  a re-test. That cost is now visible rather than assumed.
- **Realtime is untouched** by all of this: it is already window mode at ρ < 1,
  by hint, with no flag.
- **A register-mechanism gap is named**: `deprecated_env_flag` cannot express a
  CLI-default deprecation. Until it can, default-shaped register rows are
  enforced by routing pins like part 3's test. Future default-shaped rows should
  copy that pattern.
- **No engine behaviour changed** in this ADR — one test added, zero production
  lines touched.

**What would reverse this decision**: (i) the Block Default Re-Test measuring
block ahead at any cell ≫σ on both seeds — block ceases to be legacy and becomes
a named, measured exception with a permanent control arm; (ii) a payload class
appearing that needs block's cross-block in-order-decoded delivery contract in a
way the window frontier provably cannot supply (the 2026-07-05 argument,
goal-gate.md:4997-5003) — that would move block from legacy back to supported and
require its own ADR with the mechanism argued, not the history cited.

## Evidence

- Code (this branch, `docs/block-mode-adr`): `config.rs:237-241,286,328,592`;
  `main.rs:288,387`; `net/mod.rs:52-53,789-790,1390,1401-1404,1429-1435,1862`;
  `fec/traits.rs:95-98,101-104,113-114`.
- Ledger: goal-gate.md "Full Benchmark Re-Run (2026-07-08)" (:7970, C1–C5, C4
  DNF at :8000); "RWM Phase A (2026-07-06)" (:5047-5091, the C2 1.23× loss and
  "Default remains block mode"); "Windowed-RLC-all-profiles (2026-07-05)"
  (:4954, window bulk 0/10 DNF); "Streaming Crown Re-Test (2026-07-27)" cell S
  (:14197, :14311-14315); DEPRECATION REGISTER (:809-888, no block row; format
  at :833-834 and :873-874).
- Paper: §15.3 (fec-arq-model.md:5899-5901, block as degenerate streaming);
  §16.20.6 (:8661-8663, "untouched"); §17.5 (:11074-11076).
- Harness: `tools/l1/perf_rwm_c.sh:112,128` (window hardcoded);
  `tools/l1/perf_native.sh:22,27` (the surviving block driver);
  `tools/l1/tail_matrix.sh:228-230` (the retired partner arm).

## References

- ADR-0066 (the register this row joins, and the two-stage discipline),
  ADR-0067 (the default-honesty rule this ADR is enforcing), ADR-0052
  (pre-registration shape of the battery), ADR-0064 (§16.20 one machine),
  ADR-0046 (why the window path was slow before generation coding).
- CLAUDE.md, THE NO-MODE-SWITCH INVARIANT.

## Appendix A — the removal list (NOT executed; input to a separate task)

Reviewable inventory as of 2026-08-10. Line counts are `prod / tests`.

**Whole files, block-only (2,326 prod / 1,006 test):**

| file | prod | test | why block-only |
|---|---|---|---|
| `src/net/block_arq.rs` | 630 | 383 | `BlockArq` is `None` in window mode (`receiver.rs:1830`) |
| `src/net/block_sender.rs` | 327 | 0 | sole caller `net/mod.rs:1862`, after the window early-return |
| `src/net/interleave.rs` | 358 | 333 | reached only from `block_sender.rs` + block repair dispatch |
| `src/net/tasks/arq_sweep.rs` | 114 | 0 | no-ops in window mode |
| `src/net/tasks/decoder_gc.rs` | 58 | 0 | GCs `active_decoders`, populated only by `on_block_start` |
| `src/fec/raptorq_backend.rs` | 219 | 107 | reachable only via the block encoder/decoder factory |
| `src/fec/rs_backend.rs` | 264 | 97 | `is_streaming() == false`, so unreachable from window mode |
| `src/fec/rlc_backend.rs` | 324 | 86 | the BLOCK RLC codec (distinct from `rlc_window.rs`) |
| `src/fec/stream.rs` | 32 | 0 | one consumer, `encode_to_interleave_buf` |

**In-file block-only regions (~1,205 prod):** `net/mod.rs` ~650
(`encode_to_interleave_buf` 6611-6765, `send_interleaved_batches` 6773-7015, ARQ
repair dispatch 7061-7204, the five `window_mode` block arms, the block-only
consts 170-200, `BlockProfile`'s `max_block_size`/`flush_timeout`);
`net/receiver.rs` ~270 (396-439, the `feed_block_symbol` closure 441-548,
910-922, the block receive arm 1677-1720, 1792-1852); `net/control_msg.rs` ~175
(`on_block_start` 198-223, `on_ack`'s ARQ tail 380-402, `on_block_result`
404-479, `evict_oldest_decoder` 788-799); `fec/traits.rs` ~90 (`EncodingParams`,
`FecEncoder`/`FecDecoder` traits, `create_encoder`/`create_decoder`);
`net/framing.rs` ~17 (`frame_packet`/`frame_end` only — `extract_packets` is
shared with window packed mode); `config.rs` `interleave_depth` ~4.

**Tests (~3,500):** fully block — `tests/fec_codec.rs` (362), `fec_production_test.rs`
(391), `fec_realworld_recovery_test.rs` (802), `end_to_end_test.rs` (271),
`block_arq_recovery_test.rs` (227), `fec_waterfall.rs` (218),
`framing_integration.rs` (172); partial — `perf_loopback.rs::perf_loopback_small_object`
(the only live end-to-end block loopback), `ack_merge_loopback.rs`'s block scope
arm, `fec_backend_switching_test.rs` (2 of 4), `protocol_test.rs` (3 of 13),
`shutdown_test.rs` (1 of 6); benches `fec_bench.rs` / `fec_realworld_bench.rs`
(~½ each). NOTE: `tests/block_profile_test.rs` is misnamed — it is wire-format
only and SURVIVES; `tests/sim_reorder_test.rs` tests the shared `ReorderBuffer`
and survives.

**Explicitly SURVIVES (shared, merely reached through block):** scheduler
(except `block_affinity`/`pick_affinity_path`, ~150 lines that die with
`Scheduler::schedule`'s last caller), `control/*` (retarget `fec_rate.rs:166-168`'s
per-backend overhead table and its RaptorQ/RS-parameterized unit tests to `Rlc`),
`transport/*`, `tun/*`, `net/reorder.rs`, `PathBatchTracker`, `framing.rs`'s
window+packed helpers, `WireSymbol`, `enum FecBackend`. The `gf256` crate STAYS
(`fec/generation.rs` is its heaviest user). `mettle` and `streaming-codes` have
no dependency on any of it. `raptorpath-wasm`/`raptorpath-visualizer` do not
depend on the `raptorpath` crate at all and are unaffected.

**Cosmetic-but-notable:** `Cargo.toml:5`'s crate description ("Multipath
transport with RaptorQ fountain codes") becomes false; `WIRE_MAGIC = b"RPTQ"`
(`transport/protocol.rs:66`) becomes vestigial and **must not change** — it is
wire-visible.
