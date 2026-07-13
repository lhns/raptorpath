# Verdict audit: DAPS-era merges on invalid/unverified evidence

Date: 2026-07-13. Read-only audit of `C:\Users\pierr\Documents\claude\raptorpath` (main @ d63ffce).
Trigger: diagnostic (branch `diag/slow-path-anchor`, goal-gate "Slow-Path Anchor Diagnosis") proved
(1) `tools/l1/perf_rwm_c.sh` never passes `--window-generation-coding`, and
(2) §16.14's mechanism DIAG was read from the RECEIVER log (`/tmp/rwm-s.log`) instead of the bulk
sender (`--client`, `/tmp/rwm-c.log`).

Key code facts (verified in this audit, all paths absolute):

- `raptorpath/src/net/mod.rs:699-702`:
  `fmtcp = env("RWM_FMTCP").is_ok()`;
  `window_generation = window_reliable && (window_generation_coding || window_systematic_repair || fmtcp)`.
  **`RWM_DAPS` does NOT appear here** — it never has: verified identical at every experiment commit
  (226bca7, cd2882e, 824461c, 3444997, 4606829, 11e0f5e, 68d6b6c, d63ffce). `RWM_DAPS=1` alone
  therefore NEVER enables generation; the sender-side comment "RWM_DAPS implies that base"
  (net/mod.rs:3316) is true only *inside* `run_window_sender`, i.e. after `generation` was already
  decided false.
- `raptorpath/src/net/mod.rs:3317`: `daps = env("RWM_DAPS").is_ok() && generation` — the whole DAPS
  stack chains off this.
- Harness `raptorpath/tools/l1/perf_rwm_c.sh:132-145`: passes `--window-reliable` (+ optional
  `$OOO_FLAG $EXTRA`) only. Generation can enter a run ONLY via `RWM_EXTRA="--window-generation-coding"`
  (or `--window-systematic-repair`) or via env `RWM_FMTCP` (self-enabling at line 699).
- Bulk + `--window-reliable` auto-selects the RLC streaming backend (net/mod.rs:651-663), so
  `is_window_mode` (net/mod.rs:355-357) is true — the runs were window-pipeline, not block.
- **In plain (non-generation) window mode the sender's per-path Copa BtlBw/BDP anchor is NEVER fed.**
  `WindowAck` handling (net/mod.rs:6883-6947) records RTT only; `ControlMessage::Ack` (which drives
  `Scheduler::ack → PathState::on_ack → copa.record_delivery`, scheduler/mod.rs:1159-1160, 1842-1846)
  is sent only by the BLOCK receive path (net/mod.rs:2421-2497, the `else` of the window branch).
  The only window-mode anchor feeds are `on_src_delivered`/`on_src_delivered_seq`
  (net/mod.rs:4607-4625, 5827-5846), both gated `per_path_est` ⇒ `generation`.
  Consequence: a plain window-reliable SENDER must show `est=n`, `btlbw=0`, `bdp0` forever.
  Any sender-log DIAG quoting `est=Y`/large `btlbw` implies either generation was ON for that
  specific run, or the DIAG was not read from the bulk sender's log.
- Footguns: `.is_ok()` gates mean `RWM_FMTCP=0` STILL enables generation (net/mod.rs:699) and
  `RWM_DAPS=0` still counts as set (net/mod.rs:3317). Only `RWM_SRC_BP`, `RWM_RATE_SAMPLE`,
  `RWM_DAPS_DEPTH`, `RWM_DAPS_PACE`, `RWM_PACE_ALL` treat "0" as off.

---

## 1. Gate table: active without generation?

| env gate | code (raptorpath/src/net/mod.rs) | active when window_generation=false? |
|---|---|---|
| `RWM_FMTCP` | :699 (`is_ok()` self-enables generation), :3318 | **YES — it is the generation ENABLER** (needs `--window-reliable` + streaming backend, both present in every harness run). Caution: `RWM_FMTCP=0` also enables. |
| `RWM_DAPS` | :3317 `daps = is_ok() && generation` | **NO — dead.** All DAPS placement is behind `if reliable && daps` (:3991-4027). No other read site. |
| `RWM_DAPS_BDP` | :3346-3351 (`if daps {..} else {0.0}`) | **NO — dead** (gain forced 0.0). |
| `RWM_DAPS_PACE` | :3352-3353 (`daps && ..`) | **NO — dead.** |
| `RWM_PACE_ALL` | :3360-3361 (`daps_pace_on && ..`) | **NO — dead** (`paced_repair_decision` sites all under generation-mode emission blocks). |
| `RWM_SRC_BP` | :3385-3386 (`daps_pace_on && ..`); TUN-read guard :5324-5334, :5386-5391 | **NO — dead.** The TUN-read pause is NOT generation-independent: with `src_bp_on=false` the guard `(!src_bp_on \|\| src_pace_ok)` short-circuits and `src_pace_ok` is constant `true`. |
| `RWM_PER_PATH_EST` | :3394-3395 (`generation && (daps \|\| is_ok())`) | **NO — dead.** Attribution sites :4607, :5827 both `if per_path_est`. |
| `RWM_RATE_SAMPLE` | :3402-3405 (`per_path_est && ..`) | **NO — dead** (`on_src_sent` :4169-4174 gated `rate_sample`; scheduler mentions are comments only). |
| `RWM_DAPS_DEPTH` | :3422-3425 (`rate_sample && ..`) | **NO — dead** (deepest link of the chain). |

Chain: `generation → daps → daps_pace_on → {pace_all_on, src_bp_on}`; `generation → per_path_est →
rate_sample → daps_depth_on`. One false at the root kills everything below. With generation off,
every A/B toggle among the dead gates compares **byte-identical behaviour** against itself.

---

## 2. Section-by-section classification (docs/goal-gate.md)

Legend: VALID = mechanism demonstrably ran; INVALID = mechanism provably inert (dead code measured);
UNCERTAIN = ledger does not establish whether generation was on; what would settle it is stated.
NOTE: none of the DAPS-era sections record their command lines/env (unlike the systematic-repair-era
sections, which recorded `RWM_EXTRA="--window-systematic-repair"`, goal-gate.md:2667-2668, 2921, 3020).
That absence is itself the central ledger-discipline failure.

### 2.1 "DAPS + Right-Sized FEC" (goal-gate:4438, paper §16.10) — **UNCERTAIN, leaning INVALID (DAPS arms)**
Headline: C8 0.48×→0.80× (13.12 Mbit/s, seed42 25MB×5), r-sweep monotone with r*≈0.03,
r=0.02 under-FEC cliff (3.83, σ53.7), "DAPS paused=0%" vs FMTCP 13–68%; C7 no regression.
- The FMTCP arms (7.58 historical, 7.14 r=0.03) are VALID: `RWM_FMTCP` self-enables generation.
- The DAPS arms set (at best) `RWM_DAPS=1 RWM_GEN_R=...`; no `RWM_EXTRA`/`RWM_FMTCP` recorded. On the
  code at 226bca7 that is **inert** — the run would be plain `--window-reliable`, identical to the
  "shipped-default" arm (8.70, σ_s 8.79). `RWM_GEN_R` is unused in plain mode, so the entire r-sweep
  spread (3.83…13.12) would be noise of a distribution whose recorded σ_s reaches 8.8–53.7 s.
- "paused=0%" is suspicious for plain C8 (the plain store-gate demonstrably stalls: full-rerun plain
  C8 = 5.43 Mbit/s, median 78 s) but is exactly what a RECEIVER log shows — the wrong-log pattern
  proven for §16.14.
- Settling evidence: the session's `/tmp/rwm-c.log` (`cod>0`, `eff_pace>0`) or a re-run with
  `RWM_EXTRA=--window-generation-coding`. Until then the headline 0.80× and the r* sweep are
  unverified, and the "revision to §16.8/16.9" (scheduling-bound, not recovery-latency-bound) is
  unsupported.

### 2.2 "DAPS Queue Management" (goal-gate:4576) — **UNCERTAIN, leaning INVALID; deltas noise-compatible even at face value**
Headline: cap/pace lift C8 ~10.0→~11.5 (+15%), each lever ~11.2; verdict "correct mechanism
(oracle ×1.19) but INERT — residual is per-path rate estimation"; slow RTT 1364→1774 ms; DIAG
`infl=0/bdp0` throughout.
- `RWM_DAPS_BDP`/`RWM_DAPS_PACE` are dead without generation → all four arms identical code under the
  inert hypothesis; the +15% (σ_s 2.0–5.7 across 5-run arms) is within noise either way.
- Irony: the section's own conclusion (the cap/pace were inert for lack of a per-path rate signal) is
  code-true in plain mode too — WindowAcks never feed `record_delivery` in ANY window mode — so the
  merged residual-diagnosis was right-ish for possibly the wrong reason. The quoted "occasionally
  bdp71" is impossible on an inert sender (anchor never fed) — wrong log or generation-on sub-run.
- Settling evidence: same as 2.1.

### 2.3 "Per-Path Estimator" (goal-gate:4692) — **UNCERTAIN (throughput claim); mechanism DIAG internally inconsistent with the inert hypothesis**
Headline: est=Y 0%→93% (618/663), slow bufferbloat 3.7 s→~0.3 s, pooled C8 7.85→10.24 (+30%,
0.40→0.52 of ceiling), stabilizes; general-fix check 89% est=Y with `RWM_PER_PATH_EST=1`.
- There is no same-binary off-toggle for the estimator under DAPS (`per_path_est` is forced by
  `daps`), so the "baseline (pre-estimator)" arm was necessarily a different binary — the A/B is not
  the same-binary comparison the table implies.
- If generation was off, `est=Y` is impossible on the sender in EITHER arm (see key code facts), so
  the 93% figure requires either a generation-on DIAG run (unrecorded) or a misread log. Both
  possibilities detach the mechanism evidence from the throughput battery.
- The +30% pooled lift is smaller than documented same-config session swings (see §3).
- Settling evidence: re-run A/B with generation verified (`cod>0`) and est% read from `/tmp/rwm-c.log`.

### 2.4 "Pace-All Traffic" (goal-gate:4813, paper §16.11) — **UNCERTAIN, leaning INVALID**
Headline: same-binary `RWM_PACE_ALL` A/B, pooled C8 7.31→11.11 (+52%), both seeds (+55%/+49%),
σ_s collapses; C7 source-only arm 12.08 (σ_s 14.29) vs pace-all 21.02 (σ_s 0.59).
- `RWM_PACE_ALL` is dead without generation → identical arms under the inert hypothesis; the
  two-seed-consistent +52% would have to be session drift (arms not interleaved — interleaving first
  appears in §16.14). Documented same-config swings reach 2.3× across sessions (see §3), so drift of
  this size is possible, though the two-seed consistency is the strongest pro-validity signal among
  the DAPS-era sections.
- The C7 source-only 12.08 σ_s 14.29 "bimodal" arm is itself a red flag: on symmetric paths with the
  stack live, pace-all should be nearly neutral (equal buckets) — a 12.08 vs 21.02 split looks like
  the plain-reliable bimodal distribution, not a repair-pacing effect.
- Settling evidence: interleaved A/B with `cod>0` verified.

### 2.5 "Source Backpressure — REFUTED" (goal-gate:4926, #73, paper §16.12) — **UNCERTAIN; the REFUTED verdict is unsafe either way**
Headline: spill baseline 14.35/15.63 (pooled ~14.99, σ_s ~1.2) vs `RWM_SRC_BP=1` 6.60/7.39
(pooled ~7.00, σ_s 9.5/4.1) — −53% on both seeds; mechanism "source is the pipeline clock";
"gate largely inert anyway (anchor over-read ×145)".
- Code answer to the audit question: **NO, the TUN-read pause is not generation-independent.**
  `src_bp_on = daps_pace_on && RWM_SRC_BP` (net/mod.rs:3386); with it false, `src_pace_ok` is
  constant `true` and the select guard `(!src_bp_on || src_pace_ok)` (:5391) never defers a read.
  If generation was off, the two arms were byte-identical and the code CANNOT explain the −53%:
  it must be session drift (14.99 vs 10.74 vs 6.50 for the same nominal config across three
  sessions — same order of magnitude as the claimed effect).
- The `paused=100% good=0` stretches quoted as the mechanism are, in plain mode, ordinary
  store-backpressure stalls that occur in BOTH arms (plain C8 is store-stall-bound; full-rerun
  plain C8 = 5.43, median 78 s).
- Internal tension even under the generation-on hypothesis: the section simultaneously claims the
  gate "rarely engages" (bucket never dry, ×145 over-read) AND that it caused a 53% regression.
- The 14.99 baseline is also anomalous (see §3). The ship decision (default OFF) is harmless, but
  the scientific REFUTED verdict and the "source is the pipeline clock" mechanism are unverified.
- Settling evidence: interleaved A/B, generation verified, plus a paused%-by-cause trace.

### 2.6 "BtlBw Rate-Sample Fix" (goal-gate:5022, #74, paper §16.13) — **UNCERTAIN; the C7-regression sub-claim is refuted-as-noise by §16.14's own data**
Headline: anchor over-read ×158→×1.05 CLOSED, fast bufferbloat 1573→30 ms (primary metric);
C8 pooled 10.74→9.7 (−9.5%, seed-dependent); C7 20.96 (σ_s 0.55) → 16.97 (σ_s 4.38), −19%.
- If generation was off, `rate_sample` was dead in both arms AND the legacy anchor was equally unfed
  (the legacy path `on_src_delivered` is also `per_path_est`-gated) — the sender DIAG could show
  neither ×158 nor ×1.05, only `est=n/btlbw=0`. The quoted DIAG contrast therefore requires a
  generation-on run (unrecorded) or a wrong log. It cannot be tied to the throughput battery.
- C7 regression: §16.14 subsequently ran arms B and C on symmetric C7 — **provably identical code
  paths** (depth is a symmetric no-op) — and got 21.20 vs 16.96, a 20% pure-noise swing; §16.14
  itself concluded §16.13's C7 "regression" was "largely noise". Under the inert hypothesis the
  §16.13 C7 arms were identical code too, making noise the only explanation. Either way the
  "rate-throttle politeness" narrative built on C7 20.96→16.97 (and reused by §16.14's oracle
  PART 6h as its calibration target, "reproduces 0.810 exactly") is unsupported.
- C8 −9.5% pooled is well inside session noise.
- Settling evidence: re-run the anchor DIAG with generation verified on the sender log (this is
  exactly what the Slow-Path Anchor Diagnosis then did — and it found the anchor DOES establish but
  swings ~4000×, a different story from both §16.13 arms).

### 2.7 "DAPS Read-Ahead Depth" (goal-gate:5150, paper §16.14) — **INVALID (proven)**
Headline: three-arm A/B/C, arm C best/most stable (pooled 8.40 vs 7.22 vs 6.50); "slow anchor never
establishes (`est=n`, `btlbw=0`, `dbud=0`)"; "bulk C8 heterogeneous aggregation is BOUNDED below
fast-path-alone"; RECOMMENDATION: CONSOLIDATE.
- The Slow-Path Anchor Diagnosis records that the saved battery server logs show `cod=0`,
  `eff_pace=0` everywhere ⇒ zero generation emission ⇒ **DAPS, rate-sample and depth-bound were all
  inert; arms A/B/C executed identical transfer code.** The ordering A<B<C and the stability story
  are draws from one distribution. The mechanism DIAG was additionally read from the receiver log.
- "Slow anchor never establishes" was directly REFUTED by the follow-up diagnosis (anchor
  establishes 100% of the active transfer once generation is actually on).
- The one survivable observation: dual-path C8 below single-c2 held "across every arm and both
  sessions" — but every such arm was (or is unverified not to be) plain window-reliable, so it
  supports "PLAIN window-reliable dual C8 < fast-alone" (already known: full-rerun 5.43 vs 15.9),
  NOT a bound on the DAPS/generation stack. The CONSOLIDATE recommendation is void (already
  withdrawn by the diagnosis section, but the paper §16.14 still carries it — see §4).

### 2.8 "Slow-Path Anchor Diagnosis" (goal-gate:5283, 2026-07-13) — **VALID**
Headline: §16.14 refuted on both harness findings; with `RWM_EXTRA=--window-generation-coding`
(recorded command line, goal-gate:5395-5398) the pipeline activates (`eff_pace=2000`), the slow-path
anchor establishes (est=Y 90/108 DIAG lines, sent=3444, attr=3443, rej≈0) but BtlBw_slow is a
decode-clocked windowed-MAX swinging ~4000× (5–20 950 sym/s around a true 2 083), so no depth/pace
bound can key on it; verdict FIXABLE, not fundamental.
- Generation demonstrably on; sender log read; counters cumulative. Single run (12 MB × 3, seed 42),
  DIAG only — mechanism evidence is solid, no throughput claims made. This section is the model of
  what the others should have recorded.

### 2.9 Context sections (paper task): §16.8 — **VALID** (rests on the systematic-repair-era arc whose
runs recorded `RWM_EXTRA="--window-systematic-repair"`); §16.9 FMTCP — **VALID** (`RWM_FMTCP`
self-enables generation; C8 0.48×, C7 ×1.62, TUN-paused 13–68% stand, subject to ordinary VM noise).

---

## 3. Reconciling the contradictory C8 "baselines"

All rows are C8 (c2+c3) dual, `perf_rwm_c.sh`, 25 MB, bulk hint, as recorded in the ledger.
"Nominal config" is what the section believed it measured; under the inert hypothesis every row from
§16.10 onward (except FMTCP arms) is the SAME config: plain `--window-reliable`.

| recorded value | section / arm | nominal config | seeds × runs | notes |
|---:|---|---|---|---|
| 8.70 (σ_s 8.79) | DAPS §: shipped-default | plain reliable (intended) | 42 × 5 | honest plain-mode point |
| 13.12 (σ_s 1.21) | DAPS §: DAPS r=0.03 | DAPS+r* | 42 × 5 | unverified generation |
| ~10.0 (σ_s ~5.5) | QM §: no-QM | DAPS, BDP=0 PACE=0 | 42 × 5 | " |
| ~11.5 (σ_s ~2.9) | QM §: QM both | DAPS+QM | 42 × 5 | " |
| 5.88 / 9.81 → 7.85 | Estimator §: baseline | DAPS+QM pre-estimator (other binary) | 42,7 × 8 | " |
| 9.58 / 10.90 → 10.24 | Estimator §: fix | DAPS+QM+est | 42,7 × 8 | " |
| 7.67 / 6.96 → 7.31 | Pace-all §: PACE_ALL=0 | full stack minus repair-pacing | 42,7 × 8 | " |
| 11.88 / 10.34 → 11.11 | Pace-all §: default | full stack | 42,7 × 8 | " |
| **14.35 / 15.63 → 14.99** | Src-BP §: SRC_BP=0 | full stack (≡ pace-all default) | 42,7 × 8 | should ≈ 11.11; +35% |
| 6.60 / 7.39 → 7.00 | Src-BP §: SRC_BP=1 | full stack + src-bp | 42,7 × 8 | −53% vs its own baseline |
| **13.25 / 8.22 → 10.74** | Rate-sample §: RS=0 | full stack, legacy anchor (≡ 14.99 row) | 42,7 × 8 | −28% vs 14.99 |
| 10.73 / 8.71 → ~9.7 | Rate-sample §: RS=1 | full stack + rate-sample | 42,7 × 8 | |
| **6.50 (seed7 timed out)** | Depth §: arm A RS=0 | ≡ the 10.74 row | 42 × 8 | −39% vs 10.74, −57% vs 14.99; **proven plain-mode (cod=0)** |
| 7.22 / 8.40 | Depth §: arms B / C | + RS / + RS+DEPTH | 42,7 × 8 | proven plain-mode |
| 5.43 (σ_s 11.72) | Full-rerun (2026-07-08) | plain reliable, 50 MB × 6 | def × 6 | the known plain-mode anchor point |

Reconciliation:
- **Same nominal config, three sessions: 14.99 → 10.74 → 6.50 (2.3× spread).** The ledger records
  bytes/runs/seeds but NOT the full env, binary hash, `cod`/`eff_pace` sanity values, arm ordering,
  or interleaving (first used in §16.14). It therefore CANNOT reconcile these — a ledger-discipline
  finding in itself.
- The spread is however exactly what the proven-inert reading predicts: plain window-reliable dual
  C8 is a heavy-tailed, bimodal distribution (5.43 mean with σ_s 11.7; single runs from ~2 to ~16
  Mbit/s; §16.12's own single-c2 ceiling that day was bimodal, median 15.9 / mean 9.8). Small-n
  (5–8 run) arm means drawn from it wander over 4–15 Mbit/s, which covers every "A/B effect"
  claimed in the DAPS era (+15%, +30%, +52%, −53%, −19%, arm ordering in §16.14).
- Under the generation-on hypothesis the spread is unexplained (same config should not move 2.3×),
  which *also* invalidates the effect sizes riding on cross-arm comparisons of similar magnitude.
  Either way, no DAPS-era throughput verdict is quantitatively trustworthy.
- The DAPS-era "pooled" trio 7.85 / 10.24 / 8.40 are the same story: nominally different stack
  stages, but each within the same-config session spread of its neighbours.

---

## 4. Paper claims (docs/fec-arq-model.md §16.8–16.14) resting on INVALID/UNCERTAIN measurements

The paper has NO §16.15 — the Slow-Path Anchor Diagnosis exists only in goal-gate.md. The paper's
final printed position is therefore §16.14's, which is proven INVALID. Tainted claims:

- §16.9 closing note (fec-arq-model.md:7031-7034): "§16.10 revises this: the C8 cap was NOT purely
  recovery-latency … DAPS lifts C8 0.48×→0.80×" — rests on the UNCERTAIN §16.10 measurement.
  (§16.9's own FMTCP numbers stand.)
- §16.10 (:7038-7133): C8 0.48×→0.80× (13.12); paused 13–68%→0%; r-sweep monotone/r*≈0.03/fixed 0.10
  "wasted ≈34%"; "levers synergistic, each necessary"; the regime-map revision of §16.8/16.9
  (scheduling-bound, not recovery-latency-bound); the ~834 ms slow-path bufferbloat residual; the QM
  addendum (+15% lift, "rate-signal-limited", RTprop pollutes to 1.8 s). ALL UNCERTAIN.
- §16.11 (:7134-7170): pace-all lifts pooled C8 0.37→0.56 (+52%) on both seeds; slow queue halved
  (650–1030→200–540 ms); σ_s collapse; "queue-management arc … each bound realized in turn".
  UNCERTAIN.
- §16.12 (:7172-7209): source backpressure REFUTED (−53% both seeds); "source read is the
  generation-FILL clock"; anchor over-read ×145 makes the gate inert; "the spill is benign, 0.76 of
  the recovery ceiling"; regime map "rate-estimation-bound (closed), repair-pacing-bound (closed)".
  UNCERTAIN (and internally tense — see §2.5).
- §16.13 (:7211-7281): over-read ×158→×1.05 CLOSED; fast bufferbloat 1573→30 ms; C8 regresses
  10.7→9.7; C7 20.96→16.97 "politeness regression"; "true residual is the slow-path deep read-ahead
  (~3–4 s)"; the retrospective "rate-vs-depth framing correction" (:7275-7281). UNCERTAIN, with the
  C7-regression leg refuted-as-noise by §16.14's own symmetric identical-code comparison — which
  also undermines PART 6h's claimed calibration ("reproduces 20.96→16.97 exactly").
- §16.14 (:7283-7351): INVALID in full — arm ordering/stability (identical inert arms), "slow anchor
  never establishes", "bulk C8 structurally bounded below fast-path-alone", "not economically
  aggregatable", the CONSOLIDATE recommendation, and the claim that PART 6h "proves the model is not
  too coarse" (it was calibrated against a noise artifact). The goal-gate diagnosis refuting this is
  NOT yet reflected in the paper.

Unaffected in §16.8–16.9: the §16.8 arc conclusion itself (systematic-era runs recorded their
`RWM_EXTRA` flags; C8 systematic ≈ parity 14.37–15.30, plain 5.43, tail-latency/predictability wins),
the FMTCP L1 numbers, the oracle models *as models* (PARTs 5–6h are self-contained simulations;
what is tainted is each PART's claimed L1 confirmation/calibration), and all single-path ceilings
(single-c2 ≈ 16.3–16.8, single-c3 ≈ 3.1–3.3 — generation-independent and stable across sessions).

---

## 5. The two "honest negatives", specifically

**(a) Source backpressure REFUTED (#73).** `RWM_SRC_BP` is NOT active without generation
(net/mod.rs:3386 → :5324/:5391; the TUN-read defer is entirely inside `src_bp_on`). So if the DAPS
stack was inert, both arms were byte-identical and the 14.99-vs-7.00 split is not explicable by the
code — only by session/VM drift (documented at up to 2.3× for the same config across sessions, with
that day's own single-c2 ceiling bimodal) or by the runs actually having had generation on
(unrecorded). The quoted mechanism evidence (`paused=100%` stretches) occurs in plain mode in both
arms (store-gate stalls), and the section itself says the gate "rarely engages" — inconsistent with
it causing −53%. VERDICT UNSAFE: re-measure interleaved with `cod>0` verified before treating
"source is the pipeline clock" as a finding. (The ship decision — default OFF — is harmless and can
stand on prudence alone.)

**(b) Rate-sample C7 regression 20.96→16.97 (#74).** Same structure: `RWM_RATE_SAMPLE` is dead
without generation, so the arms were identical code if the stack was inert; and §16.14's own
symmetric B-vs-C comparison (provably identical code even WITH generation) measured 21.20 vs 16.96 —
a 20% swing that is pure noise by construction. The C7 regression, and the "rate-throttle
politeness-idle" narrative and PART 6h calibration built on it, should be treated as noise until an
interleaved, generation-verified re-run says otherwise. The ×158→×1.05 anchor DIAG cannot have come
from an inert sender log (no anchor feed exists in plain window mode), so it either came from an
unrecorded generation-on run or a wrong log — it cannot be attached to the throughput battery.

---

## 6. What must be re-measured vs what stands

**Stands (evidence sound):**
- §16.8 consolidated arc verdict + all systematic-repair-era numbers (flags recorded in ledger).
- §16.9 FMTCP L1 (RWM_FMTCP self-enables): C8 0.48× r=0.10 / 0.67× r=0.20, C7 ×1.62, paused 13–68%.
- Plain window-reliable reference points: C8 5.43 (50 MB) / 8.70 (25 MB, σ huge), C7 ~17.4.
- Single-path ceilings (single-c2 ≈ 16.5, single-c3 ≈ 3.2) and dnf=0 reliability in every arm.
- Slow-Path Anchor Diagnosis (generation-on, sender-log): anchor establishes; BtlBw_slow decode-
  clocked, ~4000× swing; §16.14's CONSOLIDATE premise refuted; "FIXABLE" framing.
- Unit tests / oracle simulations as such (their L1 "confirmations" do not stand).

**Must be re-measured (with `RWM_EXTRA=--window-generation-coding` or `RWM_FMTCP=1`, `cod>0` +
`eff_pace>0` verified in `/tmp/rwm-c.log`, arms interleaved, sender log only, env recorded in the
ledger):**
1. DAPS on/off + r-sweep at C8/C7 (§16.10 headline 0.80× and r*≈0.03).
2. QM cap/pace A/B (§16.10 addendum).
3. Per-path estimator A/B incl. est=Y% (needs an explicit off-knob or two-binary protocol declared).
4. Pace-all A/B (§16.11 +52%).
5. Source-backpressure A/B (§16.12 REFUTED — currently unsafe).
6. Rate-sample A/B + anchor over-read DIAG (§16.13, incl. whether ×158 exists at all under a live
   estimator; the later diagnosis suggests the real signal problem is instability, not over-read).
7. Depth-bound three-arm (§16.14 — fully void).
8. Paper: add the §16.15 correction; §16.14's CONSOLIDATE and the §16.10 revision-note must not be
   cited until re-measured. (Worker #77 on feat/gen-on-rebaseline is re-baselining; this audit made
   no repo edits.)

**Process fixes surfaced:** record full command line + env per L1 battery in the ledger; assert
`cod>0` in the harness when a generation-dependent gate is set (or make `RWM_DAPS` self-enabling at
net/mod.rs:699 like `RWM_FMTCP`); fix the `.is_ok()` footguns (`RWM_FMTCP=0`, `RWM_DAPS=0` count as
ON); always read `/tmp/rwm-c.log` for sender DIAG; interleave A/B arms.
