use std::time::{Duration, Instant};

use crate::{
    lerp,
    resample::{resample, FRAME_LOOKBACK},
};

#[derive(Debug)]
enum CompensationStrategy {
    None,
    Resample {
        start_diff: f64,
        detune: f64,
        /// lowest = oldest in sub-array
        fraction: f64,
    },
}

pub struct Sink {
    /// sample rate initialized with
    claimed_sample_rate: f64,

    /// `Instant` that the `Sink` started at
    start_time: Instant,
    frame_count: usize,

    incoming: Vec<rtrb::Consumer<f32>>,
    last_frames: Vec<[f32; FRAME_LOOKBACK]>,

    /// an estimate of where the device's buffer is at time-wise
    estimated_buffer_time: Duration,
    /// an estimate of how much ahead the device's buffer is, relative to what is
    /// currently playing
    estimated_buffer_ahead: Option<Duration>,

    strategy: CompensationStrategy,
    compensation_threshold: usize,

    scratch: Vec<f32>,
}

impl Sink {
    pub fn channels(&self) -> usize {
        self.incoming.len()
    }

    pub fn output_sample(&mut self, buffer_out: &mut [&mut [f32]]) {
        let now = Instant::now();
        let from_start = now - self.start_time;

        let out_len = buffer_out[0].len();

        // make sure we have enough incoming
        let enough = self.incoming.iter().all(|x| x.slots() >= out_len + 4);

        if !enough {
            // stupid producer isn't keeping up; buffer underrun. The show must go on though!
            for channel in buffer_out.iter_mut() {
                for frame in channel.iter_mut() {
                    *frame = 0.0;
                }
            }

            self.estimated_buffer_time +=
                Duration::from_secs_f64(out_len as f64 / self.claimed_sample_rate);
            self.frame_count += out_len;

            // TODO: figure out estimated_buffer_time after this?
            println!("underrun!");

            return;
        }

        // process a few samples before estimating sample rate
        if self.frame_count > self.claimed_sample_rate as usize / 4 {
            if let Some(estimated_buffer_ahead) = self.estimated_buffer_ahead {
                let device_time =
                    (self.estimated_buffer_time - estimated_buffer_ahead).as_secs_f64();
                let sink_time = from_start.as_secs_f64();

                let diff_secs = device_time - sink_time;

                let frames_ahead = diff_secs * self.claimed_sample_rate;
                let actual_sample_rate = (1.0 + diff_secs / sink_time) * self.claimed_sample_rate;

                let new_detune = self.claimed_sample_rate / actual_sample_rate;

                if frames_ahead.abs() as usize > self.compensation_threshold {
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
                            start_diff: diff_secs,
                            detune: new_detune,
                            fraction: 0.0,
                        };
                    } else if let CompensationStrategy::Resample { detune, .. } = &mut self.strategy
                    {
                        // lerp to help detune not to slide around too much
                        // TODO: see whether this will forever lag, or whether it will eventually
                        // even out
                        *detune = lerp(*detune, new_detune, 0.1);
                    }
                }
            } else {
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
                start_diff: _,
                detune,
                fraction,
            } => {
                for ((ring, channel_out), last_samples) in self
                    .incoming
                    .iter_mut()
                    .zip(buffer_out.iter_mut())
                    .zip(self.last_frames.iter_mut())
                {
                    resample(
                        *detune,
                        || ring.pop().unwrap(),
                        channel_out,
                        last_samples,
                        fraction,
                        &mut self.scratch,
                    );
                }
            }
        }

        self.estimated_buffer_time +=
            Duration::from_secs_f64(out_len as f64 / self.claimed_sample_rate);
        self.frame_count += out_len;
    }
}

#[cfg(test)]
mod tests {
    const TEST_BUFFER_SIZE: usize = 64;

    use hound::{SampleFormat, WavSpec, WavWriter};
    use std::{
        f64::consts::TAU,
        thread,
        time::{Duration, Instant},
    };

    use crate::{
        resample::FRAME_LOOKBACK,
        sink::{CompensationStrategy, Sink},
    };

    #[test]
    fn sample_rate_estimation() {
        let (mut producer, consumer) = rtrb::RingBuffer::new(TEST_BUFFER_SIZE * 8);

        let claimed_sample_rate = 48_000.0;
        let sample_rate = 48_100; // run slightly fast
        let buffer_size = TEST_BUFFER_SIZE;

        let mut sink = Sink {
            claimed_sample_rate,
            start_time: Instant::now(),
            frame_count: 0,
            incoming: vec![consumer],
            estimated_buffer_time: Duration::default(),
            estimated_buffer_ahead: None,
            strategy: CompensationStrategy::None,
            compensation_threshold: 20,
            scratch: vec![],
            last_frames: vec![[0.0; FRAME_LOOKBACK]; 1],
        };

        let time_started = Instant::now();

        // mimic initial filling of buffers
        let mut filling_buffer: [f32; TEST_BUFFER_SIZE * 2] = [0.0; TEST_BUFFER_SIZE * 2];
        let mut filling_buffer_ref = [filling_buffer.as_mut()];

        // output test audio
        let mut writer = WavWriter::create(
            "out-test.wav",
            WavSpec {
                channels: 1,
                sample_rate: sample_rate,
                bits_per_sample: 32,
                sample_format: SampleFormat::Float,
            },
        )
        .unwrap();

        let mut t_sin: f64 = 0.0;

        while !producer.is_full() {
            producer.push(t_sin.sin() as f32).unwrap();
            t_sin += (440.0 / claimed_sample_rate) * TAU;
        }

        sink.output_sample(&mut filling_buffer_ref);
        sink.output_sample(&mut filling_buffer_ref);

        for buffer_count in 0..700 {
            let buffer_time =
                Duration::from_secs_f64((buffer_count * buffer_size) as f64 / sample_rate as f64);

            let current_time = Instant::now() - time_started;

            while !producer.is_full() {
                producer.push(t_sin.sin() as f32).unwrap();
                t_sin += (440.0 / claimed_sample_rate) * TAU;
            }

            let mut buffer: [f32; TEST_BUFFER_SIZE] = [0.0; TEST_BUFFER_SIZE];
            let mut buffer_ref = [buffer.as_mut()];

            sink.output_sample(&mut buffer_ref);

            for sample in &buffer {
                writer.write_sample(*sample).unwrap();
            }

            if buffer_time > current_time {
                thread::sleep(buffer_time - current_time);
            }
        }

        println!("time elapsed: {:?}", Instant::now() - time_started);
    }
}
