//! The engine's one nearest-rank quantile.
//!
//! Every gauge that prints a quantile of a sample set uses this convention:
//! nearest rank on `round((len − 1)·q)`, no interpolation, so two reads of
//! one sample set always agree and the value printed is always a sample that
//! actually occurred. An empty set reads as `T::default()` (zero); callers
//! print the sample count beside it, so an empty zero is never confusable
//! with a measured one.

/// The `q`-quantile (`q ∈ [0, 1]`) of an already-sorted slice.
pub(crate) fn nearest_rank<T: Copy + Default>(sorted: &[T], q: f64) -> T {
    if sorted.is_empty() {
        return T::default();
    }
    let idx = ((sorted.len() - 1) as f64 * q).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::nearest_rank;

    #[test]
    fn nearest_rank_rounds_half_away_and_reads_a_real_sample() {
        let s: Vec<u32> = (1..=10).collect();
        assert_eq!(nearest_rank(&s, 0.0), 1);
        assert_eq!(nearest_rank(&s, 0.5), 6); // round(4.5) = 5 -> s[5]
        assert_eq!(nearest_rank(&s, 0.9), 9); // round(8.1) = 8
        assert_eq!(nearest_rank(&s, 0.99), 10); // round(8.91) = 9
        assert_eq!(nearest_rank(&s, 1.0), 10);
        assert_eq!(nearest_rank::<u32>(&[], 0.5), 0);
        assert_eq!(nearest_rank::<f32>(&[], 0.5), 0.0);
        assert_eq!(nearest_rank(&[7u32], 0.99), 7);
    }
}
