use ibc_relayer::util::task::TaskHandle;
use ibc_relayer::worker::client::spawn_refresh_client;

use crate::bootstrap::binary::chain::bootstrap_foreign_client;
use crate::bootstrap::binary::connection::bootstrap_connection;
use crate::ibc::denom::derive_ibc_denom;
use crate::prelude::*;
use crate::relayer::channel::{assert_eventually_channel_established, init_channel};
use crate::relayer::connection::{assert_eventually_connection_established, init_connection};
use crate::types::binary::client::ConnectedClients;
use crate::types::binary::connection::ConnectedConnection;
use crate::types::tagged::mono::Tagged;

use super::state::Packet;

use super::utils::{get_denom, get_wallet, wait_for_client};

pub fn setup_chains<ChainA: ChainHandle, ChainB: ChainHandle>(
    chains: &ConnectedChains<ChainA, ChainB>,
) -> Result<(), Error> {
    {
        let _refresh_task_a = spawn_refresh_client(chains.client_b_to_a.clone())
            .ok_or_else(|| eyre!("expect refresh task spawned"))?;

        let _refresh_task_b = spawn_refresh_client(chains.client_a_to_b.clone())
            .ok_or_else(|| eyre!("expect refresh task spawned"))?;

        bootstrap_connection(&chains.client_b_to_a, &chains.client_a_to_b, false)?;
    };

    wait_for_client();

    Ok(())
}

pub fn local_transfer_handler<ChainA>(
    node: Tagged<ChainA, &FullNode>,
    source: u64,
    target: u64,
    denom: u64,
    amount: u64,
) -> Result<(), Error> {
    let wallets = node.wallets();

    let source_wallet = get_wallet(&wallets, source);
    let target_wallet = get_wallet(&wallets, target);
    let denom = get_denom(&node, denom);

    node.chain_driver().local_transfer_token(
        &source_wallet.address(),
        &target_wallet.address(),
        amount,
        &denom,
    )?;

    Ok(())
}

pub fn create_channel<ChainA: ChainHandle, ChainB: ChainHandle>(
    chain_handle_a: &ChainA,
    chain_handle_b: &ChainB,
    channel: &mut Option<ConnectedChannel<ChainA, ChainB>>,
    refresh_task_a: &mut Option<TaskHandle>,
    refresh_task_b: &mut Option<TaskHandle>,
) -> Result<(), Error> {
    let port_a = tagged_transfer_port();
    let port_b = tagged_transfer_port();

    let client_b_to_a_2 = bootstrap_foreign_client(chain_handle_b, chain_handle_a)?;
    let client_a_to_b_2 = bootstrap_foreign_client(chain_handle_a, chain_handle_b)?;

    *refresh_task_a = Some(
        spawn_refresh_client(client_b_to_a_2.clone())
            .ok_or_else(|| eyre!("expect refresh task spawned"))?,
    );

    *refresh_task_b = Some(
        spawn_refresh_client(client_a_to_b_2.clone())
            .ok_or_else(|| eyre!("expect refresh task spawned"))?,
    );

    let (connection_id_b, new_connection_b) = init_connection(
        chain_handle_a,
        chain_handle_b,
        &client_b_to_a_2.tagged_client_id(),
        &client_a_to_b_2.tagged_client_id(),
    )?;

    let connection_id_a = assert_eventually_connection_established(
        chain_handle_b,
        chain_handle_a,
        &connection_id_b.as_ref(),
    )?;

    let (channel_id_b_2, channel_b_2) = init_channel(
        chain_handle_a,
        chain_handle_b,
        &client_b_to_a_2.tagged_client_id(),
        &client_a_to_b_2.tagged_client_id(),
        &connection_id_a.as_ref(),
        &connection_id_b.as_ref(),
        &port_a.as_ref(),
        &port_b.as_ref(),
    )?;

    let channel_id_a_2 = assert_eventually_channel_established(
        chain_handle_b,
        chain_handle_a,
        &channel_id_b_2.as_ref(),
        &port_b.as_ref(),
    )?;

    let new_connected_clients = ConnectedClients {
        client_id_a: DualTagged::new(client_b_to_a_2.id),
        client_id_b: DualTagged::new(client_a_to_b_2.id),
    };

    let new_connected_connection = ConnectedConnection {
        client: new_connected_clients,
        connection: new_connection_b.flipped(),
        connection_id_a,
        connection_id_b,
    };

    let connected_channel = ConnectedChannel {
        connection: new_connected_connection,
        channel: channel_b_2.flipped(),
        channel_id_a: channel_id_a_2,
        channel_id_b: channel_id_b_2,
        port_a,
        port_b,
    };

    *channel = Some(connected_channel);

    info!("Channel is created");

    Ok(())
}

pub fn expire_channel<ChainA: ChainHandle, ChainB: ChainHandle>(
    channel: &mut Option<ConnectedChannel<ChainA, ChainB>>,
    refresh_task_a: &mut Option<TaskHandle>,
    refresh_task_b: &mut Option<TaskHandle>,
) -> Result<(), Error> {
    // dropping the client handler to expire the clients
    super::utils::drop(refresh_task_a.take());
    super::utils::drop(refresh_task_b.take());

    wait_for_client();

    super::utils::drop(channel.take());

    info!("Channel expired");

    Ok(())
}

pub fn ibc_transfer_send_packet<ChainA: ChainHandle, ChainB: ChainHandle>(
    node_source: Tagged<ChainA, &FullNode>,
    node_target: Tagged<ChainB, &FullNode>,
    channels: &ConnectedChannel<ChainA, ChainB>,
    packet: &Packet,
) -> Result<(), Error> {
    let wallets_source = node_source.wallets();
    let wallets_target = node_target.wallets();

    let wallet_source = get_wallet(&wallets_source, packet.from);
    let wallet_target = get_wallet(&wallets_target, packet.to);
    let denom_source = get_denom(&node_source, packet.denom);
    let amount_source_to_target = packet.amount;

    let (port_source, channel_id_source) = (
        DualTagged::new(channels.port_a.value()),
        DualTagged::new(channels.channel_id_a.value()),
    );

    let balance_source = node_source
        .chain_driver()
        .query_balance(&wallet_source.address(), &denom_source)?;

    info!(
        "Sending IBC transfer from chain {} to chain {} with amount of {} {}",
        node_source.chain_id(),
        node_target.chain_id(),
        amount_source_to_target,
        denom_source,
    );

    node_source.chain_driver().transfer_token(
        &port_source,
        &channel_id_source,
        &wallet_source.address(),
        &wallet_target.address(),
        amount_source_to_target,
        &denom_source,
    )?;

    node_source.chain_driver().assert_eventual_wallet_amount(
        &wallet_source,
        balance_source - amount_source_to_target,
        &denom_source,
    )?;

    Ok(())
}

pub fn ibc_transfer_receive_packet<ChainA: ChainHandle, ChainB: ChainHandle>(
    node_source: Tagged<ChainA, &FullNode>,
    node_target: Tagged<ChainB, &FullNode>,
    channels: &ConnectedChannel<ChainA, ChainB>,
    packet: &Packet,
) -> Result<(), Error> {
    let wallets_target = node_target.wallets();

    let wallet_target = get_wallet(&wallets_target, packet.to);
    let denom_source = get_denom(&node_source, packet.denom);
    let amount_source_to_target = packet.amount;

    let (port_target, channel_id_target) = (
        DualTagged::new(channels.port_b.value()),
        DualTagged::new(channels.channel_id_b.value()),
    );

    let denom_target = derive_ibc_denom(&port_target, &channel_id_target, &denom_source)?;

    info!(
        "Waiting for user on chain {} to receive IBC transferred amount of {} {} (chain {}/{})",
        node_target.chain_id(),
        amount_source_to_target,
        denom_target,
        node_source.chain_id(),
        denom_source
    );

    node_target.chain_driver().assert_eventual_wallet_amount(
        &wallet_target,
        amount_source_to_target,
        &denom_target.as_ref(),
    )?;

    Ok(())
}

pub fn ibc_transfer_acknowledge_packet<ChainA: ChainHandle, ChainB: ChainHandle>(
    node_source: Tagged<ChainA, &FullNode>,
    node_target: Tagged<ChainB, &FullNode>,
    _channels: &Option<ConnectedChannel<ChainA, ChainB>>,
    packet: &Packet,
) -> Result<(), Error> {
    let denom_source = get_denom(&node_source, packet.denom);
    let amount_source_to_target = packet.amount;

    info!(
        "Waiting for user on chain {} to confirm IBC transferred amount of {} {}",
        node_source.chain_id(),
        amount_source_to_target,
        denom_source
    );

    info!(
        "Successfully performed IBC transfer from chain {} to chain {}",
        node_source.chain_id(),
        node_target.chain_id(),
    );

    Ok(())
}

pub fn ibc_transfer_expire_packet<ChainA: ChainHandle, ChainB: ChainHandle>(
    node_source: Tagged<ChainA, &FullNode>,
    node_target: Tagged<ChainB, &FullNode>,
    _channels: &Option<ConnectedChannel<ChainA, ChainB>>,
    packet: &Packet,
) -> Result<(), Error> {
    let denom_source = get_denom(&node_source, packet.denom);
    let amount_source_to_target = packet.amount;

    info!(
        "Waiting for user on chain {} to get refund of previously IBC transferred amount of {} {}",
        node_source.chain_id(),
        amount_source_to_target,
        denom_source
    );

    info!(
        "Successfully performed IBC packet expiry intended from chain {} to chain {}",
        node_source.chain_id(),
        node_target.chain_id(),
    );

    Ok(())
}
