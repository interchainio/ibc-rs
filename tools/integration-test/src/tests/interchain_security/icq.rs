//! The following tests are for the Cross-chain Queries, ICS31.
//! These tests require the first chain to be a Gaia chain and
//! the second chain a Stride chain. Only the Stride chain requires
//! to have the ICS31 enabled.
//!
//! The test `ICS31Test` registers the cosmos account as a host-zone
//! using `strided tx stakeibc register-host-zone` in order to have
//! the Stride chain trigger Cross-chain Queries.
//! The test then waits for a Cross-chain Query to be pending and
//! then processed.

use ibc_relayer::config::ChainConfig;
use ibc_test_framework::chain::cli::host_zone::register_host_zone;
use ibc_test_framework::chain::config::{
    set_crisis_denom, set_mint_mint_denom, set_staking_bond_denom, set_staking_max_entries,
    set_voting_period,
};
use ibc_test_framework::chain::ext::crosschainquery::CrossChainQueryMethodsExt;
use ibc_test_framework::framework::binary::channel::run_binary_interchain_security_channel_test;
use ibc_test_framework::prelude::*;
use ibc_test_framework::relayer::channel::{
    assert_eventually_channel_established, query_identified_channel_ends,
};
use ibc_test_framework::util::interchain_security::{
    update_genesis_for_consumer_chain, update_relayer_config_for_consumer_chain,
};
use ibc_test_framework::util::random::random_u128_range;

#[test]
fn test_ics31_cross_chain_queries() -> Result<(), Error> {
    run_binary_interchain_security_channel_test(&InterchainSecurityIcqTest { allow_ccq: true })
}

#[test]
fn test_disable_ics31_cross_chain_queries() -> Result<(), Error> {
    run_binary_interchain_security_channel_test(&InterchainSecurityIcqTest { allow_ccq: false })
}

struct InterchainSecurityIcqTest {
    pub allow_ccq: bool,
}

impl TestOverrides for InterchainSecurityIcqTest {
    fn modify_genesis_file(&self, genesis: &mut serde_json::Value) -> Result<(), Error> {
        // Gaia chain genesis file doesn't have `epochs` key.
        if let Some(epochs_list) = genesis
            .get_mut("app_state")
            .and_then(|app_state| app_state.get_mut("epochs"))
            .and_then(|epochs| epochs.get_mut("epochs"))
            .and_then(|epochs_list| epochs_list.as_array_mut())
        {
            for v in epochs_list {
                let identifier = v
                    .get("identifier")
                    .ok_or_else(|| eyre!("failed to find identifier"))?;

                if identifier.as_str() == Some("stride_epoch") {
                    let duration = v
                        .get_mut("duration")
                        .ok_or_else(|| eyre!("failed to get duration"))?;

                    *duration = serde_json::Value::String("25s".to_owned());
                } else if identifier.as_str() == Some("day") {
                    // The stride epoch must be 1/4th the length of the day epoch
                    let duration = v
                        .get_mut("duration")
                        .ok_or_else(|| eyre!("failed to get duration"))?;

                    *duration = serde_json::Value::String("100s".to_owned());
                }
            }
            set_voting_period(genesis, 10)?;
            set_staking_max_entries(genesis, "10")?;
            set_staking_bond_denom(genesis, "stake")?;
            set_mint_mint_denom(genesis, "stake")?;
            set_crisis_denom(genesis, "stake")?;
        }

        update_genesis_for_consumer_chain(genesis)?;

        Ok(())
    }

    // When calling `strided tx stakeibc register-host-zone` new channel
    // will be created. So the channel worker needs to be enabled.
    fn modify_relayer_config(&self, config: &mut Config) {
        config.mode.clients.misbehaviour = false;
        config.mode.connections.enabled = true;
        config.mode.channels.enabled = true;

        update_relayer_config_for_consumer_chain(config);

        for chain in config.chains.iter_mut() {
            match chain {
                ChainConfig::CosmosSdk(chain_config) => {
                    chain_config.allow_ccq = self.allow_ccq;
                }
            }
        }
    }
}

impl BinaryChannelTest for InterchainSecurityIcqTest {
    fn run<ChainA: ChainHandle, ChainB: ChainHandle>(
        &self,
        _config: &TestConfig,
        _relayer: RelayerDriver,
        chains: ConnectedChains<ChainA, ChainB>,
        channel: ConnectedChannel<ChainA, ChainB>,
    ) -> Result<(), Error> {
        let denom_a = chains.node_a.denom();
        let a_to_b_amount = random_u128_range(1000, 5000);
        let wallet_a = chains.node_a.wallets().user1().cloned();
        let wallet_b = chains.node_b.wallets().user1().cloned();

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

        chains.node_b.chain_driver().assert_eventual_wallet_amount(
            &wallet_b.address(),
            &denom_b.with_amount(a_to_b_amount).as_ref(),
        )?;

        info!("Registering host-zone");
        // gaiad binary doesn't have the CLI `tx stakeibc register-host-zone`
        // if this method is not called with `strided` command_path it will
        // fail.
        register_host_zone(
            chains.chain_id_b().0.as_str(),
            chains.node_b.0.chain_driver.command_path.as_str(),
            chains.node_b.0.chain_driver.home_path.as_str(),
            chains.node_b.0.chain_driver.rpc_listen_address().as_str(),
            channel.channel.dst_connection_id().as_str(),
            denom_a.0.as_str(),
            "cosmos",
            denom_b.0.as_str(),
            channel.channel_id_a.0.as_str(),
            &wallet_b.0.id.to_string(),
        )?;

        // Wait for channel to initialise so that the query can find
        // all the channels related to registering a host-zone
        std::thread::sleep(Duration::from_secs(5));

        let channels = query_identified_channel_ends::<ChainA, ChainB>(chains.handle_a())?;

        // Wait for channel created by registering a host-zone to be Open
        for channel in channels.iter() {
            let tagged_channel_id: TaggedChannelId<ChainA, ChainB> =
                DualTagged::new(channel.0.channel_id.clone());
            let tagged_port_id: TaggedPortId<ChainA, ChainB> =
                DualTagged::new(channel.0.port_id.clone());

            if channel.0.port_id.as_str() == "icahost" {
                info!(
                    "Will assert that channel {}/{} is eventually Open",
                    channel.0.channel_id, channel.0.port_id
                );
                assert_eventually_channel_established(
                    chains.handle_a(),
                    chains.handle_b(),
                    &tagged_channel_id.as_ref(),
                    &tagged_port_id.as_ref(),
                )?;
            }
        }

        // Wait for the cross chain query to be pending.
        chains
            .node_b
            .chain_driver()
            .assert_pending_cross_chain_query()?;

        // After there is a pending cross chain query, wait for it to be processed
        let processed_ccqs = chains
            .node_b
            .chain_driver()
            .assert_processed_cross_chain_query();

        if self.allow_ccq {
            assert!(processed_ccqs.is_ok());
        } else {
            assert!(processed_ccqs.is_err());
        }

        Ok(())
    }
}
