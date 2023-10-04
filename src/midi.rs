use std::{collections::VecDeque, time::Duration};

/// low and high are nibbles
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Timecode {
    FrameLow(u8),
    FrameHigh(u8),
    SecondsLow(u8),
    SecondsHigh(u8),
    MinutesLow(u8),
    MinuteHigh(u8),
    HourLow(u8),
    HourHigh(u8),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SysCommon {
    SystemExclusive { message: Vec<u8> },
    QuarterFrame { time_fragment: Timecode },
    SongPositionPointer { position: u16 },
    SongSelect { song: u8 },
    TuneRequest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SysRt {
    MidiClock,
    Tick,
    Start,
    Continue,
    Stop,
    ActiveSensing,
    Reset,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MidiData {
    NoteOff { channel: u8, note: u8, velocity: u8 },
    NoteOn { channel: u8, note: u8, velocity: u8 },
    Aftertouch { channel: u8, note: u8, pressure: u8 },
    ControlChange { channel: u8, controller: u8, value: u8 },
    ProgramChange { channel: u8, patch: u8 },
    ChannelAftertouch { channel: u8, pressure: u8 },
    PitchBend { channel: u8, pitch_bend: u16 },
    SysCommon(SysCommon),
    SysRt(SysRt),
    SysEx { id_and_data: Vec<u8> },
    Reset,
    MidiNone,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MidiMessage {
    pub data: MidiData,
    pub timestamp: Duration,
}

/// returns `None` if there isn't enough data to tell what length is needed
fn prep_message(buffer: &mut VecDeque<u8>) -> Option<usize> {
    while !buffer.is_empty() && buffer[0] & 0x80 == 0 {
        // shift through the buffer until we hit a viable message
        buffer.pop_front();
    }

    if let Some(first_byte) = buffer.get(0).copied() {
        if first_byte >= 0x80 && first_byte <= 0xEF {
            // Voice messages
            let message = first_byte >> 4;

            match message {
                0x8 => Some(3), // note on
                0x9 => Some(3), // note off
                0xA => Some(3), // aftertouch
                0xB => Some(3), // control change
                0xC => Some(2), // program change
                0xD => Some(2), // channel pressure
                0xE => Some(3), // pitch bend
                _ => unreachable!("already checked message bounds"),
            }
        } else if first_byte >> 4 == 0xF {
            match first_byte & 0x0F {
                0x0 => {
                    for (i, value) in buffer.iter().enumerate() {
                        if *value == 0xF7 {
                            return Some(i + 1);
                        } else if *value & 0x80 != 0 {
                            // if we had a normal message come up, we better
                            // drop all of the (failed) sysex message
                            buffer.drain(0..i);

                            return prep_message(buffer);
                        }
                    }

                    None
                }
                0x1 => Some(2), // quarter frame
                0x2 => Some(3), // song position
                0x3 => Some(2), // song select
                0x4 => Some(1), // reserved?
                0x5 => Some(1), // reserved?
                0x6 => Some(1), // tune request
                0x7 => Some(1), // sysex end message (will be ignored)
                0x8 => Some(1), // midi clock
                0x9 => Some(1), // midi tick
                0xA => Some(1), // midi start
                0xB => Some(1), // midi continue
                0xC => Some(1), // midi stop
                0xD => Some(1), // reserved?
                0xE => Some(1), // active sensing
                0xF => Some(1), // system reset
                _ => unreachable!("only matching & 0x0F"),
            }
        } else {
            unreachable!("no message header. Should have been established by beginning while loop");
        }
    } else {
        None
    }
}

// so I don't have to type so much
fn n(buffer: &mut VecDeque<u8>) -> u8 {
    buffer.pop_front().unwrap()
}

pub fn parse_midi(buffer: &mut VecDeque<u8>) -> Option<MidiData> {
    let needed = prep_message(buffer);

    let enough_in_buffer = if let Some(needed) = needed {
        buffer.len() >= needed
    } else {
        false
    };

    if enough_in_buffer {
        let first_byte = n(buffer);

        if first_byte >= 0x80 && first_byte <= 0xEF {
            // Voice messages
            let message = first_byte >> 4;
            let channel = first_byte & 0x0F;

            match message {
                // note off
                0x8 => Some(MidiData::NoteOff {
                    channel,
                    note: n(buffer) & 0x7F,
                    velocity: n(buffer) & 0x7F,
                }),
                // note on
                0x9 => Some(MidiData::NoteOn {
                    channel,
                    note: n(buffer) & 0x7F,
                    velocity: n(buffer) & 0x7F,
                }),
                0xA => Some(MidiData::Aftertouch {
                    channel,
                    note: n(buffer) & 0x7F,
                    pressure: n(buffer) & 0x7F,
                }), // aftertouch
                0xB => Some(MidiData::ControlChange {
                    channel,
                    controller: n(buffer) & 0x7F,
                    value: n(buffer) & 0x7F,
                }), // control change
                0xC => Some(MidiData::ProgramChange {
                    channel,
                    patch: n(buffer) & 0x7F,
                }), // program change
                0xD => Some(MidiData::ChannelAftertouch {
                    channel,
                    pressure: n(buffer) & 0x7F,
                }), // channel pressure
                0xE => Some(MidiData::PitchBend {
                    channel,
                    pitch_bend: (n(buffer) as u16 & 0x7F) | ((n(buffer) as u16 & 0x7F) << 7),
                }), // pitch bend
                _ => unreachable!("already checked message bounds"),
            }
        } else if first_byte >> 4 == 0xF {
            match first_byte & 0x0F {
                0x0 => {
                    // sysex
                    let mut data = Vec::new();

                    for _ in 0..needed.unwrap() {
                        if let Some(next_data) = buffer.pop_front() {
                            if next_data & 0x80 != 0 {
                                // gotta do this in the case there isn't a sysex end message
                                break;
                            }

                            data.push(next_data);
                        } else {
                            break;
                        }
                    }

                    Some(MidiData::SysEx { id_and_data: data })
                }
                0x1 => {
                    // quarter frame
                    let data_byte = n(buffer) & 0x7F;
                    let value_type = data_byte >> 4;
                    let value = data_byte & 0x0F;

                    Some(MidiData::SysCommon(SysCommon::QuarterFrame {
                        time_fragment: match value_type {
                            0 => Timecode::FrameLow(value),
                            1 => Timecode::FrameHigh(value),
                            2 => Timecode::SecondsLow(value),
                            3 => Timecode::SecondsHigh(value),
                            4 => Timecode::MinutesLow(value),
                            5 => Timecode::MinuteHigh(value),
                            6 => Timecode::HourLow(value),
                            7 => Timecode::HourHigh(value),
                            _ => unreachable!("value_type cannot be more than 7"),
                        },
                    }))
                }
                // song position
                0x2 => Some(MidiData::SysCommon(SysCommon::SongPositionPointer {
                    position: (n(buffer) as u16 & 0x7F) | ((n(buffer) as u16 & 0x7F) << 7),
                })),
                // song select
                0x3 => Some(MidiData::SysCommon(SysCommon::SongSelect { song: n(buffer) })),
                // reserved?
                0x4 | 0x5 | 0xD => {
                    n(buffer);
                    None
                }
                // tune request
                0x6 => Some(MidiData::SysCommon(SysCommon::TuneRequest)),
                // sysex end message (will be ignored)
                0x7 => {
                    n(buffer);
                    None
                }
                // midi clock
                0x8 => Some(MidiData::SysRt(SysRt::MidiClock)),
                // midi tick
                0x9 => Some(MidiData::SysRt(SysRt::Tick)),
                // midi start
                0xA => Some(MidiData::SysRt(SysRt::Start)),
                // midi continue
                0xB => Some(MidiData::SysRt(SysRt::Continue)),
                // midi stop
                0xC => Some(MidiData::SysRt(SysRt::Stop)),
                // active sensing
                0xE => Some(MidiData::SysRt(SysRt::ActiveSensing)),
                // system reset
                0xF => Some(MidiData::Reset),
                _ => unreachable!("only matching & 0x0F"),
            }
        } else {
            unreachable!("no message header. Should have been established by beginning while loop");
        }
    } else {
        None
    }
}
