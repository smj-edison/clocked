use std::{
    io::{stdin, stdout, Write},
    thread,
    time::Duration,
};

use clocked::{midi::MidiData, midir::start_midir_sink};
use midir::{MidiOutput, MidiOutputPort};

fn main() {
    match run() {
        Ok(_) => (),
        Err(err) => println!("Error: {}", err),
    }
}

// adapted from: https://github.com/Boddlnagg/midir/blob/master/examples/test_play.rs
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let midi_out = MidiOutput::new("My Test Output")?;

    // Get an output port (read from console if multiple are available)
    let out_ports = midi_out.ports();
    let out_port: &MidiOutputPort = match out_ports.len() {
        0 => return Err("no output port found".into()),
        1 => {
            println!(
                "Choosing the only available output port: {}",
                midi_out.port_name(&out_ports[0]).unwrap()
            );
            &out_ports[0]
        }
        _ => {
            println!("\nAvailable output ports:");
            for (i, p) in out_ports.iter().enumerate() {
                println!("{}: {}", i, midi_out.port_name(p).unwrap());
            }
            print!("Please select output port: ");
            stdout().flush()?;
            let mut input = String::new();
            stdin().read_line(&mut input)?;
            out_ports
                .get(input.trim().parse::<usize>()?)
                .ok_or("invalid output port selected")?
        }
    };

    println!("\nOpening connection");
    let (_handle, conn_out) = start_midir_sink(midi_out, out_port, "clocked-out-test")?;
    println!("Connection open. Listen!");

    let play_note = |note: u8, duration: u64| {
        // We're ignoring errors in here
        let _ = conn_out.sender.send(MidiData::NoteOn {
            channel: 0,
            note: note,
            velocity: 0x64,
        });

        thread::sleep(Duration::from_millis(duration * 150));
        let _ = conn_out.sender.send(MidiData::NoteOff {
            channel: 0,
            note: note,
            velocity: 0x64,
        });
    };

    thread::sleep(Duration::from_millis(4 * 150));

    play_note(66, 4);
    play_note(65, 3);
    play_note(63, 1);
    play_note(61, 6);
    play_note(59, 2);
    play_note(58, 4);
    play_note(56, 4);
    play_note(54, 4);

    thread::sleep(Duration::from_millis(150));
    println!("\nClosing connection");

    Ok(())
}
