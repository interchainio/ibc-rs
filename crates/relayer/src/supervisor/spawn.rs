use tracing::{error, info};

use ibc_relayer_types::core::{
    ics03_connection::connection::IdentifiedConnectionEnd,
    ics04_channel::channel::State as ChannelState,
};

use crate::{
    chain::{counterparty::connection_state_on_destination, handle::ChainHandle},
    client_state::IdentifiedAnyClientState,
    config::Config,
    object::{Channel, Client, Connection, Object, Packet, Wallet},
    registry::Registry,
    supervisor::error::Error as SupervisorError,
    telemetry,
    worker::WorkerMap,
};

use super::{
    scan::{ChainScan, ChainsScan, ChannelScan, ClientScan, ConnectionScan},
    Error,
};

/// A context for spawning workers within the supervisor.
pub struct SpawnContext<'a, Chain: ChainHandle> {
    config: &'a Config,
    registry: &'a mut Registry<Chain>,
    workers: &'a mut WorkerMap,
}

impl<'a, Chain: ChainHandle> SpawnContext<'a, Chain> {
    pub fn new(
        config: &'a Config,
        registry: &'a mut Registry<Chain>,
        workers: &'a mut WorkerMap,
    ) -> Self {
        Self {
            config,
            registry,
            workers,
        }
    }

    pub fn spawn_workers(&mut self, scan: ChainsScan) {
        let _span = tracing::error_span!("spawn").entered();

        for chain_scan in scan.chains {
            match chain_scan {
                Ok(chain_scan) => self.spawn_workers_for_chain(chain_scan),
                Err(e) => error!("failed to spawn worker for a chain, reason: {}", e), // TODO: Show chain id
            }
        }
    }

    pub fn spawn_workers_for_chain(&mut self, scan: ChainScan) {
        let _span = tracing::error_span!("chain", chain = %scan.chain_id).entered();

        let chain = match self.registry.get_or_spawn(&scan.chain_id) {
            Ok(chain_handle) => chain_handle,
            Err(e) => {
                error!(
                    "skipping workers, reason: failed to spawn chain runtime with error: {}",
                    e
                );

                return;
            }
        };

        for (_, client_scan) in scan.clients {
            self.spawn_workers_for_client(chain.clone(), client_scan);
        }

        // Let's only spawn the wallet worker if telemetry is enabled,
        // otherwise the worker just ends up issuing queries to the node
        // without making anything of the result
        telemetry!(self.spawn_wallet_worker(chain));
    }

    pub fn spawn_wallet_worker(&mut self, chain: Chain) {
        let wallet_object = Object::Wallet(Wallet {
            chain_id: chain.id(),
        });

        self.workers
            .spawn(chain.clone(), chain, &wallet_object, self.config)
            .then(|| {
                info!("spawning Wallet worker: {}", wallet_object.short_name());
            });
    }

    pub fn spawn_workers_for_client(&mut self, chain: Chain, client_scan: ClientScan) {
        let _span = tracing::error_span!("client", client = %client_scan.id()).entered();

        for (_, connection_scan) in client_scan.connections {
            self.spawn_workers_for_connection(chain.clone(), &client_scan.client, connection_scan);
        }
    }

    pub fn spawn_workers_for_connection(
        &mut self,
        chain: Chain,
        client: &IdentifiedAnyClientState,
        connection_scan: ConnectionScan,
    ) {
        let _span =
            tracing::error_span!("connection", connection = %connection_scan.id()).entered();

        let connection_id = connection_scan.id().clone();

        match self.spawn_connection_workers(
            chain.clone(),
            client.clone(),
            connection_scan.connection,
        ) {
            Ok(true) => info!(
                chain = %chain.id(),
                connection = %connection_id,
                "done spawning connection workers",
            ),
            Ok(false) => info!(
                chain = %chain.id(),
                connection = %connection_id,
                "no connection workers were spawn",
            ),
            Err(e) => error!(
                chain = %chain.id(),
                connection = %connection_id,
                "skipped connection workers, reason: {}",
                e
            ),
        }

        for (channel_id, channel_scan) in connection_scan.channels {
            match self.spawn_workers_for_channel(chain.clone(), client, channel_scan) {
                Ok(true) => info!(
                    chain = %chain.id(),
                    channel = %channel_id,
                    "done spawning channel workers",
                ),
                Ok(false) => info!(
                    chain = %chain.id(),
                    channel = %channel_id,
                    "no channel workers were spawned",
                ),
                Err(e) => error!(
                    chain = %chain.id(),
                    channel = %channel_id,
                    "skipped channel workers, reason: {}",
                    e
                ),
            }
        }
    }

    fn spawn_connection_workers(
        &mut self,
        chain: Chain,
        client: IdentifiedAnyClientState,
        connection: IdentifiedConnectionEnd,
    ) -> Result<bool, Error> {
        let config_conn_enabled = self.config.mode.connections.enabled;

        let counterparty_chain = self
            .registry
            .get_or_spawn(&client.client_state.chain_id())
            .map_err(Error::spawn)?;

        let conn_state_src = connection.connection_end.state;
        let conn_state_dst = connection_state_on_destination(&connection, &counterparty_chain)?;

        info!(
            chain = %chain.id(),
            connection = %connection.connection_id,
            counterparty_chain = %counterparty_chain.id(),
            "connection is {}, state on destination chain is {}",
            conn_state_src,
            conn_state_dst
        );

        if conn_state_src.is_open() && conn_state_dst.is_open() {
            info!(
                chain = %chain.id(),
                connection = %connection.connection_id,
                "connection is already open, not spawning Connection worker",
            );

            Ok(false)
        } else if config_conn_enabled
            && !conn_state_dst.is_open()
            && conn_state_dst.less_or_equal_progress(conn_state_src)
        {
            // create worker for connection handshake that will advance the remote state
            let connection_object = Object::Connection(Connection {
                dst_chain_id: client.client_state.chain_id(),
                src_chain_id: chain.id(),
                src_connection_id: connection.connection_id,
            });

            self.workers
                .spawn(chain, counterparty_chain, &connection_object, self.config)
                .then(|| {
                    info!(
                        "spawning Connection worker: {}",
                        connection_object.short_name()
                    );
                });

            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Spawns all the [`WorkerHandle`](crate::worker::WorkerHandle)s that will
    /// handle a given channel for a given source chain.
    pub fn spawn_workers_for_channel(
        &mut self,
        chain: Chain,
        client: &IdentifiedAnyClientState,
        channel_scan: ChannelScan,
    ) -> Result<bool, Error> {
        let _span = tracing::error_span!("channel", channel = %channel_scan.id()).entered();

        let mode = &self.config.mode;

        let counterparty_chain = self
            .registry
            .get_or_spawn(&client.client_state.chain_id())
            .map_err(SupervisorError::spawn)?;

        let chan_state_src = channel_scan.channel.channel_end.state;
        let chan_state_dst = channel_scan
            .counterparty
            .as_ref()
            .map_or(ChannelState::Uninitialized, |c| c.channel_end.state);

        info!(
            chain = %chain.id(),
            counterparty_chain = %counterparty_chain.id(),
            channel = %channel_scan.id(),
            "channel is {}, state on destination chain is {}",
            chan_state_src,
            chan_state_dst
        );

        if (mode.clients.enabled || mode.packets.enabled)
            && chan_state_src.is_open()
            && (chan_state_dst.is_open() || chan_state_dst.is_closed())
        {
            if mode.clients.enabled {
                // Spawn the client worker
                let client_object = Object::Client(Client {
                    dst_client_id: client.client_id.clone(),
                    dst_chain_id: chain.id(),
                    src_chain_id: client.client_state.chain_id(),
                });

                self.workers
                    .spawn(
                        counterparty_chain.clone(),
                        chain.clone(),
                        &client_object,
                        self.config,
                    )
                    .then(|| info!("spawned client worker: {}", client_object.short_name()));
            }

            if mode.packets.enabled {
                let has_packets = || {
                    !channel_scan
                        .unreceived_packets_on_counterparty(&chain, &counterparty_chain)
                        .unwrap_or_default()
                        .is_empty()
                };

                let has_acks = || {
                    !channel_scan
                        .unreceived_acknowledgements_on_counterparty(&chain, &counterparty_chain)
                        .unwrap_or_default()
                        .is_empty()
                };

                // If there are any outstanding packets or acks to send, spawn the worker
                if has_packets() || has_acks() {
                    // Create the Packet object and spawn worker
                    let path_object = Object::Packet(Packet {
                        dst_chain_id: counterparty_chain.id(),
                        src_chain_id: chain.id(),
                        src_channel_id: channel_scan.channel.channel_id.clone(),
                        src_port_id: channel_scan.channel.port_id.clone(),
                    });

                    self.workers
                        .spawn(
                            chain.clone(),
                            counterparty_chain.clone(),
                            &path_object,
                            self.config,
                        )
                        .then(|| info!("spawned packet worker: {}", path_object.short_name()));
                }
            }

            Ok(mode.clients.enabled)
        } else if mode.channels.enabled {
            let has_packets = || {
                !channel_scan
                    .unreceived_packets_on_counterparty(&counterparty_chain, &chain)
                    .unwrap_or_default()
                    .is_empty()
            };

            // Determine if open handshake is required
            let open_handshake = chan_state_dst.less_or_equal_progress(ChannelState::TryOpen)
                && chan_state_dst.less_or_equal_progress(chan_state_src);

            // Determine if close handshake is required, i.e. if channel state on source is `Closed`,
            // and on destination channel state is not `Closed, and there are no pending packets.
            // If there are pending packets on destination then we let the packet worker clear the
            // packets and we do not finish the channel handshake.
            let close_handshake =
                chan_state_src.is_closed() && !chan_state_dst.is_closed() && !has_packets();

            if open_handshake || close_handshake {
                // create worker for channel handshake that will advance the counterparty state
                let channel_object = Object::Channel(Channel {
                    dst_chain_id: counterparty_chain.id(),
                    src_chain_id: chain.id(),
                    src_channel_id: channel_scan.channel.channel_id,
                    src_port_id: channel_scan.channel.port_id,
                });

                self.workers
                    .spawn(chain, counterparty_chain, &channel_object, self.config)
                    .then(|| info!("spawned channel worker: {}", channel_object.short_name()));

                Ok(true)
            } else {
                Ok(false)
            }
        } else {
            Ok(false)
        }
    }
}
