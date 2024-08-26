/*!
    Helper functions for bootstrapping a channel between two chains.
*/

use eyre::{eyre, Report as Error};
use ibc_relayer::chain::handle::ChainHandle;
use ibc_relayer::channel::{Channel, ChannelSide};
use ibc_relayer_types::core::ics04_channel::channel::Ordering;
use ibc_relayer_types::core::ics04_channel::version::Version;
use ibc_relayer_types::core::ics24_host::identifier::PortId;
use tracing::{debug, info};

use super::connection::{bootstrap_connection, BootstrapConnectionOptions};
use crate::types::binary::chains::ConnectedChains;
use crate::types::binary::channel::ConnectedChannel;
use crate::types::binary::connection::ConnectedConnection;
use crate::types::binary::foreign_client::ForeignClientPair;
use crate::types::id::TaggedPortIdRef;
use crate::types::tagged::*;
use crate::util::random::random_u64_range;

pub struct BootstrapChannelOptions {
    pub order: Ordering,
    pub version: Version,
    pub pad_channel_id_a: u64,
    pub pad_channel_id_b: u64,
}

/**
   Create a new [`ConnectedChannel`] based on the provided [`ConnectedChains`].

   Also accepts the [`PortId`] that should be used for the two sides of the
   channel.
*/
pub fn bootstrap_channel_with_chains<ChainA: ChainHandle, ChainB: ChainHandle>(
    chains: &ConnectedChains<ChainA, ChainB>,
    port_a: &PortId,
    port_b: &PortId,
    connection_options: BootstrapConnectionOptions,
    channel_options: BootstrapChannelOptions,
) -> Result<ConnectedChannel<ChainA, ChainB>, Error> {
    let channel = bootstrap_channel(
        &chains.foreign_clients,
        &DualTagged::new(port_a),
        &DualTagged::new(port_b),
        connection_options,
        channel_options,
    )?;

    Ok(channel)
}

/**
    Create a new [`ConnectedChannel`] between two chains using foreign clients
    with initialized client IDs.
*/
pub fn bootstrap_channel<ChainA: ChainHandle, ChainB: ChainHandle>(
    foreign_clients: &ForeignClientPair<ChainA, ChainB>,
    port_a: &TaggedPortIdRef<ChainA, ChainB>,
    port_b: &TaggedPortIdRef<ChainB, ChainA>,
    connection_options: BootstrapConnectionOptions,
    channel_options: BootstrapChannelOptions,
) -> Result<ConnectedChannel<ChainA, ChainB>, Error> {
    let connection = bootstrap_connection(foreign_clients, connection_options)?;

    bootstrap_channel_with_connection(
        &foreign_clients.handle_a(),
        &foreign_clients.handle_b(),
        connection,
        port_a,
        port_b,
        channel_options,
    )
}

/**
   Create a new [`ConnectedChannel`] using existing [`ConnectedConnection`].
*/
pub fn bootstrap_channel_with_connection<ChainA: ChainHandle, ChainB: ChainHandle>(
    chain_a: &ChainA,
    chain_b: &ChainB,
    connection: ConnectedConnection<ChainA, ChainB>,
    port_a: &TaggedPortIdRef<ChainA, ChainB>,
    port_b: &TaggedPortIdRef<ChainB, ChainA>,
    options: BootstrapChannelOptions,
) -> Result<ConnectedChannel<ChainA, ChainB>, Error> {
    pad_channel_id(
        chain_a,
        chain_b,
        &connection,
        port_a,
        options.pad_channel_id_a,
    )?;
    pad_channel_id(
        chain_b,
        chain_a,
        &connection.clone().flip(),
        port_b,
        options.pad_channel_id_b,
    )?;

    let channel = Channel::new(
        connection.connection.clone(),
        options.order,
        port_a.0.clone(),
        port_b.0.clone(),
        Some(options.version),
    )?;

    let channel_id_a = channel
        .a_side
        .channel_id()
        .ok_or_else(|| eyre!("expect channel id"))?
        .clone();

    let channel_id_b = channel
        .b_side
        .channel_id()
        .ok_or_else(|| eyre!("expect channel id"))?
        .clone();

    info!(
        "created new chain/client/connection/channel from {}/{}/{}/{} to {}/{}/{}/{}",
        chain_a.id(),
        connection.client_ids.client_id_a,
        connection.connection_id_a,
        channel_id_a,
        chain_b.id(),
        connection.client_ids.client_id_b,
        connection.connection_id_b,
        channel_id_b,
    );

    let res = ConnectedChannel {
        connection,
        channel,
        channel_id_a: DualTagged::new(channel_id_a),
        channel_id_b: DualTagged::new(channel_id_b),
        port_a: port_a.cloned(),
        port_b: port_b.cloned(),
    };

    Ok(res)
}

/**
   Create a random number of dummy channel IDs so that the bootstrapped
   channel ID is random instead of being always `channel-0`.

   This would help us catch bugs where the channel IDs are used at
   the wrong side of the chain, but still got accepted because the
   channel IDs on both sides are the same.
*/
pub fn pad_channel_id<ChainA: ChainHandle, ChainB: ChainHandle>(
    chain_a: &ChainA,
    chain_b: &ChainB,
    connection: &ConnectedConnection<ChainA, ChainB>,
    port_id: &TaggedPortIdRef<ChainA, ChainB>,
    pad_count: u64,
) -> Result<(), Error> {
    let client_id_a = &connection.client_ids.client_id_a;
    let client_id_b = &connection.client_ids.client_id_b;

    for i in 0..pad_count {
        debug!(
            "creating new channel id {} on chain/connection/client {}/{}/{}",
            i + 1,
            chain_a.id(),
            connection.connection_id_a,
            client_id_a,
        );

        let channel: Channel<ChainB, ChainA> = Channel {
            ordering: Ordering::Unordered,
            a_side: ChannelSide::new(
                chain_b.clone(),
                client_id_b.value().clone(),
                connection.connection_id_b.value().clone(),
                port_id.cloned().into_value(),
                None,
                None,
            ),
            b_side: ChannelSide::new(
                chain_a.clone(),
                client_id_a.value().clone(),
                connection.connection_id_a.value().clone(),
                port_id.cloned().into_value(),
                None,
                None,
            ),
            connection_delay: connection.connection.delay_period,
        };

        channel.build_chan_open_init_and_send()?;
    }

    Ok(())
}

impl Default for BootstrapChannelOptions {
    fn default() -> Self {
        Self {
            order: Ordering::Unordered,
            version: Version::ics20(1),
            pad_channel_id_a: 0,
            pad_channel_id_b: 1,
        }
    }
}

impl BootstrapChannelOptions {
    pub fn order(mut self, order: Ordering) -> Self {
        self.order = order;
        self
    }

    pub fn version(mut self, version: Version) -> Self {
        self.version = version;
        self
    }

    pub fn bootstrap_with_random_ids(mut self, bootstrap_with_random_ids: bool) -> Self {
        if bootstrap_with_random_ids {
            self.pad_channel_id_a = random_u64_range(0, 6);
            self.pad_channel_id_b = random_u64_range(0, 6);
        } else {
            self.pad_channel_id_a = 0;
            self.pad_channel_id_b = 1;
        }

        self
    }
}
