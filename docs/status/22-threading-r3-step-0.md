## 22. Threading R3 step 0 — the c1d receiver re-profile — pre-registration

§16 finding 4 named the c1d ceiling: the server's one receiver task (busy
0.95, 0.85 core, `[RDIAG]` 99–100 %); §21 (R2, DELIVERED) lowered its
per-message cost but left it saturated at c1d (`[RDIAG]` median 99 %).
Plan v2's Q3 allows a receiver-role split only for a separable role of
≥ 0.3 core. This section measures what the receiver's time is made of on
**current main** (3982bd1; its engine tree is 0972c3c's — §23 changed tools
and docs only) so the R3 design is chosen by the data. It is a
**diagnosis**: nothing ships, nothing is tuned. No number below the
"Readings so far" paragraph is a result.

**Record of an invalid first attempt (stated before any result).** A first
session of this design ran on 2026-10-26 15:58:11–16:07:30Z from an
unmerged branch whose "pre-registration" commit (`42b074c`) did **not**
contain this text: the agent's draft file in a scratch directory shared
with another agent had been overwritten by that agent's §23 draft, and the
commit appended a duplicate of §23 instead. That session therefore ran
without a valid pre-registration. It ended `ABORT-TESTS` (the new
`task_obs::tests::the_rxsec_line_splits_a_message_into_its_sections` caught
runs of spaces in the instrument's format string — a real defect, fixed in
its own commit); no row, bench or profile ran, both locks were released,
0 `raptorpath`, 0 `rp-*`. Nothing from it is read. This text is the
pre-registration; it is committed before the next VM contact.

**Readings so far (not results).** The only `[RXSEC]` reads are on R1-era
trees: §17's c1d dev read (R1, with the `[LAT]` O(gap) defect live) 16.7 µs
per message — decode 34.8 %, deliv 9.0 %, ack 14.0 %, book 15.8 %, other
26.4 %, backlog p50 256 / p99 1536; §20's c1s `FIX` rows (≈ R2) 13.1–14.5
µs — decode 5.96 µs (41 %), other 4.72 µs (32 %), book 1.0 µs (7 %). One
message = one symbol (msgs ≈ data ≈ 338 k per 400 MB). By arithmetic on
0.85 core, decode ≈ 0.30–0.35 core sits **on** the 0.3 bar; book, ack,
deliv and other are each below it.

**What decode is, read from the code** (`fec/unified.rs`). An in-order
source symbol with no loss does no GF work but pays: a SipHash `HashSet`
insert of a 3-tuple (`seen`), a `BTreeMap` probe, a zeroed
`vec![0; symbol_size]` plus a copy, a `Vec` and a `VecDeque` for the
worklist, a payload clone into `recovered` (`BTreeMap<u64, Vec<u8>>`), a
`BTreeSet` insert (`delivered`), a second payload clone into the output
`Bytes`, and a scan of every stored row (`covering`): three payload copies
and about five allocations per symbol. A k = 0 repair walks
`recovered.range(anchor..anchor+width)` (O(width) node visits). And
`advance` — the only prune of `recovered`, `delivered` and `seen` — runs
once per `REPORT_INTERVAL` = 2 s (`net/receiver.rs`, the periodic block),
so the decoder holds ~2 s of payload copies (≈ 140 k × 1.2 kB ≈ 165 MB at
c1d) and frees them in one burst; the burst sits outside every `[RXSEC]`
section today (it reads as "other").

**The instrument** (`RWM_RTOBS=2` only, measurement only): three more
`[RXSEC]` sections appended to the line — `arr` (the arrival stamp: RX
jitter, `xdir` publication, revival nudge, and the per-path batch
tracker's DashMap entry), `probe` (the per-source `[RFA]` `seq_probe` and
the `RWM_DIAG` conversion duplicate test, before `add_symbol`), `prune`
(the `received_seqs` prune and the decoder's `advance`), with `prune_n`
and the run-cumulative `prune_max_us`; "other" becomes the remainder of
all seven. A decoder bench
(`fec::unified::tests::bench_in_order_add_symbol_against_the_prune_cadence`,
`#[ignore]`): 300 k in-order loss-free 1 182-byte sources and one width-200
repair per 20, `advance` to a lag of 400 either every 140 k symbols (the
2 s cadence at c1d) or every 1 k; ns per source `add_symbol`, per repair,
and the `advance` maximum. The bench and the `task_obs` unit tests run in
the session (release), before the rows.

**Hypotheses and predictions** (fixed now; read in the result as MET /
MISSED with the numbers).
- **H-alloc — decode is bookkeeping, not field arithmetic.** (a) In the
  c1d profile, the samples under `UnifiedDecoder::add_symbol` (children)
  that are in `gf256` (`mul_acc_slice`, `mul`) are ≤ 20 % of
  `add_symbol`'s children; the allocator, `memmove`/`memset`, `BTreeMap` /
  `BTreeSet` and `HashSet` frames are ≥ 50 %. (b) Bench: source
  `add_symbol` at the 140 k cadence costs ≥ 1.5 × the 1 k cadence.
- **H-burst — the 2 s prune is a stall.** `prune_max_us` ≥ 5 000 at c1d in
  every row; bench `adv_max_us` ≥ 5 000 at the 140 k cadence.
- **H-other — the remainder is per-message lookups.** At c1d `arr` +
  `probe` + `prune` ≥ half of R1's "other" share (i.e. the new remainder
  is ≤ 15 % of the handler total).
- **Re-read of the split** on current main at c1d, reported per section
  as ns per message and share; receiver `[TASK]` cores and busy;
  `[RDIAG]` busy median; backlog p50 / p99.

**Decision rule** (fixed now; the R3 design follows from it).
- Role core = section share × the receiver task's `[TASK]` cores (c1d
  median). A role is *separable* only if its inputs and outputs can cross
  a channel without breaking an invariant: decode needs the window-decoder
  state and its probes (`seq_probe`, `frontier_probe`, `rank_in`,
  `advance`); delivery needs decode's output in seq order; acks are built
  after decode (wire v9 honesty: an ack reflects decoded state);
  `[LAT]` / `[SUCC]` read the same per-symbol events.
- **R3 = make the decoder cheaper in place** (no new task) if H-alloc (a)
  holds: the decode share is then mostly removable bookkeeping, and a
  decode task would move that waste to a second thread, add a channel hop
  to the head of the ack critical path (the shape §13's `own` arm measured:
  hops added, no goodput) and a second allocator-contending thread (§16
  finding 4's [H]).
- **R3 = a decode-stage task** (one task owning the decoder, batched
  channels both ways, one structure at every N) only if H-alloc (a) is
  refuted (GF > 20 %: the cost is irreducible arithmetic) **and** decode ≥
  0.3 core.
- If neither holds (no role ≥ 0.3 core and decode is not bookkeeping), no
  split is built and the report names the largest section.
- The ship prediction any R3 must meet (§21's c1d clause: goodput BETTER
  beyond the 5.6 % MDE with the server `[RDIAG]` busy median < 90 %) needs
  the per-message handler cost down by ≥ ≈ 25 % at c1d; the design states
  its expected saving against that bar.

**Session.** The tree (this section's HEAD, `e4d078e`: main + the
instrument, without the R3 change that follows it on the branch) shipped with `git -c
core.autocrlf=false -c core.eol=lf archive`, built on the VM in a fresh
target dir, run as `raptorpath`. Envelope `tools/l1/r3diag_run.sh`, both
locks for the whole session, detached, launched only when the coordinator
releases the VM (the storage stall of 2026-10-26 evening). Launch
precondition as §21 (both locks free by `test -e`, loadavg 1-min < 2.0,
no `cargo` / `rustc` by `pgrep -x`), checked before the locks
(`ABORT-PRECONDITION`); hard deadline = launch + 3 h. Order: build;
`cargo test -p raptorpath --release --lib -- task_obs::` (a failure is
`ABORT-TESTS`); the bench; rows `c1d-400` and `c1s-400` alternated, seeds
42, 7, 42 (n = 3 per cell), bulk, `--window-reliable`, shipped defaults,
`perf_rwm_c.sh` with `RWM_GEN=0 RWM_DIAG=1 RWM_RDIAG=1 RWM_RTOBS=2
RWM_PERF_TIMEOUT_S=150` (§20's env scrub); then one server `perf record -g
--call-graph dwarf,16384 -F 999` for 3 s in the steady state of a 1 GB
`c1d` transfer (`RWM_DIAG=1`, no `RWM_RTOBS`), reported as self and
children tops, the callers of the allocator and `memmove` symbols, and the
folded `perf script` (`tid,sym`) for a receiver-rooted breakdown done off
the VM (`tools/l1/r3diag_fold.py`). A row needs a client summary and the
server's `[PIPE] pipeline=window backend=Rlc hint=bulk`; the instrument
witness is an `[RXSEC] phase=xfer side=server` line carrying `prune_n=`
with `prune_n ≥ 1` (rule 1: the new sections execute). Single-seed-class
diagnostic, not scored. `pkill -x raptorpath` only; no `ens18`, firewall,
`sshd` or non-`rp-*` namespace touched; exit state 0 `raptorpath`, 0
`rp-*`, locks released, VM dirs removed. Ledgers to
`docs/l1-raw/thread-r3-diag/`.

### 22. R3 design — written before the §22 data, gated on its verdict

Written while the VM is stalled (no §22 row exists yet), so it is the
design the decision rule's **in-place** branch would select; it is built
now and tested only by `cargo check` on the host. It ships only if §22's
rule selects it **and** its own ship battery (the next pre-registration)
passes. If §22 refutes H-alloc (a), this build is shelved and the
decode-stage design is written instead.

**Change** (paper §5.2 "Retained state", §8.8; one structure at every N,
no hint, δ, ρ or path-count branch, no new constant):
1. `fec/unified.rs` — known payloads as the received `Bytes` (zero copy;
   a payload shorter than `symbol_size` is padded once); the output shares
   that buffer; `delivered` removed (it equals `recovered`'s key set);
   source dedup by a per-entry source-seen flag on the known payload,
   repair dedup by an ordered set of `(anchor, payload_id)` pruned by
   range; covering-row lookup bounded to pivots in
   `(s − max_row_len, s]` (no scan when no row is stored); the worklist
   buffer reused; a decode-inert floor `F` set by `advance`: a source with
   seq < F, or a repair whose span ends ≤ F, is refused and counted
   (`below_floor` in `diag_stats`). `seq_probe`, `frontier_probe`,
   `rank_in`, the counters and the delivered-set contract are unchanged.
2. `net/receiver.rs` — the prune (`recv_above.sync`,
   `received_seqs.prune_below`, `window_decoder.advance`) moves from the
   `REPORT_INTERVAL` block to the loop's wait point (once per drained
   `recv_many` batch), with the same `prune_before` expression. The
   `[REASM]` probe and the FEC feedback stay on the 2 s tick.
   Declared gauge effect: `[C8CONV-R]` `dup` counts duplicates only within
   the retained span.

**Expected saving** against the 25 % bar: decode's bookkeeping (§20 c1s
FIX: 5.96 µs of 14.5) minus what remains (one BTreeMap insert, one output
`Vec`, the k = 0 walk for repairs) ≈ 3–4.5 µs per message, 20–30 % of the
handler, plus the 2 s burst. Borderline against the bar: the ship
battery's c1d prediction is reported, not gating, and a PARTIAL there with
CPUSRV BETTER is the expected reading if the saving lands at the low end.

**Red-first tests (absolute)**, red on the current tree, green after:
- `tests/decoder_alloc_bound.rs` — counting allocator, `UnifiedDecoder`
  alone, in-order 1 182-byte sources with `advance` to lag 400 every
  1 000: heap allocations per source ≤ 1.5 (today ≈ 5–6).
- zero copy — the output payload of an in-order source shares the input
  `Bytes` buffer (pointer equality).
- the floor — after `advance(F)` a source seq < F (a duplicate of a
  delivered seq) returns nothing and is counted; today it is re-delivered.
- routing (rule 1) — the receiver prunes at every drain: a loopback run
  where the decoder's retained known-payload count stays ≤ `2·W_cap +
  MSG_CHANNEL_DEPTH` at every readout (today it grows to ~2 s of seqs).
- a source-scan gate: no `REPORT_INTERVAL` guard around the prune.
Green suites: `fec::unified` and `fec::generation` differentials,
`unified_l0`, `unified_stream_l0`, the loss-truth and SACK tests in
`net::tests` and `net::seq_bits`, `reliable_delivery_hold`,
`lock_order_loopback`, `ack_delivery`, `recv_alloc_bound`,
`inbound_pool_reachability`.

### 22. R3 ship battery — pre-registration

The battery that decides whether R3 (the design above) ships. It runs
**only if** §22's diagnostic verdict selects the in-place branch (H-alloc
(a) MET); otherwise this subsection is void and says so in its result.
Committed before any battery VM contact; no number below is a result.
Nothing ships from it but by the operator's merge.

**Arms.** `MAIN` = `github/main` at launch (named by its commit in the
ledger; 3982bd1 when written), `R3` = this branch's HEAD (MAIN + the §22
instrument + the R3 change + this text and the harness). Both bulk,
`--window-reliable`, shipped defaults; every arm `env -u RWM_EST_CADENCE
RWM_POOL_ANCHOR RWM_EMIT_BATCH RWM_EMIT_BURST RWM_RTOBS RWM_RDIAG
RWM_ACKDIAG RWM_IO_RT`, then `RWM_DIAG=1 RWM_RTOBS=1 RWM_RDIAG=1` (rule
15d), `perf_rwm_c.sh` with `RWM_GEN=0 RWM_PERF_TIMEOUT_S=150 SEED=<seed>`,
one run, a fresh topology per invocation.

**Cells, n, order** (§21's, unchanged): `c1s-400`, `c1d-400`, `c2-100`,
`c8-100`; seeds 42 and 7, n = 3 per seed (6 per arm and cell),
interleaved, arm order within a cell rotated by the block index (rule 3);
the budget rule cuts at a rep boundary only.

**Witnesses per invocation** — §21's set (stage3 set, emission batching,
`wake[`, the gates, in-transfer `[RDIAG]`, `[THR]`/`[LAG]`, `[IOWN]` both
ends, the Q2 sender lane, R1's `ibp_hit=` now on **both** arms since MAIN
contains R1, the row's `sha256`) plus **the R3 execution witness,
two-sided** (rule 1): on every R3 row the server's in-transfer `[RDIAG]`
lines carry `dec[` with `recovered=` ≤ 2·200 + 4 096 = 4 496 at every line
(the decode-inert floor applied per drain; MAIN holds ~2 s of seqs); on
every MAIN row no `dec[` token. A failing row is `WITNESS-FAIL`, excluded
and counted.

**Pass rule per cell** (§21's, R3 against MAIN, verbatim): a clause is
WORSE iff R3's median is beyond MAIN's median·(1 ∓ rel) in the worse
direction **and** the [min, max] ranges are disjoint in that direction
(TREND-WORSE otherwise beyond the band; reported, not a fail):
goodput (§5 MDE: c1s 4.9 %, c1d 5.6 %, c2 1.4 %, c8 4.0 %); client and
server CPU per byte (2.4 / 6.5 / 6.3 / 11.6 %); RTprop floor per leg and
`[LAG]` p99 per side (max(5 %, MAIN's half-range / median)); fed loss =
`[TRUTH]` (per leg med(`plc`/`[TRUTH]`) within [1/1.3, 1.3] × MAIN's); DNF
(excess > 0.20). Per-cell **FAIL** iff any clause WORSE, the feed moved or
DNF; **UNSCOREABLE** iff < 3 live rows on an arm, ≥ 2 witness-failed rows,
an unread feed ratio or an abort; **PASS** otherwise.

**Outcomes** (precedence): `UNSCOREABLE` (abort); `DELIVERED` (R3 passes
at every cell: ships, by the operator's merge); `REFUTED-WITH-RECORD
(FAIL-AT-<cells>)` (nothing ships, no tuning in-battery);
`UNSCOREABLE-AT (…)`.

**Predictions** (reported, not gating; named `PREDICTION` lines):
- `c1d_goodput_better` — c1d goodput **BETTER** (beyond 5.6 % with
  disjoint ranges) **and** the server `[RDIAG]` busy median < 90 %.
  Mechanism: decode's bookkeeping removed (≈ 3–4.5 µs of ≈ 14 µs per
  message, design above) and the 2 s free burst gone; PARTIAL (goodput
  TREND-BETTER or busy ≥ 90 %) is the reading if the saving lands low.
- `cpusrv_better` — server CPU per byte BETTER at c1s and c1d.
- `c2_c8_within` — c2 and c8 goodput and both CPUs WITHIN or better.
- `c1d_backlog` (reported) — server `[RDIAG]` `q_avg` median lower on R3.

**Tests first** (inside the locks, on the R3 tree, fresh target): the
§21 set — release `cargo test -p raptorpath -p raptorpath-math --release
--no-fail-fast -- --test-threads=2`, doc, `raptorpath-wasm`
(`GOLDEN_CAPTURE` unset), debug `cargo test -p raptorpath --no-fail-fast
-- --test-threads=2` (`IDENTITY-PANICS` 0), the parser tests — plus the R3
red tests run on the MAIN+tests tree first to show them red (the
`decoder_alloc_bound`, zero-copy, floor and per-drain routing tests),
then green on R3. A failure that passes an immediate solo re-run is
`FLAKE`; any other is `ABORT-TESTS`.

**Abort causes, launch precondition, deadline, session rules** — §21's,
verbatim: `ABORT-LOCK`, `-CRLF`, `-BUILD`, `-TESTS`, `-SHA`,
`-SENTINEL-UNWRITABLE`, `-PRECONDITION` (both locks free by `test -e`,
loadavg 1-min < 2.0, no `cargo`/`rustc`), `-SMOKE` (one invocation per arm
at `c1s-400` and `c8-100`, seed 42; R3 rows must carry the `dec[`
witness), `-BUDGET`, `-RC`, `-BRINGUP`; void `VOID-COTENANT`. Hard
deadline launch + 4 h. Both locks for the whole session, detached
envelope, earned sentinels, `pkill -x raptorpath` only, no `ens18`,
firewall, `sshd` or non-`rp-*` namespace, exit 0 `raptorpath` / 0 `rp-*`,
VM dirs removed.

**Harness.** `tools/l1/threadr3_{run_all,battery}.sh`, `threadr3_parse.py`
(+ `test_threadr3_parse.py`), derived from `threadr2_*` by renaming the arm
and swapping R2's execution witness for R3's `dec[` witness. Ledgers to
`docs/l1-raw/thread-r3/`.

**Planned duration.** §21 measured: builds 300–380 s each, release +
doc + wasm + debug suites ≈ 45 min quiet to ≈ 2 h loaded, smoke < 1 min,
48-row battery ≈ 8 min. Expected session ≈ 1 h 30 quiet, ≈ 2 h 30 loaded.

### 22. R3 ship battery — Amendment 1 (before any battery VM contact)

1. **`[LAG]` is scored on E = p99 − `min_us`** of the window, client and
   server (the operator's scoring decision recorded with the c8 lag
   diagnosis), with the clause's rel unchanged (max(5 %, MAIN's half-range
   / MAIN's median), computed on E). Raw p99 stays reported. `min_us` comes
   from that work's `[LAG]` instrument (rebased `b2a5fcc`); the battery
   launches only once `github/main` contains it, with this branch rebased
   onto that main, so both arms print it. A row whose `[LAG]` window lacks
   `min_us` reads the clause UNREAD (counted; ≥ 2 such rows at a cell make
   the cell UNSCOREABLE). The parser gains the E column before launch.
2. **The floor refusal is a ρ < 1 behaviour change**, declared: below the
   decode-inert floor F the decoder now refuses a late fill of a hole the
   frontier gave up more than 2·W_cap seqs ago (paper §5.2), where it was
   accepted up to one report tick late before. On the battery's cells
   (bulk, `--window-reliable`, ρ = 1) such a refusal can only be a
   duplicate of a delivered seq. It is measured, not gated: the server's
   `below_floor=` (last `[RDIAG]` `dec[` value) is reported per R3 row and
   cell, median [min–max].

### 22. Step-0 diagnostic — Amendment 1 (before its relaunch)

The VM's storage stalled on 2026-10-26 evening (`/proc/pressure/io` `full`
≈ 97 %, then 75–85 % at loadavg 1–2, which the loadavg clause does not
see). For the step-0 diagnostic (and, by the same text, the R3 ship
battery above):
1. **Launch precondition** gains `/proc/pressure/io` `full avg60` < 5.0
   (checked with the others before the locks).
2. **Per row**, io pressure `full avg10` is recorded before, every 2 s
   during, and after the invocation; the ledger's `ROW` line carries
   `io_full10_max=`. **A row with any reading ≥ 20 is `VOID-IOSTALL`**:
   excluded from every reading of the result and counted. The perf run
   records the same before/after.
3. **The waiter** (`tools/l1/r3diag_wait.sh`) runs detached on the VM:
   it polls the precondition once a minute (locks by `test -e` only; it
   never opens or breaks a lock) for up to 3 h, then runs the envelope,
   which re-checks and takes both locks with `take_lock`; a run refused
   before the locks (precondition, or a lock taken in between) is retried.
   If the precondition never holds, the waiter logs `NOT-LAUNCHED` and
   nothing runs.
4. **Tree.** The engine tree stays `e4d078e` (main + the instrument); the
   harness files `tools/l1/r3diag_run.sh` and `r3diag_wait.sh` come from
   this amendment's commit (harness only, no engine file differs; the
   swapped files' `sha256` are logged in the ledger).

### 22. Amendment 2 (step-0 diagnostic and R3 ship battery, before either launch)

Measurements run from tmpfs (the shared rule after the storage stall): the
built binary is copied to `/dev/shm/<dir>/bin` and the run dir (ledger,
per-row logs, io samples, perf output) lives under `/dev/shm/<dir>` from
the first measured invocation to after the last; the ledgers are then
copied to disk and the tmpfs dir removed. Builds and test suites stay on
disk and are not measurements. A large target dir is deleted only while
the session still holds both locks (so no other battery is running). The
I/O precondition and `VOID-IOSTALL` of Amendment 1 stay. The step-0
harness (`r3diag_run.sh`) carries this now; the R3 battery's harness
(`threadr3_*`) gains the same, plus Amendment 1's io clause, before its
launch.

### 22. Amendment 3 (step-0 diagnostic and R3 ship battery, before either launch; supersedes Amendment 1's gates)

The operator-approved shared rule replaces the launch precondition for
both: **both locks free (`test -e`), no `cargo` / `rustc` (`pgrep -x`), and
`procs_running` (`/proc/stat`) ≤ 2 in each of 5 samples 2 s apart**. The
loadavg clause (§21) and Amendment 1's io gate are dropped: loadavg counts
disk-blocked tasks, and the measurements run in memory on `/dev/shm`
(Amendment 2). Io pressure is a **covariate**: `full avg10` is recorded
before and after every row (and every 2 s during it, as `io_full10_max=`);
a row with a reading ≥ 20 is marked `FLAG-IOSTALL` and, if it is an
outlier, reported as such — it is **not** voided. A row whose binary or
run dir is not on `/dev/shm` is `VOID-ONDISK`. Builds and test suites have
no io gate. The waiter and envelope carry this (`r3diag_wait.sh`,
`r3diag_run.sh`); the R3 battery's harness gains it before its launch.

### 22. Threading R3 step 0 — the c1d receiver re-profile — result

**Verdict: `UNREAD` on the deciding witness. The rule selects no design.**
H-alloc (a), the profile witness, has no data: the `perf record` attach
failed ("Couldn't create thread/CPU maps: No such process" — the pid
`pgrep -o -x raptorpath` returned was not the live server's), so every perf
product is empty. The decision rule builds the in-place R3 only if (a)
holds, and a decode task only if (a) is refuted; neither is read, so
neither is selected. Scored as pre-registered; nothing was changed after
the data was read.

*Session.* Launched by the waiter 2026-10-26 21:08:24Z after one clean
refusal (20:28:44Z, `ABORT-LOCK`: another agent's session had taken the
locks two seconds earlier; nothing ran). Engine `e4d078e`, harness
`35085a9` (sha256 in the ledger), binary `aa09d965…`; build 624 s;
`task_obs` tests 7 passed (the instrument's own tests, green); bench ran;
rows 21:26:04–21:26:42Z from `/dev/shm`; END 0 `raptorpath`, 0 `rp-*`,
locks released 21:27:30Z; the VM dir and the tmpfs dir are removed.
Ledgers: `docs/l1-raw/thread-r3-diag/run/` (VM user and host scrubbed).
Every row is LIVE and on tmpfs (no `VOID-ONDISK`). The first three rows
carry `FLAG-IOSTALL` (io `full avg10` max 66.7, 38.0, 26.8 — the disk
was recovering); c1d s42 r1 is the lowest c1d row (494 Mbit/s), so the
flag is reported beside it, not voided.

*The split on current main* (server `[RXSEC] phase=xfer`, ns per message,
share of the handler; one message = one symbol):

| row | goodput | total | decode | deliv | ack | book | arr | probe | prune | other | `[TASK]` cores / busy |
|---|---|---|---|---|---|---|---|---|---|---|---|
| c1d s42 r1 † | 494 | 17 837 | 6 425 (36 %) | 1 697 | 2 702 | 1 798 | 441 | 744 | 499 | 3 531 (20 %) | 0.889 / 0.966 |
| c1d s7 r2 † | 555 | 15 918 | 5 729 (36 %) | 1 488 | 2 411 | 1 578 | 380 | 691 | 412 | 3 230 (20 %) | 0.900 / 0.967 |
| c1d s42 r3 | 577 | 15 155 | 5 332 (35 %) | 1 475 | 2 357 | 1 597 | 377 | 641 | 355 | 3 020 (20 %) | 0.878 / 0.959 |
| c1s s42 r1 † | 649 | 12 376 | 4 589 (37 %) | 1 249 | 1 260 | 910 | 379 | 629 | 385 | 2 974 (24 %) | 0.815 / 0.878 |
| c1s s7 r2 | 685 | 12 193 | 4 633 (38 %) | 1 282 | 1 221 | 885 | 371 | 618 | 374 | 2 809 (23 %) | 0.854 / 0.911 |
| c1s s42 r3 | 632 | 13 492 | 5 132 (38 %) | 1 436 | 1 353 | 949 | 397 | 648 | 487 | 3 091 (23 %) | 0.859 / 0.928 |

† `FLAG-IOSTALL`. Server `[RDIAG]` busy 95–100 % in every in-transfer
window at c1d; backlog p50 20–32, p99 1 536–1 792.

*Predictions.*
- **H-alloc (a): UNREAD** (no profile; above).
- **H-alloc (b): MET.** Bench: source `add_symbol` 3 553 ns at the 140 k
  cadence vs 1 656 ns at the 1 k cadence (2.15 ×, bar 1.5 ×); a k = 0
  repair 2 114 vs 1 828 ns.
- **H-burst: MET.** `prune_max_us` 63 196–82 892 at c1d (bar 5 000) and
  65 616–89 740 at c1s, in every row; bench `adv_max_us` 74 455 at the
  140 k cadence (1 477 at 1 k).
- **H-other: MISSED.** At c1d `arr + probe + prune` = 1.4–1.7 µs, 9–10 %
  of the handler (bar: half of R1's 26.4 %, i.e. ≥ 13.2 %); the new
  remainder is 20 % (bar ≤ 15 %).
- *Role cores* (share × `[TASK]` cores, c1d): decode 0.31–0.32 core; ack
  0.13–0.14; other 0.18; book, deliv, arr, probe, prune each ≤ 0.09.
  Decode alone sits at the 0.3 bar.

*What it does not show.* The bench measures `add_symbol` with no GF work
at all (an in-order, loss-free stream: every repair takes the k = 0 path)
and finds 1.7–3.6 µs per source. That is consistent with H-alloc but is
**not** the pre-registered witness (a), which reads the live c1d decode
under loss and reordering, where the GF share is unknown. It is not
substituted for it.

*Next* (the coordinator's call): a profile-only re-read of H-alloc (a), a
new amendment before VM contact — the same 1 GB c1d server `perf record`,
the attach target taken from `perf_rwm_c.sh`'s own `srv_pid` (not
`pgrep -o`), the same rule. The R3 build stays shelved until it reads.

### 22. Amendment 4 — the profile-only re-read of H-alloc (a) (before VM contact)

Operator go, 2026-10-27. **Only H-alloc (a) is re-read**; every other
§22 reading above stands and is not re-measured. **Unchanged:** the
prediction, its bars (gf256 ≤ 20 % of `add_symbol`'s children; allocator,
`memmove`/`memset`, BTree and hash frames ≥ 50 %), the decision rule, the
engine tree (`e4d078e`), the transfer (one 1 GB `c1d` invocation, seed 42,
bulk, `--window-reliable`, `RWM_GEN=0 RWM_DIAG=1`, no `RWM_RTOBS`) and the
recording (`perf record -g --call-graph dwarf,16384 -F 999`, 3 s, starting
3 s after the driver's start line), read off the VM with
`tools/l1/r3diag_fold.py` (the "under add_symbol: leaf classes" block).
**Changed:**
1. The attach target is the server pid `perf_rwm_c.sh` itself started
   (its abort-witness `srv_pid`, written to the run dir), not
   `pgrep -o -x raptorpath`.
2. **Fail-fast witness:** `perf report --sort comm` must show samples on
   the server's `rp-w-*` worker threads and `perf record` must exit 0;
   otherwise the session ends `ABORT-PERF-ATTACH` and (a) stays UNREAD.
3. Rule 1 for the read itself: the fold must find samples under
   `run_receiver` and under `UnifiedDecoder::add_symbol`; zero under
   either is `UNREAD`, not a verdict.
4. Envelope `tools/l1/r3prof_run.sh` (build on disk; the measurement and
   the perf data on `/dev/shm`; Amendments 2–3: the shared precondition,
   io as a covariate). Started by the detached on-VM waiter
   (`r3diag_wait.sh` with `R3D_SCRIPT=r3prof_run.sh`), which waits on the
   locks (another agent may hold them) and never touches one. Hard
   deadline launch + 2 h. Ledgers to `docs/l1-raw/thread-r3-diag/prof/`.

**Reading:** (a) **MET** iff gf256 ≤ 20 % **and** allocator + memmove/memset
+ BTree + hash ≥ 50 % of the samples under `add_symbol`; **REFUTED** iff
gf256 > 20 %; otherwise (gf256 ≤ 20 % but the bookkeeping classes < 50 %)
**PARTIAL**, which selects neither branch and is reported with the leaf
table. MET selects the in-place R3 (then its ship battery, after the
rebase and the harness amendments); REFUTED with decode ≥ 0.3 core (read
above: 0.31–0.32) selects the decode-stage task.

### 22. Amendment 4 — result

**H-alloc (a): `UNREAD` again (rule 3). No design is selected.** The attach
worked this time — `srv_pid` 971842 (`comm=raptorpath`), `perf record` rc 0,
7 567 samples, `PERF-ATTACH-OK` with samples on 6 `rp-w-*` threads — but the
pre-registered read found nothing to read: the fold saw **0 samples**.
Two harness defects, both mine, found only in the products:
1. `perf script -F tid,sym` prints no call chain (perf needs `ip` in the
   field list for it), so the folded file holds tid lines only.
2. The binary is `--release`, which carries no DWARF (`debug` is off in
   `[profile.release]`), so dwarf unwinding stops in user code: in the
   children view `run_receiver` reads 6.40 % children against 6.31 % self —
   its callees are not attributed to it, and `add_symbol` reads 0.82 % /
   0.71 %. Even with `ip` the fold would have found almost nothing under
   `add_symbol`. (§16 built with line tables for exactly this; §20 did not,
   and its children view shows the same flat shape.) The repo already has
   the fix: `[profile.release-prof]` = release + `debug = 1`, declared to
   produce the same `.text`.

Session 10:41:08–10:46:32Z (after a lock wait from 09:02Z), engine
`e4d078e`, harness `42161e4`, binary `aa09d965…` (the §22 rows' binary),
1 GB c1d 640 Mbit/s, run from `/dev/shm`, io `full avg10` ≤ 0.92 (no flag);
END 0 `raptorpath`, 0 `rp-*`, locks released. Ledgers:
`docs/l1-raw/thread-r3-diag/prof/` (scrubbed). `/home/VMUSER/r3prof` (no
target dir, < 1 MB) is left until the VM is free of the §29 session.

*Not a verdict, reported for the operator:* the flat self profile (process
wide, server) shows no `gf256` symbol at ≥ 0.3 % of samples, while the
decoder's bookkeeping symbols are visible: `seq_probe` 1.53 %,
`assimilate` 1.06 % self, `add_symbol` 0.71 %, `HashSet::insert` 0.89 %,
`hash_one` 0.85 %, two `BTreeMap::insert` 0.67 % + 0.59 %,
`hashbrown::insert` 0.64 %; the allocator family (`_int_malloc` 4.37,
`_int_free_chunk` 2.00, `malloc_consolidate` 1.58, `__libc_malloc2` 1.36,
`malloc` 1.28, `cfree` 1.02, `unlink_chunk` 1.44) ≈ 13 % and `memmove`
4.78 % of the process. `mul_acc_slice` may be inlined into `assimilate`,
and the allocator and `memmove` samples cannot be attributed to the
decoder without call chains, so this is **not** substituted for (a).

*Next* (the coordinator's call): a third read with both defects fixed and
nothing else changed — `cargo build --profile release-prof`, `.text`
section hash compared with the `--release` binary's and logged (a mismatch
is `ABORT-TEXT`), `perf script -F tid,ip,sym`, and a fail-fast fold check
on the VM (≥ 1 sample under `run_receiver` and under `add_symbol`, else
`ABORT-FOLD`), as Amendment 5.

### 22. Amendment 5 — the third read of H-alloc (a) (before VM contact)

Operator go, 2026-10-27; **the last attempt**: if this read is also
`UNREAD`, the report stops there. Amendment 4 stands (the reading rule,
bars, transfer, recording, attach to `srv_pid`, `ABORT-PERF-ATTACH`)
except:
1. **Binary:** `cargo build --profile release-prof` (release + `debug =
   1`, line tables). The `--release` binary is built beside it and the two
   `.text` sections' sha256 are logged; a mismatch is `ABORT-TEXT` (the
   profiled code must be the shipped code).
2. **Call chains:** `perf script -F tid,ip,sym --inline` (inlined frames
   named, so `add_symbol` inlined under LTO is still a frame).
3. **Dry check first:** a 400 MB c1d transfer with a 5 s record, folded
   on the VM; it must show samples under `run_receiver` **and** samples
   with frames below `run_receiver` (the fold's `receiver_nonleaf`, so
   unwinding into its callees works), else `ABORT-FOLD-DRY` before the
   read. The dry run is not read.
4. **The read's own fold check** on the VM: ≥ 1 sample under
   `run_receiver` and under `add_symbol`, else `ABORT-FOLD`.
5. Envelope `tools/l1/r3prof_run.sh` (this commit), the on-VM waiter
   after the §29 session (locks by `test -e`, never broken), shared
   precondition, tmpfs, hard deadline launch + 2 h. The old
   `/home/VMUSER/r3prof` is removed at reship. Ledgers to
   `docs/l1-raw/thread-r3-diag/prof2/`.

### 22. Amendment 5 — result; §22 closes

**H-alloc (a): `UNREAD` (`ABORT-TEXT`). Per the operator's limit there is
no fourth attempt, and the §22 decision rule selects no design: the R3
build (in-place decoder) stays shelved and no decode-stage task is
designed.** Session 11:29:25–11:40:11Z (engine `e4d078e`, harness
`bcdfc4e`), both builds rc 0 (release-prof 337 s, release 308 s), then
the `.text` check failed: sha256 `fbd88f46…` (release-prof) vs `9f50f56f…`
(release). Nothing was recorded; END 0 `raptorpath`, 0 `rp-*`, locks
released; the VM dir is removed. Ledgers: `docs/l1-raw/thread-r3-diag/prof2/`.

*What it says.* The `Cargo.toml` comment on `[profile.release-prof]` ("does
not alter instruction selection … expected to produce the same `.text`")
is false on this tree: `debug = 1` changes the release `.text`. So a
release-prof profile is not a profile of the shipped binary, and the
release binary has no DWARF for user-frame attribution (Amendment 4).
Neither of this section's two read paths can attribute the receiver's
time to `add_symbol`'s callees on the shipped code. A future read needs a
different method (e.g. `RWM_RTOBS=2` sub-sections inside `add_symbol`,
measured on the release binary itself), which is a new pre-registration,
not a §22 attempt. The `Cargo.toml` claim should be corrected (recorded
here, not changed: outside this task's scope).

*§22 in sum.* Read: the c1d receiver split on current main (decode 35–36 %
of 15–18 µs per message, ≈ 0.31–0.32 core; ack 0.13–0.14; other 0.18),
H-alloc (b) MET (2.15×), H-burst MET (the 2 s prune is a 63–90 ms stall),
H-other MISSED; H-alloc (a) — the deciding witness — UNREAD three times.
Not built or shipped: the R3 decoder change (`2d59e62`, `0702593`), its
ship-battery pre-registration and harness (`0d4e910`, `6050e3c`, `c5b04eb`,
Amendment 1 of the battery) stay on the branch, shelved.

> Coordinator note: the R3 code, its harness and the battery pre-registration are not on main; they are kept at tag `archive/thread-r3`.
