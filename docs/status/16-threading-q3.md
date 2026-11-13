## 16. Threading Q3 — the investigation (step 1) — pre-registration

Plan v2's Q3 is conditional: a lock-free send under the owner (or a
receiver-role split) is built only if `[THR]` after Q2 shows a single owner
at ≥ 0.9 core, or quinn's in-lock `sendmsg` costs more than 10 % of an
owner. The trigger read so far is the `[IOWN]` asleep gauge (the owner's
wall asleep in its quinn sections: c1s ≈ 0.15–0.19, c1d ≈ 0.05–0.09 in the
§14 Q2 rows). Two facts weaken that reading before any new measurement:
(i) the gauge is wall − thread CPU, so it cannot tell a mutex wait from
preemption; (ii) §13 already ran the controlled comparison — the `own`
placement cut the owner's asleep to ≈ 0.01 and gained no goodput (c1s
778.3 vs 773.5, c1d 711.5 vs 729.5). And `[THR] os` cannot read "a single
owner's core": tasks migrate across the six workers (every Q2 worker reads
0.18–0.47 core). Committed before VM contact; this is a diagnostic
session, not a scored battery, and nothing ships from it.

**The instrument** (`RWM_RTOBS=2`, a new level of the existing opt-in
observer; `RWM_RTOBS=0/1` behave as before — `src/task_obs.rs`,
measurement only, every wrapper delegates):

1. `[TASK]` — per-task poll CPU: the sender, the receiver, each path's
   owner, the perf body, the cold tasks, and quinn's EndpointDriver /
   ConnectionDriver per path (timed through a delegating `quinn::Runtime`;
   the ConnectionDriver's poll holds the connection mutex for its whole
   length, quinn `connection.rs:243`, so its poll wall is the lock's hold
   time). Per poll, `getrusage(RUSAGE_THREAD)` splits wall − CPU into
   `asl_vol` (a voluntary switch: a futex sleep — the mutex), `asl_inv`
   (preempted only) and `asl_none` (no switch counted: steal /
   granularity).
2. `[QSOCK]` — the path socket timed (a delegating `AsyncUdpSocket`):
   `try_send` = the driver's in-lock `sendmsg` (calls, µs, datagrams →
   GSO), `poll_recv` = the EndpointDriver's `recvmmsg`.
3. `[IOWN]` appended tokens — the owner's quinn sections by kind (`qk_snd`
   the command drain, `qk_rd` the reads, `qk_pub` the view publish) with
   the same switch-class split; the back-pressure witness: producer batches
   that found the owner's channel full and their wait (`fl_full`,
   `fl_full_us`), and inbound batches that found the receiver's / sender's
   channel full (`rx_full`, `ack_full`, and the wall until forwarded).
4. Harness side (`tools/l1/q3diag_run.sh`): per-CPU `/proc/stat` at 10 Hz
   around each invocation (user / system / softirq / idle / steal per
   vCPU), and one system-wide `perf record -g` (dwarf) at c1s and c1d.

**The session.** One instrumented binary, both locks, seed 42: c1s and c1d
× 3 with `RWM_DIAG=1 RWM_RTOBS=2` (battery-like); × 2 with `RWM_RTOBS=2`
and no `RWM_DIAG` (the shipped send path: `RWM_DIAG` makes `send_one` take
the connection lock twice per datagram); × 2 with `RWM_DIAG=1
RWM_RTOBS=1` (the deep gauge's own cost on goodput); c2 and c8 × 1; the
two `perf record` runs (`RWM_RTOBS=1`).

**Decision rule (fixed now).** Build the lock-free send (cheap quinn-API
fixes first, then R6 option B) only if at least one holds at c1s or c1d:
(a) the producers are back-pressured by the owner — `fl_full_us` ≥ 5 % of
the sender's wall — or the owner task itself runs ≥ 0.9 core (`[TASK]`
`cores`); (b) a ConnectionDriver runs ≥ 0.9 core, or its in-lock
`sendmsg` (`[QSOCK]` `send_frac`) is ≥ 0.10 of wall while a vCPU it needs
is saturated. Otherwise the owner's asleep is absorbed slack — the lock is
not on the critical path — and no lock-free send is built: the report
names the measured ceiling (the task or vCPU at saturation, the kernel /
netem, or the engine's own rate law) and designs, without building, the
next fix. The asleep split itself is reported: if `asl_vol` (the mutex)
is under half of the owner's asleep, the 0.2 was mostly preemption.

### 16. Threading Q3 investigation — result

Read 2026-10-06 against the decision rule above. **No lock-free send is
built**: neither pre-registered critical-path condition holds. The plan's
literal trigger ("in-lock `sendmsg` > 10 % of an owner") *does* fire on
its letter — the driver's in-lock `sendmsg` is 0.16–0.19 of wall at c1s
and the owner's real mutex wait is ≈ 0.38 of its wall — and is rejected
because the wait is absorbed: no producer waits on the owner, and §13's
`own` arm (the wait removed) gained nothing. **The measured ceiling at
c1d is the server's receiver task** (saturated); at c1s the sender is
bound by the store cap, a constant at N = 1 (finding 3).

*Sessions.* Binary `b25fea27…f413cf9` (`81b5335`, built fresh on the
benchmark VM, line tables for perf), lib tests in the session rc 0 (26
selected, 0 failed). Session 1 13:18–13:31Z (build, tests, 18
invocations, seed 42), session 2 13:36–13:38Z (`RWM_RDIAG=1` ×4, two
per-process `perf record`s); both locks held throughout, exit state 0
`raptorpath`, 0 `rp-*`. Diagnostic, single seed, not scored. Ledgers:
`docs/l1-raw/thread-q3-diag/` (`table.txt`, `digest.txt`, `rdiag.txt`,
perf self tops, two `[DIAG]`/`[LAT]` extracts, both `era` files).

*Per task* (`RWM_DIAG=1 RWM_RTOBS=2`, n = 3 per cell; core = CPU over
the transfer, busy = poll wall over it; `[TASK]`, `[QSOCK]`, `[IOWN]`):

| | c1s-400 | c1d-400 |
|---|---|---|
| goodput Mbit/s | 720–765 | 688–729 |
| server receiver core / busy | 0.73–0.79 / 0.78–0.84 | **0.84–0.85 / 0.94–0.95** (`[RDIAG]` 99–100 %) |
| client sender core / busy | 0.40 / 0.45–0.47 | 0.53–0.55 / 0.63–0.65 |
| client owner core / busy | 0.09–0.10 / 0.41–0.48 | 0.06–0.12 per path |
| client ConnectionDriver core / busy | 0.44–0.49 / 0.53–0.58 | 0.14–0.33 per path |
| driver in-lock `sendmsg` (share of wall, µs/call, GSO) | 0.16–0.19, 21, 9.0 | 0.06–0.12, 18, 7.1–8.0 |
| owner `[IOWN]` asleep_frac (client) | 0.21–0.25 | 0.02–0.13 |
| producer batches finding the owner channel full | 6–11 of ≈ 6.5 k, ≤ 6 ms (≤ 0.14 % of wall) | ≤ 29 ms client, ≤ 41 ms server (≤ 0.9 %) |
| inbound `rx_full` / `ack_full` | 0 / 0 | ≤ 2 / 0 |
| VM busy cores of 6 / median (p90) max vCPU | 3.9–4.1 / 0.79–0.80 (0.90–1.0) | 4.2–4.4 / 0.83–0.90 (0.92–1.0) |

c2 / c8 (n = 1): every task ≤ 0.31 core, VM ≤ 1.8 cores — link-bound.
Without `RWM_DIAG` (one lock take per datagram, n = 2) the picture is the
same (c1s asleep 0.19–0.22, `fl_full` ≈ 0). The deep gauge's own cost on
goodput is not separated at this n (c1s `RWM_RTOBS=1` 805 / 731 vs
`=2` 720–765).

*Findings.*

1. **The owner's asleep is the mutex, not preemption** — §13's [H] was
   wrong. 99 % of the client owner's poll asleep falls in polls with a
   voluntary context switch (c1s r1: 1.59 s voluntary, 0.006 s preempted,
   0.008 s neither, over 4.18 s; ≈ 12.5 k futex sleeps of ≈ 127 µs). The
   section gauge under-reads it: ≈ 0.55 s of the 1.59 s is in
   `read_datagram` polls inside the `select!`, outside every section, so
   the owner's real lock wait is ≈ 0.38 of wall, not 0.25.
2. **The wait is absorbed slack for throughput.** The owner runs 0.1 core;
   the sender, receiver and control producers practically never find its
   channel full; no inbound batch waits on a full consumer channel. In the
   window-bound regime a non-full channel can still add RTT; the throughput
   bound is §13's controlled comparison (`own` removed the wait and gained
   no goodput — loose, since `own` added cross-runtime hops).
3. **Rule 18 — at N = 1 the outstanding store cap operates as a constant.**
   c1s's `[DIAG]` `win=` cap reads 1024 = `RELIABLE_STORE_MAX`
   (`net/mod.rs:441`) in every post-warm-up window of every c1s run (5
   runs, 16 windows each): the pooled law
   (`net/store_cap.rs` `pooled_store_cap`) returns `None` below two live
   paths (`[SUMCAP] eng=0/0`), so the single-path store is the constant;
   at c1d the law engages (`eng=840/840`, cap 2734, pin 0). The store sits
   at the cap at about 40 % of window samples and the sender is paused
   20–33 %; goodput ≈ 1024 symbols / RTT with RTT 9–13 ms against a
   2.4–2.7 ms floor (`[LAT]` A_x p50 6.7 ms, p95 13 ms; A_x is stamped at
   the receiver's processing, so it includes the receiver's backlog). Every
   c1s goodput in §13/§14 was measured through this constant. Recorded, not
   acted on: it is a law question (and the `n_live < 2` guard is a
   path-count step of the pattern CLAUDE.md names), outside Q3's scope.
4. **The c1d ceiling is the receiver task.** One task processes every
   datagram of every path (decode, reorder, delivery, per-datagram ack
   construction, the always-fed `[LAT]`/`[SUCC]` bookkeeping): 0.85 core,
   busy 0.95, `[RDIAG]` 99–100 % with cpu 6.6 ms per poll; its backlog
   turns into RTT (c1d 18–32 ms) and the pooled window converts that into
   goodput. Where its CPU goes is **not** measured: the perf call-graphs
   did not unwind through the async frames. The process profile (server,
   c1d, self) is flat: the glibc malloc family ≈ 13–15 % (`_int_malloc`,
   frees, `malloc_consolidate`, `sysmalloc`/`mprotect`), futex/wake ≈ 10 %
   (incl. `__lll_lock_wake_private`, glibc's lock), memmove 4.6 %, the
   receiver body 5.9 %, decoder `seq_probe`+`assimilate` 2.6 %,
   BTreeMap/HashSet inserts ≈ 2.5 %, AES-GCM 3.1 %. The receiver itself
   sleeps voluntarily ≈ 9 % of its wall (0.41 s at c1d, ≈ 68 µs each) —
   **[H]** allocator-arena contention.
5. **What the driver does under the lock** (c1s client): `sendmsg` ≈ 1/3
   of its poll wall (21 µs per call, GSO 9); AES-GCM ≈ 0.05 core (3.6 % of
   the client process samples) — the kernel send path, not the cipher, is
   the largest in-lock item. The server's driver sends the acks at GSO
   ≈ 1.5 (28 k `sendmsg` per transfer, 19 µs each).

*Next* (designed, not built; the operator's call): the receiver's
per-datagram cost — a step-0 section split of the receiver loop and the
allocator discriminator, then the cheapest allocation and per-seq-structure
fixes, and plan Q3's receiver-role split only if step 0 shows a separable
role. The proposal, its red tests and its battery design are in the Q3
report.
