# One Machine on a Dial: FEC+ARQ Multipath Transport over (δ, ρ, r)

## Abstract

Data crossing lossy, heterogeneous paths (WiFi, LTE, satellite) arrives
with different contracts: a file wants every byte, a voice stream wants each
message by a deadline, a tunnel carrying TCP wants order without stalls.
Transports usually serve these contracts with modes: a realtime mode, a bulk
mode, a switch between them and per-mode constants. raptorpath is built on
the opposite premise. It is one machine, parameterised by the triangle
(δ, ρ, r) — a latency price δ, a retention contract ρ and a correction
rate r — and by measured channel anchors. The machine is continuous in δ
and carries no mode bit; the hints Realtime, Auto and Bulk are named points
on the δ dial.

FEC repair and ARQ retransmit are treated as one stream of correction
symbols. The correction rate is a closed-form r* corrected for burst
variance under a Gilbert-Elliott channel and, on heavy-tailed channels,
provisioned against the receiver's measured window loss-mass quantile. The
rate law blends an anchored and a late-is-fine term by the dial's bulkness,
`r(β) = (1 − β)·r_anchor + β·r_late-is-fine`, both always computed. One
global incremental decoder replaces three receive machines; the difference
between realtime and bulk becomes a sender span law derived from δ. The
multipath outstanding pool is `(1 + q(δ))·Σ bwᵢ·RTpropᵢ`, with q mapped onto
the standing-queue setpoint CoDel derives from Kleinrock power. The recovery
decision is one one-sided sequential test; the value of waiting is bounded
by counters the machine already reads, and the domain in which waiting can
pay is derived from the store cap.

On emulated links the shipped machine delivers bounded message tails with
complete delivery under bursty loss (p99 36–39 ms at a 2.6 % Gilbert-Elliott
cell, where QUIC reaches 55–342 ms and TCP 0.2–1.4 s with delivery cliffs).
For bulk it is far above every Cubic-family stack but not faster than
well-tuned BBR-class single-path stacks or kernel MPTCP over BBR. The
derived flow-control laws are goodput-parity results that remove 10–200 ms
of standing queue. The paper reports the refuted designs alongside the
shipped ones, and lists every remaining constant that has no derivation.

---

## Table of Contents

1. [Introduction and Contribution](#1-introduction-and-contribution)
2. [System and Channel Model](#2-system-and-channel-model)
3. [Recovery Fundamentals](#3-recovery-fundamentals)
4. [The Rate Law](#4-the-rate-law)
5. [The Span Machine and Multipath](#5-the-span-machine-and-multipath)
6. [Flow Control](#6-flow-control)
7. [The Recovery Decision](#7-the-recovery-decision)
8. [Congestion Control and Substrate](#8-congestion-control-and-substrate)
9. [Evaluation](#9-evaluation)
10. [Refuted and Superseded Designs](#10-refuted-and-superseded-designs)
11. [Open Questions, Related Work, References](#11-open-questions-related-work-references)
- [Appendix A: Key Formulas](#appendix-a-key-formulas)

---

## 1. Introduction and Contribution

### 1.1 The problem

A tunnel over lossy, heterogeneous paths (WiFi, LTE, satellite, a clean
datacentre link) carries traffic with different contracts. A file transfer
wants every byte and does not care when any single byte arrives. A voice or
game stream wants each message within a deadline and can tolerate an
occasional drop. A tunnel carrying TCP wants in-order delivery without the
latency spikes that make the inner TCP collapse.

Two recovery tools exist. Forward error correction (FEC) spends bandwidth
up front and recovers losses without a round trip. Automatic repeat request
(ARQ) spends a round trip per loss and wastes no bandwidth. Multipath adds a
third lever: which path a symbol travels on, and how much reordering the
receiver must absorb.

The usual engineering answer is a set of modes: a realtime mode with heavy
FEC and eviction, a bulk mode with pure ARQ and retention, a switch between
them keyed on a protocol hint, and per-mode constants. Every mode boundary is
a place where behaviour steps discontinuously, where state does not transfer,
and where two implementations of the same subsystem drift apart.

### 1.2 The claim

This paper describes raptorpath, a FEC+ARQ multipath transport built on one
different premise:

> There is **one machine**, parameterised by the triangle **(δ, ρ, r)** and
> by measured channel anchors. It is continuous in δ and carries no mode bit.
> The protocol hints Realtime, Auto and Bulk are named points on the δ dial,
> not modes.

The three dials are:

| dial | meaning | range | named points |
|---|---|---|---|
| δ | latency price: the marginal cost of delay, in Copa's utility `U = log(throughput) − δ·log(delay)` [Copa2018] | (0, ∞) | Realtime 50, Auto 0.5, Bulk 0.005 |
| ρ | retention contract: the probability a symbol is eventually delivered | (0, 1] | 1 on every reliable seat |
| r | correction rate: repair plus retransmit symbols per source symbol | [0, r_max] | derived from δ and the channel, never declared |

The FEC rate, the tail target, the recovery deadline, the span width of
coded repairs, the shed budget, the standing-queue allowance of the
outstanding pool and the placement bandwidth weight are functions of these
dials and of measured anchors (bottleneck rate BtlBw, propagation delay
RTprop, loss ε̂, burst statistics). The constants that do not yet meet this
standard, and the remaining hint-keyed sites, are listed as open debts
(Sections 11.1–11.3). When two behaviours must both exist, they are written
as one formula continuous in the dial, and both terms are always computed. The canonical example is the rate law of
Section 4:

```text
   r(β)  =  (1 − β)·r_anchor  +  β·r_late-is-fine ,      β = bulkness(δ)
```

### 1.3 Contributions

1. **A recovery model that unifies FEC and ARQ** as one stream of correction
   symbols (Section 3), with a closed-form correction rate r* corrected for
   burst variance and, on heavy-tailed channels, provisioned against the
   measured window loss-mass quantile (Section 4).
2. **A continuous δ dial.** The engine reads one number δ. Its tail target,
   span horizon b(δ), bulkness β(δ), placement weight w_bw(δ), and pool
   setpoint q(δ) are all functions of δ, exact at the three named points and
   continuous between them (Sections 4–6).
3. **One decoder.** A single global incremental RREF decoder replaces three
   receive machines (sliding-window RLC, generation, streaming). The
   difference between "realtime" and "bulk" moves to the sender's span law,
   `(δ, ρ, r) → (A*, M*, Δ)` (Section 5).
4. **A flow-control law with no fitted multiplier.** At two or more paths
   the pooled outstanding cap is `(1 + q(δ))·Σᵢ bwᵢ·RTpropᵢ`, where q(δ) is
   mapped onto the standing-queue setpoint RFC 8289 derives from Kleinrock
   power (Section 6).
5. **The recovery decision as a sequential test.** Every recovery clock
   written for this machine is one one-sided sequential test on a different
   measurand. The value of waiting is bounded by counters the machine already
   reads, and the domain in which waiting can pay is derived from the store
   cap (Section 7).
6. **A measured record, including refutations.** The evaluation (Section 9)
   and the refuted-designs table (Section 10) report what the machine does on
   emulated links against QUIC, TCP and MPTCP, and which proposed mechanisms
   failed and why.

### 1.4 What the evaluation shows

The shipped machine delivers bounded message tails with complete delivery
under bursty loss — p99 36–39 ms at a 2.6 % Gilbert-Elliott cell where QUIC
reaches 55–342 ms and TCP 209–1407 ms with delivery cliffs. For bulk it is
loss-robust far above every Cubic-family stack, but it is not a faster bulk
pipe than well-tuned BBR-class single-path stacks or kernel MPTCP over BBR,
and at the heterogeneous dual cell it is measurably behind them
(Section 9.2). The two derived flow-control flips are parity results in
goodput that remove standing queue; they make the machine cheaper, not
faster.

### 1.5 Architecture as shipped

The architecture described here is the window machine (Sections 5–7). In the
current release the Bulk and Auto hints still default to a legacy block-FEC
pipeline unless `--window-reliable` is given (`is_window_mode`,
`raptorpath/src/net/mod.rs`); Realtime always runs the window machine. That
pipeline selection is a construction-time mode switch and is recorded as an
open debt (Sections 9.1, 10, 11). The bulk and multipath results for the
window machine in Section 9 are measured with `--window-reliable`.

### 1.6 Reading guide

Section 2 fixes notation and the channel model. Section 3 develops the
recovery fundamentals (FEC versus ARQ, P_lost, the taper, the correction
symbol). Section 4 states the rate law. Section 5 describes the span machine
and multipath placement. Section 6 states the flow-control law. Section 7
states the recovery decision. Section 8 covers congestion control and the
QUIC substrate. Section 9 is the evaluation, Section 10 the refuted and
superseded designs, Section 11 the open questions, related work and
references. Appendix A collects the formulas.

Each law is stated once, on its own line, with a provenance table:
**derived** (follows from stated assumptions), **cited** (a published value
with its reference), **measured** (with the cell and commit), **declared**
(a dial or resource bound), or **unprovenanced** (a literal with no
derivation; these are collected in the open-constants register, Section 11.2).

---

## 2. System and Channel Model

### 2.1 Notation

| Symbol | Meaning | Unit |
|---|---|---|
| ε, ε̂ | channel loss rate; its estimate (the 95 % predictive upper quantile unless stated) | probability |
| p, q | Gilbert-Elliott P(Good→Bad), P(Bad→Good) | probability |
| B | mean burst length 1/q | symbols |
| σ²_burst | burst variance inflation factor, Section 2.4 | — |
| W | FEC encoder window | symbols |
| r | correction rate (corrections per source symbol) | ratio |
| δ | latency price (the dial) | — |
| ρ | retention contract | probability |
| ζ(δ) | tail-loss scale δ_auto/δ, δ_auto = 0.5 | — |
| t_tail | effective tail-loss target = base·ζ(δ), base = 10⁻⁵ | probability |
| P_fec | probability a lost symbol is FEC-recovered | probability |
| P_arq | probability ARQ recovers a symbol FEC missed | probability |
| SRTT, RTTVAR | smoothed RTT and its deviation (RFC 6298 weights) | s |
| RTprop | windowed-min RTT (10 s) | s |
| BtlBw, bw_i | windowed-max delivered rate, per path | symbols/s |
| g_i | per-path goodput C_i·(1−ε_i) | symbols/s |
| b(δ) | span-horizon coefficient, Section 5.4 | round trips |
| D(δ) | recovery deadline min(b(δ)·RTprop, 2·RTprop) | s |
| β(δ) | bulkness, Section 4.5 | [0, 1] |
| χ | completion exposure, Section 4.6 | [0, 1] |
| A*, M*, Δ | span width, pipeline depth, trailing offset, Section 5.3 | symbols / count |
| q(δ) | pool standing-queue setpoint, Section 6.1 | fraction of one BDP |
| π₀, π₁ | prior that a detected hole is reordering / genuine loss | probability |
| F, S = 1 − F | lateness distribution of self-healing holes and its survival | — |
| H | store headroom time, Section 7.3 | s |

The earlier drafts of this model used δ for a tail-probability target. In
this paper δ is always the latency price; the tail target is t_tail, which
the engine derives from δ through ζ (Section 4.1).

### 2.2 Components

```text
 Sender                        Channel                      Receiver
┌─────────────────┐          ┌──────────┐          ┌────────────────────┐
│ source store    ├─source──►│          ├─────────►│ unified decoder    │
│ (retained until │          │  paths   │          │ (global RREF)      │
│  acked, ρ = 1)  ├─repair──►│  1..N    ├─────────►│ in-order frontier  │
│ span law        │          │  (GE)    │          │ reorder hold       │
│ placement law   │◄─WindowAck (cum-ack + SACK + echo)─┤ gap reports        │
│ recovery plane  │          │          │          │                    │
└─────────────────┘          └──────────┘          └────────────────────┘
```

Source symbols are sent systematically (as-is). Repair symbols are random
linear combinations over GF(256) of a contiguous span of source symbols,
self-describing on the wire by `(anchor, width, index)`. The receiver acks
with a merged `WindowAck` carrying the cumulative frontier, SACK ranges and
an echo timestamp. Retransmits are exact copies from the sender's store.
The wire runs over QUIC datagrams (quinn), protocol version 8
(`transport/protocol.rs`). The feedback message carries:

| `WindowAck` field | role |
|---|---|
| `received_up_to` | cumulative frontier: every sequence up to it received or recovered |
| `sack_ranges` | every out-of-order range above the frontier, not a bounded few |
| `echo_send_timestamp_us` | the sender's own timestamp, for RTT without clock synchronisation |
| `jitter_us` | interarrival jitter (RFC 3550 A.8) |
| `cumulative_received` | running total, a self-healing reliability counter |
| `cum_expected`, `cum_received` | per-path cumulative counters the sender differences against a cursor; a dropped ack loses nothing |

One merged `WindowAck` is sent per data message; the stream carries one
symbol per acknowledgement on the measured cells (Section 9.5). A repair
symbol carries a 14-byte header (`REPAIR_HEADER_SIZE`) in front of a
1200-byte payload. Version 8 added the receiver's `RepairRequest { spans,
cause }` (Section 7.6); the protocol version is enforced at the handshake so a
mismatched peer fails cleanly instead of mis-parsing control traffic.

### 2.3 Gilbert-Elliott channel

Wireless loss is bursty: a fade or handover drops consecutive packets. The
two-state Gilbert-Elliott (GE) chain [Gilbert1960, Elliott1963] captures this
with two parameters. In Good no symbol is lost, in Bad every symbol is lost;
for UDP datagrams, which arrive intact or not at all, this h_G = 0, h_B = 1
simplification is exact.

```text
   π_B = p/(p+q) ,   π_G = q/(p+q)          stationary state probabilities
   ε   = π_B = p/(p+q)                      average loss rate
   P(T ≥ t) = (1−q)^(t−1) ,   E[T] = 1/q    burst length T (geometric)
```

| Scenario | ε | p | q | B | σ²_burst |
|---|---|---|---|---|---|
| DC | 0.1 % | 0.0005 | 0.5 | 2.0 | 3.0 |
| WiFi | 2.5 % | 0.013 | 0.5 | 2.0 | 2.9 |
| LTE | 5 % | 0.02 | 0.4 | 2.5 | 3.8 |
| Satellite | 9 % | 0.03 | 0.3 | 3.3 | 5.1 |

The emulated evaluation cells (Section 9.1) use GE parameters from this
family: c2 is p = 1.3 %, q = 50 % (ε = 2.53 %), c3 is p = 2 %, q = 40 %
(ε = 4.76 %).

### 2.4 Burst variance

Loss counts in a window of W symbols have mean Wε. Burst correlation
inflates their variance above the binomial:

```text
   Var_GE(K)  =  W·ε·(1−ε)·σ²_burst ,      σ²_burst  =  1 + 2(1−p−q)/(p+q)
```

`raptorpath_math::burst_variance_factor`. When the estimator has seen no
Bad-state transitions, q̂ = 0 is a no-data sentinel and maps to σ²_burst = 1;
treating it as a measurement would make σ²_burst ≈ 2/p̂ explode on the
cleanest links.

### 2.5 Adequacy against real traces

GE was tested against five real cellular capacity traces (Verizon, AT&T,
T-Mobile LTE/UMTS, recorded with Saturator [Winstein2013]; loss derived by a
drop-tail queue at 0.5 load). GE misses three structures:

* **Long memory.** Real lag-20 loss autocorrelation is 5×–4100× the fitted GE
  prediction.
* **Heavy burst tails.** Extreme bursts are 3.8×–26× longer than geometric.
* **Non-stationarity.** ε drifts 0–87 % across sixths of a single trace.

At the r* the closed form prescribes (Section 4.2), the delivered window
failure on these traces is 1.2×–3.7× worse than the GE-ideal and up to 12.8×
the target. Section 4.3 corrects this by provisioning against the measured
window loss-mass tail. The multipath coding mechanic is more robust: two
independent real traces as two paths aggregate ×1.178 against a GE control
of ×1.180 (`raptorpath-math/tests/real_trace_validation.rs`).

Real path-to-path loss correlation is not tested by these traces, which are
independent by construction, and the emulated dual cells draw each leg's
loss from an independent seed (Section 9.1). Correlated loss is untested.

### 2.6 Estimation

The sender estimates loss and burst statistics from acknowledgements.

* **Beta-Binomial posterior.** `a' = a·0.995 + received`, `b' = b·0.995 +
  lost` (half-life ≈ 138 observations). Its upper quantile is the safety
  margin.
* **BOCD** [Adams2007]. Bayesian online changepoint detection over run
  lengths; the predictive quantile integrates over run lengths, so a regime
  change widens the margin automatically (hazard 0.01, run length ≤ 200).
  The implemented predictive quantile is the run-length-weighted average of
  per-run quantiles, an approximation of the mixture quantile. ε̂ in this
  paper is `predictive_loss_upper(0.95)`; before five BOCD updates it is the
  Beta posterior's normal-approximation quantile.
* **GE estimator** (`raptorpath-math/src/gilbert_elliott.rs`, re-exported as
  `control::gilbert_elliott`).
  Decayed transition counters (factor 0.999), gated by `is_valid()` until
  30 transitions; σ²_burst = 1 before that. The per-batch feed records a
  batch's losses before its receives, so the within-batch ordering the
  counters see is synthetic.
* **Window loss-mass statistics** (Section 4.3), kept by the receiver.
* **Rate and delay anchors.** BtlBw is a windowed max of delivered-rate
  samples, RTprop a 10 s windowed min of RTT samples, SRTT an EWMA with
  RFC 6298 weights. The max-filter is a monotonic deque whose front equals
  the full-window fold after every push and eviction (value-identical,
  O(1) amortised).

Three measured facts qualify what these estimators are fed:

1. **The counters are wire-arrival counts, not goodput.** Repairs enter the
   acknowledgement counters; the received/source-acked ratio is 1.1–4.4 %
   above 1 at c2-class paths and 21–34 % at c8. At a dual cell the per-path
   `expected − received` delta is further inflated by cross-path
   interleaving, so it is not a loss estimate there.
2. **Per-path loss under-reads the channel by 3–5×.** Realised per-leg ε̂ is
   0.0056 (c7) and 0.0184 (c8) against channel values 0.025 and 0.048.
3. **The anchors are whole-transfer extrema** for transfers shorter than the
   10 s filter window, which includes most measured transfers.

Anchors must also obey three hygiene laws, each of which was violated by a
measured defect before it was stated: an anchor is seeded from measured
sends (never a constructor default that gets recorded as data); its samples
exclude whole-process clock gaps (detected on a 50 ms process-clock tick,
quarantine min(gap, 2 s)); and its floors and backstops expire.

### 2.7 Rate samples

BBR's bottleneck-rate estimate is trustworthy because of per-packet
delivery-rate sampling and app-limited detection, which discards samples
taken while the sender was application- or window-limited
[Cardwell2016]. The engine's default delivery sampler divides ack-batch
counts by elapsed wall time and rejects calls closer than 1 ms apart; on the
wire the floor rejects 81–94 % of calls, each accepted sample folds 5–28 acks,
and the windowed max over-reads the link by ×4.6–7.4. Because the anchor is
used only to raise cwnd (Section 8.3) and the pool Σ is compared against a
ceiling, the over-read was partly load-bearing: the honest send-interval
sampler (`RWM_PLAIN_RS`: Δt = max(send elapsed, ack elapsed), windowed max,
app-limited exclusion, per-path attribution from each ack's frontier and SACK
difference) reads 1.02× the known link rate, costs 20 % at the single-path c2 cell under
the old cap, and ships off. The two anchor families
(delivery-clocked and send-clocked) remain the main unresolved measurement
question under every cap law.

---

## 3. Recovery Fundamentals

### 3.1 FEC and ARQ

**FEC.** The sender keeps an encoder window of recent source symbols and
emits repairs, each a random GF(256) combination of the window:

```text
   p  =  Σ_{j ∈ span} c_j · s_j
```

If k source symbols in the span are lost, any k linearly independent
surviving equations that cover them recover all k by Gaussian elimination.
Sources are sent systematically, so the decoder runs only when something is
lost. Over GF(256) the expected excess over k equations is about 1/255 of a
symbol (effectively MDS). FEC costs r/(1+r) of the wire whether or not loss
occurs, and adds no round trip.

**ARQ.** The sender infers loss from acknowledgements (cumulative ack plus
SACK) and retransmits an exact copy, usable by the receiver without
decoding. ARQ costs about ε per source symbol and at least one detection
delay plus a one-way flight per recovery:

```text
   L_arq  =  T_detect  +  RTT/2
```

A lost acknowledgement is self-healing: later cumulative acks supersede it,
and a duplicate retransmit is discarded by sequence number.

```text
  Sender:   [S1] [S2] [S3] [S4] [S5] ...                 [S3']
                        X lost                              ^ retransmit
  Receiver:  S1   S2  (gap)  S4   S5  ── WindowAck ──►      S3' used at once
                                   cum = S2, SACK {S4, S5}
            |<──────── T_detect ────────>|<── RTT/2 ──>|
```

**FEC recovery time.** Useful equations arrive at rate r/(1+r)·(1−ε) per
wire slot, so m losses in the window are recovered after

```text
   t_fec  =  m·(1+r) / (r·(1−ε)) · t_sym ,        t_sym = symbol_size / throughput
```

At 100 Mbit/s with 1200 B symbols (t_sym = 96 µs), r = 0.08, ε = 0.025, one
loss is recovered in 1.3 ms; at 1 Mbit/s the same loss takes 133 ms.

### 3.2 P_lost(t)

At age t, with no acknowledgement yet, the posterior probability that a
symbol was lost is

```text
   P_lost(t)  =  ε / [ ε + (1−ε)·P(RTT > t) ] ,    P(RTT > t) = 1 − Φ((t − SRTT)/RTTVAR)
```

`raptorpath_math::p_lost` (re-exported as `control::fec_rate::p_lost`). It stays near ε until about SRTT + RTTVAR
and reaches near-certainty two to three RTTVAR later. For WiFi-like values (ε = 0.025, SRTT = 50 ms, RTTVAR = 5 ms):

| age t | 0 | 40 ms | 50 ms | 55 ms | 60 ms | 70 ms |
|---|---|---|---|---|---|---|
| P_lost(t) | 0.025 | 0.026 | 0.049 | 0.14 | 0.53 | 0.999 |

The transition is sharp; the smoothness of a repair/retransmit mix in
practice comes from the spread of symbol ages in the buffer, not from the
curve. Choosing a retransmit with probability P_lost(t) and a repair
otherwise wastes, in expectation,
P_lost·(1 − P_lost) per slot: zero at both extremes, maximal at 0.5. At high
loss (ε = 0.5) half of all correction slots are retransmits from age 0,
which is proactive redundancy with no special case.

In the shipped window machine this probabilistic retransmit of the oldest
unacked symbol (`net/emit_source.rs`) evaluates P_lost with the worst path's
smoothed RTT and RTTVAR fixed at 0.1·SRTT. It is live code but measured inert:
`taper_copy = 0` at all four audited cells and on both loopback
topologies. Whether its ε̂ at send time is structurally near zero or the
oldest symbol never ages far enough is open (Section 11.2).

### 3.3 The taper

Correction density need not be uniform in time. After a loss the chance the
burst is still running decays as the GE survival function, so the taper
places corrections accordingly:

```text
   τ(t)  =  A·(1−q)^t ,        r  =  Σ_{t≥0} τ(t)  =  A/q   ⇒   A = r·q
```

For an i.i.d. channel (q = 1) the taper is flat.

```text
   τ(t)
    A ┤█
      ┤█▆
      ┤██▄
      ┤███▃▂
      ┤█████▂▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁   (never reaches 0)
      └┬────┬────┬────┬────► offset t
       0    B    2B   3B
```

τ(B) ≈ 0.37·A, τ(2B) ≈ 0.14·A, τ(5B) ≈ 0.007·A. Once a symbol is acknowledged
its taper stops, so the infinite tail is truncated by the acknowledgement
mechanism. The GE estimate adapts the decay; a BOCD changepoint widens the
posterior and with it the correction budget.

Two facts limit how much the shape matters. First, repairs are
window-fungible: every repair covers the whole span, so per-symbol
attribution is bookkeeping. With a continuous source stream the aggregate
correction rate is Σ_t τ(t) = r whatever the shape; shape only matters in
transients and through acknowledgement truncation (once a symbol is acked,
its taper stops). Second, the benefit of a correction at offset t is
hump-shaped, `(1−q)^t·(1 − (1−q)^t)`, because a correction sent into a
running burst is itself lost; the exponential taper has the correct tail and
over-weights the first few offsets.

**End-of-stream truncation.** A symbol at distance j < W from the end of a
finite transfer draws coverage from only j of its W window lifetimes, so the
last W symbols are progressively under-covered and each tail loss falls to a
serial ARQ round (about 1.5 RTT). Section 4.6's completion exposure χ is the
continuous remedy.

**Taper budget.** The shipped emitter banks `owed += r` per source symbol
(`TaperBudget`), so the wire consumes r as computed. An earlier emitter
reset its offset on every cumulative-ack advance and emitted about r repairs
per ack cycle rather than per source symbol; that quantity defect is fixed.

### 3.4 The correction symbol

FEC repair and ARQ retransmit occupy the same wire slot, cost the same
bandwidth, and cross the same channel. They differ in timing and content:

| | retransmit | repair |
|---|---|---|
| content | exact copy | GF(256) combination |
| receiver action | immediate use | fed to the decoder |
| waste | duplicate if the original arrives | never wasted while it adds rank |
| best when | loss is localised | loss is uncertain or bursty |

The model treats both as **correction symbols**. The taper sets how many;
P_lost(t) and loss localisation set which kind. Once a loss is localised by
SACK, one exact symbol is the cheapest correction, so the window/ARQ split
falls out of localisation rather than a rule. This reproduces the optimal
policy of Mehrotra and Li [Mehrotra2010]: repair while loss is uncertain,
retransmit once it is confirmed.

### 3.5 Delivery outcomes and the reliability leg

A source symbol is delivered on time (not lost, or FEC-recovered), late
(ARQ-recovered), or not at all:

```text
   P(on time)  =  (1 − ε) + ε·P_fec
   P(late)     =  ε·(1 − P_fec)·P_arq
   P(lost)     =  ε·(1 − P_fec)·(1 − P_arq)  =  1 − ρ
   ⇒  P_arq    =  1 − (1 − ρ) / (ε·(1 − P_fec))
```

At ρ = 1, P_arq = 1: the store retains every source symbol until it is
acknowledged (T_cut = ∞) and ARQ is the backstop for whatever FEC misses. At
ρ < 1 a finite give-up age T_cut(ρ) bounds retention. Give-up must be
age-based; buffer fullness is a flow-control signal, never licence to
destroy data (Section 5.1).

### 3.6 Codec overhead

For a systematic code the decoder runs only when the window contains a loss,
so a codec overhead ε_codec is paid with probability `1 − (1−ε)^W`:

```text
   ε_codec,eff  =  ε_codec·(1 − (1−ε)^W)
```

The engine adds ε_codec,eff to the rate whenever the core FEC term is
positive (Section 4.4), with ε_codec = 0.004 for RLC, 0.01 for RaptorQ and 0
for Reed-Solomon (`FecRateController::new_with_toggles`). Weighting by the
decoder-invocation probability matters most for a high-overhead codec on a
clean link, where the decoder rarely runs.

### 3.7 When FEC beats ARQ

**The race.** FEC and ARQ run in parallel for a lost symbol; delivery is at
`min(t_fec, L_arq)`, so

```text
   P(delivered by T)  =  1 − P(t_fec > T)·P(L_arq > T)
```

and the pair is better than either alone. P_lost sets the bandwidth split
between them, not which one runs.

**The latency crossover.** Per hole, FEC is faster when t_fec(W) < L_arq ≈
1.5·RTT: at high RTT, short bursts and high bandwidth. A decode-resolved hole
at RTT 200 ms recovered in 8.5 ms against a 279 ms ARQ round, and GF(256)
decode costs about 10 µs per symbol.

**The throughput identity.** Per-hole latency does not make proactive FEC win
on throughput. On a single saturated path an RTT sweep (10–200 ms, 2.5 % GE
loss) found pure ARQ ahead of proactive frontier FEC at every RTT (FEC/ARQ
0.61–0.75), for two reasons the latency model omits:

1. **Present or isolating, not both.** To arrive before the frontier reaches
   the hole a repair must code still-in-flight neighbours, so it cannot
   isolate the hole; to isolate it, it must code received neighbours, so it
   arrives after ARQ has fired. The covering repair was measured absent at
   the stall in every case.
2. **Displacement.** On one path there is one congestion-controlled budget
   shared by source and repair. Forcing presence (a pacer that emits repair
   for a generation while it is still filling) raised presence and lowered
   goodput 3–21 %:

```text
   R_frontier  =  R_cc · (1 − φ_early(P))
```

   where φ_early is the share of the paced budget spent on early repair to
   reach presence P.

So on a single path FEC and ARQ reach throughput parity at best, which is the
measured single-path bulk result (Section 9.3). The latency premium of FEC is
realisable without displacement only on an orthogonal path, which makes the
crossover a multipath question. Section 4.9 reaches the same corner from the
price side.

**Proactive duplication versus FEC.** At equal overhead one repair covers any
single loss in a W-symbol window, while one duplicate covers one position;
duplication approaches FEC only as ε → ½. The Realtime hint still sends a
duplicate source copy, a rate decision taken by a hint equality (a declared
corner, Section 11.1).

**Recovery starts after the burst.** Repairs sent inside a burst are lost
with it, so recovery begins only when the burst ends: burst length decides
both how many repairs are needed and how long before they can arrive. Repairs
already in flight before the burst give a head start.

**Decode micro-bursts.** A cascade decode releases several symbols at once.
For jitter-sensitive payloads a de-jitter buffer absorbs this; it costs a few
milliseconds against the round trip FEC saves.


---

## 4. The Rate Law

### 4.1 From the dial to a tail target

A hint names a point on the δ dial through one quotient
(`net::delta_price`, `scheduler::hint_delta_price`):

```text
   δ(hint)  =  δ_auto / ζ(hint) ,     δ_auto = 0.5 ,   ζ ∈ {0.01 Realtime, 1 Auto, 100 Bulk}
            ∈  {50, 0.5, 0.005}
```

`δ_auto = 0.5` is Copa's default latency price [Copa2018]; ζ is the hint's
single declared price ratio (a late symbol costs 100× more at Realtime and
100× less at Bulk than at Auto). `RWM_DELTA` replaces the map with a number
so a run can sit between the presets; nothing downstream reads the hint
itself.

The tail target the rate law protects is

```text
   t_tail(δ)  =  clamp( base · ζ(δ) ,  10⁻⁹ ,  0.1 ) ,     ζ(δ) = δ_auto/δ ,   base = 10⁻⁵
```

(`FecRateController::new_with_toggles`; `base` is `target_tail_loss`,
default 10⁻⁵, 10⁻⁴ and 10⁻⁶ in the Home and Datacenter profiles). At the
three presets `t_tail` is 10⁻⁷, 10⁻⁵ and 10⁻³.

### 4.2 The closed form r*

**Setup.** A window of W source symbols carries rW repairs. Loss count K in
the window is approximately Normal(Wε, Wε(1−ε)σ²_burst); surviving repairs
are approximately Normal(rW(1−ε), rWε(1−ε)). FEC recovers the window iff
surviving repairs ≥ K, so

```text
   P_fec(r)  =  Φ( √W · (r(1−ε) − ε) / √(ε(1−ε)(r + σ²_burst)) )
```

At the information-theoretic minimum r = ε/(1−ε) this is exactly ½.

**The constraint.** A symbol is late only if it was lost, so the per-window
constraint that meets tail target t_tail is `P(repairs < K) ≤ t_tail/ε`.
Inverting P_fec to first order (dropping r from the variance, r ≪ σ²_burst):

```text
   r*  =  max( 0 ,  ε̂/(1−ε̂)  +  z · √( ε̂·σ²_burst / (W·(1−ε̂)) ) ) ,     z = Φ⁻¹(1 − t_tail/ε̂)
```

`raptorpath_math::compute_r_star_with_z`, composed in
`raptorpath_math::controller_rate` (Section 4.4), which adds the codec term
of Section 3.6 when the core term is positive.

| term | provenance |
|---|---|
| ε̂/(1−ε̂) | derived: the erasure-channel minimum [Shannon1948], including the geometric chain of lost corrections |
| z | derived from t_tail/ε̂; t_tail from the dial (Section 4.1) |
| σ²_burst | derived from the GE estimate (Section 2.4) |
| W | derived, `derive_window` (Section 4.8), clamped [16, 512] and by the sender's `MAX_WINDOW_SIZE = 200`; on the block pipeline W = k, the block size |
| max(0, ·) | physical floor: a repair count cannot be negative |

**Continuity.** The quantile is taken at 1 − t_tail/ε̂. As the channel
improves toward the target (ε̂ → t_tail), z falls through zero, r* drops
below the IT minimum and reaches 0 continuously: pure ARQ meets the target.
There is no cutoff between "heavy FEC" and "pure ARQ".

**Worked values** (W = 50):

| channel | t_tail = 10⁻² | 10⁻⁴ | 10⁻⁶ |
|---|---|---|---|
| DC (ε = 0.1 %, σ² = 3.0) | 0 | 1.1 % | 2.5 % |
| WiFi (2.5 %, 2.9) | 3.5 % | 12.8 % | 17.8 % |
| Satellite (9 %, 5.1) | 22.2 % | 40.6 % | 52.5 % |

The margin responds to t_tail/ε, not to t_tail alone: the same target needs
no FEC on a clean link and a 12 % margin on a satellite link.

**Known limitation.** The model inflates the variance of K but treats
repair survival as independent. A burst that inflates K also kills
interleaved repairs, so Var(K − C) is wider and the closed form
under-provisions on bursty channels by roughly 30–50 % of itself. The exact
transfer-matrix computation (Section 4.7) and the measured-tail term
(Section 4.3) correct this.

### 4.3 Burst-tail provisioning

**The failure statistic.** A window of W sources and R = rW repairs spans
N = W + R wire slots. If K slots are lost, x of them repairs, the window
fails iff K − x > R − x, i.e. iff K > R: a loss that hits a repair removes
one loss and one repair. The per-window failure probability is therefore
exactly the upper tail of the window loss mass, which already contains burst
length, clustering and loss/repair correlation.

**Measurement.** The receiver bins loss observations into blocks of
w₀ = 64 wire slots and, with decayed counters, tracks for span lengths
m = 1…8 blocks: `p_nz(m) = P(J_m ≥ 1)`, and the conditional first and second
moments of the loss mass J_m. Each conditional tail is extended with a
discrete Weibull, `S(t) = θ^(t^k)`, fitted by midpoint-corrected moment
matching; k = 1 is exactly the geometric (GE) law, k < 1 is the heavy tail
real fades show.

**The corrected rate.**

```text
   F(r)      =  (1−f)·T_lo(R) + f·T_hi(R) ,     T_m(R) = p_nz(m)·S_m(R/s)
   r*_mass   =  min{ r ∈ [0, 2] :  F(r) ≤ t_tail/ε̂ }
   r*        =  max( r*_closed , r*_mass )
```

`raptorpath_math::r_star_mass`, `MassStats`; gate `RWM_RSTAR_TAIL`, default
on. The tails are read at the window's own scale x = N/w₀, interpolating
between bracketing spans; beyond eight blocks a union bound chunks the
window. `s = ε̂_now / ε_mass` rescales the long-memory tail shape to the
current BOCD level, so the term follows regime changes at estimator speed.
The term is inert until 30 nonzero-mass blocks have been observed, is zero
whenever the measured tail already meets the target at r = 0, and when even
r = 2 cannot meet it the solver returns the ceiling: the contract is
declared infeasible in-window rather than silently missed.

| validation | result |
|---|---|
| heavy-tail semi-Markov channel (k = 0.5, ε = 12.5 %, W = 50) | closed form r* = 0.486 misses 5.1×; corrected r* = 1.268 hits 0.99× |
| GE draws, WiFi / LTE / Sat | corrected r* 0.137 / 0.239 / 0.434 against the exact optimum 0.130 / 0.230 / 0.390 |
| five real cellular traces, feasible cells | worst delivered residual 2.88× → 1.41× the target |

(`raptorpath-math/tests/rstar_tail_validation.rs`.) The residual above 1× is
non-stationarity.

### 4.4 The composed rate

The function the engine evaluates, `raptorpath_math::controller_rate`,
combines the closed form with two further provisioning terms and a
saturation cap:

```text
   r_core   =  max(0, ε̂/(1−ε̂) + z·√(ε̂·σ²_burst/(W(1−ε̂))))  +  ε_codec,eff·1{core > 0}
   r_burst  =  ( B̂ / T_rtt ) · (1 − t_eff/ε̂)⁺ ,        T_rtt = RTT·throughput / symbol_size
   r_mass   =  Section 4.3 (when RWM_RSTAR_TAIL and the mass statistics are valid)

   r        =  clamp( soft_sat( max(r_core, r_burst, r_mass) , r_sat ) ,  0 ,  r_max )
```

* ε̂ is the 95 % BOCD predictive upper quantile of the **worst-loss active
  path**, not a point estimate.
* `r_burst` provisions one mean burst B̂ per round trip of symbols, scaled by
  the fraction of losses the target requires FEC to cover. It needs a valid
  GE estimate and a throughput sample.
* `r_sat = argmin_{r ∈ [0.01, 1]} (1 − P_fec)·1.5·SRTT + B̂·t_sym·(1+r)/(r(1−ε))
  + ½·(1+r)·W·t_sym` is the p99 knee past which extra repairs displace source
  and stretch recovery faster than the shrinking FEC-miss cost pays back
  (grid step 0.005). `soft_sat(x, r_sat) = r_sat − s·softplus((r_sat − x)/s)`,
  s = 0.1·r_sat, eases toward the knee without a kink and never adds FEC.
* `r_max = max_overhead`, 0.5 by default.
* An inner-feedback floor for TCP-in-tunnel payloads enters the max with
  weight 0 by default (measured completion-neutral at c2 and −28 % at c3).

On the block pipeline the repair count per block of k sources is `⌈k·r⌉`.
The kernel `compute_r_star_with_z` carries absolute pins against the worked
values of Section 4.2; the composed function carries continuity and
ordinal tests and one loose band against the exact DP (Section 4.7).

### 4.5 The rate mix r(β)

Bulk's contract is "late is fine" mid-stream: a loss recovered one RTT late
costs a bulk transfer nothing because recovery overlaps ongoing sends. The
late-is-fine law sets the effective tail target to the channel itself,
weighted by completion exposure χ (Section 4.6):

```text
   t_eff,bulk  =  ε̂ + (t_end − ε̂)·χ ,      t_end = BULK_TAIL_BUDGET = 0.05
```

At χ = 0, t_eff = ε̂, so z = Φ⁻¹(0) = −∞ and r* = 0 identically, whatever the
estimator's uncertainty.

The engine used to select this law with a hint equality
(`bulk_late_is_fine = hint == Bulk`), a mode switch on the rate leg of the
triangle. It now computes both laws and blends them by the dial:

```text
   r(β)  =  (1 − β)·r_anchor  +  β·r_late-is-fine

   β(δ)  =  clamp( (log₁₀δ_auto − log₁₀δ) / (log₁₀δ_auto − log₁₀δ_bulk) ,  0 ,  1 ) ,   δ_bulk = 0.005
```

`FecRateController::compute_repair_rate` (`control/fec_rate.rs`) builds one
`RateInputs`, evaluates `raptorpath_math::controller_rate` twice (anchor
target t_tail, and the late-is-fine target), and blends them with
`β = raptorpath_math::bulkness_of_delta(δ)`, resolved once at construction.
There is no `if β == 0` fast path; that would be the mode switch again.

**Exactness at the presets.** β(0.5) = 0/(a−b) = 0 exactly, β(0.005) =
(a−b)/(a−b) = 1 exactly, β(50) clamps to 0 exactly, so the three shipped
presets are byte-identical to the former per-hint laws (pinned by
`the_rate_mix_is_byte_identical_at_the_presets`). The mix identities
`1·a + 0·b = a` hold bit-exactly because `controller_rate` clamps both
terms to [0, max_overhead] and they are finite.

**No step between presets.** Just off the Bulk point

```text
   | r(β(δ)) − r_bulk |  ≤  (1 − β(δ))·max_overhead  →  0   as  δ → δ_bulk
```

(`the_rate_does_not_step_across_the_bulk_preset_or_anywhere_on_the_dial`).
The same law, with the same continuity gate, is the visualizer's
(`raptorpath-wasm`, `test_continuum_one_law_across_the_dial`).

`set_bulk_pure_arq(false)` is read as β := 0, so the ablation remains
meaningful.

### 4.6 Completion exposure χ

What distinguishes a loss that costs completion time from one that does not
is whether its ARQ round can still hide behind remaining sends:

```text
   χ(T_rem)  =  Φ̄( (T_rem − 1.5·SRTT) / σ_arq ) ,      σ_arq = max(4·RTTVAR, SRTT/4)
```

`raptorpath_math::completion_exposure`. Mid-stream χ = 0; over the final
~1.5 SRTT it rises smoothly to 1. The one-shot end-of-stream repair burst is
the σ_arq → 0 limit of this ramp. In the simulator the glide cut completion
599 → 562 ticks and excess overhead 5.99 % → 0.04 % at ε = 0.05 relative to
the earlier `min(0.1, ε̂)` Bulk mapping.

**Status.** A tunnel is an endless stream with no known T_rem, so χ = 0 on
the tunnel path (`completion_feed: None`, `config.rs`). Only a driver that
knows the transfer size sets it (the perf client under
`RWM_COMPLETION_EXPOSURE`). The constants 1.5, 4, 1/4 and
`BULK_TAIL_BUDGET = 0.05` are unprovenanced (Section 11.2).

### 4.7 Exact P_fec

Walking the GE chain across the interleaved wire sequence captures burst-
correlated losses, burst-correlated repair erasures and their negative
correlation exactly. With deficit D = source losses − surviving repairs and
`f_i(x, d)` the probability of chain state x and deficit d after slot i:

```text
   f₀(G, 0) = q/(p+q) ,   f₀(B, 0) = p/(p+q)
   d' = d + 1  if slot is source and x' = B
   d' = d − 1  if slot is repair and x' = G
   P_fec  =  1 − Σ_{d>0} Σ_x f_N(x, d)
```

O(W²) work (≈ 6 000 operations at W = 50). `p_fec_exact`,
`compute_r_star_exact` in `raptorpath-math`. On a memoryless channel it
reproduces the binomial reference to machine precision; against Monte Carlo
it agrees to sampling error where the normal form errs by 1–2 %. At
t_tail/ε = 10⁻² and W = 50 the exact r* is 0.170 / 0.270 / 0.450 for
WiFi / LTE / Satellite against the closed form's 0.116 / 0.193 / 0.334.

### 4.8 Choosing W

W pulls three ways: the margin falls as W^(−½) (favours large W), recovery
latency grows as W/send_rate (favours small W), and a mean burst must fit
(a floor). The derived window is

```text
   W_over  =  z²·σ²_burst·(1−ε) / (ε·α²) ,     α = 0.25       margin ≤ α × IT floor
   W_lat   =  budget · send_rate                              recovery ≤ budget
   W_bur   =  B / (ε·(1−ε)) ,   B = (σ²_burst + 1)/2          absorb a mean burst
   W*      =  clamp( W_over , min(W_bur, W_lat) , W_lat )  then clamp to [16, 512]
```

`raptorpath_math::derive_window`, read by the sender and the visualizer.
`clamp`, `min` and `max` are continuous, so W* is continuous in every input.
At loose targets (z ≤ 0) W collapses to the burst floor; at tight targets it
rides the latency ceiling. The fraction α = 0.25 and the [16, 512] bounds are
unprovenanced. The sender further caps W at `MAX_WINDOW_SIZE = 200`, which
is below the multipath lower bound of Section 5.5 at the heterogeneous cell.

Worked values, with per-hint target and latency budget (Bulk 10⁻², 2 RTT;
Auto 10⁻⁴, 1 RTT; Realtime 10⁻⁶, ½ RTT):

| channel | hint | W* | r*(W*) | r*(64) | binding term |
|---|---|---|---|---|---|
| DC (ε = 0.1 %, RTT 1 ms, 100 k sym/s) | Bulk | 200 | 0.0 % | 0.0 % | latency |
| | Auto | 100 | 0.8 % | 1.0 % | latency |
| | Realtime | 50 | 2.5 % | 2.2 % | latency |
| WiFi (2.5 %, 13 ms, 10 k sym/s) | Bulk | 120 | 3.2 % | 3.4 % | overhead knee |
| | Auto | 130 | 9.0 % | 11.8 % | latency |
| | Realtime | 65 | 16.1 % | 16.2 % | latency |
| Satellite (9 %, 210 ms, 2 k sym/s) | Bulk | 512 | 13.7 % | 20.6 % | W bound |
| | Auto | 429 | 20.3 % | 36.8 % | latency |
| | Realtime | 214 | 30.3 % | 47.2 % | latency |

The saving is largest where the 1/√W margin is fattest: on satellite the
derived window nearly halves the fixed-64 overhead at Auto.

### 4.9 Where the machine sits: the corner r* = 0

At the Bulk point β = 1 and, on the tunnel, χ = 0, so t_eff = ε̂, z = −∞, and
the burst and mass terms vanish with the required-FEC fraction:

```text
   r*(Bulk, χ = 0)  =  0      identically
```

(`test_bulk_pure_arq_zero_steady_state_rate`). The coded symbols on the
Bulk wire are the reactive plane's repairs, not proactive FEC.

**The implied proactive price.** r* is a constrained minimum, so it has a
shadow price. Writing the proactive decision as one loss per source symbol
in Copa's currency (a bandwidth fraction enters linearly, a delay as δ·Δd/d):

```text
   L_pro(r; δ, χ)  =  r  +  δ·χ·ε̂·(1 − P_fec(r))·D_arq / d
```

and using `∂z_f/∂r = 1/S`, `S = √(ε̂·σ²_burst/(W(1−ε̂)))`, the stationarity
condition is `φ(z_f(r*)) = S·d / (δ·χ·ε̂·D_arq)`. Since φ ≤ φ(0) = 1/√(2π),
an interior r* > 0 exists only if

```text
   δ·χ  ≥  δ_exit  =  √(2π) · S · d / ( ε̂ · D_arq )
```

| symbol | provenance |
|---|---|
| S | derived from ε̂, σ²_burst, W |
| ε̂ | measured; two readings differ 3–5× (channel versus estimator, Section 2.6) |
| D_arq | measured ARQ resolution delay |
| d | Copa's delay normaliser; not echoed by any gauge |
| √(2π) | 1/φ(0) |

S at the simulator's reference window W = 64, with both readings of ε̂ and
three readings of D_arq/d (Copa's operating delay ≈ 1.03; a propagation-scale
reference ≈ 2.2; the measured repair delivery delay):

| leg / cell | ε̂ | S | δ_exit at D_arq/d = 1.03 | at 2.2 | at measured |
|---|---|---|---|---|---|
| c2, channel ε | 0.0253 | 0.0343 | 3.30 | 1.54 | 0.036 |
| c3, channel ε | 0.0476 | 0.0542 | 2.77 | 1.30 | 0.117 |
| c7 worst leg, estimator ε̂ | 0.0056 | 0.0160 | 6.94 | 3.25 | 0.076 |
| c8 worst leg, estimator ε̂ | 0.0184 | 0.0332 | 4.39 | 2.06 | 0.186 |

So δ_exit ∈ [0.032, 6.94] over every admissible reading. At χ ≤ 1:

* **Bulk (δ = 0.005) is a corner under every reading**, unreachable by
  6.4× to 1 388×. r > 0 at Bulk is not under-funded; it is over-priced.
* **Auto (δ = 0.5) is undecided**: interior if d is the repair delivery
  delay, a corner if d is Copa's operating delay. One gauge field decides it.
* **Realtime (δ = 50)** is interior over the last one to three σ_arq of a
  finite stream under every reading.

`δ_exit` is not implemented in the engine; it is a model statement that
makes the anchor law's answer falsifiable. The anchor law reaches r* = 0 at
Bulk because its target equals the estimate (it has no derivative in δ);
the price form reaches it because δ·χ < δ_exit (and names the point where the
answer would flip).

**Measured.** The r > 0 battery (Section 9.8, merged at `91895dd`) funded
proactive coding at the lossy single-path cell and found it slower, with
decode-resolved holes slower than retransmit-resolved ones, and the control
confirmed at the corner.

### 4.10 Reliability below one: the three-variable problem

At ρ < 1 the taper is truncated at a give-up age T_cut:

```text
   τ(t) = A·(1−q)^t  for t ≤ T_cut ,   r  =  A·(1 − (1−q)^(T_cut+1)) / q
```

Any two of (t_tail, ρ, r) determine the third:

* **(t_tail, ρ) → r.** Find T_cut from ρ by bisection on the monotone
  recovery probability `1 − ε·(1 − P_fec)·(1 − P_arq(T_cut))`, find A from the
  tail constraint, then r = A·(1 − (1−q)^(T_cut+1))/q. At ρ = 1, T_cut = ∞ and
  this is Section 4.2's r*.
* **(r, ρ) → t_tail.** `t = ε·(1 − P_fec(r))·P_arq(ρ)/ρ`.
* **(r, t_tail) → ρ.** With F = ε(1 − P_fec), the implied lateness
  `1 − (1 − F)/ρ` is increasing in ρ, so bisection finds the largest ρ whose
  lateness fits the budget.

Examples at WiFi loss (ε = 2.5 %, W = 50, σ² = 2.9): a VoIP stream with a
150 ms budget and r = 5 % gets P_fec ≈ 0.73 and one ARQ round inside the
budget, so ρ(150 ms) ≈ 99.98 %, limited by double losses rather than by the
correction budget. A 33 ms video frame deadline is below one ARQ round, so
recovery must be FEC and r* lands between the Auto and Realtime rows of
Section 4.2 (13–18 %).

In the engine ρ < 1 exists only as the evicting Realtime seat
(Section 5.1); T_cut is not a runtime input.

### 4.11 The deadline-constrained multipath rate

With several paths, a symbol's delay decomposes as

```text
   T_delay  =  d_i  +  R_recover  +  L_reorder
   R_recover = 0 if delivered or FEC-covered, 1.5·RTT_i if ARQ-recovered
```

where L_reorder is the resequencing wait of in-order delivery. With deadline
D split as D = H + D_fec (H the reorder share), the eligible set is
`E = {i : d_i − d_min ≤ H}` and, to first order,

```text
   P(late)  ≈  Σᵢ (gᵢ/Σg) · [ 1{dᵢ − d_min > H}
                              + 1{dᵢ − d_min ≤ H} · εᵢ(1 − P_fec,i(r)) · 1{dᵢ + 1.5·RTTᵢ > D} ]
```

For fixed H, P(late) is strictly decreasing and continuous in r, so the
feasible set is an upper interval and the minimal-overhead r is its boundary
point (the KKT multiplier on the tail constraint is positive). With each path
keeping its own losses under budget:

```text
   r*_unified  =  max_{i ∈ E}  [ εᵢ/(1−εᵢ)  +  z_{tᵢ/εᵢ}·√( εᵢσ²ᵢ / (W(1−εᵢ)) ) ]
```

**N = 1 reduction.** With one path d₁ = d_min, the reorder term vanishes for
every H ≥ 0, P(late) = ε(1 − P_fec), and r*_unified is exactly Section 4.2's
r*. H → ∞ is the out-of-order corner; ordering is a delivery policy that adds
or removes L_reorder. A temporal oracle
(`raptorpath-math/tests/temporal_oracle.rs`, part 4, up to 1.2 M symbols)
confirms the reduction, the reorder term (a slow-path share of 0.25 measured
as 0.258 of symbols late when H < skew, 0.0025 when H ≥ skew) and the
monotonicities; the closed form needs about 1.5× r* to reach the target, the
Gaussian-tail gap Section 4.7 corrects. This law is a model statement: the
engine computes r* on the worst-loss path (Section 4.4) and does not evaluate
E.

### 4.12 Proactive and reactive planes at one price

The proactive and reactive planes spend the same bandwidth against the same
latency. A reactive clock `W(α) = SRTT + k(α)·σ` with false-alarm budget α has,
per source symbol,

```text
   L_react(α)  =  ν·α·(1 + h/T_pay)  +  δ·p·k(α)·σ / d ,     k(α) = √((1−α)/α)
```

with ν the fire rate per delivered symbol, p the realised loss and h the
repair header. Equating its implied price with the shadow price of r*
(Section 4.9) gives the consistency condition

```text
   α^{3/2}·(1 − α)^{1/2}   =   p·σ·G(u) / ( 2·ν·D_arq·(1 − ε̂) )     interior r* > 0
   α^{3/2}·(1 − α)^{1/2}   ≤   p·σ·G(u) / ( 2·ν·D_arq·(1 − ε̂) )     corner r* = 0

   G(u)  =  √(2π)·e^{u/2}/√u ,     u  =  W·ε̂ / ((1 − ε̂)·σ²_burst)
```

The header ratio and Copa's normaliser d cancel, because each appears once in
each leg; δ cancels too, so the condition does not depend on the latency
price. G is minimised at u = 1 (G(1) = √(2πe) = 4.13), so the un-echoed
inputs W and ε̂ can only loosen the bound. The left side peaks at
3√3/16 = 0.325 at α = 0.75, so a right-hand side above 0.325 imposes no
bound. At the shipped corner (Bulk, χ = 0) the condition is the inequality: an
upper bound on the reactive false-alarm budget. Its dominant error term is
σ, which Section 7.4 shows the tree cannot yet estimate stably.


---

## 5. The Span Machine and Multipath

### 5.1 Retention is ρ

A sliding-window code by itself is not a reliability contract. Moving the
Bulk hint onto an evicting window pipeline (which force-advances past its
window and force-delivers past unrecoverable holes) turned 10/10 completions
at 0.90 s into 0/10 DNF at a 2.5 % GE cell. The failure was the pipeline's
policy, not the code.

Retention is the triangle's ρ, realised by a give-up age T_cut(ρ). The
sender keeps a **store** of every sent source symbol's bytes; an entry
leaves the store on acknowledgement or when its age exceeds T_cut(ρ). ρ = 1
gives T_cut = ∞ (ack-only removal). Store fullness is backpressure on the
source (Section 6), never data loss. The coding window slides freely and is
only the FEC horizon; a localised hole that has aged out of it is recovered
by an exact retransmit from the store.

Two consequences follow. First, "reliable" and "lossy" are the ρ → 1 limit
and the finite-T_cut interior of one dial, not two policies. Second, the
shipped machine exposes ρ only structurally: the reliable window seat is
ρ = 1, and the evicting Realtime seat is the only ρ < 1 seat. ρ is not a
runtime input anywhere in the engine (Section 11.1).

### 5.2 One decoder

Every RLC-family wire symbol is a self-describing linear equation over the
global source-sequence space:

```text
   repair (a, w, i):   Σ_{c=0}^{w−1} coeff(a, w, i)[c] · x_{a+c}  =  payload
   source s:           x_s  =  payload
```

The set of determined sources is a property of the equations, not of the
decoder. Three receive machines existed: a sliding-window decoder computing
the full closure by incremental elimination, a generation decoder computing
a block-restricted closure keyed by `(anchor, width)`, and a separate
two-layer streaming code for Realtime. The keyed machine is not a valid
drop-in for a moving-window wire: two covering repairs of a 2-loss burst
almost always carry different spans, and a keyed machine strands both
holes where global elimination solves them.

The shipped **unified decoder** (`fec/unified.rs`, `UnifiedDecoder`) is the
global closure with the generation machine's cost model:

* known columns never enter the matrix; they are eliminated payload-only;
* only coded rows are kept, in RREF, as one contiguous
  `[coeffs over the row's span | payload]` buffer (the union of two
  overlapping interval spans is an interval, so rows stay dense-over-span);
* a unit row delivers immediately and converts to a known column;
* a repair whose span is fully known is recognised redundant in O(w).

Cost per solve with k coded rows of span ≤ L is `O(k·L·S + k²·(L + S))`,
equal to the generation machine's bound on an aligned wire. Its delivered
set equals the sliding decoder's on every wire and contains the keyed
machine's on aligned wires; differential tests against both legacy machines
and a dense reference oracle enforce this. Under reordering it is a strict
superset: the legacy sliding decoder discarded a still-informative row when
a late source displaced its pivot.

At realtime operating points the unified decoder's matrix is empty at
essentially every sample: trailing-span repairs arrive solvable or deliver
immediately; per-arrival decode is 6–11 µs.

### 5.3 The sender span law

With one decoder, the difference between "realtime" and "bulk" is the
sender's emission span. All span structure derives from the recovery budget
in symbols, `N_δ = rate · D`:

```text
   D   =  min( b(δ)·RTT , 2·RTT )                     recovery deadline, RTT = est.rtt()
   A*  =  clamp( ⌈rate·D⌉ , 1 , W )                   span width
   M*  =  clamp( ⌈rate·2·RTprop / A*_q⌉ + 1 , 2 , 32 ) quanta in flight (generation seat)
   Δ   =  clamp( ⌈rate·J⌉ , 1 , 64 )                   trailing offset, J = jitter
```

`net/emit_source.rs` (A*, Δ, D) and `gen_pipe_depth` in `net/mod.rs` (M*). `rate` is
the windowed-max send-rate anchor (`SendRateAnchor`, bucket ≈ SRTT/2, window
≈ 8 SRTT, clock-gap quarantined). Each granted repair is coded over the
trailing span `[F, F + A*)` with `F + A* ≤ sent − Δ`, F the oldest
unresolved position, so every member has landed when the repair does. A
repair is solvable on arrival iff its span contains no in-flight member;
leading-window emission violates exactly this and measured −22 pp delivered
reliability on the streaming-family arms.

The two limits:

* **Small δ (Realtime):** A* = rate·b·RTT, small fresh spans trailing the
  frontier, delivery at the arrival of the k-th covering equation.
* **Large δ, ρ = 1 (Bulk):** D = 2·RTT, A* clamps at W, and on the generation
  seat M* is the pipeline depth that keeps the frontier advancing at link
  rate. With anchors live the depth term engages: +25–31 % at a 100 ms cell
  and +62–82 % at 200 ms over a cold-anchor control.

Between the limits δ moves A* and M* smoothly. An oracle continuity sweep
checks that no completion or tail cliff appears at any δ.

The law is only as good as its anchors. Before the anchor-hygiene repairs,
A* sat at 1 for the first ~10 s of a realtime stream (a cold 2 s EWMA of the
send rate) and was then poisoned from 1 to 38 by a post-stall ack flood; a
width-1 span is a duplicate of one symbol, so only 9 % of fed repairs were
useful. With the windowed-max send-rate anchor, A* reaches its derived value
by the second 500 ms sample and the stream's p90 fell from 94 to 78 ms.

| constant | provenance |
|---|---|
| D's cap 2 | derived: delivery plus one feedback round |
| b(δ) | derived (Section 5.4) |
| M* clamp [2, 32] | declared: cold-start floor and memory ceiling |
| Δ floor 1, cap 64 | structural floor; cap unprovenanced |
| repair grant ≤ 1 per source | structural: the source clock paces repair |

The span law's continuity is not pinned by an isolating pure-law test
(Section 11.1). The sender's D uses the loss estimator's smoothed RTT, not
the min-filtered RTprop the derivation names (Section 11.3).

### 5.4 The horizon coefficient b(δ)

```text
   b(δ)  =  clamp( 2^(−½·log₁₀(δ/δ_auto)) ,  ½ ,  2 )
```

`raptorpath_math::span_horizon_b`, read through `net::delta_budget_b_of`.
The three named δ are equally spaced in log₁₀ (+2, 0, −2) and the three named
b (½, 1, 2) equally spaced in log₂, so the log-linear map through them has
no free parameter. The engine pins b = 0.5, 1.0, 2.0 at the presets with
bit-exact `assert_eq!` (`delta_budget_b_is_the_dial_not_a_mode`), so a libm
that misses a point by one ulp fails the build instead of shipping a step.
The clamp is the law's range: b counts round trips and D's own cap is 2.

The same b feeds the span deadline D(δ), the shed deadline (Section 5.6),
the pool setpoint q(δ) (Section 6.1) and the contract stall
(Section 6.4).

### 5.5 Multipath: three decode predicates

Multipath completion is governed by which symbols the decoder must wait for.

1. **Striping a unit across paths = fork-join.** The unit completes at
   `max_i(x_i·K_u/g_i + d_i + a_i·RTT_i)`; heterogeneity makes E[max] far
   exceed any path's mean [Nelson1988]. Measured at the heterogeneous cell:
   8.8 Mbit/s, below the fast path alone (14.0).
2. **Whole-unit path affinity with in-order release = resequencing queue**
   [Xia2003]. The frontier is a running max over unit completions; a path
   whose unit delivery time exceeds the fastest by more than the reorder
   hold contributes holes. The sustained rate collapses to the eligible
   set, `T ≥ K / Σ_{i∈E} g_i`; at the heterogeneous cell E = {fast}, and both
   raptorpath's block-affinity scheduler and kernel MPTCP sat at 12.6 Mbit/s.
3. **Rateless coding over a horizon = K-of-N.** Completion is the
   `K_h(1+φ)`-th order statistic of the pooled arrival process, rate Σ g_i,
   with a skew term paid once per horizon [Joshi2014, Joshi2017].

| schedule | decode predicate | completion | measured at c8 |
|---|---|---|---|
| striping within a unit | all of these symbols, scattered | Σ_units E[max_i(·)] | 8.8 Mbit/s |
| unit affinity, in-order release | units independent, released in order | K / Σ_{i∈E} g_i | 12.6 Mbit/s (kernel MPTCP 12.6) |
| rateless over a horizon | any K_h(1+φ) pooled arrivals | K_h(1+φ)/Σ_all g_i + skew | oracle ×1.19 of the fast path |

(These c8 readings predate the substrate fixes of Section 8.6, when the fast
path alone reached 14 Mbit/s; they illustrate the predicates, not today's
engine.) The coding gain for multipath is the move from the expectation of a
per-unit maximum, paid once per unit, to one interior order statistic of the
pooled process, paid once per horizon. It grows with path heterogeneity and
vanishes on symmetric paths.

In-order delivery is not itself the bottleneck: with cross-path coding over
a sliding window and retention, the frontier can advance on any sufficient
subset at the pooled rate [Cloud2014]. What caps aggregation is
per-path-affine atomic units, and eviction. At a systematic operating point
(r ≈ ε), however, a source symbol on the slow path is a specific position
the fast path cannot decode around, and the frontier reverts to fork-join.

**The multipath window bound.** A frontier hole is raced by repairs over
the current window for about one feedback round on the slowest useful path.
If arrivals overrun the window during the race, retention stalls the source.
Sustained aggregation therefore needs

```text
   W_mp  ≳  Σᵢ gᵢ · (RTT_max + t_slack)
```

≈ 600 symbols at the heterogeneous cell, three times `MAX_WINDOW_SIZE`.
Under retention an undersized window costs recovery latency on aged holes,
not correctness.

**Coded-only transmission** (every wire symbol a combination, no systematic
source) removes the specific-position long pole in theory; an oracle reaches
×1.19 of the fast path at W ≥ 384. On the real stack it was refuted: 3.9
Mbit/s at the heterogeneous dual against 15.7 for the fast path alone,
because a combination striped to a path lands after the window has advanced
past it (Section 10).

**Decode cost.** A dense GF(256) solver runs 405 / 201 / 83 / 66 Mbit/s at
generation sizes 96 / 192 / 384 / 512; the earlier sparse per-pivot decoder
ran 6–38× slower and was the binding constraint of the generation seat until
replaced. Decode is not the bottleneck at the shipped operating points.

### 5.6 δ-honest shedding

At small δ, overload must be shed, not serialised: a whole-process stall
amplified into multi-second backlog on the evicting in-order pipeline, while
a machine that dropped messages past its horizon kept its p50 and p90. The
shed law derives both ends from the triangle:

```text
   D(δ)    =  min( b(δ)·RTT , 2·RTT )                             shed deadline
   1 − ρ   =  ε̂ · (1 − P_fec(r_live, ε̂, A*, σ²_burst))            shed budget
   shed  ⇔  age > D(δ)   ∧   shed_total + 1 ≤ (1 − ρ)·src_total
```

`shed_deadline_us`, `shed_allowed` (`net/shed.rs`),
`control/fec_rate.rs::residual_loss_after_fec`; gate `RWM_UNIFIED_SHED`,
default on. The budget is evaluated with the transmit-side EWMA loss, the
spare-capped live rate and the span width A* as the window, inside the
probabilistic retransmit branch. Beyond the budget the machine serialises:
ρ wins over δ. The retain (ρ = 1) seat never sheds; the law runs only on the
evicting seat.
The receiver arm holds a hole for `SRTT/2` while its give-up budget
(≤ ε̂_recv × frontier) lasts and falls back to `(4·SRTT).clamp(60, 300) ms`
when it is spent (`net/shed.rs::shed_recv_hold`); the `½` is b(δ_Realtime)
hard-coded rather than read from the dial (Section 11.3).

Measured: zero collapse reps in 96 completed tail reps; p99 medians at or
below the retired streaming code at every cell, size and seed (c2 37/40 ms
versus 40–43/52); 100 % delivered at the c3 message cell where the
streaming code delivered 79–81 %; about 0.25 % of messages shed, less than
the streaming code's 1.0 %, because in-window FEC recovers what streaming
abandoned. This battery is what made the unified machine the default.

### 5.7 The placement law

Every source and repair symbol is placed by one continuous marginal-cost
rule, sampled as a softmax (`Scheduler::place_costs`,
`place_probs_with_temperature`, `scheduler/place.rs`):

```text
   P(i)  ∝  exp( −(c_i − min_j c_j) / T ) ,      T = PLACE_TEMPERATURE = 0.15

   c_i  =  L_i  +  B_i  +  V_i                                  (shipped)
   L_i  =  ( E_i − min(S, 9/8·srtt_i) )⁺ / ref                  completion-time term, S = 0 by default
   E_i  =  (in_flight_i / cwnd_i)·srtt_i  +  srtt_i/2  +  ε_i·srtt_i
   B_i  =  w_bw(δ) · r_i ,    r_i = ε_i/(1 − ε_i)  (10.0 for an unmeasured path)
   V_i  =  w_div · fate_i      (repairs only, w_div = 1.0)
   w_bw(δ)  =  clamp( ½ − ¼·log₁₀(δ/δ_auto) , 0 , 1 )
```

ref is the fastest active path's srtt, floored at 1 ms.
`SchedulingWeights::from_delta`: w_bw is exactly {0, ½, 1} at Realtime,
Auto, Bulk (pinned bit-exact by `scheduling_weights_are_the_dial_not_a_mode`);
the ¼ is the dial's width (δ spans four decades, the weights span 1). E_i
contains the path's current queueing delay, so the apparent load regimes are
equilibria of one rule: under light load the best path takes everything; as its queue
builds, its cost rises until the next path's is crossed; under backlog
marginal costs equalise (water-filling as the fixed point). The candidate
set is every path whose link is up; there is no capacity filter.

**The frontier term.** A placement that pushes a symbol behind the
cumulative frontier inflicts a head-of-line stall. The complete law adds

```text
   X_i  =  [ δ·s_i  +  κ·(s_i − H)⁺ ] / ref ,     s_i = [ (now + E_i) − F̂ ]⁺
```

with F̂ the running max of stamped arrival estimates of symbols already
placed and H the store headroom (Section 7.3); the implemented arm adds a
behind-frontier term `W·behind/ref`. The temperature is not a free dial but
the Gumbel scale of the scheduler's own ETA error: a softmax is
argmin under i.i.d. Gumbel noise of scale T [McFadden1974], so

```text
   T  =  (√6/π) · σ̂_e / ref
```

and the shipped `PLACE_TEMPERATURE = 0.15` asserts σ̂_e ≈ 0.19·ref at every
cell. The derived diversity weight prices only the excess burst probability,
`fate_i·(p_BB,i − ε_i)⁺·srtt_i/ref`, which vanishes on a memoryless channel
(0.475 at c2, 0.552 at c3, against the shipped 1.0).

The three derived forms are built as arms, absent by default:
`RWM_PLACE_T_DERIVED`, `RWM_PLACE_HOL` (X_i), `RWM_PLACE_WDIV_DERIVED`;
with all three absent the cost table is byte-identical to a pinned table
(`place_costs_match_the_pinned_table`).

**Measured (the placement battery, `5e6e3f5`).** Placement-manufactured
wait is about 6 % of delivered latency at both dual cells; queueing above the
path floor is the largest term; all three arms were inert on latency; and
the shipped 0.15 is not a stable quantile of the measured ETA error
(Section 9.8). At the dual cells 92–96 % of detected holes are closed by the
other leg catching up, i.e. manufactured by placement skew rather than by
loss (π₀, Section 7.2).

### 5.8 The generation seat

A bulk object can also be coded in **generations**: a pinned anchor and
width G (`RWM_GEN` = 384), repairs over the whole generation, M* generations
in flight. The unified decoder block-diagonalises on this aligned wire and
matches the generation machine's cost. Two corrections made the seat viable.
The coded-only wire (every symbol a combination) made every degree of freedom
arrive dense at both ends, O(G²·S); the systematic-repair wire sends source as
primary and codes only the deficit, O(k·G·S + k³), and doubled single-path
goodput. And a sparse per-pivot decoder ran 6–38× slower than a dense
GF(256) Gauss–Jordan with SIMD multiply-accumulate; the dense decoder clears
the link rate at G = 384. The seat is not in the default machine (generation
coding is a CLI flag), its repair budget is a constant floor (0.15
systematic, 0.20 coded) rather than r*, and it is where the depth term M* of
Section 5.3 engages.

### 5.9 Correction accounting across paths

All paths share one source stream, one retransmit store and one coding
window, so a correction on any path can recover a loss on any other.

* **Correction deficit.** `deficit = Σ_{unacked s} ε_path(s)`: each sent
  symbol adds its path's loss rate and each acknowledgement removes it. Lost
  corrections add to the deficit, which is why the IT minimum is ε/(1 − ε)
  (the geometric chain ε + ε² + …). With source on path A and corrections on
  path B the equilibrium is `r = ε_A/(1 − ε_B)`.
* **Cross-path diversity.** A correction on B survives a burst on A; for
  independent paths P(both fail) = ε_A·ε_B, so a 10 %-loss path protected from
  a 2 %-loss path fails 0.2 % of the time.
* **Interleaving.** Source and corrections must not be separated by path: a
  burst on a source-only path has no interleaved corrections to survive it.

The engine keeps a `CorrectionDeficit` counter, but its placement law does
not read it.

### 5.10 One pipeline, not mode switching

Keeping two pipelines and switching between them is the defect, for four
measured reasons.

1. **No cross-code algebra.** A repair of one code cannot help decode
   another's in-flight data, so a mid-stream switch strands every in-flight
   symbol or forces a drain. A window-mode backend switch that restarted
   sequence numbers left the acknowledgement machinery blind for a full
   window and wedged an inner TCP for minutes.
2. **State does not transfer.** A block ledger keyed by (block, batch) means
   nothing to per-sequence SACK state.
3. **Threshold selectors oscillate.** A backend chosen by hard loss
   thresholds with a debounce buys periodic switches when ε̂ sits near a
   threshold.
4. **Two pipelines are built twice and debugged never.** Window mode's
   reactive repair path was once dead code (no NACK producer) while the block
   pipeline received pacing and ARQ.

The resolution is one pipeline parameterised by retention, span, window and
the (δ, ρ, r) triangle, with codecs chosen per stream at setup and never
switched mid-stream. The Bulk/Auto block default of Section 1.5 is the
remaining instance.


### 5.11 Per-stream contracts

A tunnel commits to one point of the triangle. It cannot carry a tight-δ
realtime flow and a loose-δ bulk flow over the same paths at the same time
with each getting its own contract. The unified code makes the remedy
straightforward to state, although it is not built:

* give each stream m its own coding context and triangle (δ_m, ρ_m, r_m,
  W_m), with W_m from `derive_window` and r_m from the rate law at δ_m;
* independent coding is budget isolation: a bulk stream's loss burst draws
  down only its own repairs;
* the placement law gains a per-stream weight and an urgency term, for
  example the exposure kernel χ evaluated at the stream's δ_m and remaining
  slack, so a symbol about to become serial for a tight-δ stream outranks a
  bulk symbol with slack.

Per-stream contexts lose cross-stream coding gain: a shared window over
streams A ∪ B has an O(1/√W) smaller margin. Against that, a shared window
lets a loose-δ burst consume the tight stream's repairs, a categorical breach
of the tight contract. Isolation is preferable whenever the streams' δ differ
materially; equal-δ streams should share a window, which is one stream at a
larger W. Unequal error protection (one window, coefficients weighted toward
the tight stream) keeps the coding gain at the cost of a coupled encoder.
With one stream the construction reduces to the present machine.

---

## 6. Flow Control

### 6.1 The pooled outstanding cap

On the reliable window seat the only brake on outstanding data is the store
cap: the sender stops reading source when
`store_len ≥ effective_store_cap` (the per-path cwnd gate is inactive by
default, `RWM_INFL_CAP = 0`). With two or more live paths the cap is

```text
   cap  =  clamp( (1 + q(δ)) · Σᵢ bwᵢ·RTpropᵢ ,  floor ,  N·knee )

   q(δ)  =  q_lo + (q_hi − q_lo) · ( b(δ) − ½ ) / (2 − ½)  =  (b(δ) + 1) / 30
```

`pooled_store_cap`, `pool_value_multiplier`, `codel_setpoint_q`
(`net/store_cap.rs`); gates `RWM_SUM_CAP` and `RWM_DELTA_CAP`, both on
by default. The sum runs over all live paths (`net::channel_paths`, the
channel's membership); before plan 2b it ran over `active_paths()`, the live
paths with spare congestion window, and `RWM_STORE_CAP_UNIFIED` (retired)
was the live-set arm. Under Copa-sole pass-through the anchor is Σ cwnd.

| symbol | value | provenance |
|---|---|---|
| Σᵢ bwᵢ·RTpropᵢ | per-path windowed-max rate × windowed-min RTT | measured anchors |
| q_lo | 0.05 | cited: RFC 8289 §3.2, the conservative end of CoDel's derived 5–10 %-of-RTT setpoint band [RFC8289] |
| q_hi | 0.10 | cited: RFC 8289 §3.2, the Kleinrock peak-power end |
| b(δ) | ½ … 2 | derived (Section 5.4) |
| affine shape of q(b) | — | declared mapping between two cited endpoints and two dial endpoints, no free parameter; a log-linear map (Auto 7.07 %) is equally free of constants |
| floor | 10 | derived: max(ANCHOR_MIN_SAMPLES·cadence, RFC 6928 IW) |
| knee | 2048 per path | measured but stale (`RWM_STORE_PATH_POOL`) |

At the named points q is 0.050 (Realtime), 0.0667 (Auto), 0.100 (Bulk):
one BDP per path plus a CoDel-sized standing-queue allowance. As q → 0 the
cap is exactly one BDP per path with zero standing queue, the null candidate
a newsvendor argument also reaches, so the derived band is that null plus the
power-point allowance.

The derivation compresses the dial's authority. The span deadline lets Bulk
tolerate 2·RTprop; the setpoint band stops at 10 % of it. CoDel's power
function `(1 + 2f − f²/3)/(1 + f)²` peaks near f = 0.1 and falls beyond it,
so queue past the knee buys no goodput and costs delay. A law that grants
more (the composed cap, Section 10) measured 43–48 % worse delivered
latency at goodput parity.

**The count multiplier.** The predecessor was
`clamp(gain·N·Σ, floor, N·knee)` with gain = 2.0. At a symmetric cell
Σ = N·a, so the value was `gain·N²·a`, quadratic under a linear ceiling, and
the saturation condition `Σ ≥ knee/gain = 1024` is path-count-free. The
measured anchors (1635 at c7, 1510 at c8) sat 1.5–1.6× above it, so the cap
read exactly 4096 in 121 of 126 dual reps across five sessions: the clamp
hid the law's shape from every measurement. Deleting the ×N
(`RWM_SUM_CAP`) moved the law interior at both duals and cost nothing
measurable; it under-funded the c8 resequencing span by 45 % and goodput went
up. The δ-cap then replaced gain = 2.0, a value with no derivation, by
1 + q(δ).

**Measured (`RWM_DELTA_CAP`, 452 live invocations, one binary).** Interior at
c7 and c8 with the ceiling provably inert (pin fraction 0.0000):

| cell | Δ goodput, seed 42 / 7 (2σ_pooled) | Δ q_p50 |
|---|---|---|
| c7 | −0.15 (5.71) / −1.28 (4.28) | −16 / −10 ms |
| c8 | +7.83 (11.13) / +1.25 (14.09) | −113.5 / −117 ms |
| c8L | −2.64 (38.09) / −0.78 (36.72) | −200 / −130 ms |

Goodput parity at every dual on both seeds, and delivered queue down at every
one. The paired dead-wall contrast at c8 favours the δ-cap in 18 of 23
non-zero pairs (two-sided sign test p ≈ 0.011). At c8L the pin fraction is
0.23, between the pre-declared branches, so no verdict is claimed there. The
law returns before any multiplier is read at N < 2, so single-path cells are
bit-identical: it can neither regress nor help a single path.

### 6.2 The single-path cap

With one live path the pooled law is not engaged and the cap is

```text
   cap₁  =  clamp( gain · BtlBw·RTprop ,  10 ,  1024 ) ,     gain = RWM_STORE_GAIN = 2.0
```

falling back to a boot cap of 128 symbols (`RWM_STORE_BOOT`) until the
anchor has samples. gain = 2.0 ("one BDP of pipe plus one of recovery
runway") and the 1024 ceiling (`RELIABLE_STORE_MAX`) are unprovenanced, and
the boot cap is a cliff: when the anchor set comes back empty the cap drops
to 128 for that refresh (×6.5 at c1). A δ-priced setpoint in this seat is the named successor
(an earlier measurement showed a 481-symbol cap cutting sc2 ping p50 by
55 ms at parity); it is not implemented.

### 6.3 SACK-clocked release

The store releases payload only when the cumulative frontier passes a
symbol (`sent_store.split_off(ack + 1)`): the store is the only payload
copy, and pruning it on SACK was measured unsafe (in-order duals wedged).
Flow control, however, counts a SACKed symbol as released:

```text
   store_len  =  retained − released        (released = SACKed, not yet cumulatively acked)
```

`sack_release_mark`, gate `RWM_STORE_SACK_RELEASE`, default on. A SACK
therefore frees a flow-control slot while payload and every recovery
structure stay until the frontier passes. Measured at the symmetric dual:
goodput 142.9 → 154.8 Mbit/s alone and 1.02–1.05×Σ of the same-session
single-path sum when composed with per-path loss detection (Section 7.1),
with retransmits falling (21.6 k → 17.2 k per 200 MB alone, 5.2 k composed).
Mean counted occupancy fell from 3 157 to about 1 460 against a 4 096 cap,
with about 167 k slots released per 200 MB; the counted store returns to the
unacknowledged count to within 0.5–1.0 %, lagging only by the gap-report
interval. At a single path the term is also positive (sc2 +4.3 / +2.9
Mbit/s), because a SACK above a hole holds a slot there too.

### 6.4 The contract stall

The time a retained, genuinely lost hole freezes the frontier is declared by
the contract rather than fitted:

```text
   stall(δ, ρ)  =  (1 − ρ)·D(δ)  +  ρ·(9/8·SRTT + SRTT)
```

`net/store_cap.rs::contract_stall_s`. Both terms are always computed and the expression
is continuous in ρ. 9/8 is RFC 9002's kTimeThreshold, an empirical
recommendation (RACK uses 5/4). This expression is consumed only by the
three-term cap, which is off by default (Section 10).

### 6.5 What flow control does not do

The per-path congestion window does not gate intake on the reliable seat,
the placement candidate set does not test capacity, and the CC pacer debits
source tokens only (and does not run on the plain reliable path). The four
mechanisms — CC, rate law, retention, placement — are separately specified
but share one operand, the store cap. The divergence between source-only
pacing and total wire occupancy is bounded by a test that asserts
`wire = src + taper + retx + margin` and `wire > tokens`
(`store_cap_sf_bench.rs::pacer_debit_bounds_only_the_source_arm_not_the_wire`).

### 6.6 Why the pins passed

The ×N pool is the record's clearest case of a law exhaustively tested and
still wrong. It carried nine always-on absolute pins, two component benches,
an engine-equivalence pin and a wire gauge for a month. Every pin asserted
that the code computed the model; none asked whether the model was right.
Five mechanisms hid it: the clamp ate the evidence (4 096 in 121 of 126
reps); N ∈ {1, 2} was the entire test universe, and the exponent is
distinguishable only at N ≥ 3; two defects masked each other (the anchor
over-read ×4.6–7.4 while the multiplier over-scaled by N); the pinning was
filed as a fact about the cells rather than about the formula; and the
formula was never read as a formula.

The prevention is now code: law-shape property tests that sweep N = 1…8
synthetically, with the unclamped formula tested separately from its clamp; a
bind-fraction gauge on every clamp, with a law measured pinned treated as a
defect finding; a symmetric four-path bench cell (`c7x4`); and design review
of the formula, checked in shape (order in N, units, monotonicity) before any
number.

### 6.7 Pooling and correlation

Eppen's risk-pooling theorem [Eppen1979] says a shared stock beats N
dedicated ones by an amount that vanishes as demand correlation approaches
+1. On the per-path drain of the shared pool the cross-path correlation is
−0.814 at c7 and +0.612 at c8 (Fisher p = 0.009), a pooling benefit of 0.695
against 0.102, matching the two measured verdicts: pooling defended at the
symmetric dual, never ahead at the heterogeneous one. The analogy strains at
one joint. Eppen's demands are exogenous, while here one flow is split by a
work-conserving scheduler against a binding total, so N exchangeable series
summing to a constant have mean pairwise correlation pinned at −1/(N − 1).
Positive correlation therefore comes from a shared constraint that starves
every path at once; at c8 both legs collapse in the same window while the
fast path parks the un-SACKed frontier span. Whether pooling loses at c8
because demands are correlated, or demands are correlated because of pooling,
needs a per-path-account arm at the same geometry.


---

## 7. The Recovery Decision

### 7.1 The shipped recovery plane

A hole is a sequence number the receiver has not received while a later one
has arrived. The receiver reports holes in `WindowAck` SACK ranges, at most
once per `GAP_ACK_MIN_INTERVAL` = 2 ms. The sender turns a reported hole into
an exact retransmit when its per-path loss-detection law fires:

```text
   time threshold:    age(live flight)  ≥  max( 9/8 · max(SRTT_path, EWMA_path) , 10 ms )
   packet threshold:  ≥ 3 later symbols on the same path are known delivered
```

`mp_time_threshold_split`, `MP_PACKET_THRESHOLD = 3`, `mp_hole_ripe`
(`net/recovery_laws.rs`); gates `RWM_RECOV_MP` and `RWM_RECOV_MP_LAW`, default on.
At one path the law does not suppress at all: every reported hole is
answered at once. It is RFC 9002's loss detection [RFC9002] generalised per
path: the live flight is the last (re)send, so a retransmit is clocked on its own path, and
the cross-path packet threshold is deliberately not used, because cross-path
sequence gaps are exactly the reordering that multipath QUIC handles with
per-path packet-number spaces. The previous global gate read
scheduler-created cross-path gaps as holes: 82 % of retransmits at the
symmetric dual fired while their flight was still inside its own path's
expected arrival window. Per-path detection cut the retransmit share there
from 14.9 % to 4.5 % of source (below the single-path 8.2 %) and removed the
dual-c1 retransmit flood (8.5 % → 0.7 % of source). A time threshold alone
cut the waste and cost throughput (on a frontier-serialised store, recovery
latency buys back every megabit the waste had cost); the packet threshold is
the fast channel that makes the law pay.

Unresolved holes are re-probed on a refresh cadence and a tail sweep:

```text
   refresh  =  (2·SRTT).clamp(25 ms, 100 ms)          (100 ms before the first RTT sample)
```

`hole_nack_refresh`, `tail_sweep_timeout_us` (`net/recovery_laws.rs`); a
per-sequence retransmit cooldown has a 10 ms floor. The pooled SRTT these
clocks and the cooldown read, and the repair margin's loss rate, are taken
over the live paths (`recovery_clock_paths`), not the cwnd-saturation-filtered
`active_paths()`, which is empty when every path is cwnd-full and dropped the
clocks to their floors exactly at saturation. The bind fractions below were
read on that saturation-filtered set and are not re-measured. Measured, this clamp binds
92.4–99.7 % of the time at every cell (the 25 ms floor at c1, the 100 ms
ceiling elsewhere): its `2·SRTT` term is inert and the law is, in practice, a
constant. Its false-alarm rate exceeds RACK's own spurious budget (1/16) by
1.7–12.0× at all five cells. Every successor written for it was refuted or
measured inert (Section 10), so it remains, recorded as unprovenanced
(Section 11.2).

### 7.2 One sequential test

**Hypotheses.** A hole is detected. Either the original is in flight
(reordering, H₀, prior π₀, lateness distribution F) or it is lost (H₁,
prior π₁ = 1 − π₀). The only observable is a non-event: no arrival yet at
lateness ℓ, which has probability S(ℓ) = 1 − F(ℓ) under H₀ and 1 under H₁.
The likelihood ratio is

```text
   Λ(ℓ)  =  1 / S(ℓ)
```

Λ starts at 1 and never decreases, so Wald's lower boundary is never
reached; the test accepts H₀ only by the arrival itself. The miss probability
β is 0 and the single boundary A = 1/α gives

```text
   declare loss   ⇔   S(ℓ) ≤ α   ⇔   ℓ ≥ F⁻¹(1 − α)
```

**Every recovery clock written for this machine has this form**, on a
different measurand:

| clock | measurand | form |
|---|---|---|
| Cantelli margin | ack inter-arrival | `W(α) = SRTT + √((1−α)/α)·σ` |
| quantile-native | ack inter-arrival, order statistic | `Y_(N−K+1)` |
| hold-down | hole outstanding time at the sender | `T(q) = W_q(1 − q)` |
| lateness | hole lateness at the receiver | `F⁻¹(1 − α)` |

They are one one-sided sequential test pointed at four streams, with the
false-alarm budget α in each. Moving the threshold of a test on the wrong
measurand moves nothing: 98.99 % of 107 597 classified fires were driven by
gap data, 0.59 % by timers. The optimal-detection literature [Wald1948,
Lorden1971, Moustakides1986] returns the incumbent's form, ℓ ≥ F⁻¹(1 − α),
and says nothing about its constants; CUSUM has nothing to accumulate because
H₁ emits no observation.

**The measured prior.** Attributing each closed hole to the arrival that
closed it:

| cell | π₀ (true self-heal) | 95 % CI | closed by |
|---|---|---|---|
| c1 (single) | 0.0077 | [0.0042, 0.0142] | the sender's own retransmit, 99.2 % |
| sc2 (single) | 0.0054 | [0.0037, 0.0078] | the sender's own retransmit, 99.5 % |
| c7 (dual) | 0.9606 | [0.9599, 0.9614] | the other leg catching up |
| c8 (dual) | 0.9233 | [0.9190, 0.9273] | the other leg catching up |

At a single path there is almost nothing to wait for. At two paths almost
every detected hole is manufactured by placement skew. The true-heal
lateness distribution at the duals has p50 12.8 ms (c7) and 8.2 ms (c8). π₀
is an upper bound: the classifier that splits heal from retransmit-closed
uses an `SRTT/2` window, which attributes near-simultaneous closures to the
original.

**The lateness coordinate.** For a missing sequence s, let s⁻ and s⁺ be the
nearest arrived sequences below and above it. Under per-path FIFO the would-be
arrival instant of s is bracketed, `Â(s) ∈ [arr(s⁻), arr(s⁺)]`, and its
lateness at time t is `ℓ = t − Â(s)`. Detection age is the special case
`Â = arr(s⁺)`, the latest admissible origin, so a distribution measured in
age over-states the chance of self-healing by any T: the bias favours
waiting, and every conclusion below is conservative under a sharper
coordinate. Measured in age, the per-path time threshold is already met on
98 % of holes the first time the receiver mentions them at c1 (age p50 26.6 ms
on a 2 ms path), 17 % at sc2, 7 % at c8 and 2 % at c7: at a loaded fast path
the age threshold has nothing left to suppress.

### 7.3 The value of waiting

Per detected hole, stopping the decision at lateness T, in repair symbols:

```text
   R(T; δ, ρ, r)  =  π₀·S(T)·w                           wasted copy
                  +  π₁·P_arq(ρ, r)·δ·(T + d)/d          priced delay of a genuine loss
                  +  π₁·P_arq(ρ, r)·Φ(T)                 frontier stall

   Φ(T)  =  g·κ·(T + d − H)⁺ / T_pay
   H     =  (S_cap − BDP)/g  =  (m − 1)·RTprop_w
```

d is the ARQ resolution delay, g the goodput, κ ∈ (0, 1] the non-overlapped
fraction of stall time, T_pay the payload size.

**The frontier term, from the store cap.** On the ρ = 1 contract a genuine
loss freezes the cumulative ack for T + d. If the flow-control count were
released only by the cumulative frontier, the store would absorb
`S_cap − BDP` symbols of frozen frontier, i.e. H seconds, before the sender
must stall. For a store multiplier m (cap = m·BDP):

```text
   H  =  (m − 1) · RTprop_w          RTprop_w = rate-weighted RTprop of the summed paths
```

At one path m = gain = 2.0 and H is one RTprop. At two or more paths the
shipped δ-cap makes m = 1 + q(δ), so H = q(δ)·RTprop_w: 0.8 ms at c7 at the
Bulk point, not 8 ms. With the three-term cap armed, H becomes the contract
stall of Section 6.4 and is continuous in δ and ρ.

Two qualifications apply to the shipped stack. First, SACK-clocked release
(Section 6.3, on by default) uncounts SACKed symbols, so symbols arriving
above a frozen hole do not fill the counted store and the derivation above is
an upper bound on how quickly the store stalls, not an identity; payload,
unlike the count, is still released only by the frontier. Second, H is
observable directly as the delay from a frontier freeze to the onset of the
receiver's arrival stall (`[WIDLE]`), which read 4.6–5.1 ms at c7
(Section 9.8). The observed H, not the derived one, is what the request law
of Section 7.6 reads.

**The frontier term, checked offline.** Fitting the aggregate form
`L = 1 − g/g_CTL = x/(1 + x)`, `x = κλ₁·(T − (H − d))⁺` to 283 per-rep
(realised wait, goodput) points from the hold-down batteries recovers
`H − d = +8 ms` at c7 with R² = 0.992, independent of the gain reading it
agrees with to 11 %. With λ₁ computed from the record rather than fitted, κ
is 0.0048–0.067 at the four audited cells: admissible (κ ≤ 1), and far below
the conservative κ = 1 the placement frontier term declares.

**The theorem (a bound, not a law).** Let T* = argmin R. Then

```text
   (i)   T*  ∈  [ 0 ,  min( (H − d)⁺ ,  F⁻¹(q_d) ) ]
   (ii)  value(T*) − value(0)  ≤  R_frac · π₀ · F( (H − d)⁺ )
   (iii) T* = 0   ⇔   H ≤ d   or   q_d → 0
```

where R_frac is the machine's repair-traffic share of the transfer and q_d
the route-(d) limit of the loss function below. Above the knee T = H − d the
cost density jumps up while the benefit density is non-increasing, so an
interior optimum lies at or below the knee. Waiting can pay only inside the
headroom the store already carries, and what it can win there is capped by
the repair traffic, times the share of holes that self-heal, times the
lateness mass inside the headroom.

The value bounds below were evaluated with m = 2 at every cell. Under the
shipped δ-cap the dual cells' headroom is smaller, so their domains and
bounds are upper bounds, and the receiver-law battery observed H = 4.6–5.1 ms
at c7 (Section 9.8).

| cell | H (m = 2) | d | (H − d)⁺ | R_frac | π₀ | value bound |
|---|---|---|---|---|---|---|
| c1 | 2 ms | 1.05 ms | 0.95 ms | 0.33 % | 0.0077 | ≤ 0.0026 % |
| sc2 | 8 ms | 4.37 ms | 3.63 ms | 4.08 % | 0.0054 | ≤ 0.022 % |
| c7 | 8 ms | 0.78 ms | 7.22 ms | 3.21 % | 0.9606 | < 1.54 % |
| c8 | 8–60 ms | 3.30 ms | 4.7–56.7 ms | 4.08 % | 0.9233 | < 1.88–3.39 % |

At the single-path cells the corner T* = 0 holds at every named δ, decided by
the prior. At the dual cells an interior optimum exists at Auto and Bulk
and the corner holds at Realtime. Every waiting time any battery realised
(minimum 25.6 ms, at c7) lay outside the domain where waiting can pay (7.22
ms at c7).

The corner at the named points follows from comparing the self-heal
quantile density near the origin with `R = w·π₀·d/(π₁·δ)`:

| cell | R at δ = 1 (ms) | Realtime (δ = 50) | Auto (δ = 0.5) | Bulk (δ = 0.005) |
|---|---|---|---|---|
| c1 | 0.0082 | corner | corner | corner |
| sc2 | 0.024 | corner | corner | corner |
| c7 | 19.2 | corner | interior | interior |
| c8 | 40.2 | corner | interior | interior |

The two regimes separate by the prior, not by any timing: at single paths
the corner holds at every named δ; at the duals it holds only at Realtime.

**Dial dependence.** δ enters as a multiplier of the delay cost and through
H when the three-term cap is armed; ρ enters only through
`P_arq(ρ, r) = 1 − (1 − ρ)/(ε̂·(1 − P_fec(r)))`, which multiplies both cost
legs; lowering ρ moves the optimum toward waiting. The only indicator,
`1{T + d > H}`, is on the decision variable T.

### 7.4 The clock family

Four sender-side clocks were derived and measured. They are one test on four
measurands (Section 7.2), and each failed for a reason worth keeping. The
Cantelli and quantile-native arms are removed from the engine; the hold-down
arm remains, off, and the quantile-native order statistic survives as the
hold-down's window law (`net/recovery_clock.rs`, `net/holddown.rs`).

**Cantelli margin.** Let X be the ack-arrival time; a false alarm is X > W, so
the clock is a quantile `W(α) = F_X⁻¹(1 − α)`. Cantelli's one-sided bound
gives a distribution-free upper estimate:

```text
   W(α)  =  SRTT + k(α)·σ ,       k(α) = √((1 − α)/α)
```

Pricing α from the contract's tail-loss target (10⁻⁵ at Auto) gives k = 316
(W = 3.24 s) at Auto and 3 162 (31.7 s) at Realtime against a 100 ms shipped
ceiling, and an empirical 1 − 10⁻⁵ quantile needs about 10⁵ samples that a
min-deque RTT store does not hold. The deeper error is one of units: the tail
target is the probability a symbol is never delivered, α the probability a
retransmit is wasted; they are different failures with different costs.

**Dispersion.** Any `SRTT + k·σ̂` form needs a σ̂ that is stable across rates.
The shipped variance EWMA and a fixed-sample-lag successive difference read
σ at c8 as 0.191 / 3.140 / 54.8 ms across three reps (287×), reproduced on
loopback: the estimator, not the path. A lag of one sample is a lag of
1/rate seconds, so the statistic measures a different time scale at every
rate. The rate-invariant form fixes the lag in time:

```text
   σ̂_Δ(τ)  =  median{ |rtt(tᵢ) − rtt(t_{j(i)})| } ,   j(i) = latest j with tᵢ − tⱼ ≥ τ ,
              kept only if tᵢ − t_{j(i)} ≤ 2τ ,        τ = RTprop
```

[vonNeumann1941, Allan1966]. It confirmed rate invariance on the wire and
still missed its pre-registered stability bar.

**Quantile-native.** Delete σ and read the order statistic directly:

```text
   W_q(α)  =  X_(N−K+1) ,      N(α) = max(⌈K/α⌉, 2K) ,   K = 10
```

the K-th largest of the N most recent samples. The arms separated, but the
commanded false-alarm rate did not track α at five of five cells: the
measurand was wrong, because 98.99 % of fires answer receiver gap reports and
only 0.59 % are timer-driven.

**Hold-down.** Set the other clock: on a reported hole, emit a repair only
after the hole has been outstanding for `T(q) = W_q(1 − q)` on the
hole-resolution stream. The level comes from the cost per reported hole,

```text
   C(T)  =  w·π₀·(1 − F(T))  +  δ·(1 − π₀)·P_arq·T/d
   ⇒   s(q*)  =  w·π₀·d / ( δ·(1 − π₀)·P_arq ) ,     s(q) = dF⁻¹/dq ,   w = 1 + h/T_pay = 1.011
```

a density condition in which the fire rate divides out and every input is
measured or declared. Measured, the lever engaged fully (97.3–99.6 % of fires
suppressed) and the realised false-alarm rate moved at most 1.66×. The
sender's estimand is censored: it observes `min(original, repair)`, and a copy
is usually already in flight. A lift of the receiver's report-cadence floor
entered the sub-floor region at three of three cells and found nothing there.
Section 7.6 moves the decision to the receiver, where the censoring vanishes.

**Scoring.** Every arm of every recovery battery is scored on one number,
Copa's own utility difference at the contract's δ,

```text
   ΔU  =  ln(g / g_CTL)  −  δ · ln(lat_p95 / lat_p95,CTL)
```

so a lever that trades throughput for latency cannot be reported as a win by
quoting one half.

### 7.5 The loss function over the triangle

The same decision written on the sender's clock, one loss per source symbol
and per repair decision:

```text
   L(α; δ, ρ, r, λ)  =  ν·α·(1 + h/T_pay)                              wasted repair
                     +  δ·owed(ρ, r)·k(α)·σ / d                        delayed genuine repair
                     +  λ·owed(ρ, r)·max(0, k(α)·σ − D(δ)) / D(δ)      overrun of the δ allowance

   k(α)       =  √((1 − α)/α)
   owed(ρ, r) =  max( 0 ,  ε̂·(1 − P_fec(r)) − (1 − ρ) )
```

ν is the fire rate per delivered symbol, h = 14 B the repair header. Route
(d) is λ → 0, route (b) is λ → ∞; which one models the machine is the value
of λ, which measurement has not supplied. The recovery clock the law sets is
`W(α*) = SRTT + k(α*)·σ`, which needs the rate-stable dispersion estimator
Section 7.4 did not find. None of this is implemented as a shipped clock.

### 7.6 The request law at the receiver

Every sender-side clock failed the same way: the sender cannot observe the
lateness of the original, because a copy is usually already in flight and
the observed resolution time is `min(original, repair)`. The sender-side
estimator's fixed point was zero (1 175 evaluations, zero fed samples).

The receiver holds the frontier, the lateness distribution and the rank.
The request law puts the decision there:

```text
   ℓ*   =  min{ ℓ ≥ 0 :  w·π₀·f(ℓ)  ≤  π₁·c_L } ∧ (H − d)⁺
   c_L  =  P_arq(ρ, r)·δ/d                                  constant in ℓ on [0, ℓ*)
   α    =  S(ℓ*)                                            derived, not declared
   request  ⇔  ℓ ≥ ℓ*
```

The first line is the stationarity condition of Section 7.3 in the
receiver's coordinate. The domain cap keeps ℓ + d ≤ H, so the frontier
indicator never fires inside it and ℓ* is a level set of the self-heal
density at a height the contract sets.

**Identifiability.** If the receiver is the sole authority for requests,
no copy is in flight before ℓ*, so on [0, ℓ*) the observed resolution is the
original's: the receiver's heal hazard equals π₀·f(ℓ) exactly, on exactly the
interval the decision reads. π₁ is bounded above by the observable fraction
of holes unresolved by any means, `π₁ ≤ S_tot(ℓ*)`, which biases ℓ* toward
earlier requests, the direction of the shipped machine.

**Both regimes from one expression.** π₀ → 0 (single path): the benefit
density vanishes and ℓ* = 0, request immediately, which is what the machine
does. π₀ → 1 (dual): the cost density vanishes, the inequality has no finite
solution and the cap binds, ℓ* = (H − d)⁺ = 7.22 ms at c7. The shipped
trigger (a 2 ms report sampler) is the α ≈ 1 corner applied at both regimes.

**The request message.** The shipped vocabulary is "sequence s is missing",
answered by a copy of s. Its generalisation is one message with a span and a
rank deficit:

```text
   REQUEST  =  (a, m, k) ,     m = clamp(⌈k_½(π̂₀)⌉, 1, A*) ,   k_½ = ln 2 / (−ln π₀) ,   k = holes − pivots
```

k_½ is the span at which the chance that all m holes self-heal, and a single
parity answer is wasted, halves: 0.14 at c1 and 0.13 at sc2 (so m = 1, the
copy exactly), 17.2 at c7 and 8.7 at c8. A request for k more independent
equations over [a, a + m) has no spurious answer, only redundant ones. At a
single path a parity answer with m > 1 would under-provide rank, because
almost every hole there is a genuine loss; the law reaches m = 1 there by
itself.

**One authority.** Four mechanisms can emit a copy for the same hole: the
gap-fire loop (the dominant source), the probabilistic taper copy (measured
zero), the refresh and tail sweep, and a rank answer. Under the request law
the receiver's report is the single authority; the taper survives for the
regime where no report can arrive and the tail sweep for the end of stream.
The per-sequence gap producer is the one site to suppress; the SACK path is
left alone, because SACK drives slot release, never recoverability.

**Implementation.** `net/late.rs` computes the receiver-observable form,
with the price entering as one bar,

```text
   request  ⇔  ρ̂_heal(ℓ) ≤ c/(w + c) ,     c = δ/δ_auto ,   w = 1
```

(`request_bar`), which is ½ at Auto, rises toward 1 as δ rises (Realtime
waits less) and falls toward 0 as δ falls (Bulk waits the whole headroom).
ρ̂_heal counts a hole as healed only when its own source symbol closes it and
the closing batch's sender stamp is not later than the stamp of the arrival
that exposed the hole: originals are stamped in sequence order by one clock,
so a later-stamped closer is the sender's copy (`HoleOutcome::Retransmit`,
printed as `rtx_n=` in `[SUCC]` and `rtx=` in `[LATE]`) and is excluded. The
split needs no wire change and is a lower bound on copies: a copy stamped
before the exposer reads as an original.
It prints ℓ*, `knee_bind` (the fraction of decisions where (H − d)⁺ bound)
and `sampler_bind`, with H observed directly as the onset of the arrival
stall during a frontier freeze. The request arm (`RWM_RECV_REQUEST_LAW`,
off) moves the request decision to the receiver on the reliable window seat
using a v8 `RepairRequest` message; `RWM_RANK_FEEDBACK` (off) widens the
request to a span of `m = clamp(⌈ln 2/(−ln π̂₀)⌉, 1, A*)` symbols, the span
at which the cost of a false repair halves.

**Measured (the receiver-law battery, `0159290`).** Refuted: the threshold
was pinned at zero by the store-headroom cap (knee-bound), so the request
law reduced to the store-cap law; and the self-heal estimator conflated
self-healing with retransmit closures. The conflation is fixed in the engine
(the classifier above); the battery predates the fix and was not re-run.
Section 9.8 gives the numbers.

### 7.7 What remains open

The correct recovery law is open. It needs: α, which Section 7.6 derives
from the contract's own cost ratio but which is measured knee-bound; the
self-heal distribution F resolved inside the admissible domain at the dual
cells; a measurement of H on the shipped stack, because H is the width of the
whole domain and the store multiplier that sets it (gain = 2.0 at one path,
1 + q(δ) at several) interacts with SACK-clocked release in a way no
derivation yet covers; and a c8L audit. Until then the shipped constants
(Section 11.2) stand as undefeated, which is not the same as derived.

---

## 8. Congestion Control and Substrate

### 8.1 Why delay-based

Loss-based controllers (NewReno, Cubic) halve on every random channel loss
and collapse on exactly the links this transport targets. A delay-based
controller distinguishes congestion (rising RTT) from channel loss (stable
RTT) and leaves channel loss to FEC and ARQ. The engine's own controller is
loss-blind: `on_loss` touches cwnd only on a decode failure, never on a
FEC-recovered loss (verified in cwnd traces at a 2.5 % GE cell: cwnd grows,
never collapses).

### 8.2 Copa and the δ dial

Copa [Copa2018] maximises `U = log(throughput) − δ·log(delay)`; δ is the
marginal latency price and the equilibrium standing queue is 1/δ packets.
The engine maps the hint to δ with no new constant, `δ(hint) = δ_auto/ζ`,
so Realtime (δ = 50) tolerates an essentially empty queue (jitter headroom
governs), Auto the classic two-packet target, and Bulk 200 packets.

**Two Copa laws exist, and the default runs the older one.** The engine's
Copa state (`scheduler/copa.rs`, `CopaState`) runs on every path. With the
wire signal off, which is the default (it needs `RWM_QUIC_CC=passthrough`,
`RWM_COPA_FEED` or `RWM_COPA_WIRE`), it uses the legacy application-echo law:
δ is the constant 0.5 and the backoff fires when
`d_q > (m_q − 1)·floor + 2·jitter`, with the queue multiplier m_q a three-arm
hint table (1.08 / 1.125 / 1.25, a declared corner, Section 11.2). Its cwnd
feeds the placement cost and the anchors; it gates neither intake (Section 6)
nor the substrate, which runs BBR. The δ-priced law engages only with the
wire signal on.

**The legacy law's queue signal.** On a jittery path a windowed-min RTT
compared against a 10 s propagation floor reads a permanent queue even when
the queue is empty: at c2 (10 ms floor, ±3 ms jitter) the floor found 7.0 ms
while a typical window min sat at 12–13 ms, and the sender backed off on
about 60 % of updates with cwnd near its floor. The legacy law therefore
compares like with like:

```text
   queue_floor  =  P10 of the per-update window-min history (10 s)
   backoff  ⇔  d_q  >  (m_q − 1)·queue_floor  +  2·max(jitter_est, win_jitter_est)
```

jitter_est is an EWMA (gain 1/8) of consecutive-sample differences and
win_jitter_est the same statistic over per-update window minima (gain 1/4),
which sees correlated jitter the raw differences miss. A standing queue
shifts every window min within one SRTT while the quantile lags, so congestion
is still detected; the ramp's fast exit needs at least three samples; the
cwnd floor is 8 symbols; d_q is clamped at 0.1 ms. On a clean link the
quantile equals the floor, the jitter terms vanish and the original law is
recovered exactly. For Realtime the cost is fundamental: a queue smaller than
the jitter spread cannot be distinguished from jitter at windowed-min sample
counts.

The wire-clocked Copa implements:

1. **The wire clock.** d_q is measured on quinn's packet-timed RTT, below the
   datagram queue, so the sender's own reservoir dwell is excluded. The
   application echo RTT stays with the reliability machinery.
2. **The update law.** Per SRTT, direction = (cwnd/SRTT ≤ 1/(δ·d_q)), step
   v/δ with velocity doubling after three same-direction updates. The
   previous additive +2 probe is this law's up-step at δ = 0.5, v = 1.
3. **Two supports.** A coupling cap `cwnd ≤ BDP + 2/δ` (above it the delay
   signal decouples and the jitter-clamped d_q ratchets cwnd upward) and
   CC-rate source pacing.
4. **Floor freshness without ProbeRTT.** The ±v/δ dither drains the queue
   regularly, keeping the 10 s RTT floor at the base without a forced drain.

A TCP-competitive mode (Copa §2.2: AIMD on 1/δ when the queue has not been
nearly empty for 5 RTT, with the hint's δ as the base) is implemented and off
by default (`RWM_COPA_COMPETE`). It detects and adapts as specified, but at a
clean shared bottleneck δ is not the binder: the plain retention pipeline
under contention tail-drop is.

### 8.3 BtlBw-anchored recovery

Additive recovery after a backoff takes dozens of SRTTs from half the pipe.
The anchor promotes the product of the windowed-max delivery rate and the
windowed-min RTT to an operating point:

```text
   target  =  1.0 · BtlBw · RTprop
   cwnd   +=  max( 2 ,  0.25·(target − cwnd) )      when cwnd < target
   cwnd   ≥  0.85 · BtlBw · RTprop                  floor, never a cap
```

The anchor raises cwnd only, because the delivery-rate sampler has no
app-limited flag and reads low during warm-up. It engages after 8 samples.
The 0.85 floor gain was set by one L1 measurement (at 1.0 the floor
maintained a 16 ms standing queue); the 0.25 pull and 1.0 gain have no test.
The sampler rejects calls closer than 1 ms apart (93–96 % at loopback,
81–94 % on the wire), which is the mechanism of the ×4.6–7.4 anchor
over-read the plain send-interval sampler (`RWM_PLAIN_RS`, off) removes.

### 8.4 The substrate is a controller

QUIC datagrams bypass quinn's reliability but not its congestion window:
quinn gates every packet, datagrams included. The effective controller was
therefore min(engine CC, quinn Cubic), a loss-reactive controller under a
loss-tolerant transport, and it was the "15–17 Mbit/s link ceiling" at 2.5 %
GE loss (the same stack over BBR: 74.5 Mbit/s). The substrate controller is
now an explicit policy surface, `RWM_QUIC_CC`:

| setting | behaviour |
|---|---|
| `bbr` (default) | quinn's BBR under the engine: the bulk-throughput choice |
| `passthrough` | quinn's window reads a per-path value the engine writes, so Copa owns the rate; losses are recorded, never acted on |
| `bbr_rs` | an in-tree BBR estimator (gated) |
| `cubic`, `newreno` | legacy loss-reactive arms |

**Measured tradeoff.** On the consolidated substrate, Copa-sole
(passthrough) reaches 0.89× BBR-under bulk at sc2, 0.97× at sc3, 0.73× at c7,
0.57× at c8 and 0.66× at dc1, while holding the network standing queue
×18 / ×16 / ×6–7 tighter at sc2 / sc3 / c7 (5 / 30 / 7 ms against 89 / 487 /
50 ms) and tying BBR on the realtime message tail. The earlier finding that
Copa-sole dominated at the heterogeneous dual was an artefact of the broken
substrate: fixing the walls lifted BBR's aggregation while Copa's
δ-equilibrium caps cwnd near BDP + 1/δ and leaves the freed pipe unused.
BBR is the default. Selecting the controller by hint would be a mode switch;
the endstate is one controller (δ-priced probing over a BBR-style rate model),
which inherits this bulk gap as its target.

**Adversarial edges.** At an 8-packet bottleneck buffer the ordering
inverts: BBR-under collapses to 0.12× its clean class (burst-quantised
delivery poisons the max-filter anchor) while Copa-sole holds, ×7.7–7.9 over
BBR. At a 100 Mbit policer both starve at about 8 Mbit/s; the burst-loss
recovery pipeline binds. "Bulk → BBR" is clean deep-buffer advice, not a
universal.

**Fairness.** BBR-under takes a 0.94–1.0 share against one Cubic flow at the
lossy c2 cell (Cubic is Mathis-bound there) and 0.02–0.24 on a clean
bufferbloated bottleneck.

### 8.5 The substrate's other control loops

A transport composed over another transport must enumerate the substrate's
autonomous loops (CC, PMTU discovery, pacing, idle timers) and pin every one
whose failure silently violates an overlay assumption. The measured case:
every wire symbol is a ~1279-byte datagram, sendable only after PMTU
discovery raises quinn's MTU above its 1200-byte floor. A GE loss burst of
all-large packets is indistinguishable, to quinn's black-hole heuristic,
from an MTU black hole; quinn reset the MTU to 1200 and paused discovery for
60 s, during which every symbol send failed sender-side as `TooLarge` while
small control datagrams kept the connection looking healthy. Declaring the
requirement, `min_mtu = initial_mtu = 1350`, fixes it (a deterministic
repro went from 63.5 s to 5.8 s; `tests/mtu_blackhole_wedge.rs`).

### 8.6 The substrate chain

The transport's measured history is a chain of walls, each named with a
mechanism before it was fixed or refuted:

| # | wall | mechanism | status |
|---|---|---|---|
| 1 | quinn's hidden Cubic | every send gated on quinn's window | fixed: `RWM_QUIC_CC`, BBR default |
| 2 | PMTU black-hole detector | 60 s MTU reset below the symbol size | fixed: min MTU 1350 |
| 3 | coded-only wire | every degree of freedom arriving dense at both ends, O(G²·S) | fixed: systematic-repair wire, O(k·G·S + k³); ×2.1 |
| 4 | decoder waste | known columns entering the matrix | fixed: sparse-aware elimination, ×1.2–5.0 |
| 5 | crypto | — | refuted as a wall: AES-NI cut CPU/byte 30–38 % and moved no throughput cell |
| 6 | receiver threading | — | refuted below ~150 Mbit/s per sink: one core sinks 187.7 Mbit/s |
| 7 | per-transfer flow control | a 1024-symbol pool, a Little's-law ~100–128 Mbit/s wall | fixed: path-scaled pool, then the derived cap (Section 6) |
| 8 | multipath recovery over-emission | global clocks and loss serials under striping | fixed: per-path loss detection (Section 7.1) |

What remains after the chain is structural. A saturated single reliable
path has no spare bandwidth to carry a repair that buys back a round trip,
so FEC equals ARQ on single-path bulk (Section 9.3).

### 8.7 Application back-pressure and ECN

When the store cap is reached the sender stops reading its source (the TUN
device or the application socket); the kernel buffer fills and the
application's `write()` blocks, exactly as a full TCP send buffer does. When
r* is so high that `source_rate = total_rate/(1 + r*)` falls below what an
application needs (a voice codec's minimum bitrate, say), the right response
is the application's: lower its rate, accept lower reliability, or wait. An
application may also read per-path ε, RTT, throughput and correction rate,
and adjust its (δ, ρ) contract mid-connection.

Where a path supports ECN [RFC3168], a congestion mark is a direct signal that
separates congestion from channel loss without inferring it from RTT. QUIC
validates ECN at connection start; the delay-based signal remains the
fallback, and on the wireless paths this transport targets it is the common
case.


---

## 9. Evaluation

### 9.1 Method

**Cells.** All measurements run the engine's own `perf` driver over Linux
netem on a dedicated VM. Delay, jitter and rate apply to both egresses;
Gilbert-Elliott loss to the data egress only. Dual and quad cells have
independent per-leg netem seeds (`e1fb2f6`; earlier dual measurements shared
one seed, so at a symmetric cell both legs' loss was the same realisation).

| cell | paths | per path: bandwidth / RTT (jitter) / loss | object |
|---|---|---|---|
| c1 | 1 | 1 Gbit/s / 2 ms (0) / GE 0.05 %, 50 % → ε ≈ 0.10 % | 400 MB |
| c2, sc2 | 1 | 100 Mbit/s / 10 ms (±3 ms) / GE 1.3 %, 50 % → ε ≈ 2.53 % | 100 MB |
| c3, sc3 | 1 | 20 Mbit/s / 40 ms (±5 ms) / GE 2 %, 40 % → ε ≈ 4.76 % | 25 MB |
| c3hg | 1 | c3 with ε = 5.80 % | 1.8 MB, 25 MB |
| c7 | 2 | c2 + c2 | 200 MB |
| c8 | 2 | c2 + c3 (heterogeneous) | 25 MB |
| c8L | 2 | c2 + c3 | 200 MB |
| dual-c1 | 2 | c1 + c1 | — |
| c9h | 4 | 2 × c2 + 2 × c3 | 50–150 MB |
| shal8 | 1 | 100 Mbit/s, 8-packet queue, GE 1.3 %, 50 % | — |
| pol100 | 1 | 100 Mbit/s policer, 16 KB burst, 10 ms, no loss | — |
| jit0–25 | 1 | 100 Mbit/s / 40 ms, jitter 0–25 ms / GE 1.3 %, 50 % | — |

**Discipline.** Every battery is pre-registered in its own commit before the
drivers exist: mechanism, predicted effect and cells, falsification
condition. All arms of a battery run from one binary (sha256 recorded),
interleaved round-robin per rep, on both seeds; a result is stated against
the pre-registered criteria, and a battery never flips its own default (a
flip is a separate commit). Mechanism liveness is proved before any number
is read: every gate echoes its effective value at both endpoints and every
law carries a gauge showing it executed. Every clamp reports a bind
fraction. Two batteries in this record fired their own stop rules and were
recorded unscored rather than re-cut. Recovery-plane arms are scored on one
number, Copa's utility difference at the contract's δ (Section 7.4).

**Instruments.** Every gauge is off by default and changes no behaviour.
The ones the results below read: `[DIAG]` (per-path anchors, loss, σ; `cod=`
counts coded repair symbols actually sent, while source copies (gap
retransmits, request copies, taper copies) are counted apart as `retx=` and
`total_copy_symbols`; batteries before this split read copies inside `cod=`), `[SF]` (store-cap refresh and saturation fractions),
`[DCAP]` (the δ-cap's dial point, request and pin fraction), `[RACK]` (the
shipped clamp's false alarms against RFC 8985's 1/16 budget), `[QCLK]` (the
realised recovery clock as a distribution), `[SUCC]` and `[HOLD]` (hole outcomes and closure classes), `[FCAUSE]`
(what triggered each repair), `[RFA]` (realised false-repair fraction from
receiver ground truth), `[LAT]` (delivered-latency decomposition), `[ETA]`
(the scheduler's arrival predictions against realised arrivals), `[LATE]`
(the receiver request law's hypothetical threshold and bind gauges) and
`[WIDLE]` (the arrival-stall onset that measures H). The rival clocks'
bind-fraction readouts left `[RACK]` and `[QCLK]` with their arms (Section 10).

**The default stack.** Unified span machine with shedding and the taper
budget; δ from the hint; plain window (no generation coding); quinn BBR
under the engine; path-scaled pooled store with SACK-clocked release and
the δ-cap; per-path loss detection; the anchor-hygiene pair; merged acks;
compact framing; placement at T = 0.15. It is pinned by
`gates/tests.rs::default_env_resolves_the_shipped_stack` and byte-pinned by
`gates_echo_default_is_byte_pinned`. As stated in Section 1.5,
Bulk and Auto need `--window-reliable` to run this machine; without it they
run the legacy block pipeline.

### 9.2 Against QUIC, TCP and MPTCP

Measured at `0f27f99` (both netem seeds, same cells, same day, same VM;
competitors quinn-perf with Cubic and BBR, kernel TCP Cubic and BBR on
application-acknowledged delivery, kernel MPTCP v1). Bulk rows are updated
by the framing flip (`078b0ce`) and the ack-merge flip (`bdab7de`).

| cell × workload | raptorpath | best competitor | verdict |
|---|---|---|---|
| c2 realtime (50 msg/s, 400/1200 B) | p99 median 36–39 ms, 1000/1000 delivered | QUIC 55–342 ms; TCP 209–1407 ms, delivery down to 687/1000 | win ×1.4–8.8 vs QUIC, ×5–38 vs TCP |
| c3 realtime | p99 median 92–103 ms, 1000/1000 | QUIC 150–759 ms (worst reps 38–44 s); TCP 830–3878 ms, delivered to 525/1000 | win ×1.5–41; only delivery-complete stack |
| c1 bulk (clean 1 Gbit/s) | ~229 Mbit/s | quinn-BBR 915, kernel TCP ~900 | loss ×4 |
| c2 bulk | 87.8–88.1 | quinn-BBR 91.9–92.4 | loss −4…−5 % |
| c3 bulk | 16.6 | quinn-BBR 18.6, TCP-BBR 17.5–19.4 | loss −8…−11 % |
| c7 bulk (symmetric dual) | 147–151 | MPTCP-BBR 149 / 169 | tie / −13 % |
| c8 bulk (heterogeneous dual) | 67–74 | MPTCP-BBR 90–93; single-path TCP-BBR 89.5–92.1 | loss −21…−27 % |
| Cubic-family stacks at c2 / c3 | as above | quinn-Cubic 24–26 / 3.2–4.8; kernel Cubic 11 / 1.4–2.2; MPTCP-Cubic 23–38 / 11–17 | win ×3–11 |

**Realtime is a class, not a multiplier.** The worst raptorpath rep across
32 realtime cells is 164 ms p99 with every message delivered; kernel TCP
runs 0.2–3.9 s medians with delivery cliffs, and QUIC, which delivers
everything, carries 38–44 s worst-rep tails at c3 (the head-of-line cascade
of a reliable ordered stream under GE bursts). The tunnel's p50 cost is
about 2.5 ms. A re-measure of the realtime cells on a current binary
(`f671f61`) reproduced 7 of 8 cell-size-seed rows inside the committed
spreads (c2 36–40 ms, c3 100–113 ms).

**Bulk.** The machine is loss-robust far above every Cubic-family stack but
is not a faster bulk pipe than BBR-class single-path stacks or kernel MPTCP
over BBR. The clean-path gap is the engine's per-message service wall (a
measured opt-in, sender batching plus estimator cadence, reaches 446–505
Mbit/s at c1). The lossy-single gap was accounted to closure at `db40d2f`:
framing and MTU tax (~4.3 / 0.95 Mbit/s at c2 / c3) and reactive over-fire
(~2.7 / 1.7), with the wire ≥ 98 % utilised; compact framing then recovered
part of it. At the heterogeneous dual, slow-path source is negative-margin
under every placement law measured: goodput falls monotonically as the slow
path's source share rises.

**Validity window.** The bulk rows describe the binaries of the competitive
baseline. On the current binary line the 100 Mbit/s single path sc2 does not
complete 100 MB in 300 s (about 2 Mbit/s, 6/6 and 8/8 reps), and sc3 and c7
read far below these rows in recent batteries (sc3 2.4–3.5 Mbit/s, c7 control
54–87 Mbit/s against 157–168 earlier). This is an undiagnosed regression and
the first item of Section 11.1's owed work; no bulk number in this section is
claimed for the current binary.

### 9.3 Multipath aggregation

| result | numbers | commit |
|---|---|---|
| per-transfer flow control was the multipath binder, not CPU | AES-NI cut CPU/byte 30–38 % and moved no cell; one receiver core sinks 187.7 Mbit/s; the path-scaled pool took c7 to ×1.72–1.89 of a single path | `a7ad963` |
| SACK-clocked release with per-path loss detection | c7 1.018–1.045×Σ of same-session singles | `a52105d` |
| composed default stack | c7 0.982–0.988×Σ; dual-c1 +15 Mbit/s above its single with retransmits ×10 down | `5ebbcda` |
| heterogeneous dual | 0.72–0.76×Σ (the legacy 1024 pool 0.85–0.87×Σ) | `5ebbcda` |
| single-path FEC = ARQ | generation-coded single path 0.97–1.0× plain+BBR; FEC costs ~0.37 s receiver CPU per 25 MB | `a7ad963` |

On a saturated single reliable path there is no spare bandwidth for a repair
that buys back a round trip, so FEC and ARQ reach parity; the coding is free
in CPU, not free in throughput. Where coding helps multipath is variance:
coded arms stay unimodal where plain arms go bimodal on a lossy leg.

### 9.4 The derived laws

| law | result | commit |
|---|---|---|
| honest anchor (O(1) max filter) | value-identical statistic, goodput within 2σ at every cell and seed, CPU/byte 0.90–1.03× | `67326d8`, flip `9f6e56b` |
| ×N deletion | interior at c7/c8 for the first time; under-funds c8's span by 45.4 % and goodput moves +9.29 / +1.34 (2σ 27.07 / 14.55); bit-identical at N = 1 | `27e36e3`, flip `6a65380` |
| δ-cap | goodput parity at c7/c8/c8L on both seeds; q_p50 −16/−10 (c7), −113.5/−117 (c8), −200/−130 ms (c8L); c8 dead wall shortened in 18 of 23 pairs (p ≈ 0.011); the shipped clock's false-alarm rate falls 24 % at c8 and 50 % at c8L with no clock change | `18dbf10`, flip `e9c6b24` |
| δ-cap re-measure (n = 12, zero aborts) | q_p50 direction reproduced (Δ −115…−218 ms at c8/c8L) but inside 2σ (129–377 ms): a point estimate, not a resolved effect | `e57f365` |
| cumulative effect of one era's flips | no two-sided goodput loss at any cell or seed; c1 seed 7 +9.6 %; c8 CPU/Gbit −31 % send, −9 % receive | `f605f92` |
| δ-honest shedding, unified default | zero collapse reps in 96 tail reps; p99 at or below the streaming code everywhere; 100 % delivery at the c3 message cell (streaming 79–81 %) | Section 5.6 |

The two derived flow-control flips are parity results: the pool's former gain
was funding delay, not goodput. They make the machine cheaper, not faster.

### 9.5 Wire and engine flips

| change | effect | commit |
|---|---|---|
| merged `WindowAck` (one control datagram per data message) | c1 +12.7 / +13.0 %, receiver CPU per bit −9.1 / −8.4 %, control-datagram density 1.96 → 1.00 at c1; the stream carries one symbol per ack (p50 = p90 = 1 over 857 400 acks) | `bdab7de` |
| compact data framing (`RWM_WIRE_COMPACT`) | sc2 +2.6 / +3.6 Mbit/s, sc3 +0.55 / +0.60 | `078b0ce` |
| PMTU floor 1350 | a deterministic 63.5 s wedge became 5.8 s; zero collapse runs in 68 afterwards | Section 8.5 |
| per-path loss detection | symmetric-dual retransmit share 14.9 % → 4.5 %, dual-c1 flood 8.5 % → 0.7 % of source, dual-c1 above its single | Section 7.1 |
| anchor hygiene (A* and M* anchors, clock-gap quarantine) | realtime stream p90 94 → 78 ms; generation depth knee +25–31 % at 100 ms RTT and +62–82 % at 200 ms | Section 5.3 |

### 9.6 Adversarial cells

Measured at `754139c`, before the two flow-control flips:

| cell | BBR-under (default) | Copa-sole | reading |
|---|---|---|---|
| shal8 (8-packet buffer) | 0.12× its clean class | full class, ×7.7–7.9 over BBR | burst-quantised delivery poisons BBR's max-filter anchor about 10× |
| pol100 (policer) | ~8 Mbit/s | ~8 Mbit/s | the burst-loss recovery pipeline binds, not the controller |
| jit0 → jit25 | — | 0.29–0.38× BBR, monotone in jitter | the store-dwell ceiling at 40 ms RTprop dominates, not the delay law |
| realtime under jitter | p99 92–96 ms (36–39 ms clean) | — | the tail class survives, with wire-class inflation only |

### 9.7 The recovery plane

| result | numbers | commit |
|---|---|---|
| the refresh clamp is a constant | binds 92.4 % at c1 (floor), 95.9 / 96.6 % at c7 / c8, 99.7 / 99.5 % at c8L / sc2 (ceiling); false alarms 1.7–12× RACK's 1/16 budget | `18dbf10` |
| every timing lever is inert on false alarms | hold-down suppressed 97.3–99.6 % of repair decisions and moved the realised false-alarm rate at most 1.66×; the refresh-floor lift found nothing below the floor | `5509e37`, `a8bcce0` |
| the clamp survives on merit | zero of three stable cells won by any successor | `f2e7144` |
| the self-heal prior | π₀ = 0.0077 (c1), 0.0054 (sc2), 0.9606 (c7), 0.9233 (c8); an earlier "97.9 % of holes closed by their original" averaged the two mechanisms | `ca58e49` |
| RTT dispersion at c8 | σ = 0.191 / 3.140 / 54.8 ms across three reps (287×) on the plain-window machine; the δ agreement it was meant to test is unresolved | `f6c8cbf` |

### 9.8 The three most recent batteries

**r > 0 (`91895dd`; n = 4, one seed, truncated at the 5 h cap).**

* **Control at the corner, confirmed.** The control's `cod=` symbols (as then
  metered, source copies included) are 86–90 % retransmits at the single-path cells and 66–70 % at c8; non-NACK
  repair is 0.4–1.9 % of the wire.
* **Funding coding made the lossy single path slower.** The completion-glide
  arm at c3hg (1.8 MB) funded r at 6.5–8.2 % of the wire: completion +55 %
  (Hodges-Lehmann +3.51 s, 95 % [+2.63, +4.44]; all four reps above the
  control's maximum), goodput −36 % / −28 %. Direction only at this n.
* **Decode-resolved holes were slower than retransmit-resolved ones.** Holes
  closed by decode took 157–677 ms against 15–105 ms for retransmit-closed
  holes, 4–6×, in 16 of 16 readable single-path reps; 28–38 against 10–16 ms
  at c8. The mid-stream arm was void at 5 of 6 cell sizes because the engine
  does not echo Copa's δ.

**Receiver request law (`0159290`).** Refuted with record at both duals on
every arm. The false-repair fraction rose ×5.74 at c7 (0.038 → 0.217) and
×5.76 at c8 (0.101 → 0.582) under arm A; the composed arm did not finish
12 of 12 reps at c7/c8; delivered p99 never fell below the control's minimum.
The threshold was pinned at zero: `knee_bind` = 1.0000 and ℓ* = 0 on 48 of
48 dual rows, because the observed headroom H (4.6–5.1 ms at c7) was below the
resolution delay d (17–23 ms). The receiver's self-heal estimate read π̂₀ =
0.93–1.00, against the audited 0.0077 at c1, because it counted the sender's
retransmit copy as a self-heal. The engine now excludes retransmit-resolved
holes from π̂₀ (Section 7.6); the battery has not been re-run on the fix.

**Placement (`5e6e3f5`; n = 4 × 2 seeds, c9h n = 3).** The delivered-latency
decomposition at the duals:

| term | c7 | c8 |
|---|---|---|
| cross-path wait (placement-manufactured) | 5.9 % | 6.5 % |
| queueing above the path floor | 0.41 | 0.33 |
| same-path reordering | 0.21 | 0.32 |
| repair | 0.32 | 0.28 |

(c1 is queue-dominated at 0.88 with no cross-path term.) Every placement arm
was inert on latency as derived; the zero-temperature (argmin) arm cut c7
goodput to 0.35–0.38× of the control, and the derived-temperature arms cost
0.78–0.85× goodput at c1 with about 30 % more CPU. The dispersion of the ETA
error relative to the reference, whose 0.19 the shipped T = 0.15 asserts, read 0.21 (c1), 0.28 (c7), 0.53 (c8) and 0.76 (c9h): the constant is
not a stable quantile.

### 9.9 Where a win would come from

The latency budget at the duals is dominated by queueing above the path
floor, not by placement; the recovery clocks are inert on false alarms
because fires are gap-driven; proactive FEC at r > 0 slowed the lossy single
path; and the two flow-control flips removed queue at goodput parity. The
remaining bulk gaps are the engine's per-message service wall (clean path),
the reactive plane's over-fire and framing tax (lossy singles), slow-path
conversion at the heterogeneous dual, and the unexplained regression of
Section 9.2. Owed instruments: the sender ETA stream needs an exit flush and a
consistent time reference; the engine must echo Copa's δ; and the diagnostic
writer needs line discipline. The receiver's self-heal classifier no longer
counts retransmit copies, and `cod=` no longer counts source copies.

### 9.10 What is verified

A mechanism described in this paper is not thereby pinned. The reading rule
is strict: a law is **verified** when an always-on test asserts its absolute
value, or an exact equivalence, and the wiring that reaches it is pinned;
ordinal tests ("more than", "decreases") do not catch routing bugs and do not
count.

| stage | status | evidence |
|---|---|---|
| r* kernel | verified | `formula_verification.rs::test_r_star_worked_examples`, `test_p_fec_exact_paper_table` (Section 4.2 table to ±0.002–0.005) |
| composed rate `controller_rate` | partial | continuity sweeps, ordinal tests, one band against the exact DP |
| rate mix r(β) | verified | `the_rate_mix_is_byte_identical_at_the_presets`, `the_rate_does_not_step_across_the_bulk_preset_or_anywhere_on_the_dial`; the visualizer's `test_continuum_one_law_across_the_dial` |
| b(δ), w_bw(δ) at the presets | verified | `delta_budget_b_is_the_dial_not_a_mode`, `scheduling_weights_are_the_dial_not_a_mode` (bit-exact) |
| unified decoder | verified | differential tests against both legacy decoders and a dense reference oracle |
| span law (A*, M*, Δ) | unverified | no isolating pure-law test; continuity at the named points is not asserted (Section 11.1) |
| pooled cap and δ-cap | verified | `formula_agreement.rs::the_delta_cap_substitutes_one_factor_and_reduces_to_candidate_d`, `published_codel_setpoint_equals_the_engine_map_and_spans_the_derived_band`, law-shape tests over N = 1…8 |
| store-cap admission gate | unverified | the `store_len ≥ cap` predicate has no isolating test |
| pacing divergence | bounded | `pacer_debit_bounds_only_the_source_arm_not_the_wire` |
| placement costs | pinned | `place_costs_match_the_pinned_table`; `place_symbol`'s sampling loop is not |
| live versus active path sets | verified | `saturated_path_is_live_but_not_active`; `recovery_clocks_keep_cwnd_saturated_paths` pins the recovery clocks to the live set |
| self-heal versus copy closure | verified | `a_retransmit_resolved_hole_is_not_counted_as_self_heal` |
| correction metering (`cod=` versus copies) | verified | `corrections_are_metered_by_kind_and_only_when_sent` |
| BtlBw max filter | verified | `bw_mono_front_equals_full_window_fold` |
| SRTT (Copa EWMA) | unverified | load-bearing for pacing, the recovery plane and placement; modelled by test oracles, never pinned |
| three channel implementations | unverified | no cross-validation between the test GE, the recovery-bench chain and the netem shim |
| default stack | pinned | `gates/tests.rs::default_env_resolves_the_shipped_stack`, `gates_echo_default_is_byte_pinned`; `default_config_routes_bulk_and_auto_to_the_block_pipeline` pins the block default |


---

## 10. Refuted and Superseded Designs

Each row is a mechanism that was built (or derived) and measured, with the
reason it did not become part of the machine and the commit that measured,
removed or replaced it. **Removed** means the code is deleted; **off** means
the gate remains as a measured A/B arm, default off; **superseded** means a
successor replaced it; **legacy default** means deprecated but still
shipped.

| mechanism | why it failed, or what replaced it | commit | status |
|---|---|---|---|
| **Coding and decoding** | | | |
| METTLE FEC backend | a sparse streaming code needing ~15 % decode overhead; reachable only by explicit opt-in, with the loss-threshold backend selector's high-loss tier routed to RLC | `10ac80f` | removed |
| Mid-stream backend switching (loss-threshold selector) | no cross-code algebra: a switch strands every in-flight symbol or forces a drain; a threshold on ε̂ oscillates | `6948a64` | removed |
| Streaming two-layer code (Realtime machine) | displaced, not refuted: the unified span machine was at or below its p99 at all five historic crown cells on both seeds, 163/163 delivery-complete | `bccb32a` | removed |
| Legacy RLC decoders (sliding-window, generation) | replaced by the unified global-RREF decoder; the keyed generation machine strands 2-loss bursts on a moving-span wire | `b849acb` | superseded (`RWM_UNIFIED=0` opt-out) |
| Block-FEC pipeline (RaptorQ/RS/block-RLC with batch-ACK ARQ) | not measured as the arm under test since an early DNF 6/6 at a lossy cell; frozen; removal list in ADR-0069 awaits the block-versus-window re-test | `17f7fa9` | legacy default for Bulk/Auto |
| Coded-only generation wire | the O(G²·S) "decode ceiling" was the wire mode, not the solver; systematic-repair wire doubled single-path goodput (33.9 → 70.9 Mbit/s) | `2122481` | superseded |
| Coded-only fungible window (coded-object mode) | correct but slower on the real stack: 3.9 Mbit/s at the heterogeneous dual against 15.7 for the fast path alone; a striped combination lands after the window has moved | — | off |
| FMTCP-class decode-on-total [Cui2015] | 0.48× at the heterogeneous dual; on the clean substrate c7 ×0.11, c8 ×0.20 of the default stack, a recovery flood (coded share > 1) at about 8× plain CPU | `f841757` | removed |
| Leading-window taper emission | repairs coded over in-flight members are unsolvable on arrival: −22 pp delivered reliability; replaced by trailing-span emission | — | superseded |
| **Proactive FEC timing** | | | |
| Repair-wait / FEC-before-ARQ (`RWM_REPAIR_WAIT`) | the proactive fraction fell (0.27 → 0.22) as the wait grew; the covering repair was absent, not late (7 useful of 4 600 fed) | `3a0ca0c` | off |
| Inline / frontier-anchored repair (`RWM_INLINE_REPAIR`, `RWM_FRONTIER*`) | repair anchored at the frontier loses to its own ARQ (4 useful of 718) | `bede4a3` | removed |
| Present-at-stall proactive pacer (`RWM_PROACTIVE_PACER`) | presence rises, throughput does not: a saturated single path has no spare bandwidth for a repair that buys back a round trip | `806c0f4` | off |
| Funded proactive coding at r > 0 (the r > 0 battery) | at the lossy single path funding coding made the transfer slower, and decode-resolved holes were slower than retransmit-resolved ones; the control confirmed the r* = 0 corner | `91895dd` | refuted |
| **Scheduling and placement** | | | |
| DAPS chain, pace-all, per-path rate-sample estimator (`RWM_DAPS*`, `RWM_PACE_ALL`, `RWM_RATE_SAMPLE`, `RWM_PER_PATH_EST`) | their wins were measured in an era where the coded path was dead; the live ablation measured −17 to −30 % at the symmetric dual; the surviving ideas became the M* law and the anchor-hygiene laws | `9b48286` | removed |
| Sub-max quantile rate anchor (`RWM_RATE_WIRE`, `RWM_RATE_Q`) | decode-clocked samples make the windowed max the correct statistic; any sub-max quantile under-reads about 65× (heterogeneous dual 3–6× worse) | `f1f32c5` | removed |
| Place-slack (`RWM_PLACE_SLACK`) | at the heterogeneous dual goodput falls monotonically as slow-path source share rises (6 % → 88.6 Mbit/s, 16–18 % → 70–83); the symmetric-dual clause failed (0.86–0.90×Σ) | `cc15b83` | removed |
| Derived placement temperature, frontier term, derived diversity weight (`RWM_PLACE_T_DERIVED`, `RWM_PLACE_HOL`, `RWM_PLACE_WDIV_DERIVED`) | inert on delivered latency, with goodput regressions under the guard; placement-manufactured wait is only ~6 % of delivered latency | `5e6e3f5` | off |
| Copa per path as the substrate default (the original design) | on the clean substrate Copa-sole is 0.57–0.97× BBR bulk; its earlier heterogeneous-dual win was a broken-substrate artefact; BBR-under is the default and Copa the queue/tail policy point | `7652ccb`, `519467e` | superseded |
| Quinn's stock Cubic under the engine | the hidden loss-reactive substrate controller was the 15–17 Mbit/s "link ceiling" | `519467e` | superseded |
| Engine or receiver parallelisation | one core per side sustains 136–188 Mbit/s with an empty queue; the walls are per-symbol service time and recovery waste | `cee499c` | refuted, never built |
| **Flow control** | | | |
| Source backpressure (`RWM_SRC_BP`) | −53 % at the heterogeneous dual; source is the pipeline clock, not a holdable emitter | `8902d24` | removed |
| SACK pruning of the store (`RWM_SACK_PRUNE`) | pruning destroys the only retransmittable copy (in-order duals wedged); flat on a single path; replaced by SACK-clocked release | `3dcb39c` | removed |
| ×N pool multiplier `clamp(gain·N·Σ, floor, N·knee)` | quadratic in N under a linear ceiling, pinned at 4 096 in 121 of 126 dual reps; deleting the ×N cost nothing | `6a65380` | superseded |
| Pool gain 2.0 (N ≥ 2) | no derivation; replaced by 1 + q(δ): goodput parity 6/6, queue −10 to −200 ms | `e9c6b24` | superseded |
| Per-path store accounts (`RWM_STORE_PERCAP`, `RWM_PERCAP_GUARD`) | wins the symmetric dual (0.89–0.94×Σ) and loses the heterogeneous one (0.54–0.55 against pooled 0.62–0.69×Σ) | `4bb5b28` | removed |
| Bounded account borrowing (`RWM_STORE_BORROW`) | loans are identically zero at symmetric cells by construction; neutral at the heterogeneous dual and behind pooled | `7c3343f` | removed |
| Capacity-weighted pool (`RWM_STORE_CAPW`) | the heterogeneous binder is slow-path conversion, not pool size (0.74–0.79 against 0.87×Σ) | `4fb5b15` | removed |
| Store-cap unification over live paths (`RWM_STORE_CAP_UNIFIED`) | removes a boot-cap cliff at c1 (+16–25 %) and costs −19.6 % at the heterogeneous dual, where it carries a dead-wall mode | `865112e` | off |
| Three-term outstanding law (`RWM_THREE_TERM`) | the terms are right and the lever is wrong: the store is sized and occupied, throughput does not follow | `448a82e` | off |
| Composed cap (`RWM_COMPOSED_CAP`) | shape confirmed, magnitude refuted: pinned at `WIN_STORE_MAX` at every dual; where interior, 2.4× the queue and 1.43–1.48× worse delivered latency at goodput parity | `161b4ea` | off |
| Queue-free slack clock | removes 1.7 % of a 90 % overshoot at the heterogeneous dual, by its own arithmetic | `7e302e2` | refuted, never shipped |
| Window decoupling (`RWM_WIN_DECOUPLE`) | the queue died (echo RTT 108 → 27 ms) and goodput did not follow; re-fires are re-serve-clocked. Its sibling, compact wire framing, shipped (+2.6/+3.6 Mbit/s) | `48f60c4` | removed |
| Pool delivery-clocked anchor (`RWM_POOL_DELIV`) | worked exactly as specified and moved the symmetric dual the wrong way (0.931–0.958×Σ) | `8afd4dd` | removed |
| Floor-bound anchor (`RWM_FLOOR_BOUND`) | −14 % at c1; the ack-interval over-read is load-bearing at N = 1 | `8afd4dd` | removed |
| Sender-truth loss estimator (`RWM_LOSS_SENT_TRUTH`) | moves ε̂ 20× in the wrong direction, including at N = 1 | `27e36e3` | off |
| Composed estimator-cadence plus pool-anchor default | flipped, then reverted by its pre-set symmetric-dual clause (0.959–0.968×Σ); ships as an opt-in | `e84ef1c` | reverted |
| Emission batching as default (`RWM_EMIT_BATCH`) | +10–16 % at c1, below the pre-registered bar; receiver-side batching arms raised echo RTT 11 → 76 ms and were removed | `52b4fff`, `1313841` | opt-in |
| **Recovery clocks** | | | |
| Global loss serials under striping (`RWM_RECOV_MP_SERIAL`) | diagnosis correct (per-path loss read 0.62–0.77 at a 0.1 % cell), runtime refuted: honest small values re-heated every cadence, sender CPU ×2.4 | `ade48ad` | removed |
| Singles hole suppression (`RWM_RECOV_SP`) | +0.3 Mbit/s at sc3, a tie at sc2; re-fires are re-serve-clocked | `db40d2f` | off (kept; open) |
| Derived patience (`RWM_PATIENCE_DERIVED`) | the literal it replaces wins 0 of 177 543 evaluations at the cell it was accused at; identical to shipped across 192 bench cells | `65e92b3` | removed |
| Derived recovery sweep (`RWM_DERIVED_SWEEP`) | the argument is vindicated, the lever inert where it should act and −23 %/−28 % goodput when armed | `43b09fe` | off |
| RACK-shaped clocks (`RWM_RACK_CLOCKS`, `RWM_RACK_REO_MULT`) | fails RACK's own 6.25 % spurious budget at every arm and cell (0.21–0.78); its SRTT ceiling bound 0 times in 108 847 evaluations | `18dbf10` | removed |
| Cantelli recovery clock (`RWM_QUANTILE_CLOCKS`, `RWM_ALPHA_OVERRIDE`) | needs 316σ and ~10⁵ samples at the contract's α; pricing α from the tail-loss target is a category error | `c249c20` | removed |
| Dispersion estimators (fixed-sample-lag, fixed-time-lag) | the 287× spread is the estimator; the time-lag form confirms rate invariance and still misses its bar | `89d7946` | open |
| Quantile-native clock (`RWM_W_FORM`) | commanded false-alarm rate does not track α at 5 of 5 cells: the measurand is wrong | `6b3d3c3` | removed (the order statistic survives as the hold-down's window law) |
| Hold-down clock (`RWM_HOLDDOWN_Q`) | suppressed 97–99.6 % of fires and moved the realised false-alarm rate at most 1.66×: fires are gap-driven | `5509e37` | off |
| Refresh-floor lift (`RWM_REFRESH_FLOOR_US`) | entered the sub-floor region at 3/3 cells and found nothing; faster holding raises repair volume | `a8bcce0` | off |
| Receiver request law (`RWM_RECV_REQUEST_LAW`) | refuted at both duals on every arm: ℓ* = 0 on 48 of 48 dual rows, knee-bound by the store headroom; the receiver's π̂₀ was contaminated by retransmit copies (since fixed; not re-run); one arm DNF 12/12 | `0159290` | off |
| **Earlier verdicts** | | | |
| Generation-inert era | the harness never enabled generation coding, so the coded path was dead during a whole series of measurements; superseded by a re-baseline with a liveness guard | `161aff1` | superseded |
| "Arc concluded" aggregation verdict | measured under three hidden binders (substrate Cubic, the PMTU wedge, the 1024-symbol pool); replaced by the measured regime map | `acefc47` | superseded |
| The dead-wall and mode-hunt batteries | both fired their own stop rules; the dead-wall mode belongs to the store-cap-unification arm and vanishes at 8× the transfer size | `e6f68a7`, `2a3d82e` | unscored |

The recurring failure modes are three. A law can be exhaustively pinned and
still wrong: the ×N pool passed nine always-on pins for a month because a
clamp hid its shape, and N ∈ {1, 2} was the whole test universe. A transplant
can import a construction its source does not contain: RFC 8985 publishes no
RTT-relative ceiling for a re-probe cadence. And a number can be moved across
a unit boundary it does not cross: pricing a wasted-retransmit rate from the
never-delivered tail target.


---

## 11. Open Questions, Related Work, References

### 11.1 Open questions

| # | question | what would decide it |
|---|---|---|
| 0 | Why does the current binary line run sc2 at ~2 Mbit/s (and sc3, c7 far below their earlier readings)? | a bisect against the competitive-baseline binary (Section 9.2) |
| 1 | Should Bulk and Auto run the window machine by default, retiring the block pipeline? | the pre-registered block-versus-window re-test at c1/c2/c3/c7/c8, both hints, two seeds |
| 2 | Is ρ a runtime dial? Today ρ is structural (1 on the retain seat, < 1 only on the evicting Realtime seat) | plumbing ρ to the store and the receiver; the hard-coded `SRTT/2` receiver hold is the first site that would break |
| 3 | Does Auto's rate sit at the corner r* = 0? | an echo of Copa's delay normaliser d beside D_arq (Section 4.9) |
| 4 | Does the span law stay continuous at the named points? | an isolating pure-law test of `(δ, ρ, r) → (A*, M*, Δ)` with ±2 % nudges, as the rate law has |
| 5 | Is cross-path correlation a cause or an effect of pooling? At c8 both legs' drain collapses in the same window | a per-path-account arm (the refuted one is removed) against pooled at a four-path cell, now that legs are seeded independently; a correlated-loss dial for the harness |
| 6 | Do the three channel implementations (test GE, recovery-bench chain, L0 netem shim) agree? | a differential test at fixed seeds; none exists |
| 7 | What sets the recovery clock? | the self-heal quantile F_heal(7.22 ms) at c7; an `RWM_STORE_GAIN` contrast; a c8L attribution pass (Section 7.7) |
| 8 | Is the c8L pool cap interior? | the within-run Σ series (pin fraction 0.23 today) |
| 9 | Why does the probabilistic taper retransmit never fire? | whether ε̂ at send is structurally ~0 or the oldest symbol never ages past the P_lost knee |
| 10 | Is the per-path loss estimate 3–5× low? | a per-path truth feed against the netem channel (Section 2.6) |
| 11 | Can the machine beat BBR-class stacks on clean single-path bulk? | the engine's per-message service walls; the measured opt-in (sender batching plus estimator cadence) reaches 446–505 Mbit/s at c1 against 915 for quinn-BBR |
| 12 | Is a nested delay loop stable? | Copa's δ-priced loop sits inside the pool's δ-priced cap on the same delay; no stability analysis of that topology is published; time-scale separation holds by accident today |
| 13 | Remaining hint-keyed sites | `queue_target_mult` (1.08/1.125/1.25), `BlockProfile::from_hint`, default interleave depth (2/1/3, non-monotone in δ, so no continuous form exists), the Realtime duplicate source send, `use_packing`, `RWM_COPA_COMPETE`, `is_window_mode`, `effective_fec_backend`; each is a declared corner, not a law |

### 11.2 Open-constants register

Every constant below has no derivation. None has been changed, because its
correct value is not known; being undefeated is not being derived. Each
carries the derived form (where one exists) and the measurement that would
decide it.

| constant | site | status and derived form | deciding measurement |
|---|---|---|---|
| refresh clamp `(2·SRTT).clamp(25, 100) ms` | `hole_nack_refresh`, `tail_sweep_timeout_us` | binds 92.4–99.7 %; a cadence derivable from F at the duals, undefined at single paths (F is empty there) | the self-heal lateness distribution per cell |
| legacy age gate `SRTT/2` | `legacy_age_ripe` | runs only when per-path detection is off; its SRTT is the store-dwell-inclusive echo RTT (a max over live paths) while age runs from the original send | lateness against F |
| per-path thresholds 9/8 and 3 | `mp_time_threshold_split`, `MP_PACKET_THRESHOLD` | cited from RFC 9002 (9/8 is an empirical recommendation; RACK uses 5/4); measured against age including sender dwell | lateness coordinate in place of age |
| `GAP_ACK_MIN_INTERVAL = 2 ms` | `net/mod.rs` | a rate limit that is also the hole sampler; manufactures no holes on a lossless wire (`holeclass_reachability.rs`) | none yet; it sets the α ≈ 1 corner |
| `NACK_RETX_COOLDOWN_FLOOR_US = 10 ms` | `net/mod.rs` | 10× RFC 9002's granularity | re-fire cost against F |
| `RWM_STORE_GAIN = 2.0` | single-path cap; H = (gain − 1)·RTprop at N = 1 | sets the single-path store headroom, and was assumed at the duals by the recovery analysis | an `RWM_STORE_GAIN` contrast |
| `HONEST_RECOVERY_ROUND_S = 100 ms` | honest per-path cap (`net/store_cap.rs`) | inert by default (needs `RWM_PLAIN_RS`) | — |
| κ = 1 in the placement frontier term | `place_costs` X_i | declared upper bound; fitted κ is 0.0048–0.067 | an `s_i > H` bind fraction |
| `PLACE_TEMPERATURE = 0.15` | `scheduler/place.rs` | the argmax of a four-point sweep at one cell whose verdict was a failure; derived form `T = (√6/π)·σ̂_e/ref` | σ̂_e on the ETA stream |
| `w_div = 1.0` | `SchedulingWeights` | derived form `(p_BB − ε)⁺·srtt/ref` gives 0.475 (c2), 0.552 (c3) | per-path p_BB beside placement |
| cold-path price `r_i = 10.0` | `place_costs` | a hard exclusion (e^−66 odds at T = 0.15) in continuous form; derived form: price an unmeasured path at the worst measured one | a cold-price bind fraction |
| near-tie band 0.8 and floor 0.25 | `place_repair_spare_path` | a relative band plus an absolute floor signals a missing scale; derived form `max_spare − z·σ̂_spare` | σ̂_spare |
| `ELIGIBLE_SKEW = 75 ms` | `pick_affinity_path` (block seat only) | a threshold that selects a code path; moot on the window path | — |
| `BULK_TAIL_BUDGET = 0.05` | `raptorpath-math` | an "e.g." promoted to a constant | the completion-glide battery arm |
| χ constants 1.5, 4·RTTVAR, SRTT/4 | `completion_exposure` | — | — |
| W* fraction α = 0.25, bounds [16, 512], `MAX_WINDOW_SIZE = 200` | `derive_window`, sender | 200 is below W_mp at the heterogeneous cell | — |
| knee = 2048 per path | pooled cap ceiling | measured but stale; inert at c7/c8 under the δ-cap | — |
| `queue_target_mult` 1.08 / 1.125 / 1.25 | Copa legacy branch | a declared corner: not affine in log δ | a CoDel-derived per-δ setpoint |
| receiver hold `(4·SRTT).clamp(60, 300) ms` | `shed_recv_hold` fallback | three constants | a bind gauge (60 ms predicted to bind at c2, 300 ms at c3) |
| anchor floor 0.85, pull 0.25, gain 1.0 | Copa legacy branch | 0.85 set by one measurement; the others untested | — |
| `SRTT/2` heal classifier | attribution audit (offline) | biases π₀ upward; the engine's own classifier does not use it (sender-stamp order, Section 7.6) | — |

### 11.3 Code/model divergences

| item | model | engine |
|---|---|---|
| default pipeline for Bulk and Auto | the window machine | the block pipeline (RaptorQ) unless `--window-reliable`; selected by a `hint == Realtime` test (`is_window_mode`) |
| span deadline D and A* | `min(b·RTprop, 2·RTprop)` | uses the loss estimator's EWMA RTT (`est.rtt()`), not the min-filtered RTprop |
| receiver shed hold | `b(δ)·SRTT` | hard-codes `SRTT/2` (b at Realtime) because the evicting seat exists only for Realtime |
| Copa price | δ(hint) | constant 0.5 with a three-arm queue-multiplier table unless the wire signal is on (Section 8.2) |
| store cap at one path | δ-priced setpoint | `clamp(2.0·BDP, 10, 1024)`; the δ-cap engages only at N ≥ 2 |
| pool path set | live paths | `net::channel_paths` = `live_paths()` unconditionally (plan 2b; `RWM_STORE_CAP_UNIFIED` retired) |
| recovery-plane path set | live paths | the recovery clocks and the repair margin read `recovery_clock_paths` (live); the react-cap SRTT, the NACK-budget and `repair_rate` worst-loss picks, the taper's ε̂ at send, and the Shutdown broadcast still read `active_paths()` |
| store headroom H in the recovery analysis | `(gain − 1)·RTprop` at every cell, with the count released only by the frontier | at N ≥ 2 the multiplier is `1 + q(δ)` (H = q(δ)·RTprop_w), and SACK-clocked release uncounts SACKed symbols; H is read from `[WIDLE]` (Section 7.3) |
| r* to the generation encoder | r* sets the repair budget | the generation seat uses a constant repair floor (0.15 systematic, 0.20 coded); r* reaches the wire through the plain window's taper budget and the block pipeline's `⌈k·r⌉` |
| pacing | source and repair paced at the CC rate | the pacer debits source only and does not run on the plain reliable path (bounded by a test, Section 6.5) |
| P_lost inputs | SRTT and RTTVAR | RTTVAR fixed at 0.1·SRTT at the window call site; the worst path is picked from `active_paths()` |
| GE estimator input | per-symbol loss sequence | per-batch counts, losses fed before receives |
| BOCD quantile | mixture quantile | run-length-weighted average of quantiles |
| δ_exit (Section 4.9) | a price that locates the corner | not implemented |
| visualizer Bulk tail target | — | the wasm model interpolates to 0.05 at Bulk where the engine's t_tail is 10⁻³, and guards its rate mix with `if bulkness > 0` |

### 11.4 Related work

**Hybrid FEC-ARQ.** Mehrotra, Li and Huang [Mehrotra2010, Mehrotra2009]
solve lossless in-order streaming over a lossy channel with an MDP and find
that preempting data with FEC can reduce delay. This model derives a closed
form from the GE survival function instead of solving an MDP, carries burst
correlation in the margin, and addresses multipath and multiple classes.
Razavi et al. [Razavi2008] search joint FEC/ARQ parameters numerically under
GE.

**Streaming codes.** Delay-constrained burst erasure correction
[Martinian2004] and its layered constructions [Badr2017, Fong2019,
Krishnan2020] give rate-optimal codes for a sliding-window proxy channel;
Tambur [Rudow2023] shows the gains transfer to videoconferencing. Analytical
bounds for streaming codes over GE [Vajha2020, Vajha2020b] and for random
linear streaming codes [RLC_GE2025] could replace simulation in verifying
P_fec. A two-layer streaming code was the Realtime machine of an earlier
release; the unified RLC span machine replaced it (Section 5.6).

**Coded transport and queueing.** Fork-join analysis [Nelson1988], coded
download as an order statistic [Joshi2014, Joshi2017] and resequencing
delay [Xia2003] ground the three decode predicates of Section 5.5. Cloud,
Leith and Médard [Cloud2014] show coded packets shrink in-order delivery
delay without abandoning ordering, the antecedent of Section 5.5's claim that
in-order delivery is not the aggregation bottleneck. CloudBurst [Zeng2021]
uses proactive multipath FEC for datacenter tail latency. FMTCP [Cui2015]
is the principal published fountain-coded multipath TCP; its decode-on-total
construction measured refuted on this stack (Section 10).

**Multipath scheduling and buffers.** MPTCP's minRTT scheduler, BLEST
[Ferlin2016], ECF [Lim2017] and DAPS [Sarwar2013, Kuhn2014] address
head-of-line blocking under path heterogeneity; the MPTCP buffer lineage
[RFC6182, RFC8684, Raiciu2012] sizes receive buffers with RTT_max outside
the sum. The per-path difference form `Σ bwᵢ·(RTT_max − RTTᵢ)` used for the
resequencing span term of the three-term cap (Section 10) does not appear in
these sources and is this work's decomposition. Eppen's risk-pooling result [Eppen1979] predicts that a pooled
store beats per-path accounts by an amount that shrinks as demand
correlation rises; on this machine the ordering holds (drain correlation
−0.81 at c7, +0.61 at c8, p = 0.009), but the per-path demands are
endogenous to the pool, so the verdict is partial.

**Congestion control and queues.** BBR [Cardwell2016] and Copa [Copa2018]
anchor the substrate choice; CoDel [RFC8289] supplies the derived
standing-queue setpoint of Section 6.1; RACK-TLP [RFC8985] and QUIC loss
detection [RFC9002] supply the per-path thresholds of Section 7.1 and the
spurious-recovery budget against which the refresh clamp was measured.

**Control structure.** Copa's δ-priced delay loop runs inside the pool's
δ-priced cap, itself a delay budget: two delay-regulating loops nested on the
same path delay. The control literature supplies the constraint the
networking literature has not applied to this topology — time-scale
separation of a factor of five or more between cascaded loops
[Skogestad2005], closed-loop time constants bounded by R₀/2 in AQM-TCP loops
[Hollot2002] — and an outer adaptation loop driving the inner congestion loop
into a downward spiral has been measured in video streaming [Huang2012]. No
stability analysis of the nested topology was found; the separation holds on
this machine but has never been stated as a requirement, and the cap's clamps
are exactly the nonlinearity a linear cascade argument does not cover.

**Literature cross-check.** The engine's load-bearing formulas were checked
term by term against their published counterparts by direct text extraction
of primary sources: six of ten have exact counterparts; RFC 9002's 9/8 is an
empirical recommendation, not a derivation; the pool's former gain of 2.0
appears in no primary BBR source; and the span decomposition
`Σ bwᵢ·(RTT_max − RTTᵢ)` appears in no publication.

**Sequential detection.** Wald's sequential probability ratio test
[Wald1948], Lorden's minimax detection delay [Lorden1971] and the optimality
of CUSUM [Moustakides1986] characterise the recovery decision (Section 7.2).
No published application of sequential detection to transport loss recovery
was found; RACK's 1/16 spurious-recovery budget is a false-alarm rate chosen
by hand.

**Channel models.** Gilbert [Gilbert1960] and Elliott [Elliott1963]. The
water-filling principle [Gallager1968] reappears as the taper's allocation of
correction density in time and as the placement law's fixed point.

### 11.5 What this model contributes

| aspect | prior work | this model |
|---|---|---|
| FEC/ARQ balance | MDP or heuristic search | closed form from GE parameters and a tail target, corrected by the measured window-mass tail |
| protocol classes | separate modes and knobs | one δ dial; hints are named points |
| decoder | per-mode decoders | one global incremental RREF |
| multipath buffer | RTT_max · Σ bw | `(1 + q(δ))·Σ bwᵢ·RTpropᵢ` with a cited setpoint |
| recovery timing | hand-set RTT multiples | one sequential test; a derived domain and value bound for waiting |

### 11.6 References

**Channel models**

- [Gilbert1960] E. N. Gilbert, "Capacity of a burst-noise channel," *Bell
  System Technical Journal* 39, pp. 1253–1265, 1960.
- [Elliott1963] E. O. Elliott, "Estimates of error rates for codes on
  burst-noise channels," *Bell System Technical Journal* 42,
  pp. 1977–1997, 1963.
- [Winstein2013] K. Winstein, A. Sivaraman, H. Balakrishnan, "Stochastic
  forecasts achieve high throughput and low delay over cellular networks,"
  NSDI 2013.

**Estimation and detection**

- [Adams2007] R. P. Adams, D. J. C. MacKay, "Bayesian Online Changepoint
  Detection," arXiv:0710.3742, 2007.
- [Wald1948] A. Wald, J. Wolfowitz, "Optimum character of the sequential
  probability ratio test," *Annals of Mathematical Statistics* 19(3),
  pp. 326–339, 1948.
- [Lorden1971] G. Lorden, "Procedures for reacting to a change in
  distribution," *Annals of Mathematical Statistics* 42(6),
  pp. 1897–1908, 1971.
- [Moustakides1986] G. V. Moustakides, "Optimal stopping times for detecting
  changes in distributions," *Annals of Statistics* 14(4),
  pp. 1379–1387, 1986.
- [McFadden1974] D. McFadden, "Conditional logit analysis of qualitative
  choice behavior," in *Frontiers in Econometrics*, Academic Press, 1974.
- [vonNeumann1941] J. von Neumann, R. H. Kent, H. R. Bellinson, B. I. Hart,
  "The mean square successive difference," *Annals of Mathematical
  Statistics* 12(2), pp. 153–162, 1941.
- [Allan1966] D. W. Allan, "Statistics of atomic frequency standards,"
  *Proceedings of the IEEE* 54(2), pp. 221–230, 1966.
- [RFC3550] H. Schulzrinne et al., "RTP: A Transport Protocol for Real-Time
  Applications," RFC 3550, 2003.
- [RFC6298] V. Paxson, M. Allman, J. Chu, M. Sargent, "Computing TCP's
  Retransmission Timer," RFC 6298, 2011.
- [Abramowitz1964] M. Abramowitz, I. A. Stegun, *Handbook of Mathematical
  Functions*, National Bureau of Standards, 1964.

**FEC codes**

- [RFC6330] M. Luby et al., "RaptorQ Forward Error Correction Scheme,"
  RFC 6330, 2011.
- [RFC8681] V. Roca, B. Teibi, "Sliding Window Random Linear Code (RLC)
  Forward Erasure Correction (FEC) Schemes," RFC 8681, 2020.

**Streaming codes**

- [Martinian2004] E. Martinian, C.-E. W. Sundberg, "Burst erasure correction
  codes with low decoding delay," *IEEE Trans. Information Theory*, 2004.
- [Badr2017] A. Badr, P. Patil, A. Khisti, W.-T. Tan, J. Apostolopoulos,
  "Layered constructions for low-delay streaming codes," *IEEE Trans.
  Information Theory*, 2017, arXiv:1308.3827.
- [Fong2019] S. L. Fong, A. Khisti, B. Li, W.-T. Tan, X. Zhu,
  J. Apostolopoulos, "Optimal streaming codes for channels with burst and
  arbitrary erasures," *IEEE Trans. Information Theory* 65(7), 2019,
  arXiv:1801.04241.
- [Krishnan2020] M. N. Krishnan, V. Ramkumar, M. Vajha, P. V. Kumar, "Simple
  streaming codes for reliable, low-latency communication," *IEEE
  Communications Letters* 24(2), 2020.
- [Rudow2023] M. Rudow et al., "Tambur: Efficient loss recovery for
  videoconferencing via streaming codes," NSDI 2023.
- [Vajha2020] M. Vajha, V. Ramkumar, M. Jhamtani, P. V. Kumar, "On the
  performance analysis of streaming codes over the Gilbert-Elliott
  channel," ITW 2021, arXiv:2005.06921.
- [Vajha2020b] M. Vajha, V. Ramkumar, P. V. Kumar, "On sliding window
  approximation of Gilbert-Elliott channel for delay constrained setting,"
  arXiv:2005.06914, 2020.
- [RLC_GE2025] "On the analysis of random linear streaming codes in
  stochastic channels," arXiv:2509.01894, 2025.

**Multipath, scheduling and queueing**

- [Sarwar2013] G. Sarwar, R. Boreli, E. Lochin, A. Mifdaoui, G. Smith,
  "Mitigating receiver's buffer blocking by delay aware packet scheduling
  in multipath data transfer," IEEE WAINA (PAMS), 2013.
- [Kuhn2014] N. Kuhn, E. Lochin, A. Mifdaoui, G. Sarwar, O. Mehani,
  R. Boreli, "DAPS: Intelligent delay-aware packet scheduling for multipath
  transport," IEEE ICC, 2014.
- [Ferlin2016] S. Ferlin, Ö. Alay, O. Mehani, R. Boreli, "BLEST: Blocking
  estimation-based MPTCP scheduler for heterogeneous networks," IFIP
  Networking, 2016.
- [Lim2017] Y. Lim, E. M. Nahum, D. Towsley, R. J. Gibbens, "ECF: An MPTCP
  path scheduler to manage heterogeneous paths," ACM CoNEXT, 2017.
- [Cui2015] Y. Cui, L. Wang, X. Wang, H. Wang, Y. Wang, "FMTCP: A fountain
  code-based multipath transmission control protocol," *IEEE/ACM Trans.
  Networking* 23(2), 2015.
- [RFC6182] A. Ford et al., "Architectural Guidelines for Multipath TCP
  Development," RFC 6182, 2011.
- [RFC8684] A. Ford et al., "TCP Extensions for Multipath Operation with
  Multiple Addresses," RFC 8684, 2020.
- [Raiciu2012] C. Raiciu et al., "How hard can it be? Designing and
  implementing a deployable multipath TCP," NSDI 2012.
- [Xia2003] Y. Xia, D. N. C. Tse, "Analysis on packet resequencing for
  reliable network protocols," IEEE INFOCOM, 2003.
- [Cloud2014] J. Cloud, D. Leith, M. Médard, "In-order delivery delay of
  transport layer coding," arXiv:1408.1440, 2014.
- [Nelson1988] R. Nelson, A. N. Tantawi, "Approximate analysis of fork/join
  synchronization in parallel queues," *IEEE Trans. Computers* 37(6),
  pp. 739–743, 1988.
- [Joshi2014] G. Joshi, Y. Liu, E. Soljanin, "On the delay-storage trade-off
  in content download from coded distributed storage systems," *IEEE JSAC*
  32(5), 2014.
- [Joshi2017] G. Joshi, E. Soljanin, G. W. Wornell, "Efficient redundancy
  techniques for latency reduction in cloud systems," *ACM ToMPECS* 2(2),
  2017.
- [Eppen1979] G. D. Eppen, "Effects of centralization on expected costs in a
  multi-location newsboy problem," *Management Science* 25(5),
  pp. 498–501, 1979.

**Hybrid FEC-ARQ and tail latency**

- [Mehrotra2010] S. Mehrotra, J. Li, Y. Huang, "Optimizing FEC transmission
  strategy for minimizing delay in lossless sequential streaming," *IEEE
  Trans. Multimedia*, 2010.
- [Mehrotra2009] S. Mehrotra, J. Li, "A hybrid FEC-ARQ protocol for
  low-delay lossless sequential data streaming," IEEE MMSP, 2009.
- [Razavi2008] R. Razavi et al., "Performance evaluation of joint FEC and ARQ
  optimization heuristic algorithms under Gilbert-Elliot wireless channel,"
  IEEE CCNC, 2008.
- [Zeng2021] G. Zeng, L. Chen, B. Yi, K. Chen, "Optimizing tail latency in
  commodity datacenters using forward error correction,"
  arXiv:2110.15157, 2021.

**Congestion control, queues and loss recovery**

- [Jacobson1988] V. Jacobson, M. J. Karels, "Congestion avoidance and
  control," ACM SIGCOMM, 1988.
- [Copa2018] V. Arun, H. Balakrishnan, "Copa: Practical delay-based
  congestion control for the Internet," NSDI 2018.
- [Cardwell2016] N. Cardwell, Y. Cheng, C. S. Gunn, S. H. Yeganeh,
  V. Jacobson, "BBR: Congestion-based congestion control," *ACM Queue*
  14(5), 2016.
- [Hollot2002] C. V. Hollot, V. Misra, D. Towsley, W. Gong, "Analysis and
  design of controllers for AQM routers supporting TCP flows," *IEEE Trans.
  Automatic Control* 47(6), 2002.
- [Skogestad2005] S. Skogestad, I. Postlethwaite, *Multivariable Feedback
  Control: Analysis and Design*, 2nd ed., Wiley, 2005.
- [Huang2012] T.-Y. Huang, N. Handigol, B. Heller, N. McKeown, R. Johari,
  "Confused, timid, and unstable: picking a video streaming rate is hard,"
  ACM IMC, 2012.
- [RFC8289] K. Nichols, V. Jacobson, A. McGregor, J. Iyengar, "Controlled
  Delay Active Queue Management," RFC 8289, 2018.
- [RFC8985] Y. Cheng, N. Cardwell, N. Dukkipati, P. Jha, "The RACK-TLP Loss
  Detection Algorithm for TCP," RFC 8985, 2021.
- [RFC9002] J. Iyengar, I. Swett, "QUIC Loss Detection and Congestion
  Control," RFC 9002, 2021.
- [RFC6928] J. Chu, N. Dukkipati, Y. Cheng, M. Mathis, "Increasing TCP's
  Initial Window," RFC 6928, 2013.
- [RFC2018] M. Mathis, J. Mahdavi, S. Floyd, A. Romanow, "TCP Selective
  Acknowledgment Options," RFC 2018, 1996.
- [RFC3168] K. Ramakrishnan, S. Floyd, D. Black, "The Addition of Explicit
  Congestion Notification (ECN) to IP," RFC 3168, 2001.

**Information theory**

- [Shannon1948] C. E. Shannon, "A mathematical theory of communication,"
  *Bell System Technical Journal* 27, 1948.
- [Gallager1968] R. G. Gallager, *Information Theory and Reliable
  Communication*, Wiley, 1968.

---

## Appendix A: Key Formulas

Each formula names the section that states it and the function that
implements it. "—" means the law is a model statement not implemented in the
engine.

### A.1 Channel and estimation (Section 2)

```text
   ε  =  p/(p+q) ,   B = 1/q ,   P(T ≥ t) = (1−q)^(t−1)
   σ²_burst  =  1 + 2(1−p−q)/(p+q)                          burst_variance_factor
   Var_GE(K)  =  W·ε·(1−ε)·σ²_burst
```

### A.2 Recovery (Section 3)

```text
   t_fec      =  m·(1+r) / (r·(1−ε)) · t_sym
   P_lost(t)  =  ε / [ε + (1−ε)·(1 − Φ((t − SRTT)/RTTVAR))]            p_lost
   τ(t)       =  r·q·(1−q)^t                                            TaperFunction
   P_arq      =  1 − (1 − ρ) / (ε·(1 − P_fec))
   ε_codec,eff  =  ε_codec·(1 − (1−ε)^W)
```

### A.3 The rate law (Section 4)

```text
   δ(hint)   =  δ_auto/ζ(hint) ,   δ_auto = 0.5 ,   ζ ∈ {0.01, 1, 100}   net::delta_price
   t_tail    =  clamp(base·δ_auto/δ, 10⁻⁹, 0.1) ,   base = 10⁻⁵         FecRateController::new_with_toggles

   P_fec(r)  =  Φ( √W·(r(1−ε) − ε) / √(ε(1−ε)(r + σ²_burst)) )
   r*        =  max(0, ε̂/(1−ε̂) + Φ⁻¹(1 − t/ε̂)·√(ε̂·σ²_burst/(W(1−ε̂))))  compute_r_star_with_z

   r*_mass   =  min{ r ∈ [0,2] : (1−f)·T_lo(R) + f·T_hi(R) ≤ t/ε̂ } ,  S_m(x) = θ^(x^k)   r_star_mass
   r_burst   =  (B̂/T_rtt)·(1 − t/ε̂)⁺
   r         =  clamp( soft_sat( max(r_core, r_burst, r_mass), r_sat ), 0, r_max )   controller_rate

   t_eff,bulk  =  ε̂ + (0.05 − ε̂)·χ
   χ(T_rem)    =  Φ̄((T_rem − 1.5·SRTT)/σ_arq) ,   σ_arq = max(4·RTTVAR, SRTT/4)   completion_exposure

   β(δ)      =  clamp( (log₁₀0.5 − log₁₀δ)/(log₁₀0.5 − log₁₀0.005), 0, 1 )   bulkness_of_delta
   r(β)      =  (1 − β)·r_anchor + β·r_late-is-fine                        compute_repair_rate
   |r(β(δ)) − r_bulk|  ≤  (1 − β(δ))·r_max

   W*        =  clamp(W_over, min(W_bur, W_lat), W_lat) → [16, 512]         derive_window
   W_over = z²σ²(1−ε)/(εα²) ,  W_lat = budget·rate ,  W_bur = ((σ²+1)/2)/(ε(1−ε))

   ρ < 1:   r  =  A·(1 − (1−q)^(T_cut+1))/q ;   t = ε(1 − P_fec)·P_arq/ρ          —
   r*_unified  =  max_{i∈E} [ εᵢ/(1−εᵢ) + z_{tᵢ/εᵢ}·√(εᵢσ²ᵢ/(W(1−εᵢ))) ] ,  E = {i : dᵢ − d_min ≤ H}   —
   α^{3/2}(1 − α)^{1/2}  ≤  p·σ·G(u)/(2·ν·D_arq·(1 − ε̂)) ,  G(u) = √(2π)e^{u/2}/√u   —

   L_pro(r; δ, χ)  =  r + δ·χ·ε̂·(1 − P_fec(r))·D_arq/d                      —
   interior r* > 0  ⇔  δ·χ ≥ δ_exit = √(2π)·S·d/(ε̂·D_arq) ,  S = √(ε̂σ²/(W(1−ε̂)))   —
```

### A.4 The span machine and placement (Section 5)

```text
   b(δ)   =  clamp( 2^(−½·log₁₀(δ/δ_auto)), ½, 2 )                         span_horizon_b
   D(δ)   =  min( b(δ)·RTT, 2·RTT )                                         shed_deadline_us
   A*     =  clamp( ⌈rate·D⌉, 1, W )                                        emit_source
   M*     =  clamp( ⌈rate·2·RTprop/A*_q⌉ + 1, 2, 32 )                        gen_pipe_depth
   Δ      =  clamp( ⌈rate·J⌉, 1, 64 )                                       emit_source
   repair span  =  [F, F + A*) ,   F + A* ≤ sent − Δ

   shed  ⇔  age > D(δ)  ∧  shed_total + 1 ≤ ε̂·(1 − P_fec(r, A*))·src_total     shed_allowed

   W_mp  ≳  Σᵢ gᵢ·(RTT_max + t_slack)                                      —
   deficit = Σ_{unacked s} ε_path(s) ;  cross-path r = ε_A/(1 − ε_B)        CorrectionDeficit

   P(i) ∝ exp(−(c_i − c_min)/T) ,   T = 0.15                               place_probs_with_temperature
   c_i  =  (E_i − min(S, 9/8·srtt_i))⁺/ref + w_bw(δ)·r_i + w_div·fate_i      place_costs
   E_i  =  (in_flight_i/cwnd_i)·srtt_i + srtt_i/2 + ε_i·srtt_i
   w_bw(δ)  =  clamp( ½ − ¼·log₁₀(δ/δ_auto), 0, 1 )                          SchedulingWeights::from_delta
   X_i  =  [δ·s_i + κ·(s_i − H)⁺]/ref ,  s_i = [(now + E_i) − F̂]⁺           RWM_PLACE_HOL (off)
   T    =  (√6/π)·σ̂_e/ref                                                   RWM_PLACE_T_DERIVED (off)
```

### A.5 Flow control (Section 6)

```text
   cap (N ≥ 2)  =  clamp( (1 + q(δ))·Σ_{i∈active} bwᵢ·RTpropᵢ, 10, N·2048 )   pooled_store_cap
   q(δ)         =  0.05 + 0.05·(b(δ) − ½)/1.5  =  (b(δ) + 1)/30             codel_setpoint_q
   cap (N = 1)  =  clamp( 2.0·BtlBw·RTprop, 10, 1024 )    (boot 128)
   store_len    =  retained − released(SACKed)                              sack_release_mark
   stall(δ, ρ)  =  (1 − ρ)·D(δ) + ρ·(9/8·SRTT + SRTT)                        contract_stall_s
```

### A.6 The recovery decision (Section 7)

```text
   time threshold    age(live flight) ≥ max(9/8·max(SRTT, EWMA), 10 ms)     mp_time_threshold_split
   packet threshold  ≥ 3 later same-path symbols delivered                   MP_PACKET_THRESHOLD
   refresh           (2·SRTT).clamp(25 ms, 100 ms)                          hole_nack_refresh

   Λ(ℓ) = 1/S(ℓ) ;   declare loss  ⇔  ℓ ≥ F⁻¹(1 − α)                         —

   R(T) = π₀·S(T)·w + π₁·P_arq·δ·(T + d)/d + π₁·P_arq·Φ(T)                  —
   Φ(T) = g·κ·(T + d − H)⁺/T_pay ,   H = (m − 1)·RTprop_w                    —
   T* ∈ [0, min((H − d)⁺, F⁻¹(q_d))] ,  value(T*) − value(0) ≤ R_frac·π₀·F((H − d)⁺)   —

   W(α)  = SRTT + k(α)·σ                                                  refuted, removed (Section 10)
   σ̂_Δ(τ) = median |rtt(tᵢ) − rtt(t_{j(i)})| ,  τ ≤ tᵢ − t_{j(i)} ≤ 2τ ,  τ = RTprop   —
   W_q(α) = X_(N−K+1) ,  N = max(⌈K/α⌉, 2K) ,  K = 10                       qnative_window_n (hold-down)
   hold-down level  s(q*) = w·π₀·d/(δ·(1 − π₀)·P_arq)                        RWM_HOLDDOWN_Q (off)

   L(α) = ν·α·(1 + h/T_pay) + δ·owed·k(α)·σ/d + λ·owed·max(0, k(α)σ − D(δ))/D(δ)   —
   k(α) = √((1 − α)/α) ,   owed = max(0, ε̂·(1 − P_fec) − (1 − ρ))

   ℓ* = min{ℓ : w·π₀·f(ℓ) ≤ π₁·P_arq·δ/d} ∧ (H − d)⁺ ,   α = S(ℓ*)           net/late.rs
   receiver form:  request ⇔ ρ̂_heal(ℓ) ≤ c/(1 + c) ,  c = δ/δ_auto          request_bar
   REQUEST = (a, m, k) ,  m = clamp(⌈ln 2/(−ln π̂₀)⌉, 1, A*) ,  k = holes − pivots   request_m
```

### A.7 Congestion control (Section 8)

```text
   Copa utility       U = log(throughput) − δ·log(delay) ;  queue* = 1/δ packets
   wire-mode update   direction = (cwnd/SRTT ≤ 1/(δ·d_q)) ,  step = v/δ
   coupling cap       cwnd ≤ BDP + 2/δ
   BtlBw anchor       cwnd += max(2, 0.25·(BtlBw·RTprop − cwnd)) ;  cwnd ≥ 0.85·BtlBw·RTprop
```
