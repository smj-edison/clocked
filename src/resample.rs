pub const FRAME_LOOKBACK: usize = 4;

pub(crate) fn hermite_interpolate(x0: f32, x1: f32, x2: f32, x3: f32, t: f32) -> f32 {
    let diff = x1 - x2;
    let c1 = x2 - x0;
    let c3 = x3 - x0 + 3.0 * diff;
    let c2 = -(2.0 * diff + c1 + c3);

    0.5 * ((c3 * t + c2) * t + c1) * t + x1
}

/// Resample between arbitrary input and output
///
/// # Arguments
///
/// * `resample_ratio` - input_frames / output_frames
/// * `get_sample_in` - a function that returns the next sample
pub(crate) fn resample<F>(
    resample_ratio: f64,
    mut get_sample_in: F,
    buffer_out: &mut [f32],
    last: &mut [f32; FRAME_LOOKBACK],
    t_bounded: &mut f64,
    scratch: &mut Vec<f32>,
) where
    F: FnMut() -> f32,
{
    let out_len = buffer_out.len();

    let needed_input_samples = (resample_ratio * out_len as f64) as usize;

    // make sure scratch is the right size
    scratch.resize(needed_input_samples, 0.0);

    for sample in scratch.iter_mut() {
        *sample = get_sample_in();
    }

    for i in 0..out_len {
        let incoming_position = i as f64 * resample_ratio + *t_bounded;
        let head = incoming_position as usize;

        if head >= scratch.len() {
            // read another frame, looks like we'll need it after all
            scratch.push(get_sample_in());
        }

        // TODO: optimize from here...
        let shift_by = (incoming_position + 1.0) as usize
            - (incoming_position - resample_ratio + 1.0) as usize;

        for _ in 0..shift_by {
            for i in 0..(FRAME_LOOKBACK - 1) {
                last[i] = last[i + 1];
            }

            last[FRAME_LOOKBACK - 1] = scratch[head];
        }

        buffer_out[i] = hermite_interpolate(
            last[0],
            last[1],
            last[2],
            last[3],
            incoming_position.fract() as f32,
        );
        // TODO: ...to here
    }

    for i in 0..FRAME_LOOKBACK {
        // we can't use `needed_samples` here, as occasionally we'll have to read an
        // additional sample due to non integer ratios between sample rates
        last[i] = scratch[scratch.len() - FRAME_LOOKBACK + i];
    }

    *t_bounded = ((needed_input_samples) as f64 * resample_ratio + *t_bounded).fract();
}
