#![allow(clippy::borrowed_box)]

use core::time::Duration;
use prost_types::Any;
use serde::Serialize;
use tracing::{debug, error, info, warn};

use ibc::events::IbcEvent;
use ibc::ics04_channel::channel::{ChannelEnd, Counterparty, IdentifiedChannelEnd, Order, State};
use ibc::ics04_channel::msgs::chan_close_confirm::MsgChannelCloseConfirm;
use ibc::ics04_channel::msgs::chan_close_init::MsgChannelCloseInit;
use ibc::ics04_channel::msgs::chan_open_ack::MsgChannelOpenAck;
use ibc::ics04_channel::msgs::chan_open_confirm::MsgChannelOpenConfirm;
use ibc::ics04_channel::msgs::chan_open_init::MsgChannelOpenInit;
use ibc::ics04_channel::msgs::chan_open_try::MsgChannelOpenTry;
use ibc::ics24_host::identifier::{ChainId, ChannelId, ClientId, ConnectionId, PortId};
use ibc::tx_msg::Msg;
use ibc::Height;
use ibc_proto::ibc::core::channel::v1::QueryConnectionChannelsRequest;

use crate::chain::counterparty::{channel_connection_client, channel_state_on_destination};
use crate::chain::handle::ChainHandle;
use crate::connection::Connection;
use crate::foreign_client::ForeignClient;
use crate::object::Channel as WorkerChannelObject;
use crate::supervisor::error::Error as SupervisorError;
use crate::util::retry::retry_with_index;
use crate::util::retry::RetryResult;

pub mod error;
pub use error::ChannelError;

mod retry_strategy {
    use core::time::Duration;

    use retry::delay::Fibonacci;

    use crate::util::retry::clamp_total;

    // Default parameters for the retrying mechanism
    const MAX_DELAY: Duration = Duration::from_secs(60); // 1 minute
    const MAX_TOTAL_DELAY: Duration = Duration::from_secs(10 * 60); // 10 minutes
    const INITIAL_DELAY: Duration = Duration::from_secs(1); // 1 second

    pub fn default() -> impl Iterator<Item = Duration> {
        clamp_total(Fibonacci::from(INITIAL_DELAY), MAX_DELAY, MAX_TOTAL_DELAY)
    }
}

pub fn from_retry_error(e: retry::Error<ChannelError>, description: String) -> ChannelError {
    match e {
        retry::Error::Operation {
            error,
            total_delay,
            tries,
        } => {
            let detail = error::ChannelErrorDetail::MaxRetry(error::MaxRetrySubdetail {
                description,
                tries,
                total_delay,
                source: Box::new(error.0),
            });
            ChannelError(detail, error.1)
        }
        retry::Error::Internal(reason) => ChannelError::retry_internal(reason),
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ChannelSide<Chain: ChainHandle> {
    pub chain: Chain,
    client_id: ClientId,
    connection_id: ConnectionId,
    port_id: PortId,
    channel_id: Option<ChannelId>,
}

impl<Chain: ChainHandle> ChannelSide<Chain> {
    pub fn new(
        chain: Chain,
        client_id: ClientId,
        connection_id: ConnectionId,
        port_id: PortId,
        channel_id: Option<ChannelId>,
    ) -> ChannelSide<Chain> {
        Self {
            chain,
            client_id,
            connection_id,
            port_id,
            channel_id,
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

    pub fn port_id(&self) -> &PortId {
        &self.port_id
    }

    pub fn channel_id(&self) -> Option<&ChannelId> {
        self.channel_id.as_ref()
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Channel<ChainA: ChainHandle, ChainB: ChainHandle> {
    pub ordering: Order,
    pub a_side: ChannelSide<ChainA>,
    pub b_side: ChannelSide<ChainB>,
    pub connection_delay: Duration,
    pub version: Option<String>,
}

impl<ChainA: ChainHandle, ChainB: ChainHandle> Channel<ChainA, ChainB> {
    /// Creates a new channel on top of the existing connection. If the channel is not already
    /// set-up on both sides of the connection, this functions also fulfils the channel handshake.
    pub fn new(
        connection: Connection<ChainA, ChainB>,
        ordering: Order,
        a_port: PortId,
        b_port: PortId,
        version: Option<String>,
    ) -> Result<Self, ChannelError> {
        let b_side_chain = connection.dst_chain();
        let version = version.unwrap_or(
            b_side_chain
                .module_version(&b_port)
                .map_err(|e| ChannelError::query(b_side_chain.id(), e))?,
        );

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
                a_port,
                Default::default(),
            ),
            b_side: ChannelSide::new(
                connection.dst_chain(),
                connection.dst_client_id().clone(),
                dst_connection_id.clone(),
                b_port,
                Default::default(),
            ),
            connection_delay: connection.delay_period,
            version: Some(version),
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
            .channel_attributes()
            .ok_or_else(|| ChannelError::invalid_event(channel_open_event.clone()))?;

        let port_id = channel_event_attributes.port_id.clone();
        let channel_id = channel_event_attributes.channel_id.clone();

        let version = counterparty_chain
            .module_version(&channel_event_attributes.counterparty_port_id)
            .map_err(|e| ChannelError::query(counterparty_chain.id(), e))?;

        let connection_id = channel_event_attributes.connection_id.clone();
        let connection = chain
            .query_connection(&connection_id, Height::zero())
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
                connection_id,
                port_id,
                channel_id,
            ),
            b_side: ChannelSide::new(
                counterparty_chain,
                connection.counterparty().client_id().clone(),
                counterparty_connection_id.clone(),
                channel_event_attributes.counterparty_port_id.clone(),
                channel_event_attributes.counterparty_channel_id.clone(),
            ),
            connection_delay: connection.delay_period(),
            // The event does not include the version.
            // The message handlers `build_chan_open..` determine the version from channel query.
            version: Some(version),
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
        let a_channel = chain
            .query_channel(&channel.src_port_id, &channel.src_channel_id, height)
            .map_err(ChannelError::relayer)?;

        let a_connection_id = a_channel.connection_hops().first().ok_or_else(|| {
            ChannelError::supervisor(SupervisorError::missing_connection_hops(
                channel.src_channel_id.clone(),
                chain.id(),
            ))
        })?;

        let a_connection = chain
            .query_connection(a_connection_id, Height::zero())
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
                channel.src_port_id.clone(),
                Some(channel.src_channel_id.clone()),
            ),
            b_side: ChannelSide::new(
                counterparty_chain.clone(),
                a_connection.counterparty().client_id().clone(),
                b_connection_id.clone(),
                a_channel.remote.port_id.clone(),
                a_channel.remote.channel_id.clone(),
            ),
            connection_delay: a_connection.delay_period(),
            version: Some(a_channel.version.clone()),
        };

        if a_channel.state_matches(&State::Init) && a_channel.remote.channel_id.is_none() {
            let req = QueryConnectionChannelsRequest {
                connection: b_connection_id.to_string(),
                pagination: ibc_proto::cosmos::base::query::pagination::all(),
            };

            let channels: Vec<IdentifiedChannelEnd> = counterparty_chain
                .query_connection_channels(req)
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

    pub fn flipped(&self) -> Channel<ChainB, ChainA> {
        Channel {
            ordering: self.ordering,
            a_side: self.b_side.clone(),
            b_side: self.a_side.clone(),
            connection_delay: self.connection_delay,
            version: self.version.clone(),
        }
    }

    fn do_chan_open_init_and_send(&mut self) -> Result<(), ChannelError> {
        let event = self.flipped().build_chan_open_init_and_send()?;

        info!("done {} => {:#?}\n", self.src_chain().id(), event);

        let channel_id = extract_channel_id(&event)?;
        self.a_side.channel_id = Some(channel_id.clone());
        info!("successfully opened init channel");

        Ok(())
    }

    // Check that the channel was created on a_chain
    fn do_chan_open_init_and_send_with_retry(&mut self) -> Result<(), ChannelError> {
        retry_with_index(retry_strategy::default(), |_| {
            self.do_chan_open_init_and_send()
        })
        .map_err(|err| {
            error!("failed to open channel after {} retries", err);

            from_retry_error(
                err,
                format!("Failed to finish channel open init for {:?}", self),
            )
        })?;

        Ok(())
    }

    fn do_chan_open_try_and_send(&mut self) -> Result<(), ChannelError> {
        let event = self.build_chan_open_try_and_send().map_err(|e| {
            error!("Failed ChanTry {:?}: {:?}", self.b_side, e);
            e
        })?;

        let channel_id = extract_channel_id(&event)?;
        self.b_side.channel_id = Some(channel_id.clone());

        println!("done {} => {:#?}\n", self.dst_chain().id(), event);
        Ok(())
    }

    fn do_chan_open_try_and_send_with_retry(&mut self) -> Result<(), ChannelError> {
        retry_with_index(retry_strategy::default(), |_| {
            self.do_chan_open_try_and_send()
        })
        .map_err(|err| {
            error!("failed to open channel after {} retries", err);

            from_retry_error(
                err,
                format!("Failed to finish channel open try for {:?}", self),
            )
        })?;

        Ok(())
    }

    /// Sends the last two steps, consisting of `Ack` and `Confirm`
    /// messages, for finalizing the channel open handshake.
    ///
    /// Assumes that the channel open handshake was previously
    /// started (with `Init` & `Try` steps).
    ///
    /// Returns `Ok` when both channel ends are in state `Open`.
    /// Also returns `Ok` if the channel is undergoing a closing handshake.
    ///
    /// An `Err` can signal two cases:
    ///     - the common-case flow for the handshake protocol was interrupted,
    ///         e.g., by a competing relayer.
    ///     - Rpc problems (a query or submitting a tx failed).
    /// In both `Err` cases, there should be retry calling this method.
    fn do_chan_open_finalize(&self) -> Result<(), ChannelError> {
        fn query_channel_states<ChainA: ChainHandle, ChainB: ChainHandle>(
            channel: &Channel<ChainA, ChainB>,
        ) -> Result<(State, State), ChannelError> {
            let src_channel_id = channel
                .src_channel_id()
                .ok_or_else(ChannelError::missing_local_channel_id)?;

            let dst_channel_id = channel
                .dst_channel_id()
                .ok_or_else(ChannelError::missing_counterparty_connection)?;

            debug!(
                "do_chan_open_finalize for src_channel_id: {}, dst_channel_id: {}",
                src_channel_id, dst_channel_id
            );

            // Continue loop if query error
            let a_channel = channel
                .src_chain()
                .query_channel(channel.src_port_id(), src_channel_id, Height::zero())
                .map_err(|e| {
                    ChannelError::handshake_finalize(
                        channel.src_port_id().clone(),
                        src_channel_id.clone(),
                        channel.src_chain().id(),
                        e,
                    )
                })?;

            let b_channel = channel
                .dst_chain()
                .query_channel(channel.dst_port_id(), dst_channel_id, Height::zero())
                .map_err(|e| {
                    ChannelError::handshake_finalize(
                        channel.dst_port_id().clone(),
                        dst_channel_id.clone(),
                        channel.dst_chain().id(),
                        e,
                    )
                })?;

            Ok((*a_channel.state(), *b_channel.state()))
        }

        fn expect_channel_states<ChainA: ChainHandle, ChainB: ChainHandle>(
            ctx: &Channel<ChainA, ChainB>,
            a1: State,
            b1: State,
        ) -> Result<(), ChannelError> {
            let (a2, b2) = query_channel_states(ctx)?;

            if (a1, b1) == (a2, b2) {
                Ok(())
            } else {
                warn!(
                    "expected channels to progress to states {}, {}), instead got ({}, {})",
                    a1, b1, a2, b2
                );

                debug!("returning PartialOpenHandshake to retry");

                // One more step (confirm) left.
                // Returning error signals that the caller should retry.
                Err(ChannelError::partial_open_handshake(a1, b1))
            }
        }

        let (a_state, b_state) = query_channel_states(self)?;
        debug!(
            "do_chan_open_finalize with channel states: {}, {}",
            a_state, b_state
        );

        match (a_state, b_state) {
            // Handle sending the Ack message to the source chain,
            // then the Confirm message to the destination.
            (State::Init, State::TryOpen) | (State::TryOpen, State::TryOpen) => {
                self.flipped().build_chan_open_ack_and_send()?;

                expect_channel_states(self, State::Open, State::TryOpen)?;

                self.build_chan_open_confirm_and_send()?;

                expect_channel_states(self, State::Open, State::Open)?;

                Ok(())
            }

            // Handle sending the Ack message to the destination chain,
            // then the Confirm to the source chain.
            (State::TryOpen, State::Init) => {
                self.flipped().build_chan_open_ack_and_send()?;

                expect_channel_states(self, State::TryOpen, State::Open)?;

                self.flipped().build_chan_open_confirm_and_send()?;

                expect_channel_states(self, State::Open, State::Open)?;

                Ok(())
            }

            // Handle sending the Confirm message to the destination chain.
            (State::Open, State::TryOpen) => {
                self.build_chan_open_confirm_and_send()?;

                expect_channel_states(self, State::Open, State::Open)?;

                Ok(())
            }

            // Send Confirm to the source chain.
            (State::TryOpen, State::Open) => {
                self.flipped().build_chan_open_confirm_and_send()?;

                expect_channel_states(self, State::Open, State::Open)?;

                Ok(())
            }

            (State::Open, State::Open) => {
                info!("channel handshake already finished for {:#?}\n", self);
                Ok(())
            }

            // In all other conditions, return Ok, since the channel open handshake does not apply.
            _ => Ok(()),
        }
    }

    /// Takes a partially open channel and finalizes the open handshake protocol.
    ///
    /// Pre-condition: the channel identifiers are established on both ends
    ///   (i.e., `OpenInit` and `OpenTry` have executed previously for this channel).
    ///
    /// Post-condition: the channel state is `Open` on both ends if successful.
    fn do_chan_open_finalize_with_retry(&self) -> Result<(), ChannelError> {
        retry_with_index(retry_strategy::default(), |_| self.do_chan_open_finalize()).map_err(
            |err| {
                error!("failed to open channel after {} retries", err);
                from_retry_error(
                    err,
                    format!("Failed to finish channel handshake for {:?}", self),
                )
            },
        )?;

        Ok(())
    }

    /// Executes the channel handshake protocol (ICS004)
    fn handshake(&mut self) -> Result<(), ChannelError> {
        self.do_chan_open_init_and_send_with_retry()?;
        self.do_chan_open_try_and_send_with_retry()?;
        self.do_chan_open_finalize_with_retry()
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

    pub fn handshake_step(&mut self, state: State) -> Result<Vec<IbcEvent>, ChannelError> {
        match (state, self.counterparty_state()?) {
            (State::Init, State::Uninitialized) => Ok(vec![self.build_chan_open_try_and_send()?]),
            (State::Init, State::Init) => Ok(vec![self.build_chan_open_try_and_send()?]),
            (State::TryOpen, State::Init) => Ok(vec![self.build_chan_open_ack_and_send()?]),
            (State::TryOpen, State::TryOpen) => Ok(vec![self.build_chan_open_ack_and_send()?]),
            (State::Open, State::TryOpen) => Ok(vec![self.build_chan_open_confirm_and_send()?]),
            _ => Ok(vec![]),
        }
    }

    pub fn step_state(&mut self, state: State, index: u64) -> RetryResult<(), u64> {
        let done = '🥳';

        match self.handshake_step(state) {
            Err(e) => {
                error!("Failed Chan{:?} with error: {}", state, e);
                RetryResult::Retry(index)
            }
            Ok(ev) => {
                debug!("{} => {:#?}\n", done, ev);
                RetryResult::Ok(())
            }
        }
    }

    pub fn step_event(&mut self, event: IbcEvent, index: u64) -> RetryResult<(), u64> {
        let state = match event {
            IbcEvent::OpenInitChannel(_) => State::Init,
            IbcEvent::OpenTryChannel(_) => State::TryOpen,
            IbcEvent::OpenAckChannel(_) => State::Open,
            IbcEvent::OpenConfirmChannel(_) => State::Open,
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

        client.build_update_client(height).map_err(|e| {
            ChannelError::client_operation(self.dst_client_id().clone(), self.dst_chain().id(), e)
        })
    }

    /// Returns the channel version if already set, otherwise it queries the destination chain
    /// for the destination port's version.
    /// Note: This query is currently not available and it is hardcoded in the `module_version()`
    /// to be `ics20-1` for `transfer` port.
    pub fn dst_version(&self) -> Result<String, ChannelError> {
        Ok(self.version.clone().unwrap_or(
            self.dst_chain()
                .module_version(self.dst_port_id())
                .map_err(|e| ChannelError::query(self.dst_chain().id(), e))?,
        ))
    }

    /// Returns the channel version if already set, otherwise it queries the source chain
    /// for the source port's version.
    pub fn src_version(&self) -> Result<String, ChannelError> {
        Ok(self.version.clone().unwrap_or(
            self.src_chain()
                .module_version(self.src_port_id())
                .map_err(|e| ChannelError::query(self.src_chain().id(), e))?,
        ))
    }

    pub fn build_chan_open_init(&self) -> Result<Vec<Any>, ChannelError> {
        let signer = self
            .dst_chain()
            .get_signer()
            .map_err(|e| ChannelError::query(self.dst_chain().id(), e))?;

        let counterparty = Counterparty::new(self.src_port_id().clone(), None);

        let channel = ChannelEnd::new(
            State::Init,
            self.ordering,
            counterparty,
            vec![self.dst_connection_id().clone()],
            self.dst_version()?,
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

        let events = self
            .dst_chain()
            .send_messages_and_wait_commit(dst_msgs)
            .map_err(|e| ChannelError::submit(self.dst_chain().id(), e))?;

        // Find the relevant event for channel open init
        let result = events
            .into_iter()
            .find(|event| {
                matches!(event, IbcEvent::OpenInitChannel(_))
                    || matches!(event, IbcEvent::ChainError(_))
            })
            .ok_or_else(|| {
                ChannelError::missing_event("no chan init event was in the response".to_string())
            })?;

        match result {
            IbcEvent::OpenInitChannel(_) => Ok(result),
            IbcEvent::ChainError(e) => Err(ChannelError::tx_response(e)),
            _ => Err(ChannelError::invalid_event(result)),
        }
    }

    /// Retrieves the channel from destination and compares against the expected channel
    /// built from the message type (`msg_type`) and options (`opts`).
    /// If the expected and the destination channels are compatible, it returns the expected channel
    /// Source and destination channel IDs must be specified.
    fn validated_expected_channel(
        &self,
        msg_type: ChannelMsgType,
    ) -> Result<ChannelEnd, ChannelError> {
        // Destination channel ID must be specified
        let dst_channel_id = self
            .dst_channel_id()
            .ok_or_else(ChannelError::missing_counterparty_channel_id)?;

        // If there is a channel present on the destination chain, it should look like this:
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
            self.dst_version()?,
        );

        // Retrieve existing channel
        let dst_channel = self
            .dst_chain()
            .query_channel(self.dst_port_id(), dst_channel_id, Height::zero())
            .map_err(|e| ChannelError::query(self.dst_chain().id(), e))?;

        // Check if a channel is expected to exist on destination chain
        // A channel must exist on destination chain for Ack and Confirm Tx-es to succeed
        if dst_channel.state_matches(&State::Uninitialized) {
            return Err(ChannelError::missing_channel_on_destination());
        }

        check_destination_channel_state(
            dst_channel_id.clone(),
            dst_channel,
            dst_expected_channel.clone(),
        )?;

        Ok(dst_expected_channel)
    }

    pub fn build_chan_open_try(&self) -> Result<Vec<Any>, ChannelError> {
        // Source channel ID must be specified
        let src_channel_id = self
            .src_channel_id()
            .ok_or_else(ChannelError::missing_local_channel_id)?;

        // Channel must exist on source
        let src_channel = self
            .src_chain()
            .query_channel(self.src_port_id(), src_channel_id, Height::zero())
            .map_err(|e| ChannelError::query(self.src_chain().id(), e))?;

        if src_channel.counterparty().port_id() != self.dst_port_id() {
            return Err(ChannelError::mismatch_port(
                self.dst_chain().id(),
                self.dst_port_id().clone(),
                self.src_chain().id(),
                src_channel.counterparty().port_id.clone(),
                src_channel_id.clone(),
            ));
        }

        // Connection must exist on destination
        self.dst_chain()
            .query_connection(self.dst_connection_id(), Height::zero())
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

        let channel = ChannelEnd::new(
            State::TryOpen,
            *src_channel.ordering(),
            counterparty,
            vec![self.dst_connection_id().clone()],
            src_channel.version(),
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
            counterparty_version: src_channel.version(),
            channel,
            proofs,
            signer,
        };

        msgs.push(new_msg.to_any());
        Ok(msgs)
    }

    pub fn build_chan_open_try_and_send(&self) -> Result<IbcEvent, ChannelError> {
        let dst_msgs = self.build_chan_open_try()?;

        let events = self
            .dst_chain()
            .send_messages_and_wait_commit(dst_msgs)
            .map_err(|e| ChannelError::submit(self.dst_chain().id(), e))?;

        // Find the relevant event for channel open try
        let result = events
            .into_iter()
            .find(|event| {
                matches!(event, IbcEvent::OpenTryChannel(_))
                    || matches!(event, IbcEvent::ChainError(_))
            })
            .ok_or_else(|| {
                ChannelError::missing_event("no chan try event was in the response".to_string())
            })?;

        match result {
            IbcEvent::OpenTryChannel(_) => Ok(result),
            IbcEvent::ChainError(e) => Err(ChannelError::tx_response(e)),
            _ => Err(ChannelError::invalid_event(result)),
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

        // Check that the destination chain will accept the message
        self.validated_expected_channel(ChannelMsgType::OpenAck)?;

        // Channel must exist on source
        let src_channel = self
            .src_chain()
            .query_channel(self.src_port_id(), src_channel_id, Height::zero())
            .map_err(|e| ChannelError::query(self.src_chain().id(), e))?;

        // Connection must exist on destination
        self.dst_chain()
            .query_connection(self.dst_connection_id(), Height::zero())
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
            counterparty_version: src_channel.version(),
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

            let events = channel
                .dst_chain()
                .send_messages_and_wait_commit(dst_msgs)
                .map_err(|e| ChannelError::submit(channel.dst_chain().id(), e))?;

            // Find the relevant event for channel open ack
            let event = events
                .into_iter()
                .find(|event| {
                    matches!(event, IbcEvent::OpenAckChannel(_))
                        || matches!(event, IbcEvent::ChainError(_))
                })
                .ok_or_else(|| {
                    ChannelError::missing_event("no chan ack event was in the response".to_string())
                })?;

            match event {
                IbcEvent::OpenAckChannel(_) => {
                    info!(
                        "done with ChanAck step {} => {:#?}\n",
                        channel.dst_chain().id(),
                        event
                    );

                    Ok(event)
                }
                IbcEvent::ChainError(e) => Err(ChannelError::tx_response(e)),
                _ => Err(ChannelError::invalid_event(event)),
            }
        }

        do_build_chan_open_ack_and_send(self).map_err(|e| {
            error!("failed ChanAck {:?}: {}", self.b_side, e);
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
            .query_channel(self.src_port_id(), src_channel_id, Height::zero())
            .map_err(|e| ChannelError::query(self.src_chain().id(), e))?;

        // Connection must exist on destination
        self.dst_chain()
            .query_connection(self.dst_connection_id(), Height::zero())
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

            let events = channel
                .dst_chain()
                .send_messages_and_wait_commit(dst_msgs)
                .map_err(|e| ChannelError::submit(channel.dst_chain().id(), e))?;

            // Find the relevant event for channel open confirm
            let event = events
                .into_iter()
                .find(|event| {
                    matches!(event, IbcEvent::OpenConfirmChannel(_))
                        || matches!(event, IbcEvent::ChainError(_))
                })
                .ok_or_else(|| {
                    ChannelError::missing_event(
                        "no chan confirm event was in the response".to_string(),
                    )
                })?;

            match event {
                IbcEvent::OpenConfirmChannel(_) => {
                    info!("done {} => {:#?}\n", channel.dst_chain().id(), event);
                    Ok(event)
                }
                IbcEvent::ChainError(e) => Err(ChannelError::tx_response(e)),
                _ => Err(ChannelError::invalid_event(event)),
            }
        }

        do_build_chan_open_confirm_and_send(self).map_err(|e| {
            error!("failed ChanConfirm {:?}: {}", self.b_side, e);
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
            .query_channel(self.dst_port_id(), dst_channel_id, Height::zero())
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

        let events = self
            .dst_chain()
            .send_messages_and_wait_commit(dst_msgs)
            .map_err(|e| ChannelError::submit(self.dst_chain().id(), e))?;

        // Find the relevant event for channel close init
        let result = events
            .into_iter()
            .find(|event| {
                matches!(event, IbcEvent::CloseInitChannel(_))
                    || matches!(event, IbcEvent::ChainError(_))
            })
            .ok_or_else(|| {
                ChannelError::missing_event("no chan init event was in the response".to_string())
            })?;

        match result {
            IbcEvent::CloseInitChannel(_) => Ok(result),
            IbcEvent::ChainError(e) => Err(ChannelError::tx_response(e)),
            _ => Err(ChannelError::invalid_event(result)),
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
            .query_channel(self.src_port_id(), src_channel_id, Height::zero())
            .map_err(|e| ChannelError::query(self.src_chain().id(), e))?;

        // Connection must exist on destination
        self.dst_chain()
            .query_connection(self.dst_connection_id(), Height::zero())
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
        };

        msgs.push(new_msg.to_any());
        Ok(msgs)
    }

    pub fn build_chan_close_confirm_and_send(&self) -> Result<IbcEvent, ChannelError> {
        let dst_msgs = self.build_chan_close_confirm()?;

        let events = self
            .dst_chain()
            .send_messages_and_wait_commit(dst_msgs)
            .map_err(|e| ChannelError::submit(self.dst_chain().id(), e))?;

        // Find the relevant event for channel close confirm
        let result = events
            .into_iter()
            .find(|event| {
                matches!(event, IbcEvent::CloseConfirmChannel(_))
                    || matches!(event, IbcEvent::ChainError(_))
            })
            .ok_or_else(|| {
                ChannelError::missing_event("no chan confirm event was in the response".to_string())
            })?;

        match result {
            IbcEvent::CloseConfirmChannel(_) => Ok(result),
            IbcEvent::ChainError(e) => Err(ChannelError::tx_response(e)),
            _ => Err(ChannelError::invalid_event(result)),
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
    channel_id: ChannelId,
    existing_channel: ChannelEnd,
    expected_channel: ChannelEnd,
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
        Err(ChannelError::channel_already_exist(channel_id))
    }
}
