use crate::prelude::*;

/// Path-space as listed in ICS-024
/// https://github.com/cosmos/ics/tree/master/spec/ics-024-host-requirements#path-space
/// Some of these are implemented in other ICSs, but ICS-024 has a nice summary table.
///
use core::fmt::{Display, Formatter, Result};

use crate::ics04_channel::packet::Sequence;
use crate::ics24_host::identifier::{ChannelId, ClientId, ConnectionId, PortId};

/// ABCI Query path for the IBC sub-store
pub const IBC_QUERY_PATH: &str = "store/ibc/key";

/// ABCI Query path for the upgrade sub-store
/// ## Note: This is SDK/Tendermint specific!
pub const SDK_UPGRADE_QUERY_PATH: &str = "store/upgrade/key";

/// ABCI client upgrade keys
/// - The key identifying the upgraded IBC state within the upgrade sub-store
const UPGRADED_IBC_STATE: &str = "upgradedIBCState";
///- The key identifying the upgraded client state
const UPGRADED_CLIENT_STATE: &str = "upgradedClient";
/// - The key identifying the upgraded consensus state
const UPGRADED_CLIENT_CONSENSUS_STATE: &str = "upgradedConsState";

/// The Path enum abstracts out the different sub-paths
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Path {
    ClientType(ClientId),
    ClientState(ClientId),
    ClientConsensusState {
        client_id: ClientId,
        epoch: u64,
        height: u64,
    },
    ClientConnections(ClientId),
    Connections(ConnectionId),
    Ports(PortId),
    ChannelEnds(PortId, ChannelId),
    SeqSends(PortId, ChannelId),
    SeqRecvs(PortId, ChannelId),
    SeqAcks(PortId, ChannelId),
    Commitments {
        port_id: PortId,
        channel_id: ChannelId,
        sequence: Sequence,
    },
    Acks {
        port_id: PortId,
        channel_id: ChannelId,
        sequence: Sequence,
    },
    Receipts {
        port_id: PortId,
        channel_id: ChannelId,
        sequence: Sequence,
    },
    Upgrade(ClientUpgradePath),
}

/// Paths that are specific for client upgrades.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ClientUpgradePath {
    UpgradedClientState(u64),
    UpgradedClientConsensusState(u64),
}

impl Path {
    /// Indication if the path is provable.
    pub fn is_provable(&self) -> bool {
        !matches!(&self, Path::ClientConnections(_) | Path::Ports(_))
    }

    /// into_bytes implementation
    pub fn into_bytes(self) -> Vec<u8> {
        self.to_string().into_bytes()
    }
}

/// The Display trait adds the `.to_string()` method to the Path struct
/// This is where the different path strings are constructed
impl Display for Path {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match &self {
            Path::ClientType(client_id) => write!(f, "clients/{}/clientType", client_id),
            Path::ClientState(client_id) => write!(f, "clients/{}/clientState", client_id),
            Path::ClientConsensusState {
                client_id,
                epoch,
                height,
            } => write!(
                f,
                "clients/{}/consensusStates/{}-{}",
                client_id, epoch, height
            ),
            Path::ClientConnections(client_id) => write!(f, "clients/{}/connections", client_id),
            Path::Connections(connection_id) => write!(f, "connections/{}", connection_id),
            Path::Ports(port_id) => write!(f, "ports/{}", port_id),
            Path::ChannelEnds(port_id, channel_id) => {
                write!(f, "channelEnds/ports/{}/channels/{}", port_id, channel_id)
            }
            Path::SeqSends(port_id, channel_id) => write!(
                f,
                "nextSequenceSend/ports/{}/channels/{}",
                port_id, channel_id
            ),
            Path::SeqRecvs(port_id, channel_id) => write!(
                f,
                "nextSequenceRecv/ports/{}/channels/{}",
                port_id, channel_id
            ),
            Path::SeqAcks(port_id, channel_id) => write!(
                f,
                "nextSequenceAck/ports/{}/channels/{}",
                port_id, channel_id
            ),
            Path::Commitments {
                port_id,
                channel_id,
                sequence,
            } => write!(
                f,
                "commitments/ports/{}/channels/{}/sequences/{}",
                port_id, channel_id, sequence
            ),
            Path::Acks {
                port_id,
                channel_id,
                sequence,
            } => write!(
                f,
                "acks/ports/{}/channels/{}/sequences/{}",
                port_id, channel_id, sequence
            ),
            Path::Receipts {
                port_id,
                channel_id,
                sequence,
            } => write!(
                f,
                "receipts/ports/{}/channels/{}/sequences/{}",
                port_id, channel_id, sequence
            ),
            Path::Upgrade(ClientUpgradePath::UpgradedClientState(height)) => write!(
                f,
                "{}/{}/{}",
                UPGRADED_IBC_STATE, height, UPGRADED_CLIENT_STATE
            ),
            Path::Upgrade(ClientUpgradePath::UpgradedClientConsensusState(height)) => write!(
                f,
                "{}/{}/{}",
                UPGRADED_IBC_STATE, height, UPGRADED_CLIENT_CONSENSUS_STATE
            ),
        }
    }
}
