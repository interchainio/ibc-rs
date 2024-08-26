//! This test tests three different cases:
//!
//! - The `IbcForwardTransferTest` tests the case a packet is successfully
//!   forwarded.
//!
//! - The `MisspelledMemoFieldsIbcForwardTransferTest` tests the case where the
//!   fields inside the memo are misspelled:
//!    - Misspelled `forward`: The intermediary chain will not understand the transfer
//!      must be forwarded, and will thus keep the tokens.
//!    - Misspelled `receiver`: The intermediary chain will not find the receiver field
//!      and will thus refund the sender.
//!    - Misspelled `port`: The intermediary chain will not find the port field
//!      and will thus refund the sender.
//!    - Misspelled `channel`: The intermediary chain will not find the channel field
//!      and will thus refund the sender.
//!
//! - The `MisspelledMemoContentIbcForwardTransferTest` tests the case where the content of
//!   the memo fields are misspelled:
//!    - Misspelled receiver address, port or channel: The intermediary chain will refund the sender.

use ibc_relayer::config::{self, ModeConfig};
use ibc_test_framework::prelude::*;

use crate::tests::forward::memo::{
    MemoField, MemoInfo, MemoMisspelledField, MisspelledChannelMemoInfo, MisspelledPortMemoInfo,
    MisspelledReceiverMemoInfo,
};

#[test]
fn test_ibc_forward_transfer() -> Result<(), Error> {
    run_nary_channel_test(&IbcForwardTransferTest)
}

#[test]
fn test_misspelled_memo_fields_ibc_forward_transfer() -> Result<(), Error> {
    run_nary_channel_test(&MisspelledMemoFieldsIbcForwardTransferTest)
}

#[test]
fn test_misspelled_memo_content_ibc_forward_transfer() -> Result<(), Error> {
    run_nary_channel_test(&MisspelledMemoContentIbcForwardTransferTest)
}

struct IbcForwardTransferTestOverrides;

impl TestOverrides for IbcForwardTransferTestOverrides {
    fn modify_relayer_config(&self, config: &mut Config) {
        config.mode = ModeConfig {
            connections: config::Connections { enabled: false },
            channels: config::Channels { enabled: false },
            ..Default::default()
        };
    }

    fn modify_test_config(&self, config: &mut TestConfig) {
        config.bootstrap_with_random_ids = false;
    }

    fn should_spawn_supervisor(&self) -> bool {
        true
    }
}

impl PortsOverride<3> for IbcForwardTransferTestOverrides {}

struct IbcForwardTransferTest;

impl NaryChannelTest<3> for IbcForwardTransferTest {
    fn run<Handle: ChainHandle>(
        &self,
        _config: &TestConfig,
        _relayer: RelayerDriver,
        chains: NaryConnectedChains<Handle, 3>,
        channels: NaryConnectedChannels<Handle, 3>,
    ) -> Result<(), Error> {
        let connected_chains = chains.connected_chains_at::<0, 1>()?;

        let node_a = chains.full_node_at::<0>()?;
        let node_b = chains.full_node_at::<1>()?;
        let node_c = chains.full_node_at::<2>()?;

        let channel_a_to_b = channels.channel_at::<0, 1>()?;
        let channel_b_to_c = channels.channel_at::<1, 2>()?;

        let denom_a = connected_chains.node_a.denom();

        let denom_b = derive_ibc_denom(
            &channel_a_to_b.port_b.as_ref(),
            &channel_a_to_b.channel_id_b.as_ref(),
            &denom_a,
        )?;

        let denom_a_to_b = derive_ibc_denom(
            &channel_a_to_b.port_b.as_ref(),
            &channel_a_to_b.channel_id_b.as_ref(),
            &denom_a,
        )?;

        let denom_a_to_c = derive_ibc_denom(
            &channel_b_to_c.port_b.as_ref(),
            &channel_b_to_c.channel_id_b.as_ref(),
            &denom_b.as_ref(),
        )?;

        let wallets_a = node_a.wallets();
        let wallet_a = wallets_a.user1();

        let wallets_b = node_b.wallets();
        let wallet_b = wallets_b.user1();

        let wallets_c = node_c.wallets();
        let wallet_c = wallets_c.user1();

        let balance_a = node_a
            .chain_driver()
            .query_balance(&wallet_a.address(), &denom_a)?;

        let a_to_c_amount = 4000_u128;

        let memo_field: MemoField<MemoInfo> = MemoField::new(
            wallet_c.address().value().to_string(),
            channel_b_to_c.port_a.to_string(),
            channel_b_to_c.channel.a_channel_id().unwrap().to_string(),
        );
        let memo = serde_json::to_string(&memo_field).unwrap();

        node_a
            .chain_driver()
            .ibc_transfer_token_with_memo_and_timeout(
                &channel_a_to_b,
                &wallet_a,
                &wallet_b.address(),
                &vec![denom_a.with_amount(a_to_c_amount).as_ref()],
                Some(memo),
                None,
            )?;

        info!(
            "waiting for user on chain C to receive IBC transferred amount of {}",
            a_to_c_amount
        );

        node_c.chain_driver().assert_eventual_wallet_amount(
            &wallet_c.address(),
            &denom_a_to_c.with_amount(a_to_c_amount).as_ref(),
        )?;

        node_a.chain_driver().assert_eventual_wallet_amount(
            &wallet_a.address(),
            &(balance_a - a_to_c_amount).as_ref(),
        )?;

        node_b.chain_driver().assert_eventual_wallet_amount(
            &wallet_b.address(),
            &denom_a_to_b.with_amount(0_u128).as_ref(),
        )?;

        info!(
            "successfully performed IBC transfer from chain {} to chain {}",
            chains.chain_handle_at::<0>().unwrap().value(),
            chains.chain_handle_at::<2>().unwrap().value(),
        );

        Ok(())
    }
}

struct MisspelledMemoFieldsIbcForwardTransferTest;

impl NaryChannelTest<3> for MisspelledMemoFieldsIbcForwardTransferTest {
    fn run<Handle: ChainHandle>(
        &self,
        _config: &TestConfig,
        _relayer: RelayerDriver,
        chains: NaryConnectedChains<Handle, 3>,
        channels: NaryConnectedChannels<Handle, 3>,
    ) -> Result<(), Error> {
        let connected_chains = chains.connected_chains_at::<0, 1>()?;

        let node_a = chains.full_node_at::<0>()?;
        let node_b = chains.full_node_at::<1>()?;
        let node_c = chains.full_node_at::<2>()?;

        let channel_a_to_b = channels.channel_at::<0, 1>()?;
        let channel_b_to_c = channels.channel_at::<1, 2>()?;

        let denom_a = connected_chains.node_a.denom();

        let denom_b = derive_ibc_denom(
            &channel_a_to_b.port_b.as_ref(),
            &channel_a_to_b.channel_id_b.as_ref(),
            &denom_a,
        )?;

        let denom_a_to_b = derive_ibc_denom(
            &channel_a_to_b.port_b.as_ref(),
            &channel_a_to_b.channel_id_b.as_ref(),
            &denom_a,
        )?;

        let denom_a_to_c = derive_ibc_denom(
            &channel_b_to_c.port_b.as_ref(),
            &channel_b_to_c.channel_id_b.as_ref(),
            &denom_b.as_ref(),
        )?;

        let wallets_a = node_a.wallets();
        let wallet_a = wallets_a.user1();

        let wallets_b = node_b.wallets();
        let wallet_b = wallets_b.user1();

        let wallets_c = node_c.wallets();
        let wallet_c = wallets_c.user1();

        let balance_a = node_a
            .chain_driver()
            .query_balance(&wallet_a.address(), &denom_a)?;

        let balance_c = node_c
            .chain_driver()
            .query_balance(&wallet_c.address(), &denom_a_to_c.as_ref())?;

        let a_to_c_amount = 4000_u128;

        // Memo with field spelled `fort` instead of `port`.
        let memo_invalid_field: MemoField<MisspelledPortMemoInfo> = MemoField::new(
            wallet_c.address().value().to_string(),
            channel_b_to_c.port_a.to_string(),
            channel_b_to_c.channel.a_channel_id().unwrap().to_string(),
        );
        let memo1 = serde_json::to_string(&memo_invalid_field).unwrap();

        // Memo with field spelled `xhannel` instead of `channel`.
        let memo_invalid_field: MemoField<MisspelledChannelMemoInfo> = MemoField::new(
            wallet_c.address().value().to_string(),
            channel_b_to_c.port_a.to_string(),
            channel_b_to_c.channel.a_channel_id().unwrap().to_string(),
        );
        let memo2 = serde_json::to_string(&memo_invalid_field).unwrap();

        // Memo with field spelled `recv` instead of `receiver`.
        let memo_invalid_field: MemoField<MisspelledReceiverMemoInfo> = MemoField::new(
            wallet_c.address().value().to_string(),
            channel_b_to_c.port_a.to_string(),
            channel_b_to_c.channel.a_channel_id().unwrap().to_string(),
        );
        let memo3 = serde_json::to_string(&memo_invalid_field).unwrap();

        // Memo with field spelled `fwd` instead of `forward`.
        let memo_invalid_field: MemoMisspelledField<MemoInfo> = MemoMisspelledField::new(
            wallet_c.address().value().to_string(),
            channel_b_to_c.port_a.to_string(),
            channel_b_to_c.channel.a_channel_id().unwrap().to_string(),
        );
        let memo4 = serde_json::to_string(&memo_invalid_field).unwrap();

        {
            info!("forward transfer with invalid `port` field");

            node_a
                .chain_driver()
                .ibc_transfer_token_with_memo_and_timeout(
                    &channel_a_to_b,
                    &wallet_a,
                    &wallet_b.address(),
                    &vec![denom_a.with_amount(a_to_c_amount).as_ref()],
                    Some(memo1),
                    None,
                )?;

            // Wait before checking the balances
            std::thread::sleep(Duration::from_secs(10));

            info!("checking that the sender was refunded and other chains didn't receive tokens");

            // As the port field is misspelled, the sender will be refunded
            node_b.chain_driver().assert_eventual_wallet_amount(
                &wallet_b.address(),
                &denom_a_to_b.with_amount(0_u128).as_ref(),
            )?;

            node_a
                .chain_driver()
                .assert_eventual_wallet_amount(&wallet_a.address(), &(balance_a).as_ref())?;

            // The expected receiver will never receive the token.
            node_c.chain_driver().assert_eventual_wallet_amount(
                &wallet_c.address(),
                &denom_a_to_c.with_amount(0_u128).as_ref(),
            )?;
        }

        {
            info!("forward transfer with invalid `channel` field");

            node_a
                .chain_driver()
                .ibc_transfer_token_with_memo_and_timeout(
                    &channel_a_to_b,
                    &wallet_a,
                    &wallet_b.address(),
                    &vec![denom_a.with_amount(a_to_c_amount).as_ref()],
                    Some(memo2),
                    None,
                )?;

            // Wait before checking the balances
            std::thread::sleep(Duration::from_secs(10));

            info!("checking that the sender was refunded and other chains didn't receive tokens");

            // As the channel field is misspelled, the sender will be refunded
            node_a
                .chain_driver()
                .assert_eventual_wallet_amount(&wallet_a.address(), &(balance_a).as_ref())?;

            node_b.chain_driver().assert_eventual_wallet_amount(
                &wallet_b.address(),
                &denom_a_to_b.with_amount(0_u128).as_ref(),
            )?;

            // The expected receiver will never receive the token.
            node_c.chain_driver().assert_eventual_wallet_amount(
                &wallet_c.address(),
                &denom_a_to_c.with_amount(0_u128).as_ref(),
            )?;
        }

        {
            info!("forward transfer with invalid `receiver` field");

            node_a
                .chain_driver()
                .ibc_transfer_token_with_memo_and_timeout(
                    &channel_a_to_b,
                    &wallet_a,
                    &wallet_b.address(),
                    &vec![denom_a.with_amount(a_to_c_amount).as_ref()],
                    Some(memo3),
                    None,
                )?;

            info!("checking that the sender was refunded and other chains didn't receive tokens");

            // Wait before checking the balances
            std::thread::sleep(Duration::from_secs(10));

            node_a
                .chain_driver()
                .assert_eventual_wallet_amount(&wallet_a.address(), &(balance_a).as_ref())?;

            // As the receiver field is misspelled, a default address will be used ?
            node_b.chain_driver().assert_eventual_wallet_amount(
                &wallet_b.address(),
                &denom_a_to_b.with_amount(0_u128).as_ref(),
            )?;

            // The expected receiver will never receive the token.
            node_c.chain_driver().assert_eventual_wallet_amount(
                &wallet_c.address(),
                &denom_a_to_c.with_amount(0_u128).as_ref(),
            )?;
        }

        {
            info!("forward transfer with invalid `forward` field");

            node_a
                .chain_driver()
                .ibc_transfer_token_with_memo_and_timeout(
                    &channel_a_to_b,
                    &wallet_a,
                    &wallet_b.address(),
                    &vec![denom_a.with_amount(a_to_c_amount).as_ref()],
                    Some(memo4),
                    None,
                )?;

            info!(
                "check that only the sender lost {} tokens and the intermediary chain received {} tokens",
                a_to_c_amount,
                a_to_c_amount
            );

            node_a.chain_driver().assert_eventual_wallet_amount(
                &wallet_a.address(),
                &(balance_a - a_to_c_amount).as_ref(),
            )?;

            // As the memo doesn't have the format expected for forward transfer
            // the intermediary chain will keep the tokens.
            node_b.chain_driver().assert_eventual_wallet_amount(
                &wallet_b.address(),
                &denom_a_to_b.with_amount(a_to_c_amount).as_ref(),
            )?;

            // The expected receiver will never receive the token.
            node_c
                .chain_driver()
                .assert_eventual_wallet_amount(&wallet_c.address(), &(balance_c).as_ref())?;
        }

        info!("successfully test forward transfer with misspelled fields.");

        Ok(())
    }
}

struct MisspelledMemoContentIbcForwardTransferTest;

impl NaryChannelTest<3> for MisspelledMemoContentIbcForwardTransferTest {
    fn run<Handle: ChainHandle>(
        &self,
        _config: &TestConfig,
        _relayer: RelayerDriver,
        chains: NaryConnectedChains<Handle, 3>,
        channels: NaryConnectedChannels<Handle, 3>,
    ) -> Result<(), Error> {
        let connected_chains = chains.connected_chains_at::<0, 1>()?;

        let node_a = chains.full_node_at::<0>()?;
        let node_b = chains.full_node_at::<1>()?;
        let node_c = chains.full_node_at::<2>()?;

        let channel_a_to_b = channels.channel_at::<0, 1>()?;
        let channel_b_to_c = channels.channel_at::<1, 2>()?;

        let denom_a = connected_chains.node_a.denom();

        let denom_b = derive_ibc_denom(
            &channel_a_to_b.port_b.as_ref(),
            &channel_a_to_b.channel_id_b.as_ref(),
            &denom_a,
        )?;

        let denom_a_to_b = derive_ibc_denom(
            &channel_a_to_b.port_b.as_ref(),
            &channel_a_to_b.channel_id_b.as_ref(),
            &denom_a,
        )?;

        let denom_a_to_c = derive_ibc_denom(
            &channel_b_to_c.port_b.as_ref(),
            &channel_b_to_c.channel_id_b.as_ref(),
            &denom_b.as_ref(),
        )?;

        let wallets_a = node_a.wallets();
        let wallet_a = wallets_a.user1();

        let wallets_b = node_b.wallets();
        let wallet_b = wallets_b.user1();

        let wallets_c = node_c.wallets();
        let wallet_c = wallets_c.user1();

        let balance_a = node_a
            .chain_driver()
            .query_balance(&wallet_a.address(), &denom_a)?;

        let a_to_c_amount = 4000_u128;

        // Memo with field value: port: "misspelledPort".
        let memo_misspelled_port: MemoField<MemoInfo> = MemoField::new(
            wallet_c.address().value().to_string(),
            "misspelledPort".to_owned(),
            channel_b_to_c.channel.a_channel_id().unwrap().to_string(),
        );
        let memo1 = serde_json::to_string(&memo_misspelled_port).unwrap();

        // Memo with field value: channel: "misspelledChannel".
        let memo_misspelled_channel: MemoField<MemoInfo> = MemoField::new(
            wallet_c.address().value().to_string(),
            channel_b_to_c.port_a.to_string(),
            "misspelledChannel".to_owned(),
        );
        let memo2 = serde_json::to_string(&memo_misspelled_channel).unwrap();

        // Memo with field value: receiver: "misspelledReceiver".
        let memo_misspelled_receiver: MemoField<MemoInfo> = MemoField::new(
            "misspelledReceiver".to_owned(),
            channel_b_to_c.port_a.to_string(),
            channel_b_to_c.channel.a_channel_id().unwrap().to_string(),
        );
        let memo3 = serde_json::to_string(&memo_misspelled_receiver).unwrap();

        {
            info!("forward transfer with invalid port");

            node_a
                .chain_driver()
                .ibc_transfer_token_with_memo_and_timeout(
                    &channel_a_to_b,
                    &wallet_a,
                    &wallet_b.address(),
                    &vec![denom_a.with_amount(a_to_c_amount).as_ref()],
                    Some(memo2),
                    None,
                )?;

            // Wait before checking the balances
            std::thread::sleep(Duration::from_secs(10));

            info!("checking that the sender was refunded and other chains didn't receive tokens");

            // As the chain doesn't find the port for forwarding the transfer, the sender is
            // refunded
            node_a
                .chain_driver()
                .assert_eventual_wallet_amount(&wallet_a.address(), &(balance_a).as_ref())?;

            node_b.chain_driver().assert_eventual_wallet_amount(
                &wallet_b.address(),
                &denom_a_to_b.with_amount(0_u128).as_ref(),
            )?;

            // The expected receiver will never receive the token.
            node_c.chain_driver().assert_eventual_wallet_amount(
                &wallet_c.address(),
                &denom_a_to_c.with_amount(0_u128).as_ref(),
            )?;
        }

        {
            info!("forward transfer with invalid channel");

            node_a
                .chain_driver()
                .ibc_transfer_token_with_memo_and_timeout(
                    &channel_a_to_b,
                    &wallet_a,
                    &wallet_b.address(),
                    &vec![denom_a.with_amount(a_to_c_amount).as_ref()],
                    Some(memo3),
                    None,
                )?;

            // Wait before checking the balances
            std::thread::sleep(Duration::from_secs(10));

            info!("checking that the sender was refunded and other chains didn't receive tokens");

            // As the chain doesn't find the channel for forwarding the transfer, the sender is
            // refunded
            node_a
                .chain_driver()
                .assert_eventual_wallet_amount(&wallet_a.address(), &(balance_a).as_ref())?;

            node_b.chain_driver().assert_eventual_wallet_amount(
                &wallet_b.address(),
                &denom_a_to_b.with_amount(0_u128).as_ref(),
            )?;

            // The expected receiver will never receive the token.
            node_c.chain_driver().assert_eventual_wallet_amount(
                &wallet_c.address(),
                &denom_a_to_c.with_amount(0_u128).as_ref(),
            )?;
        }

        {
            info!("forward transfer with invalid receiver address");

            node_a
                .chain_driver()
                .ibc_transfer_token_with_memo_and_timeout(
                    &channel_a_to_b,
                    &wallet_a,
                    &wallet_b.address(),
                    &vec![denom_a.with_amount(a_to_c_amount).as_ref()],
                    Some(memo1),
                    None,
                )?;

            // Wait before checking the balances
            std::thread::sleep(Duration::from_secs(10));

            info!(
                "checking that the sender lost the tokens and other chains didn't receive tokens"
            );

            // Since the forward address is invalid, the sender is refunded
            node_a
                .chain_driver()
                .assert_eventual_wallet_amount(&wallet_a.address(), &(balance_a).as_ref())?;

            node_b.chain_driver().assert_eventual_wallet_amount(
                &wallet_b.address(),
                &denom_a_to_b.with_amount(0_u128).as_ref(),
            )?;

            // The expected receiver will never receive the token.
            node_c.chain_driver().assert_eventual_wallet_amount(
                &wallet_c.address(),
                &denom_a_to_c.with_amount(0_u128).as_ref(),
            )?;
        }

        info!("successfully tested forward transfer with misspelled memo content");

        Ok(())
    }
}

impl HasOverrides for IbcForwardTransferTest {
    type Overrides = IbcForwardTransferTestOverrides;

    fn get_overrides(&self) -> &IbcForwardTransferTestOverrides {
        &IbcForwardTransferTestOverrides
    }
}

impl HasOverrides for MisspelledMemoFieldsIbcForwardTransferTest {
    type Overrides = IbcForwardTransferTestOverrides;

    fn get_overrides(&self) -> &IbcForwardTransferTestOverrides {
        &IbcForwardTransferTestOverrides
    }
}

impl HasOverrides for MisspelledMemoContentIbcForwardTransferTest {
    type Overrides = IbcForwardTransferTestOverrides;

    fn get_overrides(&self) -> &IbcForwardTransferTestOverrides {
        &IbcForwardTransferTestOverrides
    }
}
