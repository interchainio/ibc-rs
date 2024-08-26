use std::str::FromStr;

use serde::Deserialize;
use serde::Serialize;

use ibc_proto::ibc::applications::transfer::v1::Hop as RawHop;

use crate::applications::transfer::error::Error;
use crate::core::ics24_host::identifier::ChannelId;
use crate::core::ics24_host::identifier::PortId;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hop {
    pub port_id: PortId,
    pub channel_id: ChannelId,
}

impl From<Hop> for RawHop {
    fn from(value: Hop) -> Self {
        RawHop {
            port_id: value.port_id.to_string(),
            channel_id: value.channel_id.to_string(),
        }
    }
}

impl TryFrom<RawHop> for Hop {
    type Error = Error;

    fn try_from(value: RawHop) -> Result<Self, Self::Error> {
        Ok(Hop {
            port_id: PortId::from_str(&value.port_id)
                .map_err(|e| Error::invalid_port_id(value.port_id, e))?,
            channel_id: ChannelId::from_str(&value.channel_id)
                .map_err(|e| Error::invalid_channel_id(value.channel_id, e))?,
        })
    }
}
