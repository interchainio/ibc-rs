use std::str::FromStr;

use abscissa_core::{Command, Options, Runnable};

use ibc::ics24_host::identifier::ChainId;
use ibc_relayer::{
    config::{ChainConfig, Config},
    keyring::{HDPath, KeyEntry, KeyRing, Store},
};

use crate::application::app_config;
use crate::conclude::Output;

#[derive(Clone, Command, Debug, Options)]
pub struct KeyRestoreCmd {
    #[options(free, required, help = "identifier of the chain")]
    chain_id: ChainId,

    #[options(short = "m", required, help = "mnemonic to restore the key from")]
    mnemonic: String,

    #[options(
        short = "n",
        help = "name of the key (defaults to the `key_name` defined in the config)"
    )]
    name: Option<String>,

    #[options(
        short = "p",
        help = "derivation path for this key",
        default = "m/44'/118'/0'/0/0"
    )]
    hd_path: String,
}

#[derive(Clone, Debug)]
pub struct KeysRestoreOptions {
    pub mnemonic: String,
    pub config: ChainConfig,
    pub hd_path: HDPath,
    pub key_name: String,
}

impl KeyRestoreCmd {
    fn validate_options(&self, config: &Config) -> Result<KeysRestoreOptions, String> {
        let chain_config = config
            .find_chain(&self.chain_id)
            .ok_or_else(|| format!("chain '{}' not found in configuration file", self.chain_id))?;

        let hd_path = HDPath::from_str(&self.hd_path)
            .map_err(|_| format!("invalid derivation path: {}", self.hd_path))?;

        let key_name = self
            .name
            .clone()
            .unwrap_or_else(|| chain_config.key_name.clone());

        Ok(KeysRestoreOptions {
            mnemonic: self.mnemonic.clone(),
            config: chain_config.clone(),
            hd_path,
            key_name,
        })
    }
}

impl Runnable for KeyRestoreCmd {
    fn run(&self) {
        let config = app_config();

        let opts = match self.validate_options(&config) {
            Err(err) => return Output::error(err).exit(),
            Ok(result) => result,
        };

        let key = restore_key(&opts.mnemonic, &opts.key_name, &opts.hd_path, &opts.config);

        match key {
            Ok(key) => Output::success_msg(format!(
                "Restored key '{}' ({}) on chain {}",
                opts.key_name, key.account, opts.config.id
            ))
            .exit(),
            Err(e) => Output::error(format!("{}", e)).exit(),
        }
    }
}

pub fn restore_key(
    mnemonic: &str,
    key_name: &str,
    hdpath: &HDPath,
    config: &ChainConfig,
) -> Result<KeyEntry, Box<dyn std::error::Error>> {
    let mut keyring = KeyRing::new(Store::Test, &config.account_prefix, &config.id)?;
    let key_entry = keyring.key_from_mnemonic(mnemonic, hdpath, &config.address_type)?;

    keyring.add_key(key_name, key_entry.clone())?;
    Ok(key_entry)
}
