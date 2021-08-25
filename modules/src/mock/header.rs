use core::convert::TryFrom;

use serde_derive::{Deserialize, Serialize};
use tendermint_proto::Protobuf;

use ibc_proto::ibc::mock::Header as RawMockHeader;

use crate::ics02_client::client_consensus::AnyConsensusState;
use crate::ics02_client::client_type::ClientType;
use crate::ics02_client::error::Error;
use crate::ics02_client::header::AnyHeader;
use crate::ics02_client::header::Header;
use crate::mock::client_state::MockConsensusState;
use crate::timestamp::Timestamp;
use crate::Height;

#[derive(Copy, Clone, Default, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct MockHeader {
    pub height: Height,
    pub timestamp: Timestamp,
}

impl Protobuf<RawMockHeader> for MockHeader {}

impl TryFrom<RawMockHeader> for MockHeader {
    type Error = Error;

    fn try_from(raw: RawMockHeader) -> Result<Self, Self::Error> {
        Ok(MockHeader {
            height: raw.height.ok_or_else(Error::missing_raw_header)?.into(),

            timestamp: Timestamp::from_nanoseconds(raw.timestamp)
                .map_err(Error::invalid_packet_timestamp)?,
        })
    }
}

impl From<MockHeader> for RawMockHeader {
    fn from(value: MockHeader) -> Self {
        RawMockHeader {
            height: Some(value.height.into()),
            timestamp: value.timestamp.as_nanoseconds(),
        }
    }
}

impl MockHeader {
    pub fn height(&self) -> Height {
        self.height
    }

    pub fn new(height: Height) -> Self {
        Self {
            height,
            timestamp: Default::default(),
        }
    }
}

impl From<MockHeader> for AnyHeader {
    fn from(mh: MockHeader) -> Self {
        Self::Mock(mh)
    }
}

impl Header for MockHeader {
    fn client_type(&self) -> ClientType {
        ClientType::Mock
    }

    fn height(&self) -> Height {
        self.height
    }

    fn wrap_any(self) -> AnyHeader {
        AnyHeader::Mock(self)
    }
}

impl From<MockHeader> for AnyConsensusState {
    fn from(h: MockHeader) -> Self {
        AnyConsensusState::Mock(MockConsensusState::new(h))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_any() {
        let header = MockHeader::new(Height::new(1, 10));
        let bytes = header.wrap_any().encode_vec().unwrap();

        assert_eq!(
            &bytes,
            &[
                10, 16, 47, 105, 98, 99, 46, 109, 111, 99, 107, 46, 72, 101, 97, 100, 101, 114, 18,
                6, 10, 4, 8, 1, 16, 10,
            ]
        );
    }
}
