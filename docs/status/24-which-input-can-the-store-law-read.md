## 24. Which input can the store law read? — loop time, quinn's rate, jitter — diagnosis — pre-registration

§23 found the store law to be a window controller fed by its own output:
on a store-bound sender any sampler of its own delivery reads cap/T_loop, so
`cap = (1 + q)·bw·RTprop` contracts to its floor on an honest rate and runs
to the knee on the over-read one (paper §6.1). The static sweep put the knee
near 3–3.5·RTprop·L. The coordinator's decision: no input is chosen by
reasoning; this instrument-only session on main discriminates the
candidates. No law changes, nothing ships or flips.

**Candidates and what would kill each (fixed now).**
- **Option 1, the min-filtered loop time** in place of RTprop, with the
  delivered rate. Killed if the min over a window of send→ack-processing
  times inflates with the standing queue as the cap rises past the knee:
  then the law ratchets by (1 + q) per round once a queue exists, and the
  only cure (BBR's ProbeRTT drain) is a mode, which CLAUDE.md forbids.
- **Option 3, quinn's own BBR rate** (`congestion_state().metrics()
  .pacing_rate`, = pacing_gain·max_bandwidth under the shipped quinn-proto
  0.11.14 BBR; the stored rate carries the gain at the read instant, which
  is 1 in six of ProbeBW's eight phases, so a window median reads
  max_bandwidth). Free and non-circular if it reads flat near the link rate
  at every cap; circular if it tracks the cap like ours; useless as a law
  input if it over-reads (the in-tree `bbr_rs` port documents an upstream
  ×10 over-read under burst clusters).
- `jitter_est` (Copa's consecutive-difference EWMA) is reported beside
  them as the candidate for the gap between the mean and the min loop time;
  it carries no outcome here.

**Instrument** (committed before this section, observation only, default
off): `RWM_LOOPDIAG=1` prints per path per `[DIAG]` window (250 ms)
`[LOOPDIAG] … n rtt_min_us rtt_p10_us rtt_p50_us rtprop_us jit_us srtt_us
dlv dlv_rate q_pace_bps q_cwnd q_srtt_us`: the RTT samples Copa's
`record_rtt` receives in that window (the loop time as the controller sees
it), Copa's RTprop, jitter and SRTT, the delivered count the sampler is fed
(batch granularity: one add per ack, no per-symbol work), and quinn's pacing
rate, cwnd and smoothed RTT read in the I/O owner's snapshot.

**Design.** One binary (this branch HEAD; engine = main plus the
instrument), built fresh under both locks, `sha256` per invocation. All
rows: Bulk, `--window-reliable`, shipped defaults, `RWM_GEN=0 RWM_DIAG=1
RWM_ACKDIAG=1 RWM_LOOPDIAG=1 RWM_PERF_TIMEOUT_S=150`, §23's env
withholding plus `RWM_LOOPDIAG`, `perf_rwm_c.sh`, one invocation per fresh
topology. Arms `MAIN` (no arm env) and `S<n>` (`RWM_STORE=<n>`, §23's
static sweep, its ground truth for the knee). Three blocks (42, 1), (7, 1),
(42, 2): `c1s-400` MAIN, `c2-100` MAIN, the `c1s-400` sweep n ∈ {192, 256,
384, 512, 768, 1024, 1536, 2048}, and in the first two blocks the `c2-100`
sweep n ∈ {64, 96, 128, 192, 256, 384, 512, 1024}; ascending on even
blocks, descending on odd. 6 + 24 + 16 = 46 invocations after a
2-invocation smoke (`c1s-400` MAIN, `c2-100` S192). Harness
`tools/l1/loopdiag_{run_all,battery}.sh`, parser `loopdiag_parse.py`.

*Witnesses* (a row without them is void): `[GATES]`; ≥ 1 `[ACKDIAG]`; ≥ 4
post-warm-up (t ≥ 1 s) `[LOOPDIAG]` p0 lines with n > 0; at least one
numeric `q_pace_bps`; MAIN `[SUMCAP] eng>0`; `S<n>` `[SUMCAP] eng` absent or
0 and every post-warm-up `win=` denominator equal to n.

*Abort table:* as §23 (`ABORT-LOCK`, `-CRLF`, `-SHA`,
`-SENTINEL-UNWRITABLE`, `-SMOKE`, `-BUILD`, `-TESTS`, `-RC`, `-BRINGUP`,
`VOID-COTENANT`), filled in the result.

**Readouts and outcomes (fixed now).** Per cell (`c1s-400`: L = 97 700
sym/s, link 125·10⁶ B/s; `c2-100`: 9 770 sym/s, 12.5·10⁶ B/s), over live
`S<n>` rows, per n the mean of each row's median over post-warm-up p0
windows; q = 0.1 (Bulk); W = 1 279 wire bytes per datagram.
- Knee `n*` = the smallest n with goodput ≥ 0.95·Gmax (§23's rule).
- **O1:** `Tmin(n)` = window-min loop time. `FLAT` if every Tmin(n) ≤
  1.25·Tmin(n_min); `INFLATES` if Tmin(n_max)/Tmin(n_min) ≥ 1.5;
  `AMBIGUOUS` otherwise.
- **O3:** `Q(n)` = quinn pacing rate (bytes/s) ÷ link. `FLAT-AT-LINK` if
  every Q(n) ∈ [0.8, 1.25]; `OVER-READ` if the median over n of Q > 1.25;
  `CIRCULAR` if Spearman(Q, n) ≥ 0.8 and Q(n_min) < 0.8; `NEITHER`
  otherwise.
- **Reproduction** (asks in symbols; `REPRODUCES` iff in [n*/1.5, 1.5·n*]):
  option 1's equilibrium `n_fp1` = the smallest grid n with
  1.1·D(n)·Tmin(n) < n (D the delivered rate); option 3's
  `C3 = 1.1·(Q·link/W)·RTprop` (median over n) and `C3b` with Tmin in place
  of RTprop; `CJ = 1.1·L·(RTprop + jit)` reported only.

**Decision rule (ships nothing).** An option **passes** iff its kill test
is favourable (O1 `FLAT`; O3 `FLAT-AT-LINK`) **and** its ask `REPRODUCES`
at `c2-100` (the clean knee in §23) **and** at `c1s-400` it asks ≥ 384 (the
grid point below which §23's goodput fell by ≥ 15 %; c1s's knee by the 0.95
rule was 2048 on a non-monotone curve, so it is a floor, not a band). If
both pass, option 3 is preferred (no state of the law's own). If none
passes, the report names the next option and its failure mode; nothing is
built either way until the coordinator decides.

**Predictions (before any number).** O1 `INFLATES` at both cells (the
window-min tracks the standing queue once the cap exceeds the knee; §23's rtt
rises 14 → 103 ms at c2). O3: unknown; the stated risk is `OVER-READ`.
jit at c2 ≈ 1–3 ms, short of the ≈ 12 ms gap between the knee's loop time
and RTprop.

**Duration.** Build ≈ 6 min; 48 invocations ≈ 14 min (§23: 64 in 1 138 s);
well inside the 4 h launch deadline and the 5 h cap.

### 24. Amendment (before any scored result is recorded): the first session is `VOID-IOSTALL`; relaunch under an I/O precondition

The first §24 session (binary `10c0fa64…d7ba68`, tree `33e8dc1`) waited on
the then-current precondition (locks, load1 < 2.0, no cargo/rustc), met it
at 18:41:30Z and finished `LD-ALL-DONE` at 19:41:27Z, 46/46 rows LIVE. It
ran inside the VM's storage stall (from ≈ 17:55Z; `/proc/pressure/io` `full`
avg60 77–84 % when read at 20:13–20:17Z, 75–85 % at 21:13Z), which the
load average did not show. Direct evidence in its own ledger: the fresh
build took 3 037 s against 310 s in §23 on the same tree size, and the
c1s rows read low against §23 at every comparable point (MAIN 507 against
662 Mbit/s; the sweep 360–700 against 446–727, one `S1536` row at 220).
Per-row I/O pressure was not recorded, so no row can be shown clean: **the
session is `VOID-IOSTALL` as a whole**, its score is not recorded as a
result, and its ledger is kept for the record at
`docs/l1-raw/loop-diag/void-iostall/` (the c2 rows happen to sit on §23's
curve; that is noted, not scored).

Changes, nothing else in §24 moves: the launch precondition adds
`/proc/pressure/io` `full avg60` < 5.0 (shared rules); every row records
`full avg10` before and after (`IOPRESSURE` ledger line) and a row whose
larger reading is ≥ 20 is `VOID-IOSTALL` (parser), counted in the void table
like any other void; the `era` lines record I/O pressure. Same binary tree
plus this harness change, rebuilt fresh; same 46 + 2 invocations.

Amendment 2 (before launch; the relaunch had not started): per the shared rules, the run dir and the measured binary live on tmpfs (`/dev/shm/ld24b`); the build stays on disk; the run dir is copied to disk after the last row (and on every failure path). Nothing else changes. The driver's own `/tmp` logs (`perf_rwm_c.sh`, shared) stay where they are.

Amendment 3 (before launch; supersedes amendment 1's precondition and its per-row void, shared rules as approved by the user): the launch precondition is both locks free, no cargo/rustc (`pgrep -x`), and `procs_running` in `/proc/stat` ≤ 2 in each of 5 samples 2 s apart; neither loadavg (it counts disk-blocked tasks) nor I/O pressure gates the launch. I/O `full avg10` before and after each row is recorded as a covariate and a row at ≥ 20 is flagged in the score (`IO PRESSURE` list), never voided. A row whose binary or run dir is not on tmpfs is `VOID-ONDISK`. The run dir and binary are on `/dev/shm` (amendment 2); `/tmp` on the VM is tmpfs too, so `perf_rwm_c.sh`'s logs are already in memory. The first session stays void: its binary and run dir were on disk, so it is `VOID-ONDISK` under this rule as well as having run in the stall.

### 24. Which input can the store law read? — result

Scored 2026-10-26 against the pre-registration and amendments 1–3,
literally. Diagnostic: nothing ships. **No candidate passes.** Option 1
(the min-filtered loop time) `INFLATES` at both cells and its equilibrium
sits at the bottom of the grid; option 3 (quinn's BBR rate) `OVER-READ` at
`c2-100` (×7.6 median) and `NEITHER` at `c1s-400` (≈ 1.0× the link), and its
ask `MISSES` the knee band at both cells.

*Session.* Binary `10c0fa64…d7ba68` (the same engine as the voided first
session; tree `97cc1f3`, harness changes only), built on disk (1 737 s under
the storage stall; builds are not gated), run from `/dev/shm/ld24b`.
Precondition (amendment 3) met 20:28:42Z; locks held 20:29:54–21:07:17Z;
smoke 20:59:11Z; battery 485 s, 46 rows; exit 0 `raptorpath`, 0 `rp-*`;
run dirs removed. Ledgers: `docs/l1-raw/loop-diag/final/`.

*Abort table.* No token fired (`SMOKE-PASS`, 46/46 rows LIVE, no
`VOID-ONDISK`, no retry).

*I/O covariate.* 31 of 46 rows read `full avg10` ≥ 20 (20.5–67.8) and are
flagged. The c2 rows sit on §23's curve at every cap (37.5 / 53.7 / 68.6 /
87.2 / 90.4 / 89.8 / 89.7 / 88.8 against §23's 37.7 / 54.2 / 70.2 / 89.2 /
89.2 / 89.8 / 89.9 / 89.8), so pressure did not move them. At c1s one
flagged row is an outlier: MAIN s7 r1, 436.6 Mbit/s at 43.3 % against the
other two MAIN rows' 637–660 (flagged, kept, as amendment 3 says); one
unflagged `S1536` row reads 382 (§23 saw the same cap's spread, 485–754).

| c2-100 n | 64 | 96 | 128 | 192 | 256 | 384 | 512 | 1024 |
|---|---|---|---|---|---|---|---|---|
| goodput | 37.5 | 53.7 | 68.6 | 87.2 | 90.4 | 89.8 | 89.7 | 88.8 |
| Tmin / Tp50 ms | 9.5 / 15.0 | 10.2 / 15.6 | 11.3 / 16.1 | 13.7 / 19.2 | 19.7 / 25.4 | 32.0 / 38.2 | 44.5 / 51.1 | 95.3 / 102.7 |
| RTprop ms | 7.3 | 8.1 | 9.3 | 10.0 | 11.1 | 10.7 | 12.3 | 13.0 |
| quinn rate / link | 3.1 | 4.7 | 4.2 | 7.5 | 20.3 | 7.8 | 12.6 | 9.0 |

| c1s-400 n | 192 | 256 | 384 | 512 | 768 | 1024 | 1536 | 2048 |
|---|---|---|---|---|---|---|---|---|
| goodput | 402 | 475 | 554 | 589 | 634 | 663 | 631 | 740 |
| Tmin / Tp50 ms | 2.6 / 3.7 | 2.7 / 4.2 | 3.1 / 5.3 | 3.5 / 6.7 | 4.3 / 9.3 | 5.2 / 11.6 | 15.1 / 26.6 | 11.7 / 20.3 |
| quinn rate / link | 1.06 | 1.21 | 0.89 | 1.10 | 1.01 | 1.02 | 0.75 | 1.58 |

Knees by the 0.95 rule: c2 `n*` = 192 (band [128, 288]); c1s `n*` = 2048
(band [1 365, 3 072]; the curve rises through receiver batching, as in §23).

- **O1 `INFLATES` at both** (Tmin ×10.0 across the c2 grid, ×4.5 at c1s).
  The window-min loop time is the standing queue plus a floor: at c2 cap
  1024 the min is 95 ms against a 13 ms RTprop. Option 1's map C1(n)/n stays
  < 1 at every grid point (0.66–0.99 at c2, 0.42–0.72 at c1s), so its
  equilibrium is the grid floor (`n_fp1` = 64 / 192): the honest rate times
  a min-filtered time still contracts. Option 1 is out; its only cure,
  a periodic drain to re-read T₀ (BBR's ProbeRTT), is a mode.
- **O3:** quinn's pacing rate is not circular at c1s (Spearman −0.07,
  ≈ 1.0× the link from cap 192 up) but over-reads ×3–20 at c2 (median
  ×7.6, Spearman 0.81 with the cap), the jittered cell, as the `bbr_rs` port
  documents for upstream's estimator. Its asks miss both bands: C3 = 299
  at c1s (≈ 1.1·L·RTprop, the honest BDP, far below c1s's knee) and 851 at
  c2 (×3 the band's top); with Tmin (C3b) 436 and 1 890.
- **Jitter (reported).** `jitter_est` reads 0.05–0.11 ms at c2 and
  0.002–0.014 ms at c1s, two orders short of the gap; CJ = 112 at c2 and 280
  at c1s, both miss.

*Predictions.* O1 `INFLATES` at both: MET. O3 stated risk `OVER-READ`: MET
at c2; c1s read near the link (not predicted). Jitter short of the gap: MET.

*What the data points to (outside the pre-registered set, no verdict).*
Two readings are not candidates of this section and are recorded for the
next one. (1) The spread `Tp50 − Tmin` of the loop distribution is nearly
cap-independent at c2 (4.8–7.4 ms over a 16× range of caps, while Tmin
moves 10×), and `1.1·L·(RTprop + (Tp50 − Tmin))` gives 152 at n = 128 and
166 at n = 192 — inside c2's band; at c1s the spread grows with the cap
(1.1 → 11.4 ms), but slowly enough that the same expression has a fixed point
near 800 (1.1·L·(2.79 + 5.0) ms ≈ 837 at n = 768), where §23 read 0.93·Gmax.
(2) That expression needs the link rate L, which no candidate supplies at
c2: quinn over-reads there and our own samplers are circular.
