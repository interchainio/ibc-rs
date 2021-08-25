//! Definition of domain type message `MsgUpdateAnyClient`.

use crate::prelude::*;
use core::convert::TryFrom;
use tendermint_proto::Protobuf;

use ibc_proto::ibc::core::client::v1::MsgUpdateClient as RawMsgUpdateClient;

use crate::ics02_client::error::Error;
use crate::ics02_client::header::AnyHeader;
use crate::ics24_host::error::ValidationError;
use crate::ics24_host::identifier::ClientId;
use crate::signer::Signer;
use crate::tx_msg::Msg;

pub const TYPE_URL: &str = "/ibc.core.client.v1.MsgUpdateClient";

/// A type of message that triggers the update of an on-chain (IBC) client with new headers.
#[derive(Clone, Debug, PartialEq)] // TODO: Add Eq bound when possible
pub struct MsgUpdateAnyClient {
    pub client_id: ClientId,
    pub header: AnyHeader,
    pub signer: Signer,
}

impl MsgUpdateAnyClient {
    pub fn new(client_id: ClientId, header: AnyHeader, signer: Signer) -> Self {
        MsgUpdateAnyClient {
            client_id,
            header,
            signer,
        }
    }
}

impl Msg for MsgUpdateAnyClient {
    type ValidationError = ValidationError;
    type Raw = RawMsgUpdateClient;

    fn route(&self) -> String {
        crate::keys::ROUTER_KEY.to_string()
    }

    fn type_url(&self) -> String {
        TYPE_URL.to_string()
    }
}

impl Protobuf<RawMsgUpdateClient> for MsgUpdateAnyClient {}

impl TryFrom<RawMsgUpdateClient> for MsgUpdateAnyClient {
    type Error = Error;

    fn try_from(raw: RawMsgUpdateClient) -> Result<Self, Self::Error> {
        let raw_header = raw.header.ok_or_else(Error::missing_raw_header)?;

        Ok(MsgUpdateAnyClient {
            client_id: raw
                .client_id
                .parse()
                .map_err(Error::invalid_msg_update_client_id)?,
            header: AnyHeader::try_from(raw_header)?,
            signer: raw.signer.into(),
        })
    }
}

impl From<MsgUpdateAnyClient> for RawMsgUpdateClient {
    fn from(ics_msg: MsgUpdateAnyClient) -> Self {
        RawMsgUpdateClient {
            client_id: ics_msg.client_id.to_string(),
            header: Some(ics_msg.header.into()),
            signer: ics_msg.signer.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use core::convert::TryFrom;
    use test_env_log::test;

    use ibc_proto::ibc::core::client::v1::MsgUpdateClient;

    use crate::ics02_client::header::AnyHeader;
    use crate::ics02_client::msgs::MsgUpdateAnyClient;
    use crate::ics07_tendermint::header::test_util::get_dummy_ics07_header;
    use crate::ics24_host::identifier::ClientId;
    use crate::test_utils::get_dummy_account_id;

    #[test]
    fn msg_update_client_serialization() {
        let client_id: ClientId = "tendermint".parse().unwrap();
        let signer = get_dummy_account_id();

        let header = get_dummy_ics07_header();

        let msg = MsgUpdateAnyClient::new(client_id, AnyHeader::Tendermint(header), signer);
        let raw = MsgUpdateClient::from(msg.clone());
        let msg_back = MsgUpdateAnyClient::try_from(raw.clone()).unwrap();
        let raw_back = MsgUpdateClient::from(msg_back.clone());
        assert_eq!(msg, msg_back);
        assert_eq!(raw, raw_back);
    }
}
