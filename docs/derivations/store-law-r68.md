# store-law r68: the ledger verdicts on §60's two defect findings (the allowance σ + q·ℓ̂, and ℓ̂)

Status: DERIVATION. Docs and Python only; no engine code, no cargo, no VM. Branch `r67-verdicts` off main
`1cd13f5c`. It answers the two items the §60 Result (status §60) names under "Defect findings needing a law-ledger
verdict", and R6.7's open item 2 (`store-law-r67.md` §7).

**Answer in one paragraph.**
- **Finding 1, the allowance σ + q(δ)·ℓ̂: new ledger row F07, `UNDERIVED, DISHONEST-INPUT`.**
  - At c2 the knee needs the window minimum 4.05 ms above the converged floor. The allowance supplies 2.34 ms:
    σ 1.79 plus q·ℓ̂ 0.56.
  - No term of the allowance has the knee's partial-load queue as its sentence. σ's quantile pair (P90 − P50) is the
    one the engine happened to compute (R3.1), not one derived from the release-jitter sentence.
  - In R3.1 the gap was filled by the b̂₀⁺ over-read, which R6.7 removed.
- **Finding 2, ℓ̂ = `WinSampler::life_min`: new ledger row E13, `DISHONEST-INPUT`.**
  - The law's sentence wants "the queue-free loop". ℓ̂ reads the lowest single echo of the row so far, which is two
    different things on the §60 rows:
    - **where the pool stays below the knee**, a sample-count extreme of the jitter tail. It is still falling after
      15 s (by 0.37–1.1 ms), is set in boot on only 2 of 36 floor-pool rows, and sits 3.1 ms below the floor
      F_c at c2;
    - **where the pool queues within its first window**, the minimum of the first ≤ 2 100 samples, i.e. start
      history (c2 W 256 and W 2 048: 12 of 12 rows set in window 1).
  - P3's "rise" is the step between these two regimes. It is not a property of the path at the operating point.
- **Candidates.** Each is one expression continuous in δ, with no mode bit.
  - The finding 2 fix, **q·F̂ in place of q·ℓ̂**, is ready to design now. It moves D by only 0.31 ms (+1.69 → +1.38),
    so it does not close finding 1.
  - For finding 1, only one candidate passes every gate on §60's data with a measured magnitude: **S_F + q(δ)·F̂**,
    where S_F is the floor windows' median − minimum, read on the same windows as F̂. It lands 27–33 slots above
    the c2 knee across the whole dial. Its sentence ("the knee's window minimum sits at the no-load median") is a
    hypothesis tested at one jitter-to-loop ratio.
  - A(c)-type and wider-q forms reach the knee only through a constant fitted at c2 Bulk (c_B = 2.26 ms; q′ = 0.26
    on F̂, outside RFC 8289's band). Neither has provenance: those are owner calls (U2).
  - The at-load spread (window P90 − P10) closes c2, but it fails the R-sign gate at the c8 and c1s controls (R̃
    up to 1.11 and 1.59). It funds its own queue.
- **Measurement: yes, one, before the allowance's engine form is designed.** Unless the owner declares a δ-scaled
  constant (U2), which needs none.
  - Every jittered leg §60 brought to its knee has c2's shape (c2; c8 leg 0, whose knee the coupling sets anyway).
  - So S_F's sentence, c_B and q′ cannot be told apart from the data in hand, and they differ by 1–6 ms at a
    c3-shaped leg.
  - The minimal measurement: **one c3 single-leg static sweep** (floor pools, a knee bracket, control; the D reading
    only), on the registered gauge binary `e410d640`, with no build.
  - Finding 2 needs no measurement.
- **U2.** On §60, δ's whole authority over the c2 landing is q's band times ℓ̂: 0.28 ms, ≈ 5 slots (8 with F̂).
  The shortfall is 1.69 ms ≈ 31 slots. So the q band cannot be both the δ authority and the knee's cover. The owner
  picks one of:
  - **(a)** a δ-free spread term carries the knee, and δ prices a ≈ 6-slot margin above it;
  - **(b)** δ scales a knee-sized term, so Realtime lands below the knee by design (U3's cost);
  - **(c)** the q band is re-declared, about 2.6× wider on F̂, losing RFC 8289's provenance.

**Advisor (Fable) log: what each call changed.**
1. **Before any writing.**
   - Reproduce the score first, from the committed tgz in place. Split the 4 ms rise by window quantiles before
     proposing anything.
   - Put each candidate through R6.7's screens, R6.2 uniqueness, a provenance check on every constant (the quantile
     pair included: a quantile chosen because it closes D is a fit), and the R-sign gate. Check across the dial.
   - Read finding 2 by the window at which `l` is attained: start history and sample-count drift need different
     fixes.
   - Derive the measurement answer from a table of survivors.
   - Present U2; do not pick.
   - Ledger scope: F01 and E03 describe main, while ℓ̂ and the R6.2/R6.7 line are branch-only. So add branch-only
     rows, with C03 as the precedent, and check every tag against `render.py`'s set.

**Scripts** (stdlib, `python -I -X utf8`, output beside it):

| script | input | output | what it computes |
|---|---|---|---|
| `store-law/r68allow.py` | `l1-raw/s60/rep60a-diag-ld.tgz`, `rep60b-diag-ld.tgz`, read in place with `tarfile` | `r68allow.out` | §0: the s60 score reproduced. §1: window quantiles per arm and leg. §2: the c2 need against each family's supply. §3: each candidate replayed in the R6.7 form at q = 0.05 / 0.0667 / 0.10, with D per arm, crossings, crossing count and control R̃. §3c: c1s size. §4: the uniqueness gate. §5: where `l` is attained. §6: finding 2 against finding 1, and δ's authority in slots |

**Reproduction (`r68allow.out` §0).** The script reproduces the s60 score to every printed digit (`rep60a-score.txt`):
- F_c = 8.682 and ℓ̂_c = 5.579 on 5 valid W 16 rows;
- all six D_r (+1.712 … +1.689), the pooled D of +1.692, and D per arm of +1.309 / +2.069 / +3.096;
- the c8 and c1s calibrations.

It uses the same LATE window set (t ≥ 15 s at c2 and c8, ≥ 5 s at c1s, `nk` ≥ 10), the same latch rule and the same
knee interpolation. Spread quantities come from `[WSTAT]` (p10 / p50 / p90 per window and leg), which the ledger's
`t60` does not carry.

**§60's other broken readings are context only.** None of them is needed for these verdicts, and none is re-registered
here:
- c1s's collapse rule flags the control queue;
- c8 B2 is unscoreable, because leg 1 is unbound and DECISION 12 pools legs;
- the leg-1 floor-gate fit is uninformative;
- B1's margins at the floor pools.

---

## 1. Finding 1: the allowance is short at c2

### 1.1 Formula as registered, and what the measurement says

The R6.7 line (R6.2's, with C's floor; `store-law-r67.md` §5):

```text
   target T*ᵢ  =  Φᵢ + Aᵢ + Ŝᵢ ,        Aᵢ  =  σᵢ + q(δ)·ℓ̂ᵢ          (Ŝᵢ = T̂ᵢ − T̂minᵢ)
   fixed point (one leg):   T̂min(n*)  =  F + σ + q(δ)·ℓ̂
```

- **Sentence (R3.1).** The bottleneck stays busy only if its queue floor covers the spread of the report times that
  release the store's slots (σ). q(δ)·ℓ̂ is the δ-priced margin above that.
- **What §60 measures** (`r68allow.out` §2, per replicate; Bulk):

  ```text
   need  =  T̂min(W₉₅) − F_c  =  4.05 ms   (3.90–4.11 over the six replicates)
   supply  =  σ + q·ℓ̂_c  =  1.79 + 0.56  =  2.34 ms                 D = +1.69 ms, 90 % CI [+1.59, +1.84]
  ```

- **Shape against the sentence.**
  - The need grows with utilisation: the window minimum is 3.6 ms above F at u 0.93, 5.4 at u 0.99 and 11.1 at
    W 256 (`r68allow.out` §1).
  - σ falls with load (2.50 at W 16 → 1.78 at the knee).
  - ℓ̂ is a lower extreme (§2).
  - Neither term's sentence is "the partial-load queue at the knee utilisation". The allowance therefore has no
    term with that shape, which is why it can miss at a converged floor.

### 1.2 What the 4 ms rise is made of (`r68allow.out` §1)

c2 leg 0. Medians over the six replicates of each row's LATE median, in ms:

| W | u | tmin | p10 | p50 | p90 | p50 − tmin | p90 − p10 | σ (`sig`) |
|---|---|---|---|---|---|---|---|---|
| 16 (floor) | 0.11 | 8.68 | 10.69 | 13.70 | 16.21 | 5.08 | 5.54 | 2.50 |
| 48 | 0.32 | 9.04 | 11.74 | 14.59 | 16.88 | 5.51 | 5.14 | 2.23 |
| 160 | 0.93 | 12.33 | 14.87 | 16.81 | 18.84 | 4.45 | 3.99 | 1.78 |
| 176 | 0.97 | 13.10 | 15.77 | 17.81 | 19.76 | 4.70 | 4.01 | 1.77 |
| 192 | 0.99 | 14.05 | 16.85 | 19.12 | 21.09 | 5.03 | 4.21 | 1.71 |
| 256 | 1.00 | 19.75 | 22.83 | 25.40 | 27.48 | 5.52 | 4.61 | 1.74 |

- **From W 16 to the knee (W 160), the whole distribution shifts:** p90 rises +2.6 ms and p10 +4.2 ms. That is a
  partial-load queue.
- **The spread also narrows**, P90 − P10 from 5.5 to 4.0 ms. Its low end rises furthest (p10, tmin). This is the FIFO
  behaviour that `lib.sh` documents for netem with `rate` set: a packet leaves no earlier than its predecessor, so
  low jitter draws are absorbed behind high ones as the packet rate rises. c2 is 100 Mbit/s, 5 ms one way, with
  3 ms jitter in both directions.
- **The knee's window minimum sits 0.94–1.15 ms below the floor windows' median** (13.73 ms) on every replicate.
- **c1s, which has no jitter** (1 Gbit/s, 1 ms, jitter 0), has a no-load spread of 0.43 ms (p50 − tmin). Its floor
  is flat, b = 0.0009 ms/slot.

The mechanism is read from these logged quantiles. No model beyond them is used.

### 1.3 Verdict: F07 `UNDERIVED, DISHONEST-INPUT`

- **UNDERIVED (ledger §1: a symbol or constant without provenance).**
  - σ's quantile pair P90 − P50 was adopted because `qsp_us` already existed (R3.1, "measured: the engine already
    computes"). It was not derived from the release-jitter sentence, which names a spread without naming its
    quantiles.
  - The sentence that would carry the knee has no term in the expression at all.
  - R3.1's own decomposition of the c2 knee queue was "σ + the b̂₀⁺ over-read 0.4–0.9". The over-read was the
    margin, and R6.7 removed it as a defect.
  - With honest floors (§60: CONVERGED, E = 0.18 ± 0.03), the remainder is the 1.69 ms measured here.
- **DISHONEST-INPUT**, carried as a consumer of E13 because ℓ̂ is live in q·ℓ̂ (ledger §1's consumer rule). See §2.
- **Not PINNED or DEGENERATE.** The R6.7 form is not built, so it has no clamp to measure. The replay crosses
  `<160` on 6 of 6 replicates, at every δ (`r68allow.out` §3, A0).
- **Not a reason to bias the floor.** R6.7 §5 forbids "fixing" the allowance through the floor, and the candidates
  below respect that.

### 1.4 Candidates, by family, through the same gates

**The gates, in order:**
1. R6.7's screens: mode (any predicate on k, t or n), oracle (capacity), ratchet, K/n₀, and no floor bias.
2. R6.2 uniqueness: d(T̂min − A)/dn > 0 on the legs carrying the weight.
3. Provenance of every constant.
4. The numbers (`r68allow.out` §3, R6.7-form replay on the session floors, six replicates per cell):
   - W* is the crossing per replicate;
   - "control R̃" is the maximum over the controls, and must stay < 1 (the R-sign gate);
   - c8 is replayed with both legs and the half-gap coupling;
   - c1s's control R̃ is in §3c.

**c2 landing W* against the knee W₉₅ ≈ 168, as Bulk / Auto / Realtime** (q = 0.10 / 0.0667 / 0.05):

| | form Aᵢ | symbol provenance | c2 W* (Bulk / Auto / RT) | c2 D at the 160 arm, Bulk | c8 W* | control R̃ max: c2 / c8 / c1s | gates |
|---|---|---|---|---|---|---|---|
| **A0** (registered) | σ + q·ℓ̂ | σ measured, quantile pair declared; q cited; ℓ̂ dishonest (E13) | <160 / <160 / <160 | +1.26 | > 512 (coupling-set) | 0.089 / 0.69 / 0.90 | fails the knee at every δ |
| **A0f** | σ + q·F̂ | ℓ̂ replaced by F̂ (§2.4) | <160 ×3 | +0.95 | > 512 | 0.090 / 0.70 / 0.90 | finding 2 fixed alone; still short |
| **A(c)-type** | σ + c_B·q(δ)/q_B, c_B = 2.26 ms | **c_B fitted** at c2 Bulk (the need minus σ); ledger §1 counts it as UNDERIVED | 170 / <160 / <160 | −0.45 | > 512 | 0.097 / 0.70 / **1.015** | provenance fails. c1s's allowance rises ×4.7 (0.54 → 2.58 ms), and its control R̃ reaches 1.015 on one row |
| **wider q** | σ + q′(δ)·F̂, q′ = 0.26·q(δ)/q_B | **q′ fitted**; outside RFC 8289's 5–10 % (R6.3) | 170 / <160 / <160 (= A(c) at c2, since F̂ is constant there) | −0.45 | > 512 | 0.097 / 0.72 / 0.93 | provenance fails (U2) |
| **W80** (spread at load) | (P90 − P10)_window + q·F̂ | window quantiles measured; the pair is a fit | 188 / 182 / 178 | −1.19 | > 512 | 0.113 / **1.107** / **1.587** | **fails the R-sign gate** at the c8 and c1s controls. Uniqueness fails on c8 leg 1 (0/6) and leg 0 (5/6): the at-load spread includes the standing queue's fluctuation and funds it, R3.1's warning |
| **S** (floor spread, δ-free) | S_F + q(δ)·F̂, S_F = median over the floor windows of (p50 − T̂min) | S_F measured on C's own lowest-n windows (the F̂ set); q cited; the p50 is the hypothesis below | **200 / 196 / 194** (+33 / +29 / +27) | −2.30 | > 512 | 0.105 / 0.67 / 0.89 | passes every gate in hand. Constant in n, so no ratchet and uniqueness holds by construction; no oracle, no phase |
| **Sd** (floor spread, δ-scaled) | S_F·q(δ)/q_B + q(δ)·F̂ | as S; the δ scaling of a spread has no sentence | 200 / 167 / <160 | −2.30 | 483–511 at Auto / RT | 0.105 / 0.67 / 0.89 | gates pass; δ authority ≥ 40 slots at c2, with Realtime below the knee by design |

- **S's sentence**: at the knee the bottleneck's FIFO has absorbed the lower part of the no-load echo spread, so the
  window minimum must reach the no-load median.
- **Measured at c2:** T̂min(W₉₅) = p50_F − 1.0 ms on 6 of 6 replicates. So S covers the knee with ≈ 1.0 ms
  + q·F̂ to spare, which costs ≈ 1.9 ms of loop (+33 slots) at Bulk.
- **The ratio need / S_F = 0.80 is one cell's number.** c8 leg 0 shares c2's leg shape, but its knee is set by the
  coupling: every form crosses above 512 there. c1s has no knee bracket in §60.
- **S is therefore a CANDIDATE with a measured magnitude, not a recommendation.**

All the forms are one expression, δ enters only through q(δ) (continuous, monotone), and nothing branches on k, t, n,
δ or ρ.

**Uniqueness** (`r68allow.out` §4):
- **The shipped at-load σ:** T̂min − σ rises with W on c2 and c8 leg 0 on 6 of 6 replicates.
- **It does not rise on c8 leg 1 (0/6).** This is R6.2's stated exception: leg 1 carries ≤ 16 % of the weight, and
  the pooled R̃ crosses once.
- **Floor-window reads** (S, Sd, F̂) are constant in n, so they meet the condition wherever T̂min rises.

### 1.5 The W1 cross-check

W1 (`ed4e3405:…/w1-who-limits.md` §3) recorded "q's 5–10 % band is about 1.7 ms short at c2". That is
A(c) Bulk 4.98 − A(q) Bulk 3.32 ms, with τ_s = b₀ + c at G = 0.9 against the §43b oracle line.

§60's D lives in other coordinates:
- the window-minimum rise above a converged floor;
- with σ in the supply;
- in a law with no gain G.

Both see the same physical gap: c2's knee needs more partial-load queue than the q band supplies. But they are not
the same quantity. In §60's coordinates c_B is **2.26 ms**, not 2.7, and the two 1.7 ms figures agree by
coincidence of coordinates, not by identity.

---

## 2. Finding 2: ℓ̂ is not a floor at any operating point

### 2.1 Formula and sentence

```text
   ℓ̂ᵢ  =  min{ echo sample of leg i : since the sampler's first sample }          (slaw.rs:100, `life_min`)
```

- Sentence (store-law §3.1, R6.3): "lifetime minimum of the echo series; measured; never expires". q(δ)·ℓ̂ is
  "a fraction of the queue-free loop".
- In R6.2 it also enters the anchor min(m̂, ℓ̂ + b̂₀⁺). Under R6.7's candidate C only the q-term remains.

### 2.2 What §60 shows (`r68allow.out` §5)

For each row, the window at which the final `l` is first printed, and the samples fed by then:

| cell / leg, pools | `l` set in boot (k ≤ 8) | where it is set | l(15 s) − l(end) | `l` (median) |
|---|---|---|---|---|
| c2, floor pools 16 / 32 / 48 | 2 / 18 | k = 28–316 (7–79 s), 10 k–218 k samples | 0.37–1.11 ms | 5.45 / 5.53 / 6.11 |
| c2, knee bracket 160 / 176 / 192 | 5 / 18 (all at k = 1) | the rest at k = 30–153 | 0.25–1.06 | 9.12 / 9.76 / 10.22 |
| c2, 256 / 2 048 | **12 / 12, all at k = 1** (t 0.2 s, 128–2 079 samples) | before the store fills | 0.000 | 10.75 / 12.67 |
| c8 leg 0, floor pools 32 / 64 / 96 | 0 / 18 | k = 11–327 | 0.40–0.79 | 5.53 / 6.11 / 6.77 |
| c8 leg 0, 384 / 512 / 1 280 | 16 / 18 (k = 1) | — | 0.000 | 11.10 / 10.49 / 10.36 |
| c8 leg 1, every pool | 0 / 48 | k = 25–345 (leg 1 never queues early) | 0–1.35 | 31.9–34.9 |
| c1s, floor pools (no jitter) | 1 / 18 | k = 9–120 | 0.000 | 2.24–2.30 |

So ℓ̂ is two different statistics, and which one a row reads is set by its history:
1. **On a leg that stays below the knee, a sample-count extreme of the jitter tail.**
   - It is still falling 15 s into the row, by 0.4–1.1 ms.
   - At c2 it sits 3.1 ms below the floor F_c (5.58 against 8.68 ms). That is near the netem bound
     2·(5 − 3) ms plus host time.
   - It keeps deepening with the transfer's length. Its row sd is 0.5–3 ms where the floor's is 0.10–0.25
     (`s60abs.out`, R6.7 §1.1).
2. **On a leg that queues within its first window, the minimum of the first ≤ 2 100 samples** (≤ 6 700 at c1s W 2 048): a start-history
   statistic, frozen for life.

**P3's +5.17 ms at c2** (W 16 → 256) is the step from regime 1 to regime 2. **c8 leg 1's FLAT** is a threshold
verdict, not zero rise: +2.06 ± 0.40 against its 3.31 ms allowance. Leg 1 stays in regime 1 at every pool (0 of 48
rows set in boot).

### 2.3 Verdict: E13 `DISHONEST-INPUT`

The sentence names the queue-free loop, a property of the path. The estimator measures one of two things, and
neither is that loop:
- the depth of a jitter tail reached after a history-dependent number of low-load samples;
- the first window's minimum.

Under a closed loop the regime is set by n₀ and the ramp, so the allowance would depend on the start. This is the
same class R6.7 found for b̂₀⁺, which the §60 Result anticipated ("ℓ̂'s source into R6.7, as registered").

It is not E03. E03 is Copa's 10 s minimum `min_rtt` on main. ℓ̂ is a separate estimator on the gauge branch
(`github/slaw-gauge` `d416d59b`, `net/slaw.rs`), hence a branch-only row.

### 2.4 Candidate: q(δ)·F̂ in place of q(δ)·ℓ̂

```text
   Aᵢ  =  σᵢ + q(δ)·F̂ᵢ          F̂ᵢ = median{ T̂min,k : the K windows of leg i with the lowest nₖ so far }   (R6.7 §5)
```

- **Provenance.** F̂ is R6.7's candidate floor: measured, with K's band derived in R6.7 §3. q is cited
  (RFC 8289).
- **Sentence and shape.**
  - The sentence is unchanged ("a fraction of the queue-free loop"), now read on the quantity that is identifiable.
  - Units are ms and the order in N is 0.
  - It is continuous and monotone in δ, with no ρ input.
- **What it fixes.**
  - Its sample count is fixed (K windows of 250 ms), so it does not drift with the transfer's length.
  - It reads the same windows as the floor, so it carries no second start history.
  - On §60, F_c converged within 0.18 ms (c2) and 0.55 ms (c8 leg 0) of its W → 0 limit.
- **This removes ℓ̂ from the law.** C already removed it from the anchor, so ℓ̂ stays a gauge only.
- **Route cost:** shared with F̂'s (R6.7 §4.3, filed under §13.1).
- **Alternative, not preferred:** ℓ̂_K, the minimum over the same K windows. It keeps the word "minimum" but adds a
  second statistic of the same set for no stated sentence.

### 2.5 Fixing finding 2 does not close finding 1 (`r68allow.out` §6)

- q·F_c − q·ℓ̂_c = 0.868 − 0.558 = **+0.31 ms**, so D goes from +1.69 to **+1.38**: still SHORT. The crossing
  stays below 160 at every δ (A0f, §1.4).
- The two findings need separate rows and separate fixes.

---

## 3. Is another VM measurement needed before an engine form can be designed?

**Finding 2: no.** The q·F̂ swap is designed above, and its effect is computed from §60's rows.

**Finding 1: yes. One measurement is needed, unless the owner declares the δ-scaled constant** (U2 branch (b) with a
declared c_B, or branch (c)). A declaration needs no measurement, only the after-build battery R6.7 §5 item 3 already
owes. The reasoning:
1. **Three forms survive on what is in hand.**
   - S (measured magnitude, one-cell sentence) passes every gate.
   - Sd and A(c) differ from it only in δ scaling, an owner question.
   - The wider q band also needs the owner.
2. **What separates S from a constant is how the need scales with the path's spread and loop.**
   - S scales with the no-load spread S_F.
   - A(c) is constant.
   - q′·F̂ scales with the loop.
   - On c2's leg shape they coincide by construction: c_B and q′ are fitted there.
3. **§60 has no second leg shape at its own knee.**
   - c8 leg 0 is c2-shaped, and its knee is set by the coupling.
   - c8 leg 1 is c3-shaped (20 Mbit/s, 20 ms, 5 ms jitter). It carries ≈ 16 % of the delivered share, is starved at
     the floor pools, and never reaches a knee of its own.
   - c1s has no jitter and no knee bracket.
4. **Predicted allowance at a c3-shaped leg,** using c8 leg 1's W 256 floor as a stand-in (F 37.4, S_F 10.0,
   σ 5.2 ms):

   | form | allowance |
   |---|---|
   | S | 10.0 + 3.7 = 13.7 ms |
   | q′·F̂ | 5.2 + 9.7 = 14.9 ms |
   | A(c) | 5.2 + 2.3 = 7.5 ms |

   They differ by 1.1–7.4 ms, against §3.1's registered half-width of 0.22–0.42 ms.

**The minimal measurement: a c3 single-leg static sweep.**
- **Plan.** The §60 harness plan shape, with D only. c3 is an existing cell (`lib.sh`: 20 mbit, 20 ms, jitter 5 ms,
  GE 2/40).
- **Arms:**
  - three floor pools at u ≈ 0.1–0.3: above the store floor 10 and below about 0.3 × the echo BDP, which is
    ≈ 1 925/s × 45.9 ms ≈ 88 slots (R6.7 §1.2's c3-shaped c8 leg 1), so for example W 12 / 16 / 24;
  - a knee bracket of three arms around the knee, placed by a two-row pilot;
  - a P1 arm;
  - the 2 048 control.
- **Replicates and bytes.** Six replicates, about 48 rows. Bytes sized for ≥ 30 s of LATE windows at 20 Mbit/s
  (≈ 100–200 MB).
- **Binary.** The registered gauge binary `e410d640`, with no build. `[WSTAT]` already prints the p10 / p50 / p90
  that S_F needs.
- **Scored reading.** Per candidate, D(A) at c3's knee, with the same 90 % bootstrap. Plus the falsifier of S's
  sentence: need / S_F at c3 against c2's 0.80.
- **Design follows the result.**
  - If S's sentence holds at both leg shapes, S has measured provenance at two jitter-to-loop ratios, and its engine
    form can be designed.
  - If it fails, the measured-spread family is refuted, and only the owner-call forms remain.
- **Not needed for this decision:** the c1s control-sign re-registration, B2's anchor rule, the moved-leg floor gate
  and B1's margins. Those stay on the §60 pending list.

---

## 4. What this means for owner question U2

**Measured δ authority within the registered form.**
- q's band moves q·ℓ̂_c from 0.28 ms (Realtime) to 0.56 ms (Bulk). That is **0.28 ms, ≈ 5 slots** at the c2 knee
  slope of 0.054 ms/slot, or 0.43 ms ≈ 8 slots with F̂.
- The shortfall is 1.69 ms ≈ **31 slots**.
- So the q band cannot both be δ's authority and cover the knee. §60 is W1's U2 measured in the R6.7 form.

**The three branches, each one expression continuous in δ** (c2 W* from §1.4):

| branch | form | c2 W*, Bulk / Auto / Realtime | δ's lever at c2 | what it costs |
|---|---|---|---|---|
| (a) δ-free spread carries the knee | S_F + q(δ)·F̂ | 200 / 196 / 194 | ≈ 6 slots of margin above the knee | ≈ +1.9 ms of loop at Bulk. Needs §3's measurement for S's sentence |
| (b) δ scales a knee-sized term (A(c)-type) | S_F·q(δ)/q_B + q(δ)·F̂, or σ + c_B·q(δ)/q_B | 200 / 167 / <160, or 170 / <160 / <160 | ≈ 40 slots; Realtime lands below the knee | Realtime goodput is the price by design (U3). The magnitude needs a source: S_F (measured) or c_B (none) |
| (c) re-declare q's band | σ + q′(δ)·F̂, q′ = 0.26 at Bulk | 170 / <160 / <160 | the band widened ≈ 2.6× (on F̂) or 4× (on ℓ̂) | loses RFC 8289's provenance (R6.3). Scales with the loop, not the jitter: ≈ 9.7 ms at a c3 leg |

Branches (b) and (c) need an owner declaration. Branch (a) needs the §3 measurement. This document picks none.

---

## 5. Ledger rows added (`law-ledger.yaml`, rendered)

**Two branch-only rows, with C03 as the precedent.** F01 and E03 describe main's shipped cap and Copa's `min_rtt`,
so their verdicts stand. F01 gains pointers to the two new rows and ships with them.
- **E13, the lifetime echo minimum ℓ̂ (gauge branch):** `DISHONEST-INPUT`, P0, rank 4. Fix: delete it from the
  law (q·F̂, §2.4).
- **F07, the store-law candidate allowance:** `UNDERIVED, DISHONEST-INPUT`, P0, rank 5, depends on E13. Fix: an
  owner call (U2) or the §3 measurement, then a law change needing a battery.

Ranks from 4 on move down by two, and render.py re-checks the file.

## 6. Open items

1. **S's sentence:** is the knee's window minimum the no-load median less a constant fraction of the spread at a
   second leg shape? §3's measurement decides it.
2. **U2's branch:** an owner call. W1's U3 (the Realtime cost) attaches to branch (b).
3. **σ's own quantile pair** stays declared under every branch except (a), which drops σ.
   - Under (b) and (c), a derivation of the release-spread quantile is still owed. A pair chosen because it closes D
     (W80 does at c2) is a fit.
   - W80 is also refuted by the R-sign gate.
4. **The c2 knee need is mostly a property of netem's FIFO with jitter** (§1.2).
   - The law should fund the path's measured spread, not the emulator's constant. That argues for a spread-scaled
     term.
   - A real aggregation link may behave differently. That is a domain note for the paper, not a measurement owed
     here.
