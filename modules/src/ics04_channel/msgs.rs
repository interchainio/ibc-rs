//! Message definitions for all ICS4 domain types: channel open & close handshake datagrams, as well
//! as packets.

use acknowledgement::MsgAcknowledgement;

use crate::ics04_channel::msgs::chan_close_confirm::MsgChannelCloseConfirm;
use crate::ics04_channel::msgs::chan_close_init::MsgChannelCloseInit;
use crate::ics04_channel::msgs::chan_open_ack::MsgChannelOpenAck;
use crate::ics04_channel::msgs::chan_open_confirm::MsgChannelOpenConfirm;
use crate::ics04_channel::msgs::chan_open_init::MsgChannelOpenInit;
use crate::ics04_channel::msgs::chan_open_try::MsgChannelOpenTry;

use self::{recv_packet::MsgRecvPacket, timeout::MsgTimeout, timeout_on_close::MsgTimeoutOnClose};

// Opening handshake messages.
pub mod chan_open_ack;
pub mod chan_open_confirm;
pub mod chan_open_init;
pub mod chan_open_try;

// Closing handshake messages.
pub mod chan_close_confirm;
pub mod chan_close_init;

// Packet specific messages.
pub mod acknowledgement;
pub mod recv_packet;
pub mod timeout;
pub mod timeout_on_close;

/// Enumeration of all possible messages that the ICS4 protocol processes.
#[derive(Clone, Debug, PartialEq)]
pub enum ChannelMsg {
    ChannelOpenInit(MsgChannelOpenInit),
    ChannelOpenTry(MsgChannelOpenTry),
    ChannelOpenAck(MsgChannelOpenAck),
    ChannelOpenConfirm(MsgChannelOpenConfirm),
    ChannelCloseInit(MsgChannelCloseInit),
    ChannelCloseConfirm(MsgChannelCloseConfirm),
}

/*
    `PacketMsg` represents packet messages sent between an ALPHA chain
    and a BETA chain.

    The roles of ALPHA and BETA are FIXED and never change during
    the relaying session. DIFFERENT relaying sessions are executed
    for the case where the ALPHA and BETA roles are FLIPPED

    Depending on the variant, a packet can flow in *either* direction,
    and the exact flow is documented below.

    All variants are parameterized by the SAME ALPHA and BETA
    chains, i.e. the chain roles DO NOT FLIP.

    In the same relaying session, the packet's ALPHA chain is
    ALWAYS the SOURCE chain, and the packet's BETA
    chain is ALWAYS the DESTINATION chain.
*/
#[derive(Clone, Debug, PartialEq)]
pub enum PacketMsg {
    /*
        A `RecvPacket` represents the ALPHA chain RECEIVING
        an INCOMING packet from the BETA chain.
     */
    RecvPacket(MsgRecvPacket),

    /*
        An `AckPacket` represents the ALPHA chain SENDING
        an acknowlegement as OUTGOING packet to the BETA chain.
    */
    AckPacket(MsgAcknowledgement),

    /*
        A `ToPacket` represents the ALPHA chain RECEIVING
        a timeout INCOMING packet from the BETA chain.
     */
    ToPacket(MsgTimeout),

    /*
        A `ToClosePacket` represents the ALPHA chain RECEIVING
        a timeout-on-close INCOMING packet from the BETA chain.
     */
    ToClosePacket(MsgTimeoutOnClose),
}
