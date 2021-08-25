use core::time::Duration;

use flex_error::{define_error, DetailOnly};
use ibc::application::ics20_fungible_token_transfer::msgs::transfer::MsgTransfer;
use ibc::events::IbcEvent;
use ibc::ics24_host::identifier::{ChainId, ChannelId, PortId};
use ibc::timestamp::{Timestamp, TimestampOverflowError};
use ibc::tx_msg::Msg;
use ibc::Height;

use crate::chain::handle::ChainHandle;
use crate::config::ChainConfig;
use crate::error::Error;

define_error! {
    PacketError {
        Relayer
            [ Error ]
            |_| { "relayer error" },

        Key
            [ Error ]
            |_| { "key error" },

        Submit
            { chain_id: ChainId }
            [ Error ]
            |e| {
                format!("failed while submitting the Transfer message to chain {0}",
                    e.chain_id)
            },

        TimestampOverflow
            [ DetailOnly<TimestampOverflowError> ]
            |_| { "timestamp overflow" },

        TxResponse
            { event: String }
            |e| {
                format!("tx response event consists of an error: {}",
                    e.event)
            },

        UnexpectedEvent
            { event: IbcEvent }
            |e| {
                format!("internal error, expected IBCEvent::ChainError, got {:?}",
                    e.event)
            },
    }
}

#[derive(Clone, Debug)]
pub struct TransferOptions {
    pub packet_src_chain_config: ChainConfig,
    pub packet_dst_chain_config: ChainConfig,
    pub packet_src_port_id: PortId,
    pub packet_src_channel_id: ChannelId,
    pub amount: u64,
    pub denom: String,
    pub receiver: Option<String>,
    pub timeout_height_offset: u64,
    pub timeout_seconds: Duration,
    pub number_msgs: usize,
}

pub fn build_and_send_transfer_messages<Chain: ChainHandle>(
    packet_src_chain: Chain, // the chain whose account is debited
    packet_dst_chain: Chain, // the chain whose account eventually gets credited
    opts: TransferOptions,
) -> Result<Vec<IbcEvent>, PacketError> {
    let receiver = match &opts.receiver {
        None => packet_dst_chain.get_signer(),
        Some(r) => Ok(r.clone().into()),
    }
    .map_err(PacketError::key)?;

    let sender = packet_src_chain.get_signer().map_err(PacketError::key)?;

    let timeout_timestamp = if opts.timeout_seconds == Duration::from_secs(0) {
        Timestamp::none()
    } else {
        (Timestamp::now() + opts.timeout_seconds).map_err(PacketError::timestamp_overflow)?
    };

    let timeout_height = if opts.timeout_height_offset == 0 {
        Height::zero()
    } else {
        packet_dst_chain
            .query_latest_height()
            .map_err(PacketError::relayer)?
            .add(opts.timeout_height_offset)
    };

    let msg = MsgTransfer {
        source_port: opts.packet_src_port_id.clone(),
        source_channel: opts.packet_src_channel_id.clone(),
        token: Some(ibc_proto::cosmos::base::v1beta1::Coin {
            denom: opts.denom.clone(),
            amount: opts.amount.to_string(),
        }),
        sender,
        receiver,
        timeout_height,
        timeout_timestamp,
    };

    let raw_msg = msg.to_any();
    let msgs = vec![raw_msg; opts.number_msgs];

    let events = packet_src_chain
        .send_messages_and_wait_commit(msgs)
        .map_err(|e| PacketError::submit(packet_src_chain.id(), e))?;

    // Check if the chain rejected the transaction
    let result = events
        .iter()
        .find(|event| matches!(event, IbcEvent::ChainError(_)));

    match result {
        None => Ok(events),
        Some(err) => {
            if let IbcEvent::ChainError(err) = err {
                Err(PacketError::tx_response(err.clone()))
            } else {
                panic!(
                    "internal error, expected IBCEvent::ChainError, got {:?}",
                    err
                )
            }
        }
    }
}
