//! Threading P1, D1 end to end (MEASUREMENT DISCIPLINE rules 1 and 14): the
//! shipped binary, a reliable bulk transfer over one shaped loopback path
//! (`c3`, 20 Mbit: the sender is store/cwnd-bound, so it pauses), `RWM_DIAG=1`.
//! The client sender's last `[DIAG]` carries the cumulative `wake[..]` counts.
//!
//!   * `wake[ack] > 0`: the ack-wake arm fired — the receiver routed the
//!     sender's `Notify` and the sender awaited it (the wiring executes);
//!   * `wake[paused] <= 0.05 × wake[ack]`: while paused with acks flowing,
//!     the loop is woken by acks, not by the 1 ms timer — the absolute D1
//!     invariant this battery cell family is scored on;
//!   * the transfer completes (the arm changes wake timing only).
//!
//! The numbers are printed (the component statement the V-P1 battery is read
//! against). Red on 8d7d8c1: no `wake[` token exists.

#[path = "common/gauge.rs"]
mod gauge;
#[path = "common/loopback.rs"]
mod loopback;

#[test]
fn a_paused_sender_is_woken_by_acks_not_by_the_timer() {
    let env = [("RWM_DIAG", "1"), ("RUST_LOG", "raptorpath=info")];
    let (cli, _srv) = loopback::transfer(loopback::Transfer {
        env: &env,
        bytes: "4000000",
        runs: "2",
        ..loopback::Transfer::default()
    });
    let last = cli
        .lines()
        .filter(|l| l.contains("[DIAG] t="))
        .last()
        .unwrap_or_else(|| panic!("no [DIAG] line with RWM_DIAG=1:\n{cli}"));
    let i = last.find(" wake[").unwrap_or_else(|| panic!("no wake[ token: {last}"));
    // The token's body: `tun=… paused=… … ack=…` (after ` wake[`, up to `]`).
    let tok = &last[i + " wake[".len()..];
    let tok = &tok[..tok.find(']').expect("wake token closes")];
    let n = |k: &str| gauge::u64_field(tok, &format!("{k}="));
    let (paused, ack, tun, pace) = (n("paused"), n("ack"), n("tun"), n("pace"));
    println!("[ack-wake] {tok}  (paused/ack = {:.4})", paused as f64 / ack.max(1) as f64);
    assert!(tun > 0, "the transfer never read intake: {tok}");
    assert!(ack > 0, "the ack-wake arm never fired on a store-bound sender: {tok}");
    assert!(
        paused as f64 <= 0.05 * ack as f64,
        "a paused sender is still woken by the 1 ms timer ({paused} timer wakes vs {ack} ack wakes): {tok}"
    );
    let _ = pace;
}
