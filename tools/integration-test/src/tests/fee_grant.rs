//! This test tests the fee_grant configuration:
//!
//! - The `FeeGrant` will register the address of `user2` as the granter
//!   for `user1` and will configure the `fee_granter` in Hermes. It will
//!   then assert that the `user1` doesn't pay any fees for a transfer and
//!   `user2` pays the fees.
//!
//! - The `NoFeeGrant` will register the address of `user2` as the granter
//!   for `user1`, but it will not configure the `fee_granter` in Hermes. It will
//!   then assert that the `user1` pays the fees and
//!   `user2` doesn't pay any fees.

use std::thread;

use ibc_relayer::config::ChainConfig;
use ibc_relayer_types::bigint::U256;
use ibc_test_framework::chain::ext::fee_grant::FeeGrantMethodsExt;
use ibc_test_framework::prelude::*;

#[test]
fn test_fee_grant() -> Result<(), Error> {
    run_binary_channel_test(&FeeGrantTest)
}

#[test]
fn test_no_fee_grant() -> Result<(), Error> {
    run_binary_channel_test(&NoFeeGrantTest)
}

struct FeeGrantTest;

impl TestOverrides for FeeGrantTest {}

impl BinaryChannelTest for FeeGrantTest {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        config: &TestConfig,
        relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
        channels: ConnectedChannel<ChainA, ChainB>,
    ) -> Result<(), Error> {
        let denom_a = chains.node_a.denom();
        let wallet_a = chains.node_a.wallets().user1().cloned();
        let wallet_b = chains.node_b.wallets().user1().cloned();
        let fee_denom_a = MonoTagged::new(Denom::base(config.native_token(0)));

        let a_to_b_amount = 12345u64;
        let granter = chains
            .node_a
            .wallets()
            .user2()
            .address()
            .value()
            .to_string();
        let grantee = chains
            .node_a
            .wallets()
            .user1()
            .address()
            .value()
            .to_string();

        chains.node_a.chain_driver().feegrant_grant(
            &granter,
            &grantee,
            &fee_denom_a.with_amount(381000000u64).as_ref(),
        )?;

        // Wait for the feegrant to be processed
        thread::sleep(Duration::from_secs(5));

        let denom_b = derive_ibc_denom(
            &channels.port_b.as_ref(),
            &channels.channel_id_b.as_ref(),
            &denom_a,
        )?;

        let gas_denom_str = match relayer
            .config
            .chains
            .first()
            .ok_or_else(|| eyre!("chain configuration is empty"))?
        {
            ChainConfig::CosmosSdk(chain_config) => chain_config.gas_price.denom.clone(),
        };

        let gas_denom: MonoTagged<ChainA, Denom> = MonoTagged::new(Denom::Base(gas_denom_str));

        let balance_user1_before = chains
            .node_a
            .chain_driver()
            .query_balance(&wallet_a.address(), &gas_denom.as_ref())?;
        let balance_user2_before = chains.node_a.chain_driver().query_balance(
            &chains.node_a.wallets().user2().address(),
            &gas_denom.as_ref(),
        )?;

        let mut modified_relayer = relayer;

        modified_relayer
            .config
            .chains
            .iter_mut()
            .for_each(|chain_config| {
                if chain_config.id() == chains.node_a.chain_id().0 {
                    match chain_config {
                        ChainConfig::CosmosSdk(c) => {
                            c.fee_granter = Some("user2".to_owned());
                        }
                    }
                }
            });

        let mut modified_driver = chains.node_a.chain_driver().0.clone();

        let mut modified_tx_config = chains.node_a.chain_driver().tx_config().0.clone();

        modified_tx_config.gas_config.fee_granter =
            chains.node_a.wallets().user2().address().to_string();

        modified_driver.tx_config = modified_tx_config;

        let md: MonoTagged<ChainA, &ChainDriver> = MonoTagged::new(&modified_driver);

        md.ibc_transfer_token(
            &channels,
            &wallet_a.as_ref(),
            &wallet_b.address(),
            &vec![denom_a.with_amount(a_to_b_amount).as_ref()],
        )?;

        thread::sleep(Duration::from_secs(10));

        let balance_after_a2 = chains.node_a.chain_driver().query_balance(
            &chains.node_a.wallets().user2().address(),
            &gas_denom.as_ref(),
        )?;

        // Assert that user on chain B received the tokens
        chains.node_b.chain_driver().assert_eventual_wallet_amount(
            &wallet_b.address(),
            &denom_b.with_amount(a_to_b_amount).as_ref(),
        )?;

        // Assert that user1 has not paid any fees
        chains
            .node_a
            .chain_driver()
            .assert_eventual_wallet_amount(&wallet_a.address(), &balance_user1_before.as_ref())?;

        let paid_fees = balance_user2_before
            .amount()
            .checked_sub(balance_after_a2.amount());

        assert!(
            paid_fees.is_some(),
            "subtraction between queried amounts should be Some"
        );

        // Only assert that the paid fees are non zero since they might vary
        assert_ne!(
            paid_fees.unwrap().0,
            U256::from(0),
            "granter should have paid the fees"
        );

        Ok(())
    }
}

struct NoFeeGrantTest;

impl TestOverrides for NoFeeGrantTest {}

impl BinaryChannelTest for NoFeeGrantTest {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        config: &TestConfig,
        relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
        channels: ConnectedChannel<ChainA, ChainB>,
    ) -> Result<(), Error> {
        let denom_a = chains.node_a.denom();
        let wallet_a = chains.node_a.wallets().user1().cloned();
        let wallet_a2 = chains.node_a.wallets().user2().cloned();
        let wallet_b = chains.node_b.wallets().user1().cloned();
        let fee_denom_a = MonoTagged::new(Denom::base(config.native_token(0)));

        let a_to_b_amount = 12345u64;
        let granter = chains
            .node_a
            .wallets()
            .user2()
            .address()
            .value()
            .to_string();
        let grantee = chains
            .node_a
            .wallets()
            .user1()
            .address()
            .value()
            .to_string();

        chains.node_a.chain_driver().feegrant_grant(
            &granter,
            &grantee,
            &fee_denom_a.with_amount(381000000u64).as_ref(),
        )?;

        // Wait for the feegrant to be processed
        thread::sleep(Duration::from_secs(5));

        let denom_b = derive_ibc_denom(
            &channels.port_b.as_ref(),
            &channels.channel_id_b.as_ref(),
            &denom_a,
        )?;

        let gas_denom_str = match relayer
            .config
            .chains
            .first()
            .ok_or_else(|| eyre!("chain configuration is empty"))?
        {
            ChainConfig::CosmosSdk(chain_config) => chain_config.gas_price.denom.clone(),
        };

        let gas_denom: MonoTagged<ChainA, Denom> = MonoTagged::new(Denom::Base(gas_denom_str));

        let balance_user1_before = chains
            .node_a
            .chain_driver()
            .query_balance(&wallet_a.address(), &gas_denom.as_ref())?;
        let balance_user2_before = chains.node_a.chain_driver().query_balance(
            &chains.node_a.wallets().user2().address(),
            &gas_denom.as_ref(),
        )?;

        chains.node_a.chain_driver().ibc_transfer_token(
            &channels,
            &wallet_a.as_ref(),
            &wallet_b.address(),
            &vec![denom_a.with_amount(a_to_b_amount).as_ref()],
        )?;

        thread::sleep(Duration::from_secs(10));

        let balance_user1_after = chains.node_a.chain_driver().query_balance(
            &chains.node_a.wallets().user1().address(),
            &gas_denom.as_ref(),
        )?;

        // Assert that user on chain B received the tokens
        chains.node_b.chain_driver().assert_eventual_wallet_amount(
            &wallet_b.address(),
            &denom_b.with_amount(a_to_b_amount).as_ref(),
        )?;

        // Assert that user2 has not paid any fees
        chains
            .node_a
            .chain_driver()
            .assert_eventual_wallet_amount(&wallet_a2.address(), &balance_user2_before.as_ref())?;

        let paid_fees = balance_user1_before
            .amount()
            .checked_sub(balance_user1_after.amount());

        assert!(
            paid_fees.is_some(),
            "subtraction between queried amounts should be Some"
        );

        // Only assert that the paid fees are non zero since they might vary
        assert_ne!(
            paid_fees.unwrap().0,
            U256::from(0),
            "granter should not have paid the fees"
        );

        Ok(())
    }
}
