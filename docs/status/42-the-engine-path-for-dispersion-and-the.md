## 42. The engine path for dispersion, and the store law's time term — derivation and feasibility, no VM — record

Two read-only reports, reviewed with the advisor. No code, no VM.

**(a) Kernel receive timestamps for an engine L̂. `FEASIBLE-IN-TREE`, not
built.**
- quinn-udp 0.5.14 (pinned) and 0.5.16 carry no receive timestamp:
  `RecvMeta` is addr, len, stride, ecn and dst_ip.
- Its cmsg buffer is a fixed `CMSG_LEN` (`unix.rs:100`), and `decode_recv`
  drops unknown cmsgs. Setting `SO_TIMESTAMPNS` on the fd is therefore
  unsafe: the report computes an overflow of the 88 B buffer on a
  dual-stack socket, which would silently truncate the GRO cmsg. That
  arithmetic is not yet confirmed in source.
- The measurement can only sit at the socket layer: quinn-proto takes one
  `now` per drive cycle. Each owner's socket is one leg, so no 4-tuple
  keying is needed.
- Options:
  - (a) an in-tree `AsyncUdpSocket` receive path on the existing Runtime
    seam (`io_owner.rs` ~849, the `TimedRuntime` pattern), ≈ 2–3 days;
  - (b) a `[patch]` of quinn-udp, ≈ 1–1.5 days, carried as a fork.
- Feedback needs a new `PathReport` or `WindowAck` field: wire v9 → v10
  and an ADR extending ADR-0072. The ignored `throughput_bps` field is not
  to be reused.
- Risks:
  - real-NIC GRO and interrupt coalescing over-read, as §40's `slot` did;
  - CPU (≈ 10–30 ns per buffer, unmeasured);
  - the socket sees only its own flow, where the capture saw every skb.
- Kernel behaviours the report marks UNVERIFIED stay so.

**(b) The time term S. The wire clock is not its source.**
- quinn exposes per leg only the smoothed RTT (`PathView.rtt_us` from
  `path.rtt` / `conn.rtt()`, `io_owner.rs` ~1356–1367), plus a lifetime
  minimum (`RttEstimator::min`, `paths.rs:320–331`, never expires).
  `latest` and RTTVAR are private.
- At c1s, quinn's SRTT stays 2.4–3.5 ms from cap 256 to 1 024 (p34;
  `wrtt` 3–5 ms in §36/§40) while goodput rises from 526 to 792 Mbit/s.
  Any wire-clocked time term (SRTT is its upper bound) puts the law at
  ≈ 262, 0.64 of Gmax.
- What buys goodput at c1s is time inside the endpoints: Tp50 − SRTT =
  1.5 / 3.2 / 6.2 ms at 256 / 512 / 1 024. Quinn's backlog accounts for
  only ≈ 0.8–1 ms of it.
- One residual is unmeasured: ack delay, which SRTT subtracts. It would
  need to be ≈ 1.7 ms to change this.
- So **"c1s decided on SRTT; ack-delay residual unmeasured"**.
- At c1, the knee is an engine property: an endpoint loop of 1.5–6 ms
  against 2.6 ms of wire. Engine latency work (as §32) is the lever there.
  The store law's prize is at c2 and c8.
- **Correction to §37's addendum and to paper §6.1's "D6 decides".** S =
  Tp50 − Tmin already drops a standing component wherever it stands, and
  the static-grid map slopes (0.25–0.54) were measured with dwell inside.
  The time term needs no dwell-free source. Its source is a windowed
  sampler on the existing app-echo samples (what `[LOOPDIAG]` computed).
  It must not use the SRTT level: at c2, SRTT rises with the standing
  queue and the map runs away (slope ≈ 1.1).
- At c2, the smoothed RTT attenuates the 3 ms netem jitter by ≈ ×0.3
  (series p50 − min 1.4–1.9 ms against S 4.9–5.5 ms), so ŵ cannot carry
  c2's S either.
- **The idle-leg credit.** A leg with no trains has no L̂. A "last
  measured" credit is absorbing when pessimistic: at c8 it settles at
  ≈ 160–185, with goodput 65–79 Mbit/s. A decaying-optimistic credit
  `L̃ = (ν·L̂ + κ·L₀)/(ν + κ)` self-corrects in a worked example. κ and ν's
  decay have no provenance, and L₀ inherits the aggregation over-read. It
  is a candidate, not a law, and is kept out of the paper.

**Next (cheapest first; none run yet).**
1. Offline, no VM: the law's per-window noise. On the p27 and p34
   `[LOOPDIAG]` windows at the caps bracketing each fixed point: the
   per-window ask's cv, and how often ask − n has the right sign (D died at
   34–55 %). If p29's power rows carry per-window cap and loop times, also
   check how S moves just after a cap step.
2. If that survives: a closed-loop oracle arm (L from the environment, S
   app-clocked, default off, not for merge, as `run34`), ≈ 30 rows. It is a
   build and needs the user's go.
3. Only if step 2 runs away: the split diagnostic (sender dwell against the
   endpoint loop at µs).
The socket work (2–3 days), wire v10 and the ADR come only after step 2.

### 42. Addendum — the law's per-window noise and its anchor (offline, committed ledgers; scripts and outputs in `docs/l1-raw/s42/`)

*The gate.* The row-median map reproduces §27/§34 exactly: c2 163.7, c8
324.3, c1s p34 614.3, c1s p27 701.1, c1d `CEILING`.

*Noise.*
- Scored against D's test (as briefed), per window of 250 ms:
  - c2 `NEEDS-AVERAGING(2)` (0.89 single-window, 0.97 at 2 windows);
  - c1s p34 `NOISE-BOUND, marginal`: 0.85–0.87 single-window, ≥ 0.97 at
    4 windows; it misses the ≥ 3-decisions-per-third rule by ≈ 0.3 s;
  - c8 `NOISE-BOUND` (0.71, on one live row).
- That test suits a law that steps by (1 + q). This law sets n = ask and
  contracts (slopes 0.25–0.54), so what matters for it is:
  - single-window sign agreement of 0.71–0.89, against D's 0.34–0.55;
  - D's own consecutive-sign statistic of 0.56–0.84;
  - a predicted closed-loop spread sd(ln n) ≈ 0.08 (c2), 0.11 (c1s), and
    ≈ 0.14 at c8 (one row).
- **Noise is not what blocks this law.**

*Caveat at c1.* Every c1s/c1d window overflows the 8 192-sample cap of
`[LOOPDIAG]` (8–15 k samples), so §27/§34's Tmin and Tp50 at c1 come from
≈ 40 % of each window (the first ≈ 100 ms). The direction of the bias is
unknown.

*The anchor (correcting §42: S has a source, RTprop is untested beyond one
filter length).*
- By Little, ask/n = (1+q)·(L/D)·(τ/T̄) with τ = Tp50 − (Tmin − RTprop), so
  the law's only brake is Tmin − RTprop.
- RTprop is Copa's 10 s windowed min (`rtprop_us`). Every fixed point quoted
  so far (164, 324, 614, §40's 329) used a start-of-transfer RTprop, because
  the rows were 4–9 s long, shorter than the filter. In the one 70 s row
  (p27 c1s DEF192) the filter expires and `rtprop_us` climbs from 2.5 to
  61 ms.
- Recomputed with RTprop := each row's lowest post-warm-up `rtt_min_us`
  (what an expired filter holds under a persistent queue; pessimistic):

  | cell | original | expired-anchor crossings | note |
  |---|---|---|---|
  | c2 | 164 stable (0.25) | **157 stable (0.34)**; 927 unstable (1.09) | ask/n 0.93 at 512, 1.01 at 1 024: no restoring force above ≈ 512 |
  | c1s p34 | 614 stable (0.54) | **699 stable (0.56)**, inside the knee band | queue small: brake ≤ 1.7 ms at the floor |
  | c1s p27 | 701 stable (0.82) | 889 stable (0.50) | — |
  | c8 | 324 stable (0.52) | 287 stable, 492 unstable, 526 stable | one row per cap: not decidable |

- At the low points the brake offset is ≈ 0 (c2 within ±0.7 ms, c1s
  +0.35 ms), so the anchor refreshes itself there.
- c2 has a threshold near 927, not a runaway. Below it the cap returns to
  157; above it, it drifts to the clamp. Sitting up there needs a full
  filter length: until the 10 s filter expires, ask at 1 024 is 193, a hard
  pull down. Something outside the law can still hold it there, such as
  competing traffic holding the queue or a start at a high cap.
- Main's own c2 caps already sit at 405–1 100 (p29 MAIN), so that regime is
  today's behaviour, not a new failure.

*Next (proposed to the user; a build, so their go; VM after §41).* A
closed-loop oracle arm at c2 and c1s only:
- L from the environment and S app-clocked; default off, not for merge (as
  `run34`).
- In the paper first: W = 250 ms (no provenance yet), RTprop as Copa's
  shipped 10 s filter, the refresh cadence, and n = ask with no step limit.
- Rows at least three filter lengths long (c2 400 MB, c1s 4 GB), and a
  sampler without the 8 192 cap.
- Per cell, a start-low (boot) and a start-high pair (a static cap above the
  threshold, held longer than the filter, then released to the law). The
  same end point within the predicted spread means one operating point in
  practice. If start-high rows stay high, the anchor (a lifetime minimum or
  a drain) is the user's design decision.
- c8 waits for an idle-leg rule that covers both L and S. Competing-traffic
  rows are named as untested and not added, for cost.
- About 20 rows.
