//! This test tests two different cases:
//!
//! - The `NoForwardRelayerTest` tests the case where the `counterparty_payee`'s
//!   address is not registered.
//!
//! - The `InvalidForwardRelayerTest` tests the case where the
//!   `counterparty_payee` address is invalidly registered.
//!
//! In both tests, the `auto_register_counterparty_payee` option is toggled off.
//! The tests then check that the `receive_fee` and the `timeout_fee` are
//! refunded to the payer.

use ibc_relayer_types::core::ics04_channel::version::Version;
use ibc_test_framework::prelude::*;
use ibc_test_framework::util::random::random_u128_range;

#[test]
fn test_no_forward_relayer() -> Result<(), Error> {
    run_binary_channel_test(&NoForwardRelayerTest)
}

#[test]
fn test_invalid_forward_relayer() -> Result<(), Error> {
    run_binary_channel_test(&InvalidForwardRelayerTest)
}

struct NoForwardRelayerTest;
struct InvalidForwardRelayerTest;

impl TestOverrides for NoForwardRelayerTest {
    fn modify_relayer_config(&self, config: &mut Config) {
        config.mode.packets.auto_register_counterparty_payee = false;
    }

    fn channel_version(&self) -> Version {
        Version::ics20_with_fee(1)
    }
}

impl TestOverrides for InvalidForwardRelayerTest {
    fn modify_relayer_config(&self, config: &mut Config) {
        config.mode.packets.auto_register_counterparty_payee = false;
    }

    fn channel_version(&self) -> Version {
        Version::ics20_with_fee(1)
    }
}

impl BinaryChannelTest for NoForwardRelayerTest {
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

        let balance_a = chain_driver_a.query_balance(&user_a.address(), &denom_a)?;

        let relayer_balance_a = chain_driver_a.query_balance(&relayer_a.address(), &denom_a)?;

        let send_amount = random_u128_range(1000, 2000);
        let receive_fee = random_u128_range(300, 400);
        let ack_fee = random_u128_range(200, 300);
        let timeout_fee = random_u128_range(100, 200);

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

        let denom_b = derive_ibc_denom(
            &channel.port_b.as_ref(),
            &channel.channel_id_b.as_ref(),
            &denom_a,
        )?;

        chain_driver_b.assert_eventual_wallet_amount(
            &user_b.address(),
            &denom_b.with_amount(send_amount).as_ref(),
        )?;

        // receive fee and timeout fee should be refunded,
        // as there is no counterparty address registered.
        chain_driver_a.assert_eventual_wallet_amount(
            &user_a.address(),
            &(balance_a - send_amount - ack_fee).as_ref(),
        )?;

        chain_driver_a.assert_eventual_wallet_amount(
            &relayer_a.address(),
            &(relayer_balance_a + ack_fee).as_ref(),
        )?;

        Ok(())
    }
}

impl BinaryChannelTest for InvalidForwardRelayerTest {
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

        let relayer_a = wallets_a.relayer();
        let relayer_b = wallets_b.relayer();

        let user_a = wallets_a.user1();
        let user_b = wallets_b.user1();

        let balance_a = chain_driver_a.query_balance(&user_a.address(), &denom_a)?;

        let relayer_balance_a = chain_driver_a.query_balance(&relayer_a.address(), &denom_a)?;

        let send_amount = random_u128_range(1000, 2000);
        let receive_fee = random_u128_range(300, 400);
        let ack_fee = random_u128_range(200, 300);
        let timeout_fee = random_u128_range(100, 200);

        let invalid_address =
            MonoTagged::new(WalletAddress("a very long and invalid address".to_string()));

        chain_driver_b.register_counterparty_payee(
            &relayer_b,
            &invalid_address.as_ref(),
            &channel_id_b,
            &port_b,
        )?;

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

        let denom_b = derive_ibc_denom(
            &channel.port_b.as_ref(),
            &channel.channel_id_b.as_ref(),
            &denom_a,
        )?;

        chain_driver_b.assert_eventual_wallet_amount(
            &user_b.address(),
            &denom_b.with_amount(send_amount).as_ref(),
        )?;

        // receive fee and timeout fee should be refunded,
        // as the counterparty address registered is invalid.
        chain_driver_a.assert_eventual_wallet_amount(
            &user_a.address(),
            &(balance_a - send_amount - ack_fee).as_ref(),
        )?;

        chain_driver_a.assert_eventual_wallet_amount(
            &relayer_a.address(),
            &(relayer_balance_a + ack_fee).as_ref(),
        )?;

        Ok(())
    }
}
