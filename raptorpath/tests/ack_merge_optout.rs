//! ack-merge OPT-OUT gate (goal-gate "Ack-Merge Flip", paper §16.42).
//!
//! `RWM_ACK_MERGE` flipped to DEFAULT ON on 2026-08-08 after passing its own
//! pre-registered gate set. A flip that silently welds the knob shut destroys
//! the A/B arm every future measurement of this mechanism depends on — and
//! the deletion of the now-dead window-mode legacy `Ack` branch is scheduled
//! for refactor seam B2, which means that until B2 lands the opt-out is a
//! REAL code path, not a formality.
//!
//! Own test binary, because `scheduler::ack_merge_active()` caches its
//! resolution in a process-global `OnceLock`: the default-ON assertion lives
//! in `gates.rs`'s `default_env_resolves_the_shipped_stack` (clean env, a
//! different process), and this file owns the `=0` resolution. Nothing here
//! may run in a process that has already resolved the gate.

use raptorpath::{config, gates::RuntimeGates, scheduler};

#[test]
fn zero_opts_out_and_both_read_sites_agree() {
    std::env::set_var("RWM_ACK_MERGE", "0");

    // The raw flag law (`config::env_flag` treats "0"/"false"/empty as off,
    // and ONLY an unset var takes the default) — asserted at the shipped
    // default so the test fails if the default is ever flipped back without
    // this file being revisited.
    assert!(
        !config::env_flag("RWM_ACK_MERGE", true),
        "RWM_ACK_MERGE=0 must opt OUT even though the shipped default is now ON"
    );

    // The cached resolution the RECEIVER arm (`suppress_legacy_ack`) and the
    // SENDER arm (the v6 counter re-homing) both read. They must agree, or
    // one side suppresses the legacy Ack while the other still expects it.
    assert!(
        !scheduler::ack_merge_active(),
        "scheduler::ack_merge_active() must honour the opt-out"
    );
    assert!(
        !RuntimeGates::resolve().ack_merge,
        "RuntimeGates must honour the opt-out (it reads the same cached resolution)"
    );
}

/// The emission law itself, at both settings — the pure function the receiver
/// data arm calls. This is the invariant the flip must not have disturbed:
/// with the merge OFF, `emit == advertise` (the shipped predicate verbatim,
/// byte-identical); with it ON, a datagram goes out unconditionally while
/// what the ack ADVERTISES is unchanged, which is what preserves
/// `GAP_ACK_MIN_INTERVAL`'s rate limit on gap reports.
#[test]
fn the_emission_law_is_unchanged_by_the_default_flip() {
    for adv in [false, true] {
        for gap in [false, true] {
            let (emit_off, advertise_off) = raptorpath::net::window_ack_emission(adv, gap, false);
            let (emit_on, advertise_on) = raptorpath::net::window_ack_emission(adv, gap, true);
            assert_eq!(
                emit_off,
                adv || gap,
                "gate OFF must be the shipped predicate verbatim"
            );
            assert!(emit_on, "gate ON emits once per data message, always");
            assert_eq!(
                advertise_off, advertise_on,
                "what the ack ADVERTISES — and therefore the gap rate limit and \
                 the depth-16 nack/sack channel pressure — is invariant under the gate"
            );
        }
    }
}
