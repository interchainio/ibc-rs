use alloc::collections::BTreeMap as HashMap;

use flex_error::define_error;
use tracing::{debug, trace};

use ibc::ics02_client::client_state::{AnyClientState, ClientState};
use ibc::ics02_client::trust_threshold::TrustThreshold;
use ibc::ics03_connection::connection::ConnectionEnd;
use ibc::ics04_channel::error::Error as ChannelError;
use ibc::ics24_host::identifier::{ChainId, ChannelId, ClientId, ConnectionId, PortId};
use ibc::Height;

use crate::chain::handle::ChainHandle;
use crate::error::Error as RelayerError;
use crate::object;
use crate::registry::{Registry, SpawnError};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Permission {
    Allow,
    Deny,
}

impl Permission {
    fn and(self, other: &Self) -> Self {
        if matches!(self, Self::Allow) && matches!(other, Self::Allow) {
            Self::Allow
        } else {
            Self::Deny
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum CacheKey {
    Client(ChainId, ClientId),
    Channel(ChainId, PortId, ChannelId),
    Connection(ChainId, ConnectionId),
}

define_error! {
    FilterError {
        Spawn
            [ SpawnError ]
            | _ | { "spawn error" },

        Relayer
            [ RelayerError ]
            | _ | { "relayer error" },

        Channel
            [ ChannelError ]
            | _ | { "channel error" },

    }
}

/// A cache storing filtering status (allow or deny) for
/// arbitrary identifiers.
#[derive(Default, Debug)]
pub struct FilterPolicy {
    /// A cache associating a generic identifying key, such as
    /// client id, channel id, or connection id, with an
    /// [`Allowed`] status.
    permission_cache: HashMap<CacheKey, Permission>,
}

impl FilterPolicy {
    /// Given a connection end and the underlying client for that
    /// connection, controls both the client as well as the
    /// client on the counterparty chain.
    /// Returns `true` if both clients are allowed, `false` otherwise.
    /// Caches the result for both clients as well as the connection.
    ///
    /// May encounter errors caused by failed queries. Any such error
    /// is propagated and nothing is cached.
    pub fn control_connection_end_and_client<Chain: ChainHandle>(
        &mut self,
        registry: &mut Registry<Chain>,
        chain_id: &ChainId, // Chain hosting the client & connection
        client_state: &AnyClientState,
        connection: &ConnectionEnd,
        connection_id: &ConnectionId,
    ) -> Result<Permission, FilterError> {
        let identifier = CacheKey::Connection(chain_id.clone(), connection_id.clone());

        trace!(
            "[client filter] controlling permissions for {:?}",
            identifier
        );

        // Return if cache hit
        if let Some(p) = self.permission_cache.get(&identifier) {
            trace!("[client filter] cache hit {:?} for {:?}", p, identifier);
            return Ok(*p);
        }

        // Fetch the details of the client on counterparty chain.
        let counterparty_chain_id = client_state.chain_id();
        let counterparty_chain = registry
            .get_or_spawn(&counterparty_chain_id)
            .map_err(FilterError::spawn)?;
        let counterparty_client_id = connection.counterparty().client_id();
        let counterparty_client_state = counterparty_chain
            .query_client_state(counterparty_client_id, Height::zero())
            .map_err(FilterError::relayer)?;

        // Control both clients, cache their results.
        let client_permission = self.control_client(chain_id, connection.client_id(), client_state);
        let counterparty_client_permission = self.control_client(
            &counterparty_chain_id,
            counterparty_client_id,
            &counterparty_client_state,
        );
        let permission = client_permission.and(&counterparty_client_permission);

        debug!(
            "[client filter] {:?}: relay for conn {:?}",
            permission, identifier,
        );
        // Save the connection id in the cache
        self.permission_cache
            .entry(identifier)
            .or_insert(permission);

        Ok(permission)
    }

    /// Given a client identifier and its corresponding client state,
    /// controls the client state and decides if the client should
    /// be allowed or not.
    /// Returns `true` if client is allowed, `false` otherwise.
    /// Caches the result.
    pub fn control_client(
        &mut self,
        host_chain: &ChainId,
        client_id: &ClientId,
        state: &AnyClientState,
    ) -> Permission {
        let identifier = CacheKey::Client(host_chain.clone(), client_id.clone());

        trace!(
            "[client filter] controlling permissions for {:?}",
            identifier
        );

        // Return if cache hit
        if let Some(p) = self.permission_cache.get(&identifier) {
            trace!("[client filter] cache hit {:?} for {:?}", p, identifier);
            return *p;
        }

        let permission = match state.trust_threshold() {
            Some(trust) if trust == TrustThreshold::ONE_THIRD => Permission::Allow,
            Some(_) => {
                trace!(
                    "[client filter] client {} on chain {} has a trust threshold different than 1/3",
                    client_id, host_chain
                );

                Permission::Deny
            }
            None => {
                trace!(
                    "[client filter] client {} on chain {} does not have a trust threshold set",
                    client_id,
                    host_chain
                );

                Permission::Deny
            }
        };

        debug!(
            "[client filter] {:?}: relay for client {:?}",
            permission, identifier
        );

        self.permission_cache
            .entry(identifier)
            .or_insert(permission);

        permission
    }

    pub fn control_client_object<Chain: ChainHandle>(
        &mut self,
        registry: &mut Registry<Chain>,
        obj: &object::Client,
    ) -> Result<Permission, FilterError> {
        let identifier = CacheKey::Client(obj.dst_chain_id.clone(), obj.dst_client_id.clone());

        trace!(
            "[client filter] controlling permissions for {:?}",
            identifier
        );

        // Return if cache hit
        if let Some(p) = self.permission_cache.get(&identifier) {
            trace!("[client filter] cache hit {:?} for {:?}", p, identifier);
            return Ok(*p);
        }

        let chain = registry
            .get_or_spawn(&obj.dst_chain_id)
            .map_err(FilterError::spawn)?;

        trace!(
            "[client filter] deciding if to relay on {:?} hosted chain {}",
            obj.dst_client_id,
            obj.dst_chain_id
        );

        let client_state = chain
            .query_client_state(&obj.dst_client_id, Height::zero())
            .map_err(FilterError::relayer)?;

        Ok(self.control_client(&obj.dst_chain_id, &obj.dst_client_id, &client_state))
    }

    pub fn control_conn_object<Chain: ChainHandle>(
        &mut self,
        registry: &mut Registry<Chain>,
        obj: &object::Connection,
    ) -> Result<Permission, FilterError> {
        let identifier =
            CacheKey::Connection(obj.src_chain_id.clone(), obj.src_connection_id.clone());

        trace!(
            "[client filter] controlling permissions for {:?}",
            identifier
        );

        // Return if cache hit
        if let Some(p) = self.permission_cache.get(&identifier) {
            trace!("[client filter] cache hit {:?} for {:?}", p, identifier);
            return Ok(*p);
        }

        let src_chain = registry
            .get_or_spawn(&obj.src_chain_id)
            .map_err(FilterError::spawn)?;

        trace!(
            "[client filter] deciding if to relay on {:?} hosted on chain {}",
            obj,
            obj.src_chain_id
        );

        let connection_end = src_chain
            .query_connection(&obj.src_connection_id, Height::zero())
            .map_err(FilterError::relayer)?;

        let client_state = src_chain
            .query_client_state(connection_end.client_id(), Height::zero())
            .map_err(FilterError::relayer)?;

        self.control_connection_end_and_client(
            registry,
            &obj.src_chain_id,
            &client_state,
            &connection_end,
            &obj.src_connection_id,
        )
    }

    fn control_channel<Chain: ChainHandle>(
        &mut self,
        registry: &mut Registry<Chain>,
        chain_id: &ChainId,
        port_id: &PortId,
        channel_id: &ChannelId,
    ) -> Result<Permission, FilterError> {
        let identifier = CacheKey::Channel(chain_id.clone(), port_id.clone(), channel_id.clone());

        trace!(
            "[client filter] controlling permissions for {:?}",
            identifier
        );

        // Return if cache hit
        if let Some(p) = self.permission_cache.get(&identifier) {
            trace!("[client filter] cache hit {:?} for {:?}", p, identifier);
            return Ok(*p);
        }

        let src_chain = registry
            .get_or_spawn(chain_id)
            .map_err(FilterError::spawn)?;

        let channel_end = src_chain
            .query_channel(port_id, channel_id, Height::zero())
            .map_err(FilterError::relayer)?;

        let conn_id = channel_end.connection_hops.first().ok_or_else(|| {
            FilterError::channel(ChannelError::invalid_connection_hops_length(
                1,
                channel_end.connection_hops().len(),
            ))
        })?;

        let connection_end = src_chain
            .query_connection(conn_id, Height::zero())
            .map_err(FilterError::relayer)?;

        let client_state = src_chain
            .query_client_state(connection_end.client_id(), Height::zero())
            .map_err(FilterError::relayer)?;

        let permission = self.control_connection_end_and_client(
            registry,
            chain_id,
            &client_state,
            &connection_end,
            conn_id,
        )?;

        let key = CacheKey::Channel(chain_id.clone(), port_id.clone(), channel_id.clone());

        debug!(
            "[client filter] {:?}: relay for channel {:?}: ",
            permission, key
        );

        self.permission_cache.entry(key).or_insert(permission);

        Ok(permission)
    }

    pub fn control_chan_object<Chain: ChainHandle>(
        &mut self,
        registry: &mut Registry<Chain>,
        obj: &object::Channel,
    ) -> Result<Permission, FilterError> {
        self.control_channel(
            registry,
            &obj.src_chain_id,
            &obj.src_port_id,
            &obj.src_channel_id,
        )
    }

    pub fn control_packet_object<Chain: ChainHandle>(
        &mut self,
        registry: &mut Registry<Chain>,
        obj: &object::Packet,
    ) -> Result<Permission, FilterError> {
        self.control_channel(
            registry,
            &obj.src_chain_id,
            &obj.src_port_id,
            &obj.src_channel_id,
        )
    }
}
