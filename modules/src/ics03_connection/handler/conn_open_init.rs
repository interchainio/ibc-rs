//! Protocol logic specific to ICS3 messages of type `MsgConnectionOpenInit`.

use crate::events::IbcEvent;
use crate::handler::{HandlerOutput, HandlerResult};
use crate::ics03_connection::connection::{ConnectionEnd, State};
use crate::ics03_connection::context::ConnectionReader;
use crate::ics03_connection::error::Error;
use crate::ics03_connection::events::Attributes;
use crate::ics03_connection::handler::{ConnectionIdState, ConnectionResult};
use crate::ics03_connection::msgs::conn_open_init::MsgConnectionOpenInit;
use crate::ics24_host::identifier::ConnectionId;

pub(crate) fn process(
    ctx: &dyn ConnectionReader,
    msg: MsgConnectionOpenInit,
) -> HandlerResult<ConnectionResult, Error> {
    let mut output = HandlerOutput::builder();

    // An IBC client running on the local (host) chain should exist.
    ctx.client_state(msg.client_id())?;

    let new_connection_end = ConnectionEnd::new(
        State::Init,
        msg.client_id().clone(),
        msg.counterparty().clone(),
        ctx.get_compatible_versions(),
        msg.delay_period,
    );

    // Construct the identifier for the new connection.
    let id_counter = ctx.connection_counter()?;
    let conn_id = ConnectionId::new(id_counter);

    output.log(format!(
        "success: generated new connection identifier: {}",
        conn_id
    ));

    let result = ConnectionResult {
        connection_id: conn_id.clone(),
        connection_id_state: ConnectionIdState::Generated,
        connection_end: new_connection_end,
    };

    let event_attributes = Attributes {
        connection_id: Some(conn_id),
        ..Default::default()
    };
    output.emit(IbcEvent::OpenInitConnection(event_attributes.into()));

    Ok(output.with_result(result))
}

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;
    use test_env_log::test;

    use crate::events::IbcEvent;
    use crate::ics03_connection::connection::State;
    use crate::ics03_connection::handler::{dispatch, ConnectionResult};
    use crate::ics03_connection::msgs::conn_open_init::test_util::get_dummy_raw_msg_conn_open_init;
    use crate::ics03_connection::msgs::conn_open_init::MsgConnectionOpenInit;
    use crate::ics03_connection::msgs::ConnectionMsg;
    use crate::mock::context::MockContext;
    use crate::Height;

    #[test]
    fn conn_open_init_msg_processing() {
        struct Test {
            name: String,
            ctx: MockContext,
            msg: ConnectionMsg,
            want_pass: bool,
        }

        let msg_conn_init =
            MsgConnectionOpenInit::try_from(get_dummy_raw_msg_conn_open_init()).unwrap();
        let context = MockContext::default();

        let tests: Vec<Test> = vec![
            Test {
                name: "Processing fails because no client exists in the context".to_string(),
                ctx: context.clone(),
                msg: ConnectionMsg::ConnectionOpenInit(msg_conn_init.clone()),
                want_pass: false,
            },
            Test {
                name: "Good parameters".to_string(),
                ctx: context.with_client(msg_conn_init.client_id(), Height::new(0, 10)),
                msg: ConnectionMsg::ConnectionOpenInit(msg_conn_init.clone()),
                want_pass: true,
            },
        ]
        .into_iter()
        .collect();

        for test in tests {
            let res = dispatch(&test.ctx, test.msg.clone());
            // Additionally check the events and the output objects in the result.
            match res {
                Ok(proto_output) => {
                    assert!(
                        test.want_pass,
                        "conn_open_init: test passed but was supposed to fail for test: {}, \nparams {:?} {:?}",
                        test.name,
                        test.msg.clone(),
                        test.ctx.clone()
                    );

                    assert!(!proto_output.events.is_empty()); // Some events must exist.

                    // The object in the output is a ConnectionEnd, should have init state.
                    let res: ConnectionResult = proto_output.result;
                    assert_eq!(res.connection_end.state().clone(), State::Init);

                    for e in proto_output.events.iter() {
                        assert!(matches!(e, &IbcEvent::OpenInitConnection(_)));
                    }
                }
                Err(e) => {
                    assert!(
                        !test.want_pass,
                        "conn_open_init: did not pass test: {}, \nparams {:?} {:?} error: {:?}",
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
