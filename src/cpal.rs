use std::time::{Duration, Instant};

use cpal::{traits::DeviceTrait, Device, SampleFormat, Stream, StreamConfig, SupportedStreamConfig};
use dasp_sample::Sample;
use rtrb::{Consumer, RingBuffer};
use snafu::{ResultExt, Snafu};

use crate::{StreamSink, StreamSource};

#[derive(Snafu, Debug)]
pub enum CpalError {
    #[snafu(display("Build stream error: {source}"))]
    BuildStreamError { source: cpal::BuildStreamError },
}

pub struct CpalSource {
    _stream: Stream,
    pub interleaved_in: Consumer<f32>,
}

pub fn start_cpal_source(
    device: Device,
    config: &SupportedStreamConfig,
    ring_buffer_size: usize,
) -> Result<CpalSource, CpalError> {
    let (producer, consumer) = RingBuffer::new(ring_buffer_size);

    let mut manager = StreamSource::with_defaults(producer, config.channels() as usize);
    let callback_start = Instant::now();

    let cfg: StreamConfig = config.clone().into();

    let stream = match config.sample_format() {
        cpal::SampleFormat::I8 => device
            .build_input_stream(
                &cfg,
                move |data, _: &_| input_callback::<i8>(data, &mut manager, callback_start),
                |_| {},
                None,
            )
            .context(BuildStreamSnafu)?,
        cpal::SampleFormat::I16 => device
            .build_input_stream(
                &cfg,
                move |data, _: &_| input_callback::<i16>(data, &mut manager, callback_start),
                |_| {},
                None,
            )
            .context(BuildStreamSnafu)?,
        cpal::SampleFormat::I32 => device
            .build_input_stream(
                &cfg,
                move |data, _: &_| input_callback::<i32>(data, &mut manager, callback_start),
                |_| {},
                None,
            )
            .context(BuildStreamSnafu)?,
        cpal::SampleFormat::I64 => device
            .build_input_stream(
                &cfg,
                move |data, _: &_| input_callback::<i64>(data, &mut manager, callback_start),
                |_| {},
                None,
            )
            .context(BuildStreamSnafu)?,
        cpal::SampleFormat::U8 => device
            .build_input_stream(
                &cfg,
                move |data, _: &_| input_callback::<u8>(data, &mut manager, callback_start),
                |_| {},
                None,
            )
            .context(BuildStreamSnafu)?,
        cpal::SampleFormat::U16 => device
            .build_input_stream(
                &cfg,
                move |data, _: &_| input_callback::<u16>(data, &mut manager, callback_start),
                |_| {},
                None,
            )
            .context(BuildStreamSnafu)?,
        cpal::SampleFormat::U32 => device
            .build_input_stream(
                &cfg,
                move |data, _: &_| input_callback::<u32>(data, &mut manager, callback_start),
                |_| {},
                None,
            )
            .context(BuildStreamSnafu)?,
        cpal::SampleFormat::U64 => device
            .build_input_stream(
                &cfg,
                move |data, _: &_| input_callback::<u64>(data, &mut manager, callback_start),
                |_| {},
                None,
            )
            .context(BuildStreamSnafu)?,
        cpal::SampleFormat::F32 => device
            .build_input_stream(
                &cfg,
                move |data, _: &_| input_callback::<f32>(data, &mut manager, callback_start),
                |_| {},
                None,
            )
            .context(BuildStreamSnafu)?,
        cpal::SampleFormat::F64 => device
            .build_input_stream(
                &cfg,
                move |data, _: &_| input_callback::<f64>(data, &mut manager, callback_start),
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
        _stream: stream,
        interleaved_in: consumer,
    })
}

fn input_callback<T>(input: &[T], manager: &mut StreamSource, callback_start: Instant)
where
    T: cpal::Sample + dasp_sample::ToSample<f32>,
{
    let callback = Instant::now() - callback_start;

    manager.input_samples(
        input.iter().map(|x| x.to_sample::<f32>()),
        input.len(),
        callback > Duration::from_secs(1),
    );
}

pub struct CpalSink {
    _stream: Stream,
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
    periods: usize,
) -> Result<CpalSink, CpalError> {
    let channels = config.channels;
    let ring_buffer_size = buffer_size * channels as usize * periods;

    let (producer, consumer) = RingBuffer::new(ring_buffer_size);

    let mut manager = StreamSink::with_defaults(consumer, channels as usize);
    // scratch to fill with `f32`s and then convert to whatever sample type CPAL is using
    let mut scratch = Vec::with_capacity(ring_buffer_size);

    let callback_start = Instant::now();

    let cfg: StreamConfig = config.clone().into();

    let stream = match sample_format {
        cpal::SampleFormat::I8 => device
            .build_output_stream(
                &cfg,
                move |data, _: &_| output_callback::<i8>(data, &mut manager, &mut scratch, callback_start.clone()),
                |_| {},
                None,
            )
            .context(BuildStreamSnafu)?,
        cpal::SampleFormat::I16 => device
            .build_output_stream(
                &cfg,
                move |data, _: &_| output_callback::<i16>(data, &mut manager, &mut scratch, callback_start.clone()),
                |_| {},
                None,
            )
            .context(BuildStreamSnafu)?,
        cpal::SampleFormat::I32 => device
            .build_output_stream(
                &cfg,
                move |data, _: &_| output_callback::<i32>(data, &mut manager, &mut scratch, callback_start.clone()),
                |_| {},
                None,
            )
            .context(BuildStreamSnafu)?,
        cpal::SampleFormat::I64 => device
            .build_output_stream(
                &cfg,
                move |data, _: &_| output_callback::<i64>(data, &mut manager, &mut scratch, callback_start.clone()),
                |_| {},
                None,
            )
            .context(BuildStreamSnafu)?,
        cpal::SampleFormat::U8 => device
            .build_output_stream(
                &cfg,
                move |data, _: &_| output_callback::<u8>(data, &mut manager, &mut scratch, callback_start.clone()),
                |_| {},
                None,
            )
            .context(BuildStreamSnafu)?,
        cpal::SampleFormat::U16 => device
            .build_output_stream(
                &cfg,
                move |data, _: &_| output_callback::<u16>(data, &mut manager, &mut scratch, callback_start.clone()),
                |_| {},
                None,
            )
            .context(BuildStreamSnafu)?,
        cpal::SampleFormat::U32 => device
            .build_output_stream(
                &cfg,
                move |data, _: &_| output_callback::<u32>(data, &mut manager, &mut scratch, callback_start.clone()),
                |_| {},
                None,
            )
            .context(BuildStreamSnafu)?,
        cpal::SampleFormat::U64 => device
            .build_output_stream(
                &cfg,
                move |data, _: &_| output_callback::<u64>(data, &mut manager, &mut scratch, callback_start.clone()),
                |_| {},
                None,
            )
            .context(BuildStreamSnafu)?,
        cpal::SampleFormat::F32 => device
            .build_output_stream(
                &cfg,
                move |data, _: &_| output_callback::<f32>(data, &mut manager, &mut scratch, callback_start.clone()),
                |_| {},
                None,
            )
            .context(BuildStreamSnafu)?,
        cpal::SampleFormat::F64 => device
            .build_output_stream(
                &cfg,
                move |data, _: &_| output_callback::<f64>(data, &mut manager, &mut scratch, callback_start.clone()),
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
        _stream: stream,
        data_out: producer,
        channels: channels as usize,
    })
}

fn output_callback<T>(output: &mut [T], manager: &mut StreamSink, scratch: &mut Vec<f32>, callback_start: Instant)
where
    T: cpal::Sample + dasp_sample::ToSample<T> + cpal::FromSample<f32>,
{
    let callback = Instant::now() - callback_start;

    scratch.resize(output.len(), 0.0);
    manager.output_samples(scratch, callback > Duration::from_secs(1));

    for (sample, sample_out) in scratch.iter().zip(output.iter_mut()) {
        *sample_out = sample.to_sample::<T>();
    }
}
