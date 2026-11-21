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
- **Two choices are declared, not measured. Please confirm them.**
  1. **Linux defaults stand for the TCP the tunnel's users run** (RACK, DSACK undo). A classic, more fragile TCP is measured afterwards, as a bound, once the Linux result has been read.
  2. **One byte of inner goodput lost is priced as one byte on the wire**, the same price as one wasted repair copy. A different price scales ω by the same factor.
- **Warning: this can change the rule, not only a number.** A hold stops the inner TCP's acknowledgements, so its sender stops. If the queue in front of the tunnel is shallow, holding a packet for a few milliseconds can cost the inner flow more than releasing it, at every preset. The derivation prices a stall by δ alone. That is about 0.5 symbols per stalled second at Bulk, while a stalled 10 000 packets/s flow loses up to 10 000 packets per second. If the measurement shows this, the rule as derived would release almost at once at every preset, and δ would decide nothing. That goes back to the derivation as a design question (§2), not into a constant.
- **Cost.** About 14 h of VM time for the Linux stack, in four sessions, each under the five-hour cap. The classic stack and the sweeps follow in a later amendment, budgeted after the Linux result (§12).

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
  - **c_R1, c_H1 and ω₁** come from the first event of a connection, measured with **short reps**: many connections per cell, one scored event each (§7.2).
  - **c_R∞, c_H∞ and ω_∞** come from events 2 onward, measured with **long reps** on a subset of Δt (§6.2).
  - **ω₁ governs the N = 1 release.** A released-then-filled hole is rare there, so it usually hits a flow with no recent reordering.
  - **ω_∞ governs the N = 2 skew condition** `ω ≥ δ·Δ/d` (`ordering-hold.md` §5). There every slow-path packet is a release.
- **The goodput guard (long reps).** `[ (Ḡ_CLEAN − Ḡ_REL)/Ē_REL − (Ḡ_CLEAN − Ḡ_HOLD)/Ē_HOLD ] / S` over the whole window. It catches costs that outlast A_e, such as a CUBIC regrowth of seconds after a cut that was never undone.
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

| configuration (W, Q_s) | k = 8 | k = 32 | t = 1 | t = 4 | at Q_t |
|---|---|---|---|---|---|
| 1 000 /s, RTT 10 (10, 4) | 5 | 34 | 0 | 7 | 0 everywhere (Δt·g ≤ 500 < Q_t) |
| 10 000 /s, RTT 10 (100, 25) | 0 | 9 | 0 | 90 | 0 |
| 1 000 /s, RTT 50 (50, 13) | 0 | 23 | 0 | 44 | 0 |
| 10 000 /s, RTT 50 (500, 125) | 0 | 0 | 0 | 450 | 0 |

**Against J_U's rent for the same stall**, `δ_p·Δt/d`. At Δt = 50 ms and d = 11 ms that is 0.023 symbols at Bulk, 2.3 at Auto and 227 at Realtime. A measured c_H of 450 symbols exceeds all three. And if c_H > c_R, then ω < 0: holding costs more than releasing.

**The question for the derivation, written now:** is a stall's inner-goodput cost a measured rent term that J_U is missing, beside the declared δ term?
- **If yes**, rent-or-buy compares the measured rent `c_H(t)` (plus `(δ/d)·t`) against the measured purchase price `c_R`. δ then decides only where the inner cost of the stall is small.
- **The answer may depend on the queue in front of the tunnel**, which is why Q is a core factor here (§4.3). The tunnel's own queue is deep, and there c_H ≈ 0 for every operating Δt. A shallow bottleneck elsewhere on the inner path makes c_H dominate.

**The measurement decides it (§9.1)**, per preset and per queue:
- `RENT-UNDERPRICED-<preset>`: the measured c_H1 exceeds `δ_p·Δt/d`. The finding fires: the rule, not only ω, goes back to `ordering-hold.md`.
- `RENT-COVERED-<preset>`: the measured c_H1 does not exceed it. J_U's pricing stands at that preset, and the ω zones of §9.2 apply.

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
- **Events start after warm-up.** Every arm starts with the zero table and switches to its own (CLEAN to zero again) at t_w = 3 s + 2·Q/g, so no event falls in slow start or in the first queue fill.
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

### 4.3 The queue Q: two levels, both in the core

| level | Q | provenance |
|---|---|---|
| **Q_t, the tunnel's own queue** | 4 596 packets at 10 000 /s (0.46 s); 460 at 1 000 /s, the same 0.46 s (see below) | When the tunnel is the inner flow's bottleneck, the queue in front of it is the TUN device queue plus the engine's TUN read channel. The TUN queue is the kernel default 500, since raptorpath sets no `txqueuelen`; stage T and R0 read the qlen back. The read channel is 4 096 deep (`tun/linux/mod.rs:24`). The sender's window (`WIN_STORE_MAX` 4 096) only adds to it |
| **Q_s, a shallow bottleneck elsewhere on the inner path** | max(⌈W/4⌉, 4) | not the tunnel's queue: the case where the inner path has a tighter, shallow queue (an access link) ahead of the tunnel. It is the end at which a hold is most expensive (§2) |

- **The 1 000 /s Q_t deviation, stated.** The literal 4 596 packets are 4.6 s of queue at 1 000 /s. Inner CUBIC then needs tens of seconds to reach steady state. The prediction there is identical (c_H = 0 whenever Δt·g < Q, and Δt·g ≤ 50 at 1 000 /s), so the 1 000 /s Q_t cells use the same time depth, 0.46 s.
- **Bottleneck drops per event** are a readout (§7.1). That answers how much of c_H is the release burst overflowing Q, as opposed to idle link time.

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
- **Operating range:** Δt ≤ 61 ms (three rounds at d = 11 ms, `ordering-hold.md` §4.2). Every cell here is inside it.
- **100 /s is deferred to phase C** (sweep S1). At RTT 10 it is not run at all (W = 1).

### 6.2 Core D (scored)

Stack D × g {1 000, 10 000} × RTT {10, 50} × Q {Q_t, Q_s} × F = 1: eight configurations.
- **Q_t, short reps:** CLEAN, and REL and HOLD at every Δt of §6.1. That is 15 + 17 + 17 + 17 = **66 cells**.
- **Q_t, long reps:** CLEAN, and REL and HOLD at k ∈ {3, 8} and t ∈ {1, 4}. That is 9 arms per configuration, **36 cells**.
- **Q_s, short reps:** CLEAN, and REL and HOLD at k ∈ {3, 8} and t ∈ {1, 4}. That is **36 cells**. These Δt span the sign change of (Δt·g − Q_s) in every configuration (§2's table).

### 6.3 Phase C (registered in outline; budgeted by an amendment after D is read)

- **C on the Q_t core:** short reps on the 9-arm subset.
- **Sweeps S1–S3:**
  - **S1:** 100 /s, RTT 50, k ∈ {1, 3, 8}.
  - **S2:** 4 flows at 10 000 /s, RTT 50, k ∈ {3, 8} and t = 1.
  - **S3:** event spacing 1 s against 4 s for ω_∞, at 1 000 /s, RTT 50, k = 8. It runs at 1 000 /s because 4 s at 10 000 /s needs p below the table's resolution (§11.1).
- **BBR.**
- **Estimate:** ≈ 8 h. D's result may make parts moot, for example C if `RENT-UNDERPRICED` fires everywhere.

### 6.4 VOID and witness rules: §7.2

### 6.5 Outage check (one small stage, a check of the owner's acceptance)

- **Cells.** D, 10 000 /s, RTT 50, Q_t: CLEAN, OUT-REL and OUT-HOLD, long reps, n = 8. That is **3 cells**.
- **Why so small.** The owner accepted late out-of-order delivery after long outages (2026-11-21). This stage only checks that the inner TCP agrees (§9.4).

### 6.6 Stage T: the tunnel in the path (reported)

- **Topology.** `tun_bulk.sh`'s bring-up on the `clean` scenario (no loss, no jitter, so no holes of the tunnel's own), hint `auto`. `rp-app` sits behind `rp-srv` on a veth. The reorderer is on `rp-srv`'s egress to `rp-app`, after TUN delivery, and iperf3 runs from `rp-cli` to `rp-app`.
- **Stack D, short reps:** CLEAN, and REL and HOLD at t ∈ {1, 4}, with Δt from the inner min_RTT measured in R. That is **5 cells**.
- **Witnesses.** TUNPIPE, TUNMTU, TUNSHA, and the qlen of `rpcli0` (the TUN queue of §4.3). The pre-reorderer pcap shows zero reordering in CLEAN.

**Phase D cells: 66 + 36 + 36 + 3 + 5 = 146.**

## 7. Readout and statistics

### 7.1 Per rep

- **Traffic.** iperf3 `-J --get-server-output -i 1`, one fresh connection. Short reps run t_w + 3 s; long reps run t_w + 15 s.
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
- **Socket and bottleneck.** `ss -tin` in `rp-cli` near the end (cwnd, `reord_seen`, `dsack_dups`, rtt, minrtt). `tc -s qdisc` on `cli0`: sent, drops, backlog and utilisation.

### 7.2 n, seeds, order, witnesses

- **Short reps (ω₁, c_R1, c_H1): many connections, each one scored event (its first).** They are the rule for ω₁, not a fallback.
  - **n₁ is sized from R's noise, per configuration**, before any scored data. R runs 16 short reps each of CLEAN, REL and HOLD at k = 8, which give σ̂_R and σ̂_H of D_e,1 in symbols:

    ```text
       n₁  =  ⌈ 1.96² · ( σ̂_R² + σ̂_H² ) / h² ⌉ ,   h = 0.3 · max( ω₁,pred , 1.2 ) ,   clamped to [32, 96] per arm
    ```

  - With h = 0.3·ω, the interval spans at most a factor of about 1.9, so it can cross at most one zone edge (the narrowest gaps of §8 are about ×1.9).
  - If the rule asks for more than 96, that configuration is declared `POWER-LIMITED` in the amendment, before scoring, and runs at 96.
  - The budget in §12 plans n₁ = 32.
  - A short rep whose first event falls after t_w + 2 s is `NO_EVENT`: excluded, counted and reported.
- **Long reps (ω_∞, the guard):** n = 8, about 14 later events each.
- **Seeds.** netem seed 42 for the first half of each arm's reps, 7 for the second (rule 4), with per-half medians reported. If R0 finds `seed` unsupported, the halves are rep halves under an unseeded RNG, recorded as a deviation; no cell is skipped.
- **Interleaving (rule 3).** Each round runs every arm of a configuration once, in a seeded shuffle. All arms of a configuration run in one session, on one bring-up.
- **Live minimum.** A short-rep cell needs ≥ 0.75·n₁ live events per arm, a long-rep cell ≥ 6 live reps, else `UNSCOREABLE`. NO_DATA is retried once at the end of the configuration and recorded (rule 8).
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

Compare c_H1(Δt) with J_U's rent `δ_p·Δt/d`, at the measured d, at every scored Δt:

| outcome | condition | meaning |
|---|---|---|
| `RENT-UNDERPRICED-<p>` at Q_t | c_H1's lower CI > δ_p·Δt/d at some Δt | **The finding fires for the tunnel's own queue.** The rule, not only ω, goes back to `ordering-hold.md` §2–§3, and the owner is told before any law is adopted |
| `RENT-UNDERPRICED-<p>` at Q_s only | the same, at Q_s but not at Q_t | the finding holds for inner paths with a shallow bottleneck elsewhere; reported to the derivation as a scope question; the zones below still apply for the tunnel's own queue |
| `RENT-COVERED-<p>` | c_H1's upper CI ≤ δ_p·Δt/d at every Δt, at Q_t | J_U prices the hold adequately at p; §9.2 applies at p |
| `RENT-UNDECIDED-<p>` | neither | `NEEDS-MORE-POWER` at p |
| `OMEGA-NEGATIVE` | ω₁'s upper CI < 0 at some Δt | holding costs more than releasing outright; implies `RENT-UNDERPRICED` at every preset at that queue |

Bulk's rent is tiny (≤ 0.03 symbols for Δt ≤ 61 ms at d = 11 ms). So `RENT-COVERED-Bulk` needs c_H1 to be resolved essentially at 0. It is predicted at Q_t (no idle slot, no drop), and is `RENT-UNDECIDED-Bulk` if the CI cannot show it.

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
- **`T-CONFIRMS`** iff, per first event, the excess spurious retransmissions (REL − CLEAN) agree within ± 0.5, the excess bottleneck drops agree within ± 0.5, and the idle-slot share of c_H1 agrees in sign (decision bars, not law constants).
- **`T-DIVERGES`** otherwise. Option (ii) is then reconsidered (§3).

### 9.6 Predictions (rule 11), from a re-read of `ordering-hold.md` §1–§2 and RFC 8985 §6.2

- **D at Q_t.**
  - c_H1 ≈ 0 at every Δt: the queue (0.46 s) outlasts every hold.
  - c_R1 ≈ one spurious retransmission plus an undone reduction. The deep queue absorbs the reduction's dip, so c_R1 ≈ 1.2 symbols.
  - So ω₁ ≈ 1.2: **`RENT-COVERED` at Auto and Realtime** (Bulk `COVERED` or `UNDECIDED`), and **`OMEGA-ZA`** with `STEEP-AT-Auto` at some d. Bulk keeps order, and Auto reorders holes that need a second round.
  - ω_∞ < 0.5: at N = 2 with skew, Auto reorders every slow-path packet.
- **D at Q_s.** c_H1 grows as (Δt·g − Q_s)⁺ (§2's table).
  - **`RENT-UNDERPRICED` at every preset in the cells with Δt·g ≫ Q_s** (t = 4, and k = 32 at 1 000 /s), and `OMEGA-NEGATIVE` there.
  - That is the finding for inner paths with a shallow bottleneck elsewhere. It is not predicted to fire at Q_t.
- **Refuters of the measurement, not the law.**
  - If c_H1 at Q_t is materially above 0, either the queue is not what §4.3 says or the hold triggers losses. The drops-per-event readout decides which, before anything is read.
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
5. the t_w table switch working, with no event before it;
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
- **The TUN qlen** of a raptorpath TUN, in a stage-T bring-up: it pins Q_t's 500-packet term.

**Noise calibration.** For each of the 8 core configurations: 16 short reps each of CLEAN, REL and HOLD at k = 8, and 8 long CLEAN reps. These give σ̂ for n₁ (§7.2) and the MDEs. Nothing in it is scored.

**Smoke.** One rep of every arm type on both ends, with every §7.1 readout and echo present.

**The amendment** (committed before session D1) pins: per-rep costs, the re-budget, n₁ and MDE per configuration, `POWER-LIMITED` declarations, Q_t's measured TUN qlen, stage T's min_RTT and raptorpath sha, L, `seed` support, and any cut.

### 11.2 Harness (a Sonnet worker, tools only, before session R; red tests first)

Plan token `omega`, in the `*_run_all.sh` / `*_battery.sh` convention of `threadq2_run_all.sh`, all in `tools/l1`:

| file | role |
|---|---|
| `topo_reorder.sh` | namespaces up/down/status, `guard_ns` on every name, offloads off, per-namespace sysctls with readback and the pre-flight presence check |
| `omega_dist.py` | the two-point and zero netem tables |
| `omega_cells.py` | pure: the design of §6 as data (Δt, Q, p quantised to L, t_w, n₁, the seeded order) |
| `omega_battery.sh` | one configuration, short and long reps; kill by PID |
| `omega_run_all.sh` | the session envelope: locks, sentinels proven writable at launch, CR check, host-state snapshots, R0, smoke, stopper, exit check |
| `omega_parse.py` | pure: pcap digest, events, D_e, drops versus spurious, counter diffs, iperf3 window |
| `omega_score.py` | pure: c_R, c_H, ω, CIs, MDE, n₁, `WINDOW-SHORT`, envelope, VARIES, §8 bands, §9 outcomes |
| `test_omega.py` | unit and static tests |

- **Unit tests,** red first:
  - known delays give the right events and overtakers;
  - a HOLD stall of one head and 20 queued packets is one event;
  - D_e counts idle slots and retransmissions exactly;
  - a retransmission whose original never reached `mid0` is a drop, and one whose original did is spurious;
  - the event index splits first from later events;
  - the n₁ formula and its clamp;
  - `RENT-UNDERPRICED` against `RENT-COVERED` at the rent line ± 1 %;
  - `OMEGA-NEGATIVE`;
  - the §9.2 edges ± 1 % and `OMEGA-EDGE`;
  - one noisy arm does not lift the envelope;
  - `WINDOW-SHORT`;
  - a short-rep cell under 0.75·n₁ is `UNSCOREABLE`.
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
  - `power-limited` (n₁ clamped to 96);
  - `stopper` → TRUNCATED;
  - `bad-ns`;
  - `hoststate-changed` → ABORT-HOSTSTATE.
- **Static checks:**
  - every `ip`, `tc`, `ethtool` or `sysctl` call is wrapped in `ip netns exec rp-…` or `ip -n rp-…`;
  - no `modprobe`, `nft`, `iptables`, `pkill -f` or `killall`;
  - every `kill` takes a recorded PID.
- **Done when** `test_omega.py`, `test_stubrun.py` (FAST) and the existing tools tests pass, and `harness_paths` equals what ran. No engine file changes.

## 12. Sessions and VM time (phase D; phase C by a later amendment)

Short rep ≈ 11 s (t_w ≈ 4 s, event wait ≤ 2 s, A_e, stop and snapshots, digest). Long rep ≈ 22 s. n₁ = 32 is planned; the amendment re-budgets from R.

| session | content | ≈ time |
|---|---|---|
| R | R0 and tool checks, noise calibration (8 × 48 short + 8 × 8 long), smoke | 2.0 h |
| D1 | Q_t, RTT 10: short (32 cells × 32), long (18 × 8), outage check (3 × 8) | 4.2 h |
| D2 | Q_t, RTT 50: short (34 × 32), long (18 × 8) | 4.2 h |
| D3 | Q_s short (36 × 32), stage T (5 × 32, bring-up included) | 3.8 h |
| **phase D** | | **≈ 14.2 h, four sessions, each under 5 h** |
| phase C | C on the Q_t subset, S1–S3, BBR: after D is read, by amendment | ≈ 8 h (estimate) |

- **If n₁ rises above 32 in the amendment, sessions are added, never squeezed.**
- **Cut order for phase D, pre-registered:**
  1. Q_s at t = 1;
  2. long reps at k = 3;
  3. n₁'s planning floor from 32 to 24, which never lowers a configuration below its sized n₁ if that is smaller.

  Together they save ≈ 2 h.
- **A detached stopper per session** ends at a balanced round boundary, or at launch + 4.9 h. It stops only its recorded PIDs and raptorpath by exact name, lets the driver tear down its namespaces, clears both locks and writes `TRUNCATED-<session>`.
- **No polling (rule 13).** The watcher waits on `DONE-ALL || FAILED-ALL || TRUNCATED-*`.
- **Exit state, verified:** no raptorpath; no recorded PID alive; no `rp-*` namespace; both locks released; root `tcp_*` sysctls equal to pre-flight.

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
