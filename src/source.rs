use std::{collections::VecDeque, time::Duration};

use crate::{
    resample::{new_samples_needed, resample, FRAME_LOOKBACK},
    CompensationStrategy,
};

pub struct StreamSource {
    /// sample rate initialized with
    claimed_sample_rate: f64,

    out: Vec<rtrb::Producer<f32>>,
    local_buffers: Vec<VecDeque<f32>>,
    last_frames: Vec<[f32; FRAME_LOOKBACK]>,

    strategy: CompensationStrategy,
    /// in frames
    compensation_start_threshold: f64,
}

impl StreamSource {
    pub fn new(sample_rate: f64, out: Vec<rtrb::Producer<f32>>, capacity: usize) -> StreamSource {
        StreamSource {
            claimed_sample_rate: sample_rate,
            out,
            local_buffers: vec![VecDeque::with_capacity(capacity)],
            last_frames: vec![[0.0; FRAME_LOOKBACK]],
            strategy: CompensationStrategy::None,
            compensation_start_threshold: 20.0,
        }
    }

    pub fn channels(&self) -> usize {
        self.out.len()
    }

    pub fn get_strategy(&self) -> &CompensationStrategy {
        &self.strategy
    }

    fn handle_new_inputs(&mut self, in_len: usize, callback: Duration, capture: Duration) {
        // process a few samples before estimating sample rate
        if callback > Duration::from_secs_f64(5.0) {
            let device_time = capture.as_secs_f64();
            let sink_time = callback.as_secs_f64();

            let diff_secs = device_time - sink_time;

            let frames_ahead = diff_secs * self.claimed_sample_rate;
            let actual_sample_rate = (1.0 + diff_secs / sink_time) * self.claimed_sample_rate;

            let new_ratio = self.claimed_sample_rate / actual_sample_rate;

            if frames_ahead.abs() > self.compensation_start_threshold {
                // we've drifted enough that we should start using a strategy

                if let CompensationStrategy::None = self.strategy {
                    // fill up `last` with previous values for hermite interpolation
                    for (channel_in, last) in self.local_buffers.iter_mut().zip(self.last_frames.iter_mut()) {
                        for last_sample in last.iter_mut().skip(1) {
                            *last_sample = channel_in.pop_front().unwrap();
                        }
                    }

                    self.strategy = CompensationStrategy::Resample {
                        resample_ratio: new_ratio,
                        time: 0.0,
                    };
                } else if let CompensationStrategy::Resample { resample_ratio, .. } = &mut self.strategy {
                    // lerp to help detune not to slide around too much
                    // TODO: see whether this will forever lag, or whether it will eventually
                    // even out
                    *resample_ratio = new_ratio;
                }
            }
        }

        match &mut self.strategy {
            CompensationStrategy::None => {
                for (channel_in, ring) in self.local_buffers.iter_mut().zip(self.out.iter_mut()) {
                    while let Some(sample_in) = channel_in.pop_front() {
                        ring.push(sample_in).unwrap();
                    }
                }
            }
            CompensationStrategy::Resample { resample_ratio, time } => {
                for ((channel_in, ring), last_samples) in self
                    .local_buffers
                    .iter_mut()
                    .zip(self.out.iter_mut())
                    .zip(self.last_frames.iter_mut())
                {
                    let mut scratch: [f32; 2] = [0.0; 2];

                    'inner: loop {
                        let new_sample_count = new_samples_needed(*resample_ratio, *time);

                        // do we have enough?
                        if channel_in.len() >= new_sample_count {
                            for i in 0..new_sample_count {
                                scratch[i] = channel_in.pop_front().unwrap();
                            }

                            let (out, new_time) = resample(
                                *resample_ratio,
                                scratch[0..new_sample_count].iter().copied(),
                                last_samples,
                                *time,
                            );

                            *time = new_time;

                            let _ = ring.push(out);
                        } else {
                            break 'inner;
                        }
                    }
                }
            }
        }
    }

    pub fn input_sample(&mut self, buffer_in: &[&[f32]], callback: Duration, capture: Duration) {
        let in_len = buffer_in[0].len();

        // copy incoming to a local buffer
        for (channel_in, local_buffer) in buffer_in.iter().zip(self.local_buffers.iter_mut()) {
            local_buffer.extend(channel_in.iter());
        }

        self.handle_new_inputs(in_len, callback, capture);
    }

    pub fn input_sample_interleaved(
        &mut self,
        iter: impl Iterator<Item = f32>,
        total_samples: usize,
        callback: Duration,
        capture: Duration,
    ) {
        let in_len = total_samples / self.channels();

        // copy incoming to a local buffer
        let mut channel_i = 0;

        for sample_in in iter {
            self.local_buffers[channel_i].push_back(sample_in);

            channel_i = (channel_i + 1) % self.channels();
        }

        self.handle_new_inputs(in_len, callback, capture);
    }
}

#[cfg(test)]
mod tests {
    const TEST_BUFFER_SIZE: usize = 256;
    const WRITE_TEST_AUDIO_TO_FILE: bool = false;

    use hound::{SampleFormat, WavSpec, WavWriter};
    use std::{f64::consts::TAU, time::Duration};

    use crate::{source::StreamSource, CompensationStrategy};

    #[test]
    fn sample_rate_estimation() {
        let (producer, mut consumer) = rtrb::RingBuffer::new(TEST_BUFFER_SIZE * 8);

        let claimed_sample_rate = 48_000.0; // run slightly slow
        let sample_rate = 49_000;
        let buffer_size = TEST_BUFFER_SIZE;

        let mut source = StreamSource::new(claimed_sample_rate, vec![producer], buffer_size * 8);

        // output test audio
        let mut writer = if WRITE_TEST_AUDIO_TO_FILE {
            Some(
                WavWriter::create(
                    "out-test.wav",
                    WavSpec {
                        channels: 1,
                        sample_rate: sample_rate as u32,
                        bits_per_sample: 32,
                        sample_format: SampleFormat::Float,
                    },
                )
                .unwrap(),
            )
        } else {
            None
        };

        let mut t_sin: f64 = 0.0;

        let mut consumed = 0;
        let mut produced = 0;

        for i in 0..15000 {
            let mut buffer: [f32; TEST_BUFFER_SIZE] = [0.0; TEST_BUFFER_SIZE];

            for sample in buffer.iter_mut() {
                *sample = t_sin.sin() as f32;
                t_sin += (440.0 / claimed_sample_rate) * TAU;
            }

            let buffer_ref = [buffer.as_ref()];

            source.input_sample(
                &buffer_ref,
                Duration::from_secs_f64((1.0 / claimed_sample_rate as f64) * buffer_size as f64) * i,
                Duration::from_secs_f64((1.0 / sample_rate as f64) * buffer_size as f64) * i,
            );

            consumed += buffer.len();

            while !consumer.is_empty() {
                if WRITE_TEST_AUDIO_TO_FILE {
                    writer.as_mut().unwrap().write_sample(consumer.pop().unwrap()).unwrap();
                } else {
                    consumer.pop().unwrap();
                }

                produced += 1;
            }
        }

        let ratio = consumed as f64 / produced as f64;
        let expected_ratio = claimed_sample_rate as f64 / sample_rate as f64;

        assert!((ratio - expected_ratio).abs() < 0.001);

        if let CompensationStrategy::Resample { resample_ratio, .. } = &source.strategy {
            assert!((resample_ratio - expected_ratio).abs() < 0.001);
        } else {
            unreachable!("Compensation strategy should have been used by now");
        }
    }
}
