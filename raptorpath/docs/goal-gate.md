# Goal Gate — surpass the TCP-style baseline + model reacts correctly

Executable form of the project goal, at fidelity level **L0** (in-process
simulation per ADR-0051). Run:

```
cargo test --test gate_suite -p raptorpath --release -- --test-threads 1
```

**Status: GREEN** (2026-07-02, 12/12 tests, 10 trials/cell, fixed seeds,
95%-CI separation required for every win).

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

## Honest scope

- L0's baseline is our own simulation model. It now includes slow-start,
  AIMD, and in-order semantics, but it is not CUBIC/BBR. **The claim this
  gate supports is "surpasses the SimRetx model under ADR-0051 conditions",
  not "surpasses real TCP".**
- The loss feedback timing is per-RTT-batch with sender-side knowledge of
  wire outcomes (oracle timing, same convention as bench_suite).
- Next fidelity level (L1, ADR-0051): real CUBIC/BBR/quinn/MPTCP stacks
  over netns + netem — requires Linux/WSL2; the win conditions transfer
  unchanged.
