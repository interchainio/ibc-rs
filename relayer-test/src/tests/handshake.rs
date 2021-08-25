use eyre::{eyre, Result};
use tracing::info;

use ibc::events::IbcEvent;
use ibc::ics02_client::client_state::AnyClientState;
use ibc::ics04_channel::channel::Order;
use ibc::ics07_tendermint::client_state::ClientState;
use ibc::ics24_host::identifier::{ChainId, ChannelId, ClientId, ConnectionId, PortId};
use ibc::timestamp::ZERO_DURATION;
use ibc::Height;
use ibc_relayer::chain::handle::ChainHandle;
use ibc_relayer::channel::{Channel, ChannelSide};
use ibc_relayer::config::Config;
use ibc_relayer::connection::{Connection, ConnectionSide};
use ibc_relayer::foreign_client::ForeignClient;
use ibc_relayer_cli::cli_utils::spawn_chain_runtime;

fn create_client(
    source_chain: Box<dyn ChainHandle>,
    target_chain: Box<dyn ChainHandle>,
) -> Result<ClientId> {
    let foreign_client = ForeignClient::restore(
        ClientId::default(),
        target_chain.clone(),
        source_chain.clone(),
    );

    let event = foreign_client.build_create_client_and_send()?;
    let create_event = match event {
        IbcEvent::CreateClient(ev) => Ok(ev),
        _ => Err(eyre!("expected CreateClient event, instead got {}", event)),
    }?;

    Ok(create_event.client_id().clone())
}

fn query_client_state(
    target_chain: Box<dyn ChainHandle>,
    client_id: &ClientId,
) -> Result<ClientState> {
    let any_client_state = target_chain.query_client_state(client_id, Height::zero())?;

    match any_client_state {
        AnyClientState::Tendermint(s) => Ok(s),
        _ => Err(eyre!(
            "expected Tendermint client state, instead got {:?}",
            any_client_state
        )),
    }
}

fn update_client_state(
    source_chain: Box<dyn ChainHandle>,
    target_chain: Box<dyn ChainHandle>,
    source_on_target_client_id: &ClientId,
) -> Result<Vec<IbcEvent>> {
    let foreign_client =
        ForeignClient::find(source_chain, target_chain, source_on_target_client_id)?;

    let events = foreign_client.build_update_client_and_send(Height::zero(), Height::zero())?;

    Ok(events)
}

fn create_client_pair(
    chain_a: Box<dyn ChainHandle>,
    chain_b: Box<dyn ChainHandle>,
) -> Result<(ClientId, ClientId)> {
    let a_on_b_client_id = create_client(chain_a.clone(), chain_b.clone())?;

    info!(
        "created client for chain A on chain B: {}",
        a_on_b_client_id
    );

    let b_on_a_client_id = create_client(chain_b.clone(), chain_a.clone())?;

    info!(
        "created client for chain B on chain A: {}",
        b_on_a_client_id
    );

    {
        let a_on_b_client_state = query_client_state(chain_b.clone(), &a_on_b_client_id)?;

        info!(
            "client state of chain A on chain B: {:?}",
            a_on_b_client_state
        );

        let b_on_a_client_state = query_client_state(chain_a.clone(), &b_on_a_client_id)?;

        info!(
            "client state of chain B on chain A: {:?}",
            b_on_a_client_state
        );
    }

    let update_a_on_b_events =
        update_client_state(chain_a.clone(), chain_b.clone(), &a_on_b_client_id)?;

    info!(
        "result of updating client for chain A on chain B: {:?}",
        update_a_on_b_events
    );

    let update_b_on_a_events =
        update_client_state(chain_b.clone(), chain_a.clone(), &b_on_a_client_id)?;

    info!(
        "result of updating client for chain A on chain B: {:?}",
        update_b_on_a_events
    );

    {
        let a_on_b_client_state = query_client_state(chain_b.clone(), &a_on_b_client_id)?;

        info!(
            "client state of chain A on chain B: {:?}",
            a_on_b_client_state
        );

        let b_on_a_client_state = query_client_state(chain_a.clone(), &b_on_a_client_id)?;

        info!(
            "client state of chain B on chain A: {:?}",
            b_on_a_client_state
        );
    }

    Ok((a_on_b_client_id, b_on_a_client_id))
}

fn connection_init(
    chain_a: Box<dyn ChainHandle>,
    chain_b: Box<dyn ChainHandle>,
    a_on_b_client_id: ClientId,
    b_on_a_client_id: ClientId,
) -> Result<ConnectionId> {
    let connection = Connection {
        delay_period: ZERO_DURATION,
        a_side: ConnectionSide::new(chain_a, b_on_a_client_id.clone(), None),
        b_side: ConnectionSide::new(chain_b, a_on_b_client_id.clone(), None),
    };

    let event = connection.build_conn_init_and_send()?;

    let connection_id = match event {
        IbcEvent::OpenInitConnection(e) => e
            .connection_id()
            .clone()
            .ok_or_else(|| eyre!("Expected connection_id to be present in event"))?,
        _ => {
            return Err(eyre!(
                "Expected OpenInitConnection event, instead got {}",
                event
            ));
        }
    };

    Ok(connection_id)
}

fn connection_try(
    chain_a: Box<dyn ChainHandle>,
    chain_b: Box<dyn ChainHandle>,
    a_on_b_client_id: ClientId,
    b_on_a_client_id: ClientId,
    connection_id_on_b: ConnectionId,
) -> Result<ConnectionId> {
    let connection = Connection {
        delay_period: ZERO_DURATION,
        a_side: ConnectionSide::new(chain_a, b_on_a_client_id.clone(), Some(connection_id_on_b)),
        b_side: ConnectionSide::new(chain_b, a_on_b_client_id.clone(), None),
    };

    let event = connection.build_conn_try_and_send()?;

    let connection_id = match event {
        IbcEvent::OpenTryConnection(e) => e
            .connection_id()
            .clone()
            .ok_or_else(|| eyre!("Expected connection_id to be present in event"))?,
        _ => {
            return Err(eyre!(
                "Expected OpenTryConnection event, instead got {}",
                event
            ));
        }
    };

    Ok(connection_id)
}

fn connection_ack(
    chain_a: Box<dyn ChainHandle>,
    chain_b: Box<dyn ChainHandle>,
    b_on_a_client_id: ClientId,
    a_on_b_client_id: ClientId,
    connection_id_on_a: ConnectionId,
    connection_id_on_b: ConnectionId,
) -> Result<ConnectionId> {
    let connection = Connection {
        delay_period: ZERO_DURATION,
        a_side: ConnectionSide::new(
            chain_a,
            b_on_a_client_id.clone(),
            Some(connection_id_on_a.clone()),
        ),
        b_side: ConnectionSide::new(chain_b, a_on_b_client_id.clone(), Some(connection_id_on_b)),
    };

    let event = connection.build_conn_ack_and_send()?;

    let connection_id = match event {
        IbcEvent::OpenAckConnection(e) => e
            .connection_id()
            .clone()
            .ok_or_else(|| eyre!("Expected connection_id to be present in event"))?,
        _ => {
            return Err(eyre!(
                "Expected OpenAckConnection event, instead got {}",
                event
            ));
        }
    };

    if connection_id != connection_id_on_a {
        return Err(eyre!(
            "Expected result OpenAckConnection contains connection id on A ({}), instead got {}",
            connection_id_on_a,
            connection_id
        )
        .into());
    }

    Ok(connection_id)
}

fn channel_open_init(
    chain_a: Box<dyn ChainHandle>,
    chain_b: Box<dyn ChainHandle>,
    a_on_b_client_id: ClientId,
    connection_id_on_b: ConnectionId,
    port_id: PortId,
) -> Result<ChannelId> {
    let channel = Channel {
        connection_delay: Default::default(),
        ordering: Order::Unordered,
        a_side: ChannelSide::new(
            chain_a,
            ClientId::default(),
            ConnectionId::default(),
            port_id.clone(),
            None,
        ),
        b_side: ChannelSide::new(
            chain_b,
            a_on_b_client_id.clone(),
            connection_id_on_b.clone(),
            port_id.clone(),
            None,
        ),
        version: None,
    };

    let event = channel.build_chan_open_init_and_send()?;

    let channel_id = match event {
        IbcEvent::OpenInitChannel(e) => e.channel_id().map(|id| id.clone()).ok_or(eyre!(
            "Expected OpenInitChannel event to contain channel id"
        ))?,
        _ => {
            return Err(eyre!(
                "Expected OpenInitChannel event, instead got {}",
                event
            ));
        }
    };

    Ok(channel_id)
}

fn raw_connection_handshake(
    chain_a: Box<dyn ChainHandle>,
    chain_b: Box<dyn ChainHandle>,
    a_on_b_client_id: ClientId,
    b_on_a_client_id: ClientId,
) -> Result<(ConnectionId, ConnectionId)> {
    let connection_id_on_b = connection_init(
        chain_a.clone(),
        chain_b.clone(),
        a_on_b_client_id.clone(),
        b_on_a_client_id.clone(),
    )?;

    let connection_id_on_a = connection_try(
        chain_b.clone(),
        chain_a.clone(),
        a_on_b_client_id.clone(),
        b_on_a_client_id.clone(),
        connection_id_on_b.clone(),
    )?;

    connection_ack(
        chain_a.clone(),
        chain_b.clone(),
        b_on_a_client_id.clone(),
        a_on_b_client_id.clone(),
        connection_id_on_a.clone(),
        connection_id_on_b.clone(),
    )?;

    Ok((connection_id_on_a, connection_id_on_b))
}

pub fn test_connection_handshake(
    config: &Config,
    chain_id_a: ChainId,
    chain_id_b: ChainId,
) -> Result<()> {
    let chain_a = spawn_chain_runtime(config, &chain_id_a)?;
    let chain_b = spawn_chain_runtime(config, &chain_id_b)?;

    let (a_on_b_client_id, b_on_a_client_id) =
        create_client_pair(chain_a.clone(), chain_b.clone())?;

    let (connection_id_a, connection_id_b) = raw_connection_handshake(
        chain_a.clone(),
        chain_b.clone(),
        a_on_b_client_id.clone(),
        b_on_a_client_id.clone(),
    )?;

    Ok(())
}

pub fn test_connection_and_channel_handshake(
    config: &Config,
    chain_id_a: ChainId,
    chain_id_b: ChainId,
    port_id: PortId,
) -> Result<()> {
    info!(
        "initiating raw channel handshake between chain A (id: {}) and chain B (id: {})",
        chain_id_a, chain_id_b
    );

    let chain_a = spawn_chain_runtime(config, &chain_id_a)?;
    let chain_b = spawn_chain_runtime(config, &chain_id_b)?;

    let (a_on_b_client_id, b_on_a_client_id) =
        create_client_pair(chain_a.clone(), chain_b.clone())?;

    let connection_id_on_b = connection_init(
        chain_a.clone(),
        chain_b.clone(),
        a_on_b_client_id.clone(),
        b_on_a_client_id.clone(),
    )?;

    let channel_id_on_b = channel_open_init(
        chain_a.clone(),
        chain_b.clone(),
        a_on_b_client_id.clone(),
        connection_id_on_b.clone(),
        port_id,
    )?;

    // TODO: more to come..

    Ok(())
}
