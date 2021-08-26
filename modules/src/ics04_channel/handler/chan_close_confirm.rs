//! Protocol logic specific to ICS4 messages of type `MsgChannelCloseConfirm`.
use crate::events::IbcEvent;
use crate::handler::{HandlerOutput, HandlerResult};
use crate::ics03_connection::connection::State as ConnectionState;
use crate::ics04_channel::channel::{ChannelEnd, Counterparty, State};
use crate::ics04_channel::context::ChannelReader;
use crate::ics04_channel::error::Error;
use crate::ics04_channel::events::Attributes;
use crate::ics04_channel::handler::verify::verify_channel_proofs;
use crate::ics04_channel::handler::{ChannelIdState, ChannelResult};
use crate::ics04_channel::msgs::chan_close_confirm::MsgChannelCloseConfirm;

pub(crate) fn process(
    ctx: &dyn ChannelReader,
    msg: MsgChannelCloseConfirm,
) -> HandlerResult<ChannelResult, Error> {
    let mut output = HandlerOutput::builder();

    // Retrieve the old channel end and validate it against the message.
    let mut channel_end = ctx.channel_end(&(msg.port_id().clone(), msg.channel_id().clone()))?;

    // Validate that the channel end is in a state where it can be closed.
    if channel_end.state_matches(&State::Closed) {
        return Err(Error::channel_closed(msg.channel_id().clone()));
    }

    // Channel capabilities
    let channel_cap = ctx.authenticated_capability(&msg.port_id().clone())?;

    // An OPEN IBC connection running on the local (host) chain should exist.
    if channel_end.connection_hops().len() != 1 {
        return Err(Error::invalid_connection_hops_length(
            1,
            channel_end.connection_hops().len(),
        ));
    }

    let conn = ctx.connection_end(&channel_end.connection_hops()[0])?;

    if !conn.state_matches(&ConnectionState::Open) {
        return Err(Error::connection_not_open(
            channel_end.connection_hops()[0].clone(),
        ));
    }

    // Proof verification in two steps:
    // 1. Setup: build the Channel as we expect to find it on the other party.

    let expected_counterparty =
        Counterparty::new(msg.port_id().clone(), Some(msg.channel_id().clone()));

    let counterparty = conn.counterparty();
    let ccid = counterparty.connection_id().ok_or_else(|| {
        Error::undefined_connection_counterparty(channel_end.connection_hops()[0].clone())
    })?;

    let expected_connection_hops = vec![ccid.clone()];

    let expected_channel_end = ChannelEnd::new(
        State::Closed,
        *channel_end.ordering(),
        expected_counterparty,
        expected_connection_hops,
        channel_end.version(),
    );

    verify_channel_proofs(
        ctx,
        &channel_end,
        &conn,
        &expected_channel_end,
        msg.proofs(),
    )?;

    output.log("success: channel close confirm ");

    // Transition the channel end to the new state & pick a version.
    channel_end.set_state(State::Closed);

    let result = ChannelResult {
        port_id: msg.port_id().clone(),
        channel_id: msg.channel_id().clone(),
        channel_id_state: ChannelIdState::Reused,
        channel_cap,
        channel_end,
    };

    let event_attributes = Attributes {
        channel_id: Some(msg.channel_id().clone()),
        ..Default::default()
    };
    output.emit(IbcEvent::CloseConfirmChannel(event_attributes.into()));

    Ok(output.with_result(result))
}
