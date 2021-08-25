//! Types for the IBC events emitted from Tendermint Websocket by the channels module.
use crate::events::IbcEvent;
use crate::ics02_client::height::Height;
use crate::ics04_channel::packet::Packet;
use crate::ics24_host::identifier::{ChannelId, ConnectionId, PortId};
use crate::prelude::*;

use serde_derive::{Deserialize, Serialize};

/// Channel event types
const OPEN_INIT_EVENT_TYPE: &str = "channel_open_init";
const OPEN_TRY_EVENT_TYPE: &str = "channel_open_try";
const OPEN_ACK_EVENT_TYPE: &str = "channel_open_ack";
const OPEN_CONFIRM_EVENT_TYPE: &str = "channel_open_confirm";
const CLOSE_INIT_EVENT_TYPE: &str = "channel_close_init";
const CLOSE_CONFIRM_EVENT_TYPE: &str = "channel_close_confirm";

/// Channel event attribute keys
const CONNECTION_ID_ATTRIBUTE_KEY: &str = "connection_id";
const CHANNEL_ID_ATTRIBUTE_KEY: &str = "channel_id";
const PORT_ID_ATTRIBUTE_KEY: &str = "port_id";
const COUNTERPARTY_CHANNEL_ID_ATTRIBUTE_KEY: &str = "counterparty_channel_id";
const COUNTERPARTY_PORT_ID_ATTRIBUTE_KEY: &str = "counterparty_port_id";

/// Packet event types
const SEND_PACKET: &str = "send_packet";
const WRITE_ACK: &str = "write_acknowledgement";
const ACK_PACKET: &str = "acknowledge_packet";
const TIMEOUT: &str = "timeout_packet";

/// Packet event attribute keys
const PKT_SEQ_ATTRIBUTE_KEY: &str = "packet_sequence";
const PKT_DATA_ATTRIBUTE_KEY: &str = "packet_data";
const PKT_SRC_PORT_ATTRIBUTE_KEY: &str = "packet_src_port";
const PKT_SRC_CHANNEL_ATTRIBUTE_KEY: &str = "packet_src_channel";
const PKT_DST_PORT_ATTRIBUTE_KEY: &str = "packet_dst_port";
const PKT_DST_CHANNEL_ATTRIBUTE_KEY: &str = "packet_dst_channel";
const PKT_TIMEOUT_HEIGHT_ATTRIBUTE_KEY: &str = "packet_timeout_height";
const PKT_TIMEOUT_TIMESTAMP_ATTRIBUTE_KEY: &str = "packet_timeout_timestamp";

const PKT_ACK_ATTRIBUTE_KEY: &str = "packet_ack";

pub fn try_from_tx(event: &tendermint::abci::Event) -> Option<IbcEvent> {
    match event.type_str.as_str() {
        OPEN_INIT_EVENT_TYPE => Some(IbcEvent::OpenInitChannel(OpenInit::from(
            extract_attributes_from_tx(event),
        ))),
        OPEN_TRY_EVENT_TYPE => Some(IbcEvent::OpenTryChannel(OpenTry::from(
            extract_attributes_from_tx(event),
        ))),
        OPEN_ACK_EVENT_TYPE => Some(IbcEvent::OpenAckChannel(OpenAck::from(
            extract_attributes_from_tx(event),
        ))),
        OPEN_CONFIRM_EVENT_TYPE => Some(IbcEvent::OpenConfirmChannel(OpenConfirm::from(
            extract_attributes_from_tx(event),
        ))),
        CLOSE_INIT_EVENT_TYPE => Some(IbcEvent::CloseInitChannel(CloseInit::from(
            extract_attributes_from_tx(event),
        ))),
        CLOSE_CONFIRM_EVENT_TYPE => Some(IbcEvent::CloseConfirmChannel(CloseConfirm::from(
            extract_attributes_from_tx(event),
        ))),
        SEND_PACKET => {
            let (packet, write_ack) = extract_packet_and_write_ack_from_tx(event);
            // This event should not have a write ack.
            assert_eq!(write_ack.len(), 0);
            Some(IbcEvent::SendPacket(SendPacket {
                height: Default::default(),
                packet,
            }))
        }
        WRITE_ACK => {
            let (packet, write_ack) = extract_packet_and_write_ack_from_tx(event);
            Some(IbcEvent::WriteAcknowledgement(WriteAcknowledgement {
                height: Default::default(),
                packet,
                ack: write_ack,
            }))
        }
        ACK_PACKET => {
            let (packet, write_ack) = extract_packet_and_write_ack_from_tx(event);
            // This event should not have a write ack.
            assert_eq!(write_ack.len(), 0);
            Some(IbcEvent::AcknowledgePacket(AcknowledgePacket {
                height: Default::default(),
                packet,
            }))
        }
        TIMEOUT => {
            let (packet, write_ack) = extract_packet_and_write_ack_from_tx(event);
            // This event should not have a write ack.
            assert_eq!(write_ack.len(), 0);
            Some(IbcEvent::TimeoutPacket(TimeoutPacket {
                height: Default::default(),
                packet,
            }))
        }
        _ => None,
    }
}

fn extract_attributes_from_tx(event: &tendermint::abci::Event) -> Attributes {
    let mut attr = Attributes::default();

    for tag in &event.attributes {
        let key = tag.key.as_ref();
        let value = tag.value.as_ref();
        match key {
            PORT_ID_ATTRIBUTE_KEY => attr.port_id = value.parse().unwrap(),
            CHANNEL_ID_ATTRIBUTE_KEY => attr.channel_id = value.parse().ok(),
            CONNECTION_ID_ATTRIBUTE_KEY => attr.connection_id = value.parse().unwrap(),
            COUNTERPARTY_PORT_ID_ATTRIBUTE_KEY => {
                attr.counterparty_port_id = value.parse().unwrap()
            }
            COUNTERPARTY_CHANNEL_ID_ATTRIBUTE_KEY => {
                attr.counterparty_channel_id = value.parse().ok()
            }
            _ => {}
        }
    }

    attr
}

fn extract_packet_and_write_ack_from_tx(event: &tendermint::abci::Event) -> (Packet, Vec<u8>) {
    let mut packet = Packet::default();
    let mut write_ack: Vec<u8> = Vec::new();
    for tag in &event.attributes {
        let key = tag.key.as_ref();
        let value = tag.value.as_ref();
        match key {
            PKT_SRC_PORT_ATTRIBUTE_KEY => packet.source_port = value.parse().unwrap(),
            PKT_SRC_CHANNEL_ATTRIBUTE_KEY => packet.source_channel = value.parse().unwrap(),
            PKT_DST_PORT_ATTRIBUTE_KEY => packet.destination_port = value.parse().unwrap(),
            PKT_DST_CHANNEL_ATTRIBUTE_KEY => packet.destination_channel = value.parse().unwrap(),
            PKT_SEQ_ATTRIBUTE_KEY => packet.sequence = value.parse::<u64>().unwrap().into(),
            PKT_TIMEOUT_HEIGHT_ATTRIBUTE_KEY => packet.timeout_height = value.parse().unwrap(),
            PKT_TIMEOUT_TIMESTAMP_ATTRIBUTE_KEY => {
                packet.timeout_timestamp = value.parse().unwrap()
            }
            PKT_DATA_ATTRIBUTE_KEY => packet.data = Vec::from(value.as_bytes()),
            PKT_ACK_ATTRIBUTE_KEY => write_ack = Vec::from(value.as_bytes()),
            _ => {}
        };
    }

    (packet, write_ack)
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Attributes {
    pub height: Height,
    pub port_id: PortId,
    pub channel_id: Option<ChannelId>,
    pub connection_id: ConnectionId,
    pub counterparty_port_id: PortId,
    pub counterparty_channel_id: Option<ChannelId>,
}

impl Attributes {
    pub fn port_id(&self) -> &PortId {
        &self.port_id
    }
    pub fn channel_id(&self) -> Option<&ChannelId> {
        self.channel_id.as_ref()
    }
}

impl Default for Attributes {
    fn default() -> Self {
        Attributes {
            height: Default::default(),
            port_id: Default::default(),
            channel_id: Default::default(),
            connection_id: Default::default(),
            counterparty_port_id: Default::default(),
            counterparty_channel_id: Default::default(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OpenInit(Attributes);

impl OpenInit {
    pub fn attributes(&self) -> &Attributes {
        &self.0
    }
    pub fn channel_id(&self) -> Option<&ChannelId> {
        self.0.channel_id.as_ref()
    }
    pub fn port_id(&self) -> &PortId {
        &self.0.port_id
    }
    pub fn height(&self) -> Height {
        self.0.height
    }
    pub fn set_height(&mut self, height: Height) {
        self.0.height = height;
    }
}

impl From<Attributes> for OpenInit {
    fn from(attrs: Attributes) -> Self {
        OpenInit(attrs)
    }
}

impl From<OpenInit> for IbcEvent {
    fn from(v: OpenInit) -> Self {
        IbcEvent::OpenInitChannel(v)
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OpenTry(Attributes);

impl OpenTry {
    pub fn attributes(&self) -> &Attributes {
        &self.0
    }
    pub fn channel_id(&self) -> Option<&ChannelId> {
        self.0.channel_id.as_ref()
    }
    pub fn port_id(&self) -> &PortId {
        &self.0.port_id
    }
    pub fn height(&self) -> Height {
        self.0.height
    }
    pub fn set_height(&mut self, height: Height) {
        self.0.height = height;
    }
}

impl From<Attributes> for OpenTry {
    fn from(attrs: Attributes) -> Self {
        OpenTry(attrs)
    }
}

impl From<OpenTry> for IbcEvent {
    fn from(v: OpenTry) -> Self {
        IbcEvent::OpenTryChannel(v)
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OpenAck(Attributes);

impl OpenAck {
    pub fn attributes(&self) -> &Attributes {
        &self.0
    }
    pub fn channel_id(&self) -> Option<&ChannelId> {
        self.0.channel_id.as_ref()
    }
    pub fn port_id(&self) -> &PortId {
        &self.0.port_id
    }
    pub fn height(&self) -> Height {
        self.0.height
    }
    pub fn set_height(&mut self, height: Height) {
        self.0.height = height;
    }

    pub fn counterparty_channel_id(&self) -> Option<&ChannelId> {
        self.0.counterparty_channel_id.as_ref()
    }
}

impl From<Attributes> for OpenAck {
    fn from(attrs: Attributes) -> Self {
        OpenAck(attrs)
    }
}

impl From<OpenAck> for IbcEvent {
    fn from(v: OpenAck) -> Self {
        IbcEvent::OpenAckChannel(v)
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OpenConfirm(Attributes);

impl OpenConfirm {
    pub fn attributes(&self) -> &Attributes {
        &self.0
    }
    pub fn channel_id(&self) -> Option<&ChannelId> {
        self.0.channel_id.as_ref()
    }
    pub fn port_id(&self) -> &PortId {
        &self.0.port_id
    }
    pub fn height(&self) -> Height {
        self.0.height
    }
    pub fn set_height(&mut self, height: Height) {
        self.0.height = height;
    }
}

impl From<Attributes> for OpenConfirm {
    fn from(attrs: Attributes) -> Self {
        OpenConfirm(attrs)
    }
}

impl From<OpenConfirm> for IbcEvent {
    fn from(v: OpenConfirm) -> Self {
        IbcEvent::OpenConfirmChannel(v)
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CloseInit(Attributes);

impl CloseInit {
    pub fn port_id(&self) -> &PortId {
        &self.0.port_id
    }

    pub fn channel_id(&self) -> &ChannelId {
        // FIXME(romac): Rework encoding of IbcEvents which use `Attributes`
        self.0
            .channel_id
            .as_ref()
            .expect("CloseInit should always have a channel_id")
    }

    pub fn counterparty_port_id(&self) -> &PortId {
        &self.0.counterparty_port_id
    }

    pub fn counterparty_channel_id(&self) -> Option<&ChannelId> {
        self.0.counterparty_channel_id.as_ref()
    }

    pub fn height(&self) -> Height {
        self.0.height
    }

    pub fn set_height(&mut self, height: Height) {
        self.0.height = height;
    }
}

impl From<Attributes> for CloseInit {
    fn from(attrs: Attributes) -> Self {
        CloseInit(attrs)
    }
}

impl From<CloseInit> for IbcEvent {
    fn from(v: CloseInit) -> Self {
        IbcEvent::CloseInitChannel(v)
    }
}

impl core::fmt::Display for CloseInit {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> Result<(), core::fmt::Error> {
        write!(
            f,
            "{} {} {:?}",
            self.height(),
            CLOSE_INIT_EVENT_TYPE,
            self.0
        )
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CloseConfirm(Attributes);

impl CloseConfirm {
    pub fn channel_id(&self) -> Option<&ChannelId> {
        self.0.channel_id.as_ref()
    }
    pub fn height(&self) -> Height {
        self.0.height
    }
    pub fn set_height(&mut self, height: Height) {
        self.0.height = height;
    }
}

impl From<Attributes> for CloseConfirm {
    fn from(attrs: Attributes) -> Self {
        CloseConfirm(attrs)
    }
}

impl From<CloseConfirm> for IbcEvent {
    fn from(v: CloseConfirm) -> Self {
        IbcEvent::CloseConfirmChannel(v)
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SendPacket {
    pub height: Height,
    pub packet: Packet,
}

impl SendPacket {
    pub fn height(&self) -> Height {
        self.height
    }
    pub fn set_height(&mut self, height: Height) {
        self.height = height;
    }
    pub fn src_port_id(&self) -> &PortId {
        &self.packet.source_port
    }
    pub fn src_channel_id(&self) -> &ChannelId {
        &self.packet.source_channel
    }
    pub fn dst_port_id(&self) -> &PortId {
        &self.packet.destination_port
    }
    pub fn dst_channel_id(&self) -> &ChannelId {
        &self.packet.destination_channel
    }
}

impl From<SendPacket> for IbcEvent {
    fn from(v: SendPacket) -> Self {
        IbcEvent::SendPacket(v)
    }
}

impl core::fmt::Display for SendPacket {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> Result<(), core::fmt::Error> {
        write!(f, "SendPacket - h:{}, {}", self.height, self.packet)
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ReceivePacket {
    pub height: Height,
    pub packet: Packet,
}

impl ReceivePacket {
    pub fn height(&self) -> Height {
        self.height
    }
    pub fn set_height(&mut self, height: Height) {
        self.height = height;
    }
    pub fn src_port_id(&self) -> &PortId {
        &self.packet.source_port
    }
    pub fn src_channel_id(&self) -> &ChannelId {
        &self.packet.source_channel
    }
    pub fn dst_port_id(&self) -> &PortId {
        &self.packet.destination_port
    }
    pub fn dst_channel_id(&self) -> &ChannelId {
        &self.packet.destination_channel
    }
}

impl From<ReceivePacket> for IbcEvent {
    fn from(v: ReceivePacket) -> Self {
        IbcEvent::ReceivePacket(v)
    }
}

impl core::fmt::Display for ReceivePacket {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> Result<(), core::fmt::Error> {
        write!(f, "ReceivePacket - h:{}, {}", self.height, self.packet)
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct WriteAcknowledgement {
    pub height: Height,
    pub packet: Packet,
    #[serde(serialize_with = "crate::serializers::ser_hex_upper")]
    pub ack: Vec<u8>,
}

impl WriteAcknowledgement {
    pub fn height(&self) -> Height {
        self.height
    }
    pub fn set_height(&mut self, height: Height) {
        self.height = height;
    }
    pub fn src_port_id(&self) -> &PortId {
        &self.packet.source_port
    }
    pub fn src_channel_id(&self) -> &ChannelId {
        &self.packet.source_channel
    }
    pub fn dst_port_id(&self) -> &PortId {
        &self.packet.destination_port
    }
    pub fn dst_channel_id(&self) -> &ChannelId {
        &self.packet.destination_channel
    }
}

impl From<WriteAcknowledgement> for IbcEvent {
    fn from(v: WriteAcknowledgement) -> Self {
        IbcEvent::WriteAcknowledgement(v)
    }
}

impl core::fmt::Display for WriteAcknowledgement {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> Result<(), core::fmt::Error> {
        write!(
            f,
            "WriteAcknowledgement - h:{}, {}",
            self.height, self.packet
        )
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AcknowledgePacket {
    pub height: Height,
    pub packet: Packet,
}

impl AcknowledgePacket {
    pub fn height(&self) -> Height {
        self.height
    }
    pub fn set_height(&mut self, height: Height) {
        self.height = height;
    }
    pub fn src_port_id(&self) -> &PortId {
        &self.packet.source_port
    }
    pub fn src_channel_id(&self) -> &ChannelId {
        &self.packet.source_channel
    }
}

impl From<AcknowledgePacket> for IbcEvent {
    fn from(v: AcknowledgePacket) -> Self {
        IbcEvent::AcknowledgePacket(v)
    }
}

impl core::fmt::Display for AcknowledgePacket {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> Result<(), core::fmt::Error> {
        write!(f, "h:{}, {}", self.height, self.packet)
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TimeoutPacket {
    pub height: Height,
    pub packet: Packet,
}

impl TimeoutPacket {
    pub fn height(&self) -> Height {
        self.height
    }
    pub fn set_height(&mut self, height: Height) {
        self.height = height;
    }
    pub fn src_port_id(&self) -> &PortId {
        &self.packet.source_port
    }
    pub fn src_channel_id(&self) -> &ChannelId {
        &self.packet.source_channel
    }
    pub fn dst_port_id(&self) -> &PortId {
        &self.packet.destination_port
    }
    pub fn dst_channel_id(&self) -> &ChannelId {
        &self.packet.destination_channel
    }
}

impl From<TimeoutPacket> for IbcEvent {
    fn from(v: TimeoutPacket) -> Self {
        IbcEvent::TimeoutPacket(v)
    }
}

impl core::fmt::Display for TimeoutPacket {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> Result<(), core::fmt::Error> {
        write!(f, "TimeoutPacket - h:{}, {}", self.height, self.packet)
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TimeoutOnClosePacket {
    pub height: Height,
    pub packet: Packet,
}

impl TimeoutOnClosePacket {
    pub fn height(&self) -> Height {
        self.height
    }
    pub fn set_height(&mut self, height: Height) {
        self.height = height;
    }
    pub fn src_port_id(&self) -> &PortId {
        &self.packet.source_port
    }
    pub fn src_channel_id(&self) -> &ChannelId {
        &self.packet.source_channel
    }
    pub fn dst_port_id(&self) -> &PortId {
        &self.packet.destination_port
    }
    pub fn dst_channel_id(&self) -> &ChannelId {
        &self.packet.destination_channel
    }
}

impl From<TimeoutOnClosePacket> for IbcEvent {
    fn from(v: TimeoutOnClosePacket) -> Self {
        IbcEvent::TimeoutOnClosePacket(v)
    }
}

impl core::fmt::Display for TimeoutOnClosePacket {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> Result<(), core::fmt::Error> {
        write!(
            f,
            "TimeoutOnClosePacket - h:{}, {}",
            self.height, self.packet
        )
    }
}
