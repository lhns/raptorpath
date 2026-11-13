## 10. The per-path datagram feeder — evidence record (code not merged; superseded by the threading redesign)

Recorded here as evidence for the threading redesign (§9; the topology
audit cites it). The two batteries ran on branch `feat/quic-feeder-v2`,
whose `docs/status.md` §9 and §10 hold the full pre-registrations, results
and ledgers (`docs/l1-raw/feeder/`, `docs/l1-raw/feeder2/` on that branch).
**The feeder code is not merged**: neither battery recommended a flip, and
the threading redesign replaces the mechanism (a queue + drainer in front of
quinn's connection mutex) with exclusive per-path ownership, which removes
the contended mutex instead of moving the wait.

**Feeder v1** (that branch's §9; a per-path FIFO + tokio-task drainer for
every datagram, data and control; engine `7729992`, binary `1d2b7bdc…`,
n = 8 × 2 seeds, 128 invocations, VM 2026-10-05). Result
`WORSE-AT-c1s-400,c1d-400,c8-100` (`c2-100` SAME). `c1d-400` goodput 292.4 →
491.4 Mbit/s (+68 %) and CPUCLI −32 %; `c1s-400` CPUCLI +25.8 % (MDE 2.4 %),
4 of 16 FEED rows collapsed (down to 77.7 Mbit/s, RTprop floor up to
83 ms) because the server's ack feeder sat behind its receiver task (server
sojourn max 42–94 ms); `c8-100` goodput −4.5 %. The SOJOURN-BOUNDED clause
was mis-derived (iteration 19 µs, not 50–150 µs). Off-CPU capture at c1d:
sender wall asleep in quinn's `send_datagram`/mutex 0.202 → 0.016 (the wait
moved to the drainer, ≈ 0.10 of its wall).

**Feeder v2** (that branch's §10; data-only queue, control datagrams on the
caller's thread; drainer as a tokio task `FDT` or an OS thread `FDH`; the
direct seam `NEW` as control; engine `c9742a8`, binary `cd9fb661…`, n = 8 ×
2 seeds, 192 invocations, VM 2026-10-05, emission batching on). Result
**`FDT WORSE-AT-c1s-400,c1d-400,c2-100`; `FDH WORSE-AT-c1s-400,c2-100`**;
nothing flipped. Medians, n = 16:

| cell | goodput NEW / FDT / FDH (Mbit/s) | CPUCLI s NEW / FDT / FDH | µs CPU per datagram NEW / FDT / FDH | CPUSRV s NEW |
|---|---|---|---|---|
| `c1s-400` | 510.4 / 534.1 / 510.0 | 5.73 / 7.00 (+22 %) / 7.52 (+31 %) | 16.8 / 20.6 / 22.1 | 8.26 |
| `c1d-400` | 424.4 / **501.3 (+18 %)** / 476.8 (+12 %) | 11.19 / 9.30 (−17 %) / 10.76 | 32.2 / 26.5 / 30.8 | 12.02 |
| `c2-100` | 89.1 / 89.0 / 89.2 | 3.23 / 3.41 / 3.83 (+19 %) | 35.2 / 36.7 / 41.0 | 4.83 |
| `c8-100` | 101.6 / 102.3 / 103.0 | 5.35 / 4.96 / 5.68 | 56.6 / 50.4 / 60.4 | 6.04 |

What fired: FDT the RTprop-floor clause (c1d leg p1 +5.6 % against +5 %;
c2 +11 %: the task drainer runs only at the sender's yield) and CPU at c1s;
FDH CPU at c1s and c2, one TAIL excursion (c1s smoothed RTT 133 ms) and one
post-close control `drop`. Sender busy NEW / FDT / FDH: c1s 35 / 56 / 52 %,
c1d 96 / 50 / 59 %. Off-CPU capture at c1d: sender wall asleep in quinn
0.226 / 0.016 / 0.057; the drainer carries it (FDT 0.085, FDH 0.183 of its
wall); idle-worker sleeps 16.1 / 19.6 / 38.8 k/s.

**What the evidence says, for the redesign.** (1) At c1d the sender's wall
is the handoff into quinn's per-connection mutex (held across AES-GCM and
`sendmsg`); taking it off the sender is worth +18 % goodput at −17 % CPU.
(2) Every feeder arm moved that wait rather than removing it, and paid for
the move with +22–31 % CPU on single-path cells where goodput is pinned at
the ≈ 500 Mbit/s non-sender ceiling. (3) The task/thread A/B is an A/B of
tokio's wake placement: a wake from a worker lands in that worker's LIFO
slot (serialized, batched, deferred to the sender's yield), a wake from a
non-worker goes through the inject queue + an unpark (parallel, colliding
on the mutex, doubling worker park/unpark churn). Hence the plan: owned
paths (one I/O worker owns a path's endpoint and connection, so the mutex is
uncontended), batched rings with a parked-flag wake, and a logic actor that
owns the scheduler — measured first by P0 (§9).
