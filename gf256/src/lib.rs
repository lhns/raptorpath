//! GF(2^8) finite field arithmetic.
//!
//! Galois Field with 256 elements using irreducible polynomial 0x11D (x^8 + x^4 + x^3 + x^2 + 1),
//! the same polynomial used by AES and Reed-Solomon standards.
//!
//! Scalar operations use log/exp table lookups for O(1) multiplication.
//! Bulk operations (`mul_acc_slice`, `mul_slice`) use SIMD acceleration (AVX2/SSSE3)
//! when available, with automatic runtime CPU feature detection.

/// Irreducible polynomial for GF(2^8): x^8 + x^4 + x^3 + x^2 + 1 = 0x11D
const POLY: u16 = 0x11D;

/// Generate log and exp tables at compile time.
const fn gen_tables() -> ([u8; 256], [u8; 256]) {
    let mut exp = [0u8; 256];
    let mut log = [0u8; 256];

    let mut x: u16 = 1;
    let mut i = 0u16;
    while i < 255 {
        exp[i as usize] = x as u8;
        log[x as usize] = i as u8;
        x <<= 1;
        if x & 0x100 != 0 {
            x ^= POLY;
        }
        i += 1;
    }
    // exp[255] wraps to exp[0] = 1 (since α^255 = 1 in GF(2^8))
    exp[255] = exp[0];
    // log[0] is undefined (log of zero), leave as 0 — callers must check for zero

    (exp, log)
}

const TABLES: ([u8; 256], [u8; 256]) = gen_tables();
const EXP_TABLE: [u8; 256] = TABLES.0;
const LOG_TABLE: [u8; 256] = TABLES.1;

/// GF(2^8) addition (XOR).
#[inline(always)]
pub fn add(a: u8, b: u8) -> u8 {
    a ^ b
}

/// GF(2^8) multiplication via log/exp tables.
#[inline(always)]
pub fn mul(a: u8, b: u8) -> u8 {
    if a == 0 || b == 0 {
        return 0;
    }
    let log_a = LOG_TABLE[a as usize] as u16;
    let log_b = LOG_TABLE[b as usize] as u16;
    let log_sum = (log_a + log_b) % 255;
    EXP_TABLE[log_sum as usize]
}

/// GF(2^8) multiplicative inverse: inv(a) * a = 1.
/// Panics if a == 0 (zero has no inverse).
#[inline(always)]
pub fn inv(a: u8) -> u8 {
    debug_assert!(a != 0, "zero has no multiplicative inverse in GF(2^8)");
    let log_a = LOG_TABLE[a as usize] as u16;
    EXP_TABLE[(255 - log_a) as usize]
}

/// Checked multiplicative inverse: returns `None` for zero, `Some(inv(a))` otherwise.
/// Defense-in-depth for callers that may not have already guarded against zero.
#[inline(always)]
pub fn checked_inv(a: u8) -> Option<u8> {
    if a == 0 {
        None
    } else {
        Some(inv(a))
    }
}

// ---------------------------------------------------------------------------
// SIMD module (x86_64 only) + runtime dispatch
// ---------------------------------------------------------------------------

#[cfg(target_arch = "x86_64")]
mod simd;

use std::sync::OnceLock;

type MulFn = fn(u8, &[u8], &mut [u8]);
type XorFn = fn(&[u8], &mut [u8]);

static MUL_ACC_FN: OnceLock<MulFn> = OnceLock::new();
static MUL_SLICE_FN: OnceLock<MulFn> = OnceLock::new();
static XOR_ACC_FN: OnceLock<XorFn> = OnceLock::new();

fn detect_mul_acc() -> MulFn {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") {
            return simd::mul_acc_avx2_dispatch;
        }
        if is_x86_feature_detected!("ssse3") {
            return simd::mul_acc_ssse3_dispatch;
        }
    }
    mul_acc_slice_scalar
}

fn detect_mul_slice() -> MulFn {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") {
            return simd::mul_slice_avx2_dispatch;
        }
        if is_x86_feature_detected!("ssse3") {
            return simd::mul_slice_ssse3_dispatch;
        }
    }
    mul_slice_scalar
}

fn detect_xor_acc() -> XorFn {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") {
            return simd::xor_acc_avx2_dispatch;
        }
        if is_x86_feature_detected!("ssse3") {
            return simd::xor_acc_ssse3_dispatch;
        }
    }
    xor_acc_scalar
}

// ---------------------------------------------------------------------------
// Scalar implementations (fallback + SIMD tail processing)
// ---------------------------------------------------------------------------

fn xor_acc_scalar(src: &[u8], dst: &mut [u8]) {
    for (d, &s) in dst.iter_mut().zip(src.iter()) {
        *d ^= s;
    }
}

/// Scalar multiply-accumulate for coeff ∉ {0, 1}.
pub(crate) fn mul_acc_slice_scalar(coeff: u8, src: &[u8], dst: &mut [u8]) {
    let log_c = LOG_TABLE[coeff as usize] as u16;
    for (d, &s) in dst.iter_mut().zip(src.iter()) {
        if s != 0 {
            let log_s = LOG_TABLE[s as usize] as u16;
            *d ^= EXP_TABLE[((log_c + log_s) % 255) as usize];
        }
    }
}

/// Scalar multiply-slice for coeff ∉ {0, 1}.
pub(crate) fn mul_slice_scalar(coeff: u8, src: &[u8], dst: &mut [u8]) {
    let log_c = LOG_TABLE[coeff as usize] as u16;
    for (d, &s) in dst.iter_mut().zip(src.iter()) {
        if s == 0 {
            *d = 0;
        } else {
            let log_s = LOG_TABLE[s as usize] as u16;
            *d = EXP_TABLE[((log_c + log_s) % 255) as usize];
        }
    }
}

// ---------------------------------------------------------------------------
// Public API (unchanged signatures)
// ---------------------------------------------------------------------------

/// Multiply-accumulate: `dst[i] ^= coeff * src[i]` for all i.
/// This is the hot loop for RLC encoding/decoding.
/// Uses SIMD acceleration (AVX2/SSSE3) when available.
#[inline]
pub fn mul_acc_slice(coeff: u8, src: &[u8], dst: &mut [u8]) {
    if coeff == 0 {
        return;
    }
    if coeff == 1 {
        (XOR_ACC_FN.get_or_init(detect_xor_acc))(src, dst);
        return;
    }
    (MUL_ACC_FN.get_or_init(detect_mul_acc))(coeff, src, dst);
}

/// Multiply a slice by a scalar: `dst[i] = coeff * src[i]`.
/// Uses SIMD acceleration (AVX2/SSSE3) when available.
#[inline]
pub fn mul_slice(coeff: u8, src: &[u8], dst: &mut [u8]) {
    if coeff == 0 {
        for d in dst.iter_mut() {
            *d = 0;
        }
        return;
    }
    if coeff == 1 {
        dst[..src.len()].copy_from_slice(src);
        return;
    }
    (MUL_SLICE_FN.get_or_init(detect_mul_slice))(coeff, src, dst);
}

// ---------------------------------------------------------------------------
// SplitMix64 PRNG — fast, deterministic, good avalanche for coefficient generation
// ---------------------------------------------------------------------------

/// SplitMix64 PRNG for deterministic coefficient generation.
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }

    pub fn next_nonzero_u8(&mut self) -> u8 {
        loop {
            let v = self.next_u64() as u8;
            if v != 0 {
                return v;
            }
        }
    }
}

/// Generate deterministic coefficients for a window-mode repair symbol.
pub fn generate_window_coefficients(
    window_start: u64,
    window_count: u16,
    repair_index: u32,
) -> Vec<u8> {
    let seed = window_start
        .wrapping_mul(65537)
        .wrapping_add((window_count as u64) << 48)
        .wrapping_add(repair_index as u64);
    let mut rng = SplitMix64::new(seed);
    (0..window_count).map(|_| rng.next_nonzero_u8()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_is_xor() {
        assert_eq!(add(0x53, 0xCA), 0x53 ^ 0xCA);
        assert_eq!(add(0, 42), 42);
        assert_eq!(add(42, 42), 0); // a + a = 0 in GF(2^8)
    }

    #[test]
    fn test_mul_identity() {
        for a in 0..=255u8 {
            assert_eq!(mul(a, 1), a);
            assert_eq!(mul(1, a), a);
        }
    }

    #[test]
    fn test_mul_zero() {
        for a in 0..=255u8 {
            assert_eq!(mul(a, 0), 0);
            assert_eq!(mul(0, a), 0);
        }
    }

    #[test]
    fn test_mul_commutative() {
        for a in 1..=255u8 {
            for b in 1..=255u8 {
                assert_eq!(mul(a, b), mul(b, a));
            }
        }
    }

    #[test]
    fn test_inv() {
        for a in 1..=255u8 {
            assert_eq!(mul(a, inv(a)), 1, "a={a}: a * inv(a) should be 1");
        }
    }

    #[test]
    fn test_mul_acc_slice() {
        let src = [1u8, 2, 3, 4, 5];
        let mut dst = [0u8; 5];
        mul_acc_slice(3, &src, &mut dst);
        for (i, &s) in src.iter().enumerate() {
            assert_eq!(dst[i], mul(3, s));
        }
        // Accumulate again
        let prev = dst;
        mul_acc_slice(7, &src, &mut dst);
        for (i, &s) in src.iter().enumerate() {
            assert_eq!(dst[i], add(prev[i], mul(7, s)));
        }
    }

    #[test]
    fn test_checked_inv() {
        assert_eq!(checked_inv(0), None);
        for a in 1..=255u8 {
            let inv_a = checked_inv(a).unwrap();
            assert_eq!(mul(a, inv_a), 1);
        }
    }

    #[test]
    fn test_mul_slice_basic() {
        let src = [10u8, 20, 30];
        let mut dst = [0u8; 3];
        mul_slice(5, &src, &mut dst);
        for (i, &s) in src.iter().enumerate() {
            assert_eq!(dst[i], mul(5, s));
        }
    }

    #[test]
    fn test_generate_window_coefficients_deterministic() {
        let c1 = generate_window_coefficients(100, 10, 5);
        let c2 = generate_window_coefficients(100, 10, 5);
        assert_eq!(c1, c2);
        // All coefficients should be non-zero
        assert!(c1.iter().all(|&c| c != 0));
    }

    #[test]
    fn test_generate_window_coefficients_different_seeds() {
        let c1 = generate_window_coefficients(100, 10, 5);
        let c2 = generate_window_coefficients(100, 10, 6);
        assert_ne!(c1, c2);
    }

    // --- SIMD correctness tests ---

    #[test]
    fn test_mul_acc_all_coefficients() {
        // Verify SIMD matches scalar for all 255 non-zero coefficients
        let mut rng = SplitMix64::new(0xDEADBEEF);
        let src: Vec<u8> = (0..1200).map(|_| rng.next_u64() as u8).collect();

        for coeff in 1..=255u8 {
            let mut dst_simd = vec![0u8; 1200];
            let mut dst_scalar = vec![0u8; 1200];
            mul_acc_slice(coeff, &src, &mut dst_simd);
            mul_acc_slice_scalar(coeff, &src, &mut dst_scalar);
            assert_eq!(dst_simd, dst_scalar, "mismatch for coeff={coeff}");
        }
    }

    #[test]
    fn test_mul_acc_edge_lengths() {
        let sizes = [0, 1, 15, 16, 17, 31, 32, 33, 63, 64, 65, 100, 1200];
        let mut rng = SplitMix64::new(0xCAFEBABE);

        for &sz in &sizes {
            let src: Vec<u8> = (0..sz).map(|_| rng.next_u64() as u8).collect();
            let mut dst_simd = vec![0u8; sz];
            let mut dst_scalar = vec![0u8; sz];

            mul_acc_slice(42, &src, &mut dst_simd);
            mul_acc_slice_scalar(42, &src, &mut dst_scalar);
            assert_eq!(dst_simd, dst_scalar, "mul_acc mismatch at size={sz}");
        }
    }

    #[test]
    fn test_mul_slice_simd_vs_scalar() {
        let coeffs = [0u8, 1, 2, 42, 128, 255];
        let mut rng = SplitMix64::new(0x12345678);
        let src: Vec<u8> = (0..1200).map(|_| rng.next_u64() as u8).collect();

        for &c in &coeffs {
            let mut dst_dispatch = vec![0xFFu8; 1200];
            let mut dst_expected = vec![0xFFu8; 1200];

            mul_slice(c, &src, &mut dst_dispatch);
            // Compute expected with scalar mul
            for (i, &s) in src.iter().enumerate() {
                dst_expected[i] = mul(c, s);
            }
            assert_eq!(dst_dispatch, dst_expected, "mul_slice mismatch for coeff={c}");
        }
    }

    #[test]
    fn test_mul_acc_accumulation_semantics() {
        let mut rng = SplitMix64::new(0xABCD);
        let src: Vec<u8> = (0..512).map(|_| rng.next_u64() as u8).collect();
        let mut dst = vec![0u8; 512];

        // Two successive mul_acc_slice calls should XOR correctly
        mul_acc_slice(17, &src, &mut dst);
        mul_acc_slice(42, &src, &mut dst);

        for (i, &s) in src.iter().enumerate() {
            let expected = mul(17, s) ^ mul(42, s);
            assert_eq!(dst[i], expected, "accumulation mismatch at i={i}");
        }
    }

    #[test]
    fn test_xor_acc_via_coeff_one() {
        let mut rng = SplitMix64::new(0x9999);
        let src: Vec<u8> = (0..1200).map(|_| rng.next_u64() as u8).collect();
        let initial: Vec<u8> = (0..1200).map(|_| rng.next_u64() as u8).collect();
        let mut dst = initial.clone();

        mul_acc_slice(1, &src, &mut dst);

        for i in 0..1200 {
            assert_eq!(dst[i], initial[i] ^ src[i], "xor_acc mismatch at i={i}");
        }
    }
}
