# The price of disorder ω: pre-registration (paper only, no VM contact)

Draft for review before any VM contact. No row has run and no harness line exists. Base text: github main `9bc656bf`. Roadmap node: `meas-disorder-price`. Design inputs:
- `docs/derivations/ordering-hold.md` §1–§5, §10, §11 and its owner answers of 2026-11-21;
- `docs/derivations/skip-rule-efficiency.md` §6;
- `docs/measurement-discipline.md`: rules 1–19, the pre-registration protocol, the VM protocol and the five-hour cap.

House format: `docs/derivations/vm-batch-1-registration.md`. Revised after Fable's review (§14).

**Owner decisions (taken, 2026-11-21).**
- Measure ω on a TCP-in-tunnel test after VM batch 1, and follow the result.
- At Bulk, after a long outage, late out-of-order delivery is acceptable instead of a stall, once the release rule ships.

**For the owner, in plain words.**
- **What is measured.** What one late packet costs a TCP flow inside the tunnel. It is measured two ways:
  - **released:** the packets behind it go ahead;
  - **held:** they wait for it, which is what the receiver does today.

  Both costs are measured against a run with no late packet. ω is the difference between the two, and the receiver's hold is `H_o = ω·d/δ`.
- **No engine code.** A kernel reorderer in the `rp-*` test namespaces makes the same late deliveries a release, or a hold, would make. A small reported stage repeats a few cells with the real tunnel in the path (§3, §6.6).
- **Three choices are declared, not measured. Please confirm them.**
  1. **Linux defaults stand for the TCP the tunnel's users run** (RACK, DSACK undo). A classic, more fragile TCP is measured afterwards, as a bound, once the Linux result has been read.
  2. **One byte of inner goodput lost is priced as one byte on the wire**, the same price as one wasted repair copy. A different price scales ω by the same factor.
  3. **Eight inner TCP flows share the tunnel.** A hold stalls all of them, and a release disorders one. A lone flow is measured later, in phase C, as a sweep.
- **Warning: this can change the rule, not only a number.** A hold stops the inner TCP's acknowledgements, so its sender stops. If the queue in front of the tunnel is shallow, holding a packet for a few milliseconds can cost the inner flow more than releasing it, at every preset. The derivation prices a stall by δ alone. That is about 0.5 symbols per stalled second at Bulk, while a stalled 10 000 packets/s flow loses up to 10 000 packets per second. If the measurement shows this, the rule as derived would release almost at once at every preset, and δ would decide nothing. That goes back to the derivation as a design question (§2), not into a constant.
- **Cost.** About 20 h of VM time for the Linux stack at the minimum repetition count, or about 14 h if the tunnel's standing queue turns out shallow. It is up to about 52 h at the cap, which a pre-registered cut order brings to about 27 h. Anything above 20 h after cuts comes back to you before launch (§12). The classic stack and the sweeps follow in a later amendment.

**Inputs read** (`github/main` `9bc656bf`):
- `ordering-hold.md`: ω's definition and its "matched no-release run" (§1); J_U, with the stall priced as `(δ/d)·T_stall` (§2); the rent-or-buy rule (§3.2); the preset tables (§4); skew (§5); the gauges (§6); the owner answers;
- `skip-rule-efficiency.md` §6: the toy, its jitter (0, 1, 4 ms), the 25 ms refresh floor;
- `tools/l1`: `lib.sh` (`guard_ns`, `stop_raptorpath`), `lib_battery.sh` (`take_lock`, `release_locks`), `topo.sh`, `tun_bulk.sh`, `threadq2_run_all.sh`, `stubrun_gen.py`, `test_stubrun.py`, `harness_paths.py`;
- the tunnel's buffering:
  - `raptorpath/src/tun/linux/mod.rs:24–25`: the TUN read and inject channels, 4 096 packets each;
  - no `txqueuelen` set anywhere in `raptorpath/src/tun`, so the kernel's TUN default applies (500, Linux `drivers/net/tun.c`; R0 reads it back);
  - `net/store_cap.rs:988` `WIN_STORE_MAX` = 4 096;
  - `net/reorder.rs` and `net/receiver.rs:166`, the reorder buffer;
- `raptorpath/src/perf.rs:52`: Bulk/Auto `symbol_size` = 1200 B, TUN MTU 1196;
- RFC 5681 §3.2, RFC 6675, RFC 8985 §6.2, RFC 9438 (CUBIC, β = 0.7).

## 1. What is measured

### 1.1 Arms

Three arms per configuration, all through the same qdisc tree (§4):

| arm | a selected packet | emulates |
|---|---|---|
| **REL(Δt)** | delayed Δt; its successors pass | release at H_o, filled Δt later |
| **HOLD(Δt)** | delayed Δt; every successor waits behind it, then all leave in order at 10 × g | no release: the frontier waits for the fill (`ordering-hold.md` §1's matched no-release run) |
| **CLEAN** | same tree, delay 0 | no event |

### 1.2 The two primary scored curves, and ω derived from them

The bottleneck is saturated by construction (§4). Every slot it leaves idle, and every slot it spends on a retransmission, is one inner packet not delivered. Both are counted from a pcap taken right after the bottleneck, in an attribution window after each event e:

```text
   D_e    =  idle slots  +  retransmitted segments  at the bottleneck output,   in  A_e = [ t_e , t_e + Δt + 4·RTT + Q/g )   (cut at the next event)

   c_R(Δt)  =  ( D̄_REL  − D̄_CLEAN ) · MSS / S          cost of a released-then-filled packet       (scored curve 1)
   c_H(Δt)  =  ( D̄_HOLD − D̄_CLEAN ) · MSS / S          cost of the same packet held                (scored curve 2)
   ω(Δt)    =  c_R(Δt) − c_H(Δt)                         the purchase price net of the rent saved    (derived)

   S = 1200 B (symbol_size, perf.rs:52);   D̄_CLEAN over pseudo-events at seeded random instants with REL's event rate
```

- **Both curves are scored, and ω is derived from them.** ω alone would hide which side moved. §2 needs c_H on its own.
- **First event and later events, kept apart.** RACK remembers reordering: after the first DSACK it widens reo_wnd, and the widening lasts 16 recoveries (RFC 8985 §6.2). So the first event of a connection can cost something and later ones nothing. A per-event average would then scale like 1/(events per connection), which is a design choice.
  - **c_R1** comes from the first release that hits each connection and **c_R∞** from its later releases. A hold disorders no flow, so c_H is one curve for both (§4.4): ω₁ = c_R1 − c_H and ω_∞ = c_R∞ − c_H.
  - Both come from the same reps: F = 8 long-lived connections share the bottleneck, events are switched on only once the standing queue is built (§4.4), and each event is labelled by the flow it hit and its index in that flow (§7.2).
  - **ω₁ governs the N = 1 release.** A released-then-filled hole is rare there, so it usually hits a flow with no recent reordering.
  - **ω_∞ governs the N = 2 skew condition** `ω ≥ δ·Δ/d` (`ordering-hold.md` §5). There every slow-path packet is a release.
- **The goodput guard.** `[ (Ḡ_CLEAN − Ḡ_REL)/Ē_REL − (Ḡ_CLEAN − Ḡ_HOLD)/Ē_HOLD ] / S` over the whole window. It catches costs that outlast A_e, such as a CUBIC regrowth of seconds after a cut that was never undone.
  - If its CI lies entirely above the slot estimate of ω_∞, the outcome is `WINDOW-SHORT` at that configuration. The slot estimates there are VOID, and the goodput guard is read instead, with its MDE (§7.3).
- **Unit.** This is declared choice 2 of the owner section. If the owner prices inner goodput differently, every curve scales linearly.

## 2. Finding for `ordering-hold.md` (a question for the derivation; its law is not edited here)

**What the derivation assumes.**
- J_U (`ordering-hold.md` §2) prices a stall only as `(δ/d)·T_stall`: frontier seconds at the declared delay price.
- The late packet's own delay is policy-invariant there.
- ω is the only measured term, and the rule `B_s = (δ/d)(t − t_detect) ≥ ω` buys when the δ-priced rent reaches ω.
- Nothing in J_U charges what the stall does to the inner flow's sending.

**What a hold does to inner TCP (prediction, from the topology of §4).**
- While a hole is held, the receiver gets nothing past it, so no ACK advances. The sender sends at most its remaining window, then stops.
- The queue in front of the bottleneck keeps the link busy for Q/g, then the link idles until the held run is released and its ACKs return.
- Then the ACK burst releases a send burst of about Δt·g packets into the queue, and whatever does not fit is dropped.
- So, to first order:

```text
   c_H(Δt)  ≈  ( Δt·g − Q )⁺ · MSS/S   +   burst losses when Δt·g exceeds the queue's free space          (shallow Q, saturated bottleneck)
```

**Predicted c_H at the two queues of §4.3** (symbols; MSS/S ≈ 1.2; Δt from §6.1):

| configuration (W, Q_s) | k = 8 | k = 32 | t = 1 | t = 4 | at Q_t (valid events) |
|---|---|---|---|---|---|
| 1 000 /s, RTT 10 (10, 4) | 5 | 34 | 0 | 7 | 0 while Δt·g ≤ B_t, then (Δt·g − B_t)⁺ |
| 10 000 /s, RTT 10 (100, 25) | 0 | 9 | 0 | 90 | the same |
| 1 000 /s, RTT 50 (50, 13) | 0 | 23 | 0 | 44 | the same |
| 10 000 /s, RTT 50 (500, 125) | 0 | 0 | 0 | 450 | the same |

**Against J_U's rent for the same stall**, `δ_p·Δt/d`. At Δt = 50 ms and d = 11 ms that is 0.023 symbols at Bulk, 2.3 at Auto and 227 at Realtime. A measured c_H of 450 symbols exceeds all three. And if c_H > c_R, then ω < 0: holding costs more than releasing.

**The question for the derivation, written now:** is a stall's inner-goodput cost a measured rent term that J_U is missing, beside the declared δ term?
- **If yes**, rent-or-buy compares the measured rent `c_H(t)` (plus `(δ/d)·t`) against the measured purchase price `c_R`. δ then decides only where the inner cost of the stall is small.
- **The answer may depend on the queue in front of the tunnel**, which is why Q is a core factor here (§4.3). The tunnel's own queue is deep, and there c_H ≈ 0 for every operating Δt. A shallow bottleneck elsewhere on the inner path makes c_H dominate.

**The candidate rent, on one line** (a question for `ordering-hold.md`, not a law proposed here):

```text
   rent(t)  =  (δ/d) · t   +   c_H(t)                 c_H(t) ≈ ( t · g − B )⁺ · MSS/S   when a standing backlog B sits in front of the bottleneck
```

**What the engine could read online.** A cost the engine cannot read cannot become a law. It cannot see the inner TCP (cwnd, RACK state, or a queue elsewhere on the inner path). It can see:
- **t**, the hold age;
- **g**, its own delivery rate at the receiver;
- **B, but only where the tunnel is the bottleneck.** B is then the sender's own backlog: the TUN device queue plus the TUN read channel plus the store. The sender holds it, so the receiver would need it on the wire, a feedback field the protocol does not carry.

So the proxy question for the derivation is: is `(t·g − B_tunnel)⁺` an admissible rent term, read from the sender's TUN backlog and carried to the receiver? And for a shallow bottleneck elsewhere (Q_s), which the engine cannot observe, is that cost a declared dial, or out of scope? This registration measures whether the term is material. It does not decide whether it can ship.

**The measurement decides it (§9.1)**, per preset and per queue:
- `RENT-UNDERPRICED-<preset>-<side>`: the measured c_H exceeds `δ_p·Δt/d`. The finding fires: the rule, not only ω, goes back to `ordering-hold.md`.
- `RENT-COVERED-<preset>-<side>`: the measured c_H does not exceed it, on that side of the standing delay. J_U's pricing stands at that preset, and the ω zones of §9.2 apply.

## 3. Engine code: none

| | (i) netem-only reorderer in `rp-*` namespaces | (ii) a test seam in raptorpath that forces releases |
|---|---|---|
| what controls the displacement | the reorderer, exactly: per selected packet, Δt and hence k = Δt·rate | the tunnel's ARQ: the fill comes from a repair the seam cannot time |
| HOLD arm | the same qdisc in FIFO mode | today's receiver, but stall length again set by ARQ |
| moving parts | netem, prio and plug qdiscs, iperf3, tcpdump, ethtool (`adv_cells.sh` already uses iperf3 and ethtool, `perf_rwm_c.sh` tcpdump) | a `test-seams` twin build, the tunnel's CC, pacer, loss process and ARQ, plus all of (i) to read the inner TCP |
| binary identity | untouched | the seam must be `#[cfg(feature = "test-seams")]`, default off, with `binid.yml` unchanged |
| cannot see | bundling (one tunnel seq carrying several small IP packets), the tunnel's pacing jitter | cannot isolate them either |

**Chosen: (i).**
- **Why.** ω and c_H are properties of the inner flow's reaction to a displacement. The displacements that occur are an input, from the release rule's gauges (`ordering-hold.md` §6), and the cells sweep them.
- **Deviation from the owner's wording, stated.** The scored stages run TCP through a reorderer, not through the tunnel.
  - Stage T (§6.6) runs the same reorderer behind the real tunnel and compares, so the deviation is bounded, not argued.
  - It reorders on the inner side, after TUN delivery: the tunnel delivers in order, so underlay reordering never reaches the inner TCP.
- **If Stage T reads DIVERGES**, option (ii) is reconsidered under its own registration.
  - The seam would delay one chosen delivered seq at the reorder-buffer expiry (`receiver.rs:1012–1024`).
  - It would be default-off behind `test-seams`, with no env read in the default build and binid unchanged (ADR-0082).
  - Nothing here builds it.

## 4. Topology, reorderer and queue

Three namespaces, all `rp-*` (`rp-app` is added in stage T), joined by veth pairs. A new script `tools/l1/topo_reorder.sh` brings them up and down, and calls `guard_ns` on every name.

```text
   rp-cli ── cli0 ═══ mid0 ── rp-mid ── mid1 ═══ srv0 ── rp-srv
            egress: bottleneck        egress: the reorderer       egress: the RTT (ACK path)
            netem rate g·1500 B       (§4.1)                       netem delay RTT, no rate
            limit Q, no delay
```

- **Bottleneck** on `cli0`: `netem rate` at g × 1500 B, queue limit Q, no delay, so the limit counts the queue alone. netem with `rate` is FIFO (rule 19). The reorderer sits after it, where packets are spaced 1/g while the queue is busy.
- **RTT:** the whole propagation delay, on the ACK leg (`srv0` egress).
- **Offloads off.** TSO, GSO and GRO are off on all veth ends (`ethtool -K` inside each namespace), so one skb is one packet. Witness: no frame in the pcap exceeds MTU + 14.
- **Forwarding.** `net.ipv4.ip_forward = 1` is set inside `rp-mid` only.

### 4.1 The reorderer (semantics fixed here; the exact `tc` lines are the harness's, verified in R0)

- **REL, HOLD and CLEAN: one netem on `mid1`, with a two-point delay table** (loaded through `TC_LIB_DIR`, nothing written under `/usr/lib`). A fraction p of the entries is +Δt, the rest 0.
  - **REL:** no `rate`. netem's time-sorted queue lets the unselected packets pass the delayed one.
  - **HOLD:** `rate` at 10 × g. netem is then FIFO, with its time reference at the queue tail, so the successors wait, then leave as a 10 × g burst, as a reorder buffer releases a held run.
  - **CLEAN:** the same, with an all-zero table.
- **Selection** is random per packet with probability p, quantised to the table size (R0, §11.1). Events are spaced ≈ 1 s per flow, and the realised count comes from the pcap, never from p.
- **Events start once the queue is built, not at a fixed time** (§4.4). Every arm starts with the zero table and switches to its own (CLEAN to zero again) when the warm-up witness first holds.
- **Outage check (§6.5): prio + plug on `mid1`.**
  - Band 1 takes the odd IP IDs. Linux uses a per-socket IP ID counter for DF packets, so this is exact alternation within a flow, which is N = 2 round robin; R0 verifies it.
  - OUT-REL blocks band 1 for 1 s. OUT-HOLD blocks both bands for the same second. CLEAN blocks neither.
- **Packet displacement from time:** Δt = (k + ½)/g_flow.

### 4.2 VM rules this plan respects

- **Namespaces and devices.** Only namespaces named `rp-*`. Every `ip`, `tc`, `ethtool` and `sysctl` command runs as `ip netns exec rp-…` or `ip -n rp-…`. No `nft` or `iptables`. `ens18`, the firewall and `sshd` are never touched.
- **No root-namespace sysctl is written.**
  - **Pre-flight, before any write:** each §5 sysctl must be present under `/proc/sys/net/ipv4` in a fresh `rp-*` namespace. Global net sysctls are registered for the initial namespace only, so presence there means the sysctl is per namespace. A missing one is ABORT-SYSCTL, with nothing written.
  - The root namespace's `net.ipv4.tcp_*` values are snapshotted at pre-flight, compared right after the first per-namespace write, and compared again at exit (ABORT-HOSTSTATE).
- **No `modprobe`.** `sch_netem` and `sch_plug` autoload when a qdisc is added, as `topo.sh` relies on today. `lsmod` is recorded before and after. A congestion control that is not already available is skipped.
- **Processes.** Killed by exact name or recorded PID only: iperf3 with `--pidfile`, tcpdump by `$!`, raptorpath (stage T) by `stop_raptorpath`.
- **Locks.** `/tmp/rwm-vm.lock` and `${RWM_RP_LOCK:-$HOME/rp.lock}` are held for the whole session.
  - If `test -e` finds either, the session refuses (ABORT-LOCK).
  - Otherwise `take_lock` (noclobber + flock) takes it and `release_locks` frees it.
  - A lock this session did not create is never removed. Under sudo, HOME resolves to the invoking user.
- **Names.** Docs and logs say "the VM", with no user name, address or key path.

### 4.3 The queue Q: measured standing queue (Q_t) and shallow (Q_s), both in the core

**A queue limit is not a standing queue.** A hold costs the link nothing only while a backlog of at least Δt·g packets is already queued in front of the bottleneck when it starts. So the scored level must come from the backlog that inner TCP actually builds in front of the tunnel, and every event carries a witness of it (§4.4).

**Fill time on paper.**
- Linux CUBIC leaves slow start early (HyStart, on a small RTT rise). From there it probes upward at `C·t³` packets with C = 0.4 (RFC 9438).
- Building a 4 596-packet backlog therefore takes `t = (4 596 / 0.4)^(1/3) ≈ 23 s`, about the same at either rate, since the growth is in time.
- v2's 18 s reps with events from about 4 s would have held against a nearly empty queue, and `RENT-UNDERPRICED` at Q_t would have fired as an artefact.

| level | standing backlog B | provenance |
|---|---|---|
| **Q_t, the tunnel's own** | **measured in session R** (§11.1): `B_t = g_tun · (srtt − min_rtt)` of a long-lived inner CUBIC flow through the real tunnel, with the tunnel as the bottleneck | where that queue sits is measured, not assumed: the TUN device's qdisc (whatever `net.core.default_qdisc` gives it on the VM, often fq_codel, which would hold it near 5 ms), the TUN read channel (4 096, `tun/linux/mod.rs:24`), the store (`WIN_STORE_MAX` 4 096), or the underlay's own queue. The 500 + 4 096 of v2 is an upper bound on depth, not a standing queue |
| **Q_s, a shallow bottleneck elsewhere** | limit max(⌈W/4⌉, 4), filled within a few RTT | the end at which a hold is most expensive (§2) |

- **Is the tunnel the bottleneck in stage T's `clean` scenario?** Yes. `clean` is 100 Mbit/s, 5 ms one-way, no loss, no jitter (`lib.sh` `scenario_params`), as a `netem rate` on the underlay leg, so the inner flow is limited by the tunnel's path. R checks it (the inner goodput must sit at ≥ 90 % of the tunnel's measured goodput ceiling).
  - Whether the standing queue then forms in the tunnel's TUN queue and read channel, or in the underlay netem queue, is read from three places: `tc -s qdisc` backlog on `rpcli0`, on the underlay `cli0`, and the engine's channel and store if its gauges print them.
- **Emulating Q_t.** The amendment picks the bottleneck's queue discipline and limit at the netem cells so that the warm-up witness reproduces B_t's standing delay within ± 25 %.
  - The candidates are a netem limit (tail drop), or fq_codel as the rate netem's child, as `scenario_child_qdisc` does, if the tunnel's queue is AQM-managed.
  - It is one Q_t per rate, pinned with its measured B_t and the qdisc that produced it.
- **Bottleneck drops per event** are a readout (§7.1). They tell how much of c_H is the release burst overflowing the queue, as opposed to idle time.

### 4.4 Warm-up, the standing-level witness and the two sides of a hold

- **Warm-up ends on a witness, not a clock.** A sampler (recorded PID, in every arm, so its cost is matched) reads `tc -s qdisc` backlog on `cli0` every 10 ms.
  - Warm-up ends when the backlog's 1 s median first reaches **0.75·B**, the lower edge of the emulation's ± 25 % tolerance (§4.3). B is B_t at Q_t and the limit at Q_s.
  - Waiting for the full B would read NO_FILL on most reps, because a CUBIC backlog oscillates below its peak.
  - A rep that has not reached 0.75·B by 60 s is `NO_FILL`: excluded, counted and reported.
- **Per-event validity: the queue is at its standing level.** An event is **valid** iff the backlog sample just before t_e lies in [0.75·B, 1.25·B]. Otherwise it is **OFF-LEVEL**: reported, and excluded from every scored number.
  - The same rule applies to REL events, HOLD events and CLEAN pseudo-events, so the comparison stays matched.
  - This replaces v3's per-event rule "backlog ≥ Δt·g" (FILLED). That rule kept only holds the queue could cover, where c_H is 0 by construction. It made every hold beyond the standing delay unscoreable, and those are the holds the rule makes when the standing queue is shallow.
- **Two sides of a hold, both scored.** With τ_q = B/g the standing delay (measured: Q_t's from R, Q_s's from its witness), each hold arm is on one side:
  - **within**, Δt ≤ τ_q: the queue covers the hold, and c_H is predicted ≈ 0;
  - **beyond**, Δt > τ_q: the link idles for about Δt − τ_q, and c_H is predicted ≈ (Δt·g − B)·MSS/S.

  Each side gets its own §9.1 verdict line per preset and queue. The two are never pooled.
- **Which side the rule's holds fall on, stated now.** At N = 1 a hold lasts min(H_o, R), with R the fill time: τ_k = d + (k − 1)·25 ms. At the predicted ω₁ ≈ 1.2 (§9.6), H_o = ω·d/δ gives the following, for d ∈ [1, 11] ms:

  | preset | H_o | holds the rule makes | if τ_q ≈ 5 ms (an fq_codel TUN queue) | if τ_q ≥ 61 ms (a deep tail-drop queue) |
  |---|---|---|---|---|
  | Bulk | 240·d ≥ 240 ms | τ₁, τ₂, τ₃ = 1–11, 26–36, 51–61 ms | round 1 within iff d ≤ 5 ms; rounds 2–3 beyond | all within |
  | Auto | 2.4·d = 2.4–26 ms | d for round-1 fills, else capped at 2.4·d | within iff the hold ≤ 5 ms: round 1 at d ≤ 5 ms, a capped hold at d ≤ 2.1 ms; otherwise beyond | all within |
  | Realtime | 0.024·d ≤ 0.26 ms | ≤ 0.26 ms | within | within |

  - The amendment recomputes this table with the measured τ_q and batch 1's measured d, before scoring.
  - §9.1 reads, per preset, only the arms whose Δt lies in that preset's hold set: Bulk [1, 61] ms, Auto [1, 26] ms, Realtime ≤ 0.35 ms.
- **First events reach a built queue.** A rep is F = 8 long-lived connections started together (iperf3 `-P 8`, declared choice 3). They build the standing queue during warm-up, and the reorderer is switched on only after the witness.
- **Event ranks.**
  - **REL:** a release disorders one flow, the flow of the delayed packet. Its rank is that flow's count of REL events so far: rank 1 gives ω₁'s c_R1, ranks ≥ 2 give ω_∞'s c_R∞.
  - **HOLD:** a hold stalls all eight flows and disorders none, so no flow's RACK reordering state changes. A hold therefore has no per-flow "first". c_H uses every valid HOLD event, whatever its rank, for both ω₁ and ω_∞.
  - **Check for HOLD:** the first hold of a rep is compared with the later ones. If they differ beyond their CIs, `HOLD-RANK-DEPENDENT` is reported, and ω₁ then uses the rep's first holds only.
  - **The minimum (§7.2)** is ≥ 32 valid rank-1 REL events, and ≥ 32 valid HOLD events of any rank, per arm.
- **Independence, stated.** The events of one rep share one queue and eight flows. The bootstrap and the n formula resample reps (clusters), never single events. Aggregate D_e over all flows is the consumer's cost, since the TUN carries all flows.
- **Displacement per flow.** With F = 8, a release with k aggregate overtakers gives its flow about k/8, so the aggregate k arms are mild for the flow. §6.1 therefore adds **per-flow arms** with Δt = (k_f + ½)·F/g, for per-flow k_f = 3 and 8. Cells are labelled by per-flow k (`k_f`) where they target it, and by aggregate k (`k_a`) otherwise. Realised per-flow k is reported for every REL event.

## 5. Inner stacks: D first, C after D's result

Set by `sysctl -w` inside `rp-cli` and `rp-srv` only, and read back as the arm's echo (rule 15). A mismatch is ABORT-SYSCTL. `uname -r` and `lscpu` head the log (rule 9).

| `net.ipv4.` | **D**: Linux defaults (phase D, scored) | **C**: classic, the conservative bound (phase C, after D) |
|---|---|---|
| `tcp_congestion_control` | cubic | cubic |
| `tcp_recovery` | 1 (RACK-TLP) | 0 |
| `tcp_reordering` / `tcp_max_reordering` | 3 / 300 | 3 / 3 (dupthresh pinned) |
| `tcp_sack` / `tcp_dsack` | 1 / 1 | 1 / 0 |
| `tcp_timestamps` / `tcp_frto` | 1 / 2 | 0 / 0 |
| `tcp_early_retrans` / `tcp_ecn` | 3 / 2 | 3 / 2 |
| `tcp_no_metrics_save` | **1** | **1** |

- **No carry-over between reps.** `tcp_no_metrics_save = 1` and `ip tcp_metrics flush all` run before every rep, in both namespaces, and every rep is a fresh connection. Otherwise cached reordering and ssthresh leak from one rep to the next.
- **D is the shipped default in every other row,** set explicitly and read back.
- **C** is RFC 5681 + RFC 6675 with no undo. It is measured only after D's result is read, under an amendment that may cut it.
- **BBR** (D with `bbr`) runs in phase C, and only if `bbr` is already available.

## 6. Cells (phase D)

### 6.1 Grid

W = g·RTT in 1 500 B packets. Δt in ms: k targets use (k + ½)/g, time targets c × RTT/4.

| g /s (Mbit/s) | W at 10 / 50 ms | k = 1 | 2 | 3 | 8 | 32 | t = 0.25 | 1 | 4 |
|---|---|---|---|---|---|---|---|---|---|
| 1 000 (12) | 10 / 50 | 1.5 | 2.5 | 3.5 | 8.5 | 32.5 | 0.625 / 3.125 | 2.5 / 12.5 | 10 / 50 |
| 10 000 (120) | 100 / 500 | 0.15 | 0.25 | 0.35 | 0.85 | 3.25 | same | same | same |

- **Degenerate:** (1 000, RTT 10, t = 0.25) gives Δt·g < 1 and is not run.
- **Operating range:** Δt ≤ 61 ms (three rounds at d = 11 ms, `ordering-hold.md` §4.2).
- **Hold lengths around the standing queue (new).** The hold cost starts where Δt·g passes the standing backlog. Each configuration adds Δt ∈ {0.5, 1, 2} × τ_q, with τ_q = B/g its measured standing delay: Q_t's from R, and Q_s's from its witness.
  - Those above 61 ms are **measured but out of range.** They are reported with §9.1's comparison, and excluded from the ω envelope of §7.4.
  - If τ_q·g is below 1 packet, that triple collapses and is not run.
- **Per-flow arms (new):** k_f = 3 and 8 at F = 8, Δt = (k_f + ½)·8/g: 28 and 68 ms at 1 000 /s, 2.8 and 6.8 ms at 10 000 /s. The 68 ms arm is measured but out of range.
- **Rule-hold arms (new):** Δt ∈ {11, 36, 61} ms, the three fill rounds at d = 11 ms (§4.4). Holds at d = 1 ms are covered by the grid's 1–3.5 ms arms.
- **The aggregate k columns above are k_a.**
- **100 /s is deferred to phase C** (sweep S1). At RTT 10 it is not run at all (W = 1).

### 6.2 Core D (scored)

Stack D × g {1 000, 10 000} × RTT {10, 50} × Q {Q_t, Q_s} × F = 8: eight configurations, n = 8 to 24 reps per arm (§7.2).
- **Q_t:** CLEAN, plus REL and HOLD at 16 Δt: k_a ∈ {1, 2, 3, 8, 32}, t ∈ {0.25, 1, 4}, the three τ_q arms, k_f ∈ {3, 8} and the three rule-hold arms. The degenerate t = 0.25 cell drops out at (1 000, RTT 10). That is 31 + 33 + 33 + 33 = **130 cells**.
- **Q_s:** CLEAN, plus REL and HOLD at 12 Δt: k_a ∈ {3, 8}, t ∈ {1, 4}, the τ_q arms, k_f ∈ {3, 8} and the rule-hold arms. That is 25 per configuration, **100 cells**.
- **Sides.** Each hold arm is labelled within or beyond (§4.4) from its configuration's measured τ_q, in the amendment.
- **Event spacing.** About 1 s in aggregate. For arms whose A_e exceeds that, spacing is 3 s where the table can represent it, and otherwise the table's minimum p, with A_e cut at the next event (§11.1).

### 6.3 Phase C (registered in outline; budgeted by an amendment after D is read)

- **C on the Q_t core,** on a 9-arm subset.
- **Sweeps S1–S3:**
  - **S1:** 100 /s, RTT 50, k ∈ {1, 3, 8}.
  - **S2:** F = 1, a lone flow (with its own warm-up witness), at 10 000 /s, RTT 50, k ∈ {3, 8} and t = 1.
  - **S3:** event spacing 1 s against 4 s for ω_∞, at 1 000 /s, RTT 50, k = 8. It runs at 1 000 /s because 4 s at 10 000 /s needs p below the table's resolution (§11.1).
- **BBR.**
- **Estimate:** ≈ 8 h. D's result may make parts moot, for example C if `RENT-UNDERPRICED` fires everywhere.

### 6.4 VOID and witness rules: §7.2

### 6.5 Outage check (one small stage, a check of the owner's acceptance)

- **Cells.** D, 10 000 /s, RTT 50, Q_t: CLEAN, OUT-REL and OUT-HOLD, n = 8, one outage per rep after the warm-up witness. That is **3 cells**.
- **Why so small.** The owner accepted late out-of-order delivery after long outages (2026-11-21). This stage only checks that the inner TCP agrees (§9.4).

### 6.6 Stage T: the tunnel in the path (reported)

- **Topology.** `tun_bulk.sh`'s bring-up on the `clean` scenario (no loss, no jitter, so no holes of the tunnel's own), hint `auto`. `rp-app` sits behind `rp-srv` on a veth. The reorderer is on `rp-srv`'s egress to `rp-app`, after TUN delivery, and iperf3 runs from `rp-cli` to `rp-app`.
- **Stack D, F = 8, after the warm-up witness:** CLEAN, and REL and HOLD at t ∈ {1, 4}, with Δt from the inner min_RTT measured in R. That is **5 cells**.
- **Witnesses.** TUNPIPE, TUNMTU, TUNSHA, and the qlen of `rpcli0` (the TUN queue of §4.3). The pre-reorderer pcap shows zero reordering in CLEAN.

**Phase D cells: 130 + 100 + 3 + 5 = 238.**

## 7. Readout and statistics

### 7.1 Per rep

- **Traffic.** iperf3 `-P 8 -J --get-server-output -i 1`, fresh connections per rep, running through warm-up (until the witness) plus a 12 s event phase.
- **Truth: pcaps.** tcpdump, headers only (`-s 96`), on `mid0` ingress (the bottleneck's output, before the reorderer) and on `srv0` ingress (after it), in every arm. A pure digest matches packets by (flow, TCP seq, IP ID).
  - **REL event:** a packet delayed by more than Δt/2.
  - **HOLD event:** the head of a stall, that is, a delayed packet whose predecessor at `mid0` was not delayed. The successors queued behind it are part of its stall, not events.
  - **Event index:** the event's rank within its connection.
  - **D_e:** idle slots `⌊|A_e|·g⌋ − packets seen in A_e`, plus retransmitted segments, all from `mid0`.
  - **Drops per event (new readout):** a retransmission in A_e whose original never reached `mid0` is a **bottleneck drop**. One whose original did reach it is **spurious**. Both are reported per event, by arm and by index. The pcap is taken before the reorderer, so reordering is never mistaken for retransmission.
  - **CLEAN pseudo-events** are seeded random instants at REL's event rate.
  - **Realised k:** the overtakers of each REL event, reported.
  - pcaps are digested between reps. Only digests are kept, plus rep 1's raw pcaps per cell.
- **Counters,** per rep, inside `rp-cli` and `rp-srv` (`/proc/net/netstat`, `/proc/net/snmp`, diffed):
  - `TcpRetransSegs`;
  - `TCPDSACKRecv` and `TCPDSACKOldSent`;
  - `TCPSackRecovery` and `TCPTimeouts`;
  - the undos `TCPFullUndo`, `TCPPartialUndo`, `TCPDSACKUndo` and `TCPLossUndo`;
  - `TCPSACKReorder` and `TCPSpuriousRTOs`;
  - `TCPLossProbes`.

  R0 checks the names; a missing one is printed `-`.
- **Socket and bottleneck.**
  - `ss -tin` in `rp-cli` at the witness instant and near the end: cwnd, `reord_seen`, `dsack_dups`, srtt, minrtt. srtt − minrtt cross-checks the backlog.
  - `tc -s qdisc` on `cli0`, sampled every 10 ms (§4.4): sent, drops, backlog, utilisation.
  - **Each event carries its valid/OFF-LEVEL label and its rank.** Each hold arm carries its within/beyond side.

### 7.2 n, seeds, order, witnesses

- **Reps and samples.** Each rep's 12 s event phase gives about 12 events at about 1 s aggregate spacing.
  - In a REL arm, about 6 of them are a flow's first release (a coupon-collector draw over 8 flows) and about 6 are later releases.
  - In a HOLD arm, all ≈ 12 count (§4.4).
- **n, with the clustering design effect explicit.** Events within a rep are correlated, so the unit of the variance is the rep, not the event. Let s²_R,rep and s²_H,rep be the between-rep variances of the per-rep mean D_e (in symbols) of the rank-1 REL events and of the HOLD events. Their design effect relative to independent events is

  ```text
     DE  =  1 + (m − 1)·ICC          with m the events per rep and ICC the intraclass correlation
  ```

  It is already contained in these variances, since `s²_rep = σ²_event·DE / m`. Then:

  ```text
     n  =  ⌈ 1.96² · ( s²_R,rep + s²_H,rep ) / h² ⌉ ,     h = 0.3 · max( ω₁,pred , 1.2 ) ,     clamped to [8, 24] reps per arm
  ```

  - **Small-sample guard.** Each s²_rep is taken at the upper 80 % χ² bound for its R degrees of freedom, so a lucky low estimate cannot under-size n.
  - DE and ICC are reported per configuration. Nothing divides by events per rep as if they were independent.
  - A configuration that needs more than 24 is `POWER-LIMITED` in the amendment, before scoring.
- **Seeds.** netem seed 42 for the first half of each arm's reps, 7 for the second (rule 4), with per-half medians reported. If R0 finds `seed` unsupported, the halves are rep halves under an unseeded RNG, recorded as a deviation; no cell is skipped.
- **Interleaving (rule 3).** Each round runs every arm of a configuration once, in a seeded shuffle. All arms of a configuration run in one session, on one bring-up.
- **Live minimum.** A cell needs ≥ 6 live reps (NO_FILL excluded), and per arm ≥ 32 valid rank-1 REL events (REL arms) or ≥ 32 valid HOLD events (HOLD arms), else `UNSCOREABLE`. NO_DATA is retried once at the end of the configuration and recorded (rule 8).
- **VOID-WITNESS (a rep).**
  - A REL or HOLD rep with fewer than 95 % of its events delayed within Δt ± max(50 µs, 5 %).
  - A HOLD rep with any overtaker.
  - A CLEAN rep with any delayed packet.
  - A sysctl readback that changed during the rep.
  - **Realised k is not a witness.** After a cut the queue empties and fewer successors overtake, so a k rule would void exactly the reps where the effect is largest. The k check lives in R0, on a paced probe.

### 7.3 Estimates, intervals, power

- **CIs.** Each of c_R and c_H (and ω, as their difference) gets a percentile bootstrap CI: 10 000 resamples, connections resampled within each arm.
- **A/A floor.** CLEAN's first half against its second, through the same estimator. An effect is claimed only if its CI excludes 0 and it exceeds the floor (rule 5).
- **MDEs** are pinned per configuration in the amendment:
  - slot estimator: `MDE ≈ 2.8·σ̂·√(2/n)·MSS/S`;
  - goodput guard: `MDE_G ≈ 2.8·σ_G·√(2/n)/(E·S)`, illustratively 17 symbols at 1 000 /s and 170 at 10 000 /s for σ_G = 1 %, so the guard only catches large, long costs.
- **Never zero by default.** A cell whose interval crosses more than one zone edge of §8 is `NEEDS-MORE-POWER`, never ω = 0.

### 7.4 From curves to numbers (applied to the Q_t core for the zones; Q_s feeds §9.1)

1. **ω₁ per configuration:** the envelope of the bounds over Δt ≤ 61 ms. The interval is [max lo, max hi], with each arm's bounds at the Bonferroni level 1 − 0.05/m. A per-hole price must not undercharge any hole the rule can release, and one noisy arm cannot lift the envelope.
2. **`OMEGA-VARIES-X`:** two arms of one configuration have ω₁ intervals in different zones, with disjoint CIs. The purchase price then depends on when the rule buys, and the question goes back to `ordering-hold.md` §3.
3. **`OMEGA-VARIES-RATE`, `-RTT` or `-Q`:** the same test across configurations, named by the factor that separates them. ω is then not a constant, and no constant is picked.
4. **Otherwise**, D's ω₁ (and ω_∞) is the envelope of the Q_t configurations' intervals. That is the number `ordering-hold.md` uses, subject to §9.1.

## 8. Where the presets sit (the steep parts)

At N = 1, round k's fill comes at τ_k = d + (k − 1)·25 ms, with τ₁ ≈ d (`ordering-hold.md` §4.1). H_o = ω·d/δ crosses it at

```text
   ω*_{p,k}  =  δ_p · τ_k / d            (preset p releases before round k's fill iff ω < ω*_{p,k});    at N = 2:  ω* = δ_p · Δ / d
```

| preset (δ) | round 1 | round 2 (d = 11 / 1 ms) | round 3 | skew Δ = 20 ms | band over d ∈ [1, 11] ms |
|---|---|---|---|---|---|
| Bulk 0.005 | 0.005 | 0.016 / 0.13 | 0.028 / 0.26 | 0.009 / 0.1 | **Z_B = [0.005, 0.26]** |
| Auto 0.5 | 0.5 | 1.6 / 13 | 2.8 / 26 | 0.91 / 10 | **Z_A = [0.5, 26]** |
| Realtime 50 | 50 | 164 / 1 300 | 277 / 2 550 | 91 / 1 000 | **Z_R = [50, 2 600]** |

- **Steep part.** At fixed d, a ±2 % δ nudge changes rbf materially only within

  ```text
     band_{p,k}  =  ω*_{p,k} · [ 1/(1 + b_k) , 1 + b_k ] ,   b_k = max( 0.02 , 2·σ_R/τ_k )
  ```

  σ_R comes from batch 1's `[LAT]` where it exists, else the toy's 4 ms. (d, τ) are batch 1's measured pairs, else 11 ms and 0.78 ms.
- **`STEEP-AT-<preset>`** fires iff ω₁'s interval meets a round band, or ω_∞'s interval meets the skew band.
- **The recommendation then** is the toy's P3 nudge check with the measured σ_R and ω. If P3 fails, a revised law, or the owner's larger declared ω that clears the band (never a value inside a band). Never a clamp or a preset-keyed tweak.
- **The gaps between bands** are (0.26, 0.5) and (26, 50), about ×1.9 each.

## 9. Outcome set

Verdict terms follow `docs/measurement-discipline.md`. Nothing outside this set is recorded as a verdict.

### 9.1 First: is the stall priced? (the §2 finding; per preset p, per queue level)

Compare c_H(Δt) with J_U's rent `δ_p·Δt/d`, at the measured d, on valid events, at the Δt in preset p's hold set (§4.4). **Each preset gets two verdict lines per queue level, one for the within side and one for the beyond side.** Each line is one of the outcomes below. An empty side (no arm of the hold set on it) reads `NO-ARMS`, and that it is empty is itself reported.

| outcome | condition | meaning |
|---|---|---|
| `RENT-UNDERPRICED-<p>-<side>` at Q_t | c_H's lower CI > δ_p·Δt/d at some Δt of that side in p's hold set (Bulk: the count rule below) | **The finding fires for the tunnel's own queue.** The rule, not only ω, goes back to `ordering-hold.md` §2–§3, and the owner is told before any law is adopted |
| `RENT-UNDERPRICED-<p>-<side>` at Q_s only | the same, at Q_s but not at Q_t | the finding holds for inner paths with a shallow bottleneck elsewhere; reported to the derivation as a scope question; the zones below still apply for the tunnel's own queue |
| `RENT-COVERED-<p>-<side>` | c_H's upper CI ≤ δ_p·Δt/d at every Δt of that side in p's hold set, at Q_t | J_U prices the hold adequately at p; §9.2 applies at p |
| `RENT-UNDECIDED-<p>-<side>` | neither | `NEEDS-MORE-POWER` at p |
| `OMEGA-NEGATIVE` | ω₁'s upper CI < 0 at some Δt | holding costs more than releasing outright; implies `RENT-UNDERPRICED` at every preset at that queue |

**Bulk is scored as a count, not a mean.** Bulk's rent, `δ_B·Δt/d`, is at most 0.03 symbols for Δt ≤ 61 ms at d = 11 ms, which no mean can resolve.
- A valid HOLD event is **free** iff A_e shows zero idle slots and zero bottleneck drops. The count is kept per side. CLEAN pseudo-events give the baseline non-free share.
- Let x be the excess non-free share (HOLD − CLEAN), with its 95 % Newcombe interval over clusters, and c̄_nf the mean D_e of a non-free HOLD event, with its bootstrap CI, in symbols.
- **`RENT-COVERED-Bulk`** iff `upper(x) · upper(c̄_nf) ≤ δ_B·Δt/d` at every scored Δt ≤ 61 ms.
- **`RENT-UNDERPRICED-Bulk`** iff `lower(x) · lower(c̄_nf) > δ_B·Δt/d` at some Δt.
- **`RENT-UNDECIDED-Bulk`** otherwise.
- **Predicted at Q_t:** x = 0 on the within side, so covered, provided ≥ 300 valid HOLD events per Δt (then a zero count puts upper(x) ≈ 1 %). On the beyond side every hold idles the link, so x ≈ 1 and `RENT-UNDERPRICED-Bulk-beyond` is predicted. The amendment checks that n reaches this, and otherwise declares Bulk `POWER-LIMITED`.

### 9.2 Then: where ω₁ sits (stack D, Q_t; read at presets with RENT-COVERED)

| outcome | ω₁ interval | Bulk | Auto | Realtime |
|---|---|---|---|---|
| `OMEGA-Z0` | upper < 0.005 | releases before the first repair. **Default change** | the same. **Default change** | the same (as today) |
| `OMEGA-ZB` | inside Z_B | `STEEP-AT-Bulk` | releases at detect. **Default change** | releases at detect |
| `OMEGA-GAP-BA` | inside (0.26, 0.5) | keeps order | releases before the first repair. **Default change** | releases at detect |
| `OMEGA-ZA` | inside Z_A | keeps order | `STEEP-AT-Auto` at some d; below the measured d's edges Auto reorders second-round holes. **Default change at that d** | releases before the first repair |
| `OMEGA-GAP-AR` | inside (26, 50) | keeps order | keeps order | releases before the first repair (today ≈ SRTT/2) |
| `OMEGA-ZR` | inside Z_R | keeps order | keeps order | `STEEP-AT-Realtime` |
| `OMEGA-Z6` | lower > 2 600 | keeps order | keeps order | holds through the first repair, longer than today. **Default change** |
| `OMEGA-EDGE-<e>` | contains exactly one zone boundary e | both adjacent rows are reported: decided where they agree, `NEEDS-MORE-POWER` at a preset where they differ | | |
| `NEEDS-MORE-POWER` | contains more than one boundary | undecided; the close names more connections for that configuration | | |
| `OMEGA-VARIES-<factor>` | §7.4 items 2–3 | back to `ordering-hold.md`; no constant | | |
| `UNSCOREABLE` | fewer than 3 of the 4 Q_t configurations scored | nothing is read | | |

**At N = 2 with skew (ω_∞).** Preset p keeps skewed packets in order iff ω_∞ ≥ δ_p·Δ/d at the measured (d, Δ). Otherwise every slow-path packet is reordered: a default change at Bulk or Auto, and the C1 consequence at Realtime.

Each "Default change" is the owner's call (`ordering-hold.md` §11 Q1): accept it, or declare a larger ω that clears the band.

### 9.3 The measurement itself

- **`DELIVERED`:** R0, the smoke and the witnesses hold, and §9.1 and §9.2 each name an outcome.
- **`VOID`:** a stage whose witness fails at ≥ 25 % of its reps.
- **`TRUNCATED`:** the stopper fired; scored at the n reached.

### 9.4 Outage check

- **`OUT-RELEASE-WORSE`:** OUT-REL costs more than OUT-HOLD, CIs disjoint. **Back to the owner**: the inner TCP contradicts the 2026-11-21 acceptance.
- **`OUT-RELEASE-BETTER`** or **`OUT-NULL`:** confirms the acceptance.

### 9.5 Stage T (on counts, which have the power)

Compared with Q_t core (D, 10 000, RTT 10) at the same t:
- **`T-CONFIRMS`** iff, per first event, the excess spurious retransmissions (REL − CLEAN) agree within ± 0.5, the excess bottleneck drops agree within ± 0.5, and the idle-slot share of c_H agrees in sign (decision bars, not law constants).
- **`T-DIVERGES`** otherwise. Option (ii) is then reconsidered (§3).

### 9.6 Predictions (rule 11), from a re-read of `ordering-hold.md` §1–§2 and RFC 8985 §6.2

- **D at Q_t, valid events.**
  - c_H ≈ 0 for Δt < τ_q, growing as (Δt·g − B_t)⁺ above it (the τ_q arms show the knee).
  - If B_t turns out shallow (for example an fq_codel TUN queue near 5 ms), Q_t behaves like Q_s, and the finding fires for the tunnel itself. That is a real result, not an artefact, because the witness holds.
  - c_R1 ≈ one spurious retransmission plus an undone reduction. A standing backlog B_t ≥ (1 − β)·W absorbs the reduction's dip, so c_R1 ≈ 1.2 symbols; a shallower B_t adds ((1 − β)·W − B_t)⁺ slots.
  - So ω₁ ≈ 1.2: **`RENT-COVERED-<p>-within` at Auto and Realtime** (Bulk `COVERED` or `UNDECIDED`), **`RENT-UNDERPRICED-<p>-beyond` at Bulk and Auto** wherever τ_q is shallow enough for the rule's holds to fall beyond it (§4.4), and **`OMEGA-ZA`** with `STEEP-AT-Auto` at some d. Bulk keeps order, and Auto reorders holes that need a second round.
  - ω_∞ < 0.5: at N = 2 with skew, Auto reorders every slow-path packet.
- **D at Q_s.** c_H grows as (Δt·g − Q_s)⁺ (§2's table).
  - **`RENT-UNDERPRICED` at every preset in the cells with Δt·g ≫ Q_s** (t = 4, and k = 32 at 1 000 /s), and `OMEGA-NEGATIVE` there.
  - That is the finding for inner paths with a shallow bottleneck elsewhere. It is not predicted to fire at Q_t.
- **Refuters of the measurement, not the law.**
  - If c_H on valid within-side events is materially above 0, the hold triggers losses or the witness misreads the queue. The drops-per-event readout and the srtt − minrtt cross-check decide which, before anything is read.
  - If ω₁'s lower bound exceeds 26, the RACK expectation is contradicted, and the sysctl and `reord_seen` witnesses are checked first.

## 10. Abort classes (filled in the Result, in this order)

| token | cause |
|---|---|
| ABORT-LOCK | a lock exists at `test -e`, or `take_lock` refuses |
| ABORT-CRLF | a CR byte in `tools/l1/lib.sh` or any new script (rule 10) |
| ABORT-TOOLS | iperf3, tcpdump, ethtool or `ss` missing, or netem tables cannot be loaded (`plug` missing only skips §6.5) |
| ABORT-BENCH | R0 refutes a reorderer property; no battery is launched |
| ABORT-SYSCTL | a §5 sysctl is not per namespace (pre-flight), or a readback differs |
| ABORT-NS | `guard_ns` refused a name |
| ABORT-SMOKE | a witness is missing on either end |
| ABORT-BUILD | stage T: the raptorpath sha differs from the amendment's pin |
| TUN-BRINGUP-FAIL | stage T only: three attempts exhausted. Stage T is skipped, not the session |
| ABORT-DISK | free space below the session's budget at launch |
| ABORT-HOSTSTATE | a root-namespace `tcp_*` sysctl differs from pre-flight |
| a non-zero stage exit | |

## 11. Rehearsal, component bench and harness

### 11.1 Session R (≈ 2 h): R0, noise calibration, smoke

**R0, the reorderer alone (rule 14).** A paced, sequence-numbered UDP probe at 1 000 and 10 000 /s, with pre and post pcaps. It must show, per Δt and per mode:
1. selected packets delayed Δt ± max(50 µs, 5 %);
2. REL: unselected packets undelayed (< 50 µs), and ≥ 90 % of events with exactly k overtakers (k ± 1 below 0.3 ms);
3. HOLD: zero overtakers, and the successors released within 1.1 × their 10 × g serialisation;
4. a selection rate inside the binomial 99 % interval of p;
5. the table switch at the warm-up witness working, with no event before it;
6. CLEAN undelayed.

**R0 tool checks, each with its fallback fixed now (skip, never improvise):**
- **netem `seed`:** does the VM's tc and kernel accept it, and does the same seed reproduce the same selection sequence on the probe? If not, rep halves stand in for seed halves (§7.2), recorded.
- **Distribution table size:** load tables of growing size and record the largest accepted, L (Fable recalls 16 384).
  - p resolution is 1/L. At L = 16 384, 1 s spacing at 10 000 /s needs p = 1.0·10⁻⁴, which becomes 2 entries (0.82 s, used). 1 s at 1 000 /s becomes 16 entries (1.02 s).
  - 4 s at 10 000 /s needs 2.5·10⁻⁵ < 1/L, so it is not representable, which is why S3 runs at 1 000 /s.
  - Any cell needing p < 1/L is `SKIPPED-TABLE`.
- **Consecutive per-socket IP IDs** with a TCP flow. If not, the outage check is `SKIPPED-SELECTOR`.
- **`plug`** holds and releases as a burst. If not, the outage check is `SKIPPED-PLUG`.
- **Every §7.1 counter name exists.** A missing one is printed `-`.
- **The TUN qlen and qdisc** of a raptorpath TUN, in a stage-T bring-up (inputs to Q_t's provenance above).

**Standing queue through the real tunnel (Q_t's provenance).**
- Setup: `tun_bulk.sh`'s bring-up on `clean`, hint `auto`, D sysctls, one long-lived inner CUBIC flow for 90 s, then F = 8 for 90 s.
- Record:
  - `ss -tin` srtt − minrtt every 100 ms;
  - inner goodput, against the tunnel's ceiling (the bottleneck check of §4.3);
  - `tc -s qdisc` backlog on `rpcli0` and `cli0`;
  - `net.core.default_qdisc` and the TUN qlen.
- **Result:** B_t = g_tun·median(srtt − minrtt) over the last 60 s, and where it sits. The amendment pins it, the emulating qdisc and limit (§4.3), and τ_q.
- If inner goodput stays below 90 % of the ceiling, the tunnel is not the bottleneck in `clean`. Q_t is then measured with an underlay rate set below the tunnel's ceiling, a new scenario named in the amendment.

**Noise calibration, on REL and HOLD reps, not only CLEAN.** For each of the 8 core configurations, 6 reps each of CLEAN, REL at k_f = 8, HOLD at k_f = 8 and HOLD at the configuration's longest within-side Δt.
- They give s²_R,rep and s²_H,rep, DE and ICC (§7.2), the fill time, and the OFF-LEVEL share.
- Those set n, the MDEs and the budget. Nothing in it is scored.

**Smoke.** One rep of every arm type on both ends, with every §7.1 readout and echo present.

**The amendment** (committed before session D1) pins:
- per-rep costs, measured fill times and the re-budget;
- n and the MDEs per configuration, and any `POWER-LIMITED` declarations;
- B_t, its location, the emulating qdisc and limit, and τ_q per configuration;
- stage T's min_RTT and raptorpath sha;
- L and `seed` support;
- any cut.

### 11.2 Harness (a Sonnet worker, tools only, before session R; red tests first)

Plan token `omega`, in the `*_run_all.sh` / `*_battery.sh` convention of `threadq2_run_all.sh`, all in `tools/l1`:

| file | role |
|---|---|
| `topo_reorder.sh` | namespaces up/down/status, `guard_ns` on every name, offloads off, per-namespace sysctls with readback and the pre-flight presence check |
| `omega_dist.py` | the two-point and zero netem tables |
| `omega_cells.py` | pure: the design of §6 as data (Δt incl. τ_q arms, Q, p quantised to L, n, the seeded order) |
| `omega_battery.sh` | one configuration: the backlog sampler, warm-up until the witness at 0.75·B, the event phase; kill by PID |
| `omega_run_all.sh` | the session envelope: locks, sentinels proven writable at launch, CR check, host-state snapshots, R0, smoke, stopper, exit check |
| `omega_parse.py` | pure: pcap digest, events, D_e, drops versus spurious, counter diffs, iperf3 window |
| `omega_score.py` | pure: c_R, c_H, ω, CIs, MDE, n, `WINDOW-SHORT`, envelope, VARIES, §8 bands, §9 outcomes |
| `test_omega.py` | unit and static tests |

- **Unit tests,** red first:
  - known delays give the right events and overtakers;
  - a HOLD stall of one head and 20 queued packets is one event;
  - D_e counts idle slots and retransmissions exactly;
  - a retransmission whose original never reached `mid0` is a drop, and one whose original did is spurious;
  - the event index splits first from later events;
  - the n formula and its clamp;
  - the witness ends warm-up at 0.75·B, and NO_FILL at 60 s;
  - an event outside [0.75·B, 1.25·B] is OFF-LEVEL and excluded;
  - the within/beyond side, and its two separate verdict lines;
  - REL rank per flow; HOLD events unranked, with the `HOLD-RANK-DEPENDENT` check;
  - per-flow k_f arms get Δt = (k_f + ½)·F/g;
  - the n formula from between-rep variances, with the χ² guard;
  - the cluster bootstrap;
  - the Bulk count rule's three outcomes;
  - `RENT-UNDERPRICED` against `RENT-COVERED` at the rent line ± 1 %;
  - `OMEGA-NEGATIVE`;
  - the §9.2 edges ± 1 % and `OMEGA-EDGE`;
  - one noisy arm does not lift the envelope;
  - `WINDOW-SHORT`;
  - a cell under 6 live reps, or under 32 valid rank-1 REL or 32 valid HOLD events, is `UNSCOREABLE`.
- **Stub paths** in `harness_paths.HARNESS_PATHS`, executed by `test_stubrun.py`, with `ip`, `tc`, `tcpdump`, `iperf3`, `ss`, `ethtool` and `sysctl` stubbed:
  - `rehearse` and `full`;
  - `lock-held` → ABORT-LOCK, with no stub command issued;
  - `sysctl-not-per-ns` → ABORT-SYSCTL, nothing written;
  - `sysctl-mismatch` → ABORT-SYSCTL;
  - `bench-refutes` → ABORT-BENCH;
  - `no-seed` → rep halves recorded;
  - `table-too-small` → SKIPPED-TABLE;
  - `rel-no-events` and `hold-reorders` → VOID-WITNESS;
  - `under-min` → UNSCOREABLE;
  - `ids-not-consecutive` → SKIPPED-SELECTOR;
  - `rent-underpriced`;
  - `omega-negative`;
  - `one-edge` → OMEGA-EDGE;
  - `wide-ci` → NEEDS-MORE-POWER;
  - `window-short`;
  - `power-limited` (n clamped to 24);
  - `no-fill` → NO_FILL rep excluded;
  - `off-level` → OFF-LEVEL events excluded;
  - `beyond-side` → a beyond-side verdict line next to the within line;
  - `hold-rank` → HOLD-RANK-DEPENDENT;
  - `stopper` → TRUNCATED;
  - `bad-ns`;
  - `hoststate-changed` → ABORT-HOSTSTATE.
- **Static checks:**
  - every `ip`, `tc`, `ethtool` or `sysctl` call is wrapped in `ip netns exec rp-…` or `ip -n rp-…`;
  - no `modprobe`, `nft`, `iptables`, `pkill -f` or `killall`;
  - every `kill` takes a recorded PID.
- **Done when** `test_omega.py`, `test_stubrun.py` (FAST) and the existing tools tests pass, and `harness_paths` equals what ran. No engine file changes.

## 12. Sessions and VM time (phase D; phase C by a later amendment)

- **Rep length.** Warm-up to the witness, plus a 12 s event phase, plus ≈ 4 s for start, stop, snapshots and the digest. That is ≈ 41 s at Q_t at the worst case on paper (a tail-drop backlog of thousands of packets, ≈ 25 s of fill), and ≈ 19 s at Q_s. If R measures a shallow B_t, Q_t reps fall to about Q_s's.
- **Fixed parts:** session R ≈ 3 h (R0, the tunnel standing-queue measurement, calibration with 8 × 4 × 6 reps, smoke); outage check and stage T at n = 8, ≈ 0.8 h.

| n per arm | Q_t (130 cells) | Q_s (100 cells) | phase D total, deep B_t | phase D total, shallow B_t | battery sessions (≤ 4.5 h each) |
|---|---|---|---|---|---|
| 8 (the floor) | 11.8 h | 4.2 h | **≈ 20 h** | ≈ 14 h | 4 (deep) |
| 24 (the clamp) | 35.5 h | 12.7 h | **≈ 52 h** | ≈ 33 h | 11 (deep) |

**Cut order, pre-registered.** Applied in the amendment, in this order, until phase D fits; savings at n = 24 (deep / shallow):
1. Q_s keeps only k_f ∈ {3, 8}, t = 4 and the rule-hold arms (−56 cells): 7.1 h.
2. At Q_t, drop k_a ∈ {1, 2} and t ∈ {0.25, 1} (−30 cells): 8.2 / 3.8 h.
3. Drop configuration (1 000 /s, RTT 10) at both queues (W = 10, the least tunnel-like): 8.2 / 4.6 h.
4. Lower the clamp from 24 to 16 at Q_s: 1.4 h.

After cuts 1–4 the total is ≈ 27 h (deep) or ≈ 16 h (shallow) at the clamp.
- **Rule:** if phase D still exceeds 20 h after the cuts, it goes back to the owner before launch, with the per-configuration n that caused it.
- **Sessions are added, never squeezed.**
- **A detached stopper per session** ends at a balanced round boundary, or at launch + 4.9 h. It stops only its recorded PIDs and raptorpath by exact name, lets the driver tear down its namespaces, clears both locks and writes `TRUNCATED-<session>`.
- **No polling (rule 13).** The watcher waits on `DONE-ALL || FAILED-ALL || TRUNCATED-*`.
- **Exit state, verified:** no raptorpath; no recorded PID alive; no `rp-*` namespace; both locks released; root `tcp_*` sysctls equal to pre-flight.
- **Phase C** (C on the Q_t subset, S1–S3, BBR) is budgeted by its own amendment after D is read, ≈ 8 h estimated at n = 8.

## 13. What goes back to the owner (with the Result)

- **§9.1 per preset and queue.** Whether a hold costs the inner TCP more than the derivation charges for it. If `RENT-UNDERPRICED` fires at the tunnel's own queue, the rule is revised before anything is built.
- **D's ω₁ and ω_∞ intervals and zones,** with the default change each implies. Where a change is implied, the choice between accepting it and declaring a larger ω.
- **Any `STEEP-AT-<preset>`**, with the toy P3 check it triggers.
- **Any `OMEGA-VARIES-<factor>`** or `OMEGA-NEGATIVE`.
- **The outage check.** `OUT-RELEASE-WORSE` re-opens the 2026-11-21 acceptance.
- **Stage T's verdict.**
- **The phase C amendment.**
- **What stays blocked either way:** C9, C11 and C12's release arm wait on this result and on the toy (`ordering-hold.md` §10). R1 (ω = ∞) is unaffected.

## 14. Review

Fable reviewed the first draft. Its findings are addressed as follows:
- The HOLD arm was unpriced in the predictions. It is now predicted per cell (§2), and REL − CLEAN and HOLD − CLEAN are the primary scored curves, with ω derived from them. The stall-pricing question is written as a finding for `ordering-hold.md` (§2, §9.1), without editing its law.
- The shallow queue penalised HOLD. Q is now a core factor, at the tunnel's own queue (with provenance) and at a shallow queue, and bottleneck drops per event are a readout (§4.3, §7.1).
- ω₁'s power was too low. Short reps are now the rule, with n₁ sized from R's noise (§7.2).
- R0 now checks netem `seed` support and the distribution table's size, each with a fallback fixed in advance (§11.1).
- The outage stage is reduced to a check of the owner's acceptance (§6.5, §9.4).
- Stack D runs first, and C only after D's result is read (§5, §6.3, §12).
- The owner section now states the two declared choices and warns that the rule itself may change.

Fable checked v2:
- **A queue limit is not a standing queue.** Q_t now comes from the standing queue measured through the real tunnel (§4.3, §11.1). Warm-up ends on a witness, and every event is labelled and never pooled (§4.4; superseded in round 4). First events come from F = 8 flows that have built the queue, with the cluster bootstrap (§4.4, §7.2).
- **Bulk** is scored as a count (§9.1).
- **Hold lengths** now include 0.5, 1 and 2 × τ_q. Those above 61 ms are measured but out of range (§6.1).
- **The candidate rent** is written out, with what the engine could read online, as a question for `ordering-hold.md` (§2).

Fable read round 3:
- **The FILLED-only verdict skipped the holds the rule makes.** An event is now valid when the queue is at its standing level, and hold arms are split within and beyond the standing delay. Both sides are scored, with their own verdict lines, and the side of each preset's holds is stated in advance (§4.4, §9.1).
- **Per-flow arms** at k_f = 3 and 8, and the flow count as declared choice 3 (§4.4, §6.1).
- **HOLD events are unranked**, with a rank check, and the minimum is per arm type (§4.4, §7.2).
- **Warm-up** waits for the tolerance's lower edge, 0.75·B (§4.4).
- **n** is built from between-rep variances, so the design effect is explicit, and R calibrates on REL and HOLD reps (§7.2, §11.1).
- **VM time** is stated at both n = 8 and the n = 24 clamp, with a cut order (§12).

## 15. Draft amendment: how the harness reads the registration (Draft, not accepted)

Draft written with the harness (`tools/l1/omega_*`, branch `omega-harness`). Nothing here is adopted until the owner accepts it. Each row is one place where the harness had to interpret or extend v4. "Matches" = the code does what the text says; "extends" = the text is silent and the code adds a rule; "contradicts" = the code differs from the text on purpose. Line numbers refer to the harness commit named in the branch head.

| # | decision | file:line | vs. registration |
|---|---|---|---|
| 0 | **Truncation.** A `TRUNCATED-<session>` ledger is scored at the n it reached. The first line lists n_reached per cell, every result line carries `[TRUNCATED-<s> n_reached=min..max]`, the measurement is never `DELIVERED`, a cell under its live minimum stays UNSCOREABLE. A footerless or FAILED-ALL ledger is not scored. | `omega_score.py:452` | matches §9.3 (the first harness draft contradicted it, on a wrong brief) |
| 1 | A row missing a required event field (D, idle, retx, drops, valid, rank, rank_in_rep) is MISSING: void, counted against the 6-live-rep minimum, never read as 0. | `omega_score.py:275` | extends §7.2 |
| 2 | Witness fields (tc counters moved, qdiscs in force, sysctls held, queue at level) must be present and true, else VOID-WITNESS; a stage with at least 25 % such rows is VOID. | `omega_parse.py:286`, `omega_score.py` (stage loop) | matches §7.2, §9.3; "present" extends |
| 3 | MSS/S = 1448/1200 (a 1500 B packet with timestamps). | `omega_score.py:26` | extends (§1.2 says "about 1.2") |
| 4 | omega_inf also needs at least 32 valid events (rank >= 2 REL). | `omega_score.py:351` | extends (§7.2 states the minimum for rank-1 REL and HOLD only) |
| 5 | Envelope: [max lo, max hi] over arms with dt <= 61 ms, each arm at Bonferroni 0.05/m, m = arms in the envelope. | `omega_score.py:207` | matches §7.4 item 1; m extends |
| 6 | Zones closed at the upper edge, gaps open; exactly one boundary strictly inside = `OMEGA-EDGE-<e>`; more than one = NEEDS-MORE-POWER. 9.2 is read at a preset only if it has a RENT-COVERED line at Q_t, else NOT-READ. | `omega_score.py:134`, `omega_score.py:33` | matches §9.2; the reading condition extends "read at presets with RENT-COVERED" |
| 7 | `OMEGA-VARIES-DT` within a configuration; `-RATE`, `-RTT`, `-Q` for pairs of configurations differing in exactly one factor (intervals in different single zones, disjoint). | `omega_score.py:214` | extends (§7.4 item 2 has no name for the within-configuration case) |
| 8 | 9.1 verdict line per (queue level, preset, side), pooled over that level's configurations. UNDERPRICED needs lower CI > rent and c_H above the A/A floor; no floor, no claim (UNDECIDED). Bulk: Newcombe interval on the non-free share with n/DE; an arm with no non-free hold gets c_nf upper = largest D_e seen, lower = 0 (can be COVERED, never UNDERPRICED). | `omega_score.py:174`, `omega_score.py:668` | extends (§9.1 does not fix the pooling, the floor use, or the empty-non-free case) |
| 9 | HOLD-RANK-DEPENDENT: first hold = `rank_in_rep == 1`. WINDOW-SHORT fires when the guard's lower CI exceeds the omega_inf point estimate. OMEGA-NEGATIVE also needs |omega| above the A/A floor. | `omega_score.py:424`, `omega_score.py:582`, `omega_score.py:579` | extends |
| 10 | STEEP-AT-<preset> evaluated at one measured d (default 11 ms) and sigma_R (default 4 ms), not over the whole d range. | `omega_score.py:224` | extends (§8 gives bands at d = 1 and 11 ms) |
| 11 | B_t per rate = g * median(srtt - minrtt) of the F = 8 tunnel phase, last 60 s. | `omega_parse.py:432`, `omega_tunnel.sh` (omega_tunnel_measure) | extends (§4.3 asks for one Q_t per rate; the scaling is mine) |
| 12 | A stop request is honoured at the next round boundary, only after one full round; hard deadline refuses a row that cannot end before it. | `omega_battery.sh:161` | matches §12 ("balanced round boundary"); the one-round floor extends |
| 13 | The rehearsal runs the whole envelope: topology, tables, sysctls, R0 (paced probe, table size, seed, plug, ip-id), the tunnel bring-up and standing-queue measurement, and one row per arm type plus both outage arms. Any failure prints NO-GO. It scores nothing. | `omega_run_all.sh:317`, `omega_cells.py:208` | extends §11.1 (R0 and the tunnel are session R's; the rehearsal repeats them as the dry run) |
| 14 | iperf3 and every long process are direct children, stopped by TERM on the recorded PID (a background job ignores SIGINT; iperf3 prints its interval JSON on TERM). | `omega_topo.sh` (omega_bg), `omega_battery.sh:115` | extends §4.2 (process control by PID) |
| 15 | Netem seed. The design does not need equal selection sequences across matched arms: events are counted from the pcap, never from p (§4.1), and the seed only gives run-to-run reproducibility (§7.2, rule 4: seed 42 for the first half of each arm's reps, 7 for the second). R0 runs the same-seed probe twice and compares the selected ids; a difference is the registered fallback (rep halves stand in, recorded), not a NO-GO. | `omega_run_all.sh:292` (omega_seed_check) | matches §11.1 |
| 16 | Recorded PIDs carry their start time (`/proc/<pid>/stat` field 22) and are signalled only while it still matches, so a reused PID is never touched; no start time, no signal. | `omega_topo.sh:74` (omega_pid_ours, omega_signal), `omega_run_all.sh:91` | extends §4.2 (process control by recorded PID) |
