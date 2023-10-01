use std::{
    f64::consts::TAU,
    thread,
    time::{Duration, Instant},
};

use clocked::cpal::start_cpal_sink;
use cpal::{
    traits::{DeviceTrait, HostTrait},
    BufferSize, StreamConfig,
};
use hound::{SampleFormat, WavSpec, WavWriter};

fn main() {
    let host = cpal::default_host();
    let output_device = host.default_output_device().unwrap();

    let supported_config = output_device.default_output_config().unwrap();
    // let config: StreamConfig = supported_config.clone().into();
    let config = StreamConfig {
        channels: 2,
        sample_rate: cpal::SampleRate(48_000),
        buffer_size: BufferSize::Fixed(2048),
    };

    let buffer_size = match config.buffer_size {
        BufferSize::Fixed(buffer_size) => Some(buffer_size as usize),
        BufferSize::Default => None,
    }
    .unwrap();

    println!("buffer size: {}", buffer_size);
    println!("sample rate: {}", config.sample_rate.0);

    let mut t_sin: f64 = 0.0;
    let mut sink = start_cpal_sink(
        output_device,
        &config,
        supported_config.sample_format(),
        buffer_size * 2,
    )
    .unwrap();

    let start = Instant::now();
    let mut frames_processed = 0;

    let actual_sample_rate = 47_500;

    loop {
        'block: for _ in 0..64 {
            for _ in 0..sink.channels() {
                if let Err(_) = sink.data_out.push(t_sin.sin() as f32 * 0.02) {
                    println!("overrun");

                    break 'block;
                }
            }

            t_sin += (440.0 / config.sample_rate.0 as f64) * TAU;

            frames_processed += 1;
        }

        let buffer_time_secs = frames_processed as f64 / actual_sample_rate as f64;
        let now_secs = (Instant::now() - start).as_secs_f64();

        if buffer_time_secs > now_secs {
            thread::sleep(Duration::from_secs_f64(buffer_time_secs - now_secs));
        }
    }
}
