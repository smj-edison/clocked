use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use cpal::{
    traits::{DeviceTrait, StreamTrait},
    Device, SampleFormat, Stream, StreamConfig,
};
use dasp_sample::Sample;
use rtrb::{Consumer, RingBuffer};

use crate::{StreamSink, StreamSource};

#[derive(Debug)]
pub struct CpalSource {
    pub interleaved_in: Consumer<f32>,
    channels: usize,
}

impl CpalSource {
    pub fn channels(&self) -> usize {
        self.channels
    }
}

pub fn start_cpal_source(
    device: &Device,
    config: &StreamConfig,
    sample_format: SampleFormat,
    ring_size: usize,
) -> Result<(Stream, CpalSource), cpal::BuildStreamError> {
    let channels = config.channels as usize;
    let ring_buffer_size = ring_size * channels;

    let (producer, consumer) = RingBuffer::new(ring_buffer_size);

    let mut manager = StreamSource::with_defaults(producer, channels);
    let callback_start = Instant::now();

    let cfg: StreamConfig = config.clone();

    let stream = match sample_format {
        cpal::SampleFormat::I8 => device.build_input_stream(
            &cfg,
            move |data, _: &_| input_callback::<i8>(data, &mut manager, callback_start),
            |_| {},
            None,
        )?,
        cpal::SampleFormat::I16 => device.build_input_stream(
            &cfg,
            move |data, _: &_| input_callback::<i16>(data, &mut manager, callback_start),
            |_| {},
            None,
        )?,
        cpal::SampleFormat::I32 => device.build_input_stream(
            &cfg,
            move |data, _: &_| input_callback::<i32>(data, &mut manager, callback_start),
            |_| {},
            None,
        )?,
        cpal::SampleFormat::I64 => device.build_input_stream(
            &cfg,
            move |data, _: &_| input_callback::<i64>(data, &mut manager, callback_start),
            |_| {},
            None,
        )?,
        cpal::SampleFormat::U8 => device.build_input_stream(
            &cfg,
            move |data, _: &_| input_callback::<u8>(data, &mut manager, callback_start),
            |_| {},
            None,
        )?,
        cpal::SampleFormat::U16 => device.build_input_stream(
            &cfg,
            move |data, _: &_| input_callback::<u16>(data, &mut manager, callback_start),
            |_| {},
            None,
        )?,
        cpal::SampleFormat::U32 => device.build_input_stream(
            &cfg,
            move |data, _: &_| input_callback::<u32>(data, &mut manager, callback_start),
            |_| {},
            None,
        )?,
        cpal::SampleFormat::U64 => device.build_input_stream(
            &cfg,
            move |data, _: &_| input_callback::<u64>(data, &mut manager, callback_start),
            |_| {},
            None,
        )?,
        cpal::SampleFormat::F32 => device.build_input_stream(
            &cfg,
            move |data, _: &_| input_callback::<f32>(data, &mut manager, callback_start),
            |_| {},
            None,
        )?,
        cpal::SampleFormat::F64 => device.build_input_stream(
            &cfg,
            move |data, _: &_| input_callback::<f64>(data, &mut manager, callback_start),
            |_| {},
            None,
        )?,
        _ => {
            unreachable!("this program has crashed due to a `TooManyObfuscatingAbstractions` error")
        }
    };

    // finally
    Ok((
        stream,
        CpalSource {
            interleaved_in: consumer,
            channels,
        },
    ))
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

#[derive(Debug)]
pub struct CpalSink {
    pub interleaved_out: rtrb::Producer<f32>,
    pub measure_xruns: Arc<AtomicBool>,
    channels: usize,
}

impl CpalSink {
    pub fn channels(&self) -> usize {
        self.channels
    }
}

pub fn start_cpal_sink(
    device: &Device,
    config: &StreamConfig,
    sample_format: SampleFormat,
    ring_size: usize,
) -> Result<(Stream, CpalSink), cpal::BuildStreamError> {
    let channels = config.channels;
    let ring_buffer_size = ring_size * channels as usize;

    let (producer, consumer) = RingBuffer::new(ring_buffer_size);

    let mut manager = StreamSink::with_defaults(consumer, channels as usize);
    // scratch to fill with `f32`s and then convert to whatever sample type CPAL is using
    let mut scratch = Vec::with_capacity(ring_buffer_size);

    let cfg: StreamConfig = config.clone();

    let measure_xruns = Arc::new(AtomicBool::new(false));
    let measure_xruns_clone = measure_xruns.clone();

    let stream = match sample_format {
        cpal::SampleFormat::I8 => device.build_output_stream(
            &cfg,
            move |data, _: &_| output_callback::<i8>(data, &mut manager, &mut scratch, &measure_xruns),
            |_| {},
            None,
        )?,
        cpal::SampleFormat::I16 => device.build_output_stream(
            &cfg,
            move |data, _: &_| output_callback::<i16>(data, &mut manager, &mut scratch, &measure_xruns),
            |_| {},
            None,
        )?,
        cpal::SampleFormat::I32 => device.build_output_stream(
            &cfg,
            move |data, _: &_| output_callback::<i32>(data, &mut manager, &mut scratch, &measure_xruns),
            |_| {},
            None,
        )?,
        cpal::SampleFormat::I64 => device.build_output_stream(
            &cfg,
            move |data, _: &_| output_callback::<i64>(data, &mut manager, &mut scratch, &measure_xruns),
            |_| {},
            None,
        )?,
        cpal::SampleFormat::U8 => device.build_output_stream(
            &cfg,
            move |data, _: &_| output_callback::<u8>(data, &mut manager, &mut scratch, &measure_xruns),
            |_| {},
            None,
        )?,
        cpal::SampleFormat::U16 => device.build_output_stream(
            &cfg,
            move |data, _: &_| output_callback::<u16>(data, &mut manager, &mut scratch, &measure_xruns),
            |_| {},
            None,
        )?,
        cpal::SampleFormat::U32 => device.build_output_stream(
            &cfg,
            move |data, _: &_| output_callback::<u32>(data, &mut manager, &mut scratch, &measure_xruns),
            |_| {},
            None,
        )?,
        cpal::SampleFormat::U64 => device.build_output_stream(
            &cfg,
            move |data, _: &_| output_callback::<u64>(data, &mut manager, &mut scratch, &measure_xruns),
            |_| {},
            None,
        )?,
        cpal::SampleFormat::F32 => device.build_output_stream(
            &cfg,
            move |data, _: &_| output_callback::<f32>(data, &mut manager, &mut scratch, &measure_xruns),
            |_| {},
            None,
        )?,
        cpal::SampleFormat::F64 => device.build_output_stream(
            &cfg,
            move |data, _: &_| output_callback::<f64>(data, &mut manager, &mut scratch, &measure_xruns),
            |_| {},
            None,
        )?,
        _ => {
            unreachable!("this program has crashed due to a `TooManyObfuscatingAbstractions` error")
        }
    };
    stream.play().unwrap();

    // finally
    Ok((
        stream,
        CpalSink {
            interleaved_out: producer,
            channels: channels as usize,
            measure_xruns: measure_xruns_clone,
        },
    ))
}

fn output_callback<T>(output: &mut [T], manager: &mut StreamSink, scratch: &mut Vec<f32>, measure_xruns: &AtomicBool)
where
    T: cpal::Sample + dasp_sample::ToSample<T> + cpal::FromSample<f32>,
{
    scratch.resize(output.len(), 0.0);
    manager.output_samples(scratch, measure_xruns.load(Ordering::Relaxed));

    for (sample, sample_out) in scratch.iter().zip(output.iter_mut()) {
        *sample_out = sample.to_sample::<T>();
    }
}
