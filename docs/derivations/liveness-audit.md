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

**Counts** (`scripts/count.py` over this table). There are 65 engine rows × 7 seats = 455 cells. Excluded: the 8 n/a-everywhere rows (C03, C13 + M01–M04 as one line, F07, K10) and the 12 n/a cells of E13/E14 off the E seat. That leaves 443 classified:

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

(filled below)

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

## 3. Ranking

(filled below)

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
