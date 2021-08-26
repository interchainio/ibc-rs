use crate::events::IbcEvent;
use crate::handler::{HandlerOutput, HandlerResult};
use crate::ics04_channel::channel::State;
use crate::ics04_channel::channel::{ChannelEnd, Counterparty, Order};
use crate::ics04_channel::events::TimeoutPacket;
use crate::ics04_channel::handler::verify::{
    verify_next_sequence_recv, verify_packet_receipt_absence,
};
use crate::ics04_channel::msgs::timeout::MsgTimeout;
use crate::ics04_channel::packet::{PacketResult, Sequence};
use crate::ics04_channel::{context::ChannelReader, error::Error};
use crate::ics24_host::identifier::{ChannelId, PortId};
use crate::timestamp::Expiry;

#[derive(Clone, Debug)]
pub struct TimeoutPacketResult {
    pub port_id: PortId,
    pub channel_id: ChannelId,
    pub seq: Sequence,
    pub channel: Option<ChannelEnd>,
}

pub fn process(ctx: &dyn ChannelReader, msg: MsgTimeout) -> HandlerResult<PacketResult, Error> {
    let mut output = HandlerOutput::builder();

    let packet = &msg.packet;

    let mut source_channel_end =
        ctx.channel_end(&(packet.source_port.clone(), packet.source_channel.clone()))?;

    if !source_channel_end.state_matches(&State::Open) {
        return Err(Error::channel_closed(packet.source_channel.clone()));
    }

    let _channel_cap = ctx.authenticated_capability(&packet.source_port)?;

    let counterparty = Counterparty::new(
        packet.destination_port.clone(),
        Some(packet.destination_channel.clone()),
    );

    if !source_channel_end.counterparty_matches(&counterparty) {
        return Err(Error::invalid_packet_counterparty(
            packet.destination_port.clone(),
            packet.destination_channel.clone(),
        ));
    }

    let connection_end = ctx.connection_end(&source_channel_end.connection_hops()[0])?;

    let client_id = connection_end.client_id().clone();

    // check that timeout height or timeout timestamp has passed on the other end
    let proof_height = msg.proofs.height();
    let packet_height = packet.timeout_height;

    if (!packet.timeout_height.is_zero()) && packet_height > proof_height {
        return Err(Error::packet_timeout_height_not_reached(
            packet.timeout_height,
            proof_height,
        ));
    }

    let consensus_state = ctx.client_consensus_state(&client_id, proof_height)?;

    let proof_timestamp = consensus_state.timestamp();

    let packet_timestamp = packet.timeout_timestamp;
    if let Expiry::Expired = packet_timestamp.check_expiry(&proof_timestamp) {
        return Err(Error::packet_timeout_timestamp_not_reached(
            packet_timestamp,
            proof_timestamp,
        ));
    }

    //verify packet commitment
    let packet_commitment = ctx.get_packet_commitment(&(
        packet.source_port.clone(),
        packet.source_channel.clone(),
        packet.sequence,
    ))?;

    let input = format!(
        "{:?},{:?},{:?}",
        packet.timeout_timestamp, packet.timeout_height, packet.data,
    );

    if packet_commitment != ChannelReader::hash(ctx, input) {
        return Err(Error::incorrect_packet_commitment(packet.sequence));
    }

    let result = if source_channel_end.order_matches(&Order::Ordered) {
        if packet.sequence < msg.next_sequence_recv {
            return Err(Error::invalid_packet_sequence(
                packet.sequence,
                msg.next_sequence_recv,
            ));
        }
        verify_next_sequence_recv(
            ctx,
            client_id,
            packet.clone(),
            msg.next_sequence_recv,
            &msg.proofs.clone(),
        )?;

        source_channel_end.state = State::Closed;
        PacketResult::Timeout(TimeoutPacketResult {
            port_id: packet.source_port.clone(),
            channel_id: packet.source_channel.clone(),
            seq: packet.sequence,
            channel: Some(source_channel_end),
        })
    } else {
        verify_packet_receipt_absence(ctx, client_id, packet.clone(), &msg.proofs.clone())?;

        PacketResult::Timeout(TimeoutPacketResult {
            port_id: packet.source_port.clone(),
            channel_id: packet.source_channel.clone(),
            seq: packet.sequence,
            channel: None,
        })
    };

    output.log("success: packet timeout ");

    output.emit(IbcEvent::TimeoutPacket(TimeoutPacket {
        height: Default::default(),
        packet: packet.clone(),
    }));

    Ok(output.with_result(result))
}

#[cfg(test)]
mod tests {

    use crate::events::IbcEvent;
    use crate::ics02_client::height::Height;
    use crate::ics03_connection::connection::ConnectionEnd;
    use crate::ics03_connection::connection::Counterparty as ConnectionCounterparty;
    use crate::ics03_connection::connection::State as ConnectionState;
    use crate::ics03_connection::version::get_compatible_versions;
    use crate::ics04_channel::channel::{ChannelEnd, Counterparty, Order, State};
    use crate::ics04_channel::context::ChannelReader;
    use crate::ics04_channel::handler::timeout::process;
    use crate::ics04_channel::msgs::timeout::test_util::get_dummy_raw_msg_timeout;
    use crate::ics04_channel::msgs::timeout::MsgTimeout;
    use crate::ics24_host::identifier::{ChannelId, ClientId, ConnectionId, PortId};
    use crate::timestamp::ZERO_DURATION;

    use crate::mock::context::MockContext;

    use std::convert::TryFrom;
    use test_env_log::test;

    #[test]
    fn timeout_packet_processing() {
        struct Test {
            name: String,
            ctx: MockContext,
            msg: MsgTimeout,
            want_pass: bool,
        }

        let context = MockContext::default();

        let height = Height::default().revision_height + 2;
        let timeout_timestamp = 5;

        let client_height = Height::new(0, Height::default().revision_height + 2);

        let msg =
            MsgTimeout::try_from(get_dummy_raw_msg_timeout(height, timeout_timestamp)).unwrap();
        let packet = msg.packet.clone();

        let mut msg_ok = msg.clone();
        msg_ok.packet.timeout_timestamp = Default::default();

        let input = format!(
            "{:?},{:?},{:?}",
            msg_ok.packet.timeout_timestamp,
            msg_ok.packet.timeout_height.clone(),
            msg_ok.packet.data.clone()
        );
        let data = ChannelReader::hash(&context, input);

        let source_channel_end = ChannelEnd::new(
            State::Open,
            Order::default(),
            Counterparty::new(
                packet.destination_port.clone(),
                Some(packet.destination_channel.clone()),
            ),
            vec![ConnectionId::default()],
            "ics20".to_string(),
        );

        let mut source_ordered_channel_end = source_channel_end.clone();
        source_ordered_channel_end.ordering = Order::Ordered;

        let connection_end = ConnectionEnd::new(
            ConnectionState::Open,
            ClientId::default(),
            ConnectionCounterparty::new(
                ClientId::default(),
                Some(ConnectionId::default()),
                Default::default(),
            ),
            get_compatible_versions(),
            ZERO_DURATION,
        );

        let tests: Vec<Test> = vec![
            Test {
                name: "Processing fails because no channel exists in the context".to_string(),
                ctx: context.clone(),
                msg: msg.clone(),
                want_pass: false,
            },
            Test {
                name: "Processing fails because the client does not have a consensus state for the required height"
                    .to_string(),
                ctx: context.clone().with_channel(
                    PortId::default(),
                    ChannelId::default(),
                    source_channel_end.clone(),
                )
                .with_port_capability(packet.destination_port.clone())
                .with_connection(ConnectionId::default(), connection_end.clone()),
                msg: msg.clone(),
                want_pass: false,
            },
            Test {
                name: "Processing fails because the proof's timeout has not been reached "
                    .to_string(),
                ctx: context.clone().with_channel(
                    PortId::default(),
                    ChannelId::default(),
                    source_channel_end.clone(),
                )
                .with_client(&ClientId::default(), client_height)
                .with_port_capability(packet.destination_port.clone())
                .with_connection(ConnectionId::default(), connection_end.clone()),
                msg,
                want_pass: false,
            },
            Test {
                name: "Good parameters Unordered channel".to_string(),
                ctx: context.clone()
                    .with_client(&ClientId::default(), client_height)
                    .with_connection(ConnectionId::default(), connection_end.clone())
                    .with_port_capability(packet.destination_port.clone())
                    .with_channel(
                        packet.source_port.clone(),
                        packet.source_channel.clone(),
                        source_channel_end,
                    )
                    .with_packet_commitment(
                        msg_ok.packet.source_port.clone(),
                        msg_ok.packet.source_channel.clone(),
                        msg_ok.packet.sequence,
                        data.clone(),
                    ),
                msg: msg_ok.clone(),
                want_pass: true,
            },
            Test {
                name: "Good parameters Ordered Channel".to_string(),
                ctx: context
                    .with_client(&ClientId::default(), client_height)
                    .with_connection(ConnectionId::default(), connection_end)
                    .with_port_capability(packet.destination_port.clone())
                    .with_channel(
                        packet.source_port.clone(),
                        packet.source_channel,
                        source_ordered_channel_end,
                    )
                    .with_packet_commitment(
                        msg_ok.packet.source_port.clone(),
                        msg_ok.packet.source_channel.clone(),
                        msg_ok.packet.sequence,
                        data,
                    )
                    .with_ack_sequence(
                         packet.destination_port,
                         packet.destination_channel,
                         1.into(),
                     ),
                msg: msg_ok,
                want_pass: true,
            },
        ]
        .into_iter()
        .collect();

        for test in tests {
            let res = process(&test.ctx, test.msg.clone());
            // Additionally check the events and the output objects in the result.
            match res {
                Ok(proto_output) => {
                    assert!(
                        test.want_pass,
                        "TO_packet: test passed but was supposed to fail for test: {}, \nparams {:?} {:?}",
                        test.name,
                        test.msg.clone(),
                        test.ctx.clone()
                    );

                    assert!(!proto_output.events.is_empty()); // Some events must exist.

                    for e in proto_output.events.iter() {
                        assert!(matches!(e, &IbcEvent::TimeoutPacket(_)));
                    }
                }
                Err(e) => {
                    assert!(
                        !test.want_pass,
                        "timeout_packet: did not pass test: {}, \nparams {:?} {:?} error: {:?}",
                        test.name,
                        test.msg.clone(),
                        test.ctx.clone(),
                        e,
                    );
                }
            }
        }
    }
}
