# Literature Map — FEC-Transport Findings vs the Networking/Coding Literature

**Scope.** Desk research mapping the empirical findings of this project's
FEC/ARQ transport work (goal-gate.md FINAL CONSOLIDATED VERDICT; the RWM Phase
A/B/C, Fungible-Frontier, and SACK+BDP arcs; paper §8, §15, §16) onto the
established literature. For each finding: what is established (and we merely
re-derived), any known solution we may have missed, and an honest,
conservative novelty classification.

**Headline.** Almost everything the arc concluded is **established**. The
single most consequential thing the literature already knew and we spent ~15
L1 investigations rediscovering is **RFC 9265 §3**: on a congestion-limited
reliable transfer, FEC repair competes for the same window as the data, so it
"mainly reduces goodput." That is our presence⊥throughput identity, verbatim,
in a 2022 IRTF RFC. The single most important *solution* we under-weighted is
**FMTCP** (fountain-coded MPTCP, Cui et al., IEEE/ACM ToN 2015): it aggregates
heterogeneous paths by exactly the rateless/decode-on-total mechanism our
oracle proved reaches ×1.19 and our production sliding-window attempt failed to
realize. We did not invent a new bound; our contribution is a rigorous,
mechanism-level re-derivation plus one clean synthesis (the (H, r) fungibility
duality) and one concrete negative result (moving-anchor coded windows
aggregate *negatively* on real per-path-timed multipath — a rediscovery of why
FMTCP uses per-block fountains with a stable anchor).

---

## Finding 1 — Reliable-throughput is recovery-latency-bound; FEC's "recover without a round-trip" needs SPARE bandwidth (presence⊥throughput)

**The literature.** This is established from two independent directions.
(a) The steady-state throughput of a loss-reactive reliable flow is
recovery-limited: **Mathis et al. 1997** and **Padhye, Firoiu, Towsley & Kurose
(SIGCOMM 1998)** give throughput ≈ (MSS/RTT)·1/√p (the "Mathis/PFTK" law) —
throughput falls with RTT and with loss precisely because every loss costs a
recovery event. (b) The FEC-specific half — that repair *displaces* source
bytes on a rate-limited path — is the explicit subject of **RFC 9265,
"Forward Erasure Correction (FEC) Coding and Congestion Control in Transport"**
(Kuhn, Lochin, Michel & Welzl, IRTF NWCRG, July 2022) and its companion draft
"Coding and Congestion Control in Transport" (draft-irtf-nwcrg-coding-and-
congestion). RFC 9265 §4 states that "an increase of the repair ratio should be
done conjointly with a decrease of the source sending rate," and §3 that "for
reliable transfers, coding usage does not guarantee better performance;
instead, it would *mainly reduce goodput*." That is the presence⊥throughput
identity in one sentence: a saturated reliable path has no spare, so a repair
buys a round-trip back only by evicting the source it would otherwise carry.
Google's own QUIC XOR-FEC experiment corroborates empirically — it measurably
degraded YouTube QoE on congestion-limited paths and was removed before IETF
standardization (Langley et al., "The QUIC Transport Protocol," SIGCOMM 2017,
and the QUIC design retrospective). The HARQ/coded-ARQ throughput-delay
literature (e.g. Malak, Médard et al., "ARQ with Cumulative Feedback…," and the
general HARQ analyses) makes the same point: coding gains show up as *delay*
and *robustness*, with throughput gains only where feedback/loss structure
leaves slack.

**KNOWN vs NOVEL: KNOWN (re-derived).** The result is textbook-plus-RFC. Our
"presence⊥throughput identity" is a crisp restatement of RFC 9265 §3–4, and the
1/√p recovery-bound is the Mathis/PFTK law. No novelty in the result; value is
only in having measured it end-to-end on our own stack.

**Missed solution.** RFC 9265 itself — had it been the starting axiom, the
"can spare-less FEC beat ARQ on a saturated reliable link?" question is answered
"no" a priori. Also the NWCRG "coding-and-congestion" draft's placement taxonomy
(FEC above / within / below transport) directly frames why FEC-below-CC looks
like a free lunch and isn't.

---

## Finding 2 — On a single saturated reliable path FEC = ARQ (parity); FEC wins TAIL LATENCY and PREDICTABILITY, not bulk throughput

**The literature.** This is the settled understanding of coded transport.
**TCP/NC** (Sundararajan, Shah, Médard et al., "Network Coding Meets TCP:
Theory and Implementation," Proc. IEEE 2011) and **Coded TCP / CTCP** (Kim,
Cloud, ParandehGheibi, Urbina, Fouli, Leith, Médard, "Network Coded TCP
(CTCP)," arXiv:1212.2291, 2012) both demonstrate that the coding win is
concentrated in *lossy / high-loss / interference-limited* regimes (CTCP:
>90% goodput efficiency at loss rates where TCP collapses) — i.e. where ARQ's
recovery cost is high — not on clean saturated links. The tail-latency framing
is the explicit thesis of **Tambur** (Rudow et al., NSDI 2023, videoconferencing
streaming codes), **CloudBurst / Zeng et al.** ("Optimizing Tail Latency … using
FEC," arXiv:2110.15157, 2021 — p99 cut 60–75% via proactive FEC), and
**Mehrotra & Li** (hybrid FEC-ARQ for low-delay streaming, MMSP 2009 / IEEE
Trans. Multimedia 2010). RFC 9265 §3 again supplies the negative half:
FEC "mainly reduces goodput" for reliable transfer. Google's QUIC-FEC removal is
the production data point that XOR parity on a congestion-limited reliable path
is a net loss.

**KNOWN vs NOVEL: KNOWN.** FEC-buys-tail-and-predictability-not-throughput is
the consensus of the coded-transport literature. Our measured "0.99× parity,
1.04× only at RTT200/10%, 12–60× tail-p99, ~93× lower completion variance" is a
clean quantitative instance, not a new phenomenon.

**Missed solution.** None fundamental. Tambur's ML-driven loss-prediction rate
adaptation is a more sophisticated realization of our r* controller for the
streaming (lossy, ρ<1) profile — worth citing as prior art for the Realtime hint.

---

## Finding 3 — In-order cumulative-ack frontier serialization caps heterogeneous multipath aggregation at parity

**The literature.** This is the **receive-buffer / head-of-line-blocking bound**
of multipath transport, extremely well established. The resequencing-queue
analysis is **Xia & Tse, "Analysis on Packet Resequencing for Reliable Network
Protocols," INFOCOM 2003** (already cited in the paper as the queue behind §16.1
regime 2). The MPTCP-specific statement — a slow subflow's out-of-order arrivals
stall the in-order delivery frontier and can make aggregate < fast-path-alone —
is the motivation of essentially every MPTCP scheduler paper: **BLEST** (Ferlin
et al., IFIP Networking 2016), **DAPS** (Delay-Aware Packet Scheduler, Kuhn et
al.), **ECF**, and the receive-buffer analyses of **Barré/Bonaventure** and
**"Tolerating path heterogeneity in MPTCP with bounded receive buffers"**
(Comput. Netw. 2014). Our "cumulative-ack frontier that freezes on a hole and
advances at ≈1 ARQ round/RTT" is exactly TCP/MPTCP's in-order receive-window
semantics; the "slow path's out-of-order contribution can't advance the frontier
faster than recovery" is the HoL bound restated. Coding as the fix for this
specific bound is **"MPTCP meets FEC"** (Ferlin, Kucera, Claussen, Kuhn et al.,
IEEE/ACM Trans. Networking 26(5), 2018) and, decisively, **FMTCP** (see missed
solution below).

**KNOWN vs NOVEL: KNOWN.** The bound is standard MPTCP theory. Framing it as a
"cumulative-ack frontier serialization" is a faithful restatement, not a new
result.

**Missed solution — the important one. FMTCP** (Cui, Wang, Wang, Wang & Wang,
"FMTCP: A Fountain Code-Based Multipath Transmission Control Protocol," IEEE/ACM
Trans. Networking 23(2):465–478, 2015). FMTCP's abstract states our exact
finding — "a subflow experiencing high delay and loss … becomes the bottleneck
… significantly degrading the aggregate goodput" — and its fix is precisely the
"rateless ack-frontier where a hole is never a fixed in-order position" that our
goal-gate names as the unbuilt fix: fountain-encoded symbols are fungible across
subflows, and a data-allocation rule schedules by *expected arrival time* so the
decoder completes each block on decode-on-total rather than an in-order byte
frontier. This is the mechanism our multipath oracle proved reaches ×1.19 at C8.
We independently re-derived FMTCP's rationale; we should cite it as the prior art
that already realized heterogeneous coded aggregation.

---

## Finding 4 — Sliding-window vs block FEC as one continuum parameterized by window ADVANCE STEP (block = advance-by-W, streaming = advance-by-1)

**The literature.** The block↔sliding-window spectrum is well understood in the
RLNC literature; "generation size" *is* block size, and finite-sliding-window
RLNC is its continuous generalization. See **CRLNC / "Caterpillar RLNC"**
(Wunderlich, Cabrera, Fitzek et al., 2017), **"We don't need no generation — a
practical approach to sliding window RLNC"** (Wunderlich et al., 2017), and the
coding-depth studies (e.g. Liaskos et al., WoWMoM 2023). The delay/efficiency
tradeoff of window overlap is the subject of the streaming-codes theory the
paper already cites — **Martinian & Sundberg 2004**, **Badr et al. 2017** (proves
streaming capacity C(T,B)=T/(T+B)), **Fong, Khisti, Li & Tan 2019** — and of the
low-delay-RLNC-over-a-stream line: **Karzand & Leith, "Low delay random linear
coding over a stream," Allerton 2014**, and **Karzand, Leith, Cloud & Médard,
"Design of FEC for Low Delay in 5G," IEEE JSAC 35(8):1783–1793, 2017**, plus
**Cloud & Médard, "In-Order Delivery Delay of Transport Layer Coding,"
arXiv:1408.1440, 2014** (already cited). RFC 8681 (sliding-window RLC FEC) and
RFC 6330 (RaptorQ block fountain) are the two IETF endpoints of the same axis.

**KNOWN vs NOVEL: KNOWN components; MILD expository novelty.** That block and
sliding-window are two settings of one code is textbook; parameterizing the
continuum specifically by *window advance step* (advance-by-W = block,
advance-by-1 = streaming) is a clean pedagogical framing we did not find stated
in exactly those terms, but it is a restatement, not a new result. No novel
theorem. Our paper §15's "block = σ→0 spike-limit of the streaming taper" is the
same unification in a different parameter.

**Missed solution.** None — this is a modeling/exposition finding, not a problem
needing a solution. The Karzand–Leith throughput-delay-optimal placement of
redundancy *within* the window is the concrete design lever our advance-step
parameter abstracts.

---

## Finding 5 — Gilbert-Elliott inadequacy for real cellular loss (long memory / heavy-tailed bursts / non-stationarity; r* under-provisions)

**The literature.** Well established, with a useful split by link type. GE is
*adequate for wired backbone* loss — **Hasslinger & Hohlfeld, "The Gilbert-
Elliott Model for Packet Loss in Real Time Services on the Internet," MMB 2008**
fit GE to Deutsche Telekom backbone traces and found simple Markov models
appropriate. GE is *inadequate for wireless* — the 802.11/cellular literature
repeatedly shows two-state Markov chains miss the long-memory autocorrelation
and heavy burst tails: higher-order **hidden Markov models** (e.g. Konrad et al.;
"Accurate hidden Markov modeling of packet losses in indoor 802.11 networks,"
IEEE Comm. Letters 2009; and HMM birth-death constructions that out-perform GE on
ACF and burst-length CCDF). Non-stationarity of cellular capacity/loss is the
premise of the trace-driven emulation line: **Winstein, Sivaraman & Balakrishnan,
"Stochastic Forecasts Achieve High Throughput and Low Delay over Cellular
Networks" (Sprout), NSDI 2013** and the **Mahimahi** trace toolset — the very
Saturator/Mahimahi traces our real-trace validation replays. FEC-provisioning
against *empirical* rather than model-tail loss is exactly the recommendation of
**Vajha et al.** (streaming codes over GE, arXiv:2005.06921) and the DCSW-to-GE
approximation work (Vajha, Ramkumar & Kumar, arXiv:2005.06914).

**KNOWN vs NOVEL: KNOWN.** GE's wireless inadequacy is decades-old. Our specific
contribution — quantifying that GE under-provisions r* by ~2–4× beyond its own
GE-ideal, up to 12.8× the target δ, on replayed real cellular traces — is a
concrete measurement instance, not a new modeling insight. The recommended
enrichment (semi-Markov / heavy-tailed-sojourn / regime-switching + empirical-
quantile provisioning) restates the HMM-over-GE consensus.

**Missed solution.** The HMM-birth-death and semi-Markov burst models are the
"solution" we flagged as future work but did not build; they are the standard
answer. Provisioning against the empirical window-loss quantile (not the
Gaussian/GE tail) is the pragmatic fix and is implicit in the streaming-codes-
over-GE evaluation methodology.

---

## Finding 6 — Order-statistic / fork-join framing of multipath completion (E[max] vs K-of-N)

**The literature.** The cited anchors are **correct and canonical.**
Fork-join E[max] response time: **Nelson & Tantawi, "Approximate Analysis of
Fork/Join Synchronization in Parallel Queues," IEEE Trans. Computers 37(6), 1988**
(the harmonic-number H_N law). The move from fork-join max to a K-of-N interior
order statistic under coding: **Joshi, Liu & Soljanin, "On the Delay-Storage
Trade-off in Content Download from Coded Distributed Storage Systems," IEEE JSAC
32(5), 2014**, and **Joshi, Soljanin & Wornell, "Efficient Redundancy Techniques
for Latency Reduction in Cloud Systems," ACM ToMPECS 2(2), 2017** — which
quantifies exactly the E[max]-vs-order-statistic gap §16.1 uses. These are the
right anchors; no re-attribution needed. Newer work worth adding: the
redundancy-queueing survey/monograph line (Gauri Joshi's *Efficient Redundancy*
monograph; Peng & Soljanin, "Diversity vs. Parallelism in Distributed Computing
with Redundancy"), and — most directly on point for *coded multipath transport*
— **"On the Role of Preemption for Timing Metrics in Coded Multipath
Communication," arXiv:2302.07562, 2023**, which carries the order-statistic
timing analysis into the multipath-transport setting our §16 targets.

**KNOWN vs NOVEL: KNOWN (correct import).** The framing is a faithful application
of established fork-join / coded-download theory to reliable multipath transport
completion. The mapping "within-unit striping = fork-join, whole-unit affinity =
resequencing queue, rateless-over-horizon = K-of-N order statistic" (paper §16.1)
is a clean taxonomy, but each cell is a known result (Nelson–Tantawi, Xia–Tse,
Joshi et al. respectively). FMTCP and Joshi's coded-download analysis already
apply order statistics to coded multipath, so even the transport application is
not new.

---

## Known solutions to the recovery-latency / in-order-frontier bound

**Does the literature offer a way to *exceed* the in-order-frontier recovery-
latency throughput bound for reliable delivery on a lossy link?** Short answer:
**not for tight-δ, in-order, incremental-consumer delivery on a saturated path.**
Every technique that "beats" the bound does so by changing one of the two things
our own §16.7 (H, r) surface identifies — it relaxes ordering, or it spends spare
bandwidth — never by violating the bound itself.

1. **FMTCP** (Cui et al., IEEE/ACM ToN 2015) — the strongest missed solution.
   Fountain-codes the aggregate so no symbol is a fixed in-order position and
   completes each block on decode-on-total; provably mitigates heterogeneity.
   This is our unbuilt "rateless ack-frontier." It does **not** break the bound —
   it delivers whole blocks/objects (decode-on-total, i.e. H→∞ per block), not an
   in-order byte stream to an incremental consumer.
2. **CTCP / TCP/NC** (Kim/Cloud/Médard 2012; Sundararajan/Médard 2011) — sliding
   coding window with coded ACKs; "seen" packets advance the window before
   decoding, which *shrinks* the in-order wait (Cloud & Médard 2014) but still
   pays recovery latency where there is no spare — consistent with the bound.
3. **MPTCP meets FEC** (Ferlin et al., ToN 2018) — proactive coded redundancy to
   cut HoL-induced recovery rounds on the slow subflow; buys the recovery back
   with bandwidth (the r knob), exactly as our (H, r) surface predicts.
4. **Multi-path low-delay network codes** (Cloud & Médard, GLOBECOM 2016) and
   **Low-Delay RLC over Multiple Interfaces** (IEEE TMC 2017) — coded multipath
   scheduling for in-order delay; same lever.
5. **SCDP** (systematic rateless coding for data-centre transport, 2019) and the
   AeroMTP/HMTP/JDAFC fountain-multipath streaming line — bandwidth-aggregation
   via rateless coding with decode-on-total object semantics.
6. **Google QUIC XOR-FEC** — the *negative* data point: an attempt to buy
   round-trips back with parity on congestion-limited reliable paths, removed
   because it reduced goodput. Confirms the bound rather than breaking it.

**Conclusion on the bound.** No known technique breaks the recovery-latency
frontier bound for *reliable in-order incremental delivery on a saturated link*.
The literature's escapes are precisely our two knobs: (H) relax ordering to
decode-on-total (unordered/object/message delivery — FMTCP, fountain multipath),
or (r) spend spare bandwidth on rateless fungibility (MPTCP+FEC, coded multipath).
Our own arc arrived at the same two-knob conclusion (§16.7) — which is
reassurance that the arc is correct, and a caution that the destination was
already mapped.

---

## What (if anything) is genuinely novel in our framing — honest assessment

Conservatively: **no novel theorems, no new bound.** All six findings are
established, several by results decades old (Nelson–Tantawi 1988, Mathis 1997,
Padhye 1998, Xia–Tse 2003) and one by a 2022 IRTF RFC (9265) that states our
central identity outright. The candidates for genuine, if modest, contribution:

1. **The (H, r) fungibility duality (paper §16.7).** Framing the reorder horizon
   H and the repair rate r as *dual knobs*, each of which "buys fungibility" — H
   paying in latency (free in bandwidth), r paying in bandwidth (free in latency)
   — with the triangle's δ selecting the operating point, is a synthesis we did
   not find stated in exactly these terms. Its components are all known (Cloud &
   Médard on coding-vs-in-order-delay; the MPTCP eligibility-set literature on H),
   so this is an *expository/unifying* contribution, not a result. **Mild
   novelty.**

2. **The moving-anchor negative result (Fungible-Frontier / temporal-oracle arc).**
   The concrete, measured finding that a *send-time-windowed* coded sliding
   window aggregates **negatively** on real per-path-timed multipath (dual worse
   than single, ×0.26 at C8), because per-path arrivals land against a frontier
   that has already moved — and that the fix is generation coding with a **stable
   per-generation anchor** — is a useful engineering result. But it is, honestly,
   a *rediscovery of why FMTCP uses per-block fountains with an expected-arrival
   allocation* rather than a naive sliding window. Its value is a crisp isolation
   of the failure mechanism (window misalignment vs fungibility-in-the-abstract),
   not a new idea. **Mild novelty as a negative result.**

3. **The presence⊥throughput "identity" as a named invariant.** A memorable
   restatement of RFC 9265 §3–4, useful internally, but not a new result.

Everything else — recovery-latency-bounded throughput, FEC=ARQ on saturated
reliable links, the HoL/resequencing multipath cap, the block↔sliding-window
continuum, GE's wireless inadequacy, and the fork-join/order-statistic
completion taxonomy — is **KNOWN and re-derived.** The honest value of the arc is
not novelty but *rigor*: an independent, mechanism-level, measured confirmation
of the established position, with a verification oracle that reconciled L0/L1 and
a real-trace validation of the channel-model limits. We should not claim novelty
beyond items 1–2, and even those are unifications/negatives rather than new
theory.

---

## References

Verified by title + authors + venue/year unless noted. IETF RFCs are cited by
number + title + year (stable identifiers); RFC 9265's content was confirmed by
direct fetch.

**Recovery-latency / throughput bounds**
- M. Mathis, J. Semke, J. Mahdavi, T. Ott, "The Macroscopic Behavior of the TCP
  Congestion Avoidance Algorithm," ACM SIGCOMM CCR 27(3), 1997.
- J. Padhye, V. Firoiu, D. Towsley, J. Kurose, "Modeling TCP Throughput: A Simple
  Model and its Empirical Validation," ACM SIGCOMM 1998, pp. 303–314.

**FEC and congestion control**
- N. Kuhn, E. Lochin, F. Michel, M. Welzl, "Forward Erasure Correction (FEC)
  Coding and Congestion Control in Transport," IRTF RFC 9265 (NWCRG), July 2022.
- (companion) draft-irtf-nwcrg-coding-and-congestion, IRTF NWCRG.
- A. Langley et al., "The QUIC Transport Protocol: Design and Internet-Scale
  Deployment," ACM SIGCOMM 2017 (documents removal of QUIC's XOR-FEC).

**Coded transport (FEC vs ARQ; tail latency)**
- J.K. Sundararajan, D. Shah, M. Médard, S. Jakubczak, M. Mitzenmacher, J. Barros,
  "Network Coding Meets TCP: Theory and Implementation," Proc. IEEE 99(3), 2011.
- M. Kim, J. Cloud, A. ParandehGheibi, L. Urbina, K. Fouli, D. Leith, M. Médard,
  "Network Coded TCP (CTCP)," arXiv:1212.2291, 2012.
- M. Rudow et al., "Tambur: Efficient loss recovery for videoconferencing via
  streaming codes," USENIX NSDI 2023.
- G. Zeng, L. Chen, B. Yi, K. Chen, "Optimizing Tail Latency in Commodity
  Datacenters using Forward Error Correction," arXiv:2110.15157, 2021.
- S. Mehrotra, J. Li, "A hybrid FEC-ARQ protocol for low-delay lossless
  sequential data streaming," IEEE MMSP 2009.

**Multipath HoL / resequencing / coded multipath**
- Y. Xia, D.N.C. Tse, "Analysis on Packet Resequencing for Reliable Network
  Protocols," IEEE INFOCOM 2003, pp. 990–1000.
- S. Ferlin, Ö. Alay, O. Mehani, R. Boreli, "BLEST: Blocking Estimation-based
  MPTCP Scheduler for Heterogeneous Networks," IFIP Networking 2016.
- S. Ferlin et al., "MPTCP meets FEC: Supporting Latency-Sensitive Applications
  over Heterogeneous Networks," IEEE/ACM Trans. Networking 26(5), 2018.
- Y. Cui, L. Wang, X. Wang, H. Wang, Y. Wang, "FMTCP: A Fountain Code-Based
  Multipath Transmission Control Protocol," IEEE/ACM Trans. Networking 23(2),
  pp. 465–478, 2015.  ← the principal missed solution.
- J. Cloud, M. Médard, "Multi-Path Low Delay Network Codes," IEEE GLOBECOM 2016.
- J. Cloud, D. Leith, M. Médard, "In-Order Delivery Delay of Transport Layer
  Coding," arXiv:1408.1440, 2014.

**Sliding-window vs block coding continuum**
- E. Martinian, C.-E.W. Sundberg, "Burst erasure correction codes with low
  decoding delay," IEEE Trans. Information Theory, 2004.
- A. Badr, P. Patil, A. Tan, A. Dey, "Layered Constructions for Low-Delay
  Streaming Codes," IEEE Trans. Information Theory, 2017.
- S.L. Fong, A. Khisti, B. Li, A. Tan, "Optimal Streaming Codes for Channels with
  Burst and Arbitrary Erasures," IEEE Trans. Information Theory 65(7), 2019.
- M. Karzand, D.J. Leith, "Low delay random linear coding over a stream,"
  Allerton 2014.
- M. Karzand, D.J. Leith, J. Cloud, M. Médard, "Design of FEC for Low Delay in
  5G," IEEE JSAC 35(8), pp. 1783–1793, 2017.
- S. Wunderlich, J.A. Cabrera, F.H.P. Fitzek et al., "Caterpillar RLNC (CRLNC): A
  Practical Finite Sliding Window RLNC Approach," 2017; and "We don't need no
  generation — a practical approach to sliding window RLNC," 2017.
- IETF RFC 8681 (sliding-window RLC FEC, 2020); IETF RFC 6330 (RaptorQ, 2012).

**Gilbert-Elliott adequacy / wireless loss modeling**
- G. Hasslinger, O. Hohlfeld, "The Gilbert-Elliott Model for Packet Loss in Real
  Time Services on the Internet," GI/ITG MMB 2008, pp. 269–283.
- (HMM for 802.11 loss) "Accurate hidden Markov modeling of packet losses in
  indoor 802.11 networks," IEEE Communications Letters, 2009; and HMM
  birth-death constructions out-performing GE on ACF/CCDF of loss bursts.
- K. Winstein, A. Sivaraman, H. Balakrishnan, "Stochastic Forecasts Achieve High
  Throughput and Low Delay over Cellular Networks" (Sprout), USENIX NSDI 2013;
  Mahimahi trace toolset (the Saturator/Mahimahi traces our validation replays).
- M. Vajha, V. Ramkumar, M. Jhamtani, P.V. Kumar, "On the Performance Analysis of
  Streaming Codes over the Gilbert-Elliott Channel," arXiv:2005.06921, 2020.

**Fork-join / order-statistic / coded-queueing**
- R. Nelson, A.N. Tantawi, "Approximate Analysis of Fork/Join Synchronization in
  Parallel Queues," IEEE Trans. Computers 37(6), pp. 739–743, 1988.
- G. Joshi, Y. Liu, E. Soljanin, "On the Delay-Storage Trade-off in Content
  Download from Coded Distributed Storage Systems," IEEE JSAC 32(5), 2014.
- G. Joshi, E. Soljanin, G.W. Wornell, "Efficient Redundancy Techniques for
  Latency Reduction in Cloud Systems," ACM ToMPECS 2(2), 2017 (arXiv:1508.03599).
- "On the Role of Preemption for Timing Metrics in Coded Multipath
  Communication," arXiv:2302.07562, 2023 (newer coded-multipath timing analysis).

*Attribution note:* where a search surfaced a paper by title and venue but I
could not independently confirm the full author list (the 802.11 HMM
Communications-Letters paper; the coded-multipath-preemption arXiv), it is cited
by title + venue/year and flagged as such rather than with a guessed author list,
per the desk-research verification rule.
