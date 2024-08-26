//! The following tests are for the Interchain Security.
//! These tests require the first chain to be a Provider chain and
//! the second chain a Consumer chain.
use ibc_test_framework::framework::binary::channel::run_binary_interchain_security_channel_test;
use ibc_test_framework::prelude::*;
use ibc_test_framework::util::interchain_security::{
    update_genesis_for_consumer_chain, update_relayer_config_for_consumer_chain,
};
use ibc_test_framework::util::random::random_u128_range;

#[test]
fn test_ics_transfer() -> Result<(), Error> {
    run_binary_interchain_security_channel_test(&InterchainSecurityTransferTest)
}

struct InterchainSecurityTransferTest;

impl TestOverrides for InterchainSecurityTransferTest {
    fn modify_genesis_file(&self, genesis: &mut serde_json::Value) -> Result<(), Error> {
        update_genesis_for_consumer_chain(genesis)
    }

    // The `ccv_consumer_chain` must be `true` for the Consumer chain.
    // The `trusting_period` must be strictly smaller than the `unbonding_period`
    // specified in the Consumer chain proposal. The test framework uses 100s in
    // the proposal.
    fn modify_relayer_config(&self, config: &mut Config) {
        update_relayer_config_for_consumer_chain(config);
    }
}

impl BinaryChannelTest for InterchainSecurityTransferTest {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        _config: &TestConfig,
        _relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
        channel: ConnectedChannel<ChainA, ChainB>,
    ) -> Result<(), Error> {
        let denom_a = chains.node_a.denom();

        let wallet_a = chains.node_a.wallets().user1().cloned();
        let wallet_b = chains.node_b.wallets().user1().cloned();
        let wallet_c = chains.node_a.wallets().user2().cloned();

        let balance_a = chains
            .node_a
            .chain_driver()
            .query_balance(&wallet_a.address(), &denom_a)?;

        let a_to_b_amount = random_u128_range(1000, 5000);

        info!(
            "Sending IBC transfer from chain {} to chain {} with: channel id {}, port id {} and amount of {} {}",
            chains.chain_id_a(),
            chains.chain_id_b(),
            channel.channel_id_a.to_string(),
            channel.port_a.to_string(),
            a_to_b_amount,
            denom_a
        );

        chains.node_a.chain_driver().ibc_transfer_token(
            &channel,
            &wallet_a.as_ref(),
            &wallet_b.address(),
            &vec![denom_a.with_amount(a_to_b_amount).as_ref()],
        )?;

        let denom_b = derive_ibc_denom(
            &channel.port_b.as_ref(),
            &channel.channel_id_b.as_ref(),
            &denom_a,
        )?;

        info!(
            "Waiting for user on chain B to receive IBC transferred amount of {}",
            a_to_b_amount
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

        let balance_c = chains
            .node_a
            .chain_driver()
            .query_balance(&wallet_c.address(), &denom_a)?;

        let b_to_a_amount = random_u128_range(500, a_to_b_amount);

        info!(
            "Sending IBC transfer from chain {} to chain {} with: channel id {}, port id {} and amount of {}",
            chains.chain_id_b(),
            chains.chain_id_a(),
            channel.channel_id_b.to_string(),
            channel.port_b.to_string(),
            b_to_a_amount,
        );

        chains.node_b.chain_driver().ibc_transfer_token(
            &channel.flip(),
            &wallet_b.as_ref(),
            &wallet_c.address(),
            &vec![denom_b.with_amount(b_to_a_amount).as_ref()],
        )?;

        chains.node_b.chain_driver().assert_eventual_wallet_amount(
            &wallet_b.address(),
            &denom_b.with_amount(a_to_b_amount - b_to_a_amount).as_ref(),
        )?;

        chains.node_a.chain_driver().assert_eventual_wallet_amount(
            &wallet_c.address(),
            &(balance_c + b_to_a_amount).as_ref(),
        )?;

        info!(
            "successfully performed reverse IBC transfer from chain {} back to chain {}",
            chains.chain_id_b(),
            chains.chain_id_a(),
        );
        Ok(())
    }
}
