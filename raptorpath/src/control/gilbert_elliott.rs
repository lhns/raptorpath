//! Gilbert-Elliott two-state HMM loss model.
//!
//! Models bursty wireless channels as a two-state Markov chain:
//!   - **Good** state: low loss probability
//!   - **Bad** state: high loss probability (burst losses)
//!
//! The estimator tracks transition counts with exponential decay to
//! adapt to changing channel conditions. When burst lengths exceed a
//! threshold, the FEC rate controller can increase repair symbols to
//! protect against correlated losses that i.i.d. models underestimate.

/// HMM state: either Good (low loss) or Bad (high loss).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HmmState {
    Good,
    Bad,
}

/// Gilbert-Elliott two-state HMM estimator for bursty loss channels.
///
/// Tracks transitions between Good and Bad states using decayed counters,
/// providing estimates of transition probabilities and mean burst length.
#[derive(Debug)]
pub struct GilbertElliottEstimator {
    state: HmmState,
    /// Good → Good transition count (decayed)
    g_to_g: f64,
    /// Good → Bad transition count (decayed)
    g_to_b: f64,
    /// Bad → Good transition count (decayed)
    b_to_g: f64,
    /// Bad → Bad transition count (decayed)
    b_to_b: f64,
    /// Decay factor applied to all counters before each update (e.g., 0.999)
    decay: f64,
    /// Minimum transitions before estimates are considered valid
    min_samples: u64,
    /// Total transitions observed
    total_transitions: u64,
    /// --- window loss-mass statistics (paper Section 8.4.1) ---
    /// Losses observed in the current (partial) mass block.
    mass_cur_losses: f64,
    /// Symbols observed in the current (partial) mass block.
    mass_cur_count: u32,
    /// Ring of the last MASS_SCALES completed block masses.
    mass_ring: [f64; MASS_SCALES_LOCAL],
    /// Completed blocks (mod ring).
    mass_blocks_total: u64,
    /// Nonzero single-scale blocks observed (undecayed validity counter).
    mass_nz_total: u64,
    /// Per-scale decayed counters: all spans / nonzero spans / sum / sum^2.
    mass_cnt: [f64; MASS_SCALES_LOCAL],
    mass_nz: [f64; MASS_SCALES_LOCAL],
    mass_s1: [f64; MASS_SCALES_LOCAL],
    mass_s2: [f64; MASS_SCALES_LOCAL],
}

/// Block scale w0 of the window loss-mass statistic, in wire symbols
/// (the default encoder window; paper Section 8.4.1).
pub const MASS_BLOCK_SCALE: u32 = 64;
/// Local mirror of raptorpath_math::MASS_SCALES (kept equal by the
/// mass_stats() construction below).
const MASS_SCALES_LOCAL: usize = 8;

impl GilbertElliottEstimator {
    pub fn new() -> Self {
        Self {
            state: HmmState::Good,
            g_to_g: 0.0,
            g_to_b: 0.0,
            b_to_g: 0.0,
            b_to_b: 0.0,
            decay: 0.999,
            min_samples: 30,
            total_transitions: 0,
            mass_cur_losses: 0.0,
            mass_cur_count: 0,
            mass_ring: [0.0; MASS_SCALES_LOCAL],
            mass_blocks_total: 0,
            mass_nz_total: 0,
            mass_cnt: [0.0; MASS_SCALES_LOCAL],
            mass_nz: [0.0; MASS_SCALES_LOCAL],
            mass_s1: [0.0; MASS_SCALES_LOCAL],
            mass_s2: [0.0; MASS_SCALES_LOCAL],
        }
    }

    /// Record a single symbol observation.
    /// `received`: true if the symbol was received, false if lost.
    pub fn record_symbol(&mut self, received: bool) {
        let new_state = if received {
            HmmState::Good
        } else {
            HmmState::Bad
        };

        // Decay all counters
        self.g_to_g *= self.decay;
        self.g_to_b *= self.decay;
        self.b_to_g *= self.decay;
        self.b_to_b *= self.decay;

        // Increment the appropriate transition counter
        match (self.state, new_state) {
            (HmmState::Good, HmmState::Good) => self.g_to_g += 1.0,
            (HmmState::Good, HmmState::Bad) => self.g_to_b += 1.0,
            (HmmState::Bad, HmmState::Good) => self.b_to_g += 1.0,
            (HmmState::Bad, HmmState::Bad) => self.b_to_b += 1.0,
        }

        self.state = new_state;
        self.total_transitions += 1;

        // Window loss-mass statistic (paper Section 8.4.1): bin the same
        // per-symbol observations into blocks of MASS_BLOCK_SCALE and
        // track the multi-scale sliding block-mass moments.
        if !received {
            self.mass_cur_losses += 1.0;
        }
        self.mass_cur_count += 1;
        if self.mass_cur_count >= MASS_BLOCK_SCALE {
            self.complete_mass_block();
        }
    }

    /// Fold the completed block into the ring and update the per-scale
    /// decayed moments of the sliding m-block mass sums. Each counter
    /// decays once per block sample — the same per-observation decay
    /// convention as the transition counters.
    fn complete_mass_block(&mut self) {
        let mass = self.mass_cur_losses;
        self.mass_cur_losses = 0.0;
        self.mass_cur_count = 0;
        let idx = (self.mass_blocks_total % MASS_SCALES_LOCAL as u64) as usize;
        self.mass_ring[idx] = mass;
        self.mass_blocks_total += 1;
        if mass > 0.0 {
            self.mass_nz_total += 1;
        }
        for m in 1..=MASS_SCALES_LOCAL {
            if (self.mass_blocks_total as usize) < m {
                break;
            }
            // Sum of the last m completed blocks (ring walk).
            let mut j = 0.0;
            for back in 0..m {
                let i = ((self.mass_blocks_total as usize + MASS_SCALES_LOCAL - 1 - back)
                    % MASS_SCALES_LOCAL) as usize;
                j += self.mass_ring[i];
            }
            let s = m - 1;
            self.mass_cnt[s] *= self.decay;
            self.mass_nz[s] *= self.decay;
            self.mass_s1[s] *= self.decay;
            self.mass_s2[s] *= self.decay;
            self.mass_cnt[s] += 1.0;
            if j > 0.0 {
                self.mass_nz[s] += 1.0;
                self.mass_s1[s] += j;
                self.mass_s2[s] += j * j;
            }
        }
    }

    /// Current HMM state.
    // Test-only consumer: this file's `mod tests` asserts the Good/Bad
    // transition. Not on the data path.
    #[allow(dead_code)]
    pub fn state(&self) -> HmmState {
        self.state
    }

    /// P(Good → Bad): probability of entering a burst.
    pub fn p_gb(&self) -> f64 {
        let total = self.g_to_g + self.g_to_b;
        if total < 1.0 {
            return 0.0;
        }
        self.g_to_b / total
    }

    /// P(Bad → Good): probability of exiting a burst.
    pub fn p_bg(&self) -> f64 {
        let total = self.b_to_g + self.b_to_b;
        if total < 1.0 {
            return 0.0;
        }
        self.b_to_g / total
    }

    /// Mean burst length = 1 / P(Bad → Good).
    /// Returns 1.0 if no burst data is available.
    pub fn mean_burst_length(&self) -> f64 {
        let p = self.p_bg();
        if p < 1e-10 {
            return 1.0;
        }
        1.0 / p
    }

    /// Conditional loss rate given current state.
    /// In Good state: P(Good → Bad), in Bad state: P(Bad → Bad).
    // Test-only consumer: `tests/gate_suite.rs` (the burst-conditional arm)
    // reads it. Integration tests are separate crates, so the lib's dead_code
    // lint cannot see that use — the allow records it instead of hiding it.
    #[allow(dead_code)]
    pub fn conditional_loss_rate(&self) -> f64 {
        match self.state {
            HmmState::Good => self.p_gb(),
            HmmState::Bad => {
                let total = self.b_to_g + self.b_to_b;
                if total < 1.0 {
                    return 0.0;
                }
                self.b_to_b / total
            }
        }
    }

    /// Whether enough transitions have been observed for valid estimates.
    pub fn is_valid(&self) -> bool {
        self.total_transitions >= self.min_samples
    }

    /// The measured multi-scale window loss-mass statistics (paper
    /// Section 8.4.1), for the r* burst-tail provisioning term. Returns
    /// the no-data default until `min_samples` nonzero blocks have been
    /// observed (the same validity threshold as the transition counts):
    /// clean channels keep the tail term inert.
    pub fn mass_stats(&self) -> raptorpath_math::MassStats {
        let mut out = raptorpath_math::MassStats::default();
        if self.mass_nz_total < self.min_samples {
            return out; // block_scale = 0.0 marks no-data
        }
        debug_assert_eq!(MASS_SCALES_LOCAL, raptorpath_math::MASS_SCALES);
        out.block_scale = MASS_BLOCK_SCALE as f64;
        for s in 0..MASS_SCALES_LOCAL {
            if self.mass_cnt[s] >= 1.0 && self.mass_nz[s] > 0.0 {
                out.nz[s] = (self.mass_nz[s] / self.mass_cnt[s]).clamp(0.0, 1.0);
                out.m1[s] = self.mass_s1[s] / self.mass_nz[s];
                out.m2[s] = self.mass_s2[s] / self.mass_nz[s];
            }
        }
        out
    }
}

impl Default for GilbertElliottEstimator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_received_stays_good() {
        let mut ge = GilbertElliottEstimator::new();
        for _ in 0..100 {
            ge.record_symbol(true);
        }
        assert_eq!(ge.state(), HmmState::Good);
        assert!(ge.p_gb() < 0.01, "p_gb should be ~0: {}", ge.p_gb());
    }

    #[test]
    fn test_burst_loss_enters_bad() {
        let mut ge = GilbertElliottEstimator::new();
        // 10 receives
        for _ in 0..10 {
            ge.record_symbol(true);
        }
        // 5 losses
        for _ in 0..5 {
            ge.record_symbol(false);
        }
        assert_eq!(ge.state(), HmmState::Bad);
    }

    #[test]
    fn test_burst_length_estimation() {
        let mut ge = GilbertElliottEstimator::new();
        // Alternating: 10 good, 5 bad — repeat several times
        for _ in 0..10 {
            for _ in 0..10 {
                ge.record_symbol(true);
            }
            for _ in 0..5 {
                ge.record_symbol(false);
            }
        }
        let mbl = ge.mean_burst_length();
        // Mean burst length should be approximately 5
        assert!(
            mbl > 3.0 && mbl < 8.0,
            "mean_burst_length should be ~5, got {mbl}"
        );
    }

    #[test]
    fn test_transition_probabilities() {
        let mut ge = GilbertElliottEstimator::new();
        // Feed a known pattern: 20 good, 10 bad, 20 good
        for _ in 0..20 {
            ge.record_symbol(true);
        }
        for _ in 0..10 {
            ge.record_symbol(false);
        }
        for _ in 0..20 {
            ge.record_symbol(true);
        }

        // p_gb should be small (1 transition out of ~39 Good→* transitions)
        assert!(ge.p_gb() > 0.0, "p_gb should be > 0");
        assert!(ge.p_gb() < 0.2, "p_gb should be small: {}", ge.p_gb());

        // p_bg should be small (1 transition out of ~10 Bad→* transitions)
        assert!(ge.p_bg() > 0.0, "p_bg should be > 0");
        assert!(ge.p_bg() < 0.3, "p_bg should be small: {}", ge.p_bg());
    }

    #[test]
    fn test_decay_forgets_old_data() {
        let mut ge = GilbertElliottEstimator::new();

        // Old pattern: lots of bursts (10 good, 10 bad)
        for _ in 0..5 {
            for _ in 0..10 {
                ge.record_symbol(true);
            }
            for _ in 0..10 {
                ge.record_symbol(false);
            }
        }
        let old_p_gb = ge.p_gb();

        // New pattern: all good (long run of no bursts)
        for _ in 0..2000 {
            ge.record_symbol(true);
        }
        let new_p_gb = ge.p_gb();

        assert!(
            new_p_gb < old_p_gb,
            "p_gb should decrease after long good run: old={old_p_gb} new={new_p_gb}"
        );
    }

    #[test]
    fn test_is_valid_threshold() {
        let mut ge = GilbertElliottEstimator::new();
        assert!(!ge.is_valid(), "should be invalid with no data");

        for _ in 0..29 {
            ge.record_symbol(true);
        }
        assert!(!ge.is_valid(), "should be invalid with < min_samples");

        ge.record_symbol(true);
        assert!(ge.is_valid(), "should be valid at min_samples");
    }
}
