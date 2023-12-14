use std::{
    error::Error,
    io::{stdin, stdout, Write},
    sync::mpsc,
};

use clocked::midir::start_midir_source;
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

    let conn_in = start_midir_source(midi_in, in_port, "clocked-read-input")?;

    println!("Connection open, reading input from '{}'.", in_port_name);

    while let Ok(message) = conn_in.receiver.recv() {
        println!("Parsed message: {:?}", message);
    }

    println!("Closing connection");
    Ok(())
}
