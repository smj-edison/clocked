pub const FRAME_LOOKBACK: usize = 4;

pub(crate) fn hermite_interpolate(x0: f32, x1: f32, x2: f32, x3: f32, t: f32) -> f32 {
    let diff = x1 - x2;
    let c1 = x2 - x0;
    let c3 = x3 - x0 + 3.0 * diff;
    let c2 = -(2.0 * diff + c1 + c3);

    0.5 * ((c3 * t + c2) * t + c1) * t + x1
}

#[inline]
pub fn new_samples_needed(resample_ratio: f64, time: f64) -> usize {
    (time + resample_ratio) as usize
}

/// Resample between arbitrary input and output
///
/// # Arguments
///
/// * `resample_ratio` - input_sample_rate / output_sample_rate
/// * `new_samples_in` - an array with _new_ incoming samples (use [`new_samples_needed`]
///    to figure out how many new samples are needed)
/// * `last` - an array with the previous values
/// * `time` - ref to current time fraction (between 0.0 and 1.0 exclusive)
pub fn resample(
    resample_ratio: f64,
    new_samples_in: &[f32],
    last: &mut [f32; FRAME_LOOKBACK],
    time: &mut f64,
) -> f32 {
    let out = hermite_interpolate(last[0], last[1], last[2], last[3], *time as f32);

    *time += resample_ratio;

    let mut consumed = 0;
    while *time >= 1.0 {
        for i in 0..(FRAME_LOOKBACK - 1) {
            last[i] = last[i + 1];
        }

        last[FRAME_LOOKBACK - 1] = new_samples_in[consumed];

        *time -= 1.0;
        consumed += 1;
    }

    out
}
