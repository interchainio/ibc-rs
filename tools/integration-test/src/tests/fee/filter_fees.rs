#![allow(clippy::mutable_key_type)]

use std::collections::HashMap;

use ibc_relayer::config::filter::{ChannelPolicy, FeePolicy, FilterPattern, MinFee};
use ibc_relayer::config::{ChainConfig, PacketFilter};
use ibc_relayer_types::core::ics04_channel::version::Version;
use ibc_test_framework::prelude::*;
use ibc_test_framework::util::random::random_u128_range;

#[test]
fn test_filter_incentivized_fees_relayer() -> Result<(), Error> {
    run_binary_channel_test(&FilterIncentivizedFeesRelayerTest)
}

#[test]
fn test_filter_by_channel_incentivized_fees_relayer() -> Result<(), Error> {
    run_binary_channel_test(&FilterByChannelIncentivizedFeesRelayerTest)
}

struct FilterIncentivizedFeesRelayerTest;

impl TestOverrides for FilterIncentivizedFeesRelayerTest {
    fn modify_relayer_config(&self, config: &mut Config) {
        config.mode.packets.auto_register_counterparty_payee = true;
        let recv_fee = MinFee::new(50, Some("samoleans".to_owned()));
        let fees_filters = FeePolicy::new(vec![recv_fee]);
        let min_fees =
            HashMap::from([(FilterPattern::Wildcard("*".parse().unwrap()), fees_filters)]);
        let packet_filter = PacketFilter::new(ChannelPolicy::default(), min_fees);
        for chain_config in config.chains.iter_mut() {
            match chain_config {
                ChainConfig::CosmosSdk(chain_config) => {
                    chain_config.packet_filter = packet_filter.clone();
                }
            }
        }
    }

    fn channel_version(&self) -> Version {
        Version::ics20_with_fee(1)
    }
}

impl BinaryChannelTest for FilterIncentivizedFeesRelayerTest {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        _config: &TestConfig,
        _relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
        channel: ConnectedChannel<ChainA, ChainB>,
    ) -> Result<(), Error> {
        let chain_driver_a = chains.node_a.chain_driver();
        let chain_driver_b = chains.node_b.chain_driver();

        let denom_a = chains.node_a.denom();

        let wallets_a = chains.node_a.wallets();
        let wallets_b = chains.node_b.wallets();

        let relayer_a = wallets_a.relayer();

        let user_a = wallets_a.user1();
        let user_b = wallets_b.user1();

        {
            info!("Verify that packet without enough fees is not relayed");

            let balance_a1 = chain_driver_a.query_balance(&user_a.address(), &denom_a)?;
            let relayer_balance_a = chain_driver_a.query_balance(&relayer_a.address(), &denom_a)?;

            let send_amount = random_u128_range(1000, 2000);
            let receive_fee_fail = 49;
            let ack_fee = random_u128_range(200, 300);
            let timeout_fee = random_u128_range(100, 200);

            let balance_a2 = balance_a1.clone() - send_amount;

            chain_driver_a.ibc_token_transfer_with_fee(
                &channel,
                &user_a,
                &user_b.address(),
                &vec![denom_a.with_amount(send_amount).as_ref()],
                &denom_a.with_amount(receive_fee_fail).as_ref(),
                &denom_a.with_amount(ack_fee).as_ref(),
                &denom_a.with_amount(timeout_fee).as_ref(),
                Duration::from_secs(60),
            )?;

            let denom_b = derive_ibc_denom(
                &channel.port_b.as_ref(),
                &channel.channel_id_b.as_ref(),
                &denom_a,
            )?;

            std::thread::sleep(Duration::from_secs(10));

            chain_driver_a.assert_eventual_escrowed_amount_ics29(
                &user_a.address(),
                &balance_a2.as_ref(),
                receive_fee_fail,
                ack_fee,
                timeout_fee,
            )?;

            chain_driver_b.assert_eventual_wallet_amount(
                &user_b.address(),
                &denom_b.with_amount(0u128).as_ref(),
            )?;

            chain_driver_a.assert_eventual_wallet_amount(
                &relayer_a.address(),
                &(relayer_balance_a).as_ref(),
            )?;
        }

        {
            info!("Verify that packet with enough fees is relayed");
            let balance_a = chain_driver_a.query_balance(&user_a.address(), &denom_a)?;
            let relayer_balance_a = chain_driver_a.query_balance(&relayer_a.address(), &denom_a)?;

            let send_amount = random_u128_range(1000, 2000);
            let receive_fee_success = 50u128;
            let ack_fee = random_u128_range(200, 300);
            let timeout_fee = random_u128_range(100, 200);

            chain_driver_a.ibc_token_transfer_with_fee(
                &channel,
                &user_a,
                &user_b.address(),
                &vec![denom_a.with_amount(send_amount).as_ref()],
                &denom_a.with_amount(receive_fee_success).as_ref(),
                &denom_a.with_amount(ack_fee).as_ref(),
                &denom_a.with_amount(timeout_fee).as_ref(),
                Duration::from_secs(60),
            )?;

            let denom_b = derive_ibc_denom(
                &channel.port_b.as_ref(),
                &channel.channel_id_b.as_ref(),
                &denom_a,
            )?;

            chain_driver_b.assert_eventual_wallet_amount(
                &user_b.address(),
                &denom_b.with_amount(send_amount).as_ref(),
            )?;

            chain_driver_a.assert_eventual_wallet_amount(
                &relayer_a.address(),
                &(relayer_balance_a + receive_fee_success + ack_fee).as_ref(),
            )?;

            chain_driver_a.assert_eventual_wallet_amount(
                &user_a.address(),
                &(balance_a - send_amount - receive_fee_success - ack_fee).as_ref(),
            )?;
        }

        Ok(())
    }
}

struct FilterByChannelIncentivizedFeesRelayerTest;

impl TestOverrides for FilterByChannelIncentivizedFeesRelayerTest {
    fn modify_relayer_config(&self, config: &mut Config) {
        config.mode.packets.auto_register_counterparty_payee = true;
        let recv_fee = MinFee::new(50, Some("samoleans".to_owned()));
        let fees_filters = FeePolicy::new(vec![recv_fee]);
        let min_fees = HashMap::from([(
            FilterPattern::Wildcard("other-channel*".parse().unwrap()),
            fees_filters,
        )]);
        let packet_filter = PacketFilter::new(ChannelPolicy::default(), min_fees);
        for chain_config in config.chains.iter_mut() {
            match chain_config {
                ChainConfig::CosmosSdk(chain_config) => {
                    chain_config.packet_filter = packet_filter.clone();
                }
            }
        }
    }

    fn channel_version(&self) -> Version {
        Version::ics20_with_fee(1)
    }
}

impl BinaryChannelTest for FilterByChannelIncentivizedFeesRelayerTest {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        _config: &TestConfig,
        _relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
        channel: ConnectedChannel<ChainA, ChainB>,
    ) -> Result<(), Error> {
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
        let receive_fee = 49;
        let ack_fee = random_u128_range(200, 300);
        let timeout_fee = random_u128_range(100, 200);

        let balance_a2 = balance_a1.clone() - send_amount;

        let denom_b = derive_ibc_denom(
            &channel.port_b.as_ref(),
            &channel.channel_id_b.as_ref(),
            &denom_a,
        )?;

        info!("Verify that packet without enough fees is not relayed");

        chain_driver_a.ibc_token_transfer_with_fee(
            &channel,
            &user_a,
            &user_b.address(),
            &vec![denom_a.with_amount(send_amount).as_ref()],
            &denom_a.with_amount(receive_fee).as_ref(),
            &denom_a.with_amount(ack_fee).as_ref(),
            &denom_a.with_amount(timeout_fee).as_ref(),
            Duration::from_secs(60),
        )?;

        chain_driver_a.assert_eventual_escrowed_amount_ics29(
            &user_a.address(),
            &balance_a2.as_ref(),
            receive_fee,
            ack_fee,
            timeout_fee,
        )?;

        chain_driver_b.assert_eventual_wallet_amount(
            &user_b.address(),
            &denom_b.with_amount(send_amount).as_ref(),
        )?;

        chain_driver_a.assert_eventual_wallet_amount(
            &user_a.address(),
            &(balance_a1 - send_amount - receive_fee - ack_fee).as_ref(),
        )?;

        chain_driver_a.assert_eventual_wallet_amount(
            &relayer_a.address(),
            &(relayer_balance_a + receive_fee + ack_fee).as_ref(),
        )?;

        Ok(())
    }
}
