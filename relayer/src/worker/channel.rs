use core::time::Duration;
use std::thread;

use crossbeam_channel::Receiver;
use tracing::{debug, info, warn};

use crate::channel::Channel as RelayChannel;
use crate::telemetry::Telemetry;
use crate::{
    chain::handle::{ChainHandle, ChainHandlePair},
    object::Channel,
    util::retry::retry_with_index,
    worker::retry_strategy,
};

use super::error::RunError;
use super::WorkerCmd;

pub struct ChannelWorker<ChainA: ChainHandle, ChainB: ChainHandle> {
    channel: Channel,
    chains: ChainHandlePair<ChainA, ChainB>,
    cmd_rx: Receiver<WorkerCmd>,

    #[allow(dead_code)]
    telemetry: Telemetry,
}

impl<ChainA: ChainHandle, ChainB: ChainHandle> ChannelWorker<ChainA, ChainB> {
    pub fn new(
        channel: Channel,
        chains: ChainHandlePair<ChainA, ChainB>,
        cmd_rx: Receiver<WorkerCmd>,
        telemetry: Telemetry,
    ) -> Self {
        Self {
            channel,
            chains,
            cmd_rx,
            telemetry,
        }
    }

    /// Run the event loop for events associated with a [`Channel`].
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
                        // there can be up to two event for this channel, e.g. init and try.
                        // process the last event, the one with highest "rank".
                        let last_event = batch.events.last();
                        debug!(
                            channel = %self.channel.short_name(),
                            "channel worker starts processing {:#?}", last_event
                        );

                        match last_event {
                            Some(event) => {
                                let mut handshake_channel = RelayChannel::restore_from_event(
                                    a_chain.clone(),
                                    b_chain.clone(),
                                    event.clone(),
                                )
                                .map_err(RunError::channel)?;

                                retry_with_index(
                                    retry_strategy::worker_default_strategy(),
                                    |index| handshake_channel.step_event(event.clone(), index),
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
                            channel = %self.channel.short_name(),
                            "Channel worker starts processing block event at {:#?}", current_height
                        );

                        let height = current_height.decrement().map_err(RunError::ics02)?;

                        let (mut handshake_channel, state) = RelayChannel::restore_from_state(
                            a_chain.clone(),
                            b_chain.clone(),
                            self.channel.clone(),
                            height,
                        )
                        .map_err(RunError::channel)?;

                        retry_with_index(retry_strategy::worker_default_strategy(), |index| {
                            handshake_channel.step_state(state, index)
                        })
                    }

                    WorkerCmd::Shutdown => {
                        info!(channel = %self.channel.short_name(), "shutting down Channel worker");
                        return Ok(());
                    }

                    WorkerCmd::ClearPendingPackets => Ok(()), // nothing to do
                };

                if let Err(retries) = result {
                    warn!(channel = %self.channel.short_name(), "Channel worker failed after {} retries", retries);

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
    pub fn object(&self) -> &Channel {
        &self.channel
    }
}
