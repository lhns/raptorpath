# The verdict law for the evicting seat (derivation, no code)

Roadmap node `rec-evict-blocker-unresolved`. Owner decision of 2026-11-20:
fix the defect with one rule, "every abandoned seq gets a verdict and the
receiver is told". Inputs: `evict-fix-prechecks.md`, `core-extraction-recovery.md`,
ADR-0064, ADR-0079 with Amendment 1, ADR-0081 (and its draft on
`recovery-counters` e6355bec, which adds the `E` term), plan S4,
`liveness-audit.md` item 4, and paper §4.10, §5.1, §5.6, §6.1 and §6.4.
Code read on github/main `1ef0f634`. Paths are under `raptorpath/src/`
unless they start with `tests/` or `docs/`.

The precheck numbers are one seed, run on Windows before the port rekey, in
a simulation with no CPU cost. Here they show direction, not size, and no
test below pins any of them.

FORMULA-FIRST: each law appears on its own line next to the sentence it
implements, and each symbol has a provenance line. Section 8 holds the paper
text. Nothing here changes code.

## 0. Two words, one partition

- **Verdict** (sender side). The sender declares "I will not serve this seq
  again". A seq gets a verdict in one of two ways:
  - a **shed** (`H`), which is discretionary and can be refused by the budget;
  - a **forced verdict** (`E`), taken when the last servable copy is gone.
    The coding window evicts the seq while it is unresolved and no store
    holds it, so this verdict cannot be refused.
- **Abandoned** (receiver side). The receiver's in-order frontier passed the
  seq without delivering it. Only the receiver knows this. At ρ < 1 the
  cumulative ack also passes holes the receiver gave up on. So from the ack
  alone the sender cannot tell a delivered seq from an abandoned one. On
  `recovery-counters` the lost tail reads `A = 1`, `E = 11 799`, and nearly
  all of those seqs were delivered.

**The 1 − ρ budget counts abandonment, not verdicts.** A verdict is only an
upper bound on it.

The sender's partition of the source seqs it has sent, U being the
*unresolved* set (sent, not acked, no verdict):

```text
   S_sent  =  A  +  V  +  |U|          V = H + E,  each seq in exactly one term
```

- A seq with a verdict stays in `V`, even when a later cumulative ack covers
  it. Its delivery or abandonment is the receiver's fact (§4).
- ADR-0081's `O_serv + O_strand = |U|`. Part 2 below makes `O_strand ≡ 0` by
  construction, because no unresolved seq can lie below the servable span.
- At ρ = 1, `V = 0` by data. The store holds every unresolved seq, so `E = 0`,
  and the budget is 0, so `H = 0`.

## 1. The verdict rule (parts 1 and 2 are one change)

Parts 1 and 2 ship together. A shed judged on the unclamped gap that never
tells the receiver turns "the sender stalls" into "the receiver waits". The
precheck shows it: in `sr_evict_b05` the untold shed left the ack at 12 087,
while refuse-and-resend in `sr_evict` reached 12 147.

### 1.1 Forced verdict at the eviction sites

"A seq whose last servable copy leaves the sender gets a verdict at that
instant."

```text
   evict(s)  ∧  s ∈ U  ∧  s ∉ store   ⇒   U ← U ∖ {s},  E ← E + 1,  record s ∈ V
```

- The sites are the two window advances: the size cap at `net/mod.rs:3594–3597`
  and the ack-relative slide at `net/sender_phases.rs:1374`.
- The slide cannot reach an unresolved seq: it advances to `ack − keep_behind`,
  which is below the ack. So it is a witness only.
- A forced verdict removes `s` everywhere a seq is pruned today at a shed:
  `retransmit_buffer`, `nack_retx_at`, and `source_path_map` (already pruned
  at `:3601`).
- At ρ = 1 the store holds `s`, so the antecedent is false by data. The
  formula reads no seat bit.

### 1.2 Judging a reported gap

"Every unresolved seq in a reported gap is judged on the unclamped gap. The
clamp only bounds what is served."

```text
   for s ∈ [g_s, g_e] ∩ U :                                     (no clamp here)
      verdict(s) = SHED     if  age(s) > D  ∧  Ab + 1 ≤ (1 − ρ)·S_src
                 = SERVE    else if  s ∈ [f, c]                 (clamp_gap's servable span)
                 = FORCED   otherwise                           (counted, never refused)
```

- Seqs in the gap that are not in U (acked or already given a verdict) are
  skipped. The floor covers them (§2).
- After §1.1, `U ⊆ [f, c]` holds at every instant, so the FORCED arm is empty
  by construction at the NACK and sweep sites. It is kept as the rule's
  closure, with a gauge that must read 0 (§7).
- The clamp `clamp_gap(s, e, f, c)` (core-extraction §1.1) is unchanged. It
  now runs after the shed check, on the seqs the shed check left to serve.
- The shed arm is ADR-0064's `shed_allowed`. Only its budget operand changes
  (§4): `Ab` and `(1 − ρ)` replace `shed_total` and `ε̂·(1 − P_fec)`.

The tail sweep target is unchanged: `tail_sweep_gap(min U)` (core-extraction
§1.2). After §1.1, `min U` is always servable, so the D1c defect (a sweep
aimed below the window) cannot occur. Its gap goes through §1.2.

### 1.3 The deadline D: refreshed where verdicts are taken

"D is the span law's own deadline at the live RTT, read when the verdict is
taken."

```text
   D  =  min( b(δ)·RTT_now , 2·RTT_now )        evaluated once per judged batch and per sweep fire
```

- Today D is refreshed only inside `emit_source` (`net/emit_source.rs:630–633`),
  so it freezes at the last source emission: `D = 13 ms` beside a live
  `rtt = 844 ms`.
- The RTT operand is `est.rtt()`, the EWMA RTT. The parameter is named
  `rtprop_us` and the doc comment on `shed_deadline_us` (`net/shed.rs`) says
  RTprop. This is a documentation divergence, so the refresh does change D
  under queueing.
- `est.rtt()` returns the 50 ms constructor seed until an RTT is measured
  (`control/estimator.rs:527–533`). The refresh must read the measured
  accessor instead. If no RTT has been measured, `D = 0`, and
  `shed_allowed`'s `deadline_us > 0` makes nothing sheddable: the seq is
  served. A seed is not an anchor (MEASUREMENT DISCIPLINE rule 1).
- The deadline does not bound how much is shed. Under queueing, every queued
  seq is past `D = RTT/2`. Only the budget term bounds the count.

### 1.4 Provenance

| symbol | provenance |
|---|---|
| U, A, H, E, S_src | measured state (sender counters; ADR-0081 sites) |
| age(s) | measured: `now − send_ts(s)` from `retransmit_buffer` |
| b(δ), the min with 2·RTT | derived, paper §5.3/§5.4 (ADR-0064 decision 2) |
| RTT_now | measured: the estimator's measured EWMA RTT; no seed |
| [f, c] | measured state: `servable_span(window, store)` (core-extraction §1.1) |
| ρ | declared dial (§3.4 says what must supply it) |
| Ab | measured (§4) |

## 2. The receiver floor

### 2.1 What the sender asserts

"Every seq below the floor is acked or has a verdict."

```text
   F  =  min U        if U ≠ ∅
   F  =  hi + 1       if U = ∅              hi = the highest source seq sent
```

At ρ = 1 with `V = 0`, `F = A_rx`, the receiver's last advertised
`next_expected`:
- every seq below `A_rx` is pruned from U;
- `A_rx` itself is unacked and in U if it was sent, and `F = hi + 1 = A_rx`
  otherwise.

So `F > A_rx` happens only after a verdict, and the floor is never sent at
ρ = 1. This is by data, not by a seat check, and a gauge witnesses it (§7).

### 2.2 When the sender sends it, and the ack

"While the floor is above the receiver's acked frontier, the sender repeats
it on the tail-sweep clock. The receiver's cumulative ack acknowledges it."

```text
   send WindowFloor{F} on every live path   at each tail-sweep fire  while  F > A_rx
   sweep armed  ⇔  U ≠ ∅  ∨  F > A_rx                      (today: U ≠ ∅)
   sender exit held  while  F > A_rx
   acked  ⇔  WindowAck.next_expected ≥ F
```

- **The arming change is required.** A verdict on the last unresolved seq
  empties `retransmit_buffer`, which today disarms the sweep
  (`net/mod.rs:2788–2813`). That reproduces the 12 087 stall. With `F > A_rx`
  in the arming predicate, one condition covers both cases: data follows, and
  no data follows. At ρ = 1 the new disjunct is false by §2.1, so arming is
  unchanged there.
- **Cadence.** The cadence is `tail_sweep_timeout_us(srtt) = clamp(2·SRTT,
  TAIL_SWEEP_MIN_US, TAIL_SWEEP_MAX_US)` (`net/recovery_laws.rs:331`). It is
  the existing law, so no new constant is introduced. A raise of F does not
  trigger an immediate send. Raises can come in bursts of thousands (every
  forced verdict raises F), and the receiver already passes holes that have
  data above them on its own hold.
- **Delivery after n repeats** has probability `1 − p_ctrl^n`, with `p_ctrl`
  the control datagram loss on the worst live path.
- **The ack already exists.** It is the bare `WindowAck` the receiver sends
  when its frontier moves with no arrival (`net/receiver.rs:1026–1047`). That
  ack is sent once and can be lost. So the receiver answers **every**
  `WindowFloor` with a bare `WindowAck` once its `next_expected ≥ F`. The
  sender's repeat then also repairs a lost ack.

### 2.3 The receiver's rule for passing a hole below the floor

"A floor received on every live path is treated as a payload-free arrival at
seq F, under the same hold and the same expiry as a data arrival."

```text
   F_p   =  max floor received on path p            (0 before the first)
   F_rx  =  min_{p ∈ P_live} F_p
   on F_rx rising above next_deliver:  insert marker (F_rx, t = now) into the reorder hold
   drain_expired: the marker expires at t + H, exactly like a pending entry;
                  it passes the holes below F_rx and delivers the pending entries below it in order;
                  the frontier moves to F_rx (not F_rx + 1: the marker carries no payload)
```

- **The in-flight bound is per-path FIFO.** A `WindowFloor` on path p is a
  datagram on the same connection and the same bottleneck queue as earlier
  data on p. So when it arrives, every seq below F sent on p has either
  arrived or been lost, up to reordering within the path.
- The hold H covers that reordering. It is the receiver's existing hold
  (`shed_recv_hold`: `SRTT/2` while the receiver's budget is open,
  `(4·SRTT).clamp(60, 300) ms` otherwise). No new constant is introduced.
- The exposure is no larger than what a data arrival at seq F already causes
  today. This answers the precheck's contradiction: an eviction while 8 000
  symbols were in flight. Those symbols precede the floor on their path, so
  they arrive before it.
- **The marker arms the drain timer itself.** Today no timer runs without a
  pending entry (`net/reorder.rs:181–189`).
- **The pass reads no seat bit.** `drain_expired` returns early when
  `reliable` (`net/reorder.rs:117–119`). The floor pass therefore goes in its
  own function, which runs on both seats. At ρ = 1 it never fires, because
  floors are never sent (§2.1).
- **Edge.** A blackholed path whose connection is still open holds `F_rx`
  down until QUIC's idle timeout closes the connection. That is bounded, but
  not by this law. It is tested in §6, T5.

### 2.4 The wire change: yes, v11

The coordinator asked whether repair symbols already carry the coding
window's start. They do, but it cannot serve as the floor:
- **The full-window repair** stamps the window front:
  `[window_start(8)][window_count(2)][repair_index(4)]`
  (`fec/rlc_window.rs:11`, `:113–128`).
- **The trailing-span repair** (`generate_repair_range`, `:142`) stamps the
  span start. That is not the window start.
- **At ρ = 1 the window start is not a floor at all.** The store serves
  below it.
- **Using the header as the floor** would make the receiver's rule depend on
  the seat. It would also be silent exactly when no data follows, which is
  the case that stalls.

So the floor rides its own message:

```rust
ControlMessage::WindowFloor { floor: u64 }   // appended after RepairRequest (tag = declaration index)
WindowAck { .., passed_undelivered: u64 }     // §4: the receiver's abandonment count
```

- Both changes go in one bump, `PROTOCOL_VERSION` 10 → 11, with a v11 entry
  in the version-history comment (`transport/protocol.rs:25–104`).
- The change extends **ADR-0010**, the version header and the hard refusal
  of a mismatch.
- The rule itself, a new control message and a new ack field, is a
  hard-to-reverse protocol decision. It should get its own ADR, proposed as
  **ADR-0084, "Every verdict reaches the receiver"**. That ADR also amends
  ADR-0081's identity (§0), and its field supersedes the ack counter
  `cumulative_received` for this purpose.
- **`cumulative_received` cannot carry the count.** On in-order streams it is
  the per-path received count; on OOO streams it is the delivered-set size
  (`net/receiver.rs:2022–2028`). It is 0 on timer acks. So the sender cannot
  derive passed-undelivered from it.

## 3. The source-rate bound

### 3.1 Why `reliable &&` is there (git history)

- **The gate came with the store.** `4c901536` (2026-07-06, "RWM Phase A")
  introduced it as `store_backpressure(reliable, store_len) = reliable &&
  store_len >= RELIABLE_STORE_MAX`. The doc comment reads: "EVICT mode never
  backpressures on retention (Realtime's correct spend of the budget)". It
  was a *store-full* pause, and the store existed only at ρ = 1. The gate
  records where the bytes were. It was not a flow-control decision for
  ρ < 1.
- **`cwnd_full` was added later without a reason for its scope.** `92e2f780`
  (2026-07-07, a generation-mode diagnosis) put it inside the same
  parenthesis, giving `reliable && (store_len >= store_max || cwnd_full)`.
  The commit says nothing about restricting it to the reliable seat.
- **The present form** is blamed to `ae83022e` (2026-09-27), the
  `effective_store_cap` rename.

So no derivation ever made the ρ < 1 source unbounded. It is a leftover of
where the bytes live.

### 3.2 The candidates, against four checks

The four checks:
1. **(i)** The form is exactly today's at ρ = 1 under the default gates.
2. **(ii)** It binds at ρ < 1 on the shipped arm.
3. **(iii)** It bounds unresolved seqs, not only bytes in flight.
4. **(iv)** Every constant in it has a source.

**(a) The congestion window gating intake at every ρ.** This is the simplest
form, with no ρ in it:

```text
   P  =  (O_store ≥ B)  ∨  F_cwnd ,      F_cwnd = ∀ i ∈ live: in_flightᵢ ≥ cwndᵢ      (no reliable &&)
```

- (i) It is exact only if the brake stays as armed as today, and today it is
  unarmed by default (`RWM_LATE_BRAKE` off, `RWM_INFL_CAP = 0`).
  - Left unarmed, it binds nothing at ρ < 1 on the shipped arm:
    `O_store = 0` (no store) and `F_cwnd = false`. That is core-extraction
    §1.3's redundancy-by-data.
  - Arming it by default at every ρ changes ρ = 1. The brake alone was never
    measured as a default; its only verdict came inside the composed cap,
    which was refuted in 2026-08 (paper §10).
  - Arming it at ρ < 1 only is a seat switch.
- (ii) It binds if armed. `in_flightᵢ` is maintained by `expire_in_flight` on
  every path (`net/mod.rs:2421`). That it is maintained on the evicting
  seat's send paths is **[I]**, and needs a witness.
- (iii) It fails. A lost seq leaves `in_flight` when it expires but stays in
  U, so the brake bounds the pipe and not the unresolved set.
- (iv) It passes: cwnd is derived by the CC.

**(b) The unresolved count against the pooled cap at every ρ (recommended):**

```text
   P  =  (O_U ≥ B)  ∨  F_cwnd ,      O_U = |U| − |released ∩ U| ,      B = §6.1's pooled cap, at every ρ
```

- (i) It is exact by data, with one pin. At ρ = 1 on the plain arm,
  `|U| = sent_store.len()`. Both are inserted once per source emission
  (`net/emit_source.rs:347`, `:505`) and pruned on the same cumulative twin
  (`net/sender_phases.rs:1403`, `:1411`), and `H = E = 0`. So
  `O_U = sack_release_outstanding(sent_store.len(), released)`, today's
  `store_len`.
  - **[I]**: this needs a per-tick equality pin at ρ = 1 before it ships.
  - The generation and coded-only arms keep their own O. Both require
    ρ = 1 (`config.rs:304`).
  - `F_cwnd` and its arming are untouched.
- (ii) It binds. B is today's `effective_store_cap`. At ρ < 1 that
  needs `plain_dyn_cap` without its `reliable &&` (`net/sender_policy.rs:521`),
  so the cap law (1 + q(δ))·Σ bwᵢ·RTpropᵢ runs on both seats. The law has no
  ρ input (paper §6.1). U grows when the source outruns the acks, so the
  pause stops the unpaced 44 ms emission of the precheck.
- (iii) It passes: O_U counts unresolved seqs by definition.
- (iv) Partly.
  - The law's terms are cited or derived (§6.1 table).
  - `knee` is a resource bound whose value is stale.
  - `RELIABLE_STORE_MAX = 1024` has no provenance ("≈ 10× the C2 BDP",
    `4c901536`). It does not ship as a law term: it is a memory ceiling, and
    it gets a bind gauge (§7).

It has no ρ and no δ beyond B's existing q(δ), so it is continuous by
construction. `released` is empty at ρ < 1 today, because SACK release
requires `reliable` (`net/sender_policy.rs:628`). O_U then over-counts the
seqs received above the frontier. That errs on the safe side; owner
question 8.

**(c) Pause before a forced verdict:**

```text
   P  =  (b)  ∨  [ evict would force a verdict on s_front  ∧  ¬(age > D ∧ Ab + 1 ≤ (1 − ρ)·S_src) ]
```

- It puts ρ into the bound and makes forced verdicts refusable.
- It fails on §0's finding: a forced verdict on an in-flight seq is not
  abandonment, so it throttles intake for a non-loss.
- It inherits the cumulative budget's burst. After a long clean run,
  `(1 − ρ − R)·S_src` of headroom lets the window run ahead by thousands.
  That is the precheck's queue overflow again.
- It is not recommended. It is the right form only if the owner wants forced
  verdicts refusable without a store at ρ < 1 (owner question 2 makes that
  unnecessary).

**(d) A store at every ρ (paper §5.1).** `sent_store` fills on both seats and
an entry leaves on ack or on verdict. Then:
- every unresolved seq stays servable;
- `E ≡ 0`, and the FORCED arm is empty everywhere;
- (b)'s `|U| = sent_store` outstanding holds on both seats;
- the `reliable` bool leaves the store, the clamp and the intake pause.

It is the form that removes the ρ = 1 step (§5). It is a larger behaviour
change: a per-source store insert at Realtime and store memory up to B. It is
owner question 2.

**Recommendation: (b) now. (d) as the next step if the owner accepts it.**

### 3.3 Which part of the budget the rate bound enforces

The rate bound reads no ρ, so it enforces no part of the 1 − ρ budget
directly. What it bounds is the *self-inflicted* term:
- O_U ≤ B keeps the standing queue near q(δ)·RTprop (Little's law, §6.1).
- That removes most queue-overflow losses: 1 110 against 91 random losses in
  the precheck, direction only.
- Those losses would otherwise become receiver abandonment (`G_rx`, §4) or
  forced verdicts.

The budget is enforced by the shed arm on `H` alone. Receiver give-ups and
forced verdicts on truly lost seqs cannot be refused. That is the overload
region of §3.5.

### 3.4 ρ as a number: what must supply it

- **Where ρ is missing.** The engine has no numeric ρ on this seat:
  `contract_rho: f64 = 1.0` is a literal (`net/sender_policy.rs:616`), read by
  no budget site. `window_reliable` is a bool (`config.rs:293–295`).
- **The source.** A declared config field, `window_rho: Option<f64>`, a dial
  next to `window_reliable`. It is carried in `SenderPolicy` as
  `contract_rho`, and its only reader in this law is the shed arm's
  `(1 − ρ)·S_src`.
- **Presets.** Bulk and Auto: ρ = 1. Realtime: **no value has a source in
  the repository.** Paper §4.10's 99.98 % is an achievable value computed for
  one cell, not a contract. No ρ < 1 preset ships until the owner declares
  one (owner question 4).
- **`reliable` is not derived from ρ ≥ 1.** That would be the forbidden
  threshold. The seat bool stays the existing debt (`w3-per-leg-state.md`)
  until (d) removes it. The consistency of the two config values is checked
  at config time, not in a runtime branch (owner question 11).
- **The receiver** needs no ρ under this law: the sender does the counting
  (§4). Supplying ρ to the receiver's own give-up budget would need a
  handshake field, a second wire change (owner question 5).

### 3.5 The overload region (owner question, not chosen here)

Let C be the source rate the live paths carry after the rate law's
correction share, `C = Σᵢ bwᵢ·(1 − ε̂ᵢ) / (1 + r)`. Let λ be the offered
source rate. Abandoning a share `a` of the offered seqs leaves a carried rate
`λ·(1 − a)`. The contract allows abandonment `a ≤ 1 − ρ`, and FEC residue is
predicted to use R of it (§4). So the sender can keep up with λ without
delaying the source and without breaking the contract only while

```text
   λ · ( 1 − (1 − ρ − R)⁺ )  ≤  C
```

Above that line the offered load exceeds what the dial can absorb. Exactly
one of these happens:
1. **The source is delayed.** The ρ contract holds and δ is violated: intake
   backpressure, which is (b)'s behaviour once the shed budget is spent. This
   is ADR-0064's "ρ wins over δ".
2. **The contract is broken.** δ holds and abandonment exceeds 1 − ρ: today's
   ρ < 1 behaviour, unbounded and unreported.
3. **Thin first, then delay.** Abandon at the source up to `(1 − ρ − R)⁺`
   (shed before the first send, which costs no wire), then delay. This needs
   a new source-side shed site.
4. **Break, but report.** As option 2, with the breach as a measured,
   printed reading.

(b) ships option 1 for the sender-side part. Receiver give-ups and forced
verdicts can still push abandonment past 1 − ρ inside the region. With (b),
that is reported (`G_rx` against the budget, §7), not prevented.

## 4. The 1 − ρ budget law

"Abandonment, counted where it happens plus the verdicts not yet resolved,
stays within the declared 1 − ρ of the source. The FEC residue R is the
predicted use of that budget, not its size."

```text
   G_rx    =  seqs the receiver's frontier passed undelivered, minus late fills of those seqs
   Ab      =  G_rx  +  |{ v ∈ V : v ≥ A_rx }|          (verdicts the receiver has not yet passed or acked)
   shed admissible  ⇔  age > D  ∧  Ab + 1 ≤ (1 − ρ) · S_src
   forced verdicts:  counted in Ab, never refused
   predicted use:   û  =  R  =  ε̂ · (1 − P_fec(r_live, ε̂, A*, σ²_burst))
   contract (end of run):  G_rx  ≤  (1 − ρ) · S_src
```

- **Ab is an upper bound that converges to the truth.** A pending verdict
  either becomes a hole (and moves into `G_rx`) or is delivered (and leaves
  Ab) once the receiver's frontier passes it.
- **Double counting.** A verdict on a seq the receiver gives up on before the
  floor arrives counts in both terms. That errs on the safe side and is
  gauged (§7).
- **What this replaces.** It supersedes `budget = min(1 − ρ, R)` (paper §5.6;
  model option (ii), owner decision 2026-11-14, `giveup-law-model.md`). The
  engine evaluates that law today as budget = R on the evicting seat:
  - cold (R = 0 before the first ε̂);
  - frozen (refreshed only on source emission);
  - ρ-blind (liveness audit item 4).
  The new law is never cold: 1 − ρ is declared, and S_src and Ab are
  counters.
- **The model must move too.** The cartoon and wasm give-up law change to
  budget = 1 − ρ. That is a deliberate model change with a
  `GOLDEN_CAPTURE=1` re-capture. The visualizer anchor "= R when 1 − ρ ≥ R"
  (`test_visualizer_ui.mjs:238–241`) becomes "= 1 − ρ". This is owner
  question 3, raised and not assumed.

**The receiver counter exists but is incomplete.** `recv_shed_holes`
(`net/receiver.rs:195`, `:980`, `:1767`):
- counts the frontier's advance minus the entries delivered, at the two
  `drain_expired` sites only;
- misses `force_drain_oldest` (`net/reorder.rs:149–163`) and the new floor
  pass;
- never subtracts late fills below the frontier (`net/reorder.rs:76–82`);
- is gated on `recv_shed_on`.

`G_rx` is that counter extended:
- to all three frontier writers plus the floor pass;
- with late fills of passed seqs subtracted;
- with no gate.

It is carried on every `WindowAck` as `passed_undelivered`.

`undeliverable_symbols` (`net/mod.rs:1452`, the F2 counter) counts symbols
that fail framing. It is unrelated.

**The receiver's own give-up budget stays.** It is `shed_recv_budget_ok`:
`holes < ε̂_recv × frontier` (`net/shed.rs:389`). It has no ρ (owner
question 5), and it is an existing law selection by budget state inside
`shed_recv_hold`.

| symbol | provenance |
|---|---|
| G_rx | measured at the receiver; reported on the ack (v11) |
| V, A_rx, S_src | measured sender state |
| 1 − ρ | declared dial |
| R | derived, §5.6. It inherits W22's `p_fec_normal` cut and is provisional. Here it is a gauge, not an operand |

## 5. Continuity

The formulas:
- **The verdict rule** reads ρ only through `(1 − ρ)·S_src`, which is linear
  in ρ with slope `−S_src`. It reads δ only through D's `b(δ)`, which is
  continuous (§5.4).
- **The floor, F_rx and the rate bound (b)** read neither ρ nor δ. The only
  exception is B's `q(δ)`, which already has its preset tests (§6.1).
- **The budget** `1 − ρ` is Lipschitz-1 and is 0 at ρ = 1.
- No term selects a law by a threshold on δ or ρ.

**The behaviour still steps at ρ = 1, and this is a defect, not shown
continuity.** Retention is a bool. At ρ = 1 − ε:
- the budget admits fewer than one shed while `S_src < 1/ε`;
- yet the seat has no store, so forced verdicts still happen.

At ρ = 1 the store prevents them. In the limit ρ → 1⁻ the evicting seat
therefore abandons seqs that the ρ = 1 seat delivers. Only (d) removes the
step (owner question 2). Until then the step gets a test that bounds it
(T8).

The ±2 % tests to write. Each is absolute, at each preset, in
`raptorpath-core` where the function lives:
- **`shed_budget(ρ, S)`** at ρ ∈ {ρ_p, ρ_p·0.98} and, for ρ_p < 1,
  ρ_p·1.02 clipped to 1. Assert `|Δ budget| = S·|Δρ|` exactly, and that
  `budget(1) = 0`. The nudge at ρ = 1 is one-sided.
- **`D(δ)`** at δ_p·(1 ± 0.02) for Bulk, Auto and Realtime through
  `delta_budget_b_of`. Assert `|D(δ_p·(1 ± 0.02)) / D(δ_p) − 1|` is within
  the tolerance of the existing b(δ) preset test.
  - Note: the engine reads b at the hint's named point (`delta_budget_b(hint)`,
    `net/emit_source.rs:619`). The test therefore runs on the δ-numeric form
    the core takes.
- **`judge_seq`** at fixed state across the same nudges. The verdict may
  change only where `age − D` or `(1 − ρ)·S − (Ab + 1)` changes sign. That
  is a state comparison, not a dial threshold: assert no other flip.
- **`floor_rx(per_path)`** at N = 1 and N = 2: absolute values, including a
  path that has received no floor yet (F_p = 0).
- **`floor(min_u, hi)`** at the empty-U and non-empty cases, absolute.

## 6. Red-first tests

General rules for every test below:
- Liveness at ρ < 1 is asserted on the **frontier and the ack**, never on
  object completion. At ρ < 1 every abandoned seq fails `perf`'s
  `got.len() == total` (`perf.rs:144`), so `client_ok` stays a ρ = 1 oracle
  only.
- The frontier is read from the sender's received `WindowAck.next_expected`
  (a new test-seams hook, or `[ACCT]`). It is not read from `ev=frontier`,
  which prints only on arrival and falls silent when arrivals stop.
- **Order.** Each test asserts the frontier and ack bound **first**, then the
  account. Completion is asserted only in the ρ = 1 control. That way the
  frontier assertion executes (the current `assert_live` fails earlier on
  completion, so its frontier loop never ran).

Execution witnesses carried by every test:
- verdict-site visits (shed, serve, forced);
- `floor_sent`;
- `floor_passes` at the receiver;
- `G_rx`.

The tests:
- **T1 lost tail** (`sim_lost_tail`, k = 11 800, ρ < 1).
  - The ack reaches `S_src` by
    `t_k + n_sweep·tail_sweep_timeout + owd + H_max`, where n_sweep is the
    floor repeats observed.
  - k has a verdict (shed, or forced), or is filled.
  - `floor_sent ≥ 1` and `floor_passes ≥ 1`.
  - Red on main: the ack stays at 11 800.
- **T2 stalled blocker** (`sim_shed_reach`, forced budget 0.05).
  - The ack reaches 12 147.
  - Every shed seq is below the final ack by `t_s + n_sweep·sweep + owd + H_max`.
  - Red on main: the ack stays at 12 087.
- **The replacement for `assert_live`.** `assert_frontier_live(r)`, in this
  order:
  1. ack = S_src within the bound above;
  2. for each verdict seq s: frontier > s by `t_v(s) + n_sweep·sweep + owd + H_max`;
  3. the account closes, `S_sent = A + V + |U|`, with `|U| = 0`;
  4. `G_rx ≤ Ab` at the end;
  5. `G_rx` and `(1 − ρ)·S_src` are printed. They are asserted only where a
     budget was declared.
- **T3 ρ = 1 control.** The same scenarios with `SIM_RELIABLE=true`:
  - `V = 0`, `floor_sent = 0`, `G_rx = 0`;
  - the arming disjunct `F > A_rx` is never true;
  - `client_ok`.
- **T4 floor loss and ack loss.**
  - A seam drops the first n `WindowFloor` datagrams: T1 stays green with
    n + 1 sweeps.
  - A seam drops the bare `WindowAck`: the sender repeats, the receiver
    re-acks, and the sender does not exit before `A_rx ≥ F`.
- **T5 multipath in-flight bound.** N = 2, where the slow path still carries
  seqs below F when the fast path's floor lands.
  - The receiver passes no hole before the slow path's floor plus H.
  - The late fills of passed seqs number 0.
  - A variant blackholes one path: the pass waits for that connection's
    idle close. The bound is reported, not pinned.
- **T6 budget.** At a declared ρ < 1:
  - sheds stop when `Ab + 1 > (1 − ρ)·S_src`, and further candidates are
    served (the refusal counter rises);
  - R and `G_rx / S_src` are printed side by side.
- **T7 rate bound.**
  - At ρ < 1 with the unpaced source: `O_U ≤ B` at every DIAG tick
    (`O_hwm ≤ B`), and `drop_queue` falls against main. The direction is
    asserted, not the size.
  - At ρ = 1: the per-tick pin `O_U == sack_release_outstanding(sent_store.len(), released)`.
- **T8 the ρ = 1 step (record-only until owner question 2).** The same seed
  at ρ = 1 and at ρ = 1 − ε with `ε·S_src < 1`. It reports the difference in
  delivered set and in `E`. It asserts only that `H = 0` on both sides
  (budget < 1).

## 7. Bind gauges

| clamp or min | gauge | expected |
|---|---|---|
| serve clamp `clamp_gap` on U members | gap ∩ U seqs clamped out per call, with call count | 0 after §1.1 (`O_strand ≡ 0`); non-zero is a defect |
| FORCED arm at the NACK and sweep sites | visits vs forced | 0 forced |
| forced verdicts at the eviction sites | `E`, per site, with visit count | > 0 at ρ < 1; = 0 at ρ = 1 |
| D's `min(b·RTT, 2·RTT)` | share of judged batches where 2·RTT binds | 0 at b ≤ 2 (all presets); reported |
| D cold (`D = 0`, no measured RTT) | count of judged batches with D = 0 | early-run only |
| shed admission | refusals split by the deadline conjunct and by the budget conjunct | reported per run |
| intake pause (b) | paused-iteration share split by `O_U ≥ B` and by `F_cwnd` | reported (`[DIAG] paused=`) |
| B's clamp | `[SUMCAP] floor=` / `pin=` (existing), plus `RELIABLE_STORE_MAX` binds | the memory ceiling should not bind |
| floor min over paths | share of floor passes where a non-latest path set `F_rx` | reported |
| floor repeat | sends per `F` raise, until acked | ≈ 1 + p_ctrl/(1 − p_ctrl) |
| receiver hold H | `SHEDH` at_floor / at_cap / interior (existing) | reported |
| Ab double count | verdict seqs also counted in `G_rx` before their floor | reported |
| floor at ρ = 1 | `floor_sent`, and sweep fires armed only by `F > A_rx` | 0 |

## 8. Paper text

The exact paragraphs follow. Each is placed after the named anchor. The
coordinator applies them with the engine change, not before.

**§5.1, appended after the paragraph ending "ρ is not a runtime input
anywhere in the engine (Section 11.1).":**

> *Verdicts.* A sent source seq leaves the sender's unresolved set U in one
> of three ways: acknowledgement, a shed (Section 5.6), or a forced verdict
> when its last servable copy is gone. On the evicting seat the coding window
> is that copy, so a window eviction of an unresolved seq is a verdict at
> that instant.
>
> ```text
>    S_sent  =  A  +  H  +  E  +  |U|            each seq in exactly one term
> ```
>
> A verdict is a sender fact ("I will not serve it"). Abandonment is a
> receiver fact ("the frontier passed it undelivered"). The contract counts
> the second (Section 5.6). At ρ = 1 the store holds every unresolved seq, so
> E = 0 by data. The same data makes the seat a step at ρ → 1⁻ (Section 11.3).
>
> *The floor.* Every verdict reaches the receiver as a floor below which
> every seq is acknowledged or given up:
>
> ```text
>    F  =  min U   (U ≠ ∅),     F  =  hi + 1   (U = ∅)
> ```
>
> The sender repeats `WindowFloor{F}` on every live path at the tail-sweep
> cadence while F exceeds the receiver's acknowledged frontier. The
> receiver's cumulative ack is its acknowledgement. The receiver takes
> `F_rx = min_p F_p` over its live paths and treats it as a payload-free
> arrival at F_rx under its ordinary hold. Per-path FIFO makes every earlier
> seq on that path arrive or be lost first, so the floor exposes no more
> than a data arrival at F does. At ρ = 1, F equals the acknowledged
> frontier whenever no verdict exists, so the floor is never sent.

**§5.6, replacing the law block and the paragraph after it ("R is derived;
… (Section 11.3)."):**

> ```text
>    D(δ)    =  min( b(δ)·RTT , 2·RTT )                  read at the verdict, from the measured RTT
>    G_rx    =  seqs the receiver passed undelivered     (measured at the receiver, on the ack)
>    Ab      =  G_rx  +  #verdicts at or above the acknowledged frontier
>    shed  ⇔  age > D(δ)   ∧   Ab + 1  ≤  (1 − ρ)·src_total
>    R       =  ε̂ · (1 − P_fec(r_live, ε̂, A*, σ²_burst))    predicted use of the budget
> ```
>
> The budget is the declared 1 − ρ. It counts abandonment where it happens,
> at the receiver, plus the verdicts the receiver has not yet resolved. R no
> longer sizes it: R predicts how much of it FEC's residue will use, and
> R > 1 − ρ marks an operating point where the contract can be met only by
> delaying the source. A forced verdict is counted and never refused, and
> past-budget sheds are served: ρ wins over δ. This supersedes
> budget = min(1 − ρ, R) (owner decision 2026-11-14). At ρ = 1 the budget is
> 0 at every δ.

**§6.1, replacing the sentence "The engine also has no ρ < 1 seat for it:
… never evicts above its frontier.":**

> The source bound runs at every ρ on the count of unresolved seqs, sent and
> neither acknowledged nor given a verdict:
>
> ```text
>    pause  ⇔  O_U ≥ cap  ∨  cwnd_full ,      O_U = |U| − |released ∩ U|
> ```
>
> At ρ = 1, O_U equals the store's outstanding count (pinned), so the bound
> is the one above. At ρ < 1 it is the first outstanding bound the evicting
> seat has had. It has no ρ input. Past the offered load
> λ·(1 − (1 − ρ − R)⁺) > C, the source is delayed or the contract breaks
> (Section 11.1).

**§11.1, a new open question:**

> *The overload region.* When λ·(1 − (1 − ρ − R)⁺) exceeds the carried rate
> C, the dial cannot hold both δ and ρ. The engine delays the source once the
> shed budget is spent. Receiver give-ups and forced verdicts can still
> exceed 1 − ρ there, and they are reported, not prevented. Thinning at the
> source within 1 − ρ − R is not implemented.

**§11.3, two new divergence rows:**

> | the ρ = 1 step | the evicting seat has no store, so forced verdicts occur at ρ → 1⁻ and not at ρ = 1 | bounded by the step test (record-only) until the store runs at every ρ |
> | the Realtime ρ preset | no declared value; the engine's shed budget needs one | config-time; no ρ < 1 budget ships undeclared |

**Appendix A.4, replacing the shed line:**

> `shed ⇔ age > D(δ) ∧ G_rx + V_pending + 1 ≤ (1 − ρ)·src_total`;
> `F = min U`; `pause ⇔ O_U ≥ cap ∨ cwnd_full` (every ρ).

## 9. Open questions for the owner

Each question has a recommended default.

1. **The overload region (§3.5).** Delay the source, break the contract, thin
   and then delay, or break and report? **Default: delay** (option 1, ρ wins
   over δ, ADR-0064's rule). Report every breach that receiver give-ups and
   forced verdicts still cause.
2. **A store at every ρ, (d).** It removes the ρ = 1 step, makes forced
   verdicts zero, and takes `reliable` out of the store, the clamp and the
   intake pause. **Default: yes, as the step after this fix**, with its own
   guard battery. This fix does not wait for it.
3. **Supersede `min(1 − ρ, R)` with `1 − ρ`, R as predicted use.** **Default:
   yes.** It is a deliberate model change: wasm give-up law, visualizer
   anchors, and a `GOLDEN_CAPTURE=1` re-capture, in its own commit.
4. **The Realtime preset ρ.** The repository has no source for it.
   **Default:** the owner declares a named point. 0.99 is a candidate: it
   leaves room above the measured 0.36 % shed share (liveness audit item 4)
   and is labelled declared, not derived. Until it is declared, the budget at
   Realtime is 0, which means serve and never shed.
5. **ρ at the receiver.** The receiver's give-up budget reads ε̂_recv.
   Carrying ρ in the handshake would be a second wire change. **Default: no.**
   The sender counts `G_rx` and reports breaches.
6. **Floor cadence.** Send on the tail-sweep clock only, or also immediately
   on a raise? **Default: the sweep clock only.** Raises can come in bursts,
   and holes with data above them pass on the receiver's own hold.
7. **D's RTT.** Use the measured EWMA RTT, with no seed (`D = 0` when cold),
   or the windowed-min RTprop that the doc comment names? **Default: the
   measured EWMA**, which is what the code passes. Fix the doc comment, and
   gauge D against the receiver hold.
8. **SACK release at every ρ.** It would make O_U exclude seqs received above
   the frontier. **Default: no for now.** Over-counting errs on the safe
   side; revisit with T7's `O_hwm` reading.
9. **Arm the cwnd brake by default at every ρ, (a).** **Default: no.** It
   changes ρ = 1, the brake alone has no verdict as a default, and (b) binds
   without it.
10. **A cumulative or a windowed budget.** The shed law's cumulative count
    admits bursts after long clean runs. **Default: keep it cumulative**
    (ADR-0064's form, no new constant) and gauge the largest shed burst.
11. **The config pair (`window_reliable = false`, ρ = 1).** It gives a zero
    budget with forced verdicts, which breaks the contract by construction.
    **Default: refuse the pair at config time.** That is a validation, not a
    runtime path; (d) dissolves it.
12. **ADR.** **Default:** a new ADR-0084, "Every verdict reaches the
    receiver", for the v11 message, the ack field and the verdict rule. It
    extends ADR-0010 (version) and amends ADR-0081's identity.
