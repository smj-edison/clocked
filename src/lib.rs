pub mod cpal;
pub mod engine;
pub mod resample;
pub mod sink;
pub mod source;

pub fn lerp(start: f64, end: f64, amount: f64) -> f64 {
    (end - start) * amount + start
}

#[derive(Debug)]
pub enum CompensationStrategy {
    None,
    Resample { resample_ratio: f64, time: f64 },
}
