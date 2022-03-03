//! Relayer configuration

mod error;
mod filter_pattern;
mod proof_specs;
pub mod reload;
pub mod types;

use alloc::collections::BTreeMap;
use core::{fmt, time::Duration};
use itertools::Itertools;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use std::{fs, fs::File, io::Write, path::Path};

use serde::{de, ser, Deserializer, Serializer};
use serde_derive::{Deserialize, Serialize};
use tendermint_light_client_verifier::types::TrustThreshold;

use ibc::core::ics23_commitment::specs::ProofSpecs;
use ibc::core::ics24_host::identifier::{ChainId, ChannelId, PortId};
use ibc::timestamp::ZERO_DURATION;

use crate::config::types::{MaxMsgNum, MaxTxSize, Memo};
use crate::keyring::Store;

pub use error::Error;
use filter_pattern::{channel::ChannelFilterMatchVisitor, port::PortFilterMatchVisitor};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GasPrice {
    pub price: f64,
    pub denom: String,
}

impl GasPrice {
    pub const fn new(price: f64, denom: String) -> Self {
        Self { price, denom }
    }
}

impl fmt::Display for GasPrice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.price, self.denom)
    }
}

/// Represents the ways in which packets can be filtered.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    rename_all = "lowercase",
    tag = "policy",
    content = "list",
    deny_unknown_fields
)]
pub enum PacketFilter {
    /// Allow packets from the specified channels.
    Allow(ChannelFilters),
    /// Deny packets from the specified channels.
    Deny(ChannelFilters),
    /// Allow any & all packets.
    AllowAll,
}

impl Default for PacketFilter {
    /// By default, allows all channels & ports.
    fn default() -> Self {
        Self::AllowAll
    }
}

impl PacketFilter {
    /// Returns true if the packets can be relayed on the channel with [`PortId`] and [`ChannelId`],
    /// false otherwise.
    pub fn is_allowed(&self, port_id: &PortId, channel_id: &ChannelId) -> bool {
        match self {
            PacketFilter::Allow(spec) => spec.matches(&(port_id.clone(), channel_id.clone())),
            PacketFilter::Deny(spec) => !spec.matches(&(port_id.clone(), channel_id.clone())),
            PacketFilter::AllowAll => true,
        }
    }
}

/// The internal representation of channel filter policies.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelFilters(Vec<(PortFilterMatch, ChannelFilterMatch)>);

impl ChannelFilters {
    /// Indicates whether a match for the given [`PortId`]-[`ChannelId`] pair
    /// exists in the filter policy.
    pub fn matches(&self, channel_port: &(PortId, ChannelId)) -> bool {
        let (port_id, channel_id) = channel_port;
        self.0.iter().any(|(port_filter, chan_filter)| {
            port_filter.matches(port_id) && chan_filter.matches(channel_id)
        })
    }

    /// Indicates whether this filter policy contains only exact patterns.
    #[inline]
    pub fn is_exact(&self) -> bool {
        self.0.iter().all(|(port_filter, channel_filter)| {
            port_filter.is_exact() && channel_filter.is_exact()
        })
    }

    /// An iterator over the [`PortId`]-[`ChannelId`] pairs that don't contain wildcards.
    pub fn iter_exact(&self) -> impl Iterator<Item = (&PortId, &ChannelId)> {
        self.0.iter().filter_map(|port_chan_filter| {
            if let &(FilterPattern::Exact(ref port_id), FilterPattern::Exact(ref chan_id)) =
                port_chan_filter
            {
                Some((port_id, chan_id))
            } else {
                None
            }
        })
    }
}

impl fmt::Display for ChannelFilters {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            self.0
                .iter()
                .map(|(pid, cid)| format!("{}/{}", pid, cid))
                .join(", ")
        )
    }
}

impl ser::Serialize for ChannelFilters {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeSeq;

        struct Pair<'a> {
            a: &'a FilterPattern<PortId>,
            b: &'a FilterPattern<ChannelId>,
        }

        impl<'a> ser::Serialize for Pair<'a> {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                let mut seq = serializer.serialize_seq(Some(2))?;
                seq.serialize_element(self.a)?;
                seq.serialize_element(self.b)?;
                seq.end()
            }
        }

        let mut outer_seq = serializer.serialize_seq(Some(self.0.len()))?;

        for (port, channel) in &self.0 {
            outer_seq.serialize_element(&Pair {
                a: port,
                b: channel,
            })?;
        }

        outer_seq.end()
    }
}

/// Newtype wrapper for expressing wildcard patterns compiled to a [`regex::Regex`].
#[derive(Clone, Debug)]
pub struct Wildcard(regex::Regex);

impl Wildcard {
    #[inline]
    pub fn is_match(&self, text: &str) -> bool {
        self.0.is_match(text)
    }
}

impl FromStr for Wildcard {
    type Err = regex::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let regex = regex::escape(s).replace("\\*", "(?:.*)").parse()?;
        Ok(Self(regex))
    }
}

impl fmt::Display for Wildcard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = self.0.to_string().replace("(?:.*)", "*");
        write!(f, "{}", s)
    }
}

impl ser::Serialize for Wildcard {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

/// Represents a single channel to be filtered in a [`ChannelFilters`] list.
#[derive(Clone, Debug)]
pub enum FilterPattern<T> {
    /// A channel specified exactly with its [`PortId`] & [`ChannelId`].
    Exact(T),
    /// A glob of channel(s) specified with a wildcard in either or both [`PortId`] & [`ChannelId`].
    Wildcard(Wildcard),
}

impl<T> FilterPattern<T> {
    /// Indicates whether this filter is specified in part with a wildcard.
    pub fn is_wildcard(&self) -> bool {
        matches!(self, Self::Wildcard(_))
    }

    /// Indicates whether this filter is specified as an exact match.
    pub fn is_exact(&self) -> bool {
        matches!(self, Self::Exact(_))
    }

    /// Matches the given value via strict equality if the filter is an `Exact`, or via
    /// wildcard matching if the filter is a `Pattern`.
    pub fn matches(&self, value: &T) -> bool
    where
        T: PartialEq + AsRef<str>,
    {
        match self {
            FilterPattern::Exact(v) => value == v,
            FilterPattern::Wildcard(regex) => regex.is_match(value.as_ref()),
        }
    }

    /// Returns the contained value if this filter contains an `Exact` variant, or
    /// `None` if it contains a `Pattern`.
    pub fn exact_value(&self) -> Option<&T> {
        match self {
            FilterPattern::Exact(value) => Some(value),
            FilterPattern::Wildcard(_) => None,
        }
    }
}

impl<T: fmt::Display> fmt::Display for FilterPattern<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FilterPattern::Exact(value) => write!(f, "{}", value),
            FilterPattern::Wildcard(regex) => write!(f, "{}", regex),
        }
    }
}

impl<T> ser::Serialize for FilterPattern<T>
where
    T: AsRef<str>,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            FilterPattern::Exact(e) => serializer.serialize_str(e.as_ref()),
            FilterPattern::Wildcard(t) => serializer.serialize_str(&t.to_string()),
        }
    }
}

/// Type alias for a [`FilterPattern`] containing a [`PortId`].
pub type PortFilterMatch = FilterPattern<PortId>;
/// Type alias for a [`FilterMatch`] containing a [`ChannelId`].
pub type ChannelFilterMatch = FilterPattern<ChannelId>;

impl<'de> de::Deserialize<'de> for PortFilterMatch {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<PortFilterMatch, D::Error> {
        deserializer.deserialize_string(PortFilterMatchVisitor)
    }
}

impl<'de> de::Deserialize<'de> for ChannelFilterMatch {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<ChannelFilterMatch, D::Error> {
        deserializer.deserialize_string(ChannelFilterMatchVisitor)
    }
}

/// Defaults for various fields
pub mod default {
    use super::*;

    pub fn tx_confirmation() -> bool {
        true
    }

    pub fn clear_packets_interval() -> u64 {
        100
    }

    pub fn rpc_timeout() -> Duration {
        Duration::from_secs(10)
    }

    pub fn clock_drift() -> Duration {
        Duration::from_secs(5)
    }

    pub fn max_block_time() -> Duration {
        Duration::from_secs(10)
    }

    pub fn connection_delay() -> Duration {
        ZERO_DURATION
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub global: GlobalConfig,
    #[serde(default)]
    pub mode: ModeConfig,
    #[serde(default)]
    pub rest: RestConfig,
    #[serde(default)]
    pub telemetry: TelemetryConfig,
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub chains: Vec<ChainConfig>,
}

pub type SharedConfig = Arc<RwLock<Config>>;

impl Config {
    pub fn has_chain(&self, id: &ChainId) -> bool {
        self.chains.iter().any(|c| c.id == *id)
    }

    pub fn find_chain(&self, id: &ChainId) -> Option<&ChainConfig> {
        self.chains.iter().find(|c| c.id == *id)
    }

    pub fn find_chain_mut(&mut self, id: &ChainId) -> Option<&mut ChainConfig> {
        self.chains.iter_mut().find(|c| c.id == *id)
    }

    /// Returns true if filtering is disabled or if packets are allowed on
    /// the channel [`PortId`] [`ChannelId`] on [`ChainId`].
    /// Returns false otherwise.
    pub fn packets_on_channel_allowed(
        &self,
        chain_id: &ChainId,
        port_id: &PortId,
        channel_id: &ChannelId,
    ) -> bool {
        match self.find_chain(chain_id) {
            Some(chain_config) => chain_config.packet_filter.is_allowed(port_id, channel_id),
            None => false,
        }
    }

    pub fn chains_map(&self) -> BTreeMap<&ChainId, &ChainConfig> {
        self.chains.iter().map(|c| (&c.id, c)).collect()
    }
}

#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModeConfig {
    pub clients: Clients,
    pub connections: Connections,
    pub channels: Channels,
    pub packets: Packets,
}

impl ModeConfig {
    pub fn all_disabled(&self) -> bool {
        !self.clients.enabled
            && !self.connections.enabled
            && !self.channels.enabled
            && !self.packets.enabled
    }
}

impl Default for ModeConfig {
    fn default() -> Self {
        Self {
            clients: Clients {
                enabled: true,
                refresh: true,
                misbehaviour: true,
            },
            connections: Connections { enabled: false },
            channels: Channels { enabled: false },
            packets: Packets {
                enabled: true,
                clear_interval: default::clear_packets_interval(),
                clear_on_start: true,
                tx_confirmation: true,
            },
        }
    }
}

#[derive(Copy, Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Clients {
    pub enabled: bool,
    #[serde(default)]
    pub refresh: bool,
    #[serde(default)]
    pub misbehaviour: bool,
}

#[derive(Copy, Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Connections {
    pub enabled: bool,
}

#[derive(Copy, Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Channels {
    pub enabled: bool,
}

#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Packets {
    pub enabled: bool,
    #[serde(default = "default::clear_packets_interval")]
    pub clear_interval: u64,
    #[serde(default)]
    pub clear_on_start: bool,
    #[serde(default = "default::tx_confirmation")]
    pub tx_confirmation: bool,
}

impl Default for Packets {
    fn default() -> Self {
        Self {
            enabled: false,
            clear_interval: default::clear_packets_interval(),
            clear_on_start: false,
            tx_confirmation: default::tx_confirmation(),
        }
    }
}

/// Log levels are wrappers over [`tracing_core::Level`].
///
/// [`tracing_core::Level`]: https://docs.rs/tracing-core/0.1.17/tracing_core/struct.Level.html
#[derive(Copy, Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl Default for LogLevel {
    fn default() -> Self {
        Self::Info
    }
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LogLevel::Trace => write!(f, "trace"),
            LogLevel::Debug => write!(f, "debug"),
            LogLevel::Info => write!(f, "info"),
            LogLevel::Warn => write!(f, "warn"),
            LogLevel::Error => write!(f, "error"),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct GlobalConfig {
    pub log_level: LogLevel,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TelemetryConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            host: "127.0.0.1".to_string(),
            port: 3001,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RestConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
}

impl Default for RestConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            host: "127.0.0.1".to_string(),
            port: 3000,
        }
    }
}

/// It defines the address generation method
/// TODO: Ethermint `pk_type` to be restricted
/// after the Cosmos SDK release with ethsecp256k1
/// <https://github.com/cosmos/cosmos-sdk/pull/9981>
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(
    rename_all = "lowercase",
    tag = "derivation",
    content = "proto_type",
    deny_unknown_fields
)]
pub enum AddressType {
    Cosmos,
    Ethermint { pk_type: String },
}

impl Default for AddressType {
    fn default() -> Self {
        AddressType::Cosmos
    }
}

impl fmt::Display for AddressType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AddressType::Cosmos => write!(f, "cosmos"),
            AddressType::Ethermint { .. } => write!(f, "ethermint"),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChainConfig {
    pub id: ChainId,
    pub rpc_addr: tendermint_rpc::Url,
    pub websocket_addr: tendermint_rpc::Url,
    pub grpc_addr: tendermint_rpc::Url,
    #[serde(default = "default::rpc_timeout", with = "humantime_serde")]
    pub rpc_timeout: Duration,
    pub account_prefix: String,
    pub key_name: String,
    #[serde(default)]
    pub key_store_type: Store,
    pub store_prefix: String,
    pub default_gas: Option<u64>,
    pub max_gas: Option<u64>,
    pub gas_adjustment: Option<f64>,
    pub fee_granter: Option<String>,
    #[serde(default)]
    pub max_msg_num: MaxMsgNum,
    #[serde(default)]
    pub max_tx_size: MaxTxSize,

    /// A correction parameter that helps deal with clocks that are only approximately synchronized
    /// between the source and destination chains for a client.
    /// This parameter is used when deciding to accept or reject a new header
    /// (originating from the source chain) for any client with the destination chain
    /// that uses this configuration, unless it is overridden by the client-specific
    /// clock drift option.
    #[serde(default = "default::clock_drift", with = "humantime_serde")]
    pub clock_drift: Duration,

    #[serde(default = "default::max_block_time", with = "humantime_serde")]
    pub max_block_time: Duration,

    /// The trusting period specifies how long a validator set is trusted for
    /// (must be shorter than the chain's unbonding period).
    #[serde(default, with = "humantime_serde")]
    pub trusting_period: Option<Duration>,

    #[serde(default)]
    pub memo_prefix: Memo,
    #[serde(default, with = "self::proof_specs")]
    pub proof_specs: ProofSpecs,

    // these two need to be last otherwise we run into `ValueAfterTable` error when serializing to TOML
    /// The trust threshold defines what fraction of the total voting power of a known
    /// and trusted validator set is sufficient for a commit to be accepted going forward.
    #[serde(default)]
    pub trust_threshold: TrustThreshold,
    pub gas_price: GasPrice,
    #[serde(default)]
    pub packet_filter: PacketFilter,
    #[serde(default)]
    pub address_type: AddressType,
}

/// Attempt to load and parse the TOML config file as a `Config`.
pub fn load(path: impl AsRef<Path>) -> Result<Config, Error> {
    let config_toml = std::fs::read_to_string(&path).map_err(Error::io)?;

    let config = toml::from_str::<Config>(&config_toml[..]).map_err(Error::decode)?;

    Ok(config)
}

/// Serialize the given `Config` as TOML to the given config file.
pub fn store(config: &Config, path: impl AsRef<Path>) -> Result<(), Error> {
    let mut file = if path.as_ref().exists() {
        fs::OpenOptions::new().write(true).truncate(true).open(path)
    } else {
        File::create(path)
    }
    .map_err(Error::io)?;

    store_writer(config, &mut file)
}

/// Serialize the given `Config` as TOML to the given writer.
pub(crate) fn store_writer(config: &Config, mut writer: impl Write) -> Result<(), Error> {
    let toml_config = toml::to_string_pretty(&config).map_err(Error::encode)?;

    writeln!(writer, "{}", toml_config).map_err(Error::io)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{load, store_writer, ChannelFilters, FilterPattern, PacketFilter};
    use test_log::test;

    #[test]
    fn parse_valid_config() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/config/fixtures/relayer_conf_example.toml"
        );

        let config = load(path).expect("could not parse config");

        dbg!(config);
    }

    #[test]
    fn serialize_valid_config() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/config/fixtures/relayer_conf_example.toml"
        );

        let config = load(path).expect("could not parse config");

        let mut buffer = Vec::new();
        store_writer(&config, &mut buffer).unwrap();
    }

    #[test]
    fn deserialize_packet_filter_policy() {
        let toml_content = r#"
            policy = 'allow'
            list = [
              ['ica*', '*'],
              ['transfer', 'channel-0'],
            ]
            "#;

        let filter_policy: PacketFilter =
            toml::from_str(toml_content).expect("could not parse filter policy");

        dbg!(filter_policy);
    }

    #[test]
    fn serialize_packet_filter_policy() {
        use std::str::FromStr;

        use ibc::core::ics24_host::identifier::{ChannelId, PortId};

        let filter_policy = ChannelFilters(vec![
            (
                FilterPattern::Exact(PortId::from_str("transfer").unwrap()),
                FilterPattern::Exact(ChannelId::from_str("channel-0").unwrap()),
            ),
            (
                FilterPattern::Wildcard("ica*".parse().unwrap()),
                FilterPattern::Wildcard("*".parse().unwrap()),
            ),
        ]);

        let fp = PacketFilter::Allow(filter_policy);
        let toml_str = toml::to_string_pretty(&fp).expect("could not serialize packet filter");

        println!("{}", toml_str);
    }
}
