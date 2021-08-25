use alloc::sync::Arc;
use core::{fmt, ops::Deref, str::FromStr};
use std::thread;

use abscissa_core::{application::fatal_error, Command, Options, Runnable};
use itertools::Itertools;
use tokio::runtime::Runtime as TokioRuntime;
use tracing::{error, info};

use ibc::{events::IbcEvent, ics24_host::identifier::ChainId};

use ibc_relayer::{
    config::ChainConfig,
    event::monitor::{EventMonitor, EventReceiver},
};

use crate::prelude::*;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum EventFilter {
    NewBlock,
    Tx,
}

impl EventFilter {
    pub fn matches(&self, event: &IbcEvent) -> bool {
        match self {
            EventFilter::NewBlock => matches!(event, IbcEvent::NewBlock(_)),
            EventFilter::Tx => {
                !(matches!(
                    event,
                    IbcEvent::NewBlock(_) | IbcEvent::Empty(_) | IbcEvent::ChainError(_)
                ))
            }
        }
    }
}

impl fmt::Display for EventFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NewBlock => write!(f, "NewBlock"),
            Self::Tx => write!(f, "Tx"),
        }
    }
}

impl FromStr for EventFilter {
    type Err = Box<dyn std::error::Error>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "NewBlock" => Ok(Self::NewBlock),
            "Tx" => Ok(Self::Tx),
            invalid => Err(format!("unrecognized event type: {}", invalid).into()),
        }
    }
}

#[derive(Command, Debug, Options)]
pub struct ListenCmd {
    /// Identifier of the chain to listen for events from
    #[options(free)]
    chain_id: ChainId,

    /// Add an event type to listen for, can be repeated. Listen for all events by default (available: Tx, NewBlock)
    #[options(short = "e", long = "event", meta = "EVENT")]
    events: Vec<EventFilter>,
}

impl ListenCmd {
    fn cmd(&self) -> Result<(), Box<dyn std::error::Error>> {
        let config = app_config();

        let chain_config = config
            .find_chain(&self.chain_id)
            .ok_or_else(|| format!("chain '{}' not found in configuration", self.chain_id))?;

        let events = if self.events.is_empty() {
            &[EventFilter::Tx, EventFilter::NewBlock]
        } else {
            self.events.as_slice()
        };

        listen(chain_config, events)
    }
}

impl Runnable for ListenCmd {
    fn run(&self) {
        self.cmd()
            .unwrap_or_else(|e| fatal_error(app_reader().deref(), &*e));
    }
}

/// Listen to events
pub fn listen(
    config: &ChainConfig,
    filters: &[EventFilter],
) -> Result<(), Box<dyn std::error::Error>> {
    info!(
        "listening for events `{}` on '{}'...",
        filters.iter().format(", "),
        config.id
    );

    let rt = Arc::new(TokioRuntime::new()?);
    let (event_monitor, rx) = subscribe(config, rt)?;

    thread::spawn(|| event_monitor.run());

    while let Ok(event_batch) = rx.recv() {
        match event_batch {
            Ok(batch) => {
                let matching_events = batch
                    .events
                    .into_iter()
                    .filter(|e| event_match(e, filters))
                    .collect_vec();

                if matching_events.is_empty() {
                    continue;
                }

                info!("- event batch at height {}", batch.height);

                for event in matching_events {
                    info!("+ {:#?}", event);
                }

                info!("");
            }
            Err(e) => error!("- error: {}", e),
        }
    }

    Ok(())
}

fn event_match(event: &IbcEvent, filters: &[EventFilter]) -> bool {
    filters.iter().any(|f| f.matches(event))
}

fn subscribe(
    chain_config: &ChainConfig,
    rt: Arc<TokioRuntime>,
) -> Result<(EventMonitor, EventReceiver), Box<dyn std::error::Error>> {
    let (mut event_monitor, rx, _) = EventMonitor::new(
        chain_config.id.clone(),
        chain_config.websocket_addr.clone(),
        rt,
    )
    .map_err(|e| format!("could not initialize event monitor: {}", e))?;

    event_monitor
        .subscribe()
        .map_err(|e| format!("could not initialize subscriptions: {}", e))?;

    Ok((event_monitor, rx))
}
