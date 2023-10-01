use std::{
    iter::repeat_with,
    time::{Duration, Instant},
};

use cpal::{
    traits::DeviceTrait, Device, InputStreamTimestamp, OutputStreamTimestamp, SampleFormat, Stream, StreamConfig,
    StreamInstant, SupportedStreamConfig,
};
use dasp_sample::Sample;
use rtrb::{Consumer, RingBuffer};
use smallvec::SmallVec;
use snafu::{ResultExt, Snafu};

use crate::{sink::StreamSink, source::StreamSource};

#[derive(Snafu, Debug)]
pub enum CpalError {
    #[snafu(display("Build stream error: {source}"))]
    BuildStreamError { source: cpal::BuildStreamError },
}

pub struct CpalSource {
    stream: Stream,
    pub data_in: Vec<Consumer<f32>>,
}

pub fn start_cpal_source(
    device: Device,
    config: &SupportedStreamConfig,
    ring_buffer_size: usize,
) -> Result<CpalSource, CpalError> {
    let (producers, consumers): (Vec<_>, Vec<_>) = repeat_with(|| RingBuffer::new(ring_buffer_size))
        .take(config.channels() as usize)
        .unzip();

    let mut manager = StreamSource::new(config.sample_rate().0 as f64, producers, ring_buffer_size);
    let mut starts: Option<InputStreamTimestamp> = None;

    let why_are_there_two_config_types: StreamConfig = config.clone().into();

    // why is there not an abstraction for this?? O_O
    let stream = match config.sample_format() {
        cpal::SampleFormat::I8 => device
            .build_input_stream(
                &why_are_there_two_config_types,
                move |data, meta: &_| input_callback::<i8>(data, &mut manager, &meta.timestamp(), &mut starts),
                |_| {},
                None,
            )
            .context(BuildStreamSnafu)?,
        cpal::SampleFormat::I16 => device
            .build_input_stream(
                &why_are_there_two_config_types,
                move |data, meta: &_| input_callback::<i16>(data, &mut manager, &meta.timestamp(), &mut starts),
                |_| {},
                None,
            )
            .context(BuildStreamSnafu)?,
        cpal::SampleFormat::I32 => device
            .build_input_stream(
                &why_are_there_two_config_types,
                move |data, meta: &_| input_callback::<i32>(data, &mut manager, &meta.timestamp(), &mut starts),
                |_| {},
                None,
            )
            .context(BuildStreamSnafu)?,
        cpal::SampleFormat::I64 => device
            .build_input_stream(
                &why_are_there_two_config_types,
                move |data, meta: &_| input_callback::<i64>(data, &mut manager, &meta.timestamp(), &mut starts),
                |_| {},
                None,
            )
            .context(BuildStreamSnafu)?,
        cpal::SampleFormat::U8 => device
            .build_input_stream(
                &why_are_there_two_config_types,
                move |data, meta: &_| input_callback::<u8>(data, &mut manager, &meta.timestamp(), &mut starts),
                |_| {},
                None,
            )
            .context(BuildStreamSnafu)?,
        cpal::SampleFormat::U16 => device
            .build_input_stream(
                &why_are_there_two_config_types,
                move |data, meta: &_| input_callback::<u16>(data, &mut manager, &meta.timestamp(), &mut starts),
                |_| {},
                None,
            )
            .context(BuildStreamSnafu)?,
        cpal::SampleFormat::U32 => device
            .build_input_stream(
                &why_are_there_two_config_types,
                move |data, meta: &_| input_callback::<u32>(data, &mut manager, &meta.timestamp(), &mut starts),
                |_| {},
                None,
            )
            .context(BuildStreamSnafu)?,
        cpal::SampleFormat::U64 => device
            .build_input_stream(
                &why_are_there_two_config_types,
                move |data, meta: &_| input_callback::<u64>(data, &mut manager, &meta.timestamp(), &mut starts),
                |_| {},
                None,
            )
            .context(BuildStreamSnafu)?,
        cpal::SampleFormat::F32 => device
            .build_input_stream(
                &why_are_there_two_config_types,
                move |data, meta: &_| input_callback::<f32>(data, &mut manager, &meta.timestamp(), &mut starts),
                |_| {},
                None,
            )
            .context(BuildStreamSnafu)?,
        cpal::SampleFormat::F64 => device
            .build_input_stream(
                &why_are_there_two_config_types,
                move |data, meta: &_| input_callback::<f64>(data, &mut manager, &meta.timestamp(), &mut starts),
                |_| {},
                None,
            )
            .context(BuildStreamSnafu)?,
        _ => {
            unreachable!("this program has crashed due to a `TooManyObfuscatingAbstractions` error")
        }
    };

    // finally
    Ok(CpalSource {
        stream: stream,
        data_in: consumers,
    })
}

fn input_callback<T>(
    input: &[T],
    manager: &mut StreamSource,
    timestamps: &InputStreamTimestamp,
    starts: &mut Option<InputStreamTimestamp>,
) where
    T: cpal::Sample + dasp_sample::ToSample<f32>,
{
    let starts: InputStreamTimestamp = if let Some(starts) = starts {
        starts.clone()
    } else {
        *starts = Some(timestamps.clone());

        timestamps.clone()
    };

    let callback = timestamps
        .callback
        .duration_since(&starts.callback)
        .unwrap_or(Duration::ZERO);
    let capture = timestamps
        .capture
        .duration_since(&starts.capture)
        .unwrap_or(Duration::ZERO);

    manager.input_sample_interleaved(
        input.iter().map(|x| x.to_sample::<f32>()),
        input.len(),
        callback,
        capture,
    );
}

pub struct CpalSink {
    stream: Stream,
    pub data_out: rtrb::Producer<f32>,
    channels: usize,
}

impl CpalSink {
    pub fn channels(&self) -> usize {
        self.channels
    }
}

pub fn start_cpal_sink(
    device: Device,
    config: &StreamConfig,
    sample_format: SampleFormat,
    buffer_size: usize,
) -> Result<CpalSink, CpalError> {
    let channels = config.channels;
    let ring_buffer_size = buffer_size * channels as usize;

    let (producer, consumer) = RingBuffer::new(ring_buffer_size);

    let mut manager = StreamSink::new(47_500 as f64, consumer, channels as usize, 20.0, Duration::from_secs(1));
    let mut scratch = vec![];

    let callback_start = Instant::now();
    let mut playback_start: Option<StreamInstant> = None;
    let mut starts_count: usize = 0; // whyyyyyy

    let why_are_there_two_config_types: StreamConfig = config.clone().into();

    // why is there not an abstraction for this?? O_O
    let stream = match sample_format {
        cpal::SampleFormat::I8 => device
            .build_output_stream(
                &why_are_there_two_config_types,
                move |data, meta: &_| {
                    output_callback::<i8>(
                        data,
                        &mut manager,
                        &mut scratch,
                        meta.timestamp().playback,
                        callback_start.clone(),
                        &mut playback_start,
                        &mut starts_count,
                    )
                },
                |_| {},
                None,
            )
            .context(BuildStreamSnafu)?,
        cpal::SampleFormat::I16 => device
            .build_output_stream(
                &why_are_there_two_config_types,
                move |data, meta: &_| {
                    output_callback::<i16>(
                        data,
                        &mut manager,
                        &mut scratch,
                        meta.timestamp().playback,
                        callback_start.clone(),
                        &mut playback_start,
                        &mut starts_count,
                    )
                },
                |_| {},
                None,
            )
            .context(BuildStreamSnafu)?,
        cpal::SampleFormat::I32 => device
            .build_output_stream(
                &why_are_there_two_config_types,
                move |data, meta: &_| {
                    output_callback::<i32>(
                        data,
                        &mut manager,
                        &mut scratch,
                        meta.timestamp().playback,
                        callback_start.clone(),
                        &mut playback_start,
                        &mut starts_count,
                    )
                },
                |_| {},
                None,
            )
            .context(BuildStreamSnafu)?,
        cpal::SampleFormat::I64 => device
            .build_output_stream(
                &why_are_there_two_config_types,
                move |data, meta: &_| {
                    output_callback::<i64>(
                        data,
                        &mut manager,
                        &mut scratch,
                        meta.timestamp().playback,
                        callback_start.clone(),
                        &mut playback_start,
                        &mut starts_count,
                    )
                },
                |_| {},
                None,
            )
            .context(BuildStreamSnafu)?,
        cpal::SampleFormat::U8 => device
            .build_output_stream(
                &why_are_there_two_config_types,
                move |data, meta: &_| {
                    output_callback::<u8>(
                        data,
                        &mut manager,
                        &mut scratch,
                        meta.timestamp().playback,
                        callback_start.clone(),
                        &mut playback_start,
                        &mut starts_count,
                    )
                },
                |_| {},
                None,
            )
            .context(BuildStreamSnafu)?,
        cpal::SampleFormat::U16 => device
            .build_output_stream(
                &why_are_there_two_config_types,
                move |data, meta: &_| {
                    output_callback::<u8>(
                        data,
                        &mut manager,
                        &mut scratch,
                        meta.timestamp().playback,
                        callback_start.clone(),
                        &mut playback_start,
                        &mut starts_count,
                    )
                },
                |_| {},
                None,
            )
            .context(BuildStreamSnafu)?,
        cpal::SampleFormat::U32 => device
            .build_output_stream(
                &why_are_there_two_config_types,
                move |data, meta: &_| {
                    output_callback::<u32>(
                        data,
                        &mut manager,
                        &mut scratch,
                        meta.timestamp().playback,
                        callback_start.clone(),
                        &mut playback_start,
                        &mut starts_count,
                    )
                },
                |_| {},
                None,
            )
            .context(BuildStreamSnafu)?,
        cpal::SampleFormat::U64 => device
            .build_output_stream(
                &why_are_there_two_config_types,
                move |data, meta: &_| {
                    output_callback::<u64>(
                        data,
                        &mut manager,
                        &mut scratch,
                        meta.timestamp().playback,
                        callback_start.clone(),
                        &mut playback_start,
                        &mut starts_count,
                    )
                },
                |_| {},
                None,
            )
            .context(BuildStreamSnafu)?,
        cpal::SampleFormat::F32 => device
            .build_output_stream(
                &why_are_there_two_config_types,
                move |data, meta: &_| {
                    output_callback::<f32>(
                        data,
                        &mut manager,
                        &mut scratch,
                        meta.timestamp().playback,
                        callback_start.clone(),
                        &mut playback_start,
                        &mut starts_count,
                    )
                },
                |_| {},
                None,
            )
            .context(BuildStreamSnafu)?,
        cpal::SampleFormat::F64 => device
            .build_output_stream(
                &why_are_there_two_config_types,
                move |data, meta: &_| {
                    output_callback::<f64>(
                        data,
                        &mut manager,
                        &mut scratch,
                        meta.timestamp().playback,
                        callback_start.clone(),
                        &mut playback_start,
                        &mut starts_count,
                    )
                },
                |_| {},
                None,
            )
            .context(BuildStreamSnafu)?,
        _ => {
            unreachable!("this program has crashed due to a `TooManyObfuscatingAbstractions` error")
        }
    };

    // finally
    Ok(CpalSink {
        stream: stream,
        data_out: producer,
        channels: channels as usize,
    })
}

fn output_callback<T>(
    output: &mut [T],
    manager: &mut StreamSink,
    scratch: &mut Vec<f32>,

    playback_now: StreamInstant,

    callback_start: Instant,
    playback_start: &mut Option<StreamInstant>,
    starts_count: &mut usize,
) where
    T: cpal::Sample + dasp_sample::ToSample<T> + cpal::FromSample<f32>,
{
    // did I mention cpal is silly?
    if *starts_count < 10 {
        *playback_start = Some(playback_now);
        *starts_count += 1;
    }

    let playback_start = playback_start.unwrap();
    let buffer_len = output.len() / manager.channels();

    let callback = Instant::now() - callback_start;
    let playback = playback_now.duration_since(&playback_start).unwrap_or(Duration::ZERO);

    scratch.resize(output.len(), 0.0);

    manager.output_sample(scratch, callback, playback);

    for (sample, sample_out) in scratch.iter().zip(output.iter_mut()) {
        *sample_out = sample.to_sample::<T>();
    }
}
