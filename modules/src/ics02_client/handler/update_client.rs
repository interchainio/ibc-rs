#![allow(unreachable_code, unused_variables)]

use crate::handler::{HandlerOutput, HandlerResult};
use crate::ics02_client::client_def::{AnyClient, ClientDef};
use crate::ics02_client::context::{ClientKeeper, ClientReader};
use crate::ics02_client::error::{Error, Kind};
use crate::ics02_client::handler::ClientEvent;

use crate::ics02_client::msgs::MsgUpdateAnyClient;
use crate::ics02_client::state::ClientState;
use crate::ics24_host::identifier::ClientId;

#[derive(Debug)]
pub struct UpdateClientResult<CD: ClientDef> {
    client_id: ClientId,
    client_state: CD::ClientState,
    consensus_state: CD::ConsensusState,
}

pub fn process(
    ctx: &dyn ClientReader,
    msg: MsgUpdateAnyClient<AnyClient>,
) -> HandlerResult<UpdateClientResult<AnyClient>, Error> {
    let mut output = HandlerOutput::builder();

    let MsgUpdateAnyClient { client_id, header } = msg;

    let client_type = ctx
        .client_type(&client_id)
        .ok_or_else(|| Kind::ClientNotFound(client_id.clone()))?;

    let client_state = ctx
        .client_state(&client_id)
        .ok_or_else(|| Kind::ClientNotFound(client_id.clone()))?;

    let latest_height = client_state.latest_height();
    let consensus_state = ctx
        .consensus_state(&client_id, latest_height)
        .ok_or_else(|| Kind::ConsensusStateNotFound(client_id.clone(), latest_height))?;

    // Use client_state to validate the new header against the latest consensus_state.
    // This function will return the new client_state (its latest_height changed) and a
    // consensus_state obtained from header. These will be later persisted by the keeper.
    let (new_client_state, new_consensus_state) = client_state
        .check_header_and_update_state(header)
        .map_err(|_| Kind::HeaderVerificationFailure)?;

    output.emit(ClientEvent::ClientUpdated(client_id.clone()));

    Ok(output.with_result(UpdateClientResult {
        client_id,
        client_state: new_client_state,
        consensus_state: new_consensus_state,
    }))
}

pub fn keep(
    keeper: &mut dyn ClientKeeper,
    result: UpdateClientResult<AnyClient>,
) -> Result<(), Error> {
    keeper.store_client_state(result.client_id.clone(), result.client_state)?;
    keeper.store_consensus_state(result.client_id, result.consensus_state)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ics02_client::client_type::ClientType;
    use crate::ics02_client::context_mock::MockClientContext;
    use crate::mock_client::header::MockHeader;
    use crate::mock_client::state::{MockClientState, MockConsensusState};
    use std::collections::HashMap;
    use tendermint::block::Height;

    #[test]
    fn test_update_client_ok() {
        let mut ctx = MockClientContext {
            client_type: Some(ClientType::Tendermint),
            client_states: HashMap::with_capacity(1),
            consensus_states: HashMap::with_capacity(1),
        };

        ctx.client_states.insert(
            "mockclient".parse().unwrap(),
            MockClientState(MockHeader(Height(42))).into(),
        );
        ctx.consensus_states
            .insert(Height(42), MockConsensusState(MockHeader(Height(42))));

        let msg = MsgUpdateAnyClient {
            client_id: "mockclient".parse().unwrap(),
            header: MockHeader(Height(46)).into(),
        };

        let output = process(&ctx, msg.clone());

        match output {
            Ok(HandlerOutput {
                result: _,
                events,
                log,
            }) => {
                assert_eq!(
                    events,
                    vec![ClientEvent::ClientUpdated(msg.client_id).into()]
                );
                assert!(log.is_empty());
            }
            Err(err) => {
                panic!("unexpected error: {}", err);
            }
        }
    }

    #[test]
    fn test_update_nonexisting_client() {
        let mut ctx = MockClientContext {
            client_type: Some(ClientType::Tendermint),
            client_states: HashMap::with_capacity(1),
            consensus_states: HashMap::with_capacity(1),
        };

        ctx.client_states.insert(
            "mockclient1".parse().unwrap(),
            MockClientState(MockHeader(Height(42))).into(),
        );
        ctx.consensus_states
            .insert(Height(42), MockConsensusState(MockHeader(Height(42))));

        let msg = MsgUpdateAnyClient {
            client_id: "nonexistingclient".parse().unwrap(),
            header: MockHeader(Height(46)).into(),
        };

        let output = process(&ctx, msg.clone());

        match output {
            Ok(_) => {
                panic!("unexpected success (expected error)");
            }
            Err(err) => {
                assert_eq!(err.kind(), &Kind::ClientNotFound(msg.client_id));
            }
        }
    }

    #[test]
    fn test_update_client_ok_multiple() {
        let client_ids: Vec<ClientId> = vec![
            "mockclient1".parse().unwrap(),
            "mockclient2".parse().unwrap(),
            "mockclient3".parse().unwrap(),
        ];

        let initial_height = Height(45);
        let update_height = Height(49);

        let mut ctx = MockClientContext {
            client_type: Some(ClientType::Tendermint),
            client_states: HashMap::with_capacity(client_ids.len()),
            consensus_states: HashMap::with_capacity(client_ids.len()),
        };

        for cid in &client_ids {
            ctx.client_states.insert(
                cid.clone(),
                MockClientState(MockHeader(initial_height)).into(),
            );
            ctx.consensus_states.insert(
                initial_height,
                MockConsensusState(MockHeader(initial_height)),
            );
        }

        for cid in &client_ids {
            let msg = MsgUpdateAnyClient {
                client_id: cid.clone(),
                header: MockHeader(update_height).into(),
            };

            let output = process(&ctx, msg.clone());

            match output {
                Ok(HandlerOutput {
                    result: _,
                    events,
                    log,
                }) => {
                    assert_eq!(
                        events,
                        vec![ClientEvent::ClientUpdated(msg.client_id).into()]
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
