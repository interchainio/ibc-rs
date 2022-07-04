use abscissa_core::clap::Parser;
use abscissa_core::{Command, Runnable};
use ibc_relayer::chain::requests::{IncludeProof, QueryHeight, QueryPacketAcknowledgementRequest};
use subtle_encoding::{Encoding, Hex};

use ibc::core::ics04_channel::packet::Sequence;
use ibc::core::ics24_host::identifier::{ChainId, ChannelId, PortId};
use ibc_relayer::chain::handle::ChainHandle;

use crate::cli_utils::spawn_chain_runtime;
use crate::conclude::{exit_with_unrecoverable_error, Output};
use crate::error::Error;
use crate::prelude::*;

#[derive(Clone, Command, Debug, Parser)]
pub struct QueryPacketAcknowledgmentCmd {
    #[clap(
        long = "chain",
        required = true,
        value_name = "CHAIN_ID",
        help = "Identifier of the chain to query"
    )]
    chain_id: ChainId,

    #[clap(
        long = "port",
        required = true,
        value_name = "PORT_ID",
        help = "Identifier of the port to query"
    )]
    port_id: PortId,

    #[clap(
        long = "channel",
        alias = "chan",
        required = true,
        value_name = "CHANNEL_ID",
        help = "Identifier of the channel to query"
    )]
    channel_id: ChannelId,

    #[clap(
        long = "sequence",
        alias = "seq",
        required = true,
        value_name = "SEQUENCE",
        help = "Sequence of packet to query"
    )]
    sequence: Sequence,

    #[clap(
        long = "height",
        value_name = "HEIGHT",
        help = "Height of the state to query. Leave unspecified for latest height."
    )]
    height: Option<u64>,
}

impl QueryPacketAcknowledgmentCmd {
    fn execute(&self) -> Result<String, Error> {
        let config = app_config();

        debug!("Options: {:?}", self);

        let chain = spawn_chain_runtime(&config, &self.chain_id)?;

        chain
            .query_packet_acknowledgement(
                QueryPacketAcknowledgementRequest {
                    port_id: self.port_id.clone(),
                    channel_id: self.channel_id.clone(),
                    sequence: self.sequence,
                    height: self.height.map_or(QueryHeight::Latest, |revision_height| {
                        QueryHeight::Specific(
                            ibc::Height::new(chain.id().version(), revision_height)
                                .unwrap_or_else(exit_with_unrecoverable_error),
                        )
                    }),
                },
                IncludeProof::No,
            )
            .map_err(Error::relayer)
            .map(|(bytes, _)| {
                Hex::upper_case()
                    .encode_to_string(bytes.clone())
                    .unwrap_or_else(|_| format!("{:?}", bytes))
            })
    }
}

impl Runnable for QueryPacketAcknowledgmentCmd {
    fn run(&self) {
        match self.execute() {
            Ok(hex) => Output::success(hex).exit(),
            Err(e) => Output::error(format!("{}", e)).exit(),
        }
    }
}
