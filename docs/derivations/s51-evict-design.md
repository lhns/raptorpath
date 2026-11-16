# s51-evict-design: one session for the off-Bulk δ sweep (§51) and the evicting-seat m̄ / W1 / K4 block (plan 59)

Status: DESIGN. No engine change in this commit, no VM, nothing registered. It merges two pending registrations:
- `s51-design.md` (branch `s51-design` `5a9dc401`, read at `7968c870`): the off-Bulk baseline and the δ sweep. Roadmap `meas-s51-*`.
- The evicting-seat registration that status §60 Amendment 2 asked for. Roadmap `rate-evict-registration`, `rate-spare-clamp-verdict`, `rate-spare-cwnd`.

Every `file:line` was re-grepped on `github/main` `1cd13f5c`, unless it is marked as a gauge-tree line (`slaw-gauge` `d416d59b`). Where this doc is silent, `s51-design.md` governs block 51, and status §60 §6 (`s60-registration.md` §6) governs block E.

## 0. The overdriven-load question, answered first

**Do §51's Realtime and Auto rows use the same unpaced offered load as the evicting row? The load is the same, but it is not overdriven the same way. Pacing is needed for block E only.**

- **The client is the same everywhere.** `perf.rs` `run_object` feeds chunks back to back: `mem.feed.send(pkt)`, bounded only by the run deadline. Backpressure comes only from the memory TUN's 8 192-chunk queue.
  - `perf_rwm_c.sh` passes `--bytes --runs` and no rate.
  - No pacing knob exists in `perf.rs` or the wrapper. The only pacer is engine-side, `RWM_CC_PACE` (§5.3).
- **What differs is the engine's admission gate.** TUN reads stop on `tx_paused = reliable && (store_len >= effective_store_cap || cwnd_full)` (`net/mod.rs:2397`; select arm `:2824`).
  - Every §51 arm runs `--window-reliable`, so the store cap holds the source to the ack clock.
  - The evicting seat (`RWM_C_RELIABLE=0`) has `reliable = false`, so nothing holds it. `cwnd_full` is false by default on the plain window: `brake_armed = eff_infl_cap > 0 || late_brake` (`:2324`). The inputs are `infl_cap` 0; `infl_bdp` gain 0 unless `gen_pipe` (`sender_policy.rs:756-759`), where `gen_pipe = gates.gen_pipe && generation` (`:342`) is false at `RWM_GEN=0`; and `RWM_LATE_BRAKE=0`.
  - At ρ < 1 the source therefore enters at CPU speed. quinn evicts the excess from its 4 MB datagram buffer (`transport/io_owner.rs:129-150`).
- **The records agree.**

| rows | seat | `[DIAG]` ticks with infl ≥ cwnd | `dgev`/`hand` at end | median `[SPAN] rr` | source |
|---|---|---|---|---|---|
| store-law-n1 c2 Realtime LAW r1–r3 (100 MB, October binary `8a43cd2`, `window_reliable: true`, `window_generation_coding: false`) | ρ = 1 | 2–4 % | 0.0000–0.0001 | 0.240 / 0.255 / 0.278 | `docs/l1-raw/store-law-n1/diag-sl/` |
| same, Auto LAW r1–r3 | ρ = 1 | 2 % | 0.0065–0.0091 | 0.133 / 0.145 / 0.219 | same |
| §60 c2 W 2 048 control (current-era Bulk, 600 MB) | ρ = 1 | 0 % (infl median 36, cwnd 10 000 on 62 % of ticks) | 0.0002 | 0 (β = 1) | `docs/l1-raw/s60/rep60a-diag-ld.tgz` |
| §60 60b rehearsal evicting row (Amendment 2, n = 1) | ρ < 1 | 48 of 52 (92 %) | not read; netem-derived 1 − 164/600 ≈ 0.73 (600 MB emitted, 164 MB sent by netem, headers included) | 0.000 | status §60 Amendment 2 |

(The store-law-n1 `[GATES]` line prints `RWM_GEN=384` as a default. The engine's own `PeerConfig` echo gives the plain-window, ρ = 1 reading above.)

- **Consequence for the design.**
  - §51's scored arms stay unpaced. Pacing them would make goodput equal the offered rate, and the goodput clause would go blind.
  - The paced load is built once, for block E. It is also a down payment on `meas-s51b-rho`, which needs a paced ρ < 1 load as well.
  - §51 gains a **not-overdriven witness** from existing tokens (§3.2).

## 1. §51's prerequisites, re-checked against `1cd13f5c`

| item | at `s51-design` | now | holds? |
|---|---|---|---|
| §54 harness merge (`meas-s51-harness` depends on `rec-s54-session`) | blocking | merged; plan 60's tools are on main (`1cd13f5c`) | **met**. The node's `blocked` state is stale |
| F1, `RWM_DELTA` pins the store cap at its floor | — | fixed `39008a01`, merged `9b8b42ca` | **met** |
| `rate-fix-rate-readout` (P2) | recommended before the session; gates the δ\* verdict | not built: there is no `[RATE]` print in `raptorpath/src`, and the node reads `next` | **holds as stated.** The session runs without it, and δ\* stays provisional. Block E's `spc`/`wr`/`fr` (§6) is a first slice of P2 (law r before the clamp, spare bind), but only on the gauge tree and one leg, so §51's rows do not get it |
| harness items §6.1–§6.7 | owed | **all owed**: no plan 51, no `s51_score.py`, no d80 table, no stub path | **holds.** Patterns now exist (§9.2) |
| harness text | `loopdiag_battery.sh:293/:313` hard-code `bulk`; parser pins `bulk` `:40/:172` | the battery takes a per-row hint from `plan60.py armspec` (`:317-320`). `ARM_SPEC[arm][2]` is the hint, and the 4th field is the **estimator-cadence echo** (`stage3_parse.py:84`), not batching | **stale refs**. Batching needs its own witness field |
| ρ < 1 exclusion (§2.3) | four reasons | (1) DNF by construction (`perf.rs:32-36`); (2) no send stamp in the chunk header (`perf.rs:15`); (3) ρ is tied to the hint (`config.rs:293-295`; `--window-reliable` only sets true, `main.rs:152`; `RWM_C_RELIABLE=0` only drops the flag in the harness); (4) the shed budget is armed only when `!reliable` (`shed_armed`, `sender_policy.rs:667`; read at `emit_source.rs:587`) | **holds** for T1/T2. Block E is ρ < 1 and **reported-only**: it reads engine-side tokens only (no goodput, no delay) and enters neither T1 nor T2 |
| engine changes since `7968c870` | — | δ laws unchanged. The snapshot-order law (`87c2135d`) shipped and changes recovery | the d80 rows (§48b BASE) predate `87c2135d`, so add post-`87c2135d` rows to the d80 recomputation (§9.2 item 51-5). The in-session A/A (H-A against N-A) governs resolution |

## 2. Blocks, trees and launches

| block | tree | binary rule | suites |
|---|---|---|---|
| **51** (§51 as designed, c2 and c8) | **main at launch** (`LD_BASE_SHA`), no engine change | record `bin_sha` | none. A dial-pin pre-flight as `s51-design` §6.2 |
| **E** (evicting seat, c2) | **`s51e-tree`** = main `1cd13f5c` + merge of `slaw-gauge` `d416d59b` + the two commits of §9.1 | a new registered `bin_sha` from an **E-pre** run (`suites_run.sh` + `binid_run.sh`, as 60-pre) | full, once, in E-pre |

- Block E needs a gauge tree because its readouts exist only there: `[WSTORE] adv`, `dbt_acc`/`dbt_em`/`dbt_share` and `fec=` (K4's ε and σ²). Main prints no `[WSTORE]`.
- **Engine identity check.** Over `raptorpath raptorpath-math raptorpath-core raptorpath-wasm gf256 Cargo.toml Cargo.lock`:
  - `git diff aefe6d97 1cd13f5c` is empty;
  - `cb88df3a` (the §60 `s60-tree`) is `aefe6d97` + `d416d59b`, 17 files under `raptorpath/` (`git diff aefe6d97 cb88df3a`).
  - So main + `d416d59b` should reproduce the §60 engine. §9.1 item 1's diff against `cb88df3a` is the witness.
- §60's `bin_sha` (`e410d640…`) and `ref60-manifest` are reusable only for the no-new-binary fallback (§5.3). The two §9.1 commits change the binary, so E-pre is required.
- Block E cannot ride §51's c2 launch, whose worst case is already 4 h 50 min against 5 h.

**Launch order:** E-pre → E rehearsal → E battery → 51 rehearsal → 51-c2 → 51-c8.
- E goes first. Its VM time is ≈ 1.9 h typical and ≈ 2.6 h worst (§11), and it unblocks `rate-pfec-oracle`.
- The blocks are independent: no 51 rule reads an E result.

## 3. Block 51

### 3.1 Unchanged from `s51-design`

Cells, arms, δ points, order, statistics, T1/T2 and predictions: `s51-design.md` §2–§5, verbatim. That is c2-100 and c8-100 at 600 MB, the 14 arms, 76 rows per cell, Auto carrier for the numeric arms, two-sided rank min-p per preset with Bonferroni over the three presets, and T2 FAIL predicted at Realtime.

### 3.2 Added (existing tokens, no engine change)

Per row, reported. These enter neither T1 nor T2.

| readout | formula | source |
|---|---|---|
| τ_x, saturated-tick share | share of `[DIAG]` ticks with src > 1 000 sym/s that have `infl ≥ cwnd` | `[DIAG] cwnd= infl= src=` |
| eviction share | (`hand` − `tx`)/`hand` at the last tick, per leg (`dgq{i}[hand= tx=]`, `dgev{i}`) | `net/diag.rs:780-785` |
| `MAX_CWND` bind share | share of sending ticks with `cwnd` = 10 000 (`PathState::MAX_CWND`, `scheduler/path.rs:116`) | `[DIAG] cwnd=` |
| spare-clamp tick bracket, **c2 only** | s = (cwnd − infl)⁺/infl per tick. Lower bound: share(s = 0); upper bound: share(s < 0.5), where 0.5 is `max_overhead`, which window_rate never exceeds. The clamp cannot bind above the upper bound, and must bind at s = 0 whenever r > 0 | `[DIAG]` (single path: `spare_capacity` is that leg's own) |
| (c8) | MISSING: `[DIAG]` prints no per-leg scheduler cwnd, so the min over legs is unread | — |

- **Records:** the spare < 0.5 share was 12–26 % on the October ρ = 1 rows above. So at Auto and Realtime the clamp can bind at ρ = 1. This bracket is what W-ROUTE's "spare-bound" attribution (`s51-design` §4) needs.
- **Smoke witness (not-overdriven, new).** On the H-R and N-A smoke rows:
  - `dgev`/`hand` ≤ 0.02;
  - τ_x ≤ 0.10;
  - `rr` > 0 (N-A; or R-INERT attributed as `s51-design` §4).
  - Predicted from the records above: `dgev`/`hand` ≤ 0.0001 (Realtime) and 0.007–0.009 (Auto), and τ_x 0.02–0.04.
  - A failing row is NO-GO, and the session is amended before the battery.

## 4. Block E: rows, cells, hints, δ

- **Cell:** c2-100 single, i.e. netem 5 ms ± 3 ms each way, 100 Mbit, GE (`lib.sh`). On one path `spare_capacity` is that leg's own and S08 is inert.
- **Seat:** `--protocol-hint realtime`, with no `--window-reliable` (`RWM_C_RELIABLE=0`), so δ = 50, β = 0 and b = ½. Also `RWM_GEN=0 RWM_DIAG=1 RWM_RDIAG=1 RWM_SLAW=1`, plan 59's env otherwise (no RTOBS).
  - **One δ point, by necessity:** ρ < 1 is reachable only on the Realtime carrier (status §3.3 row "the CLI's ρ"). This is the named point, not a mode.
  - The other dials are scrubbed as §51: `-u RWM_DELTA RWM_COPA_DELTA RWM_COPA_WIRE RWM_COMPLETION_EXPOSURE RWM_MIN_R RWM_CC_PACE`.
- **Load points.** The two paced points come from what the readings need. They are not merely "below the link".

| arm | role | offered load λ (payload) | rate_sym = λ/(8·494 B) | bytes = λ·T_e/8, T_e = 60 s | `RWM_PERF_TIMEOUT_S` = T_e + 15 | n |
|---|---|---|---|---|---|---|
| **PL** | K4 in the oracle's decidable window (A\* 3–7, `pfec-oracle-spec.md` §6.4) | 2.7664 Mbit | 700 | 20 748 000 | 75 | 5 |
| **PM** | the clamp at a moderate link fraction; m̄/W1 at a large span | 35 Mbit | 8 856 | 262 500 000 | 75 | 5 |
| **U** | unpaced positive control: the gauge must bind; the cwnd-enforcement witness at n > 1 | unpaced | ≈ 90–108 k (Amendment 2) | 600 000 000 | 30 | 2 |

- **Seeds:** 42 / 7 / 42 / 7 / 42 (plan 59). U uses 42 and 7.
- **Order:** blocked by replicate: PL, PM, U in the first two replicates, then PL and PM only; PL/PM order alternates.
- **The DNF is an expected datum.** A ρ < 1 object misses its holes forever (`perf.rs:32-36`), so the row ends at the timeout, 15 s after emission ends.
- **494 B** is the Realtime chunk payload (`perf.rs` `chunk_payload_len`: 508 − 14). At Realtime one chunk is one source symbol, so rate_sym = chunks/s.

**Per-point derived quantities (registered predictions, §8):**

```text
  A*   = clamp( ceil(rate_sym · min(b, 2) · rtprop), 1, W )      emit_source.rs:577-579; b(δ=50) = 2^(−½·log10(50/0.5)) = ½ (math lib.rs:183)
  Δ    = clamp( ceil(rate_sym · J), 1, 64 )                      emit_source.rs:580-582; J = est.jitter_us
  wire = λ · e · (1 + r + x)                                      e(1+r+x) = 1.43–1.45 measured at r ≈ 0.25 (store-law-n1 Realtime QDISC Sent 143.3–145.1 MB per 100 MB)
  infl ≈ rate_sym · srtt · (1 + r)
```

- **rtprop** is `est.rtt()`, a smoothed RTT, so it includes queue and dwell. **Band: 11–30 ms.**
  - The lower end is the recorded `[DIAG] rtp` (RTprop) median on the store-law-n1 c2 Realtime rows, 11 / 14 / 12 ms.
  - The upper end is the dwell-inclusive c2 srtt of `s51-design` §5.1 (E01), 30 ms.
  - Those ρ = 1 rows read EWMA `rtt=` ≈ 114 ms at a standing queue near link rate, which does not apply below the link.
  - rate_sym in A\* is the windowed-max send-rate anchor (`st.astar_anchor.rate`, `emit_source.rs:568`), not the mean. Under 1 ms pacing granularity it can read somewhat above the paced rate.
- **J** (`est.jitter_us`): 1–3 ms, given netem's ± 3 ms per direction.
- **Wire bound:** e ≈ 1.12, from 1.44/(1 + 0.25 + 0.03). The bound uses r_max = 0.5 (`max_overhead`) and x ≤ 0.05.
- **PL:** A\* = ⌈700·½·(0.011–0.030)⌉ = 4–11, Δ 1–3, wire ≤ 2.77·1.2·1.55 = 5.2 Mbit.
  - A\* moves 0.35 units per ms of rtprop, so the band is wider than the oracle's window. The calibration below is part of the design.
- **PM:** A\* = 49–133, below W (`MAX_WINDOW_SIZE` 200, `net/mod.rs:205`) unless W itself is smaller, Δ 9–27, wire typical 35·1.44 = 50 Mbit and at most 35·1.12·1.55 = 61 Mbit (≤ 0.61 of the link).
  - A\* moves 4.4 units per ms of rtprop at PM, so PM is **predicted K4-UNSCOREABLE** under the steady-window rule. PM's role is the clamp, m̄ and W1, and its K4 is reported only if it reaches ≥ 30 steady windows.
- **PL calibration rule (registered now).** It fires if the rehearsal's PL row has its LATE median A\* outside [3, 7] **or** fewer than 30 K4-steady LATE windows (§6.3's rule).
  - From the rehearsal, read x̂ = Ã\* − 0.5, the midpoint estimate of rate·b·rtprop at the median A\*.
  - Set the PL rate to 700·4.5/x̂, rounded to 10 sym/s, so that rate·b·rtprop sits mid-integer at A\* = 5. Recompute bytes and λ by the table's formulas.
  - Run **one PL-only recheck row** (`LD_REHEARSE=1`, PL only) at the new rate. It must pass §10 (c)–(h) for PL. Otherwise PL is dropped from the battery by amendment, and PM and U run as registered.
  - The rate change and the recheck are recorded as an amendment before the battery. Nothing else changes.

## 5. The paced-load mechanism

### 5.1 The knob: `RWM_PERF_RATE_MBIT` (perf client, default off)

- **What it is.** An application-side pacer in `perf.rs` `run_object`, on an absolute schedule:

```text
  due(i) = t0 + 8 · (Σ_{j<i} len_j) / λ          λ = RWM_PERF_RATE_MBIT · 1e6  (payload bits/s)
```

  Before `mem.feed.send(chunk i)`, the client waits until `due(i)`, unless that is already past. The wait stays inside the run deadline (a `due` past the deadline is a DNF, as today).
- **Hint-independent.** The knob is a property of the application's offered load, so it composes with every δ and ρ (NO-MODE-SWITCH).
- **Scope.** The warm-up object stays unpaced. Unset or absent means exactly today's loop, with no sleep and no extra clock read.
- A value that is not a finite number > 0 makes the client fail at start. It is never silently unpaced.
- **Liveness (rule 1):**
  - an echo at client start: `perf offered-load pacing ACTIVE (RWM_PERF_RATE_MBIT=<v>)`;
  - a `[GATES]` token `RWM_PERF_RATE_MBIT=<v>|unset`;
  - after each timed object's feed, one JSON line: `{"pace":true,"obj":<id>,"rate_mbit":<λ>,"chunks":<n>,"feed_s":<s>,"achieved_mbit":<8·bytes/feed_s>,"late":<k>,"late_max_ms":<m>}`. Here `late` counts the chunks whose send completed more than 2 ms after `due`: the TUN-backpressure share, the pacer's own bind gauge.
- **Read through:** `gates.rs`, as `perf_timeout_raw` is (`:292`, `:405`). It is added to `lib.sh` `RWM_FORWARD`, which `gate_forwarding_list_covers_the_engine_surface` enforces.

### 5.2 Its executes-witness (block E, every paced row)

- Both echoes are present.
- Median LATE `[DIAG] src=` is within ± 5 % of rate_sym.
- The last tick with src > 0 is at t ≥ T_e − 2 s.
- `achieved_mbit` is within ± 2 % of λ, and `late` ≤ 1 % of chunks.
- Failing any of these makes the row VOID (`pace-*`), re-run once.

### 5.3 The existing engine knob, and why it is not used

- **The knob:** `RWM_CC_PACE=1` (`gates.rs:354`). It is a token bucket on TUN reads at `clamp(1.1·max(goodput EWMA, Σ cwnd_i/srtt_i), RWM_GEN_RATE_FLOOR, RWM_GEN_RATE·n_live)` (`net/mod.rs:2655-2693`).
- **Why not:**
  - It paces **inside the sender loop**: a 1 ms pace wake (`:2788`) and a burst stop (`:3156`). That changes the loop cadence m̄ is read from (one cancellation per iteration, `pfec-oracle-spec.md` §6.1).
  - Its rate reads the scheduler cwnd, the quantity under question (§7). With cwnd near `MAX_CWND`, the ceiling `RWM_GEN_RATE` always binds, so the pacer reduces to a constant inside the machine under test.
- **Fallback only (owner Q2):** `RWM_CC_PACE=1 RWM_GEN_RATE=<rate_sym> RWM_GEN_RATE_FLOOR=<rate_sym>` on the registered §60 binary `e410d640…`.
  - No suites are needed (the `ref60-manifest` HIT).
  - The spare gauge becomes the tick bracket of §3.2 instead of §6's exact count.
  - The m̄ reading is labelled "under CC pacing".

## 6. The spare-clamp gauge (engine print, gauge tree, gated)

### 6.1 Where and what

- **The site.** `net/emit_source.rs:537` (main; on the gauge tree it is the line after `note_fec_inputs`), in the rate recompute's `Some(est)` arm:

```text
  flat_rate = window_rate.min(spare.max(0.0))          spare = Scheduler::spare_capacity() = min over active paths of (cwnd − infl)⁺/infl (∞ at infl 0), scheduler/mod.rs:568, path.rs:467-471
```

- **Gate.** `pol.slaw_on` (`RWM_DIAG && RWM_SLAW`, default off). It is observation only, so nothing in the engine reads it.
- **New calls into `net::slaw`:**
  - `note_spare_clamp(window_rate, spare)`, right after `flat_rate`;
  - `note_spare_none()`, in the `None =>` arm (`:606`);
  - `note_debt_accrued` (gauge tree) additionally counts the accrual against the clamp state of the recompute whose cached rate it uses.
- **Definitions per recompute** (wr = `window_rate`, the cadenced law r **before** the clamp; fr = `flat_rate`):
  - **bound** ⇔ fr < wr;
  - **zero** ⇔ fr = 0 ∧ wr > 0;
  - a non-finite wr counts as neither and is still counted in `rn`.
- **New field in `[WSTORE]`,** appended after `fec=`, as per-window deltas in the same static-atomic pattern as `fec=`:

```text
  spc=<ab>/<az>/<an>/<rb>/<rn>/<r0> wr=<Σwr/rn>/<Σfr/rn>         (6 decimals; sums in fec='s micro() quantisation; spc=0/0/0/0/0/0 wr=-/- on an empty window)
    an = accruals; ab, az = accruals whose rate came from a bound / zero recompute
    rn = recomputes reaching the clamp; rb = bound among them; r0 = recomputes with no estimator
```

- **Denominator: accruals (source symbols).** Each source accrues the cached rate of its recompute, so the source-weighted share is what multiplies r.
  - At this seat a recompute runs **per source symbol**: `taper_recompute_due` returns true when `!batching` (`emit_burst.rs:46`), and `emit_batch_on` needs `reliable && !use_packing` (`sender_policy.rs:737`).
  - Under batching (Bulk/Auto at ρ = 1) a recompute runs per burst. That is why `an` and `rn` are kept apart.
- **Per-leg argmin:** not printed. On c2 the one leg is the min. A multi-leg argmin belongs to P2 (`rate-fix-rate-readout`).
- **Readouts:**
  - bind fraction φ = Σab/Σan;
  - zero fraction ζ = Σaz/Σan;
  - partial share π = (Σab − Σaz)/Σan;
  - mean clamp deficit Σwr/rn − Σfr/rn.
  - The clamp also feeds the shed budget: `residual_loss_after_fec(ε, flat_rate, …)` (`emit_source.rs:593-597`). So φ also covers the 1 − ρ budget's r.

### 6.2 Witnesses (rule 1, before any φ is read)

- **Host unit tests** (`slaw_tests.rs`):
  - spare = ∞ ⇒ not bound;
  - spare = 0 with wr > 0 ⇒ zero;
  - 0 < spare < wr ⇒ bound and not zero;
  - spare ≥ wr ⇒ free;
  - a cache-hit accrual inherits the last state;
  - `note_spare_none` ⇒ free;
  - empty window ⇒ `spc=0/0/0/0/0/0 wr=-/-`.
- Updated parser and sample: `testdata/slaw_sample.log` gains the field, and the parser test checks it.
- **On every E row and window (exact; a mismatch VOIDs the gauge):**
  - `an` = `fec=`'s n (same call site);
  - `rn` + `r0` = `an` (per-symbol recompute at this seat);
  - grant ≡ flat_rate (E1, with `RWM_MIN_R` = 0) on every window: |(Σfr/rn)·rn/(rn + r0) − `fec=` r| ≤ 1.5e-5.
    - The r0 recomputes hand r = 0, which is why the rn/(rn + r0) factor appears.
    - The tolerance is the print and quantisation bound: `fec=` r at 5 decimals (± 5e-6), fr at 6 (± 5e-7) and the micro() floor (≤ 1e-6 per mean), ≈ 6.5e-6, doubled.
    - No `ab` restriction applies, because flat_rate ≤ spare always holds.
  - `fec=` r = 0 on windows with `az` + (accruals from r0) = `an`.
- **Positive control:** U rows have ζ ≥ **0.5** over their sending windows. If ζ < 0.5 on both U rows, φ is NO VERDICT for the session.
  - 0.5 is the validity threshold. The **prediction** is ζ ≥ 0.8.
  - The only datum is Amendment 2's 46 of 53 windows with r = 0 (0.87, by window count, n = 1), and ζ is source-weighted.
- **Bit-identity:** with `RWM_SLAW` unset, `decision_trace_pin` (7 digests) and the gauge's cap-sequence test pass unchanged on `s51e-tree`.

### 6.3 Outcome classes (registered)

| class | condition (per arm, pooled over its rows' LATE windows; U over its sending windows) |
|---|---|
| SLACK | φ < 0.05 |
| PARTIAL | 0.05 ≤ φ ≤ 0.95 |
| PINNED | φ > 0.95 |

**The verdict input to `rate-spare-clamp-verdict`.** The first class that matches wins, so the set is exhaustive:

1. **NO VERDICT:** U's positive control fails (ζ < 0.5 on both U rows), or a gauge-witness VOID leaves an arm with no rows.
2. **PINNED-PACED:** PL or PM is PINNED. That arm's rate law operates as a constant.
   - Its rr and shed readings are re-scoped as clamp readings.
   - This is a defect finding (MEASUREMENT DISCIPLINE 18).
   - K4 still reads it: K4 witnesses the debt loop at whatever r was handed.
3. **LIVE-RANGE:** PL or PM is PARTIAL. The clamp operates in its continuous range at a paced load.
   - φ, π and the deficit per arm are reported for the ledger.
   - This session draws no defect class from it.
4. **DEGENERATE-AS-WIRED** (predicted): PL and PM are SLACK, and U passes the control.
   - In words: at every non-overdriven load measured, the clamp never binds. It closes repair only when the source outruns an unenforced cwnd (§7). So it acts as a saturation switch, not as a headroom law.
   - This is a **defect finding** (MEASUREMENT DISCIPLINE 18), not a footnote. It names the clamp and both of its consumers (the repair grant and the shed budget's r).
   - π on U is reported as the switch's transition share; Amendment 2 had 7 of 53 windows between r = 0 and the law. π does not enter the class.

**K4 steady windows** (status §60 §6's rule: n ≥ 1, one `a_star` and one `delta` across the window and both neighbours) additionally require `ab` = 0 in the window and both neighbours, so that the handed r is the law r.

## 7. `rate-spare-cwnd`: is spare's cwnd enforced?

**Answered by reading; registered as the prediction: NOT ENFORCED at either seat, and the session reads the executed witness.**

- **What reads the scheduler `p.cwnd`:**
  - `available()`, and through it:
    - `active_paths()` (`scheduler/mod.rs:226-232`);
    - `redundant_source_path` (`:552-562`, S08);
    - `best_repair_path` (`:511-519`);
    - **`best_source_path`** (`:490-498`), the evicting seat's source pick (`emit_source.rs:312-318`: `select_source_path` when `!reliable`);
  - `spare_capacity` (the repair clamp);
  - the brake (`cwnd_full`, `net/mod.rs:2324-2360`). It reads `p.cwnd` only under `RWM_LATE_BRAKE`; otherwise it uses gain·BDP (`RWM_INFL_BDP`) or the static `RWM_INFL_CAP`. All three are off on the plain window;
  - `RWM_CC_PACE` (off).
- **What steers but never gates:**
  - At ρ < 1, `best_source_path` filters `available() > 0`, so cwnd steers *which* path gets the source. With no path available it returns `None`, and `select_source_path` sends on path 0 anyway (`net/mod.rs:1382-1384`). On c2's single path that is the only path.
  - At ρ = 1, `place_symbol` returns `None` only "if no path is up at all" (`scheduler/place.rs:84-89`), and the caller falls back to 0.
- **Source admission:** `tx_paused` needs `reliable` (`net/mod.rs:2397`).
- So nothing on the source path withholds a symbol because the scheduler cwnd is full.
- **Where the scheduler cwnd comes from:** it is Copa-lite's model cwnd (`scheduler/path.rs` `on_delivery_signal`), clamped at `MAX_CWND` = 10 000.
- **What enforces the wire:** quinn's BBR (`qcwnd`, bytes) and quinn's 4 MB datagram buffer, which evicts the oldest datagrams and still returns `Ok` (`transport/io_owner.rs:129-150`).
- **At ρ = 1** the store cap bounds in_flight instead: 36 against a cwnd of 10 000 on §60's c2 control. That is why the clamp is slack there at Bulk.

**Executed witness, per sending tick, on every E row:**

```text
  τ_x = share of ticks with infl ≥ cwnd  ∧  src > 1 000 sym/s  ∧  paused = 0 %
```

- It counts source emission continuing while the scheduler cwnd is full. An enforced window can be exceeded by at most one emission burst.
- **NOT-ENFORCED:** τ_x ≥ 0.5 on both U rows (Amendment 2: 48 of 52).
- **CONTRADICTED:** τ_x = 0 on both U rows. The reading is then wrong and is investigated before any φ verdict.
- **MIXED:** anything else (0 < τ_x < 0.5, or the U rows disagree). The code reading stands as the answer, but the witness does not confirm it. Reported, with the ticks listed.
- **Reported beside it:**
  - `qcwnd` against cwnd·512 B;
  - `dgev`/`hand`;
  - the `MAX_CWND` share;
  - per §51 row, τ_x at ρ = 1 (§3.2).

## 8. Block E readouts and predictions

### 8.1 Readouts (formulas and provenance)

| readout | formula | provenance |
|---|---|---|
| **m̄** | Δ`cum` ack / Σ`adv` count over LATE (t ≥ 5 s); witness `mbar_lo` = Δcum/Σ`wait n` ≤ m̄ | `adv` is pushed once per cumulative advance in `on_ack_advance` (`sender_phases.rs:1225`, called per loop iteration from `net/mod.rs:3482`); `s60_score.evict_stats`; `mbar59_score` |
| **W1** | Σ`dbt_em` / Σ`dbt_acc` over LATE; bracket max(0, 1 − 1/(rr·mbar_lo)) ≤ W1 ≤ (cod/src)/rr | `pfec-oracle-spec.md` §0 E2, §6.2; `mbar59_score` |
| **K4 inputs** | per steady window: `fec=` ε/σ²/r/A\*/n, Δ from `[SPAN] delta=` | §60 §6, plus `ab` = 0 (§6.3) |
| φ, ζ, π, clamp deficit | §6.1 | new gauge |
| τ_x, `qcwnd`/cwnd, `MAX_CWND` share | §7 | `[DIAG]` |
| eviction share | (`hand` − `tx`)/`hand` | `[DIAG] dgq0` |
| `max_overhead` bind | share of LATE `[SPAN]` lines with `rr=0.500` | `max_overhead` ≤ 0.5 (`pfec-oracle-spec.md` §6.3) |
| A\*, Δ, W | medians of `[SPAN] a_star= delta= wsize=` over LATE | `emit_source.rs:577-582` |
| shed | last `[SPAN] shed=`/`bud=` | `emit_source.rs:651-660` |
| pacing | §5.2 | §5.1 |

**K4 itself** (oracle W1 at the row's inputs against engine W1, |Δ| ≤ 3·SE across rows) is computed **off-VM**, on `pfec-oracle`, as §60 registered. This session supplies the inputs only.

### 8.2 Predictions (computed before any run)

| reading | PL | PM | U | from |
|---|---|---|---|---|
| src (sym/s, LATE median) | 700 ± 5 % | 8 856 ± 5 % | 90–108 k | knob; Amendment 2 |
| last sending tick | ≥ 58 s | ≥ 58 s | ≈ 13 s | T_e; Amendment 2 |
| A\* | 4–11 before calibration; 3–7 after it (≥ 30 steady windows) | 49–133; K4-UNSCOREABLE predicted | — | §4 formula, rtprop 11–30 ms |
| Δ | 1–3 | 9–27 | — | §4 formula |
| wire (Mbit) | ≤ 5.2 | 50 typical, ≤ 61 | link | §4 formula |
| median rr | > 0; magnitude unpredicted (W ≈ 8–25 lies outside every recorded row) | 0.15–0.32 | 0.000 | store-law-n1 Realtime 0.24–0.28 (W 201); `s51-design` §5.1 0.176–0.249 at δ = 50; r_sat 0.315 |
| φ class | SLACK | SLACK | ζ ≥ 0.8 (validity ≥ 0.5) | infl ≈ rate·srtt·(1 + r): 10–27 (PL) and 127–345 (PM), against a cwnd with no standing queue to back it off (Copa-lite backs off only on queue, `path.rs:501-509`) ⇒ spare ≫ 0.5 |
| outcome | — | — | — | **DEGENERATE-AS-WIRED predicted** |
| eviction share | ≤ 0.001 | ≤ 0.001 | ≥ 0.5 | records §0 |
| τ_x | ≈ 0 | ≈ 0 | ≥ 0.5 | §7 |
| m̄ | 1–2 | reported (context: 0.7–6.5 on reliable c2 MAIN rows) | not read | `RWM_ACK_MERGE`: one ack per data message (`sender_policy.rs:31-43`); at 700 sym/s each source is its own message, and holes add jumps of ≈ rate·D ≈ 4 at ε ≈ 2.5 % |
| W1 | reported, no prediction | reported | not read | 0.055 withdrawn (oracle §6.2); the rehearsal's r > 0 windows read 0.97–0.9995 (n = 1) |

The U rows are not read for m̄, W1 or K4: the wire is overdriven and quinn evicts.

## 9. Work items

### 9.1 Engine (one Sonnet worker, host cargo, branch `s51e-tree`)

1. **Tree.** Branch from main `1cd13f5c` and merge `github/slaw-gauge` `d416d59b`.
   - Witness: `git diff cb88df3a HEAD -- raptorpath raptorpath-math raptorpath-core raptorpath-wasm gf256 Cargo.toml Cargo.lock` is empty.
2. **Commit A, the pacing knob (§5.1):**
   - `gates.rs`: a `perf_rate_raw` field and its `[GATES]` echo;
   - `perf.rs` `run_object`: the schedule, the start echo and the `{"pace":…}` line;
   - `tools/l1/lib.sh` `RWM_FORWARD`: add the knob.
   - Tests:
     - a pure `due_offset(bytes_before, λ)`, with absolute values at 1 and 2 chunks and a full and a short last chunk;
     - an invalid value is rejected;
     - unset ⇒ no wait path, asserted by a unit test of the selector;
     - the gate-forwarding test passes.
3. **Commit B, the spare gauge (§6.1):**
   - the two `emit_source.rs` call sites;
   - the `net::slaw` statics, `note_spare_clamp`/`note_spare_none`, the accrual inheritance and the `[WSTORE]` field;
   - the tests of §6.2;
   - the sample and parser update in `tools/l1` (the `[WSTORE]` parser `s60_score.py` uses).
4. **Done when:**
   - `cargo test` passes (lib, `slaw_tests`, `slaw_reachability`, `decision_trace_pin` 7/7);
   - wasm goldens are unchanged (no wasm edit);
   - clippy is clean on the touched files;
   - with `RWM_SLAW` and `RWM_PERF_RATE_MBIT` unset, no new output line appears;
   - report the two shas.

### 9.2 Harness (one Sonnet worker, no cargo; after 9.1 for E, independent for 51)

**Block E (plan `51e`):**
- **E-1, `plan51e.py`** (the `plan60.py` one-table pattern). The arms PL/PM/U, with their bytes, timeout, `realtime` and env:
  - `RWM_C_RELIABLE=0 RWM_SLAW=1`, plus `RWM_PERF_RATE_MBIT` (PL/PM);
  - the env scrub of §4;
  - seeds and order of §4;
  - smoke = one PM row;
  - rehearsal = PL, PM, U, one each.
- **E-2, `loopdiag_battery.sh` / `loopdiag_run_all.sh`:**
  - plan `51e` with `LD_SHA` = E-pre's `bin_sha` (abort on mismatch, never re-run);
  - `BUDGET_S["51e"]`, `LD_HARD` and `LD_REHEARSE`;
  - the PL calibration rule (A* and steady-window count, the new rate by §4's formula, and the PL-only recheck path `LD_REHEARSE=1 LD_ONLY=PL`) printed by the rehearsal scorer. The battery reads the PL rate from the table only, after the amendment.
  - Also add `rate=${RWM_PERF_RATE_MBIT:-unset}` to the `perf_rwm_c.sh` header line.
- **E-3, parser and scorer `s51e_score.py`.** Reuse `s60_score._evict` / `evict_stats` / `evict_line` and `mbar59_score.row_metrics`. Add:
  - the `spc`/`wr` parse and φ, ζ, π, clamp deficit (§6);
  - the §6.2 on-row witnesses and the U positive control;
  - τ_x, `qcwnd`, the `MAX_CWND` share and the eviction share;
  - the `{"pace":…}` parse and the §5.2 witness;
  - K4 steady windows with the `ab` = 0 rule;
  - the §6.3 classes, the outcome line and `rate-spare-cwnd`'s NOT-ENFORCED/CONTRADICTED line;
  - the §8.2 prediction bands beside each reading.
  - `smoke51e`/`rehearse51e` give PASS/FAIL per §10.
- **E-4, `stubrun_gen.py` + `test_stubrun.py`:**
  - the stub emits `[WSTORE] … spc=… wr=…`, the `{"pace":…}` line, `[GATES] RWM_PERF_RATE_MBIT=`, paced `src=` and a U-style saturated series;
  - `HARNESS_PATHS["51e"]` covers rehearse and full;
  - a negative control for each witness (`an` ≠ fec n, a missing pace echo, U ζ < 0.5, PL with < 30 steady windows triggering the calibration line).

**Block 51 (plan `51`):** `s51-design` §6 items 1–7, as written, with these corrections:
- **51-1 / 51-3:** use the `plan60.py armspec` pattern for the per-row hint, bytes and timeout. Arm hints go in `ARM_SPEC[arm][2]`. The batching echo (`ACTIVE` at bulk/auto, `out of scope` at realtime) is a **new** witness field: the 4th `ARM_SPEC` field is the estimator-cadence echo.
- **51-3:** also add §3.2's readouts (τ_x, eviction share, `MAX_CWND` share, the c2 tick bracket) and the not-overdriven smoke witness.
- **51-5:** in the d80 recomputation, add 600 MB c2/c8 BASE rows built at or after `87c2135d`, identified by `bin_sha`. The §48b and §53 rows predate it.

## 10. Go/no-go, witnesses, VOID

**E rehearsal (`LD_PLAN=51e LD_REHEARSE=1`, rows PL, PM and U). GO iff all of the following hold:**
- (a) `bin_sha` = E-pre's, and the E-pre manifest HITs;
- (b) plan 59's liveness: the shed echo, `shed=` on `[SPAN]`, `RWM_UNIFIED`/`RWM_UNIFIED_SHED` on both ends, no retain-until-acked echo, the Realtime echo;
- (c) the pacing witness of §5.2 on PL and PM;
- (d) **median LATE `[SPAN] rr` > 0 on PL and PM** (Amendment 2's go/no-go);
- (e) ≥ 120 LATE windows with src > 0 on PL and PM;
- (f) eviction share ≤ 0.001 on PL and PM, and > 0.1 on U;
- (g) the §6.2 on-row gauge witnesses on all three rows, and U ζ ≥ 0.5 (the validity threshold; 0.8 is the prediction);
- (h) on PL, **≥ 30 K4-steady LATE windows** under the full rule (§6.3: n ≥ 1, one `a_star` and one `delta` over the window and both neighbours, `ab` = 0), with median A\* in [3, 7].

**(h) is handled by the calibration, not by NO-GO.**
- A rehearsal that fails only (h) triggers §4's calibration rule and its one PL-only recheck row.
- A recheck that fails (h) drops PL from the battery by amendment.

**Reported in the rehearsal, not gating:**
- PM's steady-window count (predicted < 30);
- τ_x on U.

**NO-GO** on any of (a)–(g) means an amendment before the battery. Rehearsal rows are never re-run.

**Battery VOID and re-runs (once each, `s60` Amendment 1's rule):**
- an echo or liveness VOID, including `pace-*` and `no-realtime-echo`;
- a gauge-witness VOID (`spc-*`).
- A wrong binary aborts. The DNF is a datum and is never re-run.

**Block 51:** `s51-design` §2.2 smoke and §6.7, plus §3.2's not-overdriven witness on H-R and N-A.

## 11. Budget and launches

| launch | content | typical | worst | `LD_HARD` |
|---|---|---|---|---|
| E-pre | `suites_run.sh` + `binid_run.sh` on `s51e-tree` (as 60-pre) | ≈ 75 min | ≈ 90 min | — |
| E rehearsal | build 6 min + 3 rows (90 + 90 + 45 s); plus, if calibration fires, one PL recheck launch (build + 90 s ≈ 8 min) | ≈ 12 min | ≈ 25 min | + 1 h |
| E battery | build 6 + smoke 1.5 + 12 rows (5·90 + 5·90 + 2·45 = 990 s) | ≈ 25 min | 990 s more for one re-run of every slot: ≈ 42 min | + 1.5 h |
| 51 rehearsal | `s51-design` §7 | ≈ 20 min | — | as designed |
| 51-c2, 51-c8 | `s51-design` §7, 76 rows each | ≈ 2 h each | 4 h 50 min each | + 5 h each |

- Row time is the timeout (the DNF) plus ≈ 15 s of bring-up.
- Block E costs ≈ 1.9 h of VM time typical and ≈ 2.6 h worst (E-pre + rehearsal + recheck + battery). Nothing E needs fits inside the §51 launches.

## 12. Owner questions

1. **§51's binary.** Should §51 run on main's binary as designed (recommended: it is the baseline of the shipped machine)? The alternative is `s51e-tree` with `RWM_SLAW=1` on every §51 row:
   - it would gain the exact spare gauge and `fec=` (the law's inputs) at ρ = 1;
   - but the baseline would be "main + a dormant gauge" without a guard;
   - and `RWM_SLAW=1`'s CPU cost would sit inside every CPU/GB clause (uniform across arms, so T1/T2 are unaffected, but the absolute baseline shifts).
2. **The pacing mechanism.** The app-side `RWM_PERF_RATE_MBIT` (recommended) costs a new binary and E-pre (≈ 75 min of VM). The fallback is `RWM_CC_PACE` on the §60 binary, with no suites, the tick bracket only, and m̄ read under an engine pacer (§5.3).
3. **The load points.** Accept PL (700 sym/s, A\* 4–6, the oracle's decidable window) and PM (35 Mbit), with PL's one-shot calibration rule? Alternatively, PM alone, if K4 at A\* 3–7 is not wanted from the engine.
4. **The merge path.** Should the pacing knob and the spare gauge reach main later (each needs a guard, because the release binary changes) or stay on the measurement tree? `meas-s51b-rho` will need the pacing knob again.
5. **The clamp verdict.** Confirm that the registered DEGENERATE-AS-WIRED outcome (§6.3) goes to `rate-spare-clamp-verdict` as a defect finding. The clamp reads a model cwnd nothing enforces (§7), so its intended "never exceed link headroom" contract is void at both seats.
