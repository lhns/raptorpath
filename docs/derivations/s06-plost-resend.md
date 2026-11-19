# S06: P_lost conditioned on resends in flight (the taper copy law)

Status: DERIVATION, proposed. No engine change, no paper edit, no VM. Base `github/main` at
`87e646f7`. Law-ledger row S06 (rank 33), with E01 and E09 as inputs. Paper §3.2, §3.4, §3.7,
§4.13 and A.2 (`docs/fec-arq-model.md`). The measured defect is status §51 block 51 Result,
Finding 1, which goes to `rate-c8-frontier-recopy`.

**In plain words.** Each correction slot asks "was this symbol lost?" and answers from the
symbol's *first* send alone. It never counts the resends of that symbol already on the wire. So
once the oldest unacked symbol is older than about 1.2 srtt, every slot copies it again, until the
cumulative ack moves. The fix has two parts. (1) The question includes every send of the symbol:
the probability that it is still missing is the product of one P_lost factor per send. (2) Each
symbol gets one random draw per resend, not one per slot, and only holes the receiver has named are
candidates. Copies then fall from 5–8 per loss to about 0.12–1.3 per loss. The freed slots go to
coded repair; the number of slots does not change.

## 0. The defect as read on `87e646f7`

Code read for this document:

- `net/emit_source.rs:739–822`. Per debt slot, `st.retransmit_buffer.first()`, the lowest unacked
  seq, is evaluated with `p_lost(age_since_FIRST_send, eps_at_send, srtt, rttvar)`. The slot then
  copies it if `rand::random() < p`.
- The copy does not stamp `nack_retx_at`. Only the NACK loop (`sender_phases.rs:1091`) and the
  request-copy producer (`net/mod.rs:3493`) stamp it. So the NACK cooldown (`sender_phases.rs:871–878`)
  does not see proactive copies, and the copy decision does not see NACK resends.
- `eps_at_send` is the worst-ε estimator at the original send (`emit_source.rs:499–505`), not the
  ε of the path that carried the seq. `(srtt, rttvar)` is the worst-ε path's `est.rtt()` and
  `0.1·srtt` (`channel_set.rs:65–73`; E01, E09).
- Entries leave the buffer only through shed or `prune_below(ack+1)` (`sender_phases.rs:1403`).
  SACK marks never remove them (`sack.rs:140–150`).

So between two cumulative advances every slot evaluates the same seq. Its probability is computed
from the same original send time and is ≈ 1 past the knee.

**Measured** (my reads of `docs/l1-raw/s51b`, 6-row medians unless a row is named):

| cell, Auto (HA) | src/row | wire loss | NACK retx / src | taper copies / src | coded / src | copies per source loss |
|---|---|---|---|---|---|---|
| c2 | 507 616 | 0.0252 | 0.0262 | 0.126 | 0.072 | 5.0 |
| c8 | 507 616 | 0.0293 (egress-weighted, both legs) | 0.0267 | 0.238 | 0.0039 | 8.1 |

Sources for the table:
- Wire loss is the median over the six HA rows of `truth_lost / truth_egress` from `P38ROW` in `ld.log`.
- NACK retx per source is `retx/GB ÷ (src/byte · 10⁹)`, from `score-host.txt`.
- Taper and coded per source are the status §51 medians.
- The cited row c2 HA s42 r1 (`rows.py` on `s51b-diag-cited.tgz`) reads taper 0.094 and cod 0.096, so this one row is less extreme than the median. c8 HA s42 r1 reads taper 0.236 and cod 0.0042.
- I take source losses as src × wire loss. This assumes the wire drops every datagram class alike.

**From the coordinator's diagnosis** (the brief for this document; these figures are not yet in a
committed ledger):

- copies are 4–7× true wire losses over the 152 rows;
- at least about 70 % of copies land below the receiver's decode floor;
- up to about 137 copies per long stall at c8;
- the c8 blocker's age is 5–7 s against srtt 0.26–0.6 s.

I did not re-derive these. Status calls the ages an inference, because no age gauge is printed.
The table above is consistent with them: NACK retx ≈ loss on every arm, so each loss already gets
one NACK resend, and the copies are on top of it.

## 1. The channel model (unchanged from §3.2)

These are the paper's assumptions for P_lost, kept as they are:

- Each send j of a seq k is lost independently with probability ε_j (the i.i.d. reading of §3.2).
- If send j is delivered, the first report that reflects it reaches the sender after a random
  time RTT_j with survival function S_j(a) = P(RTT_j > a) = 1 − Φ((a − SRTT_j)/RTTVAR_j).
- "Reflects" means that the cumulative ack passes k, or that a SACK range covers k.
- **A report speaks only up to its horizon.** A capped report is a *prefix* of the receiver's
  received runs: at most `MAX_SACK_RANGES` = 64 ranges (`sack.rs:518–545`). Its horizon h is the
  end of its last range. Above h the report is silent: "not reported (may be in flight)". For
  k > h, the absence of k from the report is no evidence at all. At a stalled c8 frontier with
  20k–74k unacked seqs and a loss every ~35 seqs, 64 ranges cover only about 2k of them, so most
  unacked seqs sit above every report's horizon.

The paper's §3.2 is the case of exactly one send (m = 0):

```text
   P_lost(a; ε, SRTT, RTTVAR)  =  ε / [ ε + (1−ε)·S(a) ]                      (§3.2, p_lost)
```

## 2. The posterior with resends

**Evidence.** At time t the sender holds:
- the sends of k at times t_0 < t_1 < … < t_m, where t_0 is the original and t_1… are NACK resends
  and proactive copies alike, on paths p_0 … p_m;
- the latest report, in which neither the cumulative ack nor any SACK range covers k.

The reports that have arrived by t are exactly the ones whose RTT has elapsed. So the "no report
yet" event for send j is "RTT_j > a_j", with a_j = t − t_j. The sender's clock is "now", and the
report path is inside RTT_j.

**Hypotheses.** k is still missing (it will not arrive from what has been sent) iff every send is
lost. The prior is Π ε_j. The likelihood of "no report reflects k" is 1 under "all lost". Under
"send j delivered", the likelihood of send j's term is S_j(a_j). The sends are independent, so the
marginal factorises per send, and so does the posterior:

> **Sentence.** The probability that seq k is still missing at the receiver, given every send of k
> so far and no report reflecting it, is the product over its sends of the single-send P_lost of
> each send's own age on its own path.

```text
   π_k(t)  =  Π_{j=0}^{m}  P_lost( t − t_j ;  ε_{p_j}, SRTT_{p_j}, RTTVAR_{p_j} )    for ack < k ≤ h  (S06′)
   π_k(t)  =  0          if a report covers k (cum-ack or SACK)
   π_k(t)  =  Π_j ε_{p_j}    for k > h: no report has spoken about k, so the posterior is the prior
```

All three lines are one likelihood: send j's no-report term is S_j(a_j) where a report could have
reflected k, 0 where one did, and 1 where none could. They are evidence cases, not a δ or ρ branch.
The third line matters: a seq delivered above the horizon would otherwise age to π → 1 and be
copied, once per distinct seq. Each such seq is new, so the per-epoch cap of §4 would not bound it.

**Shape against the sentence.** Every factor is in [0, 1], so π_k ≤ min_j P_lost(a_j). Adding a
send can only lower π. Each factor rises with its age, so π rises with time. The only order in m is
the product. Units: ages and RTT moments are all in seconds. For m = 0, S06′ is §3.2 bit for bit.

**What it is not.**
- π_k is not the probability that the *original* was lost. That is the m = 0 factor, which is
  what the code evaluates today.
- π_k is also not conditioned on coded repairs whose span covers k. A coded row multiplies in one
  more factor ≤ 1, so S06′ is an upper bound when such rows exist. The trailing span
  [end − A*, end − Δ) does not reach an old blocker, so for the seqs this law acts on the bound is
  tight. §4.13's DoF ledger is where the coupling belongs (open question Q4).

**Symbol provenance.**

| symbol | provenance |
|---|---|
| t_j, p_j | the sender's own state: the original in `retransmit_buffer` (send time, path); resends in `nack_retx_at` (time, path), extended in §5 |
| ε_p | measured: path p's loss estimator. Today the original's factor reads the worst-ε estimator at send (`emit_source.rs:499–503`), which is DIVERGENT at N ≥ 2; the law wants the send path's ε |
| SRTT_p | measured: path p's smoothed send-to-report RTT. Today it is the worst-ε path's app-echo EWMA (E01, DISHONEST-INPUT) |
| RTTVAR_p | measured RTT dispersion per path. Today it is a stand-in, 0.1·SRTT floored at 1 ms (E09, UNDERIVED) |
| Φ | cited: the Gaussian RTT model of §3.2 |
| "covered by a report" | the exact reflection signal: cum-ack and SACK ranges, §4.13's definition of *live* |

## 3. Shape checks

All values are computed from S06′ (`scratchpad` arithmetic with the paper's §3.2 WiFi values
ε = 0.025, SRTT = 50 ms, RTTVAR = 5 ms, unless stated).

**ε → 0.** Every factor goes to 0 at a fixed age, so π → 0. A clean link never copies, and the
slot goes to coded repair. The order of limits matters: at fixed ε > 0, a → ∞ gives 1. In the code,
`p_lost` returns 0 at ε ≤ 0. Read under the model, that is an anomaly verdict: a seq that is never
acked on a link with "zero loss" is not a loss the model can price. NACK and the tail sweep stay
its backstop.

**One copy in flight, age below srtt.** The original is aged 70 ms, so its factor is 0.9988. Now
add a resend aged a_1:

| a_1 | 0 | 20 ms | 40 ms | 50 ms | 55 ms | 60 ms | 70 ms |
|---|---|---|---|---|---|---|---|
| π | 0.0250 | 0.0250 | 0.0255 | 0.0487 | 0.139 | 0.529 | 0.998 |

- A young resend brings π down to ≈ ε_{p_1}: the probability that the resend itself is lost.
- π comes back to 1 only once the resend's own report is overdue. The curve is §3.2's curve,
  re-started at the resend.
- Today's code reads 0.9988 throughout. That is the measured duplication.

**age ≫ srtt with m resends that all failed.** Every factor → 1, so π → 1. The symbol is surely
still missing, and a copy is right. m does not lower π: each overdue report refutes delivery
outright, because the Gaussian survival falls faster than ε^m does.

The same point is a fragility. If reports are late for a reason the model lacks (dwell in
`est.rtt()`, E01; a too-small RTTVAR, E09; a slow ack path), every factor is overconfident *in the
same direction*. The sends are then not independent evidence. S06′ stays correct only with honest
per-path SRTT and RTTVAR. The rate of copies this can cause is bounded by the draw rule of §4: at
most one per resend epoch, not one per slot.

**N = 1.** All factors share (ε, SRTT, RTTVAR). π = Π_j P_lost(a_j), and the newest send dominates.
The candidate set of §5 (the named holes) has no path dimension, at any N.

**N = 2.** Each factor uses the path of its own send. A c8-like example:
- the original was sent on the slow leg (ε 0.045, SRTT 577 ms, RTTVAR 57.7 ms) and is aged 6 s;
- a resend went on the fast leg (ε 0.025, SRTT 237 ms, RTTVAR 23.7 ms).

| resend age | 50 ms | 200 ms | 237 ms | 260 ms | 300 ms |
|---|---|---|---|---|---|
| π, per-path inputs (S06′) | 0.025 | 0.027 | 0.049 | 0.134 | 0.867 |
| π, worst-ε path's inputs for every factor (today's input choice) | 0.045 | 0.045 | 0.045 | 0.045 | 0.045 |

With today's inputs, the fast-leg resend is judged on the slow leg's clock. Its loss is noticed
only after 577 ms + 3·58 ms instead of 237 + 3·24 ms. That is late, not wasteful, but only because
the worst-ε leg is the slow one here. Where the worst-ε leg is the fast one, a slow-leg resend is
judged on the fast clock, and the error is early copies: waste. Per-path inputs
are required for the shape at N ≥ 2. They need no path-count branch: the product runs over sends,
whatever N is.

**Continuity in δ and ρ.** S06′ has no δ and no ρ input. The δ dial reaches this law only through:
- the number of slots, r(β(δ)), which is already continuous and pinned;
- the shed deadline D(δ) inside the slot.

ρ reaches it only through the shed budget. So the copy law is trivially continuous through every
preset, and the test of §8 asserts this at the engine (copies per hole), not only at the formula.

## 4. The slot decision: when a copy of k beats a coded symbol

**The per-slot draw is the second defect.** Today a slot copies with probability π, and slots are
far more frequent than RTTs:
- c2 Auto has about 8.1k source/s × 0.198 ≈ 1.6k slots/s, so about 390 evaluations per
  srtt (245 ms);
- c8 has about 2.1k slots/s, so 500–1 300 per fast-leg srtt.

Re-stamping alone therefore does not fix the duplication. With a NACK resend in flight, π ≈ ε =
0.025, and 1 − 0.975³⁹⁰ ≈ 1, so you still get about one copy per loss, duplicating its NACK resend.
The paper's "retransmit with probability P_lost(t)" is right *per symbol* and wrong *per slot*.
The law has to state it per symbol.

**The Bayes comparison, and why it cannot ship yet.** Both kinds of symbol cross a channel that
loses them with the same ε, so ε cancels. A slot that carries a copy of k delivers a needed
degree of freedom (DoF) with probability π_k. A coded symbol delivers one with probability u, the
chance that its span still has a missing DoF that no live correction covers. The expected-DoF rule
is:

```text
   copy k  iff  π_k  >  u                                                         (Bayes form)
```

The paper's only closed form for u is the window model at the operating rate, u ≈ 1 − P_fec(r)
(A.3).
- At c2 Auto (W 201, ε 0.025, r 0.19, σ² = 1) this is Q(13.3) ≈ 7·10⁻⁴¹. The rule then copies on
  every slot, because every unacked seq has π ≥ ε ≫ u. That reproduces the defect.
- So the model u is DEGENERATE at the operating point: r ≫ ε, which is R01's and R02's business.
- A measured u needs §4.13's DoF ledger: per-component k_c − a_c from receiver reports. The plain
  window does not implement it (§11.3, "the rate law's feedback leg").

This form is therefore the target, not the shipped law (Q1).

**The shippable form: one draw per symbol per send epoch.** Read §3.2's rule literally: the
*symbol* is retransmitted with probability P_lost by the time its report is overdue. Give each
(seq, send count) one uniform variate, U_{k,m} ~ U(0,1), and copy at the first slot where the
posterior reaches it:

> **Sentence.** A slot carries a copy of the candidate seq k when the probability that k is still
> missing has reached that seq's draw for its current send epoch; otherwise the slot carries a coded
> symbol.

```text
   copy k   iff   π_k(t)  ≥  U_{k,m_k} ,     U_{k,m} = h(k, m) / 2⁶⁴                 (S06′ decision)
```

Here h is a fixed 64-bit mix of (seq, send count), the same draw on every evaluation. It needs no
RNG state, and a test can reproduce it.

Properties:

1. **At most one copy per epoch.** A copy increments m_k and starts a fresh epoch, with its own
   draw, and π falls to ≈ π·ε_c. So per epoch, P(copy) = P(U ≤ sup π over the epoch) ≤ 1.
   Immediate re-fires happen with probability ≈ ε_c. This holds whatever the slot rate, so it
   holds whatever r(β(δ)) is.
2. **NACK and copies compose.** Both producers stamp the same per-seq clock (§5). A NACK resend
   starts an epoch the copy law sees, and a copy engages the NACK cooldown. Together they produce
   about one correction per seq per resend round, whichever fires first.
3. **The waste per epoch, integrated.** Take an epoch whose send is in fact delivered. π rises
   with age, so the copy fires before the report iff U ≤ P_lost(RTT), where RTT is that send's
   report time. By the probability integral transform, S(RTT) ~ U(0,1), so

   ```text
      w(ε)  =  E[ P_lost(RTT) ]  =  ∫₀¹ ε / (ε + (1−ε)s) ds  =  ε·ln(1/ε) / (1 − ε)
   ```

   This is 0.095 at ε = 0.025 and 0.158 at ε = 0.05. It is not ε: the posterior spends its ramp
   inside the delivered send's own RTT spread. An epoch whose send is lost copies with
   probability → 1, and that copy is useful.
   - Per *named hole*, then: about w·(1 − ε) + ε ≈ 0.12–0.21 copies per resend epoch.
   - Per evaluated *delivered* symbol (any never-lost seq the slot reaches near its report time):
     w. If the candidate set reached every unreported seq, that would mean ln(1/ε) ≈ 3.7 redundant
     copies per useful one. The candidate set of §5 must therefore be the named holes only.
     §3.4's localisation rule ("retransmit once the loss is confirmed, repair while it is
     uncertain") says the same.
4. **No constant and no mode.** The only symbols are the posterior and the uniform. No δ or ρ
   enters, so there is no threshold on either and no hint branch.

**The ρ seat inside this slot is debt, not a dependency.** The `pol.shed_on` block in the slot
(`emit_source.rs:760–795`) is the give-up seat switch (§11.3, give-up row; ledger M04 and related
rows). This law does not build on it. Composition: the shed test runs first, on the original age
against D(δ), exactly as today. A seq it sheds leaves the candidate set, and its posterior is moot.
A seq refused by the ρ budget falls through to S06′ unchanged. When the seat switch is replaced by
the continuous give-up law, S06′ does not change.

## 5. Which seq a slot evaluates, and the state

**The premise of §3.2 fails as wired.** "The smoothness comes from the spread of symbol ages in
the buffer" assumes that the slot sees many seqs. The code reads only `first()`. That is the
receiver's last reported `next_expected`, the cumulative-ack blocker, so it is never SACKed as of
the latest report. With the age measured from the original send, it is also the seq whose π is
most over-stated.

**The candidate set: the named holes.** A named hole is a seq with ack < k ≤ h that is not
covered by any SACK range of the latest report and is not shed. The cumulative blocker `first()`
is one of them. Two other kinds of seq are not candidates:
- A SACKed seq has π = 0 by evidence, so the law excludes it without a branch.
- A seq above the horizon has π = Π ε_j (the prior, §2). Its loss is not localised, so it belongs
  to coded repair (§3.4). Making it a candidate would copy w ≈ 0.1 of every delivered symbol (§4,
  property 3).

The slot evaluates every named hole and takes the one with the largest π − U_{k,m} ≥ 0. If there
is none, the slot carries a coded symbol.

**Cost.**
- There are at most `MAX_SACK_RANGES` + 1 = 65 named holes per report: one gap before each range,
  plus the blocker.
- The set is rebuilt per report, as `sack_to_gaps` already does for the NACK loop, in
  O(ranges).
- One evaluation is O(65) per slot, constant in N, in the window size and in `retx_buf`.
- Memory is O(65) beyond the per-seq state below.

**SACK must be excluded, and the state must not depend on a gate.** The sender's SACK record
(`sack_released`, `net/mod.rs:1911`) is filled only when `store_sack_release_on`. That is
`reliable && !generation && !coded_only && gate` (`sender_policy.rs:627–628`). If S06′ read it as
it stands, its evidence would be selected by the ρ boolean. That is a mode switch by the back door.

So the engine requirement is a per-seq "covered by a SACK range" predicate, kept from the
reports on every seat. For example, the `SackMemo` interval set, maintained independently of
whether the store release consumes it.

A SACKed seq can still be evicted at the receiver (`sack.rs` comment on the store release). The
NACK path, which serves from `sent_store`, stays its backstop.

**Per-seq state (the requirement).** Extend `nack_retx_at: seq → (t_last, path_last)` to
`seq → (t_last, path_last, m, ε_last)`. That is 8 B more per resent seq. The map is pruned at
the cumulative ack (`sender_phases.rs:1434`), so its size is ≤ |retransmit_buffer|.

The original's (t_0, ε_0, path_0) stays in `retransmit_buffer`. The ε stored should be the send
path's, not the worst path's.

**The approximation, with its direction.** The state holds the original and the newest resend. It
drops the factors of intermediate resends.

```text
   π̂_k  =  P_lost(a_0) · P_lost(a_m)   ≥   π_k
```

Every dropped factor is ≤ 1, so π̂ over-states π and can only bring a copy forward. Once the newest
resend is older than about SRTT + 3·RTTVAR, every dropped factor is ≥ the newest's and ≈ 1, so the
error is in the 3rd significant figure.

The worst case is two resends younger than SRTT: an immediate re-fire, probability ≈ ε_c. Then π̂
over-states by 1/ε_c for one epoch. That costs at most the one extra copy that property 1 already
counts. Keeping the two newest resends removes the case, at 12 B more per seq (Q5).

**Side effects of a copy stamping the clock.** They are intended, and the request-copy producer at
`net/mod.rs:3493` already does the same.
- (a) The NACK cooldown (`sender_phases.rs:871`) suppresses a NACK for k within SRTT of a copy.
- (b) `mp_flight` (`:884`) clocks the copy's flight on its own path. That is the "live flight" the
  hole law should see.
- (c) The packet-threshold fast path is disabled for k (`!nack_retx_at.contains_key`, `:913`). This
  is by design: wire order ≠ seq order for resent seqs.
- (d) `hold_echo.on_retx` must be called beside the stamp, as both other producers do.

## 6. Predictions (arithmetic only; Auto, 6-row medians)

The slots are fixed: the debt accrues rr per source, and S06′ does not change that. So
`coded = slots − copies`, and the wire volume does not move to first order.

Let L be the source losses per source, ε_c the loss of the path carrying a copy, and E the
number of resend epochs a hole needs.

With w(ε) = ε·ln(1/ε)/(1 − ε), the integrated waste per delivered epoch from §4 (property 3):

- **Lower edge: the NACK always fires first.** Every hole's first epoch is a NACK resend. The copy
  law adds w(ε_c)·(1 − ε_c) redundant copies plus ε_c useful ones per epoch. So copies ≈
  L·(w·(1 − ε_c) + ε_c) ≈ 0.12 L at ε_c = 0.025, and 0.20 L at 0.05.
- **Upper edge: the copy always wins the first epoch.** Copies replace NACKs, so copies ≈
  L·(1 + w·(1 − ε_c) + ε_c)/(1 − ε_c), and NACK retx falls by about L.
- **Either way,** copies + NACK ≈ L·(1 + w(1−ε) + ε)/(1 − ε) + W_cal. Here W_cal is the
  calibration waste of §3, from reports later than the Gaussian model allows.
- **No floor from never-lost seqs.** Only named holes are candidates, so the r·w floor that would
  come from evaluating delivered symbols at their report time does not arise. It would be
  0.019 per source at c2 and 0.023–0.038 at c8.

| per source symbol | c2 measured | c2 under S06′ | c8 measured | c8 under S06′ |
|---|---|---|---|---|
| L (wire loss) | 0.0252 | — | 0.0293 | — |
| ε_c used | — | 0.025 | — | 0.025–0.05 (p0 / p1) |
| slots (consumed r) | 0.198 | 0.198 | 0.242 | 0.242 |
| taper copies | 0.126 | 0.0030 – 0.029, + W_cal | 0.238 | 0.0034 – 0.037, + W_cal |
| coded | 0.072 | 0.169 – 0.195, − W_cal | 0.0039 | 0.205 – 0.239, − W_cal |
| copies per source loss | 5.0 | 0.12 – 1.15 | 8.1 | 0.12 – 1.26 |

So at c8 the proactive coded channel goes from 1.6 % of slots to 85–99 % of them. At c2 it goes
from 36 % to 85–99 %.

**What a run must answer.**

1. **Where in the band each cell lands.** This is the race between packet-threshold or
   time-threshold NACK detection (about 1 RTT, C01, C02) and the posterior's knee (about SRTT +
   2·RTTVAR on the newest send). Its timing distribution is not printed.
2. **W_cal.** It comes from report delays outside the Gaussian model: dwell in `est.rtt()` and
   RTTVAR = 0.1·SRTT. It is bounded by one copy per epoch, so it is at most (hole lifetime ÷ epoch
   length) per hole. For a 5–7 s c8 stall at SRTT 0.26–0.6 s, that is 8–27 copies, against about
   137 today (brief). It cannot be smaller than 0 or predicted without an age-at-copy gauge.
3. **Whether the freed coded slots are useful.** At c8, r ≈ 0.24 against L ≈ 0.03. The trailing
   span can use at most about L of them, so most of the coded symbols may be wasted too. That is
   the rate law's over-provision (R01, R02, r_sat), which this law exposes rather than fixes.
4. **Goodput and in-order delay, at c2 and c8.** In particular, whether the c8 blocker age and
   `retx_buf` collapse once the stale copies stop. Status calls the ages an inference.
5. **The NACK count once the cooldown is shared.** It is in the band, not pinned.

## 7. Proposed paper text

**§3.2, replace from "The transition is sharp;" to the end of the section with:**

> P_lost is the posterior of a symbol sent once. A symbol already resent has more evidence: each
> send j, at age a_j on path p_j, is either lost or delivered with its report not yet back, and the
> sends are independent under the same channel model. The probability that symbol k is still
> missing is then the product of one P_lost factor per send:
>
> ```text
>    π_k(t)  =  Π_{j=0}^{m} P_lost(t − t_j ; ε_{p_j}, SRTT_{p_j}, RTTVAR_{p_j})    for k within the latest report's horizon
>    π_k     =  0 once a report covers k ;   π_k = Π_j ε_{p_j} above the horizon (no report has spoken)
> ```
>
> With one send this is P_lost. A young resend brings π_k down to about its own path's ε. π_k comes
> back to one only when that resend's report is overdue. Resends that are all overdue leave π_k at
> one, however many there are.
>
> **The copy decision is per symbol, not per slot.** Each symbol draws one uniform U_{k,m} per send
> epoch, and a correction slot copies the candidate k once π_k reaches U_{k,m}. Otherwise the slot
> carries a coded symbol. A symbol is therefore copied with probability π_k by the time its report
> is overdue, and at most once per epoch, whatever the slot rate.
>
> A draw per slot repeats the same question hundreds of times per RTT. That was the shipped defect:
> copies were 5–8 per wire loss, and the coded channel was shut at two paths (status §51 block 51,
> Finding 1).
>
> The candidates are the named holes: seqs inside the latest report's horizon that it neither
> acks nor SACKs. A seq whose loss is not localised gets coded repair (Section 3.4). Every send
> stamps one per-seq clock (time, path, count), which the NACK cooldown reads too.
>
> For an epoch whose send is in fact delivered, the copy fires before its report with probability
> E[P_lost(RTT)] = ε·ln(1/ε)/(1 − ε): 0.095 at ε = 0.025. An epoch whose send is lost copies with
> probability one. The law has no δ or ρ input. The dial reaches it only through the number of
> slots r(β(δ)).
>
> The expected-DoF comparison, "copy iff π_k exceeds the chance u that a coded symbol supplies a
> missing DoF", is the target form. It needs a measured u (the DoF ledger of Section 4.13). The
> model's u = 1 − P_fec(r) is about 10⁻⁴⁰ at the operating rates, which would turn every slot into
> a copy.

**§3.2, the last paragraph** ("In the shipped window machine …") is replaced by the engine state,
once it ships, and the "measured inert" sentence is withdrawn. Status §51 measured the copy firing
on every off-Bulk row.

**A.2, add a line under P_lost:**

```text
   π_k(t)     =  Π_j P_lost(t − t_j ; ε_{p_j}, SRTT_{p_j}, RTTVAR_{p_j})  (named holes) ;  copy iff π_k ≥ U_{k,m}      p_miss
```

**§11.3, P_lost inputs row:** add "per-send path inputs; the original's ε is the worst path's at
send (`emit_source.rs:499`)" until it is fixed.

## 8. Proposed law-ledger S06 entry (verdict words from §1's vocabulary only)

These are proposals for the ledger owner. I did not edit the md or the yaml.

- *2. formula (code):* unchanged until the engine change ships.
- *3. paper:* A.2 P_lost and the proposed π_k (`p_miss`); §3.2 as amended above; §4.13 for the
  target u.
- *4. derivation:* `docs/derivations/s06-plost-resend.md` (this document).
- *5. measured in regime?:* fires on every off-Bulk row of §51 block 51. Copies are 5.0× (c2) and
  8.1× (c8) source losses on top of NACK retx ≈ loss. The coded share is 1.6 % of slots at c8.
  "Never fires" is withdrawn.
- *6. clamp bind:* the [0, 1] clamp binds at 1 at c8 by the brief's age reading. **No gauge**:
  add a `plost_p` bind fraction, the share of slot evaluations with p ≥ 0.999, before tagging
  PINNED.
- *7. inputs honest?:* unchanged (E01, E09), plus the original's ε is the worst path's at send.
- *9. verdict:* **DIVERGENT, DISHONEST-INPUT**, plus **PINNED(c8)** once the bind gauge reads it.
  - DIVERGENT: the code evaluates the cumulative-ack blocker on its first-send age, with a per-slot
    draw. The derived law is a per-symbol posterior over every send, read only for the named holes
    inside the report horizon, with one draw per epoch.
    The paper's own §3.2 text lacks the resend conditioning, so the paper is amended too.
  - DEGENERATE(L1: never fires) is withdrawn.
- *10. fix class, priority:* law change needing a battery, plus paper.
  - **P1** is proposed (owner call): the default Auto path, wrong by a factor of 5–8 at both measured
    cells, and it shuts the coded channel at two paths.
  - Depends on E01 and E09 (per-path honest SRTT and RTTVAR, for W_cal), and on a new requirement:
    gate-independent SACK state at the sender.
- *flags:* measured yes, gauge partial (copies yes, ages no, bind no).

## 9. Tests the engine change needs (red first)

Each test fails on `87e646f7`, or on the change minus its mechanism, before it passes.

1. **`raptorpath-math`: `p_miss_is_the_product_of_per_send_p_lost`** (absolute values).
   - m = 0 equals `p_lost` bit for bit.
   - The §3 tables: 0.0250 / 0.529 / 0.998 at a_1 = 0 / 60 / 70 ms, and ≥ 1 − 10⁻¹² for three
     overdue sends.
   - ε = 0 gives 0, and a covered seq gives 0.
   - **N = 1** (one path's moments for all sends) and **N = 2** (the 0.025 / 0.867 row of §3) are
     absolute.
   - The value is invariant to the order of sends.
2. **`raptorpath-math`: `p_miss_draw_is_once_per_epoch`.** U_{k,m} is a pure function of (k, m).
   1 000 evaluations at a fixed state give one decision, and incrementing m changes the draw.
3. **Engine, host loopback, N = 1 and N = 2: `taper_copies_per_seq_bounded_under_stalled_cum_ack`.**
   - **Preconditions, each with a witness.** Without them the slot never fires, even on main.
     `p_lost` returns 0 at ε ≤ 0, and `loss_rate()` is the TX EWMA (`estimator.rs:498`), which a
     single drop leaves near 0. The paper already records `taper_copy = 0` on both loopback
     topologies.
     - Warm the loss estimate with seeded i.i.d. loss of about 2.5 % for at least a few RTTs.
     - Run at an off-Bulk δ (Auto), so that rr > 0.
     - Assert that the slot count and `eps_at_send` of seq k are both > 0 before the stall starts.
   - Then drop seq k's original and its first resend.
   - Have the receiver withhold the cumulative ack past k for T = 20·SRTT, while still SACKing above
     it.
   - Assert copies(k) + NACK(k) ≤ ⌈T / SRTT⌉ + 2. Assert that no SACKed seq and no seq above the
     report horizon is ever copied. Both are absolute counts.
   - **Red on main:** roughly one copy of k per slot over T.
   - It must also print the mechanism witness (MEASUREMENT DISCIPLINE rule 1): the stamps that the
     copies wrote to `nack_retx_at`.
4. **Engine: `taper_copy_stamps_the_resend_clock`.**
   - After a copy, `nack_retx_at[k]` = (now, path, m + 1).
   - A gap report for k within SRTT increments `mpd_supp_cool` and sends no NACK.
   - Red on main, which writes no stamp.
5. **Engine: `copy_evidence_does_not_follow_the_rho_seat`.** The same loss trace is run with
   `store_sack_release_on` true and false. A SACKed seq is never copied in either. Red on main
   only once the candidate set is widened; on main it guards the gate-independent SACK state.
6. **Continuity through the δ presets (CLAUDE.md).**
   - `p_miss` takes no δ or ρ. Its continuity test asserts identical output for every δ in
     {Bulk, Auto, Realtime} × {0.98, 1, 1.02}: a compile-level fact made explicit, and it extends
     the gate list.
   - At the engine, run test 3's trace at the nine δ values and assert per preset triple
     {0.98, 1, 1.02}·δ_p. One assertion over all nine values would be false, because Bulk has
     rr = 0 and so no slots and no copies.
     - **Auto and Realtime triples (slots plentiful):** copies per named hole are identical within
       a triple, as absolute counts. The copy share of slots differs by no more than the slot
       count's own change across the triple.
     - **Bulk triple (slot-starved):** at every point, copies ≤ slots. Copies and coded may both be
       0; the copy share is then undefined, not a step. Assert that the copy count is
       non-decreasing in the slot count across the triple, with no jump larger than the slot-count
       change.
   - This adds to `raptorpath-visualizer/test_visualizer.mjs`'s preset sweep only if the wasm model
     gains a copy decision. I did not check whether it has one.

The change touches the engine, so it also needs the decision pins and the host ns bench (scope
rules). It is a deliberate behaviour change, so the §41c guard does not apply. It is not a
refactor.

## 10. Open questions for the owner

- **Q1. Target form or shippable form?**
  - The Bayes form "copy iff π > u" is the right comparison, but u needs §4.13's ledger, and the
    model's u ≈ 10⁻⁴⁰ is degenerate.
  - The proposal ships the per-epoch draw now and records the Bayes form as the target.
  - Alternative: ship nothing until §4.13. Copies stay 5–8× losses until then.
- **Q2. Per-path SRTT and RTTVAR (E01, E09) before or with S06′?** Without them W_cal is unbounded
  except by the epoch rule. With today's worst-path inputs, the fast-leg epochs at c8 run on the
  slow leg's clock. That is late, not wasteful, while the worst-ε leg is the slow one. Where the
  worst-ε leg is the fast one, the error flips to early copies, which is waste.
- **Q7. Use v9's `received_above`?** The report's aggregate count above the frontier is evidence
  about seqs past the horizon, as a count rather than per seq. A mean-field factor could sharpen
  the prior there. It is not needed while those seqs are not candidates.
- **Q3. Gate-independent SACK state at the sender.** It is required by the law, and today it exists
  only under the store-release gate, which is selected by `reliable`. Who owns it: S06, or the
  store-release row?
- **Q4. Coded rows covering k.** Should π_k carry a factor for coded repairs whose span meets k?
  That is §4.13's a_c. It is not needed for the old-blocker case, and it is needed once the
  candidate set reaches the trailing span.
- **Q5. One resend or two in the per-seq state?** Two removes the one-epoch over-statement after
  an immediate re-fire, at 12 B more per resent seq.
- **Q6. The rate law's over-provision at Auto** (r ≈ 0.2–0.24 against loss 0.025–0.03, R01/R02):
  S06′ moves 87–99 % of slots to coded repair, which may be mostly wasted. Should S06′ ship with,
  or after, a rate-law verdict, so that the run does not read a rate-law waste as a copy-law
  result?
