use ibc_test_framework::prelude::*;

#[test]
fn test_ibc_denom_trace() -> Result<(), Error> {
    run_binary_channel_test(&IbcDenomTraceTest)
}

pub struct IbcDenomTraceTest;

impl TestOverrides for IbcDenomTraceTest {}

/// In order to test the denom_trace at first transfer IBC tokens from Chain A
/// to Chain B, and then retrieving the trace hash of the transferred tokens.
/// The trace hash is used to query the denom_trace and the result is verified.
impl BinaryChannelTest for IbcDenomTraceTest {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        _config: &TestConfig,
        _relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
        channel: ConnectedChannel<ChainA, ChainB>,
    ) -> Result<(), Error> {
        let a_to_b_amount: u64 = 1234;

        let denom_a = chains.node_a.denom();
        let wallet_a = chains.node_a.wallets().user1().cloned();
        let wallet_b = chains.node_b.wallets().user1().cloned();

        let balance_a = chains
            .node_a
            .chain_driver()
            .query_balance(&wallet_a.address(), &denom_a)?;

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

        let denom_trace = chains
            .handle_b()
            .query_denom_trace(denom_b.value().hash_only())?;

        assert_eq(
            "Path returned by denom_trace query should be <PORT>/<CHANNEL>",
            &denom_trace.path,
            &format!(
                "{}/{}",
                channel.port_b.as_ref(),
                channel.channel_id_b.as_ref()
            ),
        )?;

        assert_eq(
            "Denom returned by denom_trace query should be the same as denom_a",
            &denom_trace.base_denom,
            &denom_a.value().as_str().to_string(),
        )?;

        Ok(())
    }
}
