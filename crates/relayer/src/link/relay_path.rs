use alloc::collections::BTreeMap as HashMap;
use alloc::collections::VecDeque;
use ibc_relayer_types::core::ics04_channel::packet::Sequence;
use ibc_relayer_types::core::ics04_channel::version::Version;
use serde_json::Error;
use std::ops::Sub;
use std::time::{Duration, Instant};

use ibc_proto::google::protobuf::Any;
use ibc_proto::ibc::applications::transfer::v2::FungibleTokenPacketData as RawPacketData;
use ibc_proto::ibc::applications::transfer::v2::FungibleTokenPacketDataV2 as RawPacketDataV2;
use itertools::Itertools;
use tracing::{debug, error, info, span, trace, warn, Level};

use ibc_relayer_types::core::ics02_client::events::ClientMisbehaviour as ClientMisbehaviourEvent;
use ibc_relayer_types::core::ics04_channel::channel::{
    ChannelEnd, Ordering, State as ChannelState,
};
use ibc_relayer_types::core::ics04_channel::events::{SendPacket, WriteAcknowledgement};
use ibc_relayer_types::core::ics04_channel::msgs::{
    acknowledgement::MsgAcknowledgement, chan_close_confirm::MsgChannelCloseConfirm,
    recv_packet::MsgRecvPacket, timeout::MsgTimeout, timeout_on_close::MsgTimeoutOnClose,
};
use ibc_relayer_types::core::ics04_channel::packet::{Packet, PacketMsgType};
use ibc_relayer_types::core::ics24_host::identifier::{ChannelId, ClientId, ConnectionId, PortId};
use ibc_relayer_types::events::{IbcEvent, IbcEventType, WithBlockDataType};
use ibc_relayer_types::signer::Signer;
use ibc_relayer_types::timestamp::Timestamp;
use ibc_relayer_types::tx_msg::Msg;
use ibc_relayer_types::Height;

use crate::chain::counterparty::unreceived_acknowledgements;
use crate::chain::counterparty::unreceived_packets;
use crate::chain::endpoint::ChainStatus;
use crate::chain::handle::ChainHandle;
use crate::chain::requests::Paginate;
use crate::chain::requests::QueryChannelRequest;
use crate::chain::requests::QueryClientEventRequest;
use crate::chain::requests::QueryHeight;
use crate::chain::requests::QueryHostConsensusStateRequest;
use crate::chain::requests::QueryNextSequenceReceiveRequest;
use crate::chain::requests::QueryPacketCommitmentRequest;
use crate::chain::requests::QueryTxRequest;
use crate::chain::requests::QueryUnreceivedAcksRequest;
use crate::chain::requests::QueryUnreceivedPacketsRequest;
use crate::chain::requests::{IncludeProof, Qualified};
use crate::chain::tracking::TrackedMsgs;
use crate::chain::tracking::TrackingId;
use crate::channel::error::ChannelError;
use crate::channel::Channel;
use crate::config::types::ics20_field_size_limit::Ics20FieldSizeLimit;
use crate::config::types::ics20_field_size_limit::ValidationResult;
use crate::event::source::EventBatch;
use crate::event::IbcEventWithHeight;
use crate::foreign_client::{ForeignClient, ForeignClientError};
use crate::link::error::{self, LinkError};
use crate::link::operational_data::{
    OperationalData, OperationalDataTarget, TrackedEvents, TransitMessage,
};
use crate::link::packet_events::query_packet_events_with;
use crate::link::packet_events::query_send_packet_events;
use crate::link::packet_events::query_write_ack_events;
use crate::link::pending::PendingTxs;
use crate::link::relay_sender::{AsyncReply, SubmitReply};
use crate::link::relay_summary::RelaySummary;
use crate::link::LinkParameters;
use crate::link::{pending, relay_sender};
use crate::path::PathIdentifiers;
use crate::telemetry;
use crate::util::collate::CollatedIterExt;
use crate::util::pretty::PrettyEvents;
use crate::util::queue::Queue;

const MAX_RETRIES: usize = 5;

/// Whether or not to resubmit packets when pending transactions
/// fail to process within the given timeout duration.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Resubmit {
    Yes,
    No,
}

impl Resubmit {
    /// Packet resubmission is enabled when the clear interval for packets is 0. Otherwise,
    /// when the packet clear interval is > 0, the relayer will periodically clear unsent packets
    /// such that resubmitting packets is not necessary.
    pub fn from_clear_interval(clear_interval: u64) -> Self {
        if clear_interval == 0 {
            Self::Yes
        } else {
            Self::No
        }
    }
}

pub struct RelayPath<ChainA: ChainHandle, ChainB: ChainHandle> {
    channel: Channel<ChainA, ChainB>,

    pub(crate) path_id: PathIdentifiers,

    // Operational data, targeting both the source and destination chain.
    // These vectors of operational data are ordered decreasingly by
    // their age, with element at position `0` being the oldest.
    // The operational data targeting the source chain comprises
    // mostly timeout packet messages.
    // The operational data targeting the destination chain
    // comprises mostly RecvPacket and Ack msgs.
    pub src_operational_data: Queue<OperationalData>,
    pub dst_operational_data: Queue<OperationalData>,

    // Toggle for the transaction confirmation mechanism.
    confirm_txes: bool,

    // Stores pending (i.e., unconfirmed) operational data.
    // The relaying path periodically tries to confirm these pending
    // transactions if [`confirm_txes`] is true.
    pending_txs_src: PendingTxs<ChainA>,
    pending_txs_dst: PendingTxs<ChainB>,

    pub max_memo_size: Ics20FieldSizeLimit,
    pub max_receiver_size: Ics20FieldSizeLimit,
    pub exclude_src_sequences: Vec<Sequence>,
}

impl<ChainA: ChainHandle, ChainB: ChainHandle> RelayPath<ChainA, ChainB> {
    pub fn new(
        channel: Channel<ChainA, ChainB>,
        with_tx_confirmation: bool,
        link_parameters: LinkParameters,
    ) -> Result<Self, LinkError> {
        let src_chain = channel.src_chain().clone();
        let dst_chain = channel.dst_chain().clone();

        let src_chain_id = src_chain.id();
        let dst_chain_id = dst_chain.id();

        let src_channel_id = channel
            .src_channel_id()
            .ok_or_else(|| LinkError::missing_channel_id(src_chain.id()))?
            .clone();

        let dst_channel_id = channel
            .dst_channel_id()
            .ok_or_else(|| LinkError::missing_channel_id(dst_chain.id()))?
            .clone();

        let src_port_id = channel.src_port_id().clone();
        let dst_port_id = channel.dst_port_id().clone();

        let path = PathIdentifiers {
            port_id: dst_port_id.clone(),
            channel_id: dst_channel_id.clone(),
            counterparty_port_id: src_port_id.clone(),
            counterparty_channel_id: src_channel_id.clone(),
        };

        Ok(Self {
            channel,

            path_id: path,

            src_operational_data: Queue::new(),
            dst_operational_data: Queue::new(),

            confirm_txes: with_tx_confirmation,
            pending_txs_src: PendingTxs::new(src_chain, src_channel_id, src_port_id, dst_chain_id),
            pending_txs_dst: PendingTxs::new(dst_chain, dst_channel_id, dst_port_id, src_chain_id),

            max_memo_size: link_parameters.max_memo_size,
            max_receiver_size: link_parameters.max_receiver_size,

            exclude_src_sequences: link_parameters.exclude_src_sequences,
        })
    }

    pub fn src_chain(&self) -> &ChainA {
        self.channel.src_chain()
    }

    pub fn dst_chain(&self) -> &ChainB {
        self.channel.dst_chain()
    }

    pub fn src_client_id(&self) -> &ClientId {
        self.channel.src_client_id()
    }

    pub fn dst_client_id(&self) -> &ClientId {
        self.channel.dst_client_id()
    }

    pub fn src_connection_id(&self) -> &ConnectionId {
        self.channel.src_connection_id()
    }

    pub fn dst_connection_id(&self) -> &ConnectionId {
        self.channel.dst_connection_id()
    }

    pub fn src_port_id(&self) -> &PortId {
        &self.path_id.counterparty_port_id
    }

    pub fn dst_port_id(&self) -> &PortId {
        &self.path_id.port_id
    }

    pub fn src_channel_id(&self) -> &ChannelId {
        &self.path_id.counterparty_channel_id
    }

    pub fn dst_channel_id(&self) -> &ChannelId {
        &self.path_id.channel_id
    }

    pub fn channel(&self) -> &Channel<ChainA, ChainB> {
        &self.channel
    }

    fn src_channel(&self, height_query: QueryHeight) -> Result<ChannelEnd, LinkError> {
        self.src_chain()
            .query_channel(
                QueryChannelRequest {
                    port_id: self.src_port_id().clone(),
                    channel_id: self.src_channel_id().clone(),
                    height: height_query,
                },
                IncludeProof::No,
            )
            .map(|(channel_end, _)| channel_end)
            .map_err(|e| LinkError::channel(ChannelError::query(self.src_chain().id(), e)))
    }

    fn dst_channel(&self, height_query: QueryHeight) -> Result<ChannelEnd, LinkError> {
        self.dst_chain()
            .query_channel(
                QueryChannelRequest {
                    port_id: self.dst_port_id().clone(),
                    channel_id: self.dst_channel_id().clone(),
                    height: height_query,
                },
                IncludeProof::No,
            )
            .map(|(channel_end, _)| channel_end)
            .map_err(|e| LinkError::channel(ChannelError::query(self.dst_chain().id(), e)))
    }

    fn src_signer(&self) -> Result<Signer, LinkError> {
        self.src_chain()
            .get_signer()
            .map_err(|e| LinkError::signer(self.src_chain().id(), e))
    }

    fn dst_signer(&self) -> Result<Signer, LinkError> {
        self.dst_chain()
            .get_signer()
            .map_err(|e| LinkError::signer(self.dst_chain().id(), e))
    }

    pub(crate) fn src_latest_height(&self) -> Result<Height, LinkError> {
        self.src_chain()
            .query_latest_height()
            .map_err(|e| LinkError::query(self.src_chain().id(), e))
    }

    pub(crate) fn dst_latest_height(&self) -> Result<Height, LinkError> {
        self.dst_chain()
            .query_latest_height()
            .map_err(|e| LinkError::query(self.dst_chain().id(), e))
    }

    fn src_time_at_height(&self, height: Height) -> Result<Instant, LinkError> {
        Self::chain_time_at_height(self.src_chain(), height)
    }

    fn dst_time_at_height(&self, height: Height) -> Result<Instant, LinkError> {
        Self::chain_time_at_height(self.dst_chain(), height)
    }

    pub(crate) fn src_time_latest(&self) -> Result<Instant, LinkError> {
        let elapsed = Timestamp::now()
            .duration_since(
                &self
                    .src_chain()
                    .query_application_status()
                    .unwrap()
                    .timestamp,
            )
            .unwrap_or_default();

        Ok(Instant::now().sub(elapsed))
    }

    pub(crate) fn dst_time_latest(&self) -> Result<Instant, LinkError> {
        let elapsed = Timestamp::now()
            .duration_since(
                &self
                    .dst_chain()
                    .query_application_status()
                    .unwrap()
                    .timestamp,
            )
            .unwrap_or_default();

        Ok(Instant::now().sub(elapsed))
    }

    pub(crate) fn src_max_block_time(&self) -> Result<Duration, LinkError> {
        // TODO(hu55a1n1): Ideally, we should get the `max_expected_time_per_block` using the
        // `/genesis` endpoint once it is working in tendermint-rs.
        Ok(self
            .src_chain()
            .config()
            .map_err(LinkError::relayer)?
            .max_block_time())
    }

    pub(crate) fn dst_max_block_time(&self) -> Result<Duration, LinkError> {
        Ok(self
            .dst_chain()
            .config()
            .map_err(LinkError::relayer)?
            .max_block_time())
    }

    fn unordered_channel(&self) -> bool {
        self.channel.ordering == Ordering::Unordered
    }

    fn ordered_channel(&self) -> bool {
        self.channel.ordering == Ordering::Ordered
    }

    pub fn build_update_client_on_dst(&self, height: Height) -> Result<Vec<Any>, LinkError> {
        let client = self.restore_dst_client();
        client
            .wait_and_build_update_client(height)
            .map_err(LinkError::client)
    }

    pub fn build_update_client_on_src(&self, height: Height) -> Result<Vec<Any>, LinkError> {
        let client = self.restore_src_client();
        client
            .wait_and_build_update_client(height)
            .map_err(LinkError::client)
    }

    fn build_chan_close_confirm_from_event(
        &self,
        event: &IbcEventWithHeight,
    ) -> Result<Option<Any>, LinkError> {
        // Build the `MsgChannelCloseConfirm` only from `Timeout` or `CloseInitChannel` event types
        if event.event.event_type() != IbcEventType::Timeout
            && event.event.event_type() != IbcEventType::CloseInitChannel
        {
            return Ok(None);
        }

        // Nothing to do if channel on destination is already closed
        let dst_channel = self.dst_channel(QueryHeight::Latest)?;

        if dst_channel.state_matches(&ChannelState::Closed) {
            return Ok(None);
        }

        let src_channel_id = self.src_channel_id();
        let proofs = self
            .src_chain()
            .build_channel_proofs(self.src_port_id(), src_channel_id, event.height)
            .map_err(|e| LinkError::channel(ChannelError::channel_proof(e)))?;

        let counterparty_upgrade_sequence = self.src_channel(QueryHeight::Latest)?.upgrade_sequence;

        // Build the domain type message
        let new_msg = MsgChannelCloseConfirm {
            port_id: self.dst_port_id().clone(),
            channel_id: self.dst_channel_id().clone(),
            proofs,
            signer: self.dst_signer()?,
            counterparty_upgrade_sequence,
        };

        Ok(Some(new_msg.to_any()))
    }

    /// Determines if the events received are relevant and should be processed.
    /// Only events for a port/channel matching one of the channel ends should be processed.
    fn filter_relaying_events(
        &self,
        events: Vec<IbcEventWithHeight>,
        tracking_id: TrackingId,
    ) -> TrackedEvents {
        let src_channel_id = self.src_channel_id();

        let mut result = vec![];

        for event_with_height in events.into_iter() {
            match &event_with_height.event {
                IbcEvent::SendPacket(send_packet_ev) => {
                    if src_channel_id == send_packet_ev.src_channel_id()
                        && self.src_port_id() == send_packet_ev.src_port_id()
                    {
                        result.push(event_with_height);
                    }
                }
                IbcEvent::WriteAcknowledgement(write_ack_ev) => {
                    if src_channel_id == write_ack_ev.dst_channel_id()
                        && self.src_port_id() == write_ack_ev.dst_port_id()
                    {
                        result.push(event_with_height);
                    }
                }
                IbcEvent::CloseInitChannel(chan_close_ev) => {
                    if src_channel_id == chan_close_ev.channel_id()
                        && self.src_port_id() == chan_close_ev.port_id()
                    {
                        result.push(event_with_height);
                    }
                }
                IbcEvent::TimeoutPacket(timeout_ev) => {
                    if src_channel_id == timeout_ev.src_channel_id()
                        && self.channel.src_port_id() == timeout_ev.src_port_id()
                    {
                        result.push(event_with_height);
                    }
                }
                _ => {}
            }
        }

        // Transform into `TrackedEvents`
        TrackedEvents::new(result, tracking_id)
    }

    fn relay_pending_packets(
        &self,
        height: Option<Height>,
        clear_limit: usize,
    ) -> Result<(), LinkError> {
        let _span = span!(Level::ERROR, "relay_pending_packets", ?height).entered();

        let tracking_id = TrackingId::new_packet_clearing();
        telemetry!(received_event_batch, tracking_id);

        let src_config = self.src_chain().config().map_err(LinkError::relayer)?;
        let chunk_size = src_config.query_packets_chunk_size();

        for i in 1..=MAX_RETRIES {
            let cleared_recv = self.schedule_recv_packet_and_timeout_msgs(
                height,
                chunk_size,
                clear_limit,
                tracking_id,
            );

            let cleared_ack =
                self.schedule_packet_ack_msgs(height, chunk_size, clear_limit, tracking_id);

            match cleared_recv.and(cleared_ack) {
                Ok(()) => return Ok(()),
                Err(e) => error!(
                    "failed to clear packets, retry {}/{}: {}",
                    i, MAX_RETRIES, e
                ),
            }
        }

        Err(LinkError::old_packet_clearing_failed())
    }

    /// Clears any packets that were sent before `height`.
    /// If no height is passed in, then the latest height of the source chain is used.
    pub fn schedule_packet_clearing(
        &self,
        height: Option<Height>,
        clear_limit: usize,
    ) -> Result<(), LinkError> {
        let _span = span!(Level::ERROR, "schedule_packet_clearing", ?height).entered();

        let clear_height = height
            .map(|h| h.decrement().map_err(|e| LinkError::decrement_height(h, e)))
            .transpose()?;

        self.relay_pending_packets(clear_height, clear_limit)?;

        debug!(height = ?clear_height, "done relaying pending packets at clear height");

        Ok(())
    }

    /// Generate & schedule operational data from the input `batch` of IBC events.
    pub fn update_schedule(&self, batch: EventBatch) -> Result<(), LinkError> {
        let _span = span!(
            Level::ERROR,
            "update_schedule",
            %batch.tracking_id,
            %batch.height,
        )
        .entered();

        // Collect relevant events from the incoming batch & adjust their height.
        let events = self.filter_relaying_events(batch.events, batch.tracking_id);

        // Update telemetry info
        telemetry!({
            for event_with_height in events.events() {
                self.backlog_update(&event_with_height.event);
            }
        });

        // Transform the events into operational data items
        self.events_to_operational_data(events)
    }

    /// Produces and schedules operational data for this relaying path based on the input events.
    pub(crate) fn events_to_operational_data(
        &self,
        events: TrackedEvents,
    ) -> Result<(), LinkError> {
        // Obtain the operational data for the source chain (mostly timeout packets) and for the
        // destination chain (e.g., receive packet messages).
        let (src_opt, dst_opt) = self.generate_operational_data(events)?;

        if let Some(src_od) = src_opt {
            self.schedule_operational_data(src_od)?;
        }
        if let Some(dst_od) = dst_opt {
            self.schedule_operational_data(dst_od)?;
        }

        Ok(())
    }

    /// Generates operational data out of a set of events.
    /// Handles building operational data targeting both the destination and source chains.
    ///
    /// For the destination chain, the op. data will contain `RecvPacket` messages,
    /// as well as channel close handshake (`ChanCloseConfirm`), `WriteAck` messages.
    ///
    /// For the source chain, the op. data will contain timeout packet messages (`MsgTimeoutOnClose`
    /// or `MsgTimeout`).
    fn generate_operational_data(
        &self,
        events: TrackedEvents,
    ) -> Result<(Option<OperationalData>, Option<OperationalData>), LinkError> {
        let _span = span!(
            Level::ERROR,
            "generate_operational_data",
            tracking_id = %events.tracking_id(),
        )
        .entered();

        let input = events.events();
        let src_height = match input.first() {
            None => return Ok((None, None)),
            Some(ev) => ev.height,
        };

        let dst_latest_info = self
            .dst_chain()
            .query_application_status()
            .map_err(|e| LinkError::query(self.src_chain().id(), e))?;

        let dst_latest_height = dst_latest_info.height;

        // Operational data targeting the source chain (e.g., Timeout packets)
        let mut src_od = OperationalData::new(
            dst_latest_height,
            OperationalDataTarget::Source,
            events.tracking_id(),
            self.channel.connection_delay,
        );

        // Operational data targeting the destination chain (e.g., SendPacket messages)
        let mut dst_od = OperationalData::new(
            src_height,
            OperationalDataTarget::Destination,
            events.tracking_id(),
            self.channel.connection_delay,
        );

        for event_with_height in input {
            trace!(event = %event_with_height, "processing event");

            if let Some(packet) = event_with_height.event.packet() {
                let channel_version = self.channel().dst_version();
                // If the event is a ICS-04 packet event, and the packet contains ICS-20
                // packet data, check that the ICS-20 fields are within the configured limits.
                if !check_ics20_fields_size(
                    &packet.data,
                    self.max_memo_size,
                    self.max_receiver_size,
                    channel_version,
                ) {
                    telemetry!(
                        filtered_packets,
                        &self.src_chain().id(),
                        &self.dst_chain().id(),
                        &packet.source_channel,
                        &packet.destination_channel,
                        &packet.source_port,
                        &packet.destination_port,
                        1
                    );
                    continue;
                }
            }

            let (dst_msg, src_msg) = match &event_with_height.event {
                IbcEvent::CloseInitChannel(_) => (
                    self.build_chan_close_confirm_from_event(event_with_height)?,
                    None,
                ),
                IbcEvent::TimeoutPacket(_) => {
                    // When a timeout packet for an ordered channel is processed on-chain (src here)
                    // the chain closes the channel but no close init event is emitted, instead
                    // we get a timeout packet event (this happens for both unordered and ordered channels)
                    // Here we check that the channel is closed on src and send a channel close confirm
                    // to the counterparty.
                    if self.ordered_channel()
                        && self
                            .src_channel(QueryHeight::Specific(event_with_height.height))?
                            .state_matches(&ChannelState::Closed)
                    {
                        (
                            self.build_chan_close_confirm_from_event(event_with_height)?,
                            None,
                        )
                    } else {
                        (None, None)
                    }
                }
                IbcEvent::SendPacket(ref event) => {
                    if self.send_packet_event_handled(event)? {
                        debug!(?event, "SendPacket event has already been handled");

                        (None, None)
                    } else {
                        self.build_recv_or_timeout_from_send_packet_event(
                            event,
                            &dst_latest_info,
                            event_with_height.height,
                        )?
                    }
                }
                IbcEvent::WriteAcknowledgement(ref event) => {
                    if self
                        .dst_channel(QueryHeight::Latest)?
                        .state_matches(&ChannelState::Closed)
                    {
                        (None, None)
                    } else if self.write_ack_event_handled(event)? {
                        debug!(
                            ?event,
                            "WriteAcknowledgement event has already been handled"
                        );

                        (None, None)
                    } else {
                        (
                            self.build_ack_from_recv_event(event, event_with_height.height)?,
                            None,
                        )
                    }
                }
                _ => (None, None),
            };

            // Collect messages to be sent to the destination chain (e.g., RecvPacket)
            if let Some(msg) = dst_msg {
                trace!(%msg.type_url, event = %event_with_height, "collected event");

                dst_od.batch.push(TransitMessage {
                    event_with_height: event_with_height.clone(),
                    msg,
                });
            }

            // Collect timeout messages, to be sent to the source chain
            if let Some(msg) = src_msg {
                // For Ordered channels a single timeout event should be sent as this closes the channel.
                // Otherwise a multi message transaction will fail.
                if self.unordered_channel() || src_od.batch.is_empty() {
                    trace!(%msg.type_url, event = %event_with_height, "collected event");

                    src_od.batch.push(TransitMessage {
                        event_with_height: event_with_height.clone(),
                        msg,
                    });
                }
            }
        }

        let src_od = Some(src_od).filter(|s| !s.batch.is_empty());
        let dst_od = Some(dst_od).filter(|s| !s.batch.is_empty());

        Ok((src_od, dst_od))
    }

    /// Relays an [`OperationalData`] using a specific
    /// sender, which implements [`relay_sender::Submit`].
    pub(crate) fn relay_from_operational_data<S: relay_sender::Submit>(
        &self,
        initial_od: OperationalData,
    ) -> Result<S::Reply, LinkError> {
        // We will operate on potentially different operational data if the initial one fails.
        let _span = span!(Level::INFO, "relay", odata = %initial_od.info()).entered();

        let mut odata = initial_od;

        for i in 0..MAX_RETRIES {
            debug!(retry.current = i + 1, retry.max = MAX_RETRIES, "retrying");

            // Consume the operational data by attempting to send its messages
            match self.send_from_operational_data::<S>(&odata) {
                Ok(reply) => {
                    // Done with this op. data
                    info!("submitted");

                    telemetry!({
                        let (chain, counterparty, channel_id, port_id) =
                            self.target_info(odata.target);

                        ibc_telemetry::global().tx_submitted(
                            reply.len(),
                            odata.tracking_id,
                            &chain,
                            channel_id,
                            port_id,
                            &counterparty,
                        );
                    });

                    return Ok(reply);
                }
                Err(LinkError(error::LinkErrorDetail::Send(_), _)) => {
                    if i + 1 == MAX_RETRIES {
                        error!("{}/{} retries exhausted, giving up", i + 1, MAX_RETRIES)
                    } else {
                        debug!("{}/{} retries exhausted, retrying with newly-generated operational data", i + 1, MAX_RETRIES);

                        // If we haven't exhausted all retries, regenerate the op. data & retry
                        match self.regenerate_operational_data(odata.clone()) {
                            None => return Ok(S::Reply::empty()), // Nothing to retry
                            Some(new_od) => odata = new_od,
                        }
                    }
                }
                Err(e) => {
                    // Unrecoverable error, propagate up the stack
                    return Err(e);
                }
            }
        }

        Ok(S::Reply::empty())
    }

    /// Generates fresh operational data for a tx given the initial operational data
    /// that failed to send.
    ///
    /// Return value:
    ///   - `Some(..)`: a new operational data from which to retry sending,
    ///   - `None`: all the events in the initial operational data were exhausted (i.e., turned
    ///     into timeouts), so there is nothing to retry.
    ///
    /// Side effects: may schedule a new operational data targeting the source chain, comprising
    /// new timeout messages.
    pub(crate) fn regenerate_operational_data(
        &self,
        initial_odata: OperationalData,
    ) -> Option<OperationalData> {
        let op_info = initial_odata.info();

        warn!(
            "failed. Regenerate operational data from {} events",
            op_info.batch_len()
        );

        // Retry by re-generating the operational data using the initial events
        let (src_opt, dst_opt) = match self.generate_operational_data(initial_odata.into_events()) {
            Ok(new_operational_data) => new_operational_data,
            Err(e) => {
                error!(
                    "failed to regenerate operational data from initial data: {} \
                    with error {}, discarding this op. data",
                    op_info, e
                );
                return None;
            } // Cannot retry, contain the error by reporting a None
        };

        if let Some(src_od) = src_opt {
            if src_od.target == op_info.target() {
                // Our target is the _source_ chain, retry these messages
                info!(odata = %src_od.info(), "will retry");
                return Some(src_od);
            } else {
                // Our target is the _destination_ chain, the data in `src_od` contains
                // potentially new timeout messages that have to be handled separately.
                if let Err(e) = self.schedule_operational_data(src_od) {
                    error!(
                        "failed to schedule newly-generated operational data from \
                        initial data: {} with error {}, discarding this op. data",
                        op_info, e
                    );
                    return None;
                }
            }
        }

        if let Some(dst_od) = dst_opt {
            if dst_od.target == op_info.target() {
                // Our target is the _destination_ chain, retry these messages
                info!(odata = %dst_od.info(), "will retry");
                return Some(dst_od);
            } else {
                // Our target is the _source_ chain, but `dst_od` has new messages
                // intended for the destination chain, this should never be the case
                error!(
                    "generated new messages for destination chain while handling \
                    failed events targeting the source chain!",
                );
            }
        } else {
            // There is no message intended for the destination chain
            if op_info.target() == OperationalDataTarget::Destination {
                info!("exhausted all events from this operational data");
                return None;
            }
        }

        None
    }

    /// Sends a transaction based on the [`OperationalData`] to
    /// the corresponding target chain.
    ///
    /// Returns the appropriate reply associated with the given
    /// [`relay_sender::Submit`]. The reply consists of either the tx
    /// hashes generated by the target chain, if [`Async`] sender,
    /// or the ibc events, if the sender is [`Sync`].
    ///
    /// Propagates any encountered errors.
    fn send_from_operational_data<S: relay_sender::Submit>(
        &self,
        odata: &OperationalData,
    ) -> Result<S::Reply, LinkError> {
        if odata.batch.is_empty() {
            error!("ignoring empty operational data!");
            return Ok(S::Reply::empty());
        }

        let msgs = odata.assemble_msgs(self)?;

        match odata.target {
            OperationalDataTarget::Source => S::submit(self.src_chain(), msgs),
            OperationalDataTarget::Destination => S::submit(self.dst_chain(), msgs),
        }
    }

    fn enqueue_pending_tx(&self, reply: AsyncReply, odata: OperationalData) {
        if !self.confirm_txes {
            return;
        }

        match odata.target {
            OperationalDataTarget::Source => {
                self.pending_txs_src.insert_new_pending_tx(reply, odata);
            }
            OperationalDataTarget::Destination => {
                self.pending_txs_dst.insert_new_pending_tx(reply, odata);
            }
        }
    }

    /// Checks if a sent packet has been received on destination.
    fn send_packet_received_on_dst(&self, packet: &Packet) -> Result<bool, LinkError> {
        let unreceived_packet = self
            .dst_chain()
            .query_unreceived_packets(QueryUnreceivedPacketsRequest {
                port_id: self.dst_port_id().clone(),
                channel_id: self.dst_channel_id().clone(),
                packet_commitment_sequences: vec![packet.sequence],
            })
            .map_err(LinkError::relayer)?;

        Ok(unreceived_packet.is_empty())
    }

    /// Checks if a packet commitment has been cleared on source.
    /// The packet commitment is cleared when either an acknowledgment or a timeout is received on source.
    fn send_packet_commitment_cleared_on_src(&self, packet: &Packet) -> Result<bool, LinkError> {
        let (bytes, _) = self
            .src_chain()
            .query_packet_commitment(
                QueryPacketCommitmentRequest {
                    port_id: self.src_port_id().clone(),
                    channel_id: self.src_channel_id().clone(),
                    sequence: packet.sequence,
                    height: QueryHeight::Latest,
                },
                IncludeProof::No,
            )
            .map_err(LinkError::relayer)?;

        Ok(bytes.is_empty())
    }

    /// Checks if a send packet event has already been handled (e.g. by another relayer).
    fn send_packet_event_handled(&self, sp: &SendPacket) -> Result<bool, LinkError> {
        Ok(self.send_packet_received_on_dst(&sp.packet)?
            || self.send_packet_commitment_cleared_on_src(&sp.packet)?)
    }

    /// Checks if an acknowledgement for the given packet has been received on
    /// source chain of the packet, ie. the destination chain of the relay path
    /// that sends the acknowledgment.
    fn recv_packet_acknowledged_on_src(&self, packet: &Packet) -> Result<bool, LinkError> {
        let unreceived_ack = self
            .dst_chain()
            .query_unreceived_acknowledgements(QueryUnreceivedAcksRequest {
                port_id: self.dst_port_id().clone(),
                channel_id: self.dst_channel_id().clone(),
                packet_ack_sequences: vec![packet.sequence],
            })
            .map_err(LinkError::relayer)?;

        Ok(unreceived_ack.is_empty())
    }

    /// Checks if a receive packet event has already been handled (e.g. by another relayer).
    fn write_ack_event_handled(&self, rp: &WriteAcknowledgement) -> Result<bool, LinkError> {
        self.recv_packet_acknowledged_on_src(&rp.packet)
    }

    /// Returns the `processed_height` for the consensus state at specified height
    fn update_height(
        chain: &impl ChainHandle,
        client_id: ClientId,
        consensus_height: Height,
    ) -> Result<Height, LinkError> {
        let events_with_heights = chain
            .query_txs(QueryTxRequest::Client(QueryClientEventRequest {
                query_height: QueryHeight::Latest,
                event_id: WithBlockDataType::UpdateClient,
                client_id,
                consensus_height,
            }))
            .map_err(|e| LinkError::query(chain.id(), e))?;

        // The handler may treat redundant updates as no-ops and emit `UpdateClient` events for them
        // but the `processed_height` is the height at which the first `UpdateClient` event for this
        // consensus state/height was emitted. We expect that these events are received in the exact
        // same order in which they were emitted.
        match events_with_heights.first() {
            Some(event_with_height) => {
                if matches!(&event_with_height.event, IbcEvent::UpdateClient(_)) {
                    Ok(event_with_height.height)
                } else {
                    Err(LinkError::unexpected_event(event_with_height.event.clone()))
                }
            }
            None => Err(LinkError::update_client_event_not_found()),
        }
    }

    /// Loops over `tx_events` and returns a tuple of optional events where the first element is a
    /// `ChainError` variant, the second one is an `UpdateClient` variant and the third one is a
    /// `ClientMisbehaviour` variant. This function is essentially just an `Iterator::find()` for
    /// multiple variants with a single pass.
    #[inline]
    fn event_per_type(
        mut tx_events: Vec<IbcEventWithHeight>,
    ) -> (
        Option<IbcEvent>,
        Option<Height>,
        Option<ClientMisbehaviourEvent>,
    ) {
        let mut error = None;
        let mut update = None;
        let mut misbehaviour = None;

        while let Some(event_with_height) = tx_events.pop() {
            match event_with_height.event {
                IbcEvent::ChainError(_) => error = Some(event_with_height.event),
                IbcEvent::UpdateClient(_) => update = Some(event_with_height.height),
                IbcEvent::ClientMisbehaviour(event) => misbehaviour = Some(event),
                _ => {}
            }
        }

        (error, update, misbehaviour)
    }

    /// Returns an instant (in the past) that corresponds to the block timestamp of the chain at
    /// specified height (relative to the relayer's current time). If the timestamp is in the future
    /// wrt the relayer's current time, we simply return the current relayer time.
    fn chain_time_at_height(
        chain: &impl ChainHandle,
        height: Height,
    ) -> Result<Instant, LinkError> {
        let chain_time = chain
            .query_host_consensus_state(QueryHostConsensusStateRequest {
                height: QueryHeight::Specific(height),
            })
            .map_err(LinkError::relayer)?
            .timestamp();
        let duration = Timestamp::now()
            .duration_since(&chain_time)
            .unwrap_or_default();
        Ok(Instant::now().sub(duration))
    }

    /// Handles updating the client on the destination chain
    /// Returns the height at which the client update was processed
    fn update_client_dst(
        &self,
        src_chain_height: Height,
        tracking_id: TrackingId,
    ) -> Result<Height, LinkError> {
        self.do_update_client_dst(src_chain_height, tracking_id, MAX_RETRIES)
    }

    /// Perform actual update_client_dst with retries.
    ///
    /// Note that the retry is only performed in the case when there
    /// is a ChainError event. It would return error immediately if
    /// there are other errors returned from calls such as
    /// build_update_client_on_dst.
    fn do_update_client_dst(
        &self,
        src_chain_height: Height,
        tracking_id: TrackingId,
        retries_left: usize,
    ) -> Result<Height, LinkError> {
        info!( "sending update_client to client hosted on source chain for height {} (retries left: {})", src_chain_height, retries_left );

        let dst_update = self.build_update_client_on_dst(src_chain_height)?;
        let tm = TrackedMsgs::new(dst_update, tracking_id);
        let dst_tx_events = self
            .dst_chain()
            .send_messages_and_wait_commit(tm)
            .map_err(LinkError::relayer)?;

        info!("result: {}", PrettyEvents(dst_tx_events.as_slice()));

        let (error, update, misbehaviour) = Self::event_per_type(dst_tx_events);
        match (error, update, misbehaviour) {
            // All updates were successful, no errors and no misbehaviour.
            (None, Some(update_event_height), None) => Ok(update_event_height),
            (Some(chain_error), _, _) => {
                // At least one chain-error so retry if possible.
                if retries_left == 0 {
                    Err(LinkError::client(ForeignClientError::chain_error_event(
                        self.dst_chain().id(),
                        chain_error,
                    )))
                } else {
                    self.do_update_client_dst(src_chain_height, tracking_id, retries_left - 1)
                }
            }
            (None, None, None) => {
                // `tm` was empty and update wasn't required
                match Self::update_height(
                    self.dst_chain(),
                    self.dst_client_id().clone(),
                    src_chain_height,
                ) {
                    Ok(update_height) => Ok(update_height),
                    Err(_) if retries_left > 0 => {
                        self.do_update_client_dst(src_chain_height, tracking_id, retries_left - 1)
                    }
                    _ => Err(LinkError::update_client_failed()),
                }
            }
            // At least one misbehaviour event, so don't retry.
            (_, _, Some(_misbehaviour)) => Err(LinkError::update_client_failed()),
        }
    }

    /// Handles updating the client on the source chain
    /// Returns the height at which the client update was processed
    fn update_client_src(
        &self,
        dst_chain_height: Height,
        tracking_id: TrackingId,
    ) -> Result<Height, LinkError> {
        self.do_update_client_src(dst_chain_height, tracking_id, MAX_RETRIES)
    }

    /// Perform actual update_client_src with retries.
    ///
    /// Note that the retry is only performed in the case when there
    /// is a ChainError event. It would return error immediately if
    /// there are other errors returned from calls such as
    /// build_update_client_on_src.
    fn do_update_client_src(
        &self,
        dst_chain_height: Height,
        tracking_id: TrackingId,
        retries_left: usize,
    ) -> Result<Height, LinkError> {
        info!("sending update_client to client hosted on source chain for height {} (retries left: {})", dst_chain_height, retries_left);

        let src_update = self.build_update_client_on_src(dst_chain_height)?;
        let tm = TrackedMsgs::new(src_update, tracking_id);
        let src_tx_events = self
            .src_chain()
            .send_messages_and_wait_commit(tm)
            .map_err(LinkError::relayer)?;

        info!("result: {}", PrettyEvents(src_tx_events.as_slice()));

        let (error, update, misbehaviour) = Self::event_per_type(src_tx_events);
        match (error, update, misbehaviour) {
            // All updates were successful, no errors and no misbehaviour.
            (None, Some(update_event_height), None) => Ok(update_event_height),
            (Some(chain_error), _, _) => {
                // At least one chain-error so retry if possible.
                if retries_left == 0 {
                    Err(LinkError::client(ForeignClientError::chain_error_event(
                        self.src_chain().id(),
                        chain_error,
                    )))
                } else {
                    self.do_update_client_src(dst_chain_height, tracking_id, retries_left - 1)
                }
            }
            (None, None, None) => {
                // `tm` was empty and update wasn't required
                match Self::update_height(
                    self.src_chain(),
                    self.src_client_id().clone(),
                    dst_chain_height,
                ) {
                    Ok(update_height) => Ok(update_height),
                    Err(_) if retries_left > 0 => {
                        self.do_update_client_src(dst_chain_height, tracking_id, retries_left - 1)
                    }
                    _ => Err(LinkError::update_client_failed()),
                }
            }
            // At least one misbehaviour event, so don't retry.
            (_, _, Some(_misbehaviour)) => Err(LinkError::update_client_failed()),
        }
    }

    /// Schedules the relaying of [`MsgRecvPacket`] and [`MsgTimeout`] messages.
    ///
    /// The optional [`Height`] parameter allows specify a height on the source
    /// chain where to query for packet data. If `None`, the latest available
    /// height on the source chain is used.
    ///
    /// Blocks until _all_ outstanding messages have been scheduled.
    pub fn schedule_recv_packet_and_timeout_msgs(
        &self,
        opt_query_height: Option<Height>,
        chunk_size: usize,
        clear_limit: usize,
        tracking_id: TrackingId,
    ) -> Result<(), LinkError> {
        let _span = span!(
            Level::ERROR,
            "schedule_recv_packet_and_timeout_msgs",
            query_height = %opt_query_height.map(|h| h.to_string()).unwrap_or_default()
        )
        .entered();

        // Pull the s.n. of all packets that the destination chain has not yet received.
        let (sequences, src_response_height) = unreceived_packets(
            self.dst_chain(),
            self.src_chain(),
            &self.path_id,
            Paginate::All,
        )
        .map_err(LinkError::supervisor)?;

        let query_height = opt_query_height.unwrap_or(src_response_height);

        // Skip: no relevant events found.
        if sequences.is_empty() {
            return Ok(());
        }

        // Retain only sequences which should not be filtered out
        let raw_sequences: Vec<Sequence> = sequences
            .into_iter()
            .filter(|sequence| !self.exclude_src_sequences.contains(sequence))
            .collect();

        let sequences = &raw_sequences[..raw_sequences.len().min(clear_limit)];

        debug!(
            dst_chain = %self.dst_chain().id(),
            src_chain = %self.src_chain().id(),
            total = sequences.len(),
            sequences = %sequences.iter().copied().collated().format(", "),
            "sequence numbers of unreceived packets to send to the destination chain out of the ones with commitments on the source chain",
        );

        // Chunk-up the list of sequence nrs. into smaller parts,
        // and schedule operational data incrementally across each chunk.
        for events_chunk in query_packet_events_with(
            sequences,
            Qualified::SmallerEqual(query_height),
            self.src_chain(),
            &self.path_id,
            chunk_size,
            query_send_packet_events,
        ) {
            // Update telemetry info
            telemetry!({
                for event_with_height in events_chunk.iter() {
                    self.record_cleared_send_packet(event_with_height);
                }
            });

            self.events_to_operational_data(TrackedEvents::new(events_chunk, tracking_id))?;
        }

        Ok(())
    }

    /// Schedules the relaying of [`MsgAcknowledgement`] messages.
    ///
    /// The `opt_query_height` parameter allows to optionally use a specific height on the source
    /// chain where to query for packet data. If `None`, the latest available height on the source
    /// chain is used.
    pub fn schedule_packet_ack_msgs(
        &self,
        opt_query_height: Option<Height>,
        chunk_size: usize,
        clear_limit: usize,
        tracking_id: TrackingId,
    ) -> Result<(), LinkError> {
        let _span = span!(
            Level::ERROR,
            "build_packet_ack_msgs",
            query_height = %opt_query_height.map(|h| h.to_string()).unwrap_or_default()
        )
        .entered();

        let sequences_and_height = unreceived_acknowledgements(
            self.dst_chain(),
            self.src_chain(),
            &self.path_id,
            Paginate::All,
        )
        .map_err(LinkError::supervisor)?;

        let Some((sequences, src_response_height)) = sequences_and_height else {
            return Ok(());
        };

        let query_height = opt_query_height.unwrap_or(src_response_height);

        // Skip: no relevant events found.
        if sequences.is_empty() {
            return Ok(());
        }

        // Retain only sequences which should not be filtered out
        let raw_sequences: Vec<Sequence> = sequences
            .into_iter()
            .filter(|sequence| !self.exclude_src_sequences.contains(sequence))
            .collect();

        let sequences = &raw_sequences[..raw_sequences.len().min(clear_limit)];

        debug!(
            dst_chain = %self.dst_chain().id(),
            src_chain = %self.src_chain().id(),
            total = sequences.len(),
            sequences = %sequences.iter().copied().collated().format(", "),
            "sequence numbers of ack packets to send to the destination chain out of the ones with acknowledgments on the source chain",
        );

        // Incrementally process all the available sequence numbers in chunks
        for events_chunk in query_packet_events_with(
            sequences,
            Qualified::SmallerEqual(query_height),
            self.src_chain(),
            &self.path_id,
            chunk_size,
            query_write_ack_events,
        ) {
            telemetry!(self.record_cleared_acknowledgments(events_chunk.iter()));
            self.events_to_operational_data(TrackedEvents::new(events_chunk, tracking_id))?;
        }

        Ok(())
    }

    fn build_recv_packet(&self, packet: &Packet, height: Height) -> Result<Option<Any>, LinkError> {
        let proofs = self
            .src_chain()
            .build_packet_proofs(
                PacketMsgType::Recv,
                &packet.source_port,
                &packet.source_channel,
                packet.sequence,
                height,
            )
            .map_err(|e| LinkError::packet_proofs_constructor(self.src_chain().id(), e))?;

        let msg = MsgRecvPacket::new(packet.clone(), proofs.clone(), self.dst_signer()?);

        trace!(packet = %packet, height = %proofs.height(), "built recv_packet msg");

        Ok(Some(msg.to_any()))
    }

    fn build_ack_from_recv_event(
        &self,
        event: &WriteAcknowledgement,
        height: Height,
    ) -> Result<Option<Any>, LinkError> {
        let packet = event.packet.clone();

        let proofs = self
            .src_chain()
            .build_packet_proofs(
                PacketMsgType::Ack,
                &packet.destination_port,
                &packet.destination_channel,
                packet.sequence,
                height,
            )
            .map_err(|e| LinkError::packet_proofs_constructor(self.src_chain().id(), e))?;

        let msg = MsgAcknowledgement::new(
            packet,
            event.ack.clone().into(),
            proofs.clone(),
            self.dst_signer()?,
        );

        trace!(packet = %msg.packet, height = %proofs.height(), "built acknowledgment msg");

        Ok(Some(msg.to_any()))
    }

    fn build_timeout_packet(
        &self,
        packet: &Packet,
        height: Height,
    ) -> Result<Option<Any>, LinkError> {
        let dst_channel_id = self.dst_channel_id();

        trace!(%packet, %height, "build timeout for channel");

        let (packet_type, next_sequence_received) = if self.ordered_channel() {
            let (next_seq, _) = self
                .dst_chain()
                .query_next_sequence_receive(
                    QueryNextSequenceReceiveRequest {
                        port_id: self.dst_port_id().clone(),
                        channel_id: dst_channel_id.clone(),
                        height: QueryHeight::Specific(height),
                    },
                    IncludeProof::No,
                )
                .map_err(|e| LinkError::query(self.dst_chain().id(), e))?;

            (PacketMsgType::TimeoutOrdered, next_seq)
        } else {
            (PacketMsgType::TimeoutUnordered, packet.sequence)
        };

        let proofs = self
            .dst_chain()
            .build_packet_proofs(
                packet_type,
                &packet.destination_port,
                &packet.destination_channel,
                next_sequence_received,
                height,
            )
            .map_err(|e| LinkError::packet_proofs_constructor(self.dst_chain().id(), e))?;

        let msg = MsgTimeout::new(
            packet.clone(),
            next_sequence_received,
            proofs.clone(),
            self.src_signer()?,
        );

        trace!(packet = %msg.packet, height = %proofs.height(), "built timeout msg");

        Ok(Some(msg.to_any()))
    }

    fn build_timeout_on_close_packet(
        &self,
        packet: &Packet,
        height: Height,
    ) -> Result<Option<Any>, LinkError> {
        let dst_channel_id = self.dst_channel_id();

        trace!(%packet, %height, "build timeout on close for channel");
        let (packet_type, next_sequence_received) = if self.ordered_channel() {
            let (next_seq, _) = self
                .dst_chain()
                .query_next_sequence_receive(
                    QueryNextSequenceReceiveRequest {
                        port_id: self.dst_port_id().clone(),
                        channel_id: dst_channel_id.clone(),
                        height: QueryHeight::Specific(height),
                    },
                    IncludeProof::No,
                )
                .map_err(|e| LinkError::query(self.dst_chain().id(), e))?;

            (PacketMsgType::TimeoutOnCloseOrdered, next_seq)
        } else {
            (PacketMsgType::TimeoutOnCloseUnordered, packet.sequence)
        };

        let proofs = self
            .dst_chain()
            .build_packet_proofs(
                packet_type,
                &packet.destination_port,
                &packet.destination_channel,
                next_sequence_received,
                height,
            )
            .map_err(|e| LinkError::packet_proofs_constructor(self.dst_chain().id(), e))?;

        let counterparty_upgrade_sequence = self.src_channel(QueryHeight::Latest)?.upgrade_sequence;

        let msg = MsgTimeoutOnClose::new(
            packet.clone(),
            next_sequence_received,
            proofs.clone(),
            self.src_signer()?,
            counterparty_upgrade_sequence,
        );

        trace!(packet = %msg.packet, height = %proofs.height(), "built timeout on close msg");

        Ok(Some(msg.to_any()))
    }

    fn build_timeout_from_send_packet_event(
        &self,
        event: &SendPacket,
        dst_info: &ChainStatus,
    ) -> Result<Option<Any>, LinkError> {
        let packet = event.packet.clone();

        if self
            .dst_channel(QueryHeight::Specific(dst_info.height))?
            .state_matches(&ChannelState::Closed)
        {
            Ok(self.build_timeout_on_close_packet(&event.packet, dst_info.height)?)
        } else if packet.timed_out(&dst_info.timestamp, dst_info.height) {
            Ok(self.build_timeout_packet(&event.packet, dst_info.height)?)
        } else {
            Ok(None)
        }
    }

    fn build_recv_or_timeout_from_send_packet_event(
        &self,
        event: &SendPacket,
        dst_info: &ChainStatus,
        height: Height,
    ) -> Result<(Option<Any>, Option<Any>), LinkError> {
        crate::time!(
            "build_recv_or_timeout_from_send_packet_event",
            {
                "src_channel": event.packet.source_channel,
                "dst_channel": event.packet.destination_channel,
            }
        );

        let timeout = self.build_timeout_from_send_packet_event(event, dst_info)?;

        if timeout.is_some() {
            Ok((None, timeout))
        } else {
            Ok((self.build_recv_packet(&event.packet, height)?, None))
        }
    }

    /// Drives the relaying of elapsed operational data items meant for
    /// a specified target chain forward.
    ///
    /// Given an iterator of `OperationalData` elements, this function
    /// first determines whether the current piece of operational data
    /// has elapsed.
    ///
    /// A piece of operational data is considered 'elapsed' if it has been waiting
    /// for an amount of time that surpasses both of the following:
    /// 1. The time duration specified in the connection delay
    /// 2. The number of blocks specified in the connection delay
    ///
    /// If the current piece of operational data has elapsed, then relaying
    /// is performed using the asynchronous sender. Operational data is
    /// retained as pending and is associated with one or more transaction
    /// hash(es).
    ///
    /// Should an error occur when attempting to relay a piece of operational
    /// data, this function returns all subsequent unprocessed pieces of
    /// operational data back to the caller so that they can be re-queued
    /// for processing; the operational data that failed to send is dropped.
    ///
    /// Note that pieces of operational data that have not elapsed yet are
    /// also placed in the 'unprocessed' bucket.
    fn execute_schedule_for_target_chain<I: Iterator<Item = OperationalData>>(
        &mut self,
        mut operations: I,
        target_chain: OperationalDataTarget,
    ) -> Result<VecDeque<OperationalData>, (VecDeque<OperationalData>, LinkError)> {
        let mut unprocessed = VecDeque::new();

        while let Some(od) = operations.next() {
            let elapsed_result = match target_chain {
                OperationalDataTarget::Source => od.has_conn_delay_elapsed(
                    &|| self.src_time_latest(),
                    &|| self.src_max_block_time(),
                    &|| self.src_latest_height(),
                ),
                OperationalDataTarget::Destination => od.has_conn_delay_elapsed(
                    &|| self.dst_time_latest(),
                    &|| self.dst_max_block_time(),
                    &|| self.dst_latest_height(),
                ),
            };

            match elapsed_result {
                Ok(elapsed) => {
                    if elapsed {
                        // The current piece of operational data has elapsed; we can go ahead and
                        // attempt to relay it.
                        match self
                            .relay_from_operational_data::<relay_sender::AsyncSender>(od.clone())
                        {
                            // The operational data was successfully relayed; enqueue the associated tx.
                            Ok(reply) => self.enqueue_pending_tx(reply, od),
                            // The relaying process failed; return all of the subsequent pieces of operational
                            // data along with the underlying error that occurred.
                            Err(e) => {
                                unprocessed.extend(operations);

                                return Err((unprocessed, e));
                            }
                        }
                    } else {
                        // The current piece of operational data has not elapsed; add it to the bucket
                        // of unprocessed operational data and continue processing subsequent pieces
                        // of operational data.
                        unprocessed.push_back(od);
                    }
                }
                Err(e) => {
                    // An error occurred when attempting to determine whether the current piece of
                    // operational data has elapsed or not. Add the current piece of data, along with
                    // all of the subsequent pieces of data, to the unprocessed bucket and return it
                    // along with the error that resulted.
                    unprocessed.push_back(od);
                    unprocessed.extend(operations);

                    return Err((unprocessed, e));
                }
            }
        }

        Ok(unprocessed)
    }

    /// While there are pending operational data items, this function
    /// performs the relaying of packets corresponding to those
    /// operational data items to both the source and destination chains.
    ///
    /// Any operational data items that do not get successfully relayed are
    /// dropped. Subsequent pending operational data items that went unprocessed
    /// are queued up again for re-submission.
    pub fn execute_schedule(&mut self) -> Result<(), LinkError> {
        let src_od_iter = self.src_operational_data.take().into_iter();

        match self.execute_schedule_for_target_chain(src_od_iter, OperationalDataTarget::Source) {
            Ok(unprocessed_src_data) => self.src_operational_data = unprocessed_src_data.into(),
            Err((unprocessed_src_data, e)) => {
                self.src_operational_data = unprocessed_src_data.into();
                return Err(e);
            }
        }

        let dst_od_iter = self.dst_operational_data.take().into_iter();

        match self
            .execute_schedule_for_target_chain(dst_od_iter, OperationalDataTarget::Destination)
        {
            Ok(unprocessed_dst_data) => self.dst_operational_data = unprocessed_dst_data.into(),
            Err((unprocessed_dst_data, e)) => {
                self.dst_operational_data = unprocessed_dst_data.into();
                return Err(e);
            }
        }

        Ok(())
    }

    /// Kicks off the process of relaying pending txs to the source and destination chains.
    ///
    /// See [`Resubmit::from_clear_interval`] for more info about the `resubmit` parameter.
    pub fn process_pending_txs(&self, resubmit: Resubmit) -> RelaySummary {
        if !self.confirm_txes {
            return RelaySummary::empty();
        }

        let mut summary_src = self.process_pending_txs_src(resubmit).unwrap_or_else(|e| {
            error!("error processing pending events in source chain: {}", e);
            RelaySummary::empty()
        });

        let summary_dst = self.process_pending_txs_dst(resubmit).unwrap_or_else(|e| {
            error!(
                "error processing pending events in destination chain: {}",
                e
            );

            RelaySummary::empty()
        });

        summary_src.extend(summary_dst);
        summary_src
    }

    fn process_pending_txs_src(&self, resubmit: Resubmit) -> Result<RelaySummary, LinkError> {
        let do_resubmit = match resubmit {
            Resubmit::Yes => {
                Some(|odata| self.relay_from_operational_data::<relay_sender::AsyncSender>(odata))
            }
            Resubmit::No => None,
        };

        let res = self
            .pending_txs_src
            .process_pending(pending::TIMEOUT, self, do_resubmit)?
            .unwrap_or_else(RelaySummary::empty);

        Ok(res)
    }

    fn process_pending_txs_dst(&self, resubmit: Resubmit) -> Result<RelaySummary, LinkError> {
        let do_resubmit = match resubmit {
            Resubmit::Yes => {
                Some(|odata| self.relay_from_operational_data::<relay_sender::AsyncSender>(odata))
            }
            Resubmit::No => None,
        };

        let res = self
            .pending_txs_dst
            .process_pending(pending::TIMEOUT, self, do_resubmit)?
            .unwrap_or_else(RelaySummary::empty);

        Ok(res)
    }

    /// Refreshes the scheduled batches.
    /// Verifies if any sendPacket messages timed-out. If so, moves them from destination op. data
    /// to source operational data, and adjusts the events and messages accordingly.
    pub fn refresh_schedule(&self) -> Result<(), LinkError> {
        let _span = span!(Level::ERROR, "refresh_schedule").entered();

        // Bail fast if no op. data to refresh
        if self.dst_operational_data.is_empty() {
            return Ok(());
        }

        let dst_status = self
            .dst_chain()
            .query_application_status()
            .map_err(|e| LinkError::query(self.src_chain().id(), e))?;

        let dst_current_height = dst_status.height;

        // Intermediary data struct to help better manage the transfer from dst. operational data
        // to source operational data.
        let mut all_dst_odata = self.dst_operational_data.clone_vec();

        let mut timed_out: HashMap<usize, OperationalData> = HashMap::default();

        // For each operational data targeting the destination chain...
        for (odata_pos, odata) in all_dst_odata.iter_mut().enumerate() {
            // ... check each `SendPacket` event, whether it should generate a timeout message
            let mut retain_batch = vec![];

            for gm in odata.batch.iter() {
                let TransitMessage {
                    event_with_height, ..
                } = gm;

                match &event_with_height.event {
                    IbcEvent::SendPacket(event) => {
                        // Catch any SendPacket event that timed-out
                        if self.send_packet_event_handled(event)? {
                            debug!(?event, "SendPacket event has already been handled");
                        } else if let Some(new_msg) =
                            self.build_timeout_from_send_packet_event(event, &dst_status)?
                        {
                            debug!(
                                "found a timed-out message in the operational data: {}",
                                odata.info(),
                            );

                            timed_out
                                .entry(odata_pos)
                                .or_insert_with(|| {
                                    OperationalData::new(
                                        dst_current_height,
                                        OperationalDataTarget::Source,
                                        odata.tracking_id,
                                        self.channel.connection_delay,
                                    )
                                })
                                .push(TransitMessage {
                                    event_with_height: event_with_height.clone(),
                                    msg: new_msg,
                                });
                        } else {
                            // A SendPacket event, but did not time-out yet, retain
                            retain_batch.push(gm.clone());
                        }
                    }
                    IbcEvent::WriteAcknowledgement(event) => {
                        if self.write_ack_event_handled(event)? {
                            debug!(?event, "WriteAcknowledgement has already been handled");
                        } else {
                            retain_batch.push(gm.clone());
                        }
                    }
                    _ => retain_batch.push(gm.clone()),
                }
            }

            // Update the whole batch, keeping only the relevant ones
            odata.batch = retain_batch;
        }

        // Possibly some op. data became empty (if no events were kept).
        // Retain only the non-empty ones.
        all_dst_odata.retain(|o| !o.batch.is_empty());

        // Replace the original operational data with the updated one
        self.dst_operational_data.replace(all_dst_odata);

        // Handle timed-out events
        if timed_out.is_empty() {
            // Nothing timed out in the meantime
            return Ok(());
        }

        // Schedule new operational data targeting the source chain
        for (_, new_od) in timed_out.into_iter() {
            info!(
                "re-scheduling from new timed-out batch of size {}",
                new_od.batch.len()
            );

            self.schedule_operational_data(new_od)?;
        }

        Ok(())
    }

    /// Adds a new operational data item for this relaying path to process later.
    /// If the relaying path has non-zero packet delays, this method also updates the client on the
    /// target chain with the appropriate headers.
    fn schedule_operational_data(&self, mut od: OperationalData) -> Result<(), LinkError> {
        let _span = span!(Level::INFO, "schedule", odata = %od.info()).entered();

        if od.batch.is_empty() {
            info!(
                "ignoring operational data for {} because it has no messages",
                od.target
            );
            return Ok(());
        }

        // Update clients ahead of scheduling the operational data, if the delays are non-zero.
        // If the connection-delay must be taken into account, set the `scheduled_time` to an
        // instant in the past, i.e. when this client update was first processed (`processed_time`)
        let scheduled_time = if od.conn_delay_needed() {
            debug!("connection delay must be taken into account: updating client");
            let target_height = od.proofs_height.increment();
            match od.target {
                OperationalDataTarget::Source => {
                    let update_height = self.update_client_src(target_height, od.tracking_id)?;
                    od.set_update_height(update_height);
                    self.src_time_at_height(update_height)?
                }
                OperationalDataTarget::Destination => {
                    let update_height = self.update_client_dst(target_height, od.tracking_id)?;
                    od.set_update_height(update_height);
                    self.dst_time_at_height(update_height)?
                }
            }
        } else {
            debug!("connection delay need not be taken into account: client update message will be prepended later");
            Instant::now()
        };

        od.set_scheduled_time(scheduled_time);

        match od.target {
            OperationalDataTarget::Source => self.src_operational_data.push_back(od),
            OperationalDataTarget::Destination => self.dst_operational_data.push_back(od),
        };

        Ok(())
    }

    /// Pulls out the operational elements with elapsed delay period and that can
    /// now be processed.
    pub(crate) fn try_fetch_scheduled_operational_data(
        &self,
    ) -> Result<(VecDeque<OperationalData>, VecDeque<OperationalData>), LinkError> {
        // Extracts elements from a Vec when the predicate returns true.
        // The mutable vector is then updated to the remaining unextracted elements.
        fn partition<T>(
            queue: VecDeque<T>,
            pred: impl Fn(&T) -> Result<bool, LinkError>,
        ) -> Result<(VecDeque<T>, VecDeque<T>), LinkError> {
            let mut true_res = VecDeque::new();
            let mut false_res = VecDeque::new();

            for e in queue.into_iter() {
                if pred(&e)? {
                    true_res.push_back(e);
                } else {
                    false_res.push_back(e);
                }
            }

            Ok((true_res, false_res))
        }

        let (elapsed_src_ods, unelapsed_src_ods) =
            partition(self.src_operational_data.take(), |op| {
                op.has_conn_delay_elapsed(
                    &|| self.src_time_latest(),
                    &|| self.src_max_block_time(),
                    &|| self.src_latest_height(),
                )
            })?;

        let (elapsed_dst_ods, unelapsed_dst_ods) =
            partition(self.dst_operational_data.take(), |op| {
                op.has_conn_delay_elapsed(
                    &|| self.dst_time_latest(),
                    &|| self.dst_max_block_time(),
                    &|| self.dst_latest_height(),
                )
            })?;

        self.src_operational_data.replace(unelapsed_src_ods);
        self.dst_operational_data.replace(unelapsed_dst_ods);
        Ok((elapsed_src_ods, elapsed_dst_ods))
    }

    fn restore_src_client(&self) -> ForeignClient<ChainA, ChainB> {
        ForeignClient::restore(
            self.src_client_id().clone(),
            self.src_chain().clone(),
            self.dst_chain().clone(),
        )
    }

    fn restore_dst_client(&self) -> ForeignClient<ChainB, ChainA> {
        ForeignClient::restore(
            self.dst_client_id().clone(),
            self.dst_chain().clone(),
            self.src_chain().clone(),
        )
    }

    // we need fully qualified ChainId to avoid unneeded imports warnings
    fn target_info(
        &self,
        target: OperationalDataTarget,
    ) -> (
        ibc_relayer_types::core::ics24_host::identifier::ChainId, // source chain
        ibc_relayer_types::core::ics24_host::identifier::ChainId, // destination chain
        &ChannelId,
        &PortId,
    ) {
        match target {
            OperationalDataTarget::Source => (
                self.src_chain().id(),
                self.dst_chain().id(),
                self.src_channel_id(),
                self.src_port_id(),
            ),
            OperationalDataTarget::Destination => (
                self.dst_chain().id(),
                self.src_chain().id(),
                self.dst_channel_id(),
                self.dst_port_id(),
            ),
        }
    }

    fn backlog_update(&self, event: &IbcEvent) {
        match event {
            IbcEvent::SendPacket(send_packet_ev) => {
                ibc_telemetry::global().backlog_insert(
                    send_packet_ev.packet.sequence.into(),
                    &self.src_chain().id(),
                    self.src_channel_id(),
                    self.src_port_id(),
                    &self.dst_chain().id(),
                );
            }
            IbcEvent::WriteAcknowledgement(write_ack_ev) => {
                ibc_telemetry::global().backlog_remove(
                    write_ack_ev.packet.sequence.into(),
                    &self.dst_chain().id(),
                    self.dst_channel_id(),
                    self.dst_port_id(),
                    &self.src_chain().id(),
                );
            }
            IbcEvent::TimeoutPacket(timeout_packet) => {
                ibc_telemetry::global().backlog_remove(
                    timeout_packet.packet.sequence.into(),
                    &self.src_chain().id(),
                    self.src_channel_id(),
                    self.src_port_id(),
                    &self.dst_chain().id(),
                );
            }
            _ => {}
        }
    }

    fn record_cleared_send_packet(&self, event_with_height: &IbcEventWithHeight) {
        if let IbcEvent::SendPacket(send_packet_ev) = &event_with_height.event {
            ibc_telemetry::global().send_packet_events(
                send_packet_ev.packet.sequence.into(),
                event_with_height.height.revision_height(),
                &self.src_chain().id(),
                self.src_channel_id(),
                self.src_port_id(),
                &self.dst_chain().id(),
            );
            ibc_telemetry::global().cleared_send_packet_events(
                send_packet_ev.packet.sequence.into(),
                event_with_height.height.revision_height(),
                &self.src_chain().id(),
                self.src_channel_id(),
                self.src_port_id(),
                &self.dst_chain().id(),
            );
        }
    }

    fn record_cleared_acknowledgments<'a>(
        &self,
        events_with_heights: impl Iterator<Item = &'a IbcEventWithHeight>,
    ) {
        for event_with_height in events_with_heights {
            if let IbcEvent::WriteAcknowledgement(write_ack_ev) = &event_with_height.event {
                ibc_telemetry::global().cleared_acknowledgment_events(
                    write_ack_ev.packet.sequence.into(),
                    event_with_height.height.revision_height(),
                    &self.src_chain().id(),
                    self.src_channel_id(),
                    self.src_port_id(),
                    &self.dst_chain().id(),
                );
            }
        }
    }
}

#[tracing::instrument(skip_all)]
fn check_ics20_fields_size(
    data: &[u8],
    memo_limit: Ics20FieldSizeLimit,
    receiver_limit: Ics20FieldSizeLimit,
    maybe_channel_version: Option<&Version>,
) -> bool {
    let channel_version = maybe_channel_version
        .cloned()
        .unwrap_or_else(|| Version::ics20(1));
    match extract_memo_and_receiver(&channel_version, data) {
        Ok((memo, receiver)) => {
            match (
                memo_limit.check_field_size(&memo),
                receiver_limit.check_field_size(&receiver),
            ) {
                (ValidationResult::Valid, ValidationResult::Valid) => true,

                (memo_validity, receiver_validity) => {
                    warn!("found invalid ICS-20 packet data, not relaying packet!");
                    warn!("    ICS-20 memo:     {memo_validity}");
                    warn!("    ICS-20 receiver: {receiver_validity}");

                    false
                }
            }
        }
        Err(e) => {
            trace!("failed to decode ICS20 packet data with error `{e}`");

            true
        }
    }
}

fn extract_memo_and_receiver(
    channel_version: &Version,
    data: &[u8],
) -> Result<(String, String), Error> {
    if channel_version.is_ics20_v2() {
        match serde_json::from_slice::<RawPacketDataV2>(data) {
            Ok(packet_data) => Ok((packet_data.memo, packet_data.receiver)),
            Err(e) => Err(e),
        }
    } else {
        match serde_json::from_slice::<RawPacketData>(data) {
            Ok(packet_data) => Ok((packet_data.memo, packet_data.receiver)),
            Err(e) => Err(e),
        }
    }
}
