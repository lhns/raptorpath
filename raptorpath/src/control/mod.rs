//! Control plane: loss estimation, FEC rate computation, and feedback control.
//!
//! Architecture: feedforward (statistical) + feedback (PI correction)
//!
//! The feedforward component uses the binomial model to compute the exact
//! number of repair symbols needed for a target tail loss probability.
//! The feedback component (PI controller) compensates for model mismatch
//! (correlated losses, bursty channels, estimation lag).

pub mod anchor;
// `backend_selector` was DELETED (refactor: dead code batch 1). Mid-stream FEC
// backend switching was removed from the data path (paper §16.4; the live
// `warn!` in net::run that ignores an inbound WindowSwitch is its epitaph), so
// the threshold heuristic had no consumer and no verification.
pub mod changepoint;
pub mod estimator;
pub mod fec_rate;
pub mod gilbert_elliott;

pub use anchor::{DeliveryRateAnchor, SendRateAnchor, StallWitness};
pub use estimator::LossEstimator;
pub use fec_rate::{FecRateController, TaperBudget, TaperFunction};
