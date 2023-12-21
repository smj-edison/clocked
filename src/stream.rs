use std::{collections::VecDeque, thread, time::Duration};

use nalgebra::DMatrix;

use crate::{
    lerp,
    resample::{new_samples_needed, resample, FRAME_LOOKBACK, ROLLING_AVG_LENGTH},
    CompensationStrategy, PidSettings,
};

/// A stream sink, to be called from an audio callback. Using half of a ring
/// buffer, it will automatically compensate for xruns by resampling in real-time
/// (currently implemented using a PID targeting half ring capacity).
pub struct StreamSink {
    /// Incoming samples
    ring_in: rtrb::Consumer<f32>,
    /// Channel count
    channels: usize,
    /// Total ring size
    ring_size: usize,

    /// Previous values (for resampling)
    last_frames: DMatrix<f32>,

    /// PID settings
    pid_settings: PidSettings,
    /// Values for calculating rolling average of available ring slots
    rolling_ring_avg: [usize; ROLLING_AVG_LENGTH],
    /// Integral part of PID
    ring_integral: f64,
    /// Last available slot average (for derivative part of PID)
    last_avg: f64,
    /// \# of xruns
    pub xruns: u64,

    /// \# of xruns before starting compensation
    compensation_start_threshold: u64,
    /// Compensation strategy
    strategy: CompensationStrategy,

    /// Scratch for use during resampling
    resample_scratch: DMatrix<f32>,

    debug_counter: u64,
}

impl StreamSink {
    /// Creates a stream sink.
    ///
    /// * `ring_in` - the `Consumer` half of a `rtrb` ring buffer (interleaved)
    /// * `channels` - the number of channels
    /// * `compensation_start_threshold` - the number of xruns
    /// * `pid_settings` - various PID settings
    pub fn new(
        ring_in: rtrb::Consumer<f32>,
        channels: usize,
        compensation_start_threshold: u64,
        pid_settings: PidSettings,
    ) -> StreamSink {
        let ring_size = ring_in.buffer().capacity();

        StreamSink {
            ring_in,
            ring_size,
            channels,
            last_frames: DMatrix::zeros(FRAME_LOOKBACK, channels),
            pid_settings,
            rolling_ring_avg: [0; ROLLING_AVG_LENGTH],
            ring_integral: 0.0,
            last_avg: 0.0,
            strategy: CompensationStrategy::None,
            compensation_start_threshold,
            resample_scratch: DMatrix::zeros(4, channels),
            xruns: 0,
            debug_counter: 0,
        }
    }

    /// Creates a stream sink with defaults (see [`StreamSink::new`]).
    ///
    /// * `ring_in` - the `Consumer` half of a `rtrb` ring buffer (interleaved)
    /// * `channels` - the number of channels
    pub fn with_defaults(ring_in: rtrb::Consumer<f32>, channels: usize) -> StreamSink {
        Self::new(ring_in, channels, 15, PidSettings::default())
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    /// See what strategy is currently being used.
    pub fn get_strategy(&self) -> &CompensationStrategy {
        &self.strategy
    }

    /// Ensures that interleaved data is never unaligned. This is useful in the case
    /// that the sink is reading data, but underruns halfway through a frame. We need
    /// to make sure that the ring buffer is left in an aligned state between calls.
    fn preserve_alignment(&mut self, channel_i: usize) {
        let align = (self.channels - channel_i) % self.channels;

        for _ in 0..align {
            while self.ring_in.pop().is_err() {
                thread::sleep(Duration::from_micros(50));
            }
        }
    }

    fn handle_xrun(&mut self, measure_xruns: bool) {
        // if it's during the startup phase, don't count xruns
        if measure_xruns {
            self.xruns += 1;
        }
    }

    fn clean_up(&mut self, channel_i: usize, measure_xruns: bool) {
        // make sure we don't get channels unaligned
        self.preserve_alignment(channel_i);
        self.handle_xrun(measure_xruns);
    }

    /// Meant to be called from an audio callback. This outputs the stream into whatever buffer the
    /// audio callback provides. If there are more xruns than `compensation_start_threshold`, it will
    /// start resampling by trying to keep the ring at half capacity (implemented with rolling average
    /// and PID).
    ///
    /// * `buffer_out` - audio callback buffer to be written into
    /// * `measure_xruns` - whether to measure xruns. Helpful for startup, as there may be some xruns
    ///    while things are all getting set up (which should not be counted for compensation check).
    pub fn output_samples(&mut self, buffer_out: &mut [f32], measure_xruns: bool) {
        debug_assert_eq!(buffer_out.len() % self.channels, 0);

        let frames_out_len = buffer_out.len() / self.channels;
        let ring_slots = self.ring_in.slots();

        if ring_slots == self.ring_size {
            self.handle_xrun(measure_xruns);
            // don't end function because of overrun
        }

        if self.xruns >= self.compensation_start_threshold {
            let avg = self.rolling_ring_avg.iter().map(|x| *x as f64).sum::<f64>()
                / self.rolling_ring_avg.len() as f64
                / self.ring_size as f64;

            // target is half of capacity
            // TODO: let target be more flexible
            let target = 0.5;
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
                // we've drifted enough that we should start using a strategy
                println!("sample rate compensation activated");

                // reset integral so it doesn't overshoot
                self.ring_integral = 0.0;

                self.strategy = CompensationStrategy::Resample {
                    resample_ratio: 1.0,
                    time: 0.0,
                };

                // fill up `last` with values for hermite interpolation
                'outer: for frame_i in 1..FRAME_LOOKBACK {
                    for channel_i in 0..self.channels {
                        if let Ok(sample_in) = self.ring_in.pop() {
                            self.last_frames[(frame_i, channel_i)] = sample_in;
                        } else {
                            self.clean_up(channel_i, measure_xruns);
                            break 'outer;
                        }
                    }
                }

                self.last_avg = avg;
            } else if let CompensationStrategy::Resample { resample_ratio, .. } = &mut self.strategy {
                // lerp to help detune not to slide around too much
                *resample_ratio = lerp(*resample_ratio, new_ratio, self.pid_settings.factor_last_interp);
            }
        }

        self.rolling_ring_avg.rotate_left(1);
        self.rolling_ring_avg[self.rolling_ring_avg.len() - 1] = ring_slots;

        match self.strategy {
            CompensationStrategy::None | CompensationStrategy::Never => {
                for (i, sample_out) in buffer_out.iter_mut().enumerate() {
                    if let Ok(sample) = self.ring_in.pop() {
                        *sample_out = sample;
                    } else {
                        self.clean_up(i % self.channels, measure_xruns);

                        break;
                    }
                }
            }
            CompensationStrategy::Resample {
                resample_ratio,
                mut time,
            } => {
                'outer: for frame_i in 0..frames_out_len {
                    let needed_new_samples = new_samples_needed(resample_ratio, time);
                    let mut next_time: f64 = 0.0;

                    for new_sample_i in 0..needed_new_samples {
                        for channel_i in 0..self.channels {
                            if let Ok(sample) = self.ring_in.pop() {
                                self.resample_scratch[(new_sample_i, channel_i)] = sample;
                            } else {
                                self.clean_up(channel_i, measure_xruns);

                                break 'outer;
                            }
                        }
                    }

                    for (channel_i, mut channel) in self.last_frames.column_iter_mut().enumerate() {
                        let (out, new_time) = resample(
                            resample_ratio,
                            self.resample_scratch.column(channel_i).iter().copied(),
                            &mut channel,
                            time,
                        );

                        next_time = new_time;

                        buffer_out[frame_i * self.channels + channel_i] = out;
                    }

                    time = next_time;
                }
            }
        }

        if self.debug_counter % 500 == 0 {
            println!("{:?}", buffer_out);
        }

        self.debug_counter += 1;
    }

    /// Forces compensation to start
    pub fn enable_compensation(&mut self) {
        self.xruns = self.compensation_start_threshold;
        self.strategy = CompensationStrategy::None;
    }

    /// Forces compensation to never happen
    pub fn disable_compensation(&mut self) {
        self.xruns = 0;
        self.strategy = CompensationStrategy::Never;
    }

    /// Resets mode to auto (default mode), as well as resetting xruns.
    pub fn reset_compensation(&mut self) {
        self.xruns = 0;
        self.strategy = CompensationStrategy::None;
    }
}

pub struct StreamSource {
    ring_out: rtrb::Producer<f32>,
    channels: usize,
    ring_size: usize,

    last_frames: DMatrix<f32>,
    local_buffer: VecDeque<f32>,

    /// PID settings
    pid_settings: PidSettings,
    /// Values for calculating rolling average of available ring slots
    rolling_ring_avg: [usize; ROLLING_AVG_LENGTH],
    /// Integral part of PID
    ring_integral: f64,
    /// Last available slot average (for derivative part of PID)
    last_avg: f64,
    /// \# of xruns
    pub xruns: usize,

    /// \# of xruns before starting compensation
    compensation_start_threshold: usize,
    /// Compensation strategy
    strategy: CompensationStrategy,

    /// Scratch for use during resampling
    resample_scratch: DMatrix<f32>,
}

impl StreamSource {
    /// Creates a stream source.
    ///
    /// * `ring_out` - the `Producer` half of a `rtrb` ring buffer (interleaved)
    /// * `channels` - the number of channels
    /// * `compensation_start_threshold` - the number of xruns
    /// * `startup_time` - how long to wait before measuring xruns
    /// * `pid_settings` - various PID settings
    pub fn new(
        ring_out: rtrb::Producer<f32>,
        channels: usize,
        compensation_start_threshold: usize,
        pid_settings: PidSettings,
    ) -> StreamSource {
        let ring_size = ring_out.buffer().capacity();

        StreamSource {
            ring_out,
            channels,
            ring_size,
            last_frames: DMatrix::zeros(FRAME_LOOKBACK, channels),
            local_buffer: VecDeque::with_capacity(ring_size),
            pid_settings,
            rolling_ring_avg: [0; ROLLING_AVG_LENGTH],
            ring_integral: 0.0,
            last_avg: 0.0,
            xruns: 0,
            compensation_start_threshold,
            strategy: CompensationStrategy::None,
            resample_scratch: DMatrix::zeros(4, channels),
        }
    }

    /// Creates a stream source with defaults (see [`StreamSource::new`]).
    ///
    /// * `ring_out` - the `Producer` half of a `rtrb` ring buffer (interleaved)
    /// * `channels` - the number of channels
    pub fn with_defaults(ring_out: rtrb::Producer<f32>, channels: usize) -> StreamSource {
        Self::new(ring_out, channels, 15, PidSettings::default())
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    /// See what strategy is currently being used.
    pub fn get_strategy(&self) -> &CompensationStrategy {
        &self.strategy
    }

    /// Ensures that interleaved data in the ring is never unaligned. This is useful in the case
    /// that the source is reading data, but overruns halfway through a frame. We need to make sure
    /// that the ring buffer is left in an aligned state between calls.
    fn preserve_alignment(&mut self, channel_i: usize) {
        let align = (self.channels - channel_i) % self.channels;

        for _ in 0..align {
            while self.ring_out.push(0.0).is_err() {
                thread::sleep(Duration::from_micros(50));
            }
        }
    }

    fn handle_xrun(&mut self, measure_xruns: bool) {
        // if it's during the startup phase, don't count xruns
        if measure_xruns {
            self.xruns += 1;
        }
    }

    fn clean_up(&mut self, channel_i: usize, measure_xruns: bool) {
        // make sure we don't get channels unaligned
        self.preserve_alignment(channel_i);
        self.handle_xrun(measure_xruns);

        // we're screwed regardless, but this should make sure local_buffer doesn't grow forever
        self.local_buffer.clear();
    }

    pub fn input_samples(&mut self, buffer_in: impl IntoIterator<Item = f32>, buffer_len: usize, measure_xruns: bool) {
        let ring_slots = self.ring_out.slots();

        if ring_slots < 10 {
            self.handle_xrun(measure_xruns);
        }

        assert_eq!(buffer_len % self.channels, 0);
        debug_assert_eq!(self.local_buffer.len() % self.channels, 0); // basic sanity check

        self.local_buffer.extend(buffer_in);

        if self.xruns > self.compensation_start_threshold {
            // target is half of capacity
            // TODO: let target be more flexible
            let target = 0.5;
            let avg = self.rolling_ring_avg.iter().map(|x| *x as f64).sum::<f64>()
                / self.rolling_ring_avg.len() as f64
                / self.ring_size as f64;
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
                // we've drifted enough that we should start using a strategy
                println!("sample rate compensation activated");

                // reset integral so it doesn't overshoot
                self.ring_integral = 0.0;

                self.strategy = CompensationStrategy::Resample {
                    resample_ratio: 1.0,
                    time: 0.0,
                };

                // fill up `last` with values for hermite interpolation
                for frame_i in 1..FRAME_LOOKBACK {
                    for channel_i in 0..self.channels {
                        self.last_frames[(frame_i, channel_i)] = self.local_buffer.pop_front().unwrap();
                    }
                }
            } else if let CompensationStrategy::Resample { resample_ratio, .. } = &mut self.strategy {
                // lerp to help detune not to slide around too much
                *resample_ratio = lerp(*resample_ratio, new_ratio, self.pid_settings.factor_last_interp);
            }
        }

        self.rolling_ring_avg.rotate_left(1);
        self.rolling_ring_avg[self.rolling_ring_avg.len() - 1] = ring_slots;

        match self.strategy {
            CompensationStrategy::None | CompensationStrategy::Never => {
                for (i, sample) in self.local_buffer.iter().enumerate() {
                    if self.ring_out.push(*sample).is_err() {
                        self.clean_up(i % self.channels, measure_xruns);

                        return;
                    }
                }

                self.local_buffer.clear();
            }
            CompensationStrategy::Resample {
                resample_ratio,
                mut time,
            } => {
                loop {
                    let new_sample_count = new_samples_needed(resample_ratio, time);

                    // do we have enough?
                    if self.local_buffer.len() >= new_sample_count * self.channels {
                        for channel_i in 0..self.channels {
                            for i in 0..new_sample_count {
                                self.resample_scratch[(i, channel_i)] =
                                    self.local_buffer[i * self.channels + channel_i];
                            }

                            let (out, new_time) = resample(
                                resample_ratio,
                                self.resample_scratch.column(channel_i).iter().copied(),
                                &mut self.last_frames.column_mut(channel_i),
                                time,
                            );

                            time = new_time;

                            if self.ring_out.push(out).is_err() {
                                self.clean_up(channel_i, measure_xruns);

                                return;
                            }
                        }

                        self.local_buffer.drain(0..(self.channels * new_sample_count));
                    } else {
                        return;
                    }
                }
            }
        }
    }

    /// Forces compensation to start
    pub fn enable_compensation(&mut self) {
        self.xruns = self.compensation_start_threshold;
        self.strategy = CompensationStrategy::None;
    }

    /// Forces compensation to never happen
    pub fn disable_compensation(&mut self) {
        self.xruns = 0;
        self.strategy = CompensationStrategy::Never;
    }

    /// Resets mode to auto (default mode)
    pub fn auto_compensation(&mut self) {
        self.xruns = 0;
        self.strategy = CompensationStrategy::None;
    }
}
