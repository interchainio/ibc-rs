//! Tests the relayer's support of fee transfer functionality.
//!
//! This test performs an `ibc_token_transfer_with_fee` between `chain_driver_a`
//! (the source chain) and `chain_driver_b` (the destination chain). Before it
//! can perform this operation, it first checks that no `counterparty_payee` has
//! been registered, and then registers `relayer_a`'s address as the
//! `counterparty_payee` so that the fees will be sent to it. After the transfer,
//! it checks to make sure that all the appropriate fees were paid out to the
//! correct parties involved in the transaction.

use ibc_relayer_types::core::ics04_channel::version::Version;
use ibc_test_framework::prelude::*;
use ibc_test_framework::util::random::random_u128_range;

#[test]
fn test_forward_relayer() -> Result<(), Error> {
    run_binary_channel_test(&ForwardRelayerTest)
}

struct ForwardRelayerTest;

impl TestOverrides for ForwardRelayerTest {
    fn channel_version(&self) -> Version {
        Version::ics20_with_fee(1)
    }
}

impl BinaryChannelTest for ForwardRelayerTest {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        _config: &TestConfig,
        _relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
        channel: ConnectedChannel<ChainA, ChainB>,
    ) -> Result<(), Error> {
        let chain_driver_a = chains.node_a.chain_driver();
        let chain_driver_b = chains.node_b.chain_driver();

        let chain_id_a = chain_driver_a.chain_id();
        let chain_id_b = chain_driver_b.chain_id();

        let denom_a = chains.node_a.denom();

        let port_b = channel.port_b.as_ref();

        let channel_id_b = channel.channel_id_b.as_ref();

        let wallets_a = chains.node_a.wallets();
        let wallets_b = chains.node_b.wallets();

        let relayer_a = wallets_a.relayer();
        let relayer_b = wallets_b.relayer();

        info!(
            "registering counterparty address of relayer {} on chain {} to be {} on chain {}",
            relayer_b.address(),
            chain_id_b,
            relayer_a.address(),
            chain_id_a
        );

        {
            let counterparty_payee =
                chain_driver_b.query_counterparty_payee(&channel_id_b, &relayer_b.address())?;

            assert_eq(
                "counterparty address should be None before registering",
                &counterparty_payee,
                &None,
            )?;
        }

        chain_driver_b.register_counterparty_payee(
            &relayer_b,
            &relayer_a.address(),
            &channel_id_b,
            &port_b,
        )?;

        {
            let counterparty_payee =
                chain_driver_b.query_counterparty_payee(&channel_id_b, &relayer_b.address())?;

            assert_eq(
                "counterparty address should match registered address",
                &counterparty_payee,
                &Some(relayer_a.address().cloned()),
            )?;
        }

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

        chain_driver_a.assert_eventual_wallet_amount(
            &user_a.address(),
            &(balance_a - send_amount - receive_fee - ack_fee).as_ref(),
        )?;

        chain_driver_a.assert_eventual_wallet_amount(
            &relayer_a.address(),
            &(relayer_balance_a + ack_fee + receive_fee).as_ref(),
        )?;

        Ok(())
    }
}
