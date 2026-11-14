# store-law: the sender store size as a time law (F01 + E03 + E04 + C02)

Status: DERIVATION. No engine change and no VM contact. Base: `github/main` `fd81f408` (2026-11-14), branch
`store-law-derive`. It goes to review before any code.

Scope: law ledger §4 items 1–2, rows F01, E03, E04 and C02. The owner paused the lever-B guard on 2026-11-14 to
derive this law first.

Builds on:
- W1 (`ed4e3405:raptorpath/docs/derivations/w1-who-limits.md`): candidate A, the τ windows and U1–U3;
- W3 (`9e03d84a:…/w3-per-leg-state.md`): the per-leg state and the (1 + r) unit mismatch;
- `p0-dwell-diag.md` §2 and §8;
- status §23, §24, §34, §42, §43/§43b, §44 and its erratum, §54 (both Results) and §55.

Where this text departs from one of them, it says so at that point.

Helper scripts (stdlib Python, run with `-I`; each output is committed beside its script):

| script | input | output | what it computes |
|---|---|---|---|
| `store-law/little54b.py` | `s54/rep54b-diag-ld.tgz`, extracted | `little54b.out` | per-leg Little's law, holes and `[HOLD]`/`[SUCC]` on the §54b block |
| `store-law/map27.py` | `loop-diag/p27/diag-ld`, `loop-diag/p34/run/diag-ld` | `map27-p27.out`, `map27-p34.out` | the law and its rejected forms on the static-cap maps |
| `store-law/b0est.py` | the same two ledgers | `b0est.out` | the test of a b₀ estimator |
| `store-law/b0bound.py` | the same two ledgers | `b0bound.out` | one-sided b₀ bounds against each row's no-queue brake |
| `store-law/wire48b.py` | `bbr48b/c2c8-diag-qcc.tgz`, extracted | `wire48b.out` | the wire-clock anchor candidates (`bbr_rs` m̂, quinn `rtt.min()`) |

## 0. Answer

*Revision 2 (after review).* Three blockers are closed in this revision:
- b₀ is now a one-sided upper bound (§3.5);
- the anchor's rise is bounded, with a gauge (§3.3);
- the leg target is coupled by the in-order argument, settled on the map (§5.2).

O1 is no longer an owner option: it is the formula.

**The law needs no rate.** Fed an honest delivered rate, the shipped law is already a pure time law,
cap' = cap · T*/T̂. Its collapse at c2 is a time-target defect, T* < T₀, not a rate defect (§2). The derived
law reads only times, plus dimensionless per-leg weights. One update per 250 ms window k:

```text
   n_{k+1}  =  clamp( n_k · Σᵢ wᵢ·( Fᵢ + q(δ)·ℓ̂ᵢ + Ŝᵢ ) / Σᵢ wᵢ·T̂ᵢ ,  floor ,  N·knee )

   Fᵢ    =  max( m̃ᵢ + b̂₀ᵢ⁺ ,  maxⱼ ( ℓ̂ⱼ + b̂₀ⱼ⁺ ) )         target queue-free floor of leg i
   m̃ᵢ    =  min( m̂ᵢ ,  ℓ̂ᵢ + b̂₀ᵢ⁺ )                          10 s echo anchor, rise-bounded
   b̂₀ᵢ⁺  =  max over the first K = 8 windows of ( T̂minᵢ,ₖ − m̂ᵢ,ₖ )   one-sided upper bound of the no-queue brake
```

> *Sentence:* once per window, scale the store by the ratio of target loop to measured loop, averaged over the
> legs by delivered share. A leg's target loop is the later of its own queue-free floor and the slowest leg's
> lifetime floor, plus CoDel's setpoint q(δ) on its own lifetime floor, plus its spread. In-order delivery waits
> for the slowest leg, so a faster leg's queue up to that floor costs no in-order delay.

**At N = 1 it reduces to the per-leg law**, since ℓ̂ ≤ m̃. Its fixed point is a time condition: the standing
queue floor equals q·ℓ plus the bound's over-read (b̂₀⁺ − b₀). Little then gives the store as
n* = Σλᵢ·T*ᵢ + H. The holes are funded implicitly (§3.4).

**The three asymmetric inputs, each closed by a one-sided form:**
- **b₀.** An under-read by more than q·ℓ collapses the store; an over-read only adds that many ms of queue
  (§3.5).
  - The unbiased boot median collapses 6/6 c2 rows, 4/4 c8 leg-0 rows and 3/4 c8 leg-1 rows on the static
    maps.
  - The boot sample maximum b̂₀⁺ collapses none of the no-queue rows on either ledger, apart from one row that
    was not queue-free: a known quinn-window-collapse outlier.
  - Its median cost is 0.41 ms (c2), 1.0 ms (c8 leg 0), 3.7 ms (c8 leg 1) and 0.1–0.5 ms (c1).
  - The total allowance above b₀ at c2 is ≈ 0.8–1.4 ms, against the ≈ 2.7 ms of §43b/W1, the thin allowances
    measured as working.
- **The anchor.** The anchor can rise above the lifetime floor by at most b̂₀⁺, so it cannot drift (§3.3).
  - Unbounded, the drift would reach 16–24 ms/min at c2 and c8, and 2–4 ms/min at c1d. That is enough to re-pin
    a long c1d row within minutes.
  - The wire candidates read honest values here (`bbr_rs` m̂ and quinn's `rtt.min()`), but only on the `bbr_rs`
    substrate. They remain a later replacement.
- **The slow leg's target.** A per-leg target leaves an idle leg at its own target, whatever the weights.
  - Equal weights land at 164–220 against 148–212 with delivered share.
  - Only the coupled F moves c8, to ≈ 545–553 on both legs.
  - Every form has one attractor on the static map, and every start-high returns (§5.2).

**Predictions are static-map crossings. They are NOT registrable: §43 shows static reasoning fails in closed
loop** (c1s predicted to converge, one runaway).

| cell | point, Realtime → Bulk (fresh) | goodput on the static map | guard consequence |
|---|---|---|---|
| c2 | 174 → 192 | 0.92 → 0.984 | inside the 1 % c2 resolution at Bulk only |
| c1s | 370 → 509 | 0.79 → 0.84 | **−10 to −20 %: fails the guard** (c1s goodput d80 5 %) |
| c1d | 518 → 622 | 0.87 → 0.89 | **fails the guard** (d80 8 %) unless main is itself below Gmax |
| c8 | ≈ 545–553, both legs | ≈ 0.95 on 8 s rows | likely −3 to −5 %: **fails** at the c8 d80 of 2 % unless the 600 MB maps (§13) show less |

The Bulk costs are the δ trade (U3). As written, the law would not pass guard v2 at c1s, c1d or probably c8.

**§54b does not bracket the knee.**
- The 640 arms sit at 6.8 × BDP, and leg 1 is already on its shoulder there (L r₁ 1 974–2 009 at W = 4 096
  against 1 866–1 946 at 1 280).
- c8 has a leg-1 shoulder near W ≈ 500–700 and a leg-0 shoulder near 180–250.
- **§13 is a stand-alone, reported-only registration** of the pool sweep that measures both. It needs no build
  and does not depend on the blockers.

**hn_n** is (reorder holes) × (visibility through the 20-run horizon), explained in both directions. Its p0 excess
on a FIFO leg is open, and it touches launch 1's guard witness (§6).

## 1. Little's law on §54b, per leg (check 1)

LATE window: sending ticks at t ≥ 15 s. The inputs per row:
- rᵢ = Δ`dgqᵢ tx`/Δt;
- rttᵢ = app-echo EWMA, `wrtt` = quinn;
- T_W = W/Σr is the store's residence time if the whole store were the queue;
- T̄ = Σrᵢ·rttᵢ/Σrᵢ is the rate-weighted mean echo loop;
- resid = `win` − Σrᵢ·rttᵢ.

Medians of per-row values, with [row ranges] where they matter. Source: `little54b.out`.

| arm | W | r₀ /s | r₁ /s | rtt₀ / wrtt₀ ms | rtt₁ / wrtt₁ ms | q₀ | **T_W** ms | **T̄** ms | resid (old holes) |
|---|---|---|---|---|---|---|---|---|---|
| B2048 | 4 096 | 9 811 | 1 938 [1 897–1 979] | 198 / 118 | 235 / 222 | 873 | 349 | 205 | 1 601 (1 289) |
| B1024 | 2 048 | 9 828 | 1 923 [1 862–1 995] | 152 / 132 | 224 / 191 | 153 | 174 | 162 | 118 (0) |
| B640 | 1 280 | 9 811 | 1 882 [1 839–1 913] | 96 / 91 | 146 / 133 | 18 | 109 | 104 | 48 (0) |
| L2048 | 4 096 | 9 845 | 1 982 [1 974–2 009] | 354 / 112 | 294 / 284 | 2 374 | 346 | 343 | 35 (0) |
| L1024 | 2 048 | 9 802 | 1 989 [1 922–2 006] | 162 / 126 | 217 / 197 | 343 | 174 | 171 | 17 (0) |
| L640 | 1 280 | 9 826 | 1 916 [1 866–1 946] | 98 / 92 | 146 / 130 | 22 | 109 | 107 | 19 (0) |

**It holds, per leg.**
- On the L arms (no old holes), T_W and T̄ agree within 1–3 % (resid/W 0.3–3.6 %).
- On B2048 the residual is the hole population (old holes 1 289 ≤ resid 1 601 ≤ all open holes 2 023).
- The brief's comparison with p0's RTT alone (354 / 162 / 98 against 350 / 175 / 109) is the 83 %-weighted
  approximation. Leg 1's loop (294 / 217 / 146 ms) is the rest.

**Units.**
- `dgq tx` counts wire datagrams: source, retransmits (≈ 14–16 k per run, ≈ 2.8 % of tx) and coded symbols
  (≈ 0.4 %).
- The counted store holds source seqs, and a retransmit re-occupies its seq's slot.
- So Σr reads the store's unit flow high by ≈ 3 %, which is the size of the L residuals. The law below never
  imports a rate, so this mismatch, and W3's (1 + r), do not reach it (§3.2).

**Consequences.**
- At every arm both legs run at or near link rate: Σr = 11.63–11.85 k/s. The store is the queue:
  **W − H = Σᵢ λᵢ·Tᵢ, and T inflates until it fills W.**
- T_W = W/Σλ is linear in W at a fixed rate: 349 → 174 → 109 ms.
- The extra population sits wherever the binding window lets it:
  - quinn's p0 buffer when W > qcwnd₀ (L2048: q₀ 2 374);
  - netem's queue when W < qcwnd₀ (B640: q₀ 18, wrtt₀ 91 ms on a 10 ms leg).
- In-order delay tracks the loop: late mean ÷ T̄ is 2.7–3.4 on B640 and the three L arms (B1024 5.2; B2048 27,
  with old holes). It is a loop for the frontier plus about two repair loops, because a hole opens every
  ≈ 3 ms at 3 % loss and takes ≈ 1.5 loops to close (`[SUCC] rep_p50` 147–295 ms).

**Shape of the law, from the identity.** The size the store needs is
n* = Σᵢ λᵢ·T*ᵢ + H(horizon, T*), and it is wrong whenever T* is wrong.

The question is the target time T*, not the store count. A fixed constant (640, or any other) pins T_W = W/Σλ
to whatever the cell's rate makes it: 109 ms at c8, ≈ 133 ms at c2, ≈ 14 ms at c1s. That is not a law.

## 2. Does the law need a rate? No, and why every rate candidate failed

**Derivation.** On a store-bound sender, Little gives the delivered rate as λ = (n − H)/T̄. Any law of the form
n' = λ̂·T* fed by the store's own delivery is

```text
   n'  =  (n − H) · T* / T̄  +  H·(…)          →  a ratio of two times; the rate cancels
```

Its fixed point is T̄ = T*. It grows when the leg has no queue (T̄ = T₀ < T*) and shrinks above. §23's
"circularity" is this loop, and it is harmless iff **T* > T₀**, where T₀ is the no-queue loop on the same clock.

The shipped law sets T* = (1 + q)·RTprop. RTprop is the 10 s minimum and T₀ is the loop the store holds a symbol
for: minimum + b₀ (the short-against-long minimum gap) + S (spread), plus the ACK→SACK release lag. So
T*/T₀ < 1 wherever b₀ + S > q·RTprop:

| cell | (1 + q)·RTprop | T₀ (no-queue loop) | g = T*/T₀ | measured with an honest rate |
|---|---|---|---|---|
| c2 | 1.1 × 6.5–6.9 ≈ 7.4 ms | 12–15 ms (§23); p27 at 64–128: Tp50 14.9–16.1 | 0.5–0.6 | 41 → 10 in 3 s, 7.2 Mbit/s (§23 Q2) |
| c1s | 1.1 × 2.24 ≈ 2.5 ms | ≈ 2.9 ms (§23) | ≈ 0.85 | 188 against 663 Mbit/s |

The ack-interval over-read (×8–29) lifts g·x above 1, which is why it was "load-bearing". **The law needs a time
target above T₀, not a rate.**

**Why each recorded candidate failed, against this form:**

| candidate (status) | same form? | what killed it | does this law escape it? |
|---|---|---|---|
| shipped law on the send-interval sampler (§23 Q2) | yes, with T* = (1 + q)·RTprop | T* < T₀ (table above) | conditionally: T* = m̂ + b̂₀ + Ŝ + q·ℓ̂ exceeds T₀ = m + b₀ + S iff b̂₀ > b₀ − q·ℓ̂. The margin is only q·ℓ (≈ 0.35–1.0 ms at c2), so b̂₀ is load-bearing (§3.5) |
| min-filtered loop time 1.1·D·Tmin (§24 O1, `INFLATES`) | yes, with T* = 1.1·Tmin | the allowance 0.1·Tmin < S, so the map is below 1 at every cap | yes: S is in T* (it cancels in the fixed point; only the floor error q·ℓ − (β − b₀) remains) |
| quinn's BBR rate (§24, `OVER-READ` ×3–20) | no: an exogenous rate | ack compression at c2 jitter | no rate is read |
| (b1)/(b2) quinn's window K (§34) | no: a window as the store | over-reads ×5–15 at c2/c8; Spearman(K, n) 0.90 | not read |
| (b4) receiver rate / μ_R / dispersion (§34, §36, §40) | no: rates | circular by identity / over-asks where the receiver is not the bottleneck / `NOT-YET` | not read |
| `bbr_rs` b̂ (§27, `CIRCULAR` at c2) | no: a rate | a delivered rate on a store-bound sender | not read |
| residence-time form g·L̂·(R̄ − brake) (§44 item 2, refuted) | partly: the reference is R̄ − brake = m + S | the brake undershoot (b₀ read as queue); an L̂ was still required; c1s spread 0.32–0.41 | the target adds b₀ + q·ℓ; no L̂; noise is §3.3's open item |
| oracle (1 + q)·ΣL(RTprop + S) (§36, §43) | yes, with an oracle L | an oracle L; a second crossing near 927 at c2 with an expired anchor (§42); q multiplies the anchor | L cancels; q·ℓ is not scaled by the anchor, so the expired c2 map has no upper crossing (§4.1) |
| G·ΣL(RTprop + S + τ), G 0.9, τ 5 ms (§43b, passed at c2) | yes, with an oracle L | G and τ unprovenanced; c1s τ window unsettled | G = 1 (derived); τ = b₀ + q(δ)·ℓ (cited q; b₀ blocking); see §3.3 for when G < 1 returns |
| W1 A(q): τ = b₀ + q·RTprop at G = 0.9 | yes | ≈ 8.5 % short at c2 | at G = 1 with q on ℓ, c2 lands on the knee, 192 at Bulk (§4.1). **This departs from W1, which held G < 1 required; see §3.3** |
| W1 A(c): τ = b₀ + c_B·q/0.1 | yes | c_B ≈ 2.7 ms has no source | no c_B |
| this derivation's first draft: max over legs of (m + b₀ + S) | yes, coupled | at c8, S₁ rises 8.6 → 118 ms with n; ask/n 1.9–2.9, crossing near 1 850 (`map27` history). On §54b the shipped anchors order the legs wrongly (`rtp0` medians 43–54 ms > `rtp1` 38 ms at W = 1 280) | the max structure is kept on non-inflating inputs (ℓ̂ + b̂₀⁺): the coupled F (§5.2) |

## 3. The law

### 3.1 Formula and provenance

```text
   n_{k+1}  =  clamp( n_k · R_k ,  floor ,  N·knee )

   R_k   =  Σᵢ wᵢ,ₖ · ( Fᵢ + q(δ)·ℓ̂ᵢ + Ŝᵢ,ₖ )  /  Σᵢ wᵢ,ₖ · T̂ᵢ,ₖ           Ŝᵢ,ₖ = T̂ᵢ,ₖ − T̂minᵢ,ₖ
   Fᵢ    =  max( m̃ᵢ + b̂₀ᵢ⁺ ,  maxⱼ ( ℓ̂ⱼ + b̂₀ⱼ⁺ ) )
   m̃ᵢ    =  min( m̂ᵢ ,  ℓ̂ᵢ + b̂₀ᵢ⁺ )
   b̂₀ᵢ⁺  =  max_{k ≤ K} ( T̂minᵢ,ₖ − m̂ᵢ,ₖ ) ,  K = 8 boot windows (n ≤ n₀ = 128)
```

| symbol | meaning | provenance |
|---|---|---|
| T̂ᵢ,ₖ, T̂minᵢ,ₖ | median and minimum of leg i's app-echo loop samples in window k | measured. The echo is the loop the store holds a symbol for; the key is the placement stamp (§55 Erratum 1). Sampler: the oracle arm's (`[OCAP]`, archived), not on main |
| wᵢ,ₖ | leg i's sample count in window k (∝ delivered rate) | measured; dimensionless. The weighting is settled on the map (§5.2): it does not decide the attractor |
| m̂ᵢ | Copa's 10 s minimum of the echo series | shipped (`copa.rs:1000-1008`). It is E03's dishonest input; it enters only through m̃ |
| ℓ̂ᵢ | lifetime minimum of the echo series | measured; never expires. Under-read only on a route increase (§3.3) |
| m̃ᵢ | the anchor, its rise above ℓ̂ bounded by b̂₀⁺ | derived: at no queue the 10 s minimum exceeds the lifetime minimum only by the brake's spread, which b̂₀⁺ bounds. Any larger rise is own queue (E03) or a route change. Gauge: `anc=` bind fraction (§8) |
| b̂₀ᵢ⁺ | one-sided upper bound of the no-queue brake: the sample maximum over the first K windows | derived as one-sided (§3.5). K = 8 (2 s) is **declared**: the calibration length at n ≤ n₀. The bound's width (b̂₀⁺ − b₀) is the standing queue it adds; measured median 0.1–3.7 ms by cell (§3.5) |
| Fᵢ | target queue-free floor: the later of leg i's own floor and the slowest leg's lifetime floor | derived from ρ = 1 in-order delivery (§5.2). It is a max of continuous values and reads no dial. At N = 1, Fᵢ = m̃ᵢ + b̂₀ᵢ⁺ |
| q(δ) | CoDel setpoint (b + 1)/30 ∈ [0.05, 0.10] | cited, RFC 8289 §3.2; shipped `codel_setpoint_q`. **The only δ seat** |
| window T_w | 250 ms, tumbling | **declared**, as paper §6.1's oracle arm: inside its derived range only near Bulk, with the `wbind=` gauge. Every number here is a 250 ms statistic. The scale-free Tp50/q(δ) is not adopted (Q8): it depends on n, puts q in a second place, and is ambiguous at N = 2 |
| n₀ (start) | 128 | shipped `RWM_STORE_BOOT`. "A fit to one cell" (§11.2); no provenance (Q7) |
| empty window | a leg with no samples drops out of both sums; with no samples on any leg, R := 1 | a statistic of an empty set does not exist (§6.1's oracle boot rule) |
| floor | 10 | resource bound (shipped) |
| N·knee | 2 048 per live path | resource bound (shipped). Bind gauge `[SUMCAP] pin=` |

**No rate and no gain constant.** G = 1 is the update that makes T̄ = T* the fixed point. The declared constants
are K = 8, T_w = 250 ms and n₀ = 128, each with its open question.

### 3.2 Shape check (FORMULA-FIRST)

The sentence and the expression agree in shape:

- **Order in N.**
  - R is a weighted mean ratio, order 0 in N; symmetric legs give the same R at every N.
  - The fixed point n* = Σᵢ λᵢ·T*ᵢ + H is a sum over legs, order 1.
  - The clamp is linear in N. At N = 1 the sums have one term, and no path-count branch exists.
  - Compare the shipped predecessor's N²·a (paper §6.1, "The count multiplier").
- **Units.**
  - R is time ÷ time.
  - n stays in the store's own unit (source seqs) by construction.
  - There is no wire-against-source (1 + r) factor (W3 §2 item 8), because no rate is imported.
- **Monotonicity in δ.** R and n* are strictly increasing in q, and q is continuous and strictly monotone in
  b(δ) between the Realtime and Bulk anchors.
- **ρ.** No ρ input. Holes are funded implicitly (§3.4), whatever their residence, so §6.1's
  ε̂·bw·stall(δ, ρ) term is not needed. The store gate still requires `reliable` (F02); extending the gate to
  ρ < 1 is F02's question, and this formula needs no change for it.
- **NO-MODE-SWITCH.** One expression. δ enters only through q. There is no hint branch, no δ/ρ threshold, and no
  path-count branch. The clamp's bounds are resource bounds, carried with bind gauges.
- **Departure from the shipped sentence** ("BDP plus one recovery round of runway", the stale doc comment):
  - the new sentence is about time (target loop ÷ measured loop);
  - the shipped expression's (1 + q)·Σbw·RTprop is its special case λ̂·T* with T* = (1 + q)·RTprop.

### 3.3 Dynamics, noise and the anchor

**Saturated leg.** The brake rises one-for-one with the queue: β = Q/μ + b₀, and T̂ ≈ n/μ. So
n·R ≈ n − (Q − Q*)·n/(Σλ·T̂). The loop gain is n/(Σλ·T̂), which reads 1.03–1.11 at c2 and 1.12–1.33 at c1 on
the static maps (`map27` column "Little"). That gives one-window convergence with at most a 0.3 overshoot. It is
stable while the gain is < 2.

**Unsaturated leg.** Here β ≈ b₀ and R ≈ 1 + q·ℓ/T₀ ≈ 1.06 per window at Bulk, at every cell (the ratio is
scale-free). At T_w = 250 ms the climb from n₀ = 128 to n* takes:
- c2: ≈ 7 windows ≈ 1.7 s;
- c1s: ≈ 24 windows ≈ 6 s.

**This ramp is a stated cost.** Most c1s guard rows are shorter than 6 s (14 `[DIAG]` ticks), so they would run
in start-up. A faster ramp needs either a shorter window or a different n₀; both are open (Q7, Q8).

**Noise.** At gain ≈ 1, sd(ln n) ≈ the per-window coefficient of variation of (β − b₀)/T̂. §42 measured the
oracle form at T_w = 250 ms: 0.08 at c2, 0.11 at c1s. **Open:** the noise of this R (§10 Part B).

**The anchor: E03 is not fixed by the law's fixed point alone. It is bounded.**
- *What the shipped anchor reads* (§54b, `little54b.out`):
  - leg 1 at W ≤ 2 048: `rtp1` 35–46 ms against a boot value of 33–37;
  - leg 0: `rtp0` 34–60 ms at W = 1 280 and 237–319 ms at L2048, on a 10 ms leg.
  - That is E03: a minimum of a dwell-inclusive clock whose queue never drains.
- *Unbounded drift at the law's fixed point.* The queue floor target is (b̂₀⁺ − b₀) + q·ℓ. If the floor's
  fluctuation f within 10 s is below that, m̂ climbs by up to (b̂₀⁺ − b₀ + b₀ + q·ℓ − f) per filter period, and the
  target follows it. With f = 0 that is:

  | cell | per 10 s | per minute | what a long block shows |
  |---|---|---|---|
  | c2 | 2.6–3.3 ms | 16–20 ms/min | the loop doubles (T₀ ≈ 17 ms) in ≈ 1 min |
  | c8 leg 0 | 3.3–4.0 ms | 20–24 ms/min | the same in ≈ 40 s; F is unaffected (it is set by ℓ̂₁) |
  | c1d | 0.4–0.65 ms | 2.2–3.9 ms/min | T₀ ≈ 5–7 ms, so n ∝ T grows ×1.4–1.7 per minute and reaches the 4 096 clamp in ≈ 3–5 min |

  On a §53-length c1d block (5 GB rows, ≈ 60 s each) that is +2–4 ms of `rtp_us` and +40–70 % of the cap per
  row, re-pinning the longest rows. Linear is not bounded, so the formula bounds it.
- **The mechanism (closes the anchor question): m̃ = min(m̂, ℓ̂ + b̂₀⁺).**
  - The anchor cannot rise more than b̂₀⁺ above the lifetime floor.
  - The total queue floor is then at most 2·b̂₀⁺ − b₀ + q·ℓ, with zero drift after that, whatever f is.
  - On §54b it would read ℓ̂₀ + b̂₀⁺ ≈ 8.4–14.2 + 1–3 ms on leg 0, where `rtp0` reads 34–319.
  - The price is that a route increase Δm is under-read by Δm − b̂₀⁺, and the law collapses if that exceeds
    q·ℓ. L1 cells have no route change, so this is a **domain condition** with a gauge: the `anc=` bind
    fraction (m̂ − ℓ̂ > b̂₀⁺). Persistent binding with the queue floor at target is the signature of a route
    change.
- **The wire candidates the brief names** (`wire48b.py` on status §48b's `c2c8-diag-qcc` rows, 12 per arm, LATE
  medians, [boot]):

  | cell, leg | `bbr_rs` m̂ (`qrt`) | quinn `rtt.min()` (`qmin`) | Copa echo `rtp_us` | wire `wrtt` |
  |---|---|---|---|---|
  | c2 p0 (CAND = `bbr_rs`) | 7.8 [8.0] | 6.2 [8.0] | 189.8 [11.1] | 22.0 |
  | c8 p0 (CAND) | 8.2 [7.4] | 6.9 [7.1] | 80.8 [12.1] | 21.5 |
  | c8 p1 (CAND) | 39.6 [37.9] | 36.8 [37.8] | 121.2 [40.7] | 87.2 |

  Both read propagation while the echo anchor reads the store's queue:
  - `bbr_rs` m̂ is drained by ProbeRTT and is windowed, so it would also follow a route increase;
  - quinn's `rtt.min()` is a lifetime minimum, with the same route-change exposure as ℓ̂.

  Neither is a drop-in source, for three reasons:
  1. Both are on the wire clock. They exclude the echo's engine, quinn-buffer and report delays, so they need
     the echo-minus-wire no-queue offset. That is another boot calibration (the same structure as ℓ̂ + b̂₀⁺).
  2. The `[QCC]` gauge prints only under `bbr_rs`. Quinn's BBR (the default) exposes `rtt.min()` without a
     gauge, so no log here shows it.
  3. `bbr_rs` is not the default (K09/K10, §48b).

  **Decision:** m̃ ships with ℓ̂. `bbr_rs` m̂ plus the boot offset becomes the route-following replacement for ℓ̂
  if K10 promotes `bbr_rs`. The `anc=` gauge decides whether that is needed.

- **Departure from W1 (G < 1 required).**
  - W1's requirement came from (a) the oracle form with q·RTprop, and (b) c8's trap edge at G = 1 for a
    constant τ: τ < 3.08 ms.
  - This law's τ on leg 0 is b₀ + q·ℓ ≈ 2.85 + 0.4–1.2 ≈ 3.3–4.0 ms, above that edge at every δ.
  - The `map27` c8 expired map shows no upper crossing at G = 1, but both readings rest on the thin p27 c8 grid
    (two rows per cap, a lock-out row at 768). **c8 start-high is undecided**, and today's c8 start is the pin
    (Q1).
  - At c2 the allowance does not scale with the anchor, and the expired map is trap-free at G = 1.
  - G < 1 is incompatible with a q-sized allowance: a leg saturates only if G·(1 + q·ℓ/T₀) > 1. On every map,
    G = 0.9 gives no crossing at all (`map27-*.out`): it collapses.
  - What G = 1 cannot be shown to do from the logs is leave today's start-high state. §43's start-high hold ratios
    (1.006 / 1.059 / 1.005 at gain 1.1) transfer to R ≈ 0.98–1.03 at H = 1 536, using the c2 Little factor 1.05.
    That is marginal. **c2 does not collapse; it is not shown to leave the pin** (owner question Q1).
  - So at G = 1 neither c2 nor c8 is shown to leave today's start-high state.

### 3.4 The hole term H and the C02 coupling

**H is implicit.** Write n' = n·R(n). At the fixed point R = 1 whatever H is, and
dn'/dn = 1 + n·R'(n) = 1 − (1 − H')·n/(n − H).

H' is the share of an added slot that becomes a hole. It lowers the loop gain and moves neither the point nor the
sign of the slope.

**Measured H' per horizon** (`little54b.out`; residual ≈ holes):

| horizon (MAX_NACK_GAPS) | W 4 096 → 2 048 | W 2 048 → 1 280 | where old holes appear |
|---|---|---|---|
| 20 (B) | (1 601 − 118)/2 048 = **0.72** | (118 − 48)/768 = 0.09 | between transit 1.9 k (B1024: 0) and 2.5 k (B2048: 1 289) |
| 64 (L) | (35 − 17)/2 048 ≈ 0.01 | ≈ 0 | none up to transit 4.1 k |

**The coupling, as a condition.** Here "binds" means **old holes accumulate**, not that reports are truncated.
Reports are truncated at every arm (visibility 0.30 even at W = 1 280, §6) while H' ≈ 0.09. Old holes accumulate
when the 20 oldest runs are all long-lived holes. That happens once the open runs ε_run·c·transit exceed what
repair clears per horizon, so it sets in above a transit P_h ∝ G_h:
- P_h(20) ∈ (1.9 k, 2.5 k) at c8;
- predicted P_h(64) ≈ 3.2 × that, about 6–8 k. That is consistent with L2048, which shows no old holes at 4.1 k.

Above P_h, H' → 0.72 and the store law loses most of its authority. Its slots go to holes.

**P_h per cell, as a domain condition.** P_h scales as G_h/ε_run, where ε_run is the loss runs per symbol. From
the c8 measurement and each cell's calibrated loss and burst (`lib.sh`):

| cell | ε_run (runs per symbol) | P_h(20), estimated | law's transit* | margin |
|---|---|---|---|---|
| c8 | ≈ 4.5·10⁻³ (leg 0 2.7 %/8 + leg 1 4.1 %/4) | 1.9–2.5 k, **measured** | ≈ 550 | 3.5–4.5 × |
| c2 | ≈ 2.6·10⁻³ (2.6 %/10) | ≈ 3.3–4.3 k | ≈ 190 | ≈ 20 × |
| c1s, c1d | ≈ 2·10⁻⁴ (0.1 %/5) | ≈ 40–55 k | 370–654 | ≥ 60 × |

- **Lever B is not needed for the store's sake on these four cells only.** The condition transit* < P_h(G_h)
  fails on a cell whose loss-run rate × transit is about 5 × c8's.
- Gauges: old holes and `ripe_frac`.
- Lever B's residual at W = 1 280 (in-order 0.30 against 0.35 s, no overlap) is a recovery-latency gain, not a
  store one.

The store size and the gap horizon are one question, as the brief says. With a time law the answer is the
inequality transit* < P_h(G_h). The gauges (§8) are old holes and `[HOLD]` `ripe_frac` = `age_ripe/age_n` (p0
0.98 at B2048 against 0.13 at B640), both of which exist.

### 3.5 b₀: an asymmetric input, so a one-sided bound

**The asymmetry, from the fixed point.** At no own queue, the window minimum sits at m + b₀ (b₀ is the true
no-queue brake) and T̂ = T₀. The law's target floor is m + b̂₀ + q·ℓ. So:

```text
   no queue:    R − 1  =  ( b̂₀ − b₀ + q·ℓ ) / T₀             grows iff  b̂₀ > b₀ − q·ℓ
   fixed point: queue floor  =  b̂₀ − b₀ + q·ℓ                (ms of standing queue; the over-read adds 1:1)
```

- An **under-read** by more than q·ℓ (≈ 0.35–1.0 ms at c2, 0.12–0.26 ms at c1s) gives R < 1 at no queue. There is
  no restoring force, so the store decays to its floor: §23's collapse.
- An **over-read** by Δ costs Δ ms of standing queue and nothing else. At c2 1 ms is ≈ 10 symbols and +5 % of the
  loop.
- So b₀ needs a one-sided **upper** bound, not an unbiased estimate. The bound's width is a term of the formula:
  the fixed-point queue floor is (b̂₀⁺ − b₀) + q·ℓ.

**Candidates evaluated against each row's own no-queue brake** (`b0bound.py` → `b0bound.out`):
- Rows: the static caps at or below the BDP.
- The candidate reads only the row's first 8 windows. The law starts at n₀ = 128, at or below the BDP at all four
  cells.
- margin = candidate − b₀,row + q·ℓ at Realtime q = 0.05, the thinnest; collapse iff margin < 0.

| cell, leg (ledger) | b₀,row | median of first 8 (unbiased boot) | max of first 8 (b̂₀⁺) | max over the whole row |
|---|---|---|---|---|
| c2 p0 (p27, 6 rows) | 1.27–2.82 | **collapses 6/6** (margin −1.99) | 0/6; cost median 0.41, max 0.90 | 0/6; cost 1.74 / 2.50 |
| c8 p0 (p27, 4) | 1.87–4.29 | **4/4** (−2.26) | 0/4; cost 1.0 / 48.8 | 0/4; 1.8 / 48.8 |
| c8 p1 (p27, 4) | 3.49–4.94 | **3/4** (−0.77) | 0/4; cost 3.7 / 45.0 | 0/4; 6.3 / 45.0 |
| c1s p0 (p34, 3) | 0.09–0.22 | 0/3 (+0.09) | 0/3; cost 0.12 / 0.20 | 0/3; 0.20 / 0.21 |
| c1d p0 / p1 (p34, 3) | 0.13–0.36 | 1/3 on p0 | 0/3; cost 0.33 / 0.34 and 0.20 / 0.35 | 0/3 |
| c1s p0 (p27, 4) | 0.15–26.4 | 1/4 | 1/4 | 0/4 |

Notes on the table:
- The c1s p27 failure is `DEF192 s42 r1`, the §41b quinn-window-collapse row (brake 26 ms, not a no-queue row).
- With 8 samples, the nearest-rank 90th percentile is the maximum, so "window minimum quantile" and "sample
  maximum" coincide at K = 8.

**Reading.**
- The unbiased boot estimate collapses most c2 and c8 rows. Boot windows read a *lower* brake than the row,
  because the running anchor is still falling, so the asymmetry is not hypothetical.
- The boot sample maximum covers every no-queue row on both ledgers.
- Its cost has a heavy tail at c8 (one 45–49 ms window per leg, from windows with few samples on a starved leg).
  This is the bound's known weakness. It is visible in the `[SLAW]` b̂₀⁺ readout. A robust order statistic (the
  second-largest) would add a constant, so it is not proposed before Part B measures the tail.
- Against the measured working allowances: §43b's τ = 5 ms is b₀ + 2.7 ms at c2, and W1 A(c) has c_B ≈ 2.7 ms.
  b̂₀⁺ + q·ℓ puts c2 at b₀ + 0.8–1.4 ms (median), thinner than the measured 2.7 ms, but with zero collapses on the
  map.
- The spread estimator of revision 1 (max_k S_k − mean_k S_k, `b0est.out`) is kept only as the record of a failed
  candidate (it inflates at load).

**Part B falsifier B1 (one-sided, absolute):** on any row, b̂₀⁺ < that row's later no-queue median brake − q·ℓ̂,
or R < 1 in the boot windows.

## 4. Per-cell predictions (static maps, the inputs the law reads)

**Not registrable: §43 shows static-map reasoning fails in closed loop.** These are the maps' crossings, kept to
size the closed-loop arm (§10 Part B) and to read against §13's current-era maps. The Bulk costs at c1s (−10 to
−20 %), c1d (≈ −10 %) and c8 (≈ −3 to −5 % with the coupled F) would fail guard v2's goodput clauses. A ship
needs either U3 (accept the cost) or a larger allowance (U2).

The c2/c1 numbers below use the per-leg form with the ledger b₀. The coupled F is identical at N = 1. b̂₀⁺ adds its
cost: +0.4 ms at c2 moves n* by ≈ +4.

How these are computed:
- The fixed points are crossings of R(n) = 1 on the static-cap ledgers (`map27-*.out`, form `ratioL`, G = 1,
  H = 0; H = ε·λ·2·srtt changes no crossing by more than 2).
- Era:
  - p27 (2026-10-27, 100 MB rows, 2 rows per cap) for c2 and c8;
  - p34 (post-§32 receiver, 3 rows per cap, c1 windows truncated at 8 192 samples) for c1s and c1d.
- "Fresh" is the boot anchor. "Expired" is the row's lowest window minimum, a pessimistic expired filter.
- **None of these is a closed-loop result.** The closed-loop arm is §10.

### 4.1 c2 (the "must not collapse" test)

| q | fresh n* | expired n* | goodput at n* (p27 interpolated) | Tp50 at n* |
|---|---|---|---|---|
| 0.05 (Realtime) | 174 | 140 (and U81) | 0.92 / 0.80 | ≈ 18 ms |
| 0.0667 (Auto) | 180 | 148 (and U77) | 0.94 / 0.84 | ≈ 18.5 ms |
| 0.10 (Bulk) | **192** | 164 (and U69) | **0.984** / 0.89 | ≈ 19 ms |

Against MAIN, Tp50 is ≈ 208 ms at 600 MB (§43).

**No collapse with a fresh anchor:** R ≥ 1.01 at every cap 64–128. The expired map's lower unstable point (69–81)
sits below boot 128, and at boot the anchor is fresh.

**From today's pin** (c2 `pin` 0.80 on the guard's CAND rows): the expired map has R = 0.94 at 1 024 (Bulk), so it
descends. The §43 hold transfer reads 0.98–1.03, which is marginal (§3.3).

**Falsifiers:**
- the cap below 100 for more than 3 s after t = 2 s;
- goodput < 0.85 × main;
- the upper crossing: n not below 512 within 15 s of a start at 2 048.

### 4.2 c1s (today the clamp binds ≈ 0.7)

| q | fresh n* | expired n* | goodput at n* (p34; Gmax 832) |
|---|---|---|---|
| 0.05 | 370 | 561 | 0.79 / 0.86 |
| 0.0667 | 385 | 590 | 0.80 / 0.87 |
| 0.10 | 509 | 645 | 0.84 / 0.88 |

- **Bind fraction: 0.71 (guard CAND row) → predicted 0.** n* ≪ 2 048. Falsifier: `pin` > 0.1.
- **Cost:** at the power point, not the 0.95 knee (1 024). This is the §6.1 reading: on a receiver-bound cell
  the receiver's throughput keeps rising with queue (S 1.2 → 10.2 ms over 256 → 2 048), and a delay target
  declines the last 10–15 %. That is −10 to −20 % against a pinned main (≈ 826 Mbit/s): owner question Q3.
- **Era caveat:** p34's c1 windows are truncated (8 192 samples, start of window), with an unknown direction.

### 4.3 c1d (p34)

n* = 518 / 555 / 622 (fresh), 514 / 565 / 654 (expired). At Bulk with an expired anchor there is a touch at
1 020–1 025.

Goodput is 0.87–0.90 of Gmax 818. Today c1d is already interior (guard CAND row: `pin` 0.00, cap ≈ 1 230), so
"bind 0" tests nothing.

**Falsifiers:** a law cap outside 400–800, or goodput < 0.85 × main.

### 4.4 c8 (N = 2)

| form | fresh n* (Realtime / Auto / Bulk) | expired n* | leg 1 at n* |
|---|---|---|---|
| **coupled F (the law)** | **546 / 548 / 553** | 545 / 547 / 552 | ≈ 1 550 /s (p27 512) |
| per leg, delivered-share weights | 195 / 201 / 212 | 148 / 159 / 181 | 103–232 /s (starved) |
| per leg, equal weights | 204 / 210 / 220 | 164 / 187 / 203 | starved |

- p27 goodput at 512–768 is 0.95 of Gmax 100.9 (8 s rows). In-order delay is ≈ 3 × T̄ ≈ 3 × 47 ms.
- **Falsifiers:**
  - a law cap outside 400–750;
  - leg 1 < 1 200 /s;
  - Σ send rate < 0.95 × the W = 1 280 control.

## 5. c8: why §54b does not bracket the knee, and where it breaks (check 2)

**The 640 arms are not near the knee.** W = 1 280 is 6.8 × the base BDP: ≈ 190 symbols at 1 200 B (leg 0
125 kB, leg 1 100 kB; `place46_parse.py`). Its p0 loop of 96 ms is 9.6 × leg 0's propagation.

**What 1 280 does show** is that leg 1 is on its shoulder already:
- L r₁ is [1 974–2 009] at W = 4 096 and [1 866–1 946] at 1 280. The ranges do not overlap, and Σr does not
  overlap either (11 809–11 852 against 11 712–11 765). This is the status §54 send-rate fall.
- On B, r₁ is [1 897–1 979] against [1 839–1 913], which overlaps.

**Two shoulders.** Placement prices delivery time: E_i = (I/W)·s + (½ + ε)·wire (K05). So leg 1 gets traffic only
in proportion to how far leg 0's loop has risen toward leg 1's. Across the ledgers, leg 1's rate share against
leg 0's loop is:

| p0 loop | 34 ms | 45 | 73 | 96 | 152 | 198 | 354 |
|---|---|---|---|---|---|---|---|
| r₁ / ≈ 2 000 | 0.75 (p27 384) | 0.77 (p27 512) | 0.88 (p27 1 024) | 0.93 (B640) | 0.95 (B1024) | 0.96 (B2048) | 0.99 (L2048) |

The p27 columns predate the wire placement base (ADR-0073) and come from 8 s rows: shape only.

**The shoulders, predicted for the current era:**
- **K₁, the leg-1 shoulder:** r₁ falls below 0.8 × its W = 1 280 value once leg 0's loop is below T₀,₁ ≈ 46 ms
  (leg 1's no-queue loop, p27: Tp50 46–47 at 128–192). That is W ≈ Σμ·T₀,₁ ≈ 500–700.
- **K₂, the leg-0 shoulder:** leg 0 leaves link rate below W ≈ μ₀·(m₀ + b₀ + S₀) + H ≈ 9.8 × 18 + 10 ≈ 180–250
  (p27 c8: r₀ 9 258 at 192, 9 608 at 256).
- Between K₂ and K₁, goodput ≈ leg 0 alone. That is −13 to −17 %, at an in-order delay that keeps falling
  ∝ W.

**Do not fit to 640.** The slope §54b anchors is T̄ ≈ W/Σλ (109 ms per 1 280). The optimum is a δ choice on the
curve below.

### 5.1 The delay-against-goodput curve at c8 (shown, not picked)

| W | Σ send rate /s | T̄ (≈ W/Σλ) | in-order late mean | source |
|---|---|---|---|---|
| 4 096 | 11 826 | 343 ms | 0.93 s | L2048, measured |
| 2 048 | 11 790 | 171 ms | 0.49 s | L1024, measured |
| 1 280 | 11 745 | 107 ms | 0.30 s | L640, measured |
| 768 | 11 450–11 650 (predicted) | 60–70 ms | 0.18–0.25 s | §13 arm |
| 512 | 11 100–11 600 | 42–48 ms | 0.13–0.17 s | §13 arm (≈ the law's coupled point, 545–553) |
| 320 | 10 300–10 900 | 28–32 ms | 0.08–0.12 s | §13 arm |
| 224 | 9 700–10 100 | ≈ 22 ms | 0.06–0.08 s | §13 arm (≈ the rejected per-leg point) |
| 160 | ≤ 9 300 | ≈ 17 ms | — | §13 arm (below K₂) |

The δ dial moves the law along this curve only through q·ℓ: a few ms, ≈ 10–30 symbols. **At c8 the curve is
shaped by the placement coupling, not by δ.**

### 5.2 The leg target, settled on the map

**Why the per-leg target starves leg 1, whatever the weights.** An idle leg has a brake of ≈ b₀, so it already sits
at its own target. Its term in R is ≈ 1 + q·ℓ₁/T₁, a push of a few percent, whatever weight it gets. The store
grows; placement (K05, delivery-time price) sends the extra slots to leg 0; leg 0's brake rises past its own target;
and R → 1 at the fast-leg point.

This does not come from the weights. Equal weights land at 164–220, against 148–212 with delivered share
(`map27-p27.out`, `ratioE`). The attractor is set by the target.

**Attractors** (`map27-p27.out`, c8, G = 1, all q, fresh and expired):

| form | ask/n at 128 → 2 048 | stable crossings | start-high (2 048) |
|---|---|---|---|
| per leg, delivered share | 1.02–1.08 … 0.71–0.81 (1 024), 0.28–0.32 | one: 148–212 | returns (0.28–0.32) |
| per leg, equal weights | 1.02–1.07 … 0.79–0.87, 0.30–0.34 | one: 164–220 | returns |
| coupled F (ℓ̂-based) | 2.63–2.65 … 1.06–1.08 (512), 0.58–0.59 (768), 0.93–0.94 (1 024), 0.42–0.44 | one: 545–553 | returns |

There is no second attractor on the static map, so §43's BISTABLE is not reproduced there. The thin grid has two
rows per cap. The 768 row carries a leg-1 lock-out. 1 024 reads 0.93–0.94 under the coupled F, so a closed-loop
start-high remains Part B's readout.

**Why the max-over-legs form was discarded in revision 1, re-read.** It was discarded for its inputs, not its
structure:
- it took the maximum of m̂ⱼ + b₀ⱼ + Ŝⱼ, and Ŝ₁ inflates with load (8.6 → 118 ms);
- m̂₀ inflates under the queue the coupling itself creates (§54b `rtp0` 43–54 > `rtp1` 38).

F keeps the max, on lifetime floors ℓ̂ⱼ + b̂₀ⱼ⁺. That input neither inflates with load nor with the fast leg's own
queue.

**Why the coupling is derived, not a preference.**
- Under ρ = 1 the in-order frontier waits for the slowest leg. A faster leg's queue up to that leg's floor moves
  waiting from the reorder buffer into the fast leg's queue, and adds no in-order delay.
- The current placement law feeds the slow leg only once the fast leg's loop approaches it. A per-leg target
  therefore contradicts placement, and the store cannot fund the slow leg.
- The coupled F is continuous (a max of values), reads no dial, and reduces exactly at N = 1.
- What remains a preference is narrower (Q2): a consumer of first arrivals (ρ < 1, Realtime) pays ≈ +30 ms of
  fast-leg first-arrival delay for it. That is F02/ρ's question, not the ρ = 1 store's.

## 6. hn_n: 168 → 11 994 → 55 038 (check 4)

`[HOLD] hn_n` counts holes that a gap report named (`on_reported`, stamped only inside the report's 20 oldest
runs, and only before the C09 repair cap breaks the loop, `sender_phases.rs:806`) and that a later report showed
filled, with no retransmit ever emitted. From `little54b.out` (s42 r1 rows for `fed`, medians otherwise):

| arm | receiver `det` (all holes) | receiver `orig_n` (filled by the original) | sender p0 `fed` (holes it was told about) | p0 `hn_n` | hn₀ / det |
|---|---|---|---|---|---|
| B2048 | 200 k | 199 k | 7.0 k | 168 | 0.0008 |
| B1024 | 185 k | 183 k | 51 k | 11 994 | 0.065 |
| B640 | 181 k | 179 k | 63 k | 55 038 | 0.30 |
| L2048 | 273 k | 272 k | 174 k | 174 618 | 0.64 |
| L1024 | 183 k | 182 k | — | 98 678 | 0.54 |
| L640 | 175 k | 174 k | 94 k | 89 199 | 0.51 |

**Explained.**
- The receiver sees ≈ 180–270 k short-lived reorder holes on every arm. `orig_p50` is 49–106 ms, about the legs'
  loop difference. Receiver-side, the population hardly moves on B.
- hn_n is the share of them visible to the sender, so **hn_n ≈ det × visibility(G_h, W)**:
  - On B (20 runs), the oldest runs are occupied by long-lived holes: old holes at 4 096, and loss holes in
    repair at 2 048. Their count ≈ λ_run·T_rep falls with the loop, so visibility rises 0.0008 → 0.065 → 0.30
    as W shrinks. `age_ripe/age_n` on p0 falls 0.98 → 0.13 (B2048 → B640): holes are first reported already
    ripe when the horizon is full.
  - On L (64 runs), visibility is high and nearly flat (0.51–0.64). hn_n follows the reorder volume itself, which
    falls with W (det 273 k → 175 k). That is the opposite direction, reproduced by the same product.
- **It is a gauge of the horizon's headroom, not of a store regime.** Launch 1's witness reading (CAND 174 against
  BASE 10 848 at the default pool) is horizon visibility plus the filter, and holds only at that pool, as the
  §54 erratum says.

**Flagged remainder.**
- p0 holes should not arise from cross-leg reorder at B640. Leg 1's forward one-way is ≈ 35–50 ms longer than
  leg 0's (rtt₁ 146 against rtt₀ 96, with 20 against 5 ms returns), and netem is FIFO. So no later-placed seq can
  overtake an earlier p0 seq.
- Yet p0 hn_n = 55 k ≫ p0's real losses (≈ 2.7 % × 425 k ≈ 11.5 k).
- The attribution fallback (`last_source_path`) is not it: `source_path_map` is pruned only under EVICT, so the
  map holds every seq.
- **Open, and it touches a guard witness.** p0 holes come either from a per-seq delay inversion not visible in
  the medians (quinn GSO batching across connections, the receiver's per-path I/O batching) or from the counter's
  resolution rule.
  - Launch 1's c8 outcome witness (`hn_n` < 2 000 on p0, §54 Amendment 1 deviation 2) reads this counter.
  - Until the anomaly is explained, that witness's reading as "the filter removed spurious fires" rests on an
    unexplained counter.
  - One `RWM_SUCC_DUMP` row settles it; it is in §13. It does not affect the law.

## 7. δ's seat and the trade curve (check 5)

δ enters only through q(δ)·ℓ.

| cell | n* Realtime → Bulk (fresh) | goodput | delay |
|---|---|---|---|
| c2 | 174 → 192 | 0.92 → 0.984 | Tp50 18 → 19 ms |
| c1s | 370 → 509 | 0.80 → 0.84 | — |
| c1d | 518 → 622 | 0.87 → 0.89 | — |
| c8 | 546 → 553 (coupled F, both legs) | ≈ 0.95 (p27) | ≈ 3 × 47 ms in-order |

δ's lever stays small, as W1 U2 found. q's 5–10 % band is the whole of δ's authority. At c8 the coupled F (not
δ-keyed) sets the curve's position. Widening δ's authority means widening q's band (U2). Weighting the coupling by
a δ readout would be a second δ seat, so it is not proposed.

## 8. Gauges

| gauge | content | why |
|---|---|---|
| `[SUMCAP] pin=` / `floor=` (exists) | clamp bind fractions, whole row and last third | rule 18; predicted 0 / 0 at every cell |
| **`[SLAW]` (new)**, per window close | n, R, T_w; per leg: wᵢ, T̂, T̂min, m̂, ℓ̂, b̂₀, the error (β − b̂₀ − q·ℓ̂) | the law's own trace; routing witness: the `[DIAG] win=` denominator = `[SLAW] n` on ≥ 99 % of ticks |
| **`anc=` (new)**, in `[SLAW]` | per leg, the bind fraction of m̃ = ℓ̂ + b̂₀⁺, plus (m̂ − ℓ̂) median and last-third max | E03's bound. Binding is expected on a queued fast leg. Persistent binding with the floor at target is the route-change signature (the domain condition, §3.3) |
| **b̂₀⁺ (new)**, in `[SLAW]` | the bound and its K windows' sample counts | the bound's tail cost (§3.5) |
| old holes (exists, server `[RANK]`) and `ripe_frac` (exists, `[HOLD]`) | holes older than one readout; share of holes already ripe at first report | the horizon binding as §3.4 defines it (old holes accumulate) |
| **truncation share (new)**, counter in `sack_to_gaps` | share of gap reports truncated at `MAX_NACK_GAPS` | C02's clamp has no gauge (ledger C02 column 6). It measures visibility (§6), not binding |
| **C09 cap bind (new)** | share of reports whose loop broke on `cached_max_repairs` / budget | it gates which holes get stamped (§6). C09's own gauge, owed by the ledger |
| per-leg placement share (exists, P38ROW) | shares[i] | reads K₁ |
| quinn backlog B = Σ`dgq[hand − tx]` (exists) | authority gauge (paper §6.1) | must stay ≈ 0 at the law's point |

## 9. Engine sources per input

| input | source | status |
|---|---|---|
| T̂ᵢ,ₖ, T̂minᵢ,ₖ, wᵢ,ₖ | a per-leg window sampler of `record_rtt_sample` | **missing on main.** The uncapped sampler exists on archived branches (oracle arm `run43`; `net/loopdiag.rs` on `archive/store-power`); port it |
| m̂ᵢ | Copa `min_rtt` (10 s) | present (E03; honest under §3.3's condition) |
| ℓ̂ᵢ | running minimum of the same echo series | missing (one field). quinn's `rtt.min()` is the wire clock, not the echo, so it is not a substitute |
| b̂₀ᵢ⁺ | the sampler's first K windows | missing (with the sampler); the map test passes (§3.5); closed loop is Part B |
| wire anchor (later) | `bbr_rs` `qrt` plus the boot echo−wire offset | present only on `bbr_rs` (K10) |
| q(δ) | `codel_setpoint_q` | present |
| T_w = 250 ms | the sampler's window close | missing (with the sampler) |
| n₀ = 128 | `RWM_STORE_BOOT` | present; unprovenanced (Q7) |
| a rate | — | **none needed.** E04 (`max_bw`) leaves the store law; its other readers (Copa's anchor floor, K01) are out of scope |

## 10. Diagnostic design (reported only; nothing scored)

### Part A: no build

Moved to §13 as a stand-alone registration.

### Part B: a gauge-only build

Port the window sampler, ℓ̂, m̃ and the boot b̂₀⁺, and compute R as a counterfactual readout
(`[SLAW]`, observation only). Gates:
- `GOLDEN_CAPTURE` unset;
- the shipped cap sequence is bit-identical with the gauge on.

Rows: §13's arms.

**Readouts:**
- R per window → its crossing W per cell, predicted c2 174–192, c8 545–553, c1s 370–509 (fresh; not registrable as values);
- b̂₀ at boot against the ledger values (2.28 / 2.85 / 3.91 / 0.29);
- the anchor creep m̂ − ℓ̂;
- sd(ln R) per window at T_w = 250 ms (the noise item of §3.3);
- the horizon bind and C09 bind fractions.

**Falsifiers:**
- the crossing outside ±30 % of prediction;
- **B1 (collapse condition):** on any row, b̂₀⁺ < (that row's later no-queue median brake) − q·ℓ̂, or R < 1 in
  the boot windows while n ≤ 128. It is one-sided and absolute;
- the `anc=` bind fraction > 0.5 on a leg whose queue floor is below target (a spurious bound);
- a second attractor: from a 15 s hold at 2 048, n not below 1 000 within 15 s of release (c2, c8);
- sd(ln R) at T_w = 250 ms > 0.15 (§42's oracle form read 0.08–0.11).

Part B decides whether an engine law can be registered. Nothing ships from either part.

## 11. Paper draft (text for `fec-arq-model.md` §6.1; not applied)

> ### 6.1 The outstanding store: a time law
>
> The store holds every sent symbol until its receipt is reported (Section 6.3). By Little's law, on each leg
> i the store holds λᵢ·Tᵢ symbols in transit plus the holes H awaiting repair; Tᵢ is the leg's echo loop
> from the placement stamp to the report. When the links are full, any store beyond Σλᵢ·T₀ᵢ + H is standing
> queue, and Tᵢ grows until it fills the store (status §54b: W/Σλ = 349 / 174 / 109 ms against a
> rate-weighted echo loop of 343 / 171 / 107 ms). The store's size therefore needs a target *time*, not a rate:
>
> ```text
>    n_{k+1}  =  clamp( n_k · Σᵢ wᵢ( Fᵢ + q(δ)·ℓ̂ᵢ + Ŝᵢ ) / Σᵢ wᵢ T̂ᵢ ,  floor ,  N·knee )
>    Fᵢ = max( min(m̂ᵢ, ℓ̂ᵢ + b̂₀ᵢ⁺) + b̂₀ᵢ⁺ ,  maxⱼ (ℓ̂ⱼ + b̂₀ⱼ⁺) ) ,   b̂₀ᵢ⁺ = max over the first K windows of (T̂minᵢ − m̂ᵢ)
> ```
>
> b₀ enters as a one-sided bound because an under-read collapses the store while an over-read costs only that many
> ms of queue. The anchor's rise above the lifetime floor is bounded by the same b̂₀⁺, which caps E03's creep. The
> maximum over legs is ρ = 1's in-order frontier: a faster leg may queue up to the slowest leg's floor at no
> in-order cost.
>
> Once per window, the store is scaled by the delivered-share-weighted ratio of each leg's target loop to its
> measured loop. The target is the no-queue loop (the 10 s echo minimum m̂, the window-against-filter minimum
> offset b̂₀, and the window spread Ŝ = T̂ − T̂min) plus CoDel's setpoint q(δ) on the leg's lifetime-minimum
> loop ℓ̂ [RFC8289]. Its fixed point is the time condition T̂min − m̂ − b̂₀ = q(δ)·ℓ̂: the standing queue floor
> is the setpoint. The store is then Σλᵢ·T*ᵢ + H, with the holes funded implicitly; H affects only the loop
> gain, 1 − (1 − H′)·n/(n − H), not the fixed point.
>
> The law reads no rate. The shipped (1 + q)·Σ bwᵢ·RTpropᵢ fed the store's own delivered rate is the map
> cap′ = cap·(1 + q)·RTprop/T̄, which contracts wherever (1 + q)·RTprop is below the no-queue loop
> (status §23: g ≈ 0.55–0.6 at c2). The defect was the time target, not the sampler. Every rate source on
> record (§23, §24, §27, §34, §36, §40, §43b) is therefore unnecessary.
>
> | symbol | provenance |
> |---|---|
> | T̂ᵢ, T̂minᵢ, wᵢ | measured: window median, minimum and sample count of the leg's echo loop |
> | m̂ᵢ | measured: Copa's 10 s echo minimum; honest while the leg's queue floor drains within the window (gauge: m̂ − ℓ̂) |
> | ℓ̂ᵢ | measured: lifetime echo minimum; scales the allowance only |
> | b̂₀ᵢ⁺ | measured: sample maximum of the no-queue brake over K boot windows (K = 8, declared) |
> | m̃ᵢ, Fᵢ | derived: the bounded anchor; the in-order floor |
> | q(δ) | cited: RFC 8289 §3.2, (b(δ) + 1)/30 |
> | window | 250 ms, declared (as the oracle arm), with the `wbind=` gauge |
> | n₀ | the shipped boot cap 128; no provenance (open) |
> | floor, knee | resource bounds; bind gauges `[SUMCAP] floor=` / `pin=` |
>
> Shape: order 0 in N for the update and order 1 for the fixed point; dimensionless in rates, so in the store's
> own unit; increasing and continuous in δ through q; no ρ input; one expression at every N.
>
> At N ≥ 2 the fixed point is a single scalar condition, and placement decides the per-leg split. With
> delivery-time placement (Section 5.7) a slow leg is fed only once the fast leg's loop approaches it. Under ρ = 1
> in-order delivery waits for the slowest leg anyway, so the floor term Fᵢ lets every leg queue up to the slowest
> leg's lifetime floor. A per-leg target would leave the slow leg idle at every weighting (static c8 map: 148–220
> against 545–553 with Fᵢ).

## 12. Open questions for the owner

- **Q1. G = 1 and start-high in closed loop.**
  - Every static map has one attractor with a returning start-high.
  - The c2 hold transfer (§43) reads R ≈ 0.98–1.03, and c8's coupled F reads 0.93–0.94 at 1 024.
  - Accept Part B's start-high readout as the test?
- **Q2 (narrowed). First-arrival consumers.** The ρ = 1 store couples to the slowest leg (derived, §5.2). Should
  the ρ < 1 gate (F02), when it exists, keep the coupling? Its consumers pay ≈ +30 ms of fast-leg first arrival.
- **Q3 (U3). The Bulk goodput cost** at c1s, c1d and c8 would fail guard v2 (§4): accept it, or widen the
  allowance (Q6).
- **Q6 (U2).** Is q's 5–10 % band δ's intended authority?
- **Q7. n₀ = 128** has no provenance, sets the c1s ramp (≈ 6 s at 250 ms) and is b̂₀⁺'s calibration condition.
- **Q8. The window:** 250 ms (declared, evaluated) or a scale-free derivation.

Closed in revision 2: b₀'s source (now a one-sided bound, §3.5); the anchor (m̃, a bound with a gauge, §3.3).

## 13. Registration: the store-pool sweep (reported only, no build; copyable into status)

**Why.**
- §54b measured three pools at 6.8–21.8 × the c8 BDP and does not bracket the knee.
- The store-law derivation (`docs/derivations/store-law.md`) predicts two shoulders at c8 and a single-leg knee at
  c2. Its inputs are static maps of an older era (p27: 100 MB, 2 rows per cap; p34 for c1).
- This block measures current-era static maps with the existing knob. It decides nothing by itself. It feeds the
  store law's closed-loop registration (Part B), the δ-authority question U2, and the lever-B guard.
- It is independent of the law's open blockers, because no law is built.

**Tree, knob and suites.**
- **Tree:** current main. The SHA is pinned by amendment after the VM pre-flight.
- **Knob:** `RWM_STORE_PATH_POOL=<pool>` (`gates.rs:323`).
  - The realised ceiling is `max(N·pool, floor)` (`store_cap.rs:90`). The boot cap is `min(128, store_max)`.
  - Every arm's W is ≥ 160 > 128, so neither bound inverts against the boot cap.
  - W binds only while the shipped ask exceeds it; see Liveness.
  - `RWM_STORE=<n>` is not used: it switches the law off and moves the seat.
  - The battery scrubs `-u RWM_STORE_PATH_POOL` (as plan 54b).
- **Engine suites** are skipped only if `raptorpath/src` and `raptorpath/tests` are byte-identical to the tree that
  passed §54's suites (`cand54` `7fd92dd5` / its merge on main). This is checked by a reuse manifest: a per-file
  sha256 list, diffed on the VM.
  - Any difference runs the full suites as in §54.
  - The tools tests (`tools/l1` python and bash tests) and a rehearsal (`LD_REHEARSE=1`) always run, because the
    sweep is a new harness path.
- **`RWM_SUCC_DUMP`** exists on main (default `RWM_SUCC_DUMP_MAX` = 200 000 records). The dump is written under
  the run tree on `/dev/shm`. The pre-flight checks that `/dev/shm` has ≥ 1 GB free, and the rehearsal records the
  dump's actual size.

**Arms** (Bulk, `--window-reliable`, `RWM_DIAG=1 RWM_RDIAG=1 RWM_PERF_TIMEOUT_S=300`):

| cell, size | pools (W) | n per pool | rows |
|---|---|---|---|
| c8-100, 600 MB | **640 (W 1 280, control = §54b B640)**, 384 (768), 256 (512), 160 (320), 112 (224), 80 (160) | 4 | 24 |
| c2-100, 600 MB | **2 048 (control, the shipped pool)**, 384, 256, 192, 160, 128 | 3 | 18 |
| c1s-400, 5 GB | **2 048 (control)** and 512: n = 6; 1 024, 768 and 384: n = 3 | 6 / 3 | 21 |
| c8-100, 600 MB, `RWM_SUCC_DUMP=1` | 640 | 1 | 1 (the hn_n anomaly, §6) |

- **Total:** 64 rows.
- **Order:** cells in blocks, c8 → c2 → c1s. Within a block, a Latin-square rotation of pools per replicate, seeds
  42 / 7 alternating. The c1s n = 6 arms take the extra replicates at the block's end, rotated.
- **Smoke:** one row per cell at its smallest pool.
- **Length:** about 24 + 18 + 22 + 1 min of rows. With the tools tests, the rehearsal and overheads that is about
  1.5 h. `LD_HARD` = launch + 3 h.

**Liveness and row treatment.**
- **Pool echo.** `[GATES]` and `pool_per_path` echo the requested pool on both ends. If they do not, the row is
  VOID and is replaced once.
- **Binding.**
  - **Control arms are exempt.** They are the shipped behaviour: c1s binds ≈ 0.7 at 2 048, and c2 is not pinned
    at 2 048.
  - A **non-control** row binds if its LATE-window bind fraction is ≥ 0.95. The LATE window is sending ticks at
    t ≥ 15 s (c1s: t ≥ 5 s).
  - An unbound non-control row is **not replaced**, since a rerun would repeat the miss. It is reported at its
    realised cap: the LATE-window mean of `win=…/b`. It is plotted at that W, and labelled `UNBOUND`.
- **DNF** (as 54b): goodput counts as 0, and every other readout comes from the row's `[DIAG]` lines.
- **Flags** (reported, never voiding):
  - **c1s quinn-window collapse:** the share of windows with `qcwnd` < ¼ of the row median exceeds 0.1 (§43b's
    duration measure).
    - Flagged rows are reported in every table.
    - They are **excluded from F8**, and the count is stated.
  - **c8:** the lock-out classifier (§3.9).
- **The SUCC_DUMP row** must write a dump with ≥ 100 000 records. Otherwise it is re-run once.

**Readouts per row** (LATE window; `store-law/little54b.py` and `pool54_score.py` definitions):
- per leg: tx rate rᵢ, app-echo rtt and wire rtt, `rtp_us` against its boot value, q₀ (`dgq hand − tx`), and the
  placement share;
- Σ send rate, goodput, tail, H0;
- in-order delay, late mean and p95;
- old holes (server `[RANK]`), `[HOLD]` `hn_n`, `fed` and `ripe_frac` per path, `[SUCC]` `det`;
- the Little residual (W − Σrᵢ·rttᵢ)/W;
- duplicates, retransmits/GB, client and server CPU/GB;
- the realised cap and the bind fraction.

**Readout per cell (new): the standing-queue time at the knee.**
- **Definitions:**
  - T̄(W) = the rate-weighted LATE echo loop Σrᵢ·rttᵢ/Σrᵢ.
  - T₀ = the same statistic over the first 1 s of ticks of the cell's smallest-W rows (no queue).
  - ℓ = the rate-weighted boot `rtp_us`.
- **Readout:** from the arm medians, interpolated in W, the W where goodput first reaches 0.95 and 0.98 of the
  control. At those two points, report T̄ − T₀ in ms and as a multiple of ℓ.
- **Why.** The derived law puts the standing queue at (b̂₀⁺ − b₀) + q(δ)·ℓ.
  - The estimator slack (0.4–3.7 ms) already exceeds δ's whole term (q·ℓ ≈ 0.12–4 ms).
  - The static-map points move only 174 → 192 (c2) and 370 → 509 (c1s) from Realtime to Bulk.
  - So δ has almost no authority. That is U2, which goes to the owner with this readout after Part A.

**Predictions** (bands; these are static-cap measurements, so registrable here).

c8, every control band from the §54b **B640** rows (`docs/l1-raw/s54/rep54b-*`, main's 20-run horizon;
`little54b.out`):

| c8 W | 1 280 (control) | 768 | 512 | 320 | 224 | 160 |
|---|---|---|---|---|---|---|
| Σ send rate /s | 11 629–11 732 (B640) | 11 400–11 650 | 11 050–11 600 | 10 300–10 900 | 9 700–10 100 | ≤ 9 300 |
| r₀ /s | 9 776–9 840 | ≥ 9 750 | ≥ 9 700 | ≥ 9 600 | 9 400–9 800 | ≤ 9 300 |
| r₁ /s | 1 839–1 913 | 1 650–1 900 | 1 400–1 800 | ≤ 1 000 | ≤ 400 | ≤ 300 |
| T̄ ms (≈ W/Σr) | 103–107 | 60–70 | 42–48 | 28–32 | ≈ 22 | ≈ 17 |
| rtt₀ / rtt₁ ms | 96 / 138–161 | — | — | — | — | — |
| goodput Mbit/s | 106.5–107.6 | — | — | — | — | — |
| in-order late mean s | 0.34–0.39 | 0.18–0.26 | 0.13–0.18 | 0.08–0.12 | 0.06–0.08 | — |
| old holes | 0 | 0 | 0 | 0 | 0 | 0 |

The c8 standing-queue readout is predicted to sit at W ≈ 500–700 (leg-1 shoulder), with T̄ − T₀ ≈ 25–35 ms
(the coupled floor, §5.2).

- **c2:** goodput ≥ 0.97 × control at W ≥ 192; 0.88–0.95 at 160; 0.72–0.82 at 128. T̄ ≈ W/9.6 k.
- **c1s:** goodput / control ≈ 0.94 (1 024), 0.92 (768), 0.84 (512), 0.80 (384), ± 0.05 (p34 era). Flagged rows
  are excluded.
- **Anchor:** `rtp_us` on c8 p0 falls toward boot as W shrinks, ≤ boot + 10 ms at W ≤ 320. On leg 1 it stays within
  boot + 8 ms at every W.

**Re-basing (control drift).**
- The c8 control is compared with B640 by **medians**, not by six-row min–max (that check nearly always fires,
  §54 launch 2).
- A readout is re-based iff |control median − B640 median| > tol, where tol = the B640 rows' full range for that
  readout, floored at 1 % of the B640 median. For example: Σ send rate tol = 103 /s; r₁ tol = 74 /s; in-order
  late tol = 0.05 s.
- If a readout re-bases, that readout's bands at the other W shift by the same median offset, and the shift is
  reported.
- The c2 and c1s controls have no prior bands. They set their cell's reference directly.

**Falsifiers (reported; each changes the derivation, not a ship decision):**

| # | what fires it | what it means |
|---|---|---|
| F1 | c8 r₁ at W = 320 ≥ 0.8 × r₁ at 1 280 | leg 1 is fed without a fast-leg queue; the coupled F is not needed at c8 |
| F2 | c8 Σ rate at 768 < 0.97 × control | the knee is above 768 |
| F3 | c8 r₀ < 0.97 × 9 800 at W = 224 | the leg-0 shoulder is above prediction |
| F4 | c8 `rtp0` at W ≤ 320 > boot + 10 ms | E03's queue persists even at small W; the bound m̃ is load-bearing at c8 |
| F5 | \|W − Σrᵢ·rttᵢ − old holes\| > 0.1·W at any bound W ≥ 320 | Little fails |
| F6 | old holes > 50 at any W ≤ 1 280 | P_h is lower than the derivation says |
| F7 | c2 goodput median at 192 < 0.95 × control | the c2 knee moved up in this era |
| F8 | c1s goodput median at 512 ≥ 0.95 × control, over unflagged rows of n = 6 (excluded count stated) | the c1s cost in the derivation is overstated |
| F9 | the SUCC_DUMP row shows p0 holes overtaken by later-placed p1 seqs | the hn_n anomaly is a real delay inversion, not a counter rule |
| F10 | the standing-queue multiple (T̄ − T₀)/ℓ at the 0.95 point differs across c1s, c2 and c8 by more than ×3 (max/min) | **the q·ℓ form is wrong**, not just its band: the knee's queue does not scale with the lifetime loop |

**Decision rule.** None; reported only. The maps replace p27/p34 as the inputs of Part B. F9's reading goes to the
guard-witness note (§6). F10 and the standing-queue readout go to the owner with U2.

### 13.1 Carried to Part B (open; nothing solved here)

- **K = 8 and n₀ = 128 have no provenance.** "Declared constant" is not an allowed class (FORMULA-FIRST). Each needs
  a derivation or a resource bound stated outside the law.
- **b̂₀⁺'s tail.** One bad boot window (45–49 ms at c8) becomes a standing queue for the transfer's lifetime. Gauge
  the K windows' spread per row (max − median, sample counts), and derive a bound that a single window cannot set.
- **The max over lifetime floors steps when a leg joins or dies.** An idle or dead slow leg sets the fast leg's
  target indefinitely, because ℓ̂ never expires. Check the shape in N (legs joining, symmetric and asymmetric), and
  the leg-death case: the term must leave F continuously when the leg leaves the live set.
- **ρ.** The coupling is justified at ρ = 1 only, and "a future ρ < 1 gate" is mode-switch wording. The coupling
  needs a form continuous in ρ, or an explicitly stated domain (the store gate exists only at ρ = 1, F02).
