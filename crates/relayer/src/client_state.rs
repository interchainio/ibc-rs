use core::time::Duration;

use serde::{Deserialize, Serialize};

use ibc_proto::google::protobuf::Any;
use ibc_proto::ibc::core::client::v1::IdentifiedClientState;
use ibc_proto::ibc::lightclients::localhost::v1::ClientState as RawLocalhostV1ClientState;
// use ibc_proto::ibc::lightclients::localhost::v2::ClientState as RawLocalhostV2ClientState;
use ibc_proto::ibc::lightclients::tendermint::v1::ClientState as RawTmClientState;
#[cfg(test)]
use ibc_proto::ibc::mock::ClientState as RawMockClientState;
use ibc_proto::protobuf::Protobuf;
use ibc_relayer_types::clients::ics07_tendermint::client_state::{
    ClientState as TmClientState, TENDERMINT_CLIENT_STATE_TYPE_URL,
};
use ibc_relayer_types::clients::ics09_localhost::v1::client_state::{
    ClientState as LocalhostV1ClientState, LOCALHOST_V1_CLIENT_STATE_TYPE_URL,
};
use ibc_relayer_types::clients::ics09_localhost::v2::client_state::{
    ClientState as LocalhostV2ClientState, RawClientState as RawLocalhostV2ClientState,
    LOCALHOST_V2_CLIENT_STATE_TYPE_URL,
};
use ibc_relayer_types::core::ics02_client::client_state::ClientState;
use ibc_relayer_types::core::ics02_client::client_type::ClientType;
use ibc_relayer_types::core::ics02_client::error::Error;
use ibc_relayer_types::core::ics02_client::trust_threshold::TrustThreshold;

use ibc_relayer_types::core::ics24_host::error::ValidationError;
use ibc_relayer_types::core::ics24_host::identifier::{ChainId, ClientId};
#[cfg(test)]
use ibc_relayer_types::mock::client_state::MockClientState;
#[cfg(test)]
use ibc_relayer_types::mock::client_state::MOCK_CLIENT_STATE_TYPE_URL;
use ibc_relayer_types::Height;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AnyClientState {
    Tendermint(TmClientState),
    LocalhostV1(LocalhostV1ClientState),
    LocalhostV2(LocalhostV2ClientState),

    #[cfg(test)]
    Mock(MockClientState),
}

impl AnyClientState {
    pub fn chain_id(&self) -> ChainId {
        match self {
            Self::Tendermint(tm_state) => tm_state.chain_id(),
            Self::LocalhostV1(local_state) => local_state.chain_id(),
            Self::LocalhostV2(local_state) => local_state.chain_id(),

            #[cfg(test)]
            Self::Mock(mock_state) => mock_state.chain_id(),
        }
    }

    pub fn latest_height(&self) -> Height {
        match self {
            Self::Tendermint(tm_state) => tm_state.latest_height(),
            Self::LocalhostV1(local_state) => local_state.latest_height(),
            Self::LocalhostV2(local_state) => local_state.latest_height(),

            #[cfg(test)]
            Self::Mock(mock_state) => mock_state.latest_height(),
        }
    }

    pub fn frozen_height(&self) -> Option<Height> {
        match self {
            Self::Tendermint(tm_state) => tm_state.frozen_height(),
            Self::LocalhostV1(local_state) => local_state.frozen_height(),
            Self::LocalhostV2(local_state) => local_state.frozen_height(),

            #[cfg(test)]
            Self::Mock(mock_state) => mock_state.frozen_height(),
        }
    }

    pub fn trust_threshold(&self) -> Option<TrustThreshold> {
        match self {
            Self::Tendermint(state) => Some(state.trust_threshold),
            Self::LocalhostV1(_) => None,
            Self::LocalhostV2(_) => None,

            #[cfg(test)]
            Self::Mock(_) => None,
        }
    }

    pub fn max_clock_drift(&self) -> Duration {
        match self {
            Self::Tendermint(state) => state.max_clock_drift,
            Self::LocalhostV1(_) => Duration::new(0, 0),
            Self::LocalhostV2(_) => Duration::new(0, 0),

            #[cfg(test)]
            Self::Mock(_) => Duration::new(0, 0),
        }
    }

    pub fn client_type(&self) -> ClientType {
        match self {
            Self::Tendermint(state) => state.client_type(),
            Self::LocalhostV1(state) => state.client_type(),
            Self::LocalhostV2(state) => state.client_type(),

            #[cfg(test)]
            Self::Mock(state) => state.client_type(),
        }
    }

    pub fn refresh_period(&self) -> Option<Duration> {
        match self {
            AnyClientState::Tendermint(tm_state) => tm_state.refresh_time(),
            AnyClientState::LocalhostV1(_) => None,
            AnyClientState::LocalhostV2(_) => None,

            #[cfg(test)]
            AnyClientState::Mock(mock_state) => mock_state.refresh_time(),
        }
    }

    pub fn expired(&self, elapsed_since_latest: Duration) -> bool {
        match self {
            AnyClientState::Tendermint(tm_state) => tm_state.expired(elapsed_since_latest),
            AnyClientState::LocalhostV1(local_state) => local_state.expired(elapsed_since_latest),
            AnyClientState::LocalhostV2(local_state) => local_state.expired(elapsed_since_latest),

            #[cfg(test)]
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
                Protobuf::<RawTmClientState>::decode_vec(&raw.value)
                    .map_err(Error::decode_raw_client_state)?,
            )),

            LOCALHOST_V1_CLIENT_STATE_TYPE_URL => Ok(AnyClientState::LocalhostV1(
                Protobuf::<RawLocalhostV1ClientState>::decode_vec(&raw.value)
                    .map_err(Error::decode_raw_client_state)?,
            )),

            LOCALHOST_V2_CLIENT_STATE_TYPE_URL => Ok(AnyClientState::LocalhostV2(
                Protobuf::<RawLocalhostV2ClientState>::decode_vec(&raw.value)
                    .map_err(Error::decode_raw_client_state)?,
            )),

            #[cfg(test)]
            MOCK_CLIENT_STATE_TYPE_URL => Ok(AnyClientState::Mock(
                Protobuf::<RawMockClientState>::decode_vec(&raw.value)
                    .map_err(Error::decode_raw_client_state)?,
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
                value: Protobuf::<RawTmClientState>::encode_vec(&value),
            },

            AnyClientState::LocalhostV1(value) => Any {
                type_url: LOCALHOST_V1_CLIENT_STATE_TYPE_URL.to_string(),
                value: Protobuf::<RawLocalhostV1ClientState>::encode_vec(&value),
            },

            AnyClientState::LocalhostV2(value) => Any {
                type_url: LOCALHOST_V2_CLIENT_STATE_TYPE_URL.to_string(),
                value: Protobuf::<RawLocalhostV2ClientState>::encode_vec(&value),
            },

            #[cfg(test)]
            AnyClientState::Mock(value) => Any {
                type_url: MOCK_CLIENT_STATE_TYPE_URL.to_string(),
                value: Protobuf::<RawMockClientState>::encode_vec(&value),
            },
        }
    }
}

impl ClientState for AnyClientState {
    fn chain_id(&self) -> ChainId {
        self.chain_id()
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

    fn expired(&self, elapsed_since_latest: Duration) -> bool {
        self.expired(elapsed_since_latest)
    }
}

impl From<TmClientState> for AnyClientState {
    fn from(cs: TmClientState) -> Self {
        Self::Tendermint(cs)
    }
}

impl From<LocalhostV1ClientState> for AnyClientState {
    fn from(cs: LocalhostV1ClientState) -> Self {
        Self::LocalhostV1(cs)
    }
}

impl From<LocalhostV2ClientState> for AnyClientState {
    fn from(cs: LocalhostV2ClientState) -> Self {
        Self::LocalhostV2(cs)
    }
}

#[cfg(test)]
impl From<MockClientState> for AnyClientState {
    fn from(cs: MockClientState) -> Self {
        Self::Mock(cs)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
    use ibc_relayer_types::clients::ics07_tendermint::client_state::test_util::get_dummy_tendermint_client_state;
    use ibc_relayer_types::clients::ics07_tendermint::header::test_util::get_dummy_tendermint_header;
    use test_log::test;

    use super::AnyClientState;

    #[test]
    fn any_client_state_serialization() {
        let tm_client_state: AnyClientState =
            get_dummy_tendermint_client_state(get_dummy_tendermint_header()).into();

        let raw: Any = tm_client_state.clone().into();
        let tm_client_state_back = AnyClientState::try_from(raw).unwrap();
        assert_eq!(tm_client_state, tm_client_state_back);
    }
}
