# VISION-TRIAGE-2026-07 — feature triage for the code-consolidation pass

**Date**: 2026-07-21 (branch `docs/adr-migration`)
**Companion**: ADR-0052…0067 (the decision index), goal-gate.md
DEPRECATION REGISTER (the argued dispositions), the code-surface line
references below (approximate, surveyed at commit c3a9d76).

## The vision (the paper's own axioms)

ONE continuous mechanism parameterized by (δ, ρ, r); no mode switches;
no magic constants — derived laws on measured anchors; simple,
maintainable code. Every feature below is classified against those axioms
AND its measured record. Classification: **CORE** (is the product),
**EXPERIMENT-KEEP** (gated, argued future value), **REMOVE** (schedule
deletion in the code-consolidation pass), plus the **STREAMING MACHINE**
as its own entry.

Counts: CORE 12 surfaces · EXPERIMENT-KEEP 9 · REMOVE 8 chains
(~1,000–1,500 LOC of gated mechanism code) · 1 retirement path
(streaming, ~1,230 LOC, re-test-gated).

---

## 1. CORE — keep; this is the product

All default-ON (or the default policy surface), each LOO- or
battery-defended, each a derived law on measured anchors:

| surface | why it is the product | ADR |
|---|---|---|
| Unified span machine (`fec/unified.rs`, `RWM_UNIFIED` + `RWM_UNIFIED_SHED` + `RWM_ASTAR_ANCHOR` + `RWM_TAPER_R`) | THE vision realized: one decoder, one (δ,ρ,r) span law A\*/M\*/Δ, δ-honest shedding; no mode switch | 0064 |
| Substrate CC policy (`RWM_QUIC_CC`: bbr default; cubic/newreno/passthrough) | wall #1's fix as policy; hint-priced controller selection is the endstate | 0054 |
| Copa wire-signal feed (`RWM_COPA_FEED`/`_WIRE`/`_DELTA`, scheduler Copa-lite) | the δ-capable controller — the latency-priced arm of the policy surface; δ from the hint, no new constants | 0062 |
| MTU floor 1350 (`RWM_MTU_FLOOR`) | wall #2; ships ON | 0055 |
| SACK-clocked store release (`RWM_STORE_SACK_RELEASE`) | wall #9; slot release ≠ recoverability | 0060 |
| Path-scaled outstanding pool (`RWM_STORE_PATHS`, `RWM_STORE*` sizing knobs) | wall #7; the derived flow-control law (c8-aware law = named follow-up) | 0058 |
| Multipath recovery suppression (`RWM_RECOV_MP`, `_LAW`) | wall #8; RFC 9002 per path | 0059 |
| Anchor hygiene (`RWM_ANCHOR_HYGIENE` umbrella: `RWM_MSTAR_ANCHOR`, `RWM_CLOCK_GAP`; `control/anchor.rs`) | the three anchor laws — the precondition of "derived laws on measured anchors" | 0061 |
| r\* solver + mass-quantile term (`RWM_RSTAR_TAIL`, `control/fec_rate.rs` incl. TaperBudget, BOCD/estimator) | the r of (δ,ρ,r), honest about feasibility | 0063, 0050 |
| Systematic-repair generation wire + sparse-aware cost model | the bulk realization the unified decoder block-diagonalizes to | 0056 |
| The instruments: `RWM_DIAG`/`RWM_RDIAG`/`RWM_FDIAG`/`RWM_TRACE`, L0 netem shim (`RWM_L0_*`), gauges, `tools/l1/*` harness, gate_suite | the measurement discipline is load-bearing (ADR-0052 items are un-satisfiable without them) | 0052 |
| `config::env_flag`/`deprecated_env_flag`/`anchor_gate` (config.rs:376–417) | the gate hygiene layer | 0052/0066 |

Consolidation note (not removal): `net/mod.rs` is a 12,696-line gate hub;
the code-consolidation pass's biggest simplicity win is not deletion of
CORE but extraction — the gate block (~4838–8280) reads ~70 env vars
inline. After the REMOVE list lands, fold the surviving defaults into
plain code paths (drop the `=1` branches where the `=0` arm is the only
alternative and is register-retained).

---

## 2. EXPERIMENT-KEEP — keep gated; argued value

| gate | surface (file:lines) | argument for keeping |
|---|---|---|
| `RWM_PLAIN_RS` (+ `RWM_RS_ATTR`, `RWM_RS_TRACE`) | net/mod.rs 1555, 5602 + sampling machinery 812–906 | LOO pending: composition probe showed its witness cost RESOLVED (best-or-equal c8 arm at s7); named flip candidate riding the c8-aware pool battery. Also the honest-anchor input any percap cap law needs. |
| `RWM_STORE_PERCAP` + `RWM_PERCAP_GUARD` + `RWM_HONEST_CAP` | net/mod.rs 5522–5660, 11270–11700, PerPathAccount ~4077–4390 (~400–550 LOC) | c7 percap ≥ pooled both seeds with the pooled collapse mode absent — the SYMMETRIC-cell tool; the c8-aware pool follow-up may compose it. Class B, successor scheduled. |
| `RWM_STORE_BORROW` | net/mod.rs 5556 + loan law | measured law-perfect (loans ≡ 0 at c7 by theorem AND gauge) — a working derived law whose tax verdict could flip if the c8 pool law changes; cheap to keep, §16.22 documents it. Downgrade to REMOVE if the c8-aware pool law ships without it. |
| `RWM_COPA_COMPETE` | scheduler/mod.rs 206 + mode-switch fields | faithful Copa §2.2, unit-proven; its cell is unwinnable until the CC-independent contention-recovery blocker (named successor) lands — the mechanism is right, the substrate isn't ready. |
| `RWM_GEN_PIPE` (+ `RWM_GEN`, `RWM_GEN_R`, `RWM_GEN_RATE*`, `RWM_PIPELINE`, `RWM_GEN_INFLIGHT`, `RWM_REPORT_GENS`) | net/mod.rs 1969/2202/4906 + M\* depth law | carries the derived M\* law and the engaged knee (+25–82%); the generation-default question inherits it (ADR-0067's anchor-pair verdict). Gen mode is the measured STABILIZER at multipath cells. |
| `RWM_PROACTIVE_PACER` | net/mod.rs 5898 + emission block ~8208–8280 (~100–150 LOC) | the documented resolution of the frontier/inline family — its null IS the presence⊥throughput identity's evidence arm; keep until the identity is textbook (cheap), or fold its citation into ADR-0066 and remove with the frontier family in a later pass. Borderline: acceptable REMOVE if the pass wants it. |
| `RWM_QUIC_CC=cubic`/`newreno` arms | transport/quic.rs 167 | the fairness-conservative opt-out and control arm (ADR-0054 keeps Cubic deliberately). |
| Harness/bench knobs (`RWM_L0_BACKEND`, `RWM_B_*`, `RWM_SL_*`, `RWM_PERF_TIMEOUT_S`, `RWM_WEDGE_CONTROL`, `RWM_TEST_EF_*`) | tests/tools only | instruments, not product gates. |
| `RWM_REASM_BDP`, `RWM_OOO_RETAIN`, `RWM_INFL_CAP`/`_BDP`, `RWM_CC_PACE`/`_HR`, `RWM_REACT_CAP`, `RWM_MIN_R`, `RWM_REPAIR_WAIT`, `RWM_PLACE_T` | net/mod.rs + scheduler (small individual gates) | sub-levers shared by live experiment chains (percap, gen-pipe, FMTCP-era); triage them WITH their parent chain in the consolidation pass — delete the ones whose only parent is a REMOVE entry. |

---

## 3. REMOVE — schedule deletion in the code-consolidation pass

Register rows with no re-test owed, plus vision-based removals. Work-list
with code surface (from the 2026-07-21 survey; sizes are clean-removal
estimates):

| gate | code surface | est. LOC | argument |
|---|---|---|---|
| `RWM_SACK_PRUNE` | net/mod.rs 2019–2023, 5505–5515 guard, prune branch ~7087–7100, comments 4577/5494/12183 | ~70–90 | **Register: deprecate-HARD, removal next pass.** Structurally unsafe (destroys recoverability); goal achieved safely by ADR-0060. Kept this pass only as the precedence-warned control arm — that role ends when SR's first post-ship battery cycle closes (it has: §16.25 + consolidation). |
| `RWM_RECOV_MP_SERIAL` | net/mod.rs 6054–6060, mp_batch_ctr 6074–6075, serial branch 6084–6095 | ~30–45 | Refuted ON the clean substrate (×2.4 CPU, signal re-heat); register: no re-test. Its diagnostic value is recorded in §16.24; a future cheaper implementation is a NEW pre-registered build, not this code. |
| `RWM_INLINE_REPAIR` + `RWM_INLINE_W` | net/mod.rs 5873–5878, 5914–5921, emission ~8143–8200 | ~100–140 | Refuted on substrate-independent GEOMETRY (grid-stranding); superseded via PROACTIVE_PACER → presence⊥throughput (structural, re-confirmed post-divide). Vision: a second, non-derived emission grid contradicts one-mechanism. |
| `RWM_FRONTIER`/`_GAIN`/`_R`/`_OFFSET` | net/mod.rs 5788–5843 + frontier_debt/emission ~5922+ (+FDIAG couplings) | ~150–220 | Refuted on geometry (repair anchored at the ack frontier loses to its own ARQ); superseded twice (pacer; then the unified TRAILING span, which is the correct realization of the same intent). Vision: the unified span law IS this idea done right — keep the FDIAG counters, delete the mechanism. |
| `RWM_RATE_WIRE` + `RWM_RATE_Q` | scheduler/mod.rs 774/778 + robust-quantile machine (~300–360, 606, 633, 965, 1506, 2051, 4774) | ~80–120 | Refuted by its own structural argument (decode-clocked samples ⇒ any sub-max quantile under-reads); need met by the honest-anchor family (ADR-0061). Vision: a de-noise band-aid vs a measured-seed anchor — the anchor won. |
| `RWM_SRC_BP` | net/mod.rs 5006–5011 + src_bp pause logic | ~50–90 | Era verdict uncertain, but the mechanism space (defer source into per-path budgets) was re-asked BY the percap family on live code with gauges and lost for a named structural reason. Vision: source is the pipeline clock — holding it is a mode switch on the data path. Remove; the register's "rides any future gen-mode consolidation" clause transfers to the ledger text, not the code. |
| DAPS chain: `RWM_DAPS`, `_BDP`, `_PACE`, `RWM_PACE_ALL`, `RWM_RATE_SAMPLE`, `RWM_PER_PATH_EST`, `RWM_DAPS_DEPTH` | net/mod.rs 4862–5067 (gate block ~200) + estimator per-path rate + scheduler SendPacket rate-sample 1900–1950 + depth limiter + pacing buckets | ~450–700 | **Recommendation below.** |
| `RWM_FMTCP` + `RWM_FMTCP_WIN` | net/mod.rs 1278, 4874, 4933–4988 + doc blocks 4856–4874, 7262 (excl. shared sub-levers ooo_retain/reasm_bdp/xpath) | ~120–170 | **Recommendation below.** |

Also in the REMOVE sweep (vision-based, small): `RWM_XPATH_REPAIR`,
`RWM_CODED_SRC`, `RWM_NO_REACTIVE`, `RWM_PFRAC` — measured-era experiment
arms whose parent chains are all above; audit each in-pass and delete
where the only consumer is a REMOVE entry.

### The DAPS re-test question — recommendation: REMOVE without further VM time

The register formally owes a re-test (refuted with W1/PRE-DIV active).
Honest argument against spending it:

1. The mechanism space already GOT its clean-code re-test: "Gen-ON Stack
   Ablation" ran the same stack with generation actually ON and the
   stack itself was the C7 collapse (rate-sample −22%, depth −20…−30%).
   What that ablation lacked (BBR substrate, post-divide) matters for
   ABSOLUTE numbers, not for the finding that the stack harms its own
   cells.
2. Every surviving idea in the chain was re-derived better elsewhere and
   is CORE or EXPERIMENT-KEEP: per-path BDP cap + derived depth →
   `RWM_GEN_PIPE`'s M\* law (ADR-0064); honest per-path anchors →
   ADR-0061; per-path admission → the percap family (ADR-0058). A DAPS
   re-test could at best resurrect a worse-derived duplicate of laws the
   vision already ships.
3. DAPS is generation-mode-only; the shipped default is plain-mode. The
   one live win (`RWM_DAPS_DEPTH` hetero C8 +8%, gen mode) predates the
   substrate walls; the unified M\* depth law occupies the same lever.
4. Vision axioms: the chain is seven interacting knobs with tuned
   constants — the opposite of one mechanism with derived laws.

If the next GENERATION-mode consolidation battery is scheduled anyway,
one composed arm (GPB stack ± `RWM_DAPS_DEPTH`) is a cheap piggyback and
would close the register clause with data; do NOT schedule VM time for
DAPS alone. Deletion order: gate block first (safe — deprecated-warned,
default-off), then the estimator/scheduler per-path remnants not shared
with `RWM_GEN_PIPE`/`RWM_PLAIN_RS` (audit `source_path_map` and pacing
buckets for shared use before deleting).

### The FMTCP re-test question — recommendation: one piggybacked battery, else REMOVE

The register calls FMTCP the strongest re-test case (refuted
pre-EVERY-wall; its named failure mechanism — recovery over a
bufferbloat-inflated RTT — is exactly the W7/W8 class later fixed).
Honest counter-arguments:

1. Its failure reproduced FMTCP's own published pathology (slow subflow
   = bottleneck), which is architecture, not substrate; and the
   clean-substrate c8 story (§16.22 no-borrowing tax, the c8 WATCH)
   still names that same structural axis.
2. What FMTCP wanted — sender decoupled from the in-order frontier —
   the default stack now ACHIEVES safely and better: SACK-clocked slot
   release + path-scaled pool + per-path recovery clocks are the same
   goal as derived laws (ADR-0059/0060), measured to 0.98–1.05×Σ at c7.
   The composite's surviving pieces already live on: per-path BDP cap
   (GEN_PIPE), the win backstop now DERIVED as (M\*+2)·G (ADR-0061).
3. Vision: decode-on-total is a MODE (a second delivery contract bolted
   beside the window law), not a δ-point of the unified mechanism.

Recommendation: since the c8-aware pool battery (ADR-0058's WATCH) is
already the next pre-registered VM session, add ONE FMTCP arm at c8/c7
(gen-sys, current defaults, ~30 min of VM time) to close the register's
strongest clause with data. If that session doesn't happen this quarter,
remove anyway on the supersession argument — record the removal in the
register as "superseded by the default stack's own FC; re-test clause
discharged by supersession".

---

## 4. STREAMING MACHINE — its own entry (retirement path)

**Surface**: `src/fec/streaming.rs` (352 LOC adapter) + the
`streaming-codes` crate (845 LOC: decoder 547, encoder 194, params 74,
lib 30) + selection glue (~30 LOC: net/mod.rs 1216–1231, 9428–9483,
`is_streaming()` 573, config.rs 277, backend_selector.rs 129,
fec_rate.rs `compute_streaming_params` 376–393). Total ~1,230 LOC.

**Standing**: not refuted — DISPLACED (ADR-0064). Retained as the
`RWM_UNIFIED=0` opt-out, no activation warning. The register's re-test
clause stands and is honest: the 12–48× message-tail crown record spans
HISTORIC cells (L2/L3 message-tail batteries, quinn-vs-rp Metric A) the
flip battery did not re-run. The vision says one machine; streaming is by
construction a second machine and a mode switch.

**Recommended retirement path** (two stages, per ADR-0066):

1. **The historic-crown re-test battery** (one VM session, ~half a day):
   re-run the Metric-A message-tail cells (quinn + kernel-TCP baselines
   vs rp) and the L2/L3 tail-battery cells on the unified default,
   `stream` vs `unified` arms interleaved, both seeds, per ADR-0052.
   Gate: unified holds the 12–48× class (≥ streaming within the rep
   spread) at EVERY historic cell. The 2026-07-21 battery already showed
   unified ≤ streaming at all 8 current cells, so the prior is strongly
   favorable; the session is confirmation, not exploration.
2. **Removal pass** (~1–2 dev days): delete the adapter + crate +
   selection glue; `FecBackend::Streaming` becomes a parse error with a
   pointer; `RWM_UNIFIED=0` then means "legacy RLC decoders" only —
   fold that arm's remaining value into the differential test suite
   (the legacy decoders retire on the same battery per §17.5, so stage 2
   can take all three legacy machines out together: streaming.rs +
   streaming-codes + `RlcWindowDecoder` + pre-rewrite reference decoder
   arms, keeping the differential fixtures as recorded traces rather
   than live code).

If stage 1 finds ANY historic cell where streaming still wins, the
machine stays as the documented (δ, ρ) point for that cell and the
register entry is updated — that outcome is a finding, not a failure.

---

## 5. Follow-ups this triage does NOT preempt

Named, pre-registerable, owned by the roadmap (not this document): the
c8-aware pool law (+`RWM_PLAIN_RS` as a stack member); the
shared-bottleneck contention-recovery successor (gates any Copa default
flip); the within-deadline P_fec ρ-budget refinement; the gen-mode
consolidation battery (which inherits the anchor pair's knee win and the
DAPS/FMTCP piggyback arms above).
