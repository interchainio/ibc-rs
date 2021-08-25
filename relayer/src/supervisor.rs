use alloc::collections::btree_map::BTreeMap as HashMap;
use alloc::sync::Arc;
use core::ops::Deref;
use core::time::Duration;
use std::sync::RwLock;

use crossbeam_channel::{Receiver, Sender};
use itertools::Itertools;
use tracing::{debug, error, info, trace, warn};

use ibc::{
    events::IbcEvent,
    ics24_host::identifier::{ChainId, ChannelId, PortId},
    Height,
};

use crate::{
    chain::handle::ChainHandle,
    config::{ChainConfig, Config},
    event,
    event::monitor::{Error as EventError, ErrorDetail as EventErrorDetail, EventBatch},
    object::Object,
    registry::Registry,
    rest,
    telemetry::Telemetry,
    util::try_recv_multiple,
    worker::{WorkerMap, WorkerMsg},
};

pub mod client_state_filter;
use client_state_filter::{FilterPolicy, Permission};

pub mod error;
pub use error::{Error, ErrorDetail};

pub mod dump_state;
use dump_state::SupervisorState;

pub mod spawn;
use spawn::SpawnContext;

pub mod cmd;
use cmd::{CmdEffect, ConfigUpdate, SupervisorCmd};

use self::spawn::SpawnMode;

type ArcBatch = Arc<event::monitor::Result<EventBatch>>;
type Subscription = Receiver<ArcBatch>;

pub type RwArc<T> = Arc<RwLock<T>>;

/// The supervisor listens for events on multiple pairs of chains,
/// and dispatches the events it receives to the appropriate
/// worker, based on the [`Object`] associated with each event.
pub struct Supervisor<Chain: ChainHandle> {
    config: RwArc<Config>,
    registry: Registry<Chain>,
    workers: WorkerMap,

    cmd_rx: Receiver<SupervisorCmd>,
    worker_msg_rx: Receiver<WorkerMsg>,
    rest_rx: Option<rest::Receiver>,
    client_state_filter: FilterPolicy,

    #[allow(dead_code)]
    telemetry: Telemetry,
}

impl<Chain: ChainHandle + 'static> Supervisor<Chain> {
    /// Create a [`Supervisor`] which will listen for events on all the chains in the [`Config`].
    pub fn new(
        config: RwArc<Config>,
        rest_rx: Option<rest::Receiver>,
        telemetry: Telemetry,
    ) -> (Self, Sender<SupervisorCmd>) {
        let registry = Registry::new(config.clone());
        let (worker_msg_tx, worker_msg_rx) = crossbeam_channel::unbounded();
        let workers = WorkerMap::new(worker_msg_tx, telemetry.clone());
        let client_state_filter = FilterPolicy::default();

        let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();

        let supervisor = Self {
            config,
            registry,
            workers,
            cmd_rx,
            worker_msg_rx,
            rest_rx,
            client_state_filter,
            telemetry,
        };

        (supervisor, cmd_tx)
    }

    /// Returns `true` if the relayer should filter based on
    /// client state attributes, e.g., trust threshold.
    /// Returns `false` otherwise.
    fn client_filter_enabled(&self) -> bool {
        // Currently just a wrapper over the global filter.
        self.config.read().expect("poisoned lock").global.filter
    }

    /// Returns `true` if the relayer should filter based on
    /// channel identifiers.
    /// Returns `false` otherwise.
    fn channel_filter_enabled(&self) -> bool {
        self.config.read().expect("poisoned lock").global.filter
    }

    fn relay_packets_on_channel(
        &self,
        chain_id: &ChainId,
        port_id: &PortId,
        channel_id: &ChannelId,
    ) -> bool {
        // If filtering is disabled, then relay all channels
        if !self.channel_filter_enabled() {
            return true;
        }

        self.config
            .read()
            .expect("poisoned lock")
            .packets_on_channel_allowed(chain_id, port_id, channel_id)
    }

    fn relay_on_object(&mut self, chain_id: &ChainId, object: &Object) -> bool {
        // No filter is enabled, bail fast.
        if !self.channel_filter_enabled() && !self.client_filter_enabled() {
            return true;
        }

        // First, apply the channel filter
        if let Object::Packet(u) = object {
            if !self.relay_packets_on_channel(chain_id, u.src_port_id(), u.src_channel_id()) {
                return false;
            }
        }

        // Second, apply the client filter
        let client_filter_outcome = match object {
            Object::Client(client) => self
                .client_state_filter
                .control_client_object(&mut self.registry, client),
            Object::Connection(conn) => self
                .client_state_filter
                .control_conn_object(&mut self.registry, conn),
            Object::Channel(chan) => self
                .client_state_filter
                .control_chan_object(&mut self.registry, chan),
            Object::Packet(u) => self
                .client_state_filter
                .control_packet_object(&mut self.registry, u),
        };

        match client_filter_outcome {
            Ok(Permission::Allow) => true,
            Ok(Permission::Deny) => {
                warn!(
                    "client filter denies relaying on object {}",
                    object.short_name()
                );

                false
            }
            Err(e) => {
                warn!(
                    "denying relaying on object {}, caused by: {}",
                    object.short_name(),
                    e
                );

                false
            }
        }
    }

    /// Collect the events we are interested in from an [`EventBatch`],
    /// and maps each [`IbcEvent`] to their corresponding [`Object`].
    pub fn collect_events(
        &self,
        src_chain: &impl ChainHandle,
        batch: &EventBatch,
    ) -> CollectedEvents {
        let mut collected = CollectedEvents::new(batch.height, batch.chain_id.clone());

        let handshake_enabled = self
            .config
            .read()
            .expect("poisoned lock")
            .handshake_enabled();

        for event in &batch.events {
            match event {
                IbcEvent::NewBlock(_) => {
                    collected.new_block = Some(event.clone());
                }
                IbcEvent::UpdateClient(ref update) => {
                    if let Ok(object) = Object::for_update_client(update, src_chain) {
                        // Collect update client events only if the worker exists
                        if self.workers.contains(&object) {
                            collected
                                .per_object
                                .entry(object)
                                .or_default()
                                .push(event.clone());
                        }
                    }
                }
                IbcEvent::OpenInitConnection(..)
                | IbcEvent::OpenTryConnection(..)
                | IbcEvent::OpenAckConnection(..) => {
                    if !handshake_enabled {
                        continue;
                    }

                    let object = event
                        .connection_attributes()
                        .map(|attr| Object::connection_from_conn_open_events(attr, src_chain));

                    if let Some(Ok(object)) = object {
                        collected
                            .per_object
                            .entry(object)
                            .or_default()
                            .push(event.clone());
                    }
                }
                IbcEvent::OpenInitChannel(..) | IbcEvent::OpenTryChannel(..) => {
                    if !handshake_enabled {
                        continue;
                    }

                    let object = event
                        .channel_attributes()
                        .map(|attr| Object::channel_from_chan_open_events(attr, src_chain));

                    if let Some(Ok(object)) = object {
                        collected
                            .per_object
                            .entry(object)
                            .or_default()
                            .push(event.clone());
                    }
                }
                IbcEvent::OpenAckChannel(ref open_ack) => {
                    // Create client and packet workers here as channel end must be opened
                    if let Ok(client_object) =
                        Object::client_from_chan_open_events(open_ack.attributes(), src_chain)
                    {
                        collected
                            .per_object
                            .entry(client_object)
                            .or_default()
                            .push(event.clone());
                    }

                    if let Ok(packet_object) =
                        Object::packet_from_chan_open_events(open_ack.attributes(), src_chain)
                    {
                        collected
                            .per_object
                            .entry(packet_object)
                            .or_default()
                            .push(event.clone());
                    }

                    // If handshake message relaying is enabled create worker to send the MsgChannelOpenConfirm message
                    if handshake_enabled {
                        if let Ok(channel_object) =
                            Object::channel_from_chan_open_events(open_ack.attributes(), src_chain)
                        {
                            collected
                                .per_object
                                .entry(channel_object)
                                .or_default()
                                .push(event.clone());
                        }
                    }
                }
                IbcEvent::OpenConfirmChannel(ref open_confirm) => {
                    // Create client worker here as channel end must be opened
                    if let Ok(client_object) =
                        Object::client_from_chan_open_events(open_confirm.attributes(), src_chain)
                    {
                        collected
                            .per_object
                            .entry(client_object)
                            .or_default()
                            .push(event.clone());
                    }
                    if let Ok(packet_object) =
                        Object::packet_from_chan_open_events(open_confirm.attributes(), src_chain)
                    {
                        collected
                            .per_object
                            .entry(packet_object)
                            .or_default()
                            .push(event.clone());
                    }
                }
                IbcEvent::SendPacket(ref packet) => {
                    if let Ok(object) = Object::for_send_packet(packet, src_chain) {
                        collected
                            .per_object
                            .entry(object)
                            .or_default()
                            .push(event.clone());
                    }
                }
                IbcEvent::TimeoutPacket(ref packet) => {
                    if let Ok(object) = Object::for_timeout_packet(packet, src_chain) {
                        collected
                            .per_object
                            .entry(object)
                            .or_default()
                            .push(event.clone());
                    }
                }
                IbcEvent::WriteAcknowledgement(ref packet) => {
                    if let Ok(object) = Object::for_write_ack(packet, src_chain) {
                        collected
                            .per_object
                            .entry(object)
                            .or_default()
                            .push(event.clone());
                    }
                }
                IbcEvent::CloseInitChannel(ref packet) => {
                    if let Ok(object) = Object::for_close_init_channel(packet, src_chain) {
                        collected
                            .per_object
                            .entry(object)
                            .or_default()
                            .push(event.clone());
                    }
                }
                _ => (),
            }
        }

        collected
    }

    /// Create a new `SpawnContext` for spawning workers.
    fn spawn_context(&mut self, mode: SpawnMode) -> SpawnContext<'_, Chain> {
        SpawnContext::new(
            &self.config,
            &mut self.registry,
            &mut self.client_state_filter,
            &mut self.workers,
            mode,
        )
    }

    /// Spawn all the workers necessary for the relayer to connect
    /// and relay between all the chains in the configurations.
    fn spawn_workers(&mut self, mode: SpawnMode) {
        self.spawn_context(mode).spawn_workers();
    }

    /// Run the supervisor event loop.
    pub fn run(mut self) -> Result<(), Error> {
        self.spawn_workers(SpawnMode::Startup);

        let mut subscriptions = self.init_subscriptions()?;

        loop {
            if let Some((chain, batch)) = try_recv_multiple(&subscriptions) {
                self.handle_batch(chain.clone(), batch);
            }

            if let Ok(msg) = self.worker_msg_rx.try_recv() {
                self.handle_worker_msg(msg);
            }

            if let Ok(cmd) = self.cmd_rx.try_recv() {
                let after = self.handle_cmd(cmd);

                if let CmdEffect::ConfigChanged = after {
                    match self.init_subscriptions() {
                        Ok(subs) => {
                            subscriptions = subs;
                        }
                        Err(Error(ErrorDetail::NoChainsAvailable(_), _)) => (),
                        Err(e) => return Err(e),
                    }
                }
            }

            // Process incoming requests from the REST server
            self.handle_rest_requests();

            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Subscribe to the events emitted by the chains the supervisor is connected to.
    fn init_subscriptions(&mut self) -> Result<Vec<(Chain, Subscription)>, Error> {
        let chains = &self.config.read().expect("poisoned lock").chains;

        let mut subscriptions = Vec::with_capacity(chains.len());

        for chain_config in chains {
            let chain = match self.registry.get_or_spawn(&chain_config.id) {
                Ok(chain) => chain,
                Err(e) => {
                    error!(
                        "failed to spawn chain runtime for {}: {}",
                        chain_config.id, e
                    );
                    continue;
                }
            };

            match chain.subscribe() {
                Ok(subscription) => subscriptions.push((chain, subscription)),
                Err(e) => error!(
                    "failed to subscribe to events of {}: {}",
                    chain_config.id, e
                ),
            }
        }

        // At least one chain runtime should be available, otherwise the supervisor
        // cannot do anything and will hang indefinitely.
        if self.registry.size() == 0 {
            return Err(Error::no_chains_available());
        }

        Ok(subscriptions)
    }

    /// Handle the given [`SupervisorCmd`].
    ///
    /// Returns an [`CmdEffect`] which instructs the caller as to
    /// whether or not the event subscriptions needs to be reset or not.
    fn handle_cmd(&mut self, cmd: SupervisorCmd) -> CmdEffect {
        match cmd {
            SupervisorCmd::UpdateConfig(update) => self.update_config(update),
            SupervisorCmd::DumpState(reply_to) => self.dump_state(reply_to),
        }
    }

    /// Dump the state of the supervisor into a [`SupervisorState`] value,
    /// and send it back through the given channel.
    fn dump_state(&self, reply_to: Sender<SupervisorState>) -> CmdEffect {
        let state = self.state();
        let _ = reply_to.try_send(state);

        CmdEffect::Nothing
    }

    /// Returns a representation of the supervisor's internal state
    /// as a [`SupervisorState`].
    fn state(&self) -> SupervisorState {
        let chains = self.registry.chains().map(|c| c.id()).collect_vec();
        SupervisorState::new(chains, self.workers.objects())
    }

    /// Apply the given configuration update.
    ///
    /// Returns an [`CmdEffect`] which instructs the caller as to
    /// whether or not the event subscriptions needs to be reset or not.
    fn update_config(&mut self, update: ConfigUpdate) -> CmdEffect {
        match update {
            ConfigUpdate::Add(config) => self.add_chain(config),
            ConfigUpdate::Remove(id) => self.remove_chain(&id),
            ConfigUpdate::Update(config) => self.update_chain(config),
        }
    }

    /// Add the given chain to the configuration and spawn the associated workers.
    /// Will not have any effect if the chain is already present in the config.
    ///
    /// If the addition had any effect, returns [`CmdEffect::ConfigChanged`] as
    /// subscriptions need to be reset to take into account the newly added chain.
    fn add_chain(&mut self, config: ChainConfig) -> CmdEffect {
        let id = config.id.clone();

        if self.config.read().expect("poisoned lock").has_chain(&id) {
            info!(chain.id=%id, "skipping addition of already existing chain");
            return CmdEffect::Nothing;
        }

        info!(chain.id=%id, "adding new chain");

        self.config
            .write()
            .expect("poisoned lock")
            .chains
            .push(config);

        debug!(chain.id=%id, "spawning chain runtime");

        if let Err(e) = self.registry.spawn(&id) {
            error!(
                "failed to add chain {} because of failure to spawn the chain runtime: {}",
                id, e
            );

            // Remove the newly added config
            self.config
                .write()
                .expect("poisoned lock")
                .chains
                .retain(|c| c.id != id);

            return CmdEffect::Nothing;
        }

        debug!(chain.id=%id, "spawning workers");
        let mut ctx = self.spawn_context(SpawnMode::Reload);
        ctx.spawn_workers_for_chain(&id);

        CmdEffect::ConfigChanged
    }

    /// Remove the given chain to the configuration and spawn the associated workers.
    /// Will not have any effect if the chain was not already present in the config.
    ///
    /// If the removal had any effect, returns [`CmdEffect::ConfigChanged`] as
    /// subscriptions need to be reset to take into account the newly added chain.
    fn remove_chain(&mut self, id: &ChainId) -> CmdEffect {
        if !self.config.read().expect("poisoned lock").has_chain(id) {
            info!(chain.id=%id, "skipping removal of non-existing chain");
            return CmdEffect::Nothing;
        }

        info!(chain.id=%id, "removing existing chain");

        self.config
            .write()
            .expect("poisoned lock")
            .chains
            .retain(|c| &c.id != id);

        debug!(chain.id=%id, "shutting down workers");
        let mut ctx = self.spawn_context(SpawnMode::Reload);
        ctx.shutdown_workers_for_chain(id);

        debug!(chain.id=%id, "shutting down chain runtime");
        self.registry.shutdown(id);

        CmdEffect::ConfigChanged
    }

    /// Update the given chain configuration, by removing it with
    /// [`Supervisor::remove_chain`] and adding the updated
    /// chain config with [`Supervisor::remove_chain`].
    ///
    /// If the update had any effect, returns [`CmdEffect::ConfigChanged`] as
    /// subscriptions need to be reset to take into account the newly added chain.
    fn update_chain(&mut self, config: ChainConfig) -> CmdEffect {
        info!(chain.id=%config.id, "updating existing chain");

        let removed = self.remove_chain(&config.id);
        let added = self.add_chain(config);
        removed.or(added)
    }

    /// Process the given [`WorkerMsg`] sent by a worker.
    fn handle_worker_msg(&mut self, msg: WorkerMsg) {
        match msg {
            WorkerMsg::Stopped(id, object) => {
                self.workers.remove_stopped(id, object);
            }
        }
    }

    fn handle_rest_requests(&self) {
        if let Some(rest_rx) = &self.rest_rx {
            let config = self.config.read().expect("poisoned lock");
            if let Some(cmd) = rest::process_incoming_requests(&config, rest_rx) {
                self.handle_rest_cmd(cmd);
            }
        }
    }

    fn handle_rest_cmd(&self, m: rest::Command) {
        match m {
            rest::Command::DumpState(reply) => {
                let state = self.state();
                reply.send(Ok(state)).unwrap_or_else(|e| {
                    error!("[rest/supervisor] error replying to a REST request {}", e)
                });
            }
        }
    }

    /// Process the given batch if it does not contain any errors,
    /// output the errors on the console otherwise.
    fn handle_batch(&mut self, chain: Chain, batch: ArcBatch) {
        let chain_id = chain.id();

        match batch.deref() {
            Ok(batch) => {
                let _ = self
                    .process_batch(chain, batch)
                    .map_err(|e| error!("[{}] error during batch processing: {}", chain_id, e));
            }
            Err(EventError(EventErrorDetail::SubscriptionCancelled(_), _)) => {
                warn!(chain.id = %chain_id, "event subscription was cancelled, clearing pending packets");

                let _ = self.clear_pending_packets(&chain_id).map_err(|e| {
                    error!(
                        "[{}] error during clearing pending packets: {}",
                        chain_id, e
                    )
                });
            }
            Err(e) => {
                error!("[{}] error in receiving event batch: {}", chain_id, e)
            }
        }
    }

    /// Process a batch of events received from a chain.
    fn process_batch(&mut self, src_chain: Chain, batch: &EventBatch) -> Result<(), Error> {
        assert_eq!(src_chain.id(), batch.chain_id);

        let height = batch.height;
        let chain_id = batch.chain_id.clone();

        let collected = self.collect_events(&src_chain, batch);

        for (object, events) in collected.per_object.into_iter() {
            if !self.relay_on_object(&src_chain.id(), &object) {
                trace!(
                    "skipping events for '{}'. \
                    reason: filtering is enabled and channel does not match any allowed channels",
                    object.short_name()
                );

                continue;
            }

            if events.is_empty() {
                continue;
            }

            let src = self
                .registry
                .get_or_spawn(object.src_chain_id())
                .map_err(Error::spawn)?;

            let dst = self
                .registry
                .get_or_spawn(object.dst_chain_id())
                .map_err(Error::spawn)?;

            let worker = {
                let config = self.config.read().expect("poisoned lock");
                self.workers.get_or_spawn(object, src, dst, &config)
            };

            worker
                .send_events(height, events, chain_id.clone())
                .map_err(Error::worker)?
        }

        // If there is a NewBlock event, forward the event to any workers affected by it.
        if let Some(IbcEvent::NewBlock(new_block)) = collected.new_block {
            for worker in self.workers.to_notify(&src_chain.id()) {
                worker
                    .send_new_block(height, new_block)
                    .map_err(Error::worker)?
            }
        }

        Ok(())
    }

    fn clear_pending_packets(&mut self, chain_id: &ChainId) -> Result<(), Error> {
        for worker in self.workers.workers_for_chain(chain_id) {
            worker.clear_pending_packets().map_err(Error::worker)?;
        }

        Ok(())
    }
}

/// Describes the result of [`collect_events`].
#[derive(Clone, Debug)]
pub struct CollectedEvents {
    /// The height at which these events were emitted from the chain.
    pub height: Height,
    /// The chain from which the events were emitted.
    pub chain_id: ChainId,
    /// [`NewBlock`] event collected from the [`EventBatch`].
    pub new_block: Option<IbcEvent>,
    /// Mapping between [`Object`]s and their associated [`IbcEvent`]s.
    pub per_object: HashMap<Object, Vec<IbcEvent>>,
}

impl CollectedEvents {
    pub fn new(height: Height, chain_id: ChainId) -> Self {
        Self {
            height,
            chain_id,
            new_block: Default::default(),
            per_object: Default::default(),
        }
    }

    /// Whether the collected events include a [`NewBlock`] event.
    pub fn has_new_block(&self) -> bool {
        self.new_block.is_some()
    }
}
