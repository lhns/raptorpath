# Three knobs: the plan for proving it against today's model (plan, no runs, no code)

**Status.** Plan only. Nothing here has been run, and no code has been written for it. It is meant for the owner to sign before any run (§1), so that the cells, readouts and pass rules are fixed before results exist.

**Goal.** Show that the three-knob model (lifetime T, delay price δ, order hold H; `three-knobs.md`) is at least as good as the current model everywhere and better somewhere, or fix it until it is, under a comparison the owner accepts as fair. Then merge it in two parts (§7).

**Base read for this plan.**
- Derivation `docs/derivations/three-knobs.md` on `three-knobs` `7521faf8`: laws §1–§10 and Revisions 1–3 (§13, §15, §17); findings §12, §14, §16, §18 with its amendment, and §19.
- The offline simulation on `toy-stage3-knobs` `e17b4c64`: `tools/toy/toy2.py`, `batch3d.py`, `results-stage3d.md`. The RTT-sampling fix is being re-baselined on `toy-stage3d-rttfix` (not pushed; running).
- Stage 3e registration on `reg-stage3e` `b3dfe3ad`: `docs/derivations/three-knobs-stage3e-registration.md`.
- Interactive simulator on `viz-three-knobs` `82416cf7`, plan `visualizer-three-knobs-plan.md`; `viz-path-fix` `d71e924c`.
- Engine on `main` `46b705fa`: `raptorpath/src/net/shed.rs`, `emit_source.rs`, `sender_phases.rs`, `receiver.rs`, `mod.rs`, `config.rs`; `docs/roadmap/nodes.yaml`.
- `CLAUDE.md` and `docs/measurement-discipline.md`.

Reviewed with the advisor (Fable) before writing, on the baseline check (§2) and the deciding row (§3).

## Short answer

- **Fairness is its own rule set (§0)**, applied to every row. No stage 3d comparison between the old and the new model meets it. Every headline row compares one point of each design, all of them predate the RTT-sampling fix, and none includes the sender that ships.
- **The baseline that ships is not in the offline simulation (§2).**
  - The old rows so far are ADR-0084's sender, which was never built, paired with today's Realtime receiver (`R0e`, which does model `shed_recv_hold`).
  - The shipped Realtime sender differs from it in six ways:
    - its give-up budget is the derived `ε̂·(1 − P_fec)` and not `(1 − ρ)·sent`;
    - its deadline reads the smoothed RTT with a 50 ms seed;
    - it sends no floor and no copy;
    - it cannot resend below the 200-symbol window;
    - it has no source pause;
    - it duplicates first sends on a second path.
  - Today's Bulk and Auto pair is reproduced, by the never-give-up sender with strict order.
  - A new arm, "today's Realtime", is specified for both ends. It is what a merge replaces.
- **The deciding row is not decided by the data yet (§3).** In the capacity-limited outage cell, ADR-0084's sender gains about 0.9 s of recovery over the any-order never-give-up rows in both sweep forms.
  - The stored tables point at one mechanism: the early-skip receiver gives up the roughly 1 000 units in flight when the outage began, and so frees their link slots during the drain. §19's on-time law gives 4.78 s for exactly that give-up, against 4.80 s measured for ADR-0084's sender.
  - The new model gives up that same set at a lifetime of about 1.35 to 1.6 s, before any intake unit expires. So the 2 s row is not at matched share.
  - The same law misses the never-give-up row by 0.42 s, and that miss is unexplained. So the prediction is two-sided: at matched share the new model is 0 to about 0.4 s behind, against a bar of about 0.24 s. The deciding row can fail.
  - A fine T sweep decides it. Revision 4 is drafted only if that sweep lands below beyond noise (§3.5).
- **Stage 3e compares no old sender** (quoted in §4). Seven amendments add them, with repair on and off at the same rate law, and the sweeps of §1.
- **A two-path stage 3f after 3e (§5)** leads with the call point. Strict order within T on two paths is limited by the floor hold. The estimate is `share_T ≈ exp(−λ·p_B·H_f)` (§5.4), with a registered check: about 0.47 at 10 000 units/s after the trigger fix, and about 0.996 at call rates.
- **Robustness is folded into those stages (§6):**
  - the one-way estimate and R biased by ±5, 10 and 20 ms;
  - a reverse-queue cell;
  - the receiver's relative one-way delay as the fix candidate;
  - fine paired sweeps of T, H and δ through every named point.
- **Merge (§7):**
  - part (a) now, docs, simulation branches and the proposal page, with main's page behaviour and goldens unchanged;
  - part (b) the engine, only after the table passes: ADR, paper section, then three guarded steps. The off-Bulk guard rows that part (b) needs do not exist yet.
- **Cost (§9):** about 9 600 runs, about three to four days of machine time, one batch at a time, deciding row first.
- **Owner questions** are in §10, each with a default.

## 0. Fairness (owner: "but make sure that the comparison is fair")

### 0.1 The rules

Every comparison row in every stage below obeys these. A row that cannot obey one says which, before the run, in its own table cell.

- **F1, same trade.** Designs are compared only at matched delivered share, or as whole curves: the on-time share and the recovery time plotted against the share delivered, each design sweeping its own dial. One point of each design is never a comparison. The matching rule is fixed in §1.2.
- **F2, same machinery.** Within a row, the following are identical across every compared arm:
  - repair: the rate law r(δ), the span width A*, the decoder and the loss draw;
  - the resend budget (10 per 5 ms) and the per-seq cooldown;
  - the sweep form;
  - the RTT estimator;
  - the link, the source model and the path placement;
  - δ.

  What a design does with that machinery may differ, and that difference is the design. Examples: its span's lower edge, its give-up rule, its receiver. Where a feature exists in only one design (the dequeue rule, the duplicate first send, the reorder window), both designs run with it off, and with it on where it can be expressed in both. The row says which.
- **F3, same code and seeds.** Every arm compared in a row runs from the same commit, built on the RTT-sampling fix (`toy-stage3d-rttfix`), on paired seeds 0–11 with phases per seed. Numbers are never mixed from different stages. Every old arm is rerun in the batch that compares it.
- **F4, the real baseline.** Every row includes what ships today at that point (§2), not only ADR-0084's sender. Each baseline is tuned as far as its own laws allow:
  - its own dial is swept (ρ for ADR-0084's sender; δ and the retention bool for today's engine);
  - it gets the engine-like sweep as the primary form, because the whole-tail sweep doubles the old arms' duplicates (stage 3d: ADR-0084's sender 0.269 duplicate arrivals per copy against 0.002);
  - it gets the engine's estimator where the engine has one.

  No baseline runs with a setting that the new model avoids and that its own laws would not choose.
- **F5, fixed in advance.** The owner signs §1 before any run. After results are seen:
  - no cell, arm, readout, sweep point or bar is added, dropped or moved;
  - every row is reported, losses included;
  - a reading-code bug may be fixed and must be reported, as stage 3d did;
  - a supplement run is reported as unregistered and never enters a reading.
- **F6, rows are named by behaviour.** A row's name says what its sender and its receiver do. Code names (`C1`, `S5`, `R0e`) appear only in parentheses.

**Row names used from here on:**

| name | sender | receiver | code |
|---|---|---|---|
| today's Realtime | sheds a hole silently when it is older than half the smoothed RTT, within an `ε̂·(1 − P_fec)` budget; resends only inside the 200-symbol window; no store pause | in-order hold of SRTT/2 while its ε̂ budget is open, else 4·SRTT clamped to [60, 300] ms | new arm, §2.3 |
| today's Bulk | never gives up | strict order | `C3` |
| today's object API | never gives up | any order | S5 at T = ∞, H = 0 (equal to it by identity) |
| ADR-0084 at ρ | gives up a hole older than b(δ)·RTprop while `(1 − ρ)·sent` allows, with a copy and a floor | today's Realtime receiver | `C1` at ρ = 0.99 |
| lifetime T, hold H | gives up what cannot arrive by T; announces a floor on its data | releases a hole H after detection, or at the floor | `S5` (with repair: `RL`) |

### 0.2 The stage 3d comparisons against these rules

| comparison (stage 3d, `e17b4c64`) | F1 | F2 | F3 | F4 | F5 | F6 | verdict |
|---|---|---|---|---|---|---|---|
| CL-long recovery: ADR-0084 at 0.99 (99.2 % delivered) against lifetime 2 s (99.9 %) and 0.4 s | **breaks**: one point each, at 99.2, 99.9 and 92.4 % delivered | partly: no repair in either (equal, but no "on" arm); ADR-0084's deadline reads the exact propagation RTT, an idealisation in its favour; the quoted whole-tail rows load the old arms with duplicates | **breaks**: before the RTT fix. The 0.4 s rows move +1.76 points; ADR-0084's sender sends floors and is not listed among the arms the fix leaves unchanged, so it is per the re-baseline | **breaks**: ADR-0084's sender was never built, and today's Realtime is absent | **breaks**: the arms were registered (§17.9), but no pass rule compared them; the matched-share comparison is post hoc | tables yes, prose no | not a comparison |
| CL (10 s) recovery: "not within the run" against 0.1 s | breaks | as above | breaks for the 0.4 s rows | breaks | registered | yes | run geometry (§16), not evidence |
| OV, within 0.4 s: lifetime 0.4 s at 83 % against the never-give-up rows | breaks | the dequeue rule exists only in the new design. That is a real design difference, but today's Realtime has no source pause and sheds at the link queue, which no arm models | OV moves only past the third decimal | **breaks**: no arm without a source pause, so the shipped overload behaviour is unmeasured | registered | yes | not a comparison |
| OV-long, within 2 s: lifetime 2 s at 80.5 % against 0 % | breaks | as OV | ok | breaks, as OV | **partly**: the window placement ("after the first dequeue expiry of the same seed's 2 s run") was a worker decision, applied to every arm | yes | not a comparison |
| Strict-order fix: the protected floor against the verdict floor (ablation) | not needed: the same design and T, a mechanism check | ok | breaks (pre-fix), small for OV | n/a | the liveness supplement `SUP-S5v` was added after the runs, and was kept out of the readings | yes | a valid ablation, re-read after the fix |
| P2, two paths: never give up against lifetime 0.075 s and 0.4 s | breaks: one T | **breaks**: the reorder window is not the engine's rule. It makes 30 % of the baseline's copies duplicates and suppresses every resend at 0.075 s | ok (P2 is unchanged by the fix) | breaks: no old arm below ρ = 1, and today's Realtime duplicates first sends on the second path rather than striping | registered | yes | not a comparison |
| K cells: lifetime 0.4 s against stage 3c's sender | same design | ok | ok | n/a | registered | yes | a valid self-check |
| Unplanted nudges and the A/A | same design | ok | ok | n/a | the long-cell nudges were added after the runs began, but before any reading | yes | valid; 24 undetermined are neither passes nor failures |
| §19's T sweep 0.36–0.44 s (380 runs) | same design | ok | before the fix | n/a | **report only, unregistered** | yes | evidence of method, not a result |

**So no old-against-new claim stands yet.** What stage 3d established is a set of properties of the new design:
- identity at T = ∞;
- the commitment holding;
- the overload floor fix;
- recovery at T = 0.4 s.

The comparison has to be made from scratch under §1.

## 1. The acceptance table (signed by the owner before any run)

### 1.1 Readouts

Every arm reports every readout, over all units handed over. A unit that is never delivered, or is given up, counts as not delivered.
- **Delivered at all**, `share_any`: the x axis of every curve.
- **On time at the use case's deadline τ** (`share_by_τ`, hand-over to delivery ≤ τ): the first y axis. τ is 0.4 s for a call and for a tunnel datagram stream at Realtime, and 2 s for buffered video. Files and Bulk tunnels have no deadline; their y axis is completion time, read lower is better.
- **On time at the application's own T** (`share_T`), reported beside `share_by_τ` so that a design is not judged only by a deadline it was not given.
- **Recovery time after the outage**: §17.4's window rule at τ, resolution 0.1 s. This is the second y axis in outage cells.
- **Monitored, no pass rule; a shown regression is reported as a finding:**
  - delay from hand-over, p50, p90 and p99;
  - out of order per delivered;
  - copies and repairs per delivered;
  - duplicate arrivals per copy;
  - link use;
  - queue drops inside and outside the outage;
  - budget refusals;
  - late fills of floor-passed holes;
  - intake expiries.
- **Absolute gates, each must be 0 or true in every run:**
  - `finished`;
  - L1 violations (no payload staged at age ≥ T);
  - commitment violations (no copy below an announced floor);
  - the worth-edge violation of stage 3e;
  - the T = ∞ identity with the never-give-up arm, on every key and trace.

### 1.2 The curves and the matching rule

- **Each design sweeps its own dial over a registered set:**
  - **new model:** T over a set per cell (§1.4), always including T = ∞, at H = 0 for the curve and H = ∞ at three of the T points;
  - **ADR-0084 at ρ:** ρ ∈ {0.95, 0.98, 0.99, 0.995, 1};
  - **today's Realtime:** δ ∈ {0.5, 5, 50} with the budget derived (the engine has no ρ number, §2.1);
  - **today's Bulk and object API:** single points. Their retention is a bool, `window_reliable`, so they have no curve.
- **A design's curve** is its sweep points (x = delivered at all, y = a y readout), sorted by x and joined by straight lines. Where two points have x within noise of each other, the better y is kept, which is the design tuned as well as its law allows.
- **A comparison at a baseline point (x_b, y_b):**
  - for each seed, interpolate the new curve of that seed at that seed's x_b;
  - take the paired difference to the baseline's y_b;
  - read it with `summarize2.read` against `m = max(T_bar, NF)`.
  - T_bar: for shares, `max(1/units, 0.05·(1 − share_ref))` (stage 3); for recovery and completion time, `max(5 % of the reference, 0.1 s, NF)` (stage 3d worker decision 12, now registered).
  - NF: the stage's frozen A/A floor.
- **Curve against curve** (the new model against ADR-0084's ρ sweep): compared at every point of each curve that falls inside the other's x range, in both directions.
- **No extrapolation.** A baseline point outside the new curve's x range is reported as "outside the swept range" and counts as not passed. The registered T sets reach low enough that this should not occur. If it does, it is a finding and not a reason to add a point.

### 1.3 The pass rule

The new model passes when all of these hold:
1. **Nowhere below.** In every cell, no comparison of §1.2 on `share_by_τ` or recovery time reads "shown worse" for the new model.
2. **Above somewhere.** At least one comparison outside the identity cells reads "shown better".
3. **Gates.** Every absolute gate of §1.1 holds in every run.
4. **Identity cells.** Every T = ∞ point equals today's Bulk or object API on every key. These cells have no "better" to show.

A failed row follows §8: a derived fix, then a rerun of that row only.

### 1.4 The cells

The one-path cells run in stage 3e as amended (§4). The two-path cells run in stage 3f (§5). Every cell runs with repair off and with the live rate law (`RL`, true loss; `RE`, estimated loss, where 3e has it), so F2 holds both ways.

**Common rows in every cell.**
- The baselines are §0.1's rows. Today's Realtime and ADR-0084 at ρ run at the cell's δ. Today's Realtime also runs at δ = 50, its named point.
- The new model runs at the use case's named point, with the T sweep listed.

| use case | paths | condition | cell (new marked †) | τ | new model: point, then T sweep | today's point it replaces |
|---|---|---|---|---|---|---|
| call | 1 | clean | RT (3e: L10, 50 ms, 50/s, no outage) | 0.4 s, and 0.12 s as in 3e | T = 0.4, δ = 50, H = 0; T ∈ {0.08, 0.10, 0.12, 0.14, 0.16, 0.20, 0.40, ∞} | today's Realtime |
| call | 1 | outage | K-I2, K-L10 (1.5 s outage at 20 s) | 0.4 s | as RT | today's Realtime |
| call | 1 | overload | K-OV† (50/s offered, link 40/s, queue 1 s of data) | 0.4 s | T ∈ {0.2, 0.3, 0.4, 0.6, 1.0, ∞} | today's Realtime |
| tunnel datagrams at Realtime | 1 | clean | CLc† (CL with no outage) | 0.4 s | T = 0.4; T ∈ {0.4, 1.0, ∞} | today's Realtime |
| tunnel datagrams at Realtime | 1 | outage | **CL-long (the deciding row, §3)**, CL, CLw-long | 0.4 s | T ∈ {0.4, 0.6, 0.8, 1.0, 1.2, 1.25, 1.3, 1.35, 1.4, 1.45, 1.5, 1.55, 1.6, 2.0, ∞}; H = ∞ at {0.4, 1.4, ∞} | today's Realtime |
| tunnel datagrams at Realtime | 1 | overload | OV, OV-long | 0.4 s and 2 s | T ∈ {0.4, 0.6, 1.0, 2.0, ∞} | today's Realtime |
| buffered video | 1 | clean, outage, overload | CLc-v†, CL-long-v†, OV-long-v† (the same cells at δ = 0.005) | 2 s | T = 2, δ = 0.005, H = 0; T ∈ {1.0, 1.5, 2.0, 3.0, ∞} | no shipped point. Compared with today's Auto (never gives up, strict) and with today's Realtime at δ = 0.005 |
| file, Bulk and Auto tunnel | 1 | clean, outage | Ec† (E-I2 with no outage), E-I2, M-I2, CLw-long at T = ∞ | completion time, longest stall | T = ∞ only (identity) | today's Bulk; today's object API |
| file | 1 | overload | none: an object has no deadline, so overload only slows completion. CL-long at T = ∞ reports that | — | — | — |
| call | 2 | clean, outage | P-call, P-call-out, P-lossfree (§5) | 0.4 s | T ∈ {0.2, 0.3, 0.4, 0.6, ∞} × H ∈ {0, ∞} | today's Realtime, which duplicates on the second path |
| tunnel datagrams at Realtime | 2 | clean, outage, overload | P2, P2-out†, P2-OV† (a per-path link, §5) | 0.4 s | as the one-path tunnel | today's Realtime |
| stress | 2 | clean | P2 at 75 ms, strict order (§5.4) | 75 ms | T ∈ {0.065, 0.075, 0.085, 0.1, 0.4} | reported against its bound, not by the pass rule |
| buffered video | 2 | clean, outage, overload | P2-v, P2-out-v, P2-OV-v (P2, P2-out and P2-OV at δ = 0.005) | 2 s | T = 2, δ = 0.005, H = 0; T ∈ {1.0, 1.5, 2.0, 3.0, ∞} | as one-path video |
| file | 2 | clean, outage | P2 and P2-out at T = ∞ (identity) | completion time, longest stall | T = ∞ only | today's Bulk; today's object API |
| file | 2 | overload | none: as one path, an object has no deadline. P2-OV at T = ∞ reports it | — | — | — |

## 2. The baseline: what a merge replaces

### 2.1 What ships (`main` `46b705fa`)

- **The retention dial is a bool, not a number.** `config.rs:293–295`: `window_reliable = config.window_reliable.unwrap_or(protocol_hint != ProtocolHint::Realtime)`. Bulk and Auto retain until acked. Realtime evicts.
- **Bulk and Auto (retain):**
  - never give up;
  - a store pause at the store cap (`net/mod.rs:2473`, `tx_paused = reliable && (store_len >= effective_store_cap || cwnd_full)`);
  - strict order (`ReorderBuffer::new_reliable()`), or any order with `window_out_of_order`.
- **Realtime (evict) sender** (`RWM_UNIFIED_SHED`, on by default; `shed.rs`, `emit_source.rs:629–642`, `:774–790`, `sender_phases.rs:845–870`):
  - **Shed a hole iff** `age > D ∧ (shed_total + 1) ≤ budget·src_total`. Here age is from first send; `D = min(b(δ)·est.rtt(), 2·est.rtt())`; `est.rtt()` is the smoothed RTT (`ewma_rtt`, seeded at 50 ms); `budget = residual_loss_after_fec(ε̂, r, A*, σ²) = ε̂·(1 − P_fec)`, which is 0 on a cold estimator.
  - The shed is silent: the seq leaves the retransmit buffer, and no floor or copy is sent.
  - The store is metadata only. The bytes die when the encoder window evicts them, so a hole more than `MAX_WINDOW_SIZE = 200` symbols behind cannot be resent, and the NACK site clamps gaps to the window before the shed check (`rec-evict-blocker-unresolved`).
  - There is no source pause, because `tx_paused` is reliable-only.
  - There is a duplicate first send on the second-best path (`emit_source.rs:508`, a recorded hint debt) and a single best-path placement.
- **Realtime (evict) receiver:**
  - an in-order buffer with hold `shed_recv_hold` (`shed.rs:58`): SRTT/2 while `holes_given_up < ε̂_recv·frontier`, else `clamp(4·SRTT, 60, 300 ms)`;
  - a force drain at `reorder_max_size = 500`.

### 2.2 Which arm in `toy2.py` reproduces it

| arm in `batch3d.py` | sender | receiver | reproduces |
|---|---|---|---|
| `C3` = `S1c`, ρ = 1, `R1` | never gives up (allowance `floor((1 − ρ)·sent) = 0`), store pause at B | strict order | **today's Bulk and Auto**, up to the sweep form (the engine's is the blocker form, so the `@b` cells are the faithful ones) and with no repair until stage 3e |
| S5 at T = ∞, H = 0 | never gives up | any order | **today's object API**, by the identity |
| `C1` = `S1c`, ρ = 0.99, `R0e` | ADR-0084: allowance `(1 − ρ)·sent`; deadline `D_rt = min(b·rtt, 2·rtt)` on the exact base RTT (`d_arm = "RT"`); a copy and a floor at each verdict; store pause at B | `R0e`: `shed_recv_hold` with the engine's per-batch ε̂ EWMA (`recv_eps = "engine"`) | **the receiver only.** The sender is ADR-0084's, never built |
| `C2` = `S1c`, ρ = 0.99, `R1` (stage 3) | as `C1` | strict | neither |

**No arm reproduces today's Realtime sender.** The receiver half exists (`R0e`). The pause law is the same in `C1` and S5 (O counted on receiver feedback; `C1`'s `resume_on_verdict` re-check is inert, because a verdict does not change O). So same-machinery holds for the pause.

### 2.3 The missing arm, "today's Realtime" (new code, registered with stage 3e's amendment)

New `Cfg` fields. Each defaults to the stage 3e behaviour, so the identity of §4 holds. Arm `E-rt` = `S1c` with:
- `budget = "resid"`: the shed allowance is `ε̂_s·(1 − P_fec(r, A*, ε̂_s))·src_sent`, refreshed on each source emission as the engine does.
  - ε̂_s is the sender's loss estimate. It uses the same estimator as 3e's `RE` arm where repair is on, and the true loss under 3e's Q21 default otherwise; the row says which. It is 0 before the first estimate.
  - The cumulative count is compared as `shed_allowed` does.
  - P_fec is ported exactly as shipped: `raptorpath-math` `p_fec_normal`, including its cut (0 when r ≤ 0 or `r·(1 − ε) ≤ ε`; recorded debt W22, the "0 → ½ step"). So with repair off the budget is ε̂.
  - **Today's Realtime never ships with repair off** (`RWM_UNIFIED` and `RWM_TAPER_R` are on by default). Its repair-off rows are a machinery-matched control for F2, labelled as such, and are never quoted as "today".
- `d_arm = "EWMA"` (already in `toy2.py`): `D = min(b(δ), 2)·srtt`, with the 50 ms seed until the first sample.
- `verdict_copy = False`, `floor = None`: the shed sends nothing, and the receiver learns only through its own hold.
- `evict_window = 200`: a hole more than 200 seqs below the send frontier is neither served nor shed; it is left, as the engine leaves it. This reproduces the stalled-blocker defect. That is intentional, because it is what ships.
- `pause_B = ∞` for this arm: no source pause, so the cell's link queue drops instead.
- Two paths: `place = "dup"`, the first send on the best path plus a duplicate on the other.
- Receiver `R0e` as is, with `reorder_max_size = 500` added (the force drain).
- Not modelled, a recorded divergence: the probabilistic correction-slot retransmit (`emit_source.rs:760–790`, P(retransmit) = P_lost(age)). That is the repair plane's slot ARQ; stage 3e's repair stands in for that plane in both designs.

**Tests, red first:**
- the budget reads 0 cold;
- a hole past 200 behind the frontier is never served;
- a shed sends nothing;
- with no link, `pause_B = ∞` equals the pause build;
- with `budget = "resid"`, `d_arm = "EWMA"`, the copy and the floor removed, and `evict_window` and `pause_B` set to infinity, `E-rt` equals `C1`'s sender at the allowance that `ε̂(1 − P_fec)` produces. This checks the arm by taking it apart.

### 2.4 Why ADR-0084's sender stays

It is the old model's best-tuned law, and the only one with a continuous retention dial. It is the curve that F1 needs for the old side. Today's Realtime gives points.

The pass rule (§1.3) runs against both. The new model must not be below today's points, and must not be below ADR-0084's curve.

## 3. The deciding row: CL-long at matched share

### 3.1 The numbers (stage 3d, before the RTT fix; seconds after the outage end)

| row | sweep | delivered at all | within 0.4 s | recovery | copies per delivered | budget refusals | out of order per delivered |
|---|---|---|---|---|---|---|---|
| ADR-0084 at 0.99 | whole-tail | 99.2 % | 64.2 % | 5.68 | 0.0473 | 260 555 | 0.0258 |
| ADR-0084 at 0.99 | engine-like | 99.2 % | 68.6 % | **4.80** | 0.0301 | 989 | 0.0254 |
| today's Bulk (never gives up, strict) | engine-like | 100 % | 60.9 % | 6.73 | 0.0402 | 153 458 | 0 |
| never gives up, any order | engine-like | 100 % | 65.0 % | **5.67** | 0.0402 | 153 458 | 0.0347 |
| lifetime 2 s, any order | engine-like | 99.9 % | 65.3 % | 5.63 | 0.0396 | 142 356 | 0.034 |
| lifetime 0.4 s, any order | engine-like | 92.3 % (94.1 % after the fix, whole-tail) | 92.3 % | 0.10 (0.00 after the fix) | 0.0335 | 0 | 0.0298 |

- Order alone is worth about 1.06 s (6.73 against 5.67).
- ADR-0084's sender is about 0.87 s ahead of the any-order rows, and about 0.9 s in the whole-tail form too. So the gap is not the receiver's order.
- The 2 s lifetime equals never giving up, because no unit waits 2 s.

### 3.2 What gives ADR-0084's sender its second

**Not its verdict copy.** A copy adds a send, so it can only slow the drain.

**Its receiver's give-up, which in the offline simulation is also a sender give-up.**
- When `R0e`'s hold expires, `pass_below` removes the hole from `holes`. The next report no longer lists it, and its cumulative ack moves past it (`toy2.py` `pass_below`, `on_tick`).
- So the sender never resends a hole its receiver passed, and the outstanding count falls with it.
- Right after the outage the receiver's ε̂ EWMA is high, so the budget `holes_given_up < ε̂·frontier` is open. The hold is SRTT/2 ≈ 5 ms, and the receiver passes the outage's holes almost at once.

The stored rows agree with this:
- **Undelivered 0.8 %.** That is about 0.5 % for the roughly B = 1 000 units in flight when the outage began (lost, then passed), plus about 0.3 % for holes whose one resend was lost (3 % × 10 %) after the receiver had passed them.
- **Budget refusals 989 against about 150 000.** This sender hardly tries to resend the outage burst.
- **Copies per delivered 0.0301 against 0.0402.** About 2 000 fewer copies over the run, so about 1 % more headroom during the drain.

**§19's on-time recovery law gives the size.** With headroom `μ_eff − λ`, `μ_eff = C·(1 − ε) = 12 125/s`, `λ = 10 000/s` and outage L = 1.5 s:

```text
   t_rec  =  ( backlog / μ_eff  −  τ ) / ( 1 − λ/μ_eff )
   never give up:     backlog = λ·L + B_lost = 16 000   →   t_rec ≈ (1.320 − 0.4) / 0.175 = 5.25 s      (measured 5.67)
   B_lost given up:   backlog = λ·L          = 15 000   →   t_rec ≈ (1.237 − 0.4) / 0.175 = 4.78 s      (measured, ADR-0084's sender: 4.80)
```

| symbol | provenance |
|---|---|
| C = 12 500/s, λ = 10 000/s, L = 1.5 s, τ = 0.4 s | the cell (declared) |
| ε = 0.03 | the cell's first-send loss |
| B_lost ≈ 1 000 | the store pause at B, units in flight at the outage start |
| the law | §19, checked there against the interactive simulator (4.78 s predicted, 4.90 s read) |

- The B_lost term is 0.47 s, and the law with B_lost given up hits ADR-0084's row (4.78 against 4.80 s).
- **The law misses the never-give-up row by 0.42 s, and that miss is unexplained.** The lifetime 2 s row behaves like that row (0.0396 copies per delivered), so the miss applies to it too.
  - The copy difference does not explain it. Of the about 2 000 extra copies, about 1 000 or more are the B_lost resends already counted in the backlog term. About 600 are second resends (0.3 % × 200 000).
  - Under the engine-like sweep the outage carries only blocker resends, so few copies are spent into the outage.
- So the mechanism is mostly the receiver's give-up of the in-flight units. The 0.42 s is open, and so is the split of the give-up between the sender's allowance and the receiver's passes. §3.4 reads both.

### 3.3 The matched point, predicted before the sweep

- **In-flight outage units.** They were sent at about 2.0 s and are about 1.5 s old at the outage end. Their earliest resend waits behind the link queue (up to Q/C = 0.1 s) plus ô. So they are unprotected, and given up at once, for every T below about 1.6 s.
- **Intake units.** They wait at most the drain wait `(λL + B_lost)/μ_eff ≈ 1.32 s`. So nothing in the intake expires while `T > 1.32 + R ≈ 1.34 s`.
- **Prediction 1, a plateau.** For T in about (1.34, 1.6) s the new model gives up exactly B_lost plus the lost-resend tail older than T − ô. That gives delivered at all ≈ 99.4–99.5 %, recovery ≈ 4.78 s on the law's scale, and on time ≈ 1 − (L + t_rec)/20 s ≈ 68.6 %. ADR-0084's row reads 68.6 % on time and 4.80 s.
- **Prediction 2, a straight line below it.** For T below about 1.34 s the intake head expires (dequeue rule, §13.2). Each expired unit frees one slot, so recovery falls by `1/(μ_eff − λ)` s per unit, and delivered share falls in step. The curve is a straight line from the plateau down to the 0.4 s point (94.1 %, 0 s).
- **Prediction 3, the comparison, two-sided.** ADR-0084's point (99.2 %) lies just left of the plateau, on that line.
  - If the law's 0.42 s miss belongs only to the never-give-up drain, the new curve there matches ADR-0084's recovery.
  - If the miss carries over to the plateau, the new curve is up to about 0.4 s behind.
  - The bar is about 0.24 s (5 % of the reference). So the row can read "shown worse", and §3.5 is a live branch, not a formality.
- **Registered bands**, to be re-centred on the re-baseline values with their widths kept (as stage 3e does):
  - recovery at the matched share in [4.6, 5.3] s;
  - matched T in [1.25, 1.6] s;
  - plateau delivered share in [99.3 %, 99.6 %].

### 3.4 Diagnostic reads and runs (the stored outputs `out3d/` are not on disk)

1. **Read, no new run.** The re-baseline on `toy-stage3d-rttfix` regenerates CL-long@b with `windows = True`. From it:
   - the per-window first sends, resends and braked attempts over 3.5–6 s, for ADR-0084 at 0.99 and for the 2 s lifetime;
   - `pass_r0e`, `verdicts`, `verdict_copies`, `p1_viol` and `p1_worst` for ADR-0084's row. `p1_viol` says whether its receiver gives up past the sender's 1 %. If it does, the row is not "ρ = 0.99" at all, and it is reported as such.
2. **Registered, run only if the matched-share comparison of §3.3 reads "shown worse":**
   - a 2 × 2 cross of sender {ADR-0084 at 0.99, never gives up} × receiver {today's Realtime receiver, any order};
   - ADR-0084's sender without its copy (`S1`);
   - all in CL-long@b, 12 seeds, repair off. That is 5 configurations and 60 runs. It splits the second into a sender part and a receiver part.

### 3.5 Revision 4, only if the sweep lands below beyond noise

- **If the gap is concentrated give-up,** a lower T already gives up the same units (§3.3), and no law changes.
- **If the cross shows the gap is the drain-time copy rate** (the old sender resends less during the backlog), the candidate law is L2's full form (§2.2) with a slot cost that has provenance:

```text
   resend s   ⇔   P( owd ≤ T − a(s) )  ≥  w / (1 + w) ,        w  =  W_COST_RATIO · ( 1 + P_exp )
```

| symbol | provenance |
|---|---|
| W_COST_RATIO = 1 | the existing register row (`net/late.rs:64`): one wire symbol |
| P_exp | measured: the fraction of dequeues in the current refresh window that expired the head (§13.2's rule firing). A resend slot then displaces one unit that will expire |
| P(owd ≤ ·) | §3.2's normal RTT model, as in §2.2 |

- At T = ∞ and with an empty intake, P_exp = 0, so w = 1 and the law reduces to today's L2. It is continuous in P_exp, and it has no new constant.
- Its effect is small on a 5 ms path, where the RTT spread is tiny. That is stated now, so a weak result is not tuned upward.
- **The order of work if it is needed:** derive it in the paper style with a bind gauge for P_exp, register it as a rerun of CL-long, CL-long-v and OV only (§8), then read it.

## 4. Amendments to stage 3e

**The registration does not compare the old senders.** Quoted from `three-knobs-stage3e-registration.md` (`reg-stage3e` `b3dfe3ad`):

> **What stage 3e adds.** The stage 3d sender S5 gains proactive repair, coded over the engine's trailing span at the rate law's r(δ), and the receiver gains a decoder. Everything else is S5 as the re-baselined stage 3d code runs it.

Its §5 arms table lists only the S5 family. "All arms: δ = 50, H = 0. T = 0.4 s unless stated". Its only never-give-up rows are

> | `S5-inf`, `RL-inf`, `RS-inf` | as S5, RL, RS at T = ∞ (never gives up, any order) | | | CL-long, CLs-long |

There is no ADR-0084 row, no strict-order never-give-up row, no today's Realtime row and no T sweep.

**Amendments, registered before 3e runs.** The registered arms, cells and predictions are untouched. Its stop rules apply to every added arm.

- **A1, the baselines with repair.** Every old row of §0.1 is added to every one-path cell of §1.4, under repair off and under 3e's `RL` and `RE` (where 3e runs `RE`). It uses the same rate law r(δ), span width A*, decoder and loss draw as the new arms. Each design keeps its own span lower edge:
  - the new model keeps 3e's worth edge `a + ô < T`;
  - ADR-0084's sender and today's Bulk keep paper §5.3's oldest unresolved seq;
  - today's Realtime keeps the oldest seq still in its 200-window and not shed.
- **A2, the sweeps of §1.4** for the new model and for each baseline.
- **A3, the new cells:** CLc, K-OV, Ec and the video variants at δ = 0.005. Each gets an A/A row in `nf_stage3e.json`.
- **A4, the arm today's Realtime** (§2.3), with its tests in the red-first list. The identity of every existing 3e arm with new fields at defaults is part of `check_ident3e.py`.
- **A5, the primary sweep form** is the engine-like one in every link cell (3e already does this). The whole-tail form is not run, so the old arms are not loaded with duplicates.
- **A6, the RTT fix.** Every arm, old ones included, runs on the fixed code in the same batch. No stage 3d number enters a 3e comparison.
- **A7, robustness and smoothness rows** (§6), folded in as their own parts with their own markers.
- **Run order inside 3e:**
  - 3e's steps 0–7 unchanged;
  - then **part DECIDE** (CL-long's sweeps and baselines, §3);
  - then the registered MAIN;
  - then the amendment parts.

  If DECIDE fails its pass rule, the batch stops (a `STOP` marker), and the rest waits for the §3.5 decision. A design that will change is not run across every cell.

## 5. Stage 3f: two paths (registered after 3e reads)

### 5.1 The named call point first

**P-call:** a call at 50 units/s, paths at 10 and 60 ms, I2 loss on each, T = 0.4 s, δ = 50, H = 0 and H = ∞, with repair off and with `RL`. Baselines as §1.4. Today's Realtime runs with its duplicate placement. Every arm also runs with striping (φ = 0.5) and with duplicate placement (F2: the placement is machinery).

### 5.2 The two unmasking cells

- **P-callrate:** P2 at the call rate (50/s), T ∈ {0.075, 0.4}, H = ∞.
  - At 50/s the next unit follows 20 ms later, so a lost fast-path unit no longer keeps the floor about 65 ms behind (stage 3d's masking).
  - Registered prediction at H_f = 0: late fills of floor-passed holes ≈ the slow-path share of units in flight when the floor rises past them, read from §17.7's arithmetic with the call rate.
- **P-lossfree:** P2 with a loss-free fast path and 3 % on the slow path, at 10 000/s.
  - No fast holes means no trail, so the floor reaches the send frontier and passes slow-path units in flight.
  - §17.7's registered 35–50 % late fills at H_f = 0 is re-registered here. With H_f estimated, the prediction is ≤ 3 % and 0 after 1 s.

### 5.3 An old baseline without the reorder-window qualifier, and the cold-start fix

- **The engine's own multipath gap gate for every arm.** Before the build, the worker reads the engine's gate from `sender_phases.rs`: the per-path threshold `mp_thr_pure(&mp_clocks, p)` at the gap serve. The offline simulation then models it. The RACK window with the measured skew stays as the secondary form, run for every arm, so neither design has a gate the other lacks.
- **The cold-start RTT seed** (stage 3d: 105 late fills per run, all in the first 0.1 s, from the 333 ms seed).
  - The derived rule: a path with no RTT sample has no evidence for a trigger or a one-way delay. So `protected` uses the bounds that cannot pass data in flight, `d(s) = 0` and `ô = 0`, until that path's first sample. The protection test then reads L1 alone, `a(s) < T`.
  - This adds no constant. The cost is at most one round trip at start in which nothing is given up early.
  - The test: 0 late fills among units handed over before each path's first sample, in every P cell.

### 5.4 The stress cell: 75 ms with strict order, and whether it is feasible

**Setup:**
- fast path one-way a = 10 ms, slow path b = 60 ms, skew s = b − a = 50 ms;
- resends on the fast path, so ô ≈ a;
- the receiver's floor hold H_f ≥ s, needed so the floor does not pass slow-path data in flight (measured 50.8 ms);
- strict order (H = ∞).

A unit u handed over at t_u is on time iff every hole h before it is released by `t_u + T`. Here `t_rel(h) = min(fill(h), t_F(h) + H_f)`.

**Three kinds of hole:**
- **A, given up at birth** (unprotected when sent). The floor rides the next fast datagram, so `t_rel − t_h ≈ a + H_f ≥ b`. Units right behind it are on time iff `T ≥ a + H_f ≈ owd_slow`, plus a tick. This is feasible at 75 ms.
- **B, protected, then neither filled nor resent in time.** Protection lasts to age `T − ô`, so `t_rel − t_h ≈ T − ô + a + H_f`. Every unit handed over within about `a + H_f − ô ≈ H_f` after h is late. This is infeasible for any H_f > 0. Stage 3d's strict delay of 121 ms matches `(T − ô) + a + H_f = 65 + 10 + 50 = 125 ms`.
- **C, filled by a resend** before `t_u + T`. This is feasible iff the resend's trigger plus ô leaves room.

**Why case B dominated P2.** The reorder window made every fast hole a resend candidate only at about 63.5 ms (`results-stage3d.md`, item 3 under "Things that looked wrong"). But `protected` assumed the trigger at srtt_A ≈ 20 ms. That is a trigger seam of the same kind as §17.1.

**The derived fix:** the trigger delay reads the candidate gate, `d(s) = srtt_{path(s)} + skew_hat` for a report-driven trigger under the reorder window. Then a hole that cannot be resent in time is case A, and the others are case C.

**What stays case B after the fix.** A resent hole whose resend is lost is unprotected after its resend (`d = c`), and is passed at `t_resend + a + H_f`. That is still late by about H_f. So:

```text
   share_T(H = ∞)   ≈   exp( − λ · p_B · H_f ) ,      p_B  ≈  φ_A · p0 · pc          (a fast-path loss whose one resend is also lost)
```

| symbol | provenance |
|---|---|
| λ | the cell's rate |
| p_B | the per-unit rate of case-B holes: the fast-path share φ_A = 0.5, the first-send loss p0 = 0.03, the resend loss pc = 0.10 (cell I2, `toy.CELLS`) |
| H_f | measured, the floor hold, about the skew |
| the form | a Poisson count of case-B holes in the lateness span of about H_f before a unit, once H_f holds its sample |

- At 10 000/s: `exp(−10 000 × 0.0015 × 0.05) ≈ 0.47`.
- At 50/s: about 0.996.
- **It is an estimate, not a proven bound.** Before the fix p_B ≈ 0.015 (no fast hole is resent), so the estimate is about 6·10⁻⁴, but 2.0 % was measured.
  - The likely reason is that the P2 run is only 1.2 s long. In its first part the floor hold has no sample (H_f starts at 0, so there is no lateness span), and those units are on time.
  - **Registered check:** the estimate is read only on units handed over after the floor hold's first sample, and that reading is reported beside the whole-run share. If the scoped reading still exceeds the estimate beyond noise, the form is wrong, and that is a finding.

**Stated as a limit:** strict order within T, with T near the path spread, is feasible at call rates and limited to about one half at 10 000/s by lost resends. It is reported against the estimate and is not under the pass rule.

### 5.5 Two-path overload and capacity outage

- **P2-OV and P2-out** need a link per path. This is new code: one `link` per path, with the same FIFO and drop rule as the one-path link.
- The cells mirror OV and CL-long, with capacity split 1:1.
- They carry the same baselines and sweeps as one path.

## 6. Robustness, folded into 3e and 3f

- **One-way estimate bias** (3e cells OV and CL-long; 3f P-call). The new model runs with ô biased by ±5, ±10 and ±20 ms.
  - Under overload, delivery sits at T − R + d_fwd, so the dequeue margin R is biased by the same six amounts as separate arms.
  - §18 attributed the 10 ms sensitivity to ô, but the dequeue rule reads R (§13.2).
  - Predictions from the formulas: a +x bias gives up x earlier, so `share_T` falls by about λ·x/units per outage; a −x bias in OV sends units that arrive x late, so `share_T` collapses once x exceeds the slack `R − d_fwd`.
  - No pass rule. The readings bound how much error the design tolerates.
- **A reverse-direction queue cell, RQ** (3e, new): CL-long with a standing queue on the report direction, sized like the forward queue.
  - ô = srtt − RTprop/2 assumes all queueing is on the data direction (Q18). So here ô overestimates, and the design gives up early.
  - Every §0.1 baseline runs in RQ too, because today's deadline also reads the smoothed RTT.
- **The fix candidate, the receiver's relative one-way delay.**
  - The receiver measures `d_rel = t_arr − t_send_stamp` (clock offset unknown) and reports `q_f = d_rel − min_W d_rel`, the forward queue, exact without clock sync.
  - The sender uses `ô = RTprop/2 + q_f`. The propagation split stays symmetric, a declared assumption, and the queue part is measured.
  - In the offline simulation it is exact. In the engine it needs ADR-0078's planned receiver timestamp field, so it is not part of this merge.
  - Arms: the new model with `owd_est = "rel"`, in RQ, OV and P2-OV.
  - Tests: equal to `fwd` with no reverse queue, and correct under a reverse-only queue.
- **Smoothness through every named point** (3e and 3f). The method is the one that worked near 0.4 s (§19): paired seeds, fine steps. The registered K and P statistics (`kp_stats`) are slid over every five consecutive points. A "shown" K or P anywhere is a step or a kink.
  - **T:** [0.36, 0.44] s in 5 ms steps (call, tunnel); [1.8, 2.2] s in 25 ms steps (video). T = ∞ is one-sided: T = 5, 10, 20 and 40 s converge to the identity (§2.10).
  - **H:** [0, 10] ms in 1 ms steps, and [0.0196, 0.0204] s around 0.02.
  - **δ:** ±10 % in 2 % steps around 0.005, 0.5 and 50. These run with repair on, where δ moves r and A*, for the new model and for today's Realtime alike, so a step in today's law is found too.
  - The A/A was 21 % undetermined in stage 3d, so every fine sweep reports its own A/A spread.
  - The cells are OV, CL-long and K-L10 for T and H, and RT and CL-long for δ.

## 7. Merge in two parts

### 7.1 Part (a): now, on the owner's yes

- `three-knobs`: the derivation, marked **Proposed** (not Accepted). This plan goes with it.
- The offline simulation branches: `toy-stage3-knobs` (stages 3 to 3d); `toy-stage3d-rttfix` once it is pushed with its re-baseline; `reg-stage3e`.
- The proposal page: `viz-three-knobs`, the new knobs page from the same wasm, and its plan branch `viz-three-knobs-plan`.
- `viz-path-fix`: it changes only `WASM_B64` in main's page (the builder's home path left the embedded wasm). Its own check read 66 cells and 64 getters with 0 mismatches.
- **Main's page behaviour and its six goldens stay unchanged.** Its bytes change by that one blob.
- No engine file changes, so the binary identity workflow is not touched.
- The roadmap moves in the same push: `model-three-knobs` and `model-viz-three-knobs` get their merge state. `tools/roadmap/build.py` and `check.py` run first.

### 7.2 Part (b): the engine

The steps are in this order, each gated on the one before:
1. **Owner acceptance** of the §1 table's results.
2. **An ADR**, "three application knobs":
   - it supersedes ADR-0084 (decisions 3, 5 and 6; Am. 1 item 1; Am. 2 decision 1; Am. 3's exception and fallback);
   - it amends ADR-0064 (ρ leaves the triangle as a dial and returns as the reported share);
   - it amends CLAUDE.md's "ρ is the retention contract — an INDEPENDENT dial". That text is the owner's to change.
3. **The paper section before any code**, per FORMULA-FIRST: L1, L2 with ô, the protected floor with the trigger delay, the dequeue rule, L6, L7, the repair edge, and T's place in §4.10. Each symbol gets its provenance and each clamp a bind gauge.
4. **Three guarded steps.** Each is inert at T = ∞, proven by a trace identity on `sim_lost_tail` and `sim_shed_reach`, by the decision pins and by binid.
   1. **The wire floor.** The floor's meaning becomes "below this the sender will not resend", carried in the data header (Q15 (a)); this needs a wire version. No floor is sent at T = ∞.
   2. **The receiver release, L6.** It replaces C9–C11, `shed_recv_hold` and `recv_shed_on`. At H = ∞ and T = ∞ it is today's reliable buffer.
   3. **The sender lifetime.** It covers the intake stamp and the dequeue rule, L2 at every resend site, the protected floor, and the deletion of `shed_allowed`, `shed_deadline_us` and the budget refresh. Everything is the identity at T = ∞.
5. **The engine guard** (CLAUDE.md, "No engine merge on a Bulk-only guard"). Each step rides a §41c guard with off-Bulk stages and execution witnesses. It needs these rows:
   - **Stage A:** Auto at c2 and c8, with per-row witnesses that the repair path ran;
   - **Stage R:** a paced evicting seat at Realtime;
   - **R-call:** a Realtime datagram stream (a tunnel at T = 0.4 s) through a 1.5 s outage, the engine analogue of the K cells (§13.3: "no existing Realtime cell exercises a finite T");
   - **R-CL:** the same stream on a capacity-limited link through an outage, the engine analogue of CL-long;
   - **R-OV:** the same stream above link capacity.

   **What ran and what is missing.**
   - Evicting-seat rows have run once: `rate-evict-session` (done, status §51 block E, 12 slots, `REPORTED-ONLY`, `DEGENERATE-AS-WIRED`). That was a measurement on a measurement tree with a pacer, not a reusable guard stage.
   - `meas-guard-bulk-blind` is still in state `next`. Its done criterion is "the guard has off-Bulk stages with witnesses as a reusable plan", and Stages A and R are only planned in `s06-delete-plan.md`.
   - No row anywhere exercises a finite T (§13.3).
   - So Stages A and R as reusable guard stages, and R-call, R-CL and R-OV, must be built and baselined on `main` before step 4.1.
6. **VM rows** decide whether users are better off (§8). The offline simulation only clears the design for the build.

## 8. Rules

- **A failed row gets a derived fix and a rerun of that row only.** The rerun runs on the new code with every gate of §1.1 and the identity of the unchanged arms re-proven. It never gets a tuned constant. A new constant needs a provenance line (measured, cited, a declared dial, or a resource bound), or it does not ship.
- **A miss is a finding to explain, never a reason to move a bar.** A law that reads pinned or degenerate over its range is a defect finding with a ledger verdict (MEASUREMENT DISCIPLINE 17 and 18).
- **Every mechanism a row relies on has an execution witness:**
  - the floor passed something;
  - the dequeue rule fired;
  - today's Realtime shed something;
  - the evict window bound.

  An inert reading without its precondition witnessed is not a pass (MEASUREMENT DISCIPLINE rule 1).
- **No mode switch.** No arm, law or reading compares T, H, δ or ρ with a literal to select a path (CLAUDE.md). The deny test extends to every new field. Today's Realtime's own branches on `reliable` are modelled as shipped, are named in the deny test's exemption list as recorded debt, and are never copied.
- **The offline simulation clears the design for an engine build.** Only the VM rows of §7.2 show that it is better for users.

## 9. Cost and order (one batch at a time; at most two workers, long cells with one)

Stage 3d's first batch died of a `MemoryError` with 15 workers, in a long-cell run of ADR-0084's sender, and those runs grow faster than linearly. The machine therefore runs one batch at a time, the identity checks alone, and nothing while the re-baseline runs.

| order | batch | configurations | runs (12 seeds) | estimated machine time |
|---|---|---|---|---|
| 0 | re-baseline on `toy-stage3d-rttfix` (running) | — | — | — |
| 1 | 3e steps 0–7 as registered (tests, identity, R0, RTT gate, RATE, SIMREAD, A/A) plus A4's tests | — | as registered | under 1 h |
| 2 | **DECIDE: CL-long's sweeps and baselines, repair off and `RL`** (new: 15 T points at H = 0 and 3 at H = ∞, × 2 repair; old: 18) | 54 | 648 | about 10 h (18 old configurations at about 2 min per run) |
| 3 | diagnostic cross (only if 2 fails) | 5 | 60 | about 2 h |
| 4 | 3e MAIN as registered | 59 | 708 | about 3 h |
| 5 | the amendment rows, one path (A1–A3) | about 330 | about 3 960 | 40–60 h; the old senders in OV-long and OV-long-v are about two thirds of it |
| 6 | robustness (§6: biases, RQ, relative one-way delay) | about 38 | about 456 | about 6 h |
| 7 | smoothness (§6) | about 137 | about 1 644 | about 8 h |
| 8 | 3f, two paths (registered after 3e reads; video and file rows included) | about 170 | about 2 040 | about 7 h |
| | **total** | **about 800** | **about 9 600** | **about 3 to 4 days** |

- The per-run times come from stage 3c and 3d: ADR-0084's sender took 37 s per 100 000 units in 3c, and the S5 family about 3 s per full-rate short run (3e §4).
- The worker times the first run of each cell, as 3e already requires. Any cell more than twice this estimate is reported before the batch goes on.

## 10. Owner questions (each with a default)

- **OQ1, sign §1:** the cells, readouts, sweeps, matching rule and pass rule. **Default: as written.**
- **OQ2, today's Realtime modelled with its known defects** (the evict window, no source pause, silent shed) as the baseline a merge replaces, beside ADR-0084's curve as the best-tuned old law. **Default: yes, both.**
- **OQ3, the engine-like sweep as the only form in link cells.** **Default: yes.** The whole-tail form loads the old arms with duplicates.
- **OQ4, seeds:** 12 paired seeds everywhere, about 9 600 runs. The alternative is 6 seeds for the non-named ρ and δ points in the 40 s overload cells, saving about one day. **Default: 12.**
- **OQ5, deadlines:** τ = 0.4 s for calls and Realtime tunnel datagrams, 2 s for buffered video, and completion time for files, with `share_T` reported beside them. **Default: as written.**
- **OQ6, "beyond seed noise"** means `summarize2.read` reads "shown" against `m = max(T_bar, NF)`, with NF from the stage's frozen A/A. **Default: yes.**
- **OQ7, Revision 4** is drafted only if DECIDE fails at matched share, and then as §3.5's slot-cost law. **Default: yes.**
- **OQ8, the cold-start rule** (`d = 0`, `ô = 0` until a path's first sample) and the trigger-gate fix of §5.4. Both are registered for 3f. **Default: yes.**
- **OQ9, placement:** every two-path row runs both striping and duplicate placement for both designs. **Default: yes.**
- **OQ10, merge part (a) now** (§7.1). **Default: yes.**
