use crate::application::ics20_fungible_token_transfer::context::Ics20Context;
use crate::application::ics20_fungible_token_transfer::error::Error;
use crate::application::ics20_fungible_token_transfer::msgs::transfer::MsgTransfer;
use crate::handler::HandlerOutput;
use crate::ics04_channel::handler::send_packet::send_packet;
use crate::ics04_channel::packet::Packet;
use crate::ics04_channel::packet::PacketResult;

pub(crate) fn send_transfer<Ctx>(
    ctx: &Ctx,
    msg: MsgTransfer,
) -> Result<HandlerOutput<PacketResult>, Error>
where
    Ctx: Ics20Context,
{
    let source_channel_end = ctx
        .channel_end(&(msg.source_port.clone(), msg.source_channel.clone()))
        .map_err(Error::ics04_channel)?;

    let destination_port = source_channel_end.counterparty().port_id().clone();
    let destination_channel = source_channel_end
        .counterparty()
        .channel_id()
        .ok_or_else(|| {
            Error::destination_channel_not_found(
                msg.source_port.clone(),
                msg.source_channel.clone(),
            )
        })?;

    // get the next sequence
    let sequence = ctx
        .get_next_sequence_send(&(msg.source_port.clone(), msg.source_channel.clone()))
        .map_err(Error::ics04_channel)?;

    //TODO: Application LOGIC.

    let packet = Packet {
        sequence,
        source_port: msg.source_port,
        source_channel: msg.source_channel,
        destination_port,
        destination_channel: destination_channel.clone(),
        data: vec![0],
        timeout_height: msg.timeout_height,
        timeout_timestamp: msg.timeout_timestamp,
    };

    let handler_output = send_packet(ctx, packet).map_err(Error::ics04_channel)?;

    //TODO:  add event/atributes and writes to the store issued by the application logic for packet sending.
    Ok(handler_output)
}
