use std::{
    thread,
    time::{Duration, Instant},
};

use clocked::engine::start_engine;

fn main() {
    let start = Instant::now();

    let engine_handler = start_engine(
        move |_| {
            let time_diff = Instant::now() - start;

            println!("Time since start: {time_diff:?}");
        },
        48_000,
        24_000,
    );

    loop {
        thread::sleep(Duration::from_millis(100));
    }
}
