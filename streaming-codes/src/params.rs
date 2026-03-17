/// Parameters for the streaming code.
#[derive(Debug, Clone, Copy)]
pub struct StreamingParams {
    /// Delay constraint: max positions behind newest for recovery
    pub t: u32,
    /// Burst length the code is designed to tolerate
    pub b: u32,
    /// Random (non-burst) loss rate
    pub epsilon: f64,
    /// Fraction of repair symbols allocated to burst layer (rest goes to random layer)
    pub burst_rate: f64,
    /// Fraction of repair symbols allocated to random layer
    pub random_rate: f64,
}

impl StreamingParams {
    /// Compute streaming parameters from channel estimates.
    ///
    /// `burst_length`: estimated mean burst length from GE model
    /// `loss_rate`: upper-bound loss rate (e.g., 95th percentile)
    /// `safety_factor`: over-provisioning multiplier (e.g., 1.15 for 15%)
    pub fn from_channel(burst_length: f64, loss_rate: f64, safety_factor: f64) -> Self {
        // B = ceil(burst_length * safety_factor), at least 1
        let b = ((burst_length * safety_factor).ceil() as u32).max(1);

        // T must satisfy T >= B for the burst layer to work.
        // For multipath: T ≈ max_rtt / symbol_interval, but we use T = B as baseline
        // and let the caller override if needed.
        let t = b;

        // Streaming capacity C = T/(T+B). Code rate = 1 - C overhead.
        // Burst layer rate: B/(T+B) of total repair
        // Random layer rate: ε/(1-ε) additional repair for random losses
        let epsilon = (loss_rate * safety_factor).min(0.5);

        // Burst layer: produces 1 repair per T source symbols (covers the diagonals)
        let burst_rate = 1.0 / t as f64;

        // Random layer: covers residual random loss not handled by burst layer
        let random_rate = if epsilon > 0.001 {
            epsilon / (1.0 - epsilon)
        } else {
            0.0
        };

        Self {
            t,
            b,
            epsilon,
            burst_rate,
            random_rate,
        }
    }

    /// Total repair rate (repair symbols per source symbol)
    pub fn total_rate(&self) -> f64 {
        self.burst_rate + self.random_rate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_streaming_params_from_channel() {
        let params = StreamingParams::from_channel(3.0, 0.05, 1.2);
        assert_eq!(params.b, 4); // ceil(3.0 * 1.2)
        assert_eq!(params.t, 4); // T = B
        assert!(params.burst_rate > 0.0);
        assert!(params.random_rate > 0.0);
        assert!(params.total_rate() < 1.0);
    }
}
