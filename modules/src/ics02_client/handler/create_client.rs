//! Protocol logic specific to processing ICS2 messages of type `MsgCreateAnyClient`.

use crate::events::IbcEvent;
use crate::handler::{HandlerOutput, HandlerResult};
use crate::ics02_client::client_consensus::AnyConsensusState;
use crate::ics02_client::client_state::AnyClientState;
use crate::ics02_client::client_type::ClientType;
use crate::ics02_client::context::ClientReader;
use crate::ics02_client::error::Error;
use crate::ics02_client::events::Attributes;
use crate::ics02_client::handler::ClientResult;
use crate::ics02_client::msgs::create_client::MsgCreateAnyClient;
use crate::ics24_host::identifier::ClientId;

/// The result following the successful processing of a `MsgCreateAnyClient` message. Preferably
/// this data type should be used with a qualified name `create_client::Result` to avoid ambiguity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Result {
    pub client_id: ClientId,
    pub client_type: ClientType,
    pub client_state: AnyClientState,
    pub consensus_state: AnyConsensusState,
}

pub fn process(
    ctx: &dyn ClientReader,
    msg: MsgCreateAnyClient,
) -> HandlerResult<ClientResult, Error> {
    let mut output = HandlerOutput::builder();

    // Construct this client's identifier
    let id_counter = ctx.client_counter()?;
    let client_id = ClientId::new(msg.client_state().client_type(), id_counter).map_err(|e| {
        Error::client_identifier_constructor(msg.client_state().client_type(), id_counter, e)
    })?;

    output.log(format!(
        "success: generated new client identifier: {}",
        client_id
    ));

    let result = ClientResult::Create(Result {
        client_id: client_id.clone(),
        client_type: msg.client_state().client_type(),
        client_state: msg.client_state(),
        consensus_state: msg.consensus_state(),
    });

    let event_attributes = Attributes {
        client_id,
        ..Default::default()
    };
    output.emit(IbcEvent::CreateClient(event_attributes.into()));

    Ok(output.with_result(result))
}

#[cfg(test)]
mod tests {
    use std::convert::TryInto;
    use std::time::Duration;
    use test_env_log::test;

    use crate::events::IbcEvent;
    use crate::handler::HandlerOutput;
    use crate::ics02_client::client_consensus::AnyConsensusState;
    use crate::ics02_client::client_state::AnyClientState;
    use crate::ics02_client::client_type::ClientType;
    use crate::ics02_client::handler::{dispatch, ClientResult};
    use crate::ics02_client::msgs::create_client::MsgCreateAnyClient;
    use crate::ics02_client::msgs::ClientMsg;
    use crate::ics02_client::trust_threshold::TrustThreshold;
    use crate::ics07_tendermint::client_state::{AllowUpdate, ClientState};
    use crate::ics07_tendermint::header::test_util::get_dummy_tendermint_header;
    use crate::ics24_host::identifier::ClientId;
    use crate::mock::client_state::{MockClientState, MockConsensusState};
    use crate::mock::context::MockContext;
    use crate::mock::header::MockHeader;
    use crate::test_utils::get_dummy_account_id;
    use crate::Height;

    #[test]
    fn test_create_client_ok() {
        let ctx = MockContext::default();
        let signer = get_dummy_account_id();
        let height = Height::new(0, 42);

        let msg = MsgCreateAnyClient::new(
            MockClientState(MockHeader::new(height)).into(),
            MockConsensusState::new(MockHeader::new(height)).into(),
            signer,
        )
        .unwrap();

        let output = dispatch(&ctx, ClientMsg::CreateClient(msg.clone()));

        match output {
            Ok(HandlerOutput {
                result, mut events, ..
            }) => {
                assert_eq!(events.len(), 1);
                let event = events.pop().unwrap();
                let expected_client_id = ClientId::new(ClientType::Mock, 0).unwrap();
                assert!(
                    matches!(event, IbcEvent::CreateClient(e) if e.client_id() == &expected_client_id)
                );
                match result {
                    ClientResult::Create(create_result) => {
                        assert_eq!(create_result.client_type, ClientType::Mock);
                        assert_eq!(create_result.client_id, expected_client_id);
                        assert_eq!(create_result.client_state, msg.client_state());
                        assert_eq!(create_result.consensus_state, msg.consensus_state());
                    }
                    _ => {
                        panic!("unexpected result type: expected ClientResult::CreateResult!");
                    }
                }
            }
            Err(err) => {
                panic!("unexpected error: {}", err);
            }
        }
    }

    #[test]
    fn test_create_client_ok_multiple() {
        let existing_client_id = ClientId::default();
        let signer = get_dummy_account_id();
        let height = Height::new(0, 80);

        let ctx = MockContext::default().with_client(&existing_client_id, height);

        let create_client_msgs: Vec<MsgCreateAnyClient> = vec![
            MsgCreateAnyClient::new(
                MockClientState(MockHeader::new(Height {
                    revision_height: 42,
                    ..height
                }))
                .into(),
                MockConsensusState::new(MockHeader::new(Height {
                    revision_height: 42,
                    ..height
                }))
                .into(),
                signer.clone(),
            )
            .unwrap(),
            MsgCreateAnyClient::new(
                MockClientState(MockHeader::new(Height {
                    revision_height: 42,
                    ..height
                }))
                .into(),
                MockConsensusState::new(MockHeader::new(Height {
                    revision_height: 42,
                    ..height
                }))
                .into(),
                signer.clone(),
            )
            .unwrap(),
            MsgCreateAnyClient::new(
                MockClientState(MockHeader::new(Height {
                    revision_height: 50,
                    ..height
                }))
                .into(),
                MockConsensusState::new(MockHeader::new(Height {
                    revision_height: 50,
                    ..height
                }))
                .into(),
                signer,
            )
            .unwrap(),
        ]
        .into_iter()
        .collect();

        // The expected client id that will be generated will be identical to "9999-mock-0" for all
        // tests. This is because we're not persisting any client results (which is done via the
        // tests for `ics26_routing::dispatch`.
        let expected_client_id = ClientId::new(ClientType::Mock, 0).unwrap();

        for msg in create_client_msgs {
            let output = dispatch(&ctx, ClientMsg::CreateClient(msg.clone()));

            match output {
                Ok(HandlerOutput {
                    result, mut events, ..
                }) => {
                    assert_eq!(events.len(), 1);
                    let event = events.pop().unwrap();
                    assert!(
                        matches!(event, IbcEvent::CreateClient(e) if e.client_id() == &expected_client_id)
                    );
                    match result {
                        ClientResult::Create(create_res) => {
                            assert_eq!(create_res.client_type, msg.client_state().client_type());
                            assert_eq!(create_res.client_id, expected_client_id);
                            assert_eq!(create_res.client_state, msg.client_state());
                            assert_eq!(create_res.consensus_state, msg.consensus_state());
                        }
                        _ => {
                            panic!("expected result of type ClientResult::CreateResult");
                        }
                    }
                }
                Err(err) => {
                    panic!("unexpected error: {}", err);
                }
            }
        }
    }

    #[test]
    fn test_tm_create_client_ok() {
        let signer = get_dummy_account_id();

        let ctx = MockContext::default();

        let tm_header = get_dummy_tendermint_header();

        let tm_client_state = AnyClientState::Tendermint(ClientState {
            chain_id: tm_header.chain_id.clone().into(),
            trust_level: TrustThreshold::ONE_THIRD,
            trusting_period: Duration::from_secs(64000),
            unbonding_period: Duration::from_secs(128000),
            max_clock_drift: Duration::from_millis(3000),
            latest_height: Height::new(0, u64::from(tm_header.height)),
            frozen_height: Height::zero(),
            allow_update: AllowUpdate {
                after_expiry: false,
                after_misbehaviour: false,
            },
            upgrade_path: vec!["".to_string()],
        });

        let msg = MsgCreateAnyClient::new(
            tm_client_state,
            AnyConsensusState::Tendermint(tm_header.try_into().unwrap()),
            signer,
        )
        .unwrap();

        let output = dispatch(&ctx, ClientMsg::CreateClient(msg.clone()));

        match output {
            Ok(HandlerOutput {
                result, mut events, ..
            }) => {
                assert_eq!(events.len(), 1);
                let event = events.pop().unwrap();
                let expected_client_id = ClientId::new(ClientType::Tendermint, 0).unwrap();
                assert!(
                    matches!(event, IbcEvent::CreateClient(e) if e.client_id() == &expected_client_id)
                );
                match result {
                    ClientResult::Create(create_res) => {
                        assert_eq!(create_res.client_type, ClientType::Tendermint);
                        assert_eq!(create_res.client_id, expected_client_id);
                        assert_eq!(create_res.client_state, msg.client_state());
                        assert_eq!(create_res.consensus_state, msg.consensus_state());
                    }
                    _ => {
                        panic!("expected result of type ClientResult::CreateResult");
                    }
                }
            }
            Err(err) => {
                panic!("unexpected error: {}", err);
            }
        }
    }
}
