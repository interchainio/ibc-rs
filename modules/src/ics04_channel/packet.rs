use crate::prelude::*;
use core::convert::TryFrom;
use core::str::FromStr;

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

/// Enumeration of proof carrying ICS4 message, helper for relayer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PacketMsgType {
    Recv,
    Ack,
    TimeoutUnordered,
    TimeoutOrdered,
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

impl core::fmt::Display for PacketMsgType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
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

impl core::fmt::Display for Sequence {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> Result<(), core::fmt::Error> {
        write!(f, "{}", self.0)
    }
}

#[derive(PartialEq, Deserialize, Serialize, Hash, Clone)]
pub struct Packet {
    pub sequence: Sequence,
    pub source_port: PortId,
    pub source_channel: ChannelId,
    pub destination_port: PortId,
    pub destination_channel: ChannelId,
    #[serde(serialize_with = "crate::serializers::ser_hex_upper")]
    pub data: Vec<u8>,
    pub timeout_height: Height,
    pub timeout_timestamp: Timestamp,
}

impl Packet {
    pub fn timed_out(&self, dst_chain_height: Height) -> bool {
        (self.timeout_height != Height::zero() && self.timeout_height < dst_chain_height)
            || (self.timeout_timestamp != Timestamp::none()
                && Timestamp::now().check_expiry(&self.timeout_timestamp) == Expired)
    }
}

impl core::fmt::Debug for Packet {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> Result<(), core::fmt::Error> {
        write!(
            f,
            "{:?} {:?} {:?}",
            self.source_port, self.source_channel, self.sequence
        )
    }
}

/// Custom debug output to omit the packet data
impl core::fmt::Display for Packet {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> Result<(), core::fmt::Error> {
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
            data: Vec::new(),
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
    use crate::prelude::*;
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
    use crate::prelude::*;
    use core::convert::TryFrom;
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
