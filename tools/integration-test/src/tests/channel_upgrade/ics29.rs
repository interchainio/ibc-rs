//! Tests channel upgrade features:
//!
//! - `ChannelUpgradeICS29` tests that only after the upgrade handshake is completed
//!   and the channel version has been updated to ICS29 can Incentivized packets be
//!   relayed.

use ibc_relayer::chain::requests::{IncludeProof, QueryChannelRequest, QueryHeight};
use ibc_relayer_types::core::ics04_channel::packet::Sequence;
use ibc_relayer_types::core::ics04_channel::version::Version;
use ibc_test_framework::chain::config::{set_max_deposit_period, set_voting_period};
use ibc_test_framework::prelude::*;
use ibc_test_framework::relayer::channel::{
    assert_eventually_channel_established, assert_eventually_channel_upgrade_open,
    ChannelUpgradableAttributes,
};
use ibc_test_framework::util::random::random_u128_range;

#[test]
fn test_channel_upgrade_ics29() -> Result<(), Error> {
    run_binary_channel_test(&ChannelUpgradeICS29)
}

const MAX_DEPOSIT_PERIOD: &str = "10s";
const VOTING_PERIOD: u64 = 10;

pub struct ChannelUpgradeICS29;

impl TestOverrides for ChannelUpgradeICS29 {
    fn modify_relayer_config(&self, config: &mut Config) {
        config.mode.channels.enabled = true;
        config.mode.packets.auto_register_counterparty_payee = true;
        config.mode.packets.clear_interval = 0;
        config.mode.packets.clear_on_start = false;

        config.mode.clients.misbehaviour = false;
    }

    fn modify_genesis_file(&self, genesis: &mut serde_json::Value) -> Result<(), Error> {
        set_max_deposit_period(genesis, MAX_DEPOSIT_PERIOD)?;
        set_voting_period(genesis, VOTING_PERIOD)?;
        Ok(())
    }
}

impl BinaryChannelTest for ChannelUpgradeICS29 {
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

        let chain_driver_a = chains.node_a.chain_driver();
        let chain_driver_b = chains.node_b.chain_driver();

        let denom_a = chains.node_a.denom();

        let wallets_a = chains.node_a.wallets();
        let wallets_b = chains.node_b.wallets();

        let relayer_a = wallets_a.relayer();

        let user_a = wallets_a.user1();
        let user_b = wallets_b.user1();

        let balance_a1 = chain_driver_a.query_balance(&user_a.address(), &denom_a)?;

        let relayer_balance_a = chain_driver_a.query_balance(&relayer_a.address(), &denom_a)?;

        let send_amount = random_u128_range(1000, 2000);
        let receive_fee = random_u128_range(300, 400);
        let ack_fee = random_u128_range(200, 300);
        let timeout_fee = random_u128_range(100, 200);

        let total_sent = send_amount + receive_fee + ack_fee + timeout_fee;

        let balance_a2 = balance_a1 - total_sent;

        let channel = channels.channel.clone();

        let ics29_transfer = chain_driver_a.ibc_token_transfer_with_fee(
            &channels,
            &user_a,
            &user_b.address(),
            &vec![denom_a.with_amount(send_amount).as_ref()],
            &denom_a.with_amount(receive_fee).as_ref(),
            &denom_a.with_amount(ack_fee).as_ref(),
            &denom_a.with_amount(timeout_fee).as_ref(),
            Duration::from_secs(60),
        );

        assert!(ics29_transfer.is_err(), "{ics29_transfer:?}");

        let old_ordering = channel_end_a.ordering;
        let old_connection_hops_a = channel_end_a.connection_hops;
        let old_connection_hops_b = channel_end_b.connection_hops;

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

        info!("Check that the channel upgrade successfully upgraded the version...");

        assert_eventually_channel_upgrade_open(
            &chains.handle_a,
            &chains.handle_b,
            &channels.channel_id_a.as_ref(),
            &channels.port_a.as_ref(),
            &upgraded_attrs,
        )?;

        // Since the channel has been updated to ICS29 version after the Hermes instance
        // was started, the `auto_register_counterparty_payee` has not registered the
        // counterparty payee. It is required to register it manually.
        chain_driver_b.register_counterparty_payee(
            &wallets_b.relayer(),
            &relayer_a.address(),
            &channels.channel_id_b.as_ref(),
            &channels.port_b.as_ref(),
        )?;

        {
            let counterparty_payee = chain_driver_b.query_counterparty_payee(
                &channels.channel_id_b.as_ref(),
                &wallets_b.relayer().address(),
            )?;

            assert_eq(
                "counterparty address should match registered address",
                &counterparty_payee,
                &Some(relayer_a.address().cloned()),
            )?;
        }

        chain_driver_a.ibc_token_transfer_with_fee(
            &channels,
            &user_a,
            &user_b.address(),
            &vec![denom_a.with_amount(send_amount).as_ref()],
            &denom_a.with_amount(receive_fee).as_ref(),
            &denom_a.with_amount(ack_fee).as_ref(),
            &denom_a.with_amount(timeout_fee).as_ref(),
            Duration::from_secs(60),
        )?;

        let denom_b = derive_ibc_denom(
            &channels.port_b.as_ref(),
            &channels.channel_id_b.as_ref(),
            &denom_a,
        )?;

        chain_driver_b.assert_eventual_wallet_amount(
            &user_b.address(),
            &denom_b.with_amount(send_amount).as_ref(),
        )?;

        chain_driver_a.assert_eventual_wallet_amount(
            &user_a.address(),
            &(balance_a2 + timeout_fee).as_ref(),
        )?;

        chain_driver_a.assert_eventual_wallet_amount(
            &relayer_a.address(),
            &(relayer_balance_a + ack_fee + receive_fee).as_ref(),
        )?;

        Ok(())
    }
}
