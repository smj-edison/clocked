mod intermittent;
pub mod midi;
pub mod resample;
mod stream;

#[cfg(feature = "client_impls")]
pub mod cpal;

use std::time::Duration;

pub use intermittent::{IntermittentSink, IntermittentSource, TimedValue};
pub use stream::{StreamSink, StreamSource};

pub fn lerp(start: f64, end: f64, amount: f64) -> f64 {
    (end - start) * amount + start
}

#[derive(Debug)]
pub enum CompensationStrategy {
    Never,
    None,
    Resample { resample_ratio: f64, time: f64 },
}

#[derive(Debug, Clone)]
pub struct PidSettings {
    /// Proportional strength
    pub prop_factor: f64,
    /// Integrative strength
    pub integ_factor: f64,
    /// Derivative strength
    pub deriv_factor: f64,

    /// to help prevent massive jerks in speed
    pub min_factor: f64,
    /// to help prevent massive jerks in speed
    pub max_factor: f64,
    /// how much of the new factor is applied (`lerp(last, new, factor_last_interp)`)
    pub factor_last_interp: f64,
}

impl Default for PidSettings {
    fn default() -> Self {
        PidSettings {
            prop_factor: 0.00001,
            integ_factor: 0.00000007,
            deriv_factor: 0.00001,
            min_factor: -0.2,
            max_factor: 0.2,
            factor_last_interp: 0.05,
        }
    }
}

pub(crate) enum DeltaDuration {
    Positive(Duration),
    Negative(Duration),
}

impl DeltaDuration {
    pub(crate) fn sub(first: Duration, second: Duration) -> DeltaDuration {
        if second > first {
            DeltaDuration::Negative(second - first)
        } else {
            DeltaDuration::Positive(first - second)
        }
    }

    pub(crate) fn add_to(&self, other: Duration) -> Duration {
        match self {
            DeltaDuration::Positive(duration) => other + *duration,
            DeltaDuration::Negative(duration) => other - *duration,
        }
    }
}
