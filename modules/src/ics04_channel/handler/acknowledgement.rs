use crate::events::IbcEvent;
use crate::handler::{HandlerOutput, HandlerResult};
use crate::ics02_client::height::Height;
use crate::ics03_connection::connection::State as ConnectionState;
use crate::ics04_channel::channel::State;
use crate::ics04_channel::channel::{Counterparty, Order};
use crate::ics04_channel::events::AcknowledgePacket;
use crate::ics04_channel::handler::verify::verify_packet_acknowledgement_proofs;
use crate::ics04_channel::msgs::acknowledgement::MsgAcknowledgement;
use crate::ics04_channel::packet::{PacketResult, Sequence};
use crate::ics04_channel::{context::ChannelReader, error::Error};
use crate::ics24_host::identifier::{ChannelId, PortId};

#[derive(Clone, Debug)]
pub struct AckPacketResult {
    pub port_id: PortId,
    pub channel_id: ChannelId,
    pub seq: Sequence,
    pub seq_number: Option<Sequence>,
}

pub fn process(
    ctx: &dyn ChannelReader,
    msg: MsgAcknowledgement,
) -> HandlerResult<PacketResult, Error> {
    let mut output = HandlerOutput::builder();

    let packet = &msg.packet;

    let source_channel_end =
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

    if !connection_end.state_matches(&ConnectionState::Open) {
        return Err(Error::connection_not_open(
            source_channel_end.connection_hops()[0].clone(),
        ));
    }

    let client_id = connection_end.client_id().clone();

    // Verify packet commitment
    let packet_commitment = ctx.get_packet_commitment(&(
        packet.source_port.clone(),
        packet.source_channel.clone(),
        packet.sequence,
    ))?;

    let input = format!(
        "{:?},{:?},{:?}",
        packet.timeout_timestamp, packet.timeout_height, packet.data,
    );

    if packet_commitment != ctx.hash(input) {
        return Err(Error::incorrect_packet_commitment(packet.sequence));
    }

    // Verify the acknowledgement proof
    verify_packet_acknowledgement_proofs(
        ctx,
        packet,
        msg.acknowledgement().clone(),
        client_id,
        msg.proofs(),
    )?;

    let result = if source_channel_end.order_matches(&Order::Ordered) {
        let next_seq_ack = ctx
            .get_next_sequence_ack(&(packet.source_port.clone(), packet.source_channel.clone()))?;

        if packet.sequence != next_seq_ack {
            return Err(Error::invalid_packet_sequence(
                packet.sequence,
                next_seq_ack,
            ));
        }

        PacketResult::Ack(AckPacketResult {
            port_id: packet.source_port.clone(),
            channel_id: packet.source_channel.clone(),
            seq: packet.sequence,
            seq_number: Some(next_seq_ack.increment()),
        })
    } else {
        PacketResult::Ack(AckPacketResult {
            port_id: packet.source_port.clone(),
            channel_id: packet.source_channel.clone(),
            seq: packet.sequence,
            seq_number: None,
        })
    };

    output.log("success: packet ack");

    output.emit(IbcEvent::AcknowledgePacket(AcknowledgePacket {
        height: Height::zero(),
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
    use crate::ics04_channel::handler::acknowledgement::process;
    use crate::ics04_channel::msgs::acknowledgement::test_util::get_dummy_raw_msg_acknowledgement;
    use crate::ics04_channel::msgs::acknowledgement::MsgAcknowledgement;
    use crate::ics24_host::identifier::{ChannelId, ClientId, ConnectionId, PortId};
    use crate::mock::context::MockContext;
    use crate::timestamp::ZERO_DURATION;
    use test_env_log::test;

    use std::convert::TryFrom;

    #[test]
    fn ack_packet_processing() {
        struct Test {
            name: String,
            ctx: MockContext,
            msg: MsgAcknowledgement,
            want_pass: bool,
        }

        let context = MockContext::default();

        let client_height = Height::new(0, Height::default().revision_height + 2);

        let msg = MsgAcknowledgement::try_from(get_dummy_raw_msg_acknowledgement(
            client_height.revision_height,
        ))
        .unwrap();
        let packet = msg.packet.clone();

        let input = format!(
            "{:?},{:?},{:?}",
            packet.timeout_timestamp,
            packet.timeout_height.clone(),
            packet.data.clone()
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
                name: "Processing fails because the port does not have a capability associated"
                    .to_string(),
                ctx: context.clone().with_channel(
                    PortId::default(),
                    ChannelId::default(),
                    source_channel_end.clone(),
                ),
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
                        packet.source_channel.clone(),
                        source_channel_end,
                    )
                    .with_packet_commitment(
                        packet.source_port,
                        packet.source_channel,
                        packet.sequence,
                        data,
                    ) //with_ack_sequence required for ordered channels
                    .with_ack_sequence(
                        packet.destination_port,
                        packet.destination_channel,
                        1.into(),
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
                        "ack_packet: test passed but was supposed to fail for test: {}, \nparams {:?} {:?}",
                        test.name,
                        test.msg.clone(),
                        test.ctx.clone()
                    );

                    assert!(!proto_output.events.is_empty()); // Some events must exist.

                    for e in proto_output.events.iter() {
                        assert!(matches!(e, &IbcEvent::AcknowledgePacket(_)));
                    }
                }
                Err(e) => {
                    assert!(
                        !test.want_pass,
                        "ack_packet: did not pass test: {}, \nparams {:?} {:?} error: {:?}",
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
