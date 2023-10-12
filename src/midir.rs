use std::{
    io,
    sync::mpsc::{Receiver, Sender},
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
    instance: MidiInputConnection<()>,
}

pub fn start_midir_source(
    device: MidiInput,
    port: &MidiInputPort,
    name: &str,
    stream: Sender<TimedValue<MidiData>>,
) -> Result<MidirSource, ConnectError<MidiInput>> {
    let mut interm = IntermittentSource::new(stream, |buffer, time| {
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

    Ok(MidirSource { instance })
}

pub struct MidirSink {
    handle: JoinHandle<()>,
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

pub fn start_midi_sink(
    device: MidiOutput,
    port: &MidiOutputPort,
    name: &str,
    stream: Receiver<MidiData>,
) -> Result<MidirSink, ConnectError<MidiOutput>> {
    let mut conn_out = MidiOutputConnectionWrapper(device.connect(port, name)?);

    Ok(MidirSink {
        handle: thread::spawn(move || {
            while let Ok(message) = stream.recv() {
                let _ = midi::data_to_bytes(&message, &mut conn_out);
            }
        }),
    })
}
