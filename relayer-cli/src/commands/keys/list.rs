use alloc::collections::btree_map::BTreeMap as HashMap;

use abscissa_core::{Command, Options, Runnable};

use ibc::ics24_host::identifier::ChainId;
use ibc_relayer::{
    config::{ChainConfig, Config},
    keyring::{KeyEntry, KeyRing, Store},
};

use crate::conclude::Output;
use crate::{application::app_config, conclude::json};

#[derive(Clone, Command, Debug, Options)]
pub struct KeysListCmd {
    #[options(free, required, help = "identifier of the chain")]
    chain_id: ChainId,
}

impl KeysListCmd {
    fn options(&self, config: &Config) -> Result<KeysListOptions, String> {
        let chain_config = config
            .find_chain(&self.chain_id)
            .ok_or_else(|| format!("chain '{}' not found in configuration file", self.chain_id))?;

        Ok(KeysListOptions {
            chain_config: chain_config.clone(),
        })
    }
}

impl Runnable for KeysListCmd {
    fn run(&self) {
        let config = app_config();

        let opts = match self.options(&config) {
            Err(err) => return Output::error(err).exit(),
            Ok(result) => result,
        };

        match list_keys(opts.chain_config) {
            Ok(keys) if json() => {
                let keys = keys.into_iter().collect::<HashMap<_, _>>();
                Output::success(keys).exit()
            }
            Ok(keys) => {
                let mut msg = String::new();
                for (name, key) in keys {
                    msg.push_str(&format!("\n- {} ({})", name, key.account));
                }
                Output::success_msg(msg).exit()
            }
            Err(e) => Output::error(e).exit(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct KeysListOptions {
    pub chain_config: ChainConfig,
}

pub fn list_keys(
    config: ChainConfig,
) -> Result<Vec<(String, KeyEntry)>, Box<dyn std::error::Error>> {
    let keyring = KeyRing::new(Store::Test, &config.account_prefix, &config.id)?;
    let keys = keyring.keys()?;
    Ok(keys)
}
