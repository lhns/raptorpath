//! The model's three-knob laws (src/knobs.rs) against vectors generated from
//! the offline simulation's own law functions (tests/gen_knob_vectors.py;
//! plan docs/derivations/visualizer-three-knobs-plan.md §5.3). Bit for bit.

use raptorpath_wasm::knobs::*;

const VECTORS: &str = include_str!("knob_vectors.txt");

fn f(h: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(h, 16).expect("hex f64"))
}

fn bit(s: &str) -> bool {
    match s {
        "0" => false,
        "1" => true,
        _ => panic!("bool must be 0 or 1, got {s}"),
    }
}

#[test]
fn knob_vectors_match_toy() {
    let header = VECTORS
        .lines()
        .find_map(|l| l.strip_prefix("# KNOB_LAW_REVISION: "))
        .expect("revision header");
    assert_eq!(header, KNOB_LAW_REVISION, "vectors were generated for another law revision");
    let mut n = 0usize;
    let mut by_name = std::collections::BTreeMap::<&str, usize>::new();
    for line in VECTORS.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()) {
        let (lhs, rhs) = line.split_once(" -> ").expect("arrow");
        let mut it = lhs.split(' ');
        let name = it.next().unwrap();
        let a: Vec<&str> = it.collect();
        let (want, _src) = rhs.split_once(' ').expect("result and source");
        let ok = match name {
            "intake_expired" => intake_expired(f(a[0]), f(a[1])) == bit(want),
            "within_lifetime" => within_lifetime(f(a[0]), f(a[1])) == bit(want),
            "resend_admissible" => resend_admissible(f(a[0]), f(a[1]), f(a[2])) == bit(want),
            "dequeue_expired" => dequeue_expired(f(a[0]), f(a[1]), f(a[2])) == bit(want),
            "protected" => {
                protected(f(a[0]), f(a[1]), f(a[2]), f(a[3]), f(a[4]), f(a[5])) == bit(want)
            }
            "release_at" => {
                release_at(f(a[0]), f(a[1]), f(a[2]), f(a[3])).to_bits() == f(want).to_bits()
            }
            "released_by_hold" => released_by_hold(f(a[0]), f(a[1]), f(a[2]), f(a[3])) == bit(want),
            "ack_timeout" => {
                ack_timeout(f(a[0]), f(a[1]), f(a[2]), f(a[3])).to_bits() == f(want).to_bits()
            }
            "trigger_delay" => {
                trigger_delay(bit(a[0]), f(a[1]), f(a[2]), f(a[3])).to_bits() == f(want).to_bits()
            }
            "one_way_fwd" => one_way_fwd(f(a[0]), f(a[1])).to_bits() == f(want).to_bits(),
            other => panic!("unknown law in vectors: {other}"),
        };
        assert!(ok, "vector mismatch: {line}");
        *by_name.entry(name).or_default() += 1;
        n += 1;
    }
    // Every law function is covered, so a dropped generator section fails.
    for name in [
        "intake_expired", "within_lifetime", "resend_admissible", "dequeue_expired",
        "protected", "release_at", "released_by_hold", "ack_timeout", "trigger_delay", "one_way_fwd",
    ] {
        assert!(by_name.get(name).copied().unwrap_or(0) > 0, "no vectors for {name}");
    }
    assert!(n > 500, "only {n} vectors");
}
