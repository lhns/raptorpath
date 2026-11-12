# ADR-0073: An arm is promoted or archived at its verdict

## Status: Accepted (user, 2026-11-10). Supersedes ADR-0052's two-stage retirement.

## Context

ADR-0052 retired a refuted mechanism in two stages: deprecate (the gate
warns on activation), re-test where the refuting substrate may have been
broken, then delete. The warn stage no longer exists in code. ADR-0066 and
`config::deprecated_env_flag` were deleted in 94bf58d. Meanwhile default-off
arms accumulated: main carries 95 `RWM_` gates, 32 of them arm gates plus 13
for the generation seat. None was refuted by a battery on the current
substrate (status §45 addendum). An arm left in place still costs tests,
§11.2 rows, `[GATES]` bytes and reader attention.

## Decision

- **An arm is a question.** At its verdict it is either promoted or
  archived. When promoted, the old path is deleted in the same merge. When
  archived, the steps are:
  1. Tag the parent commit `archive/<arm>`.
  2. Delete the code, its tests and its §11.2 rows.
  3. Write a paper §10 row with the verdict cite, date, substrate, the
     removal route, the tag, and a **re-ask clause**: the condition under
     which the question should be asked again.
- **A removal route is one of:**
  - a structural refutation (the `RATE_WIRE` precedent);
  - supersession (the `SRC_BP` precedent);
  - a user ruling for an arm refuted on an older substrate.
- **Same commit:** each removal updates `RWM_FORWARD` and the byte-pinned
  `[GATES]` echo in the commit that removes the gate.
- **Unchanged from ADR-0052:** its measurement rules (liveness,
  pre-registration, interleaved arms, noise floor) stand. Only the
  retirement stages are superseded.

## Consequences

- The warn stage and the open-ended "kept off" state are gone. A revived
  arm is checked out from its tag and re-registered.
- The re-ask clause carries what the warn stage was meant to protect: the
  chance that an old substrate broke the test.
- First application: status §45 batch 1, the honest store family; and the
  user's grouped ruling of 2026-11-10:
  - the pre-discipline four;
  - THREE_TERM with COMPOSED_CAP (the contract-stall formula stays in the
    paper);
  - the recovery three (re-ask once the store law sets H).
