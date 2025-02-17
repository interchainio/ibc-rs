//! This test tests four different cases:
//!
//! - The `ClientUpgradeTest` tests the case where the client upgrade works
//!   correctly after the chain was upgraded.
//!
//! - The `InvalidClientUpgradeTest` tests the case where the
//!   client upgrade fails as the chain has not been upgraded.
//!
//! - The `HeightTooHighClientUpgradeTest`tests the case where the client
//!   fails to upgrade because a height too big is given as input.
//!
//! - The `test_height_too_low_client_upgrade`tests the case where the client
//!   fails to upgrade because a height too small is given as input.

use http::Uri;
use std::str::FromStr;

use ibc_relayer::chain::requests::IncludeProof;
use ibc_relayer::chain::requests::QueryClientStateRequest;
use ibc_relayer::chain::requests::QueryHeight;
use ibc_relayer::client_state::AnyClientState;
use ibc_relayer::upgrade_chain::{build_and_send_ibc_upgrade_proposal, UpgradePlanOptions};
use ibc_relayer_types::core::ics02_client::height::Height;
use ibc_test_framework::chain::config::cosmos::{
    set_max_deposit_period, set_min_deposit_amount, set_voting_period,
};
use ibc_test_framework::chain::ext::bootstrap::ChainBootstrapMethodsExt;
use ibc_test_framework::prelude::*;
use ibc_test_framework::util::proposal_status::ProposalStatus;

const MAX_DEPOSIT_PERIOD: &str = "10s";
const VOTING_PERIOD: u64 = 10;
const DELTA_HEIGHT: u64 = 15;
const WAIT_CHAIN_UPGRADE: Duration = Duration::from_secs(4);
const WAIT_CHAIN_HEIGHT: Duration = Duration::from_secs(3);
const MIN_DEPOSIT: u64 = 10000000u64;

#[test]
fn test_client_upgrade() -> Result<(), Error> {
    run_binary_chain_test(&ClientUpgradeTest)
}

#[test]
fn test_invalid_client_upgrade() -> Result<(), Error> {
    run_binary_chain_test(&InvalidClientUpgradeTest)
}

#[test]
fn test_height_too_high_client_upgrade() -> Result<(), Error> {
    run_binary_chain_test(&HeightTooHighClientUpgradeTest)
}

#[test]
fn test_height_too_low_client_upgrade() -> Result<(), Error> {
    run_binary_chain_test(&HeightTooLowClientUpgradeTest)
}

struct ClientUpgradeTestOverrides;

struct ClientUpgradeTest;

impl TestOverrides for ClientUpgradeTestOverrides {
    /// Update the genesis file in order to reduce the time required to upgrade the chain.
    fn modify_genesis_file(&self, genesis: &mut serde_json::Value) -> Result<(), Error> {
        set_max_deposit_period(genesis, MAX_DEPOSIT_PERIOD)?;
        set_voting_period(genesis, VOTING_PERIOD)?;
        // Set the min deposit amount the same as the deposit of the Upgrade proposal to
        // assure that the proposal will go to voting period
        set_min_deposit_amount(genesis, MIN_DEPOSIT)?;
        Ok(())
    }
}

impl BinaryChainTest for ClientUpgradeTest {
    fn run<
        ChainA: ibc_test_framework::prelude::ChainHandle,
        ChainB: ibc_test_framework::prelude::ChainHandle,
    >(
        &self,
        config: &ibc_test_framework::prelude::TestConfig,
        _relayer: ibc_test_framework::prelude::RelayerDriver,
        chains: ibc_test_framework::prelude::ConnectedChains<ChainA, ChainB>,
    ) -> Result<(), ibc_test_framework::prelude::Error> {
        let upgraded_chain_id = ChainId::new("upgradedibc".to_owned(), 1);
        let fee_denom_a: MonoTagged<ChainA, Denom> = MonoTagged::new(Denom::base(
            &config.native_tokens[0],
            &config.native_tokens[0],
        ));
        let foreign_clients = chains.clone().foreign_clients;

        // Create and send a chain upgrade proposal
        let opts = create_upgrade_plan(config, &chains, &upgraded_chain_id)?;

        build_and_send_ibc_upgrade_proposal(
            chains.handle_a().clone(),
            chains.handle_b().clone(),
            &opts,
        )
        .map_err(Error::upgrade_chain)?;

        info!("Assert that the chain upgrade proposal is eventually in voting period");

        let driver = chains.node_a.chain_driver();

        driver.value().assert_proposal_status(
            driver.value().chain_id.as_str(),
            &driver.value().command_path,
            &driver.value().home_path,
            &driver.value().rpc_listen_address(),
            ProposalStatus::VotingPeriod,
            "1",
        )?;

        // Retrieve the height which should be used to upgrade the client
        let upgrade_height = driver.query_upgrade_proposal_height(
            &Uri::from_str(&driver.value().grpc_address()).map_err(handle_generic_error)?,
            1,
        )?;

        let client_upgrade_height = Height::new(
            foreign_clients.client_a_to_b.src_chain().id().version(),
            upgrade_height,
        )
        .map_err(handle_generic_error)?;

        // Vote on the proposal so the chain will upgrade
        driver.vote_proposal(&fee_denom_a.with_amount(381000000u64).to_string(), "1")?;

        info!("Assert that the chain upgrade proposal is eventually passed");

        driver.value().assert_proposal_status(
            driver.value().chain_id.as_str(),
            &driver.value().command_path,
            &driver.value().home_path,
            &driver.value().rpc_listen_address(),
            ProposalStatus::Passed,
            "1",
        )?;

        let halt_height = (client_upgrade_height - 1).unwrap();

        // Wait for the chain to get to the halt height
        loop {
            let latest_height = chains.handle_a().query_latest_height()?;
            info!("latest height: {latest_height}");

            if latest_height >= halt_height {
                break;
            }
            std::thread::sleep(WAIT_CHAIN_HEIGHT);
        }
        // Wait for an additional height which is required to fetch
        // the header
        std::thread::sleep(WAIT_CHAIN_HEIGHT);

        // Trigger the client upgrade
        // The error is ignored as the client state will be asserted afterwards.
        let _ = foreign_clients.client_a_to_b.upgrade(client_upgrade_height);

        // Wait to seconds before querying the client state
        std::thread::sleep(WAIT_CHAIN_UPGRADE);

        let (state, _) = chains.handle_b().query_client_state(
            QueryClientStateRequest {
                client_id: foreign_clients.client_a_to_b.id().clone(),
                height: QueryHeight::Latest,
            },
            IncludeProof::No,
        )?;

        match state {
            AnyClientState::Tendermint(client_state) => {
                assert_eq!(client_state.chain_id, upgraded_chain_id);
                Ok(())
            }
        }
    }
}

struct InvalidClientUpgradeTest;

/// The genesis file doesn't need to be updated since the chain isn't upgraded
/// for this test.
impl TestOverrides for InvalidClientUpgradeTest {}

impl BinaryChainTest for InvalidClientUpgradeTest {
    fn run<
        ChainA: ibc_test_framework::prelude::ChainHandle,
        ChainB: ibc_test_framework::prelude::ChainHandle,
    >(
        &self,
        _config: &ibc_test_framework::prelude::TestConfig,
        _relayer: ibc_test_framework::prelude::RelayerDriver,
        chains: ibc_test_framework::prelude::ConnectedChains<ChainA, ChainB>,
    ) -> Result<(), ibc_test_framework::prelude::Error> {
        let arbitrary_height = 5u64;
        let foreign_clients = &chains.foreign_clients;

        // Wait for the chain to reach a given height
        let client_upgrade_height = Height::new(
            foreign_clients.client_a_to_b.src_chain().id().version(),
            arbitrary_height,
        )
        .map_err(handle_generic_error)?;

        // Wait a bit before trying to upgrade the client
        std::thread::sleep(Duration::from_secs(2));

        // Trigger the client upgrade
        // The error is ignored as the client state will be asserted afterwards.
        let _ = foreign_clients.client_a_to_b.upgrade(client_upgrade_height);

        // Wait to seconds before querying the client state
        std::thread::sleep(Duration::from_secs(2));

        let (state, _) = chains.handle_b().query_client_state(
            QueryClientStateRequest {
                client_id: foreign_clients.client_a_to_b.id().clone(),
                height: QueryHeight::Latest,
            },
            IncludeProof::No,
        )?;

        match state {
            AnyClientState::Tendermint(client_state) => {
                assert_eq!(client_state.chain_id, chains.handle_a().id());
                Ok(())
            }
        }
    }
}

struct HeightTooHighClientUpgradeTest;

impl BinaryChainTest for HeightTooHighClientUpgradeTest {
    fn run<
        ChainA: ibc_test_framework::prelude::ChainHandle,
        ChainB: ibc_test_framework::prelude::ChainHandle,
    >(
        &self,
        config: &ibc_test_framework::prelude::TestConfig,
        _relayer: ibc_test_framework::prelude::RelayerDriver,
        chains: ibc_test_framework::prelude::ConnectedChains<ChainA, ChainB>,
    ) -> Result<(), ibc_test_framework::prelude::Error> {
        let upgraded_chain_id = ChainId::new("upgradedibc".to_owned(), 1);
        let fee_denom_a: MonoTagged<ChainA, Denom> = MonoTagged::new(Denom::base(
            &config.native_tokens[0],
            &config.native_tokens[0],
        ));
        let foreign_clients = chains.clone().foreign_clients;

        // Create and send a chain upgrade proposal
        let opts = create_upgrade_plan(config, &chains, &upgraded_chain_id)?;

        build_and_send_ibc_upgrade_proposal(
            chains.handle_a().clone(),
            chains.handle_b().clone(),
            &opts,
        )
        .map_err(Error::upgrade_chain)?;

        info!("Assert that the chain upgrade proposal is eventually in voting period");

        let driver = chains.node_a.chain_driver();

        driver.value().assert_proposal_status(
            driver.value().chain_id.as_str(),
            &driver.value().command_path,
            &driver.value().home_path,
            &driver.value().rpc_listen_address(),
            ProposalStatus::VotingPeriod,
            "1",
        )?;

        // Retrieve the height which should be used to upgrade the client
        let upgrade_height = driver.query_upgrade_proposal_height(
            &Uri::from_str(&driver.0.grpc_address()).map_err(handle_generic_error)?,
            1,
        )?;

        let client_upgrade_height = Height::new(
            foreign_clients.client_a_to_b.src_chain().id().version(),
            upgrade_height,
        )
        .map_err(handle_generic_error)?;

        // Vote on the proposal so the chain will upgrade
        driver.vote_proposal(&fee_denom_a.with_amount(381000000u64).to_string(), "1")?;

        // The application height reports a height of 1 less than the height according to Tendermint
        client_upgrade_height.increment();

        info!("Assert that the chain upgrade proposal is eventually passed");

        driver.value().assert_proposal_status(
            driver.value().chain_id.as_str(),
            &driver.value().command_path,
            &driver.value().home_path,
            &driver.value().rpc_listen_address(),
            ProposalStatus::Passed,
            "1",
        )?;

        // Wait for the chain to upgrade
        std::thread::sleep(WAIT_CHAIN_UPGRADE);

        // Trigger the client upgrade using client_upgrade_height + 1.
        // The error is ignored as the client state will be asserted afterwards.
        let _ = foreign_clients
            .client_a_to_b
            .upgrade(client_upgrade_height.increment());

        // Wait to seconds before querying the client state
        std::thread::sleep(Duration::from_secs(2));

        let (state, _) = chains.handle_b().query_client_state(
            QueryClientStateRequest {
                client_id: foreign_clients.client_a_to_b.id().clone(),
                height: QueryHeight::Latest,
            },
            IncludeProof::No,
        )?;

        match state {
            AnyClientState::Tendermint(client_state) => {
                assert_eq!(client_state.chain_id, chains.handle_a().id());
                Ok(())
            }
        }
    }
}

struct HeightTooLowClientUpgradeTest;

impl BinaryChainTest for HeightTooLowClientUpgradeTest {
    fn run<
        ChainA: ibc_test_framework::prelude::ChainHandle,
        ChainB: ibc_test_framework::prelude::ChainHandle,
    >(
        &self,
        config: &ibc_test_framework::prelude::TestConfig,
        _relayer: ibc_test_framework::prelude::RelayerDriver,
        chains: ibc_test_framework::prelude::ConnectedChains<ChainA, ChainB>,
    ) -> Result<(), ibc_test_framework::prelude::Error> {
        let upgraded_chain_id = ChainId::new("upgradedibc".to_owned(), 1);
        let fee_denom_a: MonoTagged<ChainA, Denom> = MonoTagged::new(Denom::base(
            &config.native_tokens[0],
            &config.native_tokens[0],
        ));
        let foreign_clients = chains.clone().foreign_clients;

        let opts = create_upgrade_plan(config, &chains, &upgraded_chain_id)?;

        build_and_send_ibc_upgrade_proposal(
            chains.handle_a().clone(),
            chains.handle_b().clone(),
            &opts,
        )
        .map_err(Error::upgrade_chain)?;

        info!("Assert that the chain upgrade proposal is eventually in voting period");

        let driver = chains.node_a.chain_driver();

        driver.value().assert_proposal_status(
            driver.value().chain_id.as_str(),
            &driver.value().command_path,
            &driver.value().home_path,
            &driver.value().rpc_listen_address(),
            ProposalStatus::VotingPeriod,
            "1",
        )?;

        // Retrieve the height which should be used to upgrade the client
        let upgrade_height = driver.query_upgrade_proposal_height(
            &Uri::from_str(&driver.value().grpc_address()).map_err(handle_generic_error)?,
            1,
        )?;

        let client_upgrade_height = Height::new(
            foreign_clients.client_a_to_b.src_chain().id().version(),
            upgrade_height,
        )
        .map_err(handle_generic_error)?;

        // Vote on the proposal so the chain will upgrade
        driver.vote_proposal(&fee_denom_a.with_amount(381000000u64).to_string(), "1")?;

        // The application height reports a height of 1 less than the height according to Tendermint
        client_upgrade_height
            .decrement()
            .expect("Upgrade height cannot be 1");

        info!("Assert that the chain upgrade proposal is eventually passed");

        driver.value().assert_proposal_status(
            driver.value().chain_id.as_str(),
            &driver.value().command_path,
            &driver.value().home_path,
            &driver.value().rpc_listen_address(),
            ProposalStatus::Passed,
            "1",
        )?;

        // Wait for the chain to upgrade
        std::thread::sleep(WAIT_CHAIN_UPGRADE);

        // Trigger the client upgrade using client_upgrade_height - 1.
        // The error is ignored as the client state will be asserted afterwards.
        let _ = foreign_clients.client_a_to_b.upgrade(
            client_upgrade_height
                .decrement()
                .map_err(handle_generic_error)?,
        );

        // Wait to seconds before querying the client state
        std::thread::sleep(Duration::from_secs(2));

        let (state, _) = chains.handle_b().query_client_state(
            QueryClientStateRequest {
                client_id: foreign_clients.client_a_to_b.id().clone(),
                height: QueryHeight::Latest,
            },
            IncludeProof::No,
        )?;

        match state {
            AnyClientState::Tendermint(client_state) => {
                assert_eq!(client_state.chain_id, chains.handle_a().id());
                Ok(())
            }
        }
    }
}

fn create_upgrade_plan<ChainA: ChainHandle, ChainB: ChainHandle>(
    config: &ibc_test_framework::prelude::TestConfig,
    chains: &ibc_test_framework::prelude::ConnectedChains<ChainA, ChainB>,
    upgraded_chain_id: &ChainId,
) -> Result<UpgradePlanOptions, Error> {
    let fee_denom_a: MonoTagged<ChainA, Denom> = MonoTagged::new(Denom::base(
        &config.native_tokens[0],
        &config.native_tokens[0],
    ));
    let foreign_clients = chains.clone().foreign_clients;

    let src_client_id = foreign_clients.client_id_b().0.clone();

    let gov_account = chains.node_a.chain_driver().query_auth_module("gov")?;
    // Create and send a chain upgrade proposal
    Ok(UpgradePlanOptions {
        src_client_id,
        amount: MIN_DEPOSIT,
        denom: fee_denom_a.to_string(),
        height_offset: DELTA_HEIGHT,
        upgraded_chain_id: upgraded_chain_id.clone(),
        upgraded_unbonding_period: None,
        upgrade_plan_name: "plan".to_string(),
        gov_account,
    })
}

impl HasOverrides for ClientUpgradeTest {
    type Overrides = ClientUpgradeTestOverrides;

    fn get_overrides(&self) -> &ClientUpgradeTestOverrides {
        &ClientUpgradeTestOverrides
    }
}

impl HasOverrides for HeightTooLowClientUpgradeTest {
    type Overrides = ClientUpgradeTestOverrides;

    fn get_overrides(&self) -> &ClientUpgradeTestOverrides {
        &ClientUpgradeTestOverrides
    }
}

impl HasOverrides for HeightTooHighClientUpgradeTest {
    type Overrides = ClientUpgradeTestOverrides;

    fn get_overrides(&self) -> &ClientUpgradeTestOverrides {
        &ClientUpgradeTestOverrides
    }
}
