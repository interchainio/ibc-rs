use abscissa_core::clap::Parser;
use abscissa_core::Runnable;

use ibc_relayer::chain::handle::ChainHandle;
use ibc_relayer::chain::requests::{
    IncludeProof, PageRequest, QueryClientStateRequest, QueryConnectionsRequest, QueryHeight,
};
use ibc_relayer_types::core::ics24_host::identifier::{ChainId, ConnectionId};

use crate::cli_utils::spawn_chain_runtime;
use crate::conclude::{exit_with_unrecoverable_error, Output};
use crate::prelude::*;

#[derive(Clone, Command, Debug, Parser, PartialEq, Eq)]
pub struct QueryConnectionsCmd {
    #[clap(
        long = "chain",
        required = true,
        value_name = "CHAIN_ID",
        help_heading = "REQUIRED",
        help = "Identifier of the chain to query"
    )]
    chain_id: ChainId,

    #[clap(
        long = "counterparty-chain",
        value_name = "COUNTERPARTY_CHAIN_ID",
        help = "Filter the query response by the counterparty chain"
    )]
    counterparty_chain_id: Option<ChainId>,

    #[clap(
        long = "verbose",
        help = "Enable verbose output, displaying the client for each connection in the response"
    )]
    verbose: bool,
}

// hermes query connections ibc-0
impl Runnable for QueryConnectionsCmd {
    fn run(&self) {
        let config = app_config();

        let chain = spawn_chain_runtime(&config, &self.chain_id)
            .unwrap_or_else(exit_with_unrecoverable_error);

        let mut connections = chain
            .query_connections(QueryConnectionsRequest {
                pagination: Some(PageRequest::all()),
            })
            .unwrap_or_else(|e| {
                Output::error(format!(
                    "An error occurred trying to query connections: {e}"
                ))
                .exit()
            });

        // Check the counterparty chain id only if filtering is required.
        if let Some(counterparty_filter_id) = self.counterparty_chain_id.clone() {
            connections.retain(|connection| {
                let client_id = connection.end().client_id().to_owned();
                let chain_height = chain.query_latest_height();
                let (client_state, _) = chain
                    .query_client_state(
                        QueryClientStateRequest {
                            client_id,
                            height: QueryHeight::Specific(chain_height.unwrap()),
                        },
                        IncludeProof::No,
                    )
                    .unwrap();

                let counterparty_chain_id = client_state.chain_id();

                counterparty_chain_id == counterparty_filter_id
            });
        }

        if self.verbose {
            Output::success(connections).exit()
        } else {
            let ids: Vec<ConnectionId> = connections
                .into_iter()
                .map(|identified_connection| identified_connection.connection_id)
                .collect();

            Output::success(ids).exit()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::QueryConnectionsCmd;

    use abscissa_core::clap::Parser;
    use ibc_relayer_types::core::ics24_host::identifier::ChainId;

    #[test]
    fn test_query_connections_required_only() {
        assert_eq!(
            QueryConnectionsCmd {
                chain_id: ChainId::from_string("chain_id"),
                counterparty_chain_id: None,
                verbose: false,
            },
            QueryConnectionsCmd::parse_from(["test", "--chain", "chain_id"])
        )
    }

    #[test]
    fn test_query_connections_counterparty_chain() {
        assert_eq!(
            QueryConnectionsCmd {
                chain_id: ChainId::from_string("chain_id"),
                counterparty_chain_id: Some(ChainId::from_string("chain_counterparty_id")),
                verbose: false,
            },
            QueryConnectionsCmd::parse_from([
                "test",
                "--chain",
                "chain_id",
                "--counterparty-chain",
                "chain_counterparty_id"
            ])
        )
    }

    #[test]
    fn test_query_connections_verbose() {
        assert_eq!(
            QueryConnectionsCmd {
                chain_id: ChainId::from_string("chain_id"),
                counterparty_chain_id: None,
                verbose: true,
            },
            QueryConnectionsCmd::parse_from(["test", "--chain", "chain_id", "--verbose"])
        )
    }

    #[test]
    fn test_query_connections_no_chain() {
        assert!(QueryConnectionsCmd::try_parse_from(["test"]).is_err())
    }
}
