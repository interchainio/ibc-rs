use core::fmt;
use core::iter::Iterator;
use core::time::Duration;
use std::time::Instant;

use tracing::{debug, error, trace};

use ibc::events::IbcEvent;
use ibc::ics24_host::identifier::{ChainId, ChannelId, PortId};
use ibc::query::{QueryTxHash, QueryTxRequest};

use crate::error::Error as RelayerError;
use crate::link::error::LinkError;
use crate::util::queue::Queue;
use crate::{
    chain::handle::ChainHandle,
    link::{operational_data::OperationalData, relay_sender::AsyncReply, RelaySummary, TxHashes},
};

pub const TIMEOUT: Duration = Duration::from_secs(100);

/// A wrapper over an [`OperationalData`] that is pending.
/// Additionally holds all the necessary information
/// to query for confirmations:
///     - hashes for all transactions in that op. data,
///     - the target chain to query for confirmations,
///     - timestamp to track time-outs and declare an
///         operational data as pending.
#[derive(Clone)]
pub struct PendingData {
    pub original_od: OperationalData,
    pub tx_hashes: TxHashes,
    pub submit_time: Instant,
    pub error_events: Vec<IbcEvent>,
}

/// The mediator stores all pending data
/// and tries to confirm them asynchronously.
pub struct PendingTxs<Chain> {
    pub chain: Chain,
    pub channel_id: ChannelId,
    pub port_id: PortId,
    pub counterparty_chain_id: ChainId,
    pub pending_queue: Queue<PendingData>,
}

impl<Chain> PendingTxs<Chain> {
    pub fn new(
        chain: Chain,
        channel_id: ChannelId,
        port_id: PortId,
        counterparty_chain_id: ChainId,
    ) -> Self {
        Self {
            chain,
            channel_id,
            port_id,
            counterparty_chain_id,
            pending_queue: Queue::new(),
        }
    }
}

impl<Chain: ChainHandle> PendingTxs<Chain> {
    pub fn chain_id(&self) -> ChainId {
        self.chain.id()
    }

    // Insert new pending transaction to the back of the queue.
    pub fn insert_new_pending_tx(&self, r: AsyncReply, od: OperationalData) {
        let mut tx_hashes = Vec::new();
        let mut error_events = Vec::new();

        for response in r.responses.into_iter() {
            if response.code.is_err() {
                // If the response is an error, we do not want to check for the
                // transaction confirmation status because it is never going to
                // be committed. Instead we convert it into an error event and
                // store it to be returned in the RelaySummary after all other
                // transactions have been confirmed.
                // This is not ideal, but is just to follow the previous synchronous
                // behavior of handling the OperationalData.
                trace!(
                    "[{}] putting error response in error event queue: {:?} ",
                    self,
                    response
                );

                let error_event = IbcEvent::ChainError(format!(
                    "deliver_tx on chain {} for Tx hash {} reports error: code={:?}, log={:?}",
                    self.chain_id(),
                    response.hash,
                    response.code,
                    response.log
                ));
                error_events.push(error_event);
            } else {
                tx_hashes.push(response.hash);
            }
        }

        let u = PendingData {
            original_od: od,
            tx_hashes: TxHashes(tx_hashes),
            submit_time: Instant::now(),
            error_events,
        };
        self.pending_queue.push_back(u);
    }

    fn check_tx_events(&self, tx_hashes: &TxHashes) -> Result<Option<Vec<IbcEvent>>, RelayerError> {
        let mut all_events = Vec::new();
        for hash in &tx_hashes.0 {
            let mut events = self
                .chain
                .query_txs(QueryTxRequest::Transaction(QueryTxHash(*hash)))?;

            if events.is_empty() {
                return Ok(None);
            } else {
                all_events.append(&mut events)
            }
        }
        Ok(Some(all_events))
    }

    // Try and process one pending transaction if available.
    pub fn process_pending(
        &self,
        timeout: Duration,
        resubmit: impl FnOnce(OperationalData) -> Result<AsyncReply, LinkError>,
    ) -> Result<Option<RelaySummary>, LinkError> {
        // We process pending transactions in a FIFO manner, so take from
        // the front of the queue.
        if let Some(pending) = self.pending_queue.pop_front() {
            let tx_hashes = &pending.tx_hashes;
            let submit_time = &pending.submit_time;

            if tx_hashes.0.is_empty() {
                return Ok(Some(RelaySummary::from_events(pending.error_events)));
            }

            // Process the given pending transaction.
            trace!("[{}] trying to confirm {} ", self, tx_hashes);

            // Check for TX events for the given pending transaction hashes.
            let events_result = self.check_tx_events(tx_hashes);
            let res = match events_result {
                Ok(None) => {
                    // There is no events for the associated transactions.
                    // This means the transaction has not yet been committed.

                    trace!(
                        "[{}] transaction is not yet committed: {} ",
                        self,
                        tx_hashes
                    );

                    if submit_time.elapsed() > timeout {
                        // The submission time for the transaction has exceeded the
                        // timeout threshold. Returning Outcome::Timeout for the
                        // relayer to resubmit the transaction to the chain again.
                        error!("[{}] timed out while confirming {}", self, tx_hashes);

                        let resubmit_res = resubmit(pending.original_od.clone());

                        match resubmit_res {
                            Ok(reply) => {
                                self.insert_new_pending_tx(reply, pending.original_od);
                                Ok(None)
                            }
                            Err(e) => {
                                self.pending_queue.push_back(pending);
                                Err(e)
                            }
                        }
                    } else {
                        // Reinsert the pending transaction, this time
                        // to the back of the queue so that we process other
                        // pending transactions first in the meanwhile.
                        self.pending_queue.push_back(pending);
                        Ok(None)
                    }
                }
                Ok(Some(events)) => {
                    // We get a list of events for the transaction hashes,
                    // Meaning the transaction has been committed successfully
                    // to the chain.

                    debug!(
                        "[{}] confirmed after {:#?}: {} ",
                        self,
                        pending.submit_time.elapsed(),
                        tx_hashes
                    );

                    // Convert the events to RelaySummary and return them.
                    let mut summary = RelaySummary::from_events(events);
                    summary.extend(RelaySummary::from_events(pending.error_events));

                    Ok(Some(summary))
                }
                Err(e) => {
                    // There are errors querying for the transaction hashes.
                    // This may be temporary errors when the relayer is communicating
                    // with the chain endpoint.

                    error!(
                        "[{}] error querying for tx hashes {}: {}. will retry again later",
                        self, tx_hashes, e
                    );

                    // Push it to the back of the pending queue to process it
                    // again at a later time.
                    self.pending_queue.push_back(pending);

                    Err(LinkError::relayer(e))
                }
            };

            if !self.pending_queue.is_empty() {
                trace!(
                    "[{}] total pending transactions left: {}",
                    self,
                    self.pending_queue.len()
                );
            }

            res
        } else {
            Ok(None)
        }
    }
}

impl<Chain: ChainHandle> fmt::Display for PendingTxs<Chain> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}/{} -> {}",
            self.chain_id(),
            self.port_id,
            self.channel_id,
            self.counterparty_chain_id
        )
    }
}
