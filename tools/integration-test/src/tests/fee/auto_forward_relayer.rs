//! Tests the relayer's support for the `auto_register_counterparty_payee`
//! configuration option. When this option is toggled on, the relayer will
//! discover the address of the counterparty payee automatically without the
//! user needing to provide it.
//!
//! The test enables this option and then performs an `ibc_token_transfer_with_fee`
//! between `chain_driver_a` (the source chain) and `chain_driver_b` (the
//! destination chain) without explicitly registering the counterparty's address.
//! It then checks to make sure all the appropriate fees are paid out to the
//! correct parties involved in the transaction.

use ibc_relayer_types::core::ics04_channel::version::Version;
use ibc_test_framework::prelude::*;
use ibc_test_framework::util::random::random_u128_range;

#[test]
fn test_auto_forward_relayer() -> Result<(), Error> {
    run_binary_channel_test(&AutoForwardRelayerTest)
}

struct AutoForwardRelayerTest;

impl TestOverrides for AutoForwardRelayerTest {
    fn modify_relayer_config(&self, config: &mut Config) {
        config.mode.packets.auto_register_counterparty_payee = true;
    }

    fn channel_version(&self) -> Version {
        Version::ics20_with_fee(1)
    }
}

impl BinaryChannelTest for AutoForwardRelayerTest {
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

        info!("Will assert user b received the transferred token");

        chain_driver_b.assert_eventual_wallet_amount(
            &user_b.address(),
            &denom_b.with_amount(send_amount).as_ref(),
        )?;

        info!("Will assert user a transferred the sent amount, the recv fee and ack fee");

        chain_driver_a.assert_eventual_wallet_amount(
            &user_a.address(),
            &(balance_a - send_amount - receive_fee - ack_fee).as_ref(),
        )?;

        info!("Will assert the relayer received the recv fee and ack fee");

        chain_driver_a.assert_eventual_wallet_amount(
            &relayer_a.address(),
            &(relayer_balance_a + ack_fee + receive_fee).as_ref(),
        )?;

        Ok(())
    }
}
