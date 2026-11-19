# S06: P_lost conditioned on resends in flight (the taper copy law)

Status: DERIVATION, proposed, review round 1 addressed. No engine change, no paper or ledger edit,
no VM.
- **Base:** `github/main` at `b65b25df`.
- **Ledger row:** S06, with E01 and E09 as inputs.
- **Paper:** §3.2, §3.4, §4.13, §11.3 and A.2 (`docs/fec-arq-model.md`).
- **Measured defect:** status "§51 block 51 Result, Addendum 2", with evidence in
  `docs/l1-raw/s51b/recopy/`.

**In plain words.**
- **Today.** Each correction slot asks "was the oldest unacked symbol lost?" and answers from its
  first send alone. It never counts the resends of that symbol already on the wire. So past the
  knee, every slot copies the same symbol until the cumulative ack moves.
- **The fix, part 1: count every send.** The probability that a symbol is still missing is the
  product of one P_lost factor per send. Each factor's age runs to the newest report, not to "now".
- **The fix, part 2: one draw per symbol per resend.** The draw is made once per resend, not once
  per slot. Only holes the receiver has named are candidates.
- **What it changes.** Copies fall from 4–6 per wire loss to between about 0.1 and 1 per wire loss.
  The freed slots go to coded repair; the slot count does not change.
- **What it does not settle.** Whether a copy is worth more than a coded symbol is still
  undecided. The "copy with probability π" rule is kept from the paper, not derived.

## 0. The defect as read on `b65b25df`

Code read for this document:
- **Today's decision** (`net/emit_source.rs:739–822`): each debt slot evaluates
  `p_lost(age since FIRST send, eps_at_send, srtt, rttvar)` on `retransmit_buffer.first()`, the
  cumulative-ack blocker, and copies it if `rand::random() < p`.
- **No stamp.** The copy writes no `nack_retx_at`, which only the NACK loop
  (`sender_phases.rs:1091`) and the request-copy producer (`net/mod.rs:3493`) write. So the NACK
  cooldown (`sender_phases.rs:871–878`) and the copy decision do not see each other.
- **Inputs.** `eps_at_send` is the worst-ε estimator at the original send (`emit_source.rs:499–505`),
  not the send path's. `(srtt, rttvar)` is the worst-ε path's `est.rtt()` and 0.1·srtt
  (`channel_set.rs:65–73`).
- **Pruning.** Entries leave only through shed or `prune_below(ack+1)` (`sender_phases.rs:1403`).

**Measured**, status Addendum 2 (`recopy/acct-c2.txt`, `acct-c8.txt`; per-arm medians, Auto HA):

| cell | taper / src | retx / src | lost / src | taper / lost | (taper + retx) / lost | below-floor arrivals |
|---|---|---|---|---|---|---|
| c2 | 0.1256 | 0.0262 | 0.0315 | 4.02 | 4.85 | 60 686 |
| c8 | 0.2379 | 0.0267 | 0.0377 | 6.29 | 7.00 | 93 436 |

How to read the table:
- **Denominators.** Status's `lost` is the sum over the legs of `[TRUTH] lost=`: every wire datagram
  lost, an upper bound on the source losses. Below, I also use L = src × the per-datagram loss
  rate (c2 0.0252, c8 0.0293: the median over the six HA rows of `truth_lost / truth_egress` from
  `P38ROW`) as the *source*-loss estimate. Each column header says which one it uses.
- **Copy share and blocker.** Addendum 2 reads a copy share of slots of 0.98 at c8 and a c8
  blocker age of 5–7 s against a worst-path srtt of 0.26–0.6 s.
- **Duplicates.** At least about 70 % of copies land below the receiver's decode floor.
- **NACK.** NACK resends are about the loss rate on every arm.

## 1. Channel model and assumptions

These are §3.2's assumptions, made explicit:
- **A1 (stated assumption): independent losses across sends.** Each send j of seq k is lost
  independently, with probability ε_j. This is §3.2's i.i.d. reading. Under GE bursts, a resend
  sent into the same burst is correlated with the loss it answers. A1 then makes π too low for
  resends sent within about B symbol times of the original.
- **A2: the report time is random.** A delivered send j is reflected by the first report generated
  after it arrives. The report reaches the sender RTT_j after the send, with S_j(a) =
  P(RTT_j > a) = 1 − Φ((a − SRTT_j)/RTTVAR_j). RTT_j is send → report arrival. That is what an
  app-echo sample measures, so the one-way return share is inside it.
- **A3: a report speaks only up to its horizon.** A WindowAck's SACK list is a *prefix* of the
  receiver's received runs (at most `MAX_SACK_RANGES` = 64, `sack.rs:518–545`). Its horizon h is
  the end of the last range. Above h the report says nothing ("not reported (may be in flight)").

§3.2 is the case of exactly one send:
```text
   P_lost(a; ε, SRTT, RTTVAR)  =  ε / [ ε + (1−ε)·S(a) ]                         (§3.2, p_lost)
```

## 2. The posterior with resends

**Evidence and its clock (review item 1).** The sender holds:
- the sends of k: t_0 is the original, and t_1 … t_m are NACK resends and copies alike, on paths
  p_j;
- the newest report R, which arrived at sender time t_R, has horizon h_R ≥ k, and does not cover
  k (neither its cumulative ack nor its SACK ranges).

R was generated at the receiver at t_R − D_ret. A delivered send j is reflected in R iff
t_j + D_fwd,j ≤ t_R − D_ret, that is, iff RTT_j ≤ t_R − t_j. So **each factor's age runs on the
report clock:**
```text
   a_j  =  t_R − t_j          (not now − t_j; the one-way share is already inside RTT_j)
```
- **No flip on elapsed time.** Between two reports π does not move. A check that ran the age to
  "now" would take π from 0.025 to 0.998 within 70 ms at SRTT 50 ms on elapsed time alone, with
  no new evidence. That flip does not happen here.
- **Each new report is new evidence.** A report that still does not cover k raises π.
- **Approximation (N ≥ 2).** D_ret is the return delay of R's own arrival path. Using the carrying
  leg's RTT_j assumes the return shares are comparable. Where they are not, the factor is off by
  the difference in return delay.

**Report cadence in the engine:**
- **Gap reports:** a SACK-bearing WindowAck goes out whenever a higher seq arrives, at most once
  per `GAP_ACK_MIN_INTERVAL` = 2 ms (`receiver.rs:1953–1957`, `net/mod.rs:236`). It is sent at
  every seat; `window_ack_emission` (`sack.rs:482–489`) has no seat input.
- **Cumulative advances** are advertised at once: `receiver.rs:1953` on data, and `:1077` on
  force-delivery.
- **Stalled frontier with no arrivals:** the hole is re-advertised every 2·SRTT, clamped to
  [25, 100] ms (`recovery_laws.rs:340`, `mod.rs:241–242`). That refresh runs only on the
  reliable seat (`receiver.rs:856`, `if recv_window_reliable`).
- **Net:** while data flows, t_R lags "now" by about 2 ms plus the return path.
- **Measured volume:** the c8 HA rows carry about 22.5k gap reports each (`mpr rep=`); the c2 HA
  rows about 21–23k.

**Limit: reports late or lost.** t_R freezes, so π freezes at its last value. No copy fires on
elapsed time; the absence of a report is not evidence about k. Recovery then rests on the
existing backstops:
- the receiver's stalled-hole refresh, at ρ = 1;
- the sender's tail sweep, 2·SRTT clamped (`recovery_laws.rs:327–332`, `mod.rs:250`);
- the evicting seat's force-delivery, at ρ < 1.

The copy law is not a liveness mechanism, and should not be one.

> **Sentence.** The probability that seq k is still missing, given every send of k and the newest
> report that could have named it, is the product over the sends of each send's single-send
> P_lost, aged to that report's arrival on the send's own path.

```text
   π_k  =  Π_{j=0}^{m} P_lost( t_R − t_j ; ε_{p_j}, SRTT_{p_j}, RTTVAR_{p_j} )    ack < k ≤ h_R, k not covered   (S06′)
   π_k  =  0                 k covered by R (cum-ack or SACK)
   π_k  =  Π_j ε_{p_j}       k > h_R: no report has spoken, so the posterior is the prior
```

The three lines are one likelihood. Send j's no-report term is S_j(a_j) where R could reflect k,
0 where R did, and 1 where R could not. They are evidence cases, not a δ or ρ branch. For m = 0
and k ≤ h_R, S06′ is §3.2 bit for bit, except that the age runs to t_R.

**Shape against the sentence:**
- every factor is in [0, 1];
- adding a send lowers π;
- π rises only when a report arrives;
- the only order in m is the product;
- all quantities are in seconds.

**Symbol provenance.**

| symbol | provenance |
|---|---|
| t_j, p_j | the sender's own state: `retransmit_buffer` holds the original (time, path); `nack_retx_at` holds the newest resend, extended in §5 |
| t_R, h_R | the sender's own state: the arrival time of the newest gap report, and the end of its last range |
| ε_p | measured: path p's loss estimator. The original's factor reads the worst-ε path's ε today (§2.1) |
| SRTT_p | measured: path p's smoothed app-echo RTT, `est.rtt()` (E01; dwell-inclusive, which is the right quantity for a send → report time) |
| RTTVAR_p | measured: path p's RFC 6298 mean deviation, `rtt_mdev_us` (`scheduler/path.rs:820–843`), ×1.2533 for σ. Today it is the 0.1·SRTT stand-in (E09) |
| Φ | cited: §3.2's Gaussian RTT model |

### 2.1 Input honesty, as a reading (review item 2)

**Method.** I read every `[DIAG]` tick with src > 0 on the c8 sweep rows (full logs; 6 rows per
arm; `[DIAG] … pN: pl=… rtt=…/wrtt=…/rtp…ms … rvar_us=…`). For each tick I took the worst-ε leg
the way `p_lost_inputs` does (the max of `pl=`, which is `estimator.loss_rate()`). The script is
the scratchpad `rtt.py`; it is not committed.

| c8 arm | worst leg = p0 | SRTT used: median (p10–p90) | 0.1·SRTT used | p0 app-echo rtt / wire rtt | p1 app-echo rtt / wire rtt | measured RTTVAR p0 / p1 |
|---|---|---|---|---|---|---|
| HA | 66 % of ticks | 308 ms (164–705) | 31 ms | 282 / 113 ms | 610 / 585 ms | 0.5 / 1.6 ms |
| NA | 67 % | 297 ms (166–699) | 30 ms | 275 / 117 ms | 612 / 578 ms | 0.5 / 1.6 ms |
| AM, AP, RM, RP | 67–70 % | 283–311 ms | 28–31 ms | 260–287 / 121–138 ms | 607–638 / 576–596 ms | 0.5 / 1.5–1.7 ms |

**The ε input reads mostly 0** (scratchpad `pl.py`; all ticks with src > 0):

| arm | ticks with max(`pl=`) > 0 | its median when > 0 | p90 | truth per-datagram loss |
|---|---|---|---|---|
| c8 HA | 45 % | 0.0032 | 0.044 | 0.025 / 0.045 (p0 / p1) |
| c2 HA | 18 % | 0.0033 | 0.037 | 0.025 |

**Readings:**
1. **SRTT used is the wrong leg two thirds of the time.** It is the fast leg's clock (about
   300 ms), while a resend placed on p1 is reported after about 610 ms. Every p1 epoch then ends in
   a copy: π crosses its draw at about 1.2 × 300 ms, long before the p1 report.
   - The worst-ε leg is p0, the fast leg, so this is the "early copy" direction of §3, not the
     harmless late one.
   - Per-path SRTT removes it. The p1 factor would then use about 610 ms.
2. **0.1·SRTT is 20–60× the measured RTTVAR.** That is 30 ms against 0.5 ms (p0) and 1.6 ms (p1).
   - The stand-in makes the knee *wider*, so it is not the source of early copies here.
   - But measured RTTVAR is not honest either. The app-echo EWMA drifts 137–336 ms (p0, p10–p90)
     over a run, and the per-sample mean deviation does not see the drift. With RTTVAR = 0.5 ms the
     posterior flips within 2 ms of SRTT. Any drift faster than the EWMA turns into copies.
   - So neither input is honest for S06′'s S_j. The honest one is the dispersion of the send →
     report time around the *current* SRTT, including its drift over one RTT. No gauge prints it.
3. **ε̂ is 0 on 55 % (c8) and 82 % (c2) of ticks,** against a truth of 0.025–0.045. `p_lost`
   returns 0 at ε ≤ 0, so a send whose ε̂ at send was 0 is never copied.
   - This damps today's defect. It does not cause it: the blocker's `eps_at_send` was taken on a
     tick where ε̂ > 0.
   - Under S06′ it would make factors on those sends 0, so their holes would rest on NACK alone.
   - ε̂ is the TX EWMA (`estimator.rs:498`). That is a DISHONEST-INPUT finding for every reader
     of `loss_rate()` as a per-send loss probability. It belongs on the estimator row, not here
     (reported, not fixed).

**The copy-chain bound (one copy per knee time, no backoff).** Assume the worst case: every
report is later than the knee, so every epoch ends in a copy.
- **Per hole.** An epoch whose draw is U > 2ε needs S ≤ ε(1−U)/(U(1−ε)) ≤ 1/2, so it lasts at
  least SRTT_used. With probability ≤ 2ε it ends sooner. So per named hole, copies ≤
  SRTT_used⁻¹/(1 − 2ε) ≈ 1.1 per SRTT_used.
- **Per sender.** Copies/s ≤ 1.1·H / SRTT_used, where H is the number of named seqs. H is measured
  as gap seqs per report (`mpr seqs/rep`): 67–77 at c8 HA and 33–42 at c2 HA.

| cell | H | SRTT_used | chain bound | slots/s today (r × src rate) | copies/s today |
|---|---|---|---|---|---|
| c8 HA | 72 | 164 ms (p10) | ≤ 480/s | 0.242 × 8 770 ≈ 2 120 | 2 067 (Addendum 2) |
| c8 HA | 72 | 308 ms (median) | ≤ 260/s | 2 120 | 2 067 |
| c2 HA | 38 | 245 ms | ≤ 170/s | 0.198 × 8 080 ≈ 1 600 | about 1 000 |

The worst case is ≤ 23 % of today's slot count at c8 and ≤ 11 % at c2. So it stays below the slot
count, and below today's copies by 4–8×. Without backoff, the chain holds as long as the hole
does. That is the cost of inputs that are not honest, and per-path SRTT removes the p1 half of it
(owner question Q2).

## 3. Shape checks

Values are computed from S06′ with §3.2's WiFi values (ε = 0.025, SRTT = 50 ms, RTTVAR = 5 ms)
unless stated. Ages are on the report clock.

- **ε → 0.** π → 0 at a fixed age, so a clean link never copies. The order of limits matters: at a
  fixed ε > 0, a → ∞ gives 1. `p_lost` returns 0 at ε ≤ 0; under the model that is an anomaly the
  model cannot price, and NACK and the tail sweep are its backstop. §2.1 reading 3 shows this
  limit is live: ε̂ reads 0 on most ticks.
- **One resend in flight, younger than SRTT.** The original is 70 ms old (factor 0.9988):

  | a_1 | 0 | 20 ms | 40 ms | 50 ms | 55 ms | 60 ms | 70 ms |
  |---|---|---|---|---|---|---|---|
  | π | 0.0250 | 0.0250 | 0.0255 | 0.0487 | 0.139 | 0.529 | 0.998 |

  A young resend brings π down to its own path's ε. Today's code reads 0.9988 throughout.
- **age ≫ SRTT with m resends that all failed.** π → 1, and a copy is right. m does not lower π:
  each overdue report refutes delivery. The fragility is that late reports make every factor
  overconfident in the same direction (§2.1). The chain bound limits the cost to about 1.1 copies
  per SRTT_used per hole.
- **N = 1.** All factors share (ε, SRTT, RTTVAR), and the newest send dominates.
- **N = 2.** Each factor uses its own send's path. A c8-like case: the original is on the slow leg
  (0.045, 577 ms, 57.7 ms) and aged 6 s; the resend is on the fast leg (0.025, 237 ms, 23.7 ms):

  | resend age | 50 ms | 200 ms | 237 ms | 260 ms | 300 ms |
  |---|---|---|---|---|---|
  | π with per-path inputs | 0.025 | 0.027 | 0.049 | 0.134 | 0.867 |
  | π with the worst-ε (slow) leg's inputs | 0.045 | 0.045 | 0.045 | 0.045 | 0.045 |

  - When the worst-ε leg is the slow one, the error is a late copy, which is harmless.
  - When it is the fast one, which is the measured c8 case (§2.1), the error is early copies:
    waste.
  - Per-path inputs need no path-count branch: the product runs over sends.
- **δ and ρ.** S06′ has no δ or ρ input. δ reaches the slot only through the slot count
  r(β(δ)) and the shed deadline D(δ). ρ reaches it through the shed budget and the seat switch
  (§4, §6).

## 4. The slot decision

**The per-slot draw.** Today's draw runs once per slot, and slots are far more frequent than RTTs:
about 390 evaluations per SRTT at c2 Auto (1.6k slots/s × 245 ms), and 500–1 300 at c8. With a
resend in flight, π ≈ ε still gives 1 − 0.975³⁹⁰ ≈ 1, so you still get about one copy per loss.
The draw has to be made per symbol:

> **Sentence.** A slot carries a copy of a named hole k when π_k has reached k's draw for its
> current send epoch; otherwise it carries a coded symbol.

```text
   copy k   iff   π_k  ≥  U_{k,m_k} ,     U_{k,m} = h(k, m) / 2⁶⁴   (h: a fixed 64-bit mix)      (S06′ decision)
```

**Properties:**
1. **At most one copy per epoch per hole.** A copy increments m and draws afresh. Re-fires within
   an epoch happen with probability ≈ ε. This holds whatever the slot rate r(β(δ)) is.
2. **Integrated waste per delivered epoch.** The copy fires before the report iff U ≤ P_lost(RTT).
   S(RTT) ~ U(0,1), so
   ```text
      w(ε)  =  ∫₀¹ ε / (ε + (1−ε)s) ds  =  ε·ln(1/ε) / (1 − ε)            (0.095 at ε = 0.025; 0.158 at 0.05)
   ```
   This holds on honest inputs only; on the §2.1 inputs, p1 epochs end in a copy (the chain bound).
3. **Shared stamp.** NACK resends and copies stamp one per-seq clock (§5). Per epoch they produce
   one correction between them, plus the ≈ w redundant copy.

**The marginal value of a copy (review item 3).** Candidates are named holes, and the NACK path
already resends each one once per SRTT from the same stamp. So, at ρ = 1, a copy adds **no
degree of freedom** that the NACK would not send.

It adds only *timing*, in two forms:
- **(a) An earlier first resend,** when the posterior's knee (about 1.2·SRTT_p) comes before NACK
  ripeness.
  - Measured: the mean live-flight age at NACK fire (`mpr age=`, a cumulative mean over the run)
    is 195 ms at c2 HA, below its knee of about 290 ms. So at c2 the NACK usually wins and a copy
    adds about nothing.
  - At c8 the same gauge reads 3.0–3.6 s at the median (p10 0.41 s), against a knee of
    0.36–0.75 s. There a copy is the earlier trigger by seconds. That is C02's "the law sees only
    old holes", which the copy is papering over.
- **(b) Cover for the resend's own loss** (probability ε), a cooldown earlier than the NACK's next
  ripe report.

So a copy is a second *trigger* for the same correction, not a second correction. Its value is
latency, priced by δ. The slot decision does not read δ.

**What is derived and what is kept.**
- **Derived:** S06′ (the posterior), the epoch rule (one draw per symbol per epoch), the named-hole
  candidate set, and the bounds in properties 1–3.
- **Kept from §3.2, not derived:** "copy with probability π", i.e. U uniform. The derived
  comparison would weigh π_k's latency value against a coded symbol's usefulness u:
  ```text
     copy k  iff  π_k · v_copy(δ)  >  u · v_coded(δ)                      (target form)
  ```
  - u needs §4.13's DoF ledger. The model's u = 1 − P_fec(r) is about 7·10⁻⁴¹ at c2 Auto
    (W 201, ε 0.025, r 0.19), so it is degenerate.
  - The v(δ) needs the latency price of an earlier trigger.

  Neither exists, so **this part stays UNDERIVED** (owner question Q1).

**ρ < 1 (review item 4).** The `pol.shed_on` block in the slot (`emit_source.rs:760–795`) is
the give-up seat switch: existing debt (§11.3 give-up row), which this law does not build on. The
shed test runs first, on the original age against D(δ); a shed seq leaves the candidate set.

Does ρ select the named-hole set through the wire? It does not, provided the set is built from the
**gap stream**:
- **The receiver sends SACK-bearing gap reports at every seat.** The data-arm path is ungated
  (`receiver.rs:1953–1990`, `sack.rs:482`).
- **The sender gets the gaps at every seat.** `sack_to_gaps` (`control_msg.rs:728`) feeds
  `nack_tx`, which is `Some` whenever `!generation && !request_law` (`net/mod.rs:1062–1069`).
- **Only the SACK-range forward is seat-gated.** `sack_tx` is `Some` only on the reliable,
  non-generation, non-coded-only seat with the store-release gate on (`net/mod.rs:1101–1109`).
  Reading `sack_released` would make the evidence ρ-selected. So the requirement is: **build the
  named-hole set from the gap batches, never from `sack_released`** (§5, R1).
- **What ρ does change:** the stalled-hole refresh exists only at ρ = 1 (`receiver.rs:856`), and
  force-delivery only at ρ < 1. So the *report cadence* at a stall is seat-selected. That is the
  same seat-switch debt. It changes t_R, not the definition of a named hole.

**Arithmetic on block E, PM s42 r1** (c2, paced 35 Mbit/s, evicting seat; `s51e-session.tgz`,
last `[DIAG]` and `[TRUTH]`):

| quantity | value |
|---|---|
| src | 531 378 |
| coded | 101 361 |
| taper = plost | 10 197 |
| NACK retx | 9 315 |
| shed | 1 934 |
| wire loss | 0.0253 (10 864 lost) |
| `retx_buf` at end | 117 |
| app-echo srtt | 14 ms |
| mean NACK live-flight age | 17 ms |

- **Today.** L ≈ 0.0253 × 531 378 ≈ 13.4k. Copies are 0.76 per source loss, NACKs 0.69 and sheds
  0.14, for a total of 1.6 per loss. Copies are 9.1 % of slots (10 197 / 111 558).
- **There is no stuck blocker.** `retx_buf` is 117, so `first()` turns over within about one
  RTT, and today's law already behaves like a per-epoch rule there.
- **Under S06′** the band is 0.12–1.15 copies per source loss, so 1.6k–15.4k copies. Today's
  10.2k is inside it.
- **So S06′ changes little at ρ < 1 paced.** The defect is a stalled-frontier defect.

## 5. Candidates, state and requirements

**R1: the candidate set is the named holes,** built from the gap stream at every seat. A named
hole is a seq inside the newest gap batch's gaps (ack < k ≤ h_R, not SACKed) that is not shed;
the blocker is one of them.
- A SACKed seq has π = 0.
- A seq above h_R has π = Π ε. Its loss is not localised, so it belongs to coded repair (§3.4).
  Making it a candidate would copy w ≈ 0.1 of every delivered symbol.

**R2: the SACK and gap evidence must be held independently of any seat or gate.** This is not an
owner question: NO-MODE-SWITCH forces it. Per R1 it already holds if the set is read from the
`nack_tx` gap batches. `sack_released` must never be read.

**Selection and cost.**
- The slot takes the named hole with the largest π that meets π ≥ U. The order of candidates is
  by newest send time; it is held as a heap keyed by t_last and rebuilt per gap batch.
- H, the number of named seqs, is measured at 33–42 per report at c2 and 67–77 at c8. So the cost
  is O(H log H) per report and O(log H) per slot, and memory is O(H).

**Per-seq state.** Extend `nack_retx_at` from `seq → (t_last, path_last)` to
`(t_last, path_last, m, ε_last)`.
- That is 8 B more per resent seq. The map is pruned at the cumulative ack
  (`sender_phases.rs:1434`), so it holds ≤ |`retransmit_buffer`| entries.
- The original's (t_0, path_0) stays in `retransmit_buffer`.
- The ε stored at send should be the send path's.

**Settled here: one resend in the state (was Q5).** The state holds the original and the newest
resend:
```text
   π̂_k  =  P_lost(a_0) · P_lost(a_m)   ≥   π_k
```
- The dropped factors are ≤ 1, so the approximation can only bring a copy forward.
- Once the newest resend is older than SRTT + 3·RTTVAR, the dropped factors are ≈ 1.
- The only material case is an immediate re-fire (probability ≈ ε), which over-states by 1/ε for
  one epoch and costs at most the one copy that property 1 already counts.
- A second slot (12 B per seq) does not pay for itself.

**Settled here: ignore coded coverage (was Q4), with a bound.**
- A coded repair covers k only while k is inside the trailing span: within A*/R of k's send. That
  is about 109 / 8.1k ≈ 13 ms at c2 and about 109 / 8.8k ≈ 12 ms at c8 (`[SPAN] a_star=`).
- A recovery by such a row is reflected by the report that follows it. So the over-statement of
  π from ignoring coded rows is at most S(a_0 − A*/R) in absolute terms.
- At the first knee (a_0 ≈ SRTT + 2·RTTVAR), that is ≤ Q(2 − (A*/R)/RTTVAR). This is 0.055 with
  RTTVAR = 30 ms, and ≈ 0 with an RTTVAR of milliseconds. It is 0 for every later epoch, since k
  has left the span for good.
- The bound is smaller than w, so no factor is added.

**Settled here: do not use v9's `received_above` (was Q7).** It is evidence about seqs above the
horizon, which are not candidates. It would sharpen a prior that S06′ never acts on.

**Side effects of a copy stamping the clock.** These are intended; the request-copy producer at
`net/mod.rs:3493` already does the same.
- (a) The NACK cooldown (`sender_phases.rs:871`) suppresses a NACK within SRTT of a copy.
- (b) `mp_flight` clocks the copy's own flight.
- (c) The packet-threshold fast path is skipped for a resent seq (`!nack_retx_at.contains_key`).
- (d) `hold_echo.on_retx` is called beside the stamp.

## 6. Model side (review item 5)

**What I searched:** `raptorpath-wasm`, `raptorpath-math` and `raptorpath-visualizer`.
- `raptorpath-math` has `p_lost` (bit-pinned by `tests/engine_fold_pins.rs:51`), `p_lost_seq` and
  `p_lost_combined` (`lib.rs:1412–1421`). None of them is a copy decision.
- `raptorpath-wasm` takes one decision, its retransmit at `src/lib.rs:1266–1300`:
  - Candidates are symbols that are truly `lost && !recovered` (an oracle; the engine has no
    such flag).
  - A per-symbol cooldown of one path `rtt_ticks` since `last_retx_tick` is applied.
  - `p_lost` is evaluated on the age since the *original* send, with the loss belief and RTT of the
    path the symbol was *last* sent on.
  - A per-slot `rng() < pl` draw follows.
- The model has one channel (NACK and copy are merged), a stamp, and an oracle candidate set. So it
  never shows the engine's defect, and it diverges from S06′ in its candidate set (oracle against
  named holes) and in its age clock.

**What the change needs:**
- `p_miss` lands in `raptorpath-math` as a **new** function. `p_lost` is unchanged, so its bit pin
  and the wasm model are untouched in that commit (ADR-0076 phase 6, class A: no hint, δ or ρ
  input).
- The model–engine divergence then needs a **bounding test**, under CLAUDE.md's rule that every
  documented divergence carries a test that bounds it. In `raptorpath-wasm`, assert that
  retransmits per lost symbol per `rtt_ticks` are ≤ 1, the model's analogue of property 1, so the
  divergence is bounded in copies per hole.
- Moving the model onto `p_miss` is a separate, deliberate model change, with a `GOLDEN_CAPTURE=1`
  re-capture of `test_multipath_n1_identical_golden`, stated as such. I do not propose it in the
  engine change.

## 7. Predictions (arithmetic only; Auto, 6-row medians)

Assumptions:
- Slots are fixed by rr, so coded = slots − copies.
- L = src × per-datagram loss, the source-loss estimate. The last row restates the result per
  status's wire-loss denominator.

The edges of the band:
- **Lower edge:** the NACK wins the first epoch. Copies ≈ L·(w(1−ε) + ε).
- **Upper edge:** the copy wins the first epoch. Copies ≈ L·(1 + w(1−ε) + ε)/(1 − ε).
- **The chain term:** add up to the §2.1 chain bound while the inputs are not honest.

| per source symbol | c2 now | c2 under S06′ | c8 now | c8 under S06′ |
|---|---|---|---|---|
| L (source losses) | 0.0252 | — | 0.0293 | — |
| slots (taper + cod) | 0.198 | 0.198 | 0.242 | 0.242 |
| taper copies | 0.126 | 0.0030 – 0.029 | 0.238 | 0.0034 – 0.037 |
| coded | 0.072 | 0.169 – 0.195 | 0.0039 | 0.205 – 0.239 |
| copies per source loss (L) | 5.0 | 0.12 – 1.15 | 8.1 | 0.12 – 1.26 |
| copies per wire loss (status's `lost`) | 4.02 | 0.10 – 0.92 | 6.29 | 0.09 – 0.98 |

- **Where each cell lands, from the NACK-age reading of §4.** c2 lands near the lower edge: NACK
  fires at about 195 ms, before the knee. c8 lands near the upper edge: NACK fires at about 3 s,
  after it.
- **c8 under the worst case.** The chain bound adds ≤ 260–480 copies/s (≤ 0.03–0.055 per source)
  while SRTT is the worst leg's. The coded share of slots goes from 1.6 % to at least about 60 %
  in the worst case, and to 85–99 % on per-path inputs.
- **Shares:** at c2 from 36 % to 85–99 %; at c8 from 1.6 % to 85–99 % on per-path inputs.

**What a run must answer:**
- the race placement per hole, since `age=` is a run-mean, not a distribution;
- how much of the chain term is realised;
- whether the freed coded slots are useful, with r ≈ 0.24 against L ≈ 0.03 (R01, R02 over-provision);
- goodput and in-order delay;
- whether the c8 blocker age collapses;
- the NACK count with a shared cooldown.

Addendum 2's goodput arithmetic stands: spending the slots on coded repair leaves goodput
unchanged, because the slot count is the consumed r.

## 8. Proposed paper text

**§3.2, replace from "The transition is sharp;" to the end of the section with:**

> P_lost is the posterior of a symbol sent once. With resends, each send j is either lost or
> delivered and not yet reflected by the newest report R that could name the symbol, arrived at
> t_R. Under independent losses across sends, the probability that symbol k is still missing is
> then:
>
> ```text
>    π_k  =  Π_{j=0}^{m} P_lost(t_R − t_j ; ε_{p_j}, SRTT_{p_j}, RTTVAR_{p_j})   for k inside R's horizon
>    π_k  =  0 once a report covers k ;   π_k = Π_j ε_{p_j} above every report's horizon
> ```
>
> - Ages run on the report clock, so the posterior moves only when evidence arrives. If reports
>   stop, π freezes and the stall backstops recover the symbol.
> - A young resend brings π_k down to its own path's ε. Resends whose reports are all overdue
>   leave π_k at one.
>
> **The copy is decided per symbol, not per slot.** The candidates are the holes the newest report
> names. Each draws one uniform U_{k,m} per send epoch, and a correction slot copies k once π_k
> reaches it; otherwise the slot carries a coded symbol. Every send stamps one per-seq clock
> (time, path, count), which the NACK cooldown also reads.
>
> - For a delivered epoch, the copy fires before the report with probability
>   ε·ln(1/ε)/(1 − ε): 0.095 at ε = 0.025.
> - The copy adds no degree of freedom over the NACK resend. It is an earlier trigger and cover
>   for the resend's loss.
> - "Probability π" is retained from the single-send rule, not derived. The derived comparison
>   needs the latency value of an earlier trigger and the usefulness of a coded symbol (the DoF
>   ledger of Section 4.13).
> - A per-slot draw repeats the same question hundreds of times per RTT. That was the shipped
>   defect (status §51 block 51, Addendum 2).

**§3.2, the last paragraph:** withdraw "measured inert", and replace it with the engine state
once that ships.

**A.2:**
```text
   π_k  =  Π_j P_lost(t_R − t_j ; ε_{p_j}, SRTT_{p_j}, RTTVAR_{p_j})  (named holes) ;  copy iff π_k ≥ U_{k,m}      p_miss
```

**§11.3, P_lost inputs row:** add three things until they are fixed.
- The original's ε is the worst path's at send.
- SRTT is the worst-ε leg's, which is the fast leg on about 2/3 of c8 ticks.
- ε̂ reads 0 on 55–82 % of ticks.

## 9. S06 verdict: one set (review item 6)

**Adopt main's set**, with two refinements in the evidence text:
**PINNED(c8: P_lost ≈ 1 on the blocker), UNDERIVED(no resend in flight in the posterior;
"copy w.p. π" kept, not derived), DISHONEST-INPUT.**

- **I withdraw my DIVERGENT.** The code and §3.2 agree in shape: both lack the resend
  conditioning. So the defect is a law without provenance (UNDERIVED), not a code–paper gap.
  Once S06′ is in the paper, the code diverges from it until the engine change lands. That
  DIVERGENT belongs to the post-paper state.
- **The bind gauge already exists, at a stated tolerance.** The copy share of slots is E[p] over
  slot evaluations. A share of 0.98 at c8 therefore implies that at least (0.98 − θ)/(1 − θ) of
  evaluations have p ≥ θ: ≥ 50 % at θ = 0.96, and ≥ 80 % at θ = 0.9. That meets the ledger's
  PINNED threshold (bind fraction ≥ 0.5) at "p ≥ 0.96". The exact p ≥ 0.999 fraction waits for
  the `pmax=` counter that `c8-gauges` is adding.
- **DISHONEST-INPUT evidence to add:**
  - the worst-ε SRTT is the fast leg's on 66–70 % of c8 ticks, while p1 reports arrive at about
    610 ms;
  - ε̂ (`loss_rate()`) reads 0 on 55 % (c8) and 82 % (c2) of ticks.
  This is not my earlier "E01 / E09 are the cause". They are inputs, and they set the chain term,
  not the defect.
- **Fix class:** paper (this derivation) first, then the engine law with the red-first tests
  below. **P1** is proposed; the owner calls it.

## 10. Tests the engine change needs (red first)

1. **`raptorpath-math`: `p_miss_is_the_product_of_per_send_p_lost`**, absolute values.
   - m = 0 equals `p_lost` bit for bit.
   - The §3 tables: 0.0250 / 0.529 / 0.998 at a_1 = 0 / 60 / 70 ms, and ≥ 1 − 10⁻¹² for three
     overdue sends.
   - ε = 0 gives 0, a covered seq gives 0, and above the horizon the value is Π ε.
   - **N = 1** and **N = 2** (the 0.025 / 0.867 row) are absolute.
   - The value is invariant to the order of sends.
   - **Report clock:** the same value at t_R and at t_R + 1 s with no new report.
2. **`raptorpath-math`: `p_miss_draw_is_once_per_epoch`.** U_{k,m} is a pure function of (k, m).
   1 000 evaluations at a fixed state give one decision.
3. **Engine, host loopback, N = 1 and N = 2: `taper_copies_per_seq_bounded_under_stalled_cum_ack`.**
   - **Preconditions, each with a witness.** On main the slot never fires without them: `p_lost`
     is 0 at ε ≤ 0, and ε̂ is often 0 (§2.1).
     - Warm ε̂ with seeded i.i.d. loss of 2.5 %.
     - Run at Auto, so that rr > 0.
     - Assert that the slot count and `eps_at_send(k)` are both > 0 before the stall.
   - Drop k's original and its first resend.
   - Have the receiver withhold the cumulative ack past k for T = 20·SRTT, while still gap-reporting
     above it.
   - **Assert, as absolute counts:**
     - copies(k) + NACK(k) ≤ ⌈1.1·T / SRTT⌉ + 2;
     - no SACKed seq is copied;
     - no seq above the horizon is copied;
     - with reports suppressed for 5·SRTT, zero copies are made in that interval (the report-clock
       limit).
   - **Red on main:** about one copy of k per slot.
   - **Witness:** the stamps the copies wrote to `nack_retx_at`.
4. **Engine: `taper_copy_stamps_the_resend_clock`.**
   - After a copy, `nack_retx_at[k]` = (now, path, m + 1).
   - A gap report within SRTT increments `mpd_supp_cool` and sends no NACK.
5. **Engine: `named_holes_do_not_follow_the_rho_seat`.** One loss trace is run on the reliable seat
   and on the evicting seat, and with the store-release gate on and off.
   - The named-hole set fed to the copy law is identical in all four runs, as absolute seqs, at
     equal reports.
   - No SACKed seq is copied in any of them.
6. **Continuity through the δ presets (CLAUDE.md), per preset triple {0.98, 1, 1.02}·δ_p.**
   - `p_miss` takes no δ or ρ, so its output is identical at all nine points.
   - **At the engine, Auto and Realtime triples:** copies per named hole are identical within a
     triple, as absolute counts. The copy share differs by no more than the slot count's own
     change.
   - **At the engine, the Bulk triple:** this is the slot-starved regime, and Bulk has rr = 0. At
     every point copies ≤ slots, and the copy count is non-decreasing in the slot count, with no
     jump larger than the slot-count change. When both are 0 the share is undefined, which is not
     a step.
7. **`raptorpath-wasm`: the divergence bound of §6.** Retransmits per lost symbol per `rtt_ticks`
   are ≤ 1.

The change also needs the decision pins and the host ns bench. It is a deliberate behaviour
change, not a refactor, so the §41c guard does not apply.

## 11. Open questions for the owner

- **Q1. Ship the kept rule now, or wait for the derived comparison?**
  - The posterior, the epoch rule and the named-hole set are derived. "Copy w.p. π" is not (§4).
  - A third option is to drop the copy branch and give every proactive slot to coded repair,
    relying on NACK alone. That is the lower edge of §7 with zero copies.
  - Measured trade: at c2 it costs nothing, since NACK fires first. At c8 it loses the earlier
    trigger against a NACK that fires seconds late, which is C02's defect rather than S06's.
- **Q2. Per-path SRTT (E01's leg choice) with S06′, or before it?** The reading in §2.1:
  - the worst-ε SRTT is the fast leg on 2/3 of c8 ticks, so p1 epochs end in a copy, bounded at
    ≤ 260–480 copies/s, ≤ 23 % of slots;
  - neither the 0.1·SRTT stand-in nor the measured mean deviation (0.5–1.6 ms) is the right
    dispersion;
  - ε̂ reads 0 on most ticks.
  With per-path SRTT, the chain term at c8 falls to the drift-driven residual.
- **Q6. Ship S06′ with or after a rate-law verdict** (r ≈ 0.2–0.24 against loss 0.025–0.03; R01,
  R02)? S06′ moves 60–99 % of slots to coded repair, much of which the trailing span cannot use.
