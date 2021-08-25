use crate::prelude::*;
use core::convert::{TryFrom, TryInto};
use tendermint_proto::Protobuf;

use ibc_proto::ibc::lightclients::tendermint::v1::Misbehaviour as RawMisbehaviour;

use crate::ics02_client::misbehaviour::AnyMisbehaviour;
use crate::ics07_tendermint::error::Error;
use crate::ics07_tendermint::header::Header;
use crate::ics24_host::identifier::ClientId;
use crate::Height;

#[derive(Clone, Debug, PartialEq)]
pub struct Misbehaviour {
    pub client_id: ClientId,
    pub header1: Header,
    pub header2: Header,
}

impl crate::ics02_client::misbehaviour::Misbehaviour for Misbehaviour {
    fn client_id(&self) -> &ClientId {
        &self.client_id
    }

    fn height(&self) -> Height {
        self.header1.height()
    }

    fn wrap_any(self) -> AnyMisbehaviour {
        AnyMisbehaviour::Tendermint(self)
    }
}

impl Protobuf<RawMisbehaviour> for Misbehaviour {}

impl TryFrom<RawMisbehaviour> for Misbehaviour {
    type Error = Error;

    fn try_from(raw: RawMisbehaviour) -> Result<Self, Self::Error> {
        Ok(Self {
            client_id: Default::default(),
            header1: raw
                .header_1
                .ok_or_else(|| Error::invalid_raw_misbehaviour("missing header1".into()))?
                .try_into()?,
            header2: raw
                .header_2
                .ok_or_else(|| Error::invalid_raw_misbehaviour("missing header2".into()))?
                .try_into()?,
        })
    }
}

impl From<Misbehaviour> for RawMisbehaviour {
    fn from(value: Misbehaviour) -> Self {
        RawMisbehaviour {
            client_id: value.client_id.to_string(),
            header_1: Some(value.header1.into()),
            header_2: Some(value.header2.into()),
        }
    }
}

impl core::fmt::Display for Misbehaviour {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> Result<(), core::fmt::Error> {
        write!(
            f,
            "{:?} h1: {:?}-{:?} h2: {:?}-{:?}",
            self.client_id,
            self.header1.height(),
            self.header1.trusted_height,
            self.header2.height(),
            self.header2.trusted_height,
        )
    }
}
