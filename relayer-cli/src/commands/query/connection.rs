use alloc::sync::Arc;

use abscissa_core::{Command, Options, Runnable};
use tokio::runtime::Runtime as TokioRuntime;

use ibc::{
    ics03_connection::connection::State,
    ics24_host::identifier::ConnectionId,
    ics24_host::identifier::{ChainId, PortChannelId},
};
use ibc_proto::ibc::core::channel::v1::QueryConnectionChannelsRequest;
use ibc_relayer::chain::{ChainEndpoint, CosmosSdkChain};

use crate::conclude::Output;
use crate::error::Error;
use crate::prelude::*;

#[derive(Clone, Command, Debug, Options)]
pub struct QueryConnectionEndCmd {
    #[options(free, required, help = "identifier of the chain to query")]
    chain_id: ChainId,

    #[options(free, required, help = "identifier of the connection to query")]
    connection_id: ConnectionId,

    #[options(help = "height of the state to query", short = "h")]
    height: Option<u64>,
}

// cargo run --bin hermes -- query connection end ibc-test connectionidone --height 3
impl Runnable for QueryConnectionEndCmd {
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

        let height = ibc::Height::new(chain.id().version(), self.height.unwrap_or(0_u64));
        let res = chain.query_connection(&self.connection_id, height);
        match res {
            Ok(connection_end) => {
                if connection_end.state_matches(&State::Uninitialized) {
                    Output::error(format!(
                        "connection '{}' does not exist",
                        self.connection_id
                    ))
                    .exit()
                } else {
                    Output::success(connection_end).exit()
                }
            }
            Err(e) => Output::error(format!("{}", e)).exit(),
        }
    }
}

/// Command for querying the channel identifiers associated with a connection.
/// Sample invocation:
/// `cargo run --bin hermes -- query connection channels ibc-0 connection-0`
#[derive(Clone, Command, Debug, Options)]
pub struct QueryConnectionChannelsCmd {
    #[options(free, required, help = "identifier of the chain to query")]
    chain_id: ChainId,

    #[options(free, required, help = "identifier of the connection to query")]
    connection_id: ConnectionId,
}

impl Runnable for QueryConnectionChannelsCmd {
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

        let req = QueryConnectionChannelsRequest {
            connection: self.connection_id.to_string(),
            pagination: ibc_proto::cosmos::base::query::pagination::all(),
        };

        let res: Result<_, Error> = chain.query_connection_channels(req).map_err(Error::relayer);

        match res {
            Ok(channels) => {
                let ids: Vec<PortChannelId> = channels
                    .into_iter()
                    .map(|identified_channel| PortChannelId {
                        port_id: identified_channel.port_id,
                        channel_id: identified_channel.channel_id,
                    })
                    .collect();
                Output::success(ids).exit()
            }
            Err(e) => Output::error(format!("{}", e)).exit(),
        }
    }
}
