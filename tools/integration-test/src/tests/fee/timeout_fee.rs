//! This test ensures that a `timeout_fee` is correctly paid to a relayer in
//! the case that it relays a timeout packet when an `ibc_token_transfer_with_fee`
//! operation times out.

use ibc_relayer_types::core::ics04_channel::version::Version;
use ibc_test_framework::prelude::*;
use ibc_test_framework::util::random::random_u128_range;
use std::thread;

#[test]
fn test_timeout_fee() -> Result<(), Error> {
    run_binary_channel_test(&TimeoutFeeTest)
}

struct TimeoutFeeTest;

impl TestOverrides for TimeoutFeeTest {
    fn should_spawn_supervisor(&self) -> bool {
        false
    }

    fn channel_version(&self) -> Version {
        Version::ics20_with_fee(1)
    }
}

impl BinaryChannelTest for TimeoutFeeTest {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        _config: &TestConfig,
        relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
        channel: ConnectedChannel<ChainA, ChainB>,
    ) -> Result<(), Error> {
        let chain_driver_a = chains.node_a.chain_driver();

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

        chain_driver_a.ibc_token_transfer_with_fee(
            &channel,
            &user_a,
            &user_b.address(),
            &vec![denom_a.with_amount(send_amount).as_ref()],
            &denom_a.with_amount(receive_fee).as_ref(),
            &denom_a.with_amount(ack_fee).as_ref(),
            &denom_a.with_amount(timeout_fee).as_ref(),
            Duration::from_secs(5),
        )?;

        // Sleep to wait for IBC packet to timeout before start relaying
        thread::sleep(Duration::from_secs(6));

        relayer.with_supervisor(|| {
            chain_driver_a.assert_eventual_wallet_amount(
                &user_a.address(),
                &(balance_a2 + send_amount + receive_fee + ack_fee).as_ref(),
            )?;

            chain_driver_a.assert_eventual_wallet_amount(
                &relayer_a.address(),
                &(relayer_balance_a + timeout_fee).as_ref(),
            )?;

            Ok(())
        })
    }
}
