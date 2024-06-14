use core::fmt::{Display, Error as FmtError, Formatter};
use core::time::Duration;

pub use error::ChannelError;
use ibc_proto::google::protobuf::Any;
use ibc_proto::ibc::core::channel::v1::{MsgMultihopProofs, MultihopProof};
use ibc_proto::Protobuf;
use ibc_relayer_types::core::ics04_channel::channel::{
    ChannelEnd, Counterparty, IdentifiedChannelEnd, Ordering, State,
};
use ibc_relayer_types::core::ics04_channel::msgs::chan_close_confirm::MsgChannelCloseConfirm;
use ibc_relayer_types::core::ics04_channel::msgs::chan_close_init::MsgChannelCloseInit;
use ibc_relayer_types::core::ics04_channel::msgs::chan_open_ack::MsgChannelOpenAck;
use ibc_relayer_types::core::ics04_channel::msgs::chan_open_confirm::MsgChannelOpenConfirm;
use ibc_relayer_types::core::ics04_channel::msgs::chan_open_init::MsgChannelOpenInit;
use ibc_relayer_types::core::ics04_channel::msgs::chan_open_try::MsgChannelOpenTry;
use ibc_relayer_types::core::ics23_commitment::commitment::CommitmentProofBytes;
use ibc_relayer_types::core::ics23_commitment::merkle::apply_prefix;
use ibc_relayer_types::core::ics24_host::identifier::{
    ChainId, ChannelId, ClientId, ConnectionId, PortId,
};
use ibc_relayer_types::core::ics24_host::path::{
    ChannelEndsPath, ClientConsensusStatePath, ConnectionsPath, Path,
};
use ibc_relayer_types::core::ics33_multihop::channel_path::ConnectionHops;
use ibc_relayer_types::events::IbcEvent;
use ibc_relayer_types::proofs::MultihopProofHeights;
use ibc_relayer_types::tx_msg::Msg;
use ibc_relayer_types::Height;
use serde::Serialize;
use tracing::{debug, error, info, warn};

use crate::chain::counterparty::{channel_connection_client, channel_state_on_destination};
use crate::chain::handle::ChainHandle;
use crate::chain::requests::{
    IncludeProof, PageRequest, QueryChannelRequest, QueryConnectionChannelsRequest,
    QueryConnectionRequest, QueryConsensusStateRequest, QueryHeight,
};
use crate::chain::tracking::TrackedMsgs;
use crate::connection::Connection;
use crate::foreign_client::{ForeignClient, HasExpiredOrFrozenError};
use crate::object::Channel as WorkerChannelObject;
use crate::registry::get_global_registry;
use crate::supervisor::error::Error as SupervisorError;
use crate::util::pretty::{PrettyDuration, PrettyOption};
use crate::util::retry::retry_with_index;
use crate::util::retry::RetryResult;
use crate::util::task::Next;

pub mod error;
pub mod version;
use version::Version;

pub mod channel_handshake_retry {
    //! Provides utility methods and constants to configure the retry behavior
    //! for the channel handshake algorithm.

    use crate::channel::ChannelError;
    use crate::util::retry::{clamp, ConstantGrowth};
    use core::time::Duration;

    /// Approximate number of retries per block.
    const PER_BLOCK_RETRIES: u32 = 5;

    /// Defines the increment in delay between subsequent retries.
    /// A value of `0` will make the retry delay constant.
    const DELAY_INCREMENT: Duration = Duration::from_secs(0);

    /// Maximum number of retries
    const MAX_RETRIES: u32 = 10;

    /// The default retry strategy.
    /// We retry with a constant backoff strategy. The strategy is parametrized by the
    /// maximum block time expressed as a `Duration`.
    pub fn default_strategy(max_block_time: Duration) -> impl Iterator<Item = Duration> {
        let retry_delay = max_block_time / PER_BLOCK_RETRIES;

        clamp(
            ConstantGrowth::new(retry_delay, DELAY_INCREMENT),
            retry_delay + DELAY_INCREMENT * MAX_RETRIES,
            MAX_RETRIES as usize,
        )
    }

    /// Translates from an error type that the `retry` mechanism threw into
    /// a crate specific error of [`ChannelError`] type.
    pub fn from_retry_error(e: retry::Error<ChannelError>, description: String) -> ChannelError {
        ChannelError::max_retry(description, e.tries, e.total_delay, e.error)
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(bound(serialize = "(): Serialize"))]
pub struct ChannelSide<Chain: ChainHandle> {
    #[serde(skip)]
    pub chain: Chain,
    client_id: ClientId,
    connection_id: ConnectionId,
    connection_hops: Option<ConnectionHops>,
    port_id: PortId,
    channel_id: Option<ChannelId>,
    version: Option<Version>,
}

impl<Chain: ChainHandle> Display for ChannelSide<Chain> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), FmtError> {
        match (&self.channel_id, &self.version) {
            (Some(channel_id), Some(version)) => write!(f, "ChannelSide {{ chain: {}, client_id: {}, connection_id: {}, port_id: {}, channel_id: {}, version: {} }}", self.chain, self.client_id, self.connection_id, self.port_id, channel_id, version),
            (Some(channel_id), None) => write!(f, "ChannelSide {{ chain: {}, client_id: {}, connection_id: {}, port_id: {}, channel_id: {}, version: None }}", self.chain, self.client_id, self.connection_id, self.port_id, channel_id),
            (None, Some(version)) => write!(f, "ChannelSide {{ chain: {}, client_id: {}, connection_id: {}, port_id: {}, channel_id: None, version: {} }}", self.chain, self.client_id, self.connection_id, self.port_id, version),
            (None, None) => write!(f, "ChannelSide {{ chain: {}, client_id: {}, connection_id: {}, port_id: {}, channel_id: None, version: None }}", self.chain, self.client_id, self.connection_id, self.port_id),
        }
    }
}

impl<Chain: ChainHandle> ChannelSide<Chain> {
    pub fn new(
        chain: Chain,
        client_id: ClientId,
        connection_id: ConnectionId,
        connection_hops: Option<ConnectionHops>,
        port_id: PortId,
        channel_id: Option<ChannelId>,
        version: Option<Version>,
    ) -> ChannelSide<Chain> {
        Self {
            chain,
            client_id,
            connection_id,
            connection_hops,
            port_id,
            channel_id,
            version,
        }
    }

    pub fn chain_id(&self) -> ChainId {
        self.chain.id()
    }

    pub fn client_id(&self) -> &ClientId {
        &self.client_id
    }

    pub fn connection_id(&self) -> &ConnectionId {
        &self.connection_id
    }

    pub fn connection_hops(&self) -> Option<&ConnectionHops> {
        self.connection_hops.as_ref()
    }

    pub fn port_id(&self) -> &PortId {
        &self.port_id
    }

    pub fn channel_id(&self) -> Option<&ChannelId> {
        self.channel_id.as_ref()
    }

    pub fn version(&self) -> Option<&Version> {
        self.version.as_ref()
    }

    pub fn map_chain<ChainB: ChainHandle>(
        self,
        mapper: impl Fn(Chain) -> ChainB,
    ) -> ChannelSide<ChainB> {
        ChannelSide {
            chain: mapper(self.chain),
            client_id: self.client_id,
            connection_id: self.connection_id,
            connection_hops: self.connection_hops,
            port_id: self.port_id,
            channel_id: self.channel_id,
            version: self.version,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(bound(serialize = "(): Serialize"))]
pub struct Channel<ChainA: ChainHandle, ChainB: ChainHandle> {
    pub ordering: Ordering,
    pub a_side: ChannelSide<ChainA>,
    pub b_side: ChannelSide<ChainB>,
    pub connection_delay: Duration,
}

impl<ChainA: ChainHandle, ChainB: ChainHandle> Display for Channel<ChainA, ChainB> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), FmtError> {
        write!(
            f,
            "Channel {{ ordering: {}, a_side: {}, b_side: {}, connection_delay: {} }}",
            self.ordering,
            self.a_side,
            self.b_side,
            // FIXME: add connection hops
            PrettyDuration(&self.connection_delay)
        )
    }
}

impl<ChainA: ChainHandle, ChainB: ChainHandle> Channel<ChainA, ChainB> {
    /// Creates a new channel on top of the existing connection. If the channel is not already
    /// set-up on both sides of the connection, this functions also fulfils the channel handshake.
    pub fn new(
        connection: Connection<ChainA, ChainB>,
        ordering: Ordering,
        a_port: PortId,
        b_port: PortId,
        a_side_hops: Option<ConnectionHops>,
        b_side_hops: Option<ConnectionHops>,
        version: Option<Version>,
    ) -> Result<Self, ChannelError> {
        let src_connection_id = connection
            .src_connection_id()
            .ok_or_else(|| ChannelError::missing_local_connection(connection.src_chain().id()))?;
        let dst_connection_id = connection
            .dst_connection_id()
            .ok_or_else(|| ChannelError::missing_local_connection(connection.dst_chain().id()))?;

        let mut channel = Self {
            ordering,
            a_side: ChannelSide::new(
                connection.src_chain(),
                connection.src_client_id().clone(),
                src_connection_id.clone(),
                a_side_hops,
                a_port,
                Default::default(),
                version.clone(),
            ),
            b_side: ChannelSide::new(
                connection.dst_chain(),
                connection.dst_client_id().clone(),
                dst_connection_id.clone(),
                b_side_hops,
                b_port,
                Default::default(),
                version,
            ),
            connection_delay: connection.delay_period,
        };

        channel.handshake()?;

        Ok(channel)
    }

    pub fn restore_from_event(
        chain: ChainA,
        counterparty_chain: ChainB,
        channel_open_event: IbcEvent,
    ) -> Result<Channel<ChainA, ChainB>, ChannelError> {
        let channel_event_attributes = channel_open_event
            .clone()
            .channel_attributes()
            .ok_or_else(|| ChannelError::invalid_event(channel_open_event))?;

        let port_id = channel_event_attributes.port_id.clone();
        let channel_id = channel_event_attributes.channel_id;

        // FIXME: connection_id is an instance of ConnectionIds(Vec<ConnectionId>), but ChannelSide::new() requires
        // a single ConnectionId. To avoid further changes in ChannelSide, get only the 0th element for now.
        // In the future, modify ChannelSide to use a Vec<ConnectionId>.
        let connection_id = channel_event_attributes.connection_id.as_slice()[0].clone();

        let (connection, _) = chain
            .query_connection(
                QueryConnectionRequest {
                    connection_id: connection_id.clone(), // FIXME: Add support for multihop connections queries.
                    height: QueryHeight::Latest,
                },
                IncludeProof::No,
            )
            .map_err(ChannelError::relayer)?;

        let connection_counterparty = connection.counterparty();

        let counterparty_connection_id = connection_counterparty
            .connection_id()
            .ok_or_else(ChannelError::missing_counterparty_connection)?;

        Ok(Channel {
            // The event does not include the channel ordering.
            // The message handlers `build_chan_open..` determine the order included in the handshake
            // message from channel query.
            ordering: Default::default(),
            a_side: ChannelSide::new(
                chain,
                connection.client_id().clone(),
                connection_id.clone(),
                None, //FIXME: Unsure what to add here ('None' for now), can we get the hops from the event?
                port_id,
                channel_id,
                // The event does not include the version.
                // The message handlers `build_chan_open..` determine the version from channel query.
                None,
            ),
            b_side: ChannelSide::new(
                counterparty_chain,
                connection.counterparty().client_id().clone(),
                counterparty_connection_id.clone(),
                None, //FIXME: Unsure what to add here ('None' for now), can we get the hops from the event?
                channel_event_attributes.counterparty_port_id.clone(),
                channel_event_attributes.counterparty_channel_id,
                None,
            ),
            connection_delay: connection.delay_period(),
        })
    }

    /// Recreates a 'Channel' object from the worker's object built from chain state scanning.
    /// The channel must exist on chain and its connection must be initialized on both chains.
    pub fn restore_from_state(
        chain: ChainA,
        counterparty_chain: ChainB,
        channel: WorkerChannelObject,
        height: Height,
    ) -> Result<(Channel<ChainA, ChainB>, State), ChannelError> {
        let (a_channel, _) = chain
            .query_channel(
                QueryChannelRequest {
                    port_id: channel.src_port_id.clone(),
                    channel_id: channel.src_channel_id.clone(),
                    height: QueryHeight::Specific(height),
                },
                IncludeProof::No,
            )
            .map_err(ChannelError::relayer)?;

        let a_connection_id = a_channel.connection_hops().first().ok_or_else(|| {
            ChannelError::supervisor(SupervisorError::missing_connection_hops(
                channel.src_channel_id.clone(),
                chain.id(),
            ))
        })?;

        let (a_connection, _) = chain
            .query_connection(
                QueryConnectionRequest {
                    connection_id: a_connection_id.clone(),
                    height: QueryHeight::Latest,
                },
                IncludeProof::No,
            )
            .map_err(ChannelError::relayer)?;

        let b_connection_id = a_connection
            .counterparty()
            .connection_id()
            .cloned()
            .ok_or_else(|| {
                ChannelError::supervisor(SupervisorError::channel_connection_uninitialized(
                    channel.src_channel_id.clone(),
                    chain.id(),
                    a_connection.counterparty().clone(),
                ))
            })?;

        let mut handshake_channel = Channel {
            ordering: *a_channel.ordering(),
            a_side: ChannelSide::new(
                chain.clone(),
                a_connection.client_id().clone(),
                a_connection_id.clone(),
                None, // FIXME: Unsure about what to add here ('None' for now)
                channel.src_port_id.clone(),
                Some(channel.src_channel_id.clone()),
                None,
            ),
            b_side: ChannelSide::new(
                counterparty_chain.clone(),
                a_connection.counterparty().client_id().clone(),
                b_connection_id.clone(),
                None, // FIXME: Unsure about what to add here ('None' for now)
                a_channel.remote.port_id.clone(),
                a_channel.remote.channel_id.clone(),
                None,
            ),
            connection_delay: a_connection.delay_period(),
        };

        if a_channel.state_matches(&State::Init) && a_channel.remote.channel_id.is_none() {
            let channels: Vec<IdentifiedChannelEnd> = counterparty_chain
                .query_connection_channels(QueryConnectionChannelsRequest {
                    connection_id: b_connection_id,
                    pagination: Some(PageRequest::all()),
                })
                .map_err(ChannelError::relayer)?;

            for chan in channels {
                if let Some(remote_channel_id) = chan.channel_end.remote.channel_id() {
                    if remote_channel_id == &channel.src_channel_id {
                        handshake_channel.b_side.channel_id = Some(chan.channel_id);
                        break;
                    }
                }
            }
        }

        Ok((handshake_channel, a_channel.state))
    }

    pub fn src_chain(&self) -> &ChainA {
        &self.a_side.chain
    }

    pub fn dst_chain(&self) -> &ChainB {
        &self.b_side.chain
    }

    pub fn a_chain(&self) -> ChainA {
        self.a_side.chain.clone()
    }

    pub fn b_chain(&self) -> ChainB {
        self.b_side.chain.clone()
    }

    pub fn src_client_id(&self) -> &ClientId {
        &self.a_side.client_id
    }

    pub fn dst_client_id(&self) -> &ClientId {
        &self.b_side.client_id
    }

    pub fn src_connection_id(&self) -> &ConnectionId {
        &self.a_side.connection_id
    }

    pub fn dst_connection_id(&self) -> &ConnectionId {
        &self.b_side.connection_id
    }

    pub fn src_port_id(&self) -> &PortId {
        &self.a_side.port_id
    }

    pub fn dst_port_id(&self) -> &PortId {
        &self.b_side.port_id
    }

    pub fn src_channel_id(&self) -> Option<&ChannelId> {
        self.a_side.channel_id()
    }

    pub fn dst_channel_id(&self) -> Option<&ChannelId> {
        self.b_side.channel_id()
    }

    pub fn a_channel_id(&self) -> Option<&ChannelId> {
        self.a_side.channel_id()
    }

    pub fn b_channel_id(&self) -> Option<&ChannelId> {
        self.b_side.channel_id()
    }

    pub fn src_version(&self) -> Option<&Version> {
        self.a_side.version.as_ref()
    }

    pub fn dst_version(&self) -> Option<&Version> {
        self.b_side.version.as_ref()
    }

    fn a_channel(&self, channel_id: Option<&ChannelId>) -> Result<ChannelEnd, ChannelError> {
        if let Some(id) = channel_id {
            self.a_chain()
                .query_channel(
                    QueryChannelRequest {
                        port_id: self.a_side.port_id.clone(),
                        channel_id: id.clone(),
                        height: QueryHeight::Latest,
                    },
                    IncludeProof::No,
                )
                .map(|(channel_end, _)| channel_end)
                .map_err(|e| ChannelError::chain_query(self.a_chain().id(), e))
        } else {
            Ok(ChannelEnd::default())
        }
    }

    fn b_channel(&self, channel_id: Option<&ChannelId>) -> Result<ChannelEnd, ChannelError> {
        if let Some(id) = channel_id {
            self.b_chain()
                .query_channel(
                    QueryChannelRequest {
                        port_id: self.b_side.port_id.clone(),
                        channel_id: id.clone(),
                        height: QueryHeight::Latest,
                    },
                    IncludeProof::No,
                )
                .map(|(channel_end, _)| channel_end)
                .map_err(|e| ChannelError::chain_query(self.b_chain().id(), e))
        } else {
            Ok(ChannelEnd::default())
        }
    }

    /// Returns a `Duration` representing the maximum value among the
    /// [`ChainConfig.max_block_time`] for the two networks that
    /// this channel belongs to.
    fn max_block_times(&self) -> Result<Duration, ChannelError> {
        let a_block_time = self
            .a_chain()
            .config()
            .map_err(ChannelError::relayer)?
            .max_block_time();
        let b_block_time = self
            .b_chain()
            .config()
            .map_err(ChannelError::relayer)?
            .max_block_time();
        Ok(a_block_time.max(b_block_time))
    }

    pub fn flipped(&self) -> Channel<ChainB, ChainA> {
        Channel {
            ordering: self.ordering,
            a_side: self.b_side.clone(),
            b_side: self.a_side.clone(),
            connection_delay: self.connection_delay,
        }
    }

    /// Queries the chains for latest channel end information. It verifies the relayer channel
    /// IDs and updates them if needed.
    /// Returns the states of the two channel ends.
    ///
    /// The relayer channel stores the channel identifiers on the two chains a and b.
    /// These identifiers need to be cross validated with the corresponding on-chain ones at some
    /// handshake steps.
    /// This is required because of crossing handshake messages in the presence of multiple relayers.
    ///
    /// Chain a is queried with the relayer's `a_side.channel_id` (`relayer_a_id`) with result
    /// `a_channel`. If the counterparty id of this channel, `a_counterparty_id`,
    /// is some id then it must match the relayer's `b_side.channel_id` (`relayer_b_id`).
    /// A similar check is done for the `b_side` of the channel.
    ///
    ///  a                                 relayer                                    b
    ///  |                     a_side -- channel -- b_side                         |
    ///  a_id _____________> relayer_a_id             relayer_b_id <______________> b_id
    ///  |                      \                                /                    |
    /// a_counterparty_id <_____________________________________/                     |
    ///                           \____________________________________>   b_counterparty_id
    ///
    /// There are two cases to consider.
    ///
    /// Case 1 (fix channel ID):
    ///  a                                                      b
    ///  | <-- Init (r1)                                        |
    ///  | a_id = 1, a_counterparty_id = None                   |
    ///  |                                         Try (r2) --> |
    ///  |                    b_id = 100, b_counterparty_id = 1 |
    ///  |                                         Try (r1) --> |
    ///  |                    b_id = 101, b_counterparty_id = 1 |
    ///  | <-- Ack (r2)
    ///  | a_id = 1, a_counterparty_id = 100
    ///
    /// Here relayer r1 has a_side channel 1 and b_side channel 101
    /// while on chain a the counterparty of channel 1 is 100. r1 needs to update
    /// its b_side to 100
    ///
    /// Case 2 (update from None to some channel ID):
    ///  a                                                      b
    ///  | <-- Init (r1)                                        |
    ///  | a_id = 1, a_counterparty_id = None                   |
    ///  |                                         Try (r2) --> |
    ///  |                    b_id = 100, b_counterparty_id = 1 |
    ///  | <-- Ack (r2)
    ///  | a_id = 1, a_counterparty_id = 100
    ///
    /// Here relayer r1 has a_side channel 1 and b_side is unknown
    /// while on chain a the counterparty of channel 1 is 100. r1 needs to update
    /// its b_side to 100
    fn update_channel_and_query_states(&mut self) -> Result<(State, State), ChannelError> {
        let relayer_a_id = self.a_side.channel_id();
        let relayer_b_id = self.b_side.channel_id().cloned();

        let a_channel = self.a_channel(relayer_a_id)?;
        let a_counterparty_id = a_channel.counterparty().channel_id();

        if a_counterparty_id.is_some() && a_counterparty_id != relayer_b_id.as_ref() {
            warn!(
                "updating the expected {} of side_b({}) since it is different than the \
                counterparty of {}: {}, on {}. This is typically caused by crossing handshake \
                messages in the presence of multiple relayers.",
                PrettyOption(&relayer_b_id),
                self.b_chain().id(),
                PrettyOption(&relayer_a_id),
                PrettyOption(&a_counterparty_id),
                self.a_chain().id(),
            );
            self.b_side.channel_id = a_counterparty_id.cloned();
        }

        let updated_relayer_b_id = self.b_side.channel_id();
        let b_channel = self.b_channel(updated_relayer_b_id)?;
        let b_counterparty_id = b_channel.counterparty().channel_id();

        if b_counterparty_id.is_some() && b_counterparty_id != relayer_a_id {
            if updated_relayer_b_id == relayer_b_id.as_ref() {
                warn!(
                    "updating the expected {} of side_a({}) since it is different than the \
                    counterparty of {}: {}, on {}. This is typically caused by crossing handshake \
                    messages in the presence of multiple relayers.",
                    PrettyOption(&relayer_a_id),
                    self.a_chain().id(),
                    PrettyOption(&updated_relayer_b_id),
                    PrettyOption(&b_counterparty_id),
                    self.b_chain().id(),
                );
                self.a_side.channel_id = b_counterparty_id.cloned();
            } else {
                panic!(
                    "mismatched channel ids in channel ends: {} - {} and {} - {}",
                    self.a_chain().id(),
                    a_channel,
                    self.b_chain().id(),
                    b_channel,
                );
            }
        }
        Ok((*a_channel.state(), *b_channel.state()))
    }

    /// Sends a channel open handshake message.
    /// The message sent depends on the chain status of the channel ends.
    fn do_chan_open_handshake(&mut self) -> Result<(), ChannelError> {
        let (a_state, b_state) = self.update_channel_and_query_states()?;
        debug!(
            "do_chan_open_handshake with channel end states: {}, {}",
            a_state, b_state
        );

        match (a_state, b_state) {
            // send the Init message to chain a (source)
            (State::Uninitialized, State::Uninitialized) => {
                let event = self
                    .flipped()
                    .build_chan_open_init_and_send()
                    .map_err(|e| {
                        error!("failed ChanOpenInit {}: {}", self.a_side, e);
                        e
                    })?;
                let channel_id = extract_channel_id(&event)?;
                self.a_side.channel_id = Some(channel_id.clone());
            }

            // send the Try message to chain a (source)
            (State::Uninitialized, State::Init) | (State::Init, State::Init) => {
                let event = self.flipped().build_chan_open_try_and_send().map_err(|e| {
                    error!("failed ChanOpenTry {}: {}", self.a_side, e);
                    e
                })?;

                let channel_id = extract_channel_id(&event)?;
                self.a_side.channel_id = Some(channel_id.clone());
            }

            // send the Try message to chain b (destination)
            (State::Init, State::Uninitialized) => {
                let event = self.build_chan_open_try_and_send().map_err(|e| {
                    error!("failed ChanOpenTry {}: {}", self.b_side, e);
                    e
                })?;

                let channel_id = extract_channel_id(&event)?;
                self.b_side.channel_id = Some(channel_id.clone());
            }

            // send the Ack message to chain a (source)
            (State::Init, State::TryOpen) | (State::TryOpen, State::TryOpen) => {
                self.flipped().build_chan_open_ack_and_send().map_err(|e| {
                    error!("failed ChanOpenAck {}: {}", self.a_side, e);
                    e
                })?;
            }

            // send the Ack message to chain b (destination)
            (State::TryOpen, State::Init) => {
                self.build_chan_open_ack_and_send().map_err(|e| {
                    error!("failed ChanOpenAck {}: {}", self.b_side, e);
                    e
                })?;
            }

            // send the Confirm message to chain b (destination)
            (State::Open, State::TryOpen) => {
                self.build_chan_open_confirm_and_send().map_err(|e| {
                    error!("failed ChanOpenConfirm {}: {}", self.b_side, e);
                    e
                })?;
            }

            // send the Confirm message to chain a (source)
            (State::TryOpen, State::Open) => {
                self.flipped()
                    .build_chan_open_confirm_and_send()
                    .map_err(|e| {
                        error!("failed ChanOpenConfirm {}: {}", self.a_side, e);
                        e
                    })?;
            }

            (State::Open, State::Open) => {
                info!("channel handshake already finished for {}", self);
                return Ok(());
            }

            (a_state, b_state) => {
                warn!(
                    "do_conn_open_handshake does not handle channel end state combination: \
                    {}-{}, {}-{}. will retry to account for RPC node data availability issues.",
                    self.a_chain().id(),
                    a_state,
                    self.b_chain().id(),
                    b_state
                );
            }
        }
        Err(ChannelError::handshake_finalize())
    }

    /// Executes the channel handshake protocol (ICS004)
    fn handshake(&mut self) -> Result<(), ChannelError> {
        let max_block_times = self.max_block_times()?;

        retry_with_index(
            channel_handshake_retry::default_strategy(max_block_times),
            |_| {
                if let Err(e) = self.do_chan_open_handshake() {
                    if e.is_expired_or_frozen_error() {
                        RetryResult::Err(e)
                    } else {
                        RetryResult::Retry(e)
                    }
                } else {
                    RetryResult::Ok(())
                }
            },
        )
        .map_err(|err| {
            error!("failed to open channel after {} retries", err.tries);

            channel_handshake_retry::from_retry_error(
                err,
                format!("failed to finish channel handshake for {self:?}"),
            )
        })?;

        Ok(())
    }

    pub fn counterparty_state(&self) -> Result<State, ChannelError> {
        // Source channel ID must be specified
        let channel_id = self
            .src_channel_id()
            .ok_or_else(ChannelError::missing_local_channel_id)?;

        let channel_deps =
            channel_connection_client(self.src_chain(), self.src_port_id(), channel_id)
                .map_err(|e| ChannelError::query_channel(channel_id.clone(), e))?;

        channel_state_on_destination(
            &channel_deps.channel,
            &channel_deps.connection,
            self.dst_chain(),
        )
        .map_err(|e| ChannelError::query_channel(channel_id.clone(), e))
    }

    pub fn handshake_step(
        &mut self,
        state: State,
    ) -> Result<(Option<IbcEvent>, Next), ChannelError> {
        let event = match (state, self.counterparty_state()?) {
            // Open handshake steps
            (State::Init, State::Uninitialized) => Some(self.build_chan_open_try_and_send()?),
            (State::Init, State::Init) => Some(self.build_chan_open_try_and_send()?),
            (State::TryOpen, State::Init) => Some(self.build_chan_open_ack_and_send()?),
            (State::TryOpen, State::TryOpen) => Some(self.build_chan_open_ack_and_send()?),
            (State::Open, State::TryOpen) => Some(self.build_chan_open_confirm_and_send()?),
            (State::Open, State::Open) => return Ok((None, Next::Abort)),

            // If the counterparty state is already Open but current state is TryOpen,
            // return anyway as the final step is to be done by the counterparty worker.
            (State::TryOpen, State::Open) => return Ok((None, Next::Abort)),

            // Close handshake steps
            (State::Closed, State::Closed) => return Ok((None, Next::Abort)),
            (State::Closed, _) => Some(self.build_chan_close_confirm_and_send()?),

            _ => None,
        };

        // Abort if the channel is at OpenAck, OpenConfirm or CloseConfirm stage, as there is
        // nothing more for the worker to do
        match event {
            Some(IbcEvent::OpenConfirmChannel(_))
            | Some(IbcEvent::OpenAckChannel(_))
            | Some(IbcEvent::CloseConfirmChannel(_)) => Ok((event, Next::Abort)),
            _ => Ok((event, Next::Continue)),
        }
    }

    pub fn step_state(&mut self, state: State, index: u64) -> RetryResult<Next, u64> {
        match self.handshake_step(state) {
            Err(e) => {
                if e.is_expired_or_frozen_error() {
                    error!(
                        "failed to establish channel handshake on frozen client: {}",
                        e
                    );
                    RetryResult::Err(index)
                } else {
                    error!("failed Chan{} with error: {}", state, e);
                    RetryResult::Retry(index)
                }
            }
            Ok((Some(ev), handshake_completed)) => {
                info!("channel handshake step completed with events: {}", ev);
                RetryResult::Ok(handshake_completed)
            }
            Ok((None, handshake_completed)) => RetryResult::Ok(handshake_completed),
        }
    }

    pub fn step_event(&mut self, event: &IbcEvent, index: u64) -> RetryResult<Next, u64> {
        let state = match event {
            IbcEvent::OpenInitChannel(_) => State::Init,
            IbcEvent::OpenTryChannel(_) => State::TryOpen,
            IbcEvent::OpenAckChannel(_) => State::Open,
            IbcEvent::OpenConfirmChannel(_) => State::Open,
            IbcEvent::CloseInitChannel(_) => State::Closed,
            _ => State::Uninitialized,
        };

        self.step_state(state, index)
    }

    pub fn build_update_client_on_dst(&self, height: Height) -> Result<Vec<Any>, ChannelError> {
        let client = ForeignClient::restore(
            self.dst_client_id().clone(),
            self.dst_chain().clone(),
            self.src_chain().clone(),
        );

        client.wait_and_build_update_client(height).map_err(|e| {
            ChannelError::client_operation(self.dst_client_id().clone(), self.dst_chain().id(), e)
        })
    }

    pub fn build_update_client_on_last_hop(
        &self,
        height: Height,
    ) -> Result<Vec<Any>, ChannelError> {
        let channel_id = self
            .a_side
            .channel_id()
            .ok_or(ChannelError::missing_local_channel_id())?;

        let connection_hops =
            self.a_side
                .connection_hops()
                .ok_or(ChannelError::missing_local_connection_hops(
                    channel_id.clone(),
                    self.a_side.chain_id().clone(),
                ))?;

        let last_hop = connection_hops.hops.iter().last().ok_or(
            ChannelError::missing_local_connection_hops(
                channel_id.clone(),
                self.a_side.chain_id().clone(),
            ),
        )?;

        // Get access to the registry to get or spawn chain handles
        let registry = get_global_registry();

        let last_hop_src_chain = registry
            .get_or_spawn(&last_hop.src_chain_id)
            .map_err(ChannelError::spawn)?;

        // Restore the client hosted by the channel path's (a_side to b_side) destination chain
        // to track the state of the penultimate chain.
        let client = ForeignClient::restore(
            self.dst_client_id().clone(),
            self.dst_chain().clone(),
            last_hop_src_chain.clone(),
        );

        client.wait_and_build_update_client(height).map_err(|e| {
            ChannelError::client_operation(self.dst_client_id().clone(), self.dst_chain().id(), e)
        })
    }

    pub fn build_chan_open_init(&self) -> Result<Vec<Any>, ChannelError> {
        let signer = self
            .dst_chain()
            .get_signer()
            .map_err(|e| ChannelError::query(self.dst_chain().id(), e))?;

        let counterparty = Counterparty::new(self.src_port_id().clone(), None);

        // If the user supplied a version, use that.
        // Otherwise, either use the version defined for the `transfer`
        // or an empty version if the port is non-standard.
        let version = self
            .dst_version()
            .cloned()
            .or_else(|| version::default_by_port(self.dst_port_id()))
            .unwrap_or_else(|| {
                warn!(
                    chain = %self.dst_chain().id(),
                    channel = ?self.dst_channel_id(),
                    port = %self.dst_port_id(),
                    "no version specified for the channel, falling back on empty version"
                );

                Version::empty()
            });

        let channel = ChannelEnd::new(
            State::Init,
            self.ordering,
            counterparty,
            self.b_side
                .connection_hops
                .as_ref()
                .map(|hops| hops.connection_ids())
                .unwrap_or_else(|| vec![self.dst_connection_id().clone()]),
            version,
            0,
        );

        // Build the domain type message
        let new_msg = MsgChannelOpenInit {
            port_id: self.dst_port_id().clone(),
            channel,
            signer,
        };

        Ok(vec![new_msg.to_any()])
    }

    pub fn build_chan_open_init_and_send(&self) -> Result<IbcEvent, ChannelError> {
        let dst_msgs = self.build_chan_open_init()?;

        let tm = TrackedMsgs::new_static(dst_msgs, "ChannelOpenInit");

        let events = self
            .dst_chain()
            .send_messages_and_wait_commit(tm)
            .map_err(|e| ChannelError::submit(self.dst_chain().id(), e))?;

        // Find the relevant event for channel open init
        let result = events
            .into_iter()
            .find(|event_with_height| {
                matches!(event_with_height.event, IbcEvent::OpenInitChannel(_))
                    || matches!(event_with_height.event, IbcEvent::ChainError(_))
            })
            .ok_or_else(|| {
                ChannelError::missing_event("no chan init event was in the response".to_string())
            })?;

        match &result.event {
            IbcEvent::OpenInitChannel(_) => {
                info!("🎊  {} => {}", self.dst_chain().id(), result);
                Ok(result.event)
            }
            IbcEvent::ChainError(e) => Err(ChannelError::tx_response(e.clone())),
            _ => Err(ChannelError::invalid_event(result.event)),
        }
    }

    /// Retrieves the channel from destination and compares it
    /// against the expected channel. built from the message type [`ChannelMsgType`].
    ///
    /// If the expected and the destination channels are compatible,
    /// returns the expected channel
    ///
    /// # Precondition:
    /// Source and destination channel IDs must be `Some`.
    fn validated_expected_channel(
        &self,
        msg_type: ChannelMsgType,
    ) -> Result<ChannelEnd, ChannelError> {
        // Destination channel ID must be specified
        let dst_channel_id = self
            .dst_channel_id()
            .ok_or_else(ChannelError::missing_counterparty_channel_id)?;

        // If there is a channel present on the destination chain,
        // the counterparty should look like this:
        let counterparty =
            Counterparty::new(self.src_port_id().clone(), self.src_channel_id().cloned());

        // The highest expected state, depends on the message type:
        let highest_state = match msg_type {
            ChannelMsgType::OpenAck => State::TryOpen,
            ChannelMsgType::OpenConfirm => State::TryOpen,
            ChannelMsgType::CloseConfirm => State::Open,
            _ => State::Uninitialized,
        };

        let dst_expected_channel = ChannelEnd::new(
            highest_state,
            self.ordering,
            counterparty,
            vec![self.dst_connection_id().clone()],
            Version::empty(),
            0,
        );

        // Retrieve existing channel
        let (dst_channel, _) = self
            .dst_chain()
            .query_channel(
                QueryChannelRequest {
                    port_id: self.dst_port_id().clone(),
                    channel_id: dst_channel_id.clone(),
                    height: QueryHeight::Latest,
                },
                IncludeProof::No,
            )
            .map_err(|e| ChannelError::query(self.dst_chain().id(), e))?;

        // Check if a channel is expected to exist on destination chain
        // A channel must exist on destination chain for Ack and Confirm Tx-es to succeed
        if dst_channel.state_matches(&State::Uninitialized) {
            return Err(ChannelError::missing_channel_on_destination());
        }

        check_destination_channel_state(dst_channel_id, &dst_channel, &dst_expected_channel)?;

        Ok(dst_expected_channel)
    }

    pub fn update_channel_path_clients(&self) -> Result<Vec<MultihopProofHeights>, ChannelError> {
        let channel_id = self
            .a_side
            .channel_id()
            .ok_or(ChannelError::missing_local_channel_id())?;

        // Make sure the connection_hops are not 'None'
        let connection_hops =
            self.a_side
                .connection_hops()
                .ok_or(ChannelError::missing_local_connection_hops(
                    channel_id.clone(),
                    self.a_side.chain_id().clone(),
                ))?;

        // Get the source chain's latest height. This height will be used to query the key/commitment
        // proof in the sending chain.
        let query_height = self
            .src_chain()
            .query_latest_height()
            .map_err(|e| ChannelError::query(self.src_chain().id(), e))?;

        // This height will be provided as the target to update the client hosted by the next
        // chain in this channel path. The height must be equal to 'query_height' + 1, as any proofs
        // queried at height `query_height` can only be verified by having access to the application's
        // state Merkle Root (the AppHash) for height 'query_height', which is only included in the
        // subsequent block.
        let mut target_client_height = query_height.increment();

        // Store the heights at which the proofs must be queried after the clients in the channel
        // path are updated. Here, for the first chain in the channel path, store only the height
        // at which to query the proofs since the chain does not have to be queried for a consensus
        // state from a previous chain in the path.
        let mut proof_heights = vec![MultihopProofHeights::new(query_height, None)];

        // Get access to the registry to get or spawn chain handles
        let registry = get_global_registry();

        // Update the clients along the channel path from the sending chain (a_side)
        // towards the receiving chain (b_side), except for the client on the destination,
        // which will receive the MsgUpdateClient bundled with the main message to be sent.
        for conn_hop in connection_hops
            .hops
            .iter()
            .take(connection_hops.hops.len() - 1)
        {
            let hop_src_chain = registry
                .get_or_spawn(&conn_hop.src_chain_id)
                .map_err(ChannelError::spawn)?;

            let hop_dst_chain = registry
                .get_or_spawn(&conn_hop.dst_chain_id)
                .map_err(ChannelError::spawn)?;

            // Restore the client hosted by hop_dst_chain to track the state of hop_src_chain
            let client = ForeignClient::restore(
                conn_hop.connection().counterparty().client_id().clone(),
                hop_dst_chain.clone(),
                hop_src_chain.clone(),
            );

            // Build and send a MsgUpdateClient to update the client so that it tracks the
            // consensus state for the height 'target_client_height'
            client
                .build_update_client_and_send(QueryHeight::Specific(target_client_height), None)
                .map_err(|e| {
                    ChannelError::client_operation(client.id().clone(), hop_dst_chain.id(), e)
                })?;

            // Fetch the UpdateClient event which updates the client to track 'target_client_height'
            let maybe_update = client
                .fetch_update_client_event(target_client_height)
                .map_err(|e| {
                    ChannelError::client_operation(client.id().clone(), hop_dst_chain.id(), e)
                })?;

            // Retrieve the height at which the UpdateClient message was included in the chain.
            // This height can be used to query for the consensus state that corresponds to
            // `target_client_height`.
            let update_event_height = match maybe_update {
                Some((_, height)) => height,
                None => {
                    return Err(ChannelError::failed_channel_path_client_update(
                        client.id().clone(),
                        hop_dst_chain.id(),
                        channel_id.clone(),
                        self.a_side.chain_id().clone(),
                    ))
                }
            };

            proof_heights.push(MultihopProofHeights::new(
                // The height at which the consensus state for the previous chain was included
                // in this chain. Consensus proofs and other required proofs will be queried at
                // this height.
                update_event_height,
                // This is the consensus height from the previous chain that was received in an
                // update at height 'update_event_height'. This height will be used as the
                // consensus_height to query for when querying for the consensus state proof
                // at chain height 'update_event_height'.
                Some(target_client_height),
            ));

            // The height to use for updating the next client in the channel path. Not used if
            // the next chain is the channel path's destination chain. Allows for the verification
            // of proofs queried at height 'update_event_height'.
            target_client_height = update_event_height.increment();
        }

        Ok(proof_heights)
    }

    pub fn build_multihop_proofs(
        &self,
        proof_heights: &[MultihopProofHeights],
    ) -> Result<MsgMultihopProofs, ChannelError> {
        let src_channel_id = self
            .a_side
            .channel_id()
            .ok_or(ChannelError::missing_local_channel_id())?;

        let connection_hops =
            self.a_side
                .connection_hops()
                .ok_or(ChannelError::missing_local_connection_hops(
                    src_channel_id.clone(),
                    self.a_side.chain_id().clone(),
                ))?;

        if proof_heights.len() != connection_hops.hops.len() {
            return Err(ChannelError::missing_multihop_proof_heights(
                src_channel_id.clone(),
                self.src_chain().id().clone(),
            ));
        }

        let src_chain_query_height = QueryHeight::Specific(proof_heights[0].query_height());

        // Query channel proof in src_chain
        let (src_channel, maybe_channel_proof) = self
            .src_chain()
            .query_channel(
                QueryChannelRequest {
                    port_id: self.src_port_id().clone(),
                    channel_id: src_channel_id.clone(),
                    height: src_chain_query_height,
                },
                IncludeProof::Yes,
            )
            .map_err(|e| ChannelError::query(self.src_chain().id(), e))?;

        let Some(channel_proof) = maybe_channel_proof else {
            return Err(ChannelError::queried_proof_not_found());
        };

        let channel_proof_bytes =
            CommitmentProofBytes::try_from(channel_proof).map_err(ChannelError::malformed_proof)?;

        let key_path = vec![Path::ChannelEnds(ChannelEndsPath(
            self.src_port_id().clone(),
            src_channel_id.clone(),
        ))
        .to_string()];

        let store_prefix = self
            .src_chain()
            .query_commitment_prefix()
            .map_err(|e| ChannelError::chain_query(self.src_chain().id(), e))?;

        let prefixed_key = apply_prefix(&store_prefix, key_path);

        let key_proof = MultihopProof {
            proof: channel_proof_bytes.into_bytes(),
            value: src_channel.encode_vec(),
            prefixed_key: Some(prefixed_key),
        };

        let src_connection_id = connection_hops
            .hops
            .first()
            .ok_or(ChannelError::missing_connection_hops(
                src_channel_id.clone(),
                self.src_chain().id(),
            ))?
            .connection_id();

        let (src_connection, maybe_conn_proof) = self
            .src_chain()
            .query_connection(
                QueryConnectionRequest {
                    connection_id: src_connection_id.clone(),
                    height: src_chain_query_height,
                },
                IncludeProof::Yes,
            )
            .map_err(|e| ChannelError::query(self.src_chain().id(), e))?;

        let Some(conn_proof) = maybe_conn_proof else {
            return Err(ChannelError::queried_proof_not_found());
        };

        let conn_proof_bytes =
            CommitmentProofBytes::try_from(conn_proof).map_err(ChannelError::malformed_proof)?;

        // Path of connection on src_chain
        let connection_path =
            vec![Path::Connections(ConnectionsPath(src_connection_id.clone())).to_string()];

        let prefixed_key = apply_prefix(&store_prefix, connection_path);

        let src_connection_proof = MultihopProof {
            proof: conn_proof_bytes.into_bytes(),
            value: src_connection.encode_vec(),
            prefixed_key: Some(prefixed_key),
        };

        let mut connection_proofs = vec![src_connection_proof];
        let mut consensus_proofs: Vec<MultihopProof> = Vec::new();

        let registry = get_global_registry();

        for (proof_height, conn_hop) in proof_heights
            .iter()
            .skip(1)
            .zip(connection_hops.hops.iter().skip(1))
        {
            let hop_src_chain = registry
                .get_or_spawn(&conn_hop.src_chain_id.clone())
                .map_err(ChannelError::spawn)?;

            let query_height = QueryHeight::Specific(proof_height.query_height());

            let (hop_connection, maybe_conn_proof) = hop_src_chain
                .query_connection(
                    QueryConnectionRequest {
                        connection_id: conn_hop.connection_id().clone(),
                        height: query_height,
                    },
                    IncludeProof::Yes,
                )
                .map_err(|e| ChannelError::query(hop_src_chain.id(), e))?;

            let Some(conn_proof) = maybe_conn_proof else {
                return Err(ChannelError::queried_proof_not_found());
            };

            let conn_proof_bytes = CommitmentProofBytes::try_from(conn_proof)
                .map_err(ChannelError::malformed_proof)?;

            // Path of connection on hop_src_chain
            let connection_path =
                vec![
                    Path::Connections(ConnectionsPath(conn_hop.connection_id().clone()))
                        .to_string(),
                ];

            let prefixed_key = apply_prefix(&store_prefix, connection_path);

            let hop_connection_proof = MultihopProof {
                proof: conn_proof_bytes.into_bytes(),
                value: hop_connection.encode_vec(),
                prefixed_key: Some(prefixed_key),
            };

            let desired_consensus_height = proof_height.consensus_height().ok_or(
                ChannelError::missing_multihop_proof_heights(
                    src_channel_id.clone(),
                    self.src_chain().id(),
                ),
            )?;

            let (consensus_state, maybe_consensus_state_proof) = hop_src_chain
                .query_consensus_state(
                    QueryConsensusStateRequest {
                        client_id: conn_hop.connection().counterparty().client_id().clone(),
                        consensus_height: desired_consensus_height.clone(),
                        query_height,
                    },
                    IncludeProof::Yes,
                )
                .map_err(|e| ChannelError::query(hop_src_chain.id(), e))?;

            let Some(consensus_state_proof) = maybe_consensus_state_proof else {
                return Err(ChannelError::queried_proof_not_found());
            };

            let consensus_state_proof_bytes = CommitmentProofBytes::try_from(consensus_state_proof)
                .map_err(ChannelError::malformed_proof)?;

            // Path of consensus state on hop_src_chain
            let consensus_state_path = vec![Path::ClientConsensusState(ClientConsensusStatePath {
                client_id: conn_hop.connection().counterparty().client_id().clone(),
                epoch: desired_consensus_height.revision_number(),
                height: desired_consensus_height.revision_height(),
            })
            .to_string()];

            let prefixed_key = apply_prefix(&store_prefix, consensus_state_path);

            let hop_consensus_proof = MultihopProof {
                proof: consensus_state_proof_bytes.into_bytes(),
                value: consensus_state.encode_vec(),
                prefixed_key: Some(prefixed_key),
            };

            connection_proofs.push(hop_connection_proof);
            consensus_proofs.push(hop_consensus_proof);
        }

        connection_proofs.reverse();
        consensus_proofs.reverse();
        Ok(MsgMultihopProofs {
            key_proof: Some(key_proof),
            connection_proofs,
            consensus_proofs,
        })
    }

    pub fn build_multihop_chan_open_try(&self) -> Result<Vec<Any>, ChannelError> {
        // Source channel ID must be specified
        let src_channel_id = self
            .src_channel_id()
            .ok_or_else(ChannelError::missing_local_channel_id)?;

        // Channel must exist on source
        let (src_channel, _) = self
            .src_chain() //~ ChainA
            .query_channel(
                QueryChannelRequest {
                    port_id: self.src_port_id().clone(),
                    channel_id: src_channel_id.clone(),
                    height: QueryHeight::Latest,
                },
                IncludeProof::No,
            )
            .map_err(|e| ChannelError::query(self.src_chain().id(), e))?;

        // The channel end in src_chain must be in `Init` state
        if !src_channel.state_matches(&State::Init) {
            return Err(ChannelError::unexpected_channel_state(
                src_channel_id.clone(),
                State::Init,
                *src_channel.state(),
            ));
        }

        if src_channel.counterparty().port_id() != self.dst_port_id() {
            return Err(ChannelError::mismatch_port(
                self.dst_chain().id(),
                self.dst_port_id().clone(),
                self.src_chain().id(),
                src_channel.counterparty().port_id().clone(),
                src_channel_id.clone(),
            ));
        }

        self.dst_chain()
            .query_connection(
                QueryConnectionRequest {
                    connection_id: self.dst_connection_id().clone(),
                    height: QueryHeight::Latest,
                },
                IncludeProof::No,
            )
            .map_err(|e| ChannelError::query(self.dst_chain().id(), e))?;

        let query_height = self
            .src_chain()
            .query_latest_height()
            .map_err(|e| ChannelError::query(self.src_chain().id(), e))?;

        // let proofs = self
        //     .src_chain()
        //     .build_channel_proofs(self.src_port_id(), src_channel_id, query_height)
        //     .map_err(ChannelError::channel_proof)?;

        // Update the clients along the channel path and store the heights necessary for querying
        // multihop proofs. 'proof_heights' contains the height at which proofs should be queried,
        // ordered from the sending chain to the penultimate chain in the channel path. In order to
        // verify proofs queried at the height 'query_height'  stored in 'proof_query_heights', the
        // client on the chain that receives the proof must be updated to store the consensus state
        // for height 'query_height + 1'.
        let proof_heights = self.update_channel_path_clients()?;

        // Get the multihop proof heights for the chain from which the last hop originates, i.e,
        // the penultimate chain in the channel path.
        let last_hop_heights =
            proof_heights
                .last()
                .ok_or(ChannelError::missing_multihop_proof_heights(
                    src_channel_id.clone(),
                    self.src_chain().id(),
                ))?;

        // Build the message to update the client on the channel path's destination. The client
        // update height must greater than the height at which the proofs will be queried at the
        // penultimate chain, therefore the target update height is equal to the penultimate chain's
        // `query_height` + 1.
        let mut msgs =
            self.build_update_client_on_last_hop(last_hop_heights.query_height().increment())?;

        let multihop_proofs = self.build_multihop_proofs(&proof_heights)?;

        let multihop_proof_bytes = prost::Message::encode_to_vec(&multihop_proofs);

        // let multihop_proof_bytes = prost::Message::encode_to_vec(multihop_proofs).unwrap();
        // --------- IN PROGRESS BELOW --------- //

        let counterparty =
            Counterparty::new(self.src_port_id().clone(), self.src_channel_id().cloned());

        // Reuse the version that was either set on ChanOpenInit or overwritten by the application.
        let version = src_channel.version().clone();

        let proofs = ibc_relayer_types::proofs::Proofs::new(
            CommitmentProofBytes::try_from(multihop_proof_bytes).unwrap(),
            None,
            None,
            None,
            None,
            // proof_heights[0].query_height().increment(),
            last_hop_heights.query_height(),
        )
        .unwrap(); // FIXME

        println!("\n\n\n {:?} \n\n\n", proofs);

        let channel = ChannelEnd::new(
            State::TryOpen,
            *src_channel.ordering(),
            counterparty,
            vec![self.dst_connection_id().clone()],
            version,
            0,
        );

        // Get signer
        let signer = self
            .dst_chain()
            .get_signer()
            .map_err(|e| ChannelError::fetch_signer(self.dst_chain().id(), e))?;

        let previous_channel_id = if src_channel.counterparty().channel_id.is_none() {
            self.b_side.channel_id.clone()
        } else {
            src_channel.counterparty().channel_id.clone()
        };

        // Build the domain type message
        let new_msg = MsgChannelOpenTry {
            port_id: self.dst_port_id().clone(),
            previous_channel_id,
            counterparty_version: src_channel.version().clone(),
            channel,
            proofs,
            signer,
        };

        msgs.push(new_msg.to_any());
        Ok(msgs)
    }

    pub fn build_chan_open_try(&self) -> Result<Vec<Any>, ChannelError> {
        // Source channel ID must be specified
        let src_channel_id = self
            .src_channel_id()
            .ok_or_else(ChannelError::missing_local_channel_id)?;

        // Channel must exist on source
        let (src_channel, _) = self
            .src_chain()
            .query_channel(
                QueryChannelRequest {
                    port_id: self.src_port_id().clone(),
                    channel_id: src_channel_id.clone(),
                    height: QueryHeight::Latest,
                },
                IncludeProof::No,
            )
            .map_err(|e| ChannelError::query(self.src_chain().id(), e))?;

        // The channel end in src_chain must be in `Init` state
        if !src_channel.state_matches(&State::Init) {
            return Err(ChannelError::unexpected_channel_state(
                src_channel_id.clone(),
                State::Init,
                *src_channel.state(),
            ));
        }

        if src_channel.counterparty().port_id() != self.dst_port_id() {
            return Err(ChannelError::mismatch_port(
                self.dst_chain().id(),
                self.dst_port_id().clone(),
                self.src_chain().id(),
                src_channel.counterparty().port_id().clone(),
                src_channel_id.clone(),
            ));
        }

        // Connection must exist on destination
        self.dst_chain()
            .query_connection(
                QueryConnectionRequest {
                    connection_id: self.dst_connection_id().clone(),
                    height: QueryHeight::Latest,
                },
                IncludeProof::No,
            )
            .map_err(|e| ChannelError::query(self.dst_chain().id(), e))?;

        let query_height = self
            .src_chain()
            .query_latest_height()
            .map_err(|e| ChannelError::query(self.src_chain().id(), e))?;

        let proofs = self
            .src_chain()
            .build_channel_proofs(self.src_port_id(), src_channel_id, query_height)
            .map_err(ChannelError::channel_proof)?;

        // Build message(s) to update client on destination
        let mut msgs = self.build_update_client_on_dst(proofs.height())?;

        let counterparty =
            Counterparty::new(self.src_port_id().clone(), self.src_channel_id().cloned());

        // Reuse the version that was either set on ChanOpenInit or overwritten by the application.
        let version = src_channel.version().clone();

        let channel = ChannelEnd::new(
            State::TryOpen,
            *src_channel.ordering(),
            counterparty,
            vec![self.dst_connection_id().clone()],
            version,
            0,
        );

        // Get signer
        let signer = self
            .dst_chain()
            .get_signer()
            .map_err(|e| ChannelError::fetch_signer(self.dst_chain().id(), e))?;

        let previous_channel_id = if src_channel.counterparty().channel_id.is_none() {
            self.b_side.channel_id.clone()
        } else {
            src_channel.counterparty().channel_id.clone()
        };

        // Build the domain type message
        let new_msg = MsgChannelOpenTry {
            port_id: self.dst_port_id().clone(),
            previous_channel_id,
            counterparty_version: src_channel.version().clone(),
            channel,
            proofs,
            signer,
        };

        msgs.push(new_msg.to_any());
        Ok(msgs)
    }

    pub fn build_chan_open_try_and_send(&self) -> Result<IbcEvent, ChannelError> {
        let dst_msgs = if self.a_side.connection_hops.is_some() {
            self.build_multihop_chan_open_try()?
        } else {
            self.build_chan_open_try()?
        };

        let tm = TrackedMsgs::new_static(dst_msgs, "ChannelOpenTry");

        let events = self
            .dst_chain()
            .send_messages_and_wait_commit(tm)
            .map_err(|e| ChannelError::submit(self.dst_chain().id(), e))?;

        // Find the relevant event for channel open try
        let result = events
            .into_iter()
            .find(|events_with_height| {
                matches!(events_with_height.event, IbcEvent::OpenTryChannel(_))
                    || matches!(events_with_height.event, IbcEvent::ChainError(_))
            })
            .ok_or_else(|| {
                ChannelError::missing_event("no chan try event was in the response".to_string())
            })?;

        match &result.event {
            IbcEvent::OpenTryChannel(_) => {
                info!("🎊  {} => {}", self.dst_chain().id(), result);
                Ok(result.event)
            }
            IbcEvent::ChainError(e) => Err(ChannelError::tx_response(e.clone())),
            _ => Err(ChannelError::invalid_event(result.event)),
        }
    }

    pub fn build_chan_open_ack(&self) -> Result<Vec<Any>, ChannelError> {
        // Source and destination channel IDs must be specified
        let src_channel_id = self
            .src_channel_id()
            .ok_or_else(ChannelError::missing_local_channel_id)?;
        let dst_channel_id = self
            .dst_channel_id()
            .ok_or_else(ChannelError::missing_counterparty_channel_id)?;

        // Check that the destination chain will accept the Ack message
        self.validated_expected_channel(ChannelMsgType::OpenAck)?;

        // Channel must exist on source
        let (src_channel, _) = self
            .src_chain()
            .query_channel(
                QueryChannelRequest {
                    port_id: self.src_port_id().clone(),
                    channel_id: src_channel_id.clone(),
                    height: QueryHeight::Latest,
                },
                IncludeProof::No,
            )
            .map_err(|e| ChannelError::query(self.src_chain().id(), e))?;

        // Connection must exist on destination
        self.dst_chain()
            .query_connection(
                QueryConnectionRequest {
                    connection_id: self.dst_connection_id().clone(),
                    height: QueryHeight::Latest,
                },
                IncludeProof::No,
            )
            .map_err(|e| ChannelError::query(self.dst_chain().id(), e))?;

        let query_height = self
            .src_chain()
            .query_latest_height()
            .map_err(|e| ChannelError::query(self.src_chain().id(), e))?;

        let proofs = self
            .src_chain()
            .build_channel_proofs(self.src_port_id(), src_channel_id, query_height)
            .map_err(ChannelError::channel_proof)?;

        // Build message(s) to update client on destination
        let mut msgs = self.build_update_client_on_dst(proofs.height())?;

        // Get signer
        let signer = self
            .dst_chain()
            .get_signer()
            .map_err(|e| ChannelError::fetch_signer(self.dst_chain().id(), e))?;

        // Build the domain type message
        let new_msg = MsgChannelOpenAck {
            port_id: self.dst_port_id().clone(),
            channel_id: dst_channel_id.clone(),
            counterparty_channel_id: src_channel_id.clone(),
            counterparty_version: src_channel.version().clone(),
            proofs,
            signer,
        };

        msgs.push(new_msg.to_any());
        Ok(msgs)
    }

    pub fn build_chan_open_ack_and_send(&self) -> Result<IbcEvent, ChannelError> {
        fn do_build_chan_open_ack_and_send<ChainA: ChainHandle, ChainB: ChainHandle>(
            channel: &Channel<ChainA, ChainB>,
        ) -> Result<IbcEvent, ChannelError> {
            let dst_msgs = channel.build_chan_open_ack()?;

            let tm = TrackedMsgs::new_static(dst_msgs, "ChannelOpenAck");

            let events = channel
                .dst_chain()
                .send_messages_and_wait_commit(tm)
                .map_err(|e| ChannelError::submit(channel.dst_chain().id(), e))?;

            // Find the relevant event for channel open ack
            let result = events
                .into_iter()
                .find(|event_with_height| {
                    matches!(event_with_height.event, IbcEvent::OpenAckChannel(_))
                        || matches!(event_with_height.event, IbcEvent::ChainError(_))
                })
                .ok_or_else(|| {
                    ChannelError::missing_event("no chan ack event was in the response".to_string())
                })?;

            match &result.event {
                IbcEvent::OpenAckChannel(_) => {
                    info!("🎊  {} => {}", channel.dst_chain().id(), result);
                    Ok(result.event)
                }
                IbcEvent::ChainError(e) => Err(ChannelError::tx_response(e.clone())),
                _ => Err(ChannelError::invalid_event(result.event)),
            }
        }

        do_build_chan_open_ack_and_send(self).map_err(|e| {
            error!("failed ChanOpenAck {}: {}", self.b_side, e);
            e
        })
    }

    pub fn build_chan_open_confirm(&self) -> Result<Vec<Any>, ChannelError> {
        // Source and destination channel IDs must be specified
        let src_channel_id = self
            .src_channel_id()
            .ok_or_else(ChannelError::missing_local_channel_id)?;
        let dst_channel_id = self
            .dst_channel_id()
            .ok_or_else(ChannelError::missing_counterparty_channel_id)?;

        // Check that the destination chain will accept the message
        self.validated_expected_channel(ChannelMsgType::OpenConfirm)?;

        // Channel must exist on source
        self.src_chain()
            .query_channel(
                QueryChannelRequest {
                    port_id: self.src_port_id().clone(),
                    channel_id: src_channel_id.clone(),
                    height: QueryHeight::Latest,
                },
                IncludeProof::No,
            )
            .map_err(|e| ChannelError::query(self.src_chain().id(), e))?;

        // Connection must exist on destination
        self.dst_chain()
            .query_connection(
                QueryConnectionRequest {
                    connection_id: self.dst_connection_id().clone(),
                    height: QueryHeight::Latest,
                },
                IncludeProof::No,
            )
            .map_err(|e| ChannelError::query(self.dst_chain().id(), e))?;

        let query_height = self
            .src_chain()
            .query_latest_height()
            .map_err(|e| ChannelError::query(self.src_chain().id(), e))?;

        let proofs = self
            .src_chain()
            .build_channel_proofs(self.src_port_id(), src_channel_id, query_height)
            .map_err(ChannelError::channel_proof)?;

        // Build message(s) to update client on destination
        let mut msgs = self.build_update_client_on_dst(proofs.height())?;

        // Get signer
        let signer = self
            .dst_chain()
            .get_signer()
            .map_err(|e| ChannelError::fetch_signer(self.dst_chain().id(), e))?;

        // Build the domain type message
        let new_msg = MsgChannelOpenConfirm {
            port_id: self.dst_port_id().clone(),
            channel_id: dst_channel_id.clone(),
            proofs,
            signer,
        };

        msgs.push(new_msg.to_any());
        Ok(msgs)
    }

    pub fn build_chan_open_confirm_and_send(&self) -> Result<IbcEvent, ChannelError> {
        fn do_build_chan_open_confirm_and_send<ChainA: ChainHandle, ChainB: ChainHandle>(
            channel: &Channel<ChainA, ChainB>,
        ) -> Result<IbcEvent, ChannelError> {
            let dst_msgs = channel.build_chan_open_confirm()?;

            let tm = TrackedMsgs::new_static(dst_msgs, "ChannelOpenConfirm");
            let events = channel
                .dst_chain()
                .send_messages_and_wait_commit(tm)
                .map_err(|e| ChannelError::submit(channel.dst_chain().id(), e))?;

            // Find the relevant event for channel open confirm
            let result = events
                .into_iter()
                .find(|event_with_height| {
                    matches!(event_with_height.event, IbcEvent::OpenConfirmChannel(_))
                        || matches!(event_with_height.event, IbcEvent::ChainError(_))
                })
                .ok_or_else(|| {
                    ChannelError::missing_event(
                        "no chan confirm event was in the response".to_string(),
                    )
                })?;

            match &result.event {
                IbcEvent::OpenConfirmChannel(_) => {
                    info!("🎊  {} => {}", channel.dst_chain().id(), result);
                    Ok(result.event)
                }
                IbcEvent::ChainError(e) => Err(ChannelError::tx_response(e.clone())),
                _ => Err(ChannelError::invalid_event(result.event)),
            }
        }

        do_build_chan_open_confirm_and_send(self).map_err(|e| {
            error!("failed ChanOpenConfirm {}: {}", self.b_side, e);
            e
        })
    }

    pub fn build_chan_close_init(&self) -> Result<Vec<Any>, ChannelError> {
        // Destination channel ID must be specified
        let dst_channel_id = self
            .dst_channel_id()
            .ok_or_else(ChannelError::missing_counterparty_channel_id)?;

        // Channel must exist on destination
        self.dst_chain()
            .query_channel(
                QueryChannelRequest {
                    port_id: self.dst_port_id().clone(),
                    channel_id: dst_channel_id.clone(),
                    height: QueryHeight::Latest,
                },
                IncludeProof::No,
            )
            .map_err(|e| ChannelError::query(self.dst_chain().id(), e))?;

        let signer = self
            .dst_chain()
            .get_signer()
            .map_err(|e| ChannelError::fetch_signer(self.dst_chain().id(), e))?;

        // Build the domain type message
        let new_msg = MsgChannelCloseInit {
            port_id: self.dst_port_id().clone(),
            channel_id: dst_channel_id.clone(),
            signer,
        };

        Ok(vec![new_msg.to_any()])
    }

    pub fn build_chan_close_init_and_send(&self) -> Result<IbcEvent, ChannelError> {
        let dst_msgs = self.build_chan_close_init()?;

        let tm = TrackedMsgs::new_static(dst_msgs, "ChannelCloseInit");

        let events = self
            .dst_chain()
            .send_messages_and_wait_commit(tm)
            .map_err(|e| ChannelError::submit(self.dst_chain().id(), e))?;

        // Find the relevant event for channel close init
        let result = events
            .into_iter()
            .find(|event_with_height| {
                matches!(event_with_height.event, IbcEvent::CloseInitChannel(_))
                    || matches!(event_with_height.event, IbcEvent::ChainError(_))
            })
            .ok_or_else(|| {
                ChannelError::missing_event("no chan init event was in the response".to_string())
            })?;

        match &result.event {
            IbcEvent::CloseInitChannel(_) => {
                info!("👋 {} => {}", self.dst_chain().id(), result);
                Ok(result.event)
            }
            IbcEvent::ChainError(e) => Err(ChannelError::tx_response(e.clone())),
            _ => Err(ChannelError::invalid_event(result.event)),
        }
    }

    pub fn build_chan_close_confirm(&self) -> Result<Vec<Any>, ChannelError> {
        // Source and destination channel IDs must be specified
        let src_channel_id = self
            .src_channel_id()
            .ok_or_else(ChannelError::missing_local_channel_id)?;
        let dst_channel_id = self
            .dst_channel_id()
            .ok_or_else(ChannelError::missing_counterparty_channel_id)?;

        // Check that the destination chain will accept the message
        self.validated_expected_channel(ChannelMsgType::CloseConfirm)?;

        // Channel must exist on source
        self.src_chain()
            .query_channel(
                QueryChannelRequest {
                    port_id: self.src_port_id().clone(),
                    channel_id: src_channel_id.clone(),
                    height: QueryHeight::Latest,
                },
                IncludeProof::No,
            )
            .map_err(|e| ChannelError::query(self.src_chain().id(), e))?;

        // Connection must exist on destination
        self.dst_chain()
            .query_connection(
                QueryConnectionRequest {
                    connection_id: self.dst_connection_id().clone(),
                    height: QueryHeight::Latest,
                },
                IncludeProof::No,
            )
            .map_err(|e| ChannelError::query(self.dst_chain().id(), e))?;

        let query_height = self
            .src_chain()
            .query_latest_height()
            .map_err(|e| ChannelError::query(self.src_chain().id(), e))?;

        let proofs = self
            .src_chain()
            .build_channel_proofs(self.src_port_id(), src_channel_id, query_height)
            .map_err(ChannelError::channel_proof)?;

        // Build message(s) to update client on destination
        let mut msgs = self.build_update_client_on_dst(proofs.height())?;

        // Get signer
        let signer = self
            .dst_chain()
            .get_signer()
            .map_err(|e| ChannelError::fetch_signer(self.dst_chain().id(), e))?;

        // Build the domain type message
        let new_msg = MsgChannelCloseConfirm {
            port_id: self.dst_port_id().clone(),
            channel_id: dst_channel_id.clone(),
            proofs,
            signer,
            counterparty_upgrade_sequence: 0,
        };

        msgs.push(new_msg.to_any());
        Ok(msgs)
    }

    pub fn build_chan_close_confirm_and_send(&self) -> Result<IbcEvent, ChannelError> {
        let dst_msgs = self.build_chan_close_confirm()?;

        let tm = TrackedMsgs::new_static(dst_msgs, "ChannelCloseConfirm");

        let events = self
            .dst_chain()
            .send_messages_and_wait_commit(tm)
            .map_err(|e| ChannelError::submit(self.dst_chain().id(), e))?;

        // Find the relevant event for channel close confirm
        let result = events
            .into_iter()
            .find(|event_with_height| {
                matches!(event_with_height.event, IbcEvent::CloseConfirmChannel(_))
                    || matches!(event_with_height.event, IbcEvent::ChainError(_))
            })
            .ok_or_else(|| {
                ChannelError::missing_event("no chan confirm event was in the response".to_string())
            })?;

        match &result.event {
            IbcEvent::CloseConfirmChannel(_) => {
                info!("👋 {} => {}", self.dst_chain().id(), result);
                Ok(result.event)
            }
            IbcEvent::ChainError(e) => Err(ChannelError::tx_response(e.clone())),
            _ => Err(ChannelError::invalid_event(result.event)),
        }
    }

    pub fn map_chain<ChainC: ChainHandle, ChainD: ChainHandle>(
        self,
        mapper_a: impl Fn(ChainA) -> ChainC,
        mapper_b: impl Fn(ChainB) -> ChainD,
    ) -> Channel<ChainC, ChainD> {
        Channel {
            ordering: self.ordering,
            a_side: self.a_side.map_chain(mapper_a),
            b_side: self.b_side.map_chain(mapper_b),
            connection_delay: self.connection_delay,
        }
    }
}

pub fn extract_channel_id(event: &IbcEvent) -> Result<&ChannelId, ChannelError> {
    match event {
        IbcEvent::OpenInitChannel(ev) => ev.channel_id(),
        IbcEvent::OpenTryChannel(ev) => ev.channel_id(),
        IbcEvent::OpenAckChannel(ev) => ev.channel_id(),
        IbcEvent::OpenConfirmChannel(ev) => ev.channel_id(),
        _ => None,
    }
    .ok_or_else(|| ChannelError::missing_event("cannot extract channel_id from result".to_string()))
}

/// Enumeration of proof carrying ICS4 message, helper for relayer.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ChannelMsgType {
    OpenTry,
    OpenAck,
    OpenConfirm,
    CloseConfirm,
}

fn check_destination_channel_state(
    channel_id: &ChannelId,
    existing_channel: &ChannelEnd,
    expected_channel: &ChannelEnd,
) -> Result<(), ChannelError> {
    let good_connection_hops =
        existing_channel.connection_hops() == expected_channel.connection_hops();

    // TODO: Refactor into a method
    let good_state = *existing_channel.state() as u32 <= *expected_channel.state() as u32;
    let good_channel_port_ids = existing_channel.counterparty().channel_id().is_none()
        || existing_channel.counterparty().channel_id()
            == expected_channel.counterparty().channel_id()
            && existing_channel.counterparty().port_id()
                == expected_channel.counterparty().port_id();

    // TODO: Check versions

    if good_state && good_connection_hops && good_channel_port_ids {
        Ok(())
    } else {
        Err(ChannelError::channel_already_exist(channel_id.clone()))
    }
}
