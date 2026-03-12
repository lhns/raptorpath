# ADR-0008: Receiver Doesn't Handle BlockStart Messages

## Status
**Resolved** — sender sends `BlockStart` before symbols via `encode_and_send_block()`. Receiver handles `BlockStart` in `handle_control_message()` to create decoders with correct params.

## Context
The sender is supposed to send a `ControlMessage::BlockStart { params, transfer_length }` before transmitting symbols for a new block. The receiver needs this to create a properly configured decoder.

## Problem
1. **Sender never sends BlockStart**: no code in the sender task emits this message
2. **Receiver never handles it**: `handle_control_message()` has no match arm for `BlockStart`
3. **Decoder created with wrong params**: when the first symbol arrives, a decoder is created with `source_symbols: 0` and `transfer_length: MAX_BLOCK_SIZE`

With `source_symbols: 0`, the decoder's `is_complete_source()` check incorrectly returns true immediately, and the direct-source reconstruction path produces empty data.

## Decision Required
### Sender: send BlockStart before each block
```rust
for (path_id, _) in &assignments {
    transport.send_control(*path_id, ControlMessage::BlockStart {
        params: encoding_params,
        transfer_length: block_data.len() as u64,
    }).await?;
}
```

### Receiver: create decoder from BlockStart
```rust
ControlMessage::BlockStart { params, transfer_length } => {
    recv_decoders.entry(params.block_id)
        .or_insert_with(|| Decoder::new(params, transfer_length));
}
```

### Handle out-of-order: symbol before BlockStart
If a symbol arrives before its BlockStart (possible with multipath), create a placeholder decoder and update it when BlockStart arrives. Or buffer symbols for unknown blocks briefly.

## Consequences
- Correct decoder initialization
- Slight latency from reliable BlockStart delivery
- Need to handle BlockStart/symbol ordering race

## Related
- ADR-0005 (ACK mechanism — BlockStart should use reliable stream)
