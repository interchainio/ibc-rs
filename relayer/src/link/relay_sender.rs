use core::fmt;

use prost_types::Any;
use tendermint_rpc::endpoint::broadcast::tx_sync;
use tracing::info;

use ibc::events::{IbcEvent, PrettyEvents};

use crate::chain::handle::ChainHandle;
use crate::link::error::LinkError;
use crate::link::RelaySummary;

pub trait SubmitReply {
    fn empty() -> Self;
}

impl SubmitReply for RelaySummary {
    fn empty() -> Self {
        RelaySummary::empty()
    }
}

/// Captures the ability to submit messages to a chain.
pub trait Submit {
    type Reply: SubmitReply;

    fn submit(target: &impl ChainHandle, msgs: Vec<Any>) -> Result<Self::Reply, LinkError>;
}

/// Synchronous sender
pub struct SyncSender;

impl Submit for SyncSender {
    type Reply = RelaySummary;

    // TODO: Switch from the `Chain::send_msgs` interface in this method
    //  to use `Chain::submit_msgs` instead; implement waiting for block
    //  commits directly here (instead of blocking in the chain runtime).
    fn submit(target: &impl ChainHandle, msgs: Vec<Any>) -> Result<Self::Reply, LinkError> {
        let tx_events = target
            .send_messages_and_wait_commit(msgs)
            .map_err(LinkError::relayer)?;

        info!(
            "[Sync->{}] result {}\n",
            target.id(),
            PrettyEvents(&tx_events)
        );

        let ev = tx_events
            .clone()
            .into_iter()
            .find(|event| matches!(event, IbcEvent::ChainError(_)));

        match ev {
            Some(ev) => Err(LinkError::send(ev)),
            None => Ok(RelaySummary::from_events(tx_events)),
        }
    }
}

pub struct AsyncReply {
    pub responses: Vec<tx_sync::Response>,
}

impl SubmitReply for AsyncReply {
    fn empty() -> Self {
        Self { responses: vec![] }
    }
}

// TODO(Adi): Consider removing the senders and keep only a generic
//     send/submit method.
pub struct AsyncSender;

impl Submit for AsyncSender {
    type Reply = AsyncReply;

    fn submit(target: &impl ChainHandle, msgs: Vec<Any>) -> Result<Self::Reply, LinkError> {
        let a = target
            .send_messages_and_wait_check_tx(msgs)
            .map_err(LinkError::relayer)?;
        let reply = AsyncReply { responses: a };
        info!("[Async~>{}] {}\n", target.id(), reply);

        Ok(reply)
    }
}

impl fmt::Display for AsyncReply {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "response(s): {}", self.responses.len())?;
        self.responses
            .iter()
            .try_for_each(|r| write!(f, "; {:?}:{}", r.code, r.hash))
    }
}
