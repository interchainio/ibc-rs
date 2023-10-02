use std::convert::{TryFrom, TryInto};
use std::time::Duration;

use prost::Message;
use serde::{Deserialize, Serialize};

use ibc_proto::google::protobuf::Any;
use ibc_proto::ibc::lightclients::localhost::v1::ClientState as RawClientState;
use ibc_proto::protobuf::Protobuf;

use crate::core::ics02_client::client_state::ClientState as Ics02ClientState;
use crate::core::ics02_client::client_type::ClientType;
use crate::core::ics02_client::error::Error as Ics02Error;
use crate::core::ics24_host::identifier::ChainId;
use crate::Height;

use super::error::Error;

pub const LOCALHOST_V1_CLIENT_STATE_TYPE_URL: &str = "/ibc.lightclients.localhost.v1.ClientState";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClientState {
    pub chain_id: ChainId,
    pub height: Height,
}

impl ClientState {
    pub fn new(chain_id: ChainId, height: Height) -> ClientState {
        Self { chain_id, height }
    }

    pub fn height(&self) -> Height {
        self.height
    }
}

impl Ics02ClientState for ClientState {
    fn chain_id(&self) -> ChainId {
        self.chain_id.clone()
    }

    fn client_type(&self) -> ClientType {
        ClientType::Localhost
    }

    fn latest_height(&self) -> Height {
        self.height
    }

    fn frozen_height(&self) -> Option<Height> {
        None
    }

    fn expired(&self, _elapsed: Duration) -> bool {
        false
    }
}

impl Protobuf<RawClientState> for ClientState {}

impl TryFrom<RawClientState> for ClientState {
    type Error = Error;

    fn try_from(raw: RawClientState) -> Result<Self, Self::Error> {
        Ok(Self {
            chain_id: ChainId::from_string(raw.chain_id.as_str()),
            height: raw
                .height
                .ok_or_else(Error::missing_height)?
                .try_into()
                .map_err(|_| Error::missing_height())?,
        })
    }
}

impl From<ClientState> for RawClientState {
    fn from(value: ClientState) -> Self {
        Self {
            chain_id: value.chain_id.to_string(),
            height: Some(value.height.into()),
        }
    }
}

impl Protobuf<Any> for ClientState {}

impl TryFrom<Any> for ClientState {
    type Error = Ics02Error;

    fn try_from(raw: Any) -> Result<Self, Self::Error> {
        use bytes::Buf;
        use core::ops::Deref;

        fn decode_client_state<B: Buf>(buf: B) -> Result<ClientState, Error> {
            RawClientState::decode(buf)
                .map_err(Error::decode)?
                .try_into()
        }

        match raw.type_url.as_str() {
            LOCALHOST_V1_CLIENT_STATE_TYPE_URL => {
                decode_client_state(raw.value.deref()).map_err(Into::into)
            }
            _ => Err(Ics02Error::unexpected_client_state_type(
                LOCALHOST_V1_CLIENT_STATE_TYPE_URL.to_string(),
                raw.type_url,
            )),
        }
    }
}

impl From<ClientState> for Any {
    fn from(client_state: ClientState) -> Self {
        Any {
            type_url: LOCALHOST_V1_CLIENT_STATE_TYPE_URL.to_string(),
            value: Protobuf::<RawClientState>::encode_vec(&client_state),
        }
    }
}
