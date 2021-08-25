use core::fmt::Debug;

use crossbeam_channel as channel;
use serde::{Serialize, Serializer};

use ibc::ics02_client::client_consensus::{AnyConsensusState, AnyConsensusStateWithHeight};
use ibc::ics02_client::client_state::{AnyClientState, IdentifiedAnyClientState};
use ibc::ics02_client::events::UpdateClient;
use ibc::ics02_client::misbehaviour::MisbehaviourEvidence;
use ibc::ics03_connection::connection::IdentifiedConnectionEnd;
use ibc::ics04_channel::channel::IdentifiedChannelEnd;
use ibc::ics04_channel::packet::{PacketMsgType, Sequence};
use ibc::query::QueryTxRequest;
use ibc::{
    events::IbcEvent,
    ics02_client::header::AnyHeader,
    ics03_connection::connection::ConnectionEnd,
    ics03_connection::version::Version,
    ics04_channel::channel::ChannelEnd,
    ics23_commitment::commitment::CommitmentPrefix,
    ics24_host::identifier::ChainId,
    ics24_host::identifier::ChannelId,
    ics24_host::identifier::{ClientId, ConnectionId, PortId},
    proofs::Proofs,
    signer::Signer,
    Height,
};
use ibc_proto::ibc::core::channel::v1::{
    PacketState, QueryChannelClientStateRequest, QueryChannelsRequest,
    QueryConnectionChannelsRequest, QueryNextSequenceReceiveRequest,
    QueryPacketAcknowledgementsRequest, QueryPacketCommitmentsRequest, QueryUnreceivedAcksRequest,
    QueryUnreceivedPacketsRequest,
};
use ibc_proto::ibc::core::client::v1::{QueryClientStatesRequest, QueryConsensusStatesRequest};
use ibc_proto::ibc::core::commitment::v1::MerkleProof;
use ibc_proto::ibc::core::connection::v1::QueryClientConnectionsRequest;
use ibc_proto::ibc::core::connection::v1::QueryConnectionsRequest;

use crate::{connection::ConnectionMsgType, error::Error, keyring::KeyEntry};

use super::{reply_channel, ChainHandle, ChainRequest, ReplyTo, Subscription};

#[derive(Debug, Clone)]
pub struct ProdChainHandle {
    /// Chain identifier
    chain_id: ChainId,

    /// The handle's channel for sending requests to the runtime
    runtime_sender: channel::Sender<ChainRequest>,
}

impl ProdChainHandle {
    pub fn new(chain_id: ChainId, sender: channel::Sender<ChainRequest>) -> Self {
        Self {
            chain_id,
            runtime_sender: sender,
        }
    }

    fn send<F, O>(&self, f: F) -> Result<O, Error>
    where
        F: FnOnce(ReplyTo<O>) -> ChainRequest,
        O: Debug,
    {
        let (sender, receiver) = reply_channel();
        let input = f(sender);

        self.runtime_sender.send(input).map_err(Error::send)?;

        receiver.recv().map_err(Error::channel_receive)?
    }
}

impl ChainHandle for ProdChainHandle {
    fn new(chain_id: ChainId, sender: channel::Sender<ChainRequest>) -> Self {
        Self::new(chain_id, sender)
    }

    fn id(&self) -> ChainId {
        self.chain_id.clone()
    }

    fn shutdown(&self) -> Result<(), Error> {
        self.send(|reply_to| ChainRequest::Shutdown { reply_to })
    }

    fn subscribe(&self) -> Result<Subscription, Error> {
        self.send(|reply_to| ChainRequest::Subscribe { reply_to })
    }

    fn send_messages_and_wait_commit(
        &self,
        proto_msgs: Vec<prost_types::Any>,
    ) -> Result<Vec<IbcEvent>, Error> {
        self.send(|reply_to| ChainRequest::SendMessagesAndWaitCommit {
            proto_msgs,
            reply_to,
        })
    }

    fn send_messages_and_wait_check_tx(
        &self,
        proto_msgs: Vec<prost_types::Any>,
    ) -> Result<Vec<tendermint_rpc::endpoint::broadcast::tx_sync::Response>, Error> {
        self.send(|reply_to| ChainRequest::SendMessagesAndWaitCheckTx {
            proto_msgs,
            reply_to,
        })
    }

    fn get_signer(&self) -> Result<Signer, Error> {
        self.send(|reply_to| ChainRequest::Signer { reply_to })
    }

    fn get_key(&self) -> Result<KeyEntry, Error> {
        self.send(|reply_to| ChainRequest::Key { reply_to })
    }

    fn module_version(&self, port_id: &PortId) -> Result<String, Error> {
        self.send(|reply_to| ChainRequest::ModuleVersion {
            port_id: port_id.clone(),
            reply_to,
        })
    }

    fn query_latest_height(&self) -> Result<Height, Error> {
        self.send(|reply_to| ChainRequest::QueryLatestHeight { reply_to })
    }

    fn query_clients(
        &self,
        request: QueryClientStatesRequest,
    ) -> Result<Vec<IdentifiedAnyClientState>, Error> {
        self.send(|reply_to| ChainRequest::QueryClients { request, reply_to })
    }

    fn query_client_state(
        &self,
        client_id: &ClientId,
        height: Height,
    ) -> Result<AnyClientState, Error> {
        self.send(|reply_to| ChainRequest::QueryClientState {
            client_id: client_id.clone(),
            height,
            reply_to,
        })
    }

    fn query_client_connections(
        &self,
        request: QueryClientConnectionsRequest,
    ) -> Result<Vec<ConnectionId>, Error> {
        self.send(|reply_to| ChainRequest::QueryClientConnections { request, reply_to })
    }

    fn query_consensus_states(
        &self,
        request: QueryConsensusStatesRequest,
    ) -> Result<Vec<AnyConsensusStateWithHeight>, Error> {
        self.send(|reply_to| ChainRequest::QueryConsensusStates { request, reply_to })
    }

    fn query_consensus_state(
        &self,
        client_id: ClientId,
        consensus_height: Height,
        query_height: Height,
    ) -> Result<AnyConsensusState, Error> {
        self.send(|reply_to| ChainRequest::QueryConsensusState {
            client_id,
            consensus_height,
            query_height,
            reply_to,
        })
    }

    fn query_upgraded_client_state(
        &self,
        height: Height,
    ) -> Result<(AnyClientState, MerkleProof), Error> {
        self.send(|reply_to| ChainRequest::QueryUpgradedClientState { height, reply_to })
    }

    fn query_upgraded_consensus_state(
        &self,
        height: Height,
    ) -> Result<(AnyConsensusState, MerkleProof), Error> {
        self.send(|reply_to| ChainRequest::QueryUpgradedConsensusState { height, reply_to })
    }

    fn query_commitment_prefix(&self) -> Result<CommitmentPrefix, Error> {
        self.send(|reply_to| ChainRequest::QueryCommitmentPrefix { reply_to })
    }

    fn query_compatible_versions(&self) -> Result<Vec<Version>, Error> {
        self.send(|reply_to| ChainRequest::QueryCompatibleVersions { reply_to })
    }

    fn query_connection(
        &self,
        connection_id: &ConnectionId,
        height: Height,
    ) -> Result<ConnectionEnd, Error> {
        self.send(|reply_to| ChainRequest::QueryConnection {
            connection_id: connection_id.clone(),
            height,
            reply_to,
        })
    }

    fn query_connections(
        &self,
        request: QueryConnectionsRequest,
    ) -> Result<Vec<IdentifiedConnectionEnd>, Error> {
        self.send(|reply_to| ChainRequest::QueryConnections { request, reply_to })
    }

    fn query_connection_channels(
        &self,
        request: QueryConnectionChannelsRequest,
    ) -> Result<Vec<IdentifiedChannelEnd>, Error> {
        self.send(|reply_to| ChainRequest::QueryConnectionChannels { request, reply_to })
    }

    fn query_next_sequence_receive(
        &self,
        request: QueryNextSequenceReceiveRequest,
    ) -> Result<Sequence, Error> {
        self.send(|reply_to| ChainRequest::QueryNextSequenceReceive { request, reply_to })
    }

    fn query_channels(
        &self,
        request: QueryChannelsRequest,
    ) -> Result<Vec<IdentifiedChannelEnd>, Error> {
        self.send(|reply_to| ChainRequest::QueryChannels { request, reply_to })
    }

    fn query_channel(
        &self,
        port_id: &PortId,
        channel_id: &ChannelId,
        height: Height,
    ) -> Result<ChannelEnd, Error> {
        self.send(|reply_to| ChainRequest::QueryChannel {
            port_id: port_id.clone(),
            channel_id: channel_id.clone(),
            height,
            reply_to,
        })
    }

    fn query_channel_client_state(
        &self,
        request: QueryChannelClientStateRequest,
    ) -> Result<Option<IdentifiedAnyClientState>, Error> {
        self.send(|reply_to| ChainRequest::QueryChannelClientState { request, reply_to })
    }

    fn proven_client_state(
        &self,
        client_id: &ClientId,
        height: Height,
    ) -> Result<(AnyClientState, MerkleProof), Error> {
        self.send(|reply_to| ChainRequest::ProvenClientState {
            client_id: client_id.clone(),
            height,
            reply_to,
        })
    }

    fn proven_connection(
        &self,
        connection_id: &ConnectionId,
        height: Height,
    ) -> Result<(ConnectionEnd, MerkleProof), Error> {
        self.send(|reply_to| ChainRequest::ProvenConnection {
            connection_id: connection_id.clone(),
            height,
            reply_to,
        })
    }

    fn proven_client_consensus(
        &self,
        client_id: &ClientId,
        consensus_height: Height,
        height: Height,
    ) -> Result<(AnyConsensusState, MerkleProof), Error> {
        self.send(|reply_to| ChainRequest::ProvenClientConsensus {
            client_id: client_id.clone(),
            consensus_height,
            height,
            reply_to,
        })
    }

    fn build_header(
        &self,
        trusted_height: Height,
        target_height: Height,
        client_state: AnyClientState,
    ) -> Result<(AnyHeader, Vec<AnyHeader>), Error> {
        self.send(|reply_to| ChainRequest::BuildHeader {
            trusted_height,
            target_height,
            client_state,
            reply_to,
        })
    }

    fn build_client_state(&self, height: Height) -> Result<AnyClientState, Error> {
        self.send(|reply_to| ChainRequest::BuildClientState { height, reply_to })
    }

    fn build_consensus_state(
        &self,
        trusted: Height,
        target: Height,
        client_state: AnyClientState,
    ) -> Result<AnyConsensusState, Error> {
        self.send(|reply_to| ChainRequest::BuildConsensusState {
            trusted,
            target,
            client_state,
            reply_to,
        })
    }

    fn check_misbehaviour(
        &self,
        update_event: UpdateClient,
        client_state: AnyClientState,
    ) -> Result<Option<MisbehaviourEvidence>, Error> {
        self.send(|reply_to| ChainRequest::BuildMisbehaviour {
            client_state,
            update_event,
            reply_to,
        })
    }

    fn build_connection_proofs_and_client_state(
        &self,
        message_type: ConnectionMsgType,
        connection_id: &ConnectionId,
        client_id: &ClientId,
        height: Height,
    ) -> Result<(Option<AnyClientState>, Proofs), Error> {
        self.send(
            |reply_to| ChainRequest::BuildConnectionProofsAndClientState {
                message_type,
                connection_id: connection_id.clone(),
                client_id: client_id.clone(),
                height,
                reply_to,
            },
        )
    }

    fn build_channel_proofs(
        &self,
        port_id: &PortId,
        channel_id: &ChannelId,
        height: Height,
    ) -> Result<Proofs, Error> {
        self.send(|reply_to| ChainRequest::BuildChannelProofs {
            port_id: port_id.clone(),
            channel_id: channel_id.clone(),
            height,
            reply_to,
        })
    }

    fn build_packet_proofs(
        &self,
        packet_type: PacketMsgType,
        port_id: &PortId,
        channel_id: &ChannelId,
        sequence: Sequence,
        height: Height,
    ) -> Result<(Vec<u8>, Proofs), Error> {
        self.send(|reply_to| ChainRequest::BuildPacketProofs {
            packet_type,
            port_id: port_id.clone(),
            channel_id: channel_id.clone(),
            sequence,
            height,
            reply_to,
        })
    }

    fn query_packet_commitments(
        &self,
        request: QueryPacketCommitmentsRequest,
    ) -> Result<(Vec<PacketState>, Height), Error> {
        self.send(|reply_to| ChainRequest::QueryPacketCommitments { request, reply_to })
    }

    fn query_unreceived_packets(
        &self,
        request: QueryUnreceivedPacketsRequest,
    ) -> Result<Vec<u64>, Error> {
        self.send(|reply_to| ChainRequest::QueryUnreceivedPackets { request, reply_to })
    }

    fn query_packet_acknowledgements(
        &self,
        request: QueryPacketAcknowledgementsRequest,
    ) -> Result<(Vec<PacketState>, Height), Error> {
        self.send(|reply_to| ChainRequest::QueryPacketAcknowledgement { request, reply_to })
    }

    fn query_unreceived_acknowledgement(
        &self,
        request: QueryUnreceivedAcksRequest,
    ) -> Result<Vec<u64>, Error> {
        self.send(|reply_to| ChainRequest::QueryUnreceivedAcknowledgement { request, reply_to })
    }

    fn query_txs(&self, request: QueryTxRequest) -> Result<Vec<IbcEvent>, Error> {
        self.send(|reply_to| ChainRequest::QueryPacketEventData { request, reply_to })
    }
}

impl Serialize for ProdChainHandle {
    fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        self.id().serialize(serializer)
    }
}
