//! Threading P1, D1 end to end (MEASUREMENT DISCIPLINE rules 1 and 14): the
//! shipped binary, a reliable bulk transfer over one shaped loopback path
//! (`c3`, 20 Mbit: the sender is store/cwnd-bound, so it pauses), `RWM_DIAG=1`.
//! The client sender's last `[DIAG]` carries the cumulative `wake[..]` counts.
//!
//!   * `wake[ack] > 0`: the ack-wake arm fired — the receiver routed the
//!     sender's `Notify` and the sender awaited it (the wiring executes);
//!   * `wake[timer_acked] <= 0.05 × wake[ack]`: while paused with acks
//!     flowing, the loop is woken by acks, not by the 1 ms timer — a timer
//!     wake during whose wait an ack landed is only a `select!` tie (the
//!     absolute D1 invariant the V-P1 battery reads);
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
    let (paused, ack, tun, acked) = (n("paused"), n("ack"), n("tun"), n("timer_acked"));
    println!(
        "[ack-wake] {tok}  (timer_acked/ack = {:.4}, paused/ack = {:.4})",
        acked as f64 / ack.max(1) as f64,
        paused as f64 / ack.max(1) as f64
    );
    assert!(tun > 0, "the transfer never read intake: {tok}");
    assert!(ack > 0, "the ack-wake arm never fired on a store-bound sender: {tok}");
    // With acks flowing, the ack ends the wait: a 1 ms timer wake during
    // whose wait an ack landed is only the `select!` tie (both ready at the
    // same poll). `paused` itself also counts timer wakes in true ack gaps
    // longer than the poll, which a 20 Mbit / 20 ms cell has by construction
    // (measured 943 of 4469 paused-type wakes before `timer_acked` existed);
    // those are legitimate and not bounded here.
    assert!(
        acked as f64 <= 0.05 * ack as f64,
        "acks landed during {acked} timer-resolved waits vs {ack} ack wakes: the ack does not wake \
         the paused sender: {tok}"
    );
}
