# s51-design: the off-Bulk baseline and the δ sweep (§51), designed before any VM contact

Status: DESIGN. No engine change, no VM, nothing registered. This note gives the §51 pre-registration its cells, arms, readouts, statistics and predictions, and lists the prerequisites it found. Feeds: tech-tree nodes `meas-s51-design` → `meas-s51-session`; law ledger R01, R02, R03, R04, R07, S08, S09, K01 (`docs/derivations/law-ledger.md`); status §3.3, §3.10 item 2, §41d. Code lines were read at `github/main` `7968c870`.

**The finding that shapes the design.** δ can already be set as a number (`RWM_DELTA`), but the number moves only the continuous δ laws. Five behaviours stay keyed on the hint *string*, so a sweep on one carrier hint cannot see a step at a preset. The design therefore crosses the carrier hint with the numeric δ (§2). It scores two things separately: continuity of the δ laws through each preset (T1), and whether the named hint equals the same δ as a number (T2). T2 is predicted to FAIL at Realtime. That FAIL measures known NO-MODE-SWITCH debts; it is not a surprise.

## 1. The dials on the engine today

### 1.1 How each dial is set

| dial | how it is set | where |
|---|---|---|
| δ, by name | `--protocol-hint realtime\|auto\|bulk` (perf and run). The default is Auto. | `main.rs:146`, `config.rs:235` |
| δ, as a number | `RWM_DELTA=<f64>` (finite, > 0). It replaces the hint→δ map everywhere `net::delta_price` is read: on the sender, on the receiver, and in Copa's δ unless `RWM_COPA_DELTA` outranks it. Echoed in `[GATES]`. Forwarded by `lib.sh` `RWM_FORWARD` to both endpoints. | `gates.rs:58-65`, `net/store_cap.rs:311` |
| Copa δ | `RWM_COPA_DELTA` ▸ `RWM_DELTA` ▸ the hint's map. Consumed only by the wire-clocked Copa update (`RWM_COPA_WIRE`, off by default). On the default substrate (quinn BBR) Copa's δ is therefore not read. | `scheduler/copa.rs:38-66`, `:1504` |
| ρ | A boolean, not a number. `window_reliable` defaults to `hint != Realtime` (`config.rs:295`). `--window-reliable` can only set it to `Some(true)` (`main.rs:295`), so the perf CLI reaches ρ < 1 only on the Realtime carrier. `perf_rwm_c.sh` always passes `--window-reliable` (`:84`). | — |
| r | No knob. It is the law's output: `RWM_MIN_R` floors it (an off arm), and `RWM_COMPLETION_EXPOSURE` feeds χ to the Bulk term from the perf client's T_rem (off; χ ≡ 0 otherwise). | `emit_source.rs:678`, `sender_phases.rs:1316` |

The hint→δ map is `δ(hint) = δ_Auto/ζ(hint)`: Realtime **50**, Auto **0.5**, Bulk **0.005** (`scheduler/copa.rs:47`, bit-exact, pinned by `delta_budget_b_is_the_dial_not_a_mode`). The laws that read δ (all through `delta_price`, so all of them move with `RWM_DELTA`):

```text
  β(δ)  = clamp( log10(δ_Auto/δ) / log10(δ_Auto/δ_Bulk), 0, 1 )     rate-mix weight      (math lib.rs:196)
  t(δ)  = clamp( 1e-5 · δ_Auto/δ, 1e-9, 0.1 )                       anchor tail target   (fec_rate.rs:335; base 1e-5 = config.rs:337)
  r(δ)  = (1 − β)·r_anchor(t(δ)) + β·r_late(χ)                      rate mix             (fec_rate.rs:157)
  b(δ)  = clamp( 2^(−½·log10(δ/δ_Auto)), ½, 2 )                     span horizon, D, A*  (math lib.rs:183; emit_source.rs:571)
  w_bw(δ) = clamp( ½ − ¼·log10(δ/δ_Auto), 0, 1 ), w_lat = 1 − w_bw  placement weights    (scheduler/mod.rs:79)
  q(δ)  = (b(δ) + 1)/30                                             pooled store cap 1+q (sender_policy.rs:585)
```

With χ ≡ 0 the late term is exactly 0 (`controller_rate`: δ_eff = p ⇒ z = −∞ ⇒ r = 0), so on every §51 row

```text
  r(δ) = (1 − β(δ)) · r_anchor(t(δ))
```

R01's measurement is therefore a measurement of the anchor term (R02), weighted by 1 − β. The presets sit at β = 1 (Bulk), β = 0 with a slope kink (Auto), and β = 0 with t = 1e-7 (Realtime). The kink at Auto is in the slope only; there is no step.

### 1.2 What the hint string still selects (not moved by `RWM_DELTA`)

Each row is a behaviour step at a preset (status §3.3; paper divergence row 13):

| site | Realtime | Auto | Bulk | ledger |
|---|---|---|---|---|
| source-symbol size `symbol_size_for_hint` (`net/mod.rs:179`); perf chunk (`perf.rs:53`). Every law constant counted in symbols (pool 2048 per path, cap clamp N·pool, W ≤ 200, A*, store boot 128, emission burst 64) therefore covers 2.3× fewer bytes at Realtime. | 512 B (494 B payload) | 1200 | 1200 | no row (new candidate; §8 P6) |
| duplicate source on the cheapest other path (`emit_source.rs:461`) | on | off | off | S08 |
| symbol packing; emission batching requires `!use_packing` (`sender_policy.rs:678`, `:737`) | packing on, batching out of scope | batching ACTIVE | batching ACTIVE | S09 |
| Copa queue target `queue_target_mult` (`scheduler/copa.rs:314`); it feeds cwnd, and cwnd feeds `spare_capacity` (the rate's clamp) and placement | 1.08 | 1.125 | 1.25 | K01 |
| default ρ (`config.rs:295`) | EVICT | retain | retain | §3.3 row 5 |

**Prerequisite?** No knob is missing for δ. A non-preset δ is settable today, and `tools/l1/r_battery.sh`'s MID arm (`RWM_DELTA=0.05`) already used it. That arm was VOID by W7 because Copa's δ had no echo then; the echo has existed since `50b17af` (status §3.1 item 5). The missing pieces are a numeric ρ (§2.3) and readouts (§3, §8).

## 2. Cells, arms and the sweep

### 2.1 Cells

- **c2-100**: single path, c2 (100 Mbit, 5 ms, GE 1.3/50 ⇒ ε ≈ 2.5 %, per-datagram bursts ≈ 10). It is the cell of the §3.10 crossing r = 0.0267. On a single path `redundant_source_path` returns `None` (`scheduler/mod.rs:555`, `p.id != primary`), so S08 is inert there.
- **c8-100**: dual c2/c3. The rate law reads the worst-ε path (`cadenced_repair_rate` → `worst_eps_path`, `emit_source.rs:886`), which is the c3 leg (ε ≈ 4.1–4.9 %). c8 therefore carries the c3 crossing (0.0515), and c3 single is not needed (at 20 Mbit, 600 MB would not fit the 200 s timeout). On c8, S08 is live.
- c2 against c8 separates S08 from the other Realtime table effects in T2.
- Both cells run at **600 MB**, `RWM_PERF_TIMEOUT_S=200`, as in guard v2 (§41d): in-order delay resolves only on long rows. A row with its last tick before 30 s is VOID-SHORT.

### 2.2 Arms (per cell; one launch per cell, §7)

All arms run `--window-reliable` (ρ = 1), `RWM_GEN=0` (the plain window; `perf_rwm_c.sh` turns generation on otherwise) and `RWM_DIAG=1`. `RWM_DELTA`, `RWM_COPA_DELTA`, `RWM_COPA_WIRE`, `RWM_COMPLETION_EXPOSURE` and `RWM_MIN_R` are scrubbed, then set per arm.

| arm | carrier `--protocol-hint` | `RWM_DELTA` | β | role | n |
|---|---|---|---|---|---|
| H-B | bulk | unset | 1 | named Bulk (the L1 corner; same-session control) | 6 |
| H-A | auto | unset | 0 | named Auto (the shipped default, never measured) | 6 |
| H-R | realtime | unset | 0 | named Realtime (at ρ = 1) | 6 |
| N-A | auto | 0.5 | 0 | the same δ as H-A, as a number: bit-identical laws. It is both the A/A and the `RWM_DELTA` wiring witness. | 6 |
| B− / B+ | auto | 0.0049 / 0.0051 | 1 / 0.9957 | ±2 % nudges at Bulk | 6 + 6 |
| A− / A+ | auto | 0.49 / 0.51 | 0 / 0 | ±2 % nudges at Auto | 6 + 6 |
| R− / R+ | auto | 49 / 51 | 0 / 0 | ±2 % nudges at Realtime | 6 + 6 |
| I1 | auto | 0.0075 | 0.912 | below every predicted crossing δ* | 4 |
| I2 | auto | 0.0158 | 0.750 | between the c2 and c8 δ* (§5) | 4 |
| I3 | auto | 0.05 | 0.500 | the r battery's MID point, above every δ* | 4 |
| I4 | auto | 5 | 0 | between Auto and Realtime; only t, b and w_bw move | 4 |

- That is 76 rows per cell: 10 scored arms × 6, plus 4 interior arms × 4.
- The δ points are 0.005, 0.0075, 0.0158, 0.05, 0.5, 5 and 50, with nudges at each preset. That is seven distinct points through and between the presets.
- **The carrier is Auto for every numeric arm.** That makes the sweep the shipped default machine moved along δ.
- Why the other carriers are not swept: the carrier tables of §1.2 do not move with δ. Re-running the sweep on Bulk and Realtime carriers would triple the rows and measure the same tables. T2 measures them once per preset.
- **Order.** There are six blocks, with seeds 42 and 7 alternating. Every block runs all scored arms; the interior arms run in blocks 1–4. Within a block, each nudge pair runs back to back (the first member alternating by block), so drift between the two sides of a preset is minimal. Pairs and singletons are rotated by block.
- **Smoke:** H-B, H-R, B+ and N-A (one each).
- No replacement rows, except under the guard's VOID rules.

### 2.3 ρ < 1: excluded, with the reasons

1. **The perf driver cannot score it.** A shed or force-delivered hole leaves its object forever incomplete. The row is a DNF by construction (`perf.rs:32-36`), so there is no goodput.
2. **There is no delivered-delay readout.** The perf chunk header carries no send stamp (`[magic][obj][idx][total]`, `perf.rs:15`). The guard's in-order delay reads the cumulative frontier, and under shed the frontier passes undelivered seqs (§41d domain note). A delay read there would reward shedding.
3. **ρ has no numeric dial.** It is a boolean on the engine, and on the perf CLI it is coupled to the hint (ρ < 1 only at Realtime). A ρ block could only be "Realtime without `--window-reliable`", one point, not a dial.
4. **The P_fec step has a δ-reader only at ρ < 1.** At ρ = 1, P_fec enters only `r_saturation(p, σ², W, srtt, t_sym)`, which takes neither δ nor r, so no behaviour step in δ is predicted there (§5.2). At ρ < 1 the shed budget `ε(1 − P_fec(r_live))` (`emit_source.rs:587`, armed only when `!reliable`, `shed.rs:24`) steps from ε to ε/2 where r(δ) crosses ε/(1−ε). The excluded block is exactly where R03 bites, which is why it becomes its own node (§8 P4): a numeric ρ at any carrier, per-chunk send stamps, and a paired readout. The pair is the resolution delay `t − t_src(frontier(t))`, with the frontier = delivered-or-abandoned, scored only together with the abandoned fraction (shed/src), never alone.

## 3. Readouts

**Per row, from existing instruments (no engine change):**

| readout | source | scored in |
|---|---|---|
| goodput | perf JSON `mbps` | T1, T2 |
| in-order delay mean (whole run) | `threadp1_parse.inorder_delay_stats` (the guard v2 clause); B′ (source phase only) reported beside it | T1, T2 |
| CPU/GB, client and server | driver `CPU:` / bytes | T1, T2 |
| retransmits/GB | last `[DIAG] retx=` | reported |
| receiver duplicates | `rx_dup` (place38 row) | reported |
| coded share | `cod/(src+cod)`, last `[DIAG] cum=` | reported |
| **consumed r (primary)** | `(cod + taper)/src` from the last `[DIAG]`, minus the same session's H-B share at that cell (H-B's `cod` is the NACK margin alone; see below). Approximate: the margin scales with retransmits, which r changes. | reported |
| **granted r (secondary)** | `[SPAN] rr=` (2 Hz under `RWM_DIAG`, `emit_source.rs:657`). It prints the cached per-symbol TaperBudget grant from the last taper recompute: after the spare clamp, reused for up to one emission burst (`taper_recompute_due`, `:497`; per symbol at Realtime, where batching is out of scope). Each source symbol adds it to `repair_debt`, so its time mean tracks the proactive per-source rate. It has **three decimals**, so B+ (≈ 0.0005) reads 0.000 or 0.001, and I1 (0.007–0.015) resolves only to ≈ 10 %. Mean, median and non-zero fraction over the source phase. | reported; witness W-ROUTE |
| W | `[SPAN] wsize=` (201 on the §53 c8 smoke row; R07's 200 clip) | reported |
| ε̂ per leg | `[DIAG] pN: plu=` (the BOCD upper quantile the law reads) and `plc=` (cumulative) | reported |
| P_fec side | per row: `rr_mean` against `plu/(1−plu)` and `plc/(1−plc)` of the leg the rate read | reported |
| taper copies | `[DIAG] mpr[... taper=]` | reported |
| store-cap pin | `[SUMCAP] pin=`. q(δ) moves the cap, but at c8 it is pinned 0.74–0.98 (F01), so q(δ) is predicted invisible there. | reported |
| spare headroom, approximate | `[DIAG] cwnd= infl=`, and per leg `pN:infl=` | reported |
| lag | the guard's `lag_E_*` | reported only (blind at 20–100 %, §41d) |

**Missing gauges** (the session runs without them; each is reported as MISSING in its readout slot; §8):
- the **law r** before the spare clamp, with r_anchor, r_late and β;
- the **term that wins** inside `controller_rate`: core, burst, mass or the inner floor, with the bind fractions of `soft_saturate(r_sat)` and `max_overhead`. R02 says no gauge exists; `rce=` carries only counts and cost;
- the **spare-clamp bind** fraction (per leg, exact);
- the **coded channel split**. `cod=` sums three emitters: the proactive slot (`emit_source.rs:768`), the NACK margin (`sender_phases.rs:1108-1162`) and request-law equations (`net/mod.rs:3402`, off by default). Until the split exists, the proactive consumed rate is not separable from the margin, and `[SPAN] rr` stands in for it.

**Reconciling "r ≡ 0" with §53's ≈ 0.5 % coded share at c8.**
- At β = 1 the proactive slot cannot fire. The granted rate is 0 (`TaperBudget::accrue`: `owed` stays 0 and the grant is `min(owed, …) = 0`), `repair_debt += 0`, and the emission loop needs `debt ≥ 1` (`emit_source.rs:692-695`). The §53 c8 BASE smoke row (`rep53-diag-smoke.tgz`, `c8-100@def-BASE-s42-r1-a1-c.log`) confirms it: `owed=0.00 rr=0.000 debt=0.00` on 88 of its 88 `[SPAN]` lines (counted).
- The request channel is off by default (`recv_request_tx = None`, `net/mod.rs:1051`).
- That leaves the NACK repair margin: after each gap-serve pass with `retransmitted > 0`, it sends `ceil(retransmitted · ε̂_max)` coded symbols (`sender_phases.rs:1116`). The ceil makes it at least one per pass.
- The same row reads `cum=507616/3073/…`, `retx=14391`, `rep=18083`: 3 073 coded symbols against 14 391 retransmits. *Inference, not counted:* if each pass carries ≈ 4.7 retransmits, that is one margin symbol per pass, which is what the ceil floor gives at ε̂ ≈ 5 %. The number of passes is not printed.
- It is also consistent with the paper's corner reading: the "`ceil(retx·ε̂)` margin" closes `fill_coded` 0.56 % at c3hg CTL, `fec-arq-model.md` §4.13.
- So the ledger's "r ≡ 0" is right about the rate law. The coded share is reactive insurance, not proactive r. Confirming this needs the channel split.

## 4. The continuity assertion, stated statistically

Correctness comes first: under NO-MODE-SWITCH a resolved step is a defect, whatever its size. Each cell is scored on its own, since the cells run in different launches.

- **The statistic is a variant of the guard's rank-minp, not the guard's own.** `place41b_score.statistic_rank` is **one-sided** (worse-signed) and **seed-stratified** (3 v 3 per seed). Both would break T1:
  - a step has no "worse" direction;
  - the stratified two-sided floor is 2/C(6,3)² = 2/400 = 0.005 per clause. Over a 15-clause family, a fully separated step at one preset would then reach a family p of only ≈ 0.015–0.07, so the gate could fail to reject a real step.
- `score51` therefore uses:
  - a **two-sided** rank-sum per clause;
  - **no seed strata** (each arm has 3 rows per seed by construction, so the seed is balanced);
  - the min-p over a **family of one preset** (4 clauses), with the null from exact or 10 000 joint relabellings of the 12 rows, where a row's clause vector moves as a unit;
  - **Bonferroni over the three presets**: FAIL at P iff its family p ≤ 0.05/3.
- **Attainable:** a fully separated 6 v 6 on one clause has a per-clause p of 2/924 = 0.0022, and a family p ≤ 4 × 0.0022 = 0.0087 < 0.0167. So the gate can reject. `score51`'s unit tests must show this on a synthetic ledger in the real block/seed layout (§6 item 4).
- **Scored clauses:** goodput, in-order delay mean, CPU/GB on the client and CPU/GB on the server: the guard v2's c2/c8 clauses minus lag. Retransmits/GB is reported, not scored; it has no d80 yet.

**T1, through-the-dial continuity (the engine twin of `test_visualizer.mjs`).**
- At each preset P ∈ {Bulk, Auto, Realtime}, compare P/1.02 with P·1.02 on the Auto carrier, 6 v 6, per cell.
- **FAIL at P iff its family p ≤ 0.05/3.** The failing preset and clause are named, and the step is entered as a ledger defect.
- After both launches, a **cell-stratified** version (6 v 6 per cell, relabelled within cell) is reported as the joint reading. It does not override a per-cell FAIL.
- What the smooth law moves across ±2 %, from the closed forms at W = 200:
  - r changes by ≤ 0.001 (§5);
  - b by ±0.3 %, w_bw by ±0.002 and q by ±0.1 %;
  - the predicted goodput change is ≈ Δr/(1+r) ≤ 0.1 %, far below every d80.
- So any resolved difference is a step and not the slope.

**T2, the hint as a named point.** The named hint must equal the same δ as a number.
- At Auto: H-A v N-A, 6 v 6 (identical laws).
- At Bulk: H-B v (B− ∪ B+), 6 v 12. At Realtime: H-R v (R− ∪ R+), 6 v 12. The union stands for the dial at P only if T1 passes at P. If T1 fails at P, T2 at P is reported against each nudge separately and not scored.
- There is one family per preset (4 clauses), with the same statistic and the same 0.05/3 threshold. It is a defect quantification, not a merge gate.
- **DNF rows.** An H-R row that hits the 200 s timeout is kept and ranked worst on every clause, the guard's convention; it is not VOID. A named hint that DNFs where the same δ as a number completes is the step itself. DNFs do not enter T1, which uses only Auto-carrier rows.

**A/A noise and resolution.**
- *Before registration (offline estimate, computed for this design).* The source is the §48b BASE 600 MB rows: 12 per cell, goodput and CPU from `run/bbr48/ld.log`, delay from `guard-clause-dry/rowvals-bbr48b.json`. The method is a bootstrap with replacement, 40 draws per grid point and 400 relabellings, with the shift applied to one clause on one side. The design is 6 v 6, family of 4, threshold 0.05/3. The resulting d80:

| clause | c2 alone | c8 alone | stratified c2 + c8 |
|---|---|---|---|
| goodput | 1 % | 5 % | 2 % |
| in-order delay mean | 20 % | 30 % | 12 % |
| CPU/GB client | 8 % | 12 % | 8 % |
| CPU/GB server | 12 % | 20 % | 8 % |

  - It is coarse: the grid is 1/2/3/5/8/12/20/30/50 %, there are 40 draws, and resampling 12 rows with replacement understates the spread.
  - Before registration, the harness worker recomputes it at 200 draws, adds the §53 c8 BASE + BASES rows, and adds the 6 v 12 T2 design (§6 item 5). The recomputed table replaces this one.
  - Even this estimate is far above the smooth law's movement across a nudge (≤ 0.1 % goodput), so T1 cannot be failed by the slope.
- *In session:* H-A v N-A is a same-session A/A at δ = 0.5. Its per-clause bootstrap d80 is reported next to the historical one. If the in-session A/A d80 exceeds the historical one by more than 2×, the session reports reduced resolution.
- **Wording rule.** A non-rejection is reported as "no step above d80 = x % at P", never as "continuous".

**Shape (reported).** The r(δ) table against §5's bands. The crossing δ* is read as the first sweep point where the consumed r (with `rr` beside it) exceeds plu/(1−plu); it is reported with both ε readings. Near δ* (I1, I2) r is 0.007–0.046, while the margin share subtracted from it is ≈ 0.35–0.6 % and varies with retransmits. Without P2's channel split the δ* reading is therefore **provisional**. The session does not wait for P2, but the δ* verdict that `rate-user-rate-law` consumes does (§8). Goodput, delay and CPU/GB against δ are reported per point with median and range. A monotone non-increasing goodput in δ is the model's expectation (§5.3); a violation is reported, not scored.

**VOID and witnesses (per row, two-sided, before any number is read):**
- `[GATES] RWM_DELTA=` equals the arm's value (or `unset`) on both ends;
- `pipe_cli/srv = [window, Rlc, <arm hint>]`;
- the emission-batching echo is `ACTIVE` at bulk/auto and `out of scope` at realtime. This is the expected table witness, and it is not a failure;
- source symbols per byte match the carrier's symbol size;
- `RWM_GEN` is off; `[CHI]` is absent or n = 0;
- **W-ROUTE:** `rr` is exactly 0 on every B− row and on every H-B row. If `rr ≡ 0` on every row with β < 1 in a cell, the session reads R-INERT, as the r battery's W5 did. It is attributed in this order: spare-bound (`cwnd − infl` ≈ 0), estimator-bound (`plu` < t) or wiring;
- **N-A ≡ H-A:** a T2 FAIL at Auto is an instrument or harness defect. The session is NO VERDICT until it is explained.

## 5. Predictions (closed forms)

### 5.1 r per δ point

Computed with `raptorpath-math::controller_rate` (the formula the engine and `raptorpath-wasm` share) through the engine's mix.
- **Inputs:**
  - p is the plu median (§52/§53 ledgers: c2 0.014; c8's c3 leg 0.017) or the cumulative plc (c2 0.026; c3 leg 0.049);
  - per-datagram GE with B = 10 (c2) or 6 (c8's c3 leg), giving σ² = 18.6 and 10.6;
  - W = 200 (R07's clip; `wsize=201` observed);
  - srtt 30 ms at c2 and 250 ms at c8 (the dwell-inclusive app-echo, E01); throughput 90 and 18 Mbit; codec 0.004; base t = 1e-5; χ = 0; saturation cap on.
- **Lower bound:** `MassStats::default()`, so the window-mass term (`RWM_RSTAR_TAIL`, on by default) is inert in this table. With measured mass data r can only rise, up to the soft cap r_sat.
- The script is outside the tree; its inputs are listed here so the table can be reproduced.

| δ | β | c2 (plu – plc) | c8, c3 leg (plu – plc) |
|---|---|---|---|
| 0.0049 (B−), 0.005 (H-B) | 1 | 0 | 0 |
| 0.0051 (B+) | 0.996 | 0.0003 – 0.0005 | 0.0003 – 0.0007 |
| 0.0075 (I1) | 0.912 | 0.007 – 0.011 | 0.007 – 0.015 |
| 0.0158 (I2) | 0.750 | 0.023 – 0.036 | 0.021 – 0.046 |
| 0.05 (I3) | 0.500 | 0.054 – 0.081 | 0.049 – 0.100 |
| 0.49 / 0.5 / 0.51 | 0 | 0.133 – 0.197 (Δ across ±2 % ≤ 0.001) | 0.120 – 0.227 |
| 5 (I4) | 0 | 0.156 – 0.225 | 0.139 – 0.240 |
| 49 / 50 / 51 | 0 | 0.176 – 0.249 | 0.154 – 0.246 |
| r_sat (the soft cap) | | 0.315 | 0.245 – 0.250 |
| crossing ε/(1−ε) | | 0.0142 – 0.0267 | 0.0173 – 0.0515 |
| **crossing δ*** | | **0.011 – 0.012** (β* 0.81–0.83) | **0.013 – 0.018** (β* 0.72–0.79) |

- **What wins (prediction, no gauge).** At Auto the GE core term dominates: core 0.13–0.20 against burst 0.035 at c2, and 0.11–0.21 against 0.013 at c8. The soft cap is slack at c2, but at Realtime on c8 (plc) r sits at r_sat, so the cap is predicted to bind there. The mass term is unpredicted (above).
- **Realtime carrier (H-R).** The 512-byte symbol halves t_sym and T_rtt, so r at H-R differs from R±. This is a table effect through a continuous law, and it is part of T2's Realtime step.
- **The one prior off-corner reading.** Paper §4.13 has the r battery's MID arm at δ = 0.05, the same δ as I3. Its coded share (fill_coded + rep_redundant, which includes the margin) was ≈ 9.9 % of source at sc2 (c2 single) and ≈ 11.7 % at c3hg. The table predicts 0.054–0.081 at c2 and 0.114 for c3 single (W = 200, plc). Same order, coded share a little higher. Caveats: one seed, n = 4, 1.8 MB objects (completion-heavy), the September binary at ≈ 2 Mbit out-of-band on 69 of 72 rows, the CC pinned at Bulk by `RWM_COPA_DELTA`, and MID VOID by W7. It is context, not a baseline.
- **Granted against law r.** `[SPAN] rr` reads after the spare clamp. Spare is expected slack, because Copa's cwnd is inflated by the anchor floor (K01, K04; np_act = 2 on nearly every c8 tick). If spare binds, rr < the table and R-INERT attribution applies.

### 5.2 Where the P_fec step should appear

P_fec has a 0 → ½ cut at r = ε/(1−ε) (R03). Every point with δ ≥ 0.0158 at c2, and with δ ≥ 0.05 at c8, is predicted on the **P_fec > 0 side**. Auto and Realtime are well above the crossing (r ≈ 5–10 × ε/(1−ε)). Bulk is at r = 0, on the P_fec = 0 side. The answer to §3.10's open question is therefore "both, depending on δ, with the crossing between Bulk and I2". At ρ = 1 the cut is read by `r_saturation` only, which does not depend on δ or on the live r. So **no behaviour step at δ\*** is predicted; the step's δ-visible form is the ρ < 1 shed budget (§2.3), out of scope here. A resolved goodput or CPU discontinuity between I1 and I2 would refute this. It would be reported as a finding, since T1 does not test interior points.

### 5.3 Behaviour (from the throughput identity, paper §3.7)

- **Goodput.** On a saturated wire, repair shares the wire, so G(δ)/G(Bulk) ≈ (1 + x_B)/(1 + x_B + r(δ)). Here x_B ≈ 3 % is retransmits plus margin per source at Bulk (c8 smoke row: 14 391 + 3 073 on 507 616), and copies are assumed not saved (preempt ≈ 0.85·fill_coded, paper §4.13).
  - At Auto: c2 0.84–0.89, c8 0.82–0.90.
  - At Realtime δ on the Auto carrier: 0.81–0.86 at both cells.
  - Goodput is predicted non-increasing in δ.
  - At c8 the store cap is pinned (F01) and the frontier is hold-bound, so the c8 ratio is the less certain one.
- **CPU/GB** rises with r: one span-coded repair over A* ≈ 220 symbols per proactive slot on the client, plus decodes on the server. No ns bench of span-repair encode cost exists, so the magnitude is reported, not predicted.
- **In-order delay.** No sign is predicted. FEC closes holes without an ARQ round, but decode-closed holes measured 4–6× slower than copy-closed ones (paper §9.8), and the extra wire load adds queue at c8. The clause is two-sided.
- **T1:** PASS at all three presets in both cells (the δ laws are continuous by construction and pinned by unit tests). A FAIL is a new defect.
- **T2:**
  - **Auto: PASS** (identical laws).
  - **Bulk: PASS predicted.** The only table difference is `queue_target_mult` 1.25 against 1.125. A FAIL is a ledger verdict on K01's live hint table.
  - **Realtime: FAIL predicted in both cells.** CPU/GB rises from 512-byte symbols (≈ 2.4× packets per byte) and from batching being out of scope (S09). Every constant counted in symbols halves in bytes on that carrier (§1.2): the per-path pool 2048, the c8 cap clamp 4 096 (≈ 4.9 MB → 2.1 MB), W = 200, A*, store boot 128 and the emission burst 64. At c8, where the cap is already pinned (F01), H-R is predicted store-bound: lower goodput and higher delay. That is also the DNF risk against the 200 s timeout. Goodput and CPU change at c2 without S08. At c8, retransmits/GB, duplicates and goodput additionally change from S08. The c2/c8 contrast attributes S08. Each failing clause is entered in the ledger as the measured size of its §3.3 debt.

## 6. Harness needs (a Sonnet worker, no cargo)

1. **`loopdiag_battery.sh` plan `51`:**
   - `LD_CELL=c2-100|c8-100`, one cell per launch;
   - the arm table of §2.2, with a per-arm hint replacing the hard-coded `bulk` at `:293` (echo) and `:313` (`perf_rwm_c.sh` argument 3);
   - per-arm `RWM_DELTA`;
   - the env scrub `-u RWM_DELTA -u RWM_COPA_DELTA -u RWM_COPA_WIRE -u RWM_COMPLETION_EXPOSURE -u RWM_MIN_R`;
   - `RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=200`, 600 MB;
   - the six-block order with adjacent nudge pairs, and the smoke rows.
2. **`loopdiag_run_all.sh` plan `51`:**
   - one tree (BASE = main at launch, `LD_BASE_SHA` required), no suites (nothing merges);
   - a VM pre-flight running the dial pins on the built tree: `net::tests::delta_budget_b_is_the_dial_not_a_mode`, `scheduling_weights_are_the_dial_not_a_mode`, the `control::fec_rate` no-step tests and `raptorpath-wasm` `test_continuum_one_law_across_the_dial`;
   - `LD_HARD` with ABORT-BUDGET before each block (start a block only if ≥ its rows × 215 s remain);
   - `LD_REHEARSE=1` support.
3. **Row parser** (`place38_parse`, `LD_PLAN=51`):
   - `ARM_SPEC` per arm: the hint in the pipe witness (today it pins `bulk`, `:40`/`:172`), and the batching echo per carrier (`ACTIVE`, or `out of scope` at realtime);
   - the two-sided `RWM_DELTA` gate witness on both ends;
   - new fields: `r_consumed = (cod + taper)/src` (with the same-session H-B share subtracted in the scorer), `rr_mean`, `rr_p50` and `rr_nz_frac` from `[SPAN]` (source phase), `wsize_med`, `src_per_byte`, `sumcap_pin`, and `cwnd`/`infl` headroom;
   - existing fields reused: in-order delay, CPU/GB, retx/GB, `rx_dup`, `cod`, `taper`, `plu`/`plc`.
4. **Scorer `score51`** (a new `tools/l1/s51_score.py`). It reuses `place41b_score`'s rank machinery but not its statistic: it is **two-sided** and **not seed-stratified**, with a family per preset and Bonferroni over the presets (§4). It also carries the reported cell-stratified joint T1:
   - T1, T2 and the in-session A/A d80;
   - the W-ROUTE / R-INERT attribution;
   - the r(δ) table with §5.1's bands beside it, the P_fec-side reading and δ*;
   - the VOID rules of §4.
   - Unit tests: a fully separated synthetic step on one clause at one preset, in the real block/seed layout, FAILs T1 (family p ≤ 0.0087); a step in the *better* direction FAILs too (two-sided); a smooth slope of Δ = 5·d80 across the full dial PASSes T1; N-A ≡ H-A PASSes T2; a missing `RWM_DELTA` echo VOIDs the row.
5. **Pre-registration d80 table:** `guard_power.py c` adapted to `score51`'s statistic at 6 v 6 (T1) and 6 v 12 (T2), 200 draws, on the §48b BASE and §53 c8 BASE/BASES 600 MB rows. It replaces §4's 40-draw estimate and is committed with the registration.
6. **Stub-run path:**
   - `stubrun_gen.py` emits `[SPAN] … rr=<x>` with x > 0 for β < 1 arms (today it emits `cod=0` and no `[SPAN]`), the realtime pipe echo and its batching echo, realtime-sized `src` counts, and `[GATES] RWM_DELTA=<v>`;
   - `HARNESS_PATHS["51"] = ("rehearse", "full")`;
   - `test_stubrun.py` executes both paths.
7. **`LD_REHEARSE=1` must show:** one row per arm kind (H-B, H-R, B+, N-A) LIVE, with the `RWM_DELTA` echo on both ends, the hint pipe, `rr` parsed (exactly 0 on H-B; > 0 on N-A unless R-INERT. B+ is not a witness: its r ≈ 0.0005 prints as 0.000 at three decimals), the in-order delay non-None, and the H-R row inside the 200 s timeout.

## 7. Session length (`LD_HARD` = 5 h per launch)

- **Per launch (one cell):**
  - build ≈ 6 min, pre-flight dial tests ≈ 5 min, smoke 4 rows ≈ 5 min;
  - 76 rows. A row is the transfer plus ≈ 15 s of bring-up: H-B ≈ 53 s at c2 (≈ 52 s at c8 with its tail); off-Bulk rows ≈ 60–67 s at the predicted goodput; H-R up to ≈ 90 s. That is ≈ 75 s mean, ≈ 95 min.
  - Expected total ≈ 2 h. Worst case at the 200 s timeout: 76 × 215 s + 16 min ≈ 4 h 50 min, inside 5 h.
- **Two launches** (c2 first: the lower-noise cell, and it settles the S08-free table step), plus one `LD_REHEARSE=1` launch (≈ 20 min) before them.
- One launch for both cells would be ≈ 3.7 h expected but ≈ 9.5 h at the worst case, so it does not fit.

## 8. Prerequisites found (candidate tech-tree nodes)

| id | node | gates §51? | lane |
|---|---|---|---|
| P1 | **`meas-s51-harness`**: §6 items 1–7 (plan 51, parser, `score51`, d80 table, stub path, rehearsal). | **yes** | no-cargo |
| P2 | **`rate-fix-rate-readout`**: a `[RATE]` readout on the `[DIAG]` cadence, behaviour-neutral. It carries the cadenced law r (mean, p50), r_anchor, r_late and β; the tail target; the spare-clamped `flat_rate` mean with a per-leg spare-bind fraction; `soft_saturate` and `max_overhead` bind fractions; the W* = 200 clip bind (R07); the path the rate read; and the fraction of evaluations on the P_fec > 0 side. It also splits `cod=` into proactive, margin and request, and echoes the resolved dial (δ, β, b, t, w_bw, q, symbol size) once at start-up on both ends (rule 1). Without it §51 reads the granted r only (`[SPAN] rr`) and reports the law r, the clamps and the channel split as MISSING. | not the session, but it **gates the δ\* verdict**: at I1/I2 r is 0.007–0.046 against a margin share of ≈ 0.35–0.6 % that the session can only subtract approximately. Recommended before the session. | host-cargo |
| P3 | `rate-fix-term-gauge` (exists): which `max()` term of `controller_rate` wins, per evaluation. It can merge into P2 if `controller_rate` returns its argmax bit-identically (a wasm golden check). Otherwise §51 reports it MISSING. | no | host-cargo |
| P4 | **`meas-s51b-rho`**: the ρ < 1 block. It needs a numeric or at least carrier-independent ρ on the CLI (today ρ < 1 is reachable only at Realtime), perf completion with declared loss (delivered fraction), a per-chunk send stamp, and the paired readout of §2.3 (resolution delay with the abandoned fraction). This is where R03's step becomes δ-visible (the shed budget). | no (follow-on session) | host-cargo, then vm |
| P5 | **ns bench of span-repair encode and decode** at A* ≈ 220, 1200 B (the `nsbench41c` pattern), so CPU/GB against r can be predicted next time. | no | vm |
| P6 | **Ledger row for `symbol_size_for_hint`** (512 / 1200, plus the perf chunk): a live hint table and a step at Realtime. It has no ledger row today (only paper row 13). T2 at Realtime measures it. | no (record) | no-cargo |
| P7 | **Finding, the CLI couples ρ to the hint:** `--window-reliable` can only set ρ = 1, and the ρ < 1 default exists only at Realtime. This is an interface-level NO-MODE-SWITCH debt for status §3.3. | no (record) | no-cargo |

The defects §51 is predicted to quantify (T2 at Realtime) already have nodes or ledger rows (S08, S09, K01, P6). §51 only measures them.
