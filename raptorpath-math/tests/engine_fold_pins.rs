//! Bit pins for the math the engine folded onto this crate (cleanup
//! Stage 3): `normal_quantile` (was copied in the engine's
//! `control/changepoint.rs` and `control/estimator.rs`), and
//! `normal_survival`, `p_fec_normal` and `p_lost` (were copied in the
//! engine's `control/fec_rate.rs`). Before the copies were deleted they were
//! checked bit-identical to these definitions over a dense grid (every
//! `to_bits()` equal); the engine now calls these directly. The pins below
//! keep the single remaining definition from drifting under either consumer
//! (the engine or the wasm model).

use raptorpath_math::{normal_quantile, normal_survival, p_fec_normal, p_lost};

#[test]
fn normal_quantile_is_bit_pinned() {
    for (p, bits) in [
        (0.001, 0xc008b963b77a4bb3u64),
        (0.025, 0xbfff5dc70f770608),
        (0.1, 0xbff481f6033859a1),
        (0.5, 0x0000000000000000),
        (0.9, 0x3ff481f6033859a1),
        (0.975, 0x3fff5dc70f770606),
        (0.999, 0x4008b963b77a4bb3),
    ] {
        assert_eq!(normal_quantile(p).to_bits(), bits, "normal_quantile({p})");
    }
}

#[test]
fn normal_survival_is_bit_pinned() {
    for (z, bits) in [
        (-3.0, 0x3feff4f0e9d5a279u64),
        (-1.0, 0x3feaec4bcbd00e4e),
        (0.0, 0x3fdfffffff768fa1),
        (0.5, 0x3fd3bf143a0c15c0),
        (2.0, 0x3f974bcae054b1d4),
        (7.9, 0x3cd95e59316335b0),
    ] {
        assert_eq!(normal_survival(z).to_bits(), bits, "normal_survival({z})");
    }
}

#[test]
fn p_fec_normal_is_bit_pinned() {
    for ((r, e, w, s), bits) in [
        ((0.15, 0.1, 50.0, 1.0), 0x3fe8eeadf69c746fu64),
        ((0.15, 0.1, 50.0, 3.0), 0x3fe5ba1835ebc746),
        ((0.12, 0.1, 50.0, 1.0), 0x3fe2433b19963fc6),
        ((0.2, 0.025, 384.0, 1.5), 0x3ff0000000000000),
    ] {
        assert_eq!(p_fec_normal(r, e, w, s).to_bits(), bits, "p_fec_normal({r}, {e}, {w}, {s})");
    }
}

#[test]
fn p_lost_is_bit_pinned() {
    for ((a, e, s, v), bits) in [
        ((0.0, 0.025, 0.05, 0.005), 0x3f9999999999999au64),
        ((0.05, 0.025, 0.05, 0.005), 0x3fa8f9c190022264),
        ((0.2, 0.025, 0.05, 0.005), 0x3ff0000000000000),
        ((0.03, 0.1, 0.02, 0.0), 0x3ff0000000000000),
    ] {
        assert_eq!(p_lost(a, e, s, v).to_bits(), bits, "p_lost({a}, {e}, {s}, {v})");
    }
}
