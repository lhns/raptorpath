# ADR-0075: Vendored quinn-proto with a controller pacing-rate hook

## Status: Accepted (user decision, 2026-11-12)

## Context

quinn-proto's `congestion::Controller` trait lets a controller set only the
window. A `pacing_rate` exists in `ControllerMetrics`, but its only reader is
qlog. The pacer refills at 1.25 x window / srtt whatever the controller
computes (`connection/pacing.rs`). A controller that owns a probe cycle in
rate, such as `bbr_rs`'s 1.25 / 0.75 gain cycle and Drain, therefore has no
actuator: behind the trait the cycle is dead and the window alone probes
(ADR-0068 Addendum 2 and 3, `docs/derivations/w2-pacing-seam.md`, seam S3).
No newer quinn-proto has an equivalent (0.11.19 is the latest checked; the
trait and pacer are unchanged).

## Decision

- Vendor quinn-proto 0.11.14 at `vendor/quinn-proto/`, wired by
  `[patch.crates-io] quinn-proto = { path = "vendor/quinn-proto" }`. `vendor/`
  is excluded from the workspace. The version stays 0.11.14; the first commit
  is a byte-identical copy, so `git diff` between that commit and HEAD is the
  whole patch.
- The patch is minimal and inert by default: `Controller::pacing_rate(&self)
  -> Option<u64>` (bytes/s, default `None`). With `Some(r > 0)` the pacer
  refills at exactly `r` and its burst capacity is `r` x 2 ms, inside the
  existing 10..=256 MTU clamp. With `None` the original code path runs
  unchanged. The window still bounds bytes in flight. Pacing-blocked still
  counts as congestion-blocked, so `app_limited` stays honest.
- `bbr_rs` overrides `pacing_rate()` only under the diagnostic
  `RWM_BBR_PACE=1` (default off, so default behaviour is bit-identical). The
  `[QCC]` line carries `qpace`, `qpr` and `qpn` (count of rates the pacer
  actually read) as the execution witness.
- Rebase the patch on each quinn-proto update we take. Offer it upstream
  (`vendor/quinn-proto/UPSTREAM_PR_DRAFT.md`, drafted, not submitted); if
  accepted, delete the vendored copy.

## Consequences

- We maintain a fork of a dependency: a quinn-proto security or bug-fix
  release needs a manual rebase (the patch is about 70 lines in three files, plus tests).
- Suite-reuse manifests must cover `Cargo.toml`, `Cargo.lock` and `vendor/`,
  and BASE-vs-CAND diff checks must include `vendor/`; a source tree without
  it builds stock quinn-proto and silently ignores the hook.
- The 10-packet burst floor is unchanged, so a controller rate below
  about 10 MTU per 2 ms is burst-limited (the shal8 8-packet-buffer case in
  the audit's risk list). Lowering it is a separate decision.
- `vendor/` is excluded from the workspace, so the vendored crate's own tests
  run with `cd vendor/quinn-proto && cargo test --lib pacing`. In a git
  worktree nested under another workspace, cargo first needs an empty
  `[workspace]` table appended to that `Cargo.toml` (revert it afterwards).
- Cargo resolves exactly one quinn-proto, the path one (`cargo tree -i
  quinn-proto`).
