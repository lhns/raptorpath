# The interactive simulator under the three knobs (plan, no code)

**Question.** How must the L0 model (`raptorpath-wasm` and `raptorpath-visualizer/`) change to show the proposed knobs, lifetime T, delay price δ and order hold H, with the delivered share ρ as a reported outcome? The proposal is `github/three-knobs:docs/derivations/three-knobs.md` (§1–§10, then Revision 1 §13, Revision 2 §15 and the stage 3c findings §16; later sections supersede earlier ones). The owner likes it but has not accepted it, so everything here lives on a branch and must not replace main's model.

**Base.** `github/main` `70b4b066`. Toy: `github/toy-stage3-knobs` `02eda90b` (`tools/toy/toy2.py`, arm S4). Line numbers refer to the base.

**Short answer.**
- **Adjust; do not rewrite.** The model's coupling to ρ is one 30-line give-up block, one clamp, one finish term and five accessors. Its hint coupling is now one construction-time alias table. What T and H need is new state: hand-over stamps, an intake, an outage, the floor and a consumer stage. Every piece of it is inert at T = ∞, λ = ∞ and H = ∞ by IEEE arithmetic, so the six goldens, every existing native test and both page gates must pass unmodified. That is the identity gate, and it is cheaper and stronger than any rewrite could offer (§2).
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

`raptorpath-wasm/src/lib.rs` is 3 316 lines: about 1 900 of code and 1 420 of tests (39 `fn test_` in `lib.rs`; the crate total is recorded at P0). `Simulation::step` (:1201–:1594) runs one 1 ms tick:
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
| finish :1512–:1520 (`cum_decoded + given_up ≥ num_source`; no lost unrecovered) | end of run | **gains `+ intake_expired + passed_undelivered`**, both 0 at T = ∞. Each unit is counted once (§7, the sender-state rules) |
| frontier gauge :1566–:1571 | contiguous delivered or given-up prefix | it also skips floor-passed seqs (0 at T = ∞) |
| `get_reliability` :1814 (decoded / total_src) | today's ρ readout | **kept.** The knobs page reads `share_any` and `share_T` instead (§4.3) |
| hint alias table :1080–:1086 | `bulk → 0.005`, `realtime → 50`, `continuum → custom_delta`, else 0.5 | unchanged: construction only, class A. The knobs page passes only `"continuum"` |
| `fixed_r` :1088, :735, :914, :1786 | the r-override (audit class D, owner's word pending) | unchanged; not reachable from either page |
| `legacy_*` (`#[cfg(test)]`) :427, :432, :731, :931, :1449 | test-only ablations (class C) | unchanged |
| `continuum_tail` β guard :797 | `δ_eff` readout only (audit W8, class C) | unchanged |
| `send_tick` :447, latency from first send :1001 | latency readout | **kept** as the arrival-delay readout. The new delay is measured from hand-over, at the consumer (§4.3) |
| `completion_chi_raw` :688 (`t_rem = remaining / capacity`) | completion exposure | **unchanged.** For a stream it underestimates the time to the end. A stream form would be a law edit with no provenance in the paper or the derivation, so it is not made (§6, Q-V9) |
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
| preset buttons :3045 (V14) | named points of (T, δ, H) (§4.2) |
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
  - every existing native test;
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
| recovery time after the outage | Split hand-overs into windows of 50 units each. A window's on-time share is the fraction of its units delivered within τ. The recovery time is the start of the first window after the outage end whose on-time share is at least the **minimum** over the pre-outage windows, minus the outage end. Taking the pre-outage minimum lets the baseline's own noise set the bar, with no tolerance constant. A last-late-unit definition would read the run's last random loss instead of the drain. This is §16's requested readout. When the toy registers its own definition (stage 3d), the model adopts it |
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

`raptorpath-wasm/src/knobs.rs` holds pure f64 functions, written to `raptorpath-core`'s rules (no I/O, no env, no hint type, no statics). It imports only `std` and `raptorpath_math`, so it carries no `#[wasm_bindgen]`. The page gets the functions through thin forwarding wrappers in `lib.rs`, the way `lib.rs` already re-exports `raptorpath-math`. The names and argument order follow `toy2.py`'s law functions. Two names differ: the toy calls the first one `intake_expired_fn`, and `ack_timeout` is the toy's method `R_now`.

| function | form | source | stability |
|---|---|---|---|
| `intake_expired(T, age)` | `age ≥ T` | L1, §2.1 | stable since §2 |
| `resend_admissible(T, age, x)` | `age + x < T` | L2, §2.2 | stable since §2 |
| `within_lifetime(T, age)` | `age ≤ T` | L3 readout | stable |
| `dequeue_expired(T, age, m)` | `age + m ≥ T`, m = R | §13.2 | **young**: one revision, untested with FEC |
| `protected(T, now, last_send, c, h, srtt)` | `max(now, last_send + c) − h + srtt/2 < T` | §15.3 | **unstable**: §16's commitment seam failed in the toy |
| `release_at(t_detect, H, t_floor, H_f)` | `min(t_detect + H, t_floor + H_f)` | L6, §2.6 | stable form; H_f is not confirmed (Q-V5) |
| `ack_timeout(srtt, rttvar, g, ack_delay)` | `srtt + max(4·rttvar, g) + ack_delay` | §13, RFC 6298 | stable |
| `validate_knobs(T, H) -> Result<(), String>` | domain: T ∈ (0, ∞], H ∈ [0, ∞]; NaN is rejected | §3 of the derivation | the only place T and H meet a literal. `multipath_knobs` returns `Result<Simulation, String>`, which throws in JS. If wasm-bindgen refuses `String` as the error type there, use `JsError` at the boundary and keep `String` in a native `try_multipath_knobs`. The page's slider domains never produce an error, and the UI gate asserts no throw at any position |

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

- **Shared vectors.** Rust and Python cannot share code, so they share vectors. `raptorpath-wasm/tests/gen_knob_vectors.py` takes `--toy-dir` (a directory holding `toy.py` and `toy2.py`, extracted by `git show github/toy-stage3-knobs:tools/toy/{toy,toy2}.py`; the toy imports numpy). It imports the toy's law functions and writes `raptorpath-wasm/tests/knob_vectors.txt`:
  - the inputs and outputs on a grid: T, H ∈ {0.01, 0.4·(1 ± {0, 0.01, 0.02}), 2, 1e6, ∞}, ages and margins on both sides of each threshold, and the exact-equality corners;
  - header comments with the toy's commit sha and `KNOB_LAW_REVISION`.

  Run it with `python -I`, outside the toy directory.
- **Format.** It is a std-only line format, because JSON has no ∞ and the crate has no JSON parser. Adding one would be a new dependency. Each line reads `<rust_name> <arg as 16-hex f64 bits>… -> <result>`, where an f64 result is 16-hex bits (Python `struct.pack('>d')`, Rust `f64::from_bits(u64::from_str_radix(..))`) and a bool is `0` or `1`. `#` starts a comment.
- **Where the expected values come from.** `release_at` and `ack_timeout` have no single toy function: L6 is event-structured in the toy, and `R_now` is a method. Their vectors are computed from the formulas of §2.6 and §13, and the file marks them `formula`. The other five come from the toy's own functions, marked `toy`.
- `binid.yml` does not fire: its path filter covers only engine files.
- **The Rust test `knob_vectors_match_toy`** asserts every vector bit for bit.
- **When the toy changes.** CI cannot be assumed to fetch the toy branch, so staleness is a manual step: the generator prints a diff against the committed file, and re-generation is part of every revision step (§7, P1 done criteria).
- **The claim stops at the laws and their identity.** The two models differ in link, feedback and FEC (§6), so consistency is claimed for:
  - the law functions (vectors);
  - the shared identity "T = ∞ equals full delivery", which each side gates on its own;
  - the ARQ half only: the toy has no proactive repair, so T × repair (the encoder age edge, `span_cut_by_T`) is model-only and carries its own absolute test (L1).

  No matched-cell numeric agreement is gated.

### 5.4 Gates (what must exist and pass at the end)

**Native (`cargo test -p raptorpath-wasm`)**
1. **Unchanged:** every existing test in the crate (count recorded at P0). That includes `test_multipath_n1_identical_golden` without capture and `test_continuum_one_law_across_the_dial`.
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
5. **Continuity in T** at each finite named point, T ∈ {0.4, 2}·{0.98, 0.99, 1, 1.01, 1.02}.
   - **Cell placement.** Resend arrival ages recur at every round: `t_k ≈ k·RTT + ô + 1 tick` in this model (retry timer = path RTT, ô = RTT/2). Each `t_k` is a physical step in the share (§2.2's accepted step, here sharp because the model has no jitter). Choose the stream cell's RTT from {40, 50, 80} ms so that every `t_k` is at least 0.1·RTT from both 0.4 s and 2 s. Report the nearest `t_k`.
   - **Bound,** derived from the measured continuous change and not a flat number (the audit's §7d3 lesson). For each readout (`share_T`, `share_any`), measure the slope off the point on each side, from T·{0.90, 0.95} and T·{1.05, 1.10}. The tolerance for each 1 % step is `1.5 × max(side slopes) × 0.01·T + 1/units`, where the last term is one unit's quantum. Each neighbouring pair in the nudge grid must stay within it;
   - **positive control,** injected and not argued: a test-only planted step of five times the cell's tolerance in `share_T` at the centre point (`#[cfg(test)]`, like toy's `plant_scale`) must fail the bound;
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
    - one stream cell at T = 0.4 and T = 0.4·1.02 within the bound of item 5;
   - **`SimWrapper` (shared by both pages through the build's init block) has no presence test.** `new SimWrapper(params)`, the call on main's unchanged page, builds through `multipath_knobs` with a constant `MAIN_KNOBS = {lifetimeS: Infinity, holdS: Infinity, sourceRate: Infinity, numSource: 2000, outageStartS: Infinity, outageLenS: 0}`. The knobs page calls `SimWrapper.withKnobs(params, knobs)`. Both reach the one wasm constructor, and the baseline memo key includes the knobs.
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
| the completion exposure for a stream | `completion_chi_raw` reads `remaining / capacity`, so for a stream it expects the end too early, and the completion term fires early | unchanged: a stream form has no provenance (Q-V9). At T = ∞ the identity is unaffected. On stream cells, the end-of-stream FEC readouts are labelled as carrying it |
| calls at real packet rates (50/s) | 2 000 units under a 20 000-tick cap | streams are scaled to λ/C ≥ 0.05; a call is "a stream at Realtime with T = 0.4" |

## 7. Implementation brief (phases)

**Branch.** Create `viz-three-knobs` from this plan's branch head (`viz-three-knobs-plan`, which is `github/main` plus this file), in a worktree in the session scratchpad. Read the derivation with `git show github/three-knobs:docs/derivations/three-knobs.md` and the toy with `git show github/toy-stage3-knobs:tools/toy/...`; do not merge either branch. Push only to `github`. Never merge to main.

**Must not touch:**
- `raptorpath/` (the engine), `raptorpath-math/` and `raptorpath-core/`;
- the script of `interactive-visualizer.html` (only its embedded build changes);
- any existing test assertion;
- the six golden rows;
- `docs/roadmap/` (the coordinator places the node).

**Sender-state rules (P2 and P3 implement exactly these).** Per-seq arrays, indexed by seq, because `symbols` is pruned:
- `h_seq`, the hand-over;
- `last_send_seq`, the last send tick of any copy;
- `path_seq`, the path of the last copy;
- `acked_seq`, meaning the sender knows the receiver holds s or has passed it.

`acked_seq[s]` becomes true at exactly these points, each an existing set-point of `sym.recovered` except the last:
1. the ack of an arrived source symbol after its own path's RTT (the block after the give-up law);
2. a resend's arrival, an existing idealisation (§6);
3. a decode recovery in `mark_recovered`;
4. a ρ give-up (inert on the knobs page);
5. **new:** the consumer's floor pass of s, plus `paths[path_seq[s]].rtt_ticks`, which is the feedback that ends the brake.

The derived quantities:
- **The sender's ack frontier** is `A_known` = the first s with `acked_seq[s]` false. The existing `frontier_acked` gauge is **not** used: it lags by the aggregate RTT, so at T = ∞ it would sit below `F_ann` and fire the floor repeat, breaking the goldens.
- **The floor `F_ann`** is a running maximum. The scan starts at `max(F_ann, A_known)`, steps over every s with `acked_seq[s]` or `!protected(s)`, and stops at the first s with `!acked_seq[s] ∧ protected(s)` or at `next_seq`. At T = ∞, `protected` is always true, so `F_ann = A_known`.
- **The floor repeat** fires on the sweep clock while `F_ann > A_known`, so it never fires at T = ∞.

**Commitment.** When s < `F_ann` is lost and unrecovered, set `sym.recovered = true` (no more resends), decrement `lost_pending` and increment `passed_undelivered`. Its store slot is **not** released then; set-point 5 releases it. A later decode of s moves it from `passed_undelivered` to `cum_decoded`: the decrement and the increment happen in the same place, so every unit sits in exactly one of delivered, intake-expired, given-up or passed-undelivered. `mark_recovered` already skips symbols with `recovered = true`, so the slot is released once.

**Builds.** Each phase runs `cargo test -p raptorpath-wasm` (own target dir, `CARGO_BUILD_JOBS=1`, the host cargo mutex) and, from P6 on, `bash raptorpath-visualizer/build_visualizer.sh`.

| phase | work | done when |
|---|---|---|
| **P0 baseline** | Record the native test results and run both `.mjs` gates on the committed page | all native tests (count recorded) and both gates green; the numbers are noted in the first commit message |
| **P1 law module** | `src/knobs.rs` (§5.1) with its unit tests; `KNOB_LAW_REVISION`; the forwarding wrappers in `lib.rs`; `tests/gen_knob_vectors.py`, `tests/knob_vectors.txt` and `knob_vectors_match_toy`; `tests/knob_deny.rs` with its planted fixture. No sim change | native green; vectors match the toy at its recorded sha; the deny test's self-test flags the fixture |
| **P2 workload and link** | `multipath_knobs` (§3.1), with `multipath` and `new` delegating, and `validate_knobs` at its entry. The per-seq arrays of the sender-state rules. Hand-over `h(u) = u/λ`; the source arm sends only if `h(u_next) ≤ now`; at the intake, L1 (`intake_expired`) and then the dequeue rule (`dequeue_expired` with R; a younger unit is waiting). The outage window in `wire_lost` (still consuming the channel state, so the post-outage pattern stays aligned). Accessors `get_lifetime_s`, `get_hold_s`, `get_source_rate` | all existing tests unmodified and green; identity test §5.4 item 2 for the workload alone (T = ∞); the L1 intake half of item 4; `validate_knobs` errors tested |
| **P3 sender laws** | L2 on the retx candidate. `A_known`, `F_ann`, the commitment and set-point 5 as the sender-state rules state them. `F_ann` is recomputed at every send and once per tick, and is carried free on every arrival. A floor-repeat datagram on the sweep clock `clamp(2·srtt, 25, 100 ms)` while `F_ann > A_known`: it takes one slot and a loss draw. The encoder's L1 age edge before each repair. Finish terms (§2.1) | items 2 and 4 green, with T = 1e6 bit-identical to ∞; L1 absolute over every payload datagram, including repair members; `commit_viol = 0`; a test that every handed-over unit lands in exactly one outcome class |
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

- **Q-V9, the completion exposure on a stream.** **Default: unchanged.** A stream form, `t_rem` from the hand-over rate, would need its own line in the paper first.

**Before any merge to main** (none is planned):
- the owner accepts the proposal;
- the ADR "three application knobs" (the derivation's §10 step 1) is written;
- every law the model runs has its formula and derivation in the paper (FORMULA-FIRST), including the two model-side seats: L1 at the encoder window and L6's consumer stage;
- the owner amends CLAUDE.md (ρ stops being a dial).

Until then the branch is an exhibit, not a candidate.

## 9. Final review (advisor)

The advisor reviewed the finished draft (first push `092050cf`) and confirmed the adjust verdict and the two-page structure. It found four holes that would have forced the worker into design choices, and five tightenings. All were taken:
1. **Sender-view state.** The draft named an undefined `resolved_seq` and "the sender's ack frontier". Using the existing `frontier_acked` gauge would have fired floor repeats at T = ∞ and broken the goldens. Now §7's sender-state rules define `acked_seq` with its five set-points, `A_known`, `F_ann` and the repeat condition.
2. **Accounting.** The draft's `+ floor_passed` double-counted a passed hole that is later decoded, and left committed holes in `lost_pending`. Now each unit is in exactly one outcome class, the commitment resolves the symbol at the sender, and the slot waits for feedback.
3. **The vector file.** JSON cannot hold ∞, and the crate has no parser. It is now a std-only line format with f64 bit patterns, and the toy name mapping (`intake_expired_fn`, `R_now`) is stated.
4. **The `t_rem` stream edit** was a law form with no provenance. It is dropped and recorded (§6, Q-V9).
5. **Recovery time** is now a windowed on-time share measured against the pre-outage minimum, not the last late unit.
6. **The continuity bound** is now derived from the measured side slopes, not a flat 0.02, and the cells clear every resend round `t_k`, not only the first.
7. **`knobs.rs`** carries no `#[wasm_bindgen]`; forwarding wrappers sit in `lib.rs`.
8. **Three choices are pinned:**
   - the `SimWrapper` entry points (no presence test);
   - `validate_knobs`'s error path;
   - the brake's RTT (the last copy's path).
9. **The merge precondition** was added (§8).

The nits were taken too: the preset-button line, and the test count (recorded at P0 instead of "39").

## 10. Addendum 1 (2026-11-24): out-of-order on the graph, a forward leg with a queue, the resend budget

**Owner (2026-11-24), on §6:** "of course it can show out of order percentage or show the out of order packets in a different color in the graph. also queue and budget. ask fable, how". The owner is right. Three rows of §6 were scope choices, not limits of the model:
- out-of-order as a visible quantity;
- a link queue with its drops and delay;
- the resend budget.

This addendum adds phases P9 to P14 after P8. P0–P8 are unchanged; the notes below say where each one is extended.

**Rule kept throughout: the page draws only what the model computes.** No page code derives a class, a count or a delay from raw model state.

**Read first.** The implementer's branch `viz-three-knobs` at `2932b275` already has:
- the Revision 3 laws in `knobs.rs` (`KNOB_LAW_REVISION = "17 (Revision 3)"`, `trigger_delay`, `one_way_fwd`);
- per-seq ack queues for set-point 1 (`ack_due`, popped at the tick: this replaces §7's per-symbol ack block, which pruned symbols would never reach);
- §17.4's recovery windows;
- a per-unit out-of-order marker (`get_ooo_units`, `get_unit_delays`, indexed by unit, with intake expiries reading false).

This addendum builds on that code, not on the plan text it replaced.

**Advisor (Fable), before writing.** It changed six things:
- It had me read the implementer's branch first. Revision 3 alignment is already done there, so no alignment phase is needed. Set-point 1 already sits on a per-seq ack queue.
- It split the rewrite question in two: deferring arrivals is an extraction, and routing the sender's knowledge through feedback is a separate choice. That choice goes to the coordinator with measurements.
- It required every reader of `rtt_ticks` to be listed with a stated source, and the retry timer and `protected`'s cooldown to read one function.
- It moved the forward leg's defaults into time units (service time 0, queue limit ∞), avoiding the toy's `link is None` branch and its 0·∞ corner.
- It had the link vectors driven through a real `toy2.Sim`.
- It ruled that two-path display needs a coordinator choice, not a branch on path count or an H_f selector.

It also reviewed the finished addendum; that review is recorded in A8.

### A1. Out-of-order, late and given up on the graph (P9)

**Model, extending P4.**
- **One class per handed-over unit,** decided in the model, by this precedence:
  1. `expired` (intake, no seq);
  2. `undelivered` (passed by the floor or given up, never delivered);
  3. `out_of_order` (delivered after a higher-numbered unit; the toy's `ooo_loss`, as in `get_ooo_units`);
  4. `late` (delivered, with delay from hand-over > T, via `within_lifetime`);
  5. `on_time`;
  6. `pending` (not yet resolved).
- A unit that is both late and out of order is classed `out_of_order`, and the overlap is reported as its own count `ooo_late`, so nothing is hidden.
- **Shares:** `ooo / delivered` (the existing `get_ooo_share`) and `late / delivered`.
- **Export as append-only events:** `take_unit_events() -> Vec<f64>`, flat triples `[unit, time_s, class]`, holding only the units whose class changed since the last call. This keeps the per-frame cost within the 12 ms step budget the UI gate already enforces. The full arrays stay for tests.
  - A unit is emitted when it is delivered, when it expires at the intake, or when it is passed or given up (`undelivered`, so it shows while the run plays).
  - **One superseding event per unit is allowed:** an `undelivered` unit filled later becomes `out_of_order` or `late`. The page then moves its dot, and the UI gate counts final classes.
  - Undelivered and expired dots sit on a strip at the chart's top edge, x = the hand-over time, with no delay value drawn.

**Page, extending P6.**
- A new chart: hand-over time on x, delay from hand-over on y, one dot per unit.
- One colour per class. `out_of_order` gets its own hue, distinct from `late` and from `undelivered`; `expired` sits on a baseline strip.
- The T line is drawn at y = T, and is absent at T = ∞ because the canvas clips it.
- A readout gives the ooo share, the late share and the `ooo_late` count.
- The page only appends the events it is handed.

**Gates.**
- **Native:**
  - the class is a function of the consumer's delivery ticks, re-derived in the test from `delivered_at` and the unit order;
  - every unit gets exactly one class;
  - `out_of_order = 0` at H = ∞ with T = ∞;
  - at H = 0, `out_of_order` equals the late fills of released holes;
  - the events, concatenated over a run, equal the final arrays.
- **UI (extends P7):**
  - the drawn dot counts per colour equal the model's class counts (routing witness);
  - the drawing code reads only `take_unit_events`, checked by a stub that counts accessor calls;
  - no delivery-tick or seq comparison appears in the drawing function: it is added to the page-script deny check, with a planted fixture.

### A2. The forward leg: extraction first (P10)

**What `lib.rs` does today.**
- The tick is 1 ms (`TICK_SECS`).
- A send consumes a slot of its path's `capacity` and draws the path's next channel state (`wire_lost`). If the datagram is not lost, the arrival work runs **at the send site, inside the slot loop**: `decoder.feed_*`, `mark_recovered`, `record_delivery`, `cum_decoded` and the knob bookkeeping (`carry_floor`, `ack_due`). This happens in three places: the retransmit arm, `send_repair` and the source arm.
- Feedback is a per-path `feedback_queue` popped `rtt_ticks` after the **send**.
- The sender is paced exactly to the link: one send is one slot is one wire symbol. So **a queue can only form once the link's service rate is an input separate from the sender's slots** (`capacity` stays the sender's per-tick send budget).

**The extraction.**
- All arrival work moves into one routine, `deliver(dg)`, keyed by **seq**. It is not keyed by the retransmit arm's `symbols[i]` index, which can be stale by the time of arrival.
- A datagram `dg` carries:
  - kind (source, resend, repair with its coded data, or floor repeat);
  - seq;
  - send tick;
  - path;
  - the **floor stamped at its send** (`F_ann` at that instant, read by the consumer at arrival; this replaces "the sender's floor at the arrival tick");
  - the loss outcome.
- A per-path forward leg holds the datagrams in flight. `deliver` is called:
  - right after each send, for every datagram already due, so at zero delay the just-sent datagram is delivered **at the same code point as today**;
  - at the start of each tick, before the feedback pop, for the rest.
- The slot loop's priority (retransmit, repair, source) is not touched.
- **Delivery order at tick start:** across paths, by arrival time; ties go by path index, then by send order.
- **Two cases a delay makes reachable**, each with one rule:
  - **duplicate:** a copy arrives for a seq the receiver already holds (a second copy sent while the first sat in the queue, because the retry timer lags a growing backlog). It increments `dup` and nothing else: no decode work, no slot release, no class change;
  - **late fill of a committed seq:** a copy (or a decode) arrives for a seq the floor already passed (§17.1's copy in flight). The unit moves from `passed_undelivered` to delivered exactly once, its class changes once (A1's superseding event), and its store slot is released only by set-point 5, never a second time.
- **Decoder order:** `RlcDecoder` is incremental Gaussian elimination over pivots with a cascade on each source (`raptorpath-math/src/rlc.rs` :146–:200), so by construction it does not depend on arrival order. P10 still adds a test before any N = 2 link cell: feeding a run's sources and repairs in a permuted order yields the same recovered set. That matters because the sim's header comment ties "no encoder lag" to FIFO.

**Physical against believed RTT.** Every reader of `rtt_ticks`, `srtt_secs` and `rttvar_secs` on `2932b275` gets a stated source. "Physical" is the configured base RTT and the forward leg's propagation `d_p`. "Belief" is the measured estimator of A3.

| reader (line on `2932b275`) | today | source after P10–P11 |
|---|---|---|
| `feedback_queue` pop (:1661) | send + rtt | **physical:** (arrival or would-be arrival) + (rtt − d_p), stored as a due tick. A queue drop is due at its **would-be arrival** (departure as if served + d_p) + (rtt − d_p), so the front-popped queue stays monotone |
| the arrived-symbol ack block (:2001) | `tick − send ≥ rtt` | **physical:** the same due tick as set-point 1. It is kept only for the `symbols` pruning it drives |
| `get_path_rtt_ms` (:2140) | rtt_ticks | unchanged: it reports the configured base RTT. A new `get_path_srtt_meas_ms` reports the belief |
| set-point 1 `ack_due` (:1921) | send + rtt | **physical:** arrival + (rtt − d_p) |
| set-point 5 `floor_ack_due` (:855) | consumer pass + rtt | **physical:** pass tick + (rtt − d_p) |
| estimator flush period (:1677) | rtt | belief: `retry_delay(p)` |
| retry timer (:1741) | `tick − last_retx ≥ rtt` | **belief:** `retry_delay(p)` |
| `seq_protected`'s c and never-resent trigger (:784) | the path RTT | **belief:** the same `retry_delay(p)`. One function for both, so the law and the gate cannot disagree (§17.1's seam). The model's first and later resends share one timer, so Revision 3's two-case `d(s)` collapses to `c` (`p_reach = ∞`, as bound now) |
| P_lost inputs (:1757–:1758), the controller and completion via `agg_srtt`/`agg_rttvar` (:974–:977), the burst term (:1226) | fixed `rtt/1000`, `rtt/8000` | srtt: belief (`base + measured excess`, A3). rttvar: **kept at the fixed rtt/8000**, a declared model constant for main's laws; a gauge reports the measured rttvar beside it |
| per-path clocks gauge (:2060–:2065), `frontier_acked` (:2100), the span deadline (:2472) via `agg_rtt_ticks` | fixed | belief, rounded as today |
| the `rtt.max(2)` clamp (:1517) | `rtt_ticks ≥ 2`, while `srtt_secs = rtt/1000` keeps the unclamped value | unchanged. The estimator is seeded at `rtt_ticks`, and belief srtt in seconds is `srtt_secs_base + (srtt_meas_ticks − rtt_ticks)·TICK_SECS`, so `+ 0.0` keeps it bit-identical at rtt = 1 |

**Defaults are values.** A link with service time `sv_p = 0` s, queue limit `q_p = ∞` s (in time, the toy's `link_qs`) and propagation `d_p = 0`:
- the departure is `max(t, free_p) + sv_p`, which equals `t`;
- the drop test `max(0, free_p − t) ≥ q_p` reads `0 ≥ ∞`, which is false;
- the arrival is `departure + d_p`, which equals `t`.

There is no `link is None` branch, and no 0·∞ product anywhere. The page states the queue limit in milliseconds of buffer, with domain (0, ∞], so a finite buffer behind an unlimited link never drops (backlog 0).

**Loss and outage.** Both are decided **at send for the departure instant**, which is known at enqueue. The datagram consumes its channel state at send, in send order (the same as departure order on a FIFO), as today. A queue drop takes no channel state and no loss draw (toy: "dropped at the bottleneck queue: no arrival, no loss draw"). The P2 outage window is tested against the departure tick.

**Delay accounting: one physical term, never the belief.** `record_delivery` adds `agg_srtt()·500` ms and `unit_delays` (:908) adds `one_way_s()`. After P11 both would carry the queue excess that the arrival tick already contains, so `share_T`, the `late` class and the recovery time would double-count it.
- Every delay readout therefore adds one **physical** accounting term, `acc = agg(rtt_base/2 − d_p)` from the configured values, which is bit-identical to today's addend at `d_p = 0`.
- ô (the belief) is read only by L2 and `protected`.
- Domain: `d_p ∈ [0, rtt_p/2]` (`validate_link`).

**ABI, wrapper and page controls.**
- **ABI:** `Simulation::multipath_link(… every multipath_knobs argument …, service_s: Vec<f64>, queue_s: Vec<f64>, prop_s: Vec<f64>, resend_limit: f64)` is the one constructor. `multipath_knobs` delegates to it with `(0, ∞, 0)` per path and `resend_limit = ∞`, one level more of the same delegation pattern. `validate_link` rejects negative or NaN values, a `queue_s` of 0, `prop_s > rtt/2`, and a `resend_limit < 0`.
- **`SimWrapper`:** `MAIN_KNOBS` gains `{serviceS: 0, queueS: Infinity, propS: 0, resendLimit: Infinity}`, with no presence test; `withKnobs` passes the page's values.
- **Routing-witness getters:** `get_link_service_s(i)`, `get_link_queue_s(i)`, `get_link_prop_s(i)`, `get_resend_limit()`.

**The knobs page's controls.** Each map has a value at its end, not a flag:

| control | map | end values |
|---|---|---|
| link rate μ/C | `u/(1 − u)`, page writes `sv = 1/μ` | ∞ end gives `sv = 0`: no bottleneck |
| buffer, ms | `10·(u/(1 − u))` | ∞ end: no limit |
| propagation share | u ∈ [0, 1], `d_p = u·rtt/2` | — |
| resend budget per 5 ms | `10·(u/(1 − u))^2` | ∞ end: unlimited, the page default |

The engine's 10 is a named point at u = 0.5, with a button. Under A6 option (b), only the budget control and A1 ship before stage 3d; the link controls stay hidden, the page passes the defaults, and A4's UI gates run as "the default link draws zero queue and zero drops".

**Sender knowledge: a choice for the coordinator.** Today the sender learns a resend's arrival, or a decode recovery, at the arrival instant (the oracle `recovered` flag). With a forward leg, that moves from the send tick to the arrival tick and stays inert at the default. Making the sender learn only through feedback (one reverse delay later) does not reorder the loop either, but it may move the six goldens: FEC-recovered holes could be resent before the feedback lands.
- P10 measures that on a throwaway build, not committed, and reports the result.
- Default, unless the coordinator decides otherwise: arrival-tick knowledge, with its lead gauged per resend (`knowledge_lead`, the reverse delay it skips). It is **bounded by a test:** for every resend, `knowledge_lead ≤ rtt − d_p` of its path, the reverse delay. It equals that exactly, because the model's reverse leg has no queue.
- Full feedback routing would need a sender-view flag in every scan. If that moved the goldens, it would be the rewrite trigger of §2.2, and it goes back to the coordinator.

**P10 done when:**
- main's six golden rows and `test_knobs_golden` pass **unmodified**, with no capture;
- every existing native test and all four `.mjs` gates pass;
- a new test shows that with the default link every datagram is delivered inline at its send tick, and that `free_p ≤ now` always holds;
- the decoder permutation test passes, and so do the duplicate and late-fill rule tests (both driven by a test-only forward delay);
- the sender-knowledge measurement is reported.

If the extraction cannot be made bit-identical, stop and return the diff that moved.

### A3. The RTT the sender believes, and the one-way term (P11)

**The estimator.**
- Per path, RFC 6298 (α = 1/8, β = 1/4), **in integer-tick units**, with no 333 ms seed: the model knows its base, and Revision 3's seed defect is not modelled (§6).
- **Seeds:** srtt at `rtt_ticks`, and rttvar at `rtt_ticks/8`, today's fixed `rttvar_secs` in ticks. With constant samples the measured rttvar decays from that seed by 0.75 per sample. That decay is what changes R in the re-captured knob rows; main's laws never read it.
- **Samples:** at the feedback pop, `pop_tick − send_tick` of each never-resent source symbol (Karn). With no queue every sample equals `rtt_ticks`, and `0.875·r + 0.125·r` is exact for an integer r, so the estimate stays bit-equal to the base.
- `retry_delay(p) = round(srtt_meas_p)` ticks.
- **Inertness is a test, not an argument:** on all six golden cells and the knob golden cells, `srtt_meas` equals `rtt_ticks` at every tick.

**The knob laws' inputs.** Revision 3 (§17.1) binds ô as `srtt − RTprop/2`.
- ô is now the **measured** aggregate srtt minus the **configured** aggregate RTprop/2. RTprop = the configured base RTT, exact, as the toy pins it. Before P11, `one_way_s` passes `agg_srtt` as both arguments, so it is srtt/2. That is exact only without a queue, and it is the error Revision 3 fixed.
- **The resend path is pinned** as the aggregate (capacity-weighted) path, the same weighting the delay accounting uses. `best_correction_path` changes from slot to slot, and a per-decision path would make ô depend on slot order.
- R (`ack_timeout`) reads the measured srtt and the **measured RFC rttvar**.
- These laws act only at finite T, so this is a deliberate model change for the knob rows. `test_knobs_golden` is re-captured **once**, in a commit of its own ("deliberate model change: measured RTT in the knob laws, addendum 1, A3"). Main's six rows do not move, because main's laws keep the fixed rttvar.

**The divergence, bounded rather than described.**
- N = 1, standing queue (stream λ steady, below the link rate, constant backlog): for every admitted resend, `|ô − its true one-way delay| ≤ 2 ticks`.
- **N = 2:** for every admitted resend, `|ô − its true one-way delay| ≤ |ô − ô_p| + 2 ticks`, where ô_p = `srtt_meas_p − rtt_p/2` of the path that actually carried it. That bounds the aggregate weighting by the spread between paths, and the maximum and p99 are reported.
- **The fixed rttvar in main's laws (P_lost, completion), bounded:** on every link cell, `|rttvar_meas − rttvar_fixed| ≤ rttvar_fixed + max queue swing / 4`. It is computed, reported and asserted per cell. A cell where the bound fails is a finding for the coordinator (owner question Q-V10), not a tolerance to widen.
- Reported on every link cell: the fraction of L2-admitted resends that arrive after T (`l2_late`).

**P11 done when:**
- the inertness test passes;
- main's six rows are unmodified;
- the knob rows are re-captured in their own commit, after every other test passed without capture;
- the N = 1 bound holds.

### A4. The queue on (P12)

**What fills the queue.** The model has no congestion control (neither has the toy): the sender sends whenever it holds a slot and has something to send. A queue forms when the offered traffic on a path (source at λ, plus repairs at r per source, plus resends) exceeds `1/sv_p`. The only brake is the store pool (512 per path, M01).
- An object (λ = ∞) behind a bottleneck therefore drops continuously, and the drop rate is shown.
- The page's **bottleneck preset** follows the toy's CL cell, sized for the repair overhead the toy lacks. The offered load on the link is `λ·(1 + r) + resends`, where r is the rate the current δ yields: up to about 0.3–0.5 at Realtime on lossy cells. The preset therefore sets `μ = 1.25·λ·(1 + r_est)`, with r_est read from the model's `get_r_star` at the preset's δ before the run (a model accessor, so the page computes nothing), and a 100 ms buffer.
  - Queue drops feed ε̂ and raise r, a feedback loop the preset cannot remove. It is stated on the page and shown by the drop counter.

**Readouts, all model accessors:**
- per path: queue length per tick (a ring the page appends), queue drops inside and outside the outage, and one-way delay (last value, p50, p99);
- cross-path reordering: arrivals with a seq below an earlier arrival from another path;
- link use.

**Page.** A queue-length strip per path under the channel strip, the drop counter, and the one-way delay readout.

**Gates.**
- **Toy vectors:**
  - the forward leg is a **pure function** in its own module `src/link.rs` (no knob identifiers, enforced by the deny test): `fn leg_send(state, t_s, sv_s, q_s, d_s) -> Option<(depart_s, arrive_s)>`;
  - the generator builds a real `toy2.Sim` from a CL-like `Cfg` (`link = dict(C, Q)`, `J = 0`, no sawtooth) and calls its `fwd(t)` for a scripted sequence of send times in seconds;
  - `link.rs` is fed the same seconds with `sv = 1/C` and `q = Q/C`, and every departure, arrival and drop must match **bit for bit**;
  - the tick discretisation, an arrival processed at the first tick ≥ `arrive_s`, is a separate absolute test in `lib.rs`.
- **Absolute:**
  - arrivals are FIFO per path;
  - conservation: sent = arrived + wire-lost + queue-dropped + in flight;
  - the backlog never exceeds `q_p` plus one service time;
  - on a loss-free, queue-free cell, delay from hand-over is **identical for any split** of the base RTT into `d_p` and the reverse leg (N = 1, and N = 2 with equal splits);
  - on a loss-free cell **with a standing queue**, each unit's delay from hand-over equals its queue wait plus `rtt/2`, for any split (no double-count of the belief).
- **UI:**
  - at the "no bottleneck" end the drawn queue is 0 and the drops are 0;
  - the drawn queue samples equal the accessor ring.

**P12 done when:** these gates are green, `build_visualizer.sh` is green, and main's page and its gates are unchanged.

### A5. The resend budget (P13)

**The law, ported from the engine and the toy.** At most `MAX_NACK_REPAIRS_PER_NACK = 10` resends per `NACK_REPAIR_COOLDOWN_US = 5 ms` window (`raptorpath/src/net/mod.rs:227`, `:229`).
- In ticks: windows of 5 ticks aligned at tick 0, `window = tick / 5` in integer arithmetic.
- The gate sits **at the point of sending**, after the P_lost draw succeeds. A refusal falls through to repair and source exactly as a failed P_lost draw does, does not touch `last_retx`, and is retried at the next slot that picks the seq.
- Floors are exempt (control traffic, as in the toy §15.4).
- **Simplifications, as in the toy:** the congestion multiplier is held at 1, and the `nack_cap·sources` term is omitted.
- **Default unlimited:** the limit is `∞` resends per window, and the test `used < limit` is always true. Main's page never sets it.
- **The window form differs from the toy.** The toy windows by `int(now/0.005)` in float seconds, and at exact multiples of 5 ms that can round down. The vector file records each such boundary case, and the integer-tick form is taken as the model's.

**Counters.**
- `budget_used` per window (a ring the page draws as bars);
- `resend_braked_units` = the **distinct seqs** held back at least once (a per-seq set);
- `resend_braked_attempts` = refusals, reported separately and not drawn as the headline. Stage 3c's single counter read 366 504 attempts against 2 031 units because it counted retries of the same decision.

**Gates.**
- **Absolute:** resends per window ≤ the limit, in every window.
- **Identity:** unlimited reproduces every golden row.
- **Toy vectors:** a scripted attempt sequence through `toy2.Sim.send_copy` with `resend_brake = "engine"` gives admit/brake decisions that match, except at the recorded boundary cases.
- **UI:** the drawn bars equal the accessor ring, and the page shows `resend_braked_units` as the headline.

**P13 done when:** these gates are green and both pages build.

### A6. Two paths, H_f and the per-path outage (P14, blocked on stage 3d's P2 result)

- **The leg creates skew at N ≥ 2 even with identical settings.** Per-path queues fill differently under goodput striping. Arrivals then reorder across paths, and the floor stamped on a fast-path datagram can pass a hole whose slow-path copy is still in flight. With H_f = 0 the page would show exactly the defect that stage 3d's P2 cell tests (§17.7).
- Hiding that by testing the path count would be a branch on N. An H_f selector would be a mode bit. Neither is added.
- **The coordinator picks one of two options:**
  - **(a)** P12's page controls ship, with one link setting written to all paths. `late_floor` (late fills of floor-passed holes) is shown, labelled "H_f = 0, not confirmed; stage 3d P2 tests this";
  - **(b)** P12 lands in the model and its gates, but the page's link controls wait for stage 3d P2, so the page keeps the default link until then. A1 and A5 ship regardless.
- **Recommended: (b),** because the coordinator has said two-path display waits for that result. Its cost: the owner's queue request stays off the page until stage 3d reads. Out-of-order (A1) and the budget (A5) ship now. With (a), the queue ships now, along with a labelled display of an open defect.
- **After P2:** the form of H_f the owner confirms becomes the one H_f law in `knobs.rs`. Its vectors come from the toy (`HEst` with `strict = False`, or 0), and per-path link and outage controls open on the page.

**Defaults revisited.**
- **Q-V5 (H_f = 0):** no longer harmless once a forward leg exists, because it is the setting that shows §16's two-path risk. It stays 0 only until stage 3d P2 reads, and then follows the owner's confirmed form (one law, no selector).
- **Q-V6 (floor carried in the header at no cost):** kept. The header adds no service time on the leg, a divergence of about 8 bytes per symbol, stated and not gated. Floor-repeat datagrams now pass through the queue, so they pay service time and can be dropped.
- **Q-V7 (outage on all paths):** kept for now. It now applies at the departure instant (A2). The per-path outage opens with P14.

### A7. Phases after P8

| phase | depends on | done when |
|---|---|---|
| **P9** out-of-order classes, events and the delay-dot chart (A1) | P4, P6, P7 (extends all three) | A1's native and UI gates; `build_visualizer.sh` green |
| **P10** forward-leg extraction with defaults (A2) | P8 | goldens and knob rows unmodified; the inline-delivery test; sender-knowledge measurement reported |
| **P11** the measured RTT and the knob laws' ô and R (A3) | P10 | inertness test; main's rows unmodified; knob rows re-captured in their own commit; the N = 1 ô bound |
| **P12** the queue on: readouts, vectors, page per A6's option (A4) | P11 | A4's gates |
| **P13** the resend budget (A5) | P10 (independent of P11 and P12) | A5's gates |
| **P14** two-path display, the H_f law, per-path outage (A6) | stage 3d's P2 result and the owner's H_f answer | to be registered when unblocked |

- **Order:** P9, then P10, then P13 and P11 (either order), then P12.
- **The rewrite trigger stands as in §2.2:** if any of P10–P13 must reorder the slot loop or cannot be inert at its default, stop and return.
- **Extensions to P0–P8:** P4's consumer gains the class (A1); P6's page and P7's UI gate gain the panels and checks above; P5's knob golden rows are re-captured once in P11.
- **§6 changes:** its rows for "link queue … resend brake", "cross-path reordering" and "the sender learning a resend's fate" are superseded by A2–A6 from P10 on. The rest of §6 stands.

- **Q-V10 (new), the fixed rttvar in main's laws under a queue.** **Default: keep it fixed,** bounded per A3. If a link cell fails the bound, the choice goes to the coordinator: either main's laws read the measured rttvar on link cells (the six goldens are safe, because there is no queue there), or the page labels that cell.

### A8. Final review (advisor)

The advisor reviewed the first push of this addendum (`01999437`). It kept the structure: extraction first, defaults as values, and the two coordinator choices. It found five holes that would have forced the P10–P13 worker into design choices, and seven tightenings. All were taken:
1. **The delay readouts would have inherited the belief.** After P11, `unit_delays` and `record_delivery` would have double-counted the queue excess that the arrival tick already contains. Now one physical accounting term serves every delay readout, ô serves only L2 and `protected`, and a queued-cell test covers it.
2. **`deliver`'s duplicate and late-fill-after-commitment cases** are now specified, along with the cross-path order of deliveries at tick start.
3. **The ABI, the wrapper defaults, the page's dial maps,** the routing-witness getters and A4's UI gates under option (b) are now stated.
4. **The RTT reader table** now includes the arrived-symbol ack block (:2001) and `get_path_rtt_ms` (:2140), and the datagram kinds include the floor repeat.
5. **The measured rttvar's seed** (rtt/8 in ticks) and its effect on the re-captured knob rows are stated.
6. **Tightenings:**
   - undelivered units show during the run, with one superseding event per unit;
   - drop feedback is due at the would-be arrival, keeping the queues monotone;
   - the link vectors compare bit for bit through a pure `link.rs`;
   - the bottleneck preset is sized for repair overhead;
   - the knowledge lead, the N = 2 ô and the fixed rttvar each carry a bounding test, with Q-V10 for the last;
   - the decoder's order independence is checked by a test before any N = 2 link cell;
   - option (b)'s cost to the owner is stated.
