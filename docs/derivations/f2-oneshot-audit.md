# F2 audit: state set only by a one-shot or unreliable control message

Scope: every receiver or sender state that is set by a control message sent
once, or only as an unreliable datagram, with no periodic refresh. Facts at
main `c98ecdeb`. Line numbers are in `raptorpath/src/`. "Fix" lines are one
line each and are not proposals.

Transport facts used below:
- Control datagrams (`send_control_datagram`, `transport/quic.rs:801`) are
  unreliable QUIC datagrams, staged in the control lane.
- Control streams (`send_control`, `transport/quic.rs:808`) are reliable.
- The connection handshake is a reliable stream (`transport/io_owner.rs:741-794`).

## 1. `WindowStart { symbol_size, backend, packed }`

Sent once per path, as a datagram, at sender start; the send result is
discarded (`net/mod.rs:1862-1871`, `let _ =`). Never re-sent. The path set is
fixed at that instant (`control_broadcast_paths`, `net/channel_set.rs:114`);
a path added later (`next_path_id`, `net/mod.rs:1286`) never gets one.

Receive side: `net/receiver.rs:2151-2154` reads `packed` only. The generic
handler (`net/control_msg.rs:122-124`) only logs.

| field | state set | file:line | if the message is lost | reachable |
|---|---|---|---|---|
| `packed` | receiver `window_packed`, default false | `receiver.rs:138`, set `:2153`, read at `:1016`, `:1524`, `:1621`, `:1756` via `extract_window_packets` (`net/mod.rs:1372`) | Realtime (`use_packing`, `sender_policy.rs:678`) writes BE-length packed symbols (`framing.rs:20`); the unpacked reader is LE (`framing.rs:81-89`) and yields no packet. The receiver still records the delivery and acks the seq (`receiver.rs:1610-1640`), so the sender prunes it and ARQ does not notice. Dead for the connection while counters look healthy. Bulk and Auto do not pack, so the default matches and a lost message is harmless there. | Yes: observed (2 of 3 E-rehearsal rows; 1 of 6 committed Realtime rows). Also if `WindowStart` is delayed or reordered behind the first data symbols: symbols read before it are misread. Multi-path: only needs the first one to arrive on any path. |
| `symbol_size` | none: `recv_symbol_size` comes from local config (`net/mod.rs:1171`, passed `:1244`, used at `receiver.rs:117`) | `net/control_msg.rs:122-124` (log only) | Nothing changes. | Not state-setting. A peer with a different local `symbol_size` is not detected by this message. |
| `backend` | none: `recv_fec_backend` is the local `effective_fec_backend` (`net/mod.rs:1164`) | `net/control_msg.rs:122-124` (log only) | Nothing changes. | Not state-setting. A backend mismatch between peers is not detected by this message. |

## 2. `Shutdown`

Sent once per path as a datagram on sender shutdown (`net/mod.rs:2982-2985`).
Receive side only logs (`net/control_msg.rs:119-121`).

- Lost: nothing is set, so nothing is lost; the receiver's own shutdown comes
  from its channel or broadcast, and the peer is detected as gone by quinn.
- Reachable: yes, but no state depends on it.

## 3. `WindowSwitch { flush_seq, new_backend, symbol_size }`

Never sent by this binary. Receive side warns and ignores it
(`net/receiver.rs:2160-2167`, `net/control_msg.rs:136-138`). No state.
Not reachable.

## 4. `Handshake { version, max_block_size, symbol_size, path_id }`

Reliable stream (`transport/io_owner.rs:741-794`), so no loss. The local value
is the constant `symbol_size: 1200` (`:794`); the peer's `symbol_size` and
`max_block_size` are read into `peer` and no receiver or sender state is set
from them (no use found by grep on `main`). A version mismatch fails the
connect (`transport/protocol.rs:310-313`).

## 5. Messages that carry state but are periodic (self-healing on loss)

Listed to show the audit's boundary; each is re-sent, so a loss costs one
interval, not the connection.

| message | state it sets | sent | file:line |
|---|---|---|---|
| `WindowAck` | sender's cumulative ack, SACK, RTT echo, `received_above` | per data message and by timers | `net/receiver.rs` ack sites; handled `net/control_msg.rs:174-188` |
| `Ack` (legacy per-batch) | sender's batch-loss / in-flight release | per batch; suppressed under ack-merge | `net/receiver.rs:2131` |
| `PathReport` | sender's per-path loss/RTT/throughput view | periodic, reliable stream (`send_control`) | `net/tasks/report.rs:118`, `:200`; handled `net/control_msg.rs:166-181` |
| `Ping` / `Pong` | path liveness | periodic, stream / datagram | `net/tasks/report.rs:128`; `net/control_msg.rs:183-187` |
| `GenerationDeficit` | sender's per-generation deficit | per receiver cycle, generation mode only | `net/control_msg.rs:125-127` |
| `RepairRequest` | counted and dropped in v8 | never sent in v8 | `net/control_msg.rs:130-132` |

## 6. Other one-shot receiver state

None found in the control plane. The one other piece of receiver framing state
chosen at construction is derived from local config, not from the peer:
`recv_window_reliable` (`receiver.rs:86`, from `window_reliable`). A peer
pair configured with different hints or ρ agrees on it only by configuration;
nothing on the wire checks it. (Mentioned as a fact, outside the one-shot
scope.)

## Summary

Only `packed` is state set by a one-shot unreliable message. Its loss is
reachable and unrecoverable for the connection. `symbol_size` and `backend`
in the same message are not read by the receiver. Fix (one line): carry the
framing in every data symbol or on a reliable channel, never in a one-shot
datagram.
