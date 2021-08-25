use core::fmt;

use crossbeam_channel::Sender;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info};

use crate::{
    chain::handle::{ChainHandle, ChainHandlePair},
    config::Config,
    object::Object,
    telemetry::Telemetry,
};

pub mod retry_strategy;

mod error;
pub use error::WorkerError;

mod handle;
pub use handle::WorkerHandle;

mod cmd;
pub use cmd::WorkerCmd;

mod map;
pub use map::WorkerMap;

mod client;
pub use client::ClientWorker;

mod connection;
pub use connection::ConnectionWorker;

mod channel;
pub use channel::ChannelWorker;

mod packet;
pub use packet::PacketWorker;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkerId(u64);

impl WorkerId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

impl fmt::Display for WorkerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkerMsg {
    Stopped(WorkerId, Object),
}

/// A worker processes batches of events associated with a given [`Object`].
pub enum Worker<ChainA: ChainHandle, ChainB: ChainHandle> {
    Client(WorkerId, ClientWorker<ChainA, ChainB>),
    Connection(WorkerId, ConnectionWorker<ChainA, ChainB>),
    Channel(WorkerId, ChannelWorker<ChainA, ChainB>),
    Packet(WorkerId, PacketWorker<ChainA, ChainB>),
}

impl<ChainA: ChainHandle + 'static, ChainB: ChainHandle + 'static> fmt::Display
    for Worker<ChainA, ChainB>
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{} <-> {}]", self.chains().a.id(), self.chains().b.id(),)
    }
}

impl<ChainA: ChainHandle + 'static, ChainB: ChainHandle + 'static> Worker<ChainA, ChainB> {
    /// Spawn a worker which relays events pertaining to an [`Object`] between two `chains`.
    pub fn spawn(
        chains: ChainHandlePair<ChainA, ChainB>,
        id: WorkerId,
        object: Object,
        msg_tx: Sender<WorkerMsg>,
        telemetry: Telemetry,
        config: &Config,
    ) -> WorkerHandle {
        let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();

        debug!("spawning worker for object {}", object.short_name(),);

        let worker = match &object {
            Object::Client(client) => Self::Client(
                id,
                ClientWorker::new(client.clone(), chains, cmd_rx, telemetry),
            ),
            Object::Connection(connection) => Self::Connection(
                id,
                ConnectionWorker::new(connection.clone(), chains, cmd_rx, telemetry),
            ),
            Object::Channel(channel) => Self::Channel(
                id,
                ChannelWorker::new(channel.clone(), chains, cmd_rx, telemetry),
            ),
            Object::Packet(path) => Self::Packet(
                id,
                PacketWorker::new(
                    path.clone(),
                    chains,
                    cmd_rx,
                    telemetry,
                    config.global.clear_packets_interval,
                ),
            ),
        };

        let thread_handle = std::thread::spawn(move || worker.run(msg_tx));
        WorkerHandle::new(id, object, cmd_tx, thread_handle)
    }

    /// Run the worker event loop.
    fn run(self, msg_tx: Sender<WorkerMsg>) {
        let id = self.id();
        let object = self.object();
        let name = format!("{}#{}", object.short_name(), id);

        let result = match self {
            Self::Client(_, w) => w.run(),
            Self::Connection(_, w) => w.run(),
            Self::Channel(_, w) => w.run(),
            Self::Packet(_, w) => w.run(),
        };

        if let Err(e) = result {
            error!("[{}] worker aborted with error: {}", name, e);
        }

        if let Err(e) = msg_tx.send(WorkerMsg::Stopped(id, object)) {
            error!(
                "[{}] failed to notify supervisor that worker stopped: {}",
                name, e
            );
        }

        info!("[{}] worker stopped", name);
    }

    fn id(&self) -> WorkerId {
        match self {
            Self::Client(id, _) => *id,
            Self::Connection(id, _) => *id,
            Self::Channel(id, _) => *id,
            Self::Packet(id, _) => *id,
        }
    }

    fn chains(&self) -> &ChainHandlePair<ChainA, ChainB> {
        match self {
            Self::Client(_, w) => w.chains(),
            Self::Connection(_, w) => w.chains(),
            Self::Channel(_, w) => w.chains(),
            Self::Packet(_, w) => w.chains(),
        }
    }

    fn object(&self) -> Object {
        match self {
            Worker::Client(_, w) => w.object().clone().into(),
            Worker::Connection(_, w) => w.object().clone().into(),
            Worker::Channel(_, w) => w.object().clone().into(),
            Worker::Packet(_, w) => w.object().clone().into(),
        }
    }
}
