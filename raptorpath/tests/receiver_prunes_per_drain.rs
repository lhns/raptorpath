//! Threading R3 routing gate (CLAUDE.md rule 1: prove the mechanism executes).
//! Paper §5.2 "Retained state": the receiver evaluates the decode-inert floor
//! at every wait (once per drained `recv_many` batch), not on the 2 s report
//! tick. A source scan, because the prune is a statement inside the receiver
//! loop; the loopback witness is the `dec[... recovered=]` field of `[RDIAG]`.

const SRC: &str = include_str!("../src/net/receiver.rs");

#[test]
fn the_decoder_prune_runs_at_the_wait_point_not_in_the_report_block() {
    let adv = "window_decoder.advance(";
    assert_eq!(SRC.matches(adv).count(), 1, "exactly one decoder prune site");
    let adv_pos = SRC.find(adv).unwrap();
    let wait = SRC
        .find("let waiting = queued.is_empty();")
        .expect("the loop's wait point");
    let flush = SRC
        .find("recv_transport.flush(&mut b).await;")
        .expect("the staged-ack flush at the wait point");
    let tick = SRC
        .find("now.duration_since(last_nack_time) >= REPORT_INTERVAL")
        .expect("the 2 s report block");
    assert!(
        wait < adv_pos && adv_pos < flush && flush < tick,
        "the prune must sit between the `waiting` binding and the ack flush, \
         ahead of (outside) the REPORT_INTERVAL block"
    );
    // Guarded by `waiting`; the other two prune lines travel with it.
    let guard = &SRC[wait..adv_pos];
    assert!(guard.contains("if waiting {"), "prune must be guarded by `if waiting {{`");
    assert!(guard.contains("received_seqs.prune_below("), "{guard}");
    assert!(guard.contains("recv_above.sync("), "{guard}");
    // No prune call remains in or after the report block.
    let block = &SRC[tick..];
    assert!(!block.contains("prune_below("), "prune_below left in the tick block");
    assert!(!block.contains(".advance(prune_before"), "advance left in the tick block");
}

#[test]
fn rdiag_carries_the_decoder_state_witness() {
    assert!(SRC.contains("dec[{"), "the [RDIAG] line must append the decoder's diag_stats");
}
