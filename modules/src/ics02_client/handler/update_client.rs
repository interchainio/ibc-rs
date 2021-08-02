//! Protocol logic specific to processing ICS2 messages of type `MsgUpdateAnyClient`.

use tracing::info;

use crate::events::IbcEvent;
use crate::handler::{HandlerOutput, HandlerResult};
use crate::ics02_client::client_consensus::AnyConsensusState;
use crate::ics02_client::client_def::{AnyClient, ClientDef};
use crate::ics02_client::client_state::{AnyClientState, ClientState};
use crate::ics02_client::context::ClientReader;
use crate::ics02_client::error::Error;
use crate::ics02_client::events::Attributes;
use crate::ics02_client::handler::ClientResult;
use crate::ics02_client::header::Header;
use crate::ics02_client::msgs::update_client::MsgUpdateAnyClient;
use crate::ics24_host::identifier::ClientId;
use crate::timestamp::Timestamp;

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
    let client_type = ctx
        .client_type(&client_id)
        .ok_or_else(|| Error::client_not_found(client_id.clone()))?;

    let client_def = AnyClient::from_client_type(client_type);

    // Read client state from the host chain store.
    let client_state = ctx
        .client_state(&client_id)
        .ok_or_else(|| Error::client_not_found(client_id.clone()))?;

    if client_state.is_frozen() {
        return Err(Error::client_frozen(client_id));
    }

    // Read consensus state from the host chain store.
    let latest_consensus_state = ctx
        .consensus_state(&client_id, client_state.latest_height())
        .ok_or_else(|| {
            Error::consensus_state_not_found(client_id.clone(), client_state.latest_height())
        })?;

    info!("latest consensus state {:?}", latest_consensus_state);

    let duration = Timestamp::now()
        .duration_since(&latest_consensus_state.timestamp())
        .ok_or_else(|| {
            Error::invalid_consensus_state_timestamp(
                latest_consensus_state.timestamp(),
                header.timestamp(),
            )
        })?;

    if client_state.expired(duration) {
        return Err(Error::header_not_within_trust_period(
            latest_consensus_state.timestamp(),
            header.timestamp(),
        ));
    }

    // Use client_state to validate the new header against the latest consensus_state.
    // This function will return the new client_state (its latest_height changed) and a
    // consensus_state obtained from header. These will be later persisted by the keeper.
    let (new_client_state, new_consensus_state) = client_def
        .check_header_and_update_state(ctx, client_id.clone(), client_state, header)
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
    use crate::ics02_client::client_consensus::AnyConsensusState;
    use crate::ics02_client::client_state::{AnyClientState, ClientState};
    use crate::ics02_client::client_type::ClientType;
    use crate::ics02_client::error::{Error, ErrorDetail};
    use crate::ics02_client::handler::dispatch;
    use crate::ics02_client::handler::ClientResult::Update;
    use crate::ics02_client::header::{AnyHeader, Header};
    use crate::ics02_client::msgs::update_client::MsgUpdateAnyClient;
    use crate::ics02_client::msgs::ClientMsg;
    use crate::ics24_host::identifier::{ChainId, ClientId};
    use crate::mock::client_state::MockClientState;
    use crate::mock::context::MockContext;
    use crate::mock::header::MockHeader;
    use crate::mock::host::{HostBlock, HostType};
    use crate::test_utils::get_dummy_account_id;
    use crate::timestamp::Timestamp;
    use crate::Height;

    #[test]
    fn test_update_client_ok() {
        let client_id = ClientId::default();
        let signer = get_dummy_account_id();

        let timestamp = Timestamp::now();

        let ctx = MockContext::default().with_client(&client_id, Height::new(0, 42));
        let msg = MsgUpdateAnyClient {
            client_id: client_id.clone(),
            header: MockHeader::new(Height::new(0, 46))
                .with_timestamp(timestamp)
                .into(),
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
                            AnyClientState::Mock(MockClientState(
                                MockHeader::new(msg.header.height()).with_timestamp(timestamp)
                            ))
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

    #[test]
    fn test_update_syntetic_tendermint_client_adjacent_ok() {
        let client_id = ClientId::new(ClientType::Tendermint, 0).unwrap();
        let client_height = Height::new(1, 20);
        let update_height = Height::new(1, 21);

        let ctx = MockContext::new(
            ChainId::new("mockgaiaA".to_string(), 1),
            HostType::Mock,
            5,
            Height::new(1, 1),
        )
        .with_client_parametrized(
            &client_id,
            client_height,
            Some(ClientType::Tendermint), // The target host chain (B) is synthetic TM.
            Some(client_height),
        );

        let ctx_b = MockContext::new(
            ChainId::new("mockgaiaB".to_string(), 1),
            HostType::SyntheticTendermint,
            5,
            update_height,
        );

        let signer = get_dummy_account_id();

        let block_ref = ctx_b.host_block(update_height);
        let mut latest_header: AnyHeader = block_ref.cloned().map(Into::into).unwrap();

        latest_header = match latest_header {
            AnyHeader::Tendermint(mut theader) => {
                theader.trusted_height = client_height;
                AnyHeader::Tendermint(theader)
            }
            AnyHeader::Mock(m) => AnyHeader::Mock(m),
        };

        let msg = MsgUpdateAnyClient {
            client_id: client_id.clone(),
            header: latest_header,
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
                        assert!(!upd_res.client_state.is_frozen());
                        assert_eq!(upd_res.client_state.latest_height(), msg.header.height(),)
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
    fn test_update_syntetic_tendermint_client_non_adjacent_ok() {
        let client_id = ClientId::new(ClientType::Tendermint, 0).unwrap();
        let client_height = Height::new(1, 20);
        let update_height = Height::new(1, 21);

        let ctx = MockContext::new(
            ChainId::new("mockgaiaA".to_string(), 1),
            HostType::Mock,
            5,
            Height::new(1, 1),
        )
        .with_client_parametrized_history(
            &client_id,
            client_height,
            Some(ClientType::Tendermint), // The target host chain (B) is synthetic TM.
            Some(client_height),
        );

        let ctx_b = MockContext::new(
            ChainId::new("mockgaiaB".to_string(), 1),
            HostType::SyntheticTendermint,
            5,
            update_height,
        );

        let signer = get_dummy_account_id();

        let block_ref = ctx_b.host_block(update_height);
        let mut latest_header: AnyHeader = block_ref.cloned().map(Into::into).unwrap();

        let trusted_height = client_height.clone().sub(1).unwrap_or_default();

        latest_header = match latest_header {
            AnyHeader::Tendermint(mut theader) => {
                theader.trusted_height = trusted_height;
                AnyHeader::Tendermint(theader)
            }
            AnyHeader::Mock(m) => AnyHeader::Mock(m),
        };

        let msg = MsgUpdateAnyClient {
            client_id: client_id.clone(),
            header: latest_header,
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
                        assert!(!upd_res.client_state.is_frozen());
                        assert_eq!(upd_res.client_state.latest_height(), msg.header.height(),)
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
    fn test_update_syntetic_tendermint_client_duplicate_ok() {
        let client_id = ClientId::new(ClientType::Tendermint, 0).unwrap();
        let client_height = Height::new(1, 20);

        let chain_start_height = Height::new(1, 11);

        let ctx = MockContext::new(
            ChainId::new("mockgaiaA".to_string(), 1),
            HostType::Mock,
            5,
            chain_start_height,
        )
        .with_client_parametrized(
            &client_id,
            client_height,
            Some(ClientType::Tendermint), // The target host chain (B) is synthetic TM.
            Some(client_height),
        );

        let ctx_b = MockContext::new(
            ChainId::new("mockgaiaB".to_string(), 1),
            HostType::SyntheticTendermint,
            5,
            client_height,
        );

        let signer = get_dummy_account_id();

        let block_ref = ctx_b.host_block(client_height);
        let latest_header: AnyHeader = match block_ref.cloned().map(Into::into).unwrap() {
            AnyHeader::Tendermint(mut theader) => {
                let cons_state = ctx
                    .latest_consensus_states(&client_id, &client_height)
                    .clone();
                if let AnyConsensusState::Tendermint(tcs) = cons_state {
                    theader.signed_header.header.time = tcs.timestamp;
                }
                AnyHeader::Tendermint(theader)
            }
            AnyHeader::Mock(header) => AnyHeader::Mock(header),
        };

        let msg = MsgUpdateAnyClient {
            client_id: client_id.clone(),
            header: latest_header,
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
                        assert!(!upd_res.client_state.is_frozen());
                        assert_eq!(
                            upd_res.client_state,
                            ctx.latest_client_states(&client_id).clone()
                        );
                        assert_eq!(upd_res.client_state.latest_height(), msg.header.height(),)
                    }
                    _ => panic!("update handler result has incorrect type"),
                }
            }
            Err(_err) => {
                panic!("unexpected error");
            }
        }
    }

    #[test]
    fn test_update_syntetic_tendermint_client_duplicate_height_frozen() {
        let client_id = ClientId::new(ClientType::Tendermint, 0).unwrap();
        let client_height = Height::new(1, 20);

        let chain_start_height = Height::new(1, 11);

        let ctx = MockContext::new(
            ChainId::new("mockgaiaA".to_string(), 1),
            HostType::Mock,
            5,
            chain_start_height,
        )
        .with_client_parametrized(
            &client_id,
            client_height,
            Some(ClientType::Tendermint), // The target host chain (B) is synthetic TM.
            Some(client_height),
        );

        let signer = get_dummy_account_id();

        let block_ref = HostBlock::generate_block(
            ChainId::new("mockgaiaB".to_string(), 1),
            HostType::SyntheticTendermint,
            client_height.revision_height,
        );

        let latest_header: AnyHeader = Some(block_ref).map(Into::into).unwrap();

        let msg = MsgUpdateAnyClient {
            client_id: client_id.clone(),
            header: latest_header,
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
                        assert!(upd_res.client_state.is_frozen());
                        assert_ne!(
                            upd_res.client_state,
                            ctx.latest_client_states(&client_id).clone()
                        );
                        assert_eq!(upd_res.client_state.latest_height(), msg.header.height(),)
                    }
                    _ => panic!("update handler result has incorrect type"),
                }
            }
            Err(_err) => {
                panic!("unexpected error");
            }
        }
    }

    #[test]
    fn test_update_syntetic_tendermint_client_lower_height() {
        let client_id = ClientId::new(ClientType::Tendermint, 0).unwrap();
        let client_height = Height::new(1, 20);

        let client_update_height = Height::new(1, 19);

        let chain_start_height = Height::new(1, 11);

        let ctx = MockContext::new(
            ChainId::new("mockgaiaA".to_string(), 1),
            HostType::Mock,
            5,
            chain_start_height,
        )
        .with_client_parametrized(
            &client_id,
            client_height,
            Some(ClientType::Tendermint), // The target host chain (B) is synthetic TM.
            Some(client_height),
        );

        let ctx_b = MockContext::new(
            ChainId::new("mockgaiaB".to_string(), 1),
            HostType::SyntheticTendermint,
            5,
            client_height,
        );

        let signer = get_dummy_account_id();

        let block_ref = ctx_b.host_block(client_update_height);
        let latest_header: AnyHeader = block_ref.cloned().map(Into::into).unwrap();

        let msg = MsgUpdateAnyClient {
            client_id,
            header: latest_header,
            signer,
        };

        let output = dispatch(&ctx, ClientMsg::UpdateClient(msg));

        match output {
            Ok(_) => {
                panic!("update handler result has incorrect type");
            }
            Err(err) => {
                // assert_eq!(err, Error::header_verification_failure);
                println!("err is {}", err.to_string());
            }
        }
    }
}
