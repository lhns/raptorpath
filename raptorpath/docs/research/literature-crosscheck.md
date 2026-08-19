# Formula Cross-Check — every load-bearing formula against the published literature

**A desk-research review, not a decision record.** Companion to
`literature-map.md`, which did this for the arc's *findings*; this document does
it for the *expressions*. Nothing here is an ADR: no decision is taken, no
default moves, no ADR-0071 candidate is picked, ranked or recommended.

For each load-bearing formula in the tree — the shipped laws, the refuted ones,
and the ADR-0071 candidates — this puts **our expression** next to **the
published counterpart, quoted verbatim with its citation**, and records
AGREE / DIVERGE / NO-COUNTERPART plus what the divergence implies. Where the
literature *settles* a question we have been deriving from scratch, it says so;
where our measurement *contradicts* the literature, it says that too, and does
not resolve it in either direction.

**Date**: 2026-08-19
**Branch**: `docs/literature-crosscheck` from main@`62f078b`. **DOCS ONLY.** No
VM was contacted, no L1 number re-derived, no benchmark run, no engine line
touched.
**Paper pointer**: §16.65. **Related**: ADR-0070 (the provenance trial),
ADR-0071 (the candidates — **not adjudicated here**), Appendix B.

> **The one part of this document that is decision-shaped**, and the only part
> that asks anything of anybody: **three claims in the tree are stronger than
> their sources support** — `9/8` as "cited not fitted", `gain = 2.0`'s
> recovery-runway rationale, and any citation of the MPTCP literature for our
> span decomposition. Tier 0 of the prioritized list corrects all three, at
> zero measurement cost. If the user wants those corrections recorded as a
> decision rather than as a finding, they are the natural content of a short
> ADR-0072; this document deliberately does not presume that.

---

## Why this document exists

ADR-0070 found the shipped cap law to be a fitted constant wearing a law's
clothes. ADR-0071 enumerated successors and deliberately took no decision.
Between them the tree now carries roughly a dozen expressions whose
derivations are either absent, fossilised, or freshly invented. The observation
that produced this review is simple and it is the user's:

> **Many of these questions have published answers. Confirming or diverging
> from a published formula is faster than re-deriving one.**

That turns out to be true to an uncomfortable degree. Six of the ten transport
formulas below have exact published counterparts; two of them are *the same
number reached by a different argument*; one of our "cited, not fitted"
constants is, in its own source, explicitly an empirical recommendation; and
the single most consequential result — a derived setpoint for the standing
queue — has been sitting in an IETF RFC since 2018 and predicts our §16.57
measurement in advance.

**The cross-domain half was the user's second instruction, and it earned its
place.** The slack term is a newsvendor problem with a degenerate cost ratio,
and operations research has the closed form for exactly when the optimal
reserve is zero. The resequencing span is a reorder-buffer sizing problem. The
dead wall is a textbook metastable failure. The pooled-vs-per-path decision
ADR-0058 reached empirically was published as a theorem in 1979.

---

## METHOD, AND WHAT IS AND IS NOT VERIFIED

Discipline applied, because a literature cross-check that misquotes is worse
than none:

1. **Every quotation below was fetched from a primary source in this session**
   unless explicitly marked otherwise. RFCs were fetched from
   `rfc-editor.org` as plain text and quoted from the retrieved file; papers
   were fetched as PDF and converted locally.
2. **Anything I could not verify verbatim is marked `[UNVERIFIED]`** with the
   reason, and is never used to support a verdict. A verdict resting on an
   unverified quote is labelled as such.
3. **Secondary sources are labelled as secondary.** Where only a teaching
   source or encyclopedia entry reproduces a classical formula, the formula is
   given with that provenance and the primary citation is recorded separately
   as *un-consulted*, not as *consulted*.
4. **Where the published constant differs from the commonly-repeated version
   of it, the primary source wins and the discrepancy is flagged.** This
   happened twice and both times it mattered.
5. **Citations are recorded in full, paper-ready form** in the References
   section, with URLs, so §16.65 and Appendix B can cite them directly without
   re-deriving the bibliography.

**Honest limitation.** The desk research was split across six workers; one
reported that PDF retrieval failed in its environment and could recover only
publisher abstracts for the operations-research classics. Every affected item
is marked. The OR section therefore quotes *abstracts and teaching sources*
verbatim and flags the interior theorems as un-consulted. This is a real gap:
Eppen 1979's closed form, Scarf 1960's interior, and Sterman 1989's
oscillation condition are the three worth an institutional pull, and each is
named at the point of use.

**Nine places where the commonly-repeated version of a published constant is
NOT what the primary source says** are collected in "Folklore corrected"
below. Three of them touch our own record.

---

# PART I — THE TRANSPORT CROSS-CHECK

---

## 1. The shipped pool law vs the MPTCP receive/send-buffer lineage

**OURS** (the `RWM_SUM_CAP` form §16.62 published and §16.63 recommended for flip):

```text
cap = clamp( 2 · Σᵢ(bwᵢ · RTTᵢ),  floor,  N · knee )     + span     [per-path RTTᵢ]
```

**THEIRS — RFC 6182 §5.3 "Buffers", verbatim:**

> "In regular, single-path TCP, it is usually recommended to set the receive
> buffer to 2*BDP … **One BDP allows supporting reordering of segments by the
> network. The other BDP allows the connection to continue during fast
> retransmit**: when a segment is fast retransmitted, the receiver must be able
> to store incoming data during one more RTT."

> "The worst-case scenario would be when the subflow with the highest RTT/RTO …
> experiences a timeout … the smallest connection-level receive buffer that
> would be needed to avoid stalling with subflow failures is
> **sum(BW_i)*RTO_max** … This is an order of magnitude more … and is probably
> too expensive for practical purposes. **A more sensible requirement is to
> avoid stalls in the absence of timeouts.** Therefore, the RECOMMENDED receive
> buffer is **2*sum(BW_i)*RTT_max** …"

> "**Send Buffer:** The RECOMMENDED send buffer is the same size as the
> recommended receive buffer, i.e., 2*sum(BW_i)*RTT_max. **This is because the
> sender must locally store the segments sent but unacknowledged by the
> connection level ACK.**"

**Independently, Raiciu et al., NSDI 2012 §4.2, verbatim:**

> "Assuming there are no losses, and no special scheduling at the sender, the
> receive buffer must be at least **∑ xᵢ·RTT_max** … This allows all paths to
> keep sending while waiting for an early packet to be delivered on the slowest
> path. If we want to allow all paths to keep sending while any path is fast
> retransmitting, **the buffer must be doubled: 2 ∑ xᵢ·RTT_max**."

> "A 3G path with a bandwidth of 2 Mbps and 150 ms RTT needs just 75 KB of
> receive-buffer, while a WiFi path running at 8 Mbps with 20 ms RTT needs
> around 40 KB. MPTCP running on the same two paths will need 375 KB — **nearly
> four times the sum of the path BDPs**."

**And Barré, Paasch & Bonaventure, IFIP Networking 2011 §4.1, verbatim:**

> "rbuf = 2 ∗ Σ_{i∈subflows} BW_i ∗ RTT_max"

**VERDICT: AGREE on the ×2 and on the shape — DIVERGE on the clock, and the
divergence is exactly our span term.**

> **THE MAPPING, stated explicitly because the two are not trivially the same
> thing.** Nearly all of this literature sizes a **receive** buffer (receiver
> memory, holding out-of-order data until the in-order frontier catches up).
> Ours is the **sender's** outstanding cap (`sent_store`, holding sent-but-
> unacknowledged data). These are the same Little's-law quantity observed from
> opposite ends of the same in-flight population, and **RFC 6182 makes the
> identification itself** — it recommends the *same expression* for the send
> buffer, with the rationale *"the sender must locally store the segments sent
> but unacknowledged by the connection level ACK,"* which is `sent_store`'s job
> description verbatim. RFC 8684 §3.3.5 states the ordering constraint: *"The
> send buffer MUST, at a minimum, be as big as the receive buffer, to enable
> the sender to reach maximum throughput."* So the transfer is licensed by the
> primary source rather than assumed by us — but every quotation below that
> says "receive buffer" is being read as a sender-side cap on that authority,
> and the reader should hold that in view.

**(a) The published law is also a SEND-buffer law.** Per the mapping note
above. **ADR-0070 finding 2's search for provenance can stop here**:
`2·Σ bwᵢ·RTTᵢ` is one substitution away from an IETF Informational RFC,
reproduced independently by two implementation papers.

**(a′) RFC 8684 §3.3.4 brackets the answer, and the BRACKET IS OUR SPAN
TERM'S JOB.** It gives a lower and an upper bound rather than a formula:

> "**The lower bound for full network utilization is the maximum
> bandwidth-delay product of any one of the paths.** However, this might be
> insufficient when a packet is lost on a slower subflow and needs to be
> retransmitted. **A tight upper bound would be the maximum round-trip time
> (RTT) of any path multiplied by the total bandwidth available across all
> paths.** This permits all subflows to continue at full speed while a packet
> is fast-retransmitted on the maximum RTT path."

The gap between `max_i(BDP_i)` and `RTT_max · Σ bwᵢ` **is** the heterogeneity
allowance — precisely the job our span term does. The standards-track RFC
declines to place a value inside that bracket and says so: *"Determining the
relationship between retransmission strategies and receive buffer sizing is
left for future study."*

**(b) Every published multipath buffer formula uses `RTT_max` OUTSIDE the sum;
none uses per-path `RTTᵢ` inside it.** RFC 6182, RFC 6824/8684, Barré 2011,
Raiciu 2012 and Kuhn 2014 are unanimous.

**(c) The difference IS our span term — and OURS IS HALF THEIRS.** Writing it
out (this decomposition is **ours**, see the honesty note):

```text
2·Σᵢ bwᵢ·RTT_max  −  2·Σᵢ bwᵢ·RTTᵢ  =  2·Σᵢ bwᵢ·(RTT_max − RTTᵢ)
```

At N = 2 the slow leg's term vanishes and this is `2·bw_fast·(RTT_max −
RTT_fast)`. Our span is `2·rate_fast·skew` with `skew = (max − min)/2`, i.e.
`rate_fast·(RTT_max − RTT_min)` — **exactly half** the published quantity.

> **HONESTY NOTE, and it is important.** *No published source writes the
> subtracted form* `Σ bwᵢ·(RTT_max − RTTᵢ)` *as a named resequencing term.*
> This was checked against all five sources above. The decomposition is one
> step of algebra from their formula, but **it is our formulation and must be
> presented as our derivation, never as a quotation.** The paper must not cite
> this literature for the span term's *shape*, only for its *magnitude*.

**(d) They pre-declare our c8L finding, in 2011.** RFC 6182 §5.3:

> "there may be extreme cases where fast, high throughput paths (e.g., 100 Mb/s,
> 10 ms RTT) are used in conjunction with slow paths (e.g., 1 Mb/s, 1000 ms
> RTT). In that case, the required receive buffer would be 12.5 MB, which is
> likely too big. **In extreme cases such as this example, it may be prudent to
> only use some of the fastest available paths for the MPTCP connection,
> potentially using the slow path(s) for backup only.**"

ADR-0071 finding 1 declares c8L MEMORY-STARVED and UNSCORABLE because term 1
alone is 1.83× `WIN_STORE_MAX`. RFC 6182 reaches the same place by the same
arithmetic **and prescribes a response we do not have in our design space:
drop the slow path.** Barré 2011 says the same, operationally:

> "In practice, this dynamic tuning may reach the maximum allowed receive buffer
> configured on the system. **This should be used as a hint to indicate that a
> subflow is underperforming and disable the slowest path.**"

**That is a published architectural answer to the c8L problem the tree
currently treats as a resource-limit embarrassment**, and it converts
`WIN_STORE_MAX`'s bind fraction from a STOP condition into a *signal with a
prescribed action*.

**(e) FOLKLORE CORRECTION — the RFC family contradicts itself.** RFC 8684
§3.3.4 (and RFC 6824 before it) calls the **undoubled** quantity the bound:

> "A tight upper bound would be the maximum round-trip time (RTT) of any path
> multiplied by the total bandwidth available across all paths. This permits all
> subflows to continue at full speed while a packet is fast-retransmitted on
> the maximum RTT path. **Even this might be insufficient** to maintain full
> performance in the event of a retransmit timeout on the maximum RTT path.
> **Determining the relationship between retransmission strategies and receive
> buffer sizing is left for future study.**"

So the standards-track MPTCP RFC (i) omits the ×2 and (ii) **declares the
question we are working on to be open**. Anyone citing "the MPTCP buffer
formula" must say *which* RFC.

**IMPLICATION.** (i) Free: record RFC 6182 §5.3 + Raiciu NSDI'12 §4.2 as the
provenance for the ×2 and the shape — a documentation change discharging half
of ADR-0070 finding 3. (ii) Cheap test: the `Σ bwᵢ·RTT_max` form is a
*magnitude the ladder already sweeps*. (iii) The 2× span discrepancy is
arithmetic, not a battery. **Context differs because** RFC 6182 sizes a buffer
to *never stall*, while our cap is also the sole congestion brake (ADR-0070
finding 7) — generous is harmless for a receive buffer and actively harmful for
a sender-side queue budget. That tension is §4's subject.

---

## 2. The span term vs BLEST, and the "is the buffer binding?" regime question

**OURS:** `span = 2·rate_fast·skew = rate_fast·(RTT_max − RTT_min)`.

**THEIRS — BLEST (Ferlin, Alay, Mehani, Boreli, IFIP Networking 2016 §V),
verbatim from the IFIP proceedings PDF:**

> "rtts = RTT_S / RTT_F"
> "**X = MSS_F · (CWND_F + (rtts − 1)/2) · rtts**"
> "If X·λ > |M| − MSS_S·(inflight_S + 1), the next segment will not be sent on
> S. Instead, the scheduler waits for the faster subflow to become available."

BLEST's own gloss: X estimates the data "that will be sent on F during RTT_S".
To leading order `MSS_F·CWND_F/RTT_F = rate_fast`, so **X ≈ rate_fast ·
RTT_slow**.

**VERDICT: DIVERGE — BLEST charges the FULL slow RTT where we charge only the
DIFFERENCE.** With RFC 6182 and Raiciu, that is **three independent published
sources sizing the reordering term on `RTT_max`, against our `RTT_max −
RTT_min`.**

**And BLEST measured its own analytic estimate to be an OVER-estimate.**
Verbatim:

> "The estimate of X, however, can be inaccurate at times. To address this, **we
> introduce a correction factor λ, to scale X.** λ is adjusted as follows.
> HoL-blocking during one RTT_F is an event that triggers an increase of λ; the
> absence of HoL-blocking triggers a decrease … In the beginning of the
> connection we set λ = 1.0, i.e., no correction of the estimation."

> "**λ is corrected to lower values than its initial setting of 1.0**, because
> the model does not incorporate losses."

**This is the literature confirming our wire result from the opposite
direction, and it is the most useful thing in this section.** §16.63 measured
the `×N` deletion under-funding c8's span by **45.4 %** with goodput going
**UP** at both seeds, concluding *"the span … was not load-bearing at c8 in
this era."* BLEST independently found its analytically-derived blocking
estimate over-provisions in practice and shipped **a measured multiplicative
correction that converges below 1.0**. Two systems, two derivations, same
finding: the closed-form span estimate is too big, and the honest fix is to
scale it by something measured.

**AND THE LITERATURE PREDICTS OUR c7 RESULT EXACTLY.** Raiciu et al., NSDI 2012
§4.2, verbatim:

> "**For equal delay paths, MPTCP's receiver memory consumption is also close to
> zero.**"

§16.57 measured our span term reading **identically 0.000 in all 340
single-path evaluations**, and the three-term pre-registration's sharpest
prediction was that **c7's span term is zero at N = 2 because c7's two paths
are identical** — measured 0.0000 over every rep (goal-gate `:20471-20475`,
`:20920-20922`). **That is Raiciu's sentence, reproduced on our wire, at our
cell.** The span term's *structure* is therefore confirmed by the literature at
both endpoints of the heterogeneity axis: zero when paths match (c7, and
Raiciu), non-zero and growing with skew (c8, and RFC 6182's 12.5 MB example).

**So this is an AGREE-IN-STRUCTURE / DIVERGE-IN-CONSEQUENCE result, and that
is the honest summary of the whole section.** The literature and our engine
agree on *what the span term is for* and on *when it vanishes*. They disagree
on *how much it should be worth*: three published sources say `RTT_max`, we say
`RTT_max − RTT_min` (half), and **our ladder then measured even that half to be
over-funded by 45 % at c8 with goodput going up.** The gap between published
sizing and measured requirement is therefore not 2× but closer to 4×, and it
runs in the direction of *less*. Nothing in the literature explains that, and
BLEST's sub-1.0 λ is the only published hint that the analytic estimate is
systematically high.

**A THIRD field says the same thing (see cross-domain 2).** Eyerman et al.
(ACM TOCS 2009 §3.1.4) drop the analogous `W/D` buffer-coverage term from their
processor model outright — *"we assume this term is zero"* — because it is
small against the latency it is supposed to cover. **Three independent
literatures agree that the closed-form reorder-buffer term over-states its own
importance, and our wire agrees with all three.**

**The buffer-limited vs window-limited threshold: NO CLEAN COUNTERPART.**
`[UNVERIFIED — searched, not found]` No published predicate of the form "the
reorder buffer binds rather than the window when X" was located. What *is*
published is the sizing that makes the buffer *not* bind (item 1); the converse
appears to be exactly what BLEST's adaptive λ exists to discover empirically
*because no closed form was available*. **Our ladder's c8 result is therefore a
datum in a place the literature also handles empirically** — a mild vindication
of having measured it rather than derived it.

**DAPS — and its Eq. (2) is the closest published statement of our span's own
continuity property.** The buffer rule (Kuhn et al., ICC 2014 §II.D Eq. 3) is
`R_buf_min = Σᵢ cᵢ × maxᵢ rᵢ` — `RTT_max` again, a fourth source — with its own
caveat *"This solution is however neither optimal nor scalable, as R_buf_min can
quickly grow beyond manageability."* But the blocking-time model, §II.B Eq. (2),
verbatim:

> "**T_maxblock = t2 − t1 = rs/2 + 8L/cs − rf/2 − 8L/cf. (2)**"

**`T_maxblock` is proportional to `(rs − rf)/2` plus a serialisation
difference, so it vanishes CONTINUOUSLY as the RTT skew goes to zero, with no
threshold and no regime switch.** That is precisely the property our span term
has and which `three_term_span_vanishes_continuously_as_skew_goes_to_zero`
pins — and it is the property THE NO-MODE-SWITCH INVARIANT requires. **The
published blocking TIME uses the RTT difference; the published buffer SIZE uses
`RTT_max`.** So the difference form is not unknown to the literature — it is
just used for a different quantity, and no source multiplies it by a bandwidth
to get a buffer. Our span term is that unwritten product. `[DAPS's prose never
says "negligible when paths are similar"; Eq. (2) says it, the prose does not.]`

**ECF** (Lim, Nahum, Towsley & **Gibbens** — `[authorship correction: the fourth
author is Richard J. Gibbens, not "Lee"]`, CoNEXT 2017 §4) decides by comparing
completion times rather than by sizing a buffer, verbatim:

> "**(1 + k/CWNDf) × RTTf < RTTs + δ**"  · "**(k/CWNDs) × RTTs ≥ 2RTTf + δ**"
> · "**(1 + k/CWNDf) × RTTf < (1 + β)(RTTs + δ)**"

where *"we add a margin **δ = max(σf, σs)**, where σf and σs are the standard
deviations of RTTf and RTTs"*, and the third inequality *"adds some hysteresis
to the system and prevents it from switching states … too frequently."*

**Two things worth carrying from ECF.** (i) Its margin is a **dispersion**
term — `max(σf, σs)`, the standard deviation of the RTTs — which is the same
shape correction CD-1 derives for the slack from base-stock theory: *provision
against variability, not against a mean.* Two unrelated literatures, same
correction. (ii) ECF needs explicit **hysteresis** (`β`) to avoid state
flapping, which is a published acknowledgement that a threshold-shaped
scheduling decision has exactly the flapping problem our invariant exists to
forbid — and ECF's answer is a mode switch with damping, where ours would have
to be a continuous law.

**IMPLICATION.** Test, do not adopt: published span sizings are *larger* than
ours and our wire says ours is already over-funded. The transferable idea is
not BLEST's magnitude but **BLEST's λ mechanism** — an adaptive scalar on the
span, driven by observed HoL-blocking, initialised at 1.0. Note it inherits the
loop-stability objection ADR-0071 raises against candidate (c) — **but note
also that BLEST's λ moves on an EVENT (blocking seen / not seen) rather than on
a MAGNITUDE (measured idle time), which is a materially different loop and may
be why it is stable.** That distinction is offered for the user's attention and
is not adjudicated here.

---

## 3. `gain = 2.0` vs BBR's `cwnd_gain = 2`

**OURS:** `gain = 2.0`, ADR-0070 finding 3 verdict **FOSSIL**; source comment
argues *"≥2 keeps the pipe full (≈1 BDP) while leaving ≈1 BDP of headroom to
keep sending fresh data during a one-RTT recovery round."*

**THEIRS — BBRv1, draft-cardwell-iccrg-bbr-congestion-control-00 §4.2.3.2:**

> "Scaling up the BDP by cwnd_gain … **bounds in-flight data to a small multiple
> of the BDP, in order to handle common network and receiver pathologies, such
> as delayed, stretched, or aggregated ACKs**."

**BBR, ACM Queue 2016, "Delayed and Stretched ACKs":**

> "Cellular, Wi-Fi, and cable broadband networks often delay and aggregate ACKs.
> When inflight is limited to one BDP, this results in throughput-reducing
> stalls. **Raising ProbeBW's cwnd_gain to two allowed BBR to continue sending
> smoothly at the estimated delivery rate, even when ACKs are delayed by up to
> one RTT.** This largely avoids stalls."

**BBRv3, draft-ietf-ccwg-bbr §2.5 — a DIFFERENT derivation:**

> "**BBR.DefaultCwndGain**: A constant specifying **the minimum gain value that
> allows the sending rate to double each round** (2)."

**And the queue-bound statement, draft-ietf-ccwg-bbr §5.3.1.1:**

> "Once the pipe is full, a queue typically forms, but **the BBR.cwnd_gain
> bounds any queue to (BBR.cwnd_gain - 1) * estimated_BDP**, which is
> approximately (2 - 1) * estimated_BDP = estimated_BDP. The immediately
> following Drain state is designed to quickly drain that queue."

**VERDICT: AGREE on the value; DIVERGE on the derivation — and our stated
derivation matches NEITHER published one.**

**(a) FOLKLORE CORRECTION, and it lands on our own comment.** The research pass
searched all four BBR draft versions plus the paper: **"leaving room to send
during a recovery round" is not in any primary BBR source.** Recovery is
handled by packet conservation and `prior_cwnd`, never by `cwnd_gain`. BBR's
two published rationales are (i) **delayed/stretched/aggregated ACK
absorption** (v1, the paper, the Linux comment) and (ii) **the minimum gain
permitting per-round rate doubling** (v2/v3, with the cited derivation
explicitly noting *"this model ignores ACK aggregation effects"*). Our source
comment's "recovery runway" argument is a **third** argument, and it is the
un-published one. So `gain = 2.0` is not a fossil — **it is the right value
with the wrong citation**, and the tree has been repeating a rationale the
literature does not support.

**(b) BBR states §16.56's design sentence as arithmetic.** *"`cap − BDP` IS the
standing queue"* is `(cwnd_gain − 1)·estimated_BDP` renamed. This is direct
published confirmation that ADR-0071 family 2's *framing* is standard, and it
supplies the conversion the family needs: **any multiplier `g` on the BDP is a
promise of `(g − 1)·BDP` of standing queue.**

**(c) It therefore prices the composed law in one line.** The composed law is
`3.125·Σ(rate·K·RTprop)`, i.e. `g = 3.125`, i.e. **2.125 BDP of standing
queue** — precisely what §16.57 measured (2.4× queue, 43–48 % worse delivered
latency at goodput parity). BBR's published operating range for this exact
coefficient is **[0.5, 2.25]**: 2 by default, 2.25 transiently in ProbeBW_UP
(*"It also raises BBR.cwnd_gain to 2.25"*), and **0.5 in ProbeRTT**. Our
composed law sits at 3.125 — **above the top of the published range,
permanently, with no Drain and no ProbeRTT.** That last clause matters: BBR
pairs `cwnd_gain = 2` with two mechanisms whose entire job is to *remove* the
1 BDP it permits. We have neither.

**(d) The MPTCP ×2 is the same constant again.** RFC 6182's `2*` and BBR's
`cwnd_gain = 2` are independent derivations landing on 2. **Our shipped 2.0
agrees with both; the composed law's 3.125 agrees with neither.**

**IMPLICATION.** The strongest published support in this document for a verdict
the tree already reached on the wire (§16.57 MAGNITUDE refuted). Cheapest
action, no measurement: a documentation fix citing RFC 6182 §5.3 and
draft-ietf-ccwg-bbr §2.5/§5.3.1.1 as provenance, **and a correction removing
the unsupported "recovery runway" rationale.** That discharges ADR-0070 finding
3 entirely. **Nothing here licenses re-fitting `gain`**; ADR-0070 Decision item
4 still forbids it, and the literature agrees with the current value.

---

## 4. The δ-priced queue bound (ADR-0071 family 2) vs Copa's δ and CoDel's target

The item where the literature is furthest ahead of us, and the most
consequential section of this document.

**OURS** (ADR-0071 family 2):

```text
δ_headroomᵢ = D(δ, RTpropᵢ) = min( b(δ)·RTpropᵢ, 2·RTpropᵢ )
   b(Realtime) = ½,  b(Auto) = 1,  b(Bulk) = 2      ← round trips of RTprop
```

Permitted standing queue = **b × RTprop** of time, i.e. **b × BDP** of packets.

### 4a. Copa's δ — the same letter, incompatible units

**THEIRS — Copa (Arun & Balakrishnan, NSDI 2018), verbatim:**

> "The objective function we use combines a flow's average throughput, λ, and
> packet delay (minus propagation delay), d: **U = log λ − δ log d** … Here, **δ
> determines how much to weigh delay compared to throughput; a larger δ
> signifies that lower packet delays are preferable.**"

> "the steady-state sending rate … that maximizes U is **λ = 1/(δ·dq)**, (1)
> where dq is the mean per-packet queuing delay (in seconds), and **1/δ is in
> units of MTU-sized packets**."

> "**At equilibrium, when the target rate, λt = 1/(δ·dq), equals the actual
> rate, cwnd/RTT, there are 1/δ packets in the queue.**"

> "the queue length at the bottleneck … oscillate[s] between having 0 and 2.5/δ̂
> packets every five RTTs … **The equilibrium queue length is 1.25/δ̂
> packets.**"

Default and its justification:

> "A value of 1 causes one packet in the queue on average at equilibrium …
> jitter causes packets to be imperfectly paced in practice, causing frequently
> empty queues and wasted transmission slots … Hence we choose **δ = 1/2**,
> providing headroom for packet pacing."

**VERDICT: DIVERGE — a UNIT MISMATCH the tree has not named.**

Copa's δ prices the standing queue in **packets** — `1.25/δ`, an *absolute
count independent of the BDP*, ≈ **2.5 packets** at its default δ = 0.5. Our δ
prices it in **round trips of RTprop**, i.e. `b·BDP` packets, which scales with
the path. At c8 (`BDP ≈ 1605` symbols, ADR-0071's own table):

| | permitted standing queue at c8 |
|---|---|
| Copa, δ = 0.5 (its default) | **≈ 2.5 symbols** |
| Ours, `b = ½` (Realtime — the *tightest* point of our dial) | **≈ 800 symbols** |
| Ours, `b = 2` (Bulk) | **≈ 3 210 symbols** |

**Roughly three orders of magnitude, on the same letter**, and the *direction is
inverted* (larger Copa δ ⇒ less queue; larger `b(δ)` ⇒ more). Neither choice is
wrong — Copa's δ is a utility weight, ours is a budget — but **ADR-0068
proposes fusing them** (*"δ remains the ONLY latency knob"*) while the cap
layer's δ means something numerically incompatible with the CC layer's. **That
is a concrete, cheap-to-check hazard for ADR-0068, recorded here and not
adjudicated.**

Copa also states, verbatim, the two measured conditions under which its own
delay reasoning breaks — **and both describe our dual cells**:

> "We have found that this behavior breaks only under two conditions in
> practice: (1) when the propagation delay is much smaller than the queuing
> delay and (2) **when different senders have very different propagation delays,
> and the delay synchronization weakens.**"

### 4b. CoDel — a DERIVED setpoint for the standing queue, which predicts §16.57

**THEIRS — RFC 8289 (CoDel) §3.2 "Target Setpoint", verbatim:**

> "It is straightforward to derive an analytic expression for the average
> goodput of a TCP conversation at a given round-trip time r and target f (where
> f is expressed as a fraction of r). Reno TCP, for example, yields:
> **goodput = r (3 + 6f - f^2) / (4 (1+f))**"
>
> "Since the peak queue delay is simply the product of f and r, power is solely
> a function of f since the r's … cancel:
> **power is proportional to (1 + 2f - 1/3 f^2) / (1 + f)^2**"

> "As Kleinrock observed, the best operating point … is the peak power point …
> a target of 0.1r runs the risk of pushing shorter RTT connections over the
> knee … Generally, a more conservative **target of 0.05r offers a good
> utilization vs. delay trade-off** while giving enough headroom to work well
> with a large variation in real RTT."

> "**This results in a particularly simple form for the target: the ideal range
> for the permitted standing queue, or the target setpoint, is between 5% and
> 10% of the TCP connection's RTT.**"

> "As the above analysis shows, **a very small standing queue gives close to
> 100% utilization of the bottleneck link.** While this result was for Reno TCP,
> the derivation uses only properties that must hold for any 'TCP friendly'
> transport."

**VERDICT: DIVERGE by one to two orders of magnitude — and CoDel's DERIVATION
PREDICTS OUR MEASURED RESULT.**

| our operating point | standing queue as fraction of RTT | ratio to CoDel's 5 % |
|---|---|---|
| δ dial, Realtime (`b = ½`) | 50 % | **10×** |
| δ dial, Auto (`b = 1`) | 100 % | **20×** |
| δ dial, Bulk (`b = 2`) | 200 % | **40×** |
| shipped `gain = 2.0` ⇒ `(g−1)·BDP` | 100 % | **20×** |
| composed law `3.125` ⇒ `2.125·BDP` | 212 % | **≈ 42×** |

**CoDel's derivation says exactly what §16.57 measured.** *"A very small
standing queue gives close to 100% utilization"* — so raising the cap above a
few percent of the BDP buys **no goodput** and costs **pure delay**. §16.57
measured the composed law at sc2 granting 2.24× the cap, 2.4× the queue,
**goodput parity within 2σ (0.993 / 1.003)** and **1.43–1.48× WORSE delivered
latency on both seeds, far outside 2σ.** That is CoDel §3.2's result reproduced
on our wire.

**This is the single most valuable finding in this document.** The tree treats
"how much standing queue should δ permit?" as an open derivation question
(§16.57's closing sentence, §16.59's successor, ADR-0071 family 2's reason for
existing). **It has a published, derived answer — from Kleinrock power
maximisation, not a fit — and the answer is ≈5 % of the RTT, 10–40× tighter
than every point of our δ dial.**

Two caveats, stated rather than argued past:

1. **CoDel's target is an AQM setpoint at a bottleneck; ours is a sender-side
   pool ceiling.** Same physical quantity, different measurement point, and
   Little's law converts (ADR-0071's sc2 conversion closes to 3 %). But CoDel
   assumes a *TCP-friendly AIMD* sender and a queue *drained by drops*; we
   backpressure instead. The conclusion rests only on "properties that must
   hold for any 'TCP friendly' transport" (the RFC says so), so the transfer is
   defensible — **but it is a transfer.**
2. **5 % of RTT at c8's BDP ≈ 1605 is ≈80 symbols.** Every ADR-0071 family-2
   candidate asks for at least 10× that. Whether an FEC-carrying,
   retain-until-acked multipath sender needs more than a TCP-friendly
   single-path flow is a real question — but it must now be **argued against a
   published derived setpoint** rather than into open space.

**CoDel's INTERVAL is our recovery-clock question, also derived** (§3.1):

> "**Conservatively, this interval SHOULD be at least a round-trip time to avoid
> falsely detecting a persistent queue and not a lot more than a round-trip
> time to avoid delay in detecting the persistent queue.** This suggests that
> the appropriate interval value is **the maximum round-trip time of all the
> connections sharing the buffer.**"

Note the shape — **RTT-relative with a two-sided justification, never an
absolute millisecond clamp.** That is the template §6 needs. And it is the
*fourth* independent appearance of `RTT_max`.

> **FOLKLORE CORRECTION.** CoDel's shipped constants are `TARGET = 5 ms` and
> `INTERVAL = 100 ms` (§5.3), and 5 ms is 5 % of the 100 ms *interval*, which
> stands in for the RTT. The derived quantity is the **ratio 0.05**, not the
> 5 ms. Anyone porting CoDel's number rather than its ratio has ported nothing.

**IMPLICATION — highest value, lowest cost in the document, and it changes no
code.** **ADR-0071 family 2's dial should be scored against CoDel's derived
5–10 % setpoint before anything else.** The ladder already sweeps cap as a
magnitude, so the 5 %-of-RTT rung is *a number already computable per cell*
(`BDP + 0.05·BDP`: c1 ≈ 184, sc2 ≈ 344, c7 ≈ 1161, c8 ≈ 1685, c8L ≈ 5225
symbols) and may be readable off curves we already have. **If goodput at those
rungs is at parity, CoDel is confirmed on our wire and the entire δ dial is
mis-scaled by 10–40×.** That is falsifiable from existing data.

---

## 5. The slack/stall law and `17/8` vs published recovery provisioning

**OURS:** `stall = (1−ρ)·D(δ) + ρ·(9/8·srtt + srtt)`, giving `17/8·srtt =
2.125·srtt` at the shipped ρ = 1; `slack/window ≡ 2.125` in 833 of 833
evaluations (§16.57); measured payout **zero** at saturated sc2.

**THEIRS — RFC 6182 §5.3 is the closest published counterpart, and it makes the
distinction ADR-0071 family 1 is asking about:**

> "The **worst-case** scenario would be when the subflow with the highest
> RTT/RTO experiences a timeout … the smallest connection-level receive buffer
> that would be needed to avoid stalling with subflow failures is
> **sum(BW_i)*RTO_max** … **This is an order of magnitude more** than the
> receive buffer required for a single connection, and is **probably too
> expensive for practical purposes. A more sensible requirement is to avoid
> stalls in the absence of timeouts.**"

**VERDICT: AGREE on the existence of a recovery reserve; the literature
EXPLICITLY REJECTS provisioning it for the worst case, which is what ours
does.**

This is the published answer to family 1's central question and it has been
sitting in an RFC since 2011. RFC 6182 considers exactly two provisioning
levels — **timeout-proof** (`RTO_max`) and **fast-retransmit-proof**
(`2·RTT_max`) — evaluates the first, calls it *"an order of magnitude more"*
and *"too expensive"*, and **standardises the second**. Our `17/8·srtt` is
built from RFC 9002 *loss detection* (9/8) plus *one full retransmit round
trip* — i.e. it provisions the **recovery event**, permanently, which sits
between the two published levels but is charged **at every instant whether or
not anything is stalled** (ADR-0071's own indictment).

**NO published counterpart provisions a standing reserve for a transient
recovery.** `[Searched; not found]` Every published sizing in this family
(RFC 6182, Raiciu, Barré, DRS, Linux `tcp_rcvbuf_grow`) sizes a buffer so the
*sender does not stall during* a recovery — a **capacity** argument — never a
*reserve held against the possibility* of one. The transient/standing
distinction the wire measured (payout zero at saturated cells) is exactly the
distinction RFC 6182 draws when it refuses `RTO_max`.

**The operations-research counterpart is far sharper and settles it in closed
form — see cross-domain mapping 1.** The newsvendor's critical fractile at zero
underage cost gives optimal reserve **exactly zero**, and — more importantly —
the base-stock literature says our slack has the **wrong shape** independent of
its size: safety stock is `z·σ·√L`, driven by the *dispersion* of recovery
delay and *sub-linear* in it, where ours is linear in the *mean*.

**IMPLICATION.** Do not adopt anything here; the value is that ADR-0071 family
1's framing is confirmed as the right question by a published body that already
rejected the worst-case answer. **The cheapest validation is cross-domain
mapping 1's, not this one.**

---

## 6. The recovery clocks vs RACK-TLP and QUIC loss recovery

**OURS** (`net/mod.rs:814-815`): tail sweep and hole refresh both run on

```text
round = (2 · srtt).clamp(25 ms, 100 ms)
```

**THEIRS — RFC 8985 (RACK-TLP) §7.2, verbatim:**

```
TLP_calc_PTO():
    If SRTT is available:
        PTO = 2 * SRTT
        If FlightSize is one segment:
           PTO += TLP.max_ack_delay
    Else:
        PTO = 1 sec
    If Now() + PTO > TCP_RTO_expiration():
        PTO = TCP_RTO_expiration() - Now()
```

with its stated derivation:

> "**First, the default PTO interval is 2*SRTT.** By that time, it is prudent to
> declare that an ACK is overdue since under normal circumstances, i.e., no
> losses, an ACK typically arrives in one SRTT. **Choosing the PTO to be exactly
> an SRTT would risk causing spurious probes** given that network and end-host
> delay variance can cause an ACK to be delayed beyond the SRTT. Hence, the PTO
> is conservatively chosen to be the next integral multiple of SRTT."

**The reordering window, §6.2 Step 4:**

> "**Return min(RACK.reo_wnd_mult * RACK.min_RTT / 4, SRTT)**"

> "the RACK.reo_wnd becomes **(N+1) * min_RTT / 4** … **The RACK reordering
> window MUST be bounded, and this bound SHOULD be SRTT.**"

> "RACK persists using the inflated RACK.reo_wnd for up to **16 loss
> recoveries** … The rationale … is to bound such spurious recoveries to
> approximately once every 16 recoveries (**less than 7%**)."

**And, decisively for our clamp, §3.3.1 "Reordering Design Rationale":**

> "the degree of reordering in time difference in such cases is usually within a
> single round-trip time … **Hence, using a time threshold instead of a packet
> threshold strikes a middle ground**, allowing a bounded degree of reordering
> resilience while still allowing fast recovery."

**THEIRS — RFC 9002 (QUIC) §6.1.2 / §6.2.1, verbatim:**

> "**max(kTimeThreshold * max(smoothed_rtt, latest_rtt), kGranularity)**"
> "The RECOMMENDED time threshold (kTimeThreshold), expressed as an RTT
> multiplier, is **9/8**."
> "**PTO = smoothed_rtt + max(4*rttvar, kGranularity) + max_ack_delay**"

**VERDICT: AGREE on the base `2·SRTT` — it is RFC 8985 §7.2's TLP PTO verbatim,
with a published derivation. DIVERGE on the clamp: every published bound in
this family is RTT-RELATIVE; ours is two absolute millisecond constants.**

**(a) Our `2·SRTT` is not arbitrary.** The tree has described this clock as
un-derived. Its *base* is a published standard **with an argument we did not
have**: exactly-1×SRTT risks spurious probes on delay variance, so round up to
the next integral multiple. Our source comment reaches the same conclusion
(*"Must sit above the ack arrival time (~1×SRTT + jitter …)"*) — **same
reasoning, independently, and RFC 8985 is the citation for it.**

**(b) RACK's bounds are all relative, and there are three.** Lower `min_RTT/4`,
upper `SRTT`, outer `TCP_RTO_expiration()`. Not one absolute millisecond
appears. Against our `[25 ms, 100 ms]`: at loopback the 25 ms floor is enormous
(RACK's floor would be microseconds), and at c8's slow leg the 100 ms ceiling
may sit *below* `2·SRTT`, silently truncating. **Our clamp is the only part of
the expression with no counterpart, and it is the part that binds at both
extremes of the cell table.**

**(c) The tree's own derived candidate is already two-thirds RACK-shaped.**
`RWM_DERIVED_SWEEP` computes `round = max(2*srtt, patience_floor(jitter, srtt))`
with *"NO ceiling and zero new constants"*. That is RACK's structure minus
RACK's *upper* bound. The echo's documented **coincidence property** (*"the two
laws agree wherever 2·srtt already lies inside [25, 100] ms"*) means an arm
that never leaves the clamp is bit-identical to its control — and RACK says
what to add so it binds: **an `SRTT` ceiling and a `min_RTT/4`-shaped floor.**

**(d) A CORRECTION TO OUR OWN RECORD, and it weakens a claim the tree leans
on.** §16.43, ADR-0070 and §16.56 describe `9/8` as **"RFC 9002 §6.1.2
`kTimeThreshold`, cited not fitted"**, and ADR-0070 Deliverable 2 uses that to
claim the composed law has "zero fitted constants". RFC 9002's own text does
not support "not fitted":

> "| Note: TCP's RACK [RFC8985] specifies a slightly larger threshold,
> | equivalent to **5/4**, for a similar purpose.
> | **Experience with QUIC shows that 9/8 works well.**"

> "Implementations **MAY experiment with absolute thresholds, thresholds from
> previous connections, adaptive thresholds**, or the including of RTT
> variation. Smaller thresholds reduce reordering resilience and increase
> spurious retransmissions, and larger thresholds increase loss detection
> delay."

**So `9/8` is a cited EMPIRICAL RECOMMENDATION, and a non-unique one** — RACK
uses 5/4 for the same job, and RFC 8985's own `min_RTT/4` is likewise inherited
Linux practice (*"Linux TCP used the same factor … experience showed this
worked reasonably well"*), not a derivation. The composed law's `17/8 = 9/8 + 1`
therefore inherits a **tuned** constant. This changes no measurement, but
"zero fitted constants" is stronger than the source supports. Conversely,
§16.59's constraint that a successor moving 9/8 off a smoothed RTT "owes a new
citation or a new derivation" is **softened** by the `MAY experiment` clause —
the RFC anticipates the move, it just does not bless a value.

**(e) RFC 8985 documents our own §16.63 failure mode, by name.** §6.2 Step 4:

> "the reordering detection … has a **self-reinforcing** drawback when the
> reordering window is too small … RACK could spuriously mark reordered
> segments as lost, causing them to be retransmitted. In turn, **the
> retransmissions can prevent the necessary conditions … to detect
> reordering** since this mechanism requires ACKs or SACKs only for segments
> that have never been retransmitted. **In some cases, such scenarios can
> persist**, causing RACK to continue to spuriously mark segments as lost
> without realizing the reordering window is too small."

§16.63 measured `retx` rising **4.56× at c7** under `RWM_LOSS_SENT_TRUTH`.
**That is a published, named, self-sustaining retransmit loop in the exact
mechanism our recovery clocks implement** — and the transport instance of
cross-domain mapping 3's metastable-failure pattern. RACK's answer to it is the
DSACK-driven adaptive window with the 16-recovery persistence and the <7 %
spurious budget.

**IMPLICATION.** (i) Free: cite RFC 8985 §7.2 for `2·SRTT` and correct the
`9/8` record. (ii) Derivation, not battery: replacing `[25, 100] ms` with
RACK's relative bounds — the shape is published and `RWM_DERIVED_SWEEP` is
most of the way there. (iii) The genuinely missing mechanism is an **adaptive**
reordering window driven by observed spurious retransmits, which is the
published fix for the loop §16.63 measured.

---

## 7. Sender-truth ε̂ vs published sender-side loss accounting — **THE LITERATURE APPEARS TO EXPLAIN THE N = 1 ANOMALY**

**OURS** (§16.58's law, refuted on the wire at §16.63):

```text
eps_p  =  1  −  d(cum_received_p) / d(symbols_sent_p)
```

with, per §16.58's own provenance table:

> `symbols_sent_p` — **measured**, locally, `PathStats::symbols_sent`: **one
> increment per wire handoff on path *p* (source, repair and retransmit
> alike).**

**The measured refutation** (§16.63): ε̂ moves **20.1× HIGHER** at c7, 3.8×
higher at c8 — the wrong direction — and, decisively, **it survives at N = 1**
(c1 and sc2 read `pl_max` 0.0000 shipped against **0.3614** and **0.5821**
corrected), *where the cross-path attribution error it was built to repair
cannot exist by construction.* Meanwhile `retx` rises **4.56× at c7**, and the
two-sided `[ACKDIAG]` witness shows the wire's actual loss did not move.

**THEIRS — RFC 6675 §4 `SetPipe()`, verbatim:**

> "(a) If IsLost (S1) returns false: **Pipe is incremented by 1 octet.** … those
> segments that are still assumed to be in the network."
>
> "(b) If S1 <= HighRxt: **Pipe is incremented by 1 octet.** The effect of this
> condition is that pipe is incremented for **the retransmission of the
> octet**."
>
> "**Note that octets retransmitted without being considered lost are counted
> twice by the above mechanism.**"

and, in `NextSeg()`'s notes, the same hazard stated at length:

> "in sending these segments, **the sender has two copies of the same data
> considered to be in the network** (and also in the pipe estimate…). When an
> ACK or SACK arrives covering this retransmitted segment, **the sender cannot
> be sure exactly how much data left the network** (one of the two
> transmissions of the packet or both transmissions of the packet)."

**VERDICT: DIVERGE — and the published warning names, in one sentence, a
mechanism that would produce exactly the refutation §16.63 measured, including
its survival at N = 1.**

**The mechanism, stated as a hypothesis with its arithmetic.** Our denominator
`Δ(symbols_sent_p)` counts **every wire handoff, retransmits included**. Our
numerator `Δ(cum_received_p)` counts arrivals. So a retransmitted symbol is
counted **once in the denominator per transmission** but contributes **at most
once to the numerator**. Therefore:

```text
eps_hat = 1 − Δrecv/Δsent    with retransmits inflating Δsent only
        ⇒  eps_hat reads HIGH by roughly the retransmit fraction
```

and — this is the part that matters — **the inflation closes a positive
feedback loop**:

```text
eps_hat ↑ → repair_debt / P_lost / NACK budgets ↑ → retx ↑ → Δsent ↑ → eps_hat ↑
```

That loop is **path-count-independent**. It is present at N = 1 exactly as at
N = 2, which is precisely the observation §16.63 records as inexplicable:
*"Whatever `RWM_LOSS_SENT_TRUTH` is doing on this wire, it is not repairing a
per-path attribution error, because the effect is present where that error
cannot be."* And the loop's own signature — retransmissions rising 4.56× while
the independent witness shows the wire's loss unmoved — is what the battery
measured.

**AND THERE IS A PAPER ON EXACTLY THIS BIAS, WITH MAGNITUDES.** Allman, Eddy &
Ostermann, "Estimating Loss Rates With TCP", *ACM Performance Evaluation Review*
31(3):12–24, December 2003 — abstract, verbatim:

> "we first show that **using a simple count of the number of retransmissions
> yields inaccurate estimates of the loss rate** in many cases. The
> mis-estimation stems from flaws in TCP's retransmission schemes that cause the
> protocol to **spuriously retransmit data** in a number of cases."

and §3, the measured magnitudes:

> "For TCP Reno transfers, we have found that retransmits exactly estimate the
> loss rate in roughly 26% of the transfers. However, in roughly two-thirds of
> the transfers, using retransmits as an estimate of the loss rate is off by
> more than 10%. Further, **in approximately 16% of the transfers, the
> discrepancy between retransmissions and losses is over 100%.** Finally, the
> median percent difference between the number of retransmits and the actual
> number of losses in the Reno transfers is roughly **33%**."

**AND the published estimators never use our denominator.** In every
sender-side loss/delivery estimator surveyed, the denominator is **newly
delivered**, per-packet, resolved at (S)ACK time — never "packets sent" over a
window. `draft-cheng-iccrg-delivery-rate-estimation` §2.2, verbatim:

> "**Since the rate samples only include packets actually cumulatively and/or
> selectively acknowledged, the sender knows the exact octets that were
> delivered to the receiver (not lost)**, and the sender can compute an estimate
> of a bottleneck delivery rate over that time interval."

with an explicit warning against naive differencing (§2.2.1):

> "**it is not safe to simply calculate a bandwidth estimate by using the time
> between the transmit of a packet and the acknowledgment of that packet.**
> Transmits and ACKs can happen out of phase with each other… **Because of this
> effect, drastic over-estimates can happen**…"

an explicit in-flight-boundary rule (§3.2):

> "**If there are packets already in flight, then we need to start delivery rate
> samples from the time we received the most recent ACK, to try to ensure that
> we include the full time the network needs to deliver all in-flight
> packets.**"

and an explicit anti-double-count guard (§3.3): `if P.delivered_time == 0
return /* P already SACKed */`.

**PCC (Dong et al., NSDI 2015) solves the boundary by construction** — §3.1:
loss for a Monitor Interval is computed over *the set of packets sent in that
interval*, resolved ≈1 RTT after it ends, explicitly including retransmissions
in the sent set but attributing outcomes per packet: *"**At time T1,
approximately one RTT after T0 + Tm, it has received the SACKs for all packets
sent out in MI1.**"*

**So there are THREE published bias sources for our estimator, all inflating
ε̂, and all path-count-independent:**

| bias | source | why it survives at N = 1 |
|---|---|---|
| (a) **in-flight boundary** — `Δsent` includes a BDP whose ACKs have not arrived | RFC 9002's three-way exit: "sent but are not acknowledged, declared lost, or discarded" | a property of the window, not of paths |
| (b) **retransmits in the denominator** | RFC 6675's double-count note; Allman et al.'s 33 % median / >100 % in 16 % | a property of retransmission |
| (c) **ACK aggregation** | DRE §2.2.2: "ACK arrivals can temporarily make it appear as if data packets were delivered much faster" | a property of the link layer |

> **This is a HYPOTHESIS about our engine, not a finding, and it is labelled as
> one.** Nothing here was measured; this document measures nothing. What the
> literature supplies is a standards-track double-counting warning, a measured
> paper on the same bias, a unanimous published convention for the *other*
> denominator, and the fact that all three bias sources are properties of
> windows and retransmissions rather than of multipath — which is exactly why
> the effect would survive at N = 1.
>
> **The falsifier is cheap and needs no VM**: recompute ε̂ with `symbols_sent_p`
> counting *first transmissions only*, on the `[ACKDIAG]` cursors already
> captured in the ladder's logs, and see whether the 20× collapses. **§16.58's
> own bound already covers bias (a)** — `sender_truth_loss_delta_is_unbiased_under_a_constant_in_flight_lag`
> pins it under a *constant* lag — so if the hypothesis is right, (b) is the
> live term, because retransmit multiplicity is **not** constant in steady
> state: it moves with ε̂ itself. If the 20× survives the first-transmissions-only
> denominator, this hypothesis is dead and §16.63's anomaly is still open.

**Note also what §16.58 already got right, against the literature.** Its own
residual analysis — *"`symbols_sent` counts a symbol at handoff and
`cum_received` counts it ≈RTT later, so the sent cursor leads by ≈`in_flight`.
The offset is constant in steady state, hence the DELTAS are unbiased"* — is
correct **for the in-flight boundary**, and is pinned by
`sender_truth_loss_delta_is_unbiased_under_a_constant_in_flight_lag`. The
literature's objection is to a *different* term: not the boundary lag, but the
**retransmit multiplicity**, which is not constant in steady state because it
moves with ε̂ itself. **§16.58 bounded the one it saw and did not model the one
RFC 6675 warns about.**

**IMPLICATION.** Highest-value cheap test in the transport half of this
document. Adopt nothing; **test the first-transmissions-only denominator**
against the ladder's existing logs. RFC 6675's note is the citation for why
that is the right denominator, and it also supplies the standard's own
resolution: `pipe` counts retransmissions deliberately *because it is a
capacity estimate*, whereas a *loss-rate* estimate needs the opposite
convention. **The two quantities want different denominators, and we used the
capacity one for a loss question.**

---

## 8. The loss estimator's GE model and cross-path contamination vs network tomography

**OURS:** the GE channel (`e = p/(p+q)`, `σ²_burst = 1 + 2(1−p−q)/(p+q)`,
Appendix A) feeding `r*`; and §16.58's structural claim about attribution:

> "**Per-path attribution is not recoverable receiver-side.** The receiver can
> cheaply subtract, from a path's gap, the seqs that arrived on some other path;
> **it cannot attribute a seq that arrived NOWHERE, because the path identity is
> precisely what the loss destroyed.**"

**THEIRS — Cáceres, Duffield, Horowitz & Towsley, IEEE Trans. IT 45(7), 1999**
(`[quoted from the author preprint; journal pagination NOT verified]`):

> "Theorem 3. (i) **The model is identifiable, i.e., α, α′ ∈ (0,1]^#R and
> P_α = P_α′ implies α = α′.** (ii) As n → ∞, α̂ → α … almost surely."

and — this is the load-bearing clause —

> "**The key to this approach is that multicast traffic introduces correlation in
> the end-to-end losses measured by receivers.**"

**Castro, Coates, Liang, Nowak & Yu, *Statistical Science* 19(3), 2004 §2:**

> "**In general, A is not full rank, so that identifiability concerns arise.**
> Either one must be content to resolve only linear combinations of the
> parameters or one must employ statistical means to introduce regularization
> and induce identifiability."

**VERDICT: CONFIRMS §16.58's structural claim — and the standards bodies have
independently reached the same conclusion for exactly our topology.**

Network tomography owns "infer per-link loss from confounded end-to-end
observations", and its answer is precise: **per-path loss rates are identifiable
only when the observation matrix has full rank, and the classical positive
result buys that rank from an *induced correlation* (multicast) that a unicast
striped sequence space does not have.**

> `[CORRECTION carried from the research pass: Castro et al. 2004's
> identifiability statements are GENERIC rank statements about the linear model;
> its worked examples are multicast delay and OD matrices, not loss. The
> loss-specific unicast non-identifiability statement belongs to Coates & Nowak
> (ITC 2000) and Coates/Hero/Nowak/Yu (IEEE SP Magazine 2002). Cite those for
> the loss claim.]`

**Coates, Hero, Nowak & Yu, *IEEE Signal Processing Magazine*, May 2002,
verbatim — this is the exact statement:**

> "**If the routing matrix A is full rank, then unique maximum likelihood
> estimates of the loss rates can be formed by solving a set of linear
> equations. If A is not full rank, then there is no unique mapping** of the
> path success probabilities to the success probabilities on individual links."

**Coates & Nowak, ITC 2000 §7, give the continuous degradation** — no
threshold, which matters for our invariant. With `Λ = ∏γ` over the shared
subpath:

> "**If the conditional success probabilities γ are all exactly one, then it can
> be shown that maximum likelihood estimates of the unconditional losses α will
> tend to their true values** as the number of packet measurements increases."
>
> "**If one or more of the γ are less than one, then a systematic bias is
> introduced into the estimation process and the maximum likelihood estimators
> are not consistent.** However, the severity of the bias is directly linked to
> the extent to which the γ deviate from one."

with the estimators bracketed in `[Λ·αᵢ, αᵢ/Λ]`. **Λ = 1 ⇒ identifiable;
Λ → 0 ⇒ the interval is (0,1) and nothing is learned. It degrades continuously
in Λ, with no mode switch** — the same shape our own laws are required to have.

**★ AND THE IETF HAS WRITTEN DOWN OUR EXACT PROBLEM.**
`draft-ietf-quic-multipath-02` §9.1, on the *shared* packet-number-space design
— which is our `batch_seq` design:

> "If a zero-length connection ID is used, one packet number space for all paths…
> ACK frames report the numbers of packets that have been received so far,
> **regardless of the path on which they have been received. That means the
> sender needs to maintain an association between sent packet numbers and the
> path over which these packets were sent.**"

> "**senders MUST be able to infer the sending path from the acknowledged packet
> numbers, for example by remembering which packet was sent on what path.**" …
> "**Therefore, senders cannot directly use the packet sequence numbers to
> compute the Packet Thresholds** defined in Section 6.1.1 of [QUIC-RECOVERY].
> Relying only on Time Thresholds produces correct results, but is somewhat
> suboptimal."

**And the working group subsequently abandoned the shared space entirely.**
draft-14 (April 2025) §1:

> "This extension uses **multiple packet number spaces, one for each path**…
> Using multiple packet number spaces enables direct use of the loss detection
> and congestion control mechanisms defined in [QUIC-RECOVERY] **on a per-path
> basis**."

**This is as close to external validation as §16.58 could get.** The IETF
independently concluded that (i) with one shared sequence space, per-path loss
is recoverable **only from sender-side side information** — *"remembering which
packet was sent on what path"*, which is exactly `PathStats::symbols_sent` —
(ii) packet-threshold detection **cannot** be computed from the shared sequence
numbers, which is our §16.24 finding, and (iii) the durable fix is per-path
sequence spaces, which is what §16.24 built and refuted on cost. **Every one of
our three positions on this question has an IETF counterpart that reached it
independently.**

**A negative result worth recording, because it bounds what any fix could
achieve.** Gupta, Kumar & Vassilvitskii, "On Mixtures of Markov Chains", NIPS
2016, verbatim:

> "**consider a mixture where two of the matrices M_ℓ and M_ℓ′ in M are
> identical. Then for a fixed vector v, any s^ℓ and s^ℓ′ with s^ℓ + s^ℓ′ = v
> will give the same observations, regardless of the length of the trails.**"

**If two paths have the same loss behaviour, their split is unidentifiable at
any data volume** — only the aggregate is identified. That is c7 exactly (two
identical legs), and it is the same degeneracy as tomography's series-link
collapse. **No receiver-side estimator, however good, can attribute loss between
two statistically identical paths.** Which is a second, independent reason
§16.58's sender-side reading is the right architecture.

**Two consequences worth recording:**

1. **§16.24's rejected per-path serial NAMESPACE was the tomographically correct
   fix, and §16.58 is right that it solved a harder problem than necessary.**
   Putting the path identity on the wire makes `A` full rank by construction —
   it is the "tag at source" move (see CD-8). §16.58's insight is that the
   *sender* never had the identifiability problem at all, so the cheap fix is to
   read the sender's own counter rather than to restore rank at the receiver.
   **Tomography says both are valid; §16.58 picks the cheaper, and the
   literature supports that reasoning.**
2. **But item 7 above suggests the sender-side counter has a *different* defect**
   (retransmit multiplicity) that tomography has nothing to say about. **The two
   findings are independent**: §16.58's attribution argument survives this
   cross-check intact; §16.63's refutation is explained, if at all, by RFC
   6675's double-counting note, not by an attribution error.

**On GE itself: NO COUNTERPART, and the right result is not the one you would
reach for.** Appendix B Finding 5 already records the GE-inadequacy literature
(Hasslinger & Hohlfeld 2008; the 802.11/cellular HMM line; Sprout/Mahimahi) and
this pass surfaced nothing beyond it. **No published work was found on GE
parameter estimation from observations multiplexing several channels**
`[searched; not found]` — the tomography literature handles the *loss-rate*
inference problem but not the *burst-parameter* one under confounding.

**One structural correction is worth recording, because it redirects any future
attempt** (this is analysis, not a quotation): **two independent GE channels
interleaved into one sequence space are NOT a mixture of HMMs.** A mixture draws
a whole trajectory from one chain; interleaving produces a *function of the
product Markov chain* on 2×2 = 4 states. **The mixture-of-HMMs literature is the
wrong tool; the identifiability result to use is the HMM one.** Allman, Matias &
Rhodes, *Annals of Statistics* 37(6A), 2009, Theorem 6, verbatim:

> "The parameters of an HMM with r hidden states and κ observable states are
> **generically identifiable from the marginal distribution of 2k + 1
> consecutive variables** provided k satisfies ((k+κ−1) choose (κ−1)) ≥ r. (3)"
> … "**The worst case (i.e., the largest value for k) arises when κ = 2**."

**A binary loss trace has κ = 2 — the theorem's worst case.** A single GE
(r = 2) needs 3 consecutive observations; two multiplexed GE channels form a
product chain with r = 4 and need **7**. And only *generically*, only *up to
label swapping* (nothing says which factor is which path), and **nothing forces
the recovered 4-state chain to factor as a product of two 2-state chains at
all.** Combined with the Gupta et al. degeneracy above, the honest summary is:
**recovering per-path GE parameters from a shared sequence space is not merely
un-researched, it is ill-posed in exactly the cases we care about.** That is a
positive argument for the sender-side reading, not a gap to fill.

**IMPLICATION.** Record the tomography citations as the formal backing for
§16.58's non-recoverability claim — a documentation change. Nothing to adopt;
the estimator's open question is item 7's, not this one's.

---

## 9. Copa inside a δ-budgeted cap: nested delay-control loops (ADR-0068)

**OURS:** ADR-0068 proposes Copa's δ-priced delay control as the inner law with
a BBR-style rate model, while ADR-0071 family 2 proposes an outer cap that is
*itself* a delay budget. **Two delay-regulating loops, nested, on the same
path delay.**

**THEIRS — the cascade-control time-scale separation rule. Skogestad &
Postlethwaite, *Multivariable Feedback Control*, 2nd ed., §10.2 p. 387,
verbatim:**

> "**With a 'reasonable' time scale separation between the layers, typically a
> factor of five or more in terms of closed-loop response time**, we have the
> following advantages:
> 1. The stability and performance of a lower (faster) layer is not much
> influenced by the presence of upper (slow) layers because the frequency of the
> 'disturbance' from the upper layer is well inside the bandwidth of the lower
> layer.
> 2. With the lower (faster) layers in place, the stability and performance of
> the upper (slower) layers do not depend much on the specific controller
> settings used in the lower layers…"

and p. 420: *"in cascade control, it is usually assumed that the inner loop (K2)
is much faster than the outer loop (K1)"*.

**Seborg, Edgar, Mellichamp & Doyle, *Process Dynamics and Control*, 4th ed.
§16.1 p. 283:** *"**For a cascade control system to function properly, the
secondary control loop must respond faster than the primary loop.**"* — and the
tuning order, p. 284: *"the secondary controller should be tuned first with the
primary controller in the manual mode."*

**Hollot, Misra, Towsley & Gong, IEEE TAC 47(6), 2002, Remarks 2 — the delay
bound for a queue-control loop:**

> "**Stablizing an AQM control system in the face of the time-delay R0 places
> hard limits on the closed-loop control bandwidth** and, consequently, on the
> achievable speed of transient response. Indeed, for stable behavior,
> **closed-loop time constants are approximately bounded by R0/2 seconds.**"

with the PI tuning giving the crossover bound `ω_g = β/R₀`, `β ∈ (0, 0.85)`
for positive phase margin (§6, Eqs. 14–15) — i.e. **`ω_g·R₀ < 0.85`**.

**VERDICT: NO DIRECT COUNTERPART — and the absence is well-evidenced enough to
be a finding rather than a search failure.**

> **The negative result, with its method.** A systematic pass found **no
> published stability analysis of a delay-based congestion controller operating
> inside an outer, delay-budgeted window or buffer limit.** Queries run and
> returning nothing relevant: arXiv `abs:"nested control loops" AND
> abs:"congestion"` → 0; `all:"congestion control" AND "flow control" AND
> "nested"` → 0; `abs:"congestion control" AND abs:"cascaded"` → 0;
> `abs:"congestion control" AND abs:"inner loop"` → 0; `abs:"congestion
> control" AND abs:"two-level"` → 1 (irrelevant); DBLP `nested loops congestion
> control` → 0. A full-text search of the Copa paper for `nested`, `receive
> window`, `rwnd`, `flow control`, `cascade` returns **zero hits**. Honest
> rating: *"no such result is prominent or well-cited"*, not *"provably does not
> exist"* — the paywalled full-text indexes were not queryable.
>
> **So the composition ADR-0068 × ADR-0071-family-2 proposes is, as far as this
> pass can tell, un-analysed in the networking literature.** That is worth
> knowing before building it.

**What the general theory does say, and it is unanimous: the inner loop's gain
must scale as 1/delay.** Three independent statements:

- **Vinnicombe** (CUED/F-INFENG/TR.398, 2000), the sharpest form:
  *"**k_i ‖e_iᵀ P(jω) X‖₁ < π/(2 T_i) ∀i, ω**"*
- **Low, Paganini & Doyle** (*IEEE Control Systems Magazine* 22(1), 2002), the
  constructive form: *"instability will always occur at high values of τ unless
  the gain is made a function of τ. Indeed, **introducing a gain K/τ in the loop
  … leads to a loop gain (K/τ)·(e^{−sτ}/s), which is scale invariant**"*, with
  the reason: *"**it is impossible for a stable loop to track variations that
  are faster than the pure delay of the loop.**"*
- **Hollot et al.**, the AQM form quoted under CD-4.

And the same paper supplies the warning that matches our fastest cells:
*"**Perhaps more striking is the destabilizing effect of high capacity; as
routers become faster Reno is bound to go into an unstable regime.**"*

**Copa itself contains one proved gain-vs-BDP bound — for the variant it
rejected.** §3, "Alternate approaches to reaching equilibrium", verbatim:

> "A different approach would be to *directly* set the current sending rate to
> the target rate of 1/δdq. We experimented with and analyzed this approach, but
> found that the system converges only under certain conditions. **We proved
> that the system converges to a constant rate when C·∑ᵢ 1/δᵢ < (bandwidth delay
> product), where C ≈ 0.8 is a dimensionless constant. With ns-2 simulations, we
> found this condition to be both necessary and sufficient for convergence.
> Otherwise it oscillates.**"

**That is a necessary-and-sufficient "loop aggressiveness vs BDP" condition for
a Copa-class delay controller, and it governs precisely the high-gain,
zero-damping variant that a hard outer cap most resembles.** It is the closest
thing in the CC literature to the bound our composition would need.

**The cascade rule, in its most precise published form** — Skogestad, *Annual
Reviews in Control* 56 (2023) §2.5, verbatim:

> "**Time scale separation = τc1/τc2 (3)** … Shinskey (1981) recommends a time
> scale separation of at least 4, whereas Skogestad & Postlethwaite (2005) and
> Smith (2010) recommend at least 5. **If the time scale separation gets too
> small, typically 3 or less, the layers (loops) start interacting and resonance
> occurs …, such that performance degrades even nominally.**"

> "a process gain decrease in the lower layer (inner loop) is 'bad' as it
> translates into a larger ('slower') value of τc2 which reduces the time scale
> separation τc1/τc2, and **in addition τc2 appears as an effective delay as
> seen from the upper layer (outer loop)**."

**That last clause is the mechanism that makes the composition harder than it
looks**: the inner CC's own response time *adds to the outer cap's effective
delay*, which by Vinnicombe/Low then *lowers the outer loop's admissible gain*.
The two constraints tighten each other.

**And there is one measured, published instance of an outer window loop
destabilising an inner CC loop** — Huang, Handigol, Heller, McKeown & Johari,
"Confused, Timid, and Unstable: Picking a Video Streaming Rate is Hard", ACM IMC
2012, where the outer loop is *literally* a receive-window limit:

> "**Service B and Service C rely on the TCP receive window: when the playback
> buffer is full, TCP reduces the receive window to slow down the server.**"

> "rate selection based on inaccurate estimates can trigger a feedback loop,
> leading to undesirably variable and low-quality video. **We call this
> phenomenon the downward spiral effect.**"

with the structural diagnosis that transfers directly to us:

> "**The problem is that because it observes the throughput above TCP, it is not
> aware that TCP itself is having trouble reaching its fair share of the
> bandwidth.** Coupled with a (natural) tendency to pick rates conservatively,
> the rate drops down."

**An outer limiter that measures the throughput its own limiting produced is the
same circularity ADR-0070 finding 6 and ADR-0071 candidate (c) both name.**
It is a loss-based inner loop and a measurement paper — no theorem, no bound —
but it is the one published case of this topology going wrong, and it went wrong
in the direction our tree has already refuted twice.

**The problem, stated as arithmetic.** Cascade stability wants the inner loop
≥5× faster than the outer. But:

- Copa's inner loop has a **published period of ≈5 RTT** — its own §3 describes
  the queue oscillating "between having 0 and 2.5/δ̂ packets **every five
  RTTs**".
- Hollot bounds any queue-control loop's closed-loop time constant to ≈`R₀/2`,
  i.e. the *fastest* a delay loop may safely be is about half an RTT.
- Our outer cap refreshes on the anchor's own windowed estimators, whose
  windows are **seconds** (§16.59 measured `K` moving from 1.04 at a 2.5 s
  transfer to 1.505 at 20 s — *"the clock takes on 50 % of standing queue when
  the transfer runs long enough for the queue to fill the estimator's ≈10 s
  window"*).

So the separation may in fact be satisfied — **the outer loop is far slower
than 5× the inner** — which is the reassuring reading. **But the separation is
satisfied by accident, has never been stated as a requirement, and the
`WIN_STORE_MAX`/knee clamps are exactly the kind of nonlinearity a linear
cascade argument does not cover.** The actionable statement is that **ADR-0068
and ADR-0071 family 2 together constitute a cascade, and a cascade has a
published design rule that should be written into whichever ADR ships first.**

**The genuinely on-point published result is about the SELF-DERIVED BASELINE,
and it is ADR-0070 finding 6's loop, published in 2002.** Low, Peterson & Wang,
"Understanding TCP Vegas: A Duality Model", *J. ACM* 49(2), 2002 §4.2:

> "**when a source starts, its observed round trip time includes queueing delay
> due to packets in its path from existing sources. It hence overestimates its
> propagation delay ds and attempts to put more than αs·ds packets in its path,
> leading to persistent congestion.**"

> "**Persistent congestion is a consequence of Vegas' reliance on queueing delay
> as a congestion measure, which makes backlog indispensable in conveying
> congestion to the sources.**"

**That is our `cap → queue → RTT → cap` loop.** ADR-0070 finding 6 argues the
`max_bw·min_rtt` pair *"breaks that loop by construction: the rate max cannot be
inflated by queueing and the RTT min is the queue-free floor."* Low et al. show
the loop is real, name its consequence (persistent congestion), and identify the
mechanism (a delay baseline polluted by the standing queue the controller itself
permits). **§16.59 then MEASURED it: `K` = 1.04 at c8 vs 1.505 at c8L, the same
geometry at 8× the length — the min-RTT baseline absorbing standing queue as
the window fills.** So our min-filter is the right defence and it is *not
complete*: a min over a finite window is only queue-free if the queue empties
within the window, which is exactly Copa's argument for why it must oscillate:

> "**If the queue never empties, flows that arrive later will over-estimate
> their minimum RTT and hence underestimate their queuing delay.** … Thus, we
> need a scheme that … **makes small oscillations about the equilibrium to
> regularly drain the queues.**"

**This is the sharpest transferable design statement in the section: a
delay-baseline estimator is only honest if something guarantees the queue
empties periodically.** BBR guarantees it with ProbeRTT (`cwnd_gain = 0.5`);
Copa guarantees it with its 5-RTT oscillation; CoDel guarantees it by targeting
5 % of RTT. **We have no such mechanism**, and §16.59's measured `K` drift is
the predicted consequence.

**IMPLICATION.** (i) Record the cascade rule (Skogestad p. 387) as a design
constraint on the ADR-0068 × ADR-0071-family-2 composition, with the honest
note that the separation currently appears satisfied by accident. (ii) **The
load-bearing, cheap item: the tree has no queue-draining guarantee, and three
independent published designs each have one.** §16.59's `K` = 1.04 → 1.505
measurement is the evidence that its absence bites. That is a named,
citable gap and it does not require choosing any ADR-0071 candidate.

---

## 10. Boot/warm-up sizing and the knee vs IW and receive-buffer autotuning

**OURS:** `floor = max(ANCHOR_MIN_SAMPLES·cadence, RFC6928_IW) = max(8, 10) =
10` (§16.60, derived); `boot = 128` (ARGUED, never a battery arm); `knee =
2048/path` (MEASURED BUT STALE, ADR-0070 finding 4).

**THEIRS — RFC 6928 §2, verbatim:**

> "the upper bound for the initial window will be
> **min (10*MSS, max (2*MSS, 14600))** (1)"

with its provenance, which is **empirical and says so**:

> "We have tried different sizes in our large-scale experiments, and found that
> **10 segments seem to give most of the benefits for the services we tested**
> while not causing significant increase in the retransmission rates."
> … "at initial windows larger than 10, the results are mixed."

**VERDICT (floor): AGREE, and our use is more careful than the citation
requires.** §16.60 already derives the floor as `max(anchor warm-up, IW)` and
cites RFC 6928 for the IW term. The RFC's own value is empirical rather than
derived, which our derivation does not claim otherwise — the floor's provenance
is *"the largest of two independently-justified minima"*, which survives the
IW term being a measured recommendation. **No change owed.**

**THEIRS — Dynamic Right-Sizing.** The rule, from Fisk & Feng (LANL Tech
Report LAUR 00-3321) §7:

> "The receive buffer space is then increased, if necessary, to make sure that
> the next window advertised will be **at least twice as large as the amount of
> data received during the last measurement period**."

and the derivation, §6.5 — **note it is NOT the RFC 6182 argument:**

> "**In order to keep pace with the growth of the sender's congestion window
> during slow-start, the receiver should use the same doubling factor.** Thus
> the receiver should advertise a window that is twice the size of the last
> measured window size."

Linux `tcp_rcv_space_adjust()` (the classic comment, v4.9–v6.15):

> "/* A bit of theory : copied = bytes received in previous RTT, our base
> window. **To cope with packet losses, we need a 2x factor. To cope with slow
> start, and sender growing its cwin by 100 % every RTT, we need a 4x factor**,
> because the ACK we are sending now is for the next RTT, not the current one
> */"

**VERDICT (knee): NO COUNTERPART — and that is itself the finding.**

The autotuning literature has **no knee at all.** DRS and Linux size the buffer
as a *multiple of measured recent delivery*, recomputed every RTT, with **no
absolute per-path ceiling anywhere.** The only absolute is an administrative
memory limit (`tcp_rmem[2]`), which is exactly the role ADR-0071 family 2
assigns to `WIN_STORE_MAX`: *"a resource limit stated outside the law."*

**So the published architecture is precisely the one ADR-0071 family 2
proposes** — a law with no fitted ceiling, plus a separate administrative
memory bound — and it has been the default in Linux for two decades
(`tcp_moderate_rcvbuf` default 1). **`knee = 2048/path` has no counterpart in
this literature because the literature does not have a knee.** ADR-0070 finding
4's verdict (MEASURED BUT STALE, "per path" an untested inference) is
strengthened: the quantity is not merely stale, it is **structurally absent
from every comparable published design.**

> **FOLKLORE CORRECTION, and it touches our ×2 story.** DRS's factor 2 is a
> *slow-start-matching* argument, **not** RFC 6182's "one BDP for reordering +
> one BDP for fast retransmit". Three different published derivations
> (RFC 6182's two-BDP split, DRS's slow-start doubling, BBR's rate-doubling
> minimum) land on the same factor 2 for the same-shaped quantity. **The
> constant is robust across derivations; no single one of them is "the"
> provenance**, and a paper claiming one should say which.
>
> Also: **current Linux no longer tells the old story.** From v6.16 the
> function was restructured into `tcp_rcvbuf_grow()`, and the "2x for losses,
> 4x for slow start" comment is gone — replaced by `/* DRS is always one RTT
> late. */ rcvwin = newval << 1;` plus a slow-start growth term. The
> loss-cushion half of the rationale was **dropped**. Anyone citing the Linux
> comment must cite a kernel version.

**VERDICT (boot = 128): NO COUNTERPART, but the closest analogue contradicts
its magnitude.** `boot = 128` symbols is ~13× RFC 6928's IW of 10. Our own
§16.61 already found `128` to be "a fit to c2's link budget rounded to a power
of two". The IW literature's entire lesson is that the cold-start burst should
be **small and empirically bounded**, with RFC 6928 §1's rationale *"Ten
segments are likely to fit into queue space available at any broadband access
link"*. **`boot = 128` is the one constant in the chain that both lacks
provenance and exceeds its nearest published analogue by an order of
magnitude**, and it is the terminal `else` of both cap chains — i.e. the
`active_paths()` cliff lands a steady-state sender on it (ADR-0070 finding 5).

**IMPLICATION.** (i) `floor` needs nothing. (ii) The knee's absence from the
autotuning literature is a *free argument* for ADR-0071 family 2's shape and
should be recorded in that ADR's support column — it is not a decision, it is
prior art for the architecture. (iii) `boot = 128` vs IW = 10 is a cheap
pre-registerable arithmetic comparison, and §16.61 already derived the
replacement.

---

# PART II — THE CROSS-DOMAIN CROSS-CHECK

The user's instruction: *"these formulas have twins in other fields with older
and deeper literature."* They do. Four of the eight mappings below return a
closed form we do not have; two return a *shape* correction that is more
important than any magnitude; one returns a named failure mode with a published
mitigation list; and one supplies the review discipline whose absence ADR-0070's
postmortem is about.

Each mapping gives: the established result quoted, CONFIRMS / CONTRADICTS /
SHARPENS, and the translation table. **The numbering follows the user's list;
the sections are ordered by theme (inventory → buffers → failure modes →
control → detection → estimation → review discipline), so CD-4 through CD-8 do
not appear in numeric order.**

---

## CD-1. The slack term as a NEWSVENDOR problem — **this settles the `17/8` question in closed form**

**TRANSLATION TABLE**

| ours | operations research |
|---|---|
| slack reserve `rate·stall` | order quantity / stock level `Q` |
| stall duration | stochastic demand `D` |
| standing queueing delay from over-provisioning | overage / holding cost `c_o` |
| idle wire from under-provisioning | underage / stockout cost `c_u` |
| `rate·RTprop` (term 1) | mean demand over lead time, `μL` |
| the shipped ρ = 1 scope | a single-period decision |

**THEIRS — the critical fractile.** The canonical statement, from a teaching
source (`[SECONDARY — primary Arrow, Harris & Marschak 1951 NOT CONSULTED;
paywalled]`):

> "CF = C_u/(C_u + C_o)"  ·  "The optimal order quantity is the inverse CDF of
> demand evaluated at the critical fractile: **Q\* = F⁻¹(CF)**"

**THE ZERO-ORDER CONDITION — the result that matters** (`[SECONDARY]`):

> "If p < c (i.e. the retail price is less than the purchase price), the
> numerator becomes negative. **In this situation, the optimal purchase quantity
> is zero.**"

**VERDICT: CONFIRMS — and settles it.** Our measured case is `c_u = 0`: §16.57
measured the slack's payout at saturated sc2 as **zero** (goodput 0.993/1.003,
parity within 2σ) while its premium was **2.4× the standing queue**. Then

```text
CF = c_u/(c_u + c_o) = 0/(0 + c_o) = 0        ⇒   Q* = F⁻¹(0) = 0
```

**With zero shortage cost and strictly positive holding cost, the optimal
reserve is exactly zero, and any positive reserve is strictly dominated.** Our
"2.4× the queue for zero payout" is not a tuning error; it is the `c_u = 0`
corner of a problem solved in 1888.

**And the OR form satisfies CLAUDE.md's invariant by construction, which is the
non-obvious part.** `Q* = F⁻¹(c_u/(c_u+c_o))` is **continuous in the cost
ratio**. As the payout falls to zero the optimal reserve *slides* to zero; it
does not switch off. So the newsvendor prescribes a **dial**, not a mode bit —
which is exactly the shape ADR-0071 candidate (a′) reaches for with its
`p_lost`-weighted slack, and exactly what candidate (a)'s boolean `ARMED` is
not. **The OR literature independently arrives at the continuous form the
no-mode-switch invariant requires.** This is offered as a structural
observation about the candidate space; **it is not a recommendation and picks
no candidate.**

**SHARPENS, and this is the deeper result: our slack has the WRONG SHAPE,
independent of its size.** The base-stock reorder point (`[SECONDARY]`, and the
mapping is exact):

> "**ROP = L · E(D) + z_α σ_D √L**"  ·  "**SS = z_α × √[E(L)σ_D² + (E(D))²σ_L²]**"
> · "z_α is the inverse distribution function of a standard normal distribution
> with cumulative probability α"

`L·E(D)` **is** `rate·RTprop` — the bandwidth-delay product, term 1, exactly.
And `z_α σ_D √L` is the safety stock — the slot our `rate·stall` occupies. But:

- textbook safety stock is **proportional to the DISPERSION** `σ` of
  lead-time demand; ours is proportional to the **MEAN** stall duration;
- textbook safety stock is **sub-linear in lead time** (`√L`); ours is
  **linear** in `srtt`;
- lead-time *variability* enters under the square root via `(E(D))²σ_L²` — i.e.
  **RTT jitter belongs inside the safety-stock radical, not as a separate
  additive reserve.**

**A reserve linear in the mean recovery delay systematically over-provisions
relative to the base-stock optimum whenever recovery duration has low
variance** — which is a mechanism for the measured over-provisioning that is
independent of, and additional to, the `c_u = 0` argument. **Two independent OR
results both say the slack is too big, for different reasons.**

**Our own `r*` is ALREADY a newsvendor formula, and nobody noticed.** Appendix
A, §8.4:

```text
r* = max(0, e/(1-e) + z_{delta/e} · sqrt(e · s2_burst / (W · (1-e))))
     z_{delta/e} = normal_quantile(1 - delta/e)
```

That is `mean + z·σ` with `z` chosen from a **service level** — the safety-stock
formula, with `δ/e` as the fill-rate target. **The tree already implements the
OR-correct shape for the FEC rate and the OR-incorrect shape for the slack.**
The template for fixing the second is sitting in the same paper.

**IMPLICATION.** Do not adopt a number; **adopt the shape question**. The
cheapest validation is the one ADR-0071 candidate (d) already names —
`slack_bench.rs`'s idle-vs-backlog replay, 576 cells in 13 s, no VM — but
scored against the newsvendor prediction (`c_u ≈ 0 ⇒ reserve ≈ 0`) rather than
against a coverage point. **The OR literature does not pick between ADR-0071's
candidates and neither does this document**; what it does is say that any
candidate whose reserve is *linear in the mean* is the wrong functional form
regardless of its coefficient.

`[VERIFICATION GAP: Arrow/Harris/Marschak 1951, Scarf 1960's interior, and
Zipkin/Porteus were not consulted — all paywalled and PDF retrieval failed in
the research environment. The formulas above are quoted from teaching sources
and are standard, but the primary citations are un-consulted and are listed as
such in the References.]`

---

## CD-2. The resequencing span as REORDER-BUFFER sizing — **a third field agrees the term is over-stated**

**TRANSLATION TABLE**

| ours | computer architecture |
|---|---|
| `span = rate_fast · (RTT_max − RTT_min)` | ROB coverage: `rob_size / dispatch_width` |
| symbol emission rate | dispatch width `D` |
| RTT skew | miss latency `c_L2` |
| out-of-order arrivals awaiting the frontier | in-flight instructions awaiting in-order retirement |

**THEIRS — Karkhanis & Smith, ISCA 2004 §4.3, verbatim:**

> "short misses – the ones that have latency significantly less than **the
> maximum ROB fill time, i.e. rob_size/dispatch_width**"

> "if the load that misses happens to be the newest instruction in the window,
> then it will take approximately **rob_size/dispatch_width** cycles to fill the
> ROB in behind the load, so the penalty will be approximately
> **D − (rob_size / dispatch_width)**."

**Eyerman, Eeckhout, Karkhanis & Smith, ACM TOCS 27(2) Art. 3, 2009 §3.1.4:**

> "the time it takes to fill the entire ROB, **W/D**, minus the time it takes
> for the load to issue after it has been dispatched … the execution time for
> an isolated long back-end miss interval equals **N/D + c_L2 − (W/D − c_lr)**."

**VERDICT: CONFIRMS the units — `buffer / rate = the latency it covers` is
literally our `span = rate × Δlatency` solved for the buffer.**

**But the load-bearing finding is the next sentence, and it CONFIRMS our
ladder:**

> "Because the amount of useful work done under the long-latency loads,
> **W/D − c_lr, is relatively small compared to the main memory access latency
> c_L2** …, **we assume this term is zero** and approximate the penalty for
> isolated and overlapping long-latency loads as c_L2."

With that paper's own numbers (`W = 128`, `D = 4`, `c_L2 = 250` cycles) the
buffer covers **32 of 250 cycles**; full coverage would need `W ≥ D·c_L2 =
1000` entries and **no design does this.** So a mature engineering field, on a
formally identical quantity, **drops the reorder-buffer coverage term from its
performance model as negligible** — and three further results agree that the
buffer is not the binding constraint:

> "beyond a window size of 32 instructions, Maxwin only accounts for 50% or less
> of the MLP inhibiting conditions. Therefore, **issue window/ROB size
> limitation itself is only one of several impediments**." — Chou, Fahs &
> Abraham, ISCA 2004 §5.3.1

> "We show scheduling window size to be **less critical than other design
> aspects** for large instruction window processors. A significantly smaller
> 128-entry scheduling window is mostly sufficient to realize the performance
> potential of a large, 2048-entry, instruction window processor." — Akkary,
> Rajwar & Srinivasan, MICRO-36, 2003

**This is the third independent literature — after BLEST's sub-1.0 λ and our
own ladder — to conclude that the closed-form reorder-buffer term over-states
its own importance.** §16.63's finding that c8's span was "not load-bearing" is
in good company.

**CONTRADICTS the naive linear form, and this is the sharpest published
counter-result.** Eyerman et al. §2.2 gives the only real definition of a
balanced design:

> "We define an out-of-order processor design to be **balanced** if, for a given
> dispatch width D, the ROB (window size) and other resources … are of
> sufficient size to achieve sustained processor performance of D instructions
> per cycle **in the absence of miss events**. Furthermore, … **reducing the
> size of any one of the resources will reduce sustained performance below D**."

and §4.1 the scaling law:

> "for a balanced processor design, **ROB size scales superlinearly with both
> pipeline width and depth** because β/(β−1) > 1. Prior work … indicate ROB size
> **scales at least quadratically with D**."

**In the no-miss regime the binding constraint is the dependence critical path,
giving `W ∝ D²` — superlinear in RATE and INDEPENDENT of LATENCY.** So "buffer
= rate × latency" is *not* the general law even in the field that owns the
problem; it is the *miss-driven* regime only. **If our span term is meant to
bind, that is a claim to defend against this result.**

**And the cleanest literal `rate × latency` statement in architecture is about
MSHRs, not the ROB** — Mark D. Hill, arXiv:1901.02926, 2018:

> "how many buffers must a cache have to record outstanding misses if it
> receives 2 memory references per cycle at 2.5 GHz, has miss ratio 6.25%, and
> average miss latency is 100 ns? **Little's Law reveals the answer of 32
> buffers. However, … more buffers will be needed for the common case when
> misses occur unscheduled and bursts make some miss latencies larger than
> 100 ns.**"

**`rate × latency` is published as a LOWER bound; burstiness forces
over-provisioning.** That cuts *against* the ladder's under-funding result and
is recorded as the honest counterweight.

**CITATION WARNINGS the research pass surfaced, worth carrying:**
- **Riseman & Foster 1972 is NOT a primary source for "IPC ∝ √(window size)"** —
  their √ is over *conditional jumps bypassed*, and their conclusion is
  negative. Cite **Michaud, Seznec & Jourdan, IJPP 29(1), 2001 §3.2** for the
  square-root law: *"the IPC varies according to the square root of the reorder
  buffer size W … α√W ≤ IPC ≤ 2α√W"*.
- **The "Amdahl/Case rule" is not in Amdahl's 1967 paper**, which contains no
  equations. Cite Hennessy & Patterson's rules-of-thumb page.

**Distributed-systems analogue: one hit, three explicit negatives.** The hit —
Santos & Schiper, *Theoretical Computer Science* 496 (2013) §4.3 Eq. (9) — is
the BDP form (`w = ⌈min(w_cpu, w_net)⌉`, window = instance latency × bottleneck
throughput) but never uses the phrase. **Raft (Ongaro's thesis §10.2.2) has no
throughput × latency window bound at all**; chain replication's `Sent_i` is a
correctness invariant with no size bound; PBS bounds staleness *probability*,
not bytes. **Do not cite the consensus literature for a window formula.**

The most useful inverse citation is **Kafka KIP-16**, which *deleted* a
message-count replica-lag bound precisely because it is `throughput × time` and
therefore non-portable: *"We need a consistent way to measure replica lag in
terms of time."* **That is the strongest published engineering argument that
such a bound must be expressed as rate × time rather than as a constant — and
it is an argument against `knee = 2048/path`** (item 10).

**IMPLICATION.** Adopt nothing. Record two things: the balanced-design `D²`
result as the standing objection to a linear span law, and Hill's
burstiness caveat as the objection to deleting it. **Both are arguments the
tree does not currently have, on both sides of an open question.**

---

## CD-3. The dead wall as METASTABLE FAILURE — **named, characterised, and with a published mitigation list**

**TRANSLATION TABLE**

| ours | metastable-failure literature |
|---|---|
| the c8 "dead wall" | metastable failure state |
| bistable throughput statistic across identical runs | vulnerable state + trigger |
| recovery/retransmit work sustaining the collapse | **sustaining effect** (work amplification) |
| the `active_paths()` cliff to `boot = 128` | an accidental, un-designed **load shed** |
| `RWM_INFL_CAP` / `cwnd_full` (built, disabled) | circuit breaker / admission control |

**THEIRS — Bronson, Aghayev, Charapko & Zhu, HotOS '21, verbatim:**

> "**Metastable failures occur in open systems with an uncontrolled source of
> load where a trigger causes the system to enter a bad state that persists even
> when the trigger is removed.** In this state the goodput (i.e., throughput of
> useful work) is unusably low, and there is a **sustaining effect—often
> involving work amplification or decreased overall efficiency—that prevents the
> system from leaving the bad state.**"

> "A system starts in a stable state. **Once the load rises above a certain
> threshold—implicit and invisible—the system enters a vulnerable state.** The
> vulnerable system is healthy, but may fall into an unrecoverable metastable
> state due to a trigger."

> "**We consider the root cause of a metastable failure to be the sustaining
> feedback loop, rather than the trigger.**"

**Huang et al., OSDI '22, verbatim:**

> "**Definition 3 (Sustaining effect).** A sustaining effect is a feedback loop
> that keeps the system in an overloaded state such that Lsys(t) ≥ Csys(t) even
> after the trigger is removed."

> "**By far, the most common sustaining effect is due to the retry policy,
> affecting more than 50% of the studied incidents.**"

> "**Theorem 2 (Stable region).** Define **Cstable = Cnorm /(w∗L ∗ w∗C)**. If
> Lnorm < Cstable, then the system will never have a metastable failure."

**VERDICT: CONFIRMS, and supplies vocabulary, a quantified hysteresis gap, and
a mitigation list the tree does not have.**

**(a) Our dead wall matches the definition on every clause.** A bistable
throughput statistic whose collapsed branch persists; recovery work
(retransmits, repair) that consumes the capacity its own necessity is created
by; and — §16.57's finding — an instability that *"belongs to the cell's
bistability"* rather than to any measurand. §16.63 reports **0/27 c8 reps below
60 Mbit/s on the composed arm against 2/21 on the control**: a *rate*, which is
exactly how the metastability literature reports these ("~35% load-spike
triggers"), and exactly why our per-arm means kept failing.

**(b) The hysteresis gap is quantified, and it explains why our brake is
ad-hoc.** OSDI '22 Theorem 2 says recovery requires dropping below
`Cnorm/(w*L·w*C)` — the *amplification factor* below the tipping point, not
just below it. The paper's own comment on the consequence:

> "Load shedding was the most popular mitigation effort used in over 50% of the
> incidents. … **However, without a proper understanding of the metastability
> and feedback loops, it is hard to know just how much the load needs to be
> reduced.** This results in long mitigations and additional destructive steps."

**That sentence is a description of `boot = 128`.** ADR-0070 finding 5 records
the `active_paths()` cliff dropping the cap ≥6× to a cold-start constant,
mid-transfer, and calls it *"the loop's only stabiliser"* — a defect supplying a
brake by accident. The literature's verdict: an un-derived constant brake is
*precisely what you get when the amplification factor is unknown*, and the fix
is to measure the amplification, not to tune the constant.

The Google SRE Book supplies the numeric shape of the gap:

> "if a service was healthy at 10,000 QPS, but started a cascading failure due
> to crashes at 11,000 QPS, **dropping the load to 9,000 QPS will almost
> certainly not stop the crashes** … **the request rate would need to drop to
> about 1,000 QPS** in order for the system to stabilize and recover."

Trips at 11,000, recovers below ~1,000 — **an 11× gap.** Our cliff's ≥6× drop
is the same order, which may be why it works at all.

**(c) A vocabulary warning worth heeding in the paper.** "Hysteresis",
"bistable" and "bistability" **do not appear** in Bronson et al., Huang et al.,
or the HotOS '25 follow-up. That is not the literature's vocabulary. Cite
`Cstable = Cnorm/(w*L·w*C)` for the gap, not "hysteresis".

**(d) The same phenomenon was named in our own field in 1984, with an
experimental reproduction claim.** RFC 896 (Nagle), verbatim:

> "Should the round-trip time exceed the maximum retransmission interval for any
> host, that host will begin to introduce more and more copies of the same
> datagrams into the net. … Hosts are sending each packet several times … **This
> is congestion collapse.**"

> "**This condition is stable.** Once the saturation point has been reached, if
> the algorithm for selecting packets to be dropped is fair, the network will
> continue to operate in a degraded condition. In this condition every packet is
> being transmitted several times and throughput is reduced to a small fraction
> of normal. **We have pushed our network into this condition experimentally and
> observed its stability.**"

And, directly against the instinct to grow the pool:

> "**Adding additional memory to the gateways will not solve the problem.** The
> more memory added, the longer round-trip times must become before packets are
> dropped. Thus, **the onset of congestion collapse will be delayed but when
> collapse occurs an even larger fraction of the packets in the net will be
> duplicates** and throughput will be even worse."

**That is a 1984 argument against the entire "make the cap bigger" direction**,
and it is the same conclusion §16.57 reached by measurement. RFC 2914 §5 names
it: *"We call the congestion collapse that results from the unnecessary
retransmission of packets **classical congestion collapse**. Classical
congestion collapse is a **stable condition** that can result in throughput
that is a small fraction of normal."*
`[FOLKLORE CORRECTION: RFC 2914 §5 describes only TWO collapse forms. The
five-way taxonomy is in Floyd & Fall 1999, not the RFC.]`

**(e) The published brake designs, since ours is a constant.** HotOS '21's
mitigation list, verbatim:

> "we might **disable failover and retries or set a retry budget**, switch to
> **LIFO scheduling**, **reduce internal queue sizes**, **enforce priorities
> during overload**, **shed load by rejecting a fraction of requests or
> clients**, or even use the **Circuit Breaker pattern** to block all requests."

Note "**reduce internal queue sizes**" is on the list — the same direction
CoDel and §16.57 point. Two concrete clock designs:

- **Envoy outlier detection**: `base_ejection_time` 30 s, and critically *"The
  real time is equal to the base time multiplied by the number of times the host
  has been ejected and is capped by max_ejection_time"* — **multiplicative
  growth per re-trip with a cap**, the minimal principled upgrade over a
  constant.
- **DAGOR (WeChat, SoCC '18)**, five years in production, derives the clock
  from the system's own time constant: *"the threshold of the average request
  queuing time to indicate server overload is set to 20 ms"*, refreshed *"every
  second or every 2000 requests, whichever … is met"*, with *"α=5% and β=1%"*
  AIMD on the admission threshold. **The overload signal is queuing time, not
  utilisation** — which is our δ budget, at the admission layer.

**(f) And a warning that lands squarely on CLAUDE.md's invariant.** Marc
Brooker (AWS) argues against circuit breakers and for token buckets:
*"Circuit breakers are designed to turn partial failures into complete
failures."* … *"The adaptive strategy isn't modal in the same way, and seems to
perform better at lower failure rates."*

**The token bucket is the continuous formulation; the circuit breaker is the
mode switch.** Given THE NO-MODE-SWITCH INVARIANT, this is the published
argument for preferring a continuous admission law over a tripping brake — and
it bears directly on ADR-0071 candidate (a)'s boolean `ARMED` versus (a′)'s
continuous `p_lost` weight. **Stated, not adjudicated.**

**IMPLICATION.** This mapping changes no formula but supplies four things the
tree lacks: (i) the *name* and the published definition, so the c8 statistic's
bistability stops being an instrument problem and becomes an expected property
of the system class; (ii) `Cstable = Cnorm/(w*L·w*C)` as the quantity to
measure instead of tuning the cliff; (iii) a published mitigation list in which
"reduce internal queue sizes" appears and "add memory" is explicitly rejected;
(iv) RFC 896 as a 1984 citation for the direction §16.57 measured. **The
cheapest action is the ADR-0071 dead-wall instrument reframed: stop trying to
resolve a mean and measure the COLLAPSE RATE and the amplification factor,
which is what the literature reports and what §16.63's 0/27-vs-2/21 already
is.**

---

## CD-5. The cap as a BASE-STOCK policy, and pooled-vs-per-path as EPPEN'S RISK POOLING — **ADR-0058's verdict, published in 1979**

**TRANSLATION TABLE**

| ours | inventory theory |
|---|---|
| one shared outstanding pool (ADR-0058, shipped) | centralized / pooled stock |
| `RWM_STORE_PERCAP` per-path accounts (refuted) | decentralized multi-location stock |
| per-path stall/loss demand | per-location demand `D_i` |
| paths with different RTTs | suppliers with different lead times |
| `RWM_STORE_BORROW` bounded borrowing (refuted) | lateral transshipment |

**THEIRS — Eppen 1979, *Management Science* 25(5):498–501, abstract verbatim:**

> "This paper concerns a multilocation newsboy problem with normal demand at
> each location and identical linear holding and penalty cost functions at each
> location. … an expression is derived for the resulting expected holding and
> penalty costs … The expression is used to demonstrate that **(i) the expected
> holding and penalty costs in a decentralized system exceed those in a
> centralized system; (ii) the magnitude of the saving depends on the
> correlation of demands; and (iii) if demands are identical and uncorrelated,
> the costs increase as the square root of the number of consolidated
> demands.**"

**VERDICT: CONFIRMS ADR-0058's shipped decision — with a stated condition we
have never checked, and the condition is the interesting part.**

**(a) The pooled pool is right, and it was a theorem 47 years ago.** ADR-0058
built the per-path account family, chased it through three derived refinements
(percap → guard+honest caps → bounded borrowing) and refuted it empirically,
concluding *"lender-solvent slack cannot match pooled depth."* Eppen (i) is
that result: decentralized cost strictly exceeds centralized. **The three
sub-experiments were a rediscovery.** Note especially that ADR-0058's bounded
borrowing is *lateral transshipment*, whose known limitation is exactly what
was measured: it recovers part of the pooling benefit, never all of it.

**(b) The condition we have never checked, and it could invert the reading.**
Eppen (ii): *"the magnitude of the saving depends on the correlation of
demands."* The √N law holds for **uncorrelated** demands; as ρ → 1 the pooling
benefit **vanishes entirely** (`[SECONDARY — Eppen's closed form NOT
CONSULTED, paywalled]`; the pooled-variance identity `σ²_pool = Σσ²ᵢ +
2Σ_{i<j} ρ_ij σᵢ σⱼ` is quoted from a teaching source).

**Network paths that share a bottleneck or an access network have strongly
positively correlated loss and stall events.** So Eppen predicts a *testable
split we have never tested*: pooling should win big on independent paths and
win nothing on correlated ones. **c7 (two identical legs, likely correlated) and
c8 (asymmetric, likely less so) are exactly the two cells to check** — and
ADR-0058's own record shows the pooled/percap verdict *differing between c7 and
c8*, which is the signature Eppen predicts. **This is a genuinely new reading
of an existing result and it costs one correlation measurement.**

**(c) The √N law is not distribution-free.** From the heavy-tailed pooling
literature (Bimpikis & Markakis, *Management Science* 62(6), 2016), the
square-root law *"depends critically on the 'light-tailed' nature of the demand
uncertainty."* **Our loss process is Gilbert-Elliott — explicitly bursty, and
Appendix B's Finding 5 records GE itself under-provisioning against real
cellular traces by 2–4×.** So the √N pooling benefit should be expected to be
*smaller* than the classical law predicts, in the direction our measurements
already show.

**(d) Multi-source lead times — the closest published counterpart to our span
term, and it is structural rather than algebraic.** Fukuda 1964, *Management
Science* 10(4), abstract verbatim:

> "amounts of stock ordered at unit prices c_k and c_{k+1} … are delivered,
> respectively, k and k + 1 periods later. It is demonstrated that under
> suitable cost conditions, **the optimal policies are similar to those of the
> dynamic inventory problem with a delivery lag of k + 1 periods, except for an
> additional constant stock level** up to which it is desired to order at unit
> price c_K."

**Read that against RFC 6182.** Fukuda's optimal policy is *the policy for the
SLOW lead time, plus an additive constant for the fast source.* RFC 6182 sizes
on `RTT_max` — **the slow path** — for `Σ BW_i`. **Two fields, sixty years and
one discipline apart, both say: size the pool on the SLOWEST supplier's lead
time, and let the fast one contribute a separate additive term.** Our law sizes
per-path (`Σ bwᵢ·RTTᵢ`) and adds a half-sized span. That is a third independent
signal pointing the same way as items 1 and 2.

`[HONEST LIMIT: no primary source writes a `(L_slow − L_fast)·rate` term.
Fukuda's optimality is proven only for **consecutive** lead times (k and k+1);
beyond that no simple policy is optimal, which is why the dual-index heuristic
literature exists (Veeraraghavan & Scheller-Wolf 2008: within "1% or 2%" of
optimal). Do not cite Fukuda for the algebraic form, only for the structure.]`

**(e) Clark & Scarf does NOT license decomposing our pool.** The echelon
base-stock decomposition is exact only for **serial** systems; multipath is a
*distribution* (one-to-many) topology, which is precisely where the exact
decomposition breaks. **That is an argument FOR the single pooled cap ADR-0058
shipped**, not against it. `[Clark & Scarf 1960 abstract NOT CONSULTED.]`

**(f) Little's law, since every term of our cap rests on it.** Little 1961,
*Operations Research* 9(3):383–387, abstract verbatim:

> "if the three means are finite and the corresponding stochastic processes
> strictly stationary, and, if the arrival process is metrically transitive with
> nonzero mean, then **L = λW**."

with the scope note that the proof *"does not depend on arrival-or service-time
distributions, on the number of servers in the system, or on the queuing
discipline."* **Consequence for us: every unit of cap provisioned above
`rate·RTprop` and actually in flight sits in queue, and delivered residence
rises by exactly that excess over the rate. The holding cost is not a modelling
convention — it is a theorem**, and ADR-0071's own sc2 conversion closing to
3 % is Little's law being obeyed.

**IMPLICATION.** (i) Record Eppen 1979 as the prior art for ADR-0058 — a
documentation change. (ii) **The one genuinely new, cheap experiment this
mapping suggests: measure cross-path correlation of stall/loss events, and
check whether the pooling advantage tracks Eppen's √N or collapses toward 1.**
That reframes ADR-0058's c7-vs-c8 split from an anomaly into a prediction.
(iii) Fukuda + RFC 6182 agreeing on "size on the slowest lead time" is a third
vote on item 1's open question.

---

## CD-4. The δ-queue budget as CLASSICAL CONTROL — the AQM stability literature

**TRANSLATION TABLE**

| ours | control theory |
|---|---|
| δ budget / permitted standing queue | reference setpoint `q_ref` |
| the cap | actuator (admission limit) |
| Copa's rate law | inner loop |
| the δ-priced cap | outer loop |
| RTprop | loop dead time `R₀` |
| anchor estimator windows | sensor filter time constant |

**THEIRS — Hollot, Misra, Towsley & Gong, IEEE TAC 47(6):945–959, 2002.** The
linearised TCP/AQM plant, Eq. (6):

> P(s) = (C²/2N) / [(s + 2N/(R₀²C))(s + 1/R₀)]

carried through the loop with the delay as `P(s)e^{−sR₀}`. The stability
condition, §4.2 Eqs. (9)–(10):

> "we first allow RED's low-pass filter to dominate the loop by requiring ω_g to
> be less than the corner frequencies of either the TCP or queue dynamic; that
> is, **ω_g ≤ min { 2N/(R₀²C), 1/R₀ }** (9)"

and the PI tuning, §6 Eqs. (14)–(15):

> "**z = 2N/(R₀²C)** (14) … we take the loop's unity gain crossover frequency as
> **ω_g = β/R₀** (15) … **values of β ∈ (0, 0.85) yield positive phase
> margins** … **β = 0.5 gives a phase margin of about 30°.**"

plus the gain warning, Remarks 2: *"either small TCP loads N or large link
capacities C increase this gain, leading to decreased stability margins and
increased oscillatory response."*

**VERDICT: SHARPENS — a queue setpoint is not free; it comes with a
delay-product bound on how fast it may be enforced.**

**(a) The bound is `ω_g·R₀ < 0.85`, and it is the quantity our design does not
state.** ADR-0071 family 2 specifies *what* queue δ permits; it says nothing
about *how fast* the cap may move to enforce it. Hollot says the closed-loop
bandwidth is bounded by the RTT — *"closed-loop time constants are approximately
bounded by R₀/2 seconds"* — regardless of the setpoint. **A δ budget plus a cap
that reacts faster than ≈`R₀/0.85` is an oscillator, and no amount of correct
setpoint arithmetic fixes it.** This is the missing half of family 2's
specification.

**(b) RED's documented failure is our estimator-window question.** Misra, Gong
& Towsley, SIGCOMM 2000:

> "**We point out a flaw in the RED averaging mechanism which we believe is a
> cause of tuning problems for RED.**"
> "**If we maintain a high value of [K], then the AQM function starts tracking
> the instantaneous queue length closely resulting in sustained oscillations.**"
> "**As the link capacity increases, the RED average queue estimate tracks the
> instantaneous queue length more closely, essentially resulting in sustained
> oscillations.**"

and Firoiu & Borden, INFOCOM 2000, on the fix:

> "**It follows that the 'ideal' sampling rate should be 'once every RTT'**,
> since this would capture each change of value. … If the flows have different
> RTTs, then … we recommend the sampling interval to be equal to the **minimum
> RTT**."

**That is a published, derived answer to "how long should the anchor's window
be?"** — a question the tree has never posed as a control question. §16.59
measured `K` drifting 1.04 → 1.505 purely by *transfer length*, i.e. the
estimator window interacting with the standing queue: **the same class of defect
RED's averaging flaw is.** And note the direction of Misra's warning — *higher
capacity makes the averaged estimate track the instantaneous queue more
closely* — which predicts the drift is worse at fast cells.

**(c) A useful negative finding: PIE's target has no justification either.**
RFC 8033's control law is quoted verbatim as

> `p = alpha * (current_qdelay - QDELAY_REF) + beta * (current_qdelay - PIE->qdelay_old_);`

with the honest tuning rule *"**if we cut T_UPDATE in half, we should also cut
alpha by half and increase beta by alpha/4**"* and defaults `alpha = 1/8`,
`beta = 1+1/4`, `QDELAY_REF = 15 ms`, `T_UPDATE = 15 ms`. **But the research
pass verified, by exhaustive search, that RFC 8033 gives NO justification
anywhere for the 15 ms target** — it is asserted with SHOULD and no RTT-relative
argument. **So the standards-track AQM RFC has exactly our problem: a delay
setpoint as an absolute millisecond constant with no derivation.** CoDel (item
4) is the one that derives its setpoint; PIE is not. **This is worth recording
precisely because it stops the CoDel comparison from reading as "everyone else
has this solved."**

**IMPLICATION.** Adopt no numbers. Record two design constraints that family 2
currently lacks and that cost nothing to state: **(i) the cap's own reaction
bandwidth is bounded by `≈0.85/R₀` independent of the setpoint; (ii) the
anchor's averaging window is a control parameter, and the published guidance is
to sample on the order of the minimum RTT, not on a fixed wall-clock window.**
Item (ii) is directly testable against §16.59's `K` drift with no VM.

---

## CD-6. Recovery clocks as SEQUENTIAL CHANGE DETECTION — the principled answer to "how long to wait"

**TRANSLATION TABLE**

| ours | sequential analysis |
|---|---|
| "has the ack failed to come back?" | change-point detection |
| `2·SRTT` clamp `[25,100] ms` | a fixed stopping rule |
| spurious retransmit | false alarm |
| late loss detection | detection delay |
| `9/8`, `min_rtt/4`, `[25,100] ms` | tuned thresholds |

**THEIRS — Lorden, *Ann. Math. Statist.* 42(6):1897–1908, 1971** (transcribed
from the scanned Annals pages). The criterion, p. 1897:

> "**subject to E₀N ≥ γ, we seek to minimize Ē₁N**"

**THEOREM 1, p. 1899:**

> "Then N\*(γ) is a stopping variable, (7) E₀N\*(γ) ≥ γ for all γ, and for all
> θ ∈ Θ {N\*(γ), γ > 1} **minimizes Ē_θN\*(γ) asymptotically subject to (7)**, by
> virtue of the relation (8) **Ē_θN\*(γ) ~ log γ / I(θ) as γ → ∞.**"

**Moustakides, *Ann. Statist.* 14(4):1379–1387, 1986**, abstract:

> "**It is shown that Page's stopping time is optimal for the detection of
> changes in distributions, in a well defined sense.**"

with the CUSUM recursion (4) `S₀ = 0, S_n = max{S_{n−1}, 1}·l(X_n)`, and the
exact-optimality statement p. 1382: *"**N_P minimizes D̄(N) by simultaneously
minimizing its numerator and maximizing its denominator.**"*

**Wald & Wolfowitz, *Ann. Math. Statist.* 19(3), 1948**, Summary:

> "**of all tests with the same power the sequential probability ratio test
> requires on the average fewest observations.**"

**VERDICT: NO COUNTERPART IN OUR FIELD — and that is the finding. There is a
mature optimality theory for exactly our question, and transport does not use
it.**

**(a) The theory answers our question exactly.** *"How long should the sender
wait before declaring the ack failed?"* is quickest-change-detection. Lorden's
Theorem 1 gives the answer in closed form: **subject to a false-alarm budget γ,
the minimum achievable worst-case detection delay is `log γ / I(θ)`**, where `I`
is the Kullback–Leibler information per observation, and CUSUM achieves it —
exactly, per Moustakides. **Every constant in our recovery plane (`9/8`,
`min_rtt/4`, `2·SRTT`, `[25,100] ms`) is a hand-set point on a curve this theory
characterises.**

**(b) And the theory's parameter is one we already report.** RACK's design
budget — *"bound such spurious recoveries to approximately once every 16
recoveries (**less than 7%**)"* — **is a false-alarm rate, i.e. Lorden's `γ`,
chosen by hand.** So RFC 8985 is already reasoning in this framework without
naming it. **That is the cleanest possible bridge: our recovery clocks could be
specified by declaring the spurious-retransmit budget (a contract quantity, like
δ and ρ) and deriving the threshold, rather than by clamping milliseconds.**

**(c) A verified negative finding, and it is a genuine opportunity.** The
research pass found **no published application of SPRT or quickest-change
detection to TCP timeouts or transport loss detection** `[searched; not found]`.
CUSUM appears in network *security* anomaly detection, not in loss recovery.
**This is one of the few places in this entire document where the literature does
NOT already have our answer.**

**(d) The systems literature's continuous answer, and it satisfies CLAUDE.md's
invariant.** Hayashibara, Défago, Yared & Katayama, SRDS 2004, §4.1:

> "**φ(t_now) =def −log₁₀(P_later(t_now − T_last))**"

> "**Instead of providing information of a binary nature (trust vs. suspect),
> accrual failure detectors output a suspicion level on a continuous scale.** The
> principal merit of this approach is that it favors a nearly complete
> decoupling between application requirements and the monitoring of the
> environment."

**That is precisely the shape THE NO-MODE-SWITCH INVARIANT demands** — a
continuous scalar rather than a threshold that selects behaviour — and it is
*already the shape of `p_lost`*, which ADR-0071 candidate (a′) uses to weight
the slack continuously. **The φ-accrual detector is published prior art for
"replace a timeout with a continuous suspicion level", and our engine already
computes such a scalar on every emission.** Chen, Toueg & Aguilera (DSN 2000)
add the QoS vocabulary — **detection time `T_D`, mistake recurrence time
`T_MR`, mistake duration `T_M`** — which is the right way to *report* a recovery
clock's quality and which our DIAG does not currently produce.

**IMPLICATION.** The most intellectually valuable mapping in the document, and
the least immediately actionable. Nothing to adopt today. **What it changes is
the shape of the question**: recovery-clock constants should be derived from a
declared false-alarm budget (RACK's own <7 %, or a contract dial) rather than
tuned, and the reporting vocabulary (`T_D`, `T_MR`, `T_M`) is free to adopt.
**Given (c), a derived recovery clock in this framework would be a genuine
contribution rather than a re-derivation — the only such item this
cross-check found.**

---

## CD-8. Cross-path loss attribution as DATA ASSOCIATION — and the honest correction

**TRANSLATION TABLE**

| ours | tracking / sensor fusion |
|---|---|
| a lost symbol with unknown path | a measurement of unknown origin |
| per-path loss estimate | per-target state estimate |
| shared `batch_seq` space | unlabelled measurement stream |
| §16.24's per-path serial namespace | tagging the measurement at source |

**THEIRS — Reid, IEEE Trans. Automatic Control AC-24(6):843–854, 1979, p. 843:**

> "**The foremost difficulty in the application of multiple-target tracking
> involves the problem of associating measurements with the appropriate
> tracks**, especially when there are missing reports…, unknown targets…, and
> false reports (from clutter)."

**On whether tagging fixes it — Bar-Shalom, Kirubarajan & Gokberk, IEEE Trans.
AES 41(3), 2005:**

> "**Target class information … can also be used to improve data association to
> give better tracking accuracy. The use of target class information in data
> association can improve discrimination by yielding purer tracks and preserving
> their continuity.**"

**VERDICT: CONFIRMS §16.58's framing — with a correction to the "tag at source"
claim that I am recording against my own brief.**

**(a) The mapping is exact and the field is old.** Estimating per-path loss from
an unlabelled shared sequence space *is* measurement-origin uncertainty, the
problem MHT and JPDA exist to solve. §16.58's *"it cannot attribute a seq that
arrived NOWHERE"* is measurement-origin uncertainty in the hardest case: the
measurement does not merely lack a label, **it does not exist.**

**(b) HONEST CORRECTION.** My brief asked the research pass to confirm that "the
established fix is to tag the observation at source rather than infer the
assignment." **No published sentence asserting that was found** `[verified
absent, not merely unfound]`. What exists is the weaker, quoted result above:
class information *improves* association. **The strong claim is folklore and
this document does not make it.**

The honest and still-strong framing: **the entire PDA/JPDA/MHT machinery exists
only because measurements arrive unlabelled.** A per-path sequence namespace
removes the problem the machinery exists to mitigate — which is a statement
about the problem, not a citable theorem about the fix. **§16.24 built exactly
that namespace and it was refuted at runtime for cost (×2.4 sender CPU) and for
cadence re-heating, not for being the wrong idea; §16.58's contribution is
noticing the sender never had the problem at all.** Both remain correct after
this cross-check.

**(c) The identifiability half is item 8's** and is the rigorous version of the
same statement: `A` is not full rank, so per-path parameters are not
identifiable without either induced correlation or regularisation.

**IMPLICATION.** Documentation only. Record Reid 1979 and the tomography
citations as the formal framing for §16.58's non-recoverability argument, **and
record that the "tagging trivialises association" claim is not supported** so it
does not enter the paper as received wisdom.

---

## CD-7. The N² escape as DIMENSIONAL ANALYSIS — the review standard ADR-0070 reinvented

**OURS:** ADR-0070's postmortem — a law quadratic in `N` where its own doc
comment described a linear quantity, surviving a month of exhaustive
measurement because *"nobody ever reviewed the formula as a formula."* The
prevention kit's items 4 and 5 (CLAUDE.md FORMULA-FIRST; MEASUREMENT DISCIPLINE
17 and 18) were derived from first principles.

**THEIRS — Buckingham, *Physical Review* 4(4):345–376, 1914, §2**
(`[archive.org OCR; subscripts should be re-checked against the APS PDF before
print]`):

> "By reason of the principle of dimensional homogeneity, every complete physical
> equation … is reducible to the form (9) in which [Π₁] = [Π₂] = ⋯ = [Πᵢ] = [1]
> … the number of products Π which appear as independent variables in equation
> (9) is **i = n − k**."

`[NOTE: the paper contains no boxed "Pi Theorem" in the modern form; this is the
actual argument.]`

**Rayleigh, "The Principle of Similitude", *Nature* 95:66–68, 1915, verbatim:**

> "**I have often been impressed by the scanty attention paid even by original
> workers in physics to the great principle of similitude. It happens not
> infrequently that results in the form of 'laws' are put forward as novelties
> on the basis of elaborate experiments, which might have been predicted a
> priori after a few minutes' consideration.**"

**Kennedy, *Programming Languages and Dimensions*, PhD thesis, Cambridge
UCAM-CL-TR-391, 1996, p. 1, verbatim:**

> "**Dimensions are to science what types are to programming. In science and
> engineering, dimensional consistency provides a first check on the correctness
> of an equation or formula**, just as in programming the typability of a program
> or program fragment eliminates one possible reason for program failure."

**VERDICT: CONFIRMS — and the discipline is 110 years old.** ADR-0070's rule
*"check that the sentence and the expression agree in SHAPE (order in N, units,
monotonicity) before looking at any number"* **is dimensional analysis applied
to a dimensionless parameter (the path count).** Rayleigh's sentence is, almost
word for word, ADR-0070's postmortem: an elaborate experimental programme
producing a "law" that a few minutes' consideration of its scaling would have
settled. **The `×N` defect is a scaling-exponent error — the exact class this
discipline exists to catch, and the exact class our nine always-on absolute pins
could not see, because every one of them was an equality at fixed `N ∈ {1,2}`.**

**The canonical cautionary citation, verbatim — Mars Climate Orbiter Mishap
Investigation Board, Phase I Report, 10 Nov 1999, pp. 6, 16:**

> "**The MCO MIB has determined that the root cause for the loss of the MCO
> spacecraft was the failure to use metric units in the coding of a ground
> software file**, 'Small Forces,' used in trajectory models."

and — the sentence that makes it our postmortem rather than merely a famous
accident:

> "**Unfortunately for MCO, the root cause was not caught by the processes
> in-place in the MCO project**"

with Contributing Cause 8: *"End-to-end testing to validate the small forces
ground software performance and its applicability to the specification did not
appear to be accomplished."* **A dimensional error that every existing process
passed** — ADR-0070 mechanism 5, in 1999, with a spacecraft.

**The sharpest one-liner for our purpose — Bentley, "Programming Pearls: The
Envelope Is Back", *CACM* 29(3):176–182, March 1986, p. 178**, set off on its
own line in the original:

> "**Dimension tests check the form of equations.**"

**And the exact structural analogue of the prevention item — Roy, "Review of
code and solution verification procedures for computational simulation",
*J. Comput. Phys.* 205:131–156, 2005, §2.3:**

> "**The most rigorous code verification test is the order of accuracy test**,
> which determines whether or not the discretization error is reduced at the
> expected rate. … Since it is **the most difficult test to satisfy (and
> therefore the most sensitive to coding mistakes)**, the order of accuracy test
> is the recommended acceptance test for code verification."

**That is MEASUREMENT DISCIPLINE 17, arrived at independently in scientific
computing.** Magnitude-plausibility checks pass indefinitely; a *rate-of-change*
check across the governing parameter fails on the first two honest data points.
Our law-shape template sweeping `N = 1…8` synthetically is an order-of-accuracy
test in `N`.

> **CORRECTION TO MY OWN BRIEF, recorded rather than quietly dropped.** I asked
> the research pass to confirm Kennedy argues "dimension checking catches a
> class of errors no test catches." **No such sentence exists in Kennedy**
> `[verified absent]`; his framing is static-checking versus runtime failure.
> The stronger claim is not made here. Kennedy's own later note (CEFP'09) is
> pointed in a different and more useful direction: on the MCO report, *"**Notably
> absent … was any suggestion that programming languages might assist in the
> prevention of such errors**, either through static analysis tools, or through
> type-checking."*

**IMPLICATION.** Mostly editorial and free: CLAUDE.md's FORMULA-FIRST rule and
MEASUREMENT DISCIPLINE 17 gain an established name and a 110-year citation
lineage instead of standing as house rules — which matters for the paper, since
a reviewer will recognise the discipline. **The one concrete upgrade the analogy
suggests: MEASUREMENT DISCIPLINE 17's law-shape test should assert the EXPONENT
of each governing quantity (`N`, `rate`, `RTprop`, `srtt`), not merely
monotonicity and continuity.** `cap(2N)/cap(N) = 2` is a dimensional assertion
and is strictly stronger than "monotone in N"; the tree's template already
sweeps `N = 1…8` synthetically, so this strengthens an existing test rather than
adding an instrument.

---

# PART III — THE SCORECARD

## Verdicts, one line each

| # | ours | theirs | verdict |
|---|---|---|---|
| 1 | `2·Σ bwᵢ·RTTᵢ` + span | RFC 6182 §5.3 `2·Σ BW_i·RTT_max` (**send** buffer too) | **AGREE** on ×2 and shape; **DIVERGE** on clock — ours is `RTTᵢ` + a HALF-sized span |
| 2 | `span = rate_fast·(RTT_max − RTT_min)` | BLEST `X ≈ rate_fast·RTT_slow`, adapted by measured λ < 1 | **DIVERGE** in magnitude (theirs 2×), **AGREE** in structure (zero at equal delay); our wire says even ours is 45 % over-funded |
| 3 | `gain = 2.0` "recovery runway" | BBR `cwnd_gain = 2` (ACK aggregation / rate doubling); RFC 6182 ×2 | **AGREE** on value, **DIVERGE** on derivation — our stated rationale is in no primary source |
| 4 | δ permits `b·RTprop` standing queue (50–200 % RTT) | CoDel **5–10 % of RTT**, derived from Kleinrock power; Copa `1.25/δ` **packets** | **DIVERGE 10–40×**; CoDel's derivation predicts §16.57's measurement |
| 5 | `17/8·srtt` standing slack | RFC 6182 rejects worst-case (`RTO_max`) provisioning as "too expensive" | **AGREE** the reserve exists; literature **rejects** provisioning it for the worst case, permanently |
| 6 | `2·SRTT` clamped `[25,100] ms` | RFC 8985 §7.2 `PTO = 2 * SRTT`; bounds `min_rtt/4` … `SRTT` | **AGREE** on the base (verbatim); **DIVERGE** on the clamp — theirs is RTT-relative, ours absolute |
| 7 | `ε̂ = 1 − Δrecv/Δsent`, retransmits in the denominator | RFC 6675 *"retransmitted … counted twice"*; Allman et al. 2003 measure >100 % error in 16 % of transfers; every published estimator uses **newly-delivered** | **DIVERGE** — three published bias sources, all inflating ε̂, all path-count-independent |
| 8 | per-path loss from a shared sequence space | tomography: no unique mapping unless `A` full rank; **MPQUIC draft-02 §9.1 states our exact case**; draft-14 abandons the shared space | **AGREE** — confirms §16.58 on all three of its positions |
| 9 | Copa inside a δ-budgeted cap | cascade separation 4–10× (Skogestad 2023); gain ∝ 1/delay (Vinnicombe, Low); Copa's own proved `C·Σ1/δ < BDP` | **NO COUNTERPART** — the topology appears **un-analysed**; our separation holds by accident |
| 10 | `knee = 2048/path`; `boot = 128` | DRS / Linux autotuning: **no knee at all**; RFC 6928 IW = 10 | **NO COUNTERPART** for the knee — the published architecture is ADR-0071 family 2's |
| CD-1 | slack `rate·stall`, linear in mean | newsvendor `Q* = F⁻¹(c_u/(c_u+c_o))`; base stock `z·σ·√L` | **CONFIRMS** payout-zero ⇒ `Q* = 0`; **SHARPENS** — wrong SHAPE (mean vs dispersion, linear vs √) |
| CD-2 | span as buffer sizing | ROB `W/D` coverage; balanced design `W ∝ D²` | **CONFIRMS** units; **CONTRADICTS** the linear form; third field to call the term negligible |
| CD-3 | the c8 dead wall | metastable failure; `Cstable = Cnorm/(w*L·w*C)`; RFC 896 *"This condition is stable"* | **CONFIRMS** — named class, quantified gap, published mitigation list |
| CD-4 | δ setpoint enforcement speed | Hollot `ω_g·R₀ < 0.85`; RED averaging flaw | **SHARPENS** — family 2 specifies the setpoint but not the bandwidth bound |
| CD-5 | one pooled cap (ADR-0058) | Eppen 1979 √N risk pooling; Fukuda 1964 slow-lead-time base | **CONFIRMS** the pooled decision — with an untested correlation condition |
| CD-6 | recovery clocks | Lorden/Moustakides CUSUM optimality `log γ / I`; φ-accrual | **NO COUNTERPART IN TRANSPORT** — a real opportunity |
| CD-7 | FORMULA-FIRST / discipline 17 | Buckingham 1914, Rayleigh 1915, Roy 2005 order-of-accuracy | **CONFIRMS** — 110-year-old discipline, independently re-derived |
| CD-8 | cross-path attribution | Reid 1979 measurement-origin uncertainty | **CONFIRMS** framing; "tag at source" claim **NOT SUPPORTED** |

## What the literature settles outright — things we have been deriving from scratch

1. **The standing-queue setpoint has a derived published value: 5–10 % of RTT**
   (RFC 8289 §3.2, from Kleinrock power maximisation). §16.57's open derivation
   question, §16.59's successor and ADR-0071 family 2's central premise all
   point at a quantity that was settled in 2018 — and the derivation *predicts*
   the goodput-parity/worse-latency result we measured.
2. **`gain = 2.0` has two published derivations** (RFC 6182 §5.3; BBR
   `DefaultCwndGain`). ADR-0070 finding 3's "fossil" verdict can be discharged
   by citation, not by measurement — and our own stated rationale should be
   corrected, since it appears in no primary source.
3. **`2·SRTT` is RFC 8985's TLP PTO verbatim, with the derivation we lacked.**
   Only the `[25,100] ms` clamp is ours.
4. **The optimal reserve under zero payout is exactly zero** (newsvendor
   critical fractile), and **the reserve's correct functional form is
   dispersion-driven and sub-linear in lead time** (base stock). Both are closed
   forms; neither required a battery.
5. **Pooling beats per-path accounts, with a √N law** (Eppen 1979) — ADR-0058's
   three-experiment refutation chain rediscovered a 1979 theorem.
6. **The dead wall is a named failure class** with a definition, a quantified
   hysteresis gap, and a published mitigation list — and *"add memory"* is
   explicitly rejected on it (RFC 896, 1984).
7. **A knee is not part of the published architecture at all.** Linux has sized
   receive buffers with no per-path ceiling, plus a separate administrative
   memory bound, for two decades.
8. **`9/8` is empirical, not derived** — which corrects our own record.

## The prioritized ADOPT-OR-TEST list, cheapest first

**Tier 0 — free. Documentation and record corrections; no measurement, no code.**

| # | action | why |
|---|---|---|
| 0.1 | Correct the `9/8` record: it is a **cited empirical recommendation**, not a derivation; RACK uses 5/4; RFC 9002 invites experiment. Soften "zero fitted constants" wherever it appears (ADR-0070 Deliverable 2, §16.43, §16.56). | Our own claim is stronger than its source. |
| 0.2 | Cite RFC 6182 §5.3 + Raiciu NSDI'12 §4.2 + BBR `DefaultCwndGain` as provenance for `gain = 2.0` and the law's shape; **delete the unsupported "recovery runway" rationale** from `sender_policy.rs`'s comment. | Discharges ADR-0070 finding 3 without a measurement. |
| 0.3 | Cite RFC 8985 §7.2 for `2·SRTT`. | The base of both recovery clocks is a standard. |
| 0.4 | Record in ADR-0071 that family 2's *architecture* (law + separate resource bound, no knee) is Linux's shipped design, and that the knee has no counterpart. | Prior art for an open proposal, in its support column. |
| 0.5 | Record Eppen 1979 as prior art for ADR-0058; Reid 1979 + tomography for §16.58; metastable-failure vocabulary for the dead wall. | Paper-ready framing the arc currently lacks. |
| 0.6 | Record that **no published source writes our span decomposition** `Σ bwᵢ(RTT_max − RTTᵢ)`. | Prevents a mis-citation in the paper. |

**Tier 1 — cheap tests against data we may already have. No VM.**

| # | test | falsifier |
|---|---|---|
| 1.1 | **Score the ladder's existing curves at the CoDel rung**: `BDP·1.05` per cell (c1 ≈ 184, sc2 ≈ 344, c7 ≈ 1161, c8 ≈ 1685, c8L ≈ 5225 symbols). | If goodput at those rungs is at parity, the δ dial is mis-scaled 10–40× and family 2's dial needs re-basing. **Highest value in the document.** |
| 1.2 | **Recompute ε̂ with first-transmissions-only in the denominator**, from the ladder's captured `[ACKDIAG]` cursors. Three published bias sources predict the sign and the path-count-independence; Allman et al. 2003 measured the same bias at >100 % in 16 % of transfers. | If the 20× collapses, §16.58 is repaired rather than abandoned; if not, this hypothesis dies and §16.63's anomaly stays open. |
| 1.3 | **`slack_bench.rs` idle-vs-backlog replay scored against the newsvendor prediction** (`c_u ≈ 0 ⇒ reserve ≈ 0`) rather than a coverage point. 576 cells, 13 s, no VM. | Non-zero idle at `W + S` anywhere means the payout was not zero. |
| 1.4 | **The `K`-drift-vs-window test**: read the `[3T]` `window=` series *within* one c8L run at t ≈ 2.5 s and t ≈ 20 s (ADR-0071 finding 2 already names this). | Confirms the estimator window is a control parameter (CD-4) and that c8 measures warm-up. |
| 1.5 | **Strengthen MEASUREMENT DISCIPLINE 17's law-shape test to assert EXPONENTS** in each governing quantity, not just monotonicity. | An order-of-accuracy test in `N`; strengthens an existing instrument. |

**Tier 2 — derivations, not batteries. Write the formula first, per FORMULA-FIRST.**

| # | item |
|---|---|
| 2.1 | **RACK-shaped recovery clocks**: replace `[25,100] ms` with `min(2·SRTT, …)` bounded relatively — a `min_rtt/4`-shaped floor and an `SRTT`-shaped ceiling. `RWM_DERIVED_SWEEP` is two-thirds of the way there and already has its liveness echo. |
| 2.2 | **State the cascade constraint** on ADR-0068 × ADR-0071-family-2: inner loop ≥5× faster (Skogestad p. 387) and the cap's own bandwidth bounded by `≈0.85/R₀` (Hollot). Currently satisfied by accident. |
| 2.3 | **Name the missing queue-draining guarantee.** BBR has ProbeRTT, Copa has its 5-RTT oscillation, CoDel has a 5 % target; we have none, and §16.59's `K` 1.04 → 1.505 is the measured consequence. |
| 2.4 | **The δ unit mismatch** between Copa's `1.25/δ` packets and our `b·RTprop` — resolve before ADR-0068 fuses the two, since both call it δ. |

**Tier 3 — measurement, and each is a new axis rather than a re-run.**

| # | item |
|---|---|
| 3.1 | **Cross-path correlation of stall/loss events** (Eppen's condition). Does the pooling advantage track √N or collapse toward 1? Reframes ADR-0058's c7-vs-c8 split from anomaly to prediction. |
| 3.2 | **The metastable amplification factor** `w*L·w*C` at c8, instead of tuning the cliff. Report the **collapse RATE** (§16.63's 0/27 vs 2/21 already is one), not a mean — the literature's own reporting convention, and the answer to the bistable-statistic problem. |
| 3.3 | **A recovery clock derived from a declared spurious-retransmit budget** (Lorden's γ; RACK's own <7 %). The one item in this document where the literature does **not** already have our answer. |

## Folklore corrected — nine places the repeated version is not the source

1. **BBR's `cwnd_gain = 2` is not about recovery headroom.** No primary source says so; it is ACK aggregation (v1) or rate doubling (v2/v3). **This is our own comment's claim.**
2. **RFC 6824/8684 omit the ×2** and call the undoubled quantity "a tight upper bound", contradicting RFC 6182 — and declare the question open.
3. **RFC 9002's `9/8` has no derivation**: *"Experience with QUIC shows that 9/8 works well."* RACK uses 5/4.
4. **RACK's `min_rtt/4` is inherited Linux practice**, not derived; and TLP's delayed-ACK term applies **only when FlightSize is one segment**.
5. **BBR's startup gain `2/ln2 ≈ 2.89` was corrected to `4·ln2 ≈ 2.77`** in v2/v3; Linux still ships 2.89.
6. **Copa's queue is `1.25/δ̂` on average**, not `1/δ` — that is the equilibrium *threshold*; and competitive mode does AIMD on `1/δ`, not δ.
7. **DRS's factor 2 is a slow-start-matching argument**, not RFC 6182's reordering+retransmit split. Current Linux (v6.16+) dropped the loss half of the rationale entirely.
8. **CoDel's derived quantity is the ratio 0.05, not 5 ms.** Porting the millisecond ports nothing.
9. **RFC 2914 §5 lists only TWO collapse forms**; the five-way taxonomy is Floyd & Fall 1999. And **"hysteresis"/"bistable" appear nowhere in the metastability literature** — use `Cstable = Cnorm/(w*L·w*C)`.

Plus four corrections against my own brief or against internal notes, recorded
rather than dropped:

- **Kennedy does not claim dimension checking catches errors testing cannot** —
  his framing is static-checking versus runtime failure.
- **No source states that source-tagging trivialises data association.**
- **Castro et al. 2004 does not contain the loss-specific unicast
  non-identifiability statement** — its rank statements are generic. Cite
  **Coates & Nowak ITC 2000** or **Coates/Hero/Nowak/Yu SP Mag 2002** for the
  loss claim.
- **ECF's fourth author is Richard J. Gibbens**, not "Lee"; and the *Computer
  Networks* 2014 bounded-receive-buffer paper is **Li, Lukyanenko, Tarkoma,
  Cui & Ylä-Jääski**.

**And one methodological warning worth carrying into any future desk research
here:** the fetch-summarisers used by two of the research passes **refused
RFC 6675 outright and silently paraphrased elsewhere.** Every quotation in this
document that matters was obtained by direct text extraction of a primary PDF or
RFC plaintext. A literature cross-check conducted through a summariser would
have produced plausible, wrong quotations for at least three of the constants
above.

---

## What this document concludes

**Nothing, in the decision sense.** It flips no default, adds no gate, touches
no engine file or test, and **explicitly does not adjudicate ADR-0071** — every
candidate in that document is annotated with literature support or literature
tension above, and none is preferred, ranked or recommended. ADR-0071's Status
is unchanged.

Two things it *does* assert, both of which are findings about our record rather
than about the machine:

1. **Three claims in the tree are stronger than their sources support** —
   `9/8` as "cited not fitted", `gain = 2.0`'s recovery-runway rationale, and
   (prospectively) any citation of the MPTCP literature for our span
   decomposition. Tier 0 corrects all three.
2. **One measured refutation (§16.63) now has a candidate mechanism** from a
   standards-track RFC, with a falsifier that needs no VM. It is a hypothesis
   and is labelled as one.

## Consequences for the record

- **The arc's "we re-derived what was known" pattern (Appendix B's headline)
  extends from the FINDINGS to the FORMULAS.** Appendix B concluded that almost
  every *result* of the FEC/ARQ arc was established. This document finds the
  same of the *expressions*: six of ten transport formulas have exact published
  counterparts, and four cross-domain mappings return closed forms we lacked.
  **The honest reading is unchanged from Appendix B's — the value is rigour and
  measurement on our own stack, not novelty — with one addition: CD-6 is a
  place where the literature genuinely does not have our answer.**
- **ADR-0070's postmortem gains a 110-year-old name.** FORMULA-FIRST and
  MEASUREMENT DISCIPLINE 17 are dimensional analysis and the order-of-accuracy
  test. That is reassuring about the rules and unflattering about how long it
  took to write them down.
- **A verification-gap ledger now exists** (below). Roughly a dozen primary
  sources are paywalled and were quoted at abstract level or via secondary
  restatement; each is marked at its point of use, and the OR classics are the
  ones most worth an institutional pull.
- **No engine behaviour changed; zero production lines touched.**

**What would reverse or amend the findings here:** (i) obtaining Eppen 1979's
closed form, Scarf 1960's interior, or Arrow/Harris/Marschak 1951 could
strengthen or qualify CD-1 and CD-5 — both currently rest on abstracts plus
teaching sources; (ii) a primary BBR source stating a recovery rationale for
`cwnd_gain` would withdraw folklore item 1; (iii) measuring Tier 1.2 and finding
the 20× intact would kill item 7's hypothesis outright.

---

## VERIFICATION-GAP LEDGER

Quoted **first-hand from a primary source in this session**: RFC 6182, RFC 6675,
RFC 6928, RFC 8289, RFC 8985, RFC 9002, draft-ietf-ccwg-bbr, Copa (NSDI 2018
PDF), BLEST (IFIP 2016 proceedings PDF), Bronson et al. (HotOS '21 PDF).

Quoted by research workers from primary sources, **not independently
re-verified by me**: Raiciu NSDI '12, Barré IFIP 2011, Kuhn ICC 2014,
RFC 6824/8684, BBR ACM Queue 2016, Fisk & Feng LAUR 00-3321, Linux source
comments, Huang et al. OSDI '22, Google SRE Book, Envoy/Hystrix docs, DAGOR
SoCC '18, Hollot TAC 2002, RFC 8033, Misra SIGCOMM 2000, Firoiu INFOCOM 2000,
Skogestad §10.2, Seborg §16.1, Åström & Murray, Jacobson 1988 (LBL revision),
RFC 6298, Lorden 1971, Moustakides 1986, Wald 1945, Wald–Wolfowitz 1948,
φ-accrual SRDS 2004, Chen/Toueg/Aguilera DSN 2000, Buckingham 1914,
Rayleigh 1915, Kennedy 1996, MCO MIB report, Bentley 1986, Roy 2005,
Cáceres 1999 preprint, Castro 2004, Reid 1979, Low/Peterson/Wang JACM 2002.

**NOT OBTAINED — quoted at abstract level, via secondary/teaching sources, or
not quoted at all. None of these supports a verdict on its own:**

| source | status | affects |
|---|---|---|
| Arrow, Harris & Marschak 1951 | not consulted | CD-1 (critical fractile via teaching source) |
| Eppen 1979 — **closed form** | abstract only | CD-5 (√N law and correlation extension) |
| Scarf 1960 — interior | not consulted | CD-1 (K-convexity via encyclopedia) |
| Clark & Scarf 1960 | not consulted | CD-5 (echelon decomposition) |
| Veeraraghavan & Scheller-Wolf 2008 | fragments | CD-5 (dual-index structure) |
| Sterman 1989 — oscillation condition | abstract only | CD-3/CD-1 (feedback provisioning) |
| Page 1954 (Biometrika) | **no open copy anywhere** | CD-6 (CUSUM quoted via Lorden/Moustakides) |
| Åström & Hägglund, *Advanced PID* | second-hand | CD-4/9 (the "5×" attribution — use Skogestad) |
| Hollot INFOCOM 2001 | paywalled | CD-4 (quoted from TAC 2002 companion) |
| PIE HPSR 2013 | paywalled | CD-4 (only RFC 8033 quoted) |
| Li et al., *Comput. Netw.* 2014 | paywalled | item 2 (bounded receive buffers) |
| ECF, CoNEXT 2017 | not obtained | item 2 (send-decision arithmetic **owed**) |
| Fisk & Feng SC 2001 | archives dead | item 10 (quoted from LAUR 00-3321 instead) |
| Rothenberg 1971 | paywalled | CD-8 (rank condition via restatements) |
| Bar-Shalom & Fortmann 1988 | no access | CD-8 (Reid 1979 used instead) |
| SIGCOMM '88 Jacobson proceedings text | not obtained | CD-6 (LBL Nov 1988 revision used) |

**Authorship correction carried from the research pass:** the *Computer
Networks* 64:1–14 (2014) bounded-receive-buffer paper is **Li, Lukyanenko,
Tarkoma, Cui & Ylä-Jääski** — not the attribution used in earlier internal
notes.

---

## References — paper-ready

**IETF (all fetched from `rfc-editor.org` / `ietf.org`, quotable as printed)**

- A. Ford, C. Raiciu, M. Handley, S. Barré, J. Iyengar, "Architectural Guidelines for Multipath TCP Development," RFC 6182, IETF, March 2011. https://www.rfc-editor.org/rfc/rfc6182.txt
- A. Ford, C. Raiciu, M. Handley, O. Bonaventure, C. Paasch, "TCP Extensions for Multipath Operation with Multiple Addresses," RFC 8684, IETF, March 2020 (and RFC 6824, January 2013).
- E. Blanton, M. Allman, L. Wang, I. Jarvinen, M. Kojo, Y. Nishida, "A Conservative Loss Recovery Algorithm Based on Selective Acknowledgment (SACK) for TCP," RFC 6675, IETF, August 2012. https://www.rfc-editor.org/rfc/rfc6675.txt
- N. Dukkipati, T. Refice, Y. Cheng, J. Chu, T. Herbert, A. Agarwal, A. Jain, N. Sutin, "Increasing TCP's Initial Window," RFC 6928, IETF, April 2013.
- K. Nichols, V. Jacobson, A. McGregor, J. Iyengar, "Controlled Delay Active Queue Management," RFC 8289, IETF, January 2018. https://www.rfc-editor.org/rfc/rfc8289.txt
- Y. Cheng, N. Cardwell, N. Dukkipati, P. Jha, "The RACK-TLP Loss Detection Algorithm for TCP," RFC 8985, IETF, February 2021. https://www.rfc-editor.org/rfc/rfc8985.txt
- J. Iyengar, I. Swett (eds.), "QUIC Loss Detection and Congestion Control," RFC 9002, IETF, May 2021. https://www.rfc-editor.org/rfc/rfc9002.txt
- R. Pan, P. Natarajan, F. Baker, G. White, "Proportional Integral Controller Enhanced (PIE)," RFC 8033, IETF, February 2017.
- V. Paxson, M. Allman, J. Chu, M. Sargent, "Computing TCP's Retransmission Timer," RFC 6298, IETF, June 2011.
- J. Nagle, "Congestion Control in IP/TCP Internetworks," RFC 896, IETF, 6 January 1984.
- S. Floyd, "Congestion Control Principles," RFC 2914 / BCP 41, IETF, September 2000.
- N. Cardwell, Y. Cheng, S. H. Yeganeh, I. Swett, V. Jacobson, "BBR Congestion Control," draft-ietf-ccwg-bbr (BBRv3); and draft-cardwell-iccrg-bbr-congestion-control-00/-01/-02.

**Transport / multipath**

- V. Arun, H. Balakrishnan, "Copa: Practical Delay-Based Congestion Control for the Internet," USENIX NSDI 2018. https://www.usenix.org/conference/nsdi18/presentation/arun
- S. Ferlin, Ö. Alay, O. Mehani, R. Boreli, "BLEST: Blocking Estimation-based MPTCP Scheduler for Heterogeneous Networks," IFIP Networking 2016. https://dl.ifip.org/db/conf/networking/networking2016/1570234725.pdf
- C. Raiciu, C. Paasch, S. Barré, A. Ford, M. Honda, F. Duchene, O. Bonaventure, M. Handley, "How Hard Can It Be? Designing and Implementing a Deployable Multipath TCP," USENIX NSDI 2012.
- S. Barré, C. Paasch, O. Bonaventure, "MultiPath TCP: From Theory to Practice," IFIP Networking 2011, LNCS 6640.
- N. Kuhn, E. Lochin, A. Mifdaoui, G. Sarwar, O. Mehani, R. Boreli, "DAPS: Intelligent Delay-Aware Packet Scheduling for Multipath Transport," IEEE ICC 2014.
- Y. S. Li, A. Lukyanenko, S. Tarkoma, Y. Cui, A. Ylä-Jääski, "Tolerating path heterogeneity in multipath TCP with bounded receive buffers," *Computer Networks* 64:1–14, 2014. **[not obtained]**
- N. Cardwell, Y. Cheng, C. S. Gunn, S. H. Yeganeh, V. Jacobson, "BBR: Congestion-Based Congestion Control," *ACM Queue* 14(5), 2016 / *CACM* 60(2), 2017.
- V. Jacobson, M. Karels, "Congestion Avoidance and Control," ACM SIGCOMM 1988 (LBL revised version, November 1988). https://ee.lbl.gov/papers/congavoid.pdf
- M. Fisk, W. Feng, "Dynamic Adjustment of TCP Window Sizes," LANL Tech. Report LAUR 00-3321, 2000.
- S. H. Low, L. L. Peterson, L. Wang, "Understanding TCP Vegas: A Duality Model," *J. ACM* 49(2):207–235, 2002.
- Y. Lim, E. M. Nahum, D. Towsley, R. J. Gibbens, "ECF: An MPTCP Path Scheduler to Manage Heterogeneous Paths," ACM CoNEXT 2017.
- M. Allman, W. M. Eddy, S. Ostermann, "Estimating Loss Rates With TCP," *ACM Performance Evaluation Review* 31(3):12–24, 2003. https://www.icir.org/mallman/pubs/AEO03/AEO03.pdf
- N. Cardwell, Y. Cheng, S. H. Yeganeh, V. Jacobson, "Delivery Rate Estimation," draft-cheng-iccrg-delivery-rate-estimation-02, IETF, March 2022.
- M. Dong, Q. Li, D. Zarchy, P. B. Godfrey, M. Schapira, "PCC: Re-architecting Congestion Control for Consistent High Performance," USENIX NSDI 2015.
- Q. De Coninck, O. Bonaventure et al. (eds.), "Multipath Extension for QUIC," draft-ietf-quic-multipath (-02, July 2022; -14, April 2025), IETF.
- T.-Y. Huang, N. Handigol, B. Heller, N. McKeown, R. Johari, "Confused, Timid, and Unstable: Picking a Video Streaming Rate is Hard," ACM IMC 2012, pp. 225–238.
- G. Vinnicombe, "On the stability of end-to-end congestion control for the Internet," Cambridge Univ. Eng. Dept. Tech. Report CUED/F-INFENG/TR.398, 2000.
- S. H. Low, F. Paganini, J. C. Doyle, "Internet Congestion Control," *IEEE Control Systems Magazine* 22(1):28–43, 2002.
- S. Skogestad, "Advanced control using decomposition and simple elements," *Annual Reviews in Control* 56:100903, 2023.

**AQM / control theory**

- C. V. Hollot, V. Misra, D. Towsley, W. Gong, "Analysis and Design of Controllers for AQM Routers Supporting TCP Flows," *IEEE Trans. Automatic Control* 47(6):945–959, 2002; and "On Designing Improved Controllers for AQM Routers Supporting TCP Flows," IEEE INFOCOM 2001.
- V. Misra, W. Gong, D. Towsley, "Fluid-based Analysis of a Network of AQM Routers Supporting TCP Flows with an Application to RED," ACM SIGCOMM 2000.
- V. Firoiu, M. Borden, "A Study of Active Queue Management for Congestion Control," IEEE INFOCOM 2000.
- S. Skogestad, I. Postlethwaite, *Multivariable Feedback Control: Analysis and Design*, 2nd ed., Wiley, 2005, §10.2 p. 387. https://folk.ntnu.no/skoge/book/ps/bookall.pdf
- D. E. Seborg, T. F. Edgar, D. A. Mellichamp, F. J. Doyle III, *Process Dynamics and Control*, 4th ed., Wiley, §16.1.
- K. J. Åström, R. M. Murray, *Feedback Systems*, 2nd ed., Princeton, 2020.

**Operations research / inventory**

- J. D. C. Little, "A Proof for the Queuing Formula: L = λW," *Operations Research* 9(3):383–387, 1961.
- G. D. Eppen, "Note—Effects of Centralization on Expected Costs in a Multi-Location Newsboy Problem," *Management Science* 25(5):498–501, 1979.
- K. J. Arrow, T. Harris, J. Marschak, "Optimal Inventory Policy," *Econometrica* 19(3):250–272, 1951. **[not consulted]**
- H. Scarf, "The Optimality of (S,s) Policies in the Dynamic Inventory Problem," in Arrow, Karlin, Suppes (eds.), *Mathematical Methods in the Social Sciences*, Stanford UP, 1960. **[not consulted]**
- A. J. Clark, H. Scarf, "Optimal Policies for a Multi-Echelon Inventory Problem," *Management Science* 6(4):475–490, 1960. **[not consulted]**
- Y. Fukuda, "Optimal Policies for the Inventory Problem with Negotiable Leadtime," *Management Science* 10(4):690–708, 1964.
- S. Veeraraghavan, A. Scheller-Wolf, "Now or Later: A Simple Policy for Effective Dual Sourcing in Capacitated Systems," *Operations Research* 56(4):850–864, 2008.
- A. Sheopuri, G. Janakiraman, S. Seshadri, "New Policies for the Stochastic Inventory Control Problem with Two Supply Sources," *Operations Research* 58(3):734–745, 2010.
- H. L. Lee, V. Padmanabhan, S. Whang, "Information Distortion in a Supply Chain: The Bullwhip Effect," *Management Science* 43(4):546–558, 1997.
- J. D. Sterman, "Modeling Managerial Behavior: Misperceptions of Feedback in a Dynamic Decision Making Experiment," *Management Science* 35(3):321–339, 1989.
- F. Chen, Z. Drezner, J. K. Ryan, D. Simchi-Levi, "Quantifying the Bullwhip Effect in a Simple Supply Chain," *Management Science* 46(3):436–443, 2000.
- S. Bimpikis, M. G. Markakis, "Inventory Pooling Under Heavy-Tailed Demand," *Management Science* 62(6), 2016.

**Computer architecture**

- T. Karkhanis, J. E. Smith, "A First-Order Superscalar Processor Model," ISCA-31, 2004.
- S. Eyerman, L. Eeckhout, T. Karkhanis, J. E. Smith, "A Mechanistic Performance Model for Superscalar Out-of-Order Processors," *ACM TOCS* 27(2), Article 3, 2009.
- P. Michaud, A. Seznec, S. Jourdan, "An Exploration of Instruction Fetch Requirement in Out-of-Order Superscalar Processors," *Int'l J. Parallel Programming* 29(1), 2001.
- Y. Chou, B. Fahs, S. Abraham, "Microarchitecture Optimizations for Exploiting Memory-Level Parallelism," ISCA 2004.
- H. Akkary, R. Rajwar, S. T. Srinivasan, "Checkpoint Processing and Recovery: Towards Scalable Large Instruction Window Processors," MICRO-36, 2003.
- M. D. Hill, "Three Other Models of Computer System Performance," arXiv:1901.02926, 2018.
- N. Santos, A. Schiper, "Optimizing Paxos with batching and pipelining," *Theoretical Computer Science* 496:170–183, 2013.

**Metastability / overload**

- N. Bronson, A. Aghayev, A. Charapko, T. Zhu, "Metastable Failures in Distributed Systems," ACM HotOS '21, pp. 221–227. https://sigops.org/s/conferences/hotos/2021/papers/hotos21-s11-bronson.pdf
- L. Huang, M. Magnusson, A. B. Muralikrishna, S. Estyak, R. Isaacs, A. Aghayev, T. Zhu, A. Charapko, "Metastable Failures in the Wild," USENIX OSDI '22, pp. 73–90.
- B. Beyer, C. Jones, J. Petoff, N. R. Murphy (eds.), *Site Reliability Engineering*, O'Reilly, 2016, Ch. 21–22.
- H. Zhou, M. Chen, Q. Lin, Y. Wang, X. She, S. Liu, R. Gu, B. C. Ooi, J. Yang, "Overload Control for Scaling WeChat Microservices," ACM SoCC 2018.

**Sequential detection / failure detectors**

- E. S. Page, "Continuous Inspection Schemes," *Biometrika* 41(1/2):100–115, 1954. **[not obtained]**
- G. Lorden, "Procedures for Reacting to a Change in Distribution," *Ann. Math. Statist.* 42(6):1897–1908, 1971.
- G. V. Moustakides, "Optimal Stopping Times for Detecting Changes in Distributions," *Ann. Statist.* 14(4):1379–1387, 1986.
- A. Wald, "Sequential Tests of Statistical Hypotheses," *Ann. Math. Statist.* 16(2):117–186, 1945; A. Wald, J. Wolfowitz, "Optimum Character of the Sequential Probability Ratio Test," *Ann. Math. Statist.* 19(3):326–339, 1948.
- N. Hayashibara, X. Défago, R. Yared, T. Katayama, "The φ Accrual Failure Detector," IEEE SRDS 2004, pp. 66–78.
- W. Chen, S. Toueg, M. K. Aguilera, "On the Quality of Service of Failure Detectors," IEEE DSN 2000 / *IEEE Trans. Computers* 51(5), 2002.

**Tomography / estimation / data association**

- R. Cáceres, N. G. Duffield, J. Horowitz, D. Towsley, "Multicast-Based Inference of Network-Internal Loss Characteristics," *IEEE Trans. Information Theory* 45(7):2462–2480, 1999.
- R. Castro, M. Coates, G. Liang, R. Nowak, B. Yu, "Network Tomography: Recent Developments," *Statistical Science* 19(3):499–517, 2004.
- M. Coates, R. Nowak, "Network Loss Inference Using Unicast End-to-End Measurement," ITC Conf. on IP Traffic, Modelling and Management, 2000, paper 28. **[the loss-specific unicast non-identifiability result]**
- M. Coates, A. O. Hero III, R. Nowak, B. Yu, "Internet Tomography," *IEEE Signal Processing Magazine* 19(3):47–65, 2002.
- E. S. Allman, C. Matias, J. A. Rhodes, "Identifiability of parameters in latent structure models with many observed variables," *Annals of Statistics* 37(6A):3099–3132, 2009 (arXiv:0809.5032).
- R. Gupta, R. Kumar, S. Vassilvitskii, "On Mixtures of Markov Chains," NIPS 2016.
- D. B. Reid, "An Algorithm for Tracking Multiple Targets," *IEEE Trans. Automatic Control* AC-24(6):843–854, 1979.
- Y. Bar-Shalom, T. Kirubarajan, C. Gokberk, "Tracking with Classification-Aided Multiframe Data Association," *IEEE Trans. Aerospace and Electronic Systems* 41(3):868–878, 2005.

**Dimensional analysis / verification discipline**

- E. Buckingham, "On Physically Similar Systems; Illustrations of the Use of Dimensional Equations," *Physical Review* 4(4):345–376, 1914.
- Lord Rayleigh, "The Principle of Similitude," *Nature* 95:66–68, 18 March 1915.
- A. J. Kennedy, *Programming Languages and Dimensions*, PhD thesis, University of Cambridge, UCAM-CL-TR-391, 1996.
- Mars Climate Orbiter Mishap Investigation Board, *Phase I Report*, NASA, 10 November 1999. https://llis.nasa.gov/llis_lib/pdf/1009464main1_0641-mr.pdf
- J. Bentley, "Programming Pearls: The Envelope Is Back," *CACM* 29(3):176–182, 1986.
- C. J. Roy, "Review of code and solution verification procedures for computational simulation," *J. Computational Physics* 205:131–156, 2005.
- G. Pólya, *How to Solve It*, Princeton UP, 1945 ("Test by Dimension").

---

## Internal references

- **ADR-0070** — the provenance trial. This document's items 1, 3, 6 and 10 bear
  on its findings 2, 3, 4 and 6; **none of its verdicts is reversed**, and
  finding 3's "fossil" is downgraded to "right value, wrong citation".
- **ADR-0071** — the candidate enumeration. **Not adjudicated here.** Items 4, 5
  and CD-1 annotate family 1; items 4, 10 and CD-4 annotate family 2. Its
  candidate (d)'s explicitly-open verification item — *"a retransmit re-sends an
  ALREADY-STORED symbol and needs no new slot … I did not verify it in the
  code"* — **is confirmed by inspection**: `sent_store` is keyed by `block_id`
  and inserted only on the source-emission path (`emit_source.rs:322`), and
  ADR-0060 retains the payload until the cumulative frontier passes. That is a
  code fact, not a preference between candidates.
- **ADR-0058** — CD-5 supplies its prior art (Eppen 1979) and one untested
  condition (demand correlation).
- **ADR-0068** — item 4a and CD-4/item 9 record the δ unit mismatch and the
  cascade constraint as hazards for the proposed fusion.
- **ADR-0066** (era honesty), **ADR-0067** (why nothing flips), **ADR-0052**
  (pre-registration shape for anything in Tier 1–3).
- **CLAUDE.md, FORMULA-FIRST LAWS** and **MEASUREMENT DISCIPLINE 17/18** —
  CD-7 gives them their established name and suggests one strengthening.
- **Paper**: §16.55–§16.65, Appendix B (extended, not duplicated), Appendix C.

