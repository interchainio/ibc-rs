//! Tests channel upgrade:
//!
//! - `ChannelUpgradeTimeoutAckHandshake` tests that the channel worker will timeout the
//!   upgrade handshake if too much time passes before relaying the Upgrade Ack.
//!
//! - `ChannelUpgradeTimeoutConfirmHandshake` tests that the channel worker will timeout the
//!   upgrade handshake if too much time passes before relaying the Upgrade Confirm.
//!
//! - `ChannelUpgradeManualTimeoutWhenFlushingHandshake` tests that the channel upgrade can be timed out
//!   and cancelled if the packets take too much time to be flushed.
//!
//! - `ChannelUpgradeHandshakeTimeoutWhenFlushing` tests that the channel worker will timeout the
//!   upgrade handshake if the counterparty does not finish flushing the packets before the upgrade timeout.
//!
//! - `ChannelUpgradeHandshakeTimeoutOnAck` tests that the channel worker will cancel the
//!   upgrade handshake if the Ack step fails due to an upgrade timeout.
//!
//! - `ChannelUpgradeHandshakeTimeoutOnPacketAck` tests that the channel worker will cancel the
//!   upgrade handshake if the chain acknowledges a packet after the upgrade timeout expired.
//!
//!
//!
use std::thread::sleep;

use ibc_relayer::chain::requests::{IncludeProof, QueryChannelRequest, QueryHeight};
use ibc_relayer_types::core::ics04_channel::channel::State as ChannelState;
use ibc_relayer_types::core::ics04_channel::packet::Sequence;
use ibc_relayer_types::core::ics04_channel::version::Version;
use ibc_relayer_types::events::IbcEventType;
use ibc_test_framework::chain::config::{set_max_deposit_period, set_voting_period};
use ibc_test_framework::prelude::*;
use ibc_test_framework::relayer::channel::{
    assert_eventually_channel_established, assert_eventually_channel_upgrade_ack,
    assert_eventually_channel_upgrade_cancel, assert_eventually_channel_upgrade_flushing,
    assert_eventually_channel_upgrade_init, assert_eventually_channel_upgrade_try,
    ChannelUpgradableAttributes,
};

#[test]
fn test_channel_upgrade_timeout_ack_handshake() -> Result<(), Error> {
    run_binary_channel_test(&ChannelUpgradeTimeoutAckHandshake)
}

#[test]
fn test_channel_upgrade_timeout_confirm_handshake() -> Result<(), Error> {
    run_binary_channel_test(&ChannelUpgradeTimeoutConfirmHandshake)
}

#[test]
fn test_channel_upgrade_manual_timeout_when_flushing() -> Result<(), Error> {
    run_binary_channel_test(&ChannelUpgradeManualTimeoutWhenFlushing)
}

#[test]
fn test_channel_upgrade_handshake_timeout_when_flushing() -> Result<(), Error> {
    run_binary_channel_test(&ChannelUpgradeHandshakeTimeoutWhenFlushing)
}

#[test]
fn test_channel_upgrade_handshake_timeout_on_ack() -> Result<(), Error> {
    run_binary_channel_test(&ChannelUpgradeHandshakeTimeoutOnAck)
}

#[test]
fn test_channel_upgrade_handshake_timeout_on_packet_ack() -> Result<(), Error> {
    run_binary_channel_test(&ChannelUpgradeHandshakeTimeoutOnPacketAck)
}

const MAX_DEPOSIT_PERIOD: &str = "10s";
const VOTING_PERIOD: u64 = 10;

struct ChannelUpgradeTestOverrides;

impl TestOverrides for ChannelUpgradeTestOverrides {
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

struct ChannelUpgradeTimeoutAckHandshake;

impl BinaryChannelTest for ChannelUpgradeTimeoutAckHandshake {
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

        let channel = channels.channel;
        let new_version = Version::ics20_with_fee(1);

        let old_attrs = ChannelUpgradableAttributes::new(
            old_version.clone(),
            old_version.clone(),
            old_ordering,
            old_connection_hops_a.clone(),
            old_connection_hops_b.clone(),
            Sequence::from(1),
        );

        info!("Will update channel params to set a short upgrade timeout...");

        chains.node_b.chain_driver().update_channel_params(
            5000000000,
            chains.handle_b().get_signer().unwrap().as_ref(),
            "1",
        )?;

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

        info!("Will run ChanUpgradeTry step...");

        channel.build_chan_upgrade_try_and_send()?;

        info!("Check that the step ChanUpgradeTry was correctly executed...");

        assert_eventually_channel_upgrade_try(
            &chains.handle_b,
            &chains.handle_a,
            &channels.channel_id_b.as_ref(),
            &channels.port_b.as_ref(),
            &old_attrs.flipped(),
        )?;

        std::thread::sleep(Duration::from_secs(10));

        info!("Check that the channel upgrade was successfully cancelled...");

        // This will assert that both channel ends are eventually
        // in Open state, and that the fields have not changed.
        relayer.with_supervisor(|| {
            assert_eventually_channel_upgrade_cancel(
                &chains.handle_a,
                &chains.handle_b,
                &channels.channel_id_a.as_ref(),
                &channels.port_a.as_ref(),
                &old_attrs,
            )?;

            Ok(())
        })
    }
}

struct ChannelUpgradeTimeoutConfirmHandshake;

impl BinaryChannelTest for ChannelUpgradeTimeoutConfirmHandshake {
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

        let channel = channels.channel;
        let new_version = Version::ics20_with_fee(1);

        let old_attrs = ChannelUpgradableAttributes::new(
            old_version.clone(),
            old_version.clone(),
            old_ordering,
            old_connection_hops_a.clone(),
            old_connection_hops_b.clone(),
            Sequence::from(1),
        );

        info!("Will update channel params to set a short upgrade timeout...");

        chains.node_a.chain_driver().update_channel_params(
            5000000000,
            chains.handle_a().get_signer().unwrap().as_ref(),
            "1",
        )?;

        info!("Will initialise upgrade handshake with governance proposal...");

        chains.node_a.chain_driver().initialise_channel_upgrade(
            channel.src_port_id().as_str(),
            channel.src_channel_id().unwrap().as_str(),
            old_ordering.as_str(),
            old_connection_hops_a.first().unwrap().as_str(),
            &serde_json::to_string(&new_version.0).unwrap(),
            chains.handle_a().get_signer().unwrap().as_ref(),
            "2",
        )?;

        info!("Will run ChanUpgradeTry step...");

        channel.build_chan_upgrade_try_and_send()?;

        info!("Check that the step ChanUpgradeTry was correctly executed...");

        assert_eventually_channel_upgrade_try(
            &chains.handle_b,
            &chains.handle_a,
            &channels.channel_id_b.as_ref(),
            &channels.port_b.as_ref(),
            &old_attrs.flipped(),
        )?;

        info!("Will run ChanUpgradeAck step...");

        channel.flipped().build_chan_upgrade_ack_and_send()?;

        info!("Check that the step ChanUpgradeAck was correctly executed...");

        assert_eventually_channel_upgrade_ack(
            &chains.handle_a,
            &chains.handle_b,
            &channels.channel_id_a.as_ref(),
            &channels.port_a.as_ref(),
            ChannelState::FlushComplete,
            ChannelState::Flushing,
            &old_attrs,
        )?;

        std::thread::sleep(Duration::from_secs(10));

        info!("Check that the channel upgrade was successfully cancelled...");

        // This will assert that both channel ends are eventually
        // in Open state, and that the fields have not changed.
        relayer.with_supervisor(|| {
            assert_eventually_channel_upgrade_cancel(
                &chains.handle_a,
                &chains.handle_b,
                &channels.channel_id_a.as_ref(),
                &channels.port_a.as_ref(),
                &old_attrs,
            )?;

            Ok(())
        })
    }
}

struct ChannelUpgradeHandshakeTimeoutWhenFlushing;

impl BinaryChannelTest for ChannelUpgradeHandshakeTimeoutWhenFlushing {
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

        info!("Will update channel params to set a shorter upgrade timeout...");

        // the upgrade timeout should be long enough for chain a
        // to complete Ack successfully so that it goes into `FLUSHING`
        chains.node_b.chain_driver().update_channel_params(
            25000000000,
            chains.handle_b().get_signer().unwrap().as_ref(),
            "1",
        )?;

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

        // send a IBC transfer message from chain a to chain b
        // so that we have an in-flight packet and chain a
        // will move to `FLUSHING` during Ack
        let denom_a = chains.node_a.denom();
        let wallet_a = chains.node_a.wallets().user1().cloned();
        let wallet_b = chains.node_b.wallets().user1().cloned();
        let a_to_b_amount = 12345u64;

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

        info!("Will run ChanUpgradeAck step...");

        channels
            .channel
            .flipped()
            .build_chan_upgrade_ack_and_send()?;

        info!("Check that the step ChanUpgradeAck was correctly executed...");

        assert_eventually_channel_upgrade_flushing(
            &chains.handle_a,
            &chains.handle_b,
            &channels.channel_id_a.as_ref(),
            &channels.port_a.as_ref(),
            &old_attrs,
        )?;

        // wait enough time so that timeout expires while chain a is in FLUSHING
        sleep(Duration::from_nanos(35000000000));

        info!("Will run ChanUpgradeTimeout step...");

        // Since the chain a has not moved to `FLUSH_COMPLETE` before the upgrade timeout
        // expired, then we can submit `MsgChannelUpgradeTimeout` on chain b
        // to cancel the upgrade and move the channel back to `OPEN`
        let timeout_event = channels.channel.build_chan_upgrade_timeout_and_send()?;
        assert_eq!(
            timeout_event.event_type(),
            IbcEventType::UpgradeTimeoutChannel
        );

        relayer.with_supervisor(|| {
            info!("Check that the step ChanUpgradeTimeout was correctly executed...");

            assert_eventually_channel_upgrade_cancel(
                &chains.handle_b,
                &chains.handle_a,
                &channels.channel_id_b.as_ref(),
                &channels.port_b.as_ref(),
                &old_attrs.flipped(),
            )?;

            Ok(())
        })
    }
}

struct ChannelUpgradeManualTimeoutWhenFlushing;

impl BinaryChannelTest for ChannelUpgradeManualTimeoutWhenFlushing {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        _config: &TestConfig,
        _relayer: RelayerDriver,
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

        info!("Will update channel params to set a shorter upgrade timeout...");

        // the upgrade timeout should be long enough for chain a
        // to complete Ack successfully so that it goes into `FLUSHING`
        chains.node_b.chain_driver().update_channel_params(
            25000000000,
            chains.handle_b().get_signer().unwrap().as_ref(),
            "1",
        )?;

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

        // send a IBC transfer message from chain a to chain b
        // so that we have an in-flight packet and chain a
        // will move to `FLUSHING` during Ack
        let denom_a = chains.node_a.denom();
        let wallet_a = chains.node_a.wallets().user1().cloned();
        let wallet_b = chains.node_b.wallets().user1().cloned();
        let a_to_b_amount = 12345u128;

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

        info!("Will run ChanUpgradeAck step...");

        channels
            .channel
            .flipped()
            .build_chan_upgrade_ack_and_send()?;

        info!("Check that the step ChanUpgradeAck was correctly executed...");

        assert_eventually_channel_upgrade_flushing(
            &chains.handle_a,
            &chains.handle_b,
            &channels.channel_id_a.as_ref(),
            &channels.port_a.as_ref(),
            &old_attrs,
        )?;

        // wait enough time so that timeout expires while chain a is in FLUSHING
        sleep(Duration::from_nanos(35000000000));

        info!("Will run ChanUpgradeTimeout step...");

        // Since the chain a has not moved to `FLUSH_COMPLETE` before the upgrade timeout
        // expired, then we can submit `MsgChannelUpgradeTimeout` on chain b
        // to cancel the upgrade and move the channel back to `OPEN`
        let timeout_event = channels.channel.build_chan_upgrade_timeout_and_send()?;
        assert_eq!(
            timeout_event.event_type(),
            IbcEventType::UpgradeTimeoutChannel
        );

        let cancel_event = channels
            .channel
            .flipped()
            .build_chan_upgrade_cancel_and_send()?;
        assert_eq!(
            cancel_event.event_type(),
            IbcEventType::UpgradeCancelChannel
        );

        info!("Check that the step ChanUpgradeTimeout was correctly executed...");

        assert_eventually_channel_upgrade_cancel(
            &chains.handle_b,
            &chains.handle_a,
            &channels.channel_id_b.as_ref(),
            &channels.port_b.as_ref(),
            &old_attrs.flipped(),
        )?;

        Ok(())
    }
}

struct ChannelUpgradeHandshakeTimeoutOnAck;

impl BinaryChannelTest for ChannelUpgradeHandshakeTimeoutOnAck {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        _config: &TestConfig,
        _relayer: RelayerDriver,
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

        let channel = channels.channel;
        let new_version = Version::ics20_with_fee(1);

        let old_attrs = ChannelUpgradableAttributes::new(
            old_version.clone(),
            old_version.clone(),
            old_ordering,
            old_connection_hops_a.clone(),
            old_connection_hops_b.clone(),
            Sequence::from(1),
        );

        info!("Will update channel params to set a short upgrade timeout...");

        chains.node_b.chain_driver().update_channel_params(
            5000000000,
            chains.handle_b().get_signer().unwrap().as_ref(),
            "1",
        )?;

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

        info!("Check that the step ChanUpgradeInit was correctly executed...");

        assert_eventually_channel_upgrade_init(
            &chains.handle_a,
            &chains.handle_b,
            &channels.channel_id_a.as_ref(),
            &channels.port_a.as_ref(),
            &old_attrs,
        )?;

        info!("Will run ChanUpgradeTry step...");

        channel.build_chan_upgrade_try_and_send()?;

        info!("Check that the step ChanUpgradeTry was correctly executed...");

        assert_eventually_channel_upgrade_try(
            &chains.handle_b,
            &chains.handle_a,
            &channels.channel_id_b.as_ref(),
            &channels.port_b.as_ref(),
            &old_attrs.flipped(),
        )?;

        // wait enough time so that ACK fails due to upgrade timeout
        sleep(Duration::from_secs(10));

        info!("Will run ChanUpgradeAck step...");

        let ack_event = channel.flipped().build_chan_upgrade_ack_and_send()?;

        info!("Check that the step ChanUpgradeAck timed out...");

        // ACK should fail because the upgrade has timed out
        assert_eq!(ack_event.event_type(), IbcEventType::UpgradeErrorChannel);

        info!("Will run ChanUpgradeCancel step...");

        // Since the following assertion checks that the fields of channel ends
        // have not been updated, asserting there is a `UpgradeCancelChannel` event
        // avoids having a passing test due to the Upgrade Init step failing
        let cancel_event = channel.build_chan_upgrade_cancel_and_send()?;
        assert_eq!(
            cancel_event.event_type(),
            IbcEventType::UpgradeCancelChannel
        );

        info!("Check that the step ChanUpgradeCancel was correctly executed...");

        assert_eventually_channel_upgrade_cancel(
            &chains.handle_b,
            &chains.handle_a,
            &channels.channel_id_b.as_ref(),
            &channels.port_b.as_ref(),
            &old_attrs.flipped(),
        )?;

        Ok(())
    }
}

struct ChannelUpgradeHandshakeTimeoutOnPacketAck;

impl BinaryChannelTest for ChannelUpgradeHandshakeTimeoutOnPacketAck {
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

        info!("Will update channel params to set a short upgrade timeout...");

        // the upgrade timeout should be long enough for chain a
        // to complete Ack successfully so that it goes into `FLUSHING`
        chains.node_b.chain_driver().update_channel_params(
            80000000000,
            chains.handle_b().get_signer().unwrap().as_ref(),
            "1",
        )?;

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
        let a_to_b_amount = 12345u128;

        info!(
            "Sending IBC transfer from chain {} to chain {} with amount of {} {}",
            chains.chain_id_a(),
            chains.chain_id_b(),
            a_to_b_amount,
            denom_a
        );

        chains
            .node_a
            .chain_driver()
            .ibc_transfer_token_with_memo_and_timeout(
                &channels,
                &wallet_a.as_ref(),
                &wallet_b.address(),
                &vec![denom_a.with_amount(a_to_b_amount).as_ref()],
                None,
                Some(Duration::from_secs(600)),
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

        // wait enough time so that timeout expires while chain a is in FLUSHING
        // and when packet lifecycle completes with acknowledge packet on chain a
        // it will abort the upgrade
        sleep(Duration::from_nanos(80000000000));

        info!("Check that the channel upgrade aborted...");

        // start supervisor to clear in-flight packets
        // and move channel ends to `FLUSH_COMPLETE`
        relayer.with_supervisor(|| {
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
            // have NOT been correctly updated, because chain a aborted the upgrade
            assert_eventually_channel_upgrade_cancel(
                &chains.handle_a,
                &chains.handle_b,
                &channels.channel_id_a.as_ref(),
                &channels.port_a.as_ref(),
                &old_attrs,
            )?;

            Ok(())
        })
    }
}

impl HasOverrides for ChannelUpgradeTimeoutAckHandshake {
    type Overrides = ChannelUpgradeTestOverrides;

    fn get_overrides(&self) -> &ChannelUpgradeTestOverrides {
        &ChannelUpgradeTestOverrides
    }
}

impl HasOverrides for ChannelUpgradeTimeoutConfirmHandshake {
    type Overrides = ChannelUpgradeTestOverrides;

    fn get_overrides(&self) -> &ChannelUpgradeTestOverrides {
        &ChannelUpgradeTestOverrides
    }
}

impl HasOverrides for ChannelUpgradeManualTimeoutWhenFlushing {
    type Overrides = ChannelUpgradeTestOverrides;

    fn get_overrides(&self) -> &ChannelUpgradeTestOverrides {
        &ChannelUpgradeTestOverrides
    }
}

impl HasOverrides for ChannelUpgradeHandshakeTimeoutWhenFlushing {
    type Overrides = ChannelUpgradeTestOverrides;

    fn get_overrides(&self) -> &ChannelUpgradeTestOverrides {
        &ChannelUpgradeTestOverrides
    }
}

impl HasOverrides for ChannelUpgradeHandshakeTimeoutOnAck {
    type Overrides = ChannelUpgradeTestOverrides;

    fn get_overrides(&self) -> &ChannelUpgradeTestOverrides {
        &ChannelUpgradeTestOverrides
    }
}

impl HasOverrides for ChannelUpgradeHandshakeTimeoutOnPacketAck {
    type Overrides = ChannelUpgradeTestOverrides;

    fn get_overrides(&self) -> &ChannelUpgradeTestOverrides {
        &ChannelUpgradeTestOverrides
    }
}
