use std::{collections::VecDeque, sync::mpsc, time::Duration};

use crate::DeltaDuration;

type MapFunc<In, Out> = Box<dyn FnMut(&mut VecDeque<In>, Duration) -> Option<TimedValue<Out>> + Send>;

pub struct StreamMapper<In, Out> {
    pub values_in: VecDeque<In>,
    step: MapFunc<In, Out>,
}

impl<In, Out> StreamMapper<In, Out> {
    pub fn new<F>(step: F) -> StreamMapper<In, Out>
    where
        F: FnMut(&mut VecDeque<In>, Duration) -> Option<TimedValue<Out>> + Send + 'static,
    {
        StreamMapper {
            values_in: VecDeque::new(),
            step: Box::new(step),
        }
    }

    pub fn step(&mut self, since_start: Duration) -> Option<TimedValue<Out>> {
        if !self.values_in.is_empty() {
            (self.step)(&mut self.values_in, since_start)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub struct TimedValue<T> {
    pub since_start: Duration,
    pub value: T,
}

pub struct IntermittentSink<Output> {
    channel_in: mpsc::Receiver<Output>,
    send: Box<dyn FnMut(Output)>,
}

impl<Output> IntermittentSink<Output> {
    pub fn new<F>(channel_in: mpsc::Receiver<Output>, send: F) -> Self
    where
        F: FnMut(Output) + 'static,
    {
        IntermittentSink {
            channel_in,
            send: Box::new(send),
        }
    }

    /// this function blocks; probably best to run in a thread
    pub fn start(&mut self) {
        while let Ok(value) = self.channel_in.recv() {
            (self.send)(value);
        }
    }
}

pub struct IntermittentSource<Input, Converted> {
    relative: Option<DeltaDuration>,
    channel_out: mpsc::Sender<TimedValue<Converted>>,
    mapper: StreamMapper<Input, Converted>,
}

impl<Input, Converted> IntermittentSource<Input, Converted> {
    pub fn new<F>(out: mpsc::Sender<TimedValue<Converted>>, convert: F) -> Self
    where
        F: FnMut(&mut VecDeque<Input>, Duration) -> Option<TimedValue<Converted>> + 'static + Send,
    {
        IntermittentSource {
            relative: None,
            channel_out: out,
            mapper: StreamMapper::new(convert),
        }
    }

    pub fn input_messages(
        &mut self,
        messages_in: impl IntoIterator<Item = Input>,
        since_start: Duration,
        timestamp: Duration,
    ) {
        let processed_timestamp = if let Some(relative) = &self.relative {
            relative.add_to(timestamp)
        } else {
            self.relative = Some(DeltaDuration::sub(timestamp, since_start));

            since_start
        };

        self.mapper.values_in.extend(messages_in);

        while let Some(value) = self.mapper.step(processed_timestamp) {
            if self.channel_out.send(value).is_err() {
                return; // looks like the channel hung up
            }
        }
    }
}
