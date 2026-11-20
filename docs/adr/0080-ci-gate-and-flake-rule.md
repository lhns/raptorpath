# ADR-0080: CI is the gate of record, and a flaky test is a failing test

## Status: Proposed (2026-11-20). Plan: `docs/derivations/test-detection-plan.md` S1.

## Context

- The repo has no CI. Main failed its own gate tests after a "tools only" merge dropped an engine half (fixed in `e304921b`), and nobody saw it until a later run.
- `tools/l1/suites_run.sh` counts a failure that passes one immediate solo re-run as `FLAKE` with exit code 0. A test that passed 4/10 on one tree and 1/10 on another was recorded green on 1–2 samples.
- The host allows one cargo job at a time, so long suites and repeat runs compete with engine work.
- The `github` remote is public. Its standard runners (4 vCPU, 16 GB, 6 h per job) are free with no minute limit.

## Decision

1. **Two-tier GitHub Actions.**
   - Tier A runs on every push to `main` and every PR: `--lib gates`, the math and core crates, the release `--lib` tests, the `tools/l1` tests, the roadmap check and the visualizer gates.
   - Tier B runs nightly: the full `suites_run.sh` set plus a repeat job.
2. **A re-run pass no longer turns a failure into a pass.** `FLAKE` is still recorded, but it fails the suite unless the test is on `tests/quarantine.txt`, with an owner and a reason. A quarantined test is `#[ignore]`d with that reason.
3. **A timing test leaves quarantine after N consecutive passes in the tier-B repeat job** (proposed N = 30: 0.9^30 = 0.04, so 30 clean runs show a failure rate ≤ 10 % at 95 %). One failure sends it back. A timing test that cannot reach N moves to the deterministic simulator (ADR-0082).

## Consequences

- A broken main shows on the push that broke it, or before the merge if tier A is a required PR check.
- The host slot is freed from suites, coverage, mutation and Kani, which all run on CI.
- The first weeks sort today's timing tests into tier A or quarantine. Some currently "green" tests will be quarantined.
- Actions sees only pushes to `github`. If `origin` stays the main remote, a mirror push is part of every merge.
- Open for the owner: canonical remote, push-only or required check, N, quarantine owner.
