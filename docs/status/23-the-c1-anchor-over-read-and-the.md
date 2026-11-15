## 23. The c1 anchor over-read and the store law's input — diagnosis — pre-registration

§18/§19 left the next defect named: at one path the pooled law reads an
anchor that over-reads the delivered rate ×8–29 (`[DIAG]` `btlbw` against
goodput; ×8–10 at the 1024 constant, ×12–29 at the 2048 knee, ×8–11 at c2),
so `c1s-400` pins at the knee in 56–90 % of windows and the cap is again a
constant (rule 18). This is a **diagnostic session**: one binary, main
(`0972c3c`, built fresh), nothing ships or flips from it. It answers three
questions the fix depends on, before any law is written down.

**What is already known (not re-litigated).** The mechanism class is named
in paper §2.7/§8.3 and in the code (`control_msg.rs`, `copa.rs`
`record_delivery`): the default sampler is ack-interval
`Δdelivered / Δt_ack` with a 1 ms floor, and its 10 s windowed max latches
bunched acks. The send-interval sampler (`RWM_PLAIN_RS`, BBR's
`max(send_elapsed, ack_elapsed)` with an interval ≥ RTprop) read 1.02× the
link at sc2 (ledger at `ac1aed1`, "Anchor Hygiene"); fed to the old
`2·BDP` store chain it cost sc2 −20 % ("the over-read is load-bearing"),
which the `RWM_HONEST_CAP` re-derivation repaid with a 100 ms runway term
(`HONEST_RECOVERY_ROUND_S`, an open constant). `RWM_FLOOR_BOUND` cost c1
−14 % as a Copa cwnd-floor bound. Every one of these predates either
SACK-clocked release (ADR-0060, 2026-07-21) or the δ-cap (1 + q), and none
was read at c1s under the current law. Not tested here: the honest anchor
as a cwnd-floor input (that is `RWM_FLOOR_BOUND`'s question, not this one).

**Questions.**
- **Q1 (rule 1: does the named mechanism produce the c1s/c2 anchor?)** The
  samples that set the windowed max are ack-interval samples whose Δt sits at
  the 1 ms floor, i.e. the max is a bunch of acks divided by the floor, not a
  delivery rate.
- **Q2 (the honest input under the current law).** With the send-interval
  sampler feeding the shipped law (`RWM_PLAIN_RS=1 RWM_HONEST_CAP=0`, so the
  pooled `(1 + q)·Σ bw·RTprop` law runs on it and the honest-cap arm does
  not), what does the anchor read at each cell and path, what does the law
  ask, and what happens to goodput, `A_x` and sender CPU?
- **Q3 (the store the cell actually needs).** A static cap sweep
  (`RWM_STORE=<n>`, which disables the dynamic cap and changes nothing else on
  this path) maps goodput and `A_x` against the cap at the two single-path
  cells. The knee of that curve is the ground truth any law must reach; Q2's
  honest law value either reaches it or names the missing headroom.

**Instruments** (all existing, observation only, no code change):
`[DIAG]` (`btlbw`, `win=`, `rtt`, `rtp_us`), `[SUMCAP]`, `[LAT]` `A_x`,
`[ACKDIAG]` (`RWM_ACKDIAG=1`: ack `gap_us`, `drecv` per ack, the per-sample
over-read `x[p50 p90 p99]`, `xanchor`), and `[RSTRACE-LEGACY]` /
`[RSTRACE]` (`RWM_RS_TRACE=<thr>`: every accepted sample at or above `thr`
with its `delta` and `elapsed_ms`, resp. its send/ack interval). `thr` is
2× the cell's link datagram rate: 195 000 sym/s at c1 (1 Gbit/s ÷ 1 279 B
wire datagrams = 97 700/s), 19 500 at c2 and c8.

**Design.** One binary, built fresh in a fresh target under both locks, its
`sha256` checked per invocation. All rows: shipped defaults otherwise, Bulk,
`--window-reliable`, `RWM_GEN=0 RWM_DIAG=1 RWM_ACKDIAG=1
RWM_RS_TRACE=<thr> RWM_PERF_TIMEOUT_S=150`, the same env withholding as §19
plus the arm gates (`env -u RWM_EST_CADENCE RWM_POOL_ANCHOR RWM_EMIT_BATCH
RWM_EMIT_BURST RWM_RTOBS RWM_ACKDIAG RWM_IO_RT RWM_STORE_PATHS RWM_PLAIN_RS
RWM_HONEST_CAP RWM_STORE` before the arm's env), `perf_rwm_c.sh`, one
invocation per fresh topology. Arms:

| arm | env | cells | n (seeds) |
|---|---|---|---|
| `MAIN` | none | `c1s-400`, `c2-100`, `c1d-400`, `c8-100` | 3 (42, 7, 42) |
| `PRS` | `RWM_PLAIN_RS=1 RWM_HONEST_CAP=0` | the same four | 3 (42, 7, 42) |
| `S<n>` | `RWM_STORE=<n>` | `c1s-400`: n ∈ {192, 256, 384, 512, 768, 1024, 1536, 2048} | 3 (42, 7, 42) |
| `S<n>` | `RWM_STORE=<n>` | `c2-100`: n ∈ {64, 96, 128, 192, 256, 384, 512, 1024} | 2 (42, 7) |

Order: per (rep, seed) block, the four cells × {MAIN, PRS} with the arm
order rotated by the block index, then that block's sweep points (c1s every
block, c2 in the first two), ascending on even blocks and descending on odd.
24 (MAIN, PRS) + 24 (c1s sweep) + 16 (c2 sweep) = 64 invocations in three
blocks ((42, 1), (7, 1), (42, 2)), preceded by a 3-invocation smoke
(`c1s-400` MAIN and PRS, `c2-100` `S256`).

*Witnesses* (a row without them is void, not null). Every row: the
`[GATES]` echo carries the arm's gates (`RWM_PLAIN_RS`, effective
`RWM_HONEST_CAP`, `RWM_STORE`), and at least one `[ACKDIAG]` line. `MAIN`:
`[SUMCAP] eng>0`, no "send-interval SAMPLER ACTIVE" echo. `PRS`: the
"plain-mode send-interval SAMPLER ACTIVE" echo, `[GATES]` effective
`RWM_HONEST_CAP=0`, no "honest floor-clock store caps ACTIVE" echo,
`[SUMCAP] eng>0`. `S<n>`: `[SUMCAP] eng=0` (or absent), and every
post-warm-up `[DIAG]` `win=` denominator equals n.

*Abort table* (tokens fixed now; the result opens with it, filled):

| cause | token |
|---|---|
| a VM lock not held by the envelope | `ABORT-LOCK` |
| CR bytes in the synced tree | `ABORT-CRLF` |
| binary `sha256` differs from the recorded one | `ABORT-SHA` |
| a sentinel path not write+unlink probed | `ABORT-SENTINEL-UNWRITABLE` |
| the smoke fails a witness | `ABORT-SMOKE` |
| the fresh build or the parser test fails | `ABORT-BUILD` / `ABORT-TESTS` |
| a driver exits non-zero (row void, session goes on) | `ABORT-RC` |
| no summary after the bring-up tries | `ABORT-BRINGUP` (row `NO_DATA`) |
| a compile or build process ran on the VM during the row | `VOID-COTENANT` |

**Readouts and outcomes (fixed now).** Means over live rows; the link
datagram rate `L` is 97 700/s at c1 and 9 770/s at c2 (the c3 leg 1 955/s).
- **Q1, per cell `c1s-400` and `c2-100` (MAIN rows).** `M` = the
  `[RSTRACE-LEGACY]` samples at or above `thr`. `MECHANISM-CONFIRMED` if, in
  every live MAIN row at the cell, (a) the row's median post-warm-up `btlbw`
  is ≥ 3·L (the over-read is present) and (b) ≥ 90 % of `M` have
  `elapsed_ms` < 2.0 (the sample's length is the floor plus at most one
  sender loop, not a delivery interval). `MECHANISM-OPEN` otherwise, with the
  `elapsed_ms` distribution of `M` reported. Reported beside it: `[ACKDIAG]`
  `gap_us`, `drecv` and `x` quantiles, `xanchor`, and `rd[acc rej]`.
- **Q2, per cell (PRS rows).** Per path, `o = btlbw / L_path` (median of
  post-warm-up `[DIAG]` windows). `HONEST` at a cell if `o ≤ 1.2` on every
  path of every live row; `OVER-READ` otherwise, with `o` reported (the
  dual cells are the slow-path residual (iii) check). Reported, not scored:
  PRS − MAIN goodput, `A_x` p50, `[SUMCAP]` cap and pin, rtt, and client CPU
  per GB, each with the 2σ_pooled band (n = 3 per arm is a diagnostic, not a
  battery; nothing here is a ship verdict).
- **Q3, per cell (`S<n>` rows).** `G(n)` = mean goodput at cap n, `Gmax` =
  max over the grid, knee `n*` = the smallest grid n with
  `G(n) ≥ 0.95·Gmax`. `ask_h` = the honest law's ask
  `(1 + 0.1)·bw_h·RTprop` with `bw_h` and `RTprop` the PRS rows' medians at
  that cell. `LAW-SUFFICIENT` if `n* ≤ 1.5·ask_h` (one grid step: the honest
  input alone reaches the knee); `HEADROOM-MISSING` if `n* > 1.5·ask_h`,
  reported as `H = n* − ask_h` symbols and `H / bw_h` ms; `UNSCOREABLE` if
  fewer than 2 live rows at more than two grid points. Reported beside it:
  `A_x` p50/p95 and rtt against n, and `xanchor` against n (prediction,
  reported only: `xanchor` rises with n at c1s, i.e. the over-read grows
  with the store it sizes — the circularity).

**Decision rule (what the design takes from this; ships nothing).**
`MECHANISM-CONFIRMED` at c1s names the defect as the rate input, so the fix
changes the input at the cap seat (not the law's shape). Then, per Q3:
`LAW-SUFFICIENT` at both single-path cells → the fix is the send-interval
rate as the pooled law's `bwᵢ` at the cap seat only, no new term;
`HEADROOM-MISSING` at either → the missing headroom is a term the paper must
derive and measure before a build (candidates named now: BBR's `extra_acked`
ack-aggregation allowance, draft-ietf-ccwg-bbr; the hole residency of lost
symbols under SACK-clocked release), and a second diagnostic is
pre-registered for it. `MECHANISM-OPEN` → the input is not changed until the
mechanism is shown. Anything else is `OUTSIDE THE PRE-REGISTERED SET`.

**Predictions (written before any number exists, reported).** Q1 confirmed
at both cells. Q2: `HONEST` at both single paths; PRS asks ≈ 300 symbols at
c1s and ≈ 100 at c2, and goodput falls at c1s by more than 2σ (the law on an
honest input under-funds the c1s loop, as `RWM_FLOOR_BOUND` did) with `A_x`
p50 down. Q3: the c1s knee lies between 384 and 768 and the c2 knee between
128 and 384, i.e. `HEADROOM-MISSING` at c1s and undecided at c2.

**Duration.** Build ≈ 10 min; 67 invocations at ≈ 14 s (§19: 54 in 757 s)
≈ 16 min; under one hour with the lock waits seen in §18/§19, well inside
the 4 h launch deadline and the 5 h cap.

### 23. The c1 anchor over-read and the store law's input — result

Scored 2026-10-26 against the pre-registration above, literally; no
threshold was touched after the data was read. Diagnostic: nothing ships or
flips. **Q1 `MECHANISM-CONFIRMED` at `c1s-400` and `c2-100`. Q2 `HONEST`
at all four cells by its rule, which has no lower bound: the send-interval
anchor *under*-reads ×0.1–0.5 and the shipped law on it collapses to its
floor (`OUTSIDE THE PRE-REGISTERED SET`, below). Q3 `HEADROOM-MISSING` at
both single-path cells.** By the decision rule no input change is built;
the finding below changes what the fix has to be.

*Binary and session.* One binary, `sha256 54b7eaa7…cf8b4ff`, built fresh
(310 s) from `025bc52` (engine identical to main `0972c3c`; the commits after
it are this pre-registration and the harness). Envelope
`tools/l1/anchordiag_run_all.sh` (battery `anchordiag_battery.sh`, parser
`anchordiag_parse.py`, its test run in the session). Launched 16:01:29Z,
waited 6.5 min for the locks (another agent), held both 16:08:00–16:32:47Z;
cotenants cargo 0 / rustc 0 at lock time; smoke 16:13:49Z; battery 1 138 s,
64 invocations; exit state 0 `raptorpath`, 0 `rp-*`; run dir removed.
Ledgers: `docs/l1-raw/anchor-diag/` (`ad.log` with every `ADROW`,
`score.txt`, `smoke*`, `all-era.txt`, `BINSHA.txt`, per-invocation logs in
`diag-ad/`, `diag-smoke/`).

*Abort table (filled).* `ABORT-LOCK`, `-CRLF`, `-SHA`,
`-SENTINEL-UNWRITABLE`, `-SMOKE` (`SMOKE-PASS`, 3 rows), `-BUILD`, `-TESTS`,
`-RC`, `-BRINGUP`, `VOID-COTENANT`: none fired. 64 of 64 rows LIVE, no
retry.

*Q1 (MAIN rows; `L` = link datagram rate).*

| cell | `btlbw` / L | samples ≥ thr with `elapsed_ms` < 2 | `elapsed_ms` p50 | `delta` p50 (symbols) | `rd` accepted / rejected |
|---|---|---|---|---|---|
| c1s-400 | 7.7–10.0 | 0.952–0.985 (n 242–263) | 1.00–1.01 | 401–404 | 872–1 035 / 146–152 k |
| c2-100 | 7.3–12.8 | 0.951–0.970 (n 349–370) | 1.19–1.26 | 30–31 | 935–991 / 18.2–18.3 k |

The samples that set the max are one floor long: 99.3–99.4 % of
`record_delivery` calls at c1s land inside the 1 ms floor and are carried
into the next accepted sample, which then divides ≈ 400 symbols (≈ 4 ms of
delivery at L) by ≈ 1 ms. `[ACKDIAG]` ack inter-arrival p50 1–2 µs at both
cells: the sender processes acks in bunches. The location of the bunching
(receiver emission, ack path, sender task) is not discriminated and does
not need to be for what follows.

*Q2 (PRS: `RWM_PLAIN_RS=1 RWM_HONEST_CAP=0`, the shipped law on the
send-interval anchor; n = 3 per arm, not scored).*

| cell | `btlbw` / L per path | goodput PRS / MAIN (Mbit/s) | `[SUMCAP]` cap PRS / MAIN | `A_x` p50 PRS / MAIN (ms) | rtt PRS / MAIN (ms) | client CPU s/GB PRS / MAIN |
|---|---|---|---|---|---|---|
| c1s-400 | 0.23–0.34 | 188.2 / 662.5 | 64 / 1 707 | 0.4 / 14.0 | 2.9 / 22.2 | 28.0 / 15.2 |
| c2-100 | 0.10 | 7.2 / 89.9 | 11 / 995 | 4.4 / 106.5 | 13.4 / 127.7 | 73.1 / 33.9 |
| c1d-400 | 0.39–0.50 | 439.2 / 569.6 | 214 / 1 757 | 1.0 / 19.8 | 4.0 / 29.2 | 23.5 / 19.7 |
| c8-100 | p0 0.11–1.05, p1 0.00–0.51 | 32.9 / 100.7 | 63 / 1 599 | 5.1 / 92.8 | 75.8 / 243.2 | 50.9 / 45.8 |

*Q3 (static cap sweep; means, [min–max]).*

| c1s-400 n | 192 | 256 | 384 | 512 | 768 | 1024 | 1536 | 2048 |
|---|---|---|---|---|---|---|---|---|
| goodput | 445.8 | 439.4 [278–535] | 596.8 | 608.7 | 679.7 | 598.0 [407–698] | 634.2 [485–754] | 727.2 |
| `A_x` p50 ms | 0.9 | 2.9 | 1.8 | 2.6 | 4.6 | 6.7 | 12.3 | 15.4 |
| rtt ms | 3.4 | 5.8 | 5.1 | 5.6 | 7.7 | 25.9 | 24.1 | 21.6 |

| c2-100 n | 64 | 96 | 128 | 192 | 256 | 384 | 512 | 1024 |
|---|---|---|---|---|---|---|---|---|
| goodput | 37.7 | 54.2 | 70.2 | 89.2 | 89.2 | 89.8 | 89.9 | 89.8 |
| `A_x` p50 ms | 5.1 | 4.9 | 5.6 | 7.4 | 13.8 | 24.6 | 36.9 | 86.0 |
| rtt ms | 14.4 | 14.9 | 15.8 | 18.8 | 24.9 | 37.6 | 51.1 | 102.8 |

c1s: `n*` = 2048 by the 0.95 rule (`ask_h` = 57, `H` = 1 991 symbols); the
curve is not monotone and its spread is wide (receiver-bound, §16 finding 4:
goodput rises with the queue through receiver batching, not through a
network knee). c2: `n*` = 192 (`ask_h` = 7, `H` = 185 symbols = 192 ms at
the under-read `bw_h`; at L it is ≈ 19 ms). `xanchor` against n: Spearman
0.90 at c1s (the over-read grows with the store it sizes), −0.29 at c2.

*Predictions.* Q1 confirmed at both: MET. Q2 `HONEST`: MET by the rule;
the c1s goodput fall: MET in sign, its size (−72 %, c2 −92 %) and the
collapse to the floor not predicted. Q3: c2 knee in [128, 384] MET (192);
c1s in [384, 768] MISSED (2048 by the rule; 768 reads 0.93·Gmax).

*Outside the pre-registered set: the law is a window controller fed by its
own output.* The PRS c2 trace (`diag-ad/c2-100-PRS-s42-r1-a1-c.log`)
decays geometrically, `win=` 41 → 32 → 29 → 25 → 17 → 10 in 3 s, and sits
at the floor 10 for the remaining 45 s with `btlbw` ≈ 870–1 030 = 10 /
rtt. On a store-bound sender the delivered rate is `cap / T_loop`, so a law
`cap = (1 + q)·bw·RTprop` fed by any sampler of the sender's own delivery is
the map

```text
   cap'  =  g · cap ,      g  =  (1 + q)·RTprop / T_loop      (send-interval sampler)
   cap'  =  g · x(cap) · cap                                  (ack-interval sampler, x = its over-read)
```

At c2 `T_loop` ≥ 12–15 ms against RTprop 6.5–6.9 ms even with ten
symbols outstanding (netem jitter on both directions), so g ≈ 0.55–0.6;
at c1s g ≈ 1.1·2.24/2.9 ≈ 0.85. Below 1 the honest law contracts to its
floor; the ack-interval over-read (x ≈ 8–13, rising with the cap) makes the
product exceed 1 and the law runs to the knee. Neither sampler measures the
bottleneck, because the sender never sends above its own cap: **the
over-read is the law's accidental probe**, which is why every honest-anchor
arm on record lost goodput ("load-bearing"). What a working law must reach
is visible in Q3: c2 at a static 192 holds goodput (89.2 vs MAIN 89.9) at
`A_x` p50 7.4 ms against MAIN's 106 ms; c1s at 768 reads 680 vs 662 at 4.6
ms against 14.0 ms. The two knees sit near 3.0·RTprop·L (c2) and
3.5·RTprop·L (c1s).

*What this settles.* The c1 anchor defect is the ack-interval sampler at
its 1 ms floor (Q1), but replacing the input does not fix the law: the
store-cap seat has no measurement of the rate the path could carry, only of
the rate the cap lets through. The next step is a design decision on the
controller (an unloaded loop-time term, an explicit probe, or the
substrate's rate), not a build; it is the coordinator's.
