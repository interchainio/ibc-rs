use alloc::sync::Arc;
use core::str::FromStr;

use abscissa_core::{Command, Options, Runnable};
use tokio::runtime::Runtime as TokioRuntime;
use tracing::debug;

use tendermint::abci::transaction::Hash;

use ibc::ics24_host::identifier::ChainId;
use ibc::query::{QueryTxHash, QueryTxRequest};

use ibc_relayer::chain::handle::{ChainHandle, ProdChainHandle};
use ibc_relayer::chain::runtime::ChainRuntime;
use ibc_relayer::chain::CosmosSdkChain;

use crate::conclude::Output;
use crate::error::Error;
use crate::prelude::app_config;

/// Query the events emitted by transaction
#[derive(Clone, Command, Debug, Options)]
pub struct QueryTxEventsCmd {
    #[options(free, required, help = "identifier of the chain to query")]
    chain_id: ChainId,

    #[options(free, required, help = "transaction hash to query")]
    hash: String,
}

// cargo run --bin hermes -- query tx events ibc-0 B8E78AD83810239E21863AC7B5FC4F99396ABB39EB534F721EEF43A4979C2821
impl Runnable for QueryTxEventsCmd {
    fn run(&self) {
        let config = app_config();

        debug!("Options: {:?}", self);

        let chain_config = match config
            .find_chain(&self.chain_id)
            .ok_or_else(|| format!("chain '{}' not found in configuration file", self.chain_id))
        {
            Err(err) => return Output::error(err).exit(),
            Ok(result) => result,
        };

        let rt = Arc::new(TokioRuntime::new().unwrap());
        let chain =
            ChainRuntime::<CosmosSdkChain>::spawn::<ProdChainHandle>(chain_config.clone(), rt)
                .unwrap();

        let res = Hash::from_str(self.hash.as_str())
            .map_err(|e| Error::invalid_hash(self.hash.clone(), e))
            .and_then(|h| {
                chain
                    .query_txs(QueryTxRequest::Transaction(QueryTxHash(h)))
                    .map_err(Error::relayer)
            });

        match res {
            Ok(res) => Output::success(res).exit(),
            Err(e) => Output::error(format!("{}", e)).exit(),
        }
    }
}
