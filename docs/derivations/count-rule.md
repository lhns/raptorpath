# The count rule for the floor minimum (R6.7 open item 4), after s62's NO-GO-GATE

Status: DERIVATION. Docs and Python only: no engine code, no cargo, no VM. Branch `count-rule` off `github/main`
`40ebd766`. It answers the derivation that status "§62 Result" names: R6.7 item 4's count rule, starting from the D12
dump. **Every reading here is post hoc**, on existing ledgers. Whatever is proposed is untested.

**In plain words.**
- **What the J = 0 floor looks like.** With the jitter off, the floor's echo samples sit in a tight bulk (median
  46.76 ms) with a separate low mode near 41.2–41.9 ms. About 2–3 % of samples are in the low mode.
  - The low mode is the queue-free loop: the 40 ms propagation plus one datagram and processing.
  - Its samples are the first one or two sent in a **small flight**: a release of one to five slots, which reaches an
    idle link, so its ack comes back without waiting.
  - Full flights (about 10 slots at W 12) never produce one. Their samples carry the flight's own serialization at
    the bottleneck, about 10 × 0.49 ms.
- **Why the native window misses it.** Small flights come in episodes. Only a third of native windows (60 samples,
  250 ms) hold a low sample, so the native minimum reads 44.6 ms where 250 or more samples read 41.3–41.7.
  - The step from native windows to 250 samples is this occupancy of an episodic low state.
  - It is not an extreme-value bias on a smooth thin tail, and not the windows' phase.
- **The law's floor is a native-window quantity.**
  - R6.2's F = min(m̂, ℓ̂ + b̂₀⁺) + b̂₀⁺ lands at or above the native window minimum on every ledger: 46.0 at J = 0,
    44.2–46.7 at J = 5, 9.4 at c2.
  - R6.7 candidate C is the native median by construction.
  - Only m̂ (Copa's 10 s minimum, ≈ 2 400 samples at c3's floor) reads the low mode.
- **No floor reading is both the law's quantity and count-stable at J = 0 and J = 5** (§3 table).
  - A minimum over n₀ ≥ 500 samples is stable at J = 0 (≤ 0.21 ms per 8×). It still falls 2.5–4.1 ms at J = 5 and
    1.2 at c2, because there the jitter sets the tail.
  - A per-window quantile is stable at both, but at J = 0 it reads the bulk, ≈ 5 ms above the queue-free loop.
- **For a re-run of s62 at J = 0** under an n₀ = 500 floor (untested): the floor's gate would pass (predicted
  0.19–0.21 ms). But the knee side's minimum still falls with count at J = 0: W 80 −0.63 and W 128 −7.73 ms from 500
  to 4 200 samples.
  - So the knee need is readable only with a knee-side gate, and existing rows cannot say whether it passes.
  - At J = 5 floor and knee fell together (equal-count need stable to 0.2–0.3 ms). At J = 0 the floor stopped falling
    and the knee did not.

**Advisor (Fable) log.**
1. **Before writing.**
   - Classify low samples by send time, not by arrival gap.
   - Test the window-edge reading against absolute time mod 250 ms.
   - Find the independent draw (random against consecutive, per unit).
   - Read the law's inputs from `[SLAW]` (m, l, b0p) rather than deriving them, and quote Copa's expiry on the gauge
     tree.
   - Put J = 5 beside J = 0 per arm before concluding on the knee.
   - If no reading meets both conditions, say so in a table, and mark any proposal untested.
2. All applied.

**Scripts** (stdlib, `python -I -X utf8`, run from `docs/`; output beside them in `store-law/`):

| script | input (read in place) | output | what it computes |
|---|---|---|---|
| `crdump.py` | `l1-raw/s62/rep62r0-diag.tgz` (the D12 `RWM_RTT_DUMP` row) | `crdump.out` | the alignment of the dump with the `[SLAW]` windows, the shape, what a low sample follows (arrival and send views), phase, the independent draw, the minimum against the count, quantiles |
| `crrule.py` | `rep62r0-diag.tgz`, `l1-raw/s61/rep61c-…`, `rep61a-…`, `l1-raw/s60/rep60a-…`, `rep60b-…` (each loaded on its own) | `crrule.out` | the floor minimum against the count per ledger, every arm's ladder at J = 0 and J = 5, the knee need at equal counts, the law's floor from its printed inputs, and the rule's check |

---

## 1. The D12 dump: what the J = 0 floor's low samples are (`crdump.out`)

**Alignment.**
- The dump format is `[RTTDUMP] p= t0= n= d=dt,rtt;…` in µs (`net/rttdump.rs`, `slaw-gauge` `d416d59b`).
- It is fed in `record_rtt_sample` immediately before the `[SLAW]` sampler, with the same value
  (`scheduler/path.rs`).
- Sample i belongs to the window where the cumulative `[SLAW]` nk passes i. The recomputed window minimum equals the
  printed tmin on **368 of 368** windows inside the dump.
- That leaves 306 LATE windows, with a median nk of 60.

**The shape** (17 506 LATE samples; §1 of the output):

| quantile | p0 | p0.5 | p1 | p2 | p5 | p10 | p25 | p50 | p90 | p99 |
|---|---|---|---|---|---|---|---|---|---|---|
| ms | 41.18 | 41.73 | 41.97 | 42.48 | 45.66 | 46.22 | 46.56 | 46.76 | 47.40 | 49.13 |

- 2.19 % of samples lie more than 4 ms below the median, and 5.2 % more than 1 ms below. It is a separate low mode,
  not a stretched tail. The bulk p10–p90 spans only 1.2 ms.
- **The low mode is the queue-free loop:** 41.2–41.9 ms is the 40 ms propagation plus ≈ one datagram (0.49 ms at
  20 Mbit/s) plus processing.
- **The bulk** sits ≈ 5.4 ms higher. That is about one full W 12 flight's serialization (≈ 10 datagrams × 0.49 ms)
  plus the ack waiting for its tail.

**What a low sample follows: by send time** (s = t − rtt; flights are samples whose send times lie within 2 ms of
each other; §2b).
- The 1 738 flights are 5.7 per window, about 10 samples each, one loop apart (median send gap 46.6 ms).
- **Deep-low samples** (> 4 ms below the median) by case:
  - 57 are alone in a one-sample flight;
  - 75 are in the first flight after a send pause of ≥ 60 ms;
  - 251 are inside a multi-sample flight.
  - Almost all are among the first sent: send ranks 0 / 1 hold 206 / 140 of 383.
- **By flight size,** the share of flights holding a deep-low sample is:

  | flight size | 1 | 2 | 4 | 5 | 10 (full) | 12+ |
  |---|---|---|---|---|---|---|
  | share | 57/65 | 72/219 | 8/8 | 4/8 | **0/194** | 65/1 212 |

  The low mode is the head of a small release, and never part of a full one.
- **By the send gap before the flight:**

  | send gap | ≥ 60 ms | 20–40 ms | one loop (40–60 ms) |
  |---|---|---|---|
  | share low | 0.92 | 0.63 | 0.095 |

**Not the window's phase.**
- Deep-low samples by absolute dump time mod 250 ms are flat: 30–45 per decile against all samples' 1 625–1 890.
- The apparent edge enrichment by position inside a window (§2: deciles 0 / 9 hold 83 / 68) is the normalisation. The
  first and last flight of a window are pinned to its edges.

**Not evenly spread in time.**
- Low samples arrive in episodes: the inter-arrival CV is 2.02, where 1 would be Poisson-like.
- Only **99 of 306** native windows hold any deep-low sample. If the windows that do hold one hold 2–6, those 2–6
  come from one or two small flights.

**The independent draw** (§2c). At every unit (samples, arrival bursts, flights), a random draw of one native window's
worth reaches the low mode, while the consecutive one does not:

| unit | random | consecutive |
|---|---|---|
| samples | 42.05 | 44.66 |
| flights | 42.12 | 44.55 |

- From 4 windows (≈ 230 samples, ≈ 1 s) random and consecutive agree (41.55 / 41.56).
- Independent flights would put a low sample in 51 % of windows (p = 0.119 per flight, 5.7 per window). The measured
  share is **32 %**.
- So the draw is an episode of small flights lasting longer than one window, and ≈ 1 s of samples mixes episodes.

**Q1's answer: the native → G(250) step (2.97 ms) is windowing of an episodic state.** A native window (60 samples,
≈ 6 flights) lies inside one sender-release episode, and misses the queue-free state two-thirds of the time.
- From ≈ 250 samples the minimum is in the low mode, and it creeps only within the mode's own 0.5 ms width: 41.55 /
  41.40 / 41.37 / 41.33 / 41.25 at 250 / 500 / 1 000 / 2 100 / 4 200 consecutive samples (§3).
- So the step is a property of the sender's release pattern (flight size), sampled by 250 ms windows. It is not a
  thin smooth tail's extreme-value bias.

---

## 2. The law's floor, and the count each input takes its minimum over (`crrule.out` §4)

**R6.2's line** (`store-law.md`: N = 1, single leg):

```text
   F  =  min( m̂ , ℓ̂ + b̂₀⁺ ) + b̂₀⁺          b̂₀⁺ = max over the first K = 8 windows of ( T̂min,k − m̂k )
   m̂: Copa min_rtt over a 10 s time window   (raptorpath/src/scheduler/copa.rs: window_duration = Duration::from_secs(10), :641;
       the monotonic deque at :990–1008; on the gauge tree slaw-gauge d416d59b, expire_old_samples :1489–1495 pops samples older than now − 10 s)
   ℓ̂: the lifetime minimum of the same series (net/slaw.rs WinSampler::observe, life_min, slaw-gauge d416d59b)
   T̂min,k: the native 250 ms window's minimum (WinSampler::observe min, closed per [DIAG] tick)
```

**Read from the printed `[SLAW]` fields** (m, l, b0p; LATE medians; ms):

| floor arm | m̂ | ℓ̂ | b̂₀⁺ | **R6.2 F** | **C F̂** (median of the last 4 native minima) | F native | F G(250) | F G(2 100) |
|---|---|---|---|---|---|---|---|---|
| c3 J = 0, s62 W 12 pair | 41.30 | 41.09 | 4.69 | **45.99** | **44.59** | 44.66 | 41.69 | 41.30 |
| c3 J = 0, D12 (dump on) | 41.33 | 41.04 | 5.40 | 46.73 | 44.55 | 44.60 | 41.55 | 41.33 |
| c3 J = 5, 61c W 12 | 34.16 | 32.95 | 9.53 | **44.20** | 42.69 | 42.75 | 38.78 | 34.32 |
| c3 J = 5, 61a W 12 | 35.33 | 33.07 | 11.01 | 46.74 | 42.38 | 42.36 | 38.89 | 35.29 |
| c2 s60 W 16 | 6.35 | 5.59 | 2.80 | **9.38** | 8.67 | 8.68 | 8.51 | 7.07 |
| c1s s60 W 64 | 2.26 | 2.25 | 0.17 | 2.43 | 2.38 | 2.39 | 2.39 | 2.39 |

**The count each input takes its minimum over** (the floor arm's LATE sample rate × the input's span):

| cell | samples/s | native window | m̂ (10 s) | ℓ̂ (lifetime, at t = 15 s) |
|---|---|---|---|---|
| c3 (J = 0 and J = 5) | 240 | 60 | ≈ 2 400 | ≥ 3 600 |
| c2 | 1 128 | 282 | ≈ 11 300 | ≥ 17 000 |
| c1s | 20 870 | 5 218 | ≈ 209 000 | ≥ 313 000 |

At the c3 knee (≈ 1 840 samples/s) m̂ covers ≈ 18 400.

**Q2's answer: the law's floor is a native-window quantity.**
- m̂ alone takes the minimum over ≈ 2 400 samples at c3 and reads the G(2 100) level (41.30 against 41.30 at J = 0;
  34.16 against 34.32 at J = 5).
- b̂₀⁺ adds back the gap between a native window's minimum and m̂, at its maximum over 8 boot windows. When the min
  takes its m̂ branch, F = m̂ + max_k(T̂min,k − m̂k) ≈ max_k T̂min,k.
  - That is why R6.2's F reads at or above the native minimum: +1.3 ms at J = 0, +1.4–4.4 at J = 5, +0.7 at c2.
  - The branch can switch under load, so this closed form is not general, and the table is the reading.
- C reads the native median by construction.
- **So both of the law's floors carry the native window count:** 60 samples at the c3 floor, 282 at c2, 5 218 at c1s.
  The count varies 87× across cells for the same 250 ms.
- ℓ̂ is start-history-prone (E13). On D12 the first window (nk 184, tmin 41.226) set m and l.

---

## 3. The count rule: candidates against both conditions (`crrule.out` §1, §2, §2b, §3)

**(a)** Is it the same quantity the law uses? **(b)** How far does it drift above its count, from n₀ to 8·n₀ (or the
largest count available)? J = 0 has three rows: two floor rows and one dump-on row.

| candidate floor reading | (a) the law's quantity? | J = 0 (pair / D12) | J = 5 (61c / 61a) | c2 | c1s |
|---|---|---|---|---|---|
| native window minimum (C; R6.2's level) | **yes** | 3.40 / — (native → 4 200) | 9.52 / 7.34 | 1.95 | 0.00 |
| minimum over n₀ = 250 samples | no (a count-window floor) | 0.36 / 0.22 | 4.46 / 3.60 | 1.38 | 0.00 |
| **minimum over n₀ = 500 samples** | no | **0.21 / 0.19** | 4.11 / 2.49 | 1.21 | 0.00 |
| minimum over n₀ = 1 000 samples | no | 0.11 / 0.14 | 2.48 / 1.47 | 0.78 | 0.00 |
| m̂ (10 s time window) | an input of the law, not its F | its count is time-set: 2 400 / 209 000 samples by cell | | | |
| per-window p10 (count-free) | no | 0.22 (dump, native → 36 windows), but reads the bulk at **≈ 46.2–46.5, ≈ 5 ms above the queue-free 41.3** | ≤ 0.18 (`knee-margin-rederive` `kmr.out` §10a) | 0.05 | – |
| equal-count difference (need at one n on both sides) | the law's form if both sides change | the knee side moves (below) | **0.31 / 0.21** spread over 460 → 1 850 | **0.29** over 2 340 → 9 360 | – |

**No candidate meets both (a) and (b).**
- The law's own native minimum drifts by 2–10 ms.
- A count-window minimum is stable only where the jitter is absent (J = 0, c1s).
- A quantile is stable but measures the flight-serialized bulk, not the queue-free loop that §1 identifies and the
  law's sentence names.
- **"Queue-free" at J = 0 means the low mode** (§1). Only a minimum over ≥ ≈ 250 samples (≈ 1 s at c3's floor) reads it.

**The knee side, J = 0 beside J = 5** (`crrule.out` §2b, drift from 500 to 4 200 samples):

| arm | J = 0 drift | J = 5 (61c) drift |
|---|---|---|
| floor W 12 | 0.23 | 4.13 |
| W 80 | 0.63 | 2.56 |
| W 88–120 | – | 2.29–3.16 |
| W 128 | 7.73 | – |
| W 152 | – | 8.42 |

- **At J = 5,** floor and knee arms fall together. So the equal-count need is stable: 8.47–8.78 on 61c, 8.32–8.53
  on 61a, 5.45–5.75 at c2, over the window ladder with ≥ 3 floor groups per row.
- **At J = 0** the floor stops falling, but the knee-side arms do not. So an equal-count need₀ would carry the knee's
  own count dependence. One row below the knee (W 80) and one above (W 128) cannot bound it at the knee.

**Proposed rule (untested), for review:**

```text
   F(n₀)  =  the minimum over n₀ = 500 consecutive echo samples of the floor arm        (a count window, not a 250 ms window)
   T̂min(n₀) at the operating point over the same n₀                                     (one count on both sides)
   floor gate:  F(n₀) − F(8·n₀) < 0.5 ms          knee gate:  T̂min,knee(n₀) − T̂min,knee(8·n₀) < 0.5 ms
```

**Provenance.**
- **n₀ = 500 is declared.** It is the smallest tested count that holds the low mode with margin at J = 0. The minimum
  is in the mode from ≈ 250 samples, and drifts 0.19–0.21 ms above 500.
- The 0.5 ms gates are s62's (half of Δ₀).
- The knee gate is new. It is required because the knee's minimum falls with count at J = 0.

**It meets (b) at J = 0 for the floor only.** It does not make the J = 5 or c2 floor stable. There only the
equal-count difference is stable, so the rule's knee and floor must move together.

**For the law,** adopting n₀-sample windows for T̂min and the floor is a law change (R6.7 item 4), reviewed as one.
- At c3's floor, 500 samples take ≈ 2.1 s, so b̂₀⁺'s K = 8 boot windows would take ≈ 17 s. That cost is the design's
  to weigh.

---

## 4. What the rule predicts for a re-run of s62 at J = 0

| reading | prediction (from the pair, D12 and the probes) | bounded by existing rows? |
|---|---|---|
| floor gate F(500) − F(4 000) | 0.19–0.21 ms: **PASS** (< 0.5) | yes (three rows) |
| F₀ = F(500) | 41.44–41.49 ms, so Δ₀ = ½·q_R·F₀ ≈ 1.04 ms | yes |
| knee gate T̂min,knee(500) − T̂min,knee(4 000) | between W 80's 0.63 and W 128's 7.73 ms. At the knee (W₉₅ ≈ 100–115) it likely exceeds 0.5 | **no**: register it as the re-run's first reading |
| need₀ at n₀ = 500 | not predicted | no |

**So the knee need could be read on a re-run only if the knee gate passes, and the existing rows suggest it may not.**
- Below the knee, W 80's 0.63 already fails the 0.5 ms gate.
- If the knee gate fails, the J = 0 knee reading, like the J = 5 one, needs a statistic whose knee and floor count
  dependence cancel. An equal-count difference does that at J = 5, and the J = 0 data show it would not at J = 0.
- That is a statement about the operating point's minimum: it falls with count through rare queue-drain events at
  both J. It belongs to the next derivation, not to this one.

## 5. Owner-facing

**There is no owner choice yet.**
- Whether the store law moves from 250 ms windows to n₀-sample windows is design work for review, a law change under
  R6.7 item 4.
- A re-run of s62 would need a registration amendment (the n₀ floor gate, the new knee gate). Its value depends on the
  knee gate, which this derivation predicts may fail.
- So the next step is the design review of §3's rule and of the knee side, not a VM session.
