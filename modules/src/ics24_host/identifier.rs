use crate::prelude::*;
use core::convert::TryFrom;
use core::fmt::{self, Display, Formatter};
use core::str::FromStr;
use serde::{Deserialize, Serialize};

use crate::ics02_client::client_type::ClientType;
use crate::ics24_host::error::ValidationError;

use super::validate::*;

/// This type is subject to future changes.
///
/// TODO: ChainId validation is not standardized yet.
///       `is_epoch_format` will most likely be replaced by validate_chain_id()-style function.
///       See: <https://github.com/informalsystems/ibc-rs/pull/304#discussion_r503917283>.
///
/// Also, contrast with tendermint-rs `ChainId` type.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(from = "tendermint::chain::Id", into = "tendermint::chain::Id")]
pub struct ChainId {
    id: String,
    version: u64,
}

impl ChainId {
    /// Creates a new `ChainId` given a chain name and an epoch number.
    ///
    /// The returned `ChainId` will have the format: `{chain name}-{epoch number}`.
    /// ```
    /// use ibc::ics24_host::identifier::ChainId;
    ///
    /// let epoch_number = 10;
    /// let id = ChainId::new("chainA".to_string(), epoch_number);
    /// assert_eq!(id.version(), epoch_number);
    /// ```
    pub fn new(name: String, version: u64) -> Self {
        Self {
            id: format!("{}-{}", name, version),
            version,
        }
    }

    /// Get a reference to the underlying string.
    pub fn as_str(&self) -> &str {
        &self.id
    }

    // TODO: this should probably be named epoch_number.
    /// Extract the version from this chain identifier.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Extract the version from the given chain identifier.
    /// ```
    /// use ibc::ics24_host::identifier::ChainId;
    ///
    /// assert_eq!(ChainId::chain_version("chain--a-0"), 0);
    /// assert_eq!(ChainId::chain_version("ibc-10"), 10);
    /// assert_eq!(ChainId::chain_version("cosmos-hub-97"), 97);
    /// assert_eq!(ChainId::chain_version("testnet-helloworld-2"), 2);
    /// ```
    pub fn chain_version(chain_id: &str) -> u64 {
        if !ChainId::is_epoch_format(chain_id) {
            return 0;
        }

        let split: Vec<_> = chain_id.split('-').collect();
        split
            .last()
            .expect("get revision number from chain_id")
            .parse()
            .unwrap_or(0)
    }

    /// is_epoch_format() checks if a chain_id is in the format required for parsing epochs
    /// The chainID must be in the form: `{chainID}-{version}`
    /// ```
    /// use ibc::ics24_host::identifier::ChainId;
    /// assert_eq!(ChainId::is_epoch_format("chainA-0"), false);
    /// assert_eq!(ChainId::is_epoch_format("chainA"), false);
    /// assert_eq!(ChainId::is_epoch_format("chainA-1"), true);
    /// ```
    pub fn is_epoch_format(chain_id: &str) -> bool {
        let re = regex::Regex::new(r"^.+[^-]-{1}[1-9][0-9]*$").unwrap();
        re.is_match(chain_id)
    }
}

impl FromStr for ChainId {
    type Err = ValidationError;

    fn from_str(id: &str) -> Result<Self, Self::Err> {
        let version = if Self::is_epoch_format(id) {
            Self::chain_version(id)
        } else {
            0
        };

        Ok(Self {
            id: id.to_string(),
            version,
        })
    }
}

impl Display for ChainId {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", self.id)
    }
}

impl From<ChainId> for tendermint::chain::Id {
    fn from(id: ChainId) -> Self {
        tendermint::chain::Id::from_str(id.as_str()).unwrap()
    }
}

impl From<tendermint::chain::Id> for ChainId {
    fn from(id: tendermint::chain::Id) -> Self {
        ChainId::from_str(id.as_str()).unwrap()
    }
}

impl Default for ChainId {
    fn default() -> Self {
        "defaultChainId".to_string().parse().unwrap()
    }
}

impl TryFrom<String> for ChainId {
    type Error = ValidationError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_str(value.as_str())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ClientId(String);

impl ClientId {
    /// Builds a new client identifier. Client identifiers are deterministically formed from two
    /// elements: a prefix derived from the client type `ctype`, and a monotonically increasing
    /// `counter`; these are separated by a dash "-".
    ///
    /// ```
    /// # use ibc::ics24_host::identifier::ClientId;
    /// # use ibc::ics02_client::client_type::ClientType;
    /// let tm_client_id = ClientId::new(ClientType::Tendermint, 0);
    /// assert!(tm_client_id.is_ok());
    /// tm_client_id.map(|id| { assert_eq!(&id, "07-tendermint-0") });
    /// ```
    pub fn new(ctype: ClientType, counter: u64) -> Result<Self, ValidationError> {
        let prefix = Self::prefix(ctype);
        let id = format!("{}-{}", prefix, counter);
        Self::from_str(id.as_str())
    }

    /// Get this identifier as a borrowed `&str`
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns one of the prefixes that should be present in any client identifiers.
    /// The prefix is deterministic for a given chain type, hence all clients for a Tendermint-type
    /// chain, for example, will have the prefix '07-tendermint'.
    pub fn prefix(client_type: ClientType) -> &'static str {
        match client_type {
            ClientType::Tendermint => ClientType::Tendermint.as_str(),

            #[cfg(any(test, feature = "mocks"))]
            ClientType::Mock => ClientType::Mock.as_str(),
        }
    }

    /// Get this identifier as a borrowed byte slice
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

/// This implementation provides a `to_string` method.
impl Display for ClientId {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", self.0)
    }
}

impl FromStr for ClientId {
    type Err = ValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        validate_client_identifier(s).map(|_| Self(s.to_string()))
    }
}

impl Default for ClientId {
    fn default() -> Self {
        Self::new(ClientType::Tendermint, 0).unwrap()
    }
}

/// Equality check against string literal (satisfies &ClientId == &str).
/// ```
/// use core::str::FromStr;
/// use ibc::ics24_host::identifier::ClientId;
/// let client_id = ClientId::from_str("clientidtwo");
/// assert!(client_id.is_ok());
/// client_id.map(|id| {assert_eq!(&id, "clientidtwo")});
/// ```
impl PartialEq<str> for ClientId {
    fn eq(&self, other: &str) -> bool {
        self.as_str().eq(other)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ConnectionId(String);

impl ConnectionId {
    /// Builds a new connection identifier. Connection identifiers are deterministically formed from
    /// two elements: a prefix `prefix`, and a monotonically increasing `counter`; these are
    /// separated by a dash "-". The prefix is currently determined statically (see
    /// `ConnectionId::prefix()`) so this method accepts a single argument, the `counter`.
    ///
    /// ```
    /// # use ibc::ics24_host::identifier::ConnectionId;
    /// let conn_id = ConnectionId::new(11);
    /// assert_eq!(&conn_id, "connection-11");
    /// ```
    pub fn new(counter: u64) -> Self {
        let id = format!("{}-{}", Self::prefix(), counter);
        Self::from_str(id.as_str()).unwrap()
    }

    /// Returns the static prefix to be used across all connection identifiers.
    pub fn prefix() -> &'static str {
        "connection"
    }

    /// Get this identifier as a borrowed `&str`
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Get this identifier as a borrowed byte slice
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

/// This implementation provides a `to_string` method.
impl Display for ConnectionId {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", self.0)
    }
}

impl FromStr for ConnectionId {
    type Err = ValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        validate_connection_identifier(s).map(|_| Self(s.to_string()))
    }
}

impl Default for ConnectionId {
    fn default() -> Self {
        Self::new(0)
    }
}

/// Equality check against string literal (satisfies &ConnectionId == &str).
/// ```
/// use core::str::FromStr;
/// use ibc::ics24_host::identifier::ConnectionId;
/// let conn_id = ConnectionId::from_str("connectionId-0");
/// assert!(conn_id.is_ok());
/// conn_id.map(|id| {assert_eq!(&id, "connectionId-0")});
/// ```
impl PartialEq<str> for ConnectionId {
    fn eq(&self, other: &str) -> bool {
        self.as_str().eq(other)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PortId(String);

impl PortId {
    /// Get this identifier as a borrowed `&str`
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Get this identifier as a borrowed byte slice
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

/// This implementation provides a `to_string` method.
impl Display for PortId {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", self.0)
    }
}

impl FromStr for PortId {
    type Err = ValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        validate_port_identifier(s).map(|_| Self(s.to_string()))
    }
}

impl Default for PortId {
    fn default() -> Self {
        "defaultPort".to_string().parse().unwrap()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ChannelId(String);

impl ChannelId {
    /// Builds a new channel identifier. Like client and connection identifiers, channel ids are
    /// deterministically formed from two elements: a prefix `prefix`, and a monotonically
    /// increasing `counter`, separated by a dash "-".
    /// The prefix is currently determined statically (see `ChannelId::prefix()`) so this method
    /// accepts a single argument, the `counter`.
    ///
    /// ```
    /// # use ibc::ics24_host::identifier::ChannelId;
    /// let chan_id = ChannelId::new(27);
    /// assert_eq!(&chan_id, "channel-27");
    /// ```
    pub fn new(counter: u64) -> Self {
        let id = format!("{}-{}", Self::prefix(), counter);
        Self::from_str(id.as_str()).unwrap()
    }

    pub fn prefix() -> &'static str {
        "channel"
    }

    /// Get this identifier as a borrowed `&str`
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Get this identifier as a borrowed byte slice
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

/// This implementation provides a `to_string` method.
impl Display for ChannelId {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}", self.0)
    }
}

impl FromStr for ChannelId {
    type Err = ValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        validate_channel_identifier(s).map(|_| Self(s.to_string()))
    }
}

impl Default for ChannelId {
    fn default() -> Self {
        Self::new(0)
    }
}

/// Equality check against string literal (satisfies &ChannelId == &str).
impl PartialEq<str> for ChannelId {
    fn eq(&self, other: &str) -> bool {
        self.as_str().eq(other)
    }
}

/// A pair of [`PortId`] and [`ChannelId`] are used together for sending IBC packets.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PortChannelId {
    pub channel_id: ChannelId,
    pub port_id: PortId,
}

impl Display for PortChannelId {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{}/{}", self.port_id, self.channel_id)
    }
}
