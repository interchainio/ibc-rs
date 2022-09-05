/// Models for serializing and deserializing IBC path JSON data found in the `_IBC/` directory of the registry repository
use crate::fetchable::Fetchable;
use ibc::core::ics24_host::identifier::{ChannelId, ClientId, ConnectionId, PortId};
use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct IBCPath {
    #[serde(rename = "$schema")]
    pub schema: String,
    pub chain_1: Chain1,
    pub chain_2: Chain2,
    pub channels: Vec<Channel>,
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Chain1 {
    pub chain_name: String,
    pub client_id: ClientId,
    pub connection_id: ConnectionId,
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Chain2 {
    pub chain_name: String,
    pub client_id: ClientId,
    pub connection_id: ConnectionId,
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Channel {
    pub chain_1: ChannelChain1,
    pub chain_2: ChannelChain2,
    pub ordering: String,
    pub version: String,
    pub tags: Tags,
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChannelChain1 {
    pub channel_id: ChannelId,
    pub port_id: PortId,
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChannelChain2 {
    pub channel_id: ChannelId,
    pub port_id: PortId,
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tags {
    pub dex: String,
    pub preferred: bool,
    pub properties: String,
    pub status: String,
}

/// Represents an IBC path tag
pub enum Tag {
    Dex(String),
    Preferred(bool),
    Properties(String),
    Status(String),
}

impl Fetchable for IBCPath {
    fn path(resource: &str) -> PathBuf {
        ["_IBC", resource].iter().collect()
    }
}

#[allow(clippy::bool_assert_comparison)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::RegistryError;

    use crate::constants::ALL_PATHS;

    #[tokio::test]
    #[ignore]
    async fn fetch_paths() -> Result<(), RegistryError> {
        let mut handles = Vec::with_capacity(ALL_PATHS.len());

        for resource in ALL_PATHS {
            handles.push(tokio::spawn(IBCPath::fetch(resource.to_string(), None)));
        }

        for handle in handles {
            let path: IBCPath = handle.await.unwrap()?;
            assert!(!path.channels.is_empty());
        }

        Ok(())
    }

    #[test]
    fn paths_path() {
        let path = IBCPath::path("test");
        assert_eq!(path, PathBuf::from("_IBC/test"));
    }

    #[test]
    fn paths_deserialize() {
        use ibc::core::ics24_host::identifier::{ChannelId, ClientId, ConnectionId, PortId};
        use std::str::FromStr;

        let path = r#"{
            "$schema": "https://github.com/cosmos/chain-registry/blob/master/ibc_data.schema.json",
            "chain_1": {
                "chain_name": "chain_1",
                "client_id": "tendermint_1",
                "connection_id": "connection_1"
            },
            "chain_2": {
                "chain_name": "chain_2",
                "client_id": "tendermint_2",
                "connection_id": "connection_2"
            },
            "channels": [
                {
                    "chain_1": {
                        "channel_id": "channel_1",
                        "port_id": "port_1"
                    },
                    "chain_2": {
                        "channel_id": "channel_2",
                        "port_id": "port_2"
                    },
                    "ordering": "ordering",
                    "version": "version",
                    "tags": {
                        "dex": "dex",
                        "preferred": true,
                        "properties": "properties",
                        "status": "status"
                    }
                }
            ]
        }"#;
        let path: IBCPath = serde_json::from_str(path).unwrap();
        assert_eq!(path.schema, "https://github.com/cosmos/chain-registry/blob/master/ibc_data.schema.json");
        assert_eq!(path.chain_1.chain_name, "chain_1");
        assert_eq!(
            path.chain_1.client_id,
            ClientId::from_str("tendermint_1").unwrap()
        );
        assert_eq!(
            path.chain_1.connection_id,
            ConnectionId::from_str("connection_1").unwrap()
        );
        assert_eq!(path.chain_2.chain_name, "chain_2");
        assert_eq!(
            path.chain_2.client_id,
            ClientId::from_str("tendermint_2").unwrap()
        );
        assert_eq!(
            path.chain_2.connection_id,
            ConnectionId::from_str("connection_2").unwrap()
        );
        assert_eq!(path.channels.len(), 1);
        assert_eq!(
            path.channels[0].chain_1.channel_id,
            ChannelId::from_str("channel_1").unwrap()
        );
        assert_eq!(
            path.channels[0].chain_1.port_id,
            PortId::from_str("port_1").unwrap()
        );
        assert_eq!(
            path.channels[0].chain_2.channel_id,
            ChannelId::from_str("channel_2").unwrap()
        );
        assert_eq!(
            path.channels[0].chain_2.port_id,
            PortId::from_str("port_2").unwrap()
        );
        assert_eq!(path.channels[0].ordering, "ordering");
        assert_eq!(path.channels[0].version, "version");
        assert_eq!(path.channels[0].tags.dex, "dex");
        assert_eq!(path.channels[0].tags.preferred, true);
        assert_eq!(path.channels[0].tags.properties, "properties");
        assert_eq!(path.channels[0].tags.status, "status");
    }
}
