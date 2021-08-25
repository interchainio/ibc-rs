use crate::prelude::*;
use core::convert::TryFrom;

use tendermint_proto::Protobuf;

use ibc_proto::ibc::core::client::v1::MsgSubmitMisbehaviour as RawMsgSubmitMisbehaviour;

use crate::ics02_client::error::Error;
use crate::ics02_client::misbehaviour::AnyMisbehaviour;
use crate::ics24_host::identifier::ClientId;
use crate::signer::Signer;
use crate::tx_msg::Msg;

pub const TYPE_URL: &str = "/ibc.core.client.v1.MsgSubmitMisbehaviour";

/// A type of message that submits client misbehaviour proof.
#[derive(Clone, Debug, PartialEq)]
pub struct MsgSubmitAnyMisbehaviour {
    /// client unique identifier
    pub client_id: ClientId,
    /// misbehaviour used for freezing the light client
    pub misbehaviour: AnyMisbehaviour,
    /// signer address
    pub signer: Signer,
}

impl Msg for MsgSubmitAnyMisbehaviour {
    type ValidationError = crate::ics24_host::error::ValidationError;
    type Raw = RawMsgSubmitMisbehaviour;

    fn route(&self) -> String {
        crate::keys::ROUTER_KEY.to_string()
    }

    fn type_url(&self) -> String {
        TYPE_URL.to_string()
    }
}

impl Protobuf<RawMsgSubmitMisbehaviour> for MsgSubmitAnyMisbehaviour {}

impl TryFrom<RawMsgSubmitMisbehaviour> for MsgSubmitAnyMisbehaviour {
    type Error = Error;

    fn try_from(raw: RawMsgSubmitMisbehaviour) -> Result<Self, Self::Error> {
        let raw_misbehaviour = raw
            .misbehaviour
            .ok_or_else(Error::missing_raw_misbehaviour)?;

        Ok(MsgSubmitAnyMisbehaviour {
            client_id: raw
                .client_id
                .parse()
                .map_err(Error::invalid_raw_misbehaviour)?,
            misbehaviour: AnyMisbehaviour::try_from(raw_misbehaviour)?,
            signer: raw.signer.into(),
        })
    }
}

impl From<MsgSubmitAnyMisbehaviour> for RawMsgSubmitMisbehaviour {
    fn from(ics_msg: MsgSubmitAnyMisbehaviour) -> Self {
        RawMsgSubmitMisbehaviour {
            client_id: ics_msg.client_id.to_string(),
            misbehaviour: Some(ics_msg.misbehaviour.into()),
            signer: ics_msg.signer.to_string(),
        }
    }
}
