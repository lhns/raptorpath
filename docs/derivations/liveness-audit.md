# Liveness audit: never executed, pinned, or decided but not wired

Read-only audit (2026-11-19), against main `44f54b94`. Its engine tree equals the §51 block 51 binary's base `d9dd18dd`: `git diff d9dd18dd 44f54b94 -- raptorpath raptorpath-math raptorpath-core gf256` is empty. Block E ran on `e-tree` `a271e0bb`, which is main plus `slaw-gauge` and the pacer. Committed logs only: no VM, no cargo, no fixes. The scripts are in the method section. Nothing here is a ledger verdict. Every PINNED or NEVER-RUNS reading below is a candidate for one (MEASUREMENT DISCIPLINE 17/18).

**Seats.**
- **c2**: c2-100, single path. **c8**: two legs. Both ran at ρ = 1, 600 MB, one perf object per row, six rows per arm. **B / A / R** are the named hints H-B / H-A / H-R (`docs/l1-raw/s51b/full/b51c{2,8}-full.tgz`).
- **E**: block E's evicting seat, ρ < 1, Realtime, paced: PM (35 Mbit) with PL (700 sym/s) noted, five rows each (`s51e-session.tgz`).
- §65 (`s65-session.tgz`, c2 Realtime 100 MB, RB/RC, 52 rows) was used as a replicate for the c2 R column. It agrees with it on every field read here (a* = W on 100 % of ticks, rr = 0.5 on 10.5 %, nbud = 10 on 86–87 %, eb_bursts 0).

**Classes.**
- **EX**: the path ran and its output varies.
- **PIN**: it ran but sits at one clamp or constant on ≥ 90 % of ticks; the bound is named.
- **NR**: never runs, because its precondition did not hold.
- **NG**: no gauge; nothing printed decides it.
- **EX\***: varies, but sits at one bound on 70–89 % of ticks (near-pinned). It counts as EX.
- **n/a**: model or branch only.

Shares are per sender `[DIAG]` / `[SPAN]` tick (t ≥ 2 s, src > 0), pooled over the arm's rows. End-of-run counters are the last line per row; the median is shown, with the range in brackets.

## 1. Liveness table

| id | c2 B | c2 A | c2 R | c8 B | c8 A | c8 R | E | evidence (field = value) |
|---|---|---|---|---|---|---|---|---|
| E01 | EX | EX | EX | EX | EX | EX | EX | `[DIAG] rtt=` c2 A 41–289 ms (p50 245); E `[QCLK] srtt_us_mean` 14.3 ms |
| E02 | EX | EX | EX | EX | EX | EX | EX | per-path `pN:…rtt=` varies with E01 (`net/diag.rs:701`) |
| E03 | EX | EX | EX | EX | EX | EX | EX | `rtp_us` c2 A 10.4–221.8 ms (p50 208 on a ~10 ms base); E 8.5–11.1 ms |
| E04 | EX | EX | EX | EX | EX | EX | EX | `btlbw` c2 A 38.9k–155k sym/s against ~8.5k src; c8 R p50 669k; E PM 50k–112k |
| E05 | EX | EX | EX | EX | EX | EX | EX | `wpl_us` c2 A 22.7–267 ms; E 12.1–15.5 ms |
| E06 | EX\* | EX\* | EX\* | EX\* | EX\* | EX\* | EX | `pl=0.0000` on 82–88 % of c2/c8 ticks (Addendum 4 shape); E PM 76 %, PL 48 % |
| E07 | EX | EX | EX | EX | EX | EX | EX | `plu` c2 A 0.0015–0.70 (p50 0.019) |
| E08 | NG | NG | NG | NG | NG | NG | EX | σ² and burst are not printed on main; E K4 inputs `sigma2` 3.07–3.65 (PL) |
| E09 | NG | NG | NG | NG | NG | NG | NG | the stand-in is not printed; only the unused candidates `rvar/qsp/msd/tlag` are |
| E10 | EX | EX | EX | PIN | PIN | EX | NR | `[QCLK] site=receiver srtt_us_mean`: c2 7.5–14 ms, which is the reverse leg's own srtt; c8 B/A **50 000 µs seed on every row**; c8 R 35.7–46.1 ms; E has no receiver QCLK (refresh is reliable-only) |
| E11 | EX | EX | EX | EX | EX | EX | EX | `[QCLK] site=sender srtt_us_mean` c2 A 212–218 ms, c8 A 596–660 ms, E 14.3 ms |
| E12 | NG | NG | NG | NG | NG | NG | NG | no anchor gauge; both clamps have no bind gauge |
| E13 | n/a | n/a | n/a | n/a | n/a | n/a | NR | branch-only on main; on the E tree `[SLAW] W=0 nk=0`, so no window closed at the evicting seat |
| E14 | n/a | n/a | n/a | n/a | n/a | n/a | NR | as E13 |
| D01 | EX | EX | EX | EX | EX | EX | EX | δ per arm moves `[DCAP] q`/`b` and rr (dial) |
| D02 | EX | EX | EX | EX | EX | EX | NR | `[DCAP] b` = 2 / 1 / 0.5 at B / A / R (constant per seat by design); E has no `[DCAP]` (reliable-only store law) |
| D03 | EX | EX | EX | EX | EX | EX | NR | `[DCAP] q` = 0.1 / 0.0667 / 0.05 |
| D04 | EX | EX | EX | EX | EX | EX | EX | β = 1 at B is seen as rr = 0 on 100 % of ticks; β = 0 at A/R (rr > 0) |
| D05 | NR | NR | NR | NG | NG | NG | NR | N = 1 placement is trivial; at c8 nothing prints w_bw's effect |
| D06 | NR | NG | NG | NR | NG | NG | NG | at B the anchor term is weighted 0; elsewhere t_tail is not printed, and perf pins the base at 1e-5 (`main.rs:282-301`) |
| F01 | EX | EX | EX | EX | EX | **PIN** | NR | `[SUMCAP] pin` c2 0.78 / 0.83 / 0.86, c8 0.74 / 0.82 / **0.93** (ceiling N·2048 = 4096); E has no `[SUMCAP]` |
| F02 | EX | EX | EX | EX | EX | EX | **PIN** | ρ = 1 takes the reliable branch. E: store cap `win=…/1024` on 100 % of ticks, a constant (the live MODE-SWITCH) |
| F03 | PIN | PIN | NR | PIN | PIN | NR | NR | the law is the constant 64. `eb_end cap:` ends 0.1–0.2 % of bursts at c2 and 4–14 % at c8; `store:` ends the rest. R and E do not batch (`eb_bursts` 0) |
| F04 | NR | NR | NR | NR | NR | NR | NR | `[GATES] RWM_THREE_TERM=0`; control arms off |
| F05 | NR | NR | NR | NR | NR | NR | NR | `window_generation_coding: false` |
| F06 | EX | EX | EX | EX | EX | EX | NR | `srel=` c2 A p50 1.8k, c8 A 20.5k; absent at E (not reliable) |
| F07 | n/a | n/a | n/a | n/a | n/a | n/a | n/a | derivation only |
| R01 | PIN | EX | EX | PIN | EX | EX | EX | `[SPAN] rr`: B = 0 on 100 % (β = 1, χ = 0); c2 A 0.024–0.5 (p50 0.245); c8 R p50 0.001 (rr = 0 on 47 %: spare-clamped, finding 2); E PM 0.046–0.5 |
| R02 | NR | EX | EX | NR | EX | EX | EX | the 0.5 cap binds on rr = 0.5 shares of c2 6.7 / 6.6 %, c8 11.9 / 0 %, E PM 4.3 %. Which `max()` term wins: NG |
| R03 | NR | NG | NG | NR | NG | NG | NG | no P_fec or r_sat gauge (Addendum 3: live r_sat not printed) |
| R04 | NR | NG | NG | NR | NG | NG | NG | as R03 |
| R05 | NR | NR | NR | NR | NR | NR | NR | `[CHI] n=0` on every row of every seat |
| R06 | NR | NG | NG | NR | NG | NG | NG | no term-win gauge (`rate-fix-term-gauge`) |
| R07 | PIN | PIN | PIN | PIN | PIN | PIN | EX\* | `[SPAN] wsize=201` (keep-behind clip 200) on c2 96 / 95 / 100 %, c8 96 / 95 / 100 %; E PM 88.8 %, PL 22–112 |
| R08 | PIN | PIN | PIN | PIN | PIN | PIN | EX | inferred, since the period is not printed: `rtt=` > 20 ms on 100 % of c2/c8 ticks, so SRTT/4 > 5 ms and the 5 ms cap binds; E rtt < 20 ms on 100 % |
| R09 | NR | NR | NR | NR | NR | NR | NR | generation seat off |
| S01 | NG | NG | NG | NG | NG | NG | NG | D is not printed |
| S02 | PIN | PIN | PIN | PIN | PIN | PIN | EX | `a_star ≥ wsize` on c2 98 / 98 / 100 %, c8 97 / 97 / 100 % (span = W); E PM 59–123 < W on 100 %, PL 4–9 |
| S03 | PIN | PIN | PIN | PIN | PIN | PIN | PIN | `[SPAN] delta=Some(1)` on 100 % of lines, every seat (floor 1) |
| S04 | NR | NR | NR | NR | NR | NR | PIN | shed inert at ρ = 1. E `score.txt` ROW51E: `bud=0.0000` on all 10 PM/PL rows, yet `shed=181–1992` (VM: which budget admits them) |
| S05 | PIN | PIN | PIN | PIN | PIN | EX | PIN | receiver `[SHEDH]`: c2 `floor_frac=1` (60 ms); c8 B/A `mean_us=200000` exactly on every row (= f(50 ms seed), E10); c8 R 143–185 ms; E PM `floor_frac=1` (`dial_frac` 0.11). At ρ = 1 nothing consumes it |
| S06 | NR | EX | EX | NR | PIN | EX | EX | `plost=taper`: c2 A 47.9k–63.4k copies per row; c8 A ≈ 119k, P_lost ≈ 1 on the blocker, copy share 0.98 (Addendum 2); c8 R 640; E PM 10 197; B 0 (no debt at r = 0) |
| S07 | NG | NG | NG | NG | NG | NG | NG | no refresh-cause tally |
| S08 | NR | NR | NR | NR | NR | EX | NR | hint-keyed; needs N ≥ 2. c8 R p1 in-flight > 9 000 on 82.5 % of ticks |
| S09 | NR | NR | EX | NR | NR | EX | EX | packing hint-keyed: R src/byte 0.0020 (512 B), `eb_bursts` 0 |
| S10 | PIN | PIN | PIN | PIN | PIN | PIN | PIN | `[SPAN] owed=0.00` on 100 % of lines, every seat (pass-through) |
| C01 | NR | NR | NR | EX | EX | EX | NR | `mpr supp=…/…/law`: law = 0 at N = 1; c8 p50 11k–60k. `pf=` floor-bound count 0 at every seat (10 ms floor never binds) |
| C02 | EX | EX | EX | PIN | PIN | PIN | NR | c2: `fast=0` (packet threshold NR at N = 1), 20-gap horizon live (`gapdrop` p50 385–867). c8: SACK cap 64 binds (63.7 ranges/report), `fast` 47–15k. E: `fast=0`, `[SLAWC] gaptrunc=0` |
| C03 | n/a | n/a | n/a | n/a | n/a | n/a | n/a | branch only; `[SNAPV] stale=0` on main everywhere |
| C04 | **PIN** | **PIN** | **PIN** | NR | NR | NR | **PIN** | **`mpr supp=…/age/…` = 0 on 100 % of ticks**, c2 all arms and E, against ~13.5k fires per c2 row. The N = 1 age gate never suppresses (it always passes). It is NR at N = 2 (mp law) |
| C05 | EX | EX | EX | EX | EX | EX | EX | `supp=cool/…` c2 A p50 339k cumulative |
| C06 | PIN | PIN | PIN | PIN | PIN | PIN | EX | `[QCLK] site=sender w_us_p05=p95=100000` on every c2/c8 row (100 ms cap); E 25.6–31.2 ms |
| C07 | PIN | PIN | PIN | PIN | PIN | EX | NR | `[QCLK] site=receiver w_us`: c2 25 000 µs (floor) on every row; c8 B/A 100 000 (cap, from the 50 ms seed); c8 R 71–92 ms; E reliable-only |
| C08 | PIN | PIN | PIN | PIN | PIN | PIN | PIN | `[LATE] sampler_bind` c2 1.0, c8 0.92–0.95, E 0.98 (2 ms floor) |
| C09 | EX\* | EX\* | EX\* | EX\* | EX\* | EX\* | PIN | `nbud=10` on c2 87.6 / 89.4 / 83.0 %, c8 79.9 / 86.4 / 70.0 %, E PM 93.8 %, PL 97.3 %. E-tree `[SLAWC] c09_frac` 0.063 (the per-report cap) |
| C10 | NG | NG | NG | NG | NG | NG | NG | no margin gauge |
| C11 | NR | EX | EX | NR | EX | EX | EX | `[SPAN] debt` non-zero on c2 A 14 %, R 54 %; c8 A 7 %, R 31 %; E 46 %; B 0 on 100 % |
| C12 | NR | NR | NR | NR | NR | NR | NR | `[REQS] on=0`, `[REQ] on=0` |
| C13, M01–M04 | n/a | n/a | n/a | n/a | n/a | n/a | n/a | wasm model |
| K01 | EX | EX | EX | EX | EX | EX | EX | `cwnd=10000` (MAX_CWND) on c2 59 / 70 / 76 %; c8 sum = 20 000 on 0 / 21 / 79 % (per-leg NG); E PM 509–1884 |
| K02 | NR | NR | NR | NR | NR | NR | NR | wire mode off; the constant 0.5 is used |
| K03 | NR | NR | NR | NR | NR | NR | NR | opt-in off |
| K04 | PIN | PIN | PIN | NG | NG | PIN | PIN | c2 slack: `s51-evict` bracket s = 0 on 0.4–0.5 % and s < 0.5 on 2–5 % of ticks, so `min()` takes window_rate on ≥ 95 %. c8 B/A: no per-leg cwnd (bracket MISSING). c8 R pinned at spare 0.000–0.002. E SLACK φ = ζ = 0 (1 105 windows) |
| K05 | NR | NR | NR | EX | EX | EX | NR | `[ETA] cold_r=0` everywhere (cold clamp never binds); c8 p1 gets 17.5k of 490k placements |
| K06 | NR | NR | NR | NG | NG | NG | NR | constant T; no gauge of its effect |
| K07 | NR | NR | NR | NG | NG | NG | NR | repairs only, and coded is ~0.004/src at c8 |
| K08 | NR | NR | NR | NR | NR | NR | NR | `RWM_XPATH_REPAIR=0` |
| K09 | EX | EX | EX | EX | EX | EX | EX | `qcwnd` c2 A 0.46–3.26 MB; never binds (E `qcwnd/(cwnd·512)` 250–394) |
| K10 | n/a | n/a | n/a | n/a | n/a | n/a | n/a | branch |

**Counts** (scratch script `count.py` over this table, §4). There are 65 engine rows × 7 seats = 455 cells. Excluded: the 8 n/a-everywhere rows (C03, C13 + M01–M04 as one line, F07, K10) and the 12 n/a cells of E13/E14 off the E seat. That leaves 443 classified:

| class | cells | share |
|---|---|---|
| EX (incl. 13 EX\*) | 169 | 38 % |
| PIN | 81 | 18 % |
| NR | 121 | 27 % |
| NG | 72 | 16 % |

At the default hint (the c2 A and c8 A columns, 126 classified cells): EX 49 (4 EX\*), PIN 25, NR 28, NG 24. Only 45 of 126 law cells at the default hint execute and vary clearly.

**Where a "never fires / inert" reading exists, did its precondition hold where it was read?**
- **S06's withdrawn DEGENERATE(L1: never fires).** It was read on Bulk-only cells, where r = 0, so the copy branch's precondition (debt > 0) never held. Off Bulk it fires on every row. The withdrawal is right.
- **R01 / R05 / S10 / C11 "r ≈ 0 on every measured cell".** Bulk only (β = 1). The precondition held nowhere they were read. Off Bulk, R01 and C11 execute. S10 is PIN (owed = 0) even with rr ≈ 0.25, so S10's DEGENERATE(r = 0) is the wrong reason: it is pinned at every r.
- **C04 "inert at c1" (§3.4) and Addendum 4's open question** (can the srtt/2 age gate ever hold at N = 1?). The precondition (N = 1, diag on) holds on all 42 c2 rows plus 10 E rows, and the suppression count is 0 on every tick. The gate never holds. That answers the question with a count, not a code read: the shipped N = 1 recovery law is a constant `true`.
- **K02 "inert by default".** Wire mode is off on every seat, so the precondition held. Correct.
- **R05 "χ = 0 on the tunnel".** It holds in perf too (`[CHI] n=0` on all 152 + 12 rows). Correct.
- **K04 DEGENERATE(paced c2).** Read at E (PM/PL SLACK). It also holds unpaced at c2 (≤ 5 % of ticks). It binds only when a leg is overdriven (c8 R, E U). The reading generalizes.
- **M04 "ρ inert at Auto and Realtime"** is model-only; no engine reading exists.
- **F03 "binds 0.005 at c2".** It holds: 0.1–0.2 % of bursts end on the cap. The store, not the burst bound, ends bursts at c2 (99.9 %).

## 2. Wiring table

### 2a. Accepted ADRs

Process ADRs (0042, 0051, 0052, 0073) and test infrastructure (0031) are n/a. 0068 and 0076 are Proposed and are skipped; 0076 phase 6 is the same work as 0077's PARTIAL row. Classes:
- **WIRED-OFF**: a production path exists but sits behind a gate that is off by default.
- **SUPERSEDED (code-only)**: the code changed without a superseding ADR.

Paths are under `raptorpath/src` unless named.

| ADR | decision | call site | class | note |
|---|---|---|---|---|
| 0002 | BE u16 length framing | `net/framing.rs:34` (now LE) | SUPERSEDED | by 0078 |
| 0003 | loss from batch-seq gaps | `net/receiver.rs:2145` → `scheduler/mod.rs:438` | WIRED | |
| 0003 | sender-side loss feeds the estimator | `net/control_msg.rs:625-637` → `control/estimator.rs:342-351` | PARTIAL | `RWM_LOSS_SENT_TRUTH=0`; unweighted per-ack mean (E06: 0.0000 on 82–88 % of ticks) |
| 0003 | periodic PathReport | `net/tasks/report.rs:200` | WIRED | |
| 0004 | drop the decoder on decode; `DECODER_TIMEOUT` | none | SUPERSEDED | the block map went with 0069 |
| 0005 | Ack / BlockResult / PathReport / Ping | `receiver.rs:2141`; `control_msg.rs:116,186` | PARTIAL | Ack suppressed under `ACK_MERGE=1`; BlockResult ignored |
| 0007 | echo-timestamp RTT | `control_msg.rs:153-161,189` | WIRED | |
| 0010 | RPTQ magic + version | `transport/protocol.rs:107,311,324-325`; `io_owner.rs:796` | WIRED | code is at v10; the README row says 8 |
| 0011 | large channels, TUN `try_send` | `net/delivery.rs:63,86` | WIRED | |
| 0012 | TOML, profiles, check, setup | `config.rs:140-161`; `main.rs:414,523` | PARTIAL | profiles do not reach perf |
| 0013 | stats, `/status` | `monitor/http.rs:52` | WIRED | `status` prints a PI term that is always 0 |
| 0014 | skip seen symbol ids | `fec/unified.rs:385-393` | WIRED | |
| 0015 | Ctrl+C flush + Shutdown | `net/mod.rs:3061`; `control_msg.rs:119` | WIRED | |
| 0017 | symbol size = min path `max_datagram_size` | `Scheduler::min_mtu` `scheduler/mod.rs:578`, **no caller** | UNWIRED | size comes from `symbol_size_for_hint` (`net/mod.rs:181,623`), which is hint-keyed and has no ledger row; SUPERSEDED (code-only) in practice |
| 0018 | `POST/DELETE /paths` | `monitor/http.rs:54-55`; `net/mod.rs:1290-1312` | WIRED | |
| 0018 | PathAdd/PathRemove peer messages | none | UNWIRED | |
| 0020 | SHA-256 cert pinning | `transport/quic.rs:960,1025` | WIRED | |
| 0023 | GE estimator feeds the rate law | `fec_rate.rs:456-497`, fed `estimator.rs:319-330` | WIRED | per-batch lumped feed; `record_symbol` (`estimator.rs:418`) has no production caller |
| 0023 | log burst multiplier | none | SUPERSEDED | by 0050; `ge_burst_factor` config is dead (§2b) |
| 0023 | GE in placement | `scheduler/place.rs:366` | WIRED-OFF | `RWM_PLACE_WDIV_DERIVED=0` |
| 0041 | SIMD gf256 | `gf256/src/lib.rs:97-113` | WIRED | |
| 0050 | NackAck message | none; RX loss from data batches | SUPERSEDED (code-only) | |
| 0050 | nack_effectiveness = (1 − ε_rx)² | `estimator.rs:130`, read only at `net/mod.rs:3316` | PARTIAL | ε_rx ≈ 1 in a one-way flow (inferred); it feeds only the floored NACK budget |
| 0050 | spare-capacity clamp | `net/emit_source.rs:574-580,589` | WIRED | `compute_repair_rate_capped` (`fec_rate.rs:561`) has no caller. Live: **PIN-slack at c2 and E** (K04), because spare reads PathState cwnd, which sits at MAX_CWND (K01) |
| 0050 | "BBR reduces source rate first when spare < needed" | none | UNWIRED | |
| 0050 | BOCD replaces EWMA + margin + PI | `estimator.rs:520` → `fec_rate.rs:491` | WIRED | |
| 0050 | PI is a no-op | `fec_rate.rs:411-414` | WIRED | dead call at `tx_inputs.rs:183`; `enable_pi_feedback` has no reader |
| 0050 | **Phase 3: proactive = total − nack_expected** | `BudgetAllocator::proactive_rate` `fec_rate.rs:832`, **called only from `control/fec_rate/tests.rs:629-650`** | **UNWIRED** | `controller_rate` (`raptorpath-math/src/lib.rs:735-830`) has no NACK term |
| 0050 | NACK repairs gated by the remaining budget | `net/mod.rs:3309-3337` → `sender_phases.rs:664,806,1098,1158` | PARTIAL | input is the heuristic `current_loss·0.5`, floored at 10 (`:3335`); `nbud=10` on 70–97 % of ticks (C09) |
| 0050 | hint → target_tail_loss | `fec_rate.rs:335-340` → `:504` → math `lib.rs:752` | PARTIAL | perf base always 1e-5; inert at B; no gauge (D06 NG) |
| 0050 | the BOCD quantile at the adjusted confidence IS the mechanism | `fec_rate.rs:491` (fixed 0.95) | PARTIAL | the target enters via z(δ_eff) (`lib.rs:764`); only the NACK budget uses 1 − target (`net/mod.rs:3315`); no superseding ADR |
| 0050 | codec overhead × P(decoder invoked) | math `lib.rs:756-758` | WIRED | |
| 0054 | `RWM_QUIC_CC`, BBR default | `transport/quic.rs:79-93,191-211` | WIRED | |
| 0054 | endstate: the hint price chooses the controller | none | UNWIRED | deferred to 0068 (Proposed) |
| 0055 | min_mtu = initial_mtu = 1350 | `transport/quic.rs:901-913` | WIRED | |
| 0056 | sparse-aware decoding | `net/mod.rs:3639-3641` | WIRED | |
| 0056 | systematic wire (generation arm) | `net/mod.rs:648` | WIRED-OFF | opt-in |
| 0059 | per-path RFC 9002 hole law | `sender_policy.rs:692-693`; `sender_phases.rs:770,892-931` | PARTIAL | N > 1 only. **At N = 1 the legacy srtt/2 gate (`sender_phases.rs:951-962`) runs and never suppresses** (C04: `supp age=0` on 100 % of ticks) |
| 0060 | SACK releases the slot, keeps the payload | `net/mod.rs:2241-2262,2303-2310` | WIRED | ρ = 1 only (F06 NR at E) |
| 0061 | A\* windowed-max send anchor | `emit_source.rs:559,610-611` | WIRED | E12 NG |
| 0061 | M\* plain subset, seed from first sample | `control_msg.rs:413-434`; `estimator.rs:284` | WIRED | the receiver-side srtt still reads the 50 ms seed at c8 (E10) |
| 0061 | RTprop floor + (M\*+2)·G backstop | `sender_policy.rs:347` | WIRED-OFF | generation only |
| 0061 | clock-gap witness | `control/anchor.rs:152-162`; `control_msg.rs:241,268` | WIRED | |
| 0061 | honest BtlBw max-deque | `gates/scheduler_gates.rs:102` | WIRED | E04 still reads 5–80× the link at c2/c8 |
| 0061 | rule 1 for placement (cold leg) | `gates/scheduler_gates.rs:164` | WIRED-OFF | `COLD_PLACE=0` |
| 0061 | `RWM_PLAIN_RS` | removed | SUPERSEDED | |
| 0062 | Copa wire signal | `gates/scheduler_gates.rs:41-54` | WIRED-OFF | K02 NR |
| 0062 | hint → δ = 0.5/ζ | `net/store_cap.rs:316-317` | WIRED | |
| 0062 | competitive mode | `control_msg.rs:396` | WIRED-OFF | as declared |
| 0063 | max(r\*, r_mass) | `gates.rs:396` → `fec_rate.rs:349` → math `lib.rs:786-798` | WIRED | which term wins: NG (R06) |
| 0063 | TaperBudget consumes r | `emit_source.rs:584-591` | WIRED | **pass-through: `owed=0.00` on 100 % of lines** (S10 PIN) |
| 0064 | one decoder | `net/mod.rs:3639-3641` | WIRED | |
| 0064 | span law A\*, Δ, D | `emit_source.rs:602-626` | WIRED | A\* ≥ W on 96–100 % (S02 PIN), Δ = 1 on 100 % (S03 PIN); "RTprop" is srtt |
| 0064 | M\* depth | `sender_policy.rs:332-347` | WIRED-OFF | generation only |
| 0064 | no mode switch | `net/mod.rs:181`; `sender_policy.rs:678`; `emit_source.rs:508-509` | PARTIAL | hint-keyed symbol size, packing, duplicate (T2 Realtime FAIL both cells); store cap fixed at 1024 at ρ < 1 (F02) |
| 0064 | δ-honest shed within 1 − ρ | `shed.rs:23-25`; `sender_phases.rs:846-869`; `emit_source.rs:770-797` | WIRED | inert at ρ = 1; at E the budget reads 0 (S04) |
| 0069 | block pipeline deleted | `net/mod.rs:385-392`; `config.rs:255-270` | WIRED | remnants: BlockStart/BlockResult variants, a dead RaptorQ/RS arm (`fec_rate.rs:323-326`), `RWM_UNIFIED=0` legacy decoders |
| 0072 | `n_live < 2` guard removed | `store_cap.rs:76-96`; `sender_phases.rs:109,191` | WIRED | |
| 0077 | formulas to `raptorpath-math` | `fec_rate.rs:161-164` → math `lib.rs:735` | PARTIAL | `span_forms.rs` is used only by the wasm; the engine keeps its own inline copy (`emit_source.rs:619-626`) |
| 0077 | decisions to `raptorpath-core` | `raptorpath-core/src/lib.rs` (14 lines) | UNWIRED | no crate depends on it |
| 0078 | one writer / one reader | `framing.rs:34`; `net/mod.rs:1396` → `receiver.rs:1014,1527,1623` | WIRED | |
| 0078 | `WindowStart.packed` removed | `protocol.rs:457` | WIRED | `use_packing` still hint-keyed (listed debt) |
| 0078 | wire v10; sender size in handshake; unframable closes | `protocol.rs:105,324-325`; `io_owner.rs:796`; `net/mod.rs:1399-1440` | WIRED | |
| 0079 | delete the taper copy branch | `emit_source.rs:743-822` | UNWIRED (pending) | live: `plost=taper` 48k–172k copies/row off Bulk; writes no `nack_retx_at` |
| 0079 | shed kept at the NACK site | `sender_phases.rs:846-869` | WIRED | inert at ρ = 1 |

**Counts:** WIRED 40, WIRED-OFF 7, PARTIAL 9, UNWIRED 7, SUPERSEDED 5 (two code-only), n/a 5.

**Recovery channels that ignore each other** (code-read):
- **A decode close does not cancel the pending NACK resend.** The gap-fire path (`sender_phases.rs:789-962`) checks shed, cooldown, the hole law or age gate, and `sent_store`. It never checks decoder or SACK state.
- **NACK spend is never credited to the rate law** (0050 Phase 3).
- **The taper copy and the NACK cooldown never see each other.** The copy writes no `nack_retx_at`.
- **The NACK budget is its floor by construction.** `nack_cap · source_symbols_this_period` resets every 10 acked seqs. The code's own comment says it "would truncate it to 0 almost always", so `.max(MAX_NACK_REPAIRS_PER_NACK)` (`net/mod.rs:3331-3333`) is the law. That answers Addendum 4's open question: the 10 is `MAX_NACK_REPAIRS_PER_NACK` used as a floor, and the 1 − target quantile feeding it is dead weight.

### 2b. Config fields and CLI flags

Flow: profile, then TOML, then CLI overlay (`main.rs:331-399`), then `config::resolve` (`config.rs:216`), then `net::run`. `cmd_perf` builds its own config from its own flags (`main.rs:282-301`) through the same `resolve`. **Perf never sees a profile or a TOML file**, and the global `--config` is parsed and dropped for `perf` (`main.rs:263`). Paths are under `raptorpath/src`.

| field / flag | tunnel | perf | engine reader | class |
|---|---|---|---|---|
| `target_tail_loss` / `--target-tail-loss` | `main.rs:362`, `config.rs:337` | **none: always 1e-5** | ×ζ(δ), clamp [1e-9, 0.1] `control/fec_rate.rs:336-337`; anchor `:504`; W\* `:547`; NACK-budget quantile `net/mod.rs:3315`, floored at `MAX_NACK_REPAIRS_PER_NACK` `:3333` | WIRED tunnel / **PERF-MISSING**; inert in the rate law at B |
| `max_fec_overhead` / `--max-fec-overhead` | `main.rs:363`, `config.rs:338` | **none: 0.5** | `fec_rate.rs:341,516` → `raptorpath-math/src/lib.rs:828` clamp. Bypassed by `RWM_MIN_R`, the generation floor, NACK repair and the S08 duplicate. No bind gauge | WIRED / PERF-MISSING |
| `inner_feedback_weight` / `--inner-feedback-weight` | `main.rs:384` | **none: 0.0** | `net/mod.rs:791` → `fec_rate.rs:398,514` | WIRED / PERF-MISSING |
| `protocol_hint` / `--protocol-hint` | `main.rs:364`, default `auto` `config.rs:238` | `main.rs:294` | `delta_price` `net/store_cap.rs:316`; ρ preset `config.rs:295`; `symbol_size_for_hint` `net/mod.rs:181`; packing `net/sender_policy.rs:678`; duplicate `net/emit_source.rs:509` | WIRED (hint-equality branches: S08, S09, symbol size) |
| `window_reliable` / `--window-reliable` (ρ) | `config.rs:293-295`, default `hint != Realtime` ⇒ **ρ = 1 at Auto** | `main.rs:295` | `net/mod.rs:616,627,1195`; `receiver.rs:161-172` | WIRED; the bare-bool flag cannot set ρ < 1 at B/A from the CLI (TOML only on the tunnel, unreachable in perf) |
| `--profile` Home / Datacenter | `main.rs:336-339` | none | via fields | WIRED tunnel only |
| δ (`RWM_DELTA`) | **env only** (`gates.rs:64`) | env | every δ-priced law; the hint-keyed branches keep the hint's value | no config surface |
| `enable_pi_feedback` | TOML → `net/mod.rs:782` | default true | **none**: `_enable_pi_feedback` ignored at `fec_rate.rs:316`; `pi_correction` hard 0.0 `:575` (`status` prints +0.000) | NO-READER |
| `ge_burst_factor`, `realtime_burst_extra` | parsed and merged (`config.rs:65-67,198-199`), **never reach PeerConfig, no warning** | – | none (docs promise default 0.10) | NO-READER (silent) |
| `reorder_timeout_ms` | TOML → `net/mod.rs:1282` | 20 | `receiver.rs:165-166`, only `!reliable && !ooo`; overwritten by `shed_recv_hold` once SRTT exists (`receiver.rs:792,1758`) | INERT at ρ = 1; mostly also at ρ < 1 |
| `reorder_max_size` | TOML → `net/mod.rs:1283` | 500 | `receiver.rs:166` (ρ < 1 only) | INERT at ρ = 1 |
| `window_out_of_order`, `_coded_only`, `_generation_coding`, `_systematic_repair` | the CLI overlay sets None but **a TOML value survives `merge`** (`main.rs:389-396`) | `main.rs:296-299` | `net/mod.rs:632,646-648,941,1208` | WIRED; the tunnel's in-order guard is comment-only |
| `fec_switch_*`, `fec_auto_switch` | warned and ignored, `config.rs:315-330` | – | none | NO-READER (intended) |
| `interleave_depth`, `mp_block_affinity` | bail naming ADR-0069 | – | – | OK |
| `server`, `bind`, `peer`, `tun_*`, `status_addr`, `route`, `dns`, `pin_cert`, `fec_backend` | wired | partly n/a in perf | read | OK |
| `completion_feed` | None on the tunnel | `perf.rs:273-277` under `RWM_COMPLETION_EXPOSURE` | `sender_phases.rs:1302-1314` | OK (χ = 0 on the tunnel forever) |

**Profiles at their own hint:**
- **Datacenter** (1e-6, 0.5, bulk) is inert in the rate law (β = 1, r = 0, measured on H-B: rr = 0 on 100 %). Its other readers are W\*, which is PIN at the 200 clip on 96 % of c2/c8 B ticks, and the NACK-budget quantile, which is floored at 10 (`nbud=10` on 80–88 % of B ticks). **So the Datacenter tail target has no measurable reader.** Its 0.5 cap equals the default and clamps 0.
- **Home** (1e-4, 0.3, auto) is live in principle (β = 0). The 0.3 clamp has no bind gauge. Unmeasured: perf cannot load a profile.
- `--target-tail-loss` ≥ 1e-3 at Bulk saturates the [1e-9, 0.1] clamp silently. The perf log prints the raw 1e-5, but the controller uses raw × ζ(δ).
- `set_protocol_hint` (`scheduler/mod.rs:596`) has no non-test caller, so "Auto-detect based on packet patterns" (`fec_rate.rs:47`) is false: Auto is the fixed point δ = 0.5.

## 3. Ranking (expected effect on the default: tunnel, Auto, ρ = 1)

**Basis.** These are the measured §51 medians at the named hints:

| | goodput (Mbit/s) | in-order delay | CPU/GB, client |
|---|---|---|---|
| c2 Bulk | 90.0 | 340 ms | 30.2 |
| c2 Auto | 76.1 | 403 ms | 41.8 |
| c8 Bulk | 97.6 | 4.7 s | 34.3 |
| c8 Auto | 83.0 | 5.5 s | 43.7 |

So Auto pays −15 % goodput, +18 % (c2) / +16 % (c8) delay and +39 % / +27 % client CPU against Bulk, and buys nothing at ρ = 1. That gap is the budget the first items share; their estimates are not additive.

**Coverage.** "Covered" names an existing roadmap node or ledger row. **New** means no node or ledger reading found.

### Pattern A: recovery channels that ignore each other

1. **S06 / ADR-0079: the copy branch is live.** Copies are 64 % (c2) and 98 % (c8) of proactive slots, at 4.5–7.3 copies per true loss.
   - **Estimate (Addendum 2 arithmetic):** c2 Auto 76 → about 85 Mbit/s and c8 to about 97 if the slots were not sent. CPU scales with them.
   - Covered: `rate-s06-plost-derivation`, ADR-0079.
2. **ADR-0050 Phase 3 unwired: the rate law gives the NACK channel no credit.** r ≈ 0.20–0.26 at ρ = 1, where ARQ alone delivers.
   - **Estimate:** after S06 the same slots become coded symbols, and goodput is unchanged (Addendum 2). The Auto−Bulk gap of about 14 Mbit/s and +11 CPU/GB at c2 is the upper bound of what r costs.
   - Covered: `rate-overprovision-verdict`, `rate-user-rate-law`. The unwired split itself is new as a node (Addendum 4 only).
3. **A decode close does not cancel the pending NACK resend.** Resends run at 1.02–1.03 per source loss while decode closes 42–46 % of holes at c2 Auto. retx/GB is 22.1k at Auto against 23.1k at Bulk.
   - **Estimate:** about 0.44 × retx/src 0.026 ≈ 1.1 % of wire is redundant at c2 Auto. More importantly, it is why coded repair buys no bandwidth.
   - **New** (status Addendum 4 text only).
4. **The NACK budget is its own floor** (C09, `nbud=10` on 70–97 % of ticks). It is constant by construction (`net/mod.rs:3331-3333`), so neither the target nor nack_effectiveness reaches it. Effect at Auto: unknown, and needs a VM arm with the floor removed.
   - Covered: `rec-fix-nack-throttle` (no gauge yet; this gives the reading).

### Pattern B: never executed or pinned

5. **K01: the scheduler cwnd sits at MAX_CWND 10 000 on 59–76 % of c2 ticks** (21 % / 79 % at c8 A / R, both legs). Because spare = cwnd − in-flight reads that cwnd, this pin is **why the spare clamp is slack at c2** (K04 PIN-slack, s < 0.5 on ≤ 5 % of ticks).
   - Effect: the "never hurts" clamp of ADR-0050 is inert at the default and binds only on an overdriven leg (c8 R finding 2, E U).
   - **New** for K01's bind. K04 is covered by `rate-spare-clamp-verdict`.
6. **S03 Δ = 1 on 100 % of lines, every seat.** Covered: `rate-delta-jitter-fix`.
7. **S02 A\* ≥ W on 96–100 % of ticks**, so the span is the whole 201-window off E, and the A\* law is pinned at W wherever W is the clip. **New** (the ledger has no bind fraction). Effect on decode-close latency unknown; needs a VM arm.
8. **R07 W = 201 on 95–100 % (clip 200), with the NACK-budget floor (item 4).** Together with D06 NG this means **target_tail_loss has no live reader at the default except the anchor term's z.** It also has no perf flag.
   - Covered: `rate-user-rate-law` (flag sweep, perf pass-through). R07's bind fraction is new.
9. **C04: the N = 1 age gate never suppresses** (0 of every tick, all c2 arms and E). The shipped single-path recovery law is the constant `true`.
   - Effect: every judged gap fires once cooldown allows. `[RFA] false_frac` is 0.08 at c2 A (6.4k of 80.6k), but whether that counts these fires was not verified.
   - Covered: `rec-fix-perleg-clock-n1`. This answers Addendum 4's open question.
10. **C06 / C07 / E10 / S05 pinned on seeds or clamps.**
    - Tail sweep at its 100 ms cap on every c2/c8 row.
    - The receiver refresh at its 25 ms floor (c2) or its 100 ms cap from the 50 ms seed (c8 B/A).
    - The receiver shed hold at its 60 ms floor (c2, E) or exactly 200 ms (c8 B/A, same seed).
    - Covered: `rec-defect-qclk-clamp`, `rec-defect-refresh-input`, `rec-fix-refresh-clock`. S05's pin is new, and it matters only at ρ < 1.
11. **S10 TaperBudget pass-through** (`owed=0` on 100 % at every r, so the "DEGENERATE(r = 0)" reason is wrong). Covered: `rate-defect-taper-inert`.
12. **F01 store cap at its ceiling** on 74–86 % of ticks, and 93 % at c8 R. Covered: `store-defect-cap-constant`.
13. **R08 cadence at its 5 ms cap** (inferred), **C08 sampler at the 2 ms floor** (0.92–1.0), **F03 burst bound** (a constant; it ends 0.1–14 % of bursts). Low effect. Covered: `meas-fix-gauges-p3`, `perf-fix-burst-bound`.
14. **E06 `loss_rate()` 0.0000 on 82–88 % of ticks.** It feeds worst-path selection, the E shed budget and nack_effectiveness. Covered: `rate-fix-estimators`.
15. **NR at every seat** (R05 χ, C12, F04, F05, R09, K02, K03, K08): opt-in or off by design. They are listed for completeness, with no effect at the default. R05 is covered by the ledger (DEGENERATE tunnel).

### Pattern C: decided or declared but not wired

16. **The perf client drops the dials.**
    - It has no `--target-tail-loss`, `--max-fec-overhead`, `--inner-feedback-weight` or profile, and `--config` is silently dropped. The logged 1e-5 is the raw base, not × ζ(δ).
    - Effect on the default tunnel: none. It blocks every measurement of those dials.
    - Covered partly: `rate-user-rate-law` (target pass-through). The rest is new.
17. **Dead config knobs:** `enable_pi_feedback` (no reader; `status` prints PI +0.000), and `ge_burst_factor` / `realtime_burst_extra` (silently dropped, docs promise 0.10). No behaviour effect; a user setting them gets nothing. **New.**
18. **δ has no config or CLI surface** (`RWM_DELTA` only). Auto is the fixed point δ = 0.5, and `set_protocol_hint` has no caller ("auto-detect" is false). **New.**
19. **The tunnel's in-order guarantee is comment-only:** TOML `window_out_of_order` / `coded_only` / `generation_coding` survive `merge`. Effect only if set. **New.**
20. **0077 `raptorpath-core` is empty, and the span law exists twice** (engine inline, and `span_forms.rs` used only by the wasm). There is no runtime effect, but the model and the engine can drift. Covered: `model-shared-core`.
21. **Hint-keyed paths** (S08, S09, `symbol_size_for_hint`, F02's 1024 at ρ < 1). Not the default, but T2 Realtime FAILs in both cells. Covered: `model-fix-hint-keyed-paths`, `model-mode-switch-debts`. `symbol_size_for_hint` also makes ADR-0017 UNWIRED (code-only supersession).
22. **Docs drift:** README 0010 says v8 (code v10). 0050's NackAck and 0017's MTU sizing were replaced without an ADR.

**Needs a VM reading to classify:**
- the NG cells (E09, E12, S01, S07, R03/R04/R06 term-win, D06, C10, K06/K07);
- the per-leg cwnd at c8 B/A (K01, K04);
- the effect of items 3, 4 and 7;
- S04 at E: `bud=0.0000` with `shed=181–1992` on every row; which budget admits those sheds is a code read plus a gauge.

## 4. Method and limits

**Method.**
- The tarballs were extracted into a scratch directory outside the repo. Four read-only python scripts (run with `-I`, paths as arguments; not committed, since this branch carries one doc):
  - `live.py`: per-key tick distributions of the sender's `[DIAG]` / `[SPAN]` (n, mode, mode share, p10 / p50 / p90, distinct count) per arm;
  - `endl.py`: the last line of every other tag per row, as median and range across rows;
  - `derived.py`: the paired shares (a\* ≥ W, rr = 0.5 and 0, cwnd = N·MAX_CWND, rtt > 20 ms, nbud = 10, p1 in-flight > 9 000, `eb_end` cap and store shares);
  - `count.py`: the class counts of §1.
- Gauge meanings were read from their format sites: `net/diag.rs:840-960` (`mpr`, `pf`, `supp=cool/age/law`), `net/late.rs:45-55`, `net/holddown.rs:305-320`, `control/fec_rate.rs:268`, `net/emit_burst.rs:160`.
- The ADR call sites were checked by a delegated code read; I spot-checked the load-bearing ones (`fec_rate.rs:316`, `config.rs:198`, `main.rs:263`, `scheduler/mod.rs:596`, `sender_phases.rs:957`).

**Limits.**
- **"A" is δ = 0.5 under the named Auto hint in perf**, with the target pinned at 1e-5, `max_fec_overhead` 0.5 and no profile. The tunnel at Auto has the same laws but was not measured; perf's `..Default::default()` fields are the tunnel defaults.
- **Single-object 600 MB rows.** Start-up and completion are 1–2 ticks of 250 ms and are excluded (t ≥ 2 s). A law that acts only at boot or at the tail can read NR or PIN here and still act once per connection.
- **The ≥ 90 % PIN line is per tick, pooled over rows.** Several cells sit at 80–89 % (EX\*). Moving the line to 80 % would move C09 and E06 to PIN.
- **R08 and part of S02 are inferred** from neighbouring printed fields (rtt, a_star against wsize), not from a printed bind counter.
- **c8 per-leg readings** are limited by the missing per-leg cwnd (`meas-s51-c8-gauges`). K01, K04 and D05 at c8 B/A stay NG.
- **The E column is PM**, with PL noted where it differs. U, the unpaced control, is cited only for K04.
- **No VM was used.** The table cannot classify rows that have no printed field (the NG cells). Items needing a VM reading are flagged in §3.
