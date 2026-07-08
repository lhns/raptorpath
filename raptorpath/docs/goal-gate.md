# Goal Gate — surpass the TCP-style baseline + model reacts correctly

Executable form of the project goal, at fidelity level **L0** (in-process
simulation per ADR-0051). Run:

```
cargo test --test gate_suite -p raptorpath --release -- --test-threads 1
```

**Status: GREEN** (2026-07-04, 15/15 tests, 10 trials/cell, fixed seeds,
95%-CI separation required for every win; re-run on the P10a branch —
the inner-feedback floor is weight 0 in the gate driver, so the gate
path is bit-identical).

## Design note — unified sliding-window model (paper §15)

Paper §15 ("The Unified Sliding-Window Model") formalizes that BLOCK and
WINDOW FEC are one sliding-window RLC at two settings of two continuous knobs
(window advance/overlap and repair schedule), with block mode the σ→0
spike-limit of the streaming taper (same pattern as §14.29 / §14.26). It
exists to enable **per-stream triangles**: one tunnel carrying a tight-δ
realtime flow and a loose-δ bulk flow over the same paths simultaneously —
which the current global-per-tunnel mode structurally cannot do. Motivation
is measured here: at C2 the split does not even deliver its intended latency
benefit — block/Bulk holds a 91 ms message p99 while window/Realtime, the
mode whose purpose IS the low tail, sits at 513 ms (L2 workstream 2 below),
for path-specific reasons (508 B window MTU fragmentation, late-maturing NACK
path) the unification erases. Design-only; no code change on that branch.

## G1 — surpass the SimRetx baseline (ADR-0051 win conditions)

Baseline = **SimRetx**, NOT "real TCP": reliable ARQ transport model with
slow-start + AIMD congestion window, min-SRTT multipath scheduling, and
TCP in-order delivery semantics. Channels are the paper §2.4 GE
parameterizations (h_B = 1, so ε = p/(p+q) exactly).

| Cell | Win condition | raptorpath | SimRetx | Verdict |
|------|---------------|-----------:|--------:|---------|
| C1 DC (tie cell) | completion within 2% (+1 RTT allowance), overhead ≤ 1% | 0.025 s, 0.6% | 0.027 s, 0.1% | tie (actually faster) |
| C2 WiFi | compl ≤ 0.9×, p99 ≤ 0.7× | 0.185 s / 28 ms | 0.818 s / 53 ms | 0.23× / 0.53× |
| C3 LTE | same | 0.97 s / 84 ms | 3.76 s / 272 ms | 0.26× / 0.31× |
| C4 Satellite | same | 1.60 s / 356 ms | 23.0 s / 1565 ms | 0.07× / 0.23× |
| C5 Bad WiFi | same | 0.41 s / 34 ms | 2.26 s / 81 ms | 0.18× / 0.42× |
| C7 dual sym | beat dual min-RTT AND best single | 0.110 s | 0.405 s (single 0.867) | 0.27× |
| C8 dual asym RTT | same | 0.177 s | 0.695 s (single 0.816) | 0.25× |
| C9 outage | goodput ≥ 90% steady within 3×RTT of path recovery | 8/10 trials ≤ ~34 ms | — | pass |

## G2 — the model reacts correctly

- Estimator converges to each channel's true (ε, q) (per-symbol GE feed).
- Controller rate re-converges within 25 batches of a 1%→10% regime change
  (measured: 1 batch, BOCD).
- Spare-capacity gate clamps FEC monotonically to spare; zero spare → zero FEC.
- Outage: ε̂ saturates ≤ 10 batches, P_lost → >0.99, recovery ≤ 30 batches.

## What building the gate found and fixed

Production bugs (fixed in src/):
1. **σ²_burst sentinel blow-up** (`fec_rate.rs`, `raptorpath-math`): on very
   clean channels the GE estimator's decayed Bad-state counters empty out and
   `p_bg()` returns its 0.0 no-data sentinel; treating it as a measurement
   made σ² ≈ 2/p̂ ≈ 4000 and over-provisioned DC links ~50×. No data now
   maps to iid (σ² = 1). Paper §8.3 note added.
2. **Reorder-buffer stranding** (`net/reorder.rs`): `drain_expired` advanced
   `next_deliver_seq` past still-pending younger entries — a hole filled by
   FEC just after a later entry expired sat for a full extra timeout.
   Expiring seq k now releases everything pending up to k, in order.
3. **Estimator burst-bias API** (`control/estimator.rs`): `record_batch`
   lumps losses (overestimates σ²_burst ~2×). Added `record_counts` +
   `record_symbol` so SACK-informed callers feed the true loss pattern
   (paper §7.5).

Model/driver findings (documented in the paper):
4. **Continuous r\*** (§8.4): quantile at 1 − δ/ε — rate glides to 0 when
   pure ARQ meets the tail target; no cutoff branch; hint enters only via z.
5. **Steady-state taper shape invariance** (§4.2): aggregate correction
   rate = r regardless of shape; a global-offset τ(t) accumulator (latent
   bench_suite bug) decays to zero and starves repair generation.
6. **Jitter-horizon encoder lag** (§14.24, new): repairs covering
   not-yet-arrived symbols park as deep pivots; lagging the encoder by
   L = J × send_rate symbols made hole-fill ~10× faster.
7. **Pacing**: windowed-count pacing degenerates into RTT-synchronized
   mega-bursts; token-bucket at Copa's cwnd/SRTT keeps the send process
   smooth (and a delay-based CC keeps queues empty — that, not just FEC,
   is half the p99 win vs the AIMD baseline).
8. **Fast path-recovery signal**: blending the GE state-conditional loss
   rate (paper C.6 ε_burst) into path selection detects outage recovery
   within one delivery instead of ~6 estimator batches.

## Quality sweep (protocol hints, diagnostic `quality_hint_sweep`)

Post-improvement numbers (P1-P5 all on; 6 trials, same seeds). Completion =
flow-completion time of the 1.8 MB transfer to application delivery.
Ratios vs SimRetx.

| Cell | Hint | Completion | p50 | p99 | Overhead |
|------|------|-----------:|----:|----:|---------:|
| C2-WiFi | SimRetx | 0.860s | 8.1ms | 46.2ms | 2.5% |
| C2-WiFi | Bulk | 0.163s (0.19x) | 29.5ms | 46.0ms | **3.4%** |
| C2-WiFi | Realtime | 0.187s (0.22x) | 13.4ms | 23.8ms (0.52x) | 20.1% |
| C3-LTE | SimRetx | 3.80s | 28.8ms | 277ms | 5.2% |
| C3-LTE | Bulk | 0.79s (0.21x) | 74ms | 134ms | **6.5%** |
| C3-LTE | Realtime | 0.96s (0.25x) | 35.3ms | 78ms (0.28x) | 30.1% |
| C4-Sat | SimRetx | 22.3s | 367ms | 1556ms | 9.3% |
| C4-Sat | Bulk | 1.22s (0.05x) | 284ms | 410ms | **12.6%** |
| C4-Sat | Realtime | 1.21s (0.05x) | 139ms | 259ms (0.17x) | 41.9% |
| C5-BadWiFi | SimRetx | 2.25s | 14.6ms | 80ms | 17.2% |
| C5-BadWiFi | Bulk | 0.34s (0.15x) | 25.6ms | 46ms | 31.6% |
| C5-BadWiFi | Realtime | 0.39s (0.17x) | 13.2ms | 28ms (0.35x) | 48.5% |

Reading:
- **Bulk now runs at volume parity**: overhead ~= the channel's own loss
  rate (3.4% at eps=2.5%, 6.5% at 4.8%, 12.6% at 9.1%) — the continuous
  r* glides to ~0 steady-state FEC and the completion-tail burst (14.25)
  buys back the last RTTs. Bulk completion BEATS SimQuic on 3 of 4 cells
  (see below).
- **Realtime pays for the tail, as designed**: p99 = 0.17-0.52x of SimRetx
  (0.38-0.47x of SimQuic) at the paper-predicted overhead; the saturation
  cap (14.21) removed the C4 reversal — Realtime == Auto there instead of
  worse.
- Hints now separate cleanly: Bulk = min volume + deep queue, Realtime =
  min tail + near-empty queue, Auto between.

## Improvement ablations (each flag isolated, same seeds)

| Flag | Target metric | Off | On |
|------|---------------|----:|---:|
| P2 estimated_floor | C2 Auto p50 | 17.7ms | 13.0ms |
| P1 hint_delay_target | C2 Realtime p50 | (post-P2) 12.8ms | 12.8ms (no-regression; Bulk trades p50 18.8ms for best completion) |
| P4a bulk_arq_delta | C2 Bulk overhead | 11.0% | 3.3% (completion also improved) |
| P4b tail_fec | C2/C4 Bulk completion | — | -7.7ms / -10.5ms (pre-P6; the P6 χ ramp now subsumes the burst for Bulk) |
| P5 saturation_cap | C4 Realtime p99 | 378.7ms | 332.3ms (reversal vs Auto gone; C2 bit-identical, cap non-binding) |
| P6 completion-exposure δ | wasm Bulk vs old min(0.1, ε̂) | see below | completion -6%/-8%, excess overhead 5.99→0.04% / 8.21→0.91% |
| P7 production Copa-lite port | real-link (C2 netem) tunnel throughput — production scheduler, not the driver | cwnd collapsed 10→2 symbols on the first burst (rate-formula target) | implemented, L1-verified pending |
| P8 block-mode ARQ (paper 14.27) | real-link (C2 netem) tunnel completion — 1.8 MB took ~8 s vs quinn 0.175 s with NO block-mode loss recovery (Bulk mid-stream r*=0 relies on this path existing) | lost symbols waited out the 30 s decoder eviction; inner TCP saw raw 2.6% loss | implemented, L1-verified pending |
| P10a inner-feedback repair floor (paper 14.28) | L1 C2/C3 rp-bulk median completion (TCP-in-tunnel), weight 0 vs 1 | C2 1.179 s, C3 5.34 s (pure glide) | C2 1.192 s, C3 6.82 s — floor verified ACTIVE (+2.2% FEC volume), NEGATIVE result: neutral at C2, −28% at C3; production default stays weight 0 |
| P-CC BtlBw-anchored recovery (paper §12.6, bbr-lessons #1) | L1 C2/C3 rp-native 1.8 MB median completion + does cwnd reach BDP ~160 | C2 0.883 s, C3 10.43 s; cwnd p50 ~80-110 (P9b measured) | C2 0.911 s, C3 10.00 s (flat, within run stdev); cwnd p50 **139** / max 165 (reaches BDP). cwnd deficiency FIXED; completion REFUTED (structural bottleneck). Floor gain converged 1.0→0.85 (drains the standing queue floor 1.0 held). Shipped, gate 15/15 |

Notable: most of the median-latency win came from P2 (the ground-truth
floor hid the jitter bound); P1's hint mapping is retained for semantics.
P4's branch also found and fixed a second ReorderBuffer stranding bug
(late fills below next_deliver_seq).

P6 (completion-exposure δ, paper 14.26) replaces Bulk's δ_eff =
min(0.1, ε̂) with the glide δ_eff = ε̂ + (0.05 − ε̂)·χ, where
χ(T_rem) = Φ̄((T_rem − 1.5·SRTT)/σ_arq) is the probability a loss NOW can
no longer hide behind remaining sends. Mid-stream χ = 0 gives r* = 0
IDENTICALLY (kills the M1 cold-start pin at max_overhead — the old
mapping lost to a fixed r = 0.01 floor in 20/24 wasm grid cells on
completion, 24/24 on overhead — and the M2 permanent FEC leak at
ε ≥ 0.1); the χ ramp over the final ~1.5 SRTT subsumes the P4b one-shot
tail burst as its continuous limiting case. Measured in the wasm sim
(same seeds, `test_ablation_p6_completion_exposure`): ε=0.05/RTT=50
completion 599→562 ticks, excess overhead 5.99→0.04%; ε=0.10/RTT=50
674→620 ticks, 8.21→0.91%; ε=0.05/RTT=150 is a wash (724→720, 16.1→16.2%
excess — a 0.5 s transfer fits inside the χ horizon at 150 ms RTT, so the
cold-start prior governs both arms; documented in 14.26). Bulk now beats
fixed(0.01) on both completion (562 vs 598) and overhead (5.31% vs
6.11%). The production tunnel keeps χ = 0 (endless stream, T_rem
unknown); an idle-onset heuristic is future work.

P7 (production Copa-lite port, paper 12.4 implementation notes) closes
the documented driver-vs-production CC gap: the production scheduler's
Copa used instantaneous dq = RTT − min_RTT with no min-RTT windowing, no
ramp discipline, and no pacing, so on a real emulated link (C2: 100 Mbit,
10 ms RTT, netem) the initial burst inflated its own RTT samples and the
rate-formula target crushed cwnd from 10 to its floor of 2 symbols on
the very first burst (observed: tx_paused=true, in_flight=41, cwnd=2).
The port mirrors the gate driver's semantics in
`src/scheduler/mod.rs` (windowed-min queue signal, 10 s floor window,
hint queue targets 1.08/1.125/1.25, ×1.5+1 → +2/×0.92 two-speed ramp,
cwnd floor 8) and adds token-bucket pacing (cwnd/SRTT, burst
max(10, cwnd/8)) to the interleaver drain in `src/net/mod.rs`. The first
L1 round confirmed the CC fix (cwnd no longer collapses; ACKs flow,
blocks decode) but exposed the batch-granular pacing approximation:
whole 64KB blocks (~56 symbols) overdrafted as one burst, self-queueing
~5.4ms at C2 — above Bulk's 2.5ms backoff threshold — pinning cwnd at
~34. The follow-up made pacing SYMBOL-level (per-path carry queue;
partial sends up to floor(tokens); carried symbols count toward the
TUN gate) and cut the Bulk flush timeout 50ms → 5ms, which had been
serializing block assembly with the CC gate into ~300ms ACK clumps.
The second L1 round (post symbol-pacing) still crawled at ~30 KB/s with
the TUN gate cycling at the 2 s leak-guard cadence: root cause was a
DOUBLE CHARGE of the in_flight budget — Scheduler::schedule charged at
schedule time and the paced drain charged the same symbols again at
wire time, leaking +1 per symbol until the gate jammed and only the
2 s 25% decay let trickles through (also the pre-P7 in_flight=41 at
cwnd=2). Fixed: budget charged once at schedule time; ACK releases go
through a FIFO charge log; stranded charges (lost best-effort ACK
datagrams) expire after max(4×SRTT, 250 ms). Echo-timestamp RTT was
checked and is honest (batches are stamped at wire time, after the
carry). Unit-tested at L0 (C2-loop sim with lossy ACKs must ramp cwnd
past 200 in 5 s and stay ack-clocked; budget conservation; stranded-
budget expiry; paced ramp reaches >100 symbols in 15 SRTTs with no
spurious backoff); L1 throughput verification on the VM pending.

P8 (block-mode ARQ via batch acknowledgements, paper 14.27) implements
the missing retransmission half of the §5 correction model in the
production block pipeline. The receiver already acked every SymbolBatch;
v4 Acks now echo `batch_seq` (protocol version 3 → 4), keying a
sender-side ledger of (batch_seq → path, symbols, send time). A batch is
declared lost on 3 later same-path acks (dup-ACK analogue) or
max(1.5×SRTT, 50 ms) timeout (25 ms sweep task for transfer tails). The
sender retains source data for the last 64 blocks (≤ 4 MB LRU) and mints
FRESH repairs (RaptorQ/RLC — new ESIs past everything sent; RS/METTLE
fall back to exact source resends) sized missing + continuous-fractional
ε̂ margin, charged against the same in_flight/pacing budgets as scheduled
symbols. Repair batches re-enter the ledger (lost repair → next round,
doubled margin, 3 rounds max); the receiver drops symbols for
already-decoded blocks so lost-Ack spurious repairs (~ε̂²) are harmless,
and estimator feeds are unchanged (no double counting). Unit-tested at
L0 (ledger diff incl. mixed-block batches, dup-ACK/timeout legs, lost-Ack
non-amplification, LRU caps, margin math, fresh-vs-resend per backend)
plus end-to-end loss→Ack-diff→repair→decode tests with r = 0 proactive
FEC (`tests/block_arq_recovery_test.rs`); the C2 completion re-measure on
the VM is pending.

## SimQuic (L0.5 adversary)

SimRetx collapses on random loss because AIMD treats channel loss as
congestion. The honest modern adversary is QUIC/BBR-class: **loss-blind**.
`run_baseline_quic` models QUIC-as-deployed at L0 fidelity: single path,
Copa-lite delay-based CC (identical pacing/window structure to the
raptorpath driver — loss never shrinks the window), sender-side SACK-timed
ARQ with RFC 9002 time-threshold loss detection (retransmit after
9/8 × SRTT without delivery, no oracle), and single-stream in-order
delivery. Sanity (`simquic_sanity`): SimQuic completion is < 0.7× SimRetx
on C2/C4 and its retransmit volume tracks ε — the model behaves like a
competent transport, not a strawman.

Measured (gate cells 22/23/25/27/28, 10 trials; sanity cells 6 trials;
completion s / p99 ms):

| Cell | raptorpath (Auto) | SimQuic | SimRetx | rp p99 vs SimQuic |
|------|------------------:|--------:|--------:|------------------:|
| C2 WiFi | 0.187 s / 28.2 ms | 0.172 s / 60.7 ms | 0.818 s / 53 ms | 0.47× |
| C3 LTE | 0.955 s / 82.1 ms | 0.830 s / 215.0 ms | 3.76 s / 272 ms | 0.38× |
| C5 BadWiFi | 0.419 s / 35.6 ms | 0.335 s / 83.3 ms | 2.26 s / 81 ms | 0.43× |
| C4 Sat (sanity) | 1.60 s / 356 ms | 1.446 s / 883 ms | 24.5 s / 1528 ms | — |
| C7 dual sym (completion) | 0.113 s | 0.172 s (single C2) | 0.405 s (dual) | 0.66× compl |
| C8 dual asym (completion) | 0.171 s | 0.170 s (single C2) | 0.695 s (dual) | 1.01× compl |

Reading:
- **The latency claim survives the honest adversary**: raptorpath p99 is
  0.38–0.47× SimQuic's (gate `gate_vs_simquic_p99`, ≤ 0.7×, CI-separated).
  SimQuic keeps queues empty, so its entire tail IS the ARQ head-of-line
  stall (≥ 9/8 SRTT per hole) — exactly what proactive FEC removes.
- **The completion claim mostly does not**: SimQuic runs within ~15% of the
  channel serialization floor, so on single paths it finishes 5–15% sooner
  than Auto-hint raptorpath (the FEC overhead tax). This is the P4 gap:
  Bulk should converge to pure ARQ + tail FEC
  (`gate_vs_simquic_bulk_completion`, ignored until P4 lands).
- **Multipath is the structural win only when the added path adds real
  capacity**: C7 (2× WiFi) completes at 0.66× single-path SimQuic (gate
  bound 0.75×, CI-separated; the 0.6× target is under the physical floor
  ratio once FEC overhead is counted). C8 (WiFi+LTE, +20% capacity) is
  parity (1.01×) — gate bound is no-regression (≤ 1.1×, CI-separated).

## L1 Phase 1 — REAL kernel TCP over netem (first real-world data)

Fedora VM, netns+veth+netem with the paper 2.4 GE parameters verbatim
(tools/l1). transfer_bench.py measures full object delivery (app-level
ack) at microsecond resolution. 1.8 MB objects, 10 runs, seed 42.

| Cell | CUBIC median / max | BBR median / max | Steady goodput C/B |
|------|--------------------|------------------|--------------------|
| C1 DC | 0.027s / 0.15s | 0.028s / 0.028s | 930 / 929 Mbit/s |
| C2 WiFi | 0.64s / 1.25s | 0.22s / 0.92s | 10.5 / 93 Mbit/s |
| C3 LTE | 5.2s / 37s | 1.00s / 1.09s | 1.4 / 18.1 Mbit/s |
| C4 Sat | 52.5s / 191s | 6.9s / 131s | DNF(>20min/10MB) / 11.0 Mbit/s |
| C5 BadWiFi | 131s / 149s (3 runs) | 1.00s / 6.4s | ~0.14 / 9.9 Mbit/s |

Reading:
- **CUBIC's collapse is real and worse than L0 modeled** (L0 SimRetx:
  17-25% of capacity; reality: 10% at C2, 7% at C3, RTO-dominated
  near-zero at C4/C5 — retransmits themselves get lost).
- **BBR validates the SimQuic adversary class** (loss-blind, ~93% of
  capacity at C2) — but its SMALL-OBJECT TAILS explode with RTT x loss
  (C4: median 6.9s, max 131s): serial-ARQ pathology, exactly the tail
  FEC exists to remove. That tail is the L1 target raptorpath must beat.
- C5 CUBIC: median 131 s for 1.8 MB on a 50 Mbit link (0.14 Mbit/s
  effective, 130x slower than BBR) — RTO-dominated at 15% bursty loss.
  Two earlier attempts could not finish 10 objects inside 15 minutes.
  Harness lesson: collapsed CCs need DNF-as-result semantics (sweep_tcp.sh
  and run_cell.sh now record timeouts as results).

## L1 Phase 2 — real QUIC (quinn) + kernel MPTCP

quinn-perf, one warm connection, sequential 1.8 MB requests over 60 s
(mean completion = window/requests; cold-connection TCP numbers above
are not directly comparable — noted). Kernel MPTCP v1 via python
IPPROTO_MPTCP, dual-path topologies per ADR-0051.

| Cell | quinn mean completion | quinn goodput |
|------|----------------------|---------------|
| C1 DC | 0.027 s | 545 Mbit/s |
| C2 WiFi | 0.175 s | 84 Mbit/s |
| C3 LTE | 0.94 s | 15.5 Mbit/s |
| C4 Sat | 1.09 s | 13.5 Mbit/s |
| C5 BadWiFi | 0.46 s | 31.8 Mbit/s |

Real QUIC is the strongest small-object adversary measured: its loss
recovery avoids kernel BBR's serial-tail blowups (C4 1.09 s vs BBR
median 6.9 s) and CUBIC's collapse entirely.

MPTCP (50 MB bulk): C7 dual-WiFi 15.4 Mbit/s vs 10.6 single-path — only
+45% from a second identical 100 Mbit path, because CUBIC subflows
collapse under the 2.6% loss (single-path BBR does 93 Mbit/s!). C8
WiFi+LTE: 12.6 Mbit/s. Kernel multipath does NOT solve lossy-path
aggregation — the structural opening ADR-0051's C7/C8 win conditions
target. Small objects: C7 0.256 s mean, C8 0.64 s.

## L1 Phase 3 — raptorpath itself (bring-up log)

First-ever runs of the production binary over real links. Eight
transport bugs found and fixed during bring-up (see commits b57a202,
804101a): tunnel now stable, zero liveness deaths, blocks decode, CC
ramps (P7 + single-charge fix: 19 blocks/15s vs 8 pre-fix at C2).

Pre-P8 baseline (Bulk, 1.8 MB objects, WITHOUT block-mode ARQ — the
correction model's retransmit half was missing in production):
| Cell | raptorpath median | best baseline (quinn) |
|------|-------------------|----------------------|
| C2 | 7.97 s | 0.175 s |
| C3 | 20.5 s | 0.94 s |

Cause (measured): block mode abandoned failed blocks (BlockResult
false -> stats only); under the P6 Bulk glide r*=0 mid-stream, the
inner TCP saw the raw 2.6-4% loss and collapsed; additionally ~77% of
56-symbol blocks contain a loss at C2, each firing a false congestion
backoff (cwnd suppressed to ~16 vs BDP ~104). P8 (block-mode ARQ via
batch-Ack diff) addresses both.

### Full post-P8 sweep (sweep_l1.sh, 10 objects, seed 42, 2026-07-04)

Median 1.8 MB completion (mean for quinn):
| Cell | CUBIC | BBR | quinn | rp-bulk | rp-realtime |
|------|-------|-----|-------|---------|-------------|
| C2 | 0.24 | 0.22 | 0.20 | 3.21 (was 7.97 pre-P8) | 2.18 mean (P9a) |
| C3 | 6.31 | 1.00 | 0.90 | 9.61 (was 20.5) | 25.5 mean (P9a) |
| C4 | 42.2 | 3.64 (max 125) | 1.30 | 56.5 | tunnel failed |
| C5 | DNF | 0.56 (max 57) | 0.55 | 17.4 | DNF (P9a note) |

HONEST READING (the L1 milestone's central lesson):
- P8 doubled rp-bulk everywhere it runs and made completion consistent,
  and rp-bulk now finishes cells where CUBIC cannot (C5). But the L0
  model-level wins have NOT yet transferred to L1 system-level wins:
  rp-bulk is 10-45x slower than quinn across lossy cells. The gap is
  production data-plane engineering, not the model: block-assembly
  latency + TCP-in-tunnel dynamics + 20 ms reorder buffer + residual
  false congestion backoffs (repair-batch RTT samples) + a young CC.
- rp-realtime (window mode): P9a fixed three stacked bring-up bugs
  (silent task-exit in select! — arq sweep returned instantly in window
  mode and took the tunnel down; backend-switch self-deadlock on
  non-reentrant locks; TUN MTU 1500 vs 508-byte window symbols silently
  truncating TCP segments). Realtime now runs at L1: C2 2.18 s mean
  (beats rp-bulk's 3.21), C3 25.5 s, C5 DNF (TCP-in-tunnel stalls at
  15% loss — FEC-rate work item). Window-mode runtime backend switching
  pinned off until the switch protocol carries seq state (hazard note
  in net/mod.rs). Keeper: every select! task exit is now logged —
  silent exits are structurally gone.
- Baselines reproduce within noise across runs (quinn 0.20 vs 0.175;
  BBR C3 1.00 vs 1.00) — the harness is claim-grade even where our
  numbers are not yet.

P9 roadmap (in order of measured leverage): rp-realtime bring-up;
repair-RTT purity in Copa samples; reorder_timeout_ms reduction/
adaptivity; block size/latency tradeoff at high RTT (56-symbol blocks
serialize 5.4 ms at 100 Mbit); inner-flow interplay study (the tunnel
is TCP-friendly only if recovery latency < inner RTO).

### P9b — closing the C2 gap (2026-07-04, this session)

Measure → hypothesize → fix → measure, one variable at a time, all at
C2 (bulk, 1.8 MB, seed 42, 5 runs each). Sequence of MEASURED causes
and fixes:

1. **Jitter-blind Copa queue signal** (the dominant term). Debug
   counters showed 60% of cwnd updates taking a ×0.92 backoff with an
   empty queue; client cwnd histogram pinned at 8-14 symbols vs BDP
   ~160. Root cause: the queue signal compares a min-of-~10-samples
   statistic against the 10 s min-of-thousands floor — under C2 jitter
   (RTT samples 7-22 ms, floor 7.0 ms, typical window min 12-13 ms)
   that gap is ~5 ms of pure statistics, twice Bulk's 2.5 ms threshold.
   netem's jitter FIFO correlates consecutive samples (raw consecutive
   diffs ~0.85 ms), so no per-sample jitter estimator can bridge it.
   Fix (paper §12.4 "jitter-robust queue signal"): quantile queue floor
   (P10 of window-min history — same statistic as the signal), jitter
   headroom 2×max(sample-level, window-level consecutive-difference
   EWMA), ramp fast-exit needs ≥3 samples. All vanish on clean links.
   3.21 s → **1.42 s median** (backoff rate 60% → ~30%).
2. **Cross-block reordering broke the inner TCP** (delivery contract).
   /proc/net deltas in the client ns: 879 inner fast-retransmits, 733
   SACK-reorder events, 263 DSACKs (spurious) per 3 transfers — block
   mode injected each block on decode, so a block waiting one ARQ round
   was overtaken and the inner TCP saw 64 KB holes. Fix: in-order block
   delivery via reorder buffer keyed by (per-peer sequential) block_id,
   SRTT-adaptive hold 4×SRTT clamp [60,300] ms (must survive TWO ARQ
   rounds — GE burst kills the first repair with ~50% probability).
   Retransmits → ~25/3 transfers, reorder events → 0. Completion flat
   in isolation (TCP waits instead of retransmitting) but unlocks 3.
3. **Interleave depth 4 inflated inner RTT** (latency chain). Depth-4
   interleaving delays every block by 3 block-serialization times; in
   the TCP-in-tunnel closed loop that feeds back (slow TCP → low rate →
   longer block time). With ARQ + in-order delivery covering bursts,
   Bulk default depth 4 → 1: **1.63 → 1.38 s median**; with the 4×SRTT
   hold: **1.17 s**; win-jitter headroom: **1.14 s median / 1.18 s
   mean** (stdev 0.24, min 0.94). Inner TCP now clean (0 reorder, ~2-3
   SACK recoveries per 5 transfers, 0 RTOs).

Net: rp-bulk C2 **3.21 s → 1.11 s median (2.9×)** (final build
confirmation: median 1.11 / mean 1.23 / min 0.944, 16 inner
retransmits per 5 runs); quinn 0.20 s (gap 16× → 5.5×). The ≤1.0 s
P9 target is narrowly missed at the median; individual runs reach
0.89-0.96 s. Stopped per the diminishing-returns rule (last two
bulk-affecting changes < 10% each). Non-findings (measured, ruled
out): RLC decode cost (p50 37 µs, p99 < 1 ms); block size at current
rates (the 5 ms flush already caps blocks at rate×5 ms ≪ 64 KB).

rp-realtime (window mode) at C2 remains erratic: 3.0-5.1 s median
across builds, 380-500 inner retransmits, 4-8 inner RTOs per 5 runs —
its 20 ms static reorder hold sits below one NACK/repair round (the
same delivery-contract bug block mode had). Making the window hold
SRTT-adaptive exposed a LATENT DEADLOCK: the window reorder drain ran
only on symbol arrival, so a held hole could wedge the whole tunnel
(hole → no delivery advance → no WindowAck → sender window full → no
sends → no arrivals → no drain; captured live with ss -ti: inner TCP
lastrcv 174 s). Fixed with a drain timer in the receiver select! (both
modes) + a bare WindowAck on expiry (echo sentinel 0, guarded against
SRTT poisoning). Realtime C2 after: median 1.89 s / mean 2.24 (was
2.18 mean pre-session, but with the deadlock hazard now structurally
gone and inner RTOs 8 → 1). Remaining realtime gap: ~430 inner
retransmits per 5 runs — window NACK repair leaves real holes; window-
mode recovery latency needs its own P9c pass.

What remains for parity with quinn at C2 (~1.14 s vs 0.20 s, in
suspected-leverage order): (a) each GE loss event still stalls delivery
~1 ARQ round (~15-25 ms × ~20 events/transfer) — the paper's §14.26
"mid-stream recovery is free" holds for tunnel throughput but not for
the inner flow's delivery latency; a small mid-stream r_min for
TCP-in-tunnel Bulk is the candidate model revision (unmeasured).
(b) Residual ~30% Copa backoff rate under the jitter wave caps cwnd
p50 at ~80-110 vs BDP 160 (gate-full only 8-24% of ACKs, so (a) binds
first). (c) Inner slow-start: quinn IS the transport; we carry a whole
extra TCP.

### P10a — inner-feedback repair floor (paper 14.28): measured, refuted

Hypothesis (a) above, formalized and tested. Paper §14.28 derives a
mid-stream repair floor r_min for payloads whose delivery latency
feeds back into their own throughput (TCP-in-tunnel): the smallest r
whose residual stall fraction S(r) = ε̂·q̂·T_arq·(1−C(r)) sits within
delivery-jitter noise, with C(r) the §14.14 burst-marginalized
recovery race against the ARQ horizon T_arq = min(1.5·SRTT,
max(0.2 s, SRTT))/t_sym. At the C2 operating point it solves to
r_min ≈ 0.029-0.036 across ε̂ = 2.6-4.5% — the same 0.01-0.04 band the
P6 fixed-floor ablation had pointed at. Continuous in every input,
0 on clean channels, weighted by a new `inner_feedback` input in the
shared `controller_rate` (weight 0 = old behavior bit-identically; L0
gate driver, wasm sim, bench_suite all stay 0 — their payloads ARE the
measured object).

Instrumentation found a real production bug first: the floor never
fired because the estimator had NO local throughput feed — the only
`record_throughput` call took the peer's PathReport value, which is
the peer's own `estimator.throughput()`: circular, so both sides sat
at 0.0 forever and every throughput-gated model term (§14.28 floor,
P5/§14.21 saturation cap, §8.4 burst B/T term) was silently
sentinel-disabled on real links. Fixed: the report task feeds the
achieved send rate (symbols-sent delta per 2 s report interval); the
circular peer-feed is removed (it would mix the reverse direction's
ACK-trickle rate into the data direction's t_sym).

Ablation (1.8 MB × runs, seed 42, fresh topo per arm, sudo-journal
audited for cross-session interference; verified per-arm via resolved
config in /tmp/rp-client.log and client /status FEC counters):

| Cell | weight 0 (pure glide) | weight 1 (floor on) |
|------|----------------------|---------------------|
| C2 median (10 runs) | 1.179 s (mean 1.107) | 1.192 s (mean 1.121) |
| C2 median (5-run repeat) | 0.784 s | 1.158 s |
| C2 client FEC volume | 2.46% (reactive P8 only) | 4.66% (floor active) |
| C2 inner TCP RetransSegs / 5 runs | 11 | 21 |
| C3 median (5 runs) | 5.34 s (mean 5.90) | 6.82 s (mean 7.00), +28% |

NEGATIVE, and informative: the floor verifiably fires and pays its
budget, yet completion is flat at C2, the inner loss-recovery
signature does not shrink, and C3 regresses 28%. Post-P8 + P9b the
inner TCP absorbs the residual ~20-60 ms stalls (its RTO floor is
200 ms; the in-order hold already smooths reordering), so hypothesis
(a) is now RULED OUT (measured): the remaining C2 gap belongs to (b)
the Copa backoff ceiling and (c) inner slow-start. Floor repairs also
displace source symbols in the same inner-limited closed loop —
§14.21's dilution cost made system-visible, which is the C3
regression. Production default: `inner_feedback_weight = 0` (knob kept
for genuinely stall-brittle payloads; measure before enabling). Full
derivation + refutation: paper §14.28.

### P-CC — BtlBw-anchored recovery (paper §12.6): cwnd deficiency fixed, completion refuted

Directly tests loose end (b) from P10a ("the remaining C2 gap belongs to
the Copa backoff ceiling"). Proposal #1 of `docs/research/bbr-lessons.md`:
`max_bw` (a delivery-rate max-filter) and `min_rtt` were tracked but fed
only the diagnostic `copa_target_cwnd()`. Now their product is an active
**BtlBw×RTprop = BDP anchor** with two effects on the post-backoff
trajectory: (1) a continuous proportional recovery pull toward BDP
(`cwnd += max(2, α·(BDP−cwnd))`, α=0.25, decaying into the +2 probe as
cwnd→BDP — no discrete phase), and (2) a cwnd **floor** at
`ANCHOR_FLOOR_GAIN×BDP`. Floor, NOT cap: `record_delivery` is coarse
ACK-batch sampling with no app-limited detection, so `max_bw`
structurally underestimates a warm-up-limited flow — the anchor is
therefore gated on ≥8 delivery samples + a min-RTT sample and is only
ever allowed to RAISE cwnd, never suppress it.

Measured (rp-native `perf`, no inner TCP — isolates the wire CC from the
tunnel/inner-TCP pipeline; 1.8 MB, seed 42, 10 runs, fresh topo per arm;
anchor presence confirmed via `strings` on the binary + `bdp_anchor=`
trace fields, and a stale-mtime rebuild trap caught and fixed — cargo had
skipped the recompile until the shipped sources were `touch`ed):

| Metric | BASE (33d5a79) | FLOOR gain 1.0 | FLOOR gain 0.85 (shipped) |
|--------|---------------:|---------------:|--------------------------:|
| C2 median | 0.883 s | 0.901 s | 0.911 s |
| C2 mean (stdev) | 0.847 (0.170) | 0.888 (0.172) | 0.890 (0.231) |
| C3 median | 10.43 s | 9.12 s (1 DNF) | 10.00 s (0 DNF, stdev 1.41) |
| C2 cwnd p50 / max (trace) | ~80-110 (P9b) | 126 / 194 | **139 / 165** |
| C2 bdp_anchor p50 / max | — | 126 / 194 | 137 / 164 |
| C2 above-target update frac | — | ~100% | 52% |

**cwnd reaches BDP — YES.** Post-change cwnd sits at p50 139 (up from the
P9b-measured 80-110), tracking bdp_anchor 137, with peaks at 165 — at/above
the BDP ~160 target. The mechanistic deficiency in bbr-lessons #1 is real
and now closed.

**Completion — REFUTED (flat).** C2 medians 0.883 / 0.901 / 0.911 s are all
inside one run-to-run stdev (~0.2 s ≈ 22%); each configuration step moves
the C2 median <10% (base→1.0 +2%, 1.0→0.85 +1%) → **converged** by the
stop rule. C3 is flat within its large variance. This is exactly the
prediction bbr-lessons #1 made against its own proposal: "bounded on the
C2 1.8 MB headline (gate binds 8-24% of ACKs; structural term dominates)."
Driving cwnd to BDP does not move the 1.8 MB completion because that
metric is bottlenecked by the tunnel pipeline / inner-flow warm-up (L2 ws3
fair-geometry), not by the congestion window. A refutation of the
*completion* leverage, like P10a — not a refutation of the mechanism.

**Floor-gain iteration.** At gain 1.0 the floor pinned cwnd exactly at
bdp_anchor even while the delay signal reported queue-above-target
(`above=true` on ~100% of updates): the floor was maintaining a ~16 ms
standing queue the backoff could no longer drain. Gain 0.85 leaves the
delay backoff ~15% of authority around BDP (above-target fraction
100%→52%, so the queue drains on half the updates) at identical completion
and with the C3 DNF gone and variance cut — shipped default. The recovery
pull (gain 1.0) still re-fills toward full BDP each clean update, so cwnd
oscillates just under the pipe rather than in standing bufferbloat.

**Kept** despite the completion refutation: it fixes a real, documented
deficiency (cwnd now reaches BDP), is safe by construction (floor-not-cap,
underestimate-tolerant, gated), keeps the gate at 15/15 and lib+CC tests
green, and its expected payoff is the sustained-throughput / multipath-
aggregation cells (C7/C8 `B_eff` reads cwnd/SRTT) rather than the 1.8 MB
completion headline. Full derivation + honest constants: paper §12.6.

## L1 convergence assessment (end of P10, 2026-07-04)

Combined-state verification at C2 (merged P10a+P10b, 5x1.8MB, seed 42):
rp-bulk 1.05 s median (13.5 Mbit/s), rp-realtime 1.17 s median. Session
trajectory at C2: bulk 7.97 -> 3.21 -> 1.11 -> 1.05 s; realtime
dead -> 2.18 -> 1.57 -> 1.17 s. quinn: 0.20 s.

The improvement loop (P6-P10) is at measured convergence for this
iteration:
- Every hypothesis with projected >10% leverage has been implemented
  and measured; the last model candidate (P10a r_min) was derived and
  REFUTED by measurement (recorded in paper 14.28).
- The remaining C2 gap decomposes into (a) inner-TCP slow-start over
  the tunnel vs quinn's warm native QUIC connection — structural for
  the TCP-in-tunnel measurement geometry, not a protocol defect: the
  fair comparison is a warm inner flow or an rp-native object API
  (future work, requires new instrumentation); (b) the Copa backoff
  ceiling (~30% residual backoff rate) — next measurable candidate,
  but P9b/P10 data show the gate binds only 8-24% of ACKs, so its
  leverage is bounded well under the structural term.
- Loss-recovery is no longer the bottleneck in either mode (bulk: 25
  inner retx / 5 transfers; realtime: 38, zero RTOs).

Claim status vs ADR-0051 at L1: raptorpath completes where CUBIC
cannot (C5), beats CUBIC on lossy cells, and its two modes now behave
as the model predicts (realtime ~ bulk at C2 with far fewer inner
retransmits). It does NOT yet beat BBR/quinn at L1 on completion time;
the L0 gate wins (vs SimRetx/SimQuic models) stand, with the L0->L1
transfer documented honestly above. Ten latent production bugs plus
two dead subsystems (block ARQ, window reactive repair) were found
ONLY by the L1 harness — the milestone's core value.

## L2 workstream 1 — multipath at L1 (first measurement)

50 MB goodput, seed 42, 2 runs:
| Topology | rp dual | rp single (fast path) | kernel MPTCP dual | MPTCP single |
|----------|---------|----------------------|-------------------|--------------|
| C7 WiFi+WiFi | **23.9 Mbit/s** | 14.0 | 15.4 | 10.6 |
| C8 WiFi+LTE | 8.81 | 14.0 | **12.6** | 10.6 |

- **C7 VALIDATED**: aggregation 1.71x over own single path (MPTCP: 1.45x),
  +55% over kernel MPTCP on the identical topology. The structural claim
  holds where paths are symmetric.
- **C8 REFUTED (per-symbol striping scheduler)**: the slow lossy LTE path
  dragged the aggregate BELOW the fast path alone. Fixed by the
  improvement cycle below.

### L2 ws1 improvement cycle — C8 asymmetric regression (2026-07-04)

Mechanism (measured, per-block debug instrumentation; not hypothesis):
per-symbol striping put 27% of source on the slow path and striped 15%
of blocks across BOTH paths; a striped block completes at the MAX over
the paths it touches, and its losses recover at the slow path's own RTT
(per-BLOCK loss probability 1-(1-eps)^K = 0.94 at K=56, eps=4.8% — the
per-symbol E_i undercounts ~10x). Striped blocks: mean 189 ms vs
17.5 ms A-only; 92% of P9b in-order head-of-line waits were caused by
slow-path blocks; 151 holds per 100 MB expired the 300 ms cap and were
force-delivered as inner-stream HOLES (inner TCP retransmit/cwnd
collapse). Paper verdict: the 13.8 objective itself was blind — its
latency term composes linearly, which silently assumes independent
per-symbol delivery; under block decode + cross-block in-order release
it must be evaluated per DELIVERY UNIT. Section 13.8 extended
(in-order delivery coupling: block-granular y_i, per-block delivery
time D_i with the P_blk ARQ term, hold-horizon eligibility constraint
D_i - D_min <= H/4); production implements the extension.

Ablation (C8 = c2+c3, seed 42, one arm at a time, same binary):

| arm | 50 MB x2 mean | 1.8 MB x5 median | B src share | striped blocks | hold expiries/100 MB |
|-----|---------------|------------------|-------------|----------------|----------------------|
| per-symbol striping (ablation flag) | 9.82 Mbit/s | 3.07 s | 26.7% | 14.6% | 151 |
| + block-granular affinity (WRR on B_eff) | 11.39 | — | 13.6% | 0% | 118 |
| + HOL eligibility (per-block D_i, EWMA loss) | ~12.5 (dbg) | — | 12.0% | 0% | 102 |
| + long-run loss + strict eligibility (final) | **12.61** | **1.15 s** | 6.1% (warm-up only) | 0% | 96 |

(original striping baseline at beae05b: 8.81; the striping arm above
additionally carries the PMTU-shrink reroute fix, which is orthogonal:
quinn PMTUD blackhole suspicion shrank a path's datagram limit
mid-flight and 529 already-encoded symbols per run were dropped as
"datagram too large", orphaning whole blocks — they are now rerouted
to the widest live path.)

Regression checks: C7 (c2+c2) 23.28 Mbit/s (was 23.9, gate >= 22 OK);
gate_suite 15/15 release; cargo test --lib 224 green.

Honest verdict on C8: 12.61 reaches kernel-MPTCP-dual parity (12.6)
and closes 43% of the gap from 8.81, but stays ~10% below rp's own
single fast path (14.0). With the in-order delivery contract, the
extended model says the slow path's optimal SOURCE share at these
parameters is ~zero (hold-infeasible: D_B - D_A ~ 130 ms > H/4); the
tunnel converges to fast-path source + slow-path repair/retransmit
diversity. The residual gap vs single-path is warm-up blocks admitted
before the loss posterior stabilizes (6.1% of source) plus the
remaining ~96 tail expiries. MPTCP aggregates at C8 (12.6 > its 10.6
single) because its receiver tolerates cross-subflow reordering in the
SAME sequence space — an option the tunnel's inner-TCP delivery
contract forecloses. Candidate next lever (unmeasured): distinct
delivery contract for bulk (deadline-aware hold release past
slow-path blocks) — rejected for now, it reintroduces inner reordering
that P9b measurably fixed (879 spurious fast-retransmits per 3x1.8MB).

**Sharpened C8 claim (paper §16, Fountain Multipath Aggregation).** The
in-order hold IS why we only tie MPTCP: paper §16.2 makes it a theorem —
under in-order delivery the aggregate is bounded by the order-ELIGIBLE
paths' goodput (T_inorder ≥ K/Σ_{i∈E} g_i), and on C8's heterogeneity
E collapses to {fast path}, i.e. K/g_A = fast-path-alone (12.6–14.0). No
in-order schedule can beat 14.0 here; MPTCP hits the same wall. The
unlock is OUT-OF-ORDER fountain delivery (§16.1): pour rateless RaptorQ
symbols across ALL paths, decode on K·(1+φ) symbols TOTAL, completion
T_fountain = K(1+φ)/Σ_i C_i(1−ε_i) with no dependence on the slow path's
RTT and no HOL coupling — so the LTE path's g_B ≈ 19 Mbit/s is recovered
instead of forfeited. This requires dropping the P9b hold, which the
NATIVE object API (perf/MemTun, no inner TCP) already permits (a
TCP-in-tunnel cannot — documented boundary). Bulk's optimal code is
therefore PATH-COUNT dependent (§16.4): N=1 → pure ARQ (r*=0, the §14.26
glide); N≥2 heterogeneous → rateless fountain. Proving experiment (§16.6):
`raptorpath perf` native object over C8, out-of-order, target completion
goodput > 14.0 (fast-path-alone) → toward Σ; vs current in-order 12.6 and
MPTCP 12.6. Pass = strictly beats 14.0 (which in-order provably cannot).

## L2 workstream 2 — small-message latency percentiles (first tail data)

stream_bench.sh: 1200 B messages at 50/s for 30 s, one-way delivery
latency (shared kernel clock), seed 42.

C2 (100 Mbit, 10 ms RTT, GE 1.3%/50%):
| stack | p50 | p90 | p99 | p999 | max (ms) |
|-------|-----|-----|-----|------|----------|
| cubic | 9.1 | 120 | 13,252 | 13,452 | 13,471 |
| bbr | 8.2 | 74.6 | 13,426 | 13,669 | 13,689 |
| rp-realtime | 8.6 | 15.6 | 513 | 727 | 747 |
| rp-bulk | 12.3 | 14.8 | **91** | 173 | 187 |

**HEADLINE: the tail claim is VALIDATED vs kernel TCP at C2** — both
kernel CCs suffer 13-SECOND p99 tails (RTO cascades under GE bursts);
raptorpath holds 91-513 ms at equal p50: 26-147x. This is the model's
thesis measured on real stacks. (The quinn message-tail comparison — the
stronger claim, rp beats real QUIC's tail, not just kernel TCP — is now
measured directly; see "quinn message-tail vs raptorpath" below.)

C3 (20 Mbit): bbr p99 198 ms vs rp-bulk 569 ms — BBR WINS; tunnel
block latency dominates at low rate. C5 (15% loss): rp-bulk breaks
down (24 s tails, 359 messages missing). rp-realtime stream runs
produced NO summary at c3/c5 (silent failure — open item, echoes the
earlier C5-realtime DNF). Honest scope: the tail win is demonstrated
where capacity headroom exists; low-rate and extreme-loss cells need
the block-latency and FEC-rate work items.

### quinn message-tail vs raptorpath (2026-07-04)

Completes the strongest form of the tail claim: rp beats REAL QUIC's
tail, not just kernel TCP. `msg_lat` (a quinn *example* built on the VM
from the same proven quinn checkout as quinn-perf — source archived at
tools/l1/quic_msg_lat.rs, runner tools/l1/quic_stream_bench.sh) is the
QUIC analogue of transfer_bench.py's stream mode: 1200 B messages at
50/s for 30 s over ONE ordered, reliable QUIC stream (QUIC's own loss
recovery = the fair comparison for rp's in-order delivery), each carrying
an 8-byte CLOCK_REALTIME send stamp; one-way latency over the shared
kernel clock. Direct QUIC over the netem veth (server in rp-srv bound
10.77.0.2, client in rp-cli) — the SAME geometry and parameters as the
kernel-TCP and rp stream runs. seed 42.

Message-latency percentiles (ms), one QUIC stream, all 1500 messages
delivered:
| Cell | quinn p50 | quinn p99 | quinn p999 | quinn max |
|------|----------:|----------:|-----------:|----------:|
| C2 WiFi (1.3% GE) | 6.2 | 2824 | 3010 | 3017 |
| C3 LTE (2% GE) | 22.0 | 1393 | 1474 | 1480 |
| C5 BadWiFi (5.3% GE) | 8.6 | 45,152 | 45,349 | 45,365 |

Tail (p99, ms) side by side with the recorded rp + kernel-TCP stream runs:
| Cell | cubic | bbr | quinn | rp-realtime | rp-bulk |
|------|------:|----:|------:|------------:|--------:|
| C2 | 13,252 | 13,426 | **2824** | **513** | **91** |
| C3 | — | 198 | 1393 | (silent fail) | 569 |
| C5 | — | — | 45,152† | DNF | 24,000† (359 msg missing) |

† C5: both reliable stacks melt down at 5.3% burst loss. quinn delivers
all 1500 but a single GE-burst hole head-of-line-blocks the ordered
stream for tens of seconds (p90 already 42.8 s); rp-bulk drops 359
messages with 24 s tails. This is the ARQ/HOL pathology proactive FEC
exists to remove, at a loss level past both stacks' current headroom.
(With a shorter idle timeout quinn instead sheds the stuck tail: 1290/1500
delivered, p99 2.7 s — same pathology, reported either as a latency
cliff or a delivery cliff.)

**VERDICT — does rp-realtime/rp-bulk beat quinn's tail?**
- **C2 (headline): YES, decisively.** Real QUIC's p99 is 2.82 s — 4.7×
  better than kernel TCP's 13.3 s (QUIC's loss recovery avoids the RTO
  cascades), but it STILL suffers multi-second in-order-delivery tails
  under GE bursts. rp-realtime p99 513 ms = **0.18× quinn (5.5× lower)**;
  rp-bulk p99 91 ms = **0.032× quinn (31× lower)**, at equal p50. The tail
  claim now stands against the strongest real adversary, not just kernel
  TCP.
- **C3: rp-bulk beats quinn** (569 ms vs 1393 ms, 2.4×) but NEITHER beats
  kernel BBR (198 ms) — at 20 Mbit the tunnel's block latency dominates,
  and quinn's own tail is worse than BBR's here too. rp-realtime C3 data
  is still missing (the open silent-failure item).
- **C5: no winner** — both reliable stacks break down at 5.3% burst loss
  (quinn 45 s HOL cascade / rp-bulk 359 dropped). Extreme-loss cells
  remain the FEC-rate / block-latency work item for both.

Bottom line: the model's central tail thesis is validated at C2 against
real QUIC (rp 5.5–31× lower p99); it holds partially at C3 (bulk beats
quinn but not BBR) and not yet at C5.

## L2 claim table (re-issued, fair geometry — 2026-07-04)

1.8 MB objects at C2, median, matched geometry where possible:
| stack | cold conn | warm conn |
|-------|-----------|-----------|
| quinn (native QUIC) | — | 0.20 s |
| kernel BBR | 0.22 | 0.166 |
| kernel CUBIC | 0.24 | — |
| rp-bulk (tunnel) | 1.05 | 1.02 |

Geometry finding: warm vs cold moved BBR 25% but rp only ~3% — the
inner-TCP cold-start term was NOT rp's bottleneck; the residual ~5x is
the tunnel's own pipeline (residual Copa backoff rate ~30%, block
pipeline latency). The prior "structural" scoping is CORRECTED
accordingly: an rp-native object API remains the right fair-geometry
endpoint, but the pipeline gap is real and owned.

CLAIM STATUS after L2 (all L1/L2, real stacks, reproducible):
1. Multipath aggregation (C7 symmetric): VALIDATED — 23.9 Mbit/s vs
   kernel MPTCP 15.4, aggregation 1.71x vs MPTCP's 1.45x.
2. Asymmetric multipath (C8): kernel-MPTCP PARITY (12.61 vs 12.6)
   after the 13.8 order-coupling extension; full aggregation above the
   fast path is foreclosed by the inner-TCP in-order contract
   (model-scoped, paper-documented).  Native coded-window aggregation was
   attempted and REFUTED at L1 (coded-only ×0.26); the corrected temporal
   oracle (below, "Corrected Oracle / Final Aggregation Verdict")
   reproduces that refutation and shows the fix is generation-based coding
   with a stable anchor (oracle ×1.19, no drag) — ACHIEVABLE, a build
   recommendation, not yet built.
3. Tail latency (the model's thesis): VALIDATED vs kernel TCP at C2 —
   p99 91 ms (bulk) / 513 ms (realtime) vs 13,300-13,400 ms for BOTH
   kernel CCs at equal p50. Open: quinn message-tail comparison
   (needs a QUIC echo tool), C3/C5 tails not yet won, realtime
   streams silently fail at c3/c5 (open diagnostic).
4. Object completion vs modern stacks: NOT YET WON at L1/L2 (5-6x to
   quinn at C2 even warm); the improvement loop owns the pipeline gap.
5. Where TCP dies, rp lives: C5 objects complete (17.4 s) where CUBIC
   DNFs; message streams at C2 stay functional where kernel TCP
   spends 13 s per retransmission cascade.
## L2 workstream — visualizer-driven model refinements (2026-07-04)

Two MODEL questions raised from the interactive visualizer, both fixed
continuous (no hard cutoffs) per project convention. Gate re-run GREEN
(15/15 release) — the saturation change is in the shared controller.

1. **Soft saturation (paper §14.21.1).** The saturation cap was a hard
   `min(r, r_sat)` — a kink, shown as a binary "CAP BINDING" badge.
   Physically the p99 curve has a smooth interior minimum, so the cost of
   exceeding r_sat grows continuously (queue-delay-like), not as a wall.
   Replaced with a one-sided softplus cap
   `r_eff = r_sat − s·softplus((r_sat − r)/s)`, `s = 0.1·r_sat`, which
   approaches r_sat asymptotically, never crosses it, is ≤ min(r, r_sat)
   everywhere, and → the old hard min as s → 0. Its derivative complement
   `σ((r − r_sat)/s)` is a continuous **saturation pressure** ∈ [0,1] (0
   slack, ½ at r_sat, →1 held) that supersedes the binary badge. The
   smoothing width is a deliberately narrow honest constant, NOT the
   model's curvature: §14.21 distrusts the model's DEPTH, so a
   curvature-derived width would over-soften the gate-validated cap. C4:
   hard cap emitted exactly 0.400, soft cap emits 0.398 at pressure 0.90 —
   same ceiling, now continuous. Shared `controller_rate`, so production +
   visualizer inherit; `get_saturation_pressure` added to the wasm sim.

2. **End-of-stream taper completion (paper §4.2 note + §14.29).** The
   taper's forward integral is TRUNCATED at the stream end — a symbol at
   distance j < W from the last source symbol misses ~r·(W−j) of its
   steady repairs (no future source symbols → no future debt), so the last
   window suffers a serial-ARQ latency cliff. §14.26's χ glide fixed this
   for Bulk only; Auto/Realtime relied on the ad-hoc one-shot burst
   (§14.25). Generalized to ALL hints: meter the SAME budget B_tail =
   r_tail·W (exact-DP rate at the hint's own δ) as a Stieltjes measure
   `B_tail·dχ_trunc` over a SOURCE-POSITION kernel
   `χ_trunc = Φ̄((remaining − W/2)/(W/4))` concentrated on the truncated
   region — the burst is its σ → 0 limit. Wall-time spreading (Bulk's χ)
   would dilute the final window and regress it (measured 27 → 49 ms), so
   the truncation term is source-position, distinct from Bulk's wall-time
   economics χ. Wasm sim: last-window p99 now ≈ mid-stream (no cliff) for
   auto (27 vs 29 ms) and realtime (25 vs 29 ms); vs the burst it is at
   parity at RTT 50 and IMPROVES at RTT 150 (80 → 76 ms, the burst's
   single-window coverage gap) for ≤ 2% overhead. Production window mode is
   unchanged (endless stream → χ_trunc = 0; tail holes close via
   NACK/tail-sweep, §14.27). The gate driver keeps its already-paced
   end-of-stream burst (n_pre + n_tail over pacing tokens) as the discrete
   limiting case of the ramp; the wasm sim carries the reference continuous
   implementation.

## L2 workstream 3 — rp-NATIVE object geometry (the fair-geometry verdict)

`raptorpath perf`: objects straight over the transport (FEC/ARQ/CC via a
memory TUN), NO inner TCP, NO kernel TUN — the true apples-to-apples
endpoint vs quinn-perf. C2, bulk, seed 42:

| object | rp-native | reliability | vs tunnel | vs quinn |
|--------|-----------|-------------|-----------|----------|
| 4 KB | 0.024 s | 5/5 | — | — |
| 100 KB | 0.094 s (8.5 Mbit/s) | 5/5 | — | — |
| 500 KB | 0.170 s (23.5 Mbit/s) | 5/5 | — | — |
| 1.8 MB | 0.83 s mean, 10/10 (17.3 Mbit/s) | 10/10 (was "see bug", now fixed) | tunnel 1.05 s | quinn 0.20 s |

**THE VERDICT (why this exercise existed):** removing the inner TCP moved
the 1.8 MB completion only 1.05 s -> 0.92 s (~13%). The remaining ~4.5x
to quinn is the rp PIPELINE itself — block-assembly latency, CC ramp,
decode — NOT measurement geometry. This is the SAME conclusion the
warm-flow test reached (warm barely moved rp), now confirmed a second,
independent way (native transport). The "5x gap is geometry" hypothesis
is REFUTED; the gap is real and owned. Encouraging signal: native
goodput climbs with object size (8.5 -> 23.5 Mbit/s at 100 KB -> 500 KB)
as the engine fills, so the pipeline's fixed per-object latency (warm-up
+ first-block) is the dominant small-object cost.

**Bug found by the native harness (real transport, not a harness
artifact) — RESOLVED (2026-07-04, branch fix/block-idle-tail):** large
objects (>~500 KB) probabilistically STALLED when the sender idled after
the final chunk. 500 KB reliable 5/5; 1 MB flaky; 1.8 MB usually stalled
on run 2. Small/medium objects and all streaming were unaffected.

ROOT CAUSE (measured, instrumented at L1 C2): NOT a tail-flush/ARQ gap —
the batch ledger and its 25 ms sweeper (a separate task, fires while
idle) both work. The real cause is a **lost BlockStart datagram**. A
block's BlockStart is a single best-effort datagram; if it is lost but
the block's symbols are delivered, the receiver buffers those symbols
pre-decoder (`pre_start_symbols`) AND acks every batch anyway. Those acks
clear the sender's ARQ ledger, so neither the dup-ack diff nor the tail
sweep ever fires — yet the block never decodes (no decoder without its
params). The block is orphaned with an EMPTY ledger. With 28 blocks per
1.8 MB object at ~2.5 % datagram loss, P(some block loses its BlockStart
while its symbols survive) ≈ 50 %, matching "usually stalls on run 2".
TCP-in-tunnel masked it by never idling (continuous ACK traffic).

FIX (P8 idle re-announce, §14.27): a **send-idle re-announce** driven by
the existing 25 ms sweeper timer (fires during idle, not gated on TUN
reads). While any block is still retained (not `on_block_done`) and quiet
past ~max(1.5·SRTT, 50 ms) (cadence clamped ≤200 ms so an inflated SRTT
cannot stall recovery), it re-sends BlockStart + a capped-geometric spare
of fresh rateless repairs (round 0 = ε̂ probe → the receiver replays its
full pre-start buffer and decodes the common pure-orphan case; ramps to a
deficit-covering resend, per-round burst capped so a constrained path is
not flooded). Bounded by `MAX_REANNOUNCE_ROUNDS`. Two supporting fixes:
(1) the receiver re-acks (BlockResult success) a re-announced BlockStart
for an already-delivered block — recovers a lost success-ack and prevents
a zombie decoder being re-created; (2) reannounce cadence clamp.

VERIFIED at L1 (seed 42, rp-native `perf`, bulk):

| cell | object | runs | DNF | mean / median |
|------|--------|------|-----|---------------|
| C2 (100 Mbit, GE 1.3/50) | 1.8 MB | 10 | **0** | 0.83 s / 0.88 s |
| C2 | 500 KB | 5 | **0** | 0.27 s / 0.26 s |
| C3 (20 Mbit, GE 2/40) | 1.8 MB | 10 | **0** | 7.30 s / 4.57 s |

Pre-fix: C2 1.8 MB DNF'd ~1/2 runs; C3 1.8 MB DNF'd. C2 numbers unchanged
by the fix (recovery only engages on the rare orphan). C3 is throughput-
constrained by the cell itself (kernel CUBIC there: 1.4 Mbit/s, 5.2 s
median; rp-native max here 31.8 s on the worst run, still 0 DNF) — the
deliverable was 0 DNF, now met. Regression guard:
`block_arq` unit tests `idle_reannounce_recovers_orphaned_block` and
`idle_reannounce_bounded_by_round_cap` reproduce the empty-ledger orphan
and assert bounded recovery.

## L3 — realtime path "death" RESOLVED: fatal TUN write, not keepalive (2026-07-04, branch fix/window-liveness)

The user flagged that rp-REALTIME's message-tail p99 (513ms at C2) was
WORSE than rp-BULK's (91ms), with the client logging `path timed out —
marking inactive` ~10s after the handshake. The earlier L3 note
hypothesised this was a window-mode KEEPALIVE-DELIVERY bug (the server's
PathReport/Ping not reaching the client). **That hypothesis was wrong.**

MEASURED (RUST_LOG, VM logs) — the real root cause is a fatal TUN write:
the server's own log, at the moment of failure, is

    TUN write error e=Os { code: 22, kind: InvalidInput }   (EINVAL)
    TUN inject channel closed
    tunnel task exited — shutting down tunnel  task="receiver"

i.e. the SERVER tunnel dies FIRST; the client's `path timed out` is a
downstream symptom logged ~6-7s later (DEAD_PATH_TIMEOUT), correctly, on
a peer that has genuinely gone silent. The keepalive/liveness plumbing
(report task, ctrl fast-path, touch_path on PathReport/Ping/WindowAck) is
shared between modes and works — hypotheses (a)-(d) were all disproven by
reading + measurement.

The mechanism (tun/linux/mod.rs, tun/windows/mod.rs): the window+packing
(Realtime) delivery path OCCASIONALLY hands the kernel TUN a malformed
packet (a mis-framed / mis-decoded FEC symbol). The kernel rejects the
write with EINVAL; the old write loop `break`ed on ANY write error, which
dropped inject_rx, closed the receiver's inject channel, and tore down
the WHOLE tunnel. Block mode never hit this (it delivers clean decoded
blocks), so it showed 0 deaths — which is why the bug looked
window-specific. It is intermittent (only when a malformed packet
occurs) and catastrophic (the entire tunnel dies).

FIX (principled, minimal): a single bad inner packet must never tear down
the tunnel. The TUN write loop now (1) drops packets that don't look like
IP (version nibble + min header length) before writing, (2) on a write
error logs and CONTINUES rather than breaking, and (3) only gives up
after 64 CONSECUTIVE failures (device genuinely gone). Applied to both
the Linux and Windows writers; guarded by `looks_like_ip` unit tests.

RESULT at C2, 50 msg/s, 400 B (before = base 651f93c, after = fixed):
- **Path deaths: 0/25 reps after** (10 realtime + confirmed structurally
  impossible); the catastrophic death is eliminated. Every run delivers
  1000/1000 messages; the tunnel never dies.
- Bulk (block mode) unaffected: 5/5 reps, 1000/1000, 0 deaths.
- C3-LTE (the earlier "silent failure" cell): now delivers — 3/3 reps
  1000/1000, p99 110-282ms.

HONEST correction to the earlier note: the run-to-run p99 SWINGS
(before: 38-549ms; after: 38-226ms, with a rare multi-second outlier)
are NOT the path death — they are inner-TCP HEAD-OF-LINE recovery stalls
in window mode, present in BOTH before and after and in bulk (97-373ms)
too. The stream is kernel TCP, so one lost/corrupt segment stalls all
later messages until RTO recovery. That tail is the P10b reactive-repair
territory (a separate issue), NOT the liveness bug. What this fix removes
is the CATASTROPHIC failure mode (whole-tunnel death → connection lost),
not the ordinary loss-recovery tail.

## L3 REGIME MAP — where raptorpath wins, ties, loses (vs best-of-baseline)

Synthesis of all L1/L2/L3 measurements. Each cell marks raptorpath vs
the BEST of {quinn, kernel BBR, kernel MPTCP, CUBIC} for that metric.
Cells: loss/RTT per paper §2.4 (C1 DC 0.05%/1ms → C5 BadWiFi 5.3%/10ms,
C4 Sat 3%/100ms; C7/C8 dual-path).

### Metric A — message TAIL latency, single path (p99, 50 msg/s stream)
| cell | rp p99 | best baseline | verdict |
|------|--------|---------------|---------|
| C2 WiFi | 44–226 ms (realtime) / 91 ms (bulk) | quinn 2824 / BBR 13,400 | **rp WINS 12–60×** |
| C3 LTE | 569 ms (bulk) | BBR **198** (quinn 1393) | rp LOSES to BBR, beats quinn 2.4× |
| C5 BadWiFi | melts (24 s) | quinn melts (45 s) | NO WINNER (both break >5% loss) |

### Metric B — object COMPLETION, single path (1.8 MB median)
| cell | rp-native | best baseline | verdict |
|------|-----------|---------------|---------|
| C1 DC | ~0.025 s | quinn 0.027 / BBR 0.028 | **PARITY** (rp ≈ or slightly ahead) |
| C2 WiFi | 0.83 s | quinn **0.20** / BBR 0.22 | rp LOSES ~4× |
| C3 LTE | ~7.3 s | quinn **0.90** / BBR 1.0 | rp LOSES ~8× |
| C4 Sat | ~56 s (tunnel) | quinn **1.09** / BBR 3.6 | rp LOSES badly |
| C5 BadWiFi | 17.4 s | quinn/BBR **0.55**; CUBIC **DNF** | rp LOSES to quinn/BBR; **BEATS CUBIC** |

### Metric C — MULTIPATH goodput, dual path (50 MB)
| cell | rp dual | best baseline | verdict |
|------|---------|---------------|---------|
| C7 WiFi+WiFi (sym) | **23.9 Mbit/s** | MPTCP 15.4 | **rp WINS 1.55×** |
| C8 WiFi+LTE (asym) | 12.6 | MPTCP 12.6 | **PARITY** |

### "raptorpath is the right transport when…"

**raptorpath is the right transport when your traffic is latency-sensitive
over lossy links, or when you can aggregate multiple lossy paths, or when
the alternative is loss-reactive TCP that collapses.** Concretely it WINS
in three regimes: (1) message-tail latency on lossy moderate-RTT single
links — its p99 is 44–226 ms at WiFi-class loss where QUIC spikes to 2.8 s
and kernel TCP to 13 s (12–60×), because FEC recovers loss in-band instead
of head-of-line-blocking an ordered stream; (2) multipath aggregation over
symmetric lossy paths — 1.55× kernel MPTCP, whose subflows collapse under
the loss that raptorpath's FEC absorbs; (3) any link lossy enough to break
loss-reactive CUBIC, which it outlasts (completes at C5 where CUBIC DNFs).
It reaches PARITY on clean/low-loss links and asymmetric multipath. It is
NOT (yet) the right choice for maximum single-path BULK THROUGHPUT/
completion against a tuned QUIC or BBR — it trails 4–8× on lossy single
paths (worse at satellite RTT), and BBR wins the low-rate tail at C3.
Crucially, that completion deficit is a userspace-tunnel PIPELINE cost
(block assembly, CC ramp, decode — confirmed by native + warm-flow
geometry), NOT the model: at L0 the principled controller beats both
TCP-class and QUIC-class adversaries; the L1 gap is engineering the data
plane up to the model, which the CC/pipeline work (P-CC, §12.6) is
attacking. Boundary rule of thumb: **choose raptorpath above ~1% loss when
tail latency or multipath matters; choose QUIC/BBR for single-path bulk on
a low-loss or very-high-RTT path.**


## Windowed-RLC-all-profiles experiment (branch `exp/windowed-rlc-all`, 2026-07-05)

**Question.** Only Realtime uses the sliding-window RLC pipeline in
production; Bulk/Auto use block mode (RaptorQ, 64 KB blocks, per-block
per-path affinity scheduler). Does unifying ALL hints on windowed RLC — the
code path the visualizer + paper model center on — help bulk single-path
and multipath?

**Change (reversible, opt-out kept).** `is_window_mode` → `backend.is_streaming()`
for every hint (was Realtime-only); Bulk/Auto auto-select `FecBackend::Rlc`
when no `--fec-backend` is given, so they take the window pipeline. Block
mode stays reachable for the A/B via `--fec-backend raptorq`. Added
`--fec-backend` to `raptorpath perf`; fixed the perf harness bulk chunk size
1400 → 1196 B so `frame_window_packet` (symbol_size 1200 → MTU 1196) does
not truncate chunks. `cargo test --lib` 235 green; gate_suite unaffected
(it is a pure in-process `run_fec` sim and never calls `net::run` /
`is_window_mode`).

### Phase 0 — reconnaissance (two blocking questions)

1. **Does the window sender spread symbols across paths?** No. Every source
   symbol goes to `select_source_path` = `Scheduler::best_source_path()`, the
   single lowest-cost path *that has cwnd capacity*; when that path saturates
   (`available()==0`) it spills to the next-best. It does **not** stripe via
   `scheduler.schedule()` the way block mode's affinity scheduler does. Repair
   symbols go to `best_repair_path()` (single highest-goodput path); Realtime
   additionally *duplicates* source onto a redundant path. So windowed RLC has
   **no proactive multipath aggregation for source data** — it is
   single-path-with-congestion-spillover. True windowed multipath would need
   the sender to stripe source symbols across paths (a large new effort).

2. **Symbol size / MTU.** The bulk `BlockProfile` already uses
   `symbol_size = 1200`, so window mode clamps the TUN MTU to 1196 — full-size
   packets are **not** fragmented. (The 512 B → 508 B → 3× fragmentation
   problem is Realtime-specific.) Symbol size was therefore *not* the obstacle
   for bulk; no profile change was needed.

### Phase 1 — windowed RLC for bulk, single path (rp-native perf, C2)

| arm | backend / mode | 1.8 MB × 10 | completion |
|-----|----------------|-------------|------------|
| windowed-RLC-bulk (`--protocol-hint bulk`, default) | Rlc / **window** | **0 / 10 (all DNF)** | only 64 B warmup completed; client hit the 600 s wall |
| block-mode-bulk (`--protocol-hint bulk --fec-backend raptorq`) | RaptorQ / block | 10 / 10 | mean **0.895 s** / median 0.93 / min 0.51 / max 1.16 (16.1 Mbit/s) |

Both arms same binary, C2 (100 Mbit, 10 ms RTT, GE 1.3 %/50 %), seed 42.
The block arm reproduces the existing baseline (0.83–0.88 s). Windowed bulk
**cannot complete a single 1.8 MB object**.

**Root cause (structural, not a tuning miss).** The window pipeline is
*loss-tolerant by design*, which is correct for Realtime (a dropped packet is
fine) and fatal for bulk (every byte must arrive):
- The sender never blocks on window fullness. When
  `encoder.window_size() > MAX_WINDOW_SIZE` (200) it **force-advances,
  evicting un-acked source symbols** (`net/mod.rs` ~2763). Evicted source
  symbols can no longer be regenerated or retransmitted.
- The receiver's reorder buffer **force-delivers past unrecoverable holes**
  on expiry (~20 ms), skipping the missing packet and advancing the cumulative
  point.
- At C2's 1.3 % GE loss a 1.8 MB object is ~1520 symbols → ~20 loss events.
  Any symbol lost and not NACK-repaired within ~200 symbols (the window drains
  in ≈19 ms, near one RTT) is evicted → the packet is permanently dropped →
  the perf object (which needs every chunk) never assembles → DNF.

C3 (20 Mbit, GE 2 %/40 %) was not separately run: it is strictly lossier, so
windowed bulk DNFs there a fortiori for the identical reason. The 50 msg/s
message-tail run and any C3 windowed sweep were scoped out for the same
foregone-conclusion reason — a hard single-path DNF is the ceiling.

### Phase 2 — windowed RLC multipath: not feasible in scope

Phase 0 shows the window sender does not stripe source symbols across paths,
and Phase 1 shows single-path windowed bulk already DNFs. Dual-path would DNF
*harder*: cross-path reordering widens the holes the reorder buffer must
cover, and the eviction/force-deliver loss remains. Making the sender stripe
(the prerequisite) is a large effort and pointless until windowed RLC delivers
reliably at all. This also matches §16 (Fountain Multipath Aggregation): the
in-order delivery contract the window receiver enforces caps aggregation at
fast-path-alone (C8 14.0) regardless — so even a striping windowed sender
could not beat the block-affinity 12.6 / 23.9 numbers without *out-of-order*
delivery, which the window receiver does not do.

### Verdict — windowed RLC does NOT help bulk (it breaks it)

Unifying on windowed RLC regresses bulk from **10/10 @ 0.90 s to 0/10 (total
DNF)** single-path at C2, and offers nothing for multipath (the sender does
not aggregate). The window pipeline's loss-tolerance (source-symbol eviction +
force-deliver-past-holes) is fundamentally incompatible with bulk's
every-byte-required contract. Block mode (RaptorQ + P8 block-ARQ) stays the
correct choice for Bulk/Auto. Do **not** ship windowed-all as a default; a
prerequisite for any future windowed-bulk is a reliable-delivery mode that
never evicts an un-acked source symbol before it is delivered/acked (bulk ARQ
inside the window), plus a striping sender before multipath is even meaningful.
The experiment stays on this branch, unmerged, as the record.

**Paper follow-up (2026-07-06, branch paper/rwm-rewrite).** Paper §16 was
rewritten to the converged Reliable Windowed Multipath (RWM) formulation and
§15 gained the reliability-policy axis (§15.7), superseding the earlier
"out-of-order is the unlock" framing — motivated by this negative result plus
the user corrections: reliability is pipeline POLICY (evict vs
retain-until-acked), not codec; and per-path-affine ATOMIC UNITS, not
in-order delivery, are what cap multipath aggregation.

## RWM Phase A — reliable window pipeline (branch `feat/rwm-phase-a`, 2026-07-06)

**Goal.** Make the window pipeline RELIABLE as a per-stream POLICY (paper
§15.7/§16.3 RETAIN-UNTIL-ACKED) and verify at L1 that bulk on the reliable
window reaches completion parity with the block baseline — the prerequisite
the windowed-RLC-all experiment above identified before any Phase B
(striping/multipath) work is meaningful.

**Design as shipped (per the §16.3 user correction: retention lives at the
ARQ layer, not in the coding window).**
- **Sent-data store (sender).** Every sent source symbol's bytes are
  retained in a store until the peer's cumulative WindowAck passes them —
  removal by ack ONLY, never timeout, never pressure. The coding window
  keeps sliding freely (cap eviction stays): it is only the FEC horizon.
  An aged SACK-confirmed hole that slid out of the window is recovered by
  a TARGETED retransmit of exactly that symbol from the store (NACK gaps
  are no longer clamped to the window span in reliable mode).
- **Backpressure, not loss.** Store full (RELIABLE_STORE_MAX = 1024
  symbols ≈ 1.2 MB ≈ 10× the C2 BDP) ⇒ the sender stops reading the TUN
  (same contract as the block path's cwnd gate) while still servicing
  acks/NACKs/tail sweeps; a 1 ms poll arm re-checks store drain.
- **Receiver holds at holes.** `ReorderBuffer::new_reliable()`: no expiry
  force-delivery, no capacity force-drain — delivery advances only through
  the recovered in-order prefix. While stalled, the receiver re-advertises
  the gap (SACK-bearing WindowAck) every 2×SRTT (25–100 ms clamp); the
  sender's tail sweep is the backstop. Realtime's EVICT policy is
  untouched (policy is per-config, not global).
- **WindowAck seq-space fix (both policies).** The sender's shared
  `window_ack_seq` atomic was only ever written with the LOCAL receiver's
  inbound delivery counter — a different seq space — so ack-driven advance
  / retransmit pruning ran on garbage. The peer's `received_up_to` is now
  published from `handle_control_message` (fetch_max across paths); the
  receiver keeps a local dedupe counter instead.
- **Opt-in.** `--window-reliable` flag / `window_reliable` config field;
  Bulk/Auto then auto-select windowed RLC (codec-agnostic policy — RLC is
  just the natural sliding-window codec). Default remains block mode.
  perf chunk geometry unified at 1196 B for both A/B arms.

**Mid-stream backend auto-switching REMOVED (paper §16.4).** The runtime
FEC backend switch (hard ε̂ thresholds 0.01/0.12 + debounce; `fec_auto_switch`)
violated the no-hard-cutoffs convention and its switch cost is structural
(no cross-code algebra ⇒ in-flight data stranded; P9a measured the window
variant blinding the ACK/NACK machinery for ~a window of traffic). The
block-mode evaluate-per-block call and the P9a-pinned window switch block
are deleted; the codec is pinned at startup. Config fields still parse
(deprecated, warn-if-set) so existing configs keep loading; the receiver
ignores WindowSwitch with a warning; `BackendSelector` survives only as an
unwired reference implementation with its tests.

**L1 A/B (rp-native `perf`, 1.8 MB × 10, seed 42, same binary, flag-only
difference; per-measurement hard timeouts).**

C2 (100 Mbit, 10 ms RTT, GE 1.3 %/50 %):
| arm | completion | median | mean | min | max | stdev |
|-----|-----------|--------|------|-----|-----|-------|
| block (baseline) | 10/10 | 0.884 s | 0.890 s | 0.437 | 1.357 | 0.263 |
| reliable window | **10/10** | **1.092 s** | 1.157 s | 0.397 | 2.382 | 0.637 |

Median ratio **1.23×** — within the ~1.3× parity gate. **GATE PASSED**:
the same pipeline that DNF'd 0/10 under EVICT completes 10/10 under
retention, at C2 the policy axis alone flips the result (the §15.7 claim,
now measured in both directions). The reliable arm carries more variance
(stdev 0.64 vs 0.26; worst run 2.38 s) — recovery of aged holes costs a
tail-sweep/NACK round trip where block-ARQ's ledger repairs ride batch
acks.

C3 (20 Mbit, 40 ms RTT, GE 2 %/40 %):
| arm | completion | median | mean | min | max |
|-----|-----------|--------|------|-----|-----|
| block (baseline) | 10/10 | 9.964 s | 9.198 s | 5.328 | 11.594 |
| reliable window | **10/10** | **5.113 s** | 5.347 s | 3.868 | 7.868 |

Reliable window is ~1.9× FASTER at C3 (5.1 vs 10.0 s median): the known
block-mode C3 structural bottleneck (64 KB block serialization at 20 Mbit,
see the P-CC row above — 10.4 s median, flat across CC work) does not
apply to the streaming window, whose per-symbol pipeline keeps the link
filled. First measured instance of the window pipeline beating block mode
on a bulk contract.

**Realtime regression check** (stream_bench c2 rp-realtime, 50 msg/s ×
30 s, seed 42): 1500/1500 delivered, p50 8.4 ms / p90 12.9 / p99 54.9 /
p999 95.4 / max 111.9; zero path deaths. Baseline (P10b record): p50 8.6 /
p99 513 / p999 727. No regression — the tail is ~10× BETTER; plausibly
the WindowAck seq-space fix (reactive repair now keyed to real peer acks),
though single-run variance means the improvement is indicative, not a
measured claim.

**Verification.** `cargo test --lib` 243 green (new: store retention
survives window eviction + removal-by-ack-only, store backpressure
engagement, reliable reorder holds past holes / never force-drains, SACK
range helper round-trip); gate_suite --release 15/15; new in-process
`perf_loopback_reliable_window` exercises the full reliable pipeline over
real QUIC loopback.

**Scope.** Phase A only: no striping (the window sender remains
single-path-with-spillover; Phase B implements the §16.3 marginal-cost
placement law), no frontier-decode changes beyond what reliability
requires, block mode untouched as the default.

## Honest scope

### P10b — realtime (window-mode) reactive repair (2026-07-04)

Target: cut rp-realtime's ~430 inner retransmits / 5×1.8MB and its
completion time at C2 (goal ≤1.2 s median, <100 retransmits).

Root cause (code reading first, then confirmed on the wire): window
mode had NO functioning reactive repair path — three independent
breaks stacked so that only proactive FEC ever repaired a loss, and
anything FEC missed waited out the 4×SRTT reorder hold and was
force-delivered as a HOLE to the inner TCP:

1. WindowNack is deprecated and never sent; the SACK-extended
   WindowAck that replaced it carried the gap info, but the sender's
   WindowAck handler explicitly ignored sack_ranges — and nack_tx (the
   channel driving the entire NACK retransmission machinery) had no
   producer at all. Dead code since the SACK migration.
2. The receiver only sent WindowAck when the cumulative delivery point
   advanced — a hole silences ALL acks exactly while the gap signal is
   needed (no dupack analog).
3. The sender drained the NACK channel only after a TUN read — the
   inner TCP stalls on the hole → no TUN packets → no repair
   processing, precisely when repair is the only way to unstall.

Fixes (raptorpath/src/net/mod.rs), each verified by RUST_LOG=debug
event counts on an isolated build:
- Dupack-style gap acks: WindowAck also sent while the cumulative
  point is stalled but higher seqs keep arriving (rate-limited 2 ms).
- sack_to_gaps(): the WindowAck handler inverts SACK ranges into
  missing-seq gaps and feeds nack_tx (age gate SRTT/2 against
  cross-path skew; per-seq cooldown 1×SRTT so repeated gap acks
  cannot flood, but a lost repair is re-sent one SRTT later).
- Sender select! now wakes on nack_rx AND a tail ARQ sweep timer
  (2×SRTT clamp [25,100] ms, block-mode P8 sweeper analog): the LAST
  symbols of a burst have no successors, so the receiver can never
  SACK a gap behind them; the sweep synthesizes a gap report for the
  oldest un-ACKed seq. The sweep MUST rearm on every fire even when
  the retransmit is skipped — a past deadline left the timer arm
  permanently ready and would spin the select loop.
- ADR-0050 NACK budget floored at one repair burst per 5 ms refresh:
  the raw cap (≈ loss/2 × sources-this-period) truncated to 0 almost
  always because the period resets every 10 acked seqs — silently
  suppressing the whole reactive path. Congestion safety stays with
  the ADR-0046 multiplier (which can still zero repairs).

Measured at L1 C2 (1.8 MB × 5, seed 42, isolated build dir — the
shared ~/raptorpath tree was being concurrently modified by another
session, which invalidated two intermediate runs; all numbers below
are from ~/rp-p10b builds verified by debug event counts):

  baseline (a7c20d7):        median 3.49 s / mean 3.35, 496 inner
                             retransmits, 52 SACK recoveries
  + SACK reactive repair:    median 2.34 s / mean 2.27, 287 retrans
    (gap acks + gap wiring;  (2nd sample: median 1.74 / mean 1.80,
    budget still suppressed)  171 retrans — variance is real)
  + tail sweep + budget      median 1.57 s / mean 1.63, 38 inner
    floor + sweep rearm      retransmits, 13 SACK recoveries, 0 RTOs
    (full P10b):

Debug-instrumented run (same code, RUST_LOG=debug): 637 gap reports →
526 targeted retransmits + 159 tail sweeps per 5 transfers on the
data sender; 0 "TUN inject channel full" drops. Retransmit goal met
5× over (38 « 100); the ≤1.2 s median is not reached (1.57) — the
remaining gap is no longer inner-TCP recovery (only ~13 halvings per
5 runs) but window-mode goodput + inner slow-start, i.e. the same
(a)/(b)/(c) list as bulk above.

Not separately ablated (VM occupied by a concurrent session for the
remainder of the window): tail sweep vs budget floor within the last
step — they ship together; the sweep depends on the floor (a
budget=0 drop must not stall the sweep's rearm, see the
last_tail_sweep_us comment in net/mod.rs).

- L0's baseline is our own simulation model. It now includes slow-start,
  AIMD, and in-order semantics, but it is not CUBIC/BBR. **The claim this
  gate supports is "surpasses the SimRetx model under ADR-0051 conditions",
  not "surpasses real TCP".**
- The loss feedback timing is per-RTT-batch with sender-side knowledge of
  wire outcomes (oracle timing, same convention as bench_suite).
- Next fidelity level (L1, ADR-0051): real CUBIC/BBR/quinn/MPTCP stacks
  over netns + netem — requires Linux/WSL2; the win conditions transfer
  unchanged.

## RWM Phase B — striping window sender (per-symbol placement law, paper §16.3)

Phase A gave the reliable window pipeline retention (RETAIN-UNTIL-ACKED,
ρ = 1) but left the sender SINGLE-PATH. Phase B adds the striping sender:
every source AND repair symbol is placed across paths by ONE continuous
marginal-cost rule (`Scheduler::place_symbol`, no load regimes, no case
splits), wiring it into `run_window_sender` for the reliable window only
(single-path is byte-identical — the law with N=1 is that path always).

### The placement law as implemented

For each active path i, softmax over a marginal cost:

```
  cost_i = Ê_i(load)/ref_srtt  +  w_bw·r_i  +  w_div·ρ_fate(s,i)
  P(i) ∝ exp(−cost_i / T)          T = PLACE_TEMPERATURE = 0.15 (RWM_PLACE_T override)
```

- **Ê_i(load)** = `in_flight_i/(cwnd_i/SRTT_i) + SRTT_i/2 + eps_i·RTT_i` — the
  expected frontier-completion-TIME. The queue term drains at the path's live
  PACING RATE, so a backlog on a slow/low-capacity path costs proportionally
  more real time. This is what water-fills by CAPACITY. Unit-weighted (not
  `w_lat`): on a reliable IN-ORDER stream, latency-to-frontier is the
  completion cost itself, not a per-hint preference. Normalised by the fastest
  SRTT so it is O(1). (A dimensionless `in_flight/cwnd` fill — equal-fraction,
  capacity-BLIND — was tried first and MEASURED catastrophic at C8: 3.4 Mbit/s,
  §below.)
- **r_i** — correction/loss burden, the hint's `w_bw` dial (Bulk = 1).
- **ρ_fate(s,i)** — repairs only: the fraction of the window symbols this repair
  covers that path i already carried (continuous form of the old hard
  `best_repair_path_avoiding`).
- **T** — the one dial §16.3 names; T → 0 = strict best-path (unit-tested argmin).

`place_symbol` unit-tested (6 tests, all green): (a) idle → concentrates on
cheapest; (b) monotonic continuous spillover as in_flight rises (no threshold
jump — the congestion term is the continuous form of "skip a full path");
(c) water-filling equilibrium (equal fill fraction ⇒ balanced placement ⇒
throughput ∝ capacity); (d) repair fate steers off the covered path; (e) T → 0
= argmin; plus single-path = identity. Receiver needed NO change: it already
decodes every path into one window decoder + one seq-keyed reorder buffer, so
in-order delivery is path-agnostic (the aggregation is structural).

### GATE numbers (rp-native `perf`, --window-reliable bulk, seed 42, this binary)

| arm | workload | goodput | vs fast-path-alone |
|-----|----------|---------|--------------------|
| Fast-path-alone (single c2) | 50 MB ×3 | **15.42 Mbit/s** | 1.00× (the §16.2 ceiling ref) |
| Single-path (c2, no regression) | 1.8 MB ×10 | 14.57, **10/10** | Phase-A parity |
| **GATE 1** C7 sym (c2+c2) dual | 50 MB ×3 | **21.73 Mbit/s** | **1.41×** |
| **GATE 2** C8 het (c2+c3) dual | 50 MB ×3 | **12.55 Mbit/s** (T=0.15 best) | **0.81× — FAILS** |

C8 T-sweep (50 MB ×2): T=0.05 → 10.6, 0.15 → 12.5, 0.30 → 11.3, 0.60 → 7.6
Mbit/s. No temperature beats fast-path-alone.

**GATE 1 (no-regression): PARTIAL PASS.** RWM aggregates 1.41× on symmetric
paths (21.73 vs 15.42 single) — the striping MECHANISM is sound. It sits ~9%
under block-affinity's 23.9 (a different pipeline: block-ARQ carries no
fountain overhead φ; the window/RLC pipeline's single-path is itself 15.4, so
this is parity-minus-φ, not a striping regression). Below the strict 2×/23.9
bar but no catastrophe.

**GATE 2 (the point): FAIL, mechanism identified.** RWM at C8 = 12.5 Mbit/s,
BELOW the 15.42 fast-path-alone ceiling, ≈ the block-affinity/kernel-MPTCP
in-order parity (12.6). Aggregation factor 0.81× vs L0's predicted ×1.18.

### Mechanism: why C8 fails (the assumption that broke)

The order-statistic aggregation of §16.3 predicate (3) requires the coding
window to be RATELESS across paths — the frontier advances on ANY sufficient
K_h(1+φ) symbols regardless of which path carried which. In this
implementation source symbols are striped in SEQUENCE order and the bulk
repair rate is loss-driven (~2%), far too low to make the window fungible. So
a source symbol placed on the slow path is a specific in-order position the
frontier must WAIT for — the fast path cannot decode around it (not enough
coded degrees of freedom). Striping thus recreates the exact per-path-affine
head-of-line penalty §16.2 bounds at ≤ Σ_{E={fast}} g = 14.0, which is why the
number lands on the in-order parity, not above it.

Two measured signatures confirm this is HOL, not a scheduling bug:
1. **Symmetric works (GATE 1: 1.41×).** With equal paths there is no
   fork-join asymmetry, so striping aggregates — the law is correct.
2. **Capacity-blindness is catastrophic, capacity-awareness merely parity.**
   The first cut used a dimensionless `in_flight/cwnd` load term (equal
   window-fraction): it over-loaded the 5×-smaller slow path and collapsed the
   frontier to **3.4 Mbit/s** (116 s/50 MB). Switching Ê_i(load) to
   frontier-completion-TIME (pacing-rate-aware) recovered it to 12.5 — i.e. the
   best the placement law can do is stop the slow path from HURTING; it cannot
   make the slow path HELP an in-order frontier without fungible coding.

**The next lever (not placement).** Beating 14.0 at C8 needs the rateless
frontier itself: multipath repair provisioning raised so the fast path carries
enough coded symbols to decode each window independently (paper §16.5 W-sizing
/ the K_h(1+φ) overhead), with the slow path contributing fungible degrees of
freedom rather than in-order source positions. That is a codec/rate-control
change beyond Phase B's placement-law scope. Phase B's honest result: the
striping law is implemented, unit-tested, aggregates on symmetric paths, and
is safe on heterogeneous paths (no collapse) — but the decisive C8 aggregation
the paper predicts is gated on rateless-window provisioning, which Phase B does
not add.

Reproduce: `tools/l1/perf_rwm.sh <scenA> <scenB> <hint> <bytes> <runs>
<dual|single> [T]` (topo_dual up/down per arm, one measurement at a time).

## RWM Phase C — out-of-order object delivery (branch `feat/rwm-phase-c`, 2026-07-06)

Phase B proved the striping law aggregates symmetric paths (C7 1.41×) but at
C8 the reliable in-order frontier hit MPTCP parity — for a systematic (low-r)
window the slow path's SOURCE symbols are fixed positions the fast path
cannot decode around. Phase C implements the FREE unlock for the OBJECT case:
**out-of-order delivery** — decode each source symbol and hand it to the
consumer the instant it decodes, reassemble by offset, complete on
total-decoded — the H → ∞ corner of paper §16.7 (H = the reorder/latency
horizon; out-of-order = H → ∞; ordering is a per-stream POLICY, off = today's
in-order). This is the correct metric for a file (nothing reads offset k
before the file is whole) and never for a live in-order stream, so the flag
is OFF by default and set only by the native object path.

### What shipped
- **General unordered-delivery policy** (`window_out_of_order`, requires
  `window_reliable`; `perf --window-out-of-order`). The reliable receiver
  BYPASSES the reorder buffer entirely (`reorder_buf = None`) and delivers
  each decoded symbol immediately; a lightweight frontier over `received_seqs`
  drives the cumulative WindowAck so retention/retransmit stay correct (holes
  are still retransmitted until acked — reliability is unchanged, only the
  DELIVERY wait is removed). The object/perf path is one consumer; datagram /
  RPC / telemetry are equally served. Sender path untouched (byte-identical).
- **`deliver_packet` policy helper + tests** (the user's requirement:
  "if loss is allowed it doesn't actually block"): reliable delivery
  backpressures on a full channel (never drop → no phantom hole); lossy
  delivery drops (never block → a slow consumer can't stall the stream).
  Unit-tested both directions + closed-channel. (The ooo object path uses the
  lossy policy: the bounded 8192 inject channel only fills under a
  pathological burst, and blocking there deadlocks the loopback's
  feeds-and-drains feedback loop — MEASURED; a rare drop is recovered by
  retransmit.)
- **`RWM_MIN_R` env repair-rate floor** — a TEST-ONLY instrument (default 0,
  never a production control law; per the standing "reactive repair floors
  are bad in production" finding) to drive the paper's raise-r arm.

### GATE numbers (rp-native `perf`, seed 42, this binary; C8 = c2+c3, C7 = c2+c2)

| arm | workload | goodput | notes |
|-----|----------|--------:|-------|
| Fast-path-alone (single c2) | 50 MB ×3 | **15.68 Mbit/s** | the §16.2 ceiling ref |
| **C8 in-order** (H bounded) | 50 MB ×8 | 8.39 mean / 8.1 med | stdev 6.9 (variable) |
| **C8 out-of-order** (H→∞) | 50 MB ×8 | **11.87 mean / 12.0 med** | **stdev 3.2** (stable) |
| C8 in-order **raise-r=0.18** | 50 MB ×5 | 7.87 | no unlock (≈ r≈0) |
| **C7 out-of-order** (regression) | 50 MB ×3 | **21.61** | ≈ Phase B 21.73 ✓ stdev 0.5 |

### DECISIVE VERDICT: FAIL the strict bar, with mechanism

- **Does C8 out-of-order beat 15.42 (fast-alone)? NO.** 11.87 Mbit/s = 0.76×
  fast-alone. It does not beat 15.42, and its median 12.0 does not beat the
  14.0 goal-gate ceiling either. **Aggregation factor vs fast-alone: 0.76×**
  (vs L0's predicted ×1.18 ≈ 18.5 Mbit/s — **not met at L1**). It lands at
  kernel-MPTCP / whole-block-affinity parity (12.6).
- **Does out-of-order help vs in-order? YES, modestly and STABLY.** 11.87 vs
  8.39 = **1.42× the in-order mean, ~2× lower variance** (stdev 3.2 vs 6.9).
  The gain is implementation overhead removed, not new aggregation: the
  in-order reorder buffer accumulates the whole out-of-order suffix behind
  each hole and drains it in erratic bursts; deliver-on-decode removes that
  buffer and its tail. So out-of-order is the **more robust** realization of
  decode-on-total — it buys stability, not aggregation above the fast path.
  This REFINES the §16.2 equivalence (ooo ≡ deep-buffer in-order): identical
  in theory, but ooo is measurably steadier in practice.
- **Does the r-knob unlock it? NO (at r=0.18).** In-order + forced r≈0.18
  measured 7.87 — no better than r≈0. Forcing 18% repair adds straggler load
  without making the window fungible enough to reconstruct the slow-path
  source positions from fast-path repairs. (A blanket reactive-repair floor
  was separately measured to REGRESS C8 14→9 — congestion safety must win;
  the floor was removed.) Whether a much larger r with repairs pinned to the
  slow path crosses fast-alone is OPEN — neither knob crossed it here.
- **C7 symmetric regression: PASS.** 21.61 ≈ Phase B 21.73, stdev 0.5 — where
  paths match there is no straggler and out-of-order is neutral (mechanism
  sound; C8 heterogeneity is where both knobs fall short).

**Regime-map note (for the merger).** The multipath row is NOT the clean
"out-of-order → aggregation" headline the plan hoped for. The measured
position is: out-of-order is the correct, simpler, lower-variance delivery
policy for bulk objects, but at C8 it reaches MPTCP parity, not above the
fast path — heterogeneous aggregation-above-fast-path is unproven on this
stack by either the H knob (out-of-order) or a modest r, and is a measured
OPEN problem, not a demonstrated win (paper §16.7).

### Honest caveats
- **C8 is high-variance** (individual 50 MB runs 28–95 s across the session).
  The earlier one-off "14.0" out-of-order sample (3 runs) did not reproduce;
  the x8 numbers above are the robust estimate. Phase B's in-order 12.5 (3
  runs) was likewise an optimistic sample vs the x8 in-order 8.4 here — the
  in-order path is byte-identical to Phase B, so the delta is run/session
  variance, not a regression.
- **Pre-existing loss-recovery fragility** (NOT introduced by Phase C):
  reliable-window transfers occasionally stall on the near-zero-RTT loopback
  when a datagram-loss burst collapses the ADR-0046 congestion multiplier to
  0, fully suppressing recovery until the QUIC idle timeout. MEASURED ~1/6 on
  the WINDOWS dev loopback for BOTH the in-order (`perf_loopback_reliable_
  window`) and out-of-order tests; on LINUX (VM) `perf_loopback` passes 6/6
  and the netem C8 transfers never DNF. It is a Windows-loopback timing
  artifact of a real recovery gap; the proper fix (a cheap idle-triggered
  sweep, not a per-round floor) is deferred RWM hardening.
  **UPDATE (verification-oracle, Phase 4): the idle-triggered fix is now
  LANDED** — `NackCongestionState::effective_multiplier(idle)` floors the
  multiplier only when no new source has been sent for > 2×SRTT (idle ⇒ no
  straggler load to protect); active-transfer behavior is bit-for-bit
  unchanged. Unit-tested (`test_idle_triggered_recovery_floor`).

### ORACLE VERDICT (formula- AND wasm-sim-independent MC; verification-oracle)

An independent Monte-Carlo oracle (`raptorpath-math/tests/multipath_oracle.rs`
— does NOT call `compute_r_star`/`p_fec`/`controller_rate` or the wasm sim;
models per-path capacity + one-way delay + GE loss, striped placement,
fungible repairs over a sliding horizon with eviction, cross-path ARQ, and
in-order frontier decode) was run at the EXACT C8 netem params (c2+c3). It
RECONCILES the L0/L1 contradiction:

| oracle config (C8, K≈20k) | factor | reads as |
|---|---:|---|
| goodput ceiling Σg_i/g_fast | **×1.195** | physical max |
| FUNGIBLE cross-path RWM, whole-object horizon | **×1.19** | == L0 wasm (×1.18) |
| ATOMIC path-affine (regime 2) + cross-path ARQ | ×0.92–0.94 | sub-unity |
| ATOMIC + SAME-path recovery | ×0.48–0.57 | broken recovery |

**The true out-of-order object case AGGREGATES to the goodput ceiling
(×1.19), matching L0 — so heterogeneous aggregation-above-fast-path is NOT
fundamentally fork-join-bounded.** L1's ×0.76 sits INSIDE the broken-transport
band (between atomic-clean ×0.92 and atomic+same-path ×0.48–0.57), reproducible
in the oracle ONLY by breaking fungibility AND cross-path recovery. VERDICT:
**×0.76 is a PRODUCTION limitation, not a fundamental bound** — block/path-affine
atomicity + same-path/suppressed recovery + eviction, exactly the §16.2(i)/(ii)
caps.

**Lever decomposition (which fix buys how much aggregation), independent-GE
oracle, best→worst:**
- **FUNGIBILITY = cross-path frontier decode (RWM, §16.2)** is the DOMINANT
  lever: ATOMIC ×0.92 → FUNGIBLE ×1.19. Without it, even perfect pull +
  cross-path recovery caps at ×0.92 (sub-unity).
- **CROSS-PATH recovery** is next: same-path ×0.48 → cross-path ×0.92.
- **PLACEMENT (pull vs push) is NEGLIGIBLE** here: ×1.190 (pull) vs ×1.190
  (static push) in the fungible case — fungible frontier fill MASKS the
  slow-path long pole. (This CORRECTS the intuition that pull placement is
  the big lever; in the independent-GE oracle it is ~0. Placement/push may
  still matter under shared-bottleneck path CORRELATION, which this oracle's
  independent GE chains do not model — flagged for real-trace validation.)

**r-sweep (oracle):** at the whole-object horizon the dual beats fast-alone at
r=0 ALREADY (×1.19); raising r only matters when the coding window is too
small (H=256 crosses fast-alone at r≈0.18; H≥1024 aggregates at any r). This
EXPLAINS Phase C's raise-r=0.18 "no unlock": the C8 bottleneck is
fungibility + cross-path recovery, not repair volume — raising r cannot make a
path-affine atomic unit fungible.

**Grounded position for the regime map:** symmetric multipath aggregates
(C7 ×1.71 measured; oracle symmetric ×1.99). Heterogeneous OBJECT completion
aggregates to the goodput ceiling (~×1.19 at C8) **iff** the transport realizes
windowed FUNGIBLE cross-path frontier decode (RWM = the §16.3 EMPTY quadrant).
Production BULK today is RaptorQ 64 KB atomic blocks (path-affine) → oracle-
capped at ~×0.92 even with perfect pull + cross-path recovery; the measured
×0.76 is that ceiling dragged down by same-path/suppressed recovery + eviction.
The path to ×1.19 is the RWM subsystem (fungible sliding-window frontier decode
+ never-suppressed cross-path repair supply — the idle-recovery fix is one
prerequisite of the latter), NOT a placement tweak or a modest r. Aggregation-
above-fast-path at C8 remains OPEN **in production**, but is now proven
ACHIEVABLE in principle (oracle ×1.19) with a named, scoped mechanism.

### Verification
`cargo test --lib` 252 green (new: 3 `deliver_packet` policy tests); the ooo
loopback object test `perf_loopback_out_of_order_object` passes on Linux;
gate_suite 15/15 release.

Reproduce: `tools/l1/perf_rwm_c.sh <scenA> <scenB> <hint> <bytes> <runs>
<dual|single>` with `RWM_OOO=1` (out-of-order) and/or `RWM_MIN_R=<r>`
(raise-r), one measurement at a time.

## Fungible Frontier — coded-object mode (§16.3 empty quadrant, branch `feat/fungible-frontier`, 2026-07-07)

The culmination of the RWM arc: build the FUNGIBLE cross-path frontier the
verification oracle proved is required for heterogeneous aggregation (~×1.19),
and measure it at L1. RWM Phase B/C established that a SYSTEMATIC striped
window caps at fork-join parity (a slow-path source symbol is a fixed in-order
position — §16.7). The fix (§16.3): emit CODED-ONLY symbols (random linear
combinations over the window); any K independent coded symbols from ANY path
reconstruct the K sources, so no symbol is a long-pole.

### PART 2 — ORACLE-CONFIRMED reachable (before building)
`multipath_oracle.rs::oracle_c8_fungible_wmp_window` (new), exact C8 params:
a coded fungible window at the §16.5 W_mp bound reaches the aggregation
ceiling. **W≥384 → ×1.186–1.190 at r≥0.05** (ceiling ×1.195); W=600–1024 →
×1.15–1.18 even at r=0. The earlier ×0.99 at H=256 was simply W < W_mp. So
×1.19 is reachable by a FINITE-window coded design, not only H→∞. (Full
oracle reconciliation unchanged: fungibility is the dominant lever.)

### PART 3 — implementation (built, correct)
`window_coded_only` flag (config + PeerConfig + `--window-coded-only`; requires
`window_reliable`, implies out-of-order). In `run_window_sender`, coded-only
sends `encoder.generate_repair()` (a fresh RLC combination over the current
window) on the wire IN PLACE of the raw systematic source; the source bytes
still populate the encoder window + retention store for the targeted-ARQ
backstop. Window widened to W_mp (default 640, `RWM_WINDOW`); backpressure
store sized to the window (`RWM_STORE`). Receiver reuses the Phase-C
out-of-order decode-and-deliver path (the decoder emits each seq as GE
recovers it). Loopback test `perf_loopback_coded_object` passes: 1 MB across
many windows, ZERO systematic passthrough, all bytes, decode-on-K.

### PART 4 — DECISIVE L1 MEASUREMENT (rp-native `perf`, seed 42, this binary)

| arm | workload | goodput | vs fast-alone |
|-----|----------|--------:|--------------------|
| systematic fast-path-alone (single c2) | 50 MB ×3 | **15.24 Mbit/s** | 1.00× (the bar) |
| **coded-only C8 het (c2+c3) dual** | 50 MB ×6 | **3.93 mean / ~4.5 med** (stdev_s 58) | **0.26× — FAILS** |
| coded-only C7 sym (c2+c2) dual | 50 MB ×3 | 5.46 | 0.36× (symmetric collapse) |
| coded-only SINGLE c2 | 50 MB ×2 | 12.88 | 0.85× (codec cost only) |
| systematic C7 ooo dual (regression) | 50 MB ×2 | 21.74 | ≈ Phase B/C 21.6 ✓ no regression |
| systematic C8 ooo dual (regression) | 50 MB ×2 | 10.34 | ≈ Phase C 11.9 within C8 variance |

W/r/store sweep at C8 (all ≪ bar): W=200 r=0 → **2.0** (ARQ-starved, §16.5
predicted); W=640 r=0 → ~2; W=640 r=0.10 → **4.5**; r=0.20/0.30 → no better,
extremes DNF; store lifted (no backpressure) → **DNF**; W=2048 → **2.4 + DNF**
(decode O(W) cost). CPU during a run: ~1 core, ~80 % idle — **stalling, not
compute-bound**.

### DECISIVE VERDICT: FAIL the strict bar (>15.68), with a sharp mechanism

- **C8 coded-only = 3.93 Mbit/s = 0.26× fast-alone.** Does NOT beat 15.68/15.24;
  does not reach the oracle's ×1.19 (~18.5). **FAIL.**
- **The failure is cross-path coding itself, NOT heterogeneity.** Coded-only
  *single-path* = 12.9 (works; the ~18 % gap under systematic is the
  O(W)-per-symbol codec cost of 100 %-coded vs ~2 %). But DUAL is WORSE than
  single on BOTH symmetric (5.5) and heterogeneous (3.9) paths — adding any
  second path *drags*, the opposite of the oracle's monotone aggregation. The
  independent-GE oracle's ×1.19 is **not realized** on the real
  sliding-window + per-path-timing + CC + ARQ stack.
- **Mechanism (DERIVED).** A coded symbol combines over the sender's window
  *at send time*; a symbol striped to a path lands one path-delay later, by
  which point the window/frontier has advanced (on the fast path by
  ~Σg·RTT ≫ W_mp), so a second path's symbols cover already-decoded /
  misaligned windows — little useful pooled rank, plus cross-path reordering
  and congestion-throttled ARQ on every transient undecoded seq. §16.5's W_mp
  sizing is NECESSARY (lifted 2 → 4.5) but not sufficient.
- **No regression.** `window_coded_only` is default-off; the win_cap/store
  changes only apply when it is set. Systematic C7 21.74 (= Phase B/C 21.6),
  systematic C8 10.34 (≈ Phase C within variance), fast-alone 15.24.

### Standing position for the regime map (for the merger)
The §16.3 empty quadrant now has a *correct implementation* and an
*independent-GE proof of achievability* (oracle ×1.19), but the L1 transport
does not realize it — coded-only over the current per-path-timed sliding
window aggregates *negatively*. Heterogeneous aggregation-above-fast-path
stays **OPEN and unrealized in production**. What oracle-×1.19 vs L1-×0.26
together isolate: the missing piece is not fungibility-in-the-abstract (built,
proven) but **cross-path window ALIGNMENT** — coding horizons whose per-path
arrivals pool over the *same* live window, which send-time-windowed RLC does
not provide. That is the named next mechanism; the multipath regime-map row
is NOT rewritten to a win.

**UPDATE (Corrected Oracle / Final Aggregation Verdict, 2026-07-07).** The
"named next mechanism" above is now MADE PRECISE and oracle-tested. The
corrected temporal oracle (`temporal_oracle.rs`) reproduces the L1 refutation
faithfully (×0.259 het / ×0.362 sym, dual < single on both) and shows the
alignment fix is **generation-based coding with a STABLE per-generation anchor**
(oracle ×1.19 at C8, no drag; stable anchor is the dominant lever). Regime-map
multipath row, honest final position:
- **C7 symmetric: aggregates** (L1 ×1.71; oracle ×1.96) — unchanged.
- **C8 heterogeneous: aggregation-above-fast-path is ACHIEVABLE (oracle-proven
  ×1.19 under stable-anchor generation coding), OPEN/unbuilt in production.**
  The shipped moving-window coded-only design is REFUTED (×0.26) and the
  barrier is identified (moving anchor + per-seq throttled recovery), not
  fungible coding as such. The row is a *scoped build recommendation*, not a
  demonstrated production win and not an unbounded open problem.

### Verification
`cargo test --lib` 253 green + `perf_loopback_coded_object` (Linux/loopback);
`cargo test -p raptorpath-math` green incl. new `oracle_c8_fungible_wmp_window`;
gate_suite 15/15 release. Reproduce: `RWM_WINDOW=640 RWM_MIN_R=0.10
RWM_EXTRA="--window-coded-only" tools/l1/perf_rwm_c.sh c2 c3 bulk 50000000 6
dual` (one measurement at a time).

## Corrected Oracle / Final Aggregation Verdict (branch `feat/oracle-temporal`, 2026-07-07)

The Fungible-Frontier oracle above (`multipath_oracle.rs::oracle_c8_fungible_
wmp_window`) predicted a coded fungible window reaches ×1.19 at C8; L1 REFUTED
it (coded-only C8 = 3.93 Mbit/s = ×0.26, and the decisive signature: DUAL is
worse than SINGLE on both symmetric AND heterogeneous paths). Per the /goal's
own rule — "no model term is trusted until the oracle confirms fidelity" — the
oracle FAILED FIDELITY and was corrected before it may render any verdict. The
correction adds the temporal dynamics the old oracle abstracted away
(`raptorpath-math/tests/temporal_oracle.rs`, 3 tests, all green).

**The temporal correction (what the old oracle lacked).** A coded symbol is a
combination over the sender's window *as of its send time*; it is striped to a
path and arrives one path-delay later. The old oracle credited every arrival
with whole-window rank, ignoring this. The corrected oracle models: send-time
window, per-path one-way delay, finite store, per-generation rank decode, and
the production *per-seq* reliability/ARQ/reorder layer that lives beneath the
coding.

**FIDELITY REPRODUCTION (corrected oracle vs L1, side by side).** A single
fitted constant — the throttled-recovery collapse stall (the ADR-0046
congestion-multiplier collapse), 190 ms — reproduces BOTH L1 numbers at once
(the het/sym ratio falls out, not fit). It reproduces the ×0.26 AND the
dual-worse-than-single signature:

| signature | L1 measured | corrected oracle |
|-----------|------------:|-----------------:|
| C8 het dual / fast-alone | ×0.26 | **×0.259** |
| C7 sym dual / fast-alone | ×0.36 | **×0.362** |
| coded single / systematic | ×0.85 | ×0.94 (codec cost only) |
| dual < single on BOTH sym & het? | YES (drag) | **YES** (0.259 & 0.362 < 1) |

The fidelity-reproduction test is `temporal_fidelity_reproduces_l1_refutation`.
Note (DERIVED): at W = W_mp = 640 the pure send-time *stranding* is negligible
(W ≫ D·owd_slow ≈ 140), so the drag is NOT information-theoretic temporal
misalignment — it is W-insensitive (matches L1's W = 200→2.0, W = 2048→2.4) and
is a **realization pathology**: the moving coding anchor makes each window's
per-path shares behave path-affine (fast path cannot cover a stranded slow
position — only a congestion-throttled per-seq ARQ can), plus a per-window
cross-path reorder tax present even on symmetric paths. The oracle reproduces
L1 only when this per-seq realization layer is modeled; the *ideal* fungible
temporal model aggregates. Honest: the fitted 190 ms sets the drag magnitude,
but the VERDICT below does not depend on it — only on generations structurally
avoiding the per-seq throttle.

**ALIGNMENT-FIX RESULT (does generation-based coding reach ×1.19?).** YES.
Replacing the moving window with fixed generations (code within each fixed
generation, stripe ∝ goodput, decode per-generation on any K_g symbols from any
paths, pipeline; fungible cross-path recovery) reaches the goodput ceiling with
NO drag (`temporal_alignment_fix_generation_coding`, `temporal_lever_
decomposition`):

| config (C8 het, K=20k, r=0.10) | factor |
|--------------------------------|-------:|
| goodput ceiling Σg/g_fast | ×1.195 |
| aligned generations, best (G≈384–512, M≥2) | **×1.194** |
| aligned generations, G=640 M=3 | ×1.181 |
| C7 symmetric control (G=640, M=3) | ×1.96 (no drag) |
| — lever decomposition — | |
| moving anchor + throttled recovery, M=1 (== L1) | ×0.21 |
| moving anchor + throttled recovery, M=3 | ×0.60 (pipelining alone: partial) |
| stable anchor + fungible recovery, M=1 | ×1.13 (stable anchor alone: works) |
| stable anchor + fungible recovery, M=3 (FULL FIX) | ×1.18 |

The **stable anchor is the dominant lever** (×0.21 → ×1.13); pipelining is
secondary (×0.21 → ×0.60).

**VERDICT: heterogeneous aggregation-above-fast-path IS ACHIEVABLE.** The
required production mechanism is **generation-based cross-path fungible coding
with a stable per-generation anchor**: generation size ≈ W_mp (best ×1.19 at
G ≈ 384–512 symbols at C8), pipeline depth M ≥ 2, ∝-goodput striping of each
generation's coded symbols, out-of-order per-generation decode, fungible
cross-path recovery, and — critically — NO per-seq targeted ARQ beneath the
code (the per-seq layer is what makes the moving window path-affine and invokes
the ADR-0046 throttle). This is a BUILD recommendation for a future arm; it is
NOT built here (oracle/model work only). The earlier ×1.19 "achievable" claim
was from an UNFAITHFUL oracle (no temporal alignment, no per-seq layer) — that
record is corrected in paper §16.3/§16.7: the ×1.19 was an idealization; the
shipped moving-window realization is correctly REFUTED (×0.26); the ×1.19 is
recoverable only under the stable-anchor generation design.

**Verification.** `cargo test -p raptorpath-math` green (4 existing
multipath_oracle tests + 3 new temporal_oracle tests: fidelity-reproduces-L1,
alignment-fix, lever-decomposition). No production code changed —
`cargo test -p raptorpath --lib` untouched/green. Reproduce:
`cargo test -p raptorpath-math --test temporal_oracle -- --nocapture`.

## Real-Trace Validation — is Gilbert-Elliott adequate for REAL loss? (branch `feat/real-trace-validation`, 2026-07-07)

The whole ladder above (formula ← oracle ← netem) is proven *for a GE world*.
This rung tests the bottom assumption itself: does the two-state Gilbert-Elliott
chain (§2.1) capture REAL link loss well enough for our r\*? Harness:
`raptorpath-math/tests/real_trace_validation.rs` (analysis/oracle only; no
production code changed — only new tests + vendored traces).

**Traces (provenance).** Five REAL U.S. cellular capacity traces —
Verizon-LTE-short, ATT-LTE-driving-2016, TMobile-UMTS-driving,
TMobile-LTE-short, Verizon-LTE-driving (down-link) — from the *Saturator*
tool (Winstein et al., NSDI 2013) via the mahimahi repo, vendored under
`tests/data/traces/` (see `PROVENANCE.md`; long traces time-truncated to bound
repo size). These are CAPACITY traces (per-ms 1500 B delivery opportunities),
so loss is DERIVED honestly: a drop-tail queue (offer ρ=0.5 of mean capacity,
64-packet buffer, drained at the trace's instantaneous capacity) turns each real
capacity fade into a real loss burst. Derived ε = 5.2%–24.5%.

**PART 1 — what GE misses** (`real_trace_ge_mismatch_structure`). Fit GE per
trace, compare its own predictions to the trace:

| structure | GE prediction | real measurement |
|-----------|---------------|------------------|
| autocorrelation, lag-20 | (1−p−q)²⁰ ≈ 0 | **5×–4104×** higher (e.g. 0.54 vs 0.0001) — long memory |
| burst-length tail | geometric | **3.8×–26×** heavier extreme tail (max bursts 210–597 sym) |
| stationarity | one (p,q) | ε swings 0%–87%, q̂ swings up to 0.47 within a trace |

**PART 2 — r\* fidelity on real loss** (`real_trace_r_star_fidelity`). Compute
r\* from the closed form (§8.4) fitted to each trace's (ε, σ²_burst), full
burst-variance margin, then run the ACTUAL real loss sequence through the FEC/ARQ
window process (W=50). Achieved residual window-failure vs target δ/ε vs the
GE-ideal (1 − P_fec exact DP, §8.7) at the same r\*:

| trace | ε | σ² | tgt δ/ε | r\* | real WF | GE-ideal WF | real/GE |
|-------|---|----|---------|-----|---------|-------------|---------|
| Verizon-LTE-short | 8.0% | 4.6 | 0.02 | 0.27 | 0.135 | 0.043 | 3.1× |
| ATT-LTE-2016 | 13.5% | 12.6 | 0.02 | 0.56 | 0.161 | 0.067 | 2.4× |
| TMobile-UMTS | 24.5% | 20.5 | 0.02 | 1.07 | 0.255 | 0.082 | 3.1× |
| TMobile-LTE-short | 8.4% | 19.9 | 0.02 | 0.49 | 0.106 | 0.067 | 1.6× |
| Verizon-LTE-driving | 5.2% | 6.3 | 0.02 | 0.23 | 0.086 | 0.056 | 1.5× |

Worst case: real residual = **12.8× the target**, and **3.1× worse than the
GE-ideal** the model predicts for the SAME r\* — even at r\* up to ~100% overhead
and even with the exact-DP r\*. The gap beyond GE-ideal is pure channel-model
mismatch: σ²_burst inflates lag-1 variance but cannot cover long memory / heavy
fade tails / regime shifts.

**PART 3 — generation-coding aggregation on real per-path traces**
(`real_trace_generation_oracle_aggregation`). Two DIFFERENT real traces
(Verizon-LTE-short fast + ATT-LTE-2016 slow) as two independent paths through the
validated stable-generation design, at the C8 rates/OWD that produced the ×1.19
GE reference, with per-path GE draw replaced by real loss replay:

| config | factor | efficiency (factor/ceiling ×1.188) |
|--------|-------:|-----------------------------------:|
| REAL per-path loss | **×1.178** | 0.991 |
| GE control (fitted p,q) | ×1.180 | 0.994 |

Real per-path burst structure does NOT break the aggregation mechanic — it
tracks its GE control and the real goodput ceiling.

**VERDICT.**
- **GE is INADEQUATE for real single-path loss w.r.t. r\*.** It under-provisions
  the tail by ~2×–4× beyond its own GE-ideal prediction (up to 12.8× the target),
  because real loss has long memory, heavy fade tails, and non-stationarity that
  a stationary two-state Markov chain omits. σ²_burst is only a partial fix.
- **Generation-coding aggregation IS ROBUST** on real independent per-path
  dynamics (×1.178 ≈ GE ×1.19).
- **Recommended enrichment** (recommendation only, not built): move beyond a
  single stationary GE to a semi-Markov/heavy-tailed-sojourn burst model (fade
  tail + long memory) or a regime-switching hierarchical model (non-stationarity),
  and provision r\* against the *empirical* window-loss quantile rather than the
  Gaussian/GE tail.

**CORRELATION GAP (open milestone).** Public single-path traces are independent
by construction — this tests real per-path *dynamics*, NOT path *correlation*
(shared-bottleneck WiFi+LTE losing together). Settling correlation needs
simultaneous dual-link capture or a dual-radio hardware testbed. Not claimed
here.

**Verification.** `cargo test -p raptorpath-math` green (all suites; 4 new
`real_trace_validation` tests). No production code changed —
`cargo test -p raptorpath --lib` untouched. Reproduce:
`cargo test -p raptorpath-math --test real_trace_validation -- --nocapture`.

## Generation Coding — production build of the stable-anchor design (branch `feat/generation-coding`, 2026-07-07)

The culminating build: implement the oracle-validated generation-based cross-path
fungible coding and measure it at L1. **Oracle-config confirmation → build →
measure.** Outcome: oracle **CONFIRMED**, codec + design **IMPLEMENTED and
codec-verified**, and the L1 decisive number is an **honest FAIL-WITH-MECHANISM**
(the multi-generation transport does not yet complete over the real datagram
path). The generation-coding *mechanism* is correct; a residual transport
plumbing gap — the missing per-generation **deficit feedback** — holds
production heterogeneous aggregation OPEN.

### PART 0 — Oracle-config confirmation (`raptorpath-math/tests/temporal_oracle.rs`)
Re-ran the corrected temporal oracle to confirm the EXACT production config
reaches the ceiling AND that the losing config reproduces the L1 ×0.26:

| config (C8 het, K=20k, r=0.10) | factor |
|--------------------------------|-------:|
| goodput ceiling Σg/g_fast | ×1.195 |
| aligned generations, best **G=384, M=2** | **×1.194** |
| aligned generations, G=512 / G=640 M=2 | ×1.189 / ×1.181 |
| stable anchor + fungible recovery, M=1 | ×1.134 (stable anchor alone) |
| moving anchor + throttled recovery, M=1 (== L1 refutation) | **×0.259** |
| C7 symmetric control (G=640, M=3) | ×1.961 (no drag) |

So the production parameters (**G=384, M=2, ∝-goodput striping, generation-level
recovery, NO per-seq ARQ**) reach ×1.19 in the oracle, and dropping to a moving
anchor + per-seq ARQ reproduces the ×0.26 drag. CONFIRMED — the build targets the
WINNING config. 3 tests green.

### PART 2 — Implementation (substrate + what shipped)
**Substrate chosen: extend the existing coded-only sliding-window path** (least
invasive) rather than a fresh RLC stack. The key realization: a generation-coded
symbol is an RLC repair over a FIXED span `[g·G, g·G+gen_len)`, so it carries the
IDENTICAL self-describing wire header (`window_start` = the STABLE generation
anchor, `window_count` = K_G), and the existing `RlcWindowDecoder` decodes each
generation's K_G×K_G system independently the instant K_G independent symbols for
that anchor arrive — **zero decoder change**. Only a stable-anchor *encoder* and
the ARQ-disable are new.
- **`raptorpath/src/fec/generation.rs`** — `GenerationEncoder` (impl
  `WindowEncoder`): fixed generations of `RWM_GEN` (384) source symbols, coded
  ONLY when SEALED so every coded spans the full width, round-robin over M
  in-flight generations, per-generation proactive budget + frontier recovery cap.
- **`window_generation_coding` flag** (config / CLI / `PeerConfig`) composing with
  the object/perf bulk path; realtime + in-order stream untouched. Implies
  coded-only wire + out-of-order delivery.
- **ARQ OFF** — the receiver installs no NACK producer in generation mode
  (`recv_nack_tx = None`); no sent-data store, no retransmit buffer, no tail
  sweep. Recovery is generation-level (more coded for a short generation,
  fungible cross-path), never a per-seq resend — exactly the design's contract.
- **Pipeline** — ack-clocked flow-control window bounds coded to
  `ack·(1+r) + W_inflight` ahead of the decode frontier (bounds QUIC buffer),
  plus a fixed-rate pacing token bucket.
- **Verification (green): `fec::generation` unit tests** — decode-on-K,
  out-of-order recovery under loss, per-generation independence, pipeline-depth
  bound. `cargo test -p raptorpath --lib` green (257 tests). The full-transport
  loopback tests `perf_loopback_generation_object` / `_dual_path` are
  `#[ignore]`d (see the DECISIVE note) with a documented reason.

### PART 3 — L1 measurement: DECISIVE C8 (c2+c3, 50 MB native perf)
**Result: does NOT beat the 15.7 Mbit/s fast-path-alone bar — a
FAIL-WITH-MECHANISM.** The build does NOT complete the multi-generation object
over real netem (nor over a real-RTT single path), so there is no aggregation
number to report. The failure is localized precisely (instrumented on the VM,
release build), and it is **NOT the generation design**:

- **The first generation decodes correctly end-to-end on real netem** — the full
  stable-anchor + out-of-order + generation-level-recovery + no-per-seq-ARQ
  pipeline works over real per-path timing/loss for one generation. Both paths
  deliver (dual-path striping via `place_symbol` works), symbols arrive unique
  (no dedup), zero object-path send/datagram-size failures.
- **Generations after the first stall.** The cumulative-ack frontier advances one
  generation, then wedges. Root cause, isolated by instrumenting the encoder's
  frontier span vs the decoder's rank: the sealed generation IS full in the
  encoder (`base_contig = G`) and its coded ARE full-span, but they arrive
  **bursty** on the droppable QUIC datagram path and are dropped faster than the
  O(G²)-per-symbol decode keeps up, so the frontier generation never reaches K_G
  — and the **feedback-free recovery cap** then deadlocks it (once the cap is hit
  the sender emits no more, and with no per-generation deficit signal it cannot
  know the generation is still short). Fixed-rate pacing lifted completion from 1
  to ~2 generations but did not close it.

**The named missing mechanism: per-generation DEFICIT FEEDBACK.** The design
(§16.3, oracle) assumes the receiver tells the sender each generation's residual
rank ("generation g needs N more coded"). The build used the cumulative ack as a
feedback-free proxy, which cannot simultaneously (a) bound recovery (unbounded →
floods/bursts) and (b) fund the frontier generation under backpressure (bounded →
starves/deadlocks). Closing this needs a `GenerationDeficit` control message +
receiver per-generation rank tracking — a scoped next step, not a redesign.

### VERDICT
- **Oracle: ×1.19 CONFIRMED** for G=384/M=2 (Part 0). The design is sound.
- **Codec + mechanism: IMPLEMENTED and VERIFIED** (`fec::generation` unit tests;
  generations decode on K, out-of-order, no per-seq ARQ). Paper §16.3/§16.7
  updated from build-recommendation to IMPLEMENTED.
- **L1 DECISIVE (>15.7 Mbit/s): NOT MET — honest fail-with-mechanism.** The
  production transport does not complete multi-generation transfers over the real
  datagram path; the missing piece is per-generation deficit feedback (named,
  scoped). Heterogeneous aggregation-above-fast-path remains **oracle-proven but
  not yet L1-realized in production**. The number was NOT forced.
- **Regression:** single-path native / C7 not measured to completion (the
  multi-generation stall blocks them too); the non-generation modes
  (systematic/coded-only, realtime, in-order stream) are untouched by the flag.

**Verification.** `cargo test -p raptorpath --lib` green (257, incl. 4
`fec::generation`); `cargo test -p raptorpath-math --test temporal_oracle` green
(3). Reproduce the oracle: `cargo test -p raptorpath-math --test temporal_oracle
-- --nocapture`. The generation transport is behind `--window-generation-coding`
(requires `--window-reliable`); `RWM_GEN` / `RWM_PIPELINE` tune G / M.

## Generation Coding — per-generation DEFICIT FEEDBACK landed + L1 MEASURED (branch `feat/gen-deficit-feedback`, 2026-07-07)

The named missing mechanism from the build above — **per-generation deficit
feedback** — is now implemented, and the multi-generation transport **COMPLETES
end-to-end over real netem**, closing the prior build's stall. But the DECISIVE
C8 goodput still **FAILS the >15.7 Mbit/s bar** — and the binding constraint has
MOVED: it is no longer a transport-plumbing deadlock (fixed) but the **RLC
generation DECODE throughput** (O(G²) incremental GE), which sits below the link
rate at the oracle's aggregating G, so heterogeneous aggregation has no headroom.
This is an **honest fail-with-mechanism**, one layer deeper than before.

### The mechanism that shipped (deficit feedback, §16.3)
- **Receiver rank tracking** (`WindowDecoder::rank_in`, impl in `rlc_window.rs`):
  independent rank the decoder holds over a generation's span = solved sources +
  un-resolved pivot rows in `[anchor, anchor+K_g)`. `deficit_g = K_g − rank_g`.
- **Wire** (`ControlMessage::GenerationDeficit { deficits: Vec<(anchor, u32)> }`):
  the receiver reports each frontier generation's residual deficit, learning K_g
  self-describingly from the coded wire header (`window_count`). Sent on decode
  progress AND on a periodic ~SRTT timer (the timer is essential — a sender that
  emitted its budget and went quiet must still be re-pulled; without it the loop
  deadlocks when no data is flowing).
- **Sender** (`GenerationEncoder::generate_repair_for(anchor)` + the recovery loop
  in `run_window_sender`): on a deficit report, emit exactly `deficit − in_flight`
  MORE coded for each generation, round-robin, paced — bypassing the ack-clocked
  target (the ack is stalled precisely when recovery is needed). In-flight
  accounting (`emitted − emitted_at_last_report`) implements the classic
  rateless-with-feedback loop: send the deficit, wait ~RTT, re-evaluate — never
  re-send the full deficit each tick. Proactive emission is now capped at the
  per-generation budget K_g(1+r); recovery beyond it is PURELY deficit-driven
  (the fixed feedback-free recovery cap is removed). Per-seq ARQ stays OFF.
- **Delivered-goodput pacing**: the token-bucket rate is clocked to the measured
  ack (decode) rate ×1.5 (floor `RWM_GEN_RATE_FLOOR`), so coded emission does not
  outrun the receiver's decode and overrun the droppable datagram path.

### Multi-generation completion: YES (the prior build's stall is CLOSED)
- **Loopback (in-proc, real QUIC):** all three generation loopback tests
  un-ignored and green — `perf_loopback_generation_object` (1 MB, ~3 gens),
  `_dual_path` (1 MB), and the new `_multi_dual_path` (2 MB, ≥5 gens over a dual
  path), all `dnf:0`.
- **L1 real netem, C8 (c2+c3):** multi-generation 50 MB transfers COMPLETE
  **6/6 (`dnf:0`)** at G=96/M=2, and complete at the oracle's G=384/M=2 too. The
  prior build stalled at 1–2 generations; the deficit loop pipelines all of them.

### DECISIVE C8 (c2+c3, 50 MB native perf, dual path) — the number
| config | mean Mbit/s | median s | stdev s | completion | vs 15.7 |
|--------|------------:|---------:|--------:|-----------:|--------:|
| **C8 dual, generation G=96 / M=2, 50 MB ×6** | **10.97** | 35.73 | 2.41 | **6/6** | **✗ 0.70×** |
| single-path c2, generation G=96 / M=2, 50 MB ×3 | 10.95 | 36.64 | 0.50 | 3/3 | ✗ |
| C8 dual, generation G=384 / M=2 (oracle cfg), 20 MB ×3 | — | — | — | **0/3 DNF (300 s timeout)** | ✗ |
| C8 dual, generation G=384 / M=2, 10 MB ×2 | 3.43 | — | — | 2/2 | ✗ 0.22× |
| C8 dual, generation G=192 / M=2, 10 MB ×2 | 11.05 | — | — | 2/2 | ✗ |

**Aggregation factor = 10.97 / 10.95 = 1.00 (NONE), matched at 50 MB.** The
decisive, apples-to-apples comparison: adding the second heterogeneous path (c3)
to the fast path (c2) yields ZERO extra goodput. That is the failure signature —
the receiver is already at its decode ceiling on one path. And at the oracle's
**G=384 the decode is so slow the pipeline STALLS at 20 MB (0/3, 300 s timeout)** —
it only "completes" at toy 10 MB objects (3.43 Mbit/s); it cannot sustain a real
transfer, so the ×1.19-aggregating config is not viable in this decoder at all.

### The mechanism of the shortfall: DECODE-BOUND (not the deficit loop)
Three independent measurements localize it to the decoder, not the network or the
feedback mechanism:
1. **Throughput scales inversely with G** (C8 dual, 10 MB): G=384 → 3.4, G=192 →
   11.0, G=96 → 12.6 Mbit/s — the O(G) total-decode-work signature (decode cost
   per symbol ~O(G²), K/G generations ⇒ work ∝ G).
2. **Network-independent ceiling at the oracle's G:** at G=384, in-proc loopback
   (localhost, no bandwidth limit, no loss) delivers ~3.0 Mbit/s and the 100 Mbit
   C8 fast path delivers ~3.4 — the SAME. The wall is CPU decode, not the link.
3. **No aggregation headroom:** single-path c2 (10.95 Mbit/s, 50 MB) = dual C8
   (10.97 Mbit/s, 50 MB) at G=96 — factor 1.00. The receiver cannot decode the
   POOLED cross-path arrivals any faster than one path already supplies, so a
   second path cannot help. Deepening the pipeline (M×G held at 384: G=96/M=4,
   G=64/M=6, G=128/M=3) all give ~10 Mbit/s — it does not lift the ceiling.

At the oracle's **G=384 — the exact config proven to reach ×1.19** — the RLC
generation decode runs at 3.4 Mbit/s, ~4.6× BELOW the 15.7 fast-path bar, so the
aggregation the oracle predicts is unreachable in this decoder. Shrinking to G=96
lifts throughput to ~11 Mbit/s (now CC/loss-bound on the real paths) but forfeits
the cross-path fungibility horizon (W_mp ≳ 384) AND still falls below 15.7 with no
aggregation. The decode/aggregation tension is fundamental to THIS decoder
(`RlcWindowDecoder`: incremental GE with per-pivot `BTreeMap<u64,u8>` coefficients
+ cascade — allocation-heavy, far below the paper's SIMD-dense-GE 708 Mbit/s claim).

### Secondary finding (not the binding constraint)
On C8 a coded symbol (1200 B + 14 B repair header + framing ≈ 1260 B) occasionally
exceeds path-1's negotiated QUIC `max_datagram_size` → `WARN … datagram too large
path=1` → that symbol is dropped. Rare in the runs measured (0 in the 50 MB single
baseline), but on the aggregating G it further erodes the second path's
contribution. A generation-mode symbol-size clamp to the min-PMTU across paths
would remove it; it does not change the decode-bound verdict.

### Regression (non-generation modes — untouched by the flag, RE-CONFIRMED)
| baseline (my build, no `--window-generation-coding`) | mean Mbit/s | target | status |
|------------------------------------------------------|------------:|-------:|:------:|
| single-path c2, coded reliable, 50 MB ×3 | **15.66** | ~15.7 | ✓ |
| C7 (c2+c2), coded reliable, 50 MB ×3 | **21.25** | ~21.6 | ✓ |

The `GenerationDeficit` wire variant, the two new trait methods (`rank_in`,
`generate_repair_for`), and the receiver/sender deficit plumbing are all additive
and gated on generation mode; systematic / coded-only / realtime / in-order-stream
are byte-for-byte unchanged and still meet their bars.

### VERDICT (updated)
- **Deficit-feedback mechanism: IMPLEMENTED + VERIFIED.** Completion is achieved
  where the prior build stalled — 6/6 on C8 50 MB, all loopback generation tests
  green, `cargo test -p raptorpath --lib` 258 green, gate suite 15/15 release,
  `temporal_oracle` 3 green. The mechanism the goal named is DONE and works.
- **L1 DECISIVE (>15.7 Mbit/s): NOT MET.** C8 = 10.97 Mbit/s (6/6), aggregation
  factor ≈ 1.0. Honest FAIL-WITH-MECHANISM — the number was NOT forced.
- **The blocker moved one layer down:** from a transport deadlock (fixed by the
  deficit loop) to **RLC generation-decode throughput** (O(G²) incremental GE,
  BTreeMap representation), which is CPU-bound below the link rate at the oracle's
  aggregating G — so cross-path aggregation has no headroom. Realizing the
  oracle's ×1.19 in production now requires a **fast dense/SIMD GF(256) generation
  decoder** (the paper's 708 Mbit/s decode-cost claim assumes one; the shipped
  `RlcWindowDecoder` is ~200× slower), a scoped codec-performance step — not more
  transport work.
- **Regression:** none (single-path 15.66, C7 21.25; non-generation modes intact).

## Generation Coding — FAST DENSE DECODER landed; decode UNBLOCKED but C8 aggregation still fails (branch `feat/fast-gen-decoder`, 2026-07-07)

The named blocker from the build above — **RLC generation-decode throughput** — is
now removed: a dense per-generation GF(256) Gauss–Jordan decoder replaces the
sparse `RlcWindowDecoder` on the generation path, **27× faster at G=384**, and
the oracle's G=384 config that previously **DNF'd at 20 MB now COMPLETES**. But
the **DECISIVE C8 goodput still FAILS the >15.7 Mbit/s bar** — and the binding
constraint has moved AGAIN, one layer below decode: to the **coded-datagram
transport control loop** (ack-clocked pacing over the unreliable QUIC datagram
path + per-generation decode-on-K / deficit-feedback RTT). Decode is no longer
the bottleneck; the second path still adds no capacity. Honest
FAIL-WITH-MECHANISM, one layer deeper than before. The number was NOT forced.

### PART 1 — the fast decoder (route (b): densify the generation decode path)
Route (a) — reuse the "708 Mbit/s bench" decoder — was NOT viable: that bench
used a *dense* solver, whereas BOTH the production `RlcWindowDecoder` (sliding
window) AND `raptorpath-math`'s `RlcDecoder` store per-pivot coefficients
**sparsely** (`BTreeMap<u64,u8>` / `Vec<(u64,u8)>`) with cascade — there was no
existing dense decoder to reuse. So I built one: **`GenerationDecoder`**
(`raptorpath/src/fec/generation.rs`, impl `WindowDecoder`):
- **Dense fused rows.** Each pivot row is ONE contiguous `[coeffs (K_G) | payload
  (symbol_size)]` buffer, so a single SIMD `mul_acc_slice` (the existing
  AVX2/SSSE3 PSHUFB GF(256) kernel) eliminates both the coefficient and payload
  halves per pivot — halving the per-call table-build/dispatch overhead.
- **Incremental reduced-row-echelon Gauss–Jordan**, keyed by `(anchor, K_G)`.
  Each generation is a self-contained K_G×K_G system; at full rank every source
  is already an identity row and delivered in one shot (no cascade, no back-sub).
  Per-generation independent, decode-on-K, out-of-order — identical contract to
  the sparse path (unit test `gen_decoder_matches_rlc_window` asserts byte-exact
  parity vs `RlcWindowDecoder` on a lossy, reordered stream).
- **Known-source pre-loading.** A fresh generation pre-loads any already-recovered
  source in its span as a unit pivot row (the sparse decoder's Step-1
  elimination), so an overlapping trickle channel (the reverse per-object ACK
  stream re-codes the same anchor at widths 1,2,3,…) still makes progress. Zero
  cost for the disjoint-span large-object case. **This was load-bearing** — two
  real bugs surfaced and were fixed here: (1) the object stream reuses the
  absolute seq space across objects, so one anchor hosts different-K_G
  generations — keying by `(anchor,K_G)` (not anchor alone) stops them
  thrashing; (2) without known-source pre-loading the reverse ACK channel
  deadlocked (the 2nd object never acked). All three generation loopback tests
  green (`perf_loopback_generation_{object,dual_path,multi_dual_path}`).

**Decode microbench (dev box, AVX2; `fec::generation::tests::bench_generation_decode_throughput`, 1200 B symbols, single core):**

| G   | dense (this build) | sparse `RlcWindowDecoder` | speedup |
|-----|-------------------:|--------------------------:|--------:|
| 96  | 405 Mbit/s | 67 Mbit/s | 6.1× |
| 192 | 201 Mbit/s | 16 Mbit/s | 12.8× |
| **384** | **83 Mbit/s** | **3.1 Mbit/s** | **27×** |
| 512 | 66 Mbit/s | 1.7 Mbit/s | 38× |

At the oracle's **G=384 the dense decoder does 83 Mbit/s — clear of the 100 Mbit
link and ~9× the 8.9 Mbit/s the transport actually achieves**, so decode is
provably NO LONGER the binding constraint. (Effective ~3.8 GB/s is per-call
PSHUFB-table-build bound; throughput is O(G) per delivered byte, so it falls with
G but stays far above the cells for every G ≤ 512.)

### PART 2 — L1 MEASURED (C8 = c2+c3, native perf, VM AVX2)

**FIRST: G=384 now COMPLETES.** Isolated single-object 25 MB transfers at
G=384/M=2: **16/16 completed, dnf:0** (8 dual + 8 single). The prior build DNF'd
G=384 at 20 MB (300 s timeout); the fast decoder closes that. (Caveat: a *warm
connection carrying 6 sequential* 50 MB objects still intermittently stalls on a
later object — a multi-object-on-one-connection transport issue, NOT single-object
decode; single-object completion is now reliable.)

**DECISIVE C8 (c2+c3), G=384/M=2, 25 MB × 8 isolated:**

| config | mean Mbit/s | median | stdev | completion | vs 15.7 |
|--------|------------:|-------:|------:|-----------:|--------:|
| **C8 dual, generation G=384/M=2** | **8.90** | 8.94 | 0.31 | 8/8 | **✗ 0.57×** |
| single c2, generation G=384/M=2 | 9.11 | 9.13 | 0.14 | 8/8 | ✗ |

**Aggregation factor = 8.90 / 9.11 = 0.98 — NO aggregation** (dual marginally
BELOW single, as before). Adding the second heterogeneous path (c3) yields zero
extra goodput even though decode now has ~9× headroom. So decode was necessary
but NOT sufficient: the ×1.19 the oracle predicts is still unreachable in
production.

### The mechanism of the (remaining) shortfall: CONTROL-LOOP BOUND, not decode
Client-side trace (`RWM_TRACE`, G=384 dual) shows the uploader is
**`tx_paused=true` in ~90% of samples** with the retention window pinned at
`win=1152` (= 3 generations) and coded emission running only ~1.3× the delivered
ack. The sender is not decode-limited (decode is 83 Mbit/s) — it is throttled by
the generation-mode control loop:
- **Ack-clocked coded pacing over the DROPPABLE datagram path.** Coded symbols
  ride QUIC's unreliable datagram path; the pacing is deliberately clocked to the
  delivered-ack rate (×1.5) so it does not overrun the datagram intake. Pushing
  it faster does not help — it hurts: `RWM_GEN_RATE_FLOOR=6000` (force ~57 Mbit/s
  coded) DROPPED C8 dual to **5.2 Mbit/s** (overrun → datagram drops → generations
  stall); `RWM_GEN_INFLIGHT=6000` left it unchanged (8.3). The knob that would
  raise throughput is exactly the one that overruns the path.
- **Per-generation decode-on-K / deficit RTT serialization.** Nothing in a
  generation delivers until all K_G=384 independent symbols arrive; the frontier
  advances in 384-symbol jumps gated by the slow path's (c3: 40 ms RTT, 4.8 %
  loss) stragglers, and the deficit-feedback top-up costs a further RTT. With
  M=2, only 2 generations hide that latency. Deepening M helps only weakly
  (G=384: M=2→8.9, M=4→8.4, **M=8→10.6** Mbit/s) and never reaches the bar.

### G-sweep (C8 dual, 25 MB × 3 isolated) — smaller G is faster, none aggregate
| G | mean Mbit/s | completion | note |
|---|------------:|-----------:|------|
| 96 | **12.03** | 3/3 | L1 sweet spot, but forfeits the W_mp≳384 fungibility horizon |
| 192 | 9.65 | 2/3 (1 DNF) | flaky |
| 384 | 8.90 | 8/8 | the oracle's aggregating config |
| 512 | 9.16 | 3/3 | |

Throughput FALLS with G and NONE aggregate (all < 15.7, dual ≈ single). Even the
best (G=96, 12.0) forfeits the cross-path fungibility horizon (W_mp ≳ 384, §16.5)
and still misses the bar. This is the *inverse* of the prior build's signature
(which was decode-bound, ∝1/G): here decode has huge headroom at every G, so the
falloff with G is the per-generation decode-on-K/RTT serialization, not decode
cost.

### Regression (systematic / non-generation modes — RE-CONFIRMED clean)
| baseline (plain `--window-reliable`, no generation flag) | mean Mbit/s | target | status |
|----------------------------------------------------------|------------:|-------:|:------:|
| single-path c2, 50 MB ×3 | **15.55** | ~14.5–15.7 | ✓ |
| C7 (c2+c2), 50 MB ×3 | **21.39** | ~21.6 | ✓ |
| C8 (c2+c3), 50 MB ×3 | 12.11 | ~12.6 | ✓ |

The dense decoder is gated on generation mode (`create_window_decoder(…,
generation)`), so systematic / coded-only / realtime / in-order-stream are
byte-for-byte unchanged and meet their bars. (Note the fast-path-alone bar
*is* the 15.55 single-path systematic number here; generation C8 dual at 8.90 is
0.57× of it — and even plain systematic C8 dual, 12.11, beats generation C8.)

### VERDICT
- **Decode: FIXED and PROVEN.** Dense `GenerationDecoder` is 27× the sparse path
  at G=384 (83 vs 3.1 Mbit/s), clears the link rate, and unblocks G=384
  completion at L1 (16/16 isolated). `cargo test -p raptorpath --lib` 261 green
  (incl. 3 new dense-decoder unit tests), all generation loopback tests green,
  gate suite 15/15 release, `temporal_oracle` 3 green.
- **L1 DECISIVE (>15.7 Mbit/s): NOT MET.** C8 dual G=384 = **8.90 Mbit/s** (8/8),
  aggregation factor **0.98**. Honest FAIL-WITH-MECHANISM — number NOT forced.
- **The blocker moved one layer DOWN AGAIN:** from generation-decode throughput
  (fixed here) to the **coded-datagram transport control loop** — ack-clocked
  pacing over the unreliable datagram path + per-generation decode-on-K/deficit
  RTT. This is not a decode or a plumbing gap; it is the fundamental tension of
  racing a rateless coded stream over a droppable datagram path at a
  bandwidth-limited, high-RTT heterogeneous cell. Closing it would need a
  different emission model (e.g. systematic-symbol pass-through to kill the
  decode-on-K latency, or coded-on-a-reliable-substream), beyond this goal's
  decode-fix scope.

## Systematic+Repair Oracle — a cheaper realization than coded-only generations (branch `feat/oracle-systematic-repair`, 2026-07-07)

The coded-only generation design above reached the oracle's ×1.19 but died at L1
(×0.98, 8.9 Mbit/s) on three structural costs of making **every** symbol a coded
combination: whole-object/whole-generation **O(G²) decode**, **decode-on-K
latency** (nothing delivers until K_G independent symbols land), and a fragile
**ack-clocked coded-datagram** emission loop. This rung validates — in the
oracle, before anyone builds it — a DIFFERENT realization that keeps the same
cross-path fungibility **more cheaply**: **SYSTEMATIC source + deficit-driven
cross-path REPAIR**. Model: `raptorpath-math/tests/temporal_oracle.rs` PART 3
(4 new tests; oracle/model only, no production code). It extends the same
temporal machinery as the corrected oracle (send-time events, per-path OWD,
per-path independent GE = netem, work-conserving pull, deficit feedback on the
fast path).

**The design modelled.** K source symbols are striped work-conserving (each
source pulled by exactly ONE path at its rate; the fast path pulls ~83 %) and a
delivered source is one degree of freedom used **directly** — zero decode,
out-of-order (object = all K recovered, any order). A path with no fresh source
emits **windowed REPAIR** — an RLC over the live window `[F−W_span, F)` — placed
on the best path; a received repair is one dof that substitutes for ANY missing
source in its window. The receiver's rank = distinct source + independent
repair; it decodes the **deficit only** (a tiny dense solve over the local
holes) and completes at rank K = K/(g_fast+g_slow) → full aggregation.

### The four answers (DERIVED/MEASURED in-oracle, C8 = c2+c3, r=0.06, W_span≈486)
| question | answer |
|----------|--------|
| **Q1 AGGREGATION** | C8 het **×1.188** (99.4 % of ceiling ×1.195); C7 sym **×1.992** (~2×, no drag); arq_used=0 (pure fungible repair, no per-seq ARQ) |
| **Q2 REPAIR φ BOUNDED?** | φ_total = **0.060** (= r, the loss-FEC baseline, bounded); **structural φ_tail → 0** with K: 0.0030 (5 MB) → 0.0000 (25/50/200 MB). The deficit-driven cross-path repair ≈ the slow in-flight window (≈32 sym), K-independent ⇒ φ_tail = O(1/K). **No structural deficit.** |
| **Q3 DECODE SIZE** | max concurrent unknowns = **7–10 symbols** (1.8–2.6 % of G=384), **K-independent** (7 at 25 MB, 7 at 50 MB, 10 at 200 MB). The dense solve is O(deficit²) over ≈10 unknowns, NOT O(384²), and does not grow with the object. |
| **Q4 CONTRAST / knob** | in-order+affine+store = **×0.932** (faithfully reproduces the paper's ≈0.92 fork-join long pole); in-order+**cross-path repair** = **×1.188** (repair advances the frontier fungibly → pole removed); out-of-order affine = **×1.171** (the ≈0.92 pole is an **in-order artifact**, absent in the bulk regime); provisioning sweep: r < ~ε → **DNF** (mid-object losses strand past the W_span horizon), r ≥ 0.05 (≈1.5·ε) → ceiling. |

### VERDICT: BUILD (this is the design to build)
Systematic + deficit-driven cross-path windowed repair reaches the ×1.19 ceiling
with (a) **bounded φ → r** and vanishing structural deficit, (b) a **tiny
K-independent deficit-decode** (~10 unknowns vs 384), (c) **no decode-on-K**
(source delivers on arrival with zero decode), and (d) **no per-seq ARQ**
(recovery is fungible repair). It is strictly cheaper than coded-only on exactly
the two axes that sank the L1 build — decode cost and delivery latency — while
matching its aggregation. Honest caveat: the ×1.19 needs proactive **r ≳ ε** so
the windowed repair clears holes within the horizon; pure end-of-object deficit
repair (r=0) strands mid-object losses (DNF). And this is an independent-GE model
(same fidelity class as the corrected oracle); it does not model the QUIC
datagram control loop — but the two L1-killers are **structurally absent** here
(systematic source rides the reliable path with zero decode; the solve is ~10
symbols), so the residual risk is much smaller than for coded-only.

### Minimal production change (a MODIFICATION of the merged generation machinery)
Reuse striping + deficit-feedback + the dense GF(256) decoder already built;
drop coded-only primary:
1. **Systematic primary.** Send raw source symbols as primary (work-conserving
   pull, one path per source), delivered directly out-of-order — replace the
   coded-only-wire encoder's "every symbol coded" with source pass-through.
2. **Windowed cross-path repair.** Emit RLC repair over a bounded live window
   W_span ≈ W_mp (≈500 at C8) at a proactive rate r ≳ ε (covers losses inline)
   plus a deficit-driven top-up placed on the best path. Reuse the existing
   `GenerationDeficit` feedback, re-scoped from per-generation to per-window
   rank deficit.
3. **Deficit-only decode.** Solve just the local hole set (~10 symbols) with the
   existing dense `GenerationDecoder`, sized to the deficit — not to G. Kills
   both decode-on-K latency and the O(G²) per-object cost.
4. **No per-seq ARQ.** Recovery is fungible windowed repair only.

**Verification.** `cargo test -p raptorpath-math` green — all suites, incl.
`temporal_oracle` **7** tests (3 corrected-oracle + 4 new systematic-repair:
`systematic_repair_aggregation`, `_volume_bounded`, `_deficit_decode_size`,
`_provisioning_curve`). No production code changed. Reproduce:
`cargo test -p raptorpath-math --test temporal_oracle systematic -- --nocapture`.

## Systematic+Repair — PRODUCTION BUILD + L1 MEASURED (branch `feat/systematic-repair`, 2026-07-07)

The oracle-validated systematic + deficit-repair design above, built into
production as a MODIFICATION of the merged generation machinery and measured at
L1. Outcome: the design's **structural claims are VALIDATED in production** — it
completes robustly, decode is a non-factor, and the coded-only anti-aggregation
DRAG is removed — but the **DECISIVE C8 >15.7 Mbit/s bar is NOT met (15.0
Mbit/s, aggregation factor 0.99)**. Honest FAIL-WITH-MECHANISM, and the residual
constraint is now cleanly isolated to the **per-connection transport control
loop**, NOT the FEC: it is proven by a SYMMETRIC-path (C7 c2+c2) control that
also does not aggregate. The number was NOT forced.

### The production change (what shipped vs coded-only)
A submode of the generation machinery — reuses the fixed-generation repair
anchors, per-generation deficit feedback, dense `GenerationDecoder`,
out-of-order delivery, and no-per-seq-ARQ contract UNCHANGED. Two differences:
- **`GenerationEncoder::new_systematic`** (`fec/generation.rs`): the proactive
  per-generation budget is `ceil(len·r)` — the loss-FEC overhead ONLY — instead
  of coded-only's `ceil(len·(1+r))`. The K base degrees of freedom ride the wire
  as raw source, so coded symbols cover only the holes; the deficit loop
  (`generate_repair_for`) tops up the residual.
- **Systematic source on the wire** (`run_window_sender`): the raw source
  symbol is emitted as PRIMARY (striped ∝-goodput via `place_symbol`, delivered
  out-of-order with ZERO decode) — the per-source wire send that coded-only
  skipped, re-enabled with `sent_store`/retransmit/taper still OFF.
- Behind **`--window-systematic-repair`** (requires `--window-reliable`;
  `PeerConfig.window_systematic_repair`), composing with the perf/object bulk
  path; realtime + in-order stream untouched. `RWM_GEN` (~480 at C8) sets the
  repair-window / fungibility horizon, `RWM_GEN_R` (default 0.15 ≳ 1.5·ε) the r.

### Codec + mechanism: VERIFIED
- **`fec::generation` unit tests** (2 new): `systematic_budget_is_repair_overhead_only`
  (budget = ceil(len·r), strictly less than coded-only) and
  `systematic_source_primary_repair_recovers_deficit_only` — the four-claim proof
  over a lossy stream: received source delivered DIRECTLY (zero decode), windowed
  repair recovers the holes, the **deficit-decode == the hole count (≪ G)** (known
  sources pre-load as unit pivots, so the dense solve is O(deficit) not O(G²)),
  and NO per-seq resend anywhere.
- **`perf_loopback_systematic_repair_dual_path`** — end-to-end over a dual
  loopback link (source pass-through + windowed cross-path repair + deficit
  frontier, composing with the perf object protocol).
- `cargo test -p raptorpath --lib` **263** green (+2), `temporal_oracle` **7**
  green, `gate_suite` **15/15** release.

### DECISIVE C8 (c2+c3, 50 MB native perf, VM AVX2, G=480 / M=2 / r=0.15)
| config | mean Mbit/s | median s | stdev s | completion | vs 15.7 |
|--------|------------:|---------:|--------:|-----------:|--------:|
| **C8 dual, systematic-repair, 50 MB ×6** | **15.045** | 26.78 | 1.54 | **6/6 (dnf:0)** | **✗ 0.96×** |
| single c2, systematic-repair, 50 MB ×6 | 15.198 | 26.77 | 0.90 | 6/6 (dnf:0) | ✗ |

**Aggregation factor = 15.045 / 15.198 = 0.99 — NONE.** BUT note the absolute
rate: this is **1.24× the plain-systematic C8 dual (12.11)** and **1.69× the
coded-only C8 dual (8.90)** — the design lands at the FULL single-path rate with
**the anti-aggregation drag REMOVED** (coded-only and plain-systematic both put
C8 dual BELOW single; this build puts it AT single). Completion is robust (6/6 vs
coded-only's fragile G=384 stalls); φ ≈ **0.15** (= r, bounded) with the deficit
loop essentially IDLE (holes covered inline by proactive r) — so **decode and the
deficit loop are NOT the binding constraint**, unlike every coded-only build.

### The binding constraint: per-connection control loop (proven by a SYMMETRIC control)
| control config (50 MB ×6, G=480/M=2) | mean Mbit/s | vs single 15.2 |
|--------------------------------------|------------:|---------------:|
| **C7 SYMMETRIC dual (c2+c2), systematic-repair** | **15.445** | **×1.02 — NO aggregation** |

Two IDENTICAL 100 Mbit paths (no slow-path long pole, no heterogeneity, no
loss-rate skew) still yield only single-path throughput. This **rules out the
FEC-layer explanations** (decode, deficit-RTT, striping ∝-goodput, slow-path
laggard coverage, repair provisioning) — none of them apply on symmetric paths —
and localizes the ceiling to the **per-connection transport control loop**: a
single-path perf transfer extracts only ~15 Mbit from a 100 Mbit link (the same
~15 for plain-systematic AND generation modes), and adding a second path — even
an identical one — adds nothing. Client trace (`RWM_TRACE`, C8 dual): the sender
is **`tx_paused=true` in ~87% of samples**, backpressured by the generation-mode
store (`store_max = G·(M+1)`) which is pruned by the IN-ORDER cumulative ack —
so even though DELIVERY is out-of-order, RETENTION/backpressure is coupled to the
in-order frontier, serializing the paths to the single-path frontier-advance rate.

**Slack does not help — it overruns.** M-sweep (25 MB ×3): M=4 → **0/3 DNF**,
M=8 (single c2) → **0/3 DNF** (300 s timeout). Loosening the store lets the
sender push MORE onto the droppable QUIC datagram path, which then overruns
(drops → the object never completes) rather than aggregating. So the datagram
path CAN carry more than 15 Mbit, but the control loop cannot use it
productively — the exact "racing a rateless stream over a droppable datagram
path" tension the coded-only verdict named, here isolated to the SENDER'S
in-order-coupled backpressure rather than to decode.

### Regression (non-systematic modes — my build, RE-CONFIRMED clean)
| baseline (no `--window-systematic-repair`) | mean Mbit/s | target | status |
|--------------------------------------------|------------:|-------:|:------:|
| PLAIN reliable C7 dual (c2+c2), 50 MB ×3 | **20.006** | ~21.4 | ✓ (within variance; still ×1.3-aggregates) |
| single-path systematic-repair (native parity) | 15.198 | ~14.5–15.7 | ✓ |

The send-macro edit is byte-inert for non-systematic modes (the source-on-wire
branch only diverges when `window_systematic_repair` is set, and
`window_generation` only pulls systematic in when the flag is set): plain
reliable STILL aggregates on C7 (20.0 ×1.3, vs systematic-repair's 15.4 ×1.02 —
the crisp contrast that the generation-mode control loop is what forfeits
aggregation), and `fec::generation` / all generation loopback tests are green.

### VERDICT
- **Design structural claims: VALIDATED in production.** Systematic source
  delivers on arrival (zero decode), windowed repair recovers holes with a tiny
  deficit-decode, per-seq ARQ is off, and it **removes the two coded-only
  L1-killers** — proven by robust 6/6 completion at full single-path rate with an
  idle deficit loop and decode a non-factor. It also **removes the
  anti-aggregation DRAG** that put plain-systematic (12.11) and coded-only (8.90)
  C8 dual BELOW single: this build sits AT single (15.0).
- **L1 DECISIVE (>15.7 Mbit/s): NOT MET.** C8 dual = **15.045 Mbit/s** (6/6),
  aggregation factor **0.99**. Honest FAIL-WITH-MECHANISM — number NOT forced.
- **The binding constraint is the per-connection transport control loop, NOT the
  FEC.** Proven by the C7 SYMMETRIC control (15.4, ×1.02 — no aggregation with
  two identical paths) and the M-sweep overrun DNFs. A single perf connection
  extracts ~15 Mbit from a 100 Mbit link regardless of path count; the residual
  is the generation-mode sender's in-order-coupled backpressure + droppable-
  datagram overrun, one layer below everything the oracle models (the oracle's
  independent-GE model assumes each path delivers at its link goodput and uses an
  unbounded store — production's per-connection cwnd/pacing ceiling and bounded
  in-order-coupled store are outside that model). Closing it needs a transport
  change — decouple retention/backpressure from the in-order frontier (prune on
  per-generation completion) and/or grow the datagram-path send window without
  overrunning — beyond this design's FEC scope.

**Verification.** `cargo test -p raptorpath --lib` 263 green; `temporal_oracle`
7 green; `gate_suite` 15/15 release. L1: `RWM_GEN=480 RWM_EXTRA=
"--window-systematic-repair" bash ~/l1/perf_rwm_c.sh c2 c3 bulk 50000000 6 dual`.

## Unified deadline-constrained r* — N=1→§8.4 reduction + oracle fidelity (branch `feat/unified-rstar`, 2026-07-07)

Formalized the §16.7 "two knobs, one budget" claim: **H (reorder horizon) and r
(FEC rate) spend the SAME deadline D.** A symbol is late iff its total delay
(propagation + ARQ-recovery-if-not-FEC-covered + cross-path reorder wait)
exceeds D; the controller picks the minimal r s.t. P(late) ≤ δ across the path
set. Paper §8.9 (new): the P(late) decomposition, the overhead-minimization
(convex feasible set ⇒ r* is its boundary, KKT), the closed form
r*_unified = max_{i∈E}[ e_i/(1−e_i) + z_{δ_i/e_i}√(e_iσ²_i/(W(1−e_i))) ] with
E = {i : d_i − d_min ≤ H}, the limits/monotonicities, and the N=1 theorem.

### N=1 reduction — CONFIRMED (the correctness gate)

With one path d_1 − d_min ≡ 0, so E = {1}, the reorder term is identically 0,
D collapses to "within-window-or-ARQ", and r*_unified reduces **exactly** to
§8.4's r*(δ, e, σ², W). Oracle Part 4a (5 scenarios, K=1.2 M, W=64): at
r=r*(§8.4) every late symbol is an ARQ miss (measured reorder ≡ 0), the tail
== e(1−P_fec), and r* places that tail at 1.20–1.52×δ. §8.4 is the one-path
limit, not a separate formula.

### Oracle-fidelity (MEASURED-through-oracle, `temporal_oracle.rs` Part 4)

- **Reorder term & ordering-as-policy (4b):** in-order H<skew → p_reorder =
  0.258 ≈ slow-path goodput share 0.25 (E={fast}); H≥skew → 0.0025 (collapse);
  unordered → 0.000. The ordering flag is exactly what turns L_reorder on.
- **Monotonicities (4c):** P(late) ↓ in r, reorder ↓ in H, P(late) ↑ in e —
  all signs from the closed form confirmed.
- **Full-grid union bound (4d):** the closed form tracks the measured tail;
  worst ratio 1.37 and it OVER-estimates (conservative — a slow-path symbol
  can be both a reorder hole and an ARQ overflow; the bound double-counts).

### The one discrepancy (reported, not forced)

At r=r*(§8.4) the oracle miss tail is **1.2–1.5×δ** — the closed form
under-provisions; the oracle needs **≈1.51× r*** to actually hit δ. This is
the KNOWN §8.4/§8.7 Gaussian-tail + loss/repair-correlation gap, not a new
defect: the oracle confirms its sign and bounds its size (~1.5×, within the
§8.7 exact-DP band). Production uses r*_unified as the analytic floor and
`compute_min_rate_exact` (§8.7) to close it; r* is never a dangerous
over-estimate (r_min ≥ 0.85·r* always).

### Scope (honest)

r* is the FEC-rate controller for the **reliability/latency** budget —
orthogonal to the **throughput** ceiling. The L1 finding that heterogeneous
throughput aggregation is transport-ceiling-limited (this file, above) does
NOT affect r*: Part 4 credits each path's own FEC to its own budget (no
cross-path fungible repair), so its verdict is independent of the Parts 1–3
aggregation result. The r* model assumes only that the transport delivers at
per-path rates g_i.

**Verification.** `cargo test -p raptorpath-math` all green (temporal_oracle
now 11 tests: 7 prior + 4 new unified-r*); no production code (`raptorpath
--lib` untouched). DERIVED + MEASURED-through-oracle; no term trusted until the
oracle confirmed fidelity.

## Transport Ceiling — bufferbloat SERIALIZES dual-path aggregation (branch `feat/transport-ceiling`, 2026-07-07)

The systematic+repair merge above isolated the residual blocker to the
per-connection transport control loop (proven by a C7-symmetric control that
did not aggregate). This branch DIAGNOSED that loop by instrumentation (a new
`RWM_DIAG` per-250 ms constraint report) and L1 measurement, and landed a fix
for the part that is tractable. Outcome: **symmetric dual-path aggregation is
UNLOCKED (C7 c2+c2: 9.8 → 22.3 Mbit/s, ×1.43), and single-path is de-jittered
(50 MB×6 stdev 24.8 → 1.0 s), by decoupling backpressure from the bloated
retention window** — but the **DECISIVE heterogeneous C8 bar (>15.7) is STILL
NOT met (best ~14.5, high-variance median ~8–10)**: the heterogeneous slow-path
drag is a SEPARATE constraint that transport tuning does not touch. Number NOT
forced.

### DIAGNOSIS — the ~15 Mbit single-path ceiling is a per-symbol PROCESSING limit, NOT a window/CC problem
Instrumented single-path c2 (`RWM_DIAG`, sender side) + a controlled sweep
refute the "no CC ramping to BDP" hypothesis outright — throughput is
**window-, bandwidth-, and RTT-INDEPENDENT**:
| lever swept | result | conclusion |
|-------------|--------|------------|
| store_max (flow-control window) 1440→540 | throughput FLAT 15.0–15.7, RTT 700→74 ms | **not window-limited** (Little's law: only ~115 sym outstanding at store=540 ≫ below the 540 cap — rate-limited, not window-limited) |
| link bandwidth 100 Mbit→1 Gbit (plain reliable single, clean) | 28.7 → 29.7 Mbit | **not bandwidth-limited** (10× BW, no change) |
| RTT 10 ms (clean) vs 1 ms (c1) | 28.7 vs 29.7 | **not RTT-limited** |
| loss 0 (clean) → 2.6% (c2) | 28.7 → 16.4 | **loss-sensitive** (in-order-frontier head-of-line stalls) |

So the single-connection ceiling is a **per-symbol processing rate** (~3000
sym/s ≈ 29 Mbit clean, ~1700 sym/s ≈ 16 Mbit at 2.6% loss), UNIVERSAL across
window modes (plain reliable single 16.4, systematic-repair single 15.0) — it is
the transport substrate, ~4.5× below native quinn stream (~72 Mbit at C2, the
documented tunnel/processing gap), NOT the FEC and NOT a congestion window.

### DIAGNOSIS — the store_max = G·(M+1) bufferbloat SERIALIZES aggregation
`store_max = win_cap = G·(M+1) = 1440` at G=480/M=2 is **14× the C2 BDP (~104
sym)**. Because generation-mode source emission is backpressured only by this
(oversized) unacked-window bound, the unacked pipeline is a multi-hundred-ms
standing queue — `RWM_DIAG` MEASURED the sender's estimator RTT inflated to
**0.5–1.3 s** (true C2 RTT ≈ 10 ms), with the window pinned full and goodput
oscillating in a stall-burst sawtooth (repeated 0.0 Mbit stretches for ~1 s,
then 40–56 Mbit bursts). This bloat does NOT cap single-path throughput (it is
window-independent, above) but it (a) produces catastrophic slow-run outliers
and (b) **serializes dual-path aggregation**: the fast path stalls waiting on
the slow, bloated in-order-frontier cross-path feedback, so a second path — even
an IDENTICAL one — adds nothing (the C7-symmetric ×1.02 the merge reported), and
heterogeneous C8 falls BELOW single.

### THE FIX — backpressure at 2 generations (retention stays at M+1)
The send frontier needs only TWO generations outstanding to pipeline (one
filling head + one sealed-and-recovering), not M+1. Ship the generation-mode
`store_max` DEFAULT as `2·G` (RETENTION `win_cap = G·(M+1)` unchanged for decode
headroom), overridable by `RWM_STORE`. This decouples the standing queue from
the retention horizon. Also added (env-gated, default-off, no behavior change):
`RWM_DIAG` (the diagnostic instrument), `RWM_CODED_SRC` (clock the coded budget
to the SENT frontier, removing a small-G ack-clocked deadlock), `RWM_INFL_CAP`.

### L1 MEASURED (G=480/M=2, 50 MB, VM AVX2, netem independent qdiscs)
| config | store=G·(M+1)=1440 (before) | store=2·G=960 (after, DEFAULT) | factor |
|--------|----------------------------:|------------------------------:|-------:|
| single c2 (×6) | 11.2 mean, **stdev 24.8 s** | **15.4 mean, stdev 1.0 s** | de-jittered |
| **C7 c2+c2 symmetric (×6)** | **9.8 (×0.65 anti-agg)** | **22.4 (×1.43 AGG)** | **+128%** |
| C8 c2+c3 heterogeneous (×6–8) | 9.45 | 8.1 / 9.8 / 14.5 (high-var) | ~neutral |

- **Single-path RAISED + de-jittered**: the bloat's catastrophic slow-run
  outliers (stdev 24.8 s over 6 reps) are gone (stdev 1.0 s); mean 11.2 → 15.4.
- **Symmetric aggregation UNLOCKED**: C7 goes from BELOW single (9.8, ×0.65) to
  **22.4 Mbit ×1.43** — robust (stdev_s 0.96), and now ABOVE plain reliable C7
  (20.0). This PROVES the bufferbloat serialization was the binding constraint
  on symmetric aggregation, and it is fixed. C7 store=540 (~1 generation) is
  even tighter (22.0, stdev 0.93) — smaller store = less bloat = fewer stalls.
- **DECISIVE C8 (heterogeneous c2+c3): NOT met.** Best ~14.5 (store=960, a
  local peak at exactly 2 generations: 840→9.8, 960→14.5, 1200→11.5, 1440→9.4),
  but HIGH-VARIANCE across batches (8.1 / 9.8 / 14.5; median-completion ~8–10
  Mbit) and never robustly near 15.7. Store tuning, `RWM_CODED_SRC`, and a
  raised proactive `r`=0.30 all fail to lift it. The heterogeneous slow-path
  drag (FEC-layer slow-path coverage / striping, per the systematic-repair
  verdict) is a SEPARATE constraint, independent of the transport store.

### VERDICT
- **Diagnosis CORRECTED and sharpened**: the single-path ~15 Mbit ceiling is a
  per-symbol PROCESSING limit (window/BW/RTT-independent, loss-sensitive), NOT a
  congestion-window that fails to ramp. The store_max = G·(M+1) BUFFERBLOAT
  (RTT 0.5–1.3 s) is what SERIALIZES dual-path aggregation.
- **Fix LANDED (symmetric)**: 2-generation backpressure (`store_max = 2·G`
  default) unlocks symmetric aggregation (C7 9.8 → 22.4, ×1.43) and de-jitters
  single-path (stdev 24.8 → 1.0 s), with no regression (`--lib` 261, `gate_suite`
  15/15, `temporal_oracle` 7/7, math 37/37; the change is scoped to generation
  mode, an opt-in flag).
- **DECISIVE C8 (>15.7): NOT met** — best ~14.5, high-variance. Honest
  FAIL-WITH-MECHANISM: the residual is the heterogeneous slow-path drag, one
  layer above the transport (FEC striping / slow-path coverage), not the store.
  A single generation is 4.6× the BDP, so the ONLY structural escape to
  BDP-sized operation (small G) hits a LATENT generation-decoder frontier-advance
  deadlock (receiver reports full rank / zero deficit yet the in-order frontier
  wedges — traced at G=96); fixing that decoder bug + decoupling the sender's
  retention/backpressure from the in-order frontier (prune on per-generation OOO
  decode) is the identified next step, beyond this pass's safe scope.

**Verification.** `cargo test -p raptorpath --lib` 261 green; `gate_suite` 15/15
release; `temporal_oracle` 7 green; raptorpath-math 37 green. L1:
`RWM_GEN=480 RWM_GEN_R=0.15 bash ~/l1/perf_rwm_c.sh c2 c2 bulk 50000000 6 dual`
(C7, default store=2·G) and `RWM_STORE=` override for the sweep;
`RWM_DIAG=1 bash ~/l1/diag_rwm.sh c2 c2 50000000 single` for the constraint
report.

## C8 Final — small-G frontier-advance DEADLOCK fixed; C8 still transport-ceiling-bound (branch `feat/c8-final`, 2026-07-07)

The Transport Ceiling section above named the last blocker to BDP-scale (small-G)
operation: a generation-decoder **frontier-advance deadlock** traced at G=96, and
a slow-path coverage question. This branch **FIXES the deadlock** — small G now
completes robustly where it previously WEDGED — but the **DECISIVE C8 >15.7 Mbit/s
bar is STILL NOT met (best 15.07 Mbit/s at G=384, aggregation factor 0.98)**.
Honest FAIL-WITH-MECHANISM: with the deadlock gone, the residual is cleanly the
per-connection processing ceiling (systematic-repair extracts ~15 Mbit from ONE
100 Mbit path and a second heterogeneous path adds nothing), NOT the FEC, NOT the
deadlock, NOT a slow-path drag. Number NOT forced.

### The deadlock — ROOT CAUSE and FIX (the structural escape to small G)
ROOT CAUSE (found by static trace + reproduced): in systematic-repair mode the
receiver learned a generation's width K_g **only from a repair header**. A
generation whose ENTIRE `ceil(G·r)` proactive repair budget was lost on the wire
therefore never entered the receiver's deficit map — so the receiver reported
**ZERO deficit** for it while its hole **wedged the in-order frontier forever**,
and the sender (proactive budget spent, no deficit to fund) went idle
(in_flight=0/src=0/cod=0, exactly the measured signature). At **large G** the whole
`ceil(G·r)` budget (72 symbols at G=480) is never fully lost, which is why only
**small G wedged** — and small G is precisely what BDP-scale operation needs.

FIX (`net/mod.rs`, receiver arm, `send_gen_deficits`): **seed the width (= G) of
every PROVABLY-FULL generation from the primary seqs alone** — a generation whose
end lies at/below the highest seq seen certainly has G sources, so its deficit is
computable from the delivered primaries WITHOUT ever seeing a repair for it. This
closes the circular dependency: the receiver now always reports the frontier
generation's true deficit, and the sender's **deficit-recovery loop — which is
ack-clock-INDEPENDENT** — always funds the frontier hole. The fix is generation-
mode-only and adds NO traffic to a healthy flow (a fully-received generation seeds
deficit 0). Regression test `small_g_generation_recovers_from_deficit_when_all_proactive_lost`
(G=96) asserts the invariant end to end: with NO repair seen, `rank_in(anchor,G)`
== G−holes and the generation completes in exactly `holes` coded symbols.

### The deadlock A/B (gold standard, same VM/netem, 50 MB, G=96)
| build | C8 c2+c3 dual, G=96, 50 MB | outcome |
|-------|---------------------------|---------|
| **clean base** (no seeding) | warmup completes, then **NO 50 MB run finishes in 210 s** | **WEDGE** |
| **this fix** (seeding) | **6/6 complete, 13.74 Mbit/s, stdev 1.27 s** | **robust** |

### DECISIVE C8 (c2+c3, systematic-repair, store=2·G, r=0.15, 50 MB ×6, VM)
| G | C8 dual Mbit/s | median s | stdev s | completion | single c2 | agg factor | vs 15.7 |
|--:|---------------:|---------:|--------:|-----------:|----------:|-----------:|:-------:|
| 96  | 13.74 | 30.03 | 1.27 | **6/6** | — | — | ✗ |
| 192 | 14.92 | 26.95 | 1.34 | **6/6** | 15.04 | **0.99** | ✗ |
| 384 | **15.07** | 26.59 | 1.93 | **6/6** | 15.36 | **0.98** | **✗ 0.96×** |

C8 rides at the **full single-path rate** at every G (factor 0.98–0.99), monotone
in G and flattening toward the ~15 Mbit single-connection ceiling — best **15.07 <
15.7**. Completion is now **6/6 at EVERY G including G=96** (the deadlock is gone)
with **low variance** (stdev 1.3–1.9 s), a marked improvement over Transport
Ceiling's high-variance C8 (8.1/9.8/14.5, median-completion ~8–10).

### Controls (no regression)
- **Plain-reliable C7 (c2+c2) = 22.31 Mbit/s** (×1.43, 4 reps, stdev 1.07 s) —
  the symmetric aggregation win is **INTACT** and untouched (the fix is
  generation-mode-only). NOTE: the "C7 ×1.43 = 22.4" win is a **plain-reliable**
  result (its measurement carries NO `--window-systematic-repair`); **systematic-
  repair C7 has NEVER aggregated** (14.5–15.4, ×1.02 — as the systematic-repair
  merge section already reported), so it is the SAME per-connection ceiling, not a
  regression from this change.
- **single c2 systematic-repair = 15.04–15.36** (≥15 parity ✓).
- **6/6 completion, dnf:0** across all arms.

### Slow-path coverage (residual #2) — addressed, not the binding constraint
The deficit-recovery loop runs every iteration (incl. under backpressure) and
places covering repair by the ∝-goodput placement law, which already biases toward
the fast path proportionally. A hard best-path (argmax) concentration was tried and
REVERTED: it changed C8 by nothing measurable and starves a symmetric second path.
With the deadlock gone and the deficit loop always funded, the slow path is not the
long pole — C8 tracks single-path rate, not below it.

### VERDICT
- **Deadlock: FIXED.** Small G completes 6/6 robustly (clean base WEDGES at G=96);
  the receiver-seeding closes the circular width-learning dependency and the
  ack-clock-independent deficit loop funds the frontier. This unlocks BDP-scale
  (small-G) operation with low variance.
- **L1 DECISIVE (>15.7): NOT MET.** C8 best **15.07 Mbit/s** (G=384, 6/6),
  aggregation factor **0.98**. Honest FAIL-WITH-MECHANISM — number NOT forced.
- **Binding residual: the per-connection PROCESSING ceiling** (systematic-repair
  extracts ~15 Mbit from one 100 Mbit path — window/BW/RTT-independent, loss-
  sensitive, documented above — and adding a heterogeneous second path adds
  nothing). This is the transport substrate (~4.5× below native quinn), one layer
  below the FEC and below this deadlock. It is the SAME ceiling that caps single
  systematic-repair; heterogeneous C8 aggregation over the systematic-repair
  transport is blocked by it, not by the FEC design (the plain-reliable path DOES
  aggregate symmetrically to 22.3). Closing it needs a transport-throughput change
  (per-symbol processing / datagram path), beyond this FEC branch's scope.

**Verification.** `cargo test -p raptorpath --lib` 264 green (+1 regression test);
raptorpath-math 11 green; `gate_suite` 15/15 release. L1 (VM, netem independent
qdiscs): `RWM_GEN=<G> RWM_GEN_R=0.15 RWM_EXTRA="--window-systematic-repair" bash
~/l1/perf_rwm_c.sh c2 c3 bulk 50000000 6 dual` (C8 sweep) and `... c2 c2 ... single`
(single control); clean-base G=96 wedge reproduced by rebuilding without the
receiver-seeding fix.

## Per-Symbol Perf — the "processing ceiling" was ONE O(n²); low-loss single-path now 3–5.6× faster; C8 (lossy) still latency-bound (branch `feat/per-symbol-perf`, 2026-07-07)

The three sections above isolated a per-connection **processing ceiling** (~15
Mbit from a 100 Mbit link, ~4.5× below native quinn, framed as "per-symbol
processing, window/BW/RTT-independent"). This branch **PROFILED** that ceiling
(perf, VM AVX2, single-path C2 native-perf, systematic-repair) and found the
premise is **half right and half wrong**:
- **RIGHT:** there WAS a dominant per-symbol CPU cost — a single accidental
  **O(n²)**. Fixing it makes **low-loss single-path 3.0–5.6× faster and it now
  SCALES with bandwidth** (the old "not bandwidth-limited" evidence was an
  artifact of this bug pegging the CPU).
- **WRONG:** the ceiling is NOT a "per-symbol processing" limit in general. At
  the rate it caps, the sender uses only **40% of one core** (20% after the
  fix), the receiver 30% — **neither side is CPU-saturated**. The **lossy** C2 /
  C8 ceiling is a SEPARATE mechanism: loss-recovery in-order-frontier latency,
  which CPU headroom does not touch. **C8 (>15.7) still NOT met.** Number NOT
  forced.

### PROFILE — the top per-symbol costs (perf, single-path C2 systematic-repair)
| rank | cost | self % of sender CPU | what it is |
|-----:|------|---------------------:|------------|
| 1 | `PathState::record_rtt_sample` → `CopaState::record_rtt` | **~42%** | **O(n) full rescan** of the entire 10 s RTT-sample deque to recompute the windowed min, on EVERY ACK. At L1 (~1700–3000 ACK-driven samples/s × 10 s ⇒ **~20–30k-element deque**) this is a hidden **O(n²)** over a transfer — the single largest cost by 4×. |
| 2 | syscall + futex + epoll (`do_syscall_64`, `sendmsg`, `futex_wake`) | ~20% | the per-symbol/per-ACK async round-trip: one QUIC datagram per symbol, one `sendmsg` per ACK, cross-thread wakeups to the quinn driver. Latency, not a CPU wall. |
| 3 | `gf256::simd::mul_acc_ssse3` / `ring` AEAD (gcm+vpaes) | ~10% / ~12% | FEC coded-symbol GF multiply + QUIC crypto — necessary work. |
| 4 | `GenerationEncoder::gen_len` (BTreeMap `range().count()`) | ~5% | O(gen_size) retained-source count, called per coded emission. Identified, NOT fixed (irrelevant to the lossy bottleneck; cacheable later). |

**Decisive framing fact:** sender **40% CPU**, receiver **30%** — the ~15 Mbit
ceiling is reached with **>half a core idle on each side**. It is latency-bound,
not CPU-bound.

### THE FIX — windowed-min via a monotonic deque (O(1) amortised, exact same value)
`net/scheduler/mod.rs`, `CopaState::record_rtt`: replace the O(n) `rtt_samples.iter().min()`
rescan with a **monotonic non-decreasing deque**. A new sample evicts every
pending candidate with RTT ≥ its own (they can never be the window min while a
newer, smaller-or-equal sample is in the window, and it expires strictly later),
so the front is always the current windowed min; time-expiry still pops the
(oldest-timestamp) front. **Byte-identical `min_rtt`** (so congestion control is
unchanged — verified: C7 aggregation intact), O(1) amortised. Sender CPU
**40%→20%**; `record_rtt_sample` **vanishes** from the profile.

### L1 MEASURED — single-path before/after (VM AVX2, netem, 50–60 MB)
| config | before (documented / this branch base) | after | factor |
|--------|---------------------------------------:|------:|-------:|
| **clean 100 Mbit, plain-reliable single** | 28.7 | **86.4** (×5, stdev 0.02 s) | **3.0×** |
| **c1 1 Gbit, plain-reliable single** | 29.7 | **166.3** (×5, stdev 0.17 s) | **5.6×** |
| c2 (lossy) systematic-repair single | 15.0–15.4 | 15.24 (×6) | ~flat |
| c2 (lossy) plain-reliable single | 16.4 | 15.76 (×6) | ~flat |

The low-loss / high-BW regime — where the O(n²) actually pegged the CPU — is
**3–5.6× faster and now tracks link bandwidth** (86 at 100 Mbit clean → 166 at 1
Gbit), directly **overturning** the Transport-Ceiling section's "100 Mbit→1 Gbit:
28.7→29.7, not bandwidth-limited" (that was the RTT quadratic capping CPU, not a
processing ceiling). raptorpath clean-100 Mbit (86.4) now **exceeds** the quoted
native-quinn-at-C2 rate (72). The **lossy** c2 rate is unchanged because it is
gated by loss recovery, not CPU.

### DECISIVE C8 (c2+c3, systematic-repair, store=2·G, r=0.15, 50 MB ×6, VM)
| G | C8 dual Mbit/s | stdev s | completion | single c2 | agg factor | vs 15.7 |
|--:|---------------:|--------:|-----------:|----------:|-----------:|:-------:|
| 192 | 15.03 | 1.00 | **6/6** | 15.24 | **0.99** | ✗ |
| 384 | 14.91 | 2.03 | **6/6** | 15.24 | **0.98** | **✗ 0.95×** |

C8 rides at the single-path rate (factor 0.98–0.99), **unchanged** by the CPU fix
— exactly as expected, since both c2 and c3 are lossy and the binding constraint
is loss-recovery latency, not processing. **>15.7 NOT met.**

### Controls (no regression)
- **C7 plain-reliable c2+c2 = 21.88 Mbit/s** (×1.39 over single plain c2 = 15.76;
  stdev 0.85 s, 6/6) — **symmetric aggregation INTACT** (the fix keeps `min_rtt`
  byte-identical, so CC is unchanged). One earlier batch read 18.4 (high-variance
  lossy dual); the tight rerun confirms 21.9.
- `cargo test -p raptorpath --lib` **262 green, 0 failed** (scheduler RTT tests
  included); `gate_suite` **15/15 release**; raptorpath-math **11 green**.
- **6/6 completion, dnf:0** across every arm.

### VERDICT
- **The "processing ceiling" was a single O(n²)** — profiled, fixed, MEASURED.
  Low-loss single-path is **3.0–5.6× faster** and **scales with bandwidth**; the
  Transport-Ceiling "not bandwidth-limited / per-symbol processing" claim is
  **overturned for the low-loss regime**. Sender CPU halved (40%→20%).
- **The DECISIVE C8 heterogeneous bar (>15.7): STILL NOT MET** (best 15.03,
  factor 0.99). Honest FAIL-WITH-MECHANISM: the c2/c3 ceiling is **not** CPU
  (both sides <30% core) — it is **loss-recovery in-order-frontier latency**, a
  layer above the transport CPU path this branch fixed. A 1.3% GE loss collapses
  single-path from 86 (clean) to 15 (c2): a 5.6× loss-recovery penalty that the
  CPU fix does not touch and that a second lossy path cannot parallelise. Closing
  it is a loss-recovery-latency problem (the FEC/reliability design already
  explored in the three sections above), NOT a per-symbol-processing one.
- **Net:** a large, correct, low-risk transport-CPU win that lands the
  low-loss/high-BW throughput but not the specific lossy-heterogeneous C8
  aggregation bar, because that bar is latency-bound.

**Verification.** perf (`perf record -F 999 -g` on the sender/receiver PIDs),
per-process CPU via `/proc/<pid>/stat`. L1 (VM, netem independent qdiscs):
`RWM_GEN=<G> RWM_GEN_R=0.15 RWM_EXTRA="--window-systematic-repair" bash
~/l1/meas_rwm.sh c2 c3 50000000 6 dual` (C8), `... c2 c2 ... single` (single
control), `RWM_GEN=none ... meas_rwm.sh clean clean … single` and `… c1 c1 …`
(low-loss single before/after), `RWM_GEN=none … c2 c2 … dual` (C7 control).
Profiling harness: `~/l1/prof_single.sh`.

## Loss-Recovery — the C2 single-path collapse DIAGNOSED (branch `feat/loss-recovery`, 2026-07-07)

**Finding under test.** After the O(n²) CPU fix, plain-reliable single-path flies on
a CLEAN 100 Mbit link (76 Mbit/s) but COLLAPSES to ~14 Mbit under C2's ~2.5 % bursty
GE loss — a 5.5× drop. Hypothesis in the brief: this is loss-triggered cwnd reduction,
which would VIOLATE the §12 loss-blind (delay-only) CC claim.

Reproduced (this binary, `perf_rwm_c.sh … single`, 1.8 MB × 5, seed 42):
- CLEAN single: **76.8 Mbit/s** (median 0.182 s).
- C2 single (plain reliable): **13.7 Mbit/s** baseline → the 5.5× collapse. CONFIRMED.

### The hypothesis is REFUTED — the CC IS loss-blind, §12 HOLDS in code
cwnd traces (`RWM_DIAG=1`, per-250 ms) under C2 show cwnd is NOT collapsed — it GROWS:
plain reliable cwnd 29→628, systematic-repair cwnd 254→**3390**. There is no
loss-triggered reduction on the hot path: `PathState::on_loss(fec_recovered)` returns
early whenever FEC recovered the loss and only touches cwnd on an actual decode
FAILURE (net/mod.rs BlockResult). The large cwnd is RTT-INFLATED, not suppressed.
**The Copa-lite loss-blind claim (paper §12.1/§12.4) is correct end-to-end; it is NOT
the cause.** (Record corrected: the brief's prime suspect does not hold.)

### Actual mechanism — two coupled defects, neither is the CC
1. **Bufferbloat (the reliable window sender BYPASSES the delay-based CC).** TUN-read
   backpressure gates on `sent_store.len()` capped at the fixed `RELIABLE_STORE_MAX =
   1024` ≈ **12× the C2 BDP (~83 sym)**. Nothing bounds the standing queue to the pipe.
   MEASURED RTT inflates to **410–518 ms** (plain) / **236–660 ms** (systematic) vs the
   10 ms base — 40–66×. This is the same class of bug the generation path already fixed
   (store = 2·G); the plain-reliable `else` branch was left at fixed 1024.
2. **In-order cumulative-ack frontier serialization (the throughput cap).** Both
   completion and the store-drain gate key on the CONTIGUOUS frontier `window_ack_seq`,
   which FREEZES on every hole; recovery is reactive (gap-ack → retransmit, ~1
   recovery-round/RTT). The frontier advances at ≈ window/RTT, and because bufferbloat
   makes RTT scale WITH the window, goodput ≈ window/RTT is roughly constant — a
   **window-INDEPENDENT ~16 Mbit ceiling**.

Evidence the ceiling is frontier-bound, not queue-bound (C2 single, `diag_plain.sh`):
- Throughput ≈ 16.5 Mbit INVARIANT across `store ∈ {96,160,256,512,1024}` and
  `infl_cap ∈ {100,160}`. `store=96` drops RTT to ~40 ms but goodput stays 16.6.
- `--window-out-of-order` (decode-on-total): NO improvement (13.6) — because
  `window_ack_seq` is still the CONTIGUOUS frontier over `received_seqs`; OOO changes
  app-delivery order, not the ack frontier that gates the sender.
- Proactive repair `RWM_MIN_R ∈ {0.15,0.30}`: WORSE (12.5 / 10.3) — repair eats goodput
  without pre-covering frontier holes.
- It is NOT a "per-symbol processing ceiling": the SAME code path does 76 Mbit on a
  clean link. The ceiling is loss-specific — recovery latency at the frontier.

### Fix shipped — bound the reliable window to the delay-based BDP (§12)
`PathState::copa_bdp_anchor()` (existing accessor) exposes BtlBw×RTprop (bufferbloat-
robust: windowed-max rate × min-RTT floor). The plain-reliable sender now caps the
OUTSTANDING store at
`gain × BDP` (default gain 2.0; bootstrap 128 until the anchor warms; `RELIABLE_STORE_MAX`
kept as the memory ceiling). `RWM_STORE` forces a static window; `RWM_STORE_GAIN` /
`RWM_STORE_BOOT` tune. Effect: RTT **410 → 40–120 ms** (bufferbloat removed), clean
single **unchanged 76.8** (no regression), C2 single **13.7 → 15.7 Mbit** (+15 %, modest).
It fixes the queue/latency, NOT the throughput collapse — which is frontier-bound.

### C8 (the aggregation bar) — still NOT met, same root
`perf_rwm_c.sh … dual`, 1.8 MB × 6, seed 42:
- C7 (c2+c2 symmetric) plain: **19.0 Mbit** = **1.21×** single (mild aggregation).
- C8 (c2+c3 heterogeneous) plain: **13.3 Mbit** = **0.85×** single (15.7) — below bar.
- C8 systematic-repair: **13.0 Mbit** = **0.89×** single (14.6) — below bar.

C8 anti-aggregates because the fast path stalls on the SLOW path's in-order frontier
holes — the cross-path form of defect (2). The 15.7 Mbit bar with factor > 1 is NOT met.

### HONEST VERDICT
The collapse is **not the congestion control** (loss-blind holds; §12 correct). It is
(a) a real bufferbloat defect in the reliable window sender bypassing the delay-based CC
— **FIXED** (RTT 410→~40 ms, no regression); and (b) a recovery-latency limit at the
in-order cumulative-ack frontier under bursty loss that is largely **fundamental to the
bulk / pure-ARQ (r*→0) design** — NOT reducible by the CC, by out-of-order delivery, by
proactive repair, or by any window/in-flight cap tested. Closing (b) needs PIPELINED
frontier recovery (recover all in-window holes per RTT) or a genuinely rateless frontier
so a hole is never a fixed in-order position — a transport-pipeline change beyond this
branch. C8 aggregation stays blocked by (b)'s cross-path form.

**Controls.** clean single 76.8 (no regression); all runs dnf:0; `cargo test -p
raptorpath --lib` 262 green; `raptorpath-math` all green; `gate_suite` 15/15 release.
**Harness.** `~/l1/perf_rwm_c.sh` (default = plain reliable; `RWM_EXTRA=
"--window-systematic-repair"` for systematic), `~/l1/diag_plain.sh` (RWM_DIAG constraint
report; RWM_STORE / RWM_STORE_GAIN / RWM_STORE_BOOT / RWM_INFL_CAP / RWM_MIN_R).

## SACK Flow Control — the in-order-frontier decoupling TESTED, negative result (branch `feat/sack-flow-control`, 2026-07-07)

**Hypothesis under test.** The prior Loss-Recovery diagnosis pinned the C2 collapse on
the sender's flow control being gated by the IN-ORDER cumulative-ack frontier
(`window_ack_seq`): the sent-store drains, and TUN-read backpressure gates, only on the
CONTIGUOUS frontier, which freezes on every hole → the send window can't stay BDP-full →
goodput ≈ window/RTT (~16 Mbit) instead of BDP/RTT. Proposed fix: make the sender's flow
control SACK-based (selective) — prune the sent-store for ANY out-of-order-received
(SACKed) symbol, so `sent_store.len()` tracks TRUE outstanding-unacked and the send window
stays full across holes, with holes recovered in the background by the existing per-seq
NACK/tail-sweep ARQ.

**Implemented.** A SACK channel forwards the receiver's RECEIVED-above-frontier ranges
(the `received_sack_ranges` the P10b SACK machinery already computes) to the plain-reliable
window sender, which drains them non-blocking at the top of the send loop and prunes the
sent-store + per-seq ARQ maps (`retransmit_buffer`, `source_path_map`, `nack_retx_at`) for
each received seq. The hole itself (never in a received range) stays retained and recovers
via the orthogonal NACK path. Gated behind `RWM_SACK_PRUNE` (see below); unit test
`test_sack_pruning_advances_sender_past_a_hole` asserts a single hole leaves only the hole
retained (outstanding 90→1) while the frozen contiguous frontier would pin 90.

### The fix does NOT lift lossy throughput — and is UNSAFE for in-order delivery
Matched in-session A/B, base `ae7f3a8` vs branch, 1.8 MB × 6, seed 42, `perf_rwm_c.sh`:

| arm | BEFORE (base) | `RWM_SACK_PRUNE=1` | DEFAULT (gate off) |
|---|---|---|---|
| c2 single plain (Mbit) | 16.07 | **16.09 (×1.00)** | 16.54 |
| c2 single OOO | — | 14.73 | 15.95 |
| clean single (control) | 77.49 | 76.45 | 77.36 |
| C7 c2+c2 dual plain | 20.15 (×1.25) | **DNF (stall)** | 20.98 (×1.27) |
| C8 c2+c3 dual plain | 12.30 | **DNF (stall)** | 14.91 |
| C8 dual OOO | — | 10.16 | 11.85 |
| C8 dual systematic G384 | 11.15 | 12.99 | 13.28 |

Two findings, both decisive:

1. **No throughput lift.** SACK-decoupling the sender leaves c2 single-path EXACTLY at the
   ~16 Mbit ceiling (16.09 vs 16.07). This CONFIRMS the prior diagnosis that the limiter is
   NOT the sender's store backpressure but the receiver-side in-order RECOVERY LATENCY: the
   sender was never the true bottleneck (throughput was already store-cap-invariant), so
   letting it inject further ahead buys nothing — completion still waits for the in-order
   frontier to walk each hole at ~1 recovery-round/RTT. OOO completion (`RWM_OOO=1`) does not
   help either (14.73), same reason.

2. **It BREAKS in-order reliability.** With the sender no longer held near the frontier it
   races the whole object ahead, but the receiver's in-order reassembly window is BOUNDED
   (`MAX_WINDOW_SIZE`). A symbol can be received (→ SACKed → pruned at the sender) and then
   EVICTED at the receiver before the in-order frontier consumes it — destroying the ONLY
   retained copy, so its seq can never be recovered and completion wedges. MEASURED: C7 and
   C8 in-order dual **DNF** under `RWM_SACK_PRUNE=1`, while the OOO-completion arms (not
   frontier-bound at the receiver) complete. The frontier-coupled backpressure the fix
   removes is precisely what keeps the send frontier inside the receiver's reassembly
   window — it is load-bearing for reliability, not just a throughput artifact.

### HONEST VERDICT
SACK-based sender flow control is **not** the fix. The freeze is on the RECEIVER side
(bounded in-order reassembly + reactive per-hole recovery latency), not the sender's store.
Decoupling the sender either (a) changes nothing (the sender was not the bottleneck) or
(b) violates reliability for in-order delivery (receiver-window eviction of a pruned-but-
un-consumed symbol). This SHARPENS the prior diagnosis: closing the C2 collapse needs
PIPELINED receiver-side frontier recovery (recover all in-window holes per RTT) or a
genuinely rateless frontier where a hole is never a fixed in-order position AND an
unbounded/rateless receiver reassembly so no received symbol is ever evicted before use — a
receiver-pipeline change, unchanged by any sender-side flow-control law. The generation /
systematic-repair modes already avoid the fixed-position hole; their ceiling is decode/
recovery latency, not this.

**Shipped state.** The SACK mechanism is committed but **gated OFF** by `RWM_SACK_PRUNE`
(unset = default): with the gate off the code path is byte-for-byte base — DEFAULT column
above reproduces base with **0 DNF** across all 7 arms (c2 single 16.54, clean 77.36, C7
×1.27, C8 plain/OOO/systematic all complete). The flag exists only to reproduce the negative
result. **>15.7 with factor > 1 NOT met; C2 collapse NOT closed.**

**Controls.** clean single 77.4 (no regression); DEFAULT all 6/6 dnf:0; `cargo test -p
raptorpath --lib` 265 green (adds the SACK-pruning test); `raptorpath-math` all green;
`gate_suite` 15/15 release. **Harness.** `~/l1/sack_meas.sh <label> <reps>` (the A/B
battery); `RWM_SACK_PRUNE=1` enables the experiment.

## Proactive Frontier — proactive FEC decode at the in-order frontier TESTED, REFUTED (branch `feat/proactive-frontier`, 2026-07-07)

**Hypothesis under test (the core value prop).** raptorpath's premise (§5–8): proactive
FEC recovers holes WITHOUT a round-trip. The prior Loss-Recovery + SACK diagnosis pinned
the C2 collapse (clean 76 → c2 ~16, 5.5×) on receiver-side in-order-frontier recovery
latency: `window_ack_seq` freezes on every hole, recovery is a reactive ~1-RTT ARQ round,
so goodput ≈ window/RTT. The hypothesis: with ~2.6 % loss the receiver has FAR more than
enough in-flight proactive repair to decode any frontier hole THE INSTANT it appears — the
frontier should advance at the DECODE rate, never stalling for a NACK RTT. If the receiver
were NACK-and-waiting while decodable repair sat in its buffer, THAT would be the bug.

### PART 1 — DIAGNOSIS (receiver frontier instrumented, `RWM_FDIAG`)
Added a receiver probe (`WindowDecoder::frontier_probe` → `(holes, buffered_equations)`
over `[frontier, highest_seen]`) and a per-hole recovery classifier (DECODE = a repair
solved it, no round-trip; SOURCE = a retransmitted source arrived, a ~1-RTT ARQ round).
c2 single, native `perf`, 1.8 MB, seed 42:

- **DEFAULT (Bulk, r*→0 pure-ARQ):** `repairs_fed` reaches only ~7 over a whole 1.8 MB
  transfer — proactive FEC is essentially OFF (§14.26 glide: mid-stream χ=0 ⇒ r*=0). Of
  ~72 frontier holes, **71 resolve by SOURCE retransmit** (avg ~11–19 ms ≈ 1 RTT), **1 by
  decode**. `probe_buffered = 0` at every stall. **VERDICT: the repair is genuinely ABSENT
  at the frontier, NOT present-but-unused.** The "receiver NACK-and-waiting on decodable
  buffered repair" bug does not exist — there is nothing buffered to decode.
- **Prior RWM_MIN_R=0.15 (leading-window proactive repair):** `repairs_fed=576`, and holes
  DO now decode (DECODE 14 > SOURCE 7) — but each decode takes **~19–32 ms, LONGER than the
  ARQ round it replaces**, because a leading-window RLC repair entangles the frontier hole
  with not-yet-received in-flight symbols: it lands as a multi-unknown GE pivot that cannot
  resolve until the window TAIL arrives ~1 RTT later. Throughput fell to 10.8 Mbit. This is
  WHY the earlier RWM_MIN_R arm regressed.

### PART 2 — FIX BUILT: pre-positioned TRAILING-window frontier repair
Implemented proactive repair coded over a small window that TRAILS the send frontier by a
fixed offset (`build_frontier_repair` from the retain-until-acked `sent_store`, wire-identical
to a normal RLC repair; `WindowEncoder::generate_repair_range` is the encoder-window analogue,
unit-tested). Intent: cover a symbol WHILE it is fresh but whose window members are all
already received by decode time, so a hole solves the instant a covering repair lands — no
round-trip, no future-symbol entanglement. Knobs `RWM_FRONTIER` (width, default 32),
`RWM_FRONTIER_OFFSET` (default 8), `RWM_FRONTIER_GAIN`/`RWM_FRONTIER_R` (rate = gain·ε̂, or
forced). The **isolated mechanism is CORRECT** (unit test
`test_frontier_range_repair_decodes_hole_no_retransmit`: one trailing-window repair decodes a
mid-stream hole from received neighbours, no retransmit).

### DECISIVE L1 MEASUREMENT — the fix does NOT lift throughput (c2 single, native perf, seed 42)
| arm | 5-run mean Mbit |
|---|---|
| baseline (frontier OFF) | **15.0** |
| ack-anchored r=0.10 | 12.1 (rf=718, **ru=4** — repairs redundant: arrive AFTER ARQ) |
| trailing r=0.10/W32/off8 | 11.8 (rf=744, ru=20, but `present_at_stall`=0) |
| trailing r=0.25/W24/off4 | 9.9 |
| trailing r=0.15/W48/off6 | 11.4 |

**Every proactive configuration REGRESSES; more proactive repair ⇒ worse.** The
instrumentation shows why, decisively:

1. **A pre-position-vs-isolate catch-22.** To arrive BEFORE the receiver's frontier reaches
   a hole (≈½ RTT after the symbol is sent) a repair must code FRESH symbols — whose
   neighbours are still in flight, so it can't isolate the hole. To ISOLATE the hole
   (neighbours already received) it must code OLD symbols — so it arrives AFTER the hole has
   already stuck and the ARQ retransmit is already in flight. `present_at_stall=0` in every
   run: a covering equation is NEVER buffered at the instant a hole sticks.
2. **Decode latency > ARQ at small RTT.** When a trailing repair does decode, it sits as a
   multi-unknown GE pivot accumulating rank (bursty loss ⇒ holes>equations in-window,
   `probe_holes` 7–35 vs `probe_buffered` 2–7); MEASURED decode ~25–67 ms vs the ~13–16 ms
   ARQ retransmit. At C2's ~13 ms RTT, reactive ARQ is simply faster than sliding-window RLC.
3. **Displacement.** The repair bandwidth competes for the pacing/cwnd budget with BOTH new
   source AND the ARQ retransmits that actually clear the stuck oldest-hole frontier — so it
   slows the very mechanism that advances the frontier.

C8 (c2+c3 dual) with the fix on: 11.7 Mbit — within the (large) C8 variance of the ~9–15
baseline, **not** a lift, still far below the >15.68 factor>1 bar.

### HONEST VERDICT — the FEC value prop is UNREALIZABLE at this frontier via sliding-window RLC
The prior diagnosis is CONFIRMED and SHARPENED, not overturned. The repair is genuinely
ABSENT in the pure-ARQ default (not a present-but-unused receiver bug); and when MADE present
(`repairs_fed` up to ~750) it does not help, for a now-instrumented structural reason:
proactive sliding-window RLC frontier repair faces a pre-position-vs-isolate catch-22 and, at
C2's small RTT, decodes SLOWER than the ARQ retransmit it would replace while displacing it.
This deepens the Loss-Recovery verdict ("largely fundamental to the bulk / pure-ARQ design")
with the receiver-side decode-latency evidence it previously lacked. The generation /
systematic-repair modes sidestep the fixed-position hole entirely (fungible cross-path
recovery, no in-order frontier) — that, not plain-reliable frontier repair, is where the
proactive-FEC premise actually lives at L1.

**Shipped state.** The frontier-repair machinery + `RWM_FDIAG` instrumentation are committed
but **gated OFF** (`frontier_experiment` requires `RWM_FRONTIER`/`RWM_FRONTIER_R`); with no
env the sender/receiver hot paths are byte-for-byte the baseline. Flags exist only to
reproduce the diagnosis and negative result. **>15.68 with factor>1 NOT met; C2 collapse NOT
closed.**

**Controls (default, gated off).** clean single **76.9** (no regression); c2 single 14.6;
C7 c2+c2 18.5 (**×1.27**); C8 c2+c3 0 DNF; ALL arms dnf:0 (reliability intact — every 1.8 MB
object reassembles fully). `cargo test -p raptorpath --lib` 267 green (adds two frontier-repair
tests); `raptorpath-math` all green; `gate_suite` **15/15** release.
**Harness.** `~/l1/perf_rwm_c.sh <A> <B> bulk <bytes> <runs> single|dual`;
`RWM_FDIAG=1` prints the receiver frontier diagnosis to the server log;
`RWM_FRONTIER_R=<r>` / `RWM_FRONTIER=<w>` / `RWM_FRONTIER_OFFSET=<n>` enable the experiment.

## FEC-vs-ARQ Crossover — the RTT sweep that settles the over-claim (branch `feat/fec-arq-crossover`, 2026-07-08)

**The over-claim under challenge.** The Proactive-Frontier verdict above (and §8/§14.7
of the paper) was read as "ARQ beats FEC / the ~16 Mbit lossy cap is FUNDAMENTAL." That
was generalized from ONE regime (C2, ~10 ms RTT) with the stated mechanism "MEASURED FEC
decode ~25-67 ms > the ~13 ms ARQ retransmit." Two hypotheses were put under test:
**H1 (scenario):** at HIGHER RTT the ARQ round (~1.3-1.5·RTT) should EXCEED the FEC decode,
so proactive frontier repair should BEAT pure-ARQ above some crossover RTT.
**H2 (error):** the ~25-67 ms is suspiciously high — the dense GF(256) decoder runs a symbol
in microseconds — so it is not raw decode but either the slow SPARSE decoder on the frontier
path (a) or symbol-arrival WAITING to isolate the hole (b, entanglement), either of which
would drop the crossover.

### H2 — the "decode latency" is WAITING, not compute (measurement corrected)
Added a receiver probe (`RWM_FDIAG`, net/mod.rs) that times the RAW `win_dec.add_symbol()`
GF(256) call separately from the per-hole RESOLUTION wall-time the prior branch reported.
The prior "DECODE avg" spanned hole-armed → frontier-passes and thus **included the wait for
enough rank to isolate the hole**; it was never decode compute. Measured (native perf,
1.8 MB, seed 42):

- **Raw decode compute = 6-10 µs/call, 33-54 ms TOTAL over the WHOLE 1.8 MB transfer**
  (`COMPUTE calls≈5300 avg=10us total=54ms`). Compute is < 1 % of a 5-11 s transfer.
- The sparse `RlcWindowDecoder` IS the decoder on the plain-reliable frontier path
  (net/mod.rs `create_window_decoder`, generation-off arm) — **H2(a) is a true fact but a
  false cause**: at 10 µs/call it is not the bottleneck, so routing frontier decode through
  the dense `GenerationDecoder` (generation-aligned; not even a drop-in for arbitrary
  sliding windows) would change nothing. No hot-path decoder swap is warranted.
- The real per-hole latency is **WAITING for rank** — **H2(b) confirmed**. At the moment a
  hole sticks, `present_at_stall = 0` in EVERY run: a covering equation is never buffered
  (the pre-position-vs-isolate catch-22). Under bursty loss the recent window holds MORE
  holes than buffered equations (`probe_holes 19` vs `probe_buffered 4`), so a covering
  repair lands as a multi-unknown pivot and cannot isolate until neighbours arrive.

So the prior "decode ~25-67 ms > ARQ" was **doubly wrong**: (a) it was resolution wall-time,
not compute; and (b) — see H1 below — at high RTT a decode-resolved hole is FASTER than ARQ,
not slower.

### H1 — the RTT sweep: NO crossover (single-path, 100 mbit, GE 1.3/50 ≈ 2.5 % loss, jitter=0, seed 42, 1.8 MB × 5)
Pure-ARQ (default) vs proactive frontier-FEC (`RWM_FRONTIER=32 RWM_FRONTIER_R=0.10`),
RTT swept via netem one-way delay (cells `c2r10…c2r200` in `lib.sh`):

| RTT (ms) | ARQ (Mbit/s) | FEC W32/r0.10 (Mbit/s) | FEC/ARQ |
|---|---|---|---|
| 10  | 20.32 | 14.94 | 0.74 |
| 30  |  9.94 |  6.51 | 0.65 |
| 50  |  6.81 |  4.19 | 0.61 |
| 100 |  3.22 |  2.01 | 0.62 |
| 200 |  1.62 |  1.21 | 0.75 |

**FEC never beats ARQ. The ratio is ~0.61-0.75, FLAT across a 20× RTT range — it does not
narrow toward 1.0 at high RTT.** Throughput ∝ 1/RTT for BOTH arms (window/RTT frontier
serialization; the window is not BDP-scaled at high RTT), and frontier-FEC is a roughly
constant ~35 % throughput TAX on top. Config robustness at RTT=200 (the most FEC-favorable
point, 5 reps each): ARQ 1.77; FEC r0.05/W16/off2 **1.36**, r0.05/W48/off8 **1.38**,
r0.10/W16/off2 1.09, r0.15/W64/off2 0.83, r0.20/W32/off4 0.79. **Best FEC = 0.78× ARQ;
higher r monotonically worse. No W/offset/r combination crosses.**

### Why FEC loses even though its decode IS faster at high RTT
The RTT=200 FDIAG is decisive: **DECODE-resolved holes take 8.5 ms vs the ARQ SOURCE round
279 ms (≈ 1.4·RTT) — FEC decode is 33× FASTER, exactly as H1 predicted.** But it never
converts to throughput because of the catch-22, now fully instrumented:
1. **`present_at_stall = 0`:** only **3 of 86** frontier holes actually decode; the other 83
   still resolve by the slow ARQ round. The fast path fires on ~3 % of holes.
2. **97 % of repair is wasted** (`rf=486 ru=16`): the covering repair almost always arrives
   AFTER the ARQ retransmit already cleared the hole, so it is pure displacement — it
   competes for the shared cwnd/pacing budget with the new source AND the ARQ retransmits
   that actually advance the window/RTT-limited frontier, slowing them.

### HONEST VERDICT — the throughput claim SUPPORTED, the stated mechanism REFUTED and corrected
- **Is "ARQ beats FEC on throughput" over-claimed? NO — it holds across a 20× RTT sweep
  (10-200 ms) with real multi-point evidence, not one data point. There is NO crossover, and
  no crossover appears under 6 tuned FEC configs at the most-favorable RTT.** In THAT narrow
  sense the "fundamental" reading is EMPIRICALLY SUPPORTED for plain-reliable frontier-FEC.
- **But the STATED MECHANISM was wrong and is corrected:** (a) decode COMPUTE is 10 µs, not
  25-67 ms; (b) at high RTT a decode-resolved hole (8.5 ms) is 33× FASTER than the ARQ round
  (279 ms). FEC loses NOT because "decode is slow at low RTT" but because the sliding-window
  **pre-position-vs-isolate catch-22** makes the fast decode fire on only ~3 % of holes while
  its 97 %-wasted repair displaces the frontier-advancing traffic — structural to proactive
  sliding-window RLC over an in-order frontier, confirmed across the whole RTT range.
- **Paper record (§14.7 / §8):** the analytical rule "FEC wins when t_fec(W) < 1.5·RTT" is
  CONTRADICTED at L1 for frontier repair — it assumes the covering repair is present and
  isolating at decode time (true for a stable/systematic window, i.e. generation mode) and
  ignores the shared-budget displacement. Corrected in §8.9 / §14.7 with the sweep.
- The proactive-FEC premise lives in the generation/systematic modes (fungible cross-path
  recovery, no fixed-position frontier hole), not plain-reliable frontier repair — consistent
  with, and now mechanistically explained by, the prior verdict.

**Shipped state.** Frontier-repair machinery unchanged and still gated OFF
(`RWM_FRONTIER`/`RWM_FRONTIER_R`). Added only the `RWM_FDIAG` COMPUTE-time probe (gated on
`RWM_FDIAG`; default hot path is a plain `add_symbol` call). No production hot-path behaviour
change — the "decode-latency error" was a measurement mislabel, not a code inefficiency, so
there was nothing to fix in the transport.
**Controls.** clean single **76.3 Mbit/s** (no regression vs ~76.9 baseline);
`cargo test -p raptorpath --lib` 267 green; `raptorpath-math` all green;
`gate_suite` **15/15** release; ALL sweep/tune arms dnf:0 (reliability intact).
**Harness.** `~/l1/rtt_sweep.sh <reps> <bytes>` (the ARQ-vs-FEC RTT curve);
`~/l1/fec_tune.sh <reps> <scen>` (config robustness); cells `c2r10…c2r200` in `lib.sh`
(c2 loss/bw, jitter=0, one_way=RTT/2); `RWM_FDIAG=1` now also reports `COMPUTE calls/avg/total`.

## Proactive FEC vs ARQ (high RTT) — the FUNGIBLE-mode RTT sweep (branch `feat/proactive-fec-highrtt`, 2026-07-08)

The FEC-vs-ARQ Crossover section above tested the WRONG realization for the
crossover hypothesis (plain-reliable in-order FRONTIER repair, which fires on only
~3 % of holes — the pre-position-vs-isolate catch-22) and found no crossover. It
explicitly deferred the premise to "the generation/systematic modes (fungible
cross-path recovery, no fixed-position frontier hole)". THIS rung tests exactly
that: **fungible PROACTIVE systematic FEC** (systematic source on the wire +
windowed generation repair provisioned upfront + out-of-order object completion)
vs pure-ARQ, swept across RTT, with the hypothesis that at HIGH RTT proactive FEC
(≈ propagation + K(1+r)/rate, no per-loss round-trips) should BEAT ARQ (~1.5·RTT
per loss-episode). **Result: the hypothesis is REFUTED — there is no high-RTT
crossover, and the advantage runs the OTHER way (FEC loses MORE as RTT grows).**
Honest FAIL-WITH-MECHANISM, fully instrumented; the win was actively searched for
across ~12 configs and not forced.

### Instrumentation added (proves proactive vs reactive; both env-gated, default-off)
- **`RWM_PFRAC`** (`run_window_sender`): counts coded repair emitted PROACTIVELY
  (the open-loop per-generation provisioning round-robin, `generate_repair` — NO
  round-trip) vs REACTIVELY (the deficit-driven recovery loop, `generate_repair_for`,
  which fires only after a receiver `GenerationDeficit` — one round-trip). Prints
  `proactive_coded / recovery_coded / proactive_fraction`. This is the direct
  measure of whether Mode B genuinely recovers holes from upfront repair.
- **`RWM_NO_REACTIVE`** (`run_window_sender`): disables the deficit-driven reactive
  loop entirely — the PURE-PROACTIVE demonstrator (systematic source + fixed upfront
  r, out-of-order, zero round-trips). No production behaviour change (default off).

### The RTT sweep — Mode A pure-ARQ vs Mode B fungible proactive FEC
Single path, 100 mbit, GE 1.3/50 ≈ 2.6 % loss, jitter=0 (RTT the only variable),
1.8 MB × reps. Mode B = `--window-systematic-repair` + `--window-out-of-order`,
G=768, r=0.20, store=2·G, dnf:0 on BOTH arms at every point.

| RTT (ms) | ARQ (Mbit/s) | proactive-FEC (Mbit/s) | FEC/ARQ | proactive fraction |
|---:|---:|---:|---:|---:|
| 10  | 19.3 / 22.3 | 28.3 / 17.8 | 1.47 / 0.80 | 0.95 / 0.92 |
| 30  | 7.9 | 7.0 | 0.89 | 0.70 |
| 50  | 7.4 | 5.7 | 0.78 | 0.88 |
| 100 | 4.0 | 3.1 | 0.77 | 0.83 |
| 200 | 1.9 | 1.1 | **0.55** | **0.23** |

(RTT10 shown for two independent batches — the 1.8 MB object completes in <1 s
there, so low-RTT throughput is warmup/ramp-dominated NOISE: FEC×1.47 in one batch,
×0.80 in the other → a TIE at low RTT, not a win.) **The robust signal is the
high-RTT trend: FEC/ARQ falls MONOTONICALLY with RTT (→ 0.55 at RTT200), the
opposite of the hypothesis, and the proactive fraction COLLAPSES from 0.95 to
0.23.** The reactive coded count explodes with RTT (rcod 63 → 160 → 255 → 4078)
while the proactive budget is constant (pcod ≈ 1214): at high RTT recovery becomes
REACTIVE-dominated and round-trip/overrun-bound — exactly ARQ's failure mode plus
coding overhead.

### Mechanism — why fungible proactive FEC LOSES at high RTT (four measured causes)
1. **Proactive-fraction collapse via burst-overrun.** A larger RTT ⇒ larger BDP ⇒
   the systematic source + coded ride the DROPPABLE QUIC-datagram path in bigger
   bursts, which netem drops. Per-generation loss then exceeds the fixed `ceil(len·r)`
   proactive budget, so generations arrive SHORT and the receiver reports deficits.
   Proactive fraction 0.95 (RTT10) → 0.23 (RTT200).
2. **Reactive runaway.** Deficit-driven recovery is EXEMPT from the in-flight
   congestion cap (by design, so it can always fund a frontier hole) and paced at
   ~86 Mbit; at RTT200 that overruns the 100 mbit link already carrying source, its
   own recovery symbols drop, the stale (~RTT-old) deficit persists, and the sender
   re-floods — MEASURED recovery_coded up to **252 k symbols for a 5 k-symbol object**
   (30–120× the object). This collapses throughput below ARQ.
3. **Pure-proactive (RWM_NO_REACTIVE) is genuinely proactive but DNFs.** With the
   reactive loop OFF the trace confirms `proactive_fraction = 1.0000, recovery_coded
   = 0` (zero round-trips — the clean demonstrator the directive asked for), but the
   object **never completes**: the coupon-collector tail always leaves SOME generation
   a few DoF short of its fixed upfront budget, and with no recovery that generation
   wedges the in-order frontier forever (ack stuck; MEASURED at RTT200, both 1.8 MB
   and 6 MB). Open-loop FEC cannot guarantee reliable delivery — it structurally
   needs feedback for the last ε, and that feedback is the round-trip that erases the
   high-RTT advantage.
4. **In-order-coupled sender flow control.** Generation-mode retention/backpressure
   prunes on the CUMULATIVE (in-order) decode ack (`encoder.advance(ack+1)`), so even
   with out-of-order DELIVERY a single hole stalls the sender's window for the whole
   recovery latency — reproducing ARQ's ∝1/RTT serialization. Decoupling it (advance
   retention on out-of-order generation completion) is the documented "next step,
   beyond scope" from the Transport-Ceiling / C8-Final sections; it is the same
   binding constraint, here shown to also block the proactive-FEC-vs-ARQ crossover.

### The per-hole physics ARE favorable — but do not convert to throughput
Confirmed (prior + this branch): a decode-resolved hole at RTT200 recovers ~8.5 ms
vs ARQ's ~279 ms — FEC is 33× FASTER PER HOLE. But sustaining a BDP-scaled pipe on
the droppable-datagram substrate at high RTT is impossible without EITHER
burst-overrun (→ reactive round-trips, cause 1+2) OR wedging (pure-proactive DNF,
cause 3), and the sender window stalls in-order regardless (cause 4). So the 33×
per-hole win never becomes a throughput win on a reliable bulk transfer at high RTT.

### HONEST VERDICT
- **The task hypothesis (fungible proactive FEC beats ARQ at HIGH RTT): REFUTED.**
  No crossover; FEC/ARQ falls monotonically with RTT (1.47/0.80 tie at RTT10 →
  0.55 at RTT200). raptorpath's reliable-bulk single-path FEC does NOT have a
  high-RTT winning regime on this transport.
- **Proven genuinely proactive where it matters:** the RWM_NO_REACTIVE demonstrator
  ran at `proactive_fraction = 1.0000` (zero round-trips) and still could not win —
  it DNFs — so the loss is NOT "it was secretly reactive"; it is the deeper
  coupon-collector + transport-substrate mechanism above. When reactive IS enabled
  to make it complete, the proactive fraction collapses at high RTT (0.23) and the
  reactive loop runs away.
- **The binding constraint is the transport substrate, not the FEC coding:**
  droppable-datagram burst-overrun + reactive emission exempt from the congestion
  cap + in-order-coupled sender backpressure + per-generation decode-on-K
  intolerance to one short generation. Same root cause as the C8 aggregation ceiling.
- **Path to an actual high-RTT win (identified, not built):** pace the systematic
  SOURCE to link rate (kill burst-overrun so proactive stays ≈1.0 at BDP scale),
  bound reactive to a low non-exempt rate (rare batched tail fallback, not runaway),
  and decouple sender retention from the in-order frontier (advance on OOO generation
  completion). All three are transport changes below the FEC layer.

### Record corrected
- **This section** + paper **§14.7 / §8.9** (the fungible-mode measured correction:
  the crossover the isolated latency model predicts does NOT appear for reliable
  bulk transfer even in the fungible mode — it inverts with RTT, mechanism above).
- **Controls / no regression.** Changes are two env-gated, default-off sender knobs
  (`RWM_PFRAC` trace, `RWM_NO_REACTIVE`) + harness plumbing; byte-inert to every
  shipped path. `cargo test -p raptorpath --lib` green; `raptorpath-math` green;
  `gate_suite` 15/15 release.
**Harness.** `~/l1/pf_sweep.sh <reps> <bytes> [scens…]` (ARQ vs fungible
proactive-FEC, reads the client `[PFRAC]` trace); Mode B tuned via `BGEN/BR/BSTORE/
BINFLIGHT` env; `perf_rwm_c.sh` extended to propagate `RWM_STORE/GEN_INFLIGHT/
GEN_RATE/GEN_RATE_FLOOR/INFL_CAP/CODED_SRC/NO_REACTIVE/PFRAC/TRACE/DIAG`.
