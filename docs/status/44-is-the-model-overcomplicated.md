## 44. Is the model overcomplicated? — law inventory and simplification review — no VM, record

The user asked whether the formulas could be simpler. An Opus inventory of
every shipped, default-off and candidate law (with its formula, inputs,
constants, dial dependence and coupling) was reviewed with the advisor.
Nothing is changed here.

**Verdict.** The laws that act are few. The complexity sits in duplicate
inputs, switched-off arms and constants. On the measured Bulk cells only a
handful of mechanisms move the wire: placement's base cost, the pooled cap
and its knee, SACK release, the recovery thresholds, quinn's BBR, and the
burst bound.

*Inventory counts.*
- Smoothed RTT: four copies (the app-echo `est.rtt()` and Copa `srtt` are
  the same samples at the same gain; quinn SRTT; a pooled max).
- RTprop / min RTT: four (Copa's 10 s min, `est.rtt()` used as RTprop in
  the deadline, quinn's lifetime min, a 250 ms Tmin), plus Copa's P10 floor.
- Delivery rate: six, plus three candidates.
- δ → coefficient maps: nine.
- Repair ledgers: three, plus the generation deficit.
- Loss estimators read by different laws: three.
- Recovery timeouts: about eight k·SRTT forms with unshared clamps.

**1. One dial coordinate (algebra checked against the shipped maps).**
Let u = −½·log₁₀(δ/δ_auto), so Realtime is −1, Auto 0 and Bulk +1. Then:
- b = 2^u;
- β = clamp(u, 0, 1);
- w_bw = (1 + u)/2;
- q = (2^u + 1)/30;
- t_tail = 10⁻⁵·100^u.

So the five shipped δ maps are one coordinate with five readouts, and one
continuity gate can cover them. A store-law gain g would be a sixth
readout: the "continuous parameter in the triangle" the user asked for.
This also shows β is flat across the whole Realtime half.

**2. The store-law candidate simplifies.** Shipped: cap = (1+q)·Σ
rate·RTprop. Candidate: cap = g·Σ L̂·T₀, with T₀ = R̄ − (Tmin − RTprop) and
R̄ = occupancy ÷ departures. It is the same sentence (gain × rate ×
unloaded time) with both inputs replaced. R̄ is two counters and needs no
sample buffer. By Little it already includes symbols awaiting repair, so
the deferred hole term and the stall formula may fold into it. That is to
be checked with the pending offline result. The part that grows more
complicated is the idle-leg credit (κ, ν), which is kept out.

**3. Bit-identical clean-ups (pre-merge cleanup; each verified first).**
- `est.rtt()` and Copa `srtt` read the same app-echo stream at gain 1/8
  while `RWM_COPA_WIRE` is off. That makes D1's `max(SRTT_p, EWMA_p)` a
  no-op. Check seeds and idle behaviour, assert equality, keep one.
- `CorrectionDeficit` is computed and read by nothing.
- The deadline code binds a smoothed RTT to a variable named `rtprop`
  (`emit_source.rs` ~570). Rename it only; changing what it reads is a
  behaviour change.

**4. For the user's decision (not done unasked).**
- About a dozen default-off arms still carry tests and register constants.
  Examples: the request law (refuted), the hold-down, the three-term and
  honest caps, Copa's wire mode, and the derived placement arms. ADR-0052
  retires refuted ones in two stages. The generation seat has no
  disposition.
- Copa's legacy controller gates neither intake nor the wire. If its cwnd
  feeds only placement's queue term, `spare_capacity` and the active-path
  filter, then a controller, its hint table (`queue_target_mult`, a
  NO-MODE-SWITCH debt) and three anchor constants exist to supply one
  denominator. The readers are to be grepped first. Its RTprop filter must
  stay.
- One per-leg state read by every law, in place of the duplicate
  estimators.

**5. Not judgeable yet.**
- The rate law: a max of three terms through a soft saturation and a grid
  search. r ≈ 0 on every measured cell, so one off-Bulk row with a
  which-term-wins tally comes first.
- Two divergences to verify for §11.3: χ is set only by the perf client (so
  it is 0 on the tunnel), and W* runs one budget at every δ while the paper
  tabulates per-hint budgets.
- The recovery timeouts wait until the store law settles H.
- The paper's `RWM_RSTAR_TAIL` "on by default" and status §1's on-list
  disagree; reconcile them.


### 44. Erratum — the residence-time form is refuted (offline, `s43res`)

§44 item 2 ("the store-law candidate simplifies") is withdrawn.
- **It rests on a false premise.** On the p27/p34 static rows, residence
  time R̄ = occupancy ÷ delivered rate matches Tp50 within 1–3 % at c2 and
  ±5 % at c1s. Little's law holds in the windows.
- **The undershoot is the brake itself.** The 20–25 % gap below the knee is
  the brake Tmin − RTprop, not the time symbols wait in the store. At c2,
  with no queue, the 250 ms window minimum sits 1.7–2.5 ms above the 10 s
  minimum under netem jitter, so the law reads jitter as a standing queue.
- **D = L never occurs.** The maximum D/L is 0.984 at c2 (loss and repair)
  and 0.85–0.95 at c1s.
- **Little movement, more noise.** With a gain g, the form moves c2 only
  from 164 to 171 at g = 1.1. At c1s it is ill-conditioned, and its
  predicted spread is 0.32–0.41 against the registered form's 0.11.
- **What the user was told is withdrawn.** "g = 1 is the knee, and getting
  both costs only the jitter" does not hold.

The registered form already carries the continuous parameter: q(δ). It
stops at 0.10 at Bulk, while the knees need ≈ 1.2 (c2, c8) and 1.2–1.3
(c1s). Pending, offline:
- Debias the brake by its no-queue value, so both minima sit on one
  horizon. If c2 and c8 then land in their knee bands, the miss is
  estimator bias and q stays as it is.
- Run the registered form at 1.2 against the expired anchor. That shows
  the anchor margin a larger gain costs.


*The debias result (offline, `docs/l1-raw/s42/s43gain.out`; registered
form, p27 static rows).*

| variant | c2 fresh | c2 expired | c2 stuck-high threshold | c8 fresh / expired (thin) |
|---|---|---|---|---|
| registered, 1.1 | 164, 0.887 Gmax, Tp50 17.8 ms | 157 | 927 | 324 / 287 |
| brake offset b₀ = 2.47 ms added, 1.1 | **195 (slope 0.08), 0.985 Gmax, Tp50 19.5 ms** | 196 | **648** | 413 / 346 |
| gain 1.2 | 184, 0.957 Gmax | 179 | 504 | 399 / 330 |

- At c2 the undershoot was estimator bias: with the no-queue brake offset
  added back, the law lands on the knee at 0.985 of Gmax, with Tp50 at
  19.5 ms against 40–100 ms on main. So "both" is reachable at c2 on the
  offline map. q stays as it is.
- **The price is anchor margin:** the stuck-high threshold falls from 927 to
  648.
- **b₀ has no engine source.** It is an oracle constant read off this
  ledger's smallest caps. "Both minima on one horizon", taken literally,
  removes the brake, which is the 250 ms-minimum case with no fixed point.
  A source for b₀ is the open design item.
- c8 is thin and c1s is unread (truncated windows). These are offline maps,
  not closed-loop runs.
