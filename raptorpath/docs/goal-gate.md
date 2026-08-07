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

## MEASUREMENT DISCIPLINE (2026-07-13) — required for every future L1 verdict

*Decision record: → [ADR-0052](adr/0052-measurement-discipline.md)*

The lesson of the generation-inert era (the 2026-07-12 DAPS-era sections below;
paper §16.10–16.14; see "Methodology Audit (2026-07-13)" at the end of this
file): six mechanism verdicts were merged on measurements in which the
mechanism under test never executed, because nobody checked. No L1 verdict is
eligible for merge unless ALL of the following are in the ledger section:

1. **Mechanism-liveness proof.** The recorded run shows the mechanism under
   test actually executed: the harness `cod>0` guard output (`GUARD OK`)
   and/or the enabling flag echoed in the recorded command line. Dead code
   measures noise.
2. **Full command line + env recorded.** The exact harness invocation, every
   `RWM_*` env var, and the binary/commit hash. (The DAPS-era sections
   recorded none of this — that absence is why they can only be classified
   UNCERTAIN rather than retro-validated or definitively voided.)
3. **Interleaved same-binary arms.** A/B arms alternate within one session to
   cancel VM/session drift (documented same-nominal-config drift: 2.3×).
4. **Both seeds + per-run distributions.** Per-seed means AND per-run values;
   pooled means alone hide bimodality.
5. **Effect must exceed the recorded noise floor.** A claimed delta must
   exceed the measured same-config spread (σ_s and cross-session drift).
   Every DAPS-era "effect" (+15%, +30%, +52%, −53%, −19%) was inside it.

~~Env footguns (until fixed in code): `RWM_FMTCP=0` and `RWM_DAPS=0` still count
as SET (`.is_ok()` gates) — only some knobs treat "0" as off.~~ **[FIXED
2026-07-13, "Gen-ON Stack Ablation" JOB 1: `config::env_flag` — `=0`/`=false`
is OFF for every boolean gate; numeric-value knobs unchanged.]**

Additions from the 2026-07-14…19 batteries (binding alongside 1–5):

6. **Mechanism liveness must be proven at the RECEIVER, not just the sender.**
   The #85 span-probe datum was VOIDED because its repairs were emitted
   (sender cod/src healthy) but dropped on entry by a mismatched-backend
   decoder — it measured pure wire load. A recovery-mechanism verdict needs a
   receiver-side liveness signal (backend/decoder echo, `repairs_useful > 0`),
   not only emission counters. ("Unified Decoder", §16.20.4.)
7. **Harness arm-liveness under `set -e`.** The first #61 tail battery lost
   every legacy-RLC arm silently: `lib.sh` forces `set -e` and a no-match
   `grep` in the echo pipeline (plus a no-summary rep) killed the matrix
   mid-arm with no error. Battery drivers must `|| true`-guard match
   pipelines AND assert a per-arm result count — an arm that produced zero
   summaries fails the battery loudly, it does not vanish (commit bd13985).
8. **Known caveat class: the seed-7 topo-ping double-abort.** GE loss on
   seed 7 occasionally eats both verification echoes; aborted invocations
   leave a stale-server-log echo line (recorded, discounted). Record n per
   arm, keep per-rep values, retry per protocol, never discard a captured
   result — and never present an n<8 mean without its n.
9. **Hardware era is part of the config.** Record `lscpu` in every log
   header; compare only within the same era (the 2026-07-14 HARDWARE DIVIDE:
   qemu64/SSSE3 vs passthrough E5-2650 v3 with aes+avx2+pclmulqdq). A
   cross-era comparison must name the divide.
10. **Ops: line endings on VM sync.** A tree synced from the Windows dev box
   (e.g. via `git archive`) can carry CRLF; the harness scripts fail or
   misbehave under bash until converted (`dos2unix`/`sed -i 's/\r$//'`).
   Convert before the first harness invocation (bit the #86 battery).

11. **Pre-registration.** Before any build, write in the ledger: (a) the
   mechanism, (b) predicted effect size + cells, (c) the falsification
   condition, (d) — the borrowing lesson — *re-read the derivation for
   self-contained predictions of failure* (if the math already bounds the
   effect below relevance: research, don't build). A build whose prediction
   fails defaults to the deprecation register, not iteration, unless the
   failure itself names a new mechanism. (Added 2026-07-21, the
   consolidation pass; first exercised by "SACK-Clocked Store Release".)

12. **The VM lock covers ALL VM activity — not just batteries.** Builds,
   probes, iperf3/ping checks, netem/tbf setup, cell validation: everything
   that touches the measurement VM's CPU or network stack waits for
   `/tmp/rwm-vm.lock`. Co-tenancy has contaminated measurements three times
   (a killed crown rep 2026-07-27; compile-load on L0 tails; the 2026-08-06
   A1/B1 overlap, user-observed). Non-holders do purely local work.
   (Added 2026-08-06.)

## CONSOLIDATED VERDICT (2026-07-19) — the hardware-honest regime map

*Decision record: → decision index [ADR-0052…0067](adr/README.md) + [VISION-TRIAGE-2026-07](adr/VISION-TRIAGE-2026-07.md)*

This is the settled synthesis of the post-audit evidence base (everything
from the 2026-07-13 "Methodology Audit" onward: Generation-ON Re-Baseline →
Gen-ON Stack Ablation → Gen Substrate Ceiling → Decode-CPU Ceiling →
Copa-Sole → Copa Wire-Signal → Frontier Wedge → r\*/Taper → Hardware-Honest
Re-Baseline → Per-Path Outstanding Accounting → Unified Decoder). It
SUPERSEDES the "FINAL CONSOLIDATED VERDICT (2026-07-08)" and the "L3 REGIME
MAP" further down this file (both retained, bannered, as the honest record
of their era) and the corresponding paper regime text (now paper §17). Paper
cross-refs: §16.15–§16.20, §12.11–§12.12, §8.4.1, §8.8.

**Era discipline.** Two eras of L1 numbers exist. *Pre-divide* (≤ 2026-07-13):
qemu64 vCPU, SSSE3 only, software AES-GCM. *Post-divide* (≥ 2026-07-14):
host-passthrough E5-2650 v3 with AES-NI/AVX2/PCLMULQDQ (the HARDWARE DIVIDE
banner in "Hardware-Honest Re-Baseline"). The divide itself was an
instrument: every plain/Copa cell reproduced its pre-divide value on the
faster CPU (crypto was never a wall), so pre-divide RATIOS carry, but
absolute cross-era comparisons must name the divide. Numbers below are
post-divide unless marked.

### 1. The substrate chain — what the walls were, in order

The old maps concluded bulk was "recovery-latency-bound at ~15–17 Mbit/s"
and heterogeneous aggregation "bounded at ~parity". Both conclusions were
measurements of SUBSTRATE ARTIFACTS stacked under the transport. The walls,
in the order found, each fixed or refuted:

| # | wall | what it was | disposition | where |
|---|---|---|---|---|
| 1 | **quinn's hidden Cubic** | every datagram send is gated on quinn's own congestion window; the "15–17 Mbit link ceiling" was loss-reactive Cubic collapsing under GE loss (plain 17.5 → plain+BBR 74.5 pooled, ×4.3) | FIXED as policy: `RWM_QUIC_CC` (§2 below) | "Gen Substrate Ceiling"; paper §12.11, §16.17 |
| 2 | **quinn's PMTU black-hole detector** | a GE all-large loss burst reads as an MTU black hole → MTU reset to 1200 < the 1279-B symbol → every data send fails `TooLarge` for the 60-s cooldown = the cross-arm ~60-s "collapse run" | FIXED, ships default-ON: `min_mtu = initial_mtu = 1350`; 0/68 collapse runs vs 7.6% historic (p≈0.005); deterministic repro 63.5 s → 5.8 s | "Frontier Wedge"; paper §12.12 |
| 3 | **the coded-only wire O(G²·S)** | the ~34 Mbit/s "generation machine ceiling" was the WIRE MODE (every DoF a dense row, both ends), not the solver | FIXED: systematic-repair wire = the O(k·G·S+k³) machine; gen single-c2 33.9 → 70.9 (pre-divide), ×2.1 | "Decode-CPU Ceiling"; paper §16.18 |
| 4 | **decoder waste** | known sources materialized as full-width pivot rows etc. | FIXED: sparse-aware global rewrite, output-identical (differential-tested), ×1.2–5.0 at L0 | "Decode-CPU Ceiling"; §16.18 |
| 5 | **crypto** | software AES-GCM on every packet (qemu64 era) | REFUTED as a wall: AES-NI cut CPU 30–38%/byte and moved NOT ONE throughput cell | "Hardware-Honest Re-Baseline"; §16.19 |
| 6 | **receiver threading** | the "single-thread receiver ceiling ~93–104" | REFUTED below ~150 Mbit/sink: the engine sinks 187.7 Mbit/s single-path; the pinned receiver runs C7 at 0.66 core; parallelization NOT built (profile refutes it) — **REFUTED AGAIN 2026-07-19 at 137–144: 1+1 pinned cores = full throughput, engine 81–87% busy with empty queue; the sink ceiling attributed to per-process service-time walls (~19.5–22k sym/s), not threads ("Engine Parallelization", §16.23)** | §16.19, §16.23 |
| 7 | **per-transfer flow control** | the outstanding pool (`RELIABLE_STORE_MAX` = 1024) is a per-TRANSFER constant = a Little's-law ~100–128 Mbit wall, CPU-invariant — the ACTUAL multipath binder | FIXED, ships DEFAULT ON since 2026-07-21 ("Consolidation" LOO battery): path-scaled pool `RWM_STORE_PATHS` (knee ≈ 2048/path) — removal re-opens the c7 collapse class both seeds; per-path accounts `RWM_STORE_PERCAP` built, honest-cap-repaired (c7 ≥ pooled, sc2 exact), still < pooled at c8; bounded borrowing (`RWM_STORE_BORROW`, §16.22) derived+measured — law-perfect at the gauges, tax NOT repaid; percap family stays OFF. **c8 WATCH (2026-07-21): under SACK-release the LEGACY pool reads better at c8 (0.85–0.87×Σ vs the stack's 0.72–0.76) — the §16.22 pooled-c8 verdict was pre-SR and has MOVED; c8-aware pool law = the named follow-up. [2026-07-27, "C8-Aware Pool Law": the capacity-weighted pool (`RWM_STORE_CAPW`) derived+built+REFUTED at c8 — the gauges show the binder is SLOW-PATH CONVERSION, not pool sizing (fast path parks the span, slow path converts ~nothing, legacy c8 = fast single + 2.7); no flip, the WATCH stands with a sharper name] [2026-08-06, "C8 Slow-Path Conversion": the conversion question ANSWERED structurally — slow-path SOURCE share is monotonically anti-correlated with c8 goodput across five placement arms (6% share → 88.6 = 0.874×Σ; 16–18% → 70–83), conversion itself works (~90% first-copy) but costs more in frontier stalls + drain tail than it banks under EVERY law measured, matching kernel MPTCP-BBR's own +3.1/−2.4 vs its single; the frontier-slack placement law (`RWM_PLACE_SLACK`) refuted → register; `RWM_RECOV_MP_LIVE` (hole-law N/clocks on live_paths — the saturation-filter trap at the recovery plane) proven at the gauge (young fires 412–749 → 16-class) and half-repairs pbs's c8 collapse, flip-blocked by a 3/3-pairwise dc1 regression (named follow-up); the c8 remaining-gap owner is the SINGLE-PATH c2 gap (§16.30), not multipath]** | §16.19, §16.22, §16.29; "Per-Path Outstanding Accounting", "C8-Aware Pool Law" |
| 8 | **multipath recovery-plane over-emission** | the recovery engine keeps GLOBAL clocks/serials under striping: 82% of c7 retransmits fire inside their flight's own-path RTT clock (scheduler-created gaps read as holes, retransmits never reset the clock), and per-path loss estimators read 0.62–0.77 at a 0.1%-loss cell (global batch serials → striping gaps counted as loss) → retx ×1.8 + repair ×2.2–2.5 waste, dual-c1 sinks BELOW single | FIXED as knob: `RWM_RECOV_MP` = RFC 9002 loss detection generalized per path (9/8 time threshold on the LIVE flight + kPacketThreshold=3 same-path fast channel + snapshot coalescing); c7 retx 14.9→4.5% (+5.3/+6.4 Mbit), dual-c1 anti-scaling ELIMINATED (192.3/193.2 vs single 186.0/181.0; retx 8.5–9.5→0.3–0.7%); serial namespaces vindicated as diagnosis, runtime-refuted (default OFF); residual Σ-gap owner moves to frontier-recovery latency; **DEFAULT ON since 2026-07-21 ("Consolidation": removal −12.3/−13.9 ≫σ at c7, dual-c1 retx flood returns; `=0` = legacy opt-out)** | "Multipath Recovery Suppression"; paper §16.24 |
| 9 | **frontier-clocked store release** | the retention store frees slots only on the CUMULATIVE frontier, so SACKed-but-not-cumulative symbols hold flow-control slots a full frontier round — at c7 the store recycles at frontier latency, not path rate (the §16.24 residual: wire un-full, goodput stopped) | FIXED, ships DEFAULT ON: `RWM_STORE_SACK_RELEASE` = SACKed seqs uncounted from the outstanding gate, payload + ARQ maps retained until the frontier (slot release ≠ recoverability — the SACK_PRUNE distinction); c7 0.885→0.959×Σ SR-only, **1.018–1.045×Σ composed with `RWM_RECOV_MP`** (both seeds); sc2 +4.3/+2.9 ≫σ; dual-c1 composed +20–22 above single; occupancy 3,157→1,460 at 167k slots released/200 MB with retx FALLING | "SACK-Clocked Store Release"; paper §16.25 |

What remains STRUCTURAL (not a wall): the presence⊥throughput identity —
on a saturated single reliable path FEC = ARQ parity is the ceiling,
confirmed again post-divide (gen-sys single = 0.97–1.0× plain+BBR; the
coding is free, not free throughput).

### What the transport measures today (post-divide, same-session interleaved)

| cell | best measured | config | reference point |
|---|---|---|---|
| single-c2 plain | 76–79 Mbit/s | plain + `RWM_QUIC_CC=bbr` | legacy Cubic-under: 17 |
| single-c2 gen-sys | 75.5–75.7 (0.97–1.0× plain+BBR) | GPB stack + `--window-systematic-repair` | FEC tax ≈ 0.37 s recv CPU / 25 MB |
| single-c3 (lossy 20 Mbit) | 15.6–15.9 (the recovery ceiling) | plain+BBR; gen-sys 14.9–15.1 = 0.95× (pre-divide) | legacy Cubic: 3.2 |
| C7 (c2+c2) | **166.3–166.7 = 0.982–0.988×Σ AT THE SHIPPED DEFAULT** (the consolidation stack, 2026-07-21; same class as §16.25's composed 1.018–1.045×Σ vs the base-arm Σ) | SR-only (pre-stack ship) 146.5–148.0 = 0.86–0.88×Σ; PB legacy-era baseline 100–105 | "Consolidation" |
| C8 (c2+c3) | **85.9–87.4 = 0.854–0.870×Σ** (best measured: SR + `RWM_RECOV_MP` + anchors on the LEGACY pool — the consolidation loo-pbs arm, both seeds, σ 2.9–3.8) | shipped default (stack incl. path pool): 72.6–76.1 = 0.72–0.76×Σ — the c8 WATCH (wall #7 row); historic pooled record 0.74–0.80×Σ | PB legacy-era baseline 44–65 (bimodal) |
| engine sink | 187.7 Mbit/s (177–193 re-measured 2026-07-19) | single-path c1, 1 receiver task | attributed: sender-emission service wall ~19.5–20k sym/s first, receiver engine ~20–22k msgs/s just above; NOT thread-count (pins −7%/−2%); dual-c1 sinks BELOW single (spurious-retx flood 9.3%) — "Engine Parallelization"; flood KILLED by `RWM_RECOV_MP` (dual 192–193 ABOVE single, retx 0.3–0.7%; §16.24) |
| realtime delivery, c3 100 KB | unified 99.4–100% / streaming 73.8–76.0% | `RWM_UNIFIED=1` vs shipped | ×3–4 completer-median cost |
| message-tail p99 | 12–48× vs quinn/kernel-TCP at C2 (pre-divide crown, flip-gate-defended) | shipped streaming Realtime | post-divide c3 matrix: legacy-RLC medians best (§5 below) |

**Default honesty (rewritten 2026-07-21 — the scandal is CLOSED):** the
shipped default IS the best-measured composed configuration. What ships ON
with everything unset: BBR-under (`RWM_QUIC_CC` unset ⇒ bbr, Item 0),
SACK-clocked store release (`RWM_STORE_SACK_RELEASE`, §16.25), the
path-scaled outstanding pool (`RWM_STORE_PATHS`), multipath recovery
suppression (`RWM_RECOV_MP`), the anchor-hygiene pair
(`RWM_MSTAR_ANCHOR`, `RWM_CLOCK_GAP`), the MTU floor (wall #2) and the
corrected r\* solver (`RWM_RSTAR_TAIL`). Each of the four consolidation
members carries its own leave-one-out row on both seeds ("Consolidation",
2026-07-21); the legacy behaviors remain the explicit `=0` opt-out arms.
The one place the default is knowingly not the best-measured config is
heterogeneous c8 (the wall #7 c8 WATCH — a named, pre-registerable
follow-up worth +11–13 Mbit); everything refuted is in the DEPRECATION
REGISTER above with a walls-active argument and a re-test clause.

### 2. The CC policy surface — substrate CC is POLICY (`RWM_QUIC_CC`)

- **Cubic (the unset default): dead as a performance choice.** It was wall
  #1; it survives only as the untouched shipped default pending the
  fairness/competitive batteries.
- **BBR-under (`bbr`): the bulk-throughput champion.** Plain single-c2
  74.5–79, C7 ~100–105, the reference arm of every battery since §16.17.
  Cost: standing queue (38 ms wireQ at sc2; 88–124 ms on the C8 slow path,
  p90 to 2.5 s) and a c3/C8 bimodal collapse mode (partly the wedge, fixed;
  partly BBR's own).
- **Copa-sole (`passthrough` + wire signal): the queue/tail champion — a
  MEASURED TRADEOFF, not a bulk-parity candidate.** The engine's per-path
  Copa-lite cwnd IS quinn's window; after the wire-clock fix (paper §12.4
  addendum) it holds the NETWORK standing queue ×18/×16/×6–7 tighter than
  BBR-under at sc2/sc3/c7 (wireQ 5/30/7 ms vs 89/487/50) with natural
  RTT-floor freshness (the ±v/δ dither refreshes the raw 10-s min without
  BBR's ProbeRTT drain — no FEC protection gap), and ties BBR on the
  realtime c2 message tail. δ maps from the hint with no new constants
  (δ = 0.5/ζ; live-verified Bulk 0.005 / Auto 0.5 / Realtime 50). **The
  bulk cost is real and does NOT close on the fixed substrate** (goal-gate
  "Copa-Sole on Clean Substrate", 2026-07-22): copa/bbr 0.89× sc2, 0.97×
  sc3, 0.73× c7, 0.57× c8, 0.66× dc1, ≫σ both seeds. The #82 "C8
  domination" was a broken-substrate artifact (it suppressed BBR); on the
  consolidated stack BBR-under leads C8 throughput and the walls WIDENED
  the gap — Copa's δ-equilibrium caps cwnd near BDP + 1/δ and leaves the
  freed pipe on the table. NO default flip; the fusion (ADR-0068) inherits
  the gap.
- ~~**The named gap that blocks any default flip:** Copa-lite has NO
  TCP-competitive mode (Copa §4 not built) — against loss-based
  cross-traffic a delay-based controller yields, and no cross-traffic cell
  has ever been measured (this gates BBR's fairness case too). Substrate CC
  remains a policy surface with defaults unchanged. ("Copa-Sole", "Copa
  Wire-Signal"; paper §12.11.)~~ **[CLOSED-and-MOVED 2026-07-19, roadmap
  item 6 (`feat/copa-compete`): Copa §2.2 competitive mode BUILT
  (`RWM_COPA_COMPETE`, default OFF) and the first cross-traffic battery
  MEASURED. At the lossy c2 cell Copa-sole is cross-traffic-safe (0.88–0.90
  share vs Cubic, compete irrelevant, Cubic-friendlier than BBR). At the
  CLEAN shared bottleneck Copa-sole starves (share 0.023) and competitive
  mode does NOT restore share — attributed by probe: δ is not the binder;
  the plain ARQ/1024-pool pipeline under contention tail-drop is (Little's
  law, wall #7 at a shared bottleneck). BBR-under: 0.24 share at a
  305–316 ms standing queue. The CC-flip gate is now the
  shared-bottleneck contention-recovery pipeline, not the CC. Goal-gate
  "Copa Competitive Mode + Cross-Traffic".]**

### 3. Aggregation vs Σ — the bulk N× verdict

The user's claim: bulk multipath striping should approach N× per-path rate.
Verdict: **substantially validated at C7, mechanism-named gap at C8.**

- **C7 symmetric: 0.87–0.97×Σ.** The binder was never the receiver (wall
  #6 refuted) — it was the per-transfer pool (wall #7). Path-scaled pool
  (PBS): ×1.72–1.89 of the same-session single, 0.86–0.94×Σ (#84). Per-path
  accounts (PBP): 0.87/0.97×Σ — the ≈1.0 target TOUCHED at s7, with the
  pooled arm's collapse mode absent (#86). Copa rides the same unlock
  (C1P ×1.34–1.54 of its own single).
- **C8 heterogeneous: best ~0.74×Σ (pooled PBS arm; 0.79–0.80×Σ in the #84
  session — session drift is why every verdict is same-session
  interleaved).** One shared pool cannot be sized for a c2-deep and a
  c3-shallow path at once; the percap lever BUILT for this cell REGRESSED
  it to 0.38–0.43×Σ under both CC families — the cap-full placement
  redirect over-commits the slow account (~2048 symbols on a 15.7 Mbit
  path ≈ 1.3 s dwell; holes recover ~13× slower; frontier serializes).
  `RWM_STORE_PERCAP` stays OFF.
- **The two named residuals** (roadmap items 1–2): the percap
  redirect-guard — MEASURED 2026-07-19: it halves the c8 regression
  (0.40–0.41 → 0.52–0.55×Σ) but does not reach the PBS bar; the residual
  was own-pick parking under the knee-clamped slow cap + account
  no-borrowing — **HONEST-CAP FOLLOW-ON MEASURED same day
  (`feat/percap-honest-cap`): the cap channel is fixed as far as honest
  inputs allow (sc2 −20% resolved exactly, c7 percap ≥ pooled at
  0.89–0.90×Σ both seeds, c8 +3.4/+3.8 over the knee-clamped control with
  the parking tail halved) and the residual c8 gap is the account
  structure itself: C1P-H < C1 twice with caps honest by construction —
  the NO-BORROWING TAX is the confirmed c8 binder (item 1, redirected to
  bounded account borrowing; a measured sub-residual: the slow path's
  send-interval anchor over-reads ×3–5 under multipath placement)** —
  and receiver/sender task parallelization — LIVE at the symmetric cell
  (PBP c7-s7 = 147.4 ≈ the ~150 threshold), noted, not built —
  **profiled and REFUTED a third time 2026-07-19 ("Engine
  Parallelization", §16.23): 1+1 pinned cores sustain the operating
  point; the c7 binder is multipath recovery-plane over-emission on a
  saturated wire; successor lever = multipath-aware recovery
  suppression (named, not built).** **[SUCCESSOR BUILT + MEASURED
  2026-07-21 ("Multipath Recovery Suppression", `RWM_RECOV_MP`,
  §16.24): the waste was two per-path-vs-global defects; the RFC-9002
  per-path law kills it (c7 retx 14.9→4.5%, +5.3/+6.4 Mbit to
  0.88–0.89×Σ; dual-c1 anti-scaling ELIMINATED) — and the freed wire
  NOT converting 1:1 into goodput REVISES the §16.23 attribution: the
  remaining c7 Σ-gap owner is frontier-recovery latency on the
  ack-serialized store. Flip OFF (c7 ~1.0×Σ target missed).]**
- No cell exceeds its link-class Σ ceiling; "every wall so far has been an
  unscaled constant or a hidden substrate controller, not the architecture"
  (§16.19).

### 4. The FEC story, honestly

- **Single-path throughput: parity, the identity confirmed.** Gen-sys
  single-c2 = 0.97–1.0× plain+BBR; c3 = 0.95× the plain+BBR recovery
  ceiling (pre-divide). FEC does not buy saturated single-path bulk
  throughput; that is a property of reliable delivery, not a gap.
- **Coding is ~free on real silicon.** On AVX2 the whole FEC machine costs
  ~0.37 s recv CPU per 25 MB over plain (0.97× throughput) — the "coding
  tax" era is over (walls #3–#5).
- **Generation coding is the STABILIZER.** Gen-bare C8 σ 0.14–0.48 vs
  plain's 2.0–2.1 (ablation, pre-divide); gen-sys C8 σ halved vs plain+BBR
  with the bimodality gone (§16.18). Where plain+BBR pays a bimodal penalty
  for touching a lossy path, the coded arms park stably.
- **Tail latency is the crown: 12–48× message-p99** vs quinn/kernel-TCP at
  C2-class loss (Full Benchmark Re-Run, Metric A — pre-divide, reproduced
  across re-runs), held by the SHIPPED streaming machine and DEFENDED at
  the 2026-07-19 flip gate (unified realtime is NOT tail-parity: p99
  medians 2.7–3.3× legacy-RLC at c3 + a 3/10 stream-collapse class).
- **NEW measured point — delivery-complete realtime.** The unified small-δ
  machine at the c3 realtime cell: 99.4–100% delivered vs the shipped
  streaming 73.8–76.0%, at ×3–4 completer medians and cod/src 0.34–0.42
  (r consumed as computed, recovery-live at the receiver). That is a
  DISTINCT (δ, ρ) profile candidate — +24–26 pp reliability bought with
  completion tail — not a defect of either machine.
- **r\* bursty provisioning: correct at the solver, entangled at the
  shipped realtime wire.** The §8.4.1 mass-quantile solver is
  oracle-validated (feasible-cell worst residual 2.88× → 1.41×; heavy-tail
  synthetic 5.1×-miss → 0.99×-hit; infeasible cells DECLARED) and ships ON.
  The wire quantity law is FIXED (`RWM_TAPER_R`: cod/src 0.06–0.09 →
  0.32–0.35 at L1) but consuming r DEGRADES streaming-family delivery
  (−19/−25 pp, both seeds, both rungs) — the leading-window unsolvable-span
  entanglement, CONFIRMED on the real substrate; flip closed. The
  solvable-span emission exists structurally in the unified machine
  (trailing span, §16.20.3) — where it IS delivery-complete. The RLC family
  at the same cell is ARQ-complete (the −22 pp was a streaming-family
  property, rescoped in §16.20.4).

### 5. The three-machine map

| machine | measured niche | flip-gate status | retires when |
|---|---|---|---|
| **Streaming two-layer** (shipped Realtime default) | the 12–48× message-tail crown (Metric A); burst-optimal diagonal layer | DEFENDED 2026-07-19: unified fails tail parity (p99 ×2.7–3.3 + collapse class) → keeps Realtime | only if a retirement case engages BOTH the unified trade AND the L1 ordering datum below; honest liabilities recorded: 24–26% DNFs at the c3 100 KB realtime perf cell, and at the post-divide c3 tail matrix the legacy-RLC arm posts BETTER p99 medians (234/273 ms vs streaming 510/822 pooled) — the "streaming-retirement gap", roadmap item 7 |
| **Legacy RLC family** (`RlcWindowDecoder` plain window + `GenerationDecoder` gen wire) | the bulk half: gen-sys = the bulk-champion wire (parity with plain at ~free CPU); the plain-RLC realtime arm posts the best c3 p99 medians of all three machines | bulk gate: unified reached parity+CPU-parity against it (PASS); realtime: it BEAT unified at the bursty cell, but carries its own 2/10 total-wedge class | both decoders retire when unified passes ≥ legacy-RLC EVERYWHERE — the remaining gap is realtime tail only; known defect: rank loss on late sources under reorder (unified already fixes it) |
| **Unified span machine** (`RWM_UNIFIED`, default OFF) | ONE decoder + δ-continuous span law (A\*/M\*/Δ), differential-proven vs all three legacy decoders; bulk gen-sys parity + CPU parity PASS at L1; delivery-complete realtime (+24–26 pp) | BOTH flips NO (2026-07-19): named blocker = the c3-1200B stream-collapse rep class (3/10, p50 in seconds) — **ATTRIBUTED 2026-07-19 (`diag/unified-collapse`): not the decoder; the A\*-anchor defect + the family-level transient-amplification response (COLLAPSE ATTRIBUTION)**; ~~M\* knee UNREACHABLE behind two anchor defects~~ **anchor repairs BUILT 2026-07-19 ("Anchor Hygiene", `feat/anchor-hygiene`): A\* live in ~1 RTT + flood-poison-proof, M\* knee ENGAGES at L1 (r100 +25/+31%, r200 +62/+82%)** | it is the DESTINATION: the flip-battery re-run is measurement-ready (fixes A+B built); the flip itself still gates on the re-run + the overload-shedding policy (fix C); streaming retirement is a separate, harder gate |

(The block RaptorQ pipeline remains §15's other knob, untouched; the
Full-Benchmark-Re-Run showed its lossy completion is recovery-bound and
unchanged by the CPU-era fixes — the window/gen path is where the wins live.)

### 6. THE ROADMAP — every named-not-built follow-up, prioritized

Per MEASUREMENT DISCIPLINE these are named, scoped, and NOT built; each
carries its gating decision.

1. **percap-redirect-guard** — ~~bound the cap-full placement redirect by
   the target account's absolute dwell~~ **[BUILT + MEASURED 2026-07-19,
   `fix/percap-redirect-guard` 689b9f1: the floor-clock bound
   (bound_j = rate_j·RTprop_j; κ=1 on the loaded echo clock is provably
   vacuous) closes the redirect channel — slow-account dwell collapses
   ~4×, +12/+11.5 Mbit over the unguarded control, both CC families both
   seeds — but PBP-G < PBS at c8 both seeds (0.52–0.55×Σ vs 0.67–0.72),
   so the flip stays NO and the c8 record remains pooled PBS. The
   residual parking is now attributed PAST the redirect: (i) own-pick
   placement under the plain-anchor over-read's knee-clamped slow cap —
   the CAP needs the same floor-clock dwell bound
   (cap_i ≤ gain·rate_i·RTprop_i) and/or the #79 sampler generalized to
   plain mode; (ii) under honest Copa caps, account isolation denies the
   fast path the pooled law's borrowing at asymmetric cells. Both named,
   NOT built; they inherit this item's gate on any `RWM_STORE_PERCAP`
   flip and the C8 0.9×Σ target. Ledger: "Per-Path Outstanding
   Accounting" → GUARD RESULTS.]**
   **[Residual (i) BUILT + MEASURED 2026-07-19 (`feat/anchor-hygiene`,
   `RWM_PLAIN_RS`): the #79 send-interval sampler generalized to plain
   mode (sampling-only CopaFeed). The knee-clamp over-read is GONE — plain
   btlbw reads 1.02× truth at sc2 (was ×4.6–6.2) and the c8 slow path
   ×4.7–7.4 → ≤1× — and c8 plain throughput improves with σ collapsed
   (48.3/57.5 σ 5.1/19.1 → 55.4/61.9 σ 9.3/4.0). Named costs: sc2 single
   −20% (the over-read was accidentally load-bearing for the anchor-sum
   store cap — the same circularity §16.19 documented for the Copa feed)
   and slow-path UNDER-read when placement starves it of source (safe
   direction for a cap). The percap cap re-derivation (cap_i from honest
   BtlBw) + re-battery remains this item's open follow-up; see "Anchor
   Hygiene" gate-readiness.]**
   **[Cap re-derivation BUILT + MEASURED 2026-07-19
   (`feat/percap-honest-cap` 5d30c02, `RWM_HONEST_CAP` under
   `RWM_PLAIN_RS`): cap_i = anchor_i·(K_i+gain−1) + rate_i·(gain−1)·R —
   residence on the measured unloaded drain clock (K = windowed-min
   echoSRTT/RTprop, self-queue-proof) + one recovery round on the
   RECOVERY engine's clock (R = the 100-ms hole-refresh/tail-sweep
   ceiling; the literal floor-clock form was refuted by its own smoke:
   c2's RTprop is 8 ms and the good 1024 store is ~12× the floor BDP).
   The sc2 −20% is RESOLVED exactly (PBP-H = PB both seeds; the =0
   control reproduces −18/−22%); c7 percap ≥ PBS both seeds
   (0.89–0.90×Σ); c8 improves +3.4/+3.8 over the knee-clamped control
   with the slow-path parking tail halved — but PBP-H < PBS at c8 both
   seeds and C1P-H < C1 both seeds with honest cwnd caps: residual (ii)
   — the NO-BORROWING TAX — is the confirmed c8 binder. Flip NO;
   `RWM_STORE_PERCAP`/`RWM_PLAIN_RS`/`RWM_HONEST_CAP` all stay default
   OFF. THIS ITEM REDIRECTS to bounded account borrowing (named, not
   built: a borrowed symbol parks on the lender's account but flies on
   the borrower's pipe — the dwell derivation needs a new law, not a
   clamp) or accepting pooled PBS as the c8 record. Sub-residual (iii),
   measured: the slow path's send-interval anchor over-reads ×3–5 under
   multipath placement (frontier-advance burst attribution suspected;
   honest at N=1 and on the fast path). Ledger: "Per-Path Outstanding
   Accounting" → HONEST-CAP RESULTS.]** (#86)
2. ~~**Receiver/sender task parallelization** — refuted below ~150 Mbit/sink,
   now LIVE at the symmetric cell (PBP c7-s7 147.4; engine sink 187.7). The
   next C7 lever after flow control. (#84/#86)~~ **[CLOSED 2026-07-19 as the
   THIRD refutation, binder named (`feat/engine-parallel`, goal-gate "Engine
   Parallelization"): at the best c7 arm both processes pinned to 1 core
   EACH sustain full throughput on both seeds (pinned 136.3 ≈ unpinned
   136.2), the engine receiver task runs 81–87% busy with a near-empty
   queue (new `RWM_RDIAG` gauge), and the c7 wire is measured SATURATED by
   recovery-plane over-emission — retx share ×1.8 and repair share
   ×2.2–2.5 the same-config single-path level ≈ exactly the Σ-gap
   (0.85–0.86 this session). The dual-c1 control reproduces the flood with
   ~zero real loss (retx 9.3% of source vs single-c1's 0.2%; dual sinks
   BELOW single, 174–176 vs 180–184). The real task-service walls are
   measured at ~19.5–20k sym/s (sender emission, binds first) and ~20–22k
   msgs/s (receiver engine) ≈ 185–200 Mbit/sink — c1-class only.
   `RWM_ENGINE_PAR` was NOT built (nothing to flip). SUCCESSOR lever
   (named, not built): multipath-aware recovery suppression — cross-path
   in-flight awareness for the hole-refresh/tail-sweep engine; it now owns
   the c7 ~12–15% Σ-gap and the dual-c1 anti-scaling. Paper §16.23.]**
   **[SUCCESSOR DONE 2026-07-21 (`feat/recovery-suppression`,
   `RWM_RECOV_MP`, goal-gate "Multipath Recovery Suppression", paper
   §16.24): the per-NACK trace split the over-emission into (1) a
   GLOBAL hole clock — 82% of c7 retransmits fired inside their
   flight's own-path RTT window — and (2) GLOBAL batch serials
   poisoning the per-path loss estimators (0.62–0.77 read at 0.1%
   loss). The law = RFC 9002 loss detection per path (time threshold
   9/8 on the live flight, packet threshold 3 on same-path delivered
   successors, retransmit inherits its clock, snapshot coalescing).
   MEASURED both seeds: dual-c1 control retx 8.5–9.5% → 0.3–0.7% with
   the dual aggregate ABOVE single (anti-scaling eliminated); c7 retx
   14.9–15.0 → 4.5–4.7% (BELOW single parity), +5.3/+6.4 Mbit to
   0.88–0.89×Σ; c8 null-to-positive; N=1 identity clean. The ~1.0×Σ c7
   target is MISSED and the miss is the discovery: the freed wire does
   not convert — the Σ-gap's residual owner is frontier-recovery
   latency on the ack-serialized retention store (next lever:
   SACK-clocked store release composed with the suppression). The
   serial fix is vindicated as diagnosis, refuted as runtime (honest
   signals re-heat every SRTT/loss-scaled cadence; ×2.4 CPU) — the
   honest-signal cadence re-derivation is the named follow-up. Flip
   OFF; all knobs byte-identical unset.]**
3. **Unified-realtime c3-1200B stream-collapse attribution** — **DONE
   2026-07-19 (`diag/unified-collapse`, "Unified Decoder" → COLLAPSE
   ATTRIBUTION):** reproduced at a new L0 sustained-stream rung; NOT a
   decoder mechanism (global RREF empty throughout, decode µs-class — the
   L-growth/re-elimination/allocation candidates refuted by trace); the
   class is a whole-process-transient amplification shared by BOTH
   RLC-family realtime arms (reliable-in-order backlog + post-stall
   anchor poisoning: BtlBw ×13, cwnd ×16, RTT ×3), while the streaming
   arm sheds ~1% of messages and stays flat. Two unified anchor defects
   NAMED: A\* pinned at 1 for ~10 s (cold 2-s EWMA rate anchor ⇒ realtime
   FEC inert, ru/rf ≈ 9%) and flood-poisonable A\*. Blocker decomposed
   into named fixes A (anchor repair) + B (estimator clock-gap hygiene) +
   C (δ-honest overload shedding); flip (a) re-opens on A(+B) + battery
   re-run. (#61)
4. **Legacy-RLC realtime total-wedge class** — **SAME ROOT as item 3
   (measured at L0: the legacy-rlc arm shows the identical episodic class
   with no unified code in the loop; one class, two terminal behaviors).**
   Closes with item 3's fixes. (#61)
5. ~~**The M\* anchor pair + knee re-run** — (i) RTprop floor under-read (a
   DEFAULT_SRTT-class 50-ms seed surviving inside the 10-s min-window at a
   200-ms cell), (ii) the static `(pipeline+2)·G` win backstop (not
   M\*-coupled). Fix both, then re-run c2r100/c2r200 — oracle PART 7b's knee
   is neither confirmed nor refuted until then. (#61; §16.17's residual)~~
   **[BUILT + KNEE MEASURED 2026-07-19, `feat/anchor-hygiene`
   (`RWM_MSTAR_ANCHOR`): the 50-ms floor was the PEER-REPORT feedback loop —
   `PathReport.avg_rtt_us` is the peer's estimator VALUE (its own 50-ms-seeded
   EWMA, never fed on a pure receiver) recorded as a local RTT sample every
   ~2 s, re-planting a 50-ms "sample" in the 10-s min-window forever. Fix =
   don't record estimates as samples + seed the local EWMA from the first
   measured sample + derive the win backstop from M\* once anchors live. The
   KNEE ENGAGES at L1: c2r100 36.5/38.8 → 47.9/48.5 (+31/+25%), c2r200
   19.2/20.3 → 34.9/32.9 (+82/+62%), n=8 both seeds, 0 DNF, non-overlapping
   per-rep distributions at r200. Oracle PART 7b's m=2 deficit is confirmed
   in DIRECTION and ordering (r200 gap ≫ r100 gap); measured m=2/M\* ratios
   0.76-0.80 (r100) and 0.55-0.62 (r200) vs the in-model 0.64/0.39 — the
   wire keeps other binders (receiver ~1-core ceiling class). See "Anchor
   Hygiene". Item 8's bookkeeping-cost datum is SUPERSEDED in the fixed
   regime (the M\*-law arm now clearly above fixed-depth).]**
6. ~~**Copa competitive mode + the cross-traffic cell** — build Copa §4 mode
   switching; measure the FIRST shared-bottleneck/cross-traffic battery
   (this also carries BBR's unevaluated fairness). Gates ANY substrate-CC
   default flip. (#80/#82)~~ **DONE 2026-07-19 (`feat/copa-compete`;
   goal-gate "Copa Competitive Mode + Cross-Traffic"): mechanism built
   faithfully (Copa §2.2 — detection + AIMD on 1/δ + δ(hint)-base
   composition), unit- and liveness-proven; 4-arm battery at c2 AND clean
   × both seeds. Outcome: c2 cross-traffic-safe (0.88–0.90 share);
   clean-cell starvation REAL (0.023) and NOT a CC problem (δ-null probes)
   — the binder is the contention-loss recovery pipeline (pool × frontier
   × ARQ under tail-drop); BBR fairness measured (0.24 share, 305–316 ms
   queue; crushes Cubic at c2). NO default flip. SUCCESSOR item:
   shared-bottleneck contention recovery (contention-scaled pool /
   loss-burst NACK cadence / FEC-protected blocker retransmit), measured
   against the clean cross-traffic cell.**
7. **The streaming-retirement gap** — attribute why the legacy-RLC realtime
   arm beats the shipped streaming machine's p99 medians at the L1 c3 cells
   (234/273 vs 510/822 ms): the L0 c3heavy proxy predicted the opposite
   ordering. Any future streaming-retirement case must engage this datum.
   (#61 bonus finding)
8. ~~**r200 M\*-arm bookkeeping cost** — the M\*-law arms sit ~1–2 Mbit BELOW
   fixed-depth at c2r200 on both machines/seeds (~1.3–2σ). (#61)~~
   **[SUPERSEDED 2026-07-19 ("Anchor Hygiene"): that datum was measured in
   the anchor-broken regime where M\* never left its floor (the law's
   bookkeeping without its benefit); with the anchors fixed the M\* law
   clears fixed-depth by ×1.6–1.8 at r200.]**
9. **The solvable-span default-flip chain** — code streaming-family/plain
   proactive repair over a decodable TRAILING span (or route realtime
   through the RLC/unified family), and revisit whether contract-priced
   repair should bypass the spare-cap gate. Gates `RWM_TAPER_R` and the
   full realization of the corrected r\* at the realtime wire. (#85/#46)

10. **The CC endgame: adversarial cells → measured Copa breakage → the
   fusion [ADR-0068](adr/0068-copa-bbr-fusion.md)** (named 2026-07-21;
   NOT buildable-falsifiable on the current rig). The clean-substrate
   Copa question closes with the "Copa-Sole on Clean Substrate" battery
   (below); what remains structurally open is the regime the current
   cells cannot reach — delay-jitter (WiFi/LTE aggregation class),
   shallow buffers, policers — where a pure delay-priced controller is
   predicted fragile and BBR's explicit rate model is the known answer.
   The lever is ONE controller, not a switch: δ-priced probing over a
   measured rate model (the ADR-0061 anchors) with ε̂-referenced loss
   discrimination. Gate chain, pre-registered in the ADR: build the
   adversarial cells FIRST, measure Copa-sole's breakage as the
   baseline, then pre-register the fusion per discipline item 11;
   literature verification (BBRv2 / PCC-Vivace / Nimbus mechanisms from
   sources) required before any build. No breakage measured ⇒ nothing
   to build.

Minor named items (recorded, unranked): the c7 unified-receiver +3–5% CPU
signal; the `np` 2→1 live-path flap under saturation (shared contributor at
both dual cells); the `RWM_STORE_PATHS` default-flip battery; the harness
gen-arm default is still the coded-only wire (flipping the battery default
to systematic-repair is recommended, §16.18); ~~BBR-under fairness battery
(folds into item 6)~~ (measured 2026-07-19 with item 6: 0.24 share vs
Cubic on clean at a 305–316 ms standing queue; 0.95–0.96 share at c2).

**[ADDENDUM 2026-07-22, "Competitive Baseline" (end of file): the first
external referee run re-prices this roadmap. New/re-priced levers from
the losses: emission batching/GSO (c1 ×5.5 to userspace QUIC), the
lossy-single recovery residual (−9…−14% to BBR-class arms at c2/c3),
and the c8-aware pool law (now worth ~+20 Mbit against kernel
MPTCP-BBR, which also matches rp at c7). The realtime crown verified as
the durable class win (only delivery-complete arm; bounded tails).]**

### 7. Superseded-artifact index (what to NOT cite as current)

- "FINAL CONSOLIDATED VERDICT (2026-07-08)" below — systematic-repair-era,
  pre-substrate-chain; its structural claims (presence⊥throughput,
  recovery-latency serialization AT that era's operating point) stand, its
  ceilings (15–17 Mbit "link", C8 14.7 "bounded at parity", "NOT a
  faster-bulk-transfer transport") are superseded by walls #1–#7.
- "L3 REGIME MAP" below — pre-arc baselines with 2026-07-08 patches;
  Metric A's tail-crown CLASS stands; every throughput/completion row is
  era-bound.
- Paper §16.8 verdict and §16.6's "Grounded verdict" — same era, bannered
  in the paper; see paper §17.
- §16.10–16.14 (DAPS era) — generation-inert, audit-classified; already
  bannered per section.

## DEPRECATION REGISTER (2026-07-21, consolidation pass) — Class-C gates: two-stage deprecate → re-test-on-clean-substrate → delete

*Decision record: → [ADR-0066](adr/0066-deprecation-register.md)*

Discipline (per the consolidation roadmap, Pierre's amendment (a)): before
any refuted mechanism is DELETED, ask whether its refutation predates a
since-removed wall it plausibly collided with (the project's own history:
DAPS "refuted" on dead code; FMTCP's "strictly worse" measured before the
MTU-wedge/pool/recovery fixes). Every Class-C gate below now `warn!`s on
activation naming its refuting section (`config::deprecated_env_flag`).
NOTHING is deleted this pass; deletion requires the re-test column's clause
satisfied on the consolidated stack (BBR + SR + PBS + MP + anchors — see
"Consolidation" below). **[2026-07-27 UPDATE: the code-consolidation pass
executed the register — per-row REMOVED status + commits below; see "Code
Consolidation (2026-07-27)".]** **[2026-07-27 LATER: consolidation pass 2
executed the two rows that pass left behind their re-test clauses — FMTCP
(f841757) and the streaming machine (bccb32a); the register is now FULLY
EXECUTED, no Class-C gate remains in the tree; see "Code Consolidation 2
(2026-07-27)".]** Walls key: **W1** quinn hidden Cubic · **W2** MTU
black-hole wedge · **W7** 1024-pool flow-control law · **W8** global
recovery clocks (phantom retx) · **GEN-INERT** generation-inert harness era
(§16.10–16.14, audit-classified) · **PRE-DIV** pre-hardware-divide
(qemu64/SSSE3).

| gate | refuting section | walls ACTIVE at refutation | re-test required? | status |
|---|---|---|---|---|
| `RWM_FMTCP` (+`_WIN`) | "FMTCP Aggregation Build (2026-07-08)": C8 0.48×Σ-fast, strictly worse than plain (14.37→7.58) | **W1, W2, W7, W8, PRE-DIV** — its entire table sits in the Cubic-era 7–25 Mbit band; its named mechanism ("recovers over a bufferbloat-inflated RTT", ~2 s spikes) is exactly the class walls W7/W8 later explained for plain mode | **YES — the strongest re-test case in the register.** Refuted pre-EVERY-wall; the composite (total-in-flight + per-path BDP + fungible repair) has never run on the clean substrate. Counter-weight, recorded honestly: its failure reproduced FMTCP's own abstract's pathology (slow subflow = bottleneck), and the clean-substrate c8 story (§16.22 no-borrowing tax) still names that same structural axis — the re-test is owed but the prior is against it | **RE-TESTED 2026-07-27 → CONFIRMED-REFUTED** ("C8-Aware Pool Law" battery, piggybacked arm, binary 1d09eb32… = 080073c, seeds 42+7 ×4 interleaved on the FULL clean substrate — BBR + MTU floor + SR + PBS + MP + anchors): c7 18.30/18.98, c8 14.30/15.03 Mbit/s = ×0.11/×0.20 of the same-session default stack, strictly worse than every plain arm at both cells both seeds ≫σ; dnf=0; cod_share 1.02–1.17 (recovery flood), ~8× plain CPU. The 2026-07-08 pathology reproduces with every wall removed — the refutation was never wall-tainted. ~~CLEARED FOR DELETION next consolidation pass~~ **REMOVED f841757 (2026-07-27, Code Consolidation 2)** — chain deleted (`RWM_FMTCP`, `RWM_FMTCP_WIN`, the forced sub-lever couplings; the sub-lever gates themselves and the shared per-path in-flight/percap patterns retained). No ADR re-opens |
| `RWM_SRC_BP` | "Source Backpressure (2026-07-12)": C8 −53% both seeds | **GEN-INERT (audit: §16.10–16.13 UNVERIFIABLE — no recorded env; −53% fits inside the 2.3× era noise), W1, W2, W7, W8, PRE-DIV** — the section pre-dates the discipline it helped motivate; the "live code at a bottlenecked era" reading is the CHARITABLE one and cannot be verified from the record | YES in principle, **LOW priority** — the mechanism space (defer source emission into per-path pacing budgets) was superseded by the per-path account family (`RWM_STORE_PERCAP`/honest caps/borrowing, §16.21–16.22), which asked the same admission question on live code with gauges and lost to pooled at c8 for a NAMED structural reason (no-borrowing tax) | **REMOVED 8902d24 (2026-07-27)** — VISION-TRIAGE ruling accepted: the mechanism space was re-asked by the percap family on live code and lost for a named structural reason (ADR-0058); the gen-mode re-test clause transfers to this row's text, not the code |
| `RWM_SACK_PRUNE` | "SACK+BDP Reassembly (2026-07-08)": C7/C8 in-order DNF (wedge) | walls were active (W1, W7, PRE-DIV) but **IRRELEVANT: the unsafety is STRUCTURAL** — pruning `sent_store` destroys the only retransmittable copy, so a received-then-evicted symbol at the receiver's bounded reassembly window is unrecoverable. No wall excuses destroying recoverability | **NO.** SUPERSEDED 2026-07-21 by `RWM_STORE_SACK_RELEASE` (default ON), which releases the SLOT and never the recoverability — the same goal achieved safely and battery-proven (c7 0.96–1.05×Σ) | **REMOVED 3dcb39c (2026-07-27)** — SR's first post-ship battery cycle closed (§16.25 + Consolidation); the precedence-warned control-arm role ended as scheduled |
| `RWM_RECOV_MP_SERIAL` | "Multipath Recovery Suppression (2026-07-21)": diagnosis vindicated (per-path loss 0.62–0.77 at a 0.1% cell), runtime refuted — honest signal re-heats every SRTT/loss cadence, sender CPU ×2.4 | **NONE** — refuted on the post-wall substrate itself (BBR default, MTU floor, path pool, suppression law live) | **NO** — the refutation IS the clean-substrate datum. A cheaper serial-namespace implementation would be a NEW build with its own item-11 pre-registration, not a re-test | **REMOVED ade48ad (2026-07-27)** — the diagnosis (per-path loss poisoning by global serials) stays documented at the module design note; a cheaper serial-namespace implementation is a NEW item-11 build |
| `RWM_INLINE_REPAIR` | "Repair In-Flight (2026-07-08)" (interspersed separate-grid inline repair): every inline config wedged or crawled; W=G reduction argument | W1, W7, PRE-DIV active, but the refutation's core is the GRID-STRANDING geometry (a separate inline grid strands its generation behind the frontier), which is substrate-independent | **NO on supersession grounds** — the goal (repair present at stall) was achieved by `RWM_PROACTIVE_PACER` ("Present-at-Stall"), whose own measured null resolved into the presence⊥throughput identity — re-confirmed post-divide as STRUCTURAL (Consolidated Verdict) | **REMOVED bede4a3 (2026-07-27)** — the negative result stays documented at the PROACTIVE_PACER site + here |
| `RWM_FRONTIER*` (`RWM_FRONTIER`,`_GAIN`,`_R`,`_OFFSET`) | "Proactive Frontier (2026-07-07)": rf=718 emitted, ru=4 useful — repair anchored at the ack frontier loses the race to its own ARQ retransmit | **W1, W7, W8, PRE-DIV** (earliest refutation in the register) — but the mechanism died on GEOMETRY (a trailing window anchored ½-RTT stale covers holes only after they stick), not on throughput | **NO on supersession grounds** — same successor chain as INLINE_REPAIR (`RWM_PROACTIVE_PACER` → presence⊥throughput structural); the single-path recovery-latency cell it targeted has been re-measured post-walls repeatedly (sc3 recovery ceiling 15.6–15.9) without this mechanism being the missing term | **REMOVED bede4a3 (2026-07-27)** — the FDIAG instrument (RWM_FDIAG) is retained; only the mechanism died |
| `RWM_RATE_WIRE` (+`RWM_RATE_Q`) | "Slow-Path Anchor Diagnosis STEP 3 (2026-07-13)": refuted LIVE, same-binary A/B, post-audit discipline | W1 (pre-BBR-lever, same day), PRE-DIV — but the refutation names the mechanism's own structural error: generation-mode rate samples are decode-clocked, so the windowed-MAX is near-correct and ANY sub-max quantile UNDER-reads and throttles | **NO** — a wall did not produce the verdict; the sample-clocking argument did. The rate-signal need was later met by the honest-anchor family (`RWM_PLAIN_RS`, rate-sample fix) | **REMOVED f1f32c5 (2026-07-27)** — effective_btlbw is now always the windowed-MAX (the shipped default, byte-identical) |
| `RWM_PLACE_SLACK` | "C8 Slow-Path Conversion (2026-08-06)": c7 protection clause FAILS ≫σ both seeds (145.6/151.0 = 0.858/0.896×Σ vs required ≥0.97); c8 never ≥ both incumbents; the smoke-falsified unbounded form was re-derived once (recovery-patience bound, D_i = min(S, 9/8·srtt_i)) per item-11's names-a-new-mechanism clause, then the battery falsified the bounded form too | **NONE** — refuted on the full current default stack (BBR + SR + PBS + MP + anchors + unified), same-session interleaved, instrumented | **NO** — the refutation is the clean-substrate datum AND carries the structural finding: slow-path source share is monotonically anti-correlated with c8 goodput across five arms (6%→88.6, 18%→70–83); the mechanism WORKED (placement reached capacity share, ~90% first-copy conversion) and the cell still paid more than it banked. Any future conversion ask must first refute the negative-margin table, not rebuild placement | **DEPRECATED 2026-08-06, default OFF** — law + 5 unit tests retained as the measured A/B arm until the next consolidation pass deletes it |
| `RWM_WIN_DECOUPLE` | "Window Decoupling + MTU Scaling (2026-08-06)": predictions 2–3 failed BOTH seeds (sc2 −1.76/−0.37 vs a +1.5…3 band; sc3 +0.09/+0.22 vs +0.8…1.6) with the law engaged exactly as derived (wd gauge live, echo RTT collapsed 108→27 / 520→230 ms) — the goodput never followed the queue | **NONE** — refuted on the full current default stack, same-session interleaved, both seeds, instrumented; the one defect found mid-battery (the paused-feed scope leak at N ≥ 2) was fixed and the duals re-measured TIE before any verdict | **NO** — the refutation carries three named mechanisms that supersede a re-test: (i) the §16.30 re-fire loop is re-serve-clocked (receiver re-advertise + per-seq cooldown), NOT queue-sustained — fired stays ×3.3–4.2 realized drops at a 27 ms echo; (ii) the 1024-latch's honest insurance value at sc2 is only ~0.4–1.8 Mbit (sub-sweep ack-granularity + drop-granularity cover), the PBH0 −20% cliff sits below ~256; (iii) the B1 jitter dwell is recovery-latency-owned — releasing the ceiling moves Copa −1.0…−1.4. A future re-ask starts from the composed datum (fix+mtu = best-ever sc3 16.86/16.84) and must attack the re-serve clock or the recovery dwell, not the window again | **DEPRECATED 2026-08-06, default OFF** — law + 4 unit tests + loopback retained as the measured A/B arm; the N1-scoped sampler pause (paused feed ≡ absent feed) retained as shared machinery |
| DAPS chain (`RWM_DAPS`,`_BDP`,`_PACE`,`RWM_PACE_ALL`,`RWM_RATE_SAMPLE`,`RWM_PER_PATH_EST`,`RWM_DAPS_DEPTH`) | §16.10–16.14 arc (2026-07-12) — VOIDED/UNCERTAIN by "Methodology Audit (2026-07-13)"; the LIVE refutation is "Gen-ON Stack Ablation (2026-07-13)": generation actually ON, rate-sample −22%, depth −17…−30% at sym C7 — the C7 collapse IS the stack; defaults flipped OFF there | original arc: **GEN-INERT (the defining case), W1, W2, W7, W8, PRE-DIV**. The live ablation: W1 (pre-BBR lever), PRE-DIV | YES formally, **LOW priority — argued honestly:** (i) the era verdicts were superseded by the live `Gen-ON Stack Ablation` on the SAME mechanism space, which is the re-test the register would otherwise order (its residual walls: W1/PRE-DIV); (ii) DAPS is generation-mode-only while the shipped default stack is plain-mode; (iii) `RWM_DAPS_DEPTH` retains its one live win (hetero C8 +8%) as a gen-mode opt-in. A deletion decision rides the next generation-mode consolidation battery (BBR substrate), not this plain-mode pass | **REMOVED 9b48286 (2026-07-27)** — VISION-TRIAGE ruling accepted (ADR-0065 §arguments 1–4): the live Gen-ON ablation already re-tested the mechanism space, every surviving idea is re-derived better (M* law / ADR-0061 anchors / percap family). The SHARED send-interval sampler (RsPacket, rs_on_sent/rs_on_delivered, on_src_sent/on_src_delivered_seq, charge_src/src_inflight, btlbw_sym_per_s) is RETAINED under the anchor-hygiene/CopaFeed family — only the DAPS-specific consumers died. A future gen-mode DAPS_DEPTH re-ask is a NEW item-11 build |

| STREAMING MACHINE (`fec/streaming.rs` + `streaming-codes`, the Realtime two-layer code) | NOT refuted — DISPLACED: "Unified Shedding + Flip Battery (2026-07-21)" flipped `RWM_UNIFIED` default ON after unified+shed beat streaming's p99 medians at every battery cell (c2/c3 × 400/1200 × both seeds), delivered 100% vs 79/81% at the c3 perf cell, with zero collapse reps | none relevant (displacement, not refutation; measured on the full current default stack) | **YES — the RE-TEST CLAUSE governs retirement**: the 12–48× message-tail crown record spans HISTORIC cells (L2/L3 message-tail batteries, quinn-vs-rp Metric A) this battery did not re-run; code removal requires a later pass holding that record cell-by-cell on the unified default | **RE-TESTED 2026-07-27 → CLEARED FOR RETIREMENT** ("Streaming Crown Re-Test", binary 2aac6b5f… ≡ 44dd7d4 Rust, seeds 42+7, per-rep interleaved `RWM_UNIFIED=0` vs ship): unified ≤ streaming p99 medians at ALL 5 historic crown cells × both seeds (10/10 cell-seeds, −1.2…−26.8 ms), p50 equal-class, delivery identical-complete (163/163 reps), bulk-hint inert. One recorded non-gating datum: cell-5 (L2 30-s shape) p999 MEDIANS favor streaming −6.7/−12.2 ms both seeds, deep sub-noise, worst-rep sign REVERSED (S 335 vs U 129) — the "cell-5 p999 WATCH", transfers to the deletion notes. ~~Deletion GO next consolidation pass~~ **REMOVED bccb32a (2026-07-27, Code Consolidation 2, scoped streaming-only)** — adapter + `streaming-codes` crate + selection glue deleted (−1,708 net LOC); `fec_backend streaming` is a parse error with a pointer; **OPT-OUT SEMANTICS CHANGE: `RWM_UNIFIED=0` + Realtime now selects the LEGACY-RLC windowed machine** (its own retirement clause, §17.5, stays open — NOT re-argued); the cell-5 p999 WATCH is HISTORICAL (a property of the deleted machine, measured and bounded above). Was: RETAINED as the live `RWM_UNIFIED=0` opt-out arm (no activation warning) |

Class-B gates (concept incomplete, successor named — `RWM_TAPER_R`,
`RWM_STORE_PERCAP`/`_GUARD`/`_HONEST_CAP`, `RWM_STORE_BORROW`,
`RWM_UNIFIED`, `RWM_COPA_COMPETE`) are NOT in this register: each is a
documented negative-or-partial result whose successor is scheduled in the
roadmap; they deprecate (or flip) when their successor's battery settles.
**[2026-07-21 update: `RWM_TAPER_R` and `RWM_UNIFIED` left Class B by
FLIPPING — both ship default ON under the unified umbrella ("Unified
Shedding + Flip Battery"); the streaming machine entered the register
above with its re-test clause.]**

## Code Consolidation (2026-07-27) — the register executed + the gate-block extraction (branch `refactor/consolidation`, base 769577c)

*Decision record: → [ADR-0066](adr/0066-deprecation-register.md) +
[VISION-TRIAGE-2026-07](adr/VISION-TRIAGE-2026-07.md) (both ruling sets
honored; per-row commits in the register table above).*

**What was deleted** (each its own commit, register/ADR section cited;
tests exercising only the deleted path deleted with it; shared machinery
mapped before cutting):

| chain | commit | net LOC | ruling honored |
|---|---|---|---|
| `RWM_SACK_PRUNE` | 3dcb39c | −108 | deprecate-HARD (structural unsafety; SR superseded, its post-ship cycle closed) |
| `RWM_RECOV_MP_SERIAL` | ade48ad | −30 | refuted ON the clean substrate; no re-test owed |
| `RWM_INLINE_REPAIR` + `RWM_FRONTIER*` | bede4a3 | −290 | refuted on geometry; superseded via PROACTIVE_PACER → presence⊥throughput; FDIAG instrument kept |
| `RWM_RATE_WIRE`/`RWM_RATE_Q` | f1f32c5 | −131 | refuted by its own sample-clocking argument |
| `RWM_SRC_BP` | 8902d24 | −209 | triage: re-test LOW, superseded by the percap family |
| DAPS chain (7 gates + scheduler surface + `tests/daps_loopback.rs`) | 9b48286 | −1168 | triage: mechanism space live-re-tested (Gen-ON ablation); surviving ideas re-derived better |

Total: **−1,936 LOC of refuted mechanism code** (the triage estimated
~1,000–1,500 for these chains). **The RsPacket boundary** (the work-list's
named hazard): the BBR send-interval sampler
(`RsPacket`/`rs_on_sent`/`rs_on_delivered` + the path-level
`on_src_sent`/`on_src_delivered_seq`/`charge_src`/`src_inflight` gauges and
`btlbw_sym_per_s`) is Anchor-Hygiene/CopaFeed property (ADR-0061 fix 3) and
was RETAINED; only the DAPS-specific consumers (placement branch, pace
buckets, depth budget, per-path-est attribution, the count-based
`on_src_delivered`) died. `RWM_FMTCP` (+ forced sub-levers
REASM_BDP/OOO_RETAIN/XPATH) NOT deleted — the register grants it the
piggybacked re-test arm on the c8-pool session. **[2026-07-27 later same
day: the re-test RAN ("C8-Aware Pool Law" battery) → CONFIRMED-REFUTED,
row updated above — the chain is now cleared for deletion at the next
consolidation pass.]**

**The extraction** (be878cc): the ~70-env-var inline gate block became
`src/gates.rs` — one `RuntimeGates` struct, resolved ONCE per engine start
in `run_impl`, passed to the receiver task / control fast-path /
`run_window_sender`; every gate documented in one place with default + ADR
pointer; deprecation warns fire in `resolve()`. Behavior-preserving: same
defaults, same parse/clamp rules, the order-sensitive chained defaults
(`RWM_GEN_PIPE`/`RWM_TAPER_R` ← `unified_active()`, `RWM_CC_PACE` ←
`copa_wire_active()`, the `RWM_ANCHOR_HYGIENE` umbrella) preserved exactly;
mode-dependent effective defaults stay at the use site as raw `Option`
overrides (incl. the `RWM_STORE` set-but-unparsable subtlety,
`store_env_set`). Scar cleanup 04d2517 (comments only).

**LOC accounting**: `net/mod.rs` 12,696 → 11,510 (−1,186); `scheduler/mod.rs`
5,226 → 4,473 (−753); `gates.rs` +319 new; whole-repo diff vs 769577c ≈
−1,700 net.

**Suites** (every commit boundary + final tree): `cargo test -p raptorpath
--lib` 364/364 (376 baseline − 13 deleted-path tests + 1 new gates test);
`raptorpath-math` 59+25; `gate_suite --release` 15/15 (twice: post-deletions
and post-extraction); `mtu_blackhole_wedge`, `perf_loopback` 8/8,
`fmtcp_loopback`, `recov_mp_loopback`, `copa_sole_loopback`, `backpressure`,
`unified_stream_l0 --ignored --release` all green.

**L1 identity smoke** (VM 10.1.5.16, lock held 11:21–11:31 UTC, binary
sha256 b04bc50f8e0b… = commit 04d2517 built fresh on the VM (old binary
rm'd first), E5-2650 v3 aes+avx2+pclmulqdq (post-divide), seed 42, default
env (`SEED=42 RWM_GEN=0 RWM_DIAG=1`), ×4 per cell, liveness echoes
asserted (sr=1 every rep; pbs=1+mp=1 at c7)): **sc2 85.49/85.11/85.02/85.83
Mbit/s (known class ~84–85); c7 168.54/165.83/165.37/163.15 (known class
~163–169); 0 DNF.** The shipped default is unchanged by the pass — the
deleted code was all default-OFF/dead, and the extraction is a refactor.

## Code Consolidation 2 (2026-07-27) — the register's two re-test-cleared rows executed (branch `refactor/consolidation-2`, base b81f5c3)

*Decision record: → [ADR-0064](adr/0064-unified-span-machine.md) +
[ADR-0066](adr/0066-deprecation-register.md) +
[VISION-TRIAGE-2026-07](adr/VISION-TRIAGE-2026-07.md) §3 (FMTCP) / §4
(streaming stage 2). Both deletions were gated on same-day-earlier
re-tests: FMTCP's "C8-Aware Pool Law" piggyback arm
(CONFIRMED-REFUTED) and the "Streaming Crown Re-Test" (CLEARED). The
register is now FULLY EXECUTED — no Class-C gate remains in the tree.*

**What was deleted** (each its own commit citing its register row +
re-test numbers; tests exclusive to the deleted path deleted with it;
shared machinery mapped before cutting):

| chain | commit | net LOC | ruling honored |
|---|---|---|---|
| `RWM_FMTCP` + `RWM_FMTCP_WIN` (composite wiring: fmtcp_tx_paused total-in-flight FC, win backstop static+derived, window_decoded_seq publish/read chain, forced `\|\| fmtcp` couplings on REASM_BDP/OOO_RETAIN/XPATH/react_cap/infl_bdp/gen_r, DIAG fields, fmtcp_loopback.rs, harness pass-through) | f841757 | −264 | RE-TESTED 2026-07-27 → CONFIRMED-REFUTED (c7 18.30/18.98, c8 14.30/15.03 = ×0.11–0.20 of the default stack, both seeds ≫σ, on the FULL clean substrate) |
| Streaming machine (`fec/streaming.rs` 352 + `streaming-codes` crate 845 + selection glue + `FecBackend::Streaming` variant + streaming_code_test.rs + bench/test arms + tail_matrix `streaming`/`bulkstream` arms retired loudly) | bccb32a | −1,708 | RE-TESTED 2026-07-27 → CLEARED (crown held 10/10 cell-seeds); **scoped streaming-only** — the legacy-RLC machines and the differential `reference` decoder are NOT in scope (their §17.5 clause was never re-argued) |

Total: **−1,972 net LOC** (211 insertions, 2,183 deletions vs base
b81f5c3). `net/mod.rs` 11,965 → 11,775 (−190); `gates.rs` 368 → 357 (the
`fmtcp`/`fmtcp_win` fields and the last `deprecated_env_flag` caller
gone — the helper itself stays, CORE gate-hygiene layer per the triage).

**Shared-machinery boundary** (the work-list's named hazard, honored):
`fmtcp_percap_full` → renamed `infl_percap_full` and RETAINED — the
gen_pipe stack (remedy 1) consumes it and `percap_store_full` mirrors it
(percap family = EXPERIMENT-KEEP); the WindowAck `cumulative_received`
wire field STAYS (wire format + debug datum; only FMTCP's sender-side FC
consumer died); the M* anchor machinery keeps its plain-live subset (the
mstar echo drops its now-dead "derived win backstop" clause). Historic
battery drivers (`c8pool_*.sh`, `crown_*.sh`) keep their fmtcp/streaming
arms as the record of the executed re-tests.

**The `RWM_UNIFIED=0` OPT-OUT SEMANTICS CHANGE, stated explicitly:**
after bccb32a the realtime path under `RWM_UNIFIED=0` can no longer
select streaming — it falls back to the LEGACY-RLC windowed machine
(`RlcWindowDecoder`; echo: "Realtime mode (RWM_UNIFIED=0): streaming
machine retired — riding the legacy-RLC windowed machine"). The
legacy-RLC machines stay: their retirement clause ("unified ≥ legacy-RLC
everywhere", §17.5, the flip battery's c3-1200B sign-flip class
unresolved) was never argued and remains open. `fec_backend streaming`
is a parse error with a pointer. The cell-5 p999 WATCH becomes
HISTORICAL — it described the deleted machine (sub-noise p999-median
edge, worst-rep sign reversed), measured and bounded in "Streaming Crown
Re-Test".

**Suites** (every commit boundary + final tree): lib 362/362 (368
baseline − 2 FMTCP-only − 4 streaming-adapter tests); raptorpath-math
all 8 result sets green; `gate_suite --release` 15/15 (twice — once per
deletion boundary); `mtu_blackhole_wedge`, `perf_loopback` 8/8,
`recov_mp_loopback`, `copa_sole_loopback`, `backpressure`,
`emit_batch_loopback`, `sim_backend`/`fec_waterfall`/`config`/
`fec_backend_switching`/`fec_window` tests, `unified_stream_l0
--ignored --release` — all green; all test+bench targets compile
(gates.rs default-stack test updated: fmtcp fields gone).

**L1 identity smoke** (VM 10.1.5.16, lock held 22:45–22:56 UTC
2026-07-27 (found FREE), binary sha256 01001268fee6… = commit bccb32a
built fresh on the VM (stale binary rm'd first), E5-2650 v3
aes+avx2+pclmulqdq, seed 42, default env (`SEED=42 RWM_GEN=0
RWM_DIAG=1`), ×4 per cell, liveness echoes asserted (sr=1 every rep;
pbs=1+mp=1 at c7)): **sc2 84.12/84.70/85.57/85.05 Mbit/s (known class
~84–85.5); c7 165.36/168.57/166.98/164.69 (known class ~163–169); 0
DNF.** Tail crown spot (tail_matrix c2 `ship` ×4, the crown classes):
**400B p99 35.0/35.1/35.2/40.5 med 35 (class ~35–42); 1200B p99
36.2/39.7/40.9/43.0 med 40; delivered 1000/1000 EVERY rep; p50 7.9–8.3
ms; full unified echo set at both endpoints.** The shipped default is
untouched by both deletions — FMTCP was default-OFF/deprecated-warned,
and streaming was reachable only through the `RWM_UNIFIED=0` opt-out.
VM left clean (no processes, no netns, lock released 22:56 UTC; logs
`/home/vibe/consol2/smoke-{bulk,tail}.log`).

## Window Decoupling + MTU Scaling (2026-08-06) — PRE-REGISTRATION (discipline item 11 — this block written and committed BEFORE any build and BEFORE any VM run; branch `feat/window-mtu` from 6e59a11; ARC A item 2 — the lossy-singles STRUCTURAL terms, carrying three fronts: (i) the c2/c3 gap vs quinn-bbr (91.9/18.6), (ii) the B1 jitter-cell Copa dwell ceiling (1024-store Little's law ≈ 36 Mbit), (iii) c8-via-c2 (the residual c8 gap ≡ the single-path c2 gap, "C8 Slow-Path Conversion"))

*Decision record context: → goal-gate "Lossy-Single Residual" (§16.30 — the
CLOSED accounting this section executes: framing tax ~4.3/0.95 Mbit +
spurious retx ×5.7 from the queue-sustained re-fire loop; the 1024-latch is
ALSO the stall insurance — the honest-size static window idled the wire
12%), "Adversarial Cells (B1)" (the jitter-cell dwell attribution),
"Per-Path Outstanding Accounting" HONEST-CAP (the cap-law derivation
template), ADR-0055 (the MTU floor), ADR-0061 (anchor hygiene; the
`RWM_PLAIN_RS` c7 −22…−27 composition price, "C8-Aware Pool Law"
ATTRIBUTION).*

The two parts are pre-registered SEPARATELY and stand or fall
independently; the battery carries each part's own arms plus one composed
arm at the singles.

### PART 1 pre-registration — window/inflight decoupling (`RWM_WIN_DECOUPLE`, default OFF)

**(a) The problem, precisely.** The 1024-slot outstanding latch
(`RELIABLE_STORE_MAX`, reached because the legacy plain anchor over-reads
×4.6–7.5 so the `gain·Σanchor` dyn cap always clamps) is simultaneously:
(1) the spurious-retx re-fire queue — its standing queue (echo RTT 109–111
ms at sc2 vs RTprop 13; 528–558 ms at sc3 vs ~45) ages every hole past the
9/8·SRTT law so re-fires are LEGAL and sustained (fired ×5.0–5.7 realized
drops; `RWM_RECOV_SP` could only trim −24…−31%); (2) the only stall
insurance — the honest-size static window idles the wire (sc3-s384: 12%
idle, goodput 14.77 vs 16.13); (3) the B1 jitter-cell dwell ceiling —
under Copa-sole at 40 ms RTprop cells the 1024 outstanding × the measured
~250–350 ms dwell is a Little's-law ceiling ≈ 36 Mbit, and Copa sits AT it
(0.38× BBR-under at ZERO jitter — a CC×store interaction, not a delay-law
failure).

**(b) Diagnosis FIRST (one instrumented run each way; instruments
DIAG-gated and behavior-inert).** Name the insurance term instead of
inheriting it: what EXACTLY does the honest-size window stall on?
New [DIAG] gauges (sender): `wnd2=` wire-head outstanding split —
`head=<last_sent − release_frontier>` (the live head span) vs
`hole=<unSACKed total − head>` (recovery-stalled seqs below the SACK
frontier), `relgap=<max ms since the SACK/cum frontier last advanced>`
(release clumping), plus the existing sidle/paused/win gauges. Arms (seed
42, ×2 each, sc3 25 MB AND sc2 100 MB): default (1024 latch) ↔ honest-size
static (`RWM_STORE=384` at sc3 — the known 12%-idle arm; `RWM_STORE=256`
at sc2 — the arm the July flake class lost). Decision rule, fixed now: the
insurance term is named by which gauge saturates in the static arm's
sidle-gap windows — (D1) HOLE-PINNING: `hole=` ≥ ~⅓ of the window with
oldest-age ≫ SRTT (holes eat the budget; fresh admission starves);
(D2) RELEASE CLUMPING: `relgap=` clumps at the [25,100] ms sweep cadence
with `head=` pinned at cap (the budget starves between SACK refreshes);
(D3) MULTI-ROUND TAIL: hole ages cluster at multiples of (R + RTprop)
(GE re-kill; insurance = hole capacity for N rounds, not 1).

**(c) The law family (pre-registered; the exact constants finalized by an
AMENDMENT appended after the diagnosis and BEFORE the battery — the
PLACE_SLACK amendment pattern).** Decouple the three roles into O(1)
gauges with every term measured or a named engine constant:

    wire_out  = last_sent_seq − release_frontier          (holes EXCLUDED)
    allow     = anchor·(K + gain − 1) + rate·min(stall_age, R_ins)
    admission pauses when wire_out ≥ allow
                OR unSACKed_total ≥ cap_ret               (retention backstop)
    cap_ret   = anchor·(K + gain − 1) + rate·(R_ins + N_hole·(R + RTprop))
                clamped [floor 64, WIN_STORE_MAX 4096]

- `anchor`/`rate`/`RTprop`/`K` are the HONEST-CAP terms (`honest_store_cap`
  derivation: anchor = BtlBw·RTprop windowed honest, K = windowed-min
  echoSRTT/RTprop, R = `HONEST_RECOVERY_ROUND_S` = 100 ms, gain = 2.0).
- `stall_age` = now − (last SACK/cum frontier advance): the stall-insurance
  term made EXPLICIT and CONTINUOUS (no mode bit): during steady flow it is
  ~ack-interarrival (allowance ≈ residence + headroom, standing queue
  ≈ (K+gain−2)·anchor ≈ 1 BDP-class); during a frontier freeze the
  allowance grows at exactly the anchor rate, so the wire stays fed through
  a recovery round WITHOUT a permanent standing queue. `R_ins` (expected =
  R; the diagnosis may name D3 multiples) caps it.
- Holes live against RETENTION (`cap_ret`), not the wire budget — the D1
  channel is structural in the law; `N_hole` (expected 1–4 recovery rounds
  of hole capacity) is priced by the diagnosis hole-age tail (p95 age /
  (R + RTprop)).
- Scope: plain in-order reliable window, N = 1 live path ONLY (N ≥ 2 keeps
  the configured pooled laws bit-exactly — the c7/c8 pool battle is
  settled separately and stays untouched). Under Copa-sole (`owns_cc`) the
  residence term is `gain·Σcwnd` (Copa's honest pipe) and the clamp
  ceiling lifts from the 1024 latch to `cap_ret` — the B1 dwell-ceiling
  release. Warm-up (no anchor) keeps the legacy boot/latch path.
- Anchor feeding at N = 1 (the trustworthy-anchor requirement WITHOUT the
  PLAIN_RS c7 composition price −22…−27 ≫σ, which was measured at
  c7/N = 2): the sampling-only CopaFeed engages under this gate at N = 1
  ONLY — its sample-recording sites dynamically no-op while
  `live_paths() ≥ 2` (the c7 arm must carry the sampler-inert gauge).

**(d) Predictions (pre-registered).**
1. MECHANISM: sc2 echo RTT 109–111 → ≤ ~30 ms class; sc3 528–558 → ≤ ~150
   ms; fired/realized-drops collapses ×5.0–5.7 → ≤ ~×1.5 at BOTH cells
   WITHOUT `RWM_RECOV_SP` (the re-fire loop was queue-sustained; remove
   the queue and holes recover inside the law's threshold).
2. sc2 100 MB: **+1.5 to +3** (→ ~86.5–88, toward the 91.9 bar; the ~2.7
   Mbit spurious-retx wire term reclaimed as source on a full wire), ≫ σ_s
   (~0.7–1.0), both seeds.
3. sc3 25 MB: **+0.8 to +1.6** (→ ~17.0–17.7, the tcp-bbr class, toward
   18.6), ≫ σ_s (~0.1–0.2), both seeds; wire utilization stays ≥ ~98%
   (the s384 12%-idle class MUST NOT appear); ≥ the RWM_RECOV_SP arm's
   +0.32–0.35 (subsumption bar — see (g)).
4. B1 jitter cross-check (jit5/jit15, Copa-sole arms, same-session
   BBR-under reference): the `win=1024/1024` pin disappears (gauge), and
   IF the dwell ceiling was the binder, Copa lifts from 0.32–0.36× to
   ≥ ~0.7× BBR-under. A Copa that stays ~0.35× with the window unpinned
   REFUTES the store-ceiling share of the B1 attribution and isolates the
   empty-pipe recovery-stall share — attribution-bearing either way, NOT
   flip-gating.
5. c7 ≥ 0.97×Σ and c8 ≥ the 0.87 line (legacy-arm class) — the N = 1 scope
   makes the fix arm bit-identical at duals (echo may print; the law and
   the sampler must be gauge-inert), so Δ within σ.
6. dnf = 0 everywhere; tail_matrix c2 spot ×4 unregressed (shared reliable
   plane).

**(e) Falsification (fixed now).** (1) Either single cell regressing ≫σ on
both seeds ⇒ the law is refuted → default OFF, register row with the gauge
state (which budget starved), NO tuning pass. (2) fired stays ≥ ×3 with
the queue measurably gone (echo ≈ RTprop class) ⇒ the queue-sustained
attribution of §16.30 is WRONG — the re-fire loop has another owner;
record it, register. (3) sc3 wire idle ≥ 5% with the insurance term live
⇒ the insurance derivation missed the real stall — register with the
diagnosis gauges. (4) Goodput flat at BOTH cells with mechanism gauges all
confirming (queue gone, fired collapsed, wire full) ⇒ the freed wire went
to margin, not goodput — the §16.30 spurious-retx pricing is refuted as a
GOODPUT term; register, no flip on a wrong attribution. (5) c7/c8 moved
≫σ ⇒ scope defect (a bug, not a result): fix before any verdict.

**(f) Derivation re-read — self-contained failure predictions.** (1) The
sc2 risk class is known and bounded: PBH0 (cap ≈ 150–175, no K/R) lost
−18…−22% at sc2 — my residence-only base (anchor·(K+1) ≈ 190) sits just
above that class, so the stall-metered insurance term is LOAD-BEARING at
sc2; if BBR's probe/ack-aggregation gaps are not what the diagnosis says
they are, sc2 regresses toward that class and falsification (1) fires.
(2) At sc2 the wire is already 98.4% full — the +1.5…+3 prediction
requires reclaimed spurious wire to CONVERT, which the RECOV_SP battery
showed is NOT automatic at sc2 (its freed 1.1 MB vanished into margin);
the decoupling differs by ALSO removing the queue (recovery latency, not
just wire waste), which is why the prediction stays positive — but
falsification (4) prices the honest failure. (3) The frontier gauge
`wire_out` under-counts in-flight retransmits (they fly below the
frontier): ≤ ~realized-drop-count symbols, ≪ σ of the budget — accepted.
(4) Under heavy reorder (jit cells) the SACK frontier runs ahead of
in-flight seqs, widening the effective window by ~the reorder depth — the
SAFE direction (more insurance), bounded by cap_ret. (5) A receiver whose
SACK advertisement is itself clumped (GRO ~13-datagram batches) feeds K;
K's windowed-min is self-queue-proof — no positive feedback handle
(HONEST-CAP battery, measured). (6) The B1 Copa ceiling-lift risk: Copa's
own cwnd law was measured SANE on jitter cells (btlbw ≈ 1× link at
shal8); lifting the retention clamp cannot push Copa's cwnd — only stop
truncating it; regression channel is memory only, bounded at 4096 × 1.2
KB ≈ 5 MB.

**(g) Relation to `RWM_RECOV_SP` (ships OFF at sc3 +0.32–0.35).** The
decoupling removes the standing queue that makes young re-fires LEGAL
(holes aged past 9/8·SRTT by queue dwell alone). If prediction 1 holds
(fired ≤ ~×1.5 WITHOUT recov_sp), the decoupling SUBSUMES the RECOV_SP
lever (nothing left to suppress) and the ledger records the relation;
RECOV_SP remains a default-OFF measured arm in either case (it never
flipped — no register row owed).

**(h) Flip rule (fixed before the battery).** `RWM_WIN_DECOUPLE` flips
default ON only if predictions 1–3 hold on BOTH seeds AND c7/c8 hold
their lines (5) AND the tail spot is unregressed AND suites stay green.
Prediction 4 (B1 Copa) is attribution, not a gate.

### PART 2 pre-registration — MTU/payload scaling (the ~4.3/0.95 Mbit framing term; `RWM_WIRE_COMPACT`, default OFF)

**(a) The derivation (done BEFORE choosing what to build — the wire
arithmetic, every number measured or read from the code).** The framing
tax: rp puts 1200 payload B on ~1319 wire B (qdisc-measured mean, sc2
diagnosis) = 0.910 efficiency vs quinn MTUD ~0.957. The per-symbol wire
overhead is **119 B, ALL FIXED, none per-byte**: 28 IP+UDP, ~26 QUIC
1-RTT (short header + CID + PN + AEAD tag + DATAGRAM frame), and **65 B
of rp's own framing** — 8 magic+version + 57 bincode-fixint (4 enum tag +
8 Vec len + WireSymbol{8 block_id + 4 payload_id + 1 is_repair + 8 data
len + 4 backend} + 8 send_ts + 8 batch_seq + 4 path_id); repair symbols
add 14 B INSIDE the payload (span header — load-bearing, untouched).
The three candidate levers, priced:
- **(A) Fill the 1350 floor**: the floor guarantees ~1317 B datagrams;
  the worst-case (repair-batch) symbol datagram is 1279 → payload can
  rise only 1200 → ~1212. Efficiency 0.9098 → 0.9106 = **+0.1 Mbit at
  c2** — BELOW the session noise floor. REFUTED BY DERIVATION; not
  built (discipline 11d).
- **(B) MTUD-style payload scaling above the floor** (symbol sized to the
  verified path MTU ~1452): payload → ~1340, efficiency → 0.9184 =
  **+0.9 at c2 / +0.2 at c3** — real but ~1σ; AND a symbol sized above
  the floor is exactly the wedge geometry (a black-hole reset to the
  1350 floor makes every data send TooLarge for the 60 s cooldown)
  unless the floor RISES with the payload, which trades away 1500−MTU
  external validity (PPPoE-class paths), or symbols re-size mid-stream
  (protocol surgery across the span law's symbol units, G, and the
  store accounting). Named for the roadmap with its price; NOT built
  this session.
- **(C) The framing diet (BUILT — the term the derivation actually
  names)**: the recoverable tax is rp's own 65 B/pkt, not the MTU. A
  compact DATA wire frame (v5-compact, env `RWM_WIRE_COMPACT` sender-
  gated A/B): tag byte (∉ 'R' — classified against the legacy magic
  unambiguously) + flags(is_repair|backend) + varint path_id/seq/
  payload_id/batch_seq/send_ts + payload = REST OF DATAGRAM (the QUIC
  datagram boundary IS the length — both 8-B bincode length fields
  deleted). ~14–16 B vs 65 ⇒ overhead 119 → ~69 ⇒ efficiency 1200/1269
  = **0.9456** — recovers ~3.4–3.9 of the 4.3 Mbit c2 term and ~0.6–0.8
  of the 0.95 c3 term with NO MTU change, NO symbol-size change (zero
  interaction with G / span units / message packing), NO wedge exposure
  (datagrams get SMALLER: 1279 → ~1228). Receive support is
  unconditional (a non-'R' first byte is a parse ERROR today — compact
  parsing converts dead space, byte-identical for all legacy traffic);
  PROTOCOL_VERSION bumps 4 → 5 so pre-compact binaries refuse cleanly
  at handshake instead of dying mid-stream. Control/ack framing
  unchanged (out of scope; priced separately in §16.30 term 3).

**(b) Predictions (pre-registered).**
1. WIRE TRUTH (mechanism gauge, qdisc bytes/pkts): mean data-pkt overhead
   119 → ≤ ~75 B; framing efficiency 0.910 → ≥ 0.94 at both cells.
2. sc2: **+2.5 to +4** (→ ~87.5–89), ≫ σ_s, both seeds.
3. sc3: **+0.5 to +0.9** (→ ~16.6–17.0), ≫ σ_s (~0.1–0.2), both seeds.
4. c7/c8: lift-or-hold (the same framing rides both paths; no regression
   ≫σ). Composed fix+mtu at singles: ≈ additive within σ.
5. Crown gate (MANDATORY): tail_matrix c2 spot ×4 — p99 medians within
   the historic class (~36–48 ms), 1000/1000 delivered, both arms.
6. dnf = 0; `mtu_blackhole_wedge` stays green; env-unset tree
   byte-identical on the wire (legacy serializer verbatim).

**(c) Falsification.** Overhead does NOT drop ≤ ~80 B ⇒ the serializer
misses the hot path (mechanism defect — fix or withdraw, no verdict).
Goodput flat with the overhead gauge collapsed ⇒ the freed wire did not
convert at that cell — the §16.30 framing-tax pricing is refuted AS A
GOODPUT TERM at that cell; register row, no flip on wrong attribution.
Crown spot regressed ⇒ no flip regardless of throughput.

**(d) Derivation re-read — failure predictions.** (1) GSO/GRO are
byte-transparent — segment sizes change ~4%, no kernel-batching cliff
predicted. (2) The tag byte must never collide with legacy classification:
legacy starts 'R' (0x52); compact tag 0xC1 — disjoint by construction;
handshake rides a STREAM (never datagrams) — unambiguous. (3)
MAX_SYMBOLS_PER_BATCH guard: compact = exactly one symbol/frame by
construction; multi-symbol batches (block mode) keep legacy framing —
scope is the window-mode one-symbol datagram path. (4) At sc2 the wire is
full: +3.9% wire efficiency converts ~1:1 ONLY if loss/recovery waste is
rate-independent — the retx share rides the same framing, so the gain is
on ALL sent bytes; conversion is arithmetic, not behavioral — this is the
strongest-prior prediction of the session. (5) Realtime symbols (512 B)
carry proportionally MORE fixed overhead — compact helps the tunnels or
is neutral; the crown spot is the gate.

**(e) Flip rule.** `RWM_WIRE_COMPACT` flips default ON only if
predictions 1–3 + 5 + 6 hold on both seeds and c7/c8 unregressed; else
OFF with the register row.

### BATTERY (pre-registered; one session, arms per part evaluated INDEPENDENTLY)

VM 10.1.5.16 per MEASUREMENT DISCIPLINE 1–12: lock `/tmp/rwm-vm.lock`
taken 2026-08-06 17:25:55 UTC (found FREE; covers builds + probes +
battery); tree synced via git archive of THIS branch + CRLF conversion
before the first harness invocation; stale binary removed before every
build; binary sha256 + commit + lscpu + kernel in every log header;
FOREGROUND polling only; rp-* netns only; fresh topology per invocation;
seed-7 topo-abort protocol (n recorded, nothing discarded); logs
preserved under `/home/vibe/winmtu/`. Driver `tools/l1/winmtu_*.sh`:
- DIAGNOSIS (part 1): sc3 {def, RWM_STORE=384} + sc2 {def, RWM_STORE=256}
  ×2, seed 42, RWM_DIAG=1 — the insurance-term naming runs; amendment
  appended to this section BEFORE the battery.
- SINGLES (PRIMARY): sc2 100 MB + sc3 25 MB × arms def ↔ fix
  (`RWM_WIN_DECOUPLE=1`) ↔ mtu (`RWM_WIRE_COMPACT=1`) ↔ both,
  interleaved round-robin per rep ×8, seeds 42+7; bars: quinn-bbr
  91.9/18.6 ("Competitive Baseline", same cells/seeds).
- DUALS (no-regression): c7 200 MB + c8 25 MB × def ↔ fix ↔ mtu ×8,
  seeds 42+7; same-session Σ from the singles arms (Σ_c7 = 2×sc2,
  Σ_c8 = sc2+sc3, per arm env); c7 ≥ 0.97×Σ clause, c8 vs the 0.87 line.
- B1 JITTER CROSS-CHECK: jit5 + jit15 (adv_cells.sh recipes verbatim) ×
  arms bbr-def ↔ copa-def ↔ copa-fix ×5, seeds 42+7 (the same-session
  BBR-under reference the ratio needs).
- CROWN: tail_matrix c2 spot ×4, seed 42, arms def + mtu (+ both iff
  both parts pass singles).
- Liveness echoes asserted per arm both directions; ARMCOUNT per arm;
  runtimes stated; aborts preserved.

*(The diagnosis results, the amendment, and the battery results below
this line were written AFTER the respective runs.)*

### PART 1 DIAGNOSIS RESULTS (VM 10.1.5.16, 2026-08-06 17:53:26–17:55:12 UTC; binary sha256 1306bea40182… = commit 7ebba0e, built fresh (stale rm'd, CRLF-converted); E5-2650 v3; kernel 7.0.14-101.fc43; seed 42 ×2/arm, 8/8 clean, 0 retries; driver `tools/l1/winmtu_diag.sh`; log `/home/vibe/winmtu/diagnose-s42.log` + full per-run DIAG series under `diag/`)

| arm | goodput (Mbit/s) | echo rtt | fired (y) | wnd2 hole max | relgap mx steady | qdisc sent / pkt / drop | wire util |
|---|---|---|---|---|---|---|---|
| sc3-def (1024 latch) | 16.25 / 15.87 | **554–569 ms** | 2270–2317 (×4.6 drops; y 60–66%) | 22–61 | 3–10 ms | 30.5–30.6 MB / 26.9–27.9 k / 497–502 | ~99% |
| sc3-s384 (honest-size static) | **16.22 / 16.43 — TIE** | **194–209 ms** | 1669–1810 (×3.4; y 86–96%) | 2–38 | 10–12 ms (one 101 ms at the drain tail) | 29.9–30.0 MB / 26.6–27.6 k / 498–514 | ~99% |
| sc2-def | 85.54 / 85.25 | **104–109 ms** | 3121–3156 (y ~74%) | 1–70 | 2–4 ms | 115.93 MB / 86.3–86.4 k / 325–337 | **99.2%** |
| sc2-s256 | 84.53 / 84.32 (**−1.0**) | **23–27 ms** | 2669–2987 (y ~55–62%) | 0–20 | 2–4 ms | 115.99 MB / 94.9 k / 556 | **98.0%** |

**The D-rule verdicts (the pre-registered decision rule, applied):**
- **D1 (hole-pinning): REFUTED.** Holes never exceed ~70 of a 256–1024
  window (≤ 7%) in any arm at either cell — recovery-stalled seqs do NOT
  eat the budget.
- **D2 (release clumping at the [25,100] ms sweep scale): REFUTED.**
  relgap-max stays 2–12 ms through steady transfer in every arm (the
  per-symbol in-order acks + 2 ms gap-ack cadence keep the frontier
  moving); the sweep-cadence starvation the derivation feared does not
  occur.
- **D3 (multi-round tail): NOT BINDING** (the hole population is too
  small to need multi-round capacity; N_hole = 1 suffices).
- **The insurance term, NAMED (none of the pre-registered three): SUB-
  SWEEP ACK-GRANULARITY COVER.** At sc2 the honest-size window's whole
  cost is **~1.2% of wire time** (util 99.2% → 98.0% at EQUAL wire
  bytes ⇒ the −1.0 Mbit), spread over sub-3-ms micro-stalls (invisible
  to the ≥3 ms sidle gauge; relgap mx 2–4 ms): a static window is
  consumed by its own queue (Little's law — win 256 ⇒ echo 25 ms ⇒
  residence ≈ 256: ZERO slack), so every ack-clock hiccup beyond the
  pacing smoothing idles the wire. The 1024-latch buys those ~190 ms/run
  by never being the binder. The law's stall-metered term is EXACTLY
  this cover, made continuous: during any frontier freeze of g ms the
  allowance grows rate·g (micro or sweep scale alike), and resets when
  the frontier jumps.
- **Session datum vs the July record:** the sc3-s384 "12% idle /
  14.77" datum did NOT reproduce (16.22/16.43 = tie with def) — that
  was one run in the flake-class diagnosis session; the idle-insurance
  story at sc3 is SESSION-DEPENDENT at most. And the queue-shrink arms
  cut fired only −15…−25% with y-fires persisting at 23–27 ms echo
  (×3.4–4 fired/drops) — **the re-fire loop is only PARTLY queue-
  sustained**, and the freed retx wire did not convert to goodput in
  either static arm. Predictions 2–3 (sc2 +1.5…3, sc3 +0.8…1.6) are
  therefore AT RISK by this diagnosis's own evidence; they stand
  unchanged as the falsifiable bet, and falsification clause (4) (freed
  wire → margin, not goodput) is the expected failure mode if they
  fail. The B1 Copa-ceiling half of the law (prediction 4) is untouched
  by this risk — the jitter cells need the ceiling RAISED, the
  opposite direction.

**AMENDMENT — constants fixed for the build (BEFORE the build; no other
change to the pre-registered law):** R_ins = R = `HONEST_RECOVERY_ROUND_S`
(100 ms, the sweep-cadence clamp — the same named constant); N_hole = 1
(from D3); residence/K/gain verbatim from `honest_store_cap`; memory
backstop `WIN_STORE_MAX` = 4096 (~5 MB); under Copa-sole (`owns_cc`) the
residence term is gain·Σcwnd (Copa's own honest pipe, un-truncated) with
the same stall meter and retention backstop — the B1 ceiling release.

*(Battery results below this line were written after the runs.)*

### L1 BATTERY RESULTS (VM 10.1.5.16, 2026-08-06 18:46–20:16 UTC; E5-2650 v3 aes+avx2+pclmulqdq, kernel 7.0.14-101.fc43 in every log header; seeds 42 AND 7, arms interleaved round-robin per rep, fresh topology per invocation, 1 run/invocation, RWM_GEN=0 RWM_DIAG=1 everywhere; drivers `tools/l1/winmtu_{battery,jit,phase2}.sh` + `tail_matrix.sh`; logs + per-run diag preserved under `/home/vibe/winmtu/`; binaries: A = sha256 335e07f1… (commit 3e7f43a, built fresh, stale rm'd, CRLF-converted) for the s42+s7 batteries; B = 0e4f5cde… (commit 44fe5aa = A + the scope fix below) for the dual re-run, the jitter cross-check, and the crown tails; runtimes: s42 battery 18:46:31–19:10 (24 min, 112/112 clean, 0 retries), s7 19:10–19:35 (25 min, 106 completed + the seed-7 flake class: RUN-RETRY recovered, RUN-LOST sc2-def 1 / sc2-fix 1 / sc3-def 2 / c7-fix 1 / c7-mtu 1, n quoted), phase-2 build 19:37–19:41, redual 19:41–19:51, jit 19:51–19:59, tails 19:59–20:16; dnf = 0 in every completed run of every battery; liveness echoes asserted per arm both directions, 0 ARM-LIVENESS-FAIL / 0 ARM-CONTAMINATION on captured runs)

**INCIDENT, recorded first — the falsification-(5) scope defect, found,
fixed, re-measured.** On binary A the s42 duals read c7-fix 64.05 ± 0.39
(vs def 166.35) — the pre-registered clause "c7/c8 moved ≫σ ⇒ scope
defect (a bug, not a result)" fired. Gauge forensics named it exactly:
with the N1-scoped feed PRESENT-BUT-PAUSED at N ≥ 2, (a) `charge_src`/
`on_src_sent` still ran per send while attribution was paused —
src_inflight leaked to ~165 k; (b) the per-batch Ack arm suppressed the
legacy `record_delivery` anchor feed while the paused feed supplied no
samples either — btlbw=0/est=n on BOTH paths, the dyn cap stuck at the
128 boot value for the whole transfer. Fix (commit 2ea195f): every
feed-conditional site filters on `!n1_paused()` (a paused feed ≡ absent
feed), and the feed starts PAUSED under `RWM_WIN_DECOUPLE` so a dual
bring-up never charges a symbol. The dual re-run on binary B (redual,
def2 ↔ fix2 interleaved ×8 both seeds): **c7 fix2 166.40 ± 2.13 /
165.45 ± 1.86 vs def2 166.38 ± 0.96 / 165.71 ± 2.24 (7) — TIE; c8 fix2
75.20 ± 17.44 / 71.78 ± 11.33 (7) vs def2 76.54 ± 8.86 / 70.16 ± 15.37 —
TIE within the cell's episodic σ.** Prediction 5 (dual inertness) HOLDS
on the fixed binary. Singles are UNAFFECTED (at N = 1 the pause never
engages; code path identical).

**Goodput, singles (mean ± σ_s (n); bars = quinn-bbr 91.9 / 18.6,
"Competitive Baseline", same cells/seeds):**

| cell | arm | s42 | s7 |
|---|---|---|---|
| sc2 (c2 single 100 MB) | def | 85.17 ± 0.78 (8) | 84.48 ± 0.82 (7) |
| | fix (`RWM_WIN_DECOUPLE`) | 83.41 ± 0.85 (8) **−1.76** | 84.11 ± 0.77 (7) −0.37 |
| | **mtu (`RWM_WIRE_COMPACT`)** | **87.76 ± 0.98 (8) +2.59 ≫σ** | **88.12 ± 0.68 (8) +3.64 ≫σ** |
| | both | 87.17 ± 0.83 (8) +2.00 | 86.69 ± 1.01 (8) +2.21 |
| sc3 (c3 single 25 MB) | def | 16.08 ± 0.21 (8) | 16.04 ± 0.09 (6) |
| | fix | 16.17 ± 0.23 (8) +0.09 | 16.26 ± 0.12 (8) +0.22 |
| | **mtu** | **16.63 ± 0.19 (8) +0.55 ≫σ** | **16.64 ± 0.15 (8) +0.60 ≫σ** |
| | both | **16.86 ± 0.19 (8) +0.78** | **16.84 ± 0.31 (8) +0.80** |

**Duals (binary A for def/mtu; the fix column is the binary-B redual):**

| cell | def | mtu | fix (redual, vs its def2) |
|---|---|---|---|
| c7 s42 | 166.35 ± 1.12 (0.977×Σ) | **174.41 ± 2.85 (+8.1; 0.994×Σ-own)** | 166.40 ± 2.13 vs 166.38 (tie; 0.997×Σ-own) |
| c7 s7 | 166.52 ± 1.73 (0.986×Σ) | **171.13 ± 2.90 (7) (+4.6; 0.971×Σ-own)** | 165.45 ± 1.86 vs 165.71 (7) (tie; 0.983×Σ-own) |
| c8 s42 | 76.09 ± 11.67 | 75.98 ± 13.95 (tie) | 75.20 ± 17.44 vs 76.54 (tie) |
| c8 s7 | 69.23 ± 10.65 | **86.62 ± 7.64 (+17.4 — the episodic mode caught NOT firing; direction consistent, inside the pooled arm's session spread class)** | 71.78 ± 11.33 (7) vs 70.16 (tie) |

The c7 ≥ 0.97×Σ clause holds for EVERY arm on both seeds; c8 never
regresses (the 0.87 line belongs to the legacy pool arm, unrun here; the
shipped-pool def arms read their documented episodic class).

**Wire truth (qdisc cli0, per-run means — the part-2 mechanism gauge):**
sc2 def 116.11 MB for the 100 MB object → mtu **111.90 MB** (−48 B/pkt ≈
overhead 119 → **~71 B**, framing efficiency 0.910 → ~0.944 — the
derivation's number, measured); sc3 30.72 → 29.76 MB; c7 115.97 → 111.30
MB. Drops sc2: def 451 → mtu 374 (fewer packets, fewer GE events).

**Mechanism, part 1 (the decoupled law engaged exactly as derived —
`wd=al…` gauge on every fix rep; sc2 allow ≈ 300 = anchor·(K+1) with
honest r ≈ 9.8–10.5 k, ret ≈ 2400; sc3 allow ≈ 232, ret ≈ 685):**
- echo RTT collapses as predicted: sc2 107.8/105.4 → **26.9/30.7 ms**;
  sc3 520/541 → **232/219 ms** (prediction-1 echo clause ✓).
- **fired does NOT collapse: sc2 3288/3292 → 3455/3233 (flat); sc3
  2435/2359 → 2172/1979 (−11…−16%)** — prediction-1's fired clause
  FAILS and falsification (2) lands: with the standing queue measurably
  gone the re-fire loop persists, so §16.30's "queue-sustained re-fire
  loop" attribution is AMENDED — the re-fires are receiver-re-advertise
  + per-seq-cooldown clocked re-serves of open holes (and at 27 ms echo
  most are no longer even young: y share 80% → ~55–60%), not
  queue-aged-past-the-law fires. `RWM_RECOV_SP` is NOT subsumed (its
  sc3 +0.32–0.35 remains the only ≫σ singles lever of that family;
  relation recorded per pre-registration (g)).
- the sc2 static-probe insurance number reproduces as the law's cost:
  −1.76/−0.37 — the diagnosis's micro-stall/drop-granularity channels
  (equal wire bytes, ~2% wire idle, more drop EVENTS at a shallow
  queue), NOT the PBH0 −20% class (the cliff sits below ~256).

**B1 jitter cross-check (prediction 4; jit5/jit15 per adv_cells.sh, ×5
per arm per seed, all ARMCOUNT 5/5, same-session BBR-under reference):**

| cell | A = bbr | B = copa | Bfix = copa + decouple | B/A · Bfix/A |
|---|---|---|---|---|
| jit5 s42 | 78.88 ± 3.88 | 27.78 ± 0.41 | 26.72 ± 0.51 | 0.35 · 0.34 |
| jit5 s7 | 76.56 ± 3.53 | 26.65 ± 0.28 | 25.64 ± 0.62 | 0.35 · 0.33 |
| jit15 s42 | 75.65 ± 6.25 | 24.38 ± 0.34 | 23.00 ± 0.51 | 0.32 · 0.30 |
| jit15 s7 | 72.67 ± 3.51 | 22.49 ± 1.40 | 21.15 ± 1.54 | 0.31 · 0.29 |

The gauge shows the ceiling RELEASED (`win=1024/1024` pin → outstanding
~1100–1180 against allow ≈ 1050/ret ≈ 1900; wd live on every copafix
rep) — and Copa does not move (−1.0…−1.4, consistent both seeds).
**The store-ceiling share of the B1 dwell attribution is REFUTED: the
1024 latch was not the jitter-cell binder.** The B1 CC×store interaction
is owned by the EMPTY-PIPE RECOVERY-STALL share alone (outstanding only
*wants* ~1150 at Copa's own operating point; the ~300 ms dwell is
recovery latency, not store truncation) — the pre-registered
attribution-bearing alternative, now measured on both seeds. ADR-0068's
jitter-cell bar sharpens accordingly: lifting store ceilings buys
nothing; the recovery-plane dwell itself is the target.

**Crown gate (tail_matrix c2 spot ×4, seed 42, binary B; per-rep p99
medians, n = 1000 delivered on EVERY rep, all arms):** default 400 B
35.7 [35.4–36.3] / 1200 B 39.5 [35.4–68.4]; **mtu 36.1 [35.3–38.9] /
40.8 [35.4–43.8]**; wdfix 35.5 / 40.7; wdmtu 36.1 / 40.4 — all inside
the historic ~36–48 ms class. **Crown UNREGRESSED for both parts.**

### VERDICTS vs the pre-registrations — PART 2 FLIPS, PART 1 DOES NOT

- **PART 2 (`RWM_WIRE_COMPACT`): every pre-registered clause holds on
  both seeds** — (1) overhead gauge 119 → ~71 B ≤ 75 ✓; (2) sc2 +2.59/
  +3.64 in the +2.5…4 band ≫σ ✓; (3) sc3 +0.55/+0.60 in the +0.5…0.9
  band ≫σ ✓; (4) c7 +8.1/+4.6, c8 tie/+17.4, composed ≈ additive ✓;
  (5) crown unregressed ✓; (6) dnf = 0, `mtu_blackhole_wedge` green
  (datagrams SHRINK — no floor interaction) ✓. **FLIP: `RWM_WIRE_COMPACT`
  ships DEFAULT ON** (`=0` = the legacy-framing opt-out arm; PROTOCOL_
  VERSION 5 refuses pre-compact peers cleanly at handshake). vs the
  bars: sc2 87.8–88.1 vs quinn-bbr 91.9 (the c2 gap ~6.9 → ~3.9 Mbit);
  sc3 16.6 vs 18.6 (2.55 → ~1.95); per §16.32 the c8-remaining-gap ≡
  the c2 gap, so the c8-to-kernel-MPTCP distance shrinks by the same
  term. The residual c2 gap decomposes as ~1.2 Mbit of remaining
  fixed-overhead delta (71 vs quinn's ~61 B/pkt on bigger MTUD packets)
  + the ~2.7 Mbit reactive-plane term the fired-count amendment above
  re-attributes (below) + margin.
- **PART 1 (`RWM_WIN_DECOUPLE`): predictions 2 and 3 FAIL on both
  seeds** (sc2 −1.76/−0.37 against a +1.5…3 band; sc3 +0.09/+0.22
  against +0.8…1.6); prediction 1 PARTIAL (echo collapse exact, fired
  flat — falsification (2) fires and amends §16.30, above);
  prediction 4 lands on its attribution-refuting branch (store-ceiling
  share = 0); prediction 5 holds after the scope fix; crown clause ✓.
  Per the flip rule and discipline item 11: **NO FLIP — `RWM_WIN_
  DECOUPLE` ships DEFAULT OFF**, register row added (the failure names
  three mechanisms: the re-fire loop is re-serve-clocked rather than
  queue-sustained; the 1024-latch's honest insurance value at sc2 is
  ~0.4–1.8 Mbit of sub-sweep ack-granularity/drop-granularity cover;
  the jitter-cell dwell is recovery-latency-owned, not store-owned).
  Retained as the measured A/B arm with its law tests; the N=1-scoped
  sampler pattern (pause semantics) stays — it is the reusable piece.
- **Composition note (recorded, not flip-bearing):** both = fix+mtu is
  the best sc3 arm ever measured (16.86/16.84 — above the RECOV_SP
  record 16.45–16.48) while costing ~−0.6/−1.4 vs mtu alone at sc2 —
  the decoupled window's sc3 value appears only once the framing tax is
  paid down. A future part-1 re-ask starts from that composed datum,
  not from scratch.

**Suites (final tree, flip committed):** lib 376/376 (4 new part-1 law
tests + 5 compact-codec tests + the pause law); raptorpath-math full
59/19/22/4/4/3/25; gate_suite 15/15 release; `mtu_blackhole_wedge` 2/2;
`perf_loopback` 8/8; `win_decouple_loopback` + `wire_compact_loopback`
(new) + `copa_sole_loopback` + `emit_batch_loopback` +
`recov_mp_loopback` + `backpressure` — all green. Gates-default test
pins `win_decouple=false`; the compact gate is a transport resolve-once
knob (`wire_compact_active`, default ON, noted in gates.rs).

Ops: lock `/tmp/rwm-vm.lock` taken 2026-08-06 17:25:55 UTC (found FREE),
held through diagnosis → batteries → phase 2, released 20:24:12 UTC
after teardown verification (no rp processes, no rp-* netns) + log
preservation;
rp-* netns torn down per invocation and verified at teardown; logs +
per-run diag under `/home/vibe/winmtu/` (diagnose-s42, battery-s{42,7}
incl. redual appendix, jit-s{42,7}, tails-s42, phase2, diag/); binaries
1306bea4… / 335e07f1… / 0e4f5cde… with sha256 + commit + lscpu + kernel
in every log header; seed-7 abort ns recorded above; the winmtu battery
harness note for FUTURE sessions: with the compact default ON, the
`compact DATA framing ACTIVE` echo now prints on DEF arms too (the
def-arm contamination check keys must move to `=0`-arm absence).

## C8 Slow-Path Conversion (2026-08-06) — DIAGNOSIS-FIRST (branch `feat/c8-conversion` from f2f1c78; the "C8-Aware Pool Law" verdict's named successor: the binder is NOT pool sizing — WHY does the slow path convert ~nothing at c8?)

*Decision record context: → [ADR-0058](adr/0058-path-scaled-outstanding-pool.md)
(pool arithmetic REFUTED for c8), the "C8-Aware Pool Law" section above
(the measured fact this work re-settles: every pool law converges to
fast-single + ~2.6–2.7 Mbit of the slow path's ~16; legacy-1024 0.866/0.868×Σ
vs shipped path-scaled 0.71–0.76×Σ; fast path parks the un-SACKed span,
slow path holds ≤10% of the pool), and "Competitive Baseline" (the external
bar: kernel MPTCP-BBR 89.7–92.6 at c8 — noting honestly that the kernel's
own slow-path conversion is +3.1/−2.4 vs its same-session single-path BBR
89.5/92.1, i.e. ~0±3 Mbit: no in-order transport measured to date banks the
c3 path's Σ-share at this cell).*

**Diagnosis plan (this block written BEFORE the instrumented runs; the
instruments are DIAG-gated and behavior-inert — commit 15de9f6).** Four
candidate conversion-failure channels, to be distinguished by per-path
gauges in ONE instrumented c8 pass (legacy + pbs arms, seed 42, ×2):

- (a) PLACEMENT STARVATION — the placement law puts too little SOURCE on
  the slow path. Gauge: `[C8CONV-S] splace` (per-path first source
  placements) vs the capacity share (~16–19%); `[C8CONV-R] fst` per path.
  Mechanical prior (code read, to be confirmed): at Bulk the placement
  cost's `srtt_i/2 / ref_srtt` propagation term alone puts the idle slow
  path ~1.5–1.75 dimensionless units above the fast path — e^10:1 odds at
  T=0.15 — and the POOL gate pauses admission before the fast path's queue
  term can ever climb enough to spill; predicted signature: splace_slow
  share ≪ capacity share with paused > 0.
- (b) BEHIND-THE-FRONTIER ARRIVALS — slow-path deliveries arrive after the
  region was already served (they displace would-be-retransmits, add no
  goodput). Gauge: `[C8CONV-R] dup` share of slow-path arrivals (a source
  arrival for an already-received seq), plus which side's copies win.
- (c) HoL/REASSEMBLY COUPLING — slow-path-owned holes serialize the
  cumulative frontier. Gauge: `[C8CONV-S] stallo` (frontier-stall wall time
  by blocking-hole OWNER path) + `[C8CONV-R] unb` (stall time by RESOLVING
  arrival path).
- (d) SKEW MIS-SCHEDULING — source lands on the slow path with too little
  lead: the recovery plane re-serves it on the fast path before the
  original arrives (spurious cross-path retx → the dup flood → slow-path
  work displaced). Gauges: `[C8CONV-S] retxo` (retx by ORIGINAL placement
  path) / splace ratio vs the path's realized loss; `[C8CONV-R] lead`
  (first-copy frontier lead at arrival). Code-read prior (to be confirmed):
  the `RWM_RECOV_MP` hole law keys `mp_n_paths` + its per-path clocks on
  `active_paths()` — the SATURATION-FILTERED set (`available() > 0`) whose
  cwnd-full-path trap is already documented at the Copa-sole store law and
  `capw_store_cap` — so a cwnd-saturated path drops the law to N=1 bypass
  (legacy age gate, cross-path clock) mid-transfer; the July c8 mpr gauge
  shows the matching signature (pbs: 1063/1539 retx fired YOUNG vs their
  own flight-path law threshold; 1056 of the fired flights were slow-path).

(b)+(d) together = the arrival-alignment question. The refuted-DAPS history
(ADR-0065) refutes the OLD implementations, not the geometry — if the
diagnosis points here, the fix must derive lead-time from the honest
per-path anchors (RTprop_i, rate_i), placed as ONE continuous law, no mode,
no per-topology branch (the no-mode-switch invariant; the July verdict's
"heterogeneity detector → legacy-span law" suggestion is NOT eligible).

A fix ships only under its own item-11 pre-registration appended BELOW
after the diagnosis names the dominant channel with numbers, gated in
`gates.rs` default OFF, with the c7 (≥0.97×Σ held) / singles-inert
no-regression clause. Battery: c8 primary (arms shipped / legacy-1024 /
+fix on the pool the diagnosis says) + c7 + sc2/sc3 identity, seeds 42+7
×8 interleaved, same-session Σ, per MEASUREMENT DISCIPLINE 1–11.

*(Diagnosis results and everything below written AFTER the runs.)*

### DIAGNOSIS RESULTS (VM 10.1.5.16, 2026-08-06 15:48:11–15:48:30 UTC; binary sha256 070d5443393f235f… = commit be4062f, built fresh on the VM (stale binary rm'd, CRLF-converted); E5-2650 v3; c8 ×2 per pool arm, seed 42, RWM_DIAG=1; driver `tools/l1/c8conv_diag.sh`, log `/home/vibe/c8conv/diagnose-s42.log` + per-run diag/; lock taken 15:43:43Z with the VM verified QUIET (0 rp processes, no rp netns) — the B1 co-tenancy window ended before any run of this workstream)

**SESSION DATUM FIRST (n=2/arm, honest σ-unknown): on the f2f1c78 tree the
c8 cell reads BETTER than the July record on BOTH pool arms — legacy
88.3/89.8, pbs 85.8/83.7 (July: legacy 86.7±3.3, pbs 72.3±17.5 with the
stall-burst mode). The pbs collapse class did NOT appear in either rep;
whether it is gone or episodic is a battery question (n=8, both seeds).**

Per-path conversion gauges (p0 = fast c2, p1 = slow c3; capacity share of
p1 ≈ 16%):

| gauge | legacy r1 | legacy r2 | pbs r1 | pbs r2 |
|---|---|---|---|---|
| goodput (Mbit/s) | 88.28 | 89.83 | 85.81 | 83.73 |
| splace p1 share | 837/21152 = **4.0%** | 1816/19833 = **9.2%** | 3629/21152 = **17.2%** | 3429/21152 = **16.2%** |
| fst p1 (receiver first-copies) | 821 | 1879 | 3160 | 3061 |
| dup share of p1 arrivals | 10.5% | 10.4% | 6.9% | 7.1% |
| retxo p1 / splace p1 | 109/837 = **13.0%** | 112/1816 = 6.2% | 725/3629 = **20.0%** | 534/3429 = **15.6%** |
| retxo p0 / splace p0 | 3.1% | 3.6% | 3.2% | 3.6% |
| mpr young fires | 171 | 120 | **749** | **412** |
| stallo p1 share (ms) | 427/1689 = 25% | 801/1484 = **54%** | 750/2087 = 36% | 749/2005 = 37% |
| p1 echo rtt at end (rtp) | 274 (39) | 140 (40) | **511 (40)** | **563 (34)** |
| mean win / cap | 912/1024, paused 5.7% | 1000/1024, 4.9% | 2112/4096, 0.9% | 2155/4096, 1.6% |

**The dominant channel, named: (a)+(d) — PLACEMENT STARVATION under the
binding pool, becoming ARRIVAL-MISALIGNMENT once placement is fed.**

- (b) is REFUTED as the dominant channel: ~90% of slow-path arrivals are
  FIRST copies in every rep of both arms — slow deliveries DO convert when
  they happen; displacement (dup ≈ 7–10%) is a tax, not the wall.
- Under legacy-1024 the limiter is (a): the slow path is under-placed ×2–4
  vs its capacity share (4.0–9.2% vs 16%) — the mechanical prior confirmed:
  with the pool PAUSING admission (4.9–5.7%) the fast path's queue term
  never climbs enough for the Bulk softmax (whose `srtt_i/2/ref` term alone
  is worth e^10:1 odds) to spill; the slow path gets scraps.
- Under pbs the placement DOES reach capacity share (16–17%) — and the
  cell still nets LESS than legacy: the conversion is eaten by (d): 15.6 to
  20% of slow-placed symbols are re-served (vs their ~4.8% realized GE
  loss, ×3–4 spurious), young fires 412–749 (the `active_paths()`
  saturation-bypass prior — the law drops to N=1 mid-transfer), the slow
  path's echo RTT inflates to 511–563 ms against a 34–40 ms RTprop (an
  UNBOUNDED slow queue: placement is capacity-proportional but not
  NEED-TIME-bounded), and slow-owned holes carry 36–37% of frontier-stall
  time on 16% of placements.
- (c) is real but derivative: slow-owned stall burden per placement is
  ×5–10 the fast path's — it is the SYMPTOM of (d)'s lateness, not an
  independent reassembly defect.

**What a fix must do (the geometry the numbers demand): feed the slow path
∝ capacity (kill (a)) while bounding each slow placement's LATENESS to
what the in-order frontier can absorb (kill (d)) — the arrival-alignment
law, derived from the honest anchors, as ONE continuous term. Pool
arithmetic stays refuted: pbs already proves ∝-capacity placement without
need-time bounding nets NEGATIVE.**

### FIX PRE-REGISTRATION — `RWM_PLACE_SLACK` (discipline item 11; written BEFORE the build; default OFF; secondary lever `RWM_RECOV_MP_LIVE`, default OFF, separately gated + echoed)

**(a) Mechanism — the frontier-slack placement law (ONE continuous term,
no mode, no threshold, no per-topology branch).** The placement cost's
load term becomes

    cost_i = max(0, Ê_i(load) − S) / ref_srtt + w_bw·r_i + w_div·ρ_fate

where S = the FRONTIER SLACK — the time the in-order frontier will take to
need the symbol being placed:

    S = clamp( (sent_edge − cum_ack) / R_ack , 0, 250 ms ),   S = 0 until
    R_ack has a sample (cold start = shipped), S set only when N ≥ 2 live
    paths (N = 1 placement is degenerate anyway), refreshed on the existing
    5 ms dyn-cap cadence; R_ack = EWMA of the cumulative-ack advance rate
    (delivery-truth, self-measured, immune to the plain anchor's ×5–9
    over-read; sent_edge − cum_ack = the live stream span).

Shape: S = 0 reproduces the shipped cost BIT-EXACTLY (max(0, x−0) = x —
the law is a strict generalization, continuous in S). A path whose
delivery time fits inside the frontier's need-time costs nothing extra —
so the slow path earns placements up to EXACTLY the backlog it can deliver
by need-time (deadline-aware water-filling: equilibrium backlog_i ≈
rate_i·(S − owd_i), capacity-proportional); beyond that its queue term
crosses S and the softmax chokes it CONTINUOUSLY. The (a) starvation ends
(idle-slow cost clamps to ~w_bw·r_s instead of carrying the e^10 latency
odds); the (d) lateness is bounded by construction (a placement whose
Ê exceeds S is priced, so the unbounded 511–563 ms slow queue cannot
form). At c7 (symmetric) any symmetric cost gives the same 50/50 split —
the law changes burst micro-structure at most (n=8 both seeds watches it).
Realtime is untouched (place_symbol is reliable-window-only; S is derived
from the measured span, which a latency-tight stream keeps small —
continuous self-honesty, no hint gate).

**Secondary lever (the (d) young-fire repair), `RWM_RECOV_MP_LIVE`:** the
`RWM_RECOV_MP` hole law's `mp_n_paths` + per-path clock snapshot move from
`active_paths()` (saturation-filtered — `available() > 0`; a cwnd-full
path collapses the law to the N=1 bypass = legacy age gate on a cross-path
clock) to `live_paths()` — the same trap already fixed at the Copa-sole
store law and `capw_store_cap`, now at the recovery plane. Default OFF;
battery arms attribute it separately from the slack law.

**(b) Prediction (effect size + cells).** On the SHIPPED pool base (pbs —
the diagnosis says the pool is not the binder on this tree): slack law
splace_p1 → capacity share (14–19%) with retxo_p1/splace_p1 ≤ ~2× realized
GE (≤ 10%), slow echo RTT bounded ≤ ~150 ms, young fires below the pbs
control; c8 goodput ≥ BOTH incumbents on both seeds with the target
≥ 0.90×Σ (≈ 90+; the banked slow share), toward the external 89.7–92.6
bar. c7 ≥ 0.97×Σ held (placement split unchanged by symmetry). Singles
bit-inert at N = 1 (S never set; gauge zero) — the arm's env carries only
the INFO echo. `+RWM_RECOV_MP_LIVE` composed: young fires → ~0 class at
c8, no c7 effect ≫σ.

**(c) Falsification.** (1) If slack-law c8 ≤ the pbs control on both seeds
(≫σ), the alignment geometry is refuted ON THIS SUBSTRATE — record the
gauge state (splace share, retxo ratio, rtt bound: which sub-claim failed)
and STOP (register, no tuning pass). (2) If splace_p1 reaches capacity
share AND retxo_p1 stays ≤ 10% AND rtt stays bounded AND goodput STILL
does not beat legacy — conversion at this cell is structurally
displacement-bounded; that verdict redirects the roadmap (the honest
outcome the kernel MPTCP-BBR datum (+3.1/−2.4 vs its own single) already
prices) and the c8 chapter closes with legacy-class as the ceiling. (3)
c7 < 0.97×Σ on either seed or singles regress ≫σ ⇒ no flip regardless of
c8.

**(d) Derivation re-read for self-contained failure predictions.** (1) The
S-clamped region flattens the SHORT-TERM queue differential (both costs 0
until loads reach S) — at c7 this could coarsen transient load balancing;
by symmetry the mean split is unchanged, so any damage appears as c7
σ inflation, watched at n=8×2 seeds — this is the law's one plausible
regression channel and it is covered by falsification (3). (2) The
transient over-placement window at c8 (both costs clamped → ~uniform until
the slow queue builds to S) is bounded by rate_slow·S ≈ 200–300 symbols
≈ 100 ms — 5% of a 2.3 s transfer, priced, not disqualifying. (3) R_ack
during a full frontier stall decays → S grows → MORE slow placement (a
positive-feedback risk); bounded by the 250 ms clamp and by the slow
path's own queue term crossing S; if the battery shows stall-coupled
oscillation the law needs a stall-witness guard (NAMED follow-up, not a
tuning pass). (4) The prize is bounded: Σ-share of the slow path is
~16 Mbit and the kernel reference banks ~0±3 of it; predicting ≥ 0.90×Σ
(> legacy + ~2) is deliberately ABOVE the kernel's realized conversion —
failure against it while beating both incumbents would still be a
positive-but-humble result, handled under the flip rule below.

**FLIP RULE (fixed before the battery).** `RWM_PLACE_SLACK` flips default
ON only if, on BOTH seeds: c8 slack ≥ legacy AND ≥ pbs (Δ ≫ σ_s against
at least one, no Δ ≪ −σ against either), c7 ≥ 0.97×Σ, singles inert
(within σ). `RWM_RECOV_MP_LIVE` flips only if its composed arm is
inert-or-better everywhere with the young-fire gauge collapsing at c8.
Otherwise both stay OFF with the falsification outcome recorded. The
POOL-LAW re-settlement (which pool wins both cells WITH conversion
working) is reported either way from the same battery.

**Battery (pre-registered; driver `tools/l1/c8conv_battery.sh`).** VM
protocol per MEASUREMENT DISCIPLINE 1–11: seeds 42+7, ×8 interleaved
round-robin per rep, fresh tunnel per invocation, same binary every arm
(sha256 + lscpu + env in the log header), per-arm echo assertion both
directions (SR default; PBS/fix per arm; the fix INFO echo follows the ENV
at singles — the c8pool harness-note lesson pre-applied), same-session Σ
singles per arm env, seed-7 topo-abort protocol (n recorded, nothing
discarded), per-rep UTC wall-clock stamps (the B1 co-tenancy discipline:
any rep overlapping another worker's reported VM activity is
contamination-suspect and re-run). Arms: legacy / pbs / fix
(= `RWM_PLACE_SLACK=1` on the shipped pool) / lfix (= slack on legacy
pool, c8 only — the pool re-settlement arm) / fix+live (c8 only, the
secondary-lever attribution arm); cells c8 (25 MB), c7 (200 MB), sc2
(100 MB) + sc3 (25 MB) singles.

*(Results below this line were written after the runs.)*

### AMENDMENT (pre-battery, 2026-08-06 ~16:20 UTC) — the smoke run falsifies the UNBOUNDED-S form and NAMES the missing mechanism; the law is re-derived with the recovery-patience bound BEFORE any battery

One smoke run of the law as first pre-registered (c8, seed 42, sha
817edc57…, `RWM_PLACE_SLACK=1`): **66.2 Mbit — below BOTH incumbents.**
The gauges say exactly why, and it is the derivation re-read's named risk
(3) plus a coupling the derivation missed: S clamped at its 250 ms
ceiling (`slk=250ms/r9094`), placement DID reach capacity share
(splace_p1 = 3172/21152 = 15.0% — the (a) starvation is dead), but
**retxo_p1 = 1562/3172 = 49%**: the placement plane now tolerates 250 ms
of slow-path lateness while the RECOVERY plane's patience for a slow
flight is only ~9/8·srtt_slow (~55–70 ms) — the planes FIGHT, half the
slow placements are re-served cross-path (dup_p0 = 1544 wasted fast
arrivals), and the frontier serializes behind 250 ms-late symbols
(stallo_p1 876 ms/44).

Per discipline item 11 this failure NAMES a new mechanism (it is not a
tuning miss): **the placement lateness budget must be bounded by the
recovery plane's patience for the placed path.** Re-derived law (the
battery runs on THIS form; no other change):

    D_i    = min(S, 9/8 · srtt_i)        ← per-path deadline; 9/8 is RFC
                                            9002 kTimeThreshold — the SAME
                                            constant `mp_time_threshold_us`
                                            already uses, not a new dial
    cost_i = max(0, Ê_i − D_i)/ref_srtt + w_bw·r_i + w_div·ρ_fate

S = 0 still reproduces shipped bit-exactly (min(0, ·) = 0). The slow
path's admissible backlog becomes rate_i·(9/8·srtt_i − owd_i) once the
frontier slack covers it — deep enough to convert continuously, never
deeper than what the hole law will not re-serve. Predictions/falsification
conditions of the pre-registration carry over unchanged against this
form; prediction sub-claim "retxo_p1 ≤ ~2× realized GE" is now load-
bearing (it was the clause the unbounded form broke).

*(Battery results below this line were written after the runs.)*

### L1 BATTERY RESULTS (VM 10.1.5.16; binary sha256 4d23d0f9698b049c… = commit 87135a0, SAME binary every arm, built fresh (stale rm'd, CRLF-converted); E5-2650 v3 aes+avx2+pclmulqdq (post-divide) in every log header; 1 run/invocation, 12 arms interleaved round-robin per rep ×8, fresh tunnel per invocation, seeds 42 AND 7, RWM_GEN=0 RWM_DIAG=1 everywhere; per-arm echo assertion both directions: **0 completed-run liveness mismatches on any of the 4 logs** (s42 96/96 completed; s7 96 headers / 53 completed / 43 seed-7 topo-ping aborts, every abort SUMMARY-LESS, n recorded per arm, nothing discarded; the 32 s42 + singles-class s7 "CONTAMINATION-pbs" flags are the July harness-note INFO-echo class — pbs default-configured while N≥2-gated — plus the abort stale-log class, counts matching); drivers `tools/l1/c8conv_{battery,live}.sh`, logs `/home/vibe/c8conv/{battery,live}-s{42,7}.log` + per-run diag/ (13 MB preserved) + `BINARIES.txt`; lock 15:43:43→16:54:29 UTC; runtimes: s42 battery 16:14:50–16:34:24 (19.5 min), s7 16:34:33–16:46:08 (11.6 min), live supplemental 16:46:20–16:52:10 (5.8 min), dc1 probe ~16:52–16:54. CO-TENANCY: every run of this workstream started AFTER the B1 worker's stop (lock taken with the VM verified quiet, 0 processes/netns); no rep overlaps any other worker's activity — nothing contamination-suspect, nothing re-run.)

Σ = same-session same-env singles (sc2+sc3 at c8; 2×sc2 at c7), per arm
env: s42 pbs-env Σ_c8 = 101.43, Σ_c7 = 170.72; fix-env Σ_c8 = 100.84,
Σ_c7 = 169.70; s7 pbs-env Σ_c8 = 100.55, Σ_c7 = 169.18; fix-env
Σ_c8 = 100.22, Σ_c7 = 168.52.

**c8 (the target cell), mean ± σ_s (n) → vs own-env Σ:**

| arm | s42 | vs Σ | s7 | vs Σ |
|---|---|---|---|---|
| **legacy** (`RWM_STORE_PATHS=0`) | **88.62 ± 3.30 (8)** | **0.874** | **87.62 ± 2.99 (4)** | **0.871** |
| pbs (shipped default) | 70.46 ± 14.64 (8; lows 49.5, 50.6) | 0.695 | 84.21 ± 6.54 (4; low 73.0) | 0.838 |
| fix (slack, pbs pool) | 72.33 ± 13.02 (8; lows 53.8, 54.1) | 0.717 | 77.77 ± 8.34 (5) | 0.776 |
| lfix (slack, legacy pool) | 88.38 ± 5.60 (8) | 0.876 | 89.40 (2) | 0.892 |
| fixlive (slack+live, pbs pool) | 82.66 ± 5.66 (8; min 73.3) | 0.820 | 81.43 ± 7.25 (5) | 0.813 |
| live ALONE (supplemental, pbs pool) | **81.83 ± 9.68 (6)** vs same-block pbs2 73.42 ± 12.16 (6) | — | **83.27 ± 6.49 (4)** vs pbs2 71.27 ± 20.74 (3) | — |

**c7 (the protection cell), mean ± σ_s (n) → vs own-env Σ:**

| arm | s42 | vs Σ | s7 | vs Σ |
|---|---|---|---|---|
| legacy | 158.63 ± 15.63 (8; collapse tail) | 0.929 | 163.01 ± 2.95 (6) | 0.964 |
| **pbs** | **166.48 ± 3.44 (8)** | **0.975** | **166.73 ± 1.13 (5)** | **0.985** |
| fix (slack) | **145.60 ± 3.07 (8)** | **0.858** | **150.96 (2)** | **0.896** |
| live ALONE (supplemental) | 167.75 ± 1.23 (6) vs pbs2 166.14 ± 1.64 (6) | — | 167.19 ± 2.94 (4) vs pbs2 167.86 ± 3.22 (2) | — |

**Singles (Σ terms + N=1 inertness):** sc2 pbs/fix = 85.36 ± 0.31 /
84.85 ± 0.72 (s42), 84.59 ± 1.03 / 84.26 ± 0.66 (s7); sc3 16.07 ± 0.16 /
15.99 ± 0.29 (s42), 15.96 ± 0.12 / 15.96 ± 0.28 (s7) — fix inert within σ
(−0.3…−0.5 class, and the slack law is N≥2-gated by construction).
dnf = 0 on every completed run, all logs. **dual-c1 probe (±live, seed 42
×3 interleaved pairs): default 202.1/237.3/214.2, live 194.4/229.6/196.2 —
live LOWER in 3/3 pairs (mean −11.2).**

**The conversion gauges across ALL arms (the mechanism table; aggregated
over reps; p1 = slow c3):**

| c8 arm | splace p1 share | retxo p1/splace p1 | goodput s42/s7 |
|---|---|---|---|
| legacy | 6.2% / 6.5% | 15.8% / 25.3% | **88.6 / 87.6** |
| lfix | 11.2% / 9.8% | 9.5% / 19.5% | 88.4 / 89.4 |
| pbs | 16.1% / 11.3% | 26.0% / 28.7% | 70.5 / 84.2 |
| fix | 15.5% / 12.3% | 21.2% / 22.3% | 72.3 / 77.8 |
| fixlive | 17.6% / 14.5% | **3.6% / 8.2%** | 82.7 / 81.4 |

### VERDICT vs the pre-registration — BOTH flips NO; the structural verdict lands, with numbers

- **`RWM_PLACE_SLACK`: falsified, both clauses.** (1) c8 never ≥ both
  incumbents (s42 72.3 vs legacy 88.6; s7 77.8 vs pbs 84.2); (2) **c7
  fails the protection clause ≫σ on BOTH seeds: 145.60 ± 3.07 / 150.96 =
  0.858/0.896×Σ vs the required ≥ 0.97** — the derivation re-read's named
  regression channel (1) (the S-clamped region flattens the short-term
  queue differential; at c7 the law let both paths run ~100 ms-class
  backlogs before any placement signal, where the shipped cost corrects
  per-symbol). The singles stayed inert as predicted; the law engaged as
  designed everywhere (echo + slk gauge live; splace_p1 reached capacity
  share 15.5/12.3%) — the MECHANISM worked, the PREDICTION failed. **NO
  FLIP; deprecation register row added; no tuning pass.**
- **`RWM_RECOV_MP_LIVE`: mechanism PROVEN, flip BLOCKED at dual-c1.** The
  young-fire gauge collapses exactly as predicted (c8 y: pbs-class
  412–749 → 16-class; retxo_p1 26–29% → 3.6–8.2% composed) and c8 improves
  in 3 of 4 independent pairings (+8.4/+12.0 supplemental both seeds,
  +12.2 composed s42; −2.8 ≪σ composed s7) with the pbs collapse floor
  lifted (min 49.5 → 73.3) and c7 inert-or-better — but the pre-registered
  gate is "inert-or-better EVERYWHERE", and the dual-c1 probe reads
  live LOWER in **3/3 interleaved pairs (−11.2 mean)**. DEFAULT OFF;
  retained as the measured A/B arm. Named follow-up: attribute the dc1
  interaction (the law now ENGAGES at clean saturated duals where the
  saturation filter used to bypass it — its young-suppression may be
  delaying the retransmits dc1's scheduler-gap churn actually needs).
- **The structural verdict (falsification (c)(2), measured across the
  whole placement spectrum):** slow-path source share and c8 goodput are
  MONOTONICALLY ANTI-CORRELATED across five arms on both seeds — 6.2%
  share → 88.6; 11.2% → 88.4; 16–18% → 70–83. Conversion itself WORKS
  (~90% of slow-path arrivals are first copies; displacement refuted),
  and killing the re-serving tax (fixlive: retxo 3.6%) still leaves the
  fed arms ~6 below the starved one: the residual costs are the
  frontier's slow-owned stalls (24–39% of stall time) and the end-of-
  object drain tail (the last slow-queued symbols serialize completion —
  visible in the DIAG tails as src=0 drain phases). **At this cell's
  asymmetry (5× rate, 4× RTT, ~2× loss), feeding the slow path SOURCE is
  negative-margin under every placement law measured; the c8 optimum is
  the starved corner — fast-path source + slow-path recovery traffic ≈
  fast single + 2.6, which legacy-1024 reaches by accident of its pool
  arithmetic.** The external reference agrees: kernel MPTCP-BBR banks
  +3.1/−2.4 vs its own same-session single-path BBR at this cell.
- **vs the bar:** best rp c8 = legacy 88.62/87.62 = 0.874/0.871×Σ — the
  pre-registered ≥ 0.87×Σ line is HELD (by the legacy arm, both seeds),
  1–4 Mbit under kernel MPTCP-BBR's 89.7–92.6. The REMAINING gap to the
  bar is owned by the single-path c2 gap (§16.30's framing/MTU +
  reactive-plane accounting: rp sc2 84.6–85.4 vs kernel single-path BBR
  89.5–92.1), NOT by multipath conversion — closing c2 closes c8.
- **Pool-law re-settlement (the "C8-Aware Pool Law" tension, re-asked
  with conversion instrumented):** NO pool law wins both cells — legacy
  wins c8 (88.6/87.6 vs 70.5/84.2, 5 sessions consistent in direction)
  and pbs wins c7 (166.5/166.7 = 0.975/0.985×Σ vs legacy 158.6/163.0,
  with legacy's c7 collapse tail visible again at s42, σ 15.6). The c8
  WATCH stands, now with its mechanism named (above) and with
  `RWM_RECOV_MP_LIVE` as the measured half-repair of pbs's c8 collapse
  mode (flip-blocked at dc1, follow-up named). The shipped default is
  UNCHANGED.
- **Session datum, recorded:** on this tree the pbs c8 collapse class is
  EPISODIC across seeds/sessions (s42 70.5 ± 14.6 with 49-class lows; s7
  84.2 ± 6.5) — the July "0.71–0.76 both seeds" reading was the mode
  firing on both; the 2026-08-06 diagnosis pair caught it NOT firing
  (85.8/83.7). Whole-session comparisons at c8 need the per-run
  distribution, not the mean (discipline item 4, re-affirmed).

Ops: rp-* netns torn down after every invocation (harness trap) and
verified empty at teardown; logs + per-run diag + binary hashes preserved
under `/home/vibe/c8conv/`; lock released 16:54:29 UTC after cleanup.

## C8-Aware Pool Law (2026-07-27) — PRE-REGISTRATION (discipline item 11 — this block written BEFORE the diagnosis runs and the battery; branch `feat/c8-pool-law` from be24660; env `RWM_STORE_CAPW`, default OFF)

*Decision record: → [ADR-0058](adr/0058-path-scaled-outstanding-pool.md)
(the "c8 WATCH" follow-up clause), [ADR-0060](adr/0060-sack-clocked-store-release.md)
(the release law that moved the c8 story).*

**(a) Mechanism (the question + the candidate law).** The c8 WATCH, twice
measured and externally priced: under SACK-release the LEGACY 1024 pool
beats the path-scaled N×2048 pool at c8 (0.854/0.870×Σ vs 0.722/0.758,
both seeds, "Consolidation") while path-scaled wins c7 (its removal
re-opens the c7 collapse class) — and kernel MPTCP-BBR banks 90–93 at the
cell where the shipped stack holds 67–74 ("Competitive Baseline": BELOW
same-session single-path kernel BBR). Diagnosis hypothesis, to be
gauge-checked FIRST (one instrumented c8 run per pool arm, per-path
store-attribution `sout` gauge added DIAG-only this branch): (a) the
path-COUNT-scaled pool over-weights the slow path — its 1/5-rate
contribution does not earn half the depth, so the slow path parks
unacked-frontier slots it cannot drain within its recovery round; (b)
under SACK-release the pool's binding role shifted from total-in-flight to
the UNACKED-FRONTIER SPAN (outstanding = retained − SACK-released), where
excess depth = the resequencing/HoL span the cumulative frontier must
cross behind the slow path's stragglers. The derived law (`RWM_STORE_CAPW`,
NOT tuned): the CAPACITY-WEIGHTED shared pool —

  pool = clamp(Σ_i cap_i, floor, N·knee),
  cap_i = anchor_i·(K_i + gain − 1) + rate_i·(gain − 1)·R
        = rate_i·(K_i·RTprop_i + (gain−1)·(R + RTprop_i))

— the honest-cap per-path law (`honest_store_cap`, §16.22 addenda) SUMMED
AS ONE SHARED POOL, not per-path accounts: each path earns depth
proportional to its OWN pipe + recovery round (rate_i·(echoRTT_i + R)
class), while admission still gates on the pooled total so cross-path
borrowing stays free — ADR-0058's pooled-vindicated verdict kept, only
the SIZING law changes. Engaged N ≥ 2 with EVERY live anchor warm;
fallback = the configured pooled law until anchors live; N = 1 legacy
bit-exact (the STORE_PATHS singles contract). The law needs the honest
anchor: the arm composes `RWM_PLAIN_RS=1` (the ADR-0058/LOO-named
composition; with the legacy ×4.6–7.4 over-read the Σ clamps at the
N×knee ceiling ≡ path-scaled, documented at `capw_store_cap`). Numeric
shape at the anchors' cross-check points: c8 pool ≈ 1250 (fast c2 term)
+ 400–500 (slow c3 term) ≈ 1650–1750 — strictly between legacy-1024 and
path-scaled-4096; c7 ≈ 2×1250 ≈ 2500 (symmetric degenerate ≈ N×single).

**(b) Prediction (effect size + cells).** Derived law ≥ max(legacy,
path-scaled) at BOTH c7 AND c8, both seeds: c8 ≥ the loo-pbs class
(target ≥ 0.87×Σ, i.e. ≥ ~87 Mbit — banking the measured +11–13 vs the
shipped stack, toward the external 90–93 bar); c7 unregressed vs the
stack class (0.98–0.99×Σ, the collapse class absent). Singles inert at
N = 1 (CAPW never engages; the capw arm's singles carry the RS witness
cost, pre-measured −3–5 Mbit class, and price it). Gauges must show the
mechanism: capw pool cap ≈ 1.6–1.8k at c8 with slow-path sout bounded
near its own term and slow dwell ≈ its recovery round (vs the pbs arm's
parked span).

**(c) Falsification.** If the derived law LOSES to plain legacy-1024 at
c8 (both seeds, ≫σ), the binder is not pool sizing — report the gauge
evidence (which path holds the span, dwell, paused fraction) and STOP:
the register gets the falsified law, the diagnosis names the true binder,
no tuning pass. If the diagnosis gauges refute hypothesis (a)/(b) BEFORE
the battery (slow-path share NOT outsized under pbs; the frontier span
NOT the stalled quantity), record what they show instead and re-derive or
stop — the battery does not run on a refuted premise.

**(d) Derivation re-read for self-contained failure predictions.** (1)
The law's c8 pool (~1.65–1.75k) sits ABOVE legacy-1024 — if legacy's c8
advantage is actually "smaller is better monotonically" (an un-modeled
binder below 1024), CAPW lands between the arms and loses to legacy: that
outcome is covered by (c) and is informative (names a sub-1024 binder).
(2) The slow-path term rides RTprop_c3 ≈ 40 ms + R = 100 ms; if the c3
path's GE-burst recovery routinely exceeds one R-round, the term
under-funds the slow path's recovery runway — visible in the gauges as
slow-path retx-stalls with sout at its term; that failure names the next
mechanism (recovery-round-aware R_i), not a tuning knob. (3) The RS
witness cost (−3–5 singles class) is carried openly by the capw arm's
own same-session Σ terms — the composed stack-rs probe already measured
c8 stack-rs ≥ stack (+6.5/+11.9), so the prior is FOR the composition at
the dual cell.

**FMTCP piggyback (the DEPRECATION REGISTER's owed re-test, ADR-0066 —
pre-registered here).** One arm `RWM_FMTCP=1` (the composite self-selects
its systematic generation submode; shipped params G=384, r=0.10) at
c7+c8, ×4 per seed, on the clean substrate (BBR + MTU floor + SR + PBS +
MP + anchors — every wall its 2026-07-08 refutation predates is now
fixed). Register-expected outcome: still worse than the default stack at
c8 (its refutation reproduced FMTCP's own abstract's slow-subflow
pathology, and the clean-substrate c8 story names the same structural
axis) → the register row becomes RE-TESTED/CONFIRMED-REFUTED and the
chain is cleared for deletion next consolidation pass. If it SURPRISES
(≥ the default stack anywhere ≫σ), that is recorded prominently and
re-opens its ADR instead.

**Battery (pre-registered).** VM protocol per MEASUREMENT DISCIPLINE
1–11: seeds 42+7, ×8 interleaved round-robin per rep (fmtcp ×4), fresh
tunnel per invocation, same binary every arm (sha256 + lscpu + env
recorded), per-arm echo assertions both directions (SR default; PBS/CAPW/
RS/FMTCP-warn per arm), same-session Σ singles per arm env, seed-7
topo-abort protocol (n recorded, nothing discarded), runtimes recorded.
Arms: legacy (`RWM_STORE_PATHS=0`) / pbs (env unset = shipped) / capw
(`RWM_STORE_CAPW=1 RWM_PLAIN_RS=1`) / fmtcp (`RWM_FMTCP=1`, c7+c8 only).
Cells: c7 (200 MB), c8 (25 MB), sc2/sc3 identity singles. Diagnosis runs
FIRST (c8 ×2 per pool arm, seed 42, gauges read before the battery).
FLIP `RWM_STORE_CAPW` (+ its `RWM_PLAIN_RS` dependency) default ON only
if prediction (b) holds on both seeds in both cells with singles inert;
else default OFF with the falsification/gauge outcome recorded.
Driver `tools/l1/c8pool_{diag,battery,all}.sh`.

*(Results below this line were written after the runs.)*

### The law as built (commit 080073c)

`capw_store_cap` (net/mod.rs): pool = clamp(Σ_i honest cap_i, floor,
N·knee) over LIVE paths (live_paths(), not the saturation-filtered
active_paths()), engaged only when the gate is on, N ≥ 2, and EVERY live
anchor is warm; else `None` → the caller's configured pooled law
(path-scaled / legacy) verbatim. Precedence over the hsum/path-scaled
laws at the dyn-cap refresh; per-path terms = `honest_store_cap` with the
existing `EchoRatioMin` K_i. Plus the diagnosis instrument: per-path
store-attribution gauge (`percap_track` = the percap account maps
maintained DIAG-only under pooled laws — behavior-inert, every percap
decision site keys on `percap_caps` non-empty; DIAG `sout=` now live on
pooled arms). Unit tests 4 new (lib 368/368): symmetric = N×single,
asymmetric weights by capacity (c8 shape 1648 strictly between 1024 and
4096, slow share ~24% not 50%), off/N=1/unwarm → None, over-read clamps
to the N×knee ≡ path-scaled. Suites: math 59+25, gate 15/15 release,
wedge, perf_loopback 8/8 (+ CAPW+RS forced), recov_mp/fmtcp loopbacks,
backpressure — all green.

### DIAGNOSIS RESULTS (VM 10.1.5.16, 2026-07-27 16:47 UTC; binary sha256 1d09eb3238faa48e… = commit 080073c; E5-2650 v3 aes+avx2+pclmulqdq; c8 ×2 per pool arm, seed 42, RWM_DIAG=1; log `/home/vibe/c8pool/diagnose-s42.log` + per-run diag/)

The per-path gauge answers the question and AMENDS hypothesis (a):

- **legacy-1024**: win pegged 954–1009/1024 (paused ~6%), and the pool is
  ~95% FAST-path-held — sout_fast 914–973 (max 1024), sout_slow 36–39
  falling to ≈0 after ~1.3 s. Goodput 85.8–87.4 ≈ the fast single + 2–4
  Mbit: legacy c8 is effectively the FAST PATH ALONE with the slow path
  starved of span.
- **path-scaled (shipped)**: the pool latches at the N×knee 4096 — the
  legacy plain anchor over-reads ×5–9 (btlbw gauge 51–115k sym/s vs true
  ~10.4k), so gain·N·Σpipe ≫ ceiling. The FAST path parks up to
  3810–3949 un-SACKed slots (mean 1535–2219) with its echo RTT inflating
  to 279–452 ms (RTprop 9–13; wire RTT to 272 ms), and goodput runs
  STALL-THEN-BURST: 15.7–33.4 Mbit while the span fills, 170–234 at
  release — the measured c8 bimodality (battery σ 5.5–17.5, one 33.9
  collapse run). The slow path holds only ~200–380.
- **capw (derived)**: with `RWM_PLAIN_RS=1` the anchors read ≈1× truth
  (btlbw 9.3–13.6k) and the pool computes LIVE at 1303–2548 (the derived
  ~1.65–1.75k class ± anchor warmth) — never latched. Occupancy and span
  sit between the incumbents; goodput sits between them too.

**Diagnosis verdict:** hypothesis (a) as stated is REFUTED — the slow
path never holds the depth (≤~10% of the pool in every arm); it is the
FAST path that parks the un-SACKed span. Hypothesis (b) is CONFIRMED and
is the mechanism: under SACK-release the pool bounds the UNACKED-FRONTIER
span, and every slot above ~the fast path's own honest pipe+runway
(~1024–1250) is frontier-stall exposure — holes in a 4× span recover
across a bloated queue while the cumulative frontier (= goodput) waits.
The premise "pool sizing binds at c8" is supported (goodput tracks the
span law monotonically: 1024 → fast-alone 0.87×Σ; 4096 → stall-burst
0.71–0.76×Σ), so the battery proceeded.

### L1 battery RESULTS (VM 10.1.5.16, 2026-07-27 16:49–18:01 UTC + attribution top-up 18:04–18:10; binary sha256 1d09eb3238faa48e… = commit 080073c, SAME binary every arm, built fresh on the VM (stale binary rm'd); E5-2650 v3 aes+avx2+pclmulqdq (post-divide) in every log header; 1 run/invocation, arms interleaved round-robin per rep ×8 (fmtcp ×4), fresh tunnel per invocation, seeds 42 AND 7, RWM_GEN=0 RWM_DIAG=1 everywhere; per-arm echo assertion (SR/PBS/CAPW/RS/FMTCP-warn, both directions): **0 completed-run liveness mismatches on either seed** (s42 104/104 battery + 12/12 top-up invocations completed; s7 battery 62 completed + 42 seed-7 topo-ping aborts, s7 top-up 3 completed + 9 aborts, every abort verified SUMMARY-LESS, n recorded per arm, nothing discarded); drivers `tools/l1/c8pool_{diag,battery,topup,all}.sh`, logs `/home/vibe/c8pool/{battery,topup}-s{42,7}.log` + per-run diag under `/home/vibe/c8pool/diag/` (17 MB preserved) + `BINARIES.txt`; lock `/tmp/rwm-vm.lock` held 16:43:05→18:14:56 UTC (waited out the LEVER-1 worker's hold 12:35–16:42, foreground polls). dnf=0 on ALL completed runs, both seeds. HARNESS NOTE (bookkeeping, not contamination): the battery's expected-echo matrix wrongly expected pbs/capw echoes ABSENT at singles — the INFO echoes print whenever the gate is configured while the laws are N≥2-gated (unit-tested N=1-inert; singles goodputs confirm) — the 48+31 "CONTAMINATION/FAIL" flags at singles/aborts are all this class or the abort class.**

Σ = same-session same-env singles (2×sc2 at c7; sc2+sc3 at c8), per arm.

**c8 (the target cell), mean ± σ_s (n) → vs Σ:**

| arm | s42 | vs Σ | s7 | vs Σ |
|---|---|---|---|---|
| **legacy** (`RWM_STORE_PATHS=0`) | **86.69 ± 3.30 (8)** | **0.866** | **87.85 ± 2.80 (4)** | **0.868** |
| pbs (shipped default) | 72.26 ± 17.52 (8, one 33.9 collapse) | 0.711 | 75.16 ± 5.52 (5) | 0.744 |
| capw (derived; +RS) | 79.13 ± 10.34 (8, low 58.1) | 0.794 | 74.17 ± 10.77 (6, low 58.2) | 0.743 |
| rs (top-up: pbs+RS, no capw) | 77.69 ± 6.60 (6) | 0.780 | 82.99 (2) | — |
| fmtcp (register re-test) | 14.30 ± 0.24 (4) | 0.141 | 15.03 (2) | 0.149 |

**c7 (symmetric preservation), mean ± σ_s (n) → vs Σ:**

| arm | s42 | vs Σ | s7 | vs Σ |
|---|---|---|---|---|
| legacy | 162.74 ± 2.92 (8) | 0.968 | 161.75 ± 2.87 (6) | 0.950 |
| **pbs** | **166.34 ± 2.91 (8)** | **0.973** | **166.15 ± 3.28 (7)** | **0.978** |
| capw | 143.94 ± 3.84 (8) | 0.860 | 143.78 ± 2.24 (4) | 0.859 |
| rs (top-up) | 139.40 ± 2.21 (6) | 0.833 | 142.77 (1) | — |
| fmtcp | 18.30 ± 0.76 (4) | 0.107 | 18.98 (2) | 0.112 |

**Singles (Σ terms + N=1 inertness):** sc2 legacy/pbs/capw =
84.05/85.47/83.67 (s42), 85.16/84.92/83.71 (s7); sc3 15.95–16.12 both
seeds. CAPW is N=1-inert as constructed; the capw arm's singles carry the
RS witness cost (−1.2…−1.8, the known −3–5 class at its mild end).
NOTE (honest session datum): the c7 legacy COLLAPSE CLASS (loo-pbs 3/8
runs at 86–97, "Consolidation") did NOT reproduce — 0/14 legacy c7 runs
collapsed this session; legacy trails pbs by only −3.6/−4.4 (~1.3σ,
consistent direction). The STORE_PATHS LOO defense stands on the
consolidation record; its collapse class is session-dependent (WATCHED).

### ATTRIBUTION (the top-up's answer — why capw lost c7)

The c7-capw pool cap computed 3.7–3.9k (≈ the pbs 4096, NOT binding:
occupancy only 410–594, per-path infl 33–48 vs pbs 126–254) — the
throttle is not the sizing law. The rs control (pbs pool + `RWM_PLAIN_RS`,
no capw) lands 139.4/142.8 = the capw class (143.9/143.8), both ≪ pbs
166: **the entire c7 regression is owned by the RS sampling composition,
not by the capacity-weighted law** (capw ≥ rs at every cell it shares).
NEW PRICED FINDING: the RS witness cost, −1.2…−1.8 at N=1 and resolved
in composition at c8 (rs 77.7/83.0 vs pbs 72.3/75.2), SCALES TO
−22…−27 Mbit ≫σ at the symmetric dual cell — the consolidation LOO's
named flip candidate ("carry RS as a full stack member") is REFUTED at
c7; `RWM_PLAIN_RS` stays default OFF, and any law that needs the honest
anchor inherits this dual-cell cost until its mechanism is found (named
follow-up; the gauge signature: sender not store-bound, win ≪ cap, infl
collapsed — an emission-side suppression under the sampling feed).

### VERDICT vs the pre-registration — prediction (b) FAILS; falsification (c) applies, honestly

- c8: the derived law, correctly engaged at its derived size
  (gauge-verified 1.3–2.5k, honest anchors, all-warm) and sitting exactly
  BETWEEN the incumbents as derived, LOSES to plain legacy-1024 on both
  seeds (−7.6/−13.7, direction consistent, ≫ legacy's σ 2.8–3.3; inside
  capw's own wide σ at s42). Target ≥0.87×Σ NOT reached (0.79/0.74 vs
  legacy's 0.866/0.868).
- c7: capw regressed ≫σ — but attributed to the RS composition (above),
  not the law; the law itself never bound at c7.
- **The binder is NOT pool sizing.** The gauge evidence names it: the
  slow path converts almost no pooled span into goodput at c8 in ANY arm
  (sout_slow ≤ ~10% everywhere; legacy c8 = fast single + only 2.6–2.7
  Mbit), so Σ_i-shaped pools — capw included — buy fast-path
  frontier-span exposure with no slow-path payback. The c8 data supports
  pool ≈ **max_i cap_i** (the fast path's own pipe+runway ≈ 1024–1250 ≈
  what legacy latches at by accident), and that law can only formalize
  the 0.85–0.87×Σ legacy already measures. **The true c8 binder, named:
  SLOW-PATH CONVERSION — placement + recovery at the asymmetric cell
  leave the c3 path's ~16 Mbit unbanked (the external bar: kernel
  MPTCP-BBR 89.7–92.6 banks it; rp legacy 86.7–87.9 = fast + 2.7).** The
  next pre-registerable item is a conversion mechanism (slow-path
  admission/recovery that turns span into goodput), not pool arithmetic.
- **FLIP: NO.** `RWM_STORE_CAPW` stays DEFAULT OFF (retained as the
  measured Σ-law arm with its unit-tested degenerates); `RWM_PLAIN_RS`
  stays OFF (c7 dual cost newly priced); the shipped default is
  unchanged. The c8 WATCH stands, sharpened: the shipped path-scaled
  pool remains the c8 worst pool law (0.71–0.76×Σ vs legacy 0.87×Σ, now
  4 sessions consistent) — a per-topology gate (heterogeneity detector →
  legacy-span law) is the mechanical fix the data supports, but it is a
  NEW item-11 build, not this session's flip.

### FMTCP re-test RESULTS (the register's owed arm) — CONFIRMED-REFUTED on the clean substrate

`RWM_FMTCP=1` (self-selected systematic generation submode, shipped
params G=384 r=0.10, activation warn = liveness echo on every run,
cod_share 1.02–1.17 — the coded plane very live), on the full clean
substrate (BBR + MTU floor + SR + PBS + MP + anchors — every wall its
2026-07-08 refutation predated is fixed): **c7 18.30/18.98, c8
14.30/15.03 — ×0.11/×0.20 of the same-session default stack, strictly
worse than EVERY plain arm at both cells on both seeds, ≫σ; dnf=0 (it
delivers, slowly) with cod_share >1 (more repair than source — the
recovery-flood class) and ~8× the plain arms' CPU (45 s/200 MB at c7).**
The 2026-07-08 pathology (the slow-subflow/decode-on-total amplification,
FMTCP's own abstract's failure mode) reproduces with every wall removed —
the refutation was NEVER wall-tainted. Register row updated:
RE-TESTED → CONFIRMED-REFUTED; the chain (`RWM_FMTCP`, `RWM_FMTCP_WIN`,
its forced sub-levers) is CLEARED FOR DELETION at the next consolidation
pass. No ADR re-opens.

Ops: lock held 16:43:05–18:14:56 UTC (2 polls/5 min class while the
LEVER-1 worker held it 12:35–16:42); CRLF converted after sync
(discipline 10); rp-* netns only, torn down; stale binary removed before
the fresh build; battery/topup/diagnosis logs + 17 MB per-run diag + the
binary hash preserved under `/home/vibe/c8pool/`; s7 abort count (42
battery + 9 top-up, all summary-less) recorded above; runtimes: s42
battery 43 min/105 invocations, s7 26 min/104, top-up 6 min.

## Lossy-Single Residual (2026-07-27) — PRE-REGISTRATION (discipline item 11 — this block written BEFORE the instrumented runs; branch `diag/lossy-residual` from 44dd7d4; DIAGNOSIS-FIRST: the ACCOUNTING TABLE is the deliverable; a fix ships only if the table names a dominant AND cheaply-fixable term, under its own pre-registered prediction appended before any battery)

**(a) The question.** LEVER 2/3 of the competitive-baseline losses: where do
the missing 9–14% go on lossy SINGLE paths? The external bar ("Competitive
Baseline", same VM/cells/seeds): c2 rp 78.6–78.7 vs quinn-bbr 91.9–92.4
(−14%); c3 rp 16.1 vs quinn-bbr 18.6 / tcp-bbr 17.5–17.8 (−9…−13%). The
candidate terms, EACH to be priced in Mbit/s from instrumented sc2/sc3 runs
(new DIAG-only gauges this branch: sender emission-gap `sidle=` + cumulative
`cum=src/cod/ack` in [DIAG]; receiver inter-arrival `[WIDLE]`; QDISC
wire-byte/pkt/drop echo in perf_rwm_c.sh — plus the existing DIAG/SPAN/RDIAG
surface):

1. **Framing/MTU tax (structural).** rp rides the ADR-0055 1350-B MTU floor
   with a ~1200-B source payload per ~1340-B IP packet (wire efficiency
   ≈ 0.89); quinn MTUD reaches ~1452 on the same veth (≈ 0.96). Derivation
   re-read, self-contained prediction: this alone is ~4–6 Mbit of the c2 gap
   and ~0.8–1.2 Mbit at c3, and it is NOT a this-session fix (the floor is
   the blackhole defense; a symbol-size/MTU-scaled-payload raise is a named
   roadmap item). The QDISC byte counters price it exactly.
2. **Object-scale ramp at the bar's 25 MB geometry.** Known class: rp 78.7 at
   25 MB vs its own 100-MB steady 84–85 (vs quinn's quasi-steady sequential
   uploads) → ~6 Mbit at c2. Re-measured same-session (25 MB AND 100 MB arms
   at both cells).
3. **FEC overhead.** Emitted cod/src (cod includes retx by counter
   semantics). Prior: r* ≈ 0.03 at c2-class loss → ~2.5–3 Mbit; the #46-era
   question — is the taper emitting MORE than r* under the unified span law
   (spare-capacity cap read from an over-reading anchor)?
4. **Recovery wire idle.** Idle wire during hole-recovery rounds (SACK sweep
   clamp [25,100] ms; BBR keeps the wire full through loss). Gauges: [WIDLE]
   (wire truth), sidle (engine handoff), GOODSERIES microstructure.
5. **Anchor over-read → store-cap bloat → retx queue delay.** The legacy
   plain anchor over-reads ×4.6–7.4 (§16.21) so the dynamic cap latches at
   RELIABLE_STORE_MAX=1024 at c2 (~4.4×BDP) and ~684+ at c3 (~9×BDP); the
   excess outstanding sits in quinn's FIFO datagram queue, so every
   retransmit waits behind it (~60 ms class at c2, ~285 ms at c3, derived).
   Probes: static RWM_STORE arms bracketing the honest cap (c2 256/512, c3
   192/384) + the RWM_PLAIN_RS honest-anchor arm (known −1.2…−1.8 witness
   cost at singles). Honest prior: the RS arm measured ≈neutral at sc2, so
   the expected static-store effect is SMALL at c2 (±1–2 Mbit); c3 (deeper
   relative bloat, longer recovery rounds) is the cell where this term can
   be large. A null is informative (prices the queue-delay term ≈ 0).
6. **Per-message engine cost.** Predicted NON-BINDING: c2/c3 rates are
   ~9.8k/2.4k sym/s ≪ the 22–23k msgs/s receiver wall (§16.23). Verify:
   RDIAG busy% + CPUSRV/CPUCLI (expect busy ≪ 100%, CPU/wall ≪ 1 core).

**(b) Predictions (pre-registered).** (i) The table CLOSES: terms 1–6 sum to
the measured residual within the session noise floor at both cells. (ii) At
c2, structural terms (framing + ramp + FEC) cover ≥ ~2/3 of the −13-to-14
gap; the actionable mechanism residual (idle + queue-delay) is ≤ ~4 Mbit.
(iii) At c3, framing + FEC cover ~half; the rest is recovery idle/queue
delay. (iv) Receiver busy < 50% both cells. (v) If (iii)'s
idle/queue-delay term is large AND the static-store probe moves sc3 ≫σ, the
named fix candidate is store-cap honesty at singles — to be pre-registered
separately before any battery.

**(c) Falsification / verdict rule.** If the accounting does NOT close
(unexplained remainder > ~2σ of the cell), the remainder is reported as
UNACCOUNTED — no invented term, and the table still ships as the deliverable.
A fix is built ONLY for a term that is (i) dominant (≥ half the cell's
actionable residual) and (ii) cheaply fixable this session; everything else
is named + sized for the roadmap. A probe arm that regresses ≫σ is itself a
datum (the incumbent law is protective), not a tuning invitation.

**(d) Derivation re-read for self-contained failure predictions.** Terms 1–2
are arithmetic/measured priors, not mechanisms — they cannot close the whole
gap (1+2+3 ≈ 12–15 Mbit at c2 would OVER-close it; the table's job is the
honest split, and over-closure would itself falsify the naive sum, naming
double-counting between ramp and idle — the ramp IS partly idle at object
edges, so the 25 MB vs 100 MB split must be read as geometry, not added to
idle blindly). At c3 the derived retx-queue-delay bound (~285 ms/retx) is
close to one [25,100]-ms sweep + one inflated RTT — if measured echo-RTT
stays ~RTprop-class, term 5 is already refuted at c3 and the store probes
should read null.

**Battery (pre-registered).** VM 10.1.5.16 per MEASUREMENT DISCIPLINE 1–11:
lock priority 1 (a parallel streaming-retest worker polls behind); CRLF
after sync; FOREGROUND polling only; rm stale binary; sha256 + commit +
lscpu + env headers; diagnosis `tools/l1/lossy_diag.sh` seed 42 ×2/arm
(arms: sc2/sc3 × 25M/100M defaults + static-store + plain-rs probes). IFF a
fix ships: sc2 (100 MB) + sc3 (25 MB) default ↔ fix, seeds 42+7 ×8
interleaved, fresh topology per invocation, dnf recorded; tail_matrix c2
spot ×4 iff the fix touches emission. No flip without the fix's own
pre-registered prediction holding on both seeds; suites green.

*(Results below this line were written after the runs.)*

### DIAGNOSIS RESULTS (VM 10.1.5.16, 2026-07-27 18:56–19:02 UTC; binary sha256 e8a0af12c971b9b5… = commit e6f0859, built fresh (stale rm'd), CRLF-converted; E5-2650 v3 aes+avx2+pclmulqdq; kernel 7.0.14-101.fc43; seed 42; `tools/l1/lossy_diag.sh` — logs `/home/vibe/lossyres/diagnose-s42.log` + per-run diag/)

**Ops incident, recorded first:** the 20-invocation rapid-fire tripped a
flake class — server bind `Address already in use` cascades (pkill racing
the next invocation) plus silent early process deaths (no OOM, no panic in
logs) — 8 invocations lost (wedge signature: warmup never acked, CPUSRV=0,
sweeps=retx). Completed runs are clean (liveness echoes + qdisc counters
consistent); the battery driver is retry-hardened (`lossy_battery.sh`:
port-free wait + ≤3 attempts, aborts preserved). Also an instrument
caveat: the new [WIDLE] 3 ms inter-arrival gauge over-counts at c3 — GRO
delivers ~13-datagram batches ≈ 7 ms apart at 1.8k pkt/s, so its "idle"
includes batching cadence; the QDISC byte counters are the idle authority.

**Wire truth (qdisc counters, completed runs):**
- sc2-def-100M: 116.25 MB / 93 983 pkt in 9.4534 s = **98.4 Mbit/s on the
  100 Mbit wire — idle ≤ 1.6%**; drops 580 = **0.61% realized** (GE nominal
  2.53%, seed-42 realization); mean data pkt ≈ 1319 B wire per 1200 B
  payload → **framing efficiency 0.910** (quinn MTUD ~1452 ⇒ ~0.957).
- sc3-def-25M: 30.94 MB / 27 670 pkt in 12.4621 s = **19.87 Mbit/s of
  20 — idle ≈ 0**; drops 510 = **1.81% realized** (nominal 4.76%).
- Goodputs (this session): sc2-def 25M/100M = 82.86/84.63; sc3-def-25M
  16.05; probes: sc2-rs 85.17, sc3-rs 15.26, sc3-s384 14.77 (sc2 static
  arms lost to the flake class).

**Engine truth ([DIAG]/[SPAN] cumulative):**
- sc2-100M: src 83 870, cod 3 764 (4.5%), retx 3 313 — vs ~580 real drops:
  **×5.7 over-fire; mpr y=2659 (80%) fired on flights YOUNGER than the
  hole law's own threshold, supp_law=0 — the RFC9002 law is `mp_n_paths>1`-
  gated and INERT at singles**. Ripe fires 654 ≈ the realized drop count.
- sc3-25M: src 21 152, cod 2 817 (13.3%), retx 2 556 vs ~510 drops (×5.0;
  y=1572, ripe 984); 145 472 gap-seqs walked (the receiver re-advertises
  holes ~each [25,100] ms sweep while recovery crosses the bloated queue);
  age-at-fire 357 ms.
- **The taper emits ≈ NOTHING at singles**: [SPAN] rr=0.000 owed=0.00
  everywhere; sender per-path `pl` reads 0.0000–0.0102 at 2.5–4.8% cells
  (the sender estimator is loss-blind on singles) — so the #46-era
  "emitting MORE than r*" is REFUTED in the opposite direction: proactive
  FEC is dead; ALL recovery overhead is the reactive plane.
- Anchor over-read at singles CONFIRMED: btlbw gauge 50.5–67.0k sym/s at
  sc2 (true ~8.9k → ×5.7–7.5), 12.9–18.4k at sc3 (true ~1.8k → ×7–10) ⇒
  dyn store cap latched at 1024 ⇒ standing queue ON the wire: echo/wire
  RTT 109–111 ms vs RTprop 13 (sc2), 528–558 vs ~45 (sc3).
- Receiver service: RDIAG busy 43–45% (sc2) / 8–10% (sc3); CPUSRV 7.0 s /
  9.45 s wall (sc2-100M) — **term (e) NON-BINDING, confirmed**.

### THE ACCOUNTING TABLE (each term priced; gap = rp vs quinn-bbr same cell)

**sc2 steady (rp 84.6–85.2 vs quinn-bbr 91.9–92.4 → gap ≈ 7.3 Mbit):**

| term | Mbit/s | evidence |
|---|---|---|
| (1) framing/MTU tax (structural) | **~4.3** | 1319 B wire / 1200 B payload (×1.099) vs quinn ~×1.045: qdisc bytes |
| (2) spurious retransmissions | **~2.7** | 2659 y-fires × 1319 B / 9.45 s = 2.97 Mbit wire × 0.91 framing |
| (3) honest retx + margin + control | ~0.9 | 654 ripe + ~450 margin + 6.3k ctrl pkts |
| (4) wire idle (b) | ≤ 1.0 | 98.4 of 100 Mbit qdisc |
| (5) engine service (e) | 0 | busy 43–45% |
| Σ vs gap | 7.9–8.9 vs 7.3 | CLOSES (overlap: (3) partially present in quinn's own ~2.9% overhead; (4) partially BBR-probe-shared) |
| (+) 25 MB bar geometry ramp | +1.7 this session (historic to +6) | 82.9 vs 84.6 same session |

**sc3 25 MB (rp 16.05 vs quinn-bbr 18.6 → gap ≈ 2.55; tcp-bbr 17.5–17.8):**

| term | Mbit/s | evidence |
|---|---|---|
| (1) framing/MTU tax (structural) | **~0.95** | same ratio on the 19.87 Mbit wire |
| (2) spurious retransmissions | **~1.7** | ~2046 above-honest fires × 1319 B / 12.46 s × 0.91 |
| (3) honest retx delta vs quinn | ~0 | realized loss identical on the shared netem |
| (4) wire idle | ~0 | 19.87 of 20 Mbit |
| (5) engine service | 0 | busy 8–10% |
| Σ vs gap | 2.65 vs 2.55 | **CLOSES** |

**Verdicts on the pre-registered candidates:** (a) FEC overspend REFUTED
(r* consumed ≈ 0; overhead is reactive); (b) recovery idle REFUTED as a
term (the bloated window is accidental wire insurance — the waste keeps
the pipe "full"); (c) anchor over-read CONFIRMED alive and priced
INDIRECTLY: the 1024-latch queue (100/500 ms) is what makes re-fires
spurious (flights still queued read as lost) and what the static-store
probes cannot fix (sc3-s384 = 14.77: a right-sized window IDLES the wire
12% during stalls — window-vs-inflight decoupling is the structural
successor, roadmap); (d) app-layer conservatism REFUTED (the opposite:
4–13×BDP); (e) non-binding CONFIRMED. Prediction (b)(i) HELD (the table
closes); (ii) HELD (structural ≈ 4.3 + 2.7 spurious ≥ 2/3); (iii) HELD.

### FIX PRE-REGISTRATION — `RWM_RECOV_SP` (written BEFORE the battery; default OFF)

**Mechanism.** The dominant actionable term at BOTH cells is the spurious
reactive plane (×5.0–5.7 over-fire). Root cause, named by the y-class: the
RFC 9002 §6.1.2 time-threshold hole law (`recov_mp_law`) is gated
`mp_n_paths > 1` on the premise "single-path gaps are FIFO-real" — refuted
on a jittery substrate (netem delay-jitter reorders tens of packets; gap
reports name merely-late seqs; re-fires chase flights still crossing the
store-cap standing queue). The fix: apply the SAME law at N = 1 — a gap
seq with a live flight fires only at age ≥ 9/8×max(smoothed clocks)
(`mp_time_threshold_us`, no new constants); TIME channel only (the §6.1.1
packet channel is excluded at N=1: reorder depth ≫ kPacketThreshold);
suppression-only (the receiver hole-refresh re-advertises).

**Predictions.** (1) Mechanism gauge: fired collapses from ×5.0–5.7 to
≤ ~2× realized drops; supp_law absorbs the former y class. (2) sc2 100 MB:
**+2 to +3 Mbit** (→ ~87–88) both seeds, ≫ σ_s (~0.7–1.0). (3) sc3 25 MB:
**+1 to +1.7** (→ ~17.0–17.7, the tcp-bbr class), ≫ σ_s (~0.1–0.2).
(4) dnf = 0; no cell regresses ≫σ.

**Falsification.** Either cell regressing ≫σ (first-retx delay
serializing recovery outweighs the reclaimed waste) REFUTES the arm →
default OFF, register row, no tuning. Goodput flat WITH the gauge
collapsed ⇒ the freed wire went idle, not to goodput — the window/inflight
coupling is then the named binder, and no flip happens on a wrong
attribution.

**Flip rule.** Default ON only if (1)–(4) hold on BOTH seeds AND a
tail_matrix c2 spot ×4 is unregressed (the fire path is shared with the
tunnels' reliable plane) AND suites stay green. Battery:
`tools/l1/lossy_battery.sh` — sc2-100M + sc3-25M, def ↔ sp interleaved
per rep, ×8, seeds 42+7, fresh topology per invocation, RWM_DIAG=1, n
quoted per arm, aborts preserved.

*(Battery results below this line were written after it ran.)*

### L1 BATTERY RESULTS (VM 10.1.5.16, 2026-07-27 20:52–21:16 UTC; binary sha256 ef6ed448…= commit 982b1a0, built fresh (stale rm'd), same binary every arm; E5-2650 v3 (post-divide); arms def ↔ `RWM_RECOV_SP=1` interleaved per rep, fresh topology per invocation, 1 run/invocation, RWM_GEN=0 RWM_DIAG=1; liveness echo asserted per arm (sp=1 in every sp run, 0 in every def run); drivers `tools/l1/lossy_battery.sh`; logs `/home/vibe/lossyres/battery-s{42,7}.log` + `battery-s7-pass1.log` + per-run diag; runtimes s42 7 min (32/32 clean, 0 retries), s7 pass-1 5 min + top-up 6 min)

**Incidents (recorded first).** (i) s7 pass-1: 13 seed-7 topo-ping aborts
(the known class; ~3 s marker-to-marker, pre-run) AND the driver's retry
check was defeated by the stale-client-log summary (discipline-8's
stale-log class, new instance) — fixed mid-session (rm logs per attempt,
committed), pass-1 completed runs kept (n quoted), top-up ×5 run with the
fixed driver (12 RUN-RETRY recovered, 2 RUN-LOST after 3 attempts).
(ii) VM co-tenancy: the p2 streaming-crown battery was mid-flight at my
first lock take (its hold predated mine and its controller launches
stages without re-polling); one of its realtime tunnel pairs was killed
at ~19:14 UTC by my pre-battery cleanup before I understood its session
was live — ITS crown-s42 log carries that dead rep (flagged in
`/home/vibe/crown/`). I then yielded and waited out its full battery
(19:07–20:45 UTC), re-took the lock 20:48, and ran on a quiet VM.

**Goodput (Mbit/s, mean ± σ_s (n); merged s7 = pass-1 + top-up):**

| cell | def (s42) | sp (s42) | def (s7) | sp (s7) | verdict |
|---|---|---|---|---|---|
| sc2 (c2 single 100 MB) | 84.82 ± 0.80 (8) | 85.60 ± 0.89 (8) | 85.04 ± 0.75 (8) | 85.09 ± 0.93 (12) | **TIE** (+0.78 ≈ 1σ s42; +0.05 s7) |
| sc3 (c3 single 25 MB) | 16.13 ± 0.12 (8) | **16.45 ± 0.20 (8)** | 16.14 ± 0.06 (7) | **16.48 ± 0.12 (10)** | **+0.32/+0.35 ≫ σ_def, BOTH seeds** |

dnf = 0 in all 69 completed runs.

**Mechanism gauge (rep-8 class, s42):** the law is LIVE and does what it
says — y (young fires) 2739→**0**, supp_law 0→12 386 (sc2) / 1606→0 with
supp_law 18 404 (sc3) — but total fired only drops 3615→2485 (sc2) /
2542→1928 (sc3), i.e. −24…−31%, NOT the predicted collapse to ~2× drops:
the y-class was not one-shot spuriousness but a QUEUE-SUSTAINED RE-FIRE
LOOP — each retransmit crosses the store-cap standing queue (~110 ms at
c2 / ~350–500 ms at c3), so the hole stays open past every 9/8×SRTT
threshold and legitimately re-ripens until the frontier passes. Wire
truth: sp trims 1.1 MB (sc2) / 0.7 MB (sc3) of wire per object at equal
drops; at sc3 ~70% of the freed wire converts to goodput, at sc2 it
vanishes into the BBR-probe/idle margin.

### VERDICT vs the fix pre-registration — predictions (2)+(3) FAIL; NO FLIP

- (1) gauge: PARTIAL (y→0 and supp_law live as predicted; fired does NOT
  reach ≤2× drops — attribution amended above). (2) sc2 +2…3: **FAILED**
  (tie both seeds). (3) sc3 +1…1.7: **FAILED** (+0.32/+0.35 — real, ≫σ,
  consistent, but ~¼ the band; 16.45–16.48 vs the predicted ≥17.0).
  (4) dnf=0: PASS.
- **FLIP: NO — `RWM_RECOV_SP` ships DEFAULT OFF**, retained as a measured
  arm (the only ≫σ singles-goodput lever this session: +0.3–0.4 at sc3 on
  both seeds, tie at sc2, zero regressions). Per the falsification clause
  the binder behind the residual is NAMED, not tuned at:
  **window/inflight coupling** — the over-read-latched 1024 store cap is
  simultaneously (i) the standing queue that keeps every hole's recovery
  crossing 100–500 ms (sustaining the re-fire loop the law cannot
  legally suppress) and (ii) the only thing keeping the wire full through
  frontier stalls (the sc3-s384 probe: honest-sized window → wire idles
  12%, goodput 14.8). BBR-class stacks decouple these; our window is
  both. That decoupling is a NEW pre-registerable build, not this
  session's.

### Roadmap (named + sized, from the closed accounting)

1. **MTU/payload scaling** (structural): the 1350-floor/1200-payload
   framing tax is ~4.3 Mbit at c2, ~0.95 at c3 — the single largest c2
   term. Candidate: scale symbol payload to the MTUD-verified path MTU
   (keep the 1350 floor as the blackhole defense). Up to +4/+1 Mbit.
   **[EXECUTED 2026-08-06, "Window Decoupling + MTU Scaling" part 2:
   the tax is mostly rp's own 65-B framing, not the MTU — the v5
   compact frame banks +2.6/+3.6 at sc2 and +0.55/+0.60 at sc3 ≫σ both
   seeds and FLIPPED default ON (`RWM_WIRE_COMPACT`); the two literal
   MTU options refuted-by-derivation / roadmap-with-price.]**
2. **Window/inflight decoupling at lossy singles**: keep the wire fed
   through frontier stalls WITHOUT retaining a 4–13×BDP un-SACKed span
   (candidates: retx priority lane ahead of the fresh-symbol queue;
   honest inflight target + spare-capacity filler). Sized: the remaining
   reactive-plane overhead above honest retx ≈ 1.5–2 Mbit at c2,
   ~1.0–1.4 at c3 (post-RECOV_SP residual).
   **[EXECUTED + REFUTED 2026-08-06, "Window Decoupling + MTU Scaling"
   part 1: the decoupled law killed the queue (echo 108→27 / 520→230
   ms) and the goodput did not follow (sc2 −1.76/−0.37, sc3 tie) —
   fired stays ×3.3–4.2 with the queue gone, so THIS section's
   "queue-sustained re-fire loop" attribution is AMENDED: the re-fires
   are re-serve-clocked (hole re-advertisement + per-seq cooldown).
   Register row; the reactive-plane residual's owner is the re-serve
   clock, not the window.]**
3. **Sender loss-estimator honesty at singles**: per-path `pl` reads
   0.000–0.010 at 2.5–4.8% cells ⇒ r* = 0 ⇒ the proactive plane is dead
   at singles; whether funded proactive r* beats reactive-only at bulk is
   an open item-11 question (it did NOT bind this session's gap — the
   waste was reactive).

Ops: lock takes 2026-07-27 ~18:48 (yielded, see incident ii) and
20:48–21:20 UTC, released after teardown; rp-* netns torn down, no stray
processes; CRLF converted after each sync; stale binary removed before
each build; binaries e8a0af12… (diagnosis, e6f0859) / ef6ed448… (battery,
982b1a0), sha256 + lscpu + kernel in every log header; all logs + 1.4 MB
per-run diag preserved under `/home/vibe/lossyres/`; seed-7 abort ns
recorded above; suites on the fix commit: lib 368/368, math 136
(59/19/22/4/4/3/25), gate_suite 15/15 release, gates-default test pins
`recov_sp=false`.

## Anchor Hygiene (2026-07-19) — the convergent anchor-defect family FIXED as one workstream (branch `feat/anchor-hygiene`, commit 988960c): A\* live in ~1 RTT (was pinned ~10 s+) and flood-poison-proof; the M\* 50-ms floor was the PEER-REPORT feedback loop and with it fixed the PART 7b knee ENGAGES at L1 (r100 +25/+31%, r200 +62/+82%); plain-mode BtlBw reads ≈1× truth (was ×4.6–7.4); post-stall estimator poisoning discarded by a PROCESS-clock stall witness (the arrival-clock design REFUTED by measurement). All default-OFF (`RWM_ANCHOR_HYGIENE` umbrella); shipped path byte-identical

*Decision record: → [ADR-0061](adr/0061-anchor-hygiene.md)*

Three investigations ended at the same defect family: the COLLAPSE
ATTRIBUTION (designs A+B: A\* pinned/poisoned), the #61 knee battery (the M\*
anchor pair), and the percap GUARD RESULTS (residual (i): the plain-anchor
over-read). This build fixes the family as ONE workstream under one
principle (paper §16.21).

**THE PRINCIPLE.** An anchor is trustworthy only if (1) it is SEEDED from
measured sends — a windowed statistic of real samples, live within ~1 RTT,
never a static default surviving warm-up; (2) its samples EXCLUDE scheduler
clock gaps — a sample whose interval spans a process stall measures the
stall, not the link (detect on the PROCESS clock; discard, don't average);
and (3) its floors/backstops EXPIRE — a floor that outlives its min-window
is a constant wearing a floor's clothes. Every defect below violates one.

**The four fixes (each env-gated for A/B; `RWM_ANCHOR_HYGIENE=1` = all).**

1. **A\* rate anchor** (`RWM_ASTAR_ANCHOR`; rules 1+2). The span law's rate
   was `est.throughput()` — a 2-s-interval α=0.125 EWMA of the report-tick
   send rate. Now: `control::anchor::SendRateAnchor`, a windowed-max
   send-rate (bucket ≈ SRTT/2 clamped [5, 250] ms; window ≈ 8·SRTT clamped
   [0.5, 10] s) fed by the sender's own send events at the span site; a
   bucket whose Δt is a clock gap (`is_clock_gap`: > max(8×expected,
   250 ms)) is DISCARDED and a quarantine = min(gap, 2 s) swallows the
   release-flood buckets; through the disturbance the window HOLDS its
   pre-gap max (no collapse to "no sample").
2. **M\* anchor pair** (`RWM_MSTAR_ANCHOR`; rules 1+3). ROOT CAUSE of the
   50-ms floor: `PathReport.avg_rtt_us` carries the PEER'S ESTIMATOR VALUE
   — a 50-ms-seeded EWMA that a pure receiver NEVER feeds with a
   measurement — and the PathReport arm recorded it as a local RTT sample
   every ~2 s, re-planting a perpetual 50-ms "sample" inside the 10-s
   min-RTT window (the #61 `rtp=50ms` floor-freshness FAIL, reproduced
   in-session in the U1 arm's DIAG at both knee cells). Fix: estimates are
   not samples (the report keeps keepalive/monitoring/loss roles); the
   local RTT EWMA seeds from its FIRST measured sample (no 50-ms blend);
   the gen-pipe delivered-rate filter seeds from 500-ms buckets (ring 8,
   same ~4-s window class) instead of a 2-s pin; and the static
   `(pipeline+2)·G` FMTCP win backstop becomes the DERIVED `(M*+2)·G` once
   anchors are live (cold-start M\*=2 reproduces the legacy 4·G exactly, so
   the static value's reign is bounded to ~one rate bucket; explicit
   `RWM_FMTCP_WIN` still wins).
3. **Plain-mode send-interval sampler** (`RWM_PLAIN_RS`; rule 1). The #79
   BBR sampler (RsPacket snapshots, Δt = max(send_elapsed, ack_elapsed),
   windowed max, app-limited exclusion) generalized to plain
   window-reliable mode under ANY substrate CC, by running the Copa-feed
   WindowAck frontier/SACK attribution machinery in a new SAMPLING-ONLY
   CopaFeed mode: rate samples flow, but cwnd dynamics keep the legacy
   per-batch-Ack call site/cadence (minus the polluted ack-interval
   `record_delivery` sample) and the store-cap/percap laws stay on their
   legacy branches (`owns_cc()`). The Copa-sole feed is unchanged.
4. **Post-stall hygiene at the shared sampling layer** (`RWM_CLOCK_GAP`;
   rule 2). A process-global `StallWitness`: a dedicated 50-ms timer tick;
   a tick interval ≫ the period (same `is_clock_gap` predicate — factored
   ONCE) is a whole-process stall, and the ack feed sites
   (Ack/WindowAck/PathReport arms + the report-tick throughput feed)
   discard samples during the quarantine (budget release and loss counts
   are NOT discarded — they stay valid). **Negative result, recorded:**
   the first implementation detected gaps on the ACK-ARRIVAL clock
   (median, then p90 cadence statistic) and was REFUTED at an r200 gen L0
   rung — ack silences of 0.5–3 s are NORMAL protocol behavior there
   (frontier waves, deficit rounds), and the detector quarantined exactly
   the post-recovery ack waves carrying the true rate (measured discard
   storms gapd 7/2487 and 9/5578 during healthy transfer). The process
   clock cannot be fooled this way: ack silence with a live process never
   trips a timer.

**Unit evidence (lib 350/350 green, tree of 988960c).** The flood-poison
injection (`send_rate_anchor_flood_poison_injection_does_not_move_the_max`:
steady 150 sym/s, synthetic 1-s gap + 150-send backlog flood ⇒ anchor moves
< 20% and re-measures truth after quarantine; the gap is DETECTED);
seeding (`send_rate_anchor_seeds_from_first_measured_sends`: truthful
within one SRTT of stream start); witness law
(`stall_witness_quarantines_process_stalls_not_ack_silences` +
`stall_witness_quarantine_is_capped`: steady ticks never quarantine, a 1-s
stall quarantines ≈ its own length, expires, no cascade); estimator seed
(`rtt_seeds_from_first_measured_sample_under_hygiene` + the legacy-blend
control); backstop coupling
(`fmtcp_backstop_couples_to_derived_depth_after_cold_start`: cold start =
legacy 1536 exactly); `sampling_only_feed_does_not_own_cc`. Plain-sampler
correctness under batched acks + a standing queue was already law
(`rate_sample_anchor_reads_true_btlbw_under_aggregation_and_queue`) — fix 3
reuses that machinery verbatim. N=1/identity: all gates default OFF; with
the env unset every feed site takes its legacy branch (lib suite green on
the same tree with no env set).

### L0 mechanism evidence (dev box — its own hardware era; same-session interleaved arms, test binary from 988960c)

- **A\* trajectory (unified_stream_l0, c3-1200B, RWM_DIAG [SPAN] trace).**
  Base arm: `a_star=1` at EVERY 500-ms sample of the whole 20-s stream
  (the EWMA never lifted it — defect (i) live). Hygiene arm: a\*=6 by the
  second trace sample (t≈0.6 s), settling at its derived value 3 for the
  stream (anchor rate ar≈64–66 sym/s steady, gap counters 0/0 on a quiet
  box). Same cell, same seeds, same binary.
- **Quiet-box stream battery (14 seeds × {U, U+AH}, interleaved).** 0
  outage-class reps in EITHER arm (the trigger is environmental and was
  absent); every message delivered in all 28 reps; p50 identical (47.5 ms);
  **U+AH's p90 is systematically lower — median-of-reps p90 78 vs 94 ms**
  (13/14 seeds ≤ base; the live span converts ARQ round-trips into
  in-window FEC recovery — the ru/rf≈9% inertness closing).
- **Interference stream batteries (same interleaved arms under concurrent
  compile-class host load — the attribution's on-demand trigger).** Two
  passes: 14×2 under a light build loop (mettle crate) and 6×2 under a
  heavy loop (full raptorpath release rebuild, repeated). HONEST RESULT:
  the collapse class did NOT reproduce today — 0 outage-class reps in
  BOTH arms across all 68 local reps (quiet+light+heavy; the attribution
  session's box produced 3/14 under its background load; today's did
  not, even loaded — the trigger is environmental and was absent). What
  the loaded passes DID show: U+AH's p90 advantage persists (median 78
  vs 87–94 ms), and the single disturbed rep of the day (heavy-load
  s42-U: p99 903 ms, max 1089 — a ~1-s episode) landed in the BASE arm
  and did not chain, while the same-session U+AH rep held p99 126 ms —
  suggestive of the amplification removal, but n=1 and NOT claimed as
  more than that.
- **Plain-sampler shim truth check (gen_substrate_l0 plain mode, RWM_DIAG,
  mid-transfer btlbw vs the shim's configured rate).** c3: base 11,383
  (×5.5 over) → PRS **2,144 = 1.03× truth** (ANCHOR attr/gen ≈ 4.8k —
  sampler live). c2: base 82,785 (×7.9 over) → PRS 3,946 — an honest read
  OF THE ACHIEVED rate (~1.0× its own ~19-Mbit delivered rate; the L0
  shim run self-limits under the honest anchor: samples cannot read above
  what flows — the same anchor⇄cap circularity, stronger at L0 where the
  shim caps the loopback). The L1 check above (real substrate) is the
  verdict-carrying row: 1.02× truth at line rate.
- **Gen-substrate r200-class rung (`custom:100;100;3;1.3;50`, a
  NEVER-BEFORE-RUN L0 cell): mechanism liveness only.** [GPIPE] shows M\*
  climbing 2→9 on honest rtprop ≈ 200–216 ms under the gate (vs 2→3–4 on
  a 120–190-ms under-read without), and the witness records 0 false
  quarantines (gapd 0/0) — but the CELL ITSELF carries a pre-existing
  wedge class in BOTH arms locally (base 3/10 DNF across passes; the
  hygiene arm wedges too, `win=977/768` over-cap stall signature), so no
  local throughput verdict is drawn. The L1 knee battery below is the
  arbiter — it ran 64/64 with 0 DNF.

### L1 RESULTS (VM 10.1.5.16, 2026-07-19 ~16:00–17:10 UTC+2; binary sha256 e17df72b7641… = commit 988960c, SAME binary every arm; host-passthrough E5-2650 v3, aes+avx2+pclmulqdq in every log header (post-divide); arms interleaved per rep, fresh tunnel per invocation, seeds 42+7, RWM_DIAG=1 everywhere; drivers `/home/vibe/anchorhyg/{ah_all,ah_knee,ah_plain}.sh`, logs `/home/vibe/anchorhyg/{knee,plain,tailah}-*.log` + per-run `diag-*`; lock `/tmp/rwm-vm.lock`)

**(a) The #61 knee re-run** (`perf_rwm_c.sh c2r100|c2r200 … bulk 25 MB
single`, gen-sys wire, GPB stack `RWM_GEN_R=0.03 RWM_QUIC_CC=bbr`; arms U1 =
`RWM_UNIFIED=1 RWM_GEN_PIPE=1` (the #61 M\*-law arm, hygiene OFF) vs U1AH =
U1 + `RWM_ANCHOR_HYGIENE=1`; ×8 interleaved, n=8/8 everywhere, 0 DNF; U1AH
liveness echoes on every rep, `rtp=50ms`-class DIAG reproduced on U1 reps,
`rtp=100/200ms` + [GPIPE] M\* 2→5..9 on U1AH reps):

| cell | U1 mean Mbit/s (σ_s) s42 · s7 | U1AH s42 · s7 | Δ | m=2/M\* ratio (PART 7b predicts) |
|---|---|---|---|---|
| c2r100 | 36.49 (3.51) · 38.84 (1.10) | **47.86 (2.20) · 48.48 (2.82)** | +31% / +25% | 0.76 / 0.80 (0.64) |
| c2r200 | 19.18 (0.74) · 20.30 (0.79) | **34.85 (2.16) · 32.94 (2.95)** | +82% / +62% | 0.55 / 0.62 (0.39) |

**The knee ENGAGES.** At r200 the per-rep distributions do not overlap
(U1AH min 32.0/27.5 vs U1 max 20.1/21.2); at r100 they barely touch
(s7 U1AH min 43.5 vs U1 max 40.9). U1 reproduces the #61 class exactly
(r100 33–37, r200 19–21 — no session drift on the control). Oracle PART 7b
is CONFIRMED in direction and ordering (the m=2 deficit exists and is
deeper at r200, saturating shape); the measured deficit is SHALLOWER than
in-model (0.76–0.80 / 0.55–0.62 vs 0.64/0.39) — the wire keeps binders the
oracle does not model (receiver ~1-core sink class, §16.19). The #61
"M\*-arm ~1–2 Mbit bookkeeping cost" datum (roadmap item 8) is superseded
in the fixed regime: the M\* law now pays for itself ×1.6–1.8 at r200.

**(b) Plain-anchor truth check** (`perf_rwm_c.sh` plain bulk 25 MB, RWM_GEN=0
RWM_QUIC_CC=bbr; sc2 single + c8 dual (c2+c3); P = base vs PRS =
`RWM_PLAIN_RS=1`; ×4 interleaved; truth at 1200 B: c2 ≈ 10.4k, c3 ≈ 2.1k
sym/s; sampler liveness echo on every PRS rep, ANCHOR counters attr/gen > 0):

| gauge | P (ack-interval) | PRS (send-interval) |
|---|---|---|
| sc2 btlbw | 47.5–64.5k = **×4.6–6.2 over** | 6.5–10.7k = **0.6–1.0× truth** |
| c8 fast (c2) btlbw | 48.1–68.8k = ×4.6–6.6 over | 6.4–10.2k = 0.6–1.0× |
| c8 slow (c3) btlbw | 9.9–15.5k = **×4.7–7.4 over (the knee-clamp)** | 0–2.0k = ≤1× (under-reads when placement starves it of source) |

| cell | P mean Mbit/s (σ_s, n) s42 · s7 | PRS s42 · s7 |
|---|---|---|
| sc2 | 79.95 (2.68, 4) · 77.31 (1.82, 4) | 61.66 (11.05, 4) · 62.46 (4.81, **n=2**) |
| c8 | 48.26 (5.09, 4) · 57.49 (**19.08**, 4) | **55.35 (9.28, 4) · 61.88 (4.00, 4)** |

The GUARD-RESULTS over-read is REMOVED (target "within ~2× truth" met on
the over-read side everywhere; the residual is slow-path UNDER-read under
source starvation — the safe direction for a cap). At c8 — the cell the
percap fix needs — honest anchors also IMPROVE throughput and collapse the
bimodal spread (s7 σ 19.1 → 4.0). Named cost, honestly: sc2 single-path
−20% — the over-read was accidentally load-bearing for the anchor-sum
store cap (the SAME circularity §16.19 documented when the Copa feed got
honest samples), so `RWM_PLAIN_RS` is a measurement/cap-derivation arm,
NOT a default-flip candidate as-is. (sc2-s7 PRS lost 2 invocations to the
documented seed-7 topo-ping double-abort class; recorded, n quoted.)

**(c) Unified 3-arm tail, ONE hygiene pass** (`RWM_TM_ARMS='stream unified
rlc' RWM_ANCHOR_HYGIENE=1 RWM_DIAG=1 tail_matrix.sh c3 5`, both seeds — the
readiness probe for the queued flip-battery re-run, NOT the battery).
Harness defect, recorded first: the first s42 pass DIED silently after the
unified-1200B reps — my new [SPAN]-scraper had a `head` inside a pipeline
under lib.sh's `set -e -o pipefail` (SIGPIPE — the EXACT discipline-item-7
class recurring), losing that pass's rlc arm; fixed in the harness (awk-
internal cap + `|| true`) and s42 was re-run complete (`tailah-c3-s42b.log`).

Median [min–max] of per-rep p99 (ms) at c3; per-rep p50s all in MS unless
noted:

| arm·size | s42 rerun (n) | s7 (n) | #61 base battery (for class reference; cross-session) |
|---|---|---|---|
| stream 1200B | 880 [343–3258] (5) | 527 [284–21561] (5) | 420/1498 |
| unified 1200B | **396 [219–1782] (5)** | 579 [261–770] (3) | 794/3064 **+ 3/10 reps p50 in SECONDS** |
| rlc 1200B | 929 [893–5905] (4) | 201 [139–815] (4) | 340/205 (n=5+3; 2/10 lost) |
| unified 400B | 345 [108–706] (5) | 628 [102–11065] (5) | 181/1202 |

- **The unified collapse class: 0/13 completed unified-1200B reps with p50
  in seconds** (p50 range 26.2–39.4 ms across s42 first pass + s42 rerun +
  s7) vs the #61 base 3/10. A\* liveness echo + [SPAN] a\*=2–7 (derived, not
  pinned) on every unified rep.
- In the SAME s42-rerun session, unified-1200B posts the BEST p99 median of
  the three arms (396 vs stream 880 / rlc 929) — in #61 it was the worst.
  ONE pass, n=5: a readiness signal, not the gate.
- Honest counterweights: (i) 2/5 s7 unified-1200B reps and 2/10 rlc-1200B
  reps produced no summary within 30 s (the #61 rlc total-wedge class
  and/or the seed-7 topo class — indistinguishable here; recorded, n
  quoted); (ii) the trigger is environmental and one pass cannot bound the
  collapse rate — the FULL battery (≥10 reps/arm, the queued protocol)
  remains the arbiter.

### Gate-readiness verdicts (the two re-opened gates)

- **`RWM_UNIFIED` flip battery re-run: READY on the measurement side.**
  Fix A (A\* anchor) + fix B (clock-gap hygiene) — the two the COLLAPSE
  ATTRIBUTION gated the re-run on — are built, unit-proven, and
  mechanism-live at L0 and L1 ([SPAN] a\*=derived within ~1 RTT; witness
  quarantining real stalls only). The one-pass readiness probe (battery
  (c)) came back clean: 0/13 unified-1200B collapse reps, unified the
  best-of-three p99 median in its rerun session, A\* live on every rep.
  The FULL flip battery (tail matrix ×10 reps + c3 perf + bulk parity +
  byte-identity, per the queued protocol) remains QUEUED — it needs its
  own session, and flip (a) additionally still gates on fix C (δ-honest
  overload shedding), which is NOT built. No default flipped here.
- **The percap floor-clock cap fix: UNBLOCKED.** Residual (i)'s named
  prerequisite — an honest plain-mode BtlBw_i — exists and is measured
  ≈1× truth where fed; the knee-clamp over-read that held cap_slow ≈ 2048
  is gone under `RWM_PLAIN_RS`. The cap re-derivation
  (cap_i ≤ gain·rate_i·RTprop_i on honest rate) + the percap re-battery
  is the next session's work; the sc2 −20% single-path cost says the
  honest anchor must feed the CAP, not necessarily the cwnd anchor floor,
  and that trade-off is that battery's first arm.
  **[DONE 2026-07-19, `feat/percap-honest-cap`: the literal floor-clock
  form was refuted by its own smoke (c2 RTprop = 8 ms); the landed law
  adds the recovery-clock runway and the measured drain-ratio K. sc2
  −20% resolved exactly; c8 flip still NO (the no-borrowing tax).
  Ledger: "Per-Path Outstanding Accounting" → HONEST-CAP RESULTS.]**

### Controls / caveats / discipline

- Shipped default byte-identical: every gate reads env-unset ⇒ legacy
  branch; the full local suite is green with no env set (below); the U1/P
  control arms in every battery ran the NEW binary with gates off and
  reproduced their documented classes (#61 knee values; GUARD-RESULTS
  over-read magnitudes).
- The collapse TRIGGER is environmental. Today's box did not produce it
  in 68 reps (0 outage-class in both arms, quiet AND loaded) vs the
  attribution session's 3/14 — so THIS session measured the trigger's
  ABSENCE, not its cure. The claims held to are the ones measured: the
  amplifiers are gone at the unit/anchor level (flood-poison injection
  law; A\* live instead of inert; witness quarantining real stalls only)
  and the in-band tail improves (p90). Collapse-INCIDENCE deltas await a
  session where the trigger fires (or the queued L1 flip battery's
  larger n).
- The local r200-class L0 cell is UNSTABLE in both arms (pre-existing
  wedge class, recorded above) — it is mechanism-liveness evidence only.
- Cross-session vs #61: only in-session interleaved deltas are claimed;
  the #61 numbers are quoted as the control arm's reproduction check.

### Tests

Local (dev box, tree 988960c, all hygiene env unset — the byte-identity
proof of the shipped path): lib 350/350 (7 new anchor/witness laws + the
seed/backstop/ownership tests; 3 arrival-clock detector tests REMOVED with
the refuted design); math full 59/19/22/4/4/3/25 (incl. PART 7);
gate_suite 15/15 release (224.9 s); mtu_blackhole_wedge 2/2;
perf_loopback 8/8; fmtcp/copa_sole/daps loopbacks 1/1 each — all green
(`suites.log` in the session scratchpad; VM binary e17df72b… built from
the same commit). Harness: tail_matrix.sh gains the A\*/hygiene echo
scrape + [SPAN] trajectory dump (pipeline-failure-safe after the recorded
s42 SIGPIPE defect).

## Unified Decoder (built; L1 flip-gate battery MEASURED 2026-07-19, both flips NO) (2026-07-18) — task #61, the principle debt: ONE decoder for the RLC family (global sparse-aware closure) + the δ-derived span law A*/M*/Δ replacing the realtime/bulk machine switch; differential-proven vs all three legacy decoders; oracle δ-continuum green incl. the M* knee at RTT100/200; L0 measured (no cliff, bulk parity, tail class preserved) — and the honest finding that the #85 span-probe datum is VOID (backend-guard drop). **L1 (`meas/unified-battery`): bulk gen-sys parity + CPU parity PASS, realtime delivery-complete at c3 (+24–26 pp vs shipped streaming, span law recovery-live), but the realtime TAIL gate FAILS — unified p99 medians 2.7–3.3× legacy-RLC at c3 with a 3/10 stream-collapse rep class, so `RWM_UNIFIED` stays DEFAULT OFF (named blocker) and streaming keeps Realtime + the 12–48× crown jewel; the M* knee is UNREACHABLE at L1 behind two named anchor defects (RTprop floor under-read, static win backstop)** (branch `feat/decoder-unify`)

*Decision record: → [ADR-0064](adr/0064-unified-span-machine.md)*

**The derivation (paper §16.20, written BEFORE the code).** The two RLC-family
decoders decode the SAME self-describing wire equations; they differ only in
algebra SCOPE: `RlcWindowDecoder` computes the global closure (any spans
combine — but ~200× cost, and a newly-found rank-loss defect: it DISCARDS a
still-informative row when a late source hits its pivot), `GenerationDecoder`
a block-restricted closure keyed by `(anchor,width)` (the §16.18 sparse-aware
cost — but it provably strands the generic 2-loss burst on moving-span wires:
two covering repairs with different sliding spans never combine). The unified
machine (`fec/unified.rs`) computes the FULL global closure WITH the
sparse-aware cost model: known columns eliminated payload-only (never in the
matrix), coded rows dense over interval spans (union growth), unit rows
deliver per-arrival, O(k·L·S + k²·(L+S)) — block-diagonalizes to §16.18's
bound on aligned wires. **The realtime tail property is (a) per-arrival
incremental decode and (b) span freshness; both are preserved by
construction** (delivered-set equality with legacy-RLC is a differential
assertion, not an aspiration). The SENDER carries the rest: span width
A\* = clamp(rate·D, 1, W), D = min(H, 2·RTprop) (H = the hint's §8.8 latency
budget, b·RTprop with b ∈ {½, 1, 2}), depth M\* = ceil(rate·2·RTprop/A\*_q)+1
(§16.17's law, now the large-δ limit of the same formula), trailing offset
Δ = clamp(ceil(rate·jitter), 1, 64) — every parameter from (δ,ρ,r) + measured
anchors (constants audit §16.20.5). Realtime = the small-δ limit; bulk = the
large-δ limit; NO machine switch anywhere on the axis.

**Honest re-examination — the #85 span-probe datum is VOID.** The #85
differential probe (RWM_FRONTIER trailing-span repair, 62.5% vs taper's
50–57.5%) emitted `FecBackend::Rlc` repairs into a tunnel whose Realtime hint
had auto-selected the STREAMING backend; `StreamingDecoder::add_symbol` drops
mismatched-backend symbols on entry, so the probe's repairs never reached any
decoder — it measured trailing-span repair as PURE WIRE LOAD (empirically
re-confirmed this session via the engine's backend echo). Binder #3 (emission
span) stays PLAUSIBLE but its L0 confirmation is withdrawn; this battery
re-ran the comparison with the whole RLC family end-to-end (below).
MEASUREMENT DISCIPLINE lesson: mechanism liveness must be proven at the
RECEIVER (repairs decoded), not just the sender (cod/src).

**Differential evidence (unit, all green).**
- Aligned generation wires (systematic + coded-only, 5–25% loss, late
  sources, FILL_FLAG, dups, deficit top-ups, advance; 5 seeds): unified is
  EXACTLY equal per `add_symbol` call to `GenerationDecoder` AND the
  pre-§16.18 `reference` oracle — sets, bytes, `total_fed`/`repairs_fed`/
  `repairs_useful` (added-rank), `rank_in`
  (generation.rs `unified_matches_generation_and_reference_on_aligned_traces`).
- Moving-span wires, in-order (loss only; 20 seeds): EXACT per-call equality
  vs `RlcWindowDecoder` (sets + bytes).
- Moving-span wires under reorder/dup (30 seeds): superset-or-equal, bytes
  identical on common seqs; the 6 extra deliveries across 30 traces are the
  LEGACY RANK-LOSS DEFECT (rlc_window drops the displaced pivot row on a
  late source), isolated in `unified_recovers_rank_legacy_drops_on_late_source`.
- The §16.20.1 minimal trap (2 holes × 2 different spans, jointly
  determining): unified solves both; the keyed machine strands.

**Oracle (temporal_oracle PART 7, math suite green).** δ sweep at the
c3-class cell with r = r*(W=A\*): H=20 ms arm p99 48 ms vs pure-ARQ 62 ms and
deadline-miss 1.37% vs 4.77% (the in-band-recovery tail property); H=∞ lands
on legacy gen(384,2) within 0.1%; every adjacent δ step bounded (no cliff);
the moving→pinned anchor handoff at D = 2·RTprop is metric-inert (×1.000
completion, ×1.03 p99); the bulk point at realtime δ misses ×3.2 more
deadlines (the cliff the formula replaces). **Depth term VALIDATED in-model
in its engagement regime (BDP > G):** RTT100 knee exactly at M\*=6
(m=2: 63.1 → M\*: 99.0 Mbit/s = the m=32 ceiling), RTT200 at M\*=10
(37.3 → 96.4); saturating shape, no regression as m grows.

### L0 battery (2026-07-18, local, MEASUREMENT DISCIPLINE)

Same test binary all arms (`unified_l0-61de866448147a56.exe`, sha256
`ac74054c2d3df554…`, built from the 28138b9 tree), transport netem shim
(`RWM_L0_NETEM`), seeds 42 AND 7, arms interleaved within one session,
NOTHING else running, `RWM_DIAG=1`, engine mechanism echoes surfaced
(backend selection, "unified global decoder", "span law ACTIVE"). Per-object
completion seconds = the local tail proxy (100 KB ≈ 197 chunks @508 B,
`RWM_PERF_TIMEOUT_S=5` ⇒ DNF).

**Realtime cell (c3heavy — the #85 heavy-tail cell; 40 objects/arm):**

| arm (env) | family | cod/src s42/s7 | dnf s42/s7 | p50 s42/s7 | p90 s42/s7 | max s42/s7 |
|---|---|---|---|---|---|---|
| S shipped legacy (none) | streaming | 0.065/0.049 | **25**/**14** | 0.249/0.174 | 0.391/0.313 | 0.75/0.78 |
| A legacy RLC (`RWM_L0_BACKEND=rlc`) | RLC | 0.088/0.060 | 0/0 | 0.189/0.174 | 0.423/0.283 | 2.83/0.42 |
| B legacy RLC + leading taper (`+RWM_TAPER_R=1`) | RLC | 0.434/0.419 | 0/0 | 0.189/0.142 | 0.936/0.313 | 2.62/0.52 |
| C unified (`RWM_UNIFIED=1`) | RLC | 0.465/0.431 | 0/0 | 0.190/0.189 | 0.405/0.263 | 3.03/1.28 |

- The quantity law is LIVE at the wire in B and C (cod/src 0.42–0.47 vs
  0.06–0.09), and in C the repairs are Rlc-family end-to-end (decoder echo) —
  the receiver-side liveness #85 lacked.
- **The #85 −22 pp does NOT reproduce on the RLC family.** Every RLC arm
  delivers 40/40 with 0 DNFs at the very cell where the streaming arms DNF
  35–62%: consuming r as computed does not degrade RLC-family delivery here
  (B ≈ A), so the #85 degradation was a property of the streaming-family
  arms (decoded two-layer leading-window repairs + its delivery machinery),
  not of r-consumption per se. The −22 pp attribution is therefore RESCOPED,
  not merely the probe: at this cell the RLC family is ARQ-complete and the
  span question moves to the completion TAIL, where the trailing span (C)
  beats the leading window (B) at p90 on BOTH seeds (0.405 vs 0.936;
  0.263 vs 0.313) with medians tied; p99/max at n=40 is 1–2 outliers and
  inconclusive — the L1 tail battery is the arbiter.
- Tail class preserved: unified (C) matches legacy-RLC (A) at p50 exactly
  and at p90 (better both seeds); no bulk-class batching signature. The
  12–48× L1 crown jewel belongs to the SHIPPED streaming machine, which this
  build does not touch (default legacy); tail parity of the unified small-δ
  machine vs it is exactly what the queued L1 battery must prove before any
  default flip.

**δ sweep (c3 GE cell, hints realtime/auto/bulk = b ∈ {½,1,2}·RTprop;
30 objects/arm; U = `RWM_UNIFIED=1`, L = legacy `RWM_L0_BACKEND=rlc`):**

| hint | U p50 s42/s7 | L p50 s42/s7 | U mean s42/s7 | L mean s42/s7 | dnf |
|---|---|---|---|---|---|
| realtime | 0.249/0.298 | 0.313/0.423 | 0.375/0.539 | 0.436/0.516 | 0 all |
| auto     | 0.171/0.172 | 0.157/0.158 | 0.214/0.220 | 0.178/0.176 | 0 all |
| bulk     | 0.157/0.158 | 0.158/0.157 | 0.171/0.159 | 0.175/0.156 | 0 all |

No cliff between adjacent δ points in either machine (adjacent-hint median
ratios ≤ 1.5/1.7, same shape as legacy); realtime-U beats realtime-L at p50
on both seeds; the auto-U arm pays a small mean/tail premium (+20% mean,
max 0.58/0.70 vs 0.28/0.41) — the honest bandwidth price of consuming r at a
mid-δ point where the legacy taper emits ~nothing; bulk ties exactly.

**Bulk gen-sys parity (c2, `--window-systematic-repair` wire, 5 MB × 8,
timeout 60 s):**

| arm | s42 mean Mbit/s (median s) | s7 mean Mbit/s (median s) |
|---|---|---|
| L legacy machine | 75.8 (0.531) | 78.2 (0.514) |
| U unified (+M\* pipe) | 72.6 (0.521) | 69.7 (0.520) |
| U + `RWM_GEN_PIPE=0` (decoder-only attribution) | 75.3 (0.533) | 74.5 (0.534) |

Median parity ≤ 1.2% in BOTH directions; the U-arm mean dips are single-run
outliers (max 0.68/0.93 s once per arm, σ 0.07–0.14 vs legacy 0.006–0.027) —
at c2 the M\* law sits at its cold-start floor (BDP ≪ G ⇒ M\*=2) so the
decoder swap is the only live delta; the depth term's engagement cells
(RTT 100/200) are L1-only and queued.

**Suites (all green, this tree):** lib 332; math full (incl. PART 7);
gate_suite 15/15 release (1118 s); mtu_blackhole_wedge 2/2; fmtcp/copa_sole/
daps loopbacks; perf_loopback 8/8.

**Verdict, scoped honestly.** The RLC-family unification is BUILT and
locally validated: one decoder (differential-exact on both legacy wires, and
a strict improvement under reorder — the legacy rank-loss defect), one span
law continuous in δ (oracle + L0 sweep, no cliff), bulk parity at the
median, tail class preserved vs legacy-RLC. NOT claimed: parity with the
shipped STREAMING realtime machine's L1 message-tail (the 12–48×) — that
comparison is the queued battery's job, and `RWM_UNIFIED` stays DEFAULT OFF
(shipped path byte-identical) until it passes. The streaming two-layer code
and the block pipeline remain separate machines by declared scope
(§16.20.6).

### Queued L1 parity battery (VM; protocol — run when VM access returns)

Binary from `feat/decoder-unify` (or main after merge), sha256 + commit in
the log; seeds 42 AND 7; interleaved same-binary arms; full MEASUREMENT
DISCIPLINE (cod/src + the RWM_UNIFIED/backend engine echoes per arm; the
unified arms must show "unified global decoder" at the RECEIVER).

1. **Realtime tail parity** — `tools/l1/tail_matrix.sh c2 5` (+ c3):
   {shipped legacy realtime (streaming)} × {unified realtime
   (`RWM_UNIFIED=1`)} × {legacy plain-RLC realtime (`fec_backend=rlc`)},
   400 B and 1200 B, p50/p99 distributions over ≥5 reps. GATE: unified p99
   within the legacy-RLC arm's distribution AND no regression class vs the
   shipped streaming arm (the 12–48× property). A unified arm losing the
   tail win ⇒ default stays legacy, finding recorded.
2. **rt_sweep / rstar-battery realtime cell** — `tools/l1/rstar_battery.sh`
   c3 single-path × {legacy, RWM_UNIFIED=1}: delivered reliability + cod/src
   (the #85 spot-check EXPECTations updated: the RLC-family arms should be
   ARQ-complete; the observable is the completion tail).
3. **Bulk parity** — `tools/l1/perf_rwm_c.sh c2 c2 bulk 25000000 8` single +
   C7/C8 dual, gen-sys wire × {legacy, RWM_UNIFIED=1}: throughput within σ_s
   of the legacy arm (goal-gate "Decode-CPU Ceiling" numbers are the
   reference class), receiver CPU recorded (the unified decoder must not
   regress the §16.18 sparse-aware budget).
4. **Depth-term engagement cells** — `c2r100` / `c2r200` (+`l5` loss
   variants) single-path gen-sys × {RWM_GEN_PIPE=0, RWM_GEN_PIPE=1}× 
   {legacy, unified}: the oracle PART 7b knee (m=2 ≈ 0.64×/0.39× of M\*)
   must appear on the wire — the §16.17 depth law's first L1 validation in
   its engagement regime.
5. Full-suite regression on the VM (gate_suite release, loopbacks) with
   RWM_UNIFIED unset — byte-identical shipped path proof.

Flip decision: all of 1–4 green ⇒ RWM_UNIFIED default ON in a follow-up
session and the legacy machines are scheduled for removal; any red ⇒ the
specific property that blocks unification is the deliverable.

### L1 RESULTS (VM 10.1.5.16, 2026-07-19 05:37–07:45 UTC; binary sha256 3654214ef4ca8eb3… = commit dada6ec/bd13985 on `meas/unified-battery` — byte-identical to the #86/#85 battery binary (no Rust source changed since 8ef5ff1; docs+harness only), SAME binary every arm; host-passthrough E5-2650 v3, aes+avx2+pclmulqdq in every log header (post-divide era — compared only against post-divide numbers); seeds 42+7, arms interleaved (per-rep round-robin in batteries 2–4; per-arm blocks within one warm-tunnel session in battery 1, the tail_matrix precedent); driver `/home/vibe/unified61/u61_all.sh`, logs `/home/vibe/unified61/{tail2-*,c3rt-*,bulk-*,knee-*}.log` + per-run `diag-*` + `probe-r200-{L0,L1}-rwm-c.log`; lock `/tmp/rwm-vm.lock` held 04:47–~08:30 UTC)

**Harness caveats, recorded first.** (i) The first tail_matrix pass lost every
legacy-RLC arm to a harness defect — `lib.sh` forces `set -e` and the new
3-arm mode's echo-grep pipeline (plus a no-summary rep) killed the matrix
silently mid-arm; fixed in bd13985 (`|| true` guards + a `backend=Rlc`
liveness echo for the explicit-backend arm) and the WHOLE battery 1 was rerun
clean (first-pass stream/unified data preserved in `tail-*.log`, consistent
with the rerun). (ii) The known seed-7 topo-ping double-abort recurred:
battery 2 s7 kept n=5 (S) / n=7 (U) of 8; battery 3 s7 kept n=3/6 (sc2
LS/US) and n=4/6 (c7); battery 4 s42 lost 1 rep in three arms (n=7); aborted
invocations produce a stale-server-log echo line (recorded, discounted). No
captured result was discarded.

**1. THE FLIP GATE — 3-arm realtime tail matrix** (`RWM_TM_ARMS='stream
unified rlc' SEED=s tail_matrix.sh {c2,c3} 5`; run-mode tunnels, 50 msg/s ×
20 s per rep, per-rep p99 over ≥5 reps/seed; arms: stream = shipped Realtime
(streaming two-layer), unified = `RWM_UNIFIED=1` (RLC family, unified
decoder + span law), rlc = `--fec-backend rlc` (legacy `RlcWindowDecoder`);
liveness echoes at BOTH endpoints on every arm — backend selection,
"unified global decoder", "span law ACTIVE", `backend=Rlc`).

Median [min–max] of per-rep p99 (ms), s42/s7 (n=5+5):

| cell·size | stream (shipped) | unified | legacy-rlc |
|---|---|---|---|
| c2 400B | 65/68 [39–2977] | 44/180 [36–573] | 51/51 [35–2553] |
| c2 1200B | 130/106 [55–5439] | 545/153 [30–2993] | 47/327 [40–1327] |
| c3 400B | 172/715 [111–3149] | 181/1202 [109–1363] | 185/283 [96–1310] |
| c3 1200B | 420/1498 [335–11520] | 794/3064 [149–18715] | 340/205 [126–5781] (n=5+3) |

p50 ties everywhere (~8 ms c2, ~24–33 ms c3) EXCEPT the unified c3-1200B
arm, which shows a **stream-collapse rep class: 3/10 reps with p50 in
SECONDS (1.96/2.34/9.15 s — the whole 20-s stream backlogged)**, absent in
the stream arm (0/10) and in the rlc arm's completed reps (0/8; but rlc
LOST 2/10 reps outright at this cell — no stream summary within the 30-s
timeout, a total-wedge class of its own, recorded).

**Tail-gate verdict.** At c2 all three arms are one tail class (arm medians
44–545 vs intra-arm rep spread 30→5439 — no separable regression). At the
bursty c3 cell the unified arm is NOT ≥ legacy-rlc: pooled p99 medians 633
vs 234 (400B) and 908 vs 273 (1200B), it ties rlc only at s42-400B, and it
carries the collapse class rlc's completed reps do not show. Against the
shipped streaming arm it is the same broad class at 400B (633 vs 510) and
worse-with-collapses at 1200B (908 vs 822, 3 collapse reps vs 0). **The
12–48× tail property stays with the shipped streaming machine** (which this
battery leaves untouched as default); honest bonus finding: at THESE cells
the legacy-RLC realtime arm posts the best p99 medians of all three
machines (234/273 at c3) — the streaming-vs-RLC ordering at L1 tail cells
is not what the L0 c3heavy proxy suggested, recorded for the roadmap.

**2. Realtime delivered reliability, c3 perf cell** (`perf_rwm_c.sh c3 c3
realtime 100000 20 single`, RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=5, ×8
interleaved; S = shipped streaming, U = `RWM_UNIFIED=1`):

| arm | s42 delivered (per-rep /20) | s7 delivered | cod/src s42/s7 | completer median_s |
|---|---|---|---|---|
| S | 118/160 = **73.8%** [14 12 18 15 14 12 17 16] | 76/100 = **76.0%** (n=5) [14 15 16 15 16] | 0.056/0.179 | 0.11–0.17 |
| U | 159/160 = **99.4%** [20 20 20 19 20 20 20 20] | 140/140 = **100%** (n=7) | 0.416/0.341 | 0.38–0.55 |

The unified small-δ machine is delivery-complete at the cell where the
shipped streaming machine leaves 24–26% of objects as DNFs (+25.6/+24.0 pp,
every U rep ≥ every S rep, ≫ the σ_rep ≈ 2-object spread) — the quantity
law + trailing solvable span is RECOVERY-LIVE at the receiver (100%
delivery at ε≈4.8% with the 20-ms reorder horizon ≪ the 90-ms ARQ round
means the holes were FEC-decoded in-window; cod/src 0.34–0.42 = r consumed
as computed, the #85 wire law). The price: completer medians 3–4× slower
(0.38–0.55 s vs 0.11–0.17 s survivor-only) — the same
reliability-vs-completion-tail trade battery 1 measures from the other side.
S-arm baselines reproduce the #85 spot-check class (68.8–76.2%) —
session-drift anchor holds.

**3. Bulk gen-sys parity, sc2 + c7** (`perf_rwm_c.sh c2 c2 bulk 25000000 1
{single,dual}` ×8 interleaved, gen-sys wire `--window-systematic-repair`,
GPB stack RWM_GEN_R=0.03 RWM_GEN_PIPE=1 RWM_QUIC_CC=bbr, GUARD OK all
invocations; LS = legacy `GenerationDecoder`, US = `RWM_UNIFIED=1` unified
global decoder, receiver echo on every US run):

| cell | LS mean Mbit/s (σ_s, n) s42 · s7 | US s42 · s7 | CPU srv·cli /25 MB (LS → US) |
|---|---|---|---|
| sc2 | 72.20 (3.93, 8) · 72.13 (4.00, 3) | 75.32 (1.26, 8) · 73.42 (3.97, 6) | 2.38·1.58 → 2.40·1.58 (s42) |
| c7 | 81.83 (7.63, 8) · 82.38 (6.56, 4) | 83.89 (7.17, 8) · 77.60 (6.53, 6) | 2.67·2.10 → 2.80·2.26 (s42) |

**Throughput parity PASS** (every Δ within σ_s, sign flips across seeds).
**CPU parity PASS at sc2** (Δ ≤ 0.02 s both seeds); at c7 the US receiver
reads +0.13/+0.07 s (+4.9%/+2.6%, same direction both seeds) — inside the
run scatter but recorded as the honest dual-path cost signal of the global
matrix. First post-divide gen-sys anchors: LS sc2 72.2 = 0.92× the same-era
plain+BBR single (78.6, #86 battery) — the pre-divide RATIO (70.9/77.1 =
0.92) reproduces on the new hardware.

**4. Depth-term knee, c2r100 + c2r200** (gen-sys single, 2×2
{legacy, RWM_UNIFIED=1} × {RWM_GEN_PIPE=0,1} ×8 interleaved; oracle PART 7b
prediction: m=2 ≈ 0.64× of M\*=6 at RTT100, 0.39× of M\*=10 at RTT200):

| cell | L0 (fixed m=2) | L1 (M\* law) | U0 | U1 | s42·s7 per column |
|---|---|---|---|---|---|
| c2r100 | 33.63 (2.01) · 35.63 (1.68) | 35.26 (1.98, n7) · 36.72 (2.29) | 32.66 (3.09, n7) · 36.15 (1.27) | 35.26 (2.24, n7) · 36.52 (2.56) | Mbit/s |
| c2r200 | 20.23 (1.60) · 21.36 (0.44) | 19.14 (0.93) · 19.36 (1.51) | 19.92 (0.89, n7) · 19.24 (1.00) | 21.24 (1.48) · 19.44 (0.63) | Mbit/s |

**The knee does NOT appear — and the DIAG probes name why.** All four arms
are flat (r100 ≈ 33–37, r200 ≈ 19–21; the oracle's 0.64×/0.39× m=2 deficit
is absent; at r200 the M\*-law arms even sit ~1–2 Mbit BELOW fixed-depth on
both machines and both seeds, ~1.3–2σ — the depth law's bookkeeping is a
small net cost at this operating point, recorded). Probe runs
(`probe-r200-{L0,L1}-rwm-c.log`): both arms peg `win=768/768` (= 2·G intake
cap) with `stall[budget=90–95%]` and TUN `paused=13–39%` while cwnd (2190–
3256) and the per-path BDP cap (3865) have headroom — the window/budget
ceiling binds, not the pipe. M\* never left the cold-start floor: the DIAG
floor echo reads `rtp=50ms` at a 200-ms-RTprop cell (floor-freshness FAIL —
a DEFAULT_SRTT-class 50-ms seed surviving the ~10-s run inside the 10-s
min-window), so `gen_pipe_depth(rate·2·rtprop)` computes ≈2, and the STATIC
`fmtcp_win_backstop = (RWM_PIPELINE+2)·G = 1536` is not M\*-coupled anyway.
BDP (2 200–2 900 sym) ≫ G — the engagement REGIME is real, but the L1 wire
cannot reach it. **Oracle PART 7b's knee is neither confirmed nor refuted
at L1: it is UNREACHABLE behind two named anchor defects** (RTprop floor
under-read; delivered-rate warm-up loop + static backstop). Unified ≡
legacy at every point (Δ within σ_s) — the decoder swap itself is
knee-neutral.

**Byte-identity (battery 5).** The shipped path with `RWM_UNIFIED` unset is
proven identical three ways: (i) the binary is sha256-identical
(3654214ef4ca8eb3…) to the #86/#85 battery binary — the #61 code was
already in that tree, all of it behind `unified_active()` (default false);
(ii) every baseline arm in this battery ran with the knob unset and
reproduced its documented class (S-arm 73.8/76.0% vs #85's 68.8–76.2%;
LS-sc2 0.92× same-era plain+BBR = the pre-divide ratio) with the legacy
echoes and ZERO unified echoes; (iii) the full suite (below) is green on
the final tree with the knob unset.

**Suites (VM, final tree bd13985, RWM_UNIFIED unset):** lib 330 (+2
ignored); math full 59/19/22/4/4/3/25 (incl. PART 7); gate_suite 15/15
release (244 s — the passthrough era's first gate_suite timing, vs 1118 s
pre-divide); mtu_blackhole_wedge 2/2; perf_loopback 8/8; fmtcp/copa_sole/
daps loopbacks — all green (`/home/vibe/unified61/suites.log`).

**FLIP DECISIONS.**

- **(a) `RWM_UNIFIED` default for the RLC-family paths: NO.** Bulk parity
  and CPU parity PASS, but the gate requires ≥ legacy-RLC EVERYWHERE and
  the realtime tail matrix fails it at the bursty cell: p99 medians 633/908
  vs legacy-rlc's 234/273 at c3, plus the unified-only stream-collapse rep
  class (3/10 reps, p50 in seconds, both seeds represented). The named
  blocker is the **unified-realtime sustained-stream collapse class at
  c3-1200B** — until it is attributed and closed, `RWM_UNIFIED` stays
  DEFAULT OFF (shipped path byte-identical).
- **(b) Streaming retirement: NO** (as the gate expected). The unified
  small-δ machine does not reach streaming-class tails under sustained
  load: ×1.1–1.24 pooled p99-median gap at c3 plus the collapse class, and
  its delivery-complete mode costs 3–4× on completer medians at the c3
  perf cell. The quantified trade for the roadmap: **+24–26 pp delivered
  reliability for ×3–4 completion medians and a 3/10 collapse-rep tail** —
  a different point on the (δ, ρ) surface, not parity. The streaming
  two-layer code keeps the Realtime default and the 12–48× crown jewel.

**Named follow-ups (not built, this task is measurement):** (1)
unified-realtime c3-1200B stream-collapse class — attribution (candidates:
EVICT-window × trailing-span repair interaction under sustained bursts,
decode/delivery backlog, retention pressure) **[ATTRIBUTED 2026-07-19 at
L0 — see COLLAPSE ATTRIBUTION below: NOT the decoder; a
transient-stall-amplification class shared with legacy-RLC]**; (2)
legacy-RLC realtime
total-wedge class (2/10 reps, no summary in 30 s, same cell) **[same root
family, see below]**; (3) the M\*
L1 anchor pair — RTprop floor under-read (50-ms seed inside the 10-s
min-window) and the static (pipeline+2)·G win backstop — fix, then re-run
the knee; (4) the r200 M\*-arm ~1–2 Mbit bookkeeping cost; (5) c7 US
receiver +3–5% CPU. Honest scope: batteries 1–2 measure the RLC family at
REALTIME δ where the streaming default was never displaced; the bulk/auto
window-reliable RLC paths (unified's other half) passed every gate they
were given (parity + CPU + byte-identity).

### COLLAPSE ATTRIBUTION (2026-07-19, roadmap item 3, branch `diag/unified-collapse`) — the c3-1200B stream-collapse class REPRODUCED at L0 and attributed: NOT a decoder mechanism; a whole-process-transient amplification class shared by BOTH RLC-family realtime arms, with two NAMED unified-specific anchor defects found on the way

**Instrument (this branch).** A new L0 sustained-stream rung
(`tests/unified_stream_l0.rs`): the L1 tail_matrix stream shape (50 msg/s ×
20 s × 1200 B, messages split into ≤508-B chunks like the L1 TUN-MTU-clamped
inner TCP) driven through TWO real engines (memory TUNs, real QUIC on
127.0.0.1) under the transport netem shim at the EXACT L1 c3 params (20 Mbit,
20 ms one-way, 5 ms jitter, GE p=2 q=40 — `lib.sh scenario_params` ==
`l0_scenario("c3")`), per-message latency at the server's in-order delivery
point, per-message JSON + p50/p90/p99. Plus DIAG-gated instrumentation (all
default-off, shipped path untouched): decoder-internal `diag_stats()` (active
RREF rows L, widest span, coeff bytes) in the RWM_FDIAG report + per-call
`add_symbol` max; a `[SPAN]` sender trace (live A\*/Δ, TaperBudget owed,
repair rate/debt, retransmit-buffer depth); shim transit counters
(enq/GE-drops/tail-drops/quinn-send ok/err/queue depth) + quinn datagram
frame rx/tx at both endpoints — the "where did the packets die" layer the #85
lesson demands.

**Provenance.** Local dev box (Windows 11, 16 cores — its own hardware era;
no cross-era number is compared, all arms same-session same-binary). Test
binary `unified_stream_l0-30acebbd33c901e0.exe` sha256 `f114612b28dcebe3…`
built from this branch off 729327e; arms are env-selected per process
(`RWM_UNIFIED=1` / `RWM_L0_BACKEND=rlc` / unset), liveness echoes recorded
per rep (backend selection, "unified global decoder", "span law ACTIVE",
netem cfg+seed). Logs: session scratchpad `bat1/` (14×2 reps), `bat2/`
(stream ×6), `ib-*` (exposure-controlled interleaved 3-arm ×4),
`ctrace-1`/`ttrace-9` (instrumented collapse reps), `trace-build-*` (A/B/C
under build interference).

**Reproduction (L0, MEASUREMENT DISCIPLINE: same binary, interleaved arms,
seeds recorded).** 14 seeds × {unified, rlc} + 6 × stream at the c3-1200B
cell: the class reproduces with the L1 battery's structure — episodic
1–2.6 s TOTAL delivery outages, chained 3–5 deep in the heavy reps (p90
2.5–3.1 s): unified 3/14 outage-class reps (s42 p90 3063 ms ×5 outages, s7
2486 ms ×4, s1 mild ×1), legacy-rlc 1/14 (s42 p90 3043 ms ×5 outages + 22
lost messages = the L1 wedge signature); stream arm 0/6 outage-class but
sheds 5–13 messages/rep. The class is
seed-NONdeterministic (same seed re-runs clean) and concentrates where host
background load was present; a concurrent `cargo build` (memory-bandwidth +
I/O interference, Defender-scan class) reproduces it ON DEMAND with the same
family split (exposure-controlled interleaved battery: RLC-family arms p90
0.45–3.0 s in affected reps; stream arm p90 ≤ 400 ms in EVERY rep, p99 ~1 s,
loses ~1% of messages instead).

**The trace that names the mechanism (collapse rep vs clean rep,
instrumented).** During a collapse episode the transit counters show the shim
forwarder (a timer-driven task) FROZEN in BOTH engines simultaneously —
`enq` advancing, `ok` flat, queue 0→174 over ~1 s — i.e. a WHOLE-PROCESS
scheduling/timer stall (the trigger is outside the transport), followed by a
release flood. The transport's RESPONSE to that transient is the collapse
class, and it is FAMILY-level:

- **The decoder is exonerated.** Throughout collapse AND clean reps the
  unified global RREF is EMPTY: `rows=0, max_span=0, coeff_kb=0` in every
  FDIAG sample; per-arrival `add_symbol` averages 6–11 µs (max 13.9 ms
  once, during the stall itself — scheduling, not compute; total decode
  compute 12–21 ms per 20-s rep). Candidates (a) global-closure L-growth,
  (b) span-law re-elimination storm, (d) allocation churn: REFUTED by
  direct observation at the collapse cell.
- **What actually happens:** the reliable-in-order EVICT pipeline queues
  the ENTIRE stream behind post-stall recovery (cumulative ack frozen
  ~1.3 s, retransmit buffer 15→120), and the post-stall ack flood POISONS
  the measured anchors — delivered-rate windowed-max reads 63,536 sym/s
  (≈260 Mbit on a 20 Mbit link), cwnd 169→2,740, EWMA RTT 61→176 ms —
  extending the disturbed regime; chained episodes follow. The streaming
  arm under the SAME transient force-skips holes past its reorder horizon
  (loses ~1% of messages) and its p90 never moves: the two responses are
  the two (δ, ρ) semantics, and the RLC family's realtime path currently
  has the WRONG one for small δ (it serializes the whole stream behind
  recovery — a δ-contract violation under overload).
- **Legacy-RLC same-root datum (roadmap item 4): YES, same family.** The
  legacy-rlc arm shows the identical episodic class at L0 (chained outages,
  p90 3.0 s, plus lost chunks — the L1 total-wedge signature), with NO
  unified code in the loop. The L1 pair (unified 3/10 collapse; rlc 2/10
  wedge) is one class with two terminal behaviors, not two roots.
- **L1 linkage (inference, honestly scoped).** The L1 trigger was not
  directly observed (no transit counters ran there), but the structure
  transfers: the L1 stream arm's own per-rep maxima at the same cells
  (3–11.5 s in completed, non-collapse reps) show the transient episodes
  were present in ALL L1 arms; only the RLC-family arms converted them
  into p50-seconds/wedges. At L1 the inner TCP adds the escalator L0
  lacks (RTO backoff on force-delivered holes), which is the plausible
  gap between L0's p90-seconds and L1's p50-seconds. CONFIRMATION
  PROTOCOL (queued for the next VM window): rerun tail_matrix c3-1200B
  with per-rep host-steal sampling (/proc/stat steal) + these transit
  counters; verdict = collapse reps coincide with stall episodes.

**Two unified-specific defects FOUND (named, not the collapse cause).**
(i) **The A\* rate anchor is cold and slow:** `est.throughput()` is an
α=0.125 EWMA fed every 2 s from the send rate, so at the c3-1200B realtime
cell A\* = clamp(rate·D, 1, W) sits at **1** for the first ~10 s of every
stream (measured in every [SPAN] trace; the data-direction arm reaches 4–7
only at t≈11 s). A width-1 "trailing span" is a duplicate of one
near-frontier symbol: the quantity law's r (cod/src 0.3–0.5) is consumed as
near-pure overhead — repairs_useful/repairs_fed ≈ 9% — and recovery is
ARQ-bound (FDIAG: SOURCE-resolved holes outnumber DECODE-resolved ~4:1 at
~40 ms each). The small-δ machine's FEC is therefore largely INERT at
exactly the cell the flip gate measured. (ii) **The anchor is
flood-poisonable:** post-stall A\* spikes 1→38 off the corrupted rate
sample. Both are the same anchor-defect family as the #61 M\* findings
(RTprop floor under-read; delivered-rate warm-up loop).

**Fix status: none built (gate honored — nothing small+obvious).** The
trigger is environmental; the amplifiers are design-level. Named designs:
(A) **A\* anchor repair** — seed/blend the span-law rate from the
instantaneous send rate (windowed-max, §16.15 statistic) instead of the
cold 2-s EWMA, and discard samples spanning a detected monotonic-clock gap;
gates the L1 tail-battery re-run. (B) **Post-stall estimator hygiene** —
same clock-gap guard for RTT/throughput/BtlBw samples (prevents the ×13
BtlBw / ×16 cwnd poisoning). (C) **The δ-honest overload policy** — under
backlog beyond H the realtime RLC path should shed like the streaming arm
(EVICT means evict), not serialize; this is the structural §16.20 item and
the real gate for flip (a). (D) The L1 confirmation protocol above.

**Flip readiness.** `RWM_UNIFIED` stays DEFAULT OFF. The blocker is now
ATTRIBUTED and decomposed: not decode cost, but (i) the A\*=1 anchor defect
(fix A) making realtime FEC inert, and (ii) the family-level
transient-amplification response (fix C + estimator hygiene B). The flip
battery re-run is gated on A (+B); C is the streaming-retirement-class
question and can gate separately. All instrumentation added this branch is
DIAG-gated and the shipped path is byte-identical with the knobs unset.

**Suites (final tree, this branch, local box, all diag knobs unset):**
lib 332 (+2 ignored); math full 59/19/22/4/4/3/25 (incl. PART 7);
gate_suite 15/15 release (235.7 s); mtu_blackhole_wedge 2/2; perf_loopback
8/8; fmtcp/copa_sole/daps loopbacks 1/1 each — all green.

## Taper Emission Fix (2026-07-18) — the #46 per-ack-cycle emission inertness FIXED as a mechanism (RWM_TAPER_R budget law, unit + L0-wire validated: cod/src 0.03-0.05 → 0.21-0.34) — and the honest L0 2x2 verdict: r* is STILL not realized at the realtime plain-mode wire; the next binders are NAMED and measured (spare-cap compression + leading-window entanglement). Default OFF. L1 spot check MEASURED 2026-07-19 (`meas/percap-battery`): wire-consumption CONFIRMED (cod/src 0.06–0.09 → 0.32–0.35), the −22 pp degradation REPRODUCED on the real substrate (−25/−19 pp, both seeds) — the entanglement attribution stands, the flip stays closed. (task #85, branch `fix/taper-emission`)

*Decision record: → [ADR-0063](adr/0063-rstar-window-mass-provisioning.md)*

**The bug (located by #46, fixed here).** Plain-mode proactive repair accrues
emission debt from the taper density τ(t) = r·q̂(1−q̂)^t (net/mod.rs) and the
offset t resets on every cumulative-ack advance, so emitted proactive repair
sums to Σ_t τ(t) = **r symbols per ACK CYCLE** — an ack cycle at BDP is
hundreds of symbols, so wire overhead was ~r/cycle, nearly independent of r's
computed magnitude. #46 L1 measured the consequence: legacy r* = 0.206 and
corrected 0.255 both emitted cod/src ≈ 0.03–0.10; the whole r* control loop
(including the §8.4.1 burst-tail correction) was inert at the wire.

**The budget law (`TaperBudget`, control/fec_rate.rs; consumed in the
net/mod.rs plain-mode emission block; env `RWM_TAPER_R`, default OFF ⇒
byte-identical).** Per source symbol the computed rate is BANKED
(`owed += r`; Σ grants tracks r × source — the wire consumes r AS COMPUTED,
per coding window) and the emission grant is

```
   grant = min( owed, max(desire, r), spare, 1.0 )
   desire = r · shape(t mod W),  shape(t) = W·q̂(1−q̂)^t / (1−(1−q̂)^W)
```

— the SAME GE-survival taper shape renormalized to mean 1 over the coding
window W, so the taper's intent (repair concentrated right after the frontier
advances) survives as a RE-TIMING while the TOTAL is governed by the budget,
not the ack cadence. No new constants: floor r (the desire tail cannot strand
the budget), `spare` = the legacy headroom anchor, 1.0 = ≤ 1 repair per
source send (the source clock paces — no bursts), owed cap = max(r·W, 1)
(a spare-starved window's budget expires; the budget IS r×W).

**Unit evidence (fec_rate.rs, 4 tests, green).** (i) emitted tracks r×source
within 15% for r ∈ {0.05, 0.25} (measured 5.0× apart) where the legacy law
emits ~r per ack cycle for BOTH (r-invariant, ~50× less at 200-symbol
cycles); (ii) ack-cadence invariance: per-symbol acks, 300-symbol cycles, and
one endless cycle all emit ≈ r×source (legacy endless cycle emits ~r TOTAL —
the executable statement of the bug); (iii) spare-starved ⇒ zero grants, owed
expires at r·W, backlog drains ≤ 1 symbol per source send on recovery;
(iv) frontier grant > mid-span grant (concentration kept).

### L0 2x2 (2026-07-18, local, MEASUREMENT DISCIPLINE): the heavy-tail wire rung

Netem `gemodel` IS GE, so the L1 VM cannot express the §8.4.1 heavy-tail
claim at all; the transport L0 netem shim was therefore EXTENDED with the
#46 ARM-3 heavy-tail loss law (semi-Markov: geometric Good sojourns,
discrete-Weibull(θ=0.55, k=0.5) Bad sojourns, E[burst]=6.2 — quic.rs, plus a
`c3heavy` scenario) — this L0 shim is the correct local rung for the tail
claim. Harness `tests/taper_emission_l0.rs`: realtime hint, plain
window-reliable, single path, 100 KB objects (~197 chunks @508 B), per-object
DNF = app-level delivered reliability (same observable as
tools/l1/rstar_battery.sh), `RWM_PERF_TIMEOUT_S=5`. Cell
`heavy:20;20;5;0.6;0.55;0.5` (c3 rate/RTT/jitter, onset 0.6% ⇒ ε ≈ 3.6%;
onset 2.3% = the #46 ε=12.5% synthetic kills every object in every arm —
recorded, not used). Same test binary all 8 arms (taper_emission_l0 sha256
928de1a20165fc22…, built from d190b29 + this branch's working tree),
40 objects/arm, seeds 42 AND 7, NOTHING else running
(a first battery that overlapped concurrent builds was DISCARDED — the DNF
observable is wall-clock sensitive; its numbers reproduced anyway).
Mechanism liveness: `RWM_TAPER_R` echo + per-arm cod/src (DIAG lines with
src ≥ 100 sym/s — the bulk sender during feed).

| arm (RWM_TAPER_R × RWM_RSTAR_TAIL) | s42 delivered | s7 delivered | pooled | cod/src s42 | cod/src s7 |
|---|---|---|---|---|---|
| fix OFF × legacy    | 25/40 (62.5%) | 30/40 (75.0%) | 68.8% | 0.050 | 0.032 |
| fix OFF × corrected | 24/40 (60.0%) | 26/40 (65.0%) | 62.5% | 0.052 | 0.028 |
| fix ON  × legacy    | 13/40 (32.5%) | 20/40 (50.0%) | 41.3% | 0.317 | 0.209 |
| fix ON  × corrected | 13/40 (32.5%) | 23/40 (57.5%) | 45.0% | 0.344 | 0.247 |

- **The emission fix is LIVE at the wire.** cod/src 0.03–0.05 → 0.21–0.34
  (~6–10×), replicating #46's L1 inertness locally in the OFF arms and
  breaking it in the ON arms. This is the build's claim, and it holds.
- **The r* arms stay TIED in delivered reliability in BOTH emission modes**
  (fix OFF 68.8 vs 62.5%, fix ON 41.3 vs 45.0% — both inside the per-arm
  spread). Controller attribution (unit probe
  `probe_rstar_arms_c3heavy`, this cell's loss law + c3 anchors):
  r_legacy = 0.248 vs r_corrected = 0.445 — the arms DIFFER 1.8× at the
  controller, but the wire shows only +0.03–0.04: the **spare-capacity gate**
  (`compute_repair_rate_capped`: r ≤ spare = (cwnd−in_flight)/in_flight)
  compresses both arms to ≈ the same consumed rate. The never-hurts gate is
  a real contract, but it means corrected provisioning cannot be expressed
  on a saturated realtime flow — binder #2, now named.
- **Consuming r DEGRADES delivered reliability at this profile** (−22 pp
  pooled, consistent across both seeds). Attribution: plain-mode taper
  repair codes over the LEADING sliding window (up to ~1024 symbols,
  including in-flight) — the documented RWM_MIN_R entanglement defect — so
  a covering repair is not solvable at the receiver until the window tail
  arrives (~½–1 RTT ≫ realtime's 20 ms reorder horizon): the repair is
  recovery-inert, pure wire load. Differential probe: trailing-window
  frontier repair at the SAME consumed rate (RWM_FRONTIER=32
  RWM_FRONTIER_R=0.25, taper off, seed 7, cod/src 0.271) delivers 25/40
  (62.5%) vs the taper arms' 50–57.5% — the SPAN, not the quantity, is
  binder #3 — and still ≤ the 75% no-proactive baseline: at this cell every
  proactive-repair form tested has negative marginal reliability value
  under the horizon. Binder #3 (emission span) confirmed.

**Verdict, scoped honestly.** #46's located defect (quantity: r per ack
cycle) is REAL and is FIXED — the wire now consumes r as computed (unit
proof + wire liveness above). The EXPECTED 2×2 separation did NOT appear:
realizing r* at the realtime plain-mode wire is blocked one layer deeper by
two further, now-measured binders — the spare-cap compression of the r*
arms, and the leading-window (unsolvable-span) coding of the emitted repair.
`RWM_TAPER_R` therefore stays DEFAULT OFF (shipped path byte-identical);
flipping it is gated on the solvable-span emission follow-up (code plain-mode
proactive repair over a decodable trailing span, or route realtime through
the generation/pacer path, and revisit whether contract-priced repair should
bypass the spare gate) — not on L1 alone.

### Queued L1 spot check (VM; protocol — run when VM access returns)

Re-run #46's cell: `tools/l1/rstar_battery.sh` seeds 42+7, x8 interleaved
same-binary reps, c3 single-path netem, realtime, plain window mode, with the
2×2 env (`RWM_RSTAR_TAIL` × `RWM_TAPER_R`), sender DIAG preserved per run.
EXPECT (from this L0 evidence): (a) fix-ON arms emit cod/src ≈ 0.2–0.35 vs
fix-OFF 0.03–0.10 — the wire-consumption claim, the only thing netem-L1 can
prove (netem `gemodel` is GE: L1 tests the §8.7 closed-form-vs-exact gap,
NOT the heavy-tail claim — that claim's wire rung is THIS L0 shim);
(b) delivered reliability in the fix-ON arms does NOT improve (L0 predicts
degradation) — if L1 reproduces that, the leading-window entanglement is
confirmed on the real substrate and the solvable-span follow-up is the
named next task; (c) r* arms tied at the wire (spare-cap compression).
A fix-ON arm that IMPROVES delivered reliability at L1 would falsify the
entanglement attribution and reopen the default-flip question.

### L1 spot-check RESULTS (VM 10.1.5.16, 2026-07-19 03:59–04:37 UTC; binary sha256 3654214ef4ca8eb3… = commit b317983 on `meas/percap-battery`; `tools/l1/perf_rwm_c.sh c3 c3 realtime 100000 20 single`, RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=5, arms interleaved per rep ×8, seeds 42+7; driver `/home/vibe/rstar2_battery.sh`, logs `/home/vibe/percap85/c3rt-s{42,7}.log` + per-run sender `diag-*.log`; harness commit adds the RWM_TAPER_R forward — the knob was not previously plumbed)

| arm (RWM_TAPER_R × RWM_RSTAR_TAIL) | s42 delivered (per-rep /20) | s7 delivered (per-rep /20) | cod/src s42 | cod/src s7 |
|---|---|---|---|---|
| off × legacy (LN)   | 110/160 = **68.8%** [15 13 13 17 13 15 13 11] | 85/120 = **70.8%** [14 15 13 13 15 15] (n=6) | 0.064 | 0.072 |
| off × corrected (TN)| 122/160 = **76.2%** [13 16 12 17 18 15 16 15] | 44/60 = **73.3%** [15 14 15] (n=3) | 0.070 | 0.091 |
| ON × legacy (LB)    | 61/140 = **43.6%** [12 7 6 8 9 9 10] (n=7) | 41/80 = **51.2%** [11 10 11 9] (n=4) | 0.323 | 0.349 |
| ON × corrected (TB) | 70/160 = **43.8%** [10 8 7 9 9 8 11 8] | 44/80 = **55.0%** [10 13 11 10] (n=4) | 0.353 | 0.321 |

All three EXPECT items land, on both seeds:

- **(a) The wire consumes r as computed — CONFIRMED at L1.** cod/src
  0.064–0.091 (fix OFF, the #46 inertness reproduced on the real netem
  substrate) → 0.321–0.353 (fix ON), ~4–5×, inside the L0-predicted
  0.2–0.35 envelope; `budget-conserving taper emission ACTIVE` echo on
  every ON run, absent on every OFF run. This is the only claim netem-L1
  can prove (gemodel = GE; the heavy-tail rung stays the L0 shim).
- **(b) Consuming r DEGRADES delivered reliability — the L0 −22 pp
  reproduced on the real substrate**: −25.2 pp pooled at s42
  (72.5→43.7 %), −19.0 pp at s7 (71.4→53.1 %); ≈2.5–3× the per-rep
  spread (σ_rep ≈ 1.8–2.1 objects), every ON rep ≤ every OFF rep's mean.
  **The leading-window (unsolvable-span) entanglement attribution is
  CONFIRMED, not falsified — the RWM_TAPER_R flip stays CLOSED** and the
  solvable-span emission follow-up remains the named next task
  (§16.20.4's rescope of the −22 pp to the streaming family stands).
- **(c) r* arms tied at the wire — spare-cap compression confirmed**:
  legacy-vs-corrected Δcod/src ≤ 0.03 in both emission modes (controller
  probe says 1.8×); delivered Δ = +7.4 pp (s42) / +2.5 pp (s7) in favor
  of corrected in the OFF arms — ≤ ~0.8 σ_rep, inside noise (recorded,
  not claimed).

**Verdict.** `RWM_TAPER_R` stays DEFAULT OFF; #46's quantity defect
remains fixed-as-mechanism; realizing r* at the realtime plain wire
remains blocked by the two named binders (spare-cap compression,
leading-window span) — unchanged by L1, now with the substrate's
signature on it. Caveats: seed-7 lost 15/32 invocations to the known
topo-ping double-abort (TN kept only n=3 reps — its per-rep values are
tight and agree with s42; no captured result discarded); zero lost at
s42 except one LB invocation.

## r* Bursty-Loss Provisioning (2026-07-13) — the GE 2-4x under-provisioning FIXED: r* now provisions against the receiver's MEASURED window loss-mass quantile (paper §8.4.1); oracle-validated on the #43 real traces (feasible-cell worst residual 2.88x → 1.41x, GE control tracks §8.7 exact, heavy-tail synthetic 5.1x-miss → 0.99x-hit); shipped default RWM_RSTAR_TAIL=1 (branch `feat/rstar-bursty`, task #46)

*Decision record: → [ADR-0063](adr/0063-rstar-window-mass-provisioning.md)*

**The problem (from #43 / paper §2.5).** r* was derived for GE-geometric
bursts. Real traces carry (i) burst-length tails 3.8x–26x heavier than
geometric AND (ii) burst CLUSTERING (lag-20 memory 5x–4100x GE), so the
delivered window-failure missed the δ/ε target by 2–4x beyond the GE-ideal —
worst 12.8x the target — even at 55–100% overhead. The realtime profile
(δ small, no in-window retransmit) is where this breaks the (δ, ρ) contract.

### The derivation chosen (paper §8.4.1) — and why

Candidate (b) of the task (quantile provisioning), applied to the RIGHT
statistic. Two findings forced the final form:

1. **The exact failure statistic is window loss MASS, not burst length.**
   A window of W source + R = rW repairs (N slots) fails iff total losses
   K_N > R — independent of how many losses hit repairs (a repair loss
   removes one loss AND one repair). First implementation used the
   single-burst-length quantile (task hint (b) literally): it PASSED the
   controlled heavy-tail synthetic but still missed 2.4–10x on the real
   traces, because real windows die from CLUSTERED bursts — two 20-loss
   bursts kill like one 40-loss burst. Provisioning the mass quantile
   subsumes burst tails, clustering, and loss/repair correlation at once.
   (This is exactly §2.5's own recommendation: "the empirical window-loss
   quantile rather than the Gaussian/GE tail".)
2. **Measure at the window's own scale.** A single-scale statistic +
   union bound over-provisioned ~3x on GE. The estimator therefore tracks
   the sliding m-block mass tails for m = 1..8 blocks of w0 = 64 slots
   (`MassStats`) and the solver reads the tail at the scale matching
   N = W(1+r), interpolating linearly in probability (the conservative
   side). Tail extension beyond observation: discrete-Weibull
   S(t) = θ^(t^k) fit from the two decayed conditional moments — k = 1 IS
   the geometric law, so a GE channel measures itself back (no new
   contract parameters; same decayed-counter pattern as the GE counts).

Solver: `r_star_mass` = least r on [0, 2] with F(r) ≤ δ_wf = δ/ε;
production emits max(r*_§8.4, r*_mass). Continuity preserved (Bulk χ=0
identity r*(δ=ε̂)=0 survives; term inert until 30 nonzero-mass blocks =
cold start unchanged). Infeasible contracts (fades no in-window rate ≤ 2.0
covers) return the ceiling — DECLARED, not silently missed.

**Level rescale (added after `test_full_control_loop` caught it).** The
mass moments decay once per BLOCK sample (long memory — rare tails need
samples), which made regime-DOWN adaptation ~64× slower than the BOCD
level estimate (10%→1% regime: r stuck at max_overhead). Fix: level
equivariance — the tail is read at the current level,
P(K>R) = T(R·ε_mass/ε̂_now) with ε_mass = p_nz·m1/w0 the level the mass
stats embody and ε̂_now the BOCD upper quantile. The term now follows
level changes at estimator speed while the tail SHAPE keeps its long
memory; ε̂_now being the conservative upper keeps the architecture's
estimation-uncertainty layering. (Paper 8.4.1 "Level rescaling";
`test_r_star_mass_level_rescale`; control_loop suite green again.)

### Trace-suite delivered reliability (oracle, `rstar_tail_validation.rs`, W=50)

Old = §8.4 closed form (production pre-#46); New = max(old, r*_mass), both
fitted from the trace itself; delivered = block-replay window-failure /
target (1.00x = exactly on target).

GE-SYNTHETIC control (2M symbols, seed 42 — no over-provisioning check):

| cell | tgt δ_wf | r_old | r_new | r*_exact (§8.7) | del_old | del_new |
|---|---|---|---|---|---|---|
| WiFi 2.5% | 0.05 | 0.090 | 0.099 | 0.090 | 1.56x | 0.96x |
| WiFi | 0.02 | 0.105 | 0.137 | 0.130 | 2.41x | 0.90x |
| LTE 4.8% | 0.05 | 0.151 | 0.176 | 0.170 | 1.19x | 0.87x |
| LTE | 0.02 | 0.177 | 0.239 | 0.230 | 2.17x | 0.80x |
| Sat 9.1% | 0.05 | 0.265 | 0.331 | 0.310 | 1.63x | 0.73x |
| Sat | 0.02 | 0.306 | 0.434 | 0.390 | 2.67x | 0.60x |

r_new tracks r*_exact (×0.92–1.11): on GE the correction converges to what
the GE world itself requires — the +13–42% over r_old is §8.7's own
documented closed-form shortfall (r_old misses its target 1.2–2.7x even on
GE), not heavy-tail over-provisioning.

REAL traces (#43 derivation; "NO" = solver declared infeasible in-window):

| trace | eps | tgt | r_old | r_new | feas | del_old | del_new |
|---|---|---|---|---|---|---|---|
| Verizon-LTE-short | 8.0% | 0.05 | 0.234 | 1.190 | yes | 2.88x | 1.41x |
| Verizon-LTE-short | | 0.02 | 0.270 | 2.000 | NO | 6.77x | 1.03x |
| ATT-LTE-driving | 13.5% | 0.05 | 0.483 | 2.000 | NO | 3.77x | 1.58x |
| ATT-LTE-driving | | 0.02 | 0.564 | 2.000 | NO | 8.05x | 3.95x |
| TMobile-UMTS-driving | 24.5% | 0.05 | 0.924 | 2.000 | NO | 5.10x | 4.29x |
| TMobile-UMTS-driving | | 0.02 | 1.073 | 2.000 | NO | 12.77x | 10.74x |
| TMobile-LTE-short | 8.4% | 0.05 | 0.407 | 1.381 | yes | 2.12x | 1.37x |
| TMobile-LTE-short | | 0.02 | 0.485 | 2.000 | NO | 5.28x | 2.44x |
| Verizon-LTE-driving | 5.2% | 0.05 | 0.192 | 0.701 | yes | 1.72x | 1.00x |
| Verizon-LTE-driving | | 0.02 | 0.226 | 2.000 | NO | 4.30x | 1.46x |

- FEASIBLE cells: worst residual 2.88x → **1.41x** (Verizon-driving lands
  exactly 1.00x). The residual above 1x is NON-STATIONARITY (one moment
  set for a drifting trace) — documented, not hidden.
- INFEASIBLE cells (6/10): deep multi-window fades (e.g. UMTS-driving at
  ε=24.5%: ~21% of windows sit inside fades no in-window r ≤ 2 covers).
  No solver can meet these in-window at W=50; the new solver SAYS so
  (ceiling) and still improves the residual everywhere (12.77x → 10.74x
  at worst). Feasibility restoration would need W growth (§8.8) or ARQ —
  a (δ, ρ, r) contract renegotiation, not a solver fix.
- HEAVY-TAIL SYNTHETIC (semi-Markov, Weibull k=0.5 bursts, ε=12.5%,
  documented params, 27k windows): old 2.4x/5.1x MISS → new 1.00x/0.99x
  HIT at r 0.418→0.778 / 0.486→1.268.

### Production deltas (shipped default ON; RWM_RSTAR_TAIL=0 = legacy A/B)

r at the standard cells (GE prewarm 200k symbols seed 42, W=64, saturation
cap active, max_overhead 0.5, target_tail_loss 1e-5):

| cell | hint | r_old | r_new |
|---|---|---|---|
| c2-WiFi | Bulk | 0.000 | 0.000 (χ=0 identity intact) |
| c2-WiFi | Auto | 0.119 | 0.213 |
| c2-WiFi | Realtime | 0.150 | 0.230 |
| c3-LTE | Bulk | 0.000 | 0.000 |
| c3-LTE | Auto | 0.167 | 0.244 |
| c3-LTE | Realtime | 0.206 | 0.255 |

The Auto/Realtime increases are BY DESIGN: on a GE cell the legacy closed
form under-provisions its own target (§8.7: r*_exact ≈ 1.5x closed form);
the new r converges toward the exact requirement and is then bounded by
the saturation cap (which is why Realtime ≈ Auto at both cells — r_sat
binds first). Bulk pays nothing anywhere (pure-ARQ identity). Cold start
(< 30 nonzero-mass blocks ≈ 2–3k symbols on these cells) is byte-identical
to legacy.

Scoping honesty: on bulk profiles ARQ covers residuals and the term is 0
via the contract itself (δ_eff = ε̂), not via a mode hack. The cost lands
only on tight-δ profiles on measured-bursty channels — the profiles whose
contract demands it.

### Gate suite (release): 15/15 after ONE principled recalibration

First run: 14/15 — `gate_vs_simquic_multipath` C8-dual-asym failed its
CI-separated 1.1x no-regression bound by a hair (fec 0.177±0.001 vs simquic
0.170±0.008 → mean+ci 0.178 vs bound 0.1782). Attribution CONFIRMED by
same-binary A/B: `RWM_RSTAR_TAIL=0` passes. Cause: the cell runs hint=Auto,
and the corrected Auto r* (~0.22–0.26) prices the honest contract where the
old bound was calibrated on a rate (~0.12–0.17) that under-delivered its own
target 2x+; dual source capacity with corrected overhead = 15/1.24 ≈
12.1 MB/s vs the FEC-free single-path 12.5 MB/s → floor ratio ≈ 1.03x
(measured 1.04x). Bound recalibrated 1.1 → 1.15 (justification comment in
the test; NOT a fudge — the physics of the declared overhead price).
Second run: **15/15** (`cargo test --test gate_suite --release`,
14 unchanged cells identical). raptorpath --lib 314/314; raptorpath-math
full suite (58 lib + formula 19 + monte-carlo 22 + multipath 4 +
real-trace #43 4 + rstar-tail 3 + temporal 23) all green.

### L1 spot check (VM 10.1.5.16, 2026-07-13 ~18:20–18:37 UTC): the wire arms are INDISTINGUISHABLE — the corrected r* is diluted by the plain-mode EMISSION path (a NEW, precisely-attributed instance of the §12.9/§8.4 substrate caveat), not refuted

**Method (MEASUREMENT DISCIPLINE).** Binary sha256 f6c68660a9db… built on
the VM from commit 4538a9b (COMMIT file records provenance). Cell: c3
single-path (netem `gemodel 2% 40%`, 20 mbit, 40 ms RTT + 5 ms jitter,
netem `seed $SEED`), hint=realtime, plain window mode. Same-binary
interleaved arms per rep: T = `RWM_RSTAR_TAIL=1` (shipped), L = `=0`
(legacy); x8 reps, seeds 42 AND 7. Full command + env per run in
`/home/vibe/rstar/c3rt-s{42,7}.log`, per-run sender DIAG in `diag-*.log`;
driver `tools/l1/rstar_battery.sh`. Delivered-reliability observable:
realtime's 20 ms reorder horizon << the ~90 ms ARQ round, so a loss not
recovered IN-WINDOW is force-delivered as an app hole and the 100 KB perf
object (~203 chunks @508 B) can never complete → per-object DNF fraction
IS app-level delivered reliability. `RWM_PERF_TIMEOUT_S=5` caps expected
misses (new env knob, src/perf.rs).

**Result (delivered reliability, objects completed):**

| arm | seed 42 | seed 7 | pooled |
|---|---|---|---|
| L (legacy r*) | 116/160 (72.5%) | 43/60 (71.7%) | 72.3% |
| T (corrected r*) | 115/160 (71.9%) | 68/100 (68.0%) | 70.4% |

Per-invocation DNF counts are BIMODAL on seed 7 (0 and 5–9 within the
same arm) and spread sd ≈ 1.7/20 runs on seed 42 — the noise floor is
several points of delivered fraction; the −2 pp pooled delta is inside
it. Harness caveat recorded: the 5 s timeout also applies to the WARM-UP
object, and on seed 7 half the invocations (3 T vs 5 L — non-differential)
aborted at warm-up ("tunnel not passing traffic") and are excluded; seed
42 had 16/16 clean invocations.

**Why the arms tie — the honest attribution.** Sender DIAG shows emitted
repair overhead cod/src ≈ 0.03–0.10 in BOTH arms (mean T 0.058/0.083,
L 0.060/0.098 by seed) — an order below either arm's r* (L 0.206, T 0.255
at this cell, verified at the controller by unit probe; the L0 gate suite,
which applies `compute_repair_rate` directly per symbol, DID shift — C8
recalibration above — so the solver is live where the rate is consumed
as computed). The plain-mode emission path is the diluting stage
(net/mod.rs ~4605): per source symbol it adds
`min(τ(taper_offset), spare)` to the repair debt with
τ(t) = r·q̂·(1−q̂)^t, and `taper_offset` resets only on CUMULATIVE-ACK
advancement — so total proactive repair ≈ Σ_t τ(t) = **r symbols per ack
cycle**, and an ack cycle at c3 BDP is hundreds of symbols: the emitted
overhead is ~r/cycle, nearly independent of r's magnitude. Raising r*
therefore cannot reach the wire in plain window mode — the same class of
substrate limitation §8.4's "Measured caveat (2026-07-08)"/§12.9 already
document for proactive repair, now attributed to the taper-reset
mechanism specifically.

**Verdict, scoped honestly.** The claim "new r* meets ρ where old missed"
is VALIDATED at the oracle rung (real traces + heavy-tail synthetic,
tables above) and at L0 (gate suite consumes r* directly), and is NOT
REALIZED at L1 in plain window mode because the emission scheduler — not
the solver — is the binding stage there. (Also noted: netem `gemodel` IS
GE, so even a faithful emission path would test the §8.7 closed-form-vs-
exact gap at this rung, not the heavy-tail gap; heavier-than-GE loss is
not expressible with netem.) Overhead delta at the wire: none measurable
(cod/src equal within noise) — the corrected r* costs nothing at L1
today for the same reason it fixes nothing there. FOLLOW-UP (out of #46
scope, named): make the plain-mode emission path honor the computed rate
per source symbol (or route realtime through the generation/pacer path
that does), then re-run this cell — the L1 realization of the corrected
contract lives or dies on that emission fix. [DONE 2026-07-18, task #85:
built (`RWM_TAPER_R` budget law) and L0-validated — the wire now consumes
r as computed, but the 2×2 did NOT separate: two further binders measured
(spare-cap compression, leading-window entanglement). See "Taper Emission
Fix" above.]

## FINAL CONSOLIDATED VERDICT (2026-07-08) — the aggregation/throughput arc

> **⚠ SUPERSEDED ERA (banner added 2026-07-19).** This was the settled
> position of the SYSTEMATIC-REPAIR ERA, measured on the qemu64 vCPU with
> quinn's stock Cubic silently underneath every arm (wall #1) and the PMTU
> black-hole wedge live (wall #2). Its structural findings stand — the
> presence⊥throughput identity, and recovery-latency serialization at THAT
> era's operating point — but its ceilings do not: "15–17 Mbit link",
> "C8 bounded at ~parity (14.7)", "NOT a faster-bulk-transfer transport"
> were substrate artifacts (plain+BBR single 76–79; C7 136–147 = 0.87–0.97×Σ;
> C8 72–76 = 0.74–0.80×Σ post-divide). Read
> **"CONSOLIDATED VERDICT (2026-07-19)"** at the top of this file for the
> current map; this section is retained as the record of its era.

This is the single honest capstone for the heterogeneous-multipath-aggregation
+ FEC-throughput arc, which is now CONCLUDED. It supersedes the scattered
per-branch verdicts below and reconciles the (pre-arc) L3 REGIME MAP with
everything the arc measured. Read it as the settled position; the dated
sections that follow are the primary record it summarizes.

**[AUDIT 2026-07-13: the UPDATE blockquote below is VOIDED — generation-inert
measurement.]** The §16.10 DAPS battery it cites was (at best) unverified: the
harness never enabled generation and the DAPS arms are classified UNCERTAIN,
leaning INVALID (see "Methodology Audit (2026-07-13)"). The 0.48×→0.80× lift,
paused 13–68%→0%, and the "scheduling-bound AND queue-bound, not solely
recovery-latency-bound" revision are not validly established. The 2026-07-08
verdict body below rests on the systematic-repair era (flags recorded in the
ledger) and STANDS. Valid generation-ON numbers: "Generation-ON Re-Baseline
(2026-07-13)".

> **UPDATE (2026-07-12), see "DAPS + Right-Sized FEC" at the end of this file.**
> The "bounded by recovery latency" conclusion is PARTIALLY REVISED for C8: the
> heterogeneous cap was substantially the cost-based-CURRENT placement stranding
> the slow path at the frontier, NOT purely recovery latency. Delay-aware (DAPS)
> arrival-aligned scheduling — the slow path carries FUTURE data offset by the
> latency skew — plus right-sized FEC (r*≈0.03 not 0.10) lift C8 from 0.48× to
> **0.80× single-fast** (frontier pause 13–68% → 0%). It does not yet cross
> parity; the residual is slow-path bufferbloat (queue-bound), a BLEST-style
> follow-on. The regime is scheduling-bound AND queue-bound, not solely
> recovery-latency-bound.

### The scientific conclusion (the core result)

**Reliable-delivery THROUGHPUT on a lossy link is bounded by RECOVERY
LATENCY.** You cannot deliver data you have not yet recovered, and recovery
costs time. FEC's only escape — recover a loss *without* spending a round-trip
— requires SPARE bandwidth to carry the repair. A saturated reliable path has
no spare (the repair displaces the source it would otherwise carry: the
**presence⊥throughput identity**), and multipath's slow path cannot supply it
beyond parity, because the slow path hits the same in-order cumulative-ack
frontier bound the fast path does. This was established EXHAUSTIVELY over ~15
L1 investigations. Every lever was tried and ruled out WITH a mechanism, not
by exhaustion of ideas: coding structure (block vs window vs generation),
repair rate r, cross-path repair placement (repair on the spare path), decode
speed (fast dense decoder), sender pacing, reactive-ARQ bounding
(once-per-SRTT deficit), receiver-tail parallelization, out-of-order delivery
(H → ∞), and SACK sender flow-control decoupling. The sender was PROVEN never
the bottleneck: composing the SACK-prune sender decoupling with a BDP
reassembly clamp made decoupling reliable (dnf 0, buffer bounded ≈ BDP, every
byte delivered) and it measured FLAT (16.5 → 17.1 → 17.2 Mbit/s, within run
noise). The independent-path assumption was explicitly ruled out too: the
netem paths are independent qdiscs and the bound still holds. The binding
constraint is receiver-side recovery latency — a hole walks the in-order
frontier at ≈ 1 ARQ round / RTT — which is structural to reliable in-order-
capable delivery and unmoved by any sender-side law.

### What is PROVEN (measured wins — the real return)

- **Beats native quinn on CLEAN links.** An accidental O(n²) — a full rescan
  of the ~20–30k-element RTT-sample deque on every ACK
  (`CopaState::record_rtt`) — capped everything at ~15 Mbit and produced the
  false "not bandwidth-limited / per-symbol processing ceiling" evidence.
  Fixed with a monotonic-deque windowed-min (byte-identical `min_rtt`, so CC
  unchanged): single-path clean 28 → 86 Mbit (3.0×) and it now SCALES with
  bandwidth (166 Mbit @ 1 Gbit, 5.6×). raptorpath clean-100Mbit (86) now
  exceeds the quoted native-quinn-at-C2 rate (72).
- **FEC recovery was silently DEAD, now revived.** A decoder defect
  (`GenerationDecoder` froze its known-source set at slot creation and never
  admitted late sources) left arriving repairs 99.85% redundant
  (`repairs_useful` 0.15%) — every prior "FEC-vs-ARQ" number was
  ARQ-with-FEC-overhead. Fixed (inject late sources into the live matrix):
  `repairs_useful` → 66–72%. With the decoder alive AND the reactive-ARQ
  over-request bounded (once-per-SRTT deficit + repair-wait coalesce, which
  collapsed a 30,703-symbol ARQ flood to 437), FEC went from LOSING (0.88×) to
  PARITY with ARQ (0.99×), with a slight edge (1.04×) at high RTT/loss
  (RTT200/10%).
- **Tail latency.** 12–60× better message-p99 than QUIC/kernel-TCP on lossy
  moderate-RTT single links (Metric A below).
- **Predictability.** ~93× lower completion-time variance under high loss;
  DNF-free where loss-reactive ARQ (CUBIC) cascades to collapse.
- **Symmetric multipath aggregation.** C7 ×1.26–1.43 dual-over-single across
  the arc's runs — beats kernel MPTCP, whose subflows collapse under the loss
  raptorpath's FEC absorbs.
- **Surrounding correctness.** Loss-blind Copa CC vindicated (cwnd grows under
  loss; never the collapse cause). Bufferbloat fixed (RTT 410 → 40 ms).
  Small-G frontier-advance decoder deadlock fixed. SACK sender-decoupling made
  reliable (invariant holds, buffer bounded ≈ BDP). Broken present-at-stall
  frontier probe fixed (was structurally 0 in generation mode).
- **Surrounding rigor.** A verification oracle that caught real errors
  including its own over-modeling; unified deadline-constrained r* (§8.9) with
  the N=1 → §8.4 reduction proven; real-trace GE validation showing GE
  under-provisions r* on real bursty loss (→ task #46).

### What is BOUNDED (honest limits)

- **Heterogeneous multipath THROUGHPUT aggregation (the C8 bar: >15.7 Mbit/s,
  factor > 1): NOT achieved.** Bounded at ~parity — best dual C8 (c2+c3) is
  the plain-systematic baseline at **14.70 Mbit/s (0.97× fast-alone)**, and
  every cross-path-repair arm is STRICTLY worse. The independent-Monte-Carlo
  oracle predicts ×1.19, but that oracle does NOT model the in-order frontier
  recovery-latency serialization, which is the binding L1 constraint; it is a
  sound theoretical target, not a realized production number. Closing the gap
  needs a recovery-pipeline redesign (pipelined per-RTT frontier recovery, or
  a genuinely rateless ack-frontier so a hole is never a fixed in-order
  position), plus a per-path (not summed-across-paths) outstanding cap — and
  even the corner that was probed (out-of-order H → ∞; SACK+BDP decoupling)
  measured flat single-path and REGRESSED C8.
- **Single-path reliable BULK throughput on a saturated link: FEC = ARQ, at
  parity, is the max.** This is the presence⊥throughput identity again: on a
  saturated reliable path there is no spare to carry a repair that would let a
  loss decode without a round-trip. This is ARQ's home turf; it is not an
  engineering gap to close but a property of reliable delivery.

### STALE-NUMBER RECONCILIATION (read the L3 REGIME MAP with this)

The L3 REGIME MAP's **Metric B — object COMPLETION** numbers (rp LOSES ~4–8×
at C2/C3, worse at C4/C5) are **PRE-CPU-FIX and now STALE/SUPERSEDED.** They
were measured before the O(n²) RTT-rescan fix that gave single-path clean
28 → 86 Mbit (3.0×) and 1 Gbit 29 → 166 Mbit (5.6×). The honest current
direction: single-path throughput now BEATS quinn on clean links and sits at
ARQ PARITY under loss, so the old "loses 4–8× on completion" no longer holds.
A precise post-fix completion re-measure across C1–C5 is the honest
follow-up — flagged as **TODO (not run; this is a doc-consolidation task,
no VM)**. The Metric B table below is annotated inline as stale. Metric A
(tail latency) and Metric C symmetric-multipath (C7) wins STAND; the C8 row
is updated to the final bounded-at-parity position with the recovery-latency
mechanism.

### raptorpath's value proposition (one paragraph)

raptorpath is a transport whose value is **PREDICTABILITY and TAIL LATENCY on
lossy links**, plus **symmetric multipath aggregation** and **beats-quinn
throughput on clean links** — for latency-sensitive and lossy-link workloads
(live media, messaging, RPC over WiFi/LTE/satellite-class links, and any link
lossy enough to break loss-reactive TCP). It is **NOT** a faster-bulk-transfer
transport, and the arc proved that is not a gap to close but a property of
reliable delivery: reliable throughput is recovery-latency-bound, and FEC
cannot buy a round-trip back without spare bandwidth a saturated reliable
path does not have.

## FMTCP Aggregation Build (2026-07-08) — the literature-blessed retry, MEASURED

*Decision record: → [ADR-0066](adr/0066-deprecation-register.md) (re-test clause; triage in [VISION-TRIAGE-2026-07](adr/VISION-TRIAGE-2026-07.md))*

Built and measured the FMTCP/SCDP-class **pure decode-on-total** config — the
empty quadrant the arc never tested — behind the composite env-gate `RWM_FMTCP`
(shipped path byte-untouched, default off). This flips BOTH capping levers at once
on the systematic-repair generation submode and fixes the enumerated adjacent bugs
(#64 summed-anchor BDP; #59/#60 deficit re-flood). **Result: REFUTES the oracle's
×1.19 target at C8 — it REGRESSES heterogeneous C8 below parity — while confirming
strong SYMMETRIC aggregation and the recovery-latency mechanism of the FINAL
CONSOLIDATED VERDICT.**

**The 4 changes (bulk/object profile only):** (1) total-in-flight flow control —
the sender pipelines a BOUNDED number of generations past the in-order frontier
(win-backstop `(pipeline+2)·G`), not stalling on a hole, and the receiver's total
decode count `d` rides back on `WindowAck.cumulative_received` for observability;
(2) per-path (not summed-anchor #64) BDP in-flight cap — each path capped at its
OWN `gain·BtlBw_i·RTprop_i`, full only when NO path has room; (3) fungible cross-
path fountain repair, per-seq ARQ off, once-per-RTT deficit coalesce; (4) OOO
retention decouple + receiver reassembly clamp (never-evict), decode-on-total via
the generation decoder. r = 0.10 (RWM_GEN_R).

**Oracle param-confirm (temporal_oracle PART 5c, new test — PASSED).** The SHIPPED
params (r=0.10, G=384, one-feedback-per-RTT, per-path BDP) reach **×1.190 at C8**
(ceiling ×1.195), **0 ARQ, 0 idle slots, emergent in-flight 195 ≈ aggregate BDP
145** (no #64 bufferbloat). So the design is sound IN THE MODEL at the exact
production params.

**DECISIVE L1 (25 MB × 6, independent netem GE qdiscs — favorable, no path
correlation; VM shared, waited for free):**

| Arm | mean | median | stdev | factor vs single-fast | dnf |
|---|---:|---:|---:|---:|---:|
| **C8 het (c2+c3) FMTCP r=0.10** | **7.58 Mbit/s** | 26.6 s | 12.3 s | **0.48×** | **0** |
| C8 het FMTCP r=0.20 (raise ε) | 10.43 Mbit/s | 19.0 s | 4.2 s | 0.67× | 0 |
| C8 het plain systematic (baseline) | 14.37 Mbit/s | 13.8 s | 1.7 s | 0.92× | 0 |
| single-fast FMTCP (parity, denom) | 15.65 Mbit/s | 13.1 s | 0.55 s | — | 0 |
| **C7 sym (c2+c2) FMTCP** | **25.39 Mbit/s** | 8.1 s | 0.57 s | **1.62×** | 0 |

**Verdict: the C8 bar (>15.7, factor>1) is NOT met — and FMTCP is STRICTLY WORSE
than the plain baseline at C8** (14.37 → 7.58, ×0.92 → ×0.48). The total-in-flight
decouple AMPLIFIES the heterogeneous slow-path long pole rather than escaping it.

**Occupancy / the oracle signature — which parts held, which failed:**
- HELD: in-flight/reassembly bounded (`[REASM]` max_span ≈ 1520 ≈ 4·G, max_pending
  ≈ 990 — NOT the whole object). The win-backstop anti-bufferbloat bound worked;
  reliability held (dnf 0 every arm, every byte delivered/reassembled-by-offset).
- FAILED: the oracle's **0 idle slots** — the C8 sender is TUN-paused **13–68 %**
  of iterations (`RWM_DIAG`): the recovery-latency stall the oracle does not model,
  measured directly. And the ×1.19 became ×0.48–0.67 (anti-aggregation).

**Mechanism (the honest residual — real-vs-model, not path correlation).** The
netem paths are independent qdiscs, so it is NOT path correlation. It is the
production **recovery scheduling**: a heterogeneous-path generation that loses more
than its budget strands, recovers over a bufferbloat-inflated RTT (MEASURED RTT
spikes to ~2 s), and the total-in-flight decouple lets the frontier run past it so
the object waits on the slow tail (high variance: min 13 s ≈ near-baseline, max
45 s crawl). Two failure modes were traversed and are documented in-code: exempting
recovery from the congestion cap → 2 s bufferbloat; gating it → the stranded
generation starves/wedges. ε under-provisioning (GE, per Finding 5) is SECONDARY:
r 0.10 → 0.20 lifted 7.58 → 10.43 and cut variance 12.3 → 4.2 s but did not reach
parity. On SYMMETRIC paths (no slow path, no long pole) the identical build
aggregates cleanly at ×1.62 — better than the arc's prior ×1.26–1.55.

**This is exactly the FMTCP abstract's OWN stated pathology reproduced, not
escaped** ("a subflow experiencing high delay and loss becomes the bottleneck").
It doubly-confirms the FINAL CONSOLIDATED VERDICT: the C8 heterogeneous bound is
production recovery-latency, and flipping both levers cleanly does not cross it.
Gate 15/15 green; `cargo test -p raptorpath --lib` + `-p raptorpath-math` green
(incl. PART 5c param-confirm + two FMTCP lever unit tests + the fmtcp_loopback
reliability guard). Env-gated, default-off; shipped path byte-untouched.

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
   with a stable anchor (oracle ×1.19, no drag) — ACHIEVABLE in theory, a
   build recommendation, not yet built. **[FINAL 2026-07-08:** the arc's
   subsequent production builds (working FEC decoder, bounded reactive ARQ,
   cross-path spare-path repair, SACK+BDP sender decoupling) all BOUNDED C8
   at ~0.97× fast-alone — the oracle's ×1.19 does not model the in-order
   frontier recovery-latency serialization that is the binding L1 constraint.
   Heterogeneous throughput aggregation above the fast path is BOUNDED in
   production; see the FINAL CONSOLIDATED VERDICT at the top of this file.**]**
3. Tail latency (the model's thesis): VALIDATED vs kernel TCP at C2 —
   p99 91 ms (bulk) / 513 ms (realtime) vs 13,300-13,400 ms for BOTH
   kernel CCs at equal p50. Open: quinn message-tail comparison
   (needs a QUIC echo tool), C3/C5 tails not yet won, realtime
   streams silently fail at c3/c5 (open diagnostic).
4. Object completion vs modern stacks: ~~NOT YET WON at L1/L2 (5-6x to
   quinn at C2 even warm); the improvement loop owns the pipeline gap.~~
   **[SUPERSEDED 2026-07-08:** the "5–6× to quinn" was the O(n²) RTT-rescan
   CPU cap. Fixed → single-path clean BEATS quinn (86 vs 72 Mbit) and lossy
   single-path FEC reaches ARQ PARITY (0.99×). The "not yet won" framing no
   longer holds; a precise post-fix C1–C5 completion re-measure is the honest
   TODO (not run). See FINAL CONSOLIDATED VERDICT.**]**
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

> **⚠ SUPERSEDED ERA (banner added 2026-07-19).** Pre-arc baselines with
> 2026-07-08 patches, all pre-divide (qemu64) and pre-substrate-chain
> (stock Cubic underneath, PMTU wedge live, 1024-pool flow control). What
> survives: the Metric A tail-crown CLASS (12–48/60× — re-confirmed 2026-07-08
> and defended at the 2026-07-19 flip gate) and the C5 beats-CUBIC DNF-free
> property. Every throughput/completion row and both multipath rows are
> era-bound; the C8 "bounded at ~parity" verdict was walls #1/#2/#7, since
> dissolved. Current map: **"CONSOLIDATED VERDICT (2026-07-19)"** at the top
> of this file. Retained as the record of its era.

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

> **⚠ STALE / SUPERSEDED (2026-07-08) — PRE-CPU-FIX numbers. Do not cite.**
> This table predates the O(n²) RTT-rescan fix (`CopaState::record_rtt`
> monotonic-deque windowed-min) that lifted single-path clean throughput
> 28 → 86 Mbit (3.0×) and 1 Gbit 29 → 166 Mbit (5.6×), and predates the FEC
> decoder-revival + bounded-reactive-ARQ work that took single-path lossy FEC
> from 0.88× to ARQ PARITY (0.99×). The "rp LOSES ~4–8×" verdicts below NO
> LONGER HOLD: single-path throughput now BEATS quinn on clean links and is at
> ARQ parity under loss. A precise post-fix completion re-measure across C1–C5
> is the honest follow-up — **TODO, not run** (doc-consolidation task, no VM).
> See the FINAL CONSOLIDATED VERDICT at the top of this file.

| cell | rp-native (STALE, pre-fix) | best baseline | verdict (STALE) |
|------|-----------|---------------|---------|
| C1 DC | ~0.025 s | quinn 0.027 / BBR 0.028 | **PARITY** (rp ≈ or slightly ahead) |
| C2 WiFi | 0.83 s | quinn **0.20** / BBR 0.22 | ~~rp LOSES ~4×~~ (stale — now ARQ parity) |
| C3 LTE | ~7.3 s | quinn **0.90** / BBR 1.0 | ~~rp LOSES ~8×~~ (stale — now ARQ parity) |
| C4 Sat | ~56 s (tunnel) | quinn **1.09** / BBR 3.6 | ~~rp LOSES badly~~ (stale — re-measure) |
| C5 BadWiFi | 17.4 s | quinn/BBR **0.55**; CUBIC **DNF** | rp LOSES to quinn/BBR; **BEATS CUBIC** (DNF-free stands) |

### Metric C — MULTIPATH goodput, dual path (50 MB)
| cell | rp dual | best baseline | verdict |
|------|---------|---------------|---------|
| C7 WiFi+WiFi (sym) | **20.8–23.9 Mbit/s** | MPTCP 15.4 | **rp WINS ×1.26–1.55** (symmetric aggregation intact) |
| C8 WiFi+LTE (asym) | **14.70 (0.97×)** | MPTCP 12.6 | **BOUNDED AT ~PARITY** — the C8 bar (>15.7, factor>1) NOT met; every cross-path-repair arm strictly worse. Binding constraint: in-order-frontier recovery-latency serialization the slow path cannot parallelize (oracle predicts ×1.19 but does not model it). See FINAL CONSOLIDATED VERDICT. |

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
It reaches PARITY on clean/low-loss links and asymmetric multipath.

**[UPDATED 2026-07-08 — the "trails 4–8×" framing below is SUPERSEDED.]** The
original paragraph here said raptorpath "trails 4–8× on lossy single paths"
for bulk. The aggregation/throughput arc overturned the numbers: the O(n²)
RTT-rescan fix made single-path clean throughput BEAT quinn (86 vs 72 Mbit),
and the FEC decoder-revival + bounded-reactive-ARQ work took single-path lossy
FEC to ARQ PARITY (0.99×). So single-path bulk is no longer a 4–8× loss — it
is beats-quinn-clean and ARQ-parity-under-loss. What the arc DID confirm as a
genuine bound: reliable bulk throughput is recovery-latency-bound, so FEC =
ARQ parity is the ceiling on a saturated reliable path (the presence⊥
throughput identity), and heterogeneous multipath bulk aggregation (C8) is
bounded at ~0.97× fast-alone. BBR still wins the low-rate tail at C3.
Boundary rule of thumb: **choose raptorpath above ~1% loss when tail latency,
predictability, or symmetric multipath matters; single-path bulk is at
parity (clean: ahead), and heterogeneous-multipath bulk aggregation above the
fast path is a bounded open problem, not a shipped win.** See the FINAL
CONSOLIDATED VERDICT at the top of this file for the settled position.


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

## Transport Substrate Fix — the three named defects, built and measured (branch `feat/transport-substrate`, 2026-07-08)

The prior section pinned the high-RTT FEC loss on the TRANSPORT substrate (not the
FEC coding) and named three defects + a path to a win. THIS rung **builds all
three fixes**, measures each incrementally, and reports honestly. **Result: the
three defects are decisively FIXED at the mechanism level — the reactive runaway
and its DNF are eliminated, the proactive fraction is restored (0.04→0.90), and
the transfer is stabilized (stdev 7.2 s→0.6 s) — and FEC/ARQ improves from the
prior 0.55× to ~0.85× at RTT200. But there is STILL NO crossover: proactive FEC
does not beat ARQ at high RTT.** The residual is a FOURTH, receiver-side + regime
constraint, characterized below. All knobs are env-gated, default-off — the
shipped/gate path is byte-identical; `gate_suite` 15/15, lib 268, math green.

### The three fixes (each `run_window_sender` / `GenerationEncoder`, env-gated)
- **Fix 1 — CC-rate pacing (`RWM_CC_PACE`).** The systematic source rode the
  droppable QUIC-datagram path driven only by TUN intake, gated by a BDP-sized
  WINDOW but NO RATE — so at high RTT it BURST-overran (defect #1). Now the source
  AND coded emission draw from a token bucket paced at the link rate with a small
  burst (no BDP-sized burst). Rate signal = **max(Copa cwnd/SRTT, delivered-goodput
  EWMA)×headroom**: the goodput EWMA alone is clocked on the IN-ORDER ack, which
  stalls at 0 on any hole (pinning the pace at the 24 Mbit bootstrap floor and
  THROTTLING the ramp below ARQ); the frontier-independent cwnd/SRTT (cwnd grows
  to MAX on delivery feedback regardless of the hole) lifts that throttle.
- **Fix 2 — bounded reactive under CC (`RWM_REACT_CAP`).** The deficit loop was
  exempt from the congestion cap and re-emitted the reported residual on EVERY
  decode-progress report (sub-RTT), each resetting the in-flight baseline, so it
  re-sent ~the full deficit every few ms → **MEASURED recovery_coded 60 k–252 k
  for a ~5 k-symbol object (up to 120×), DNF at RTT200** (defect #2). Fix: **per-
  generation RTT-spacing** (act on a generation's deficit at most once per SRTT —
  "send the deficit, wait ~RTT, re-evaluate") + non-exempt from the in-flight cap.
- **Fix 3 — out-of-order retention/coding decouple (`RWM_OOO_RETAIN`).** Generation
  backpressure capped the send frontier at ~3 generations behind the CUMULATIVE
  (in-order) ack, so one hole stalled the pipeline ∝1/RTT (defect #3). Fix:
  `GenerationEncoder` gains a `code_base` (proactive-coding floor) decoupled from
  the retention floor — `set_code_base` follows the SEND frontier so fresh
  generations get their upfront budget while a stalled generation is left to the
  (now bounded) reactive tail — and the backpressure window widens to `ooo_gens`
  generations. **Reliability preserved:** retention still drops on the in-order
  ack (`advance(ack+1)`), so a stalled generation's sources stay retained for
  reactive recovery; memory bounded by `ooo_gens·G`.

### Measured effect of each fix (single-path, 100 mbit, GE 1.3/50 ≈ 2.6 %, jitter=0)
Direct A/B at **RTT200, 8 MB** (the clear runaway-repro point; G=768, r=0.20):

| arm | mean Mbit/s | dnf | proactive_frac | recovery_coded | stdev_s |
|---|---:|---:|---:|---:|---:|
| ARQ (pure) | 1.26 | 0 | — | — | ~11 |
| FEC unpaced (prior) | 1.06 | **1** | **0.025** | **154 295** | 9.0 |
| FEC + Fix1 (pace) | 0.80 | 0 | 0.042 | 90 118 | 15 |
| FEC + Fix1+2 | 1.01 | 0 | **0.90** | **436** | 9.6 |
| FEC + Fix1+2+3 | 1.09 | 0 | 0.31 | 8 799 | **0.6–4** |

- **Fix 2 is decisive on the mechanism:** recovery_coded **90 118 → 436 (207×)**,
  proactive fraction **0.042 → 0.90**, and it removes the DNF. The reactive
  runaway — the named defect #2 — is eliminated.
- **Fix 3 collapses the variance** (stdev 9.6 s → 0.6 s at RWM_STORE≈5 gens): the
  slow-run outliers vanish. It trades some proactive fraction (the coding window
  following the frontier under-provisions a fraction of generations) for stability.

### RTT sweep — ARQ vs full-stack FEC (Fix1+2+3 + cwnd-pacing), 6 MB
| RTT (ms) | ARQ (Mbit/s) | FEC-fixes (Mbit/s) | FEC/ARQ | (prior FEC/ARQ) |
|---:|---:|---:|---:|---:|
| 50  | 4.68 | 3.55 | 0.76 | — |
| 100 | 2.43 | 2.06 | 0.85 | 0.77 |
| 200 | 1.26 | 1.09 | **0.86** | **0.55** |

**FEC/ARQ improves from the prior 0.55× to ~0.85× at RTT200 and holds ~0.76–0.86×
across RTT — up, tighter (lower variance), and DNF-free — but does NOT cross 1.0.**

### Why there is still no crossover — the FOURTH constraint (measured, RWM_DIAG)
At RTT200 **both ARQ (1.26) and FEC (~1.1) sit at ~1 % of the 100 Mbit link** — a
shared LATENCY-bound regime, not a bandwidth one. The DIAG trace shows the transfer
sends all source in a few seconds (`src→0`) then spends ~50 s draining a slow,
RTT-bound reactive TAIL: the pipe is near-EMPTY (`infl`≈0 ≪ cwnd), `good`=0 most of
the time with occasional decode bursts, RTT inflated to 0.4–1.1 s. The binding
residual is **receiver-side**: the receiver's reactive recovery is serialized
FRONTIER-FIRST (deficit reports cover only the frontier ± `MAX_REPORTED_GENS`, so
holes are recovered roughly in order), and each such round costs an inflated RTT.
Raising the proactive `r` (0.2→0.5→1.0) does NOT help — the proactive fraction was
already 0.90 and the extra coded only spends free bandwidth — confirming the tail
is round-trip-bound at the receiver, not proactive-coverage-bound at the sender.
This is exactly the directive's caveat ("the RECEIVER reassembly is bounded"): the
three SENDER-side defects are fixed, but a reliable bulk transfer's last-ε recovery
remains RTT-bound at the receiver, and at high RTT that ε costs as much as ARQ's
per-loss round-trip. So the 33×-per-hole physics still does not convert to a
throughput win on this reliable-bulk substrate.

### HONEST VERDICT
- **The three named transport defects are FIXED** (measured): datagram burst-overrun
  (Fix 1 pacing), reactive runaway + DNF (Fix 2, recovery_coded 90 k→436, DNF gone),
  in-order sender coupling (Fix 3, stdev 9.6 s→0.6 s). FEC/ARQ 0.55→~0.85 at RTT200.
- **Proactive FEC still does NOT beat ARQ at high RTT.** No crossover; FEC/ARQ
  ~0.76–0.86× across RTT {50,100,200}. The residual is a shared latency-bound
  regime + receiver-side frontier-serialized reactive tail — a FOURTH constraint
  below the FEC layer, receiver-side, not addressed by the three sender fixes.
- **Path to an actual win (identified, not built):** parallelize the receiver's
  reactive tail (report + recover ALL outstanding generations' deficits at once,
  not frontier-first — lift `MAX_REPORTED_GENS`, aggressive one-RTT tail flush) and
  cut the bufferbloat RTT inflation. Both are receiver-side / queue changes below
  the sender fixes shipped here.

### Controls / no regression
- All knobs env-gated, **default-off** → shipped + gate path byte-identical.
  `gate_suite` **15/15** release; `cargo test -p raptorpath --lib` **268** (incl. a
  new `set_code_base_moves_proactive_window_past_stalled_generation` Fix-3 unit
  test); `raptorpath-math` green.
- **ARQ unregressed:** clean (no-loss) ARQ **82.4 Mbit/s** (> the ~76 reference),
  c2r10 ARQ 18.2 — the ARQ path is untouched by these changes.
- **Reliability intact:** `dnf:0` on EVERY arm at every point (clean, c2r10, RTT
  50/100/200); every byte delivered (perf completion assert). Clean FEC 57.5 Mbit
  (`proactive_fraction=1.0`, recovery_coded=0) is the generation-mode coding tax,
  not a pacing throttle (pace ceiling ≈108 Mbit).

**Harness.** `~/l1/ts_sweep.sh <reps> <bytes> <arms> [scens…]` — arms `arq,fec,
fecpace,fecpace2,fecpace3` (Fix1 / +Fix2 / +Fix3); Mode-B env `BGEN/BR/BSTORE/
BINFLIGHT/BPIPE/CCHR/REACTCAP/OOORETAIN`. `perf_rwm_c.sh` propagates the new
`RWM_CC_PACE/CC_PACE_HR/REACT_CAP/OOO_RETAIN`.

## Receiver Tail + FEC Regimes — the tail parallelized, and measured in FEC's favorable corners (branch `feat/receiver-tail`, 2026-07-08)

The prior section pinned the residual on a FOURTH, RECEIVER-side constraint: the
reactive tail is serialized FRONTIER-FIRST (deficit reports cover only frontier ±
`MAX_REPORTED_GENS = 6`), so a lossy bulk transfer recovers ≈ one generation per
round-trip. THIS rung **builds the receiver-tail parallelization + a BDP-derived
recovery-queue cap** and measures in the regimes the physics says FEC should win
(higher loss, higher RTT, larger/steady-state transfers). **Result: the receiver
fix is mechanically real and DNF-free, and it buys dramatic tail-latency STABILITY
— but proactive FEC STILL does not beat ARQ on mean throughput in any tested
regime. The crossover is refuted a fourth time; the binding constraint is the
droppable-datagram SUBSTRATE, not the receiver report bound and not the coding
rate.** All knobs env-gated, default-off — shipped/gate path byte-identical;
`gate_suite` 15/15, lib **269** (incl. `receiver_tail_reports_all_deficits_in_one_round`),
math green.

### PART 1 — the receiver-tail fix (built, mechanism VERIFIED at L1)
- **Report ALL outstanding deficits (`RWM_REPORT_GENS`).** The `MAX_REPORTED_GENS = 6`
  cap (and the +7 anti-wedge seeding bound) is lifted to a configurable
  `report_gens`; the reporting logic is extracted to a pure `collect_gen_deficits`
  fn and unit-tested (50 outstanding generations → all 50 reported in ONE round vs
  6 under the legacy bound). Every in-flight hole is now NACKed in a single
  round-trip (parallel tail flush), not ≈6-per-round serially.
- **BDP-derived in-flight cap (`RWM_INFL_BDP`).** Total in-flight is bounded to
  gain × Σ Copa `bdp_anchor` (BtlBw·RTprop, bufferbloat-robust), gating BOTH
  proactive AND (Fix-2 non-exempt) reactive emission, so the parallel flush cannot
  re-bloat the recovery-round queue. Generation mode previously had only a memory
  bound (`store_max`), not a pipe bound.
- **Mechanism VERIFIED (L1, `RWM_TRACE`).** With the fix active on a wide store the
  `[RCV]` deficit reports were measured spanning **up to 11 generations in a single
  report** (> the legacy 6-cap), total residual up to **~5.2 k symbols requested at
  once** — the tail flush is genuinely parallel, not frontier-first.

### PART 2 — measured in FEC's favorable regimes (single-path, 100 mbit, jitter=0, GE loss)
**LOSS sweep, RTT 100, 25 MB, wide-store receiver-tail arm (r=0.20):**
| loss | ARQ (Mbit/s) | FEC-tail (Mbit/s) | FEC/ARQ | ARQ stdev_s | FEC stdev_s |
|---:|---:|---:|---:|---:|---:|
| 2.6% | 2.216 | 1.997 | 0.90 | 3.6 | **1.4** |
| 5%   | 1.395 | 1.250 | 0.90 | 12.3 | **1.3** |
| 10%  | 0.845 | 0.742 (dnf 1/2) | 0.88 | 16.9 | — |

**LOSS sweep, RTT 100, narrow-store receiver-tail arm (r=0.35, 15 MB):**
| loss | ARQ | FEC-tail | FEC/ARQ | ARQ stdev_s | FEC stdev_s |
|---:|---:|---:|---:|---:|---:|
| 10% | 0.68 | 0.69 | **1.01 (TIE)** | **61.6** | **0.66** |

**RTT sweep, r=0.35 narrow store, 15 MB:**
| RTT | loss | ARQ | FEC-tail | FEC/ARQ |
|---:|---:|---:|---:|---:|
| 200 | 2.6% | 1.197 | 0.924 | 0.77 |
| 200 | 10% | **DNF (2/2)** | **DNF (2/2)** | — (shared collapse) |

**r-sweep at RTT 100 / 10% (narrow store):** r 0.35 → 0.69 Mbit/s (pfrac 0.35);
r 0.60 → 0.62 Mbit/s (pfrac 0.40). **Raising r HURTS** — the extra proactive coded
are dropped at the link loss rate on the droppable substrate (pfrac pinned ≈ 0.4
regardless of r), adding overhead without buying coverage. This CONFIRMS the prior
"raising r doesn't help" observation and locates the constraint in the transport,
not the coding rate.

### The verdict — crossover REFUTED (4th time); the win is STABILITY, not throughput
- **No mean-throughput crossover in any tested regime.** FEC/ARQ ∈ [0.77, 1.01]:
  ≈0.90 flat across RTT 100 loss {2.6,5,10}%, 0.77 at RTT 200/2.6%, a TIE at RTT
  100/10%, and a shared DNF at RTT 200/10%. The receiver-tail parallelization
  removed the frontier-first serialization (verified) but throughput did not move —
  both arms remain recovery-round-bound at ~1% of link at high RTT.
- **The measured GAIN is tail-latency STABILITY.** At RTT 100/10% the receiver-tail
  arm completes with **stdev 0.66 s vs ARQ's 61.6 s (≈93× tighter)** at equal mean;
  ARQ's completion-time variance EXPLODES with loss (stdev 3.6 → 12.3 → 16.9 → 61.6)
  as it nears retransmit-cascade collapse, while FEC stays flat and DNF-free. FEC
  here buys PREDICTABILITY under loss, not higher goodput.
- **Root cause = the droppable-datagram SUBSTRATE (unchanged).** Proactive fraction
  stays ≈ 0.35–0.50 even at r=0.6 because proactive coded symbols are themselves
  dropped at the link loss rate (and/or arrive after the receiver's reactive NACK
  timer). Neither the receiver report bound (PART 1, now lifted) nor the coding rate
  (r-sweep) is binding; the QUIC-datagram loss substrate is. Same root cause as the
  three prior sessions.
- **A honest regression to note:** the WIDE store the parallelization exercises is
  net-negative at high loss — it tanks the proactive fraction (0.44 → 0.16 at RTT
  100) and floods reactive (rcod 10 k → 43 k), and at RTT 100/10% one rep DNFs. The
  narrow store (≤ ~5 generations, where the 6-cap already sufficed) is strictly
  better, which is itself evidence the receiver serialization was NOT the binding
  constraint at the tuned operating point.

### PART 3 — multipath: NOT warranted
The directive gates multipath on a single-path FEC win. There is none (FEC/ARQ ≤
1.01, a tie at best), so heterogeneous multipath was not run — there is no
single-path advantage to aggregate.

### Controls / no regression
- All knobs (`RWM_REPORT_GENS`, `RWM_INFL_BDP`) env-gated, **default-off** →
  shipped + gate path byte-identical. `gate_suite` **15/15** release; lib **269**
  (new pure-fn unit test); `raptorpath-math` green.
- **Clean-link controls (no loss, 20 MB):** ARQ **84.4 Mbit/s** (> the ~76
  reference — ARQ path untouched), full receiver-tail FEC **58.4 Mbit/s** (the
  generation-coding tax, matches the ~57.5 reference), **dnf:0** both.
- **Reliability intact** at every feasible operating point (narrow store, RTT
  50/100/200, loss 2.6/5%): `dnf:0`, every byte delivered. DNFs occur only in the
  extreme corners (wide-store RTT 100/10%, and RTT 200/10% where ARQ ALSO DNFs).

**Harness.** `~/l1/rt_sweep.sh <reps> <bytes> <arms> [scens…]` — arms `arq,
fecprior,fecwide,fectail` (tuned-narrow / wide-legacy / wide+receiver-tail);
env `BSTORE_TAIL/OOORETAIN_TAIL/REPORTGENS/INFLBDP` + the Mode-B knobs. New GE
higher-loss cells in `lib.sh`: `c2r{100,200}l{5,10}` (p solved for 5/10% mean at
q=50). `perf_rwm_c.sh` propagates `RWM_REPORT_GENS/RWM_INFL_BDP`.

## NACK-Timing / Repair-Wait — the timing-race hypothesis, tested and REFUTED (branch `feat/nack-timing`, 2026-07-08)

The prior section left one loose end in the root-cause: proactive coded may be
dropped at the link rate **and/or** "arrive after the receiver's reactive NACK
timer." THIS rung isolates that second alternative — the **timing-race** theory:
on gap detection the receiver's deficit report (which IS the reactive NACK in
generation mode) fires immediately, *before* the in-flight proactive repair
covering the hole (riding with the surrounding data, ~1 generation-span later)
can decode it — so a hole proactive repair WOULD cover eats a redundant ARQ
round-trip, pinning the proactive fraction at ~0.4. **The fix built: a
repair-coverage horizon (`RWM_REPAIR_WAIT`, ms) that WITHHOLDS a frontier hole's
deficit until the covering proactive repair has had time to arrive+decode; only
on horizon expiry does the reactive NACK fire (FEC-before-ARQ discipline).**
Result: **the timing-race hypothesis is REFUTED — a 5th refutation of the
crossover.** Delaying the NACK does NOT raise the proactive fraction toward 0.9;
at high loss it slightly LOWERS it. `FDIAG` nails why: when the frontier stalls,
a covering proactive equation is **never present** (`present_at_stall=0`,
`repairs_useful=7 / repairs_fed=4609`). There is nothing in flight to wait for —
the proactive repair is dropped/wasted on the substrate, exactly as three prior
sessions concluded. All knobs env-gated, **default-off** (shipped/gate path
byte-identical); `gate_suite` **15/15**, lib **271** (incl.
`horizon_withholds_nack_until_repair_window_then_falls_back`), math green.

### The mechanism (built, VERIFIED active at L1)
- **`horizon_gate_deficits` (pure, unit-tested).** Each frontier generation's
  residual deficit is only ELIGIBLE to fire a reactive deficit report once it has
  been outstanding ≥ `horizon`. A newly-deficient anchor is ARMED and WITHHELD; an
  anchor that decodes within the horizon drops out (proactive win, no NACK); only
  an anchor whose horizon expires is reported (reliability fallback). `horizon=0`
  ⇒ byte-identical shipped path. δ-aware: clamped to ≤ ½·SRTT so low-RTT /
  latency-tight (Realtime) paths never over-wait and the wait can never exceed the
  round-trip it would save.
- **Dose-response confirms the knob is live.** At RTT 100 / 10%, sweeping
  `RWM_REPAIR_WAIT` monotonically moved `recovery_coded` and `proactive_fraction`
  — so the gate is genuinely withholding reports; the effect is just the wrong
  sign.

### PRIMARY metric — the proactive fraction did NOT climb (single-path, 100 mbit, G=384, r=0.35, narrow store)
| cell | wait ms | pfrac | recovery_coded | FEC Mbit/s | ARQ Mbit/s |
|---|---:|---:|---:|---:|---:|
| RTT100 / 10%  | 0  | 0.271 | 12709 | 0.659 | 0.816 |
| RTT100 / 10%  | 16 | 0.262 | 13348 | 0.640 | — |
| RTT100 / 10%  | 32 | 0.247 | 14421 | 0.661 | — |
| RTT100 / 10%  | 48 | 0.224 | 16443 | 0.615 | — |
| RTT100 / 2.6% | 0  | 0.619 | 2908  | 1.774 | 2.631 |
| RTT100 / 2.6% | 16 | 0.641 | 2665  | 1.747 | — |
| RTT100 / 2.6% | 32 | 0.406 | 6972  | 1.655 | — |
| RTT100 / 2.6% | 48 | 0.600 | 3172  | 1.667 | — |
| RTT10 / 2.6% (control) | 0  | 0.887 | 605 | 12.44 | 19.19 |
| RTT10 / 2.6% (control) | 16 | 0.905 | 502 | 13.18 | — |

- **High loss (10%): pfrac FALLS with the wait** (0.27→0.22), `recovery_coded`
  RISES (12.7k→16.4k). Waiting the whole generation-span (48 ms > one 37 ms
  generation at 100 mbit) recovered nothing proactively — it only delayed the
  inevitable reactive pull and let more deficit accumulate.
- **Low loss (2.6%): at best a marginal, noisy bump** (0.62→0.64 at 16 ms) then
  regression (the 32 ms point is an unstable outlier, both reps pinned to an
  identical time, `stdev 0.005 s`). Throughput monotonically DROPS with the wait.
- **The horizon only helps where pfrac is already high** — the low-RTT/low-loss
  control (already 0.89) ticks to 0.90. Exactly the regime where FEC does NOT need
  help. Where FEC must win (high RTT + high loss) there is no proactive repair to
  wait for.

### Why — `FDIAG` isolates it definitively (RTT 100 / 10%)
```
wait=0 : DECODE n=5 present_at_stall=0 | SOURCE n=0 | rf=4609 ru=7  | pfrac=0.086
wait=32: DECODE n=5 present_at_stall=0 | SOURCE n=0 | rf=4506 ru=8  | pfrac=0.080
```
`present_at_stall=0` in BOTH arms: when the in-order frontier stalls on a hole,
there is **never** a buffered proactive equation covering it. `repairs_useful ≈ 7`
of `repairs_fed ≈ 4600`: the proactive repair symbols that DO arrive are almost
entirely useless (wrong generation / linearly dependent). So the hole is not
"proactive-covered-but-NACKed-too-early" — the covering repair is simply absent.
Waiting up to 48 ms (> a generation-span) changes nothing because **there is
nothing in flight to decode it.** The binding constraint is the droppable-datagram
SUBSTRATE, not NACK timing — the same root cause, now with the timing alternative
positively excluded.

### Verdict, controls, reliability
- **Crossover REFUTED a 5th time; timing-race alternative positively excluded.**
  FEC never beats ARQ (FEC/ARQ 0.65–0.81 at 10%, 0.63–0.67 at 2.6%, 0.65 at RTT10)
  and the repair-wait does not move the crossover in any regime.
- **Multipath: NOT warranted** (gated on a single-path FEC win; there is none).
- **Controls / no regression.** `RWM_REPAIR_WAIT` env-gated, **default-off** →
  shipped + gate path byte-identical; `gate_suite` **15/15** release, lib **271**,
  math green. Low-RTT control (`c2r10`) does NOT regress — the ½·SRTT clamp holds
  the wait to ~5 ms at RTT 10 (dnf:0, small improvement). **Reliability intact:**
  `dnf:0` at every measured cell/wait, every rep completed (every byte delivered).
- **The residual, stated honestly:** the proactive fraction is set by how often a
  generation's proactive budget is dropped/insufficient on the wire (a substrate
  property), NOT by when the receiver decides to NACK. Raising the coding rate
  (prior r-sweep) and delaying the NACK (this rung) both fail for the same reason.
  The only lever left that the physics points to is the transport substrate itself
  (a non-droppable / retention-coupled coded channel), not any receiver-side
  scheduling knob.

**Harness.** `~/l1/rw_sweep.sh <reps> <bytes> <waits_ms> [scens…]` — per cell an
ARQ baseline + the narrow-store FEC arm swept over `RWM_REPAIR_WAIT`; reports
`pfrac/pcod/rcod/mbps/stdev`. `perf_rwm_c.sh` now also propagates
`RWM_REPAIR_WAIT`. Mechanism via `RWM_FDIAG` (`present_at_stall`, `rf/ru`).

## FEC Recovery Bug — the decoder DISCARDED late sources; repairs were 99.85% redundant (branch `feat/fec-recovery-bug`, 2026-07-08)

THE bug behind "proactive FEC recovery is dead" was in the RECEIVER decoder, not
the substrate and not NACK timing. Diagnosed per-generation, fixed, measured. The
smoking gun (`rf≈4609 ru≈7`, 99.85 % of arriving repair symbols useless) is now
**`rf=8383 ru=6053` — 72 % useful.** The waste was NOT fundamental; it was a
one-spot decoder defect.

### Per-generation trace — which of A/B/C/D
Cause **(C)/(D) blend: the generation decode matrix froze its known-source set at
slot-creation and never admitted late sources**, so arriving repairs were reduced
against a STALE unknown space and were linearly redundant relative to the real
holes. NOT (A) — there is NO raw-source ARQ in generation mode at all: `sent_store`
is populated only `if reliable && !generation`, so the SACK-gap retransmit path
`continue`s (no-op); recovery is coded-repair-only. NOT the substrate-drop theory:
`repairs_fed≈4600` proves the repairs ARRIVE; they are simply wasted on arrival.

Mechanism (`GenerationDecoder`, `fec/generation.rs`). Systematic sources ride the
wire as primary and land in `recovered`. A generation's `(anchor,width)` Gauss–
Jordan matrix is created only when its FIRST repair arrives, at which point it
pre-loads the sources **then present** as unit pivots. In production, source and
repair symbols INTERLEAVE and reorder, so a generation's own non-lost sources
routinely arrive AFTER its first repair. Those late sources were written to
`recovered` but **never injected into the live matrix** — the matrix kept treating
them as unknowns forever. Consequences, all measured/derived:
- `rank_in(anchor,K_g)` returns the MATRIX rank, so the receiver reports a deficit
  of `K_g − rank` inflated by the late-source count, not the true `holes`.
- the sender then floods `K_g − rank` coded repair where only `holes` were needed;
  the surplus repairs merely re-derive already-received sources → linearly
  dependent → `repairs_useful` (completions) pinned near zero.
- a generation could only finish by re-solving its FULL width in coded repair —
  which the proactive `ceil(K_g·r)` budget never supplies, so every hole fell to
  the round-trip-bound reactive deficit loop. Proactive recovery therefore looked
  dead, and every prior "FEC-vs-ARQ" number was ARQ-with-FEC-overhead.

Isolated by a unit test reproducing the interleave (`diag_late_source_after_first_
repair_still_recovers_from_coded`): a generation with **2 true holes + 4 non-lost
late sources** needed **6** coded repairs to decode (= holes + late), not 2 —
exactly `K_g − present_at_first_repair`. The frozen pre-load, proven in isolation.

### The fix
`GenerationDecoder::add_symbol` non-repair branch now calls
`inject_source_into_active_gens(seq,data)`: for every existing Solving matrix whose
fixed span covers `seq`, feed the unit equation `e_c·x = data` (c = seq−anchor)
into it. The unknown space shrinks to the real holes the instant the source
arrives; a late source that is the last missing DoF completes the generation and
its holes are delivered. `insert_equation` now returns `(added_rank, delivered)`
so `repairs_useful` counts rank-ADDS (the honest per-hole signal) not per-
generation completions. Default/shipped path untouched — `GenerationDecoder` is
instantiated only under `--window-systematic-repair`; the shipped non-generation
decoder is byte-identical (ARQ/shipped clean control **84.4 Mbit/s**, in range).

### PRIMARY metric — repairs_useful came alive (single-path, G=384, r=0.35, narrow store, `RWM_FDIAG`)
| cell | rf (fed) | ru (useful) | useful % | pfrac |
|---|---:|---:|---:|---:|
| c2r100l10 (RTT100/10%) — BEFORE | 4609 | 7 | **0.15 %** | 0.27 |
| c2r100l10 (RTT100/10%) — AFTER | 8383 | 6053 | **72 %** | 0.234 |
| c2r200l10 (RTT200/10%) — AFTER | 6108 | 4016 | **66 %** | 0.127 |
| c2r10 (RTT10/2.6%, control) — AFTER | 5240 | 1896 | **36 %** | 0.738 |

Proactive/coded repair now genuinely recovers holes: `repairs_useful` climbed from
0.15 % to 66–72 % of fed. **The decoder bug is fixed.**

### THROUGHPUT — FEC still does NOT beat ARQ; the residual, stated honestly
The decoder fix was NECESSARY but is NOT SUFFICIENT for the crossover.
| cell | ARQ Mbit/s | FEC Mbit/s | FEC/ARQ | pfrac | dnf |
|---|---:|---:|---:|---:|---:|
| c2r100l10 (RTT100/10%) | 0.761 | 0.667 (wait32) | 0.88 | 0.28 | 0 |
| c2r200l10 (RTT200/10%) | 0.448 | 0.262 | 0.58 | 0.13 | 0 |
| c2r10 (RTT10/2.6%, control) | ~19.2 | 13.19 | 0.69 | 0.74 | 0 |

**The next residual, exactly.** Recovery stays REACTIVE-dominated at high loss
(`recovery_coded` ≫ `proactive_coded`; pfrac 0.13–0.28). The proactive repair for a
generation is emitted at the send frontier and PACED OUT, so it arrives ~a
generation-span AFTER the generation's sources — and the receiver's deficit report
fires the instant the hole is seen, so the round-trip-bound reactive coded wins the
race before the (now-useful) proactive repair can decode the hole. Re-tested post-
fix, the two levers that failed pre-fix still don't cross it: the repair-wait
horizon gives only +0.05 pfrac / +0.05 Mbit/s at RTT100/10% (and regresses past a
generation-span), and RAISING r (0.35→0.60) LOWERS throughput (0.67→0.57) — the
extra coded overhead congests the droppable datagram path and drops more, for +0.02
pfrac. The binding constraint is now purely transport: to beat ARQ the proactive
budget must ride in the SAME flight as the sources (so a hole decodes with zero
round-trip), which the paced-after-the-fact coded channel structurally cannot do at
high RTT. The decoder no longer wastes the repair — but the repair still arrives
too late to pre-empt the reactive round-trip.

### Controls / reliability
- `cargo test -p raptorpath --lib` **271** green (incl. the new late-source
  regression); `cargo test -p raptorpath-math` green; `gate_suite` **15/15**
  release. `diag_late_source_after_first_repair_still_recovers_from_coded` now
  passes (2 coded == 2 holes regardless of source arrival order).
- **Reliability intact:** `dnf:0` at every measured cell; every transfer completed
  byte-exact. Low-RTT control (c2r10) not regressed (13.2 Mbit/s, ~prior). Clean
  shipped control **84.4 Mbit/s** (in the 76–84 band); systematic-FEC-arm clean
  61.3 Mbit/s (its own transport-fix overhead, unchanged by the decoder fix).
- **Multipath: NOT warranted** — gated on a single-path FEC win, which the
  crossover still lacks.

### Verdict
**THE decoder bug is found and fixed — proactive FEC recovery is revived
(`repairs_useful` 0.15 %→72 %).** But it does NOT by itself flip FEC past ARQ:
throughput crossover is blocked by the reactive-vs-proactive RACE under high-RTT
pacing (proactive repair arrives a generation-span late and loses to the deficit
round-trip). Honest headline: the 99.85 %-wasted-repair pathology was real and is
eliminated; the FEC-vs-ARQ crossover remains a TRANSPORT-substrate problem (deliver
the proactive budget in-flight with the sources), not a decoder one — now proven
with the decoder defect removed rather than masking it.

**Harness.** Same `pf_sweep.sh` / `rw_sweep.sh` / `perf_rwm_c.sh`; `RWM_FDIAG` `rf/ru`
now report rank-ADD usefulness. Fix in `raptorpath/src/fec/generation.rs`.

## Repair In-Flight — the ARQ over-request was the real waste; FEC reaches PARITY (branch `feat/repair-inflight`, 2026-07-08)

Chased the last residual ("proactive repair paced a generation-span LATE → reactive
round-trip wins the race → pfrac stuck, no crossover"). Two things came out, one an
instrumentation artifact and one a genuine — and DIFFERENT — win than the brief
predicted. The decoder is (still) fixed; the crossover blocker was NOT the repair
arriving late, it was the receiver OVER-REQUESTING ARQ.

### FIRST: the `present_at_stall=0` residual was partly a BROKEN PROBE
`GenerationDecoder` never implemented `frontier_probe` — it inherited the trait
default `(0,0)`. So in generation/systematic mode `present_at_stall` and
`probe_buffered` were **structurally 0 in every prior run**, regardless of whether
proactive repair was actually buffered. The "present_at_stall≈0 → repair always
arrives late" diagnosis that motivated this task was measuring a probe that could
only ever return 0. Implemented a real `frontier_probe` for the dense decoder
(`holes` = span − recovered; `buffered` = pivot rows at hole columns across the
Solving matrices). With the instrument fixed, `present_at_stall` reads **1→16**,
responsive to G — proactive repair IS sometimes present; the residual was overstated.

### THE REAL WASTE: the reactive deficit OVER-REQUESTED ARQ (the coordinator's lever)
At high loss the systematic FEC arm was NOT losing to late repair — it was FLOODING
reactive ARQ. The deficit `K−rank_in` is honest (rank counts buffered repair), but
the report fires on EVERY sub-RTT decode-progress, each resetting the in-flight
baseline, so the sender re-sends ~the full deficit faster than a round-trip can
reflect it. MEASURED at c2r100l10 (G=384, r=0.15, single path): `recovery_coded`
**30 703** for a ~6 k-symbol object (≈5 ARQ/source), pfrac 0.035, **0.32 Mbit/s**.
Bounding it — `RWM_REACT_CAP` (act on a generation's deficit at most once per SRTT)
+ `RWM_REPAIR_WAIT` (coalesce: let in-flight repair shrink the deficit before ARQ
fires) — collapses the flood:

| c2r100l10, single | recovery_coded | pfrac | present_at_stall | Mbit/s |
|---|---:|---:|---:|---:|
| unbounded (flood) | 30 703 | 0.035 | 1 | 0.32 |
| **bounded** (react_cap + wait40) | **437** | **0.72** | 1 | **0.913** |
| pure ARQ (same cell) | — | — | — | 0.919 |

**FEC/ARQ 0.32→0.99 — from 0.88× (prior goal-gate) to PARITY.** The dominant prior
"FEC-vs-ARQ" deficit was self-inflicted ARQ over-request, not late repair. Decoder
change (`fec/generation.rs`): `propagate()` re-injects any hole recovered in one
coding grid into every other active matrix, so the deficit stays honest when two
grids coexist (without it, inline flooded `recovery_coded` to **94 141**).

### SMALLER G raises proactive-at-stall, but only to PARITY (not a crossover)
The fungible, non-stalling way to get repair in-flight is a smaller generation (it
seals — and its proactive repair flows — sooner), via the proven batched path:

| cell (bounded, single) | G | pfrac | present_at_stall | FEC Mbit/s | ARQ Mbit/s | FEC/ARQ |
|---|---:|---:|---:|---:|---:|---:|
| c2r100l10 | 384 | 0.72 | 1 | 0.913 | 0.919 | 0.99 |
| c2r100l10 | 128 | 0.64 | 11 | 0.893 | 0.919 | 0.97 |
| c2r200l10 | 128 | 0.65 | 10 | 0.417 | 0.400 | **1.04** |
| c2r200l10 (r=0.30) | 128 | 0.53 | 16 | 0.377 | 0.400 | 0.94 |

Smaller G lifts `present_at_stall` 1→16 (proactive decode now genuinely happens),
and at RTT200 FEC **edges ahead (1.04×)**. But it is only an edge: ARQ at RTT200/10%
is a hard `~window/RTT` **0.40 Mbit/s** (SAME at 1.8 MB and 5 MB — steady-state, not
ramp), and FEC still pays a round-trip for the ~60 % of holes with NO proactive
repair present at detection, so it does not decisively escape the serialization.
Raising r buys more `present_at_stall` (16) but the extra coded overhead on the
droppable path costs more than the round-trips it saves (0.417→0.377).

### The interspersed separate-grid inline repair (`RWM_INLINE_REPAIR`) — REFUTED
Implemented exactly as the brief specified (emit one proactive repair per ~1/r
sources over a trailing block of width W of already-sent source). Decodes correctly
in ISOLATION (unit tests: block repair present at a hole → proactive decode;
`frontier_probe` reports it buffered). At L1 it is REFUTED for two structural
reasons: **(1) stall-starved** — it emits from the source-send path, so under
backpressure/frontier-stall (exactly when needed) no source ⇒ no repair, while the
batched path emits every loop iteration; **(2) cross-grid stranding** — for W<G the
block (W) and generation (G) repairs form SEPARATE Gaussian systems, so a buffered
block equation cannot combine with reactive generation repair (MEASURED
`probe_buffered` climbing while the frontier wedges, gap 900–1100). Unifying W=G
removes the stranding but reduces to "small G" (above), which the non-stalling
batched path already does. Kept env-gated, default-OFF, as a documented negative
result (`net/mod.rs`, `generate_repair_range` in `fec/generation.rs`).

### Controls / reliability
- `cargo test -p raptorpath --lib` **273** green (+2: `interspersed_block_repair_
  present_at_hole_decodes_proactively`, `frontier_probe_reports_buffered_proactive_
  equation`); `raptorpath-math` green; `gate_suite` **15/15** release.
- Low-RTT control c2r10 (RTT10/2.6 %): FEC bounded G=128 **21.9 Mbit/s**, pfrac 0.91
  — no regression (above the prior 13–19). Clean shipped control **83.7 Mbit/s**
  (76–84 band, shipped path byte-untouched); systematic-FEC clean 73.7. **dnf:0 at
  every measured cell**, every transfer byte-exact.

### Verdict
The residual was mis-attributed. FEC was not losing to late repair — it was losing
to its own **reactive ARQ over-request** (the probe that "proved" late repair was
itself stuck at 0). Bounding the request to the honest once-per-SRTT deficit takes
FEC from 0.88× to **PARITY** (1.0×), with a slight **1.04×** edge at RTT200/10 %.
A decisive FEC>ARQ crossover still requires `present_at_stall` to DOMINATE (proactive
present for ~all holes), which neither smaller-G nor the interspersed repair reaches
here. HONEST HEADLINE: the ARQ-waste win is real and shippable (enable
`RWM_REACT_CAP`+`RWM_REPAIR_WAIT` for the systematic arm); the interspersed-repair
timing fix is refuted as a separate mechanism; the crossover remains an open
`present_at_stall`-dominance problem, now measurable for the first time.

**Harness.** `perf_rwm_c.sh` (+ `RWM_INLINE_REPAIR`/`RWM_INLINE_W` propagation);
`RWM_FDIAG` `present_at_stall`/`probe_buffered` now REAL in generation mode.
Fixes in `raptorpath/src/fec/generation.rs` (`frontier_probe`, `propagate`,
`generate_repair_range`) + `raptorpath/src/net/mod.rs` (bounded-reactive levers,
gated inline emission).

## Present-at-Stall — the proactive pacer makes repair PRESENT but NOT a throughput win (branch `feat/present-at-stall`, 2026-07-08)

Attacked the residual the "Repair In-Flight" section left open: `present_at_stall`
is not dominant (proactive repair present for only ~a fifth of frontier holes), so
FEC pays a round-trip for the rest and stays at parity, not a win. The named fix was
a DEDICATED proactive-repair pacer that emits repair EARLY — while a generation is
still FILLING — on the GENERATION grid, independent of source availability and of
the ack-clock, so the covering equation is buffered at the receiver before the
in-order frontier reaches the hole. It fixes both refutations of the earlier
interspersed inline repair: it is not stall-starved (runs every loop iteration,
incl. `tx_paused` wakeups) and not cross-grid stranded (codes the generation grid).

### The mechanism WORKS (implemented + validated end to end)
The pacer (`RWM_PROACTIVE_PACER`, systematic only, default-OFF) codes a filling
generation over its retained contiguous PREFIX but emits at the FULL generation
MATRIX width `G`, carrying a 2-byte `coded_width` (with a `FILL_FLAG` in the wire
coded-index) so the decoder zeroes columns `[coded_width, G)`. Every symbol for a
generation — filling, sealed, or reactive-deficit — therefore keys to the SAME
`(anchor, G)` matrix and combines fungibly (no cross-width stranding). The dense
decoder gained INCREMENTAL unit-row delivery (a source is delivered the instant its
pivot row becomes isolated, not only at full generation rank) so a filling-generation
repair recovers an early hole present-at-stall. Unit-tested
(`proactive_pacer_recovers_filling_generation_hole_under_backpressure`): the pacer
emits for an UNSEALED generation under backpressure and recovers its early hole,
then combines with a later sealed deficit repair in the same matrix. 274 lib +
`raptorpath-math` + `gate_suite` 15/15 all green. The receiver skips `gen_widths`
learning for `FILL_FLAG` repairs so a still-filling generation is never mistaken for
a full one and NACK-flooded.

### At L1 the pacer raises presence + pfrac but LOSES throughput (and wedges at high RTT)
Single path, in-order (`OOO=0`), systematic G=256, r=0.15, `RWM_GEN_INFLIGHT=1024`,
`RWM_CC_PACE`+`RWM_REACT_CAP`+`RWM_REPAIR_WAIT`, 4 MB × 2. `present_at_stall` is the
count of frontier holes with proactive repair buffered at detection / DECODE-resolved
holes; `pfrac` = proactive_coded / (proactive+recovery), sender-side.

| cell (loss) | ARQ Mbps | FEC Mbps (pfrac, present) | FEC+pacer Mbps (pfrac, present, dnf) | FEC/ARQ → pacer/ARQ |
|---|---:|---:|---:|---:|
| c2r100 (2.6%)  | 3.44 | 2.43 (0.91, 0/15) | 1.92 (0.93, 1/19)      | 0.71 → 0.56 |
| c2r100l5 (5%)  | 1.52 | 1.22 (0.81, 0/23) | 1.19 (0.88, 4/32)      | 0.80 → 0.78 |
| c2r100l10 (10%)| 0.92 | 0.83 (0.72, 1/26) | 0.71 (0.70, 10/39)     | 0.91 → 0.77 |
| c2r200 (2.6%)  | 1.74 | 1.51 (0.89, 1/17) | 2.10 (0.98, 1/21) **dnf=1** | 0.87 → n/a |
| c2r200l5 (5%)  | 0.68 | 0.61 (0.81, 0/22) | 0.59 (0.87, 4/27)      | 0.89 → 0.86 |
| c2r200l10 (10%)| 0.45 | 0.40 (0.70, 0/24) | 0.32 (0.74, 7/31) **dnf=1** | 0.88 → 0.71 |

Two robust facts, one of each sign:

1. **Presence rose exactly as designed.** `present_at_stall` climbs in every cell
   (present-fraction e.g. c2r100l10 0.04→0.26, c2r200l10 0.00→0.23), and the pacer
   shifts recovery from reactive to proactive (`pfrac` up in 5/6 cells; reactive
   `recovery_coded` drops, e.g. c2r100l10 403→380, c2r200 126→18, c2r200l10 435→335).
   The instrument confirms the covering equation IS buffered earlier.

2. **It does not convert to throughput; it regresses it (−3 % to −21 %), and it
   WEDGES at high RTT (dnf=1 at c2r200 and c2r200l10).** On a SINGLE path the
   pacer's advantage is self-defeating: to be present EARLY the repair must be sent
   EARLY, which — under one shared CC-paced link budget — steals send capacity from
   the SOURCE symbols, so the in-order frontier LAGS (measured frontier gap 0→507)
   and net goodput falls. The reactive it removes (a round-trip saved) is worth less
   than the source bandwidth it displaces, because the baseline already recovers most
   holes from late-but-proactive repair with no round-trip (`source_n=0` throughout —
   systematic mode never per-seq-ARQs). The `dnf=1` at RTT200 is a genuine
   reliability regression of the gated path (a generation occasionally never
   completes), a hard blocker on its own. `c2r200` pacer 2.10 > ARQ 1.74 is NOT a
   win — it is one completing run of two, the other DNF.

Low-RTT control c2r10 (RTT10/2.6 %): pacer 14.9 vs FEC 15.7 vs ARQ 19.7, dnf=0 — no
low-RTT wedge, ~5 % cost. Baseline FEC (pacer OFF) reproduced the prior goal-gate
numbers exactly (c2r100l10 pfrac 0.719 == the "Repair In-Flight" 0.72), confirming
the new incremental-delivery decode path did NOT regress the default generation
decoder. Shipped path byte-untouched (pacer/`FILL_FLAG` emission is `RWM_PROACTIVE_
PACER`-gated, default-off); `gate_suite` 15/15 release.

### Verdict — the crossover is a SINGLE-PATH bandwidth problem, not a presence problem
The "Repair In-Flight" diagnosis said the crossover needs `present_at_stall`
dominance. This branch shows that is necessary but NOT sufficient — and, more
importantly, that on a single path presence and throughput are in DIRECT TENSION:
buying presence (early repair) costs source bandwidth (later frontier), so pushing
present-at-stall up pushes throughput DOWN. FEC stays at-or-below ARQ. The exact next
residual is therefore **cross-path fungibility**: the pacer's early proactive repair
should ride the SECOND path while source rides the first, so presence is bought
withOUT displacing source — the C8 aggregation goal. That test is gated (per the
brief) on FEC first beating ARQ single-path, which it does not here, so heterogeneous
multipath was NOT run. HONEST HEADLINE: the present-at-stall mechanism is built,
correct, and measurably raises presence, but it does not produce the throughput
crossover on a single path and introduces a high-RTT wedge; kept env-gated,
default-OFF, as a documented negative result alongside `RWM_INLINE_REPAIR`.

**Impl.** `raptorpath/src/fec/generation.rs`: `FILL_FLAG`, `generate_repair_filling`
/`code_generation_full`/`next_fill_gen`/`codeable_filling`, incremental unit-row
delivery in `insert_equation`, `FILL_FLAG` decode parse. `window_traits.rs`:
`generate_repair_filling`/`wants_filling_coding` (defaults). `net/mod.rs`:
`RWM_PROACTIVE_PACER` pacer loop + `gen_widths` FILL_FLAG guard. Harness:
`pmeas.sh`/`pmeas2.sh`, `RWM_PROACTIVE_PACER` propagation in `perf_rwm_c.sh`.

## C8 Cross-Path Repair — the pacer's early repair on the SPARE path, MEASURED: still does not aggregate (branch `feat/c8-crosspath-repair`, 2026-07-08)

The "Present-at-Stall" section closed on the single-path presence⊥throughput
tension (buying early repair costs source bandwidth) and named the exact untested
residual: **cross-path fungibility** — the pacer's early proactive repair should
ride the SECOND (spare) path while source rides the first, so presence is bought
WITHOUT displacing source. This branch built and MEASURED that realization at C8.

### PART 0 — ORACLE-CONFIRM (the config the identity implies)
`temporal_oracle.rs::systematic_repair_aggregation` (independent Monte-Carlo, exact
C8 netem params) confirms THIS placement in theory: systematic source striped
work-conserving + FUNGIBLE cross-path deficit-driven repair (any path's repair
clears any hole in the window), out-of-order frontier, NO per-seq ARQ, unbounded
store → **C8 het ×1.188** (goodput ceiling ×1.195), C7 sym ×1.992, `phi_tail`→0,
`max_deficit`=2 symbols, `arq_used`=0. So the oracle says the specific placement
DOES beat fast-path-alone toward ~1.19. Confirmed before building.

### The wiring (env-gated, default-OFF; shipped path byte-untouched)
`Scheduler::place_repair_spare_path` (new): routes REPAIR to the max-spare-capacity
path (`max spare_capacity()` = the underutilized path — the slow path once the fast
path is source-saturated) instead of the marginal-cost softmax (which biases repair
toward the FAST path, so it competes with systematic source). Symmetric paths (equal
spare) → uniform split of the near-tie set (no hard-argmax concentration → no C7
regression; unit-tested `place_repair_spare_routes_to_underutilized_path`). Wired
into all three repair-emission sites (sealed batched proactive, `RWM_PROACTIVE_PACER`
filling pacer, deficit recovery) behind `RWM_XPATH_REPAIR` (generation/systematic
only). Loopback: 275 lib green (incl. the new placement test + the pacer
filling-hole recovery test), `raptorpath-math` green, `gate_suite` 15/15 release.

### PART 3 — DECISIVE C8 at L1 (VM, netem independent qdiscs, seed 42)
Fast-alone reference **single c2 G=192 50 MB ×5 = 15.18 Mbit/s** (median 26.6 s,
stdev 0.88, dnf 0). Bar: dual C8 (c2+c3) STRICTLY > 15.7 AND factor > 1.0. All arms
G=192, 50 MB, dnf 0:

| C8 dual (c2+c3) arm | Mbit/s | factor vs 15.18 | stdev s | note |
|---|---:|---:|---:|---|
| **baseline** plain systematic (no flags) | **14.70** | **0.97×** | 1.70 | BEST dual — slow path carries SOURCE |
| XPATH + pacer + REACT_CAP + CC_PACE (no ooo) | 13.59 | 0.90× | 0.90 | stable, but below baseline |
| XPATH + REACT_CAP only (deficit cross-path) | 11.26 | 0.74× | 4.37 | bounded reactive + tight store hurts |
| XPATH + pacer + OOO_RETAIN=16 + REACT_CAP | 7.52 | 0.50× | **28.9** | wide retention → bufferbloat, 103 s outliers |

(8 MB ×3 screen showed the opposite ranking — baseline 13.28, XPATH+pacer 14.07 —
but that is a startup-dominated artifact of small objects; at the DECISIVE 50 MB the
ranking inverts. The pacer's proactive fraction IS high on dual — 0.72–0.88, dnf 0,
NO single-path wedge because the repair rides the slow path not the fast — so the
mechanism works as designed; it just does not convert to throughput.)

### DECISIVE VERDICT — NOT crossed; cross-path repair does not aggregate, it HURTS
The C8 bar (>15.7, factor>1) is **NOT met by any arm**. The best dual is the
**plain-systematic baseline at 14.70 (0.97×)** — and every cross-path-repair arm is
STRICTLY WORSE. Cross-path proactive repair on the spare path does not lift C8; it
lowers it.

**Mechanism (per-path split, the honest why).** In the baseline the slow c3 path
carries SOURCE (∝ goodput placement), so its ~10 Mbit goodput adds real aggregate
throughput that roughly cancels the in-order-frontier drag its losses add → 0.97×,
the near-parity ceiling the three prior C8 sections already hit. `RWM_XPATH_REPAIR`
DIVERTS that same slow-path capacity from SOURCE to REPAIR. But the fast c2 path
ALREADY recovers its own losses cheaply from its own `r`=0.15 proactive repair (it
does 15.18 alone), so the cross-path repair it receives is largely redundant — while
the SOURCE the slow path stops carrying is a real throughput loss. The trade is
net-negative: **loss-presence bought from the slow path costs the source that
capacity would otherwise carry** — the presence⊥throughput identity, now confirmed
to hold in the cross-path case too, not just single-path. This is exactly the
prediction the identity makes; the oracle's ×1.188 assumed the slow path's SOURCE
is delivered out-of-order with zero frontier cost (Σg aggregation), but at L1 the
in-order cumulative-ack frontier serialization (Loss-Recovery defect 2) caps the
slow path's usable source contribution at ~parity, so repair cannot buy back more
than it costs.

**Grounded final verdict: heterogeneous throughput aggregation is bounded even with
working FEC + cross-path proactive repair.** The bottleneck is NOT repair placement
and NOT FEC recovery (which works — proactive fraction 0.72–0.88, repairs_useful
~66 %): it is the in-order-frontier recovery latency that a second, slower, lossy
path cannot parallelise. Closing it needs the transport-pipeline change the
Loss-Recovery section named (pipelined per-RTT frontier recovery or a genuinely
rateless ack-frontier so a hole is never a fixed in-order position), NOT a placement
law. C8 stays at ~0.97× fast-alone.

*[2026-07-19: era-bound. This "bounded" held at the Cubic-substrate ~15 Mbit
operating point; the binding stack underneath (walls #1/#2/#7) was found and
dissolved later — see "CONSOLIDATED VERDICT (2026-07-19)". The
presence⊥throughput reading of cross-path repair itself still stands.]*

### Controls (no regression)
- **C7 SHIPPED plain-reliable (c2+c2, default path):** dual **20.82** / single 16.42
  = **×1.27 symmetric aggregation INTACT**, dnf 0 — the shipped path is byte-
  identical (`RWM_XPATH_REPAIR` gated OFF), so no regression.
- single c2 fast-alone 15.18 (reference); all C8/C7 arms **dnf 0**, every byte
  delivered (reliability intact).
- (C7 *systematic-mode* did not summarize — a pre-existing "datagram too large" on
  the symmetric second path that the BASELINE systematic C7 hits identically,
  independent of this branch's repair-placement flag; the DECISIVE-C8 section already
  recorded "systematic-repair C7 has NEVER aggregated". Not caused here.)
- `cargo test -p raptorpath --lib` **275 green**; `raptorpath-math` green;
  `gate_suite` 15/15 release.

**Impl.** `scheduler/mod.rs`: `place_repair_spare_path` + unit test. `net/mod.rs`:
`RWM_XPATH_REPAIR` flag, applied at the three repair-placement sites. Harness:
`RWM_XPATH_REPAIR` propagation in `perf_rwm_c.sh`. Oracle: `temporal_oracle.rs::
systematic_repair_aggregation` (pre-existing, re-confirmed ×1.188).

## SACK+BDP Reassembly — the composed sender-decoupling attack on the frontier, with the #52 failure modes fixed (branch `feat/sack-bdp-reassembly`, 2026-07-08)

**The attack.** The proven root cause (Loss-Recovery defect 2, six prior sections)
is the in-order cumulative-ack frontier serialization: the sender's flow control
(store drain + TUN backpressure) gates on the CONTIGUOUS frontier `window_ack_seq`,
which FREEZES on every hole → goodput caps at ≈ window/RTT (~16 Mbit C2 single
lossy; ~parity C8). The prior SACK attempt (#52) tried to decouple the sender but
(a) BROKE reliability — the receiver reassembly evicted a pruned-but-unconsumed
symbol — and (b) ran on DEAD FEC. This branch composes the two fixes #52 lacked and
re-attacks: **`RWM_SACK_PRUNE`** (sender: prune the sent-store on ANY out-of-order
ack, gate on TRUE outstanding, keep sending past a hole) **+ new `RWM_REASM_BDP`**
(receiver: clamp the decoder/received-seq prune so it can NEVER advance above the
delivered frontier → a SACK-pruned symbol is never evicted before use; the reorder
buffer is already non-evicting; an occupancy probe `[REASM]` reports the bound).
Both env-gated, default-off; the shipped path is byte-identical.

### PRIMARY — single-path C2 lossy: the reliability invariant HOLDS, but there is NO throughput lift
Native `perf_rwm_c.sh`, 50 MB × 3, seed 42, c2 (~2.5 % GE loss). (1.8 MB objects
complete in <1 s — warmup-dominated noise, per prior sections; 50 MB gives the
steady-state signal.)

| single-path C2 arm | Mbit/s | dnf | reassembly peak (held-behind-frontier) |
|---|---:|---:|---|
| baseline (gate off) | 16.54 | 0 | — |
| **SACK+REASM (in-order)** | **17.09** | 0 | **max_pending 1888 / ~50 000 sym — BOUNDED** |
| OOO + SACK + REASM | 17.22 | 0 | max_pending 1541 — BOUNDED |

**No lift (16.54 → 17.09 → 17.22, all within the ~5 % run-to-run stdev).** This
CONFIRMS the prior diagnosis at 50 MB and with the reliability fix in place: the
sender was never the bottleneck (throughput is store-cap-invariant), so decoupling
it buys nothing — completion still waits for the in-order frontier to walk each
hole at ≈ 1 ARQ round / RTT. The bound is receiver-side RECOVERY LATENCY.

**The reliability invariant HOLDS (the thing #52 broke).** dnf 0 on every arm;
the reassembly occupancy stays BOUNDED at ≈ BDP (peak 1541–1888 symbols out of a
~50 000-symbol object) as the frontier advances 75 k→125 k — it never grows toward
the whole object. Every byte delivered, no eviction. The composed guard makes the
sender-decoupling safe for reliable in-order delivery. Unit test
`test_sack_bdp_reassembly_delivers_every_byte_past_a_hole` codifies the loop end-to-
end (sender advances past a hole, receiver holds OOO non-evicting, hole recovers by
retransmit, every symbol delivered in order).

### C8 heterogeneous dual — the BDP bound FAILS; decoupling UNBOUNDS the buffer and stalls
C8 (c2+c3), 50 MB × 3, seed 42:
- **baseline** plain (gate off): **10.86 Mbit/s** (0.66× fast-alone 16.54), dnf 0,
  high variance (stdev 6.9 s) — the slow c3 path carries SOURCE, the near-parity
  ceiling.
- **SACK+REASM: the reassembly grows to `max_pending` 38 820 / ~50 000 ≈ 78 % of
  the whole object; a single rep did NOT complete in 300 s** (severe bufferbloat
  stall). The SACK-decoupled sender races the FAST path ahead while the SLOW path's
  frontier hole lingers ≈ its larger RTT; the dual store cap = gain·Σ BtlBw×RTprop
  sums BOTH paths' anchors (slow-path RTT-inflated), so outstanding is NOT bounded
  to the fast path's BDP → the receiver holds nearly the whole object. **This is
  EXACTLY where the invariant fails: the sender-outstanding cap is SUMMED across
  paths, not per-path, so on heterogeneous RTT the "BDP-sized" reassembly is not
  BDP-sized.** SACK+REASM makes C8 strictly WORSE than baseline; **>15.7 with
  factor > 1 NOT met.**

### HONEST VERDICT
The composed fix does what #52 could not — it makes sender-side SACK decoupling
SAFE for reliable in-order delivery (dnf 0, buffer bounded ≈ BDP, every byte
delivered on single-path) — but it does **NOT unlock lossy throughput.** The
in-order cumulative-ack frontier's serialization is a RECOVERY-LATENCY bound
(holes walk at ≈ 1 ARQ round / RTT), structural to reliable in-order-capable
delivery on this transport and unmoved by any sender flow-control law — the same
conclusion the six prior L1 investigations reached, now with the sender-decoupling
made reliable and still measured flat. On heterogeneous multipath the decoupling
actively REGRESSES C8: the slow path's RTT-inflated BDP anchor defeats the summed
store cap, so the receiver reassembly grows unbounded and bufferbloat stalls the
transfer. Closing the collapse still needs the transport-pipeline change §14.7
named (pipelined per-RTT frontier recovery, or a rateless ack-frontier where a
hole is never a fixed in-order position) PLUS a per-path (not summed) outstanding
cap. **The frontier bound is fundamental; the reliability invariant is preserved on
single-path but throughput is not lifted — honest negative, failure mode located.**

**Controls (no regression).** clean single SACK+REASM **86.24** vs default **86.09**
(no regression); C7 dual c2+c2 baseline **20.88 = ×1.26** (symmetric aggregation
intact); all single-path / control arms dnf 0 (reliability intact). `cargo test -p
raptorpath --lib` **276 green** (+`test_sack_bdp_reassembly_delivers_every_byte_past_a_hole`);
`raptorpath-math` green; `gate_suite` **15/15** release.
**Impl.** `net/mod.rs`: `RWM_REASM_BDP` receiver flag (prune clamp to the delivered
frontier + `[REASM]` occupancy probe), composed with the existing `RWM_SACK_PRUNE`
sender decoupling. Harness: `RWM_SACK_PRUNE`/`RWM_REASM_BDP` propagation in
`perf_rwm_c.sh`.
**Harness.** `sudo RWM_SACK_PRUNE=1 RWM_REASM_BDP=1 bash perf_rwm_c.sh c2 c2 bulk
50000000 3 single`; `[REASM]` occupancy in the server log (`/tmp/rwm-s.log`, the
bulk receiver — `perf --client` uploads).

## Full Benchmark Re-Run (2026-07-08) — current numbers post-arc-fixes

> **⚠ ERA NOTE (2026-07-19): "current" here means 2026-07-08** — pre-divide
> (qemu64), stock-Cubic substrate, pre-wedge-fix, 1024-pool. Metric A is the
> origin of the 12–48× streaming tail crown and STANDS as the crown's
> reference measurement (defended at the 2026-07-19 flip gate). The
> throughput/completion/multipath rows are era-bound; see "CONSOLIDATED
> VERDICT (2026-07-19)" for the current numbers.

Fresh L1 re-measure of raptorpath's OWN numbers with the current binary built
from `main` @ `6d2a05b` (all arc fixes in: O(n²) Copa `record_rtt` monotonic-
deque fix, FEC decoder-revival, bufferbloat cap, bounded reactive ARQ). Branch
`bench/full-rerun-20260708`. VM: fedora 6-core, `cargo 1.96.1`, release build
`3m04s`. The external baselines (quinn / kernel-BBR / kernel-MPTCP / CUBIC) in
the L3 REGIME MAP are UNCHANGED and kept as-is; only rp's rows are refreshed
here. **Do not edit the stale L3 tables above — this section supersedes rp's
rows in them.** Seed 42 throughout; per-measurement hard timeouts (perf 600 s,
rwm 700 s). Total measurement wall-time ≈ 55 min.

### Metric B — object COMPLETION, single path (rp-native `perf`, 1.8 MB × 6)

The direct refresh of the STALE metric, apples-to-apples with the old table
(same `perf_native.sh`, rp-native block pipeline — RaptorQ + P8 block-ARQ,
which is the shipped default for `--protocol-hint bulk`). Both hints run.

| cell | rp bulk (current) mean / median / mbps | dnf | rp realtime | best baseline (kept) | verdict (refreshed) |
|------|----------------------------------------|-----|-------------|----------------------|---------------------|
| C1 DC     | 0.127 / 0.117 s · 113.5 Mbit/s | 0/6 | 0.150 s · 95.8 Mbit/s (dnf 0) | quinn 0.027 / BBR 0.028 | rp trails (~0.13 s vs 0.027; 1.8 MB is slow-start-bound over 1 Gbit) |
| C2 WiFi   | 0.862 / 0.969 s · 16.7 Mbit/s  | 0/6 | **DNF** (600 s, 6/6) | quinn 0.20 / BBR 0.22 | rp bulk ≈ stale (0.86 vs 0.83 s) — still ~4× behind quinn; block path NOT lifted by the CPU fix (loss-bound, not CPU-bound). realtime cannot complete a reliable object under loss. |
| C3 LTE    | 9.91 / 11.12 s · 1.45 Mbit/s   | 0/6 | not run (DNFs, C2 mechanism) | quinn 0.90 / BBR 1.0 | rp ≈ stale / slightly worse (9.9 vs 7.3 s) — still ~11× behind. Recovery-bound at 40 ms RTT / 2 % loss. |
| C4 Sat    | **DNF** (600 s, 6/6)           | 6/6 | not run | quinn 1.09 / BBR 3.6 | **REGRESSION/DNF** — block-mode bulk stalls at 200 ms RTT / 3 % loss (no run completed in 600 s). Flagged. |
| C5 BadWiFi| 13.74 / 11.07 s · 1.05 Mbit/s (min 2.40, max 30.25, sd 9.69) | 0/6 | not run | quinn/BBR 0.55; CUBIC DNF | rp completes DNF-free (beats CUBIC) but ~25× behind quinn. Completes despite 5.3 % loss because RTT is low (10 ms); C4 shows RTT, not loss, is what breaks block completion. |

**Headline correction to the optimistic pre-run expectation:** the CPU fix did
NOT make single-path *completion* "much better" on the lossy cells. It lifted
the CPU-bound CLEAN / high-BW regime (28 → 85 Mbit, confirmed below), but
C2/C3/C4/C5 are RECOVERY-latency-bound, and the rp-native BLOCK pipeline
(`perf_native`) is essentially UNCHANGED there (C2 0.86≈0.83 s, C3 9.9≳7.3 s)
and C4 now DNFs at 200 ms RTT. The FEC→ARQ-parity gains the consolidation doc
cites live in the WINDOW-RELIABLE + generation-coding path (`perf_rwm_c` with
`RWM_GEN`), NOT in this block-mode completion metric.

### Single-path THROUGHPUT — clean vs lossy (rp-native bulk, 20 MB × 5)

| link | mean_mbps | median s | stdev s | dnf | note |
|------|-----------|----------|---------|-----|------|
| clean (100 Mbit, 0 % loss) | **85.5** | 1.860 | 0.023 | 0/5 | the "86" — CPU fix live; exceeds quoted native-quinn-at-C2 (72) |
| c2 (100 Mbit, GE 1.3/50 ≈ 2.5 %) | **15.9** | 9.640 | 0.729 | 0/5 | the "15" — loss-bound |

The **86 vs 15** story reproduces exactly (85.5 vs 15.9 Mbit/s). Clean-link
throughput is not loss/recovery bound, so the CPU fix's 3× shows there;
the lossy path collapses to recovery-latency-bound ~16 Mbit regardless.

### Metric C — MULTIPATH goodput, dual path (window-reliable, 50 MB × 6)

Both arms requested: **plain-reliable** (`--window-reliable`, pure ARQ over the
window) and **systematic** (`+ --window-systematic-repair`). `topo_dual`,
C7 = c2+c2, C8 = c2+c3. NOTE: these use the WINDOW pipeline, whose sender does
NOT stripe source symbols across paths (documented Phase 0 finding) — so they
measure LOWER aggregation than the regime map's kept C7 20.8–23.9 / C8 14.70,
which came from the block-affinity scheduler / tuned generation coding.

| cell | arm | rp dual mean_mbps | median s | stdev s | dnf | dual-over-single (÷15.9) |
|------|-----|-------------------|----------|---------|-----|--------------------------|
| C7 (c2+c2) | plain-reliable | 17.40 | 23.45 | 1.83 | 0/6 | 1.09× |
| C7 (c2+c2) | systematic     | 15.40 | 26.25 | 0.98 | 0/6 | 0.97× (systematic worse than plain) |
| C8 (c2+c3) | plain-reliable | 5.43  | 78.33 | 11.72 | 0/6 | 0.34× (REGRESSES below single c2) |
| C8 (c2+c3) | **systematic** | **15.30** | 26.41 | 1.18 | 0/6 | **0.96×** (systematic recovers C8 to ~parity) |

**Verdict (refreshed):** the two arms split cleanly by topology.
- **C8 (heterogeneous c2+c3): systematic FEC is essential and reaches
  parity.** Plain reactive-ARQ over the window collapses to 5.43 Mbit/s
  (0.34×) because the slow c3 path drags the in-order cumulative-ack frontier;
  adding `--window-systematic-repair` lifts it to **15.30 Mbit/s (0.96×
  single-c2)** — matching the regime map's kept C8 baseline (14.70, 0.97×).
  So the arc's "C8 bounded at ~parity" verdict REPRODUCES on the current
  binary, and systematic repair is what buys the parity.
- **C7 (symmetric c2+c2): window pipeline does NOT reach the aggregation
  win.** Plain 17.40 (1.09×), systematic 15.40 (0.97×, worse — repair
  displaces source on a path that had no spare deficit). Neither reaches the
  regime map's kept C7 20.8–23.9 (1.26–1.55×), which came from the
  BLOCK-affinity scheduler that stripes source across paths; the window
  sender does not stripe (documented Phase 0). The C7 symmetric-aggregation
  win therefore stands ONLY on the block path, not the window path.

### Metric A — message TAIL latency, single path (tail_matrix, 50 msg/s × 20 s, p99 over 5 reps)

Warm tunnel, `{realtime,bulk} × {400,1200}B`, p99 DISTRIBUTION over 5 reps
(single-run p99 is variance-dominated). Values are p99 in ms: min / median / max.

| cell | hint | 400 B (min/med/max) | 1200 B (min/med/max) | best baseline (kept) | verdict |
|------|------|---------------------|----------------------|----------------------|---------|
| C2 WiFi | realtime | 42 / **59** / 637 | 39 / **145** / 2655 | quinn 2824 / BBR 13,400 | **rp WINS ~12–48×** (median 59–145 ms vs quinn 2.8 s / BBR 13 s) |
| C2 WiFi | bulk     | 84 / 102 / 120 | 71 / 154 / 481 | — | tight, DNF-free |
| C3 LTE  | realtime | 105 / **209** / 1409 | 334 / **1771** / 3154 | BBR 198 / quinn 1393 | rp 400 B ≈ BBR (209 vs 198), beats quinn ~6×; 1200 B worse than BBR |
| C3 LTE  | bulk     | 143 / 176 / 11022 | melts (NO_DATA, ≥3/5 reps no summary) | — | 400 B OK; 1200 B melts at C3 (20 Mbit / 2 % loss) |

**Metric A verdict STANDS (refreshed on current binary):** raptorpath's
message-tail p99 at C2 (realtime, 59–145 ms median) crushes quinn (2.8 s) and
kernel-TCP (13 s) by ~12–48×, exactly the regime map's 12–60× win. At C3 the
low-rate BBR still owns the tail (198 ms) — rp ties at 400 B, loses at 1200 B.
Large frames (1200 B) at C3 melt, consistent with the "C3 melts" narrative.
The tail-latency win is the headline that is UNAFFECTED by (and independent of)
the throughput/completion story: it comes from in-band FEC recovery avoiding
head-of-line blocking, not from CC throughput.

### vs the stale pre-CPU-fix regime map: what changed

**Nothing improved on single-path COMPLETION at the lossy cells, and C4 now
DNFs — the optimistic "much better post-CPU-fix" completion expectation does
NOT hold for the rp-native block pipeline.** The CPU fix is real and confirmed
(clean throughput 85.5 Mbit/s, 3× the old 28), but it only helps the
CPU-bound clean/high-BW regime; the loss/recovery-bound cells (C2 0.86≈0.83 s,
C3 9.9≳7.3 s, C4 DNF, C5 13.7 s) are essentially the STALE numbers or worse.
The genuine post-arc wins — clean-link throughput 3×, FEC→ARQ parity, C8
systematic parity (15.30, 0.96×), and the 12–48× message-tail win — all
REPRODUCE; but the "loses 4–8× on lossy completion" was NOT overturned for
block-mode bulk (it lives in the window+generation path, a different code
path than `perf_native`). Honest bottom line: single-path lossy BULK
completion via the shipped block pipeline is unchanged-to-worse; the arc's
gains are throughput-on-clean, tail-latency, and the window/systematic path.


---

## DAPS + Right-Sized FEC (2026-07-12) — delay-aware scheduling escapes the frontier long pole; C8 0.48×→0.80×; over-FEC refuted (branch `feat/daps-rightsized-fec`)

*Decision record: → [ADR-0065](adr/0065-daps-era-refutations.md)*

**[AUDIT 2026-07-13: UNCERTAIN, leaning INVALID — generation-inert
measurement.]** The DAPS arms recorded no generation-enabling flag; on this
code `RWM_DAPS=1` alone leaves generation OFF (`daps = RWM_DAPS &&
generation`), so the headline C8 0.48×→0.80× (13.12), the monotone r-sweep /
r*≈0.03, and "paused=0%" are voided as unverified — under the inert reading the
whole r-sweep spread is noise (recorded σ_s reaches 8.8–53.7 s). The FMTCP arms
(7.58, 7.14) remain VALID (`RWM_FMTCP` self-enables generation). Survives:
unit tests, the citations, and oracle PART 6 as a model (its claimed L1
confirmation does not). The "revision to §16.8/16.9" is unsupported. Valid
numbers: "Generation-ON Re-Baseline (2026-07-13)"; classification:
"Methodology Audit (2026-07-13)".

Tests the user's two ideas against the honest ceiling: **(A)** arrival-aligned
(DAPS-style) scheduling — the slow path carries FUTURE stream data offset by the
latency skew so it arrives in sync with the fast path reaching that position, and
a slow-path loss is a loss of FUTURE data with pre-fetch slack to recover before
the frontier catches up; **(B)** RIGHT-SIZED bulk FEC — replace the FMTCP fixed
r=0.10 (≈4× the 2.6% loss) with the derived §8.4 r* for the bulk/loose-δ profile.
Grounded in the PUBLISHED delay-aware MPTCP scheduling literature (see citations).

### Published algorithms followed (not a naive re-derivation)

- **DAPS** — G. Sarwar, R. Boreli, E. Lochin, A. Mifdaoui, G. Smith, "Mitigating
  Receiver's Buffer Blocking by Delay Aware Packet Scheduling in Multipath Data
  Transfer," WAINA/PAMS 2013, pp.1119–1124; and N. Kuhn, E. Lochin, A. Mifdaoui,
  G. Sarwar, O. Mehani, R. Boreli, "DAPS: Intelligent Delay-Aware Packet
  Scheduling for Multipath Transport," IEEE ICC 2014. DAPS precomputes a schedule
  over the LCM of the per-path forward delays so segments ARRIVE IN ORDER; the
  two-subflow form assigns the slow path sequence numbers spaced by the RTT ratio
  (10× skew ⇒ segs 1..10 fast, seg 11 slow). We adapt the RTT-ratio offset to our
  coded transport: slow-path delay-skew offset **Δ_j = (RTprop_j − RTprop_min)·Σ
  BtlBw_i** symbols (from the Copa anchors).
- **ECF** — Y. Lim, E. Nahum, D. Towsley, R. Gibbens, "ECF: An MPTCP Path
  Scheduler to Manage Heterogeneous Paths," ACM CoNEXT 2017. Completion-time
  guard `(1 + k/CWND_f)·RTT_f < RTT_s + σ` — only use the slow path for data the
  fast path could not deliver sooner. We apply it as a placement gate (a path j
  is eligible for a source at lead L iff L ≥ Δ_j), the published fix for DAPS's
  known static-schedule failure mode (a near-frontier slow-path symbol stalling
  the frontier; ECF/BLEST report plain DAPS otherwise regresses — it performed
  WORST of the family in ECF's own evaluation).
- **BLEST** (S. Ferlin, Ö. Alay, O. Mehani, R. Boreli, IFIP Networking 2016) and
  MPTCP-default **minRTT** are the send-window-blocking and lowest-RTT baselines
  the cost-based FMTCP build corresponds to (place CURRENT data on the min-RTT
  path). Our coded transport adds fungible cross-path repair + out-of-order
  decode, which repairs DAPS's other failure mode (a lost FUTURE slow-path symbol
  recovers within the pre-fetch slack).

### Oracle-confirm FIRST (temporal_oracle.rs PART 6, `cargo test -p raptorpath-math`)

The FMTCP PART-5 oracle predicted ×1.19 at C8; L1 measured 0.48×. PART 6 adds
the two things it skipped — the per-path LATENCY SKEW + bounded in-order
reassembly buffer, and the bursty STRAND — and compares cost-based-current
(minRTT/FMTCP marginal-cost placement) vs DAPS delay-aligned + ECF.

- **PART 6a (buffer sweep, C8 het, K=25MB, ceiling ×1.195, right-sized r*=0.05):**
  cost-based-current **anti-aggregates** at bounded buffers — reproducing the
  production 0.48× regime — while DAPS never drops below 1.0× and reaches the
  ceiling at HALF the buffer occupancy and 7× less stall:

  ```
    buffer |  cost fac  cost occ  cost st% |  DAPS fac  DAPS occ  DAPS st%
       192 |    0.679x       192    42.2%  |    1.000x       121     0.2%
       256 |    0.779x       256    34.6%  |    1.000x       121     0.2%
       384 |    0.882x       384    25.7%  |    1.000x       121     0.2%
       512 |    1.045x       501    11.8%  |    1.000x       121     0.2%
       768 |    1.182x       566     1.7%  |    1.194x       252     0.2%
      1024 |    1.182x       566     1.7%  |    1.194x       252     0.2%
  ```
  DAPS reaches the ×1.195 ceiling; the slow-path frontier-freeze collapses from
  1.71% (cost-based) to 0.24% — this IS the long-pole escape. Cost-based needs
  the deep buffer AND still stalls; DAPS needs only Δ (≈535 syms) and never
  freezes df on the slow path.
- **PART 6b (C7 symmetric):** skew 0 ⇒ Δ=0 ⇒ DAPS ≡ cost-based; both ×1.99 (no
  regression).
- **PART 6c (bursty strand):** a deep-enough buffer (pre-fetch lead ≥ skew)
  bridges a q_bg=0.20 strand — feasible; the required depth is reported.
- **PART 6d (r-sweep):** throughput-optimal r=0.05 at the C8 2.6% loss; r=0.10
  spends ≈2× the wire for no gain — the over-FEC hypothesis, confirmed in-model.

Verdict from the oracle: DAPS ESCAPES the slow-path long pole that capped the
cost-based FMTCP build (frontier-freeze 1.71%→0.24%; anti-aggregation 0.68×→≥1.0×),
and the right-sized r is throughput-optimal below 0.10. GREENLIT to build.

### DECISIVE L1 (VM 10.1.5.16, dual netns, seed 42, 25MB × 5, rp-native perf)

Baselines (ceiling denominators, single-path measured, same binary):
`single-c2` (fast, 100Mbit/5ms/1.3%p) = **16.41 Mbit/s**; `single-c3` (slow,
20Mbit/20ms/2%p) = **3.14 Mbit/s**. Recovery-bound ceiling C7 = 2·16.41 = 32.82;
C8 = 16.41+3.14 = **19.55 Mbit/s**. Every arm dnf=0 (reliable, every byte).

**C8 (c2+c3) — the heterogeneous aggregation bar:**

| C8 arm | Mbit/s | ×single-fast | eff (÷19.55 ceiling) | stdev(s) |
|---|---:|---:|---:|---:|
| FMTCP r=0.10 (historical) | 7.58 | 0.48× | 0.39 | — |
| shipped-default (no FMTCP/DAPS) | 8.70 | 0.53× | 0.45 | 8.79 |
| FMTCP-only r=0.03 (right-sized r ALONE) | 7.14 | 0.44× | 0.37 | 10.45 |
| DAPS r=0.10 (placement ALONE) | 8.65 | 0.53× | 0.44 | 2.13 |
| **DAPS r=0.05** | 10.47 | 0.64× | 0.54 | 1.03 |
| **DAPS r=0.03 (both levers)** | **13.12** | **0.80×** | **0.67** | 1.21 |
| DAPS r=0.02 | 3.83 | 0.23× | 0.20 | 53.7 (under-FEC cliff) |

**The headline:** DAPS delay-aware scheduling + right-sized r lift C8 from 0.48×
(FMTCP) to **0.80× single-fast = 13.12 Mbit/s = 0.67 of the recovery ceiling**,
and stabilize it (stdev 8.8→1.2). The two levers are SYNERGISTIC and each
NECESSARY: right-sized r alone (FMTCP r=0.03) does NOT help (7.14, still 0.44×,
unstable); DAPS placement alone (r=0.10) helps modestly (8.65); together they
nearly double C8 (7.58→13.12, +73%). DIAG confirms the mechanism: the FMTCP build
was TUN-paused 13–68% (frontier stall); **DAPS is paused=0%** — the long pole is
gone. The r-sweep is monotone (0.03>0.05>0.10) — the fixed r=0.10 wasted ≈34% of
C8 throughput; r=0.02 under-provisions (near-DNF, stdev 54s), so r*≈0.03 (= loss +
small margin) is the throughput optimum, exactly the §8.4 bulk r*.

**HONEST RESIDUAL:** 0.80× is still below parity (1.0×). DAPS removed the
frontier-stall long pole (paused 0%), but a SECOND cap appears — the slow path
bufferbloats to ~834ms RTT under the deep read-ahead, so the future-offset data's
pre-fetch slack is partly consumed by queue latency. The remaining gap to parity
/ ceiling is slow-path queue management (BLEST-style: cap the slow path at its
BDP so it does not bloat), NOT the frontier serialization DAPS fixed. Reported,
not forced.

**C7 (c2+c2) — symmetric control, no regression:** DAPS r=0.03 = **20.87** (1.27×
single-c2), r=0.10 = 20.88, vs shipped-default (no DAPS) = **20.29** (1.24×). DAPS
matches/slightly beats the default on C7 (skew 0 ⇒ Δ=0 ⇒ the DAPS gate is inert),
r-sweep flat (low symmetric loss has no over-FEC headroom to reclaim). The shipped
default path is byte-identical (all DAPS code is RWM_DAPS-gated).

### Controls / no regression

- `cargo test -p raptorpath --lib` 280/280; `-p raptorpath-math` all green incl.
  temporal_oracle PART 6 (4 DAPS tests). DAPS scheduler unit tests: slow path
  carries future-offset data (lead<Δ_slow ⇒ fast-only; lead≥Δ_slow ⇒ slow admitted),
  symmetric ⇒ no restriction. `daps_loopback` reliable completion (dnf 0, every byte).
- Right-sized r wired from §8.4 (`compute_r_star_with_z`, bulk δ=0.2ε), NOT
  hardcoded; RWM_GEN_R overrides for the sweep. Shipped default untouched (no
  RWM_DAPS). Reliability intact: every arm dnf=0.

**VERDICT:** BOTH user ideas partially validated against the honest ceiling.
Arrival-aligned scheduling DOES escape the slow-path long pole that capped the
cost-based FMTCP build (C8 0.48×→0.80×, frontier pause 13–68%→0%, +73%, stabilized);
right-sized bulk FEC IS throughput-optimal below the fixed 0.10 (r*≈0.03, the fixed
r=0.10 wasted ≈34% at C8) — but the two levers are synergistic, not independent.
C8 does not yet cross parity/ceiling: the residual is a slow-path bufferbloat
second-order cap, honestly a queue-management (BLEST) follow-on, not a scheduling
failure. Merge as a measured build; regime-map: heterogeneous aggregation is
scheduling-bound (delay-alignment lifts it materially) AND queue-bound (the
residual), not recovery-latency-bound as the pre-DAPS arc concluded.

## DAPS Queue Management (2026-07-12) — per-path BDP cap + BtlBw pacing; the queue bound is right but rate-signal-limited in generation mode (branch `feat/daps-queue-mgmt`)

*Decision record: → [ADR-0065](adr/0065-daps-era-refutations.md)*

**[AUDIT 2026-07-13: UNCERTAIN, leaning INVALID — generation-inert
measurement.]** `RWM_DAPS_BDP`/`RWM_DAPS_PACE` are dead without generation,
which the harness never enabled — under the inert reading all four arms ran
identical code, and the +15% (~10.0→~11.5, σ_s 2.0–5.7 on 5-run arms) is
within noise either way; the throughput deltas are voided. The quoted
"occasionally bdp71" DIAG is impossible on an inert sender (wrong log or an
unrecorded generation-on run). Survives: unit tests, oracle PART 6e as a model;
the "rate-signal-limited" residual diagnosis happens to be code-true in plain
mode too (WindowAcks never feed `record_delivery`) but was not validly
established here. Valid numbers: "Generation-ON Re-Baseline (2026-07-13)".

Attacks the DAPS residual above: DAPS removed the frontier stall but the slow
path BUFFERBLOATED to ~0.8–1.8 s RTT, consuming the pre-fetch slack and capping
C8 below parity. Implements the two published queue bounds and measures them.

### The diagnosis — why the slow path bloats DESPITE the per-path cap

`RWM_DAPS` sets `fmtcp=true`, so the FMTCP per-path BDP cap IS active — but it
only gates the **aggregate TUN-read PAUSE** (`fmtcp_percap_full`: pause only when
EVERY path is at its cap). The fast path is almost never at its cap, so TUN
reads never pause, and `place_source_daps`'s softmax keeps routing a capacity
share to the slow path with **no hard per-path bound at placement time** (the
cost term rises only softly with in_flight). There is also **no per-path
pacing** — one aggregate `src_tokens` bucket — so the deep DAPS read-ahead
(window backstop `(pipeline+6)·G`) is dumped onto the slow path faster than
`BtlBw_slow` drains. The cap governs *when to stop reading TUN*, never *how much
to commit to one subflow*.

### The fix (both DAPS-gated; shipped non-DAPS default byte-identical)

- **BLEST per-path placement cap** — `place_source_daps_capped(lead, gain)` drops
  a path at its own BDP from the eligible set, bounding slow-path OUTSTANDING at
  `gain·BtlBw_slow·RTprop_slow`. `RWM_DAPS_BDP=gain` (default **1.0** = exactly
  one BDP; 0 disables).
- **BBR per-path pacing** — each path emits at its own BtlBw (`btlbw_sym_per_s`);
  when the slow path's BtlBw bucket is dry, the source spills to the fast path
  this instant. `RWM_DAPS_PACE=0` disables (default on under DAPS).
- The DAPS offset `Δ_j` already uses the **min-filtered RTprop** (`daps_offset_syms`
  → `min_rtt()`), not the inflated RTT — verified, so a bloated RTT cannot
  mis-size the offset.

### Oracle-confirm FIRST (temporal_oracle PART 6e)

PARTs 6a–d paced each path per-tick, so they never bufferbloated and already hit
the ceiling — which is exactly why they missed this residual. PART 6e adds the
queue physics: the slow path is a FIFO server at BtlBw_slow, standing queue
`Q = max(0, outstanding − BDP_slow)`, delay `q = Q/BtlBw_slow`; future data
placed Δ ahead arrives `q` late and is useful only while `q ≤ skew`.

```
   scheduler |  outstanding  queue(ms) slowRTT(ms)    factor
        DUMP |          640        344         384    1.000x   (queue eats the slack → parity)
     BDP-cap |           67          0          40    1.195x   (ceiling)
        PACE |           67          0          40    1.195x   (ceiling)
```

The oracle confirms — **GIVEN a correct per-path BDP** — the cap/pace collapse the
queue and lift C8 from parity (x1.000) to the ceiling (x1.195); the gain sweep
shows gain 1.0 is optimal and ≥2.0 re-inflates. So the queue bound IS the right
lever *in the model*.

### DECISIVE L1 (VM 10.1.5.16, dual netns, seed 42, 25 MB × 5, rp-native perf)

Baselines (same binary): single-c2 (fast) = **16.71**, single-c3 (slow) = **3.33**
Mbit/s ⇒ recovery ceiling C8 = **20.04**, C7 = **33.42**; raw-link goodput ceiling
C8 = 100·(1−ε)+20·(1−ε) ≈ 116 (not protocol-achievable). Every arm **dnf=0**.

**C8 (c2+c3), same binary, apples-to-apples:**

| C8 arm (r=0.03) | Mbit/s | stdev(s) | ×single-fast | eff ÷20.04 |
|---|---:|---:|---:|---:|
| no-QM (BDP=0 PACE=0) | 9.84 / 10.16 (~10.0) | 5.7 / 5.4 | 0.60× | 0.50 |
| cap-only (BDP=1 PACE=0) | 11.20 | 4.46 | 0.67× | 0.56 |
| pace-only (BDP=0 PACE=1) | 11.21 | 4.06 | 0.67× | 0.56 |
| **QM both (default)** | **12.56 / 10.46 (~11.5)** | **2.0 / 3.7** | **0.69×** | **0.57** |
| QM + CC_PACE | 10.90 | 3.58 | 0.65× | 0.54 |

r-sweep at QM: **r=0.03 → 12.56**, r=0.05 → 8.12, r=0.10 → 7.61 (monotone; r*≈0.03
optimum holds, over-FEC penalty sharper under the bound). **C7 (c2+c2) QM =
20.68** (stdev 0.58, 1.24× single-c2) — **no regression** (skew 0 ⇒ Δ=0 ⇒ DAPS
inert; symmetric cap/pace act equally).

**HONEST RESULT:** queue-mgmt lifts C8 modestly (~10.0 → ~11.5, ~+15%) and cuts
the WITHIN-run stdev (~5.5 → ~2.9); each lever alone gives ~11.2, together ~11.5
(synergistic); reliability intact. But it does **NOT** reach parity/ceiling and
does **NOT** bound the slow-path RTT.

**Slow-path RTT before/after (RWM_DIAG, per-path probe):** no-QM p1 climbs
95→356→688→1023→**1364 ms** (RTprop pollutes 46→178→**961 ms**); QM p1 still
climbs to **~1774 ms** (RTprop → 1820 ms). The QM DIAG shows `p1:infl=0/bdp0`
throughout: **our in_flight gauge reads 0 and the per-path Copa anchor is only
intermittently established** (`bdp0`, occasionally `bdp71`≈true BDP).

### The REVISED residual (this revises the prior "BLEST follow-on" verdict)

The queue bound is the right idea but is **rate-signal-limited in generation
mode**, defeated by two production realities the oracle abstracted away:

1. **No per-path BtlBw anchor in generation mode.** WindowAcks do not drive
   `record_delivery` (in_flight releases by time-EXPIRY, not per-path ack), so
   `copa_bdp_anchor()` on the slow path reads `None`/tiny — the cap and pacing,
   which key on it, have no stable per-path BDP to act on. The intermittent
   moments the anchor establishes (`bdp71`) are exactly where the benefit comes
   from — hence modest and inconsistent.
2. **The queue is in the QUIC datagram send buffer, BELOW the in_flight gauge**
   (which reads 0), so bounding in_flight cannot bound it; and because the queue
   never drains within the 10 s min-RTT window, the RTprop min-filter itself
   pollutes to ~1.8 s (which would eventually mis-size Δ_j).

So the true long pole is **per-path BtlBw estimation + QUIC-send-buffer
visibility in generation mode**, NOT placement queue depth. The `BLEST/BBR`
mechanism is correct (oracle-confirmed) and merged env-gated as the substrate;
closing it needs a per-path delivered-rate estimator driven by the cumulative
ack + per-path ownership (a subsystem follow-on: generation mode keeps no
seq→path map). Regime-map update: heterogeneous aggregation is
scheduling-bound (DAPS) AND **rate-estimation-bound** in generation mode — the
queue bound needs a per-path rate it currently lacks.

### Controls / no regression

`cargo test -p raptorpath --lib` 281/281 (new `daps_bdp_cap_bounds_slow_path_outstanding`);
`-p raptorpath-math` 19/19 incl. temporal_oracle PART 6e (`daps_queue_mgmt_lifts_c8_to_ceiling`);
`daps_loopback` reliable (dnf 0). Shipped default untouched (all queue-mgmt code
RWM_DAPS-gated; RWM_DAPS_BDP=0 RWM_DAPS_PACE=0 reproduces pre-QM behaviour).

## Per-Path Estimator (2026-07-12) — the diagnosed rate-estimation residual, CLOSED: per-path BtlBw now establishes, RTprop stays at base, slow-path bufferbloat 3.7 s→~0.3 s; C8 lifts + STABILIZES (branch `feat/per-path-estimator`)

*Decision record: → [ADR-0065](adr/0065-daps-era-refutations.md)*

**[AUDIT 2026-07-13: UNCERTAIN — generation-inert measurement; mechanism DIAG
internally inconsistent.]** The throughput claims (+30% pooled, 7.85→10.24,
"stabilizes") are voided as unverified: the lift is smaller than documented
same-config session swings (up to 2.3×), and the "baseline (pre-estimator)"
arm was necessarily a different binary (no same-binary off-toggle exists under
DAPS), so this was never the same-binary A/B the table implies. If generation
was off, "est=Y 93%" is impossible on the sender in EITHER arm — the DIAG
comes from an unrecorded generation-on run or a misread log and cannot be
attached to the throughput battery. Survives: unit tests; the estimator code
itself (the later Slow-Path Anchor Diagnosis shows the anchor DOES establish
generation-ON — but is decode-clocked and unstable). Valid numbers:
"Generation-ON Re-Baseline (2026-07-13)".

Builds the two pieces the DAPS Queue Management work diagnosed as the true long
pole: generation mode never ESTIMATED a per-path delivered rate (WindowAcks did
not drive `record_delivery`; `in_flight` released by TIME-EXPIRY, not per-path
ack), so the BLEST cap + BBR pacer keyed on a per-path BDP that was `None`/`bdp0`
throughout — the queue bound was correct but INERT. And the queue hid in the QUIC
datagram send buffer, below the `in_flight` gauge (which read 0).

### The two pieces (both generation-mode-only; shipped non-generation default byte-identical)

- **PIECE 1 — per-path delivered-rate estimator.** The sender already owns a
  seq→path map (`source_path_map`: which path each SOURCE seq's DAPS placement
  committed it to). New per-path ACK ATTRIBUTION drives it: on every SACK range
  (OOO delivery — the frontier is frozen exactly when the estimator is most
  starved) AND on each cumulative-frontier advance, every newly-acked source seq
  is attributed to its owning path and calls `PathState::on_src_delivered`, which
  (a) feeds `copa.record_delivery` so **BtlBw_i = that path's own delivered
  source-rate** (BtlBw_i·RTprop_i = the per-path BDP the Copa anchor reports), and
  (b) releases a new per-path SOURCE outstanding gauge `src_inflight` (the BLEST
  `in_flight_i`) by ACK, not time-expiry. `source_path_map` became a `BTreeMap`
  so the attribution range-queries a SACK/ack span in O(unattributed), not
  O(span). The BLEST cap now reads `src_inflight` (source units, matching the
  source-unit BtlBw) instead of the coded, time-expired `in_flight`. Gated
  `generation && (RWM_DAPS || RWM_PER_PATH_EST)`.
- **PIECE 2 — QUIC-send-buffer bounding.** quinn exposes no datagram-send-buffer
  occupancy, so the bound is by PACING: with Piece 1's real per-path BtlBw the
  existing BBR pacer meters SOURCE placement on each path at BtlBw_i, so the
  sender never hands quinn more than the path drains and the send queue stays near
  one ≤4 ms burst. RTprop is the min-filtered base (verified in the DIAG: it stays
  at the 44 ms slow / 12 ms fast propagation floor, immune to the standing queue),
  so a bloated RTT can never mis-size Δ_j or the BDP.
- Per-path DIAG added: `sinfl` (BLEST in_flight_i), `btlbw` (sym/s), `est=Y/n`
  (anchor established?), alongside the existing infl/bdp/rtt/rtprop.

### Oracle re-confirm (temporal_oracle PART 6e) — the assumption is now MET

PART 6e proved that GIVEN a correct per-path BDP the cap/pace collapse the queue
and reach ×1.195. The estimator realizes EXACTLY that BDP — `BtlBw_i·RTprop_i`
with the min-filtered RTprop — so PART 6e's assumption is now met, not changed;
**PART 6e stands** (`-p raptorpath-math` 19/19 unchanged). What L1 adds below is
that realizing the per-path BDP is necessary and materially helps, but a SECOND
queue (coded/repair + fast-path) that PART 6e abstracts away keeps C8 short of the
modelled ceiling.

### DECISIVE L1 (VM 10.1.5.16, dual netns, 25 MB × 8, rp-native perf, TWO seeds)

Ceilings (post-change binary, single-path, seed 42): `single-c2` (fast) =
**16.54**, `single-c3` (slow) = **3.26** Mbit/s ⇒ recovery ceiling C8 =
**19.80**, C7 = **33.08**; raw-link goodput ceiling C8 = 100(1−ε)+20(1−ε) ≈ **117**
(not protocol-achievable). Every arm **dnf=0** (reliable, every byte).

**C8 (c2+c3), DAPS+QM r=0.03, same-binary apples-to-apples, per seed:**

| arm | seed42 Mbit/s (stdev_s) | seed7 Mbit/s (stdev_s) | pooled | ×fast | eff ÷19.80 |
|---|---:|---:|---:|---:|---:|
| baseline (pre-estimator: `bdp0`/`est=n`) | 5.88 (14.6) | 9.81 (4.8) | ~7.85 | 0.47× | 0.40 |
| **+ per-path estimator (this work)** | **9.58 (5.8)** | **10.90 (3.5)** | **~10.24** | **0.62×** | **0.52** |

**THE HONEST RESULT (two-seed, stabilize-before-comparing).** The baseline is
BIMODAL across seeds — one seed bloats catastrophically (5.88, median 4.9 Mbit/s,
slow-path RTT/RTprop both **3734 ms**), another is fine (9.81) — exactly the
run-to-run variance the #69/#70 gap flagged. The per-path estimator (1) ELIMINATES
the catastrophic-bloat regime (seed42 5.88→9.58, +63%), (2) lifts the already-OK
seed modestly (seed7 9.81→10.90, +11%), and (3) STABILIZES: post-change seed
means are 9.58/10.90 (range 1.3) vs baseline 5.88/9.81 (range 3.9). Pooled C8
rises ~7.85→~10.24 Mbit/s (+30%), from **0.40→0.52 of the recovery ceiling**
(×0.47→×0.62 single-fast).

**Mechanism confirmed at L1 (sender DIAG).** The diagnosed residual is CLOSED:
- **Per-path BtlBw/BDP now ESTABLISHES** — baseline `est=Y` in **0%** of DIAG
  lines (`bdp0` throughout); post-change **618/663 = 93%** `est=Y`. Real per-path
  rates: fast BtlBw ≈ 20 000 sym/s (BDP ≈ 240), slow ≈ 1 150–5 000 sym/s (BDP ≈
  50–217).
- **Slow-path RTprop stays at the 44 ms base** (min-filtered, immune to the
  queue), vs baseline where it polluted to 3734 ms.
- **Slow-path live RTT collapses 3734 ms → ~200–380 ms** (bufferbloat largely
  drained by the per-path source pacing).

**REMAINING RESIDUAL (honest — improves but does NOT reach the ceiling).** C8 is
0.52 of the recovery ceiling, not ~1.0. A standing queue remains: slow-path live
RTT ~200–380 ms (vs 44 ms base) and the FAST path also carries ~140 ms (live
~150 ms vs 12 ms base). The per-path pacer meters only SOURCE placement; the
coded/repair emission and the aggregate send buffer are not per-path
pace-bounded, so a second queue persists. `sinfl≈0` throughout means the OOO
attribution drains the SOURCE gauge promptly, so the PACER is the binding lever
and the BLEST cap (proven correct in unit tests) rarely engages under it. The
next residual is per-path pacing of the coded/repair traffic + the fast-path
queue — NOT the rate estimation this work fixed.

**C7 (c2+c2) — symmetric control, no regression:** DAPS+QM = **21.41** (stdev
0.73, 1.29× single-c2), vs the DAPS-QM build's 20.68 — no regression (skew 0 ⇒
Δ=0 ⇒ DAPS inert; the estimator is symmetric).

**GENERAL-FIX CHECK (per-path signal beyond DAPS).** A PLAIN generation multipath
run with `RWM_PER_PATH_EST=1` and NO DAPS also establishes per-path BtlBw
consistently — **263/295 = 89%** `est=Y`, fast BtlBw ≈ 24 500 sym/s — so the CC
and the placement law now get a stable per-path rate signal in plain multipath,
not only under DAPS (C8 there = 8.08 Mbit/s; DAPS placement still adds on top).

### Controls / no regression

`cargo test -p raptorpath --lib` 283/283 (2 new: `per_path_ack_attribution_updates_only_the_owning_path`,
`per_path_estimator_bounds_slow_path_outstanding_at_one_bdp`; the reworked
`daps_bdp_cap_bounds_slow_path_outstanding` now drives `src_inflight`);
`-p raptorpath-math` 19/19 incl. temporal_oracle PART 6e (unchanged); gate_suite
15/15 release. Shipped non-generation default byte-identical (attribution gated
`generation && (RWM_DAPS || RWM_PER_PATH_EST)`; `RWM_PER_PATH_EST` unset + no DAPS
reproduces prior behaviour). All L1 arms dnf=0.

**VERDICT.** The diagnosed rate-estimation residual is CLOSED: generation mode now
estimates a real per-path BtlBw/RTprop/BDP (93% established vs 0%), the min-filter
keeps RTprop at the propagation base, and the per-path source pacer drains the
slow-path bufferbloat from 3.7 s to ~0.3 s. This eliminates the catastrophic-bloat
regime and stabilizes C8, lifting pooled C8 from 0.40→0.52 of the recovery ceiling
(×0.47→×0.62 single-fast). It does NOT reach the ceiling — a second queue
(coded/repair + fast-path, not yet per-path pace-bounded) is the honest next
residual. Regime-map: heterogeneous aggregation is scheduling-bound (DAPS) AND
rate-estimation-bound (this work, now closed) AND still queue-bound on the
non-source traffic — improved and stabilized materially, not yet at parity.

## Pace-All Traffic (2026-07-12) — pace the CODED/REPAIR emission at BtlBw_i too; the standing queue that the SOURCE-only pacer left; C8 lifts + STABILIZES on BOTH seeds, does NOT reach the ceiling (branch `feat/pace-all-traffic`)

*Decision record: → [ADR-0065](adr/0065-daps-era-refutations.md)*

**[AUDIT 2026-07-13: UNCERTAIN, leaning INVALID — generation-inert
measurement.]** `RWM_PACE_ALL` is dead without generation, which the harness
never enabled — under the inert reading both "same-binary A/B" arms ran
identical code, and the +52% pooled lift (7.31→11.11), the σ_s collapse, and
the C7 12.08→21.02 split would be session drift (documented same-config swings
reach 2.3×; arms were not interleaved). The two-seed consistency is the
strongest pro-validity signal of the era but does not establish the mechanism;
the throughput deltas are voided as unverified. Survives: unit tests, oracle
PART 6e as a model. Valid numbers: "Generation-ON Re-Baseline (2026-07-13)".

The per-path estimator (above) made BtlBw_i real and paced SOURCE placement at it,
but the CODED/REPAIR emission — the per-generation proactive budget, the filling
proactive pacer, the deficit top-up, and inline repair — was emitted OUTSIDE that
per-path pacer (path picked by `place_symbol`/`place_repair_spare_path`, metered
only by the GLOBAL delivered-goodput `gen_tokens` bucket). So TOTAL per-path
emission = source (paced at BtlBw_i) + repair (unpaced per-path) EXCEEDED BtlBw_i
and a standing queue persisted on BOTH paths (`sinfl≈0` throughout confirmed the
SOURCE gauge drains promptly — the queue is fed by unpaced REPAIR, not source
backlog).

### The fix (generation+DAPS-only; shipped non-DAPS default byte-identical)

Route EVERY repair symbol through the SAME per-path BtlBw token bucket
(`daps_pace_tok`) as source, via one pure gate `paced_repair_decision(tok, cand,
fast)` applied at all four repair emission sites:
- **candidate funded** (bucket ≥ 1) → emit there, consume one token;
- **candidate dry, fast funded** → spill to the fast (min-RTprop) path so the slow
  path never over-queues;
- **BOTH dry** → HOLD (discard the rateless symbol WITHOUT decrementing the
  deficit want; retry next loop as the buckets refill at BtlBw_i). The HOLD is what
  bounds the FAST path too: source has priority, repair uses only the LEFTOVER
  per-path capacity, so neither path is driven above BtlBw_i. Repair only ever
  draws from a bucket that is ≥ 1, so it can never overdraw a path — the "total
  per-path emission ≤ BtlBw_i incl. repair" invariant (unit-tested).
The gate applies BEFORE the symbol is generated/charged, so a HOLD wastes no coded
symbol. An un-warmed path (anchor not established) is transparent (emits, consumes
nothing), mirroring the source gate. Gated `pace_all_on = daps_pace_on &&
RWM_PACE_ALL != 0` (ON by default under DAPS pacing; `RWM_PACE_ALL=0` reproduces
the SOURCE-only pacer — the same-binary A/B baseline).

### Oracle re-confirm (temporal_oracle PART 6e) — the model ALREADY assumed total-pacing

PART 6e's "PACE (BBR)" scheduler admits ≤ BtlBw_slow **total** (`the slow path
admits <= BtlBw_slow -> outstanding ~ one BDP`) → queue 344 ms → 0, ×1.195. The
model does not split source vs repair — it paces the TOTAL per-path admission. So
the model already assumed total-pacing; the production gap was purely the
SOURCE-only pacer. Routing repair through the same gate REALIZES PART 6e's PACE
assumption — the model is unchanged (`-p raptorpath-math` 19/19). NOTE the model
covers only the SLOW path (the fast path is its min-RTprop reference, assumed
unbloated); the measured FAST-path queue is a residual the model abstracts away.

### DECISIVE L1 (VM 10.1.5.16, dual netns, 25 MB × 8, rp-native perf, SAME-binary A/B, TWO seeds)

Ceilings (post-change binary, single-path, DAPS off): `single-c2` (fast) =
**16.71**, `single-c3` (slow) = **3.13** Mbit/s ⇒ recovery ceiling C8 = **19.84**,
C7 = **33.41**; raw-link goodput ceiling C8 = 100(1−ε)+20(1−ε) ≈ **116** (not
protocol-achievable). Every arm **dnf=0** (reliable, every byte).

**C8 (c2+c3), same binary, `RWM_PACE_ALL` toggle, per seed:**

| arm | seed42 Mbit/s (σ_s) | seed7 Mbit/s (σ_s) | pooled | ×fast | eff ÷19.84 |
|---|---:|---:|---:|---:|---:|
| source-only pacer (`RWM_PACE_ALL=0`) | 7.67 (4.46) | 6.96 (9.56) | ~7.31 | 0.44× | 0.37 |
| **+ pace-all repair (this work)** | **11.88 (1.92)** | **10.34 (2.51)** | **~11.11** | **0.67×** | **0.56** |

**THE RESULT (two-seed, same-binary, stabilize-before-comparing).** Pace-all lifts
C8 on BOTH seeds — seed42 7.67→11.88 (**+55%**), seed7 6.96→10.34 (**+49%**),
pooled ~7.31→~11.11 (**+52%**), from **0.37→0.56 of the recovery ceiling**
(×0.44→×0.67 single-fast) — AND STRONGLY STABILIZES: within-arm σ_s collapses
4.46/9.56 → 1.92/2.51 s and worst-run max_s 30.2/40.6 → 19.3/22.0 s. The
source-only pacer is bimodal on both seeds this run (σ_s up to 9.6 s); pace-all
removes that catastrophic-bloat tail. Median across seeds: pace-all 11.11 (range
1.55) vs source-only 7.31 (range 0.71 in means, but σ_s 2–4× larger within arm).

**Mechanism confirmed (sender per-path DIAG, base slow RTprop ~42–46 ms, fast
~6–10 ms).** The slow-path standing queue is roughly HALVED and RTprop stays at
the propagation base:
- **Slow-path (p1) live RTT** — source-only ~650–1030 ms (RTprop polluting to
  293→1902 ms); pace-all ~200–540 ms (seed42) / ~94–713 ms (seed7), **RTprop stays
  at the 42–46 ms base** (min-filter clean).
- **Fast-path (p0) live RTT** — source-only ~113–162 ms; pace-all ~63–136 ms.

**REMAINING RESIDUAL (honest — lifts but does NOT reach the ceiling).** C8 is 0.56
of the recovery ceiling, not ~1.0. The slow-path queue is halved (not collapsed to
base: ~200–540 ms vs 42 ms) and a fast-path queue (~100 ms) persists. Pacing the
REPAIR closed the dominant unpaced contributor, but a residual queue remains — the
SOURCE spill still drives the fast path's bucket negative (the source gate spills
but does not HOLD, unlike repair), and the fast-path source burst is not bounded.
The next residual is a TRUE per-path hold on the SOURCE spill (bound the fast path
for source too) + the fast-path burst — NOT the repair pacing this work fixed.

**C7 (c2+c2) — symmetric control, no regression:** pace-all = **21.02** (σ_s 0.59,
1.26× single-fast), within noise of the shipped DAPS+QM C7 (21.41) ⇒ no regression;
the same-binary source-only was **12.08** (σ_s **14.29**, bimodal this run), so
pace-all also STABILIZES C7 strongly (σ_s 14.3→0.6). Symmetric skew ⇒ repair
spreads evenly across equal-BtlBw buckets, so the gate rarely holds and does not
starve.

### Controls / no regression

`cargo test -p raptorpath --lib` 285/285 (2 new:
`pace_all_traffic_bounds_total_per_path_emission_at_btlbw`,
`pace_all_traffic_holds_when_both_paths_dry`); `-p raptorpath-math` 19/19 incl.
temporal_oracle PART 6e (unchanged); gate_suite 15/15 release. Shipped non-DAPS
default byte-identical (`pace_all_on` requires `RWM_DAPS`; the gate is a NO-OP
otherwise, and the non-generation default never reaches the repair-emission blocks;
the gate-first reorder is behaviour-neutral when it returns `Some(candidate)`). All
L1 arms dnf=0. r*≈0.03 (RWM_GEN_R=0.03).

**VERDICT.** The diagnosed residual (unpaced coded/repair emission) is CLOSED:
total per-path emission (source + repair) is now metered at BtlBw_i, roughly halving
the slow-path standing queue while RTprop stays at the propagation base. This lifts
pooled C8 from 0.37→0.56 of the recovery ceiling (×0.44→×0.67 single-fast), holds on
BOTH seeds (+49% / +55%), and strongly stabilizes C8 AND C7. It does NOT reach the
ceiling — the SOURCE spill (which spills but does not hold) plus the fast-path burst
leave a residual per-path queue (slow ~200–540 ms, fast ~100 ms). Regime-map:
heterogeneous aggregation is scheduling-bound (DAPS) AND rate-estimation-bound
(estimator, closed) AND repair-pacing-bound (this work, closed) AND still
source-spill/fast-path queue-bound — improved and stabilized materially on every
axis measured, not yet at the goodput ceiling.

## Source Backpressure (2026-07-12) — REFUTED at L1: deferring the SOURCE to the per-path bucket REGRESSES C8 ~53% on BOTH seeds; the spill baseline is benign; kept as a default-OFF, oracle-modelled, unit-tested knob (branch `feat/source-backpressure`)

*Decision record: → [ADR-0065](adr/0065-daps-era-refutations.md)*

**[AUDIT 2026-07-13: UNCERTAIN — the REFUTED verdict is UNSAFE either way;
generation-inert measurement.]** `RWM_SRC_BP` is dead without generation
(`src_bp_on = daps_pace_on && …`; the TUN-read defer is entirely inside
`src_bp_on`), which the harness never enabled: if inert, both arms were
byte-identical and the code CANNOT explain the −53% — only session drift can
(the same nominal config measured 14.99 / 10.74 / 6.50 across three sessions).
The quoted `paused=100%` stretches occur in plain mode in BOTH arms (ordinary
store-gate stalls), and the section itself says the gate "rarely engages" —
inconsistent with it causing −53%. VOIDED: the scientific REFUTED verdict and
the "source is the pipeline clock" mechanism (re-measure interleaved,
generation-verified, before citing either). SURVIVES: the #73 default-OFF ship
decision, on prudence alone; unit tests; oracle PART 6f as a model. Valid
numbers: "Generation-ON Re-Baseline (2026-07-13)".

Pace-all (above) held the rateless REPAIR when both per-path buckets were dry but
the SOURCE placement gate still SPILLED to the fast path unconditionally and
decremented its BtlBw bucket NEGATIVE (an unmetered burst). Hypothesis: source is
payload (cannot be dropped), so the discipline should be DEFER not discard — when
neither the DAPS candidate NOR the fast (spill) path has a funded bucket, PAUSE the
TUN read (the app / QUIC send-buffer backpressures) instead of bursting, making
TOTAL per-path emission (source + repair) ≤ BtlBw_i on EVERY path (the source
analogue of the repair HOLD). **L1 REFUTED it.**

### The fix (as implemented, now default OFF)

One pure gate `source_pace_admit(tok, cand, fast)` (net/mod.rs ~476) peeks whether a
source symbol can be emitted on SOME path without overdrawing any bucket (candidate
funded → admit; candidate dry but fast funded → admit-spill; BOTH dry → DEFER).
Wired into the sender's `read_packet` select arm (`src_pace_ok`) with a 1 ms refill
wake, gated `src_bp_on = daps_pace_on && RWM_SRC_BP∈{1}`. Unit-tested
(`source_backpressure_defers_when_both_paths_dry`,
`source_backpressure_bounds_total_per_path_emission_no_negative_bucket` — the latter
proves no bucket goes negative under over-offer AND that the pace-all spill baseline
DOES go negative, the residual being targeted).

### Oracle re-confirm (temporal_oracle PART 6f — the fast-path model 6e abstracted)

PART 6e paced only the SLOW path (fast = unbloated min-RTprop reference). PART 6f
(NEW) models the fast path under source burst: unpaced source → fast outstanding =
share·(deep read-ahead) → q_f = 374 ms (>> the 30 ms DAPS slack); the fast bucket
driven negative reports "dry" so repair spills to the slow path, re-creating 6e's
DUMP (q_s = 344 ms) → C8 falls to **parity x1.000**. Deferring source → fast
outstanding → one BDP (q_f → 0, RTT → base) AND the coupling removed (q_s → 0) →
C8 → the resequencing optimum **x1.195**, no queue residual after both paced (the
model's structural floor IS the ceiling). `-p raptorpath-math` 20/20 (6e unchanged,
6f added). **The model predicts defer-source lifts C8; L1 measured the OPPOSITE.**

### DECISIVE L1 (VM 10.1.5.16, dual netns, 25 MB × 8, rp-native, SAME-binary A/B via `RWM_SRC_BP`, seeds 42 AND 7)

Ceilings (this binary): single-c2 (fast) median **15.9** (mean 9.8 bimodal — 1 stall
outlier, prior 16.71), single-c3 (slow) **3.26** Mbit/s ⇒ recovery ceiling C8 ≈
**19.8**; raw-link goodput ceiling ≈ 100(1−ε)+20(1−ε) ≈ **116** (not
protocol-achievable). Every arm **dnf=0** (reliable, every byte).

**C8 (c2+c3), same binary, `RWM_SRC_BP` toggle, per seed:**

| arm | seed42 Mbit/s (σ_s) | seed7 Mbit/s (σ_s) | pooled | eff ÷19.8 |
|---|---:|---:|---:|---:|
| **spill baseline (`RWM_SRC_BP=0`, shipped default)** | **14.35 (1.11)** | **15.63 (1.35)** | **~14.99** | **0.76** |
| + source backpressure (`RWM_SRC_BP=1`) | 6.60 (9.48) | 7.39 (4.13) | ~7.00 | 0.35 |

**THE RESULT (two-seed, same-binary).** Source backpressure REGRESSES C8 on BOTH
seeds — seed42 14.35→6.60 (**−54%**), seed7 15.63→7.39 (**−53%**), pooled
~14.99→~7.00 (**−53%**, 0.76→0.35 of the recovery ceiling) — AND destabilizes it
(σ_s 1.11/1.35 → 9.48/4.13 s, max_s 15/14 → 44/30 s). It is REFUTED. dnf=0 in both
arms (reliability intact).

**Mechanism (sender per-path DIAG).** (1) Deferring the source TUN read STALLS the
generation-fill pipeline — the source read is the pipeline CLOCK, so pausing it
starves coded emission too (long `paused=100% good=0` stretches). Unlike the
rateless repair HOLD (a dropped repair is free), source is the pipeline input;
holding it wedges the whole transfer. (2) The gate is also largely INERT: the
per-path BtlBw ANCHOR is OVER-READ under fast-path bufferbloat (DIAG: fast bdp
14509 sym / RTprop 12 ms ⇒ btlbw ≈ 1.2M sym/s vs the true ~8333 sym/s — ~145×), so
the pace bucket (burst-cap 64, refill ≫ drain) almost never goes dry ⇒ the
backpressure rarely engages where the queue actually is, and where it DOES engage it
only stalls. The fast-path live RTT did NOT collapse under backpressure (~1000–1800
ms in both arms) — confirming the residual is the anchor over-read, NOT the source
spill.

**The spill baseline is BENIGN.** Spilling source to the fast path when the slow
bucket is dry is fine: the fast LINK (100 Mbit netem) drains it, so the fast queue
is a LATENCY cost, not a throughput cost, for a bulk transfer. The baseline sits at
**0.76 of the recovery ceiling**, stable (σ_s ~1.2 s), on both seeds — materially
BETTER than the pace-all report's 0.56 (the binary/estimator lineage evolved; the
full x8 is more favorable). Nothing about the fast-path SPILL needs fixing.

### Controls / no regression

`cargo test -p raptorpath --lib` 287/287 (2 new source-BP tests); `-p
raptorpath-math` 20/20 (temporal_oracle 6f added, 6e unchanged); gate_suite 15/15
release. **C7 (c2+c2) shipped default = 21.01** (σ_s 0.93, dnf=0) — matches prior
21.02, NO regression. Single-c2/c3 parity (the source-BP gate is a NO-OP on single
path: cand == fast, always admittable). Shipped DEFAULT byte-identical: `src_bp_on`
requires `RWM_SRC_BP=1`; unset/0 computes nothing (the read-guard clause
`(!src_bp_on || src_pace_ok)` short-circuits, `src_pace_ok` not evaluated). r*≈0.03.

**VERDICT.** The hypothesis is REFUTED: source is NOT a droppable/holdable emitter —
it is the generation-fill clock, so per-path backpressuring it stalls the pipeline
and regresses C8 ~53% on both seeds; and the gate is inert anyway because the
per-path BtlBw anchor is over-read under bufferbloat so the bucket never binds. The
pace-all SPILL of source is benign (the fast link drains it) and the shipped default
already sits at 0.76 of the recovery ceiling, stable on both seeds. The named NEXT
residual is the **per-path BtlBw anchor over-read under fast-path bufferbloat**
(ack-aggregation / delivered-rate over-read) — the signal that would let ANY per-path
pacer (repair OR source) actually bind — NOT the source spill. Feature retained as a
default-OFF, oracle-modelled, unit-tested knob for the scientific record.

## BtlBw Rate-Sample Fix (2026-07-12) — the per-path anchor over-read CLOSED (fast ×158→×1, fast bufferbloat 1573→30ms); but C8 does NOT rise under DAPS — it REGRESSES; the true residual is the slow-path deep read-ahead, not the source anchor (branch `feat/btlbw-rate-sample`)

*Decision record: → [ADR-0065](adr/0065-daps-era-refutations.md)*

**[AUDIT 2026-07-13: UNCERTAIN — generation-inert measurement; the C7
"politeness regression" sub-claim is refuted-as-noise.]** If generation was
off (the harness never enabled it), `rate_sample` was dead AND the legacy
anchor was equally unfed in both arms — the sender DIAG could show neither
×158 nor ×1.05, so the anchor-DIAG contrast comes from an unrecorded
generation-on run or a wrong log and cannot be attached to the throughput
battery; the ×158→×1.05 "CLOSED" and fast-bufferbloat 1573→30 ms claims are
unverified. The C7 20.96→16.97 "rate-throttle politeness" regression is
refuted-as-noise by §16.14's own symmetric identical-code arms (21.20 vs
16.96, a 20% pure-noise swing) — which also voids oracle PART 6h's claimed
calibration ("reproduces 0.810 exactly"). C8 −9.5% pooled is inside session
noise. Survives: unit tests; PART 6g as a model. The later Slow-Path Anchor
Diagnosis found the anchor DOES establish generation-ON but swings ~4000× — a
different story from both arms here. Valid numbers: "Generation-ON Re-Baseline
(2026-07-13)".

The prior three sections (Per-Path Estimator, Pace-All, Source Backpressure) each
named the SAME residual: the per-path BtlBw anchor is over-read under bufferbloat so
no per-path pacer/cap can bind. This work fixes the anchor with BBR-correct
delivery-rate sampling and reports the honest consequence.

### The bug and the fix

The estimator (#71) derived BtlBw_i from `Δdelivered / Δt_ack` — delivered-rate over
the ACK-ARRIVAL interval. Under DAPS acks arrive BATCHED, collapsing Δt_ack, so the
windowed-MAX locked onto the aggregation spike. The fix (Cardwell/Cheng,
draft-cheng-iccrg-delivery-rate-estimation): per-path BBR delivery-rate sampling —
each source symbol snapshots `(sent_time, delivered, delivered_time, first_sent_time,
app_limited)` at send (`on_src_sent`); its ack computes ONE sample `Δdelivered /
max(send_elapsed, ack_elapsed)` (`on_src_delivered_seq`). Δt is the SEND interval, so
a batched ack (tiny ack_elapsed) is overridden by the true send spacing. Two BBR
guards: samples spanning `< MinRTT` (RTprop) are rejected (the ack-aggregation /
send-burst artefact — this is what fixes the DAPS SLOW-path burst over-read), and
app-limited samples may only RAISE the max. Gated `RWM_RATE_SAMPLE` (on by default
under the estimator; =0 = legacy ack-interval anchor, same-binary A/B). Shipped
non-DAPS default byte-identical.

### Oracle re-confirm (temporal_oracle PART 6g)

PART 6g (`rate_anchor_overread_makes_pacer_inert`) models the anchor over-read: a
token bucket clocked at the ANCHOR rate holds ~R·RTprop outstanding, so a ×145
over-read makes the bucket occupancy (640 slow / 3200 fast syms) swamp the deep
read-ahead share → the pace/cap is INERT → both paths DUMP → C8 = ×1.000 (parity,
matching the measured 14.99 ≈ single-c2 alone). Feeding the CORRECT (×1) anchor makes
the bucket bind at one BDP → queues collapse → C8 = ×1.195 (ceiling). The model shows
the fix UNLOCKS the previously-inert pacer — so the build proceeded. (L1 below shows
the model's optimism is right on the FAST path but the SLOW path carries a second,
read-ahead-driven queue the queue model omits.)

### DECISIVE L1 (VM 10.1.5.16, dual netns, 25 MB × 8, rp-native perf, SAME-binary A/B via `RWM_RATE_SAMPLE`, seeds 42 AND 7; DAPS r=0.03; 1200-B symbols)

**PRIMARY METRIC — the anchor over-read, CLOSED (sender DIAG, C8 dual).** True link
rates: fast (c2 100 Mbit) = 10 416 sym/s, slow (c3 20 Mbit) = 2 083 sym/s.

| arm | fast BtlBw_i (sym/s) | ×over-read | fast bdp / cap | fast live RTT | slow RTprop | slow BtlBw_i |
|---|---:|---:|---:|---:|---:|---:|
| legacy (`RWM_RATE_SAMPLE=0`) | **1 644 200** | **×158** | 19 403 | **1 573 ms** | polluted 128 ms | 20 364 (×9.8) |
| **rate-sample (`=1`)** | **≈ 10 900** | **×1.05** | ~90 | **~30 ms** | base **41 ms** | ~3 200–7 700 (×1.5–3.7) |

The fast-path anchor over-read drops **×158 → ×1.05** and the fast-path bufferbloat
collapses **1 573 ms → ~30 ms** (RTprop 8 ms base). The slow-path RTprop, polluted to
128 ms under the queue, returns to the 41 ms base and its over-read drops ×9.8 → ~×2–3.
This is the PRIMARY success metric and it is met.

**Single-path — throughput NEUTRAL, both arms stable (fix arm, x8, seed42).** single-c2
`RWM_RATE_SAMPLE=1` = **16.65 Mbit/s (σ_s 1.19, dnf=0)** vs legacy `=0` = **16.29
(σ_s 1.27)** — statistically identical, both TIGHT. The fast-path over-read
bufferbloats the RTT (1573 ms, DIAG) but on a SINGLE 100-Mbit path that is pure
LATENCY: the link drains at line rate regardless of buffer occupancy, so throughput
is unaffected. (The earlier report's single-c2 bimodality — median 15.9 / mean 9.8 —
is a different-binary/regime artefact and is NOT reproduced here; the anchor fix
therefore does not "stabilize" single-c2 — it was already stable.) single-c3 fix =
**3.19** (σ_s 2.26) / legacy = 3.20 (σ_s 1.98) ⇒ recovery ceiling C8 (fix) = 16.65 +
3.19 = **19.84 Mbit/s** (C8 fix pooled ~9.7 = **0.49 of the recovery ceiling**;
legacy pooled ~10.74 — the fix REGRESSES C8 pooled ~9.5%, seed-dependent).

**C8 (c2+c3) — does NOT rise; the honest critical finding.**

| arm | seed42 Mbit/s (σ_s) | seed7 Mbit/s (σ_s) | pooled |
|---|---:|---:|---:|
| legacy anchor (`=0`) | 13.25 (2.31) | 8.22 (4.19) | **~10.74** |
| **rate-sample (`=1`)** | **10.73 (1.59)** | **8.71 (3.65)** | **~9.7** |

Correcting the anchor does NOT lift C8 — it REGRESSES it pooled (~10.74 → ~9.7,
−9.5%), and the effect is SEED-DEPENDENT: seed42 regresses clearly (13.25 → 10.73),
seed7 is neutral (8.22 → 8.71). (Both arms carry heavy cross-seed spread — the LEGACY
arm is in fact MORE seed-bimodal, 13.25/8.22 range 5.0, than the fix, 10.73/8.71 range
2.0 — so the fix trades seed42's peak for cross-seed consistency, not a net win.) The
mechanism (per-path DIAG): the fast-path SPILL the over-read enabled was BENIGN — the
100-Mbit fast link drained it (latency, not throughput). BINDING the fast pacer (via
the correct anchor) removes that benign spill and forces load onto the slow path, whose
live RTT then bloats to **~3–4 s** EVEN THOUGH its anchor and BDP cap are now correct
and its per-path SOURCE gauge (`sinfl`) sits AT the cap. So the slow queue is NOT the
source rate anchor: it is the DEEP DAPS read-ahead (`(pipeline+6)·G`, winbackstop 3072)
+ future-offset placement + coded/repair depth, which over-commits the slow path and
holds the receiver's resequencing frontier — a queue the corrected SOURCE pacer does
not bound. The CLEAREST aggregation regression is the symmetric C7 (below): with DAPS
Δ=0 there is no read-ahead placement asymmetry, yet binding both pacers at the correct
(lower) rate still drops C7 20.96 → 16.97 and destabilizes it — the corrected pacer
simply leaves link capacity unused that the over-read spill had opportunistically filled.

**C7 (c2+c2) symmetric control:** fix `=1` = **16.97** (σ_s 4.38, median 20.85) vs
legacy `=0` = **20.96** (σ_s 0.55) — the fix REGRESSES and DESTABILIZES C7 too (−19%),
even though DAPS Δ=0 on a symmetric pair: binding both pacers at the corrected (lower)
rate leaves link capacity unused vs the over-read spill. **Shipped-default no-DAPS
controls (byte-identical path — `rate_sample` requires `generation && (DAPS ||
RWM_PER_PATH_EST)`):** C7 = **21.52** (σ_s 0.57, matches prior 21.01), single-c2 = **16.81** (σ_s 1.18) — match the
prior shipped default (unchanged by this branch).

### Controls / no regression

`cargo test -p raptorpath --lib` 289/289 (2 new: `rate_sample_anchor_reads_true_btlbw_under_aggregation_and_queue`,
`rate_sample_excludes_app_limited_samples_below_the_max`); `-p raptorpath-math` 21/21
(temporal_oracle PART 6g added); gate_suite 15/15 release. All L1 arms dnf=0. Shipped
non-DAPS default byte-identical (`rate_sample` gated on the DAPS/estimator path; the
legacy `on_src_delivered` path is untouched when `RWM_RATE_SAMPLE=0` or off-DAPS).

### VERDICT

The rate anchor was genuinely broken and is now fixed: fast-path over-read **×158 →
×1.05**, fast-path bufferbloat **1 573 → 30 ms**, and the slow-path RTprop de-polluted
(**128 → 41 ms**). That is a real correctness win (a truthful per-path rate signal +
collapsed fast-path latency) and the necessary precondition for any binding per-path
pacer. It is throughput-NEUTRAL on single-path (single-c2 16.65 vs 16.29, both stable
— the over-read bufferbloat there is benign latency). But it does NOT lift
heterogeneous aggregation — under DAPS it REGRESSES it (C8 pooled 0.55 → 0.49 of the
recovery ceiling; C7 20.96 → 16.97, the clearest and most stable regression) — because
the aggregation gap is NOT the source rate anchor. The corrected anchor merely REMOVES
the benign fast-path spill (which the fast link had drained for free) and, on the
heterogeneous cell, exposes the true C8 residual: the slow-path DEEP READ-AHEAD
over-commit (DAPS future placement + coded/repair depth, not source pacing), whose
~3–4 s slow-path queue survives a correct anchor + BDP cap. The regime map: aggregation
is scheduling-bound (DAPS), rate-estimation-bound (this work, CLOSED — fast anchor
×158→×1), and — the newly-isolated residual — slow-path **read-ahead-depth-bound** /
pacer-politeness-bound (a correctly-paced link is left under-filled where the over-read
spill opportunistically filled it). HONEST BOTTOM LINE: the anchor was the primary bug
and it is fixed, but fixing it does NOT buy throughput — it is throughput-neutral on
single-path and regresses DAPS aggregation. It ships on by default under the estimator
because it is the correct rate signal (and the precondition any future pacer needs);
the aggregation regression it exposes is the honest handoff to the read-ahead /
pacer-headroom work, reproducible via the same-binary `RWM_RATE_SAMPLE=0`.

## DAPS Read-Ahead Depth (2026-07-12) — the last structural lever: depth-bounding is the CORRECT, best-performing, most-stable mechanism, but it CANNOT bind because the slow-path rate anchor never establishes; bulk C8 heterogeneous aggregation is BOUNDED below fast-path-alone — CONSOLIDATE (branch `feat/daps-readahead-depth`)

*Decision record: → [ADR-0065](adr/0065-daps-era-refutations.md)*

**[AUDIT 2026-07-13: INVALID (PROVEN) — generation-inert measurement +
wrong-log DIAG.]** The saved battery sender logs show `cod=0`/`eff_pace=0`
everywhere: DAPS, rate-sample, and the depth bound were ALL inert; arms A/B/C
executed identical transfer code, so the A<B<C ordering and the stability
story are draws from one distribution — the throughput verdict is void.
"Slow anchor never establishes" was read from the RECEIVER log
(`/tmp/rwm-s.log`) and is directly REFUTED by the Slow-Path Anchor Diagnosis
(generation-ON, sender log: it establishes for the whole active transfer).
The CONSOLIDATE recommendation is VOID (withdrawn by that diagnosis). The one
survivable observation — dual C8 below single-c2 — holds only for PLAIN
window-reliable (already known: 5.43 vs 15.9), NOT as a bound on the
DAPS/generation stack. Survives: unit tests; oracle PART 6h as a model (its
claimed L1 calibration target was a noise artifact). Valid numbers:
"Generation-ON Re-Baseline (2026-07-13)".

The three prior negatives (Pace-All §16.11, Source-Backpressure §16.12, Rate-Sample
§16.13) all converged on ONE residual: with the anchor now CORRECT (§16.13: fast
×158→×1, fast bufferbloat 1573→30 ms) AND the BLEST BDP cap engaged AND `sinfl`
pinned at the cap, the SLOW path STILL bloats to ~3–4 s live RTT. The diagnosed cause
is the deep DAPS read-ahead OVER-COMMIT: DAPS places FUTURE data on the slow path
offset by the skew Δ, but over-commits the DEPTH (far more than Δ·BtlBw_slow of
look-ahead), so that data arrives after the fast path would have delivered the
in-order region → HoL-blocks reassembly. The §16.13 rate pacer could not fix it
because throttling RATE left the link IDLE (politeness regression, C7 20.96→16.97).
This work bounds the DEPTH, not the rate — and reports the HONEST BOUND.

### The fix — bound read-ahead DEPTH to the skew (correct DAPS/ECF), NOT rate

Each non-fastest path j may hold at most `skew_j·BtlBw_j` symbols of read-ahead beyond
the fast-path frontier — exactly the latency-skew depth, so its queue delay
(`outstanding_j/BtlBw_j`) stays ≤ skew and the slow segment arrives in-order-aligned,
never later than the fast path would deliver that region (the ECF/BLEST completion
guard done on DEPTH). Beyond that depth path j is dropped from the DAPS-eligible set
(fresh SOURCE → fast path) and REPAIR is steered off it (`daps_depth_over_budget`).
Crucially a DEPTH limiter, NOT a rate throttle: the pace bucket still refills at
BtlBw_j, so within the budget the path emits at its natural link rate (never idled) —
that is what escapes §16.13's rate-throttle politeness idle. It is STRICTLY tighter
than the BLEST BDP cap (skew ≤ RTprop). Gated `RWM_DAPS_DEPTH` (ON under DAPS+
rate-sample; =0 = current unbounded read-ahead, the same-binary A/B). Requires the
correct anchor (rate_sample) so `skew·BtlBw_j` is right-sized. Shipped non-DAPS
default byte-identical (the gate requires `generation && DAPS`).

### Oracle re-confirm FIRST (temporal_oracle PART 6h) — models BOTH failure modes

PART 6h adds the UTILIZATION axis the pure queue model (6e/6f/6g) lacked, so it can
distinguish a DEPTH bound (keeps the link FULL) from a RATE throttle (idles it):
- **A DEPTH-UNBOUNDED** (current): full link (util=1) but the whole read-ahead depth
  (~3.5 s bloat) queues → useful→0 → C8 → **×1.000** (parity, the bloat wasted).
- **B RATE-THROTTLE** (§16.13): queue bounded (useful=1) but the rate clock idles the
  link (util η=0.81) → **×1.158**; applied symmetrically to C7 it reproduces the
  measured **20.96→16.97 exactly** (0.810), proving the model is not too coarse.
- **C DEPTH-BOUND** (this work): full link (util=1) AND read-ahead within one skew
  (useful=1) → C8 → **×1.195** (ceiling), C7 restored.
The model shows depth-bound beats BOTH traps and reproduces the §16.13 regression → the
build proceeded. `-p raptorpath-math` 22/22.

### DECISIVE L1 (VM 10.1.5.16, dual netns, 25 MB × 8, rp-native perf, SAME-binary THREE-arm A/B, seeds 42 & 7; DAPS r=0.03; 1200-B symbols; interleaved to cancel VM drift)

Arms (same binary): **A** legacy anchor (`RWM_RATE_SAMPLE=0`), **B** rate-sample only
(`RWM_RATE_SAMPLE=1 RWM_DAPS_DEPTH=0`, the §16.13 regressed arm), **C** rate-sample +
depth-bound (`RWM_RATE_SAMPLE=1 RWM_DAPS_DEPTH=1`, the fix). Ceilings (arm-C binary,
single path): single-c2 (fast) = **16.45** (σ_s 1.12), single-c3 (slow) = **3.24**
(σ_s 2.15) ⇒ recovery ceiling C8 = **19.69** Mbit/s. Every arm **dnf=0** (every byte).
NOTE this session ran in a NOISIER/lower regime than §16.13 (A-legacy C8 6.5 vs the
prior 13.25) — but single-c2 (16.45 vs 16.65) and the CROSS-ARM ordering are stable,
and the aggregation verdict is regime-independent (see below).

**C8 (c2+c3), per seed — mean Mbit/s (σ_s) [worst→best per-run Mbit/s]:**

| arm | seed42 | seed7 | pooled | ×fast (16.45) | eff ÷19.69 |
|---|---:|---:|---:|---:|---:|
| A legacy (`RS=0`) | 6.50 (10.3) [4.6→15.7] | — (timed out) | ~6.50 | 0.40× | 0.33 |
| B rate-sample (`RS=1 DEPTH=0`) | 7.79 (26.8) [**2.2**→13.6] | 6.65 (29.7) [**1.9**→14.3] | ~7.22 | 0.44× | 0.37 |
| **C depth-bound (`RS=1 DEPTH=1`)** | **8.24 (5.6)** [6.7→16.5] | **8.55 (9.1)** [5.3→14.8] | **~8.40** | **0.51×** | **0.43** |

**THE RESULT — best arm, most stable, but NO aggregation.** Arm C is the BEST of the
three on BOTH seeds (pooled 8.40 vs B 7.22 vs A 6.50) and DRAMATICALLY the most stable:
σ_s collapses 26.8/29.7 → 5.6/9.1 s and it REMOVES arm B's catastrophic bimodal bloat
tail (B's worst single runs 92 s / 103 s ≈ 1.9–2.2 Mbit/s; C's worst 30–37 s ≈
5.3–6.7 Mbit/s — the worst-case FLOOR roughly triples). **BUT C8 arm C (8.40) is only
0.51× fast-path-alone (16.45) and 0.43 of the recovery ceiling — it does NOT aggregate.
Adding the slow path leaves dual-path C8 at HALF of using the fast path alone.** This
holds across EVERY arm and BOTH sessions: even §16.13's best C8 (10.7) was already below
its single-c2 (16.65). In no measured configuration does heterogeneous bulk C8 exceed
fast-path-alone.

**Mechanism — the depth bound is INERT because the slow anchor never establishes.**
Sender per-path DIAG (arm C, C8 het): the fast path (p0) warms its BtlBw anchor
(`est=Y`), but the SLOW path (p1) **never does — `est=n`, `btlbw=0`, and therefore
`dbud=0` (the skew-depth budget `skew·BtlBw_slow` is UNDEFINED) throughout**. With no
slow-path rate anchor there is no `skew·BtlBw_slow` to bound the depth against, so the
depth guard is a NO-OP on the very path it targets — exactly as the pacers/caps before
it were inert. The slow path's live RTT bloats UNBOUNDED to ~1.4–1.5 s (RTprop pollutes
to ~1.4 s), the residual intact. The chain estimator→correct-anchor→depth-bound breaks
at the SLOW anchor, which does not warm in this loss/skew regime (the slow path is
acked too sparsely / too batched for the BBR min-RTT-guarded sampler to populate a
max-filter). So arm C's modest lift + big stability win over B is NOT the depth
mechanism binding (it can't — `dbud=0`); it is the fast-path repair-steer + the removal
of the over-read spill's worst transients. The correct, oracle-confirmed, unit-tested
depth mechanism has no rate signal to act on.

**C7 (c2+c2) symmetric — NOISE-dominated, no reliable signal:** A `RS=0` = 17.20
(σ_s 2.5), B `RS=1 DEPTH=0` = **21.20** (σ_s 1.0), C `RS=1 DEPTH=1` = **16.96** (σ_s
1.1); shipped-noDAPS control = 13.29 (σ_s 14.7, bimodal). The depth bound is a PROVABLE
no-op on symmetric paths (skew 0 ⇒ no depth budget ⇒ `daps_depth_over_budget`=false;
unit-tested), so arms B and C execute the SAME code path — yet they differ by 20%
(21.20 vs 16.96). That 20% is therefore PURE VM NOISE, which also implies §16.13's C7
"regression" (20.96→16.97, −19%) was itself largely noise, not a real rate-throttle
effect. C7 gives no reliable evidence either way; the depth bound does not regress C7
in mechanism.

### Controls / no regression

`cargo test -p raptorpath --lib` 292/292 (3 new: `daps_depth_bound_caps_slow_path_
readahead_at_skew_btlbw`, `daps_depth_bound_does_not_rate_throttle_within_budget`,
`daps_depth_bound_noop_on_symmetric_and_warmup`); `-p raptorpath-math` 22/22
(temporal_oracle PART 6h); gate_suite 15/15 release. All L1 arms dnf=0 (reliable, every
byte). single-c2/c3 parity (depth is a NO-OP on single path). Shipped non-DAPS default
byte-identical (`daps_depth_on` requires `rate_sample` ⇒ `generation && DAPS`; unset
computes nothing). r*≈0.03. (Gap: A-legacy seed7 hit the 760 s battery timeout across
its x8 — the RS=0 fast-path over-read bufferbloat stalls that arm; A-legacy is the old
baseline, not load-bearing for the verdict.)

### VERDICT — the HONEST BOUND: bulk C8 aggregation is structurally bounded; CONSOLIDATE

The depth bound is the CORRECT mechanism (oracle-confirmed to reach the ×1.195 ceiling,
unit-tested, byte-identical shipped default) and empirically the BEST and most STABLE of
the three arms — it removes rate-sample's catastrophic bimodal bloat tail (worst-case
floor ~1.9→5.3 Mbit/s) and is harmless (dnf=0, no C7-mechanism regression). It ships ON
by default under DAPS+rate-sample as the best-available stack. **But it does NOT land
heterogeneous aggregation: C8 arm C (8.40) sits at 0.51× fast-path-alone and 0.43 of the
recovery ceiling — dual-path is WORSE than the fast path alone — and this holds across
every arm and both measurement sessions.** The mechanism is INERT where it matters: the
slow-path BtlBw anchor never establishes (`est=n`/`dbud=0`), so `skew·BtlBw_slow` is
undefined and no depth (or rate) bound can bind to the slow path, whose RTT bloats
unbounded to ~1.5 s. **This was the last structural scheduling lever.** The queue is
LATENCY-not-throughput (HoL/resequencing coupling), and the slow path's marginal
~3.2 Mbit/s is not economically aggregatable for bulk under this loss (GE ε≈0.026) and
skew (Δ≈30 ms): its contribution is dominated by its own tail/anchor-establishment cost.
**RECOMMENDATION: CONSOLIDATE — stop the pacing/scheduling line.** The full evidence
chain (estimator #71 → correct anchor §16.13 → depth-bound this work) shows each lever
is correct in isolation and confirmed in the oracle, but the binding constraint is the
slow path's failure to establish a usable rate anchor in this regime — a channel/CC
property no source-side scheduler can synthesize. Reproducible via the same-binary
`RWM_DAPS_DEPTH=0` / `RWM_RATE_SAMPLE=0`.

## Slow-Path Anchor Diagnosis (2026-07-13) — the §16.14 "slow anchor never establishes" is REFUTED: it establishes for the whole active transfer, but BtlBw_slow is a decode-clocked windowed-MAX that swings ~4000× (5–20950 sym/s) so no depth/pace bound can key on it — FIXABLE (stabilize the per-path rate signal), NOT fundamental (branch `diag/slow-path-anchor`, DIAG only, no feature)

*Decision record: → [ADR-0065](adr/0065-daps-era-refutations.md)*

Diagnostic investigation of why the per-path SLOW-path BtlBw anchor "never
establishes" (`est=n`, `btlbw=0`, `dbud=0`) that §16.14 named as the binding
constraint before recommending CONSOLIDATE. Added temporary per-path DIAG
counters that trace the BBR rate-sample pipeline end-to-end (snapshotted-at-send /
attributed / generated / rejected-by-guard / windowed-max fill), gated under
`RWM_DIAG`; shipped default byte-identical (the counters live in the
`rate_sample` path, which is `generation && (DAPS || RWM_PER_PATH_EST)`-gated, and
the DIAG print is `RWM_DIAG`-gated). One representative L1 C8 run (VM 10.1.5.16,
dual netns, c2+c3, 12 MB × 3, seed 42, `RWM_DAPS=1 RWM_GEN_R=0.03
RWM_RATE_SAMPLE=1 RWM_DAPS_DEPTH=1`).

### TWO HARNESS FINDINGS that invalidate the §16.14 mechanism evidence (not its C8 numbers, its DIAG story)

1. **§16.14's per-path DIAG was read off the RECEIVER, not the sender.** The
   depth battery `cp /tmp/rwm-s.log` captures the `--server`, which in
   `perf_rwm_c.sh` is the perf RECEIVER of the bulk transfer; its sender loop
   emits only a trickle of reverse traffic (measured `sent=3` source snapshots
   over a whole run). So "slow path p1 `est=n`, `btlbw=0` throughout" was a
   wrong-log artefact — the receiver legitimately places ~no source on any path.
   The bulk SENDER is the `--client` (`/tmp/rwm-c.log`), where the anchor lives.
2. **`perf_rwm_c.sh` never passes `--window-generation-coding`.** `generation`
   requires that CLI flag (or `--window-systematic-repair`/`--window-coded-only`
   +systematic/`RWM_FMTCP`); `RWM_DAPS`/`RWM_GEN_R` only *configure* generation,
   they do not *enable* it (`daps = RWM_DAPS && generation`). The saved §16.14
   server logs confirm it: `cod=0`, `eff_pace=0` everywhere ⇒ zero coded/
   generation emission. So the depth battery ran PLAIN `--window-reliable` block
   mode with DAPS + rate-sample + depth-bound ALL INERT (each gates on
   `generation`). The `est=Y`/`est=n` it quoted was the legacy `on_ack →
   record_delivery` anchor, which is a *different* estimator than the rate-sample
   pipeline the section was reasoning about. This diagnosis re-ran with
   `RWM_EXTRA=--window-generation-coding` so the mechanism under test actually
   runs (`eff_pace=2000`, the pipeline activates).

### The end-to-end trace (sender/client DIAG, generation actually ON)

| path | sent | attr | gen (samples) | rej[iv/zr/al] | fill (max) | est | BtlBw (sym/s) |
|---|---:|---:|---:|---:|---:|:--:|---:|
| p0 FAST (c2, true ≈10 400) | 27 016 | 24 588 | 24 535 | 0 / 0 / 52 | 925 | **Y** (100% active) | 13 271 – **85 860** (×1.3–8) |
| p1 SLOW (c3, true ≈2 083) | **3 444** | **3 443** | **3 435** | **0 / 0 / 8** | **17–2 589** | **Y (90/108 lines; 100% of the active transfer)** | **5 – 20 950 (~4000× swing)** |

The slow path is **NOT starved** (`sent=3444` real source placed, ~230 sym/s
delivered share), **NOT mis-attributed** (`attr=3443`, every ack resolves to p1),
**NOT guard-rejected** (only 8 app-limited rejects; `iv=0` MinRTT never fires,
`zr=0`), and it **GENERATES samples** (`gen=3435`) and **ESTABLISHES the anchor**
(`est=Y` in 90/108 DIAG lines = 100% of the active transfer, t=0.3–22.9 s). This
DIRECTLY REFUTES §16.14's "`est=n`/`btlbw=0`/`dbud=0` throughout, never warms."

### The ONE proven root cause — an UNSTABLE (decode-clocked) per-path rate signal, not a missing one

`BtlBw_slow` over one active transfer (sender DIAG time-series):
`1116 → 5837 → 46 → 102 → 780 → 20751 → 20950 → 7000 → 59 → 592 → 5` sym/s — a
**~4000× swing (5 … 20 950) around the true 2 083**, all while `est=Y`. The DAPS
depth budget it feeds, `dbud = skew·BtlBw_slow`, swings **0 → 612** in lock-step
(≈0 at BtlBw 5, 612 at BtlBw 20 950). A depth/pace bound cannot key on a signal
that jumps 4000× per second: half the time `dbud≈0` (the bound is INERT — exactly
§16.14's observed no-op), the other half `dbud` is large enough to not bind. The
CAUSE: on the slow path the BBR send-interval delivery-rate sample measures the
**generation DECODE cadence, not the slow-link drain**. The slow path carries
FUTURE source that is delivered by fungible generation decode (gated by fast-path
DoF arrival / OOO-frontier advance), so `Δdelivered/Δt` alternates between decode
BURSTS (windowed-MAX latches a spike → over-read 20 950) and inter-burst gaps
(sparse samples, the ~1 s min-clamped window decays → under-read 5). The estimator
is measuring the wrong clock, and the windowed-MAX amplifies the burstiness rather
than smoothing it. (The FAST path has the same burstiness — btlbw 13 k–86 k, ×1.3–8
over-read — but its floor stays large enough that its pacer/cap always engage; only
the slow path's swing straddles zero-utility.) The late-tail `est=n` (t≥23.6 s) is
the same defect's other face: once the slow path idles, the sparse samples expire
from the short window and `fill 17→3 (<ANCHOR_MIN_SAMPLES=8)` → the anchor decays.

### VERDICT — FIXABLE (estimator-stability bug), NOT fundamental channel starvation

The channel is fine: the slow link carries real source (3 444 symbols), its RTprop
stays pinned at the 41 ms propagation base (never polluted on the sender side), and
its ~230 sym/s delivered share is genuine. What is broken is the per-path rate
SIGNAL — it is present but unusably NOISY because it is derived from decode-clocked
source-seq attribution through a windowed-MAX. This is a source-side estimator
design bug, not "a channel/CC property no source-side scheduler can synthesize"
(§16.14). The §16.14 CONSOLIDATE call rested on (a) the wrong log and (b)
generation-off inert mechanisms; on the correct sender log with generation on, the
anchor establishes and the real residual is signal STABILITY.

**Specific minimal fix (recommended next build — coordinator's call to build):**
De-noise `BtlBw_slow`. Replace the short windowed-MAX of per-symbol decode-clocked
samples with a rate estimate that is (i) robust to decode bursts and (ii) clocked
by the slow link, in rough order of preference: **(1)** widen/robustify the
per-path filter — use an EWMA or a high-quantile of the per-path DELIVERED rate
over a multi-RTprop horizon (and raise the windowed-MAX min-window floor well above
the slow path's decode-burst spacing, which the current `10·RTprop` clamp-to-1 s
undershoots); **(2)** measure the per-path rate from the slow link's ACTUAL wire
traffic — the coded/repair symbols physically sent on p1 and their per-path
ack/delivery over the send interval (steadily link-clocked), instead of the
fungible source-seq decode attribution; **(3)** seed a conservative per-path BtlBw
prior (configured/probed link rate) so `dbud` is never 0/garbage and the anchor
only refines it. Expected C8 effect: with a STABLE `BtlBw_slow`, the DAPS
depth-bound (`skew·BtlBw_slow`, oracle-proven to reach ×1.195 in §16.14 PART 6h but
INERT there because `dbud` swung through 0) can finally bind consistently — at
minimum stopping the negative aggregation (dual C8 8.4–10.5 < fast-alone 16.45) and
recovering toward the fast-alone floor, with upside toward the oracle ceiling IF
the slow link's ~3.2 Mbit sustains under decode coupling. That last residual — is
the slow marginal rate real throughput once stably paced, or latency-dominated — is
the genuinely-open question, and it can only be answered AFTER the signal is stable
(so it is not yet grounds to consolidate).

### Controls

DIAG-only change (counters + one DIAG print field); the guard split in
`rs_on_delivered` (combined `interval<MinRTT || delivered==0` → two counted `if`s)
is behaviourally identical (same early returns). `cargo test -p raptorpath --lib`
green + gate_suite 15/15. Shipped non-generation default byte-identical (the
counters only increment inside the `rate_sample` path; the DIAG line is
`RWM_DIAG`-gated). Reproduce: `RWM_DAPS=1 RWM_GEN_R=0.03 RWM_RATE_SAMPLE=1
RWM_DAPS_DEPTH=1 RWM_DIAG=1 RWM_EXTRA=--window-generation-coding SEED=42 bash
perf_rwm_c.sh c2 c3 bulk 12000000 3 dual`, then read the p1 `ANCHOR …` counters in
`/tmp/rwm-c.log` (the CLIENT/sender log, NOT `/tmp/rwm-s.log`).

## Generation-ON Re-Baseline (2026-07-13) — the FIRST VALID heterogeneous C8 measurement: the arc's coded path was DEAD in measurement (PROVEN for §16.14; §16.10–16.13 UNVERIFIABLE — no recorded env; the harness never enabled generation by itself); ceilings + C8/C7 re-measured with generation ACTUALLY ON, and a hard guard so the class of bug cannot recur (branch `feat/gen-on-rebaseline`)

*Decision record: → [ADR-0053](adr/0053-generation-inert-era-audit.md)*

> **CRITICAL — the entire recent arc is SUSPECT.** The §16.10–16.14 goal-gate
> results (DAPS, pace-all §16.11, source-backpressure §16.12, rate-sample §16.13,
> read-ahead depth §16.14) and their paper entries were measured on a binary
> running the **coded/generation path DEAD**. DAPS + the per-path estimator +
> rate-sample + the depth bound + source-backpressure ALL gate on `generation`
> (`daps = RWM_DAPS && generation`, `per_path_est = generation && …`,
> `rate_sample = per_path_est && …`, `daps_depth_on = rate_sample && …`), and the
> harness never turned generation on — so every one of those mechanisms was INERT
> in the very measurements that "evaluated" it. Verdicts overturned below.

### THE METHODOLOGY BUG (code evidence)

`perf_rwm_c.sh` passed only `--window-reliable` to both the server and client.
`net/mod.rs:701-702` gates `window_generation` on
`window_reliable && (window_generation_coding || window_systematic_repair || fmtcp)`.
The `RWM_DAPS` / `RWM_GEN_R` / `RWM_RATE_SAMPLE` / `RWM_DAPS_DEPTH` envs only
*configure* generation; they do NOT *enable* it. So the arc's C8 arms ran PLAIN
`--window-reliable` block/ARQ mode with the whole DAPS+estimator+rate-sample+depth
stack switched off — confirmed by the §16.14 saved sender logs (`cod=0`,
`eff_pace=0`). The §16.14 diagnosis (previous section) proved this end-to-end.

### THE FIX + HARD ANTI-REGRESSION GUARD

- `perf_rwm_c.sh` now adds `--window-generation-coding` to BOTH the server and the
  client, DEFAULT ON, gated by `RWM_GEN` (`=0` → the plain-window control). The
  name-collision with the binary's generation-SIZE `RWM_GEN` is handled: the gate
  sentinels `0`/`1` are not forwarded as a size (=1 would set a 1-symbol generation).
- **HARD SANITY GUARD:** after each run the SENDER log (`--client` =
  `/tmp/rwm-c.log` — NOT the `--server`/receiver `/tmp/rwm-s.log`, the §16.14
  wrong-log trap) is parsed for cumulative `total_coded`; if it is 0 when
  generation was requested the run prints `FATAL: generation requested but cod=0
  (mechanism inert)` and exits non-zero. A measurement where the mechanism under
  test did not run now FAILS LOUDLY instead of silently reporting a number.
  Validated: the guard reports `GUARD OK` (coded 178 k–191 k symbols) on EVERY arm
  of the battery below.

### THE FIRST-VALID CEILINGS + C8/C7 (generation ON, current-main stack `RWM_DAPS=1 RWM_GEN_R=0.03 RWM_RATE_SAMPLE=1 RWM_DAPS_DEPTH=1`, 25 MB × 8, seeds 42 & 7, VM 10.1.5.16, dnf=0 every arm, GUARD OK every arm)

**Ceilings (re-measured with generation ON — the old §16.14 ceilings are from a generation-inert binary):**

| single arm | seed42 mean Mbit/s (σ_s) [min→max run] | seed7 mean (σ_s) [min→max] | pooled |
|---|---|---|---:|
| single-c2 (FAST) | 13.90 (0.97) [12.43→15.20] | 14.09 (0.57) [13.34→15.14] | **13.99** |
| single-c3 (SLOW) | 3.03 (4.01) [2.83→3.49] | 3.06 (6.03) [2.73→3.49] | **3.04** |
| **recovery ceiling** (fast+slow) | 16.93 | 17.15 | **17.03** |

Note the fast single-path ceiling is LOWER than §16.14's generation-inert 16.45 —
generation coding carries a single-path throughput tax (~15 %, the coding overhead +
decode latency). The §16.14 ceilings were not comparable (different binary).

**C8 (c2+c3) heterogeneous — per seed, mean Mbit/s (σ_s) [per-run distribution]:**

| seed | C8 mean (σ_s) | per-run Mbit/s | ×fast (÷13.99) | eff ÷ceiling |
|---|---|---|---:|---:|
| 42 | 9.97 (3.43) | 7.76 8.01 9.70 11.63 11.16 11.18 11.04 11.07 | 0.72× | 0.59 |
| 7 | 13.52 (0.67) | 13.26 13.51 14.86 12.92 13.28 13.19 13.20 14.17 | 0.96× | 0.79 |
| **pooled** | **~11.74** | (seed42 shows a clear warm-up ramp 7.8→11.1) | **~0.84×** | **~0.69** |

**C7 (c2+c2) symmetric — per seed:**

| seed | C7 mean (σ_s) | per-run Mbit/s | ×fast | eff ÷2×fast |
|---|---|---|---:|---:|
| 42 | 12.05 (1.41) | 10.20 12.56 12.42 11.24 12.88 12.49 12.29 12.85 | 0.87× | 0.43 |
| 7 | 12.59 (1.02) | 13.66 12.06 11.38 12.49 13.25 13.28 13.05 11.95 | 0.89× | 0.45 |

### DOES IT AGGREGATE? — NO (still ≤ fast-alone), but FAR above the inert §16.14 bound; and a NEW finding: even SYMMETRIC C7 is below fast-alone

- **C8 does NOT exceed fast-alone** on either seed (0.72× / 0.96×) — so heterogeneous
  bulk aggregation is still not landed. BUT this is a very different picture from the
  inert §16.14 "0.51× / 8.40 Mbit/s": with generation genuinely ON, C8 is 11.74 pooled
  and reaches **0.96× (parity) on seed7**. The §16.14 quantitative bound (0.51×) and
  its "channel starvation no source-side scheduler can fix" framing are overturned;
  what remains is a much narrower parity-vs-slightly-below gap, and it is SEED-UNSTABLE
  (0.72 vs 0.96, seed42 σ_s 3.43 with a multi-run warm-up ramp) → STEP 3 warranted.
- **NEW, load-bearing finding: C7 SYMMETRIC (c2+c2) is ALSO below fast-alone
  (0.87–0.89×).** The depth bound is a provable no-op on symmetric skew (skew 0 ⇒ no
  depth budget), so this penalty is INDEPENDENT of the slow-anchor de-noise: there is
  a residual dual-path **generation-mode** throughput tax (coding overhead + cross-path
  reassembly/decode coupling) that caps even two identical paths below one of them.
  This bounds what any slow-anchor fix can buy for C8, and is the honest new open item.

### OVERTURNED / SUSPECT VERDICTS

- **§16.14 "slow anchor NEVER establishes (`est=n`/`btlbw=0`/`dbud=0`), CONSOLIDATE,
  C8 structurally bounded at 0.51× fast":** overturned. The anchor DOES establish
  (already shown by the diagnosis); the §16.14 numbers were generation-inert; gen-ON
  C8 is 0.84× pooled / 0.96× seed7, not 0.51×.
- **§16.11 pace-all, §16.12 source-backpressure (REFUTED), §16.13 rate-sample
  (the "C8 regresses" finding):** all measured generation-inert — the A/B arms compared
  binaries with the coded path DEAD, so those mechanism verdicts are SUSPECT and are not
  validly established. They must be re-run generation-ON before any conclusion stands.
- **§16.14 C7 "20 % is pure VM noise":** the actual gen-ON C7 is 12.3 Mbit/s (0.88× fast),
  a different regime; the old C7 numbers (17–21) were the inert block mode.

### STEP 3 — De-noise BtlBw_slow (robust quantile): REFUTED at L1 — the generation-mode rate samples are decode-clocked, so the windowed-MAX is near-correct and ANY sub-max quantile UNDER-reads and throttles the path (same-binary `RWM_RATE_WIRE` A/B)

Because STEP 2 left C8 ≤ fast-alone and seed-unstable, the diagnosed de-noise was built
and A/B-tested. It is gated `RWM_RATE_WIRE` (robust quantile `RWM_RATE_Q`, default median,
of the per-path delivered-rate samples for the DAPS pace/offset/depth signal; the cwnd
anchor untouched), DEFAULT OFF ⇒ byte-identical.

**Same-binary A/B (seed42, C8 het):**

| arm | C8 Mbit/s | fast-path anchor `btlbw` (true ≈10 400 sym/s) | slow-path |
|---|---:|---|---|
| **OFF** (`RATE_WIRE=0`, = STEP 2 stack, 25 MB × 8) | **7.80** (σ_s 2.79) | max-filter (correct) | est=Y |
| ON (`RATE_WIRE=1` median, 6 MB × 3) | **~1.3** | **159** (65× UNDER-read) | starved sinfl=3, est=n |
| ON (`RATE_WIRE=1` q=0.9, 6 MB × 3) | ~2.7 (6.1→3.2→1.6) | 198 | starved |

**The de-noise REGRESSES C8 3–6×.** Mechanism (the decisive finding): in generation mode
the per-path delivery-rate samples are **decode-FRONTIER-clocked** — a source seq is
"delivered" when the OOO frontier passes it (fungible decode), which advances in BURSTS with
long inter-decode gaps — so the sample distribution is **mostly LOW with the true link rate
at the burst-peak TOP**. The windowed-**MAX is therefore the near-correct recovery statistic**
(it grabs the peak ≈ true rate); ANY sub-max quantile (median → 159, even p90 → 198) lands
in the low cluster and under-reads the fast path ~65×, collapsing the DAPS pace bucket →
throughput collapse. The §16.15-diagnosis "over-read to 20 950" is a rare UPPER-TAIL spike,
not the bulk — rejecting the top removes the signal itself. (First implementation also
hard-STALLED (dnf): `btlbw_sym_per_s` is read once per send-loop iteration by the DAPS pacer,
so an O(n log n) quantile sort there made the sender CPU-bound; fixed by caching the quantile
once per delivered sample — the throughput-collapse result above is the corrected, cached
version, so the regression is the mechanism, not the perf bug.)

**Verdict:** the robust-quantile de-noise is **REFUTED at L1** — the correct fix is NOT a
filter over the decode-clocked samples but the task's option (2): measure per-path rate from
the path's OWN WIRE acks (link-clocked source+coded ACKed on path i), which the current
attribution (decode-frontier source-seq) does not do — a larger change, DEFERRED. The knob
ships gated OFF (byte-identical), unit-tested, and oracle-modelled (PART 6i: a STABLE anchor
WOULD reach the ×1.195 depth-bound ceiling — but the quantile does not PRODUCE a stable
anchor, it under-reads). Combined with the STEP 2 finding that even symmetric C7 is below
fast-alone, the honest standing conclusion is that gen-ON heterogeneous C8 is at
parity-to-slightly-below fast-alone, and the residual is BOTH the decode-clocked rate signal
(needs a wire-clocked estimator, not a filter) AND a symmetric dual-path coding tax.

### Controls / tests

`cargo test -p raptorpath --lib` 293/293 (1 new: `robust_btlbw_rejects_the_decode_
burst_over_read_latch`); `-p raptorpath-math` 23/23 (1 new oracle: PART 6i
`anchor_noise_makes_depth_bound_inert_stable_anchor_restores_it`); gate_suite 15/15
release. The de-noise (`RWM_RATE_WIRE`, robust per-path rate for the DAPS pace/offset/
depth signal) is DEFAULT OFF ⇒ `effective_btlbw == max_bw` ⇒ byte-identical shipped
default; the cwnd recovery anchor (`bdp_anchor`) is untouched. Every battery arm dnf=0.

## Methodology Audit (2026-07-13) — how the generation-inert era happened

*Decision record: → [ADR-0053](adr/0053-generation-inert-era-audit.md)*

Full reports, in-repo verbatim:
[audits/2026-07-13-verdict-audit.md](audits/2026-07-13-verdict-audit.md)
(section-by-section VALID/INVALID/UNCERTAIN classification, the env-gate table,
the baseline non-reconciliation, the tainted-paper-claims list) and
[audits/2026-07-13-session-audit.md](audits/2026-07-13-session-audit.md)
(the full session error audit, top-10 misses, systemic patterns). The
per-section status banners above quote these classifications.

Summary of findings:

- **The harness never enabled generation.** `perf_rwm_c.sh` passed only
  `--window-reliable`; `window_generation = window_reliable &&
  (window_generation_coding || window_systematic_repair || fmtcp)` — `RWM_DAPS`
  does not appear in that gate and never has. Every DAPS-era mechanism
  (`daps → daps_pace_on → {pace_all_on, src_bp_on}`; `per_path_est →
  rate_sample → daps_depth_on`) chains off `generation`, so one false at the
  root made every A/B toggle compare byte-identical behaviour against itself.
- **Env `=0` counted as ON.** `RWM_FMTCP` and `RWM_DAPS` are `.is_ok()` gates:
  `RWM_FMTCP=0` still enables generation and `RWM_DAPS=0` still counts as set.
- **The DAPS-era ledger sections recorded no command lines/env** (unlike the
  systematic-repair-era sections, which recorded
  `RWM_EXTRA="--window-systematic-repair"`). That absence is the central
  ledger-discipline failure: it is why §16.10–16.13 can only be classified
  UNCERTAIN rather than retro-validated or definitively voided. §16.14 alone
  is INVALID-proven, because its saved sender logs show `cod=0`/`eff_pace=0`.
- **Era noise exceeded every claimed effect.** The same nominal config
  measured 14.99 → 10.74 → 6.50 Mbit/s across three sessions (2.3× spread);
  plain window-reliable dual C8 is heavy-tailed/bimodal (mean 5.43, σ_s 11.7,
  single runs ~2–16 Mbit/s). Every claimed DAPS-era delta (+15%, +30%, +52%,
  −53%, −19%, the §16.14 arm ordering) fits inside that spread.
- **§16.14 read the wrong log.** Its mechanism DIAG ("est=n/btlbw=0
  throughout") came from `/tmp/rwm-s.log` — the perf `--server`, i.e. the
  RECEIVER — not the bulk sender (`--client`, `/tmp/rwm-c.log`). The follow-up
  sender-log diagnosis showed the anchor DOES establish.
- **The correction:** (1) the harness now passes `--window-generation-coding`
  to server and client by default, gated `RWM_GEN` (`=0` = the plain-window
  control); (2) a HARD GUARD fails any generation-requested run whose sender
  log shows cumulative `total_coded = 0` (`FATAL: … mechanism inert`);
  (3) the "Generation-ON Re-Baseline (2026-07-13)" section records the first
  valid generation-ON ceilings + C8/C7. The MEASUREMENT DISCIPLINE checklist
  at the top of this file is now binding for every future L1 verdict.

## Gen-ON Stack Ablation (2026-07-13) — FIRST ablation of the DAPS-era stack with generation actually ON: the symmetric C7 collapse 21→12 is the STACK (rate-sample −22%, depth −20…−30%), NOT the coding (gen-bare ≈ plain, keeps ×1.35 aggregation); gen-mode C8 is SUBSTRATE-bound (per-path ≈10 Mbit/s generation ceiling), and PLAIN C8 beats every gen arm ×1.6 same-day; plus the env-parse footgun fixed (`RWM_*=0` now truly OFF) (branch `feat/gen-on-ablation`)

*Decision record: → [ADR-0065](adr/0065-daps-era-refutations.md) (+ [ADR-0053](adr/0053-generation-inert-era-audit.md))*

The §16.15 re-baseline left an open attribution: with generation ON, symmetric C7
collapsed 21→12 (0.88× fast-alone). Was that (a) generation coding's intrinsic
coordination cost, (b) the DAPS-era stack live for the FIRST time and actively
harmful, or (c) split? Nobody had ever measured gen-ON *bare*. This section answers
it with a five-arm same-binary ablation, both topologies, both seeds, interleaved.

### JOB 1 — the env-parse fix (prerequisite: "explicitly OFF" arms were inexpressible)

Every `RWM_*` boolean gate that used `std::env::var(..).is_ok()` counted `=0` (and
`=false`) as ON — `RWM_FMTCP=0` *enabled* generation (net/mod.rs:699), `RWM_DAPS=0`
counted as set (net/mod.rs:3317) — the verdict-audit footgun. ONE helper now parses
every boolean gate, `config::env_flag(name, default)`: unset → shipped default;
`""`/`"0"`/`"false"` (case-insensitive, trimmed) → OFF; `"1"`/anything else → ON.
Converted 21 gates in net/mod.rs (`RWM_FMTCP`×2, `RWM_DAPS`, `RWM_SACK_PRUNE`,
`RWM_REASM_BDP`, `RWM_FDIAG`, `RWM_TRACE`×2, `RWM_DAPS_PACE`, `RWM_PACE_ALL`,
`RWM_SRC_BP`, `RWM_PER_PATH_EST`, `RWM_RATE_SAMPLE`, `RWM_DAPS_DEPTH`,
`RWM_CC_PACE`, `RWM_OOO_RETAIN`, `RWM_INLINE_REPAIR`, `RWM_PROACTIVE_PACER`,
`RWM_XPATH_REPAIR`, `RWM_DIAG`, `RWM_CODED_SRC`, `RWM_NO_REACTIVE`, `RWM_PFRAC`)
+ `RWM_RATE_WIRE` (scheduler/mod.rs). Numeric-VALUE knobs (`RWM_GEN_R`, `RWM_STORE`,
`RWM_FRONTIER*`, …) untouched — 0 is a legitimate value there. Shipped defaults for
UNSET are identical everywhere; behaviour changes only for anyone who passed `=0`
expecting OFF — which is the fix. 3 unit tests (`config::env_flag_tests`). Verified
at the runtime surface: `RWM_FMTCP=0` + plain run → 0 coded lines (pre-fix it
self-enabled generation).

### JOB 2 — method (VM 10.1.5.16, dual netns, same binary, interleaved, guard-verified)

Arms (r=0.03 for all gen arms; after JOB 1, `=0` truly means OFF):

| arm | env | meaning |
|---|---|---|
| P | `RWM_GEN=0` | plain window-reliable control (harness gate; no `--window-generation-coding`) |
| G0 | `RWM_GEN_R=0.03` | generation ON, BARE — no DAPS, no per-path est, no rate-sample, no depth |
| G1 | + `RWM_DAPS=1 RWM_RATE_SAMPLE=0 RWM_DAPS_DEPTH=0` | + DAPS (legacy ack-interval anchor) |
| G2 | + `RWM_DAPS=1 RWM_RATE_SAMPLE=1 RWM_DAPS_DEPTH=0` | + BBR rate-sample estimator chain |
| G3 | + `RWM_DAPS=1 RWM_RATE_SAMPLE=1 RWM_DAPS_DEPTH=1` | FULL stack = the §16.15/161aff1 config |

25 MB × 1 run per invocation × 8 reps, arm order P,G0,G1,G2,G3 round-robin per rep
(cancels session drift *within* each battery — the §16.14 lesson), seeds 42 AND 7,
C7 (c2+c2) and C8 (c2+c3), fresh tunnel per invocation, hard timeouts, `cod>0`
GUARD asserted on the SENDER log for every gen-arm run (GUARD OK on ALL of them;
plain arms have no guard by construction). Singles (25 MB × 8, seed 42) once per
mode for the ceilings. Runtimes: build 3 min; c7s42 10 min; c7s7 12.5 min; singles
46 min; c8s42 21 min; c8s7 17 min (~2 h total). Full logs on the VM under
`/home/vibe/ablation/results-*.log`.

Two harness caveats, recorded honestly: (1) `topo_dual.sh`'s verification `ping -c 2`
runs under `set -e` and seed-7's GE loss occasionally eats both echoes → instant
pre-measurement abort; the driver retries ×3 per invocation (31 retries C7/19 C8 at
seed 7; 2 reps lost in c7s7 — G0 n=7, G3 n=7 — and 2 in c8s7 — G0 n=6). (2) Because
every rep is a fresh 1-run invocation, each sample is a "run-1" (cold engine); the
§16.15 battery ran 8 warm runs per invocation, so LEVELS here sit slightly below
§16.15's for the same config (G3 C8 9.27/10.80 vs 9.97/13.52) — cross-arm
comparisons within this battery are unaffected (that is what interleaving buys).

### C7 (c2+c2) SYMMETRIC — the tax attribution table

| arm | seed42 mean (σ_s) [runs] | seed7 mean (σ_s) [runs] | pooled | ×fast-alone (15.15) |
|---|---|---|---:|---:|
| P | 20.47 (1.20) [19.28 20.96 22.03 18.25 20.40 20.89 20.58 21.37] | 25.09 (0.82) [26.15 25.24 23.87 24.52 24.32 25.46 25.10 26.10] | **22.78** | **1.50×** |
| G0 | 20.83 (0.81) [20.63 21.71 19.41 20.11 20.85 21.89 20.86 21.15] | 20.54 (2.28, n=7) [22.26 21.10 20.41 15.61 21.37 22.26 20.76] | **20.70** | **1.37×** |
| G1 | 20.22 (0.35) [19.80 20.57 20.64 20.11 20.17 20.62 20.01 19.84] | 20.64 (0.23) [20.44 21.10 20.61 20.85 20.49 20.47 20.45 20.71] | 20.43 | 1.35× |
| G2 | 15.55 (0.66) [16.33 15.40 14.20 15.48 15.40 15.43 16.09 16.04] | 16.16 (1.19) [14.51 16.66 14.91 18.32 15.54 16.31 16.42 16.63] | 15.86 | 1.05× |
| G3 | 12.12 (2.18) [8.27 10.43 11.75 13.91 13.90 13.37 10.77 14.53] | 11.29 (2.33, n=7) [8.03 13.00 8.08 12.35 13.36 11.14 13.06] | **11.73** | **0.77×** |

**Attribution (the answer to the §16.15 open question): it is (b) — the stack.**
- **Coding intrinsic (P→G0): ~0 to −18 %, seed-dependent** (s42 +0.4, within noise;
  s7 −4.6). Bare generation KEEPS plain-class aggregation: G0 = 20.7 = ×1.37
  fast-alone — real symmetric aggregation, on par with plain's historic ×1.28–1.35.
- **DAPS placement alone (G0→G1): FREE** (−0.3 pooled, within noise, both seeds).
- **Rate-sample estimator (G1→G2): −22 %** (−4.7 s42 / −4.5 s7 — consistent).
- **Depth bound (G2→G3): −17 % (s42) / −30 % (s7)** — and NOTE: §16.15 claimed depth
  is "a provable no-op on symmetric skew"; it is NOT in practice, because the
  decode-clocked per-path anchors swing so hard that one path always *looks*
  transiently slower, acquires a garbage skew/budget, and gets depth-throttled.
- G3 replicates §16.15's C7 (12.12/11.29 vs 12.05/12.59) — consistency check PASS;
  the whole −45 % symmetric collapse is rate-sample+depth stacked on a free base.

### C8 (c2+c3) HETEROGENEOUS

| arm | seed42 mean (σ_s) [runs] | seed7 mean (σ_s) [runs] | pooled | ×fast-alone (15.15) |
|---|---|---|---:|---:|
| P | 14.99 (2.01) [11.83 16.44 16.65 16.90 12.42 16.55 14.18 14.95] | 14.93 (2.10) [14.60 16.01 14.80 12.95 15.88 16.33 11.11 17.78] | **14.96** | **0.99×** |
| G0 | 9.27 (0.14) [9.24 9.16 9.38 9.15 9.47 9.25 9.42 9.10] | 9.11 (0.48, n=6) [9.16 8.85 8.94 10.04 8.94 8.73] | 9.19 | 0.61× |
| G1 | 8.05 (0.77) [7.13 7.60 8.33 9.17 8.76 7.00 8.47 7.97] | 8.15 (0.68) [8.69 7.31 8.47 8.00 7.71 8.67 7.28 9.07] | 8.10 | 0.53× |
| G2 | 7.33 (0.78, n=7, **1 DNF**) [7.51 6.82 8.81 6.87 7.59 6.42 7.26] | 9.27 (2.13) [9.78 8.05 8.19 8.14 14.32 8.21 8.39 9.07] | 8.30 | 0.55× |
| G3 | 9.27 (1.57) [9.33 7.14 7.36 11.45 10.61 10.24 8.14 9.87] | 10.80 (2.46) [12.29 13.04 12.02 9.19 9.05 14.34 7.18 9.31] | **10.04** | 0.66× |

### Ceilings (singles, 25 MB × 8, seed 42, same day/binary)

| single arm | mean (σ_s) [runs] |
|---|---|
| PLAIN single-c2 | **15.15** (5.26) [17.27 14.86 17.50 15.40 18.74 16.54 **2.56** 18.35] — median 16.90; one bimodal-low run (the known plain tail) |
| GEN-BARE single-c2 | **9.70** (0.32) [9.72 9.02 9.63 9.64 9.60 9.98 10.02 9.96] — GUARD OK |
| PLAIN single-c3 | **3.31** (0.15) [3.24 3.16 3.34 3.28 3.57 3.23 3.50 3.15] |
| GEN-BARE single-c3 | **COLLAPSED**: run1 0.78 Mbit/s then DNF (GUARD OK, 764 k coded — generation ran). Bare gen at r=0.03 is NOT viable on the lossy slow path alone; §16.15's full-stack 3.04 needed the DAPS-deepened window. |

### THE LOAD-BEARING STRUCTURAL FINDING — gen-mode C8 is SUBSTRATE-bound, not scheduling-bound

Line up three numbers: gen-bare single-c2 **9.70** (σ 0.32) · gen-bare C8 **9.27/9.11**
(σ 0.14/0.48) · gen-bare C7 **20.7** (≈ ×2.15 its own single). In generation mode each
path delivers ≈10 Mbit/s regardless of what the link can carry (plain c2 does 15–17
alone): the binder is the generation pipeline itself (the (pipeline+2)·G in-flight
window / window-fill decode serialization — on C7 both paths fill generations in
parallel and per-path throughput stays ~10.3; single-path and C8-fast-path hit the same
~9.7–10 wall, and the slow path adds ≈0 net). That is why gen C8 sits pinned at
9.2 ± 0.1 — it IS the substrate ceiling (0.95× of gen's own fast-alone), and why no
DAPS-era scheduling lever could ever have lifted it. It also explains §16.15's
"single-path coding tax": bare gen single is 9.70 (−36 % vs plain); the full stack's
13.99 was DAPS's deeper (pipeline+6)·G read-ahead partially relieving the window
serialization ON SINGLE PATH — the same knobs that tax duals.

### Best-achievable C8 today + does ANYTHING beat fast-alone? — NO

- **Best absolute C8 config: PLAIN window-reliable, 14.96 pooled** — 0.99× same-day
  fast-alone mean (0.89× its 16.90 median): parity, NOT aggregation. Every gen arm is
  ×1.5–1.8 below plain on C8 same-day. (Today's plain C8 never collapsed — min 11.1
  across 16 runs; the historical bimodal 5–8.7 did not manifest in these sessions.)
- **Best GEN C8: G3 (full stack), 10.04 pooled** (s7 10.80) — the depth bound DOES
  help hetero gen mode (+0.85 pooled vs G0, +1.7 on s7), the one place the stack
  earns anything — but it starts 0.61× down and lands at 0.66×.
- **Stability: generation is the stabilizer.** G0 C8 σ_s 0.14/0.48 vs plain's 2.01/2.10
  same-day (and 5–15 Mbit/s bimodal across the historical record). Gen-bare C8 is the
  most repeatable multipath number ever measured on this rig — at a −38 % mean cost.
- Efficiency vs recovery ceilings: plain C8 14.96 / (15.15+3.31=18.46) = **0.81**;
  gen-bare C8 9.19 / its own substrate ceiling ≈9.7+ε = **0.95** (the gen pipeline is
  nearly perfectly utilized — there is simply less pipe).

### VERDICT + the recommended next lever

1. **The DAPS-era stack should not ship ON under generation.** Rate-sample (−22 % C7)
   and depth (−17…−30 % C7, +8 % C8) are a bad trade; DAPS placement alone is free but
   buys nothing measurable (G1 ≤ G0 on C8 too). If generation ships, ship it BARE for
   symmetric/unknown topologies; the depth bound is only defensible as a
   hetero-C8-specific opt-in.
2. **The next lever is the SUBSTRATE, not the scheduler: raise the per-path ~10 Mbit/s
   generation ceiling.** Candidates, in the order the data points: (i) deepen/overlap
   the generation pipeline on single/few-path configs (DAPS's window-floor already
   proves +44 % single-path headroom exists — decouple that from the harmful pacer
   levers); (ii) reduce decode/window-fill serialization (systematic-source submode so
   source rides the wire un-decode-gated); (iii) only then re-visit a wire-clocked
   per-path estimator (§16.15's deferred option 2) — a stable anchor is worthless
   while the substrate caps ×0.64 below the link.
3. **Honest C8 position: still no aggregation** — plain = parity-with-fast-alone
   (0.99×/0.89×), gen = substrate-capped 0.61–0.66×. The §16.15 "parity on seed7
   (0.96×)" was measured against gen's own depressed 13.99 ceiling; against the LINK
   (plain fast-alone) nothing has ever exceeded 1.0 on C8.

### Controls / tests

`cargo test -p raptorpath --lib` 296/296 (3 new `env_flag` tests); `-p raptorpath-math`
47/47; gate_suite 15/15 release — env-parse fix does not break gate defaults (all
defaults-for-unset preserved; A/B'd at the runtime surface with `RWM_FMTCP=0`).
dnf=0 in every arm except the two recorded gen-substrate DNFs (G2 c8s42 ×1;
G0 single-c3 — both reported above, not hidden). Env + command line recorded per arm
in the battery logs; every gen-arm run GUARD-verified `cod>0` on the SENDER log.

## Gen Substrate Ceiling (2026-07-13) — the ~10 Mbit/s per-path generation wall NAMED and RAISED: the binder is quinn's loss-reactive CUBIC underneath the datagram path × generation-mode's own standing-queue RTT inflation; `RWM_QUIC_CC=bbr` alone ×3.4 (9.77→33), `RWM_GEN_PIPE` app fix alone 9.77→14.3, together 33.8; single-c3 bare collapse 0.78→13.0 FIXED; C8 gen 32.3 = ×1.9 the plain fast-alone link control — and the SAME lever exposes that plain's own 15–17 "link ceiling" was the substrate too (plain+BBR single 76, C7 94.6) (branch `feat/gen-substrate-ceiling`)

*Decision record: → [ADR-0054](adr/0054-substrate-cc-policy-bbr-default.md)*

§16.16 ended pointing at one lever: gen mode has a ~10 Mbit/s PER-PATH substrate
ceiling (gen-bare single-c2 9.70 vs plain 15.15; gen C8 pinned at 0.95× of gen's own
single; C7 = ×2.15 of gen's own single ⇒ per-path wall). This section names the
binding stage with instrument numbers, builds the principled fix, and re-measures.

### JOB 1 — diagnosis: L0 first, then the instrumented L1 run

**New instruments (all default-OFF / DIAG-gated; shipped path byte-identical):**
(1) an L0 netem shim in the QUIC transport (`RWM_L0_NETEM=c2|c3|c2,c3|custom:…`,
src/transport/quic.rs) that reproduces the L1 per-path rate+delay+jitter+GE-loss
INSIDE the datagram send path, so the in-process loopback bench
(tests/gen_substrate_l0.rs, `#[ignore]`) runs the full engine under c2/c3 shaping
locally — crucially the shim drops/delays BEFORE quinn, so quinn's own congestion
controller still sees a clean loopback (that asymmetry is the diagnostic);
(2) GDIAG/GLIFE sender stall attribution under `RWM_DIAG` — per-250 ms time-weighted
split of the generation data plane across its gates
[emit/budget/fill/target/tokens/cwnd] + per-generation lifecycle
(fill→code→ack-wait ms); (3) `RWM_QUIC_CC=bbr|newreno|cubic` — quinn
congestion-controller override (quinn gates EVERY send, datagrams included, on its
congestion window; default Cubic — quinn-proto connection/mod.rs "blocked by
congestion control").

**L0 result (Windows loopback, shim = c2 params, gen-bare r=0.03, 12.5 MB):** the
app-level generation machine does **34.0 Mbit/s** — NOT 9.7 — and plain does 67.
Same knobs, same RTT/loss/rate. c3-shim gen-bare: 12.2, no collapse. ⇒ the L1 wall
is NOT the app pipeline (window-fill/decode serialization exists but binds ~34);
it lives in what the shim bypasses: quinn seeing the lossy link.

**L1 instrumented run (gen-bare single-c2, RWM_DIAG=1, seed 42 — the wall
reproduces at 9.69):** the sender's own gates are OPEN most of the time
(stall: emit 46–80%, budget/ack-wait 17–67%, tokens/target ≈ 0) while
**rtt = 312–802 ms vs rtprop 12–41 ms** — a multi-second standing queue between
the app pacer and the wire (our emission enqueues into quinn's 4 MB datagram buffer
faster than quinn's loss-collapsed Cubic window drains it) — and each generation's
coded phase takes **410–874 ms** (GLIFE), with eff_pace pinned at the 2000 sym/s
floor (the decode-clocked EWMA decays between generation acks). Total coded:
38 280 emitted for ~21 800 needed = **1.76× waste** (deficit re-sends at the
bloated RTT). So THE stage: **quinn's loss-reactive Cubic is the per-path
substrate ceiling, and bare generation mode makes it worse by bufferbloating the
RTT Cubic's throughput divides by.** Per-connection = per-path — exactly why C7
scaled ×2 while single/C8-fast hit the same wall, and why no app-level scheduler
lever could ever lift it.

### JOB 2 — the fix (two orthogonal, env-gated levers; no new magic constants)

1. **`RWM_QUIC_CC=bbr` (substrate):** a loss-tolerant FEC transport must not ride a
   hidden loss-reactive CC underneath its own CC; BBR is quinn's model-based
   (delivery-rate) controller, still congestion-safe at the bottleneck. Default
   UNSET = stock Cubic, byte-identical.
2. **`RWM_GEN_PIPE=1` (app, generation-gated, default OFF):** composes
   (a) the per-path BDP in-flight cap (gain 1.5, the existing FMTCP mechanism) so
   the standing queue — and the RTT any substrate CC sees — stays ≈ RTprop;
   (b) DERIVED pipeline depth **M\* = ceil(rate·2·RTprop/G)+1** (task #61's
   A\* = clamp(D·rate, 1, W) quantized to generations; `gen_pipe_depth`,
   net/mod.rs), recomputed every 5 ms from the windowed-MAX delivered rate
   (§16.15's statistic — the decode-clocked samples are mostly-low) and RTprop
   (min-RTT, NOT the self-inflated live SRTT — the BBR discipline), clamped
   [2, 32]; drives the encoder round-robin span (`set_pipeline_depth`), the
   intake cap M\*·G, retention, the receiver reassembly span, and the deficit
   report width; (c) coded budget clocked on the SENT frontier (the stalled
   cumulative ack must not freeze fresh generations' provisioning);
   (d) pace = windowed-max × 1.25 (BBR probe gain; wire must fund
   (1+r)/(1−ε) ≈ 1.08× delivered + ramp) instead of the decaying EWMA × 1.5;
   (e) once-per-SRTT deficit action (react_cap 1.0, the FMTCP bound).
   Constants audit: 1.5 (BDP gain) and 1.0 (react spacing) inherited from the
   FMTCP arm; 1.25 = BBR probe gain; 2 s rate bucket derived from the ack-burst
   quantum (bucket ≥ 4·G/R for ≤25 % quantization); 32 = memory backstop.

### JOB 3 — L1 A/B (VM 10.1.5.16, 2026-07-13, ~06:39–08:30 UTC; binary = this branch tree as committed in 0d9f26e (source files identical; docs finalized after the runs);
25 MB × 1 run/invocation × 8 reps, arms interleaved round-robin per rep, fresh
tunnel each invocation, seeds 42 AND 7, r=0.03 on every gen arm, `cod>0` GUARD OK
on every gen run, full env+command per run in `/home/vibe/gensub/*.log`; driver
`/home/vibe/gensub_battery.sh`)

Arms: P = plain (`RWM_GEN=0`) · PB = plain+`RWM_QUIC_CC=bbr` · G0 = gen-bare
(`RWM_GEN_R=0.03`) · GP = +`RWM_GEN_PIPE=1` · GB = +`RWM_QUIC_CC=bbr` ·
GPB = both.

**single-c2 (the PRIMARY; target was ≥14):**

| arm | seed42 mean (σ_s) [runs] | seed7 mean (σ_s) [runs] | pooled |
|---|---|---|---:|
| P | 17.01 (0.31) [17.3 16.5 17.0 16.7 17.1 17.0 17.4 17.0] | 18.94 (0.34, n=3) [18.7 18.8 19.3] | 17.5 |
| PB | **76.06** (2.01) [77.8 72.9 79.5 75.6 75.4 75.7 74.6 76.9] | **72.62** (5.11, n=7) [73.2 74.2 74.4 75.1 77.2 61.5 72.9] | **74.5** |
| G0 | 9.77 (0.16) [9.7 10.1 9.6 9.9 9.8 9.8 9.7 9.6] | 9.87 (0.31) [9.5 9.9 9.8 10.2 9.5 9.8 9.9 10.4] | 9.82 |
| GP | 14.33 (0.37) [14.6 13.8 14.2 14.1 14.3 14.2 14.3 15.1] | 14.63 (0.39, n=7) [15.3 14.6 14.7 14.1 14.8 14.6 14.2] | 14.5 |
| GB | 32.91 (1.28) [34.4 32.1 32.4 34.3 30.4 32.9 33.4 33.3] | 33.66 (1.01) [34.5 34.2 33.4 34.6 31.5 33.3 33.7 34.1] | 33.3 |
| GPB | **33.83** (1.11) [34.1 33.1 34.6 34.4 31.6 34.3 33.4 35.2] | **34.11** (1.05, n=7, **1 DNF**) [34.3 33.3 33.0 33.4 35.5 35.5 33.8] | **33.9** |

G0 replicates §16.16's 9.70 exactly (9.77/9.87). The app fix alone (GP) clears the
≥14 target under the Cubic substrate (+47 %, and coded waste 1.76×→1.15×); the
substrate lever alone (GB) is ×3.4; together 33.9 — which equals the L0
app-machine ceiling (34), i.e. the substrate is FIXED and the next binder is the
app/decode machine. **And the control that reframes the whole arc: plain+BBR = 76
— plain's 15–17 was never the link, it was the same Cubic substrate.** GPB seed7
rep1 was 1 honest DNF (300 s timeout with all data coded — a tail wedge;
1/16 GPB runs).

**single-c3 (the bare-collapse case; §16.16: G0 = 0.78 then DNF; plain 3.31):**

| arm | seed42 mean (σ_s) [runs] | seed7 mean (σ_s) [runs] |
|---|---|---|
| P | 3.20 (0.09) [3.1 3.4 3.2 3.1 3.2 3.2 3.2 3.3] | 3.72 (0.07) [3.7 3.8 3.8 3.8 3.7 3.6 3.6 3.7] |
| PB | 14.14 (4.61, 1 low-run 2.8) [2.8 15.7 16.0 15.3 15.7 15.4 16.2 16.1] | 13.95 (4.75, 1 low-run 2.2) [15.7 2.2 15.8 15.6 15.8 15.2 15.6 15.7] |
| GB | 8.66 (2.72, 1 low-run 2.5, **1 DNF**) [2.5 9.6 9.7 9.7 9.6 9.7 9.9 0.0] | 9.69 (0.06, n=7) [9.7 9.7 9.7 9.7 9.7 9.8 9.6] |
| GPB | **13.00** (0.18) [12.6 13.0 13.1 13.0 13.2 13.1 13.0 13.0] | **12.68** (0.26, n=5) [12.3 12.6 12.8 12.8 13.0] |

The bare collapse (§16.16: 0.78 then DNF) is FIXED: GPB = **13.0/12.7, σ ≤ 0.26,
dnf = 0** on the 20 Mbit lossy path — ~3.9× plain's own 3.2/3.7, and 0.83× of
plain+BBR's ~15.7 link-class. Note GPB > GB (+3.6): on c3 the deficit path is hot
(ε ≈ 4.8 % > r = 0.03), so the app-side queue/reactive discipline earns real
throughput even on the fixed substrate — and GB without it still throws the
gen-bare-class low-run/DNF (seed42). **G0-c3 collapse control on THIS binary
(post-battery, seed 42, ×1): 1.06 Mbit/s (189 s for 25 MB), 232 521 coded emitted
for ~21 800 needed = 10.7× reactive waste** — the §16.16 collapse class
reproduces, so the fix is measured against a live failure, not a stale record.

**Fix-arm mechanism snapshot (GPB single-c2, RWM_DIAG, post-battery, 33.7 Mbit/s):**
rtt 35–100 ms vs G0's 312–802 ms (the standing queue is GONE, ~8×), per-generation
code phase 82–290 ms vs 410–874 ms, stall[emit 70–93 %, budget ≤ 18 %], eff_pace
tracking 3 600–9 000 (no floor-pinning). Caveat recorded: the per-path BtlBw
max-filter still over-reads at L1 under BBR (btlbw 79–98 k sym/s vs true ~10.4 k;
decode-burst spikes), so the 1.5·BDP per-path cap is loose (~1 600 sym) — the
queue discipline is carried by the pace/store bounds; a wire-clocked per-path
estimator (§16.15's deferred option 2) would tighten it.

**C7 (c2+c2) symmetric:**

| arm | seed42 (σ_s) [runs] | seed7 (σ_s) [runs] | pooled |
|---|---|---|---:|
| P | 20.99 (0.45) [21.1 21.1 20.8 21.1 20.1 21.6 21.0 21.1] | 24.76 (1.68, n=6) [26.2 27.1 24.6 22.8 23.1 24.9] | 22.9 |
| PB | 84.36 (29.4) [12.5 99.5 96.4 95.4 92.4 87.2 101.2 90.2] | **94.62** (15.6) [97.6 93.3 97.9 59.8 115.4 95.2 99.8 97.9] | **89.5** |
| G0 | 20.56 (0.51) [20.5 21.2 20.2 19.7 21.0 20.9 20.2 20.9] | 20.16 (1.65, n=6) [21.2 19.7 17.5 22.4 20.5 19.5] | 20.4 |
| GPB | 33.23 (2.59) [29.4 36.1 35.7 30.9 35.5 34.8 31.7 31.7] | 31.05 (2.82, n=7) [29.6 34.0 30.8 32.0 31.3 25.8 33.8] | 32.1 |

**C8 (c2+c3) heterogeneous:**

| arm | seed42 (σ_s) [runs] | seed7 (σ_s) [runs] | pooled |
|---|---|---|---:|
| P | 14.67 (0.99) [14.0 14.8 15.1 13.0 14.6 14.3 14.9 16.5] | 13.43 (2.95, n=7) [14.9 14.3 10.1 16.5 9.6 16.9 11.8] | 14.1 |
| PB | 37.51 (24.7, BIMODAL) [3.2 36.1 51.7 2.5 61.2 43.8 32.2 69.5] | 54.59 (10.0, n=7) [49.9 65.4 61.3 36.5 49.1 60.2 59.8] | 45.5 |
| G0 | 9.40 (0.21, n=7, **1 DNF**) [9.1 9.3 9.2 9.4 9.7 9.6 9.5] | 9.07 (0.49) [9.2 9.2 8.3 9.2 9.2 8.5 9.9 8.9] | 9.22 |
| GPB | **32.33** (5.00) [35.7 25.2 33.7 35.0 35.8 23.5 34.7 35.1] | 27.70 (11.0) [35.2 27.9 35.2 27.2 33.7 3.0 23.2 36.2] | **30.0** |

### VERDICT — what was won, and the honest framing

1. **The per-path substrate ceiling is NAMED (quinn Cubic × queue-bloat) and
   RAISED ×3.5** (gen single-c2 9.77→33.9, σ ~1; single-c3 0.78-DNF→13.0;
   C8 9.2→30.0). The primary target (≥14, link-class 15.15) is exceeded 2.2×.
2. **C8 gen vs fast-alone:** GPB C8 = 32.3/27.7 vs the same-day plain fast-alone
   17.0/18.9 ⇒ **×1.9/×1.5 — the first C8 numbers above the historic link-class
   fast-alone, per §16.16's own framing** (against the plain link control). BUT
   the same lever moves the goalposts honestly: on the SAME (BBR) substrate,
   plain fast-alone is 74.5, so gen C8 = 0.42× of the new single — and gen C8 ≈
   0.95× of gen's OWN single (33.9) exactly as before, one level up. C8 still
   does not aggregate above its own single-path ceiling; neither does plain+BBR
   (C8 45.5 bimodal σ 25 vs single 74.5 = 0.61×, vs gen's σ 5–11).
3. **The gen machine is now the binder at ~34 total regardless of path count**
   (single 33.9 ≈ C7 32.1 ≈ C8 30.0, and = the L0 shim ceiling 34 on a faster
   CPU): the substrate is no longer the wall; the residual is the app/decode
   machine — the receiver's per-generation Gauss-Jordan (O(G²·S) ≈ 90 ms CPU per
   384-symbol generation ≈ 4 000 sym/s ≈ 39 Mbit/s) plus the remaining
   fill/decode serialization. That is the next lever, and it is CPU, not
   networking.
4. **M\* engaged honestly:** at c2/c3 BDP < G so M\* stays 2 — GP's +47 % came
   from the queue discipline (in-flight cap), the sent-frontier clock, the
   windowed-max pace, and the once-per-SRTT reactive bound, NOT extra depth.
   The depth term of #61 is implemented and unit-tested but only engages at
   higher BDP (RTT100/200) — unvalidated there; that is what remains of #61.
5. **C7 symmetric:** bare gen keeps ×1.37-class aggregation under Cubic
   (G0 20.4 ≈ §16.16); with BBR the gen machine's ~34 ceiling swallows the
   aggregation (GPB C7 32.1 ≈ single 33.9 = ×0.95) while plain+BBR aggregates
   ×1.2–1.3 (89.5 vs 74.5). Gen aggregation needs the decode ceiling raised
   before C7 can show it again on the fast substrate.

### Controls / caveats / discipline items

- **Plain arms unchanged:** P single 17.0/18.9, C7 21.0/24.8, C8 14.7/13.4 —
  all within the recorded historic ranges (15.15/20.5–25.1/14.96 §16.16).
- **Shipped default byte-identical:** `RWM_GEN_PIPE`/`RWM_QUIC_CC`/`RWM_L0_NETEM`
  unset ⇒ stock paths (gate_suite 15/15 release, lib 298/298, math 47/47 confirm);
  the two ablation-recommended default flips (RWM_RATE_SAMPLE, RWM_DAPS_DEPTH now
  default OFF, explicit =1 to enable) only affect the generation+DAPS opt-in
  stack, not the shipped non-generation default.
- **Noise floor:** same-config σ_s ≤ 1.3 on the headline arms (G0 0.16–0.49,
  GB/GPB 1.0–1.3 single); every claimed effect (+4.6, +23, +24 Mbit/s single) is
  10–100× that. Cross-seed spread ~2 Mbit/s. PB/GPB dual arms are the noisy ones
  (σ 5–29, PB C8 outright bimodal 2.5–69.5) — reported, not hidden.
- **DNFs (all reported):** GPB single-c2 seed7 rep1 (300 s timeout, all data
  coded — tail wedge, 1/16); G0 C8 seed42 rep4 (the §16.16-class gen-bare C8
  substrate DNF); GPB C8 seed7 rep6 ran at 3.0 (collapse-run, counted in the
  mean). Everything else dnf=0.
- **Harness caveat (pre-existing):** seed-7 GE occasionally eats the topo
  verification ping (§16.16's caveat); the driver retries once — arms with n<8
  (P-sc2-s7 n=3, GP-s7 n=7, …) lost those reps to double aborts; all n recorded.
- **`RWM_QUIC_CC=bbr` is an EXPERIMENT knob, not yet a shipped default** —
  fairness/safety of BBR-under-loss on shared bottlenecks is not evaluated here;
  flipping the default is a separate decision with its own battery.
- Full logs: VM `/home/vibe/gensub/{sc2,sc3,c7,c8}-s{42,7}.log` + the probe runs;
  instrument runs `/tmp/rwm-c.log` (GDIAG lines quoted above).

### Tests

`cargo test -p raptorpath --lib` 298/298 (2 new: `gen_pipe_depth_covers_bdp_plus_
one_deficit_round`, `set_pipeline_depth_widens_the_proactive_span`);
`-p raptorpath-math` 47/47; gate_suite 15/15 release. L0 bench + loopback suite
(perf_loopback 8/8, fmtcp/daps loopbacks) pass with the shim off and on.


## Decode-CPU Ceiling (2026-07-13) — the ~34 Mbit/s generation machine was the CODED-ONLY WIRE's O(G²·S), not the solver: the systematic-repair wire is the O(k·G·S+k³) machine — gen single-c2 33.9→70.9 (×2.1, = 0.92× plain+BBR), c3 13.0→15.0 (0.95× the plain+BBR recovery ceiling), C8 het 30.0→69.8 (beats plain+BBR's own C8 ×1.25–1.5 with σ halved); sparse-aware decoder (pure speedup, output-identical, differential-tested) (branch `feat/decode-cpu-ceiling`)

*Decision record: → [ADR-0056](adr/0056-systematic-wire-sparse-decoder.md)*

§16.17 left ONE binder: the generation machine capped the whole gen transport at
~34 Mbit/s regardless of path count (single 33.9 ≈ C7 32.1 ≈ C8 30.0 = the L0
shim ceiling), attributed to the receiver's dense per-generation Gauss–Jordan
(O(G²·S)). This section profiles that machine at L0 (JOB 1), finds the
attribution HALF right — the quadratic is real but lives in the WIRE MODE, not
the solver — fixes it (JOB 2), and re-measures at L1 (JOB 3).

### JOB 1 — profile: the coding machine alone (tests/gen_decode_bench.rs, new)

`#[ignore]` micro-bench of encoder+decoder on L1-shaped parameters (G=384,
S=1200 bulk symbols, r=0.03, ε=2.6 %, 10 % late/reordered source, 30
generations, SplitMix64 seed 42), per-call attribution into buckets [src =
source with no covering matrix | src+mat = late source into a live matrix (the
#59 injection) | rep0 = first repair of a generation (slot creation) | rep =
subsequent repairs]. Run on the dev box AND the L1 VM — whose CPU is
`QEMU Virtual CPU 2.5+` exposing **SSSE3 only, no AVX2** (GF(256) mul-acc
kernel: 4.1 GB/s there; dev AVX2 4.2–7.1 GB/s).

**VM numbers, pre-rewrite decoder (the machine §16.17 measured):**

| trace | delivered sym/s | dominant per-call costs |
|---|---:|---|
| coded-only ε=2.6 % (the L1 gen-arm wire) | **5 943 ≈ 57 Mbit/s** | rep 166 µs/row ⇒ ~64 ms/generation |
| encoder, coded-only | **4 922 (203 µs/coded sym)** | the SENDER pays the same quadratic |
| systematic ε=2.6 %+late | 125 479 | src 1.2 µs, inject 23 µs, rep0 257 µs, rep 106 µs |
| systematic clean in-order | 128 564 | rep0 1 262 µs (slot preload), rep 95 µs — ALL of it redundant work |
| systematic fill-flag-heavy | 66 221 | every source pays an 11 µs injection |

**The KEY answer — why O(G²·S) instead of O(k·G·S):** the generation arm every
battery measures (`--window-generation-coding`) is **coded-only on the wire**:
NO raw source rides it (§16.17's own arithmetic — 38 280 coded emitted for
~21 800 source symbols). Every one of the G DoF per generation arrives as a
dense combination, so ~G dense rows × O(G) row-ops × (G+S) bytes at the
receiver AND ~G coded emissions × O(G·S) at the sender are
information-structural for that wire — no SIMD (already PSHUFB AVX2/SSSE3,
ADR-0041) or thread pool changes the asymptotic. enc 4.9 k + dec 5.9 k sym/s
per VM core ≈ the 34 Mbit/s ceiling exactly. At ε=2.6 % only k ≈ ε·G ≈ 10 DoF
per generation are actually missing. The O(k·G·S + k³) machine is the EXISTING
systematic-repair submode (`--window-systematic-repair`, §16.3's oracle pick):
source delivers as unit rows in O(S); only ~⌈G·r⌉+deficit ≈ 13 repair rows per
generation are dense — 125 k sym/s (≈1.2 Gbit/s·core) on the SAME VM before
any new code.

### JOB 2 — the fix, in increments (each measured at L0)

**Increment 1 — sparse-aware decoder (pure speedup, UNCONDITIONAL, delivered
set byte-identical).** The old decoder still wasted work in systematic mode:
every known source was materialized as a full-width fused unit pivot row (slot
creation copied O(G·(G+S)) bytes), every dense repair was reduced against all
G rows with (G+S)-byte fused SIMD calls even when 374/384 of them meant
"subtract a known source", late-source injection built and reduced full-width
rows, and unit-row detection re-counted whole coefficient rows. Rewrite
(src/fec/generation.rs): a per-slot `known` bitmap — known sources NEVER enter
the matrix; incoming rows eliminate known columns PAYLOAD-ONLY against the
shared `recovered` store (S bytes, not G+S); only coded rows (≤ k + deficit
margin) are kept, in incremental RREF; a row that turns UNIT is delivered on
the spot and converted to `known` (the active system stays k×k; completion =
`known_count == width`, no separate full-rank pass); a span already fully
recovered never creates a matrix — a redundant repair costs O(G) with ZERO GF
work (the k=0 case); `advance` keeps `recovered` entries alive for the span of
any live Solving slot (the payloads the old code privately copied). Cost per
generation: O(k·G·S + k²·(G+S)). On the coded-only wire nothing is ever known,
the arithmetic degenerates to the same dense elimination as before (plus an
early-exit unit scan) — measured 1.2× faster, no regression.
  The pre-rewrite decoder is kept VERBATIM as `fec::generation::reference`
(doc-hidden, never constructed by the engine) and a NEW differential test
(`sparse_decoder_matches_reference_on_random_traces`) drives both on random
traces — systematic AND coded-only wires, 5–25 % loss, late sources, FILL_FLAG
filling repairs, duplicates, deficit top-ups, mid-trace `advance`, 4 seeds —
asserting per-call delivered sets + payload bytes + `rank_in` + `total_fed` /
`repairs_fed` / `repairs_useful` equality at every step. (One documented
divergence: the ORDER of seqs within a single completing `add_symbol` return;
every consumer keys on seq.)

L0 micro gains, same-run old→new:

| trace | VM (SSSE3) | dev (AVX2) |
|---|---|---|
| sys ε+late | 125 k → **160 k** sym/s (×1.27) | 154 k → 171 k (×1.11) |
| sys clean in-order (k=0 path) | 129 k → **643 k** (×5.0) | 163 k → 604 k (×3.7) |
| sys fill-heavy | 66 k → 86 k (×1.29) | 63 k → 73 k (×1.15) |
| coded-only | 5.9 k → 7.2 k (×1.20) | 5.7 k → 7.2 k (×1.27) |

**Increment 2 (SIMD) — already present; SKIPPED with numbers:** ADR-0041's
PSHUFB nibble-table kernel with runtime AVX2/SSSE3 dispatch is in place; the
VM has no AVX2 yet still does 4.1 GB/s, and the systematic-mode floor
(known-elimination ≈ 105 µs × ~13 repairs/gen ≈ 1.4 ms/gen ≈ 280 k sym/s) is
far above link rate — the kernel is not the binder. **Increments 3 (parallel
decode) and 4 (G-shrink) — NOT NEEDED** (L1 below: the decode machine no
longer sets the clock).

**Increment 2' — the algorithmic lever at the system level (EXISTING flag, no
new semantics):** run generation mode with the systematic-repair wire. L0
full-engine bench (gen_substrate_l0, netem shim, 12.5 MB × 3, Windows,
before-binary vs after-binary):

| mode | c2 before | c2 after | c3 after |
|---|---:|---:|---:|
| gen (coded-only) | 32.5 | 33.4 | 11.3 |
| sys (systematic-repair) | **70.1** | **68.3** | 12.3 |
| plain | 70.4 | 68.2 | 15.5 |

**sys == plain at L0** — the coding machine's CPU tax is structurally gone
(and c3 is link/loss-bound for every mode, as it should be).

### JOB 3 — L1 A/B (VM 10.1.5.16, 2026-07-13 ~09:13–12:08 UTC; 25 MB × 1
run/invocation × 8 reps, arms interleaved round-robin per rep, fresh tunnel
each invocation, seeds 42 AND 7, `cod>0` GUARD OK on every gen run, dnf and n
recorded, full env+command+binary-md5 per run in VM `/home/vibe/gendec/*.log`,
driver `/home/vibe/decode_battery.sh`, harness copy `/home/vibe/l1d` with
per-invocation CPU capture; shared-VM lock `/tmp/rwm-vm.lock` held for the
session; binaries: before = 02d240c, after = da926a5)

Arms (every gen arm on the GPB stack `RWM_GEN_R=0.03 RWM_GEN_PIPE=1
RWM_QUIC_CC=bbr`): **PB** = plain+BBR (after-binary; the plain path is
untouched by this branch) · **Bgen/Agen** = coded-only generation wire on
before/after binary (§16.17's GPB arm; the pure-speedup A/B) · **Bsys/Asys** =
same + `--window-systematic-repair` (the algorithmic arm; same-binary flag
A/B against the gen arms).

**single-c2 (PRIMARY; §16.17 GPB = 33.9; target ≥60). CPU = mean seconds per
25 MB invocation, srv=receiver/decoder · cli=sender/encoder:**

| arm | seed42 mean (σ_s) [runs] | seed7 mean (σ_s) [runs] | CPU srv·cli |
|---|---|---|---|
| PB | 77.11 (2.74) [75.9 72.9 78.4 74.7 75.6 80.6 80.1 78.6] | 77.63 (3.57, n=6) [80.3 76.2 79.4 77.9 71.2 80.7] | 2.97 · 2.02 |
| Bgen | 33.53 (1.39) [32.6 35.5 34.3 32.6 34.9 34.3 31.9 32.1] | 33.06 (1.21, n=7) [32.1 34.3 31.7 34.4 33.3 33.9 31.7] | 5.54 · 4.45 |
| Agen | 36.66 (1.88) [39.3 35.2 33.6 37.4 38.4 36.2 37.7 35.5] | 35.42 (0.93, n=6) [33.9 35.9 35.1 35.5 36.7 35.4] | 4.98 · 4.36 |
| Bsys | 70.09 (3.17) [67.3 75.8 71.9 66.1 68.2 68.6 71.7 71.2] | 71.08 (2.32, n=6) [66.7 72.9 71.1 73.1 71.7 70.9] | 3.37 · 2.24 |
| **Asys** | **70.93 (3.21)** [70.3 67.2 68.5 70.4 76.6 70.2 75.0 69.4] | **70.77 (2.59, n=7)** [72.0 67.0 68.8 74.2 73.3 69.3 70.8] | 3.38 · 2.25 |

Bgen replicates §16.17's 33.9 exactly (33.5/33.1 — session comparability
anchored; PB replicates 76.1 → 77.1/77.6). The systematic wire is **×2.1
(70.9/70.8, σ ≤ 3.2, dnf 0) = 0.92×/0.91× of plain+BBR's own single** — the
≥60 target is cleared with the link-class control in sight. The decoder
rewrite alone moves the coded-only arm +3.1/+2.4 (2–3× its σ_s — real, small,
exactly as the profile predicted: the coded wire is the quadratic, not the
solver). Bsys ≈ Asys at L1 (+0.8/−0.3, inside noise): at 70 Mbit the old
decoder's systematic-mode waste did not yet bind the wall — it binds CPU
(below) and the next rate class.

**single-c3 (the lossy 4×-win; §16.17 GPB = 13.0; plain+BBR-c3 measured here
as the honest recovery ceiling):**

| arm | seed42 (σ_s) [runs] | seed7 (σ_s) [runs] | CPU srv·cli |
|---|---|---|---|
| PB | 15.63 (0.45) [14.9 15.9 15.2 16.3 15.4 15.7 15.9 15.7] | 15.84 (0.20, n=7) [15.5 15.9 16.0 15.7 15.8 16.1 15.7] | 3.54 · 3.17 |
| Bgen | 12.75 (0.24, n=7, **1 DNF**) [12.8 12.7 12.6 13.1 12.4 13.0 12.8] | 12.80 (0.21, n=6) [12.9 12.5 12.9 12.6 13.0 12.9] | 8.50 · 6.26 |
| Agen | 12.92 (0.26) [12.6 12.7 13.2 13.0 13.0 13.0 12.5 13.2] | 12.61 (0.47, n=7, **1 DNF**) [12.6 12.6 12.6 11.6 13.0 13.0 12.7] | 7.72 · 6.13 |
| **Asys** | 13.33 (4.32, median **14.9**, 1 collapse-run 2.7) [14.9 14.9 14.6 14.7 2.7 15.6 15.1 14.1] | **15.06 (0.50)** [15.0 15.4 14.7 15.6 15.6 14.9 14.2] | 4.92 · 4.01 |

The 4× lossy-path FEC win (vs plain-Cubic 3.2/3.7) HOLDS and improves:
13.0 → 14.9-median/15.1 = **0.95× of plain+BBR-c3's 15.6/15.8** — generation
recovery now rides essentially at the substrate's own recovery ceiling. One
collapse-run (2.7, counted in the mean) — the same low-run class PB-c3 itself
showed in §16.17 (2.8/2.2). CPU on the deficit-hot path: srv 8.5→4.9 s,
cli 6.3→4.0 s.

**C7 (c2+c2) symmetric and C8 (c2+c3) heterogeneous (after-binary arms):**

| arm | C7 s42 (σ) [runs] | C7 s7 (σ) [runs] | C8 s42 (σ) [runs] | C8 s7 (σ) [runs] |
|---|---|---|---|---|
| PB | 93.08 (14.3) [87.0 110.3 62.5 93.8 99.3 98.3 90.8 102.7] | 94.51 (11.2) [101.2 99.5 106.0 69.2 97.9 90.4 97.7 94.2] | 55.73 (13.2) [62.9 70.1 54.2 70.9 56.7 57.1 33.6 40.4] | 45.42 (12.0, n=7) [29.3 63.5 33.5 45.4 44.0 56.6 45.8] |
| Agen | 34.63 (2.91) [36.4 33.2 34.4 37.6 36.6 28.4 36.1 34.4] | 33.44 (3.04) [33.9 33.8 37.4 27.8 30.3 34.6 33.7 35.9] | 34.87 (3.34) [27.0 37.3 35.6 36.0 34.4 36.7 35.0 37.2] | 36.70 (0.81, n=4, **1 DNF**) [35.5 36.8 37.2 37.3] |
| **Asys** | 72.31 (4.05) [72.1 75.0 78.8 67.1 70.2 73.6 67.0 74.6] | 72.42 (3.80) [67.4 72.9 76.7 68.5 78.2 73.3 72.6 69.6] | **69.77 (5.04)** [62.8 67.6 62.1 72.5 74.8 73.8 70.8 73.7] | **69.10 (5.49, n=6)** [68.9 74.0 58.7 73.2 69.5 70.3] |

### VERDICT — every factor WITH its control

1. **The decode-CPU ceiling is DISSOLVED.** Gen single-c2 33.9 → **70.9/70.8**
   (×2.1, 10–14× the arm's σ_s) = 0.92× of plain+BBR's same-session 77.1/77.6.
   The mechanism: the O(G²·S) was the coded-only WIRE (both ends), and the
   systematic-repair wire + sparse decoder is the O(k·G·S+k³) machine.
2. **The c3 lossy-path win holds and improves:** 13.0 → 14.9-median/15.1 =
   0.95× of the plain+BBR-c3 recovery ceiling (15.6/15.8) measured same-
   session; ~4.1× plain-Cubic's historic 3.2/3.7.
3. **C8 heterogeneous: gen+sys is now the BEST C8 config measured on this
   testbed** — 69.8/69.1 (σ 5.0/5.5, dnf 0) vs plain+BBR's own C8 55.7/45.4
   (σ 13.2/12.0, runs swinging 29–71): **×1.25/×1.52 with the variance halved
   and no bimodality**, and the first C8 in link-class territory
   (0.90×/0.89× of plain+BBR fast-alone 77.1/77.6). Honest ceiling stack-up:
   vs gen's OWN single 70.9 it is 0.98× — parity, still not aggregation —
   and vs the per-path plain+BBR singles summed (92.7/93.4) it is 0.75×.
   The FEC value at C8 is *stability + the slow path costing nothing*, where
   plain+BBR pays a 0.72×/0.59× bimodal penalty for touching the lossy path.
4. **C7 symmetric: no aggregation above gen's own single** (72.3/72.4 =
   ×1.02 of 70.9) while plain+BBR does aggregate (93.1/94.5 = ×1.21). The
   next binder is NOT decode: see CPU below.
5. **Pure decoder speedup (binary A/B, coded-only arm): +3.1/+2.4** — real
   (2–3× σ_s) but small at L1, as the profile predicted; its L1 value is the
   CPU headroom and the k=0/injection paths, its headline is the L0 ×1.3–5.0.

### CPU — the binder visibly moved (per-25 MB CPU seconds, whole invocation)

| config | recv (decoder) | send (encoder) | rate |
|---|---:|---:|---:|
| gen coded-only (Bgen sc2) | 5.54 s | 4.45 s | 33.5 |
| gen systematic (Asys sc2) | **3.38 s** | **2.25 s** | **70.9** |
| plain+BBR (PB sc2) | 2.97 s | 2.02 s | 77.1 |

Coded-only at 33.5 Mbit/s burns CPU-seconds ≈ 0.8× its (stretched) wall on
each side — the machine IS the clock. Systematic at 70.9 sits within 14 %
(recv) / 11 % (send) of plain+BBR's CPU at the same link-class rate: the FEC
tax at r=0.03 is now ~0.41 s recv + 0.23 s send per 25 MB. Per delivered bit,
recv CPU fell ×3.4 and send ×4.2. **The residual per-core limit:** in Asys
C7/C8 (and PB C7) the RECEIVER process runs at ~1.0 core (3.6–3.7 s CPU over
~3.5 s wall) — the single-threaded receive/reassembly/delivery engine caps
one gen-sys sink at ≈72 Mbit/s on this VM core (plain ≈93); decode itself is
now ≤ ~15 % of that budget (micro: 160 k sym/s available vs ~7.3 k consumed).
C7 aggregation for gen mode is receiver-engine-bound, not decode-bound.

### Controls / caveats / discipline items

- **Liveness:** GUARD OK (cod>0) on every gen run; sys arms emit only repair
  (coded ≈ 1 000–2 300 per 25 MB ≈ r+deficit — the wire is really systematic;
  coded-only arms ≈ 22 500–28 700).
- **Noise floor:** headline arm σ_s 0.2–5.5; the claimed effects are +37
  (single), +2.1/+2.3 c3, +14.0/+23.7 C8-vs-PB-C8 — 3–14× the respective σ_s.
  PB dual arms remain the noisy ones (σ 11–14), reported not hidden.
- **DNFs (all 3, all on coded-only gen arms):** Bgen sc3-s42 rep6, Agen
  sc3-s7 rep1, Agen c8-s7 rep3 (300 s timeouts — the §16.16/16.17 gen-arm
  tail-wedge class). **Sys arms: 60/60 runs, dnf 0**; one Asys sc3 collapse-
  run (2.7) counted in its mean. n<8 arms lost reps to the seed-7 topo-ping
  double-abort (harness caveat, recorded; 9 RETRYs in c8-s7 alone).
- **Shipped default byte-identical in behavior:** the decoder rewrite is a
  pure speedup with a byte-identical delivered SET (differential-tested;
  intra-call ordering divergence documented above); `--window-systematic-
  repair` is a pre-existing CLI mode, default OFF; RWM_GEN_PIPE/RWM_QUIC_CC
  remain default-OFF experiment knobs (BBR fairness still unevaluated).
- **VM shared with a second worker** (feat/copa-sole-cc): `/tmp/rwm-vm.lock`
  protocol honored — lock held 09:13–12:08 UTC incl. builds, released after
  teardown; my binaries/tree in `/home/vibe/rp-decode` (the other worker's
  `/home/vibe/raptorpath` tree was left untouched).
- **What this does NOT claim:** no multipath aggregation above the best
  single path anywhere (gen C7/C8 ≈ gen single ≈ 0.9× plain fast-alone);
  the harness gen default is still the coded-only flag — flipping the
  battery default to systematic is a separate (recommended) decision.

### Tests

`cargo test -p raptorpath --lib` 299/299 (new: the old-vs-new differential
`sparse_decoder_matches_reference_on_random_traces`); `-p raptorpath-math`
all green (47/19/22/4/4/23); `gate_suite` 15/15 release; `perf_loopback` 8/8,
fmtcp/daps loopbacks green; L0 engine bench green before/after. Micro-bench:
`cargo test --test gen_decode_bench --release -- --ignored --nocapture`
(RWM_B_* knobs documented in the file).

## Copa-Sole Substrate CC (2026-07-13) — `RWM_QUIC_CC=passthrough`: the engine's per-path Copa-lite cwnd IS quinn's congestion window, fed for the first time with CLEAN plain-mode delivery samples; Copa-sole does NOT earn bulk-throughput parity with BBR-under (0.4–0.6×, mechanism named) but holds a 3–6× tighter standing queue everywhere (slow-path tail up to ×8), kills plain-BBR's c3 collapse mode (σ 6.5→0.63), and aggregates C7 at ×1.98 of its own single (branch `feat/copa-sole-cc`, code commit a895205)

*Decision record: → [ADR-0062](adr/0062-copa-wire-signal-competitive-mode.md)*

Task #80. The Gen Substrate Ceiling section proved quinn gates every datagram
send on its own congestion controller — so the effective window was always
min(app CC, quinn CC), with quinn's loss-reactive Cubic silently the binder.
This build makes the substrate controller an explicit POLICY surface and
measures the third policy: OUR Copa-lite owning the window outright.

### Design (as built)

1. **Pass-through shim** (`src/transport/quic.rs`): a
   `quinn::congestion::Controller` whose `window()` reads an `Arc<AtomicU64>`
   (bytes) the engine writes; `on_congestion_event` etc. are recorded
   (per-path counters) and never acted on — loss is the FEC layer's job
   (paper §12.1), congestion safety is Copa's delay backoff. One factory per
   endpoint = per connection = per path; the transport keeps the handles
   (`set_cc_window_bytes(path, bytes)`). Initial window 256 KB (handshake and
   pre-feed traffic never starved; ack-only reverse connections simply keep
   it), floor 2 MTUs (a zero write can never wedge the connection). quinn's
   own pacer derives from the window, so the wire send process is paced at
   Copa's cwnd/RTT. `RWM_QUIC_CC` accepts `passthrough` next to
   `bbr|newreno|cubic`; default UNSET = stock Cubic, byte-identical
   (gate_suite 15/15 release on this tree).
2. **Conversion**: Copa cwnd [symbols] × 1250 B/symbol (plain mode = one
   ~1200 B symbol per datagram + framing) → window bytes, written after every
   Copa update (WindowAck feed; the block-mode Ack arm writes it too).

### The plain-mode Copa feeding fix — and a code-fact CORRECTION to the 2026-07-13 verdict audit

The verdict audit claimed plain window-reliable mode never feeds Copa's
`record_delivery` (WindowAcks record RTT only; only the block-path
`ControlMessage::Ack` drives it). HALF right: WindowAcks are indeed RTT-only,
BUT the per-batch `ControlMessage::Ack` send site (net/mod.rs, "ADR-0005:
send ACK with echo timestamp") sits AFTER the window/block receive branch and
fires in WINDOW mode too — plain mode has ALWAYS driven
`Scheduler::ack -> on_ack -> record_delivery`, just with the ACK-INTERVAL Δt
estimator. MEASURED on the L0 netem shim (c2 params, shipped plain default,
`RWM_RS_TRACE` forensic): that estimator's windowed max over-reads ~×10
(btlbw 108 739 vs true ~10.4 k sym/s; spike anatomy: Δ≈101 symbols over a
~1 ms ack-bunch), est=Y, cwnd pinned 4 793 by the anchor floor, and the plain
`plain_dyn_cap` (2×anchor) store cap latched at RELIABLE_STORE_MAX 1024 — the
plain-mode standing queue (203–356 ms at L1 c2) is the OVER-READ, not a
missing feed. Copa was never blind in plain mode; it was fed garbage.

The feed (active only under `RWM_QUIC_CC=passthrough` or standalone
`RWM_COPA_FEED=1`; in-order plain window-reliable only; shipped default
byte-identical):

- **Send side**: every plain source send (and targeted retransmit) records
  seq→path (`CopaFeed`) + a BBR rate-sample snapshot (`on_src_sent` — the
  existing §16.13 send-interval machinery, previously generation-gated).
- **Ack side**: each WindowAck's cumulative-frontier advance + newly-SACKed
  seqs are diffed against an attribution cursor (each seq attributed exactly
  once, out-of-order/duplicate-ack safe, per-ack work bounded) and attributed
  to the path that carried them: `on_src_delivered_seq` (SEND-interval Δt —
  ack-aggregation robust) + `on_delivery_signal` (the `record_delivery`-free
  half of `on_ack`; update rules byte-identical) + the pass-through window
  write. RTT floor and delivery signal are then BOTH live per path (DIAG:
  est=Y, btlbw 8 048–10 659 ≈ true 10.4 k at c2 — over-read CLOSED at L0 and
  at L1 on the fast path; the c3/slow path still over-reads ~×4, decode/ack-
  burst clocked, recorded below).
- **Legacy pollution suppressed**: under the feed the per-batch Ack arm
  releases the wire-level in-flight budget only (no `record_delivery`).
- **Store-cap re-key**: with honest samples the 2×anchor outstanding cap is
  CIRCULAR (samples can never read above the cap they set — L0 measured the
  collapse: anchor stuck ~3.2 k of 10.4 k, throughput 18.5 of 66 Mbit/s; the
  legacy over-read was accidentally load-bearing). Under the feed the cap is
  `RWM_STORE_GAIN × Σcwnd` (Copa's probe state escapes the loop; default
  gain 2.0) — L0 restored 47.5.

### L1 battery — PLAIN mode only (VM 10.1.5.16, 2026-07-13 ~12:19–13:20 UTC; binary sha256 c2248e40a1db0b… built from commit a895205; 25 MB × 1 run/invocation × 8 reps, arms interleaved round-robin per rep, fresh tunnel per invocation, seeds 42 AND 7, `RWM_DIAG=1` on every arm; full env + command per run in `/home/vibe/copasole/{sc2,sc3,c7,c8}-s{42,7}.log`, per-run sender DIAG in `diag-*.log`; driver `/home/vibe/copasole_battery.sh`)

Arms (all PLAIN, `RWM_GEN=0`, same binary): **A** = stock Cubic-under
(shipped default) · **B** = `RWM_QUIC_CC=bbr` (the §16.17 reference) ·
**C** = `RWM_QUIC_CC=passthrough` (Copa-sole) · **D** (sc2 only) = C +
`RWM_STORE_GAIN=1.25` (reservoir probe). Mechanism liveness: every completed
C/D run's sender log carries the `passthrough` + `feed ACTIVE` config echo
and est=Y DIAG; the 9 seed-7 invocations lost to the known topo-ping abort
(the n<8 entries) are exactly the runs with no result. Session validated
against the same-day §16.17 references: A 17.0/19.5 (historic 17.0/18.9),
B 75.9/75.4 (historic 76.1/72.6).

**single-c2** (Mbit/s, mean (σ_s) [runs]):

| arm | seed42 | seed7 |
|---|---|---|
| A | 17.02 (0.26) [17.3 16.9 16.9 17.5 17.1 17.0 16.7 16.9] | 19.45 (2.29, n=7) [18.8 24.6 19.4 18.2 18.9 18.2 18.2] |
| B | **75.89** (1.92) [76.8 76.1 77.1 74.1 76.5 74.1 79.1 73.4] | **75.43** (3.08, n=7) [77.8 75.6 73.6 72.8 72.5 74.8 81.0] |
| C | 28.86 (5.95) [39.2 34.3 24.3 28.7 20.0 30.9 26.2 27.4] | 31.22 (8.42, n=6) [33.2 15.4 40.0 33.6 35.2 30.0] |
| D | 32.60 (2.87) [32.4 31.6 33.4 38.1 34.9 31.2 30.1 29.0] | 38.39 (2.56) [39.5 33.5 38.1 42.3 37.8 37.6 40.4 38.0] |

**single-c3**:

| arm | seed42 | seed7 |
|---|---|---|
| A | 3.19 (0.10) | 3.63 (0.13, n=7) |
| B | 10.60 (**6.51, BIMODAL**: 3× ~2.75 + 5× ~15.3) | 15.53 (0.28, n=7) |
| C | **9.54 (0.63)** [10.5 10.1 9.4 9.5 8.4 9.5 9.3 9.8] | **9.87 (0.74, n=6)** [9.2 10.9 9.3 9.8 9.4 10.6] |

**C7 (c2+c2)**:

| arm | seed42 | seed7 |
|---|---|---|
| A | 20.56 (0.82) | 24.67 (1.70, n=6) |
| B | **96.73** (13.68) [69.7 110.7 84.6 108.0 99.3 106.9 97.4 97.3] | **99.93** (6.56) |
| C | 57.05 (9.34) [51.3 72.0 45.9 67.7 55.7 48.0 61.8 54.1] | 51.08 (3.72, n=6) |

**C8 (c2+c3)**:

| arm | seed42 | seed7 |
|---|---|---|
| A | 14.25 (2.05) | 12.93 (2.25, n=6) |
| B | **54.50** (9.50) [49.2 49.1 61.5 45.6 57.3 48.1 74.1 51.0] | **52.94** (10.64, n=7) [63.3 38.7 56.7 45.7 41.6 62.7 61.9] |
| C | 28.35 (8.21) [37.3 24.5 14.0 40.2 27.1 30.1 30.0 23.6] | 29.35 (4.13, n=5) [29.0 31.9 34.5 27.8 23.6] |

**Queue behavior (Copa's selling point) — per-path live RTT vs RTprop from the
sender DIAG, pooled steady-state (per-run lines 4+), p50 queue = rtt p50 −
rtp p50 (ms). NOTE: this echo-RTT includes the sender's OWN store reservoir
buffered in quinn's datagram queue — the app-layer pipeline delay a consumer
actually experiences — in all arms alike:**

| cell/path | A | B | C | D |
|---|---|---|---|---|
| sc2 s42 | 203 (rtt p50/p90 216/330) | 65 (77/102) | **23 (33/77)** | **16 (25/79)** |
| sc2 s7 | 356 (370/654) | 87 (99/**512**) | **21 (32/78)** | **12 (22/38)** |
| sc3 s42 | 1501 (1572/2892) | 120 (162/436) | **77 (116/336)** | — |
| sc3 s7 | 1536 (1584/2757) | 430 (474/**2450**) | **83 (124/438)** | — |
| c7 s42 (p0/p1) | 197/180 | 33/42 | **31/28** | — |
| c7 s7 (p0/p1) | 226/207 | 26/31 | **34/23** | — |
| c8 s42 fast/slow | 101/**1146** | 33/313 | **30/70** | — |
| c8 s7 fast/slow | 99/**1615** | 52/395 (slow p90 **2474**) | **23/131** (slow p90 321) | — |

### VERDICT — honest

1. **Copa-sole does NOT earn bulk-throughput parity with BBR-under**: C/B =
   0.38–0.41 (sc2), 0.51–0.59 (C7), 0.52–0.55 (C8), 0.62–0.90 (sc3). Every
   delta ≫ the recorded arm σ_s. The MECHANISM is Copa working as designed
   plus one structural coupling: Copa equilibrates its perceived queuing
   delay at the hint target (+jitter headroom, ~10 ms at c2), and its
   app-layer echo-RTT signal includes the sender's own (gain−1)×cwnd store
   reservoir draining through quinn — a self-signal that caps the equilibrium
   cwnd near BDP + target·BtlBw, i.e. ~40–60% utilization at the wire, where
   BBR runs 2×BDP cwnd and simply eats 65–430 ms of queue. Arm D (reservoir
   1.25×cwnd instead of 2×) confirms the self-signal term is real: +13/+23%
   throughput AND a tighter queue (p50 16/12 ms, p90 38 ms at s7).
2. **The queue claim is decisively DELIVERED**: standing queue p50 3–6×
   tighter than BBR-under in every cell (23 vs 65–87 at sc2; 77–83 vs
   120–430 at sc3; c8 slow path 70–131 vs 313–395), and the TAILS are the
   headline — no PROBE_BW overshoot / PROBE_RTT-class stalls: sc2-s7 p90 78
   (C) / 38 (D) vs 512 (B); c8-s7 slow p90 321 vs 2474. Cubic-under for
   scale: 203–1615 ms p50.
3. **Stability**: Copa-sole never DNF'd and never entered plain-B's c3
   collapse mode — sc3 σ_s 0.63/0.74 vs B's bimodal 6.51 (3/8 runs at
   ~2.75); C8 σ 4.1–8.2 vs B 9.5–10.6 (historic plain-B C8 was outright
   bimodal σ 25, 2.5–69).
4. **Aggregation preserved**: C7 = ×1.98/×1.64 of C's own single (per-path
   Copa fills each pipe independently; B aggregates ×1.27/×1.32 over its
   single). C8 = 0.94–0.98× of C's single (parity with its own fast-alone,
   like every substrate before it).
5. **Ownership demonstrated**: the substrate window followed Copa's cwnd end
   to end (est=Y everywhere, fast-path anchor honest ×1.0–1.05, no
   loss-reactive collapse with GE loss present) — the min()-coupling is
   gone; what remains is Copa's own operating point.

### Caveats / deployment

- **Competitive-mode gap (deployment caveat)**: Copa-lite has NO
  TCP-competitive mode (Copa §4 mode switching was deliberately not built —
  out of scope for this build). Against loss-based cross-traffic on a shared
  bottleneck a delay-based controller yields; no cross-traffic cell was
  measured. `passthrough` is an experiment knob; **BBR-under remains the
  bulk-throughput reference and the sensible default-fallback**; shipped
  default remains stock Cubic (unset), byte-identical.
- **Slow-path anchor still over-reads ~×4 under the feed at c3** (btlbw 8 547
  vs true ~2 083; rej[iv] high — decode/ack-burst clocked): the send-interval
  sampler needs a paced wire to be exact, and c3's ack cadence is bursty. It
  no longer matters for the window (cwnd, not the anchor, owns the rate) but
  is recorded for the record.
- **The echo-RTT conflation is structural**: any app-layer CC over a buffered
  substrate reads its own reservoir as queue. D shows the reservoir gain is
  the right lever if Copa-sole throughput ever matters; flipping
  RWM_STORE_GAIN's default is NOT done here (it also affects legacy plain).
- **Ack-only / pre-feed connections** keep the 256 KB static window (no Copa
  feed on that direction) — fine for control traffic; a bulk reverse flow
  gets a feed of its own by symmetry.
- n<8 arms: the known seed-7 GE topo-ping abort (driver retries once; double
  aborts drop the rep) — all n recorded, all losses accounted (9 aborted
  invocations = the 9 diag files without a config echo).

### Tests

`cargo test -p raptorpath --lib` 306/306 (new: 4 pass-through shim — window
follows the atomic, handshake not starved + zero-write floor, clone shares
the atomic, congestion events are recorded no-ops; 4 CopaFeed — exactly-once
frontier/SACK attribution, dedupe, retransmit path-reassignment, bounded
per-ack work); `-p raptorpath-math` pass; gate_suite 15/15 release (shipped
default untouched); new `copa_sole_loopback` end-to-end guard (passthrough +
feed over real QUIC). L0 shim smokes recorded above (`RWM_RS_TRACE`).

## Copa Wire-Signal (2026-07-13) — the #80 bulk gap CLOSED where it was named: wire-clocked delay term + hint→δ mapping + Copa's real update law take Copa-sole from 0.4× to 0.86–0.89× BBR-under at single-c2 and to PARITY at C8 (1.01×/0.95× with σ 3.7/1.6 and a ×18–25 tighter slow-path queue); arm-D's reservoir sensitivity is GONE; residuals: C7 0.73–0.76× (recovery-idle, named) and a pre-existing CROSS-ARM receiver-side frontier wedge (~60 s, forensics recorded) that C1's larger operating point triggers more often (branch `feat/copa-wire-signal`)

*Decision record: → [ADR-0062](adr/0062-copa-wire-signal-competitive-mode.md)*

Follow-up to "Copa-Sole Substrate CC" (#80), which named the bulk-gap
mechanism: Copa's delay term was fed the APP-LAYER ECHO RTT — including the
sender's own store/reservoir dwell in quinn's datagram queue — so Copa backed
off against self-inflicted delay that is not in the network (arm D proved the
term). This build fixes the SIGNAL and maps bulk onto Copa's δ.

### The fix (paper §12.4 wire-signal addendum; all gated)

1. **Wire clock**: Copa's queue signal is quinn's packet-timed path RTT
   (`Connection::rtt()` per path = per connection; ack-delay corrected;
   measured BELOW the datagram queue ⇒ the sender's own reservoir dwell is
   structurally excluded). The app-echo RTT stays with the
   LossEstimator/ARQ machinery. d_q = wire_standing − wire_RTTmin(10 s) −
   2·jitter, where wire_standing is the LATEST smoothed sample (Copa's
   RTTstanding) and the floor is the RAW 10 s min.
2. **hint→δ (no new constants)**: δ is the latency price in Copa's utility
   U = log(tput) − δ·log(delay); the hint's one declared price ratio is its
   tail-loss scale ζ ∈ {0.01, 1, 100} (`ProtocolHint::tail_loss_scale`).
   δ(hint) = 0.5/ζ ∈ {50 Realtime, 0.5 Auto, 0.005 Bulk}; equilibrium queue
   = 1/δ packets. `RWM_COPA_DELTA` overrides (the frontier knob).
3. **Copa's actual update law** (wire mode only): direction = cwnd/srtt vs
   1/(δ·d_q), step v/δ per SRTT, velocity doubles after a ≥3-update
   same-direction streak (Copa §2.2), down-steps capped at the measured
   queue μ̂·d_q; plus a **coupling cap** cwnd ≤ BDP+2/δ and **CC-rate source
   pacing** default-ON (`RWM_CC_PACE`, aggregate-correct: Σ cwnd_i/SRTT_i
   over live paths, ceiling gen_rate × path count).
4. **Gates**: active iff `RWM_QUIC_CC=passthrough` or `RWM_COPA_FEED=1`;
   `RWM_COPA_WIRE=0` reproduces #80's app-echo arm byte-for-byte; env fully
   unset ⇒ shipped stock-Cubic path byte-identical (gate_suite 15/15).

### The diagnosis chain (each step MEASURED at L1 before the next)

- v1 (wire clock + δ + windowed-min signal): sc2 53.4 vs B 77.7 — cwnd
  ratcheted to MAX_CWND: the δ-sawtooth's drain trough falls inside every
  update window, so a per-window MIN reads "queue empty" every update →
  standing-sample signal (Copa's RTTstanding).
- v2: cwnd still 4 000–7 800 vs the ≈300 fixed point — above the outstanding
  store cap the delay signal is DECOUPLED (queue no longer grows with cwnd)
  and the jitter-clamped d_q votes up forever; window/RTT bursts tail-drop
  the 1 000-packet qdisc → coupling cap BDP+2/δ.
- v2b: store cap flapping 1024↔128 — the Σcwnd store cap summed over
  `active_paths()`, whose spare-capacity filter drops a SATURATED path (the
  normal state of a wire-bound sender) → `live_paths()`.
- v3 (smoke 55.7→67): Copa assumes a PACED wire; under passthrough quinn's
  pacer derives from the engine window (≈5×BDP at Bulk's δ) and never binds
  — pure ack-clocking lets every GE recovery micro-stall idle the bottleneck
  → CC-rate source pacing default-ON under the wire signal.
- v3 battery: C7 = ×1.00 of own single — the pace ceiling (gen_rate = 9 000
  sym/s ≈ 90 Mbit) is a single-link burst guard that clamped the TWO-path
  aggregate, and Σcwnd/max(SRTT) under-reads heterogeneous aggregates →
  aggregate-correct rate + per-path-scaled ceiling (v4).

### L1 battery v4 — PLAIN mode (VM 10.1.5.16, 2026-07-13 ~14:33–16:00 UTC; binary sha256 ed81395d…, commit e6b0cf2; 25 MB × 1 run/invocation × 8 reps, arms interleaved round-robin per rep, fresh tunnel per invocation, seeds 42 AND 7, RWM_DIAG=1 everywhere; logs `/home/vibe/copawire/{sc2,sc3,c7,c8}-s{42,7}.log` + per-run `diag-*.log`; driver `/home/vibe/copawire_battery.sh`; v1–v3 intermediate batteries archived in `/home/vibe/copawire-v{1,3}`)

Arms (all PLAIN, `RWM_GEN=0`, same binary): **B** = `RWM_QUIC_CC=bbr`
(reference) · **C0** = passthrough + `RWM_COPA_WIRE=0` (replicates #80's C)
· **C1** = passthrough (wire signal + bulk-δ + pacing, all default) · sc2
only: **CR** = C1 + `RWM_STORE_GAIN=1.25` (reservoir re-probe) · **F1/F2** =
C1 + `RWM_COPA_DELTA=0.05/0.001` (δ frontier). Liveness: every C1 diag log
carries `feed ACTIVE` + `Copa queue-signal clock: wire=true … delta=0.005
cc_pace=true` + est=Y (stale logs from the known aborted-invocation retries
excluded and listed). Session validated: B sc2 76.5/75.1 (historic 75.9/75.4).

**Throughput (Mbit/s, mean (σ_s) [runs]; W = wedge runs (see Stability),
modal = mean excluding W):**

| cell | B | C0 (app-echo) | C1 (wire) | C1/B |
|---|---|---|---|---|
| sc2 s42 | 76.49 (1.17) | 30.29 (4.69) | **68.09 (1.76)** [68.9 69.0 69.2 64.9 70.0 68.3 68.5 65.9] | **0.89** |
| sc2 s7 | 75.09 (3.20) | 26.76 (2.61) | 56.64 (21.7; 1W) modal **64.27** (1.86, n=7) | **0.86** |
| sc3 s42 | 13.50 (4.58; 1W) modal 15.11 | 8.35 (3.65; 2W) modal 10.29 | **11.83 (0.46)** [11.8 12.2 11.7 11.1 12.3 11.5 11.6 12.5] | 0.78 |
| sc3 s7 | 15.62 (0.28, n=5) | 9.76 (0.85, n=6) | 6.07 (5.10; 5W) modal 12.21 (n=3) | 0.78 |
| c7 s42 | 103.22 (6.57) | 47.71 (11.9) | 57.23 (34.0; 2W) modal **75.24** (7.2, n=6) [73.0 67.8 82.2 81.6 82.4 64.5] | 0.73 |
| c7 s7 | 89.90 (38.6; 1W) modal 104.35 | 53.76 (4.95, n=6) | 69.95 (23.5; 1 partial 23.4) modal **79.26** (n=5) | 0.76 |
| c8 s42 | 54.64 (13.45) | 26.94 (10.8; 1W) | **55.01 (3.70)** [48.6 54.4 57.7 56.5 50.2 57.5 56.4 58.7] | **1.01** |
| c8 s7 | 58.14 (7.10) | 28.13 (6.00, n=6) | **55.30 (1.60, n=6)** [54.1 53.6 56.3 57.5 53.9 56.4] | **0.95** |

Every C1-vs-C0 delta ≫ σ_s (sc2 +38 at σ≤4.7; c8 +27–28 at σ≤10.8). C1 ×2.1
over C0 at sc2, ×2.0 at c8, ×1.5 at c7, ×1.2 at sc3.

**Queue distributions (per-path, sender DIAG lines 4+, wedge/stale logs
excluded and pooled per arm; TWO clocks: appQ = app-echo p50 − rtp p50 (the
consumer-experienced pipeline incl. the sender's own reservoir), wireQ =
quinn packet-timed p50 − rtp p50 (the NETWORK standing queue)):**

| cell/path | B wireQ (appQ) | C0 wireQ (appQ) | C1 wireQ (appQ) |
|---|---|---|---|
| sc2 s42 | 38 (48) | 4 (19) | **5 (35)** |
| sc2 s7 | 38 (70) | 4 (19) | **6 (42)** |
| sc3 s42 | 221 (315) | 11 (75) | **43 (361)** |
| sc3 s7 | 2 (50) [n big, mixed] | 10 (73) | 35 (427) (n=229, 3 runs) |
| c7 s42 p0/p1 | 14/28 (18/32) | 5/6 (31/29) | **4/7 (24/41)** |
| c8 s42 fast/slow | 26/**124** (37/268) | 6/14 (28/74) | **7/5 (73/9)** |
| c8 s7 fast/slow | 29/**88** (40/158) | 5/11 (20/114) | **5/3 (30/7)** |

- **The network-queue advantage is KEPT and widened at c2-class paths**: C1
  wireQ 4–7 ms vs B's 38 ms at sc2 (p90 17–18 vs 87–88) — Copa holds the
  wire queue ~6× tighter while giving up only 11–14% of B's throughput.
- **C8 is the showcase**: C1 ≥ B's throughput with slow-path wireQ 3–5 ms vs
  B's 88–124 ms (appQ 7–9 vs 158–268) — a ×18–25 tighter standing queue AT
  parity throughput, and σ collapsed (3.7/1.6 vs B's 13.5/7.1).
- **Honest c3 caveat**: C1's Bulk-δ tolerates 1/δ = 200 symbols ≈ 96 ms at
  c3's 2 083 sym/s; measured wireQ 35–43 ms sits between C0's 10–11 and
  B-s42's 221, and the app-layer reservoir dwell (2×cwnd store) grows to
  ~360–430 ms p50 — deeper than C0's 73–75. At c3 the δ-map trades queue
  for throughput exactly as designed (δ is the knob; a c3-tight profile
  is `RWM_COPA_DELTA` upward). c7-s7 B/C0 queue rows are polluted by
  stale logs of aborted invocations (frozen rtp ≈ 78 ms) and not tabled.

**RTT-floor freshness (the DIAG the wire law needs)**: per-path rtp p50 =
10–12 ms on every c2 path (netem base 10) and 40–44 ms on every c3 path
(base 40), min 7–9/33–44, in every clean C1 run — the ±v/δ dither refreshes
the raw 10 s min without ProbeRTT, under a standing Bulk queue. (In WEDGE
runs the floor goes stale with the estimators frozen — part of the wedge
signature below.)

**Reservoir re-probe (arm-D term resolved)**: CR (reservoir 1.25×cwnd) =
64.75 (1.30) / 60.96 (4.35) vs C1 68.09 / 64.27 — the #80 sensitivity
(+13–23% AND tighter queue from SHRINKING the reservoir) is GONE; the
residual is −5% (less recovery runway), i.e. the store dwell no longer
pollutes d_q. No second self-queue term found.

**δ frontier (sc2, C1 = δ 0.005 vs F1 = 0.05 vs F2 = 0.001):**

| δ | tput s42/s7 | wireQ p50 | appQ p50 |
|---|---|---|---|
| 0.05 (queue-tight) | 42.4 (16.6; 1W) / 41.3 (9.6) | 4–5 | 16 |
| **0.005 (Bulk map)** | **68.1 / 64.3** | 5–6 | 35–42 |
| 0.001 (queue-deep) | 66.8 (1.1) / 65.1 (2.3) | 6 | 47–57 |

The knee is AT the hint-mapped δ = 0.005: ×10 tighter δ costs −38%
throughput; ×5 deeper δ buys nothing (−1.9%, ≈σ) and only deepens the
reservoir. (Deeper still collapses: δ ≤ 0.0005 re-enters the decoupled
regime — cap > store latch — measured 3.1 Mbit at the v2 smoke.)

### Stability — the wedge (honest; the one #80 property NOT kept)

#80's C had zero collapse runs. This session a ~2.2–3.3 Mbit/s collapse mode
appears in ALL arms — **B 2/59 runs (sc3-s42 2.22, c7-s7 3.23), C0 3/57
(sc3-s42 ×2, c8-s42), C1 7/59 + 1 partial** (sc2-s7 ×1, sc3-s7 ×4 (+1 run at
1.46), c7-s42 ×2; ZERO at sc3-s42 and both c8 seeds) — so it is a
pre-existing transport failure mode (it is plain-B's historic c3/C8 collapse
value), NOT introduced by the wire signal, but C1's larger operating point
(cwnd ≈ BDP+200 vs C0's ≈BDP+30) triggers it ~3× more often, clustered by
seed×cell. Forensics (captured live, `/home/vibe/wedge-c.log`, plus the new
`sweeps/retx/gapdrop/nbud` DIAG counters): transfer freezes at good=0 with
store full, in_flight 0, cwnd healthy (482), nack budget healthy; the sender
resends the cumulative blocker ~1/SRTT for 50+ s (retx 7 067→7 647 in 5 s of
wedge) and quinn-level ACKs keep the wire RTT fresh (wrtt 42–44 ms ≈ base) —
the retransmits ARE delivered to the receiver host, yet the receiver's
in-order frontier never advances; self-resolves at ~55–65 s. i.e. a
RECEIVER-SIDE delivery/frontier deadlock. Naming it further needs
receiver-side instrumentation — follow-on task, coordinator's call; it is
the top blocker for making C1 the substrate default.

### VERDICT

1. **The named mechanism is CLOSED**: with the CC delay term wire-clocked,
   Copa-sole bulk goes 0.40→0.86–0.89× BBR-under at sc2 (Δ ≫ σ), 0.95–1.01×
   at C8, 0.73–0.76× at C7, 0.78× at sc3 — and the reservoir-sensitivity
   probe confirms the self-queue term is gone.
2. **The queue claim survives the throughput**: the NETWORK queue stays
   4–7 ms p50 at c2-class paths (B: 38) and 3–7 ms on the C8 slow path
   (B: 88–124) — C1 did not buy throughput with B's bufferbloat. At c3 the
   δ-map spends real queue (35–43 ms wire; deep app reservoir) — the
   documented, tunable trade.
3. **C8 het-multipath is the first cell where Copa-sole strictly DOMINATES
   BBR-under**: ≥ parity throughput, ×18–25 tighter slow-path queue, σ
   collapsed, zero wedges in 14 runs.
4. **Residual gaps, named**: (a) C7 0.73–0.76× — C1 aggregates ×1.11–1.23 of
   its own single vs B's ×1.35–1.39; the same recovery-idle mechanism the
   pacing fix attacks remains partially open on dual paths (the modal runs
   reach 82 = 0.79×B; velocity/probe dynamics are NOT the residual — the δ
   frontier is flat above the knee). (b) sc3 0.78× — the c3 wire queue
   needed for parity exceeds what the anchor's ×4 slow-path over-read can
   steer precisely. (c) The receiver-side wedge above.
5. Copa-sole under passthrough is now a credible substrate default for
   HETEROGENEOUS multipath (C8-class) and a queue-first single-path choice
   at 86–89% of BBR's bulk; the wedge fix is the gate to flipping any
   default. BBR-under remains the bulk-throughput reference; shipped
   default remains stock Cubic (unset), byte-identical.

### Env / commands (reproduction)

```
# C1 (the fix, all defaults):  RWM_GEN=0 RWM_QUIC_CC=passthrough
# C0 (#80's arm):              RWM_GEN=0 RWM_QUIC_CC=passthrough RWM_COPA_WIRE=0
# reference:                   RWM_GEN=0 RWM_QUIC_CC=bbr
# knobs: RWM_COPA_DELTA=<f>   (δ override; frontier)
#        RWM_CC_PACE=0        (disable pacing under wire signal)
#        RWM_STORE_GAIN=<f>   (reservoir; re-probe arm)
sudo env SEED=42 RWM_DIAG=1 <arm-env> bash perf_rwm_c.sh c2 c2 bulk 25000000 1 single
```

### Tests

`cargo test -p raptorpath --lib` 313/313 (new: wire gate decision fn, hint→δ
mapping incl. override, two-clock separation — Copa keys on the wire feed
while the estimator holds a 500 ms app echo, velocity law with 3-update
hysteresis + queue-capped drain, coupling cap, legacy byte-identity);
`-p raptorpath-math` pass; gate_suite 15/15 release on the final tree
(shipped default untouched); `copa_sole_loopback` e2e (passthrough + wire
defaults over real QUIC); `congestion_control` 19/19.

## Frontier Wedge (2026-07-13) — the cross-arm c3/C8 ~60 s "collapse run" ROOT-CAUSED and FIXED: not a receiver frontier bug at all — quinn's PMTU BLACK-HOLE DETECTOR misreads a GE all-large loss burst as an MTU black hole, resets the path MTU to 1200 < the 1279-byte symbol datagram, and EVERY data send (incl. every retransmit of the blocker) fails sender-side with `TooLarge` for exactly `black_hole_cooldown` = 60 s; fix = `min_mtu = initial_mtu = 1350` (the engine's real floor), deterministic same-binary repro 63.5 s → 5.8 s (branch `fix/frontier-wedge`)

*Decision record: → [ADR-0055](adr/0055-mtu-floor-1350.md)*

Follow-up to "Copa Wire-Signal" (#82), whose battery recorded the collapse
mode in EVERY substrate arm (B 2/59, C0 3/57, C1 7/59 + 1 partial) and
captured live sender forensics (`/home/vibe/wedge-c.log`). The working
hypothesis going in — a RECEIVER-side frontier/dup-filter wedge — is
REFUTED by the evidence below; the receiver's frozen frontier is the
symptom, the sender's silent MTU collapse is the disease.

### The forensic chain (each step from the captured wedge log / quinn source)

1. **The wedge window is a total data blackout, not a frontier stall**: in
   `/home/vibe/wedge-c.log` (sc3-s7 C1, wedge t≈14→74 s) the sender's
   app-echo SRTT decays 661→54 ms in a geometric ×0.917-per-2 s staircase —
   exactly ONE fresh RTT sample per REPORT_INTERVAL (2 s). The only 2 s-cadence
   RTT feed is the PathReport circular echo (net/mod.rs `PathReport` arm →
   `record_rtt`) riding the reliable CONTROL STREAM; per-batch data `Ack`s —
   which would sample at 100+/s — are absent for the whole window, i.e. the
   receiver processed ~ZERO data batches for ~59 s, while quinn-level acks
   kept `wrtt` fresh at 41–45 ms (small control datagrams still crossing).
2. **The sender's sends were failing, not being ignored**: the same log
   holds **8 077 `failed to send NACK retransmission … e=datagram too
   large` WARNs at ~130/s from 14:29:06 to 14:30:06** — every targeted
   retransmit of the receiver-advertised holes AND every window repair for
   exactly 60 s. `diag_retx`/`sweeps` kept rising because they count
   attempts (budget/cooldown bookkeeping precedes the send result).
3. **The mechanism, in quinn-proto 0.11.14 source**: every wire symbol is
   ONE ~1279-byte QUIC datagram (measured: `mtu_floor_covers_symbol_batch`);
   quinn's defaults are `initial_mtu = min_mtu = 1200`, so symbol datagrams
   are only sendable because post-handshake PMTUD raises `current_mtu` to
   ~1452. quinn's `BlackHoleDetector` calls a loss burst "suspicious" iff it
   contains no packet smaller than min_mtu / smaller than a more recently
   acked packet — a GE burst on a bulk wire (where essentially every packet
   is a 1305-byte symbol datagram) matches by construction. At
   BLACK_HOLE_THRESHOLD suspicious bursts it resets `current_mtu :=
   min_mtu` (1200) and pauses discovery for `black_hole_cooldown` (default
   **60 s**). `max_datagram_size` (~1170) < 1279 ⇒ `SendDatagramError::
   TooLarge` on every symbol until the cooldown expires and PMTUD re-probes.
4. **Every observed property follows**: cross-arm (below the CC layer — B,
   C0, C1 all ride the same datagram path); loss-timing-dependent
   (needs enough all-large suspicious bursts; C1's larger operating point
   ⇒ longer tail-drop bursts ⇒ ~3× the trigger rate); ~60 s self-resolve
   (the cooldown, to the second); wire acks fresh + path alive (control
   datagrams are small and unaffected); the historic 2.2–3.3 Mbit/s
   "collapse throughput" is just 25 MB / (60 s + normal transfer time);
   and the wedge NEVER reproduced under the L0 netem shim because the shim
   drops ABOVE quinn's packet layer — quinn never sees the large-packet
   loss pattern (the fidelity boundary, now proven load-bearing).

### The deterministic reproduction (tests/mtu_blackhole_wedge.rs, new)

An in-process lossy UDP proxy drops every UDP payload ≥ 1280 bytes for a
3-second window mid-transfer (a REAL transient MTU black hole below quinn;
QUIC Initials are padded to exactly 1200 and pass). 8 MB plain reliable
window transfer over real QUIC loopback, same binary, same hole:

| arm | env | elapsed | mean rate |
|---|---|---|---|
| stock quinn MTUD | `RWM_MTU_FLOOR=0` (`RWM_WEDGE_CONTROL=1` test arm) | **63.5 s** | 1.0 Mbit/s |
| MTU floor (fix, default) | — | **5.8 s** | 11.1 Mbit/s |

63.5 s = 3 s hole + ~60 s cooldown + transfer: the collapse run, on demand.

### The fix (`QuicTransport::apply_mtu_floor`, transport/quic.rs — ships default-ON)

`min_mtu = initial_mtu = 1350` on both client and server transport configs.
The engine structurally REQUIRES ~1279-byte datagrams (a symbol is never
fragmented), so 1350 (= 1279 + ~33 QUIC 1-RTT overhead + margin) is a
requirement the code already had, now declared to quinn: a black-hole reset
lands AT the floor and symbol sends keep working; PMTUD upward probing and
the black-hole detector stay active (quinn's 60 s cooldown remains as its
own safety net — it just can't take our datagrams below the floor). This is
a transport-correctness fix in the shipped path and ships UNCONDITIONALLY
(the wedge exists in stock-Cubic mode too); `RWM_MTU_FLOOR=<n>` overrides,
`=0` restores stock quinn (the control arm). A path that truly cannot carry
1350-byte UDP payloads could never carry a symbol anyway — before the fix
that failed as a silent 60 s-cycle send blackout, now it fails loudly as
persistent large-packet loss. Config echo (`MTU floor: …`/`MTU floor OFF`)
logged at every endpoint creation.

Receiver-side wedge DIAG added while falsifying the frontier hypothesis
(kept, RWM_DIAG-gated): when the in-order frontier is frozen > 1 s the
reliable hole-refresh arm prints `[WEDGE]` once per second with the blocker
seq's decoder state (`seq_probe`: seen-as-source / recovered / output — the
dup-filter signature probe), reorder-buffer pending, intake batches/s, and
quinn DATAGRAM frame rx/tx per path (`datagram_frame_stats`) — the line
that discriminates "retransmits eaten by the decoder" from "retransmits
never arrive" in one read.

### L1 battery (VM 10.1.5.16, 25 MB × 1 run/invocation, arms interleaved per rep, fresh tunnel per invocation, seeds 42+7, RWM_DIAG=1 everywhere; binary sha256 9ec7eef8…; logs `/home/vibe/wedgefix/`; driver `/home/vibe/wedge_battery.sh`)

Arms: **B** = plain + `RWM_QUIC_CC=bbr` (fix on) · **C1** = plain +
passthrough (fix on) · **C1o** = sc3 only: C1 + `RWM_MTU_FLOOR=0`
(same-binary stock-quinn control). Per-run forensics: `toobig` (sender
TooLarge count) + `[WEDGE]` receiver lines + MTU config echo both sides.

**Throughput (Mbit/s, mean (σ_s) [n]; W = collapse runs < 5 Mbit/s; historic
= the v4 battery same cell/arm):**

| cell | B (fix) | B historic | C1 (fix) | C1 historic | C1o (stock, control) |
|---|---|---|---|---|---|
| sc3 s42 | **15.60 (0.52) [8] W=0** | 13.50 (4.58; 1W) | **11.99 (0.44) [8] W=0** | 11.83 (0.46) | 11.82 (0.73) [8] W=0 (2 runs toobig=1 — black-hole flirts) |
| sc3 s7 | **15.87 (0.39) [8] W=0** | 15.62 (0.28, n=5) | **12.26 (0.18) [6] W=0** | 6.07 (5.10; **5W**) modal 12.21 | 11.00 (3.70) [7] **W=1**: 2.63 Mbit / 76.1 s, toobig=6127, 58 [WEDGE] lines |
| c8 s42 | 61.90 (8.55) [8] W=0 | 54.64 (13.45) | **54.89 (3.23) [8] W=0** | 55.01 (3.70) | — |
| c8 s7 | 53.09 (8.78) [7] W=0 | 58.14 (7.10) | **54.22 (7.40) [7] W=0** | 55.30 (1.60, n=6) | — |
| sc2 s42 (spot) | 75.46 (3.73) [4] | 76.49 (1.17) | 66.37 (0.99) [4] | 68.09 (1.76) | — |

- **Collapse count, fix arms: 0/68 runs** (B+C1 pooled over all cells)
  vs the historic pooled incidence 9/118 (B 2/59 + C1 7/59, ≈7.6%);
  P(0/68 | p=0.076) ≈ 0.005 — and the SAME-DAY stock control reproduced the
  collapse (sc3-s7 C1o r7: 2.63 Mbit/s, 76.1 s — the historic collapse value
  exactly) with the full mechanism signature: 6 127 sender TooLarge + 58
  receiver [WEDGE] lines reading `blocker seen_src=false recovered=false
  … batches/s=0 syms/s=0` for 59.9 s (intake ZERO, blocker NEVER arrived —
  the dup-filter/receiver hypotheses refuted in the same line that proves
  the send blackout) + self-resolve at the 60 s cooldown. Two more stock
  runs show toobig=1 flirts (detector armed, single failed send).
- **The C1 sc3-s7 cell is the cleanest before/after**: 6.07 (σ 5.10, 5/8
  collapsed) → 12.26 (σ 0.18, 0/6) — the mean doubles to exactly the
  historic modal value and σ collapses ×28; the fix removed the collapse
  mode and touched nothing else (C1o non-wedge runs average 12.28 ≈ C1 —
  the floor costs nothing when the detector stays quiet).
- B's sc3 bimodality is likewise gone (s42: 13.50 σ4.58 1W → 15.60 σ0.52).
- **No regression in clean cells**: sc2 B 75.5 vs 76.5, C1 66.4 vs 68.1
  (≈1σ_s); c8 at historic levels both arms.
- Integrity: several invocations aborted in < 7 s with no result (known
  harness BUSY/setup race, listed per cell in the logs as RETRY/topup) —
  they started no transfer and cannot mask a collapse (a collapse run
  COMPLETES at ~76 s with a low-mbps result); n per arm reported honestly.

### VERDICT

1. **The mechanism is PROVEN, not conjectured**: quinn PMTU black-hole
   reset below the symbol datagram size, cooldown-timed. Every link of the
   chain is measured — sender TooLarge storm (8 077 historic / 6 127 wild
   control), receiver intake zero with the blocker never-seen ([WEDGE]
   probe), 60 s self-resolve matching `black_hole_cooldown`, deterministic
   same-binary repro (63.5 s stock vs 5.8 s floor), and a wild same-day
   control-arm reproduction under netem GE.
2. **The fix kills the collapse mode**: 0/68 fix-arm collapse runs across
   the historically wedge-prone cells (vs 7.6% pooled baseline, p≈0.005),
   C1 sc3-s7 σ ×28 tighter at the historic modal mean, no clean-cell
   regression. It ships default-ON (transport correctness; the wedge
   exists under stock Cubic too); `RWM_MTU_FLOOR=0` preserves the stock
   arm for reproduction.
3. **The Copa-sole default-flip blocker named in #82 is CLEARED**: the
   "receiver-side frontier wedge" was never a frontier bug and is fixed
   below the CC layer, for every arm. The remaining Copa-sole gaps (C7
   aggregation 0.73–0.76×, sc3 0.78×) are unchanged and already named.
4. The L0 netem shim's fidelity boundary is now proven load-bearing: the
   shim drops above quinn's packet layer, so quinn-level pathologies
   (CC, PMTUD) are structurally invisible at L0 — quinn-loop hypotheses
   must be tested with sub-quinn loss (the new UDP-proxy repro) or at L1.

### Tests

`cargo test -p raptorpath --lib` (all pass, incl. the untouched frontier /
reorder / SACK machinery); `-p raptorpath-math`; gate_suite 15/15 release;
`mtu_blackhole_wedge` (new): `mtu_floor_covers_symbol_batch` (hard size
invariant: max repair batch = 1279 B ≤ 1305 B floor budget) +
`mtu_black_hole_does_not_wedge_transfer` (fix arm, CI: 3 s hole → completes
< 40 s; control arm via `RWM_WEDGE_CONTROL=1`: asserts the 60 s wedge
reproduces, 63.5 s measured); `copa_sole_loopback` + `congestion_control`
unchanged.

## Hardware-Honest Re-Baseline + Receiver Parallelization (2026-07-14) — the bulk N× test on real silicon: AES-NI moved the CPU but NOT ONE WALL (the "single-thread receiver ceiling ~93–104" attribution is REFUTED — the engine sinks 187.7 Mbit/s single-path); the true C7/C8 binder is the plain-reliable OUTSTANDING POOL not scaling with path count (`win=1024/1024` pegged), and the path-scaled pool (`RWM_STORE_PATHS`) takes C7 plain+BBR 100→136/142 (×1.72/×1.89 of the same-session single, 0.86/0.94 of Σ singles) and C8 65/56→75.8/72.3 with σ halved; receiver parallelization NOT built — the profile refutes it (task #84, branch `feat/recv-parallel`)

*Decision record: → [ADR-0057](adr/0057-profile-before-parallelize.md) + [ADR-0058](adr/0058-path-scaled-outstanding-pool.md)*

════════════════════════════════════════════════════════════════════════
**HARDWARE DIVIDE.** Every L1 number in the sections ABOVE this line was
measured on a qemu64-model vCPU (`QEMU Virtual CPU 2.5+`: SSSE3 only — no
AES-NI, no AVX2, no PCLMULQDQ), i.e. quinn's TLS did SOFTWARE AES-GCM on
every packet. The VM is now host passthrough — **Intel Xeon E5-2650 v3,
6 cores, `aes avx2 pclmulqdq` live** (confirmed at battery start; `lscpu`
recorded in every log header). Numbers below this banner are the honest
hardware; any cross-banner comparison must name the divide.
════════════════════════════════════════════════════════════════════════

Task #84 — the user's bulk N× claim: bulk mode is not latency-constrained,
so multipath ARQ striping should approach N× the per-path rate (the
resequencing buffer absorbs skew; mid-transfer losses recover in parallel;
only the tail pays serially). Historic C7 sat at ×1.35, "pinned exactly at
the single-thread receiver wall ~93–104", so the task shipped with a
receiver-parallelization plan. Measure first, build what the numbers demand
— the numbers demanded something else entirely. Paper §16.19.

### STEP 0 — re-baseline (VM 10.1.5.16, 2026-07-13 21:35–22:00 UTC; binary sha256 84a1f014… = main aba1a52 unmodified; 25 MB × 1 run/invocation × 8 reps, arms interleaved round-robin per rep, fresh tunnel per invocation, seeds 42+7, wedge fix ON = shipped default, `cod>0` GUARD OK on every gen run, full env+command per run in `/home/vibe/recvpar/step0/*.log`, driver `/home/vibe/rebase_battery.sh`)

Arms: **PB** = plain+`RWM_QUIC_CC=bbr` · **C1** = plain+`RWM_QUIC_CC=
passthrough` (Copa wire-signal defaults) · **GS** (sc2 only) = generation
systematic-repair on the GPB stack (`RWM_GEN_R=0.03 RWM_GEN_PIPE=1
RWM_QUIC_CC=bbr --window-systematic-repair`).

| cell | PB new (σ_s) | PB qemu64 | C1 new (σ_s) | C1 qemu64 | GS new (σ_s) | GS qemu64 |
|---|---|---|---|---|---|---|
| sc2 s42 | 78.08 (3.09) | 76.1–77.1 | 67.96 (1.97) | 68.1 | **75.71 (2.82)** | 70.9 |
| sc2 s7 | 75.85 (1.97) | 72.6–77.6 | 66.27 (0.89, n=7) | 64.3 | **75.46 (1.04, n=7)** | 70.8 |
| sc3 s42 | 15.74 (0.53) | 15.60 | 11.65 (0.51) | 12.0 | — | — |
| sc3 s7 | 15.70 (0.41, n=6) | 15.87 | 12.21 (0.15, n=5) | 12.3 | — | — |
| c7 s42 | 102.29 (4.00) | 93.1–103.2 | 82.90 (3.18) | 75.2 modal | — | — |
| c7 s7 | 100.17 (7.69, n=5) | 94.5–104.4 | 81.83 (2.79, n=7) | 79.3 modal | — | — |
| c8 s42 | 46.47 (5.69) | 54.6–61.9 | 51.48 (7.93) | 54.9–55.0 | — | — |
| c8 s7 | 48.73 (8.09, n=7) | 45.4–58.1 | 57.09 (2.16, n=5) | 54.2–55.3 | — | — |

**CPU per 25 MB invocation (recv·send), new vs qemu64:** PB sc2 1.99·1.26
vs 2.97·2.02 (recv −33 %, send −38 %); GS sc2 2.36·1.52 vs 3.38·2.25
(−30 %/−32 %); PB sc3 2.26·2.10 vs 3.54·3.17.

**The finding: hardware crypto moved the CPU and not one wall.** Every
plain/Copa cell replicates its qemu64 value inside the recorded
session-to-session spread (PB C8, the documented bimodal arm, drifts within
its historic 29–76 envelope). The only real mover is gen-sys single (+4.8 →
**0.97×/0.995× of PB's own single** — on AVX2 the whole FEC machine now
costs ~0.37 s recv CPU per 25 MB over plain; the coding tax is essentially
free). And the divide itself is an instrument: a CPU-bound wall must move
when the CPU gets ~35 % faster per byte. C7's "~93–104 receiver wall"
reproduced at 100–102 — **the qemu64-era receiver-wall attribution is
refuted by the hardware upgrade it failed to react to.**

### STEP 1 — profile: the C7 wall is NOT the receiver engine

(200 MB single-run probes at C7 PB, VM 22:00–22:50 UTC; `perf record` dwarf
+ per-thread CPU + affinity pinning; artifacts `/home/vibe/recvpar/profile/`.)

1. **Flat profile.** At C7 = 104.9 Mbit/s the receiver process burns 1.12
   cores / sender 0.89 — but the samples are FLAT: top symbol 3.9 %
   (`__ieee754_exp_fma`), estimator math ~11 %, allocator ~7 %,
   `WireMessage::deserialize` 2.7 %, decoder+GF(256) ~4.5 %,
   `_aesni_ctr32_ghash_6x` **1.3 %** (crypto is now noise), spread evenly
   over all 6 tokio workers. No stage to parallelize.
2. **Pinning kills the CPU hypothesis.** Server pinned to ONE core: 95.5
   Mbit/s (−8 %) at 0.66 core busy. Client pinned: 96.6. Neither side is
   CPU-starved at the default operating point (the unpinned 1.1 cores is
   ~⅓ scheduler/migration overhead).
3. **The engine sink ceiling is 187.7 Mbit/s** — single-path c1 (1 Gbit,
   GE 0.1 %) PB runs 187.7 (dual-c1 185.3, same wall) with the receiver at
   ~1.05 cores. The single-threaded receive/reassembly/delivery task sinks
   ~1.9× the C7 wall on this hardware. C7's limiter cannot be the engine.
4. **Not the frontier either**: C7 with OOO delivery = 105.6 ≈ in-order 103.
5. **The sender DIAG names it**: `win=1024/1024` PEGGED — the plain-reliable
   OUTSTANDING pool at its ceiling — with `infl=0` idle spikes on both
   paths and np flapping 2→1. `RELIABLE_STORE_MAX` = 1024 symbols ≈ 1.28 MB
   is a per-TRANSFER constant; the delay-based dynamic cap (2·Σ anchor-BDP)
   latches at it on fast paths because the legacy ack-interval anchor
   over-reads ×7 (§16.13). The pool that must fund Σ_paths (BDP + one
   recovery round × aggregate rate) does not scale with path count:
   C7 at the pegged pool is Little's law, 1024·1250 B·8 / ~80–100 ms
   echo-RTT ≈ 100–128 Mbit/s — **CPU-invariant, which is exactly why the
   "wall" survived the hardware upgrade and every historic CPU lever.**
6. **Same-binary static-store proof** (PB, s42, 100 MB, `RWM_STORE`):

| RWM_STORE | sc2 | sc3 | C7 | C8 |
|---|---|---|---|---|
| default (1024 latch) | 76–78 | 15.7 | ~103 | ~47–65 |
| 2048 | 81.6 | 14.0 | 122.7 | 51.8 |
| 4096 | 75.6 | 12.0 | **141.3** | **71.5** |
| 8192 | **43.0 COLLAPSE** | — | 143.7 (sat.) | **31.8 COLLAPSE** |

The knee is ≈**2048 outstanding symbols per live path**; deeper static
pools re-enter the documented bufferbloat/recovery collapse (§12), and at
the knee the wall moves exactly as Little's law predicts. Sender side
checked (task requirement): sender CPU is 0.9–1.1 cores at the raised pool
— within ~15 % of the receiver, as historically; neither binds below ~140.

### STEP 2 — the minimal change the profile justifies

**Receiver parallelization was NOT built.** The profile refutes it as the
binder: the engine sink (187.7) exceeds both cell targets (2×sc2 ≈ 152,
Σ-C8 ≈ 92), the pinned receiver runs at 0.66 core, and the flat profile
offers no stage whose parallelization buys anything at this operating
point. An `RWM_RECV_PAR` arm would have measured noise (the
generation-inert-era lesson: dead mechanisms measure session drift).

Built instead — **the path-scaled outstanding pool** (`RWM_STORE_PATHS`,
default OFF = shipped byte-identical; commit 5cace52): for N ≥ 2 live
paths, `cap = clamp(gain·N·pipe_sum, floor, N·2048)` where `pipe_sum` is
the existing dynamic base (Σ anchor-BDP; Σ Copa cwnd under the feed) and
2048 (`RWM_STORE_PATH_POOL`) is the measured per-path knee; **N = 1 keeps
the legacy law bit-exactly**, so singles are unchanged even with the flag
ON (measured below). Mechanism-liveness config echo; unit tests
(`path_scaled_store_cap_*`); harness forwards the knobs.

### STEP 3 — the N× verdict (VM, 2026-07-13 22:52–23:16 UTC; binary sha256 961e377b… = commit 5cace52, SAME binary in every arm; 25 MB × 1 run/invocation × 8 reps, arms interleaved per rep, seeds 42+7, `path-scaled … ACTIVE` liveness echo on every S run and echo=0 asserted on every baseline run; logs `/home/vibe/recvpar/step3/*.log`, driver `/home/vibe/step3_battery.sh`)

Arms: PB / C1 as STEP 0 · **PBS / C1S** = same + `RWM_STORE_PATHS=1`.

**Singles (the N=1 identity control — the flag must be inert):**

| cell | PB (σ_s) | PBS (σ_s) |
|---|---|---|
| sc2 s42 | 78.85 (2.90) | 77.51 (3.12) |
| sc2 s7 | 75.23 (2.80, n=7) | 74.49 (4.61, n=7) |
| sc3 s42 | 15.61 (0.43) | 15.74 (0.36) |
| sc3 s7 | 15.78 (0.19, n=6) | 15.63 (0.31) |

Identity holds everywhere (every Δ ≪ σ_s). Ceilings for the verdict
(same-session Σ of per-path singles): C7 = 157.7 / 150.5; C8 = 94.5 / 91.0.

**Duals (mean (σ_s) [runs]; ratio = arm / same-session single; Σ-ratio =
arm / Σ singles):**

| cell | PB | PBS | PB | PBS | PBS Σ-ratio |
|---|---|---|---|---|---|
| c7 s42 | 100.40 (9.84) [95.6 115.8 80.0 95.1 102.6 107.1 105.0 101.8] | **135.98 (6.91)** [131.7 133.0 145.0 141.6 125.3 132.5 146.2 132.6] | ×1.27 | **×1.72** | 0.86 |
| c7 s7 | 101.19 (13.09) [104.5 111.8 95.1 112.2 110.5 77.1 84.7 113.5] | **142.13 (6.34)** [138.0 146.8 127.3 142.9 146.4 147.4 146.2 142.0] | ×1.35 | **×1.89** | 0.94 |
| c8 s42 | 64.91 (8.87) [51.2 72.1 61.2 76.5 54.7 75.8 67.0 60.9] | **75.77 (4.01)** [75.6 75.7 68.5 81.4 74.4 71.9 80.2 78.4] | 0.69 Σ | 0.80 Σ | **0.80** |
| c8 s7 | 55.90 (14.41) [66.0 38.4 44.6 68.5 39.7 76.3 68.7 44.9] | **72.33 (6.05, n=7)** [75.6 83.6 68.1 63.2 70.9 75.3 69.6] | 0.61 Σ | 0.79 Σ | **0.79** |

| cell | C1 | C1S | C1S / C1-single |
|---|---|---|---|
| c7 s42 | 78.96 (8.63) | **97.58 (5.76)** | ×1.44 (single 67.96) |
| c7 s7 | 74.96 (6.12, n=7) | **113.37 (7.22, n=7)** | ×1.71 (single 66.27) |
| c8 s42 | 53.33 (5.80) | 51.39 (8.31) | — (no change) |
| c8 s7 | 57.37 (1.84) | 56.13 (7.30) | — (no change) |

(C1 singles are the same-day STEP 0 cells; the flag is measured inert at
N = 1 and the C1 code path is untouched by the commit.)

### VERDICT — the bulk N× claim, against the NEW ceilings

1. **C7 plain+BBR: the aggregation unlock is REAL and the claim
   substantially LANDS** — ×1.27/×1.35 → **×1.72/×1.89** of the
   same-session single (0.86/0.94 of Σ singles); Δ +35.6/+40.9 at arm σ_s
   6.3–13. The user's mechanism was right all along — bulk striping WAS
   being serialized by an artificial constraint, just not the conjectured
   one: flow control, not receiver threading.
2. **C8 heterogeneous: real but partial** — 0.69/0.61 → **0.80/0.79 of Σ**
   (Δ +10.9/+16.4) **with σ halved** (8.9/14.4 → 4.0/6.1: the historic PB-C8
   bimodality is largely a store-starvation artifact). The 0.9 target is
   not reached; residual named below.
3. **Copa-sole C7 rides the same unlock**: ×1.16→×1.44 (s42) / ×1.13→×1.71
   (s7) of its own single — the pool was ALSO part of #82's "recovery-idle"
   C7 residual. Copa C8 is unchanged (Copa's own cwnd law, already at
   parity with its single, is the binder there — not the pool).
4. **CPU per bit FELL while throughput rose** (PBS c7 recv 1.98 s at 136
   vs PB 2.14 s at 100): the starved sender was burning cycles idling.
5. **The N× threading hypothesis is answered with instruments, not code**:
   AES-NI freed ~35 % CPU and moved nothing; the engine sinks 187.7 single-
   path; the receiver at C7 idles 34 % of one core. Parallelization would
   become the lever only above ~150–190 Mbit aggregate per sink.

### Residual (named, with evidence)

- **C7 (0.86–0.94 of Σ)**: at the raised pool the operating point is
  saturating — static 4096→141.3, 8192→143.7 (deeper pool buys queue, not
  rate: the pooled flow control's self-queue equilibrium), and the engine
  begins to matter exactly there: server pinned to 1 CPU at pool 4096 =
  125.6 vs 138.8 on 2 CPUs (receiver process 1.33 cores at 141; sender
  1.13). PBS DIAG residual signature: pool episodically re-pegged
  (`win=4096/4096` on ~⅓ of ticks) and np flaps 2→1 (the live-path filter
  drops a saturated path; the dyn cap sags with it: `win=2058/2982`).
  The next TWO levers, in evidence order: per-path outstanding accounting
  (a hole on path A should not starve path B's pool share — the FMTCP
  percap structure, giving the recovery-latency bound per path instead of
  pooled), THEN receiver/sender task parallelization (relevant above ~150).
- **C8 (0.79–0.80 of Σ)**: the shared pool cannot be sized for both paths
  at once — the c3 slow path needs it shallow (its recovery latency scales
  with pool dwell: static 8192 → 31.8 collapse) while the c2 fast path
  wants it deep. Same named lever: per-path accounting.
- The `np` 2→1 flap under saturation (live-path spare-capacity filter) is
  recorded as a contributor at both cells — the v2b copa-sole lesson
  applies to the plain scheduler too.

### Controls / caveats / discipline items

- **Liveness**: `path-scaled outstanding pool ACTIVE` echo on every PBS/C1S
  run; echo=0 recorded on every PB/C1 run (both checked per-run in the
  logs); GUARD OK (cod>0) on every STEP-0 GS run.
- **Noise floor**: claimed C7 effects (+35.6/+40.9) are 3–6× the largest
  same-arm σ_s and >2× the worst cross-session drift observed this session
  (PB-C8 46.5→64.9 across STEP 0→3, the known bimodal arm — which is why
  all verdicts are same-session interleaved A/B only). C8 effects
  (+10.9/+16.4) are 1.2–2.7× the baseline arm's σ_s — smaller, but both
  seeds agree in sign and the PBS arms' σ collapse supports the mechanism.
- **DNFs: zero in 124 STEP-3 runs.** n<8 arms (c7-s7 C1/C1S n=7, c8-s7 PBS
  n=7, sc2-s7 n=7, sc3-s7 PB n=6) are the known seed-7 topo-ping
  double-abort, recorded per log (RETRY lines); no result was discarded.
- **Shipped default byte-identical**: `RWM_STORE_PATHS` unset ⇒ the dyn-cap
  branch is the pre-commit expression verbatim (helper returns None);
  gate_suite 15/15 release + full lib/loopback suites green on this tree.
  The flag is an EXPERIMENT knob; flipping any default (and per-path
  accounting) is follow-on work with its own battery.
- **Static sweep caveat**: `RWM_STORE=n` disables the dynamic cap, so its
  sc3 rows measure static-vs-dynamic, not just depth; the shipped dynamic
  law is the right one for slow singles (binds at ~684 there) — which is
  why the fix scales the DYNAMIC law instead of adopting a static pool.
- **VM lock** `/tmp/rwm-vm.lock` held 21:30–23:5x UTC for the whole session
  (STEP 0 → profile → STEP 3 → DIAG probe), released after teardown; only
  rp-* netns + `pkill -x raptorpath` used; binaries/tree in
  `/home/vibe/rp-recv/`.
- **What this does NOT claim**: no cell exceeds its link-class Σ ceiling;
  C8 remains below target; gen-mode multipath (own structural caps,
  untouched by this flag) was re-baselined at sc2 only; BBR fairness and
  the flag's default remain unevaluated/unflipped.

### Tests

`cargo test -p raptorpath --lib` 316/316 (2 new: `path_scaled_store_cap_is_
legacy_for_singles_and_off`, `path_scaled_store_cap_scales_value_and_
ceiling_with_paths`); `-p raptorpath-math` all green; `gate_suite` 15/15
release; `mtu_blackhole_wedge` 2/2 (wedge fix NOT regressed — the
`apply_mtu_floor` path is untouched); `perf_loopback` 8/8;
`copa_sole_loopback`, `fmtcp_loopback`, `daps_loopback` green.

## Per-Path Outstanding Accounting (2026-07-18) — the #84 residual lever BUILT: each path's outstanding gets its OWN derived cap (gain·BtlBw_i·echoRTT_i, floor/knee-bounded), per-path draw/release on the retention store, admission = "any account has headroom"; unit + L0 mechanism evidence GREEN — and **L1 MEASURED (2026-07-19): c7 symmetric parity-or-better with the pooled fix (0.87/0.97 of Σ — the ≈1.0 target touched at s7, with the PBS collapse mode absent), but the heterogeneous c8 — the cell this lever was BUILT for — REGRESSES to 0.38–0.39×Σ under BOTH CC families (the cap-full placement redirect over-commits the slow path's account; forensics below). `RWM_STORE_PERCAP` stays DEFAULT OFF; the redirect's delay-aware guard is the named follow-up** (task #86, branch `feat/store-percap`; L1 battery branch `meas/percap-battery`) — **GUARD FOLLOW-UP MEASURED (2026-07-19, roadmap item 1, `fix/percap-redirect-guard`): the floor-clock redirect bound recovers HALF the c8 regression (0.41→0.55 / 0.40→0.52×Σ, both CC families) but PBP-G stays below the pooled PBS bar; flip still NO — see "GUARD RESULTS" below** — **HONEST-CAP FOLLOW-ON MEASURED (2026-07-19, `feat/percap-honest-cap`): caps re-derived on the honest anchor (residence K·RTprop + recovery-clock runway); the sc2 −20% RESOLVED exactly, c7 percap ≥ PBS both seeds (0.89–0.90×Σ), c8 PBP-H > the knee-clamped control but STILL < PBS, and C1P-H < C1 with honest cwnd caps — the NO-BORROWING TAX is confirmed as the c8 binder; flip NO, see "HONEST-CAP RESULTS"** — **BOUNDED BORROWING DERIVED+MEASURED (2026-07-19, `feat/store-borrowing`, paper §16.22): the lender-solvent loan law behaves exactly as derived (c7 loans ≡ 0 by theorem AND by gauge; c8 loans one-directional slow→fast, bounded, repaid) but CANNOT repay the c8 tax — PBP-B < PBS both seeds; the pooled `RWM_STORE_PATHS` design is VINDICATED as the c8 answer, percap(+borrow) is the symmetric-cell tool; residual (iii) attributed to spurious cross-path-retransmit delivery attribution and PARTLY fixed (the flight witness, `RWM_RS_ATTR`); flip NO, see "BORROWING RESULTS"**

*Decision record: → [ADR-0058](adr/0058-path-scaled-outstanding-pool.md)*

**Why (proven by #84).** The multipath binder was flow control: the
per-transfer outstanding pool (1024) WAS the historic ~100–128 Mbit wall by
Little's law. The path-scaled pool (`RWM_STORE_PATHS`) landed C7 at
0.86–0.94 of Σ but C8 stuck at 0.79–0.80: ONE shared pool cannot fit a
c2-deep (fast) and a c3-shallow (slow) path simultaneously — static 8192
collapsed the slow path to 31.8 Mbit/s while the fast path wanted the
depth. Knee ≈ 2048 outstanding per live path. Both #84 residual bullets
named the same lever: per-path outstanding accounting, the FMTCP percap
structure (`fmtcp_percap_full`, the #64 fix) generalized to the
plain-reliable store. Paper §16.19 addendum.

### The derivation as built (env `RWM_STORE_PERCAP`, default OFF = shipped byte-identical)

Derived, not tuned — per-path Little's law on the retention store itself:

    cap_i = clamp(gain × BtlBw_i × echoSRTT_i, floor, pool)

- **rate_i** = `btlbw_sym_per_s()` — that path's OWN delivered-rate
  anchor. Honesty note: in plain (non-generation) mode this is the LEGACY
  ack-interval anchor (the #79 send-interval sampler is generation-only),
  which over-reads on fast paths (§16.13) — there the cap clamps at the
  knee and the account degrades gracefully to a per-path-knee bound.
- **echoRTT_i** = that path's smoothed app-echo SRTT, NOT RTprop: the
  account's residence clock is the ACK (store dwell = delivery + queue +
  ack path), so Little's law needs the echo RTT. The echo-RTT positive
  feedback (deeper store → longer echo → bigger cap) is bounded by…
- **pool** = 2048 (`RWM_STORE_PATH_POOL`), the #84 MEASURED per-path knee,
  as the per-account ceiling; **floor** = 64 (the existing dyn-cap floor).
- **gain** = `RWM_STORE_GAIN` (2.0): ~1 pipe full + ~1 recovery round of
  runway, per path — the same gain law as the pooled cap, now per account.
- **Copa-sole feed**: pipe_i = Copa cwnd_i (Copa's operating point IS the
  per-path pipe — mirrors the pooled Σcwnd law per-path).
- **Warm-up** (anchor not established): the account inherits an equal
  share of the LEGACY pooled cap (legacy/N, clamped to [floor, pool]) and
  converges to the derived cap as the anchor warms. With STORE_PATHS also
  set, the warm-up share inherits the path-scaled pool (the two compose:
  percap supersedes the pooled GATE; STORE_PATHS shapes only the warm-up
  baseline).
- **N = 1 is bit-exact legacy**: the percap law engages only for ≥ 2 live
  paths (`percap_caps` stays empty at N = 1 — the tx_paused expression is
  the legacy branch verbatim), so singles are unchanged even with the flag
  ON. Same identity-control obligation as #84's STEP 3.

**Accounting.** A symbol placed on path i charges account i at the
`sent_store` insert (`percap_charge`, seq→path in lockstep with retention);
released ONLY by the ack that removes it from the store —
`percap_release_seq` on SACK/OOO ranges (the account frees on THAT path's
delivery evidence, not the in-order frontier) and
`percap_release_cumulative` on the frontier advance (split_off twin;
SACK-then-cumulative cannot double-release — seq ownership moves out of the
account map on first release). A cross-path retransmit does NOT
re-attribute (the account bounds the pipe the symbol was ADMITTED against;
dwell ends at the same ack either way). **Admission** pauses TUN intake
only when NO live account has headroom (`percap_store_full` — the exact
`fmtcp_percap_full` mirror), with the pooled `store_len ≥ Σcap_i` test
retained as a stranded-account memory backstop. **Placement**: a cap-full
softmax pick is redirected to the live path with the most relative account
headroom (`percap_place_path`) — so the shallow account is never
over-committed past its own pipe while the deep account keeps deepening
(DAPS placement, when on, keeps its own delay-aware law; accounts still
charge and gate). Mechanism-liveness `info!` echo (“per-path outstanding
accounting ACTIVE”) per MEASUREMENT DISCIPLINE; harness forwards
`RWM_STORE_PERCAP` (tools/l1/perf_rwm_c.sh).

### Unit evidence (`cargo test -p raptorpath --lib` 322/322, 6 new)

- `percap_store_cap_is_rate_x_echo_rtt_with_floor_and_ceiling` — the
  derivation: c2-like (10 400 sym/s × 80 ms × 2 = 1664), c3-like (2000 ×
  60 ms × 2 = 240), knee ceiling 2048, floor 64.
- `percap_store_cap_warmup_inherits_equal_legacy_share` — legacy/N bounded,
  converges to derived on warm.
- `percap_store_full_pauses_only_when_no_account_has_headroom` — the slow
  path's full account never starves the fast path's admission.
- `percap_place_redirects_capfull_pick_to_headroom_path` — redirect to max
  relative headroom; all-full keeps the pick (gate closes next iteration).
- `percap_deep_and_shallow_accounts_coexist_without_coupling` — **the C8
  conflict in miniature**: fast pipe deepens ×2 (800→1600) while the slow
  cap does NOT move (240 — per-path independence, the exact property the
  shared pool lacks); striped placement fills slow to EXACTLY its own 240
  and overflows to the deep account (1600); OOO fast-path acks drain ONLY
  the fast account; re-release idempotent; cumulative release attributes
  per path; gauges stay Σ-consistent with the account map.
- `percap_warmup_share_degenerates_to_legacy_at_n1` — a 2→1 live-path flap
  has no behavior cliff (share → the full legacy cap).

### L0 mechanism evidence (loopback + RWM_L0_NETEM shim — LOCAL, not L1)

Dual-path L0 cells via the existing shim (`RWM_L0_DUAL=1`,
`RWM_L0_NETEM=c2,c3` / `c2,c2`; no infra extension needed), release
binary, plain mode, 12.5 MB × 1 run/invocation × 3 reps, arms interleaved
per rep (base / SP=`RWM_STORE_PATHS=1` / PC=`RWM_STORE_PERCAP=1`), logs in
the session scratchpad (`l0out/*.log`). Windows dev box, default quinn CC
(the shim drops BEFORE quinn — see caveats).

| cell | base (runs) | SP (runs) | PC (runs) |
|---|---|---|---|
| c7-like c2,c2 | 70.8 [62.2 79.7 70.5] | 23.1 [36.2 16.8 16.4] | **33.1** [34.5 36.9 28.1] |
| c8-like c2,c3 | 65.3 [64.6 67.6 63.7] | 6.3 [10.9 5.0 3.1] | **18.5** [24.4 12.1 18.8] |
| sc2 single (identity) | 66.4 | — | 67.6 (flag inert at N=1 ✓) |

Reading these for what they are (Mbit/s, LOCAL):

1. **The mechanism gauges behave as derived** (RWM_DIAG `sout=out_i/cap_i`
   per path): accounts charge on placement and release on SACK/cumulative
   acks per path (drain phases show differentiated occupancy, e.g.
   `p0 sout=1097/2048, p1 sout=113/2048`); warm-up accounts show the
   legacy-share caps (`sout=0/64` on the cold reverse channel, boot 128/2);
   when both accounts fill, `paused=100%` with `sout=2048/2048` on BOTH
   paths — per-path admission binding exactly at the caps, then reopening
   on drain. Liveness echo + gauges confirm the mechanism EXECUTES.
2. **PC vs SP — the per-path bound does its job**: in the heterogeneous
   c8-like cell percap is ~2.9× the pooled path-scaled arm (18.5 vs 6.3;
   same ranking all 3 interleaved reps) and ~1.4× at c7-like (33.1 vs
   23.1). Locally, the pooled N×2048 lets any one path's symbols bloat the
   whole budget (the L1 static-8192 collapse mode, reproduced in
   miniature); the per-path accounts bound each path individually.
3. **base > both deep-pool arms locally — expected, and it is why L0
   CANNOT deliver the C7/C8 verdict**: on the loopback shim the ack-echo
   RTT is tens of ms and the per-path pipe is ~100 symbols, so the legacy
   1024-latch pool NEVER binds (`paused=0%` on base runs) — deep pools buy
   only queue + slower GE recovery here. The L1 regime (80–100 ms echo,
   pool = the Little's-law wall) is exactly what the shim does not
   reproduce (drops before quinn, no substrate CC dynamics, dev-box
   timers). The L0 deltas are MECHANISM evidence (accounts vs pool), not
   throughput evidence.
4. **Anchor honesty caveat (named for L1)**: in plain mode the per-path
   rate anchor is the LEGACY ack-interval one (the rate-sample machinery
   is generation-mode-only), which over-reads on fast paths (§16.13) — so
   plain-mode caps typically clamp at the 2048 knee and the derivation
   degrades gracefully to per-path-knee ACCOUNTS (still per-path
   draw/release + admission, unlike the pooled cap; the slow path's cap
   differentiates whenever `2·BtlBw_i·echoSRTT_i < 2048`). Under Copa-sole
   the pipe is cwnd_i — honest per path by construction.

### The queued L1 battery (NEXT VM SESSION — copy of the discipline)

Run on the L1 VM (host-passthrough E5-2650v3; record `lscpu` in every log
header; note the #84 HARDWARE DIVIDE — compare only against post-divide
numbers). Binary = this branch's commit, sha256 recorded; same binary in
EVERY arm.

1. **Arms (same binary, interleaved round-robin per rep):**
   - `PB`  = plain + `RWM_QUIC_CC=bbr` (baseline)
   - `PBS` = PB + `RWM_STORE_PATHS=1` (the #84 pooled fix — the bar to beat)
   - `PBP` = PB + `RWM_STORE_PERCAP=1` (the percap arm)
   - `C1`  = plain + `RWM_QUIC_CC=passthrough` (Copa wire-signal defaults)
   - `C1P` = C1 + `RWM_STORE_PERCAP=1` (Copa C8 was cwnd-bound, not
     pool-bound, in #84 — percap should be ≈inert there; that PREDICTION is
     part of the test)
2. **Cells**: c7 (dual-c2 symmetric), c8 (c2+c3 heterogeneous), PLUS the
   N = 1 identity controls sc2 and sc3 with `RWM_STORE_PERCAP=1` vs unset
   (the flag must be inert at N = 1; every Δ ≪ σ_s).
3. **Protocol**: 25 MB × 1 run/invocation × 8 reps per arm×cell×seed,
   seeds 42 AND 7, arms interleaved within one session (cancels the
   documented 2.3× session drift), fresh tunnel per invocation.
4. **Per-run recording (MEASUREMENT DISCIPLINE, all five items):**
   mechanism-liveness echo (`per-path outstanding accounting ACTIVE` on
   every PBP/C1P run; its ABSENCE asserted on every baseline run), full
   command line + env + binary sha256, per-run distributions (not just
   means), both seeds, and RWM_DIAG `sout=out_i/cap_i` gauges on ≥1 probe
   run per cell — the mechanism assertion is the slow path's account
   holding near ITS pipe (sout_slow ≈ cap_slow ≪ cap_fast) while the fast
   account deepens, AND `win=` no longer pegged at a shared ceiling.
5. **Targets/verdict frame**: same-session Σ of per-path singles as the
   ceiling; C7 target ≈ 1.0×Σ (from 0.86–0.94 under PBS — does per-path
   accounting close the pooled self-queue equilibrium?); C8 target ≈
   0.9×Σ (from 0.79–0.80). Claimed deltas must exceed the recorded σ_s /
   drift floor; PB-C8 is the documented bimodal arm — report per-run
   values. Also record CPU (recv/send) per invocation: the #84 finding
   predicts engine CPU begins to matter above ~140 Mbit — if C7 lands
   ≈150+, the next lever (receiver/sender task parallelization) becomes
   live and should be noted, not built.
   Interpretation guard: in the PB arms the legacy anchor over-read means
   PBP's caps will often sit at the per-path knee (2048 each) — so a
   PBP−PBS delta there is attributable to per-path DRAW/RELEASE +
   admission, not cap sizing; the DIAG `sout=` probe runs are what
   separate the two. C1P (cwnd_i pipes) is the honest-derivation arm.
6. **VM hygiene**: `/tmp/rwm-vm.lock`, rp-* netns only, `pkill -x
   raptorpath`, logs + driver script under `/home/vibe/percap/`.

### L1 RESULTS (VM 10.1.5.16, 2026-07-19 03:19–04:00 UTC; binary sha256 3654214ef4ca8eb3… = commit b317983 on `meas/percap-battery` (= 8ef5ff1 + the RWM_TAPER_R harness-forward), SAME binary every arm; 25 MB × 1 run/invocation × 8 reps, arms interleaved round-robin per rep, fresh tunnel per invocation, seeds 42+7, `RWM_DIAG=1` on every arm; driver `/home/vibe/percap_battery.sh`, logs `/home/vibe/percap/{sc2,sc3,c7,c8}-s{42,7}.log` + per-run `diag-*.log`; lscpu in every log header: E5-2650 v3, aes+avx2+pclmulqdq — post-divide hardware, compared only against post-divide numbers)

**Singles — the N=1 identity control (flag must be inert) + the same-session
Σ denominators:**

| cell | PB (σ_s) | PBP (σ_s) | C1 (σ_s) | C1P (σ_s) |
|---|---|---|---|---|
| sc2 s42 | 78.56 (3.48) | 79.38 (1.85) | 68.78 (1.17) | 68.89 (1.01) |
| sc2 s7 | 76.25 (2.40, n=7) | 77.02 (3.28) | 66.36 (2.82) | 67.16 (0.90, n=5) |
| sc3 s42 | 15.77 (0.29) | 15.56 (0.21) | 11.72 (0.55) | 11.93 (0.50) |
| sc3 s7 | 15.70 (0.36, n=6) | 15.42 (0.74, n=7) | 12.29 (0.28, n=6) | 12.26 (0.29, n=7) |

Identity HOLDS everywhere (every Δ ≪ σ_s; the percap law provably computes
nothing at N=1 and measures that way). Same-session ceilings: **Σ-PB: C7 =
157.1/152.5, C8 = 94.3/92.0; Σ-C1: C7 = 137.6/132.7, C8 = 80.5/78.7.**

**Duals (mean (σ_s) [per-run values]; Σ-ratio = arm / same-family Σ):**

| cell | arm | mean (σ_s) [runs] | Σ-ratio |
|---|---|---|---|
| c7 s42 | PB | 104.62 (18.89) [109.4 111.4 110.7 59.8 99.4 115.5 116.6 114.2] | 0.67 |
| | PBS | 138.52 (8.46) [127.8 124.9 138.6 143.1 136.4 145.9 148.7 142.8] | 0.88 |
| | PBP | **136.50 (7.75)** [133.7 139.8 133.8 146.3 132.8 132.2 125.1 148.2] | **0.87** |
| | C1 | 80.99 (5.92) [80.2 79.0 77.6 76.6 84.5 72.3 89.4 88.3] | 0.59 |
| | C1P | **92.27 (6.05)** [100.1 100.5 89.4 84.0 91.1 96.0 86.6 90.5] | **0.67** |
| c7 s7 | PB | 104.99 (7.50, n=7) [117.8 97.8 103.7 104.1 100.0 112.6 99.0] | 0.69 |
| | PBS | 128.87 (29.93, n=4) [150.3 140.6 140.0 84.5] | 0.85 |
| | PBP | **147.38 (4.79, n=6)** [148.1 149.2 147.2 153.1 148.1 138.6] | **0.97** |
| | C1 | 81.72 (4.70) [83.9 87.0 84.2 80.6 78.5 82.1 85.3 72.2] | 0.62 |
| | C1P | **103.13 (4.36, n=7)** [100.7 102.3 104.3 107.3 105.7 94.8 106.8] | **0.78** |
| c8 s42 | PB | 56.56 (10.53) [62.9 46.1 74.9 44.7 48.1 57.5 53.6 64.7] | 0.60 |
| | PBS | 62.84 (13.17) [69.1 51.2 74.5 66.2 41.7 53.4 81.6 65.1] | 0.67 |
| | PBP | **37.00 (2.10)** [34.9 33.9 38.8 37.0 38.3 39.0 35.2 39.0] | **0.39 REGRESSION** |
| | C1 | 55.76 (3.50) [49.3 55.7 59.2 55.6 55.4 52.8 60.2 57.8] | 0.69 |
| | C1P | **34.84 (5.84)** [28.8 28.0 44.6 36.6 40.9 36.3 31.1 32.4] | **0.43 REGRESSION** |
| c8 s7 | PB | 44.43 (10.16, n=6) [47.4 39.2 48.9 54.9 49.7 26.5] | 0.48 |
| | PBS | 67.59 (8.57) [61.2 66.3 72.3 79.1 66.0 61.4 79.1 55.6] | 0.74 |
| | PBP | **35.08 (1.35, n=5)** [36.4 34.6 35.9 33.0 35.6] | **0.38 REGRESSION** |
| | C1 | 55.87 (3.86) [49.2 52.5 59.8 57.6 55.9 54.6 56.1 61.2] | 0.71 |
| | C1P | **35.16 (3.96)** [36.6 33.7 30.3 39.2 35.4 29.4 36.0 40.8] | **0.45 REGRESSION** |

**Reading, effect-by-effect (vs the recorded noise floor):**

1. **c7 symmetric — percap does the pooled fix's job, or better.**
   PBP−PB = +31.9/+42.4 (1.7–5.7× the baseline σ_s, both seeds same
   sign). PBP vs PBS: statistical tie at s42 (−2.0, ≪ σ), +18.5 at s7 —
   where PBS's n=4 includes an 84.5 collapse run (the PBS bimodal mode)
   while PBP's per-run spread is tight BOTH seeds (σ 4.8–7.8, no run
   below 125): the per-path accounts remove the pooled arm's collapse
   mode at c7. **PBP c7-s7 = 147.4 = 0.97×Σ — the ≈1.0×Σ target touched**
   (0.87 at s42). Copa: C1P−C1 = +11.3/+21.4 (≈2–4.5× σ_s) — percap
   extends the #84 pool unlock to Copa's c7 as well.
2. **c8 heterogeneous — the cell this lever was BUILT for — is a decisive
   REGRESSION under both CC families**: PBP −25.8/−32.5 vs PBS
   (2.0–3.8× the worst arm σ_s, both seeds), C1P −20.9/−20.7 vs C1
   (3.6–5.2× σ_s, both seeds). PBP/C1P σ COLLAPSES (1.4–5.8) — the arm
   is stably parked at ~35 Mbit/s: a mechanism, not noise.
   The C8 0.9×Σ target is not approached; the #84 PBS bar (0.67/0.74 Σ
   this session) stands.
3. **The ledger's C1P prediction ("percap ≈ inert under Copa — its C8 is
   cwnd-bound") is REFUTED in both directions**: +21 at c7, −21 at c8.
   The percap admission/placement structure binds BELOW Copa's cwnd law.

**Forensics — the c8 collapse mechanism, from the sout= DIAG probes
(per-run `diag-c8-*` logs):** in PBP-c8 BOTH accounts sit pegged at their
caps (median out/cap = 1.00–1.05; caps knee-clamped: fast always 2048,
slow median 1531 — the documented plain-anchor over-read means the derived
differentiation mostly never engages in PB arms, exactly the
interpretation guard's warning), `paused=36%` mean (PB 10%, PBS 22%), and
the app-echo RTT inflates to 214–235 ms mid-run. C1P-c8 shows the SAME
shape with honestly-derived caps (964/746 from cwnd_i): both accounts
pegged, `paused=30%`, slow-path echo RTT spikes to 811 ms, goodput ~10
mid-run. The named binder: **the cap-full placement redirect
(`percap_place_path`) sends fast-path overflow to the account with
RELATIVE headroom — at c8 that is the slow path, which it fills to its
full cap: ~2048 symbols parked on a 15.7 Mbit path is ≈1.3 s of store
dwell**, so every slow-path hole recovers ~13× slower than the fast
path's, the in-order frontier serializes behind it, the echo-RTT feedback
(dwell → echo → cap) holds the account open, and the all-full admission
gate then pauses intake 30–36 % of the time. The smoke probe recorded the
over-commit directly (`sout=1873/750` — out 2.5× the shrunk cap). The L0
c8-like PC>SP ranking did NOT transfer: the L0 shim (drops before quinn,
~10 ms echo) never let the slow account accumulate L1's 1.3 s dwell — L0
was mechanism evidence only, as its own caveat said.

**CPU (mean per 25 MB invocation, recv·send):** c7 PBP 1.93·1.59 (s42) /
1.86·1.58 (s7) at 136–147 Mbit vs PB 2.07·1.59 at 105 — CPU per bit still
FALLING at the higher rate. Per the #84 prediction: PBP c7-s7 lands at
~147 ≈ the ~150 threshold — **receiver/sender task parallelization is now
LIVE as the next c7 lever** (noted, NOT built). c8 PBP's higher
per-invocation CPU (2.74–3.09 recv) is run-length, not rate (25 MB at
~36 Mbit ≈ 5.5 s wall; utilization ~0.5 core).

**VERDICT + FLIP DECISION.** PERCAP ≥ STORE_PATHS fails decisively at c8
— the evidence is clean on both seeds and it is a regression in the
lever's own target cell. **`RWM_STORE_PERCAP` stays DEFAULT OFF** (shipped
tree byte-identical; no code change). What the battery DID establish:
(i) the per-path account structure at SYMMETRIC cells is at least the
pooled fix without its collapse mode (and touches Σ at s7); (ii) the c8
binder is now named at the gauge level — not cap sizing but the
**cap-full redirect + all-full admission composition**, which converts
"the slow account is never starved" into "the slow path is always
bloated". **Named follow-up (NOT built, per measurement discipline):
`percap-redirect-guard` — bound the redirect by the target account's
absolute dwell (redirect only while cap_i·dwell_i ≤ ~1 recovery round,
i.e. a delay-aware redirect in the DAPS §16.9 sense), and/or let the
slow account's cap bind by dwell (cap_i ≤ rate_i × recovery-bound) so a
c3-class account caps at its recovery budget rather than the 2048 knee.**
Re-battery c8 before any flip talk.

**Controls / discipline:** mechanism-liveness audit over all 265
result-bearing runs: every PBP/C1P run has the `per-path outstanding
accounting ACTIVE` echo, every PBS run the `path-scaled` echo, every
PB/C1 run has NEITHER (0 mismatches). 23 invocations (all seed-7)
produced no result — the known seed-7 topo-ping double-abort (RETRY
recorded per log); no captured result was discarded; c7-s7 PBS kept only
n=4 (its mean is quoted with that caveat; its 3 non-collapse runs match
#84's 142.1). Zero DNFs in captured runs. Driver quirk recorded: the
LIVENESS log line doubles a "0" count on echo-absent runs (`grep -c ||
echo 0`); the audit parsed blocks, not that line. Session drift vs #84
(PB-C8 64.9/55.9 → 56.6/44.4; PBS-C8 0.80/0.79Σ → 0.67/0.74Σ) is why all
verdicts above are same-session interleaved comparisons only.

### GUARD RESULTS (2026-07-19, roadmap item 1, branch `fix/percap-redirect-guard`, commit 689b9f1) — the delay-aware redirect guard BUILT and L1-MEASURED: it recovers HALF the c8 regression under both CC families (the redirect channel is closed — the slow account pins at its floor-clock bound and the parked dwell collapses ~4×) but PBP-G stays BELOW the pooled PBS bar at c8 on both seeds, because a SECOND parking channel is now exposed: the placement softmax's OWN picks fill the slow account to its knee-clamped cap. `RWM_STORE_PERCAP` stays DEFAULT OFF.

**The guard law, derived (not tuned).** Projected dwell of account j is
Little's law on the store: D_j = out_j / rate_j (the store drains at the
ack clock). The naive bound D_j ≤ κ·echoRTT_j with κ = 1 ("j drains its
account within one echo round") is VACUOUS on the loaded echo clock,
because the app echo is store-dwell-inclusive: echoRTT_j ≈ RTprop_j + D_j,
so D ≤ RTprop + D holds for EVERY D — this is precisely the measured #86
feedback (slow-path echo inflating 214–811 ms and holding the account
open). Solving D ≤ κ·(RTprop_j + D) for κ < 1 gives
D ≤ (κ/(1−κ))·RTprop_j; κ = 1/2 — the redirected symbol must clear within
one round even AFTER its own dwell has inflated the echo — gives
D ≤ RTprop_j, i.e. κ = 1 on the FLOOR clock:

    bound_j = rate_j × RTprop_j   (the path's honest BDP in symbols)

A cap-full redirect may park at most one UN-QUEUED pipe on the target.
Equivalently: cap_j = gain·pipe_j decomposes (gain 2.0) into "1 pipe + 1
recovery-round runway" — redirects may consume only the floor-clocked pipe
term; the runway term and any knee-clamp headroom (the plain-anchor
over-read case) are reserved for the path's OWN traffic. Copa-sole feed:
bound_j = cwnd_j (Copa's operating point is the bounded-queue pipe);
warm-up: cap_j/gain (the share's pipe term); clamp [1, cap_j].
**Composition** (`percap_place_path` + `percap_store_full_guarded`):
redirect targets must satisfy out_j < min(cap_j, bound_j); when SOME
account is cap-full and NO guard-eligible target exists the store reads
FULL for the placement and the existing battery-proven admission pause
engages — backpressure, don't park (NOT a new deferral mechanism; the #73
lesson does not recur). Own-pick placement below cap is never guard-gated;
N = 1 stays bit-exact (percap_caps empty); `RWM_PERCAP_GUARD=0` restores
the unguarded redirect as the same-binary regression-control arm. DIAG:
`sout=out/cap/b<bound>`; guard liveness echo per MEASUREMENT DISCIPLINE.

**Unit evidence** (lib 335/335, 3 new):
`percap_redirect_bound_is_floor_clock_bdp` (the derivation: c3-like
1534 sym/s × 60 ms RTprop → 93 vs the knee-adjacent 1531 cap; warm-up
cap/gain; clamps), `percap_store_full_guarded_backpressures_instead_of_parking`
(cap-full + target-past-bound ⇒ FULL where the unguarded gate admits —
THE c8 fix — and bound=cap degenerates exactly to the unguarded gate),
and `percap_redirect_guard_stops_at_dwell_bound_and_pauses_admission`
(the c8 miniature guarded: fast pegs at cap 1600, redirects fill slow to
EXACTLY bound 60 ≪ cap 240, gate reads FULL where the old gate admits,
one-placement slop on race, own picks never gated, gate reopens on
drain). Old unguarded tests retained (bound=cap).

**L1 battery** (VM 10.1.5.16, 2026-07-19 09:00–10:11 UTC; binary sha256
fef7ae5ff8bebb6b… = commit 689b9f1 `fix/percap-redirect-guard`, SAME
binary every arm; E5-2650 v3 aes+avx2+pclmulqdq in every log header
(post-divide); 25 MB × 1 run/invocation × 8 reps, arms interleaved
round-robin per rep, fresh tunnel per invocation, seeds 42+7, RWM_DIAG=1
everywhere; driver `/home/vibe/guard_battery.sh`, logs
`/home/vibe/guard/{sc2,sc3,c7,c8}-s{42,7}.log` + per-run `diag-*.log`).
Arms: PBS (pooled path-scaled, the c8 incumbent), PBPO (percap
+ `RWM_PERCAP_GUARD=0`, unguarded regression control), PBPG (guarded
percap), C1 (Copa wire-signal), C1PG (guarded percap under Copa); singles
PB/PBPG/C1/C1PG.

Singles — N=1 identity (flag+guard must be inert) + same-session Σ:

| cell | PB (σ_s) | PBPG (σ_s) | C1 (σ_s) | C1PG (σ_s) |
|---|---|---|---|---|
| sc2 s42 | 74.36 (4.85) | 77.29 (3.32) | 68.96 (1.43) | 68.26 (2.24) |
| sc2 s7 | 77.95 (2.89, n=7) | 77.63 (2.87) | 65.70 (2.93, n=7) | 63.06 (7.81, n=6) |
| sc3 s42 | 15.36 (0.85) | 15.86 (0.19) | 11.92 (0.39) | 11.83 (0.52) |
| sc3 s7 | 15.79 (0.24, n=6) | 15.79 (0.25, n=5) | 12.34 (0.32) | 12.35 (0.21, n=6) |

Identity HOLDS (every Δ within ~1σ_s; sc2-s7 C1PG carries one 47.4
outlier run in n=6, mean Δ −2.6 ≈ σ). Same-session ceilings: **Σ-PB
c7 = 148.7/155.9, c8 = 89.7/93.7; Σ-C1 c7 = 137.9/131.4, c8 = 80.9/78.0.**

Duals (mean (σ_s) [per-run values]; Σ-ratio = arm / same-family Σ):

| cell | arm | mean (σ_s) [runs] | Σ-ratio |
|---|---|---|---|
| c7 s42 | PBS | 133.34 (2.89) [131.6 131.6 133.1 132.4 135.8 130.9 139.4 131.8] | 0.90 |
| | PBPO | 139.12 (5.80) [140.5 134.0 149.1 138.3 130.9 142.1 135.2 142.8] | 0.94 |
| | PBPG | **132.52 (12.47)** [141.6 130.2 134.1 103.5 139.8 142.2 135.8 133.1] | **0.89** |
| | C1 | 80.30 (5.83) [77.1 82.0 76.9 73.5 86.8 78.4 76.9 90.8] | 0.58 |
| | C1PG | **90.17 (6.60)** [85.8 97.1 86.6 89.9 86.9 86.6 103.4 85.1] | **0.65** |
| c7 s7 | PBS | 132.58 (22.17, n=7) [83.5 138.3 144.1 135.9 136.3 140.7 149.2] | 0.85 |
| | PBPO | 138.48 (15.28, n=7) [153.1 122.1 142.1 145.2 112.0 147.4 147.4] | 0.89 |
| | PBPG | **135.90 (6.07, n=6)** [127.6 139.4 131.5 143.8 139.8 133.3] | **0.87** |
| | C1 | 74.72 (8.51, n=7) [64.3 73.7 80.5 82.4 75.6 62.6 84.0] | 0.57 |
| | C1PG | **95.72 (3.44, n=7)** [102.3 96.2 94.1 96.0 94.3 96.2 90.9] | **0.73** |
| c8 s42 | PBS | 64.74 (21.58) [82.8 83.5 74.4 22.2 55.4 78.5 74.1 47.0] | 0.72 |
| | PBPO | 37.15 (1.34) [36.6 38.0 36.3 39.4 37.9 37.0 37.1 34.9] | 0.41 |
| | PBPG | **49.52 (8.82)** [62.0 41.1 50.0 40.8 47.5 45.3 63.8 45.6] | **0.55** |
| | C1 | 53.01 (4.57) [46.8 56.5 50.8 53.8 46.4 58.8 55.5 55.5] | 0.66 |
| | C1PG | **43.37 (8.37)** [43.4 47.3 51.0 49.5 44.1 46.5 24.3 40.9] | **0.54** |
| c8 s7 | PBS | 62.92 (9.91) [70.9 53.4 77.2 68.8 46.3 60.1 65.3 61.3] | 0.67 |
| | PBPO | 37.29 (1.89, n=7) [37.4 35.1 36.8 40.6 38.5 37.3 35.2] | 0.40 |
| | PBPG | **48.81 (8.78, n=6)** [53.7 36.0 60.1 42.8 46.3 54.0] | **0.52** |
| | C1 | 55.40 (2.41, n=6) [53.3 52.3 58.3 56.7 57.4 54.3] | 0.71 |
| | C1PG | **44.18 (1.92, n=6)** [43.1 43.6 47.0 44.5 41.5 45.3] | **0.57** |

**Reading, effect-by-effect:**

1. **The guard MECHANISM works and recovers half the c8 regression, both
   CC families, both seeds.** PBPG−PBPO = +12.4/+11.5 (≥6× PBPO's σ_s,
   same sign both seeds); C1PG improves from #86's 0.43/0.45×Σ to
   0.54/0.57 (family-ratio). The unguarded control PBPO reproduces the
   #86 regression exactly (37.15/37.29 vs 37.00/35.08) — a clean
   same-binary A/B on the guard alone.
2. **But PBP-G < PBS at c8 on BOTH seeds** (−15.2/−14.1, ≈1.5σ of the
   worst arm σ_s at s42, 1.4σ at s7 — and PBS's mean carries its
   documented bimodal collapse runs 22.2/47.0/46.3; by medians the gap
   is wider). C1PG stays below C1 (−9.6/−11.2, ≥2σ). The C8 0.9×Σ target
   is not approached (0.52–0.57); the pooled PBS bar (0.67–0.72 this
   session) STANDS.
3. **c7 is preserved under the guard**: PBPG 0.89/0.87×Σ (≥ the 0.87
   target both seeds), statistical tie with PBS (−0.8/+3.3, ≪/≈σ); the
   Copa c7 percap win survives (C1PG−C1 = +9.9/+21.0, ≈1.7–2.5σ).
   Honest note: at c7 the UNGUARDED percap is the best arm this session
   (139.1/138.5 = 0.94/0.89) — the guard costs −6.6/−2.6 there (≈1σ),
   the price of pausing where the unguarded arm borrows.

**Gauge forensics — before/after (per-run `diag-c8-*` logs).** Unguarded
PBPO: both accounts pegged at 2048/2048 (bound=cap), slow-path echo RTT
**1004–1005 ms** (the parked ≈1.05 s dwell at the honest ~1954 sym/s
rate, reproducing #86's ≈1.3 s), tick-mean paused 37–62 %. Guarded PBPG,
good runs (62.0/63.8 Mbit): the slow account pins EXACTLY at its bound
(`sout=508/2048/b508`), slow echo RTT 121–301 ms — **the parked dwell
collapses ≈4× (1.05 s → ~0.26 s)** — paused 11–18 %. Guarded bad runs
(41–47 Mbit): the slow account still bloats to 1638/2048 with b674 —
ABOVE the bound — which the guard cannot cause: those symbols are the
placement softmax's OWN picks, admitted while the gate is open and placed
directly on the slow path below its cap, which the plain-anchor over-read
holds knee-clamped (btlbw reads 12.9–15.2k sym/s ≈ 8–10× the honest c3
rate, so cap_slow stays ≈2048 and never differentiates — exactly the #86
interpretation-guard warning). Under Copa (C1PG) the caps are honest
(cwnd-derived: cap 466–1038, bound = cwnd) and the slow account pins at
its bound in EVERY probe (`sout=323/b323`, `233/b233`, `433/b433`),
paused only 9–23 % — yet the arm still trails C1: with per-path accounts
the fast path is denied the POOLED law's ability to borrow the slow
path's unused share (out_fast ≤ gain·cwnd_fast vs pooled
gain·Σcwnd), a structural cost of account isolation at asymmetric cells,
not a defect of the guard.

**VERDICT + FLIP DECISION.** The redirect channel is CLOSED — measured at
the gauge level (bound-pinned slow account, dwell ~1.05 s → 0.26 s) and
at the throughput level (+11.5/+12.4 over the unguarded control, both CC
families) — but PBP-G ≥ PBS FAILS at c8 on both seeds under both CC
families, so **`RWM_STORE_PERCAP` stays DEFAULT OFF** (guard merged as
code, default-inert; shipped tree byte-identical with the env unset).
What the battery established: the #86 "cap-full redirect" attribution was
HALF the story — with redirects bounded, the residual c8 parking flows
through (i) the softmax's own picks under the plain-anchor over-read's
knee-clamped slow cap (the cap needs the SAME floor-clock dwell bound the
redirect got, i.e. cap_i ≤ gain·rate_i·RTprop_i, and/or the #79
send-interval sampler generalized to plain mode so the anchor stops
over-reading), and (ii) under honest Copa caps, the account structure's
no-borrowing property at asymmetric cells. Both are named follow-ups,
NOT built. c7 keeps percap parity under the guard; the c8 record remains
pooled PBS.

**Controls / discipline:** mechanism-liveness audit over ALL 264
captured runs (288 invocations): every PBPG/C1PG run has BOTH the percap
echo and the guard echo, every PBPO run the percap echo and NO guard
echo, every PBS run the path-scaled echo only, every PB/C1 run none — 0
mismatches. 24 invocations aborted with no result (the documented seed-7
topo-ping double-abort class, all but one on seed 7; stale-log liveness
lines on aborted invocations recorded and discounted; RETRY per
protocol; no captured result discarded; n quoted wherever < 8). Zero
DNFs in captured runs. All verdicts same-session interleaved (session
drift vs #86 visible again: PBS-c8 0.67/0.74Σ → 0.72/0.67Σ).

### HONEST-CAP RESULTS (2026-07-19, the unblocked percap follow-on, branch `feat/percap-honest-cap`, commit 5d30c02) — the caps re-derived on the HONEST plain anchor: the sc2 −20% is RESOLVED exactly (the K/R headroom law; same-binary control reproduces −18/−22%), c7 percap ≥ PBS both seeds at 0.89–0.90×Σ, and c8 improves over the knee-clamped control (+3.4/+3.8, parking tail halved) — but PBP-H < PBS at c8 on BOTH seeds and C1P-H < C1 again with caps honest by construction: **the no-borrowing tax is CONFIRMED as the c8 binder**. `RWM_STORE_PERCAP` stays DEFAULT OFF; the roadmap redirects from cap hygiene to account borrowing.

**The re-derivation (env `RWM_HONEST_CAP`, default ON but consulted only
under `RWM_PLAIN_RS` — shipped tree byte-identical; `RWM_HONEST_CAP=0` =
the floor-law control arm).** The first cut — the roadmap's literal
"cap_i = gain·rate_i·RTprop_i floor clock" — was REFUTED by its own L1
smoke before the battery: c2's true RTprop is 8 ms (the DIAG `rtp=` gauge;
the percap unit tests' "80 ms echo" was the LOADED clock), so the
legacy-good 1024 store is ~12× the floor BDP and the floor law computes
cap ≈ 150–170 → 56 Mbit (the −20% arm reproduced, khr pinned 1.00 by a
second defect, below). The headroom the over-read supplied was never
ack-batching — it is the RECOVERY clock: a plain-window hole is recovered
by the SACK re-advertisement / tail-sweep engine whose round is clamped to
[25, 100] ms (`HOLE_NACK_REFRESH_*`/`TAIL_SWEEP_*`), and GE burst loss
routinely drives it to the ceiling (measured: sweeps every ~140 ms live,
558 retx per 3.5-s sc2 transfer). Final law (`honest_store_cap`,
net/mod.rs), every term measured or a named engine constant:

    cap_i = rate_i·(K_i·RTprop_i + (gain−1)·(R + RTprop_i))
          = anchor_i·(K_i + gain − 1) + rate_i·(gain−1)·R

- **residence term** rate·K·RTprop: Little's law on the UNLOADED drain
  clock; K_i = windowed-min echoSRTT_i/RTprop_i (`EchoRatioMin`, two 5-s
  half-buckets ≈ the min-RTT window class) — the min is self-queue-PROOF
  (own dwell only raises the ratio), so the c8 dwell→echo→cap spiral has
  NO handle on any term (rate windowed-max honest, RTprop windowed-min,
  K windowed-min, R a constant). Defect found and fixed at the smoke: at
  the estimator seed instant srtt ≡ min_rtt (shared seed), ratio ≡ 1.0,
  and feeding it LATCHED the windowed min at 1.00 for a whole window —
  seed-identity samples (srtt−RTprop ≤ 5 µs) are now DISCARDED, not
  clamped (`observe_srtt_over_rtprop`).
- **runway term** (gain−1)·rate·(R + RTprop): one worst-case recovery
  round on the RECOVERY engine's clock (R = 100 ms, the cadence clamp
  ceiling — `HONEST_RECOVERY_ROUND_S`) plus the retransmit flight.
- The legacy floor law gain·anchor is the K=1, R=0 degenerate; K ≥ 1 and
  R > 0 make the honest cap STRICTLY wider — honest anchors can never
  shrink a cap below the old law (the sc2 no-regression property, now a
  unit law). Cross-checks against independently measured good points:
  sc2 → 10.4k·(K·8ms + 108ms) ≈ 1290 → latches the proven 1024 store;
  c8-slow → ~2k·(K·60ms + 160ms) ≈ 470–500 ≈ the guard session's measured
  good pin (508, 0.26 s dwell); the knee-parking regime (2048, ≈1 s) is
  unreachable for a c3-class honest rate.
- **Scope**: percap cap_i (N ≥ 2) AND the N=1/anchor-sum pooled cap (the
  −20% seat); clamps unchanged ([floor 64, 2048 knee] per account,
  [floor, 1024 store] at N=1); warm-up unchanged (legacy share; honest law
  returns None until anchor+RTprop warm); Copa-sole feed untouched
  (cwnd_i is already the honest pipe); redirect guard unchanged
  (bound_j = the floor pipe — redirects still may not consume the runway).
  DIAG gains `khr=` (the live K_i); liveness echo "honest floor-clock
  store caps ACTIVE"; harness forwards
  RWM_PLAIN_RS/RWM_ANCHOR_HYGIENE/RWM_HONEST_CAP.

**Unit evidence (lib 355/355, 5 new):**
`echo_ratio_min_is_self_queue_proof_and_window_expires` (inflation cannot
raise the min; the window expires per anchor-hygiene rule 3),
`echo_ratio_seed_identity_sample_is_discarded_not_latched` (the measured
khr=1.00 defect as a law), `honest_store_cap_is_residence_plus_recovery_runway`
(the derivation, K/gain clamps, warm-up None),
`honest_caps_shallow_account_sits_at_recovery_budget_not_knee` (the c8
miniature on honest anchors: fast 1248 differentiated < knee, slow 466 ≈
the measured good pin, per-path independence), and
`honest_anchor_sum_cap_preserves_sc2_throughput_headroom` (floor law 167
= the −20% arm; honest K=2 → 1024 latch; monotone > floor law ∀K ≥ 1).

**L1 battery (VM 10.1.5.16, 2026-07-19 16:12–16:58 UTC; binary sha256
67091cb91b73b216… = commit 5d30c02, SAME binary every arm; E5-2650 v3
aes+avx2+pclmulqdq in every log header (post-divide); 25 MB × 1
run/invocation × 8 reps, arms interleaved round-robin per rep, fresh
tunnel per invocation, seeds 42+7, RWM_DIAG=1 everywhere; driver
`/home/vibe/honest_battery.sh` (+`honest_all.sh`), logs
`/home/vibe/honest/{sc2,sc3,c7,c8}-s{42,7}.log` + per-run `diag-*.log`).**
Arms — duals: PBS (pooled path-scaled, the c8 incumbent), PBP-G-old
(percap+guard, knee-clamped caps — the guard-session control), PBP-H
(percap+guard+`RWM_PLAIN_RS` honest caps), C1 (Copa wire-signal), C1P-H
(percap under Copa); singles: PB, PBH0 (`RWM_PLAIN_RS` +
`RWM_HONEST_CAP=0`, the floor-law −20% control), PBP-H, C1, C1P-H.

Singles — the −20% resolution + N=1 identity + same-session Σ:

| cell | PB (σ_s) | PBH0 floor-law (σ_s) | PBP-H (σ_s) | C1 (σ_s) | C1P-H (σ_s) |
|---|---|---|---|---|---|
| sc2 s42 | 76.61 (1.94) | **62.88 (5.82)** | **76.85 (2.90)** | 67.21 (4.70) | 69.14 (1.12) |
| sc2 s7 | 77.04 (5.08, n=4) | **60.20 (6.01, n=7)** | **77.01 (3.45, n=6)** | 65.50 (1.22, n=5) | 67.74 (0.77, n=4) |
| sc3 s42 | 15.47 (0.23) | 13.43 (0.19) | 14.81 (0.24) | 12.08 (0.37) | 11.78 (0.32) |
| sc3 s7 | 15.33 (1.15, n=5) | 13.45 (0.23, n=7) | 14.97 (0.22) | 12.23 (0.39) | 12.15 (0.49, n=5) |

- **The sc2 −20% is RESOLVED exactly.** PBP-H − PB = +0.2/−0.0 (≪ σ_s
  both seeds); the same-binary PBH0 control differs from PBP-H ONLY in
  `RWM_HONEST_CAP` and reproduces the Anchor-Hygiene regression
  (−18%/−22%; +14.0/+16.8 for the K/R law alone, ≥ 2.3σ). Gauges: PBP-H
  sc2 cap latches 1024 (the legacy-proven point) with khr 1.2–1.3 and
  honest btlbw ~9.1–9.5k; PBH0 cap ~150–175.
- **sc3 carries a small named residual**: PBP-H −0.66/−0.36 vs PB
  (−4.3%/−2.3%; ≈2.8σ at s42, inside PB's σ at s7). The honest cap sits
  at ~355 (btlbw 1869 ≈ 0.9× truth at N=1 — the sampler IS honest on a
  single c3 — × the law) vs the legacy 1024-latch/682: the deep store's
  last ~4% at c3 is real tail-runway beyond one recovery round. PBH0
  −13% shows the K/R terms recover most of it.
- N=1 percap identity: PBP-H engages no accounts at N=1 (the law is
  N ≥ 2-gated); its sc2/sc3 deltas are entirely the anchor-sum arm.
  C1P-H − C1 = +1.9/+2.2 sc2, −0.3/−0.1 sc3 (≪/≈ σ_s) — inert under
  Copa as designed. Same-session ceilings: **Σ-PB c7 = 153.2/154.1,
  c8 = 92.1/92.4; Σ-C1 c7 = 134.4/131.0, c8 = 79.3/77.7.**

Duals (mean (σ_s) [per-run values]; Σ-ratio = arm / same-family Σ):

| cell | arm | mean (σ_s) [runs] | Σ-ratio |
|---|---|---|---|
| c7 s42 | PBS | 130.78 (4.39) [134.3 130.0 126.0 135.6 127.5 124.5 133.1 135.3] | 0.85 |
| | PBP-G-old | 137.08 (6.48) [147.3 133.2 133.1 139.9 137.2 126.9 135.3 143.7] | 0.89 |
| | PBP-H | **137.32 (9.16)** [138.5 147.2 146.6 147.2 126.0 128.0 127.9 137.2] | **0.90** |
| | C1 | 79.41 (5.30) [86.5 84.9 77.1 74.8 82.8 82.0 72.2 75.0] | 0.59 |
| | C1P-H | **91.56 (4.20)** [84.3 97.5 96.3 88.2 91.4 91.4 93.0 90.4] | **0.68** |
| c7 s7 | PBS | 130.18 (20.94) [128.5 134.7 137.5 140.6 84.5* 142.8 130.3 146.4] (*collapse run 80.6) | 0.84 |
| | PBP-G-old | 132.12 (11.70) [129.3 145.0 114.4 143.1 139.1 141.9 123.8 120.4] | 0.86 |
| | PBP-H | **137.80 (11.62, n=6)** [124.5 129.1 149.4 128.6 145.7 149.6] | **0.89** |
| | C1 | 81.40 (5.11, n=6) [88.6 77.4 75.5 78.2 84.5 84.2] | 0.62 |
| | C1P-H | **94.46 (6.32, n=7)** [85.3 101.0 97.5 90.8 102.1 95.6 88.9] | **0.72** |
| c8 s42 | PBS | 63.54 (10.87) [73.1 50.0 56.5 48.3 76.7 71.6 69.4 62.7] | 0.69 |
| | PBP-G-old | 47.24 (5.47) [46.5 58.0 41.2 41.2 50.1 49.1 44.4 47.4] | 0.51 |
| | PBP-H | **51.06 (3.05)** [55.9 51.4 51.4 49.5 55.1 47.6 49.2 48.4] | **0.55** |
| | C1 | 53.96 (4.26) [56.9 54.2 57.1 58.3 53.9 54.4 52.2 44.7] | 0.68 |
| | C1P-H | **45.42 (4.79)** [47.2 50.3 41.8 44.6 52.1 46.5 37.0 43.9] | **0.57** |
| c8 s7 | PBS | 57.72 (14.99, n=7) [56.2 41.2 52.1 72.3 83.7 51.7 46.8] | 0.62 |
| | PBP-G-old | 46.37 (7.76, n=5) [48.2 53.6 52.8 41.9 35.2] | 0.50 |
| | PBP-H | **49.80 (5.26, n=6)** [48.3 40.8 49.5 56.4 52.7 51.2] | **0.54** |
| | C1 | 56.69 (1.84) [54.6 56.8 58.4 55.9 55.9 59.2 58.5 54.3] | 0.73 |
| | C1P-H | **46.91 (2.85, n=6)** [44.2 47.3 44.9 50.1 50.5 44.5] | **0.60** |

**Reading, effect-by-effect:**

1. **c7 is preserved-or-improved under honest caps**: PBP-H 0.90/0.89×Σ —
   the ≥0.87 target met BOTH seeds, above PBS both seeds (+6.5/+7.6;
   PBS-s7 carries its documented collapse run), tie with the knee-clamped
   control at s42 (+0.2) and +5.7 at s7. The Copa c7 percap win holds a
   third session (C1P-H − C1 = +12.2/+13.1, ≥2σ).
2. **c8: honest caps improve the percap arm but do NOT reach the pooled
   bar.** PBP-H − PBP-G-old = +3.8/+3.4 (same sign both seeds, ≈0.7σ of
   the control's spread — a real but small unlock) with σ tightened
   (3.1/5.3) and the slow-path parking HALVED at the gauge level (all-rep
   p1 rtt: p50 358→204 ms, p90 943→433, max 1084→930). But PBP-H < PBS
   on both seeds (−12.5/−7.9; PBS σ 10.9/15.0 vs PBP-H 3.1/5.3 — the
   pooled arm buys its mean with spread), and 0.9×Σ is not approached
   (0.54–0.55). C1P-H < C1 again (−8.5/−9.8, 2–5σ; guard session:
   −9.6/−11.2 — reproduced cross-session).
3. **The no-borrowing tax is CONFIRMED as the c8 binder.** Under Copa the
   caps are honest BY CONSTRUCTION (cwnd_i) and were honest in BOTH
   percap sessions — yet account isolation loses ~0.13–0.16×Σ to the
   pooled Σcwnd law at the asymmetric cell, twice, both seeds. Under
   plain+BBR the honest-cap fix removed most of the cap-hygiene residual
   and the arm still trails PBS by a similar margin. The structural cost:
   out_fast ≤ cap_fast forbids the fast path from consuming the slow
   path's unused share, which the pooled law grants for free.
4. **New named residual (iii), measured at the gauge**: at c8 the SLOW
   path's send-interval anchor still over-reads ×3–5 under multipath
   placement (p1 btlbw 5.8–10.8k vs ~2.1k truth; the SAME sampler reads
   0.9× truth at sc3 N=1 and ≈1× on the c8 FAST path) — suspected
   frontier-advance burst attribution: a slow-hole fill releases a burst
   of already-received fast-path symbols into the cumulative frontier.
   Consequence: cap_slow reads 1107–2048 instead of the derived ~500, so
   the own-pick parking channel is only PARTLY closed under plain+BBR
   (the honest law computed on a dishonest input; under Copa this
   residual does not exist and the borrowing tax is isolated cleanly).

**VERDICT + FLIP DECISION.** Flip criteria were "clean two-seed both-CC
wins on BOTH cells with singles clean": c7 ✓ (both CC, both seeds),
singles ✓ at sc2 (exact), −4.3% named at sc3, c8 ✗ (PBP-H < PBS and
C1P-H < C1, both seeds). **`RWM_STORE_PERCAP` stays DEFAULT OFF** (and
`RWM_PLAIN_RS`/`RWM_HONEST_CAP` remain measurement arms; shipped tree
byte-identical). What this battery settled: the GUARD-RESULTS residual
(i) — own-pick parking under the knee-clamped cap — is FIXED as far as
honest inputs allow (sc2 exact, c8 partial behind residual (iii)), and
with both parking channels closed the remaining c8 gap is the account
structure itself. **The roadmap REDIRECTS: the c8 lever is no longer cap
hygiene but bounded account borrowing** — an account lending headroom it
cannot use (rate_i·RTprop_i satisfied) to a sibling. It does NOT fall
out of the floor-clock law cleanly (a borrowed symbol parks on the
LENDER's account but flies on the BORROWER's pipe, so the lender's
dwell-bound derivation no longer describes its own queue — a new law is
needed, not a clamp), so per discipline it is NAMED, NOT BUILT.
Alternatively: accept isolation's asymmetric-cell tax as structural and
keep pooled PBS at c8 (the standing record, 0.62–0.74×Σ across
sessions).

**Controls / discipline:** mechanism-liveness audit over all 286
captured runs (320 invocations): every PBP-H run carries percap+guard+
honest-cap+sampler echoes, PBH0 sampler-only, PBP-G-old percap+guard
only, PBS path-scaled only, PB/C1 none — every one of the 31 mismatch
lines pairs with one of the 34 aborted invocations (stale-log class;
per-file blocks_without_result == the missing n, all seed-7, the
documented topo-ping double-abort; RETRY per protocol; no captured
result discarded; n quoted wherever < 8). Zero DNFs in captured runs.
Session drift visible again (PBS-c8 0.72/0.67 → 0.69/0.62 across
sessions) — all verdicts same-session interleaved. CPU lines captured
per invocation in the logs (`CPU:` per run); c7 PBP-H remains below the
~150 parallelization threshold this session (137–138 means).

### BORROWING RESULTS (2026-07-19, the HONEST-CAP follow-on, branch `feat/store-borrowing`, commit 477ab32) — bounded account borrowing DERIVED (paper §16.22), BUILT (`RWM_STORE_BORROW`), and L1-MEASURED: the law behaves exactly as derived at every gauge (c7 loans IDENTICALLY zero — the symmetric-neutrality theorem measured; c8 loans strictly one-directional slow→fast, bounded, repaid on ack; the #86 parking direction never occurs) and the c7 percap win is preserved at 0.90/0.89×Σ — but the c8 tax is NOT repaid: PBP-B < PBS both seeds, because the honestly-lendable slack (~the slow runway term, low hundreds of symbols) is small against the pooled N×knee depth — the derivation's own named limit (3), now measured. **Flip NO; `RWM_STORE_PERCAP`+`RWM_STORE_BORROW` stay DEFAULT OFF; the pooled path-scaled PBS design is VINDICATED as the c8 answer and percap(+borrowing) is the symmetric-cell tool.** Residual (iii) is PARTLY closed by the flight-witness attribution fix (below).

**Part 0 — residual (iii) attributed and partly fixed (`resolve_flight_path`,
rides `RWM_PLAIN_RS`; `RWM_RS_ATTR=0` = legacy control).** The slow-path
multipath over-read is (at least half) a delivery-ATTRIBUTION defect, not a
sampler defect: a seq lost (or presumed lost) on one path and retransmitted
cross-path was credited to its LAST-sent path even when the ack arrived
sooner after the retransmit than that path's RTprop — impossible for the
retransmitted flight, so the delivering copy was the ORIGINAL (a spurious
retransmit; the gap was ack latency). At c8 the fast→slow retransmit
stream (cross-path placement avoids the original path) thus advanced the
SLOW path's delivered counter for symbols that flew on the FAST path. The
CopaFeed now keeps the previous distinct-path commitment and applies a
floor-clock flight witness at attribution: credit the last flight only if
its age ≥ RTprop(its path), else the previous flight. No new constants;
unknown RTprop = legacy (warm-up safe); the full Copa-sole feed keeps
legacy attribution (C1 arms untouched). MEASURED at L1-c8 (DIAG `xattr=`
cross/witness): 1057–1857 cross-path-history attributions per 25 MB run,
of which the witness reclassifies 57–76% — the phantom class is that
large; slow-path btlbw p50 4819 → 2951 (truth ≈2.1k: ×2.3 → ×1.4 over)
and cap_slow now differentiates off the knee in part of the reps
(observed 584 ≈ the derived ~500). HONEST LIMITS, named: (a) p90 btlbw
remains 8.5–10.4k in BOTH arms and cap_slow's tail stays knee-adjacent —
a SECOND over-read channel persists (suspect: same-path retransmit
re-snapshots and/or SACK-advertisement-truncation burst attribution;
sub-residual (iii-b), NOT built); (b) the witness arm reads LOWER c8
throughput than the legacy-attribution control (PBP-H 56.58/50.98 vs
PBP-H0 60.02/56.03; ≈0.5–1σ, same sign both seeds) — anchor honesty is
again mildly load-bearing at c8-plain, the third instance of the §16.19
circularity class. L0 mechanism evidence: xattr fires 226–319/run with
50–80% witness-reclassified on the dual shim; N=1 computes nothing (no
cross-path commits exist).

**The build (env `RWM_STORE_BORROW`, default OFF ⇒ shipped byte-identical;
requires the percap stack).** Placement order for a cap-full own pick:
BORROW first (stay on the picked pipe, charge the lender with max lend
room), else the guarded redirect, else FULL/backpressure. The loan ledger
(`percap_loans` seq→(lender,flyer) + `percap_lent`/`percap_borrowed`
gauges) charges the LENDER's account at `percap_charge` and repays on the
SAME acks that release the store (SACK + cumulative twins). Admission
gate = the guarded gate opened additionally by any live lend edge
(`percap_lend_edge_exists`). fly_j = out_j − lent_j + borrowed_j corrects
account→pipe occupancy for T_return. Rates for the law: honest BtlBw_i
(plain, `RWM_PLAIN_RS`) / cwnd_i/RTprop_i (Copa feed); warm-up lends 0.
DIAG `ln=lent/borrowed` per path + `loan=active/cum`; liveness echo
"bounded store borrowing ACTIVE". Unit evidence (lib 361/361, 6 new): the
lend-room reservation + post-loan solvency + one-directionality (the c8
miniature: slow lends 97 of its runway slack; a cap-full slow borrower
gets 0 from even an EMPTY fast account), the symmetric-neutrality theorem
(reservation − cap = anchor > 0 ⇒ loans ≡ 0 for every lender state), the
degenerate equivalences (T_return→0 ⇒ pooled cap−out sharing; all-full ⇒
the unguarded FULL), the loan-ledger lifecycle (charge lender / fly
borrower / SACK+cumulative repayment, idempotent), the admission-gate
composition, and the flight witness (spurious-retransmit ack younger than
RTprop credits the original flight).

**L1 battery (VM 10.1.5.16, 2026-07-19 18:25–19:07 UTC; binary sha256
0995b3f287ef6f27… = commit 477ab32 `feat/store-borrowing`, SAME binary
every arm; E5-2650 v3 aes+avx2+pclmulqdq in every log header
(post-divide); 25 MB × 1 run/invocation × 8 reps, arms interleaved
round-robin per rep, fresh tunnel per invocation, seeds 42+7, RWM_DIAG=1
everywhere; driver `/home/vibe/borrow_battery.sh` (+`borrow_all.sh`),
logs `/home/vibe/borrow/{sc2,sc3,c7,c8}-s{42,7}.log` + per-run
`diag-*.log`).** Arms — duals: PBS (pooled path-scaled, the incumbent),
PBP-H (percap+guard+honest+witness, the no-borrow control), PBP-B
(= PBP-H + `RWM_STORE_BORROW=1`), C1 (Copa wire-signal), C1P-B
(Copa + percap + borrow); c8 adds PBP-H0 (= PBP-H + `RWM_RS_ATTR=0`, the
(iii) attribution control); singles: PB, PBP-B, C1, C1P-B (identity).

Singles — N=1 identity + same-session Σ:

| cell | PB (σ_s) | PBP-B (σ_s) | C1 (σ_s) | C1P-B (σ_s) |
|---|---|---|---|---|
| sc2 s42 | 78.85 (3.34) | 77.58 (3.43) | 68.98 (1.47) | 67.81 (1.00) |
| sc2 s7 | 76.33 (3.40, n=7) | 74.21 (4.39, n=7) | 65.51 (1.40) | 65.83 (2.93, n=5) |
| sc3 s42 | 15.49 (0.18) | 14.82 (0.31) | 12.19 (0.24) | 11.92 (0.49) |
| sc3 s7 | 15.19 (0.88, n=6) | 14.91 (0.22) | 12.06 (0.28, n=7) | 12.39 (0.31, n=7) |

Identity HOLDS: sc2 Δ −1.3/−2.1 ≪ σ; C1P-B ≈ C1 everywhere; sc3 PBP-B
−0.67/−0.28 = the KNOWN honest-cap sc3 residual (−4.3%/−1.8%; last
session −0.66/−0.36 with no borrow gate — borrowing computes nothing at
N=1). Same-session ceilings: **Σ-PB c7 = 157.7/152.7, c8 = 94.3/91.5;
Σ-C1 c7 = 138.0/131.0, c8 = 81.2/77.6.**

Duals (mean (σ_s) [runs]; Σ-ratio = arm / same-family Σ):

| cell | arm | mean (σ_s, n) | Σ-ratio |
|---|---|---|---|
| c7 s42 | PBS | 132.20 (8.54) [129.1 118.7 121.4 142.1 139.3 137.4 137.5 132.1] | 0.84 |
| | PBP-H | 141.85 (7.47) [148.1 145.7 142.3 148.7 145.4 144.2 129.3 131.2] | 0.90 |
| | PBP-B | **141.69 (6.25)** [144.2 146.6 139.1 135.3 147.1 147.7 130.4 143.0] | **0.90** |
| | C1 | 78.67 (4.22) | 0.57 |
| | C1P-B | **92.53 (8.14)** | **0.67** |
| c7 s7 | PBS | 126.59 (19.93) [141.0 134.3 135.6 140.7 79.4 129.4 126.6 125.8] | 0.83 |
| | PBP-H | 143.16 (5.70, n=5) | 0.94 |
| | PBP-B | **136.51 (6.65, n=7)** [125.6 134.7 129.9 143.6 141.3 140.3 140.1] | **0.89** |
| | C1 | 80.93 (4.50, n=7) | 0.62 |
| | C1P-B | **95.25 (6.98, n=6)** | **0.73** |
| c8 s42 | PBS | 68.08 (8.96) [56.1 72.0 59.6 61.4 76.8 78.5 63.3 77.0] | 0.72 |
| | PBP-H | 56.58 (9.18) [43.1 48.6 54.8 55.1 65.2 71.2 52.3 62.3] | 0.60 |
| | PBP-H0 | 60.02 (6.71) [51.6 65.2 63.1 65.5 59.6 64.9 62.2 47.9] | 0.64 |
| | PBP-B | **52.84 (6.54)** [52.4 48.9 43.2 60.3 58.1 56.4 44.9 58.5] | **0.56** |
| | C1 | 56.25 (4.28) | 0.69 |
| | C1P-B | **55.97 (3.55)** | **0.69** |
| c8 s7 | PBS | 65.87 (10.96, n=7) [64.4 70.3 76.5 49.0 77.8 69.3 53.9] | 0.72 |
| | PBP-H | 50.98 (4.30, n=7) [51.3 47.4 54.7 55.2 50.8 43.4 54.0] | 0.56 |
| | PBP-H0 | 56.03 (3.52, n=5) [53.0 55.4 52.5 59.3 60.1] | 0.61 |
| | PBP-B | **56.80 (6.56)** [62.5 54.7 62.0 59.5 62.5 53.2 56.7 43.3] | **0.62** |
| | C1 | 52.23 (4.16, n=5) | 0.67 |
| | C1P-B | **55.66 (1.69, n=7)** | **0.72** |

**Reading, effect-by-effect:**

1. **c7 neutrality is EXACT — the theorem measured.** `loan=0/0` at every
   DIAG tick of every PBP-B c7 rep, BOTH seeds (41+47 ticks): loans are
   identically zero at the symmetric cell, as §16.22.3(c) proves
   (reservation − cap = anchor > 0). Throughput: PBP-B = PBP-H (−0.2 at
   s42, −6.6 ≈ 1σ at s7), ≥ 0.87×Σ target BOTH seeds (0.90/0.89), above
   PBS both seeds (+9.5/+9.9; PBS-s7 carries its documented 79.4 collapse
   run — the percap arms again have no collapse mode). The Copa c7 percap
   win holds a FOURTH session (C1P-B − C1 = +13.9/+14.3, ≥2σ).
2. **c8 — the mechanism works; the tax is NOT repaid.** Loans fire
   (plain: cum 34–916/run, active ~100–250; Copa: cum 1747–3318, active
   65–638), are strictly ONE-DIRECTIONAL (every nonzero `ln=` gauge:
   slow lends, fast borrows — the parking direction NEVER occurs, as
   §16.22.3(b) derives), and repay to 0. But PBP-B vs PBP-H is −3.7/+5.8
   (sign flips, ≪ joint σ — statistically NEUTRAL under plain+BBR), and
   PBP-B < PBS on BOTH seeds (−15.2/−9.1; PBS medians 67.6/69.3 vs
   PBP-B 55.3/58.1). The C8 0.9×Σ target is not approached (0.56–0.62);
   the pooled bar (0.72/0.72 this session) STANDS. The honestly-lendable
   slack — the slow account's runway term minus its reservation, ~50–250
   symbols — is an order of magnitude below the pooled arm's effective
   depth; §16.22.4 limit (3), measured.
3. **Copa datum (suggestive, not controlled): the account-isolation tax
   disappears in the borrowing arm.** C1P-B ≈ C1 (−0.3/+3.4, ≪/≈σ) where
   the two prior sessions' no-borrow Copa-percap arms lost −8.5/−9.8 and
   −9.6/−11.2 to C1 — with heavy loan traffic under honest cwnd caps.
   No same-session C1P (no-borrow) control was run, so this is a
   cross-session delta-of-deltas: bounded borrowing plausibly repays the
   ISOLATION tax to Copa-family parity — but C1 itself sits below PBS at
   c8, so it cannot change the flip.
4. **The (iii) witness at L1**: see Part 0 above — the spurious class is
   57–76% of 1057–1857 cross-path attributions/run; slow btlbw p50
   ×2.3→×1.4 over truth; p90 channel remains (iii-b); the honest arm
   costs −3.4/−5.1 vs the legacy-attribution control at c8-plain.

**VERDICT + FLIP DECISION.** Flip criteria were "c8 PBP-B ≥ PBS both
seeds both CC, c7 ≥ 0.87×Σ, singles clean": c7 ✓ (0.90/0.89, loans ≡ 0),
singles ✓ (identity; sc3 −4.3% pre-existing, named), c8 ✗ (PBP-B < PBS
both seeds; C1P-B ≤ PBS-equivalent). **`RWM_STORE_PERCAP`,
`RWM_STORE_BORROW`, `RWM_PLAIN_RS` all stay DEFAULT OFF** (shipped tree
byte-identical). What this battery SETTLED, with the law behaving exactly
as derived at every gauge: **the bounded-borrowing hypothesis is
answered — a principled lender-solvent loan bound cannot recover the
pooled law's c8 depth, because the pool's advantage is not the slow
path's unused honest headroom (small) but its unbounded willingness to
let the fast path run past every honest per-path derivation. The pooled
path-scaled design (`RWM_STORE_PATHS`, PBS) is VINDICATED as the c8
answer; per-path accounts (with or without borrowing) are the symmetric-
cell tool** (no collapse mode, 0.89–0.94×Σ, Copa +13–21). The roadmap
redirects accordingly: the c8 record stays pooled; remaining named
residuals are (iii-b) (the p90 slow-anchor channel) and the honest-anchor
throughput circularity at c8-plain (third instance).

**Controls / discipline:** mechanism-liveness audit over all 279
result-bearing runs: PBP-B/C1P-B carry the borrow echo (+ percap/guard,
+ honest+sampler on plain), PBP-H/PBP-H0 the no-borrow stack, PBS
path-scaled only, PB/C1 none — **0 mismatches**; the 25 mismatch lines in
the raw block audit all sit in blocks that captured NO result (the
documented aborted-invocation stale-log class, seed-7-dominated;
RETRY per protocol, no captured result discarded, n quoted wherever
< 8). Zero DNFs in 279 captured runs. All verdicts same-session
interleaved (PBS-c8 0.72/0.72 vs prior sessions 0.67/0.74, 0.72/0.67,
0.69/0.62 — drift class visible again). CPU lines captured per
invocation; c7 PBP-B ≈ 141 stays below the ~150 parallelization
threshold.

### Controls / caveats

- Shipped default byte-identical: env unset ⇒ no charge, no gate change,
  legacy dyn-cap expressions verbatim (percap computed AFTER them, only
  overriding when engaged). Suites green (below).
- **L0 numbers are mechanism evidence, NOT the C7/C8 verdict**: the L0
  netem shim drops BEFORE quinn (quinn sees a clean loopback — no
  substrate-CC dynamics), timers/CPU are a dev Windows box, and the L1
  operating point (BBR-under, real netem, real crypto) is not reproduced.
  What L0 CAN show: the accounts exist, size per-path, draw/release
  correctly, and the admission gate keys on them — which it does (DIAG
  gauges above).
- The `np` 2→1 live-path flap under saturation (#84 residual bullet 3) is
  untouched by this feature and remains a shared contributor at both cells.

### Tests

`cargo test -p raptorpath --lib` 322/322 (6 new, listed above);
`-p raptorpath-math` all green (134 across suites); `gate_suite` 15/15
release; `mtu_blackhole_wedge` 2/2 (wedge fix untouched); `perf_loopback`
8/8; `copa_sole_loopback` 1/1, `fmtcp_loopback` 1/1, `daps_loopback` 1/1
— all release. Shipped default byte-identical: env unset ⇒ the percap
block computes nothing, the legacy dyn-cap expressions and the tx_paused
gate are verbatim pre-commit.

L1-battery session re-run on the `meas/percap-battery` tree (main 8ef5ff1
+ the RWM_TAPER_R harness forward; no binary-code change, NO default
flipped): lib 332/332, math 136 green, `gate_suite` 15/15 release,
`mtu_blackhole_wedge` 2/2, `perf_loopback` 8/8, the three loopbacks 1/1
each — all green 2026-07-19.

Guard session (`fix/percap-redirect-guard` 689b9f1, NO default flipped —
guard active only under `RWM_STORE_PERCAP`, itself default OFF): lib
335/335 (3 new), math 136 green, `gate_suite` 15/15 release,
`mtu_blackhole_wedge` 2/2, `perf_loopback` 8/8, `copa_sole_loopback` /
`fmtcp_loopback` / `daps_loopback` 1/1 each — all green 2026-07-19.

Honest-cap session (`feat/percap-honest-cap` 5d30c02, NO default flipped —
the honest law consulted only under `RWM_PLAIN_RS`, itself default OFF;
`honest_cap_on` structurally requires the anchor gate, so the shipped path
is byte-identical): lib 355/355 (5 new), math 136 green, `gate_suite`
15/15 release, `mtu_blackhole_wedge` 2/2, `perf_loopback` 8/8,
`copa_sole_loopback` / `fmtcp_loopback` / `daps_loopback` 1/1 each — all
green 2026-07-19 (`suites-final.log`, session scratchpad).

Borrowing session (`feat/store-borrowing` 477ab32, NO default flipped —
`RWM_STORE_BORROW` requires the percap stack, itself default OFF; the
flight witness engages only with the `RWM_PLAIN_RS` sampling-only feed):
lib 361/361 (6 new), math 136 green, `gate_suite` 15/15 release,
`mtu_blackhole_wedge` 2/2, `perf_loopback` 8/8, `copa_sole_loopback` /
`fmtcp_loopback` / `daps_loopback` 1/1 each — all green 2026-07-19.

## Copa Competitive Mode + Cross-Traffic (2026-07-19) — Copa §2.2 mode switching BUILT on the wire signal (verified mechanism, faithful law, unit-proven) + the FIRST shared-bottleneck battery: at the GE c2 cell Copa-sole does NOT starve vs Cubic (0.88–0.90 share, compete irrelevant); at the CLEAN bottleneck Copa-sole starves (2.2 vs Cubic's 93, share 0.023) and competitive mode CANNOT restore share — because δ is NOT the binder (fixed δ=0.001 probe: no change): the starvation is the plain-window ARQ/retention pipeline under contention tail-drop (pool 1024 × 3.3 s dwell = Little's-law 2.4 Mbit), a CC-INDEPENDENT named blocker; BBR-under reference = 0.24 share at a 305–316 ms standing queue (Copa arms keep 20–40 ms) — substrate-CC default flip stays CLOSED with the gate MOVED from "no competitive mode" to the contention-recovery pipeline (branch `feat/copa-compete`, roadmap item 6, tasks #80/#82)

*Decision record: → [ADR-0062](adr/0062-copa-wire-signal-competitive-mode.md)*

Roadmap item 6. The CONSOLIDATED VERDICT §2 named gap: "Copa-lite has NO
TCP-competitive mode … and no cross-traffic cell has ever been measured
(this gates BBR's fairness case too). Gates ANY substrate-CC default flip."
Both halves are now closed: the mechanism is built (faithfully, from the
paper), and the first cross-traffic battery exists — with a verdict nobody
had: the gap that actually blocks deployment on shared CLEAN bottlenecks is
not the CC at all.

### The verified mechanism (citation-accurate)

Copa: Venkat Arun and Hari Balakrishnan, "Copa: Practical Delay-Based
Congestion Control for the Internet", NSDI 2018 — mechanism verified against
the paper text (people.csail.mit.edu/venkatar/copa.pdf; the earlier ledger
citations "Copa §4" were imprecise: mode switching is the paper's **§2.2**,
"Competing with Buffer-Filling Schemes"):

- **Two modes**: default (δ = 0.5 in the paper) and "a competitive mode
  where δ is adjusted dynamically to match the aggressiveness of typical
  buffer-filling schemes".
- **Detection**: Copa's dynamics empty the queue at least once every 5·RTT
  when only Copa flows share the bottleneck (paper §3). "Hence if the
  sender sees a 'nearly empty' queue in the last 5 RTTs, it remains in the
  default mode; otherwise, it switches to competitive mode. We estimate
  'nearly empty' as any queuing delay lower than 10% of the rate
  oscillations in the last four RTTs; i.e., d_q < 0.1·(RTTmax − RTTmin)
  where RTTmax is measured over the past four RTTs and RTTmin is our
  long-term minimum."
- **The competitive law**: "In competitive mode the sender varies 1/δ
  according to whatever buffer-filling algorithm one wishes to emulate
  (e.g., NewReno, Cubic, etc.). In our implementation we perform AIMD on
  1/δ based on packet success or loss … In competitive mode, δ ≤ 0.5. When
  Copa switches from competitive mode to default mode, it resets δ to 0.5."
- **Switch-back / hysteresis**: the same 5-RTT nearly-empty window on both
  edges; queues empty every ~5 RTT in competitive mode too when no
  buffer-filler is present, so erroneous switches self-correct in a few
  RTTs (the paper documents brief flaps around losses and accepts them).

### As built (scheduler/mod.rs, feat/copa-compete; env `RWM_COPA_COMPETE`, default OFF)

On top of the #82 wire clock — detection on quinn's packet-timed RTT (the
app-echo clock would re-import the #80 self-signal as phantom competitors):

1. Detector per wire RTT sample: monotonic max-deque over the past
   4·SRTT (RTTmax), long-term floor = the wire 10 s min; nearly-empty mark
   when d_q ≤ max(0.1·(RTTmax − RTTmin), DQ_FLOOR) — the floor guard keeps
   a zero-variance clean/idle link from reading "never empty".
2. Mode evaluation + AIMD once per SRTT update (the paper's per-RTT
   cadence), skipped during the startup ramp. Loss signal = the
   pass-through shim's recorded `congestion_events` (quinn's wire loss
   detection — same layer as the d_q clock), diffed per window.
3. **δ(hint) composition — the hint sets the BASE, competition adapts
   around it**: the paper's 0.5 is its default-mode δ; ours is
   δ_base = δ(hint) = 0.5/ζ (paper §12.4). Competitive mode enters at
   δ_base, AIMD keeps 1/δ ≥ 1/δ_base (the paper's "δ ≤ 0.5" generalized),
   switch-back resets δ = δ_base; 1/δ capped so the coupling cap's 2/δ
   stays ≤ MAX_CWND.
4. DIAG: `cmp=<C|D><switches>/<live δ>` per path; config echo
   `compete=true/false`. Env unset ⇒ every path byte-identical (gate_suite
   15/15 release on the final tree).

Unit evidence (6 new tests, lib 338/338): detection fires under a synthetic
never-draining queue and NOT under one that drains every ≤3 updates; the
AIMD follows the verified arithmetic exactly (1/δ: 2→3, loss → floor at
1/δ_base, 3→4, stale counter ≠ loss, real halving above the floor 5→2.5);
switch-back on drain resets δ to base; gate-off never switches; the env
gate requires the wire law.

### L1 battery — the first cross-traffic cells (VM 10.1.5.16, 2026-07-19 ~10:24–11:48 UTC; binary sha256 6da66428ce2ba91c… commit 0f9bb2b; host-passthrough E5-2650 v3 era; driver `tools/l1/cross_battery.sh` → `cross_traffic.sh`; logs `/home/vibe/copacompete/{c2,clean}-{solo,copa,compete,bbr}-s{42,7}.log` + per-run `diag-*`; 25 MB × 1 run/invocation × 8 reps, arms interleaved round-robin per rep, seeds 42 AND 7, RWM_DIAG=1 everywhere; iperf3 3.19.1 Cubic INSIDE the rp-* namespaces sharing pathA's netem qdisc, 2 s head start, per-interval JSON overlapped onto the rp transfer window)

Arms (all PLAIN, single path): **solo** = passthrough + `RWM_COPA_COMPETE=1`,
NO competitor (false-positive control) · **copa** = passthrough, compete OFF,
vs 1 Cubic · **compete** = passthrough + `RWM_COPA_COMPETE=1` vs 1 Cubic ·
**bbr** = `RWM_QUIC_CC=bbr` vs 1 Cubic. Liveness: every passthrough arm log
carries `feed ACTIVE` + `compete=` echo; the solo control matches the known
#82 solo numbers (c2: 68.8/65.1 vs historic C1 68.09/64.27). Seed-7 n<8 =
the known topo-ping double-abort class (recorded; the first s7 sub-battery
lost whole invocations to lib.sh's `set -e` before the harness guarded it —
discipline #7 bit again, now `set +e` + topo-retry-once in the script;
aborted logs preserved in `copacompete/aborted-s7-run1/`).

**c2 — the specified cell (100 Mbit, 10 ms, GE 1.3/50 ≈ 2.5% loss). rp
Mbit/s mean (σ_s) [runs] · Cubic Mbit/s · rp share · Jain:**

| arm | s42 | s7 |
|---|---|---|
| solo | 68.8 (1.0) [67.1 68.1 68.3 68.9 69.0 69.1 69.8 70.3] | 65.1 (1.4, n=6) [62.6 64.3 65.6 65.7 66.1 66.1] |
| copa | 61.5 (1.6) · Cubic 7.2 · share 0.90 · J 0.62 | 59.4 (2.2, n=6) · 8.2 · 0.88 · 0.64 |
| compete | 61.3 (1.0) · Cubic 6.9 · share 0.90 · J 0.61 | 60.5 (1.8, n=7) · 7.2 · 0.89 · 0.62 |
| bbr | 73.8 (5.1) · Cubic 3.0 · share 0.96 · J 0.54 | 73.2 (8.0) · 4.0 · 0.95 · 0.56 |

rp wire-queue p50 stays 2–11 ms in every Copa arm vs bbr's 9–60 ms —
the #82 queue profile carries over unchanged under cross-traffic.

**clean — the buffer-filler mechanism cell (100 Mbit, 10 ms, no loss;
netem seed inert here — the two "seeds" are two interleaved sessions):**

| arm | s42 | s7 |
|---|---|---|
| solo | 71.1 (0.5), zero switches 8/8 | 71.2 (0.4), zero switches 8/8 |
| copa | **2.21 (0.47)** [1.2 2.0 2.2 2.4 2.4 2.5 2.5 2.7] · Cubic 93.3 · share **0.023** · J 0.52 | **2.15 (0.44)** [1.2 1.8 2.1 2.3 2.4 2.4 2.5 2.5] · 93.4 · 0.023 · 0.52 |
| compete | **2.37 (0.19)** [2.0 2.3 2.3 2.4 2.4 2.5 2.5 2.6] · Cubic 93.0 · share 0.025 · J 0.52 | **2.24 (0.50)** [1.1 1.9 2.3 2.5 2.5 2.5 2.5 2.7] · 93.1 · 0.024 · 0.52 |
| bbr | 22.34 (0.31) · Cubic 70.7 · share 0.240 · **J 0.79** | 22.79 (0.28) · 69.9 · 0.246 · 0.79 |

**Queue/tail while competing (clean, rp sender DIAG, per-run p50 pooled):**
Copa arms wireQ p50 ≈ 7–73 ms (typ. 20–40; p90 83–263) — the queue rp
experiences is Cubic's occupancy, not its own; **bbr wireQ p50 304–316 ms
(p90 406–460) in EVERY run** — BBR buys its 0.24 share with a ~10× deeper
standing queue. Competitive mode itself cost nothing measurable: δ never
adapted past 0.0032 (1.6× base tolerance), wq unchanged vs the compete-OFF
arm.

**Mode-switching liveness (the mechanism datum):** clean/compete engages in
8/8 + 8/8 runs (3–7 switches, C-mode 56–91% of DIAG samples, δ 0.005 →
0.0032–0.0043); clean/solo NEVER engages (0/16 — false-positive control
clean). c2/solo shows a TRANSIENT engagement class (1 switch in 11/14 runs,
δ_min ≥ 0.0041, C-share of samples up to 1.0 on the short ~3 s transfers):
GE loss + jitter keeps d_q above the 10% threshold for >5 RTT stretches —
the paper's documented erroneous-switch class. Impact nil: solo throughput
matches #82's C1 within σ (68.8/65.1 vs 68.09/64.27), queue unchanged, δ
pinned ≈ base by the AI-vs-base scale (+1 per RTT on 1/δ = 200) and the
loss-MD floor. Recorded, not fixed: it is behaviorally inert by
construction exactly where it fires.

**The δ-null probes (the attribution datum; clean, s42, copa arm, n=3+2):**
fixed `RWM_COPA_DELTA=0.001` (tolerance 1/δ = 1000 pkt ≈ the ENTIRE
1000-pkt qdisc) → 2.5/1.1/2.4 Mbit — unchanged from base δ's 2.21;
`RWM_CC_PACE=0` (ack-clocked) → 2.4/2.5 — unchanged. δ is NOT the binder;
neither is the pacer.

**The actual binder (sender DIAG forensics, clean compete/copa runs):**
win=1024/1024 pegged (the single-path plain outstanding pool), TUN paused
46–100%, cwnd healthy at 460–580 (the δ-law's target — NOT the constraint),
in_flight only 100–250, app-echo RTT 2–5 s = the pool dwell, retx a trickle
(~4/s), goodput ≈ pool/dwell: 1024 sym × 1250 B × 8 / 3.3 s ≈ **2.5 Mbit/s
— Little's law on wall #7's pool, reproduced at a shared bottleneck.** The
chain: Cubic keeps the shared qdisc ~full → rp datagrams tail-drop in
bursts → holes freeze the in-order frontier → the 1024-pool fills with
sent-unacked symbols → TUN pauses → goodput = recovery rate, and the
plain-ARQ recovery under multi-second effective feedback serializes.
BBR-under passes 22 Mbit through the SAME app pipeline because its
substrate window keeps ~250 pkt resident in the queue (wq 310 ms): high
occupancy ⇒ proportionally admitted arrivals ⇒ loss rate low enough that
the frontier keeps moving. A delay-based controller cannot hold that
occupancy BY DESIGN — and δ-deep tolerance does not help because the pool,
not cwnd, is what gates emission (probe-proven).

### VERDICT

1. **The named mechanism gap is CLOSED in code**: Copa §2.2 competitive
   mode, faithful to the verified paper text (detection + AIMD on 1/δ +
   δ_base composition + switch-back), unit-proven, liveness-proven at L1,
   default OFF, byte-identical when unset.
2. **c2-class (lossy) shared bottlenecks: Copa-sole is cross-traffic-SAFE
   today** — 0.88–0.90 share vs Cubic with or without competitive mode
   (Cubic is Mathis-bound to 7–8 Mbit by the channel loss, and gets MORE
   under Copa than under BBR: 6.9–8.2 vs 3.0–4.0 — Copa-sole is the
   Cubic-friendlier neighbor, matching the paper's co-existence claim),
   queue advantage intact.
3. **Clean shared bottlenecks: the starvation baseline is REAL and
   measured** — share 0.023 (2.2 vs 93 Mbit, 1/32 of solo), both sessions.
   **Competitive mode as specified does NOT restore a fair share here**
   (2.24–2.37, Δ vs compete-OFF within σ) — and the null is ATTRIBUTED:
   the δ lever itself is null at this cell (δ=0.001 probe unchanged); the
   binder is the plain-window ARQ/retention pipeline under
   contention-induced tail-drop (pool Little's law, forensics above), a
   transport-layer mechanism BELOW the CC policy surface.
4. **BBR's fairness case, measured for the first time**: BBR-under does
   not starve (0.24 share, Jain 0.79) but is under-fair and pays 305–316 ms
   of standing queue (p90 to 460) — and at c2 it crushes the Cubic flow to
   3–4 Mbit (share 0.95–0.96, Jain 0.54–0.56). Neither CC family passes a
   fair-share bar on the clean cell: Copa starves, BBR squeezes.
5. **The Copa tail-vs-compete tradeoff never engaged**: δ adapted at most
   1.6× base tolerance (the AI step +1 is small against 1/δ_base = 200 and
   contention losses MD it back to the floor each sawtooth), so the
   documented "competitive mode = Cubic-like queues" cost was not paid —
   but it also bought nothing, because δ was not the binder.
6. **Flip-readiness: NO substrate-CC default flip; the gate MOVED.** The
   competitive-mode gap is closed, but the cross-traffic cell it was built
   to protect against reveals a deeper, CC-independent blocker: the
   contention-loss recovery pipeline (the 1024 single-path pool × frozen
   frontier under tail-drop). Until that is attacked, `passthrough` (with
   or without `RWM_COPA_COMPETE`) is deployable ONLY where the bottleneck
   is not shared with sustained loss-based bulk flows on a clean link;
   BBR-under remains the shared-bottleneck fallback; shipped default
   remains stock Cubic. NEW named follow-up (roadmap): **shared-bottleneck
   contention recovery** — the pool/frontier/ARQ chain under tail-drop
   (candidate levers: contention-scaled pool, loss-burst-aware NACK
   budget/cadence, FEC-protected retransmission of the blocker; measure
   against the clean cross-traffic cell; also re-opens the fairness case
   for BOTH CC families).

### Env / commands (reproduction)

```
# one invocation (topology + iperf3 competitor handled inside):
sudo env SEED=42 bash cross_traffic.sh <c2|clean> <solo|copa|compete|bbr> 25000000 300
# the interleaved battery:
sudo bash cross_battery.sh <c2|clean> 8 <42|7> 25000000 /home/vibe/copacompete 300
# probe knobs forward through the env: RWM_COPA_DELTA, RWM_CC_PACE, RWM_STORE_GAIN
```

### Tests

`cargo test -p raptorpath --lib` 338/338 (6 new competitive-mode tests);
`-p raptorpath-math` green; `gate_suite` 15/15 release (shipped default
byte-identical); `congestion_control` 19/19; `copa_sole_loopback` 1/1;
`mtu_blackhole_wedge` 2/2 — all on the final tree.

## Engine Parallelization (2026-07-19) — roadmap item 2, the receiver/sender task-parallelization lever PROFILED AT the ~150 threshold it went live at: the THIRD threading refutation, this time WITH the true binder named and measured — at the best c7 arm (137–144) BOTH processes pinned to ONE core each sustain full throughput (pinned mean 136.3 n=8 ≈ unpinned 136.2 n=10, both seeds), the engine receiver task runs 81–87% busy with a NEAR-EMPTY inbound queue, and the wire is measured FULL: multipath recovery-plane over-emission (retx ×1.8, repair ×2.2–2.5 the same-config single-path share) occupies ~25% of the dual wire ≈ exactly the Σ-gap; `RWM_ENGINE_PAR` NOT built (it would have measured noise); built instead: the `RWM_RDIAG` engine-saturation gauge, which also measured the REAL task-service walls (~19.5–20k sym/s sender emission, ~20–22k msgs/s receiver engine) that bracket the 187.7 sink ceiling and localize the parallelization threshold at c1-class cells only (branch `feat/engine-parallel`, tasks #84/#86)

*Decision record: → [ADR-0057](adr/0057-profile-before-parallelize.md)*

Task: roadmap item 2 — receiver/sender task parallelization, refuted by
the #84 profile below ~150 Mbit/sink, now LIVE at the symmetric cell
(best c7 arms 137–147 ≈ the threshold; engine sink 187.7; c7 Σ target
~157). PROFILE-FIRST discipline: build only what the profile demands.
The profile demanded nothing be built — and unlike #84's "no stage to
parallelize", this session names and quantifies the wall that IS there.

### Method

VM 10.1.5.16 (E5-2650 v3, aes+avx2+pclmulqdq in every log header —
post-divide), 2026-07-19 19:20–20:20 UTC. Binary sha256 40973e6b… =
commit d27ce30 = main 7c3343f + the `RWM_RDIAG` probe ONLY (default-off
instrument; shipped path byte-identical with env unset), SAME binary
every run. Best-c7 arm under profile = **PBP-H** (plain +
`RWM_QUIC_CC=bbr RWM_STORE_PERCAP=1 RWM_PLAIN_RS=1` — percap + guard +
honest caps + flight witness, the borrowing-battery configuration;
liveness echoes verified per run). 200 MB single-run c7 probes / 400 MB
c1-class probes; per-thread CPU snapshots (`/proc/<pid>/task`),
`perf record -F 397 -g` flat profiles both sides, `taskset -a` affinity
pins applied mid-run, system-wide `/proc/stat`+softirq+UDP-counter
capture, DIAG + the new RDIAG gauges. Mini-battery: arms interleaved
round-robin per rep, seeds 42+7, foreground polling only, rp-* netns
only; logs + drivers `/home/vibe/engpar/{profile/,mini-s42.log,
mini-s7.log,prof_run.sh,sys_probe.sh,mini_battery.sh}`. CRLF stripped
after tree sync (discipline item 10 — it bit again).

### STEP 1a — where the core-seconds go at 134–138 Mbit/s (c7 PBP-H)

Per-thread (400 MB @ 134.1): receiver process 1.34 cores, sender 1.59 —
both spread FLAT over the 6 tokio workers (0.20–0.27 each); no hot
thread. Flat perf, receiver: estimator math ~14% (`record_batch` 5.1 +
`exp/log_fma` 8.8), decoder+GF(256) ~5%, `WireMessage::deserialize`
3.0%, allocator ~6%, kernel futex/spin ~3%, `_aesni` 1.5% (crypto is
noise), quinn ack-path ~1.7%. Sender: `on_src_delivered_seq` 7.8% (the
percap/witness per-seq feed), estimator+FEC-rate control math ~18%
(`record_batch` 4.5, `predictive_loss_upper` 3.6, `compute_repair_rate`
3.4, exp/log 6.9), `run_window_sender` 2.6%, serialize 1.8%, BTreeMap
1.9%, sched_yield ~4%. Top symbol 5.1%/7.8% — FLAT both sides; no
parallelizable stage dominates (the #84 shape, reproduced at +35 Mbit).

### STEP 1b — the pin experiments kill threading at c7, again (harder)

Same binary, same arm, 200 MB, seed 42 (+ seed-7 battery reps):

| pin (taskset, whole process) | throughput (Mbit/s) | CPU recv·send (s/invocation) |
|---|---|---|
| none (n=5) | 131.3–138.7 (mean 135.0) | 15.5–16.0 · 18.1–18.6 |
| server → 1 core | **143.9** (best run of the session) | **9.7** · 16.6 |
| client → 1 core | 132.4 | 15.5 · **11.2** |
| both → 1+1 cores (n=6 s42, n=2 s7) | 132.5–140.5 (mean 136.3) | 9.9 · 11.1 |

**1 + 1 cores sustain the full operating point on both seeds** (pinned
mean 136.3, n=8, vs unpinned 136.2, n=10 pooled across the profile and
battery phases; the server-pinned run is the session's FASTEST). Pinning cuts the receiver's measured CPU ~40% (15.5→9.7 s)
at equal-or-better throughput: the unpinned 1.34/1.59 cores are ~⅓
scheduler-migration waste, not work. There is no thread-parallelism
deficit at c7 — with SIX cores available the engine uses the extra five
to go 0–3% slower.

### STEP 1c — the RDIAG gauge: the engine task has headroom at c7

New instrument (`RWM_RDIAG`, receiver engine task): busy fraction =
1 − time-awaiting-select, plus inbound msg-channel depth (cap 4096).

| cell / arm | Mbit/s | engine busy | msgs/s | q_avg / q_max |
|---|---|---|---|---|
| c7 PB | 103.0 | 52–74% | 10.7–14.6k | 6–30 / ≤296 |
| c7 PBS | 142.0 | 77–90% | 15.5–18.2k | ~30 / ≤451 |
| c7 PBP-H | 137.5 | 81–87% | 16–18.5k | 14–32 / ≤456 |
| single-c1 PB | 189.0 | 70–78% | 19.2–22k | ~100–130 / ≤446 |
| dual-c1 PB | 171.8 | **86–92%** | 19.4–22.4k | 55–100 / ≤445 |

At c7 the engine is ARRIVAL-limited (queue near-empty, 20+% headroom):
it drains everything the wire delivers. The engine's measured service
wall is ~20–22k msgs/s — reached only at c1-class aggregate (dual-c1),
where the queue still never builds because flow control ack-clocks the
sender to the drain rate. Σ-c7 (~157 ≈ 19–21k msgs/s at the measured
waste level) sits AT that wall's edge — but c7 never gets there, because:

### STEP 1d — the true c7 binder, named and measured: the wire is FULL of recovery-plane waste

Same-config same-session waste gauges (DIAG cumulative counters, both
seeds; share = counter / source symbols):

| cell (arm PBP-H) | retx share | repair share (`cod`) | wire arithmetic |
|---|---|---|---|
| sc2 single s42 (n=4) | 7.5–9.1% | 7.7–9.3% | 81 Mbit on a 100 Mbit path |
| sc2 single s7 (n=2) | 7.7% | 7.8–7.9% | — |
| c7 dual s42 (n=2 + 4 pinned) | **14.2–14.7%** (pinned 11.6–12.6) | **15.5–19.2%** | src 14.2k + cod 2.7k + retx ~2k ≈ 19k sym/s ≈ **190 Mbit emitted on the 2×100 wire — saturated** |
| c7 dual s7 (pinned n=2) | 12.1–12.6% | 16.0–16.7% | — |
| single-c1 (GE 0.1%) | **0.2%** | ~0 | 189 of 1000 Mbit — wire NOT binding |
| dual-c1 (GE 0.1%) | **9.1–9.3%** (×46 single) + 82–84k budget-suppressed gap reports | 12.8–13.1% | the dual sink is BELOW the single sink |

Reading: under dual-path striping the recovery plane roughly DOUBLES
its per-source retransmit share and ~2.2–2.5×es its repair share
against the SAME config run single-path. At c7 that extra ~16 pp of
source-relative emission ≈ 12–13% of the saturated wire ≈ exactly the
measured Σ-gap (session c7 = 137.5/136.4 vs Σ singles 160.8/160.2 =
0.85–0.86). The dual-c1 row is the controlled proof: at 0.1% loss there
is nothing real to recover, yet the dual arm retransmits 9.3% of source
(single: 0.2%) and STILL sinks less than one path alone (session means
~175 vs ~183) — a spurious cross-path recovery flood (SACK-gap
misreads + hole-refresh/tail-sweep re-fires under inter-path skew;
same family as residual (iii)'s spurious-retransmit class, #86) that no
receiver thread can fix. This is the FOURTH consecutive c7-class wall
that is control-plane, not compute (quinn CC → PMTU → pool law → this).

### STEP 1e — the sink ceiling attributed (which side binds, at what rate)

Single-c1 (the 187.7 datum, session 177–193): sender process ≈1.05
cores with the emission loop saturated at **~19.5–20k sym/s** (the
first app wall — wire 1 Gbit idle, kernel idle: system-wide 2.57/6
cores, softirq 0.10, ksoftirqd ~0, UDP errors 0); receiver engine 70–78%
busy at the same rate (headroom, q ≈ 5 ms). Pins: server→1 −7%,
client→1 −2–5%, both −7% — no CPU-count wall on either side, a
SERVICE-TIME wall on both (τ ≈ 45–50 µs/sym: store insert + placement +
serialize + `send_datagram` + estimator per symbol; on the receiver
deserialize + estimator + BTreeSet + frontier + ack gen + inject
hand-off). Dual-c1: both pipelines pay the multipath bookkeeping tax
(emission degrades to ~17.5–18.5k sym/s, engine busy 86–92%) AND the
spurious-retx flood eats 9% — aggregate 163–194 vs single 177–193.
**The "engine sink ceiling" is a per-process pipeline service-rate
pair (~19.5–20k sym/s send-side first, ~20–22k msgs/s recv-side just
above it), not a threading deficit** — which is why AES-NI (#84) and
core-count (this session) both failed to move it. Parallelizing
per-path stages could in principle raise the AGGREGATE sink at
c1-class cells — but no roadmap cell lives there: c7/c8 wire-classes
sit at or below the wall with the wire already full.

### STEP 2 — what was (not) built

**`RWM_ENGINE_PAR` was NOT built — the third refutation, per the task's
own gate.** Every menu item fails the profile: (a) per-path receiver
tasks — the engine drains c7 with 20% headroom and an empty queue;
(b) inject/delivery decoupling — inject is already a bounded-channel
hand-off (`tun.tx`), and delivery latency is not the binder; (c) ack
generation off the hot path — ack-path cost ~1.7% flat; (d) per-path
sender emission tasks — the emission loop saturates only at c1-class
cells, and at c7 the wire is already full (more emission capacity buys
more waste, not more goodput). A parallel engine would have measured
session drift (the #84/generation-inert lesson). Built and kept
instead: **`RWM_RDIAG`** (env-gated, default OFF, receiver-side
engine busy%/queue gauge — the instrument that made "arrival-limited
vs service-limited" measurable at all) + the harness forward for
`RWM_RDIAG` (no `RWM_ENGINE_PAR` forward: the knob does not exist).

### STEP 3 — measurement (what a ±ENGINE_PAR battery reduces to with nothing built)

Same-session interleaved mini-battery (100/200/400 MB single-run
invocations ×4 reps, seeds 42+7, arms round-robin; PIN = both processes
`taskset` to one core each mid-run):

| arm | s42 (n) | s7 (n) |
|---|---|---|
| sc2 single PBP-H | 81.0 80.8 79.4 80.5 → **80.4 (4)** | 80.7 79.5 → **80.1 (2)** |
| c7 PBP-H | 134.1 138.0 139.3 138.5 → **137.5 (4)** | 136.4 → **(1)** |
| c7 PBP-H PINNED 1+1 | 135.2 136.8 135.6 136.0 → **135.9 (4)** | 137.0 140.5 → **(2)** |
| single-c1 PB | 177.3 182.2 179.0 181.9 → **180.1 (4)** | 181.7 182.3 183.0 188.4 → **183.9 (4)** |
| dual-c1 PB | 173.2 190.8 172.6 168.3 → **176.2 (4)** | 163.3 173.0 169.4 193.6 → **174.8 (4)** |

plus the profile-phase c7 runs (133.3 134.1 137.8 131.3 138.7) and sink
probes (184.7 185.5 193.3 189.0 sc1; 178.4 171.8 dc1). Verdict frame:
c7 = 0.85–0.86 of same-session Σ (160.8/160.2) — consistent with the
honest-cap/borrowing sessions (0.89–0.90 of their Σ; session drift in
the singles, ratio stable); PINNED = UNPINNED at c7 both seeds (the ±
arm that matters: ± five cores, Δ ≈ −1.6/+2.0 ≪ σ_s ≈ 2.4–3.0); sink
ceiling single-c1 177–193 reproduces #84's 187.7 with dual-c1 BELOW it
both seeds (the anti-scaling datum). Seed-7 topo-ping double-aborts
took the documented toll (c7-s7 n=1 unpinned + 2 pinned; aborted
invocations left stale-log lines, recorded and discounted; no captured
result discarded). CPU recv·send per 200 MB @c7: unpinned 15.5–16.0 ·
18.1–18.6 s; pinned 9.9 · 11.1 s at equal throughput — CPU/bit falls
~38% when the scheduler stops migrating, the exact signature of a
NON-CPU-bound system.

### VERDICT + FLIP DECISION

- **Roadmap item 2 is CLOSED as a refutation with the binder named**:
  receiver/sender task parallelization is not the c7 (or c8) lever at
  the current operating point — 1+1 pinned cores sustain 132–144, the
  engine task idles 13–19% even at 142, and its inbound queue never
  builds. The measured thresholds where parallelization WOULD become
  the wall: ~19.5–20k sym/s sender emission / ~20–22k msgs/s receiver
  engine ≈ 185–200 Mbit per sink — c1-class territory, above every
  roadmap cell's wire class. #84's "~150–190" threshold estimate is
  sharpened to its upper edge and attributed to the SENDER first.
- **`RWM_ENGINE_PAR` default: not applicable — the knob does not exist**
  (nothing to flip; the task's flip gate "default ON only on clean
  sweep" is vacuously CLOSED-NO). `RWM_RDIAG` ships default OFF.
- **The c7 0.85–0.90×Σ residual now has a measured owner: multipath
  recovery-plane over-emission on a saturated wire** (retx share ×1.8,
  repair share ×2.2–2.5 vs same-config singles; dual-c1's loss-free
  9.3% retx flood is the controlled repro). SUCCESSOR lever (named,
  NOT built, per discipline): multipath-aware recovery suppression —
  cross-path in-flight awareness for the hole-refresh/tail-sweep
  engine (do not re-pull a seq whose retransmit/repair is younger than
  the other path's skew), the same family as the #86 flight witness
  and the Copa-compete contention-recovery successor. It gates the
  remaining ~12–15% of Σ at c7 AND the dual-c1 anti-scaling.
- No default changed; shipped tree byte-identical with env unset.

### Controls / discipline

Liveness: percap/guard/honest/sampler echoes verified on every PBP-H
run; PB/PBS runs carry none/path-scaled only. Same binary (40973e6b)
every run; lscpu in every log header; all comparisons same-session
interleaved; claimed effects are NULL effects with σ_s recorded, and the
waste shares are cumulative counters over 83k–335k symbols per run,
consistent across n=4+2 (sc2) and n=6+2 (c7) runs and both seeds. The
RDIAG probe's own overhead: RDIAG-on runs (137.5, 142.0) sit inside the
RDIAG-off spread (131.3–138.7) — instrument-neutral. VM lock held
19:20–20:20 UTC, released after teardown; netns cleaned; binaries and
logs preserved under /home/vibe/engpar/.

### Tests

`cargo test -p raptorpath --lib` 361/361 (probe tree, no new tests —
the probe is read-only); `-p raptorpath-math` 136 green across suites;
`gate_suite` 15/15 release; `mtu_blackhole_wedge` 2/2; `perf_loopback`
8/8; `copa_sole_loopback` / `fmtcp_loopback` / `daps_loopback` 1/1
release — the only code delta vs main is the
default-off RDIAG probe (no ordering/delivery surface: suites +
loopbacks green, no delivered-set change possible with the flag unset;
with it set the probe only reads).

## Multipath Recovery Suppression (2026-07-21) — the FIFTH control-plane wall's lever BUILT and MEASURED (the §16.23 successor): the over-emission root-caused to TWO instances of one mistake — recovery clocks/serials GLOBAL where multipath demands PER-PATH — the hole law rebuilt as RFC 9002 loss detection generalized per path (packet-threshold fast channel + time-threshold safety net, per-flight, retransmits inherit their own clock), the waste KILLED at both target cells (c7 retx 14.9→4.5% of source — BELOW single-path parity; dual-c1 retx 8.5→0.7%, the ×46 control answered), c7 +5.3/+6.4 Mbit (s42 Δ≫σ_s; s7 consistent) and the dual-c1 anti-scaling ELIMINATED (s42 192.3 vs single 186.0 with the bimodal collapse-mode damped; s7 193.2 vs 181.0, Δ=+24.2 ≫ σ_s) — but the wire the waste occupied does NOT convert 1:1 into goodput (c7 lands 0.89×Σ, not ≥0.95): the Σ-gap's residual owner moves from emission to frontier-recovery latency; the per-path SERIAL fix is DIAGNOSTICALLY VINDICATED (per-path loss estimates measured 0.62–0.77 at a 0.1%-loss cell) but REGRESSES at runtime (honest signal re-heats every SRTT/loss-scaled cadence; sender CPU ×2.4) and ships default-OFF as the named follow-up; flip `RWM_RECOV_MP` stays DEFAULT OFF (named: the c7 ~1.0×Σ target missed — the revised attribution — and c8 carries no Δ≫σ win; identity clean both cells) (branch `feat/recovery-suppression`, code commits 8a34520→2c632c0)

*Decision record: → [ADR-0059](adr/0059-per-path-recovery-clocks.md)*

Task: goal-gate "Engine Parallelization" VERDICT named the successor —
"multipath recovery-plane over-emission on a saturated wire" owns the c7
Σ-gap (retx ×1.8, repair ×2.2–2.5 vs same-config singles; dual-c1's
loss-free 9.3% retx flood the controlled repro). This session traced the
mechanism per-NACK, derived the law from the literature (no new
constants), built it env-gated, and ran the full A/B battery.

### The trace — the ×46 was TWO defects, named at the gauge

New DIAG instrumentation (`mpr[..]` recovery-plane trace: per-NACK cause,
fired-flight age vs law threshold, per-path fired/on, P_lost-branch count,
snapshot-coalesce count; per-path `pl=` loss-estimate gauge), plus the
existing `xattr` flight witness (RWM_RS_ATTR, §16.22 residual (iii)).
Probe runs (VM 10.1.5.16, binary 26f69029… = commit 8a34520, 2026-07-21
~00:00–00:30 UTC; 200 MB c7 / 400 MB dual-c1, seed 42):

1. **The hole law fires on scheduler-created gaps.** c7 baseline: 24,755
   targeted retransmits, of which **82% (20,357) with the seq's live
   flight YOUNGER than its own path's 9/8×smoothed-RTT clock** (mean age
   at fire 45 ms) — gaps created by striping + inter-path skew, read as
   holes by the legacy age gate (max-path-SRTT/2 since the ORIGINAL
   send; never reset by a retransmit, so an open gap re-fires every
   cooldown while copies still fly). The flight witness confirms
   delivery-side: 7,437 of 9,032 cross-path attributions (82%) credited
   the ORIGINAL flight (ack younger than the retransmit path's RTprop) —
   the direct spurious-retransmit fraction.
2. **The loss serials are poisoned by striping.** `batch_seq` is GLOBAL,
   but the receiver's per-path tracker estimates expected symbols from
   batch_seq GAPS — every path switch reads the other path's run as
   loss. MEASURED (`pl=` gauge): per-path loss estimate **0.62–0.77 at
   dual-c1 (true GE mean 0.1%)**, 0.27–0.30 at c7 (true ≈2.5%). This
   poisons everything keyed on loss: proactive `repair_debt` (the
   ×2.2–2.5 repair share), the P_lost retransmit branch, NACK budgets,
   and the per-batch phantom `release_in_flight`. (L0 unit repro: global
   serials under lossless round-robin striping read ~50% loss; per-path
   serials read 0% and still count real loss exactly.)

### The law as derived (env `RWM_RECOV_MP`, default OFF ⇒ shipped byte-identical; plain window reliable, N=1 keeps legacy gates)

RFC 9002 loss detection generalized per path (cited pattern, both
channels; no new constants — 9/8 = kTimeThreshold, 3 = kPacketThreshold,
granularity floor = the existing 10 ms per-seq cooldown floor):

- **§6.1.2 time threshold (safety net):** a reported gap seq is a
  candidate hole only once its LIVE flight (the last (re)send) is older
  than 9/8 × max of its OWN path's two smoothed RTT clocks (Copa EWMA
  srtt, estimator EWMA app-echo). The retransmit INHERITS the in-flight
  state (`nack_retx_at` now carries the retx path): the next decision
  clocks the NEW flight on ITS path — closes the re-NACK-while-flying
  feedback (hypothesis (c)).
- **§6.1.1 packet threshold (the fast honest channel — the first L1
  probe demanded it):** the ORIGINAL flight on path j is lost as soon as
  ≥3 later path-j symbols are known delivered (same-path FIFO evidence,
  from each gap report's implied delivered intervals + the sender's
  seq→path map). Scheduler-created cross-path gaps can NEVER trigger it
  (their same-path successors are exactly as un-arrived); real same-path
  losses fire in ~one skew instead of a full RTT. Retransmitted seqs
  stay time-threshold-only (their wire order is not their seq order).
  Without this channel the suppression traded waste for recovery
  LATENCY 1:1 on the frontier-serialized store and LOST (first-build
  probes: c7 139.1→134.5, dual-c1 181→142 — recorded, superseded).
- **Cross-path packet-threshold is deliberately NOT used** — cross-path
  seq gaps are the RFC 4737 reordering caveat; multipath QUIC solves
  the same problem with per-path packet-number spaces, which is exactly
  the SERIAL fix's shape.
- **Snapshot coalescing:** a gap report is a STATE SNAPSHOT (frontier +
  inverted SACK), not a delta — under the law holes legitimately
  outlive the 2 ms gap-ack cadence, so the sender processes only the
  NEWEST queued snapshot (law-gated; legacy per-report path bit-exact).
  Removes the law arms' walk tax (probe: 85k reports / 2.0M gap-seqs
  walked / 270k stale lookups per 400 MB before; CPU back to baseline
  after).
- Tail sweep verified single-fire per transfer cadence (diag_sweeps ≈
  elapsed/100 ms, unchanged across arms — hypothesis (b) REFUTED).

### The serial finding — vindicated as diagnosis, refuted as runtime fix (sub-gate `RWM_RECOV_MP_SERIAL`, default OFF)

Per-path batch serial namespaces (each path's batch stream sequential →
per-path gap = per-path loss, honestly) were built and sub-gate-measured
at L1: dual-c1 **181→134 serial-only** (c7 139→132), sender CPU ×2.4
(17.4→41.3 s per 400 MB) — the honest (small) RTT and honest (small)
loss re-heat every SRTT/loss-scaled recovery cadence the poisoned values
were accidentally damping: hole-refresh clamp lands at 25 ms instead of
100 ms, the per-seq retransmit cooldown at the 10 ms floor instead of
~53 ms, and the ADR-0046 congestion backoff no longer suppresses the
legacy flood (poisoned loss ≈ 0.6 collapsed the multiplier — an
accidental suppressor, which is also why baseline dual-c1 is BIMODAL:
runs where the multiplier collapsed early ran clean at 197–208, runs
where it didn't ran the flood at 172–176). The umbrella ships the LAW
only; the honest-signal cadence re-derivation (every SRTT/loss-scaled
constant re-audited under honest per-path estimates) is the NAMED
FOLLOW-UP, not built.

### Unit + L0 evidence

`cargo test -p raptorpath --lib` 366/366 (9 new): the skew-aware hole
law (gap on path A while in-flight on path B with B's clock not
expired → NOT a hole; expired → hole; N=1 → law inert = legacy;
unknown flight → never suppressed), 9/8-threshold + granularity floor,
retransmit flight inheritance (re-suppressed until the NEW flight's
clock expires), packet-threshold evidence inversion + decision (incl.
the cross-path-skew never-fires shape), the striping-phantom-loss
reproduction, and the §16.22 flight witness (pre-existing). New
`recov_mp_loopback` (dual-path, gate ON, real engine): completion with
dnf=0 — suppression-only gating never wedges (the receiver hole-refresh
re-advertises until a loss channel fires). L0 netem shim (same binary):
dual-c1 shape 139→172–174, c7 shape 71–76 → 97–99 with per-path loss
estimates 0.77→0.0000 — noting honestly that L0 favored the serial arm
(the shim hides quinn CC and the cadence heat; its documented fidelity
boundary), which is why the sub-gate attribution was re-done at L1 and
the L1 verdict governs.

### L1 battery (VM 10.1.5.16, 2026-07-21 00:49–~01:40 (incl. the seed-7 supplemental block) UTC; binary sha256 ba688b80… = commit 2c632c0, SAME binary every arm; E5-2650 v3 aes+avx2+pclmulqdq (post-divide) in every log header; 1 run/invocation × 8 reps, 11 arms interleaved round-robin per rep, fresh tunnel per invocation, seeds 42 AND 7, `RWM_DIAG=1` + liveness echo asserted per arm (`multipath recovery suppression ACTIVE`; mp=expect verified mechanically on all 167 COMPLETED runs); driver `tools/l1/recovmp_battery.sh`, logs `/home/vibe/recovmp/battery-s{42,7}.log` + per-run client/server logs under `/home/vibe/recovmp/diag/`)

Arms: PBP-H = plain + `RWM_QUIC_CC=bbr RWM_STORE_PERCAP=1
RWM_PLAIN_RS=1` (the best-c7 profile arm); PBS = plain+bbr+
`RWM_STORE_PATHS=1`; PB = plain+bbr; ±MP = `RWM_RECOV_MP=1` (= the law;
serials stay off).

Seed 42 (mean ± σ_s over n=8; per-run values in the log):

| arm | −MP | +MP | Δ | retx_med −/+ | cod_med −/+ |
|---|---|---|---|---|---|
| **dual-c1 PB (THE control)** | 189.1 ± 15.4 (172.5–208.2, bimodal) | **192.3 ± 6.9** (184.0–202.4) | +3.2 (σ halved) | 27,155 → **2,277** (8.5→0.7% of src; single-c1: 0.2%) | 0.115 → 0.011 |
| sc1 PB (same-session single) | 186.0 ± 4.2 | — | — | 712 (0.2%) | 0.002 |
| **c7 PBP-H** | 138.4 ± 1.7 | **143.7 ± 1.9** | **+5.3 (≫σ_s)** | 23,931 → **7,274** (14.9→4.5%) | 0.185 → **0.059** |
| sc2 PBP-H (Σ term + identity) | 80.9 ± 0.7 | 80.7 ± 0.8 (N=1 inert ✓) | −0.2 (≪σ) | 6,562 ≈ 6,446 (8.2%) | 0.081 ≈ 0.080 |
| c8 PBS | 62.4 ± 10.6 | 61.1 ± 13.6 | −1.3 (≪σ; both bimodal) | 1,697 → 2,140 | 0.111 → 0.123 |
| sc3 PB (Σ term + identity) | 15.79 ± 0.36 | 15.48 ± 0.41 | −0.31 (~0.8σ) | 2,444 → 2,940 | 0.124 → 0.147 |

Seed 7 (same protocol):

| arm | −MP | +MP | Δ | retx_med −/+ | cod_med −/+ |
|---|---|---|---|---|---|
| **dual-c1 PB (THE control)** | 169.0 ± 6.7 (n=8 — ALL-flood mode this seed, BELOW single) | **193.2 ± 14.6** (n=8; best run 222.5) | **+24.2 (≫σ_s)** | 30,525 → **918** (9.5→0.29%; single: 0.22%) | 0.128 → 0.005 |
| sc1 PB (same-session single) | 181.0 ± 4.5 (n=8) | — | — | 703 (0.22%) | 0.002 |
| **c7 PBP-H** | 135.2 ± 3.4 (n=6) | **141.6 ± 6.5** (n=6) | **+6.4** | 23,934 → **7,565** (15.0→4.7%) | 0.184 → 0.105 |
| sc2 PBP-H (Σ term + identity) | 80.2 ± 1.2 (n=8) | 79.8 ± 0.7 (n=5) | −0.4 (≪σ) | 6,214 ≈ 6,214 | 0.079 ≈ 0.077 |
| c8 PBS | 62.4 ± 14.5 (n=7) | **70.6 ± 8.7** (n=10) | +8.2 (~0.6σ_base; σ near-halved) | 2,797 → 1,781 | 0.153 → 0.106 |
| sc3 PB (Σ term + identity) | 15.66 ± 0.57 (n=8) | 15.78 ± 0.35 (n=5) | +0.12 (sign FLIPS vs s42 → noise) | 2,925 → 2,564 | 0.111 → 0.125 |

(Seed-7 n<8 arms are the documented topo-ping double-abort class
(discipline item 8): 19 aborted invocations in the main battery + 9 in
the same-session supplemental block (reps 9–12, same interleaving,
appended to the same log); every abort verified SUMMARY-LESS — the
stale-echo liveness lines are recorded and discounted, no captured
result was discarded, and no completed run has a mismatched liveness
echo (checked mechanically for all 167 completed runs).)

dnf=0 on ALL 167 completed runs, both seeds; delivered-set clean (perf
server acks only on complete reassembly). Witness corroboration at c7:
baseline 82–86% of cross-path attributions spurious (xattr
8.1–10.5k/6.5–8.9k per run) → +MP 34–44% (3.2–4.5k/1.1–2.0k) on ~3×
fewer retransmits.

### VERDICT vs Σ + the honest attribution hand-off

- **The dual-c1 control is ANSWERED:** the loss-free retransmit flood
  falls 8.5% → 0.7% of source (×12; vs the single's 0.2% ≈ ×3.2
  residual, from real 0.1% GE loss now recovered per-flight), repair
  0.115 → 0.011, and the dual aggregate moves from BELOW-single-mean
  bimodal to ABOVE single (s42 192.3 vs 186.0 with σ halved; s7 193.2 vs 181.0, Δ=+24.2 ≫ σ_s against an ALL-flood baseline) with σ
  halved — the anti-scaling is ELIMINATED. The task's "~1.9×" restated
  honestly: dual-c1 CANNOT 1.9× — the §16.23 sender emission service
  wall (~19.5–20k sym/s ≈ 190 Mbit) binds both arms; ≥1.0× single with
  the waste gone is the physical ceiling's answer.
- **c7 gains Δ=+5.3/+6.4 (s42 ≫σ_s; s7 consistent) to 0.88–0.89×Σ
  (s42 143.7/161.8; s7 141.6/160.4) — the ≥0.95×Σ target is NOT met**, and the
  reason is measured, not guessed: the +MP arm's waste is BELOW
  single-path parity (retx 4.5% vs single 8.2%, repair 0.059 vs 0.081),
  the emitted wire is ~155 of 200 Mbit — the wire is NO LONGER FULL,
  yet goodput stops at 144. The §16.23 attribution ("over-emission
  occupies ≈ the Σ-gap") is therefore REVISED: the emission was
  co-located with, but only partially causal for, the gap. The
  remaining ~11% of Σ has a new named owner: frontier-recovery latency
  on the ack-serialized retention store (every real hole still freezes
  the cumulative frontier for ≥ one path-skew + report round; the
  successor-candidate lever is SACK-clocked store release — the
  `RWM_SACK_PRUNE` machinery — composed with the suppression).
- **c8 is null-to-positive:** s42 −1.3 (≪σ 10.6–13.6, null), s7 +8.2
  with σ near-halved (14.5→8.7) — the direction is right on the noisier
  seed but no Δ≫σ claim survives the cell's bimodality; NO regression
  either seed. c8's retx waste was already modest (7–14% at a 20 Mbit
  slow path) — its binder remains the pool/no-borrowing story (§16.22),
  untouched here.
- **N=1 identity holds at both single cells:** sc2 Δ = −0.2/−0.4 ≪ σ;
  sc3 Δ = −0.31 (s42) / **+0.12 (s7 — the sign FLIPS)** → noise, not a
  cost (the s42-only sign was watched for and did not repeat). The law
  is provably inert at N=1 (unit-tested); the only live N=1 code under
  the flag is snapshot coalescing.
- **FLIP: `RWM_RECOV_MP` stays DEFAULT OFF, named.** The control cell
  sweeps clean on both seeds (waste ×12–33 down; dual ABOVE single;
  s7 Δ≫σ) and c7 improves on both seeds with identity clean — but the
  task's own c7 target (throughput → ~1.0×Σ, ≥0.95) is MISSED at
  0.88–0.89×Σ because the emission was only partially causal for the
  Σ-gap (the revised attribution above), and c8 carries no Δ≫σ win.
  Per discipline the default flip gates on the successor battery that
  composes suppression with the frontier-release lever (where the
  remaining Σ-gap now provably lives). All recovery-suppression knobs
  ship byte-identical with env unset.

### Controls / caveats / discipline items

Liveness echo asserted per run (mp=expect on 167/167 completed;
percap/pbs echoes verified per arm; seed-7 aborted invocations left the
documented stale-echo lines — recorded, discounted, all summary-less); same binary ba688b80… every run; lscpu in every log
header (post-divide); arms interleaved round-robin per rep; both seeds
with per-run values recorded; claimed c7 effect (+5.3/+6.4)
exceeds σ_s at s42 (1.7–1.9) and is consistent at s7 (σ_s 3.4–6.5); the dual-c1 claim is stated as waste-kill +
σ-halving + mean-above-single, NOT as a Δ≫σ throughput win (baseline
bimodality σ=15.4 forbids that claim at n=8). c8 session baseline
(62.4/62.4) sits below the ledger's 72–76 PBS record — session
drift, why every comparison here is same-session interleaved. The
first-build probe numbers (139→134, 181→142) are RECORDED and
superseded by the fast-channel build — they are the measured proof that
suppression without a fast loss channel trades waste for recovery
latency and loses; the serial sub-gate probes are the measured proof
that honest signals cannot simply be switched on under legacy cadences.
Probes are n=1 (channel attribution, not effect claims). CRLF stripped
after every tree sync (discipline 10). VM lock `/tmp/rwm-vm.lock` held
2026-07-20 23:45 → release after teardown; netns clean (`rp-*` only);
binaries + logs preserved under `/home/vibe/recovmp/`.

### Tests

`cargo test -p raptorpath --lib` 366/366 (9 new — law, threshold,
inheritance, packet-threshold evidence/decision, phantom-loss repro);
`-p raptorpath-math` all suites green (59/19/22/4/4/3/25); release
`gate_suite` 15/15, `mtu_blackhole_wedge` 2/2, `perf_loopback` 8/8,
`copa_sole_loopback`/`fmtcp_loopback`/`daps_loopback` 1/1, NEW
`recov_mp_loopback` 1/1 (dual-path, gate ON, dnf=0). Shipped tree
byte-identical with env unset (every change env- or DIAG-gated; the
`nack_retx_at` value widening and the mp_batch_seq fallback are
behavior-neutral, covered by the identity cells).

## Default CC Flip (2026-07-21) — `RWM_QUIC_CC` unset ⇒ BBR (branch `feat/bbr-default-and-store-release`)

*Decision record: → [ADR-0054](adr/0054-substrate-cc-policy-bbr-default.md)*

The consolidation roadmap's Item 0 (approved 2026-07-21): the shipped
default was the worst measured configuration — stock Cubic was wall #1
(plain 17.5 → plain+BBR 74.5 pooled, ×4.3, "Gen Substrate Ceiling"), every
measured best arm since has set `RWM_QUIC_CC=bbr` explicitly, and the L1
batteries were therefore never Cubic-polluted; what this flip fixes is the
SHIPPED binary and the local suites (loopbacks/unit paths) that were still
exercising the condemned Cubic path.

**Change** (`raptorpath/src/transport/quic.rs`, `quic_cc_mode` /
`quic_cc_factory`): env unset ⇒ quinn BBR. `cubic` stays selectable — **the
A/B inverts: the legacy wire is now the explicit `RWM_QUIC_CC=cubic`
opt-out arm.** Unrecognized values warn and keep the BBR default (they
previously fell back to Cubic). `newreno`/`passthrough` unchanged.
Byte-identity language for this file is hereby UPDATED: the shipped default
is intentionally no longer the legacy wire; identity claims for other
gates are unaffected (they compare against the same substrate on both
sides).

**Fairness caveat, documented at the flip site** (measured 2026-07-19,
"Copa Competitive Mode + Cross-Traffic"): BBR vs one Cubic flow takes a
0.95–0.96 share at the lossy c2 cell (Cubic is Mathis-bound there; BBR is
the aggressor) and 0.24 share on the CLEAN shared bottleneck (the Cubic
competitor fills a 305–316 ms queue and BBRv1 yields) — mildly aggressive
under loss, yielding under bufferbloat, both within the deployed-BBRv1
envelope. The endstate CC policy is the hint's declared price choosing the
controller (bulk → bbr-under; latency-priced → passthrough+Copa, the
δ-capable controller) — policy, not a mode switch (paper §17.2).

**Local suites on the new default** (this tree, 2026-07-21): `cargo test
-p raptorpath --lib` 366/366; `-p raptorpath-math` all suites green
(59/19/22/4/4/3/25); release `gate_suite` 15/15 (L0/sim, no quinn —
expectations unchanged by construction), `mtu_blackhole_wedge` 2/2,
`perf_loopback` 8/8, `copa_sole_loopback`/`fmtcp_loopback`/
`daps_loopback`/`recov_mp_loopback` 1/1 — the loopbacks now exercise BBR;
no loopback expectation was Cubic-shaped (all green unmodified).

**L1 identity check PASSED** (it's an identity, not a discovery: the
default binary must reproduce the measured `RWM_QUIC_CC=bbr` arms). VM
10.1.5.16, 2026-07-21 08:38–08:41 UTC, binary sha256 024005e3f267d2b7… =
commit 7145fcc (Rust identical to the flip commit 519467e), E5-2650 v3
post-divide, seed 42, ×4 reps per cell, env UNSET, `quinn congestion
controller: BBR (shipped default…)` liveness echo asserted on 8/8 runs,
dnf=0 on 8/8; log `/home/vibe/ccflip/identity-s42.log`:

- sc2 (c2 single plain, 100 MB): **81.4 / 81.1 / 78.7 / 80.8** (mean 80.5)
  vs expected ≈76–78 — at/above the top of the measured bbr-arm envelope
  (74.5–79 PB; 80.9 ± 0.7 same-session PBP-H in §16.24), unambiguously
  the BBR arm (the Cubic default measured ~16–17 at this cell).
- sc3 (c3 single plain, 25 MB): **15.96 / 15.16 / 15.49 / 15.23** (mean
  15.46) vs expected ≈15.7 — inside the measured 15.6–15.9 /
  15.66 ± 0.57 envelope.

The default binary reproduces the measured bbr arms; Item 1 may proceed.

## SACK-Clocked Store Release (2026-07-21) — PRE-REGISTRATION (written BEFORE the build, discipline item 11; env `RWM_STORE_SACK_RELEASE`, default OFF; branch `feat/bbr-default-and-store-release`)

*Decision record: → [ADR-0060](adr/0060-sack-clocked-store-release.md)*

**(a) Mechanism.** The retention store releases slots only on the
cumulative frontier (`sent_store = sent_store.split_off(&(ack+1))` — "the
whole retention contract"). SACKed-but-not-cumulative symbols therefore
hold slots a full frontier round: at c7 the store recycles at FRONTIER
latency, not path rate. §16.24 measured the profile this predicts: with
`RWM_RECOV_MP` the waste is below single-path parity, the emitted wire is
~155 of 200 Mbit (no longer full), yet goodput stops at ~144 — the Σ-gap's
residual owner named there is frontier-recovery latency on the
ack-serialized retention store. The lever: on a SACK range, release the
STORE SLOT (uncount the symbol from the outstanding/flow-control gate so
the window opens at path rate) while RETAINING the payload and every
recovery structure until the cumulative frontier passes it.

**(b) Prediction (effect size + cells).** SACK-clocked slot release lifts
c7 from 0.88–0.89×Σ toward ~0.95×Σ composed with `RWM_RECOV_MP`; dual-c1
and c8 (PBS arm) unregressed; sc2/sc3 identity cells inert-or-better (the
law is NOT expected to be bit-exact at N=1 — single-path SACKs above a
hole also hold slots — but any N=1 effect must be ≥0, within σ).
Store-dwell/occupancy gauges (sout/DIAG) must show the mechanism: released
slots re-open admission while holes are outstanding (store no longer
pegged at cap across a frontier stall).

**(c) Falsification.** c7 ≤0.90×Σ with the dwell gauges showing release no
longer binds (store not at cap, window open, goodput still stopped) ⇒ the
Σ-gap owner is elsewhere; report, don't force. Per discipline item 11 a
failed prediction goes to the deprecation register unless the failure
names a new mechanism.

**(d) Derivation re-read for self-contained failure predictions (the
borrowing lesson).** Two named bounds, neither disqualifying: (1) the
sender emission service wall (~19.5–20k sym/s ≈ 190 Mbit, §16.23) sits
ABOVE the c7 target (0.95×Σ ≈ 154 Mbit) — not binding; (2) the SACK Flow
Control section (2026-07-07) measured that sender-side decoupling alone
does NOT lift the single-path c2 cell (16.09 vs 16.07 — receiver-side
recovery latency owned that gap at that era's operating point). The
distinction that keeps this build eligible: that experiment predates walls
1–8 (Cubic substrate, MTU wedge, pool law, recovery suppression) and its
cell was single-path; the c7 profile TODAY is store-starved at the
SENDER (wire un-full, waste suppressed, goodput stopped — §16.24), which
is precisely the configuration in which slot release can bind. If c7
nonetheless reproduces the 2026-07-07 null, falsification (c) applies and
the 2007-07 receiver-side attribution extends to the multipath cell.

**CONSTRAINT — the `RWM_SACK_PRUNE` lesson (2026-07-07, refuted UNSAFE for
in-order): the law differs BY CONSTRUCTION.** SACK_PRUNE **removed** the
SACKed symbol from `sent_store` + `retransmit_buffer` + `nack_retx_at` +
`source_path_map` — destroying the only retained copy, so a
received-then-EVICTED symbol at the receiver's bounded reassembly window
could never be retransmitted → C7/C8 in-order DNF (wedge). The new law
releases a STORE SLOT, never recoverability: in today's code the store
slot and the retransmit copy are the SAME allocation (`sent_store` holds
the payload; `retransmit_buffer` holds only per-seq retransmit metadata
(send_time, ε, path); the NACK retransmit path serves payload from
`sent_store.get(&seq)`), so the release KEEPS the `sent_store` entry and
every ARQ map untouched and only UNCOUNTS the seq from the flow-control
gate (a released-seq set subtracted from the outstanding count; pruned by
the same cumulative `split_off` twin). Every un-cumulatively-acked symbol
remains retransmittable until the frontier passes it. Worst case under
receiver eviction is a wasted retransmit, not a wedge; the sender's
race-ahead is bounded because evicted/never-received symbols are never
SACKed and so still count against the cap. Unit invariant (pre-registered
test): SACKed → released → retransmit-still-possible → cumulative-ack →
fully freed; window opens on SACK; no double-release; released slots
return to the `RWM_STORE_PATHS` pool; released symbols keep their
`RWM_RECOV_MP` per-flight loss clocks.

**Battery (pre-registered).** VM protocol per MEASUREMENT DISCIPLINE
(items 1–10): seeds 42+7 ×8 interleaved, same-session Σ singles, liveness
echoes, env+sha256 recorded, dwell gauges before/after. Arms = best-c7
config (PBS-class: plain + BBR-default (env unset, post-flip) +
`RWM_STORE_PATHS=1`) × {±`RWM_STORE_SACK_RELEASE`} × {±`RWM_RECOV_MP`}
(4 arms); cells c7 + c8 + dual-c1 + sc2/sc3 identity. FLIP default ON only
if the prediction (b) holds on both seeds with no regressions; else
default OFF with the falsification outcome recorded.

*(Results section to follow the build — nothing below this line in this
section was written before the battery ran.)*

### The law as built (commit ff7acb4; the code shape)

`sack_release_mark` / `sack_release_prune` / `sack_release_outstanding`
(net/mod.rs): on a drained SACK range the sender marks every retained seq
into a released set (idempotent — a re-advertised snapshot releases
nothing twice); `store_len = sent_store.len() − released.len()` at the
single site every flow-control gate reads (so `RWM_STORE_PATHS`' pooled
cap composes with no extra code); the released set prunes on the same
cumulative `split_off` twin as the store (subset invariant). Per-path
accounts (`RWM_STORE_PERCAP`) free on the newly-released list; borrowing
loans repay. NOTHING else is touched: `sent_store` (the only payload
copy), `retransmit_buffer` (metadata), `nack_retx_at` (+ its
`RWM_RECOV_MP` per-flight clocks), `source_path_map` all survive until
the cumulative frontier — the SACK_PRUNE separation verified in code:
the NACK retransmit path serves from `sent_store.get(&seq)`, which the
release law never removes. If both `RWM_SACK_PRUNE` and the release gate
are set, the legacy prune experiment takes precedence (warned).
Unit tests (5 new, lib 371/371): the every-unacked-symbol-recoverable
chain (SACKed → released → retransmit-still-possible → cumulative-ack →
fully freed), window-opens/pool-return, no-double-release + percap
composition, ARQ-state/flight-clock preservation, subset invariant under
frontier races. Local wedge check: `perf_loopback` 8/8 +
`recov_mp_loopback` 1/1 green with the gate forced ON.

### L1 battery (VM 10.1.5.16, 2026-07-21 09:06–09:51 UTC; binary sha256 e79d0be2a83d9dad… = commit ff7acb4, SAME binary every arm; E5-2650 v3 aes+avx2+pclmulqdq (post-divide) in every log header; 1 run/invocation × 8 reps, 15 arms interleaved round-robin per rep, fresh tunnel per invocation, seeds 42 AND 7, RWM_DIAG=1 + per-arm sr/mp liveness echoes asserted mechanically (0 completed-run mismatches, both seeds); driver `tools/l1/sackrel_battery.sh`, logs `/home/vibe/sackrel/battery-s{42,7}.log` + per-run client/server logs under `/home/vibe/sackrel/diag/`)

Arms: PBS = plain + BBR-default (env unset — the post-flip substrate) +
`RWM_STORE_PATHS=1` (the best-c7 profile); PB = plain + BBR-default;
SR = `RWM_STORE_SACK_RELEASE=1`; MP = `RWM_RECOV_MP=1`. dnf=0 on ALL 200
completed runs, both seeds.

Seed 42 (mean ± σ_s over n; per-run values in the log; med_retx = median
targeted retransmits; med_occ = mean counted store occupancy, median over
runs; srel = median cumulative slots released):

| arm | mean ± σ_s | n | vs Σ | med_retx | med_occ (cap) | srel |
|---|---|---|---|---|---|---|
| sc2 PBS (Σ term) | 80.70 ± 0.97 | 8 | — | 3,687 | 867 (4096) | — |
| sc2 PBS+SR | **85.01 ± 0.38** | 8 | **+4.31 ≫σ** | 3,347 | 983 | 68.6k |
| sc3 PBS (Σ term) | 15.50 ± 0.50 | 8 | — | 3,199 | 976 | — |
| sc3 PBS+SR | **16.16 ± 0.22** | 8 | +0.66 (~1.3σ) | 2,339 | 972 | 20.3k |
| c7 PBS | 142.87 ± 2.20 | 8 | 0.885×Σ | 21,639 | 3,157 | — |
| c7 PBS+SR | **154.75 ± 1.59** | 8 | **0.959×Σ** (+11.9 ≫σ) | 17,229 | **1,466** | 166.9k |
| c7 PBS+MP | 155.82 ± 4.02 | 8 | 0.965×Σ | 7,294 | 3,042 | — |
| c7 PBS+SR+MP | **168.74 ± 0.85** | 8 | **1.045×Σ** (0.993×Σ_SR) | 5,208 | 1,369 | 165.8k |
| c8 PBS | 70.07 ± 6.82 | 8 | — | 1,724 | 2,492 | — |
| c8 PBS+SR | 63.01 ± 19.38 (bimodal: 25.0, 43.6 runs) | 8 | −7.1 ≪σ | 1,884 | 1,944 | 20.9k |
| c8 PBS+MP | 66.75 ± 7.38 | 8 | — | 1,401 | 2,703 | — |
| c8 PBS+SR+MP | 74.16 ± 15.18 | 8 | +4.1 ≪σ | 1,679 | 2,344 | 21.0k |
| sc1 PB (single ref) | 190.59 ± 12.35 | 8 | — | 618 | 249 | — |
| dc1 PB | 181.87 ± 5.34 | 8 | — | 27,473 | 208 | — |
| dc1 PB+SR+MP | **204.19 ± 8.80** | 8 | **+22.3 ≫σ** (above single) | **2,437** | 203 | 23.0k |

Seed 7 (same protocol; n<8 arms = the documented topo-ping double-abort
class, discipline item 8 — 40 aborted invocations, every one verified
SUMMARY-LESS, no captured result discarded, 0 completed-run liveness
mismatches):

| arm | mean ± σ_s | n | vs Σ | med_retx | med_occ | srel |
|---|---|---|---|---|---|---|
| sc2 PBS (Σ term) | 81.49 ± 0.45 | 4 | — | 3,117 | 547 | — |
| sc2 PBS+SR | **84.42 ± 0.98** | 4 | **+2.93 ≫σ** | 3,523 | 919 | 69.5k |
| sc3 PBS (Σ term) | 15.75 ± 0.52 | 3 | — | 3,321 | 976 | — |
| sc3 PBS+SR | 16.06 ± 0.21 | 6 | +0.31 (<1σ) | 2,448 | 966 | 19.9k |
| c7 PBS | 141.78 ± 1.98 | 4 | 0.870×Σ | 11,880 | 1,986 | — |
| c7 PBS+SR | **152.28 ± 1.39** | 6 | **0.934×Σ** (+10.5 ≫σ) | 20,114 | **1,460** | 167.0k |
| c7 PBS+MP | 158.78 ± 2.27 | 5 | 0.974×Σ | 6,767 | 2,930 | — |
| c7 PBS+SR+MP | **165.92 ± 1.99** | 3 | **1.018×Σ** (0.983×Σ_SR) | 6,323 | 2,218 | 166.8k |
| c8 PBS | 68.26 ± 10.44 | 5 | — | 3,031 | 2,609 | — |
| c8 PBS+SR | 70.76 ± 9.67 | 7 | +2.5 ≪σ (sign FLIPS vs s42 → noise) | 2,005 | 1,952 | 21.1k |
| c8 PBS+MP | 68.49 ± 10.19 | 3 | — | 2,005 | 2,372 | — |
| c8 PBS+SR+MP | **74.48 ± 6.15** | 6 | +6.2 (σ tightest of the four) | 1,470 | 2,035 | 21.1k |
| sc1 PB (single ref) | 185.97 ± 3.40 | 8 | — | 651 | 240 | — |
| dc1 PB | 187.76 ± 22.79 | 8 | — | 29,320 | 204 | — |
| dc1 PB+SR+MP | **208.16 ± 15.47** | 8 | **+20.4** (all 8 runs ≥196.3, above single) | **2,398** | 197 | 23.8k |

(Σ = 2 × same-session sc2-PBS base term. Σ_SR = 2 × sc2-PBS+SR — the
composed arm's own honest Σ, since SR lifts the single too.)

### Dwell gauges — the mechanism evidence (before/after)

The pre-registered mechanism signature is measured directly: at c7 the
baseline counted store occupancy sits at ~3,157 mean (s42; cap 4,096 —
the store recycling at frontier latency), and under SR it falls to
~1,460–1,466 (both seeds) with ~167k slots released per 200 MB (≈ every
symbol's slot returned on SACK evidence one frontier round early);
admission stays open across frontier stalls and goodput follows
(+11.9/+10.5 SR-only). The release does NOT inflate recovery waste —
retx falls (s42: 21.6k → 17.2k) and cod share falls (0.162 → 0.128) with
SR alone; composed with MP retx lands at 5.2k/6.3k. The dual-c1 control
composes cleanly: retx 27.5k/29.3k → 2.4k (×11–12), repair share
0.115/0.122 → 0.011, dual ABOVE single on both seeds. The 2026-07-07
"sender was never the bottleneck" null is era-resolved: on the post-wall
substrate (BBR + path pool + suppression) the sender store IS the
binder, and releasing it converts ~1:1 into goodput at the symmetric
dual cell.

### VERDICT vs the pre-registration — the prediction HOLDS (exceeded)

- **(b) predicted c7 → ~0.95×Σ composed with `RWM_RECOV_MP`: MEASURED
  1.045×Σ / 1.018×Σ (0.993/0.983 of the SR-arm's own Σ), both seeds,
  σ_s ≤ 2.0 on the composed arm.** SR alone lands 0.959/0.934×Σ
  (+11.9/+10.5 ≫ σ). The falsification condition (c) is NOT triggered
  (release binds at the gauges and goodput follows).
- dual-c1 and c8 unregressed as predicted: dc1 +22.3/+20.4 (retx ×11–12
  down, above single both seeds); c8 composed +4.1/+6.2 (≪σ at s42,
  σ-tightest arm at s7) — no Δ≫σ claim in either direction at c8 (its
  binder remains the §16.22 pool/no-borrowing story), and the s42
  SR-only bimodal low runs (25.0/43.6) do not repeat at s7 (+2.5, sign
  flip → noise class; WATCHED, not claimed).
- sc2/sc3 pre-registered "inert-or-better": BETTER — sc2 +4.31/+2.93
  ≫ σ both seeds (single-path SACKs above a hole also hold slots — the
  law's N=1 term is real), sc3 +0.66/+0.31 positive both seeds. The
  historic sc2 record (80.9 PBP-H) is exceeded at 84.4–85.0.
- **FLIP: `RWM_STORE_SACK_RELEASE` DEFAULT ON (2026-07-21)** — the
  pre-registered gate (c7 ≥~0.95×Σ or ≫σ toward it, both seeds, no
  regressions anywhere) is met: s42 0.959×Σ SR-only / 1.045×Σ composed;
  s7 0.934×Σ SR-only (≫σ toward the target) / 1.018×Σ composed; every
  other cell inert-or-better within its documented noise. `=0` is the
  legacy frontier-only-release opt-out arm; `RWM_SACK_PRUNE=1` (the
  refuted experiment) takes precedence over the release law when
  explicitly set, with a warning. The shipped default is intentionally
  no longer byte-identical to the legacy wire (as with the Default CC
  Flip above; identity claims for other gates compare on the same
  substrate both sides).
- Suites on the flipped default: lib 371/371, raptorpath-math all
  green, release gate_suite 15/15, mtu_blackhole_wedge 2/2,
  perf_loopback 8/8, copa_sole/fmtcp/daps/recov_mp loopbacks 1/1.
- **Default-env L1 smoke of the SHIPPED binary** (env fully unset —
  BBR default + SR default + legacy 1024 pool; binary sha256
  6cc5c85816333906… = commit a52105d, 2026-07-21 10:15 UTC, seed 42,
  log `/home/vibe/sackrel/default-smoke.log`): c7 **148.2/148.8**
  Mbit/s (0.91×Σ on the LEGACY pool — vs 0.885×Σ for the path-scaled
  baseline and ~20 for the pre-flip Cubic-era default), sc2
  **83.8/85.2** (= the battery's SR arm 84.4–85.0), dnf=0, `SACK-
  clocked store release ACTIVE` + BBR echoes present on every run,
  srel gauge live (163–165k slots released per 200 MB). The shipped
  default now carries both flips end to end.
- Ops: VM lock `/tmp/rwm-vm.lock` held 08:19 UTC → released after
  teardown; CRLF converted after every sync (discipline 10); rp-* netns
  only; binaries + logs preserved under `/home/vibe/sackrel/` (+
  `/home/vibe/ccflip/` for the Item-0 identity).

## Consolidation (2026-07-21) — the composed default stack: PRE-REGISTRATION (roadmap item 2; discipline item 11 — this block written before the battery's results were parsed; the flip rule was fixed in the approved roadmap before launch; branch `feat/consolidation`)

*Decision record: → [ADR-0067](adr/0067-consolidated-default-stack.md)*

**(a) Mechanism.** Not a new build — COMPOSITION. Four measured winners sit
default-OFF because each was gated on a per-knob clean sweep while the
features interact (the pile-up root cause named in the roadmap). The
candidate default stack, tested as ONE unit on top of the current shipped
defaults (BBR-under + `RWM_STORE_SACK_RELEASE`, both default ON since
2026-07-21): `RWM_STORE_PATHS=1` (wall #7's fix) + `RWM_RECOV_MP=1`
(wall #8's fix) + `RWM_MSTAR_ANCHOR=1` + `RWM_CLOCK_GAP=1` (the
estimator-hygiene pair). `RWM_PLAIN_RS` joins ONLY if a c8 composition
probe shows its known −3–5 Mbit witness cost resolved in composition.

**(b) Predictions (effect size + cells).** (1) STACK ≥ every current best:
c7 ≈1.0×Σ-class (the §16.25 composed 1.018–1.045×Σ carries), c8 ≥ PBS-class
0.74–0.80×Σ, dc1 above single, sc2/sc3 at-or-above the SR-arm singles.
(2) LOO-STORE_PATHS and LOO-RECOV_MP each fall ≫σ below STACK at c7 (their
individual deltas were +40–50 and +5–13 Mbit) — both qualify easily.
(3) The anchor pair must PROVE marginal value in composition: M*'s knee
evidence is generation-gated, so its plain-cell LOO rows may be
statistically identical — in that case its flip decision MOVES to the
generation-default question and is recorded, not forced. (4) The realtime
tail crown (shipped streaming p99 class at c2) survives the stack env
unchanged — STORE_PATHS/RECOV_MP are reliable-window-gated (inert at that
cell by construction), so only the anchor pair could move it. (5)
Cross-traffic c2 share vs 1 Cubic flow ≈ the documented BBR-under class
(share documented as a caveat, NOT a gate).

**(c) Falsification / flip rule (pre-registered).** A member joins the
default iff its LOO row shows removal HURTS (or is neutral while the member
wins elsewhere) with no cell regressed ≫σ on both seeds — the roadmap's
strictly-better criterion. A member whose LOO row shows removal HELPS at
any cell ≫σ on both seeds stays OFF and is recorded. The tail crown
regressing ≫ its documented rep spread under the stack is a STOP-and-report
(the stack does NOT ship). c8's known bimodality means no c8-only ≪σ delta
can gate any member in either direction.

**(d) Derivation re-read for self-contained failure predictions.** Known
composition risks, none disqualifying: (1) SR already banks part of the
frontier-latency win MP used to convert (§16.25 measured them composing:
1.018–1.045×Σ); (2) M*'s plain-live subset (peer-report RTT-feed
suppression + seed-from-sample) touches the SAME estimator the SR/MP laws
read clocks from — a small c7 interaction in either direction is possible
and is exactly what the LOO row exists to measure; (3) CLOCK_GAP quarantine
discards estimator samples after process stalls — at a VM cell with real
scheduler stalls this can only defer estimator updates, bounded by the
quarantine cap; (4) the PLAIN_RS witness cost is the pre-measured −3–5 Mbit
(§16.21) — the probe asks whether the honest-cap law composes it away at
c8; the derivation does not predict it does (the witness samples on the
send path regardless), so the prior is AGAINST inclusion.

**Battery (pre-registered).** VM protocol per MEASUREMENT DISCIPLINE 1–10:
seeds 42+7 ×8 interleaved round-robin per rep, fresh tunnel per invocation,
same binary every arm, liveness echoes asserted per arm (SR default-ON echo
on every arm; PBS/MP/MS/GAP echoes matched to each arm's expectation, both
directions), env + sha256 + lscpu recorded, same-session Σ singles per arm
config, seed-7 topo-abort protocol. Arms: ship (env unset), stack, LOO ×4,
stack+RS (c8 only). Cells: c7/c8/dc1 (all arms) + sc1 (ship, stack) +
sc2/sc3 (ship, stack, LOO-MP/MS/GAP; LOO-PBS ≡ stack at N=1 — STORE_PATHS
is N≥2-gated) + tail_matrix c2 `stream` vs `stack` ×5/seed + cross-traffic
c2 `bbr`±stack env ×5/seed. Priority under VM-time pressure (pre-declared):
LOO c7/c8/dc1 first, then singles, then tail, then cross-traffic (cut
cross-traffic first, say so). Driver `tools/l1/consol_{battery,all}.sh`.

*(Results below this line were written after the battery ran.)*

### L1 battery RESULTS (VM 10.1.5.16, 2026-07-21 10:35–14:40 UTC; binary sha256 773b188a69194166… = commit 5daceab, SAME binary every arm; E5-2650 v3 aes+avx2+pclmulqdq (post-divide) in every log header; 1 run/invocation, 31 arms interleaved round-robin per rep ×8 reps, fresh tunnel per invocation, seeds 42 AND 7, RWM_GEN=0 RWM_DIAG=1 everywhere; per-arm 5-echo liveness assertion (SR/PBS/MP/MS/GAP, both directions): s42 **0 mismatches over 248/248 completed invocations**, s7 **0 completed-run mismatches over 173 completed** (75 seed-7 topo-ping aborts, every one verified SUMMARY-LESS with only the documented stale-echo class, discipline 8 — n recorded per arm, no captured result discarded); drivers `tools/l1/consol_{battery,all}.sh`, logs `/home/vibe/consol/{battery-s42,battery-s7,tail-s42,tail-s7,xt-s42,xt-s7,all,run}.log` + per-run client/server logs under `/home/vibe/consol/diag/`; lock `/tmp/rwm-vm.lock` held 10:34 UTC → released after teardown)

Arms: ship = env unset (the current shipped defaults: BBR-under +
`RWM_STORE_SACK_RELEASE`, legacy 1024 pool); stack = ship +
`RWM_STORE_PATHS=1 RWM_RECOV_MP=1 RWM_MSTAR_ANCHOR=1 RWM_CLOCK_GAP=1`;
loo-X = stack minus member X; stack-rs = stack + `RWM_PLAIN_RS=1` (c8
only). Σ = same-session same-env singles (2×sc2 at c7; sc2+sc3 at c8).
dnf=0 on ALL completed runs, both seeds.

**c7 (the Σ-gap cell), mean ± σ_s (n) → vs Σ:**

| arm | s42 | vs Σ | s7 | vs Σ |
|---|---|---|---|---|
| ship | 146.51 ± 5.23 (8) | 0.864 | 147.98 ± 1.41 (5) | 0.878 |
| **stack** | **166.31 ± 2.13 (8)** | **0.982** | **166.68 ± 2.76 (6)** | **0.988** |
| loo-pbs | 135.86 ± 38.43 (8) — **collapse class 3/8: 86.0/86.1/96.9** | 0.802 | 156.33 ± 15.08 (4) — collapse 1/4: 133.8 | 0.926 |
| loo-mp | 154.01 ± 2.24 (8), retx 18.0k | 0.909 | 152.77 ± 2.31 (6), retx 18.4k | 0.905 |
| loo-ms | 166.16 ± 1.45 (8) | 0.981 | 168.04 ± 1.18 (6) | 0.996 |
| loo-gap | 167.23 ± 1.61 (8) | 0.987 | 164.72 ± 4.26 (3) | 0.976 |

**c8 (asymmetric), mean ± σ_s (n) → vs Σ:**

| arm | s42 | vs Σ | s7 | vs Σ |
|---|---|---|---|---|
| ship | 83.23 ± 1.59 (8) | 0.825 | 81.00 ± 3.83 (4) | 0.808 |
| stack | 72.64 ± 9.61 (8) | 0.722 | 76.07 ± 10.46 (4) | 0.758 |
| **loo-pbs** | **85.89 ± 3.75 (8)** | **0.854** | **87.39 ± 2.88 (6)** | **0.870** |
| loo-mp | 74.75 ± 15.19 (8) | 0.743 | 82.96 ± 9.14 (4) | 0.826 |
| loo-ms | 68.35 ± 16.80 (8) | 0.679 | 80.93 ± 10.46 (6) | 0.806 |
| loo-gap | 75.57 ± 12.72 (8) | 0.751 | 77.05 ± 10.95 (5) | 0.767 |
| stack-rs | 79.11 ± 7.35 (8) | 0.786 | 87.98 ± 4.29 (4) | 0.876 |

**dual-c1 (anti-scaling control) vs same-session sc1:**

| arm | s42 (sc1-ship 191.8, sc1-stack 192.8) | s7 (sc1-ship 194.0, sc1-stack 196.0) |
|---|---|---|
| ship | 190.46 ± 18.37 (8), retx 28.6k | 195.81 ± 18.61 (8), retx 13.4k |
| **stack** | **208.41 ± 22.02 (8), retx 2.8k** (+15.6 above single) | **211.15 ± 27.78 (8), retx 3.1k** (+15.2) |
| loo-pbs | 208.65 ± 11.43 (8) | 203.38 ± 8.43 (8) |
| loo-mp | 194.90 ± 29.70 (8), retx 29.9k | 208.83 ± 37.10 (8), retx 19.8k |
| loo-ms | 219.13 ± 20.38 (8) | 203.72 ± 13.49 (8) |
| loo-gap | 214.69 ± 23.21 (8) | 222.05 ± 29.07 (8) |

**Singles (Σ terms + N=1 inertness):** sc2 all five arms within
84.26–85.04 both seeds (spread < 1σ_s — every member inert at N=1, as
constructed); sc3 within 15.93–16.16 both seeds. sc1-stack =
sc1-ship ± σ.

**Tail crown (tail_matrix c2, shipped streaming Realtime, `stream` vs
`stack` env, 5 reps/arm/seed, warm tunnel):** p50 ~8.0–8.6 ms IDENTICAL
across arms and seeds; per-rep p99 medians [min–max]: s42 stream 43
[35–44] / 55 [44–171], stack 40 [36–43] / 45 [39–71] (400B/1200B); s7
stream 42 [36–49] / 50 [39–60], stack 40 [36–57] / 53 [40–88]. Stack ≤
stream at 3/4 cells, +3 ms at s7-1200B — deep inside the rep spread.
**The 12–48× crown SURVIVES the stack env** (STORE_PATHS/RECOV_MP are
reliable-window-gated = inert here by construction; the live members are
the anchor pair, echo-verified `clock-gap estimator hygiene ACTIVE` +
`M* peer-report RTT-feed suppression ACTIVE` on both endpoints).

**Cross-traffic c2 (one pass, documented share — a caveat, NOT a gate):**
stack (BBR default + stack env) vs 1 established Cubic flow: rp share
0.956/1.0/0.964/0.961/1.0 (s42), 0.937/0.978/0.967/1.0/0.941 (s7); ship
reference 0.896–1.0 (s42), 0.957–1.0 (s7, n=4). Same class as the
documented BBR-under fairness at the GE c2 cell (0.95–0.96 — Cubic is
Mathis-bound there); the stack members do not move fairness. The clean-
bottleneck contention story is unchanged (goal-gate "Copa Competitive
Mode + Cross-Traffic"; the named blocker there is CC-independent).

### Per-member LOO verdicts vs the pre-registered flip rule

- **`RWM_STORE_PATHS` — FLIP ON.** Removal from the stack re-opens a c7
  COLLAPSE CLASS on both seeds (3/8 runs at 86.0–96.9, 1/4 at 133.8 —
  the pool-starvation mode the member was built to kill) and costs
  −30.5/−10.3 mean; dc1/singles neutral. No cell regressed ≫σ on either
  seed. **Recorded honestly — the c8 WATCH:** at c8 the stack sits
  BELOW loo-pbs on both seeds (0.722/0.758 vs 0.854/0.870×Σ) and below
  the SR-only ship arm (0.825/0.808), at ~1.1–1.4σ_s per seed — under
  the pre-registered bimodality clause this cannot gate the member, and
  the shipped stack's c8 (0.72–0.76×Σ) does not regress the HISTORIC
  pooled record class (0.74–0.80×Σ) — but the direction is consistent
  across seeds and the register carries the named follow-up: under
  SACK-release the legacy 1024 pool has become the better c8 pool law
  (the §16.22 "pooled VINDICATED at c8" verdict was pre-SR and has
  MOVED); a c8-aware pool law (asymmetric scaling or per-topology
  gating) is the next pre-registerable item, and it can bank a measured
  +11–13 Mbit at c8.
- **`RWM_RECOV_MP` — FLIP ON.** Removal costs −12.3/−13.9 ≫σ_s at c7 on
  both seeds (retx 18.0k/18.4k vs 5.4k) and re-opens the dual-c1 retx
  flood (29.9k/19.8k vs 2.8k/3.1k, with the s7 bimodal low runs
  171–180 returning); c8/singles neutral within σ. No cell regressed ≫σ.
- **`RWM_MSTAR_ANCHOR` — FLIP ON (on the "neutral + wins elsewhere"
  clause).** Every plain bulk LOO row is inside σ on both seeds (c7
  −0.15/+1.4; c8/dc1/singles within their σ) — the plain-live subset
  (peer-report RTT-feed suppression + estimator seed-from-sample,
  echo-verified per arm) is measured FREE at the bulk cells; the tail
  cell is unregressed. Its wins are elsewhere and recorded: the M* knee
  engagement (r100 +25/31%, r200 +62/82%, §16.21) is GENERATION-gated —
  so the knee benefit ships only for gen-mode arms, and the
  generation-default question inherits it (noted per pre-registration;
  the plain default banks the hygiene subset at zero measured cost).
- **`RWM_CLOCK_GAP` — FLIP ON (same clause).** Bulk rows inside σ both
  seeds (sign flips seed-to-seed → noise); tail cell unregressed;
  echo-verified live on every arm. Its win is the §16.21 post-stall
  estimator-poisoning discard (the realtime collapse-attribution family).
- **`RWM_PLAIN_RS` — NOT flipped; probe result recorded.** The c8
  composition probe shows the −3–5 Mbit witness cost RESOLVED in
  composition — stack-rs ≥ stack on both seeds (+6.5/+11.9, sub-σ vs the
  stack arm's σ but σ-tight itself: 79.11 ± 7.35 / 87.98 ± 4.29, the
  best-or-equal c8 arm at s7). Left OUT this pass because the inclusion
  bar for a DEFAULT is the full LOO criterion and RS was probed at ONE
  cell (no c7/dc1/tail/singles composition rows, and its default would
  also engage `RWM_HONEST_CAP` semantics untested in this composition).
  Named flip candidate: the c8-aware pool follow-up battery should carry
  RS as a full stack member.

**FLIPS LANDED (defaults in code, 2026-07-21):** `RWM_STORE_PATHS=1`,
`RWM_RECOV_MP=1`, `RWM_MSTAR_ANCHOR=1`, `RWM_CLOCK_GAP=1` by default
(`=0` = the per-member legacy opt-out arms; `RWM_ANCHOR_HYGIENE=0` turns
the anchor family off as a group). The shipped default IS the composed
stack: c7 0.982/0.988×Σ, dual-c1 +15 above single with retx ×10 down,
tail crown intact, singles identical, fairness class unchanged.

**Default-env L1 smoke of the SHIPPED binary** (env fully unset — the
post-consolidation defaults; binary sha256 8c0ac420da155484… = commit
5ebbcda, 2026-07-21 13:07 UTC, seed 42, log
`/home/vibe/consol/default-smoke.log`): c7 **167.7** (= the battery's
stack arm class, 0.99×Σ), c8 88.1, dc1 192.5, sc2 84.0, dnf=0, and ALL
FIVE mechanism echoes present on every run with nothing set —
`SACK-clocked store release ACTIVE`, `path-scaled outstanding pool
ACTIVE`, `multipath recovery suppression ACTIVE`, `M* peer-report
RTT-feed suppression ACTIVE`, `clock-gap estimator hygiene ACTIVE`. The
shipped default carries the composed stack end to end.

Ops: VM lock `/tmp/rwm-vm.lock` held 10:34 UTC → released after
teardown; CRLF converted after each sync (discipline 10); rp-* netns
only; stale binary removed before every rebuild; battery + smoke logs
and per-run diag preserved under `/home/vibe/consol/` (binary hashes in
`BINARIES.txt`); seed-7 topo-abort count (75, all summary-less)
recorded above.

## Unified Shedding + Flip Battery (2026-07-21) — PRE-REGISTRATION (roadmap item 3; discipline item 11 — this block written BEFORE the build; branch `feat/unified-shedding`)

*Decision record: → [ADR-0064](adr/0064-unified-span-machine.md)*

**(a) Mechanism.** At overload the RLC-family reliable path serializes
EVERYTHING behind the frontier — the COLLAPSE ATTRIBUTION's amplification:
a whole-process transient backs up the stream, the in-order EVICT pipeline
holds delivery 4×SRTT per hole round while the sender grinds stale
retransmits oldest-first, and chained episodes turn a ~1-s stall into
p50-seconds — while the streaming machine under the SAME transient sheds
~1% of messages past its ~20-ms reorder horizon and its tails never move
(§16.20.8: "at small δ, overload must be shed, not serialized"). Fix C =
**δ-honest overload shedding for the unified realtime path**: drop (do not
serialize behind) data that can no longer meet its deadline, priced by δ,
bounded by ρ.

**The (δ, ρ) semantics, stated precisely.** Shedding applies to
DEADLINE-PRICED REALTIME delivery only — never to the reliable-transfer
contract: the RETAIN-UNTIL-ACKED path (`window_reliable`, ρ = 1) is
excluded BY CONSTRUCTION (the law is compiled out when `reliable`; the
reliable reorder buffer never gives up on a hole, unchanged). On the
EVICT/realtime path the contract is (δ, ρ) with ρ < 1: delivery within the
deadline D(δ), and a residual loss allowance 1−ρ that the (δ, ρ, r) design
already concedes. A symbol/message is SHEDDABLE iff BOTH:

1. its projected delivery exceeds D(δ) — sender side: a retransmit of
   seq s fired at age > D arrives after the receiver's own δ-horizon
   give-up (send + owd + D), pure waste; receiver side: a hole held
   longer than D can only serialize successors past THEIR deadlines;
2. its loss stays within the 1−ρ budget — cumulative shed ≤ (1−ρ) of
   the stream; beyond the budget the machine SERIALIZES (holds/keeps
   retransmitting): ρ wins over δ, the completeness contract survives
   overload.

Both thresholds are DERIVED, no new constants, from the parameters the
unified machine already carries (§16.20.3/§16.20.5):

- D(δ) = min(b(hint)·RTprop, 2·RTprop) — the span law's own deadline
  (b = ½ Realtime / 1 Auto / 2 Bulk; the same measured anchors);
- 1−ρ = ε̂ · (1 − P_fec(r_live, ε̂, A*, σ²_burst)) — the §8.1 normal-
  approximation residual the design leaves past in-window FEC at the
  operating point (ε̂ = measured loss, r_live = the consumed taper rate,
  A* = the live solvable-span width, σ²_burst = the GE burst factor):
  the same ~1% class the streaming machine sheds. Receiver side (which
  owns no r/A*): the loss-class bound 1−ρ_recv = ε̂_recv (holes given up
  ≤ the channel's own measured loss fraction of the frontier) — shed is
  intrinsically holes-only, so the realized fraction stays in the
  FEC-residual class; the ε̂ bound is the serialize backstop.

Sites: sender — the P_lost retransmit branch, the SACK-gap service loop,
and (via the synthesized gap) the tail sweep drop past-deadline seqs from
`retransmit_buffer`/`nack_retx_at` into a shed set (pruned at the
cumulative frontier, the split_off twin); receiver — the in-order reorder
hold becomes the δ-derived H = b·SRTT (the §16.20.3 "reorder_timeout IS
the δ dial", replacing the bulk-shaped 4×SRTT ∈ [60, 300] ms clamp) while
the budget holds, reverting to the legacy hold when it is spent. Env:
part of the unified machine's realtime semantics under `RWM_UNIFIED`
(not a separate knob); sub-gate `RWM_UNIFIED_SHED=0` reproduces the
serializing arm for A/B. Composition: `RWM_ASTAR_ANCHOR` becomes default
ON under `RWM_UNIFIED` (the span law ships with its repaired anchor —
fix A gates this battery; `=0`/`RWM_ANCHOR_HYGIENE=0` still opt out).

**(b) Predictions (effect size + cells).**

1. The unified realtime collapse class (3/14 L0 base rate; 3/10 L1)
   goes to ~0 with shedding ON (the environmental trigger is
   seed/load-dependent — enough reps, honest incidence reporting).
2. Unified p99 ≤ legacy-RLC at every realtime cell (the flip gate), and
   within-class of streaming on tails at the standard cells (the 12–48×
   crown row: stream p99 medians 40–45/50–55 at c2 on current defaults).
3. Unified keeps its delivery-completeness advantage where ρ demands
   it: the 99.4–100% vs 74–76% point (c3 perf cell) remains AVAILABLE
   at the high-ρ setting — shedding is δρ-parameterized, not
   unconditional (`RWM_UNIFIED_SHED=0` is that arm; with shedding ON
   delivered% ≥ 1−ε̂-class ≈ 95%+, still ≫ streaming's 74–76%).
4. A*-inertness resolution (pre-registered composition check): with
   `RWM_ASTAR_ANCHOR` ON in the unified arm, spans reach derived width
   within ~1–2 RTT of stream start ([SPAN] a*=derived, not 1) and
   ru/rf rises well clear of the 9% inertness class (FDIAG at L0).
5. Bulk parity holds (anchors hygiene-ON; the decoder swap was already
   parity); knee cells c2r100/c2r200 show no unified regression vs
   legacy at the same gates (M*/A* both anchored — first fully-live L1
   look at the §16.20 depth law).

**(c) Falsification.** If shedding-ON still collapses (outage-class reps
with p50 in seconds at the c3-1200B cell), the amplification attribution
is wrong → report, do not iterate; the build defaults to the deprecation
register per discipline 11. If unified+shed loses the tail gate to
legacy-RLC anywhere with no collapse class, the blocker is named and
`RWM_UNIFIED` stays OFF. If delivered% falls below the 1−ε̂ class, the ρ
bound is mis-derived → report.

**(d) Derivation re-read for self-contained failure predictions.** Named
bounds, none disqualifying, recorded: (1) the shed law cannot fix
pure-rate overload (no holes to shed — a saturated link is the inner
CC's job, not shedding's); the collapse class is stall×loss×recovery
serialization, which IS hole-shaped. (2) At b=½ the sender-side deadline
D = RTprop/2 sits BELOW one ARQ round (~1 SRTT), so within-budget the
realtime machine becomes FEC-only recovery — exactly the streaming
machine's operating point; the risk is delivered% falling toward 1−ε̂ if
FEC is inert — which is why fix A (`RWM_ASTAR_ANCHOR` ON) is bound into
the same arm and its liveness (prediction 4) is a gate, not a hope.
(3) The receiver δ-hold (≈ 20–45 ms at c3) re-enters the regime the
4×SRTT hold was built against ("holes force-delivered just before their
repair arrived", pre-P10b) — but that lesson was measured with LEADING-
window repair and ARQ-round recovery; the unified span law recovers
in-window within ~D by construction, and the ρ budget reverts the hold
to legacy when give-ups exceed the loss class. (4) The 2026-07-07 SACK
flow-control null ("sender-side decoupling alone does not lift c2")
does not bind: shedding is not a throughput lever here, it is a tail/
serialization lever; c3-class tails are the cell.

**Battery (pre-registered).** L0 FIRST (dev box, its own era):
`unified_stream_l0` 4 arms — stream / unified+shed / unified-no-shed
(`RWM_UNIFIED_SHED=0`) / legacy-rlc — ≥12 seeds interleaved, collapse
incidence vs the 3/14 base + tail + delivered% per arm; one FDIAG rep
per unified arm for the A*/ru-rf check. Then L1 (VM lock protocol, CRLF,
FOREGROUND polling only, rp-* netns only, rm stale binary first,
liveness echoes + env + sha256 recorded, seeds 42+7, state runtimes,
seed-7 topo-abort ns recorded): (i) 3-arm tail_matrix realtime
(stream / unified+shed / legacy-rlc) p50/p99 + delivered count at c2 AND
c3 ×8 reps/seed — THE GATE: unified+shed ≥ legacy-RLC everywhere and
within-class of streaming, delivered%/completeness reported; (ii) bulk
parity spot (unified vs gen-sys, sc2 + c7, ×4); (iii) the rstar realtime
cell (c3 perf realtime ×8: delivered% + cod/src — does the span law +
TAPER_R realize r* at the wire; the §8.4.1 chain's last link); (iv) knee
cells c2r100/c2r200 ×4 (unified vs legacy, gen-sys, anchors ON both
arms). FLIP DECISION (pre-registered): `RWM_UNIFIED` default ON iff
tails ≥ legacy-RLC everywhere + no collapse class + bulk parity + knee
no-regression, both seeds. If it flips: streaming-machine retirement
enters the DEPRECATION REGISTER with a re-test clause (the 12–48× crown
is streaming's; retirement requires unified to hold that class at every
historic tail cell in a LATER pass — the register argues it, nothing is
retired this pass). If it does not flip: the blocker is named, register
updated.

*(Results below this line were written after the build/batteries ran.)*

### The law as built (commit 6568822; the code shape)

Sender (`net/mod.rs` run_window_sender): armed iff `RWM_UNIFIED` + EVICT
(`shed_armed(unified, reliable, gate)` — `reliable` compiles the law OUT:
the ρ=1 RETAIN contract can never shed). At the two recovery decision
points (the P_lost oldest-candidate branch and the SACK-gap service loop;
the tail sweep feeds the latter) a hole whose age exceeds
D = `shed_deadline_us(b(hint), RTprop)` = min(b·RTprop, 2·RTprop) — the
span law's own deadline, b=½ Realtime — is DROPPED from
`retransmit_buffer`/`nack_retx_at` into a shed set (pruned on the
cumulative `split_off` twin), IFF cumulative shed ≤ the derived 1−ρ
budget `residual_loss_after_fec(ε̂, r_live, A*, σ²_burst)` =
ε̂·(1−P_fec) (fec_rate.rs, the §8.1 normal approximation at the live
operating point — ε̂/σ² measured, r_live = the consumed taper r*, A* =
the live span width; no new constants). Past-deadline candidates the
budget refuses are counted (`shed_denied`) and SERVED — the serialize
arm: ρ wins over δ. Receiver: the in-order EVICT hold becomes the
δ dial `shed_recv_hold` = b·SRTT (§16.20.3: "reorder_timeout IS the δ
dial") while the give-up budget `holes ≤ ε̂_recv·frontier` (the
loss-class bound) is open; spent ⇒ the legacy 4×SRTT ∈ [60, 300] ms
clamp returns (serialize). Composition: `RWM_ASTAR_ANCHOR` now defaults
ON under `RWM_UNIFIED` (the span law ships with its repaired anchor).
DIAG: `shed=total/denied bud= D=` in [SPAN]+[DIAG]; `[SHED-R]
holes/frontier/budget_open` at the receiver. Unit tests (6 new, lib
377/377): shed only past-deadline AND within-budget (incl. cold-start
zeros); the reliable contract NEVER arms; deadline ≡ the span law's D;
receiver hold = δ dial with legacy fallback bit-exact (60/300 clamps);
budget laws (ε-class receiver bound; ε·(1−P_fec) monotone in r, = ε at
r=0, 0 at cold start, c3-class value in the streaming ~1% band).

### L0 battery (dev box, its own hardware era; test binary
`unified_stream_l0-44ebc61d9322064f.exe` from 6568822, same binary all
arms; c3-1200B, 50 msg/s × 20 s, 4 arms interleaved per seed ×14 seeds;
RWM_DIAG=1; arms S = shipped streaming, U = RWM_UNIFIED=1 (shed ON),
UNS = RWM_UNIFIED=1 RWM_UNIFIED_SHED=0 (serializing control), R =
RWM_L0_BACKEND=rlc)

**Environment, recorded honestly (discipline 9-class caveat):** seeds
1–3 ran on a quiet box; from seed ~4 onward the box carried concurrent
release builds + the 258-s gate_suite run (the COLLAPSE ATTRIBUTION's
compile-class trigger, present UNCONTROLLED for most of the battery —
multi-second whole-process stalls, heavier than the attribution
session's load: today even the STREAMING arm outages). Arms are
sequential within a seed, so per-seed cross-arm exposure is NOT
controlled; incidence is comparable only at the battery level.

Per-arm summary (median over 14 reps; outage-class = ≥1 delivery gap
> 1 s; p50-sec = the L1 collapse signature, p50 > 1 s):

| arm | outage-class | **p50-sec** | median p90 [range] | median p99 | lost total (median/rep) |
|---|---|---|---|---|---|
| S stream | 3/14 | **0/14** | 95 [93–4988] | 172 | 140 (8) |
| U unified+shed | 3/14 | **0/14** | 78 [62–11504] | 127 | 275 (2; 242 of them ONE rep, below) |
| UNS unified no-shed | 2/14 | **0/14** | 79 [64–1996] | 126 | 1 (0) |
| R legacy-rlc | 4/14 | **0/14** | 95 [80–3004] | 163 | 8 (0) |

- **The L1 collapse signature (p50 in SECONDS, the whole stream
  backlogged) appears in ZERO reps of ANY arm — including the harshest
  host-load episodes.** The #61 base rate at this shape was 3/10 (L1)
  and 3/14 (L0 attribution) with p50-seconds; the amplification class is
  gone from the unified arm at L0. The outage-class reps that DO occur
  land in ALL FOUR arms (streaming included, 3/14, p90 3.6–5.0 s) —
  i.e., today's trigger overwhelms every machine equally; it is the
  environment, not the RLC-family amplification (which would spare the
  stream arm, as it did in the attribution session).
- **Quiet/typical rows: unified+shed posts the best tails of all four
  arms** — p90 62–78 vs UNS 78–79, R 94–96, S 93–95; p99 123–127 vs
  S 126–172, R 125–163. The δ-hold (the shed law's receiver arm) is the
  visible mechanism: UNS (identical but serializing) sits ~15 ms above U
  at p90.
- **The ρ story at the gauges.** The sender budget is ρ-CONSERVATIVE:
  bud = ε̂(1−P_fec) ≈ 0.000–0.002 at the healthy operating point (A* 4–7,
  r 0.2–0.5 ⇒ P_fec ≈ 1), so the sender sheds 0–25 seqs/rep while
  REFUSING 52–773 past-deadline candidates (shed_denied — the serialize
  arm live). The receiver ε̂-class budget sheds only holes (54/2142 =
  2.5% worst) and CLOSES when spent (budget_open=false observed). U's
  delivered completeness excluding the one environmental rep: 99.75%
  (33 lost / 13,000) vs streaming's 99.0% (140/14,000) — the unified
  machine sheds LESS than streaming while beating its p90.
- **The U-s11 rep, attributed honestly:** 758/1000 delivered, p90 11.5 s
  — a ~7-s whole-process outage (maxgap 7.01 s; the same seed's other
  arms all outage too). The gauges exonerate the law: sender shed 7
  (denied 681), receiver gave up 54 holes then closed its budget — the
  242 losses are the environment (chunks aged past the EVICT coding
  window during a 7-s freeze are unrecoverable in EVERY EVICT arm, and
  the box was mid-build-burst). Recorded as the trigger's tail, not the
  law's.
- **Prediction 4 (A*-inertness resolution): CONFIRMED.** FDIAG rep
  (seed 42, U arm): data-direction holes resolve by DECODE 51 vs
  SOURCE/ARQ 10–21 — the attribution's 1:4 INVERTED to ~3:1 FEC-first;
  a\*=4–7 live on ar≈94–99 sym/s within ~1 RTT ([SPAN]); ru/rf ≈ 15.5%
  (435/69) ≈ the span-hole ceiling 1−(1−ε̂)^A* ≈ 18%, well clear of the
  9% width-1 inertness class.
- Falsification check: NOT triggered — no shed-on collapse class
  (0/14 p50-sec), delivered% within the 1−ε̂ class everywhere including
  the environmental rep (75.8% ≈ 1−ε̂_episode with the law's own
  contribution ≤ 61 symbols by gauge).

### L1 flip battery RESULTS (VM 10.1.5.16, 2026-07-21 14:30–16:35 UTC; binary sha256 1bbc1e2afed2… = commit 6568822, SAME binary every arm; E5-2650 v3 aes+avx2+pclmulqdq (post-divide) in every log header; seeds 42 AND 7; drivers `tools/l1/shed_all.sh` (+ the rlc re-run and bulk-s7 top-up below), logs `/home/vibe/shed/{tail-*,c3rt-*,bulk-*,knee-*}.log` + per-run `diag-*`; lock `/tmp/rwm-vm.lock` held 13:22 UTC → released after teardown; stage runtimes: battery A 94 min, B 9 min, C+D 7 min + 12 min re-runs)

Arms run on the CURRENT shipped defaults (BBR + SACK-release +
STORE_PATHS + RECOV_MP + MSTAR/CLOCK_GAP all ON, env unset except the
arm knob). Liveness: the unified arms echo `unified span law ACTIVE` +
`unified overload shedding ACTIVE` (sender AND receiver) + `A* send-rate
anchor ACTIVE` + `RWM_UNIFIED: receive path on the unified global
decoder` at BOTH endpoints on every arm (8/8 echo sets per tail log);
tail arms ran without RWM_DIAG (the battery-1 precedent — echoes are
info-level; the shed gauges were verified at L0 and in the pre-battery
VM smoke).

**Harness caveat, recorded first (discipline item 7 recurrence).** The
c3-s7 rlc-1200B arm was LOST silently: a transient `topo.sh up` failure
under lib.sh's `set -e` killed tail_matrix mid-matrix after the arm's
start line (no BRINGUP_FAIL, no EXIT echo — the trap had been replaced).
Guard added (`|| true`; the ping probe now owns bringup failure, loudly)
and the arm re-run same-day same-binary ×8 (its 400B companion reproduced
the in-battery rlc-400B class: 110 [105–126] vs 110 [103–136] — the
session anchor for the re-run). The seed-7 topo-ping double-abort class
also hit battery C hard (bulk s7: LS-sc2 lost 9/12 invocations) — topped
up with 6 more interleaved reps/arm, ns quoted, no captured result
discarded.

**1. THE FLIP GATE — 3-arm realtime tail matrix** (`RWM_TM_ARMS='stream
unified rlc' tail_matrix.sh {c2,c3} 8`, warm tunnel, 50 msg/s × 20 s
× 1000 msgs/rep; per-rep p99 medians [min–max] over n=8):

| cell·size | stream (crown) | unified+shed | legacy-rlc |
|---|---|---|---|
| c2 400B s42/s7 | 40 [36–52] / 43 [35–59] | **37 [35–164] / 37 [35–55]** | 36 [34–41] / 37 [34–45] |
| c2 1200B s42/s7 | 52 [41–62] / 52 [37–81] | **40 [35–43] / 40 [37–61]** | 39 [37–42] / 39 [36–41] |
| c3 400B s42/s7 | 108 [103–148] / 125 [102–185] | **101 [97–125] / 108 [90–379]** | 109 [102–130] / 110 [103–136] |
| c3 1200B s42/s7 | 112 [99–116] / 133 [106–494] | **111 [92–133] / 101 [89–134]** | 102 [94–109] / 106 [90–118] (re-run) |

- **ZERO collapse-class reps: 96/96 completed reps (+ 16 re-run reps)
  have p50 ≈ 8 ms (c2) / 24–26 ms (c3) and n=1000/1000 delivered.** The
  #61 blocker (3/10 unified reps with p50 in SECONDS at c3-1200B) is
  GONE — prediction 1 CONFIRMED at L1.
- **Unified+shed ≤ streaming at ALL EIGHT cell-size-seed rows** (c2:
  37/40 vs 40–43/52; c3: 101–111 vs 108–133) — within-class and better;
  the 12–48× crown property is carried by the unified machine at these
  cells, not merely survived.
- **Unified+shed vs legacy-rlc: ≥ everywhere within the noise floor** —
  better at c3-400B both seeds (101/108 vs 109/110), tied +1 ms at the
  c2 rows, and at c3-1200B the delta SIGN-FLIPS across seeds (s42
  111 vs 102, s7 101 vs 106; rep ranges overlap broadly, Δ ≈ 1.6σ at
  s42, opposite sign at s7 — per discipline item 5 no regression claim
  survives the noise floor). The #61 base at this cell — unified 794/
  3064 vs rlc 340/205 with collapses — is replaced by three machines in
  ONE tail class with unified at-or-ahead.

**2. Realtime delivered reliability + r\* realization, c3 perf cell**
(`perf_rwm_c.sh c3 c3 realtime 100000 20 single` ×8/seed interleaved;
S = streaming (RWM_UNIFIED=0-era shipped arm), U = unified+shed, U0 =
unified `RWM_UNIFIED_SHED=0`; note this perf cell runs `--window-reliable`
⇒ ρ=1 RETAIN — the HIGH-ρ setting):

| arm | s42 delivered (n) | s7 delivered (n) | completer median_s | cod/src |
|---|---|---|---|---|
| S | 126/160 = 78.8% (8) | 114/140 = 81.4% (7) | 0.100–0.121 | 0.015–0.12 |
| U | **160/160 = 100%** (8) | **80/80 = 100%** (4) | 0.119–0.125 | 0.40–0.50 |
| U0 | **160/160 = 100%** (8) | **120/120 = 100%** (6) | 0.117–0.126 | 0.38–0.49 |

- Prediction 3 CONFIRMED and the #61 trade DISSOLVED: the unified
  machine is delivery-complete at the cell where streaming leaves
  19–21% DNFs — and the completer-median price collapsed from the #61
  ×3–4 (0.38–0.55 s) to ×1.2 (0.12 vs 0.10 s): with the A* anchor live,
  recovery is in-window FEC, not serialized ARQ.
- U ≡ U0 (both 100%, medians overlapping) — the shed law is inert on
  the ρ=1 reliable contract at L1, behaviorally confirming the
  never-shed-reliable invariant; the high-ρ point (99.4–100% class)
  remains exactly available.
- **r\* is REALIZED at the realtime wire (the §8.4.1 chain's last
  link):** cod/src 0.38–0.50 consumed as computed (S arms: 0.015–0.12 —
  the streaming emission remains r\*-inert), and the consumed r BUYS the
  measured 100% delivery. The #46/#85 arc closes: quantity law (#85
  TaperBudget) + solvable span (§16.20.3) + honest anchor (§16.21 fix A)
  together are what r\* needed to reach the wire.

**3. Bulk gen-sys parity, sc2 + c7** (25 MB ×1/invocation, interleaved;
n after the topo-abort top-up; mean ± σ_s Mbit/s):

| cell | LS legacy s42 · s7 | US unified s42 · s7 |
|---|---|---|
| sc2 | 74.11 ± 2.11 (4) · 71.54 ± 3.77 (3) | 74.12 ± 2.42 (4) · **76.26 ± 2.26 (6)** |
| c7 | 87.67 ± 11.17 (4) · 86.86 ± 6.95 (5) | 82.75 ± 4.96 (4) · 82.12 ± 8.14 (6) |

Parity within σ_s everywhere (sc2 tie/+4.7 with the sign flipping
across seeds; c7 −4.9/−4.7 at 0.6–0.7σ per seed — same class as the #61
battery's ±5 sign-flipping c7 deltas; recorded, not a gated regression;
GUARD OK + unified-decoder receiver echo on every US run).

**4. Depth knee, c2r100 + c2r200** (gen-sys single ×4/arm, both arms on
the shipped MSTAR/CLOCK_GAP defaults; L1 = legacy machine + RWM_GEN_PIPE=1,
U1 = unified; mean ± σ_s):

| cell | L1 s42 · s7 | U1 s42 · s7 |
|---|---|---|
| c2r100 | 46.22 ± 0.87 · 47.23 ± 1.07 | 47.92 ± 2.42 · 48.68 ± 4.88 |
| c2r200 | 34.59 ± 1.90 · 32.39 ± 2.02 | 35.52 ± 2.12 · 34.60 ± 2.01 |

The ENGAGED-knee class (§16.21: 47.9/48.5 at r100, 34.9/32.9 at r200)
reproduces in BOTH machines — the first fully-live L1 look at the §16.20
depth law with A\*/M\* both anchored; unified ≥ legacy at all four
cell-seeds (inside σ) — knee no-regression PASS.

### VERDICT vs the pre-registration

- Prediction 1 (collapse → ~0): **CONFIRMED** — 0 collapse reps at L1
  (vs 3/10 base) and 0 p50-sec reps in 56 L0 runs (vs 3/14 base); the
  L0 outage-class reps that remain hit ALL FOUR machines including
  streaming under the same (extreme) host-load trigger — the
  RLC-family-specific amplification is gone.
- Prediction 2 (tails): **CONFIRMED** — ≥ legacy-RLC everywhere within
  the noise floor; ≤ streaming at every row (stronger than predicted).
- Prediction 3 (completeness at high ρ): **CONFIRMED** — 100% both
  seeds, shed-inert on the reliable contract, at completer parity.
- Prediction 4 (A\*-inertness resolved): **CONFIRMED** (L0 FDIAG:
  DECODE:SOURCE inverted 1:4 → ~3:1, a\*=4–7 within ~1 RTT, ru/rf 15.5%
  ≈ the span ceiling).
- Prediction 5 (bulk parity + knee): **CONFIRMED** (within σ; knee
  engaged in both machines, unified at-or-ahead).
- Falsification clauses: none triggered.

**FLIP: `RWM_UNIFIED` DEFAULT ON (2026-07-21, commit b849acb).** The
shipped transport is now ONE machine across the δ axis: the unified
global sparse-aware decoder on both RLC wires, the (δ,ρ,r)-derived
span law A\*/M\*/Δ with its repaired send-rate anchor, and δ-honest
overload shedding on the realtime EVICT path — Realtime is the small-δ
parameterization, bulk the large-δ limit, no machine switch. `RWM_UNIFIED=0`
is the legacy three-machine opt-out arm (streaming keeps Realtime
there). Suites on the flipped default: lib 377/377; math full
(59/19/22/4/4/3/25); release gate_suite 15/15 (223.8 s, NO expectation
recalibrated); mtu_blackhole_wedge 2/2; perf_loopback 8/8; copa_sole/
fmtcp/daps/recov_mp loopbacks 1/1.

**Default-env L1 smoke of the SHIPPED binary post-flip** (env fully
unset; binary sha256 6720c00dcccc… = commit b849acb, seed 42, log
`/home/vibe/shed/default-smoke.log`): the default Realtime tunnel now
echoes the FULL unified set at both endpoints with nothing set (span
law + unified decoder + shedding sender/receiver + A\* anchor + the
consolidation stack echoes) and posts c3 p99 medians 108/91 ms
(400/1200B, n=1000/rep — the battery's unified class); the PLAIN BULK
default cells — which the flip battery did not cover and the
consolidation crown owns — HOLD their classes under the unified
default: sc2 85.4/84.7 Mbit (the §16.25 SR-arm class 84.4–85.0), c7
dual 163.0/169.3 (the consolidation stack class 166–168, ≈0.97–1.0×Σ),
dnf=0 everywhere. The shipped default carries the whole stack plus the
unified machine end to end.

**DEPRECATION REGISTER — streaming-machine retirement enters (re-test
clause; NOTHING removed this pass).** The streaming two-layer code
(`fec/streaming.rs` + `streaming-codes`) loses the Realtime default to
the unified machine but is RETAINED as the `RWM_UNIFIED=0` arm. The
crown's 12–48× record is streaming's; unified held/beat its tail class
at every cell of THIS battery (c2/c3 × 400/1200 × both seeds), but the
historic record spans more cells (the L2/L3 message-tail batteries,
quinn-vs-rp Metric A). Retirement (code removal) requires a later pass
re-arguing that record cell-by-cell on the unified default per the
register's two-stage discipline; until then streaming warns nothing
(it is a live opt-out, not a refuted mechanism).

**Named follow-ups (not built).** (1) The ρ-budget at the sender is
conservative by construction (ε̂·(1−P_fec) with the EVENTUAL P_fec —
within-deadline P_fec would be smaller ⇒ a larger honest budget); the
receiver's ε̂-class budget carried the tail work in this battery, so
nothing was owed, but the within-D form is the principled refinement.
(2) The L0 extreme-stall datum (multi-second whole-process freezes
collapse ALL machines incl. streaming) names the residual class as
environment-bound, not machine-bound — the CONFIRMATION PROTOCOL
(host-steal sampling) from the attribution remains open. (3) c7 US
−5 Mbit direction (0.6σ, both seeds) — watch at the next gen-mode
consolidation. (4) The c3-1200B s42 +9 ms vs rlc (sign-flipped at s7)
— re-measured free in any future tail battery.

Ops: VM lock `/tmp/rwm-vm.lock` taken 13:22 UTC, released after
teardown; tree synced via git archive + CRLF conversion before the
first harness invocation (discipline 10); rp-* netns only; stale
binary removed before each rebuild; battery + smoke logs and per-run
diag preserved under `/home/vibe/shed/` (binary sha256s in the log
headers: 1bbc1e2a… = 6568822 for the battery, 6720c00d… = b849acb for
the post-flip smoke); seed-7 topo-abort counts recorded per battery
above; foreground polling only, no stop-and-wait.

## Competitive Baseline (2026-07-21) — PRE-REGISTRATION (discipline item 11 — this block written BEFORE any measurement; branch `meas/competitive-baseline` from c3a9d76; MEASUREMENT task, harness glue only, no transport code touched)

**(a) The question.** Where does the SHIPPED DEFAULT stack (BBR-under +
SACK-release + path-scaled pool + recov-mp + anchor hygiene + unified, all
default ON as of c3a9d76) stand against the real competitors — native QUIC
(quinn), kernel TCP (Cubic AND BBR), and kernel MPTCP — for BULK and
REALTIME under the standard conditions? Every prior external comparison in
this file (L1 Phase 1/2, L2 claim table, Metric A, L3 REGIME MAP) predates
the substrate chain (walls #1–#9), the hardware divide, the consolidation
stack, and the unified flip; §17's claims have never been verified against
the competitors on the CURRENT binary. This battery is the external
referee for paper §17: it verifies or refutes the standing claims
(lossy-bulk advantage, the tail crown, dual-path aggregation) on the same
day, same VM, same netem cells, same seeds.

**The comparison matrix (pre-registered).**

Transports:
- **rp** = raptorpath shipped default, env unset (liveness echoes asserted
  per arm). Bulk arm = `perf --window-reliable` plain window (`RWM_GEN=0`,
  the consolidation crown's configuration; gen-sys is measured parity at
  ~free CPU — recorded, not re-run), single-path on c1/c2/c3, dual on
  c7/c8. Realtime arm = the default tunnel (unified machine) under
  tail_matrix.
- **quinn** = quinn-perf (the native-QUIC reference the substrate is built
  from), STOCK configuration; its CC is to be VERIFIED on the VM (expected:
  quinn's default Cubic) and run BBR too iff the example exposes it
  without patching — else its CC is documented as part of the record.
  DEVIATION FROM PHASE 2, pre-registered: the client runs
  `--upload-size 25M --download-size 0` so the object traverses the GE
  direction (cli→srv) like every other arm — the Phase-2 quinn numbers
  used download = the loss-FREE direction and are therefore NOT comparable
  (recorded here; this battery is the first direction-fair quinn bulk row).
- **tcp-cubic / tcp-bbr** = iperf3 `-C cubic|bbr` (availability via
  `sysctl net.ipv4.tcp_available_congestion_control`, modprobe tcp_bbr if
  needed), server in rp-srv, client in rp-cli (sender traverses the GE
  direction). Cold connection per run (documented geometry caveat; at
  25 MB the handshake is amortized). Sender-side completion without an
  app-level ack — a caveat that FLATTERS TCP slightly vs rp's
  delivery-acked completion; recorded, not corrected.
- **mptcp** = kernel MPTCP v1 (`IPPROTO_MPTCP` via transfer_bench.py,
  topo_dual's endpoint/limit configuration) over the dual topology, run
  under per-netns `net.ipv4.tcp_congestion_control` = cubic AND bbr.
  Liveness = MPTcpExt MPJoin counters (subflow actually joined) + goodput
  vs the single-path ceiling. If the kernel lacks MPTCP, that is
  DOCUMENTED and c7/c8 compare rp-dual vs the best single-path competitor
  + the theoretical Σ.

Conditions × workloads:
- **BULK: 25 MB objects.** Cells c1 (clean 1 Gbit), c2 (100 Mbit,
  GE 1.3/50 ≈ 2.6% loss), c3 (20 Mbit, GE 2/40 ≈ 4.8%), c7 (= c2+c2 dual),
  c8 (= c2+c3 dual). Metric: goodput mean±σ, n=8 per seed, netem seeds 42
  AND 7 for EVERY transport (the competitors ride the same seeded netem;
  ×8 repetitions each), arms interleaved round-robin per rep within one
  session, fresh topology/tunnel per invocation. Per-run timeout 400 s;
  DNF is a recorded datum. rp CPU recorded (CPUSRV/CPUCLI);
  iperf3/quinn client CPU via /usr/bin/time.
  At c7/c8 the arms are rp-dual, mptcp×{cubic,bbr}, tcp-bbr on path A
  (the best single-path competitor, same session); Σ references = the
  same-battery single-cell arms (rp: sc2/sc3; tcp: c2/c3-bbr).
- **REALTIME: the tail_matrix message workload** — 50 msg/s × 20 s,
  400 B and 1200 B, one-way delivered latency on the shared kernel clock,
  cells c2 and c3, n=8 reps per arm per seed, seeds 42+7. Arms: rp (the
  shipped default tunnel; tail_matrix gains a `ship` arm alias = env
  empty), tcp (kernel TCP stream, TCP_NODELAY both ends, framing = 4-byte
  length prefix + 8-byte send timestamp — transfer_bench.py stream mode;
  CC = cubic, the deployed default; CC is irrelevant at 0.5 Mbit offered
  load — loss recovery, not congestion control, owns these tails), quic
  (msg_lat, one ordered reliable QUIC stream, same framing/geometry).
  Metric: per-rep p50/p99 + delivered count (of 1000). FAIRNESS CAVEATS,
  stated in advance: TCP/QUIC have no message semantics (byte/stream
  framing above a reliable in-order stream; retransmit-based HoL applies
  — that IS the comparison point); rp's messages ride the TUN tunnel
  (inner TCP over the transport = extra geometry rp pays, historically
  ~equal p50); delivered<1000 for TCP/QUIC means tail messages were still
  HoL-blocked at harness timeout (a latency cliff reported as a delivery
  cliff — both readings recorded).
- **FAIRNESS note row (not re-run):** the ledger's cross-traffic numbers
  are reused — shipped stack vs 1 Cubic flow at GE-c2: rp share
  0.937–1.0 (Cubic is Mathis-bound there); clean shared bottleneck: BBR
  0.24 share at 305–316 ms standing queue, Copa-sole 0.023
  (contention-recovery pipeline, the CC-independent named blocker);
  goal-gate "Copa Competitive Mode + Cross-Traffic" + "Consolidation".

**(b) Expected outcomes (pre-registered; the honest priors from this
file).**

1. **c1 bulk: rp LOSES, large.** The engine's measured service walls
   (~19.5–22k sym/s ≈ 185–200 Mbit/s, §16.23) sit far below kernel TCP
   line-rate (~930 Mbit Phase-1) and below quinn's clean-path rate
   (Phase-2: 545). Expected rp ~180–195 vs tcp ~900+, quinn ~300–550.
   This row is the engine-ceiling price, stated without softening.
2. **c2 bulk: rp ≈ tcp-bbr (tie to small loss), rp ≫ tcp-cubic and
   (predicted) ≫ quinn-stock.** rp class 84–85; tcp-bbr steady ~93
   (Phase-1; loss-blind); tcp-cubic ~10–17 (Mathis-bound); quinn-stock
   carries Cubic-family CC → predicted to collapse toward the cubic class
   once the object rides the LOSSY direction (the direction fix above).
   A quinn result ≥ 70 would refute wall #1's premise and demand
   attribution.
3. **c3 bulk: possible honest LOSS to tcp-bbr.** rp recovery ceiling
   15.6–15.9 vs BBR steady ~18 (Phase-1); cubic ~1.4–3; quinn-stock
   predicted cubic-class. If tcp-bbr > rp here, the row names the lever
   (recovery-plane residual at the 20 Mbit cell), and the sub-cell claim
   "legacy Cubic: 3.2" gets its modern verification.
4. **c7 bulk: rp-dual expected to WIN vs mptcp-cubic (historic 15.4 =
   collapse class) and vs any single path (~90); mptcp-bbr is THE
   INTERESTING UNKNOWN** — loss-blind subflows could plausibly reach
   0.8–0.95×Σ (~135–160). rp class 163–168 (0.97–1.0×Σ). If mptcp-bbr ≥
   rp, that is a headline finding (kernel multipath matches the stack's
   crown cell), recorded as such.
5. **c8 bulk: the honest watch cell.** rp shipped default 0.72–0.76×Σ
   (the c8 WATCH: the stack's known worst cell, legacy pool reads
   0.85–0.87); mptcp-bbr unknown; single-path tcp-bbr on the c2 path ~90
   ≈ 0.9×Σ alone. Plausible LOSS row for rp — if so it re-prices the c8
   pool-law follow-up with an external bound.
6. **Realtime c2: rp WINS by orders of magnitude (the crown's modern
   verification).** rp p99 medians 37–52 ms class (unified default);
   kernel TCP historic p99 ~13 s (RTO cascade, both CCs); quinn historic
   p99 ~2.8 s. Prediction: tcp p99 ≥ 10× rp, quinn p99 ≥ 5× rp at
   c2-1200B. If TCP/QUIC land within ~2× of rp, the 12–48× crown claim
   is REFUTED on the modern substrate and §17.4 gets rewritten.
7. **Realtime c3: rp expected to WIN but closer.** rp 90–135 ms class;
   the pre-arc datum has kernel BBR at 198 ms p99 (Phase-1-era) and quinn
   at 1393 ms; the tcp arm here runs cubic (see (a)) — GE 4.8% bursts
   on a 91-ms-RTT-class cell should put TCP's p99 in the RTO class
   (≥ 1 s). A TCP p99 < rp's would be the honest surprise to attribute.
8. **Delivered%:** all reliable arms deliver 1000/1000 except where HoL
   outlasts the harness window (expected at c3 for tcp/quic in some
   reps); rp delivers 1000/1000 (the flip battery's class).

**(c) Verdict rule.** No flip gates on this battery — the deliverable is
the position table itself: per condition × workload, WIN/TIE/LOSS for rp
vs the best competitor arm, at the discipline-5 noise floor (a delta
inside the larger of the two σ_s is a TIE; sub-σ directions recorded as
watch, never claimed). Losses are recorded at full strength and each must
name the lever it exposes (engine service walls at c1; recovery ceiling
vs loss-blind BBR at c3; c8 pool law; mptcp-bbr vs rp-dual). If a
pre-registered expectation is refuted (e.g. quinn ≥ 70 at c2, TCP within
2× at realtime-c2), the refutation is the headline, not a footnote.

**(d) Derivation re-read / self-contained caveats.** (1) The engine-wall
prediction at c1 is already derived (§16.23) — the row cannot surprise;
it is included because a competitive table without the losing clean-path
row would be dishonest by omission. (2) iperf3's sender-side semantics
and quinn's warm-connection geometry both flatter the competitors
slightly; rp's fresh-tunnel-per-invocation includes engine warm-up in
session cost but the timed run excludes the warm-up object — geometry
deltas are documented per arm, not "corrected". (3) The realtime arms
compare a tunnel-carried TCP stream (rp) against raw sockets — rp pays
the tunnel tax; historically equal p50 within ~1 ms. (4) MPTCP's
scheduler/CC are kernel policy — both sysctls recorded; a cubic-subflow
collapse is a CC property, not an MPTCP-protocol property, which is why
the bbr arm exists. (5) netem's GE loss applies identically to every
transport; seeds 42/7 give two channel realizations, not statistical
independence across arms within a rep — which is exactly why arms are
interleaved per rep.

**Battery (pre-registered).** VM 10.1.5.16 per MEASUREMENT DISCIPLINE
1–10: lock `/tmp/rwm-vm.lock`; CRLF-convert after sync; FOREGROUND
polling only; rp-* namespaces only, never ens18/sshd/firewall; rm stale
binary before build; binary sha256 + commit + lscpu in every log header;
env record incl. kernel, iperf3 version, quinn checkout rev + verified
CC, mptcp sysctls, qdisc; seed-7 topo-abort ns recorded; ARMCOUNT
assertion per arm; runtimes stated. Missing tools installed via apt
inside the VM only. Drivers: `tools/l1/compet_battery.sh` (bulk,
interleaved) + `tools/l1/compet_rt.sh` (realtime) + a `ship` arm alias in
tail_matrix.sh — harness-only changes; lib suite run once on the branch
to prove the transport is untouched.

*(Results below this line were written after the battery ran.)*

### Environment verification (pre-battery, recorded)

- **quinn's CC is configurable without patching**: quinn-perf exposes
  `--congestion {cubic,bbr,new-reno}`, stock default **cubic** — so quinn
  ran BOTH stock-Cubic and BBR per the pre-registration's conditional
  branch. The client is the upload sender ⇒ the client's CC governs the
  measured direction.
- **MPTCP: PRESENT and used natively.** Kernel 7.0.14-101.fc43,
  `net.mptcp.enabled=1`, path_manager=kernel, scheduler=default; iperf3
  3.19.1 has native `--mptcp` (used with `-C`), transfer_bench's
  `IPPROTO_MPTCP` path is the delivery-acked arm; CC set per-netns
  (`net.ipv4.tcp_congestion_control`, both namespaces). `tcp_bbr`
  modprobed (available: reno cubic bbr). Subflow liveness proven per run:
  MPTcpExt MPJoinSynTx/MPJoinSynAckRx = 2 + goodput ABOVE the single-path
  ceiling (200 Mbit sender-side at c7).
- **iperf3's sender-side completion is disqualified as the verdict metric
  for short objects** — measured line-rate-clamped (c2-bbr "100.1 ± 0.0",
  c1 "200.0" both CCs: the send-buffer tail is not delivery). The
  Phase-1 precedent tool (transfer_bench.py, app-level delivery ack — the
  SAME semantics as rp perf) was added as the primary TCP/MPTCP metric
  (`tbtcp-*`/`tbmptcp-*` arms); iperf3 rows retained as the
  standard-tool cross-check. Harness commits 9250bf8, 05937b8.

### L1 RESULTS (VM 10.1.5.16, 2026-07-21 22:02 → 2026-07-22 02:28 UTC; raptorpath binary sha256 6720c00dcccc… — byte-identical to the b849acb post-flip binary, the no-transport-change proof for this docs+harness-only branch; quinn checkout 953b466 (quinn-perf 653baae1…, msg_lat 8661002e…); iperf3 3.19.1; python 3.14.6; E5-2650 v3 aes+avx2+pclmulqdq (post-divide) in every log header; netem seeds 42 AND 7 for EVERY transport; bulk arms interleaved round-robin per rep ×8, fresh topology per invocation; rp liveness echoes (SR/MP/MS/GAP + the full unified set on realtime) asserted per arm; drivers `tools/l1/compet_{all,battery,rt,topup}.sh` + tail_matrix `ship`; logs `/home/vibe/compet/{bulk-s42,bulk-s7,bulk-s7-topup,rt-s42,rt-s42-pass1-quiconly,rt-s7,tailrp-*,env,run}.log` + per-run diag under `/home/vibe/compet/diag/`; lock `/tmp/rwm-vm.lock` held 21:37 UTC → released after teardown; stage runtimes: bulk s42 81 min, s7 62 min + 7 min top-up; realtime rp 16–17 min/seed, competitors 34–35 min/seed)

**Incidents, recorded first (discipline 7/8):** (i) the first s42
TCP-stream pass lost all 32 reps silently — a root-owned server-log
leftover (`sudo tee`) made the vibe-shell redirect fail and the server
never started; fixed (05937b8) and the arm re-run in full ×8 same
session-class (QUIC pass-1 preserved as `rt-s42-pass1-quiconly.log`; its
medians reproduce in the re-run). (ii) The documented seed-7 topo-ping
double-abort class thinned several s7 arms (rp-c7/c8 to n=2, TOPO-FAIL
logged per invocation); +6 interleaved top-up reps/arm were run for the
affected arms (+47 runs, merged, n quoted per cell). (iii) Transient
connect failures leave n<8 on a few tb arms; n is printed everywhere, no
captured result discarded. (iv) 2/8 tcp-c3 reps in the s42 realtime
re-run produced no summary (n=7).

### BULK — 25 MB objects, goodput Mbit/s, mean ± σ_s (n)

Metric semantics per arm (fairness, as pre-registered): **rp** =
`perf --window-reliable` plain window, shipped defaults, delivery-acked
object on a warm engine (fresh tunnel per invocation, warm-up object
excluded); **tbtcp/tbmptcp** = kernel TCP/MPTCP with app-level delivery
ack (same semantics as rp); **quinn** = warm connection, sequential 25 MB
UPLOADS (the GE direction), goodput = stream bytes/duration (10/15/30 s
at c1/c2/c3) — partial objects count bytes; **tcp-\*/mptcp-\* (iperf3)** =
sender-side completion, cross-check only. All senders sit in rp-cli ⇒
every transport traverses the same lossy direction, same netem seed.

**c1 (clean 1 Gbit):**

| arm | s42 | s7 |
|---|---|---|
| rp (shipped default) | 163.9 ± 12.1 (8) | 167.5 ± 10.1 (8) |
| quinn-bbr | 914.6 ± 1.2 (8) | 915.9 ± 1.3 (8) |
| quinn-cubic (stock) | 694.2 ± 14.2 (8) | 700.7 ± 5.9 (8) |
| tcp-bbr (delivery-acked) | 896.3 ± 10.6 (8) | 902.6 ± 0.6 (8) |
| tcp-cubic (delivery-acked) | 880.0 ± 65.2 (7) | 901.1 ± 8.5 (8) |
| iperf3 rows (both CCs) | 200.0 (sender-side artifact) | 200.0 |

rp CPU: 1.26–1.29 s recv + 1.01–1.07 s send per 25 MB (whole-invocation
incl. warm-up; competitor CPU logs preserved in diag).

**c2 (100 Mbit, GE ≈2.6%):**

| arm | s42 | s7 |
|---|---|---|
| rp (shipped default) | 78.7 ± 2.2 (8) | 78.6 ± 3.3 (8) |
| quinn-bbr | **91.9 ± 0.8 (8)** | **92.4 ± 0.7 (8)** |
| quinn-cubic (stock) | 24.2 ± 0.4 (8) | 26.1 ± 0.9 (8) |
| tcp-bbr (delivery-acked) | 61.5 ± 2.1 (6) | 91.6 ± 0.7 (4) |
| tcp-cubic (delivery-acked) | 11.0 ± 0.2 (8) | 11.2 ± 2.0 (6) |
| iperf3 tcp-bbr / tcp-cubic | 100.1 ± 0.0 / 12.9 ± 1.5 | 100.1 ± 0.0 / 13.3 ± 1.8 |

(The kernel-BBR delivery split 61.5 vs 91.6 across seeds is a
realization effect — BBR's drain against one GE pattern; rp holds
78.6–78.7 on both. Stability is rp's, peak is BBR's.)

**c3 (20 Mbit, GE ≈4.8%):**

| arm | s42 | s7 |
|---|---|---|
| rp (shipped default) | 16.1 ± 0.1 (8) | 16.1 ± 0.2 (11) |
| quinn-bbr | **18.6 ± 0.2 (8)** | **18.6 ± 0.6 (8)** |
| quinn-cubic (stock) | 3.2 ± 0.1 (8) | 4.8 ± 0.3 (7) |
| tcp-bbr (delivery-acked) | 17.5 ± 0.3 (7) | 17.8 ± 0.2 (8) |
| tcp-cubic (delivery-acked) | 1.4 ± 0.1 (8) | 2.1 ± 0.2 (7) |
| iperf3 tcp-bbr / tcp-cubic | 18.4 ± 0.6 / 1.5 ± 0.1 | 19.4 ± 0.9 / 2.2 ± 0.2 |

**c7 (dual c2+c2):**

| arm | s42 | s7 |
|---|---|---|
| rp dual (shipped default) | 150.8 ± 9.5 (8) | 147.3 ± 11.7 (6) |
| mptcp-bbr (delivery-acked) | 148.9 ± 8.5 (7) | **169.3 ± 1.4 (10)** |
| mptcp-cubic (delivery-acked) | 23.5 ± 1.6 (8) | 25.1 ± 8.4 (5) |
| mptcp-bbr / mptcp-cubic (iperf3) | 175.0 ± 46.3 / 38.0 ± 5.9 | 180.0 ± 42.1 / 37.1 ± 10.6 |
| tcp-bbr path-A single (delivery-acked, same session) | 84.8 ± 11.0 (8) | 91.9 ± 0.3 (9) |

**c8 (dual c2+c3):**

| arm | s42 | s7 |
|---|---|---|
| rp dual (shipped default) | 67.4 ± 13.2 (8) | 73.8 ± 12.0 (7) |
| mptcp-bbr (delivery-acked) | **92.6 ± 5.3 (7)** | **89.7 ± 17.1 (14)** |
| mptcp-cubic (delivery-acked) | 10.8 ± 0.7 (8) | 13.9 ± 3.9 (4) |
| mptcp-bbr / mptcp-cubic (iperf3) | 78.0 ± 25.1 / 17.2 ± 4.5 | 81.7 ± 28.6 / 16.1 ± 4.3 |
| tcp-bbr path-A single (delivery-acked, same session) | **89.5 ± 3.4 (8)** | **92.1 ± 0.3 (10)** |

### REALTIME — 50 msg/s × 20 s, one-way delivered latency, per-rep p99 median [min–max] over n=8, delivered of 1000

rp = the shipped default tunnel (unified machine, echo-verified); tcp =
kernel TCP stream, TCP_NODELAY both ends, cubic (CC is irrelevant at
0.5 Mbit offered load); quic = quinn msg_lat, one ordered reliable
stream. Same framing everywhere (4-B length + 8-B timestamp).

| cell·size | rp p99 med [rng] | tcp p99 med [rng] · delivered | quic p99 med [rng] · delivered |
|---|---|---|---|
| c2·400B s42 | **37 [36–43]** | 1407 [100–13808] · 687–1000 | 55 [39–81] · all 1000 |
| c2·400B s7 | **36 [33–59]** | 1366 [100–13787] · 728–1000 | 60 [40–118] · all 1000 |
| c2·1200B s42 | **39 [36–48]** | 561 [123–6571] · 938–1000 | 138 [33–2668] · all 1000 |
| c2·1200B s7 | **38 [35–44]** | 209 [36–3318] · all 1000 | 342 [39–1343] · all 1000 |
| c3·400B s42 | **96 [89–142]** | 830 [325–16790] · 560–1000 (n=7) | 225 [141–5254] · all 1000 |
| c3·400B s7 | **103 [91–164]** | 3784 [778–16041] · 525–1000 | 150 [95–38073] · all 1000 |
| c3·1200B s42 | **96 [90–137]** | 2931 [330–15982] · 550–1000 (n=7) | 739 [177–43974] · all 1000 |
| c3·1200B s7 | **92 [88–116]** | 3878 [125–8914] · 775–1000 | 759 [121–43032] · all 1000 |

p50s are equal-class everywhere (rp 8.0–8.3 ms at c2 / 24–26 at c3 vs
raw-socket 5.5–5.9 / 21–23 — the ~2.5 ms delta IS the tunnel tax). rp
delivered **1000/1000 in all 32 realtime reps** across both seeds — the
only arm with no delivery cliff. TCP's delivered<1000 rows are HoL
cascades still blocked at the 25-s harness window (a latency cliff
reported as a delivery cliff, both readings recorded); QUIC delivers
everything but its worst reps carry 38–44 s p99 tails at c3 (the same
HoL class the L2-era measurement found at c5).

### FAIRNESS note row (reused, not re-run)

Shipped stack vs 1 established Cubic flow at GE-c2: rp share 0.937–1.0
(Cubic is Mathis-bound there; same class as BBR-under). Clean shared
bottleneck: BBR-under 0.24 share at a 305–316 ms standing queue;
Copa-sole 0.023 (the CC-independent contention-recovery pipeline
blocker). Goal-gate "Copa Competitive Mode + Cross-Traffic" +
"Consolidation" cross-traffic rows.

### THE VERDICT TABLE (noise-floor rule as pre-registered: Δ inside the larger σ_s = TIE)

| condition × workload | rp (shipped default) | best competitor | verdict |
|---|---|---|---|
| c1 bulk | 164–168 | quinn-bbr 915 / kernel TCP ~900 | **LOSS ×5.5** — the engine service walls, externally priced |
| c2 bulk | 78.6–78.7 | quinn-bbr 91.9–92.4 | **LOSS −14%** (vs kernel tcp-bbr delivery-acked: seed-split 61.5 vs 91.6 → TIE-class; vs every Cubic-family arm: WIN ×3–7) |
| c3 bulk | 16.1 | tcp-bbr 17.5–19.4, quinn-bbr 18.6 | **LOSS −9…−13%** (vs Cubic-family: WIN ×8–11 TCP, ×4–6 quinn-stock) |
| c7 bulk | 147–151 | mptcp-bbr 148.9 (s42) / 169.3 (s7) | **TIE (s42) / LOSS −13% (s7)** — kernel MPTCP-BBR matches the crown cell; both aggregate ×1.7–1.8 of single |
| c8 bulk | 67–74 | mptcp-bbr 89.7–92.6; kernel single-path bbr 89.5–92.1 | **LOSS −21…−27%** — and BELOW same-session single-path kernel BBR: the c8 WATCH externally confirmed and priced |
| c2 realtime | p99 36–39 ms, 1000/1000 | quic 55–342; tcp 209–1407 + delivery cliffs | **WIN ×1.4–8.8 vs QUIC, ×5–38 vs TCP**; only delivery-complete arm |
| c3 realtime | p99 92–103 ms, 1000/1000 | quic 150–759 (worst reps 38–44 s); tcp 830–3878 + delivered to 525/1000 | **WIN ×1.5–8 vs QUIC medians (×300+ at QUIC's worst reps), ×9–41 vs TCP**; only delivery-complete arm |
| fairness (note row) | share 0.94–1.0 at GE-c2 vs Cubic | — | documented caveat class unchanged (clean-bottleneck contention blocker stands) |

### Verdict vs the pre-registered expectations

1. c1 LOSS — CONFIRMED at the predicted magnitude (×5.5).
2. c2 — the "rp ≈ tcp-bbr" prediction held only as a seed-split tie;
   the REFUTED part is "rp ≫ quinn-stock covers quinn": quinn-BBR (which
   the environment check un-gated) beats rp by 14% on both seeds. The
   quinn-CUBIC collapse (24–26) confirms wall #1 externally on the
   reference stack itself: same quinn, CC swap cubic→bbr = ×3.8.
3. c3 LOSS to loss-blind BBR arms — CONFIRMED (−9…−13%): the 16.1
   "recovery ceiling" is ~87% of what BBR-class transports extract from
   the same lossy pipe. The historic "legacy Cubic 3.2" claim
   re-verified (1.4–2.2 delivery-acked).
4. c7 — the interesting unknown ANSWERED: MPTCP-BBR is rp's equal
   (s42) or better (s7, tight σ) at symmetric dual-path bulk.
   MPTCP-cubic collapse (23–38) reproduces the historic 15.4-class
   finding; the collapse was always the CC, not multipath.
5. c8 — the honest watch cell became the worst row: rp loses to
   MPTCP-BBR by 21–27% AND to single-path kernel BBR on the fast path.
6. Realtime c2 — crown verified with a caveat: vs kernel TCP the
   12–48× class reproduces (×5–38 at the medians, delivery cliffs on
   top); vs QUIC the median gap NARROWS to ×1.4–8.8 (quinn's c2-400B
   median is only ×1.5 rp's) — the pre-registered "quic ≥5× rp at
   c2-1200B" held at s7 (×9) but not s42 (×3.5). The crown's strongest
   modern form is: rp's worst rep across ALL 32 realtime cells is
   164 ms while every competitor's worst rep is 1.3–44 s — the tail
   CLASS (bounded vs unbounded), not a fixed multiplier.
7. Realtime c3 — WIN confirmed; the "TCP p99 ≥1 s" prediction held
   (830–3878 ms medians + delivery cliffs).
8. Delivered% — rp 1000/1000 everywhere as predicted; TCP delivery
   cliffs at both cells; QUIC complete but with 38–44 s tails.

### The levers the losses name (each pre-registerable)

1. **Clean-path emission/receive service walls (c1, ×5.5).** quinn
   moves 915 Mbit/s of QUIC in userspace on the same box — the ~190
   engine ceiling is OUR per-symbol service time (§16.23's 19.5–22k
   sym/s), not a userspace-transport bound. Lever: datagram
   batching/GSO + multi-symbol frames on the emission path.
2. **The BBR-gap at lossy singles (c2 −14%, c3 −9…−13%).** Half the c2
   gap is object-scale ramp (rp's own 100-MB steady class is 84–85);
   the rest, and the c3 gap, is recovery-plane residual — quinn-bbr's
   18.6 at c3 is the externally measured bar for the same pipe.
3. **c8-aware pool law (c8 −21…−27%)** — already the named wall-#7
   follow-up; now externally priced: kernel MPTCP-BBR banks 90–93 at
   the cell where the shipped stack holds 67–74, and kernel single-path
   BBR alone beats the shipped dual config.
4. **c7 is not a moat (TIE/−13%).** Symmetric striping over BBR
   subflows is a solved kernel problem; rp's differentiation there is
   the realtime/delivery surface, not bulk Σ.
5. **The realtime crown is the durable position** — the only
   delivery-complete arm, bounded worst-rep tails (≤164 ms vs 1.3–44 s),
   at a ~2.5 ms p50 tunnel tax. This is the product surface the bulk
   losses do not touch.

Ops: VM lock held 21:37 UTC → released 2026-07-22 after teardown;
CRLF-converted after every sync; rp-* namespaces only; stale binary
removed before build; logs + per-run diag preserved under
`/home/vibe/compet/`; lib suite run once on the branch binary
(375 passed / 0 failed / 2 ignored = the 377 set) — transport untouched,
binary hash equal to b849acb's.

## Copa-Sole on Clean Substrate (2026-07-22) — PRE-REGISTRATION (written and committed BEFORE the battery, discipline item 11; the simple mode-switch removal; branch `feat/copa-sole-clean`)

*Decision record: → [ADR-0054](adr/0054-substrate-cc-policy-bbr-default.md) (the policy surface this battery would collapse), [ADR-0068](adr/0068-copa-bbr-fusion.md) (the fusion that inherits the outcome either way)*

**(a) Mechanism.** Copa-sole's bulk gap (0.86–0.89× BBR-under at sc2,
0.73–0.76× at c7, 0.78× at sc3 — measured 2026-07-13, "Copa Wire-Signal"
#82) predates walls 8+9 and the consolidated defaults: those walls
throttled exactly the steady full-pipe regime where Copa trailed (the
frontier-clocked store starving the pipe a full frontier round per slot
— wall #9/§16.25; the phantom-retx recovery plane flooding it — wall
#8/§16.24). BBR-under's #82 reference numbers were measured against the
SAME broken substrate, but a controller that eats 38–124 ms of standing
queue rides out store-starvation stalls that a 4–7 ms-queue controller
cannot hide; if the walls were a bigger tax on the tight-queue arm, the
gap shrinks or closes on the repaired substrate. The mode switch under
test: the two-value CC policy surface (BBR for bulk, Copa for
latency-priced — ADR-0054's "endstate") vs ONE δ-parameterized
controller.

**(b) Prediction (effect size + cells).** On the current substrate
(SACK-release + `RWM_RECOV_MP` + path-scaled pool + anchor hygiene, all
default ON), passthrough+Copa-wire with δ(hint) reaches ~parity with
BBR-under on bulk — sc2/sc3/c7/c8 within ~0.95× or ≫σ-indistinguishable
— while KEEPING its measured queue/tail advantage (the ×18–25 tighter
slow-path standing queue class, re-confirmed on THIS substrate, not
assumed) and its C8 class (#82: 0.95–1.01× with σ collapsed). The
hint→δ mapping must be verified live in arm B (δ(hint) echo per
profile — bulk small-δ 0.005, realtime large-δ 50; the continuous knob
the flip is FOR).

**(c) Falsification.** A bulk gap ≫σ persisting on ≥2 cells on both
seeds ⇒ the gap is Copa's own dynamics (its δ-equilibrium operating
point), not the walls; the two-value policy surface STAYS, honestly
documented as a measured tradeoff (NOT flipped), and the fusion
ADR-0068 inherits the residual gap as its bulk target.

**(d) Derivation re-read for self-contained failure predictions.** Named
bounds, none disqualifying, recorded before measuring: (1) at
equilibrium Copa's coupling cap cwnd ≤ BDP + 2/δ is FULL utilization in
the model (BDP + a 1/δ = 200-symbol queue at Bulk δ ≫ the ~20-symbol
c2-path dither trough) — the model does not predict a structural sc2/c7
deficit once the pipe stays fed, so a persisting gap falsifies honestly;
(2) sc3 is the one cell where the δ-map ITSELF trades throughput
(1/δ = 200 symbols ≈ 96 ms of tolerated queue at c3's ~2 083 sym/s vs
the ~15.7 Mbit recovery ceiling): #82 measured 0.78× WITH the anchor ×4
slow-path over-read that Anchor Hygiene has since fixed — direction
unknown, so sc3 alone can not carry the falsification (hence the ≥2-cell
clause); (3) the Copa feed re-keys the outstanding cap to
gain×Σcwnd (#80) — under the pool/SACK-release defaults this composes
untested; a c7/c8 anomaly with healthy singles points there (the
store-cap composition, not Copa's law) and must be attributed before any
verdict; (4) the 2026-07-19 clean-shared-bottleneck starvation (share
0.023, "Copa Competitive Mode + Cross-Traffic") is OUT of this battery's
scope and stays a documented deployment caveat at the flip site — its
named binder is CC-independent (contention tail-drop recovery), it was
measured on the pre-SR pool, and no cross-traffic cell is in this
prediction.

**Battery (pre-registered).** VM 10.1.5.16 per MEASUREMENT DISCIPLINE
1–10 (lock poll FOREGROUND politely until the competitive-baseline
worker frees `/tmp/rwm-vm.lock`; CRLF conversion after sync; stale
binary removed first; env + sha256 + lscpu recorded; liveness echoes
per arm incl. the Copa wire/feed/δ and compete-default echoes; seeds
42+7 ×8 interleaved round-robin per rep; fresh tunnel per invocation;
same-session Σ; stage runtimes; seed-7 topo-abort ns recorded). Arms:
**A** = current default, env unset (BBR-under on the full consolidated
stack) · **B** = `RWM_QUIC_CC=passthrough` (Copa-sole: wire signal +
δ(hint) + feed defaults engage; `RWM_COPA_COMPETE` stays at its default
OFF) — BOTH on the full current default stack otherwise. Cells:
sc2/sc3/c7/c8 + dc1, plus per-arm queue/RTT distributions (per-path
rtp / appQ / wireQ p50+p90 from the sender DIAG clocks — the tail
advantage re-confirmed on this substrate, not assumed), plus ONE
realtime tail cell (tail_matrix c2, `default` vs `copa` arms — Copa's
tail claim on the shipped unified machine), plus δ(hint) echo one-offs
(realtime + auto) in arm B. Driver `tools/l1/copaclean_battery.sh` +
`copaclean_queues.py`; tail arms via `RWM_TM_ARMS='default copa'`.

**FLIP DECISION (pre-registered).** If the prediction holds (bulk
~parity + queue/tail advantage held, both seeds) → `RWM_QUIC_CC`
default flips to `passthrough`: Copa-sole becomes THE controller, `bbr`
joins `cubic`/`newreno` as explicit reference/fallback values, the
hint-selected mode switch is GONE and δ(hint) (+`RWM_COPA_DELTA`) is
the only latency/throughput knob; ADR-0054 gains a superseded-by note,
paper §12.11/§17.2 updated. If falsified → no flip; the residual gap is
documented as the measured tradeoff in ADR-0054 and paper §17.2, and
ADR-0068 carries the target.

*(Results below this line were written after the battery ran.)*

### L1 battery RESULTS (VM 10.1.5.16, 2026-07-22 02:40–03:16 UTC; binary sha256 6720c00dcccc1ff4… = commit b849acb's Rust (the docs+harness tree is e931981 — NO Rust source changed since b849acb, so this IS the post-consolidation+unified SHIPPED binary), SAME binary every arm; E5-2650 v3 aes+avx2+pclmulqdq (post-divide) in every log header; 1 run/invocation, 10 arms interleaved round-robin per rep ×8 reps, fresh tunnel per invocation, seeds 42 AND 7, RWM_GEN=0 RWM_DIAG=1 everywhere; drivers `tools/l1/copaclean_battery.sh` + `copaclean_queues.py` + `tail_matrix.sh` (`default`/`copa` arms), logs `/home/vibe/copaclean/{battery-s42,battery-s7,tail-c2}.log` + per-run client/server logs under `/home/vibe/copaclean/diag/`; lock `/tmp/rwm-vm.lock` held 02:34 UTC → refreshed after a spend-limit interruption, released after teardown)

Arms (all PLAIN, `RWM_GEN=0`, same binary, full current default stack —
BBR-under + SACK-release + path pool + recov-mp + anchor hygiene + unified
— otherwise): **A** = env unset (BBR-under, the shipped default) · **B** =
`RWM_QUIC_CC=passthrough` (Copa-sole; wire signal + δ(hint) + feed defaults
engage; `RWM_COPA_COMPETE` default OFF). Σ = 2× same-session sc2-A at c7;
sc2-A + sc3-A at c8. dnf=0 on ALL completed runs, both seeds.

**Liveness / stale-echo honesty (discipline items 1, 8).** Seed 42:
0 aborts, 8/8 completed every arm; every A run carries the BBR echo and
every B run the `engine-owned` + `feed ACTIVE` + `copa_wire=true
delta=0.005 cc_pace=true` echoes; 0 real contamination. Seed 7: the
documented topo-ping double-abort class hit hard (21 of 80 invocations
aborted — RUNTIME ~2 s, RATES no-diag, NO summary); on those aborted reps
the copaclean liveness check read the STALE `/tmp/rwm-c.log` left by the
prior arm and cosmetically flagged it "contamination" — every such flag
is PROVEN summary-less (per arm: 8 headers = n completed + n aborted), so
NO captured result is misattributed, and each completed run's controller
is independently confirmed by its throughput signature matching the s42
class within σ. n per arm (s7): sc2-A 3, sc2-B 6, sc3-A 6, sc3-B 7,
c7-A 5, c7-B 7, c8-A 3, c8-B 6, dc1 8/8. Every datum used is a cleanly
completed arm-rep with a valid summary; aborted invocations contribute
nothing.

**Throughput (Mbit/s, mean ± σ_s; B/A = copa/bbr ratio):**

| cell | A = BBR-under s42 · s7 | B = Copa-sole s42 · s7 | B/A s42 · s7 |
|---|---|---|---|
| sc2 | 84.98 ± 0.85 · 84.31 ± 1.11 | 75.86 ± 1.99 · 75.20 ± 1.02 | **0.89 · 0.89** |
| sc3 | 15.98 ± 0.26 · 16.23 ± 0.18 | 15.56 ± 0.31 · 15.73 ± 0.40 | 0.97 · 0.97 |
| c7  | 163.94 ± 1.95 · 166.02 ± 1.27 | 120.46 ± 2.61 · 121.79 ± 5.16 | **0.73 · 0.73** |
| c8  | 81.86 ± 3.55 · 83.82 ± 3.70 | 47.20 ± 12.88 · 47.77 ± 14.73 | **0.58 · 0.57** |
| dc1 | 194.78 ± 14.44 · 202.68 ± 23.09 | 130.03 ± 2.28 · 132.67 ± 2.37 | **0.67 · 0.65** |

Every B/A gap except sc3 is ≫ σ_s and reproduces to the third digit
across seeds. **The falsification condition (c) is TRIGGERED**: a bulk
gap ≫σ on ≥2 cells — here 4 of 5 (sc2, c7, c8, dc1) — on both seeds.
(External-baseline context, from the same-era competitive-baseline
battery on main: BBR-class transports extract ~92/18.6 Mbit at c2/c3;
BBR-under's sc2 84.98 sits in that class — Copa's 0.89× is a Copa
property, not a stack ceiling.)

**Queue distributions (s42 clean set — 0 aborts, every DIAG copy fresh;
per-path steady-state DIAG blocks 4+; wireQ = quinn packet-timed p50/p90
− RTprop = the NETWORK standing queue; appQ = app-echo − RTprop = the
consumer-experienced pipeline incl. the sender's own reservoir):**

| cell/path | A wireQ p50/p90 (appQ p50/p90) | B wireQ p50/p90 (appQ) | wireQ advantage |
|---|---|---|---|
| sc2 p0 | 89/98 (94/100) | **5/7 (90/123)** | **×18 tighter** |
| sc3 p0 | 487/515 (497/539) | **30/45 (455/517)** | **×16 tighter** |
| c7 p0/p1 | 50/40 (64/54) | **7/8 (234/126)** | **×6–7 tighter** |
| c8 fast/slow | 114/290 (357/428) | **30/112 (770/1782)** | ×3–4 tighter |
| dc1 p0/p1 | 0/0 (5/5) | 1/3 (14/50) | both ~empty (c1 unloaded) |

The s42 set is authoritative (0-abort — the whole reason for same-session
interleaving); the s7 queue distributions confirm the direction but are
partially polluted by the aborted-rep stale diag copies, so they are not
tabled as primary. The NETWORK-queue advantage is DECISIVELY re-confirmed
on the fixed substrate — Copa did not buy anything with bufferbloat.
(Honest note: at c8, Copa's tight wireQ coexists with a DEEP app-layer
reservoir — 770/1782 ms — and a collapsed, bimodal throughput: the
tight-queue equilibrium is exactly what starves the pipe at the
asymmetric cell here.)

**Realtime tail cell (tail_matrix c2, shipped UNIFIED Realtime machine,
`default` = BBR vs `copa` = passthrough, 50 msg/s × 20 s × 1000 msg/rep,
warm tunnel, n=8/arm/seed; per-rep p99 medians [min–max]; p50 ≈ 8 ms all
arms):**

| cell·size | default (BBR) s42 · s7 | copa (passthrough) s42 · s7 |
|---|---|---|
| c2 400B | 36 [35–139] · 37 [34–58] | **36 [35–39] · 37 [34–155]** |
| c2 1200B | 38 [37–43] · 39 [36–617] | **38 [35–175] · 39 [36–60]** |

Copa TIES BBR-under arm-for-arm on p99 medians at every cell-size-seed
(36/38 vs 36/38 s42; 37/39 vs 37/39 s7): at this cell the tail is
dominated by the unified Realtime machine + δ-honest shedding, not the
substrate CC, so Copa's queue advantage neither costs nor uniquely buys
the message tail — it holds the 12–48× tail class with no regression.
Copa's distinctive win is the whole-transfer standing queue on the bulk
cells above (the ×18/×16/×6–7 wireQ), where its bulk throughput is the
price.

**hint→δ live verification (arm B, s42 clean deltachecks — the flip's
continuous knob):** δ echoed = **0.005 for Bulk** (every bulk B rep,
`copa_wire=true cc_pace=true compete=false`), **0.5 for Auto**, **50 for
Realtime** — `δ(hint) = 0.5/ζ` live and continuous. The mapping the flip
would have been FOR is confirmed working; the flip is refused on
throughput, not on the knob.

### VERDICT vs the pre-registered prediction — FALSIFIED (no flip)

Prediction (b): bulk ~parity (within 0.95× or ≫σ-indistinguishable) on
sc2/sc3/c7/c8 with the queue/tail advantage held. Measured:

1. **Bulk parity FAILS decisively.** sc3 alone reaches parity (0.97×);
   sc2 holds the #82 0.89× gap UNCHANGED, and c7/c8/dc1 are FAR from
   parity (0.73/0.57/0.66×) — every gap ≫σ both seeds. The walls did
   NOT close the gap.
2. **The walls WIDENED the gap — mechanism named.** The #82 hope was
   that walls 8+9 + the pool throttled the full-pipe regime where Copa
   trailed. The opposite happened: those walls lifted BBR-under's
   aggregation (c7 ~100→166, c8 ~54→82 vs the #82 broken-substrate
   numbers) while Copa did NOT ride the same unlock — its δ-equilibrium
   caps cwnd near BDP + 1/δ regardless of how much pipe the walls free,
   so it leaves the freed capacity on the table (that tight queue IS its
   design). BBR eats the freed pipe; Copa, by construction, does not.
3. **Copa's #82 "C8 domination" is GONE — a broken-substrate artifact.**
   At #82 Copa strictly dominated BBR at C8 (0.95–1.01×) because the
   broken substrate suppressed BBR there; on the fixed substrate BBR
   c8 = 82 vs Copa 47 (0.57×, Copa bimodal σ 12.9/14.7). The domination
   inverted.
4. **The queue/tail advantage is REAL and re-confirmed on THIS
   substrate** (not assumed): ×18/×16/×6–7 tighter network standing
   queue at sc2/sc3/c7, tail parity at the realtime c2 cell. The tradeoff
   is genuine, not a substrate artifact.

**FLIP DECISION: NO FLIP.** The pre-registered gate (bulk parity +
tail/queue advantage, both seeds) is NOT met — parity fails on 4 of 5
cells ≫σ both seeds. The falsification clause governs: the gap is Copa's
own δ-equilibrium dynamics, not the walls (the walls were fixed and the
gap GREW). `RWM_QUIC_CC` default STAYS BBR-under; the two-value policy
surface STAYS, honestly documented as a MEASURED TRADEOFF (queue/tail vs
bulk throughput), NOT flipped on a wish. ADR-0054 gains the
measured-tradeoff amendment; ADR-0068 (the fusion) inherits the bulk gap
as its target — and this battery STRENGTHENS the fusion's motivation: a
BBR-style rate-model feed-forward baseline is exactly the mechanism that
would let a δ-priced controller convert the freed pipe Copa leaves on the
table (Copa leaves it precisely because it has only a delay price, no
rate model).

Ops: VM lock `/tmp/rwm-vm.lock` taken 2026-07-22 02:34 UTC (refreshed
2026-07-27 after a spend-limit interruption; both batteries had completed
cleanly BEFORE the interruption — nothing re-run), released after
teardown; tree synced via git archive + CRLF conversion before the first
harness invocation (discipline 10); stale binary removed before build;
rp-* netns only; battery + tail + smoke logs and per-run diag preserved
under `/home/vibe/copaclean/`; seed-7 topo-abort count (21) recorded
above; foreground polling only.

## Emission Batching (2026-07-27) — PRE-REGISTRATION (discipline item 11 — this block written BEFORE any profile run and BEFORE any build; branch `feat/emission-batching` from be24660)

**(a) The question.** LEVER 1 of the competitive-baseline losses: the c1
×5.5 gap (rp 164–168 vs quinn-bbr 915 on the SAME box, same VM, same
netem cells — "Competitive Baseline (2026-07-21)"). §16.23 attributes the
engine sink ceiling to per-process SERVICE-TIME walls: sender emission
saturates first at ~19.5–20k sym/s (τ ≈ 45–50 µs/sym: store insert +
placement + serialize + `send_datagram` + estimator per symbol), receiver
engine ~20–22k msgs/s just above, loop ≈ 1 core, wire/kernel idle
(system 2.6/6 cores, softirq ~0, ZERO UDP drops), NOT thread-count
(pins −7%/−2%). quinn moves 915 Mbit/s ≈ ~89k pkts/s of userspace QUIC
through the same kernel — so the ceiling is OUR per-symbol emission
path, not a userspace-transport bound.

**(b) Mechanism (pre-registered).** Per-symbol emission — ONE datagram
per `send_datagram` call with a full sender-loop iteration (select!
re-arm, tail-deadline scan, 4–5 scheduler-lock acquisitions, a
sent-store insert + symbol clone, a serialize allocation) per symbol,
and per-symbol wakeups/yields between sends — serializes the sender at
~20k sym/s ≈ 190 Mbit/s AND defeats quinn's own transmit batching: the
endpoint driver drains the datagram queue as fast as we feed it one at
a time, so poll_transmit emits ~one-datagram transmits and the
GSO/sendmmsg path (quinn-udp: up to 64 segments per sendmsg) never
engages. Note the 1279-byte symbol datagram in the 1350 MTU means one
datagram per QUIC packet — no packet-coalescing headroom; GSO batches
PACKETS per syscall and is the only available syscall amortization.
Batching the symbol→quinn handoff (emit in pacer-quantum bursts,
~64 KB ≈ 48–64 symbols, no await points inside a burst) plus removing
per-symbol allocation/locking amortizes per-send costs by ~an order of
magnitude.

**(c) Predictions (pre-registered).**
1. **Profile first**: the dominant sender core-second term at c1 is
   amortizable per-send overhead (our loop + quinn per-transmit +
   syscall density ~1 sendmsg/datagram), NOT irreducible per-packet
   compute (AEAD). Reference: quinn-perf's own syscall density at 915
   Mbit on this box (expected ≫1 packet/sendmsg via GSO).
2. **c1 (PRIMARY): default+`RWM_EMIT_BATCH` ≥ 400 Mbit/s** (external
   bar quinn-bbr 915; baseline 164–168). Mechanism evidence gate:
   sender syscalls/s drop ~an order of magnitude at equal-or-higher
   throughput.
3. **sc2/c7/c8 lift or hold** — their walls may be elsewhere
   (loss/recovery-bound; c2-class wire 100 Mbit is BELOW the emission
   wall): predicted ≈ no change ≫σ at sc2 (wire-bound at 78–79), c7/c8
   unregressed (emission headroom cannot add waste under the recov-mp
   law; any lift is a bonus, not gated).
4. **Realtime tails UNREGRESSED** (the crown is the product): ONE
   tail_matrix c2 spot-check ×4 per arm — batching must not add pacing
   burst jitter; p50/p99 within the historic class (p99 ~36–48 ms,
   1000/1000 delivered) is the gate.
5. **Reliability dnf=0** across all battery cells.

**(d) Falsification.** If the profile shows the dominant cost is NOT
amortizable per-send overhead (e.g. AEAD/crypto ≈ irreducible per
packet, or kernel-side per-packet cost dominates even under GSO), report
the honest per-core packet ceiling with the term named and build only
what the profile justifies. If c1 default+batch lands < 400 with
syscall density collapsed as predicted, the residual binder is named
with numbers (candidate: the receiver engine's ~20–22k msgs/s service
wall — receiver-side batched recv is in scope ONLY if it becomes the
measured binder mid-battery). A tail-gate regression (p99 class worse
than historic on the c2 spot) blocks the flip regardless of throughput.

**(e) Gate + flip rule.** `RWM_EMIT_BATCH` in src/gates.rs, DEFAULT OFF
for the A/B (perf-only, behavior-preserving, delivered-set unchanged;
unit tests: ordering/pacing contracts preserved, no symbol loss at burst
boundaries). Flip to default ON in the same branch IFF: c1 gate met
(≥400 both seeds), sc2/c7/c8 unregressed ≫σ, tail spot-check
unregressed, dnf=0, suites green.

**Battery (pre-registered).** VM 10.1.5.16 per MEASUREMENT DISCIPLINE
1–11: lock `/tmp/rwm-vm.lock` priority 1; CRLF-convert after sync;
FOREGROUND polling only; rm stale binary before build; binary sha256 +
commit + lscpu in every log header; seeds 42+7 ×8 interleaved arms
default ± `RWM_EMIT_BATCH` within one session; cells c1 (PRIMARY,
single-path 400 MB), sc2 (100 MB), c7 (dual 200 MB), c8 (dual 25 MB);
ONE tail_matrix c2 spot ×4; per-arm syscall density (strace -c /
/proc counters: sendmsg calls/s vs packets/s) + CPU (CPUSRV/CPUCLI);
quinn-perf syscall reference on the same box; seed-7 topo-abort ns
recorded; ARMCOUNT per arm; runtimes stated; same-session Σ references.

*(Results below this line were written after the profile/battery ran.)*

### PROFILE (2026-07-27 12:52–13:10 UTC, VM 10.1.5.16, binary b04bc50f… = code be24660; E5-2650 v3 aes+avx2+pclmulqdq; c1 cell, 1.2 GB single-path runs; logs `/home/vibe/embatch/profile-{rp-strace,rp-perf,quinn-strace}.log`)

**Sender flat profile** (perf -F 397 -g, 15 s at ~190 Mbit/s): the
dominant family is OUR per-symbol/per-ack control math — 28.7%/core:
`compute_repair_rate` 6.44 + `__ieee754_exp_fma` 6.18 +
`LossEstimator::record_batch` 5.93 + `predictive_loss_upper` 4.54 +
`__ieee754_log_fma` 3.39 + `exp` 2.19 (the taper/span block runs the
whole derivation PER SOURCE SYMBOL; record_batch runs per WindowAck —
the receiver acks EVERY in-order data symbol, ~20k acks/s). Then:
`WireMessage::serialize` 3.83, allocator family ~4.7, `memmove` 1.81,
sender-loop closure 2.51, `handle_control_message` 1.34. **AEAD is
noise: 1.67%** (aesni 1.33 + ring 0.34 — wall #5 re-confirmed).
quinn-proto ≈ 2.0 (poll_transmit 0.51, populate_packet 0.42,
process_payload 0.41, finish_and_track 0.34); kernel entries ~4–5 flat.

**Syscall density — the pre-registered prediction is HALF-REFUTED.**
strace -c (26.17 s window, throughput held 184 Mbit/s under strace):
the client makes only ~2 303 UDP sends/s for ~17.6k wire segments/s —
**quinn-udp's GSO path is ALREADY engaged at ~7.6 segments/sendmsg**
(veth mean tx packet 9 986 B), and the receive side runs GRO at ~13
datagrams/recv. The client's syscall profile is futex-dominated (7.2k/s
waker churn + 1.5k/s epoll), not send-dominated. **quinn-perf reference
(same box, same c1 topo, `--congestion bbr`, upload): 921.9 Mbit/s
sustained WITH strace attached, ~8 710 UDP sends/s ≈ 10.5 segments/send
≈ 73 sends/MB vs rp's ~100 sends/MB — syscall density per byte is the
SAME ORDER.** The ×5 gap is therefore NOT syscalls; it is per-symbol
CPU in the engine loop (τ ≈ 45–50 µs/sym of control math + loop
machinery + store/alloc, serialized). Per the falsification clause the
mechanism-evidence gate moves from "syscalls/s drop ×10" to CPU/bit +
throughput with the term named.

### BUILD (what the profile justified — and what it refuted mid-branch)

Shipped under `RWM_EMIT_BATCH` (default OFF; `RWM_EMIT_BURST` default
64 ≈ the ~64 KB pacer quantum), SENDER-ONLY:
1. **Pacer-quantum burst TUN intake** — ≤ burst symbols per loop
   iteration inside the flow-control store headroom (live local
   counters) and the cc_pace token bucket, checked per symbol; the
   select! re-arm / tail-deadline scan / SACK-drain / pacing-refresh
   iteration cost amortizes ×burst and quinn's driver sees a
   multi-datagram queue.
2. **Per-burst taper/span refresh** — the 15–17%/core per-symbol
   derivation (repair rate, A*/Δ span, shed budget) recomputes once per
   burst with a 50 ms staleness bound; the A* send-rate anchor stays
   FED per symbol. OFF ⇒ per-symbol recompute, bit-identical.
Scope, measurement-driven: **single-live-path only** (re-checked per
iteration) and **Realtime packing excluded**.

**Refuted mid-branch, removed (commits preserve the mechanisms):**
- *Engine-receiver burst drain* (± per-burst cumulative-ack coalescing,
  burst 16/64; commits 97bc6ea→8a71ed8): ANY receiver-loop drain
  collapsed c1 227.6 → 136–144 Mbit/s — echo-RTT inflation 11 → 76 ms
  (standing queue at the service-limited engine) → dynamic store cap
  growth (positive feedback) → tail-sweep/hole-refresh spurious-retx
  flood (retx ×3–6, paused 60%+, receiver CPU +80%). Isolation arms:
  sender-only 227.6; +drain 137.2; +drain−ack-coalesce 136.1 (CPUSRV
  66.4 s); burst=16 136.5. Removed (1313841).
- *Dual-path bursting* (battery rep 1, s42): c7 167 → 115, c8 88 → 52 —
  longer same-path arrival runs amplify the wall-#8 striping-gap loss
  misread (global batch serials): per-path `pl` read **0.74** at a
  2.6%-loss cell, tail recovery stretched ~4 s. Scoped out (c639d56);
  dual cells become the battery's null control. Aborted partial
  preserved (`battery-s42-ABORTED-burst64-dual.log`).

Unit tests: `emit_batch_loopback` (burst=8, many burst boundaries —
completion IS the no-symbol-loss check), gates default test. Lib suite
364/364.

### L1 BATTERY (VM 10.1.5.16; seeds 42 + 7 ×8, arms interleaved default ↔ `RWM_EMIT_BATCH=1` per rep, fresh topology per invocation, 1 run/invocation; RWM_GEN=0 RWM_DIAG=1; liveness echo asserted per arm; s42 binary 73276eca (pre dates the realtime-exclusion/staleness commits — bulk path logically identical; equivalence spot on the final binary, same seed/cell: def 201.4 / eb 225.7), s7 + tails binary 3fc50648 = 2367a51; s42 14:52–15:33 UTC (41 min), s7 16:04–16:21 (17 min, retry-hardened driver after a busy-collision abort — first s7 attempt preserved as `battery-s7-ABORTED-busycollisions.log`); logs `/home/vibe/embatch/battery-s{42,7}.log` + per-run diag)

Goodput (Mbit/s, mean ± σ_s (n); Δ = eb − def):

| cell | arm def (s42) | arm eb (s42) | def (s7) | eb (s7) | verdict |
|---|---|---|---|---|---|
| **c1 single 400 MB** | 186.2 ± 9.8 (8) | **216.2 ± 10.7 (8)** | 190.8 ± 2.6 (8) | **210.5 ± 4.7 (8)** | **+16.1% / +10.3%, Δ ≫ σ_s — per-run RANGES DISJOINT both seeds** (s42: 172.6–200.4 vs 201.4–237.6; s7: 186.6–194.1 vs 203.7–215.1) |
| sc2 single 100 MB | 85.10 ± 0.72 (8) | 85.04 ± 0.63 (8) | 84.04 ± 1.04 (7) | 85.13 ± 0.51 (8) | HOLD (wire-bound; Δ inside σ) |
| c7 dual 200 MB | 163.4 ± 3.2 (8) | 165.6 ± 2.9 (8) | 165.7 ± 2.6 (8) | 167.8 ± 2.4 (7) | HOLD (null control — emission path bit-identical at N=2 by scope) |
| c8 dual 25 MB | 67.3 ± 15.0 (8) | 74.2 ± 9.9 (8) | 80.1 ± 10.5 (8) | 80.2 ± 7.0 (8) | HOLD (Δ inside the cell's historic bimodal σ) |

**dnf = 0 in all 126 captured runs.** Seed-7 caveat class: 28 RUN-RETRY
(recovered), 2 RUN-LOST after 3 attempts (sc2-def r7, c7-eb r7 — n=7
quoted; the two stale-log liveness artifacts on those lost runs are
explained by the RUN-LOST lines, zero contamination among captured runs).

**CPU (mean s/invocation; the mechanism evidence per the corrected
profile verdict):**

| cell | CPUCLI def→eb (s42) | (s7) | CPUSRV def→eb (s42) | (s7) |
|---|---|---|---|---|
| c1 | 18.99 → **13.85** (−27%) | 17.93 → **13.54** (−24%) | 18.44 → 16.36 | 17.87 → 16.59 |
| sc2 | 6.33 → **5.40** (−15%) | 6.04 → **4.86** (−20%) | 7.41 → 7.60 | 6.91 → 6.98 |

Sender cores at c1: 1.10 → 0.94 at +16% throughput (−27% CPU/bit).
Receiver cores: 1.07 → **1.10–1.12 — SATURATED in the eb arm: the
engine-receiver service wall (~22–23k msgs/s ≈ 210–230 Mbit/s, §16.23's
recv-side wall) is the measured residual binder.** sc2: −15–20% sender
CPU at equal goodput (an efficiency win even where the wire binds).
Syscall density (in-battery 5 s samples, c1): def ~3.1–3.3k UDP
sends/s, eb ~3.8–4.1k at +16% throughput — sends/MB EQUAL, as the
corrected profile predicted (GSO already amortizes; the win is CPU).

**Tail spot (crown gate; tail_matrix c2 ×4, seed 42, both arms):**
p99 medians def {realtime 36/40 ms, tunnel-bulk 68/67} vs eb {36/39,
73/67}; **1000/1000 delivered in every rep, both arms.** Batching is
structurally inert in the tunnels (Realtime excluded by code;
tunnel-bulk rides the block sender — zero batching echoes in the arm
logs), so the single 192.6 ms rep (eb realtime-1200B r3, vs def max
77 ms this spot / historic worst-rep 164 ms) occurred on
code-identical paths = session noise, recorded. **Crown UNREGRESSED.**

### VERDICT vs the pre-registration

1. **Profile prediction (c)1 — half-refuted, honestly:** the dominant
   term IS amortizable per-send overhead, but it is control math + loop
   machinery, NOT syscall density (quinn-udp GSO was already engaged at
   7.6 segs/send; quinn's own density is the same order per byte). AEAD
   irreducibility refuted again (1.67%).
2. **c1 ≥ 400 Mbit/s — FAILED.** Measured: +10–16% both seeds with
   disjoint ranges (186–191 → 210–216). The falsification clause
   governs: the residual binder is NAMED AND MEASURED — the
   engine-receiver per-message service wall (~1.1 cores at ~22–23k
   msgs/s). Receiver-side batching was attempted in three variants and
   REFUTED with the mechanism identified (the drain destabilizes the
   ack clock: queue-delay → echo-RTT-derived store cap → spurious-retx
   spiral) — the receiver wall is not select-overhead; it is
   per-message work (estimator math, per-ack processing, locks,
   per-symbol WindowAck emission) coupled to the sender's control laws.
3. sc2/c7/c8 unregressed — as predicted (c7/c8 by construction after
   the dual-scope refutation; measured null).
4. Realtime tails unregressed; 5. dnf = 0 — both PASS.

**FLIP DECISION: NO FLIP — `RWM_EMIT_BATCH` ships DEFAULT OFF.** The
pre-registered flip rule gates on c1 ≥ 400; the measured lever is
+10–16% c1 at −25% sender CPU/bit with zero regressions — a documented,
gated opt-in, not a default. The honest per-core ceiling after this
branch: **sender ~24k sym/s/core batched (was ~19.5–20k); receiver
~22–23k msgs/s/core UNCHANGED = the system ceiling ≈ 210–230 Mbit/s
per sink.** The external bar stays quinn-bbr 915–922 on this box
(×4.3 of the batched ceiling). SUCCESSOR lever (named, NOT built): the
receiver per-message service cost — its profile terms are on record
(§16.23 receiver flat: estimator ~14%, allocator ~6%, deserialize 3%,
per-symbol ack generation ~20k control datagrams/s) and the refuted
drain family bounds the solution space: any fix must reduce
PER-MESSAGE work (or ack density) WITHOUT adding queueing between
arrival and ack emission — e.g. cheaper per-ack estimator math, ack
thinning at the PROTOCOL level (sender-negotiated, so the store law
sees honest RTT), or moving delivery off the ack path. A future
re-ask rides a fresh pre-registration.

### Tests

`cargo test -p raptorpath --lib` 364/364; `gate_suite` 15/15 release
(--test-threads 1); `mtu_blackhole_wedge` 2/2; `perf_loopback` 8/8;
`copa_sole_loopback` / `fmtcp_loopback` 1/1 release;
`unified_stream_l0` 1/1 (--ignored measurement arm); NEW
`emit_batch_loopback` 1/1 release (burst=8, ~20 burst boundaries —
completion is the no-symbol-loss/ordering check); `-p raptorpath-math`
136 green (59/19/22/4/4/3/25). Shipped path with env unset:
gate-off per-symbol recompute is code-identical (the taper cache and
burst intake are `emit_batch_live`-gated; the gates default test pins
`emit_batch=false`, `emit_burst=64`).

Ops: lock `/tmp/rwm-vm.lock` held 12:35 → released 16:41 UTC (one
crash-resume mid-session, lock refreshed 15:38; the s42 battery had
completed before the crash — nothing re-run); tree synced per
discipline 10 (CRLF converted); stale binaries removed before every
build; binaries: profile b04bc50f…, s42 battery 73276eca…, final
3fc50648… (sha256 in every log header); rp-* netns only, cleaned; all
logs + perf data preserved under `/home/vibe/embatch/`; foreground
polling only; the parallel c8-pool worker's tree untouched.

## Streaming Crown Re-Test (2026-07-27) — PRE-REGISTRATION (discipline item 11 — this block written BEFORE any measurement; branch `meas/streaming-retirement` from 44dd7d4; MEASUREMENT task, harness glue only, no transport code touched; LEVER 4's gate — the DEPRECATION REGISTER's streaming-machine re-test clause)

*Decision record: → [ADR-0064](adr/0064-unified-span-machine.md) +
[ADR-0066](adr/0066-deprecation-register.md) +
[VISION-TRIAGE-2026-07](adr/VISION-TRIAGE-2026-07.md) §4 (the retirement
path, stage 1).*

**(a) The question.** The register's streaming row: "the 12–48×
message-tail crown record spans HISTORIC cells (L2/L3 message-tail
batteries, quinn-vs-rp Metric A) this battery did not re-run; code
removal requires a later pass holding that record cell-by-cell on the
unified default." This session IS that later pass (VISION-TRIAGE stage 1:
"confirmation, not exploration" — the 2026-07-21 flip battery already
showed unified ≤ streaming at all 8 of ITS cells). Gate: the SHIPPED
DEFAULT (unified machine, env unset) must match-or-beat the streaming
machine (`RWM_UNIFIED=0`, the legacy opt-out — VERIFIED in code:
`net/mod.rs` `unified_active()` is `env_flag("RWM_UNIFIED", true)`;
`=0` + Realtime hint echoes "Realtime mode: auto-selecting streaming
backend") on p50/p99/delivered% at EVERY historic crown cell, both seeds.

**(b) The crown-cell list, as found in the ledger (era-honest provenance).**
The record's cells and their historic values (all PRE-DIVIDE qemu64,
pre-substrate-chain — walls #1/#2/#7/#8 live, stock-Cubic under, 1024
pool; the HISTORIC ABSOLUTES ARE NOT THE BAR — the hardware/wall divide
means the comparison is the SAME-DAY streaming-arm vs unified-arm ratio
per cell, plus class-consistency with the modern record):

| # | cell (this battery) | historic origin | historic streaming record (p99, ms) | why it is in the record |
|---|---|---|---|---|
| 1 | tail_matrix **c2·realtime·400B**, 50 msg/s × 20 s | "Full Benchmark Re-Run (2026-07-08)" Metric A (×5 reps, seed 42) | med **59** [42–637] vs quinn 2824 / kernel-BBR 13,400 | THE 12–48× crown cell (2824/59 ≈ 48×) |
| 2 | tail_matrix **c2·realtime·1200B**, 50 msg/s × 20 s | Metric A; L3 REGIME MAP ("12–60×") | med **145** [39–2655] vs quinn 2824 / BBR 13,400 | the crown's other half (2824/145 ≈ 19×, BBR/145 ≈ 92×) |
| 3 | tail_matrix **c3·realtime·400B**, 50 msg/s × 20 s | Metric A | med **209** [105–1409] (vs BBR 198 — rp TIED, beat quinn 1393 ~6×) | record row, not a crown win — cell-by-cell means EVERY row of the record table |
| 4 | tail_matrix **c3·realtime·1200B**, 50 msg/s × 20 s | Metric A | med **1771** [334–3154] ("worse than BBR"; the melts-adjacent cell) | record row; the modern unified class here is 92–133 — the cell the streaming-retirement gap (roadmap item 7) named |
| 5 | **c2·realtime·1200B @ 50 msg/s × 30 s** (the L2 stream_bench shape, 1500 msgs) | "L2 workstream 2 (2026-07-04)" + "quinn message-tail vs raptorpath" | p50 8.6 / p90 15.6 / **p99 513 / p999 727** / max 747 vs cubic 13,252 / bbr 13,426 (26–147×), quinn 2824 (5.5×) | the L2-era record used **p99.9** → p999 is gated on THIS shape (the only historic cell that recorded it) |
| S | tail_matrix **c2·bulk·{400,1200}B** ×4, seed 42 only | Metric A bulk rows (med 102/154) | streaming-INERT by construction (bulk hint rides the block pipeline) | the pre-registered bulk sanity spot: `RWM_UNIFIED=0` must not move the bulk tail class |

Cells the record does NOT contain, stated so the list is closed: no c4
message-tail measurement exists anywhere in the ledger (Metric A/L2/L3
never ran it — the crown claim is "C2-class loss"); c5 was measured
(L2-era) and is NOT a streaming crown cell — rp-realtime DNF'd/silent-
failed there ("no winner — both stacks break >5% loss"), so it cannot
gate retirement and is not re-run. The quinn/kernel-TCP baseline arms are
NOT re-run either: the "Competitive Baseline (2026-07-21)" battery
already re-verified the crown externally on the modern substrate against
live quinn/TCP (rp 36–39/92–103 ms vs quic 55–759 / tcp 209–3878 + del
cliffs) — ON THE UNIFIED DEFAULT; what it did not measure is the
streaming machine, which is exactly this battery's second arm.

**(c) Prediction (pre-registered).** Unified (ship) ≤ streaming per-rep
p99 medians at every cell-seed within the rep spread, delivered ≥
streaming's, p50 equal-class (~8 ms c2 / ~24–26 ms c3), p999 ≤ streaming
at cell 5. Prior strongly favorable: flip battery 8/8 rows, competitive
baseline classes. Expected modern classes: c2 rt p99 med ~36–52,
c3 rt ~90–135, both arms.

**(d) Falsification / disposition rule (pre-registered).** Any cell
where STREAMING beats unified beyond the noise floor (discipline 5: Δ
outside the larger arm's rep spread, BOTH seeds) → the register row
STAYS, that cell named as a documented (δ,ρ) point where the second
machine wins — a finding, not a failure; retirement blocked or scoped to
the cells that pass. If unified holds EVERY cell both seeds → the row
becomes RE-TESTED/CLEARED, streaming retirement is GO for the next
consolidation pass (~1,230 LOC: `fec/streaming.rs` 352 + `streaming-codes`
845 + ~30 selection glue — NOTHING deleted THIS session; the clearance +
work-list refresh are recorded). A c3-1200B unified-vs-streaming
sign-flip inside the spread (the flip battery saw ±(1–2)σ there vs rlc)
does not block — the gate is vs streaming, at the noise floor.

**Battery (pre-registered).** VM 10.1.5.16 per MEASUREMENT DISCIPLINE
1–11: priority 2 — the lossy-residual worker holds `/tmp/rwm-vm.lock`
first; ALL local prep done before touching the VM, then FOREGROUND
polite polling (2–3 min interval, elapsed stated; no stop-and-wait).
CRLF-convert after sync; rm stale binary before build; binary sha256 +
commit + lscpu in every log header; rp-* netns only; seeds 42+7; arms
`streaming` (`RWM_UNIFIED=0`) vs `ship` (env unset) INTERLEAVED PER REP
(fresh warm tunnel per rep-arm, arm order alternating per round — the
historic ×5-rep warm-tunnel protocol upgraded to the current discipline);
×8 reps/arm/seed at cells 1–4, ×5 at cell 5 (historic was n=1), ×4 at
the bulk spot (seed 42); per-rep p50/p99/p999/max/delivered scraped
(harness glue: tail_matrix.sh gains the `streaming` arm + p999/max/
count in the rep echo + rate/duration/size/hint overrides; driver
`tools/l1/crown_battery.sh`); liveness echoes asserted per arm BOTH
machines ("auto-selecting streaming backend" on every streaming rep;
"unified span law ACTIVE" + shedding + A* anchor + unified-decoder on
every ship rep, both endpoints); seed-7 topo-abort ns recorded; stage
runtimes stated; lib suite run once on the branch (transport untouched —
harness+docs only). Logs `/home/vibe/crown/`.

*(Results below this line were written after the battery ran.)*

### L1 RESULTS (VM 10.1.5.16, 2026-07-27 19:07–20:44 UTC; binary sha256 2aac6b5fd088… = commit 8cf2b6f — Rust byte-identical to main 44dd7d4, the no-transport-change branch proof (docs+harness only; local lib suite 368/368 on the branch); E5-2650 v3 aes+avx2+pclmulqdq (post-divide) in the header; seeds 42 AND 7; arms interleaved per rep (fresh warm tunnel per rep-arm, order alternating per round); drivers `tools/l1/crown_{all,battery}.sh` + tail_matrix arms; logs `/home/vibe/crown/{crown,l2shape,bulk}-s{42,7}.log` + env/run logs; lock `/tmp/rwm-vm.lock` taken 18:41 UTC (found FREE on arrival and across two checks ~10 min apart — the lossy-residual worker was not holding it), released 20:46 UTC after teardown; stage runtimes: crown-s42 36m38s, crown-s7 36m54s, l2shape 7m22s+7m24s, bulk 9m12s)*

**Incidents, recorded first (discipline 7/8).** (i) The FIRST launch died
into a bringup-collision cascade after 3 good arms (every subsequent
`topo.sh up`+ping failed, 23 s each; ~4 reps captured) — battery stopped,
log preserved (`crown-s42-ABORTED-bringups.log`, its reps NOT merged),
`run_arm` bringup retry-hardened (3 attempts, counted loudly — the
embatch "retry-hardened driver" precedent, commit 7f20057) and the
battery relaunched FROM SCRATCH. Cause unattributed honestly: the
relaunch recorded **0 BRINGUP_RETRY and 0 BRINGUP_FAIL over all 164
bringups, both seeds** — the guard was never exercised, so the cascade
was a transient VM condition, not the netns cycling rate per se.
(ii) One NO_DATA rep (ship c2-1200B s42 — summary-less, skipped datum,
n=7 quoted). (iii) The documented seed-7 topo-abort class did not appear
at all (0 aborts). Liveness: **every bringup echoed its machine at both
endpoints** — streaming arms 64/64+10/10 echo sets "Realtime mode:
auto-selecting streaming backend … backend=Streaming"; ship arms
64/64+10/10 the full unified set (span law + shedding sender/receiver +
A* anchor + unified global decoder + backend=Rlc); bulk arms 0 realtime
echoes (the block pipeline, as constructed).

**Cells 1–4 (tail_matrix, 50 msg/s × 20 s × 1000 msg/rep, ×8/arm/seed;
per-rep p99 medians [min–max]; S = streaming `RWM_UNIFIED=0`, U = ship =
env unset = the unified default):**

| cell·size | S s42 · s7 | U s42 · s7 | verdict |
|---|---|---|---|
| c2·400B (crown #1) | 36.7 [34.4–45.3] · 40.6 [36.9–57.4] | **35.4 [34.5–36.7] · 36.5 [35.3–39.1]** | U ≤ S both seeds |
| c2·1200B (crown #2) | 41.5 [38.6–57.1] · 57.5 [51.7–93.2] | **40.3 [35.9–48.0] (n=7) · 41.9 [37.0–113.3]** | U ≤ S both seeds (−1.2 / −15.6 med) |
| c3·400B (#3) | 107.2 [94.5–150.1] · 123.1 [107.1–153.2] | **104.6 [91.3–118.6] · 105.2 [88.5–112.8]** | U ≤ S both seeds |
| c3·1200B (#4) | 108.1 [90.1–122.1] · 126.1 [93.9–177.6] | **92.6 [84.3–152.1] · 99.3 [90.8–139.1]** | U ≤ S both seeds (−15.5 / −26.8 med) |

p50 equal-class everywhere (c2 7.9–8.4 ms, c3 23.7–25.8 ms, both arms
both seeds). **Delivered: 1000/1000 in EVERY captured rep, both arms,
both seeds** (127 crown reps) — at these cells even the streaming
machine sheds nothing at the summary level; the tail, not delivery,
separates the machines.

**Cell 5 (the L2 stream_bench shape: c2·1200B, 50 msg/s × 30 s × 1500
msg/rep, ×5/arm/seed — the p99.9 record cell):**

| metric | S s42 · s7 | U s42 · s7 |
|---|---|---|
| p99 med [rng] | 41.2 [40.1–68.3] · 55.8 [49.8–81.5] | **39.5 [38.4–43.3] · 43.3 [40.8–65.6]** |
| p999 med [rng] | **62.8 [58.3–111.6]** · **86.3 [84.6–335.0]** | 69.5 [60.0–117.2] · 98.5 [70.4–129.0] |
| delivered | 1500/1500 every rep | 1500/1500 every rep |

**The one direction streaming still shows, recorded at full strength:
its p999 MEDIANS are LOWER on both seeds (−6.7 / −12.2 ms).** Verdict
per the pre-registered noise-floor rule: **TIE** — the deltas sit deep
inside the per-rep spreads (s42 both arms span ~58–117; s7 U 70–129 vs
S 85–335), the sign REVERSES at the worst rep (s7: S's worst p999 is
335 ms vs U's 129 — the only >200 ms excursion in the whole battery is
STREAMING's), and U wins p99 at the same cell both seeds. This is the
diagonal layer's residual signature at the 2nd-worst-message quantile —
real enough to record, too small and too noisy to gate; named the
**cell-5 p999 WATCH** (any future tail battery re-measures it free).

**Bulk sanity spot (cell S; c2·bulk·{400,1200}B ×4, seed 42):** p99
medians U 70.1/67.3 vs S 70.2/65.5 (ranges overlap fully; the modern
tunnel-bulk class 65–80), delivered 1000/1000 every rep — `RWM_UNIFIED=0`
is INERT on the bulk-hint tail class, as constructed (block pipeline;
zero realtime-backend echoes both arms).

### VERDICT — the register clause is satisfied: RE-TESTED / CLEARED

- **The gate (unified match-or-beat streaming on p50/p99/delivered% at
  EVERY crown cell, both seeds): PASS at all five cells.** p99 medians:
  unified ≤ streaming at 10/10 cell-seeds, by −1.2 to −26.8 ms (largest
  at the c3 cells and s7-c2-1200B, smallest a statistical tie at
  s42-c2-1200B); p50 equal-class; delivered identical-complete (163/163
  captured reps full delivery, both arms).
- The pre-registered prediction (c) held including the modern classes
  (c2 rt ~35–42, c3 rt ~93–126). The p999 prediction at cell 5 held
  only as a spread-level TIE (streaming −6.7/−12.2 at the median, sub-
  noise, worst-rep sign reversed) — recorded above, does not gate.
- Era honesty: no historic ABSOLUTE was used as a bar (pre-divide walls
  + hardware divide); the comparison is same-day arm-vs-arm at the
  record's cells, and the crown itself was already externally
  re-verified on the unified default by "Competitive Baseline
  (2026-07-21)" (rp 36–39/92–103 ms vs quic/tcp 55–3878 + delivery
  cliffs).

**DISPOSITION — streaming retirement is GO (nothing deleted THIS
session).** The DEPRECATION REGISTER streaming row moves to
RE-TESTED/CLEARED (row updated in place); the next consolidation pass
may delete, per the VISION-TRIAGE stage-2 work-list: `fec/streaming.rs`
(352 LOC) + the `streaming-codes` crate (845 LOC) + the selection glue
(~30 LOC: net/mod.rs backend-selection sites, `is_streaming()`,
config.rs `FecBackend::Streaming` parse, backend_selector.rs,
fec_rate.rs `compute_streaming_params`; line refs are the c3a9d76
survey, re-locate at deletion time) ≈ **~1,230 LOC**, with
`FecBackend::Streaming` becoming a parse error with a pointer and
`RWM_UNIFIED=0` thereafter meaning "legacy RLC decoders" only. SCOPE
LIMIT, stated honestly: this battery re-tested the STREAMING clause
only — the legacy RLC decoders' own retirement condition ("unified ≥
legacy-RLC everywhere", §17.5) was NOT re-argued here (no rlc arms in
the crown record; the flip battery's c3-1200B unified-vs-rlc sign-flip
class stands unresolved), so stage 2's "take all three legacy machines
out together" option needs that separate confirmation or a scoped
streaming-only deletion. The cell-5 p999 WATCH transfers to the
deletion pass's notes: it is a property of the machine being deleted,
measured and bounded (sub-noise medians, worst-rep inverted), not a
blocker.

Ops: lock taken 18:41 → released 20:46:39 UTC after teardown; tree
synced via git archive + CRLF conversion before the first harness
invocation (discipline 10); stale binary removed before build; rp-*
netns only, deleted at teardown; VM left clean (no battery processes,
no netns, lock free); all logs preserved under `/home/vibe/crown/`
including the aborted first attempt; foreground polling only, elapsed
stated per poll.

## Visualizer Refresh (2026-07-28) — the interactive model moved to the unified-machine era (branch `feat/visualizer-unified`, from 7a3aff6; visualizer + wasm crate only, NO engine change)

The 2026-07-06-era interactive visualizer (`raptorpath-visualizer/` +
`raptorpath-wasm/`, the §16 RWM-L0 model with a hint mode-switch) now
models the CURRENT architecture. What it models:

- **The δ continuum as the centerpiece (no mode switch anywhere in the
  UI).** One log slider δ ∈ [0.005, 50] with the §12.4 δ(hint) = 0.5/ζ
  presets as buttons; a law-driven pipeline animation morphs live —
  D(δ) = min(b·RTprop, 2·RTprop), A* = clamp(rate·D, 1, W),
  M* = ceil(rate·2·RTprop/A*_q)+1 [2,32], Δ = clamp(⌈rate·J⌉, 1, 64),
  δ-honest shedding within 1−ρ = ε̂·(1−P_fec) with refused-candidates
  shown (ρ wins over δ). Formulas ported verbatim into the wasm crate
  with §16.20.3/§16.26/§12.4 citations; the RETAIN/EVICT contract is
  DERIVED from the continuum (the r*→0 Bulk limit). ρ REMAINS a live
  dial (the triangle's second corner, next to δ — a contract dial, not a
  mode): default 1 = RETAIN-until-acked; below 1 the sim gives up via
  §6.1 T_cut toward the declared target (reliability readout + given-up
  annotation restored), and the span cartoon's shed budget becomes the
  explicit 1−ρ instead of the derived §16.26 residual. The protocol
  hints remain first-class: the three presets ARE the hints, as named
  points on the (δ, ρ) dials.
- **Multipath at the current laws**: 1/2 paths + homogeneous(c7)/
  heterogeneous(c8) topology presets and per-path knobs; the wasm model
  now carries per-path RFC 9002 recovery clocks (§16.24 — first retx
  eligibility = send + own-path RTT; phantom-retx counters show what a
  global clock would have fired), the path-scaled pool (§16.19,
  512/path at sim scale, binding when cap·RTT outgrows it), and
  SACK-clocked store release with the frontier-clocked counterfactual
  gauge (§16.25). Model-era goldens re-captured (constructor-equivalence
  contract unchanged).
- **The wall chain** (§17.1 / CONSOLIDATED VERDICT) as a 9-step story
  panel; honesty footer "L0 interactive model · era 2026-07-28 · main
  7a3aff6 · models the laws of §16.20/§16.26, not the engine binary"
  plus an explicit model-vs-engine simplification table (anchors,
  b/tail interpolation between the three hint points, no CC, no grid,
  instantaneous forward flight, slot-only store).

**Formula-fidelity gate:** `test_visualizer.mjs` (run by
`build_visualizer.sh` against the BUILT single-file bundle) now asserts
≥3 hand-computed spot values per span-law formula (ζ, b, D, A*, M*, Δ,
shed budget, tail-target anchors) plus δ-continuum sim behavior
(Realtime end more FEC / Bulk end faster, both complete), store/pool
invariants (SACK ≤ frontier counterfactual; wall-#7 bind at high BDP)
and phantom-clock behavior (heterogeneous > 0, homogeneous = 0). All
green; `cargo test -p raptorpath-wasm` 31/31; `cargo test -p raptorpath
--lib` untouched-and-green (no engine change).

**UI-layer gate + the routing lesson (2026-07-28 follow-up):** the Bulk
preset initially shipped routed through a fixed 0.05 tail anchor instead
of the engine's late-is-fine law — caught by the USER, not the gates,
because the continuum tests were ordinal (rt FEC > bulk FEC) and nothing
asserted the UI→hint ROUTING: both endpoints tested, the wire between
them not (the visualizer-scale instance of MEASUREMENT DISCIPLINE rule
1). Fixed (the Bulk preset now runs the `bulk` hint verbatim; absolute
per-preset law gates added: mid-stream r = 0 exactly at ε = 5%, χ-glide
fires, ε = 10% emits tail FEC) and the hole closed structurally:
`test_visualizer_ui.mjs` — a stub-DOM harness exercising the dial→hint→
sim routing, readouts, ρ paths, span-law panel and wall chain — is now a
BUILD GATE in `build_visualizer.sh`, including a responsiveness
regression gate for the measured topology-change hang (the §16.6 hidden
single-path baseline runs were synchronous full transfers with ~W²
decode cost, seconds per Reset at derived W*; now chunked ~40 ms slices
+ memoized per config; handler returns in ~2 ms, gate < 800 ms). Stale era content purged
(hint mode switch, Bulk χ-glide copy, 14.25 burst label, Streaming/
Mettle/DAPS references in `docs/packet-flow-visualization.md`, which is
rewritten to the systematic wire + unified decode + current DIAG names,
marked descriptive).

## Adversarial Cells (B1) (2026-08-06) — PRE-REGISTRATION (discipline item 11 — this block written and committed BEFORE any cell was brought up and BEFORE any measurement; branch `meas/adversarial-cells` from f2f1c78; MEASUREMENT + HARNESS ONLY, no transport code touched; the ADR-0068 prerequisite battery — items (i)+(ii) of its "adversarial cells + measured Copa breakage" clause)

*Decision record: → [ADR-0068](adr/0068-copa-bbr-fusion.md) (the fusion whose
falsifiable targets this battery sets), [ADR-0054](adr/0054-substrate-cc-policy-bbr-default.md)
(the policy surface whose measured tradeoff this extends), [ADR-0052](adr/0052-measurement-discipline.md).*

**(a) The question.** The clean-substrate map ("Copa-Sole on Clean
Substrate", 2026-07-22) measured Copa-sole vs BBR-under only on clean
deep-buffer netem cells: bulk 0.57–0.97×, network queue ×6–18 tighter,
realtime tail parity. ADR-0068's whole case for the fusion is that BBR's
rate model matters exactly where the current rig CANNOT look: delay-noise
(jitter talks a delay-based law under capacity), shallow buffers (Copa's
1/δ queue target physically cannot park), and policers (drop-without-queue
is invisible to a delay signal). This battery builds those three cells in
the L1 harness (`tools/l1/adv_cells.sh`, netem/tbf/police inside the
existing rp-* single-path topology) and MEASURES the predicted breakage —
Copa-sole (passthrough + wire + δ(hint)) vs the shipped BBR-under default
on every cell, +compete arms where pre-registered below. If Copa-sole does
NOT break, ADR-0068 stays unbuilt (its own clause). The map is the
deliverable, not a Copa indictment: a refuted breakage prediction is the
most valuable row because it re-scopes the fusion.

**The cells** (all c2-class rate 100 mbit; data-dir GE 1.3%/50% where
noted; full recipes in `adv_cells.sh`):

| cell | mechanism | recipe (data dir → ack dir) |
|---|---|---|
| c2ctl | clean control (= topo.sh c2) | netem delay 5ms 3ms rate 100mbit GE → same, lossless |
| jit0 | jitter-family control | netem delay 20ms rate 100mbit GE → same, lossless |
| jit5/jit15/jit25 | delay-jitter dose-response, jitter both dirs, 25% correlation | netem delay 20ms {5,15,25}ms 25% rate 100mbit GE → same, lossless |
| shal8 | 8-packet bottleneck buffer (vs the ~1000-pkt deep default) | tbf 100mbit burst 15140b + CHILD netem limit 8 GE (the child holds the queue) → netem delay 10ms rate 100mbit (all propagation on the ack egress; RTprop stays c2-class 10ms) |
| pol100 | token-bucket policer, drop-WITHOUT-queue | netem delay 5ms limit 4000 no-rate (delay stage, must never drop) → ingress police 100mbit burst 16k drop on srv0 + ack netem delay 5ms rate 100mbit |

Jitter magnitudes model the aggregation/scheduling class ADR-0068 names:
WiFi A-MPDU service-burst delay variation at the 5 ms end, LTE
scheduler/HARQ-induced delay variation in the 15–25 ms class (the
cellular-measurement literature's tens-of-ms delay variability — Sprout
NSDI'13-class trace evidence; exact citations get source-verified per
ADR-0068's literature clause before any fusion build; the cells need the
CLASS, not a point estimate). HONEST MODEL NOTE (recorded before
measuring): netem jitter re-orders in-flow packets, real aggregation
mostly preserves order — the cell is strictly harsher than the modeled
class, and the transport's reorder tolerance is part of what gets
measured. Offloads (gro/gso/tso) are disabled on the shal8/pol100 veths
so the 8-pkt limit and the per-packet policer act on wire-MTU packets.

**Cell-mechanism liveness FIRST (discipline item 1 applied to the cell
itself).** Before any transport run, `adv_cells.sh validate <cell>` on all
7 cells: (i) idle ping ×30 — jit cells must show J-class mdev, shal8/
pol100 tight base RTT; (ii) iperf3 UDP overload at 120M (> the 100M
ceiling) — loss ≈ the excess for every working ceiling; (iii) ping UNDER
that load — the queue signature: c2ctl/jit (deep netem) inflate RTT
toward the ~100 ms deep-buffer class, shal8 caps RTT at base+≲1 ms
(8 × 1350 B at 100 mbit ≈ 0.9 ms), pol100 shows NO inflation at all
(drop-without-delay, THE property under test); (iv) tc -s counters. A
cell that fails validation gets its recipe fixed (harness-only) and
re-validated before its battery rows run; the validation transcript goes
in the results below.

**(b) Predictions (quantitative, per cell × arm; A = BBR-under shipped
default, B = Copa-sole passthrough, C = +RWM_COPA_COMPETE=1).**

- **P-J1 (jitter, A):** BBR is delay-noise robust — goodput ≈ flat in J:
  each jit level ≥ 0.9× its own jit0-A mean, both seeds (windowed-min
  RTprop filters jitter; BtlBw is delivery-clocked).
- **P-J2 (jitter, B):** the ADR-0068 shape — B/A falls monotonically as
  J grows: ≈ the sc2 class (0.85–0.9×) at jit0, ≤ 0.75× at jit25.
  Mechanism: jitter pollutes the wire d_q → the target rate 1/(δ·d_q)
  and the backoff both read noise as queue → under-capacity operation.
  NAMED DEFENSE (recorded now): the shipped Copa carries the §12.4
  jitter-adjusted backoff threshold (k·jitter_est headroom) that vanilla
  Copa lacks — a partial defense; if it holds, that is a REAL finding
  that re-scopes ADR-0068's delay-noise motivation (see falsification).
- **P-S1 (shal8, B):** loss-conversion — Copa's Bulk dither wants
  1/δ = 200 pkt of standing queue; the 8-pkt buffer converts the excess
  to drops: bottleneck drop fraction ≥ 3× the same-cell A arm AND ≥ 10%
  absolute; the engine's own per-path loss estimate (DIAG pl=) elevated
  in step. Derivation band: coupling cap cwnd ≤ BDP̂+2/δ ≈ 92+400 pkts
  bounds the naive storm at ~50–80% attempted-overshoot; the store's
  honest anchor-scaled caps may throttle below that — predicted measured
  band 10–60%.
- **P-S2 (shal8, B goodput):** 0.5–0.9× same-cell A (the recovery plane
  carries part of the loss; LOSS is the primary breakage metric here,
  not goodput).
- **P-S3 (shal8, A):** BBR viable — goodput ≥ 0.85× its own c2ctl-A
  mean, drop fraction single-digit-% class (ProbeBW overshoot only);
  wireQ p50 ≤ ~2 ms on BOTH arms (the buffer physically caps it — cell
  property, not controller credit).
- **P-P1 (pol100, B):** the loss storm — d_q ≈ 0 (the dq floor) forever
  says "no queue", the target 1/(δ·d_q_floor) ≫ capacity, and default-
  mode Copa has NO loss response (loss is FEC/ARQ's job, §12.1): police
  drop fraction ≥ 20% absolute and ≥ 5× the same-cell A arm; goodput
  ≤ 0.5× A; a 120 s DNF is an admissible outcome and scores as breakage.
  The delay-stage netem must show dropped=0 (cell-honesty guard).
- **P-P2 (pol100, A):** the rate model finds the ceiling — goodput
  ≥ 0.8× its own c2ctl-A mean, police drop fraction ≤ 10%.
- **P-C1 (pol100, C):** compete does NOT rescue: the §2.2 detector keys
  on d_q ≥ 0.1·(RTTmax−RTTmin) persisting 5 RTT — a policer suppresses
  exactly that signal (queue "nearly empty" every RTT) → in_compete
  never engages (cmp C-fraction ≈ 0, switches ≈ 0), goodput/loss ≈ the
  B arm within σ. What the policer needs is ADR-0068's ε̂-referenced
  loss regime, not mode switching.
- **P-C2 (shal8, C):** detection MAY fire (the standing 8-pkt queue
  never "nearly empties" under sustained overload: d_q ≈ 0.9 ms vs a
  ~0.1·oscillation threshold) — but our hint-base AIMD floor
  (1/δ ≥ 1/δ_base = 200) makes compete strictly MORE aggressive, never
  less: no rescue, ≈ B within σ (loss equal or higher). Either way the
  verdict shape is: mode switching does not buy the shallow cell.
- **P-R (realtime crown under jitter, shipped default arm):**
  tail_matrix `default` at jit15 vs the same-session clean c2 row: p99
  medians inflate by the WIRE class only (RTprop 40 ms vs 10 ms + two-way
  25%-correlated 15 ms jitter tails ⇒ wire-implied p99 floor ≈ 60–120
  ms): predict p99 median ≤ ~150 ms with delivered counts in the clean
  row's (1−ρ) class and 0 NO_DATA arms — the crown survives real jitter
  in the same order of magnitude, it does not collapse.

**(c) Falsification conditions (fixed now).**
- F-J: B/A ≥ 0.85 at every jitter level on both seeds ⇒ the delay-noise
  breakage is REFUTED for THIS Copa (the §12.4 jitter headroom defends
  it) — ADR-0068's jitter motivation must be re-scoped to whatever
  residual the dose-response shows, and the fusion loses that cell from
  its recovery list.
- F-S: shal8-B drop fraction < 3× A's or < 10% absolute on both seeds ⇒
  loss-conversion REFUTED — attribute what actually bounds the dither
  (store caps? velocity? the coupling cap?) from the DIAG gauges before
  any verdict line.
- F-P: pol100-B drop fraction < 5× A's OR goodput ≥ 0.8× A ⇒ policer
  starvation REFUTED — the single most valuable possible row (it would
  name a Copa-side mechanism ADR-0068's analysis missed); same
  attribution duty.
- F-C: a compete arm improving goodput or cutting loss by ≥ σ vs B on
  either cell ⇒ the mode switch has measurable value and the
  no-mode-switch story must carry that number honestly.
- F-R: jit15 crown p99 median ≥ ~250 ms (an order above the wire-implied
  floor) or delivery collapse/NO_DATA ⇒ the crown does NOT survive
  real jitter; the §16.31/§17.9 crown claims gain that external-validity
  caveat verbatim.
- Verdict arithmetic per discipline 5: every claimed delta must exceed
  the per-arm σ_s and the same-session c2ctl drift.

**(d) Derivation re-read — self-contained failure predictions, named
before measuring.** (1) The recovery plane can MASK breakage in goodput
terms (plain-mode ARQ+FEC carries loss) — that is why loss-rate, DIAG
pl=, and queue distributions are primary metrics beside goodput. (2) The
§12.4 jitter-adjusted threshold is a real, already-shipped defense on the
jitter cell — named in P-J2/F-J; a hold is a finding, not a wasted cell.
(3) The store's honest per-path caps (anchor-scaled ≈ 2–3×BDP) may bound
the policer/shallow storm below the coupling-cap math — the DIAG
cap/sout gauges attribute this; a bounded-but-large storm still
confirms. (4) 25 MB objects put ~seconds-scale transfers on these cells —
warmup is perf's built-in object, and all ratios are same-session
same-size; the c2ctl-25MB absolutes will sit below the 100 MB sc2 record
(85/76 Mbit) by construction — RATIOS carry the map, not absolutes.
(5) netem jitter's in-flow reordering makes jit cells harsher than the
modeled aggregation class (recorded above). (6) A 100 mbit policer with
16k burst passes ≈ line rate for conformant pacing — if BOTH arms sail
through undropped, the cell (burst too big for the probe pattern), not
the controllers, is the first suspect; the validation stage exists to
catch exactly this before the battery.

**Battery (pre-registered).** VM 10.1.5.16 per MEASUREMENT DISCIPLINE
1–10 (A1 worker holds `/tmp/rwm-vm.lock` first: ALL local work done
before polling, then FOREGROUND polite polling at 2–3 min intervals with
elapsed stated; tree synced via git archive of THIS branch + CRLF
conversion before the first harness invocation; stale binary removed
before the fresh build; binary sha256 + commit + lscpu + kernel in every
log header; rp-* netns only, never ens18/sshd/firewall; fresh cell +
fresh tunnel per invocation; interleaved round-robin per rep; seed-7
topo-ping double-abort protocol with per-arm n recorded; logs preserved
under `/home/vibe/advcells/`; lock released after teardown +
cleanup.sh). Driver `tools/l1/adv_battery.sh` (+ `adv_cells.sh`,
`tail_matrix.sh` with the new `RWM_TM_TOPO` glue): 25 MB × 1 run/
invocation, `RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=120`, seeds 42+7 —
arms A/B on c2ctl + jit0/5/15/25 (×5 reps/level, dose-response), arms
A/B/C on shal8 + pol100 (×8 reps), per-run ADVRESULT rows (goodput,
wireQ/appQ p50/p90, DIAG pl_max/retx, cmp counters, and the CELL-TRUTH
tc -s bottleneck sent/dropped incl. the police stats and the pol100
delay-stage zero-drop guard), then the two tail rows (`default` arm,
c2-clean + jit15, 8 reps × {400,1200} B, both seeds). CC liveness echoes
asserted per arm (BBR echo vs engine-owned + feed ACTIVE + copa_wire/
delta/cc_pace/compete echoes); an arm with zero captured rows fails
loudly (ARMCOUNT). NO flips are gated on this battery; no engine change
ships from this branch (suites run once to prove the tree untouched).

*(Results below this line were written after the battery ran.)*

### Cell-mechanism validation (VM 10.1.5.16, 2026-08-06 ~14:14–14:21 UTC + exclusive-lock re-run 15:44–15:48 UTC; kernel 7.0.14-101.fc43, E5-2650 v3 aes+avx2+pclmulqdq; logs `/home/vibe/advcells/validate.log` + `validate2.log`)

Every cell expressed its adversarial property BEFORE any transport run,
and the full 7-cell suite was re-run under a verified-exclusive lock
(ps snapshot: no foreign workload) after the co-tenancy correction —
both passes agree within noise:

| cell | idle ping (30×) mdev | UDP 120M overload loss | ping UNDER load (the queue signature) |
|---|---|---|---|
| c2ctl | 2.3 · 2.4 ms (3 ms netem jitter) | 18.8% ≈ excess | **RTT → 104 · 74–107 ms (deep-buffer bloat)** |
| jit0 | 0.12 · 0.04 ms | 18.8% | → ~100 ms (deep, no jitter) |
| jit5 | 4.8 · 4.2 ms | 18.9% | → ~120 ms |
| jit15 | 13.0 · 15.5 ms | 19.0% | → ~77–108 ms |
| jit25 | 21.4 · 20.0 ms (min 0.07 ms = netem's negative-delay clamp; heavy in-flow reorder, recorded) | 19.0% | → ~90 ms |
| shal8 | 0.02 · 0.04 ms | 19.6% | **10.5 ms = base + 0.4 ms — the 8-pkt cap holds (vs +~93 ms deep)** |
| pol100 | 0.05 · 0.02 ms | 18.8% | **10.07 ms = ZERO inflation at 18.8% loss — drop-WITHOUT-queue, verbatim** |

The jitter dose-response is monotone in mdev (0.1 → 4.8 → 13 → 21 ms ≈
J·0.85), the shallow buffer caps the standing queue at its arithmetic
value (8 × 1350 B ≈ 0.9 ms + tbf 1.2 ms latency), and the policer drops
the exact excess with no delay signal at all. The cells are real.

### L1 battery RESULTS (VM 10.1.5.16, 2026-08-06 14:22–15:38 UTC; binary sha256 01001268fee62fff… = commit 6c8c3d3's Rust (harness-only db501c8 on top), SAME binary every arm; E5-2650 v3 aes+avx2+pclmulqdq, kernel 7.0.14-101.fc43 in every log header; 25 MB × 1 run/invocation, arms interleaved round-robin per rep, fresh cell + fresh tunnel per invocation, seeds 42 AND 7, `RWM_GEN=0 RWM_DIAG=1 RWM_PERF_TIMEOUT_S=120` everywhere; driver `tools/l1/adv_battery.sh`; logs `/home/vibe/advcells/battery-s{42,7}.log` + 366 per-run client/server/qdisc files under `diag/`; runtimes 36m43s (s42) + 37m00s (s7); lock 14:08:35–15:43:35 UTC)

**Liveness / honesty (discipline 1, 6–8).** 98/98 invocations captured
per seed; **0 topo-ping aborts on BOTH seeds** (the seed-7 abort class
did not fire on these cells; n = 5/5 jitter/control arms, 8/8
shal/pol arms, every arm, both seeds), 0 DNF, 0 ARM-LIVENESS-FAIL,
0 ARM-CONTAMINATION (stale-log hygiene: both endpoint logs removed
before every run), 0 ARM-VANISHED. Every A run carries the BBR echo,
every B/C run `engine-owned` + `feed ACTIVE` + `copa_wire=true
delta=0.005 cc_pace=true` + `compete=false/true` per arm. Cell truth
recorded per run from tc -s: the pol100 delay-stage netem shows
**dropped 0 on all 64 policer invocations** (the policer, not the delay
line, did every drop).

**THE MAP — throughput (Mbit/s, mean ± σ_s, s42 · s7; B/A = copa/bbr):**

| cell | A = BBR-under s42 · s7 | B = Copa-sole s42 · s7 | B/A s42 · s7 |
|---|---|---|---|
| c2ctl | 81.3 ± 2.6 · 76.9 ± 2.1 | 74.1 ± 1.3 · 71.4 ± 1.2 | 0.91 · 0.93 |
| jit0 | 79.2 ± 2.9 · 75.7 ± 3.7 | 29.9 ± 0.6 · 29.0 ± 1.2 | **0.38 · 0.38** |
| jit5 | 77.7 ± 4.4 · 80.4 ± 1.5 | 27.6 ± 0.4 · 26.0 ± 0.6 | **0.36 · 0.32** |
| jit15 | 75.8 ± 4.8 · 69.0 ± 8.0 | 24.2 ± 0.2 · 23.6 ± 0.4 | **0.32 · 0.34** |
| jit25 | 66.6 ± 9.5 · 69.7 ± 3.7 | 20.5 ± 0.3 · 20.0 ± 0.3 | **0.31 · 0.29** |
| shal8 | **9.8 ± 0.7 · 10.0 ± 1.1** | **75.3 ± 3.0 · 78.8 ± 1.8** | **7.68 · 7.87 (INVERTED)** |
| pol100 | 8.1 ± 0.1 · 8.0 ± 0.4 | 8.0 ± 0.2 · 8.0 ± 0.3 | 0.99 · 1.00 |

Compete arms (C): shal8 75.9 ± 2.1 · 76.8 ± 5.0 (C/B 1.01 · 0.97),
pol100 7.9 ± 0.2 · 8.0 ± 0.2 (C/B 0.99 · 1.00) — compete never moved a
cell by ≥ σ.

**Loss / queue profile (bottleneck drop% from tc -s; DIAG retx + engine
loss estimate pl; wireQ p50 ms; s42 · s7 medians):**

| cell·arm | bottleneck drop % | retx | pl_max | wireQ p50 |
|---|---|---|---|---|
| shal8-A | **7.26 · 7.35** | ~17 k | 0.08–0.09 | 0 |
| shal8-B | 1.53 · 1.49 | ~2.1 k | 0.02 | 4 |
| pol100-A | 3.83 · 3.86 | ~21 k | 0.12–0.13 | 0 |
| pol100-B | 3.79 · 3.88 | ~22 k | 0.13–0.20 | 0 |
| c2ctl-A / B | 0.45–0.58 / 0.35–0.37 | ~1 k / ~1.1 k | ~0.001 / 0.000 | 90 / 5–6 |
| jit0–25-A | 0.5–0.9 | ~0.7 k | ≤ 0.008 | 66 → 42 |
| jit0–25-B | 0.3–0.5 | ~1.0–1.5 k | ≤ 0.055 | 0 → 10 |

**Realtime crown rows (tail_matrix `default` = shipped machine, 50 msg/s
× 20 s × 1000 msg/rep, n=8/arm/seed; per-rep p99 medians [min–max]):**

| row | 400 B s42 · s7 | 1200 B s42 · s7 |
|---|---|---|
| c2 clean (same-session control) | 36 [35–42] · 36 [35–58] | 39 [38–58] · 37 [34–44] |
| jit15 | **95 [91–110] · 96 [91–105]** | **92 [86–101] · 94 [82–590]** |

n=1000 delivered on every rep, both cells, both seeds; 0 NO_DATA, 0
bringup failures (one s7 1200 B rep carried a 590 ms p99 tail max —
recorded, within the [min–max] shown).

### VERDICTS vs the pre-registered predictions — the map, row by row

1. **P-J1 (BBR delay-noise robust): CONFIRMED at jit5/jit15**
   (0.98/0.96× jit0 s42, 1.06/0.91 s7), **PARTIAL at jit25** (0.84×
   s42 vs 0.92× s7, σ up to 9.5 — a mild, seed-inconsistent sag, no
   collapse). The rate model holds its class under aggregation-grade
   jitter.
2. **P-J2 (Copa jitter dose-response): SHAPE CONFIRMED, BASE REFUTED.**
   Copa's absolute goodput decays STRICTLY MONOTONICALLY with J on both
   seeds (29.9→27.6→24.2→20.5 s42; 29.0→26.0→23.6→20.0 s7; every step
   ≫ σ ≤ 1.2) — the delay-noise dose-response ADR-0068 predicts is
   real, −31% from jit0 to jit25. But the predicted base (0.85–0.9× at
   jit0) measured **0.38×**: the DOMINANT aggregation-cell breakage is
   the RTprop scaling 10→40 ms, present with ZERO jitter. Attribution
   (DIAG gauges, per the pre-registered duty): `sinfl=sout=1024`
   pinned, `win=1024/1024` full, sender `paused` — the 1024-slot
   outstanding pool × the measured app-echo dwell (~250–350 ms vs
   BBR's ~68 ms) is a Little's-law ceiling 1024·1198 B/dwell ≈ 36 Mbit,
   and Copa sits AT it (30). Copa's empty pipe pays a full recovery
   round of frontier stall per GE hole at 40 ms RTprop; BBR's ~60 ms
   standing queue hides repair latency. A CC×store interaction, NOT the
   pure delay-law failure the ADR predicted — and NOT visible on any
   clean cell (sc3's 40 ms RTprop sat at 20 mbit, below the store
   ceiling; this cell is 100 mbit at 40 ms).
3. **P-S1/S2/S3 (shallow buffer): REFUTED — AND INVERTED. The
   headline row of the battery.** Copa does NOT loss-convert: it holds
   its FULL clean class at an 8-packet buffer (75.3/78.8 ≈ its own
   c2ctl 74.1/71.4; drops 1.5% ≈ GE + residual; wq 4 ms; sane engine
   anchor btlbw ≈ 10.7–13.6 k sym/s ≈ the link). It is **BBR that
   loss-converts and collapses**: 9.8/10.0 Mbit (0.12× its c2ctl),
   7.3% sustained drops, retx ~17 k on a ~21 k-symbol object, and the
   named mechanism from the gauges: the engine's delivery-rate anchor
   reads **btlbw ≈ 108 k sym/s ≈ 10× the link** under the BBR arm —
   token-bucket dequeue at line-rate 15 KB bursts quantizes delivery
   into microbursts that poison a max-filter rate model, which then
   sustains 1.25×-class overshoot into an 8-packet buffer forever
   (the documented BBRv1-class shallow-buffer pathology, reproduced
   here on quinn's BBRv1-class controller; under Copa the buffer stays
   empty, dequeues are token-paced, and the same estimator reads the
   true link). Copa's 1/δ = 200-packet queue TARGET never converts to
   loss because its equilibrium settles UNDER the ceiling — the
   fairness-irrelevant single-flow case the derivation re-read did not
   grant it.
4. **P-P1/P-P2 (policer): BOTH REFUTED, in the most informative
   direction — the starvation is CC-INDEPENDENT.** No Copa loss storm
   (3.8% police drops, not ≥20%) and no BBR survival (8.1 Mbit =
   0.10× its c2ctl, not ≥0.8×): **both controllers pin at 8.0 ± 0.4
   Mbit** (B/A 0.99 · 1.00), identical drop fractions, identical zero
   wire queue, retx ~21–22 k. The token-exhaustion drops arrive as
   BURSTS (16 KB bucket ⇒ runs of consecutive losses), and each burst
   stalls the cumulative-frontier/recovery pipeline for ≥ a recovery
   round — the same CC-independent binder family as the 2026-07-19
   clean-contention starvation (share 0.023, "Copa Competitive Mode +
   Cross-Traffic"). The policer cell does not distinguish the
   controllers because the wall is BEHIND both of them.
5. **P-C1/P-C2 (compete arms): CONFIRMED — mode switching buys
   nothing here.** pol100: the §2.2 detector never engages (cmp
   C-fraction 0.00 on 15 of 16 compete runs; one transient
   single-switch on s7 rep 5, C-fraction 0.01, self-corrected per the
   paper's own hysteresis) — exactly the pre-registered mechanism: a
   policer suppresses the queue signal the detector keys on. shal8:
   the detector did NOT fire at all (consistent with Copa running
   UNDER the ceiling — the queue "nearly empties" every 5 RTT); C/B
   0.97–1.01 everywhere. F-C not triggered.
6. **P-R (crown under jitter): CONFIRMED.** p99 medians 95/92 (s42) ·
   96/94 (s7) vs clean 36/39 · 36/37 — inside the pre-registered
   wire-implied 60–150 ms class (RTprop 40 ms + two-way 15 ms
   correlated jitter tails), ×2.4–2.6 the clean row, NOT the ≥250 ms
   collapse class; every rep delivered n=1000 (no shed collapse). The
   12–48× tail crown's external validity survives aggregation-class
   jitter: the tail inflates with the wire, not with the machine.

### What ADR-0068's fusion must now beat (the measured targets)

- **c2ctl:** the standing clean target, unchanged — close 0.91–0.93×
  to parity while keeping Copa's wireQ class (5–6 ms vs 90).
- **jit0–jit25 (the hard row):** BBR-under holds 66–80 Mbit where
  Copa-sole holds 20–30; the fusion must reach ≥ 0.9× BBR-under
  ACROSS the dose-response — but the named binder is the store-dwell
  × empty-pipe interaction, so a rate-model feed-forward alone is NOT
  predicted sufficient: the fusion (or its prerequisite) must keep
  the pipe fed across GE-hole recovery rounds WITHOUT BBR's 42–66 ms
  standing queue. That is the actual mechanism bar this battery sets.
- **shal8:** the target is now **Copa's own 75–79 Mbit** — the δ
  outer law already owns this cell; the fusion's rate-model baseline
  must NOT import the measured poisoning (max-filter btlbw ≈ 10× link
  under burst-quantized delivery). Any fusion arm that regresses
  shal8 below the Copa class falsifies the fusion on its OWN
  motivating cell.
- **pol100:** the target moves OFF the CC surface: both controllers
  = 8 Mbit means the ε̂-referenced bounded-loss regime CANNOT show its
  value here until the burst-loss recovery pipeline stops binding
  first. A recovery-plane fix is a named PREREQUISITE for the
  policer cell to become CC-discriminating at all.
- **Context re-scope (honest):** two of ADR-0068's three "BBR
  structural advantage" cells came from deployed-BBR literature about
  BBRv2-class mechanisms; the SHIPPED BBR-under arm is BBRv1-class
  and does NOT deliver them — it loses shal8 ×7.7–7.9 to Copa and
  ties Copa's starvation at pol100. The fusion's case is now: keep
  Copa's shal8/queue/tail class, add jitter-cell robustness WITHOUT
  the standing queue, and gate the policer on the recovery plane.

**Suites (tree untouched proven):** `cargo test -p raptorpath --lib`
362/362 (0 failed, 2 ignored) on this branch — no engine change; the
battery binary is commit 6c8c3d3's Rust verbatim.

**Ops + co-tenancy record.** Lock `/tmp/rwm-vm.lock` was FREE (file
absent) at 14:08:18 UTC and taken atomically (noclobber) at 14:08:35;
the ARC-A1 worker (`c8conv-agent`) arrived 14:18:05, found the lock
held, and queued (`/tmp/rwm-vm.queue`, left in place at release). Per
the mid-session co-tenancy correction: full timestamped activity list
(UTC) — 14:09 tree cleared + git-archive sync + dos2unix (discipline
10), 14:10–14:13:40 fresh build (stale binary rm'd first), ~14:14–14:21
cell validation + one pol100 smoke run, 14:22:28–14:59:11 battery s42,
15:00:51–15:37:51 battery s7, 15:40:30–~15:43 EXCLUSIVE-LOCK
re-validation of all 7 cells (ps-snapshot-verified no foreign workload;
every signature reproduced within noise — recorded in `validate2.log`),
then cleanup.sh + rp-* netns/process verification, lock released
~15:43:35 — and the queued A1 worker took it at 15:43:43 (clean 8-s
handoff, verified from its lock stamp; no overlap at either end of the
session). No A1 files, processes, or builds appeared on the VM during
the measurement window (filesystem mtime sweep recorded); the
14:14–14:21 validation window overlaps A1's 14:18 arrival by ~3 min,
which is why it was re-run under verified exclusivity. rp-* namespaces
only; logs + per-run diag preserved under `/home/vibe/advcells/`;
foreground polling only.

## Receiver Per-Message Wall (2026-08-06) — PRE-REGISTRATION (discipline item 11 — this block written and committed BEFORE any VM run and BEFORE any build; branch `feat/recv-permsg` from 48f60c4; ARC A item 3 — the ×4-to-quinn c1 gap's named binder: the engine-receiver per-message service wall, "Emission Batching"'s pre-named successor)

**(a) The question.** The "Emission Batching" verdict named its residual
binder with numbers: after the sender batched (+10–16% c1, sender cores
1.10 → 0.94), the eb arm SATURATED the engine receiver (~1.10–1.12
cores) at its ~22–23k msgs/s per-message service wall ≈ 210–230
Mbit/sink — §16.23's recv-side wall, unchanged through three sessions.
The external bar is quinn-bbr 915–922 on the same box (×4.3 of the
batched ceiling). BUT: every wall number above is v4-era. The v5
compact DATA framing flipped DEFAULT ON 2026-08-06 ("Window Decoupling
+ MTU Scaling" part 2: per-packet overhead 119 → ~71 B, hand-rolled
varint parse instead of bincode) — parse/serialize and wire density
both changed, so the wall may have MOVED. PROFILE-FIRST, and
RE-BASELINE before the profile targets anything.

**(b) Solution-space bounds (inherited, BINDING).** The refuted drain
family (embatch receiver arm, commits 97bc6ea→8a71ed8, removed 1313841)
bounds every candidate: ANY queueing between datagram arrival and ack
emission collapsed c1 227.6 → 136–144 (echo-RTT 11 → 76 ms → dynamic
store-cap growth → spurious tail-sweep/hole-refresh retx flood, ×3–6).
Threading is refuted ×3 (§16.23; thread-count is not the wall — 1+1
pinned cores sustain the operating point). The admissible space:
reduce PER-MESSAGE / PER-ACK work, or carry more bytes per message —
NEVER delay the ack clock behind a queue.

**(c) STEP 0 — re-baseline on v5 (before the profile).** c1 single
400 MB, arms def ↔ def+`RWM_EMIT_BATCH=1` interleaved ×4, seeds 42+7;
engine-sink probes single-c1 + dual-c1 (400 MB, `RWM_RDIAG=1`, the
§16.23 methodology) ×2 per config. Record goodput, msgs/s, engine
busy%, q depth, CPUSRV/CPUCLI. Prediction R0 (pre-registered): the v5
frames move the receiver wall ≤ ~10% (deserialize was ~3%/core of the
v4 receiver flat and serialize 3.8%/core of the sender's; the
estimator/ack-generation/lock terms are v5-invariant), so the eb arm
re-baselines in the ~210–240 band with the receiver still the
saturated side (~1.1 cores at ~22–25k msgs/s). If instead the wall
moved ≥ ~15%, the profile targets the NEW wall and R0's failure is
recorded as a v5 datum, not a defect.

**(d) STEP 1 — profile the receiver core-second AT the wall.**
`perf record -F 397 -g` + `strace -c` on the SERVER (bulk receiver)
at the re-baselined wall (c1, 1.2 GB, the faster arm from STEP 0);
same-run sender side for the binds-first question (#84 had send-side
first at 19.5–20k sym/s; embatch moved sender to ~24k — which side
binds on v5?). Attribution table (µs/msg × rate = cores), terms fixed
in advance: (i) datagram recv + parse (quinn read_datagram → v5
deserialize + per-symbol Vec alloc), (ii) per-symbol bookkeeping
(decoder add_symbol, received_seqs BTreeSet, reorder/frontier walk),
(iii) ack generation (received_sack_ranges BTreeSet walk + serialize +
send_control_datagram, × R_ack ≈ one ack per in-order data symbol
~20k/s + the 2 ms gap-ack cadence), (iv) estimator/control math
(record_arrival, jitter, scheduler-lock family — the §16.23 receiver
flat had estimator ~14%), (v) delivery/inject hand-off, (vi)
allocator, (vii) select/loop re-arm (deadline recompute + 2 scheduler
locks per wake). Reference row, same box: quinn-perf SERVER (receiver
of the 915-Mbit upload) — CPU cores, recvmsg calls/s, datagrams/call
(GRO was ~13 dg/recv in the embatch profile), µs per DATAGRAM — what
does the reference stack pay per message for recv+parse+ack with NO
reassembly/FEC/estimator obligations? The dominant rp term is then
named with numbers, and the quinn row prices which part of our
per-message cost is FEATURE (reassembly/FEC/frontier — quinn simply
does not do it) vs OVERHEAD (locks, allocation, per-ack density,
per-message wakeups — quinn does the same job cheaper).

**(e) STEP 2 — candidate space (build ONLY what the profile names;
per-part pre-registrations with predicted c1 bands appended BELOW as
the amendment, BEFORE any build — the winmtu pattern).** Bounded by
(b): **(A)** batched datagram RECV — process N already-arrived
datagrams per wakeup (recvmmsg/GRO-class intake amortizing select/
lock/deadline overhead) with the ack clock PER-BATCH-IMMEDIATE (acks
emitted before awaiting again; the drain refutation forbids holding
acks across wakeups, not batching arrivals that are already
simultaneous — the gauge gate in (f)1 decides); **(B)** cheaper
per-symbol bookkeeping — only structures the profile names ≥ ~5%/core;
**(C)** ack cadence/aggregation at the PROTOCOL level IFF per-ack cost
(iii on the receiver + the sender's per-WindowAck record_batch) is the
dominant term — any cadence change must show echo-RTT and the store
gauges UNMOVED (the #85-class falsification) and SACK-release/recovery
clocks unaffected; **(D)** compose `RWM_EMIT_BATCH` (sender) iff STEP 1
shows the sender binds first on v5. Anything the profile does not name
is NOT built (discipline 11d).

**(f) Falsification (fixed now).** (1) Any receiver-path change whose
arm shows echo-RTT inflated ≥ ~2× the def arm's class or the dyn
store-cap gauge growing ⇒ the drain-family mechanism reappeared —
refuted, removed, register row, NO tuning pass. (2) If the profile
attributes the dominant receiver term to irreducible FEATURE work
(reassembly/FEC/frontier bookkeeping quinn does not do), the honest
deliverable is the measured ceiling WITH that attribution — nothing is
built past what the profile justifies, and the c1-vs-quinn row is
re-priced as a feature cost, not an inefficiency. (3) Per-part c1
bands are pre-registered in the amendment before build; the session
bar is the WALL VISIBLY MOVING (engine-sink probes before/after ≫ σ_s,
msgs/s wall up) with honest distance-to-915 stated — a part whose band
fails goes to the register per discipline 11. (4) CROWN GATE
(mandatory): tail_matrix c2 spot ×4 — p99 medians in the historic
~36–48 ms class, 1000/1000 delivered, both arms; ANY regression blocks
a flip regardless of throughput (receiver changes touch the delivery
path). (5) c7 dual ≥ 0.97× same-session Σ and sc2/sc3 unregressed ≫σ;
dnf = 0 everywhere.

**(g) Battery (pre-registered).** VM 10.1.5.16 per MEASUREMENT
DISCIPLINE 1–12: lock `/tmp/rwm-vm.lock` (taken 2026-08-06 21:04:20
UTC, found FREE — covers ALL VM activity incl. builds/probes); tree
synced via git archive of THIS branch + CRLF conversion before first
harness invocation; stale binary removed before every build; binary
sha256 + commit + lscpu + kernel in every log header; FOREGROUND
polling only; rp-* netns only; fresh topology per invocation; seeds
42+7, ×8/arm interleaved round-robin per rep (probes/profile ×2–×4 as
stated); seed-7 topo-abort protocol (n recorded, nothing discarded);
liveness echoes asserted per arm both directions (new gates get their
own ACTIVE echo); ARMCOUNT per arm; runtimes stated; logs under
`/home/vibe/recvwall/`. Cells: c1 PRIMARY (single 400 MB) def ↔ each
pre-registered part ↔ composed; sc2 100 MB + sc3 25 MB
(no-regression); c7 dual 200 MB (≥ 0.97×Σ from same-session singles);
tail_matrix c2 spot ×4 seed 42 (crown); engine-sink probes
before/after. Drivers `tools/l1/recvwall_*.sh` (+
`RWM_EMIT_BATCH`/`RWM_EMIT_BURST` forwarding added to `perf_rwm_c.sh`
— harness glue the embatch session kept VM-local).

*(STEP 0/1 results, the amendment, and the battery results below this
line were written AFTER the respective runs.)*

### STEP 0 — RE-BASELINE RESULTS (VM 10.1.5.16, 2026-08-06 21:17–21:31 UTC; binary sha256 6bb6ca96… = commit 1ce8ba2 (v5 compact default ON), built fresh (stale rm'd, CRLF-converted); E5-2650 v3 aes+avx2+pclmulqdq; kernel 7.0.14-101.fc43; driver `recvwall_baseline.sh`, seeds 42+7, c1 arms ×4 interleaved + sink probes ×2/config; 32/32 runs clean, 0 retries, dnf = 0; logs `/home/vibe/recvwall/baseline-s{42,7}.log` + per-run diag)

| arm (c1 single 400 MB) | s42 (n=4) | s7 (n=4) | v4-era reference |
|---|---|---|---|
| def (v5 shipped) | 200.8 (198.2–204.4) | 199.7 (187.9–205.6) | 186.2/190.8 ("Emission Batching") |
| eb (`RWM_EMIT_BATCH=1`) | 217.8 (209.1–223.8) | 223.0 (215.8–233.2) | 216.2/210.5 |

CPU: def CPUSRV 16.5–17.6 s ≈ CPUCLI 16.0–16.9 s (both ~1.03–1.05
cores); eb CPUSRV ~16.1 s at 14.5 s wall = **1.10 cores receiver** vs
CPUCLI 12.7–13.5 (0.88–0.93) — the receiver is the saturated side in
the eb arm, exactly the "Emission Batching" verdict shape. RDIAG:
def sc1 busy 83–85% at 19.4–23k msgs/s; eb sc1 busy 73–80% at 20–23k;
dual-c1 eb bursts 25–32k msgs/s serviced (busy 78–89%, q ≤ 658 of
4096) — one dc1-eb probe run banked **277.2 Mbit/s**, and the STEP-1
1.2 GB profile run sustained **267.1 Mbit/s single-path** (the longer
steady state amortizes warm-up/ramp; the 400 MB numbers carry ~2 s of
ramp). **Prediction R0 verdict: HOLDS** — v5 lifted def ~+7% (186–191
→ ~200) and eb ~+3% (210–216 → 218–223 at 400 MB), inside the ≤ ~10%
band; the receiver remains the binder at ~1.1 cores. The wall's honest
v5 position: **~22–26k msgs/s ≈ 220–270 Mbit/sink depending on
transfer length** (object-scale ramp is a visible share at 400 MB).

### STEP 1 — PROFILE RESULTS (same session, 21:32–21:44 UTC; `recvwall_profile.sh` phases rp-perf-srv / rp-strace-srv (eb arm, c1 1.2 GB, run read 267.1 Mbit/s, CPUSRV 44.5 s / CPUCLI 39.1 s ≈ 1.24 / 1.09 cores) + quinn-recv; logs `/home/vibe/recvwall/profile-*.log`, perf data preserved)

**Receiver flat (perf -F 397 -g, 15 s at the wall):** the dominant
family is `LossEstimator::record_batch` + its inlined
`BayesianChangepoint::update` — **record_batch self 7.8 +
__ieee754_exp_fma 7.75 + __ieee754_log_fma 4.86 + exp@glibc 2.01 ≈
22.4%/core**. Then allocator (_int_malloc 3.80 + malloc_consolidate
1.13 + realloc family — the BOCD update allocates TWO Vec(201) per
call), engine-loop closure 3.08, AEAD 2.21 (noise, wall-#5 again),
memmove 2.18, spin-lock slowpath 1.09. v5 deserialize does NOT appear
≥ 0.4% (the compact parse is cheap — part of why def moved +7%).

**Sender flat (same run):** the SAME family, larger — record_batch
8.28 + exp 9.39 + log 5.64 + exp@glibc 2.63 ≈ **25.9%/core** — the
sender runs `estimator.record_batch` per received legacy Ack
(`handle_control_message` Ack arm, ~one Ack per Data batch ≈ 20k+/s).
`run_window_sender` closure 2.88, handle_control_message 1.87.

**The mechanism, named:** the receiver sends a legacy block-era
`ControlMessage::Ack` for EVERY Data message (both modes, net/mod.rs
~3480–3514) and calls `estimator.record_batch(expected, received)`
under the scheduler lock per message; the sender processes that Ack
through the same `record_batch`. Inside: EWMA + Beta + burst flag
(cheap) + GE record_symbol (O(1), cheap) + **`BayesianChangepoint::
update` — O(MAX_RUN_LENGTH = 200) with ~2 ln + 1 exp per run length,
two Vec allocations, and a 200-entry stats-shift clone, PER CALL**.
changepoint.rs's own header says "Cost: O(MAX_RUN_LENGTH) per update —
negligible" and `default_fec()` documents the design cadence: "expect
regime changes every ~100 batches (**200 s at 2 s intervals**)". The
window wire calls it at ~22 kHz — ~4.4 M transcendentals + ~44k Vec
allocs per second per side. This is a mis-scaled CONTROL-PLANE cadence
(the sixth control-plane wall in a row), not irreducible per-message
feature work.

**quinn reference row (same box, quinn-perf server = receiver of the
BBR upload at 923.5 Mbit/s wire-946):** 0.455 cores for ~90k wire
pkts/s ≈ **5.1 µs/QUIC-packet**, flat profile AEAD-dominated (aesni
9.42, memmove 4.16, kernel copy 2.71) — BYTE costs, control ~free:
recvmmsg 3.7k calls/s (~24 wire pkts/call via GRO), **ack sends 3.7k/s
≈ 1 ack per ~24 data packets**. rp's receiver at the wall: **~48
µs/message** (1.24 cores / 26k msgs/s), of which ~10.6 µs is the BOCD
family, ~2.3 µs allocator, ~1.1 µs AEAD — and rp emits ~2 control
datagrams per data message (legacy Ack + WindowAck ≈ 40k control
sends/s across both directions vs quinn's 3.7k). Feature-vs-overhead
split per (f)2: reassembly/decoder/frontier bookkeeping is < ~3%/core
at c1 (decoder add_symbol did not chart ≥ 0.4%) — the gap to quinn is
NOT the FEC feature's cost at this cell; it is per-message control
overhead (estimator cadence, dual-ack density, per-message wakeups).

### AMENDMENT — what the profile names, pre-registered BEFORE the build (discipline 11)

**PART 1 (the ONLY ≥ 5%/core overhead term): `RWM_EST_CADENCE`
(default OFF, A/B) — restore the BOCD changepoint detector to its
design cadence.** In `LossEstimator::record_counts`: accumulate
(sent, received) and flush `bocd.update()` with the ACCUMULATED counts
when (a) this call carries a loss (`lost > 0` — zero staleness on the
informative observations; the BOCD sees every loss event at the same
clock it does today), or (b) ≥ 10 ms since the last flush (the
heartbeat for clean evidence; 10 ms ≪ the 100 ms recovery round, and
one flush per 10 ms of clean symbols is EXACTLY the batch-cadence
semantics `default_fec()` was designed and tuned for). EWMA, Beta,
burst flag, GE record_symbol stay per-call (all O(1), they carry the
per-symbol pattern). No wire change, no timing change, no ack-clock
change, delivered set unchanged — pure compute. Gate resolved once
(OnceLock, estimator.rs; noted in the gates.rs header list), liveness
echo "estimator heavy-math cadence ACTIVE" at first construction.

Predictions (pre-registered):
1. MECHANISM: the exp/log/record_batch perf family collapses ≥ 22% →
   ≤ ~4%/core on BOTH sides at the c1 wall; BOCD update rate ~22k/s →
   ~100/s + loss-event rate.
2. c1 PRIMARY: est ≥ +8% vs def (band 215–240 from def ~200) and
   eb+est ≥ +8% vs eb (band 235–265 from eb ~218–223), both seeds,
   Δ ≫ σ_s (~3–11); sink probes: the msgs/s wall moves ≥ +15% (22–23k
   → ≥ 26k); receiver CPU/bit −15…−25%.
3. sc2/sc3 HOLD within σ on both seeds (wire-bound cells; the
   estimator's VALUES are equal-class at cell cadence — flush-on-loss
   keeps every loss event current) AND the recovery gauges hold class
   (fired/y, retx — a moved class = the staleness falsification).
4. c7 ≥ 0.97× same-session Σ; crown tail spot ×4 unregressed
   (p99 medians ~36–48 ms class, 1000/1000); dnf = 0; echo-RTT class
   unchanged everywhere (no timing surface touched).
Falsification: (i) recovery-gauge class moves at sc2/sc3 ⇒ the
cadence starves a consumer (r*/recovery reads a stale posterior) —
register row, NO tuning past the pre-set 10 ms constant. (ii) c1 flat
with the perf family collapsed ⇒ the estimator term was not
load-bearing (mis-attribution) — register row. (iii) any echo-RTT /
store-gauge movement ⇒ scope bug (falsification-(5) class): fix
before any verdict. Flip rule: `RWM_EST_CADENCE` flips DEFAULT ON in
this branch IFF predictions 1–4 hold on both seeds and all suites
stay green; else OFF + register row.

**Explicitly NOT built (profile-refused, with numbers):** (A) batched
datagram RECV / recvmmsg-class intake — the engine-loop/wakeup share
is ~3%/core (run_impl closure 3.08) + ~1.5–2% syscall entry; below
the 5% bar, and it borders the refuted drain family (its safe variant
would buy < ~5%). (C) legacy-Ack thinning/removal at the protocol
level — after Part 1 the remaining per-ack cost (serialize + send +
sender handle ≈ 4–5%/side combined) is below the bar; it also touches
the loss-feed (the Ack's expected/received counts ARE the sender's
loss signal) — a protocol change with a control-plane consumer is not
justified by a < 5% term. Both stay NAMED with their measured shares
for a future profile to re-ask. (D) `RWM_EMIT_BATCH` composition is a
battery ARM (eb+est), not a build — the sender binds second on v5 but
its gate's own flip rule (c1 ≥ 400) is untouched here.

*(Battery results below this line were written after the runs.)*

### L1 BATTERY RESULTS (VM 10.1.5.16; binary sha256 4f6a3f5e… = commit 224b915, built fresh (stale rm'd, CRLF-converted), SAME binary every run incl. probes/tails; E5-2650 v3 aes+avx2+pclmulqdq, kernel 7.0.14-101.fc43 in every log header; seeds 42 AND 7, arms interleaved round-robin per rep, fresh topology per invocation, 1 run/invocation, RWM_GEN=0 RWM_DIAG=1; driver `recvwall_battery.sh` + `tail_matrix.sh` (new `est` arm) + in-line probe loop; runtimes: s42 22:02:01–22:19:40 UTC (18 min, 80/80 clean, 0 retries), s7 22:22:21–22:41:17 (19 min, 79 completed + the seed-7 flake class: 18 RUN-RETRY recovered, 1 RUN-LOST sc2-est r2 → n=7 quoted), tails 22:48–23:00 (incl. the ×6 re-run), probes/post-perf 23:00–23:06; dnf = 0 in every completed run; liveness echoes asserted per arm BOTH directions (est echo required on client AND server), 0 ARM-LIVENESS-FAIL / 0 ARM-CONTAMINATION on captured runs; logs + per-run diag under `/home/vibe/recvwall/`)

**Goodput (Mbit/s, mean ± σ_s (n)); est = `RWM_EST_CADENCE=1`, eb =
`RWM_EMIT_BATCH=1`:**

| cell | arm | s42 | s7 |
|---|---|---|---|
| **c1 single 400 MB** | def | 193.7 ± 4.7 (8) | 197.8 ± 5.5 (8) |
| | **est** | **314.8 ± 19.4 (8) +62.5%** | **323.1 ± 25.7 (8) +63.3%** |
| | eb | 218.7 ± 8.1 (8) | 215.2 ± 5.5 (8) |
| | **eb+est** | **446.0 ± 13.8 (8) [417.6–459.2]** | **459.7 ± 33.4 (8) [402.0–505.7]** |
| sc2 single 100 MB | def | 88.04 ± 0.69 (8) | 87.23 ± 2.03 (8) |
| | est | 88.36 ± 0.48 (8) HOLD | 88.18 ± 0.94 (7) HOLD |
| sc3 single 25 MB | def | 16.61 ± 0.27 (8) | 16.70 ± 0.16 (8) |
| | est | 16.81 ± 0.37 (8) HOLD | 16.66 ± 0.12 (8) HOLD |
| c7 dual 200 MB | def | 171.1 ± 2.3 (8) = 0.972×Σ | 173.4 ± 2.5 (8) = 0.994×Σ |
| | est | **166.5 ± 4.9 (8) = 0.942×Σ** | **167.7 ± 2.4 (8) = 0.951×Σ** |

Every c1 arm-pair's per-run RANGES are DISJOINT on both seeds (def max
204.4/205.4 < est min 273.1/267.5; eb max 229.9/220.7 < eb+est min
417.6/402.0). The eb+est composition is SUPER-additive (+16% × +63% →
+130/+132% vs def): the two levers relieve OPPOSITE sides of the same
per-message pipeline. Longer-object datum (profile runs, 1.2 GB):
eb+est sustains **480–505 Mbit/s single-path** (the 400 MB numbers
carry ~2 s of BBR ramp).

**CPU (mean s/invocation, whole invocation incl. warm-up):**

| cell | CPUSRV def→est (s42/s7) | CPUCLI def→est (s42/s7) |
|---|---|---|
| c1 | 17.5→12.0 / 17.1→11.6 (eb+est: **8.8 / 8.5**) | 17.2→13.2 / 17.0→12.7 (eb+est: **8.1 / 7.8**) |
| sc2 | 7.13→**4.42 (−38%)** / 6.87→4.14 (−40%) | 6.05→4.63 (−23%) / 5.77→4.48 (−22%) |
| sc3 | 2.39→1.63 (−32%) / 2.27→1.58 | 2.60→2.05 / 2.48→1.98 |
| c7 | 12.6→10.0 / 12.5→9.7 | 13.9→12.7 / 13.8→12.4 |

Receiver CPU/bit at c1: −58% (est), −72% (eb+est ≈ 1.0–1.05 cores at
446–460). sc2/sc3: −22…−40% CPU at EQUAL goodput — the estimator tax
was real at every cell; only c1 had wire headroom to convert it.

**Engine-sink probes (RWM_RDIAG, single-c1 400 MB, seed 42): the wall
VISIBLY MOVED** — def 19.4–23k msgs/s at busy 83–85% (STEP 0); est
24.4–33.0k at busy 72–77% (goodput 291/324); **eb+est 45.8–61.6k
msgs/s serviced at busy 76–81%** (goodput 472/480), queue bounded
(q_max ≤ 1027 of 4096, no growth trend). Per-message service time at
the wall: ~48 µs (STEP 1) → **~23 µs** (eb+est battery CPU ÷ rate).

**Mechanism gauges (predictions 1/3):** post-build receiver perf at
the eb+est wall (505 Mbit/s run): the record_batch/exp/log family is
GONE from the ≥ 0.4% chart (was 22.4%/core) — top receiver terms now
_int_malloc 5.09, engine-loop closure 4.29, memmove 3.73, AEAD 3.47,
spin-lock 2.35. Recovery gauges at the lossy cells hold class in
every arm: sc2 fired 3116–3530 (def) vs 2974–3822 (est), echo
~100–107 ms both; sc3 fired 2226–2727 vs 1746–2584, echo bimodal
55–541 vs 175–533 both; c7 fired 4929–6141 vs 4452–5927 (est fires
FEWER, y-share lower). No falsification-(i) signal anywhere.

**The c7 term, gauge-attributed (the prediction-4 failure's named
mechanism):** in the c7-est arms the LEGACY plain-mode BtlBw anchor
over-reads a further **×3.4–3.7** (per-path btlbw gauge 304–349k
sym/s vs def's 88–92k — on top of the documented ×4.6–7.4 legacy
over-read), cwnd 5860 vs 1779, per-path bdp caps 2952/3714 vs
756/1103 — with echo RTT 265 ms class (def 125–171), sidle 1995 ms/219
events (def 594/103), sweeps 21 (def 3), recovery age 274 ms (def
143). Reading: the cheaper receiver emits acks in tighter bursts; the
legacy anchor's windowed-MAX takes the burst peak; at N ≥ 2 the
path-scaled pooled store (cap 4096) has HEADROOM for the inflated
anchor, so a standing queue forms and recovery patience stretches —
−2.7/−3.3% goodput. At N = 1 the 1024 latch clamps the same
inflation inert (sc2/sc3/c1 all hold or gain) — the anchor-hygiene
family's known defect surface (ADR-0061), reached through a NEW
channel (ack-clock speed), not a property of the posterior cadence
itself.

**Crown gate (tail_matrix c2 spot, seed 42, ship ↔ est):** p99 medians
ship 35.9 (400 B) / 39.2 (1200 B); est 36.4 / 42.6 — all inside the
historic ~36–48 ms class; **1000/1000 delivered in every rep, both
arms.** One est-1200B rep read p99 222 ms (historic worst-rep class:
164–193 on code-identical paths); the arm was re-run ×6 same session:
36.8–47.6 ms, no recurrence (pooled est-1200B n=10 median 41.8) —
recorded as the documented single-rep session-noise class, delivery
complete throughout. Crown UNREGRESSED.

### VERDICT vs the amendment — the c1 predictions land ABOVE their bands; the flip is blocked by the c7 clause

1. **Mechanism (prediction 1): PASS** — the 22.4/25.9%-per-core
   estimator family is off the chart on both sides; CPU/bit −22…−72%.
2. **c1 (prediction 2): PASS, above the pre-registered bands** — est
   +62.5/+63.3% (band floor +8%, band 215–240 — measured 314.8/323.1);
   eb+est 446/460 (band 235–265); msgs/s wall 22–23k → 46–62k (gate
   ≥ 26k). The overshoot is recorded as a band miss in the FAVORABLE
   direction: the profile's 22% share under-predicted the gain because
   the freed core-seconds also relieved the futex/wake serialization
   around the same loop (the sink scales super-linearly near
   saturation).
3. **sc2/sc3 (prediction 3): PASS** — hold within σ on both seeds with
   recovery classes unchanged and −22…−40% CPU.
4. **c7 ≥ 0.97×Σ (prediction 4): FAIL on both seeds** (0.942/0.951 vs
   def's 0.972/0.994; Δ −4.6/−5.7 ≈ 2σ, consistent) — crown/dnf/echo
   clauses all pass. Per the fixed flip rule: **NO FLIP —
   `RWM_EST_CADENCE` ships DEFAULT OFF** (measured A/B lever, 3 law
   tests). NO register row: discipline 11's names-a-new-mechanism
   clause governs — the failure isolates the legacy anchor's
   burst-peak windowed-MAX under a faster ack clock at the N ≥ 2
   pooled store (gauges above), while the cadence itself behaves
   exactly as derived at every N = 1 cell. SUCCESSOR (named, NOT
   built, needs its own item-11 pre-registration): compose
   `RWM_EST_CADENCE` with the honest-anchor family at duals
   (`RWM_PLAIN_RS`/honest caps — burst-robust send-interval sampling
   is exactly ADR-0061's cure for this over-read class), or
   burst-robustify the legacy anchor's rate sample directly; either
   would also unblock the composition for the c7/c8 cells.

**The wall's new position (the deliverable):** v5 default c1 sink
~200 Mbit/s; +`RWM_EMIT_BATCH` ~218; +`RWM_EST_CADENCE` ~315–323;
**both ~446–460 (400 MB) / 480–505 sustained (1.2 GB) at ~46–62k
msgs/s serviced** — the engine-receiver per-message service wall
moved from ~22–23k msgs/s ≈ 210–230 Mbit/sink to **~46–62k msgs/s ≈
450–505 Mbit/sink** (~23 µs/message, was ~48). Distance to the
external bar: quinn-bbr 915–922 = **×1.8–2.0 of the new ceiling**
(was ×4.3). The remaining per-message gap to quinn's 5.1 µs/packet
is no longer estimator overhead: the post-build receiver chart is
allocator + loop/wake machinery + memmove + AEAD — flat (#84 shape,
top term 5%), with the dual-ack density (legacy Ack + WindowAck ≈ 2
control datagrams per data message vs quinn's 1 per ~24) the largest
REMAINING named structural term (~4–5%/side serialize+send+handle,
measured below the 5% build bar this session). Both arms of the c1
lever ship default OFF behind their pre-registered gates; the
composed opt-in (`RWM_EMIT_BATCH=1 RWM_EST_CADENCE=1`) is the
documented fast configuration for clean single-path deployments.

### Tests

lib 377+2 ignored (= the 379 set: +3 `est_cadence` law tests — per-call
default parity, accumulate/flush-on-loss/heartbeat, posterior
equal-class); `gate_suite` 15/15 release (--test-threads 1);
`mtu_blackhole_wedge` 2/2; `perf_loopback` 8/8; `emit_batch_loopback`,
`win_decouple_loopback`, `wire_compact_loopback`, `copa_sole_loopback`,
`recov_mp_loopback`, `backpressure` — all green release;
`raptorpath-math` full suite green. Env-unset tree: the gate-off branch
executes the identical per-call `bocd.update` (pinned by the
default-parity law test).

Ops: lock `/tmp/rwm-vm.lock` taken 2026-08-06 21:04:20 UTC (found
FREE), held through sync → builds → STEP 0 → STEP 1 → build → battery
→ tails → probes → suites, released 23:43:41 UTC after teardown
verification (no rp processes, no rp-* netns);
rp-* netns only, torn down per invocation; stale binaries removed
before both builds (baseline 6bb6ca96… = 1ce8ba2, battery 4f6a3f5e… =
224b915, sha256 + commit + lscpu + kernel in every log header);
seed-7 flake class quoted per arm (18 RUN-RETRY recovered, 1
RUN-LOST, nothing discarded); FOREGROUND polling only; logs + perf
data + per-run diag preserved under `/home/vibe/recvwall/`.

## Ship The Wins 1: est×honest-anchor (2026-08-07) — PRE-REGISTRATION (discipline item 11 — this block written and committed BEFORE any build and BEFORE any VM run; branch `feat/ship-est-cadence` from dc4fb78; the §16.35 verdict's NAMED SUCCESSOR: compose `RWM_EST_CADENCE` with the honest-anchor family at duals, re-gate `RWM_EMIT_BATCH` in composition, flip BOTH default ON)

**(a) The question.** §16.35 measured the c1 lever (est +62/+63%, eb+est
446–505) and blocked the flip on ONE clause: c7-est 0.942/0.951×Σ
(≥ 0.97 required), gauge-attributed — the faster ack clock's tighter ack
bursts feed the LEGACY plain-mode anchor's ack-interval Δt, whose
windowed-MAX takes the burst peak: per-path btlbw 304–349k sym/s vs
def's 88–92k (a further ×3.4–3.7 on the documented ×4.6–7.4 legacy
over-read), per-path bdp caps 2952/3714 vs 756/1103 — and only the
N ≥ 2 path-scaled pooled store (clamp 4096) has HEADROOM for the
inflated anchor, so a standing queue forms (echo 265 ms class vs def
125–171, sidle 1995 ms/219, sweeps 21 vs 3, recovery age 274 vs
143 ms). At every N = 1 cell the 1024 latch clamps the same inflation
inert. The named successor IS this task: feed the DUAL-store cap law
from a burst-immune anchor.

**(b) The anchor consumer being fixed — stated precisely.** The
STORE-CAP / POOL LAW'S RATE INPUT at N ≥ 2 (the `Σ copa_bdp_anchor`
term of `path_scaled_store_cap`), and NOTHING else. NOT the Copa cwnd
feed: the full sampling-only CopaFeed at duals (`RWM_PLAIN_RS`)
carries the measured −22…−27 Mbit c7 composition price ("C8-Aware Pool
Law" ATTRIBUTION: the entire c7-capw/rs regression is owned by the RS
feed composition — emission-side suppression, win ≪ cap, infl
collapsed), so that price must stay UNREACHABLE: no CopaFeed at
N ≥ 2, no `charge_src`/`on_src_sent`/`src_inflight` (the
falsification-5 lesson, §16.34: a scoped feed must not leak
src_inflight at duals — here NO feed machinery runs at all), no change
to `record_delivery`/cwnd dynamics (the legacy ack path stays
byte-identical). N = 1 keeps every law bit-exactly.

**(c) Mechanism (pre-registered).** The hygiene-grade send-interval
sampler is burst-immune BY CONSTRUCTION: Δt spans the SEND interval on
the sender's own clock, so an ack burst cannot collapse it (ADR-0061
principle; `control::anchor::SendRateAnchor` — per-bucket send rate
≈ SRTT/2 buckets, windowed-max ≈ 8·SRTT, clock-gap buckets DISCARDED
with quarantine). Give each path its own `SendRateAnchor` fed at
`charge_in_flight` (every wire send on that path: source, redundant,
retransmit — the true send process), and at N ≥ 2 size the pooled
store from it via the ALREADY-DERIVED honest law: pool =
clamp(Σ_i honest_store_cap(rate_i·RTprop_i, rate_i, K_i, gain),
floor, N·knee) — the `capw_store_cap` shape (one shared pool,
borrowing free), with K_i from the existing `EchoRatioMin` machinery
and rate_i from the send anchor instead of the RS delivery sampler.
Engaged only when ALL live paths' send anchors are warm (the capw
precedent: a partial sum under-provisions); until then the configured
path-scaled law runs verbatim. Send rate ≈ delivered rate + retx
share (≤ a few % at these cells) — the safe direction, ~1× truth vs
the removed ×10-class over-read. Expected pool at c7 (sr_i ≈ 8–10k
sym/s, RTprop ≈ 8–16 ms, K ≈ 1–3, gain 2): Σ ≈ 2–3k — the
1024-latch-per-path class the c8 attribution named as the good
operating point (pool ≈ Σ_i cap_i with each cap_i ≈ 1.1–1.3k),
vs def's inflated 3.7k and est's 4096 clamp.

**(d) Gates + the composed default.** New gate `RWM_POOL_ANCHOR`
(gates.rs + scheduler resolve-once): the honest pooled-store anchor at
N ≥ 2. DEFAULT = the est-cadence resolution — ONE composed default:
env-unset ⇒ `RWM_EST_CADENCE` ON (default flipped in this branch) ⇒
pool-anchor ON; `RWM_EST_CADENCE=0` ⇒ the full prior stack (both off)
— the prior-default A/B arm is one knob (+`RWM_EMIT_BATCH=0`); and
`RWM_POOL_ANCHOR=0` alone is the est-only decomposition arm (the
§16.35 blocker reproduction). `RWM_EMIT_BATCH` default flips ON in
the same branch (its §16.28 gate "c1 ≥ 400" was measured 446–505 IN
COMPOSITION with est — the re-gate here is the COMPOSED gate; its
sender-batching scope is single-live-path only, duals structurally
inert). Liveness echoes: est (existing, both directions), eb
(existing), NEW "pool-anchor honest dual-store law ACTIVE".

**(e) Predictions (pre-registered).**
1. MECHANISM at c7 (the blocker's gauges, est ON): per-path DIAG shows
   the send-anchor rate sr_i ≈ 8–11k sym/s (≈1× truth) on both paths;
   effective store cap ≈ Σ honest caps ≈ 2–3k (NOT the 4096 clamp);
   echo RTT back in the def class (≤ ~180 ms, not 265); sidle/sweeps/
   recovery-age back in the def class (sweeps ≤ ~5-class, not 21).
   The legacy btlbw gauge may STAY inflated (the cwnd feed is
   deliberately untouched) — the claim is about the CAP input, and the
   cap gauge decides.
2. c7 (THE clause): new default (est+pa; eb inert at duals) ≥ 0.97×
   same-session Σ on BOTH seeds. The est-only arm reproduces the
   blocker class (< 0.97, the control that proves the mechanism).
3. c1 (PRIMARY): new default (est+eb+pa) ≥ 430 Mbit/s mean on BOTH
   seeds at 400 MB (the §16.35 composed class 446–460; pa engages
   only at N ≥ 2 so c1 is est+eb exactly), plus one 1.2 GB sustained
   run ≥ ~480. Prior-default arm reproduces the ~200 class.
4. sc2/sc3: new default within σ of prior default (N = 1: pa inert by
   construction, est holds per §16.35, eb holds per §16.28) with
   recovery-gauge classes unchanged.
5. c8 dual 25 MB: new default ≥ the shipped-default class, with the
   derivation-predicted upside toward the 0.87×Σ legacy line (the
   honest pool Σ ≈ fast cap + slow cap ≈ 1.6–1.9k sits nearer the
   c8-attribution's pool ≈ max_i cap_i ≈ 1024–1250 good class than
   the path-scaled 3.7–4.1k) — GATE: ≥ 0.87×Σ line-class on both
   seeds is the goal's bar; an honest miss that still ≥ the shipped
   0.72–0.76 class with the pool gauge at its derived value is a
   named finding vs the c8 WATCH, judged by the pre-set falsification
   rule below.
6. CROWN (mandatory): tail_matrix c2 spot ×4 — p99 medians in the
   historic ~36–48 ms class, 1000/1000 delivered, new default vs
   prior default. 7. dnf = 0 everywhere; wedge suite green.

**(f) Falsification (fixed now).** (i) If the honest-fed store cap
still converts the denser ack clock into a standing queue at c7
(echo/sweep gauges in the est class with the cap gauge at its derived
2–3k) ⇒ the mechanism is DEEPER than the anchor — name it from the
gauges (candidates it would isolate: the recovery plane's patience
under the faster clock; the WindowAck density itself), register row,
NO tuning pass; a second falsified mechanism ⇒ structural-bound
documentation per the goal's honest exit. (ii) c7 ≥ 0.97 but c1 < 430
⇒ the composed default fails its PRIMARY gate — no flip, the c1 term
re-attributed with the CPU/sink probes. (iii) sc2/sc3/crown/c8
regression ≫σ on both seeds ⇒ scope defect or law failure — c8/sc2/
sc3 gauge state decides which; a scope defect (N = 1 or dual
inertness broken) is a BUG: fix before any verdict (the §16.34
incident protocol). (iv) est-only arm NOT reproducing the c7 blocker
⇒ the §16.35 attribution itself is session-dependent — record, and
the flip rides prediction 2 alone. Flip rule: BOTH defaults (est
composed + eb) ship ON IFF predictions 2–4 + 6–7 hold on both seeds
and c8 passes per its stated rule and suites are green; else
defaults revert (one-line change each) + register rows.

**(g) Battery (pre-registered).** VM 10.1.5.16 per MEASUREMENT
DISCIPLINE 1–12: lock `/tmp/rwm-vm.lock` PRIORITY 1 (the shal8 worker
does local work behind it; the lock covers ALL VM activity incl.
builds); tree synced via git archive of THIS branch + CRLF conversion
before the first harness invocation; stale binary removed before
every build; binary sha256 + commit + lscpu + kernel in every log
header; FOREGROUND polling only; rp-* netns only; fresh topology per
invocation; seeds 42+7, ×8/arm interleaved round-robin per rep;
seed-7 topo-abort protocol (n recorded, nothing discarded); liveness
echoes asserted per arm both directions (est/eb/pool-anchor);
ARMCOUNT per arm; runtimes stated; same-session Σ from the battery's
own singles per arm env; logs under `/home/vibe/shipest/`. Arms:
**new** (env unset = est+eb+pa), **prior** (`RWM_EST_CADENCE=0
RWM_EMIT_BATCH=0`), **estonly** (`RWM_POOL_ANCHOR=0
RWM_EMIT_BATCH=0`). Cells: c1 400 MB ×8 new↔prior (PRIMARY ≥ 430) +
1×1.2 GB new (sustained); sc2 100 MB + sc3 25 MB ×8 new↔prior;
c7 200 MB ×8 new↔prior↔estonly (THE clause + the blocker control);
c8 25 MB ×8 new↔prior; tail_matrix c2 spot ×4 seed 42 new↔prior
(crown); driver `tools/l1/shipest_battery.sh` (recvwall pattern,
retry-hardened).

### AMENDMENT (pre-battery, after the mechanism smoke — the winmtu pattern; committed BEFORE any battery run)

The first c7 default-env smoke on the VM (binary d17619e5… = a7d2d69,
2026-08-07 ~09:35 UTC, one run, 170.5 Mbit dnf 0) showed the law
ENGAGED (`pa=on`) but the mechanism gauges UNFIXED: per-path
`sr=1961 / 53354` vs truth ≈ 8.9k, Σ = 6644 → still the 4096 clamp,
echo 258 ms / sweeps 19 (the blocker class). DEFECT, named: the
pre-registered per-bucket windowed-MAX is the right statistic for a
PACED send process (the A* span consumer, where cc_pace shapes
emission) but not for the ADMISSION-GATED plain sender at N ≥ 2 — a
SACK-release burst refills the store at emission speed, so individual
≈SRTT/2 buckets legitimately read many× the drain rate and the MAX
latches them (the same burst-peak channel as the ack side, reached
from the send side). The honest send-process statistic is the
GAP-ROBUST WINDOWED MEAN: Σ count / Σ Δt over the SURVIVING buckets
of the same window (clock-gap buckets discarded, quarantine
hold-through unchanged — hygiene rules intact); a time-normalized
mean cannot be inflated by burst concentration, and under a bounded
store it converges to the true carried rate (drain + retx share).
AMENDED BUILD (one function): `SendRateAnchor::mean_rate()` alongside
the untouched `rate()` (A* keeps its windowed-max verbatim);
`PathState::send_rate_anchor()` reads the mean. The unit law test
gains the measured defect as its burst model (periodic store-refill
bursts ≫ truth: the max latches, the mean must hold ≈ truth).
Predictions 1–7 and every falsification clause stand VERBATIM on the
amended statistic; the smoke run is recorded as mechanism shakeout,
not battery evidence.

*(Results below this line were written after the runs.)*
