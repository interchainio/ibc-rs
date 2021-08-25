use core::time::Duration;
use std::thread;

use crossbeam_channel::Receiver;
use tracing::{debug, info, warn};

use crate::connection::Connection as RelayConnection;
use crate::telemetry::Telemetry;
use crate::{
    chain::handle::{ChainHandle, ChainHandlePair},
    object::Connection,
    util::retry::retry_with_index,
    worker::retry_strategy,
};

use super::error::RunError;
use super::WorkerCmd;

pub struct ConnectionWorker<ChainA: ChainHandle, ChainB: ChainHandle> {
    connection: Connection,
    chains: ChainHandlePair<ChainA, ChainB>,
    cmd_rx: Receiver<WorkerCmd>,

    #[allow(dead_code)]
    telemetry: Telemetry,
}

impl<ChainA: ChainHandle, ChainB: ChainHandle> ConnectionWorker<ChainA, ChainB> {
    pub fn new(
        connection: Connection,
        chains: ChainHandlePair<ChainA, ChainB>,
        cmd_rx: Receiver<WorkerCmd>,
        telemetry: Telemetry,
    ) -> Self {
        Self {
            connection,
            chains,
            cmd_rx,
            telemetry,
        }
    }

    /// Run the event loop for events associated with a [`Connection`].
    pub(crate) fn run(self) -> Result<(), RunError> {
        let a_chain = self.chains.a.clone();
        let b_chain = self.chains.b.clone();

        // Flag that indicates if the worker should actively resume handshake.
        // Set on start or when event based handshake fails.
        let mut resume_handshake = true;

        loop {
            thread::sleep(Duration::from_millis(200));

            if let Ok(cmd) = self.cmd_rx.try_recv() {
                let result = match cmd {
                    WorkerCmd::IbcEvents { batch } => {
                        // there can be up to two event for this connection, e.g. init and try.
                        // process the last event, the one with highest "rank".
                        let last_event = batch.events.last();

                        debug!(
                            connection = %self.connection.short_name(),
                            "connection worker starts processing {:#?}", last_event
                        );

                        match last_event {
                            Some(event) => {
                                let mut handshake_connection = RelayConnection::restore_from_event(
                                    a_chain.clone(),
                                    b_chain.clone(),
                                    event.clone(),
                                )
                                .map_err(RunError::connection)?;

                                retry_with_index(
                                    retry_strategy::worker_default_strategy(),
                                    |index| handshake_connection.step_event(event.clone(), index),
                                )
                            }
                            None => Ok(()),
                        }
                    }

                    WorkerCmd::NewBlock {
                        height: current_height,
                        new_block: _,
                    } => {
                        if !resume_handshake {
                            continue;
                        }

                        debug!(
                            connection = %self.connection.short_name(),
                            "connection worker starts processing block event at {}",
                            current_height
                        );

                        let height = current_height.decrement().map_err(RunError::ics02)?;

                        let (mut handshake_connection, state) =
                            RelayConnection::restore_from_state(
                                a_chain.clone(),
                                b_chain.clone(),
                                self.connection.clone(),
                                height,
                            )
                            .map_err(RunError::connection)?;

                        retry_with_index(retry_strategy::worker_default_strategy(), |index| {
                            handshake_connection.step_state(state, index)
                        })
                    }

                    WorkerCmd::Shutdown => {
                        info!(connection = %self.connection.short_name(), "shutting down Connection worker");
                        return Ok(());
                    }

                    WorkerCmd::ClearPendingPackets => Ok(()), // nothing to do
                };

                if let Err(retries) = result {
                    warn!(
                        connection = %self.connection.short_name(),
                        "connection worker failed after {} retries", retries
                    );

                    // Resume handshake on next iteration.
                    resume_handshake = true;
                } else {
                    resume_handshake = false;
                }
            }
        }
    }

    /// Get a reference to the uni chan path worker's chains.
    pub fn chains(&self) -> &ChainHandlePair<ChainA, ChainB> {
        &self.chains
    }

    /// Get a reference to the client worker's object.
    pub fn object(&self) -> &Connection {
        &self.connection
    }
}
