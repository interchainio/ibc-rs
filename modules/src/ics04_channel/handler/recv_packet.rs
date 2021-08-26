use crate::events::IbcEvent;
use crate::handler::{HandlerOutput, HandlerResult};
use crate::ics02_client::height::Height;
use crate::ics03_connection::connection::State as ConnectionState;
use crate::ics04_channel::channel::{Counterparty, Order, State};
use crate::ics04_channel::context::ChannelReader;
use crate::ics04_channel::error::Error;
use crate::ics04_channel::events::ReceivePacket;
use crate::ics04_channel::handler::verify::verify_packet_recv_proofs;
use crate::ics04_channel::msgs::recv_packet::MsgRecvPacket;
use crate::ics04_channel::packet::{PacketResult, Receipt, Sequence};
use crate::ics24_host::identifier::{ChannelId, PortId};
use crate::timestamp::Expiry;

#[derive(Clone, Debug)]
pub struct RecvPacketResult {
    pub port_id: PortId,
    pub channel_id: ChannelId,
    pub seq: Sequence,
    pub seq_number: Sequence,
    pub receipt: Option<Receipt>,
}

pub fn process(ctx: &dyn ChannelReader, msg: MsgRecvPacket) -> HandlerResult<PacketResult, Error> {
    let mut output = HandlerOutput::builder();

    let packet = &msg.packet;

    let dest_channel_end = ctx.channel_end(&(
        packet.destination_port.clone(),
        packet.destination_channel.clone(),
    ))?;

    if !dest_channel_end.state_matches(&State::Open) {
        return Err(Error::invalid_channel_state(
            packet.source_channel.clone(),
            dest_channel_end.state,
        ));
    }

    let _channel_cap = ctx.authenticated_capability(&packet.destination_port)?;

    let counterparty = Counterparty::new(
        packet.source_port.clone(),
        Some(packet.source_channel.clone()),
    );

    if !dest_channel_end.counterparty_matches(&counterparty) {
        return Err(Error::invalid_packet_counterparty(
            packet.source_port.clone(),
            packet.source_channel.clone(),
        ));
    }

    let connection_end = ctx.connection_end(&dest_channel_end.connection_hops()[0])?;

    if !connection_end.state_matches(&ConnectionState::Open) {
        return Err(Error::connection_not_open(
            dest_channel_end.connection_hops()[0].clone(),
        ));
    }

    let client_id = connection_end.client_id().clone();

    // Check if packet height is newer than the height of the local host chain
    let latest_height = ctx.host_height();
    if (!packet.timeout_height.is_zero()) && (packet.timeout_height <= latest_height) {
        return Err(Error::low_packet_height(
            latest_height,
            packet.timeout_height,
        ));
    }

    // Check if packet timestamp is newer than the local host chain timestamp
    let latest_timestamp = ctx.host_timestamp();
    if let Expiry::Expired = latest_timestamp.check_expiry(&packet.timeout_timestamp) {
        return Err(Error::low_packet_timestamp());
    }

    verify_packet_recv_proofs(ctx, packet, client_id, &msg.proofs)?;

    let result = if dest_channel_end.order_matches(&Order::Ordered) {
        let next_seq_recv = ctx
            .get_next_sequence_recv(&(packet.source_port.clone(), packet.source_channel.clone()))?;

        if packet.sequence != next_seq_recv {
            return Err(Error::invalid_packet_sequence(
                packet.sequence,
                next_seq_recv,
            ));
        }

        PacketResult::Recv(RecvPacketResult {
            port_id: packet.source_port.clone(),
            channel_id: packet.source_channel.clone(),
            seq: packet.sequence,
            seq_number: next_seq_recv.increment(),
            receipt: None,
        })
    } else {
        let packet_rec = ctx.get_packet_receipt(&(
            packet.source_port.clone(),
            packet.source_channel.clone(),
            packet.sequence,
        ));

        match packet_rec {
            Ok(_receipt) => return Err(Error::packet_already_received(packet.sequence)),
            Err(e) if e.detail() == Error::packet_receipt_not_found(packet.sequence).detail() => {
                // store a receipt that does not contain any data
                PacketResult::Recv(RecvPacketResult {
                    port_id: packet.source_port.clone(),
                    channel_id: packet.source_channel.clone(),
                    seq: packet.sequence,
                    seq_number: 1.into(),
                    receipt: Some(Receipt::Ok),
                })
            }
            Err(_) => return Err(Error::implementation_specific()),
        }
    };

    output.log("success: packet receive");

    output.emit(IbcEvent::ReceivePacket(ReceivePacket {
        height: Height::zero(),
        packet: msg.packet,
    }));

    Ok(output.with_result(result))
}

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;
    use test_env_log::test;

    use crate::ics03_connection::connection::ConnectionEnd;
    use crate::ics03_connection::connection::Counterparty as ConnectionCounterparty;
    use crate::ics03_connection::connection::State as ConnectionState;
    use crate::ics03_connection::version::get_compatible_versions;
    use crate::ics04_channel::channel::{ChannelEnd, Counterparty, Order, State};
    use crate::ics04_channel::handler::recv_packet::process;
    use crate::ics04_channel::msgs::recv_packet::test_util::get_dummy_raw_msg_recv_packet;
    use crate::ics04_channel::msgs::recv_packet::MsgRecvPacket;
    use crate::ics18_relayer::context::Ics18Context;
    use crate::ics24_host::identifier::{ChannelId, ClientId, ConnectionId, PortId};
    use crate::mock::context::MockContext;
    use crate::test_utils::get_dummy_account_id;
    use crate::timestamp::Timestamp;
    use crate::timestamp::ZERO_DURATION;
    use crate::{events::IbcEvent, ics04_channel::packet::Packet};

    #[test]
    fn recv_packet_processing() {
        struct Test {
            name: String,
            ctx: MockContext,
            msg: MsgRecvPacket,
            want_pass: bool,
        }

        let context = MockContext::default();

        let host_height = context.query_latest_height().increment();

        let client_height = host_height.increment();

        let msg =
            MsgRecvPacket::try_from(get_dummy_raw_msg_recv_packet(client_height.revision_height))
                .unwrap();

        let packet = msg.packet.clone();

        let packet_old = Packet {
            sequence: 1.into(),
            source_port: PortId::default(),
            source_channel: ChannelId::default(),
            destination_port: PortId::default(),
            destination_channel: ChannelId::default(),
            data: vec![],
            timeout_height: client_height,
            timeout_timestamp: Timestamp::from_nanoseconds(1).unwrap(),
        };

        let msg_packet_old =
            MsgRecvPacket::new(packet_old, msg.proofs.clone(), get_dummy_account_id());

        let dest_channel_end = ChannelEnd::new(
            State::Open,
            Order::default(),
            Counterparty::new(
                packet.source_port.clone(),
                Some(packet.source_channel.clone()),
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
                    dest_channel_end.clone(),
                ),
                msg: msg.clone(),
                want_pass: false,
            },
            Test {
                name: "Good parameters".to_string(),
                ctx: context
                    .clone()
                    .with_client(&ClientId::default(), client_height)
                    .with_connection(ConnectionId::default(), connection_end.clone())
                    .with_port_capability(packet.destination_port.clone())
                    .with_channel(
                        packet.destination_port.clone(),
                        packet.destination_channel.clone(),
                        dest_channel_end.clone(),
                    )
                    .with_send_sequence(
                        packet.destination_port.clone(),
                        packet.destination_channel.clone(),
                        1.into(),
                    )
                    .with_height(host_height)
                    .with_timestamp(Timestamp::from_nanoseconds(1).unwrap())
                    // This `with_recv_sequence` is required for ordered channels
                    .with_recv_sequence(
                        packet.destination_port.clone(),
                        packet.destination_channel,
                        packet.sequence,
                    ),
                msg,
                want_pass: true,
            },
            Test {
                name: "Packet timeout expired".to_string(),
                ctx: context
                    .with_client(&ClientId::default(), client_height)
                    .with_connection(ConnectionId::default(), connection_end)
                    .with_port_capability(PortId::default())
                    .with_channel(PortId::default(), ChannelId::default(), dest_channel_end)
                    .with_send_sequence(PortId::default(), ChannelId::default(), 1.into())
                    .with_height(host_height)
                    .with_timestamp(Timestamp::from_nanoseconds(3).unwrap()),
                msg: msg_packet_old,
                want_pass: false,
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
                            "recv_packet: test passed but was supposed to fail for test: {}, \nparams \n msg={:?}\nctx:{:?}",
                            test.name,
                            test.msg.clone(),
                            test.ctx.clone()
                        );

                    assert!(!proto_output.events.is_empty()); // Some events must exist.

                    for e in proto_output.events.iter() {
                        assert!(matches!(e, &IbcEvent::ReceivePacket(_)));
                    }
                }
                Err(e) => {
                    assert!(
                            !test.want_pass,
                            "recv_packet: did not pass test: {}, \nparams \nmsg={:?}\nctx={:?}\nerror={:?}",
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
