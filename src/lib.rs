pub mod cpal;
pub mod engine;
pub mod resample;
pub mod sink;
pub mod source;

pub fn add(left: usize, right: usize) -> usize {
    left + right
}

#[derive(Debug)]
pub enum CompensationStrategy {
    None,
    Resample { resample_ratio: f64, time: f64 },
}
