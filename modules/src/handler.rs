use crate::events::IbcEvent;
use crate::prelude::*;
use core::marker::PhantomData;

pub type HandlerResult<T, E> = Result<HandlerOutput<T>, E>;

#[derive(Clone, Debug)]
pub struct HandlerOutput<T> {
    pub result: T,
    pub log: Vec<String>,
    pub events: Vec<IbcEvent>,
}

impl<T> HandlerOutput<T> {
    pub fn builder() -> HandlerOutputBuilder<T> {
        HandlerOutputBuilder::new()
    }
}

#[derive(Clone, Debug, Default)]
pub struct HandlerOutputBuilder<T> {
    log: Vec<String>,
    events: Vec<IbcEvent>,
    marker: PhantomData<T>,
}

impl<T> HandlerOutputBuilder<T> {
    pub fn new() -> Self {
        Self {
            log: Vec::new(),
            events: Vec::new(),
            marker: PhantomData,
        }
    }

    pub fn with_log(mut self, log: impl Into<Vec<String>>) -> Self {
        self.log.append(&mut log.into());
        self
    }

    pub fn log(&mut self, log: impl Into<String>) {
        self.log.push(log.into());
    }

    pub fn with_events(mut self, mut events: Vec<IbcEvent>) -> Self {
        self.events.append(&mut events);
        self
    }

    pub fn emit(&mut self, event: IbcEvent) {
        self.events.push(event);
    }

    pub fn with_result(self, result: T) -> HandlerOutput<T> {
        HandlerOutput {
            result,
            log: self.log,
            events: self.events,
        }
    }
}
