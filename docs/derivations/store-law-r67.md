# store-law r67: the two floors (ℓ̂, b̂₀⁺), K and n₀, derived

Status: DERIVATION. Docs and Python only; no engine code, no cargo, no VM. Branch `store-law-r67` off `github/main`
(`aefe6d97`). It answers the open item of `store-law.md` R6.7, whose text is on `github/s60-design` (`8536f053`), not
on main. s60 files are cited as `github/s60-design@<sha>:path`.

**Answer in one paragraph.**
- The floors' failure is not specific to static pools. R6.2's line, started at the shipped boot cap 128, leaves
  every cell's queue-free region within 1–2 windows. Its max-of-K brake then ratchets on its own queue: in the replay,
  c2 latches +29 ms and c1s pins at the 2 048 clamp.
- The two-extreme decomposition ℓ̂ + b̂₀⁺, measured against Copa's still-falling running minimum, is what makes b̂₀⁺
  depend on K (§2.2).
- Which anchor branch the line settles on also depends on K and n₀:
  - from n₀ = 10 at c2 the anchor binds (anc 1.00 for K ≥ 3), so F = ℓ̂ + 2·b̂₀⁺(K);
  - from n₀ = 128, or at c1s with large K, it mostly does not.
- So the line's landing is set by an extreme-value statistic of an unprovenanced K. **The double-count question stays
  open per K and n₀.** What is established is the size of the second b̂₀⁺ where it binds: +55 / +93 / +146 slots at
  c2 / c8 / c1s.
- The honest replacement is one quantity: **F̂ᵢ = the median absolute window minimum over the K windows of lowest store
  size seen so far**. It reads no m̂, no b̂₀⁺ and no time index. It starts at the engine's already-derived boot value
  n₀ = 10, needs no boot hold, and K becomes a smoothing length with derived bounds.
- **Adopting it exposes a separate gap, and that gap is what stops it shipping.** With honest floors, the form's
  allowance σ + q·ℓ̂ has no demonstrated margin at c2: it lands 5–33 slots below the knee on the static map, and
  9–39 below in the replay. The second b̂₀⁺ had been supplying that margin. So the recommendation is the floor
  estimator, plus an open allowance item; it is not a shippable law yet.

**Advisor (Fable) log: what each round changed.**
1. **Before candidates.**
   - Truth is redefined as a limit. c2's late T̂min rises 9.45 → 10.08 → 11.18 ms at W 64 / 96 / 128 (u 0.42 / 0.61 /
     0.77), so no boot cap is queue-free in the absolute floor, only in the brake.
   - The anchor is derived with three no-queue quantities: window min, 10 s min, lifetime min.
   - §3.5's one-sidedness is re-checked with σ in the target.
   - Every candidate is screened: mode (any predicate on k, t or n is a phase), oracle (capacity or BDP is E04),
     occupancy (does a/W separate?), ratchet, and K/n₀ elimination.
   - The engine's derived boot value (`sender_policy.rs:84–121`, `STORE_BOOT_DERIVED` = 10) is noted.
2. **Coordinator correction (mid-task).** The c2 "lands at or above its knee by construction" is not established; it
   is K-dependent (`github/s60-design@f8ea7d6c:docs/derivations/store-law/s60flip.out`). So K's provenance is made
   part of this derivation, the line is swept over K in closed loop, and the K mechanism is shown per window (§2.2).
3. **After the numerics.** Three changes followed:
   - The replay was flat below the smallest pool, which made a low start honest by construction. It is re-run with the
     measured slope extrapolated, and both are reported. The landing moves, so the recommendation is conditional on
     the sub-64 map.
   - The c2 sign is reported against both knees (§56 goodput 169; this ledger's delivery knee 181). "Lands below" is
     replaced by "no demonstrated margin".
   - Other fixes: the c1s W 192 collapse pool is excluded from the fits, and c1d's fit is flagged. B = C is stated
     wherever no window is revisited. C is shown to refresh only by collapsing to the floor, so it is not
     route-following. K = 4 is stated as provisional within derived bounds, not as swept.
4. **Before the push.**
   - The anchor claim is weakened. "The only attractor is ℓ̂ + 2·b̂₀⁺" is withdrawn: from n₀ = 128 the anc bind
     fraction is 0.00–0.27 at c2 K 3–7, with finite landings. The window-minimum spread grows with W, so an anc 0
     landing exists. The branch depends on K and n₀.
   - E2 in `r67floor.py` is labelled a static stand-in (occupancy key, ties to the earliest). C's only evidence is the
     replay.
   - The c1s "slope" column is mostly the ℓ clamp (bind fraction 0.64), i.e. F ≈ ℓ̂.
   - Other fixes:
     - c8 under C is unexamined (leg 1's thin low-n windows), and its result is session-only;
     - s57 carries no `[LOOPDIAG]`;
     - the K upper bound is conditional on the sub-64 map;
     - the c2 shortfall is given against both knees;
     - n₀ = 10's unshipped status is quoted from source.

**Scripts** (stdlib, `python -I`, outputs beside them):

| script | input | output | what it computes |
|---|---|---|---|
| `store-law/r67floor.py` | `l1-raw/loop-diag/p27/diag-ld`, `…/p34/run/diag-ld` | `r67floor.out` | (1) truth against W and u; (2) the b̂₀⁺ decomposition and (2b) the per-window K mechanism; (3) the K sweep of each candidate on small pools, bias, sd and collapse margin with and without σ; (4) each candidate on every static pool; (5) the R crossing per cell |
| `store-law/r67loop.py` | the same | `r67loop.out` | a window-level closed-loop replay on the static map (c2, c1s): n₀ ∈ {10, 128} × estimators, the line swept over K with anc bind fraction, a route step |

**Data limits, stated once.**
- **§56 has no window minimum.** It reads 0.1 ms or 1 ms tick means (A.1's stand-in), so §56 cannot evaluate a
  window-minimum estimator. Its numbers here (ℓ scatter, late m̂, σ at load) are cited from
  `github/s60-design@8536f053:docs/derivations/store-law/s60boot.out` and `s60abs.out`.
- p27 rows are about 9 s with n = 2 per pool; p34 c1s/c1d rows are about 6 s, n = 3.
- **§57 (`l1-raw/s57`, rep57a/b) carries no `[LOOPDIAG]`**, so it has no window minima either (checked on
  rep57b-diag-ld). It is not used.
- Nothing is measured below W 64 at c2, below 128 at c8, or below 256 at c1s.
- **Static rows cannot replay a closed loop.** `r67loop.py` is a model on the static map, and it does not model
  c8's coupling.

---

## 1. The failure, stated precisely

### 1.1 Which estimator, which windows

R6.2 reads two floors per leg:
- ℓ̂ᵢ = `WinSampler::life_min`, the lowest echo sample since the first;
- b̂₀ᵢ⁺ = max over k ≤ K = 8 of (T̂min,k − m̂k), where m̂k is Copa's running 10 s minimum at window k.

Both are honest only on windows with no standing queue. b̂₀⁺ reads the first K windows by *time*. On a static pool
above the queue-free size, the store fills within the first window (`[DIAG] win=502/512` at t = 0.3 s, c2 W 512). Every
later window then carries the standing queue, and b̂₀⁺ latches it. p27 c2: 2.2 / 4.2 / 12.6 / 23.5 ms at W 128 / 192 /
256 / 384 (s60 P1; `r67floor.out` §4: E0 reads +1.5 / +4.9 / +13.2 / +24.8 ms above the small-pool truth).

ℓ̂ is the lowest single sample, so it survives only if some early sample is queue-free. On static pools it is caught in
window 1, before the fill (c2 W 512: 10.03 ms at t = 0.3 s). It is also an extreme-value statistic: row sd 0.5–3 ms
at c2, where the absolute late floor varies by 0.10–0.25 ms (s60abs).

### 1.2 What "queue-free" is: a limit, not a pool

`r67floor.out` §1 gives late T̂min (row medians) against W and utilisation u = delivered / plateau:

| cell, leg | smallest pools: W → u → late T̂min (ms) | slope below saturation | queue-free echo BDP μ·T₀ |
|---|---|---|---|
| c2 | 64 → 0.42 → 9.45; 96 → 0.61 → 10.08; 128 → 0.77 → 11.18 | 0.027 ms/slot (3 pools) | 9 624/s × 14.84 ms ≈ **143** |
| c8 leg 0 | 128 → 0.74 → 10.53 (nothing smaller) | — | 9 619/s × 16.0 ms ≈ 154 |
| c8 leg 1 | 128 → 0.06 → 37.54 | 0.008 ms/slot (5 pools) | 1 925/s × 45.9 ms ≈ 88 |
| c1s | 256 → 0.61 → 2.67 (W 192 is the §41b quinn-collapse pool, excluded) | no fit (1 pool below u 0.9) | 95 420/s × 3.84 ms ≈ **366** |
| c1d | 384 → 0.81–0.88 → 2.65–2.72 | fit not meaningful (u is non-monotone in W; flagged) | — |

The window minimum rises with load well below saturation. At c2 it rises +1.73 ms from W 64 to the boot cap 128, at
u 0.77. That is partial-load queueing at the jittered netem bottleneck: a real queue, which the knee needs, but not a
standing queue. **"The queue-free floor" is the limit u → 0**, and every pool is an upper reading of it.

### 1.3 The boot cap min(128, store_max) per cell

store_max is ≥ 1 024 on every arm, so the boot cap is 128.

| cell | 128 / echo BDP | measured at 128 | does a closed loop started at 128 stay queue-free for K = 8 windows? |
|---|---|---|---|
| c2 | 0.90 | u 0.77, T̂min +1.73 ms above W 64 | **no**: R(128) ≈ 1.05–1.09 on the static map (`r67floor.out` §5) and higher in boot. The replay's line reaches n 331 within 8 windows (`r67loop.out`) |
| c8 | 0.53 of total (128 sits on leg 0 at u 0.74, leg 1 at u 0.06) | leg 0 is the only measured point | **no**: R(128) ≈ 2.0 through the coupling term, so the loop leaves within 1–2 windows (§5 of `r67floor.out`) |
| c1s | 0.35 | unmeasured (smallest pool 256) | **no**: R ≈ 1.2 at no queue; the replay reaches n 427 within 8 windows |

**A closed loop at the boot cap does not avoid the latch. It adds a ratchet.** While k ≤ K, the line's target
contains b̂₀⁺ = max over windows so far of the brake, and the brake of a queued window includes that window's queue.
Each window therefore funds the queue it measured plus σ + q·ℓ̂. In the replay (`r67loop.out`, BELOW = flat, n₀ =
128, line as written):

| cell | F latched − truth | landing n* | vs knee |
|---|---|---|---|
| c2 | **+28.7 ms** [+10.0, +39.4] | 458 [352–580] | +289 |
| c1s | **+11.0 ms** | **2 048** (the clamp) on most seeds | +1 145 |

**The boot hold** (Ah: n held at n₀ while k ≤ 8) removes the ratchet: c2 latches +3.5 ms and lands at 214 (+45), c1s
lands at 646. It is a phase, though (a predicate on k), and n₀ = 128 still reads c2's +1.7 ms of partial-load queue.

---

## 2. The anchor: does min(m̂, ℓ̂ + b̂₀⁺) + b̂₀⁺ double-count?

### 2.1 Three no-queue quantities

Write leg i's window minimum as T̂min,k = L + βk + Qk:
- L is the path floor;
- βk ≥ 0 is window k's sampling excess above L, the jitter brake;
- Qk is the window's minimum queue.

Then:
- m̂k = L + min over the last 40 windows of (β + Q), Copa's 10 s filter;
- ℓ̂ = L + min over life of (β + Q).

At anc 0 (m̂ < ℓ̂ + b̂₀⁺) the line's per-leg condition reads:

```text
   R − 1  ∝  m̂ + b̂₀⁺ + σ + q·ℓ̂ − T̂min,k  =  b̂₀⁺ + σ + q·ℓ̂ − [ (βk + Qk) − min₄₀(β + Q) ]
```

- **The sentence:** at anc 0 the target tracks the 10 s minimum.
- **Shape:** the bracket is window k's excess over the 10 s minimum of the same series. That is a *fluctuation*,
  independent of the queue *level*.
- **Consequence:** an anc 0 fixed point exists only where the typical within-10 s excess of (β + Q) reaches
  b̂₀⁺ + σ + q·ℓ̂. b̂₀⁺ is itself a maximum of such excesses over K boot windows, but the excess grows with the
  queue's own fluctuation: the late window-minimum sd at c2 is 0.7–0.8 ms at W 128, 2.2–2.3 at 256 and 2.6–5.9 at 384
  (`r67floor.out` §1). So:
  - where b̂₀⁺ + σ + q·ℓ̂ exceeds the spread at the knee, there is no anc 0 point, n grows until m̂ ≥ ℓ̂ + b̂₀⁺, and
    the anchor binds:

    ```text
       F  =  ℓ̂ + 2·b̂₀⁺          (anc 1; the second b̂₀⁺ is the operating point, not a bound on drift)
    ```

  - where the spread is larger, an anc 0 landing exists further up, at a larger queue.

**Which branch is reached depends on K and n₀** (`r67loop.out`, "A swept over K", anc bind fraction over the last 80
windows):

| | K = 1–2 | K = 3–6 | K = 7–8 |
|---|---|---|---|
| c2, n₀ = 10 | 1.00 (trivial: b̂ ≈ 0, so the bind is m̂ ≥ ℓ̂) | **1.00** | **1.00** |
| c2, n₀ = 128 | 1.00 (trivial) | 0.00–0.27 (the ratchet's large b̂₀⁺; anc 0 landings 219–290) | 0.00 / 0.86 |
| c1s, n₀ = 10 | 1.00 | 0.78–1.00 | 0.77–1.00 |
| c1s, n₀ = 128 | 1.00 | 0.56–0.93 | 0.04–0.10 → 0.00 (clamp at 2 048) |

**What is established:**
- from n₀ = 10 at c2 (K ≥ 3) the attractor is the anc 1 state ℓ̂ + 2·b̂₀⁺;
- the second b̂₀⁺ is worth the slot shifts below wherever it binds.

**What is not established:** that the line always reaches anc 1. Its branch is a function of K and n₀, neither of
which has provenance.

**Where it binds, is that a double count?** Yes, by the sentence. F's job is "the upper reach of the queue-free window minimum".
- One b̂₀⁺ covers window-to-window β above the floor.
- The second covers m̂'s rise above ℓ̂. Under the law's own queue that rise is its own queue (E03), not a no-queue
  quantity, and the clamp stops it only at its bound.
- On the static map the second b̂₀⁺ is worth (`r67floor.out` §5, E0 → E0a at K = 8): **+55 slots at c2** (155 → 210),
  **+93 at c8** (470 → 563), **+146 at c1s** (682 → 828).

**Whether it lands above the c2 knee depends on K.** That is the coordinator's correction, reproduced in closed loop:

| line as written, c2, n₀ = 10 | K = 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 |
|---|---|---|---|---|---|---|---|---|
| landing n*, BELOW flat | 71 | 81 | 112 | 141 | 190 | 195 | 197 | 203 |
| landing n*, BELOW slope | 11 | 23 | 85 | 135 | 177 | 183 | 186 | 196 |

- Against the knee (169 / 181): **below for K ≤ 4, above for K ≥ 5**. That is the same split as s60flip's static
  verdicts (BELOW at K ≤ 4, coin-flip at K 5–8).
- At n₀ = 128 the ratchet takes over from K ≥ 3 (219 → 458).

### 2.2 Why b̂₀⁺ grows with K (`r67floor.out` §2, §2b)

The six c2 small-pool rows behind s60flip show the mechanism window by window:
- **(a)** every window that sets a new running minimum has brake T̂min,k − m̂k = 0 by construction. The early windows
  do that most often: m̂ drops at k = 2–5 on these rows, and m̂₁ − m̂₈ = 0.4–6.9 ms.
- **(b)** b̂₀⁺ is then the maximum of the remaining brakes. It is attained at k = 5–8 on all six rows, and it grows with
  K as a sample maximum does.

So b̂₀⁺(K) traces the running anchor's convergence and the extreme of K draws. It does not trace the brake's
distribution:
- c2 W 64: median b̂₀⁺ = 0.00 / 0.23 / 0.37 / 2.87 ms at K = 1 / 2 / 4 / 8;
- m̂_K − ℓ̂ over the same K: 5.33 / 3.09 / 1.91 / 0.76 ms.

**No K is "the right one" for this statistic**, so K cannot get provenance in R6.2's form.

### 2.3 The collapsed form: one absolute floor

ℓ̂ + b̂₀⁺ is meant to be the upper reach of the queue-free window minimum. Estimate that quantity directly, in absolute
terms:

```text
   F̂ᵢ  =  median of T̂min,k over a set of K queue-free windows of leg i       (no m̂, no ℓ̂, no b̂₀⁺)
```

- ℓ̂'s extreme-value noise cancels here. s60abs found corr(ℓ, floor − ℓ) ≈ −1 with the absolute floor's row sd at
  0.1–0.25 ms: the sum is identifiable, the split is not.
- **It no longer needs to be one-sided**, which re-checks §3.5. §3.5's collapse margin (candidate − b₀ + q·ℓ) predates
  σ. With σ at load in the target, the no-queue fixed-point condition becomes:

```text
   R − 1 at no queue  ∝  F̂ − E[T̂min,0] + σ + q·ℓ̂        collapse iff  E[T̂min,0] − F̂ > σ + q·ℓ̂
```

  So a central F̂ is safe while its error stays below σ + q·ℓ̂: c2 2.3–2.6 ms, c8 2.8–3.3, c1s 0.67–0.80 (q 0.05–0.10).
  Measured on the small-pool rows (`r67floor.out` §3, collapse counts at q = 0.05):
  - with σ: **E1 (central, absolute) collapses no row at any K = 1…16 on any cell or leg**;
  - without σ (§3.5's old criterion): it collapses 1–2 rows at small K;
  - R6.2's E0/E0a, by contrast, collapse c2 and c8 rows at K ≤ 4 even with σ. That is the running-anchor bias of §2.2.
- The max-of-K is therefore not required, and its 45–49 ms tail (§3.5, §13.1) goes with it.

---

## 3. Candidates for "a set of K queue-free windows", screened

| | form (per leg) | mode screen | oracle screen | static-map bias at the small pool, K 4 → 8 (ms; `r67floor.out` §3) |
|---|---|---|---|---|
| **E0** (R6.2) | ℓ̂ + b̂₀⁺, b̂₀⁺ = max_{k≤K}(T̂min,k − m̂k) | phase (k ≤ K) | — | c2 −2.22 → +0.28; c8 leg 0 −0.83 → +0.06 (K-dependent, §2.2) |
| **E0a** | ℓ̂ + 2·b̂₀⁺ (the attractor, §2.1) | phase | — | c2 −1.85 → +3.16 |
| **E1 = B** | median_{k≤K} T̂min,k (absolute, first K) | **phase** (a predicate on k: freezes after K) | none | c2 +0.69 → +0.64; c8 leg 0 −0.02 → −0.04; c1s +0.10 → +0.06 |
| E1x | max_{k≤K} T̂min,k (absolute, one-sided) | phase | none | c2 +2.98 at every K: it carries window 1's transient |
| **C** (recommended) | median of T̂min over the **K windows of lowest store size n seen so far** (ties: most recent) | **none**: an order statistic over the history keyed on the law's own state, like ℓ̂ itself | none | **not evaluable on static rows**: n is constant there, so C reads the last K windows. Its only evidence is the replay (§4.3) |
| E2 (C's static stand-in) | the same, keyed on `[DIAG] win=` occupancy a, ties to the earliest | none | none | c2 +0.30 → +0.27; c8 leg 0 −0.09 → −0.41; c1s +0.01 → +0.01. The ordering by a is arbitrary on static rows |
| occupancy weight | weight windows by (BDP̂ − aₖ)/BDP̂ | none | **oracle**: BDP̂ needs capacity (E04, the R5.4 trap) | not evaluated |
| a/W as the key | key on `[DIAG] win=a/W` | none | none | **does not separate**: a/W ≈ 1 and paused ≈ 60–65 % at both W 96 and W 512 (c2), so on static rows a carries no queue information |
| drain probe | a periodic reduced target (the law's fixed point at q → 0) | **schedule**: a period and depth are new constants, and a time-periodic target is a mode in disguise unless both are derived | none | not evaluated: no ledger has a drain |

**Why C's key is n and not t.** The queue is non-decreasing in the store size: T̂min(n) is monotone on every map in
§1.2. So the windows least contaminated by queue are those at the smallest n the loop has visited.
- **The sentence:** "the floor is the window minimum where the store was smallest."
- **The expression:** an order statistic on (nk, T̂min,k).
- **Shape check:**
  - order 0 in N (per leg);
  - units ms;
  - monotone non-decreasing in contamination;
  - no δ or ρ input;
  - no threshold, no phase, no time index.
- The start is an initial state (n₀), not a mode.
- **In the replay C equals B from n₀ = 10**, because the ramp visits each n once in increasing order. They differ from
  n₀ = 128 (c1s: 580 against 571 flat, 489 against 351 slope) and on revisits (§4.3).

**K's provenance in C: derived bounds, with insensitivity measured inside them.**
- **Lower bound K ≥ 3** (median rejects ⌊(K − 1)/2⌋ ≥ 1 transient window; K = 4 averages the middle two and also
  rejects one). The bound comes from a *measured* transient: window 1 at t ≈ 0.3 s has p50 up to 90 ms (c2 W 96,
  n = 269). E1 at K = 1 carries it: +2.75 ms at c2, +5.67 at c8 leg 1.
- **Upper bound:** the K selected windows must all lie in the low-load region:

```text
   n₀ · R₀^(K−1)  ≤  n_qf      ⇒      K  ≤  1 + ln(n_qf / n₀) / ln R₀
```

  Here n_qf is the largest store at which T̂min is within the collapse margin of its limit, and R₀ is the no-queue
  ratio. With n₀ = 10 and R₀ ≈ 1.17–1.21 (`r67floor.out` §5 at the smallest pools), n_qf = 64 would give
  **K ≤ 11**.
  - **This bound is conditional.** §1.2 shows W 64 is not itself queue-free at c2: it reads about +0.5 ms above the slope
    limit at W ≈ 40. n_qf is set by the sub-64 map, which is unmeasured.
  - The replay's windows 1–8 reach n 34–38 from n₀ = 10.
- **Inside the band:** E1's bias varies by under 0.3 ms for K ≥ 3 on every cell and leg except c8 leg 1 (c2 +0.44 to
  +0.74 ms; c1s +0.03 to +0.10; c8 leg 0 −0.11 to −0.02). E2 is looser: c2 K 3 −0.29, and c8 leg 0 −0.41 to −0.48 at
  K 6–12, on two rows each. This is insensitivity on thin data, not a selected value.
- **K = 4 is provisional.** The two rows per pool at c2 and c8, and E2's arbitrary ordering on static rows, cannot
  select within the band. The registered s60 rows can settle it (§6).

**n₀'s provenance.** The engine already carries a derived boot value: `STORE_BOOT_DERIVED` = max(ANCHOR_MIN_SAMPLES ·
MERGED_ACK_SYMBOLS_PER_SAMPLE, RFC 6928 initial window) = max(8, 10) = 10 = `STORE_CAP_FLOOR` (`sender_policy.rs`).
- It cites RFC 6928 and is derived from "the outstanding bound when the law has no measurement".
- It is not shipped. Its doc comment says the cliff that blocked it is closed and that boot is now reachable only at
  genuine cold start, "not yet re-measured, so it still does not ship". Its floor reading is unmeasured (§4.3).
- Under C, n₀ = 10 needs no hold: queue-freeness of the selected windows is the derived inequality above. It is a
  **domain condition**, BDP ≫ n₀·R₀^(K−1) ≈ 15–17 symbols for K = 4, with the gauge "max n over the K selected windows
  against the n at which T̂min first rises".
- **The cost is the ramp** (`r67loop.out`, windows to 0.95·n*):
  - c2: 17–20 windows from 10, against 4 from 128 (+3–4 s);
  - c1s: 21–44 from 10, against 7–33 from 128 (+1–4 s).

---

## 4. Numerics

### 4.1 Floors each candidate would have read (`r67floor.out` §4; K = 8; ms above the small-pool truth)

| pool | c2 E0 / E0a / E1 / E2 | c8 leg 0 E0 / E1 / E2 | c1s E0 / E1 / E2 |
|---|---|---|---|
| smallest | +0.3 / +3.2 / +0.6 / +0.3 | +0.1 / −0.0 / −0.4 | +0.1 / +0.0 / +0.0 |
| 128 | +1.5 / +3.7 / +2.1 / +1.4 | (smallest) | — |
| 192 | +4.9 / +9.0 / +3.8 / +3.9 | +27.3 / +2.0 / +2.0 | — |
| 256 | +13.2 / +25.9 / +8.3 / +8.8 | +9.4 / +7.4 / +7.0 | (smallest) |
| 512 | +37.9 / +73.8 / +35.2 / +34.2 | +27.1 / +22.9 / +25.1 | +1.0 / +0.6 / +0.5 |
| 2 048 | — | +119.5 / +84.1 / +95.5 | +12.6 / +7.7 / +7.8 |

**No estimator reads an honest floor on a static pool above the queue-free size, because no such window exists after
window 1.** The absolute forms are contaminated at about the standing queue; E0 adds the running-anchor term, and E0a
doubles it. The static ledgers can evaluate candidates only where the pool is small, and as session values.

### 4.2 R crossing per cell (`r67floor.out` §5; floors as session values from the small pools, q = 0.10)

The figures in brackets are slots against the §56 goodput knee and against this ledger's 0.95 delivery knee.

| F source | c2 (knees 169 / 181) | c8 (340 / 361) | c1s (903 / 1 458) |
|---|---|---|---|
| truth (small-pool late T̂min) | 148 (−21 / −33) | 446 (+106 / +85) | 651 (−252 / …) |
| E0 K 8 (anc 0) | 155 (−14 / −27) | 470 (+130 / +109) | 682 |
| **E0a K 8 (the attractor)** | **210 (+41 / +28)** | **563 (+223 / +202)** | 828 |
| E1 K 4 / K 8 | 164 / 163 (−5 / −18) | 440 / 441 (+100 / +80) | 685 / 673 |
| E2 K 8 | 155 (−14 / −27) | 443 (+103 / +82) | 656 |
| any form, each row's own boot estimate | > 1 024 (no crossing) | > 2 048 | > 2 048 |

- **c2 sign: unresolved, with no demonstrated margin.** The honest crossings (148–164) are interpolated inside the
  same 128–192 bracket as the knee, on two rows per pool, and the two knees differ by 12 slots.
- **c8:** honest floors land 80–106 slots above the knee (the coupling half-gap), consistent with s60's band of
  433–586. This is a **session-value result only**. C's own leg-1 floor is unexamined: leg 1 delivers 100–120
  samples/s at W 128, so its lowest-n windows are near-empty, and F₁ sets leg 0's coupling term. It needs a
  per-window sample-count eligibility rule, which is open item 4.
- **c1s:** below the knee under every form. That is the known receiver-bound result (R5.5).

### 4.3 Closed-loop replay (`r67loop.out`; c2 and c1s; 20 seeds, 240 windows)

| cell | n₀ | estimator | F − truth (ms), BELOW flat / slope | landing n*, flat / slope (knee) |
|---|---|---|---|---|
| c2 | 10 | A (R6.2, K 8) | +2.74 / +2.17 | 203 / 196 (169) |
| c2 | 10 | **C (K 4)** = B | **+0.32 / −0.71** | **160 / 130** |
| c2 | 128 | A | +28.7 / +28.7 | 458 / 458 |
| c2 | 128 | Ah (boot hold) | +3.48 | 214 |
| c2 | 128 | C | +3.56 | 218: 128 is already past the queue-free region (§1.3) |
| c1s | 10 | A | +0.26 / +0.16 | 651 / 619 (903) |
| c1s | 10 | C | −0.04 / −0.21 | 570 / 358 |
| c1s | 128 | A | +11.0 | 2 048 (clamp) |
| c1s | 128 | C | −0.02 / −0.19 | 580 / 489 |

- **BELOW = flat** holds the W 64 / 256 statistics down to n = 10, which makes a low start honest by construction.
- **BELOW = slope** extends the measured secant (c2 0.020 ms/slot) down to the lifetime minimum. With C, the floor read
  at n ≈ 10–40 then lies 0.7 ms *below* the W 64 reading, and c2's landing drops to 130.
  - At c2 the ℓ clamp never binds (bind fraction 0.00).
  - **At c1s it binds on 64 % of the below-pool windows.** The c1s slope column is therefore mostly F ≈ ℓ̂ (−0.2 ms),
    not a measured slope.
- **The landing under C is therefore set by the unmeasured sub-64 map.** That is the decisive missing data.

**Route step** (+D ms on the path floor at window 120, n₀ = 10; `r67loop.out` last block):
- **A collapses to the floor and stays there for D ≥ 3 ms at c1s and D = 10 ms at c2.** Its m̂ follows by at most
  b̂₀⁺, and its F is frozen.
- **C recovers from large steps only by first collapsing to the floor.** There the lowest-n set refreshes: c1s D 3 →
  372, D 10 → 278 with F − truth = +2.9 / +9.95; c2 D 10 → 134.
- **C parks on intermediate steps:** c2 D 3 holds n ≈ 83 with min n 36, never reaching the refresh set; c1s D 1 drops
  from 563 to 92.
- **So C is not route-following.** It is the same lifetime-floor domain condition as ℓ̂ (L1 cells have no route
  change), with a self-refresh only through collapse. A route-following floor needs a drain, which no candidate here
  supplies without a schedule constant.

---

## 5. Recommendation

**Floor estimator (recommended, as the replacement for ℓ̂ + b̂₀⁺ and the anchor):**

```text
   Fᵢ  =  median{ T̂min,k : the K = 4 windows of leg i with the lowest nₖ so far (ties: most recent) }  +  ½·max(0, maxⱼ Fⱼ − Fᵢ)
```

- **Provenance:**
  - T̂min,k is measured (the gauge's window minimum);
  - the key nₖ is the law's own state;
  - K ∈ [3, 11] is derived (transient rejection; the queue-free-span inequality), with measured insensitivity inside;
    K = 4 is provisional pending s60's sweep;
  - n₀ = 10 = `STORE_BOOT_DERIVED` is cited (RFC 6928) and derived in the engine;
  - no hold, no m̂, no b̂₀⁺, no time index.
- **Shape:** order 0 in N; ms; continuous in δ (δ is not an input); no ρ; one expression.
- **Optional, removing ℓ̂ entirely:** q(δ)·F̂ᵢ in place of q(δ)·ℓ̂ᵢ. That is the same sentence ("a fraction of the
  queue-free loop") on the identifiable quantity. It raises the allowance by q·(F̂ − ℓ̂) ≈ +0.2–0.3 ms at c2 and about
  +0.03 ms at c1s. Not evaluated.

**The law: none shippable yet.** With honest floors, the c2 landing has no demonstrated margin:
- static map 148–164 against knees 169 / 181;
- replay 130–160.

The old form's c2 margin came from estimator bias. R3.1's c2 decomposition even names it: q₉₈ = σ + "the b̂₀⁺
over-read".

The knee needs this much window-minimum rise above the W 64 reading of 9.45 ms:
- about 3.5 ms at knee 169 (T̂min ≈ 12.95, interpolated on p27);
- about 4.0 ms at knee 181 (T̂min ≈ 13.47).

σ + q·ℓ̂ supplies 2.3–2.6 ms, so the shortfall is **0.9–1.2 ms (169) or 1.4–1.7 ms (181)**. It grows if the true limit
is below 9.45 (BELOW = slope). The shortfall is the partial-load queue of §1.2.
- It is an **allowance-form** item, not a floor item.
- It must not be "fixed" by biasing the floor (that is the defect this derivation removes), nor by a capacity input
  (E04).

**What decides:**
1. **Static pools below the smallest measured pool:** c2 W 16 / 32 / 48, c8 W 32 / 64 / 96, c1s W 64 / 128 / 192.
   They decide flat against slope, i.e. where C reads the floor at n₀ = 10, and so the landing.
2. **The allowance item:** derive whether the partial-load queue at the knee utilisation has an engine-side measure.
   σ is the within-window spread; the knee queue is the rise of the window minimum.
3. **One closed-loop row per cell with the line on** (a build; not this branch), to replace the replay.

---

## 6. What the §60 session's rows can and cannot settle

s60 runs W 96 / 128 calibration rows (plus the arms) at c2 / c8 / c1s, with `[SLAW]` printing each window's `tmin`,
`m`, `b0k` and `l`.

**It can settle:**
- **C's K inside [3, 11].** Re-read E1/E2's bias and row sd against each row's late T̂min on 6 replicates per pool,
  instead of p27's 2, offline from the printed `tmin`.
- **§2.2's mechanism on more rows:** where b0p is attained against where `m` last drops. Also b0p(K) for K = 1…8 from
  `b0k`. This is s60flip's input, measured rather than predicted.
- **The partial-load rise between W 96 and 128 at c2/c8/c1s:** the absolute late `tmin` per pool.
- **P1/P3 (latch, `l` rising):** these confirm §1.1 on static pools. They are not needed for the recommendation.
- **The c2 anchor-flip readout:** reads the K = 8 line at static pools.

**It cannot settle:**
- **Anything below W 96.** That is exactly where C reads its floor from n₀ = 10, the flat-against-slope question of
  §4.3.
- **The closed-loop ratchet (§1.3) and the anc = 1 attractor (§2.1).** These are closed-loop properties, and static
  pools never revisit n. In particular, s60's "anc 0 at W 128" on static rows does not contradict §2.1: no queue
  stands there for m̂ to ride.
- **C against B:** they are identical without revisits.
- **c2's allowance margin under honest floors:** the knee bracket 128–192 is too coarse, and 160 / 176 are its only
  interior arms. It can sharpen the sign, not the cause.

## 7. Open items

1. The sub-floor map (c2 < 64, c8 < 128, c1s < 256) decides C's floor at n₀ = 10, and therefore the landing.
2. The allowance form at c2: the partial-load queue (0.9–1.7 ms short of the knee under honest floors, depending on
   which knee) is not covered by σ + q·ℓ̂. This is a new form item, outside this derivation's floors question.
3. Route following: C refreshes only through collapse to the floor. A drain-based floor needs a derived schedule, or
   an explicit domain condition plus gauge (as ℓ̂ has today).
4. c8 under C is unexamined. Leg 1's low-n windows are near-empty (100–120 samples/s at W 128; E2 K 3 reads −1.04 ±
   1.95 ms on two rows), and F₁ feeds leg 0's coupling. C needs a per-window sample-count eligibility rule, derived
   and not declared, and a replay with the coupling.
5. K = 4 within [3, 11] awaits s60's per-window `tmin` (§6).
6. The replay omits c8 (the coupling) and models m̂ as the minimum of window minima over 40 windows.
7. The anchor double-count stays open per K and n₀ (§2.1). Only its size where it binds is established.
