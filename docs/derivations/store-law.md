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

## 0. Answer

**The law needs no rate.** The shipped law fed an honest delivered rate is already a pure time law,
cap' = cap · T*/T̂. Its collapse at c2 is a time-target defect, T* < T₀, not a rate defect (§2). The derived
law therefore reads only times, plus dimensionless per-leg weights:

```text
   n_{k+1}  =  clamp( n_k · Σᵢ wᵢ·( m̂ᵢ + b̂₀ᵢ + q(δ)·ℓ̂ᵢ + Ŝᵢ ) / Σᵢ wᵢ·T̂ᵢ ,  floor ,  N·knee )        (one update per window k)
```

> *Sentence:* once per window, scale the store by the ratio of the target loop to the measured loop, averaged
> over the legs by delivered share. A leg's target loop is its no-queue loop (m̂ + b̂₀ + Ŝ) plus CoDel's
> standing-queue setpoint q(δ)·ℓ̂.

Since Ŝᵢ = T̂ᵢ − T̂minᵢ, the update is equivalently n·(1 − Σwᵢ·(β̂ᵢ − b̂₀ᵢ − q·ℓ̂ᵢ)/Σwᵢ·T̂ᵢ), where
β̂ = T̂min − m̂ is the brake. **The fixed point is a time condition:** the queue floor, net of its no-queue
offset, equals q(δ)·ℓ (one CoDel setpoint, RFC 8289).

At that point Little gives the store size n* = Σᵢ λᵢ·T*ᵢ + H. The hole term H is implicit. The store funds the
holes automatically, H drops out of the fixed-point condition and of its slope, and §6.1's deferred ρ hole term
is not needed (§3.4).

**What replaces the rate is the reference time m̂ + b̂₀.** It is observable only if the leg's queue drains
within the anchor's 10 s window, so its honesty is a property of the fixed point:
- In §54b it holds on leg 1 at W ≤ 2048, where `rtp1` is 35–46 ms against a 33–37 ms boot value. It fails on
  leg 0, where `rtp0` is 34–319 ms on a 10 ms leg.
- The allowance sits on the lifetime minimum ℓ, never on the anchor. Any anchor creep is therefore at most
  linear, bounded by b₀ + qℓ − f per 10 s (f is the floor's fluctuation), not exponential (§3.3).
- b₀ has no engine source and blocks an engine law. Without it c2 collapses again (§3.5).

**Predictions** (static maps on the inputs the law reads; §4):

| cell | the law's point, Realtime / Auto / Bulk | goodput | clamp bind |
|---|---|---|---|
| c2 | 174 / 180 / 192 (fresh anchor); 140 / 148 / 164 (expired) | Bulk at the 0.984 knee; ≥ 0.89 expired | no collapse: ask/n ≥ 1 at every cap ≤ 128 with a fresh anchor |
| c1s | 370 / 385 / 509 (fresh); 561 / 590 / 645 (expired) | 0.80–0.88 of Gmax, the power point and not the 0.95 knee | ≈ 0.7 today → predicted 0 |
| c8 | 148–212 | p27 map: 0.66–0.88 of Gmax; current-era estimate ≈ 9.6–10.0 k/s against 11.7 k/s (−15 %) | — |

The c8 point is fast-leg-only: placement does not feed leg 1 without a fast-leg queue. Owner option O1 (fund
that queue up to the slow leg's floor) moves c8 to ≈ 575 on both legs (§5.3).

**§54b does not bracket the knee** (§5):
- The 640 arms sit at 6.8 × BDP. Leg 1 is already on its shoulder there: L r₁ 1 974–2 009 at W = 4 096 against
  1 866–1 946 at 1 280, with no overlap.
- The c8 knee has two shoulders:
  - leg 1 near W ≈ Σμ·T₀,₁ ≈ 500–700;
  - leg 0 near W ≈ μ₀·T₀,₀ ≈ 180–250.
- §10 gives the reported-only arms that locate both.

**`hn_n`** = (reorder holes at the receiver) × (the share the 20-run report horizon lets the sender see):
- the receiver's `det` holds at 180–210 k on every B arm;
- the sender's `fed` on p0 rises 7.0 k → 51 k → 63 k;
- the L arms run the other way because their visibility is not horizon-limited.

One remainder is flagged (§6).

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
| shipped law on the send-interval sampler (§23 Q2) | yes, with T* = (1 + q)·RTprop | T* < T₀ (table above) | yes: T* = m̂ + b̂₀ + Ŝ + q·ℓ̂ > T₀ by construction, since T₀ = m + b₀ + S |
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
| this derivation's first draft: max over legs of (m + b₀ + S) | yes, coupled | at c8, S₁ rises 8.6 → 118 ms with n; ask/n 1.9–2.9, crossing near 1 850 (`map27` history). On §54b the shipped anchors order the legs wrongly (`rtp0` medians 43–54 ms > `rtp1` 38 ms at W = 1 280) | dropped. The coupling is owner option O1, built on ℓ (§5.3) |

## 3. The law

### 3.1 Formula and provenance

```text
   n_{k+1}  =  clamp( n_k · R_k ,  floor ,  N·knee )

   R_k  =  Σᵢ wᵢ,ₖ · ( m̂ᵢ + b̂₀ᵢ + q(δ)·ℓ̂ᵢ + Ŝᵢ,ₖ )  /  Σᵢ wᵢ,ₖ · T̂ᵢ,ₖ          Ŝᵢ,ₖ = T̂ᵢ,ₖ − T̂minᵢ,ₖ
```

| symbol | meaning | provenance |
|---|---|---|
| T̂ᵢ,ₖ, T̂minᵢ,ₖ | median and minimum of leg i's app-echo loop samples in window k | measured. The echo is the loop the store holds a symbol for; the key is the placement stamp (§55 Erratum 1). Sampler: the oracle arm's (`[OCAP]`, archived), not on main |
| wᵢ,ₖ | leg i's sample count in window k (∝ its delivered rate) | measured; dimensionless in R. Any per-leg delivered count gives the same R |
| m̂ᵢ | Copa's 10 s minimum of the same echo series | shipped (`copa.rs:1000-1008`, `path.rs:1070`). Honest only under the drain condition (§3.3) |
| ℓ̂ᵢ | lifetime minimum of the same echo series | measured; never expires. Used only as the scale of the allowance, so an under-read only shrinks the allowance by q·Δ (safe side) |
| b̂₀ᵢ | E[T̂minᵢ,ₖ − m̂ᵢ] at no own queue (window-against-filter minimum gap) | **no engine source (blocking).** Ledger values: c2 2.28, c8 2.85 / 3.91, c1s 0.19 (p27) / 0.29 (p34), c1d 0.24–0.38 ms. The spread estimator fails at load (§3.5) |
| q(δ) | CoDel setpoint (b + 1)/30 ∈ [0.05, 0.10] | cited, RFC 8289 §3.2; shipped `codel_setpoint_q`. **The only δ seat** |
| window T_w | Tp50/q(δ) | derived: paper §6.1's own lower bound (a stale share ≤ q), the named "scale-free successor". It sits below the 10 s filter at every cell (c2 ≈ 170–380 ms, c1s ≈ 40–80 ms). Its noise is unmeasured below 250 ms |
| floor | 10 | resource bound (shipped) |
| N·knee | 2 048 per live path | resource bound (shipped), not a law term. Bind gauge `[SUMCAP] pin=` |

**No rate, no gain constant and no count other than n.** G = 1 is not a parameter: it is the update that makes
T̄ = T* the fixed point. The weights wᵢ make the update dimensionless.

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
scale-free). With T_w = Tp50/q, ln(n*/128) is reached in:
- c2: ≈ 7 windows ≈ 1.2 s;
- c1s: ≈ 28 windows ≈ 1.1 s.

With a fixed T_w of 250 ms the same climb takes 1.7 s at c2 and ≈ 7 s at c1s. Most c1s guard rows are shorter
than that (14 ticks), so T_w = 250 ms would put the guard's c1s rows in start-up.

**Noise.** At gain ≈ 1, sd(ln n) ≈ the per-window coefficient of variation of (β − b₀)/T̂. §42 measured the
oracle form at T_w = 250 ms: 0.08 at c2, 0.11 at c1s. **Open:** the noise at T_w = Tp50/q (40–80 ms at c1),
which is a registered readout (§10).

**The anchor (m̂ = Copa's 10 s minimum).**
- *Drain condition.* m̂ is honest iff the leg's queue floor fluctuates by at least f ≥ b₀ + q·ℓ within each
  10 s window.
- *What §54b's anchors read* (`little54b.out`):
  - leg 1 at W ≤ 2 048: `rtp1` 35–46 ms against a boot value of 33–37. It is honest; the slow leg's queue drains;
  - leg 0: `rtp0` 34–60 ms at W = 1 280 and 237–319 at L2048, on a 10 ms leg. It never drains at W ≥ 1 280.
  - At the law's predicted points the queue floor target is only b₀ + q·ℓ ≈ 3–4 ms (c2, c8 leg 0). Whether it
    drains there is the primary falsifier of §10.
- *Creep is linear, not exponential.* If the condition fails, m̂ rises by at most (b₀ + q·ℓ − f) per filter
  period. The target rises with m̂, but the allowance does not, because it sits on ℓ.
- *The q·m̂ variant fails here.* §43's oracle form and the `ratio` variant in `map27` put q on the anchor. Under
  an expired anchor that variant has an upper trap at Bulk: c2 U765, c1d (p34) U914. The ℓ form has none at c2
  and c8, and at c1d (p34) only a touch of 1.00 at 1 020–1 025.
- **Departure from W1 (G < 1 required).**
  - W1's requirement came from (a) the oracle form with q·RTprop, and (b) a trap edge of 3.08 ms at G = 1 at c8
    with τ = 5 ms.
  - Here the allowance does not scale with the anchor, and both static expired maps are trap-free at G = 1.
  - G < 1 is incompatible with a q-sized allowance: a leg saturates only if G·(1 + q·ℓ/T₀) > 1. On every map,
    G = 0.9 gives no crossing at all (`map27-*.out`): it collapses.
  - What G = 1 cannot be shown to do from the logs is leave today's start-high state. §43's start-high hold ratios
    (1.006 / 1.059 / 1.005 at gain 1.1) transfer to R ≈ 0.98–1.03 at H = 1 536, using the c2 Little factor 1.05.
    That is marginal. **c2 does not collapse; it is not shown to leave the pin** (owner question Q1).

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

**The coupling, as a condition.** The 20-run horizon binds when the open gap runs at the receiver exceed 20. That
count is ε_run·c·transit, so it binds above a transit P_h ∝ G_h:
- P_h(20) ∈ (1.9 k, 2.5 k) at c8;
- predicted P_h(64) ≈ 3.2 × that, about 6–8 k. That is consistent with L2048, which shows no old holes at 4.1 k.

Above P_h, H' → 0.72 and the store law loses most of its authority. Its slots go to holes.

**At the law's c8 point (≤ 600) transit is ≥ 3 × below P_h(20).** So:
- the horizon does not bind there;
- lever B is not required for the store's sake. Its residual at W = 1 280 is in-order 0.30 against 0.35 s, which
  does not overlap.

The store size and the gap horizon are one question, as the brief says. With a time law the answer is the
inequality transit* < P_h(G_h), and one gauge (§8) checks it.

### 3.5 b₀: load-bearing and without a source

**b₀ is load-bearing.** Without it the target sits b₀ below the no-queue floor and c2 collapses again:
- ratioL at b₀ = 0 gives R = (9.5 + 0.95 + 4.9)/16.1 = 0.95 at 128;
- and (7 + 0.7 + 5.4)/14.9 = 0.88 at 64.

That is §23's mechanism through the other symbol.

**Tested estimator** (`b0est.py`): b̂₀ = max_k S_k − mean_k S_k, which is queue-invariant wherever S is.
- At c2 at no queue it reads 2.0–3.0 against the ledger's 2.28 ✓.
- At load it inflates to 4.5–17 ms (c2 256–1 024).
- At c1 it reads 0.4–1.1 against 0.19–0.38 (×2–3) and grows with n.
- At c8 leg 1 it reads 8 against 4.

**It fails**, because S is not queue-invariant at c8 and c1. §6.1 noted only c2's invariance.

**Remaining candidate (untested): boot calibration.**
- The rule: b̂₀ = the median of (T̂min_k − m̂) over the first windows, while n ≤ the boot cap of 128.
- 128 sits below or near the BDP at all four cells (c1s ≈ 234, c8 ≈ 190, c2 ≈ 100–140), so the boot windows
  are no-queue.
- It is an estimator with a declared calibration interval, not a δ/ρ mode.
- Risk: b₀ is not stationary (receiver load at c1).
- **Blocking until §10 Part B measures it against the ledger values.**

## 4. Per-cell predictions (static maps, the inputs the law reads)

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

Goodput is 0.87–0.90 of Gmax 818. The bind fraction is predicted 0.

### 4.4 c8 (N = 2)

ratioL gives n* = 195 / 201 / 212 (fresh) and 148 / 159 / 181 (expired). There leg 1 delivers 103–232 /s on p27,
against a link of ≈ 2 000. **It is a fast-leg-only point** (§5).

p27 goodput at 128–256 is 0.66–0.88 of Gmax 100.9 on 8 s rows. The current-era estimate is ≈ p0 alone,
9.6–10.0 k/s against 11.7 k/s (−15 %).

The in-order delay falls accordingly: ≈ 3 × T̄ ≈ 50–70 ms against 0.30–0.35 s at W = 1 280.

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
| 768 | 11 450–11 650 (predicted) | 60–70 ms | 0.18–0.25 s | §10 arm |
| 512 | 11 100–11 600 | 42–48 ms | 0.13–0.17 s | §10 arm |
| 320 | 10 300–10 900 | 28–32 ms | 0.08–0.12 s | §10 arm |
| 224 | 9 700–10 100 | ≈ 22 ms | 0.06–0.08 s | §10 arm (≈ the law's ratioL point) |
| 160 | ≤ 9 300 | ≈ 17 ms | — | §10 arm (below K₂) |

The δ dial moves the law along this curve only through q·ℓ: a few ms, ≈ 10–30 symbols. **At c8 the curve is
shaped by the placement coupling, not by δ.**

### 5.2 Why the law does not choose K₁ by itself

ratioL is per leg. Leg 1 at no queue has β₁ ≈ b₀₁, so it adds +q·ℓ₁ with weight w₁ ≈ 0.02 when starved. The
store grows; placement sends the extra slots to leg 0; leg 0's brake rises above its target; R → 1 at the
fast-leg point. The law and placement disagree about what a slot is for. This is W1 open item 7, now measured.

### 5.3 Owner option O1: the in-order coupling (not in the formula)

**Argument.** Under ρ = 1, in-order delivery waits for the slowest leg's loop. So a queue on a faster leg up to
that leg's floor costs no in-order delay; it moves waiting from the reorder buffer to the fast leg's queue.

**The form** raises each leg's floor target to the slowest leg's *lifetime* floor:

```text
   target_i  =  max( m̂ᵢ + b̂₀ᵢ ,  maxⱼ ( ℓ̂ⱼ + b̂₀ⱼ ) )  +  q(δ)·ℓ̂ᵢ
```

- It uses ℓ̂ because the fast leg's m̂ inflates under that queue: §54b `rtp0` medians 43–54 ms > `rtp1` 38 ms
  at W = 1 280. With m̂ in the max, the creep loop diverges.
- A max of continuous values is continuous; it reads no dial. At N = 1 it reduces exactly to the law (ℓ ≤ m̂).
- p27 c8 (`ratioX`): n* ≈ 573–580 (stable) on both legs, at ≈ 0.95 of Gmax. The 252–281 pair is the cap-256 row
  with no leg-1 windows, an artefact.

**What it buys and costs:** +13–17 % goodput at c8, for ≈ +25–30 ms of p0 first-arrival delay and an in-order
delay of ≈ 3 × 50 ms instead of ≈ 3 × 20 ms. **Owner question Q2.**

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
- **Open:** p0 holes come either from a per-seq delay inversion not visible in the medians (quinn GSO batching
  across connections, the receiver's per-path I/O batching) or from the counter's resolution rule. One
  `RWM_SUCC_DUMP` row settles it. It does not affect the law.

## 7. δ's seat and the trade curve (check 5)

δ enters only through q(δ)·ℓ.

| cell | n* Realtime → Bulk (fresh) | goodput | delay |
|---|---|---|---|
| c2 | 174 → 192 | 0.92 → 0.984 | Tp50 18 → 19 ms |
| c1s | 370 → 509 | 0.80 → 0.84 | — |
| c1d | 518 → 622 | 0.87 → 0.89 | — |
| c8 | 195 → 212 (fast leg) | — | — |

δ's lever stays small, as W1 U2 found. q's 5–10 % band is the whole of δ's authority, and the only lever
larger than q at c8 is O1 (§5.3), which is not δ-keyed. Two ways to widen it, neither picked:
- widen q's band (U2);
- let O1's coupling be weighted by a continuous δ readout. That would be a second δ seat and needs a new map, so
  it is not proposed.

## 8. Gauges

| gauge | content | why |
|---|---|---|
| `[SUMCAP] pin=` / `floor=` (exists) | clamp bind fractions, whole row and last third | rule 18; predicted 0 / 0 at every cell |
| **`[SLAW]` (new)**, per window close | n, R, T_w; per leg: wᵢ, T̂, T̂min, m̂, ℓ̂, b̂₀, the error (β − b̂₀ − q·ℓ̂) | the law's own trace; routing witness: the `[DIAG] win=` denominator = `[SLAW] n` on ≥ 99 % of ticks |
| **anchor creep (new)**, in `[SLAW]` | (m̂ᵢ − ℓ̂ᵢ) per leg: median and last-third max | the drain condition. Predicted ≤ b₀ + q·ℓ + 2 ms; a linear rise is the failure |
| **horizon bind (new)**, counter in `sack_to_gaps` | share of gap reports truncated at `MAX_NACK_GAPS` | C02's clamp has no gauge today (ledger C02 column 6). It decides whether transit < P_h |
| **C09 cap bind (new)** | share of reports whose loop broke on `cached_max_repairs` / budget | it gates which holes get stamped (§6). C09's own gauge, owed by the ledger |
| per-leg placement share (exists, P38ROW) | shares[i] | reads K₁ |
| quinn backlog B = Σ`dgq[hand − tx]` (exists) | authority gauge (paper §6.1) | must stay ≈ 0 at the law's point |

## 9. Engine sources per input

| input | source | status |
|---|---|---|
| T̂ᵢ,ₖ, T̂minᵢ,ₖ, wᵢ,ₖ | a per-leg window sampler of `record_rtt_sample` | **missing on main.** The uncapped sampler exists on archived branches (oracle arm `run43`; `net/loopdiag.rs` on `archive/store-power`); port it |
| m̂ᵢ | Copa `min_rtt` (10 s) | present (E03; honest under §3.3's condition) |
| ℓ̂ᵢ | running minimum of the same echo series | missing (one field). quinn's `rtt.min()` is the wire clock, not the echo, so it is not a substitute |
| b̂₀ᵢ | — | **missing, blocking.** The spread estimator fails (§3.5); boot calibration is untested |
| q(δ) | `codel_setpoint_q` | present |
| T_w = Tp50/q | the sampler's window close | missing (with the sampler) |
| a rate | — | **none needed.** E04 (`max_bw`) leaves the store law; its other readers (Copa's anchor floor, K01) are out of scope |

## 10. Diagnostic design (reported only; nothing scored)

### Part A: no build (the pool knob on current main)

**Arms.** `RWM_STORE_PATH_POOL` sets the ceiling W while the shipped ask exceeds it.
- c8-100, 600 MB, n = 4 per arm (24 rows, ≈ 30 min), rotated: W = 1 280 (control, re-anchoring §54b), 768, 512,
  320, 224, 160, i.e. pool 640 / 384 / 256 / 160 / 112 / 80.
- c2-100, 600 MB, n = 3 (18 rows): W = 2 048 (control), 384, 256, 192, 160, 128.
- c1s-400, 5 GB, n = 3 (15 rows): W = 2 048, 1 024, 768, 512, 384.

**Liveness (per row, voiding).**
- The pool echo in `[GATES]` and `pool_per_path`, as §54b.
- Bind fraction ≥ 0.95 on the LATE window. If the shipped ask fell below W, the row did not fix W and is
  `VOID-UNBOUND`, reported.
- The lock-out classifier (§3.9) at c8.

**Readouts.**
- Per leg: tx rate, rtt and wrtt, `rtp_us` against its boot value (the anchor), q₀;
- Σ rate and goodput;
- tail, in-order late mean and p95;
- old holes, `[HOLD]` hn and fed per path, `[SUCC]` det;
- the Little residual / W.

**Predictions:** c8 as §5.1; c2 goodput ≥ 0.97 × control at W ≥ 192 and 0.72–0.82 at 128; c1s as the p34
column of §4.2.

**Falsifiers:**

| # | what fires it | what it means |
|---|---|---|
| F1 | c8 r₁ at W = 320 ≥ 0.8 × r₁ at 1 280 | leg 1 is fed without a fast-leg queue; K₁ is lower; O1 is moot (good news for ratioL) |
| F2 | c8 Σ rate at 768 < 0.97 × control | the knee is above 768 |
| F3 | c8 r₀ < 0.97 × 9 800 at W = 224 | K₂ is above prediction |
| F4 | c8 `rtp0` at W ≤ 320 > boot + 10 ms in the LATE window | the drain condition fails even at small W; m̂ is refuted as the law's anchor at c8 |
| F5 | \|W − Σrᵢ·rttᵢ − old holes\| > 0.1·W at any W ≥ 320 | Little fails |
| F6 | old holes > 50 at any W ≤ 1 280 | P_h is lower than §3.4 says |
| F7 | c2 goodput at 192 < 0.95 × control | the c2 knee moved up; the law's c2 point costs > 5 % |
| F8 | c1s goodput at 512 ≥ 0.95 × control | the c1s cost of §4.2 is overstated |

### Part B: a gauge-only build

Port the window sampler, ℓ̂ and the boot b̂₀ calibration, and compute R as a counterfactual readout
(`[SLAW]`, observation only). Gates:
- `GOLDEN_CAPTURE` unset;
- the shipped cap sequence is bit-identical with the gauge on.

Rows: Part A's arms.

**Readouts:**
- R per window → its crossing W per cell, predicted c2 174–192, c8 148–212, c1s 370–509 (fresh);
- b̂₀ at boot against the ledger values (2.28 / 2.85 / 3.91 / 0.29);
- the anchor creep m̂ − ℓ̂;
- sd(ln R) per window at T_w = Tp50/q and at 250 ms (the noise item of §3.3);
- the horizon bind and C09 bind fractions.

**Falsifiers:**
- the crossing outside ±30 % of prediction;
- b̂₀ at boot outside ×[0.5, 2] of the ledger value;
- sd(ln R) at T_w = Tp50/q > 0.25.

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
>    n_{k+1}  =  clamp( n_k · Σᵢ wᵢ( m̂ᵢ + b̂₀ᵢ + q(δ)·ℓ̂ᵢ + Ŝᵢ ) / Σᵢ wᵢ T̂ᵢ ,  floor ,  N·knee )
> ```
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
> | b̂₀ᵢ | measured offset with no engine source yet (ledger: c2 2.28, c8 2.85 / 3.91, c1 0.2–0.4 ms) |
> | q(δ) | cited: RFC 8289 §3.2, (b(δ) + 1)/30 |
> | window | derived: Tp50/q(δ), the bound below which a window's stale share exceeds q |
> | floor, knee | resource bounds; bind gauges `[SUMCAP] floor=` / `pin=` |
>
> Shape: order 0 in N for the update and order 1 for the fixed point; dimensionless in rates, so in the store's
> own unit; increasing and continuous in δ through q; no ρ input; one expression at every N.
>
> At N ≥ 2 the fixed point is a single scalar condition, and placement decides the per-leg split. With
> delivery-time placement (Section 5.7) a slow leg is fed only once the fast leg's loop approaches it, so the
> law's point at c8 is fast-leg-only. Whether the store funds that queue (in-order delivery waits for the slowest
> leg either way) is an open design decision (status: store-law O1).

## 12. Open questions for the owner

- **Q1. G = 1 and the start-high margin.**
  - The ℓ form is trap-free on every static expired map at G = 1.
  - The §43 hold transfer at c2 reads 0.98–1.03: not shown to leave today's pin within a filter length.
  - G < 1 is not an option with a q-sized allowance: it collapses on every map.
  - Do you accept G = 1 with Part B's start-high readout as the test? The alternative is a larger allowance,
    i.e. U2.
- **Q2. O1:** should the store fund the fast-leg queue that placement needs before it feeds the slow leg?
  - This is W1 open item 7.
  - It means +13–17 % goodput at c8 against ≈ +30 ms of p0 first arrival and ≈ 2.5 × the in-order delay of the
    fast-leg point.
  - It does not change the formula at N = 1.
- **Q3. c1s cost.** The time law sits at the power point: −10 to −20 % goodput against a pinned main at ≈ 1/3 of
  its delay. This is W1 U3 at c1.
- **Q4. b₀'s source.** Is boot calibration (a declared calibration interval) acceptable, or must b₀ be a running
  estimator? None is known that survives load (§3.5).
- **Q5. The anchor.** If F4 fires (the queue does not drain at the law's point), the choices are:
  - an explicit periodic drain (a time-indexed probe, BBR-style, not a δ/ρ mode);
  - accepting a linear creep bounded by the transfer length.
- **Q6. U2 unchanged:** is q's 5–10 % band δ's intended authority? (§7)
