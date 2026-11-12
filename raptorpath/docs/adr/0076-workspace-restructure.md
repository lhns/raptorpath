# ADR-0076: Workspace restructure — one docs tree, one tools tree, ledgers out of the build path

## Status: Proposed (2026-11-12); open questions decided by the user the same day (see "Decisions"). Phase 0: done. Phase 2: lines committed on `restructure` (not yet merged). Phases 1, 3–6: Proposed, not executed.

Number: 0076 is the next free number across every local and remote branch
(0074 is `otel-design`, 0075 is `pacing-seam`). Drafts that exist only
uncommitted in other worktrees are not visible from here.

## Context

The workspace grew around one crate. `docs/` and `tools/` sit inside the
engine crate (`raptorpath/docs`, `raptorpath/tools`) although the paper, the
status ledger, the ADRs and the L1 harness cover the whole workspace. Measured
on `main` at 3bec7652:

- **Ledgers in the build path.** `raptorpath/docs/l1-raw` holds 4 723 files,
  226 MB (41.7 MB `loop-diag`, 20.4 MB `bbr48b`, 19.6 MB `oracle43`, ...).
  Every VM launch `git archive`s two trees of about 240 MB each, and every
  agent worktree checks them out. The python tests and scorers read 49 of
  those files, 4.9 MB (see "Ledger read set").
- **EOL.** The index is all LF (4 009 text files `i/lf`, 0 `i/crlf`). The
  1 066 `-text` files are all `*.gz` / `*.tgz` under `l1-raw`. With
  `core.autocrlf=true` 3 801 files check out CRLF, so every launch archives
  with `-c core.autocrlf=false -c core.eol=lf`, and 42 harness scripts carry
  a `crlf_guard` list against a tree shipped without that flag.
- **Size.** `net/` is 39 files, 30 220 lines (`tests.rs` 6 447, `mod.rs`
  3 792). Observability is spread over `task_obs.rs` (1 609), `runtime_obs.rs`
  (1 196), `perf.rs`, `readout.rs` and `monitor/`. `raptorpath/tests` holds 86
  integration-test binaries, each linked under `lto = true`.
- **Suite time.** The release suite (`cargo test -p raptorpath -p
  raptorpath-math --release`) ran 1 283–1 522 s wall in four archived
  sessions; its build phase was 12 m 54 s–17 m 10 s of that and the summed
  test run times 468–506 s. The `test`-profile build of the same tree (opt
  2, no LTO, debuginfo) took 10 m 15 s–14 m 48 s. The two builds cost about
  the same, but they differ in LTO, codegen units, opt level and debuginfo
  at once, so nothing here isolates the cost of LTO or of the 86 per-binary
  links (`cargo build --release --tests --timings` would).
- **Derivations.** Six branches (`derive-w1`, `adr68-amend`, `paper-recon`,
  `perleg-design`, `w1r2`, `otel-design`) each add files under
  `raptorpath/docs/derivations/`, with no name clashes and no index.

Constraints. No history rewrite and no deletion of ledgers (a past deletion
cost evidence). Open branches (`simplify-5`, `gaphorizon`, `wdiv-diag`,
`bbrrs-r2`, `pacing-seam`) must merge before any move. Any change to engine
bytes needs the §41c guard and the ns bench. Pre-registrations cite
`status §N`, the ledgers and tool paths, so every move must keep those
citations resolvable.

## Decisions (user, 2026-11-12)

1. **Ledgers (phase 2): export-ignore now** with the 49-file keep list
   below. The fixtures version (option (b)) follows with phase 3. The lines
   are committed on `restructure`; the first VM launch from a tree that
   carries them ships the 18 MB archive.
2. **status.md split: yes.** §1–§3 and an index stay in `docs/status.md`;
   each closed section moves to `docs/status/NN-slug.md`; each new
   pre-registration is its own file. The coordinator authors them; workers
   still never write status. Done as pure moves after the open branches
   merge (with phase 3).
3. **Phase 6: move.** The wasm model's five local closed forms
   (`span_deadline_d`, `span_width_a_star`, `pipeline_depth_m_star`,
   `trailing_offset_delta`, `sim_tail_target_of_delta`) and the parts of the
   `Simulation` that are law rather than sim plumbing move into
   `raptorpath-math`. Candidates for the latter, from `raptorpath-wasm/src/lib.rs`:
   the source-position completion kernel
   χ_trunc(remaining, W) = Φ̄((remaining − W/2)/(W/4)) and the tail budget
   r_tail·W·dχ·(1 − β) in `completion_debt_increment` (`:705`), and the
   continuum tail blend (log-blend of the anchor tail with
   p̂ + (BULK_TAIL_BUDGET − p̂)·χ, weighted by β) in `continuum_tail`
   (`:734`). Only branch-free formulas move: the model today still carries
   `hint_bulk` / `legacy_*` / `fixed_r` branches (27 sites) around them, and
   those stay in the model (the no-mode-switch invariant forbids carrying
   them into a law crate). This is a **deliberate model change**: the wasm
   golden fingerprints are re-captured with `GOLDEN_CAPTURE=1` in the same
   change, and the commit says so.

   It **amends CLAUDE.md's scope rule** (owner decision). The amendment is
   drafted here and is not applied to `CLAUDE.md` until the owner applies or
   approves it. Proposed replacement for the first bullet of "Scope rules":

   > - The visualizer (`raptorpath-visualizer/` + `raptorpath-wasm/`) is a
   >   self-contained L0 model: never change the engine crate
   >   (`raptorpath`) for a visualizer feature. A closed-form law the model
   >   evaluates belongs in `raptorpath-math`, shared with the engine, so
   >   both evaluate the same expression (ADR-0076 phase 6); it moves there
   >   with its paper derivation and a deliberate `GOLDEN_CAPTURE=1`
   >   re-capture. Simulation plumbing (state, RNG, stepping, hint
   >   selection) stays in `raptorpath-wasm`.

4. **Integration-test consolidation: measure first, yes.** One
   `cargo build --release --tests --timings` on the VM (pre-registered like
   any VM contact) before any consolidation.
5. **`gf256/src` in `src_manifest`: yes.** Done by the harness owner, not on
   this branch.

## Decision

Six phases. Each has its own precondition and gate and can be accepted or
declined on its own.

### Phase 0 — free items (done: `daf3d65c`, `db4aeb1e`)

A root `README.md` (what the project is, the workspace map, where the paper,
status, ADRs, discipline, harness and ledgers live, the build/test commands
the harness runs). The dead `raptorpath/Cargo.lock` is removed: cargo reads
only the workspace root lock, the member copy was last written in 65d7b1b8,
and nothing references it (the suite-reuse `src_manifest` lists
`Cargo.lock ./*/Cargo.toml`, i.e. the root lock only).

### Phase 1 — one EOL rule

`.gitattributes` becomes:

```
* text=auto eol=lf
*.gz  binary
*.tgz binary
# (pacing-seam, ADR-0075) vendored crates stay byte-identical to upstream:
vendor/** -text
```

The three existing `*.sh` / `*.py` / `*.mjs` lines become redundant and are
removed. `vendor/** -text` stays after the `*` rule so it wins.

- **No renormalisation commit.** `git add --renormalize .` under the new rule
  stages nothing (checked on this branch): the index is already LF.
- **Per worktree, once:** `git rm -rq --cached . && git reset -q --hard`.
  This rewrites every checked-out file as LF and discards uncommitted
  changes. Precondition: no worker mid-edit in that worktree.
- **What it retires.** `git archive` applies the attributes of the tree it
  archives (checked: under `core.autocrlf=true`, `raptorpath/src/lib.rs`
  archives with 15 CR bytes from `main` and 0 from a commit carrying the
  rule), so a tree at or after this commit archives LF without
  `-c core.autocrlf=false -c core.eol=lf`, and `crlf_guard` can no longer
  fire on it. Both stay until no session archives a pre-rule tree: attributes
  come from the archived tree, so a BASE at an older SHA still needs the
  flags. Keep `crlf_guard` (cheap, tested); drop the flags from the launch
  recipe once BASE is post-rule.
- **Gate:** `git ls-files --eol | grep -c 'w/crlf'` is 0 after the
  re-checkout; `git archive HEAD | tar -xO | tr -cd '\r' | wc -c` is 0.
  Count CR bytes with `tr`, not `grep $'\r'`: Git Bash's grep strips CR and
  matches every line (it reported 392 "CR lines" for a file with none).

### Phase 2 — ledgers out of archives and worktrees

Ledgers stay in the repository, unchanged, with full history. Options
weighed:

| option | cost | verdict |
|---|---|---|
| (a) `export-ignore` all of `l1-raw` except the read set; sparse checkout for agent worktrees | no tool edits; lines must track the read set | **now** |
| (b) copy the read set to `tools/l1/fixtures/` (identical blobs cost no object storage), point the tests there, `export-ignore` all of `l1-raw` | edits ~12 test paths; one ignore line that never needs updating | **with phase 3**, when those paths are edited anyway |
| (c) move ledgers to a separate repository, LFS or release assets | breaks ~100 `docs/l1-raw/...` citations in status and the paper; LFS needs server support | rejected |
| (d) delete or rewrite history | loses evidence | rejected |

The export-ignore lines and the sparse-checkout patterns are in "Ledger
read set" below.

- **Precondition:** none beyond review; it changes no source byte and no
  path.
- **Gate:** extract `git archive HEAD` on the host and run every
  `tools/l1/test_*.py` in the extracted tree (the VM `run_suites` does
  exactly this, so a missed file would be an ABORT-TESTS session).
- **Limits.** Attributes come from the archived tree: a pre-rule BASE stays
  full size. No test lists or globs a ledger directory today; one that
  does must keep that directory whole.
  A new test that reads a ledger must add its path (option (b) removes this
  upkeep).

### Phase 3 — `docs/` and `tools/` to the workspace root

One docs tree at the root (`docs/fec-arq-model.md`, `docs/status*`,
`docs/adr/`, `docs/derivations/`, `docs/measurement-discipline.md`,
`docs/research/`, `docs/l1-raw/`) and one tools tree (`tools/l1/`,
`tools/build_paper.sh`). `raptorpath/README.md` and `DESIGN.md` stay with
the crate.

- **Precondition:** the five open branches are merged; no VM session is
  running from an archived tree; no worker has uncommitted edits under
  `raptorpath/docs` or `raptorpath/tools`.
- **Execution:** pure `git mv` commits (one for `docs/`, one for `tools/`),
  then one path fix-up commit. Splitting rename from edit keeps
  `git log --follow` and blame working.
- **Citations survive.** 96 of 101 ledger citations are written
  `docs/l1-raw/...` and 106 of 110 harness citations `tools/l1/...`; after
  the move they read as root-relative paths. Only the
  `raptorpath/docs/...` / `raptorpath/tools/...` spellings need editing. The
  reverse holds for crate-relative citations in the docs (`src/net/...`,
  36 in status and the paper): they stop being root-relative; left as prose.
- **Phase 2's lines move too.** The `export-ignore` paths in
  `.gitattributes` are spelled `raptorpath/docs/l1-raw/...` and are rewritten
  in the fix-up commit (with option (b) they become one line).
- **Checked and unaffected:** `raptorpath-visualizer/build_visualizer.sh`
  (works from the workspace root, names no `docs/` or `tools/` path) and the
  `CLAUDE.md` enforcement paths (`raptorpath-visualizer/*.mjs`,
  `raptorpath-wasm`); only its two `measurement-discipline.md` pointers
  change.
- **Engine bytes.** `gates.rs:598` (test code) reads
  `CARGO_MANIFEST_DIR/tools/l1/lib.sh`; the fix-up edits engine source, so
  the suite-reuse manifest changes and the first post-move session runs the
  suites in full. The release binary does not change (the read is in
  `#[cfg(test)]`); the §41c guard is not needed for this phase alone.
- **Gate:** `tools/l1/test_stubrun.py` (the session driver end to end against
  stubs), every `tools/l1/test_*.py`, the release suite, the doc tests,
  `build_visualizer.sh` and `build_paper.sh`.
- **status.md.** 13 667 lines, 53 sections, cited by `§` number (237 cites
  outside the file); no tool parses it. Proposed, owner's call: keep
  `docs/status.md` as §1–§3 (the live stack, verdicts, debts) plus an index,
  move each closed section to `docs/status/NN-slug.md`, and write every new
  pre-registration as its own file. Every `§N` still resolves through the
  index, and parallel branches stop conflicting on appends to one file. Cost:
  `git log --follow` does not track a split, so the split commit message
  names the source SHA.

The full path-contract list is in "Path contracts" below, with the commands
that regenerate it after the merges.

### Phase 4 — one home for derivations

`docs/derivations/` (today `raptorpath/docs/derivations/`, already used
consistently by all six branches) is the one home, with a `README.md` index
of one row per file (its question, its status, the ADR or § it feeds). It
moves with phase 3.

### Phase 5 — source and crate structure (after W3.2 and W5.7)

Ordered after the per-leg state (W3.2) and the metrics registry (W5.7,
ADR-0074), which move estimators and the observability files anyway. Each
step changes engine bytes, so each needs the §41c guard and the ns bench.

1. **Source-text bounds first.** 33 test sites read engine source text.
   27 name a file (`include_str!` or `CARGO_MANIFEST_DIR.join("src/...")`):
   they fail loudly when that file is renamed, but a negative bound
   (`!contains(..)`, up to 285 such lines) passes **silently** when the code
   it guards moves to another file and the named file stays. Six walk the
   source tree and go **silently vacuous** when code leaves it:
   `gates.rs:567` (env reads vs the gate table), `readout.rs:89` (no raw
   `eprintln!`), `transport/quic.rs:1326`, `net/emit_burst.rs:271`,
   `net/tests.rs:5307` and `:6413`. Before any split: each walk asserts it
   visited the files it is meant to cover and walks every crate the engine's
   code lives in, and each negative bound is paired with a positive one that
   locates the guarded code.
2. **The suite-reuse manifest.** `src_manifest` (`loopdiag_run_all.sh:304-308`)
   hashes `raptorpath/src`, `raptorpath/tests`, `raptorpath-math/src`,
   `raptorpath-wasm/src`, `vendor/` and the Cargo files; it is mirrored in
   `test_harness_paths.py:68-69` and `test_stubrun.py:223,245`. A new crate's
   source is outside it, so a change there would not invalidate an
   `LD_PRIOR_TESTS` reuse. Any new crate is added to all three in the same
   commit. Existing gap today: `gf256/src` (an engine dependency) is not in
   the manifest either.
3. **`net/` submodules** along sender / receiver / recovery / control.
   A module move within the crate; it breaks the path-bound tests in 1.
4. **Controller crate** (`bbr_rs`, with the vendored quinn-proto as its
   dependency). On `pacing-seam`, `bbr_rs.rs` (2 055 lines) has no `crate::`
   edges (quinn-proto, rand, std), so it extracts cleanly. `cc_sim.rs` imports `QuicTransport`'s config
   builders, so it stays in the engine or takes the configs as arguments.
   The `RWM_BBR_*` gates would leave the `gates.rs:567` walk. The gain is
   one file's compile isolation; recommend only when a second
   consumer or a measured compile-time win exists.
5. **Integration-test consolidation.** Measure first
   (`cargo build --release --tests --timings`). Binaries that cannot share a
   process: the three with a `#[global_allocator]` counter
   (`alloc_per_symbol`, `recv_alloc_bound`, `recv_alloc_spread_diag`); the
   17 that call `set_var`, while 16 process-wide `OnceLock` statics in `src`
   resolve the gates (`gates.rs:87`) and the env-armed gauges once per
   process, so the first test in a shared binary would fix them for all; the two that send signals (`eta_reachability`,
   `runtime_obs_reachability`). The rest may merge into a few binaries by
   area. The FLAKE solo re-run (`-- <name> --exact`) still works, but test
   names gain a module prefix, which pre-registrations and ledgers quote.

### Phase 6 — laws shared by the engine and the model (decided: move, see Decisions)

Decided: move (Decisions, item 3). Context as reviewed:
`raptorpath-math` already is the shared-laws crate; `raptorpath-wasm` calls
it 63 times. What `raptorpath-wasm` still computes locally:
`span_deadline_d`, `span_width_a_star`, `pipeline_depth_m_star`,
`trailing_offset_delta`, `sim_tail_target_of_delta` and the `Simulation`.
None of the first four has a same-named engine counterpart. Moving them into
`raptorpath-math` changes an engine crate for a model reason, which
`CLAUDE.md`'s scope rule forbids for visualizer features; whether "the model
and the engine evaluate the same law" outranks it is the owner's call.

## Ledger read set

Measured, not inferred: every `tools/l1/test_*.py` was run on `main` under a
Python audit hook (inherited by the scorers the tests spawn) that logged each
`open` under `l1-raw`; the result was unioned with a static read of the
scorers that take a default ledger path but no test runs (`disp36.py:585`
`loop-diag/p27/ld.log` and `p34/run/ld.log`; `void_rate.py:47` and
`guard_power.py:61,257` `place38|41|41b/ld.log`; `stubrun_gen.py:22-23` the
`thread-pd/battery` fixtures). Every static path was already in the trace.
The read set is **49 files, 4.9 MB** in `anchor-diag`, `loop-diag` (p27, p29,
p34), `mergecand47`, `oracle43`, `place38`, `place41`, `place41b`,
`store-law-n1` and `thread-pd`. No test lists or globs a ledger directory.

Proposed `.gitattributes` lines (verified: `git archive` of a commit with
them carries exactly these 49 files plus the 69 extension-less marker files
such as `COMMIT` and `DONE-ALL`; the archive drops from 242 MB to 18 MB; in
the extracted archive all 30 `tools/l1/test_*.py` that `run_suites` runs pass
with the same number of `ok` checks as on `main`, so none skipped a missing
ledger. `test_stubrun.py` was not re-run there: it hit a 900 s host limit
in the trace run, and its only ledger reads, the `thread-pd/battery`
fixtures of `stubrun_gen.py`, are in the set):

```
raptorpath/docs/l1-raw/**/*.* export-ignore
raptorpath/docs/l1-raw/anchor-diag/diag-ad/c1s-400-MAIN-s42-r1-a1-c.log -export-ignore
raptorpath/docs/l1-raw/anchor-diag/diag-ad/c1s-400-MAIN-s42-r1-a1-drv.out -export-ignore
raptorpath/docs/l1-raw/anchor-diag/diag-ad/c1s-400-MAIN-s42-r1-a1-s.log -export-ignore
raptorpath/docs/l1-raw/loop-diag/p27/diag-ld/c1s-400-DEF2048-s42-r1-a1-c.log -export-ignore
raptorpath/docs/l1-raw/loop-diag/p27/diag-ld/c1s-400-DEF512-s42-r1-a1-c.log -export-ignore
raptorpath/docs/l1-raw/loop-diag/p27/ld.log -export-ignore
raptorpath/docs/l1-raw/loop-diag/p29/diag-ld/c2-100-MAIN-s42-r1-a1-c.log -export-ignore
raptorpath/docs/l1-raw/loop-diag/p29/diag-ld/c2-100-MAIN-s42-r1-a1-drv.out -export-ignore
raptorpath/docs/l1-raw/loop-diag/p29/diag-ld/c2-100-MAIN-s42-r1-a1-s.log -export-ignore
raptorpath/docs/l1-raw/loop-diag/p29/diag-ld/c2-100-PWR-s42-r1-a1-c.log -export-ignore
raptorpath/docs/l1-raw/loop-diag/p34/run/diag-ld/c1d-400-DEF1024-s42-r1-a1-c.log -export-ignore
raptorpath/docs/l1-raw/loop-diag/p34/run/diag-ld/c1d-400-DEF1024-s42-r1-a1-drv.out -export-ignore
raptorpath/docs/l1-raw/loop-diag/p34/run/diag-ld/c1d-400-DEF1024-s42-r1-a1-s.log -export-ignore
raptorpath/docs/l1-raw/loop-diag/p34/run/ld.log -export-ignore
raptorpath/docs/l1-raw/mergecand47/s47b/run/guard/ld.log -export-ignore
raptorpath/docs/l1-raw/oracle43/BINSHA.txt -export-ignore
raptorpath/docs/l1-raw/oracle43/diag-ld/c1s-400-SL-s42-r2-a1-c.log -export-ignore
raptorpath/docs/l1-raw/oracle43/ld.log -export-ignore
raptorpath/docs/l1-raw/place38/ld.log -export-ignore
raptorpath/docs/l1-raw/place38/score.txt -export-ignore
raptorpath/docs/l1-raw/place41/ld.log -export-ignore
raptorpath/docs/l1-raw/place41b/ld.log -export-ignore
raptorpath/docs/l1-raw/store-law-n1/diag-sl/c1d-400-LAW-s42-r1-a1-c.log -export-ignore
raptorpath/docs/l1-raw/store-law-n1/diag-sl/c1d-400-LAW-s42-r1-a1-drv.out -export-ignore
raptorpath/docs/l1-raw/store-law-n1/diag-sl/c1d-400-LAW-s42-r1-a1-s.log -export-ignore
raptorpath/docs/l1-raw/store-law-n1/diag-sl/c1s-400-CTL-s42-r1-a1-c.log -export-ignore
raptorpath/docs/l1-raw/store-law-n1/diag-sl/c1s-400-CTL-s42-r1-a1-drv.out -export-ignore
raptorpath/docs/l1-raw/store-law-n1/diag-sl/c1s-400-CTL-s42-r1-a1-s.log -export-ignore
raptorpath/docs/l1-raw/store-law-n1/diag-sl/c1s-400-LAW-auto-s42-r2-a1-c.log -export-ignore
raptorpath/docs/l1-raw/store-law-n1/diag-sl/c1s-400-LAW-auto-s42-r2-a1-drv.out -export-ignore
raptorpath/docs/l1-raw/store-law-n1/diag-sl/c1s-400-LAW-auto-s42-r2-a1-s.log -export-ignore
raptorpath/docs/l1-raw/store-law-n1/diag-sl/c1s-400-LAW-realtime-s42-r1-a1-c.log -export-ignore
raptorpath/docs/l1-raw/store-law-n1/diag-sl/c1s-400-LAW-realtime-s42-r1-a1-drv.out -export-ignore
raptorpath/docs/l1-raw/store-law-n1/diag-sl/c1s-400-LAW-realtime-s42-r1-a1-s.log -export-ignore
raptorpath/docs/l1-raw/store-law-n1/same-session/aborted-smoke/diag-smoke/c1s-400-MAIN-s42-r1-a1-c.log -export-ignore
raptorpath/docs/l1-raw/store-law-n1/same-session/aborted-smoke/diag-smoke/c1s-400-MAIN-s42-r1-a1-drv.out -export-ignore
raptorpath/docs/l1-raw/store-law-n1/same-session/aborted-smoke/diag-smoke/c1s-400-MAIN-s42-r1-a1-s.log -export-ignore
raptorpath/docs/l1-raw/store-law-n1/same-session/aborted-smoke/diag-smoke/c2-100-LAW-s42-r1-a1-c.log -export-ignore
raptorpath/docs/l1-raw/store-law-n1/same-session/aborted-smoke/diag-smoke/c2-100-LAW-s42-r1-a1-drv.out -export-ignore
raptorpath/docs/l1-raw/store-law-n1/same-session/aborted-smoke/diag-smoke/c2-100-LAW-s42-r1-a1-s.log -export-ignore
raptorpath/docs/l1-raw/store-law-n1/same-session/aborted-smoke/diag-smoke/c2-100-MAIN-s42-r1-a1-c.log -export-ignore
raptorpath/docs/l1-raw/store-law-n1/same-session/aborted-smoke/diag-smoke/c2-100-MAIN-s42-r1-a1-drv.out -export-ignore
raptorpath/docs/l1-raw/store-law-n1/same-session/aborted-smoke/diag-smoke/c2-100-MAIN-s42-r1-a1-s.log -export-ignore
raptorpath/docs/l1-raw/thread-pd/battery/diag-smoke/c1s-400-MAIN-s42-r1-a1-c.log -export-ignore
raptorpath/docs/l1-raw/thread-pd/battery/diag-smoke/c1s-400-MAIN-s42-r1-a1-drv.out -export-ignore
raptorpath/docs/l1-raw/thread-pd/battery/diag-smoke/c1s-400-MAIN-s42-r1-a1-s.log -export-ignore
raptorpath/docs/l1-raw/thread-pd/battery/diag-tpd/c1d-400-MAIN-s42-r1-a1-c.log -export-ignore
raptorpath/docs/l1-raw/thread-pd/battery/diag-tpd/c1d-400-MAIN-s42-r1-a1-drv.out -export-ignore
raptorpath/docs/l1-raw/thread-pd/battery/diag-tpd/c1d-400-MAIN-s42-r1-a1-s.log -export-ignore
```

Why `**/*.*` and not `l1-raw/**`: `git archive` does not descend into a
directory that carries `export-ignore`, so `l1-raw/** export-ignore` drops
the whole tree and no `-export-ignore` below it ever applies (checked). The
pattern relies on no directory under `l1-raw` having a dot in its name
(true today; a dotted directory would be dropped whole, and the archive gate
would catch a test that needs it).

Sparse checkout for agent worktrees (non-cone: cone mode cannot exclude one
directory without listing every sibling, and `docs/derivations/` is about to
appear on six branches). The same keep list, derived from `.gitattributes`:

```
git worktree add --no-checkout --detach <path> <ref> && cd <path>
{ echo '/*'; echo '!/raptorpath/docs/l1-raw/**/*.*'
  MSYS_NO_PATHCONV=1 git show "<ref>:.gitattributes" | tr -d '\r' |
    sed -n 's#^\(raptorpath/docs/l1-raw/[^ ]*\) -export-ignore$#/\1#p'
} | git sparse-checkout set --no-cone --stdin
git checkout --detach <ref>      # or: git switch -c <branch> <ref>
```

Checked exactly as written, in a fresh scratch clone: the worktree gets the
49 kept files and the 69 markers (226 MB to 4.9 MB), all other sources
intact, the main checkout not sparse. The keep list is read with `git show`
because a `--no-checkout` worktree has no `.gitattributes` on disk yet.
Notes: Git Bash rewrites a leading `/` argument and a `<ref>:<path>`
argument into Windows paths (both observed), hence `--stdin` and
`MSYS_NO_PATHCONV=1`; a directory pattern (`dir/`) does not re-include files excluded
by the file pattern, so the list names files; `git sparse-checkout` in a
linked worktree sets `extensions.worktreeConfig=true` in the shared
repository config (observed), so do the first one when no worker is running. A worker that needs a full
ledger runs `git sparse-checkout disable`.

## Path contracts

What phase 3 must edit, on `main` at 3bec7652. The list goes stale as the
open branches merge; regenerate it right before the move with:

```
git grep -nE 'HERE[^ ]*\.\./\.\.|"\.\.", "\.\."' -- 'raptorpath/tools'
git grep -nE 'raptorpath/(tools|docs)|src/raptorpath' -- ':!raptorpath/docs/l1-raw'
git grep -nE 'CARGO_MANIFEST_DIR|include_str!' -- '*.rs'
git grep -nE '\]\((\.\./)*docs/' -- '*.md' ':!raptorpath/docs'
```

Loud (a wrong path fails a test or the launch):

| contract | sites | edit |
|---|---|---|
| run scripts find the repo root as `$HERE/../../..` | `emitscope_run_all.sh:43`, `lagdiag2_run.sh:30`, `lagdiag_run.sh:22`, `loopdiag_run_all.sh:246,305`, `p0_run_all.sh:40`, `q3diag_run.sh:21`, `stage3_run_all.sh:52`, `suites_run.sh:15`, `threadp1/pd/q1/q2/r1/r2/sk_run_all.sh` (`SRC=` line), `verify4_run_all.sh:45` | `../..` |
| batteries read the run root's `COMMIT` as `$HERE/../../../COMMIT` | `emitscope, p0, stage3, threadd9, threadp1, threadpd, threadq1, threadq2, threadr1, threadr2, threadsk, verify4 _battery.sh` (12) | `$HERE/../../COMMIT` (the archive root holds `COMMIT`) |
| python reads ledgers as `HERE/../../docs/l1-raw` | `disp36.py:585`, `guard_power.py:39`, `stubrun_gen.py:20`, `void_rate.py:20`, `test_anchordiag_parse.py:19`, `test_bbr48_parse.py:335`, `test_disp36.py:356,359`, `test_harness46.py:251`, `test_harness47.py:57`, `test_loopdiag_parse.py:25,27`, `test_place38_parse.py:82`, `test_place43_parse.py:220,575,606,620`, `test_replace47.py:184`, `test_storelaw_parse.py:18,57`, `test_stubrun.py:201` | `HERE/../../docs` stays correct after both moves (`tools/l1` → `docs`) |
| `cd raptorpath/tools/l1` inside suite commands | `loopdiag_run_all.sh:292,294`, `suites_run.sh:77`, `threadpd/q1/q2/r1/r2/sk_run_all.sh` (`tst parsers` line) | `cd tools/l1` |
| the archived tree's harness path | `threadpd_alloc.sh:32,61`, `threadpd_tests.sh:35,68` (`$ROOT/<tree>/raptorpath/tools/l1/...`) | `$ROOT/<tree>/tools/l1/...` |
| the stub-run sandbox layout | `test_stubrun.py:143` (`repo/raptorpath/tools/l1`), `:158` (tree) | `repo/tools/l1` |
| the forwarding audit reads the harness list | `raptorpath/src/gates.rs:598` (`CARGO_MANIFEST_DIR/tools/l1/lib.sh`, `#[cfg(test)]`) | `../tools/l1/lib.sh` |
| the bench suite writes its results | `raptorpath/tests/bench_suite.rs:1758,1762`, ignored by `.gitignore:16-18` | write to `../docs/` and move the ignore lines, or keep them in the crate |
| the paper build finds the paper as `$SCRIPT_DIR/../docs` | `raptorpath/tools/build_paper.sh:16-19` | unchanged if both move |
| hard-coded VM checkouts | `holeaudit_battery.sh:6`, `place_battery.sh:68`, `place_run_all.sh:46`, `recvlaw_battery.sh:100` (historical batteries, absolute VM paths) | `.../tools/l1` or leave as history |

Silent (nothing fails if left wrong):

| contract | sites |
|---|---|
| **a test that skips when its file is absent** | `test_place43_parse.py:40-47` reads `HERE/../../src/net/oracle/tests.rs` and prints "skip" when it is missing: after the move it must read `../../raptorpath/src/...` or it stops comparing the Rust fixture without failing. `test_place43_parse.py:421` (`isdir(loop-diag/p27)`) does the same for a ledger, so phase 2 keeps that file. Note: `src/net/oracle/tests.rs` is not on `main`, so the fixture comparison already skips there today |
| standing-rule pointers | `CLAUDE.md:43,72` |
| markdown links into `docs/` from outside it | 12 in `raptorpath/README.md` and `DESIGN.md`; the root `README.md` |
| printed host commands | `bbr48_score.py:413`, `place38_parse.py:866`, `loopdiag_run_all.sh:413` (`python3 raptorpath/tools/l1/...`) |
| usage text and comments naming `src/raptorpath/tools/l1` | 13 `*_run_all.sh` headers, `place38_ledger_score.py:4` |
| prose citations `docs/...`, `tools/l1/...` in code comments and docs | 52 `docs/` cites in `*.rs`; 96 `docs/l1-raw` and 106 `tools/l1` in markdown — correct as root-relative after the move, so left alone |

Not path contracts: the `crlf_guard` lists name files relative to the
script's own directory and survive the move. The `$ROOT/src/raptorpath`
crate paths in the run scripts (build `cd`s) do not change, since the crate
stays where it is.

## Consequences

- Phases 0–2 change no source and no path; they shrink VM archives and
  worktrees and remove the EOL workaround for new trees.
- Phase 3 is one coordinated stop: every open branch merges first, and
  in-flight branches started before it must be rebased across a rename.
- Phase 5 is engine work with guard and bench cost per step; it is worth it
  only after the walk-based audits are made non-vacuous.
- Phase 6 needs the owner to apply the drafted `CLAUDE.md` amendment and a deliberate golden re-capture.
