use std::convert::{TryFrom, TryInto};
use std::time::Duration;

use prost::Message;
use serde::{Deserialize, Serialize};

use ibc_proto::google::protobuf::Any;
use ibc_proto::protobuf::Protobuf;

use crate::core::ics02_client::client_state::ClientState as Ics02ClientState;
use crate::core::ics02_client::client_type::ClientType;
use crate::core::ics02_client::error::Error as Ics02Error;
use crate::core::ics24_host::identifier::ChainId;
use crate::Height;

use super::error::Error;

pub use ibc_proto::ibc::lightclients::localhost::v2::ClientState as RawClientState;

pub const LOCALHOST_V2_CLIENT_STATE_TYPE_URL: &str = "/ibc.lightclients.localhost.v2.ClientState";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClientState {
    pub latest_height: Height,
}

impl ClientState {
    pub fn new(latest_height: Height) -> ClientState {
        Self { latest_height }
    }

    pub fn height(&self) -> Height {
        self.latest_height
    }
}

impl Ics02ClientState for ClientState {
    fn chain_id(&self) -> ChainId {
        unimplemented!()
    }

    fn client_type(&self) -> ClientType {
        ClientType::Localhost
    }

    fn latest_height(&self) -> Height {
        self.latest_height
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
            latest_height: raw
                .latest_height
                .ok_or_else(Error::missing_latest_height)?
                .try_into()
                .map_err(|_| Error::missing_latest_height())?,
        })
    }
}

impl From<ClientState> for RawClientState {
    fn from(value: ClientState) -> Self {
        Self {
            latest_height: Some(value.latest_height.into()),
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
            LOCALHOST_V2_CLIENT_STATE_TYPE_URL => {
                decode_client_state(raw.value.deref()).map_err(Into::into)
            }
            _ => Err(Ics02Error::unexpected_client_state_type(
                LOCALHOST_V2_CLIENT_STATE_TYPE_URL.to_string(),
                raw.type_url,
            )),
        }
    }
}

impl From<ClientState> for Any {
    fn from(client_state: ClientState) -> Self {
        Any {
            type_url: LOCALHOST_V2_CLIENT_STATE_TYPE_URL.to_string(),
            value: Protobuf::<RawClientState>::encode_vec(&client_state),
        }
    }
}
