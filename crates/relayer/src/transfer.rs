use ibc_relayer_types::core::ics04_channel::version::Version;
use ibc_relayer_types::signer::SignerError;
use std::ops::Add;
use std::str::FromStr;

use core::time::Duration;

use flex_error::{define_error, DetailOnly};
use ibc_proto::cosmos::base::v1beta1::Coin;
use ibc_proto::google::protobuf::Any;
use ibc_relayer_types::applications::transfer::error::Error as Ics20Error;
use ibc_relayer_types::applications::transfer::msgs::transfer::{
    MsgTransfer, MsgTransferV1, MsgTransferV2,
};
use ibc_relayer_types::applications::transfer::Amount;
use ibc_relayer_types::core::ics04_channel::timeout::TimeoutHeight;
use ibc_relayer_types::core::ics24_host::identifier::{ChainId, ChannelId, PortId};
use ibc_relayer_types::events::IbcEvent;
use ibc_relayer_types::signer::Signer;
use ibc_relayer_types::timestamp::{Timestamp, TimestampOverflowError};
use ibc_relayer_types::tx_msg::Msg;

use crate::chain::endpoint::ChainStatus;
use crate::chain::handle::ChainHandle;
use crate::chain::requests::{IncludeProof, QueryChannelRequest, QueryHeight};
use crate::chain::tracking::TrackedMsgs;
use crate::error::Error;
use crate::event::IbcEventWithHeight;

define_error! {
    TransferError {
        ReceiverAddress
            [ SignerError ]
            |_| { "receiver address error "},
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

        TokenTransfer
            [ Ics20Error ]
            |_| { "token transfer error" },

        ZeroTimeout
            | _ | { "packet timeout height and packet timeout timestamp cannot both be 0" },
    }
}

#[derive(Copy, Clone)]
pub struct TransferTimeout {
    pub timeout_height: TimeoutHeight,
    pub timeout_timestamp: Timestamp,
}

impl TransferTimeout {
    /**
       Construct the transfer timeout parameters from the given timeout
       height offset, timeout duration, and the latest chain status
       containing the latest time of the destination chain.

       The height offset and duration are optional, with zero indicating
       that the packet do not get expired at the given height or time.
       If both height offset and duration are zero, then the packet will
       never expire.
    */
    pub fn new(
        timeout_height_offset: u64,
        timeout_duration: Duration,
        destination_chain_status: &ChainStatus,
    ) -> Result<Self, TransferError> {
        let timeout_height = if timeout_height_offset == 0 {
            TimeoutHeight::no_timeout()
        } else {
            destination_chain_status
                .height
                .add(timeout_height_offset)
                .into()
        };

        let timeout_timestamp = if timeout_duration == Duration::ZERO {
            Timestamp::none()
        } else {
            (destination_chain_status.timestamp + timeout_duration)
                .map_err(TransferError::timestamp_overflow)?
        };

        Ok(TransferTimeout {
            timeout_height,
            timeout_timestamp,
        })
    }
}

#[derive(Clone, Debug)]
pub struct TransferOptions {
    pub src_port_id: PortId,
    pub src_channel_id: ChannelId,
    pub tokens: Vec<(Amount, String)>,
    pub receiver: Option<String>,
    pub timeout_height_offset: u64,
    pub timeout_duration: Duration,
    pub number_msgs: usize,
    pub memo: Option<String>,
}

pub fn build_transfer_message(
    src_port_id: PortId,
    src_channel_id: ChannelId,
    channel_version: Version,
    tokens: Vec<(Amount, String)>,
    sender: Signer,
    receiver: Signer,
    timeout_height: TimeoutHeight,
    timeout_timestamp: Timestamp,
    memo: Option<String>,
) -> Any {
    if channel_version.is_ics20_v2() {
        let tokens = tokens
            .into_iter()
            .map(|(amount, denom)| Coin {
                denom,
                amount: amount.to_string(),
            })
            .collect();
        let msg = MsgTransferV2 {
            source_port: src_port_id,
            source_channel: src_channel_id,
            sender,
            receiver,
            timeout_height,
            timeout_timestamp,
            memo,
            tokens,
        };

        MsgTransfer::V2(msg).to_any()
    } else {
        let token = tokens
            .first()
            .map(|(amount, denom)| Coin {
                denom: denom.to_string(),
                amount: amount.to_string(),
            })
            .unwrap();
        let msg = MsgTransferV1 {
            source_port: src_port_id,
            source_channel: src_channel_id,
            token,
            sender,
            receiver,
            timeout_height,
            timeout_timestamp,
            memo,
        };

        MsgTransfer::V1(msg).to_any()
    }
}

pub fn build_transfer_messages<SrcChain: ChainHandle, DstChain: ChainHandle>(
    src_chain: &SrcChain, // the chain whose account is debited
    dst_chain: &DstChain, // the chain whose account eventually gets credited
    opts: &TransferOptions,
) -> Result<Vec<Any>, TransferError> {
    let receiver = match &opts.receiver {
        Some(receiver) => Signer::from_str(receiver).map_err(TransferError::receiver_address)?,
        None => dst_chain.get_signer().map_err(TransferError::key)?,
    };

    let sender = src_chain.get_signer().map_err(TransferError::key)?;

    let destination_chain_status = dst_chain
        .query_application_status()
        .map_err(TransferError::relayer)?;

    let timeout = TransferTimeout::new(
        opts.timeout_height_offset,
        opts.timeout_duration,
        &destination_chain_status,
    )?;

    let (channel, _) = src_chain
        .query_channel(
            QueryChannelRequest {
                port_id: opts.src_port_id.clone(),
                channel_id: opts.src_channel_id.clone(),
                height: QueryHeight::Latest,
            },
            IncludeProof::No,
        )
        .map_err(TransferError::relayer)?;

    let message = build_transfer_message(
        opts.src_port_id.clone(),
        opts.src_channel_id.clone(),
        channel.version,
        opts.tokens.clone(),
        sender,
        receiver,
        timeout.timeout_height,
        timeout.timeout_timestamp,
        opts.memo.clone(),
    );

    let msgs = vec![message; opts.number_msgs];

    Ok(msgs)
}

pub fn send_messages<Chain: ChainHandle>(
    chain: &Chain,
    msgs: Vec<Any>,
) -> Result<Vec<IbcEventWithHeight>, TransferError> {
    let events_with_heights = chain
        .send_messages_and_wait_commit(TrackedMsgs::new_static(msgs, "ft-transfer"))
        .map_err(|e| TransferError::submit(chain.id(), e))?;

    // Check if the chain rejected the transaction
    let result = events_with_heights
        .iter()
        .find(|event| matches!(event.event, IbcEvent::ChainError(_)));

    match result {
        None => Ok(events_with_heights),
        Some(err) => {
            if let IbcEvent::ChainError(ref err) = err.event {
                Err(TransferError::tx_response(err.clone()))
            } else {
                panic!("internal error, expected IBCEvent::ChainError, got {err:?}")
            }
        }
    }
}

pub fn build_and_send_transfer_messages<SrcChain: ChainHandle, DstChain: ChainHandle>(
    // the chain whose account is debited
    src_chain: &SrcChain,
    // the chain whose account eventually gets credited
    dst_chain: &DstChain,
    // options describing the transfer
    opts: &TransferOptions,
) -> Result<Vec<IbcEventWithHeight>, TransferError> {
    let msgs = build_transfer_messages(src_chain, dst_chain, opts)?;
    send_messages(src_chain, msgs)
}
