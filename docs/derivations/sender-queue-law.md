# The sender's hand-off: one law for the three-knob sender queue and the `bbr_rs` window (derivation, no code)

**Status.** Design and derivation. No engine code, no VM run. The four verification checks (§1) are read from the §48b ledger and the code; their scripts and outputs are in `docs/l1-raw/bbr48b/handoff/` (run as `python -I <script> <dir holding the unpacked diag-ld/>`, from `docs/l1-raw/bbr48b/diag-ld-full.tgz`). The §48d predictions (§6) are written before any §48d row exists.

**Sources.**
- Engine: `raptorpath/src` on `main` (`91131216`): `transport/quic.rs`, `transport/io_owner.rs`, `net/mod.rs`, `net/sender_policy.rs`, `net/control_msg.rs`, `net/store_cap.rs`, `scheduler/path.rs`, `scheduler/place.rs`, `net/diag.rs`.
- quinn-proto 0.11.14 as vendored on `pacing-seam-2` (`389a7009`): `connection/datagrams.rs`.
- `bbr_rs`: `bbrrs-r4` (`fa9947dd`: §48 window law, one-round ProbeRtt `fab6c2ab`, send-record ring `fa9947dd`); `pacing-seam-2` (`RWM_BBR_PACE`, default off).
- Derivations: W1.2 `w1-who-limits.md` (`derive-w1` `ed4e3405`); W1r2 `w1-round2-probertt.md` and `reg-48d-draft.md` (`w1r2` `22574061`); `three-knobs.md` (main); `three-knobs-revision4.md` (`three-knobs-rev4` `fbfbc954`), §3.3, §11.4, §12.4, §13.
- Status: §47, §48b (Result, Erratum, ns bench, c8 diagnoses, `bbrrs-r2` result, in `docs/status/49-…md`), §41d (guard v2). Paper §6.1, §8.4. Measurement discipline 17 and 18.

## Short answer

**The finding that shapes the law.** In every §48b row the sender's queue sits wherever the substrate lets it, and it is the store's pinned cap that sets its size, not either controller:
- At c2 both arms hold the store at its cap n = 2 048. The cap sits at its N·P clamp on 80–82 % of lines at t ≥ 1 s (every line after its ramp ends at ≈ 8–10 s), and the store is full on 99.7 %. With quinn's BBR about 1 100 of them are on the wire and 840 in quinn's datagram queue; with `bbr_rs` about 230 on the wire and 1 800 in quinn's queue. Delivered delay is about n/x ≈ 206 ms in both.
- Quinn's datagram queue is one FIFO per path. First sends, resends, repair and control datagrams all wait in it (§1.1). Nothing in the engine sees it: placement, the send gate and the repair clamp read Copa's cwnd and an engine in-flight count that reads 33 at c2 while about 2 000 seqs are outstanding (§1.3).

**The law, on one line** (per leg i; the derivation is §2):

```text
   hand a first send to leg i  ⇔  B_i < β_i ,   β_i = R_i·L_i/s_i ,  R_i = 1.25·cwnd_i/srtt_i ,  L_i = τ̂_o + ℓ̂_o + τ̂_p + ℓ̂_p ;   else it waits in the intake, whose head expires  ⇔  a ≥ min( T − R , w(δ, x̄, t_ov) )
```

- B_i is what the producer has staged for leg i and quinn has not yet transmitted. R_i is quinn's pacer refill (RFC 9002 §7.7, N = 1.25). L_i is the worst-case reopen latency: the realized waits of the owner's republish timer and the producer's 1 ms backpressure poll, plus one owner poll and one producer iteration of work. s_i is the datagram size. **This is revision 4; paper §6.8 is the authority** (the revision history is in §0).
- w is Revision 4's wait limit (§13.3), taken by reference.
- The `bbr_rs` window is unchanged and δ-free: W_i = 2·b̂_i·m̂_i + A_i.
- Control, resends and repair are exempt from the bound but count in B_i. Repair is clamped by a state predicate: spare_i = 0 if path i refused a first send during the last completed producer turn, else +∞. The value is held through the current turn, store-full turns leave it unchanged, and the scheduler-wide value is the max over active paths. Known limit: bursts above β with link headroom (paper §6.8).
- The store law is unchanged in the first build: the δ seat (Q-S4, Q-S5) is open.

**The δ seat.** W1.2's store seat and the three-knob intake are not the same seat. W1.2's seat prices what the sender keeps *downstream* of its hand-off. It sits where three knobs' gate window would sit under Q31's alternative. The three-knob intake prices how long a unit waits *upstream*, and only under overload. **Recommended: δ on the intake, the store and gate δ-free** (§4). This is a recommendation for the owner, not a proof:
- **Quinn's queue has nothing to price.** Above the pacer's next release it buys no throughput at any δ (§2.2). That removes the downstream seat's FIFO part only.
- **What the downstream seat could still price** is a brake *below* the controller's window (W1.2 A: goodput for wire-queue delay). Keeping it would make two δ seats on one queue. The plan allows one, so it is given up unless the owner wants it back as one formula with the intake (Q-S5).
- **T = ∞ does not decide it.** The three-knob identity is T = ∞ ≡ today's ρ = 1 at the same δ (`three-knobs.md`: "float-identical to ρ = 1"), which a δ-priced store also keeps. Only the stronger reading, the same sender at every δ, would rule the store seat out, and r(β(δ)) fails that reading too (Q-S1).

If the owner accepts, W1.2's "δ on the store law" step is withdrawn. Its other conclusion, a δ-free `bbr_rs` window, stands.

**Build order** (§5):
1. Gauges.
2. §48d as drafted.
3. The δ-free hand-off gate on `bbr_rs`, with off-Bulk stages.
4. Promotion.
5. The gate and the wait limit together, offline (never run together yet).
6. The three-knob engine build.

**The three-knob engine build depends on `bbr_rs`.** The intake can only shed, and the clamp can only see saturation, if the queue is in the sender. The engine's own anchors over-read 7–14× (§1.3), so no engine-side window can take the controller's place.
- **Fully, where quinn's window never binds.** On c8's slow lossy leg quinn's BBR puts the whole queue on the link: B ≈ 1 under a window of 1–2.4 MB against a 100 KB BDP.
- **Partly, where it binds.** At c2 quinn's window binds (B = 840), so a gate on quinn's BBR would still move ≈ 40 % of the sender's queue into the intake. The other ≈ 1 100 datagrams (≈ 112 ms) stay on the link.

**§48d** (`bbr_rs` with ring and one-round ProbeRtt, against main): **`REGRESSION` predicted again.**
- The ProbeRtt fix should take c2's goodput deficit to about 0.8 % and c8's to about 2 %.
- The ring should take about 0.7 points off c8 client CPU.
- Neither touches c8 server CPU (+5.3 %, finer slow-leg I/O), the c8 in-order delay (+46 %), or the c2 delivered delay (≈ 206 ms in both arms). The two delay costs come from quinn's queue, which only the hand-off law removes (§6).

## 0. The formula first

**Revision history (2026-11-26).** Paper §6.8 is the authority for the law. Where the forms in this section, §2.3 and §2.6 differ from it, §6.8 supersedes them.

| rev | β | clamp | why it changed |
|---|---|---|---|
| 1 (`40d2fafb`) | 1.25·ν̂·(g + Δ_v) | (β − B)⁺/β | — |
| 2 (`e0561809`) | R·(g + a)/s, with R the pacer rate | (β − B)⁺/β | ν̂ throttled Startup; Δ_v had no provenance |
| 3 (`64aca2e0`) | R·L/s, L = VIEW_REFRESH + ℓ̂_o + P_poll + ℓ̂_p | window form (cwnd − F − sB)⁺/(F + sB) | g and a grew without bound in silence; nothing reopened a closed gate on a stale view; B/β read 0 at β < 1 |
| 4 (this) | R·L/s, L = τ̂_o + ℓ̂_o + τ̂_p + ℓ̂_p (realized timer waits, floored at the cited periods) | last-turn predicate, +∞ or 0 | **L doubled:** the 1 ms poll realizes at 1.75–2.4 ms p50 and 2.0–3.7 ms p99 (`handoff/q8_out.txt`). **The window form is withdrawn:** it reads ≈ 0 in loss recovery (w_rec ≥ T(1) ≈ F) and the probing margin in ProbeBw. **B/β** averages ≈ batch/(2β) under saturation. Only the predicate reads 0 exactly when a first send waits, at any β |

**Revision 4 in numbers.** β is ≈ 54 at c2, ≈ 61 and ≈ 12 on c8's legs, and ≈ 920 at c1s. That is ≥ 2.3× today's batch everywhere. At c1s it is above today's queue of 715, so the gate rarely binds there. At a 36/s call β ≈ 0.4–0.6 in ProbeBw, and ≈ 10 in Startup at W_init.
- **The owner's republish timer is assumed at τ̂_o ≈ 2 ms** until built and measured.
- **It reverses the owner's "no timer" rule** only while the owner holds unsent data, and is gauged as `owner_timer_wakes`.
- **The producer's 1 ms poll** must be armed on the gate's own pause at every ρ. Today it is armed by `tx_paused`, which carries `reliable &&`.
- **The §8.4 amendment narrows to R_i:** the predicate reads no window.

**The sentence.** Hand quinn no more than its pacer will send before the next hand-off. Everything else waits in the engine's intake, where it can be shed by age at a δ price, overtaken by resends, and seen by placement and the repair clamp.

```text
   per leg i, at each producer turn:
      hand the next unit to leg i   ⇔   B_i < β_i        (and n < N·P)
      β_i      =  R_i · ( g_i + Δ_v,i ) / s_i ,   R_i = 1.25 · cwnd_i / srtt_i ,   Δ_v,i = VIEW_REFRESH + p̂_i      [datagrams]
      spare_i  =  ( β_i − B_i )⁺ / β_i ,      r_eff,i = min( r_law , spare_i )
   intake (pooled over legs), at each dequeue with a younger unit waiting:
      expire the head u  ⇔  a(u) ≥ min( T − R , w ) ,   w = (c⁺/x̄) · min(1, 2/(1 + y)) ,  y = δ·t_ov/(c⁺ + x̄·ô)   (Revision 4 §13.3)
   substrate (unchanged): W_i = 2·b̂_i·m̂_i + A_i  (ProbeBw; paper §8.4), quinn paces at 1.25·W_i/srtt_i
```

| symbol | meaning | provenance |
|---|---|---|
| B_i | datagrams the producer has staged for leg i and quinn has not yet transmitted: the producer's own staged count minus quinn's `tx`. That includes what still sits in the owner's command channel, which `hand − tx` misses, so the producer cannot overfill by a batch | measured: the producer's count (new) and the owner's `tx` snapshot. Today's `dgq` gauge `hand − tx` reads the quinn part (`p0-dwell-diag.md`: within a few % of the queue) |
| R_i | quinn's pacer refill rate, bytes/s: 1.25·cwnd_i/srtt_i, or the controller's rate r_i when it supplies one | cited: RFC 9002 §7.7 (rate = N·cwnd/smoothed_rtt, N = 1.25), quinn-proto `pacing.rs:110`; ADR-0075 for r_i. cwnd_i, srtt_i: quinn `PathStats`, already in the owner's view |
| s_i | the leg's datagram size | measured |
| g_i | mean interval between hand-offs to leg i's owner while the intake holds units | measured: owner `tx_batches` per wall second |
| Δ_v,i | the most the `tx` snapshot can lag while the owner is active | derived from the publish rule (`io_owner.rs`: publish on a poll that did I/O once the last publish is ≥ `VIEW_REFRESH` = 1 ms old), so the lag is at most one refresh period plus one poll; p̂_i is the poll interval, measured (`[IOWN]` wall_s/polls). A stale `tx` only reads B too high, so an idle owner makes the gate tighter, never looser (§2.3a) |
| N·P | the store bound on outstanding seqs, holes included | resource bound, stated outside the law (status §1: per-path pool 2 048, shipped) |
| r_law | the repair rate law r(β(δ)) | outside this law (the rate law's own seat); passed through the clamp unchanged |
| T, a, R, ô, λ̂, D̂, t_ov, c⁺, x̄, the ramp | the three-knob intake | `three-knobs.md`, Revision 4 §3.1, §3.2, §13.3 (by reference; see §4 for its open items) |
| δ | the delay price, one seat (y) | declared dial |
| W_i, b̂_i, m̂_i, A_i | `bbr_rs`'s window | paper §8.4 (cited v1 constants, derived deviations) |

**Shape, read before any number.**
- **Units.** β is [1/s]·[s], datagrams. spare is a pure number in [0, 1].
- **In the dials.** No δ, ρ, T or hint enters β or spare, so both are identical at every preset (constant in each dial, which is trivially continuous and monotone). δ enters only y. T enters only c⁺ = max(T − R, 0).
- **In N.** β is per leg. A second leg adds its own β and changes nothing on the first. The intake is pooled, as in three knobs.
- **In the inputs.** β rises with cwnd, g and p̂, and falls with srtt. It reads quinn's pacer inputs (cwnd, srtt), not the controller's model (b̂, m̂); whether reading cwnd counts as §8.4's "substrate window as an input" is Q-S10.
- **T = ∞.** c⁺ = ∞, so w = ∞ and no head ever expires, by arithmetic, as in Revision 4 §13.3. The hand-off does not read T, so the T = ∞ sender equals today's ρ = 1 sender at the same δ, apart from the gate's own deliberate change (§2.9).

## 1. Verification

### 1.1 Do resends wait behind source data in quinn's per-path queue? **Yes, and so does everything else.**

- **One path, one connection, one queue.** Every symbol send, `send_symbols` and `send_symbol` alike, stages into the producer's `TxBatch` for the path's owner (`quic.rs:769–818`). That covers first sends (`emit_source.rs:435`), redundant copies (`:517`), the NACK resend (`sender_phases.rs:1051`) and the margin resend (`:1136`). The owner drains its command channel in order and calls `send_one` → `conn.send_datagram` for every datagram, Data lane and Ctrl lane alike (`io_owner.rs:1086–1104`).
- **quinn's queue is a FIFO with drop-oldest.** `Datagrams::send` does `outgoing.push_back`, and when over `datagram_send_buffer_size` (4 MiB, `quic.rs:944`) it pops from the front (`datagrams.rs:39–55`). `write` pops the front (`:167`). There is no priority and no deadline.
- **So a resend waits B_i/x_i behind the queue at the moment it is handed.** That wait is measured: the app-echo clock equals the wire RTT plus B_i/x_i, to within 2–15 %:

  | cell, arm, leg (window) | wrtt (ms) | B | x (dgr/s) | wrtt + B/x (ms) | app-echo A_x (ms) |
  |---|---|---|---|---|---|
  | c2 BASE | 112.4 | 840 | 9 798 | 198 | 206 |
  | c2 CAND | 21.8 | 1 804 | 9 653 | 209 | 207 |
  | c8 CAND leg 0 (8–16 s) | 21 | 1 082 | 9 560 | 134 | 131 |
  | c8 CAND leg 1 (8–16 s) | 90 | 964 | 1 995 | 573 | 565 |

  (Sources: R3 of `run/bbr48/score.txt`, `handoff/q2b_out.txt`, `q2c_out.txt`; x from `[IOWN]` `tx_dg`/`wall_s` at c2.) This settles §48b's open item "whether that clock includes the dwell in quinn's send buffer": it does.
- **Control datagrams share it too.** That matters for three knobs' floor repeats (Revision 4 §10): a standalone floor waits behind the same queue.

### 1.2 Why does `bbr_rs` hold about 900 datagrams on leg 1 early at c8? **The store cap's steps inject bursts that a saturated slow leg cannot pass, and quinn's queue keeps them.**

**Measured** (`handoff/q2b_out.txt`, `q2c_out.txt`, `q2d_out.txt`; 12 + 12 rows):
- **Leg 1 is at line rate in both arms.** Transmit rate is ≈ 2 000/s, about 20 Mbit at 1.2–1.3 KB, in every 4 s bin. Placement hands it 0.165–0.182 of the source, also in both arms.
- **CAND's leg-1 queue builds where the store cap steps.**
  - The cap ramps from ≈ 700–1 200 to its pin of 4 096 in 0.5–10.3 s.
  - In every CAND row, the net change of B1 summed over the cap-step ticks is positive: +151 to +1 428.
  - Over the other ticks before 16 s it changes by −908 to +758.
  - B1's median over 4–16 s is 482–989, and its peak is 1 017–1 329.
- **BASE gets the same bursts and drops them on the link.** The 10.3 s step to 4 096 adds +250 to +540 to B1 in BASE too, but B1's median over 4–16 s is 0–5. Quinn's BBR window on leg 1 (≈ 1–2.4 MB against a ≈ 100 KB BDP) passes the burst to the link at once, and the link queue holds it: `wrtt1` is 246–284 ms.
- **Under `bbr_rs`** the window holds leg 1's wire at ≈ 2.2 × BDP (`wrtt1` ≈ 88–90 ms). The burst stays in quinn's queue and drains only at the slack between leg 1's line rate and placement's share of it.
- **After the cap pins** (10.3 s) no more bursts arrive. Hand-over falls slightly below the drain (1 905 against 1 992/s over 16–24 s), so B1 falls to ≈ 250–320.

**Why placement does not stop it.**
- The load term is `E_i = (I_i/W_i)·s_i + (½ + ε_i)·wire_i` (`place.rs:262–277`). I_i/W_i is the engine in-flight over Copa's cwnd, and it reads ≈ 0 (§1.3). wire_i is quinn's wire srtt, which cannot see quinn's queue.
- Under `bbr_rs`, leg 1's wire RTT falls from ≈ 250 to ≈ 90 ms while its queue rises to ≈ 0.5 s, so the price falls exactly where the delay rose.

**Not shown:** what sets placement's share of each burst. Leg 1's share of the bursts is not separated from its share of steady hand-over.

**Consequence.**
- Under `bbr_rs`, a resend placed on leg 1 waits ≈ 0.24–0.49 s in quinn's queue in 4–16 s, plus 90 ms of wire.
- By §48b's tail diagnosis, 80–88 % of leg-0 holes are resent on leg 1. That is the 1.5–1.7× leg-1 hole clock, the larger H0, and the +46 % in-order delay (§1.4).

### 1.3 Does the engine's window bind? **Neither controller moves it; it does not bind at c2 or c8 in either arm.**

**What the engine's window is.**
- The send gate (`active_paths()`, `available() = cwnd − in_flight > 0`) and the repair clamp (`spare = (cwnd − in_flight)/in_flight`, `path.rs:455–472`) read **Copa's cwnd** and **the engine's in-flight count**. Neither is the QUIC controller's window.
- The 1.5 × BDP in-flight gate of Revision 4 §11.4 is a generation-mode default only (`sender_policy.rs:756–758`: `unwrap_or(if gen_pipe {1.5} else {0.0})`). It did not run in these window-pipeline rows: `[DIAG]` prints `cap0`.

**Measured** (`handoff/q3_out.txt`, `q3b_out.txt`; lines at t ≥ 1 s):

| cell | arm | share of lines with a path at cwnd − in_flight ≤ 0 (`np_act < np`) | median engine in-flight Σ | median Copa cwnd Σ | median Σ B (quinn queue) | median repair rate |
|---|---|---|---|---|---|---|
| c1s | BASE / CAND | 0.0019 / 0.0005 | 1 300 / 1 523 | 3 322 / 3 102 | 1 516 / 702 | 8 / 12 sym/s |
| c1d | BASE / CAND | 0.242 / 0.203 | 1 621 / 1 698 | 2 758 / 3 020 | 132 / 80 | 52 / 56 |
| c2 | BASE / CAND | 0.000 / 0.000 | 33 / 37 | 10 000 / 10 000 | 808 / 1 803 | 32 / 32 |
| c8 | BASE / CAND | 0.020 / 0.022 | 609 / 188 | 11 182 / 12 043 | 628 / 1 302 | 60 / 68 |

**Reading.**
- **The window does not bind at c2 or c8 in either arm.** At c1d it binds on 20–24 % of lines, also in both arms. Copa's cwnd sits at its 10 000 maximum at c2.
- **The engine's in-flight count is not the outstanding count.** It reads 33–37 at c2 while ≈ 1 900–2 030 seqs sit on the wire and in quinn's queue.
  - It is charged at staging (`emit_source.rs:449`).
  - It is released by the ack arms' `expected − received` terms, which the code itself calls contaminated (`path.rs:1158–1160`, `control_msg.rs:640–650`).
  - The cause of the under-read is not identified here. Revision 4 §3.3 flagged "the engine counts in-flight at its own send, ahead of quinn's queue" for an engine read. The read is: yes at the charge, but the releases do not track it.
- **All 96 rows are Bulk** (`hint=bulk` in every driver header). The repair rate is 8–68 sym/s against 9 500–94 000 source sym/s, so the clamp is moot in this ledger. No off-Bulk row exists, so the bracket question cannot be measured here (CLAUDE.md: no engine merge on a Bulk-only guard).

**Placement against §12.4's bracket** (CL-long: the clamp binds in the drain for cwnd roughly in [200, 1 400]).
- **The engine as built, under either controller,** sits above the bracket: Copa's cwnd is 10 000 per path against an in-flight that under-reads ≈ 60×. That is the "2 048" side, where CL-long is UNDETERMINED.
- **Inference, labelled: where each controller's window would sit if the clamp read it.**
  - `bbr_rs`: W ≈ 2·b̂·m̂ + A ≈ 2.2 × BDP (measured ≈ 227 datagrams at c2, ≈ 404 Σ at c8). For CL-long's cell (12 500/s, 10 ms) that is ≈ 260–300, inside the bracket.
  - Quinn's BBR: 7–14 × BDP on these cells (Σ qcwnd/1.2 KB ≈ 1 100 at c2, ≈ 2 450 at c8). That is at or above the bracket's top, and it "never limits" on the slow lossy leg.
  - The hand-off law (§2.6) gives the clamp a spare that binds exactly when quinn's queue is full. That is §3.3's intended reading, and it holds only if the controller keeps the queue in the sender.

**The engine's anchors over-read** (`handoff/q5_out.txt`). Copa's BtlBw reads 7–14 × the leg's transmit rate in every cell and arm. Its RTprop reads the echo clock, 190 ms at c2. So `bdp` reads 7–15 × BDP. `bbr_rs`'s b̂ reads 1.00–1.07 × the link.
- The shipped store law `(1 + q(δ))·Σ bw·RTprop` reads these anchors. That is why it asks more than N·P and sits pinned (§2.5).

### 1.4 What remains of §48b's cost after the ring and the ProbeRtt fix

**Recorded numbers, by source.**

| fix | where measured | recorded | what it implies for §48b's VM cost |
|---|---|---|---|
| send-record ring (`fa9947dd`) | host bench `nsbench48` gate (`db2cb4f4`) | net ratio `bbr_rs`/quinn **7.7× → 1.3–1.7×**. That bench's baseline read 7.72×, not the VM's 3.6–4.9× | **no VM figure is recorded.** If the relative cut carries over, the VM excess of +339 ns/packet (200 in flight) falls to ≈ +15–35 ns. That saves ≈ 3 ms of CPU per second at c8's ≈ 10 k packets/s, ≈ 0.7 % of client CPU/GB. **Inference** |
| one-round ProbeRtt (`fab6c2ab`) | `cc_sim` only (c2-jitter) | time in R 0.44–0.49 % (was 2.1 %); b̂/L inside R ≥ 1.03 (was 0.588); m̂ at exit 8.0–12.35 ms, above the derivation's 7.8–9.2 ms band | ProbeRtt's goodput cost goes from 0.84 % to the predicted 0.13–0.23 % at c2, and from 0.78 % to 0.17–0.32 % at c8 (W1r2 §2; sim, not VM) |

**What neither fix touches, by the recorded diagnoses.**
- **c8 server CPU, +5.3 %.** §48b's c8 diagnosis puts it on finer I/O on the slow leg: quinn's pacer burst falls to its 10-datagram floor under `bbr_rs`'s honest window. It is not on controller bookkeeping, since the server runs no controller on data.
- **c8 in-order delay, +46 %.** Its cause is the resend's wait in leg 1's quinn queue (§1.2).
- **The unattributed goodput rest.** ≈ 0.6 % at c2. ≈ 1.8 % at c8, of which 70 % sits in the completion tail through H0.
- **c2 delivered delay, ≈ 206 ms in both arms.** n/x with n pinned (§1.1).

**Against the guard's bars** (v1: §41c, the q95 of |x| from §48b's BASE splits; v2: §41d, d80).

| clause | remaining after both fixes (predicted) | v1 bar (q95) | v2 bar (d80) | reading |
|---|---|---|---|---|
| c2 goodput | −0.8 % (−0.5 … −1.3) | 0.21 % | 1 % | seen under v1; about even under v2 |
| c8 goodput | −2.0 % (−1.2 … −2.6) | 1.32 % | 2 % | likely seen |
| c8 cpu_cli/GB | +2.8 % (+1.5 … +3.5) | 3.6 % | — | borderline |
| c8 cpu_srv/GB | +5.3 % (+3.5 … +6.5) | 4.7 % | 8 % | seen under v1 (it read p = 0.006 in §48b) |
| c8 in-order delay mean | +46 % (+30 … +60) | not a v1 clause | 20 % | **fails v2 by itself** |
| c1s, every clause | CAND better | blind (BASE's collapsed rows: goodput q95 48 %) | — | — |

**The §48d draft's host gate is stale.**
- It requires T6's f = 1.25 mutation to be red. Round 2 recorded it not red and `#[ignore]`d it: the sim plateaus at m̂ ≈ 12.6 ms instead of growing ×1.3 per period.
- **The recorded reason** (the doc comment on `probe_rtt_ratchet_gate_sees_a_mutated_gain_of_1_25`, `cc_sim.rs` on `bbrrs-r4`): "the round's min is not the window-limited loop time, so the derivation's ×1.3 per period growth does not appear". The f = 1.25 exits read [12 750, 12 750, 12 650, 11 200, 12 550, 12 750] µs.
- **The gate can see a ratchet on the shipped form.** On the pre-fix tree the same f = 1.25 mutation grows ×2 per period, 23.9 → 50.3 → 102.6 ms (`docs/l1-raw/bbrrs-r2/red.txt` on `bbrrs-r4`). The shipped form resets m̂ to the loaded r_e at expiry; the one-round form does not. So the plateau is evidence that the fix removed the ratchet path, not that the gate is blind in general.
- **A mechanism consistent with the plateau** (inference with a fitted constant, not tested):
  - W1r2's map assumed the drained round's minimum reads f·(b̂/L)·m̂. A queue forms only once the drained window exceeds the **mean** zero-queue loop R̄₀; below that, the minimum reads the floor.
  - The onset is then m̂·(f·b̂/L − 1) > R̄₀ − m_r, with m_r the one-round floor (8–12 ms in the f = 0.75 exits), and the fixed point is m̂* ≈ R̄₀/(f·b̂/L).
  - R̄₀ ≈ 16 ms reproduces the recorded 12.55–12.75 ms. **That value is fitted to the plateau, not measured**: the sim prints only the loaded mean loop (14.3–15.7 ms in the red run's ProbeBw), which is not R̄₀. So this reading is not independent evidence.
  - On that reading, a mutation that ratchets on the fixed form needs a larger gain: f ≈ 2 gives m̂′ ≈ 12 + 2.06·12.6 − 16 ≈ 22 ms.
  - **The shipped f = 0.75 cannot ratchet on either reading:** f·b̂/L ≤ 0.8 < 1 (W1r2 §1b).
- The owner decides the gate's form (Q-S3).

## 2. The derivation

### 2.1 Where the sender's queue sits: three measured identities

Per leg i, under saturation (the Bulk transfer of every §48b row):

```text
   (I1)  n  ≈  Σ_i ( F_i/s + B_i )  +  H  +  x̂·λ          store occupancy = wire + quinn queue + holes + ACK→SACK runway
   (I2)  A_i ≈  wrtt_i + B_i / x_i                          the app-echo delay of a datagram handed now, first send or resend
   (I3)  x_i =  min( L_i , F_i / R_i ) ,  F_i ≤ W_i          the controller alone sets the rate; B_i does not enter
```

- **(I1) at c2.** CAND 227 + 1 803 ≈ 2 030; BASE ≈ 1 100 + 808 ≈ 1 910. Both are within holes and runway of n = 2 048.
- **(I2).** The table in §1.1.
- **(I3).** Paper §8.4. Quinn sends whatever the window and pacer allow from the front of its queue, so a non-empty queue adds no rate.

### 2.2 What a datagram in quinn's queue buys

**By (I3), throughput depends on B_i only through whether it is empty.** If B_i > 0 at every instant the pacer could send, the rate is the controller's. If it runs dry, the link may idle.

**By (I2), every datagram above that point adds B/x of delay to everything handed after it:**
- the first sends (A_x);
- the resends, and so the hole clock, the in-order delay and the c8 tail (§1.2);
- the control datagrams (§1.1).

**It is also invisible** to placement, the gate and the clamp (§1.3).

**So the optimal B_i is the smallest that never runs dry, at every δ.** In U = log x − δ·log d:
- above that point, removing a datagram lowers d and leaves x unchanged;
- below it, removing one lowers x and leaves d about unchanged, because the wire's standing queue (≤ b̂·m̂ + A when window-limited) is the controller's and does not move.

This is Revision 4 §13.1's free-band argument applied downstream: the band above the pacer's next release is free at every δ. **The hand-off is δ-free by U, not by assumption.**

### 2.3 The reserve β, sized

**The queue must cover what the pacer can release before the producer looks again.**
- quinn's pacer bucket refills at R_i = 1.25·cwnd_i/srtt_i (RFC 9002 §7.7; quinn `pacing.rs:110`), or at the controller's r_i under `RWM_BBR_PACE`.
- The next look comes g_i later, through a snapshot up to Δ_v,i old, so at its refill rate the pacer can release R_i·(g_i + Δ_v,i) bytes before B_i is seen again. A full bucket can add up to its capacity (10–256 MTU) at once; β does not cover that burst, and `starve_i` gauges whether it ever empties the queue with the window open.

```text
   β_i = R_i · ( g_i + Δ_v,i ) / s_i
```

**§2.3a Provenance of the two constants of the first form (revised 2026-11-26).**
- **The 1.25** is not BBR's ProbeBw pacing gain and not ours. It is the N of RFC 9002 §7.7's pacer ("using a value for N that is small, but at least 1 (for example, 1.25) ensures that variations in RTT do not result in underutilization of the congestion window"), which quinn implements as `new_tokens = window · 1.25 · elapsed_rtts`. It applies to a staging bound because the bound's job is to cover the actuator's release between two looks, and that release is exactly the bucket's refill. So the bound now reads the refill rate itself, R_i, and carries no constant of its own.
- **The first form's ν̂ (a windowed-max transmit rate) is dropped.** A bound tied to past transmissions caps the controller's growth at 1.25× per filter window, so it would slow Startup (×2.885 per round) to ×1.25. R_i follows cwnd as it grows.
- **Δ_v = 1 ms** was `VIEW_REFRESH`, a code constant with no sweep. It is now derived from the owner's publish rule: `full = dirty && now − last_full ≥ VIEW_REFRESH` (`io_owner.rs`). While the owner is active, the snapshot therefore lags by at most one refresh period plus one poll: Δ_v,i = `VIEW_REFRESH` + p̂_i, with p̂_i measured from `[IOWN]` wall_s/polls (0.26 ms at c1s, 0.81 ms at c2, 0.86 and 1.30 ms on c8's legs).
  - While the owner idles, the snapshot grows older, but `tx` only rises. A stale snapshot therefore reads B too high, and the gate errs tight. The producer's next command makes the owner poll and publish within one poll.
  - **A measured snapshot age was considered and rejected** (advisor review, 2026-11-26). After an idle second it would open β to the whole backlog: at c2, 12 900/s × 1.5 s ≈ 19 000 datagrams. That would happen exactly in the post-outage drain the clamp exists for.

**§2.3b What is gated, and in what order.** Each producer turn stages control first, then resends (gap and margin), then first sends and proactive repair while B_i < β_i.
- Resends and control are exempt from the bound but count in B_i. They are bounded by their own laws: the resend budget, and the deficit-recovery exemption the engine keeps so a full window cannot deadlock a hole.
- So a resend waits behind at most β_i plus the exempt traffic already staged.
- This is a choice by datagram kind, not by a dial.
- **At ρ < 1** the gate adds a source pause where today there is none. Units then wait in the source's queue (the TUN channel, then the kernel's tail drop) instead of quinn's drop-oldest FIFO. That is a behaviour change with no shedding law until the three-knob intake is built (Q-S11).

**Size check** (§48b CAND, s42 r1; cwnd and srtt from `[DIAG]` `qcwnd`/`wrtt`; g and p̂ from `[IOWN]` `tx_batches`, `polls` and `wall_s`):

| cell, leg | cwnd (dgr) | srtt (ms) | R (dgr/s) | g (ms) | p̂ (ms) | β (datagrams) | β in time at the rate x | today's batch size (dgr per hand-off) | today's B (CAND) |
|---|---|---|---|---|---|---|---|---|---|
| c1s | ≈ 580 | 5.0 | ≈ 145 000 | 0.62 | 0.26 | ≈ 273 | 3.0 ms | 57 | 715 |
| c2 | ≈ 227 | 22 | ≈ 12 900 | 2.30 | 0.81 | ≈ 53 | 5.5 ms | 22 | 1 804 |
| c8 leg 0 | ≈ 220 | 21 | ≈ 13 100 | 2.92 | 0.86 | ≈ 63 | 7.1 ms | 25.6 | 970 |
| c8 leg 1 | ≈ 186 | 88 | ≈ 2 640 | 2.86 | 1.30 | ≈ 13.6 | 7.5 ms | 5.2 (BASE 7.5) | 327 (p50 4–16 s: 480–990) |
| a 36/s call, `bbr_rs` in ProbeBw (W ≈ 7 dgr) | ≈ 7 | 100 | ≈ 90 | ≈ 3 | ≈ 1 | ≈ 0.4 | — | 1 | — |
| a 36/s call, `bbr_rs` in Startup at W_init (240 KB) | ≈ 195 | 100 | ≈ 2 430 | ≈ 3 | ≈ 1 | ≈ 12 | ≈ 0.33 s of the call | 1 | — |
| c8 leg 1 under quinn's BBR (BASE) | ≈ 1 400 | 250 | ≈ 7 000 | 4.16 | 1.76 | ≈ 48 | — | 7.5 | ≈ 1 |

- **β exceeds today's per-hand-off batch on every leg**, so the gate does not force finer hand-offs. It is not a floor in disguise, and it predicts no extra I/O cost; that is the c8 CPU diagnosis's mechanism, checked by gauge (§3).
- **β is above the counter's staleness term by construction.**
- **At a call rate, β depends on the controller's state.**
  - In ProbeBw β ≈ 0.4, so the gate hands only when quinn's queue is empty.
  - In Startup at W_init (which `bbr_rs` does not leave while app-limited) β ≈ 12. Under quinn's BBR β is tens.
  - With headroom B ≈ 0 anyway, because quinn sends on hand-off.
  - Under a sustained overload the path is not app-limited, so `bbr_rs` leaves Startup and β < 1.
  - So up to ≈ 12 datagrams can wait in quinn only in the Startup transient and under quinn's BBR. That matters for K-OV, and Stage R reports it.
- **Quinn's burst quantum** (clamp(W·2 ms/srtt, 10, 256) MTU) is a cap on what the pacer releases at once, not a requirement that the queue hold it. So it is not in β.
- **The wire's standing queue covers hand-off jitter.** Window-limited, `bbr_rs` keeps ≈ b̂·m̂ + A beyond the pipe (paper §8.4), so the link stays busy through a hand-off gap of up to m̂ + A/b̂ + β/x. That is ≥ 4 ms at c1 and ≥ 12 ms at c2, against g ≤ 3 ms. This is Revision 4 §11.4's link-busy premise, and it holds by a margin of ≥ 4×.

### 2.4 What is left for δ, and where it sits

**The δ readers on the hand-off path today, and their fate:**

| reader | site | where it acts | fate under the law |
|---|---|---|---|
| store cap `(1 + q(δ))·Σ bw·RTprop` | `store_cap.rs` (ADR-0072) | downstream occupancy | at its N·P clamp on 100 % of c1s lines, 80–82 % of c2's and 71–72 % of c8's at t ≥ 1 s, every line after the ramp (§9; MD18). **Removed from the hand-off**; the store is the resource bound N·P |
| Copa's cwnd, priced by `copa_delta_for_hint` | `scheduler/copa.rs` | the send gate, the repair clamp and placement's load term | **Leaves the hand-off path:** the gate reads B, the clamp B/β, placement B/β (§2.7) |
| r(β(δ)) | the rate law | how much repair to send | unchanged. It is the rate law's seat, passed through a δ-free clamp |
| the wait limit's y | Revision 4 §13.3 (not in the engine) | upstream wait under overload | **the one seat** |
| Q31's alternative gate `(1 + q(δ))·Σ bw·RTprop` | Revision 4 §11.4 | downstream occupancy | not adopted (§2.5) |

**What the T = ∞ identity does and does not decide.**
- **The three-knob identity is at the same δ.** `three-knobs.md` states T = ∞ "float-identical to ρ = 1" in every cell. A δ-priced store or gate keeps that identity, because today's ρ = 1 sender carries the same q(δ). So T = ∞ does not rule out a downstream seat.
- **The stronger reading** is "the T = ∞ sender is the same at every δ". Take an object at T = ∞ (three knobs: objects are T = ∞ at every hint) priced at δ = 50. That reading would rule out the store seat (q = 0.05 against Bulk's 0.10), Q31's gate seat, and a window seat. It would also rule out r(β(δ)), which the rate law keeps at every T. So it is not the model's reading, and it is not used here (Q-S1).
- **The other faults of those seats stand on their own.**
  - Q31's gate seat at δ ≥ 50 fails the link-busy check at 1.05 × BDP (§11.4).
  - A window seat (W1.2 B, δ in τ_w) contracts at Realtime (W1.2 §3 B).
  - The shipped store seat is pinned by over-reading inputs (§9). That is a defect of its inputs, not of the seat: W1.2 A assumed honest ones.

### 2.5 The store and the gate window become images

**The store.**
- With the hand-off gated, outstanding seqs are Σ_i (F_i/s + B_i) + H + x̂·λ ≤ Σ_i (W_i/s + β_i) + H + x̂·λ.
- The store's only remaining job is to bound H, the holes the receiver must hold. That is a memory bound: N·P, a resource constant stated outside the law.
- The shipped law's δ term is removed. It is pinned today (MD18 finding below), so at c2 and c8 its removal changes nothing measurable.

**A risk, stated: the law moves the store law's input.**
- The gate empties quinn's queue, so Copa's echo-clock RTprop (190 ms at c2) falls toward the wire RTT. If the shipped store law were kept, its pin would release and it could start braking below the gate. That is W1.2's H1 failure: STORE-LIMITED, with a large deficit.
- So the law ships with the store at N·P only, and `store_pin` is gauged.

**The gate window of Revision 4 §11.4 (Q31).** Neither the engine's 1.5 × BDP nor §6.1's `(1 + q(δ))·Σ bw·RTprop`.
- The in-flight bound is the controller's own window W_i.
- The hand-off reserve is β_i.
- The 1.5 constant's debt is retired by derivation, not adopted: its link-busy premise is §2.3's margin.

### 2.6 The repair clamp

```text
   spare_i = ( β_i − B_i )⁺ / β_i ,     r_eff,i = min( r_law , spare_i )
```

- **Headroom:** B_i ≈ 0 between hand-offs, so spare ≈ 1 ≥ r. Right after a hand-off of one batch, B = batch ≈ β/2, so spare ≈ 0.5.
  - The clamp therefore binds with headroom only if r_law > 1 − batch/β ≈ 0.5.
  - That is not verified: no §48b row has headroom. The registration's paced stage reads it.
  - Repair-off rows are identical (r = 0).
- **Saturated:** B_i → β_i, so spare → 0 and r_eff → 0. That is Revision 4 §3.3's semantics ("stop repair where there is no spare capacity"), now with an input that is honest whenever the controller keeps the queue in the sender.
- **Continuous** in B, and identical at every preset.
- **Against the engine's form** `(cwnd − in_flight)/in_flight`: the new form reads the queue quinn actually holds, not Copa's cwnd against a count that under-reads 60× (§1.3).

### 2.7 Placement's load term (a consequence, one symbol)

- `E_i = (I_i/W_i)·s_i + (½ + ε_i)·wire_i` reads the same broken in-flight. Under the gate the substitution is I_i/W_i → B_i/β_i: continuous, ≥ 0, and above 1 only transiently.
- With the gate, over-offer cannot accumulate anyway. A leg without room takes no unit, and the unit waits in the intake for any leg with room. This removes §1.2's mechanism.
- The gate filters legs as `active_paths()` does today (a capacity admission, not a dial comparison). Placement's own monotonic-spillover note (`place.rs:281–287`) applies to its price, not to this admission.

### 2.8 Does the law move its own inputs?

| input | moved by the law? | direction and bound |
|---|---|---|
| B_i | it is the regulated state, as in_flight is to cwnd | held near β_i |
| R_i (cwnd, srtt) | cwnd is the controller's; the gate moves it only through app-limited samples (next rows); srtt is quinn's wire srtt, which the FIFO does not enter | none from B; the bound follows the window up, so it cannot throttle growth |
| p̂_i | a blocked producer polls less only if it parks | p̂ up means β up, never less |
| g_i | a blocked producer may hand less often | g up means β up, toward more reserve, never less. Bounded by the producer's wake sources; gauged |
| R (srtt) in the wait limit | yes: the queue leaves the RTT | c = T − R grows, so less expiry. This is the intended effect, and it is not a loop on the gate |
| the controller's window W (as quinn's cwnd) | **read**, as the pacer's input R_i = 1.25·cwnd/srtt | the gate is not a store law and adds nothing to the window. Whether reading cwnd here counts as §8.4's "substrate window as an engine input" (W1.2's U1) is Q-S10 |
| the controller's model b̂, m̂ | not read | |
| quinn's `app_limited` flag, and through it `bbr_rs`'s samples | yes, whenever quinn's queue empties with window room | quinn sets `app_limited = buf.is_empty() && !congestion_blocked` at each transmit poll (vendored `connection/mod.rs:941`). `bbr_rs` then admits rate samples raise-only (`bbr_rs.rs:370`) and does not leave Startup (`:938`). With β ≈ 2–5 ms of drain, the queue empties when a hand-off is late by more than that, and at window jumps: at ProbeRtt exit the window opens by ≈ 1.25·b̂·m̂ + A, ≈ 10 ms of data at c2, against β ≈ 4 ms, so the refill takes a few hand-offs. Raise-only samples err high, which is the safe side for b̂. A Startup held by app-limited polls keeps its bounded T(2.885) window (paper §8.4). Gauged as `app_limited_i` and qm = S occupancy |
| Copa's RTprop and BtlBw, the store law's inputs | yes (they fall) | the store law is removed from the hand-off (§2.5) |

### 2.9 NO-MODE-SWITCH, continuity, inertness

**One expression per decision.**
- The gate compares a state with a measured quantity (B_i < β_i). It is a capacity admission and compares no dial.
- No `reliable` bit selects it. It applies at every ρ, which retires the ρ-selected sites `tx_paused = reliable && …` (`net/mod.rs:2473`) and the reliable-only in-flight gate (Revision 4 §3.3, §11.4 debt).

**Continuity.**
- β and spare are constant in δ, ρ and T.
- The intake's continuity, ±2 % at every preset (|Δw|/w ≤ 2 %·y/(1 + y)), is Revision 4 §13.3's.

**T = ∞.** No expiry, and the hand-off does not read T. The T = ∞ sender equals the ρ = 1 sender of the same law at the same δ, as three knobs requires.

**It is not inert against today's engine at Bulk.** Quinn's queue moves into the engine's intake, so the release binary changes deliberately (`binid: expected-change`).

## 3. Gauges (every clamp, plus the premises)

| gauge | what | predicted under the law |
|---|---|---|
| `hand_bind_i` | share of producer turns with B_i ≥ β_i | ≈ the leg's window-limited share: ≈ 1 in saturated Bulk cells, ≈ 0 with headroom. Reported beside the quantiles of β_i and B_i (B ≤ β + one batch), so the law's shape stays visible (MD17) |
| `starve_i` | share of time B_i = 0 while the intake holds units and the controller has room | ≈ 0. If not, the link-busy premise fails |
| `batch_i` | datagrams per hand-off per leg | not below the no-gate arm beyond A/A. If below, finer I/O: the c8 CPU mechanism |
| `spare_bind_i` | share of first sends with spare_i < r_law | ≈ 0 with headroom; binds in overload and drain |
| `store_pin` | share of lines with n ≥ N·P | ≈ 0 except during recovery backlogs (today the cap is at N·P on 0.80–0.82 of c2 lines with the store full) |
| `den_bind`, `ov_frac`, `onset_bind`, `floor_bind`, `tov_resets`, `y_max`, `w_min_over_c` | the intake's clamps and edges | Revision 4 §13.3 |
| `fifo_wait_resend_i` | B_i·s_i/R_i at each resend's hand-off | ≈ β/x (≤ 5 ms) against today's 0.2–0.5 s |
| `infl_vs_out` | the engine in-flight count against Σ(F/s + B) | the defect gauge (§9); reported |
| `app_limited_i` | share of quinn transmit polls on leg i that end app-limited | small under saturation; a rise at ProbeRtt exits is expected (§2.8) |
| `bbr_rs`'s own clamps: `wmin_bind` (W at W_min = 4 MTU), `wrec_bind` (min(W, w_rec) taking w_rec), `sfloor_bind` (sampler interval at its max(m̂, 1 ms) floor), the ring's overflow count | the controller's clamps, which the hand-off inherits | ≈ 0 outside loss recovery and the first round. `wrec_bind` is the recovery window, gauged with loss episodes. The ring's overflow counter already exists |

## 4. The δ-seat answer

**Not the same seat.**
- **W1.2's store seat** is a downstream-occupancy seat. Candidate A: n = G·Σ L̂·(RTprop + S + τ_s(δ)), the store braking below the window. It prices what the sender keeps between its hand-off and delivery. It is the same position as three knobs' gate window under Q31's alternative.
- **The three-knob intake** is an upstream-wait seat. It prices how long a unit may wait before hand-off, and only under overload (y = 0 at x̂ ≤ 1).

**Recommended: the intake holds δ.** This is a recommendation, resting on three points:
1. **The free band** (§2.2). Quinn's queue above the pacer's next release buys no throughput at any δ, so the FIFO part of the downstream seat has nothing to price.
2. **The one-seat rule.** What a downstream seat could still price is a brake below the controller's window. Keeping it next to the intake would make two δ seats acting on one queue.
3. **The remaining trade is an owner call (Q-S5),** not a derivation result.

T = ∞ does not decide it (§2.4), and the shipped store seat's pin is a fault of its inputs, not an argument against the seat.

**Kept from W1.2:** the `bbr_rs` window stays δ-free.

**What the law gives up.** Braking *below* the window: W1.2 A's c2 trade, −1.9 to −8.5 % goodput for Tp50 ≈ 16–20 ms.
- Under the law the wire's standing queue (≤ b̂·m̂ + A ≈ 12 ms at c2) is the controller's probing margin, with no δ.
- With the queue gone from quinn, the delivered delay at c2 is predicted at ≈ wrtt + β/x ≈ 25–35 ms against 206 ms, close to W1.2 C's 25–30 ms.
- The queue itself costs no goodput to remove (§2.2). The residual costs are the refills after window jumps and the app-limited samples (§2.8), and they are gauged, not assumed away.

**The intake seat is by reference and unconfirmed.** Revision 4 §13.3 carries Q24 (U read as a flow), Q33 (the ramp) and Q34 (false overload at x̂ → 1⁺), and its build B′ has not run. The δ-free part of this law (§0's hand-off, clamp and store) stands without it.

## 5. Build order

**Ruling, 2026-11-26 (coordinator and Fable, after this report).**
- Step 2 (§48d as drafted) does not run as a scored session.
- Steps 2 and 3 merge into one session (`reg-handoff-gate.md`): `bbrrs-r4` plus the gate is the scored promotion candidate on guard v2 with off-Bulk stages, and `bbrrs-r4` alone is a reported-only arm.
- The δ seat (Q-S4, Q-S5) stays open, so the gate is built δ-free and the store law is unchanged.
- The law is written in paper §6.8.

The original order follows for the record.

1. **Gauges first** (engine instrument, reported only, its own commit): the owner poll interval p̂_i, g_i, R_i, B_i quantiles (producer-staged count), `fifo_wait_resend_i`, `infl_vs_out`.
2. **§48d as drafted** (§6). It answers whether the δ-free controller fixes work in vivo. Bulk only: it cannot test the intake or the clamp.
3. **The hand-off gate on `bbr_rs`** (δ-free, T-free, ρ-free), its own registration.
   - **Against main, and against step 2's CAND.**
   - **With off-Bulk stages and execution witnesses:** the clamp acts only off Bulk (CLAUDE.md; roadmap `meas-guard-bulk-blind`).
   - **Predicted direction:** c2 and c8 delivered delay 206 → ≈ 25–35 ms and 122/251 → ≈ 30/100 ms; c8 in-order delay below main's; goodput within A/A.
   - **The risks are gauged:** `starve_i`, `batch_i`, `store_pin`.
   - **This is the step that removes `bbr_rs`'s queue-borne costs:** the in-order delay, the leg-1 hole clock, and the c8 tail through H0.
4. **Promotion of `bbr_rs` with the gate:** a new ADR superseding ADR-0054's default (owner).
5. **Offline first: the gate and the wait limit together.** Revision 4 §12.1 ran no design with a send gate, and its toy has no controller. So gate plus wait limit has never run together. A toy stage with a controller window and §0's gate, on Revision 4's link cells, registered before its run, comes before any engine build of the intake.
6. **The three-knob engine build:**
   - L1, L2 and the protected floor, with the wait limit as the δ seat;
   - the clamp's spare from §2.6;
   - the store's δ term deleted (if Q-S4 is accepted).
   - **It depends on 3 and on `bbr_rs`.** With quinn's BBR the queue sits on the link wherever quinn's window does not bind, and there the sender can neither shed nor reorder it (§1.2: c8's slow leg, B1 ≈ 1, `wrtt1` 233–330 ms). Where quinn's window binds (c2: B = 840) the dependence is partial: the gate would still move ≈ 40 % of the queue into the intake. The engine's anchors over-read 7–14×, so no engine-side window substitutes (§1.3).
   - **And on 3 for the lifetime itself.** At T = 0.4 with today's queue, c = T − R is ≈ 0.4 − 0.21 − (4·rttvar + ack delay) ≈ 0.1–0.19 s at c2. On c8's slow leg R ≈ 0.48–0.57 s in 4–16 s, so c⁺ = 0: every head expires (`den_bind` = 1) because of the sender's own queue. Under the gate, R ≈ 25–35 ms at c2 and ≈ 100 ms on c8's slow leg, so c ≈ 0.3–0.37 s.
   - Bulk caveat: these A_x are Bulk rows. At Realtime today the queue is the same 4 MiB FIFO with no source pause, and its residence is 0.65–0.70 s under overload (Revision 4 §3.3).

**The pacing seam's first reading** (`cc-pacing-seam-reading`, after §48d). Without the gate, delivered delay cannot move, because the store stays pinned and A ≈ n/x. Predicted: B rises by about what the wire loses (c2 CAND B ≈ 1 800–1 950), and A_x ≈ 206 ms ± 15 %. The reading is informative about the controller, not about delay.

## 6. §48d predictions (written before any row)

**CAND, exactly.**
- `bbrrs-r4` at `fa9947dd`, merged with main at launch: the §48 window law and `[QCC]` gauges, the one-round ProbeRtt (`fab6c2ab`, `RWM_BBR_PROBERTT_ROUNDS` unset = 1) and the send-record ring.
- Run with `RWM_QUIC_CC=bbr_rs`.
- No pacing seam (`pacing-seam-2` is stacked on `bbrrs-r2`, not on r4; if it is included, `RWM_BBR_PACE` stays unset).
- No hand-off gate.

**BASE.** Main at launch, with quinn's BBR. The cells are Bulk, with `--window-reliable`, as in §48b.

**Ratios lead.** Main carries 39 engine commits since §48b's BASE (`0a1ddb88`), among them the snapshot-order law and F2's framing. The latest main guard (§64, v2, main `3b8fca94`) reads c2 goodput 90.19 and c8 97.27 Mbit/s, close to §48b's BASE. CPU/GB has drifted ≈ +30 % (c2 cpu_cli 39.8 against 30.9), so absolute CPU is not predicted.

| cell | readout | §48b CAND/BASE | §48d predicted CAND/BASE | mechanism |
|---|---|---|---|---|
| c1s | goodput (row-median) | 1.023 | **At 5 GB rows (v1 draft): 1.00–1.06**, CAND's lowest row ≥ 0.95 × its median, ≥ 1 BASE row < 0.5 × median with probability ≈ 0.6–0.8. **At guard-size rows (v2): 0.97–1.03**, and no BASE collapse expected (§64's guard-size c1s rows on main read 674–806 Mbit/s, share of qcwnd below BDP 0.000) | quinn's collapse is unchanged on main (4 of 12 rows at 5 GB in §48b) but needs long rows to show; ProbeRtt's cost ≤ 0.05 % |
| c1s | cpu_cli/GB; cpu_srv/GB | 0.982; 0.982 | **0.95–0.99; 0.96–1.00** | the ring saves ≈ 300 ns × ≈ 90 k pkt/s ≈ 1.5 % of client CPU (inference from the host bench) |
| c1s | wire RTT (mean of row medians) | 1.32 (5.0 against 3.8 ms) | **1.15–1.60**; CAND 4.6–5.8 ms | W ≈ 2·b̂·m̂ + A with m̂ 2.15–2.4 ms |
| c1s | delivered delay A_x (p50 of row medians) | 0.94 (18.0 / 19.2 ms) | **0.80–1.05**; CAND 14–21 ms | the cap is at N·P on every c1s line, but the store is rarely full (CAND paused 0.1 %), so A_x is not n/x here |
| c1s | in-order delay mean (reported only at c1) | 0.91 (20 / 22 ms) | **0.80–1.05** | |
| c2 | goodput | 0.985 | **0.987–0.995** | ProbeRtt 0.84 % → 0.13–0.23 %; unattributed ≈ 0.6 % stays |
| c2 | cpu_cli/GB; cpu_srv/GB | 0.984; 0.970 | **0.96–1.00; 0.94–1.00** | the ring saves ≈ 0.7 % of client CPU |
| c2 | wire RTT | 0.19 (21.8 / 112.4) | **0.17–0.29**; CAND 21–31 ms | 2·m̂ + A/x with m̂ at exit 7.8–12.4 ms (sim exits 8.0–12.35) |
| c2 | delivered delay A_x | 1.003 | **0.97–1.04**; both ≈ 195–220 ms | (I1)+(I2): both arms at n = 2 048 after the ramp (cap at N·P on ≥ 0.75 of lines at t ≥ 1 s); CAND B ≈ 1 700–1 830 |
| c2 | in-order delay mean | 1.017 (350 / 344 ms) | **0.98–1.06** | sojourn equal; hole clock equal on one leg |
| c8 | goodput | 0.974 | **0.974–0.988** | ProbeRtt 0.78 % → 0.17–0.32 %; the tail via H0 stays |
| c8 | cpu_cli/GB; cpu_srv/GB | 1.035; 1.053 | **1.015–1.035; 1.035–1.065** | ring ≈ −0.7 point on the client; the server's finer slow-leg I/O is untouched |
| c8 | wire RTT, fast leg; slow leg | 0.18; 0.38 | **0.17–0.28; 0.35–0.45** (CAND 21–31; 85–100 ms) | m̂ 8.3–9.6 / 39.4–41 ms |
| c8 | A_x, fast leg; slow leg | 0.65; 1.015 | **0.55–0.80; 0.90–1.15** | (I2): leg 0 ≈ 21 + B0/x0; leg 1's queue as in §1.2 |
| c8 | in-order delay mean | 1.46 (6.14 / 4.20 s) | **1.30–1.60** | resends wait in leg 1's quinn queue (§1.2); neither fix touches it |
| c8 | B1, median over 4–16 s (witness) | CAND 482–989, BASE 0–5 | **CAND 450–1 100; BASE ≤ 10** | the cap-step bursts (§1.2) |

**Readouts the draft registers, predicted.**
- **`FIX-INERT`** (C_R > 0.4 % at c2 or > 0.6 % at c8): **not predicted**.
- **`RATCHET`** (c2 `qrt_us` p50 > 10 ms, or a row's p90 > 12 ms): **possible on noise, probability ≈ 0.3**. The mechanism cannot ratchet at f = 0.75 (§1.4), but the one-round minimum reads high (sim exits up to 12.35 ms).
- **R2b ≤ 0.02** at every cell (controlled).

**Label.**
- **`REGRESSION` ≈ 0.6.** Through p_rest at c8's server CPU, and under v2 also through the c8 in-order clause alone.
- **`CONTROLLED-WITH-COST` ≈ 0.3.** If c8's server CPU lands inside its A/A.
- **`PROMOTE-ELIGIBLE` ≤ 0.1.** It needs c2's goodput clause to miss a 0.8 % shift.

**The verdict cannot read delay.** Under v1 none of these delay readouts is scored. Under v2 only the in-order mean is.

## 7. Tests the law carries (red first, absolute; for the build in §5 step 3)

1. **Gate decision, absolute** (paper §6.8 revision 4; s = 1 233 B).
   - At cwnd = 280 000 B, srtt = 22 ms, τ̂_o = 2.00 ms, ℓ̂_o = 16 µs, τ̂_p = 2.06 ms and ℓ̂_p = 96 µs: L = 4.172 ms, R = 15 909 091 B/s and β = 53.83, so hand at B = 53 and not at 54.
   - The second leg (cwnd = 230 000 B, srtt = 88 ms, τ̂_o = 2.00 ms, ℓ̂_o = 10 µs, τ̂_p = 2.07 ms, ℓ̂_p = 557 µs: L = 4.637 ms, R = 3 267 045 B/s) has β = 12.29 and hands at 12, not at 13. Each leg is decided on its own inputs.
   - A cold start with every timer term at its 1 ms floor gives L = 2.112 ms and β = 27.25.
   - At B ≥ β a resend, a repair or a control datagram is still staged (they are exempt); a first send is not.
2. **No dial is read.** Decision traces are byte-identical across Bulk, Auto and Realtime, across ±2 % δ nudges at each preset, and at ρ < 1. This is the NO-MODE-SWITCH routing witness, extending the `test_visualizer_ui` pattern to the engine.
3. **Clamp (last-turn predicate), absolute.**
   - A 36/s call with r_law = 0.25 over 1 000 sources, each turn ending dry, sends exactly 250 repairs, each staged in its source's turn.
   - The same call under overload (the path refusing first sends) sends 0 repairs.
   - A store-full turn leaves the predicate unchanged. At cold start it reads +∞.
   - A turn that admits sources and then closes the gate sets spare = 0 for that path until a turn ends dry.
   - At N = 2, with leg 0 saturated (every turn closed) and leg 1 dry, r_law = 0.25 over 1 000 sources sends exactly 250 repairs, all placed on leg 1, and the scheduler-wide spare reads +∞.
   - With both legs closed, it sends 0.
   - Repair-off traces are identical.
4. **The leg-1 burst, red first.** In `cc_sim` or the engine sim: two legs at 100 and 20 Mbit, a store-cap step of +2 000 seqs, `bbr_rs`.
   - Without the gate, B1 > 400 for ≥ 2 s. This reproduces §1.2, and it is the red.
   - With the gate, B1 ≤ β1 + one batch on ≥ 99 % of polls, and `starve_1` ≤ 1 %.
   - Mutation: β × 100 must turn the bound red.
5. **The resend's wait.** In the same sim, a resend handed to leg 1 waits ≤ β1/x1 + g in quinn. Red without the gate (≈ 0.5 s).
6. **T = ∞ is inert.** Intake decisions at T = ∞ expire nothing at δ ∈ {Bulk, Auto, Realtime, 50}, and the T = ∞ trace equals the ρ = 1 trace of the same build at the same δ.
7. **The store image.** With the gate on, `store_pin` = 0 in the c2-like sim.
8. **Bounded β across silence, red first.** At item 1's c2 inputs, a 1.5 s silence (no hand-off, no ack, no inbound) is followed by β within ±10 % of its steady 53.83, so in [48.4, 59.2]. Red on the measured-interval form, where β ≈ 19 000.
9. **Reopen without inbound traffic, red first (a logic test).** On a host loopback with paused time, the gate closes on a view published 0.5 ms before the owner's last I/O. The owner then idles with no inbound datagram while quinn drains.
   - The producer must hand again within one republish period plus one poll period plus one owner poll and one producer iteration: ≤ 2.2 ms of virtual time.
   - Red on today's owner, whose view stays stale for ≥ 100 ms.
   - Virtual time cannot show timer latency. The realized bound L (≈ 4–6 ms) is read on the VM by `reopen_i`.
10. **Repair at β < 1.** A 36/s call with cwnd 11 datagrams, srtt 100 ms and L = 4.2 ms has β = 1.25·13 563/0.1·0.0042/1 233 ≈ 0.58. Over 1 000 sources with r_law = 0.25 it sends exactly 250 repairs, each staged in its source's turn. A second source staged before quinn has sent the first waits no longer than L.
11. **Post-outage drain witness (three-knob verdict).** In the L0 netem loopback (`RWM_L0_NETEM`, as `gate_c9_outage_recovery` uses): one path, Auto (repair on), a 1.5 s full outage, GATE.
    - Over the drain after the outage, `spare_bind` ≥ 0.5 of first sends, and the coded share in the drain is ≤ 0.2 × its pre-outage value.
    - This is the §12.4 bracket's binding side. It is asserted on the host because the VM harness cannot cut a link.

## 8. Open questions for the owner

- **Q-S1. What does "T = ∞ ≡ Bulk" mean?** The three-knob identity is T = ∞ ≡ today's ρ = 1 at the same δ, and this law keeps that. If the owner means the stronger "the same sender at every δ" (an object at a Realtime price behaves exactly like Bulk), then every downstream δ reader fails it, and so does r(β(δ)). That would decide the seat for the intake, but it is not the model's current reading.
- **Q-S2. Which guard for §48d: v1 (the draft) or v2 (accepted in §41d, with the in-order clause)?** Under v2 the c8 in-order cost (+30 to +60 %) fails on its own.
- **Q-S3. §48d's host gate requires T6's f = 1.25 mutation to be red on the fixed tree, and it is not** (plateau, ignored). It is red on the shipped tree (×2 per period), so the gate can see a ratchet there. The options:
  - require the mutation red on the shipped tree only, and T6 as a no-growth gate on the fixed one;
  - find a mutation that ratchets on the fixed form (f ≈ 2 on §1.4's fitted reading, unverified);
  - drop the mutation requirement.
- **Q-S4. Remove the store law's δ term?** The store becomes the resource bound N·P. This withdraws W1.2's "δ on the store" step, touches ADR-0072's multiplier, and needs a ledger verdict on the pin (MD18).
- **Q-S5. Accept a δ-free wire standing queue under saturation** (≤ b̂·m̂ + A, the controller's margin), giving up W1.2 A's goodput-for-delay brake?
- **Q-S6. Register `bbr_rs` together with the gate as the promotion candidate (§5 steps 3–4),** instead of promoting `bbr_rs` alone? Its delay costs are the queue's, and only the gate removes them.
- **Q-S7. The engine in-flight count** reads ≈ 60× low at c2, and placement, the gate and the clamp read it today. Fix it, or retire it from those three sites with this law (§2.6, §2.7)?
- **Q-S8. The δ seat rides on Revision 4 §13.3,** which is unconfirmed (Q24, Q33, Q34; B′ not run). This law's δ-free part ships without it.
- **Q-S10 (ruled 2026-11-26).** Reading cwnd and in-flight is allowed for the diagnostic, behind its default-off environment arm, from `RouteProbe` atomics and never from `congestion_state()`. Paper §6.8 states the §8.4 amendment the law needs; accepting it is the owner's call before any scored session.
- **Q-S11 (stays blocked).** At ρ < 1 the gate adds a source pause where today there is none. The missing shedding law is the three-knob lifetime T (L1 and the wait limit), so promotion at ρ < 1 depends on the three-knob intake. No interim law is added.
- **Q-S9. §48d's c1s row size.** The v1 draft uses 5 GB rows, where quinn's BASE collapses. Guard v2 uses guard-size rows, where §64 saw no collapse. The c1s predictions differ by that choice (§6).

## 9. Defect findings (MD18; ledger verdicts owed)

1. **The shipped store law operates as a constant at c1s, c2 and c8.** Its cap sits at the N·P clamp on 100 % of c1s lines, 80–82 % of c2's and 71–72 % of c8's at t ≥ 1 s, in both arms; at c2 and c8 that is every line after the ramp ends (≈ 8–10 s), and at c2 the store is full on 99.7 % (`handoff/q7_out.txt`, `q3_out.txt`). Its inputs over-read 7–15× (Copa BtlBw and the echo-clock RTprop, §1.3). Its δ term has no authority there. This re-scopes any §48b reading of "the store's δ" (there is none), and it is F01's pin seen again.
2. **The engine in-flight count is not the outstanding count.** 33–37 at c2 against ≈ 2 000. The send gate, the repair clamp and placement's load term read it. Cause unidentified.
3. **Quinn's datagram queue is unmanaged.** It is a 4 MiB FIFO with drop-oldest, shared by first sends, resends, repair and control, and invisible to every engine law. Under `bbr_rs` it holds 0.2–0.5 s.

**Roadmap lines** (one node each, each owing a ledger verdict):
- *Store law operates as a constant:* the shipped cap (1 + q(δ))·Σ bw·RTprop sits at its N·P clamp on 100 % of c1s, 80–82 % of c2 and 71–72 % of c8 lines in §48b under both controllers, because its inputs over-read 7–15×, so its δ term has no authority.
- *Engine in-flight is not the outstanding count:* per-path `in_flight` reads 33–37 at c2 against ≈ 2 000 outstanding, yet the send gate, the repair clamp's spare and placement's I/W term read it; the cause is unidentified (candidate: the ack arms' expected − received release).
- *Quinn's datagram queue is unmanaged:* one 4 MiB drop-oldest FIFO per path, with no priority or deadline, holds the store's surplus (840 under quinn's BBR and 1 800 under `bbr_rs` at c2; 480–990 on c8's slow leg) in front of every resend and control datagram; fix proposed in paper §6.8.
- *Placement is blind to quinn's queue:* its price reads the wire RTT and a near-zero I/W, so under `bbr_rs` it over-offers c8's saturated slow leg at each store-cap step, and the bursts wait there 0.24–0.49 s.

**Paper text: done** (§6.8 and an Appendix A.5 line; `tools/build_paper.sh` builds). **Owed:** the roadmap nodes above and a node for this law (`docs/roadmap/nodes.yaml`, with `build.py` and `check.py`).
