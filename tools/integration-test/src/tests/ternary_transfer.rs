use ibc_test_framework::prelude::*;
use ibc_test_framework::types::topology::TopologyType;

#[test]
fn test_ternary_ibc_transfer() -> Result<(), Error> {
    run_nary_channel_test(&TernaryIbcTransferTest)
}

pub struct TernaryIbcTransferTest;

impl TestOverrides for TernaryIbcTransferTest {
    fn modify_relayer_config(&self, config: &mut Config) {
        config.mode.clients.misbehaviour = false;
    }

    fn topology(&self) -> Option<TopologyType> {
        Some(TopologyType::Cyclic)
    }
}

impl PortsOverride<3> for TernaryIbcTransferTest {}

impl NaryChannelTest<3> for TernaryIbcTransferTest {
    fn run<Handle: ChainHandle>(
        &self,
        _config: &TestConfig,
        _relayer: RelayerDriver,
        chains: NaryConnectedChains<Handle, 3>,
        channels: NaryConnectedChannels<Handle, 3>,
    ) -> Result<(), Error> {
        let node_a = chains.full_node_at::<0>()?;
        let node_b = chains.full_node_at::<1>()?;
        let node_c = chains.full_node_at::<2>()?;

        let denom_a = node_a.denom();

        let wallet_a1 = node_a.wallets().user1().cloned();

        let wallet_b1 = node_b.wallets().user1().cloned();
        let wallet_b2 = node_b.wallets().user2().cloned();

        let wallet_c1 = node_c.wallets().user1().cloned();

        let balance_a = node_a
            .chain_driver()
            .query_balance(&wallet_a1.address(), &denom_a)?;

        let a_to_b_amount = 5000u64;

        let channel_a_to_b = channels.channel_at::<0, 1>()?;

        info!(
            "Sending IBC transfer from chain {} to chain {} with amount of {} {}",
            node_a.chain_id(),
            node_b.chain_id(),
            a_to_b_amount,
            denom_a
        );

        node_a.chain_driver().ibc_transfer_token(
            &channel_a_to_b,
            &wallet_a1.as_ref(),
            &wallet_b1.address(),
            &vec![denom_a.with_amount(a_to_b_amount).as_ref()],
        )?;

        let denom_a_to_b = derive_ibc_denom(
            &channel_a_to_b.port_b.as_ref(),
            &channel_a_to_b.channel_id_b.as_ref(),
            &denom_a,
        )?;

        // Chain B will receive ibc/port-b/channel-b/denom

        info!(
            "Waiting for user on chain B to receive IBC transferred amount of {} {}",
            a_to_b_amount, denom_a_to_b
        );

        node_a.chain_driver().assert_eventual_wallet_amount(
            &wallet_a1.address(),
            &(balance_a - a_to_b_amount).as_ref(),
        )?;

        node_b.chain_driver().assert_eventual_wallet_amount(
            &wallet_b1.address(),
            &denom_a_to_b.with_amount(a_to_b_amount).as_ref(),
        )?;

        info!(
            "successfully performed IBC transfer from chain {} to chain {}",
            node_a.chain_id(),
            node_b.chain_id(),
        );

        let channel_b_to_c = channels.channel_at::<1, 2>()?;

        let denom_a_to_c = derive_ibc_denom(
            &channel_b_to_c.port_b.as_ref(),
            &channel_b_to_c.channel_id_b.as_ref(),
            &denom_a_to_b.as_ref(),
        )?;

        let b_to_c_amount = 2500;

        node_b.chain_driver().ibc_transfer_token(
            &channel_b_to_c,
            &wallet_b1.as_ref(),
            &wallet_c1.address(),
            &vec![denom_a_to_b.with_amount(b_to_c_amount).as_ref()],
        )?;

        // Chain C will receive ibc/port-c/channel-c/port-b/channel-b/denom

        info!(
            "Waiting for user on chain C to receive IBC transferred amount of {} {}",
            b_to_c_amount, denom_a_to_c
        );

        node_b.chain_driver().assert_eventual_wallet_amount(
            &wallet_b1.address(),
            &denom_a_to_b
                .with_amount(a_to_b_amount - b_to_c_amount)
                .as_ref(),
        )?;

        node_c.chain_driver().assert_eventual_wallet_amount(
            &wallet_c1.address(),
            &denom_a_to_c.with_amount(b_to_c_amount).as_ref(),
        )?;

        let channel_c_to_a = channels.channel_at::<2, 0>()?;

        let denom_a_to_c_to_a = derive_ibc_denom(
            &channel_c_to_a.port_b.as_ref(),
            &channel_c_to_a.channel_id_b.as_ref(),
            &denom_a_to_c.as_ref(),
        )?;

        let c_to_a_amount = 800;

        node_c.chain_driver().ibc_transfer_token(
            &channel_c_to_a,
            &wallet_c1.as_ref(),
            &wallet_a1.address(),
            &vec![denom_a_to_c.with_amount(c_to_a_amount).as_ref()],
        )?;

        // Chain A will receive ibc/port-a/channel-a/port-c/channel-c/port-b/channel-b/denom

        info!(
            "Waiting for user on chain A to receive IBC transferred amount of {} {}",
            c_to_a_amount, denom_a_to_c_to_a
        );

        node_c.chain_driver().assert_eventual_wallet_amount(
            &wallet_c1.address(),
            &denom_a_to_c
                .with_amount(b_to_c_amount - c_to_a_amount)
                .as_ref(),
        )?;

        node_a.chain_driver().assert_eventual_wallet_amount(
            &wallet_a1.address(),
            &denom_a_to_c_to_a.with_amount(c_to_a_amount).as_ref(),
        )?;

        let c_to_b_amount = 500;

        node_c.chain_driver().ibc_transfer_token(
            &channel_b_to_c.flip(),
            &wallet_c1.as_ref(),
            &wallet_b2.address(),
            &vec![denom_a_to_c.with_amount(c_to_b_amount).as_ref()],
        )?;

        // Chain B will receive ibc/port-b/channel-b/denom

        info!(
            "Waiting for user on chain B to receive IBC transferred amount of {} {}",
            c_to_b_amount, denom_a_to_b
        );

        node_c.chain_driver().assert_eventual_wallet_amount(
            &wallet_c1.address(),
            &denom_a_to_c
                .with_amount(b_to_c_amount - c_to_a_amount - c_to_b_amount)
                .as_ref(),
        )?;

        node_b.chain_driver().assert_eventual_wallet_amount(
            &wallet_b2.address(),
            &denom_a_to_b.with_amount(c_to_b_amount).as_ref(),
        )?;

        Ok(())
    }
}
