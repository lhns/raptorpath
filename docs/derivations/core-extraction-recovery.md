# Core extraction for the recovery model check (design, no code)

Base: `github/main` at `0d885bbd`. Every file:line below refers to that
commit and is relative to the repository root. Design only: no engine or
model change, no cargo build, no VM.

Context:
- Plan decision 7 (`docs/derivations/test-detection-plan.md` §7, "Answered
  2026-11-20"): the extraction grows to the gap clamp, the tail-sweep target
  and the intake pause. They become core functions that take the window
  bounds and the store state as numbers, and `reliable` is replaced by a
  clamp bound. Whether the bound form keeps behaviour identical gets its own
  pin.
- The goal is plan step S9: the stateright model (`raptorpath-model`,
  ADR-0083, github branch `recovery-model`) calls these real decisions
  instead of hand-written copies.
- The rules: CLAUDE.md scope rules (class A only: no hint type, no δ/ρ- or
  path-count-selected branch, no env read); ADR-0077 (zero-behaviour-change
  engine moves; decision pins, host ns bench, §41c guard waived only for an
  identical release sha); `shared-core-design.md` (D4 and D9 rows, owner
  question 1).
- The constraint: the fix direction for `rec-evict-blocker-unresolved`
  (`docs/roadmap/nodes.yaml`) is the owner's and is open. The candidates are
  a source-rate gate, a shed below the window, and a different sweep target.
  This extraction makes today's behaviour checkable. It does not fix it, and
  it does not pick a fix direction.

## 0. Key decisions

1. **The clamp's bool leaves the decision but not the engine, in the
   zero-change step.** The core gets `clamp_gap(s, e, f, c)`, which has no
   seat input. The engine maps the seat to the bounds: (0, `u64::MAX`) at
   ρ = 1 (a bound that never binds) and the window at ρ < 1. This is
   identical by construction. It has the shape the engine already uses at
   `net/sender_phases.rs:1400`, and the shape recovery-counters'
   `clamp_lo` has (§4.3).
2. **The real store floor is a later, pinned step and not a refactor.**
   - "Clamp to the seqs the sender still holds" uses the hull of the coding
     window and `sent_store`. It removes `reliable` from the call site
     outright, because `sent_store` is empty at ρ < 1
     (`net/emit_source.rs:347`).
   - It is not identical by construction. At ρ = 1 a reported gap below the
     store (an already-acked seq) passes today's clamp.
     - It is counted into `nacked_count` (`net/sender_phases.rs:803`), and
       that count feeds `repair_debt` (`:1171`).
     - It is `[HOLD]`-stamped (`:828`) and counted as `mpd_stale` (`:992`)
       before the store miss skips it.
   - So the store-span bound ships only after a bind gauge reads zero binds
     at ρ = 1 with calls > 0 (§3). Otherwise it is a behaviour change for
     the owner (§6 Q2).
3. **The intake pause gets no seat-free form in this work.**
   - The pause has two disjuncts. The store disjunct can be true at ρ < 1
     under `RWM_STORE=0` or in generation mode (`store_len =
     encoder.window_size()`, `net/mod.rs:2303`). The `cwnd_full` disjunct
     can be true at ρ < 1 under `RWM_LATE_BRAKE`, `RWM_INFL_CAP` or
     `RWM_INFL_BDP`/gen_pipe. The brake is "not scoped to `plain_dyn_cap`",
     `net/sender_policy.rs:607–610`.
   - Dropping `reliable &&` would therefore start pausing intake at ρ < 1:
     a source-rate gate on the evicting seat. That is one of the D1b fix
     directions, so it is out of scope.
   - The core gets `intake_paused(O, B, F)`. The engine maps ρ < 1 to the
     inputs (0, ∞, false), which is identical by construction.
   - On the shipped arm the map is redundant by data. A pin proves that, and
     the proof is what lets the model call the core function with no seat
     map.
4. **The tail-sweep target is moved as it is: the identity on `first()`.**
   - It takes no window input and no servability filter.
   - D1c (a below-window target that the clamp empties) then arises in the
     model from composing two real core functions: target, then clamp. That
     is where it arises in the engine.
5. **The clamp → shed order stays at the call site**, in `serve_gaps` and
   in the model's hand-written step. It is the D1a defect, and its fix is the
   owner's. No core function composes clamp, shed and serve.
6. **No core function here takes δ, ρ, N or a hint**, so the ±2 % preset
   continuity test and the N = 1 / N = 2 absolute tests do not apply. The
   seat-to-number map stays in the engine. In the core it would be a step at
   ρ = 1 and would break class A.

## 1. The three decisions

New core module: `raptorpath-core/src/hole.rs`, declared as `pub mod hole;`
in `raptorpath-core/src/lib.rs`. The name follows the model's "hole
lifecycle". It is kept apart from `core::recovery`, which shared-core step
2.1 fills with D6–D8, so the two can land in either order. Seqs are `u64`
on both targets (`usize` is 32-bit on wasm32).

### 1.1 The gap clamp (D1a)

**Today** (`net/sender_phases.rs`, inside `serve_gaps` `:614`):

| what | where | value |
|---|---|---|
| input: the gap | `:790` loop over `judged_gaps(..)` (`:782`) | `(gap_start, gap_end)`, inclusive |
| input: the window | `:759` | `(win_start, win_end) = st.encoder.window_span()`; `(0, 0)` when the window is empty (`fec/rlc_window.rs:192–197`) |
| input: the seat | `:587`, `:617` | `reliable: bool` (= `pol.reliable`; `run_window_sender`'s parameter `net/mod.rs:1533`) |
| decision | `:795–799` | `if reliable { (gap_start, gap_end) } else { (gap_start.max(win_start), gap_end.min(win_end)) }` |
| output | `:800–803` | empty when start > end: the test-seams hook `on_gap_clamped` fires, then `continue`; else `nacked_count += e − s + 1` and the per-seq loop `:805` |

Downstream of the output, and unchanged by this work, come the shed check
(`:847–869`), the cooldown (`:872`), the channel verdicts (`:890–961`) and
the serve. The serve (`:985–996`) reads `sent_store` if `reliable`, and the
encoder window otherwise. That `if reliable`, the path choice at `:973` and
`:1118` stay D9's B sites (`shared-core-design.md` §1).

**Core signature:**

```rust
/// A reported gap intersected with the servable span [floor, ceil].
/// Empty when the result's start > end (the caller's existing test).
pub fn clamp_gap(gap_start: u64, gap_end: u64, floor: u64, ceil: u64) -> (u64, u64)

/// The hull of what the sender can serve: the coding window and the
/// retention store's key span (None when the store is empty).
pub fn servable_span(window: (u64, u64), store: Option<(u64, u64)>) -> (u64, u64)
```

**FORMULA.**

"A gap is served only over the seqs the sender can still serve: its
intersection with the servable span."

```text
   [s′, e′]  =  [ max(s, f) ,  min(e, c) ]          empty iff s′ > e′
```

"The sender can serve a seq from the coding window or from the retention
store." (This is the step-4 form. Step 2 passes the seat map instead.)

```text
   (f, c)  =  ( min(w_s, r_lo) ,  max(w_e, r_hi) )   store non-empty
   (f, c)  =  ( w_s , w_e )                          store empty
```

Shape check: no N, no units other than seq numbers. s′ rises with f and e′
falls with c, so the span shrinks monotonically as either bound tightens.

The hull is also the union:
- At ρ = 1 the store holds exactly (ack, hi].
  - Insertion is in seq order (`net/emit_source.rs:347`), and pruning is by
    prefix only (`net/sender_phases.rs:1411`).
  - `SeqRing` keeps its front and back slots `Some` (`net/seq_ring.rs:24–26`,
    `trim` `:125`), so the first and last keys are exact.
- The window ends at the newest sent seq hi, the same end the store has
  whenever the store is non-empty. Both intervals end at hi, so their union
  is an interval and equals the hull.
- At ρ < 1 the store is empty, and the span is the window.

**`reliable` → numbers:**

| step | (f, c) at ρ = 1 | (f, c) at ρ < 1 | where the seat enters |
|---|---|---|---|
| step 2 (zero change) | (0, `u64::MAX`): never binds, so (s′, e′) = (s, e) for every gap | (w_s, w_e) | an engine seat map next to `:759`, the `:1400` shape |
| step 4 (pinned) | `servable_span(window, sent_store span)` | the same expression; the store is empty, so (w_s, w_e) | only through whether the store retains (`net/emit_source.rs:347`) |

At ρ < 1, step 4 is today's window clamp exactly, by construction, so the
shed's reach on the evicting seat does not move (see the ordering
constraint above). The empty-window sentinel (0, 0) is kept as it is in both
steps: (0, 0) is a real span [0, 0] today, so a gap containing seq 0 still
passes at ρ < 1 when the window is empty. That is an edge for the Kani
harness (ADR-0083 decision 4), not a change.

**Call sites that change:**
- `net/sender_phases.rs:795–799`: the decision.
  - Step 2: `clamp_gap(gap_start, gap_end, f, c)` with the seat map.
  - Step 4: f and c from `servable_span`.
- `net/sender_phases.rs:759`: the bounds are computed once per batch next to
  the window read.
- `raptorpath/Cargo.toml`: `raptorpath-core = { path = "../raptorpath-core" }`,
  the engine's first edge to the core unless shared-core 2.x lands first.
- recovery-counters' `net/account.rs` `clamp_lo(reliable, win_start)`
  (uncommitted, §4.3) reads the same seat map. One function serves both,
  so the account's `O_strand` and the clamp cannot drift apart.

Unchanged: the hook `on_gap_clamped` (`:800`) and the `nacked_count` line
(`:803`) stay at the call site. The S4 counters' hooks stay at the call site
too. The core returns a span; the engine counts.

### 1.2 The tail-sweep target (D1c)

**Today** (`net/mod.rs`):

| what | where | value |
|---|---|---|
| input | `:2789` (arming, every loop iteration) and `:3035` (firing) | `st.retransmit_buffer.first()`: the lowest un-acked, un-shed seq (pruned at ack `net/sender_phases.rs:1403`, at shed `:860`, `net/emit_source.rs:782`) |
| decision | `:3035–3042` | the sweep's gap report is `(FireCause::Timer, u32::MAX, vec![(seq, seq)], None)` for that seq |
| arming | `:2788–2813` | armed iff the buffer is non-empty; deadline = max(send, last retransmit, last sweep) + `tail_sweep_timeout_us(srtt)` |
| output | `pending_gaps`, then `serve_gaps`, so it goes through the clamp (§1.1) | |

No `reliable` is read. The window is not read. That is D1c: at ρ < 1 the
target can lie below `win_start`, and the clamp then empties its gap.

**Core signature:**

```rust
/// The sweep's synthesized gap: the one-seq gap at the cumulative blocker,
/// the lowest seq still awaiting a verdict. None when none is.
pub fn tail_sweep_gap(lowest_unresolved: Option<u64>) -> Option<(u64, u64)>
```

**FORMULA.**

"On expiry the sender synthesizes a one-seq gap report for the cumulative
blocker."

```text
   g_sweep  =  (b, b),   b = min R          none when R = ∅
   R        =  keys of retransmit_buffer  (un-acked and un-shed; the window is not an input)
```

**`reliable` → numbers:** none needed; the decision has no seat input today.

**Call sites that change:**
- `net/mod.rs:3035–3042`: `tail_sweep_gap(st.retransmit_buffer.first().map(|(s, _)| s))`
  builds the batch's gap.
- `net/mod.rs:2789`: arming reads the same function (`.map(|(b, _)| b)`) and
  then the entry's send time. Both reads keep their own call, as today, so
  a later re-target changes arming and firing together. `:2789` runs on
  every loop iteration, so it is on the ns bench (§2).

The deadline arithmetic at `:2790–2813` stays inline. Its timeout law moves
with shared-core 2.1 (D8). The model abstracts timing (§5), so S9 does not
need it.

### 1.3 The intake pause (D1b)

**Today** (`net/mod.rs`):

| what | where | value |
|---|---|---|
| input O | `:2303–2310` | generation: `encoder.window_size()`; else `sack_release_outstanding(sent_store.len(), released)` (`net/sack.rs:461`, saturating) |
| input B | `:2464–2472` | `scs.dyn_store_cap` if `plain_dyn_cap` (requires `reliable`, `net/sender_policy.rs:521`), else `gen_pipe_store_cap` under gen_pipe, else `pol.store_max` |
| input F | `:2430–2435` | `cwnd_full = brake_armed && (percap_full or pipe_infl ≥ eff_infl_cap)`; `brake_armed = eff_infl_cap > 0 \|\| late_brake` (`:2400`) |
| decision | `:2473` | `tx_paused = reliable && (store_len >= effective_store_cap \|\| cwnd_full)` |
| second copy, the store disjunct only | `:3219–3229` | the `RWM_EMIT_BATCH` burst: `reliable && outstanding >= effective_store_cap`, per symbol |
| output | `:2859`, `:2864`, `:2900`, `:2907` | select arms gated on `!tx_paused`, plus the `[DIAG]` echo `:2509`, `:2641`, `:2703` |

**Core signature:**

```rust
/// Source intake pauses while the outstanding count reaches its bound or
/// the in-flight brake is full.
pub fn intake_paused(outstanding: u64, bound: u64, in_flight_full: bool) -> bool
```

**FORMULA.**

"The sender stops reading source while the store's outstanding count
reaches its cap, or every live path is at its in-flight cap."

```text
   P  =  (O ≥ B)  ∨  F
```

This is paper §6.1's "the sender stops reading source when
`store_len ≥ effective_store_cap`", plus the brake disjunct that the same
section says is inactive by default.

**`reliable` → numbers:**

| seat | (O, B, F) passed | P |
|---|---|---|
| ρ = 1 | (`store_len`, `effective_store_cap`, `cwnd_full`) | today's expression |
| ρ < 1 | (0, `u64::MAX`, false) | false, today's value (0 ≥ `u64::MAX` is false) |

- The engine keeps this map at `:2473`. It is the one remaining seat entry
  in the three decisions, and it is the D1b debt in plain view: the evicting
  seat has no intake bound.
- A seat-free form, the same (O, B, F) on both seats, is not proposed. It
  differs from today at ρ < 1 on every C arm:
  - generation: O = window size, which can reach B;
  - `RWM_STORE=0`: 0 ≥ 0;
  - `RWM_LATE_BRAKE`, `RWM_INFL_CAP`, gen_pipe's `RWM_INFL_BDP`: F can be
    true.
- Each of those would add an intake bound on the evicting seat, which is a
  D1b fix direction. It waits for the owner.
- **On the shipped arm the map is redundant by data**:
  - O = 0, because `sent_store` is never filled at ρ < 1
    (`net/emit_source.rs:347`) and `sack_released` is empty (SACK release
    requires `reliable`, `net/sender_policy.rs:628`);
  - F = false, because the brake is unarmed: `RWM_INFL_CAP` defaults to 0
    (`gates.rs:359`), `RWM_LATE_BRAKE` is off (`gates.rs:370`), and
    `infl_bdp` is 0 without gen_pipe (`net/sender_policy.rs:756–760`);
  - B ≥ 1: `RELIABLE_STORE_MAX` (`net/sender_policy.rs:509`), or
    coded-only's `≥ win_cap`.
- That redundancy is what lets the model, which has no C arms, call
  `intake_paused(O_retained, B, false)` with no seat map. §3.3 pins it.

**Call sites that change:**
- `net/mod.rs:2473`: `let tx_paused = if reliable { intake_paused(store_len as u64, effective_store_cap as u64, cwnd_full) } else { intake_paused(0, u64::MAX, false) };`.
  The step-2 text may write the map as `reliable && intake_paused(..)` on
  one line. The two are equal by construction, and the one-line form keeps
  the line count.
- `net/mod.rs:3219–3229`: `reliable && intake_paused(outstanding as u64, effective_store_cap as u64, false)`.
  Here `reliable` is redundant by policy, because `emit_batch_on` requires
  it (`net/sender_policy.rs:737`). It is left in place so that no line
  moves. Changing this site keeps the decision from forking: a D1b fix then
  changes one function, not two expressions.

### 1.4 Class A and the core's own gates

- None of the four functions takes δ, ρ, N, a hint or an env value, and none
  branches on one. Their only branches are `max`/`min` and an `Option` map.
- `raptorpath-core/tests/deny_list.rs` covers the new file as it stands. No
  forbidden token is needed.
- The functions are panic-free, so they carry no location data. `u64` adds
  are avoided, so there is no overflow-check difference between profiles.
- Tests in `hole.rs` (step 1):
  - **absolute values** at fixed points, the CLAUDE.md "absolute, not
    ordinal" rule. Examples: `clamp_gap(3, 9, 5, 7) == (5, 7)`;
    `clamp_gap(1, 2, 5, 7)` is empty; `servable_span((10, 12), Some((4, 12))) == (4, 12)`;
    `servable_span((0, 0), None) == (0, 0)`; `intake_paused(0, u64::MAX, false) == false`;
    `tail_sweep_gap(None) == None`.
  - the exhaustive differential of §3.1.
- A Kani harness under `#[cfg(kani)]` (ADR-0083 decision 4, CI only) checks
  that s′ ≥ f and e′ ≤ c whenever the result is non-empty, and that nothing
  panics for any `u64`.

## 2. Proof that each step is a zero-behaviour-change move

**Byte identity is narrower than "a pure move".** Status §58 (Part B
result) showed that any line shifted in an engine source file changes the
release sha, because the binary embeds location data. Even the two-line
`#[cfg(test)] mod decision_trace_pin;` declaration did. Only edits that
shift no existing engine line can meet ADR-0077's literal waiver (an
identical release sha). §58 leaves open a weaker identity, "the same code
section, with differences only in location data", as the owner's call.

| step | engine lines touched | release sha | proof | VM guard |
|---|---|---|---|---|
| 0 (test) | none: appended to `net/decision_trace_pin.rs`, a `cfg(test)` module whose declaration does not move | identical (expected; checked by the binid hash) | the new `hole` digest captured on the untouched tree | no (waiver) |
| 1 (core) | none: `raptorpath-core` only, which the engine does not yet depend on (§58's Y: the core does not touch the engine binary) | identical | the core's unit tests, deny-list, wasm32 check | no (waiver) |
| 2 (move) | `sender_phases.rs:795–799`, `mod.rs:2473`, `:2789`, `:3035–3042`, `:3219–3229`, `raptorpath/Cargo.toml` | **expected to differ.** The rewrite keeps each site's line count where it can, but the new dependency edge, `:2789`'s rewrite and any codegen difference across the crate boundary are each unverified | `hole` digest equal to step 0's; existing decision pins; the integration witnesses below; ns bench | **yes**, unless the sha is identical |
| 3 (gauge) | test-seams lines in `serve_gaps` (shift lines; no release code) | differs (location data only) | the gauge itself; all pins of step 2 unchanged | rides step 2's guard |
| 4 (bound) | `sender_phases.rs:759`, `:795–799` | differs (new loads) | §3.2 bind gauge green, then all of step 2's pins | **yes** |
| 5 (model) | none: `raptorpath-model` only | identical | the model's red and green verdicts unchanged (§5) | no |

**Step 2 in detail.**
- **Decision pins.**
  - `decision_trace_pin`'s new `hole` digest, computed through the core
    functions with the same scripted inputs, must equal the value step 0
    captured from verbatim `cfg(test)` copies of the old inline
    expressions. That is the §58 Part B "bit-identical against `cfg(test)`
    copies" method.
  - The script covers both seat maps, gaps wholly below, straddling,
    inside and above the window, the (0, 0) empty window, an empty and a
    non-empty buffer for the sweep, and (O, B, F) at B = 0, B = O, B = O + 1
    and `u64::MAX`.
  - The existing digests (span, recovery, shed, taper, store, snapver) do
    not move.
- **Executes witnesses.**
  - `tests/sim_lost_tail.rs`: `known_defect_lost_tail_strands_k_at_rho_below_1`
    must stay red-shaped (k stranded: gap-clamped events for the blocker,
    no shed of it) and `control_lost_tail_recovers_at_rho_1` green.
  - `tests/sim_shed_reach.rs`: `witness_nack_site_sheds_at_rho_below_1`
    green.
  - `tests/s06_reactive.rs`'s non-ignored groups.
  - These drive the moved code through the hooks that stay at the call
    sites (`on_gap_clamped`, `on_shed`), so the moved code runs: MEASUREMENT
    DISCIPLINE rule 1.
- **Host ns bench.**
  - `hole_decisions_ns`, an `#[ignore]` release micro-bench in the
    `place_costs_ns` pattern (`raptorpath/src/scheduler/place_alloc_tests.rs:75–93`):
    the median of 2000 batches of 100 calls, for the core calls against the
    `cfg(test)` verbatim copies.
  - The hot sites are `:2789` (every loop iteration), `:2473` (every
    iteration), `:3219` (every burst symbol) and the clamp (every judged
    gap).
  - Accept within the same tolerance `place_costs_ns` is read at.
- **§41c guard.** One run, expected WITHIN. It covers steps 2 and 3, and
  step 4 when its gauge is green before the batch runs (one guard per batch
  whose pins all held, `shared-core-design.md` §4 ladder 4). Whether this
  batch rides guard α is §6 Q3.

**Step 4** is not a refactor until §3.2 reads green. With zero binds at
ρ = 1 and calls > 0, the store-span bound gives the same span as the seat
map on every gap the suites drive. Wire behaviour, `nacked_count`, `[HOLD]`
and `mpd_stale` are then unchanged on those runs, and the step rides the
guard with the full ladder. With binds > 0 it changes `repair_debt`, so it
is a behaviour change and stops (§6 Q2).

## 3. The pins for "the bound form behaves identically to the bool"

### 3.1 Function level, exhaustive (step 1, `raptorpath-core/src/hole.rs`)

```text
test bound_clamp_equals_bool_clamp_under_the_retention_invariants
  for hi in 0..=12, ack in -1..hi, W in 1..=4, every gap 0 ≤ s ≤ e ≤ 13:
    window  = [max(0, hi+1−W), hi]            (and the (0,0) sentinel when nothing is sent)
    ρ < 1:  store = None
            assert clamp_gap(s, e, servable_span(window, None))
                   == (max(s, w_s), min(e, w_e))                      -- exact, no precondition
    ρ = 1:  store = (ack+1, hi) if ack < hi else None
            let old = (s, e)                                          -- today's ρ = 1 arm
            let new = clamp_gap(s, e, servable_span(window, store))
            if s ≥ min(w_s, ack+1) and e ≤ hi:  assert new == old
            else: every seq in old \ new is ≤ ack or > hi             -- not in sent_store:
                                                                      -- sent_store.get misses (:987)
    assert both classes were reached (counts > 0)                     -- not vacuous
```

This bounds the divergence: the only seqs the store-span form drops at
ρ = 1 are seqs that `sent_store.get` would miss. The wire sends are
therefore the same. The function-level test cannot show whether such gaps
occur in a run, so it cannot settle `nacked_count`. That is §3.2.

### 3.2 Engine level: the clamp's bind gauge (step 3, before step 4)

CLAUDE.md requires a bind-fraction gauge for every clamp, so the pin is
stated as one.

```text
gauge (cfg(any(test, feature = "test-seams")), fed in serve_gaps per judged gap):
  calls        every judged gap at ρ = 1
  binds        gaps where the RESULT differs: clamp_gap(s, e, servable_span(..)) != (s, e),
               today's ρ = 1 span (the bounds themselves differ on almost every gap;
               what counts is whether they cut the gap)
  binds_live   binds that drop a seq present in sent_store or the window
               (must never happen; §3.1 says it cannot under the invariants)

test bound_clamp_never_binds_at_rho_1   (integration, test-seams)
  run: sim_lost_tail's ρ = 1 control, sim_shed_reach's ρ = 1 arm,
       s06_reactive's reliable groups
  assert calls > 0                                   -- the executes witness
  assert binds == 0 and binds_live == 0
  report binds/calls per run                         -- the bind fraction

red control: an injected gap report for a seq q < min(w_s, ack + 1), i.e. below
  BOTH the window start and the store (a test-seams hook appends (q, q) to one
  judged batch, only once the window has moved past the ack; an injected
  (ack, ack) while the window still holds ack would not bind and would read
  green); the gauge must read binds = 1, binds_live = 0, and the run must
  still complete identically on the wire
```

- **At ρ < 1** a second counter compares the store-span result with
  today's window clamp on every judged gap: differences == 0 and calls > 0.
  That is the step-4 claim on the evicting seat.
- **Generation at ρ = 1** keeps no `sent_store` (`net/emit_source.rs:347`),
  so there the store-span bound is the window. If any suite drives
  `serve_gaps` in generation mode at ρ = 1, it is a gauge cell. If none
  does, step 4 states that the cell is unmeasured, and §6 Q2 covers it.

### 3.3 The intake map is redundant on the shipped arm (step 3)

```text
test intake_seat_map_is_redundant_on_the_shipped_arm   (integration, test-seams)
  at ρ < 1, with the shipped gates (sim_lost_tail's and sim_shed_reach's ρ < 1 arms):
    on every loop iteration, observe the inputs the ρ = 1 map would have passed:
      assert store_len == 0 and cwnd_full == false and effective_store_cap ≥ 1
    assert iterations > 0
  so intake_paused(store_len, cap, cwnd_full) == intake_paused(0, u64::MAX, false)
  on every iteration: the model's map-free call reproduces the engine's
red control: the same run with RWM_INFL_CAP=1 must make the assertion fail.
  The global in-flight brake (net/mod.rs:2430–2435, as read) is then armed,
  and cwnd_full is true as soon as one symbol is in flight. That shows the map
  is load-bearing on the C arms and why it stays. (RWM_LATE_BRAKE=1 is not
  used: it closes only when every live path reaches its cwnd, which the
  scenario need not do.)
```

## 4. Steps, one commit each, and the open branches

### 4.1 Order

| # | kind | commit | waits on |
|---|---|---|---|
| 0 | test | `decision_trace_pin`: the `hole` digest over verbatim `cfg(test)` copies of the three inline expressions (both seat maps), appended at the end of the file; captured on the untouched tree | — (can be written now) |
| 1 | core | `raptorpath-core/src/hole.rs`: `clamp_gap`, `servable_span`, `tail_sweep_gap`, `intake_paused`; absolute tests, §3.1, the Kani harness | — (can be written now) |
| 2 | E | the engine depends on the core and calls the four functions (§1.1–§1.3 call sites), with the seat maps in the engine. The digest is now computed through the core and must equal step 0's | 0, 1; the three open branches merged (§4.3) |
| 3 | test-seams | the clamp's bind gauge (§3.2), the intake redundancy pin (§3.3), the injected-gap red control | 2 |
| — | VM | the guard for 2–3 (and 4 if its gauge is already green) | 3 |
| P | paper | §7.1 states the servable span, §11 row 13 (`docs/fec-arq-model.md:4311`) drops the gap clamp from the reliable-bool list and keeps `tx_paused` there, as FORMULA-FIRST requires before step 4 | 3 green |
| 4 | E | the clamp's seat map is replaced by `servable_span(window, sent_store span)`; recovery-counters' `clamp_lo` follows it | P; §3.2 green; §6 Q2 if not |
| 5 | M | `raptorpath-model` calls the core (§5); its existing red and green verdicts are unchanged | 2 (clamp via seat map) or 4 (store span); `recovery-model` rebased on main |

Steps 0 and 1 touch no engine file and can land first, at any time.

Steps 2–4 do not depend on shared-core 2.1, 2.5 or 3.1, or on owner
question 1 (the path-count laws). None of these functions takes N.

S9's later scope does depend on them: the cooldown, the channel verdicts and
`shed_allowed`, which moves with shared-core 2.2. Until those move, the
model keeps its cited hand copies (§5).

### 4.2 Relation to the shared-core batches

- Batch α (shared-core 1.2–2.5, plus phase6/endspeed per §58) is not yet
  run. Steps 2–3 are the same kind of change: pure moves, guard expected
  WITHIN.
- If their pins are green when α is assembled, they can ride α. Otherwise
  they get their own batch (§6 Q3).
- They must not interleave with ADR-0076 phase 5, the `net/` split, which
  touches the same files (`shared-core-design.md` §4).

### 4.3 The open engine branches on the same lines

| branch | state | overlap |
|---|---|---|
| `s06-delete` (github `a930a72b`) | open | `retransmit_buffer`'s tuple goes from 3 to 2 fields, which hits `net/mod.rs:2789`'s closure pattern and `net/sender_phases.rs:825`, `:853`, `:963`. It reformats `:800` (the hook onto its own lines) and deletes the slot-site copy branch (`net/emit_source.rs:740–826`), the one place that already serves seat-free (`sent_store.get(..).or_else(encoder.get_source)`, `:822–826`). It conflicts with step 2 at `:2789` and `:800` |
| `c8-gauges` (github `697eeb73`) | open | the same reformat of `:800`, and `st.shed_nack += 1` at `:863`. That is the shed path, below the clamp, and not touched here |
| `recovery-counters` (local, uncommitted in its worktree at `0d885bbd`, as read) | in progress | new `net/account.rs` with `clamp_lo(reliable, win_start) = if reliable { 0 } else { win_start }`, the step-2 seat map's floor. Its `O_strand` is defined against that bound. Edits in `net/sender_phases.rs` and the sim tests |

Recommendation: extract after all three merge, at the quiet point decision 7
names.
- Step 2 then rebases once onto the final layout of `:800` and `:2789`.
- recovery-counters' `clamp_lo` becomes the one seat map that step 2 calls.
  It is lifted out of the `cfg(any(debug_assertions, feature = "test-seams"))`
  module into always-compiled code next to the clamp.
- Step 4 changes that one function, so the account's `O_strand` follows the
  clamp's real bound. If the counters land after step 2, they call the step-2
  seat map instead of defining their own.

## 5. How the stateright model calls them

`raptorpath-model` gains `raptorpath-core = { path = "../raptorpath-core" }`.
This is still inside ADR-0083's boundary: the model may depend on the core;
the core depends only on math. The model's seqs are `u8`, widened to `u64`
at each call. The model already holds every input as state, so the
ADR-0077 stop rule is met.

| model step (`raptorpath-model/src/lib.rs` on `recovery-model`) | after this work |
|---|---|
| `judge_gap`'s clamp (`if self.seat.reliable { .. } else { max/min }`) | `hole::clamp_gap(s, e, f, c)`. (f, c) comes from the model's copy of the engine seat map after step 2, and from `hole::servable_span(window_span, retained_span)` after step 4. The model gains `retained`, the `sent_store` keys, in step 5 whichever bound the clamp uses (the intake call below needs it): filled at emission only where the engine fills it, and pruned at the frontier |
| the intake pause (`self.seat.reliable && outstanding ≥ store_cap`) | `hole::intake_paused(retained_outstanding, store_cap, false)`, with no seat map. The model has no in-flight brake (a C arm), and its retained count is 0 at ρ < 1. §3.3 is the engine-side proof that this equals the engine's mapped call on the shipped arm |
| the tail sweep target (`lowest(outstanding)`) | `hole::tail_sweep_gap(lowest(outstanding))`, whose gap then goes through `judge_gap`, so D1c comes from composing the two real functions |
| the shed check (`shed_allowed`, a cited copy of `net/shed.rs:39`) | `raptorpath_core`'s `shed_allowed` once shared-core 2.2 moves it. The model's `past` flag maps to (age, D) = (D + 1, D) or (0, D) |

**Stays hand-written**, each with the engine-side witness that bounds it:

| hand-written step | why | witness |
|---|---|---|
| the clamp → shed → serve composition inside `judge_gap` | it is the D1a order, whose fix is open; it lives in `serve_gaps`, not in a core function | `sim_lost_tail::known_defect_lost_tail_strands_k_at_rho_below_1`: the engine shows the same composition (clamped, never shed) that the model's red trace shows |
| the retention insert (`sent_store` only at ρ = 1, `net/emit_source.rs:347`) and the shed arming (`shed_armed`, `net/shed.rs:23`) | the seat itself, resolved once in the engine's policy; after step 4 the seat enters the three decisions only through these | the model's `Seat` struct, cited at each site |
| the serve source (`sent_store` against the encoder, `net/sender_phases.rs:985`) | a D9 B site, not moved | the model's `served_below_window` sometimes-witness at ρ = 1 |
| the cooldown and the channel ripeness gates | taken as ripe whenever a chance is taken (the model's declared abstraction); they move with shared-core 3.1 | none yet: they cannot cause a stuck state, only a later chance |
| timing (when a sweep fires, the deadline) | bounded time is the model's liveness device (plan S7) | `tests/s06_reactive.rs` and the sim tests' timings |
| the link, loss, the ack path, the receiver (reorder buffer, hold expiry, hole refresh) | the truth side, or receiver decisions outside this extraction | as in `docs/derivations/recovery-model.md` on `recovery-model` |
| the correction-slot copy | deleted by `s06-delete`; the model drops it with that merge | — |

**Acceptance for step 5** (the plan's red/green rule):
- the lost-tail property stays red at ρ < 1 under the same bounds;
- the ρ = 1 control stays green;
- the `served_below_window` witness is still reached;
- a model-level test asserts that the model's clamp at ρ < 1 is
  `hole::clamp_gap`, by checking the red trace's clamped gap against a
  direct call.

## 6. Owner questions still open

1. **Intake pause.** Keep the engine seat map, (0, ∞, false) at ρ < 1,
   until the D1b fix direction is chosen? A seat-free form is a source-rate
   gate on the evicting seat on every C arm (§1.3).
   *Default: yes. The map is the D1b debt in plain sight, and the model is
   proven map-free on the shipped arm (§3.3).*
2. **If the clamp's bind gauge reads > 0 at ρ = 1** (stale gaps below the
   store occur, or the generation × ρ = 1 cell is unmeasured), should the
   store-span form land as a deliberate behaviour change with its own guard?
   It would stop counting already-acked seqs into `repair_debt`'s reduction,
   `[HOLD]` and `mpd_stale`. Or should the engine keep the seat map?
   *Default: keep the seat map, so the step-2 form stays. Decision 7's "no
   bool" is then met inside the core but not yet at the call site. The
   model mirrors the map hand-written and cites the engine line. The
   question is reopened with the D1a fix, which changes the clamp anyway.*
3. **Guard batching.** Should steps 2–3 (and 4) ride batch α or run as their
   own batch?
   *Default: ride α if their pins are green when α is assembled; same kind,
   one VM run.*
4. **The weaker byte identity** that §58 left open (the same code section,
   differences only in location data). This design assumes the literal rule,
   so step 2 plans for the guard.
   *Default: unchanged; calibrate on §58's base/X pair before any change.*
5. **The paper entry for the servable span** (step P). FORMULA-FIRST asks
   for it before step 4. It is a definition, not a law with constants.
   *Default: one paragraph in §7.1 and the row-13 edit, before step 4.*

6. **ADR-0083's wording.** Its decision 3 says the functions take "the
   window bounds and ρ as numbers". This design passes no ρ into the core:
   the seat enters only as bounds the engine computes (§0 item 6).
   *Default: amend decision 3 to "the window bounds and the store state as
   numbers" (decision 7's own wording) when the ADR is accepted.*

Shared-core owner question 1 (the path-count laws) is not needed for these
steps. It stays open for 3.1 and for S9's channel verdicts.

## 7. Not verified

- That stale gaps below the cumulative ack reach `serve_gaps` at ρ = 1 in
  practice. The serve-path comment at `net/sender_phases.rs:981–983` says
  they do. §3.2 measures it.
- Whether any test drives `serve_gaps` in generation mode at ρ = 1.
- That step 2 changes the release sha. It is expected (the dependency edge,
  `:2789`'s rewrite), not built.
- The uncommitted state of `recovery-counters` was read from its worktree on
  2026-11-20 and may change before it lands.
- The line-count-preserving layout of each step-2 edit is designed, not
  written.
- That the window ends at the newest sent seq at ρ = 1, which the
  hull-equals-union argument (§1.1) rests on. As read: eviction is front-only
  and size-only on both seats (`net/mod.rs:3594–3603`). That every emitted
  source enters the encoder at ρ = 1 was assumed, not traced.
- recovery-counters' edits in `net/sender_phases.rs` were not read; the diff
  printed nothing for that file. Only its new `net/account.rs` was read.
