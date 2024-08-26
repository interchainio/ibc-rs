//! This tests different scenarios for the packet sequence filter.
//! The purpose of this filter is to only filter out packets when clearing,
//! standard relaying should not be affected by this configuration.
//!
//! `FilterClearOnStartTest` tests that the packets sequences configured in
//! `excluded_sequences` are not relayed when clearing packet on start.
//!
//! `FilterClearIntervalTest` tests that the packet sequences configured in
//! `excluded_sequences` are not relayed when the clear interval is triggered.
//!
//! `ClearNoFilterTest` tests that packets are correctly cleared if there is no
//! packet sequence configured in `excluded_sequences`.
//!
//! `StandardRelayingNoFilterTest` tests that even if a packet sequence is
//! configured in the `excluded_sequences` it will be relayed by the running
//! instance.

use std::collections::BTreeMap;

use ibc_relayer::channel::Channel;
use ibc_relayer::object::Channel as ObjectChannel;
use ibc_relayer::util::excluded_sequences::ExcludedSequences;
use ibc_relayer::{chain::requests::QueryHeight, config::ChainConfig};
use ibc_test_framework::{
    prelude::*,
    relayer::channel::{assert_eventually_channel_established, init_channel},
};

#[test]
fn test_filter_clear_on_start() -> Result<(), Error> {
    run_binary_channel_test(&FilterClearOnStartTest)
}

#[test]
fn test_filter_clear_interval() -> Result<(), Error> {
    run_binary_channel_test(&FilterClearIntervalTest)
}

#[test]
fn test_clear_no_filter() -> Result<(), Error> {
    run_binary_channel_test(&ClearNoFilterTest)
}

#[test]
fn test_no_filter_standard_relaying() -> Result<(), Error> {
    run_binary_channel_test(&StandardRelayingNoFilterTest)
}

pub struct FilterClearOnStartTest;

impl TestOverrides for FilterClearOnStartTest {
    fn modify_relayer_config(&self, config: &mut Config) {
        let mut excluded_sequences = BTreeMap::new();
        excluded_sequences.insert(ChannelId::new(2), vec![2.into()]);
        let chain_a = &mut config.chains[0];
        match chain_a {
            ChainConfig::CosmosSdk(chain_config) => {
                chain_config.excluded_sequences = ExcludedSequences::new(excluded_sequences);
            }
        }
        config.mode.channels.enabled = true;

        config.mode.packets.clear_on_start = true;
        config.mode.packets.clear_interval = 0;

        config.mode.clients.misbehaviour = false;
    }

    fn should_spawn_supervisor(&self) -> bool {
        false
    }
}

impl BinaryChannelTest for FilterClearOnStartTest {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        _config: &TestConfig,
        relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
        channels: ConnectedChannel<ChainA, ChainB>,
    ) -> Result<(), Error> {
        run_sequence_filter_test(relayer, chains, channels)
    }
}

pub struct FilterClearIntervalTest;

impl TestOverrides for FilterClearIntervalTest {
    fn modify_relayer_config(&self, config: &mut Config) {
        let mut excluded_sequences = BTreeMap::new();
        excluded_sequences.insert(ChannelId::new(2), vec![2.into()]);
        let chain_a = &mut config.chains[0];
        match chain_a {
            ChainConfig::CosmosSdk(chain_config) => {
                chain_config.excluded_sequences = ExcludedSequences::new(excluded_sequences);
            }
        }
        config.mode.channels.enabled = true;

        config.mode.packets.clear_on_start = false;
        config.mode.packets.clear_interval = 10;

        config.mode.clients.misbehaviour = false;
    }

    fn should_spawn_supervisor(&self) -> bool {
        false
    }
}

impl BinaryChannelTest for FilterClearIntervalTest {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        _config: &TestConfig,
        relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
        channels: ConnectedChannel<ChainA, ChainB>,
    ) -> Result<(), Error> {
        run_sequence_filter_test(relayer, chains, channels)
    }
}

pub struct ClearNoFilterTest;

impl TestOverrides for ClearNoFilterTest {
    fn modify_relayer_config(&self, config: &mut Config) {
        config.mode.channels.enabled = true;

        config.mode.packets.clear_on_start = true;
        config.mode.packets.clear_interval = 0;

        config.mode.clients.misbehaviour = false;
    }

    fn should_spawn_supervisor(&self) -> bool {
        false
    }
}

impl BinaryChannelTest for ClearNoFilterTest {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        _config: &TestConfig,
        relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
        channels: ConnectedChannel<ChainA, ChainB>,
    ) -> Result<(), Error> {
        let denom_a = chains.node_a.denom();
        let denom_a_to_b = derive_ibc_denom(
            &channels.port_b.as_ref(),
            &channels.channel_id_b.as_ref(),
            &denom_a,
        )?;
        let denom_b = chains.node_b.denom();
        let denom_b_to_a = derive_ibc_denom(
            &channels.port_a.as_ref(),
            &channels.channel_id_a.as_ref(),
            &denom_b,
        )?;

        let wallet_a = chains.node_a.wallets().user1().cloned();
        let wallet_b = chains.node_b.wallets().user1().cloned();

        let a_to_b_amount = 12345u64;
        let b_to_a_amount = 54321u64;

        let balance_a = chains
            .node_a
            .chain_driver()
            .query_balance(&wallet_a.address(), &denom_a)?;

        let balance_b = chains
            .node_b
            .chain_driver()
            .query_balance(&wallet_b.address(), &denom_b)?;

        // Create a pending transfer from A to B with sequence 1
        chains.node_a.chain_driver().ibc_transfer_token(
            &channels,
            &wallet_a.as_ref(),
            &wallet_b.address(),
            &vec![denom_a.with_amount(a_to_b_amount).as_ref()],
        )?;

        // Create a pending transfer from A to B with sequence 2
        chains.node_a.chain_driver().ibc_transfer_token(
            &channels,
            &wallet_a.as_ref(),
            &wallet_b.address(),
            &vec![denom_a.with_amount(a_to_b_amount).as_ref()],
        )?;

        // Create a pending transfer from B to A with sequence 1
        chains.node_b.chain_driver().ibc_transfer_token(
            &channels.clone().flip(),
            &wallet_b.as_ref(),
            &wallet_a.address(),
            &vec![denom_b.with_amount(b_to_a_amount).as_ref()],
        )?;

        // Create a pending transfer from B to A with sequence 2
        chains.node_b.chain_driver().ibc_transfer_token(
            &channels.flip(),
            &wallet_b.as_ref(),
            &wallet_a.address(),
            &vec![denom_b.with_amount(b_to_a_amount).as_ref()],
        )?;

        relayer.with_supervisor(|| {
            info!("Assert that the send from A escrowed tokens for both transfers");

            chains.node_a.chain_driver().assert_eventual_wallet_amount(
                &wallet_a.address(),
                &(balance_a - 2 * a_to_b_amount).as_ref(),
            )?;

            info!("Assert that only the first transfer A to B was cleared");

            chains.node_b.chain_driver().assert_eventual_wallet_amount(
                &wallet_b.address(),
                &denom_a_to_b.with_amount(2 * a_to_b_amount).as_ref(),
            )?;
            info!("Assert that the sender from B escrowed tokens for both transfers");

            chains.node_b.chain_driver().assert_eventual_wallet_amount(
                &wallet_b.address(),
                &(balance_b - 2 * b_to_a_amount).as_ref(),
            )?;

            info!("Assert that both transfers from B to A were cleared");

            chains.node_a.chain_driver().assert_eventual_wallet_amount(
                &wallet_a.address(),
                &denom_b_to_a.with_amount(2 * b_to_a_amount).as_ref(),
            )?;
            Ok(())
        })
    }
}

pub struct StandardRelayingNoFilterTest;

impl TestOverrides for StandardRelayingNoFilterTest {
    fn modify_relayer_config(&self, config: &mut Config) {
        let mut excluded_sequences = BTreeMap::new();
        excluded_sequences.insert(ChannelId::new(2), vec![2.into()]);
        let chain_a = &mut config.chains[0];
        match chain_a {
            ChainConfig::CosmosSdk(chain_config) => {
                chain_config.excluded_sequences = ExcludedSequences::new(excluded_sequences);
            }
        }
        config.mode.packets.clear_on_start = true;
        config.mode.packets.clear_interval = 0;
    }

    fn should_spawn_supervisor(&self) -> bool {
        true
    }
}

impl BinaryChannelTest for StandardRelayingNoFilterTest {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        _config: &TestConfig,
        _relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
        channels: ConnectedChannel<ChainA, ChainB>,
    ) -> Result<(), Error> {
        let denom_a = chains.node_a.denom();
        let denom_a_to_b = derive_ibc_denom(
            &channels.port_b.as_ref(),
            &channels.channel_id_b.as_ref(),
            &denom_a,
        )?;
        let denom_b = chains.node_b.denom();
        let denom_b_to_a = derive_ibc_denom(
            &channels.port_a.as_ref(),
            &channels.channel_id_a.as_ref(),
            &denom_b,
        )?;

        let wallet_a = chains.node_a.wallets().user1().cloned();
        let wallet_b = chains.node_b.wallets().user1().cloned();

        let a_to_b_amount = 12345u64;
        let b_to_a_amount = 54321u64;

        let balance_a = chains
            .node_a
            .chain_driver()
            .query_balance(&wallet_a.address(), &denom_a)?;

        let balance_b = chains
            .node_b
            .chain_driver()
            .query_balance(&wallet_b.address(), &denom_b)?;

        // Create a pending transfer from A to B with sequence 1
        chains.node_a.chain_driver().ibc_transfer_token(
            &channels,
            &wallet_a.as_ref(),
            &wallet_b.address(),
            &vec![denom_a.with_amount(a_to_b_amount).as_ref()],
        )?;

        // Create a pending transfer from A to B with sequence 2
        chains.node_a.chain_driver().ibc_transfer_token(
            &channels,
            &wallet_a.as_ref(),
            &wallet_b.address(),
            &vec![denom_a.with_amount(a_to_b_amount).as_ref()],
        )?;

        // Create a pending transfer from B to A with sequence 1
        chains.node_b.chain_driver().ibc_transfer_token(
            &channels.clone().flip(),
            &wallet_b.as_ref(),
            &wallet_a.address(),
            &vec![denom_b.with_amount(b_to_a_amount).as_ref()],
        )?;

        // Create a pending transfer from B to A with sequence 2
        chains.node_b.chain_driver().ibc_transfer_token(
            &channels.flip(),
            &wallet_b.as_ref(),
            &wallet_a.address(),
            &vec![denom_b.with_amount(b_to_a_amount).as_ref()],
        )?;

        info!("Assert that the send from A escrowed tokens for both transfers");

        chains.node_a.chain_driver().assert_eventual_wallet_amount(
            &wallet_a.address(),
            &(balance_a - 2 * a_to_b_amount).as_ref(),
        )?;

        info!("Assert that only the first transfer A to B was cleared");

        chains.node_b.chain_driver().assert_eventual_wallet_amount(
            &wallet_b.address(),
            &denom_a_to_b.with_amount(2 * a_to_b_amount).as_ref(),
        )?;
        info!("Assert that the sender from B escrowed tokens for both transfers");

        chains.node_b.chain_driver().assert_eventual_wallet_amount(
            &wallet_b.address(),
            &(balance_b - 2 * b_to_a_amount).as_ref(),
        )?;

        info!("Assert that both transfers from B to A were cleared");

        chains.node_a.chain_driver().assert_eventual_wallet_amount(
            &wallet_a.address(),
            &denom_b_to_a.with_amount(2 * b_to_a_amount).as_ref(),
        )?;
        Ok(())
    }
}

fn run_sequence_filter_test<ChainA: ChainHandle, ChainB: ChainHandle>(
    relayer: RelayerDriver,
    chains: ConnectedChains<ChainA, ChainB>,
    channels: ConnectedChannel<ChainA, ChainB>,
) -> Result<(), Error> {
    let (channel_id_a_2, channel_id_b_2) = relayer.with_supervisor(|| {
        // During test bootstrap channel padding initialises a channel with ID 0.
        // Before creating the new channel with sequence filter, complete the handshake of
        // the pad channel in order to insure that the retrieved channel IDs `channel_id_a_2` and
        // `channel_id_b_2` are `channel-2`
        let pad_channel_id = DualTagged::new(ChannelId::new(0));
        let _ = assert_eventually_channel_established(
            chains.handle_b(),
            chains.handle_a(),
            &pad_channel_id.as_ref(),
            &channels.port_b.as_ref(),
        )?;
        let (channel_id_b_2, _) = init_channel(
            chains.handle_a(),
            chains.handle_b(),
            &chains.client_id_a(),
            &chains.client_id_b(),
            &channels.connection.connection_id_a.as_ref(),
            &channels.connection.connection_id_b.as_ref(),
            &channels.port_a.as_ref(),
            &channels.port_b.as_ref(),
        )?;

        let channel_id_a_2 = assert_eventually_channel_established(
            chains.handle_b(),
            chains.handle_a(),
            &channel_id_b_2.as_ref(),
            &channels.port_b.as_ref(),
        )?;
        Ok((channel_id_a_2, channel_id_b_2))
    })?;

    let (channel_2, _) = Channel::restore_from_state(
        chains.handle_a.clone(),
        chains.handle_b.clone(),
        ObjectChannel {
            dst_chain_id: chains.handle_b().id(),
            src_chain_id: chains.handle_a().id(),
            src_channel_id: channel_id_a_2.0.clone(),
            src_port_id: channels.port_a.0.clone(),
        },
        QueryHeight::Latest,
    )?;

    let denom_a = chains.node_a.denom();
    let denom_a_to_b_1 = derive_ibc_denom(
        &channels.port_b.as_ref(),
        &channels.channel_id_b.as_ref(),
        &denom_a,
    )?;
    let denom_a_to_b_2 = derive_ibc_denom(
        &channels.port_b.as_ref(),
        &channel_id_b_2.as_ref(),
        &denom_a,
    )?;
    let denom_b = chains.node_b.denom();
    let denom_b_to_a_1 = derive_ibc_denom(
        &channels.port_a.as_ref(),
        &channels.channel_id_a.as_ref(),
        &denom_b,
    )?;
    let denom_b_to_a_2 = derive_ibc_denom(
        &channels.port_a.as_ref(),
        &channel_id_a_2.as_ref(),
        &denom_b,
    )?;

    let wallet_a_1 = chains.node_a.wallets().user1().cloned();
    let wallet_a_2 = chains.node_a.wallets().user2().cloned();
    let wallet_b_1 = chains.node_b.wallets().user1().cloned();
    let wallet_b_2 = chains.node_b.wallets().user2().cloned();

    let a_to_b_amount = 12345u64;
    let b_to_a_amount = 54321u64;

    let balance_a_1 = chains
        .node_a
        .chain_driver()
        .query_balance(&wallet_a_1.address(), &denom_a)?;

    let balance_b_1 = chains
        .node_b
        .chain_driver()
        .query_balance(&wallet_b_1.address(), &denom_b)?;

    let balance_a_2 = chains
        .node_a
        .chain_driver()
        .query_balance(&wallet_a_2.address(), &denom_a)?;

    let balance_b_2 = chains
        .node_b
        .chain_driver()
        .query_balance(&wallet_b_2.address(), &denom_b)?;

    // Double transfer from A to B on channel with filter
    double_transfer(
        chains.node_a.chain_driver(),
        &channels,
        &wallet_a_1.as_ref(),
        &wallet_b_1.address(),
        &denom_a.with_amount(a_to_b_amount).as_ref(),
    )?;

    let port_b_2: DualTagged<ChainB, ChainA, PortId> =
        DualTagged::new(channel_2.a_side.port_id().clone());
    let port_a_2: DualTagged<ChainA, ChainB, PortId> =
        DualTagged::new(channel_2.clone().flipped().a_side.port_id().clone());

    let connected_channel_2 = ConnectedChannel {
        connection: channels.clone().connection,
        channel: channel_2,
        channel_id_a: channel_id_a_2,
        channel_id_b: channel_id_b_2,
        port_a: port_a_2,
        port_b: port_b_2,
    };

    // Double transfer from A to B on channel without filter
    double_transfer(
        chains.node_a.chain_driver(),
        &connected_channel_2,
        &wallet_a_2.as_ref(),
        &wallet_b_2.address(),
        &denom_a.with_amount(a_to_b_amount).as_ref(),
    )?;

    // Double transfer from B to A on channel with filter
    double_transfer(
        chains.node_b.chain_driver(),
        &channels.flip(),
        &wallet_b_1.as_ref(),
        &wallet_a_1.address(),
        &denom_b.with_amount(b_to_a_amount).as_ref(),
    )?;

    // Double transfer from B to A on channel without filter
    double_transfer(
        chains.node_b.chain_driver(),
        &connected_channel_2.flip(),
        &wallet_b_2.as_ref(),
        &wallet_a_2.address(),
        &denom_b.with_amount(b_to_a_amount).as_ref(),
    )?;

    relayer.with_supervisor(|| {
        info!("Assert that the send from A escrowed tokens for both transfers");

        chains.node_a.chain_driver().assert_eventual_wallet_amount(
            &wallet_a_1.address(),
            &(balance_a_1 - 2 * a_to_b_amount).as_ref(),
        )?;

        chains.node_a.chain_driver().assert_eventual_wallet_amount(
            &wallet_a_2.address(),
            &(balance_a_2 - 2 * a_to_b_amount).as_ref(),
        )?;

        info!("Assert that only the first transfer A to B was cleared on channel with filter");

        chains.node_b.chain_driver().assert_eventual_wallet_amount(
            &wallet_b_2.address(),
            &denom_a_to_b_2.with_amount(a_to_b_amount).as_ref(),
        )?;

        info!("Assert that both transfer A to B were cleared on channel without filter");

        chains.node_b.chain_driver().assert_eventual_wallet_amount(
            &wallet_b_1.address(),
            &denom_a_to_b_1.with_amount(2 * a_to_b_amount).as_ref(),
        )?;

        info!("Assert that the sender from B escrowed tokens for both transfers on both channels");

        chains.node_b.chain_driver().assert_eventual_wallet_amount(
            &wallet_b_1.address(),
            &(balance_b_1 - 2 * b_to_a_amount).as_ref(),
        )?;

        chains.node_b.chain_driver().assert_eventual_wallet_amount(
            &wallet_b_2.address(),
            &(balance_b_2 - 2 * b_to_a_amount).as_ref(),
        )?;

        info!("Assert that both transfers from B to A were cleared on both channels");

        chains.node_a.chain_driver().assert_eventual_wallet_amount(
            &wallet_a_1.address(),
            &denom_b_to_a_1.with_amount(2 * b_to_a_amount).as_ref(),
        )?;

        chains.node_a.chain_driver().assert_eventual_wallet_amount(
            &wallet_a_2.address(),
            &denom_b_to_a_2.with_amount(2 * b_to_a_amount).as_ref(),
        )?;

        Ok(())
    })
}

fn double_transfer<Chain: ChainHandle, Counterparty: ChainHandle>(
    chain_driver: MonoTagged<Chain, &ChainDriver>,
    channel: &ConnectedChannel<Chain, Counterparty>,
    sender: &MonoTagged<Chain, &Wallet>,
    recipient: &MonoTagged<Counterparty, &WalletAddress>,
    token: &TaggedTokenRef<Chain>,
) -> Result<(), Error> {
    // Create a pending transfer from B to A with sequence 1
    chain_driver.ibc_transfer_token(channel, sender, recipient, &vec![*token])?;

    // Create a pending transfer from B to A with sequence 2
    chain_driver.ibc_transfer_token(channel, sender, recipient, &vec![*token])?;

    Ok(())
}
