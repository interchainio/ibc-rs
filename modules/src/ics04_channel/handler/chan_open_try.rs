//! Protocol logic specific to ICS4 messages of type `MsgChannelOpenTry`.

use crate::events::IbcEvent;
use crate::handler::{HandlerOutput, HandlerResult};
use crate::ics03_connection::connection::State as ConnectionState;
use crate::ics04_channel::channel::{ChannelEnd, Counterparty, State};
use crate::ics04_channel::context::ChannelReader;
use crate::ics04_channel::error::Error;
use crate::ics04_channel::events::Attributes;
use crate::ics04_channel::handler::verify::verify_channel_proofs;
use crate::ics04_channel::handler::{ChannelIdState, ChannelResult};
use crate::ics04_channel::msgs::chan_open_try::MsgChannelOpenTry;
use crate::ics24_host::identifier::ChannelId;

pub(crate) fn process(
    ctx: &dyn ChannelReader,
    msg: MsgChannelOpenTry,
) -> HandlerResult<ChannelResult, Error> {
    let mut output = HandlerOutput::builder();

    // Unwrap the old channel end (if any) and validate it against the message.
    let (mut new_channel_end, channel_id) = match msg.previous_channel_id() {
        Some(prev_id) => {
            let old_channel_end = ctx.channel_end(&(msg.port_id().clone(), prev_id.clone()))?;

            // Validate that existing channel end matches with the one we're trying to establish.
            if old_channel_end.state_matches(&State::Init)
                && old_channel_end.order_matches(msg.channel.ordering())
                && old_channel_end.connection_hops_matches(msg.channel.connection_hops())
                && old_channel_end.counterparty_matches(msg.channel.counterparty())
                && old_channel_end.version_matches(&msg.channel.version())
            {
                // A ChannelEnd already exists and all validation passed.
                Ok((old_channel_end, prev_id.clone()))
            } else {
                // A ConnectionEnd already exists and validation failed.
                Err(Error::channel_mismatch(prev_id.clone()))
            }
        }
        // No previous channel id was supplied. Create a new channel end & an identifier.
        None => {
            let channel_end = ChannelEnd::new(
                State::Init,
                *msg.channel.ordering(),
                msg.channel.counterparty().clone(),
                msg.channel.connection_hops().clone(),
                msg.counterparty_version().clone(),
            );

            // Channel identifier construction.
            let id_counter = ctx.channel_counter()?;
            let chan_id = ChannelId::new(id_counter);

            output.log(format!(
                "success: generated new channel identifier: {}",
                chan_id
            ));

            Ok((channel_end, chan_id))
        }
    }?;

    // An IBC connection running on the local (host) chain should exist.
    if msg.channel.connection_hops().len() != 1 {
        return Err(Error::invalid_connection_hops_length(
            1,
            msg.channel.connection_hops().len(),
        ));
    }

    let conn = ctx.connection_end(&msg.channel().connection_hops()[0])?;
    if !conn.state_matches(&ConnectionState::Open) {
        return Err(Error::connection_not_open(
            msg.channel.connection_hops()[0].clone(),
        ));
    }

    let get_versions = conn.versions();
    let version = match get_versions.as_slice() {
        [version] => version,
        _ => return Err(Error::invalid_version_length_connection()),
    };

    let channel_feature = msg.channel().ordering().to_string();
    if !version.is_supported_feature(channel_feature) {
        return Err(Error::channel_feature_not_suported_by_connection());
    }

    // Channel capabilities
    let channel_cap = ctx.authenticated_capability(&msg.port_id().clone())?;

    if msg.channel().version().is_empty() {
        return Err(Error::empty_version());
    }

    // Proof verification in two steps:
    // 1. Setup: build the Channel as we expect to find it on the other party.
    //      the port should be identical with the port we're using; the channel id should not be set
    //      since the counterparty cannot know yet which ID did we choose.
    let expected_counterparty = Counterparty::new(msg.port_id().clone(), None);
    let counterparty = conn.counterparty();
    let ccid = counterparty.connection_id().ok_or_else(|| {
        Error::undefined_connection_counterparty(msg.channel().connection_hops()[0].clone())
    })?;
    let expected_connection_hops = vec![ccid.clone()];

    // The other party should be storing a channel end in this configuration.
    let expected_channel_end = ChannelEnd::new(
        State::Init,
        *msg.channel.ordering(),
        expected_counterparty,
        expected_connection_hops,
        msg.counterparty_version().clone(),
    );

    // 2. Actual proofs are verified now.
    verify_channel_proofs(
        ctx,
        &new_channel_end,
        &conn,
        &expected_channel_end,
        msg.proofs(),
    )?;

    output.log("success: channel open try ");

    // Transition the channel end to the new state & pick a version.
    new_channel_end.set_state(State::TryOpen);

    let result = ChannelResult {
        port_id: msg.port_id().clone(),
        channel_cap,
        channel_id_state: if matches!(msg.previous_channel_id, None) {
            ChannelIdState::Generated
        } else {
            ChannelIdState::Reused
        },
        channel_id: channel_id.clone(),
        channel_end: new_channel_end,
    };

    let event_attributes = Attributes {
        channel_id: Some(channel_id),
        ..Default::default()
    };
    output.emit(IbcEvent::OpenTryChannel(event_attributes.into()));

    Ok(output.with_result(result))
}

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;
    use test_env_log::test;

    use crate::events::IbcEvent;
    use crate::ics02_client::client_type::ClientType;
    use crate::ics03_connection::connection::ConnectionEnd;
    use crate::ics03_connection::connection::Counterparty as ConnectionCounterparty;
    use crate::ics03_connection::connection::State as ConnectionState;
    use crate::ics03_connection::msgs::test_util::get_dummy_raw_counterparty;
    use crate::ics03_connection::version::get_compatible_versions;
    use crate::ics04_channel::channel::{ChannelEnd, State};
    use crate::ics04_channel::error;
    use crate::ics04_channel::handler::{channel_dispatch, ChannelResult};
    use crate::ics04_channel::msgs::chan_open_try::test_util::get_dummy_raw_msg_chan_open_try;
    use crate::ics04_channel::msgs::chan_open_try::MsgChannelOpenTry;
    use crate::ics04_channel::msgs::ChannelMsg;
    use crate::ics24_host::identifier::{ChannelId, ClientId, ConnectionId};
    use crate::mock::context::MockContext;
    use crate::timestamp::ZERO_DURATION;
    use crate::Height;

    #[test]
    fn chan_open_try_msg_processing() {
        struct Test {
            name: String,
            ctx: MockContext,
            msg: ChannelMsg,
            want_pass: bool,
            match_error: Box<dyn FnOnce(error::ErrorDetail)>,
        }

        // Some general-purpose variable to parametrize the messages and the context.
        let proof_height = 10;
        let conn_id = ConnectionId::new(2);
        let client_id = ClientId::new(ClientType::Mock, 45).unwrap();

        // The context. We'll reuse this same one across all tests.
        let context = MockContext::default();

        // This is the connection underlying the channel we're trying to open.
        let conn_end = ConnectionEnd::new(
            ConnectionState::Open,
            client_id.clone(),
            ConnectionCounterparty::try_from(get_dummy_raw_counterparty()).unwrap(),
            get_compatible_versions(),
            ZERO_DURATION,
        );

        // We're going to test message processing against this message.
        let mut msg =
            MsgChannelOpenTry::try_from(get_dummy_raw_msg_chan_open_try(proof_height)).unwrap();

        // Assumption: an already existing `Init` channel should exist in the context for `msg`, and
        // this channel should depend on connection `conn_id`.
        let chan_id = ChannelId::new(24);
        let hops = vec![conn_id.clone()];
        msg.previous_channel_id = Some(chan_id.clone());
        msg.channel.connection_hops = hops;

        // This message does not assume a channel should already be initialized.
        let mut msg_vanilla = msg.clone();
        msg_vanilla.previous_channel_id = None;

        // A preloaded channel end that resides in the context. This is constructed so as to be
        // consistent with the incoming ChanOpenTry message `msg`.
        let correct_chan_end = ChannelEnd::new(
            State::Init,
            *msg.channel.ordering(),
            msg.channel.counterparty().clone(),
            msg.channel.connection_hops().clone(),
            msg.channel.version(),
        );

        // A preloaded channel end that resides in the context. This is constructed so as to be
        // __inconsistent__ with the incoming ChanOpenTry message `msg` due to its version field.
        let version = format!("{}-", msg.channel.version());
        let incorrect_chan_end_ver = ChannelEnd::new(
            State::Init,
            *msg.channel.ordering(),
            msg.channel.counterparty().clone(),
            msg.channel.connection_hops().clone(),
            version,
        );

        // A preloaded channel end residing in the context, which will be __inconsistent__ with
        // the incoming ChanOpenTry message `msg` due to its connection hops field.
        let hops = vec![ConnectionId::new(9890)];
        let incorrect_chan_end_hops = ChannelEnd::new(
            State::Init,
            *msg.channel.ordering(),
            msg.channel.counterparty().clone(),
            hops,
            msg.channel.version(),
        );

        let tests: Vec<Test> = vec![
            Test {
                name: "Processing fails because no channel is preloaded in the context".to_string(),
                ctx: context.clone(),
                msg: ChannelMsg::ChannelOpenTry(msg.clone()),
                want_pass: false,
                match_error: {
                    let port_id = msg.port_id.clone();
                    let channel_id = chan_id.clone();
                    Box::new(move |e| match e {
                        error::ErrorDetail::ChannelNotFound(e) => {
                            assert_eq!(e.port_id, port_id);
                            assert_eq!(e.channel_id, channel_id);
                        }
                        _ => {
                            panic!("Expected ChannelNotFound, instead got {}", e)
                        }
                    })
                },
            },
            Test {
                name: "Processing fails because no connection exists in the context".to_string(),
                ctx: context.clone(),
                msg: ChannelMsg::ChannelOpenTry(msg_vanilla.clone()),
                want_pass: false,
                match_error: {
                    let connection_id = msg.channel().connection_hops()[0].clone();
                    Box::new(move |e| match e {
                        error::ErrorDetail::MissingConnection(e) => {
                            assert_eq!(e.connection_id, connection_id);
                        }
                        _ => {
                            panic!("Expected MissingConnection, instead got {}", e)
                        }
                    })
                },
            },
            Test {
                name: "Processing fails because the port does not have a capability associated"
                    .to_string(),
                ctx: context
                    .clone()
                    .with_connection(conn_id.clone(), conn_end.clone()),
                msg: ChannelMsg::ChannelOpenTry(msg_vanilla.clone()),
                want_pass: false,
                match_error: {
                    let port_id = msg.port_id.clone();
                    Box::new(move |e| match e {
                        error::ErrorDetail::NoPortCapability(e) => {
                            assert_eq!(e.port_id, port_id);
                        }
                        _ => {
                            panic!("Expected NoPortCapability, instead got {}", e)
                        }
                    })
                },
            },
            Test {
                name: "Processing fails because of inconsistent version with preexisting channel"
                    .to_string(),
                ctx: context
                    .clone()
                    .with_connection(conn_id.clone(), conn_end.clone())
                    .with_port_capability(msg.port_id.clone())
                    .with_channel(msg.port_id.clone(), chan_id.clone(), incorrect_chan_end_ver),
                msg: ChannelMsg::ChannelOpenTry(msg.clone()),
                want_pass: false,
                match_error: {
                    let channel_id = chan_id.clone();
                    Box::new(move |e| match e {
                        error::ErrorDetail::ChannelMismatch(e) => {
                            assert_eq!(e.channel_id, channel_id);
                        }
                        _ => {
                            panic!("Expected ChannelMismatch, instead got {}", e)
                        }
                    })
                },
            },
            Test {
                name: "Processing fails because of inconsistent connection hops".to_string(),
                ctx: context
                    .clone()
                    .with_connection(conn_id.clone(), conn_end.clone())
                    .with_port_capability(msg.port_id.clone())
                    .with_channel(
                        msg.port_id.clone(),
                        chan_id.clone(),
                        incorrect_chan_end_hops,
                    ),
                msg: ChannelMsg::ChannelOpenTry(msg.clone()),
                want_pass: false,
                match_error: {
                    let channel_id = chan_id.clone();
                    Box::new(move |e| match e {
                        error::ErrorDetail::ChannelMismatch(e) => {
                            assert_eq!(e.channel_id, channel_id);
                        }
                        _ => {
                            panic!("Expected ChannelMismatch, instead got {}", e)
                        }
                    })
                },
            },
            Test {
                name: "Processing fails b/c the context has no client state".to_string(),
                ctx: context
                    .clone()
                    .with_connection(conn_id.clone(), conn_end.clone())
                    .with_port_capability(msg.port_id.clone())
                    .with_channel(
                        msg.port_id.clone(),
                        chan_id.clone(),
                        correct_chan_end.clone(),
                    ),
                msg: ChannelMsg::ChannelOpenTry(msg.clone()),
                want_pass: false,
                match_error: Box::new(|e| match e {
                    error::ErrorDetail::MissingClientState(_) => {}
                    _ => {
                        panic!("Expected MissingClientState, instead got {}", e)
                    }
                }),
            },
            Test {
                name: "Processing is successful".to_string(),
                ctx: context
                    .clone()
                    .with_client(&client_id, Height::new(0, proof_height))
                    .with_connection(conn_id.clone(), conn_end.clone())
                    .with_port_capability(msg.port_id.clone())
                    .with_channel(msg.port_id.clone(), chan_id, correct_chan_end),
                msg: ChannelMsg::ChannelOpenTry(msg.clone()),
                want_pass: true,
                match_error: Box::new(|_| {}),
            },
            Test {
                name: "Processing is successful against an empty context (no preexisting channel)"
                    .to_string(),
                ctx: context
                    .with_client(&client_id, Height::new(0, proof_height))
                    .with_connection(conn_id, conn_end)
                    .with_port_capability(msg.port_id),
                msg: ChannelMsg::ChannelOpenTry(msg_vanilla),
                want_pass: true,
                match_error: Box::new(|_| {}),
            },
        ]
        .into_iter()
        .collect();

        for test in tests {
            let res = channel_dispatch(&test.ctx, test.msg.clone());
            // Additionally check the events and the output objects in the result.
            match res {
                Ok(handler_output) => {
                    assert!(
                        test.want_pass,
                        "chan_open_ack: test passed but was supposed to fail for test: {}, \nparams {:?} {:?}",
                        test.name,
                        test.msg.clone(),
                        test.ctx.clone()
                    );

                    assert!(!handler_output.events.is_empty()); // Some events must exist.

                    // The object in the output is a channel end, should have TryOpen state.
                    let res: ChannelResult = handler_output.result;
                    assert_eq!(res.channel_end.state().clone(), State::TryOpen);

                    for e in handler_output.events.iter() {
                        assert!(matches!(e, &IbcEvent::OpenTryChannel(_)));
                    }
                }
                Err(e) => {
                    assert!(
                        !test.want_pass,
                        "chan_open_try: did not pass test: {}, \nparams:\n\tmsg={:?}\n\tcontext={:?}\nerror: {:?}",
                        test.name,
                        test.msg,
                        test.ctx.clone(),
                        e,
                    );

                    (test.match_error)(e.0);
                }
            }
        }
    }
}
