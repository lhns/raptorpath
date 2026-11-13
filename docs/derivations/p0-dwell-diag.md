# p0-dwell-diag: why lever B triples p0's quinn send backlog at c8

Status: DIAGNOSIS from logs only, with no engine change and no VM contact. Feeds: status §53, open
item 1 (`rtp_floor_p0`) and the `cand52` merge question.
Evidence: `docs/l1-raw/s53/rep53-diag-ld.tgz` (c8-100 at 600 MB; BASE, BASES, CAND, CANDS and CW0, 6 rows
each), cross-checked on `docs/l1-raw/s52/rep52-diag-ld.tgz` (BASE, CAND, W0, W05 and WDER, 6 rows each).
Scripts: `p0-dwell-diag/` next to this file. They are stdlib Python and take the extracted `diag-ld`
directory as their argument:
- `budget.py` (all ticks) and `budget2.py` (PRE/PIN phases): the population budget and the §1 dwell
  table;
- `holes.py`: receiver holes, `[LAT]` and `[SUCC]`;
- `little.py` and `inorder.py`: §5;
- `rows.py`: per-row field medians.

The `[DCAP]`, `rtp_us` step-time, `full`, `np_act`, `[QCLK]` and P38ROW-share numbers come from
grep one-liners over the same logs, not from these scripts.

## 0. Answer in one paragraph

The c8 sender is store-gated (`paused` ≈ 80 %). Its counted store is clamped at the ceiling
N·pool = 2 × 2048 = **4096** from t ≈ 10.3 s on every row of every arm. The counted store is the seqs
the receiver has not yet reported received. Both legs are link-saturated, at the same transmit rates
on every arm. So the 4096 slots form a **fixed population**, split between seqs in transit and holes
waiting for repair. Under the 20-gap horizon, about 1 250–1 500 of those slots hold holes older than
about 1 s. Lever B repairs those holes before they age (old holes → 0), so the freed slots refill with
new source. The links cannot carry it any faster, so it queues. On p0, quinn's BBR window admits only
about 1 000–1 500 datagrams to the wire, and the rest waits in quinn's per-connection datagram send
buffer (`dgq0 hand − tx`). The app-echo RTT carries that dwell, and it matches `q0 / tx rate` within
a few ms. Copa `min_rtt` is a 10 s minimum of the app-echo RTT (`RWM_COPA_WIRE` is off), and the queue
never drains within 10 s, so `min_rtt` takes the queue in too. Nothing hands p0 more *rate*; the
change is a *level* of the same pinned population moving from "holes" to "queue". The wire RTT does
not change because quinn's own window, which sets it, does not change. Retransmit volume, placement
share and quinn's drain rate are all ruled out (§3). The inflated `min_rtt` feeds the store cap's law,
which therefore asks 3–10× the ceiling. That is a positive loop, but the clamp cuts it (§4). The law
pinned at its clamp is a defect finding (§4.1). Measured in-order delivered delay *improves* about 5× under
lever B, from a mean of about 5.3 s to about 1.0 s. Under the 20-gap horizon about 50k
received seqs wait behind old holes, so the +200 ms of p0 transit is small beside it. `[LAT]`'s
eviction bound hid this on the 20-gap arms (§5).

## 1. The quantities

| field | what it is | source |
|---|---|---|
| `dgqN[hand= tx= full= err= sp=]` | cumulative per path. `hand` = `send_datagram` calls quinn accepted (counted at the I/O owner); `tx` = quinn's `stats().frame_tx.datagram`, the DATAGRAM frames actually sent; `full` = handoffs that found the 4 MB send buffer full (eviction predicate); `sp` = buffer space at the last handoff, in bytes | `transport/io_owner.rs` `DatagramQueueAudit`, `send_one`; `transport/quic.rs:558` |
| `hand − tx` (also `dgevN`) | datagrams accepted by quinn and not yet transmitted: queued in quinn's per-connection send buffer **plus** any evicted (cumulative). `sp` is the space at the *last handoff*, not a drained flag. At the last `[DIAG]` line, `hand − tx` equals `full` on 13 of 18 lever-B rows; the other 5 last lines still hold a queue (e.g. CAND s42 r3, `sp` 900 KB). Evictions: **0 on all 12 20-gap rows** (`full = 0`, `err = 0` on the last line; end `hand − tx` ≤ 11, on p0 or p1, still queued). **Up to about 1 000 per run on lever-B p0**: CAND 0–299, CANDS 0–336, CW0 6–996 (`full`). That is ≤ 0.23 % of p0 handoffs, and against a standing `q0` of 1.7–2.9k it is small, so `hand − tx` reads as the **queue** to within a few percent. `sp` agrees: `4 194 304 − sp ≈ (hand − tx) × 1 233 B` (CAND s42 r1, t = 40.6 s: 2 633 queued, 3.25 MB used). The `tx` snapshot is refreshed every 1 ms of owner activity (`VIEW_REFRESH`), so staleness is ≤ about 10 datagrams | ″ |
| `pN:rtt=` | the engine estimator's EWMA of the **app-echo** RTT: echo time minus `send_timestamp_us`, which `emit_source` stamps at the placement lock, *before* the owner channel and quinn's buffer (`net/lat.rs` header; `control_msg.rs:291/504`) | `diag.rs:549` |
| `pN:wrtt=` | quinn's smoothed path RTT (`conn.stats().path.rtt`), the wire clock. It includes netem's queue but not quinn's send buffer | `diag.rs:499`, `io_owner::publish` |
| `pN:rtp_us=` | Copa `min_rtt`: the windowed minimum over **10 s** (`window_duration`, `copa.rs:641`) of the samples fed to `record_rtt_sample`. With `RWM_COPA_WIRE` unset (`[GATES]`: `copa_wire=false`), those samples are the **app-echo** RTT (`control_msg.rs:575-581`) | `copa.rs:1008`, `path.rs:1070` |
| `win=a/b` | `a` = counted store `store_len` = retained − released-for-gate, where released = `max(SACK marks, received_above − not_retained)`. That is the retained seqs the receiver has not reported received. `b` = the effective store cap. `tx_paused` ⇔ `a ≥ b` (the in-flight brake is unarmed: `(cap0)`, `RWM_LATE_BRAKE=0`, `RWM_INFL_CAP=0`) | `mod.rs:2223-2393`, `sack.rs:387-414` |
| `srel=c/t gate=g` | `c` = SACK-marked slots; `g` = the gate's released count `max(SACK marks, received_above − not_retained)`, the quantity `store_len` subtracts. So retained = `win.a + g` = the sent store above the receiver's cumulative point. On 20-gap rows `g` ≫ `c` (BASE s42 r1, PIN phase: `g` 28–75k against `c` 4–26k); on lever B they are about equal | `diag.rs` srdiag, `sack.rs:387` |
| `qcwnd=` | quinn's BBR window in bytes; ÷ 1 233 B gives datagrams | `quic_path_stats` |
| `sinfl` | source in flight under the Copa feed; 0 here (`RWM_COPA_FEED=0`) | |
| `btlbw`, `bdp` | Copa `max_bw` (windowed max of ack-batch rates, sym/s) and `max_bw × min_rtt` | `copa.rs:1461` |
| server `[RANK] holes=… tail_overcount=…` | open holes in `[next_expected, seen_end)`, and the holes in the span that arrived since the previous readout (about 1 s earlier). `holes − tail_overcount` = **holes older than one readout** ("old holes") | `receiver.rs:2255` |

**Where the dwell is.** Per row, the median of `rtt0 − wrtt0` (app-echo excess over wire) against
`q0 / tx0_rate` (quinn send-buffer dwell), over all ticks with t ≥ 2 s and src > 0 (`budget.py`):

| arm | `rtt0 − wrtt0` ms | `q0/r0` ms |
|---|---|---|
| BASE | 64 [24–98] | 66 [22–97] |
| BASES | 64 [27–140] | 62 [30–140] |
| CAND | 222 [178–274] | 227 [180–281] |
| CANDS | 166 [80–261] | 166 [78–267] |
| CW0 | 259 [207–271] | 282 [206–299] |

On 24 of 30 rows the two columns agree within 6 ms. On CW0 the queue dwell exceeds the echo excess
by up to 35 ms. A likely reason, not checked: CW0 carries 83 % of retransmits on p0, and an echo of
0 produces no RTT sample (`control_msg.rs:569`). If retransmits carry that sentinel, they occupy the
queue without being sampled. **The extra dwell is in quinn's p0 datagram send buffer.** The
engine's I/O-owner channel adds about 0: `[IOWN]` shows `fl_full=0` and `cmdq_n=0`, and the identity
leaves no residual for it. p1 has no quinn queue (`q1` ≈ 0–8). Its in-flight sits below its BBR
window, and its queue is in netem (`wrtt1` 181–445 ms on a 40 ms path).

## 2. The mechanism: a pinned population, rebalanced

### 2.1 The cap

The shipped seat (`sender_phases.rs:191`, `store_cap.rs:76`) is:

    cap = clamp( (1 + q(δ)) · Σᵢ max_bwᵢ · min_rttᵢ ,  floor ,  N · pool )

At c8, (1 + q) = 1.1 (`[DCAP] q=0.1`), N = 2 and pool = `RWM_STORE_PATH_POOL` = 2048, so the
ceiling is 4096. `[DCAP]` per arm, over the whole run:

| arm | `ask` (mean unclamped law) | mean realized `cap` | `pin` (bind fraction) |
|---|---|---|---|
| BASE | 12.4k–25.5k | 3 481–3 791 | 0.74–0.76 |
| BASES | 12.8k–19.4k | 3 541–3 793 | 0.74–0.86 |
| CAND | 30.2k–43.0k | 3 669–4 041 | 0.77–0.98 |
| CANDS | 26.5k–36.0k | 3 603–3 892 | 0.77–0.89 |
| CW0 | 29.4k–41.3k | 3 435–3 780 | 0.76–0.78 |

The unpinned share is the start-up phase plus BASE's tail. After the first tick at cap = 4096, and
while src > 0, the cap reads 4096 on 4 206 of 4 209 ticks over the 30 rows (PIN phase below). On
30 of 30 rows the p0 `rtp_us` more than doubles from its start-up value (8.4–14.2 ms) within one
tick at t = 10.3–10.5 s. The doubling is the detector's threshold; the first value past it is
2.4–5.0× the start. This is the
moment the start-up sample leaves Copa's 10 s window. On 22 of 30 rows the cap first reaches 4096 on
that same tick. On 6 rows it gets there earlier, at 1.0–9.0 s, while `rtp_us` is still at its
start-up value. On 2 rows it gets there within 0.7 s after (step listing in §6).

### 2.2 The budget

Counted store = sent and not yet reported received:

    win  =  Σᵢ rᵢ · rttᵢ  +  H                  (Little, per leg; H = holes older than ~1 RTT)

Here rᵢ is leg i's transmit rate (`Δtx/Δt`) and rttᵢ the app-echo RTT. A seq's report reaches the
sender one app-echo RTT after its stamp. p0's population further splits as r₀·rtt₀ = q₀ + r₀·wrtt₀
(quinn buffer + wire). Medians of per-row medians over the PIN phase (cap = 4096, src > 0), with row
ranges (`budget2.py`):

| arm | r₀ /s | r₁ /s | q₀ | wire₀ = r₀·wrtt₀ | qcwnd₀ (dgram) | **path₀ = r₀·rtt₀** | path₁ | **win − Σ rᵢ·rttᵢ** | old holes | all open holes | `rtp0` ms |
|---|---|---|---|---|---|---|---|---|---|---|---|
| BASE | 9 855 [9 816–9 894] | 1 947 [1 923–1 976] | 767 [246–1 039] | 1 242 [1 012–1 570] | 1 270 | **2 020** [1 896–2 228] | 526 [427–604] | **1 556** [1 286–1 693] | 1 408 [1 178–1 467] | 2 106 [1 881–2 292] | 116 [76–117] |
| BASES | 9 866 | 1 969 | 758 [356–1 542] | 1 224 [710–1 619] | 1 255 | **2 057** [1 884–2 291] | 479 [336–637] | **1 464** [1 257–1 733] | 1 243 [1 089–1 510] | 2 179 | 104 [71–174] |
| CAND | 9 855 [9 844–9 888] | 2 001 [1 967–2 019] | 2 316 [1 772–2 823] | 1 243 [838–1 503] | 1 281 | **3 447** [3 175–3 569] | 650 [517–908] | **−8** [−19–3] | 0 [0–2] | 916 [527–1 506] | 272 [187–328] |
| CANDS | 9 876 | 1 997 | 1 747 [903–2 642] | 1 579 [887–2 205] | 1 609 | **3 346** [3 101–3 497] | 731 [558–935] | **20** [7–50] | 0 [0–1] | 752 | 231 [166–252] |
| CW0 | 9 862 | 2 003 | 2 896 [2 074–3 069] | 1 093 [950–1 481] | 1 116 | **3 663** [3 564–3 742] | 428 [392–585] | **−40** [−55–16] | 0 [0–0] | 1 634 | 299 [267–307] |

Reading the table:
- **Lever-B arms (18 / 18 rows):** the path population equals the cap within −55 to +50 (≤ 1.3 %),
  and holes older than one readout are 0–2. The whole population is in transit.
- **20-gap arms:** the residual is 1 257–1 733. On **12 of 12 rows**, per row, old holes ≤ residual
  ≤ all open holes; e.g. BASE s42 r1: 1 178 ≤ 1 286 ≤ 1 881, and BASES s7 r3: 1 116 ≤ 1 257 ≤ 2 280.
  The bracket's gap is holes aged between one RTT and one ~1 s readout. That bracket is all the data
  allows; the holes are measured on the server's ~1 s cadence, aligned to the client only
  approximately.
- **The transfer, BASE → CAND:** H falls by ≈ 1 560, path₀ rises by ≈ 1 430 and path₁ by ≈ 120. The
  population is conserved within the medians' resolution.
- **Where p0's increase lands:** `wire₀ ≈ qcwnd₀` on every arm (quinn is cwnd-limited on p0), so the
  increase goes entirely to `q₀`. quinn's window sets the split between buffer and wire, not the total.
  This is why CANDS shows a lower `q₀` and a higher `wrtt0` than CAND at the same path₀. Its rows ran
  with a larger qcwnd. The brief's ranking "CANDS backlog < CAND" is a quinn-window difference.
  path₀, the quantity lever B changes, is the same within the spread.

**Second operating point (PRE phase, 2 s ≤ t < 10 s, cap < 4096 set by the law).**
- The same identity holds at a lower population: lever-B residuals are −59 to +111 with old holes
  0–14; 20-gap residuals are −16 to 595 with old holes 0–472.
- Across CAND's rows, `q0` follows the cap: cap 2 042 → q₀ 376, 2 678 → 720, 2 912 → 938,
  2 915 → 1 134, 3 790 → 1 708.
- **A natural experiment:** CAND s42 r1's cap reaches 4096 at t = 1.0 s while its `rtp_us` is still
  8.4 ms. Its `q0` is already 1 370–1 532 at t = 4–8 s. **The backlog follows the cap, and the
  `min_rtt` rise is a consequence, not the cause.**

**§52 replicates it on other binaries (PIN phase).**
- 20-gap arms: residual 1 536 (BASE), 1 786 (W0), 1 703 (W05), 1 602 (WDER); old holes 1 310, 1 568,
  1 528, 1 324.
- CAND: residual −20 [−28–11], old holes 0; path₀ 3 382 against BASE's 2 066.

### 2.3 Why the wire RTT is unchanged and the app-echo is not

`wrtt0 ≈ qcwnd₀ / r₀` is set by quinn's BBR window on p0, and that window does not depend on the
gap horizon: qcwnd₀ is 1 116–1 609 datagrams on every arm. The app-echo adds the quinn buffer dwell
`q₀ / r₀`, which carries the whole population change. Copa's `min_rtt` is the minimum of the
app-echo over 10 s. Under lever B, `q0` never drains within a 10 s window (PIN medians 1 747–2 896),
so `min_rtt ≈ wrtt_min + q0_min / r₀` = 187–328 ms. Under the 20-gap horizon `q0` touches about 0
more often (e.g. BASE s42 r1 at t = 24.3 s: q0 = 16). The floor there is mostly quinn's own netem
queue: `wrtt0` 102–157 ms on a 10 ms path.

## 3. Candidates

| | candidate | verdict | evidence |
|---|---|---|---|
| (a) | faster hole repair releases store capacity, so more source is admitted | **confirmed, as a level, not a rate** | §2.2: under the pinned cap, H (old holes) 1 243–1 408 → 0 and path₀ +1 400. Admission *rate* = drain rate on every arm: the `hand0` − `tx0` rate is −15 to +6 /s on 20-gap rows and 0–69 /s on lever B. That is the standing queue's slow growth over 40 s plus evictions (§1), not a mismatch. The release path is the counted gate, which already counts every reported-received seq. `RWM_STORE_SACK_RELEASE` itself is identical on both binaries |
| (b) | placement shifts source toward p0 | **ruled out** | P38ROW `shares[0]`: BASE 0.822–0.828, BASES 0.819–0.828, CAND 0.817–0.820, CANDS 0.819–0.824, CW0 0.817–0.821. Hand rate p0 9 811–9 926 /s on every arm |
| (c) | retransmit volume or placement on p0 | **ruled out as the cause; it moves population between legs** | p0 retransmits per GB (status §53 means × (1 − leg-1 share)): BASE ≈ 10.4k, BASES ≈ 10.0k, CAND ≈ 8.0k, **CANDS ≈ 6.8k**, CW0 ≈ 30.7k. CANDS puts *fewer* retransmits on p0 than BASE and still has path₀ 3 346 against 2 020. Its old holes are 0, as on CAND: the filter removes spurious fires, not lever B's repair speed. The brief's claim holds. CW0 moves population from p1 (path₁ 428 against CAND's 650) to p0 (3 663 against 3 447) at the same total |
| (d) | quinn on p0 drains slower | **ruled out** | `tx0` rate 9 807–9 912 /s on every arm, equal to the link (≈ 100 Mbit at 1 233 B). qcwnd₀ is not lower on lever B (1 281 against 1 270). It sets the buffer-versus-wire split and the within-arm spread of `q0` |
| (e) | the counted-store clamp is binding and the links are full | **the frame (a) works in** | `paused` 78–80 %, `win = cap = 4096` in the PIN phase on every row, `ask` 3–10× the ceiling (§2.1). With an unbound cap or an unsaturated link the population would not be conserved, and lever B's freed slots would not have to queue |

## 4. Who consumes the inflated RTTs, and the loops

**Copa `min_rtt`** (`rtp_us`: 68–174 ms on 20-gap arms, 166–328 ms on lever B, on a 10 ms path):

| consumer | use | effect of the inflated value | live here? |
|---|---|---|---|
| `bdp_anchor = max_bw · min_rtt` → store cap Σ (`store_cap_pool_inputs`) | the cap's law | `ask` 12–43k against the 4096 ceiling; the cap is pinned. **Positive loop:** queue → `min_rtt` → ask → pinned cap → population → queue. **Cut by the clamp:** in the PIN phase the population is N·pool, not the law. The loop latches the pin when the start-up sample leaves the window (t = 10.3 s, §2.1) and keeps the law above the clamp. It does not make the BASE/CAND difference; that is the hole/queue split under the same ceiling | yes |
| `anchor_floor = 0.85 · bdp_anchor` → Copa cwnd | raises cwnd (never caps) | `cwnd=` 10–20k against `infl=` 0.1–1k. A path is almost never saturated: `np_act = 2` on every
pinned tick of BASE and CW0, and on 683 of 696 pinned ticks of CAND. cwnd enters only placement's queue term `(I/W)·s` and the `active_paths` filter, so p0's queue term reads ≈ 0. **Not a gate** (brake unarmed) | weakly, via placement |
| Copa delay signal (app-echo mode: window-min − `queue_floor`, a quantile of window mins ≥ `min_rtt`) | the cwnd direction | the standing queue enters the floor, so Copa reads "no queue" and cwnd ratchets up. Irrelevant while cwnd does not gate | inert |
| rate-sample `min_interval` and `max_bw` filter window `10·RTprop` ∈ [1, 10] s (`copa.rs:882, 918`) | `max_bw` | the window is 1.0–1.7 s on 20-gap arms and 1.7–3.3 s on lever B. The `max_bw` over-read (110k–240k sym/s against a 9.8k/s link, 11–24×) is not caused by this (a longer `min_interval` rejects more aggregates, not fewer); it is a separate input defect into the same `ask` | yes |
| `eta.on_ack(…, rtprop_us)` (`control_msg.rs:598`) | the `[ETA]` gauge, `e = rtt − (eta + RTprop/2)` | gauge only. The inflated RTprop/2 subtracts 50–160 ms from `e` and masks part of the prediction error | gauge |
| `three_term_terms` / `span_forms` `rtp_min` (`store_cap.rs:471`) | three-term cap | `RWM_THREE_TERM=0` | no |
| `gen_pipe_depth(channel_max_rtprop_s)` | generation cap | `plain_dyn_cap` wins | no |
| `publish_tx_view` → receiver `live_tx_views` | receiver clocks | read in the *receiver's* process, whose TX view is its own (server `rtp_us` = 0 on p0, so it falls back to its SRTT). The client's value never reaches it | no |

**App-echo SRTT** (`estimator.rtt()` and Copa `srtt`):
- **Time threshold** `9/8 · max(srtt, ewma)` per path, about 390 ms for p0 on CAND against about
  215 ms on BASE. The retransmit cooldown = pooled max over paths: `[QCLK] srtt_us_mean` is 257–375 ms
  on 20-gap rows, 375–542 ms on CAND/CANDS and 320–361 ms on CW0.
- **`expire_in_flight`** horizon = 4 × Copa srtt: the engine's `in_flight` is released later, which
  matters only for the placement queue term.
- **`contract_stall_s(rho, b, rtprop, srtt)`** is reached only from `three_term_store_cap`
  (`store_cap.rs:570`), which is off (`RWM_THREE_TERM=0`). The shed law's own clocks are untraced
  here (§7).
- **Sign: negative.** A longer queue slows the time channel and the cooldown, so holes would age
  more, which takes slots back from the queue. It is weak under lever B because its repair runs on
  the packet-threshold channel, which reads no RTT.
- **Tail sweep:** `2·SRTT` clamped to [25, 100] ms reads **100 ms at p05 on 30/30 rows**. The clamp
  always binds, so the inflation does not reach it.
- **Placement** `E_i = (I_i/W_i)·s_i + (½ + ε)·wire_i`: `s_i` is the echo SRTT, but it is multiplied
  by `I/W` ≈ 0.01–0.1 (above). The base term reads quinn's wire RTT, which excludes quinn's buffer.
  So **placement does not see p0's quinn queue.** It cannot steer the queue to p1; shares stay 0.82
  either way, and with both legs saturated, rates cannot change regardless.

### 4.1 Defect findings (rule 17/18: each needs a ledger verdict, not a footnote)

1. **The store cap's law is pinned at its clamp over its whole operating range at c8**, on main
   (BASE) as on lever B. `ask/ceiling` is 3–6× on BASE and 6–10× on lever B. The c8 population is
   N·pool = 4096, a resource bound, and the law's shape is invisible through it. Its two inputs are
   both over-read: `min_rtt` at 7–33× propagation and `max_bw` at 11–24× the link.
2. **Copa `min_rtt` reads the sender's own standing queue on main** (68–174 ms on a 10 ms path). The
   standing part on BASE is mostly quinn's BBR window on p0 (1.0–1.8k datagrams against a BDP of
   about 100) filling netem's queue. Lever B adds the quinn send-buffer dwell on top.
3. **The tail-sweep clock is pinned at `TAIL_SWEEP_MAX_US` = 100 ms** on all 30 rows (`[QCLK]`
   p05 = max).

## 5. Is it user-visible?

**`[LAT]` cannot answer this for the 20-gap arms.** Its arrival records live in a FIFO of 16 384
insertions (`lat.rs` `PENDING_MAX`), about 1.4 s of arrivals at 11.8k/s. A symbol delivered later
than that has lost its record, and is counted in `over=` rather than in `tot`. The final line is
cumulative, BASE's tail included. Records delivered (`p0:n + p1:n` over `n` = 507 616):

| arm | `[LAT]` coverage | `over=` |
|---|---|---|
| BASE | **0.11–0.24** | 774k–901k |
| BASES | **0.12–0.23** | 786k–889k |
| CAND | 0.82–1.00 | 4.7k–188k |
| CANDS | 0.78–0.99 | 12k–220k |
| CW0 | 0.99–1.00 | 3.7k–9.8k |

BASE's `ax_p50` (90–147 ms) and `tot_p50` (p0 655–1 180 ms) are therefore medians over the 11–24 %
of deliveries that kept their record: a survivor sample, mostly start-up and tail (below), biased
low. The §53 comparison "BASE `ax_p50` 105 ms against
CAND 311 ms, `tot_p50` about equal" is not a like-for-like reading. Histogram resolution is also
coarse: 8 sub-buckets per octave, so 65.5 ms steps at 0.5–1 s.

**Little's law on the sender's own store** replaces it. Retained = `win + gate`, the sent store above
the receiver's cumulative point (`sack.rs:391`: the store is pruned as a prefix at the cumulative
ack). That is the seqs sent and not yet delivered in order, as the latest report saw them.
Equivalently, it is `src − ack` from the `[DIAG]` `cum=src/cod/ack` totals (BASE s42 r1,
t = 15 s: 173 034 − 122 845 = 50 189 = `win + gate`).

**Measured in-order delay** is the horizontal distance between the cumulative source-sent and
cumulative-ack curves: `delay(t) = t − t_src(ack(t))`, at 250 ms ticks, from the sender's view
(`inorder.py`). Medians of per-row values [row range], in seconds:

| arm | PIN mean | PIN p50 | PIN p95 | whole-run mean | whole-run p95 |
|---|---|---|---|---|---|
| BASE | **5.30** [4.73–5.62] | 5.40 [4.94–5.75] | 6.74 [6.36–7.49] | 4.41 [4.02–4.79] | 6.76 [6.38–7.35] |
| BASES | **5.23** [4.84–5.60] | 5.28 [4.61–5.62] | 6.84 [6.35–7.88] | 4.50 [4.14–4.94] | 6.75 [6.32–7.74] |
| CAND | **0.97** [0.86–1.31] | 0.93 [0.80–1.27] | 1.47 [1.32–2.12] | 0.96 [0.87–1.29] | 1.46 [1.44–2.12] |
| CANDS | **1.09** [0.94–1.39] | 1.03 [0.90–1.31] | 1.79 [1.50–2.27] | 1.02 [0.89–1.36] | 1.77 [1.43–2.27] |
| CW0 | **0.96** [0.90–1.00] | 0.93 [0.85–1.00] | 1.31 [1.21–1.37] | 0.81 [0.77–0.85] | 1.27 [1.19–1.34] |

The stock is not stationary: on BASE s42 r1, `gate` moves between 36k and 75k across the PIN phase.
That is why the table reports the flow measurement rather than the retained/rate ratio
(`little.py`, which gives 4.8–5.5 s on 20-gap rows and 0.78–1.29 s on lever B).

`src − ack` first exceeds `[LAT]`'s 16 384 bound at t = 3.8–9.0 s on every 20-gap row. On lever-B
rows that happens only transiently (2.0–35.3 s), and coverage stays 0.78–1.00. So the 20-gap
`[LAT]` survivors are mostly start-up and tail deliveries, not a uniform sample.

Three other readings agree:
- **`[LAT]` coverage:** on 20-gap rows 76–89 % of deliveries lost their record, consistent with a
  multi-second wait; on lever B, 0–22 %.
- **`[SUCC]` hole lifetime** (from detection), holes closed by a repair: `rep_p90` is 5.2–6.8 s on
  20-gap arms against 0.52–1.18 s on lever B, and `orig_p99` is 5.2–6.8 s against 0.52–1.31 s.
- **The tail:** BASE's tail of 2.6–5.4 s after source end is this backlog draining.

The counted store is pinned at 4096 on both binaries. The retained store is not pinned: under the
20-gap horizon, about 50k seqs already received sit behind old holes at the frontier.

**Verdict.**
- **In-order delivered delay is about 5× better under lever B** (PIN-phase mean about 1.0–1.1 s
  against about 5.2–5.3 s; whole-run mean 0.8–1.0 s against 4.4–4.5 s). The extra p0 transit (+150–230 ms) is part of a much smaller total: under the 20-gap
  horizon, symbols wait seconds in the reorder buffer behind old holes; under lever B, they wait a
  fraction of a second in quinn's send buffer.
- **Correction to §53's readout.** "Delivered `tot_p50` about the same, ~0.9 s" is an artefact of
  `[LAT]`'s eviction bound on the 20-gap arms.
- What a user would see worse is per-symbol arrival latency: p0 first-arrival + 150–230 ms. It
  matters only to a consumer of out-of-order or first-copy arrivals; in-order reliable delivery does
  not see it.
- The control laws see it: Copa `min_rtt`, the time-threshold channel and the retransmit cooldown
  (§4).

## 6. What would confirm it on the VM (falsifiable; not a fix)

The mechanism says the p0 backlog is `W − wire₀ − path₁ − H`, where W is the binding ceiling.
Dial W through `RWM_STORE_PATH_POOL`. The ceiling is 2·pool, and the ask stays above it at every
value below. The variable is read from the environment (`gates.rs:323`), echoed in `[GATES]` and
forwarded by `tools/l1/lib.sh` (`RWM_FORWARD`). Do not use `RWM_STORE=<n>`: it sets
`store_env_set`, which turns the dynamic law off and so changes the seat. Run on CAND, c8-100 at
600 MB, `RWM_DIAG=1`, n = 3 per value:

- **P1 (identity):** at pool = 1 280 (W = 2 560) and pool = 1 536 (W = 3 072), against the
  shipped W = 4 096, in the PIN phase:
  - `win − Σ rᵢ·rttᵢ` stays within ±150 of 0;
  - `tx0` and `tx1` rates stay within 1 % of 9 855 and 2 000 /s;
  - goodput stays within 2 %.
- **P2 (the backlog follows W):** path₀ ≈ 0.84 · W (CAND's p0 share of the population):
  - at W = 2 560: path₀ ≈ 2 150 ± 300, q₀ ≈ path₀ − qcwnd₀ ≈ 870 ± 400;
  - at W = 3 072: path₀ ≈ 2 600 ± 300, q₀ ≈ 1 300 ± 400 (against 2 316 measured at 4 096);
  - `rtt0 − wrtt0` falls by ≈ ΔW·0.84 / 9 855 s (≈ 90 ms per 1 024 slots);
  - `rtp_us` p0 after 10.3 s falls the same way;
  - `full` (evictions) → 0 at both points.
- **Why not above 4 096:** quinn's 4 MB datagram buffer holds about 3 400 datagrams. CW0 at q₀ ≈ 2 900
  already evicts up to about 1 000 per run, and evictions become holes. A W that pushes q₀ toward
  3 400 breaks P1 for a reason outside this mechanism; any point above 4 096 must predict `full` as
  well.
- **P3 (reported, not discriminating):** BASE at pool = 1 536. The identity predicts residual = H at
  the new W, but not how the removed 1 024 slots split between H and the queue; that depends on the
  20-gap repair rate, which is not modelled here.
- **Falsifiers:**
  - `|win − Σ rᵢ·rttᵢ| > 0.1·W` on any lever-B row;
  - CAND `q0` at W = 2 560 above 1 500, or the change in `q0` from W = 2 560 to 4 096 less than
    0.5 × (the change in W) × 0.84;
  - CAND goodput falls by more than 2 % at W = 3 072, with `tx` rates still at link (that would mean
    the 4096 slots were not all transit plus holes).
- **Caveat:** keep W ≥ wire₀ + path₁ + about 500 (≈ 2 400). Below that, the population no longer
  covers the pipe and throughput falls, which tests a different statement. W = 2 560 is close to
  that edge; if its `tx` rates fall, read only W = 3 072.

**Already observed in this data (not predictions):** on 30 of 30 rows, `rtp_us` p0 steps at
t = 10.3–10.5 s. That is the 10 s window expiry of the start-up sample.

| arm | start value | value just after the step |
|---|---|---|
| BASE | 10.2–12.1 ms | 29–37 ms |
| BASES | 9.3–14.2 ms | 23–60 ms |
| CAND | 8.4–12.0 ms | 21–40 ms |
| CANDS | 9.8–13.1 ms | 29–46 ms |
| CW0 | 9.1–12.6 ms | 27–52 ms |

This is why the guard's `rtp_floor_p0` (the run minimum) is a start-up reading on lever B (no tail)
and a drained-tail reading on BASE.

## 7. Open

- **Why quinn's BBR holds 1.0–1.8k datagrams on a path with a BDP of about 100** (wire RTT
  100–160 ms at 10 ms propagation), on every arm. Not diagnosed here.
- **The `max_bw` over-read** (11–24×) feeds the same pinned law; not diagnosed here.
- The PRE/PIN alignment of server holes to client ticks is by readout index (about 1 s); a
  timestamped `[RANK]` would make the 20-gap bracket an equality test.
- **`[LAT]`'s 16 384-insertion bound truncates every 20-gap c8 row.** A delivered-delay readout that
  covers BASE needs a larger bound or a sender-side Little readout.
- **quinn send-buffer evictions on lever-B p0** (`full` 0–996 per run; CW0 highest). An evicted
  datagram looks like wire loss to the peer's tracker. Whether evictions contribute to CW0's p0
  duplicate rise (status §53 open item 5) is not tested here.
- **Not traced:** the shed law's clocks (`RWM_UNIFIED_SHED=1`), and whether retransmits carry the 0
  echo sentinel (the CW0 dwell gap in §1).

## 8. §54 design and predictions (reported only; written before any measurement)

Fable's correction to §6 applies. On CAND alone, a smaller pool only re-tests Little's law: the queue
shrinks, and nothing about the holes is tested. The discriminating row is the **same reduced pool on
BASE**. If about 1 400 of BASE's 4 096 slots are held by old holes, a ceiling near that number leaves
BASE with too few slots in transit to fill the links, while CAND, whose old holes are 0, still fills
them. So BASE loses goodput where CAND does not.

### 8.1 The knob (verified in source on `github/main` a9090e99)

- **Definition.** `RWM_STORE_PATH_POOL` is read once by `RuntimeGates::resolve` as
  `env_parse::<usize>("RWM_STORE_PATH_POOL").unwrap_or(2048)` (`gates.rs:323`).
  - Default 2048.
  - **Parse:** decimal `usize` only. `-1`, `1e3`, `1024.0`, an empty string or any other unparsable
    value falls back **silently** to 2048. `0` parses, and gives a ceiling equal to
    `max(0, floor)` = `STORE_CAP_FLOOR`.
  - So liveness must compare the echoed value with the requested one.
- **The clamp is exactly N × pool and reaches the c8 sender.**
  - `pol.store_path_pool = gates.store_path_pool` (`sender_policy.rs:554`). It feeds the shipped seat
    `pooled_store_cap(…, n_live, bdp, gain, store_cap_floor, store_path_pool)`
    (`sender_phases.rs:191-201`).
  - There, `ceiling = n_live.saturating_mul(pool).max(floor)` and the cap is
    `law.ceil().clamp(floor, ceiling)` (`store_cap.rs:90-94`).
  - `n_live = live_paths().len().max(1)` = 2 at c8.
  - This is the branch the c8 rows run: plain reliable, `RWM_STORE` unset, so `plain_dyn_cap`;
    `RWM_STORE_PATHS=1`; `RWM_COPA_FEED=0`; `RWM_THREE_TERM=0`. `[DCAP]`/`[SUMCAP]` echo it.
  - `tx_paused` ⇔ `store_len ≥ cap` (`mod.rs:2393`), and the in-flight brake is unarmed.
  - The pool's only other uses are the two gauges' clamp bounds (`mod.rs:2022, 2034`).
  - `RWM_STORE=<n>` must not be used as the dial. It sets `store_env_set`, which turns the dynamic
    law off and moves the row to a different seat.
- **Echo.**
  - `[GATES] … RWM_STORE_PATH_POOL=<n>` (`gates.rs:427/456`, byte-pinned in `gates/tests.rs:56`).
  - The INFO line `path-scaled outstanding pool ACTIVE … pool_per_path=<n>` (`mod.rs:1708`) prints
    on the sender.
  - The `[DIAG]` `win=a/b` denominator shows the realized cap.
- **Forwarding.** `RWM_STORE_PATH_POOL` is in `tools/l1/lib.sh` `RWM_FORWARD`.
- **Missing from the harness:**
  1. Arm tokens. The plan-53 battery lives on `github/cand53`; plan 54 needs `BASE'P<n>` and
     `CAND'P<n>` setting `RWM_STORE_PATH_POOL=<n>` on the respective binary.
  2. **`-u RWM_STORE_PATH_POOL` in the battery's `env -u` scrub** (`loopdiag_battery.sh` `run_one`).
     It is absent on `main` and on `cand53`, so a value left in the launching shell would leak into
     every "default" row.
  3. The readouts and liveness checks of §8.5.

### 8.2 Arms

Built on the parallel `snapver-law` tree, where the filter is the default law with no env arm. The
branch is not yet on `github` when this is written, so its SHA is pinned by amendment.

| arm | binary | env | ceiling W = 2·pool |
|---|---|---|---|
| BASE'P2048 | BASE' = main + filter law | none (shipped) | 4 096 (control) |
| BASE'P1024 | BASE' | `RWM_STORE_PATH_POOL=1024` | 2 048 |
| BASE'P640 | BASE' | `RWM_STORE_PATH_POOL=640` | 1 280 (**the discriminating row**) |
| CAND'P2048 | CAND' = BASE' + lever B | none | 4 096 (control) |
| CAND'P1024 | CAND' | `RWM_STORE_PATH_POOL=1024` | 2 048 |
| CAND'P640 | CAND' | `RWM_STORE_PATH_POOL=640` | 1 280 |

- **Setup:** c8-100 at 600 MB, `RWM_DIAG=1`, `RWM_RDIAG=1`, `RWM_PERF_TIMEOUT_S=200`.
- **Scale:** n = 6 per arm, rotated per group, 36 rows.
- **Why these sizes and not §6's 2 560 / 3 072.** Those two would sit above BASE's predicted
  saturation edge and would only move CAND's queue.

### 8.3 The model the predictions come from

From §2.2, in the PIN phase (cap = W):

    W  =  Σᵢ rᵢ · τᵢ  +  H        τᵢ = leg i's app-echo transit time; H = slots held by holes older than ~1 RTT

The links are full iff `Σ rᵢ·τᵢ ≥ P_min`. P_min is the transit population needed to keep both links
busy with no standing queue:

    P_min ≈ r₀·τ₀,min + r₁·τ₁,min ≈ 9 855 × 0.012 s + 1 950 × 0.042 s ≈ 200     (× 1–3 for ack and emit-burst slack: 200–600)

Here τ₀,min and τ₁,min are the start-up p0/p1 `rtp_us` (8.4–14.2 ms and about 40 ms, §6).
Controls (§53 reference rows with the filter on: BASES, CANDS, PIN phase): BASES H = 1 089–1 510
old holes, residual 1 257–1 733; CANDS H = 0–1.

How H responds to W is the unknown the BASE rows test. Two bounding readings:
- **(H1) H fixed in symbols:** the 20-gap horizon's backlog does not depend on W.
- **(H2) H = p̄ · r · T_H with T_H fixed in seconds:** p̄ ≈ 0.030 pooled loss; T_H ≈ H/λ ≈
  1 460 / 354 /s ≈ 4.1 s at the control.

Then:
- **BASE' saturation edge:** W* = H + P_min ≈ **1.3k–2.1k** under H1. Under H2 the edge is a little
  lower, and below it r = W / (τ_min + p̄·T_H), e.g. W = 1 280 → r ≈ 7–8k/s, about −35 %.
- **CAND' saturation edge:** W* ≈ P_min + the repair slots λ·T_repair (≈ 354 /s × 0.2 s ≈ 70) ≈
  **300–700**.

W = 1 280 is below BASE's edge under both readings and about 2× above CAND's. W = 2 048 is at BASE's
edge and serves as the dose step.

**(H3) The live alternative: holes are elastic.** BASE's old holes exist only because the oversized
pool lets the RTT, and with it the number of open gaps per report, grow past the 20-gap horizon. At
a small W the RTT drops, the horizon stops binding, and BASE behaves like CAND.
- Its signature: BASE'P640 keeps goodput, its old holes fall to about 0 and its tail shortens to
  CAND's.
- A rough check argues against it: open gaps per report ≈ λ_loss × 2·rtt ≈ 354 × 0.1 s ≈ 35 > 20
  even at a 50 ms RTT. It is not excluded.

### 8.4 Predictions (per arm; PIN-phase medians of per-row values unless marked; bands are arm ranges)

| readout | BASE'P2048 | BASE'P1024 | **BASE'P640** | CAND'P2048 | CAND'P1024 | **CAND'P640** |
|---|---|---|---|---|---|---|
| goodput Mbit/s (scorer, whole run) | 96–103 (BASES 96.3–102.1) | 75–103 | **20–85** | 107–109 (CANDS 107.2–108.3) | 105–109 | **104–109** |
| Σ tx rate in the PIN phase, /s | 11.7k–11.9k | 9k–11.9k | **≤ 10k** | 11.8k–11.9k | 11.7k–11.9k | **11.6k–11.9k** |
| p0 send backlog `q0` = `dgq0 hand − tx` | 300–1 600 (BASES 356–1 542) | 0–300 | 0–100 | 900–2 700 (CANDS 903–2 642) | 100–1 000 | 0–400 |
| p0 app-echo `rtt0` ms | 180–215 | 30–110 | 12–60 | 300–355 | 125–225 | 70–150 |
| `rtt0 − wrtt0` ms | 30–140 | 0–30 | 0–10 | 80–270 | 10–100 | 0–40 |
| budget residual `W − Σ rᵢ·rttᵢ` | 1.25k–1.75k | 1.0k–1.8k | **≥ 900** | −60…+60 | −60…+60 | −60…+60 |
| old holes (server `[RANK]` holes − tail_overcount) | 1.0k–1.55k | 0.9k–1.6k | **≥ 700** | 0–10 | 0–10 | 0–10 |
| tail s | 2.6–5.5 | 2–6 | 1.5–6 | 0.2–0.7 | 0.1–0.7 | 0.1–0.7 |
| quinn evictions (`dgq0 full`, last line) | 0 | 0 | 0 | 0–350 | 0 | 0 |
| in-order delay, PIN mean s (`inorder.py`) | 4.8–5.6 | ≥ 2.5 | **≥ 2.5** | 0.9–1.4 | 0.5–1.0 | 0.4–0.9 |
| in-order delay, PIN p95 s | 6.3–7.9 | ≥ 3.5 | ≥ 3.5 | 1.5–2.3 | 0.8–1.7 | 0.6–1.4 |
| receiver duplicates | 1 230–1 810 (BASES) | reported | reported | 1 100–1 490 (CANDS) | reported | reported |

Notes on the bands:
- **The q0, rtt and delay bands at reduced W** take the population shares measured at 4096: p0
  takes 0.84 of the population on CAND and about 0.8 of the transit population on BASE. They assume
  quinn's window does not grow when its path goes app-limited.
- **BASE'P640's goodput band** spans H1 (a stall toward the bottom) to H2 (about −35 %). The
  prediction is the side, not the point: clearly outside CAND'P640's −3 %.
- **Duplicates at reduced W:** no reading here predicts them. They are reported.
- **H3's predicted row instead:** BASE'P640 goodput ≥ 95, old holes ≤ 300, tail ≤ 1 s, in-order
  mean ≤ 1.5 s.

**Falsifiers (each separates "old holes hold the slots" from an alternative):**
1. **BASE'P640 goodput ≥ 0.95 × BASE'P2048** while its old holes stay ≥ 700. Holes then hold slots
   without costing goodput, so the identity or P_min is wrong. This **refutes** the mechanism.
2. **BASE'P640 goodput ≥ 0.95 × control and old holes ≤ 300:** H3, elastic holes. The bookkeeping
   survives, but lever B's gain is then a property of the oversized pool, which changes the merge
   argument. Reported as a finding, not a refutation.
3. **CAND'P640 goodput < 0.97 × CAND'P2048, or Σ tx < 11.5k/s:** CAND's edge is above 1 280, so P_min
   or the repair slots are underestimated. The contrast at W = 1 280 then loses its meaning; read
   W = 2 048 alone.
4. **On any lever-B row, `|W − Σ rᵢ·rttᵢ| > 0.1·W`:** the identity fails.
5. **Dose:** BASE' goodput (arm medians) not non-increasing from W = 4 096 to 2 048 to 1 280.

**Decision rule (reported only, nothing scored).**
- Falsifiers 1 and 4 do not fire, and BASE'P640 sits below CAND'P640 on goodput by more than both
  arms' ranges: **confirmed**. Old holes cost about H slots of the pool on main.
- Falsifier 2 fires: **H3**.
- Falsifier 1 or 4 fires: **refuted**; back to diagnosis.

### 8.5 Readouts and liveness the harness must add

All from the client `[DIAG]` lines unless marked. Every per-path field is anchored on
`pN:infl=`, because ` pN:` alone first matches the `mpr[… fp/on p0:… p1:…]` token.

- **PIN phase:** ticks with `win=(\d+)/(\d+)` whose denominator equals `2 × pool` (from the row's
  `[GATES]`), and ` src=(\d+)sym/s` > 0. Liveness: after the first such tick, ≥ 95 % of sending ticks
  are PIN, as at 4096 in §53 (4 206/4 209).
- **Quinn evictions per path:** `dgq(\d)\[hand=(\d+) tx=(\d+) full=(\d+) err=(\d+) sp=(\d+)\]` on
  the **last** `[DIAG]` line.
  - The readout is `full` (the count of evicting handoffs, a lower bound on datagrams evicted).
  - Beside it, `hand − tx`, an upper bound (it includes anything still queued).
  - Both per path, summed per arm.
- **Tail and in-order delay from `cum=src/cod/ack`:** ` cum=(\d+)/(\d+)/(\d+) ` with `t=([\d.]+)s`.
  `src` = `stats.fec.total_source_symbols`, which counts unique source symbols (it ends at
  507 616 = 600 MB / 1 182 B). `ack` = `window_ack_seq`, the `fetch_max` of the receiver's
  `next_expected` (`control_msg.rs:490`), i.e. the in-order frontier as the sender last heard it.
  - **In-order delay per tick:** `delay(t) = t − t_src(ack(t))`. `t_src(a)` is the time the `src`
    curve reached `a`, linearly interpolated between the 250 ms ticks. Report the mean, p50 and p95
    over PIN ticks and over all ticks with `ack > 0` (`p0-dwell-diag/inorder.py`).
  - **Tail:** `t(ack = total) − t(src = total)`, from the first tick where each reaches the final
    total. If `ack` never reaches it on a `[DIAG]` tick, keep the scorer's `tail_s` and flag the row.
- **Old holes H (server):** median over readouts `[12 : −6]` of `[RANK] holes − tail_overcount`.
  `[RANK]` lines are about 1 s apart from server start, so index 12 is past the 10.3 s step, and the
  last 6 are tail.
- **Budget residual:** per PIN tick, `win.a − (r₀·rtt₀ + r₁·rtt₁)/1000`. Here rᵢ = Δ`dgqi tx`/Δt over
  the PIN phase, and `rttᵢ` comes from `pi:infl=…rtt=(\d+)/wrtt=(\d+)` (`p0-dwell-diag/budget2.py`).
- **Kept from §53:** goodput, receiver duplicates, retransmits/GB, `[SNAPV]` and `[LAT]` (reported
  with its coverage `Σ pN:n / n`, never as the delay readout).
- **Liveness (a row counts only if):**
  - the client `[GATES]` shows `RWM_STORE_PATH_POOL=<requested n>` (2048 on controls; no silent
    fallback, §8.1), and the sender's INFO `pool_per_path=<n>` matches;
  - the PIN-phase cap equals `2·n`;
  - the filter law is live: `[SNAPV]` `pt_st + tt_st = 0` on every leg, and `stale > 0` on p1;
  - the binary witness holds at W = 4096 only (BASE' tail ≥ 2 s, CAND' ≤ 0.8 s). At reduced W the
    tail is an outcome, not a witness.

### 8.6 Does the filter being on in both arms change anything?

The §53 reference rows are BASES (filter on BASE) and CANDS (filter on CAND), and every control band
above is taken from them. Against the filter-off rows, the quantities the model uses barely move:

| quantity | BASE → BASES | CAND → CANDS |
|---|---|---|
| old holes (PIN) | 1 408 → 1 243 | 0 → 0 |
| budget residual | 1 556 → 1 464 | −8 → 20 |
| path₀ | 2 020 → 2 057 | 3 447 → 3 346 |
| goodput | 99.3 → 98.6 | 107.1 → 107.8 |
| tail | 3.95 → 4.42 s | 0.36 → 0.41 s |

Two things do change:
- **Duplicates and retransmits fall** on CAND to BASE's level, and the CAND' evictions band is
  CANDS's 0–336.
- **CANDS reads a lower `q0` than CAND** (1 747 against 2 316) at the same path₀, a quinn-window
  difference (§2.2). The q0 bands therefore use CANDS's range.

None of the predictions flips sign. The one premise to check is that the filter *law* on
`snapver-law` behaves like the §53 arm. The control rows test this: BASE'P2048 and CAND'P2048 must
land inside BASES's and CANDS's ranges on goodput, tail, old holes and duplicates. If they do not,
the reduced-W bands are re-based on the new controls before reading, and that re-basing is
reported.
