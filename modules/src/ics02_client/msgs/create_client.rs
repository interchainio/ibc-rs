//! Definition of domain type message `MsgCreateAnyClient`.

use crate::prelude::*;
use core::convert::TryFrom;

use tendermint_proto::Protobuf;

use ibc_proto::ibc::core::client::v1::MsgCreateClient as RawMsgCreateClient;

use crate::ics02_client::client_consensus::AnyConsensusState;
use crate::ics02_client::client_state::AnyClientState;
use crate::ics02_client::error::Error;
use crate::signer::Signer;
use crate::tx_msg::Msg;

pub const TYPE_URL: &str = "/ibc.core.client.v1.MsgCreateClient";

/// A type of message that triggers the creation of a new on-chain (IBC) client.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MsgCreateAnyClient {
    pub client_state: AnyClientState,
    pub consensus_state: AnyConsensusState,
    pub signer: Signer,
}

impl MsgCreateAnyClient {
    pub fn new(
        client_state: AnyClientState,
        consensus_state: AnyConsensusState,
        signer: Signer,
    ) -> Result<Self, Error> {
        if client_state.client_type() != consensus_state.client_type() {
            return Err(Error::raw_client_and_consensus_state_types_mismatch(
                client_state.client_type(),
                consensus_state.client_type(),
            ));
        }
        Ok(MsgCreateAnyClient {
            client_state,
            consensus_state,
            signer,
        })
    }

    pub fn client_state(&self) -> AnyClientState {
        self.client_state.clone()
    }

    pub fn consensus_state(&self) -> AnyConsensusState {
        self.consensus_state.clone()
    }
}

impl Msg for MsgCreateAnyClient {
    type ValidationError = crate::ics24_host::error::ValidationError;
    type Raw = RawMsgCreateClient;

    fn route(&self) -> String {
        crate::keys::ROUTER_KEY.to_string()
    }

    fn type_url(&self) -> String {
        TYPE_URL.to_string()
    }
}

impl Protobuf<RawMsgCreateClient> for MsgCreateAnyClient {}

impl TryFrom<RawMsgCreateClient> for MsgCreateAnyClient {
    type Error = Error;

    fn try_from(raw: RawMsgCreateClient) -> Result<Self, Error> {
        let raw_client_state = raw
            .client_state
            .ok_or_else(Error::missing_raw_client_state)?;

        let raw_consensus_state = raw
            .consensus_state
            .ok_or_else(Error::missing_raw_client_state)?;

        MsgCreateAnyClient::new(
            AnyClientState::try_from(raw_client_state)?,
            AnyConsensusState::try_from(raw_consensus_state)?,
            raw.signer.into(),
        )
    }
}

impl From<MsgCreateAnyClient> for RawMsgCreateClient {
    fn from(ics_msg: MsgCreateAnyClient) -> Self {
        RawMsgCreateClient {
            client_state: Some(ics_msg.client_state.into()),
            consensus_state: Some(ics_msg.consensus_state.into()),
            signer: ics_msg.signer.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use core::convert::{TryFrom, TryInto};
    use test_env_log::test;

    use ibc_proto::ibc::core::client::v1::MsgCreateClient;

    use crate::ics02_client::client_consensus::AnyConsensusState;
    use crate::ics02_client::msgs::MsgCreateAnyClient;
    use crate::ics07_tendermint::client_state::test_util::get_dummy_tendermint_client_state;
    use crate::ics07_tendermint::header::test_util::get_dummy_tendermint_header;
    use crate::test_utils::get_dummy_account_id;

    #[test]
    fn msg_create_client_serialization() {
        let signer = get_dummy_account_id();

        let tm_header = get_dummy_tendermint_header();
        let tm_client_state = get_dummy_tendermint_client_state(tm_header.clone());

        let msg = MsgCreateAnyClient::new(
            tm_client_state,
            AnyConsensusState::Tendermint(tm_header.try_into().unwrap()),
            signer,
        )
        .unwrap();

        let raw = MsgCreateClient::from(msg.clone());
        let msg_back = MsgCreateAnyClient::try_from(raw.clone()).unwrap();
        let raw_back = MsgCreateClient::from(msg_back.clone());
        assert_eq!(msg, msg_back);
        assert_eq!(raw, raw_back);
    }
}
