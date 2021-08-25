use crate::prelude::*;
use alloc::collections::btree_map::BTreeMap as HashMap;
use core::convert::Infallible;
use core::convert::{TryFrom, TryInto};
use core::time::Duration;

use serde::{Deserialize, Serialize};
use tendermint_proto::Protobuf;

use ibc_proto::ibc::mock::ClientState as RawMockClientState;
use ibc_proto::ibc::mock::ConsensusState as RawMockConsensusState;

use crate::ics02_client::client_consensus::{AnyConsensusState, ConsensusState};
use crate::ics02_client::client_state::{AnyClientState, ClientState};
use crate::ics02_client::client_type::ClientType;
use crate::ics02_client::error::Error;
use crate::ics23_commitment::commitment::CommitmentRoot;
use crate::ics24_host::identifier::ChainId;
use crate::mock::header::MockHeader;
use crate::timestamp::Timestamp;
use crate::Height;

/// A mock of an IBC client record as it is stored in a mock context.
/// For testing ICS02 handlers mostly, cf. `MockClientContext`.
#[derive(Clone, Debug)]
pub struct MockClientRecord {
    /// The type of this client.
    pub client_type: ClientType,

    /// The client state (representing only the latest height at the moment).
    pub client_state: Option<AnyClientState>,

    /// Mapping of heights to consensus states for this client.
    pub consensus_states: HashMap<Height, AnyConsensusState>,
}

/// A mock of a client state. For an example of a real structure that this mocks, you can see
/// `ClientState` of ics07_tendermint/client_state.rs.
// TODO: `MockClientState` should evolve, at the very least needs a `is_frozen` boolean field.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MockClientState(pub MockHeader);

impl Protobuf<RawMockClientState> for MockClientState {}

impl MockClientState {
    pub fn latest_height(&self) -> Height {
        (self.0).height
    }

    pub fn refresh_time(&self) -> Option<Duration> {
        None
    }
    pub fn expired(&self, _elapsed: Duration) -> bool {
        false
    }
}

impl From<MockClientState> for AnyClientState {
    fn from(mcs: MockClientState) -> Self {
        Self::Mock(mcs)
    }
}

impl TryFrom<RawMockClientState> for MockClientState {
    type Error = Error;

    fn try_from(raw: RawMockClientState) -> Result<Self, Self::Error> {
        Ok(MockClientState(raw.header.unwrap().try_into()?))
    }
}

impl From<MockClientState> for RawMockClientState {
    fn from(value: MockClientState) -> Self {
        RawMockClientState {
            header: Some(ibc_proto::ibc::mock::Header {
                height: Some(value.0.height().into()),
                timestamp: (value.0).timestamp.as_nanoseconds(),
            }),
        }
    }
}

impl ClientState for MockClientState {
    fn chain_id(&self) -> ChainId {
        todo!()
    }

    fn client_type(&self) -> ClientType {
        ClientType::Mock
    }

    fn latest_height(&self) -> Height {
        self.0.height()
    }

    fn is_frozen(&self) -> bool {
        // TODO
        false
    }

    fn wrap_any(self) -> AnyClientState {
        AnyClientState::Mock(self)
    }
}

impl From<MockConsensusState> for MockClientState {
    fn from(cs: MockConsensusState) -> Self {
        Self(cs.header)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MockConsensusState {
    pub header: MockHeader,
    pub root: CommitmentRoot,
}

impl MockConsensusState {
    pub fn new(header: MockHeader) -> Self {
        MockConsensusState {
            header,
            root: CommitmentRoot::from(vec![0]),
        }
    }

    pub fn timestamp(&self) -> Timestamp {
        self.header.timestamp
    }
}

impl Protobuf<RawMockConsensusState> for MockConsensusState {}

impl TryFrom<RawMockConsensusState> for MockConsensusState {
    type Error = Error;

    fn try_from(raw: RawMockConsensusState) -> Result<Self, Self::Error> {
        let raw_header = raw.header.ok_or_else(Error::missing_raw_consensus_state)?;

        Ok(Self {
            header: MockHeader::try_from(raw_header)?,
            root: CommitmentRoot::from(vec![0]),
        })
    }
}

impl From<MockConsensusState> for RawMockConsensusState {
    fn from(value: MockConsensusState) -> Self {
        RawMockConsensusState {
            header: Some(ibc_proto::ibc::mock::Header {
                height: Some(value.header.height().into()),
                timestamp: value.header.timestamp.as_nanoseconds(),
            }),
        }
    }
}

impl From<MockConsensusState> for AnyConsensusState {
    fn from(mcs: MockConsensusState) -> Self {
        Self::Mock(mcs)
    }
}

impl ConsensusState for MockConsensusState {
    type Error = Infallible;

    fn client_type(&self) -> ClientType {
        ClientType::Mock
    }

    fn root(&self) -> &CommitmentRoot {
        &self.root
    }

    fn validate_basic(&self) -> Result<(), Infallible> {
        Ok(())
    }

    fn wrap_any(self) -> AnyConsensusState {
        AnyConsensusState::Mock(self)
    }
}
