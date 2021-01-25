use crate::ics02_client::state::ClientState;
use crate::ics02_client::{client_def::AnyClient, client_def::ClientDef};
use crate::ics04_channel::channel::ChannelEnd;
use crate::ics04_channel::context::ChannelReader;
use crate::ics04_channel::error::{Error, Kind};
use crate::proofs::Proofs;

/// Entry point for verifying all proofs bundled in any ICS4 message.
pub fn verify_proofs(
    ctx: &dyn ChannelReader,
    channel_end: &ChannelEnd,
    expected_chan: &ChannelEnd,
    proofs: &Proofs,
) -> Result<(), Error> {
    let connection_end = match ctx.connection_end(&channel_end.connection_hops()[0].clone()) {
        Some(c) => c,
        None => {
            return Err(Kind::MissingConnection(channel_end.connection_hops()[0].clone()).into())
        }
    };

    let client = connection_end.client_id().clone();

    let port_id = channel_end.counterparty().port_id().clone();
    let chan_id = channel_end.counterparty().channel_id().unwrap().clone();


    let client_state = ctx
    .channel_client_state(&(port_id.clone(), chan_id.clone()))
    .ok_or(Kind::MissingClientState)?;

    // // Fetch the client state (IBC client on the local/host chain).
    // let client_state = ctx.channel_client_state(&(port_id.clone(), chan_id.clone()));

    // if client_state.is_none() {
    //     return Err(Kind::MissingClientState.context(client.to_string()).into());
    // }

    let client_st = client_state.ok_or(Kind::MissingClientState)?;

    // The client must not be frozen.
    if client_st.is_frozen() {
        return Err(Kind::FrozenClient.context(client.to_string()).into());
    }

    if ctx
        .channel_client_consensus_state(&(port_id, chan_id), proofs.height())
        .is_none()
    {
        return Err(Kind::MissingClientConsensusState
            .context(client.to_string())
            .into());
    }

    let client_def = AnyClient::from_client_type(client_st.client_type());

    // Verify the proof for the channel state against the expected channel end.
    // A counterparty channel id of None in not possible, and is checked by validate_basic in msg.
    Ok(client_def
        .verify_channel_state(
            &client_st,
            proofs.height(),
            connection_end.counterparty().prefix(),
            proofs.object_proof(),
            &channel_end.counterparty().port_id(),
            &channel_end.counterparty().channel_id().unwrap(),
            expected_chan,
        )
        .map_err(|_| Kind::InvalidProof)?)
}
