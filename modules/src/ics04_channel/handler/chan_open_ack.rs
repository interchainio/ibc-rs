//! Protocol logic specific to ICS4 messages of type `MsgChannelOpenAck`.
use crate::events::IbcEvent;
use crate::handler::{HandlerOutput, HandlerResult};
use crate::ics03_connection::connection::State as ConnectionState;
use crate::ics04_channel::channel::{ChannelEnd, Counterparty, State};
use crate::ics04_channel::context::ChannelReader;
use crate::ics04_channel::error::Error;
use crate::ics04_channel::events::Attributes;
use crate::ics04_channel::handler::verify::verify_channel_proofs;
use crate::ics04_channel::handler::{ChannelIdState, ChannelResult};
use crate::ics04_channel::msgs::chan_open_ack::MsgChannelOpenAck;

pub(crate) fn process(
    ctx: &dyn ChannelReader,
    msg: MsgChannelOpenAck,
) -> HandlerResult<ChannelResult, Error> {
    let mut output = HandlerOutput::builder();

    // Unwrap the old channel end and validate it against the message.
    let mut channel_end = ctx.channel_end(&(msg.port_id().clone(), msg.channel_id().clone()))?;

    // Validate that the channel end is in a state where it can be ack.
    if !channel_end.state_matches(&State::Init) && !channel_end.state_matches(&State::TryOpen) {
        return Err(Error::invalid_channel_state(
            msg.channel_id().clone(),
            channel_end.state,
        ));
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
        State::TryOpen,
        *channel_end.ordering(),
        expected_counterparty,
        expected_connection_hops,
        msg.counterparty_version().clone(),
    );
    //2. Verify proofs
    verify_channel_proofs(
        ctx,
        &channel_end,
        &conn,
        &expected_channel_end,
        msg.proofs(),
    )?;

    output.log("success: channel open ack ");

    // Transition the channel end to the new state & pick a version.
    channel_end.set_state(State::Open);
    channel_end.set_version(msg.counterparty_version().clone());
    channel_end.set_counterparty_channel_id(msg.counterparty_channel_id.clone());

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
    output.emit(IbcEvent::OpenAckChannel(event_attributes.into()));

    Ok(output.with_result(result))
}

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;
    use std::str::FromStr;
    use test_env_log::test;

    use crate::events::IbcEvent;
    use crate::ics03_connection::connection::ConnectionEnd;
    use crate::ics03_connection::connection::Counterparty as ConnectionCounterparty;
    use crate::ics03_connection::connection::State as ConnectionState;
    use crate::ics03_connection::msgs::conn_open_init::test_util::get_dummy_raw_msg_conn_open_init;
    use crate::ics03_connection::msgs::conn_open_init::MsgConnectionOpenInit;
    use crate::ics03_connection::msgs::conn_open_try::test_util::get_dummy_raw_msg_conn_open_try;
    use crate::ics03_connection::msgs::conn_open_try::MsgConnectionOpenTry;
    use crate::ics03_connection::version::get_compatible_versions;
    use crate::ics04_channel::channel::{ChannelEnd, Counterparty, State};
    use crate::ics04_channel::handler::{channel_dispatch, ChannelResult};
    use crate::ics04_channel::msgs::chan_open_ack::test_util::get_dummy_raw_msg_chan_open_ack;
    use crate::ics04_channel::msgs::chan_open_ack::MsgChannelOpenAck;
    use crate::ics04_channel::msgs::chan_open_try::test_util::get_dummy_raw_msg_chan_open_try;
    use crate::ics04_channel::msgs::chan_open_try::MsgChannelOpenTry;
    use crate::ics04_channel::msgs::ChannelMsg;
    use crate::ics24_host::identifier::ConnectionId;
    use crate::mock::context::MockContext;
    use crate::Height;

    // TODO: The tests here are very fragile and complex.
    //  Should be adapted to use the same structure as `handler::chan_open_try::tests`.
    #[test]
    fn chan_open_ack_msg_processing() {
        struct Test {
            name: String,
            ctx: MockContext,
            msg: ChannelMsg,
            want_pass: bool,
        }
        let proof_height = 10;
        let client_consensus_state_height = 10;
        let host_chain_height = Height::new(0, 35);

        let context = MockContext::default();

        let msg_conn_init =
            MsgConnectionOpenInit::try_from(get_dummy_raw_msg_conn_open_init()).unwrap();

        let conn_end = ConnectionEnd::new(
            ConnectionState::Open,
            msg_conn_init.client_id().clone(),
            ConnectionCounterparty::new(
                msg_conn_init.counterparty().client_id().clone(),
                Some(ConnectionId::from_str("defaultConnection-1").unwrap()),
                msg_conn_init.counterparty().prefix().clone(),
            ),
            get_compatible_versions(),
            msg_conn_init.delay_period,
        );

        let ccid = <ConnectionId as FromStr>::from_str("defaultConnection-0");
        let cid = match ccid {
            Ok(v) => v,
            Err(_e) => ConnectionId::default(),
        };

        let mut connection_vec0 = Vec::new();
        connection_vec0.insert(
            0,
            match <ConnectionId as FromStr>::from_str("defaultConnection-0") {
                Ok(a) => a,
                _ => unreachable!(),
            },
        );

        let msg_conn_try = MsgConnectionOpenTry::try_from(get_dummy_raw_msg_conn_open_try(
            client_consensus_state_height,
            host_chain_height.revision_height,
        ))
        .unwrap();

        let msg_chan_ack =
            MsgChannelOpenAck::try_from(get_dummy_raw_msg_chan_open_ack(proof_height)).unwrap();

        let msg_chan_try =
            MsgChannelOpenTry::try_from(get_dummy_raw_msg_chan_open_try(proof_height)).unwrap();

        let chan_end = ChannelEnd::new(
            State::Init,
            *msg_chan_try.channel.ordering(),
            Counterparty::new(
                msg_chan_ack.port_id().clone(),
                Some(msg_chan_ack.channel_id().clone()),
            ),
            connection_vec0.clone(),
            msg_chan_try.channel.version(),
        );

        let failed_chan_end = ChannelEnd::new(
            State::Open,
            *msg_chan_try.channel.ordering(),
            Counterparty::new(
                msg_chan_ack.port_id().clone(),
                Some(msg_chan_ack.channel_id().clone()),
            ),
            connection_vec0,
            msg_chan_try.channel.version(),
        );

        let tests: Vec<Test> = vec![
            Test {
                name: "Processing fails because no channel exists in the context".to_string(),
                ctx: context.clone(),
                msg: ChannelMsg::ChannelOpenAck(msg_chan_ack.clone()),
                want_pass: false,
            },
            Test {
                name: "Processing fails because the channel is in the wrong state".to_string(),
                ctx: context
                    .clone()
                    .with_client(
                        msg_conn_try.client_id(),
                        Height::new(0, client_consensus_state_height),
                    )
                    .with_port_capability(msg_chan_ack.port_id().clone())
                    .with_channel(
                        msg_chan_ack.port_id().clone(),
                        msg_chan_ack.channel_id().clone(),
                        failed_chan_end,
                    ),
                msg: ChannelMsg::ChannelOpenAck(msg_chan_ack.clone()),
                want_pass: false,
            },
            Test {
                name: "Processing fails because port does not have a capability associate"
                    .to_string(),
                ctx: context
                    .clone()
                    .with_client(
                        msg_conn_try.client_id(),
                        Height::new(0, client_consensus_state_height),
                    )
                    .with_connection(cid.clone(), conn_end.clone())
                    .with_channel(
                        msg_chan_ack.port_id().clone(),
                        msg_chan_ack.channel_id().clone(),
                        chan_end.clone(),
                    ),
                msg: ChannelMsg::ChannelOpenAck(msg_chan_ack.clone()),
                want_pass: false,
            },
            Test {
                name: "Processing fails because a connection does exist".to_string(),
                ctx: context
                    .clone()
                    .with_client(
                        msg_conn_try.client_id(),
                        Height::new(0, client_consensus_state_height),
                    )
                    .with_port_capability(msg_chan_ack.port_id().clone())
                    .with_channel(
                        msg_chan_ack.port_id().clone(),
                        msg_chan_ack.channel_id().clone(),
                        chan_end.clone(),
                    ),
                msg: ChannelMsg::ChannelOpenAck(msg_chan_ack.clone()),
                want_pass: false,
            },
            Test {
                name: "Processing fails due to missing client state ".to_string(),
                ctx: context
                    .clone()
                    .with_connection(cid.clone(), conn_end.clone())
                    .with_port_capability(msg_chan_ack.port_id().clone())
                    .with_channel(
                        msg_chan_ack.port_id().clone(),
                        msg_chan_ack.channel_id().clone(),
                        chan_end.clone(),
                    ),
                msg: ChannelMsg::ChannelOpenAck(msg_chan_ack.clone()),
                want_pass: false,
            },
            Test {
                name: "Good parameters".to_string(),
                ctx: context //  .clone()
                    .with_client(
                        msg_conn_try.client_id(),
                        Height::new(0, client_consensus_state_height),
                    )
                    .with_connection(cid, conn_end)
                    .with_port_capability(msg_chan_ack.port_id().clone())
                    .with_channel(
                        msg_chan_ack.port_id().clone(),
                        msg_chan_ack.channel_id().clone(),
                        chan_end,
                    ),
                msg: ChannelMsg::ChannelOpenAck(msg_chan_ack),
                want_pass: true,
            },
        ]
        .into_iter()
        .collect();

        for test in tests {
            let res = channel_dispatch(&test.ctx, test.msg.clone());
            // Additionally check the events and the output objects in the result.
            match res {
                Ok(proto_output) => {
                    assert!(
                            test.want_pass,
                            "chan_open_ack: test passed but was supposed to fail for test: {}, \nparams {:?} {:?}",
                            test.name,
                            test.msg.clone(),
                            test.ctx.clone()
                        );

                    assert!(!proto_output.events.is_empty()); // Some events must exist.

                    // The object in the output is a ConnectionEnd, should have init state.
                    let res: ChannelResult = proto_output.result;
                    //assert_eq!(res.channel_id, msg_chan_init.channel_id().clone());
                    assert_eq!(res.channel_end.state().clone(), State::Open);

                    for e in proto_output.events.iter() {
                        assert!(matches!(e, &IbcEvent::OpenAckChannel(_)));
                    }
                }
                Err(e) => {
                    assert!(
                        !test.want_pass,
                        "chan_open_ack: did not pass test: {}, \nparams {:?} {:?} error: {:?}",
                        test.name,
                        test.msg,
                        test.ctx.clone(),
                        e,
                    );
                }
            }
        }
    }
}
