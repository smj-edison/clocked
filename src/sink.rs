use std::{thread, time::Duration};

use nalgebra::DMatrix;

use crate::{
    lerp,
    resample::{new_samples_needed, resample, FRAME_LOOKBACK, ROLLING_AVG_LENGTH},
    CompensationStrategy,
};

#[derive(Debug, Clone)]
pub struct PidSettings {
    pub prop_factor: f64,
    pub integ_factor: f64,
    pub deriv_factor: f64,

    /// to help prevent massive jerks in speed
    pub min_factor: f64,
    /// to help prevent massive jerks in speed
    pub max_factor: f64,
    pub factor_last_interp: f64,
}

impl Default for PidSettings {
    fn default() -> Self {
        PidSettings {
            prop_factor: 0.000001,
            integ_factor: 0.00000008,
            deriv_factor: 0.00001,
            min_factor: -0.2,
            max_factor: 0.2,
            factor_last_interp: 0.05,
        }
    }
}

pub struct StreamSink {
    /// incoming samples
    incoming: rtrb::Consumer<f32>,
    channels: usize,
    ring_size: usize,

    /// previous values (for resampling)
    last_frames: Vec<[f32; FRAME_LOOKBACK]>,

    pid_settings: PidSettings,
    rolling_ring_avg: [usize; ROLLING_AVG_LENGTH],
    ring_integral: f64,
    last_avg: f64,

    /// in frames
    compensation_start_threshold: f64,
    strategy: CompensationStrategy,
    startup_time: Duration,

    resample_scratch: DMatrix<f32>,
}

impl StreamSink {
    pub fn new(
        incoming: rtrb::Consumer<f32>,
        channels: usize,
        compensation_start_threshold: f64,
        startup_time: Duration,
        pid_settings: PidSettings,
    ) -> StreamSink {
        let ring_size = incoming.buffer().capacity();

        StreamSink {
            incoming: incoming,
            ring_size: ring_size,
            channels: channels,
            last_frames: vec![[0.0; FRAME_LOOKBACK]; channels],
            pid_settings,
            rolling_ring_avg: [0; ROLLING_AVG_LENGTH],
            ring_integral: 0.0,
            last_avg: 0.0,
            strategy: CompensationStrategy::None,
            compensation_start_threshold,
            startup_time,
            resample_scratch: DMatrix::zeros(4, channels),
        }
    }

    pub fn with_defaults(incoming: rtrb::Consumer<f32>, channels: usize) -> StreamSink {
        Self::new(
            incoming,
            channels,
            0.1,
            Duration::from_millis(250),
            PidSettings::default(),
        )
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    pub fn get_strategy(&self) -> &CompensationStrategy {
        &self.strategy
    }

    pub fn output_sample<'a>(&mut self, buffer_out: &mut [f32], callback: Duration) {
        let out_len = buffer_out.len() / self.channels;

        // process a few samples before estimating sample rate
        if callback > self.startup_time {
            // target is half of ring capacity
            let target = self.ring_size as f64 / 2.0;
            let avg = self.rolling_ring_avg.iter().map(|x| *x as f64).sum::<f64>() / self.rolling_ring_avg.len() as f64;
            let error = avg - target;

            self.ring_integral += error;

            // PID controls
            let proportional = error * self.pid_settings.prop_factor;
            let integrative = self.ring_integral * self.pid_settings.integ_factor;
            let derivative = (avg - self.last_avg) * self.pid_settings.deriv_factor;

            let new_factor = (proportional + integrative + derivative)
                .max(self.pid_settings.min_factor)
                .min(self.pid_settings.max_factor);
            let new_ratio = 2_f64.powf(new_factor);

            if let CompensationStrategy::None = self.strategy {
                if new_factor.abs() > self.compensation_start_threshold {
                    // we've drifted enough that we should start using a strategy
                    println!("activated");

                    // reset integral so it doesn't overshoot
                    self.ring_integral = 0.0;

                    self.strategy = CompensationStrategy::Resample {
                        resample_ratio: 1.0,
                        time: 0.0,
                    };

                    // fill up `last` with previous values for hermite interpolation
                    'outer: for frame_i in 1..FRAME_LOOKBACK {
                        for (channel_i, last_samples) in self.last_frames.iter_mut().enumerate() {
                            if let Ok(frame_in) = self.incoming.pop() {
                                last_samples[frame_i] = frame_in;
                            } else {
                                // make sure we don't get channels unaligned
                                preserve_alignment(self.channels, channel_i, &mut self.incoming);
                                break 'outer;
                            }
                        }
                    }
                }

                self.last_avg = avg;
            } else if let CompensationStrategy::Resample { resample_ratio, .. } = &mut self.strategy {
                // lerp to help detune not to slide around too much
                // TODO: see whether this will forever lag, or whether it will eventually
                // even out
                *resample_ratio = lerp(*resample_ratio, new_ratio, self.pid_settings.factor_last_interp);
            }
        }

        self.rolling_ring_avg.rotate_left(1);
        self.rolling_ring_avg[self.rolling_ring_avg.len() - 1] = self.incoming.slots();

        match &mut self.strategy {
            CompensationStrategy::None => {
                for (i, sample_out) in buffer_out.iter_mut().enumerate() {
                    if let Ok(sample) = self.incoming.pop() {
                        *sample_out = sample;
                    } else {
                        // make sure we don't get channels unaligned
                        preserve_alignment(self.channels, i % self.channels, &mut self.incoming);

                        break;
                    }
                }
            }
            CompensationStrategy::Resample { resample_ratio, time } => {
                'outer: for frame_i in 0..out_len {
                    let needed_new_samples = new_samples_needed(*resample_ratio, *time);
                    let mut next_time: f64 = 0.0;

                    for new_sample_i in 0..needed_new_samples {
                        for channel_i in 0..self.channels {
                            if let Ok(sample) = self.incoming.pop() {
                                self.resample_scratch[(new_sample_i, channel_i)] = sample;
                            } else {
                                // make sure we don't get channels unaligned
                                preserve_alignment(self.channels, channel_i, &mut self.incoming);

                                break 'outer;
                            }
                        }
                    }

                    for (channel_i, last_samples) in self.last_frames.iter_mut().enumerate() {
                        let (out, new_time) = resample(
                            *resample_ratio,
                            self.resample_scratch.column(channel_i).iter().copied(),
                            last_samples,
                            *time,
                        );

                        next_time = new_time;

                        buffer_out[frame_i * self.channels + channel_i] = out;
                    }

                    *time = next_time;
                }
            }
        }
    }
}

fn preserve_alignment(channels: usize, channel_i: usize, ring: &mut rtrb::Consumer<f32>) {
    let align = (channels - channel_i) % channels;

    for _ in 0..align {
        while let Err(_) = ring.pop() {
            thread::sleep(Duration::from_micros(50));
            println!("preserving alignment");
        }
    }
}

#[cfg(test)]
mod tests {
    const TEST_BUFFER_SIZE: usize = 256;
    const WRITE_TEST_AUDIO_TO_FILE: bool = false;

    use hound::{SampleFormat, WavSpec, WavWriter};
    use std::{
        f64::consts::TAU,
        time::{Duration, Instant},
    };

    use crate::sink::{CompensationStrategy, StreamSink};

    #[test]
    fn sample_rate_estimation() {
        let (mut producer, consumer) = rtrb::RingBuffer::new(TEST_BUFFER_SIZE * 2);

        let claimed_sample_rate = 48_000.0;
        let sample_rate = 49_000; // run slightly fast
        let buffer_size = TEST_BUFFER_SIZE;

        let mut sink = StreamSink::with_defaults(consumer, 1);

        let time_started = Instant::now();

        // mimic initial filling of buffers
        let mut filling_buffer: [f32; TEST_BUFFER_SIZE * 2] = [0.0; TEST_BUFFER_SIZE * 2];

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

        sink.output_sample(&mut filling_buffer, time_started - time_started);
        sink.output_sample(&mut filling_buffer, time_started - time_started);

        for i in 0..15000 {
            while !producer.is_full() {
                producer.push(t_sin.sin() as f32).unwrap();
                t_sin += (440.0 / claimed_sample_rate) * TAU;
                consumed += 1;
            }

            let mut buffer: [f32; TEST_BUFFER_SIZE] = [0.0; TEST_BUFFER_SIZE];

            sink.output_sample(
                &mut buffer,
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

        assert!((ratio - expected_ratio).abs() < 0.005);

        if let CompensationStrategy::Resample { resample_ratio, .. } = &sink.strategy {
            assert!((resample_ratio - expected_ratio).abs() < 0.001);
        } else {
            unreachable!("Compensation strategy should have been used by now");
        }
    }
}
