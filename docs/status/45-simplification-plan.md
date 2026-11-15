## 45. Simplification plan — reduce complexity, keep or improve performance — no VM, plan

The user asked for a plan to simplify the model while keeping or improving
performance. It was made with the advisor from §44's inventory. Principle:
**delete before redesigning, and police every stage with one cheap A/B.**
No engine code changes while §41 and §43 use `diag36`'s tree.
Simplification branches from main after `diag36` is merged.

**The guard (keeps performance honest).**
- §41's BASE comparison, reused: new binary against the previous main,
  same session, default law, at c1s, c1d, c2 and c8, on §21's clauses. It
  takes ≈ 15–20 min per stage.
- It is calibrated by §41's MAIN-against-BASE result. If two binaries that
  behave the same do not read WITHIN, the guard is fixed first (the
  unexplained slow c1s rows), before it polices anything.
- Two Realtime cells are added before Stage 3. The headline claim lives
  there and has not been measured since the threading redesign.
- **Scoreboard**, recorded at main before Stage 1 and after each stage:
  `RWM_` gates, §11.2 rows, estimators per quantity, hint-keyed sites and
  source lines. Bind gauges and witnesses are not counted (FORMULA-FIRST
  requires them).

**Stage 1 — bit-identical.** Suites plus one guard run.
- §44's three items:
  - one smoothed-RTT copy where `est.rtt()` ≡ Copa `srtt`, after
    asserting equality;
  - delete the unread `CorrectionDeficit`;
  - rename the deadline's `rtprop` variable.
- **Unread hot-path work.** Any gauge or estimator computed per sample and
  read by nothing with diagnostics off, for example the RTT-dispersion
  candidates in `diag.rs`. c1 is engine-bound (§42), so this is where
  simplifying can also buy goodput. §41's pattern applies: host ns per call
  before any VM time.
- **The dial coordinate u (§44 item 1).** In the paper, with one continuity
  test. In code only if the wasm golden fingerprints do not move.

**Stage 2 — retire default-off arms.** User's go. Default behaviour is
unchanged by construction.
- **Inventory first.** Classify every gate: shipped, refuted on the current
  substrate, refuted on an old one, never tested, instrument, or test-only,
  each with its status-section verdict.
- **Retirement per ADR-0052.** An arm refuted on the current substrate is
  tagged `archive/<arm>`, then deleted with its tests and register rows. For
  every other arm the user picks re-test or delete.
- **Coupling.** The `[GATES]` line is byte-pinned and the harness parsers
  witness on it, so the old plans' fixtures are frozen first.
- **Standing rule against re-growth.** An arm is a question. At its verdict
  it is either promoted, with the old path deleted in the same merge, or
  archived. That covers `RWM_PLACE_WIRE` after §41, and the test hold hook.

**Stage 3 — behaviour changes, one at a time.** Each is formula-first and
pre-registered against the guard.
1. **Copa's legacy controller.** Grep the readers of its cwnd first. It is
   the largest single removal, and takes the `queue_target_mult` hint
   table, a NO-MODE-SWITCH debt, with it. Its RTprop filter stays.
2. **One per-leg state** in place of the duplicate estimators. The store
   law's brake bias (§44 erratum: two minima on two horizons) is the same
   defect.
3. **The generation seat.** Keep or archive.
4. **The remaining hint-keyed sites.** These need the Realtime cells.
5. **Waiting.**
   - The rate law needs one off-Bulk row with a which-term-wins tally
     first.
   - The recovery timeouts wait for the store law to settle H.

**Stage 4 — module splits and the harness merge** (one declarative
harness, old plans frozen). Ask first.

**Order.** §41 → §43 → cleanup and merge of `diag36` to main → scoreboard
at main → Stage 1 → user's go on Stage 2 → Stage 3 items one at a time →
Stage 4.


### 45. Addendum — the gate class table, the scoreboard, and what can go now (no VM)

The full inventory is `docs/l1-raw/s45/gate_inventory.md`, read at main
`4926822`; scripts are alongside it. It was reviewed with the advisor
before any deletion.

**Scoreboard at main.**

| quantity | count |
|---|---|
| `RWM_` gates | 95 |
| §11.2 rows | 30 |
| hint-keyed decision sites | 6 engine + 1 harness |
| Rust SLOC (non-test files) | 46 124 |
| smoothed-RTT estimators | 4 |
| RTprop estimators | 3 + P10 |
| delivery-rate estimators | 6 |
| loss estimators | 3 |

**No arm on main was refuted by a battery on the current substrate.**
Every deletion therefore needs a stated route: a structural refutation, a
supersession (`RWM_RATE_WIRE` / `RWM_SRC_BP` precedent, old ledger
GG ~955–1030), or a user ruling. Arm gates are 32, plus 13 for the
generation seat (corrected from "about a dozen").

**Batch 1 (no further ruling needed).** Worker on main, one item per
commit, with the guard after.
- **Dead code**, each with a grep proof at main:
  - `CorrectionDeficit`;
  - the per-path token bucket (`path.rs` ~1136–1165) and its three
    `consume_pace_tokens` calls. It is on the send path, so host ns per
    call is measured before and after;
  - `copa_target_cwnd` and `schedule()`.
- **The honest store family:** `RWM_PLAIN_RS`, `RWM_HONEST_CAP`,
  `RWM_HONEST_K`, `RWM_POOL_ANCHOR`.
  - The route is current-substrate evidence plus a structural derivation
    (§23, §27), as with `RATE_WIRE`.
  - `HONEST_CAP` resolves on, so a test first asserts it is inert without
    `PLAIN_RS`.
  - The shipped neighbours `RWM_HONEST_ANCHOR` and `RWM_RS_ATTR` stay.
  - Each removal updates `lib.sh` `RWM_FORWARD`, regenerates the `[GATES]`
    pin, and writes a paper §10 row (cite, date, substrate, route,
    `archive/<arm>` tag) in the same commit.

**For one grouped user ruling (recommend delete, with an archive tag and a
re-ask clause in the row):**
- the pre-discipline four (2026-07-08): `RWM_REPAIR_WAIT`,
  `RWM_PROACTIVE_PACER`, `RWM_XPATH_REPAIR` (fenced until §41b ends) and
  `RWM_REASM_BDP`;
- `RWM_THREE_TERM` with `RWM_COMPOSED_CAP`. THREE_TERM is the only reader
  of the contract-stall term, which §37's joint law still carries.
- the recovery three: `RWM_HOLDDOWN_Q`, `RWM_REFRESH_FLOOR_US` and
  `RWM_DERIVED_SWEEP`, refuted at today's dwell-inflated H.

**Left out of this pass:** every flagged gate with no verdict, plus the
NEEDS-MORE gates, `COLD_PLACE`, `INFL_CAP`, the Copa wire family, and the
generation seat.

**ADR.** ADR-0052's warn-on-activation stage no longer exists in code. The
promote-or-archive rule replaces it, which needs a short superseding ADR
and the user's go.

**Before the first gate goes:** the guard's parser (`place38_parse.py`,
in use by §41b) is fixed to accept a missing token, tested on a fabricated
`[GATES]` line, once §41b's VM session ends.

**Correction to §45 Stage 3.1 (Copa's legacy controller).** Its cwnd
gates neither intake nor the wire, as stated. But "it only feeds
placement's queue term, `spare_capacity` and the active-path filter" is
wrong. It also:
- clamps repair emission through `spare_capacity`;
- picks paths at ρ < 1 and for the Realtime duplicate;
- is the in-flight leak backstop (`report.rs:183`);
- feeds the ETA wire stamp.

The BtlBw anchor it hosts feeds the shipped store cap and stays. So Stage
3.1 is a redesign of four consumers, not the largest deletion. It needs the
Realtime cells, and moves after the per-leg state (3.2).

### 45. User ruling — the grouped deletions and ADR-0073 (2026-11-10)

- **Groups A, B and C are approved for deletion.** Each gets an archive tag
  and a re-ask clause.
  - **A (pre-discipline):** `RWM_REPAIR_WAIT`, `RWM_PROACTIVE_PACER`,
    `RWM_XPATH_REPAIR`, `RWM_REASM_BDP`. `XPATH_REPAIR` lives in `place.rs`
    and waits until §41b's fence lifts.
  - **B:** `RWM_THREE_TERM` and `RWM_COMPOSED_CAP`. The contract-stall
    formula stays in the paper. The intake-brake path it shares with
    `RWM_LATE_BRAKE` (no verdict) stays.
  - **C:** `RWM_HOLDDOWN_Q`, `RWM_REFRESH_FLOOR_US`, `RWM_DERIVED_SWEEP`.
    Re-ask once the store law sets H.
- **ADR-0073 (promote or archive at the verdict) is accepted.** It
  supersedes ADR-0052's two-stage retirement.
