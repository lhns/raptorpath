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
thesis measured on real stacks. (quinn message-tail comparison needs a
QUIC echo tool — object-level quinn numbers stand in until then.)

C3 (20 Mbit): bbr p99 198 ms vs rp-bulk 569 ms — BBR WINS; tunnel
block latency dominates at low rate. C5 (15% loss): rp-bulk breaks
down (24 s tails, 359 messages missing). rp-realtime stream runs
produced NO summary at c3/c5 (silent failure — open item, echoes the
earlier C5-realtime DNF). Honest scope: the tail win is demonstrated
where capacity headroom exists; low-rate and extreme-loss cells need
the block-latency and FEC-rate work items.

<<<<<<< HEAD
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
   (model-scoped, paper-documented).
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
=======
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
>>>>>>> feat/soft-saturation-taper

## L2 workstream 3 — rp-NATIVE object geometry (the fair-geometry verdict)

`raptorpath perf`: objects straight over the transport (FEC/ARQ/CC via a
memory TUN), NO inner TCP, NO kernel TUN — the true apples-to-apples
endpoint vs quinn-perf. C2, bulk, seed 42:

| object | rp-native | reliability | vs tunnel | vs quinn |
|--------|-----------|-------------|-----------|----------|
| 4 KB | 0.024 s | 5/5 | — | — |
| 100 KB | 0.094 s (8.5 Mbit/s) | 5/5 | — | — |
| 500 KB | 0.170 s (23.5 Mbit/s) | 5/5 | — | — |
| 1.8 MB | 0.92 s first-object (15.7 Mbit/s) | see bug | tunnel 1.05 s | quinn 0.20 s |

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
artifact):** large objects (>~500 KB) probabilistically STALL when the
sender idles after the final chunk — the last block's unrecovered loss
waits for ARQ, but there is no ongoing traffic to carry the repair and
the block-mode idle tail-flush does not fire in the native path
(TCP-in-tunnel masked this by ACKing continuously). 500 KB reliable
5/5; 1 MB flaky; 1.8 MB usually stalls on run 2. Next transport work
item: idle-triggered final-block recovery (tail sweep / NACK on
send-idle) for block mode. Small/medium objects and all streaming are
unaffected.

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
