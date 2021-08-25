use core::str::FromStr;
use std::{
    fs,
    path::{Path, PathBuf},
};

use abscissa_core::{Command, Options, Runnable};

use ibc::ics24_host::identifier::ChainId;
use ibc_relayer::{
    config::{ChainConfig, Config},
    keyring::{HDPath, KeyEntry, KeyRing, Store},
};

use crate::application::app_config;
use crate::conclude::Output;

#[derive(Clone, Command, Debug, Options)]
pub struct KeysAddCmd {
    #[options(free, required, help = "identifier of the chain")]
    chain_id: ChainId,

    #[options(short = "f", required, help = "path to the key file")]
    file: PathBuf,

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

impl KeysAddCmd {
    fn options(&self, config: &Config) -> Result<KeysAddOptions, Box<dyn std::error::Error>> {
        let chain_config = config
            .find_chain(&self.chain_id)
            .ok_or_else(|| format!("chain '{}' not found in configuration file", self.chain_id))?;

        let name = self
            .name
            .clone()
            .unwrap_or_else(|| chain_config.key_name.clone());

        let hd_path = HDPath::from_str(&self.hd_path)
            .map_err(|_| format!("invalid derivation path: {}", self.hd_path))?;

        Ok(KeysAddOptions {
            config: chain_config.clone(),
            file: self.file.clone(),
            name,
            hd_path,
        })
    }
}

#[derive(Clone, Debug)]
pub struct KeysAddOptions {
    pub name: String,
    pub config: ChainConfig,
    pub file: PathBuf,
    pub hd_path: HDPath,
}

impl Runnable for KeysAddCmd {
    fn run(&self) {
        let config = app_config();

        let opts = match self.options(&config) {
            Err(err) => return Output::error(err).exit(),
            Ok(result) => result,
        };

        let key = add_key(&opts.config, &opts.name, &opts.file, &opts.hd_path);

        match key {
            Ok(key) => Output::success_msg(format!(
                "Added key '{}' ({}) on chain {}",
                opts.name, key.account, opts.config.id
            ))
            .exit(),
            Err(e) => Output::error(format!("{}", e)).exit(),
        }
    }
}

pub fn add_key(
    config: &ChainConfig,
    key_name: &str,
    file: &Path,
    hd_path: &HDPath,
) -> Result<KeyEntry, Box<dyn std::error::Error>> {
    let mut keyring = KeyRing::new(Store::Test, &config.account_prefix, &config.id)?;

    let key_contents = fs::read_to_string(file).map_err(|_| "error reading the key file")?;
    let key = keyring.key_from_seed_file(&key_contents, hd_path)?;

    keyring.add_key(key_name, key.clone())?;
    Ok(key)
}
