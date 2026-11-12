# DRAFT, not submitted: let a congestion controller supply the pacing rate

Target: quinn-rs/quinn, `quinn-proto`.

## Problem

`congestion::Controller` exposes only `window()` to the transport.
`ControllerMetrics::pacing_rate` is read only by qlog. The pacer always
refills at `1.25 * window / smoothed_rtt`. A controller whose model is
rate-based (BBR variants, Copa) computes a pacing rate and a probe gain cycle
that nothing uses; quinn's own `Bbr` has the same dead `pacing_rate`.

## Change

Add a provided method to `Controller`:

```rust
/// Pacing rate in bytes per second; `None` keeps the built-in
/// 1.25 x window / smoothed RTT.
fn pacing_rate(&self) -> Option<u64> { None }
```

`Pacer::delay` takes it. With `Some(r)`, `r > 0`: tokens refill at `r`, burst
capacity is `r` x 2 ms inside the existing 10..=256 MTU clamp, and the
"window > u32::MAX disables pacing" rule and the zero-RTT early return do not
apply (the window and RTT are not inputs). With `None` the previous code runs
unchanged. `poll_transmit` passes `self.path.congestion.pacing_rate()`.

## Compatibility

Source-compatible: the new method has a default. Behaviour is unchanged for
`NewReno`, `Cubic` and `Bbr` (a test pins `None`). Possible follow-up, not in
this PR: have `Bbr` return its `pacing_rate`.

## Tests

- Pacer unit tests with absolute values: refill and capacity at a given rate,
  the 10-MTU floor, the 200 kB capacity at 100 MB/s, huge window and zero RTT
  with a rate, `Some(0)` falls back to the window law.
- The existing pacing tests, updated only for the extra argument.

## Open questions for maintainers

- Should `Some(0)` mean "no rate" (as here) or "blocked"?
- Should the burst floor depend on the rate rather than `MIN_BURST_SIZE`?
