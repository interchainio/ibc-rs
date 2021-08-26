//! Protocol logic specific to processing ICS2 messages of type `MsgUpdateAnyClient`.

use crate::events::IbcEvent;
use crate::handler::{HandlerOutput, HandlerResult};
use crate::ics02_client::client_consensus::AnyConsensusState;
use crate::ics02_client::client_def::{AnyClient, ClientDef};
use crate::ics02_client::client_state::AnyClientState;
use crate::ics02_client::context::ClientReader;
use crate::ics02_client::error::Error;
use crate::ics02_client::events::Attributes;
use crate::ics02_client::handler::ClientResult;
use crate::ics02_client::msgs::update_client::MsgUpdateAnyClient;
use crate::ics24_host::identifier::ClientId;

/// The result following the successful processing of a `MsgUpdateAnyClient` message. Preferably
/// this data type should be used with a qualified name `update_client::Result` to avoid ambiguity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Result {
    pub client_id: ClientId,
    pub client_state: AnyClientState,
    pub consensus_state: AnyConsensusState,
}

pub fn process(
    ctx: &dyn ClientReader,
    msg: MsgUpdateAnyClient,
) -> HandlerResult<ClientResult, Error> {
    let mut output = HandlerOutput::builder();

    let MsgUpdateAnyClient {
        client_id,
        header,
        signer: _,
    } = msg;

    // Read client type from the host chain store. The client should already exist.
    let client_type = ctx.client_type(&client_id)?;

    let client_def = AnyClient::from_client_type(client_type);

    // Read client state from the host chain store.
    let client_state = ctx.client_state(&client_id)?;

    let latest_height = client_state.latest_height();
    ctx.consensus_state(&client_id, latest_height)?;

    // Use client_state to validate the new header against the latest consensus_state.
    // This function will return the new client_state (its latest_height changed) and a
    // consensus_state obtained from header. These will be later persisted by the keeper.
    let (new_client_state, new_consensus_state) = client_def
        .check_header_and_update_state(client_state, header)
        .map_err(|e| Error::header_verification_failure(e.to_string()))?;

    let result = ClientResult::Update(Result {
        client_id: client_id.clone(),
        client_state: new_client_state,
        consensus_state: new_consensus_state,
    });

    let event_attributes = Attributes {
        client_id,
        ..Default::default()
    };
    output.emit(IbcEvent::UpdateClient(event_attributes.into()));

    Ok(output.with_result(result))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use test_env_log::test;

    use crate::events::IbcEvent;
    use crate::handler::HandlerOutput;
    use crate::ics02_client::client_state::AnyClientState;
    use crate::ics02_client::error::{Error, ErrorDetail};
    use crate::ics02_client::handler::dispatch;
    use crate::ics02_client::handler::ClientResult::Update;
    use crate::ics02_client::header::Header;
    use crate::ics02_client::msgs::update_client::MsgUpdateAnyClient;
    use crate::ics02_client::msgs::ClientMsg;
    use crate::ics24_host::identifier::ClientId;
    use crate::mock::client_state::MockClientState;
    use crate::mock::context::MockContext;
    use crate::mock::header::MockHeader;
    use crate::test_utils::get_dummy_account_id;
    use crate::Height;

    #[test]
    fn test_update_client_ok() {
        let client_id = ClientId::default();
        let signer = get_dummy_account_id();

        let ctx = MockContext::default().with_client(&client_id, Height::new(0, 42));
        let msg = MsgUpdateAnyClient {
            client_id: client_id.clone(),
            header: MockHeader::new(Height::new(0, 46)).into(),
            signer,
        };

        let output = dispatch(&ctx, ClientMsg::UpdateClient(msg.clone()));

        match output {
            Ok(HandlerOutput {
                result,
                mut events,
                log,
            }) => {
                assert_eq!(events.len(), 1);
                let event = events.pop().unwrap();
                assert!(
                    matches!(event, IbcEvent::UpdateClient(e) if e.client_id() == &msg.client_id)
                );
                assert!(log.is_empty());
                // Check the result
                match result {
                    Update(upd_res) => {
                        assert_eq!(upd_res.client_id, client_id);
                        assert_eq!(
                            upd_res.client_state,
                            AnyClientState::Mock(MockClientState(MockHeader::new(
                                msg.header.height()
                            )))
                        )
                    }
                    _ => panic!("update handler result has incorrect type"),
                }
            }
            Err(err) => {
                panic!("unexpected error: {}", err);
            }
        }
    }

    #[test]
    fn test_update_nonexisting_client() {
        let client_id = ClientId::from_str("mockclient1").unwrap();
        let signer = get_dummy_account_id();

        let ctx = MockContext::default().with_client(&client_id, Height::new(0, 42));

        let msg = MsgUpdateAnyClient {
            client_id: ClientId::from_str("nonexistingclient").unwrap(),
            header: MockHeader::new(Height::new(0, 46)).into(),
            signer,
        };

        let output = dispatch(&ctx, ClientMsg::UpdateClient(msg.clone()));

        match output {
            Err(Error(ErrorDetail::ClientNotFound(e), _)) => {
                assert_eq!(e.client_id, msg.client_id);
            }
            _ => {
                panic!("expected ClientNotFound error, instead got {:?}", output)
            }
        }
    }

    #[test]
    fn test_update_client_ok_multiple() {
        let client_ids = vec![
            ClientId::from_str("mockclient1").unwrap(),
            ClientId::from_str("mockclient2").unwrap(),
            ClientId::from_str("mockclient3").unwrap(),
        ];
        let signer = get_dummy_account_id();
        let initial_height = Height::new(0, 45);
        let update_height = Height::new(0, 49);

        let mut ctx = MockContext::default();

        for cid in &client_ids {
            ctx = ctx.with_client(cid, initial_height);
        }

        for cid in &client_ids {
            let msg = MsgUpdateAnyClient {
                client_id: cid.clone(),
                header: MockHeader::new(update_height).into(),
                signer: signer.clone(),
            };

            let output = dispatch(&ctx, ClientMsg::UpdateClient(msg.clone()));

            match output {
                Ok(HandlerOutput {
                    result: _,
                    mut events,
                    log,
                }) => {
                    assert_eq!(events.len(), 1);
                    let event = events.pop().unwrap();
                    assert!(
                        matches!(event, IbcEvent::UpdateClient(e) if e.client_id() == &msg.client_id)
                    );
                    assert!(log.is_empty());
                }
                Err(err) => {
                    panic!("unexpected error: {}", err);
                }
            }
        }
    }
}
