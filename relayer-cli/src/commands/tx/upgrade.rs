use alloc::sync::Arc;
use core::time::Duration;

use abscissa_core::{Command, Options, Runnable};
use tokio::runtime::Runtime as TokioRuntime;

use ibc::events::IbcEvent;
use ibc::ics24_host::identifier::{ChainId, ClientId};
use ibc_relayer::upgrade_chain::{build_and_send_ibc_upgrade_proposal, UpgradePlanOptions};
use ibc_relayer::{
    chain::{ChainEndpoint, CosmosSdkChain},
    config::Config,
};

use crate::conclude::Output;
use crate::error::Error;
use crate::prelude::*;

#[derive(Clone, Command, Debug, Options)]
pub struct TxIbcUpgradeChainCmd {
    #[options(free, required, help = "identifier of the chain to upgrade")]
    dst_chain_id: ChainId,

    #[options(free, required, help = "identifier of the source chain")]
    src_chain_id: ChainId,

    #[options(
        free,
        required,
        help = "identifier of the client on source chain from which the plan is created"
    )]
    src_client_id: ClientId,

    #[options(free, required, help = "amount of stake")]
    amount: u64,

    #[options(
        free,
        required,
        help = "upgrade height offset in number of blocks since current"
    )]
    height_offset: u64,

    #[options(
        short = "c",
        meta = "CHAIN-ID",
        help = "new chain identifier to assign to the upgrading chain (optional)"
    )]
    new_chain_id: Option<ChainId>,

    #[options(
        short = "u",
        meta = "PERIOD",
        help = "new unbonding period to assign to the upgrading chain, in seconds (optional)"
    )]
    new_unbonding: Option<u64>,

    #[options(
        short = "n",
        meta = "NAME",
        help = "a string to name the upgrade proposal plan (default: 'plan')"
    )]
    upgrade_name: Option<String>,

    #[options(
        help = "use legacy upgrade proposal constructs (for chains built with Cosmos SDK < v0.43.0)",
        short = "l"
    )]
    legacy: bool,
}

impl TxIbcUpgradeChainCmd {
    fn validate_options(&self, config: &Config) -> Result<UpgradePlanOptions, String> {
        let src_chain_config = config
            .find_chain(&self.src_chain_id)
            .ok_or_else(|| "missing src chain configuration".to_string())?;

        let dst_chain_config = config
            .find_chain(&self.dst_chain_id)
            .ok_or_else(|| "missing destination chain configuration".to_string())?;

        let opts = UpgradePlanOptions {
            dst_chain_config: dst_chain_config.clone(),
            src_chain_config: src_chain_config.clone(),
            src_client_id: self.src_client_id.clone(),
            amount: self.amount,
            height_offset: self.height_offset,
            upgraded_chain_id: self
                .new_chain_id
                .clone()
                .unwrap_or_else(|| self.dst_chain_id.clone()),
            upgraded_unbonding_period: self.new_unbonding.map(Duration::from_secs),
            upgrade_plan_name: self
                .upgrade_name
                .clone()
                .unwrap_or_else(|| "plan".to_string()),
            legacy: self.legacy,
        };

        Ok(opts)
    }
}

impl Runnable for TxIbcUpgradeChainCmd {
    fn run(&self) {
        let config = app_config();

        let opts = match self.validate_options(&config) {
            Err(err) => return Output::error(err).exit(),
            Ok(result) => result,
        };
        info!("Message {:?}", opts);

        let rt = Arc::new(TokioRuntime::new().unwrap());

        let src_chain_res = CosmosSdkChain::bootstrap(opts.src_chain_config.clone(), rt.clone())
            .map_err(Error::relayer);
        let src_chain = match src_chain_res {
            Ok(chain) => chain,
            Err(e) => return Output::error(format!("{}", e)).exit(),
        };

        let dst_chain_res =
            CosmosSdkChain::bootstrap(opts.dst_chain_config.clone(), rt).map_err(Error::relayer);
        let dst_chain = match dst_chain_res {
            Ok(chain) => chain,
            Err(e) => return Output::error(format!("{}", e)).exit(),
        };

        let res: Result<Vec<IbcEvent>, Error> =
            build_and_send_ibc_upgrade_proposal(dst_chain, src_chain, &opts)
                .map_err(Error::upgrade_chain);

        match res {
            Ok(ev) => Output::success(ev).exit(),
            Err(e) => Output::error(format!("{}", e)).exit(),
        }
    }
}
