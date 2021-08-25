use alloc::collections::btree_map::BTreeMap as HashMap;

use crossbeam_channel::Sender;

use ibc::ics24_host::identifier::ChainId;
use tracing::{debug, trace, warn};

use crate::{
    chain::handle::{ChainHandle, ChainHandlePair},
    config::Config,
    object::Object,
    telemetry,
    telemetry::Telemetry,
};

use super::{Worker, WorkerHandle, WorkerId, WorkerMsg};

/// Manage the lifecycle of [`Worker`]s associated with [`Object`]s.
#[derive(Debug)]
pub struct WorkerMap {
    workers: HashMap<Object, WorkerHandle>,
    latest_worker_id: WorkerId,
    msg_tx: Sender<WorkerMsg>,
    telemetry: Telemetry,
}

impl WorkerMap {
    /// Create a new worker map, which will spawn workers with
    /// the given channel for sending messages back to the [`Supervisor`].
    pub fn new(msg_tx: Sender<WorkerMsg>, telemetry: Telemetry) -> Self {
        Self {
            workers: HashMap::new(),
            latest_worker_id: WorkerId::new(0),
            msg_tx,
            telemetry,
        }
    }

    /// Returns `true` if there is a spawned [`Worker`] associated with the given [`Object`].
    pub fn contains(&self, object: &Object) -> bool {
        self.workers.contains_key(object)
    }

    /// Remove the [`Worker`] associated with the given [`Object`] from
    /// the map and wait for its thread to terminate.
    pub fn remove_stopped(&mut self, id: WorkerId, object: Object) -> bool {
        match self.workers.remove(&object) {
            Some(handle) if handle.id() == id => {
                telemetry!(self.telemetry.worker(metric_type(&object), -1));

                let id = handle.id();

                trace!(
                    worker.id = %id, worker.object = %object.short_name(),
                    "waiting for worker loop to end"
                );

                let _ = handle.join();

                trace!(
                    worker.id = %id, worker.object = %object.short_name(),
                    "worker loop has ended"
                );

                true
            }
            Some(handle) => {
                debug!(
                    worker.object = %object.short_name(),
                    "ignoring attempt to remove worker with outdated id {} (current: {})",
                    id, handle.id()
                );

                self.workers.insert(object, handle);

                false
            }
            None => {
                debug!(
                    worker.object = %object.short_name(),
                    "ignoring attempt to remove unknown worker",
                );

                false
            }
        }
    }

    /// Returns all the [`Worker`] which are interested in new block events originating
    /// from the chain with the given [`ChainId`].
    /// See: [`Object::notify_new_block`]
    pub fn to_notify<'a>(
        &'a self,
        src_chain_id: &'a ChainId,
    ) -> impl Iterator<Item = &'a WorkerHandle> {
        self.workers.iter().filter_map(move |(o, w)| {
            if o.notify_new_block(src_chain_id) {
                Some(w)
            } else {
                None
            }
        })
    }

    /// Get a handle to the worker in charge of handling events associated
    /// with the given [`Object`].
    ///
    /// This function will spawn a new [`Worker`] if one does not exists already.
    pub fn get_or_spawn<Chain: ChainHandle + 'static>(
        &mut self,
        object: Object,
        src: Chain,
        dst: Chain,
        config: &Config,
    ) -> &WorkerHandle {
        if self.workers.contains_key(&object) {
            &self.workers[&object]
        } else {
            let worker = self.spawn_worker(src, dst, &object, config);
            self.workers.entry(object).or_insert(worker)
        }
    }

    /// Spawn a new [`Worker`], only if one does not exists already.
    ///
    /// Returns whether or not the worker was actually spawned.
    pub fn spawn<Chain: ChainHandle + 'static>(
        &mut self,
        src: Chain,
        dst: Chain,
        object: &Object,
        config: &Config,
    ) -> bool {
        if !self.workers.contains_key(object) {
            let worker = self.spawn_worker(src, dst, object, config);
            self.workers.entry(object.clone()).or_insert(worker);
            true
        } else {
            false
        }
    }

    /// Force spawn a worker for the given [`Object`].
    fn spawn_worker<Chain: ChainHandle + 'static>(
        &mut self,
        src: Chain,
        dst: Chain,
        object: &Object,
        config: &Config,
    ) -> WorkerHandle {
        telemetry!(self.telemetry.worker(metric_type(object), 1));

        Worker::spawn(
            ChainHandlePair { a: src, b: dst },
            self.next_worker_id(),
            object.clone(),
            self.msg_tx.clone(),
            self.telemetry.clone(),
            config,
        )
    }

    fn next_worker_id(&mut self) -> WorkerId {
        let id = self.latest_worker_id.next();
        self.latest_worker_id = id;
        id
    }

    /// List the [`Object`]s for which there is an associated worker
    /// for the given chain.
    pub fn objects_for_chain(&self, chain_id: &ChainId) -> Vec<Object> {
        self.workers
            .keys()
            .filter(|o| o.for_chain(chain_id))
            .cloned()
            .collect()
    }

    /// List the [`WorkerHandle`]s associated with the given chain.
    pub fn workers_for_chain(&self, chain_id: &ChainId) -> Vec<&WorkerHandle> {
        self.workers
            .iter()
            .filter_map(|(o, h)| o.for_chain(chain_id).then(|| h))
            .collect()
    }

    /// Shutdown the worker associated with the given [`Object`].
    pub fn shutdown_worker(&mut self, object: &Object) {
        if let Some(handle) = self.workers.remove(object) {
            telemetry!(self.telemetry.worker(metric_type(object), -1));

            match handle.shutdown() {
                Ok(()) => {
                    trace!(object = %object.short_name(), "waiting for worker to exit");
                    let _ = handle.join();
                }
                Err(e) => {
                    warn!(object = %object.short_name(), "a worker may have failed to shutdown properly: {}", e);
                }
            }
        }
    }

    /// Get an iterator over the worker map's objects.
    pub fn objects(&self) -> impl Iterator<Item = (WorkerId, &Object)> {
        self.workers
            .iter()
            .map(|(object, handle)| (handle.id(), object))
    }
}

#[cfg(feature = "telemetry")]
fn metric_type(o: &Object) -> ibc_telemetry::state::WorkerType {
    use ibc_telemetry::state::WorkerType::*;
    match o {
        Object::Client(_) => Client,
        Object::Connection(_) => Connection,
        Object::Channel(_) => Channel,
        Object::Packet(_) => Packet,
    }
}
