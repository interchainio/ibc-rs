use crate::prelude::*;
use core::convert::{TryFrom, TryInto};
use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Serialize};
use tendermint_proto::Protobuf;

use ibc_proto::ibc::core::channel::v1::{
    Channel as RawChannel, Counterparty as RawCounterparty,
    IdentifiedChannel as RawIdentifiedChannel,
};

use crate::events::IbcEventType;
use crate::ics02_client::height::Height;
use crate::ics04_channel::{error::Error, packet::Sequence};
use crate::ics24_host::identifier::{ChannelId, ConnectionId, PortId};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentifiedChannelEnd {
    pub port_id: PortId,
    pub channel_id: ChannelId,
    pub channel_end: ChannelEnd,
}

impl IdentifiedChannelEnd {
    pub fn new(port_id: PortId, channel_id: ChannelId, channel_end: ChannelEnd) -> Self {
        IdentifiedChannelEnd {
            port_id,
            channel_id,
            channel_end,
        }
    }
}

impl Protobuf<RawIdentifiedChannel> for IdentifiedChannelEnd {}

impl TryFrom<RawIdentifiedChannel> for IdentifiedChannelEnd {
    type Error = Error;

    fn try_from(value: RawIdentifiedChannel) -> Result<Self, Self::Error> {
        let raw_channel_end = RawChannel {
            state: value.state,
            ordering: value.ordering,
            counterparty: value.counterparty,
            connection_hops: value.connection_hops,
            version: value.version,
        };

        Ok(IdentifiedChannelEnd {
            port_id: value.port_id.parse().map_err(Error::identifier)?,
            channel_id: value.channel_id.parse().map_err(Error::identifier)?,
            channel_end: raw_channel_end.try_into()?,
        })
    }
}

impl From<IdentifiedChannelEnd> for RawIdentifiedChannel {
    fn from(value: IdentifiedChannelEnd) -> Self {
        RawIdentifiedChannel {
            state: value.channel_end.state as i32,
            ordering: value.channel_end.ordering as i32,
            counterparty: Some(value.channel_end.counterparty().clone().into()),
            connection_hops: value
                .channel_end
                .connection_hops
                .iter()
                .map(|v| v.as_str().to_string())
                .collect(),
            version: value.channel_end.version,
            port_id: value.port_id.to_string(),
            channel_id: value.channel_id.to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelEnd {
    pub state: State,
    pub ordering: Order,
    pub remote: Counterparty,
    pub connection_hops: Vec<ConnectionId>,
    pub version: String,
}

impl Default for ChannelEnd {
    fn default() -> Self {
        ChannelEnd {
            state: State::Uninitialized,
            ordering: Default::default(),
            remote: Counterparty::default(),
            connection_hops: Vec::new(),
            version: "".to_string(),
        }
    }
}

impl Protobuf<RawChannel> for ChannelEnd {}

impl TryFrom<RawChannel> for ChannelEnd {
    type Error = Error;

    fn try_from(value: RawChannel) -> Result<Self, Self::Error> {
        let chan_state: State = State::from_i32(value.state)?;

        if chan_state == State::Uninitialized {
            return Ok(ChannelEnd::default());
        }

        let chan_ordering = Order::from_i32(value.ordering)?;

        // Assemble the 'remote' attribute of the Channel, which represents the Counterparty.
        let remote = value
            .counterparty
            .ok_or_else(Error::missing_counterparty)?
            .try_into()?;

        // Parse each item in connection_hops into a ConnectionId.
        let connection_hops = value
            .connection_hops
            .into_iter()
            .map(|conn_id| ConnectionId::from_str(conn_id.as_str()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(Error::identifier)?;

        let version = validate_version(value.version)?;

        Ok(ChannelEnd::new(
            chan_state,
            chan_ordering,
            remote,
            connection_hops,
            version,
        ))
    }
}

impl From<ChannelEnd> for RawChannel {
    fn from(value: ChannelEnd) -> Self {
        RawChannel {
            state: value.state as i32,
            ordering: value.ordering as i32,
            counterparty: Some(value.counterparty().clone().into()),
            connection_hops: value
                .connection_hops
                .iter()
                .map(|v| v.as_str().to_string())
                .collect(),
            version: value.version,
        }
    }
}

impl ChannelEnd {
    /// Creates a new ChannelEnd in state Uninitialized and other fields parametrized.
    pub fn new(
        state: State,
        ordering: Order,
        remote: Counterparty,
        connection_hops: Vec<ConnectionId>,
        version: String,
    ) -> Self {
        Self {
            state,
            ordering,
            remote,
            connection_hops,
            version,
        }
    }

    /// Updates the ChannelEnd to assume a new State 's'.
    pub fn set_state(&mut self, s: State) {
        self.state = s;
    }

    pub fn set_version(&mut self, v: String) {
        self.version = v;
    }

    pub fn set_counterparty_channel_id(&mut self, c: ChannelId) {
        self.remote.channel_id = Some(c);
    }

    /// Returns `true` if this `ChannelEnd` is in state [`State::Open`].
    pub fn is_open(&self) -> bool {
        self.state_matches(&State::Open)
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    pub fn ordering(&self) -> &Order {
        &self.ordering
    }

    pub fn counterparty(&self) -> &Counterparty {
        &self.remote
    }

    pub fn connection_hops(&self) -> &Vec<ConnectionId> {
        &self.connection_hops
    }

    pub fn version(&self) -> String {
        self.version.parse().unwrap()
    }

    pub fn validate_basic(&self) -> Result<(), Error> {
        if self.connection_hops.len() != 1 {
            return Err(Error::invalid_connection_hops_length(
                1,
                self.connection_hops.len(),
            ));
        }
        if self.version().trim() == "" {
            return Err(Error::empty_version());
        }
        self.counterparty().validate_basic()
    }

    /// Helper function to compare the state of this end with another state.
    pub fn state_matches(&self, other: &State) -> bool {
        self.state.eq(other)
    }

    /// Helper function to compare the order of this end with another order.
    pub fn order_matches(&self, other: &Order) -> bool {
        self.ordering.eq(other)
    }

    #[allow(clippy::ptr_arg)]
    pub fn connection_hops_matches(&self, other: &Vec<ConnectionId>) -> bool {
        self.connection_hops.eq(other)
    }

    pub fn counterparty_matches(&self, other: &Counterparty) -> bool {
        self.counterparty().eq(other)
    }

    pub fn version_matches(&self, other: &str) -> bool {
        self.version().eq(other)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counterparty {
    pub port_id: PortId,
    pub channel_id: Option<ChannelId>,
}

impl Default for Counterparty {
    fn default() -> Self {
        Counterparty {
            port_id: Default::default(),
            channel_id: None,
        }
    }
}

impl Counterparty {
    pub fn new(port_id: PortId, channel_id: Option<ChannelId>) -> Self {
        Self {
            port_id,
            channel_id,
        }
    }

    pub fn port_id(&self) -> &PortId {
        &self.port_id
    }

    pub fn channel_id(&self) -> Option<&ChannelId> {
        self.channel_id.as_ref()
    }

    pub fn validate_basic(&self) -> Result<(), Error> {
        Ok(())
    }
}

impl Protobuf<RawCounterparty> for Counterparty {}

impl TryFrom<RawCounterparty> for Counterparty {
    type Error = Error;

    fn try_from(value: RawCounterparty) -> Result<Self, Self::Error> {
        let channel_id = Some(value.channel_id)
            .filter(|x| !x.is_empty())
            .map(|v| FromStr::from_str(v.as_str()))
            .transpose()
            .map_err(Error::identifier)?;
        Ok(Counterparty::new(
            value.port_id.parse().map_err(Error::identifier)?,
            channel_id,
        ))
    }
}

impl From<Counterparty> for RawCounterparty {
    fn from(value: Counterparty) -> Self {
        RawCounterparty {
            port_id: value.port_id.as_str().to_string(),
            channel_id: value
                .channel_id
                .map_or_else(|| "".to_string(), |v| v.as_str().to_string()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum Order {
    None = 0,
    Unordered,
    Ordered,
}

impl Default for Order {
    fn default() -> Self {
        Order::Unordered
    }
}

impl fmt::Display for Order {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Order {
    /// Yields the Order as a string
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "UNINITIALIZED",
            Self::Unordered => "ORDER_UNORDERED",
            Self::Ordered => "ORDER_ORDERED",
        }
    }

    // Parses the Order out from a i32.
    pub fn from_i32(nr: i32) -> Result<Self, Error> {
        match nr {
            0 => Ok(Self::None),
            1 => Ok(Self::Unordered),
            2 => Ok(Self::Ordered),
            _ => Err(Error::unknown_order_type(nr.to_string())),
        }
    }
}

impl FromStr for Order {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "uninitialized" => Ok(Self::None),
            "unordered" => Ok(Self::Unordered),
            "ordered" => Ok(Self::Ordered),
            _ => Err(Error::unknown_order_type(s.to_string())),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum State {
    Uninitialized = 0,
    Init = 1,
    TryOpen = 2,
    Open = 3,
    Closed = 4,
}

impl State {
    /// Yields the state as a string
    pub fn as_string(&self) -> &'static str {
        match self {
            Self::Uninitialized => "UNINITIALIZED",
            Self::Init => "INIT",
            Self::TryOpen => "TRYOPEN",
            Self::Open => "OPEN",
            Self::Closed => "CLOSED",
        }
    }

    // Parses the State out from a i32.
    pub fn from_i32(s: i32) -> Result<Self, Error> {
        match s {
            0 => Ok(Self::Uninitialized),
            1 => Ok(Self::Init),
            2 => Ok(Self::TryOpen),
            3 => Ok(Self::Open),
            4 => Ok(Self::Closed),
            _ => Err(Error::unknown_state(s)),
        }
    }

    /// Returns whether or not this channel state is `Open`.
    pub fn is_open(self) -> bool {
        self == State::Open
    }

    /// Returns whether or not the channel with this state
    /// has progressed less or the same than the argument.
    ///
    /// # Example
    /// ```rust,ignore
    /// assert!(State::Init.less_or_equal_progress(State::Open));
    /// assert!(State::TryOpen.less_or_equal_progress(State::TryOpen));
    /// assert!(!State::Closed.less_or_equal_progress(State::Open));
    /// ```
    pub fn less_or_equal_progress(self, other: Self) -> bool {
        self as u32 <= other as u32
    }
}

/// Provides a `to_string` method.
impl core::fmt::Display for State {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> Result<(), core::fmt::Error> {
        write!(f, "{}", self.as_string())
    }
}

/// Used to query a packet event, identified by `event_id`, for specific channel and sequences.
/// The query is preformed for the chain context at `height`.
#[derive(Clone, Debug)]
pub struct QueryPacketEventDataRequest {
    pub event_id: IbcEventType,
    pub source_channel_id: ChannelId,
    pub source_port_id: PortId,
    pub destination_channel_id: ChannelId,
    pub destination_port_id: PortId,
    pub sequences: Vec<Sequence>,
    pub height: Height,
}

/// Version validation, specific for channel (ICS4) opening handshake protocol.
/// This field is supposed to be opaque to the core IBC protocol. No explicit validation necessary,
/// and empty version is currently allowed by the specification (cf. ICS 004, v1).
pub fn validate_version(version: String) -> Result<String, Error> {
    Ok(version)
}

#[cfg(test)]
pub mod test_util {
    use crate::ics24_host::identifier::{ChannelId, ConnectionId, PortId};
    use crate::prelude::*;
    use ibc_proto::ibc::core::channel::v1::Channel as RawChannel;
    use ibc_proto::ibc::core::channel::v1::Counterparty as RawCounterparty;

    /// Returns a dummy `RawCounterparty`, for testing only!
    /// Can be optionally parametrized with a specific channel identifier.
    pub fn get_dummy_raw_counterparty() -> RawCounterparty {
        RawCounterparty {
            port_id: PortId::default().to_string(),
            channel_id: ChannelId::default().to_string(),
        }
    }

    /// Returns a dummy `RawChannel`, for testing only!
    pub fn get_dummy_raw_channel_end() -> RawChannel {
        RawChannel {
            state: 1,
            ordering: 1,
            counterparty: Some(get_dummy_raw_counterparty()),
            connection_hops: vec![ConnectionId::default().to_string()],
            version: "ics20".to_string(), // The version is not validated.
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::prelude::*;
    use core::convert::TryFrom;
    use core::str::FromStr;
    use test_env_log::test;

    use ibc_proto::ibc::core::channel::v1::Channel as RawChannel;

    use crate::ics04_channel::channel::test_util::get_dummy_raw_channel_end;
    use crate::ics04_channel::channel::ChannelEnd;

    #[test]
    fn channel_end_try_from_raw() {
        let raw_channel_end = get_dummy_raw_channel_end();

        let empty_raw_channel_end = RawChannel {
            counterparty: None,
            ..raw_channel_end.clone()
        };

        struct Test {
            name: String,
            params: RawChannel,
            want_pass: bool,
        }

        let tests: Vec<Test> = vec![
            Test {
                name: "Raw channel end with missing counterparty".to_string(),
                params: empty_raw_channel_end,
                want_pass: false,
            },
            Test {
                name: "Raw channel end with incorrect state".to_string(),
                params: RawChannel {
                    state: -1,
                    ..raw_channel_end.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Raw channel end with incorrect ordering".to_string(),
                params: RawChannel {
                    ordering: -1,
                    ..raw_channel_end.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Raw channel end with incorrect connection id in connection hops".to_string(),
                params: RawChannel {
                    connection_hops: vec!["connection*".to_string()].into_iter().collect(),
                    ..raw_channel_end.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Raw channel end with incorrect connection id (has blank space)".to_string(),
                params: RawChannel {
                    connection_hops: vec!["con nection".to_string()].into_iter().collect(),
                    ..raw_channel_end.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Raw channel end with two correct connection ids in connection hops"
                    .to_string(),
                params: RawChannel {
                    connection_hops: vec!["connection1".to_string(), "connection2".to_string()]
                        .into_iter()
                        .collect(),
                    ..raw_channel_end.clone()
                },
                want_pass: true,
            },
            Test {
                name: "Raw channel end with correct params".to_string(),
                params: raw_channel_end,
                want_pass: true,
            },
        ]
        .into_iter()
        .collect();

        for test in tests {
            let p = test.params.clone();

            let ce_result = ChannelEnd::try_from(p);

            assert_eq!(
                test.want_pass,
                ce_result.is_ok(),
                "ChannelEnd::try_from() failed for test {}, \nmsg{:?} with error {:?}",
                test.name,
                test.params.clone(),
                ce_result.err(),
            );
        }
    }

    #[test]
    fn parse_channel_ordering_type() {
        use super::Order;

        struct Test {
            ordering: &'static str,
            want_res: Order,
            want_err: bool,
        }
        let tests: Vec<Test> = vec![
            Test {
                ordering: "UNINITIALIZED",
                want_res: Order::None,
                want_err: false,
            },
            Test {
                ordering: "UNORDERED",
                want_res: Order::Unordered,
                want_err: false,
            },
            Test {
                ordering: "ORDERED",
                want_res: Order::Ordered,
                want_err: false,
            },
            Test {
                ordering: "UNKNOWN_ORDER",
                want_res: Order::None,
                want_err: true,
            },
        ]
        .into_iter()
        .collect();

        for test in tests {
            match Order::from_str(test.ordering) {
                Ok(res) => {
                    assert!(!test.want_err);
                    assert_eq!(test.want_res, res);
                }
                Err(_) => assert!(test.want_err, "parse failed"),
            }
        }
    }
}
