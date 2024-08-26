use ibc_relayer::channel::version::Version;
use ibc_test_framework::prelude::*;
use ibc_test_framework::util::random::random_u128_range;

#[test]
fn test_ibc_transfer() -> Result<(), Error> {
    run_binary_channel_test(&IbcTransferTest)
}

#[cfg(any(doc, feature = "ics20-v2"))]
#[test]
fn test_ibc_transfer_ics20_v2() -> Result<(), Error> {
    run_binary_channel_test(&IbcTransferICS20V2Test)
}

/**
   Test that IBC token transfer can still work with a single
   chain that is connected to itself.
*/
#[cfg(not(feature = "celestia"))]
#[test]
fn test_self_connected_ibc_transfer() -> Result<(), Error> {
    run_self_connected_binary_chain_test(&RunBinaryConnectionTest::new(&RunBinaryChannelTest::new(
        &RunWithSupervisor::new(&IbcTransferTest),
    )))
}

/**
   Run the IBC transfer test as an N-ary chain test case with SIZE=2.

   The work on N-ary chain is currently still work in progress, so we put
   this behind the "experimental" feature flag so that normal developers
   are not obligated to understand how this test works yet.
*/
#[cfg(not(feature = "celestia"))]
#[test]
fn test_nary_ibc_transfer() -> Result<(), Error> {
    run_binary_as_nary_channel_test(&IbcTransferTest)
}

#[cfg(not(feature = "celestia"))]
#[test]
fn test_self_connected_nary_ibc_transfer() -> Result<(), Error> {
    run_self_connected_nary_chain_test(&RunNaryConnectionTest::new(&RunNaryChannelTest::new(
        &RunBinaryAsNaryChannelTest::new(&IbcTransferTest),
    )))
}

pub struct IbcTransferTest;

impl TestOverrides for IbcTransferTest {
    fn modify_relayer_config(&self, config: &mut Config) {
        config.mode.clients.misbehaviour = false;
    }
}

impl BinaryChannelTest for IbcTransferTest {
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
            "Sending IBC transfer from chain {} to chain {} with amount of {}",
            chains.chain_id_b(),
            chains.chain_id_a(),
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

pub struct IbcTransferICS20V2Test;

impl TestOverrides for IbcTransferICS20V2Test {
    fn modify_relayer_config(&self, config: &mut Config) {
        config.mode.clients.misbehaviour = false;
    }

    fn channel_version(&self) -> Version {
        Version::ics20(2)
    }
}

impl BinaryChannelTest for IbcTransferICS20V2Test {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        _config: &TestConfig,
        _relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
        channel: ConnectedChannel<ChainA, ChainB>,
    ) -> Result<(), Error> {
        let denom_a = chains.node_a.denom();
        let other_denom = chains.node_a.second_denom();

        let wallet_a = chains.node_a.wallets().user1().cloned();
        let wallet_b = chains.node_b.wallets().user1().cloned();
        let wallet_c = chains.node_a.wallets().user2().cloned();

        let balance_a = chains
            .node_a
            .chain_driver()
            .query_balance(&wallet_a.address(), &denom_a)?;

        let other_balance_a = chains
            .node_a
            .chain_driver()
            .query_balance(&wallet_a.address(), &other_denom)?;

        let a_to_b_amount_denom_a = random_u128_range(1000, 5000);
        let a_to_b_amount_other_denom = random_u128_range(1000, 5000);

        info!(
            "Sending IBC transfer from chain {} to chain {} with amount of {} {} and {} {}",
            chains.chain_id_a(),
            chains.chain_id_b(),
            a_to_b_amount_denom_a,
            denom_a,
            a_to_b_amount_other_denom,
            other_denom,
        );

        let token_denom_a = denom_a.with_amount(a_to_b_amount_denom_a);
        let token_other_denom = other_denom.with_amount(a_to_b_amount_other_denom);

        chains.node_a.chain_driver().ibc_transfer_token(
            &channel,
            &wallet_a.as_ref(),
            &wallet_b.address(),
            &vec![token_denom_a.as_ref(), token_other_denom.as_ref()],
        )?;

        let denom_b = derive_ibc_denom(
            &channel.port_b.as_ref(),
            &channel.channel_id_b.as_ref(),
            &denom_a,
        )?;

        let other_denom_b = derive_ibc_denom(
            &channel.port_b.as_ref(),
            &channel.channel_id_b.as_ref(),
            &other_denom,
        )?;

        info!(
            "Waiting for user on chain B to receive IBC transferred amount of {} and {}",
            a_to_b_amount_denom_a, a_to_b_amount_other_denom
        );

        chains.node_a.chain_driver().assert_eventual_wallet_amount(
            &wallet_a.address(),
            &(balance_a - a_to_b_amount_denom_a).as_ref(),
        )?;

        chains.node_a.chain_driver().assert_eventual_wallet_amount(
            &wallet_a.address(),
            &(other_balance_a - a_to_b_amount_other_denom).as_ref(),
        )?;

        chains.node_b.chain_driver().assert_eventual_wallet_amount(
            &wallet_b.address(),
            &denom_b.with_amount(a_to_b_amount_denom_a).as_ref(),
        )?;

        chains.node_b.chain_driver().assert_eventual_wallet_amount(
            &wallet_b.address(),
            &other_denom_b
                .with_amount(a_to_b_amount_other_denom)
                .as_ref(),
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

        let other_balance_c = chains
            .node_a
            .chain_driver()
            .query_balance(&wallet_c.address(), &other_denom)?;

        let b_to_a_amount_denom_b = random_u128_range(500, a_to_b_amount_denom_a);
        let b_to_a_amount_other_denom_b = random_u128_range(500, a_to_b_amount_other_denom);

        info!(
            "Sending IBC transfer from chain {} to chain {} with amount of {} {} and {} {}",
            chains.chain_id_b(),
            chains.chain_id_a(),
            b_to_a_amount_denom_b,
            denom_b,
            b_to_a_amount_other_denom_b,
            other_denom_b,
        );

        let token_denom_b = denom_b.with_amount(b_to_a_amount_denom_b);
        let token_other_denom_b = other_denom_b.with_amount(b_to_a_amount_other_denom_b);

        chains.node_b.chain_driver().ibc_transfer_token(
            &channel.flip(),
            &wallet_b.as_ref(),
            &wallet_c.address(),
            &vec![token_denom_b.as_ref(), token_other_denom_b.as_ref()],
        )?;

        chains.node_b.chain_driver().assert_eventual_wallet_amount(
            &wallet_b.address(),
            &denom_b
                .with_amount(a_to_b_amount_denom_a - b_to_a_amount_denom_b)
                .as_ref(),
        )?;

        chains.node_b.chain_driver().assert_eventual_wallet_amount(
            &wallet_b.address(),
            &other_denom_b
                .with_amount(a_to_b_amount_other_denom - b_to_a_amount_other_denom_b)
                .as_ref(),
        )?;

        chains.node_a.chain_driver().assert_eventual_wallet_amount(
            &wallet_c.address(),
            &(balance_c + b_to_a_amount_denom_b).as_ref(),
        )?;

        chains.node_a.chain_driver().assert_eventual_wallet_amount(
            &wallet_c.address(),
            &(other_balance_c + b_to_a_amount_other_denom_b).as_ref(),
        )?;

        info!(
            "successfully performed reverse IBC transfer from chain {} back to chain {}",
            chains.chain_id_b(),
            chains.chain_id_a(),
        );

        Ok(())
    }
}
