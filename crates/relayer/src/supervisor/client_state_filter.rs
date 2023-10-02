use alloc::collections::BTreeMap as HashMap;

use flex_error::define_error;
use ibc_relayer_types::core::ics02_client::trust_threshold::TrustThreshold;
use tracing::{debug, trace};

use ibc_relayer_types::core::ics03_connection::connection::ConnectionEnd;
use ibc_relayer_types::core::ics04_channel::error::Error as ChannelError;
use ibc_relayer_types::core::ics24_host::identifier::{
    ChainId, ChannelId, ClientId, ConnectionId, PortId,
};

use crate::chain::handle::ChainHandle;
use crate::chain::requests::{
    IncludeProof, QueryChannelRequest, QueryClientStateRequest, QueryConnectionRequest, QueryHeight,
};
use crate::client_state::AnyClientState;
use crate::error::Error as RelayerError;
use crate::object;
use crate::registry::Registry;
use crate::spawn::SpawnError;

/// The lower bound trust threshold value. Clients with a trust threshold less
/// than this will not be allowed due to security concerns.
const LOWER_BOUND: TrustThreshold = TrustThreshold::ONE_THIRD;

/// The lower bound trust threshold value. Clients with a trust threshold greater
/// than this will not be allowed due to cost-efficiency concerns.
const UPPER_BOUND: TrustThreshold = TrustThreshold::TWO_THIRDS;

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

impl FilterError {
    pub fn log_as_debug(&self) -> bool {
        matches!(self.detail(), FilterErrorDetail::Spawn(e) if e.source.log_as_debug())
    }
}

/// A cache storing filtering status (allow or deny) for
/// arbitrary identifiers.
#[derive(Default, Debug)]
pub struct FilterPolicy {
    /// A cache associating a generic identifying key, such as
    /// client id, channel id, or connection id, with an
    /// [`Allow`](Permission::Allow) status.
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

        trace!("controlling permissions");

        // Return if cache hit
        if let Some(permission) = self.permission_cache.get(&identifier) {
            trace!(?permission, "cache hit");

            return Ok(*permission);
        }

        // Fetch the details of the client on counterparty chain.
        let counterparty_chain_id = client_state.chain_id();
        let counterparty_chain = registry
            .get_or_spawn(&counterparty_chain_id)
            .map_err(FilterError::spawn)?;

        let counterparty_client_id = connection.counterparty().client_id();
        let (counterparty_client_state, _) = {
            counterparty_chain
                .query_client_state(
                    QueryClientStateRequest {
                        client_id: counterparty_client_id.clone(),
                        height: QueryHeight::Latest,
                    },
                    IncludeProof::No,
                )
                .map_err(FilterError::relayer)?
        };

        // Control both clients, cache their results.
        let client_permission = self.control_client(chain_id, connection.client_id(), client_state);
        let counterparty_client_permission = self.control_client(
            &counterparty_chain_id,
            counterparty_client_id,
            &counterparty_client_state,
        );

        let permission = client_permission.and(&counterparty_client_permission);

        debug!(
            ?permission,
            "computed permission for client and counterparty client"
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

        let _span = tracing::error_span!("control.client", client = ?identifier);

        trace!("controlling permissions");

        // Return if cache hit
        if let Some(permission) = self.permission_cache.get(&identifier) {
            trace!(?permission, "cache hit");

            return *permission;
        }

        let permission = match state.trust_threshold() {
            Some(threshold) => {
                if threshold < LOWER_BOUND {
                    trace!(
                        "client {} on chain {} has a trust threshold less than {}",
                        client_id,
                        host_chain,
                        LOWER_BOUND
                    );

                    Permission::Deny
                } else if threshold > UPPER_BOUND {
                    trace!(
                        "client {} on chain {} has a trust threshold greater than {}",
                        client_id,
                        host_chain,
                        UPPER_BOUND
                    );

                    Permission::Deny
                } else {
                    Permission::Allow
                }
            }
            None => {
                trace!(
                    "client {} on chain {} does not have a trust threshold set",
                    client_id,
                    host_chain
                );

                Permission::Deny
            }
        };

        debug!(?permission, "computed permission");

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

        let _span = tracing::error_span!("control.client", client = ?identifier);

        trace!("controlling permissions");

        // Return if cache hit
        if let Some(permission) = self.permission_cache.get(&identifier) {
            trace!(?permission, "cache hit");

            return Ok(*permission);
        }

        let chain = registry
            .get_or_spawn(&obj.dst_chain_id)
            .map_err(FilterError::spawn)?;

        trace!("deciding whether to relay on client");

        let (client_state, _) = chain
            .query_client_state(
                QueryClientStateRequest {
                    client_id: obj.dst_client_id.clone(),
                    height: QueryHeight::Latest,
                },
                IncludeProof::No,
            )
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

        let _span = tracing::error_span!("control.connection", connection = ?identifier).entered();

        trace!("controlling permissions");

        // Return if cache hit
        if let Some(permission) = self.permission_cache.get(&identifier) {
            trace!(?permission, "cache hit");

            return Ok(*permission);
        }

        let src_chain = registry
            .get_or_spawn(&obj.src_chain_id)
            .map_err(FilterError::spawn)?;

        trace!("deciding whether to relay on connection");

        let (connection_end, _) = src_chain
            .query_connection(
                QueryConnectionRequest {
                    connection_id: obj.src_connection_id.clone(),
                    height: QueryHeight::Latest,
                },
                IncludeProof::No,
            )
            .map_err(FilterError::relayer)?;

        let (client_state, _) = src_chain
            .query_client_state(
                QueryClientStateRequest {
                    client_id: connection_end.client_id().clone(),
                    height: QueryHeight::Latest,
                },
                IncludeProof::No,
            )
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

        let _span = tracing::error_span!("control.channel", channel = ?identifier).entered();

        trace!("controlling permissions");

        // Return if cache hit
        if let Some(permission) = self.permission_cache.get(&identifier) {
            trace!(?permission, "cache hit");
            return Ok(*permission);
        }

        let src_chain = registry
            .get_or_spawn(chain_id)
            .map_err(FilterError::spawn)?;

        let (channel_end, _) = src_chain
            .query_channel(
                QueryChannelRequest {
                    port_id: port_id.clone(),
                    channel_id: channel_id.clone(),
                    height: QueryHeight::Latest,
                },
                IncludeProof::No,
            )
            .map_err(FilterError::relayer)?;

        let conn_id = channel_end.connection_hops.first().ok_or_else(|| {
            FilterError::channel(ChannelError::invalid_connection_hops_length(
                1,
                channel_end.connection_hops().len(),
            ))
        })?;

        let (connection_end, _) = src_chain
            .query_connection(
                QueryConnectionRequest {
                    connection_id: conn_id.clone(),
                    height: QueryHeight::Latest,
                },
                IncludeProof::No,
            )
            .map_err(FilterError::relayer)?;

        let (client_state, _) = src_chain
            .query_client_state(
                QueryClientStateRequest {
                    client_id: connection_end.client_id().clone(),
                    height: QueryHeight::Latest,
                },
                IncludeProof::No,
            )
            .map_err(FilterError::relayer)?;

        let permission = self.control_connection_end_and_client(
            registry,
            chain_id,
            &client_state,
            &connection_end,
            conn_id,
        )?;

        debug!(?permission, "computed permission",);

        self.permission_cache
            .entry(identifier)
            .or_insert(permission);

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
