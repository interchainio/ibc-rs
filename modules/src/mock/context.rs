//! Implementation of a global context mock. Used in testing handlers of all IBC modules.

use std::cmp::min;
use std::collections::HashMap;

use prost_types::Any;
use sha2::Digest;

use crate::application::ics20_fungible_token_transfer::context::Ics20Context;
use crate::events::IbcEvent;
use crate::ics02_client::client_consensus::{AnyConsensusState, AnyConsensusStateWithHeight};
use crate::ics02_client::client_state::AnyClientState;
use crate::ics02_client::client_type::ClientType;
use crate::ics02_client::context::{ClientKeeper, ClientReader};
use crate::ics02_client::error::Error as Ics02Error;
use crate::ics02_client::header::AnyHeader;
use crate::ics03_connection::connection::ConnectionEnd;
use crate::ics03_connection::context::{ConnectionKeeper, ConnectionReader};
use crate::ics03_connection::error::Error as Ics03Error;
use crate::ics04_channel::channel::ChannelEnd;
use crate::ics04_channel::context::{ChannelKeeper, ChannelReader};
use crate::ics04_channel::error::Error as Ics04Error;
use crate::ics04_channel::packet::{Receipt, Sequence};
use crate::ics05_port::capabilities::Capability;
use crate::ics05_port::context::PortReader;
use crate::ics05_port::error::Error as Ics05Error;
use crate::ics07_tendermint::client_state::test_util::get_dummy_tendermint_client_state;
use crate::ics18_relayer::context::Ics18Context;
use crate::ics18_relayer::error::Error as Ics18Error;
use crate::ics23_commitment::commitment::CommitmentPrefix;
use crate::ics24_host::identifier::{ChainId, ChannelId, ClientId, ConnectionId, PortId};
use crate::ics26_routing::context::Ics26Context;
use crate::ics26_routing::handler::{deliver, dispatch};
use crate::ics26_routing::msgs::Ics26Envelope;
use crate::mock::client_state::{MockClientRecord, MockClientState, MockConsensusState};
use crate::mock::header::MockHeader;
use crate::mock::host::{HostBlock, HostType};
use crate::signer::Signer;
use crate::timestamp::Timestamp;
use crate::Height;

/// A context implementing the dependencies necessary for testing any IBC module.
#[derive(Clone, Debug)]
pub struct MockContext {
    /// The type of host chain underlying this mock context.
    host_chain_type: HostType,

    /// Host chain identifier.
    host_chain_id: ChainId,

    /// Maximum size for the history of the host chain. Any block older than this is pruned.
    max_history_size: usize,

    /// Highest height (i.e., most recent) of the blocks in the history.
    latest_height: Height,

    /// Highest timestamp, i.e., of the most recent block in the history.
    timestamp: Timestamp,

    /// The chain of blocks underlying this context. A vector of size up to `max_history_size`
    /// blocks, ascending order by their height (latest block is on the last position).
    history: Vec<HostBlock>,

    /// The set of all clients, indexed by their id.
    clients: HashMap<ClientId, MockClientRecord>,

    /// Counter for the client identifiers, necessary for `increase_client_counter` and the
    /// `client_counter` methods.
    client_ids_counter: u64,

    /// Association between client ids and connection ids.
    client_connections: HashMap<ClientId, ConnectionId>,

    /// All the connections in the store.
    connections: HashMap<ConnectionId, ConnectionEnd>,

    /// Counter for connection identifiers (see `increase_connection_counter`).
    connection_ids_counter: u64,

    /// Association between connection ids and channel ids.
    connection_channels: HashMap<ConnectionId, Vec<(PortId, ChannelId)>>,

    /// Counter for channel identifiers (see `increase_channel_counter`).
    channel_ids_counter: u64,

    /// All the channels in the store. TODO Make new key PortId X ChanneId
    channels: HashMap<(PortId, ChannelId), ChannelEnd>,

    /// Tracks the sequence number for the next packet to be sent.
    next_sequence_send: HashMap<(PortId, ChannelId), Sequence>,

    /// Tracks the sequence number for the next packet to be received.
    next_sequence_recv: HashMap<(PortId, ChannelId), Sequence>,

    /// Tracks the sequence number for the next packet to be acknowledged.
    next_sequence_ack: HashMap<(PortId, ChannelId), Sequence>,

    packet_acknowledgement: HashMap<(PortId, ChannelId, Sequence), String>,

    /// Maps ports to their capabilities
    port_capabilities: HashMap<PortId, Capability>,

    /// Constant-size commitments to packets data fields
    packet_commitment: HashMap<(PortId, ChannelId, Sequence), String>,

    // Used by unordered channel
    packet_receipt: HashMap<(PortId, ChannelId, Sequence), Receipt>,
}

/// Returns a MockContext with bare minimum initialization: no clients, no connections and no channels are
/// present, and the chain has Height(5). This should be used sparingly, mostly for testing the
/// creation of new domain objects.
impl Default for MockContext {
    fn default() -> Self {
        Self::new(
            ChainId::new("mockgaia".to_string(), 0),
            HostType::Mock,
            5,
            Height::new(0, 5),
        )
    }
}

/// Implementation of internal interface for use in testing. The methods in this interface should
/// _not_ be accessible to any Ics handler.
impl MockContext {
    /// Creates a mock context. Parameter `max_history_size` determines how many blocks will
    /// the chain maintain in its history, which also determines the pruning window. Parameter
    /// `latest_height` determines the current height of the chain. This context
    /// has support to emulate two type of underlying chains: Mock or SyntheticTendermint.
    pub fn new(
        host_id: ChainId,
        host_type: HostType,
        max_history_size: usize,
        latest_height: Height,
    ) -> Self {
        assert_ne!(
            max_history_size, 0,
            "The chain must have a non-zero max_history_size"
        );

        // Compute the number of blocks to store. If latest_height is 0, nothing is stored.
        let n = min(max_history_size as u64, latest_height.revision_height);

        assert_eq!(
            host_id.version(),
            latest_height.revision_number,
            "The version in the chain identifier must match the version in the latest height"
        );

        MockContext {
            host_chain_type: host_type,
            host_chain_id: host_id.clone(),
            max_history_size,
            latest_height,
            timestamp: Default::default(),
            history: (0..n)
                .rev()
                .map(|i| {
                    HostBlock::generate_block(
                        host_id.clone(),
                        host_type,
                        latest_height.sub(i).unwrap().revision_height,
                    )
                })
                .collect(),
            connections: Default::default(),
            client_ids_counter: 0,
            clients: Default::default(),
            client_connections: Default::default(),
            channels: Default::default(),
            connection_channels: Default::default(),
            next_sequence_send: Default::default(),
            next_sequence_recv: Default::default(),
            next_sequence_ack: Default::default(),
            port_capabilities: Default::default(),
            packet_commitment: Default::default(),
            packet_receipt: Default::default(),
            packet_acknowledgement: Default::default(),
            connection_ids_counter: 0,
            channel_ids_counter: 0,
        }
    }

    /// Associates a client record to this context.
    /// Given a client id and a height, registers a new client in the context and also associates
    /// to this client a mock client state and a mock consensus state for height `height`. The type
    /// of this client is implicitly assumed to be Mock.
    pub fn with_client(self, client_id: &ClientId, height: Height) -> Self {
        self.with_client_parametrized(client_id, height, Some(ClientType::Mock), Some(height))
    }

    /// Similar to `with_client`, this function associates a client record to this context, but
    /// additionally permits to parametrize two details of the client. If `client_type` is None,
    /// then the client will have type Mock, otherwise the specified type. If
    /// `consensus_state_height` is None, then the client will be initialized with a consensus
    /// state matching the same height as the client state (`client_state_height`).
    pub fn with_client_parametrized(
        mut self,
        client_id: &ClientId,
        client_state_height: Height,
        client_type: Option<ClientType>,
        consensus_state_height: Option<Height>,
    ) -> Self {
        let cs_height = consensus_state_height.unwrap_or(client_state_height);

        let client_type = client_type.unwrap_or(ClientType::Mock);
        let (client_state, consensus_state) = match client_type {
            // If it's a mock client, create the corresponding mock states.
            ClientType::Mock => (
                Some(MockClientState(MockHeader::new(client_state_height)).into()),
                MockConsensusState::new(MockHeader::new(cs_height)).into(),
            ),
            // If it's a Tendermint client, we need TM states.
            ClientType::Tendermint => {
                let light_block = HostBlock::generate_tm_block(
                    self.host_chain_id.clone(),
                    cs_height.revision_height,
                );
                let consensus_state = AnyConsensusState::from(light_block.clone());
                let client_state =
                    get_dummy_tendermint_client_state(light_block.signed_header.header);

                // Return the tuple.
                (Some(client_state), consensus_state)
            }
        };
        let consensus_states = vec![(cs_height, consensus_state)].into_iter().collect();

        let client_record = MockClientRecord {
            client_type,
            client_state,
            consensus_states,
        };
        self.clients.insert(client_id.clone(), client_record);
        self
    }

    /// Associates a connection to this context.
    pub fn with_connection(
        mut self,
        connection_id: ConnectionId,
        connection_end: ConnectionEnd,
    ) -> Self {
        self.connections.insert(connection_id, connection_end);
        self
    }

    pub fn with_port_capability(mut self, port_id: PortId) -> Self {
        self.port_capabilities.insert(port_id, Capability::new());
        self
    }

    /// Associates a channel (in an arbitrary state) to this context.
    pub fn with_channel(
        self,
        port_id: PortId,
        chan_id: ChannelId,
        channel_end: ChannelEnd,
    ) -> Self {
        let mut channels = self.channels.clone();
        channels.insert((port_id, chan_id), channel_end);
        Self { channels, ..self }
    }

    pub fn with_send_sequence(
        self,
        port_id: PortId,
        chan_id: ChannelId,
        seq_number: Sequence,
    ) -> Self {
        let mut next_sequence_send = self.next_sequence_send.clone();
        next_sequence_send.insert((port_id, chan_id), seq_number);
        Self {
            next_sequence_send,
            ..self
        }
    }

    pub fn with_recv_sequence(
        self,
        port_id: PortId,
        chan_id: ChannelId,
        seq_number: Sequence,
    ) -> Self {
        let mut next_sequence_recv = self.next_sequence_recv.clone();
        next_sequence_recv.insert((port_id, chan_id), seq_number);
        Self {
            next_sequence_recv,
            ..self
        }
    }

    pub fn with_ack_sequence(
        self,
        port_id: PortId,
        chan_id: ChannelId,
        seq_number: Sequence,
    ) -> Self {
        let mut next_sequence_ack = self.next_sequence_send.clone();
        next_sequence_ack.insert((port_id, chan_id), seq_number);
        Self {
            next_sequence_ack,
            ..self
        }
    }

    pub fn with_timestamp(self, timestamp: Timestamp) -> Self {
        Self { timestamp, ..self }
    }

    pub fn with_height(self, target_height: Height) -> Self {
        if target_height.revision_number > self.latest_height.revision_number {
            unimplemented!()
        } else if target_height.revision_number < self.latest_height.revision_number {
            panic!("Cannot rewind history of the chain to a smaller revision number!")
        } else if target_height.revision_height < self.latest_height.revision_height {
            panic!("Cannot rewind history of the chain to a smaller revision height!")
        } else if target_height.revision_height > self.latest_height.revision_height {
            // Repeatedly advance the host chain height till we hit the desired height
            let mut ctx = MockContext { ..self };
            while ctx.latest_height.revision_height < target_height.revision_height {
                ctx.advance_host_chain_height()
            }
            ctx
        } else {
            // Both the revision number and height match
            self
        }
    }

    pub fn with_packet_commitment(
        self,
        port_id: PortId,
        chan_id: ChannelId,
        seq: Sequence,
        data: String,
    ) -> Self {
        let mut packet_commitment = self.packet_commitment.clone();
        packet_commitment.insert((port_id, chan_id, seq), data);
        Self {
            packet_commitment,
            ..self
        }
    }

    /// Accessor for a block of the local (host) chain from this context.
    /// Returns `None` if the block at the requested height does not exist.
    fn host_block(&self, target_height: Height) -> Option<&HostBlock> {
        let target = target_height.revision_height as usize;
        let latest = self.latest_height.revision_height as usize;

        // Check that the block is not too advanced, nor has it been pruned.
        if (target > latest) || (target <= latest - self.history.len()) {
            None // Block for requested height does not exist in history.
        } else {
            Some(&self.history[self.history.len() + target - latest - 1])
        }
    }

    /// Triggers the advancing of the host chain, by extending the history of blocks (or headers).
    pub fn advance_host_chain_height(&mut self) {
        let new_block = HostBlock::generate_block(
            self.host_chain_id.clone(),
            self.host_chain_type,
            self.latest_height.increment().revision_height,
        );

        // Append the new header at the tip of the history.
        if self.history.len() >= self.max_history_size {
            // History is full, we rotate and replace the tip with the new header.
            self.history.rotate_left(1);
            self.history[self.max_history_size - 1] = new_block;
        } else {
            // History is not full yet.
            self.history.push(new_block);
        }
        self.latest_height = self.latest_height.increment();
    }

    /// A datagram passes from the relayer to the IBC module (on host chain).
    /// Alternative method to `Ics18Context::send` that does not exercise any serialization.
    /// Used in testing the Ics18 algorithms, hence this may return a Ics18Error.
    pub fn deliver(&mut self, msg: Ics26Envelope) -> Result<(), Ics18Error> {
        dispatch(self, msg).map_err(Ics18Error::transaction_failed)?;
        // Create a new block.
        self.advance_host_chain_height();
        Ok(())
    }

    /// Validates this context. Should be called after the context is mutated by a test.
    pub fn validate(&self) -> Result<(), String> {
        // Check that the number of entries is not higher than window size.
        if self.history.len() > self.max_history_size {
            return Err("too many entries".to_string());
        }

        // Check the content of the history.
        if !self.history.is_empty() {
            // Get the highest block.
            let lh = &self.history[self.history.len() - 1];
            // Check latest is properly updated with highest header height.
            if lh.height() != self.latest_height {
                return Err("latest height is not updated".to_string());
            }
        }

        // Check that headers in the history are in sequential order.
        for i in 1..self.history.len() {
            let ph = &self.history[i - 1];
            let h = &self.history[i];
            if ph.height().increment() != h.height() {
                return Err("headers in history not sequential".to_string());
            }
        }
        Ok(())
    }

    pub fn add_port(&mut self, port_id: PortId) {
        self.port_capabilities.insert(port_id, Capability::new());
    }

    pub fn consensus_states(&self, client_id: &ClientId) -> Vec<AnyConsensusStateWithHeight> {
        self.clients[client_id]
            .consensus_states
            .iter()
            .map(|(k, v)| AnyConsensusStateWithHeight {
                height: *k,
                consensus_state: v.clone(),
            })
            .collect()
    }
}

impl Ics26Context for MockContext {}

impl Ics20Context for MockContext {}

impl PortReader for MockContext {
    fn lookup_module_by_port(&self, port_id: &PortId) -> Result<Capability, Ics05Error> {
        match self.port_capabilities.get(port_id) {
            Some(cap) => Ok(cap.clone()),
            None => Err(Ics05Error::unknown_port(port_id.clone())),
        }
    }

    fn authenticate(&self, _cap: &Capability, _port_id: &PortId) -> bool {
        true
    }
}

impl ChannelReader for MockContext {
    fn channel_end(&self, pcid: &(PortId, ChannelId)) -> Result<ChannelEnd, Ics04Error> {
        match self.channels.get(pcid) {
            Some(channel_end) => Ok(channel_end.clone()),
            None => Err(Ics04Error::channel_not_found(
                pcid.0.clone(),
                pcid.1.clone(),
            )),
        }
    }

    fn connection_end(&self, cid: &ConnectionId) -> Result<ConnectionEnd, Ics04Error> {
        match self.connections.get(cid) {
            Some(connection_end) => Ok(connection_end.clone()),
            None => Err(Ics04Error::missing_connection(cid.clone())),
        }
    }

    fn connection_channels(
        &self,
        cid: &ConnectionId,
    ) -> Result<Vec<(PortId, ChannelId)>, Ics04Error> {
        match self.connection_channels.get(cid) {
            Some(pcid) => Ok(pcid.clone()),
            None => Err(Ics04Error::missing_channel()),
        }
    }

    fn client_state(&self, client_id: &ClientId) -> Result<AnyClientState, Ics04Error> {
        ClientReader::client_state(self, client_id)
            .map_err(|_| Ics04Error::missing_client_state(client_id.clone()))
    }

    fn client_consensus_state(
        &self,
        client_id: &ClientId,
        height: Height,
    ) -> Result<AnyConsensusState, Ics04Error> {
        ClientReader::consensus_state(self, client_id, height)
            .map_err(|_| Ics04Error::missing_client_consensus_state(client_id.clone(), height))
    }

    fn authenticated_capability(&self, port_id: &PortId) -> Result<Capability, Ics04Error> {
        match PortReader::lookup_module_by_port(self, port_id) {
            Ok(key) => {
                if !PortReader::authenticate(self, &key, port_id) {
                    Err(Ics04Error::invalid_port_capability())
                } else {
                    Ok(key)
                }
            }
            Err(e) if e.detail() == Ics05Error::unknown_port(port_id.clone()).detail() => {
                Err(Ics04Error::no_port_capability(port_id.clone()))
            }
            Err(_) => Err(Ics04Error::implementation_specific()),
        }
    }

    fn get_next_sequence_send(
        &self,
        port_channel_id: &(PortId, ChannelId),
    ) -> Result<Sequence, Ics04Error> {
        match self.next_sequence_send.get(port_channel_id) {
            Some(sequence) => Ok(*sequence),
            None => Err(Ics04Error::missing_next_send_seq()),
        }
    }

    fn get_next_sequence_recv(
        &self,
        port_channel_id: &(PortId, ChannelId),
    ) -> Result<Sequence, Ics04Error> {
        match self.next_sequence_recv.get(port_channel_id) {
            Some(sequence) => Ok(*sequence),
            None => Err(Ics04Error::missing_next_recv_seq()),
        }
    }

    fn get_next_sequence_ack(
        &self,
        port_channel_id: &(PortId, ChannelId),
    ) -> Result<Sequence, Ics04Error> {
        match self.next_sequence_ack.get(port_channel_id) {
            Some(sequence) => Ok(*sequence),
            None => Err(Ics04Error::missing_next_ack_seq()),
        }
    }

    fn get_packet_commitment(
        &self,
        key: &(PortId, ChannelId, Sequence),
    ) -> Result<String, Ics04Error> {
        match self.packet_commitment.get(key) {
            Some(commitment) => Ok(commitment.clone()),
            None => Err(Ics04Error::packet_commitment_not_found(key.2)),
        }
    }

    fn get_packet_receipt(
        &self,
        key: &(PortId, ChannelId, Sequence),
    ) -> Result<Receipt, Ics04Error> {
        match self.packet_receipt.get(key) {
            Some(receipt) => Ok(receipt.clone()),
            None => Err(Ics04Error::packet_receipt_not_found(key.2)),
        }
    }

    fn get_packet_acknowledgement(
        &self,
        key: &(PortId, ChannelId, Sequence),
    ) -> Result<String, Ics04Error> {
        match self.packet_acknowledgement.get(key) {
            Some(ack) => Ok(ack.clone()),
            None => Err(Ics04Error::packet_acknowledgement_not_found(key.2)),
        }
    }

    fn hash(&self, input: String) -> String {
        let r = sha2::Sha256::digest(input.as_bytes());
        format!("{:x}", r)
    }

    fn host_height(&self) -> Height {
        self.latest_height
    }

    fn host_timestamp(&self) -> Timestamp {
        self.timestamp
    }

    fn channel_counter(&self) -> Result<u64, Ics04Error> {
        Ok(self.channel_ids_counter)
    }
}

impl ChannelKeeper for MockContext {
    fn store_packet_commitment(
        &mut self,
        key: (PortId, ChannelId, Sequence),
        timeout_timestamp: Timestamp,
        timeout_height: Height,
        data: Vec<u8>,
    ) -> Result<(), Ics04Error> {
        let input = format!("{:?},{:?},{:?}", timeout_timestamp, timeout_height, data);
        self.packet_commitment
            .insert(key, ChannelReader::hash(self, input));
        Ok(())
    }

    fn store_packet_acknowledgement(
        &mut self,
        key: (PortId, ChannelId, Sequence),
        ack: Vec<u8>,
    ) -> Result<(), Ics04Error> {
        let input = format!("{:?}", ack);
        self.packet_acknowledgement
            .insert(key, ChannelReader::hash(self, input));
        Ok(())
    }

    fn delete_packet_acknowledgement(
        &mut self,
        key: (PortId, ChannelId, Sequence),
    ) -> Result<(), Ics04Error> {
        self.packet_acknowledgement.remove(&key);
        Ok(())
    }

    fn store_connection_channels(
        &mut self,
        cid: ConnectionId,
        port_channel_id: &(PortId, ChannelId),
    ) -> Result<(), Ics04Error> {
        self.connection_channels
            .entry(cid)
            .or_insert_with(Vec::new)
            .push(port_channel_id.clone());
        Ok(())
    }

    fn store_channel(
        &mut self,
        port_channel_id: (PortId, ChannelId),
        channel_end: &ChannelEnd,
    ) -> Result<(), Ics04Error> {
        self.channels.insert(port_channel_id, channel_end.clone());
        Ok(())
    }

    fn store_next_sequence_send(
        &mut self,
        port_channel_id: (PortId, ChannelId),
        seq: Sequence,
    ) -> Result<(), Ics04Error> {
        self.next_sequence_send.insert(port_channel_id, seq);
        Ok(())
    }

    fn store_next_sequence_recv(
        &mut self,
        port_channel_id: (PortId, ChannelId),
        seq: Sequence,
    ) -> Result<(), Ics04Error> {
        self.next_sequence_recv.insert(port_channel_id, seq);
        Ok(())
    }

    fn store_next_sequence_ack(
        &mut self,
        port_channel_id: (PortId, ChannelId),
        seq: Sequence,
    ) -> Result<(), Ics04Error> {
        self.next_sequence_ack.insert(port_channel_id, seq);
        Ok(())
    }

    fn increase_channel_counter(&mut self) {
        self.channel_ids_counter += 1;
    }

    fn delete_packet_commitment(
        &mut self,
        key: (PortId, ChannelId, Sequence),
    ) -> Result<(), Ics04Error> {
        self.packet_commitment.remove(&key);
        Ok(())
    }

    fn store_packet_receipt(
        &mut self,
        key: (PortId, ChannelId, Sequence),
        receipt: Receipt,
    ) -> Result<(), Ics04Error> {
        self.packet_receipt.insert(key, receipt);
        Ok(())
    }
}

impl ConnectionReader for MockContext {
    fn connection_end(&self, cid: &ConnectionId) -> Result<ConnectionEnd, Ics03Error> {
        match self.connections.get(cid) {
            Some(connection_end) => Ok(connection_end.clone()),
            None => Err(Ics03Error::connection_not_found(cid.clone())),
        }
    }

    fn client_state(&self, client_id: &ClientId) -> Result<AnyClientState, Ics03Error> {
        // Forward method call to the Ics2 Client-specific method.
        ClientReader::client_state(self, client_id)
            .map_err(|_| Ics03Error::missing_client(client_id.clone()))
    }

    fn host_current_height(&self) -> Height {
        self.latest_height
    }

    fn host_oldest_height(&self) -> Height {
        // history must be non-empty, so `self.history[0]` is valid
        self.history[0].height()
    }

    fn commitment_prefix(&self) -> CommitmentPrefix {
        CommitmentPrefix::from(vec![])
    }

    fn client_consensus_state(
        &self,
        client_id: &ClientId,
        height: Height,
    ) -> Result<AnyConsensusState, Ics03Error> {
        // Forward method call to the Ics2Client-specific method.
        self.consensus_state(client_id, height)
            .map_err(|_| Ics03Error::missing_client_consensus_state(height, client_id.clone()))
    }

    fn host_consensus_state(&self, height: Height) -> Result<AnyConsensusState, Ics03Error> {
        match self.host_block(height) {
            Some(block_ref) => Ok(block_ref.clone().into()),
            None => Err(Ics03Error::missing_local_consensus_state(height)),
        }
    }

    fn connection_counter(&self) -> Result<u64, Ics03Error> {
        Ok(self.connection_ids_counter)
    }
}

impl ConnectionKeeper for MockContext {
    fn store_connection(
        &mut self,
        connection_id: ConnectionId,
        connection_end: &ConnectionEnd,
    ) -> Result<(), Ics03Error> {
        self.connections
            .insert(connection_id, connection_end.clone());
        Ok(())
    }

    fn store_connection_to_client(
        &mut self,
        connection_id: ConnectionId,
        client_id: &ClientId,
    ) -> Result<(), Ics03Error> {
        self.client_connections
            .insert(client_id.clone(), connection_id);
        Ok(())
    }

    fn increase_connection_counter(&mut self) {
        self.connection_ids_counter += 1;
    }
}

impl ClientReader for MockContext {
    fn client_type(&self, client_id: &ClientId) -> Result<ClientType, Ics02Error> {
        match self.clients.get(client_id) {
            Some(client_record) => Ok(client_record.client_type),
            None => Err(Ics02Error::client_not_found(client_id.clone())),
        }
    }

    fn client_state(&self, client_id: &ClientId) -> Result<AnyClientState, Ics02Error> {
        match self.clients.get(client_id) {
            Some(client_record) => client_record
                .client_state
                .clone()
                .ok_or_else(|| Ics02Error::client_not_found(client_id.clone())),
            None => Err(Ics02Error::client_not_found(client_id.clone())),
        }
    }

    fn consensus_state(
        &self,
        client_id: &ClientId,
        height: Height,
    ) -> Result<AnyConsensusState, Ics02Error> {
        match self.clients.get(client_id) {
            Some(client_record) => match client_record.consensus_states.get(&height) {
                Some(consensus_state) => Ok(consensus_state.clone()),
                None => Err(Ics02Error::consensus_state_not_found(
                    client_id.clone(),
                    height,
                )),
            },
            None => Err(Ics02Error::consensus_state_not_found(
                client_id.clone(),
                height,
            )),
        }
    }

    fn client_counter(&self) -> Result<u64, Ics02Error> {
        Ok(self.client_ids_counter)
    }
}

impl ClientKeeper for MockContext {
    fn store_client_type(
        &mut self,
        client_id: ClientId,
        client_type: ClientType,
    ) -> Result<(), Ics02Error> {
        let mut client_record = self.clients.entry(client_id).or_insert(MockClientRecord {
            client_type,
            consensus_states: Default::default(),
            client_state: Default::default(),
        });

        client_record.client_type = client_type;
        Ok(())
    }

    fn store_client_state(
        &mut self,
        client_id: ClientId,
        client_state: AnyClientState,
    ) -> Result<(), Ics02Error> {
        let mut client_record = self.clients.entry(client_id).or_insert(MockClientRecord {
            client_type: client_state.client_type(),
            consensus_states: Default::default(),
            client_state: Default::default(),
        });

        client_record.client_state = Some(client_state);
        Ok(())
    }

    fn store_consensus_state(
        &mut self,
        client_id: ClientId,
        height: Height,
        consensus_state: AnyConsensusState,
    ) -> Result<(), Ics02Error> {
        let client_record = self.clients.entry(client_id).or_insert(MockClientRecord {
            client_type: ClientType::Mock,
            consensus_states: Default::default(),
            client_state: Default::default(),
        });

        client_record
            .consensus_states
            .insert(height, consensus_state);
        Ok(())
    }

    fn increase_client_counter(&mut self) {
        self.client_ids_counter += 1
    }
}

impl Ics18Context for MockContext {
    fn query_latest_height(&self) -> Height {
        self.host_current_height()
    }

    fn query_client_full_state(&self, client_id: &ClientId) -> Option<AnyClientState> {
        // Forward call to Ics2.
        ClientReader::client_state(self, client_id).ok()
    }

    fn query_latest_header(&self) -> Option<AnyHeader> {
        let block_ref = self.host_block(self.host_current_height());
        block_ref.cloned().map(Into::into)
    }

    fn send(&mut self, msgs: Vec<Any>) -> Result<Vec<IbcEvent>, Ics18Error> {
        // Forward call to Ics26 delivery method.
        let events = deliver(self, msgs).map_err(Ics18Error::transaction_failed)?;

        self.advance_host_chain_height(); // Advance chain height
        Ok(events)
    }

    fn signer(&self) -> Signer {
        "0CDA3F47EF3C4906693B170EF650EB968C5F4B2C".parse().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use crate::ics24_host::identifier::ChainId;
    use crate::mock::context::MockContext;
    use crate::mock::host::HostType;
    use crate::Height;
    use test_env_log::test;

    #[test]
    fn test_history_manipulation() {
        pub struct Test {
            name: String,
            ctx: MockContext,
        }
        let cv = 1; // The version to use for all chains.

        let tests: Vec<Test> = vec![
            Test {
                name: "Empty history, small pruning window".to_string(),
                ctx: MockContext::new(
                    ChainId::new("mockgaia".to_string(), cv),
                    HostType::Mock,
                    1,
                    Height::new(cv, 0),
                ),
            },
            Test {
                name: "[Synthetic TM host] Empty history, small pruning window".to_string(),
                ctx: MockContext::new(
                    ChainId::new("mocksgaia".to_string(), cv),
                    HostType::SyntheticTendermint,
                    1,
                    Height::new(cv, 0),
                ),
            },
            Test {
                name: "Large pruning window".to_string(),
                ctx: MockContext::new(
                    ChainId::new("mockgaia".to_string(), cv),
                    HostType::Mock,
                    30,
                    Height::new(cv, 2),
                ),
            },
            Test {
                name: "[Synthetic TM host] Large pruning window".to_string(),
                ctx: MockContext::new(
                    ChainId::new("mocksgaia".to_string(), cv),
                    HostType::SyntheticTendermint,
                    30,
                    Height::new(cv, 2),
                ),
            },
            Test {
                name: "Small pruning window".to_string(),
                ctx: MockContext::new(
                    ChainId::new("mockgaia".to_string(), cv),
                    HostType::Mock,
                    3,
                    Height::new(cv, 30),
                ),
            },
            Test {
                name: "[Synthetic TM host] Small pruning window".to_string(),
                ctx: MockContext::new(
                    ChainId::new("mockgaia".to_string(), cv),
                    HostType::SyntheticTendermint,
                    3,
                    Height::new(cv, 30),
                ),
            },
            Test {
                name: "Small pruning window, small starting height".to_string(),
                ctx: MockContext::new(
                    ChainId::new("mockgaia".to_string(), cv),
                    HostType::Mock,
                    3,
                    Height::new(cv, 2),
                ),
            },
            Test {
                name: "[Synthetic TM host] Small pruning window, small starting height".to_string(),
                ctx: MockContext::new(
                    ChainId::new("mockgaia".to_string(), cv),
                    HostType::SyntheticTendermint,
                    3,
                    Height::new(cv, 2),
                ),
            },
            Test {
                name: "Large pruning window, large starting height".to_string(),
                ctx: MockContext::new(
                    ChainId::new("mockgaia".to_string(), cv),
                    HostType::Mock,
                    50,
                    Height::new(cv, 2000),
                ),
            },
            Test {
                name: "[Synthetic TM host] Large pruning window, large starting height".to_string(),
                ctx: MockContext::new(
                    ChainId::new("mockgaia".to_string(), cv),
                    HostType::SyntheticTendermint,
                    50,
                    Height::new(cv, 2000),
                ),
            },
        ];

        for mut test in tests {
            // All tests should yield a valid context after initialization.
            assert!(
                test.ctx.validate().is_ok(),
                "Failed in test {} while validating context {:?}",
                test.name,
                test.ctx
            );

            let current_height = test.ctx.latest_height;

            // After advancing the chain's height, the context should still be valid.
            test.ctx.advance_host_chain_height();
            assert!(
                test.ctx.validate().is_ok(),
                "Failed in test {} while validating context {:?}",
                test.name,
                test.ctx
            );

            let next_height = current_height.increment();
            assert_eq!(
                test.ctx.latest_height, next_height,
                "Failed while increasing height for context {:?}",
                test.ctx
            );
            if current_height > Height::new(cv, 0) {
                assert_eq!(
                    test.ctx.host_block(current_height).unwrap().height(),
                    current_height,
                    "Failed while fetching height {:?} of context {:?}",
                    current_height,
                    test.ctx
                );
            }
        }
    }
}
