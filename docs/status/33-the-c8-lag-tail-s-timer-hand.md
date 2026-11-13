## 33. The c8 lag tail's timer hand-off component on current main — offline reading — result

§26 (a) read the c8 client `[LAG]` tail as MIXED: an OS part (about a
quarter of E) and a larger runtime part, named the timer hand-off. §28's
incremental SACK marking removed the sender's long polls but did not
re-test that part. Question: on current main (bb29595), is the timer
hand-off still a material share of the c8 client E = p99 − `min_us`, and
does it link to the c8 goodput TREND-WORSE of §28 (95.7 vs 101.5) and §32
(95.2 vs 96.9)? **Reading: NOT MATERIAL. No fix is designed, no VM
measurement is pre-registered, nothing ships.** This section reads
committed ledgers only; there was no VM contact.

*Data.* The per-tick `[LAGRAW]`/`[OSLAGRAW]` samples of the six §28 step-0
rows (SK binary, c8-100, RTOBS=2; smoke excluded) stand in for current
main; the 24 + 24 §26 rows (pre-SK MAIN 92934a7) are the contrast. The
proxy holds for the client's timer path: between the SK engine (320668c)
and bb29595 the client-side code changes are refactors only (`poll_once`
moved from `tx_inputs.rs` to `io_owner.rs`, an unused `publish` argument and
`is_dirty` removed); `emit_source.rs`, `sender_phases.rs`, `store_cap.rs`,
`diag.rs`, `ackdiag.rs`, `control_msg.rs`, `tasks/report.rs`, `gates.rs`,
`main.rs` and `runtime_obs.rs` changed in comments only; §31/§32 changed
the receiver and the decoders. Script: `tools/l1/lagfloor_read.py`
(read-only, no unit test: it gates nothing); output in `docs/l1-raw/lag-floor/`
(`sk-client.txt`, `s26-client.txt`, `s26-server.txt`). Battery numbers are
from `thread-sk/score.txt` and `thread-pd/battery/score.txt`.

*Finding 1 — about 1 ms of E is tokio's timer quantization, present with no
engine work.* tokio 1.50 rounds a deadline up to the next ms
(`deadline_to_tick`; that is φ, which `min_us` removes). It also computes
the park duration from a *floored* `now` in whole ms (`time/mod.rs`
`park_internal`: `tick_to_duration(when − now)`), so a worker that parks at
sub-ms phase f wakes ≈ f past the tick: a per-tick overshoot U ∈ [0, 1 ms)
that `min_us` does not remove. Measured: the §26 server's pre-transfer
samples (an idle process, ≈ 120 per row, 24 rows) read x = late − min at
median p50 518 µs and p95 999 µs, i.e. Uniform[0, 1 ms). So E ≥ ≈ 1 ms by
construction of the probe. §25's "per-tick U" is this; the module doc's
"0–1 ms late by construction" holds per tick, not only per process.

*Finding 2 — on the proxy, the c8 client is now at that floor plus a ~1 %
tail* (x over the transfer, medians over rows):

| quantity | c8 client, SK (≈ current main) | c8 client, §26 MAIN (pre-SK) | c8 server, §26 (control) |
|---|---|---|---|
| x p95 | **993** | 1491 | 884 |
| x p99 (≈ E) | **1554** | 3411 | 1514 |
| ticks with x ≥ 1500 µs | **1.4 %** | 4.9 % | 1.0 % |
| ticks with x ≥ 2000 µs | 0.8 % | 2.4 % | 0.7 % |
| ticks ≥ 1500 µs overlapping an OS-clock stall ≥ 300 µs | 13 / 57 (23 %) | 65 / 1163 (6 %) | 51 / 222 (23 %) |

So the current c8 client E (≈ 1.5 ms) is ≈ 1.0 ms of quantization floor
plus ≈ 0.5 ms from 1–1.5 % of ticks, of which about a quarter coincide with
an OS reference stall. The runtime hand-off proper is ≈ 0.3–0.4 ms of p99,
about a quarter of E, carried by ≈ 1 % of ticks. Because p99 sits on the
edge of that 1 % tail, a handful of ticks per row move E; that is the
spread (current main, §32 NEW: c8 client E 1328 [1088–2412]). The c8
client now reads like the §26 server under load, and on current main its E
is no higher than the other cells' (§32 NEW: c2 1262, c1s 2157). The
c8-specific tail §25 set out to explain went with §28.

*Finding 3 — none of the sender's timed arms is on the c8 path.* The sender's timed
arms at c8 (`[DIAG]` `wake[]`): `pace=0` on every line of every c8 client
row of §28 step 0 and of §32 (both arms), so the pacing poll never fires.
Current main (§32 NEW REPORT, medians per run): `paused` 1 331 against
`ack` 8 633, with `timer_acked` 23. A paused sender is woken by
the always-armed input arm when acks arrive; the 1 ms poll fires only in
true ack gaps. The tail sweep (≈ 66 fires per run) runs at 2 × SRTT, with
RTprop ≥ 9 ms here. A sub-ms hand-off therefore delays no sender store
release or emission measurably. Not examined: quinn's own timers (pacer,
loss/PTO) and the I/O owner's shim timer (`io_owner.rs`, armed only with a
non-empty shim queue) run on the same ms wheel.

*Finding 4 — no c8 goodput deficit is established, and none links to the
hand-off.* Post-SK c8 rows (§28 NEW, §32 MAIN, §32 NEW; n = 18): median
95.2 [81.9–103.9] Mbit/s. Pre-SK MAIN session medians span 86.2 (r1) to
101.9 (q1) (p2a 100.7, q2 96.0, d9 98.2, r2 95.7, sk 101.6), and rows
below 85 occur in d9, q2 and r1 before SK. The only same-session
comparison (§28) was TREND-WORSE with overlapping ranges. Spearman over the
18 post-SK rows (report-only): ρ(client E, goodput) = −0.33, ρ(server E,
goodput) = −0.43, against §25's −0.71. The four lowest-goodput rows (81.9,
83.2, 84.9, 87.0) carry client E 1238, 1835, 1103 and 2395.

*Why no fix.* The bulk of E is tokio's ms quantization. That is tokio's
design, and it touches no engine deadline at c8 (finding 3); removing it
would need a timer outside tokio's wheel, which the threading rules (§12,
the ADRs) exclude. The hand-off part (≈ 0.3–0.4 ms of p99, ≈ 1 % of
ticks) is below what §28's rule can detect at n = 6, since current main's
own c8 client E spans ×2.2. A tokio-knob change aimed at it (e.g.
`event_interval`) would be unscoreable, so none is designed.

*Limitations.* The per-tick reading is on the SK binary, not bb29595
(argued above), with n = 6 proxy rows. Current main's battery rows reach a
higher client E (§32 NEW max 2412, 2395) than any proxy row (max 1865), so
the proxy's tail may be thinner and the ≈ 0.3–0.4 ms hand-off figure is a
typical value, not a ceiling. The overlap test credits a tick to
the OS only when the OS reference thread stalled at the same time; an OS
stall that hit a worker but not `rp-osref` counts as runtime, so the
runtime share is an upper bound. Driver-side delay (no worker parked on
the driver) and run-queue delay (timer fired, task not yet polled) are not
separated; that needs a wake-vs-poll split of the probe and is not worth
building at this size.

*Recommended, not built.* The `[LAG]` E clause carries ≈ 1 ms of
quantization by construction, so it is blind below that floor and noisy at
p99 (one tail tick moves it). A future battery could score the share of
ticks with x ≥ 1500 µs beside E. That is an instrument-only change and
needs its own operator decision.
