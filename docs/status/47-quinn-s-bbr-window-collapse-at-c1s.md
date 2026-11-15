## 47. Quinn's BBR window collapse at c1s — diagnosis, no VM — record

From a read-only diagnosis of quinn-proto 0.11.14's BBR, plus kept ledger
logs. Each claim is marked as verified here or as inferred in the report.

*Verified here.*
- **Startup growth is unbounded.** In `congestion/bbr/mod.rs:326`, before
  full bandwidth, `cwnd += bytes_acked` runs while
  `self.cwnd_gain < target_window as f32`. That compares a gain (2.885)
  with a byte count, so it is always true. Before BBR declares full
  bandwidth, `qcwnd` is therefore accumulated acked bytes, not BBR's model.
  That is the MB-scale qcwnd on healthy rows.
- **Once at full bandwidth** (`mod.rs:388`), cwnd = min(target,
  cwnd + acked), with target = gain·bw·min_rtt + ack height. Both bw (a
  ratchet, §37) and min_rtt (a lifetime minimum) are one-way, so the model
  BDP can only shrink.
- **ProbeRtt fingerprint.** In §41b's collapsed row (MAIN c1s s7 r3),
  `qcwnd` = 5 808 = 4 × 1 452 (`min_window`, `mod.rs:633`) at exactly
  t = 10.0, 20.1, 30.1 and 40.1 s. ProbeRtt runs at 0.75 × model BDP,
  floored at min_window, so the model BDP is ≤ 7.7 KB against a true
  ≈ 250 KB at c1s.
- **Model BDP at c2 (`oracle43b` rows).** At the 10 s ticks, qcwnd reads
  ≈ 180–600 KB, against a true BDP of ≈ 125 KB, so the model there is an
  over-read (as in §24). The tiny model is a 1 Gbit/s effect.
- **No upstream fix.** quinn-proto 0.11.19 changes only BBR's RNG; the
  bandwidth estimator and min-max filter are unchanged.

*Inferred (not yet verified).*
- **Cause of the tiny model at c1:** quinn stamps one `now` per endpoint
  poll, so per-packet `on_ack` bandwidth samples share a timestamp. They are
  noise around 1 MTU per inter-poll interval. An RTT sample taken against
  the poll stamp can read below the physical RTT and become the lifetime
  min_rtt.
- §43's runaway row shows its ProbeRtt window stepping from 27 796 to
  10 171 bytes at ≈ 85–90 s with bw only rising. That is consistent with
  min_rtt falling ≈ 2.7× on a fixed 2 ms path.
- **Trigger:** the collapsed rows are the ones that left Startup. A loss
  burst precedes both collapses: §41b `qce` 6 → 7, lost +28 at 1.8 s; §43
  `qce` 22 → 26, lost +98 at 4.0 s. Short healthy rows stay in Startup.
- Which term dropped at the collapse moment (min_rtt or the ack-height
  lapse) is not determined.

*What it changes.*
- Before full bandwidth, quinn's window is not a controller. Every knee and
  fixed point so far, §43b included, was measured with the store as the
  only brake.
- §34's (b1) K readings, the "K fixed point beside 614", paper §8.4 and
  §10's (b1) row need re-reading. Owed.
- The c1s τ test (§43b's follow-up) is blocked: its window was derived with
  quinn uncontrolled, and a row that leaves Startup would read UNSTABLE
  again.
- The merge candidate is not blocked: main is exposed today, and nothing on
  `diag36` changes that.

*Witness (replaces the qcwnd-ratio readout).* The share of `[DIAG]` lines
with `qcwnd` below the cell's true BDP: a few percent on healthy rows (the
ProbeRtt ticks), most lines on a collapsed one. Reported only, in the
guard's tail line and in §46 by amendment.

*Next cheapest evidence.*
1. A host test driving quinn's `Bbr` with one `now` per synthetic poll at
   1 Gbit/s plus a loss burst. It is also the red test for any fix.
2. A Debug print of quinn's BBR state on a never-merged run branch, only
   if the offline read cannot separate min_rtt from bandwidth.

*The user's decision (the substrate).* Whether the store law is meant to
be the congestion controller (ADR-0068 is still Proposed). The options:
1. Leave quinn's BBR as is and carry the witness.
2. A patched in-tree `bbr_rs` with its own windowed min-RTT. Today it
   shares the min_rtt line, the Startup unit bug and ProbeRtt with
   upstream.
3. A loss-based substrate (Cubic or NewReno). That means new baselines.

A working substrate window makes quinn a second controller at every cell.
That moves baselines and makes the nested-loop question (q12) live.
