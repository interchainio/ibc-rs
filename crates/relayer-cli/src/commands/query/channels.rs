use core::fmt::{Debug, Error, Formatter};

use abscissa_core::clap::Parser;
use abscissa_core::Runnable;
use serde::Serialize;

use eyre::eyre;
use ibc_relayer::chain::handle::{BaseChainHandle, ChainHandle};
use ibc_relayer::chain::requests::{
    IncludeProof, PageRequest, QueryChannelRequest, QueryChannelsRequest, QueryClientStateRequest,
    QueryConnectionRequest, QueryHeight,
};
use ibc_relayer::registry::Registry;
use ibc_relayer_types::core::ics04_channel::channel::{ChannelEnd, State};
use ibc_relayer_types::core::ics24_host::identifier::{
    ChainId, ChannelId, ConnectionId, PortChannelId, PortId,
};

use crate::commands::query::channel_ends::ChannelEnds;
use crate::conclude::Output;
use crate::prelude::*;

#[derive(Clone, Command, Debug, Parser, PartialEq, Eq)]
pub struct QueryChannelsCmd {
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
        help = "Filter the query response by the this counterparty chain"
    )]
    dst_chain_id: Option<ChainId>,

    #[clap(
        long = "verbose",
        help = "Enable verbose output, displaying the client and connection ids for each channel in the response"
    )]
    verbose: bool,

    #[clap(
        long = "show-counterparty",
        help = "Show the counterparty chain, port, and channel"
    )]
    show_counterparty: bool,
}

fn run_query_channels<Chain: ChainHandle>(
    cmd: &QueryChannelsCmd,
) -> eyre::Result<QueryChannelsOutput> {
    let mut output = match (cmd.verbose, cmd.show_counterparty) {
        (true, _) => QueryChannelsOutput::verbose(),
        (false, true) => QueryChannelsOutput::pretty(),
        (false, false) => QueryChannelsOutput::summary(),
    };

    let config = app_config();
    let chain_id = cmd.chain_id.clone();

    let mut registry = <Registry<Chain>>::new((*config).clone());
    let chain = registry.get_or_spawn(&cmd.chain_id)?;
    let chain_height = chain.query_latest_height()?;

    let identified_channels = chain.query_channels(QueryChannelsRequest {
        pagination: Some(PageRequest::all()),
    })?;

    for identified_channel in identified_channels {
        let port_id = identified_channel.port_id;
        let channel_id = identified_channel.channel_id;
        let chain_id = chain_id.clone();
        let channel_end = identified_channel.channel_end;

        if channel_end.state_matches(&State::Uninitialized) {
            return Err(eyre!(
                "{}/{} on chain {} @ {:?} is uninitialized",
                port_id,
                channel_id,
                chain_id,
                chain_height
            ));
        }

        let connection_id = channel_end
            .connection_hops
            .first()
            .ok_or_else(|| {
                eyre!(
                    "missing connection_hops for {}/{} on chain {} @ {:?}",
                    port_id,
                    channel_id,
                    chain_id,
                    chain_height
                )
            })?
            .clone();

        let mut counterparty_chain_id = None;

        // If a counterparty chain is specified as a filter, check and skip the
        // channel if required.
        if cmd.show_counterparty || cmd.dst_chain_id.is_some() {
            let (connection_end, _) = chain.query_connection(
                QueryConnectionRequest {
                    connection_id: connection_id.clone(),
                    height: QueryHeight::Specific(chain_height),
                },
                IncludeProof::No,
            )?;

            let client_id = connection_end.client_id().clone();
            let (client_state, _) = chain.query_client_state(
                QueryClientStateRequest {
                    client_id,
                    height: QueryHeight::Specific(chain_height),
                },
                IncludeProof::No,
            )?;
            let cid = client_state.chain_id().clone();

            if let Some(dst_chain_id) = &cmd.dst_chain_id {
                if cid != *dst_chain_id {
                    continue;
                }
            }

            counterparty_chain_id = Some(cid);
        }

        match output {
            QueryChannelsOutput::Verbose(_) => {
                match query_channel_ends(
                    &mut registry,
                    &chain,
                    channel_end,
                    connection_id,
                    chain_id,
                    port_id,
                    channel_id,
                    QueryHeight::Specific(chain_height),
                ) {
                    Ok(channel_ends) => output.push_verbose(channel_ends),
                    Err(e) => error!("failed to query channel ends: {}", e),
                }
            }
            QueryChannelsOutput::Pretty(_) => {
                // Get counterparty channel_id and port_id
                let counterparty_channel = channel_end.counterparty().clone();
                let counterparty_channel_id = counterparty_channel.channel_id;
                let counterparty_channel_port = counterparty_channel.port_id;

                let pretty = PrettyOutput {
                    channel_a: channel_id,
                    port_a: port_id,
                    chain_id_a: chain_id,
                    channel_b: counterparty_channel_id,
                    port_b: counterparty_channel_port,
                    chain_id_b: counterparty_chain_id,
                };

                output.push_pretty(pretty)
            }
            QueryChannelsOutput::Summary(_) => {
                output.push_summary(PortChannelId {
                    channel_id,
                    port_id,
                });
            }
        }
    }

    Ok(output)
}

#[allow(clippy::too_many_arguments)]
fn query_channel_ends<Chain: ChainHandle>(
    registry: &mut Registry<Chain>,
    chain: &Chain,
    channel_end: ChannelEnd,
    connection_id: ConnectionId,
    chain_id: ChainId,
    port_id: PortId,
    channel_id: ChannelId,
    chain_height_query: QueryHeight,
) -> eyre::Result<ChannelEnds> {
    let (connection_end, _) = chain.query_connection(
        QueryConnectionRequest {
            connection_id: connection_id.clone(),
            height: chain_height_query,
        },
        IncludeProof::No,
    )?;
    let client_id = connection_end.client_id().clone();
    let (client_state, _) = chain.query_client_state(
        QueryClientStateRequest {
            client_id,
            height: chain_height_query,
        },
        IncludeProof::No,
    )?;
    let counterparty_chain_id = client_state.chain_id();

    let channel_counterparty = channel_end.counterparty().clone();
    let connection_counterparty = connection_end.counterparty().clone();
    let counterparty_client_id = connection_counterparty.client_id().clone();

    let counterparty_connection_id = connection_counterparty.connection_id.ok_or_else(|| {
        eyre!(
            "connection end for {} on chain {} @ {:?} does not have counterparty connection id: {:?}",
            connection_id,
            chain_id,
            chain_height_query,
            connection_end
        )
    })?;

    let counterparty_port_id = channel_counterparty.port_id().clone();

    let counterparty_channel_id =
        channel_counterparty.channel_id.ok_or_else(|| {
            eyre!(
            "channel end for {}/{} on chain {} @ {:?} does not have counterparty channel id: {:?}",
            port_id, channel_id, chain_id, chain_height_query, channel_end
        )
        })?;

    let counterparty_chain = registry.get_or_spawn(&counterparty_chain_id)?;
    let counterparty_chain_height_query =
        QueryHeight::Specific(counterparty_chain.query_latest_height()?);

    let (counterparty_connection_end, _) = counterparty_chain.query_connection(
        QueryConnectionRequest {
            connection_id: counterparty_connection_id,
            height: counterparty_chain_height_query,
        },
        IncludeProof::No,
    )?;

    let (counterparty_client_state, _) = counterparty_chain.query_client_state(
        QueryClientStateRequest {
            client_id: counterparty_client_id,
            height: counterparty_chain_height_query,
        },
        IncludeProof::No,
    )?;

    let (counterparty_channel_end, _) = counterparty_chain.query_channel(
        QueryChannelRequest {
            port_id: counterparty_port_id,
            channel_id: counterparty_channel_id,
            height: counterparty_chain_height_query,
        },
        IncludeProof::No,
    )?;

    Ok(ChannelEnds {
        channel_end,
        connection_end,
        client_state,
        counterparty_channel_end,
        counterparty_connection_end,
        counterparty_client_state,
    })
}

impl Runnable for QueryChannelsCmd {
    fn run(&self) {
        match run_query_channels::<BaseChainHandle>(self) {
            Ok(output) => Output::success(output).exit(),
            Err(e) => Output::error(e).exit(),
        }
    }
}

#[derive(Serialize, Debug)]
struct PrettyOutput {
    channel_a: ChannelId,
    port_a: PortId,
    chain_id_a: ChainId,
    channel_b: Option<ChannelId>,
    port_b: PortId,
    chain_id_b: Option<ChainId>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum QueryChannelsOutput {
    Verbose(Vec<ChannelEnds>),
    Summary(Vec<PortChannelId>),
    Pretty(Vec<PrettyOutput>),
}

impl QueryChannelsOutput {
    fn verbose() -> Self {
        Self::Verbose(Vec::new())
    }

    fn summary() -> Self {
        Self::Summary(Vec::new())
    }

    fn pretty() -> Self {
        Self::Pretty(Vec::new())
    }

    fn push_pretty(&mut self, pe: PrettyOutput) {
        match self {
            Self::Pretty(pes) => pes.push(pe),
            Self::Verbose(_) => {
                Output::error("PrettyOutput and QueryChannelsOutput::Verbose are incompatible")
                    .exit()
            }
            Self::Summary(_) => {
                Output::error("PrettyOutput and QueryChannelsOutput::Summary are incompatible")
                    .exit()
            }
        }
    }

    fn push_verbose(&mut self, ce: ChannelEnds) {
        match self {
            Self::Pretty(_) => {
                Output::error("ChannelEnds and QueryChannelsOutput::Pretty are incompatible").exit()
            }
            Self::Verbose(ces) => ces.push(ce),
            Self::Summary(_) => {
                Output::error("ChannelEnds and QueryChannelsOutput::Summary are incompatible")
                    .exit()
            }
        }
    }

    fn push_summary(&mut self, pc: PortChannelId) {
        match self {
            Self::Pretty(_) => {
                Output::error("PortChannelId and QueryChannelsOutput::Pretty are incompatible")
                    .exit()
            }
            Self::Verbose(_) => {
                Output::error("PortChannelId and QueryChannelsOutput::Verbose are incompatible")
                    .exit()
            }
            Self::Summary(pcs) => pcs.push(pc),
        }
    }
}

impl Debug for QueryChannelsOutput {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        match self {
            QueryChannelsOutput::Verbose(output) => write!(f, "{output:#?}"),
            QueryChannelsOutput::Summary(output) => write!(f, "{output:#?}"),
            QueryChannelsOutput::Pretty(output) => {
                output.iter().try_for_each(|pretty_print| {
                    write!(
                        f,
                        "\n{}: {}/{} --- {}: {}/{}",
                        &pretty_print.chain_id_a.as_str(),
                        &pretty_print.port_a.as_str(),
                        &pretty_print.channel_a.as_str(),
                        match &pretty_print.chain_id_b {
                            Some(chain_id_b) => chain_id_b.as_str(),
                            None => "None",
                        },
                        &pretty_print.port_b.as_str(),
                        match &pretty_print.channel_b {
                            Some(channel_b) => channel_b.as_str(),
                            None => "None",
                        },
                    )
                })?;
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::QueryChannelsCmd;

    use abscissa_core::clap::Parser;
    use ibc_relayer_types::core::ics24_host::identifier::ChainId;

    #[test]
    fn test_query_channels_required_only() {
        assert_eq!(
            QueryChannelsCmd {
                chain_id: ChainId::from_string("chain_id"),
                verbose: false,
                dst_chain_id: None,
                show_counterparty: false,
            },
            QueryChannelsCmd::parse_from(["test", "--chain", "chain_id"])
        )
    }

    #[test]
    fn test_query_channels_verbose() {
        assert_eq!(
            QueryChannelsCmd {
                chain_id: ChainId::from_string("chain_id"),
                verbose: true,
                dst_chain_id: None,
                show_counterparty: false,
            },
            QueryChannelsCmd::parse_from(["test", "--chain", "chain_id", "--verbose"])
        )
    }

    #[test]
    fn test_query_channels_counterparty_chain() {
        assert_eq!(
            QueryChannelsCmd {
                chain_id: ChainId::from_string("chain_id"),
                verbose: false,
                dst_chain_id: Some(ChainId::from_string("counterparty_chain")),
                show_counterparty: false,
            },
            QueryChannelsCmd::parse_from([
                "test",
                "--chain",
                "chain_id",
                "--counterparty-chain",
                "counterparty_chain"
            ])
        )
    }

    #[test]
    fn test_query_channels_no_chain() {
        assert!(QueryChannelsCmd::try_parse_from(["test"]).is_err())
    }

    #[test]
    fn test_query_channels_show_counterparty() {
        assert_eq!(
            QueryChannelsCmd {
                chain_id: ChainId::from_string("chain_id"),
                verbose: false,
                dst_chain_id: None,
                show_counterparty: true
            },
            QueryChannelsCmd::parse_from(["test", "--chain", "chain_id", "--show-counterparty",])
        )
    }

    #[test]
    fn test_query_channels_show_counterparty_dst_chain() {
        assert_eq!(
            QueryChannelsCmd {
                chain_id: ChainId::from_string("chain_id"),
                verbose: false,
                dst_chain_id: Some(ChainId::from_string("counterparty_chain")),
                show_counterparty: true
            },
            QueryChannelsCmd::parse_from([
                "test",
                "--chain",
                "chain_id",
                "--show-counterparty",
                "--counterparty-chain",
                "counterparty_chain"
            ])
        )
    }

    #[test]
    fn test_query_channels_show_counterparty_verbose() {
        assert_eq!(
            QueryChannelsCmd {
                chain_id: ChainId::from_string("chain_id"),
                verbose: true,
                dst_chain_id: None,
                show_counterparty: true
            },
            QueryChannelsCmd::parse_from([
                "test",
                "--chain",
                "chain_id",
                "--show-counterparty",
                "--verbose",
            ])
        )
    }
}
