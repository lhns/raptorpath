//! Paper §5.2 "Retained state" over a real transfer: with the decoder prune
//! at the receiver's wait point (once per drained batch) the server's decoder
//! holds at most 2·W_cap + one drain of recovered payloads, instead of ~2 s of
//! them (the prune ran on the 2 s report tick). The witness is the receiver's
//! `[RDIAG]` line (`RWM_RDIAG=1`): `dec[... recovered=R ...] wcap=W
//! prune_n=P prune_max_us=U`.
//!
//! One clean bulk `--window-reliable` transfer. The rate-only `CLEAN_RATE`
//! shaper (no loss, no delay) bounds the run's wall time from below so there
//! are several 500 ms `[RDIAG]` lines whatever the host's speed; it is also
//! what makes the base red (≈ 4 k symbols/s × 2 s of retained payloads, above
//! the bound). Own test binary: the env is process-global.

#[path = "common/loopback.rs"]
mod loopback;

/// `MSG_CHANNEL_DEPTH` (net/mod.rs): the receiver's inbound queue depth, i.e.
/// the most one drain can hold of symbols not yet pruned.
const ONE_DRAIN_WORST_CASE: u64 = 4096;

fn field(line: &str, key: &str) -> Option<u64> {
    let i = line.find(key)? + key.len();
    let digits: String = line[i..].chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

#[test]
fn the_decoder_holds_two_w_cap_plus_one_drain_not_two_seconds() {
    let (cli, srv) = loopback::transfer(loopback::Transfer {
        env: &[("RWM_RDIAG", "1")],
        client_env: &loopback::CLEAN_RATE,
        bytes: "20000000",
        runs: "1",
        // [RDIAG] prints per received message, not on idle, so none post-dates
        // the transfer; the lines of interest are already in the log.
        srv_tag: None,
        ..Default::default()
    });
    loopback::assert_clean_rate_executed(&cli, 20_000_000, 1);
    let lines: Vec<&str> = srv.lines().filter(|l| l.contains("[RDIAG]")).collect();
    assert!(lines.len() >= 3, "need >= 3 [RDIAG] lines, got {}:\n{srv}", lines.len());
    let mut max_prune_n = 0;
    for l in &lines {
        assert!(l.contains("dec["), "no decoder witness on `{l}`");
        let r = field(l, "recovered=").unwrap_or_else(|| panic!("no recovered= on `{l}`"));
        let w = field(l, "wcap=").unwrap_or_else(|| panic!("no wcap= on `{l}`"));
        let p = field(l, "prune_n=").unwrap_or_else(|| panic!("no prune_n= on `{l}`"));
        assert!(
            r <= 2 * w + ONE_DRAIN_WORST_CASE,
            "recovered {r} > 2*W_cap + one drain = {} on `{l}`",
            2 * w + ONE_DRAIN_WORST_CASE
        );
        max_prune_n = max_prune_n.max(p);
    }
    assert!(
        max_prune_n >= 2,
        "prune_n never reached 2 in a 500 ms window (the 2 s tick gives at most 1): {lines:#?}"
    );
}
