//! Registry for keeping track of [`ChainHandle`]s indexed by a `ChainId`.

use alloc::collections::btree_map::BTreeMap as HashMap;
use alloc::sync::Arc;
use once_cell::sync::OnceCell;
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use tokio::runtime::Runtime as TokioRuntime;
use tracing::{trace, warn};

use ibc_relayer_types::core::ics24_host::identifier::ChainId;

use crate::chain::handle::DefaultChainHandle;
use crate::spawn::spawn_chain_runtime_with_config;
use crate::{
    chain::handle::ChainHandle,
    config::Config,
    spawn::{spawn_chain_runtime, SpawnError},
    util::lock::RwArc,
};

/// Registry for keeping track of [`ChainHandle`]s indexed by a `ChainId`.
///
/// The purpose of this type is to avoid spawning multiple runtimes for a single `ChainId`.
#[derive(Debug)]
pub struct Registry<Chain: ChainHandle> {
    config: Config,
    handles: HashMap<ChainId, Chain>,
    rt: Arc<TokioRuntime>,
}

impl<Chain: ChainHandle> Registry<Chain> {
    /// Construct a new [`Registry`] using the provided [`Config`]
    pub fn new(config: Config) -> Self {
        Self {
            config,
            handles: HashMap::new(),
            rt: Arc::new(TokioRuntime::new().unwrap()),
        }
    }

    /// Return the size of the registry, i.e., the number of distinct chain runtimes.
    pub fn size(&self) -> usize {
        self.handles.len()
    }

    /// Return an iterator overall the chain handles managed by the registry.
    pub fn chains(&self) -> impl Iterator<Item = &Chain> {
        self.handles.values()
    }

    /// Get the [`ChainHandle`] associated with the given [`ChainId`].
    ///
    /// If there is no handle yet, this will first spawn the runtime and then
    /// return its handle.
    pub fn get_or_spawn(&mut self, chain_id: &ChainId) -> Result<Chain, SpawnError> {
        self.spawn(chain_id)?;

        let handle = self
            .handles
            .get(chain_id)
            .expect("runtime was just spawned");

        Ok(handle.clone())
    }

    /// Spawn a chain runtime for the chain with the given [`ChainId`],
    /// only if the registry does not contain a handle for that runtime already.
    ///
    /// Returns whether or not the runtime was actually spawned.
    pub fn spawn(&mut self, chain_id: &ChainId) -> Result<bool, SpawnError> {
        if !self.handles.contains_key(chain_id) {
            let handle = spawn_chain_runtime(&self.config, chain_id, self.rt.clone())?;
            self.handles.insert(chain_id.clone(), handle);
            trace!(chain = %chain_id, "spawned chain runtime");
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Shutdown the runtime associated with the given chain identifier.
    pub fn shutdown(&mut self, chain_id: &ChainId) {
        if let Some(handle) = self.handles.remove(chain_id) {
            if let Err(e) = handle.shutdown() {
                warn!(chain = %chain_id, "chain runtime might have failed to shutdown properly: {}", e);
            }
        }
    }
}

static GLOBAL_REGISTRY: OnceCell<SharedRegistry> = OnceCell::new();

pub fn set_global_registry(registry: SharedRegistry) {
    if GLOBAL_REGISTRY.set(registry).is_err() {
        panic!("global registry already set");
    }
}

pub fn get_global_registry() -> SharedRegistry {
    GLOBAL_REGISTRY
        .get()
        .expect("global registry not set")
        .clone()
}

#[derive(Clone)]
pub struct SharedRegistry {
    pub registry: RwArc<Registry<DefaultChainHandle>>,
}

impl SharedRegistry {
    pub fn new(config: Config) -> Self {
        let registry = Registry::new(config);

        Self {
            registry: Arc::new(RwLock::new(registry)),
        }
    }

    pub fn get_or_spawn(&self, chain_id: &ChainId) -> Result<DefaultChainHandle, SpawnError> {
        let read_reg = self.read();

        if read_reg.handles.contains_key(chain_id) {
            Ok(read_reg
                .handles
                .get(chain_id)
                .cloned()
                .expect("runtime exists"))
        } else {
            let chain_config = read_reg
                .config
                .find_chain(chain_id)
                .cloned()
                .ok_or_else(|| SpawnError::missing_chain_config(chain_id.clone()))?;

            let rt = Arc::clone(&read_reg.rt);
            drop(read_reg);

            let handle: DefaultChainHandle = spawn_chain_runtime_with_config(chain_config, rt)?;

            let mut write_reg = self.write();
            write_reg.handles.insert(chain_id.clone(), handle.clone());
            drop(write_reg);

            trace!(chain = %chain_id, "spawned chain runtime");

            Ok(handle)
        }
    }

    pub fn shutdown(&self, chain_id: &ChainId) {
        if let Some(handle) = self.write().handles.remove(chain_id) {
            if let Err(e) = handle.shutdown() {
                warn!(chain = %chain_id, "chain runtime might have failed to shutdown properly: {}", e);
            }
        }
    }

    pub fn write(&self) -> RwLockWriteGuard<'_, Registry<DefaultChainHandle>> {
        self.registry.write().unwrap()
    }

    pub fn read(&self) -> RwLockReadGuard<'_, Registry<DefaultChainHandle>> {
        self.registry.read().unwrap()
    }
}
