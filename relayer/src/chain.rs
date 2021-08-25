use alloc::sync::Arc;
use prost_types::Any;
use tendermint::block::Height;
use tokio::runtime::Runtime as TokioRuntime;

pub use cosmos::CosmosSdkChain;

use ibc::events::IbcEvent;
use ibc::ics02_client::client_consensus::{
    AnyConsensusState, AnyConsensusStateWithHeight, ConsensusState,
};
use ibc::ics02_client::client_state::{AnyClientState, ClientState, IdentifiedAnyClientState};
use ibc::ics02_client::header::Header;
use ibc::ics03_connection::connection::{ConnectionEnd, IdentifiedConnectionEnd, State};
use ibc::ics03_connection::version::{get_compatible_versions, Version};
use ibc::ics04_channel::channel::{ChannelEnd, IdentifiedChannelEnd};
use ibc::ics04_channel::packet::{PacketMsgType, Sequence};
use ibc::ics23_commitment::commitment::{CommitmentPrefix, CommitmentProofBytes};
use ibc::ics24_host::identifier::{ChainId, ChannelId, ClientId, ConnectionId, PortId};
use ibc::proofs::{ConsensusProof, Proofs};
use ibc::query::QueryTxRequest;
use ibc::signer::Signer;
use ibc::Height as ICSHeight;
use ibc_proto::ibc::core::channel::v1::{
    PacketState, QueryChannelClientStateRequest, QueryChannelsRequest,
    QueryConnectionChannelsRequest, QueryNextSequenceReceiveRequest,
    QueryPacketAcknowledgementsRequest, QueryPacketCommitmentsRequest, QueryUnreceivedAcksRequest,
    QueryUnreceivedPacketsRequest,
};
use ibc_proto::ibc::core::client::v1::{QueryClientStatesRequest, QueryConsensusStatesRequest};
use ibc_proto::ibc::core::commitment::v1::MerkleProof;
use ibc_proto::ibc::core::connection::v1::{
    QueryClientConnectionsRequest, QueryConnectionsRequest,
};
use tendermint_rpc::endpoint::broadcast::tx_sync::Response as TxResponse;

use crate::connection::ConnectionMsgType;
use crate::error::Error;
use crate::event::monitor::TxMonitorCmd;
use crate::keyring::{KeyEntry, KeyRing};
use crate::light_client::LightClient;
use crate::{config::ChainConfig, event::monitor::EventReceiver};

pub(crate) mod cosmos;
pub mod counterparty;
pub mod handle;
pub mod runtime;

#[cfg(test)]
pub mod mock;

/// Generic query response type
/// TODO - will slowly move to GRPC protobuf specs for queries
#[derive(Clone, Debug, PartialEq)]
pub struct QueryResponse {
    pub value: Vec<u8>,
    pub proof: Option<MerkleProof>,
    pub height: Height,
}

/// Packet query options
#[derive(Debug)]
pub struct QueryPacketOptions {
    pub port_id: PortId,
    pub channel_id: ChannelId,
    pub height: u64,
}

/// Defines a blockchain as understood by the relayer
pub trait ChainEndpoint: Sized {
    /// Type of light blocks for this chain
    type LightBlock: Send + Sync;

    /// Type of headers for this chain
    type Header: Header;

    /// Type of consensus state for this chain
    type ConsensusState: ConsensusState;

    /// Type of the client state for this chain
    type ClientState: ClientState;

    type LightClient: LightClient<Self>;

    /// Constructs the chain
    fn bootstrap(config: ChainConfig, rt: Arc<TokioRuntime>) -> Result<Self, Error>;

    #[allow(clippy::type_complexity)]
    /// Initializes and returns the light client (if any) associated with this chain.
    fn init_light_client(&self) -> Result<Self::LightClient, Error>;

    /// Initializes and returns the event monitor (if any) associated with this chain.
    fn init_event_monitor(
        &self,
        rt: Arc<TokioRuntime>,
    ) -> Result<(EventReceiver, TxMonitorCmd), Error>;

    fn shutdown(self) -> Result<(), Error>;

    /// Returns the chain's identifier
    fn id(&self) -> &ChainId;

    /// Returns the chain's keybase
    fn keybase(&self) -> &KeyRing;

    /// Returns the chain's keybase, mutably
    fn keybase_mut(&mut self) -> &mut KeyRing;

    /// Sends one or more transactions with `msgs` to chain and
    // synchronously wait for it to be committed.
    fn send_messages_and_wait_commit(
        &mut self,
        proto_msgs: Vec<Any>,
    ) -> Result<Vec<IbcEvent>, Error>;

    /// Sends one or more transactions with `msgs` to chain.
    /// Non-blocking alternative to `send_messages_and_wait_commit` interface.
    fn send_messages_and_wait_check_tx(
        &mut self,
        proto_msgs: Vec<Any>,
    ) -> Result<Vec<TxResponse>, Error>;

    fn get_signer(&mut self) -> Result<Signer, Error>;

    fn get_key(&mut self) -> Result<KeyEntry, Error>;

    // Queries

    fn query_commitment_prefix(&self) -> Result<CommitmentPrefix, Error>;

    fn query_compatible_versions(&self) -> Result<Vec<Version>, Error> {
        // TODO - do a real chain query
        Ok(get_compatible_versions())
    }

    /// Query the latest height the chain is at
    fn query_latest_height(&self) -> Result<ICSHeight, Error>;

    /// Performs a query to retrieve the state of all clients that a chain hosts.
    fn query_clients(
        &self,
        request: QueryClientStatesRequest,
    ) -> Result<Vec<IdentifiedAnyClientState>, Error>;

    fn query_client_state(
        &self,
        client_id: &ClientId,
        height: ICSHeight,
    ) -> Result<Self::ClientState, Error>;

    fn query_consensus_states(
        &self,
        request: QueryConsensusStatesRequest,
    ) -> Result<Vec<AnyConsensusStateWithHeight>, Error>;

    /// Performs a query to retrieve the consensus state (for a specific height `consensus_height`)
    /// that an on-chain client stores.
    fn query_consensus_state(
        &self,
        client_id: ClientId,
        consensus_height: ICSHeight,
        query_height: ICSHeight,
    ) -> Result<AnyConsensusState, Error>;

    fn query_upgraded_client_state(
        &self,
        height: ICSHeight,
    ) -> Result<(Self::ClientState, MerkleProof), Error>;

    fn query_upgraded_consensus_state(
        &self,
        height: ICSHeight,
    ) -> Result<(Self::ConsensusState, MerkleProof), Error>;

    /// Performs a query to retrieve the identifiers of all connections.
    fn query_connections(
        &self,
        request: QueryConnectionsRequest,
    ) -> Result<Vec<IdentifiedConnectionEnd>, Error>;

    /// Performs a query to retrieve the identifiers of all connections.
    fn query_client_connections(
        &self,
        request: QueryClientConnectionsRequest,
    ) -> Result<Vec<ConnectionId>, Error>;

    fn query_connection(
        &self,
        connection_id: &ConnectionId,
        height: ICSHeight,
    ) -> Result<ConnectionEnd, Error>;

    /// Performs a query to retrieve the identifiers of all channels associated with a connection.
    fn query_connection_channels(
        &self,
        request: QueryConnectionChannelsRequest,
    ) -> Result<Vec<IdentifiedChannelEnd>, Error>;

    /// Performs a query to retrieve the identifiers of all channels.
    fn query_channels(
        &self,
        request: QueryChannelsRequest,
    ) -> Result<Vec<IdentifiedChannelEnd>, Error>;

    fn query_channel(
        &self,
        port_id: &PortId,
        channel_id: &ChannelId,
        height: ICSHeight,
    ) -> Result<ChannelEnd, Error>;

    // TODO: Introduce a newtype for the module version string
    fn query_module_version(&self, port_id: &PortId) -> String {
        // TODO - query the chain, currently hardcoded
        if port_id.as_str() == "transfer" {
            "ics20-1".to_string()
        } else if port_id.as_str() == "ibcaccount" {
            "ics27-1".to_string()
        } else {
            "".to_string()
        }
    }

    fn query_channel_client_state(
        &self,
        request: QueryChannelClientStateRequest,
    ) -> Result<Option<IdentifiedAnyClientState>, Error>;

    fn query_packet_commitments(
        &self,
        request: QueryPacketCommitmentsRequest,
    ) -> Result<(Vec<PacketState>, ICSHeight), Error>;

    fn query_unreceived_packets(
        &self,
        request: QueryUnreceivedPacketsRequest,
    ) -> Result<Vec<u64>, Error>;

    fn query_packet_acknowledgements(
        &self,
        request: QueryPacketAcknowledgementsRequest,
    ) -> Result<(Vec<PacketState>, ICSHeight), Error>;

    fn query_unreceived_acknowledgements(
        &self,
        request: QueryUnreceivedAcksRequest,
    ) -> Result<Vec<u64>, Error>;

    fn query_next_sequence_receive(
        &self,
        request: QueryNextSequenceReceiveRequest,
    ) -> Result<Sequence, Error>;

    fn query_txs(&self, request: QueryTxRequest) -> Result<Vec<IbcEvent>, Error>;

    // Provable queries
    fn proven_client_state(
        &self,
        client_id: &ClientId,
        height: ICSHeight,
    ) -> Result<(Self::ClientState, MerkleProof), Error>;

    fn proven_connection(
        &self,
        connection_id: &ConnectionId,
        height: ICSHeight,
    ) -> Result<(ConnectionEnd, MerkleProof), Error>;

    fn proven_client_consensus(
        &self,
        client_id: &ClientId,
        consensus_height: ICSHeight,
        height: ICSHeight,
    ) -> Result<(Self::ConsensusState, MerkleProof), Error>;

    fn proven_channel(
        &self,
        port_id: &PortId,
        channel_id: &ChannelId,
        height: ICSHeight,
    ) -> Result<(ChannelEnd, MerkleProof), Error>;

    fn proven_packet(
        &self,
        packet_type: PacketMsgType,
        port_id: PortId,
        channel_id: ChannelId,
        sequence: Sequence,
        height: ICSHeight,
    ) -> Result<(Vec<u8>, MerkleProof), Error>;

    fn build_client_state(&self, height: ICSHeight) -> Result<Self::ClientState, Error>;

    fn build_consensus_state(
        &self,
        light_block: Self::LightBlock,
    ) -> Result<Self::ConsensusState, Error>;

    /// Fetch, and verify the header at `target_height`, assuming we trust the
    /// header at `trusted_height` with the given `client_state`.
    ///
    /// Returns all the supporting headers that were need to verify the target
    /// header, for use when building a `ClientUpdate` message.
    fn build_header(
        &self,
        trusted_height: ICSHeight,
        target_height: ICSHeight,
        client_state: &AnyClientState,
        light_client: &mut Self::LightClient,
    ) -> Result<(Self::Header, Vec<Self::Header>), Error>;

    /// Builds the required proofs and the client state for connection handshake messages.
    /// The proofs and client state must be obtained from queries at same height.
    fn build_connection_proofs_and_client_state(
        &self,
        message_type: ConnectionMsgType,
        connection_id: &ConnectionId,
        client_id: &ClientId,
        height: ICSHeight,
    ) -> Result<(Option<Self::ClientState>, Proofs), Error> {
        let (connection_end, connection_proof) = self.proven_connection(connection_id, height)?;

        // Check that the connection state is compatible with the message
        match message_type {
            ConnectionMsgType::OpenTry => {
                if !connection_end.state_matches(&State::Init)
                    && !connection_end.state_matches(&State::TryOpen)
                {
                    return Err(Error::bad_connection_state());
                }
            }
            ConnectionMsgType::OpenAck => {
                if !connection_end.state_matches(&State::TryOpen)
                    && !connection_end.state_matches(&State::Open)
                {
                    return Err(Error::bad_connection_state());
                }
            }
            ConnectionMsgType::OpenConfirm => {
                if !connection_end.state_matches(&State::Open) {
                    return Err(Error::bad_connection_state());
                }
            }
        }

        let mut client_state = None;
        let mut client_proof = None;
        let mut consensus_proof = None;

        match message_type {
            ConnectionMsgType::OpenTry | ConnectionMsgType::OpenAck => {
                let (client_state_value, client_state_proof) =
                    self.proven_client_state(client_id, height)?;

                client_proof = Some(CommitmentProofBytes::from(client_state_proof));

                let consensus_state_proof = self
                    .proven_client_consensus(client_id, client_state_value.latest_height(), height)?
                    .1;

                consensus_proof = Option::from(
                    ConsensusProof::new(
                        CommitmentProofBytes::from(consensus_state_proof),
                        client_state_value.latest_height(),
                    )
                    .map_err(Error::consensus_proof)?,
                );

                client_state = Some(client_state_value);
            }
            _ => {}
        }

        Ok((
            client_state,
            Proofs::new(
                CommitmentProofBytes::from(connection_proof),
                client_proof,
                consensus_proof,
                None,
                height.increment(),
            )
            .map_err(Error::malformed_proof)?,
        ))
    }

    /// Builds the proof for channel handshake messages.
    fn build_channel_proofs(
        &self,
        port_id: &PortId,
        channel_id: &ChannelId,
        height: ICSHeight,
    ) -> Result<Proofs, Error> {
        // Collect all proofs as required
        let channel_proof =
            CommitmentProofBytes::from(self.proven_channel(port_id, channel_id, height)?.1);

        Proofs::new(channel_proof, None, None, None, height.increment())
            .map_err(Error::malformed_proof)
    }

    /// Builds the proof for packet messages.
    fn build_packet_proofs(
        &self,
        packet_type: PacketMsgType,
        port_id: PortId,
        channel_id: ChannelId,
        sequence: Sequence,
        height: ICSHeight,
    ) -> Result<(Vec<u8>, Proofs), Error> {
        let channel_proof = if packet_type == PacketMsgType::TimeoutOnClose {
            Some(CommitmentProofBytes::from(
                self.proven_channel(&port_id, &channel_id, height)?.1,
            ))
        } else {
            None
        };

        let (bytes, packet_proof) =
            self.proven_packet(packet_type, port_id, channel_id, sequence, height)?;

        let proofs = Proofs::new(
            CommitmentProofBytes::from(packet_proof),
            None,
            None,
            channel_proof,
            height.increment(),
        )
        .map_err(Error::malformed_proof)?;

        Ok((bytes, proofs))
    }
}
