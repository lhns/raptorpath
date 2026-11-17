# s61: the c3 sweep (is S_F the knee's term at a second leg shape?), pre-registration

**For the owner, in plain words.**
- Your decision 8 (2026-11-17) gives the store law's allowance the form **g(δ)·S_F + q(δ)·F̂**, with g(Bulk) = 1.
  S_F is the no-load spread of the echo loop: the floor windows' median minus their minimum.
- That form rests on one cell. At c2 the knee needs a window-minimum rise of **ρ = 0.80** of S_F.
- **This session asks whether a c3-shaped leg needs the same fraction.** c3 is 20 Mbit/s, 20 ms one way, with 5 ms
  jitter. Its jitter-to-loop ratio is about a third of c2's.
  - **Same fraction (AGREES):** S_F has measured provenance at two leg shapes. g's Realtime end can then be read
    from the pooled ρ.
  - **Different fraction (DISAGREES):** S_F is not the knee's term, and the form is re-derived, as decision 8 says.
- **One thing had to be fixed first.** A window's minimum falls as the window holds more samples. A c3 floor window
  holds about 60 samples, where a c2 one holds about 280. So c3 is read at c2's own sample counts, by pooling
  consecutive windows. The rule is tested on §60's data:
  - it turns c8 leg 1's (c3-shaped) S_F from NOT-CONVERGED into flat;
  - it leaves c2's 0.80 unchanged, because c2 is read natively.
- **It also prices the side below the knee**, at c3 and at c2 (W 96 / 128 / 144, which §60 lacks). The table gives
  goodput, window minimum and Tp50, for your later call on Realtime.
- **The cost:**
  - one rehearsal with a two-row knee pilot;
  - launch 61a: c3, 66 rows, typical 1.7 h, worst 3.7 h;
  - launch 61b: c2, 30 rows, typical 0.9 h, worst 1.7 h;
  - both on the §60 gauge binary rebuilt from the same tree, with no new engine code.
- **Predicted:**
  - S's sentence gives ρ₃ ≈ 0.80, so AGREES. That resolves if c3's per-replicate scatter is at most about twice c2's
    in relative terms.
  - Every rival sentence lands at least 0.17 away, so DISAGREES.
  - The decided form at g = 1 covers c3's knee by ≈ 5.8 ms. The registered A0 form falls short by ≈ 1.1 ms.

Status: DRAFT REGISTRATION (derivation style). Docs and Python only: no engine code, no cargo, no VM contact. Branch
`s61-c3-design` off main `0e4c4113`. It is written to be copied into `docs/status.md` as §61.

It is built on:
- `store-law-r68.md` §1.4, §3 and §4;
- the owner decisions of 2026-11-17, items 8–10;
- the §60 Result and its ledger `docs/l1-raw/s60/`;
- `s60-registration.md` (the pattern), and the §60 harness on main (`tools/l1/plan60.py`, `s60_score.py`).

**Advisor (Fable) log: what each call changed.**
1. **Before writing (design sketch).**
   - **The sample-count rule must give realised-n parity at both sides.** The first draft grouped c2's knee to
     n* = 2 400. That closes c2's groups on two windows (n ≈ 4 600), a 0.35–0.55 ms drop in knee tmin, about Δ in ρ.
     So c2 is now read natively, c3 is grouped to c2's native counts, and parity is a printed scoreability
     condition (§3).
   - **ρ₂ is a number under one rule only.** It is 0.797 natively and 0.837 under a partial grouping. The owner's
     0.80 is the native number, and s61 keeps it (§5.2).
   - **One validity rule at both cells.** s61 drops LATCHED (no s61 reading consumes b0p), so ρ₂ is recomputed on
     all six W 16 rows. The point value is unchanged (§5.2).
   - **The floor gate changes under matched n.**
     - At c2, the F slope at matched n is +0.034 ms/slot, NOT-CONVERGED where §60 read CONVERGED.
     - S_F's slope turns negative, so the gate is two-sided.
     - COVERED and SHORT carry a numeric floor bracket in place of "only if CONVERGED" (§5.1, §5.3).
   - **Collapse rule.** The first excess-ratio clause could not see a collapse; only G r1 fired. CR61 now has three
     clauses (§4.1), each checked on every §60 G row: an absolute X against the row's own tmin below the knee,
     joined by a goodput cut; qshare at and above it; and the ratio (raised to 1.5, above the c8 control's 1.40).
   - **Folded in:**
     - the falsifier's half-width and the rival sentences, computed from data;
     - the c2 add-on's own control and an overlap arm;
     - a control-pool go/no-go (netem's 1 000-packet limit);
     - B1 margins as numbers;
     - topo_dual.sh confirmed to shape the c3 leg like topo.sh.

**Scripts** (stdlib, `python -I -X utf8`, output beside them in `store-law/`):

| script | input | output | what it computes |
|---|---|---|---|
| `s61pred.py` | `l1-raw/s60/rep60a-diag-ld.tgz`, `rep60b-diag-ld.tgz`, read in place; imports `r68allow.py`'s loader | `s61pred.out` | §0: per-arm window sample counts and per-slot goodput. §1: S_F and F under the grouping rule G(n*), with the floor gate per n*. §2: ρ₂ with its bootstrap, the knee tmin's sensitivity to n, and the matched-n floor slope. §3: c3 predictions from c8 leg 1, ρ₃ under each rival sentence, and D per candidate. §4: the collapse study. §4c: CR61 on every G row. §4b: B1's `err` at the floor pools. §5: the c2 price table and chords |

---

## 0. What is measured, and the form it serves

**The decided form** (owner, 2026-11-17, item 8; r68 §1.4 row S with g):

```text
   allowance  Aᵢ  =  g(δ)·S_F,ᵢ + q(δ)·F̂ᵢ          g(Bulk) = 1,  q(δ) = (b + 1)/30 ∈ [0.05, 0.10]  (RFC 8289 band, cited)
   S_F,ᵢ  =  median over the floor windows of (p50 − T̂min)                 measured on the windows F̂ is read on
   fixed point (one leg):   T̂min(n*)  =  F̂ + g(δ)·S_F + q(δ)·F̂
```

- **S's sentence (r68 §1.4).** At the knee, the bottleneck's FIFO has absorbed the lower part of the no-load echo
  spread. So the window minimum must rise by a fixed fraction ρ of that spread:

  ```text
     need  =  T̂min(W₉₅) − F      ≡      ρ · S_F                 ρ the same at every leg shape (the hypothesis)
  ```

- **Shape check.**
  - Units: ms on both sides, and ρ is dimensionless.
  - Order in N: 0.
  - It is monotone in the spread, with no load term. The window-minimum rise is a property of the path's spread,
    not of its loop (the loop-scaled rival) and not a constant (the A(c) rival).
- **Measured once.** At c2, ρ₂ = 0.797 (`s61pred.out` §2): 4.05 ms against S_F 5.08.
  - c8 leg 0 is c2-shaped, and its knee is set by the coupling.
  - c8 leg 1 is c3-shaped, but it never reaches a knee of its own.
- **What a second shape decides.**
  - **AGREES:** g's Realtime end can be set from ρ. With g_R = ρ̄, Realtime lands q_R·F̂ above the knee at every
    cell. With g_knee = ρ − q_R·F̂/S_F, it lands at the knee.
  - **DISAGREES:** S_F is not the knee's term (decision 8's falsifier).
  - Whether Realtime may land below the knee is decided later, from the price table (§5.4).

---

## 1. The cell: `c3-25`, single leg

**The netem line, confirmed.**
- `lib.sh` `scenario_params c3` is `20mbit 20 5 2 40`: rate 20 Mbit/s, 20 ms one way, 5 ms jitter, GE p 2 % /
  q 40 %, so ε = 4.76 %.
- **On `topo.sh up c3`:**
  - the data egress `cli0` carries delay 20 ms, jitter 5 ms, rate and GE loss;
  - the ACK egress `srv0` carries delay 20 ms, jitter 5 ms and rate, with no loss (`topo.sh:74–87`);
  - so jitter is on both directions.
- **c8's second leg** (`topo_dual.sh`: data `netem_args c3`, ACK `srv1` delay, jitter and rate, `:104–106`) has the
  same shape. `test_topo.sh:187/254` asserts the data line.
- **Jitter and ordering.** No `distribution` is given, so the jitter is netem's uniform ±5 ms. `rate` makes the
  queue FIFO (`lib.sh:176`).
- **netem's queue limit** is the default 1 000 packets (skbs). No `limit` is set.

**The cell name.**
- `c3-25` exists in `stage3_parse.CELL_SPEC` as `("c3", "c3", "single", 25000000)`. So `place38_parse.bdp_bytes`
  already returns 100 000 B, and `qcwnd_below_bdp` works unchanged.
- `loopdiag_battery.sh`'s `cell_spec` gains `c3-25) echo "c3 c3 single 25000000"` (§9). Every row's bytes come from
  `plan61.py`.

**BDP and rates.**
- Propagation BDP = 20 Mbit/s × 40 ms = 100 000 B (`place38_parse` comment).
- The echo BDP (r68 §3) = 1 925 sym/s × 45.9 ms ≈ 88 slots.
- **Per-slot goodput at the floor** (§60 `s61pred.out` §0):
  - c2 W 16 gives 0.632 Mbit/s per slot at tp50 13.7 ms. Scaled by c3's floor loop (c8 leg 1 p50 46.1–46.3 ms), that
    is ≈ **0.19 Mbit/s per slot**, the fast basis.
  - Scaled by tmin (≈ 4.4×) it is ≈ **0.14**, the slow basis.
- **Control goodput** ≈ 16–17.5 Mbit/s:
  - c8's W 1 280 control minus c2's control is ≈ 16 Mbit/s;
  - `stage3_parse` records the c3 bulk era value as 17.05–17.48.

**Predicted knee.**
- The linear store-limited reach u = 0.95 falls at W ≈ 82 (fast) to 112 (slow).
- c2's knee sits 1.24× beyond its own linear point (168 against 136).
- So **W₉₅ ≈ 100–140, central ≈ 115**. This is too wide to fix arms now, hence the pilot (§2.2).

**Sample counts.**
- §60 c2 reads 17.6 echo samples per slot per 250 ms window at the floor (282 at W 16). That scales as 1/loop.
- c3 floor windows are predicted to hold nk ≈ 63 / 84 / 125 at W 12 / 16 / 24, and knee windows ≈ 500–600.
- **This is the reason for §3.**

**Bytes and timeouts** (each row clears LATE, t ≥ 15 s as at c8, with ≥ 30 s of LATE windows at the fast basis):

```text
   bytes(W)  =  53 s · min(0.19·W, 17.5) Mbit/s / 8, rounded to the nearest 5 MB, ≥ 15 MB      (floor 15 / 20 / 30 MB at W 12 / 16 / 24; 120 MB at and above the knee bracket)
```

| arm class | bytes | transfer, fast / slow basis | `RWM_PERF_TIMEOUT_S` |
|---|---|---|---|
| floor 12 / 16 / 24 | 15 / 20 / 30 MB | 53 / 71 s | 120 |
| below-knee (§2.3) | the formula, ≤ 120 MB | 53 / ≤ 71 s | 120 |
| knee bracket, P1 | 120 MB | ≈ 55–60 s (at capacity) | 120 |
| control 2 048 | 120 MB | ≈ 55 s, plus the fill of a ≈ 1 s standing queue | 120 |

- The 120 s timeout is ≥ 1.7× the slow-basis transfer. A DNF is a datum, and a floor-pool DNF is re-run once (§60's
  rule).
- **The control's standing queue.**
  - 2 048 slots on an ≈ 110-slot knee leave ≈ 1 940 datagrams queued, about 1.1 s of loop.
  - That is ≈ 570 skbs at c3's measured GSO factor 3.4 (`lib.sh:166`), under netem's 1 000-packet limit. But it is
    over the limit if the factor fell to c8's slow-leg 1.5.
  - So the control carries its own go/no-go (§6), with a registered fallback to W 512.

---

## 2. Arms and replicates

### 2.1 Floor pools: W 12 / 16 / 24

- **Above the store floor 10** (`STORE_CAP_FLOOR`, `sender_policy.rs:79`). Each pool binds, since the ceiling is
  max(N·pool, floor) with N = 1.
- **Below 0.3 × the echo BDP**, about 26 slots.
- Predicted u ≈ 0.10–0.14 / 0.14–0.18 / 0.20–0.28. c2's floor pool W 16 sits at u 0.11.

### 2.2 The pilot (two rows, in the rehearsal) and the arms it places

**The pilot rows.**
- c3 G at W 88 and at W 128, with the rehearsal's c3 control row (§6) as the reference.
- u_p = gp_p / gp_ctl.

**The registered rule** (Ŵ, the pilot knee):
- If u₈₈ < 0.95 ≤ u₁₂₈: Ŵ is the linear interpolation in (W, u).
- If both are < 0.95: Ŵ = 128 + (0.95 − u₁₂₈)·(128 − 88)/(u₁₂₈ − u₈₈), extrapolated on the secant.
- If both are ≥ 0.95: Ŵ = 88·0.95/u₈₈ (store-limited proportionality below the knee).
- Ŵ is clamped to [64, 192].

**The arms**, with r4 meaning rounded to a multiple of 4:

| class | arms | c2's analogue (knee 168) |
|---|---|---|
| knee bracket K | r4(0.95·Ŵ), r4(1.05·Ŵ), r4(1.14·Ŵ) | 160 / 176 / 192 = 0.95 / 1.05 / 1.14 × 168 |
| below-knee B | r4(0.57·Ŵ), r4(0.76·Ŵ), r4(0.86·Ŵ) | 96 / 128 / 144 (§2.4) |
| P1 | r8(1.5·Ŵ) | 256 = 1.52 × 168 |
| control | 2 048 (fallback 512, §6) | 2 048 |

- The rehearsal scorer prints `PILOT61 W=<Ŵ> u88=… u128=… arms=…`, and the launch passes Ŵ verbatim as
  `LD_KNEE61`. 61a refuses to start without it (`ABORT-PILOT`).
- **The pilot's own error.**
  - On c2's curve, a secant from u ≈ 0.6 / 0.85 probes reaches 0.95 about 4 % low, because u is concave in W.
  - The bracket spans −5 % to +14 %.
- **Scoreability rule:** a replicate whose in-session W₉₅,r falls outside [K₁, P1] is UNSCOREABLE for need and D,
  because the interpolation would clamp. A cell needs ≥ 5 scoreable replicates.

### 2.3 The c3 grid (launch 61a)

- **Arms:** floor 12 / 16 / 24, B ×3, K ×3, P1 and the control: 11 arms × 6 replicates = **66 rows**.
- **Hint:** Bulk on every row, so the printed q = 0.10. RWM_SLAW = 1 and RWM_DIAG = 1 on every row, with
  RWM_RTOBS = 1 and RWM_RDIAG = 1 as §60.
- **Order:** a cyclic Latin square across replicates, with seeds 42 / 7 alternating (`plan60._grid_rows`).

### 2.4 The c2 add-on (launch 61b): the price below c2's knee

§60 has no c2 arm between W 48 and W 160.
- **Arms:** W 96 / 128 / 144 (0.57 / 0.76 / 0.86 of §60's knee 168), W 160 (the overlap arm with §60) and the 2 048
  control, × 6 = **30 rows**, all at 600 MB with a 150 s timeout, as in §60.
- **Its own control:** u is defined per replicate against a same-session control.
- **The overlap arm bounds session drift:**
  - **DRIFT-OK** iff |tmin₁₆₀(s61) − 12.33| ≤ 0.3 ms and |u₁₆₀(s61) − 0.930| ≤ 0.02, as arm medians. The 0.3 is
    §60's τ at c2.
  - Otherwise the c2 price rows are reported as their own session, and §60's arms are not merged into the table.
- **Separate launch:** 61b runs as its own launch. Together with 61a the worst case exceeds 4 h (§8).

---

## 3. The sample-count rule G(n*): a window minimum depends on how many samples it holds

**The formula** (the grouping, per row and leg, over LATE windows in time order):

```text
   groups:   consecutive LATE windows pooled until Σ nk ≥ n*  (the last, incomplete group is dropped)
   T̂min_g  =  min over the group's windows of tmin           EXACT: the minimum of a union is the minimum of the minima
   p50_g    =  nk-weighted median of the group's window p50   an approximation of the pooled median (the gauge prints no samples)
   F_row = median_g T̂min_g,     S_F,row = median_g (p50_g − T̂min_g)
```

**Why it is needed (measured, `s61pred.out` §1 and §2).**
- **c8 leg 1** is c3-shaped (native nk 15–142).
  - Natively, its S_F rises 5.47 → 10.02 ms over W 32 → 256, and tmin falls 40.3 → 37.4. That is r68's "S_F rises
    with the samples".
  - Under G(250) its S_F reads 12.34 / 11.92 / 12.83 / 11.29: **flat**, b = −0.0046 ± 0.0011 ms/slot.
  - Its F reads 33.5 / 33.8 / 33.6 / 36.3.
  - So the native rise was sample count, not load.
- **c2's knee tmin falls with n:** at W 160 it reads 12.33 / 11.98 / 11.54 / 11.02 ms at n = 2 246 / 4 491 / 8 982 /
  17 964 (groups of 1, 2, 4 and 8 windows). That is ≈ 0.4 ms per doubling, against ρ's margin Δ·S_F ≈ 0.43 ms.

**The rule (registered): c2 is read natively; c3 is read at c2's native sample counts.**
- **c2:** native windows, as §60 and r68, so ρ₂ is the owner's number. The reference counts are the medians of §60's
  native nk:
  - n_F,ref = **282** (c2 W 16);
  - n_K,ref = **2 342** (c2's knee bracket rows, W 160 / 176 / 192: 2 246 / 2 342 / 2 392).
- **c3:**
  - every floor-pool row is read under **G(250)** (realised ≈ 250 + nk/2 ≈ 280);
  - every knee-bracket, P1 and below-knee row under **G(2 100)** (realised ≈ 2 100 + nk/2 ≈ 2 350);
  - the control is not grouped, because it enters only through goodput.
- **Parity, a printed scoreability condition:**
  - the realised median n per group must lie within [0.8, 1.25] × the reference: [226, 353] at the floor and
    [1 874, 2 928] at the knee;
  - otherwise ρ₃ and D are **UNSCOREABLE-N**, and the native reading is printed beside them;
  - a c3 window cannot be split, so a native nk above the upper bound cannot be repaired.
- **Provenance.**
  - n_F,ref and n_K,ref are measured (c2's §60 rows).
  - G's minimum is exact, and its median is an approximation, labelled.
  - The parity band [0.8, 1.25] is declared. At c2's ≈ 0.4 ms per doubling, a 25 % mismatch moves the knee tmin by
    ≈ 0.13 ms, ≈ 0.03 in ρ, a third of Δ.
- **Why matching n is the right normalisation for a ratio test.**
  - A uniform jitter of half-range J has a window-minimum bias that scales as J/√n in its lower tail.
  - Matching n on each side across the cells makes every bias proportional to the cell's own spread, so ρ is
    invariant under S's sentence.
  - The floor and knee sides keep c2's unequal native counts, which are common to both cells.
  - At c3, G(2 100) spans ≈ 1 s ≈ 21 loops, against c2's native 250 ms ≈ 18 loops. So matching samples also
    roughly matches loops.
- **Native reported beside.** Every c3 reading is also printed native, as the engine's per-window estimator would see
  it. That reading decides nothing.
- **For the engine form.** If ρ AGREES, S_F and F̂ have provenance only under a sample-count rule, which is R6.7 open
  item 4. The engine's estimator must carry one, and that is a design item.

---

## 4. Row validity

### 4.1 CR61: the collapse rule, fixing §60 Result item 4

**What §60 got wrong.** §60's rule measured the excess of the EWMA `rtt=` over the row's minimum `rtp_us`, against an
X derived on small pools. It flags the control pool's own standing queue: every c1s W 2 048 row, and at c2 W 2 048 an
excess of 194–197 ms against X 14.7.

**CR61, per G row, leg 0** (`s61pred.out` §4c):

```text
   e_own(tick)  =  rtt=(tick) − (the row's median LATE tmin)        the standing queue cancels: it is in both terms
   below K₁ (floor, below-knee):  COLLAPSED  iff  share{LATE ticks: e_own > X_own} > 0.10   AND   gp < 0.95 × the arm's median gp
   at and above K₁ (knee, P1, control):  COLLAPSED  iff  qshare > 0.10   OR   E90_own / (p90 − tmin)_own > 1.5
   X_own = E90 + 2·(E90 − E50),  E50 / E90 = the medians over the cell's reference floor rows of each row's own percentile of e_own
   qshare = share of [DIAG] lines (t ≥ 1 s) with leg-0 qcwnd < the cell's BDP (place38_parse.qcwnd_below_bdp)
```

**Provenance.**
- **The X form is §60's** (E90 + 2·(E90 − E50)), now against the row's own tmin.
  - The reference is the median over rows, so a minority of collapsed rows cannot loosen it. Pooled ticks gave c1s
    6.05 ms; the row median gives 1.59.
- **The goodput cut** is the defining cost of a collapse (§60: collapsed c1s rows delivered 221–671 Mbit/s against
  ≈ 850). As an AND condition it can only remove flags, so it protects the queue-free arms against an X that does not
  transfer upward. At c1s W 192 it removes three false flags.
- **qshare is §56/§57's quinn-window flag.** It is used only at and above the knee bracket. Below it, a store-limited
  flow holds qcwnd under the BDP by construction (c2 W 16: share 1.000 on 3 of 6 healthy rows).
- **The ratio 1.5 is declared post hoc on §60.**
  - It sits between the largest healthy value (1.40, c8 leg 1 W 1 280) and G r1's 1.82.
  - It catches G r1, which qshare misses (0.014). That is the case §60 Result item 4 names.
- **The pool boundary is the arm list (K₁),** not a computed BDP in slots. The brief's "pool ≥ BDP" is the knee
  bracket.

**Checked on every §60 G row** (`s61pred.out` §4c):

| cell | arms | rows flagged | what they are |
|---|---|---|---|
| c2 | 16 / 32 / 48, 160–256, 2 048 | **0 / 48** | every row healthy (no collapse is known at c2) |
| c8 leg 0 / leg 1 | 32–96, 256–1 280 | **0 / 48** each | healthy |
| c1s | W 64 | 2 / 6 | s7 r4 (gp 138) and s7 r6 (175), against ≈ 200 |
| c1s | W 128 | 0 / 6 | — |
| c1s | W 192 | 1 / 6 | s7 r4 (gp 228, against ≈ 460) |
| c1s | W 2 048 (G) | **6 / 12** | r5, r8, r9, r11, r12 by qshare, and r1 by the ratio. These are exactly §60's five collapsed G rows plus the degraded G r1; 0 of the 6 healthy rows |

- The c1s check is post hoc. The c2 and c8 zeros are out of sample: no constant was fitted on them.
- **Frozen X_own:**
  - c2: **10.66 ms**, from §60's six W 16 rows (E50 4.95, E90 6.86). It is used at 61b's below-knee arms.
  - c3: **in session**, from the c3 W 12 rows (minority-robust). Predicted ≈ **19.3 ms** from c8 leg 1's W 256 rows
    (E50 8.64, E90 12.19), a dual-cell stand-in.
- **Gating.**
  - A COLLAPSED floor row is not valid (§4.2).
  - A COLLAPSED below-knee, knee or P1 row leaves its arm's medians and its replicate's knee interpolation, and is
    listed.
  - A COLLAPSED control row is replaced, for its replicate's u, by the median goodput of the cell's unflagged control
    rows. If fewer than 4 controls are unflagged, the cell is **CONTROL-MOVED** and nothing at it is scored.

### 4.2 Floor-row validity and the floor pool

- **A floor row is valid** iff all of these hold:
  - it completes (no DNF; a DNF is re-run once and stays a datum);
  - it is not COLLAPSED;
  - it has ≥ 6 complete groups under G(250) in LATE.
- **The cell's floor pool** is the smallest pool with ≥ 3 valid rows. If W 12 fails, the floor moves to W 16, and
  then to W 24. The move is reported.
- **Dropped from §60, with the reason:**
  - **LATCHED and ō:** they gate b0p, and no s61 reading consumes b0p (D, ρ and the gates read tmin, p50, sig and
    `l`). §60 Result item 6's ō question does not arise.
  - **R̃, R̃₆₇, the R-sign gate and B2:** s61 replays no R. D is the R6.7-form reading on one leg (§60 §1.1).
  - **DECISION 12's leg pooling** does not arise: every s61 row is single-leg, c3 and c2. B2's unbound-leg question
    (§60 Result item 2) stays on the §60 pending list.
- **Kept as validity:**
  - the per-row replay witness (`s60_score`'s `witness`, printed R and Rb to 6·10⁻⁶): a failing row is VOID;
  - the `RWM_SLAW` / pool echo on both ends (VOID, re-run once);
  - `bin_sha` (abort).

---

## 5. Readings

**The common symbols**, per cell (c3 under §3's rule, c2 native):
- **F_c:** the median over the floor pool's valid rows of F_row.
- **S_F,c:** the same median of S_F,row.
- **T̂min_r(W₉₅,r):** the replicate's knee window minimum, interpolated from the row medians at the knee-bracket and
  P1 arms (`r68allow.at_knee`).
- **W₉₅,r:** the replicate's goodput knee, where gp/gp_ctl crosses 0.95 over every non-control arm
  (`r68allow.knee`).
- **σ_r:** the median LATE `sig` over the bracketing arms (native).
- **ℓ̂_c:** the median `l` at the floor pool.
- **Bootstrap:** 2 000 draws over replicates and floor rows, 90 %; seed 61 at c3, and seed 60 at c2 (as `s61pred.py` computed ρ₂).

### 5.1 The floor gates: F and S_F, two-sided, at matched n

**The formula** (§60 §2's fit, per cell, every floor pool from the cell's floor pool up, under G(250) at c3):

```text
   b_X = OLS slope of the per-row X against W over the pools,  E_X = b_X · W₁   (X ∈ {F, S_F}; W₁ = the floor pool)
   CONVERGED  iff  |E_X| + 2·se ≤ τ_X;    NOT-CONVERGED  iff  |E_X| − 2·se > τ_X;    otherwise UNRESOLVED
   τ_F = τ_S = 0.05 · S_F,c      (c3 ≈ 0.60 ms; c2 0.25 ms)
```

**What it fixes.**
- **Two-sided |E|.** §60's one-sided thresholds passed c8 leg 1 with b ≈ 7 se below zero (§60 Result item 3).
  Under matched n, S_F's slope is negative at every cell (c2 −0.0090, c8 leg 0 −0.0057, c8 leg 1 −0.0046 ms/slot),
  so a one-sided gate would pass it trivially.
- **Matched n by construction at c3.** Every c3 floor pool is grouped to G(250), above every predicted native nk
  (≤ 125). The fit compares like with like. That is the precondition the native §60 gate lacked: c2's pools carried
  nk 282 / 536 / 786.
- **The moved pool (§60 Result item 3).** The fit runs over pools at and above the cell's own floor pool. With fewer
  than 2 such pools the gate reads **UNSCOREABLE**, and nothing is extrapolated below the floor pool. A scorer test
  carries a moved pool (§9).

**Provenance of τ.** It is declared from the consequence. A floor bias of τ_F moves ρ by τ_F/S_F = 0.05, and an S_F
bias of τ_S moves it by 0.05·ρ = 0.04. Either is about half of Δ (§5.2).

**Predictions.**
- **c3, S_F:** CONVERGED. c8 leg 1 under G(250) gives |E| + 2se = 0.21 ms (W₁ = 32) against τ ≈ 0.60. The per-slot
  slope on a single c3 leg is not measured.
- **c3, F:** CONVERGED or UNRESOLVED.
  - c8 leg 1 under G(250) gives E_F = +0.43 ± 0.03 ms against 0.60.
  - Scaled from c2's matched-n rise per unit u (0.54 ms at u 0.11, ×10/6 for the jitter), it is ≈ +0.8–1.0 ms at
    u 0.13.
- **c2, reported from §60 at matched n (G(800) over W 16 / 32 / 48):**
  - E_F = **+0.54 ± 0.03** ms: NOT-CONVERGED against τ 0.25;
  - E_S = −0.14 ± 0.04: CONVERGED.
  - §60's native CONVERGED (E 0.18 ± 0.03) was flattered by the sample count, which rises with W and lowers tmin.
- **The consequence: the floor bracket.**
  - Every scored reading below is printed at the floor pool (scored) and at the extrapolated W → 0 floor
    (F − E_F, S_F − E_S).
  - **A label is robust iff both give it**; otherwise it prints with `FLOOR-SENSITIVE`.
  - At c2 the W → 0 values come from the G(800) slope applied to the native F and S_F, an approximation, labelled.

### 5.2 need / S_F: the falsifier of S's sentence

**The formula.**

```text
   ρ_c  =  median_r [ ( T̂min_r(W₉₅,r) − F_c ) / S_F,c ]        (per replicate; pooled = the median)
   Δρ   =  ρ₃ − ρ₂,     90 % CI from 2 000 paired draws (each cell resampled on its own; seeds 61 / 60)
   margin  Δ  =  min over c ∈ {c2, c3} of  q_R · F_c / S_F,c         q_R = 0.05
```

**The comparison value ρ₂ (computed now, `s61pred.out` §2).**
- **0.797, 90 % [0.780, 0.825],** on all six W 16 rows: s61's validity, with no LATCHED rule.
- The §60 / r68 five-row set gives 0.797 [0.772, 0.836].
- This is the owner's 0.80, read natively. The partial grouping G(280) would give 0.837, which is why c2 stays
  native (§3).
- **The W → 0 variant is 0.877** (approximate, §5.1).
- ρ₂ is frozen in `s61_score.py` as its inputs (six knee tmin values and six floor (S, F) pairs), with a host test
  that recomputes them from the §60 tgz.

**The margin, and why it is c2's.**
- The consequence of a common g_R across cells is a landing error of |ρ₃ − ρ₂|·S_F. Realtime stays above its knee
  iff that error is < q_R·F̂ at every cell.
- c2's ratio is the smaller (0.05 × 8.68 / 5.08 = **0.085**). c3's predicted ratio is 0.05 × 33.6 / 12.1 = 0.139.
- So Δ = 0.085 is expected. It is computed in session as the minimum, and it is declared from the law's consequence
  (owner question 1).

**Three-way reading.**
- **AGREES** iff CI(Δρ) ⊂ (−Δ, +Δ).
- **DISAGREES** iff CI(Δρ) ∩ [−Δ, +Δ] = ∅.
- **UNRESOLVED** otherwise. That is a null without power, and is read as `NEEDS-MORE-replicates`, never as a
  refutation.
- **Robust** iff the floor-pool and W → 0 readings give the same label (§5.1).
- **UNSCOREABLE-N** if §3's parity fails.
- **UNSCOREABLE** if fewer than 5 c3 replicates are scoreable (§2.2).

**Predictions, computed first** (`s61pred.out` §3; S_F,3 = 12.07 ms and F₃ = 33.6 from c8 leg 1 under G(250); σ₃
5.17):

| sentence | need₃ (ms) | ρ₃ | Δρ | expected label |
|---|---|---|---|---|
| **S: need = ρ·S_F** | 9.61 | 0.797 | 0 | AGREES |
| constant (A(c): need = c2's 4.05) | 4.05 | 0.336 | −0.461 | DISAGREES |
| σ-scaled (need = 2.27·σ) | 11.72 | 0.971 | +0.175 | DISAGREES |
| loop-scaled (need = 0.466·F) | 15.68 | 1.299 | +0.503 | DISAGREES |

**Resolution.**
- c2's ρ half-width is 0.022.
- c3's per-replicate need scatter is dominated by the knee position: W₉₅'s ≈ 2 % spread times the knee slope, which
  is steeper per slot at 20 Mbit/s. That predicts a ρ₃ half-width of ≈ 0.03–0.05, so a Δρ half-width of
  ≈ 0.04–0.055.
- **AGREES resolves iff |Δρ| ≲ 0.03–0.045; DISAGREES iff |Δρ| ≳ 0.13–0.14.** Every rival lies ≥ 0.175 away, so it
  resolves.
- If c3's relative scatter is more than about twice c2's, S's expected label is UNRESOLVED.

**What it hands the owner.**
- **On AGREES:** ρ̄ (the two cells' pooled ρ, with its CI) and g_knee per cell = ρ − q_R·F_c/S_F,c:
  - c2 **0.711**;
  - c3 predicted **0.657**.
  - **With g_R = ρ̄,** Realtime lands q_R·F̂ above the knee at both cells.
  - **With g_R = g_knee,** it lands at that cell's knee.
  - Anything lower is priced by §5.4.
- **On DISAGREES:** decision 8's re-derivation, with c3's ρ₃ and need₃ as the data.

### 5.3 D per candidate, the decided form at g = 1 included

**The formula** (§60 §3.1, per replicate; Bulk scored, Auto and Realtime reported):

```text
   D_r(A)  =  ( T̂min_r(W₉₅,r) − F_c )  −  A,        D = median_r D_r,  90 % CI by the bootstrap
```

**The candidates** (r68 §1.4, the constants frozen there):

| row | A | role |
|---|---|---|
| **S, decided at g = 1** | S_F,c + q·F_c | the owner's form at Bulk |
| A0 | σ_r + q·ℓ̂_c | the registered R6.7 form |
| A0f | σ_r + q·F_c | finding 2's fix alone |
| Ac | σ_r + 2.261·q/q_B | c_B fitted at c2 Bulk |
| wq | σ_r + 0.260·(q/q_B)·F_c | the wider q band, fitted at c2 |
| W80 | (p90 − p10) at load + q·F_c | refuted by R-sign at c8; reported |

**Labels.**
- **SHORT** iff CI > 0; **COVERED** iff CI < 0; otherwise **UNRESOLVED**.
- Each is robust iff the floor bracket agrees. The W → 0 reading is D + E_F, with E_F's ±2se carried. This replaces
  "COVERED only if CONVERGED" with a number.
- The bracket arms just below and above W₉₅ are reported, as §60 did.

**Predictions** (D at c3 under each sentence, Bulk; c3 inputs as in §5.2):

| A | A (ms) | D under S | under constant | under σ-scaled | under loop-scaled |
|---|---|---|---|---|---|
| **S (g = 1)** | 15.43 | **−5.82 COVERED** | −11.38 | −3.71 | +0.25 |
| A0 | 8.47 | **+1.14 SHORT** | −4.42 | +3.25 | +7.21 |
| A0f | 8.53 | +1.08 | −4.48 | +3.19 | +7.15 |
| Ac | 7.43 | +2.18 | −3.38 | +4.29 | +8.25 |
| wq | 13.91 | −4.30 | −9.86 | −2.19 | +1.77 |

- **Resolution.** c2's D half-width is 0.12 ms (§60: [+1.588, +1.836]). Scaled by the spread ratio (×2.4), c3's is
  ≈ 0.3 ms. So every entry with |D| ≥ 0.3 is predicted to resolve.
- The decided form covers c3's knee under every sentence but the loop-scaled one.

### 5.4 The price table (reported; the owner's later call on Realtime)

**Per arm and cell** (c3 all arms; c2 61b's arms plus §60's 16 / 32 / 48 / 176 / 192 / 256 if DRIFT-OK):
- u = median_r gp/gp_ctl;
- the window minimum (median LATE `tmin`: native, and at c3 also under §3's rule);
- Tp50 (median LATE `tp50`);
- each with its 90 % CI.

**The landing table:**

```text
   W*(g, δ)  =  the W where T̂min(W) − F_c  =  g·S_F,c + q(δ)·F_c   (linear interpolation over the arm medians)
```

- It is printed for g ∈ {0, 0.25, 0.5, g_knee, ρ̄, 1} × {Bulk, Realtime}, each with u(W*), T̂min(W*) and Tp50(W*).
- It shows what each g costs in goodput and buys in delay.
- **No threshold is scored.** The table decides nothing, because the owner's call comes after it. It is checked for
  monotone u and tmin in W, and a non-monotone arm is listed.

**Predictions.**
- **c2** (chords over §60's 48–160, which over-read tmin and Tp50 and under-read u, since u is concave and tmin
  convex):
  - W 96: u ≥ 0.58, tmin ≤ 10.45, Tp50 ≤ 15.54;
  - W 128: u ≥ 0.76, tmin ≤ 11.39, Tp50 ≤ 16.18;
  - W 144: u ≥ 0.84, tmin ≤ 11.86, Tp50 ≤ 16.50 ms;
  - the earlier era's p27 read tmin 10.08 (W 96) and 11.18 (W 128).
- **c3:** u ∝ W below the knee at ≈ 0.011–0.012 per slot, and tmin ≈ F₃ + 0–1 ms below 0.76·Ŵ.

### 5.5 Reported only

- **B1** (a floor row's median LATE `err` > 0). It is no longer predicted "not to fire"; the margins are recomputed
  from §60's floor rows (`s61pred.out` §4b):
  - **c2:** median margin +0.90 to +1.23 ms, but the smallest is −0.06 to −0.11. It fires on 1 of 6 at each of
    W 16 / 32 / 48, so the expected c2 fire rate is ≈ 1/6.
  - **c8 leg 1 (the c3 stand-in):** smallest margin +0.78 to +1.63, median +3.4 to +3.8. So it is predicted not to
    fire at c3.
  - B1 decides nothing in s61, since no reading uses R's calibration end.
- **P1** (b0p at P1 over b0p at the floor pool), from printed fields.
- **Printed `l` per arm**, and printed R and Rb.
- **The c2 drift reading** (§2.4).
- **Every reading native at c3** (§3).

---

## 6. Witnesses, smoke and the rehearsal

**Echo witnesses.** As §60 §5:
- `RWM_SLAW` is scrubbed and echoed in `[GATES]` on both ends;
- the pool knob is echoed;
- `[SLAW] q` = 0.100000 on every row (all Bulk);
- a mismatch voids the row, which is re-run once.

**The rehearsal** (`LD_PLAN=61a LD_REHEARSE=1`, no suites, ≈ 10 min of rows plus the build):
- **R1:** c3 G 2 048 (120 MB). It is the smoke (every gauge tag, the witness PASS and the CAPLOG replay PASS) and the
  pilot's control.
- **R2, R3:** c3 G 12, seeds 42 and 7: the floor go/no-go.
- **R4, R5:** c3 G 88 and G 128: the pilot (§2.2).

**Go/no-go.**
- **Binary:** the build's `bin_sha` = e410d640…5f25d, or `ABORT-BIN-SHA` (nothing measured; owner question 2).
- **Floor:** GO iff at least one of R2 / R3 passes all of these:
  - no DNF;
  - realised floor n under G(250) within §3's band;
  - ≥ 6 groups;
  - not COLLAPSED (CR61; X_own from R2 / R3 themselves, so the goodput clause is read against their pair);
  - leg-0 `nk` > 0 on ≥ 80 % of LATE windows.
  - **NO-GO:** the floor moves to W 16 for the whole of 61a (reported). If the W 16 row of 61a then fails, §4.2's
    rule applies.
- **Control:** GO iff both hold:
  - R1's goodput ≥ 0.98 × max(R4, R5) goodput, or ≥ 16 Mbit/s if neither probe is at capacity;
  - R1's datagram loss truth (the harness's egress counter, rule 19) ≤ 4.76 % + 1.0 pp.
  - **NO-GO:** the control arm becomes **W 512** for 61a (queue ≈ 0.25 s, ≈ 130 skbs), as a registered fallback.
- **Pilot:** Ŵ by §2.2's rule, printed. A pilot row that DNFs is re-run once. Without both rows there is no Ŵ, and
  61a does not launch.
- **Witness checks, as §60 §5 on R2 / R3:** `b0k[0]` = 0, and `m` = `l` for t < 10 s. A failure is reported, not
  voided.
- **The c2 side:** 61b's smoke (one c2 G 160 row) is checked as §60's c2 smoke. It needs no rehearsal of its own,
  since c2's rows ran in §60 on this binary.

---

## 7. Binary, archive and suites

**The tree: `harness61`, a tools-only branch off `harness60-a2` `1ecee59d`.**
- Its engine is byte-identical to `s60-tree` `cb88df3a` (main `aefe6d97` + `slaw-gauge` `d416d59b`).
- `1ecee59d`'s `tools/l1` equals main `0e4c4113`'s (empty `git diff`).
- **Witness:** `git diff cb88df3a harness61 -- raptorpath raptorpath-math raptorpath-wasm raptorpath-core gf256
  Cargo.toml Cargo.lock` is empty. A tools test asserts it before the archive is made.

**The archive.**
- `git archive <harness61 sha> | gzip > h61.tgz`, recorded by sha (§60 Amendment 2: from the commit, not a
  worktree, LF).
- It carries `tools/l1/ref60-manifest.sha256` (187 files of `raptorpath/src` + `raptorpath/tests` at `cb88df3a`).
- The VM's old §60 copy is gone (VM cleanup, 2026-11-16), so the archive is extracted fresh under `~/r61`.

**The binary.**
- The harness's crate-directory build (`src/raptorpath`, as `binid_run.sh` and the battery do) must log
  **`bin_sha` = `e410d64053ab0a143e813aded2e4023176b41edf09d287e795252736b0e5f25d`**, passed as `LD_BIN_SHA_EXPECT`.
- An unequal sha aborts before any row (`ABORT-BIN-SHA`), in the rehearsal first.
- The rehearsal also logs `rustc -V` and `cargo -V`. A mismatch most likely means the VM toolchain moved since
  2026-11-15 (owner question 2).

**Suites are reused by the manifest: yes.**
- `LD_REF_MANIFEST` = `ref60-manifest.sha256`. The engine files are the 60-pre tree's, so every launch must log
  `LD-ALL suite-reuse HIT`.
- 60-pre's green run (release 1156/0, debug 1008/0, wasm, core, doc, parsers; `decision_trace_pin` 7/7) covers
  them.
- A MISS stops the launch (`ABORT-SUITES`), as for plan 60.
- The tools tests (python and bash) still run in every launch.
- 61b reuses 61a's binary (`LD_BIN_PREBUILT`), with its sha re-checked.

---

## 8. Launches, budget and order

**Per-row times.**
- Typical: c3 rows ≈ 53–71 s of transfer plus ≈ 25 s of bring-up and teardown; c2 rows 75–110 s (§60).
- Worst, per row: the timeout plus 40 s (§60 Amendment 1).
- Floor-pool DNFs are re-run once (FLOOR_RERUN_S = 160 s at c3).

| launch | block | rows | typical | worst |
|---|---|---|---|---|
| **61r** (rehearsal) | build (suite reuse HIT) + 5 rows (§6) | 5 | 20 min | 30 min |
| **61a** | build | — | 6 min | 10 min |
| | smoke (c3 G at K₂) | 1 | 2 min | 3 min |
| | c3: 12, 16, 24, B ×3, K ×3, P1, control × 6 | 66 | 1.55 h | 66 × 160 s = 2.93 h |
| | floor DNF re-runs | ≤ 18 | 0 | 18 × 160 s = 0.80 h |
| | **total** | **67** | **≈ 1.7 h** | **≈ 3.95 h** |
| **61b** | prebuilt binary (sha checked) | — | 0 | 2 min |
| | smoke (c2 G 160) | 1 | 2 min | 3 min |
| | c2: 96, 128, 144, 160, 2 048 × 6 (600 MB) | 30 | 0.8 h | 30 × 190 s = 1.58 h |
| | **total** | **31** | **≈ 0.9 h** | **≈ 1.67 h** |

- **`LD_HARD`:** launch + 4.0 h (61a) and + 1.8 h (61b). The budget guard refuses below 3.95 h and 1.67 h.
- **Why two launches:** together the worst case is 5.6 h, against ≤ 4 h per launch.
- **Order:** 61r, then 61a (with `LD_KNEE61` from 61r), then 61b.
  - 61b depends on no 61a reading, only on its binary. It may run first if the VM slot suits.
  - 61b's c2 price rows do not wait on the falsifier.
- **What was cut against r68 §3:** nothing scored. The c1s and c8 cells stay out: c8's knee is set by the coupling,
  and c1s is unread.

---

## 9. For the harness brief (a later Sonnet worker)

**Branch and files.**
- Branch `harness61` off `harness60-a2` `1ecee59d`, tools only (§7's witness). **New files**, in the `plan60.py`
  pattern, so plans 51 / 51e on `harness51` are untouched:
  1. **`tools/l1/plan61.py`:**
     - `C3 = "c3-25"`, `C2 = "c2-100"`, `NLIVE = 1` for both;
     - floor pools, pilot probes, the fractions of §2.2 and the arm derivation from `LD_KNEE61`;
     - the control and its fallback (`LD_CTL61=512` after a control NO-GO);
     - the bytes formula and timeouts (§1);
     - plans `61r` / `61a` / `61b` (Latin square, seeds 42 / 7) and the smoke lists;
     - `armspec`, `row_problems` (plan 60's echo witnesses), `replace` (an echo VOID or a floor DNF, once), `check`,
       and `BUDGET_S` (61a 14 220 s, 61b 6 000 s);
     - `all_arms()` for `place38_parse`'s ARM_SPEC.
  2. **`tools/l1/s61_score.py`.** It reuses `s60_score.row_metrics` (the [SLAW] / [WSTAT] / [DIAG] parse, the
     witness and the CAPLOG replay) and adds:
     - G(n*), with the realised-n parity check;
     - CR61 (§4.1), with X_own in session for c3 and frozen at 10.66 ms for c2;
     - floor-row validity and the floor-pool move (§4.2);
     - the two-sided gates (§5.1), including UNSCOREABLE with fewer than 2 pools;
     - ρ and Δρ, with the frozen ρ₂ inputs and the Δ minimum (§5.2);
     - D per candidate (§5.3);
     - the price and landing tables (§5.4);
     - the drift reading;
     - B1, P1 and the native readings (§5.5);
     - `smoke61` with the rehearsal go/no-go and `PILOT61` (§6);
     - a DECISIONS list for every place this document is silent;
     - labels `LABEL61 <reading> <label>` and a `VERDICT61` line.
  3. **`tools/l1/test_s61.py`.** Unit tests for:
     - G (the minimum exact on hand data, the weighted median, the dropped tail);
     - parity (in the band, below it, above it ⇒ UNSCOREABLE-N);
     - a two-sided gate with a negative slope ⇒ not passed trivially;
     - a moved floor pool, including UNSCOREABLE with fewer than 2 pools left;
     - CR61 on hand rows: a standing queue not flagged; the G r1 shape flagged by the ratio; a floor collapse flagged
       only with its goodput cut;
     - every pilot branch and the clamp;
     - the ρ three-way labels and FLOOR-SENSITIVE;
     - a host-only test that recomputes ρ₂'s frozen inputs from `docs/l1-raw/s60/*.tgz` (skipped in an archive,
       because `docs/l1-raw` is export-ignored);
     - an absolute test: ρ = need/S_F on a two-row fixture.
  4. **`tools/l1/test_harness61.py`:**
     - table counts (5 / 66 / 30), armspec for every slot, bytes and timeouts;
     - the arm derivation at Ŵ = 64, 115 and 192;
     - `ABORT-PILOT` without `LD_KNEE61`;
     - budget values;
     - the engine-diff witness of §7.

**Shared-file edits** (each is a merge point with `harness51` and main; keep them additive):
- **`loopdiag_battery.sh`:**
  - `cell_spec` gains `c3-25`;
  - a `P61` branch in `run_one`, mirroring `P60` (armspec, timeout env, per-row hint and arm env, plan 60's
    instrument env);
  - `P61` in the plan case and the file list (`:84`).
- **`loopdiag_run_all.sh`:**
  - `LD_PLAN` 61a / 61b in the plan case (`:146–148`), the rehearse list (`:156`) and the `crlf_guard` list (`:115`);
  - `P61` riding plan 60's gates: reference-manifest HIT required, `LD_BIN_SHA_EXPECT`, `LD_BIN_PREBUILT` for 61b,
    the budget guard (`:282`), the stage (`:587/:634/:636/:697`);
  - the `LD_KNEE61` / `LD_CTL61` / `LD_NOGO` pass-through.
- **`place38_parse.py`:** register `plan61.all_arms()` beside plan 60's (`:565–567`), and dispatch
  `plan61.row_problems` for plan 61 rows (`:978`).
- **`harness_paths.py`:** `"61a": ("rehearse", "reuse-hit", "reuse-miss", "bin-sha-mismatch", "slaw-echo-void",
  "pilot-missing")` and `"61b": ("reuse-hit", "reuse-miss", "bin-sha-mismatch", "prebuilt")`, plus the docstring.
- **`test_stubrun.py`** and **`stubrun_gen.py`:**
  - the 61a / 61b paths;
  - a c3 cell in the fake driver (rates from §1: 0.19 Mbit/s per slot to a 17.3 Mbit/s cap; nk ≈ 5 per slot per
    window);
  - the fast-mode cap (`REPS_CAP61`).
- **`test_harness47.py`** (if it asserts the plan-case string, as `harness51` found): add 61a / 61b.

**Done criteria.**
- Every `tools/l1` python test passes on the host and inside an extracted `git archive` (LF).
- The 61a and 61b stub paths of `test_stubrun.py` pass in fast mode.
- `test_abort_witness.sh` and `test_lib_battery.sh` pass on the VM (known Git Bash gaps on the host).
- The engine diff is empty.

**Report:** the `harness61` sha, `s61_score.py`'s DECISIONS, and any reading of this document that changes a rule or
a number, as an Amendment before VM contact.

---

## 10. Owner questions

1. **The equivalence margin Δ** = min over cells of q_R·F̂/S_F (≈ 0.085, c2's). It is declared from the consequence
   that a common g_R keeps Realtime above its knee at both cells.
   - A wider margin (for example c3's 0.139) makes AGREES easier, and is right only if Realtime may land below a
     knee anyway, which is your later price-table call.
   - **Proposed:** keep 0.085.
2. **If the rebuild does not give `e410d640`** (most likely because the VM toolchain moved): is a re-pin approved?
   - A re-pin is a new 60-pre-style pre-flight on the same tree: suites ≈ 1 h, a crate-directory build, and a new
     registered sha, as an Amendment.
   - The alternative is to restore the toolchain recorded in the 60-pre logs.
   - **Proposed:** re-pin. The engine bytes are unchanged, so only the binary's identity is re-witnessed.

No other choice in this design changes a scored outcome. The control fallback, the floor move and the pilot are
registered rules.

## 11. What follows each outcome

- **AGREES (robust):** S_F has measured provenance at two jitter-to-loop ratios.
  - The engine form g(δ)·S_F + q(δ)·F̂ can be designed, with g(Bulk) = 1 and g_R chosen from ρ̄ / g_knee and the
    price table.
  - The estimator carries §3's sample-count rule (R6.7 open item 4).
  - F07 moves from UNDERIVED toward a measured magnitude.
- **DISAGREES (robust):** S_F is not the knee's term. The form is re-derived (decision 8), and c3's need₃ against
  each rival sentence is the data for it.
- **FLOOR-SENSITIVE:** where the law reads F̂ (the boot pool against the W → 0 limit) decides the verdict. That is
  R6.7's boot-hold and n₀ item, and it comes before any engine form.
- **UNRESOLVED / UNSCOREABLE-N / CONTROL-MOVED:** `NEEDS-MORE-<named>` (replicates, a smaller floor n, or a control
  pool), never a refutation.
