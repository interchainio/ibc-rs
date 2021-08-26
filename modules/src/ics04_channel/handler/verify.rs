use crate::ics02_client::client_state::ClientState;
use crate::ics02_client::{client_def::AnyClient, client_def::ClientDef};
use crate::ics03_connection::connection::ConnectionEnd;
use crate::ics04_channel::channel::ChannelEnd;
use crate::ics04_channel::context::ChannelReader;
use crate::ics04_channel::error::Error;
use crate::ics04_channel::packet::{Packet, Sequence};
use crate::ics24_host::identifier::ClientId;
use crate::proofs::Proofs;

/// Entry point for verifying all proofs bundled in any ICS4 message for channel protocols.
pub fn verify_channel_proofs(
    ctx: &dyn ChannelReader,
    channel_end: &ChannelEnd,
    connection_end: &ConnectionEnd,
    expected_chan: &ChannelEnd,
    proofs: &Proofs,
) -> Result<(), Error> {
    // This is the client which will perform proof verification.
    let client_id = connection_end.client_id().clone();

    let client_state = ctx.client_state(&client_id)?;

    // The client must not be frozen.
    if client_state.is_frozen() {
        return Err(Error::frozen_client(client_id));
    }

    ctx.client_consensus_state(&client_id, proofs.height())?;

    let client_def = AnyClient::from_client_type(client_state.client_type());

    // Verify the proof for the channel state against the expected channel end.
    // A counterparty channel id of None in not possible, and is checked by validate_basic in msg.
    client_def
        .verify_channel_state(
            &client_state,
            proofs.height(),
            connection_end.counterparty().prefix(),
            proofs.object_proof(),
            channel_end.counterparty().port_id(),
            channel_end.counterparty().channel_id().unwrap(),
            expected_chan,
        )
        .map_err(Error::verify_channel_failed)
}

/// Entry point for verifying all proofs bundled in a ICS4 packet recv. message.
pub fn verify_packet_recv_proofs(
    ctx: &dyn ChannelReader,
    packet: &Packet,
    client_id: ClientId,
    proofs: &Proofs,
) -> Result<(), Error> {
    let client_state = ctx.client_state(&client_id)?;

    // The client must not be frozen.
    if client_state.is_frozen() {
        return Err(Error::frozen_client(client_id));
    }

    ctx.client_consensus_state(&client_id, proofs.height())?;

    let client_def = AnyClient::from_client_type(client_state.client_type());

    let input = format!(
        "{:?},{:?},{:?}",
        packet.timeout_timestamp, packet.timeout_height, packet.data
    );
    let commitment = ctx.hash(input);

    // Verify the proof for the packet against the chain store.
    client_def
        .verify_packet_data(
            &client_state,
            proofs.height(),
            proofs.object_proof(),
            &packet.source_port,
            &packet.source_channel,
            &packet.sequence,
            commitment,
        )
        .map_err(|e| Error::packet_verification_failed(packet.sequence, e))?;

    Ok(())
}

/// Entry point for verifying all proofs bundled in an ICS4 packet ack message.
pub fn verify_packet_acknowledgement_proofs(
    ctx: &dyn ChannelReader,
    packet: &Packet,
    acknowledgement: Vec<u8>,
    client_id: ClientId,
    proofs: &Proofs,
) -> Result<(), Error> {
    let client_state = ctx.client_state(&client_id)?;

    // The client must not be frozen.
    if client_state.is_frozen() {
        return Err(Error::frozen_client(client_id));
    }

    let client_def = AnyClient::from_client_type(client_state.client_type());

    // Verify the proof for the packet against the chain store.
    client_def
        .verify_packet_acknowledgement(
            &client_state,
            proofs.height(),
            proofs.object_proof(),
            &packet.source_port,
            &packet.source_channel,
            &packet.sequence,
            acknowledgement,
        )
        .map_err(|e| Error::packet_verification_failed(packet.sequence, e))?;

    Ok(())
}

/// Entry point for verifying all timeout proofs.
pub fn verify_next_sequence_recv(
    ctx: &dyn ChannelReader,
    client_id: ClientId,
    packet: Packet,
    seq: Sequence,
    proofs: &Proofs,
) -> Result<(), Error> {
    let client_state = ctx.client_state(&client_id)?;

    // The client must not be frozen.
    if client_state.is_frozen() {
        return Err(Error::frozen_client(client_id));
    }

    let client_def = AnyClient::from_client_type(client_state.client_type());

    // Verify the proof for the packet against the chain store.
    client_def
        .verify_next_sequence_recv(
            &client_state,
            proofs.height(),
            proofs.object_proof(),
            &packet.destination_port,
            &packet.destination_channel,
            &seq,
        )
        .map_err(|e| Error::packet_verification_failed(seq, e))?;

    Ok(())
}

pub fn verify_packet_receipt_absence(
    ctx: &dyn ChannelReader,
    client_id: ClientId,
    packet: Packet,
    proofs: &Proofs,
) -> Result<(), Error> {
    let client_state = ctx.client_state(&client_id)?;

    // The client must not be frozen.
    if client_state.is_frozen() {
        return Err(Error::frozen_client(client_id));
    }

    let client_def = AnyClient::from_client_type(client_state.client_type());

    // Verify the proof for the packet against the chain store.
    client_def
        .verify_packet_receipt_absence(
            &client_state,
            proofs.height(),
            proofs.object_proof(),
            &packet.destination_port,
            &packet.destination_channel,
            &packet.sequence,
        )
        .map_err(|e| Error::packet_verification_failed(packet.sequence, e))?;

    Ok(())
}
