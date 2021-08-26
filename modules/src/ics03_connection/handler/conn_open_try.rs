//! Protocol logic specific to processing ICS3 messages of type `MsgConnectionOpenTry`.

use crate::events::IbcEvent;
use crate::handler::{HandlerOutput, HandlerResult};
use crate::ics03_connection::connection::{ConnectionEnd, Counterparty, State};
use crate::ics03_connection::context::ConnectionReader;
use crate::ics03_connection::error::Error;
use crate::ics03_connection::events::Attributes;
use crate::ics03_connection::handler::verify::{check_client_consensus_height, verify_proofs};
use crate::ics03_connection::handler::{ConnectionIdState, ConnectionResult};
use crate::ics03_connection::msgs::conn_open_try::MsgConnectionOpenTry;
use crate::ics24_host::identifier::ConnectionId;

pub(crate) fn process(
    ctx: &dyn ConnectionReader,
    msg: MsgConnectionOpenTry,
) -> HandlerResult<ConnectionResult, Error> {
    let mut output = HandlerOutput::builder();

    // Check that consensus height (for client proof) in message is not too advanced nor too old.
    check_client_consensus_height(ctx, msg.consensus_height())?;

    // Unwrap the old connection end (if any) and its identifier.
    let (mut new_connection_end, conn_id) = match msg.previous_connection_id() {
        // A connection with this id should already exist. Search & validate.
        Some(prev_id) => {
            let old_connection_end = ctx.connection_end(prev_id)?;

            // Validate that existing connection end matches with the one we're trying to establish.
            if old_connection_end.state_matches(&State::Init)
                && old_connection_end.counterparty_matches(&msg.counterparty())
                && old_connection_end.client_id_matches(msg.client_id())
                && old_connection_end.delay_period() == msg.delay_period
            {
                // A ConnectionEnd already exists and all validation passed.
                output.log(format!(
                    "success: `previous_connection_id` {} validation passed",
                    prev_id
                ));
                Ok((old_connection_end, prev_id.clone()))
            } else {
                // A ConnectionEnd already exists and validation failed.
                Err(Error::connection_mismatch(prev_id.clone()))
            }
        }
        // No prev. connection id was supplied, create a new connection end and conn id.
        None => {
            // Build a new connection end as well as an identifier.
            let conn_end = ConnectionEnd::new(
                State::Init,
                msg.client_id().clone(),
                msg.counterparty(),
                msg.counterparty_versions(),
                msg.delay_period,
            );
            let id_counter = ctx.connection_counter()?;
            let conn_id = ConnectionId::new(id_counter);

            output.log(format!(
                "success: new connection end and identifier {} generated",
                conn_id
            ));
            Ok((conn_end, conn_id))
        }
    }?;

    // Proof verification in two steps:
    // 1. Setup: build the ConnectionEnd as we expect to find it on the other party.
    let expected_conn = ConnectionEnd::new(
        State::Init,
        msg.counterparty().client_id().clone(),
        Counterparty::new(msg.client_id().clone(), None, ctx.commitment_prefix()),
        msg.counterparty_versions(),
        msg.delay_period,
    );

    // 2. Pass the details to the verification function.
    verify_proofs(
        ctx,
        msg.client_state(),
        &new_connection_end,
        &expected_conn,
        msg.proofs(),
    )?;

    // Transition the connection end to the new state & pick a version.
    new_connection_end.set_state(State::TryOpen);

    // Pick the version.
    new_connection_end
        .set_version(ctx.pick_version(ctx.get_compatible_versions(), msg.counterparty_versions())?);

    assert_eq!(new_connection_end.versions().len(), 1);

    output.log("success: connection verification passed");

    let result = ConnectionResult {
        connection_id: conn_id.clone(),
        connection_id_state: if matches!(msg.previous_connection_id, None) {
            ConnectionIdState::Generated
        } else {
            ConnectionIdState::Reused
        },
        connection_end: new_connection_end,
    };

    let event_attributes = Attributes {
        connection_id: Some(conn_id),
        ..Default::default()
    };
    output.emit(IbcEvent::OpenTryConnection(event_attributes.into()));

    Ok(output.with_result(result))
}

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;
    use test_env_log::test;

    use crate::events::IbcEvent;
    use crate::ics03_connection::connection::State;
    use crate::ics03_connection::handler::{dispatch, ConnectionResult};
    use crate::ics03_connection::msgs::conn_open_try::test_util::get_dummy_raw_msg_conn_open_try;
    use crate::ics03_connection::msgs::conn_open_try::MsgConnectionOpenTry;
    use crate::ics03_connection::msgs::ConnectionMsg;
    use crate::ics24_host::identifier::ChainId;
    use crate::mock::context::MockContext;
    use crate::mock::host::HostType;
    use crate::Height;

    #[test]
    fn conn_open_try_msg_processing() {
        struct Test {
            name: String,
            ctx: MockContext,
            msg: ConnectionMsg,
            want_pass: bool,
        }

        let host_chain_height = Height::new(0, 35);
        let max_history_size = 5;
        let context = MockContext::new(
            ChainId::new("mockgaia".to_string(), 0),
            HostType::Mock,
            max_history_size,
            host_chain_height,
        );
        let client_consensus_state_height = 10;

        let msg_conn_try = MsgConnectionOpenTry::try_from(get_dummy_raw_msg_conn_open_try(
            client_consensus_state_height,
            host_chain_height.revision_height,
        ))
        .unwrap();

        // The proof targets a height that does not exist (i.e., too advanced) on destination chain.
        let msg_height_advanced = MsgConnectionOpenTry::try_from(get_dummy_raw_msg_conn_open_try(
            client_consensus_state_height,
            host_chain_height.increment().revision_height,
        ))
        .unwrap();
        let pruned_height = host_chain_height
            .sub(max_history_size as u64 + 1)
            .unwrap()
            .revision_height;
        // The consensus proof targets a missing height (pruned) on destination chain.
        let msg_height_old = MsgConnectionOpenTry::try_from(get_dummy_raw_msg_conn_open_try(
            client_consensus_state_height,
            pruned_height,
        ))
        .unwrap();

        // The proofs in this message are created at a height which the client on destination chain does not have.
        let msg_proof_height_missing =
            MsgConnectionOpenTry::try_from(get_dummy_raw_msg_conn_open_try(
                client_consensus_state_height - 1,
                host_chain_height.revision_height,
            ))
            .unwrap();

        let tests: Vec<Test> = vec![
            Test {
                name: "Processing fails because the height is too advanced".to_string(),
                ctx: context.clone(),
                msg: ConnectionMsg::ConnectionOpenTry(Box::new(msg_height_advanced)),
                want_pass: false,
            },
            Test {
                name: "Processing fails because the height is too old".to_string(),
                ctx: context.clone(),
                msg: ConnectionMsg::ConnectionOpenTry(Box::new(msg_height_old)),
                want_pass: false,
            },
            Test {
                name: "Processing fails because no client exists".to_string(),
                ctx: context.clone(),
                msg: ConnectionMsg::ConnectionOpenTry(Box::new(msg_conn_try.clone())),
                want_pass: false,
            },
            Test {
                name: "Processing fails because the client misses the consensus state targeted by the proof".to_string(),
                ctx: context.clone().with_client(msg_proof_height_missing.client_id(), Height::new(0, client_consensus_state_height)),
                msg: ConnectionMsg::ConnectionOpenTry(Box::new(msg_proof_height_missing)),
                want_pass: false,
            },
            Test {
                name: "Good parameters but has previous_connection_id".to_string(),
                ctx: context.clone().with_client(msg_conn_try.client_id(), Height::new(0, client_consensus_state_height)),
                msg: ConnectionMsg::ConnectionOpenTry(Box::new(msg_conn_try.clone())),
                want_pass: false,
            },
            Test {
                name: "Good parameters".to_string(),
                ctx: context.with_client(msg_conn_try.client_id(), Height::new(0, client_consensus_state_height)),
                msg: ConnectionMsg::ConnectionOpenTry(Box::new(msg_conn_try.with_previous_connection_id(None))),
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
                        "conn_open_try: test passed but was supposed to fail for test: {}, \nparams {:?} {:?}",
                        test.name,
                        test.msg.clone(),
                        test.ctx.clone()
                    );

                    assert!(!proto_output.events.is_empty()); // Some events must exist.

                    // The object in the output is a ConnectionEnd, should have TryOpen state.
                    let res: ConnectionResult = proto_output.result;
                    assert_eq!(res.connection_end.state().clone(), State::TryOpen);

                    for e in proto_output.events.iter() {
                        assert!(matches!(e, &IbcEvent::OpenTryConnection(_)));
                    }
                }
                Err(e) => {
                    assert!(
                        !test.want_pass,
                        "conn_open_try: failed for test: {}, \nparams {:?} {:?} error: {:?}",
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
