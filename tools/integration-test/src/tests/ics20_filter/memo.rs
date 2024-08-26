use byte_unit::Byte;

use ibc_relayer::config::types::ics20_field_size_limit::Ics20FieldSizeLimit;
use ibc_test_framework::prelude::*;

#[test]
fn test_memo_filter() -> Result<(), Error> {
    run_binary_channel_test(&IbcMemoFilterTest)
}

const MEMO_SIZE_LIMIT: usize = 2000;

pub struct IbcMemoFilterTest;

impl TestOverrides for IbcMemoFilterTest {
    fn modify_relayer_config(&self, config: &mut Config) {
        config.mode.packets.ics20_max_memo_size =
            Ics20FieldSizeLimit::new(true, Byte::from_bytes(MEMO_SIZE_LIMIT as u64));

        config.mode.clients.misbehaviour = false;
    }
}

impl BinaryChannelTest for IbcMemoFilterTest {
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

        let balance_a = chains
            .node_a
            .chain_driver()
            .query_balance(&wallet_a.address(), &denom_a)?;

        let a_to_b_amount = 23456u128;

        info!(
            "Sending invalid IBC transfer from chain {} to chain {} with amount of {} {}",
            chains.chain_id_a(),
            chains.chain_id_b(),
            a_to_b_amount,
            denom_a
        );

        // Create a memo bigger than the allowed limit
        let memo = "a".repeat(MEMO_SIZE_LIMIT + 1);

        chains
            .node_a
            .chain_driver()
            .ibc_transfer_token_with_memo_and_timeout(
                &channel,
                &wallet_a.as_ref(),
                &wallet_b.address(),
                &vec![denom_a.with_amount(a_to_b_amount).as_ref()],
                Some(memo),
                None,
            )?;

        // Wait a bit before asserting that the transaction has not been relayed
        sleep(Duration::from_secs(10));

        info!("Assert that the IBC transfer was filtered");

        let denom_b = derive_ibc_denom(
            &channel.port_b.as_ref(),
            &channel.channel_id_b.as_ref(),
            &denom_a,
        )?;

        // The sender tokens will be escrowed since the packet will not have timed out
        chains.node_a.chain_driver().assert_eventual_wallet_amount(
            &wallet_a.address(),
            &(balance_a.clone() - a_to_b_amount).as_ref(),
        )?;

        // The receiver will not have received the tokens since the packet should be
        // filtered
        chains.node_b.chain_driver().assert_eventual_wallet_amount(
            &wallet_b.address(),
            &denom_b.with_amount(0u64).as_ref(),
        )?;

        // Retry the IBC transfer without the memo field
        chains
            .node_a
            .chain_driver()
            .ibc_transfer_token_with_memo_and_timeout(
                &channel,
                &wallet_a.as_ref(),
                &wallet_b.address(),
                &vec![denom_a.with_amount(a_to_b_amount).as_ref()],
                None,
                None,
            )?;

        info!(
            "Waiting for user on chain B to receive IBC transferred amount of {}",
            a_to_b_amount
        );

        // The sender tokens from the first transaction will still be
        // escrowed since the packet will not have timed out
        chains.node_a.chain_driver().assert_eventual_wallet_amount(
            &wallet_a.address(),
            &(balance_a - a_to_b_amount - a_to_b_amount).as_ref(),
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

        Ok(())
    }
}
