use core::time::Duration;

use abscissa_core::clap::Parser;
use abscissa_core::{Command, Runnable};

use ibc_relayer::chain::handle::ChainHandle;
use ibc_relayer::chain::requests::{IncludeProof, QueryClientStateRequest, QueryHeight};
use ibc_relayer::connection::Connection;
use ibc_relayer::foreign_client::ForeignClient;
use ibc_relayer_types::core::ics24_host::identifier::{ChainId, ClientId};

use crate::cli_utils::{spawn_chain_runtime, ChainHandlePair};
use crate::conclude::{exit_with_unrecoverable_error, Output};
use crate::prelude::*;

#[derive(Clone, Command, Debug, Parser, PartialEq, Eq)]
#[clap(override_usage("hermes create connection [OPTIONS] --a-chain <A_CHAIN_ID> --b-chain <B_CHAIN_ID>

    hermes create connection [OPTIONS] --a-chain <A_CHAIN_ID> --a-client <A_CLIENT_ID> --b-client <B_CLIENT_ID>"))]
pub struct CreateConnectionCommand {
    #[clap(
        long = "a-chain",
        required = true,
        value_name = "A_CHAIN_ID",
        help_heading = "FLAGS",
        help = "Identifier of the side `a` chain for the new connection"
    )]
    chain_a_id: ChainId,

    #[clap(
        long = "b-chain",
        required = true,
        value_name = "B_CHAIN_ID",
        groups = &["a_client", "b_client"],
        help_heading = "FLAGS",
        help = "Identifier of the side `b` chain for the new connection"
    )]
    chain_b_id: Option<ChainId>,

    #[clap(
        long = "a-client",
        required = true,
        value_name = "A_CLIENT_ID",
        group = "a_client",
        help_heading = "FLAGS",
        help = "Identifier of client hosted on chain `a`; default: None (creates a new client)"
    )]
    client_a: Option<ClientId>,

    #[clap(
        long = "b-client",
        required = true,
        value_name = "B_CLIENT_ID",
        group = "b_client",
        help_heading = "FLAGS",
        help = "Identifier of client hosted on chain `b`; default: None (creates a new client)"
    )]
    client_b: Option<ClientId>,

    #[clap(
        long = "delay",
        value_name = "DELAY",
        help = "Delay period parameter for the new connection (seconds)",
        default_value = "0"
    )]
    delay: u64,
}

// cargo run --bin hermes -- create connection --a-chain ibc-0 --b-chain ibc-1
// cargo run --bin hermes -- create connection --a-chain ibc-0 --b-chain ibc-1 --delay 100
// cargo run --bin hermes -- create connection --a-chain ibc-0 --a-client 07-tendermint-0 --b-client 07-tendermint-0
impl Runnable for CreateConnectionCommand {
    fn run(&self) {
        match &self.chain_b_id {
            Some(side_b) => self.run_using_new_clients(side_b),
            None => self.run_reusing_clients(),
        }
    }
}

impl CreateConnectionCommand {
    /// Creates a connection that uses newly created clients on each side.
    fn run_using_new_clients(&self, chain_b_id: &ChainId) {
        let config = app_config();

        let chains = ChainHandlePair::spawn(&config, &self.chain_a_id, chain_b_id)
            .unwrap_or_else(exit_with_unrecoverable_error);

        info!(
            "Creating new clients hosted on chains {} and {}",
            self.chain_a_id, chain_b_id
        );

        let client_a = ForeignClient::new(chains.src.clone(), chains.dst.clone())
            .unwrap_or_else(exit_with_unrecoverable_error);
        let client_b = ForeignClient::new(chains.dst.clone(), chains.src)
            .unwrap_or_else(exit_with_unrecoverable_error);

        // Finally, execute the connection handshake.
        let delay = Duration::from_secs(self.delay);
        match Connection::new(client_a, client_b, delay) {
            Ok(conn) => Output::success(conn).exit(),
            Err(e) => Output::error(e).exit(),
        }
    }

    /// Create a connection reusing pre-existing clients on both chains.
    fn run_reusing_clients(&self) {
        let config = app_config();

        // Validate & spawn runtime for chain_a.
        let chain_a = match spawn_chain_runtime(&config, &self.chain_a_id) {
            Ok(handle) => handle,
            Err(e) => Output::error(e).exit(),
        };

        // Unwrap the identifier of the client on chain_a.
        let client_a_id = match &self.client_a {
            Some(c) => c,
            None => Output::error(
                "Option `--a-client` is necessary when <B_CHAIN_ID> is missing".to_string(),
            )
            .exit(),
        };

        // Query client state. Extract the target chain (chain_id which this client is verifying).
        let chain_b_id = match chain_a.query_client_state(
            QueryClientStateRequest {
                client_id: client_a_id.clone(),
                height: QueryHeight::Latest,
            },
            IncludeProof::No,
        ) {
            Ok((cs, _)) => cs.chain_id(),
            Err(e) => Output::error(format!(
                "failed while querying client '{}' on chain '{}' with error: {}",
                client_a_id, self.chain_a_id, e
            ))
            .exit(),
        };

        // Validate & spawn runtime for chain_b.
        let chain_b = match spawn_chain_runtime(&config, &chain_b_id) {
            Ok(handle) => handle,
            Err(e) => Output::error(e).exit(),
        };

        // Unwrap the identifier of the client on chain_b.
        let client_b_id = match &self.client_b {
            Some(c) => c,
            None => Output::error(
                "Option `--b-client` is necessary when <B_CHAIN_ID> is missing".to_string(),
            )
            .exit(),
        };

        info!(
            "Creating a new connection with pre-existing clients {} and {}",
            client_a_id, client_b_id
        );

        // Get the two ForeignClient objects.
        let client_a = ForeignClient::find(chain_b.clone(), chain_a.clone(), client_a_id)
            .unwrap_or_else(exit_with_unrecoverable_error);
        let client_b = ForeignClient::find(chain_a, chain_b, client_b_id)
            .unwrap_or_else(exit_with_unrecoverable_error);

        // All verification passed. Create the Connection object & do the handshake.
        let delay = Duration::from_secs(self.delay);
        match Connection::new(client_a, client_b, delay) {
            Ok(conn) => Output::success(conn).exit(),
            Err(e) => Output::error(e).exit(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CreateConnectionCommand;

    use abscissa_core::clap::Parser;
    use ibc_relayer_types::core::ics24_host::identifier::{ChainId, ClientId};

    use std::str::FromStr;

    #[test]
    fn test_create_connection_b_chain() {
        assert_eq!(
            CreateConnectionCommand {
                chain_a_id: ChainId::from_string("chain_a"),
                chain_b_id: Some(ChainId::from_string("chain_b")),
                client_a: None,
                client_b: None,
                delay: 0
            },
            CreateConnectionCommand::parse_from([
                "test",
                "--a-chain",
                "chain_a",
                "--b-chain",
                "chain_b"
            ])
        )
    }

    #[test]
    fn test_create_connection_b_chain_with_delay() {
        assert_eq!(
            CreateConnectionCommand {
                chain_a_id: ChainId::from_string("chain_a"),
                chain_b_id: Some(ChainId::from_string("chain_b")),
                client_a: None,
                client_b: None,
                delay: 42
            },
            CreateConnectionCommand::parse_from([
                "test",
                "--a-chain",
                "chain_a",
                "--b-chain",
                "chain_b",
                "--delay",
                "42"
            ])
        )
    }

    #[test]
    fn create_connection_a_chain_and_clients() {
        assert_eq!(
            CreateConnectionCommand {
                chain_a_id: ChainId::from_string("chain_a"),
                chain_b_id: None,
                client_a: Some(ClientId::from_str("07-client_a").unwrap()),
                client_b: Some(ClientId::from_str("07-client_b").unwrap()),
                delay: 0
            },
            CreateConnectionCommand::parse_from([
                "test",
                "--a-chain",
                "chain_a",
                "--a-client",
                "07-client_a",
                "--b-client",
                "07-client_b"
            ])
        )
    }

    #[test]
    fn create_connection_a_chain_and_clients_with_delay() {
        assert_eq!(
            CreateConnectionCommand {
                chain_a_id: ChainId::from_string("chain_a"),
                chain_b_id: None,
                client_a: Some(ClientId::from_str("07-client_a").unwrap()),
                client_b: Some(ClientId::from_str("07-client_b").unwrap()),
                delay: 42
            },
            CreateConnectionCommand::parse_from([
                "test",
                "--a-chain",
                "chain_a",
                "--a-client",
                "07-client_a",
                "--b-client",
                "07-client_b",
                "--delay",
                "42"
            ])
        )
    }

    #[test]
    fn test_create_connection_a_chain_only() {
        assert!(CreateConnectionCommand::try_parse_from(["test", "--a-chain", "chain_a"]).is_err())
    }

    #[test]
    fn test_create_connection_no_a_chain() {
        assert!(CreateConnectionCommand::try_parse_from(["test", "--b-chain", "chain_b"]).is_err())
    }

    #[test]
    fn test_create_connection_b_client_without_a_client() {
        assert!(CreateConnectionCommand::try_parse_from([
            "test",
            "--a-chain",
            "chain_a",
            "--b-client",
            "client_b"
        ])
        .is_err())
    }

    #[test]
    fn test_create_connection_a_client_required_without_b_client() {
        assert!(CreateConnectionCommand::try_parse_from([
            "test",
            "--a-chain",
            "chain_a",
            "--a-client",
            "client_a"
        ])
        .is_err())
    }

    #[test]
    fn test_create_connection_b_chain_with_a_client() {
        assert!(CreateConnectionCommand::try_parse_from([
            "test",
            "--a-chain",
            "chain_a",
            "--b-chain",
            "chain_b",
            "--a-client",
            "07-client_a"
        ])
        .is_err())
    }

    #[test]
    fn test_create_connection_b_chain_with_b_client() {
        assert!(CreateConnectionCommand::try_parse_from([
            "test",
            "--a-chain",
            "chain_a",
            "--b-chain",
            "chain_b",
            "--b-client",
            "07-client_b"
        ])
        .is_err())
    }
}
