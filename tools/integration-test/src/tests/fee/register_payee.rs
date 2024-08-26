//! This test ensures the correctness of the fee module in the case when the
//! payee is not the default.
//!
//! `register_payee` allows the user to specify the payee that receives the `ack_fee`.
//! In the default case, when only `register_counterparty_payee` is called, then
//! both the `recv_fee` and `ack_fee` will be paid to the counterparty payee.
//! If `register_payee` is called as well, then the payee will be paid the
//! `ack_fee` and the counterparty payee will be paid the `recv_fee`.
//!
//! One possible usage of this would be in the case when the forward and reverse
//! relayer are the same, but the relayer wishes to store the `recv_fee` and
//! `ack_fee` in separate wallets.

use ibc_relayer_types::core::ics04_channel::version::Version;
use ibc_test_framework::prelude::*;
use ibc_test_framework::util::random::random_u128_range;

#[test]
fn test_register_payee() -> Result<(), Error> {
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

        let port_a = channel.port_a.as_ref();
        let port_b = channel.port_b.as_ref();
        let channel_id_a = channel.channel_id_a.as_ref();

        let channel_id_b = channel.channel_id_b.as_ref();

        let wallets_a = chains.node_a.wallets();
        let wallets_b = chains.node_b.wallets();

        let relayer_a = wallets_a.relayer();
        let relayer_b = wallets_b.relayer();

        let payee_a = wallets_a.user2();

        info!(
            "registering payee address of relayer {} on chain {} to be {}",
            relayer_a.address(),
            chain_id_a,
            payee_a.address(),
        );

        chain_driver_a.register_payee(&relayer_a, &payee_a.address(), &channel_id_a, &port_a)?;

        {
            let counterparty_payee =
                chain_driver_b.query_counterparty_payee(&channel_id_b, &relayer_b.address())?;

            assert_eq(
                "counterparty address should be None before registering",
                &counterparty_payee,
                &None,
            )?;
        }

        info!(
            "registering counterparty address of relayer {} on chain {} to be {} on chain {}",
            relayer_b.address(),
            chain_id_b,
            relayer_a.address(),
            chain_id_a
        );

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
        let payee_balance_a = chain_driver_a.query_balance(&payee_a.address(), &denom_a)?;

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
            &payee_a.address(),
            &(payee_balance_a + ack_fee).as_ref(),
        )?;

        chain_driver_a.assert_eventual_wallet_amount(
            &relayer_a.address(),
            &(relayer_balance_a + receive_fee).as_ref(),
        )?;

        Ok(())
    }
}
