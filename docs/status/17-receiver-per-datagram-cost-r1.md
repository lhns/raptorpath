## 17. Receiver per-datagram cost (R1) — pre-registration

Phase R1 follows §16's finding 4: **the c1d ceiling is the server's single
receiver task** (0.85 core, busy 0.95, `[RDIAG]` 99–100 %, 6.6 ms CPU per
poll, a flat profile with the glibc malloc family at 13–15 %). R1 makes the
receiver cheaper per datagram without changing what it does: four allocation
and per-seq-structure fixes, **no wire change, no law change, identical at
every N, δ and ρ** (no hint, threshold or preset selects anything here). A
step-0 instrument says where the receiver's CPU goes. Committed before any
battery invocation (the dev builds and the red/green runs on the VM preceded
it; they are tests, not results). No number below is a result. Nothing is
flipped by this battery; shipping is the operator's merge.

**What R1 changes** (branch from `main` ce60079; commits `f75f1fc` and
`26b3204` the red tests (`f75f1fc` is the allocation test with its bound still
∞, the measuring commit), `4ee19bf` the change, `ae54b9f` the harness,
`795087c` and `2a0a119` the allocation bound (24.0, then 23.5 from the
repeats below), `cac7190` the wire-law and `[LAT]`-store differential tests,
`a402054` a test fix (below), `df99301` the `[RXSEC]` clock source).

0. **Step 0 — the instrument, opt-in under `RWM_RTOBS=2`** (never set in the
   battery: its per-section clock reads would perturb the task under test).
   `[RXSEC]` (one line per receiver per run window, `task_obs::RxProbe`):
   the receiver loop's handler time per message split into `decode` (the
   window decoder's `add_symbol`), `deliv` (the reorder buffer and the
   hand-off to the consumer), `ack` (the WindowAck / per-batch Ack build and
   staging, the SACK ranges, the loss feed), `book` (the always-fed `[ETA]` /
   `[LAT]` / `[SUCC]` / `[LATE]` bookkeeping) and `other` (the message total
   minus those), in µs and ns per message; the receiver's backlog (messages
   queued at each `recv_many` return) and, on `[IOWN]` (`cmdq_*`), the owner
   command channel's depth at each drain (the drained commands plus what
   queued behind them), each as n / p50 / p99 / max (quarter-octave buckets;
   max is run-cumulative). **Declared deviation from "per-section CPU":** the
   sections are read off the monotonic vDSO clock (~25 ns), not
   `CLOCK_THREAD_CPUTIME_ID`. The first build used the thread CPU clock — a
   real syscall (~0.7 µs on the VM, ~20 reads per message) — and the first c1d
   read ran at 233 Mbit/s with the receiver 100 % busy at 36.9 µs per message
   against ~700 uninstrumented: the gauge was most of what it measured (`df99301`).
   A section has no `.await`, so on a core-bound task its wall is its CPU;
   preemption inside it is `ivcsw` on the `[TASK]` line. The line's
   `busy=` is the handler time over the window wall; the receiver's other time
   (loop head, `recv_many`) is `[TASK] task=receiver` cpu minus `total_us`.
1. **(a) The inbound `Vec` is recycled.** The owner forwarded one fresh
   `Vec` per poll (`mem::take`, grown from zero capacity) and the receiver
   dropped it. The receiver now hands each drained `Vec` back to its path's
   `PathView` (a pool of at most 64, following `TxBatch`'s pool) and the
   owner takes the next poll's from it (`take_inbound` / `recycle_inbound`;
   `ibp_hit` / `ibp_miss` on `[IOWN]`, always counted).
2. **(b) The per-message `Vec`s go.** The `lat_arrivals` `Vec` is not
   replaced by a scratch `Vec`: the batch is still intact at its one use, so
   the source symbols are iterated in place (a deviation from the brief's
   wording, strictly simpler: no scratch `Vec` to keep). **(e) added, same site class:** the per-batch
   `Ack`'s `received_ids` `Vec` was collected on every Data message and the
   whole message dropped under the ack merge (the default); it is now built
   only when the `Ack` is sent.
3. **(c) `received_seqs` is a ring bitmap** (`net/seq_bits.rs` `SeqBits`,
   behind a `ReceivedSeqs` trait that `received_sack_ranges` and
   `ReceivedAbove::sync` are now generic over; `BTreeSet<u64>` implements it
   and is the test reference). Anchored at a word-aligned base at or below
   the prune floor: `insert` / `contains` O(1), `count_in` a popcount, the
   SACK run list a bit scan, `prune_below` pops whole words. **Declared
   behaviour the `BTreeSet` did not have, both bounded by a test:** an insert
   *below the prune floor* (a late copy of a long-delivered seq) answers
   "not newly" instead of re-inserting until the next prune — nothing the
   receiver reads lies below the cumulative point; an insert *beyond the
   span cap* (16 M seqs from the base, 2 MiB; orders beyond the sender's
   store cap) is refused and counted (`SeqBits::refused`). The degradation
   is one-sided: the set may *under*-claim (a seq that did arrive is not
   SACK-reported and may be resent) and **never claims a seq that was not
   inserted**. Next expected, `received_above` and the SACK ranges are
   otherwise exactly the `BTreeSet`'s (wire v9 honesty).
4. **(d) `[LAT]`'s pending store** is a `SeqRing` instead of a `HashMap`
   (the brief said `BTreeMap`; the real per-seq store at `net/lat.rs` was a
   `HashMap<u64,(u32,u64)>` plus an eviction FIFO). The FIFO and its
   `PENDING_MAX` bound are unchanged; a span guard (4 × `PENDING_MAX`) refuses
   a stray copy of a long-delivered seq, or sheds the oldest records on a
   jump, counted in `over=` exactly as an evicted record is.

**Red / green (dev, VM, release unless said; inside both locks).**

- *Allocation bound* (`tests/recv_alloc_bound.rs`: the in-process loopback,
  2 MB vs 12 MB objects differenced, counting global allocator, process-wide;
  public API only, so the same file runs on both trees). **Red on ce60079:**
  25.23 allocs per source symbol, `FAILED` against the bound 23.5; over 17
  runs ce60079 reads 24.998–25.337. **Green on R1:** 22.70 `ok`; over 16 runs
  22.701–22.940 (spread: ce60079 0.34, R1 0.24; alternating runs in one
  session). The bound sits 0.56 above R1's worst and 1.5 below ce60079's best.
  It is a regression gate for the *set* (the change is worth 2.4 per symbol);
  one fix alone (0.4–1) may slip under it.
- *Pool* (`tests/inbound_pool_reachability.rs`: the shipped binary on both
  perf ends, `[IOWN] phase=run`). **Red on ce60079:** `FAILED` (no `ibp_hit=`
  token). **Green on R1:** `ok` at N = 1 and 2 (≥ 90 % of the forwards served
  from the pool, misses ≤ 2 × 64 per path); the c1d dev read below: 9 447 hit /
  59 miss and 8 583 / 54 on the server's two owners (99.4 %).
- *The ring tests* (`net::seq_bits::tests`, `net::tests::the_wire_laws_read_
  the_ring_bitmap_as_the_btreeset`, `net::lat::tests`): the types do not exist
  on ce60079, so they are red by construction there; their teeth are shown by
  **mutation** on the R1 tree, each mutation making its test FAIL and the file
  restored: M1 `prune_below` mask off by one → 3 of 4 `seq_bits` tests FAIL;
  M2 SACK run end off by one → 2 of 4 FAIL; M3 an evicted `[LAT]` record left
  in the ring → `the_ring_store_matches_the_hash_map_it_replaced` FAILS (the
  absolute records-held assertion); M4 `insert` always answering "newly" → 3
  of 4 FAIL. They assert: ring = `BTreeSet` on `insert`'s answer, membership,
  the frontier walk, `count_in`, the uncapped and capped SACK run lists,
  `received_above` against brute force and the capped `received_sack_ranges`
  (a prefix of the true runs; the script reaches `MAX_SACK_RANGES` on every
  seed) over random arrivals with gaps, duplicates, late copies below the
  floor, hold-expiry jumps and prunes; beyond the span: refused, counted,
  **never a member that was not inserted**, equal to the reference on what it
  kept; `[LAT]`: ring = the replaced `HashMap` + FIFO record for record and
  in records held, lost-to-bound counts equal, and a stray far from the live
  span counted instead of padded.
- *Suites, R1 tree `cac7190`, inside the locks:* release `cargo test -p
  raptorpath -p raptorpath-math --release --no-fail-fast -- --test-threads=2`:
  **1 095 passed, 1 failed**, 50 ignored (95 binaries); debug `cargo test -p
  raptorpath --no-fail-fast -- --test-threads=2`: **954 passed, 1 failed**, 49
  ignored (86 binaries), `IDENTITY-PANICS 0`; doc rc 0; `cargo test -p
  raptorpath-wasm` (35 passed, rc 0); the parser tests (`test_l1common`,
  `test_stage3_parse`, `test_threadq1_parse`, `test_threadq2_parse`,
  `test_threadr1_parse` 34 checks) rc 0. The one failure in both profiles,
  `net::tests::q2_one_task_per_loop_no_multiplexer`, **is red on ce60079
  itself** (run there: `FAILED`): Q3 step 1 (`8311094`) wrapped each task spawn
  in `task_obs::timed`, and the source-scan test still looked for the bare
  `tokio::spawn(receiver::run_receiver(`; the session that added it ran 26
  selected lib tests. `a402054` updates its four patterns (the assertion — one
  spawn per loop — is unchanged). On the R1 tree `df99301`/`2a0a119`: lib
  release **556 passed, 0 failed**, 8 ignored; lib debug **557 passed, 0
  failed**, 8 ignored. Named, release and debug both `ok`:
  `reliable_delivery_hold` (delivery-hold), `lock_order_loopback` (owner
  identity witness), `ack_delivery`, `ack_merge_loopback`, `io_owner_topology`,
  `inbound_pool_reachability`, `recv_alloc_bound`, and the 55 loss-truth /
  SACK / `received_above` / ring tests of the lib. The later commits
  (`a402054`, `df99301`, `2a0a119`) are test-only or instrument-only and were
  covered by the lib runs above, not by a repeat of all 95 integration
  binaries; the envelope re-runs every suite from scratch before the battery.
- Windows host `cargo check -p raptorpath --all-targets` rc 0 on the final tree
  (no new warning in an R1 file).

*Dev read, unscored, n = 1* (`RWM_RTOBS=2 RWM_DIAG=1 RWM_RDIAG=1`, c1d-400, seed
42, R1 binary, both locks; an instrument run, not a result). `[RXSEC]` of the
server's receiver over the transfer window (6.09 s): 340 042 messages,
16.7 µs per message (handler busy 0.933 of the window; `[TASK]` receiver cpu
0.888 core, `[RDIAG]` 100 %): **decode 34.8 %** (5.8 µs), deliv 9.0 %
(1.5 µs), ack 14.0 % (2.3 µs), book 15.8 % (2.6 µs), other 26.4 % (4.4 µs).
Receiver backlog at each `recv_many` return p50 256 / p99 1 536 / max 1 671
messages; owner command depth p50 1 / p99 6 / max 10. Goodput 525 Mbit/s with
the deep gauges on (the owners' quinn-section clocks, `[TASK]`, `[QSOCK]` are
not free; not comparable with the scored arms).

**Mechanism and predictions** (rule 11; derivation, then the named risks).

- *What is removed.* Per Data message on the receiver task: two `Vec`
  allocations (`lat_arrivals`, `received_ids`), one `BTreeSet` node per
  source symbol and its tree walks (`insert`, the SACK `range`, the
  `received_above` `range` at every frontier move), one `HashMap` insert /
  remove with SipHash per source symbol (`[LAT]`), and per owner poll the
  four growth reallocations of a fresh inbound `Vec` plus its free on the
  receiver. The process-wide count on the loopback falls from ≈ 25.2 to ≈ 22.8
  allocations per source symbol (sender + receiver + quinn), the removed ones
  all on the receiver. What the receiver still allocates per datagram (the decoder's
  `recovered` `Vec`, the `deliverable` `Vec`, `extract_window_packets`, the
  SACK `Vec`, the serialized ack) is untouched and is the next step's.
- **c1d — the receiver-bound cell. Named prediction: goodput BETTER** (R1's
  median beyond MAIN's by more than the §5 MDE, ranges disjoint) **with the
  server receiver's `[RDIAG]` busy median below 90 %** (MAIN 94–95 % in §16).
  Read as MET only if both hold; PARTIAL if one does.
- **CPUSRV per byte BETTER at c1s and c1d** (the server's CPU is the
  receiver's work plus the owner's reads, which are unchanged; §16's malloc
  family 13–15 % is the bulk of what is removed). MET if BETTER at both,
  PARTIAL if both are at least TREND-BETTER.
- **c1s: goodput WITHIN or better — and it is capped by something R1 does not
  touch.** At N = 1 the outstanding store cap is the constant
  `RELIABLE_STORE_MAX` = 1024 (§16 finding 3: the pooled law returns `None`
  below two live paths), so c1s goodput ≈ 1024 symbols / RTT and the sender
  is paused 20–33 % of the time; the receiver at c1s was busy 0.78–0.84, not
  saturated. R1 can lower c1s CPU but is not predicted to raise its goodput.
  A c1s goodput gain would be a surprise to report, not a prediction.
- **c2 / c8 (ack-clocked, link-bound): WITHIN** on goodput and both CPU
  per byte (every task ≤ 0.31 core in §16).
- **CPUCLI**: WITHIN (the client's receiver handles only acks; its pool
  hits are on the ack path), **RTprop floor** and **`[LAG]` p99**: WITHIN or
  better (less receiver backlog is less queueing in the RTT sample).
- *Named risks.* (i) The ring bitmap's range reads cost O(words), not
  O(log n): a long-stalled hole with a large span is the case; the cap on
  the span (16 M seqs) bounds it and the store cap bounds the real span to
  thousands of seqs. (ii) The pool's lock is taken twice per owner poll
  (take, recycle): the owner already takes `TxBatch`'s pool lock per flush.

**Binaries.** R1 = this section's commit, MAIN = `main` ce60079, each
archived with `git -c core.autocrlf=false -c core.eol=lf archive` and built
fresh on the benchmark VM in its own fresh target dir; copied under the real
name `raptorpath`; `sha256` in `BINSHA.txt`, re-verified before every
invocation. Both resolve `RWM_RTOBS` and `RWM_RDIAG` as opt-ins; every arm
runs with `RWM_RTOBS=1 RWM_RDIAG=1` (and `RWM_DIAG=1`).

**Harness.** Envelope `tools/l1/threadr1_run_all.sh` (both locks for the whole
session via `lib_battery.sh`; R1 build → tests → MAIN build → smoke → budget
→ battery → score; hard backstop), driver `threadr1_battery.sh`, scorer
`threadr1_parse.py` (rows by `stage3_parse.make_row`, helpers from
`threadp1_parse` / `threadq1_parse`; offline test `test_threadr1_parse.py`,
34 checks). Derived from `threadq2_*`. No operator GO gate once the operator
has said go: SMOKE-PASS proceeds.

**Tests first** (inside the locks, on the R1 tree): `cargo build --release`;
`cargo test -p raptorpath -p raptorpath-math --release --no-fail-fast --
--test-threads=2`; `cargo test --doc -p raptorpath --release`; `cargo test -p
raptorpath-wasm` (`GOLDEN_CAPTURE` unset); the debug run `cargo test -p
raptorpath --no-fail-fast -- --test-threads=2` (`IDENTITY-PANICS` = count of
`io owner: quinn seam`, must be 0); the python parser tests. A failure that
passes on an immediate solo re-run is `FLAKE`; any other is `ABORT-TESTS`.

**Cells** (§5's geometry, size, capacity and > 5 % headroom, as §11–§16):
`c1s-400`, `c1d-400`, `c2-100`, `c8-100`. **Arms**: `MAIN`, `R1`, both bulk,
`--window-reliable`, the shipped defaults (every arm `env -u RWM_EST_CADENCE
RWM_POOL_ANCHOR RWM_EMIT_BATCH RWM_EMIT_BURST RWM_RTOBS RWM_RDIAG RWM_ACKDIAG
RWM_IO_RT`, then `RWM_RTOBS=1 RWM_RDIAG=1` on every arm; rule 15d),
`perf_rwm_c.sh` with `RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=150
SEED=<seed>`, one run, a fresh topology. **Plan per (rep, seed) block: 8
invocations**, cells in the order above, arm order within a cell rotated by
the block index (rule 3). Seeds 42 and 7; blocks rep 1 s42, rep 1 s7, rep 2
s42, …; **n = 3 per seed (6 per arm and cell)**, interleaved, cut only by the
budget rule.

**Witnesses per invocation** (a failing row is `CONTAMINATED` /
`WITNESS-FAIL`, excluded and counted; no client summary = `NO_DATA`):
`stage3_parse`'s set (`[PIPE]` `window/Rlc/bulk` both ends; `[GATES]` both;
the RLC line both; no generation guard; cadence ACTIVE both; `RWM_POOL_ANCHOR=0`
both); `[GATES] RWM_EMIT_BATCH=1` both and the `emission batching ACTIVE` echo
on the client; the client's `[DIAG]` `wake[` token; `[GATES] RWM_RTOBS=1` and
`RWM_RDIAG=1` both; at least one in-transfer `[RDIAG]` line on the server;
the main runtime's `[THR]`/`[LAG]` window of the measured object and its
`[IOWN]` lines on both ends (client `run=1`, server `obj=1`); the Q2
sender-lane witness on **both** arms (ce60079 contains Q2: `ack_batches=` on
every owner line, the client's `ack_dg` > 0, `cmd=` in the `wake[` token, no
`[TOPO]` / `RWM_IO_RT`); **the R1 execution witness, two-sided**: on every R1
row every `[IOWN]` line of the window carries `ibp_hit=` on both ends and the
server's owners reused pooled `Vec`s (`ibp_hit` > 0 summed); on every MAIN row
no `ibp_hit=` token; the row's `sha256` is its arm's binary.

**Pass rule per cell** (§14's, R1 against MAIN): a clause is **WORSE** iff
R1's median is beyond MAIN's median·(1 ∓ rel) in the worse direction **and**
the two arms' [min, max] ranges are disjoint in that direction (TREND-WORSE =
beyond the band with overlapping ranges: reported, not a fail):

| clause | direction | rel |
|---|---|---|
| goodput ≥ MAIN | higher is better | §5 MDE: c1s 4.9 %, c1d 5.6 %, c2 1.4 %, c8 4.0 % |
| client CPU per byte ≤ MAIN + MDE | lower | §5 CPUCLI MDE: 2.4 / 6.5 / 6.3 / 11.6 % |
| server CPU per byte ≤ MAIN + MDE | lower | the same, as a declared transfer (§11–§14) |
| RTprop floor ≤ MAIN + tolerance, per leg | lower | max(5 %, MAIN's half-range / MAIN's median) |
| `[LAG]` p99 ≤ MAIN + tolerance, client and server | lower | the same rule |
| fed loss = `[TRUTH]` | — | per leg med(`plc`/`[TRUTH]`) within [1/1.3, 1.3] × MAIN's |
| DNF | — | excess > 0.20 (§5) |

**Per-cell verdict**: **FAIL** iff any clause is WORSE, the feed moved or the
DNF excess fired; **UNSCOREABLE** iff R1 or MAIN has < 3 live rows or ≥ 2
witness-failed rows at the cell, a feed ratio is unread, or an abort cause
fired; **PASS** otherwise. Per arm: **PASS-EVERYWHERE**, **FAIL-AT-<cells>**,
or **UNSCOREABLE-AT-<cells>** (a FAIL outranks an UNSCOREABLE).

**Outcomes, in precedence order** (one verdict):
1. `UNSCOREABLE` — an abort cause fired.
2. `DELIVERED` — R1 passes at every cell: **ships**.
3. `REFUTED-WITH-RECORD (FAIL-AT-<cells>)` — R1 fails somewhere: nothing
   ships; the section split and the per-clause readings are the record.
   **No tuning in this battery** (no re-run with a changed pool size, span
   cap or bound; a follow-up is a new pre-registration).
4. `UNSCOREABLE-AT (…)` — R1 is not fully scoreable and fails nowhere.

The four predictions above are **reported, named** (`PREDICTION` lines:
`c1d_goodput_better`, `cpusrv_better`, `c1s_goodput_within`,
`c2_c8_within`); they do not decide the outcome.

**Reported, not gated** (per cell and arm, median [min–max], n): the
client's `[DIAG]` sender `busy`; the `[RDIAG]` busy and `q_avg` on both ends;
the server's pool `ibp_hit` / `ibp_miss`; `wake[..]`; the owner hops'
datagrams per batch, `rx_capped`, `send_err`, the lock-wait gauge; GSO factor
per leg; acks per data datagram; `[THR]` (process cores, the hottest threads
with `comm`, the main thread); parks per second.

**Abort causes, in priority order**: `ABORT-LOCK`, `ABORT-CRLF`,
`ABORT-BUILD` (either tree), `ABORT-TESTS`, `ABORT-SHA`,
`ABORT-SENTINEL-UNWRITABLE`, `ABORT-SMOKE` (one invocation per arm at
`c1s-400` and `c8-100`, seed 42: every row LIVE with goodput, both CPU
lines, `busy`, `[LAG]` p99 on both ends, the owners on both ends, the
server's `[RDIAG]`, per leg `[TRUTH]`, `plc` and an RTprop floor, the
client's `ack_dg`, and on R1 rows the server's `ibp_hit`; both arms present;
nothing in it is a result), `ABORT-BUDGET` (n < 2 per seed after the budget
rule), `ABORT-RC` (that row `VOID-RC`, the battery goes on), `ABORT-BRINGUP`
(no summary after 2 attempts: `NO_DATA`); void class `VOID-COTENANT` (a
`cargo`/`rustc` process before or after an invocation).

**Budget.** Hard backstop = launch + 3 h, soft = hard − 10 min; the 5 h cap
is not approached. Priors (§14's measured session): R1 build ≈ 4–5 min,
release + doc + wasm + debug tests ≈ 45 min, MAIN build ≈ 5 min,
`R_PRIOR` = 240 s per 8-invocation block (≈ 9.5 s per invocation measured,
≈ 76 s per block); scored battery 6 blocks ≈ 8–24 min; smoke ≈ 1–2 min.
**Expected session wall ≈ 1 h 05 min – 1 h 30 min.** n per seed = min(3,
⌊(soft − now) / (2·R_est)⌋); the battery starts no block that would cross
soft (`TRUNCATED-AT-REP-BOUNDARY`, scored at the n reached).

**Session rules.** Both locks for the whole session (the envelope's
`take_lock` *refuses* when a lock is held — it does not wait — so it is
launched only when both are free; the benchmark VM is co-tenanted and the
expected wall below excludes any wait for the locks, which the dev work for
this phase measured at several hours); detached envelope;
earned sentinels (`DONE-ALL` only with `TR1-BATTERY-DONE`, `check` rc 0 and
no truncation); the operator reads `all-era.txt` at most once per ≈ 20 min
(rule 13); `pkill -x raptorpath` only; no `ens18`, firewall, `sshd` or
non-`rp-*` namespace is touched; exit state verified (0 `raptorpath`, 0
`rp-*` namespaces, both locks released). Ledgers are copied to
`docs/l1-raw/thread-r1/`.

**Amendment 1** (committed before launch; no scored result exists or was
read). Three clarifications; the pass rule, the predictions and the outcome
rule above are unchanged.

1. **The dev read is not comparable with the scored arms, and the one MAIN
   figure there is says it may not be good news.** §16 measured MAIN at c1d at
   the same gauge level (`RWM_DIAG=1 RWM_RTOBS=2`, n = 3): 688–729 Mbit/s. The
   R1 dev read above (the same gauges, plus `[RXSEC]` at ≈ 0.5 µs per message
   and `RWM_RDIAG=1`) is **525 Mbit/s** with the server receiver still
   saturated (`[RDIAG]` 100 %, `[TASK]` busy 0.962, 0.888 core). It is one
   run, taken while the VM was co-tenanted (load average 5–10 over the
   session), so it may be load and not R1 — but it is below §16's MAIN range,
   and the `c1d_goodput_better` clause "receiver `[RDIAG]` busy median < 90 %"
   looks unlikely from it. **The battery decides** (interleaved, same session,
   n = 6): a c1d goodput WORSE is a c1d FAIL and the outcome `REFUTED-WITH-
   RECORD`, exactly as pre-registered; nothing is tuned.
2. **There is no MAIN `[RXSEC]` read** (ce60079 has no such instrument), so the
   section split above (decode 35 %, other 26 %, book 16 %, ack 14 %, deliv
   9 %) is a profile of R1, not a before/after. The drop from 36.9 µs to
   16.7 µs per message between the first and the second dev read is the clock
   fix (`df99301`), not R1.
3. **Duration: two estimates, one decision for the operator.** The Budget
   figure above (≈ 1 h 05 min – 1 h 30 min) is §14's, from a quiet VM. In this
   phase's dev session, under co-tenant load, the same steps took: R1 build
   2 132 s, release suite 2 158 s, debug suite 2 651 s — ≈ 1 h 56 min before
   the MAIN build, the smoke and the battery. If the VM is as loaded at
   launch, the session is ≈ 2 h 30 min – 3 h+, the 3 h hard backstop
   (`soft = hard − 10 min`) can truncate the battery at a rep boundary
   (`TRUNCATED-AT-REP-BOUNDARY`, scored at the n reached) or leave it no room
   to start. The options, not chosen here: launch only when `/proc/loadavg` is
   quiet and both locks are free; or pre-register **hard = launch + 4 h**
   (inside the 5 h cap) by a one-line amendment before launch.

**Amendment 2** (committed before launch; no scored result exists or was
read; the operator said go). (i) **Hard deadline = launch + 4 h** (soft = hard
- 10 min; inside the 5 h cap), replacing the 3 h of the Budget paragraph.
(ii) **Launch precondition:** both VM locks free, `/proc/loadavg` 1-minute
value < 2.0, and no `cargo` / `rustc` process (`pgrep -x`, never `-f`). The
loadavg at launch is in the ledger (`all-era.txt`, `TR1-ALL start ... load=`).
If the precondition does not hold within 1 h of the locks freeing, the run is
reported, not launched. The other agent's locks are never broken or taken
over; they are polled by lock-file checks only. (iii) The dev read's c1d
goodput (525 Mbit/s) coincided with another agent's c1d reading 547 against
its own earlier 678-755, both while building on the VM: it points at
co-tenant load rather than R1; scoring is strictly as pre-registered, the
interleaved MAIN arm absorbs it. (iv) `SeqBits::refused()` (inserts refused
past the 16 M-seq span) is counted but **not printed** by any gauge in this
tree, so the battery cannot report it; the span bound is covered by the
`seq_bits` test only.

### 17. Receiver per-datagram cost (R1) — result

**Outcome: `REFUTED-WITH-RECORD (R1 FAIL-AT-c1s-400): nothing ships.** Scored
strictly as pre-registered (Amendments 1 and 2 applied: hard = launch + 4 h;
launch precondition met: both locks free, loadavg 1.09, 0 cargo / rustc).
Session 2026-10-26 08:05–09:37Z, one envelope, both locks held throughout,
exit state 0 `raptorpath`, 0 `rp-*`, locks released. Tests first, on the R1
tree `583b712` inside the locks: release **1 096 passed, 0 failed** (50
ignored, 95 binaries), doc rc 0, wasm 35 passed, debug **955 passed, 0 failed**
(49 ignored, 86 binaries), parsers rc 0; smoke passed; the battery ran in 550 s
(48 rows, **all 48 LIVE**, no void, no witness failure, no abort token, no
truncation, n = 6 per arm and cell, `RWM_RTOBS=1 RWM_RDIAG=1`). Ledgers:
`docs/l1-raw/thread-r1/` (`all-era.txt`, `tr1.log`, `score.txt`, `TESTS.txt`,
`BINSHA.txt`, `diag-tr1/` per-run logs).

| cell | verdict | clauses |
|---|---|---|
| c1s-400 | **FAIL** | goodput **WORSE** 189 [185–192] vs 621 [344–658] Mbit/s (−69.6 %); CPUCLI/GB **WORSE** 22.5 vs 15.9 (+41 %); CPUSRV/GB **WORSE** 56.9 vs 22.8 (+149 %); `[LAG]` p99 and RTprop floor WITHIN; feed SAME |
| c1d-400 | PASS | goodput TREND-WORSE 572 [546–597] vs 609 [560–641] (−6.2 %, ranges overlap); CPUCLI WITHIN (+4.3 %); CPUSRV WITHIN (+6.2 %); server `[LAG]` p99 TREND-BETTER (−26 %) |
| c2-100 | PASS | goodput WITHIN (+0.1 %); CPUCLI WITHIN; CPUSRV TREND-WORSE (+9.6 %) |
| c8-100 | PASS | goodput TREND-BETTER (+11.6 %); CPUSRV **BETTER** (−11.9 %); client `[LAG]` p99 TREND-BETTER (−47 %) |

**Predictions (reported, not gating): all four MISSED.**
`c1d_goodput_better` MISSED (goodput TREND-WORSE; server `[RDIAG]` busy median
100 on both arms, not < 90); `cpusrv_better` MISSED (c1s WORSE, c1d WITHIN);
`c1s_goodput_within` MISSED (WORSE); `c2_c8_within` MISSED on the letter (c2
CPUSRV TREND-WORSE; c8 BETTER is on the good side).

**What the record says.**
1. **c1s is a deterministic collapse, not noise.** All six R1 rows read
   185–192 Mbit/s (client sender `busy` 12 vs 39.5; the server's receiver
   `[RDIAG]` 100 % vs MAIN's 85 %). In the server log of one pair
   (s42 r1) the receiver handles 13–21 k messages/s at 100 % busy in R1 against
   81 k at 77 % in MAIN: roughly **5× the CPU per message at N = 1**. The same
   code at c1d (two paths) costs MAIN's. MAIN's c1s is bimodal (one of six rows
   at 344, the rest 611–658), which does not rescue the comparison (R1's best
   row is below MAIN's worst).
2. **The change that was supposed to lower the receiver's cost raised it at
   N = 1 only.** The unmeasured cause lies in what R1 changed on the
   single-path receiver: the ring bitmap's range reads (`count_in`,
   `each_run`) on a stalled frontier, the `[LAT]` `SeqRing` store and its span
   guard, or the pool (c1s R1 `ibp_miss` 912–1 895 per run vs c1d 145–552, a
   consequence of the saturated receiver, not a cause). **No `[RXSEC]` was
   taken in the battery and no profile of the c1s receiver exists**; these are
   hypotheses to be separated by a `RWM_RTOBS=2` c1s run of both trees and a
   profile, as a new pre-registration, not tuned here.
3. **c8 and the c1d server `[LAG]` improved** (CPUSRV −11.9 %, client lag
   −47 %), the direction the mechanism predicts; c1d goodput did not rise and
   the receiver stayed saturated (100 %) on both arms, so the receiver-bound
   claim of §16 finding 4 is untouched by R1's allocation work.
4. The dev read's low c1d goodput (525 Mbit/s) was co-tenant load: the
   interleaved MAIN arm read 609 at c1d.
5. `SeqBits::refused()` (inserts refused past the 16 M-seq span) is counted but
   not printed by any gauge, so the battery cannot say whether it fired.
