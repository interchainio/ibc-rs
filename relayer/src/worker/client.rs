use core::time::Duration;
use std::thread;

use crossbeam_channel::Receiver;
use tracing::{debug, info, trace, warn};

use ibc::{events::IbcEvent, ics02_client::events::UpdateClient};

use crate::{
    chain::handle::{ChainHandle, ChainHandlePair},
    foreign_client::{ForeignClient, ForeignClientErrorDetail, MisbehaviourResults},
    object::Client,
    telemetry,
    telemetry::Telemetry,
};

use super::error::RunError;
use super::WorkerCmd;

pub struct ClientWorker<ChainA: ChainHandle, ChainB: ChainHandle> {
    client: Client,
    chains: ChainHandlePair<ChainA, ChainB>,
    cmd_rx: Receiver<WorkerCmd>,

    #[allow(dead_code)]
    telemetry: Telemetry,
}

impl<ChainA: ChainHandle, ChainB: ChainHandle> ClientWorker<ChainA, ChainB> {
    pub fn new(
        client: Client,
        chains: ChainHandlePair<ChainA, ChainB>,
        cmd_rx: Receiver<WorkerCmd>,
        telemetry: Telemetry,
    ) -> Self {
        Self {
            client,
            chains,
            cmd_rx,
            telemetry,
        }
    }

    /// Run the event loop for events associated with a [`Client`].
    pub fn run(self) -> Result<(), RunError> {
        let mut client = ForeignClient::restore(
            self.client.dst_client_id.clone(),
            self.chains.b.clone(),
            self.chains.a.clone(),
        );

        info!(
            "[{}] running client worker & initial misbehaviour detection",
            client
        );

        // initial check for evidence of misbehaviour for all updates
        let skip_misbehaviour = self.detect_misbehaviour(&client, None);

        loop {
            thread::sleep(Duration::from_millis(600));

            // Run client refresh, exit only if expired or frozen
            match client.refresh() {
                Ok(Some(_)) => {
                    telemetry! {
                        self.telemetry.ibc_client_update(
                            &self.client.dst_chain_id,
                            &self.client.dst_client_id,
                            1
                        )
                    };
                }
                Err(e) => {
                    if let ForeignClientErrorDetail::ExpiredOrFrozen(_) = e.detail() {
                        warn!("failed to refresh client '{}': {}", client, e);

                        // This worker has completed its job as the client cannot be refreshed any
                        // further, and can therefore exit without an error.
                        return Ok(());
                    }
                }
                _ => (),
            };

            if skip_misbehaviour {
                continue;
            }

            if let Ok(cmd) = self.cmd_rx.try_recv() {
                match self.process_cmd(cmd, &client) {
                    Next::Continue => continue,
                    Next::Abort => break,
                };
            }
        }

        Ok(())
    }

    fn process_cmd(&self, cmd: WorkerCmd, client: &ForeignClient<ChainB, ChainA>) -> Next {
        match cmd {
            WorkerCmd::IbcEvents { batch } => {
                trace!("[{}] worker received batch: {:?}", client, batch);

                for event in batch.events {
                    if let IbcEvent::UpdateClient(update) = event {
                        debug!("[{}] client was updated", client);

                        // Run misbehaviour. If evidence submitted the loop will exit in next
                        // iteration with frozen client
                        if self.detect_misbehaviour(client, Some(update)) {
                            telemetry! {
                                self.telemetry.ibc_client_misbehaviour(
                                    &self.client.dst_chain_id,
                                    &self.client.dst_client_id,
                                    1
                                )
                            };
                        }
                    }
                }

                Next::Continue
            }

            WorkerCmd::NewBlock { .. } => Next::Continue,
            WorkerCmd::ClearPendingPackets => Next::Continue,

            WorkerCmd::Shutdown => Next::Abort,
        }
    }

    fn detect_misbehaviour(
        &self,
        client: &ForeignClient<ChainB, ChainA>,
        update: Option<UpdateClient>,
    ) -> bool {
        match client.detect_misbehaviour_and_submit_evidence(update) {
            MisbehaviourResults::ValidClient => false,
            MisbehaviourResults::VerificationError => {
                // can retry in next call
                false
            }
            MisbehaviourResults::EvidenceSubmitted(_) => {
                // if evidence was submitted successfully then exit
                true
            }
            MisbehaviourResults::CannotExecute => {
                // skip misbehaviour checking if chain does not have support for it (i.e. client
                // update event does not include the header)
                true
            }
        }
    }

    /// Get a reference to the client worker's chains.
    pub fn chains(&self) -> &ChainHandlePair<ChainA, ChainB> {
        &self.chains
    }

    /// Get a reference to the client worker's object.
    pub fn object(&self) -> &Client {
        &self.client
    }
}

pub enum Next {
    Abort,
    Continue,
}
