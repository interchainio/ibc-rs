use std::convert::Infallible;
use std::str::Utf8Error;
use std::string::FromUtf8Error;

use flex_error::{define_error, DisplayOnly, TraceError};
use subtle_encoding::Error as EncodingError;
use tendermint_proto::Error as TendermintProtoError;
use uint::FromDecStrErr;

use crate::core::ics04_channel::channel::Ordering;
use crate::core::ics04_channel::error as channel_error;
use crate::core::ics24_host::error::ValidationError;
use crate::core::ics24_host::identifier::{ChannelId, PortId};
use crate::signer::SignerError;

define_error! {
    #[derive(Debug, PartialEq, Eq)]
    Error {
        UnknowMessageTypeUrl
            { url: String }
            | e | { format_args!("unrecognized ICS-20 transfer message type URL {0}", e.url) },

        Ics04Channel
            [ channel_error::Error ]
            |_ | { "Ics04 channel error" },

        DestinationChannelNotFound
            { port_id: PortId, channel_id: ChannelId }
            | e | { format_args!("destination channel not found in the counterparty of port_id {0} and channel_id {1} ", e.port_id, e.channel_id) },

        InvalidPortId
            { context: String }
            [ ValidationError ]
            | _ | { "invalid port identifier" },

        InvalidChannelId
            { context: String }
            [ ValidationError ]
            | _ | { "invalid channel identifier" },

        InvalidPacketTimeoutHeight
            { context: String }
            | _ | { "invalid packet timeout height value" },

        InvalidPacketTimeoutTimestamp
            { timestamp: u64 }
            | _ | { "invalid packet timeout timestamp value" },

        Utf8
            [ DisplayOnly<FromUtf8Error> ]
            | _ | { "utf8 decoding error" },

        EmptyBaseDenom
            |_| { "base denomination is empty" },

        InvalidTracePortId
            { pos: usize }
            [ ValidationError ]
            | e | { format_args!("invalid port id in trace at position: {0}", e.pos) },

        InvalidTraceChannelId
            { pos: usize }
            [ ValidationError ]
            | e | { format_args!("invalid channel id in trace at position: {0}", e.pos) },

        InvalidTraceLength
            { len: usize }
            | e | { format_args!("trace length must be even but got: {0}", e.len) },

        InvalidAmount
            [ TraceError<FromDecStrErr> ]
            | _ | { "invalid amount" },

        InvalidToken
            | _ | { "invalid token" },

        Signer
            [ SignerError ]
            | _ | { "failed to parse signer" },

        MissingDenomIbcPrefix
            | _ | { "missing 'ibc/' prefix in denomination" },

        MalformedHashDenom
            | _ | { "hashed denom must be of the form 'ibc/{Hash}'" },

        ParseHex
            [ TraceError<EncodingError> ]
            | _ | { "invalid hex string" },

        ChannelNotUnordered
            { order: Ordering }
            | e | { format_args!("expected '{0}' channel, got '{1}'", Ordering::Unordered, e.order) },

        CantCloseChannel
            | _ | { "channel cannot be closed" },

        PacketDataDeserialization
            | _ | { "failed to deserialize packet data" },

        AckDeserialization
            | _ | { "failed to deserialize acknowledgement" },

        ReceiveDisabled
            | _ | { "receive is not enabled" },

        SendDisabled
            | _ | { "send is not enabled" },

        ParseAccountFailure
            | _ | { "failed to parse as AccountId" },

        InvalidPort
            { port_id: PortId, exp_port_id: PortId }
            | e | { format_args!("invalid port: '{0}', expected '{1}'", e.port_id, e.exp_port_id) },

        TraceNotFound
            | _ | { "no trace associated with specified hash" },

        DecodeRawMsg
            [ TraceError<TendermintProtoError> ]
            | _ | { "error decoding raw msg" },

        UnknownMsgType
            { msg_type: String }
            | e | { format_args!("unknown msg type: {0}", e.msg_type) },

        InvalidCoin
            { coin: String }
            | e | { format_args!("invalid coin: {}", e.coin) },

        Utf8Decode
            [ TraceError<Utf8Error> ]
            | _ | { "error decoding raw bytes as UTF8 string" },
    }
}

impl From<Infallible> for Error {
    fn from(e: Infallible) -> Self {
        match e {}
    }
}
