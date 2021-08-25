use core::time::Duration;
use flex_error::define_error;
use ibc::events::IbcEvent;
use ibc::ics02_client::error::Error as ClientError;
use ibc::ics04_channel::channel::State;
use ibc::ics24_host::identifier::{ChainId, ChannelId, ClientId, PortChannelId, PortId};

use crate::error::Error;
use crate::foreign_client::ForeignClientError;
use crate::supervisor::Error as SupervisorError;

define_error! {
    ChannelError {
        Relayer
            [ Error ]
            |_| { "relayer error" },

        Supervisor
            [ SupervisorError ]
            |_| { "supervisor error" },

        Client
            [ ClientError ]
            |_| { "ICS02 client error" },

        InvalidChannel
            { reason: String }
            | e | {
                format_args!("invalid channel: {0}",
                    e.reason)
            },

        MissingLocalChannelId
            |_| { "failed due to missing local channel id" },

        MissingLocalConnection
            { chain_id: ChainId }
            | e | {
                format_args!("channel constructor failed due to missing connection id on chain id {0}",
                    e.chain_id)
            },

        MissingCounterpartyChannelId
            |_| { "failed due to missing counterparty channel id" },

        MissingCounterpartyConnection
            |_| { "failed due to missing counterparty connection" },

        MissingChannelOnDestination
            |_| { "missing channel on destination chain" },

        ChannelProof
            [ Error ]
            |_| { "failed to build channel proofs" },

        ClientOperation
            {
                client_id: ClientId,
                chain_id: ChainId,
            }
            [ ForeignClientError ]
            | e | {
                format_args!("failed during an operation on client ({0}) hosted by chain ({1})",
                    e.client_id, e.chain_id)
            },

        FetchSigner
            { chain_id: ChainId }
            [ Error ]
            |e| { format_args!("failed while fetching the signer for destination chain {}", e.chain_id) },

        Query
            { chain_id: ChainId }
            [ Error ]
            |e| { format_args!("failed during a query to chain id {0}", e.chain_id) },

        QueryChannel
            { channel_id: ChannelId }
            [ SupervisorError ]
            |e| { format_args!("failed during a query to channel id {0}", e.channel_id) },

        Submit
            { chain_id: ChainId }
            [ Error ]
            |_| { "failed during a transaction submission step to chain id {0}" },

        HandshakeFinalize
            {
                port_id: PortId,
                channel_id: ChannelId,
                chain_id: ChainId,
            }
            [ Error ]
            |e| {
                format_args!("failed to finalize a channel open handshake while querying for channel end {0}/{1} on chain chain {2}",
                    e.port_id, e.channel_id, e.chain_id)
            },

        PartialOpenHandshake
            {
                state: State,
                counterparty_state: State
            }
            | e | {
                format_args!("the channel is partially open ({0}, {1})",
                    e.state, e.counterparty_state)
            },

        IncompleteChannelState
            {
                chain_id: ChainId,
                port_channel_id: PortChannelId,
            }
            | e | {
                format_args!("channel {0} on chain {1} has no counterparty channel id",
                    e.port_channel_id, e.chain_id)
            },

        ChannelAlreadyExist
            { channel_id: ChannelId }
            |e| { format_args!("channel {} already exist in an incompatible state", e.channel_id) },

        MismatchChannelEnds
            {
                chain_id: ChainId,
                port_channel_id: PortChannelId,
                expected_counterrparty_port_channel_id: PortChannelId,
                actual_counterrparty_port_channel_id: PortChannelId,
            }
            | e | {
                format_args!("channel {0} on chain {1} expected to have counterparty {2} (but instead has {3})",
                    e.port_channel_id, e.chain_id,
                    e.expected_counterrparty_port_channel_id,
                    e.actual_counterrparty_port_channel_id)
            },

        MismatchPort
            {
                destination_chain_id: ChainId,
                destination_port_id: PortId,
                source_chain_id: ChainId,
                counterparty_port_id: PortId,
                counterparty_channel_id: ChannelId,
            }
            | e | {
                format_args!("channel open try to chain `{}` and destination port `{}` does not match \
                the source chain `{}` counterparty port `{}` for channel_id {}",
                    e.destination_chain_id, e.destination_port_id,
                    e.source_chain_id,
                    e.counterparty_port_id,
                    e.counterparty_channel_id)
            },

        MissingEvent
            {
                description: String
            }
            | e | {
                format_args!("missing event: {}", e.description)
            },

        MaxRetry
            {
                description: String,
                tries: u64,
                total_delay: Duration,
                source: Box<ChannelErrorDetail>,
            }
            | e | {
                format_args!("Error after maximum retry of {} and total delay of {}s: {}",
                    e.tries, e.total_delay.as_secs(), e.description)
            },

        RetryInternal
            { reason: String }
            | e | {
                format_args!("Encountered internal error during retry: {}",
                    e.reason)
            },

        TxResponse
            { reason: String }
            | e | {
                format_args!("tx response error: {}",
                    e.reason)
            },

        InvalidEvent
            { event: IbcEvent }
            | e | {
                format_args!("channel object cannot be built from event: {}",
                    e.event)
            },
    }
}
