//! Protocol logic specific to ICS4 messages of type `MsgChannelOpenInit`.

use crate::events::IbcEvent;
use crate::handler::{HandlerOutput, HandlerResult};
use crate::ics04_channel::channel::{ChannelEnd, State};
use crate::ics04_channel::context::ChannelReader;
use crate::ics04_channel::error::Error;
use crate::ics04_channel::events::Attributes;
use crate::ics04_channel::handler::{ChannelIdState, ChannelResult};
use crate::ics04_channel::msgs::chan_open_init::MsgChannelOpenInit;
use crate::ics24_host::identifier::ChannelId;

pub(crate) fn process(
    ctx: &dyn ChannelReader,
    msg: MsgChannelOpenInit,
) -> HandlerResult<ChannelResult, Error> {
    let mut output = HandlerOutput::builder();

    // Channel capabilities
    let channel_cap = ctx.authenticated_capability(&msg.port_id().clone())?;

    if msg.channel().connection_hops().len() != 1 {
        return Err(Error::invalid_connection_hops_length(
            1,
            msg.channel().connection_hops().len(),
        ));
    }

    // An IBC connection running on the local (host) chain should exist.
    let conn = ctx.connection_end(&msg.channel().connection_hops()[0])?;
    let get_versions = conn.versions();
    let version = match get_versions.as_slice() {
        [version] => version,
        _ => return Err(Error::invalid_version_length_connection()),
    };

    let channel_feature = msg.channel().ordering().to_string();
    if !version.is_supported_feature(channel_feature) {
        return Err(Error::channel_feature_not_suported_by_connection());
    }

    // TODO: Check that `version` is non empty but not necessary coherent
    if msg.channel().version().is_empty() {
        return Err(Error::empty_version());
    }

    // Channel identifier construction.
    let id_counter = ctx.channel_counter()?;
    let chan_id = ChannelId::new(id_counter);

    output.log(format!(
        "success: generated new channel identifier: {}",
        chan_id
    ));

    let new_channel_end = ChannelEnd::new(
        State::Init,
        *msg.channel().ordering(),
        msg.channel().counterparty().clone(),
        msg.channel().connection_hops().clone(),
        msg.channel().version(),
    );

    output.log("success: no channel found");

    let result = ChannelResult {
        port_id: msg.port_id().clone(),
        channel_id: chan_id.clone(),
        channel_end: new_channel_end,
        channel_id_state: ChannelIdState::Generated,
        channel_cap,
    };

    let event_attributes = Attributes {
        channel_id: Some(chan_id),
        ..Default::default()
    };
    output.emit(IbcEvent::OpenInitChannel(event_attributes.into()));

    Ok(output.with_result(result))
}

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;
    use test_env_log::test;

    use crate::events::IbcEvent;
    use crate::ics03_connection::connection::ConnectionEnd;
    use crate::ics03_connection::connection::State as ConnectionState;
    use crate::ics03_connection::msgs::conn_open_init::test_util::get_dummy_raw_msg_conn_open_init;
    use crate::ics03_connection::msgs::conn_open_init::MsgConnectionOpenInit;
    use crate::ics03_connection::version::get_compatible_versions;
    use crate::ics04_channel::channel::State;
    use crate::ics04_channel::handler::{channel_dispatch, ChannelResult};
    use crate::ics04_channel::msgs::chan_open_init::test_util::get_dummy_raw_msg_chan_open_init;
    use crate::ics04_channel::msgs::chan_open_init::MsgChannelOpenInit;
    use crate::ics04_channel::msgs::ChannelMsg;
    use crate::ics24_host::identifier::ConnectionId;
    use crate::mock::context::MockContext;

    #[test]
    fn chan_open_init_msg_processing() {
        struct Test {
            name: String,
            ctx: MockContext,
            msg: ChannelMsg,
            want_pass: bool,
        }

        let msg_chan_init =
            MsgChannelOpenInit::try_from(get_dummy_raw_msg_chan_open_init()).unwrap();

        let context = MockContext::default();

        let msg_conn_init =
            MsgConnectionOpenInit::try_from(get_dummy_raw_msg_conn_open_init()).unwrap();

        let init_conn_end = ConnectionEnd::new(
            ConnectionState::Init,
            msg_conn_init.client_id().clone(),
            msg_conn_init.counterparty().clone(),
            get_compatible_versions(),
            msg_conn_init.delay_period,
        );

        let cid = ConnectionId::default();

        let tests: Vec<Test> = vec![
            Test {
                name: "Processing fails because no connection exists in the context".to_string(),
                ctx: context.clone(),
                msg: ChannelMsg::ChannelOpenInit(msg_chan_init.clone()),
                want_pass: false,
            },
            Test {
                name: "Processing fails because port does not have a capability associated"
                    .to_string(),
                ctx: context
                    .clone()
                    .with_connection(cid.clone(), init_conn_end.clone()),
                msg: ChannelMsg::ChannelOpenInit(msg_chan_init.clone()),
                want_pass: false,
            },
            Test {
                name: "Good parameters".to_string(),
                ctx: context
                    .with_connection(cid, init_conn_end)
                    .with_port_capability(
                        MsgChannelOpenInit::try_from(get_dummy_raw_msg_chan_open_init())
                            .unwrap()
                            .port_id()
                            .clone(),
                    ),
                msg: ChannelMsg::ChannelOpenInit(msg_chan_init),
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
                        "chan_open_init: test passed but was supposed to fail for test: {}, \nparams {:?} {:?}",
                        test.name,
                        test.msg.clone(),
                        test.ctx.clone()
                    );

                    assert!(!proto_output.events.is_empty()); // Some events must exist.

                    // The object in the output is a ChannelEnd, should have init state.
                    let res: ChannelResult = proto_output.result;
                    assert_eq!(res.channel_end.state().clone(), State::Init);
                    let msg_init = test.msg.clone();

                    if let ChannelMsg::ChannelOpenInit(msg_init) = msg_init {
                        assert_eq!(res.port_id.clone(), msg_init.port_id().clone());
                    }

                    for e in proto_output.events.iter() {
                        assert!(matches!(e, &IbcEvent::OpenInitChannel(_)));
                    }
                }
                Err(e) => {
                    assert!(
                        !test.want_pass,
                        "chan_open_init: did not pass test: {}, \nparams {:?} {:?} error: {:?}",
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
