//! ICS4 (channel) context. The two traits `ChannelReader ` and `ChannelKeeper` define
//! the interface that any host chain must implement to be able to process any `ChannelMsg`.
//!

use crate::ics02_client::client_def::{AnyClientState, AnyConsensusState};
use crate::ics03_connection::connection::ConnectionEnd;
use crate::ics04_channel::channel::ChannelEnd;
use crate::ics04_channel::error::Error;
use crate::ics04_channel::handler::{ChannelIdState, ChannelResult};
use crate::ics05_port::capabilities::Capability;
use crate::ics24_host::identifier::{ChannelId, ClientId, ConnectionId, PortId};
use crate::Height;

use super::packet::{PacketResult, Sequence};

/// A context supplying all the necessary read-only dependencies for processing any `ChannelMsg`.
pub trait ChannelReader {
    /// Returns the ChannelEnd for the given `port_id` and `chan_id`.
    fn channel_end(&self, port_channel_id: &(PortId, ChannelId)) -> Option<ChannelEnd>;

    /// Returns the ConnectionState for the given identifier `connection_id`.
    fn connection_end(&self, connection_id: &ConnectionId) -> Option<ConnectionEnd>;

    fn connection_channels(&self, cid: &ConnectionId) -> Option<Vec<(PortId, ChannelId)>>;

    /// Returns the ClientState for the given identifier `client_id`. Necessary dependency towards
    /// proof verification.
    fn client_state(&self, client_id: &ClientId) -> Option<AnyClientState>;

    fn client_consensus_state(
        &self,
        client_id: &ClientId,
        height: Height,
    ) -> Option<AnyConsensusState>;

    fn authenticated_capability(&self, port_id: &PortId) -> Result<Capability, Error>;

    fn get_next_sequence_send(&self, port_channel_id: &(PortId, ChannelId)) -> Option<Sequence>;

    fn get_next_sequence_recv(&self, port_channel_id: &(PortId, ChannelId)) -> Option<Sequence>;

    fn get_packet_receipt(&self, key: &(PortId, ChannelId, Sequence)) -> Option<String>;

    fn get_packet_acknowledgement(&self, key: &(PortId, ChannelId, Sequence)) -> Option<String>;

    /// A hashing function for packet commitments  
    fn hash(&self, value: String) -> String;

    /// Returns the current height of the local chain.
    fn host_height(&self) -> Height;

    /// Returns the current timestamp of the local chain.
    fn host_timestamp(&self) -> u64;

    /// Returns a counter on the number of channel ids have been created thus far.
    /// The value of this counter should increase only via method
    /// `ChannelKeeper::increase_channel_counter`.
    fn channel_counter(&self) -> u64;
}

/// A context supplying all the necessary write-only dependencies (i.e., storage writing facility)
/// for processing any `ChannelMsg`.
pub trait ChannelKeeper {
    fn store_channel_result(&mut self, result: ChannelResult) -> Result<(), Error> {
        // The handler processed this channel & some modifications occurred, store the new end.
        self.store_channel(
            (result.port_id.clone(), result.channel_id.clone()),
            &result.channel_end,
        )?;

        // The channel identifier was freshly brewed.
        // Increase counter & initialize seq. nrs.
        if matches!(result.channel_id_state, ChannelIdState::Generated) {
            self.increase_channel_counter();

            // Associate also the channel end to its connection.
            self.store_connection_channels(
                result.channel_end.connection_hops()[0].clone(),
                &(result.port_id.clone(), result.channel_id.clone()),
            )?;

            // Initialize send, recv, and ack sequence numbers.
            self.store_next_sequence_send(
                (result.port_id.clone(), result.channel_id.clone()),
                1.into(),
            )?;
            self.store_next_sequence_recv(
                (result.port_id.clone(), result.channel_id.clone()),
                1.into(),
            )?;
            self.store_next_sequence_ack((result.port_id, result.channel_id), 1.into())?;
        }

        Ok(())
    }

    fn store_packet_result(&mut self, general_result: PacketResult) -> Result<(), Error> {
        match general_result {
            PacketResult::Send(res) => {
                self.store_next_sequence_send(
                    (res.port_id.clone(), res.channel_id.clone()),
                    res.seq_number,
                )?;

                self.store_packet_commitment(
                    (res.port_id.clone(), res.channel_id.clone(), res.seq),
                    res.timeout_timestamp,
                    res.timeout_height,
                    res.data,
                )?;
            }
            PacketResult::Recv(res) => {
                if res.receipt.is_none() {
                    //Ordered cchannel
                    self.store_next_sequence_recv(
                        (res.port_id.clone(), res.channel_id.clone()),
                        res.seq_number,
                    )?;
                } else {
                    //Unorderd channel: store a receipt that does not contain any data, since the packet has not yet been processed,
                    // it's just a single store key set to an empty string to indicate that the packet has been received
                    self.store_packet_receipt(
                        (res.port_id.clone(), res.channel_id.clone(), res.seq),
                        "".to_string(),
                    )?;
                }
            }
            PacketResult::WriteAck(res) => {
                self.store_packet_acknowledgement(
                    (res.port_id.clone(), res.channel_id.clone(), res.seq),
                    res.ack,
                )?;
            }
        }
        Ok(())
    }

    fn store_packet_commitment(
        &mut self,
        key: (PortId, ChannelId, Sequence),
        timestamp: u64,
        heigh: Height,
        data: Vec<u8>,
    ) -> Result<(), Error>;

    fn store_packet_receipt(
        &mut self,
        key: (PortId, ChannelId, Sequence),
        receipt: String,
    ) -> Result<(), Error>;

    fn store_packet_acknowledgement(
        &mut self,
        key: (PortId, ChannelId, Sequence),
        ack: Vec<u8>,
    ) -> Result<(), Error>;

    fn store_connection_channels(
        &mut self,
        conn_id: ConnectionId,
        port_channel_id: &(PortId, ChannelId),
    ) -> Result<(), Error>;

    /// Stores the given channel_end at a path associated with the port_id and channel_id.
    fn store_channel(
        &mut self,
        port_channel_id: (PortId, ChannelId),
        channel_end: &ChannelEnd,
    ) -> Result<(), Error>;

    fn store_next_sequence_send(
        &mut self,
        port_channel_id: (PortId, ChannelId),
        seq: Sequence,
    ) -> Result<(), Error>;

    fn store_next_sequence_recv(
        &mut self,
        port_channel_id: (PortId, ChannelId),
        seq: Sequence,
    ) -> Result<(), Error>;

    fn store_next_sequence_ack(
        &mut self,
        port_channel_id: (PortId, ChannelId),
        seq: Sequence,
    ) -> Result<(), Error>;

    /// Called upon channel identifier creation (Init or Try message processing).
    /// Increases the counter which keeps track of how many channels have been created.
    /// Should never fail.
    fn increase_channel_counter(&mut self);
}
