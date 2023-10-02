use abscissa_core::clap::Parser;
use abscissa_core::{Command, Runnable};

use console::style;
use dialoguer::Confirm;

use ibc_relayer::chain::handle::ChainHandle;
use ibc_relayer::chain::requests::{
    IncludeProof, QueryClientStateRequest, QueryConnectionRequest, QueryHeight,
};
use ibc_relayer::channel::Channel;
use ibc_relayer::connection::Connection;
use ibc_relayer::foreign_client::ForeignClient;
use ibc_relayer_types::core::ics03_connection::connection::IdentifiedConnectionEnd;
use ibc_relayer_types::core::ics04_channel::channel::Ordering;
use ibc_relayer_types::core::ics04_channel::version::Version;
use ibc_relayer_types::core::ics24_host::identifier::{ChainId, ConnectionId, PortId};

use crate::cli_utils::{spawn_chain_runtime, ChainHandlePair};
use crate::conclude::{exit_with_unrecoverable_error, Output};
use crate::prelude::*;
use ibc_relayer::config::default::connection_delay;

static PROMPT: &str = "Are you sure you want a new connection & clients to be created? Hermes will use default security parameters.";
static HINT: &str = "Consider using the default invocation\n\nhermes create channel --a-port <PORT-ID> --b-port <PORT-ID> --a-chain <CHAIN-A-ID> --a-connection <CONNECTION-A-ID>\n\nto re-use a pre-existing connection.";

/// The data structure that represents all the possible options when invoking
/// the `create channel` CLI command.
///
/// There are two possible ways to invoke this command:
///
/// `create channel --a-port <A_PORT_ID> --b-port <B_PORT_ID> --a-chain <A_CHAIN_ID> --a-connection <A_CONNECTION_ID>`
/// is the default way in which this command should be used, specifying a `Connection-ID`
/// associated with chain A for this new channel to re-use.
///
/// `create channel --a-port <A_PORT_ID> --b-port <B_PORT_ID> --a-chain <A_CHAIN_ID> --b-chain <B_CHAIN_ID> --new-client-connection`
/// can alternatively be used to indicate that a new connection/client pair is being
/// created as part of this new channel. This brings up an interactive yes/no prompt
/// to ensure that the operator at least considers the fact that they're initializing a
/// new connection with the channel. This prompt can be skipped by appending the `--yes`
/// flag to the command.
///
/// Note that `Connection-ID`s have to be considered based off of the chain's perspective. Although
/// chain A and chain B might refer to the connection with different names, they are actually referring
/// to the same connection.
#[derive(Clone, Command, Debug, Parser, PartialEq, Eq)]
#[clap(
    override_usage = "hermes create channel [OPTIONS] --a-chain <A_CHAIN_ID> --a-connection <A_CONNECTION_ID> --a-port <A_PORT_ID> --b-port <B_PORT_ID>

    hermes create channel [OPTIONS] --a-chain <A_CHAIN_ID> --b-chain <B_CHAIN_ID> --a-port <A_PORT_ID> --b-port <B_PORT_ID> --new-client-connection"
)]
pub struct CreateChannelCommand {
    #[clap(
        long = "a-chain",
        required = true,
        value_name = "A_CHAIN_ID",
        help_heading = "FLAGS",
        help = "Identifier of the side `a` chain for the new channel"
    )]
    chain_a: ChainId,

    #[clap(
        long = "b-chain",
        value_name = "B_CHAIN_ID",
        required = true,
        requires = "new-client-connection",
        group = "b_chain_group",
        help_heading = "FLAGS",
        help = "Identifier of the side `b` chain for the new channel"
    )]
    chain_b: Option<ChainId>,

    #[clap(
        long = "a-connection",
        visible_alias = "a-conn",
        value_name = "A_CONNECTION_ID",
        required = true,
        groups = &["b_chain_group", "new_client_group"],
        help_heading = "FLAGS",
        help = "Identifier of the connection on chain `a` to use in creating the new channel"
    )]
    connection_a: Option<ConnectionId>,

    #[clap(
        long = "a-port",
        required = true,
        value_name = "A_PORT_ID",
        help_heading = "FLAGS",
        help = "Identifier of the side `a` port for the new channel"
    )]
    port_a: PortId,

    #[clap(
        long = "b-port",
        required = true,
        value_name = "B_PORT_ID",
        help_heading = "FLAGS",
        help = "Identifier of the side `b` port for the new channel"
    )]
    port_b: PortId,

    #[clap(
        long = "order",
        value_name = "ORDER",
        help = "The channel ordering, valid options 'unordered' (default) and 'ordered'",
        default_value_t
    )]
    order: Ordering,

    #[clap(
        long = "channel-version",
        visible_alias = "chan-version",
        value_name = "VERSION",
        help = "The version for the new channel"
    )]
    version: Option<Version>,

    #[clap(
        long = "new-client-connection",
        visible_alias = "new-client-conn",
        required = true,
        group = "new_client_group",
        help = "Indicates that a new client and connection will be created underlying the new channel"
    )]
    new_client_connection: bool,

    #[clap(
        long = "yes",
        requires = "new-client-connection",
        help = "Skip new_client_connection confirmation"
    )]
    yes: bool,
}

impl Runnable for CreateChannelCommand {
    fn run(&self) {
        match &self.connection_a {
            Some(conn) => self.run_reusing_connection(conn),
            None => {
                if let Some(chain_b) = &self.chain_b {
                    if self.new_client_connection {
                        if self.yes {
                            self.run_using_new_connection(chain_b);
                        } else {
                            match Confirm::new()
                                .with_prompt(format!(
                                    "{}: {}\n{}: {}",
                                    style("WARN").yellow(),
                                    PROMPT,
                                    style("Hint").cyan(),
                                    HINT
                                ))
                                .interact()
                            {
                                Ok(confirm) => {
                                    if confirm {
                                        self.run_using_new_connection(chain_b);
                                    } else {
                                        Output::error("You elected not to create new clients and connections. Please re-invoke `create channel` with a pre-existing connection ID".to_string()).exit();
                                    }
                                }
                                Err(e) => {
                                    Output::error(format!(
                                        "An error occurred while waiting for user input: {e}"
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

impl CreateChannelCommand {
    /// Creates a new channel, as well as a new underlying connection and clients.
    fn run_using_new_connection(&self, chain_b: &ChainId) {
        let config = app_config();

        let chains = ChainHandlePair::spawn(&config, &self.chain_a, chain_b)
            .unwrap_or_else(exit_with_unrecoverable_error);

        info!(
            "Creating new clients, new connection, and a new channel with order {}",
            self.order
        );

        let client_a = ForeignClient::new(chains.src.clone(), chains.dst.clone())
            .unwrap_or_else(exit_with_unrecoverable_error);
        let client_b = ForeignClient::new(chains.dst.clone(), chains.src)
            .unwrap_or_else(exit_with_unrecoverable_error);

        // Create the connection.
        let con = Connection::new(client_a, client_b, connection_delay())
            .unwrap_or_else(exit_with_unrecoverable_error);

        // Finally create the channel.
        let channel = Channel::new(
            con,
            self.order,
            self.port_a.clone(),
            self.port_b.clone(),
            self.version.clone(),
        )
        .unwrap_or_else(exit_with_unrecoverable_error);

        Output::success(channel).exit();
    }

    /// Creates a new channel, reusing an already existing connection and its clients.
    fn run_reusing_connection(&self, connection_a: &ConnectionId) {
        let config = app_config();

        // Validate & spawn runtime for side a.
        let chain_a = spawn_chain_runtime(&config, &self.chain_a)
            .unwrap_or_else(exit_with_unrecoverable_error);

        // Query the connection end.
        let (conn_end, _) = chain_a
            .query_connection(
                QueryConnectionRequest {
                    connection_id: connection_a.clone(),
                    height: QueryHeight::Latest,
                },
                IncludeProof::No,
            )
            .unwrap_or_else(exit_with_unrecoverable_error);

        // Query the client state, obtain the identifier of chain b.
        let chain_b = chain_a
            .query_client_state(
                QueryClientStateRequest {
                    client_id: conn_end.client_id().clone(),
                    height: QueryHeight::Latest,
                },
                IncludeProof::No,
            )
            .map(|(cs, _)| cs.chain_id())
            .unwrap_or_else(exit_with_unrecoverable_error);

        // Spawn the runtime for side b.
        let chain_b =
            spawn_chain_runtime(&config, &chain_b).unwrap_or_else(exit_with_unrecoverable_error);

        // Create the foreign client handles.
        let client_a = ForeignClient::find(chain_b.clone(), chain_a.clone(), conn_end.client_id())
            .unwrap_or_else(exit_with_unrecoverable_error);
        let client_b = ForeignClient::find(chain_a, chain_b, conn_end.counterparty().client_id())
            .unwrap_or_else(exit_with_unrecoverable_error);

        let identified_end = IdentifiedConnectionEnd::new(connection_a.clone(), conn_end);

        let connection = Connection::find(client_a, client_b, &identified_end)
            .unwrap_or_else(exit_with_unrecoverable_error);

        let channel = Channel::new(
            connection,
            self.order,
            self.port_a.clone(),
            self.port_b.clone(),
            self.version.clone(),
        )
        .unwrap_or_else(exit_with_unrecoverable_error);

        Output::success(channel).exit();
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::CreateChannelCommand;
    use abscissa_core::clap::Parser;

    use ibc_relayer_types::core::ics04_channel::channel::Ordering;
    use ibc_relayer_types::core::ics04_channel::version::Version;
    use ibc_relayer_types::core::ics24_host::identifier::{ChainId, ConnectionId, PortId};

    #[test]
    fn test_create_channel_a_conn_required() {
        assert_eq!(
            CreateChannelCommand {
                chain_a: ChainId::from_string("chain_a"),
                chain_b: None,
                connection_a: Some(ConnectionId::from_str("connection_a").unwrap()),
                port_a: PortId::from_str("port_id_a").unwrap(),
                port_b: PortId::from_str("port_id_b").unwrap(),
                order: Ordering::Unordered,
                version: None,
                new_client_connection: false,
                yes: false
            },
            CreateChannelCommand::parse_from([
                "test",
                "--a-chain",
                "chain_a",
                "--a-connection",
                "connection_a",
                "--a-port",
                "port_id_a",
                "--b-port",
                "port_id_b"
            ])
        )
    }

    #[test]
    fn test_create_channel_version() {
        assert_eq!(
            CreateChannelCommand {
                chain_a: ChainId::from_string("chain_a"),
                chain_b: None,
                connection_a: Some(ConnectionId::from_str("connection_a").unwrap()),
                port_a: PortId::from_str("port_id_a").unwrap(),
                port_b: PortId::from_str("port_id_b").unwrap(),
                order: Ordering::Unordered,
                version: Some(Version::new("v1".to_owned())),
                new_client_connection: false,
                yes: false
            },
            CreateChannelCommand::parse_from([
                "test",
                "--a-chain",
                "chain_a",
                "--a-connection",
                "connection_a",
                "--a-port",
                "port_id_a",
                "--b-port",
                "port_id_b",
                "--channel-version",
                "v1"
            ])
        )
    }

    #[test]
    fn test_create_channel_order() {
        assert_eq!(
            CreateChannelCommand {
                chain_a: ChainId::from_string("chain_a"),
                chain_b: None,
                connection_a: Some(ConnectionId::from_str("connection_a").unwrap()),
                port_a: PortId::from_str("port_id_a").unwrap(),
                port_b: PortId::from_str("port_id_b").unwrap(),
                order: Ordering::Ordered,
                version: None,
                new_client_connection: false,
                yes: false
            },
            CreateChannelCommand::parse_from([
                "test",
                "--a-chain",
                "chain_a",
                "--a-connection",
                "connection_a",
                "--a-port",
                "port_id_a",
                "--b-port",
                "port_id_b",
                "--order",
                "ordered"
            ])
        )
    }

    #[test]
    fn test_create_channel_a_conn_alias() {
        assert_eq!(
            CreateChannelCommand {
                chain_a: ChainId::from_string("chain_a"),
                chain_b: None,
                connection_a: Some(ConnectionId::from_str("connection_a").unwrap()),
                port_a: PortId::from_str("port_id_a").unwrap(),
                port_b: PortId::from_str("port_id_b").unwrap(),
                order: Ordering::Unordered,
                version: None,
                new_client_connection: false,
                yes: false
            },
            CreateChannelCommand::parse_from([
                "test",
                "--a-chain",
                "chain_a",
                "--a-conn",
                "connection_a",
                "--a-port",
                "port_id_a",
                "--b-port",
                "port_id_b"
            ])
        )
    }

    #[test]
    fn test_create_channel_a_conn_with_new_client_conn() {
        assert!(CreateChannelCommand::try_parse_from([
            "test",
            "--a-chain",
            "chain_a",
            "--a-connection",
            "connection_a",
            "--a-port",
            "port_id_a",
            "--b-port",
            "port_id_b",
            "--new-client-connection"
        ])
        .is_err())
    }

    #[test]
    fn test_create_channel_a_conn_with_yes() {
        assert!(CreateChannelCommand::try_parse_from([
            "test",
            "--a-chain",
            "chain_a",
            "--b-chain",
            "chain_b",
            "--a-connection",
            "connection_a",
            "--a-port",
            "port_id_a",
            "--b-port",
            "port_id_b",
            "--order",
            "ordered",
            "--yes"
        ])
        .is_err())
    }

    #[test]
    fn test_create_channel_a_conn_with_b_chain() {
        assert!(CreateChannelCommand::try_parse_from([
            "test",
            "--a-chain",
            "chain_a",
            "--b-chain",
            "chain_b",
            "--a-connection",
            "connection_a",
            "--a-port",
            "port_id_a",
            "--b-port",
            "port_id_b",
            "--order",
            "ordered"
        ])
        .is_err())
    }

    #[test]
    fn test_create_channel_b_chain() {
        assert_eq!(
            CreateChannelCommand {
                chain_a: ChainId::from_string("chain_a"),
                chain_b: Some(ChainId::from_string("chain_b")),
                connection_a: None,
                port_a: PortId::from_str("port_id_a").unwrap(),
                port_b: PortId::from_str("port_id_b").unwrap(),
                order: Ordering::Unordered,
                version: None,
                new_client_connection: true,
                yes: false
            },
            CreateChannelCommand::parse_from([
                "test",
                "--a-chain",
                "chain_a",
                "--b-chain",
                "chain_b",
                "--a-port",
                "port_id_a",
                "--b-port",
                "port_id_b",
                "--new-client-connection"
            ])
        )
    }

    #[test]
    fn test_create_channel_b_chain_yes() {
        assert_eq!(
            CreateChannelCommand {
                chain_a: ChainId::from_string("chain_a"),
                chain_b: Some(ChainId::from_string("chain_b")),
                connection_a: None,
                port_a: PortId::from_str("port_id_a").unwrap(),
                port_b: PortId::from_str("port_id_b").unwrap(),
                order: Ordering::Unordered,
                version: None,
                new_client_connection: true,
                yes: true
            },
            CreateChannelCommand::parse_from([
                "test",
                "--a-chain",
                "chain_a",
                "--b-chain",
                "chain_b",
                "--a-port",
                "port_id_a",
                "--b-port",
                "port_id_b",
                "--new-client-connection",
                "--yes"
            ])
        )
    }

    #[test]
    fn test_create_channel_b_new_client_conn_alias() {
        assert_eq!(
            CreateChannelCommand {
                chain_a: ChainId::from_string("chain_a"),
                chain_b: Some(ChainId::from_string("chain_b")),
                connection_a: None,
                port_a: PortId::from_str("port_id_a").unwrap(),
                port_b: PortId::from_str("port_id_b").unwrap(),
                order: Ordering::Unordered,
                version: None,
                new_client_connection: true,
                yes: false
            },
            CreateChannelCommand::parse_from([
                "test",
                "--a-chain",
                "chain_a",
                "--b-chain",
                "chain_b",
                "--a-port",
                "port_id_a",
                "--b-port",
                "port_id_b",
                "--new-client-conn"
            ])
        )
    }

    #[test]
    fn test_create_channel_b_chain_without_new_client() {
        assert!(CreateChannelCommand::try_parse_from([
            "test",
            "--a-chain",
            "chain_a",
            "--b-chain",
            "chain_b",
            "--a-port",
            "port_id_a",
            "--b-port",
            "port_id_b"
        ])
        .is_err())
    }

    #[test]
    fn test_create_channel_no_b_port() {
        assert!(CreateChannelCommand::try_parse_from([
            "test",
            "--a-chain",
            "chain_a",
            "--b-chain",
            "chain_b",
            "--a-connection",
            "connection_a",
            "--a-port",
            "port_id_a"
        ])
        .is_err())
    }

    #[test]
    fn test_create_channel_no_a_port() {
        assert!(CreateChannelCommand::try_parse_from([
            "test",
            "--a-chain",
            "chain_a",
            "--b-chain",
            "chain_b",
            "--a-connection",
            "connection_a",
            "--b-port",
            "port_id_b"
        ])
        .is_err())
    }

    #[test]
    fn test_create_channel_no_b_chain_nor_a_conn() {
        assert!(CreateChannelCommand::try_parse_from([
            "test",
            "--a-chain",
            "chain_a",
            "--a-port",
            "port_id_a",
            "--b-port",
            "port_id_b"
        ])
        .is_err())
    }

    #[test]
    fn test_create_channel_no_a_chain() {
        assert!(CreateChannelCommand::try_parse_from([
            "test",
            "--b-chain",
            "chain_b",
            "--a-connection",
            "connection_a",
            "--a-port",
            "port_id_a",
            "--b-port",
            "port_id_b"
        ])
        .is_err())
    }
}
