use crate::prelude::*;

use core::convert::{TryFrom, TryInto};
use core::fmt;
use core::str::FromStr;
use flex_error::{define_error, TraceError};
use prost::alloc::fmt::Formatter;
use serde_derive::{Deserialize, Serialize};
use tendermint::abci::tag::Tag;
use tendermint::abci::Event as AbciEvent;

use crate::core::ics02_client::error as client_error;
use crate::core::ics02_client::events::NewBlock;
use crate::core::ics02_client::events::{self as ClientEvents};
use crate::core::ics03_connection::error as connection_error;
use crate::core::ics03_connection::events as ConnectionEvents;
use crate::core::ics03_connection::events::Attributes as ConnectionAttributes;
use crate::core::ics04_channel::error as channel_error;
use crate::core::ics04_channel::events as ChannelEvents;
use crate::core::ics04_channel::events::Attributes as ChannelAttributes;
use crate::core::ics04_channel::packet::Packet;
use crate::core::ics24_host::error::ValidationError;
use crate::core::ics26_routing::context::ModuleId;
use crate::timestamp::ParseTimestampError;

define_error! {
    Error {
        Height
            | _ | { "error parsing height" },

        Parse
            [ ValidationError ]
            | _ | { "parse error" },

        Client
            [ client_error::Error ]
            | _ | { "ICS02 client error" },

        Connection
            [ connection_error::Error ]
            | _ | { "connection error" },

        Channel
            [ channel_error::Error ]
            | _ | { "channel error" },

        Timestamp
            [ ParseTimestampError ]
            | _ | { "error parsing timestamp" },

        MissingKey
            { key: String }
            | e | { format_args!("missing event key {}", e.key) },

        Decode
            [ TraceError<prost::DecodeError> ]
            | _ | { "error decoding protobuf" },

        SubtleEncoding
            [ TraceError<subtle_encoding::Error> ]
            | _ | { "error decoding hex" },

        MissingActionString
            | _ | { "missing action string" },

        IncorrectEventType
            { event: String }
            | e | { format_args!("incorrect event type: {}", e.event) },

        MalformedModuleEvent
            { event: ModuleEvent }
            | e | { format_args!("module event cannot use core event types: {:?}", e.event) },

        UnsupportedAbciEvent
            {event_type: String}
            |e| { format_args!("Unable to parse abci event type '{}' into IbcEvent", e.event_type)}
    }
}

/// Events whose data is not included in the app state and must be extracted using tendermint RPCs
/// (i.e. /tx_search or /block_search)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum WithBlockDataType {
    CreateClient,
    UpdateClient,
    SendPacket,
    WriteAck,
}

impl WithBlockDataType {
    pub fn as_str(&self) -> &'static str {
        match *self {
            WithBlockDataType::CreateClient => "create_client",
            WithBlockDataType::UpdateClient => "update_client",
            WithBlockDataType::SendPacket => "send_packet",
            WithBlockDataType::WriteAck => "write_acknowledgement",
        }
    }
}

const NEW_BLOCK_EVENT: &str = "new_block";
const EMPTY_EVENT: &str = "empty";
const CHAIN_ERROR_EVENT: &str = "chain_error";
const APP_MODULE_EVENT: &str = "app_module";
/// Client event types
const CREATE_CLIENT_EVENT: &str = "create_client";
const UPDATE_CLIENT_EVENT: &str = "update_client";
const CLIENT_MISBEHAVIOUR_EVENT: &str = "client_misbehaviour";
const UPGRADE_CLIENT_EVENT: &str = "upgrade_client";
/// Connection event types
const CONNECTION_INIT_EVENT: &str = "connection_open_init";
const CONNECTION_TRY_EVENT: &str = "connection_open_try";
const CONNECTION_ACK_EVENT: &str = "connection_open_ack";
const CONNECTION_CONFIRM_EVENT: &str = "connection_open_confirm";
/// Channel event types
const CHANNEL_OPEN_INIT_EVENT: &str = "channel_open_init";
const CHANNEL_OPEN_TRY_EVENT: &str = "channel_open_try";
const CHANNEL_OPEN_ACK_EVENT: &str = "channel_open_ack";
const CHANNEL_OPEN_CONFIRM_EVENT: &str = "channel_open_confirm";
const CHANNEL_CLOSE_INIT_EVENT: &str = "channel_close_init";
const CHANNEL_CLOSE_CONFIRM_EVENT: &str = "channel_close_confirm";
/// Packet event types
const SEND_PACKET_EVENT: &str = "send_packet";
const RECEIVE_PACKET_EVENT: &str = "receive_packet";
const WRITE_ACK_EVENT: &str = "write_acknowledgement";
const ACK_PACKET_EVENT: &str = "acknowledge_packet";
const TIMEOUT_EVENT: &str = "timeout_packet";
const TIMEOUT_ON_CLOSE_EVENT: &str = "timeout_packet_on_close";

/// Events types
#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum IbcEventType {
    NewBlock,
    CreateClient,
    UpdateClient,
    UpgradeClient,
    ClientMisbehaviour,
    OpenInitConnection,
    OpenTryConnection,
    OpenAckConnection,
    OpenConfirmConnection,
    OpenInitChannel,
    OpenTryChannel,
    OpenAckChannel,
    OpenConfirmChannel,
    CloseInitChannel,
    CloseConfirmChannel,
    SendPacket,
    ReceivePacket,
    WriteAck,
    AckPacket,
    Timeout,
    TimeoutOnClose,
    AppModule,
    Empty,
    ChainError,
}

impl IbcEventType {
    pub fn as_str(&self) -> &'static str {
        match *self {
            IbcEventType::NewBlock => NEW_BLOCK_EVENT,
            IbcEventType::CreateClient => CREATE_CLIENT_EVENT,
            IbcEventType::UpdateClient => UPDATE_CLIENT_EVENT,
            IbcEventType::UpgradeClient => UPGRADE_CLIENT_EVENT,
            IbcEventType::ClientMisbehaviour => CLIENT_MISBEHAVIOUR_EVENT,
            IbcEventType::OpenInitConnection => CONNECTION_INIT_EVENT,
            IbcEventType::OpenTryConnection => CONNECTION_TRY_EVENT,
            IbcEventType::OpenAckConnection => CONNECTION_ACK_EVENT,
            IbcEventType::OpenConfirmConnection => CONNECTION_CONFIRM_EVENT,
            IbcEventType::OpenInitChannel => CHANNEL_OPEN_INIT_EVENT,
            IbcEventType::OpenTryChannel => CHANNEL_OPEN_TRY_EVENT,
            IbcEventType::OpenAckChannel => CHANNEL_OPEN_ACK_EVENT,
            IbcEventType::OpenConfirmChannel => CHANNEL_OPEN_CONFIRM_EVENT,
            IbcEventType::CloseInitChannel => CHANNEL_CLOSE_INIT_EVENT,
            IbcEventType::CloseConfirmChannel => CHANNEL_CLOSE_CONFIRM_EVENT,
            IbcEventType::SendPacket => SEND_PACKET_EVENT,
            IbcEventType::ReceivePacket => RECEIVE_PACKET_EVENT,
            IbcEventType::WriteAck => WRITE_ACK_EVENT,
            IbcEventType::AckPacket => ACK_PACKET_EVENT,
            IbcEventType::Timeout => TIMEOUT_EVENT,
            IbcEventType::TimeoutOnClose => TIMEOUT_ON_CLOSE_EVENT,
            IbcEventType::AppModule => APP_MODULE_EVENT,
            IbcEventType::Empty => EMPTY_EVENT,
            IbcEventType::ChainError => CHAIN_ERROR_EVENT,
        }
    }
}

impl FromStr for IbcEventType {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            NEW_BLOCK_EVENT => Ok(IbcEventType::NewBlock),
            CREATE_CLIENT_EVENT => Ok(IbcEventType::CreateClient),
            UPDATE_CLIENT_EVENT => Ok(IbcEventType::UpdateClient),
            UPGRADE_CLIENT_EVENT => Ok(IbcEventType::UpgradeClient),
            CLIENT_MISBEHAVIOUR_EVENT => Ok(IbcEventType::ClientMisbehaviour),
            CONNECTION_INIT_EVENT => Ok(IbcEventType::OpenInitConnection),
            CONNECTION_TRY_EVENT => Ok(IbcEventType::OpenTryConnection),
            CONNECTION_ACK_EVENT => Ok(IbcEventType::OpenAckConnection),
            CONNECTION_CONFIRM_EVENT => Ok(IbcEventType::OpenConfirmConnection),
            CHANNEL_OPEN_INIT_EVENT => Ok(IbcEventType::OpenInitChannel),
            CHANNEL_OPEN_TRY_EVENT => Ok(IbcEventType::OpenTryChannel),
            CHANNEL_OPEN_ACK_EVENT => Ok(IbcEventType::OpenAckChannel),
            CHANNEL_OPEN_CONFIRM_EVENT => Ok(IbcEventType::OpenConfirmChannel),
            CHANNEL_CLOSE_INIT_EVENT => Ok(IbcEventType::CloseInitChannel),
            CHANNEL_CLOSE_CONFIRM_EVENT => Ok(IbcEventType::CloseConfirmChannel),
            SEND_PACKET_EVENT => Ok(IbcEventType::SendPacket),
            RECEIVE_PACKET_EVENT => Ok(IbcEventType::ReceivePacket),
            WRITE_ACK_EVENT => Ok(IbcEventType::WriteAck),
            ACK_PACKET_EVENT => Ok(IbcEventType::AckPacket),
            TIMEOUT_EVENT => Ok(IbcEventType::Timeout),
            TIMEOUT_ON_CLOSE_EVENT => Ok(IbcEventType::TimeoutOnClose),
            EMPTY_EVENT => Ok(IbcEventType::Empty),
            CHAIN_ERROR_EVENT => Ok(IbcEventType::ChainError),
            // from_str() for `APP_MODULE_EVENT` MUST fail because a `ModuleEvent`'s type isn't constant
            _ => Err(Error::incorrect_event_type(s.to_string())),
        }
    }
}

/// Events created by the IBC component of a chain, destined for a relayer.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub enum IbcEvent {
    NewBlock(NewBlock),

    CreateClient(ClientEvents::CreateClient),
    UpdateClient(ClientEvents::UpdateClient),
    UpgradeClient(ClientEvents::UpgradeClient),
    ClientMisbehaviour(ClientEvents::ClientMisbehaviour),

    OpenInitConnection(ConnectionEvents::OpenInit),
    OpenTryConnection(ConnectionEvents::OpenTry),
    OpenAckConnection(ConnectionEvents::OpenAck),
    OpenConfirmConnection(ConnectionEvents::OpenConfirm),

    OpenInitChannel(ChannelEvents::OpenInit),
    OpenTryChannel(ChannelEvents::OpenTry),
    OpenAckChannel(ChannelEvents::OpenAck),
    OpenConfirmChannel(ChannelEvents::OpenConfirm),
    CloseInitChannel(ChannelEvents::CloseInit),
    CloseConfirmChannel(ChannelEvents::CloseConfirm),

    SendPacket(ChannelEvents::SendPacket),
    ReceivePacket(ChannelEvents::ReceivePacket),
    WriteAcknowledgement(ChannelEvents::WriteAcknowledgement),
    AcknowledgePacket(ChannelEvents::AcknowledgePacket),
    TimeoutPacket(ChannelEvents::TimeoutPacket),
    TimeoutOnClosePacket(ChannelEvents::TimeoutOnClosePacket),

    AppModule(ModuleEvent),

    ChainError(String), // Special event, signifying an error on CheckTx or DeliverTx
}

impl fmt::Display for IbcEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            IbcEvent::NewBlock(ev) => write!(f, "NewBlock({})", ev.height),

            IbcEvent::CreateClient(ev) => write!(f, "CreateClientEv({})", ev),
            IbcEvent::UpdateClient(ev) => write!(f, "UpdateClientEv({})", ev),
            IbcEvent::UpgradeClient(ev) => write!(f, "UpgradeClientEv({:?})", ev),
            IbcEvent::ClientMisbehaviour(ev) => write!(f, "ClientMisbehaviourEv({:?})", ev),

            IbcEvent::OpenInitConnection(ev) => write!(f, "OpenInitConnectionEv({:?})", ev),
            IbcEvent::OpenTryConnection(ev) => write!(f, "OpenTryConnectionEv({:?})", ev),
            IbcEvent::OpenAckConnection(ev) => write!(f, "OpenAckConnectionEv({:?})", ev),
            IbcEvent::OpenConfirmConnection(ev) => write!(f, "OpenConfirmConnectionEv({:?})", ev),

            IbcEvent::OpenInitChannel(ev) => write!(f, "OpenInitChannelEv({:?})", ev),
            IbcEvent::OpenTryChannel(ev) => write!(f, "OpenTryChannelEv({:?})", ev),
            IbcEvent::OpenAckChannel(ev) => write!(f, "OpenAckChannelEv({:?})", ev),
            IbcEvent::OpenConfirmChannel(ev) => write!(f, "OpenConfirmChannelEv({:?})", ev),
            IbcEvent::CloseInitChannel(ev) => write!(f, "CloseInitChannelEv({})", ev),
            IbcEvent::CloseConfirmChannel(ev) => write!(f, "CloseConfirmChannelEv({:?})", ev),

            IbcEvent::SendPacket(ev) => write!(f, "SendPacketEv({})", ev),
            IbcEvent::ReceivePacket(ev) => write!(f, "ReceivePacketEv({})", ev),
            IbcEvent::WriteAcknowledgement(ev) => write!(f, "WriteAcknowledgementEv({})", ev),
            IbcEvent::AcknowledgePacket(ev) => write!(f, "AcknowledgePacketEv({})", ev),
            IbcEvent::TimeoutPacket(ev) => write!(f, "TimeoutPacketEv({})", ev),
            IbcEvent::TimeoutOnClosePacket(ev) => write!(f, "TimeoutOnClosePacketEv({})", ev),

            IbcEvent::AppModule(ev) => write!(f, "AppModuleEv({:?})", ev),

            IbcEvent::ChainError(ev) => write!(f, "ChainErrorEv({})", ev),
        }
    }
}

impl TryFrom<IbcEvent> for AbciEvent {
    type Error = Error;

    fn try_from(event: IbcEvent) -> Result<Self, Self::Error> {
        Ok(match event {
            IbcEvent::CreateClient(event) => event.into(),
            IbcEvent::UpdateClient(event) => event.into(),
            IbcEvent::UpgradeClient(event) => event.into(),
            IbcEvent::ClientMisbehaviour(event) => event.into(),
            IbcEvent::OpenInitConnection(event) => event.into(),
            IbcEvent::OpenTryConnection(event) => event.into(),
            IbcEvent::OpenAckConnection(event) => event.into(),
            IbcEvent::OpenConfirmConnection(event) => event.into(),
            IbcEvent::OpenInitChannel(event) => event.into(),
            IbcEvent::OpenTryChannel(event) => event.into(),
            IbcEvent::OpenAckChannel(event) => event.into(),
            IbcEvent::OpenConfirmChannel(event) => event.into(),
            IbcEvent::CloseInitChannel(event) => event.into(),
            IbcEvent::CloseConfirmChannel(event) => event.into(),
            IbcEvent::SendPacket(event) => event.try_into().map_err(Error::channel)?,
            IbcEvent::ReceivePacket(event) => event.try_into().map_err(Error::channel)?,
            IbcEvent::WriteAcknowledgement(event) => event.try_into().map_err(Error::channel)?,
            IbcEvent::AcknowledgePacket(event) => event.try_into().map_err(Error::channel)?,
            IbcEvent::TimeoutPacket(event) => event.try_into().map_err(Error::channel)?,
            IbcEvent::TimeoutOnClosePacket(event) => event.try_into().map_err(Error::channel)?,
            IbcEvent::AppModule(event) => event.try_into()?,
            IbcEvent::NewBlock(_) | IbcEvent::ChainError(_) => {
                return Err(Error::incorrect_event_type(event.to_string()))
            }
        })
    }
}

impl TryFrom<&AbciEvent> for IbcEvent {
    type Error = Error;

    fn try_from(abci_event: &AbciEvent) -> Result<Self, Self::Error> {
        match abci_event.type_str.parse() {
            Ok(IbcEventType::CreateClient) => Ok(IbcEvent::CreateClient(
                ClientEvents::CreateClient::try_from(abci_event).map_err(Error::client)?,
            )),
            Ok(IbcEventType::UpdateClient) => Ok(IbcEvent::UpdateClient(
                ClientEvents::UpdateClient::try_from(abci_event).map_err(Error::client)?,
            )),
            Ok(IbcEventType::UpgradeClient) => Ok(IbcEvent::UpgradeClient(
                ClientEvents::UpgradeClient::try_from(abci_event).map_err(Error::client)?,
            )),
            Ok(IbcEventType::ClientMisbehaviour) => Ok(IbcEvent::ClientMisbehaviour(
                ClientEvents::ClientMisbehaviour::try_from(abci_event).map_err(Error::client)?,
            )),
            Ok(IbcEventType::OpenInitConnection) => Ok(IbcEvent::OpenInitConnection(
                ConnectionEvents::OpenInit::try_from(abci_event).map_err(Error::connection)?,
            )),
            Ok(IbcEventType::OpenTryConnection) => Ok(IbcEvent::OpenTryConnection(
                ConnectionEvents::OpenTry::try_from(abci_event).map_err(Error::connection)?,
            )),
            Ok(IbcEventType::OpenAckConnection) => Ok(IbcEvent::OpenAckConnection(
                ConnectionEvents::OpenAck::try_from(abci_event).map_err(Error::connection)?,
            )),
            Ok(IbcEventType::OpenConfirmConnection) => Ok(IbcEvent::OpenConfirmConnection(
                ConnectionEvents::OpenConfirm::try_from(abci_event).map_err(Error::connection)?,
            )),
            Ok(IbcEventType::OpenInitChannel) => Ok(IbcEvent::OpenInitChannel(
                ChannelEvents::OpenInit::try_from(abci_event).map_err(Error::channel)?,
            )),
            Ok(IbcEventType::OpenTryChannel) => Ok(IbcEvent::OpenTryChannel(
                ChannelEvents::OpenTry::try_from(abci_event).map_err(Error::channel)?,
            )),
            Ok(IbcEventType::OpenAckChannel) => Ok(IbcEvent::OpenAckChannel(
                ChannelEvents::OpenAck::try_from(abci_event).map_err(Error::channel)?,
            )),
            Ok(IbcEventType::OpenConfirmChannel) => Ok(IbcEvent::OpenConfirmChannel(
                ChannelEvents::OpenConfirm::try_from(abci_event).map_err(Error::channel)?,
            )),
            Ok(IbcEventType::CloseInitChannel) => Ok(IbcEvent::CloseInitChannel(
                ChannelEvents::CloseInit::try_from(abci_event).map_err(Error::channel)?,
            )),
            Ok(IbcEventType::CloseConfirmChannel) => Ok(IbcEvent::CloseConfirmChannel(
                ChannelEvents::CloseConfirm::try_from(abci_event).map_err(Error::channel)?,
            )),
            Ok(IbcEventType::SendPacket) => Ok(IbcEvent::SendPacket(
                ChannelEvents::SendPacket::try_from(abci_event).map_err(Error::channel)?,
            )),
            Ok(IbcEventType::WriteAck) => Ok(IbcEvent::WriteAcknowledgement(
                ChannelEvents::WriteAcknowledgement::try_from(abci_event)
                    .map_err(Error::channel)?,
            )),
            Ok(IbcEventType::AckPacket) => Ok(IbcEvent::AcknowledgePacket(
                ChannelEvents::AcknowledgePacket::try_from(abci_event).map_err(Error::channel)?,
            )),
            Ok(IbcEventType::Timeout) => Ok(IbcEvent::TimeoutPacket(
                ChannelEvents::TimeoutPacket::try_from(abci_event).map_err(Error::channel)?,
            )),
            _ => Err(Error::unsupported_abci_event(
                abci_event.type_str.to_owned(),
            )),
        }
    }
}

impl IbcEvent {
    pub fn to_json(&self) -> String {
        match serde_json::to_string(self) {
            Ok(value) => value,
            Err(_) => format!("{:?}", self), // Fallback to debug printing
        }
    }

    pub fn event_type(&self) -> IbcEventType {
        match self {
            IbcEvent::NewBlock(_) => IbcEventType::NewBlock,
            IbcEvent::CreateClient(_) => IbcEventType::CreateClient,
            IbcEvent::UpdateClient(_) => IbcEventType::UpdateClient,
            IbcEvent::ClientMisbehaviour(_) => IbcEventType::ClientMisbehaviour,
            IbcEvent::UpgradeClient(_) => IbcEventType::UpgradeClient,
            IbcEvent::OpenInitConnection(_) => IbcEventType::OpenInitConnection,
            IbcEvent::OpenTryConnection(_) => IbcEventType::OpenTryConnection,
            IbcEvent::OpenAckConnection(_) => IbcEventType::OpenAckConnection,
            IbcEvent::OpenConfirmConnection(_) => IbcEventType::OpenConfirmConnection,
            IbcEvent::OpenInitChannel(_) => IbcEventType::OpenInitChannel,
            IbcEvent::OpenTryChannel(_) => IbcEventType::OpenTryChannel,
            IbcEvent::OpenAckChannel(_) => IbcEventType::OpenAckChannel,
            IbcEvent::OpenConfirmChannel(_) => IbcEventType::OpenConfirmChannel,
            IbcEvent::CloseInitChannel(_) => IbcEventType::CloseInitChannel,
            IbcEvent::CloseConfirmChannel(_) => IbcEventType::CloseConfirmChannel,
            IbcEvent::SendPacket(_) => IbcEventType::SendPacket,
            IbcEvent::ReceivePacket(_) => IbcEventType::ReceivePacket,
            IbcEvent::WriteAcknowledgement(_) => IbcEventType::WriteAck,
            IbcEvent::AcknowledgePacket(_) => IbcEventType::AckPacket,
            IbcEvent::TimeoutPacket(_) => IbcEventType::Timeout,
            IbcEvent::TimeoutOnClosePacket(_) => IbcEventType::TimeoutOnClose,
            IbcEvent::AppModule(_) => IbcEventType::AppModule,
            IbcEvent::ChainError(_) => IbcEventType::ChainError,
        }
    }

    pub fn channel_attributes(self) -> Option<ChannelAttributes> {
        match self {
            IbcEvent::OpenInitChannel(ev) => Some(ev.into()),
            IbcEvent::OpenTryChannel(ev) => Some(ev.into()),
            IbcEvent::OpenAckChannel(ev) => Some(ev.into()),
            IbcEvent::OpenConfirmChannel(ev) => Some(ev.into()),
            _ => None,
        }
    }

    pub fn connection_attributes(&self) -> Option<&ConnectionAttributes> {
        match self {
            IbcEvent::OpenInitConnection(ev) => Some(ev.attributes()),
            IbcEvent::OpenTryConnection(ev) => Some(ev.attributes()),
            IbcEvent::OpenAckConnection(ev) => Some(ev.attributes()),
            IbcEvent::OpenConfirmConnection(ev) => Some(ev.attributes()),
            _ => None,
        }
    }

    pub fn packet(&self) -> Option<&Packet> {
        match self {
            IbcEvent::SendPacket(ev) => Some(&ev.packet),
            IbcEvent::ReceivePacket(ev) => Some(&ev.packet),
            IbcEvent::WriteAcknowledgement(ev) => Some(&ev.packet),
            IbcEvent::AcknowledgePacket(ev) => Some(&ev.packet),
            IbcEvent::TimeoutPacket(ev) => Some(&ev.packet),
            IbcEvent::TimeoutOnClosePacket(ev) => Some(&ev.packet),
            _ => None,
        }
    }

    pub fn ack(&self) -> Option<&[u8]> {
        match self {
            IbcEvent::WriteAcknowledgement(ev) => Some(&ev.ack),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ModuleEvent {
    pub kind: String,
    pub module_name: ModuleId,
    pub attributes: Vec<ModuleEventAttribute>,
}

impl TryFrom<ModuleEvent> for AbciEvent {
    type Error = Error;

    fn try_from(event: ModuleEvent) -> Result<Self, Self::Error> {
        if IbcEventType::from_str(event.kind.as_str()).is_ok() {
            return Err(Error::malformed_module_event(event));
        }

        let attributes = event.attributes.into_iter().map(Into::into).collect();
        Ok(AbciEvent {
            type_str: event.kind,
            attributes,
        })
    }
}

impl From<ModuleEvent> for IbcEvent {
    fn from(e: ModuleEvent) -> Self {
        IbcEvent::AppModule(e)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ModuleEventAttribute {
    pub key: String,
    pub value: String,
}

impl<K: ToString, V: ToString> From<(K, V)> for ModuleEventAttribute {
    fn from((k, v): (K, V)) -> Self {
        Self {
            key: k.to_string(),
            value: v.to_string(),
        }
    }
}

impl From<ModuleEventAttribute> for Tag {
    fn from(attr: ModuleEventAttribute) -> Self {
        Self {
            key: attr
                .key
                .parse()
                .expect("Key::from_str() impl is infallible"),
            value: attr
                .key
                .parse()
                .expect("Value::from_str() impl is infallible"),
        }
    }
}
