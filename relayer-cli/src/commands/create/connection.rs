use core::time::Duration;

use abscissa_core::{Command, Options, Runnable};

use ibc::ics02_client::client_state::ClientState;
use ibc::ics24_host::identifier::{ChainId, ClientId};
use ibc::Height;
use ibc_relayer::chain::handle::ChainHandle;
use ibc_relayer::connection::Connection;
use ibc_relayer::foreign_client::ForeignClient;

use crate::cli_utils::{spawn_chain_runtime, ChainHandlePair};
use crate::conclude::{exit_with_unrecoverable_error, Output};
use crate::prelude::*;

#[derive(Clone, Command, Debug, Options)]
pub struct CreateConnectionCommand {
    #[options(
        free,
        required,
        help = "identifier of the side `a` chain for the new connection"
    )]
    chain_a_id: ChainId,

    #[options(free, help = "identifier of the side `b` chain for the new connection")]
    chain_b_id: Option<ChainId>,

    #[options(
        help = "identifier of client hosted on chain `a`; default: None (creates a new client)",
        no_short
    )]
    client_a: Option<ClientId>,

    #[options(
        help = "identifier of client hosted on chain `b`; default: None (creates a new client)",
        no_short
    )]
    client_b: Option<ClientId>,

    #[options(
        help = "delay period parameter for the new connection (seconds)",
        default = "0",
        no_short
    )]
    delay: u64,
}

// cargo run --bin hermes -- create connection ibc-0 ibc-1
// cargo run --bin hermes -- create connection ibc-0 ibc-1 --delay 100
// cargo run --bin hermes -- create connection ibc-0 --client-a-id 07-tendermint-0 --client-b-id 07-tendermint-0
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

        // Validate the other options. Bail if the CLI was invoked with incompatible options.
        if self.client_a.is_some() {
            return Output::error(
                "Option `<chain-b-id>` is incompatible with `--client-a`".to_string(),
            )
            .exit();
        }
        if self.client_b.is_some() {
            return Output::error(
                "Option `<chain-b-id>` is incompatible with `--client-b`".to_string(),
            )
            .exit();
        }

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
            Err(e) => Output::error(format!("{}", e)).exit(),
        }
    }

    /// Create a connection reusing pre-existing clients on both chains.
    fn run_reusing_clients(&self) {
        let config = app_config();

        // Validate & spawn runtime for chain_a.
        let chain_a = match spawn_chain_runtime(&config, &self.chain_a_id) {
            Ok(handle) => handle,
            Err(e) => return Output::error(format!("{}", e)).exit(),
        };

        // Unwrap the identifier of the client on chain_a.
        let client_a_id = match &self.client_a {
            Some(c) => c,
            None => {
                return Output::error(
                    "Option `--client-a` is necessary when <chain-b-id> is missing".to_string(),
                )
                .exit()
            }
        };

        // Query client state. Extract the target chain (chain_id which this client is verifying).
        let height = Height::new(chain_a.id().version(), 0);
        let chain_b_id = match chain_a.query_client_state(client_a_id, height) {
            Ok(cs) => cs.chain_id(),
            Err(e) => {
                return Output::error(format!(
                    "Failed while querying client '{}' on chain '{}' with error: {}",
                    client_a_id, self.chain_a_id, e
                ))
                .exit()
            }
        };

        // Validate & spawn runtime for chain_b.
        let chain_b = match spawn_chain_runtime(&config, &chain_b_id) {
            Ok(handle) => handle,
            Err(e) => return Output::error(format!("{}", e)).exit(),
        };

        // Unwrap the identifier of the client on chain_b.
        let client_b_id = match &self.client_b {
            Some(c) => c,
            None => {
                return Output::error(
                    "Option `--client-b` is necessary when <chain-b-id> is missing".to_string(),
                )
                .exit()
            }
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
            Err(e) => Output::error(format!("{}", e)).exit(),
        }
    }
}
