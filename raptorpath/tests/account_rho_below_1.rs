//! A clean-link, rho < 1 loopback transfer (ADR-0081): the evicting seat
//! (`window_reliable` off, Realtime hint), no loss, runs to completion, and
//! its sender's exit account is read. This is the one real transfer that
//! exercises the account's rho < 1 path outside the sim. `E` and the budget
//! are reported, not gated (`E` is an upper bound on abandonment).
//! Own test binary: spawned server and client.

#[path = "common/loopback.rs"]
mod loopback;

#[test]
fn clean_link_rho_below_1_completes_with_a_clean_account() {
    if !loopback::ACCOUNT_CARRIED {
        return; // a build without the account has nothing to read
    }
    let binds = loopback::free_addrs(1);
    // No `--window-reliable` on either end: the evicting seat.
    let srv = loopback::spawn_perf_server(&binds, &[], &["--protocol-hint", "realtime"]);
    let cli = loopback::run_perf_client(
        &binds,
        &[],
        &["--bytes", "4000000", "--runs", "1", "--protocol-hint", "realtime"],
    );
    // The shared check: identity, O against the receiver frontier, O_strand, inert.
    loopback::assert_no_dnf(&cli, Some(&srv.log.text()));
    let fin: Vec<_> = loopback::account_lines(&cli).into_iter().filter(|l| l.final_line && l.u("S_sent") > 0).collect();
    assert!(!fin.is_empty(), "no data sender exit line:\n{cli}");
    for l in &fin {
        println!("rho<1 clean-link account:{}", l.raw);
        assert_eq!(l.u("reliable"), 0, "this was meant to run the evicting seat:{}", l.raw);
        assert_eq!(l.u("O_strand"), 0, "O_strand:{}", l.raw);
    }
}
