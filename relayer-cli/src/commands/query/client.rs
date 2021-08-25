use alloc::sync::Arc;

use abscissa_core::{Command, Options, Runnable};
use tokio::runtime::Runtime as TokioRuntime;
use tracing::debug;

use ibc::events::IbcEventType;
use ibc::ics02_client::client_consensus::QueryClientEventRequest;
use ibc::ics02_client::client_state::ClientState;
use ibc::ics24_host::identifier::ChainId;
use ibc::ics24_host::identifier::ClientId;
use ibc::query::QueryTxRequest;
use ibc::Height;
use ibc_proto::ibc::core::client::v1::QueryConsensusStatesRequest;
use ibc_proto::ibc::core::connection::v1::QueryClientConnectionsRequest;
use ibc_relayer::chain::ChainEndpoint;
use ibc_relayer::chain::CosmosSdkChain;

use crate::application::app_config;
use crate::conclude::Output;

/// Query client state command
#[derive(Clone, Command, Debug, Options)]
pub struct QueryClientStateCmd {
    #[options(free, required, help = "identifier of the chain to query")]
    chain_id: ChainId,

    #[options(free, required, help = "identifier of the client to query")]
    client_id: ClientId,

    #[options(help = "the chain height context for the query", short = "h")]
    height: Option<u64>,
}

/// Command for querying a client's state.
/// hermes query client state ibc-1 07-tendermint-0 --height 3
impl Runnable for QueryClientStateCmd {
    fn run(&self) {
        let config = app_config();

        let chain_config = match config.find_chain(&self.chain_id) {
            None => {
                return Output::error(format!(
                    "chain '{}' not found in configuration file",
                    self.chain_id
                ))
                .exit()
            }
            Some(chain_config) => chain_config,
        };

        let rt = Arc::new(TokioRuntime::new().unwrap());
        let chain = CosmosSdkChain::bootstrap(chain_config.clone(), rt).unwrap();
        let height = ibc::Height::new(chain.id().version(), self.height.unwrap_or(0_u64));

        match chain.query_client_state(&self.client_id, height) {
            Ok(cs) => Output::success(cs).exit(),
            Err(e) => Output::error(format!("{}", e)).exit(),
        }
    }
}

/// Query client consensus command
#[derive(Clone, Command, Debug, Options)]
pub struct QueryClientConsensusCmd {
    #[options(free, required, help = "identifier of the chain to query")]
    chain_id: ChainId,

    #[options(free, required, help = "identifier of the client to query")]
    client_id: ClientId,

    #[options(help = "height of the client's consensus state to query", short = "c")]
    consensus_height: Option<u64>,

    #[options(help = "show only consensus heights", short = "s")]
    heights_only: bool,

    #[options(
        help = "the chain height context to be used, applicable only to a specific height",
        short = "h"
    )]
    height: Option<u64>,
}

/// Implementation of the query for a client's consensus state at a certain height.
/// hermes query client consensus ibc-0 07-tendermint-0 -c 22
impl Runnable for QueryClientConsensusCmd {
    fn run(&self) {
        let config = app_config();

        let chain_config = match config.find_chain(&self.chain_id) {
            None => {
                return Output::error(format!(
                    "chain '{}' not found in configuration file",
                    self.chain_id
                ))
                .exit()
            }
            Some(chain_config) => chain_config,
        };

        debug!("Options: {:?}", self);

        let rt = Arc::new(TokioRuntime::new().unwrap());
        let chain = CosmosSdkChain::bootstrap(chain_config.clone(), rt).unwrap();

        let counterparty_chain = match chain.query_client_state(&self.client_id, Height::zero()) {
            Ok(cs) => cs.chain_id(),
            Err(e) => {
                return Output::error(format!(
                    "Failed while querying client '{}' on chain '{}' with error: {}",
                    self.client_id, self.chain_id, e
                ))
                .exit()
            }
        };

        match self.consensus_height {
            Some(cs_height) => {
                let height = ibc::Height::new(chain.id().version(), self.height.unwrap_or(0_u64));
                let consensus_height = ibc::Height::new(counterparty_chain.version(), cs_height);

                let res =
                    chain.query_consensus_state(self.client_id.clone(), consensus_height, height);

                match res {
                    Ok(cs) => Output::success(cs).exit(),
                    Err(e) => Output::error(format!("{}", e)).exit(),
                }
            }
            None => {
                let res = chain.query_consensus_states(QueryConsensusStatesRequest {
                    client_id: self.client_id.to_string(),
                    pagination: ibc_proto::cosmos::base::query::pagination::all(),
                });

                match res {
                    Ok(states) => {
                        if self.heights_only {
                            let heights: Vec<Height> = states.iter().map(|cs| cs.height).collect();
                            Output::success(heights).exit()
                        } else {
                            Output::success(states).exit()
                        }
                    }
                    Err(e) => Output::error(format!("{}", e)).exit(),
                }
            }
        }
    }
}

#[derive(Clone, Command, Debug, Options)]
pub struct QueryClientHeaderCmd {
    #[options(free, required, help = "identifier of the chain to query")]
    chain_id: ChainId,

    #[options(free, required, help = "identifier of the client to query")]
    client_id: ClientId,

    #[options(free, required, help = "height of header to query")]
    consensus_height: u64,

    #[options(help = "the chain height context for the query", short = "h")]
    height: Option<u64>,
}

/// Implementation of the query for the header used in a client update at a certain height.
/// hermes query client header ibc-0 07-tendermint-0 22
impl Runnable for QueryClientHeaderCmd {
    fn run(&self) {
        let config = app_config();

        let chain_config = match config.find_chain(&self.chain_id) {
            None => {
                return Output::error(format!(
                    "chain '{}' not found in configuration file",
                    self.chain_id
                ))
                .exit()
            }
            Some(chain_config) => chain_config,
        };

        debug!("Options: {:?}", self);

        let rt = Arc::new(TokioRuntime::new().unwrap());
        let chain = CosmosSdkChain::bootstrap(chain_config.clone(), rt).unwrap();

        let counterparty_chain = match chain.query_client_state(&self.client_id, Height::zero()) {
            Ok(cs) => cs.chain_id(),
            Err(e) => {
                return Output::error(format!(
                    "Failed while querying client '{}' on chain '{}' with error: {}",
                    self.client_id, self.chain_id, e
                ))
                .exit()
            }
        };

        let consensus_height =
            ibc::Height::new(counterparty_chain.version(), self.consensus_height);
        let height = ibc::Height::new(chain.id().version(), self.height.unwrap_or(0_u64));

        let res = chain.query_txs(QueryTxRequest::Client(QueryClientEventRequest {
            height,
            event_id: IbcEventType::UpdateClient,
            client_id: self.client_id.clone(),
            consensus_height,
        }));

        match res {
            Ok(header) => Output::success(header).exit(),
            Err(e) => Output::error(format!("{}", e)).exit(),
        }
    }
}

/// Query client connections command
#[derive(Clone, Command, Debug, Options)]
pub struct QueryClientConnectionsCmd {
    #[options(free, required, help = "identifier of the chain to query")]
    chain_id: ChainId,

    #[options(free, required, help = "identifier of the client to query")]
    client_id: ClientId,

    #[options(help = "the chain height which this query should reflect", short = "h")]
    height: Option<u64>,
}

// hermes query connections ibc-0
impl Runnable for QueryClientConnectionsCmd {
    fn run(&self) {
        let config = app_config();

        let chain_config = match config.find_chain(&self.chain_id) {
            None => {
                return Output::error(format!(
                    "chain '{}' not found in configuration file",
                    self.chain_id
                ))
                .exit()
            }
            Some(chain_config) => chain_config,
        };

        debug!("Options: {:?}", self);

        let rt = Arc::new(TokioRuntime::new().unwrap());
        let chain = CosmosSdkChain::bootstrap(chain_config.clone(), rt).unwrap();

        let req = QueryClientConnectionsRequest {
            client_id: self.client_id.to_string(),
        };

        let res = chain.query_client_connections(req);

        match res {
            Ok(ce) => Output::success(ce).exit(),
            Err(e) => Output::error(format!("{}", e)).exit(),
        }
    }
}
