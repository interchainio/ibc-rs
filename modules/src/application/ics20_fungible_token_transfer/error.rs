use crate::ics04_channel::error as channel_error;
use crate::ics24_host::error::ValidationError;
use crate::ics24_host::identifier::{ChannelId, PortId};
use crate::prelude::*;
use flex_error::define_error;

define_error! {
    #[derive(Debug, PartialEq, Eq)]
    Error {
        UnknowMessageTypeUrl
            { url: String }
            | e | { format_args!("unrecognized ICS-20 transfer message type URL {0}", e.url) },

        Ics04Channel
            [ channel_error::Error ]
            |_ | { "Ics04 channel error" },

        SequenceSendNotFound
            { port_id: PortId, channel_id: ChannelId }
            | e | { format_args!("sending sequence number not found for port {0} and channel {1}", e.port_id, e.channel_id) },

        ChannelNotFound
            { port_id: PortId, channel_id: ChannelId }
            | e | { format_args!("sending sequence number not found for port {0} and channel {1}", e.port_id, e.channel_id) },

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
    }
}
