//! This test tests two different cases:
//!
//! - The `IbcForwardHopTransferTest` tests the case a packet is successfully
//!   forwarded with a hop.
//!
//! - The `AtomicIbcForwardHopTransferTest` tests the case where the
//!   hop between chain C and D fails. In this case the sender is still refunded.

use ibc_relayer::config::{self, ModeConfig};
use ibc_test_framework::prelude::*;

use crate::tests::forward::memo::HopMemoField;

#[test]
fn test_ibc_forward_hop_transfer() -> Result<(), Error> {
    run_nary_channel_test(&IbcForwardHopTransferTest)
}

#[test]
fn test_atomic_ibc_forward_hop_transfer() -> Result<(), Error> {
    run_nary_channel_test(&AtomicIbcForwardHopTransferTest)
}

struct IbcForwardHopTransferTestOverrides;

impl TestOverrides for IbcForwardHopTransferTestOverrides {
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

impl PortsOverride<4> for IbcForwardHopTransferTestOverrides {}

struct IbcForwardHopTransferTest;

impl NaryChannelTest<4> for IbcForwardHopTransferTest {
    fn run<Handle: ChainHandle>(
        &self,
        _config: &TestConfig,
        _relayer: RelayerDriver,
        chains: NaryConnectedChains<Handle, 4>,
        channels: NaryConnectedChannels<Handle, 4>,
    ) -> Result<(), Error> {
        let connected_chains = chains.connected_chains_at::<0, 1>()?;

        let node_a = chains.full_node_at::<0>()?;
        let node_b = chains.full_node_at::<1>()?;
        let node_c = chains.full_node_at::<2>()?;
        let node_d = chains.full_node_at::<3>()?;

        let channel_a_to_b = channels.channel_at::<0, 1>()?;
        let channel_b_to_c = channels.channel_at::<1, 2>()?;
        let channel_c_to_d = channels.channel_at::<2, 3>()?;

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

        let denom_a_to_d = derive_ibc_denom(
            &channel_c_to_d.port_b.as_ref(),
            &channel_c_to_d.channel_id_b.as_ref(),
            &denom_a_to_c.as_ref(),
        )?;

        let wallets_a = node_a.wallets();
        let wallet_a = wallets_a.user1();

        let wallets_b = node_b.wallets();
        let wallet_b = wallets_b.user1();

        let wallets_c = node_c.wallets();
        let wallet_c = wallets_c.user1();

        let wallets_d = node_d.wallets();
        let wallet_d = wallets_d.user1();

        let balance_a = node_a
            .chain_driver()
            .query_balance(&wallet_a.address(), &denom_a)?;

        let a_to_d_amount = 4000_u128;

        let memo_field = HopMemoField::new(
            wallet_c.address().value().to_string(),
            channel_b_to_c.port_a.to_string(),
            channel_b_to_c.channel.a_channel_id().unwrap().to_string(),
            wallet_d.address().value().to_string(),
            channel_c_to_d.port_a.to_string(),
            channel_c_to_d.channel.a_channel_id().unwrap().to_string(),
        );
        let memo = serde_json::to_string(&memo_field).unwrap();

        node_a
            .chain_driver()
            .ibc_transfer_token_with_memo_and_timeout(
                &channel_a_to_b,
                &wallet_a,
                &wallet_b.address(),
                &vec![denom_a.with_amount(a_to_d_amount).as_ref()],
                Some(memo),
                None,
            )?;

        info!(
            "waiting for user on chain D to receive IBC transferred amount of {}",
            a_to_d_amount
        );

        node_d.chain_driver().assert_eventual_wallet_amount(
            &wallet_d.address(),
            &denom_a_to_d.with_amount(a_to_d_amount).as_ref(),
        )?;

        node_a.chain_driver().assert_eventual_wallet_amount(
            &wallet_a.address(),
            &(balance_a - a_to_d_amount).as_ref(),
        )?;

        node_b.chain_driver().assert_eventual_wallet_amount(
            &wallet_b.address(),
            &denom_a_to_b.with_amount(0_u128).as_ref(),
        )?;

        node_c.chain_driver().assert_eventual_wallet_amount(
            &wallet_c.address(),
            &denom_a_to_c.with_amount(0_u128).as_ref(),
        )?;

        info!(
            "successfully performed IBC transfer from chain {} to chain {}",
            chains.chain_handle_at::<0>().unwrap().value(),
            chains.chain_handle_at::<3>().unwrap().value(),
        );

        Ok(())
    }
}
struct AtomicIbcForwardHopTransferTest;

impl NaryChannelTest<4> for AtomicIbcForwardHopTransferTest {
    fn run<Handle: ChainHandle>(
        &self,
        _config: &TestConfig,
        _relayer: RelayerDriver,
        chains: NaryConnectedChains<Handle, 4>,
        channels: NaryConnectedChannels<Handle, 4>,
    ) -> Result<(), Error> {
        let connected_chains = chains.connected_chains_at::<0, 1>()?;

        let node_a = chains.full_node_at::<0>()?;
        let node_b = chains.full_node_at::<1>()?;
        let node_c = chains.full_node_at::<2>()?;
        let node_d = chains.full_node_at::<3>()?;

        let channel_a_to_b = channels.channel_at::<0, 1>()?;
        let channel_b_to_c = channels.channel_at::<1, 2>()?;
        let channel_c_to_d = channels.channel_at::<2, 3>()?;

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

        let denom_a_to_d = derive_ibc_denom(
            &channel_c_to_d.port_b.as_ref(),
            &channel_c_to_d.channel_id_b.as_ref(),
            &denom_a_to_c.as_ref(),
        )?;

        let wallets_a = node_a.wallets();
        let wallet_a = wallets_a.user1();

        let wallets_b = node_b.wallets();
        let wallet_b = wallets_b.user1();

        let wallets_c = node_c.wallets();
        let wallet_c = wallets_c.user1();

        let wallets_d = node_d.wallets();
        let wallet_d = wallets_d.user1();

        let balance_a = node_a
            .chain_driver()
            .query_balance(&wallet_a.address(), &denom_a)?;

        let a_to_d_amount = 4000_u128;

        let memo_field = HopMemoField::new(
            wallet_c.address().value().to_string(),
            channel_b_to_c.port_a.to_string(),
            channel_b_to_c.channel.a_channel_id().unwrap().to_string(),
            wallet_d.address().value().to_string(),
            channel_c_to_d.port_a.to_string(),
            "InvalidChannelFromCtoD".to_owned(),
        );
        let memo = serde_json::to_string(&memo_field).unwrap();

        node_a
            .chain_driver()
            .ibc_transfer_token_with_memo_and_timeout(
                &channel_a_to_b,
                &wallet_a,
                &wallet_b.address(),
                &vec![denom_a.with_amount(a_to_d_amount).as_ref()],
                Some(memo),
                None,
            )?;

        info!("checking that the sender was refunded and other chains didn't receive tokens");

        // Wait before checking the balances
        std::thread::sleep(Duration::from_secs(10));

        node_a
            .chain_driver()
            .assert_eventual_wallet_amount(&wallet_a.address(), &(balance_a).as_ref())?;

        node_b.chain_driver().assert_eventual_wallet_amount(
            &wallet_b.address(),
            &denom_a_to_b.with_amount(0_u128).as_ref(),
        )?;

        node_c.chain_driver().assert_eventual_wallet_amount(
            &wallet_c.address(),
            &denom_a_to_c.with_amount(0_u128).as_ref(),
        )?;

        node_d.chain_driver().assert_eventual_wallet_amount(
            &wallet_d.address(),
            &denom_a_to_d.with_amount(0_u128).as_ref(),
        )?;

        info!("successfully tested that packet forwarding is atomic.");

        Ok(())
    }
}

impl HasOverrides for IbcForwardHopTransferTest {
    type Overrides = IbcForwardHopTransferTestOverrides;

    fn get_overrides(&self) -> &IbcForwardHopTransferTestOverrides {
        &IbcForwardHopTransferTestOverrides
    }
}

impl HasOverrides for AtomicIbcForwardHopTransferTest {
    type Overrides = IbcForwardHopTransferTestOverrides;

    fn get_overrides(&self) -> &IbcForwardHopTransferTestOverrides {
        &IbcForwardHopTransferTestOverrides
    }
}
