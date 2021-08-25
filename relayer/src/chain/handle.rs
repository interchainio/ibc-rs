use alloc::sync::Arc;
use core::fmt::{self, Debug};

use crossbeam_channel as channel;
use ibc::ics03_connection::connection::IdentifiedConnectionEnd;
use ibc_proto::ibc::core::connection::v1::QueryConnectionsRequest;
use serde::Serialize;

use ibc::{
    events::IbcEvent,
    ics02_client::{
        client_consensus::{AnyConsensusState, AnyConsensusStateWithHeight},
        client_state::{AnyClientState, IdentifiedAnyClientState},
        events::UpdateClient,
        header::AnyHeader,
        misbehaviour::MisbehaviourEvidence,
    },
    ics03_connection::{connection::ConnectionEnd, version::Version},
    ics04_channel::{
        channel::{ChannelEnd, IdentifiedChannelEnd},
        packet::{PacketMsgType, Sequence},
    },
    ics23_commitment::commitment::CommitmentPrefix,
    ics24_host::identifier::{ChainId, ChannelId, ClientId, ConnectionId, PortId},
    proofs::Proofs,
    query::QueryTxRequest,
    signer::Signer,
    Height,
};

use ibc_proto::ibc::core::{
    channel::v1::{
        PacketState, QueryChannelClientStateRequest, QueryChannelsRequest,
        QueryConnectionChannelsRequest, QueryNextSequenceReceiveRequest,
        QueryPacketAcknowledgementsRequest, QueryPacketCommitmentsRequest,
        QueryUnreceivedAcksRequest, QueryUnreceivedPacketsRequest,
    },
    client::v1::{QueryClientStatesRequest, QueryConsensusStatesRequest},
    commitment::v1::MerkleProof,
    connection::v1::QueryClientConnectionsRequest,
};

pub use prod::ProdChainHandle;

use crate::{
    connection::ConnectionMsgType,
    error::Error,
    event::monitor::{EventBatch, Result as MonitorResult},
    keyring::KeyEntry,
};

mod prod;

/// A pair of [`ChainHandle`]s.
#[derive(Clone)]
pub struct ChainHandlePair<ChainA: ChainHandle, ChainB: ChainHandle> {
    pub a: ChainA,
    pub b: ChainB,
}

impl<ChainA: ChainHandle, ChainB: ChainHandle> ChainHandlePair<ChainA, ChainB> {
    /// Swap the two handles.
    pub fn swap(self) -> ChainHandlePair<ChainB, ChainA> {
        ChainHandlePair {
            a: self.b,
            b: self.a,
        }
    }
}

impl<ChainA: ChainHandle, ChainB: ChainHandle> Debug for ChainHandlePair<ChainA, ChainB> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChainHandlePair")
            .field("a", &self.a.id())
            .field("b", &self.b.id())
            .finish()
    }
}

pub type Subscription = channel::Receiver<Arc<MonitorResult<EventBatch>>>;

pub type ReplyTo<T> = channel::Sender<Result<T, Error>>;
pub type Reply<T> = channel::Receiver<Result<T, Error>>;

pub fn reply_channel<T>() -> (ReplyTo<T>, Reply<T>) {
    channel::bounded(1)
}

/// Requests that a `ChainHandle` may send to a `ChainRuntime`.
#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum ChainRequest {
    Shutdown {
        reply_to: ReplyTo<()>,
    },

    Subscribe {
        reply_to: ReplyTo<Subscription>,
    },

    SendMessagesAndWaitCommit {
        proto_msgs: Vec<prost_types::Any>,
        reply_to: ReplyTo<Vec<IbcEvent>>,
    },

    SendMessagesAndWaitCheckTx {
        proto_msgs: Vec<prost_types::Any>,
        reply_to: ReplyTo<Vec<tendermint_rpc::endpoint::broadcast::tx_sync::Response>>,
    },

    Signer {
        reply_to: ReplyTo<Signer>,
    },

    Key {
        reply_to: ReplyTo<KeyEntry>,
    },

    ModuleVersion {
        port_id: PortId,
        reply_to: ReplyTo<String>,
    },

    QueryLatestHeight {
        reply_to: ReplyTo<Height>,
    },

    QueryClients {
        request: QueryClientStatesRequest,
        reply_to: ReplyTo<Vec<IdentifiedAnyClientState>>,
    },

    BuildHeader {
        trusted_height: Height,
        target_height: Height,
        client_state: AnyClientState,
        reply_to: ReplyTo<(AnyHeader, Vec<AnyHeader>)>,
    },

    BuildClientState {
        height: Height,
        reply_to: ReplyTo<AnyClientState>,
    },

    BuildConsensusState {
        trusted: Height,
        target: Height,
        client_state: AnyClientState,
        reply_to: ReplyTo<AnyConsensusState>,
    },

    BuildMisbehaviour {
        client_state: AnyClientState,
        update_event: UpdateClient,
        reply_to: ReplyTo<Option<MisbehaviourEvidence>>,
    },

    BuildConnectionProofsAndClientState {
        message_type: ConnectionMsgType,
        connection_id: ConnectionId,
        client_id: ClientId,
        height: Height,
        reply_to: ReplyTo<(Option<AnyClientState>, Proofs)>,
    },

    QueryClientState {
        client_id: ClientId,
        height: Height,
        reply_to: ReplyTo<AnyClientState>,
    },

    QueryClientConnections {
        request: QueryClientConnectionsRequest,
        reply_to: ReplyTo<Vec<ConnectionId>>,
    },

    QueryConsensusStates {
        request: QueryConsensusStatesRequest,
        reply_to: ReplyTo<Vec<AnyConsensusStateWithHeight>>,
    },

    QueryConsensusState {
        client_id: ClientId,
        consensus_height: Height,
        query_height: Height,
        reply_to: ReplyTo<AnyConsensusState>,
    },

    QueryUpgradedClientState {
        height: Height,
        reply_to: ReplyTo<(AnyClientState, MerkleProof)>,
    },

    QueryUpgradedConsensusState {
        height: Height,
        reply_to: ReplyTo<(AnyConsensusState, MerkleProof)>,
    },

    QueryCommitmentPrefix {
        reply_to: ReplyTo<CommitmentPrefix>,
    },

    QueryCompatibleVersions {
        reply_to: ReplyTo<Vec<Version>>,
    },

    QueryConnection {
        connection_id: ConnectionId,
        height: Height,
        reply_to: ReplyTo<ConnectionEnd>,
    },

    QueryConnections {
        request: QueryConnectionsRequest,
        reply_to: ReplyTo<Vec<IdentifiedConnectionEnd>>,
    },

    QueryConnectionChannels {
        request: QueryConnectionChannelsRequest,
        reply_to: ReplyTo<Vec<IdentifiedChannelEnd>>,
    },

    QueryChannels {
        request: QueryChannelsRequest,
        reply_to: ReplyTo<Vec<IdentifiedChannelEnd>>,
    },

    QueryChannel {
        port_id: PortId,
        channel_id: ChannelId,
        height: Height,
        reply_to: ReplyTo<ChannelEnd>,
    },

    QueryChannelClientState {
        request: QueryChannelClientStateRequest,
        reply_to: ReplyTo<Option<IdentifiedAnyClientState>>,
    },

    QueryNextSequenceReceive {
        request: QueryNextSequenceReceiveRequest,
        reply_to: ReplyTo<Sequence>,
    },

    ProvenClientState {
        client_id: ClientId,
        height: Height,
        reply_to: ReplyTo<(AnyClientState, MerkleProof)>,
    },

    ProvenConnection {
        connection_id: ConnectionId,
        height: Height,
        reply_to: ReplyTo<(ConnectionEnd, MerkleProof)>,
    },

    ProvenClientConsensus {
        client_id: ClientId,
        consensus_height: Height,
        height: Height,
        reply_to: ReplyTo<(AnyConsensusState, MerkleProof)>,
    },

    BuildChannelProofs {
        port_id: PortId,
        channel_id: ChannelId,
        height: Height,
        reply_to: ReplyTo<Proofs>,
    },

    BuildPacketProofs {
        packet_type: PacketMsgType,
        port_id: PortId,
        channel_id: ChannelId,
        sequence: Sequence,
        height: Height,
        reply_to: ReplyTo<(Vec<u8>, Proofs)>,
    },

    QueryPacketCommitments {
        request: QueryPacketCommitmentsRequest,
        reply_to: ReplyTo<(Vec<PacketState>, Height)>,
    },

    QueryUnreceivedPackets {
        request: QueryUnreceivedPacketsRequest,
        reply_to: ReplyTo<Vec<u64>>,
    },

    QueryPacketAcknowledgement {
        request: QueryPacketAcknowledgementsRequest,
        reply_to: ReplyTo<(Vec<PacketState>, Height)>,
    },

    QueryUnreceivedAcknowledgement {
        request: QueryUnreceivedAcksRequest,
        reply_to: ReplyTo<Vec<u64>>,
    },

    QueryPacketEventData {
        request: QueryTxRequest,
        reply_to: ReplyTo<Vec<IbcEvent>>,
    },
}

pub trait ChainHandle: Clone + Send + Sync + Serialize + Debug {
    fn new(chain_id: ChainId, sender: channel::Sender<ChainRequest>) -> Self;

    /// Get the [`ChainId`] of this chain.
    fn id(&self) -> ChainId;

    /// Shutdown the chain runtime.
    fn shutdown(&self) -> Result<(), Error>;

    /// Subscribe to the events emitted by the chain.
    fn subscribe(&self) -> Result<Subscription, Error>;

    /// Send the given `msgs` to the chain, packaged as one or more transactions,
    /// and return the list of events emitted by the chain after the transaction was committed.
    fn send_messages_and_wait_commit(
        &self,
        proto_msgs: Vec<prost_types::Any>,
    ) -> Result<Vec<IbcEvent>, Error>;

    /// Submit messages asynchronously.
    /// Does not block waiting on the chain to produce the
    /// resulting events. Instead of events, this method
    /// returns a set of transaction hashes.
    fn send_messages_and_wait_check_tx(
        &self,
        proto_msgs: Vec<prost_types::Any>,
    ) -> Result<Vec<tendermint_rpc::endpoint::broadcast::tx_sync::Response>, Error>;

    fn get_signer(&self) -> Result<Signer, Error>;

    fn get_key(&self) -> Result<KeyEntry, Error>;

    fn module_version(&self, port_id: &PortId) -> Result<String, Error>;

    fn query_latest_height(&self) -> Result<Height, Error>;

    fn query_clients(
        &self,
        request: QueryClientStatesRequest,
    ) -> Result<Vec<IdentifiedAnyClientState>, Error>;

    fn query_client_state(
        &self,
        client_id: &ClientId,
        height: Height,
    ) -> Result<AnyClientState, Error>;

    fn query_client_connections(
        &self,
        request: QueryClientConnectionsRequest,
    ) -> Result<Vec<ConnectionId>, Error>;

    fn query_consensus_states(
        &self,
        request: QueryConsensusStatesRequest,
    ) -> Result<Vec<AnyConsensusStateWithHeight>, Error>;

    fn query_consensus_state(
        &self,
        client_id: ClientId,
        consensus_height: Height,
        query_height: Height,
    ) -> Result<AnyConsensusState, Error>;

    fn query_upgraded_client_state(
        &self,
        height: Height,
    ) -> Result<(AnyClientState, MerkleProof), Error>;

    fn query_upgraded_consensus_state(
        &self,
        height: Height,
    ) -> Result<(AnyConsensusState, MerkleProof), Error>;

    fn query_commitment_prefix(&self) -> Result<CommitmentPrefix, Error>;

    fn query_compatible_versions(&self) -> Result<Vec<Version>, Error>;

    fn query_connection(
        &self,
        connection_id: &ConnectionId,
        height: Height,
    ) -> Result<ConnectionEnd, Error>;

    fn query_connections(
        &self,
        request: QueryConnectionsRequest,
    ) -> Result<Vec<IdentifiedConnectionEnd>, Error>;

    fn query_connection_channels(
        &self,
        request: QueryConnectionChannelsRequest,
    ) -> Result<Vec<IdentifiedChannelEnd>, Error>;

    fn query_next_sequence_receive(
        &self,
        request: QueryNextSequenceReceiveRequest,
    ) -> Result<Sequence, Error>;

    fn query_channels(
        &self,
        request: QueryChannelsRequest,
    ) -> Result<Vec<IdentifiedChannelEnd>, Error>;

    fn query_channel(
        &self,
        port_id: &PortId,
        channel_id: &ChannelId,
        height: Height,
    ) -> Result<ChannelEnd, Error>;

    fn query_channel_client_state(
        &self,
        request: QueryChannelClientStateRequest,
    ) -> Result<Option<IdentifiedAnyClientState>, Error>;

    fn proven_client_state(
        &self,
        client_id: &ClientId,
        height: Height,
    ) -> Result<(AnyClientState, MerkleProof), Error>;

    fn proven_connection(
        &self,
        connection_id: &ConnectionId,
        height: Height,
    ) -> Result<(ConnectionEnd, MerkleProof), Error>;

    fn proven_client_consensus(
        &self,
        client_id: &ClientId,
        consensus_height: Height,
        height: Height,
    ) -> Result<(AnyConsensusState, MerkleProof), Error>;

    fn build_header(
        &self,
        trusted_height: Height,
        target_height: Height,
        client_state: AnyClientState,
    ) -> Result<(AnyHeader, Vec<AnyHeader>), Error>;

    /// Constructs a client state at the given height
    fn build_client_state(&self, height: Height) -> Result<AnyClientState, Error>;

    /// Constructs a consensus state at the given height
    fn build_consensus_state(
        &self,
        trusted: Height,
        target: Height,
        client_state: AnyClientState,
    ) -> Result<AnyConsensusState, Error>;

    fn check_misbehaviour(
        &self,
        update: UpdateClient,
        client_state: AnyClientState,
    ) -> Result<Option<MisbehaviourEvidence>, Error>;

    fn build_connection_proofs_and_client_state(
        &self,
        message_type: ConnectionMsgType,
        connection_id: &ConnectionId,
        client_id: &ClientId,
        height: Height,
    ) -> Result<(Option<AnyClientState>, Proofs), Error>;

    fn build_channel_proofs(
        &self,
        port_id: &PortId,
        channel_id: &ChannelId,
        height: Height,
    ) -> Result<Proofs, Error>;

    fn build_packet_proofs(
        &self,
        packet_type: PacketMsgType,
        port_id: &PortId,
        channel_id: &ChannelId,
        sequence: Sequence,
        height: Height,
    ) -> Result<(Vec<u8>, Proofs), Error>;

    fn query_packet_commitments(
        &self,
        request: QueryPacketCommitmentsRequest,
    ) -> Result<(Vec<PacketState>, Height), Error>;

    fn query_unreceived_packets(
        &self,
        request: QueryUnreceivedPacketsRequest,
    ) -> Result<Vec<u64>, Error>;

    fn query_packet_acknowledgements(
        &self,
        request: QueryPacketAcknowledgementsRequest,
    ) -> Result<(Vec<PacketState>, Height), Error>;

    fn query_unreceived_acknowledgement(
        &self,
        request: QueryUnreceivedAcksRequest,
    ) -> Result<Vec<u64>, Error>;

    fn query_txs(&self, request: QueryTxRequest) -> Result<Vec<IbcEvent>, Error>;
}
