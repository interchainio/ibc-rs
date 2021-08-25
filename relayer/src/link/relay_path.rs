use alloc::collections::BTreeMap as HashMap;
use alloc::collections::VecDeque;
use core::cell::RefCell;
use core::fmt;
use std::thread;
use std::time::Instant;

use itertools::Itertools;
use prost_types::Any;
use tracing::{debug, error, info, trace};

use ibc::{
    events::{IbcEvent, IbcEventType, PrettyEvents},
    ics04_channel::{
        channel::{ChannelEnd, Order, QueryPacketEventDataRequest, State as ChannelState},
        events::{SendPacket, WriteAcknowledgement},
        msgs::{
            acknowledgement::MsgAcknowledgement, chan_close_confirm::MsgChannelCloseConfirm,
            recv_packet::MsgRecvPacket, timeout::MsgTimeout, timeout_on_close::MsgTimeoutOnClose,
        },
        packet::{Packet, PacketMsgType, Sequence},
    },
    ics24_host::identifier::{ChannelId, ClientId, ConnectionId, PortId},
    query::QueryTxRequest,
    signer::Signer,
    timestamp::ZERO_DURATION,
    tx_msg::Msg,
    Height,
};
use ibc_proto::ibc::core::channel::v1::{
    QueryNextSequenceReceiveRequest, QueryUnreceivedAcksRequest, QueryUnreceivedPacketsRequest,
};

use crate::chain::counterparty::{
    unreceived_acknowledgements_sequences, unreceived_packets_sequences,
};
use crate::chain::handle::ChainHandle;
use crate::channel::error::ChannelError;
use crate::channel::Channel;
use crate::event::monitor::EventBatch;
use crate::foreign_client::{ForeignClient, ForeignClientError};
use crate::link::error::{self, LinkError};
use crate::link::operational_data::{OperationalData, OperationalDataTarget, TransitMessage};
use crate::link::pending::PendingTxs;
use crate::link::relay_sender::{AsyncReply, SubmitReply};
use crate::link::relay_summary::RelaySummary;
use crate::link::{pending, relay_sender};
use crate::util::queue::Queue;

const MAX_RETRIES: usize = 5;

pub struct RelayPath<ChainA: ChainHandle, ChainB: ChainHandle> {
    channel: Channel<ChainA, ChainB>,

    src_channel_id: ChannelId,
    src_port_id: PortId,

    dst_channel_id: ChannelId,
    dst_port_id: PortId,

    // Marks whether this path has already cleared pending packets.
    // Packets should be cleared once (at startup), then this
    // flag turns to `false`.
    clear_packets: RefCell<bool>,

    // Operational data, targeting both the source and destination chain.
    // These vectors of operational data are ordered decreasingly by
    // their age, with element at position `0` being the oldest.
    // The operational data targeting the source chain comprises
    // mostly timeout packet messages.
    // The operational data targeting the destination chain
    // comprises mostly RecvPacket and Ack msgs.
    src_operational_data: Queue<OperationalData>,
    dst_operational_data: Queue<OperationalData>,

    // The mediator stores pending operational data.
    // The relaying path periodically invokes the mediator
    // to confirm transactions.
    pending_txs_src: PendingTxs<ChainA>,
    pending_txs_dst: PendingTxs<ChainB>,
}

impl<ChainA: ChainHandle, ChainB: ChainHandle> RelayPath<ChainA, ChainB> {
    pub fn new(channel: Channel<ChainA, ChainB>) -> Result<Self, LinkError> {
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

        Ok(Self {
            channel,

            src_channel_id: src_channel_id.clone(),
            src_port_id: src_port_id.clone(),
            dst_channel_id: dst_channel_id.clone(),
            dst_port_id: dst_port_id.clone(),

            clear_packets: RefCell::new(true),
            src_operational_data: Queue::new(),
            dst_operational_data: Queue::new(),
            pending_txs_src: PendingTxs::new(src_chain, src_channel_id, src_port_id, dst_chain_id),
            pending_txs_dst: PendingTxs::new(dst_chain, dst_channel_id, dst_port_id, src_chain_id),
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
        &self.src_port_id
    }

    pub fn dst_port_id(&self) -> &PortId {
        &self.dst_port_id
    }

    pub fn src_channel_id(&self) -> &ChannelId {
        &self.src_channel_id
    }

    pub fn dst_channel_id(&self) -> &ChannelId {
        &self.dst_channel_id
    }

    pub fn channel(&self) -> &Channel<ChainA, ChainB> {
        &self.channel
    }

    fn src_channel(&self, height: Height) -> Result<ChannelEnd, LinkError> {
        self.src_chain()
            .query_channel(self.src_port_id(), self.src_channel_id(), height)
            .map_err(|e| LinkError::channel(ChannelError::query(self.src_chain().id(), e)))
    }

    fn dst_channel(&self, height: Height) -> Result<ChannelEnd, LinkError> {
        self.dst_chain()
            .query_channel(self.dst_port_id(), self.dst_channel_id(), height)
            .map_err(|e| LinkError::channel(ChannelError::query(self.src_chain().id(), e)))
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

    pub fn dst_latest_height(&self) -> Result<Height, LinkError> {
        self.dst_chain()
            .query_latest_height()
            .map_err(|e| LinkError::query(self.dst_chain().id(), e))
    }

    fn unordered_channel(&self) -> bool {
        self.channel.ordering == Order::Unordered
    }

    fn ordered_channel(&self) -> bool {
        self.channel.ordering == Order::Ordered
    }

    pub fn build_update_client_on_dst(&self, height: Height) -> Result<Vec<Any>, LinkError> {
        let client = self.restore_dst_client();
        client
            .build_update_client(height)
            .map_err(LinkError::client)
    }

    pub fn build_update_client_on_src(&self, height: Height) -> Result<Vec<Any>, LinkError> {
        let client = self.restore_src_client();
        client
            .build_update_client(height)
            .map_err(LinkError::client)
    }

    fn build_chan_close_confirm_from_event(&self, event: &IbcEvent) -> Result<Any, LinkError> {
        let src_channel_id = self.src_channel_id();
        let proofs = self
            .src_chain()
            .build_channel_proofs(self.src_port_id(), src_channel_id, event.height())
            .map_err(|e| LinkError::channel(ChannelError::channel_proof(e)))?;

        // Build the domain type message
        let new_msg = MsgChannelCloseConfirm {
            port_id: self.dst_port_id().clone(),
            channel_id: self.dst_channel_id().clone(),
            proofs,
            signer: self.dst_signer()?,
        };

        Ok(new_msg.to_any())
    }

    // Determines if the events received are relevant and should be processed.
    // Only events for a port/channel matching one of the channel ends should be processed.
    fn filter_relaying_events(&self, events: Vec<IbcEvent>) -> Vec<IbcEvent> {
        let src_channel_id = self.src_channel_id();

        let mut result = vec![];

        for event in events.into_iter() {
            match &event {
                IbcEvent::SendPacket(send_packet_ev) => {
                    if src_channel_id == send_packet_ev.src_channel_id()
                        && self.src_port_id() == send_packet_ev.src_port_id()
                    {
                        result.push(event);
                    }
                }
                IbcEvent::WriteAcknowledgement(write_ack_ev) => {
                    if src_channel_id == write_ack_ev.dst_channel_id()
                        && self.src_port_id() == write_ack_ev.dst_port_id()
                    {
                        result.push(event);
                    }
                }
                IbcEvent::CloseInitChannel(chan_close_ev) => {
                    if src_channel_id == chan_close_ev.channel_id()
                        && self.src_port_id() == chan_close_ev.port_id()
                    {
                        result.push(event);
                    }
                }
                IbcEvent::TimeoutPacket(timeout_ev) => {
                    if src_channel_id == timeout_ev.src_channel_id()
                        && self.channel.src_port_id() == timeout_ev.src_port_id()
                    {
                        result.push(event);
                    }
                }
                _ => {}
            }
        }
        result
    }

    fn relay_pending_packets(&self, height: Option<Height>) -> Result<(), LinkError> {
        for _ in 0..MAX_RETRIES {
            let cleared = self
                .build_recv_packet_and_timeout_msgs(height)
                .and_then(|()| self.build_packet_ack_msgs(height))
                .is_ok();

            if cleared {
                return Ok(());
            }
        }
        Err(LinkError::old_packet_clearing_failed())
    }

    fn should_clear_packets(&self) -> bool {
        *self.clear_packets.borrow()
    }

    /// Clears any packets that were sent before `height`, either if the `clear_packets` flag
    /// is set or if clearing is forced by the caller.
    pub fn schedule_packet_clearing(
        &self,
        height: Option<Height>,
        force: bool,
    ) -> Result<(), LinkError> {
        if self.should_clear_packets() || force {
            // Disable further clearing of old packets by default.
            // Clearing may still happen: upon new blocks, when `force = true`.
            self.clear_packets.replace(false);

            let clear_height = height
                .map(|h| h.decrement().map_err(|e| LinkError::decrement_height(h, e)))
                .transpose()?;

            info!(height = ?clear_height, "[{}] clearing pending packets", self);

            self.relay_pending_packets(clear_height)?;

            info!(height = ?clear_height, "[{}] finished scheduling pending packets clearing", self);
        }

        Ok(())
    }

    /// Generate & schedule operational data from the input `batch` of IBC events.
    pub fn update_schedule(&self, batch: EventBatch) -> Result<(), LinkError> {
        // Collect relevant events from the incoming batch & adjust their height.
        let events = self.filter_relaying_events(batch.events);

        // Transform the events into operational data items
        self.events_to_operational_data(events)
    }

    /// Produces and schedules operational data for this relaying path based on the input events.
    fn events_to_operational_data(&self, events: Vec<IbcEvent>) -> Result<(), LinkError> {
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
        input: Vec<IbcEvent>,
    ) -> Result<(Option<OperationalData>, Option<OperationalData>), LinkError> {
        if !input.is_empty() {
            info!(
                "[{}] generate messages from batch with {} events",
                self,
                input.len()
            );
        }
        let src_height = match input.get(0) {
            None => return Ok((None, None)),
            Some(ev) => ev.height(),
        };

        let dst_height = self.dst_latest_height()?;
        // Operational data targeting the source chain (e.g., Timeout packets)
        let mut src_od = OperationalData::new(dst_height, OperationalDataTarget::Source);
        // Operational data targeting the destination chain (e.g., SendPacket messages)
        let mut dst_od = OperationalData::new(src_height, OperationalDataTarget::Destination);

        for event in input {
            debug!("[{}] {} => {}", self, self.src_chain().id(), event);
            let (dst_msg, src_msg) = match event {
                IbcEvent::CloseInitChannel(_) => (
                    Some(self.build_chan_close_confirm_from_event(&event)?),
                    None,
                ),
                IbcEvent::TimeoutPacket(ref timeout_ev) => {
                    // When a timeout packet for an ordered channel is processed on-chain (src here)
                    // the chain closes the channel but no close init event is emitted, instead
                    // we get a timeout packet event (this happens for both unordered and ordered channels)
                    // Here we check that the channel is closed on src and send a channel close confirm
                    // to the counterparty.
                    if self.ordered_channel()
                        && self
                            .src_channel(timeout_ev.height)?
                            .state_matches(&ChannelState::Closed)
                    {
                        (
                            Some(self.build_chan_close_confirm_from_event(&event)?),
                            None,
                        )
                    } else {
                        (None, None)
                    }
                }
                IbcEvent::SendPacket(ref send_packet_ev) => {
                    if self.send_packet_event_handled(send_packet_ev)? {
                        debug!("[{}] {} already handled", self, send_packet_ev);
                        (None, None)
                    } else {
                        self.build_recv_or_timeout_from_send_packet_event(
                            send_packet_ev,
                            dst_height,
                        )?
                    }
                }
                IbcEvent::WriteAcknowledgement(ref write_ack_ev) => {
                    if self
                        .dst_channel(Height::zero())?
                        .state_matches(&ChannelState::Closed)
                    {
                        (None, None)
                    } else if self.write_ack_event_handled(write_ack_ev)? {
                        debug!("[{}] {} already handled", self, write_ack_ev);
                        (None, None)
                    } else {
                        (self.build_ack_from_recv_event(write_ack_ev)?, None)
                    }
                }
                _ => (None, None),
            };

            // Collect messages to be sent to the destination chain (e.g., RecvPacket)
            if let Some(msg) = dst_msg {
                debug!(
                    "[{}] {} <= {} from {}",
                    self,
                    self.dst_chain().id(),
                    msg.type_url,
                    event
                );
                dst_od.batch.push(TransitMessage {
                    event: event.clone(),
                    msg,
                });
            }

            // Collect timeout messages, to be sent to the source chain
            if let Some(msg) = src_msg {
                // For Ordered channels a single timeout event should be sent as this closes the channel.
                // Otherwise a multi message transaction will fail.
                if self.unordered_channel() || src_od.batch.is_empty() {
                    debug!(
                        "[{}] {} <= {} from {}",
                        self,
                        self.src_chain().id(),
                        msg.type_url,
                        event
                    );
                    src_od.batch.push(TransitMessage { event, msg });
                }
            }
        }

        let src_od_res = if src_od.batch.is_empty() {
            None
        } else {
            Some(src_od)
        };

        let dst_od_res = if dst_od.batch.is_empty() {
            None
        } else {
            Some(dst_od)
        };

        Ok((src_od_res, dst_od_res))
    }

    /// Relays an [`OperationalData`] using a specific
    /// sender, which implements [`relay_sender::Submit`].
    pub(crate) fn relay_from_operational_data<S: relay_sender::Submit>(
        &self,
        initial_od: OperationalData,
    ) -> Result<S::Reply, LinkError> {
        // We will operate on potentially different operational data if the initial one fails.
        let mut odata = initial_od;

        for i in 0..MAX_RETRIES {
            info!(
                "[{}] relay op. data of {} msgs(s) to {} (height {}), delayed by: {:?} [try {}/{}]",
                self,
                odata.batch.len(),
                odata.target,
                odata.proofs_height.increment(),
                odata.scheduled_time.elapsed(),
                i + 1,
                MAX_RETRIES
            );

            // Consume the operational data by attempting to send its messages
            match self.send_from_operational_data::<S>(odata.clone()) {
                Ok(reply) => {
                    // Done with this op. data
                    info!("[{}] success", self);

                    return Ok(reply);
                }
                Err(LinkError(error::LinkErrorDetail::Send(e), _)) => {
                    // This error means we could retry
                    error!("[{}] error {}", self, e.event);
                    if i + 1 == MAX_RETRIES {
                        error!(
                            "[{}] {}/{} retries exhausted. giving up",
                            self,
                            i + 1,
                            MAX_RETRIES
                        )
                    } else {
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

    /// Helper for managing retries of the `relay_from_operational_data` method.
    /// Expects as input the initial operational data that failed to send.
    ///
    /// Return value:
    ///   - `Some(..)`: a new operational data from which to retry sending,
    ///   - `None`: all the events in the initial operational data were exhausted (i.e., turned
    ///   into timeouts), so there is nothing to retry.
    ///
    /// Side effects: may schedule a new operational data targeting the source chain, comprising
    /// new timeout messages.
    fn regenerate_operational_data(
        &self,
        initial_odata: OperationalData,
    ) -> Option<OperationalData> {
        info!(
            "[{}] failed. Regenerate operational data from {} events",
            self,
            initial_odata.events().len()
        );

        // Retry by re-generating the operational data using the initial events
        let (src_opt, dst_opt) = match self.generate_operational_data(initial_odata.events()) {
            Ok(new_operational_data) => new_operational_data,
            Err(e) => {
                error!(
                    "[{}] failed to regenerate operational data from initial data: {} \
                    with error {}, discarding this op. data",
                    self, initial_odata, e
                );
                return None;
            } // Cannot retry, contain the error by reporting a None
        };

        if let Some(src_od) = src_opt {
            if src_od.target == initial_odata.target {
                // Our target is the _source_ chain, retry these messages
                info!("[{}] will retry with op data {}", self, src_od);
                return Some(src_od);
            } else {
                // Our target is the _destination_ chain, the data in `src_od` contains
                // potentially new timeout messages that have to be handled separately.
                if let Err(e) = self.schedule_operational_data(src_od) {
                    error!(
                        "[{}] failed to schedule newly-generated operational data from \
                    initial data: {} with error {}, discarding this op. data",
                        self, initial_odata, e
                    );
                    return None;
                }
            }
        }

        if let Some(dst_od) = dst_opt {
            if dst_od.target == initial_odata.target {
                // Our target is the _destination_ chain, retry these messages
                info!("[{}] will retry with op data {}", self, dst_od);
                return Some(dst_od);
            } else {
                // Our target is the _source_ chain, but `dst_od` has new messages
                // intended for the destination chain, this should never be the case
                error!(
                    "[{}] generated new messages for destination chain while handling \
                    failed events targeting the source chain!",
                    self
                );
            }
        } else {
            // There is no message intended for the destination chain
            if initial_odata.target == OperationalDataTarget::Destination {
                info!("[{}] exhausted all events from this operational data", self);
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
        odata: OperationalData,
    ) -> Result<S::Reply, LinkError> {
        if odata.batch.is_empty() {
            error!("[{}] ignoring empty operational data!", self);
            return Ok(S::Reply::empty());
        }

        let msgs = odata.assemble_msgs(self)?;

        match odata.target {
            OperationalDataTarget::Source => S::submit(self.src_chain(), msgs),
            OperationalDataTarget::Destination => S::submit(self.dst_chain(), msgs),
        }
    }

    fn enqueue_pending_tx(&self, reply: AsyncReply, odata: OperationalData) {
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
                port_id: self.dst_port_id().to_string(),
                channel_id: self.dst_channel_id().to_string(),
                packet_commitment_sequences: vec![packet.sequence.into()],
            })
            .map_err(LinkError::relayer)?;

        Ok(unreceived_packet.is_empty())
    }

    /// Checks if a packet commitment has been cleared on source.
    /// The packet commitment is cleared when either an acknowledgment or a timeout is received on source.
    fn send_packet_commitment_cleared_on_src(&self, packet: &Packet) -> Result<bool, LinkError> {
        let (bytes, _) = self
            .src_chain()
            .build_packet_proofs(
                PacketMsgType::Recv,
                self.src_port_id(),
                self.src_channel_id(),
                packet.sequence,
                Height::zero(),
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
            .query_unreceived_acknowledgement(QueryUnreceivedAcksRequest {
                port_id: self.dst_port_id().to_string(),
                channel_id: self.dst_channel_id().to_string(),
                packet_ack_sequences: vec![packet.sequence.into()],
            })
            .map_err(LinkError::relayer)?;

        Ok(unreceived_ack.is_empty())
    }

    /// Checks if a receive packet event has already been handled (e.g. by another relayer).
    fn write_ack_event_handled(&self, rp: &WriteAcknowledgement) -> Result<bool, LinkError> {
        self.recv_packet_acknowledged_on_src(&rp.packet)
    }

    /// Returns `true` if the delay for this relaying path is zero.
    /// Conversely, returns `false` if the delay is non-zero.
    pub fn zero_delay(&self) -> bool {
        self.channel.connection_delay == ZERO_DURATION
    }

    /// Handles updating the client on the destination chain
    fn update_client_dst(&self, src_chain_height: Height) -> Result<(), LinkError> {
        // Handle the update on the destination chain
        // Check if a consensus state at update_height exists on destination chain already
        if self
            .dst_chain()
            .proven_client_consensus(self.dst_client_id(), src_chain_height, Height::zero())
            .is_ok()
        {
            return Ok(());
        }

        let mut dst_err_ev = None;
        for i in 0..MAX_RETRIES {
            let dst_update = self.build_update_client_on_dst(src_chain_height)?;
            info!(
                "[{}] sending updateClient to client hosted on dest. chain {} for height {} [try {}/{}]",
                self,
                self.dst_chain().id(),
                src_chain_height,
                i + 1, MAX_RETRIES,
            );

            let dst_tx_events = self
                .dst_chain()
                .send_messages_and_wait_commit(dst_update)
                .map_err(LinkError::relayer)?;
            info!("[{}] result {}\n", self, PrettyEvents(&dst_tx_events));

            dst_err_ev = dst_tx_events
                .into_iter()
                .find(|event| matches!(event, IbcEvent::ChainError(_)));

            if dst_err_ev.is_none() {
                return Ok(());
            }
        }

        Err(LinkError::client(ForeignClientError::chain_error_event(
            self.dst_chain().id(),
            dst_err_ev.unwrap(),
        )))
    }

    /// Handles updating the client on the source chain
    fn update_client_src(&self, dst_chain_height: Height) -> Result<(), LinkError> {
        if self
            .src_chain()
            .proven_client_consensus(self.src_client_id(), dst_chain_height, Height::zero())
            .is_ok()
        {
            return Ok(());
        }

        let mut src_err_ev = None;
        for _ in 0..MAX_RETRIES {
            let src_update = self.build_update_client_on_src(dst_chain_height)?;
            info!(
                "[{}] sending updateClient to client hosted on src. chain {} for height {}",
                self,
                self.src_chain().id(),
                dst_chain_height,
            );

            let src_tx_events = self
                .src_chain()
                .send_messages_and_wait_commit(src_update)
                .map_err(LinkError::relayer)?;
            info!("[{}] result {}\n", self, PrettyEvents(&src_tx_events));

            src_err_ev = src_tx_events
                .into_iter()
                .find(|event| matches!(event, IbcEvent::ChainError(_)));

            if src_err_ev.is_none() {
                return Ok(());
            }
        }

        Err(LinkError::client(ForeignClientError::chain_error_event(
            self.src_chain().id(),
            src_err_ev.unwrap(),
        )))
    }

    /// Returns relevant packet events for building RecvPacket and timeout messages.
    /// Additionally returns the height (on source chain) corresponding to these events.
    fn target_height_and_send_packet_events(
        &self,
        opt_query_height: Option<Height>,
    ) -> Result<(Vec<IbcEvent>, Height), LinkError> {
        let mut events_result = vec![];

        let src_channel_id = self.src_channel_id();
        let dst_channel_id = self.dst_channel_id();

        let (commit_sequences, sequences, src_response_height) = unreceived_packets_sequences(
            self.src_chain(),
            src_channel_id,
            self.src_port_id(),
            self.dst_chain(),
            dst_channel_id,
            self.dst_port_id(),
        )
        .map_err(LinkError::supervisor)?;

        let query_height = opt_query_height.unwrap_or(src_response_height);

        let sequences: Vec<Sequence> = sequences.into_iter().map(From::from).collect();
        if sequences.is_empty() {
            return Ok((events_result, query_height));
        }

        debug!(
            "[{}] packets that still have commitments on {}: {} (first 10 shown here; total={})",
            self,
            self.src_chain().id(),
            commit_sequences.iter().take(10).join(", "),
            commit_sequences.len()
        );

        debug!(
            "[{}] recv packets to send out to {} of the ones with commitments on source {}: {} (first 10 shown here; total={})",
            self,
            self.dst_chain().id(),
            self.src_chain().id(),
            sequences.iter().take(10).join(", "), sequences.len()
        );

        let query = QueryTxRequest::Packet(QueryPacketEventDataRequest {
            event_id: IbcEventType::SendPacket,
            source_port_id: self.src_port_id().clone(),
            source_channel_id: src_channel_id.clone(),
            destination_port_id: self.dst_port_id().clone(),
            destination_channel_id: dst_channel_id.clone(),
            sequences,
            height: query_height,
        });

        events_result = self
            .src_chain()
            .query_txs(query)
            .map_err(LinkError::relayer)?;

        let mut packet_sequences = vec![];
        for event in events_result.iter() {
            match event {
                IbcEvent::SendPacket(send_event) => {
                    packet_sequences.push(send_event.packet.sequence);
                    if packet_sequences.len() > 10 {
                        // Enough to print the first 10
                        break;
                    }
                }
                _ => return Err(LinkError::unexpected_event(event.clone())),
            }
        }
        info!(
            "[{}] found unprocessed SendPacket events for {:?} (first 10 shown here; total={})",
            self,
            packet_sequences,
            events_result.len()
        );

        Ok((events_result, query_height))
    }

    /// Returns relevant packet events for building ack messages.
    /// Additionally returns the height (on source chain) corresponding to these events.
    fn target_height_and_write_ack_events(
        &self,
        opt_query_height: Option<Height>,
    ) -> Result<(Vec<IbcEvent>, Height), LinkError> {
        let mut events_result = vec![];

        let src_channel_id = self.src_channel_id();
        let dst_channel_id = self.dst_channel_id();

        let (acked_sequences, sequences, src_response_height) =
            unreceived_acknowledgements_sequences(
                self.src_chain(),
                src_channel_id,
                self.src_port_id(),
                self.dst_chain(),
                dst_channel_id,
                self.dst_port_id(),
            )
            .map_err(LinkError::supervisor)?;

        let query_height = opt_query_height.unwrap_or(src_response_height);

        let sequences: Vec<Sequence> = sequences.into_iter().map(From::from).collect();
        if sequences.is_empty() {
            return Ok((events_result, query_height));
        }

        debug!(
            "[{}] packets that have acknowledgments on {}: [{:?}..{:?}] (total={})",
            self,
            self.src_chain().id(),
            acked_sequences.first(),
            acked_sequences.last(),
            acked_sequences.len()
        );

        debug!(
            "[{}] ack packets to send out to {} of the ones with acknowledgments on {}: {} (first 10 shown here; total={})",
            self,
            self.dst_chain().id(),
            self.src_chain().id(),
            sequences.iter().take(10).join(", "), sequences.len()
        );

        events_result = self
            .src_chain()
            .query_txs(QueryTxRequest::Packet(QueryPacketEventDataRequest {
                event_id: IbcEventType::WriteAck,
                source_port_id: self.dst_port_id().clone(),
                source_channel_id: dst_channel_id.clone(),
                destination_port_id: self.src_port_id().clone(),
                destination_channel_id: src_channel_id.clone(),
                sequences,
                height: query_height,
            }))
            .map_err(|e| LinkError::query(self.src_chain().id(), e))?;

        let mut packet_sequences = vec![];
        for event in events_result.iter() {
            match event {
                IbcEvent::WriteAcknowledgement(write_ack_event) => {
                    packet_sequences.push(write_ack_event.packet.sequence);
                    if packet_sequences.len() > 10 {
                        // Enough to print the first 10
                        break;
                    }
                }
                _ => {
                    return Err(LinkError::unexpected_event(event.clone()));
                }
            }
        }
        info!("[{}] found unprocessed WriteAcknowledgement events for {:?} (first 10 shown here; total={})", self, packet_sequences, events_result.len());

        Ok((events_result, query_height))
    }

    /// Schedules the relaying of RecvPacket and Timeout messages.
    /// The `opt_query_height` parameter allows to optionally use a specific height on the source
    /// chain where to query for packet data. If `None`, the latest available height on the source
    /// chain is used.
    pub fn build_recv_packet_and_timeout_msgs(
        &self,
        opt_query_height: Option<Height>,
    ) -> Result<(), LinkError> {
        // Get the events for the send packets on source chain that have not been received on
        // destination chain (i.e. ack was not seen on source chain).
        let (mut events, height) = self.target_height_and_send_packet_events(opt_query_height)?;

        // Skip: no relevant events found.
        if events.is_empty() {
            return Ok(());
        }

        for event in events.iter_mut() {
            event.set_height(height);
        }

        self.events_to_operational_data(events)?;

        Ok(())
    }

    /// Schedules the relaying of packet acknowledgment messages.
    /// The `opt_query_height` parameter allows to optionally use a specific height on the source
    /// chain where to query for packet data. If `None`, the latest available height on the source
    /// chain is used.
    pub fn build_packet_ack_msgs(&self, opt_query_height: Option<Height>) -> Result<(), LinkError> {
        // Get the sequences of packets that have been acknowledged on destination chain but still
        // have commitments on source chain (i.e. ack was not seen on source chain)
        let (mut events, height) = self.target_height_and_write_ack_events(opt_query_height)?;

        // Skip: no relevant events found.
        if events.is_empty() {
            return Ok(());
        }

        for event in events.iter_mut() {
            event.set_height(height);
        }

        self.events_to_operational_data(events)?;
        Ok(())
    }

    fn build_recv_packet(&self, packet: &Packet, height: Height) -> Result<Option<Any>, LinkError> {
        let (_, proofs) = self
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

        trace!(
            "[{}] built recv_packet msg {}, proofs at height {}",
            self,
            msg.packet,
            proofs.height()
        );

        Ok(Some(msg.to_any()))
    }

    fn build_ack_from_recv_event(
        &self,
        event: &WriteAcknowledgement,
    ) -> Result<Option<Any>, LinkError> {
        let packet = event.packet.clone();

        let (_, proofs) = self
            .src_chain()
            .build_packet_proofs(
                PacketMsgType::Ack,
                &packet.destination_port,
                &packet.destination_channel,
                packet.sequence,
                event.height,
            )
            .map_err(|e| LinkError::packet_proofs_constructor(self.src_chain().id(), e))?;

        let msg = MsgAcknowledgement::new(
            packet,
            event.ack.clone(),
            proofs.clone(),
            self.dst_signer()?,
        );

        trace!(
            "[{}] built acknowledgment msg {}, proofs at height {}",
            self,
            msg.packet,
            proofs.height()
        );

        Ok(Some(msg.to_any()))
    }

    fn build_timeout_packet(
        &self,
        packet: &Packet,
        height: Height,
    ) -> Result<Option<Any>, LinkError> {
        let dst_channel_id = self.dst_channel_id();

        let (packet_type, next_sequence_received) = if self.ordered_channel() {
            let next_seq = self
                .dst_chain()
                .query_next_sequence_receive(QueryNextSequenceReceiveRequest {
                    port_id: self.dst_port_id().to_string(),
                    channel_id: dst_channel_id.to_string(),
                })
                .map_err(|e| LinkError::query(self.dst_chain().id(), e))?;
            (PacketMsgType::TimeoutOrdered, next_seq)
        } else {
            (PacketMsgType::TimeoutUnordered, packet.sequence)
        };

        let (_, proofs) = self
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

        trace!(
            "[{}] built timeout msg {}, proofs at height {}",
            self,
            msg.packet,
            proofs.height()
        );

        Ok(Some(msg.to_any()))
    }

    fn build_timeout_on_close_packet(
        &self,
        packet: &Packet,
        height: Height,
    ) -> Result<Option<Any>, LinkError> {
        let (_, proofs) = self
            .dst_chain()
            .build_packet_proofs(
                PacketMsgType::TimeoutOnClose,
                &packet.destination_port,
                &packet.destination_channel,
                packet.sequence,
                height,
            )
            .map_err(|e| LinkError::packet_proofs_constructor(self.dst_chain().id(), e))?;

        let msg = MsgTimeoutOnClose::new(
            packet.clone(),
            packet.sequence,
            proofs.clone(),
            self.src_signer()?,
        );

        trace!(
            "[{}] built timeout on close msg {}, proofs at height {}",
            self,
            msg.packet,
            proofs.height()
        );

        Ok(Some(msg.to_any()))
    }

    fn build_timeout_from_send_packet_event(
        &self,
        event: &SendPacket,
        dst_chain_height: Height,
    ) -> Result<Option<Any>, LinkError> {
        let packet = event.packet.clone();
        if self
            .dst_channel(dst_chain_height)?
            .state_matches(&ChannelState::Closed)
        {
            Ok(self.build_timeout_on_close_packet(&event.packet, dst_chain_height)?)
        } else if packet.timed_out(dst_chain_height) {
            Ok(self.build_timeout_packet(&event.packet, dst_chain_height)?)
        } else {
            Ok(None)
        }
    }

    fn build_recv_or_timeout_from_send_packet_event(
        &self,
        event: &SendPacket,
        dst_chain_height: Height,
    ) -> Result<(Option<Any>, Option<Any>), LinkError> {
        let timeout = self.build_timeout_from_send_packet_event(event, dst_chain_height)?;
        if timeout.is_some() {
            Ok((None, timeout))
        } else {
            Ok((self.build_recv_packet(&event.packet, event.height)?, None))
        }
    }

    /// Checks if there are any operational data items ready,
    /// and if so performs the relaying of corresponding packets
    /// to the target chain.
    ///
    /// This method performs relaying using the asynchronous sender.
    /// Retains the operational data as pending, and associates it
    /// with one or more transaction hash(es).
    pub fn execute_schedule(&self) -> Result<(), LinkError> {
        let (src_ods, dst_ods) = self.try_fetch_scheduled_operational_data();

        for od in dst_ods {
            let reply =
                self.relay_from_operational_data::<relay_sender::AsyncSender>(od.clone())?;

            self.enqueue_pending_tx(reply, od);
        }

        for od in src_ods {
            let reply =
                self.relay_from_operational_data::<relay_sender::AsyncSender>(od.clone())?;
            self.enqueue_pending_tx(reply, od);
        }

        Ok(())
    }

    pub fn process_pending_txs(&self) -> RelaySummary {
        let mut summary_src = self.process_pending_txs_src().unwrap_or_else(|e| {
            error!("error processing pending events in source chain: {}", e);
            RelaySummary::empty()
        });

        let summary_dst = self.process_pending_txs_dst().unwrap_or_else(|e| {
            error!(
                "error processing pending events in destination chain: {}",
                e
            );
            RelaySummary::empty()
        });

        summary_src.extend(summary_dst);
        summary_src
    }

    fn process_pending_txs_src(&self) -> Result<RelaySummary, LinkError> {
        let res = self
            .pending_txs_src
            .process_pending(pending::TIMEOUT, |odata| {
                self.relay_from_operational_data::<relay_sender::AsyncSender>(odata)
            })?
            .unwrap_or_else(RelaySummary::empty);

        Ok(res)
    }

    fn process_pending_txs_dst(&self) -> Result<RelaySummary, LinkError> {
        let res = self
            .pending_txs_dst
            .process_pending(pending::TIMEOUT, |odata| {
                self.relay_from_operational_data::<relay_sender::AsyncSender>(odata)
            })?
            .unwrap_or_else(RelaySummary::empty);

        Ok(res)
    }

    /// Refreshes the scheduled batches.
    /// Verifies if any sendPacket messages timed-out. If so, moves them from destination op. data
    /// to source operational data, and adjusts the events and messages accordingly.
    pub fn refresh_schedule(&self) -> Result<(), LinkError> {
        let dst_current_height = self.dst_latest_height()?;

        // Intermediary data struct to help better manage the transfer from dst. operational data
        // to source operational data.
        let mut all_dst_odata = self.dst_operational_data.clone_vec();

        let mut timed_out: HashMap<usize, Vec<TransitMessage>> = HashMap::default();

        // For each operational data targeting the destination chain...
        for (odata_pos, odata) in all_dst_odata.iter_mut().enumerate() {
            // ... check each `SendPacket` event, whether it should generate a timeout message
            let mut retain_batch = vec![];

            for gm in odata.batch.iter() {
                let TransitMessage { event, .. } = gm;

                match event {
                    IbcEvent::SendPacket(e) => {
                        // Catch any SendPacket event that timed-out
                        if self.send_packet_event_handled(e)? {
                            debug!(
                                "[{}] refreshing schedule: already handled send packet {}",
                                self, e
                            );
                        } else if let Some(new_msg) =
                            self.build_timeout_from_send_packet_event(e, dst_current_height)?
                        {
                            debug!(
                                "[{}] refreshing schedule: found a timed-out msg in the op data {}",
                                self, odata
                            );
                            timed_out.entry(odata_pos).or_insert_with(Vec::new).push(
                                TransitMessage {
                                    event: event.clone(),
                                    msg: new_msg,
                                },
                            );
                        } else {
                            // A SendPacket event, but did not time-out yet, retain
                            retain_batch.push(gm.clone());
                        }
                    }
                    IbcEvent::WriteAcknowledgement(e) => {
                        if self.write_ack_event_handled(e)? {
                            debug!(
                                "[{}] refreshing schedule: already handled {} write ack ",
                                self, e
                            );
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
        for (_, batch) in timed_out.into_iter() {
            let mut new_od =
                OperationalData::new(dst_current_height, OperationalDataTarget::Source);

            new_od.batch = batch;

            info!(
                "[{}] refreshing schedule: re-scheduling from new timed-out batch of size {}",
                self,
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
        if od.batch.is_empty() {
            info!(
                "[{}] ignoring operational data for {} because it has no messages",
                self, od.target
            );
            return Ok(());
        }

        info!(
            "[{}] scheduling op. data with {} msg(s) for {} (height {})",
            self,
            od.batch.len(),
            od.target,
            od.proofs_height.increment(), // increment for easier correlation with the client logs
        );

        // Update clients ahead of scheduling the operational data, if the delays are non-zero.
        if !self.zero_delay() {
            let target_height = od.proofs_height.increment();
            match od.target {
                OperationalDataTarget::Source => self.update_client_src(target_height)?,
                OperationalDataTarget::Destination => self.update_client_dst(target_height)?,
            };
        }

        od.scheduled_time = Instant::now();

        match od.target {
            OperationalDataTarget::Source => self.src_operational_data.push_back(od),
            OperationalDataTarget::Destination => self.dst_operational_data.push_back(od),
        };

        Ok(())
    }

    /// Pulls out the operational elements with elapsed delay period and that can
    /// now be processed. Does not block: if no OD fulfilled the delay period (or none is
    /// scheduled), returns immediately with `vec![]`.
    fn try_fetch_scheduled_operational_data(
        &self,
    ) -> (VecDeque<OperationalData>, VecDeque<OperationalData>) {
        // Extracts elements from a Vec when the predicate returns true.
        // The mutable vector is then updated to the remaining unextracted elements.
        fn partition<T>(
            queue: VecDeque<T>,
            pred: impl Fn(&T) -> bool,
        ) -> (VecDeque<T>, VecDeque<T>) {
            let mut true_res = VecDeque::new();
            let mut false_res = VecDeque::new();

            for e in queue.into_iter() {
                if pred(&e) {
                    true_res.push_back(e);
                } else {
                    false_res.push_back(e);
                }
            }

            (true_res, false_res)
        }

        let connection_delay = self.channel.connection_delay;
        let (elapsed_src_ods, unelapsed_src_ods) =
            partition(self.src_operational_data.take(), |op| {
                op.scheduled_time.elapsed() > connection_delay
            });

        self.src_operational_data.replace(unelapsed_src_ods);

        let (elapsed_dst_ods, unelapsed_dst_ods) =
            partition(self.dst_operational_data.take(), |op| {
                op.scheduled_time.elapsed() > connection_delay
            });

        self.dst_operational_data.replace(unelapsed_dst_ods);

        (elapsed_src_ods, elapsed_dst_ods)
    }

    /// Fetches an operational data that has fulfilled its predefined delay period. May _block_
    /// waiting for the delay period to pass.
    /// Returns `None` if there is no operational data scheduled.
    pub(crate) fn fetch_scheduled_operational_data(&self) -> Option<OperationalData> {
        let odata = self
            .src_operational_data
            .pop_front()
            .or_else(|| self.dst_operational_data.pop_front());

        if let Some(odata) = odata {
            // Check if the delay period did not completely elapse
            let delay_left = self
                .channel
                .connection_delay
                .checked_sub(odata.scheduled_time.elapsed());

            match delay_left {
                None => info!(
                    "[{}] ready to fetch a scheduled op. data with batch of size {} targeting {}",
                    self,
                    odata.batch.len(),
                    odata.target,
                ),
                Some(delay_left) => {
                    info!(
                        "[{}] waiting ({:?} left) for a scheduled op. data with batch of size {} targeting {}",
                        self,
                        delay_left,
                        odata.batch.len(),
                        odata.target,
                    );

                    // Wait until the delay period passes
                    thread::sleep(delay_left);
                }
            }

            Some(odata)
        } else {
            None
        }
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
}

impl<ChainA: ChainHandle, ChainB: ChainHandle> fmt::Display for RelayPath<ChainA, ChainB> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}/{} -> {}",
            self.src_chain().id(),
            self.src_port_id(),
            self.src_channel_id(),
            self.dst_chain().id()
        )
    }
}
