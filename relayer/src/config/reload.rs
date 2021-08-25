//! Facility for reloading the relayer configuration.

use alloc::sync::Arc;
use std::{path::PathBuf, sync::RwLock};

use crossbeam_channel::Sender;
use itertools::Itertools;
use thiserror::Error;

use ibc::ics24_host::identifier::ChainId;
use tracing::debug;

use crate::{
    supervisor::cmd::{ConfigUpdate, SupervisorCmd},
    util::diff::{gdiff, Change},
};

use super::{ChainConfig, Config};

#[derive(Debug, Error)]
pub enum Error {
    #[error("failed to load configuration from disk")]
    LoadFailed(#[source] crate::error::Error),

    #[error("configuration is inconsistent, did not find config for added/updated chain {0}")]
    InconsistentConfig(ChainId),

    #[error("internal: poisoned lock")]
    PoisonedLock,
}

/// Facility for reloading the relayer configuration.
/// See [`ConfigReload::reload`].
#[derive(Clone, Debug)]
pub struct ConfigReload {
    path: PathBuf,
    current: Arc<RwLock<Config>>,
    tx_cmd: Sender<SupervisorCmd>,
}

impl ConfigReload {
    /// Constructs a new [`ConfigReload`] value, by specifying
    /// the path to the configuration file to reload,
    /// the current configuration and a channel through which
    /// to send the computed [`ConfigUpdate`]s to the
    /// [`crate::supervisor::Supervisor`].
    pub fn new(
        path: impl Into<PathBuf>,
        current: Arc<RwLock<Config>>,
        tx_cmd: Sender<SupervisorCmd>,
    ) -> Self {
        Self {
            path: path.into(),
            current,
            tx_cmd,
        }
    }

    /// Reload the configuration.
    /// This method will read and parse the configuration from the
    /// file, then perform a diff between the current configuration
    /// and the newly parsed one, and finally send the computed
    /// [`ConfigUpdate`]s to the [`crate::supervisor::Supervisor`].
    ///
    /// See also: [`ConfigReload::update_config`]
    pub fn reload(&self) -> Result<bool, Error> {
        let new_config = super::load(&self.path).map_err(Error::LoadFailed)?;
        self.update_config(new_config)
    }

    /// Compute a diff between the current configuration and the given one,
    /// and send the computed [`ConfigUpdate`]s to the [`crate::supervisor::Supervisor`].
    pub fn update_config(&self, new: Config) -> Result<bool, Error> {
        let updates = self.compute_updates(&new)?;

        if updates.is_empty() {
            return Ok(false);
        }

        for update in updates {
            if self
                .tx_cmd
                .send(SupervisorCmd::UpdateConfig(update))
                .is_err()
            {
                debug!("failed to send config update to supervisor, channel is closed");
            }
        }

        Ok(true)
    }

    /// Compute a set of configuration updates that, when applied to the
    /// current configuration by the supervisor, will result in the given
    /// configuration.
    fn compute_updates(&self, new: &Config) -> Result<Vec<ConfigUpdate>, Error> {
        let cur = self.current.read().map_err(|_| Error::PoisonedLock)?;

        let cur_chains = cur.chains_map();
        let new_chains = new.chains_map();

        let diff = gdiff(&cur_chains, &new_chains, |a, b| config_eq(a, b));

        diff.into_iter()
            .map(|change| match change {
                Change::Added(id) => {
                    let config = new
                        .find_chain(id)
                        .cloned()
                        .ok_or_else(|| Error::InconsistentConfig((*id).clone()))?;

                    Ok(ConfigUpdate::Add(config))
                }
                Change::Updated(id) => {
                    let config = new
                        .find_chain(id)
                        .cloned()
                        .ok_or_else(|| Error::InconsistentConfig((*id).clone()))?;

                    Ok(ConfigUpdate::Update(config))
                }
                Change::Removed(id) => Ok(ConfigUpdate::Remove((*id).clone())),
            })
            .try_collect()
    }
}

// Compare configs for equality using their JSON representation until
// https://github.com/informalsystems/tendermint-rs/issues/919 is fixed.
fn config_eq(a: &ChainConfig, b: &ChainConfig) -> bool {
    match (serde_json::to_value(a), serde_json::to_value(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}
