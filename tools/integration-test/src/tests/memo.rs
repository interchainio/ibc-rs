//! This test asserts that transactions preserve the value of
//! the `memo_prefix` field of the receiving chain.
//!
//! You can find a more thorough walkthrough of this test at
//! `tools/test-framework/src/docs/walkthroughs/memo.rs`.

use ibc_relayer::config::types::Memo;
use ibc_relayer::config::ChainConfig;
use serde_json as json;

use ibc_test_framework::prelude::*;
use ibc_test_framework::util::random::{random_string, random_u128_range};

const OVERWRITE_MEMO: &str = "Overwritten memo";

#[test]
fn test_memo() -> Result<(), Error> {
    let memo = Memo::new(random_string()).unwrap();
    let test = MemoTest { memo };
    run_binary_channel_test(&test)
}

#[test]
fn test_memo_overwrite() -> Result<(), Error> {
    let memo = Memo::new(random_string()).unwrap();
    let test = MemoOverwriteTest { memo };
    run_binary_channel_test(&test)
}

pub struct MemoTest {
    memo: Memo,
}

impl TestOverrides for MemoTest {
    fn modify_relayer_config(&self, config: &mut Config) {
        for chain in config.chains.iter_mut() {
            match chain {
                ChainConfig::CosmosSdk(chain_config) => {
                    chain_config.memo_prefix = self.memo.clone();
                }
            }
        }
    }
}

impl BinaryChannelTest for MemoTest {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        _config: &TestConfig,
        _relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
        channel: ConnectedChannel<ChainA, ChainB>,
    ) -> Result<(), Error> {
        info!(
            "testing IBC transfer with memo configured: \"{}\"",
            self.memo
        );

        let denom_a = chains.node_a.denom();

        let a_to_b_amount = random_u128_range(1000, 5000);

        chains.node_a.chain_driver().ibc_transfer_token(
            &channel,
            &chains.node_a.wallets().user1(),
            &chains.node_b.wallets().user1().address(),
            &vec![denom_a.with_amount(a_to_b_amount).as_ref()],
        )?;

        let denom_b = derive_ibc_denom(
            &channel.port_b.as_ref(),
            &channel.channel_id_b.as_ref(),
            &denom_a,
        )?;

        chains.node_b.chain_driver().assert_eventual_wallet_amount(
            &chains.node_b.wallets().user1().address(),
            &denom_b.with_amount(a_to_b_amount).as_ref(),
        )?;

        let tx_info = chains
            .node_b
            .chain_driver()
            .query_recipient_transactions(&chains.node_b.wallets().user1().address())?;

        assert_tx_memo_equals(&tx_info, self.memo.as_str())?;

        Ok(())
    }
}

pub struct MemoOverwriteTest {
    memo: Memo,
}

impl TestOverrides for MemoOverwriteTest {
    fn modify_relayer_config(&self, config: &mut Config) {
        for chain in config.chains.iter_mut() {
            match chain {
                ChainConfig::CosmosSdk(chain_config) => {
                    chain_config.memo_prefix = self.memo.clone();
                    chain_config.memo_overwrite = Some(Memo::new(OVERWRITE_MEMO).unwrap())
                }
            }
        }
    }
}

impl BinaryChannelTest for MemoOverwriteTest {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        _config: &TestConfig,
        _relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
        channel: ConnectedChannel<ChainA, ChainB>,
    ) -> Result<(), Error> {
        info!(
            "testing IBC transfer with memo configured: \"{}\"",
            self.memo
        );

        let denom_a = chains.node_a.denom();

        let a_to_b_amount = random_u128_range(1000, 5000);

        chains.node_a.chain_driver().ibc_transfer_token(
            &channel,
            &chains.node_a.wallets().user1(),
            &chains.node_b.wallets().user1().address(),
            &vec![denom_a.with_amount(a_to_b_amount).as_ref()],
        )?;

        let denom_b = derive_ibc_denom(
            &channel.port_b.as_ref(),
            &channel.channel_id_b.as_ref(),
            &denom_a,
        )?;

        chains.node_b.chain_driver().assert_eventual_wallet_amount(
            &chains.node_b.wallets().user1().address(),
            &denom_b.with_amount(a_to_b_amount).as_ref(),
        )?;

        let tx_info = chains
            .node_b
            .chain_driver()
            .query_recipient_transactions(&chains.node_b.wallets().user1().address())?;

        assert_tx_memo_equals(&tx_info, OVERWRITE_MEMO)?;

        Ok(())
    }
}

fn assert_tx_memo_equals(tx_info: &json::Value, expected_memo: &str) -> Result<(), Error> {
    debug!("comparing memo field from json value {}", tx_info);

    let memo_field = &tx_info["txs"][0]["tx"]["body"]["memo"];

    info!("memo field value: {}", memo_field);

    let memo_str = memo_field
        .as_str()
        .ok_or_else(|| eyre!("expect memo string field to be present in JSON"))?;

    assert_eq!(memo_str, expected_memo);

    Ok(())
}
