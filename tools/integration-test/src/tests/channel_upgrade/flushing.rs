//! Tests that the relayer correctly flushes in-flight packets during channel upgrade.
//!
//! - `ChannelUpgradeFlushing` tests that the channel worker will complete the
//!   upgrade handshake when there are pending packets before starting the channel upgrade.
//!
//! - `ChannelUpgradeHandshakeFlushPackets` tests that the channel worker will complete the
//!   upgrade handshake when packets need to be flushed during the handshake.

use ibc_relayer::chain::requests::{IncludeProof, QueryChannelRequest, QueryHeight};
use ibc_relayer_types::core::ics04_channel::channel::State as ChannelState;
use ibc_relayer_types::core::ics04_channel::packet::Sequence;
use ibc_relayer_types::core::ics04_channel::version::Version;
use ibc_test_framework::chain::config::{set_max_deposit_period, set_voting_period};
use ibc_test_framework::prelude::*;
use ibc_test_framework::relayer::channel::{
    assert_eventually_channel_established, assert_eventually_channel_upgrade_ack,
    assert_eventually_channel_upgrade_init, assert_eventually_channel_upgrade_open,
    assert_eventually_channel_upgrade_try, ChannelUpgradableAttributes,
};
use ibc_test_framework::util::random::random_u128_range;

#[test]
fn test_channel_upgrade_simple_flushing() -> Result<(), Error> {
    run_binary_channel_test(&ChannelUpgradeFlushing)
}

#[test]
fn test_channel_upgrade_handshake_flush_packets() -> Result<(), Error> {
    run_binary_channel_test(&ChannelUpgradeHandshakeFlushPackets)
}

const MAX_DEPOSIT_PERIOD: &str = "10s";
const VOTING_PERIOD: u64 = 10;

pub struct ChannelUpgradeFlushing;

impl TestOverrides for ChannelUpgradeFlushing {
    fn modify_relayer_config(&self, config: &mut Config) {
        config.mode.channels.enabled = true;
        config.mode.packets.auto_register_counterparty_payee = true;
        config.mode.packets.clear_interval = 0;
        config.mode.packets.clear_on_start = true;

        config.mode.clients.misbehaviour = false;
    }

    fn modify_genesis_file(&self, genesis: &mut serde_json::Value) -> Result<(), Error> {
        set_max_deposit_period(genesis, MAX_DEPOSIT_PERIOD)?;
        set_voting_period(genesis, VOTING_PERIOD)?;
        Ok(())
    }

    fn should_spawn_supervisor(&self) -> bool {
        false
    }
}

impl BinaryChannelTest for ChannelUpgradeFlushing {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        _config: &TestConfig,
        relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
        channels: ConnectedChannel<ChainA, ChainB>,
    ) -> Result<(), Error> {
        info!("Check that channels are both in OPEN State");

        assert_eventually_channel_established(
            &chains.handle_b,
            &chains.handle_a,
            &channels.channel_id_b.as_ref(),
            &channels.port_b.as_ref(),
        )?;

        let channel_end_a = chains
            .handle_a
            .query_channel(
                QueryChannelRequest {
                    port_id: channels.port_a.0.clone(),
                    channel_id: channels.channel_id_a.0.clone(),
                    height: QueryHeight::Latest,
                },
                IncludeProof::No,
            )
            .map(|(channel_end, _)| channel_end)
            .map_err(|e| eyre!("Error querying ChannelEnd A: {e}"))?;

        let channel_end_b = chains
            .handle_b
            .query_channel(
                QueryChannelRequest {
                    port_id: channels.port_b.0.clone(),
                    channel_id: channels.channel_id_b.0.clone(),
                    height: QueryHeight::Latest,
                },
                IncludeProof::No,
            )
            .map(|(channel_end, _)| channel_end)
            .map_err(|e| eyre!("Error querying ChannelEnd B: {e}"))?;

        let chain_driver_a = chains.node_a.chain_driver();
        let chain_driver_b = chains.node_b.chain_driver();

        let denom_a = chains.node_a.denom();

        let wallets_a = chains.node_a.wallets();
        let wallets_b = chains.node_b.wallets();

        let user_a = wallets_a.user1();
        let user_b = wallets_b.user1();

        let send_amount = random_u128_range(1000, 2000);

        chain_driver_a.ibc_transfer_token(
            &channels,
            &user_a,
            &user_b.address(),
            &vec![denom_a.with_amount(send_amount).as_ref()],
        )?;

        sleep(Duration::from_secs(3));

        chain_driver_a.ibc_transfer_token(
            &channels,
            &user_a,
            &user_b.address(),
            &vec![denom_a.with_amount(send_amount).as_ref()],
        )?;

        let old_ordering = channel_end_a.ordering;
        let old_connection_hops_a = channel_end_a.connection_hops;
        let old_connection_hops_b = channel_end_b.connection_hops;

        let channel = channels.channel;
        let new_version = Version::ics20_with_fee(1);

        let upgraded_attrs = ChannelUpgradableAttributes::new(
            new_version.clone(),
            new_version.clone(),
            old_ordering,
            old_connection_hops_a.clone(),
            old_connection_hops_b,
            Sequence::from(1),
        );

        info!("Will initialise upgrade handshake with governance proposal...");

        chains.node_a.chain_driver().initialise_channel_upgrade(
            channel.src_port_id().as_str(),
            channel.src_channel_id().unwrap().as_str(),
            old_ordering.as_str(),
            old_connection_hops_a.first().unwrap().as_str(),
            &serde_json::to_string(&new_version.0).unwrap(),
            chains.handle_a().get_signer().unwrap().as_ref(),
            "1",
        )?;

        sleep(Duration::from_secs(5));

        info!("Check that the channel upgrade successfully upgraded the version...");

        relayer.with_supervisor(|| {
            let denom_b = derive_ibc_denom(
                &channels.port_b.as_ref(),
                &channels.channel_id_b.as_ref(),
                &denom_a,
            )?;

            chain_driver_b.assert_eventual_wallet_amount(
                &user_b.address(),
                &denom_b.with_amount(send_amount + send_amount).as_ref(),
            )?;

            assert_eventually_channel_upgrade_open(
                &chains.handle_a,
                &chains.handle_b,
                &channels.channel_id_a.as_ref(),
                &channels.port_a.as_ref(),
                &upgraded_attrs,
            )?;

            Ok(())
        })
    }
}

struct ChannelUpgradeHandshakeFlushPackets;

impl TestOverrides for ChannelUpgradeHandshakeFlushPackets {
    fn modify_genesis_file(&self, genesis: &mut serde_json::Value) -> Result<(), Error> {
        set_max_deposit_period(genesis, MAX_DEPOSIT_PERIOD)?;
        set_voting_period(genesis, VOTING_PERIOD)?;
        Ok(())
    }

    fn modify_relayer_config(&self, config: &mut Config) {
        config.mode.channels.enabled = true;

        config.mode.clients.misbehaviour = false;
    }

    fn should_spawn_supervisor(&self) -> bool {
        false
    }
}

impl BinaryChannelTest for ChannelUpgradeHandshakeFlushPackets {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        _config: &TestConfig,
        relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
        channels: ConnectedChannel<ChainA, ChainB>,
    ) -> Result<(), Error> {
        info!("Check that channels are both in OPEN State");

        assert_eventually_channel_established(
            &chains.handle_b,
            &chains.handle_a,
            &channels.channel_id_b.as_ref(),
            &channels.port_b.as_ref(),
        )?;

        let channel_end_a = chains
            .handle_a
            .query_channel(
                QueryChannelRequest {
                    port_id: channels.port_a.0.clone(),
                    channel_id: channels.channel_id_a.0.clone(),
                    height: QueryHeight::Latest,
                },
                IncludeProof::No,
            )
            .map(|(channel_end, _)| channel_end)
            .map_err(|e| eyre!("Error querying ChannelEnd A: {e}"))?;

        let channel_end_b = chains
            .handle_b
            .query_channel(
                QueryChannelRequest {
                    port_id: channels.port_b.0.clone(),
                    channel_id: channels.channel_id_b.0.clone(),
                    height: QueryHeight::Latest,
                },
                IncludeProof::No,
            )
            .map(|(channel_end, _)| channel_end)
            .map_err(|e| eyre!("Error querying ChannelEnd B: {e}"))?;

        let old_version = channel_end_a.version;
        let old_ordering = channel_end_a.ordering;
        let old_connection_hops_a = channel_end_a.connection_hops;
        let old_connection_hops_b = channel_end_b.connection_hops;

        let new_version = Version::ics20_with_fee(1);

        let old_attrs = ChannelUpgradableAttributes::new(
            old_version.clone(),
            old_version.clone(),
            old_ordering,
            old_connection_hops_a.clone(),
            old_connection_hops_b.clone(),
            Sequence::from(1),
        );

        let upgraded_attrs = ChannelUpgradableAttributes::new(
            new_version.clone(),
            new_version.clone(),
            old_ordering,
            old_connection_hops_a.clone(),
            old_connection_hops_b,
            Sequence::from(1),
        );

        info!("Will initialise upgrade handshake with governance proposal...");

        chains.node_a.chain_driver().initialise_channel_upgrade(
            channels.channel.src_port_id().as_str(),
            channels.channel.src_channel_id().unwrap().as_str(),
            old_ordering.as_str(),
            old_connection_hops_a.first().unwrap().as_str(),
            &serde_json::to_string(&new_version.0).unwrap(),
            chains.handle_a().get_signer().unwrap().as_ref(),
            "1",
        )?;

        info!("Check that the step ChanUpgradeInit was correctly executed...");

        assert_eventually_channel_upgrade_init(
            &chains.handle_a,
            &chains.handle_b,
            &channels.channel_id_a.as_ref(),
            &channels.port_a.as_ref(),
            &old_attrs,
        )?;

        // send a IBC transfer message from chain a to chain b
        // so that we have an in-flight packet and chain a
        // will move to `FLUSHING` during Ack
        let denom_a = chains.node_a.denom();
        let wallet_a = chains.node_a.wallets().user1().cloned();
        let wallet_b = chains.node_b.wallets().user1().cloned();
        let a_to_b_amount = random_u128_range(1000, 5000);

        info!(
            "Sending IBC transfer from chain {} to chain {} with amount of {} {}",
            chains.chain_id_a(),
            chains.chain_id_b(),
            a_to_b_amount,
            denom_a
        );

        chains.node_a.chain_driver().ibc_transfer_token(
            &channels,
            &wallet_a.as_ref(),
            &wallet_b.address(),
            &vec![denom_a.with_amount(a_to_b_amount).as_ref()],
        )?;

        // send a IBC transfer message from chain b to chain a
        // so that we have an in-flight packet and chain a
        // will move to `FLUSHING` during Try
        let denom_b = chains.node_b.denom();
        let b_to_a_amount = random_u128_range(1000, 5000);

        info!(
            "Sending IBC transfer from chain {} to chain {} with amount of {} {}",
            chains.chain_id_b(),
            chains.chain_id_a(),
            b_to_a_amount,
            denom_b
        );

        chains.node_b.chain_driver().ibc_transfer_token(
            &channels.clone().flip(),
            &wallet_b.as_ref(),
            &wallet_a.address(),
            &vec![denom_b.with_amount(b_to_a_amount).as_ref()],
        )?;

        info!("Will run ChanUpgradeTry step...");

        channels.channel.build_chan_upgrade_try_and_send()?;

        info!("Check that the step ChanUpgradeTry was correctly executed...");

        assert_eventually_channel_upgrade_try(
            &chains.handle_b,
            &chains.handle_a,
            &channels.channel_id_b.as_ref(),
            &channels.port_b.as_ref(),
            &old_attrs.flipped(),
        )?;

        info!("Will run ChanUpgradeAck step...");

        channels
            .channel
            .flipped()
            .build_chan_upgrade_ack_and_send()?;

        info!("Check that the step ChanUpgradeAck was correctly executed...");

        // channel a is `FLUSHING` because the packet
        // from a to b has not been cleared yet
        assert_eventually_channel_upgrade_ack(
            &chains.handle_a,
            &chains.handle_b,
            &channels.channel_id_a.as_ref(),
            &channels.port_a.as_ref(),
            ChannelState::Flushing,
            ChannelState::Flushing,
            &old_attrs,
        )?;

        info!("Check that the channel upgrade successfully upgraded the version...");

        // start supervisor to clear in-flight packets
        // and move channel ends to `FLUSH_COMPLETE`
        relayer.with_supervisor(|| {
            let ibc_denom_a = derive_ibc_denom(
                &channels.port_a.as_ref(),
                &channels.channel_id_a.as_ref(),
                &denom_b,
            )?;

            chains.node_a.chain_driver().assert_eventual_wallet_amount(
                &wallet_a.address(),
                &ibc_denom_a.with_amount(b_to_a_amount).as_ref(),
            )?;

            let ibc_denom_b = derive_ibc_denom(
                &channels.port_b.as_ref(),
                &channels.channel_id_b.as_ref(),
                &denom_a,
            )?;

            chains.node_b.chain_driver().assert_eventual_wallet_amount(
                &wallet_b.address(),
                &ibc_denom_b.with_amount(a_to_b_amount).as_ref(),
            )?;

            // This will assert that both channel ends are eventually
            // in Open state, and that the fields targeted by the upgrade
            // have been correctly updated.
            assert_eventually_channel_upgrade_open(
                &chains.handle_a,
                &chains.handle_b,
                &channels.channel_id_a.as_ref(),
                &channels.port_a.as_ref(),
                &upgraded_attrs,
            )?;

            Ok(())
        })
    }
}
