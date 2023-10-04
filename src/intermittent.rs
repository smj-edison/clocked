use core::fmt;
use std::{collections::VecDeque, sync::mpsc, time::Duration};

use crate::DeltaDuration;

pub struct StreamMapper<In, Out> {
    pub values_in: VecDeque<In>,
    step: Box<dyn FnMut(&mut VecDeque<In>, Duration) -> Option<TimedValue<Out>> + Send>,
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

pub struct IntermittentSink<T> {
    channel_in: mpsc::Receiver<T>,
}

pub struct IntermittentSource<Input, Converted> {
    relative: Option<DeltaDuration>,
    channel_out: mpsc::Sender<TimedValue<Converted>>,
    mapper: StreamMapper<Input, Converted>,
}

impl<Input: fmt::Debug, Converted: fmt::Debug> IntermittentSource<Input, Converted> {
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
