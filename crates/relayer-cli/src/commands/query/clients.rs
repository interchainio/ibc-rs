use abscissa_core::clap::Parser;
use abscissa_core::{Command, Runnable};
use ibc_relayer::chain::handle::ChainHandle;
use serde::Serialize;

use ibc_relayer::chain::requests::{PageRequest, QueryClientStatesRequest};
use ibc_relayer_types::core::ics24_host::identifier::{ChainId, ClientId};

use crate::cli_utils::spawn_chain_runtime;
use crate::conclude::{exit_with_unrecoverable_error, Output};
use crate::error::Error;
use crate::prelude::*;

/// Query clients command
#[derive(Clone, Command, Debug, Parser, PartialEq, Eq)]
pub struct QueryAllClientsCmd {
    #[clap(
        long = "host-chain",
        required = true,
        value_name = "HOST_CHAIN_ID",
        help_heading = "REQUIRED",
        help = "Identifier of the chain to query"
    )]
    chain_id: ChainId,

    #[clap(
        long = "reference-chain",
        help = "Filter for clients which target a specific chain id (implies '--omit-chain-ids')",
        value_name = "REFERENCE_CHAIN_ID"
    )]
    src_chain_id: Option<ChainId>,

    #[clap(
        long = "omit-chain-ids",
        help = "Omit printing the reference (or target) chain for each client"
    )]
    omit_chain_ids: bool,
}

#[derive(Debug, Serialize)]
struct ClientChain {
    client_id: ClientId,
    chain_id: ChainId,
}

/// Command for querying all clients.
/// hermes --config cfg.toml query clients --chain ibc-1
impl Runnable for QueryAllClientsCmd {
    fn run(&self) {
        let config = app_config();

        let chain = spawn_chain_runtime(&config, &self.chain_id)
            .unwrap_or_else(exit_with_unrecoverable_error);

        let res: Result<_, Error> = chain
            .query_clients(QueryClientStatesRequest {
                pagination: Some(PageRequest::all()),
            })
            .map_err(Error::relayer);

        match res {
            Ok(clients) => {
                match self.src_chain_id.clone() {
                    None => {
                        match self.omit_chain_ids {
                            true => {
                                // Omit chain identifiers
                                debug!(
                                    "printing identifiers of all clients hosted on chain {}",
                                    self.chain_id
                                );
                                let out: Vec<ClientId> =
                                    clients.into_iter().map(|cs| cs.client_id).collect();
                                Output::success(out).exit()
                            }
                            false => {
                                // Include chain identifiers
                                debug!("printing identifiers (and target chain identifiers) of all clients hosted on chain {}", self.chain_id);
                                let out: Vec<ClientChain> = clients
                                    .into_iter()
                                    .map(|cs| ClientChain {
                                        client_id: cs.client_id,
                                        chain_id: cs.client_state.chain_id(),
                                    })
                                    .collect();
                                Output::success(out).exit()
                            }
                        };
                    }
                    Some(source_chain_id) => {
                        debug!(
                            "printing identifiers of all clients hosted on chain {} which target chain {}",
                            self.chain_id, source_chain_id
                        );
                        // Filter and omit chain ids
                        let out: Vec<ClientId> = clients
                            .into_iter()
                            .filter(|cs| cs.client_state.chain_id().eq(&source_chain_id))
                            .map(|cs| cs.client_id)
                            .collect();
                        Output::success(out).exit()
                    }
                }
            }
            Err(e) => Output::error(e).exit(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::QueryAllClientsCmd;

    use abscissa_core::clap::Parser;
    use ibc_relayer_types::core::ics24_host::identifier::ChainId;

    #[test]
    fn test_query_clients_required_only() {
        assert_eq!(
            QueryAllClientsCmd {
                chain_id: ChainId::from_string("chain_host_id"),
                src_chain_id: None,
                omit_chain_ids: false
            },
            QueryAllClientsCmd::parse_from(["test", "--host-chain", "chain_host_id"])
        )
    }

    #[test]
    fn test_query_clients_omit_chain_ids() {
        assert_eq!(
            QueryAllClientsCmd {
                chain_id: ChainId::from_string("chain_host_id"),
                src_chain_id: None,
                omit_chain_ids: true
            },
            QueryAllClientsCmd::parse_from([
                "test",
                "--host-chain",
                "chain_host_id",
                "--omit-chain-ids"
            ])
        )
    }

    #[test]
    fn test_query_clients_reference_chain() {
        assert_eq!(
            QueryAllClientsCmd {
                chain_id: ChainId::from_string("chain_host_id"),
                src_chain_id: Some(ChainId::from_string("reference_chain_id")),
                omit_chain_ids: false
            },
            QueryAllClientsCmd::parse_from([
                "test",
                "--host-chain",
                "chain_host_id",
                "--reference-chain",
                "reference_chain_id"
            ])
        )
    }

    #[test]
    fn test_query_clients_no_chain() {
        assert!(QueryAllClientsCmd::try_parse_from(["test"]).is_err())
    }
}
