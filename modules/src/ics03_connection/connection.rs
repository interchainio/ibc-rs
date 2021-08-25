use crate::prelude::*;
use core::convert::{TryFrom, TryInto};
use core::str::FromStr;
use core::time::Duration;
use core::u64;

use serde::{Deserialize, Serialize};
use tendermint_proto::Protobuf;

use ibc_proto::ibc::core::connection::v1::{
    ConnectionEnd as RawConnectionEnd, Counterparty as RawCounterparty,
    IdentifiedConnection as RawIdentifiedConnection,
};

use crate::ics03_connection::error::Error;
use crate::ics03_connection::version::Version;
use crate::ics23_commitment::commitment::CommitmentPrefix;
use crate::ics24_host::error::ValidationError;
use crate::ics24_host::identifier::{ClientId, ConnectionId};
use crate::timestamp::ZERO_DURATION;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IdentifiedConnectionEnd {
    pub connection_id: ConnectionId,
    pub connection_end: ConnectionEnd,
}

impl IdentifiedConnectionEnd {
    pub fn new(connection_id: ConnectionId, connection_end: ConnectionEnd) -> Self {
        IdentifiedConnectionEnd {
            connection_id,
            connection_end,
        }
    }

    pub fn id(&self) -> &ConnectionId {
        &self.connection_id
    }

    pub fn end(&self) -> &ConnectionEnd {
        &self.connection_end
    }
}

impl Protobuf<RawIdentifiedConnection> for IdentifiedConnectionEnd {}

impl TryFrom<RawIdentifiedConnection> for IdentifiedConnectionEnd {
    type Error = Error;

    fn try_from(value: RawIdentifiedConnection) -> Result<Self, Self::Error> {
        let raw_connection_end = RawConnectionEnd {
            client_id: value.client_id.to_string(),
            versions: value.versions,
            state: value.state,
            counterparty: value.counterparty,
            delay_period: value.delay_period,
        };

        Ok(IdentifiedConnectionEnd {
            connection_id: value.id.parse().map_err(Error::invalid_identifier)?,
            connection_end: raw_connection_end.try_into()?,
        })
    }
}

impl From<IdentifiedConnectionEnd> for RawIdentifiedConnection {
    fn from(value: IdentifiedConnectionEnd) -> Self {
        RawIdentifiedConnection {
            id: value.connection_id.to_string(),
            client_id: value.connection_end.client_id.to_string(),
            versions: value
                .connection_end
                .versions
                .iter()
                .map(|v| From::from(v.clone()))
                .collect(),
            state: value.connection_end.state as i32,
            delay_period: value.connection_end.delay_period.as_nanos() as u64,
            counterparty: Some(value.connection_end.counterparty().clone().into()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConnectionEnd {
    pub state: State,
    client_id: ClientId,
    counterparty: Counterparty,
    versions: Vec<Version>,
    delay_period: Duration,
}

impl Default for ConnectionEnd {
    fn default() -> Self {
        Self {
            state: State::Uninitialized,
            client_id: Default::default(),
            counterparty: Default::default(),
            versions: Vec::new(),
            delay_period: ZERO_DURATION,
        }
    }
}

impl Protobuf<RawConnectionEnd> for ConnectionEnd {}

impl TryFrom<RawConnectionEnd> for ConnectionEnd {
    type Error = Error;
    fn try_from(value: RawConnectionEnd) -> Result<Self, Self::Error> {
        let state = value.state.try_into()?;
        if state == State::Uninitialized {
            return Ok(ConnectionEnd::default());
        }
        if value.client_id.is_empty() {
            return Err(Error::empty_proto_connection_end());
        }

        Ok(Self::new(
            state,
            value.client_id.parse().map_err(Error::invalid_identifier)?,
            value
                .counterparty
                .ok_or_else(Error::missing_counterparty)?
                .try_into()?,
            value
                .versions
                .into_iter()
                .map(Version::try_from)
                .collect::<Result<Vec<_>, _>>()?,
            Duration::from_nanos(value.delay_period),
        ))
    }
}

impl From<ConnectionEnd> for RawConnectionEnd {
    fn from(value: ConnectionEnd) -> Self {
        RawConnectionEnd {
            client_id: value.client_id.to_string(),
            versions: value
                .versions
                .iter()
                .map(|v| From::from(v.clone()))
                .collect(),
            state: value.state as i32,
            counterparty: Some(value.counterparty.into()),
            delay_period: value.delay_period.as_nanos() as u64,
        }
    }
}

impl ConnectionEnd {
    pub fn new(
        state: State,
        client_id: ClientId,
        counterparty: Counterparty,
        versions: Vec<Version>,
        delay_period: Duration,
    ) -> Self {
        Self {
            state,
            client_id,
            counterparty,
            versions,
            delay_period,
        }
    }

    /// Getter for the state of this connection end.
    pub fn state(&self) -> &State {
        &self.state
    }

    /// Setter for the `state` field.
    pub fn set_state(&mut self, new_state: State) {
        self.state = new_state;
    }

    /// Setter for the `counterparty` field.
    pub fn set_counterparty(&mut self, new_cparty: Counterparty) {
        self.counterparty = new_cparty;
    }

    /// Setter for the `version` field.
    pub fn set_version(&mut self, new_version: Version) {
        self.versions = vec![new_version];
    }

    /// Helper function to compare the counterparty of this end with another counterparty.
    pub fn counterparty_matches(&self, other: &Counterparty) -> bool {
        self.counterparty.eq(other)
    }

    /// Helper function to compare the client id of this end with another client identifier.
    pub fn client_id_matches(&self, other: &ClientId) -> bool {
        self.client_id.eq(other)
    }

    pub fn is_open(&self) -> bool {
        self.state_matches(&State::Open)
    }

    pub fn is_uninitialized(&self) -> bool {
        self.state_matches(&State::Uninitialized)
    }

    /// Helper function to compare the state of this end with another state.
    pub fn state_matches(&self, other: &State) -> bool {
        self.state.eq(other)
    }

    /// Getter for the client id on the local party of this connection end.
    pub fn client_id(&self) -> &ClientId {
        &self.client_id
    }

    /// Getter for the list of versions in this connection end.
    pub fn versions(&self) -> Vec<Version> {
        self.versions.clone()
    }

    /// Getter for the counterparty.
    pub fn counterparty(&self) -> &Counterparty {
        &self.counterparty
    }

    /// Getter for the delay_period field. This represents the duration, at minimum,
    /// to delay the sending of a packet after the client update for that packet has been submitted.
    pub fn delay_period(&self) -> Duration {
        self.delay_period
    }

    /// TODO: Clean this up, probably not necessary.
    pub fn validate_basic(&self) -> Result<(), ValidationError> {
        self.counterparty.validate_basic()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Counterparty {
    client_id: ClientId,
    pub connection_id: Option<ConnectionId>,
    prefix: CommitmentPrefix,
}

impl Default for Counterparty {
    fn default() -> Self {
        Counterparty {
            client_id: Default::default(),
            connection_id: None,
            prefix: Default::default(),
        }
    }
}

impl Protobuf<RawCounterparty> for Counterparty {}

// Converts from the wire format RawCounterparty. Typically used from the relayer side
// during queries for response validation and to extract the Counterparty structure.
impl TryFrom<RawCounterparty> for Counterparty {
    type Error = Error;

    fn try_from(value: RawCounterparty) -> Result<Self, Self::Error> {
        let connection_id = Some(value.connection_id)
            .filter(|x| !x.is_empty())
            .map(|v| FromStr::from_str(v.as_str()))
            .transpose()
            .map_err(Error::invalid_identifier)?;
        Ok(Counterparty::new(
            value.client_id.parse().map_err(Error::invalid_identifier)?,
            connection_id,
            value
                .prefix
                .ok_or_else(Error::missing_counterparty)?
                .key_prefix
                .into(),
        ))
    }
}

impl From<Counterparty> for RawCounterparty {
    fn from(value: Counterparty) -> Self {
        RawCounterparty {
            client_id: value.client_id.as_str().to_string(),
            connection_id: value
                .connection_id
                .map_or_else(|| "".to_string(), |v| v.as_str().to_string()),
            prefix: Some(ibc_proto::ibc::core::commitment::v1::MerklePrefix {
                key_prefix: value.prefix.into_vec(),
            }),
        }
    }
}

impl Counterparty {
    pub fn new(
        client_id: ClientId,
        connection_id: Option<ConnectionId>,
        prefix: CommitmentPrefix,
    ) -> Self {
        Self {
            client_id,
            connection_id,
            prefix,
        }
    }

    /// Getter for the client id.
    pub fn client_id(&self) -> &ClientId {
        &self.client_id
    }

    /// Getter for connection id.
    pub fn connection_id(&self) -> Option<&ConnectionId> {
        self.connection_id.as_ref()
    }

    pub fn prefix(&self) -> &CommitmentPrefix {
        &self.prefix
    }

    pub fn validate_basic(&self) -> Result<(), ValidationError> {
        Ok(())
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum State {
    Uninitialized = 0,
    Init = 1,
    TryOpen = 2,
    Open = 3,
}

impl State {
    /// Yields the State as a string.
    pub fn as_string(&self) -> &'static str {
        match self {
            Self::Uninitialized => "UNINITIALIZED",
            Self::Init => "INIT",
            Self::TryOpen => "TRYOPEN",
            Self::Open => "OPEN",
        }
    }
    // Parses the State out from a i32.
    pub fn from_i32(s: i32) -> Result<Self, Error> {
        match s {
            0 => Ok(Self::Uninitialized),
            1 => Ok(Self::Init),
            2 => Ok(Self::TryOpen),
            3 => Ok(Self::Open),
            _ => Err(Error::invalid_state(s)),
        }
    }

    /// Returns whether or not this connection state is `Open`.
    pub fn is_open(self) -> bool {
        self == State::Open
    }

    /// Returns whether or not this connection with this state
    /// has progressed less or the same than the argument.
    ///
    /// # Example
    /// ```rust,ignore
    /// assert!(State::Init.less_or_equal_progress(State::Open));
    /// assert!(State::TryOpen.less_or_equal_progress(State::TryOpen));
    /// assert!(!State::Open.less_or_equal_progress(State::Uninitialized));
    /// ```
    pub fn less_or_equal_progress(self, other: Self) -> bool {
        self as u32 <= other as u32
    }
}

impl TryFrom<i32> for State {
    type Error = Error;
    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Uninitialized),
            1 => Ok(Self::Init),
            2 => Ok(Self::TryOpen),
            3 => Ok(Self::Open),
            _ => Err(Error::invalid_state(value)),
        }
    }
}

impl From<State> for i32 {
    fn from(value: State) -> Self {
        value.into()
    }
}
