# The interactive simulator under the three knobs (plan, no code)

**Question.** How must the L0 model (`raptorpath-wasm` and `raptorpath-visualizer/`) change to show the proposed knobs, lifetime T, delay price δ and order hold H, with the delivered share ρ as a reported outcome? The proposal is `github/three-knobs:docs/derivations/three-knobs.md` (§1–§10, then Revision 1 §13, Revision 2 §15 and the stage 3c findings §16; later sections supersede earlier ones). The owner likes it but has not accepted it, so everything here lives on a branch and must not replace main's model.

**Base.** `github/main` `70b4b066`. Toy: `github/toy-stage3-knobs` `02eda90b` (`tools/toy/toy2.py`, arm S4). Line numbers refer to the base.

**Short answer.**
- **Adjust; do not rewrite.** The model's coupling to ρ is one 30-line give-up block, one clamp, one finish term and five accessors. Its hint coupling is now one construction-time alias table. What T and H need is new state: hand-over stamps, an intake, an outage, the floor and a consumer stage. Every piece of it is inert at T = ∞, λ = ∞ and H = ∞ by IEEE arithmetic, so the six goldens, the 39 native tests and both page gates must pass unmodified. That is the identity gate, and it is cheaper and stronger than any rewrite could offer (§2).
- **Two pages, one wasm.** Main's page is rebuilt from the branch's wasm with its script unchanged, and its gates are unchanged. A new `knobs-visualizer.html` shows T, δ and H, the workload and an outage, with ρ only as readouts. No page and no model selects between the two (§3, §4).
- **The laws live in one module,** `raptorpath-wasm/src/knobs.rs`. It mirrors `toy2.py`'s law functions one to one and is checked against vectors generated from the toy. Nothing moves to `raptorpath-core` or `raptorpath-math` before the owner accepts the proposal (§5).
- **Eight phases** (§7), each with its own done criteria. Owner questions are in §8 and what the model cannot express is in §6.

**Advisor review (Fable).** It was consulted before the approach was fixed, and it changed four things:
- **The adjust case rests on state, not on the 27 sites.** The brief's premise was the audit's 27 hint-keyed sites. Those were counted at `2d3b2069`, and most were removed by `d9a2a3d4`/`0857a0cd` (§2.1). What decides adjust against rewrite is whether every addition can be inert state at T = ∞.
- **ρ stays a readout on the knobs page.** An earlier draft of this plan had one superset page with ρ, T and H all live. Item 2 of the brief makes ρ a readout, so that draft was dropped for two pages.
- **The composed corner is not a law.** A ρ budget composed with a T floor appears in neither the paper nor the derivation (FORMULA-FIRST). It is made unreachable from both pages and gated (§3.3, Q-V2).
- **Model-vs-toy consistency covers only the ARQ half.** The toy has no proactive repair, so T × repair is model-only, and L1 needs an absolute test over repair members (§5.3, §6).

It also reviewed the finished plan; that review is recorded in §9.

## 1. What the model is today

`raptorpath-wasm/src/lib.rs` is 3 316 lines: about 1 900 of code and 1 420 of tests (39 `#[test]`s). `Simulation::step` (:1201–:1594) runs one 1 ms tick:
1. per-path feedback (a per-symbol oracle ack after the path RTT);
2. the rate from `controller_rate_now`;
3. a slot loop with fixed priority: P_lost-gated retransmit, then repair on debt, then new source;
4. the give-up block (:1463–:1495);
5. the ack, finish, per-path clock and frontier gauges.

The source is **saturating**: 2 000 symbols are all available at t = 0 and are sent as fast as the slots allow. The goldens finish in 569–754 ticks. There is no application rate, no intake and no hand-over time; `send_tick` is the first send. Forward flight is instantaneous: the one-way delay is added only in the latency accounting (`record_delivery`, :995, using the capacity-weighted RTT/2). There is no outage, no link queue, no gap report and no floor. Delivery is recorded at the decode instant, in decode order, and `frontier` is only a gauge.

The page (`interactive-visualizer.html`, 3 137 lines including the embedded glue) has these panels:
- paths;
- the δ and ρ dials;
- the delivery timeline;
- the span cartoon (a JS-only animation of the span law);
- metrics, store, latency and multipath;
- the (r, δ, ρ) triangle solver;
- the wall chain;
- the model-vs-engine table.

`build_visualizer.sh` builds the wasm, checks `raptorpath-core` on wasm32 together with its deny list, injects the init block (glue + `SimWrapper`) into the page, and runs `test_visualizer.mjs` and `test_visualizer_ui.mjs`. CI runs it on every push (`ci.yml:93`).

## 2. Adjust or rewrite

### 2.1 Inventory: every site that reads ρ, the hint, the give-up law, the hold or the order, and what each becomes

**`raptorpath-wasm/src/lib.rs`**

| site | what it does | under T/δ/H |
|---|---|---|
| `rho` field :435; constructor `custom_rho.unwrap_or(1.0).clamp(0.9, 1.0)` :1095 | the declared ρ | **kept, inert.** The knobs page never passes ρ, so ρ = 1 and the budget is `min(0, R) = 0`. It is removed at acceptance (Q-V2) |
| give-up block :1463–:1495 (`age > D(δ) ∧ given_up + 1 ≤ min(1 − ρ, R)·src_total`) | today's give-up law (option (ii), `giveup-law-model.md`) | **kept, inert at ρ = 1.** The lifetime's giving up is the floor (§15.3), a separate block |
| `giveup_budget` :236, `get_giveup_budget/residual/deadline` :1826–:1840 | budget readouts | kept for main's page; the knobs page does not read them |
| `given_up_seqs` filters in `send_repair` :985, `record_delivery` :998, the retx arm :1329 and the source arm :1433 | late data for a given-up seq is discarded | **unchanged.** Floor-passed seqs are *not* added to it: a late fill of a passed hole reaches the consumer out of order (O3 = yes, three-knobs §2.3) |
| finish :1512–:1520 (`cum_decoded + given_up ≥ num_source`; no lost unrecovered) | end of run | **gains `+ intake_expired + floor_passed`**, both 0 at T = ∞ |
| frontier gauge :1566–:1571 | contiguous delivered or given-up prefix | it also skips floor-passed seqs (0 at T = ∞) |
| `get_reliability` :1814 (decoded / total_src) | today's ρ readout | **kept.** The knobs page reads `share_any` and `share_T` instead (§4.3) |
| hint alias table :1080–:1086 | `bulk → 0.005`, `realtime → 50`, `continuum → custom_delta`, else 0.5 | unchanged: construction only, class A. The knobs page passes only `"continuum"` |
| `fixed_r` :1088, :735, :914, :1786 | the r-override (audit class D, owner's word pending) | unchanged; not reachable from either page |
| `legacy_*` (`#[cfg(test)]`) :427, :432, :731, :931, :1449 | test-only ablations (class C) | unchanged |
| `continuum_tail` β guard :797 | `δ_eff` readout only (audit W8, class C) | unchanged |
| `send_tick` :447, latency from first send :1001 | latency readout | **kept** as the arrival-delay readout. The new delay is measured from hand-over, at the consumer (§4.3) |
| `completion_chi_raw` :688 (`t_rem = remaining / capacity`) | completion exposure | **`t_rem = remaining · max(1/capacity, 1/λ)`.** For a stream the end comes at the hand-over rate. At λ = ∞, `1/λ = 0`, so it is unchanged |
| source arm :1361 (`!source_done`, pool check) | sends the next symbol | **gains `h(next) ≤ now`**, true at λ = ∞, and L1 plus the dequeue expiry at the intake |
| retx candidate :1270–:1278 | lost, unrecovered, own-RTT timer | **gains `seq ≥ F_ann` (the commitment) and `resend_admissible`** (L2) |
| encoder slide :1376–:1379 (count W) | the repair window | **gains the L1 age edge**: no member with `a ≥ T` (§7, P3) |
| the receiver hold and out-of-order | **absent**: the model has no reorder stage | new consumer stage (L6), output only (§7, P4) |

**`interactive-visualizer.html` (page script)**

| site | under T/δ/H (knobs page; main's page is unchanged) |
|---|---|
| `sl-rhoc` :142 and its labels :2117–:2124, `rho-readout` :2608–:2618 | replaced by the T and H dials and the share readouts |
| cartoon `spanLaw` :2716–:2745 (`giveup_budget(userRho, derived)`, `binds = (1 − ρ) < R ? …`) | the cartoon passes a hole once `protected(...)` (wasm export) is false. D(δ) stays as the span width. The budget text goes |
| triangle panel :321–:348, :2144–:2157 (`solve_*_rho`, `find_t_cut`) | omitted on the knobs page; it is main's (δ, ρ, r) form (Q-V4) |
| `const hint = 'continuum'` :2052 (V2) | the same on the knobs page, gated (§5.4) |
| preset buttons :2951– (V14) | named points of (T, δ, H) (§4.2) |
| captions V7–V13, V15 (text only, class C) | rewritten for the knobs. No T/H comparison is allowed even in text (§5.4) |

**Recount of the audit's sites on the base.** The audit's 27 lines (`hint_bulk|legacy_|fixed_r`) are now 19 outside the tests. None of them is a class-B decision site in `lib.rs`:
- F1/F2/W11/W17/V1 went in `d9a2a3d4`;
- W14 and V5/V6 went with the give-up law (ii) in `c42331e9` and `e0a2f777`;
- the one live B site left is W22 (`p_fec_normal`'s cut), in `raptorpath-math`, with its bounding test.

So the "27 hint-keyed sites" do not argue for a rewrite today.

### 2.2 Verdict: adjust

- **What the knobs need is missing state, not tangled code.** Every addition is either a gate on an existing arm, an extra state machine running next to the existing one, or an output stage:
  - gates on existing arms: L2 and the commitment on the retx candidate, the age edge on the encoder, the hand-over gate on the source slot;
  - extra state machines: the floor and its repeat clock, the brake's delayed slot release;
  - output stages: the consumer and the readouts.

  None reorders the slot loop's priority (retx, repair, source).
- **Each is inert at T = ∞, λ = ∞, H = ∞ by IEEE arithmetic** (§3.1). So the existing suite is the identity witness, unmodified:
  - the six golden rows (no `GOLDEN_CAPTURE`);
  - the 39 native tests;
  - `test_continuum_one_law_across_the_dial`;
  - both `.mjs` gates on main's page.

  A rewrite would lose that witness and would have to re-derive the rate, FEC, multipath and store machinery, about 1 900 lines, that the proposal leaves unchanged (L5: r(β(δ)) is untouched).
- **When to switch to a rewrite.** If any phase finds that an addition must reorder the slot loop, or cannot be inert at T = ∞ without a test on T, the worker stops and returns. That is the evidence for a rewrite.

## 3. Keeping main's model alongside, with no mode switch

### 3.1 One construction path, with ∞ as a value

`Simulation::multipath_knobs(eps, q, rtt_ms, capacity, w, hint, fixed_r, custom_delta, custom_rho, lifetime_s, hold_s, source_rate, num_source, outage_start_s, outage_len_s)` is the one constructor. `multipath(...)` delegates to it with `(∞, ∞, ∞, 2000, outage_start = ∞ per path, outage_len = 0 per path)`, as `new` delegates to `multipath` today (the N = 1 precedent):
- hand-over `h(u) = u / λ` gives `0` at λ = ∞ (the object: everything handed over at t = 0);
- `a + ô < ∞` is true;
- `a ≥ ∞` is false;
- `min(t_det + ∞, t_F + H_f)` is `t_F + H_f`, and that is `∞` when no floor is ever sent;
- the outage window `[∞, ∞)` is empty.

No `is_infinite`, `is_finite` or `Option`-selected branch appears anywhere (§5.4).

### 3.2 Two pages from one wasm

- `interactive-visualizer.html` is main's page. Its script is unchanged on the branch. `build_visualizer.sh` rebuilds it with the branch's wasm and runs its two gates unchanged: that is the witness that main's model behaviour survives.
- `knobs-visualizer.html` is new. It starts as a copy of main's page source; P6 removes the ρ dial, the ρ readouts and the triangle panel, and adds the knobs. The copy is deliberate. On acceptance it replaces main's page; on rejection it is deleted. Factoring the shared script out first would change main's page on the branch for no gain.
- **Comparison** is quantitative through the identity gates (the knobs model at T = ∞ equals main's at ρ = 1). It is visual through the two pages side by side, both built in the same `build_visualizer.sh` run. A "pin this run" button on the knobs page keeps one run's readouts beside the live run. It is a display snapshot and holds no model state.

### 3.3 The ρ × T corner

ρ < 1 together with T < ∞ is reachable only through the ABI:
- main's page never calls `multipath_knobs`;
- the knobs page always passes `custom_rho = null`.

No law covers that corner, so it is not a model point. `test_knobs_ui.mjs` asserts `custom_rho == null` at every dial position, and a native test asserts that the knobs-page constructor path yields `get_rho() == 1`. Nothing in the model tests for the corner. The ρ code is deleted at acceptance (Q-V2).

## 4. The knobs page

### 4.1 Dials

Each dial is one continuous map from slider position u to a plain f64, and both ends are values:

| dial | map (u ∈ slider range) | ends | named nudge point |
|---|---|---|---|
| T, s | `T = 0.4·(u/(1 − u))²`, u ∈ [0.1, 1] | u = 1 gives `1/0 = ∞` in JS (IEEE), labelled "∞, never give up"; the minimum is about 5 ms (T = 0 is a config error, §3 of the derivation) | 0.4 at u = 0.5; 2 at u ≈ 0.691 |
| δ | unchanged (log slider, [0.005, 50]) | — | 0.005, 0.5, 50 |
| H, s | `H = 0.02·(u/(1 − u))²`, u ∈ [0, 1] | u = 0 gives 0 (any order); u = 1 gives ∞ (strict) | 0.02 at u = 0.5 |
| offered load λ/C | `λ/C = u/(1 − u)`, u ∈ [0.05, 1] | u = 1 gives λ = ∞, the object | 0.5, 1.2 |
| outage | start (s) and length (s), applied to all paths (Q-V7) | length 0 is an empty window | — |

C is the aggregate goodput, Σ capᵢ·(1 − εᵢ)·1000 symbols/s. The page passes `source_rate = (λ/C)·C`, and `∞·C = ∞`.

The exponent 2 and the scales are presentation choices that put the named points mid-slider. They are not laws, and no gate depends on slider steps: the gates call the model at exact values.

**Display of ∞.** It is `x.toFixed(3).replace('Infinity', '∞')`, which needs no comparison. ρ is not a dial.

### 4.2 Named points

Each button writes a fixed triple to the sliders, from three-knobs §4 and §13.3. The hand-over kind is in the button's name, not in the model, and nothing downstream reads which button was pressed:

| button | T | δ | H |
|---|---|---|---|
| Bulk | ∞ | 0.005 | ∞ |
| Auto | ∞ | 0.5 | ∞ |
| Realtime, object | ∞ | 50 | 0 |
| Realtime, call or datagram | 0.4 | 50 | 0 |
| Object API | ∞ | (δ kept) | 0 |
| Buffered video (owner's example) | 2 | 0.005 | 0 |

There are separate workload buttons: object (λ = ∞), stream at 0.5·C, overload at 1.2·C. A preset never reads the workload: writing a T that depends on whether λ = ∞ would be the mode bit §13.3 forbids. That is why "Realtime, object" and "Realtime, call" are two buttons. There is no preset highlight for T or H, because it would need a comparison; δ keeps its existing highlight (V10, class C).

### 4.3 Readouts

All readouts are outputs of the run. None is fed back.

| readout | definition (model) |
|---|---|
| `share_any` ("delivered at all"; labelled ρ, reported) | delivered units / handed-over units, with intake expiries in the denominator |
| `share_T` ("delivered within T") | units with `t_deliver − h + ô_acc ≤ T` (`within_lifetime`), / handed-over units. Exact here: one clock |
| `share_by_ref` | the same at the reference deadline τ = 0.4 s (§13.6's τ; Q-V3), so T = ∞ runs can be compared with finite-T ones |
| delay from hand-over, p50 and p99 | consumer delivery tick − h, plus the one-way accounting term ô_acc, under the current H (L6) |
| arrival delay | today's `get_lat_*`, from first send at decode; kept so it can be compared with main's page |
| recovery time after the outage | `max{ h(u) : h(u) ≥ outage_end, delay(u) > τ } − outage_end` (0 if none). This is §16's requested readout. When the toy registers its own definition (stage 3d), the model adopts it |
| strict stall beyond the outage | the longest consumer stall that overlaps the outage, minus the outage length (§13.5) |
| out-of-order count | units delivered after a higher seq was delivered (the toy's `ooo_loss`) |
| `rel_by_hold`, `rel_by_floor` | which term of L6's min released each hole (FORMULA-FIRST bind gauge) |
| `intake_expired`, `intake_expired_deq` | L1 and dequeue expiries |
| `floors` (repeat datagrams), `floor_carried` | floor traffic |
| `span_cut_by_T` | the fraction of repairs whose window the L1 age edge cut (bind gauge, §2.4 of the derivation) |
| `l1_viol`, `commit_viol` | must read 0 (the absolute tests read the same counters) |
| the law revision | `KNOB_LAW_REVISION`, e.g. "15/16 (Revision 2)", printed in the page footer |

**The cartoon.** The span cartoon keeps A*, M* and Δ from δ. A hole turns grey ("passed by the floor") once `protected(T, now, last_retx, rtt, h, srtt)`, called through the wasm export, is false. A `T − ô` marker replaces the budget text. The page script holds no law formula.

## 5. Laws, consistency with the offline simulation, and gates

### 5.1 One law module

`raptorpath-wasm/src/knobs.rs` holds pure f64 functions, written to `raptorpath-core`'s rules (no I/O, no env, no hint type, no statics). Each is exported through `wasm_bindgen` for the page. The names and argument order mirror `toy2.py`:

| function | form | source | stability |
|---|---|---|---|
| `intake_expired(T, age)` | `age ≥ T` | L1, §2.1 | stable since §2 |
| `resend_admissible(T, age, x)` | `age + x < T` | L2, §2.2 | stable since §2 |
| `within_lifetime(T, age)` | `age ≤ T` | L3 readout | stable |
| `dequeue_expired(T, age, m)` | `age + m ≥ T`, m = R | §13.2 | **young**: one revision, untested with FEC |
| `protected(T, now, last_send, c, h, srtt)` | `max(now, last_send + c) − h + srtt/2 < T` | §15.3 | **unstable**: §16's commitment seam failed in the toy |
| `release_at(t_detect, H, t_floor, H_f)` | `min(t_detect + H, t_floor + H_f)` | L6, §2.6 | stable form; H_f is not confirmed (Q-V5) |
| `ack_timeout(srtt, rttvar, g, ack_delay)` | `srtt + max(4·rttvar, g) + ack_delay` | §13, RFC 6298 | stable |
| `validate_knobs(T, H)` | domain: T ∈ (0, ∞], H ∈ [0, ∞]; NaN is rejected | §3 of the derivation | the only place T and H meet a literal |

- Each function carries a one-line comment: its three-knobs section and the revision it implements.
- **Not modelled: the feedback-only verdict (§15.1).** §15.1 shows it unreachable under §15.3, and §16 measured it never firing. Modelling it would add a law with no executed path.
- **The model's bindings** of the symbols:
  - c = the sim's own per-symbol retry timer, the path RTT in ticks (§15.3's cooldown);
  - ô = the accounting one-way term, the capacity-weighted RTT/2, so that L2 admits exactly the resends that arrive in time in this model;
  - R = `ack_timeout(agg_srtt, agg_rttvar, 1 ms, 1 ms)`, with g and ack_delay as in the toy's tick.

### 5.2 Keeping the laws movable while Revision 3 is written

- **Only `knobs.rs` holds a knob law.** `lib.rs` only calls it. A revision changes `knobs.rs`, the vectors (§5.3) and the knob goldens (§5.5). The identity gates must keep passing unmodified: a revision that breaks the T = ∞ identity is a finding against the revision, not a reason to re-capture.
- **The worker never invents a law form.** If Revision 3 introduces a law without a formula in the derivation, that phase stops and returns.
- **Nothing moves to `raptorpath-core` or `raptorpath-math` on this branch.** Both crates are shared with the engine on main, ADR-0077's stop rule asks that a shared decision come from the engine, and the engine has none of these laws yet. At acceptance, the stable ones (`intake_expired`, `resend_admissible`, then `release_at`) move first, as the engine sketch's step 3 (§10 of the derivation) already plans, each in its own commit with the ±2 % and N = 1/N = 2 tests ADR-0077 requires. The unstable ones wait for the toy.

### 5.3 Consistency with `toy2.py`

- **Shared vectors.** Rust and Python cannot share code, so they share vectors. `raptorpath-wasm/tests/gen_knob_vectors.py` takes `--toy-dir` (a directory holding `toy.py` and `toy2.py`, extracted by `git show github/toy-stage3-knobs:tools/toy/{toy,toy2}.py`). It imports the toy's law functions and writes `raptorpath-wasm/tests/knob_vectors.json`:
  - the inputs and outputs on a grid: T, H ∈ {0.01, 0.4·(1 ± {0, 0.01, 0.02}), 2, 1e6, ∞}, ages and margins on both sides of each threshold, and the exact-equality corners;
  - the toy's commit sha;
  - `KNOB_LAW_REVISION`.

  Run it with `python -I`, outside the toy directory. `release_at` has no single function in the toy (its L6 is event-structured), so its vectors come from §2.6's formula. The file says so.
- **The Rust test `knob_vectors_match_toy`** asserts every vector bit for bit.
- **When the toy changes.** CI cannot be assumed to fetch the toy branch, so staleness is a manual step: the generator prints a diff against the committed file, and re-generation is part of every revision step (§7, P1 done criteria).
- **The claim stops at the laws and their identity.** The two models differ in link, feedback and FEC (§6), so consistency is claimed for:
  - the law functions (vectors);
  - the shared identity "T = ∞ equals full delivery", which each side gates on its own;
  - the ARQ half only: the toy has no proactive repair, so T × repair (the encoder age edge, `span_cut_by_T`) is model-only and carries its own absolute test (L1).

  No matched-cell numeric agreement is gated.

### 5.4 Gates (what must exist and pass at the end)

**Native (`cargo test -p raptorpath-wasm`)**
1. **Unchanged:** all 39 existing tests. That includes `test_multipath_n1_identical_golden` without capture and `test_continuum_one_law_across_the_dial`.
2. **The T = ∞ identity.** On the six golden cells and a 2-path cell, `multipath_knobs` at `(∞, ∞, ∞, 2000, ∞, 0)` equals `multipath` on every public accessor. The **finite limit** must also hold:
   - T = 1e6 and H = 1e6 are bit-identical to ∞ on the stream cells (λ = 0.5·C, with and without an outage);
   - **liveness:** every T = ∞ run finishes before the 20 000-tick cap (§15.1's lesson: an identity between two stalled arms proves nothing).
3. **The law module:**
   - ±2 % continuity of each function at T ∈ {0.4, 2} and H = 0.02;
   - absolute anchors at the thresholds;
   - the ∞ cases evaluated by IEEE (for example, `resend_admissible(∞, 1e9, 1e9)` is true and `release_at(t, ∞, ∞, 0)` is ∞);
   - `validate_knobs` rejects NaN, T ≤ 0 and H < 0;
   - `knob_vectors_match_toy`.
4. **Absolute invariants**, from test-only staging logs:
   - **L1:** no payload-bearing datagram (source, resend, or repair window member) for a seq with `a ≥ T`;
   - **commitment:** no resend of s after an announced `F_ann > s`;
   - **brake:** a store slot is released only by a feedback event (an ack, or a floor pass plus one RTT), never by the sender's own decision;
   - **ρ corner:** the knobs-page path gives `get_rho() == 1`.
5. **Continuity in T** at each finite named point, T ∈ {0.4, 2}·{0.98, 0.99, 1, 1.01, 1.02}, on stream cells chosen so that the physical crossover `T* ≈ a₁ + ô` (§2.2's accepted step) is at least 10 % away from the point. T* is reported:
   - **bound:** neighbouring `share_T` and `share_any` differ by ≤ 0.02;
   - **positive control,** injected and not argued: a test-only planted step of 0.05 in `share_T` at the centre point (`#[cfg(test)]`, like toy's `plant_scale`) must fail the bound;
   - the T test also runs at N = 2.
6. **H:**
   - **H = 0:** every unit's consumer delivery tick equals its decode tick;
   - **H = ∞ with T = ∞:** zero out-of-order, and delivery in seq order at the frontier tick;
   - delivery delay is non-decreasing in H over a grid, and continuous at 0.02·(1 ± 0.02);
   - `rel_by_hold + rel_by_floor` = released holes.
7. **Extend `test_continuum_one_law_across_the_dial`** (CLAUDE.md: extend the gates when touching this area) with:
   - the T = ∞ identity at the three δ presets;
   - the Realtime-call point's ±2 % T nudge.
8. **Deny test** `raptorpath-wasm/tests/knob_deny.rs`. It scans `src/lib.rs`, `src/knobs.rs` and `src/consumer.rs` and fails on:
   - `is_infinite`, `is_finite` or `INFINITY` inside a comparison;
   - any comparison of a lifetime or hold identifier (`lifetime`, `hold`, `t_life`, `hold_s`, `T`/`H` parameters) with a literal or with another knob, outside the functions pinned by name in `knobs.rs`.

   Like `raptorpath-core/tests/deny_list.rs`, it carries a planted-fixture self-test and asserts that it visited files. `knobs.rs` imports nothing but `std` and `raptorpath_math`, so it can move to the core unchanged.

**Page gates (run by `build_visualizer.sh` on every build)**
9. **Unchanged:** `test_visualizer.mjs` and `test_visualizer_ui.mjs` against the rebuilt main page.
10. **`test_knobs.mjs`** (the knobs page's engine block):
    - the wasm law exports at the named points ±2 %, including ∞ arguments;
    - `multipath_knobs` at ∞ equals `multipath` at the Bulk, Auto and Realtime aliases (eight fingerprint fields);
    - one stream cell at T = 0.4 and T = 0.4·1.02 within the bound of item 5.
11. **`test_knobs_ui.mjs`** (stub DOM, after the pattern of `test_visualizer_ui.mjs`):
    - **one hint string** `'continuum'` at every position of T, H, δ and load, both ∞ ends included, and `custom_rho == null` at every position;
    - **routing witness:** `get_lifetime_s()` and `get_hold_s()` equal the slider-mapped values (including `Infinity`) and `get_source_rate()` equals λ;
    - each preset button writes its documented triple and nothing else;
    - the readouts render "∞" at the ends;
    - **page-script deny check:** no `isFinite`, `Number.isFinite`, `=== Infinity` or `== Infinity`, and no comparison of a T or H variable with a literal anywhere in the script block. It has a planted-fixture self-test.

### 5.5 Golden scope

- The **existing six rows never re-capture** on this branch: they are the identity.
- **New knob rows** are added with `GOLDEN_CAPTURE=1` in a commit of their own, whose message says "deliberate model change: three-knob rows, proposal branch, law revision …":
  - `test_knobs_golden`: stream λ = 0.5·C at Realtime, T = 0.4, H ∈ {0, ∞}, with and without a 0.3 s outage, plus overload 1.2·C at T = 0.4;
  - fields: the eight fingerprint fields plus `share_any`, `share_T`, `ooo`, `intake_expired`.
- They are re-captured once per law revision, each time in its own commit, and only after the identity gates have passed without capture.

## 6. What the model cannot express

| proposal element | why the model cannot show it | consequence |
|---|---|---|
| RACK evidence τ_rx, gap reports, the feedback verdict (§15.1) | per-symbol oracle acks after the path RTT; no reports | not modelled. It is unreachable under §15.3 anyway |
| ô error, srtt seeds (333 ms), estimate noise, the hull bind of the running-max floor | fixed RTT per path; `srtt_secs` is a constant | ô and R are exact constants. `floor_hull_bind` would read 0 by construction, so it is not shown |
| H_f, cross-path reordering, §16's two-path floor risk (a floor passing slow-path data in flight), Realtime H = 0 releasing slow-path packets at N ≥ 2 | instantaneous forward flight; cross-path gaps exist only as losses | H_f = 0. Out-of-order comes only from late fills of lost holes. The two-path risk must be tested in the toy and the engine, not here |
| link queue, queueing delay, queue drops, the resend storm and its brake (§15.4) | fixed slots per tick, no queue | overload shows up only as intake wait, and the delay is pinned near `T − R` by the dequeue rule |
| asymmetric paths | ô = RTT/2 by construction | — |
| the sender learning a resend's fate | a resend's arrival is marked at its send tick (an existing idealisation) | L2's timing is optimistic in the model; stated in the model-vs-engine table |
| the jitter-smoothed crossover | 1 ms tick, no jitter | the share steps sharply at T*. The continuity cells avoid T* and report where it lies (§5.4 item 5) |
| the engine's r reading T (Q3 of the derivation) | the model's r is r(β(δ)) only | the §4.10 (t_tail, T) → r gauge is optional (Q-V4) |
| the floor's wire cost (Q15) | the header carrier is modelled at zero cost | the datagram carrier (S4b) is not modelled |
| the memory bound at T = ∞ (F01) | the model's pool is the constant 512 per path (M01) | as on main; not a new defect |
| calls at real packet rates (50/s) | 2 000 units under a 20 000-tick cap | streams are scaled to λ/C ≥ 0.05; a call is "a stream at Realtime with T = 0.4" |

## 7. Implementation brief (phases)

**Branch.** Create `viz-three-knobs` from this plan's branch head (`viz-three-knobs-plan`, which is `github/main` plus this file), in a worktree in the session scratchpad. Read the derivation with `git show github/three-knobs:docs/derivations/three-knobs.md` and the toy with `git show github/toy-stage3-knobs:tools/toy/...`; do not merge either branch. Push only to `github`. Never merge to main.

**Must not touch:**
- `raptorpath/` (the engine), `raptorpath-math/` and `raptorpath-core/`;
- the script of `interactive-visualizer.html` (only its embedded build changes);
- any existing test assertion;
- the six golden rows;
- `docs/roadmap/` (the coordinator places the node).

**Builds.** Each phase runs `cargo test -p raptorpath-wasm` (own target dir, `CARGO_BUILD_JOBS=1`, the host cargo mutex) and, from P6 on, `bash raptorpath-visualizer/build_visualizer.sh`.

| phase | work | done when |
|---|---|---|
| **P0 baseline** | Record the native test results and run both `.mjs` gates on the committed page | 39/39 native tests and both gates green; the numbers are noted in the first commit message |
| **P1 law module** | `src/knobs.rs` (§5.1) with its unit tests; `KNOB_LAW_REVISION`; `tests/gen_knob_vectors.py`, `tests/knob_vectors.json` and `knob_vectors_match_toy`; `tests/knob_deny.rs` with its planted fixture. No sim change | native green; vectors match the toy at its recorded sha; the deny test's self-test flags the fixture |
| **P2 workload and link** | `multipath_knobs` (§3.1), with `multipath` and `new` delegating. Per-seq vectors `h_seq`, `last_send_seq` and `resolved_seq` (Symbols are pruned, so per-seq state needs its own arrays). Hand-over `h(u) = u/λ`; the source arm sends only if `h(u_next) ≤ now`; at the intake, L1 (`intake_expired`) and then the dequeue rule (`dequeue_expired` with R; a younger unit is waiting). The outage window in `wire_lost` (still consuming the channel state, so the post-outage pattern stays aligned). `completion_chi_raw`'s `t_rem` as in §2.1. Accessors `get_lifetime_s`, `get_hold_s`, `get_source_rate` | all 39 existing tests unmodified and green; identity test §5.4 item 2 for the workload alone (T = ∞); the L1 intake half of item 4 |
| **P3 sender laws** | L2 on the retx candidate. The floor `F_ann` (toy `update_floor`), recomputed at every send and every tick, and carried free on every arrival. The commitment `seq ≥ F_ann` on retx. A floor-repeat datagram on the sweep clock `clamp(2·srtt, 25, 100 ms)` while `F_ann` is above the sender's ack frontier: it takes one slot and a loss draw, and never fires at T = ∞. The encoder's L1 age edge before each repair. The brake: a passed seq's store slot is released one path RTT after the consumer passes it. Finish terms (§2.1) | items 2 and 4 green, with T = 1e6 bit-identical to ∞; L1 absolute over every payload datagram, including repair members; `commit_viol = 0` |
| **P4 consumer and readouts** | `src/consumer.rs`, a pure per-tick stage: decoded seqs in; `t_detect`, the received floor and `release_at` per hole; consumer delivery ticks, out-of-order, stall episodes, `rel_by_*`. Accessors for every readout in §4.3 | item 6 green; consumer unit tests; native green |
| **P5 continuity and goldens** | §5.4 items 5 and 7 with the positive control; then `test_knobs_golden` captured in its own commit (§5.5) | items 1–8 green; the capture commit message as in §5.5 |
| **P6 + P7 page and page gates (tests first)** | Write `test_knobs.mjs` and `test_knobs_ui.mjs` first (they fail). Then `knobs-visualizer.html` from a copy of main's page source: the ρ dial, ρ readouts and triangle removed; T, H, load and outage dials (§4.1); preset and workload buttons (§4.2); readouts (§4.3); the cartoon through the wasm exports; model-vs-engine rows for every new law ("proposal; engine has none"). Generalise `build_visualizer.sh` to inject the init block into both pages and run all four `.mjs` gates. The `SimWrapper` gains `multipath_knobs` and the new getters; its existing members are unchanged | `build_visualizer.sh` green, all four gates included; main's page script byte-identical to the base outside the injected block |
| **P8 docs** | Add the `docs/derivations/README.md` row for this plan. Each new page panel's tooltip cites its three-knobs section | the README row is present; the secret scan is clean |

**Per-phase rules.**
- Each phase is one or more commits on `viz-three-knobs`, pushed to `github` after its checks pass.
- If an addition cannot be made inert at T = ∞ without a comparison on T, or must reorder the slot loop, stop and return the evidence (§2.2).
- If a new law revision lands mid-work, finish the current phase on the old revision, then update `knobs.rs`, the vectors and the knob goldens in one step.

**Report at the end:** the branch head; for each phase the test counts; T* per continuity cell; the bind gauges (`span_cut_by_T`, `rel_by_hold`/`rel_by_floor`, the dequeue expiries) at each named point; and any law the model could not reach.

## 8. Owner questions (each with a default)

- **Q-V1, page arrangement.** **Default: two pages from one wasm** (§3.2). The alternative, one superset page with ρ, T and H all live, keeps ρ as a dial, against the proposal.
- **Q-V2, ρ code during the proposal.** **Default: keep it in the struct, inert at ρ = 1 on the knobs page,** so that main's page and all its gates run on the same wasm. It is deleted at acceptance. The corner ρ < 1 with T < ∞ is not a law and is gated unreachable from both pages (§3.3).
- **Q-V3, the reference deadline for the T = ∞ comparisons and the recovery time.** **Default: τ = 0.4 s** (§13.6), replaced by the toy's stage 3d definition once registered.
- **Q-V4, the triangle panel on the knobs page.** **Default: omitted.** Later, the §4.10 `(t_tail(δ), T) → r` form can come back as a gauge, after the derivation's Q3.
- **Q-V5, H_f in the model.** **Default: 0.** The model has no reordering, so no form of H_f can be told apart here.
- **Q-V6, the floor carrier.** **Default: header, at zero cost** (the derivation's Q15 default (a)).
- **Q-V7, the outage scope.** **Default: all paths, one window.** The ABI takes per-path windows so that a one-path outage can be added without an ABI change.
- **Q-V8, the dequeue expiry at the intake** (§13.2, one revision old). **Default: modelled,** because it is the current derivation. Without it the overload readout shows the collapse §13.2 removed.

## 9. Final review (advisor)

*(Filled in after the finished plan was reviewed; see below.)*
