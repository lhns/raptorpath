//! Control plane: loss estimation, FEC rate computation, and feedback control.
//!
//! Architecture: feedforward (statistical) + feedback (PI correction)
//!
//! The feedforward component uses the binomial model to compute the exact
//! number of repair symbols needed for a target tail loss probability.
//! The feedback component (PI controller) compensates for model mismatch
//! (correlated losses, bursty channels, estimation lag).

pub mod anchor;
pub mod backend_selector;
pub mod changepoint;
pub mod estimator;
pub mod fec_rate;
pub mod gilbert_elliott;

pub use anchor::{SendRateAnchor, StallWitness};
pub use estimator::LossEstimator;
pub use fec_rate::{FecRateController, TaperBudget, TaperFunction};
