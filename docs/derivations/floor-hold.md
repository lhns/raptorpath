# H, the floor pass's hold (derivation, no code)

**Question (owner, ADR-0084 Amendment 3 Q-b, answer (iii)).** After the sender's floor F arrives, the receiver waits H before it passes the holes below F, because a fill can still arrive: the verdict's last copy (sent with the floor, Amendment 2), earlier copies still in flight, or the original on a slower path. Today H would be `shed_recv_hold` (`net/shed.rs:58`): `SRTT/2` or `(4·SRTT).clamp(60, 300) ms`, selected by a bit, and UNDERIVED (paper §11.3 "receiver shed hold", rho-step-sites C11). Derive H from what the receiver observes, as one expression with no ρ, δ or hint branch.

**Short answer.** H is the support of how late a packet can arrive behind a packet sent after it. The receiver measures that directly on every data arrival, across all paths, as RFC 4737's reordering late time offset with the sender's timestamp as the send order. H is the windowed maximum of that offset:

```text
   H(t)  =  max { ℓ_i  :  arrival i  with  t − W  ≤  a_i  ≤  t }          (0 when no arrival in the window was late)

   ℓ_i   =  ( a_i  −  min{ a_j : j arrived before i,  j ≻ i } )⁺           (any path; 0 when nothing sent after i arrived first)

   j ≻ i   ⇔   τ_j > τ_i   ∨   ( τ_j = τ_i  ∧  path_j ≠ path_i )             (strict on one path, non-strict across paths)
```

- **H is derived except for one memory length, W**, which is the owner's to accept (§11). The toy simulation sweeps it (`skip-rule-efficiency.md` §6.4).
- It needs **no wire field**. The floor is itself a packet sent after every fill it waits for, so its arrival time is the reference. The sender's stamp is not needed, and neither is the path of the last copy.
- It answers both of Fable's objections to `ordering-hold.md` §8. It needs no per-seq path map: it bounds the lateness of *any* earlier-sent packet behind *any* later-sent one, on any path. And it comes down: the maximum runs over a sliding window W.
- **W = 10 s**, the engine's declared path-property window (Copa's RTT-floor window, `scheduler/copa.rs:641`), cited from Copa and BBR. It is the one declared choice in the law, and it carries an age gauge.
- **δ does not enter.** H is a support bound, not a priced optimum (§4).
- **At N = 1 on a FIFO path, H = 0.** The copy is staged before the floor on the same path, so it arrives first. That holds exactly in the sim, whose links do not reorder. At N = 2 with path skew Δ, H ≈ Δ plus the jitter spread. At ρ = 1 no floor is sent, so H is never read.
- Every step-1 test (T1, T2, T1′) is N = 1 in the sim, so H_max = 0 there: tighter than `SRTT_max/2`.

**Inputs read** (`github/main` `89e584e0`):
- ADR-0084 and Amendments 1–3;
- `ordering-hold.md` §5, §8 and §9; `skip-rule-efficiency.md` §1, §3.1 and §6; `adr84-engine-brief.md` steps 1, 4 and 8; `rho-step-sites.md` C11 and §8;
- paper `docs/fec-arq-model.md` §5.1 (the floor marker), §5.6 and §7.3;
- `net/shed.rs:58–78` (`shed_recv_hold`); `net/receiver.rs`: `:808` (every message comes with its `path_id`), `:1148` and `:1225–1280` (per-batch `send_timestamp_us`, the arrival stamp, `record_arrival`, `[LAT]`);
- `net/lat.rs:1–60` and `:303–316` (`A_x` against an all-time minimum gap); `net/reorder.rs:69–92` (a fill below the frontier is delivered at once);
- `control/estimator.rs:136–152` (RFC 3550 jitter);
- `transport/io_owner.rs:540–548` and `:1499–1530` (one owner channel per path; data lane flushed before control; "a control datagram never overtakes the data staged before it");
- `transport/protocol.rs:105` (`PROTOCOL_VERSION = 10`: v11 is not yet built); `transport/quic.rs:944` (`datagram_send_buffer_size` = 4 MiB);
- `src/sim.rs:26–32` and `:78–86` (the sim link: 10 ms delay, uniform jitter up to 4 ms, 500 kB queue, 40 Mbit/s, and jitter that never reorders); `tests/sim_support/mod.rs`, `tests/sim_lost_tail.rs` and `tests/sim_shed_reach.rs` (all N = 1).
- RFC 4737 §4.3 (reordering late time offset; text checked).

## 1. What H must cover

**Per hole, t_F(s)** is the arrival of the first floor with F > s. Floors are re-sent on every live path at each tail-sweep fire, and they rise with each verdict. The marker rule (paper §5.1) is `(F_rx, t + H(t))` at each floor arrival that raises `F_rx`. When a marker expires, the frontier passes to its F. So a hole passes at the earliest expiry among the markers that cover it. A later floor never re-arms a hole that is already covered: its marker can only bring the pass forward, and §2 shows that is still safe.

**The fills that remain after t_F.** Say the floor was sent at sender time τ_F. Any fill of s, from any path, was sent at some τ ≤ τ_F:
- the last copy, at the verdict instant (Amendment 2);
- earlier copies, before it;
- the original, before those.

**The bound.** Take a fill i of s, sent at τ_i ≤ τ_F and arriving at a_i. Let a*(τ_i) be the earliest arrival, on any path, of a packet sent after τ_i. The floor is such a packet (or is sent at the same instant: see ties below), so `a*(τ_i) ≤ t_F`, and therefore:

```text
   a_i  −  t_F   ≤   a_i  −  a*(τ_i)   =   ℓ_i
```

So no fill arrives later than t_F + sup ℓ. One premise: the sampler's references are data batches, because the floor carries no stamp. The floor is queued like the data on its path (below), so ℓ measured against data references stands for ℓ measured against the floor. `fh_miss` (§6) tests that premise. That sup is taken over the distribution of ℓ, which the receiver samples on every data arrival. Two things make this work:
- The bound needs nothing about *which* path carried the fill, and nothing about the floor's own transit. That removes objection 1.
- It needs the floor to behave like a data packet on its path: queued behind every datagram staged before it. That holds today. `TxBatch::flush` hands each path's data lane to its owner before its control lane, on one channel, into one quinn datagram queue (`io_owner.rs:1499–1530`). Two requirements follow for brief step 8 (§8): the copy is staged before the floor, and the receiver processes the floor in arrival order with data.

**Ties and stamp order: strict on one path, non-strict across paths.** `send_timestamp_us` is `now_us()` at staging (`emit_source.rs`, `quic.rs:801`), so batches can share a stamp:
- *In the sim* every virtual timestamp is a multiple of 1 ms (`sim.rs:23–27`), so a whole 1 ms burst shares one stamp. The stamp step there is 1 ms.
- *In the engine* each symbol is stamped at its own `now_us()` (µs apart). A burst is at most `RWM_EMIT_BURST = 64` symbols (`gates.rs:194`), about 15 ms of serialization at 40 Mbit/s with 1 200 B symbols, and consecutive bursts sit at least that far apart.

On one path, equal-stamped batches arrive in FIFO order. Comparing them with `≥` would give ℓ > 0, one serialization gap apart, and break H = 0 on a FIFO path. So on one path the order is strict. Across paths, equal stamps are exactly the last copy and the floor leaving at one instant on two paths. Strict order there would compare a slow-path packet with the *next* stamp step instead: H would be short by up to one stamp step (sim 1 ms; engine up to one inter-burst gap when the copy is the burst's last packet), and those copies would miss. So across paths the order is `≥`. The rule needs the `path_id` every message already arrives with (`receiver.rs:808`).

**Resend stamps are fresh.** Every send site stamps `now_us()` when it builds the batch:
- first send (`emit_source.rs:435`, `src_send_ts_us`), the duplicate (`:517`), the correction (`:879`);
- `serve_gaps`' NACK resend (`sender_phases.rs:1050`), its margin repair (`:1135`), and the generation sends (`:446`, `:548`);
- the tail-sweep resend (`net/mod.rs:3455`).

No resend reuses an original's stamp, so a repair is sampled against its own send instant, and the store's age never enters ℓ. The verdict's last copy is a new send site (brief step 7). It must stamp `now_us()` too, pinned by a test, `last_copy_carries_a_fresh_stamp`. If a resend ever reused the original stamp, every repair would sample as about one RTT late and hold H up for W.

The premise that makes H = 0 on a FIFO path: on each path, stamps are non-decreasing in staging order. That holds while one task stamps and stages each path's data in order. It is witnessed by `fl_late = 0` in the N = 1 sim tests (§7).

**A later marker is safe.** A hole s first covered by floor F₁ (arriving at t₁) can pass early, at a later floor F₂'s expiry `t₂ + H(t₂) < t₁ + H(t₁)`. That is still covered: every fill of s was sent before F₂ too, so §1's bound applies with t₂ as the reference and H(t₂) as the current support. A hole therefore passes at the earliest expiry among the markers covering it. Paper §5.1 and brief step 8 describe a single marker; they take this rule (§10).

**The reference is a send-order statistic.** ℓ_i compares two arrival times on the receiver clock, chosen by an order on the sender clock. So it needs no clock offset and is immune to drift. `lat.rs`'s `A_x` floor is different: an all-time minimum of `arrival − send_ts`, which drifts with the clock pair and never comes back up.

## 2. The estimator

```text
   H(t)  =  max { ℓ_i  :  t − W ≤ a_i ≤ t }                 H = 0 over an empty window
   ℓ_i   =  ( a_i − a*_i )⁺,     a*_i  =  min { a_j : j arrived before i,  j ≻ i }      (≻ as in the short answer)
```

| symbol | meaning | provenance |
|---|---|---|
| a_i | arrival instant of data batch i, any path, read at the receiver loop's `now_us()` (`receiver.rs:1228`) | measured |
| τ_i | the batch's `send_timestamp_us`, stamped at placement (`lat.rs` header) | measured (sender stamp on the wire since before v10) |
| ℓ_i | RFC 4737 §4.3 reordering late time offset, `DstTime(i) − DstTime(j)` for the earliest-arriving j with a later send, with send order read as τ instead of sequence number. Source and repair batches both count: a fill can be either | cited (RFC 4737 §4.3), measured |
| W | window of the maximum: 10 s | **borrowed by analogy, not derived:** the engine's declared path-property window (`copa.rs:641` `window_duration`, a min-RTT window), reused for a max-lateness filter. Its external sources, Copa's RTT-floor window (Arun and Balakrishnan, NSDI 2018) and BBR's 10 s min-RTT filter (draft-cardwell-iccrg-bbr), are **not confirmed here**. Open question (§11) |
| max | the support end, not a quantile (§4: the miss is priced ω, which is unmeasured, and the rent is paid only by holes that are never filled) | declared choice, stated here |

**Computation.**
- Keep the *records*: the arrivals whose τ exceeds every earlier τ. They increase in both τ and a.
- For a batch with τ_i below the newest record, binary-search the first record with τ > τ_i. Its arrival is a*(τ_i).
- The windowed maximum is a monotone deque, or the windowed max filter of Linux `lib/win_minmax.c` (Kathleen Nichols' algorithm, used by BBR).

**Resource bound, outside the law.** Records are kept for W, coalesced to the receiver timer's 1 ms resolution, so at most 10⁴ entries. A reordered arrival whose τ_i is older than the oldest record gets a lower bound only, `a_i − a(oldest)`. That is a clamp, and it is gauged (§6).

**Cold start.** With no sample, H = 0: the floor passes at once. A marker never waits for a sample, so a dead or silent peer cannot stall a floor pass (T4). The cost is one possible miss before the first reordering is seen, and a miss is gauged and recoverable (§4).

## 3. Alternatives weighed, not adopted

- **(i) `shed_recv_hold` as is.** Bit-selected, two underived arms. Rejected by the owner.
- **(ii) `SRTT_rx/2`.** It reads no bit, but it is the wrong kind of quantity: half a round trip is a δ-shaped horizon (b(Realtime) = ½), not a reorder cover.
  - With symmetric paths it over-covers: `SRTT_max/2 ≈ owd_max ≥ Δ`, by about owd_min.
  - It has no spread term, and it under-covers when the forward delay exceeds half the RTT.
  - Rejected by the owner.
- **`ordering-hold.md` §8's path support.** The formula there was `max over paths carrying a copy (owd_p − owd(first floor path))⁺ + in-path reorder spread`.
  - The receiver cannot evaluate "carrying a copy" (objection 1). Taking the maximum over every live path fixes that only if a live set and per-path owds are known.
  - Its observed-maximum estimator never comes down (objection 2).
  - ℓ measures the same quantity, skew plus in-path spread, in one statistic. It needs no path set and no owd.
- **A per-path transit forecast, `max_p(ĝ_p + K·v_p) − min_p(ĝ_p − K·v_p)`.** Here ĝ and v are RFC 6298 or RFC 9002 §5.3 EWMAs of `arrival − send_ts`, and K = 4 as in the RTO or PTO.
  - It is offset-free and cited, but it measures transit *variability*, not reordering. On a FIFO path it gives 2K·v > 0, where the true answer is 0.
  - A queue ramp inflates v. The test bound from declared sim parameters would be `2K·(J + q + dwell) ≈ 7.5 s` in the over-driven T1, which breaks `limit_s`.
  - It needs a live set and freezes on an idle path.
  - Kept as the fallback if ℓ's records prove too costly.
- **Sender-supplied information.**
  - *The last copy's path* in `WindowFloor`: it cannot answer objection 1, because earlier copies and the original may still be in flight on other paths, and the receiver cannot rule them out.
  - *The floor's send stamp*: it is not needed, because the floor's arrival is already the reference (§1).

  Neither field is proposed. v11 is not built (`PROTOCOL_VERSION = 10`), so either could still ride v11 without a v12, but only as an amendment to the accepted decision 4.
- **Where the last copy goes.**
  - *On the floor's (fastest) path:* it does not shorten H, since earlier sends on slower paths stay in the support.
  - *On every live path:* it cuts the holes that are never filled from about ε to about ε^N, and with them H's rent (§4), at a cost of N − 1 extra symbols per verdict. Whether that pays is a δ-priced redundancy question, `(δ/d)·H·(ε − ε^N)` against `(N − 1)·w`. It belongs to the placement law (§5.7), not to H. Not proposed; Amendment 2's "bandwidth of serving once" stands.
- **A quantile instead of the maximum.** It resists one outlier, but its level would be a constant with no provenance. The age gauge (§6) shows whether a stale spike dominates.
- **Reusing ρ as the quantile level** (H = the ρ-quantile of ℓ). It is continuous, but the receiver has no ρ (R1; ADR-0084: the receiver's ρ is none on the wire). Rejected.

## 4. Pricing: why δ does not enter

Per hole passed by a floor, the two costs are:

```text
   rent(H)  =  (δ/d) · H                  paid only by a passed hole that is never filled (all its copies lost)
   miss(H)  =  ω   +   one allowance unit until the fill arrives, after which G_rx falls      per fill arriving after t_F + H
```

- **Rent.** δ/d converts stall seconds into repair symbols (`skip-rule-efficiency.md` §1, paper §7.3). A passed hole that *is* filled stops stalling at its fill, whatever H is. So H's rent falls on the never-filled share π∞, measured as `G_floor` over verdicts. **Caveat:** π∞ ≈ ε assumes independent losses. At N ≥ 2 a path stall both sets a large ℓ, held for W, and produces the verdicts whose copies are lost. So the rent is understated exactly when H is largest. The table below is the independent-loss case. `fh_us_max` beside `G_floor` per episode is the reading that decides.
- **Miss: two cases, set by the decode floor.** After the floor pass, `next_expected = F`, and the next wait prunes below `decode_inert_floor(F) = F − 1 − 2·recv_win_cap` (`receiver.rs:653–670`, `:2236–2237`).
  - **A late delivery**, if `s ≥ F − 1 − 2·recv_win_cap`. The decoder accepts the fill, and the buffer delivers it at once, out of order (`reorder.rs:75–82`). Brief step 4 subtracts it from G_rx, so nothing is lost. What lasts is one out-of-order delivery, priced ω (`ordering-hold.md` §1, unmeasured), plus the sender's `Ab` one too high until the fill, which can deny another verdict meanwhile (second order).
  - **A counted loss**, if the fill is older. The decoder refuses a source seq below its floor (`fec/unified.rs:475`, counted in `below_floor`), the hole stays in G_rx, and the packet arrived but is lost. This needs the floor to have jumped more than two receive windows above s while its fill was in flight: a verdict run longer than `2·recv_win_cap`. It is gauged separately as `fh_miss_refused` (§6) and must read 0.

**Why the support bound, with no δ and no ω.** Only an expected-cost optimum would read δ: release before the support ends when `(δ/d)·E[remaining wait] > ω·P(fill still coming)`. That trade is exactly design U's H_o = ω·d/δ (`ordering-hold.md` §3.2). It is measured from t_detect, and it composes with H as `min(t_detect + H_o, t_F + H)` in the release-rule build (`ordering-hold.md` §8). Pricing it a second time inside H would double-count δ, and would need ω, which is not measured. So H is left as the honest support end: "A_rx may pass the hole once no fill can still arrive" (`ordering-hold.md` §8). The window W and the choice of the maximum are coverage choices, not prices. Their errors are measured (§6), not optimised.

**In this build there is no H_o.** Where does the unpriced hold land?
- **Matched ρ < 1 receivers** keep the old expiry (Amendment 3 item 4). It passes a hole with data above it at its own hold from the push, which usually fires before the floor arrives. So at Realtime the floor pass mostly resolves lost tails, which have nothing behind them to stall.
- **ρ = 1 receivers** (the R1 cells, sender ρ < 1): the floor pass is the only release. At N = 1 it costs nothing (H = 0). At N = 2 with skew it holds a verdict hole whose fill rides the slow path for up to Δ. At a Realtime δ that is rent `(δ/d)·Δ` where U would have paid ω. This is stated, and it is lifted by H_o in the release-rule build.

**Numbers.** Realtime δ = 50, d = 11 ms (`ordering-hold.md`'s example), π∞ = 0.025 (the sim's link loss). The rent is per verdict:

| cell | derived H | rent | (ii) SRTT/2 | rent | (i) at a ρ = 1 receiver | rent |
|---|---|---|---|---|---|---|
| N = 1, 10 ms RTT, FIFO | 0 | 0 | 5 ms | 0.57 | 60 ms | 6.8 |
| N = 2, owd 5 / 25 ms | ≈ 20 ms + spread | ≈ 2.3 | 25 ms (SRTT_max/2) | 2.8 | 60 ms | 6.8 |

At Bulk (δ = 0.005) every entry is 10⁻⁴ of these. Misses:
- derived: only past the window's observed maximum;
- (ii): none at N = 1; at N = 2, every slow-path fill when the forward delay exceeds half the RTT;
- (i): none at 60 ms.

The derived H is not a large saving. It is right by construction at every N, it is 0 where FIFO makes it 0, and every symbol in it has a provenance.

## 5. Behaviour

- **N = 1, no in-path reordering** (real paths, and the sim exactly): ℓ ≡ 0, so H = 0. The floor passes at its arrival, and the copy has already arrived or been lost. Paper §5.1's "per-path FIFO makes every earlier seq arrive or be lost first" becomes the law's value, not an assumption beside it.
- **N = 1 with in-path reordering** (for example netem jitter without rate, or Wi-Fi): H = the largest late time offset seen in the last 10 s.
- **N = 2, skew Δ, both paths used:** slow-path batches arrive about Δ behind fast-path batches sent at the same instant, so H ≈ Δ + the jitter spread. That is `ordering-hold.md` §8's value, obtained without its path map.
- **N = 2, slow path unused:** no slow-path batch arrives, so H ≈ 0, and nothing is in flight there to wait for. H follows placement by itself. Realtime best-path today is *not* this case: its hint-keyed duplicate send puts batches on the alt path (`emit_source.rs:517`, `ordering-hold.md` §5). Those duplicates are real fills and sample ℓ ≈ Δ, so Realtime H ≈ Δ at N = 2 already. C1's striping (kept in the sender build) keeps it there.
- **A frozen idle leg (T5), a dead path, or a path that leaves:** H does not read any live set. Its samples age out within W. A copy placed on a leg that has been idle for longer than W, and that arrives late, is a miss: gauged, delivered, subtracted from G_rx.
- **ρ = 1:** no verdict, so no floor and no marker. The sampler runs at every ρ (always computed, never read at ρ = 1), so nothing branches.
- **Presets:** H reads no δ, no ρ and no hint. It is constant along both dials, so the ±2 % nudges give identical values, and behaviour continuity is the floor's own, which is the sender's continuous verdict law.
- **Mixed pairings:** the receiver's configuration never enters H, so it is the same function at every receiver.

## 6. Gauges (every term that can bind)

On `[ACCT-RX]`, with every value printed beside its count (`-` when the count is 0):
- `fh_n`, `fh_us_sum`, `fh_us_max`: the H used at each floor pass.
- `fh_zero`: the share of floor passes with H = 0. Predicted 1 at N = 1 in the sim. Not a clamp: H = 0 is the max of a set of zeros.
- `fl_n`, `fl_late`, `fl_max_us`: sampler arrivals, the share with ℓ > 0, and the largest ℓ.
- `fh_age_us`: the age of the sample that sets H, at each pass, p50 and max. This is W's bind gauge. An age near W at most passes means a stale spike dominates H, which is a finding.
- `fl_trunc`: ℓ samples cut at the record horizon (§2's clamp). Must read 0.
- `fh_miss`, `fh_miss_us_max`: fills of floor-passed holes and their lateness past `t_F + H`. This is the empirical test of the law. It is **scored** in the guard's floor-only stages (sender ρ < 1, receiver ρ = 1), as a decision bar, not a law constant:
  - at N = 1, the stage fails on any `fh_miss > 0`;
  - at N ≥ 2, it fails if the one-sided 95 % upper bound of `fh_miss / floor-passed holes` exceeds the cell's 1 − ρ. A miss holds an allowance unit until its fill, so misses at the allowance's own rate would spend it twice.
- `fh_miss_refused`: misses the decoder refused (§4). Must read 0.
- `fh_fill_first`: markers whose holes were all filled before expiry, against markers that passed at least one hole. It shows how often H binds at all.

## 7. Tests

**Absolute tests of H** (the function takes N implicitly through the arrivals; no δ or ρ argument exists to sweep):
- N = 1 FIFO arrivals give H = 0;
- one batch arriving 3 ms behind a later-sent one gives H = 3 ms, back to 0 after W;
- N = 2 with a 20 ms skew and both paths carrying traffic gives H within [20 ms, 20 ms + jitter amplitude];
- N = 2 with the slow path idle gives H = 0;
- an empty window gives H = 0, and a marker set before any sample is finite;
- a later floor never delays a hole that an earlier marker covers.

**A signature test** pins that the H function and its sampler take no `ProtocolHint`, δ or ρ. That is the NO-MODE-SWITCH form for a law with no dial input.

**Step-1 bounds.** H_max is closed from the sim's declared link: links are FIFO (`sim.rs:30`, "jitter does not reorder"), and every path has the same configuration.
- **N = 1, which covers T1, T2 and T1′:** the copy is staged before the floor on the one FIFO link, so ℓ ≡ 0 and **H_max = 0**. That is below `SRTT_max/2` (about 12 ms paced, about 422 ms at the over-driven 844 ms RTT), so every deadline is tighter and `limit_s` is unchanged. Each test also asserts `fh_us_max = 0`, which is the absolute witness that the sampler and the FIFO premise both hold.
- **N = 2 (T5, sim_mixed_rho at N = 2):** ℓ is bounded by the cross-path transit difference. On identical links that is `H_max ≤ J + q + d`, with:
  - J = 4 ms of jitter;
  - q = `queue_bytes·8/rate_bps` = 100 ms of link queue;
  - d = the 4 MiB quinn datagram buffer over 40 Mbit/s ≈ 839 ms of sender dwell (`quic.rs:944`), which is also the source of the over-driven 844 ms RTT.

  That gives H_max ≤ 4 ms paced, which is below SRTT/2, and ≤ 943 ms over-driven, which is above SRTT_max/2 ≈ 422 ms. Over-driven N = 2 cells must budget that in `limit_s`.

## 8. Wire and implementation requirements

- **Wire: nothing.** `WindowFloor { floor: u64 }` is unchanged, and v11 is as accepted.
- **Sender (brief steps 7–8):** on each path, the verdict's copy is staged before the floor, in the same `TxBatch` or an earlier one. The floor rides the datagram data channel's control lane, never a path that overtakes data. The shutdown floor on the reliable stream may arrive late, which only makes H conservative.
- **Receiver (step 8): the floor is handled only after every earlier-arrived datagram has been ingested.** Arrival order alone is not enough.
  - On one path that holds today. One owner reads the path and forwards its inbound datagrams as one `Vec` per poll, in order (`io_owner.rs:22–27`), and the receiver pops `(path_id, msg)` in order and ingests each data batch synchronously in its `Data` arm (`receiver.rs:808`, `:1147`).
  - So `WindowFloor` must be handled in the same loop's `Control` arm (`receiver.rs:2176`), with t_F read there by the same clock as a_i. It must never go through the control fast path (`net/tasks/control_fastpath.rs`).
  - Across paths, a copy still in another owner's channel when the floor is popped is not a premise break. a_i is the loop's processing instant (`receiver.rs:1228`), so that ingestion skew is sampled into ℓ like any other lateness.
  - Owed loopback test on the real I/O threads (`tests/common/loopback.rs`, not the sim), `floor_never_overtakes_its_copy_real_io`: forced verdicts whose copy is not dropped, at N = 1 and N = 2; assert `fh_miss = 0` at N = 1 and within §6's bar at N = 2.
  - The sampler is fed at the `record_arrival` site (`receiver.rs:1245`) for every data batch, source and repair.
  - The marker is `(F_rx, t + H(t))`.
  - It is I/O-free per-tick state with no hint, δ or ρ, so it is a class-A candidate for `raptorpath-core` later (ADR-0077). Not required here.

## 9. NO-MODE-SWITCH

| quantity | in δ | in ρ | hint | edges |
|---|---|---|---|---|
| ℓ_i, H | not read | not read; reached only through t_F, since no floor exists at ρ = 1 | not read | empty window: 0; a FIFO single path: 0 |
| min over covering markers | — | — | — | every marker's expiry is always computed |

## 10. Owed edits, prepared on this branch (they merge in the same push)

- **Paper, FORMULA-FIRST.** §5.1's line "H_hold: the receiver's existing hold, `shed_recv_hold` (Section 5.6); no new constant" is replaced by §2's formula with its provenance table and §1's bound. §5.6's "waits the receiver's hold H_hold" is left as it is.
- **§11.3.** The "receiver shed hold" row stays for the expiry (C11) only. Add a register row for W = 10 s (borrowed, sources to confirm), and the record horizon as a resource bound with `fl_trunc`.
- **`rho-step-sites.md` §6 C11 row:** "UNDERIVED for H's kind" becomes "the floor pass's H: a reorder cover, the support of the late time offset (`floor-hold.md`)". The expiry's hold stays UNDERIVED until the release-rule build.
- **The marker rule (paper §5.1, brief step 8):** several markers, one per floor rise; a hole passes at the earliest expiry among those covering it (§1).
- **`adr84-engine-brief.md` step 8:** H, the §6 gauges, the §7 tests, H_max = 0 in T1, T2 and T1′, and the §8 requirements. ADR-0084 Amendment 3 item 3 records the derived H.

## 11. For the owner

In plain words: after the sender says "skip everything below F", the receiver waits only as long as packets on this connection have actually been seen arriving late. It measures that as how far behind a later-sent packet an earlier one has come in, over the last 10 s. On one path with no reordering that is zero, so the skip takes effect at once. On two paths it is about the difference in their delays. Nothing is added to the wire, and the wait does not depend on the protocol setting.

Two things to know first:
- **The derived H can be longer than today's hold.** On two over-driven paths, §7's bound allows about 0.94 s, against today's 60–300 ms. It is that long only when packets really arrive that late, and only packets lost for good wait the full H.
- **One late spike holds H up for W.**

Genuine choices:
1. **The 10 s memory (W): the one part that is not derived.** It is the engine's existing path-memory window (a minimum-delay window) reused here. Confirming its Copa and BBR sources would not make it derived for this use. The toy simulation sweeps W ∈ {1, 10, 60 s} for miss rate and stall (`skip-rule-efficiency.md` §6.4); the owner accepts a value. Longer remembers a rare late spike for longer: more stall, but only on packets that are lost for good. Shorter forgets a quiet slow path sooner: more late-but-delivered packets. Neither can be priced until ω is measured; the gauges show which way it errs. **Recommended: 10 s.**
2. **Maximum, not a percentile.** A late-but-delivered packet costs ω (unmeasured), and holding costs only for packets that never come. So cover everything seen. **Recommended.**
3. **Interim cost at a ρ = 1 receiver on two skewed paths at the Realtime δ:** a skipped packet whose copy rides the slow path waits up to the skew instead of being released. This goes away with design U's release rule.
4. **Not proposed:** sending the last copy on every path. That is a placement-law question.

Recorded, not decided: sending the last copy on the floor's own path, with a per-path H, would shorten the two-path hold. It trades stall against ω, so it waits for the ω measurement (`meas-disorder-price`).
