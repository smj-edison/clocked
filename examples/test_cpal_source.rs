use std::{
    io::{stdin, stdout, Write},
    thread,
    time::{Duration, Instant},
};

use clocked::cpal::start_cpal_source;
use cpal::{
    traits::{DeviceTrait, HostTrait},
    BufferSize,
};
use hound::{SampleFormat, WavSpec, WavWriter};

fn main() {
    let host = cpal::default_host();

    let mut input_devices: Vec<_> = host.input_devices().unwrap().collect();
    let input_device = match input_devices.len() {
        0 => panic!("no input port found"),
        1 => {
            println!(
                "Choosing the only available input port: {}",
                input_devices[0].name().unwrap_or("<no name>".into())
            );

            input_devices.remove(0)
        }
        _ => {
            println!("\nAvailable input ports:");
            for (i, device) in input_devices.iter().enumerate() {
                println!("{}: {}", i, device.name().unwrap_or("<no name>".into()));
            }

            print!("Please select input port: ");
            stdout().flush().unwrap();

            let mut input = String::new();
            stdin().read_line(&mut input).unwrap();

            let selected = input.trim().parse::<usize>().unwrap();

            input_devices.remove(selected)
        }
    };

    let supported_config = input_device.default_input_config().unwrap();
    let config = supported_config.config();

    let buffer_size = match config.buffer_size {
        BufferSize::Fixed(buffer_size) => Some(buffer_size as usize),
        BufferSize::Default => None,
    }
    .unwrap_or(1024);

    println!("buffer size: {}", buffer_size);
    println!("sample rate: {}", config.sample_rate.0);

    let mut sink = start_cpal_source(input_device, &config, supported_config.sample_format(), buffer_size, 2).unwrap();

    let start = Instant::now();
    let mut frames_processed = 0;

    // test requesting data faster than the soundcard is running at
    let actual_sample_rate = (config.sample_rate.0 + 2_000) as usize;

    let mut writer = WavWriter::create(
        "test_cpal_source.wav",
        WavSpec {
            channels: config.channels,
            sample_rate: actual_sample_rate as u32,
            bits_per_sample: 32,
            sample_format: SampleFormat::Float,
        },
    )
    .unwrap();

    loop {
        let mut missed = 0;

        'block: for i in 0..buffer_size {
            for _ in 0..sink.channels() {
                if let Ok(sample) = sink.interleaved_in.pop() {
                    writer.write_sample(sample).unwrap();
                } else {
                    print!("u ");
                    stdout().flush().unwrap();

                    missed = buffer_size - i - 1;

                    break 'block;
                }
            }
        }

        // simulate missed deadline
        for _ in 0..(missed * sink.channels()) {
            writer.write_sample(0.0).unwrap();
        }

        frames_processed += buffer_size;

        let buffer_time_secs = frames_processed as f64 / actual_sample_rate as f64;
        let now_secs = (Instant::now() - start).as_secs_f64();

        if buffer_time_secs > now_secs {
            thread::sleep(Duration::from_secs_f64(buffer_time_secs - now_secs));
        }
    }
}
