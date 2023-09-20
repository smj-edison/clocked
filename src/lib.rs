pub mod engine;
pub mod estimator;
pub mod resample;
pub mod sink;

pub fn add(left: usize, right: usize) -> usize {
    left + right
}

pub(crate) fn lerp(start: f64, end: f64, amount: f64) -> f64 {
    (end - start) * amount + start
}

pub(crate) fn hermite_interpolate(x0: f32, x1: f32, x2: f32, x3: f32, t: f32) -> f32 {
    let diff = x1 - x2;
    let c1 = x2 - x0;
    let c3 = x3 - x0 + 3.0 * diff;
    let c2 = -(2.0 * diff + c1 + c3);

    0.5 * ((c3 * t + c2) * t + c1) * t + x1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
