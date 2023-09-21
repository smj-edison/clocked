use std::time::Duration;

use crate::{
    resample::{new_samples_needed, resample, FRAME_LOOKBACK},
    CompensationStrategy,
};

pub struct StreamSink {
    /// sample rate initialized with
    claimed_sample_rate: f64,

    /// incoming samples
    incoming: Vec<rtrb::Consumer<f32>>,
    /// previous values (for resampling)
    last_frames: Vec<[f32; FRAME_LOOKBACK]>,

    /// an estimate of where the device's buffer is at time-wise
    estimated_buffer_time: Duration,
    /// an estimate of how much ahead the device's buffer is, relative to what is
    /// currently playing
    estimated_buffer_ahead: Option<Duration>,

    strategy: CompensationStrategy,
    /// in frames
    compensation_start_threshold: f64,
}

impl StreamSink {
    pub fn channels(&self) -> usize {
        self.incoming.len()
    }

    pub fn get_strategy(&self) -> &CompensationStrategy {
        &self.strategy
    }

    pub fn output_sample(&mut self, buffer_out: &mut [&mut [f32]], from_start: Duration) {
        let out_len = buffer_out[0].len();

        // make sure we have enough incoming
        let enough = match self.strategy {
            CompensationStrategy::None => self.incoming.iter().all(|x| x.slots() >= out_len + 4),
            CompensationStrategy::Resample { resample_ratio, .. } => self
                .incoming
                .iter()
                .all(|x| x.slots() >= (out_len as f64 * resample_ratio) as usize + 4),
        };

        if !enough {
            // stupid producer isn't keeping up; buffer underrun. The show must go on though!
            for channel in buffer_out.iter_mut() {
                for frame in channel.iter_mut() {
                    *frame = 0.0;
                }
            }

            self.estimated_buffer_time +=
                Duration::from_secs_f64(out_len as f64 / self.claimed_sample_rate);

            return;
        }

        // process a few samples before estimating sample rate
        if from_start > Duration::from_secs_f64(0.25) {
            if let Some(estimated_buffer_ahead) = self.estimated_buffer_ahead {
                let device_time =
                    (self.estimated_buffer_time - estimated_buffer_ahead).as_secs_f64();
                let sink_time = from_start.as_secs_f64();

                let diff_secs = device_time - sink_time;

                let frames_ahead = diff_secs * self.claimed_sample_rate;
                let actual_sample_rate = (1.0 + diff_secs / sink_time) * self.claimed_sample_rate;

                let new_ratio = self.claimed_sample_rate / actual_sample_rate;

                if frames_ahead.abs() > self.compensation_start_threshold {
                    if let CompensationStrategy::None = self.strategy {
                        // we've drifted enough that we should start using a strategy

                        // fill up `last` with previous values for hermite interpolation
                        for (ring, last) in
                            self.incoming.iter_mut().zip(self.last_frames.iter_mut())
                        {
                            for last_sample in last.iter_mut().skip(1) {
                                *last_sample = ring.pop().unwrap();
                            }
                        }

                        self.strategy = CompensationStrategy::Resample {
                            resample_ratio: new_ratio,
                            time: 0.0,
                        };
                    } else if let CompensationStrategy::Resample { resample_ratio, .. } =
                        &mut self.strategy
                    {
                        // lerp to help detune not to slide around too much
                        // TODO: see whether this will forever lag, or whether it will eventually
                        // even out
                        *resample_ratio = new_ratio;
                    }
                }
            } else {
                // the buffer is probably established by now, let's set the estimated buffer time
                self.estimated_buffer_ahead = Some(self.estimated_buffer_time - from_start);
            }
        }

        match &mut self.strategy {
            CompensationStrategy::None => {
                for (ring, channel) in self.incoming.iter_mut().zip(buffer_out.iter_mut()) {
                    for sample_out in channel.iter_mut() {
                        *sample_out = ring.pop().unwrap();
                    }
                }
            }
            CompensationStrategy::Resample {
                resample_ratio,
                time,
            } => {
                for ((ring, channel_out), last_samples) in self
                    .incoming
                    .iter_mut()
                    .zip(buffer_out.iter_mut())
                    .zip(self.last_frames.iter_mut())
                {
                    let mut scratch: [f32; 2] = [0.0; 2];

                    for sample_out in channel_out.iter_mut() {
                        let new_sample_count = new_samples_needed(*resample_ratio, *time);

                        for i in 0..new_sample_count {
                            scratch[i] = ring.pop().unwrap();
                        }

                        // I'm sure there's a faster way to do this, but my head really hurts and this
                        // isn't causing distortion
                        let out = resample(
                            *resample_ratio,
                            &scratch[0..new_sample_count],
                            last_samples,
                            time,
                        );

                        *sample_out = out;
                    }
                }
            }
        }

        self.estimated_buffer_time +=
            Duration::from_secs_f64(out_len as f64 / self.claimed_sample_rate);
    }
}

#[cfg(test)]
mod tests {
    const TEST_BUFFER_SIZE: usize = 256;
    const WRITE_TEST_AUDIO_TO_FILE: bool = true;

    use hound::{SampleFormat, WavSpec, WavWriter};
    use std::{
        f64::consts::TAU,
        time::{Duration, Instant},
    };

    use crate::{
        resample::FRAME_LOOKBACK,
        sink::{CompensationStrategy, StreamSink},
    };

    #[test]
    fn sample_rate_estimation() {
        let (mut producer, consumer) = rtrb::RingBuffer::new(TEST_BUFFER_SIZE * 8);

        let claimed_sample_rate = 48_000.0;
        let sample_rate = 49_000; // run slightly fast
        let buffer_size = TEST_BUFFER_SIZE;

        let mut sink = StreamSink {
            claimed_sample_rate,
            incoming: vec![consumer],
            estimated_buffer_time: Duration::default(),
            estimated_buffer_ahead: None,
            strategy: CompensationStrategy::None,
            compensation_start_threshold: 20.0,
            last_frames: vec![[0.0; FRAME_LOOKBACK]; 1],
        };

        let time_started = Instant::now();

        // mimic initial filling of buffers
        let mut filling_buffer: [f32; TEST_BUFFER_SIZE * 2] = [0.0; TEST_BUFFER_SIZE * 2];
        let mut filling_buffer_ref = [filling_buffer.as_mut()];

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

        while !producer.is_full() {
            producer.push(t_sin.sin() as f32).unwrap();
            t_sin += (440.0 / claimed_sample_rate) * TAU;
        }

        sink.output_sample(&mut filling_buffer_ref, time_started - time_started);
        sink.output_sample(&mut filling_buffer_ref, time_started - time_started);

        for i in 0..15000 {
            while !producer.is_full() {
                producer.push(t_sin.sin() as f32).unwrap();
                t_sin += (440.0 / claimed_sample_rate) * TAU;
                consumed += 1;
            }

            let mut buffer: [f32; TEST_BUFFER_SIZE] = [0.0; TEST_BUFFER_SIZE];
            let mut buffer_ref = [buffer.as_mut()];

            sink.output_sample(
                &mut buffer_ref,
                Duration::from_secs_f64((1.0 / sample_rate as f64) * buffer_size as f64) * i,
            );

            produced += buffer.len();

            if WRITE_TEST_AUDIO_TO_FILE {
                for sample in &buffer {
                    writer.as_mut().unwrap().write_sample(*sample).unwrap();
                }
            }
        }

        let ratio = consumed as f64 / produced as f64;
        let expected_ratio = claimed_sample_rate as f64 / sample_rate as f64;

        assert!((ratio - expected_ratio).abs() < 0.001);

        if let CompensationStrategy::Resample { resample_ratio, .. } = &sink.strategy {
            assert!((resample_ratio - expected_ratio).abs() < 0.001);
        } else {
            unreachable!("Compensation strategy should have been used by now");
        }
        // println!("ratio: {}, expected ratio: {}", ratio, expected_ratio);
    }
}
