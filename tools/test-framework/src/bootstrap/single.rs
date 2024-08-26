/*!
   Helper functions for bootstrapping a single full node.
*/
use core::time::Duration;
use eyre::eyre;
use std::sync::{Arc, RwLock};
use tracing::info;

use crate::chain::builder::ChainBuilder;
use crate::chain::config;
use crate::chain::driver::ChainDriver;
use crate::chain::ext::bootstrap::ChainBootstrapMethodsExt;
use crate::error::Error;
use crate::ibc::denom::Denom;
use crate::ibc::token::Token;
use crate::types::single::node::FullNode;
use crate::types::wallet::{TestWallets, Wallet};
use crate::util::random::{random_u128_range, random_u32};

/**
   Bootstrap a single full node with the provided [`ChainBuilder`] and
   a prefix for the chain ID.

   The function would generate random postfix attached to the end of
   a chain ID. So for example having a prefix `"alpha"` may generate
   a chain with an ID  like `"ibc-alpha-f5a2a988"`

   The bootstrap function also tries to use as many random parameters
   when initializing the chain, such as using random denomination
   and wallets. This is to help ensure that the test is written to
   only work with specific hardcoded parameters.

   TODO: Due to the limitation of the `gaiad` command, currently
   parameters such as the stake denomination (`stake`) and the wallet
   address prefix (`cosmos`) cannot be overridden. It would be
   great to be able to randomize these parameters in the future
   as well.
*/
pub fn bootstrap_single_node(
    builder: &ChainBuilder,
    prefix: &str,
    use_random_id: bool,
    config_modifier: impl FnOnce(&mut toml::Value) -> Result<(), Error>,
    genesis_modifier: impl FnOnce(&mut serde_json::Value) -> Result<(), Error>,
    chain_number: usize,
) -> Result<FullNode, Error> {
    let native_token_number = chain_number % builder.native_tokens.len();
    let native_token = &builder.native_tokens[native_token_number];
    let native_denom = Denom::base(native_token);

    let denom = if use_random_id {
        Denom::base(&format!("coin{:x}", random_u32()))
    } else {
        Denom::base("samoleans")
    };

    let second_denom = if use_random_id {
        Denom::base(&format!("coinv2{:x}", random_u32()))
    } else {
        Denom::base("samoleansv2")
    };

    // Evmos requires of at least 1_000_000_000_000_000_000 or else there will be the
    // error `error during handshake: error on replay: validator set is nil in genesis and still empty after InitChain`
    // when running `evmosd start`.
    let initial_amount = random_u128_range(1_000_000_000_000_000_000, 2_000_000_000_000_000_000);
    let second_initial_amount =
        random_u128_range(1_000_000_000_000_000_000, 2_000_000_000_000_000_000);

    let initial_native_token = Token::new(native_denom, initial_amount);
    let additional_native_token = initial_native_token
        .clone()
        .checked_add(1_000_000_000_000u64)
        .ok_or(Error::generic(eyre!(
            "error creating initial {} with additional amount",
            native_token
        )))?;
    let initial_coin = Token::new(denom.clone(), initial_amount);
    let second_initial_coin = Token::new(second_denom.clone(), second_initial_amount);

    let chain_driver = builder.new_chain(prefix, use_random_id, chain_number)?;

    chain_driver.initialize()?;

    chain_driver.update_genesis_file("genesis.json", genesis_modifier)?;

    let validator = add_wallet(&chain_driver, "validator", use_random_id)?;
    let relayer = add_wallet(&chain_driver, "relayer", use_random_id)?;
    let user1 = add_wallet(&chain_driver, "user1", use_random_id)?;
    let user2 = add_wallet(&chain_driver, "user2", use_random_id)?;

    // Validator is given more tokens as they are required to vote on upgrade chain
    chain_driver.add_genesis_account(&validator.address, &[&additional_native_token])?;

    chain_driver.add_genesis_validator(&validator.id, &initial_native_token)?;

    chain_driver.add_genesis_account(
        &user1.address,
        &[&initial_native_token, &initial_coin, &second_initial_coin],
    )?;

    chain_driver.add_genesis_account(
        &user2.address,
        &[&initial_native_token, &initial_coin, &second_initial_coin],
    )?;

    chain_driver.add_genesis_account(
        &relayer.address,
        &[&initial_native_token, &initial_coin, &second_initial_coin],
    )?;

    chain_driver.collect_gen_txs()?;

    let log_level = std::env::var("CHAIN_LOG_LEVEL").unwrap_or_else(|_| "info".to_string());

    chain_driver.update_chain_config("config.toml", |config| {
        config::set_log_level(config, &log_level)?;
        config::set_rpc_port(config, chain_driver.rpc_port)?;
        config::set_p2p_port(config, chain_driver.p2p_port)?;
        config::set_pprof_port(config, chain_driver.pprof_port)?;
        config::set_timeout_commit(config, Duration::from_secs(1))?;
        config::set_timeout_propose(config, Duration::from_secs(1))?;
        config::set_mode(config, "validator")?;
        config::set_indexer(config, "kv")?;

        config_modifier(config)?;

        Ok(())
    })?;

    let minimum_gas = format!("0{}", native_token);
    chain_driver.update_chain_config("app.toml", |config| {
        config::set_grpc_port(config, chain_driver.grpc_port)?;
        config::enable_grpc(config)?;
        config::disable_grpc_web(config)?;
        config::disable_api(config)?;
        config::set_minimum_gas_price(config, &minimum_gas)?;

        Ok(())
    })?;

    let process = chain_driver.start()?;

    chain_driver.assert_eventual_wallet_amount(&relayer.address, &initial_coin)?;

    info!(
        "started new chain {} at with home path {} and RPC address {}.",
        chain_driver.chain_id,
        chain_driver.home_path,
        chain_driver.rpc_address(),
    );

    info!(
        "user wallet for chain {} - id: {}, address: {}",
        chain_driver.chain_id, user1.id.0, user1.address.0,
    );

    info!(
        "you can manually interact with the chain using commands starting with:\n{} --home '{}' --node {}",
        chain_driver.command_path,
        chain_driver.home_path,
        chain_driver.rpc_address(),
    );

    let wallets = TestWallets {
        validator,
        relayer,
        user1,
        user2,
    };

    let node = FullNode {
        chain_driver,
        denom,
        second_denom,
        wallets,
        process: Arc::new(RwLock::new(process)),
    };

    Ok(node)
}

fn add_wallet(driver: &ChainDriver, prefix: &str, use_random_id: bool) -> Result<Wallet, Error> {
    if use_random_id {
        let num = random_u32();
        let wallet_id = format!("{prefix}-{num:x}");
        driver.add_wallet(&wallet_id)
    } else {
        driver.add_wallet(prefix)
    }
}
