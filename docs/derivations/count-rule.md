# The floor minimum's count (R6.7 open item 4), after s62's NO-GO-GATE

Status: DERIVATION, with one bounded follow-up (§6, Fable's review). Docs and Python only: no engine code, no cargo,
no VM. Branch `count-rule` off `github/main` `40ebd766`. It answers the derivation that status "§62 Result" names:
R6.7 item 4's count rule, starting from the D12 dump.

**Every reading here is post hoc,** on existing rows. The mechanism rests on one dump row (D12) and is replicated on
s62's floor pair only by window-level fields.

**In plain words.**
- **What the J = 0 floor looks like.** With the jitter off, the floor's echo samples sit in a tight bulk (median
  46.76 ms) with a separate low mode at 41.2–41.9 ms. About 2–3 % of samples are in it.
  - The low mode is the queue-free loop: the 40 ms propagation plus about one datagram (0.47 ms) plus processing.
- **What produces the low mode: retransmissions.**
  - On all three J = 0 floor rows, **84–87 % of the native windows whose minimum reaches the low mode carry a
    retransmit in the same 250 ms tick, against 5–8 % of the other windows.**
  - All **53 of 53** send pauses of 60 ms or more in the dump sit in a window whose tick, or the one before, carried a
    retransmit. Across all windows that share is 48 %.
  - So the queue-free sample is the small release that follows a loss repair, sent into an idle link.
  - How often the floor sees the queue-free loop therefore depends on the loss process and the send rate. **No sample
    count is a path property.**
- **Why the native window misses it.** These events reach 29–32 % of native windows on all three rows, in runs of 1–2
  windows. So the native minimum reads 44.6 ms, where ≥ 250 samples read 41.3–41.7.
  - The native → G(250) step (2.97 ms) is a 250 ms window missing a loss-driven event.
- **The law's floor.**
  - R6.2's F = min(m̂, ℓ̂ + b̂₀⁺) + b̂₀⁺ has no count to vary. Its problem is its spread across rows and sessions:
    45.4–46.7 at J = 0; 37.2–46.2 (61c) and 42.9–138.5 (61a) at J = 5; 8.0–10.5 at c2.
  - That spread comes from b̂₀⁺, a maximum of 8 start-transient differences (§2).
  - **The anchor the law needs is a long-window minimum, and the law already has one: m̂** (Copa's 10 s minimum). At
    J = 0 m̂ reads the low mode on every row (41.26–41.33).
- **No count rule is proposed.** A minimum over 500 samples is stable at J = 0, but it drifts 2.5–4.1 ms at J = 5 and
  1.2 ms at c2, and its value comes from one cell. It stays an observation.

**Advisor (Fable) log.**
1. **Before the first write** (applied):
   - classify low samples by send time;
   - test phase against absolute time;
   - find the independent draw;
   - read the law's inputs from `[SLAW]`;
   - put J = 5 beside J = 0 per arm;
   - tabulate, and mark proposals untested.
2. **Review of `b2f89645`, the follow-up §6** (applied):
   - put R6.2's F and C in the table as the law's quantities;
   - explain F's cross-session spread from source;
   - test the flight mechanism against W;
   - replicate on R2 / R3 and align the pauses with loss;
   - correct the flight table;
   - drop the n₀ rule.

**Scripts** (stdlib, `python -I -X utf8`, run from `docs/`; output beside them in `store-law/`):

| script | input (read in place) | output | what it computes |
|---|---|---|---|
| `crdump.py` | `l1-raw/s62/rep62r0-diag.tgz` (the D12 dump row) | `crdump.out` | the alignment with `[SLAW]`, the shape, what a low sample follows (arrival and send views), phase, the independent draw, the minimum against the count, quantiles |
| `crrule.py` | `rep62r0-…`, `l1-raw/s61/rep61c-…`, `rep61a-…`, `l1-raw/s60/rep60a-…`, `rep60b-…` | `crrule.out` | the floor minimum against the count, every arm's ladder at J = 0 and J = 5, the knee need at equal counts, the law's floor from its printed inputs, the n₀ observation |
| `crfollow.py` | `rep62r0-…`, `rep61c-…`, `rep61a-…`, `rep60a-…` | `crfollow.out` | b̂₀⁺'s raw boot brakes and F per row; symbols per emit burst against W; the low mode's share and run lengths on R2 / R3 / D12; retransmit and idle deltas per tick |

---

## 1. The D12 dump: what the J = 0 floor's low samples are (`crdump.out`, `crfollow.out` C–D)

**Alignment.**
- The dump format is `[RTTDUMP] p= t0= n= d=dt,rtt;…` in µs (`net/rttdump.rs`, `slaw-gauge` `d416d59b`).
- It is fed in `record_rtt_sample` immediately before the `[SLAW]` sampler, with the same value.
- The recomputed window minimum equals the printed tmin on **368 of 368** windows inside the dump. That leaves 306
  LATE windows, with a median nk of 60.

**The shape** (17 506 LATE samples):

| quantile | p0 | p0.5 | p1 | p2 | p5 | p10 | p25 | p50 | p90 | p99 |
|---|---|---|---|---|---|---|---|---|---|---|
| ms | 41.18 | 41.73 | 41.97 | 42.48 | 45.66 | 46.22 | 46.56 | 46.76 | 47.40 | 49.13 |

- 2.19 % of samples lie more than 4 ms below the median. It is a separate low mode below a bulk whose p10–p90 spans
  1.2 ms.

**Flights by send time** (s = t − rtt; samples within 2 ms of each other; corrected table). There are 1 738 flights:
5.7 per window, 10.1 samples per flight, one loop apart.

| flight size (samples) | 1 | 2 | 3 | 4 | 5 | 7 | 9 | 10 | 11 | ≥ 12 | all |
|---|---|---|---|---|---|---|---|---|---|---|---|
| flights | 65 | 219 | 4 | 8 | 8 | 7 | 4 | 194 | 17 | **1 212** | 1 738 |
| holding a deep-low sample | 57 | 72 | 1 | 8 | 4 | 0 | 0 | 0 | 0 | 65 | 207 |

- Flights of ≤ 5 samples hold a deep-low sample in 142 of 304 cases (47 %). Flights of ≥ 10 samples do so in 65 of
  1 423 (4.6 %). Most flights (70 %) have ≥ 12 samples.
- Deep-low samples are among the first sent in their flight: send rank 0 / 1 holds 206 / 140 of 383.
- By the send gap before the flight, the share holding a deep-low sample is:

  | send gap | ≥ 60 ms | 20–40 ms | one loop (40–60 ms) |
  |---|---|---|---|
  | share | 0.92 | 0.63 | 0.095 |

**Not the window's phase.**
- Deep-low samples by absolute dump time mod 250 ms are flat: 30–45 per decile.
- The apparent edge enrichment by position inside a window is the normalisation, since a window's first and last
  flights are pinned to its edges.

**Episodic.**
- The inter-arrival CV is 2.02.
- Random against consecutive draws agree only from about 4 windows (≈ 230 samples, ≈ 1 s).
- 32 % of native windows hold a deep-low sample, against 51 % if flights were independent.

**Replicated on s62's floor pair** (window-level, `crfollow.out` C). The share of native windows whose minimum lies
> 4 ms below the row's median tp50:

| row | share | runs of low windows (median / max) |
|---|---|---|
| D12 | 100 / 309 (32 %) | 1 / 5 |
| R2 (G12 s42 r1) | 96 / 305 (31 %) | 2 / 5 |
| R3 (G12 s7 r2) | 88 / 303 (29 %) | 1 / 4 |

**What causes the pauses and the low mode: retransmissions** (`crfollow.out` D).
- Per 250 ms tick, a window whose minimum reaches the low mode carries a retransmit (the `[DIAG]` `retx=` delta) in the
  same tick on **87 / 84 / 84 %** (D12 / R2 / R3) of cases, against **8 / 7 / 5 %** for the other windows.
- The previous tick carries one on 48–54 %, against 20–25 %.
- **53 of 53** of the dump's send pauses ≥ 60 ms sit in a window whose tick, or the one before, carried a retransmit,
  against 48 % of all windows.
- **Reading:** a loss stalls the store's release. The repair and the slots it frees go out as a small flight into an
  idle link, and its echo is the queue-free loop.
  - At J = 5 the same window test separates poorly (35–38 % against 11–25 %), because jitter lows fill most windows
    there.

**Q1's answer.**
- **The low samples are not spread evenly in time.** They follow retransmit-driven send pauses, at the head of the
  small flights that follow.
- **The tail's shape:** 2–3 % of samples, 4.5–5.5 ms below the bulk, in episodes of 1–2 windows reaching ≈ 30 % of
  native windows.
- **The native → G(250) step comes from the event process,** not from an extreme-value bias on a smooth tail. A 250 ms
  window misses a loss-repair event about two-thirds of the time, and ≥ ≈ 1 s of samples almost always holds one. Above
  that the minimum creeps only within the mode's 0.5 ms width.
- **So the floor's count dependence at J = 0 is set by the loss rate and the send rate,** and no sample count is a path
  property.

---

## 1b. The bulk's offset and W: the flight mechanism tested (`crfollow.out` B)

**The prediction.** If the bulk sits above the queue-free mode because a flight queues behind itself at the
bottleneck, the offset should grow with the flight. Doubling W from 12 to 24 does not double the flight.

| arm | 61c symbols per emit burst | 61c window tp50 (ms) | 61a symbols per emit burst | 61a window tp50 (ms) |
|---|---|---|---|---|
| W 12 | 6.52 | 48.73 | 6.31 | 48.47 |
| W 16 | 7.64 | 48.59 | 7.57 | 48.50 |
| W 24 | 7.88 | 49.40 | 7.71 | 49.17 |
| W 60–108 | 8.4–8.8 | – | – | – |

- The symbols per emit burst come from the `[DIAG]` `eb_depth` (= eb_syms / eb_bursts).
- **Flights stop growing with W.** Every emit burst ends with `eb_end` = `store` (for example 1 903 of 1 906 on 61c
  W 12): the burst stops when the store has no free slot.
  - So a burst is the slots one ack frees, which is ack-clocked.
  - A larger W adds bursts per loop, not burst length.
  - c2 shows the same, at 8.2 / 10.1 / 10.6 at W 16 / 32 / 48. s62 J = 0 shows 6.2 / 5.6 / 5.3 at W 12 / 80 / 128.
- **What it explains, and what it does not:**
  - From W 12 to W 24 the burst grows by 1.36 / 1.40 symbols, which is 0.64 / 0.66 ms at 0.47 ms per datagram. The
    tp50 rose 0.67 / 0.70 ms. That matches.
  - **W 16 does not fit.** The burst grows by 1.1 symbols, yet tp50 falls by 0.14 / rises by 0.03 ms.
  - At J = 0 the bulk's offset above the low mode (≈ 5.4 ms) is larger than one burst's serialization
    (6.2 × 0.47 ≈ 2.9 ms). So **about half the offset has another source.** The receiver's ack timing is a candidate,
    not measured here.
- **The mechanism therefore holds in part.** Bursts are capped by the ack-clocked release, which explains the small
  W 12 → W 24 rise. Burst serialization covers about half of the bulk's offset, and the rest is not explained by these
  rows.

---

## 2. The law's floor, and why it spreads across sessions (`crrule.out` §4, `crfollow.out` A)

**R6.2's line** (`store-law.md`, N = 1):

```text
   F  =  min( m̂ , ℓ̂ + b̂₀⁺ ) + b̂₀⁺
   m̂:   Copa min_rtt over a 10 s time window (copa.rs window_duration = 10 s, :641; deque :990–1008;
         slaw-gauge d416d59b expire_old_samples :1489–1495)
   ℓ̂:   the lifetime minimum (net/slaw.rs WinSampler::observe, life_min)
   b̂₀⁺: net/slaw.rs WinSampler::close (slaw-gauge d416d59b, :150–165): brake_k = tmin_k − m̂_k is pushed for the process's first
         K_GAUGE = 8 windows (k ≤ 8, :152–156), and b0p = max(brakes) once k ≥ 8 (:164–165). K_GAUGE = 8 (:29), "a GAUGE parameter"
```

**b̂₀⁺ is a maximum over the process's first 8 windows (2 s, start-up included),** each differencing a native
window's minimum against an m̂ that is only seconds old. `crfollow.out` A prints the raw brakes per row:
- **At J = 5,** m̂_k falls through boot (61c r1: 48.0 → 43.5 → 40.8 → 39.5) while the late m̂ reaches 34.0–35.3. Boot
  windows hold 3–223 samples. Brakes of 0–12.6 ms enter the max, and 61a r5 has a 103.4 ms brake at k = 2.
- **So F = (late m̂) + (early max brake)** joins a long-window minimum taken late with one extreme of 8 start-transient
  differences. That is why F spreads:
  - 61c rows 37.22–46.22 (median 44.20);
  - 61a rows 42.91–138.47 (median 46.74);
  - while the native minimum differs between the sessions by 0.4 ms.
  - The medians differ through b̂₀⁺ (9.53 against 11.01) and m̂ (34.16 against 35.33, a 10 s minimum of a jittered
    stream).
- **At J = 0** the brakes are steadier (0–5.4; m̂_k flat at 41.1–41.2 from k = 1, set by the first window's start
  sample, E13's start history). F spans 45.36–46.73.
- **At c2,** F spans 7.99–10.46.

**The count each input takes its minimum over** (floor-arm sample rate × span):

| cell | native window | m̂ (10 s) | ℓ̂ (at t = 15 s) |
|---|---|---|---|
| c3 | 60 | ≈ 2 400 | ≥ 3 600 |
| c2 | 282 | ≈ 11 300 | ≥ 17 000 |
| c1s | 5 218 | ≈ 209 000 | ≥ 313 000 |

R6.7 candidate C is the median of 4 native minima, so it carries the native count.

---

## 3. The law's quantities and the candidates against stability (`crrule.out` §1, §2b, §3, §4)

**(a)** Is it the law's quantity? **(b)** Its count drift, or (for F, which has no count) its spread across rows. J = 0
has three rows: the pair and D12, which is dump-on.

| reading | (a) | J = 0 | J = 5 (61c / 61a) | c2 | c1s |
|---|---|---|---|---|---|
| **R6.2's F** | **yes** | rows 45.36–46.73 | rows 37.22–46.22 / 42.91–138.47 | rows 7.99–10.46 | 2.43 (one median) |
| **R6.7 candidate C** (median of 4 native minima) | **yes** | 44.55–44.59; carries the native count (drift as below) | 42.69 / 42.38 | 8.67 | 2.38 |
| the native window minimum (what b̂₀⁺ and C are built from) | the sampler's output | 3.40 (native → 4 200) | 9.52 / 7.34 | 1.95 | 0.00 |
| **m̂** (10 s minimum) | an input of F | 41.26–41.33 on every row (the low mode) | 33.3–36.7 by row (jitter-sampled) | 5.9–6.8 by row | 2.26 |
| minimum over 500 samples (**observation only**) | no | 0.21 / 0.19 (500 → 4 000) | 4.11 / 2.49 | 1.21 | 0.00 |
| per-window p10 | no | 0.22, but it reads the bulk ≈ 5 ms above the queue-free loop | ≤ 0.18 (`knee-margin-rederive` `kmr.out` §10a) | 0.05 | – |

**No reading is both the law's quantity and stable.**
- R6.2's F spreads by 1.4–9 ms across rows (95 ms with 61a r5's start window), through b̂₀⁺'s start-transient maximum.
- C inherits the native count.
- At J = 0, **m̂ is the long-window minimum that reads the queue-free loop on every row,** and it is already an input
  of the law. Its count is set by time (10 s × rate), not by a rule.
- At J = 5, m̂ is a jitter-sampled 10 s minimum, so it moves with the rate as every minimum does.

**The knee side, J = 0 beside J = 5** (drift from 500 to 4 200 samples; `crrule.out` §2b):

| arm | J = 0 drift | J = 5 (61c) drift |
|---|---|---|
| floor W 12 | 0.23 | 4.13 |
| W 80 | 0.63 | 2.56 |
| W 88–120 | – | 2.29–3.16 |
| W 128 | 7.73 | – |
| W 152 | – | 8.42 |

- At J = 5 floor and knee fall together, so an equal-count need is stable (0.2–0.3 ms).
- At J = 0 the floor stops falling and the knee-side arms do not. By §1's mechanism, the knee-side minimum also catches
  loss-repair drains, which are rarer per sample at higher load.

---

## 4. What this predicts for a re-run of s62 at J = 0

- **The floor:** read as m̂ (the law's long-window minimum), or as any minimum over ≥ ≈ 1 s of samples, it sits at
  41.26–41.49 on all three rows. So the floor can be read at J = 0, but its value depends on the loss process (§1).
  - The registered gate (native → G(2 100) < 0.5 ms) would fail again, as designed, because the native window is the
    count at which the loss events are missed.
- **The knee:** its minimum still falls with count at J = 0 (W 80 0.63, W 128 7.73 ms from 500 to 4 200). Existing
  rows cannot bound it at the knee itself.
- **So a re-run could read the knee need only after the knee reading's own count dependence is resolved.** Given §1,
  that dependence is the loss-repair process seen from the operating point. That is the next derivation's question (the
  operating point's minimum), not a VM session's.

## 5. Owner-facing

**None.**
- Whether the law's floor should be anchored on m̂ (a long-window minimum) instead of F's b̂₀⁺ construction is design
  work for review, under R6.2 / R6.7.
- No re-run of s62 is proposed.

## 6. Review log (the follow-up)

**What changed in the follow-up:**
- the Q3 table now leads with R6.2's F and candidate C;
- F's cross-session spread is explained from `slaw.rs:150–165`, with the raw brakes;
- the flight mechanism is tested against W: it holds in part, with about half the offset unexplained;
- the low mode is replicated on R2 / R3 (31 / 29 %);
- the send pauses and low windows are aligned with retransmits: 84–87 % against 5–8 %, and 53 / 53 pauses;
- the flight table is corrected: 1 738 flights, 1 212 of them with ≥ 12 samples;
- the n₀ = 500 proposal is dropped to an observation.

The previous version is `b2f89645`.
