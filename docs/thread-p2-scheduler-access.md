# Threading P2 — every access to the shared `Scheduler` and `FecRateController`

The P2 exploration the plan asks for before any P2 code: every access, from
every task, to the two pieces of engine state that were shared behind
mutexes on `main` e655484 (`Arc<SchedMutex>` = P1's witnessed
`parking_lot::Mutex<Scheduler>`; `Arc<parking_lot::Mutex<FecRateController>>`),
each classified:

- **(a) logic-owned** — moves into the logic actor;
- **(b) per-path state** that a later phase (P2b) moves to the path's I/O
  worker;
- **(c) a read-mostly snapshot** the actor publishes (atomics / `ArcSwap`);
- **(d) none of these** — reported, not forced.

Line numbers are e655484's. "Sites" counts `lock()` statements (a statement
may read several fields under one guard).

## 1. Who touched them (e655484)

| task (spawn site) | file / function | sched sites | fec sites | per datagram? |
|---|---|---|---|---|
| sender `run_window_sender` (`net/mod.rs` 1059) | `net/mod.rs` loop: WindowStart broadcast 1900, rate/BDP refresh 2234/2254, pooled-law refresh 2296, tail clock 2630/2678, react-cap SRTT 2817, Shutdown broadcast 2923, NACK inputs 3130, repair budget 3161–3162 (fec+sched), gap/repair placement 3300/3320/3348; `[ETA]` flush 2037 (`try_lock_for`) | 14 | 1 | yes (≈ 1–2 per loop iteration, amortized over the `EMIT_BATCH` burst) |
| sender | `net/emit_source.rs` `emit_source`: placement + charge 303/306/342/404/457/464/470/481, taper 532, `p_lost` 711, correction 821/849; `cadenced_repair_rate` 893–894 (fec+sched) | 14 | 1 | yes (≈ 5 per source symbol) |
| sender | `net/sender_phases.rs` `refresh_store_cap` 110/154/178/218; `emit_generation_coded` 550/579/608/631/701/734; `serve_gaps` 868/1160/1263/1279/1303/1313/1343; `on_ack_advance` 1479–1480 (fec+sched), 1639 | 19 | 1 | yes (store cap, ack advance) / on events (gaps, generation) |
| sender | `net/diag.rs` `report` 498/511/784/997 (`RWM_DIAG`, 250 ms); `net/ackdiag.rs` `maybe_report` 571 (`RWM_ACKDIAG`) | 5 | 0 | no (cadenced) |
| receiver `run_receiver` (`net/mod.rs` 1270) — server: data role | `net/receiver.rs`: `touch_path` 1216, arrival/jitter estimator + `[ETA]` lag 1221, incoming loss 2162, ack jitter read 1959, hold/deficit SRTT reads 530/686/722/801/1694, live-path lists for broadcasts and gauges 560/901/960/971/1049/1093/2022; fec `feedback_update_window` 2072 | 16 | 1 | yes: 1216, 1221, 1959, 2162 per data message; 722/1694 per message while a hole is pending |
| receiver — client: ack role | `net/control_msg.rs` `handle_control_message`: `on_window_ack` 519 (one guard over delivery/RTT/loss/pool/cc update), `on_ack` 233, `on_path_report` 425, `touch_path` 160 (Ping); `net/copa_feed.rs` `copa_feed_attribute` 231 (from `on_window_ack`); `net/ackdiag.rs` (from `on_window_ack`) | 5 | 0 | yes: one long guard per inbound WindowAck (≈ 1 per data datagram) |
| report `run_report` (`net/mod.rs` 1347) | `net/tasks/report.rs` 50 (path ids), 56 (send-rate feed, dead-path check, MTU store, in-flight expiry, PathReport build) | 2 | 0 | no (2 s) |
| control fast path (`net/mod.rs` 1363) | `control_fastpath.rs` → `handle_control_message` (PathReport/Ping/Pong: 160, 425) | via 2 | 0 | no |
| path-command processor (`net/mod.rs` 1330) | `net/tasks/path_cmd.rs` `add_path` 42, `remove_path` 60 | 2 | 0 | no (HTTP API) |
| datagram readers, uni-stream readers (`transport/quic.rs` 806/834) | — | 0 | 0 | — |
| status HTTP, signal handler, L0 shim, TUN reader/writer | — | 0 | 0 | — |

Totals: **77 scheduler sites, 5 FEC-controller sites, in 5 tasks.** Nothing
outside these five tasks touches either: quinn callbacks (the passthrough
CC) read `transport`'s `cc_windows` DashMap, not the scheduler; the status
HTTP server reads `SharedStats` only.

## 2. Classification

**(a) logic-owned — 71 scheduler + 5 FEC sites.** Every sender site but
the `[ETA]` flush (51: placement, charge/release, pooled store cap, tail and repair clocks,
budgets, taper, diag/ackdiag cadences, broadcasts' path lists), the
client's ack handling (`on_window_ack`, `on_ack`, `copa_feed_attribute`,
`on_path_report`), the report tick's send-rate feed / dead-path check /
in-flight expiry / PathReport build, the path add/remove, the receiver's
SRTT / RTprop / ε̂ reads that size its hold, deficit and request clocks, the
receiver's live-path lists, and every FEC-controller access (sender taper
and budget, receiver PI feedback). These read and write the estimator and
CC state the sender's laws consume; P2a moves them into the actor.

**(b) per-path state, later to the path's I/O worker — 5 sites** (plus
the MTU store inside the report guard). The receive-direction per-path
observations written per datagram: `touch_path` on arrival (receiver
1216; Ping 160), the arrival/jitter estimator `record_arrival` (1221), the
incoming-loss feed `record_incoming_loss` (2162, fed by `PathBatchTracker`,
itself already per path) and the ack's per-path jitter read (1959); and,
inside the report guard, the MTU store (`max_datagram_size` is a quinn
per-connection query, report 56). In P2a they stay in the actor
(they are `PathState` fields, written in the same estimator the sender
reads `srtt`/`min_rtt` from); moving them needs the `PathState` rx/tx split
that P2b's worker ownership implies.

**(c) read-mostly snapshots the actor publishes — 0 new.** Every reader
outside the five tasks already reads a published value: `SharedStats`
per-path atomics (`jitter_us`, `rtt_us`, `active`, counters; the HTTP
status and `[DIAG]`), `window_ack_seq` / `peer_window_ack` (atomics), the
substrate `cc_windows` (DashMap, written from `on_window_ack`). With all
five tasks inside the actor no `ArcSwap` is needed in P2a.

**(d) none of these — 1 site.** The sender's `[ETA]` exit flush
(`net/mod.rs` 2037) runs in a destructor that may execute at runtime
teardown off the actor's poll; it is neither logic nor a snapshot. P2a
gives it a non-blocking, non-asserting `try_borrow_mut` (it may lose the
line; it never blocks, as `try_lock_for` never hung).

## 3. What P2a does with it (decisions)

- **One actor, five sub-futures.** The sender, the receiver role, the
  report, the control fast path and the path-command processor run as
  sub-futures of ONE task (`crate::actor::ActorSet`), which owns the
  scheduler and the FEC controller in `ActorCell`s (no mutex; an
  off-owner borrow panics; the guard is `!Send`, so a borrow across an
  `.await` does not compile). Class (a) needs no messages: its code already
  runs in the owning task. The only cross-task hops left are the inbound
  channel (datagram readers → receiver role, drained as batches with
  `recv_many`) and the existing outbound ones.
- **The server's receiver role is the same actor, not a sibling.** Its
  per-datagram writes (class b: `touch_path`, `record_arrival`,
  `record_incoming_loss`) land in the same `PathState` estimators the
  sender role reads (`srtt`, `min_rtt`), and its hold/deficit clocks read
  them back. A sibling task would need either a per-datagram message to
  the actor or a `PathState` rx/tx split — the latter is P2b/P3's
  structural change, not P2a's. In the measured cells the server's sender
  role is idle (bulk is one-way), so co-location costs the server only the
  idle sub-futures' polls, which the per-sub-future wakers make zero. The
  receiver-role split stays P3's, gated by §9 finding 1 (server receiver
  92 % busy at c1d).
- **Client acks run in the actor.** `on_window_ack` executes in the same
  task as the sender; a batch of acks from the readers wakes the parked
  actor once, and the receiver → sender `AckWake` is served in the same
  poll (the sub-future poller wakes the parent task only when the actor is
  not already polling).
- **Not a measurement arm.** The cell replaces the mutex at the type level
  in every signature of the 77 sites; keeping the shared-mutex topology as
  an `RWM_TOPO` arm would duplicate the machine. The comparison is P2a's
  binary against main's binary, interleaved.

## 4. What threading Q2 does with it (the split by direction)

P2a is deleted (status §12). Q2 (plan v2) splits the state by direction
instead of moving it into one actor: the window **sender** owns the TX half
and the FEC controller (`net/tx_inputs.rs` `TxCore`, plain `&mut`), the
**receiver** owns the RX half (`scheduler/rx.rs` `RxScheduler`, a local).
Every site of §1, after the split:

| site (§1) | goes to | how |
|---|---|---|
| every sender site (`net/mod.rs`, `emit_source.rs`, `sender_phases.rs`, `diag.rs`, `ackdiag.rs`) — class (a) | TX half | `&mut Scheduler` / `&mut FecRateController`, no lock |
| client ack role: `on_window_ack`, `on_ack`, `copa_feed_attribute`, the `ackdiag` feed | TX half | runs IN the sender: the path owner routes WindowAck / Ack to the sender's input channel (`recv_many`, loop top + always-armed arm) |
| report task (send-rate feed, dead-path check, MTU store, in-flight expiry, PathReport build) | TX half | `SenderCmd::ReportTick` → `tasks::report::report_tick_tx` in the sender; the report task keeps the stream sends |
| fast path `on_path_report`, Ping `touch_path` | TX half | forwarded to the sender's input channel; the sender stages the Pong |
| path-cmd `add_path` / `remove_path` | TX half | `SenderCmd::AddPath` (awaited) / `RemovePath` |
| class (b): `record_arrival` (receiver 1221), `record_incoming_loss` (2162), the ack's jitter read (1959) | RX half | receiver-owned `RxScheduler` |
| class (d): the sender `[ETA]` exit flush | dissolved | `TxCore` owns the scheduler beside the flush; its destructor renders it, no lock to try |

**Sites the split found that fit neither half (reported, plan Q2; the
exploration's "class (c) = 0" holds only for the one-actor shape it was
made for):**

1. **The receiver's reads of TX estimator state** — SRTT (hold / deficit /
   horizon / shed clocks, receiver 530/686/722/801/1694), RTprop and SRTT
   for the `[ETA]` reference (1221), the RTT jitter and σ (the refresh
   clock and `[QCLK]`, 722). Resolved by publication: the sender writes
   them per path into `PathStats::xdir` after every input batch it
   processes (and at startup / path add / the report tick); the receiver
   reads the atomics. New atomics in the existing `SharedStats` (the
   plan's cross-direction channel); a reader may be one publication stale.
2. **The receiver's live-path lists** (broadcast WindowAcks, deficit and
   request reports, `[CTLD]`, `[WEDGE]`; 560/901/960/971/1049/1093/2022) —
   liveness is TX state. Read off the existing `PathStats::active` flag,
   which the sender now publishes in both directions (through Q1 the flag
   was set false by the dead check and never set true again on revival —
   a monitoring inconsistency the publication fixes).
3. **`touch_path` on a data arrival** (receiver 1216) — an RX event that
   writes TX state (the dead-path clock, and on revival the cwnd / pacing /
   Copa reset). The receiver stamps the arrival (`xdir.rx_seen_us`, one
   relaxed store); the sender applies it as `touch_path_at(stamp)` in
   `Scheduler::sync_rx` at every loop top and before the dead check; when
   the published flag reads the path dead, the receiver also sends one
   `SenderCmd::Revive` so a parked sender wakes for it.
4. **The sender's reads of RX state** — `nack_effectiveness()` (the RX loss
   EWMA, NACK budget, mod.rs 3201), the arrival-jitter fallback of
   `rtt_jitter_us()` and the emission's `est.jitter_us()` (emit_source
   589), and the PathReport's `loss_rate` / `jitter_us`. Resolved by the
   mirror: the receiver publishes both values after every update; the
   sender copies them into its estimator (`LossEstimator::set_rx_mirror`)
   at every loop top.
5. **The FEC controller's receiver site** (`feedback_update_window`,
   receiver 2072, 2 s) — a TX-state input produced by the decoder;
   delivered as `SenderCmd::FecFeedback`. (The method is a no-op body
   today; the message keeps the structure honest.)
6. **The MTU store** (exploration class (b), inside the report guard) — not
   RX: `min_mtu` is read by the sender. It stays TX, written by the report
   tick's command from the owner's published view.
