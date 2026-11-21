# The price of disorder ω: pre-registration (paper only, no VM contact)

Draft for review before any VM contact. No row has run and no harness line exists. Base text: github main `42db2bd6`. Roadmap node: `meas-disorder-price`. Design inputs: `docs/derivations/ordering-hold.md` (§1, §3.2, §4, §5, §10, §11), `docs/derivations/skip-rule-efficiency.md` §6, `docs/measurement-discipline.md` (rules 1–19, the pre-registration protocol, the VM protocol, the five-hour cap). House format: `docs/derivations/vm-batch-1-registration.md`.

**Owner decision (taken).** Measure ω on a TCP-in-tunnel test after VM batch 1, and follow the result.

**For the owner, in plain words.**
- **What is measured.** How much one packet that arrives late, after packets sent behind it, costs a TCP flow that runs inside the tunnel. That cost is ω, and the receiver's ordering hold is `H_o = ω·d/δ`.
- **No engine code.** Reordering cost belongs to the inner TCP, not to raptorpath. A kernel reorderer (`tc netem`) placed in the `rp-*` test namespaces makes exactly the late deliveries a release would make. The scored stages run plain kernel TCP through that reorderer. One small reported stage repeats a few cells with the real tunnel in the path, to check that the result carries over (§2, §5.6).
- **Two ends of the TCP spectrum.** Linux as shipped (RACK, DSACK undo) and a classic setting with those switched off. Expect the modern stack to shrug off most reordering, so ω may come out small. If it does, Auto and Bulk would deliver out of order in normal operation, and that comes back to you as a default change, not as a silent constant (§8).
- **Cost.** About 12 h of VM time in four sessions (rehearsal, then three batteries), each under the five-hour cap. A pre-registered cut order brings it to about 10 h if you want it shorter (§11).

**Inputs read** (`github/main` `42db2bd6`):
- `ordering-hold.md`: the release rule `H_o = ω·d/δ` (§3.2), ω's definition and provenance (§1), the preset tables (§4), the N = 2 skew condition `ω ≥ δ·Δ/d` (§5), the gauges (§6) and the owner questions (§11);
- `skip-rule-efficiency.md` §6: the toy, its jitter provenance (0, 1, 4 ms), the 25 ms refresh floor and the cells;
- `tools/l1`: `lib.sh` (`guard_ns`, `NS_CLI`/`NS_SRV`, `stop_raptorpath`), `lib_battery.sh` (`take_lock`, `release_locks`), `topo.sh`, `tun_bulk.sh`, `threadq2_run_all.sh` (the envelope convention), `stubrun_gen.py`, `test_stubrun.py`, `harness_paths.py`, `test_topo_netem.sh`;
- `raptorpath/src/perf.rs:52` (Bulk and Auto `symbol_size` = 1200 B, TUN MTU 1196);
- RFC 5681 §3.2, RFC 6675, RFC 8985 §6.2, RFC 9438 (CUBIC, β = 0.7).

## 1. What ω is, operationally

ω is the price of one released-then-filled hole, in repair symbols (the unit of δ and w; `ordering-hold.md` §1). The release decision trades holding (rent, priced by δ) against releasing (buy, priced by ω). So ω is the difference between the two ways one late packet can reach the inner flow, measured on the same selected packet.

Three arms per configuration, all through the same qdisc tree (§3):

| arm | what happens to a selected packet | what it emulates |
|---|---|---|
| **REL(Δt)** | delayed by Δt; its successors pass | release at H_o, fill Δt later: one packet late, displaced |
| **HOLD(Δt)** | delayed by Δt; every successor waits behind it, then all leave in order | no release: the frontier waits for the fill (the "matched no-release run" of `ordering-hold.md` §1) |
| **CLEAN** | the same classifier and qdiscs, delay 0 | no event; the A/A control |

```text
   c_R  =  ( Ḡ_CLEAN − Ḡ_REL )  / Ē_REL           bytes of inner goodput lost per REL event
   c_H  =  ( Ḡ_CLEAN − Ḡ_HOLD ) / Ē_HOLD          bytes lost per HOLD event
   ω̂    =  ( c_R − c_H ) / S                       S = 1200 B (symbol_size, perf.rs:52)
```

- Ḡ is the arm's mean inner goodput in bytes over the scored window, Ē its mean count of realised events per rep (pcap truth, §6.1).
- **ω̂ is the scored quantity.** c_R/S (the whole cost of a late packet against no event) and c_H/S (the cost of the stall, which J_U prices by δ, not by ω) are reported beside it, never scored.
- **Unit: a declared exchange rate.** One byte of consumer goodput not delivered is priced as one byte of wire symbol, the same identity that prices a wasted copy at w = 1 (`ordering-hold.md` §1). If the owner prices consumer goodput differently, ω scales linearly with that price. Spurious retransmissions are not added separately: at a saturated bottleneck each one already displaces a slot of goodput (§6.3).

## 2. Engine code: none

| | (i) netem-only reorderer in `rp-*` namespaces | (ii) a test seam in raptorpath that forces releases |
|---|---|---|
| what controls the displacement | the reorderer, exactly: per selected packet, Δt and hence k = Δt·rate | the tunnel's own ARQ. The fill comes from a repair the seam cannot time, so Δt is not a cell parameter |
| moving parts | kernel netem and prio/plug qdiscs, iperf3, tcpdump, ethtool (`adv_cells.sh` already uses iperf3 and ethtool, `perf_rwm_c.sh` tcpdump) | a `test-seams` twin build, the tunnel's CC and pacer, its loss process to make holes, its ARQ, plus everything in (i) to read the inner TCP |
| binary identity | untouched: no engine source changes | the seam must be `#[cfg(feature = "test-seams")]`, default off; `binid.yml` must show the release binary, `.text` and `.rodata` unchanged |
| what it measures | the inner TCP's response to a known displacement | the same response mixed with the tunnel's repair timing |
| what it cannot see | bundling (a tunnel seq that carries several small IP packets), the tunnel's pacing jitter | none of those, but cannot isolate them either |

**Chosen: (i).** ω is a property of the inner flow's reaction to a displacement. Which displacements occur is an input from the release rule and its gauges (`ordering-hold.md` §6, "displacement of late fills"), and the cells sweep it. So the scored stages need no raptorpath at all.

- **Deviation from the owner's wording, stated.** The scored stages are TCP through a reorderer, not TCP through the tunnel. Stage T (§5.6, reported) runs the same reorderer behind the real tunnel and compares, so the deviation is bounded, not argued. A tunnel stage is needed because reordering on the underlay legs never reaches the inner TCP: the tunnel delivers in order. Stage T therefore reorders on the inner side, after TUN delivery.
- **If Stage T reads DIVERGES (§8).** Option (ii) is then reconsidered under its own registration. The seam would sit at the receiver's delivery path, at the reorder-buffer expiry (`receiver.rs:1012–1024`), and would delay one chosen delivered seq by Δt. It must be default-off behind `test-seams`, with no env read in the default build and binid unchanged (ADR-0082). Nothing in this registration builds it.

## 3. Topology and the reorderer

Three namespaces, all `rp-*`, joined by veth pairs. A new script `tools/l1/topo_reorder.sh` brings them up and down. It calls `guard_ns` on every namespace name before use.

```text
   rp-cli ── cli0 ═══ mid0 ── rp-mid ── mid1 ═══ srv0 ── rp-srv
            egress: bottleneck        egress: the reorderer       egress: the RTT (ACK path)
            netem rate g·1500 B       (§3.1)                       netem delay RTT, no rate
            limit Q, no delay
```

- **The bottleneck** is on `cli0`: `netem rate` at g × 1500 B with queue limit Q packets and no delay, so the limit counts the queue alone. netem with `rate` is FIFO (rule 19), so the bottleneck itself never reorders. The reorderer sits after it, where packets are spaced exactly 1/g while the queue is busy.
- **The RTT** is the whole propagation delay, on the ACK direction (`srv0` egress, `netem delay`, no rate). TCP does not care on which leg the delay sits.
- **Q = max(⌈W/4⌉, 4)** packets, with W = g·RTT. A shallow queue is the conservative end: a cwnd cut drains it and costs goodput. With Q ≥ W a CUBIC cut would cost nothing. Sweep S4 measures Q = 2W (§5.4).
- **Offloads off.** TSO, GSO and GRO are switched off on all four veth ends (`ethtool -K` inside each namespace), so every skb is one packet. Witness: no frame in the pcap exceeds MTU + 14.
- **rp-mid forwards.** `net.ipv4.ip_forward = 1` is set inside `rp-mid` only.

### 3.1 The reorderer (semantics fixed here; the exact `tc` lines are fixed by the harness and verified in R0)

- **REL and HOLD: one netem on `mid1`, with a two-point delay distribution.** The harness writes a netem distribution table (loaded through `TC_LIB_DIR`, so nothing is written under `/usr/lib`). The table has a fraction p of entries at +Δt and the rest at 0. The two arms differ in one setting only:
  - **REL:** no `rate`. netem's time-sorted queue lets the unselected packets pass the delayed one.
  - **HOLD:** a `rate` of 10 × the bottleneck. With `rate`, netem is FIFO (rule 19, and `netem_enqueue`'s time reference is the queue tail), so every successor waits behind the selected packet. They then leave as a burst at 10 × g, as a reorder buffer releases a held run.
  - **CLEAN:** the same netem with an all-zero table.
  - **Selection** is random per packet with probability p, quantised to the table size. p is set so that events are spaced ≈ 1 s per flow (sweep S3 uses 4 s). The realised count comes from the pcap, never from p.
  - **Events start at t = 3 s.** Every arm starts with the all-zero table and switches to its own at t = 3 s (`tc qdisc change`, CLEAN to its zero table), so no event falls in slow start.
- **Outage (stage O): prio + plug on `mid1`.** A two-band `prio`, each band ending in a `plug` qdisc.
  - **Selector:** band 1 takes the packets with odd IP ID. Linux TCP with DF set uses a per-socket IP ID counter, one per segment, so this alternates exactly within a flow, which is N = 2 round-robin placement. R0 verifies it. If the IDs are not consecutive, stage O is not run (`SKIPPED-SELECTOR`) and nothing is improvised.
  - **OUT-REL:** band 1's plug blocks at t = 6 s and releases at t = 7 s, so one path is dead for 1 s and its packets arrive as a late burst after their successors.
  - **OUT-HOLD:** both plugs block over the same second, so the receiver holds at the first hole.
  - **CLEAN:** neither plug blocks.
- **Packet displacement from time.** For a target of k successors, Δt = (k + ½)/g_flow, with g_flow = g/F for F flows. The realised k of every event is read from the pcap (§6.1), and the cell keeps its target label.

### 3.2 VM rules this plan respects

- Only namespaces named `rp-*` (`rp-cli`, `rp-mid`, `rp-srv`, and `rp-app` in stage T). Every `ip`, `tc`, `ethtool` and `sysctl` command runs as `ip netns exec rp-…` or `ip -n rp-…`. The battery runs no `nft` or `iptables` at all.
- `ens18`, the firewall and `sshd` are never touched. No root-namespace sysctl is written: every TCP sysctl in §4 is per network namespace and set inside `rp-cli` and `rp-srv`. The root namespace's `net.ipv4.tcp_*` values are snapshotted at pre-flight and compared at exit (ABORT-HOSTSTATE if any differs).
- No `modprobe`. A qdisc or congestion control that is not already available is skipped, not loaded. The kernel autoloads `sch_netem` and `sch_plug` when a qdisc is added, as `topo.sh` relies on today; `lsmod` is recorded before and after.
- Processes are killed only by exact name or recorded PID. iperf3 servers run with `--pidfile`, tcpdump PIDs are taken from `$!`, and raptorpath (stage T only) is stopped with `stop_raptorpath` (`pkill -x raptorpath`).
- **Locks.** The box lock `/tmp/rwm-vm.lock` and the tree lock `${RWM_RP_LOCK:-$HOME/rp.lock}` are taken for the whole session. If `test -e` finds either one, the session refuses (ABORT-LOCK). Otherwise `take_lock` from `lib_battery.sh` takes it (noclobber + flock), and `release_locks` frees it at exit. A lock this session did not create is never removed. HOME resolves to the invoking user under sudo (commit `42db2bd6`).
- Docs and logs say "the VM": no user name, address or key path is written.

## 4. Inner stacks

Each stack is set by `sysctl -w` inside `rp-cli` (the sender) and `rp-srv` (the receiver), and read back into the log as the arm's echo (rule 15). A readback that differs aborts the session (ABORT-SYSCTL). `uname -r` and `lscpu` head the log (rule 9).

| sysctl (`net.ipv4.`) | **D**: Linux defaults | **C**: classic, the conservative end |
|---|---|---|
| `tcp_congestion_control` | cubic | cubic |
| `tcp_recovery` | 1 (RACK-TLP) | 0 (RACK off) |
| `tcp_reordering` | 3 | 3 |
| `tcp_max_reordering` | 300 | 3 (dupthresh pinned at 3, RFC 5681) |
| `tcp_sack` | 1 | 1 (RFC 6675 recovery) |
| `tcp_dsack` | 1 (DSACK undo) | 0 (no DSACK undo) |
| `tcp_timestamps` | 1 | 0 (no timestamp-based undo) |
| `tcp_frto` | 2 | 0 |
| `tcp_early_retrans` | 3 | 3 |
| `tcp_ecn` | 2 | 2 |
| `tcp_no_metrics_save` | **1** | **1** |

- **D is the kernel's shipped default in every row but the last.** The harness sets each value explicitly and reads it back, so a VM with changed defaults cannot pass silently.
- **`tcp_no_metrics_save = 1` in both stacks, and `ip tcp_metrics flush all` before every rep, in both namespaces.** Otherwise the kernel caches reordering and ssthresh per destination, and one rep's events would leak into the next. Every rep is also a fresh TCP connection, so RACK's `reo_wnd` state starts at zero.
- **C goes further than "RACK off".** It also removes DSACK and timestamp undo and F-RTO, so it is RFC 5681 + RFC 6675 with no undo, the most reordering-sensitive stack that is still standard.
- **B (optional): D with `tcp_congestion_control = bbr`.** It runs only if `bbr` is already in `tcp_available_congestion_control` at pre-flight. Otherwise it is `SKIPPED-BBR`, recorded, with no module load.

## 5. Cells

### 5.1 Grid facts (W = g·RTT, in 1 500 B packets)

| g (packets/s) | Mbit/s | W at RTT 10 ms | W at RTT 50 ms | Q at 10 / 50 ms |
|---|---|---|---|---|
| 100 | 1.2 | 1 | 5 | 4 / 4 |
| 1 000 | 12 | 10 | 50 | 4 / 13 |
| 10 000 | 120 | 100 | 500 | 25 / 125 |

**Displacement Δt per cell (ms).** Packet targets k ∈ {1, 2, 3, 8, 32} give Δt = (k + ½)/g. Time targets c ∈ {0.25, 1, 4} × min_RTT/4 give Δt = c·RTT/4.

| g | k = 1 | 2 | 3 | 8 | 32 | t = 0.25 | 1 | 4 (= min_RTT) |
|---|---|---|---|---|---|---|---|---|
| 100 | 15 | 25 | 35 | 85 | 325 | 0.625 / 3.125 | 2.5 / 12.5 | 10 / 50 |
| 1 000 | 1.5 | 2.5 | 3.5 | 8.5 | 32.5 | same | same | same |
| 10 000 | 0.15 | 0.25 | 0.35 | 0.85 | 3.25 | same | same | same |

Time columns read RTT 10 / RTT 50.

**Degenerate, pre-listed:**
- **Time cells with Δt·g_flow < 1** displace nothing. These are skipped, not run: (1 000, RTT 10, t = 0.25) and every 100 packets/s time cell.
- **g = 100 at RTT 10** (W = 1) is not TCP in any useful sense and is not run.
- **Operating range.** Released-then-filled holes in the operating range are filled at most about three rounds after detect: τ₃ = d + 50 ms ≤ 61 ms (`ordering-hold.md` §4.2). Cells with Δt > 61 ms (100 packets/s, k = 8 and 32) are run and reported, but excluded from the envelope of §6.4.
- **k > W** (for example k = 32 at W = 10) is kept. With SACK the sender keeps sending past the late packet once it is marked lost, so k successors can still overtake it. Such cells are labelled `beyond-window`.

### 5.2 Core grid P (scored): sessions A and B

Stacks {D, C} × g {1 000, 10 000} × RTT {10, 50 ms} × F = 1. Arms: CLEAN, plus REL(Δt) and HOLD(Δt) at each non-degenerate Δt of §5.1.
- (1 000, 10): 15 arms. (10 000, 10), (1 000, 50) and (10 000, 50): 17 arms each.
- That is 66 cells per stack and **132 cells** in all: session A runs RTT 10 (64 cells), session B runs RTT 50 (68 cells).

### 5.3 One-factor sweeps S (scored as stated): session C

Each sweep runs both stacks and carries its own CLEAN, because it runs in another session than P (rule 3).

| sweep | base | arms | cells |
|---|---|---|---|
| S1 rate 100 | g = 100, RTT 50 (W = 5) | CLEAN, REL/HOLD at k ∈ {1, 3, 8} | 14 |
| S2 flows 4 | g = 10 000, RTT 50, F = 4 (`iperf3 -P 4`) | CLEAN, REL/HOLD at k ∈ {3, 8} per flow, t = 1 | 14 |
| S3 spacing | g = 10 000, RTT 50 | CLEAN, REL/HOLD k = 8 at 1 s and at 4 s event spacing | 10 |
| S4 deep queue | g = 10 000, RTT 50, Q = 2W | CLEAN, REL/HOLD k = 8 (S3's 1 s arms are the Q = W/4 reference) | 6 |

### 5.4 Outage O (scored for owner question 2 only): session C

Stacks {D, C} × g {1 000, 10 000} × RTT {10, 50}; arms CLEAN, OUT-REL, OUT-HOLD; one outage per run, at t = 6–7 s. That is **24 cells**. It is the N = 2 equivalent of `ordering-hold.md` §4.3, one path dead for 1 s: a burst of late packets against a 1 s full stall.

### 5.5 BBR (reported, optional): session C, last

B at g = 10 000, RTT 50: CLEAN, REL/HOLD at k ∈ {3, 8}. That is 5 cells, run only if `bbr` is available.

### 5.6 Stage T, the tunnel in the path (reported): session C

- **Topology.** `tun_bulk.sh`'s bring-up on the `clean` scenario (no loss, no jitter, so the tunnel makes no holes of its own), hint `auto`, and a fourth namespace `rp-app` behind `rp-srv` on a veth.
  - The reorderer of §3.1 sits on `rp-srv`'s egress to `rp-app`, after TUN delivery.
  - iperf3 runs from `rp-cli` to `rp-app`, through the tunnel.
- **Stack D.** Arms CLEAN, REL/HOLD at t = 1 and t = 4 (Δt from the inner min_RTT measured in the rehearsal smoke and pinned in the amendment): **5 cells**.
- **Witnesses.** TUNPIPE, TUNMTU and TUNSHA as `tun_bulk.sh` prints them. The pcap taken before the reorderer shows zero out-of-order arrivals in CLEAN, so the tunnel itself added none.

Cells in all: 132 (P) + 44 (S) + 24 (O) + 5 (B) + 5 (T) = **210**.

## 6. Readout and statistics

### 6.1 Per rep

- **Traffic.** iperf3 `-t 18 -i 1 -J --get-server-output`, one fresh connection (or `-P 4` in S2). Goodput Ḡ is the receiver's interval bytes over [3, 18] s.
- **Truth: two pcaps per rep.** tcpdump runs headers only (`-s 96`) on `mid0` ingress (before the reorderer) and `srv0` ingress (after it), in every arm, so the capture cost is matched. A pure digest `omega_parse.digest` matches packets by (flow, TCP seq, IP ID) and gives, per packet, its delay through the reorderer and the number of same-flow packets that overtook it.
  - **Event:** a packet delayed by more than Δt/2 inside the scored window.
  - **Realised k:** the overtakers of each REL event.
  - **HOLD witness:** zero overtakers.
  - The raw pcaps are digested between reps (never during a rep). Only the digests are kept, plus the raw pcaps of rep 1 of each cell.
- **Counters.** `/proc/net/netstat` and `/proc/net/snmp` are read inside `rp-cli` and `rp-srv` before and after each rep (per namespace, diffed by the parser; no `nstat` history file):
  - `TcpRetransSegs`;
  - `TcpExtTCPDSACKRecv` and `TCPDSACKOldSent`;
  - `TCPSackRecovery`, `TCPRenoRecovery` and `TCPTimeouts`;
  - the undo counters `TCPFullUndo`, `TCPPartialUndo`, `TCPDSACKUndo` and `TCPLossUndo`;
  - `TCPSACKReorder`, `TCPTSReorder` and `TCPSpuriousRTOs`;
  - `TCPLossProbes`.

  R0 verifies that each name exists on the VM kernel. A missing one is printed `-` and its readout is `NEEDS-MORE-COUNTER`.
- **Socket state.** `ss -tin` in `rp-cli` at t = 17.5 s, for the data socket(s): cwnd, retrans, `reordering`, `reord_seen`, `dsack_dups`, `bytes_retrans`, rtt and minrtt.
- **Bottleneck.** `tc -s qdisc` on `cli0`: sent, drops and backlog. Utilisation is sent bytes ÷ (g × 1 500 B × window). With offloads off, netem's counters are per packet here, unlike rule 19's GSO case, and the pcap cross-checks them.

### 6.2 n, seeds, order

- **n = 8 reps per arm:** reps 1–4 with netem seed 42 and reps 5–8 with seed 7 (rule 4). Per-seed medians and every per-rep value are printed.
- **Interleaving (rule 3).** Within one configuration, each rep round runs every arm once, in an order shuffled per round with a recorded seed. All arms of one configuration run in one session, on one topology bring-up.
- **Live minimum.** A cell is scored with ≥ 6 live reps per arm, else `UNSCOREABLE`. A rep without an iperf3 summary is `NO_DATA`. It is retried once at the end of its configuration's rounds and recorded (rule 8).
- **VOID-WITNESS** (a rep, excluded from every denominator):
  - a REL rep with zero events, or fewer than 50 % of its events within ±1 of target k (F = 1 packet cells only; the others report the distribution);
  - a HOLD rep with any overtaker;
  - a CLEAN rep with any delayed packet;
  - a sysctl readback that changed during the rep.

### 6.3 Estimates, bounds and the noise floor

- **ω̂ (§1)**, with a 95 % percentile bootstrap CI: 10 000 resamples, reps resampled within each of the three arms.
- **The A/A floor.** CLEAN's seed-42 half against its seed-7 half, normalised like ω̂: `|Ḡ_C,42 − Ḡ_C,7| / Ē_REL / S`. An effect is claimed only if the CI excludes 0 and |ω̂| exceeds the floor (rule 5).
- **The count lower bound ω_LB.** Let r_x be the excess retransmitted segments per event (REL − HOLD, each normalised by its own events), and MSS the arm's echoed MSS. Then:

  ```text
     ω_LB  =  max(0, lower CI of r_x) · MSS / S          only where REL's bottleneck utilisation ≥ 0.95, else ω_LB = 0
  ```

  At a busy bottleneck a spurious retransmission takes a slot new data would have used, so it costs at least one segment of goodput. The DSACK counters confirm that the excess retransmissions were spurious.
- **The accounting upper bound ω_UB^acct.** It is valid only when the excess un-undone reductions per event (recoveries + timeouts − undos, REL − HOLD) have a CI upper bound < 0.05, and the excess spurious RTOs are 0:

  ```text
     ω_UB^acct  =  ( upper CI of r_x  +  upper CI of u_x · (1 − β) · W ) · MSS / S          β = 0.7 (CUBIC, RFC 9438)
  ```

  - u_x is the excess undone reductions per event.
  - Provenance: an undone reduction lowers cwnd by (1 − β)·W for at most about one RTT, until the DSACK or timestamp echo comes back. That is bounded by (1 − β)·W segments of send deficit.
  - If ω̂'s CI lies entirely above ω_UB^acct, the bound is contradicted: `ACCOUNTING-CONTRADICTED` is recorded and the bound is dropped.
- **The interval read in §7–§8:**

  ```text
     ω_lo  =  max( lower CI of ω̂ , ω_LB )          ω_hi  =  min( upper CI of ω̂ , ω_UB^acct where valid )
  ```

- **Power, said before the data.** The minimum detectable ω from goodput, at 80 % power and α = 0.05, is

  ```text
     MDE_ω  ≈  2.8 · σ_G · √(2/n) / ( E · S )          σ_G = per-rep goodput sd (bytes), E = events per rep ≈ 15
  ```

  - With an illustrative σ_G of 1 % of window goodput, MDE_ω ≈ 1.7 symbols at 100 packets/s, 17 at 1 000 and 170 at 10 000. The rehearsal's A/A pins σ_G per configuration in the amendment.
  - So **goodput alone cannot resolve Bulk's band (0.005–0.26) or the bottom of Auto's band at any rate here**, nor any small ω at 10 000 packets/s. The small-ω decisions rest on ω_LB and ω_UB^acct. A cell whose interval spans more than one zone edge of §7 reads `NEEDS-MORE-POWER` and is never read as ω = 0.

### 6.4 From a curve to one number

ω̂ is a function of the displacement Δt, and of the configuration (stack, g, RTT, F, Q, spacing). The release rule charges one ω per released-then-filled hole. That hole's remaining displacement is R − H_o, which itself depends on ω. So the number is read as follows, fixed now:

1. **Per configuration:** `ω_cfg = max over Δt ≤ 61 ms, non-degenerate, of ω̂(Δt)`, with the interval [ω_lo, ω_hi] of the maximising arm.
   - The maximum is used because a per-hole constant must not undercharge any hole the rule can release in the operating range.
   - The rent-or-buy bound of `ordering-hold.md` §3.2 holds against a purchase price that is not exceeded.
2. **Displacement dependence.** If, within one configuration, two scored arms with Δt ≤ 61 ms have intervals in different zones of §7 and disjoint CIs, the outcome is `OMEGA-VARIES-X`.
   - Then the purchase price depends on when the rule buys, and the derivation goes back to `ordering-hold.md` §3 (rent-or-buy with a displacement-dependent price) instead of using the envelope as a constant.
   - **Predicted for C** (a step at k = 3, RFC 5681 dupthresh). Predicted absent for D, whose detection is time-based.
3. **Configuration dependence.** For stack D, across the eight core configurations and the sweeps S1, S2 and S4, the same test reads `OMEGA-VARIES-RATE`, `-RTT`, `-FLOWS` or `-Q` (named by the factor that separates the zones).
   - Then ω is not a constant: H_o would have to read that factor, which `ordering-hold.md` §3.3 says it does not.
   - The result goes back to the derivation. **No constant is picked.**
   - **Predicted for C:** ω rises steeply with W. The reference scale is Reno's `W²/8` (12.5 packets at W = 10, 31 250 at W = 500); CUBIC's is lower.
4. **Otherwise** stack D's ω is the maximum ω_cfg over the core configurations, with its interval. That is the number `ordering-hold.md` uses.
   - C's number is reported beside it as the conservative bound and as the owner's alternative.
   - Which stack governs is fixed now: **D**, the stack the tunnel's users actually run.
5. **Spacing (S3).** If the 4 s spacing's interval is in a different zone from the 1 s one, the outcome is `OMEGA-VARIES-SPACING`. Then ω per event depends on how often events come, and that is reported to the derivation. Otherwise the 1 s value stands.

## 7. Where the presets sit (the steep parts)

At N = 1 a hole is filled after τ_k (round k): τ₁ ≈ d, and each further round adds the 25 ms refresh floor (`ordering-hold.md` §4.1), so τ_k = d + (k − 1)·25 ms. H_o = ω·d/δ crosses τ_k at

```text
   δ*_k  =  ω · d / τ_k          ⇔          ω*_{p,k}  =  δ_p · τ_k / d          (preset p releases before round k's fill iff ω < ω*_{p,k})
```

At N = 2 with skew Δ, the edge is `ω* = δ_p·Δ/d` (`ordering-hold.md` §5). Edges at d = 1 and 11 ms, Δ = 20 ms:

| preset (δ) | round 1 | round 2 | round 3 | skew Δ = 20 ms | zone (union over d ∈ [1, 11] ms) |
|---|---|---|---|---|---|
| Bulk 0.005 | 0.005 | 0.016 / 0.13 | 0.028 / 0.26 | 0.009 / 0.1 | **Z_B = [0.005, 0.26]** |
| Auto 0.5 | 0.5 | 1.6 / 13 | 2.8 / 26 | 0.91 / 10 | **Z_A = [0.5, 26]** |
| Realtime 50 | 50 | 164 / 1 300 | 277 / 2 550 | 91 / 1 000 | **Z_R = [50, 2 600]** |

The second and third columns read d = 11 ms / d = 1 ms.

- **Steep part.** At fixed d, a ±2 % nudge of δ changes rbf = S(H_o) materially only near an edge. The edge is widened by the fill-time jitter σ_R:

  ```text
     band_{p,k}  =  ω*_{p,k} · [ 1/(1 + b_k) , 1 + b_k ]          b_k  =  max( 0.02 , 2·σ_R / τ_k )
  ```

  σ_R is the per-path owd spread measured in VM batch 1 (`[LAT]`/`rttdump`) where it exists, else the toy's 4 ms (`skip-rule-efficiency.md` §6). The (d, τ₁) pairs are the measured ones from batch 1 where available, else the derivation's two examples (11 ms, and 0.78 ms from paper §7.3 c7). There is no free cross over d.
- **`STEEP-AT-<preset>`** fires iff [ω_lo, ω_hi] intersects some band_{p,k} at a measured (d, τ) pair.
- **What the doc then recommends.** The result goes back to the derivation, never to a clamp, a preset-keyed tweak or a moved threshold:
  1. the toy's P3 nudge check (`skip-rule-efficiency.md` §6) runs at that preset with the measured σ_R and ω, and decides whether behaviour is continuous there;
  2. if P3 fails, `ordering-hold.md` owes a revised law, or the owner takes the larger declared ω of `ordering-hold.md` §11 Q1, which moves that preset above its band (it keeps order). The owner may not take a value inside a band.
- **The bands are a hundredfold apart, as the δ presets are.** So, before jitter widening, one measured ω can sit on the steep part of at most one preset's band at a time. Only the gaps between bands, (0.26, 0.5) and (26, 50), and the two ends put no preset on a steep part.

## 8. Outcome set and what each means for the defaults

Verdict terms follow `docs/measurement-discipline.md`; nothing outside this set is recorded as a verdict.

**Primary, from stack D's ω interval (§6.4 item 4):**

| outcome | condition on [ω_lo, ω_hi] | Bulk | Auto | Realtime |
|---|---|---|---|---|
| `OMEGA-Z0` | ω_hi < 0.005 | releases before the first repair: every hole and every skewed packet reaches the consumer out of order. **Default change** | the same. **Default change** | the same (as today) |
| `OMEGA-ZB` | inside Z_B | `STEEP-AT-Bulk` at some d (§7) | releases at detect. **Default change** | releases at detect |
| `OMEGA-GAP-BA` | inside (0.26, 0.5) | keeps order through three rounds; rbf ≈ 0 in the operating range | releases before the first repair. **Default change** | releases at detect |
| `OMEGA-ZA` | inside Z_A | keeps order | `STEEP-AT-Auto` at some d. Below the measured d's edges Auto reorders on second-round holes, and at N = 2 on skewed packets. **Default change at that d** | releases before the first repair |
| `OMEGA-GAP-AR` | inside (26, 50) | keeps order | keeps order in the operating range | releases before the first repair (today ≈ SRTT/2) |
| `OMEGA-ZR` | inside Z_R | keeps order | keeps order | `STEEP-AT-Realtime` |
| `OMEGA-Z6` | ω_lo > 2 600 | keeps order | keeps order | holds through the first repair: Realtime waits longer than today. **Default change at Realtime** |
| `OMEGA-VARIES-<factor>` | §6.4 items 2, 3 or 5 | back to `ordering-hold.md`; no constant | | |
| `NEEDS-MORE-POWER` | the interval spans more than one zone edge in a deciding configuration | the zone is undecided. The close names the instrument: more events per rep, or a longer window at that configuration | | |
| `UNSCOREABLE` | fewer than 6 of the 8 core configurations scored | nothing is read | | |

- **Each "Default change" is the owner's call, never a merge clause** (`ordering-hold.md` §11 Q1). The options are to accept the reordering, or to declare a larger ω that clears the preset's band.
- **Pass/fail of the measurement itself.** **`DELIVERED`** when R0, the smoke and every scored cell's witnesses hold, and the primary outcome is one of the rows above. **`VOID`** for a stage whose witness fails at ≥ 25 % of its reps. **`TRUNCATED`** when the stopper fires (§11); it is scored at the n reached.

**Stack C (reported, bound):** the same table on C's interval. Predicted: `OMEGA-VARIES-X` and `OMEGA-VARIES-RATE`, with C at W ≥ 50 above Z_A.

**Outage O (owner question 2, `ordering-hold.md` §11):**
- **`OUT-RELEASE-WORSE`:** OUT-REL costs more goodput than OUT-HOLD, CIs disjoint. Late, out-of-order delivery after a long outage hurts the inner TCP more than a 1 s stall. The doc recommends keeping order after long outages, so Bulk's ω should clear `0.005·T_out/d`.
- **`OUT-RELEASE-BETTER`:** the reverse. Release after a long outage is cheaper.
- **`OUT-NULL`:** no difference beyond the A/A floor. Either answer is acceptable to the inner TCP.

**Stage T:**
- **`T-CONFIRMS`:** the tunnel cell's ω interval lies in the same zone as P's (D, 10 000, RTT 10) cell, or their CIs overlap.
- **`T-DIVERGES`:** otherwise. The netem-only ω does not carry over, and option (ii) is reconsidered (§2).

**Predictions (rule 11), from a re-read of `ordering-hold.md` §1 and RFC 8985 §6.2:**
- **D:** a fresh connection's first event costs about one spurious retransmission and one undone reduction. Later events with Δt < reo_wnd cost nothing, and reo_wnd grows per DSACK round up to SRTT. So r_x ≈ 1/E to 1 and ω ≈ 0.1–1.2 symbols: **`OMEGA-ZB` or `OMEGA-GAP-BA` or the bottom of `OMEGA-ZA`.** Bulk most likely keeps order, and Auto most likely reorders in normal operation.
- **Warning, carried to the owner:** under that prediction Auto changes default (`ordering-hold.md` §11 Q1).
- **Refuter of the measurement, not of the law:** if D's ω_lo > 26, the RACK expectation is contradicted. The sysctl and RACK witnesses (`reord_seen` > 0, `TCPSACKReorder` > 0 in REL) are checked before that value is read. If they fail, the cells are `VOID`.

## 9. Abort classes (filled in the Result, in this order)

| token | cause |
|---|---|
| ABORT-LOCK | either lock exists at `test -e`, or `take_lock` refuses |
| ABORT-CRLF | `tools/l1/lib.sh` or any new script holds a CR byte (rule 10) |
| ABORT-TOOLS | iperf3, tcpdump, ethtool, `ss`, or tc's netem distribution loading or `plug` is unavailable (`plug` alone only skips stage O) |
| ABORT-BENCH | R0 refutes a reorderer property (§10.1); no battery is launched |
| ABORT-SYSCTL | a per-namespace readback differs from §4 |
| ABORT-NS | `guard_ns` refused a name (a harness bug; nothing was touched) |
| ABORT-SMOKE | the smoke misses a witness on either end |
| ABORT-BUILD | stage T only: the raptorpath sha differs from the amendment's pin |
| TUN-BRINGUP-FAIL | stage T only: `tun_bulk.sh`'s three attempts exhausted (stage T is skipped, not the session) |
| ABORT-DISK | free space below the session's digest and pcap budget at launch |
| ABORT-HOSTSTATE | at exit, a root-namespace `tcp_*` sysctl differs from pre-flight |
| a non-zero stage exit | |

## 10. Rehearsal, component bench and harness

### 10.1 Session R: component bench R0, A/A calibration and smoke (≈ 1.5 h)

**R0, the reorderer alone (rule 14).** It runs before any TCP is scored. A paced UDP probe stream with a sequence number in each payload runs at g ∈ {1 000, 10 000}, with pre/post pcaps. The bench must show, for each Δt of §5.1 and both modes:
1. each selected packet is delayed by Δt ± max(50 µs, 5 %);
2. in REL, unselected packets are delayed < 50 µs and ≥ 90 % of events have exactly k overtakers (k ± 1 at Δt < 0.3 ms);
3. in HOLD, there are zero overtakers, and the successors leave within 1.1 × their 10 × g serialisation;
4. the selection rate lies within the binomial 99 % interval of p;
5. the t = 3 s table change takes effect, with no event before it;
6. CLEAN has zero delayed packets.

Then, with a TCP flow:
- the per-socket IP ID is consecutive within a flow (for O's selector);
- band-1 `plug` holds and releases as a burst;
- every §6.1 counter name exists.

Any failure is ABORT-BENCH for the stages that need that property. No fallback is improvised: a userspace reorderer would need its own amendment.

**A/A calibration.** Per core configuration and stack, 8 CLEAN reps give σ_G. The amendment then pins MDE_ω per cell (§6.3). If MDE_ω leaves a whole sweep unable to decide anything, the cut order of §11 applies before launch.

**Smoke.** One rep of every arm type (CLEAN, REL, HOLD, OUT-REL, OUT-HOLD, and T's three) on both stacks. Every §6.1 readout and every echo must be present on both ends. Nothing in it is a result.

**The amendment** (committed before session A) pins: the per-rep cost (which re-budgets §11), σ_G and MDE per cell, the measured inner min_RTT for stage T, the raptorpath sha for T, BBR availability, and any cut.

### 10.2 Harness (a Sonnet worker, tools only, before session R; red tests first)

Plan token `omega`. The files follow the `*_run_all.sh` / `*_battery.sh` pair convention of `threadq2_run_all.sh`, all in `tools/l1`:

| file | role |
|---|---|
| `topo_reorder.sh` | the three-namespace (four for T) topology: up/down/status, `guard_ns` on every name, offloads off, per-namespace sysctls with readback |
| `omega_dist.py` | writes the two-point netem tables for (p, Δt), and the all-zero table |
| `omega_cells.py` | pure: the design of §5 as data. Δt, Q, p, the degenerate list, the per-round shuffled order with its seed |
| `omega_battery.sh` | one configuration: bring-up, the rounds, per rep the iperf3, tcpdump, `ss`, `/proc` snapshots and the t = 3 s change; kill by PID |
| `omega_run_all.sh` | the session envelope: locks, sentinels (`DONE-ALL` / `FAILED-ALL`, proven writable at launch), the CR check, pre-flight host-state snapshot, R0, smoke, the stopper, exit-state check |
| `omega_parse.py` | pure: the pcap digest (stdlib pcap reader), the counter diffs, the iperf3 window goodput |
| `omega_score.py` | pure: ω̂, the bootstrap CI, ω_LB, ω_UB^acct, MDE, zones, the §6.4 envelope and VARIES tests, the §7 bands, the §8 outcomes |
| `test_omega.py` | unit tests of the pure files |

- **Unit tests (`test_omega.py`).** Synthetic pcaps with known delays give the right events and overtakers. HOLD with one overtaker is VOID-WITNESS. The zone table of §8 holds at each edge ± 1 %. A curve stepping at k = 3 gives `OMEGA-VARIES-X`. ω_UB^acct is invalid when u_x's upper CI is ≥ 0.05. A cell with 5 live reps is UNSCOREABLE.
- **Stub paths.** These go in `harness_paths.HARNESS_PATHS` and are executed by `test_stubrun.py`, with `ip`, `tc`, `tcpdump`, `iperf3`, `ss`, `ethtool` and `sysctl` stubbed:
  - `rehearse`;
  - `full`;
  - `lock-held` → ABORT-LOCK, with no stub command issued;
  - `sysctl-mismatch` → ABORT-SYSCTL;
  - `bench-refutes` → ABORT-BENCH, no battery;
  - `rel-no-events` → VOID-WITNESS;
  - `hold-reorders` → VOID-WITNESS;
  - `under-min` → UNSCOREABLE;
  - `no-bbr` → SKIPPED-BBR;
  - `ids-not-consecutive` → SKIPPED-SELECTOR;
  - `wide-ci` → NEEDS-MORE-POWER;
  - `zone-straddle` → OMEGA-VARIES-RATE;
  - `steep-auto` → STEEP-AT-Auto;
  - `stopper` → TRUNCATED sentinel;
  - `bad-ns` → `guard_ns` refuses;
  - `hoststate-changed` → ABORT-HOSTSTATE.
- **Static checks (in `test_omega.py`).** Every `ip`, `tc`, `ethtool` and `sysctl` call in the new scripts is wrapped in `ip netns exec rp-…` or `ip -n rp-…`. No `modprobe`, `nft`, `iptables`, `pkill -f` or `killall` appears. Every `kill` takes a recorded PID.
- **Done when** `test_omega.py`, `test_stubrun.py` (FAST) and the existing tools tests pass, and `harness_paths` equals what ran. No engine file changes, so binid is untouched by construction.

## 11. Sessions and VM time

Per rep ≈ 23 s: 18 s of iperf3, plus start, stop and snapshots, plus the pcap digest between reps at 10 000 packets/s. The figure is re-measured in R and re-budgeted in the amendment.

| session | content | reps | ≈ time |
|---|---|---|---|
| R | R0, A/A calibration (8 configurations × 8 CLEAN), smoke | ~110 | 1.5 h |
| A | P at RTT 10, both stacks (64 cells × 8) | 512 | 3.3 h |
| B | P at RTT 50, both stacks (68 cells × 8) | 544 | 3.5 h |
| C | S (44), O (24), T (5), B (5): 78 cells × 8 | 624 | 4.0 h (T's tunnel bring-up included) |
| **total** | | | **≈ 12.3 h, four sessions, each under 5 h** |

- **Cut order, pre-registered** (applied in the amendment if the owner wants less, or if R's re-budget overruns a session):
  1. BBR;
  2. S3 (spacing);
  3. the time cells whose Δt lies within 20 % of a k cell's in the same configuration, REL and HOLD, both stacks: (1 000, RTT 10, t = 1: 2.5 ms = k = 2), (1 000, RTT 10, t = 4: 10 ms against k = 8's 8.5 ms), (1 000, RTT 50, t = 0.25: 3.1 ms against k = 3's 3.5 ms) and (10 000, RTT 50, t = 0.25: 3.1 ms against k = 32's 3.25 ms). That is 16 cells;
  4. n from 8 to 6 in S and O only.

  Items 1–4 save ≈ 2 h, to a total of ≈ 10 h. S3's 1 s arms stay when S3 is cut, as S4's reference.
- **A detached stopper per session** ends the battery at a balanced round boundary, or at the hard deadline (launch + 4.9 h). It stops only the PIDs it recorded and raptorpath by exact name, lets the driver tear down its namespaces, clears both locks, and writes `TRUNCATED-<session>`.
- **No polling (rule 13).** The watcher waits on `DONE-ALL || FAILED-ALL || TRUNCATED-*` only.
- **Exit state, verified:** zero raptorpath processes; none of the recorded PIDs alive; no `rp-*` namespaces; both locks released; root-namespace `tcp_*` sysctls equal to pre-flight.

## 12. What goes back to the owner (with the Result)

- Stack D's ω interval and its zone (§8), with the default change it implies at each preset, if any. Where a change is implied: the choice between accepting it and declaring a larger ω that clears the band.
- Any `STEEP-AT-<preset>`, with the toy P3 check it triggers.
- Any `OMEGA-VARIES-<factor>`: ω is not a constant, so `ordering-hold.md` is revised before anything is built.
- The outage verdict, for owner question 2.
- Stage T's verdict, for whether the netem-only measurement stands for the tunnel.
- C's interval, as the conservative bound.
- What stays blocked either way: C9, C11 and C12's release arm wait on this result and on the toy's rbf and skew readouts (`ordering-hold.md` §10). R1, the ω = ∞ point, is unaffected.
