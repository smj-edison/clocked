use std::{
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use smallvec::SmallVec;

struct MidiMessages {
    timestamp: u64,
    data: SmallVec<[u8; 8]>,
}

enum EngineMessage {
    NewAudioInput {
        receiver: rtrb::Consumer<f32>,
        id: u32,
    },
    NewMidiInput {
        receiver: mpsc::Receiver<MidiMessages>,
        id: u32,
    },
    NewAudioOutput {
        sender: rtrb::Producer<f32>,
        id: u32,
    },
    NewMidiOutput {
        sender: mpsc::Sender<MidiMessages>,
        id: u32,
    },
    DropAudioInput {
        id: u32,
    },
    DropMidiInput {
        id: u32,
    },
    DropAudioOutput {
        id: u32,
    },
    DropMidiOutput {
        id: u32,
    },
    Stop,
}

pub struct EngineManager {
    to_engine: mpsc::Sender<EngineMessage>,
    from_engine: mpsc::Receiver<EngineMessage>,
}

struct MidiInput {
    send_to: mpsc::Sender<MidiMessages>,
}

pub struct CallbackParams<'a> {
    audio_inputs: &'a [&'a [f32]],
    midi_inputs: &'a [MidiMessages],
    audio_outputs: &'a mut [&'a mut [f32]],
    midi_outputs: &'a mut [MidiMessages],
    buffer_time: Duration,
    system_time: Duration,
}

pub fn start_engine<F>(mut callback: F, sample_rate: usize, buffer_size: usize) -> EngineManager
where
    F: FnMut(CallbackParams) + Send + 'static,
{
    let (to_engine, from_main) = mpsc::channel();
    let (to_main, from_engine) = mpsc::channel();

    let time_started = Instant::now();
    let mut buffer_count = 0;

    let mut audio_input_streams: Vec<rtrb::Consumer<f32>> = vec![];
    let mut midi_input_streams: Vec<mpsc::Receiver<MidiMessages>> = vec![];
    let mut audio_output_streams: Vec<rtrb::Producer<f32>> = vec![];
    let mut midi_output_streams: Vec<mpsc::Sender<MidiMessages>> = vec![];

    let mut audio_inputs: Vec<Vec<Vec<f32>>> = vec![];
    let mut midi_inputs: Vec<Vec<MidiMessages>> = vec![];
    let mut audio_outputs: Vec<Vec<Vec<f32>>> = vec![];
    let mut midi_outputs: Vec<Vec<MidiMessages>> = vec![];

    thread::spawn(move || loop {
        let buffer_time =
            Duration::from_secs_f64((buffer_count * buffer_size) as f64 / sample_rate as f64);

        while let Ok(message) = from_main.try_recv() {
            match message {
                EngineMessage::NewAudioInput { receiver, id } => audio_input_streams.push(receiver),
                EngineMessage::NewMidiInput { receiver, id } => midi_input_streams.push(receiver),
                EngineMessage::NewAudioOutput { sender, id } => audio_output_streams.push(sender),
                EngineMessage::NewMidiOutput { sender, id } => midi_output_streams.push(sender),
                EngineMessage::DropAudioInput { id } => todo!(),
                EngineMessage::DropMidiInput { id } => todo!(),
                EngineMessage::DropAudioOutput { id } => todo!(),
                EngineMessage::DropMidiOutput { id } => todo!(),
                EngineMessage::Stop => return,
            }
        }

        // callback(CallbackParams {
        //     audio_inputs: &audio_inputs,
        //     midi_inputs: midi_inputs.as_slice(),
        //     audio_outputs: &mut audio_outputs,
        //     midi_outputs: &mut midi_outputs,
        // });

        let current_time = Instant::now() - time_started;

        if buffer_time > current_time {
            thread::sleep(buffer_time - current_time);
        }

        buffer_count += 1;
    });

    EngineManager {
        to_engine,
        from_engine,
    }
}
