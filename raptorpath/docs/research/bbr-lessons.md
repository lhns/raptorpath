# What raptorpath's congestion control can learn from BBR

Research note. Scope: **research + written deliverable only** — no production
code changes, no VM runs. Model-changing proposals are flagged **paper-first**
per project convention (`fec-arq-model.md` is the model of record).

Status of claims: every number attributed to raptorpath is **measured** and
cited to `docs/goal-gate.md` (L0 gate / L1 netem / L2 real-stack). Every BBR
number is cited to a primary source (BBR ACM Queue paper, the IETF BBR draft,
BBRv3 IETF material). Everything else is explicitly labelled **hypothesis**.

---

## 0. TL;DR — ranked proposals and the FEC-coupling verdict

Our measured, protocol-owned CC deficiency (goal-gate P9b / L1-convergence):
a **residual ~30% Copa backoff rate** at C2 under netem jitter that pins
`cwnd` p50 at **~80–110 symbols vs BDP ~160**, plus a slow additive `+2/SRTT`
recovery after each backoff. Honest bound: post-P9b the gate binds only
**8–24% of ACKs**, and the dominant C2 completion gap vs quinn (5.5×) is the
*tunnel pipeline + inner-TCP slow-start*, **not** CC. So CC work has bounded
leverage on the C2 1.8 MB headline, but is the **largest protocol-owned term**
after the structural one and directly governs sustained-throughput cells
(C7/C8 aggregation via `B_eff`, bulk goodput, large-BDP fill).

| # | Proposal | BBR idea | Attacks (measured) | Class |
|---|----------|----------|--------------------|-------|
| 1 | **Promote `max_bw` to an active BtlBw anchor** — explicit BDP target + BtlBw-anchored recovery instead of `+2/SRTT` crawl | `BtlBw` max-filter, `cwnd = cwnd_gain × BtlBw × RTprop` | cwnd 80–110 vs BDP 160; slow post-backoff recovery | paper-first (light: §12.6 note) |
| 2 | **Decouple pacing rate from cwnd** — pace at `gain × BtlBw`, demote cwnd to a cap | pacing = primary control, cwnd = secondary cap | 30%-backoff → throughput coupling | paper-first |
| 3 | **ECN as an explicit bounded congestion signal** | BBRv2/v3 ECN mark-rate response | root cause of the 30% backoff (jitter≠queue on netem) | tuning + plumbing |
| 4 | **"Probe, don't collapse"** — continuous ProbeBW-style oscillation replacing one-sided ×0.92 | ProbeBW 1.25/0.75 cruise around 1.0 | per-backoff cost of the residual 30% | paper-first |
| 5 | **Continuous `inflight_hi`/`inflight_lo` headroom caps** | BBRv2/v3 inflight bounds | overshoot safety; bridge to FEC coupling | paper-first |

**FEC-coupled-CC verdict:** a **genuinely novel and defensible** direction,
but it is a *quantification of an existing exception*, not virgin territory.
RFC 9265 already states the rule ("a recovered packet must be considered a
lost packet [for CC]") **and its exception** ("this does not apply to the
usage of FEC on a path that is known to be lossy"). raptorpath's contribution
would be to make "known to be lossy" **continuous and measured**: bound CC
loss-blindness by the FEC's *current recovery headroom* (`r` provisioned vs
`ε̂` measured). Worth deriving and measuring; **do not ship or claim novelty
until derived (paper) and measured (gate + L1)** — and note our own P10a
result is a cautionary precedent (a well-motivated CC/FEC coupling that
measurement **refuted**).

---

## 1. BBR's model, precisely (cited)

BBR (Cardwell, Cheng, Gunn, Yeganeh, Jacobson) controls a flow from an
explicit two-parameter model of the path rather than from loss events. All
values below are from the BBR ACM Queue / CACM paper unless noted
[BBR-Queue].

### 1.1 The two estimated quantities

- **BtlBw (bottleneck bandwidth)** = a **max-filter** over the per-ACK
  *delivery-rate* samples, windowed over the **last ~6–10 RTTs**. Delivery
  rate for an ACK = (data delivered) / (elapsed time) since the ACK'd packet
  was sent; BBR takes the **windowed maximum** because the max is the only
  sample not corrupted by receiver/ACK-path delays [BBR-Queue].
- **RTprop (round-trip propagation delay)** = a **min-filter** over RTT
  samples over the **last ~10 s** [BBR-Queue]. The min strips queuing delay.

These two can never be measured simultaneously (probing bandwidth fills the
queue → inflates RTT; probing RTprop drains the queue → hides bandwidth), so
BBR **sequentially probes** them at two timescales.

### 1.2 Control law — pacing is primary, cwnd is a cap

BBR's **primary** control is the **pacing rate**; it paces *every* packet at
`pacing_gain × BtlBw` to match the bottleneck departure rate [BBR-Queue,
CACM]. `cwnd` is a **secondary** parameter: `cwnd = cwnd_gain × BDP` where
`BDP = BtlBw × RTprop`. In ProbeBW `cwnd_gain = 2`, chosen so BBR keeps
sending smoothly at the estimated rate even when ACKs are delayed up to one
RTT; once the pipe is full this bounds the standing queue to
`(cwnd_gain − 1) × BDP = 1 × BDP` [CACM]. **This decoupling is the crux for
us** (see §2): a transient `cwnd` dip does not throttle the wire, because the
wire rate is set by `BtlBw`, not by `cwnd/SRTT`.

### 1.3 The state machine and gains

| State | pacing_gain | cwnd_gain | Purpose / exit |
|-------|------------:|----------:|----------------|
| **Startup** | `2/ln2 ≈ 2.89` | `2/ln2` | Exponential fill (doubles rate ≈ every RTT); exit when BtlBw plateaus (< 25% growth over 3 RTTs) |
| **Drain** | `ln2/2 ≈ 0.35` | `2/ln2` | Drain the queue Startup created; exit when inflight ≈ BDP |
| **ProbeBW** | cycle **1.25, 0.75, 1,1,1,1,1,1** (one gain per RTprop) | 2 | Steady state; 1.25 probes for more BW, 0.75 drains the resulting queue, 1×6 cruises |
| **ProbeRTT** | 1 | — (cwnd = **4 packets**) | Entered if RTprop not re-sampled in **10 s**; drain to 4 packets for **≥200 ms** to re-measure RTprop, then resume |

The Startup gain `2/ln2` is the smallest constant that doubles the sending
rate each round given exponential-in-the-gain growth. ProbeRTT is BBR's only
"hard" drain and costs a visible throughput dip; it is bounded to
< ~2% of runtime by the 10 s interval [BBR-Queue].

### 1.4 BBRv2 / v3 additions relevant to us

BBRv1 is purely delay/rate driven and treats loss as essentially a soft
signal. **BBRv2** added explicit **loss and ECN response bounds**, and
restructured ProbeBW into four subphases
**DOWN → CRUISE → REFILL → UP** [BBR-Draft]:

- **`inflight_hi`** — the *long-term* max inflight the algorithm believes
  yields "acceptable queue pressure," set from loss/ECN in the current or
  previous probe cycle. When a probe volume produces a loss rate above the
  objective, BBR sets `inflight_hi` to that volume [BBR-Draft].
- **`inflight_lo`** — the *short-term* safe inflight for the current probe
  cycle; reduced (with `bw_lo`) on concrete loss during CRUISE [BBR-Draft].
- **`BBRLossThresh`** — the **per-round loss rate tolerated while probing,
  default 2%** [BBR-Draft]. ProbeBW_UP ends and drains when the round loss
  rate exceeds this (among other conditions). i.e. loss is a **bounded**
  signal: below 2%/round BBR keeps probing; above it, it caps inflight.

**BBRv3** fixed a v2 bug where a single loss/ECN event prematurely froze
`inflight_hi` (a circular BtlBw↔inflight dependency): v3 **probes
persistently** until loss *or* ECN-mark rate exceeds a tolerance (~1%),
requires **6 (not 8) packets lost in a round** to exit on loss, and improves
coexistence/fairness with CUBIC (reported Jain index ≈ 0.95) [BBRv3-Slides,
BBRv2v3-Eval]. The takeaway for us: **modern BBR is explicitly, and only
*boundedly*, loss-tolerant** — it still treats sustained > ~1–2% loss as
congestion. That bound is exactly where raptorpath's FEC can, in principle,
buy more room (§6).

---

## 2. Copa-lite vs BBR: our CC, and a deficiency→mechanism map

### 2.1 What we run (from `src/scheduler/mod.rs`)

"Copa-lite," delay-based, **no phases**:
- Propagation floor = **min RTT over a ~10 s window** (`min_rtt`).
- Queue signal `dq` = *windowed-min RTT since last update* − a **P10 quantile
  of window-min history** (jitter-robust floor, P9b), clamped ≥ 0.1 ms.
- Backoff when `dq > (queue_mult − 1)×floor + 2×jitter_est`, with
  `queue_mult` = 1.08 / 1.125 / 1.25 (Realtime/Auto/Bulk).
- **Two-speed ramp:** `×1.5+1` per SRTT until the first backoff, then
  `+2 / ×0.92`.
- **Rate = cwnd/SRTT** (token-bucket pacing, burst `max(10, cwnd/8)`),
  `cwnd` floor 8. `max_bw` (a delivery-rate max) **is tracked but only feeds
  a diagnostic** `copa_target_cwnd()` — it is **not** an active control input.

Note the project history: an **earlier** design (ADR-0019/0024) *was* BBR-shaped
— `cwnd = gain × max_bw × min_rtt` with a ProbeRTT drain to `cwnd=4`. It was
replaced by the ramp/backoff Copa-lite that won the L0 gate, and paper §12.3
records the reason (ProbeRTT punches a **200 ms hole in the FEC taper**). So
several "adopt from BBR" ideas below are partial *re-introductions* of
machinery we removed — that history is an asset (the estimators still exist),
and a warning (know why we removed it).

### 2.2 Deficiency → BBR mechanism

| Measured deficiency (goal-gate) | Why Copa-lite has it | BBR mechanism that addresses it |
|---|---|---|
| **cwnd 80–110 vs BDP ~160** (P9b) | No explicit BW target; after a ×0.92 backoff, recovery is `+2/SRTT` — dozens of SRTTs to climb back | Explicit `BtlBw × cwnd_gain × RTprop` **target**: BBR returns to the measured operating point, it doesn't crawl |
| **~30% residual backoff rate at C2** (P9b) | Delay signal still partly measures **jitter, not queue** on netem — the P9b quantile+headroom mitigations cut 60%→30% but can't fully separate correlated jitter from queue | (a) **ProbeBW gentle probe** (self-correcting oscillation, not a one-sided cut); (b) **ECN** — a *direct* queue signal with no jitter-disambiguation problem |
| **Backoff *costs throughput*** (30% × 8% each, slow recovery) | `rate = cwnd/SRTT` — a cwnd dip **is** a rate dip | **Pacing decoupled from cwnd**: wire rate = `BtlBw`; cwnd is only an inflight cap, so a transient cwnd dip doesn't throttle the wire |
| **Loses to BBR at low-rate C3** (stream p99 198 ms vs rp-bulk 569 ms; 1.8 MB 1.00 s vs 9.61 s) | Mostly **tunnel block latency at 20 Mbit**, per goal-gate — *not* primarily CC (be honest); the CC-attributable part is slow pipe-fill/recovery at a large BDP | BtlBw anchor (fills any BDP in ~log2 RTTs); but see §5 honesty caveat — C3 is not a CC-limited cell |
| **Realtime tail issues** (C2 p99 513 ms; C3/C5 silent fails) | Window-mode reactive repair + near-empty queue target; not a BtlBw problem | Minimal BBR leverage; this is FEC/repair-path work, out of scope here |

---

## 3. Ranked proposals

Each: the BBR idea → mapping to our scheduler → expected leverage against a
**measured** number → **paper-first vs tuning** → risk → philosophy/continuity
tension.

### Proposal 1 — Promote `max_bw` to an active BtlBw anchor (highest leverage)

**BBR idea.** `BtlBw` = windowed-max delivery rate; `cwnd = cwnd_gain ×
BtlBw × RTprop` is a *target*, and steady-state recovery returns *to* it.

**Mapping.** We already maintain `max_bw` (a delivery-rate max-filter) and
`min_rtt`; `copa_target_cwnd()` already computes an equilibrium — today it is
**diagnostic only**. Change: after a backoff, replace the additive `+2/SRTT`
climb with an increase that **converges toward** `T = cwnd_gain × max_bw ×
min_rtt` (e.g. `cwnd += max(2, α·(T − cwnd))`, continuous, `α` small), and
use `T` (not the raw ramp) as the steady-state attractor. Keep `×1.5+1`
Startup and the delay-driven backoff.

**Expected leverage.** Directly targets **cwnd 80–110 → toward BDP 160** and
the slow post-backoff recovery — i.e. it converts the residual-30%-backoff
world from "each backoff costs many SRTTs" to "each backoff costs a few."
Largest effect on **sustained-throughput cells** (C7/C8 aggregation reads
`B_eff` from `cwnd/SRTT`; bulk goodput). **Bounded** on the C2 1.8 MB
headline (gate binds 8–24% of ACKs; structural term dominates).

**Class.** **Paper-first (light).** It changes the cwnd law, but stays inside
the CC section the paper already declares "CC-agnostic"; needs a short §12.6
formalizing the BtlBw anchor and confirming it does **not** reopen a taper
gap (it doesn't — no drain phase). Not a core-model (FEC/ARQ) change.

**Risk (real).** BBR's `BtlBw` is only trustworthy because of **app-limited
detection** and **per-packet delivery-rate sampling** (RFC-draft delivery
rate estimation): samples taken while the sender is application-limited
*underestimate* BtlBw and must be **discarded**. Our `record_delivery()`
divides ACK-batch counts by wall-time — coarse, and it has **no app-limited
flag**. For a 1.8 MB object with warm-up, `max_bw` will read low exactly when
we want it high. **Mitigation:** implement app-limited marking before trusting
the anchor, or use it only as a *floor* on the ramp (never a cap) so an
underestimate can't hurt. This risk is why it's #1 in leverage but gated on
delivery-rate accounting we do not yet have.

### Proposal 2 — Decouple pacing rate from cwnd

**BBR idea.** Pacing rate = `pacing_gain × BtlBw` is primary; `cwnd =
cwnd_gain × BDP` is a secondary *cap* for ACK-delay/pathology safety.

**Mapping.** Today `pace_refill()` sets `rate = cwnd/SRTT`. Change: set
`rate = gain × max_bw` (with `max_bw` from Prop 1), and keep `cwnd` as the
**inflight cap** only. A transient ×0.92 `cwnd` dip on a jitter false-positive
then reduces headroom but **not** the wire rate.

**Expected leverage.** Neutralizes the **throughput cost** of the residual
30% backoffs — the backoff still happens, but it stops translating 1:1 into
rate loss. Complementary to Prop 1 (Prop 1 fixes recovery speed; Prop 2 stops
the dip mattering).

**Class.** **Paper-first.** Rewrites the rate law (`§12.4` rate formula) —
this is the CC change the paper's §12.3 "Copa vs BBR" table gestures at but
never specifies. Depends on Prop 1's BtlBw being trustworthy.

**Risk.** If the backoff was *correct* (a real standing queue), pacing at a
stale `BtlBw` **overshoots and builds queue** — precisely the pathology
delay-based CC exists to avoid. Requires `BtlBw` to also decay on sustained
delay (BBRv2's `bw_lo`). Without that coupling, decoupling is dangerous.

**Continuity tension.** BBR's `pacing_gain` is a **discrete 8-phase cycle** —
against our "closed-form, no hard cutoffs" philosophy. **Continuous analogue:**
drive a continuous `pacing_gain(dq)` from our existing Copa queue signal —
e.g. `gain = clamp(1 + κ(dq* − dq)/dq*, 0.75, 1.25)` — so the gain *is* the
Copa response, smooth, with no phase machine. This is arguably a cleaner Copa
than ours and a cleaner BBR than BBR.

### Proposal 3 — ECN as an explicit, bounded congestion signal

**BBR idea.** BBRv2/v3 treat ECN marks as a **direct** congestion signal with
a bounded response (mark-rate threshold ~1%, caps `inflight`) [BBR-Draft,
BBRv3-Slides].

**Mapping.** The entire P9b saga (quantile floor, dual jitter estimators,
`k×jitter` headroom) exists because our delay signal **cannot cleanly
separate jitter from queue** on netem — the *measured* root cause of the
30% backoffs. ECN sidesteps disambiguation: a mark is an unambiguous "queue
here" from the bottleneck. We could feed an ECN-mark-rate EWMA as a second
backoff trigger (`mark_rate > θ`), continuous, alongside the delay signal.

**Expected leverage.** Attacks the **root cause** of the 30% backoff rather
than its symptoms — on ECN-capable paths the jitter false-positive problem
largely disappears, which is the one deficiency the delay-signal engineering
could only *halve*.

**Class.** **Tuning + plumbing** (no core-model change): the response can be
the same continuous backoff we already have, just a cleaner input. But it
needs **ECN plumbing on QUIC datagrams** end-to-end, and it only helps where
the path (and both endpoints) support ECN — many wireless middleboxes bleach
ECN. So: high value where available, **conditional**.

**Risk.** ECN unavailability / bleaching → silent no-op (acceptable: falls
back to delay). Over-trusting a shallow-buffer AQM's marks could under-utilize
— bound the response (BBRv2 style) rather than treating a mark like a loss.

### Proposal 4 — "Probe, don't collapse": continuous ProbeBW oscillation

**BBR idea.** ProbeBW never "backs off" on delay; it **oscillates gently**
(pacing_gain 1.25 then 0.75 around 1.0) and returns to the operating point
by construction.

**Mapping.** Replace the **one-sided** `×0.92` multiplicative backoff with a
symmetric bounded oscillation around the BtlBw operating point: probe up a
little, drain a little, cruise — the *continuous* pacing_gain of Prop 2 already
delivers this if we let the gain go **above** 1.0 to probe, not just at/below.

**Expected leverage.** Reduces the **per-event cost** of the residual 30%:
an oscillation that returns to `1.0×BtlBw` by construction is self-correcting,
where our ×0.92 needs `+2/SRTT` to undo. Bounded (8–24% of ACKs).

**Class.** **Paper-first** (changes the backoff law). Largely **subsumed by
Props 1+2** — list it as the *design intent* those two should realize, not a
third independent mechanism.

**Continuity tension.** ProbeBW is BBR's most phase-machine-like part
(discrete gains, one per RTprop). The honest continuous analogue is exactly
Prop 2's `pacing_gain(dq)` — so #4 is a framing, not new machinery.

### Proposal 5 — Continuous `inflight_hi` / `inflight_lo` headroom caps

**BBR idea.** BBRv2/v3 keep a long-term `inflight_hi` and short-term
`inflight_lo` cap on inflight, set from loss/ECN, to bound overshoot
independently of the rate estimate [BBR-Draft].

**Mapping.** We have `cwnd` (one cap) and a spare-capacity gate. Adding a
*slow* `inflight_hi` (long-horizon max safe inflight) and a *fast*
`inflight_lo` gives a principled two-timescale safety envelope — and, crucially,
it is the **natural home for the FEC coupling** (§6): let the *loss objective*
that sets `inflight_hi` be **`r`-aware** (raise the tolerated loss rate by the
FEC recovery headroom).

**Expected leverage.** Small on today's numbers (overshoot isn't our measured
problem — under-utilization is); its value is as the **structural hook** for
FEC-coupled CC. Rank low as a standalone; high as an enabler.

**Class.** **Paper-first** (new state + law). Do it *with* §6 or not at all.

**Risk.** Two more caps interacting with `cwnd` and the spare-capacity gate =
more edge cases (the exact complexity ADR-0019 cited when rejecting "full
BBRv2"). Only worth it if the FEC coupling pays off.

---

## 4. Ranked summary (leverage vs measured numbers)

1. **Prop 1 (BtlBw anchor).** Leverage: `cwnd 80–110 → ~160`, fast recovery;
   biggest on C7/C8 aggregation + bulk goodput; **gated on app-limited
   delivery-rate accounting** we lack.
2. **Prop 2 (decouple pacing from cwnd).** Leverage: removes the *throughput
   cost* of the residual 30% backoffs; needs Prop 1's BtlBw + a `bw_lo` decay
   for safety.
3. **Prop 3 (ECN).** Leverage: attacks the *root cause* (jitter≠queue) the
   delay signal could only halve; conditional on ECN availability; cheapest
   (tuning, not model).

(Props 4–5 are framing/enabler, folded into 1–2 and §6.)

---

## 5. Honesty: where BBR does *not* fit, and where CC isn't the problem

- **C3 is not a CC-limited cell.** goal-gate: "at 20 Mbit the tunnel's block
  latency dominates." BBR beats us at C3 on the *tail*, but the fix is
  block-latency/pipeline, not the congestion controller. Do not sell CC work
  as the C3 lever.
- **The C2 headline gap is structural.** L2 native-object and warm-flow tests
  both isolated the 5.5× to **tunnel pipeline + inner-TCP slow-start**, with
  the Copa ceiling binding only 8–24% of ACKs. CC proposals here are
  *second-order* on the C2 1.8 MB completion by our own measurement — their
  real payoff is sustained throughput and multipath aggregation.
- **Phase machines vs closed form.** BBR's Startup/Drain/ProbeBW/ProbeRTT are
  **discrete states with hard transitions** — against our no-hard-cutoff
  convention. Every adoption above is deliberately recast as a **continuous**
  analogue (BtlBw as an attractor, `pacing_gain(dq)` as a smooth function).
  Where a BBR mechanism *only* works as a discrete phase, we should not adopt
  it (next bullet).

### What NOT to adopt from BBR, and why

1. **ProbeRTT (forced drain to `cwnd=4` for 200 ms).** **Reject.** This is
   already project-decided: paper §12.3 shows ProbeRTT punches a **200 ms hole
   in the FEC taper** — source sent during the drain gets almost no protection,
   and a GE burst during the hole is unrecoverable. It's a discrete drain with
   no continuous analogue that preserves taper coverage. Our 10 s windowed-min
   floor + natural oscillation is the right call; if min-RTT staleness is ever
   *measured* on a persistently-loaded bulk path, prefer a **gentle continuous
   re-probe** (a small `pacing_gain < 1` dip, never `cwnd=4`), not ProbeRTT.
2. **The 8-phase ProbeBW cycle as literal state.** **Reject the machine, keep
   the intent.** Adopt the *behavior* (gentle probe/drain around the operating
   point) as a continuous `pacing_gain(dq)`; don't import discrete per-RTprop
   phase counters and their edge cases (ADR-0019 rejected "full BBRv2" for
   exactly this complexity).
3. **BBR's loss-as-2%-soft-cap as our loss policy.** **Reject as-is; it's the
   *floor* we want to beat.** BBRLossThresh (2%/round) is BBR *hedging* because
   it has no real recovery mechanism. raptorpath *actually recovers* loss via
   FEC, so a fixed 2% cap under-uses our capability. The right move is §6
   (couple the tolerated loss to FEC headroom), not to copy a constant BBR
   itself admits is a compromise.
4. **BBR's cwnd-as-afterthought on our *inner-flow* geometry.** BBR assumes it
   *is* the transport. We carry an inner TCP in the tunnel; blindly letting the
   wire run at `BtlBw` while the inner flow slow-starts can *decouple* our
   pacing from the payload's actual demand (P10a-style dilution). Any BtlBw
   pacing must stay ack-clocked to real demand, not free-run.

---

## 6. Candidate novel direction: FEC-recovery-headroom-bounded CC loss-blindness

**The thesis.** BBR is loss-tolerant *by design* but still boundedly so
(BBRLossThresh 2%/round; BBRv3 ~1%): it hedges because it cannot actually
recover loss. raptorpath **can** — FEC provisioned at rate `r` recovers loss
up to a coverage limit. So raptorpath can safely be **more loss-blind than
BBR, up to that limit**. The proposal: make CC's tolerated loss rate a
**continuous function of current FEC recovery headroom**.

**Sketch (hypothesis, needs derivation).** Let `ε̂` = measured loss, `r` =
current provisioned correction rate (paper §13.4, `r = ε/(1−ε)`), and let the
FEC's residual post-recovery loss be `P_resid(ε̂, r)` (the §14.14
burst-marginalized decode-failure probability the model already computes).
Define **recovery headroom** `H = r − r_needed(ε̂)`. Then let CC's loss
tolerance scale with `H`:

```
  loss_tolerated(H) = θ_base + (θ_max − θ_base) · g(H)
```

with `g` continuous, `g(0)=0` (no headroom → behave like BBR, treat loss as
congestion), `g→1` as headroom grows (plenty of FEC → ignore loss up to the
coverage limit). This *continuously* interpolates between BBR's 2% cap and
"fully loss-blind," indexed by how much loss the FEC is *actually* recovering
right now. It plugs directly into **Prop 5's `inflight_hi`** (the loss
objective that sets `inflight_hi` becomes `loss_tolerated(H)` instead of a
constant 2%).

**Why it's defensible novelty — and its precise prior art.** RFC 9265 is the
authoritative statement of the FEC/CC interaction [RFC9265]:
- Research Recommendation 1: *"a recovered packet must be considered as a lost
  packet [for congestion control]. This does not apply to the usage of FEC on
  a path that is known to be lossy."*
- *"FEC coding mechanisms should not hide congestion signals"*; blindly
  applying FEC "may easily lead to an implementation that also hides a
  congestion signal," which "can drastically reduce the goodput of non-coded
  flows."
- It notes delay-based CC is the robust choice when FEC hides losses (which is
  already raptorpath's §12.1 thesis).

So the *rule* and its *exception* already exist. raptorpath's contribution is
to turn RFC 9265's binary, operator-asserted exception ("a path *known* to be
lossy") into a **measured, continuous, self-limiting** quantity: `H` is
observed, not asserted, and `g(H)→0` when headroom vanishes so the flow
**re-becomes a good citizen exactly when it stops actually recovering loss**.
That directly answers RFC 9265's fairness objection: we only ignore the loss
we are *demonstrably repairing*, and we ignore it via a **delay-based** CC
(so a real queue still backs us off regardless of `H`). This is a genuinely
new, publishable framing — but it is *positioned against* RFC 9265, not
claimed as first-of-kind FEC+CC work.

**Caveats (mandatory before any claim).**
- **Derive first (paper).** Needs a new section (e.g. §12.6 / §14.30):
  definition of `H`, the `g(H)` shape, the interaction with the delay backoff
  (loss-blindness must **never** override a genuine standing-queue signal —
  delay remains the hard safety), and a fairness argument (bounded by `H`,
  so self-limiting).
- **Then measure (gate + L1).** And treat P10a as precedent: the inner-feedback
  repair floor was equally well-motivated and **measurement refuted it** (flat
  at C2, −28% at C3) because repairs *displaced source* in a closed loop. A
  loss-tolerance knob that lets cwnd run hotter could over-drive the same loop.
  **No claim until the gate and L1 show a win.**
- **Coverage-limit safety.** The whole scheme is valid only *below* the FEC
  coverage limit; past it, `H→0` must clamp tolerance to the BBR-like base so
  we degrade to a conventional loss-aware flow, not a congestion collapse.

**Verdict:** pursue as a **paper-first candidate**, ranked above Prop 5
(which is its implementation vehicle) but below Props 1–3 in
*near-term measured leverage*, because unlike 1–3 it has **no measured
evidence yet** and one adjacent idea (P10a) already failed. It is the most
*novel* item here and the least *proven* — keep those two facts separate.

---

## Sources

BBR primary:
- Cardwell, Cheng, Gunn, Yeganeh, Jacobson. "BBR: Congestion-Based Congestion
  Control." ACM Queue 14(5), 2016 / CACM 60(2), 2017.
  https://queue.acm.org/detail.cfm?id=3022184 ,
  https://cacm.acm.org/practice/bbr-congestion-based-congestion-control/
  [BBR-Queue, CACM]
- IETF, "BBR Congestion Control," draft-ietf-ccwg-bbr / draft-cardwell-iccrg-
  bbr-congestion-control (BBRv2/v3 unified spec: `inflight_hi/lo`,
  `BBRLossThresh` 2%, ProbeBW DOWN/CRUISE/REFILL/UP).
  https://datatracker.ietf.org/doc/draft-ietf-ccwg-bbr/ ,
  https://www.ietf.org/archive/id/draft-cardwell-iccrg-bbr-congestion-control-01.html
  [BBR-Draft]
- BBRv3: Algorithm Overview and Google's Public Internet Deployment, IETF 119
  CCWG. https://datatracker.ietf.org/meeting/119/materials/slides-119-ccwg-bbrv3-overview-and-google-deployment-00
  [BBRv3-Slides]
- Performance/behavior evaluations of BBRv2/v3 (loss/ECN thresholds, fairness):
  MDPI Appl. Sci. 14(12):5053; IFIP Networking 2025.
  https://www.mdpi.com/2076-3417/14/12/5053 [BBRv2v3-Eval]

FEC + congestion control:
- RFC 9265, "Forward Erasure Correction (FEC) Coding and Congestion Control in
  Transport." https://datatracker.ietf.org/doc/rfc9265/ [RFC9265]

raptorpath internal:
- `docs/goal-gate.md` — all measured L0/L1/L2 numbers.
- `docs/fec-arq-model.md` §12 (Congestion Control Integration), §12.3 (Copa vs
  BBR), §12.4 (Copa rate formula), §14.14/§14.21/§14.28.
- `src/scheduler/mod.rs` — CopaState / PathState / on_ack / pacing.
- ADR-0019 (BBR delay-based CC, superseded), ADR-0024 (ProbeRTT, superseded).
