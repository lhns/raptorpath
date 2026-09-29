//! Control plane: loss estimation and FEC rate computation.
//!
//! The rate is feedforward: a BOCD posterior quantile of the loss rate
//! feeds the closed-form r* law (paper §4, ADR-0050); there is no PI
//! feedback loop.

pub mod anchor;
pub mod estimator;
pub mod fec_rate;

// The BOCD and Gilbert-Elliott estimators live once, in the shared math
// crate (the wasm model runs the same code); re-exported here.
pub use raptorpath_math::{changepoint, gilbert_elliott};

pub use anchor::{SendRateAnchor, StallWitness};
pub use estimator::LossEstimator;
pub use fec_rate::{FecRateController, RateSnapshot, RepairRateCache, TaperBudget, TaperFunction};
