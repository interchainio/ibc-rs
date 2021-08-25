//! Types for the IBC events emitted from Tendermint Websocket by the client module.

use serde_derive::{Deserialize, Serialize};
use subtle_encoding::hex;
use tendermint_proto::Protobuf;

use crate::events::IbcEvent;
use crate::ics02_client::client_type::ClientType;
use crate::ics02_client::header::AnyHeader;
use crate::ics02_client::height::Height;
use crate::ics24_host::identifier::ClientId;

/// The content of the `type` field for the event that a chain produces upon executing the create client transaction.
const CREATE_EVENT_TYPE: &str = "create_client";
const UPDATE_EVENT_TYPE: &str = "update_client";
const MISBEHAVIOUR_EVENT_TYPE: &str = "client_misbehaviour";
const UPGRADE_EVENT_TYPE: &str = "upgrade_client";

/// The content of the `key` field for the attribute containing the client identifier.
const CLIENT_ID_ATTRIBUTE_KEY: &str = "client_id";

/// The content of the `key` field for the attribute containing the client type.
const CLIENT_TYPE_ATTRIBUTE_KEY: &str = "client_type";

/// The content of the `key` field for the attribute containing the height.
const CONSENSUS_HEIGHT_ATTRIBUTE_KEY: &str = "consensus_height";

/// The content of the `key` field for the header in update client event.
const HEADER: &str = "header";

pub fn try_from_tx(event: &tendermint::abci::Event) -> Option<IbcEvent> {
    match event.type_str.as_ref() {
        CREATE_EVENT_TYPE => Some(IbcEvent::CreateClient(CreateClient(
            extract_attributes_from_tx(event),
        ))),
        UPDATE_EVENT_TYPE => Some(IbcEvent::UpdateClient(UpdateClient {
            common: extract_attributes_from_tx(event),
            header: extract_header_from_tx(event),
        })),
        MISBEHAVIOUR_EVENT_TYPE => Some(IbcEvent::ClientMisbehaviour(ClientMisbehaviour(
            extract_attributes_from_tx(event),
        ))),
        UPGRADE_EVENT_TYPE => Some(IbcEvent::UpgradeClient(UpgradeClient(
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
            CLIENT_ID_ATTRIBUTE_KEY => attr.client_id = value.parse().unwrap(),
            CLIENT_TYPE_ATTRIBUTE_KEY => attr.client_type = value.parse().unwrap(),
            CONSENSUS_HEIGHT_ATTRIBUTE_KEY => attr.consensus_height = value.parse().unwrap(),
            // TODO: `Attributes` has 4 fields and we're only parsing 3
            _ => {}
        }
    }

    attr
}

pub fn extract_header_from_tx(event: &tendermint::abci::Event) -> Option<AnyHeader> {
    for tag in &event.attributes {
        let key = tag.key.as_ref();
        let value = tag.value.as_ref();
        if let HEADER = key {
            let header_bytes = hex::decode(value).unwrap();
            let header: AnyHeader = Protobuf::decode(header_bytes.as_ref()).unwrap();
            return Some(header);
        }
    }
    None
}

/// NewBlock event signals the committing & execution of a new block.
// TODO - find a better place for NewBlock
#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
pub struct NewBlock {
    pub height: Height,
}

impl NewBlock {
    pub fn new(h: Height) -> NewBlock {
        NewBlock { height: h }
    }
    pub fn set_height(&mut self, height: Height) {
        self.height = height;
    }
    pub fn height(&self) -> Height {
        self.height
    }
}

impl From<NewBlock> for IbcEvent {
    fn from(v: NewBlock) -> Self {
        IbcEvent::NewBlock(v)
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Attributes {
    pub height: Height,
    pub client_id: ClientId,
    pub client_type: ClientType,
    pub consensus_height: Height,
}

impl Default for Attributes {
    fn default() -> Self {
        Attributes {
            height: Default::default(),
            client_id: Default::default(),
            client_type: ClientType::Tendermint,
            consensus_height: Height::default(),
        }
    }
}

impl core::fmt::Display for Attributes {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> Result<(), core::fmt::Error> {
        write!(
            f,
            "h: {}, cs_h: {}({})",
            self.height, self.client_id, self.consensus_height
        )
    }
}

/// CreateClient event signals the creation of a new on-chain client (IBC client).
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CreateClient(pub Attributes);

impl CreateClient {
    pub fn client_id(&self) -> &ClientId {
        &self.0.client_id
    }
    pub fn height(&self) -> Height {
        self.0.height
    }
    pub fn set_height(&mut self, height: Height) {
        self.0.height = height;
    }
}

impl From<Attributes> for CreateClient {
    fn from(attrs: Attributes) -> Self {
        CreateClient(attrs)
    }
}

impl From<CreateClient> for IbcEvent {
    fn from(v: CreateClient) -> Self {
        IbcEvent::CreateClient(v)
    }
}

impl core::fmt::Display for CreateClient {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> Result<(), core::fmt::Error> {
        write!(f, "{}", self.0)
    }
}

/// UpdateClient event signals a recent update of an on-chain client (IBC Client).
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UpdateClient {
    pub common: Attributes,
    pub header: Option<AnyHeader>,
}

impl UpdateClient {
    pub fn client_id(&self) -> &ClientId {
        &self.common.client_id
    }
    pub fn client_type(&self) -> ClientType {
        self.common.client_type
    }

    pub fn height(&self) -> Height {
        self.common.height
    }

    pub fn set_height(&mut self, height: Height) {
        self.common.height = height;
    }

    pub fn consensus_height(&self) -> Height {
        self.common.consensus_height
    }
}

impl From<Attributes> for UpdateClient {
    fn from(attrs: Attributes) -> Self {
        UpdateClient {
            common: attrs,
            header: None,
        }
    }
}

impl From<UpdateClient> for IbcEvent {
    fn from(v: UpdateClient) -> Self {
        IbcEvent::UpdateClient(v)
    }
}

impl core::fmt::Display for UpdateClient {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> Result<(), core::fmt::Error> {
        write!(f, "{}", self.common)
    }
}

/// ClientMisbehaviour event signals the update of an on-chain client (IBC Client) with evidence of
/// misbehaviour.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ClientMisbehaviour(Attributes);

impl ClientMisbehaviour {
    pub fn client_id(&self) -> &ClientId {
        &self.0.client_id
    }
    pub fn height(&self) -> Height {
        self.0.height
    }
    pub fn set_height(&mut self, height: Height) {
        self.0.height = height;
    }
}

impl From<ClientMisbehaviour> for IbcEvent {
    fn from(v: ClientMisbehaviour) -> Self {
        IbcEvent::ClientMisbehaviour(v)
    }
}

/// Signals a recent upgrade of an on-chain client (IBC Client).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
pub struct UpgradeClient(Attributes);

impl UpgradeClient {
    pub fn set_height(&mut self, height: Height) {
        self.0.height = height;
    }
    pub fn client_id(&self) -> &ClientId {
        &self.0.client_id
    }
}

impl From<Attributes> for UpgradeClient {
    fn from(attrs: Attributes) -> Self {
        UpgradeClient(attrs)
    }
}
