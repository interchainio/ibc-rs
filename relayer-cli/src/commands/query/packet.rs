use abscissa_core::{Command, Options, Runnable};

mod ack;
mod acks;
mod commitment;
mod commitments;
mod unreceived_acks;
mod unreceived_packets;

#[derive(Command, Debug, Options, Runnable)]
pub enum QueryPacketCmds {
    /// The `query packet commitments` subcommand
    #[options(help = "Query packet commitments")]
    Commitments(commitments::QueryPacketCommitmentsCmd),

    /// The `query packet commitment` subcommand
    #[options(help = "Query packet commitment")]
    Commitment(commitment::QueryPacketCommitmentCmd),

    /// The `query packet acks` subcommand
    #[options(help = "Query packet acknowledgments")]
    Acks(acks::QueryPacketAcknowledgementsCmd),

    /// The `query packet ack` subcommand
    #[options(help = "Query packet acknowledgment")]
    Ack(ack::QueryPacketAcknowledgmentCmd),

    /// The `query packet unreceived-packets` subcommand
    #[options(help = "Query unreceived packets")]
    UnreceivedPackets(unreceived_packets::QueryUnreceivedPacketsCmd),

    /// The `query packet unreceived-acks` subcommand
    #[options(help = "Query unreceived acknowledgments")]
    UnreceivedAcks(unreceived_acks::QueryUnreceivedAcknowledgementCmd),
}
