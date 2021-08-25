//! Types for the IBC events emitted from Tendermint Websocket by the connection module.

use serde_derive::{Deserialize, Serialize};

use crate::events::IbcEvent;
use crate::ics02_client::height::Height;
use crate::ics24_host::identifier::{ClientId, ConnectionId};

/// The content of the `type` field for the event that a chain produces upon executing a connection handshake transaction.
const INIT_EVENT_TYPE: &str = "connection_open_init";
const TRY_EVENT_TYPE: &str = "connection_open_try";
const ACK_EVENT_TYPE: &str = "connection_open_ack";
const CONFIRM_EVENT_TYPE: &str = "connection_open_confirm";

/// The content of the `key` field for the attribute containing the connection identifier.
const CONN_ID_ATTRIBUTE_KEY: &str = "connection_id";
const CLIENT_ID_ATTRIBUTE_KEY: &str = "client_id";
const COUNTERPARTY_CONN_ID_ATTRIBUTE_KEY: &str = "counterparty_connection_id";
const COUNTERPARTY_CLIENT_ID_ATTRIBUTE_KEY: &str = "counterparty_client_id";

pub fn try_from_tx(event: &tendermint::abci::Event) -> Option<IbcEvent> {
    match event.type_str.as_ref() {
        INIT_EVENT_TYPE => Some(IbcEvent::OpenInitConnection(OpenInit::from(
            extract_attributes_from_tx(event),
        ))),
        TRY_EVENT_TYPE => Some(IbcEvent::OpenTryConnection(OpenTry::from(
            extract_attributes_from_tx(event),
        ))),
        ACK_EVENT_TYPE => Some(IbcEvent::OpenAckConnection(OpenAck::from(
            extract_attributes_from_tx(event),
        ))),
        CONFIRM_EVENT_TYPE => Some(IbcEvent::OpenConfirmConnection(OpenConfirm::from(
            extract_attributes_from_tx(event),
        ))),
        _ => None,
    }
}

fn extract_attributes_from_tx(event: &tendermint::abci::Event) -> Attributes {
    let mut attr = Attributes::default();

    for tag in &event.attributes {
        let key = tag.key.as_ref();
        let value = tag.value.as_ref();
        match key {
            CONN_ID_ATTRIBUTE_KEY => attr.connection_id = value.parse().ok(),
            CLIENT_ID_ATTRIBUTE_KEY => attr.client_id = value.parse().unwrap(),
            COUNTERPARTY_CONN_ID_ATTRIBUTE_KEY => {
                attr.counterparty_connection_id = value.parse().ok()
            }
            COUNTERPARTY_CLIENT_ID_ATTRIBUTE_KEY => {
                attr.counterparty_client_id = value.parse().unwrap()
            }
            // TODO: `Attributes` has 5 fields and we're only parsing 4
            _ => {}
        }
    }

    attr
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Attributes {
    pub height: Height,
    pub connection_id: Option<ConnectionId>,
    pub client_id: ClientId,
    pub counterparty_connection_id: Option<ConnectionId>,
    pub counterparty_client_id: ClientId,
}

impl Default for Attributes {
    fn default() -> Self {
        Attributes {
            height: Default::default(),
            connection_id: Default::default(),
            client_id: Default::default(),
            counterparty_connection_id: Default::default(),
            counterparty_client_id: Default::default(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OpenInit(Attributes);

impl OpenInit {
    pub fn attributes(&self) -> &Attributes {
        &self.0
    }
    pub fn connection_id(&self) -> &Option<ConnectionId> {
        &self.0.connection_id
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
        IbcEvent::OpenInitConnection(v)
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OpenTry(Attributes);

impl OpenTry {
    pub fn attributes(&self) -> &Attributes {
        &self.0
    }
    pub fn connection_id(&self) -> &Option<ConnectionId> {
        &self.0.connection_id
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
        IbcEvent::OpenTryConnection(v)
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OpenAck(Attributes);

impl OpenAck {
    pub fn attributes(&self) -> &Attributes {
        &self.0
    }
    pub fn connection_id(&self) -> &Option<ConnectionId> {
        &self.0.connection_id
    }
    pub fn height(&self) -> Height {
        self.0.height
    }
    pub fn set_height(&mut self, height: Height) {
        self.0.height = height;
    }
}

impl From<Attributes> for OpenAck {
    fn from(attrs: Attributes) -> Self {
        OpenAck(attrs)
    }
}

impl From<OpenAck> for IbcEvent {
    fn from(v: OpenAck) -> Self {
        IbcEvent::OpenAckConnection(v)
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OpenConfirm(Attributes);

impl OpenConfirm {
    pub fn attributes(&self) -> &Attributes {
        &self.0
    }
    pub fn connection_id(&self) -> &Option<ConnectionId> {
        &self.0.connection_id
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
        IbcEvent::OpenConfirmConnection(v)
    }
}
