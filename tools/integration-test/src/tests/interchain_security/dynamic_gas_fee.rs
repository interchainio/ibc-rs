//! The [`DynamicGasTest`] test ensures that the [`DynamicGas`]
//! configuration works correctly. The test can enable or disable the dynamic
//! gas price for the second chain.
//!
//! To test dynamic gas configuration, it will enable dynamic gas price on the
//! second chain only. It will then create and relay a first IBC transfer with a
//! big memo. The gas fee paid is then recorded.
//! A second IBC transfer without memo will then be relayed. The gas fee paid
//! will also be recorded. The test will assert that the Tx with a big memo
//! and dynamic gas enabled is lower than the Tx without memo and dynamic gas
//! disabled.
//!
//! The second test disables the dynamic gas price on both chains in
//! order to ensure that the first IBC transfer will cost more if dynamic
//! gas is disabled.

use ibc_relayer::config::dynamic_gas::DynamicGasPrice;
use ibc_relayer::config::gas_multiplier::GasMultiplier;
use ibc_relayer::config::ChainConfig;
use ibc_relayer::config::GasPrice;
use ibc_test_framework::framework::binary::channel::run_binary_interchain_security_channel_test;
use ibc_test_framework::prelude::*;
use ibc_test_framework::util::interchain_security::update_genesis_for_consumer_chain;
use ibc_test_framework::util::interchain_security::update_relayer_config_for_consumer_chain;

#[test]
fn test_fee_market_dynamic_gas_transfer() -> Result<(), Error> {
    run_binary_interchain_security_channel_test(&DynamicGasTest {
        dynamic_gas_enabled: true,
    })
}

#[test]
fn test_fee_market_static_gas_transfer() -> Result<(), Error> {
    run_binary_interchain_security_channel_test(&DynamicGasTest {
        dynamic_gas_enabled: false,
    })
}

const MEMO_CHAR: &str = "a";
const MEMO_SIZE: usize = 10000;

pub struct DynamicGasTest {
    dynamic_gas_enabled: bool,
}

impl TestOverrides for DynamicGasTest {
    fn modify_genesis_file(&self, genesis: &mut serde_json::Value) -> Result<(), Error> {
        update_genesis_for_consumer_chain(genesis)?;
        // This is a configuration specifically for chains with feemarket
        if genesis
            .get_mut("app_state")
            .and_then(|app_state| app_state.get("feemarket"))
            .is_some()
        {
            let params = genesis
                .get_mut("app_state")
                .and_then(|app_state| app_state.get_mut("feemarket"))
                .and_then(|feemarket| feemarket.get_mut("params"))
                .and_then(|params| params.as_object_mut())
                .ok_or_else(|| eyre!("failed to get feemarket params in genesis file"))?;
            params.insert(
                "min_base_fee".to_owned(),
                serde_json::Value::String("0.0025".to_owned()),
            );
        }

        Ok(())
    }

    // The `ccv_consumer_chain` must be `true` for the Consumer chain.
    // The `trusting_period` must be strictly smaller than the `unbonding_period`
    // specified in the Consumer chain proposal. The test framework uses 100s in
    // the proposal.
    fn modify_relayer_config(&self, config: &mut Config) {
        config.mode.clients.misbehaviour = false;
        config.mode.clients.refresh = false;
        config.mode.packets.clear_interval = 0;

        update_relayer_config_for_consumer_chain(config);

        match &mut config.chains[0] {
            ChainConfig::CosmosSdk(chain_config_a) => {
                chain_config_a.gas_price =
                    GasPrice::new(0.3, chain_config_a.gas_price.denom.clone());

                chain_config_a.dynamic_gas_price = DynamicGasPrice::unsafe_new(false, 1.1, 0.6);
            }
        }

        match &mut config.chains[1] {
            ChainConfig::CosmosSdk(chain_config_b) => {
                chain_config_b.gas_price =
                    GasPrice::new(0.3, chain_config_b.gas_price.denom.clone());

                chain_config_b.gas_multiplier = Some(GasMultiplier::unsafe_new(1.8));

                chain_config_b.dynamic_gas_price =
                    DynamicGasPrice::unsafe_new(self.dynamic_gas_enabled, 1.1, 0.6);
            }
        }
    }

    fn should_spawn_supervisor(&self) -> bool {
        false
    }
}

impl BinaryChannelTest for DynamicGasTest {
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

        let a_to_b_amount = 12345u64;

        let denom_a_to_b = derive_ibc_denom(
            &channel.port_b.as_ref(),
            &channel.channel_id_b.as_ref(),
            &denom_a,
        )?;

        let gas_denom_a: MonoTagged<ChainA, Denom> =
            MonoTagged::new(Denom::Base("stake".to_owned()));
        let gas_denom_b: MonoTagged<ChainB, Denom> =
            MonoTagged::new(Denom::Base("stake".to_owned()));

        let balance_relayer_b_before = chains.node_b.chain_driver().query_balance(
            &chains.node_b.wallets().relayer().address(),
            &gas_denom_b.as_ref(),
        )?;

        let memo: String = MEMO_CHAR.repeat(MEMO_SIZE);

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

        // Do a simple IBC transfer with the dynamic gas configuration
        let tx1_paid_gas_relayer = relayer.with_supervisor(|| {
            // Assert that user on chain B received the tokens
            chains.node_b.chain_driver().assert_eventual_wallet_amount(
                &wallet_b.address(),
                &denom_a_to_b.with_amount(a_to_b_amount).as_ref(),
            )?;

            // Wait for a bit before querying the new balance
            sleep(Duration::from_secs(5));

            let balance_relayer_b_after = chains.node_b.chain_driver().query_balance(
                &chains.node_b.wallets().relayer().address(),
                &gas_denom_b.as_ref(),
            )?;

            let paid_fees_relayer_b = balance_relayer_b_before
                .amount()
                .checked_sub(balance_relayer_b_after.amount());

            assert!(
                paid_fees_relayer_b.is_some(),
                "subtraction between queried amounts for relayer should be Some"
            );

            info!("IBC transfer with memo was successful");

            Ok(paid_fees_relayer_b.unwrap())
        })?;

        let b_to_a_amount = 23456u64;
        let denom_b = chains.node_b.denom();

        let denom_b_to_a = derive_ibc_denom(
            &channel.port_a.as_ref(),
            &channel.channel_id_a.as_ref(),
            &denom_b,
        )?;

        let balance_relayer_a_before = chains.node_a.chain_driver().query_balance(
            &chains.node_a.wallets().relayer().address(),
            &gas_denom_a.as_ref(),
        )?;

        info!("Will ibc transfer");

        chains.node_b.chain_driver().ibc_transfer_token(
            &channel.flip(),
            &chains.node_b.wallets().user1(),
            &chains.node_a.wallets().user1().address(),
            &vec![denom_b.with_amount(b_to_a_amount).as_ref()],
        )?;

        info!("Done ibc transfer");

        let tx2_paid_gas_relayer = relayer.with_supervisor(|| {
            // Assert that user on chain B received the tokens
            chains.node_a.chain_driver().assert_eventual_wallet_amount(
                &chains.node_a.wallets().user1().address(),
                &denom_b_to_a.with_amount(b_to_a_amount).as_ref(),
            )?;

            // Wait for a bit before querying the new balance
            sleep(Duration::from_secs(5));

            let balance_relayer_a_after = chains.node_a.chain_driver().query_balance(
                &chains.node_a.wallets().relayer().address(),
                &gas_denom_a.as_ref(),
            )?;

            let paid_fees_relayer_a = balance_relayer_a_before
                .amount()
                .checked_sub(balance_relayer_a_after.amount());

            assert!(
                paid_fees_relayer_a.is_some(),
                "subtraction between queried amounts for relayer should be Some"
            );

            info!("IBC transfer without memo was successful");

            Ok(paid_fees_relayer_a.unwrap())
        })?;

        info!("paid gas fees for Tx with memo `{tx1_paid_gas_relayer}`, without memo `{tx2_paid_gas_relayer}`");

        if self.dynamic_gas_enabled {
            assert!(
                tx1_paid_gas_relayer < tx2_paid_gas_relayer,
                "with dynamic gas enabled, gas paid for the first TX should be lower"
            );
        } else {
            assert!(
                tx1_paid_gas_relayer > tx2_paid_gas_relayer,
                "with dynamic gas disabled, gas paid for the second TX should be lower"
            );
        }

        Ok(())
    }
}
