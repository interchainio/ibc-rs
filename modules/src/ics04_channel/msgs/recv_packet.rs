use crate::prelude::*;
use core::convert::{TryFrom, TryInto};
use tendermint_proto::Protobuf;

use ibc_proto::ibc::core::channel::v1::MsgRecvPacket as RawMsgRecvPacket;

use crate::ics04_channel::error::Error;
use crate::ics04_channel::packet::Packet;
use crate::proofs::Proofs;
use crate::signer::Signer;
use crate::tx_msg::Msg;

pub const TYPE_URL: &str = "/ibc.core.channel.v1.MsgRecvPacket";

///
/// Message definition for the "packet receiving" datagram.
///
#[derive(Clone, Debug, PartialEq)]
pub struct MsgRecvPacket {
    pub packet: Packet,
    pub proofs: Proofs,
    pub signer: Signer,
}

impl MsgRecvPacket {
    pub fn new(packet: Packet, proofs: Proofs, signer: Signer) -> MsgRecvPacket {
        Self {
            packet,
            proofs,
            signer,
        }
    }
}

impl Msg for MsgRecvPacket {
    type ValidationError = Error;
    type Raw = RawMsgRecvPacket;

    fn route(&self) -> String {
        crate::keys::ROUTER_KEY.to_string()
    }

    fn type_url(&self) -> String {
        TYPE_URL.to_string()
    }
}

impl Protobuf<RawMsgRecvPacket> for MsgRecvPacket {}

impl TryFrom<RawMsgRecvPacket> for MsgRecvPacket {
    type Error = Error;

    fn try_from(raw_msg: RawMsgRecvPacket) -> Result<Self, Self::Error> {
        let proofs = Proofs::new(
            raw_msg.proof_commitment.into(),
            None,
            None,
            None,
            raw_msg
                .proof_height
                .ok_or_else(Error::missing_height)?
                .into(),
        )
        .map_err(Error::invalid_proof)?;

        Ok(MsgRecvPacket {
            packet: raw_msg
                .packet
                .ok_or_else(Error::missing_packet)?
                .try_into()?,
            proofs,
            signer: raw_msg.signer.into(),
        })
    }
}

impl From<MsgRecvPacket> for RawMsgRecvPacket {
    fn from(domain_msg: MsgRecvPacket) -> Self {
        RawMsgRecvPacket {
            packet: Some(domain_msg.packet.into()),
            proof_commitment: domain_msg.proofs.object_proof().clone().into(),
            proof_height: Some(domain_msg.proofs.height().into()),
            signer: domain_msg.signer.to_string(),
        }
    }
}

#[cfg(test)]
pub mod test_util {
    use ibc_proto::ibc::core::channel::v1::MsgRecvPacket as RawMsgRecvPacket;
    use ibc_proto::ibc::core::client::v1::Height as RawHeight;

    use crate::ics04_channel::packet::test_utils::get_dummy_raw_packet;
    use crate::test_utils::{get_dummy_bech32_account, get_dummy_proof};

    /// Returns a dummy `RawMsgRecvPacket`, for testing only! The `height` parametrizes both the
    /// proof height as well as the timeout height.
    pub fn get_dummy_raw_msg_recv_packet(height: u64) -> RawMsgRecvPacket {
        RawMsgRecvPacket {
            packet: Some(get_dummy_raw_packet(height, 9)),
            proof_commitment: get_dummy_proof(),
            proof_height: Some(RawHeight {
                revision_number: 0,
                revision_height: height,
            }),
            signer: get_dummy_bech32_account(),
        }
    }
}

#[cfg(test)]
mod test {
    use crate::prelude::*;
    use core::convert::{TryFrom, TryInto};
    use test_env_log::test;

    use ibc_proto::ibc::core::channel::v1::MsgRecvPacket as RawMsgRecvPacket;

    use crate::ics04_channel::error::Error;
    use crate::ics04_channel::msgs::recv_packet::test_util::get_dummy_raw_msg_recv_packet;
    use crate::ics04_channel::msgs::recv_packet::MsgRecvPacket;

    #[test]
    fn msg_recv_packet_try_from_raw() {
        struct Test {
            name: String,
            raw: RawMsgRecvPacket,
            want_pass: bool,
        }

        let height = 20;
        let default_raw_msg = get_dummy_raw_msg_recv_packet(height);
        let tests: Vec<Test> = vec![
            Test {
                name: "Good parameters".to_string(),
                raw: default_raw_msg.clone(),
                want_pass: true,
            },
            Test {
                name: "Missing proof".to_string(),
                raw: RawMsgRecvPacket {
                    proof_commitment: Vec::new(),
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Missing proof height".to_string(),
                raw: RawMsgRecvPacket {
                    proof_height: None,
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Empty signer".to_string(),
                raw: RawMsgRecvPacket {
                    signer: "".to_string(),
                    ..default_raw_msg
                },
                want_pass: true,
            },
        ];

        for test in tests {
            let res_msg: Result<MsgRecvPacket, Error> = test.raw.clone().try_into();

            assert_eq!(
                res_msg.is_ok(),
                test.want_pass,
                "MsgRecvPacket::try_from failed for test {} \nraw message: {:?} with error: {:?}",
                test.name,
                test.raw,
                res_msg.err()
            );
        }
    }

    #[test]
    fn to_and_from() {
        let raw = get_dummy_raw_msg_recv_packet(15);
        let msg = MsgRecvPacket::try_from(raw.clone()).unwrap();
        let raw_back = RawMsgRecvPacket::from(msg.clone());
        let msg_back = MsgRecvPacket::try_from(raw_back.clone()).unwrap();
        assert_eq!(raw, raw_back);
        assert_eq!(msg, msg_back);
    }
}
