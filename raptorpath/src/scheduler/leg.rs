//! `LegState`: the per-leg estimators every law reads (W3.2, design
//! `docs/derivations/w3-per-leg-state.md` §2). `PathState` owns one.
//!
//! Phase A1 holds only the smoothed app-echo RTT `s`: it replaces both the
//! Copa `srtt` (R1) and the loss estimator's `ewma_rtt` (R2).

use super::*;

/// The smoothed app-echo RTT `s` (design §2, R1 = R2): EWMA 1/8 on one sample
/// stream, seeded from the first sample, in `Duration::mul_f64` form (Copa's
/// arithmetic, which the cadence and σ readings were captured against).
///
/// Deviation from the design's `s: Option<Duration>` sketch, forced by the
/// revival window (A0 `revived-window` row): today a revival clears Copa's
/// srtt but not the loss estimator's, so until the first post-revival sample
/// `srtt()`, `srtt_measured()` and the recovery clocks read the stale
/// pre-death value while Copa's cadence reads the 50 ms default. A bare
/// `Option` cleared on revival would move the first three. `reseed` carries
/// that: the value survives, and the next sample replaces it outright
/// instead of blending (the owner's decision: reseed on revival, as R1 does).
#[derive(Debug, Clone, Default)]
pub struct SmoothedRtt {
    s: Option<Duration>,
    reseed: bool,
}

impl SmoothedRtt {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fold one echo sample in.
    pub fn record(&mut self, rtt: Duration) {
        self.s = Some(match self.s {
            Some(s) if !self.reseed => s.mul_f64(0.875) + rtt.mul_f64(0.125),
            _ => rtt,
        });
        self.reseed = false;
    }

    /// The measurement, `None` until a sample has ever arrived.
    pub fn measured(&self) -> Option<Duration> {
        self.s
    }

    /// `measured()`, or the 50 ms `DEFAULT_SRTT` seed (what `srtt()` reads).
    pub fn or_default(&self) -> Duration {
        self.s.unwrap_or(DEFAULT_SRTT)
    }

    /// The controller's view: the default while the value awaits its reseed
    /// (Copa's cadence and σ reference never read a pre-death value).
    pub fn cadence(&self) -> Duration {
        if self.reseed { DEFAULT_SRTT } else { self.or_default() }
    }

    /// Revival: keep the stale value readable, reseed on the next sample.
    pub fn restart(&mut self) {
        self.reseed = true;
    }
}

/// The per-leg estimators (design §2). A1: `s` only; A2 moves the rest in.
#[derive(Debug, Default)]
pub struct LegState {
    pub(crate) s: SmoothedRtt,
}

impl LegState {
    /// Revival (`Scheduler::touch_path_at`): the estimators restart; `s`
    /// keeps its stale value readable and reseeds on the next sample.
    pub(crate) fn reset(&mut self) {
        self.s.restart();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoothed_rtt_seeds_blends_and_reseeds() {
        let ms = Duration::from_millis;
        let mut s = SmoothedRtt::new();
        assert_eq!((s.measured(), s.or_default(), s.cadence()), (None, DEFAULT_SRTT, DEFAULT_SRTT));
        s.record(ms(200));
        assert_eq!(s.measured(), Some(ms(200)), "the first sample IS the estimate");
        s.record(ms(100));
        assert_eq!(s.measured(), Some(ms(187) + Duration::from_micros(500)), "7/8·200 + 1/8·100");
        // Revival: the stale value stays readable, the controller's view is
        // the default, and the next sample replaces rather than blends.
        s.restart();
        assert_eq!(s.measured(), Some(ms(187) + Duration::from_micros(500)));
        assert_eq!(s.or_default(), ms(187) + Duration::from_micros(500));
        assert_eq!(s.cadence(), DEFAULT_SRTT);
        s.record(ms(40));
        assert_eq!((s.measured(), s.cadence()), (Some(ms(40)), ms(40)));
    }
}
