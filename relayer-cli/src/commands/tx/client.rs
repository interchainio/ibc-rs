use core::fmt;

use abscissa_core::{config, Command, Options, Runnable};

use ibc::events::IbcEvent;
use ibc::ics02_client::client_state::ClientState;
use ibc::ics24_host::identifier::{ChainId, ClientId};
use ibc_proto::ibc::core::client::v1::QueryClientStatesRequest;
use ibc_relayer::chain::handle::ChainHandle;
use ibc_relayer::foreign_client::ForeignClient;

use crate::application::{app_config, CliApp};
use crate::cli_utils::{spawn_chain_runtime, spawn_chain_runtime_generic, ChainHandlePair};
use crate::conclude::{exit_with_unrecoverable_error, Output};
use crate::error::Error;

#[derive(Clone, Command, Debug, Options)]
pub struct TxCreateClientCmd {
    #[options(free, required, help = "identifier of the destination chain")]
    dst_chain_id: ChainId,

    #[options(free, required, help = "identifier of the source chain")]
    src_chain_id: ChainId,
}

/// Sample to run this tx:
///     `hermes tx raw create-client ibc-0 ibc-1`
impl Runnable for TxCreateClientCmd {
    fn run(&self) {
        let config = app_config();

        if self.src_chain_id == self.dst_chain_id {
            Output::error("source and destination chains must be different".to_string()).exit()
        }

        let chains = match ChainHandlePair::spawn(&config, &self.src_chain_id, &self.dst_chain_id) {
            Ok(chains) => chains,
            Err(e) => return Output::error(format!("{}", e)).exit(),
        };

        let client = ForeignClient::restore(ClientId::default(), chains.dst, chains.src);

        // Trigger client creation via the "build" interface, so that we obtain the resulting event
        let res: Result<IbcEvent, Error> = client
            .build_create_client_and_send()
            .map_err(Error::foreign_client);

        match res {
            Ok(receipt) => Output::success(receipt).exit(),
            Err(e) => Output::error(format!("{}", e)).exit(),
        }
    }
}

#[derive(Clone, Command, Debug, Options)]
pub struct TxUpdateClientCmd {
    #[options(free, required, help = "identifier of the destination chain")]
    dst_chain_id: ChainId,

    #[options(
        free,
        required,
        help = "identifier of the client to be updated on destination chain"
    )]
    dst_client_id: ClientId,

    #[options(help = "the target height of the client update", short = "h")]
    target_height: Option<u64>,

    #[options(help = "the trusted height of the client update", short = "t")]
    trusted_height: Option<u64>,
}

impl Runnable for TxUpdateClientCmd {
    fn run(&self) {
        let config = app_config();

        let dst_chain = match spawn_chain_runtime(&config, &self.dst_chain_id) {
            Ok(handle) => handle,
            Err(e) => return Output::error(format!("{}", e)).exit(),
        };

        let src_chain_id =
            match dst_chain.query_client_state(&self.dst_client_id, ibc::Height::zero()) {
                Ok(cs) => cs.chain_id(),
                Err(e) => {
                    return Output::error(format!(
                        "Query of client '{}' on chain '{}' failed with error: {}",
                        self.dst_client_id, self.dst_chain_id, e
                    ))
                    .exit();
                }
            };

        let src_chain = match spawn_chain_runtime(&config, &src_chain_id) {
            Ok(handle) => handle,
            Err(e) => return Output::error(format!("{}", e)).exit(),
        };

        let height = match self.target_height {
            Some(height) => ibc::Height::new(src_chain.id().version(), height),
            None => ibc::Height::zero(),
        };

        let trusted_height = match self.trusted_height {
            Some(height) => ibc::Height::new(src_chain.id().version(), height),
            None => ibc::Height::zero(),
        };

        let client = ForeignClient::find(src_chain, dst_chain, &self.dst_client_id)
            .unwrap_or_else(exit_with_unrecoverable_error);

        let res = client
            .build_update_client_and_send(height, trusted_height)
            .map_err(Error::foreign_client);

        match res {
            Ok(events) => Output::success(events).exit(),
            Err(e) => Output::error(format!("{}", e)).exit(),
        }
    }
}

#[derive(Clone, Command, Debug, Options)]
pub struct TxUpgradeClientCmd {
    #[options(free, required, help = "identifier of the chain that hosts the client")]
    chain_id: ChainId,

    #[options(free, required, help = "identifier of the client to be upgraded")]
    client_id: ClientId,
}

impl Runnable for TxUpgradeClientCmd {
    fn run(&self) {
        let config = app_config();

        let dst_chain = match spawn_chain_runtime(&config, &self.chain_id) {
            Ok(handle) => handle,
            Err(e) => return Output::error(format!("{}", e)).exit(),
        };

        let src_chain_id = match dst_chain.query_client_state(&self.client_id, ibc::Height::zero())
        {
            Ok(cs) => cs.chain_id(),
            Err(e) => {
                return Output::error(format!(
                    "Query of client '{}' on chain '{}' failed with error: {}",
                    self.client_id, self.chain_id, e
                ))
                .exit();
            }
        };

        let src_chain = match spawn_chain_runtime(&config, &src_chain_id) {
            Ok(handle) => handle,
            Err(e) => return Output::error(format!("{}", e)).exit(),
        };

        let client = ForeignClient::find(src_chain, dst_chain, &self.client_id)
            .unwrap_or_else(exit_with_unrecoverable_error);

        let outcome = client.upgrade();

        match outcome {
            Ok(receipt) => Output::success(receipt).exit(),
            Err(e) => Output::error(format!("{}", e)).exit(),
        }
    }
}

#[derive(Clone, Command, Debug, Options)]
pub struct TxUpgradeClientsCmd {
    #[options(
        free,
        required,
        help = "identifier of the chain that underwent an upgrade; all clients targeting this chain will be upgraded"
    )]
    src_chain_id: ChainId,
}

impl Runnable for TxUpgradeClientsCmd {
    fn run(&self) {
        let config = app_config();
        let src_chain = match spawn_chain_runtime(&config, &self.src_chain_id) {
            Ok(handle) => handle,
            Err(e) => return Output::error(format!("{}", e)).exit(),
        };

        let results = config
            .chains
            .iter()
            .filter_map(|chain| {
                (self.src_chain_id != chain.id)
                    .then(|| self.upgrade_clients_for_chain(&config, src_chain.clone(), &chain.id))
            })
            .collect();

        let output = OutputBuffer(results);
        match output.into_result() {
            Ok(events) => Output::success(events).exit(),
            Err(e) => Output::error(e).exit(),
        }
    }
}

impl TxUpgradeClientsCmd {
    fn upgrade_clients_for_chain<Chain: ChainHandle>(
        &self,
        config: &config::Reader<CliApp>,
        src_chain: Chain,
        dst_chain_id: &ChainId,
    ) -> UpgradeClientsForChainResult {
        let dst_chain = spawn_chain_runtime_generic::<Chain>(config, dst_chain_id)?;

        let req = QueryClientStatesRequest {
            pagination: ibc_proto::cosmos::base::query::pagination::all(),
        };
        let outputs = dst_chain
            .query_clients(req)
            .map_err(Error::relayer)?
            .into_iter()
            .filter_map(|c| (self.src_chain_id == c.client_state.chain_id()).then(|| c.client_id))
            .map(|id| TxUpgradeClientsCmd::upgrade_client(id, dst_chain.clone(), src_chain.clone()))
            .collect();

        Ok(outputs)
    }

    fn upgrade_client<Chain: ChainHandle>(
        client_id: ClientId,
        dst_chain: Chain,
        src_chain: Chain,
    ) -> Result<Vec<IbcEvent>, Error> {
        let client = ForeignClient::restore(client_id, dst_chain, src_chain);
        client.upgrade().map_err(Error::foreign_client)
    }
}

type UpgradeClientResult = Result<Vec<IbcEvent>, Error>;
type UpgradeClientsForChainResult = Result<Vec<UpgradeClientResult>, Error>;

struct OutputBuffer(Vec<UpgradeClientsForChainResult>);

impl fmt::Display for OutputBuffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fn sep<'a>(pos: usize, len: usize, other: &'a str, last: &'a str) -> &'a str {
            if pos != len - 1 {
                other
            } else {
                last
            }
        }

        let outer_results = &self.0;
        writeln!(f, ".")?;
        for (o, outer_result) in outer_results.iter().enumerate() {
            write!(f, "{}", sep(o, outer_results.len(), "├─", "└─"))?;
            match outer_result {
                Ok(inner_results) => {
                    writeln!(f, ".")?;
                    for (i, inner_result) in inner_results.iter().enumerate() {
                        write!(
                            f,
                            "{} {} ",
                            sep(o, outer_results.len(), "│", " "),
                            sep(i, inner_results.len(), "├─", "└─"),
                        )?;
                        match inner_result {
                            Ok(events) => writeln!(f, "{:#?}", events)?,
                            Err(e) => writeln!(f, "{}", e.to_string())?,
                        }
                    }
                }
                Err(e) => writeln!(f, " {}", e.to_string())?,
            }
        }
        Ok(())
    }
}

impl OutputBuffer {
    fn into_result(self) -> Result<Vec<Vec<IbcEvent>>, Self> {
        let mut all_events = vec![];
        let mut has_err = false;
        'outer: for outer_result in &self.0 {
            match outer_result {
                Ok(inner_results) => {
                    for inner_result in inner_results {
                        match inner_result {
                            Ok(events) => all_events.push(events.clone()),
                            Err(_) => {
                                has_err = true;
                                break 'outer;
                            }
                        }
                    }
                }
                Err(_) => {
                    has_err = true;
                    break 'outer;
                }
            }
        }
        if has_err {
            Err(self)
        } else {
            Ok(all_events)
        }
    }
}
