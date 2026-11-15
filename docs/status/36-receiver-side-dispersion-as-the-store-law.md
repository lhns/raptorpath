## 36. Receiver-side dispersion as the store law's link-rate source — capture diagnostic — pre-registration

§34 left the store law without a link-rate source at c1s, c2 and c8.
Paper §6.1, "Receiver-side dispersion as the link-rate source", shows that
a perfect L would close the law: `(1 + q)·Σ L·(RTprop + S)` has stable fixed
points near the Π₂ argmax at c2 (164), c8 (324) and post-§32 c1s (614,
slope 0.54), with c1d at `CEILING`. It also derives from netem's release
rule why trains of skbs at the receiver should read the link. This session
tests that estimator on a packet capture. Diagnostic: no engine diff, no
law built, nothing ships.

**Tree.** github/main at launch (≥ `4926822`) plus a tools-only harness
commit: an opt-in capture in `tools/l1/perf_rwm_c.sh` (`RWM_PCAP_DIR`), a
plan 36 in `loopdiag_*`, and a capture scorer with tests. The engine binary
is main's, built fresh on disk and run from `/dev/shm`. No suites: no
engine code changes. The parser and scorer tests run in the session.

**Capture.** On each data leg's receiver device (`srv0`, `srv1` in the
`rp-*` server netns), from server bind to client exit:
`tcpdump -i <dev> -s 64 --time-stamp-precision=nano -B 16384 -w
/dev/shm/<run>/<row>.<leg>.pcap udp and src <client addr>`. The process is
stopped by its PID, never by `pkill`. Netem sits on the client-side
egress, so these timestamps are taken at the peer's receive, right after
netem's release. Per skb: (t, b), with b the bytes netem charged:
`orig_len + (segs − 1)·42`, segs = ⌈UDP payload / (udp_len − 8)⌉. The
pcaps stay on tmpfs and are deleted after the row is scored. Per-row
summaries are committed.

**Design.** Static cap `RWM_STORE=<n>`, shipped substrate BBR, Bulk,
`--window-reliable`, `RWM_GEN=0 RWM_DIAG=1`, §34's withholding list.

| cell | caps | role |
|---|---|---|
| `c1s-400` | {64, 128, 256, 512, 1024, 2048} | scored |
| `c2-100` | {16, 32, 64, 128, 256, 512} | scored |
| `c8-100` | {64, 128, 256, 384, 512, 1024} | scored, both legs |
| `c1d-400` | {128, 512, 2048} | reported |

- Arm `CAP<n>` captures. Arm `NOCAP<n>` at c1s {256, 1024, 2048} does not;
  it is the same-session control for the capture's CPU cost.
- Two blocks, seeds 42 then 7, one rep each. Ascending on the first block,
  descending on the second; cells alternated. 54 rows, after a 2-row smoke
  (c1s CAP256, c8 CAP128).

*Rules (shared file).* Launch precondition: both locks free (`test -e`
only), no cargo/rustc (`pgrep -x`), `procs_running` ≤ 2 in 5 samples 2 s
apart. Run dir, binary and pcaps on `/dev/shm`. io `full avg10`
before/after every row, flagged at ≥ 20, never voided. `VOID-ONDISK` off
tmpfs.

*Witnesses (row void otherwise).* `[GATES]`; `[SUMCAP] eng` absent or 0;
tcpdump reports 0 packets dropped by the kernel; per leg, the captured
segment count is within 1 % of `[TRUTH] netem_sent_dgrams`. A leg with
< 200 captured skbs is noted `p<i>-idle` and is not scored. The row is
void only if every leg is idle.

*Abort table:* `ABORT-LOCK`, `-CRLF`, `-SHA`, `-SMOKE`, `-BUILD`, `-TESTS`
(parser/scorer), `-RC`, `-BRINGUP`, `-BUDGET`, `VOID-COTENANT`,
`VOID-ONDISK`.

**Estimator (fixed now; it uses no L).** Per leg, skbs in arrival order;
post-warm-up = after the first 1 s of the leg's capture. Train rate
`r_k(j) = Σ_{i=j+1..j+k} bᵢ / (t_{j+k} − t_j)`. Per 250 ms window w,
`L̂_k(w) = max_j r_k(j)` over trains starting in w. Per row,
`L̂_k = median_w L̂_k(w)` and `cv_k = sd_w / mean_w`. Scored at **k = 4**;
k ∈ {2, 8, 16} and the window p90 in place of the max are reported.

**Readouts and outcomes.** Links in bytes/s: c1 125·10⁶, c2 12.5·10⁶,
c3 (c8's slow leg) 2.5·10⁶. Q(p, n) = mean over live rows of L̂_4 ÷ link.
- **D1, the reading (per cell, per leg).** §27's classes, tightened.
  `FLAT-AT-LINK` iff every Q(p, n) ∈ [0.85, 1.15]. Else `CIRCULAR` iff
  Spearman(Q, n) ≥ 0.8 and Q(n_min) < 0.85. Else `OVER-READ` iff the median
  Q > 1.15. Else `APP-LIMITED` iff the median Q < 0.85. Else `NEITHER`.
- **D2, the noise.** Within-row `cv_4` per cap: `QUIET` iff ≤ 0.10 at every
  cap, else `NOISY` (naming the caps).
- **D3, the premise (diagnostic, uses L).** In-run share = the fraction of
  consecutive pairs with spacing within ±10 % of b/R. Also the distribution
  of busy-run lengths in skbs, per cap. Reported, no outcome.
- **D4, the capture's cost.** c1s NOCAP − CAP goodput at {256, 1024, 2048}.
  Reported. A drop > 5 % is named as a covariate of c1s's D1.
- **D5, the law at L̂.** §27's fixed point with Lₚ = Q·link/W and RTprop
  and S from the same rows' `[DIAG]`, beside the oracle-L fixed point.
  Reported.

**Decision rule (ships nothing).** `DISPERSION-READS-LINK` iff D1 is
`FLAT-AT-LINK` and D2 `QUIET` at c1s, c2 and both legs of c8. Then paper
§6.1 writes the law `(1 + q)·Σ L̂ᵢ·(RTpropᵢ + Sᵢ)` with its bind gauges, an
ADR covers receive timestamps and the receiver → sender field, and a build
is pre-registered. That stops for the user's go. Otherwise `NOT-YET`,
naming the failed cells, clauses and the k at which they would pass. Any
re-scoring at another k is a new registration.

**Predictions (before any number).** c2 and c8 `FLAT-AT-LINK` (a slow link
makes J small against k·b/R; jitter builds busy runs). c1s: `QUIET`, but
`OVER-READ` at k = 4 is the stated risk (J of 20 µs against 4 × 28 µs ≈
+18 %), with the risk of short trains at caps 64–128. c1d reads ≈ L per
leg (dispersion sees the link, not the receiver). D4 < 5 %.

**Duration.** Build ≈ 6 min; 56 invocations ≈ 15 min; scoring on the VM
≈ 5 min. Inside the 4 h launch deadline and the 5 h cap.

### 36. Amendment 1 — capture accounting, kept tables, the floor caps (before VM contact)

Nothing in the estimator, the outcomes or the decision rule changes.

- *Segments.* On Linux 7.0 a UDP-GSO skb carries the total length in
  `uh->len` (`udp_send_skb`), so the per-segment length is not on the wire
  the tap sees. segs = ⌈UDP payload / m⌉, where m is the row's full-segment
  payload: the most frequent payload among skbs ≤ 1 472 B. b = `orig_len +
  (segs − 1)·42`.
- *Witnesses (replacing the segment-count one).*
  - Bytes: Σb per leg is within 1 % of the netem qdisc's `Sent … bytes` on
    the client-side device, which counts `qdisc_pkt_len`.
  - Release unit: captured skbs per leg equal `[TRUTH] egress_skbs −
    netem_dropped_skbs` within 1 %. This catches a tap that merges or
    splits skbs, where spacings would be meaningless.
  - tcpdump's kernel-drop count stays 0. It is read from tcpdump's stderr
    after SIGINT by PID and `wait`.
- *Kept tables.* The per-skb (Δt ns, b) table of every live leg is kept,
  gzipped, under `docs/l1-raw/disp36/`. Re-scoring at another k or with
  another estimator is then an offline re-registration on the committed
  ledger. Only the pcaps are deleted.
- *D5.* Main's `[DIAG]` carries no Tp50/Tmin (`[LOOPDIAG]` is off main).
  So D5 takes RTprop and S per cap from the §27 (c2, c8) and §34 (c1s)
  ledgers, interpolated in n, and says it is cross-session. It also reports
  goodput at the fixed point, interpolated from this session's static grid,
  against Gmax.
- *Floor caps.* §23's contraction trap sits at the floor. c1s and c8 add
  caps 16 and 32: c1s {16, 32, 64, 128, 256, 512, 1024, 2048}, c8 {16, 32,
  64, 128, 256, 384, 512, 1024}. That is 62 rows plus a 2-row smoke.
  `RWM_PERF_TIMEOUT_S=300`, because c8 at 16 may need about 100 s.
- *Duration.* About 35 min of rows plus the build; inside both limits.

### 36. Amendment 2 — the coupled loops, reported beside D1–D5 (before VM contact)

The store law does not act alone. Quinn's window (ADR-0068 q12), the
placement law (paper §5.7) and the recovery headroom H (§7.3) share its
fixed point, and a joint derivation follows this session. Two readouts are
added, reported only; nothing in D1–D5 or the decision rule changes.
- **D6, authority.** Per cap and leg: quinn's window (`[DIAG]` `cwnd=`) and
  backlog B = median of `dgq<i>[hand − tx]` over post-warm-up `[DIAG]` lines,
  each beside n. These show where the cap shares authority with quinn's
  window.
- **D7, placement.** Per cap: each leg's share of `[TRUTH] egress_dgrams`,
  and the idle-leg notes. These feed B2 and the shared rule the store and
  placement laws need for a path that carries no traffic, so has no trains.

A `DISPERSION-READS-LINK` verdict therefore pre-registers the joint
derivation (store × substrate window × placement × H × dials), not a build.

### 36. Amendment 3 — readouts from the joint derivation and the literature scan (before VM contact)

Reported only; D1–D5's outcomes and the decision rule do not change.

- *Grid.* c1s adds 768, which brackets the oracle-L point 614. That gives 64
  rows plus a 2-row smoke.
- *D6 additions.*
  - Per cap and leg, the median over post-warm-up `[DIAG]` lines of
    `rtt=` − `wrtt=`: the app-echo RTT minus quinn's. It reads the dwell
    (store plus quinn backlog) inside the store law's time term.
  - R̂ = `win=` numerator − `cwnd=` − B, where B > 0: what quinn has acked
    and the store has not yet released.
- *D8, idle legs.* For every leg noted `p<i>-idle` or with egress share
  < 5 %, its last-window `rtt`, `wrtt`, `rtp_us` and `infl`, as ratios to
  the active leg. A frozen dwell-inclusive srtt on an idle leg (`rtt` ≫
  `wrtt`) is a lock-out; fresh prices with a low share are the placement
  law. Feeds B2.
- *c1d receiver.* c1d rows add `RWM_RDIAG=1` (server readout, no engine
  diff). μ_R is reported split by the number of active legs.
- *Estimator robustness.* Beside the scored per-window max, also report the
  per-window median and mode of r_4. Dispersion's capacity is a local mode,
  not the global one (Dovrolis et al., ToN 2004); on netem the max is
  expected to match. Any change to the scored estimator is a new
  registration.

### 36. Erratum to amendments 2–3 (before VM contact)

- *Field names.* Quinn's window per leg is `[DIAG]`'s per-path `p<i>:… qcwnd=`
  (src/net/diag.rs:717). The top-level `cwnd=` is not quinn's per-leg window.
  D6 reads `qcwnd`, and R̂ = `win=` numerator − Σ `qcwnd` − B. The D6 dwell
  term and D8 read `rtt=` and `wrtt=` (ms), `rtp_us=` (µs) and `infl=`, all
  from the same `p<i>:` block.
- *Estimator robustness.* The mode of r_4 is dropped, because it has no
  pre-registered bin width; the median stays.

### 36. Amendment 4 — the row count, the lateness bound, tcpdump's user (before VM contact)

- *Rows.* The enumerated grids give 2 blocks × (c1s 9 + c2 6 + c8 8 +
  c1d 3 + NOCAP 3) = 58 rows, plus the 2-row smoke. The "54", "62" and "64"
  written above were miscounts; the grids stand as listed.
- *Lateness bound.* Lateness J on a train's anchor skb shortens its span
  from k·b/R to k·b/R − J. So the train reads R/(1 − x), with x =
  J/(k·b/R), an over-read of x/(1 − x), not x. At the c1s prediction (J
  20 µs, k = 4) that is ≈ +22 %, not +18 %. The prediction is unchanged in
  kind.
- *tcpdump's user.* The capture runs with `-Z root`, so a build that drops
  privileges before opening `-w` can still write the root-owned run dir.
- *Witness set.* The DEF-arm witnesses carried over (`[GATES]`, the store
  gate, every post-warm-up `win=` denominator equal to n, `[SUMCAP] eng`)
  apply as listed in the pre-registration's witness line.
- *Erratum to §37.* `goal-gate.md`'s rules live in
  `docs/measurement-discipline.md` (ADR-0052). Main's CLAUDE.md already
  cites that file; only an older branch's copy cites `goal-gate.md`.

### 36. Amendment 5 — a pre-launch check row fails both capture witnesses (after one check row, before any scored row)

*Check row.* `perf_rwm_c.sh c2 c2 bulk 100000000 1 single`, `RWM_STORE=128`,
with the capture and `RWM_RDIAG=1`, in the real `rp-*` netns on main's
binary. Not scored; no ledger kept. Results:

- tcpdump 4.99.6 `-Z root` wrote a nanosecond pcap to root-owned /dev/shm,
  with kdrop 0.
- main prints `[RDIAG]` (22 lines).
- netem on `cli0`: Sent 110 501 532 B and 90 115 pkt. `[TRUTH]`:
  egress_skbs 18 599, netem_dropped_skbs 484, so 18 115 skbs released.
- Captured on `srv0`: 17 161 skbs, 5.3 % low.
- With m = 1 252: Σb is 105 151 140 (4.8 % low) and Σsegs is 89 231 (1.0 % low).
- GRO is off on `srv0` and `cli0`.

*Two defects.*
1. Amendment 1's m rule ("most frequent payload ≤ 1 472 B") returns 36:
   small client datagrams outnumber single-segment data skbs. m becomes the
   payload that carries the most bytes, count × payload (here 1 252).
2. The skb and byte gaps (≈ 5 %) agree with each other but not with the
   datagram gap (1 %), so they are not one cause. The witnesses also compare
   a filtered, windowed capture against all-protocol counters that cover the
   qdisc's whole lifetime.

*Next (before any scored row).* One kept diagnostic row reconciles skbs,
bytes and datagrams:
- the engine's fed loss against `[TRUTH]`;
- unfiltered taps on `cli0` egress and `srv0` ingress, diffed per skb;
- counter deltas over the capture window (qdisc, clsact, `ip -s link`,
  `softnet_stat`);
- `qdisc_pkt_len` and bstats from v7.0 `net/core/dev.c`, with orig_len,
  IP length and `uh->len` dumped for multi-segment skbs.

Whatever the cause, the scored capture becomes unfiltered, and every skb on
the leg counts toward a train's span, since netem's rate server spaces them
all. The battery launches only after the row reconciles within the
witnesses' 1 %.


### 36. Amendment 6 — the capture gap diagnosed; the capture and witnesses changed (after check rows, before any scored row)

*Cause.* A tcpdump stopped within about 1 s of the last packet loses its last
partial AF_PACKET block. That was a contiguous tail of 734–748 skbs, about
0.45 s at c2, which is the whole ≈ 5 % gap. Stop-timing sweep, filtered tap
short by: 735 (0 s), 8 (0.5 s), 13–14 (≥ 1.2 s). The residue is the
non-UDP skbs the filter excluded. The datagram gap was a separate
artefact: the engine's gso_size varies per skb (multiples of 1 252: 6 050
skbs; of 1 253: 3 063; others 2 746), so one m over-counts segments by
≈ 4 % while moving bytes by only 0.15 %.

*Reconciliation (one drained c2 row, `RWM_STORE=128`, deltas over the
capture window).*
- An unfiltered `cli0 -Q out` tap and an unfiltered `srv0 -Q in` tap match
  skb for skb: 18 328 each, the same count as Δ(clsact hits) − Δ(netem
  drops). Zero skbs were lost between netem and the receiver.
- The `srv0` stamps follow `cli0` by a median of 2.5 µs, so both taps sit
  after netem.
- Σb matches netem's Δ Sent bytes to +0.15 %, which is exactly the phantom
  segments × 42 B. Linux 7.0 `qdisc_pkt_len_segs_init` charges
  `len + (segs − 1)·42`.
- So `[TRUTH]` and netem's counters are right; only the tap was short.

*Changes (harness `7c3c9b7`).*
- The scored capture is unfiltered inbound on the leg device (`-Q in`).
  Every skb of any protocol is a record with b = orig_len for single-segment
  skbs, and it counts in train spans, since netem's rate server spaces them
  all.
- m is the payload carrying the most bytes. The stop waits for netem's
  backlog to drain (≤ 5 s), then settles 1.5 s.
- The witnesses compare windowed deltas from `[SNAP]` start/stop snapshots:
  bytes against Δ Sent bytes, skbs against Δhits − Δdrops − Δbacklog. The
  1 % tolerance is unchanged.

*Fresh check rows (production capture, not scored).*
- c2: skbs 1.0004, bytes 1.0016, kdrop 0.
- c8: leg 0 at 1.0000 / 1.0010, leg 1 at 1.0000 / 1.0028, kdrop 0.
- All witnesses pass. The battery may launch.

### 36. Amendment 7 — a named re-score for small-skb trains (before any score exists)

About 27 % of c2 skbs are 36–40 B client datagrams. A k = 4 train of them
spans microseconds, so the noise bound J/(k·b/R) that the scored estimator
assumed does not hold for it, and a per-window max picks the shortest span.
The c2 check row's L̂₄ of 1.07 is consistent with that.

The scored estimator is not changed. Its re-score is named now, before
`score.txt` exists. That re-score runs on the kept per-skb tables (offline,
no VM). It uses a byte-floored train: from each anchor j, extend until
Σb ≥ 4 × the row's byte-weighted median skb size. It feeds D1 and D2
unchanged. It is reported beside the scored k = 4 result, and a verdict
from it is a new registration.

*Run tree.* `diag36` carries only docs and tools beyond main: the diff
against `4926822` outside `raptorpath/docs` and `raptorpath/tools` is empty,
so the engine is main's.

### 36. Result

Scored 2026-11-10 against the pre-registration and amendments 1–7,
literally (`docs/l1-raw/disp36/score.txt`, the scorer's output). Diagnostic:
nothing ships. **`NOT-YET`.** At k = 4, D1 is `CIRCULAR` and D2 `NOISY` on
c1s, c2 and both c8 legs. No k in {2, 8, 16} passes either clause.

*Session.* Launch 00:28:52Z; precondition met (`procs_running`
[1,1,1,1,1], no cargo/rustc); both locks; scorer gate ok; build 314 s
(binary `79572edd…edfff5`, engine = main `4926822`, tree `5d4b12a`); smoke
`SMOKE-PASS`; battery 00:34:58–00:59:29Z. 58/58 rows LIVE, all rc 0, io
`full avg10` ≤ 0.61. Abort table: none fired.
- The c1s CAP768 s42 row failed its first parse: `CAP768` was missing from
  the parser's arm list, a harness defect fixed in `d3c3045`. It was
  re-parsed from its kept pcaps and logs. The envelope stamped
  `FAILED-ALL` at 57 rows; `ANNOTATIONS.txt` records this.
- The /dev/shm binary that vanished in the check rows was logind's
  `RemoveIPC`: user-owned tmpfs files are purged at the user's last logout.
  The run tree and binary are now root-owned, and the battery has an
  `ABORT-BIN` check before each row.

*Readings (Q = estimate ÷ link, means over the 2 rows per cap; full tables in
`score.txt`).*

| cell / leg | scored max, k = 4 | window median, k = 4 | byte-floored max (am. 7) | in-run share |
|---|---|---|---|---|
| c1s | 0.11 (16) … 1.71 (256), 15.6–128 (512–1 024) | 0.06 (16), 0.48 (128), 0.95 (256), 1.00 (≥ 512) | 0.08 (16), 1.01 (64), 2.1–125 (≥ 512) | 0.10 (16) → 0.78 (2 048) |
| c2 | 0.23 (16), 1.01–1.09 (≥ 32) | 0.13 (16), 0.47 (64), 1.00 (≥ 128) | 0.18 (16), 0.38 (32), 1.00–1.02 (≥ 64) | 0.25 → 0.74 |
| c8 fast | 0.22 (16), 1.01–1.06 (32–128), 1.4–8.8 (≥ 256) | 0.13 (16), 0.99–1.00 (≥ 128) | 0.18 (16), 1.00–1.04 (≥ 64) | 0.24 → 0.77 |
| c8 slow | 0.01–0.30 (≤ 128), 1.02–1.15 (≥ 256) | ≤ 0.06 (≤ 128), 0.33 (256), 1.00 (1 024) | ≤ 0.15 (≤ 128), 1.00 (≥ 256) | 0.09 → 0.78 |
| c1d | 1.2 (128), 6.8–20.8 (512–2 048) | 0.22 (128), 0.85–1.00 (≥ 512) | 1.0–2.4 | 0.19–0.68 |

- **D4 (capture cost):** none. CAP − NOCAP goodput at c1s: +1.5 % (256),
  −0.2 % (1 024), +4.9 % (2 048).
- **D5:**
  - c2 at L̂ has a STABLE fixed point at 172 (oracle 160), 0.87 of Gmax.
  - c1s and c8 at L̂ run to CEILING, because the max over-reads.
  - The oracle points are reproduced: c1s 614 (0.945 Gmax), c8 324
    (0.89 Gmax).
- **D6:** the dwell `rtt − wrtt` at c1s grows from 1 ms (≤ 256) to 1.8 /
  2.8 / 4.0 / 13.5 ms (512 / 768 / 1 024 / 2 048), with quinn's backlog B
  at 80–96 from 512 to 1 024. At the c1s point the time term carries
  ≈ 2 ms of the law's own dwell. The R̂ column is not usable: the field
  mixes units, and qcwnd medians reach 10⁴–10⁵ on some caps.
- **D7/D8:** c8 slow-leg share is 1.4–2.1 % at 16–128 with fresh prices
  (own rtt/wrtt 1.0–1.1, the placement law), and 4–8 % at 256–512 where
  one row in two is a lock-out (own rtt/wrtt 6–8). c1d 512 has one leg at
  0.7 % with rtt/wrtt 28.8, a lock-out in both rows.

*Predictions.* c2 and c8 `FLAT-AT-LINK`: MISSED; they read the link only
from cap 32–64 (max) or 128 (median) up. c1s `QUIET` with an `OVER-READ`
risk: MISSED (`NOISY`), and the over-read is far larger than predicted.
c1d reads ≈ L per leg: MISSED (the max over-reads ×7–21). D4 < 5 %: MET.

*What it means (outside the decision rule, no verdict).*
1. **The premise holds only above a cap.** Netem's busy runs exist only
   when the store holds enough to queue at the link. Below that, every
   estimator falls toward the delivered rate. This is §23's circularity,
   one layer down, and the floor trap the amendment-1 caps were added to
   test. The crossover is ≈ 64 (c2, c8 fast, max), ≈ 128–256 (median), and
   ≈ 256 on c8's slow leg.
2. **Above the crossover, the median reads the link.** The window median is
   1.00 ± 0.01 at every leg once its in-run share passes ≈ 0.6. The max does
   at the 100 Mbit/s legs (1.00–1.04 byte-floored). At 1 Gbit/s it
   over-reads by up to ×125, because receive stamps of skbs drained in one
   batch sit microseconds apart.
3. **So dispersion is a bounded-region source.** It reads L between a floor
   crossover and the knee, and the law's oracle fixed points (c2 160, c8
   324, c1s 614) lie inside that region. A law fed by it would still need a
   way out of the floor, which is the §27 exploration question. It also
   inherits ≈ 2 ms of its own dwell in S at c1s (D6).
4. Re-scoring with the median, or with a byte floor plus the median, would
   be a new registration on the kept tables.

### 36. Result — addendum (exploratory, post hoc on the seen ledger)

Both registered estimators failed: the k = 4 max (scored) and the
amendment-7 byte-floored max. What follows are leads, computed on the kept
tables by `tools/l1/disp36_explore.py` (output in
`docs/l1-raw/disp36/explore.txt`). They are not a verdict. A pass needs a
fresh registration with one estimator fixed, plus an aggregation cell (netem
`slot`) and a policer cell (`tbf`).

- **The p90 variant (registered as a readout, omitted above).**
  - It never exceeds 1.032 of the link anywhere, including c1, where the max
    reads ×125.
  - It reaches the link one grid step below the median: from 128 at c1s,
    64 at c2 and c8 fast, and 256 at c8 slow.
- **Within-row noise of p90 and the median is large.** c1s cv is 0.29–0.56
  and c2 0.19–0.34; c8 fast reads 0.002–0.21. In many 250 ms windows the
  link forms no busy run, so the per-window value is bimodal (link or
  delivered rate). The median over windows is stable across caps and rows.
  The within-row cv measures how often busy runs form, not estimator error.
  D2 as registered does not separate the two.
- **The floor (correcting "a law would still need a way out of the
  floor").**
  - Below the crossover, L̂/D (D = the delivered rate from the same table)
    is 1.6–3.9 for p90 at every sub-crossover cap of every leg, and
    1.06–1.45 for the median (0.72–0.97 on c8's slow leg at 16–128).
  - At small caps Tmin ≈ RTprop, so RTprop + S ≈ Tp50 ≈ T, and the law's
    map gain is g ≈ (1 + q)·L̂/D. That is ≈ 1.7–4.3 for p90: the map expands
    out of the floor, so there is no trap.
  - For the median, g ≈ 1.2–1.6 holds on the active legs. On c8's slow leg
    it is ≈ 0.8–1.07: marginal there, but that leg's share is set by
    placement, not the store.
  - The boot cap 128 is at or above the p90 crossover on every leg except
    c8 slow.
- **The cause of the 1 Gbit/s over-read is netem's release, not receive
  stamping.**
  - 64-skb blocks span as little as 0.895 of their ideal b/R time at c1s
    1 024–2 048 (≥ 0.96 on the 100 Mbit/s legs).
  - Compressed k = 4 trains (span < ½ ideal) are 0.3–0.9 % at c1 against
    ≤ 0.15 % elsewhere.
  - The c2 two-tap check put the tap pair within 2.5 µs.
  - So at sub-ms scale the c1 link releases late in batches: c1 here already
    behaves as an aggregating link, which is the §37 caveat for real links.
- **Dwell in the time term: unsettled.** `rtt − wrtt` is dwell in T, not in
  S: dwell present in every sample moves Tp50 and Tmin together. It reads
  ≈ 1 ms at the small caps of every cell (an offset). Whether S at c1s 614
  carries any of it needs the µs fields of the kept client logs. Each 1 ms
  truly in S moves the c1s point by ≈ 230.
- **D5's oracle points recompute the location** from the same cross-session
  RTprop and S, so they reproduce by construction. Only the goodput at them
  (c1s 0.945, c2 0.85, c8 0.89 of Gmax) is new.

*Lead for a fresh registration (not made here).* The per-window p90 of
k = 4 train rates, the median over windows, cells c1s, c2 and c8, plus
`slot` and `tbf` cells. It is scored on D1 with a noise clause defined on
the median-over-windows' across-row spread.
