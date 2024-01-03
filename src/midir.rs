use core::fmt;
use std::{
    io,
    sync::mpsc::{self},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use midir::{
    ConnectError, MidiInput, MidiInputConnection, MidiInputPort, MidiOutput, MidiOutputConnection, MidiOutputPort,
};

use crate::{
    midi::{self, parse_midi, MidiData},
    IntermittentSource, TimedValue,
};

pub struct MidirSource {
    pub receiver: mpsc::Receiver<TimedValue<MidiData>>,
}

impl fmt::Debug for MidirSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MidirSource { .. }")
    }
}

pub fn start_midir_source(
    device: MidiInput,
    port: &MidiInputPort,
    name: &str,
) -> Result<(MidiInputConnection<()>, MidirSource), ConnectError<MidiInput>> {
    let (sender, receiver) = mpsc::channel();

    let mut interm = IntermittentSource::new(sender, |buffer, time| {
        parse_midi(buffer).map(|parsed| TimedValue {
            since_start: time,
            value: parsed,
        })
    });

    let start = Instant::now();

    let instance = device.connect(
        port,
        name,
        move |stamp, message, _| {
            interm.input_messages(
                message.iter().copied(),
                Instant::now() - start,
                Duration::from_micros(stamp),
            );
        },
        (),
    )?;

    Ok((instance, MidirSource { receiver: receiver }))
}

#[derive(Debug)]
pub struct MidirSink {
    pub sender: mpsc::Sender<MidiData>,
}

struct MidiOutputConnectionWrapper(MidiOutputConnection);

impl io::Write for MidiOutputConnectionWrapper {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        match self.0.send(buffer) {
            Ok(()) => Ok(buffer.len()),
            Err(err) => match err {
                midir::SendError::InvalidData(msg) => Err(io::Error::new(io::ErrorKind::InvalidData, msg)),
                midir::SendError::Other(msg) => Err(io::Error::new(io::ErrorKind::Other, msg)),
            },
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub fn start_midir_sink(
    device: MidiOutput,
    port: &MidiOutputPort,
    name: &str,
) -> Result<(JoinHandle<()>, MidirSink), ConnectError<MidiOutput>> {
    let (sender, receiver) = mpsc::channel();

    let mut conn_out = MidiOutputConnectionWrapper(device.connect(port, name)?);

    Ok((
        thread::spawn(move || {
            while let Ok(message) = receiver.recv() {
                let _ = midi::write_midi_bytes(&message, &mut conn_out);
            }
        }),
        MidirSink { sender },
    ))
}
