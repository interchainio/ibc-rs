//! This test ensures that the relayer defaults the `channel_version` to `ics20`,
//! the expected behavior when fees are not enabled, when the fee transaction is
//! not configured correctly.
//!
//! In the case of this test, the calls to `register_counterparty_payee` and
//! `ibc_token_transfer_with_fee` return errors because the `channel_version`
//! override to set the version to `ics20_with_fee` is left out. The test
//! ensures then that the transaction still follows through using
//! `ibc_transfer_token` if the transfer without fees is used.

use ibc_test_framework::prelude::*;
use ibc_test_framework::util::random::random_u128_range;

#[test]
fn test_non_fee_channel() -> Result<(), Error> {
    run_binary_channel_test(&NonFeeChannelTest)
}

struct NonFeeChannelTest;

impl TestOverrides for NonFeeChannelTest {
    fn modify_relayer_config(&self, config: &mut Config) {
        config.mode.packets.auto_register_counterparty_payee = true;
    }
}

impl BinaryChannelTest for NonFeeChannelTest {
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

        let port_b = channel.port_b.as_ref();
        let channel_id_b = channel.channel_id_b.as_ref();

        let wallets_a = chains.node_a.wallets();
        let wallets_b = chains.node_b.wallets();

        let user_a = wallets_a.user1();
        let user_b = wallets_b.user1();

        let relayer_a = wallets_a.relayer();
        let relayer_b = wallets_b.relayer();

        let balance_a1 = chain_driver_a.query_balance(&user_a.address(), &denom_a)?;

        let send_amount = random_u128_range(1000, 2000);

        {
            let res = chain_driver_b.register_counterparty_payee(
                &relayer_b,
                &relayer_a.address(),
                &channel_id_b,
                &port_b,
            );

            assert!(res.is_err());
        }

        {
            let res = chain_driver_a.ibc_token_transfer_with_fee(
                &channel,
                &user_a,
                &user_b.address(),
                &vec![denom_a.with_amount(send_amount).as_ref()],
                &denom_a.with_amount(10u64).as_ref(),
                &denom_a.with_amount(10u64).as_ref(),
                &denom_a.with_amount(10u64).as_ref(),
                Duration::from_secs(60),
            );

            assert!(res.is_err());
        }

        let balance_a2 = balance_a1 - send_amount;

        chain_driver_a.ibc_transfer_token(
            &channel,
            &user_a,
            &user_b.address(),
            &vec![denom_a.with_amount(send_amount).as_ref()],
        )?;

        let denom_b = derive_ibc_denom(
            &channel.port_b.as_ref(),
            &channel.channel_id_b.as_ref(),
            &denom_a,
        )?;

        chain_driver_a.assert_eventual_wallet_amount(&user_a.address(), &balance_a2.as_ref())?;

        chain_driver_b.assert_eventual_wallet_amount(
            &user_b.address(),
            &denom_b.with_amount(send_amount).as_ref(),
        )?;

        Ok(())
    }
}
