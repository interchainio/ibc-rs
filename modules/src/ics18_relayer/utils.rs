use crate::ics02_client::header::{AnyHeader, Header};
use crate::ics02_client::msgs::update_client::MsgUpdateAnyClient;
use crate::ics02_client::msgs::ClientMsg;
use crate::ics18_relayer::context::Ics18Context;
use crate::ics18_relayer::error::Error;
use crate::ics24_host::identifier::ClientId;

/// Builds a `ClientMsg::UpdateClient` for a client with id `client_id` running on the `dest`
/// context, assuming that the latest header on the source context is `src_header`.
pub fn build_client_update_datagram<Ctx>(
    dest: &Ctx,
    client_id: &ClientId,
    src_header: AnyHeader,
) -> Result<ClientMsg, Error>
where
    Ctx: Ics18Context,
{
    // Check if client for ibc0 on ibc1 has been updated to latest height:
    // - query client state on destination chain
    let dest_client_state = dest
        .query_client_full_state(client_id)
        .ok_or_else(|| Error::client_state_not_found(client_id.clone()))?;

    let dest_client_latest_height = dest_client_state.latest_height();

    if src_header.height() == dest_client_latest_height {
        return Err(Error::client_already_up_to_date(
            client_id.clone(),
            src_header.height(),
            dest_client_latest_height,
        ));
    };

    if dest_client_latest_height > src_header.height() {
        return Err(Error::client_at_higher_height(
            client_id.clone(),
            src_header.height(),
            dest_client_latest_height,
        ));
    };

    // Client on destination chain can be updated.
    Ok(ClientMsg::UpdateClient(MsgUpdateAnyClient {
        client_id: client_id.clone(),
        header: src_header,
        signer: dest.signer(),
    }))
}

#[cfg(test)]
mod tests {
    use crate::ics02_client::client_type::ClientType;
    use crate::ics02_client::header::Header;
    use crate::ics18_relayer::context::Ics18Context;
    use crate::ics18_relayer::utils::build_client_update_datagram;
    use crate::ics24_host::identifier::{ChainId, ClientId};
    use crate::ics26_routing::msgs::Ics26Envelope;
    use crate::mock::context::MockContext;
    use crate::mock::host::HostType;
    use crate::prelude::*;
    use crate::Height;
    use test_env_log::test;

    #[test]
    /// Serves to test both ICS 26 `dispatch` & `build_client_update_datagram` functions.
    /// Implements a "ping pong" of client update messages, so that two chains repeatedly
    /// process a client update message and update their height in succession.
    fn client_update_ping_pong() {
        let chain_a_start_height = Height::new(1, 11);
        let chain_b_start_height = Height::new(1, 20);
        let client_on_b_for_a_height = Height::new(1, 10); // Should be smaller than `chain_a_start_height`
        let client_on_a_for_b_height = Height::new(1, 20); // Should be smaller than `chain_b_start_height`
        let num_iterations = 4;

        let client_on_a_for_b = ClientId::new(ClientType::Tendermint, 0).unwrap();
        let client_on_b_for_a = ClientId::new(ClientType::Mock, 0).unwrap();

        // Create two mock contexts, one for each chain.
        let mut ctx_a = MockContext::new(
            ChainId::new("mockgaiaA".to_string(), 1),
            HostType::Mock,
            5,
            chain_a_start_height,
        )
        .with_client_parametrized(
            &client_on_a_for_b,
            client_on_a_for_b_height,
            Some(ClientType::Tendermint), // The target host chain (B) is synthetic TM.
            Some(client_on_a_for_b_height),
        );
        let mut ctx_b = MockContext::new(
            ChainId::new("mockgaiaB".to_string(), 1),
            HostType::SyntheticTendermint,
            5,
            chain_b_start_height,
        )
        .with_client_parametrized(
            &client_on_b_for_a,
            client_on_b_for_a_height,
            Some(ClientType::Mock), // The target host chain is mock.
            Some(client_on_b_for_a_height),
        );

        for _i in 0..num_iterations {
            // Update client on chain B to latest height of A.
            // - create the client update message with the latest header from A
            let a_latest_header = ctx_a.query_latest_header().unwrap();
            assert_eq!(
                a_latest_header.client_type(),
                ClientType::Mock,
                "Client type verification in header failed for context A (Mock); got {:?} but expected {:?}",
                a_latest_header.client_type(),
                ClientType::Mock
            );

            let client_msg_b_res =
                build_client_update_datagram(&ctx_b, &client_on_b_for_a, a_latest_header);

            assert!(
                client_msg_b_res.is_ok(),
                "create_client_update failed for context destination {:?}, error: {:?}",
                ctx_b,
                client_msg_b_res
            );

            let client_msg_b = client_msg_b_res.unwrap();

            // - send the message to B. We bypass ICS18 interface and call directly into
            // MockContext `recv` method (to avoid additional serialization steps).
            let dispatch_res_b = ctx_b.deliver(Ics26Envelope::Ics2Msg(client_msg_b));
            let validation_res = ctx_b.validate();
            assert!(
                validation_res.is_ok(),
                "context validation failed with error {:?} for context {:?}",
                validation_res,
                ctx_b
            );

            // Check if the update succeeded.
            assert!(
                dispatch_res_b.is_ok(),
                "Dispatch failed for host chain b with error: {:?}",
                dispatch_res_b
            );
            let client_height_b = ctx_b
                .query_client_full_state(&client_on_b_for_a)
                .unwrap()
                .latest_height();
            assert_eq!(client_height_b, ctx_a.query_latest_height());

            // Update client on chain B to latest height of B.
            // - create the client update message with the latest header from B
            let b_latest_header = ctx_b.query_latest_header().unwrap();
            assert_eq!(
                b_latest_header.client_type(),
                ClientType::Tendermint,
                "Client type verification in header failed for context B (TM); got {:?} but expected {:?}",
                b_latest_header.client_type(),
                ClientType::Tendermint
            );

            let client_msg_a_res =
                build_client_update_datagram(&ctx_a, &client_on_a_for_b, b_latest_header);

            assert!(
                client_msg_a_res.is_ok(),
                "create_client_update failed for context destination {:?}, error: {:?}",
                ctx_a,
                client_msg_a_res
            );

            let client_msg_a = client_msg_a_res.unwrap();

            // - send the message to A
            let dispatch_res_a = ctx_a.deliver(Ics26Envelope::Ics2Msg(client_msg_a));
            let validation_res = ctx_a.validate();
            assert!(
                validation_res.is_ok(),
                "context validation failed with error {:?} for context {:?}",
                validation_res,
                ctx_a
            );

            // Check if the update succeeded.
            assert!(
                dispatch_res_a.is_ok(),
                "Dispatch failed for host chain a with error: {:?}",
                dispatch_res_a
            );
            let client_height_a = ctx_a
                .query_client_full_state(&client_on_a_for_b)
                .unwrap()
                .latest_height();
            assert_eq!(client_height_a, ctx_b.query_latest_height());
        }
    }
}
