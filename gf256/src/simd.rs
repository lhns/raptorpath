//! SIMD-accelerated GF(2^8) multiply-accumulate using split-table PSHUFB.
//!
//! Decomposes each source byte into low/high nibbles, uses PSHUFB as a parallel
//! 16-entry table lookup, and XORs the two halves. Eliminates the per-byte branch,
//! dependent table lookups, and modular arithmetic from the scalar hot loop.

#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

/// Build two 16-entry lookup tables for split-nibble multiplication.
/// `c * b = table_lo[b & 0x0F] ^ table_hi[b >> 4]`
#[inline]
fn build_mul_tables(c: u8) -> ([u8; 16], [u8; 16]) {
    let mut lo = [0u8; 16];
    let mut hi = [0u8; 16];
    for i in 0..16 {
        lo[i] = crate::mul(c, i as u8);
        hi[i] = crate::mul(c, (i as u8) << 4);
    }
    (lo, hi)
}

// ---------------------------------------------------------------------------
// SSSE3 kernels (16 bytes/iteration)
// ---------------------------------------------------------------------------

#[target_feature(enable = "ssse3")]
unsafe fn mul_acc_ssse3(c: u8, src: &[u8], dst: &mut [u8]) {
    let len = src.len().min(dst.len());
    let (lo_tbl, hi_tbl) = build_mul_tables(c);
    let tbl_lo = _mm_loadu_si128(lo_tbl.as_ptr().cast());
    let tbl_hi = _mm_loadu_si128(hi_tbl.as_ptr().cast());
    let mask = _mm_set1_epi8(0x0F);

    let mut i = 0;
    while i + 16 <= len {
        let s = _mm_loadu_si128(src.as_ptr().add(i).cast());
        let d = _mm_loadu_si128(dst.as_ptr().add(i).cast());
        // _mm_srli_epi16 shifts 16-bit lanes; mask clears leaked high bits
        let prod = _mm_xor_si128(
            _mm_shuffle_epi8(tbl_lo, _mm_and_si128(s, mask)),
            _mm_shuffle_epi8(tbl_hi, _mm_and_si128(_mm_srli_epi16(s, 4), mask)),
        );
        _mm_storeu_si128(
            dst.as_mut_ptr().add(i).cast(),
            _mm_xor_si128(d, prod),
        );
        i += 16;
    }
    crate::mul_acc_slice_scalar(c, &src[i..len], &mut dst[i..len]);
}

#[target_feature(enable = "ssse3")]
unsafe fn mul_slice_ssse3(c: u8, src: &[u8], dst: &mut [u8]) {
    let len = src.len().min(dst.len());
    let (lo_tbl, hi_tbl) = build_mul_tables(c);
    let tbl_lo = _mm_loadu_si128(lo_tbl.as_ptr().cast());
    let tbl_hi = _mm_loadu_si128(hi_tbl.as_ptr().cast());
    let mask = _mm_set1_epi8(0x0F);

    let mut i = 0;
    while i + 16 <= len {
        let s = _mm_loadu_si128(src.as_ptr().add(i).cast());
        let prod = _mm_xor_si128(
            _mm_shuffle_epi8(tbl_lo, _mm_and_si128(s, mask)),
            _mm_shuffle_epi8(tbl_hi, _mm_and_si128(_mm_srli_epi16(s, 4), mask)),
        );
        _mm_storeu_si128(dst.as_mut_ptr().add(i).cast(), prod);
        i += 16;
    }
    crate::mul_slice_scalar(c, &src[i..len], &mut dst[i..len]);
}

#[target_feature(enable = "ssse3")]
unsafe fn xor_acc_ssse3(src: &[u8], dst: &mut [u8]) {
    let len = src.len().min(dst.len());
    let mut i = 0;
    while i + 16 <= len {
        let s = _mm_loadu_si128(src.as_ptr().add(i).cast());
        let d = _mm_loadu_si128(dst.as_ptr().add(i).cast());
        _mm_storeu_si128(
            dst.as_mut_ptr().add(i).cast(),
            _mm_xor_si128(s, d),
        );
        i += 16;
    }
    for j in i..len {
        dst[j] ^= src[j];
    }
}

// ---------------------------------------------------------------------------
// AVX2 kernels (32 bytes/iteration)
// ---------------------------------------------------------------------------

#[target_feature(enable = "avx2")]
unsafe fn mul_acc_avx2(c: u8, src: &[u8], dst: &mut [u8]) {
    let len = src.len().min(dst.len());
    let (lo_tbl, hi_tbl) = build_mul_tables(c);
    // vpshufb operates on independent 128-bit lanes — broadcast into both
    let tbl_lo_128 = _mm_loadu_si128(lo_tbl.as_ptr().cast());
    let tbl_hi_128 = _mm_loadu_si128(hi_tbl.as_ptr().cast());
    let tbl_lo = _mm256_broadcastsi128_si256(tbl_lo_128);
    let tbl_hi = _mm256_broadcastsi128_si256(tbl_hi_128);
    let mask = _mm256_set1_epi8(0x0F);

    let mut i = 0;
    while i + 32 <= len {
        let s = _mm256_loadu_si256(src.as_ptr().add(i).cast());
        let d = _mm256_loadu_si256(dst.as_ptr().add(i).cast());
        let prod = _mm256_xor_si256(
            _mm256_shuffle_epi8(tbl_lo, _mm256_and_si256(s, mask)),
            _mm256_shuffle_epi8(tbl_hi, _mm256_and_si256(_mm256_srli_epi16(s, 4), mask)),
        );
        _mm256_storeu_si256(
            dst.as_mut_ptr().add(i).cast(),
            _mm256_xor_si256(d, prod),
        );
        i += 32;
    }
    // 16-byte remainder (VEX-encoded SSE — no transition penalty)
    if i + 16 <= len {
        let mask_128 = _mm_set1_epi8(0x0F);
        let s = _mm_loadu_si128(src.as_ptr().add(i).cast());
        let d = _mm_loadu_si128(dst.as_ptr().add(i).cast());
        let prod = _mm_xor_si128(
            _mm_shuffle_epi8(tbl_lo_128, _mm_and_si128(s, mask_128)),
            _mm_shuffle_epi8(tbl_hi_128, _mm_and_si128(_mm_srli_epi16(s, 4), mask_128)),
        );
        _mm_storeu_si128(
            dst.as_mut_ptr().add(i).cast(),
            _mm_xor_si128(d, prod),
        );
        i += 16;
    }
    crate::mul_acc_slice_scalar(c, &src[i..len], &mut dst[i..len]);
}

#[target_feature(enable = "avx2")]
unsafe fn mul_slice_avx2(c: u8, src: &[u8], dst: &mut [u8]) {
    let len = src.len().min(dst.len());
    let (lo_tbl, hi_tbl) = build_mul_tables(c);
    let tbl_lo_128 = _mm_loadu_si128(lo_tbl.as_ptr().cast());
    let tbl_hi_128 = _mm_loadu_si128(hi_tbl.as_ptr().cast());
    let tbl_lo = _mm256_broadcastsi128_si256(tbl_lo_128);
    let tbl_hi = _mm256_broadcastsi128_si256(tbl_hi_128);
    let mask = _mm256_set1_epi8(0x0F);

    let mut i = 0;
    while i + 32 <= len {
        let s = _mm256_loadu_si256(src.as_ptr().add(i).cast());
        let prod = _mm256_xor_si256(
            _mm256_shuffle_epi8(tbl_lo, _mm256_and_si256(s, mask)),
            _mm256_shuffle_epi8(tbl_hi, _mm256_and_si256(_mm256_srli_epi16(s, 4), mask)),
        );
        _mm256_storeu_si256(dst.as_mut_ptr().add(i).cast(), prod);
        i += 32;
    }
    if i + 16 <= len {
        let mask_128 = _mm_set1_epi8(0x0F);
        let s = _mm_loadu_si128(src.as_ptr().add(i).cast());
        let prod = _mm_xor_si128(
            _mm_shuffle_epi8(tbl_lo_128, _mm_and_si128(s, mask_128)),
            _mm_shuffle_epi8(tbl_hi_128, _mm_and_si128(_mm_srli_epi16(s, 4), mask_128)),
        );
        _mm_storeu_si128(dst.as_mut_ptr().add(i).cast(), prod);
        i += 16;
    }
    crate::mul_slice_scalar(c, &src[i..len], &mut dst[i..len]);
}

#[target_feature(enable = "avx2")]
unsafe fn xor_acc_avx2(src: &[u8], dst: &mut [u8]) {
    let len = src.len().min(dst.len());
    let mut i = 0;
    while i + 32 <= len {
        let s = _mm256_loadu_si256(src.as_ptr().add(i).cast());
        let d = _mm256_loadu_si256(dst.as_ptr().add(i).cast());
        _mm256_storeu_si256(
            dst.as_mut_ptr().add(i).cast(),
            _mm256_xor_si256(s, d),
        );
        i += 32;
    }
    if i + 16 <= len {
        let s = _mm_loadu_si128(src.as_ptr().add(i).cast());
        let d = _mm_loadu_si128(dst.as_ptr().add(i).cast());
        _mm_storeu_si128(
            dst.as_mut_ptr().add(i).cast(),
            _mm_xor_si128(s, d),
        );
        i += 16;
    }
    for j in i..len {
        dst[j] ^= src[j];
    }
}

// ---------------------------------------------------------------------------
// Safe dispatch wrappers
// ---------------------------------------------------------------------------

pub(crate) fn mul_acc_ssse3_dispatch(c: u8, src: &[u8], dst: &mut [u8]) {
    // Safety: only called when SSSE3 is detected at runtime
    unsafe { mul_acc_ssse3(c, src, dst) }
}

pub(crate) fn mul_acc_avx2_dispatch(c: u8, src: &[u8], dst: &mut [u8]) {
    // Safety: only called when AVX2 is detected at runtime
    unsafe { mul_acc_avx2(c, src, dst) }
}

pub(crate) fn mul_slice_ssse3_dispatch(c: u8, src: &[u8], dst: &mut [u8]) {
    unsafe { mul_slice_ssse3(c, src, dst) }
}

pub(crate) fn mul_slice_avx2_dispatch(c: u8, src: &[u8], dst: &mut [u8]) {
    unsafe { mul_slice_avx2(c, src, dst) }
}

pub(crate) fn xor_acc_ssse3_dispatch(src: &[u8], dst: &mut [u8]) {
    unsafe { xor_acc_ssse3(src, dst) }
}

pub(crate) fn xor_acc_avx2_dispatch(src: &[u8], dst: &mut [u8]) {
    unsafe { xor_acc_avx2(src, dst) }
}
