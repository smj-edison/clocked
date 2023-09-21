use std::{
    thread,
    time::{Duration, Instant},
};

use clocked::engine::start_engine;
use cpal::traits::{DeviceTrait, HostTrait};

fn main() {
    let start = Instant::now();

    let host = cpal::default_host();
    let device = host.default_input_device().expect("an input device");

    println!("Input device: {}", device.name().unwrap());

    let config = device.default_input_config().unwrap();
    println!("Default input config: {:?}", config);

    let start = Instant::now();
    let mut count = 0;

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config.into(),
            move |data: &[f32], _: &_| {
                if count < 10 {
                    println!(
                        "Since start: {:?}, incoming length: {}",
                        Instant::now() - start,
                        data.len()
                    );

                    count += 1;
                }
            },
            |err| panic!("error! {}", err),
            None,
        ),
        // ah yes, how could I forget how stupid CPAL is?
        _ => todo!(),
    };

    loop {
        thread::sleep(Duration::from_millis(100));
    }
}
