use std::convert::TryFrom;
use std::str::FromStr;

use serde_derive::{Deserialize, Serialize};

use ibc_proto::ibc::core::channel::v1::Packet as RawPacket;

use crate::ics04_channel::error::Error;
use crate::ics24_host::identifier::{ChannelId, PortId};
use crate::timestamp::{Expiry::Expired, Timestamp};
use crate::Height;

use super::handler::{
    acknowledgement::AckPacketResult, recv_packet::RecvPacketResult, send_packet::SendPacketResult,
    timeout::TimeoutPacketResult, write_acknowledgement::WriteAckPacketResult,
};

/*
    Enumeration of proof carrying ICS4 message, helper for relayer.

    The `PacketMsgType` is used for identifying the message type
    for packets sent between an ALPHA chain an a BETA chain.

    It also identifies the DIRECTION on the packet, on whether
    it is an INCOMING or OUTGOING packet.
 */
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PacketMsgType {
    // INCOMING packet
    Recv,

    // OUTGOING packet
    Ack,
    TimeoutUnordered,

    // INCOMING packet
    TimeoutOrdered,

    // INCOMING packet
    TimeoutOnClose,
}

#[derive(Clone, Debug)]
pub enum PacketResult {
    Send(SendPacketResult),
    Recv(RecvPacketResult),
    WriteAck(WriteAckPacketResult),
    Ack(AckPacketResult),
    Timeout(TimeoutPacketResult),
}

#[derive(Clone, Debug)]
pub enum Receipt {
    Ok,
}

impl std::fmt::Display for PacketMsgType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PacketMsgType::Recv => write!(f, "(PacketMsgType::Recv)"),
            PacketMsgType::Ack => write!(f, "(PacketMsgType::Ack)"),
            PacketMsgType::TimeoutUnordered => write!(f, "(PacketMsgType::TimeoutUnordered)"),
            PacketMsgType::TimeoutOrdered => write!(f, "(PacketMsgType::TimeoutOrdered)"),
            PacketMsgType::TimeoutOnClose => write!(f, "(PacketMsgType::TimeoutOnClose)"),
        }
    }
}

/// The sequence number of a packet enforces ordering among packets from the same source.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct Sequence(u64);

impl Default for Sequence {
    fn default() -> Self {
        Sequence(0)
    }
}

impl FromStr for Sequence {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::from(s.parse::<u64>().map_err(|e| {
            Error::invalid_string_as_sequence(s.to_string(), e)
        })?))
    }
}

impl Sequence {
    pub fn is_zero(&self) -> bool {
        self.0 == 0
    }

    pub fn increment(&self) -> Sequence {
        Sequence(self.0 + 1)
    }
}

impl From<u64> for Sequence {
    fn from(seq: u64) -> Self {
        Sequence(seq)
    }
}

impl From<Sequence> for u64 {
    fn from(s: Sequence) -> u64 {
        s.0
    }
}

impl std::fmt::Display for Sequence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "{}", self.0)
    }
}

/*
    A `Packet` represents datagrams flowing between
    a HOST chain and a COUNTERPARTY chain.
    It is in fact used in TWO DIRECTIONS: INCOMING and OUTGOING.

    When using a packet, one must identify the HOST chain,
    the COUNTERPARTY chain, *and* also the DIRECTION.
    Inside a relaying session, the HOST and COUNTERPARTY
    chains of the packets remain FIXED and are NEVER FLIPPED.

    Depending on the packet mode, some fields in `Packet`
    belongs to DIFFERENT chain. This is further noted below.
 */
#[derive(PartialEq, Deserialize, Serialize, Hash, Clone)]
pub struct Packet {
    /*
        When the DIRECTION is INCOMING, the sequence field
        identifies the sequence on the COUNTERPARTY chain.

        When the DIRECTION is OUTGOING, the sequence field
        identifies the sequence on the HOST chain.
     */
    pub sequence: Sequence,

    /*
        The source port is the port ID for the COUNTERPARTY chain
        that is REGISTERED ON the HOST chain.
     */
    pub source_port: PortId,

    /*
        The source channel is the channel ID for the COUNTERPARTY chain
        that is REGISTERED ON the HOST chain.
     */
    pub source_channel: ChannelId,

    /*
        The destination port is the port ID for the HOST chain
        that is REGISTERED ON the COUNTERPARTY chain.
     */
    pub destination_port: PortId,

    /*
        The destination channel is the channel ID for the
        HOST chain that is REGISTERD ON the COUNTERPARTY chain.
     */
    pub destination_channel: ChannelId,
    #[serde(serialize_with = "crate::serializers::ser_hex_upper")]

    /*
        If the DIRECTION is INCOMING, the data is PRODUCED BY
        the COUNTERPARTY chain.

        If the DIRECTION is OUTGOING, the data is PRODUCED BY
        the HOST chain.
     */
    pub data: Vec<u8>,

    /*
        If the DIRECTION is INCOMING, this is the TIMEOUT HEIGHT
        of the HOST chain. If the HOST chain PROCESS the packet
        after it reaches the TIMEOUT HEIGHT, it REJECTs the packet.

        If the DIRECTION is OUTGOING, this is the TIMEOUT HEIGHT
        of the COUNTERPARTY chain. If the COUNTERPARTY chain PROCESS the packet
        after it reaches the TIMEOUT HEIGHT, it REJECTs the packet.
     */
    pub timeout_height: Height,

    /*
        If the DIRECTION is INCOMING, and if the HOST chain PROCESS
        the packet with the local timestamp later than the given timeout timestamp,
        it REJECTs the packet.

        If the DIRECTION is OUTGOING, and if the COUNTERPARTY chain PROCESS
        the packet with the local timestamp later than the given timeout timestamp,
        it REJECTs the packet.
     */
    pub timeout_timestamp: Timestamp,
}

impl Packet {
    pub fn timed_out(&self, dst_chain_height: Height) -> bool {
        (self.timeout_height != Height::zero() && self.timeout_height < dst_chain_height)
            || (self.timeout_timestamp != Timestamp::none()
                && Timestamp::now().check_expiry(&self.timeout_timestamp) == Expired)
    }
}

impl std::fmt::Debug for Packet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(
            f,
            "{:?} {:?} {:?}",
            self.source_port, self.source_channel, self.sequence
        )
    }
}

/// Custom debug output to omit the packet data
impl std::fmt::Display for Packet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(
            f,
            "seq:{}, path:{}/{}->{}/{}, toh:{}, tos:{})",
            self.sequence,
            self.source_channel,
            self.source_port,
            self.destination_channel,
            self.destination_port,
            self.timeout_height,
            self.timeout_timestamp
        )
    }
}

impl Default for Packet {
    fn default() -> Self {
        Packet {
            sequence: Sequence(0),
            source_port: Default::default(),
            source_channel: Default::default(),
            destination_port: Default::default(),
            destination_channel: Default::default(),
            data: vec![],
            timeout_height: Default::default(),
            timeout_timestamp: Default::default(),
        }
    }
}

impl TryFrom<RawPacket> for Packet {
    type Error = Error;

    fn try_from(raw_pkt: RawPacket) -> Result<Self, Self::Error> {
        if Sequence::from(raw_pkt.sequence).is_zero() {
            return Err(Error::zero_packet_sequence());
        }
        let packet_timeout_height: Height = raw_pkt
            .timeout_height
            .ok_or_else(Error::missing_height)?
            .into();

        if packet_timeout_height.is_zero() && raw_pkt.timeout_timestamp == 0 {
            return Err(Error::zero_packet_timeout());
        }
        if raw_pkt.data.is_empty() {
            return Err(Error::zero_packet_data());
        }

        let timeout_timestamp = Timestamp::from_nanoseconds(raw_pkt.timeout_timestamp)
            .map_err(Error::invalid_packet_timestamp)?;

        Ok(Packet {
            sequence: Sequence::from(raw_pkt.sequence),
            source_port: raw_pkt.source_port.parse().map_err(Error::identifier)?,
            source_channel: raw_pkt.source_channel.parse().map_err(Error::identifier)?,
            destination_port: raw_pkt
                .destination_port
                .parse()
                .map_err(Error::identifier)?,
            destination_channel: raw_pkt
                .destination_channel
                .parse()
                .map_err(Error::identifier)?,
            data: raw_pkt.data,
            timeout_height: packet_timeout_height,
            timeout_timestamp,
        })
    }
}

impl From<Packet> for RawPacket {
    fn from(packet: Packet) -> Self {
        RawPacket {
            sequence: packet.sequence.0,
            source_port: packet.source_port.to_string(),
            source_channel: packet.source_channel.to_string(),
            destination_port: packet.destination_port.to_string(),
            destination_channel: packet.destination_channel.to_string(),
            data: packet.data,
            timeout_height: Some(packet.timeout_height.into()),
            timeout_timestamp: packet.timeout_timestamp.as_nanoseconds(),
        }
    }
}

#[cfg(test)]
pub mod test_utils {
    use ibc_proto::ibc::core::channel::v1::Packet as RawPacket;
    use ibc_proto::ibc::core::client::v1::Height as RawHeight;

    use crate::ics24_host::identifier::{ChannelId, PortId};

    /// Returns a dummy `RawPacket`, for testing only!
    pub fn get_dummy_raw_packet(timeout_height: u64, timeout_timestamp: u64) -> RawPacket {
        RawPacket {
            sequence: 1,
            source_port: PortId::default().to_string(),
            source_channel: ChannelId::default().to_string(),
            destination_port: PortId::default().to_string(),
            destination_channel: ChannelId::default().to_string(),
            data: vec![0],
            timeout_height: Some(RawHeight {
                revision_number: 0,
                revision_height: timeout_height,
            }),
            timeout_timestamp,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;
    use test_env_log::test;

    use ibc_proto::ibc::core::channel::v1::Packet as RawPacket;

    use crate::ics04_channel::packet::test_utils::get_dummy_raw_packet;
    use crate::ics04_channel::packet::Packet;

    #[test]
    fn packet_try_from_raw() {
        struct Test {
            name: String,
            raw: RawPacket,
            want_pass: bool,
        }

        let proof_height = 10;
        let default_raw_msg = get_dummy_raw_packet(proof_height, 0);

        let tests: Vec<Test> = vec![
            Test {
                name: "Good parameters".to_string(),
                raw: default_raw_msg.clone(),
                want_pass: true
            },
            Test {
                name: "Src port validation: correct".to_string(),
                raw: RawPacket {
                    source_port: "srcportp34".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: true,
            },
            Test {
                name: "Bad src port, name too short".to_string(),
                raw: RawPacket {
                    source_port: "p".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Bad src port, name too long".to_string(),
                raw: RawPacket {
                    source_port: "abcdefghijasdfasdfasdfasdfasdfasdfasdfasdfasdfasdfadgasgasdfasdfasdfasdfaklmnopqrstu".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Dst port validation: correct".to_string(),
                raw: RawPacket {
                    destination_port: "destportsrcp34".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: true,
            },
            Test {
                name: "Bad dst port, name too short".to_string(),
                raw: RawPacket {
                    destination_port: "p".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Bad dst port, name too long".to_string(),
                raw: RawPacket {
                    destination_port: "abcdefghijasdfasdfasdfasdfasdfasdfasdfasdfasdfasdfadgasgasdfasdfasdfasdfaklmnopqrstu".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Src channel validation: correct".to_string(),
                raw: RawPacket {
                    source_channel: "srcchannelp34".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: true,
            },
            Test {
                name: "Bad src channel, name too short".to_string(),
                raw: RawPacket {
                    source_channel: "p".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Bad src channel, name too long".to_string(),
                raw: RawPacket {
                    source_channel: "abcdefghijasdfasdfasdfasdfasdfasdfasdfasdfasdfasdfadgasgasdfasdfasdfasdfaklmnopqrstu".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Dst channel validation: correct".to_string(),
                raw: RawPacket {
                    destination_channel: "dstchannelp34".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: true,
            },
            Test {
                name: "Bad dst channel, name too short".to_string(),
                raw: RawPacket {
                    destination_channel: "p".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Bad dst channel, name too long".to_string(),
                raw: RawPacket {
                    destination_channel: "abcdefghijasdfasdfasdfasdfasdfasdfasdfasdfasdfasdfadgasgasdfasdfasdfasdfaklmnopqrstu".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Missing timeout height".to_string(),
                raw: RawPacket {
                    timeout_height: None,
                    ..default_raw_msg
                },
                want_pass: false,
            },
        ];

        for test in tests {
            let res_msg = Packet::try_from(test.raw.clone());

            assert_eq!(
                test.want_pass,
                res_msg.is_ok(),
                "Packet::try_from failed for test {}, \nraw packet {:?} with error {:?}",
                test.name,
                test.raw,
                res_msg.err(),
            );
        }
    }

    #[test]
    fn to_and_from() {
        let raw = get_dummy_raw_packet(15, 0);
        let msg = Packet::try_from(raw.clone()).unwrap();
        let raw_back = RawPacket::from(msg.clone());
        let msg_back = Packet::try_from(raw_back.clone()).unwrap();
        assert_eq!(raw, raw_back);
        assert_eq!(msg, msg_back);
    }
}
