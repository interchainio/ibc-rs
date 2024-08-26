use time::OffsetDateTime;

use ibc_test_framework::prelude::*;
use ibc_test_framework::util::random::random_u128_range;

const CONNECTION_DELAY: Duration = Duration::from_secs(10);

#[test]
fn test_connection_delay() -> Result<(), Error> {
    run_binary_channel_test(&ConnectionDelayTest)
}

pub struct ConnectionDelayTest;

impl TestOverrides for ConnectionDelayTest {
    fn connection_delay(&self) -> Duration {
        CONNECTION_DELAY
    }
}

impl BinaryChannelTest for ConnectionDelayTest {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        _config: &TestConfig,
        relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
        channel: ConnectedChannel<ChainA, ChainB>,
    ) -> Result<(), Error> {
        relayer.with_supervisor(|| {
            let denom_a = chains.node_a.denom();

            let wallet_a = chains.node_a.wallets().user1().cloned();
            let wallet_b = chains.node_b.wallets().user1().cloned();

            let balance_a = chains
                .node_a
                .chain_driver()
                .query_balance(&wallet_a.address(), &denom_a)?;

            let a_to_b_amount = random_u128_range(1000, 5000);

            info!(
                "Sending IBC transfer from chain {} to chain {} with amount of {} {}",
                chains.chain_id_a(),
                chains.chain_id_b(),
                a_to_b_amount,
                denom_a
            );

            chains.node_a.chain_driver().ibc_transfer_token(
                &channel,
                &wallet_a.as_ref(),
                &wallet_b.address(),
                &vec![denom_a.with_amount(a_to_b_amount).as_ref()],
            )?;

            let time1 = OffsetDateTime::now_utc();

            let denom_b = derive_ibc_denom(
                &channel.port_b.as_ref(),
                &channel.channel_id_b.as_ref(),
                &denom_a,
            )?;

            info!(
                "Waiting for user on chain B to receive IBC transferred amount of {} {}",
                a_to_b_amount, denom_b
            );

            chains.node_a.chain_driver().assert_eventual_wallet_amount(
                &wallet_a.address(),
                &(balance_a - a_to_b_amount).as_ref(),
            )?;

            chains.node_b.chain_driver().assert_eventual_wallet_amount(
                &wallet_b.address(),
                &denom_b.with_amount(a_to_b_amount).as_ref(),
            )?;

            info!(
                "successfully performed IBC transfer from chain {} to chain {}",
                chains.chain_id_a(),
                chains.chain_id_b(),
            );

            let time2 = OffsetDateTime::now_utc();

            assert_gt(
                &format!(
                    "Expect IBC transfer to only be successful after {}s",
                    CONNECTION_DELAY.as_secs()
                ),
                &(time2 - time1).try_into().unwrap(),
                &CONNECTION_DELAY,
            )?;

            Ok(())
        })
    }
}
