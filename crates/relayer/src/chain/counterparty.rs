use std::collections::HashSet;

use ibc_relayer_types::{
    core::{
        ics03_connection::connection::{
            ConnectionEnd, IdentifiedConnectionEnd, State as ConnectionState,
        },
        ics04_channel::{
            channel::{IdentifiedChannelEnd, State},
            packet::Sequence,
        },
        ics24_host::identifier::{
            ChainId, ChannelId, ClientId, ConnectionId, PortChannelId, PortId,
        },
    },
    Height,
};
use serde::{Deserialize, Serialize};
use tracing::{error, trace};

use super::requests::{
    IncludeProof, PageRequest, QueryChannelRequest, QueryClientConnectionsRequest,
    QueryClientStateRequest, QueryConnectionRequest, QueryPacketAcknowledgementsRequest,
    QueryUnreceivedAcksRequest, QueryUnreceivedPacketsRequest,
};
use super::{
    handle::ChainHandle,
    requests::{QueryConnectionChannelsRequest, QueryPacketCommitmentsRequest},
};
use crate::chain::requests::QueryHeight;
use crate::channel::ChannelError;
use crate::client_state::IdentifiedAnyClientState;
use crate::path::PathIdentifiers;
use crate::supervisor::Error;

pub fn counterparty_chain_from_connection(
    src_chain: &impl ChainHandle,
    src_connection_id: &ConnectionId,
) -> Result<ChainId, Error> {
    let (connection_end, _) = src_chain
        .query_connection(
            QueryConnectionRequest {
                connection_id: src_connection_id.clone(),
                height: QueryHeight::Latest,
            },
            IncludeProof::No,
        )
        .map_err(Error::relayer)?;

    let client_id = connection_end.client_id();
    let (client_state, _) = src_chain
        .query_client_state(
            QueryClientStateRequest {
                client_id: client_id.clone(),
                height: QueryHeight::Latest,
            },
            IncludeProof::No,
        )
        .map_err(Error::relayer)?;

    trace!(
        chain_id=%src_chain.id(), connection_id=%src_connection_id,
        "counterparty chain: {}", client_state.chain_id()
    );
    Ok(client_state.chain_id())
}

fn connection_on_destination(
    connection_id_on_source: &ConnectionId,
    counterparty_client_id: &ClientId,
    counterparty_chain: &impl ChainHandle,
) -> Result<Option<ConnectionEnd>, Error> {
    let counterparty_connections = counterparty_chain
        .query_client_connections(QueryClientConnectionsRequest {
            client_id: counterparty_client_id.clone(),
        })
        .map_err(Error::relayer)?;

    for counterparty_connection in counterparty_connections.into_iter() {
        let (counterparty_connection_end, _) = counterparty_chain
            .query_connection(
                QueryConnectionRequest {
                    connection_id: counterparty_connection.clone(),
                    height: QueryHeight::Latest,
                },
                IncludeProof::No,
            )
            .map_err(Error::relayer)?;

        let local_connection_end = &counterparty_connection_end.counterparty();
        if let Some(local_connection_id) = local_connection_end.connection_id() {
            if local_connection_id == connection_id_on_source {
                return Ok(Some(counterparty_connection_end));
            }
        }
    }
    Ok(None)
}

pub fn connection_state_on_destination(
    connection: &IdentifiedConnectionEnd,
    counterparty_chain: &impl ChainHandle,
) -> Result<ConnectionState, Error> {
    if let Some(remote_connection_id) = connection.connection_end.counterparty().connection_id() {
        let (connection_end, _) = counterparty_chain
            .query_connection(
                QueryConnectionRequest {
                    connection_id: remote_connection_id.clone(),
                    height: QueryHeight::Latest,
                },
                IncludeProof::No,
            )
            .map_err(Error::relayer)?;

        Ok(connection_end.state)
    } else {
        // The remote connection id (used on `counterparty_chain`) is unknown.
        // Try to retrieve this id by looking at client connections.
        let counterparty_client_id = connection.connection_end.counterparty().client_id();

        let dst_connection = connection_on_destination(
            &connection.connection_id,
            counterparty_client_id,
            counterparty_chain,
        )?;

        dst_connection.map_or_else(
            || Ok(ConnectionState::Uninitialized),
            |remote_connection| Ok(remote_connection.state),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChannelConnectionClient {
    pub channel: IdentifiedChannelEnd,
    pub connection: IdentifiedConnectionEnd,
    pub client: IdentifiedAnyClientState,
}

impl ChannelConnectionClient {
    pub fn new(
        channel: IdentifiedChannelEnd,
        connection: IdentifiedConnectionEnd,
        client: IdentifiedAnyClientState,
    ) -> Self {
        Self {
            channel,
            connection,
            client,
        }
    }
}

/// Returns the [`ChannelConnectionClient`] associated with the
/// provided port and channel id.
pub fn channel_connection_client_no_checks(
    chain: &impl ChainHandle,
    port_id: &PortId,
    channel_id: &ChannelId,
) -> Result<ChannelConnectionClient, Error> {
    let (channel_end, _) = chain
        .query_channel(
            QueryChannelRequest {
                port_id: port_id.clone(),
                channel_id: channel_id.clone(),
                height: QueryHeight::Latest,
            },
            IncludeProof::No,
        )
        .map_err(Error::relayer)?;

    if channel_end.state_matches(&State::Uninitialized) {
        return Err(Error::channel_uninitialized(
            port_id.clone(),
            channel_id.clone(),
            chain.id(),
        ));
    }

    let connection_id = channel_end
        .connection_hops()
        .first()
        .ok_or_else(|| Error::missing_connection_hops(channel_id.clone(), chain.id()))?;

    let (connection_end, _) = chain
        .query_connection(
            QueryConnectionRequest {
                connection_id: connection_id.clone(),
                height: QueryHeight::Latest,
            },
            IncludeProof::No,
        )
        .map_err(Error::relayer)?;

    let client_id = connection_end.client_id();
    let (client_state, _) = chain
        .query_client_state(
            QueryClientStateRequest {
                client_id: client_id.clone(),
                height: QueryHeight::Latest,
            },
            IncludeProof::No,
        )
        .map_err(Error::relayer)?;

    let client = IdentifiedAnyClientState::new(client_id.clone(), client_state);
    let connection = IdentifiedConnectionEnd::new(connection_id.clone(), connection_end);
    let channel = IdentifiedChannelEnd::new(port_id.clone(), channel_id.clone(), channel_end);

    Ok(ChannelConnectionClient::new(channel, connection, client))
}

/// Returns the [`ChannelConnectionClient`] associated with the
/// provided port and channel id.
/// It checks that the connection is open.
pub fn channel_connection_client(
    chain: &impl ChainHandle,
    port_id: &PortId,
    channel_id: &ChannelId,
) -> Result<ChannelConnectionClient, Error> {
    let channel_connection_client =
        channel_connection_client_no_checks(chain, port_id, channel_id)?;

    if !channel_connection_client
        .connection
        .connection_end
        .is_open()
    {
        return Err(Error::connection_not_open(
            channel_connection_client.connection.connection_id,
            channel_id.clone(),
            chain.id(),
        ));
    }
    Ok(channel_connection_client)
}

pub fn counterparty_chain_from_channel(
    src_chain: &impl ChainHandle,
    src_channel_id: &ChannelId,
    src_port_id: &PortId,
) -> Result<ChainId, Error> {
    channel_connection_client(src_chain, src_port_id, src_channel_id)
        .map(|c| c.client.client_state.chain_id())
}

fn fetch_channel_on_destination(
    port_id: &PortId,
    channel_id: &ChannelId,
    counterparty_chain: &impl ChainHandle,
    remote_connection_id: &ConnectionId,
) -> Result<Option<IdentifiedChannelEnd>, Error> {
    let counterparty_channels = counterparty_chain
        .query_connection_channels(QueryConnectionChannelsRequest {
            connection_id: remote_connection_id.clone(),
            pagination: Some(PageRequest::all()),
        })
        .map_err(Error::relayer)?;

    for counterparty_channel in counterparty_channels.into_iter() {
        let local_channel_end = &counterparty_channel.channel_end.remote;
        if let Some(local_channel_id) = local_channel_end.channel_id() {
            if local_channel_id == channel_id && local_channel_end.port_id() == port_id {
                return Ok(Some(counterparty_channel));
            }
        }
    }
    Ok(None)
}

pub fn channel_state_on_destination(
    channel: &IdentifiedChannelEnd,
    connection: &IdentifiedConnectionEnd,
    counterparty_chain: &impl ChainHandle,
) -> Result<State, Error> {
    let remote_channel = channel_on_destination(channel, connection, counterparty_chain)?;
    Ok(remote_channel.map_or_else(
        || State::Uninitialized,
        |remote_channel| remote_channel.channel_end.state,
    ))
}

pub fn channel_on_destination(
    channel: &IdentifiedChannelEnd,
    connection: &IdentifiedConnectionEnd,
    counterparty_chain: &impl ChainHandle,
) -> Result<Option<IdentifiedChannelEnd>, Error> {
    if let Some(remote_channel_id) = channel.channel_end.counterparty().channel_id() {
        let counterparty = counterparty_chain
            .query_channel(
                QueryChannelRequest {
                    port_id: channel.channel_end.counterparty().port_id().clone(),
                    channel_id: remote_channel_id.clone(),
                    height: QueryHeight::Latest,
                },
                IncludeProof::No,
            )
            .map(|(c, _)| IdentifiedChannelEnd {
                port_id: channel.channel_end.counterparty().port_id().clone(),
                channel_id: remote_channel_id.clone(),
                channel_end: c,
            })
            .map_err(Error::relayer)?;

        Ok(Some(counterparty))
    } else if let Some(remote_connection_id) = connection.end().counterparty().connection_id() {
        fetch_channel_on_destination(
            &channel.port_id,
            &channel.channel_id,
            counterparty_chain,
            remote_connection_id,
        )
    } else {
        Ok(None)
    }
}

/// Queries a channel end on a [`ChainHandle`], and verifies
/// that the counterparty field on that channel end matches an
/// expected counterparty.
/// Returns `Ok` if the counterparty matches, and `Err` otherwise.
pub fn check_channel_counterparty(
    target_chain: impl ChainHandle,
    target_pchan: &PortChannelId,
    expected: &PortChannelId,
) -> Result<(), ChannelError> {
    let (channel_end_dst, _) = target_chain
        .query_channel(
            QueryChannelRequest {
                port_id: target_pchan.port_id.clone(),
                channel_id: target_pchan.channel_id.clone(),
                height: QueryHeight::Latest,
            },
            IncludeProof::No,
        )
        .map_err(|e| ChannelError::query(target_chain.id(), e))?;

    let counterparty = channel_end_dst.remote;
    match counterparty.channel_id {
        Some(actual_channel_id) => {
            let actual = PortChannelId {
                channel_id: actual_channel_id,
                port_id: counterparty.port_id,
            };
            if &actual != expected {
                return Err(ChannelError::mismatch_channel_ends(
                    target_chain.id(),
                    target_pchan.clone(),
                    expected.clone(),
                    actual,
                ));
            }
        }
        None => {
            error!(
                "channel {} on chain {} has no counterparty channel id ",
                target_pchan,
                target_chain.id()
            );
            return Err(ChannelError::incomplete_channel_state(
                target_chain.id(),
                target_pchan.clone(),
            ));
        }
    }

    Ok(())
}

/// Returns the sequences of the packet commitments on a given chain and channel (port_id + channel_id).
/// These are the sequences of the packets that were either:
///  - not yet received by the counterparty chain, or
///  - received on counterparty chain but not yet acknowledged by this chain,
pub fn commitments_on_chain(
    chain: &impl ChainHandle,
    port_id: &PortId,
    channel_id: &ChannelId,
) -> Result<(Vec<Sequence>, Height), Error> {
    // get the packet commitments on the counterparty/ source chain
    let (mut commit_sequences, response_height) = chain
        .query_packet_commitments(QueryPacketCommitmentsRequest {
            port_id: port_id.clone(),
            channel_id: channel_id.clone(),
            pagination: Some(PageRequest::all()),
        })
        .map_err(Error::relayer)?;

    commit_sequences.sort_unstable();

    Ok((commit_sequences, response_height))
}

/// Returns the sequences of the packets that were sent on the counterparty chain but for which
/// `MsgRecvPacket`-s have not been received on a given chain and channel (port_id + channel_id)
pub fn unreceived_packets_sequences(
    chain: &impl ChainHandle,
    port_id: &PortId,
    channel_id: &ChannelId,
    commitments_on_counterparty: Vec<Sequence>,
) -> Result<Vec<Sequence>, Error> {
    if commitments_on_counterparty.is_empty() {
        return Ok(vec![]);
    }

    chain
        .query_unreceived_packets(QueryUnreceivedPacketsRequest {
            port_id: port_id.clone(),
            channel_id: channel_id.clone(),
            packet_commitment_sequences: commitments_on_counterparty,
        })
        .map_err(Error::relayer)
}

/// Returns the sequences of the written acknowledgments on a given chain and channel (port_id + channel_id), out of
/// the commitments still present on the counterparty chain.
pub fn packet_acknowledgements(
    chain: &impl ChainHandle,
    port_id: &PortId,
    channel_id: &ChannelId,
    commit_sequences: Vec<Sequence>,
) -> Result<Option<(Vec<Sequence>, Height)>, Error> {
    // If there aren't any sequences to query for, return early.
    // Otherwise we end up with the full list of acknowledgements on chain,
    // which is potentially huge and extremely costly.
    if commit_sequences.is_empty() {
        return Ok(None);
    }

    let commit_set = commit_sequences.iter().cloned().collect::<HashSet<_>>();

    // Get the packet acknowledgments on counterparty/source chain
    let (mut acked_sequences, response_height) = chain
        .query_packet_acknowledgements(QueryPacketAcknowledgementsRequest {
            port_id: port_id.clone(),
            channel_id: channel_id.clone(),
            pagination: Some(PageRequest::all()),
            packet_commitment_sequences: commit_sequences,
        })
        .map_err(Error::relayer)?;

    acked_sequences.retain(|s| commit_set.contains(s));
    acked_sequences.sort_unstable();

    Ok(Some((acked_sequences, response_height)))
}

/// Returns the sequences of the packets that were sent on the chain and for which:
///  - `MsgRecvPacket`-s have been received on the counterparty chain but
///  - `MsgAcknowledgement`-s have NOT been received by the chain
pub fn unreceived_acknowledgements_sequences(
    chain: &impl ChainHandle,
    port_id: &PortId,
    channel_id: &ChannelId,
    acks_on_counterparty: Vec<Sequence>,
) -> Result<Vec<Sequence>, Error> {
    if acks_on_counterparty.is_empty() {
        return Ok(vec![]);
    }

    chain
        .query_unreceived_acknowledgements(QueryUnreceivedAcksRequest {
            port_id: port_id.clone(),
            channel_id: channel_id.clone(),
            packet_ack_sequences: acks_on_counterparty,
        })
        .map_err(Error::relayer)
}

/// Given a channel, this method returns:
/// - The sequences of the packets _sent_ on the counterparty chain and not _received_ by
///   the (target) chain.
/// - The counterparty height at which the query was made.
///
/// Expects an [`IdentifiedChannelEnd`] and a pair of [`ChainHandle`]s representing the chains
/// at the two ends of this channel, called a (target) chain and a counterparty chain.
///
/// ### Implementation details
/// This method involves two separate queries:
///
/// 1. It performs a [`QueryPacketCommitmentsRequest`] on the counterparty chain.
///     This query returns the sequences for the packets with stored
///     commitments in the counterparty chain's state, and the height at which the query was made
///
///     This step relies on [`commitments_on_chain`], see that method for more details.
///
/// 2. It performs a [`QueryUnreceivedPacketsRequest`] on the (target) chain.
///     Given the sequences of packet commitments on the counterparty (query #1),
///     this query returns the sequences of the packets which the target
///     chain has not yet _received_.
///
///    This step relies on [`unreceived_packets_sequences`], see that method for more details.
///
pub fn unreceived_packets(
    chain: &impl ChainHandle,
    counterparty_chain: &impl ChainHandle,
    path: &PathIdentifiers,
) -> Result<(Vec<Sequence>, Height), Error> {
    let (commit_sequences, h) = commitments_on_chain(
        counterparty_chain,
        &path.counterparty_port_id,
        &path.counterparty_channel_id,
    )?;

    let packet_seq_nrs =
        unreceived_packets_sequences(chain, &path.port_id, &path.channel_id, commit_sequences)?;

    Ok((packet_seq_nrs, h))
}

pub fn acknowledgements_on_chain(
    chain: &impl ChainHandle,
    counterparty_chain: &impl ChainHandle,
    channel: &IdentifiedChannelEnd,
) -> Result<Option<(Vec<Sequence>, Height)>, Error> {
    let counterparty = channel.channel_end.counterparty();
    let counterparty_channel_id = counterparty
        .channel_id
        .as_ref()
        .ok_or_else(Error::missing_counterparty_channel_id)?;

    let (commitments_on_counterparty, _) = commitments_on_chain(
        counterparty_chain,
        &counterparty.port_id,
        counterparty_channel_id,
    )?;

    let sequences_and_height = packet_acknowledgements(
        chain,
        &channel.port_id,
        &channel.channel_id,
        commitments_on_counterparty,
    )?;

    Ok(sequences_and_height)
}

/// Given a channel, this method returns:
/// - The sequences of all packets _received on the counterparty chain and not _acknowledged_ by
///   the (target) chain.
/// - The counterparty height at which the query was made.
///
/// Expects an [`IdentifiedChannelEnd`] and a pair of [`ChainHandle`]s representing the chains
/// at the two ends of this channel, called a (target) chain and a counterparty chain.
///
/// ### Implementation details
/// This method involves two separate queries:
///
/// 1. It performs a [`QueryPacketCommitmentsRequest`] on the target chain.
///     This query returns the sequences for the packets with stored
///     commitments in the target chain's state, and the height at which the query was made
///
///     This step relies on [`commitments_on_chain`], see that method for more details.
///
/// 2. It performs a [`QueryPacketAcknowledgementsRequest`] on the counterparty chain.
///     Given the sequences of packet commitments on the target chain (query #1),
///     this query returns the sequences of the packets which the counterparty chain has
///     _acknowledged_.
///
///     This step relies on [`packet_acknowledgements`], see that method for more details.
///
/// 3. It performs a [`QueryUnreceivedAcksRequest`] on the target chain.
///     Given the sequences of packet acknowledgements on the counterparty (step #2),
///     this query fetches the subset for which acknowledgements have not been
///     received by the target chain.
///     This step relies on [`unreceived_acknowledgements_sequences`].
pub fn unreceived_acknowledgements(
    chain: &impl ChainHandle,
    counterparty_chain: &impl ChainHandle,
    path: &PathIdentifiers,
) -> Result<Option<(Vec<Sequence>, Height)>, Error> {
    let (commitments_on_src, _) = commitments_on_chain(chain, &path.port_id, &path.channel_id)?;

    let acks_and_height_on_counterparty = packet_acknowledgements(
        counterparty_chain,
        &path.counterparty_port_id,
        &path.counterparty_channel_id,
        commitments_on_src,
    )?;

    if let Some((acks_on_counterparty, height)) = acks_and_height_on_counterparty {
        let unreceived_acks = unreceived_acknowledgements_sequences(
            chain,
            &path.port_id,
            &path.channel_id,
            acks_on_counterparty,
        )?;

        Ok(Some((unreceived_acks, height)))
    } else {
        Ok(None)
    }
}

/// A structure to display pending packet commitment IDs
/// at one end of a channel.
#[derive(Debug, Serialize)]
pub struct PendingPackets {
    /// Not yet received on the counterparty chain.
    pub unreceived_packets: Vec<Sequence>,
    /// Received on the counterparty chain,
    /// but the acknowledgement is not yet received on the local chain.
    pub unreceived_acks: Vec<Sequence>,
}

pub fn pending_packet_summary(
    chain: &impl ChainHandle,
    counterparty_chain: &impl ChainHandle,
    channel: &IdentifiedChannelEnd,
) -> Result<PendingPackets, Error> {
    let counterparty = channel.channel_end.counterparty();
    let counterparty_channel_id = counterparty
        .channel_id
        .as_ref()
        .ok_or_else(Error::missing_counterparty_channel_id)?;

    let (commitments_on_src, _) =
        commitments_on_chain(chain, &channel.port_id, &channel.channel_id)?;

    let unreceived = unreceived_packets_sequences(
        counterparty_chain,
        &counterparty.port_id,
        counterparty_channel_id,
        commitments_on_src.clone(),
    )?;

    let acks_on_counterparty = packet_acknowledgements(
        counterparty_chain,
        &counterparty.port_id,
        counterparty_channel_id,
        commitments_on_src,
    )?;

    let pending_acks = if let Some((acks_on_counterparty, _)) = acks_on_counterparty {
        unreceived_acknowledgements_sequences(
            chain,
            &channel.port_id,
            &channel.channel_id,
            acks_on_counterparty,
        )?
    } else {
        Vec::new()
    };

    Ok(PendingPackets {
        unreceived_packets: unreceived,
        unreceived_acks: pending_acks,
    })
}
