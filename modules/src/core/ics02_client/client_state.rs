use core::time::Duration;

use ibc_proto::google::protobuf::Any;
use serde::{Deserialize, Serialize};
use tendermint_proto::Protobuf;

use ibc_client_tendermint::client_state;
use ibc_client_tendermint::client_state::ClientState as TmClientState;
use ibc_proto::ibc::core::client::v1::IdentifiedClientState;

use crate::core::ics02_client::client_type::ClientType;
use crate::core::ics02_client::error::Error;
use crate::core::ics02_client::trust_threshold::TrustThreshold;
use crate::core::ics24_host::error::ValidationError;
use crate::core::ics24_host::identifier::{ChainId, ClientId};
#[cfg(any(test, feature = "mocks"))]
use crate::mock::client_state::MockClientState;
use crate::prelude::*;
use crate::Height;

pub use ibc_base::ics02_client::client_state::*;

pub const TENDERMINT_CLIENT_STATE_TYPE_URL: &str = "/ibc.lightclients.tendermint.v1.ClientState";
pub const MOCK_CLIENT_STATE_TYPE_URL: &str = "/ibc.mock.ClientState";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AnyUpgradeOptions {
    Tendermint(client_state::UpgradeOptions),

    #[cfg(any(test, feature = "mocks"))]
    Mock(()),
}

impl AnyUpgradeOptions {
    fn into_tendermint(self) -> client_state::UpgradeOptions {
        match self {
            Self::Tendermint(options) => options,

            #[cfg(any(test, feature = "mocks"))]
            Self::Mock(_) => {
                panic!("cannot downcast AnyUpgradeOptions::Mock to Tendermint::UpgradeOptions")
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AnyClientState {
    Tendermint(client_state::ClientState),

    #[cfg(any(test, feature = "mocks"))]
    Mock(MockClientState),
}

impl AnyClientState {
    pub fn latest_height(&self) -> Height {
        match self {
            Self::Tendermint(tm_state) => tm_state.latest_height(),

            #[cfg(any(test, feature = "mocks"))]
            Self::Mock(mock_state) => mock_state.latest_height(),
        }
    }

    pub fn frozen_height(&self) -> Option<Height> {
        match self {
            Self::Tendermint(tm_state) => tm_state.frozen_height(),

            #[cfg(any(test, feature = "mocks"))]
            Self::Mock(mock_state) => mock_state.frozen_height(),
        }
    }

    pub fn trust_threshold(&self) -> Option<TrustThreshold> {
        match self {
            AnyClientState::Tendermint(state) => Some(state.trust_level),

            #[cfg(any(test, feature = "mocks"))]
            AnyClientState::Mock(_) => None,
        }
    }

    pub fn max_clock_drift(&self) -> Duration {
        match self {
            AnyClientState::Tendermint(state) => state.max_clock_drift,

            #[cfg(any(test, feature = "mocks"))]
            AnyClientState::Mock(_) => Duration::new(0, 0),
        }
    }

    pub fn client_type(&self) -> ClientType {
        match self {
            Self::Tendermint(state) => state.client_type(),

            #[cfg(any(test, feature = "mocks"))]
            Self::Mock(state) => state.client_type(),
        }
    }

    pub fn refresh_period(&self) -> Option<Duration> {
        match self {
            AnyClientState::Tendermint(tm_state) => tm_state.refresh_time(),

            #[cfg(any(test, feature = "mocks"))]
            AnyClientState::Mock(mock_state) => mock_state.refresh_time(),
        }
    }

    pub fn expired(&self, elapsed_since_latest: Duration) -> bool {
        match self {
            AnyClientState::Tendermint(tm_state) => tm_state.expired(elapsed_since_latest),

            #[cfg(any(test, feature = "mocks"))]
            AnyClientState::Mock(mock_state) => mock_state.expired(elapsed_since_latest),
        }
    }
}

impl Protobuf<Any> for AnyClientState {}

impl TryFrom<Any> for AnyClientState {
    type Error = Error;

    fn try_from(raw: Any) -> Result<Self, Self::Error> {
        match raw.type_url.as_str() {
            "" => Err(Error::empty_client_state_response()),

            TENDERMINT_CLIENT_STATE_TYPE_URL => Ok(AnyClientState::Tendermint(
                client_state::ClientState::decode_vec(&raw.value)
                    .map_err(Error::decode_raw_client_state)?,
            )),

            #[cfg(any(test, feature = "mocks"))]
            MOCK_CLIENT_STATE_TYPE_URL => Ok(AnyClientState::Mock(
                MockClientState::decode_vec(&raw.value).map_err(Error::decode_raw_client_state)?,
            )),

            _ => Err(Error::unknown_client_state_type(raw.type_url)),
        }
    }
}

impl From<AnyClientState> for Any {
    fn from(value: AnyClientState) -> Self {
        match value {
            AnyClientState::Tendermint(value) => Any {
                type_url: TENDERMINT_CLIENT_STATE_TYPE_URL.to_string(),
                value: ClientState::encode_vec(&value)
                    .expect("encoding to `Any` from `AnyClientState::Tendermint`"),
            },
            #[cfg(any(test, feature = "mocks"))]
            AnyClientState::Mock(value) => Any {
                type_url: MOCK_CLIENT_STATE_TYPE_URL.to_string(),
                value: ClientState::encode_vec(&value)
                    .expect("encoding to `Any` from `AnyClientState::Mock`"),
            },
        }
    }
}

impl ClientState for AnyClientState {
    type UpgradeOptions = AnyUpgradeOptions;

    fn chain_id(&self) -> ChainId {
        match self {
            AnyClientState::Tendermint(tm_state) => tm_state.chain_id(),

            #[cfg(any(test, feature = "mocks"))]
            AnyClientState::Mock(mock_state) => mock_state.chain_id(),
        }
    }

    fn client_type(&self) -> ClientType {
        self.client_type()
    }

    fn latest_height(&self) -> Height {
        self.latest_height()
    }

    fn frozen_height(&self) -> Option<Height> {
        self.frozen_height()
    }

    fn upgrade(
        &mut self,
        upgrade_height: Height,
        upgrade_options: Self::UpgradeOptions,
        chain_id: ChainId,
    ) {
        match self {
            AnyClientState::Tendermint(tm_state) => {
                tm_state.upgrade(upgrade_height, upgrade_options.into_tendermint(), chain_id)
            }

            #[cfg(any(test, feature = "mocks"))]
            AnyClientState::Mock(mock_state) => mock_state.upgrade(upgrade_height, (), chain_id),
        }
    }

    fn encode_vec(&self) -> Result<Vec<u8>, Error> {
        Protobuf::encode_vec(self).map_err(Error::invalid_any_client_state)
    }
}

impl From<TmClientState> for AnyClientState {
    fn from(cs: TmClientState) -> Self {
        Self::Tendermint(cs)
    }
}

#[cfg(any(test, feature = "mocks"))]
impl From<MockClientState> for AnyClientState {
    fn from(cs: MockClientState) -> Self {
        Self::Mock(cs)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub struct IdentifiedAnyClientState {
    pub client_id: ClientId,
    pub client_state: AnyClientState,
}

impl IdentifiedAnyClientState {
    pub fn new(client_id: ClientId, client_state: AnyClientState) -> Self {
        IdentifiedAnyClientState {
            client_id,
            client_state,
        }
    }
}

impl Protobuf<IdentifiedClientState> for IdentifiedAnyClientState {}

impl TryFrom<IdentifiedClientState> for IdentifiedAnyClientState {
    type Error = Error;

    fn try_from(raw: IdentifiedClientState) -> Result<Self, Self::Error> {
        Ok(IdentifiedAnyClientState {
            client_id: raw.client_id.parse().map_err(|e: ValidationError| {
                Error::invalid_raw_client_id(raw.client_id.clone(), e)
            })?,
            client_state: raw
                .client_state
                .ok_or_else(Error::missing_raw_client_state)?
                .try_into()?,
        })
    }
}

impl From<IdentifiedAnyClientState> for IdentifiedClientState {
    fn from(value: IdentifiedAnyClientState) -> Self {
        IdentifiedClientState {
            client_id: value.client_id.to_string(),
            client_state: Some(value.client_state.into()),
        }
    }
}

#[cfg(test)]
mod tests {

    use ibc_proto::google::protobuf::Any;
    use test_log::test;

    use crate::core::ics02_client::client_state::AnyClientState;
    use ibc_client_tendermint::client_state::test_util::get_dummy_tendermint_client_state;
    use ibc_client_tendermint::header::test_util::get_dummy_tendermint_header;

    #[test]
    fn any_client_state_serialization() {
        let tm_client_state: AnyClientState =
            get_dummy_tendermint_client_state(get_dummy_tendermint_header()).into();

        let raw: Any = tm_client_state.clone().into();
        let tm_client_state_back = AnyClientState::try_from(raw).unwrap();
        assert_eq!(tm_client_state, tm_client_state_back);
    }
}
