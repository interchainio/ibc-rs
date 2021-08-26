use crate::events::IbcEvent;
use crate::handler::{HandlerOutput, HandlerResult};
use crate::ics04_channel::channel::State;
use crate::ics04_channel::channel::{ChannelEnd, Counterparty, Order};
use crate::ics04_channel::events::TimeoutOnClosePacket;
use crate::ics04_channel::handler::verify::verify_channel_proofs;
use crate::ics04_channel::handler::verify::{
    verify_next_sequence_recv, verify_packet_receipt_absence,
};
use crate::ics04_channel::msgs::timeout_on_close::MsgTimeoutOnClose;
use crate::ics04_channel::packet::PacketResult;
use crate::ics04_channel::{
    context::ChannelReader, error::Error, handler::timeout::TimeoutPacketResult,
};

pub fn process(
    ctx: &dyn ChannelReader,
    msg: MsgTimeoutOnClose,
) -> HandlerResult<PacketResult, Error> {
    let mut output = HandlerOutput::builder();

    let packet = &msg.packet;

    let source_channel_end =
        ctx.channel_end(&(packet.source_port.clone(), packet.source_channel.clone()))?;

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

    //verify the packet was sent, check the store
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

    let expected_counterparty = Counterparty::new(
        packet.source_port.clone(),
        Some(packet.source_channel.clone()),
    );

    let counterparty = connection_end.counterparty();
    let ccid = counterparty.connection_id().ok_or_else(|| {
        Error::undefined_connection_counterparty(source_channel_end.connection_hops()[0].clone())
    })?;

    let expected_connection_hops = vec![ccid.clone()];

    let expected_channel_end = ChannelEnd::new(
        State::Closed,
        *source_channel_end.ordering(),
        expected_counterparty,
        expected_connection_hops,
        source_channel_end.version(),
    );

    verify_channel_proofs(
        ctx,
        &source_channel_end,
        &connection_end,
        &expected_channel_end,
        &msg.proofs.clone(),
    )?;

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

    output.emit(IbcEvent::TimeoutOnClosePacket(TimeoutOnClosePacket {
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
    use crate::ics04_channel::handler::timeout_on_close::process;
    use crate::ics04_channel::msgs::timeout_on_close::test_util::get_dummy_raw_msg_timeout_on_close;
    use crate::ics04_channel::msgs::timeout_on_close::MsgTimeoutOnClose;
    use crate::ics24_host::identifier::{ChannelId, ClientId, ConnectionId, PortId};
    use crate::timestamp::ZERO_DURATION;

    use crate::mock::context::MockContext;

    use std::convert::TryFrom;
    use test_env_log::test;

    #[test]
    fn timeout_on_close_packet_processing() {
        struct Test {
            name: String,
            ctx: MockContext,
            msg: MsgTimeoutOnClose,
            want_pass: bool,
        }

        let context = MockContext::default();

        let height = Height::default().revision_height + 2;
        let timeout_timestamp = 5;

        let client_height = Height::new(0, Height::default().revision_height + 2);

        let msg = MsgTimeoutOnClose::try_from(get_dummy_raw_msg_timeout_on_close(
            height,
            timeout_timestamp,
        ))
        .unwrap();
        let packet = msg.packet.clone();

        let input = format!(
            "{:?},{:?},{:?}",
            msg.packet.timeout_timestamp,
            msg.packet.timeout_height.clone(),
            msg.packet.data.clone()
        );
        let data = ChannelReader::hash(&context, input);

        let source_channel_end = ChannelEnd::new(
            State::Open,
            Order::Ordered,
            Counterparty::new(
                packet.destination_port.clone(),
                Some(packet.destination_channel.clone()),
            ),
            vec![ConnectionId::default()],
            "ics20".to_string(),
        );

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
                name: "Processing fails no packet commitment is found".to_string(),
                ctx: context
                    .clone()
                    .with_channel(
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
                name: "Good parameters".to_string(),
                ctx: context
                    .with_client(&ClientId::default(), client_height)
                    .with_connection(ConnectionId::default(), connection_end)
                    .with_port_capability(packet.destination_port.clone())
                    .with_channel(
                        packet.source_port.clone(),
                        packet.source_channel,
                        source_channel_end,
                    )
                    .with_packet_commitment(
                        msg.packet.source_port.clone(),
                        msg.packet.source_channel.clone(),
                        msg.packet.sequence,
                        data,
                    ),
                msg,
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
                        "TO_on_close_packet: test passed but was supposed to fail for test: {}, \nparams {:?} {:?}",
                        test.name,
                        test.msg.clone(),
                        test.ctx.clone()
                    );

                    assert!(!proto_output.events.is_empty()); // Some events must exist.
                    for e in proto_output.events.iter() {
                        assert!(matches!(e, &IbcEvent::TimeoutOnClosePacket(_)));
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
