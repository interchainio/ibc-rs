use alloc::sync::Arc;
use std::thread;

use crossbeam_channel as channel;
use tokio::runtime::Runtime as TokioRuntime;
use tracing::error;

use ibc::{
    events::IbcEvent,
    ics02_client::{
        client_consensus::{AnyConsensusState, AnyConsensusStateWithHeight, ConsensusState},
        client_state::{AnyClientState, ClientState, IdentifiedAnyClientState},
        events::UpdateClient,
        header::{AnyHeader, Header},
        misbehaviour::MisbehaviourEvidence,
    },
    ics03_connection::{
        connection::{ConnectionEnd, IdentifiedConnectionEnd},
        version::Version,
    },
    ics04_channel::{
        channel::{ChannelEnd, IdentifiedChannelEnd},
        packet::{PacketMsgType, Sequence},
    },
    ics23_commitment::commitment::CommitmentPrefix,
    ics24_host::identifier::{ChannelId, ClientId, ConnectionId, PortId},
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
    connection::v1::{QueryClientConnectionsRequest, QueryConnectionsRequest},
};

use crate::{
    config::ChainConfig,
    connection::ConnectionMsgType,
    error::Error,
    event::{
        bus::EventBus,
        monitor::{EventBatch, EventReceiver, MonitorCmd, Result as MonitorResult, TxMonitorCmd},
    },
    keyring::KeyEntry,
    light_client::LightClient,
};

use super::{
    handle::{ChainHandle, ChainRequest, ReplyTo, Subscription},
    ChainEndpoint,
};

pub struct Threads {
    pub chain_runtime: thread::JoinHandle<()>,
    pub event_monitor: Option<thread::JoinHandle<()>>,
}

pub struct ChainRuntime<Endpoint: ChainEndpoint> {
    /// The specific chain this runtime runs against
    chain: Endpoint,

    /// The sender side of a channel to this runtime. Any `ChainHandle` can use this to send
    /// chain requests to this runtime
    request_sender: channel::Sender<ChainRequest>,

    /// The receiving side of a channel to this runtime. The runtime consumes chain requests coming
    /// in through this channel.
    request_receiver: channel::Receiver<ChainRequest>,

    /// An event bus, for broadcasting events that this runtime receives (via `event_receiver`) to subscribers
    event_bus: EventBus<Arc<MonitorResult<EventBatch>>>,

    /// Receiver channel from the event bus
    event_receiver: EventReceiver,

    /// Sender channel to terminate the event monitor
    tx_monitor_cmd: TxMonitorCmd,

    /// A handle to the light client
    light_client: Endpoint::LightClient,

    #[allow(dead_code)]
    rt: Arc<TokioRuntime>, // Making this future-proof, so we keep the runtime around.
}

impl<Endpoint> ChainRuntime<Endpoint>
where
    Endpoint: ChainEndpoint + Send + 'static,
{
    /// Spawns a new runtime for a specific Chain implementation.
    pub fn spawn<Handle: ChainHandle>(
        config: ChainConfig,
        rt: Arc<TokioRuntime>,
    ) -> Result<Handle, Error> {
        // Similar to `from_config`.
        let chain = Endpoint::bootstrap(config, rt.clone())?;

        // Start the light client
        let light_client = chain.init_light_client()?;

        // Start the event monitor
        let (event_batch_rx, tx_monitor_cmd) = chain.init_event_monitor(rt.clone())?;

        // Instantiate & spawn the runtime
        let (handle, _) = Self::init(chain, light_client, event_batch_rx, tx_monitor_cmd, rt);

        Ok(handle)
    }

    /// Initializes a runtime for a given chain, and spawns the associated thread
    fn init<Handle: ChainHandle>(
        chain: Endpoint,
        light_client: Endpoint::LightClient,
        event_receiver: EventReceiver,
        tx_monitor_cmd: TxMonitorCmd,
        rt: Arc<TokioRuntime>,
    ) -> (Handle, thread::JoinHandle<()>) {
        let chain_runtime = Self::new(chain, light_client, event_receiver, tx_monitor_cmd, rt);

        // Get a handle to the runtime
        let handle: Handle = chain_runtime.handle();

        // Spawn the runtime & return
        let id = handle.id();
        let thread = thread::spawn(move || {
            if let Err(e) = chain_runtime.run() {
                error!("failed to start runtime for chain '{}': {}", id, e);
            }
        });

        (handle, thread)
    }

    /// Basic constructor
    fn new(
        chain: Endpoint,
        light_client: Endpoint::LightClient,
        event_receiver: EventReceiver,
        tx_monitor_cmd: TxMonitorCmd,
        rt: Arc<TokioRuntime>,
    ) -> Self {
        let (request_sender, request_receiver) = channel::unbounded::<ChainRequest>();

        Self {
            rt,
            chain,
            request_sender,
            request_receiver,
            event_bus: EventBus::new(),
            event_receiver,
            tx_monitor_cmd,
            light_client,
        }
    }

    pub fn handle<Handle: ChainHandle>(&self) -> Handle {
        let chain_id = ChainEndpoint::id(&self.chain).clone();
        let sender = self.request_sender.clone();

        Handle::new(chain_id, sender)
    }

    fn run(mut self) -> Result<(), Error> {
        loop {
            channel::select! {
                recv(self.event_receiver) -> event_batch => {
                    match event_batch {
                        Ok(event_batch) => {
                            self.event_bus
                                .broadcast(Arc::new(event_batch));
                        },
                        Err(e) => {
                            error!("received error via event bus: {}", e);
                            return Err(Error::channel_receive(e));
                        },
                    }
                },
                recv(self.request_receiver) -> event => {
                    match event {
                        Ok(ChainRequest::Shutdown { reply_to }) => {
                            self.tx_monitor_cmd.send(MonitorCmd::Shutdown)
                                .map_err(Error::send)?;

                            let res = self.chain.shutdown();
                            reply_to.send(res)
                                .map_err(Error::send)?;

                            break;
                        }

                        Ok(ChainRequest::Subscribe { reply_to }) => {
                            self.subscribe(reply_to)?
                        },

                        Ok(ChainRequest::SendMessagesAndWaitCommit { proto_msgs, reply_to }) => {
                            self.send_messages_and_wait_commit(proto_msgs, reply_to)?
                        },

                        Ok(ChainRequest::SendMessagesAndWaitCheckTx { proto_msgs, reply_to }) => {
                            self.send_messages_and_wait_check_tx(proto_msgs, reply_to)?
                        },

                        Ok(ChainRequest::Signer { reply_to }) => {
                            self.get_signer(reply_to)?
                        }

                        Ok(ChainRequest::Key { reply_to }) => {
                            self.get_key(reply_to)?
                        }

                        Ok(ChainRequest::ModuleVersion { port_id, reply_to }) => {
                            self.module_version(port_id, reply_to)?
                        }

                        Ok(ChainRequest::BuildHeader { trusted_height, target_height, client_state, reply_to }) => {
                            self.build_header(trusted_height, target_height, client_state, reply_to)?
                        }

                        Ok(ChainRequest::BuildClientState { height, reply_to }) => {
                            self.build_client_state(height, reply_to)?
                        }

                        Ok(ChainRequest::BuildConsensusState { trusted, target, client_state, reply_to }) => {
                            self.build_consensus_state(trusted, target, client_state, reply_to)?
                        }

                       Ok(ChainRequest::BuildMisbehaviour { client_state, update_event, reply_to }) => {
                            self.check_misbehaviour(update_event, client_state, reply_to)?
                        }

                        Ok(ChainRequest::BuildConnectionProofsAndClientState { message_type, connection_id, client_id, height, reply_to }) => {
                            self.build_connection_proofs_and_client_state(message_type, connection_id, client_id, height, reply_to)?
                        },

                        Ok(ChainRequest::BuildChannelProofs { port_id, channel_id, height, reply_to }) => {
                            self.build_channel_proofs(port_id, channel_id, height, reply_to)?
                        },

                        Ok(ChainRequest::QueryLatestHeight { reply_to }) => {
                            self.query_latest_height(reply_to)?
                        }

                        Ok(ChainRequest::QueryClients { request, reply_to }) => {
                            self.query_clients(request, reply_to)?
                        },

                        Ok(ChainRequest::QueryClientConnections { request, reply_to }) => {
                            self.query_client_connections(request, reply_to)?
                        },

                        Ok(ChainRequest::QueryClientState { client_id, height, reply_to }) => {
                            self.query_client_state(client_id, height, reply_to)?
                        },

                        Ok(ChainRequest::QueryConsensusStates { request, reply_to }) => {
                            self.query_consensus_states(request, reply_to)?
                        },

                        Ok(ChainRequest::QueryConsensusState { client_id, consensus_height, query_height, reply_to }) => {
                            self.query_consensus_state(client_id, consensus_height, query_height, reply_to)?
                        },

                        Ok(ChainRequest::QueryUpgradedClientState { height, reply_to }) => {
                            self.query_upgraded_client_state(height, reply_to)?
                        }

                       Ok(ChainRequest::QueryUpgradedConsensusState { height, reply_to }) => {
                            self.query_upgraded_consensus_state(height, reply_to)?
                        }

                        Ok(ChainRequest::QueryCommitmentPrefix { reply_to }) => {
                            self.query_commitment_prefix(reply_to)?
                        },

                        Ok(ChainRequest::QueryCompatibleVersions { reply_to }) => {
                            self.query_compatible_versions(reply_to)?
                        },

                        Ok(ChainRequest::QueryConnection { connection_id, height, reply_to }) => {
                            self.query_connection(connection_id, height, reply_to)?
                        },

                        Ok(ChainRequest::QueryConnections { request, reply_to }) => {
                            self.query_connections(request, reply_to)?
                        },

                        Ok(ChainRequest::QueryConnectionChannels { request, reply_to }) => {
                            self.query_connection_channels(request, reply_to)?
                        },

                        Ok(ChainRequest::QueryChannels { request, reply_to }) => {
                            self.query_channels(request, reply_to)?
                        },

                        Ok(ChainRequest::QueryChannel { port_id, channel_id, height, reply_to }) => {
                            self.query_channel(port_id, channel_id, height, reply_to)?
                        },

                        Ok(ChainRequest::QueryChannelClientState { request, reply_to }) => {
                            self.query_channel_client_state(request, reply_to)?
                        },

                        Ok(ChainRequest::ProvenClientState { client_id, height, reply_to }) => {
                            self.proven_client_state(client_id, height, reply_to)?
                        },

                        Ok(ChainRequest::ProvenConnection { connection_id, height, reply_to }) => {
                            self.proven_connection(connection_id, height, reply_to)?
                        },

                        Ok(ChainRequest::ProvenClientConsensus { client_id, consensus_height, height, reply_to }) => {
                            self.proven_client_consensus(client_id, consensus_height, height, reply_to)?
                        },

                        Ok(ChainRequest::BuildPacketProofs { packet_type, port_id, channel_id, sequence, height, reply_to }) => {
                            self.build_packet_proofs(packet_type, port_id, channel_id, sequence, height, reply_to)?
                        },

                        Ok(ChainRequest::QueryPacketCommitments { request, reply_to }) => {
                            self.query_packet_commitments(request, reply_to)?
                        },

                        Ok(ChainRequest::QueryUnreceivedPackets { request, reply_to }) => {
                            self.query_unreceived_packets(request, reply_to)?
                        },

                        Ok(ChainRequest::QueryPacketAcknowledgement { request, reply_to }) => {
                            self.query_packet_acknowledgements(request, reply_to)?
                        },

                        Ok(ChainRequest::QueryUnreceivedAcknowledgement { request, reply_to }) => {
                            self.query_unreceived_acknowledgement(request, reply_to)?
                        },

                        Ok(ChainRequest::QueryNextSequenceReceive { request, reply_to }) => {
                            self.query_next_sequence_receive(request, reply_to)?
                        },

                        Ok(ChainRequest::QueryPacketEventData { request, reply_to }) => {
                            self.query_txs(request, reply_to)?
                        },

                        Err(e) => error!("received error via chain request channel: {}", e),
                    }
                },
            }
        }

        Ok(())
    }

    fn subscribe(&mut self, reply_to: ReplyTo<Subscription>) -> Result<(), Error> {
        let subscription = self.event_bus.subscribe();

        reply_to.send(Ok(subscription)).map_err(Error::send)?;

        Ok(())
    }

    fn send_messages_and_wait_commit(
        &mut self,
        proto_msgs: Vec<prost_types::Any>,
        reply_to: ReplyTo<Vec<IbcEvent>>,
    ) -> Result<(), Error> {
        let result = self.chain.send_messages_and_wait_commit(proto_msgs);

        reply_to.send(result).map_err(Error::send)?;

        Ok(())
    }

    fn send_messages_and_wait_check_tx(
        &mut self,
        proto_msgs: Vec<prost_types::Any>,
        reply_to: ReplyTo<Vec<tendermint_rpc::endpoint::broadcast::tx_sync::Response>>,
    ) -> Result<(), Error> {
        let result = self.chain.send_messages_and_wait_check_tx(proto_msgs);

        reply_to.send(result).map_err(Error::send)?;

        Ok(())
    }

    fn query_latest_height(&self, reply_to: ReplyTo<Height>) -> Result<(), Error> {
        let latest_height = self.chain.query_latest_height();

        reply_to.send(latest_height).map_err(Error::send)?;

        Ok(())
    }

    fn get_signer(&mut self, reply_to: ReplyTo<Signer>) -> Result<(), Error> {
        let result = self.chain.get_signer();

        reply_to.send(result).map_err(Error::send)?;

        Ok(())
    }

    fn get_key(&mut self, reply_to: ReplyTo<KeyEntry>) -> Result<(), Error> {
        let result = self.chain.get_key();

        reply_to.send(result).map_err(Error::send)?;

        Ok(())
    }

    fn module_version(&self, port_id: PortId, reply_to: ReplyTo<String>) -> Result<(), Error> {
        let result = self.chain.query_module_version(&port_id);

        reply_to.send(Ok(result)).map_err(Error::send)?;

        Ok(())
    }

    fn build_header(
        &mut self,
        trusted_height: Height,
        target_height: Height,
        client_state: AnyClientState,
        reply_to: ReplyTo<(AnyHeader, Vec<AnyHeader>)>,
    ) -> Result<(), Error> {
        let result = self
            .chain
            .build_header(
                trusted_height,
                target_height,
                &client_state,
                &mut self.light_client,
            )
            .map(|(header, support)| {
                let header = header.wrap_any();
                let support = support.into_iter().map(|h| h.wrap_any()).collect();
                (header, support)
            });

        reply_to.send(result).map_err(Error::send)?;

        Ok(())
    }

    /// Constructs a client state for the given height
    fn build_client_state(
        &self,
        height: Height,
        reply_to: ReplyTo<AnyClientState>,
    ) -> Result<(), Error> {
        let client_state = self
            .chain
            .build_client_state(height)
            .map(|cs| cs.wrap_any());

        reply_to.send(client_state).map_err(Error::send)?;

        Ok(())
    }

    /// Constructs a consensus state for the given height
    fn build_consensus_state(
        &mut self,
        trusted: Height,
        target: Height,
        client_state: AnyClientState,
        reply_to: ReplyTo<AnyConsensusState>,
    ) -> Result<(), Error> {
        let verified = self.light_client.verify(trusted, target, &client_state)?;

        let consensus_state = self
            .chain
            .build_consensus_state(verified.target)
            .map(|cs| cs.wrap_any());

        reply_to.send(consensus_state).map_err(Error::send)?;

        Ok(())
    }

    /// Constructs AnyMisbehaviour for the update event
    fn check_misbehaviour(
        &mut self,
        update_event: UpdateClient,
        client_state: AnyClientState,
        reply_to: ReplyTo<Option<MisbehaviourEvidence>>,
    ) -> Result<(), Error> {
        let misbehaviour = self
            .light_client
            .check_misbehaviour(update_event, &client_state);

        reply_to.send(misbehaviour).map_err(Error::send)?;

        Ok(())
    }

    fn build_connection_proofs_and_client_state(
        &self,
        message_type: ConnectionMsgType,
        connection_id: ConnectionId,
        client_id: ClientId,
        height: Height,
        reply_to: ReplyTo<(Option<AnyClientState>, Proofs)>,
    ) -> Result<(), Error> {
        let result = self.chain.build_connection_proofs_and_client_state(
            message_type,
            &connection_id,
            &client_id,
            height,
        );

        let result = result
            .map(|(opt_client_state, proofs)| (opt_client_state.map(|cs| cs.wrap_any()), proofs));

        reply_to.send(result).map_err(Error::send)?;

        Ok(())
    }

    fn query_clients(
        &self,
        request: QueryClientStatesRequest,
        reply_to: ReplyTo<Vec<IdentifiedAnyClientState>>,
    ) -> Result<(), Error> {
        let result = self.chain.query_clients(request);

        reply_to.send(result).map_err(Error::send)?;

        Ok(())
    }

    fn query_client_connections(
        &self,
        request: QueryClientConnectionsRequest,
        reply_to: ReplyTo<Vec<ConnectionId>>,
    ) -> Result<(), Error> {
        let result = self.chain.query_client_connections(request);

        reply_to.send(result).map_err(Error::send)?;

        Ok(())
    }

    fn query_client_state(
        &self,
        client_id: ClientId,
        height: Height,
        reply_to: ReplyTo<AnyClientState>,
    ) -> Result<(), Error> {
        let client_state = self
            .chain
            .query_client_state(&client_id, height)
            .map(|cs| cs.wrap_any());

        reply_to.send(client_state).map_err(Error::send)?;

        Ok(())
    }

    fn query_upgraded_client_state(
        &self,
        height: Height,
        reply_to: ReplyTo<(AnyClientState, MerkleProof)>,
    ) -> Result<(), Error> {
        let result = self
            .chain
            .query_upgraded_client_state(height)
            .map(|(cl, proof)| (cl.wrap_any(), proof));

        reply_to.send(result).map_err(Error::send)?;

        Ok(())
    }

    fn query_consensus_states(
        &self,
        request: QueryConsensusStatesRequest,
        reply_to: ReplyTo<Vec<AnyConsensusStateWithHeight>>,
    ) -> Result<(), Error> {
        let consensus_states = self.chain.query_consensus_states(request);

        reply_to.send(consensus_states).map_err(Error::send)?;

        Ok(())
    }

    fn query_consensus_state(
        &self,
        client_id: ClientId,
        consensus_height: Height,
        query_height: Height,
        reply_to: ReplyTo<AnyConsensusState>,
    ) -> Result<(), Error> {
        let consensus_state =
            self.chain
                .query_consensus_state(client_id, consensus_height, query_height);

        reply_to.send(consensus_state).map_err(Error::send)?;

        Ok(())
    }

    fn query_upgraded_consensus_state(
        &self,
        height: Height,
        reply_to: ReplyTo<(AnyConsensusState, MerkleProof)>,
    ) -> Result<(), Error> {
        let result = self
            .chain
            .query_upgraded_consensus_state(height)
            .map(|(cs, proof)| (cs.wrap_any(), proof));

        reply_to.send(result).map_err(Error::send)?;

        Ok(())
    }

    fn query_commitment_prefix(&self, reply_to: ReplyTo<CommitmentPrefix>) -> Result<(), Error> {
        let prefix = self.chain.query_commitment_prefix();

        reply_to.send(prefix).map_err(Error::send)?;

        Ok(())
    }

    fn query_compatible_versions(&self, reply_to: ReplyTo<Vec<Version>>) -> Result<(), Error> {
        let versions = self.chain.query_compatible_versions();

        reply_to.send(versions).map_err(Error::send)?;

        Ok(())
    }

    fn query_connection(
        &self,
        connection_id: ConnectionId,
        height: Height,
        reply_to: ReplyTo<ConnectionEnd>,
    ) -> Result<(), Error> {
        let connection_end = self.chain.query_connection(&connection_id, height);

        reply_to.send(connection_end).map_err(Error::send)?;

        Ok(())
    }

    fn query_connections(
        &self,
        request: QueryConnectionsRequest,
        reply_to: ReplyTo<Vec<IdentifiedConnectionEnd>>,
    ) -> Result<(), Error> {
        let result = self.chain.query_connections(request);

        reply_to.send(result).map_err(Error::send)?;

        Ok(())
    }

    fn query_connection_channels(
        &self,
        request: QueryConnectionChannelsRequest,
        reply_to: ReplyTo<Vec<IdentifiedChannelEnd>>,
    ) -> Result<(), Error> {
        let result = self.chain.query_connection_channels(request);

        reply_to.send(result).map_err(Error::send)?;

        Ok(())
    }

    fn query_channels(
        &self,
        request: QueryChannelsRequest,
        reply_to: ReplyTo<Vec<IdentifiedChannelEnd>>,
    ) -> Result<(), Error> {
        let result = self.chain.query_channels(request);

        reply_to.send(result).map_err(Error::send)?;

        Ok(())
    }

    fn query_channel(
        &self,
        port_id: PortId,
        channel_id: ChannelId,
        height: Height,
        reply_to: ReplyTo<ChannelEnd>,
    ) -> Result<(), Error> {
        let result = self.chain.query_channel(&port_id, &channel_id, height);

        reply_to.send(result).map_err(Error::send)?;

        Ok(())
    }

    fn query_channel_client_state(
        &self,
        request: QueryChannelClientStateRequest,
        reply_to: ReplyTo<Option<IdentifiedAnyClientState>>,
    ) -> Result<(), Error> {
        let result = self.chain.query_channel_client_state(request);

        reply_to.send(result).map_err(Error::send)?;

        Ok(())
    }

    fn proven_client_state(
        &self,
        client_id: ClientId,
        height: Height,
        reply_to: ReplyTo<(AnyClientState, MerkleProof)>,
    ) -> Result<(), Error> {
        let result = self
            .chain
            .proven_client_state(&client_id, height)
            .map(|(cs, mp)| (cs.wrap_any(), mp));

        reply_to.send(result).map_err(Error::send)?;

        Ok(())
    }

    fn proven_connection(
        &self,
        connection_id: ConnectionId,
        height: Height,
        reply_to: ReplyTo<(ConnectionEnd, MerkleProof)>,
    ) -> Result<(), Error> {
        let result = self.chain.proven_connection(&connection_id, height);

        reply_to.send(result).map_err(Error::send)?;

        Ok(())
    }

    fn proven_client_consensus(
        &self,
        client_id: ClientId,
        consensus_height: Height,
        height: Height,
        reply_to: ReplyTo<(AnyConsensusState, MerkleProof)>,
    ) -> Result<(), Error> {
        let result = self
            .chain
            .proven_client_consensus(&client_id, consensus_height, height)
            .map(|(cs, mp)| (cs.wrap_any(), mp));

        reply_to.send(result).map_err(Error::send)?;

        Ok(())
    }

    fn build_channel_proofs(
        &self,
        port_id: PortId,
        channel_id: ChannelId,
        height: Height,
        reply_to: ReplyTo<Proofs>,
    ) -> Result<(), Error> {
        let result = self
            .chain
            .build_channel_proofs(&port_id, &channel_id, height);

        reply_to.send(result).map_err(Error::send)?;

        Ok(())
    }

    fn build_packet_proofs(
        &self,
        packet_type: PacketMsgType,
        port_id: PortId,
        channel_id: ChannelId,
        sequence: Sequence,
        height: Height,
        reply_to: ReplyTo<(Vec<u8>, Proofs)>,
    ) -> Result<(), Error> {
        let result =
            self.chain
                .build_packet_proofs(packet_type, port_id, channel_id, sequence, height);

        reply_to.send(result).map_err(Error::send)?;

        Ok(())
    }

    fn query_packet_commitments(
        &self,
        request: QueryPacketCommitmentsRequest,
        reply_to: ReplyTo<(Vec<PacketState>, Height)>,
    ) -> Result<(), Error> {
        let result = self.chain.query_packet_commitments(request);

        reply_to.send(result).map_err(Error::send)?;

        Ok(())
    }

    fn query_unreceived_packets(
        &self,
        request: QueryUnreceivedPacketsRequest,
        reply_to: ReplyTo<Vec<u64>>,
    ) -> Result<(), Error> {
        let result = self.chain.query_unreceived_packets(request);

        reply_to.send(result).map_err(Error::send)?;

        Ok(())
    }

    fn query_packet_acknowledgements(
        &self,
        request: QueryPacketAcknowledgementsRequest,
        reply_to: ReplyTo<(Vec<PacketState>, Height)>,
    ) -> Result<(), Error> {
        let result = self.chain.query_packet_acknowledgements(request);

        reply_to.send(result).map_err(Error::send)?;

        Ok(())
    }

    fn query_unreceived_acknowledgement(
        &self,
        request: QueryUnreceivedAcksRequest,
        reply_to: ReplyTo<Vec<u64>>,
    ) -> Result<(), Error> {
        let result = self.chain.query_unreceived_acknowledgements(request);

        reply_to.send(result).map_err(Error::send)?;

        Ok(())
    }

    fn query_next_sequence_receive(
        &self,
        request: QueryNextSequenceReceiveRequest,
        reply_to: ReplyTo<Sequence>,
    ) -> Result<(), Error> {
        let result = self.chain.query_next_sequence_receive(request);

        reply_to.send(result).map_err(Error::send)?;

        Ok(())
    }

    fn query_txs(
        &self,
        request: QueryTxRequest,
        reply_to: ReplyTo<Vec<IbcEvent>>,
    ) -> Result<(), Error> {
        let result = self.chain.query_txs(request);

        reply_to.send(result).map_err(Error::send)?;

        Ok(())
    }
}
