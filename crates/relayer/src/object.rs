use std::fmt::Display;

use flex_error::define_error;
use serde::{Deserialize, Serialize};

use ibc_relayer_types::applications::ics29_fee::events::IncentivizedPacket;
use ibc_relayer_types::core::{
    ics02_client::events::UpdateClient,
    ics03_connection::events::Attributes as ConnectionAttributes,
    ics04_channel::events::{
        Attributes, CloseInit, SendPacket, TimeoutPacket, WriteAcknowledgement,
    },
    ics24_host::identifier::{ChainId, ChannelId, ClientId, ConnectionId, PortId},
};

use crate::chain::{
    counterparty::{
        channel_connection_client, channel_connection_client_no_checks,
        counterparty_chain_from_channel, counterparty_chain_from_connection,
    },
    handle::ChainHandle,
    requests::{IncludeProof, QueryClientStateRequest, QueryHeight},
};
use crate::error::Error as RelayerError;
use crate::supervisor::Error as SupervisorError;

/// Client
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Client {
    /// Destination chain identifier.
    /// This is the chain hosting the client.
    pub dst_chain_id: ChainId,

    /// Client identifier (allocated on the destination chain `dst_chain_id`).
    pub dst_client_id: ClientId,

    /// Source chain identifier.
    /// This is the chain whose headers the client worker is verifying.
    pub src_chain_id: ChainId,
}

impl Client {
    pub fn short_name(&self) -> String {
        format!(
            "client::{}->{}:{}",
            self.src_chain_id, self.dst_chain_id, self.dst_client_id
        )
    }
}

/// Connection
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Connection {
    /// Destination chain identifier.
    pub dst_chain_id: ChainId,

    /// Source chain identifier.
    pub src_chain_id: ChainId,

    /// Source connection identifier.
    pub src_connection_id: ConnectionId,
}

impl Connection {
    pub fn short_name(&self) -> String {
        format!(
            "connection::{}:{}->{}",
            self.src_connection_id, self.src_chain_id, self.dst_chain_id,
        )
    }
}

/// Channel
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Channel {
    /// Destination chain identifier.
    pub dst_chain_id: ChainId,

    /// Source chain identifier.
    pub src_chain_id: ChainId,

    /// Source channel identifier.
    pub src_channel_id: ChannelId,

    /// Source port identifier.
    pub src_port_id: PortId,
}

impl Channel {
    pub fn short_name(&self) -> String {
        format!(
            "channel::{}/{}:{}->{}",
            self.src_channel_id, self.src_port_id, self.src_chain_id, self.dst_chain_id,
        )
    }
}

/// A packet worker between a source and destination chain, and a specific channel and port.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Packet {
    /// Destination chain identifier.
    pub dst_chain_id: ChainId,

    /// Source chain identifier.
    pub src_chain_id: ChainId,

    /// Source channel identifier.
    pub src_channel_id: ChannelId,

    /// Source port identifier.
    pub src_port_id: PortId,
}

impl Packet {
    pub fn short_name(&self) -> String {
        format!(
            "packet::{}/{}:{}->{}",
            self.src_channel_id, self.src_port_id, self.src_chain_id, self.dst_chain_id,
        )
    }
}

/// A wallet worker which monitors the balance of the wallet in use by Hermes
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Wallet {
    /// Chain identifier
    pub chain_id: ChainId,
}

impl Wallet {
    pub fn short_name(&self) -> String {
        format!("wallet::{}", self.chain_id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CrossChainQuery {
    pub src_chain_id: ChainId,
    pub dst_chain_id: ChainId,
    pub query_id: String,
    pub connection_id: ConnectionId,
}

impl CrossChainQuery {
    pub fn short_name(&self) -> String {
        format!("cross_chain_query::{}/{}", self.dst_chain_id, self.query_id)
    }
}

/// An object determines the amount of parallelism that can
/// be exercised when processing [`IbcEvent`](ibc_relayer_types::events::IbcEvent)
/// between two chains. For each [`Object`], a corresponding
/// [`WorkerHandle`](crate::worker::WorkerHandle) is spawned and all [`IbcEvent`](ibc_relayer_types::events::IbcEvent)s mapped
/// to an [`Object`] are sent to the associated [`WorkerHandle`](crate::worker::WorkerHandle)
/// for processing.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Object {
    /// See [`Client`].
    Client(Client),
    /// See [`Connection`].
    Connection(Connection),
    /// See [`Channel`].
    Channel(Channel),
    /// See [`Packet`].
    Packet(Packet),
    /// See [`Wallet`]
    Wallet(Wallet),
    /// See [`CrossChainQuery`]
    CrossChainQuery(CrossChainQuery),
}

define_error! {
    ObjectError {
        Relayer
            [ RelayerError ]
            | _ | { "relayer error" },

        Supervisor
            [ SupervisorError ]
            | _ | { "supervisor error" },

        RefreshNotRequired
            {
                client_id: ClientId,
                chain_id: ChainId,
            }
            | e | {
                format!("client '{}' on chain {} does not require refresh",
                    e.client_id, e.chain_id)
            },

        MissingChannelId
            { event: Attributes }
            | e | {
                format!("channel_id missing in channel open event '{:?}'",
                    e.event)
            },

        MissingConnectionId
            { event: ConnectionAttributes }
            | e | {
                format!("connection_id missing from connection handshake event '{:?}'",
                    e.event)
            },
    }
}

impl Object {
    /// Returns `true` if this [`Object`] is for a [`WorkerHandle`](crate::worker::WorkerHandle)
    /// which is interested in new block events originating from the chain with
    /// the given [`ChainId`]. Returns `false` otherwise.
    pub fn notify_new_block(&self, src_chain_id: &ChainId) -> bool {
        match self {
            Object::Client(_) => false,
            Object::Connection(c) => &c.src_chain_id == src_chain_id,
            Object::Channel(c) => &c.src_chain_id == src_chain_id,
            Object::Packet(p) => &p.src_chain_id == src_chain_id,
            Object::Wallet(_) => false,
            Object::CrossChainQuery(c) => &c.src_chain_id == src_chain_id,
        }
    }

    /// Returns whether or not this object pertains to the given chain.
    pub fn for_chain(&self, chain_id: &ChainId) -> bool {
        match self {
            Object::Client(c) => &c.src_chain_id == chain_id || &c.dst_chain_id == chain_id,
            Object::Connection(c) => &c.src_chain_id == chain_id || &c.dst_chain_id == chain_id,
            Object::Channel(c) => &c.src_chain_id == chain_id || &c.dst_chain_id == chain_id,
            Object::Packet(p) => &p.src_chain_id == chain_id || &p.dst_chain_id == chain_id,
            Object::Wallet(w) => &w.chain_id == chain_id,
            Object::CrossChainQuery(c) => {
                &c.src_chain_id == chain_id || &c.dst_chain_id == chain_id
            }
        }
    }

    /// Return the type of object
    pub fn object_type(&self) -> ObjectType {
        match self {
            Object::Client(_) => ObjectType::Client,
            Object::Channel(_) => ObjectType::Channel,
            Object::Connection(_) => ObjectType::Connection,
            Object::Packet(_) => ObjectType::Packet,
            Object::Wallet(_) => ObjectType::Wallet,
            Object::CrossChainQuery(_) => ObjectType::CrossChainQuery,
        }
    }
}

/// The type of [`Object`].
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ObjectType {
    Client,
    Channel,
    Connection,
    Packet,
    Wallet,
    CrossChainQuery,
}

impl Display for ObjectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ObjectType::Client => write!(f, "client"),
            ObjectType::Channel => write!(f, "channel"),
            ObjectType::Connection => write!(f, "connection"),
            ObjectType::Packet => write!(f, "packet"),
            ObjectType::Wallet => write!(f, "wallet"),
            ObjectType::CrossChainQuery => write!(f, "cross_chain_query"),
        }
    }
}

impl From<Client> for Object {
    fn from(c: Client) -> Self {
        Self::Client(c)
    }
}

impl From<Connection> for Object {
    fn from(c: Connection) -> Self {
        Self::Connection(c)
    }
}

impl From<Channel> for Object {
    fn from(c: Channel) -> Self {
        Self::Channel(c)
    }
}

impl From<Packet> for Object {
    fn from(p: Packet) -> Self {
        Self::Packet(p)
    }
}

impl From<Wallet> for Object {
    fn from(w: Wallet) -> Self {
        Self::Wallet(w)
    }
}

impl From<CrossChainQuery> for Object {
    fn from(c: CrossChainQuery) -> Self {
        Self::CrossChainQuery(c)
    }
}

impl Object {
    pub fn src_chain_id(&self) -> &ChainId {
        match self {
            Self::Client(ref client) => &client.src_chain_id,
            Self::Connection(ref connection) => &connection.src_chain_id,
            Self::Channel(ref channel) => &channel.src_chain_id,
            Self::Packet(ref path) => &path.src_chain_id,
            Self::Wallet(ref wallet) => &wallet.chain_id,
            Self::CrossChainQuery(ref query) => &query.src_chain_id,
        }
    }

    pub fn dst_chain_id(&self) -> &ChainId {
        match self {
            Self::Client(ref client) => &client.dst_chain_id,
            Self::Connection(ref connection) => &connection.dst_chain_id,
            Self::Channel(ref channel) => &channel.dst_chain_id,
            Self::Packet(ref path) => &path.dst_chain_id,
            Self::Wallet(ref wallet) => &wallet.chain_id,
            Self::CrossChainQuery(ref query) => &query.dst_chain_id,
        }
    }

    pub fn short_name(&self) -> String {
        match self {
            Self::Client(ref client) => client.short_name(),
            Self::Connection(ref connection) => connection.short_name(),
            Self::Channel(ref channel) => channel.short_name(),
            Self::Packet(ref path) => path.short_name(),
            Self::Wallet(ref wallet) => wallet.short_name(),
            Self::CrossChainQuery(ref query) => query.short_name(),
        }
    }

    /// Build the object associated with the given [`UpdateClient`] event.
    pub fn for_update_client(
        e: &UpdateClient,
        dst_chain: &impl ChainHandle,
    ) -> Result<Self, ObjectError> {
        let (client_state, _) = dst_chain
            .query_client_state(
                QueryClientStateRequest {
                    client_id: e.client_id().clone(),
                    height: QueryHeight::Latest,
                },
                IncludeProof::No,
            )
            .map_err(ObjectError::relayer)?;

        if client_state.refresh_period().is_none() {
            return Err(ObjectError::refresh_not_required(
                e.client_id().clone(),
                dst_chain.id(),
            ));
        }
        let src_chain_id = client_state.chain_id();

        Ok(Client {
            dst_client_id: e.client_id().clone(),
            dst_chain_id: dst_chain.id(),
            src_chain_id,
        }
        .into())
    }

    /// Build the client object associated with the given channel event attributes.
    pub fn client_from_chan_open_events(
        e: &Attributes,           // The attributes of the emitted event
        chain: &impl ChainHandle, // The chain which emitted the event
    ) -> Result<Self, ObjectError> {
        let channel_id = e
            .channel_id()
            .ok_or_else(|| ObjectError::missing_channel_id(e.clone()))?;

        let client = channel_connection_client(chain, e.port_id(), channel_id)
            .map_err(ObjectError::supervisor)?
            .client;

        if client.client_state.refresh_period().is_none() {
            return Err(ObjectError::refresh_not_required(
                client.client_id,
                chain.id(),
            ));
        }

        Ok(Client {
            dst_client_id: client.client_id.clone(),
            dst_chain_id: chain.id(), // The object's destination is the chain hosting the client
            src_chain_id: client.client_state.chain_id(),
        }
        .into())
    }

    /// Build the Connection object associated with the given
    /// [`Open`](ibc_relayer_types::core::ics03_connection::connection::State::Open)
    /// connection event.
    pub fn connection_from_conn_open_events(
        e: &ConnectionAttributes,
        src_chain: &impl ChainHandle,
    ) -> Result<Self, ObjectError> {
        let connection_id = e
            .connection_id
            .as_ref()
            .ok_or_else(|| ObjectError::missing_connection_id(e.clone()))?;

        let dst_chain_id = counterparty_chain_from_connection(src_chain, connection_id)
            .map_err(ObjectError::supervisor)?;

        Ok(Connection {
            dst_chain_id,
            src_chain_id: src_chain.id(),
            src_connection_id: connection_id.clone(),
        }
        .into())
    }

    /// Build the Channel object associated with the given
    /// [`Open`](ibc_relayer_types::core::ics04_channel::channel::State::Open) channel event.
    pub fn channel_from_chan_open_events(
        attributes: &Attributes,
        src_chain: &impl ChainHandle,
        allow_non_open_connection: bool,
    ) -> Result<Self, ObjectError> {
        let channel_id = attributes
            .channel_id()
            .ok_or_else(|| ObjectError::missing_channel_id(attributes.clone()))?;

        let port_id = attributes.port_id();

        let dst_chain_id = if allow_non_open_connection {
            // Get the destination chain allowing for the possibility that the connection is not yet open.
            // This is to support the optimistic channel handshake by allowing the channel worker to get
            // the channel events while the connection is being established.
            // The channel worker will eventually finish the channel handshake via the retry mechanism.
            channel_connection_client_no_checks(src_chain, port_id, channel_id)
                .map(|c| c.client.client_state.chain_id())
                .map_err(ObjectError::supervisor)?
        } else {
            channel_connection_client(src_chain, port_id, channel_id)
                .map(|c| c.client.client_state.chain_id())
                .map_err(ObjectError::supervisor)?
        };

        Ok(Channel {
            dst_chain_id,
            src_chain_id: src_chain.id(),
            src_channel_id: channel_id.clone(),
            src_port_id: port_id.clone(),
        }
        .into())
    }

    /// Build the object associated with the given [`SendPacket`] event.
    pub fn for_send_packet(
        e: &SendPacket,
        src_chain: &impl ChainHandle,
    ) -> Result<Self, ObjectError> {
        let dst_chain_id = counterparty_chain_from_channel(
            src_chain,
            &e.packet.source_channel,
            &e.packet.source_port,
        )
        .map_err(ObjectError::supervisor)?;

        Ok(Packet {
            dst_chain_id,
            src_chain_id: src_chain.id(),
            src_channel_id: e.packet.source_channel.clone(),
            src_port_id: e.packet.source_port.clone(),
        }
        .into())
    }

    /// Build the object associated with the given [`WriteAcknowledgement`] event.
    pub fn for_write_ack(
        e: &WriteAcknowledgement,
        src_chain: &impl ChainHandle,
    ) -> Result<Self, ObjectError> {
        let dst_chain_id = counterparty_chain_from_channel(
            src_chain,
            &e.packet.destination_channel,
            &e.packet.destination_port,
        )
        .map_err(ObjectError::supervisor)?;

        Ok(Packet {
            dst_chain_id,
            src_chain_id: src_chain.id(),
            src_channel_id: e.packet.destination_channel.clone(),
            src_port_id: e.packet.destination_port.clone(),
        }
        .into())
    }

    /// Build the object associated with the given [`TimeoutPacket`] event.
    pub fn for_timeout_packet(
        e: &TimeoutPacket,
        src_chain: &impl ChainHandle,
    ) -> Result<Self, ObjectError> {
        let dst_chain_id = counterparty_chain_from_channel(
            src_chain,
            &e.packet.source_channel,
            &e.packet.source_port,
        )
        .map_err(ObjectError::supervisor)?;

        Ok(Packet {
            dst_chain_id,
            src_chain_id: src_chain.id(),
            src_channel_id: e.src_channel_id().clone(),
            src_port_id: e.src_port_id().clone(),
        }
        .into())
    }

    /// Build the object associated with the given [`CloseInit`] event.
    pub fn for_close_init_channel(
        e: &CloseInit,
        src_chain: &impl ChainHandle,
    ) -> Result<Self, ObjectError> {
        let dst_chain_id = counterparty_chain_from_channel(src_chain, e.channel_id(), e.port_id())
            .map_err(ObjectError::supervisor)?;

        Ok(Packet {
            dst_chain_id,
            src_chain_id: src_chain.id(),
            src_channel_id: e.channel_id().clone(),
            src_port_id: e.port_id().clone(),
        }
        .into())
    }

    /// Build the object associated with the given [`CrossChainQuery`] event.
    pub fn for_cross_chain_query_packet(
        p: &ibc_relayer_types::applications::ics31_icq::events::CrossChainQueryPacket,
        src_chain: &impl ChainHandle,
    ) -> Result<Self, ObjectError> {
        let dst_chain_id = counterparty_chain_from_connection(&src_chain.clone(), &p.connection_id)
            .map_err(ObjectError::supervisor)?;
        Ok(CrossChainQuery {
            src_chain_id: src_chain.clone().id(),
            dst_chain_id,
            query_id: p.query_id.to_string(),
            connection_id: p.connection_id.clone(),
        }
        .into())
    }

    /// Build the object associated with the given [`IncentivizedPacket`] event.
    pub fn for_incentivized_packet(
        e: &IncentivizedPacket,
        src_chain: &impl ChainHandle,
    ) -> Result<Self, ObjectError> {
        let dst_chain_id = counterparty_chain_from_channel(src_chain, &e.channel_id, &e.port_id)
            .map_err(ObjectError::supervisor)?;

        Ok(Packet {
            dst_chain_id,
            src_chain_id: src_chain.id(),
            src_channel_id: e.channel_id.clone(),
            src_port_id: e.port_id.clone(),
        }
        .into())
    }
}
