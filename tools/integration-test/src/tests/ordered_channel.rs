//! This test asserts that IBC transactions queued to be relayed
//! before the relayer is started are still successfully sent
//! over an ordered channel, even when the `clear_on_start` relayer
//! configuration option is not enabled.
//!
//! The test sends an IBC transfer before the relayer is started,
//! and then starts the relayer. At that point, it sends another
//! IBC transfer. It then checks the recipient wallet to see if
//! its balance reflects both transactions, the one that was sent
//! before the relayer was started, and the one that was sent
//! after the relayer was started.
//!
//! A more thorough walkthrough of this test can be found at
//! `tools/test-framework/src/docs/walkthroughs/ordered_channel.rs`.

use ibc_test_framework::prelude::*;
use ibc_test_framework::util::random::random_u128_range;

#[test]
fn test_ordered_channel() -> Result<(), Error> {
    run_binary_channel_test(&OrderedChannelTest)
}

pub struct OrderedChannelTest;

impl TestOverrides for OrderedChannelTest {
    fn modify_relayer_config(&self, config: &mut Config) {
        // Disabling clear_on_start should make the relayer not
        // relay any packet it missed before starting.
        config.mode.packets.clear_on_start = false;
        config.mode.packets.clear_interval = 0;
    }

    fn should_spawn_supervisor(&self) -> bool {
        false
    }

    fn channel_order(&self) -> Ordering {
        Ordering::Ordered
    }
}

impl BinaryChannelTest for OrderedChannelTest {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        _config: &TestConfig,
        relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
        channel: ConnectedChannel<ChainA, ChainB>,
    ) -> Result<(), Error> {
        let denom_a = chains.node_a.denom();

        let wallet_a = chains.node_a.wallets().user1().cloned();
        let wallet_b = chains.node_b.wallets().user1().cloned();

        let balance_a = chains
            .node_a
            .chain_driver()
            .query_balance(&wallet_a.address(), &denom_a)?;

        let amount1 = random_u128_range(1000, 5000);

        info!(
            "Performing IBC transfer with amount {}, which should be relayed because its an ordered channel",
            amount1
        );

        chains.node_a.chain_driver().ibc_transfer_token(
            &channel,
            &wallet_a.as_ref(),
            &wallet_b.address(),
            &vec![denom_a.with_amount(amount1).as_ref()],
        )?;

        sleep(Duration::from_secs(2));

        relayer.with_supervisor(|| {
            let amount2 = random_u128_range(1000, 5000);

            info!(
                "Performing IBC transfer with amount {}, which should be relayed",
                amount2
            );

            chains.node_a.chain_driver().ibc_transfer_token(
                &channel,
                &wallet_a.as_ref(),
                &wallet_b.address(),
                &vec![denom_a.with_amount(amount2).as_ref()],
            )?;

            sleep(Duration::from_secs(1));

            let denom_b = derive_ibc_denom(
                &channel.port_b.as_ref(),
                &channel.channel_id_b.as_ref(),
                &denom_a,
            )?;

            // Wallet on chain A should have both amount deducted.
            chains.node_a.chain_driver().assert_eventual_wallet_amount(
                &wallet_a.address(),
                &(balance_a - amount1 - amount2).as_ref(),
            )?;

            // Wallet on chain B should receive both IBC transfers
            chains.node_b.chain_driver().assert_eventual_wallet_amount(
                &wallet_b.address(),
                &denom_b.with_amount(amount1 + amount2).as_ref(),
            )?;

            Ok(())
        })
    }
}
