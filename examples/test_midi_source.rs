use std::{
    error::Error,
    io::{stdin, stdout, Write},
    sync::mpsc,
    time::{Duration, Instant},
};

use clocked::{midi::parse_midi, IntermittentSource, TimedValue};
use midir::{Ignore, MidiInput};

// mostly copied from midir's examples
fn main() {
    match run() {
        Ok(_) => (),
        Err(err) => println!("Error: {}", err),
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut midi_in = MidiInput::new("midir reading input")?;
    midi_in.ignore(Ignore::None);

    // Get an input port (read from console if multiple are available)
    let in_ports = midi_in.ports();
    let in_port = match in_ports.len() {
        0 => return Err("no input port found".into()),
        1 => {
            println!(
                "Choosing the only available input port: {}",
                midi_in.port_name(&in_ports[0]).unwrap()
            );
            &in_ports[0]
        }
        _ => {
            println!("\nAvailable input ports:");
            for (i, p) in in_ports.iter().enumerate() {
                println!("{}: {}", i, midi_in.port_name(p).unwrap());
            }

            print!("Please select input port: ");
            stdout().flush()?;

            let mut input = String::new();
            stdin().read_line(&mut input)?;

            in_ports
                .get(input.trim().parse::<usize>()?)
                .ok_or("invalid input port selected")?
        }
    };

    println!("\nOpening connection...");
    let in_port_name = midi_in.port_name(in_port)?;

    let (sender, receiver) = mpsc::channel();
    let mut interm = IntermittentSource::new(sender, |buffer, time| {
        parse_midi(buffer).map(|parsed| TimedValue {
            since_start: time,
            value: parsed,
        })
    });

    let start = Instant::now();

    // _conn_in needs to be a named parameter, because it needs to be kept alive until the end of the scope
    let _conn_in = midi_in.connect(
        in_port,
        "midir-read-input",
        move |stamp, message, _| {
            interm.input_messages(
                message.iter().copied(),
                Instant::now() - start,
                Duration::from_micros(stamp),
            );
        },
        (),
    )?;

    println!("Connection open, reading input from '{}'.", in_port_name);

    while let Ok(message) = receiver.recv() {
        println!("Parsed message: {:?}", message);
    }

    println!("Closing connection");
    Ok(())
}
