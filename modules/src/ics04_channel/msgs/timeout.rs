use crate::prelude::*;
use core::convert::{TryFrom, TryInto};
use tendermint_proto::Protobuf;

use ibc_proto::ibc::core::channel::v1::MsgTimeout as RawMsgTimeout;

use crate::ics04_channel::error::Error;
use crate::ics04_channel::packet::{Packet, Sequence};
use crate::proofs::Proofs;
use crate::signer::Signer;
use crate::tx_msg::Msg;

pub const TYPE_URL: &str = "/ibc.core.channel.v1.MsgTimeout";

///
/// Message definition for packet timeout domain type.
///
#[derive(Clone, Debug, PartialEq)]
pub struct MsgTimeout {
    pub packet: Packet,
    pub next_sequence_recv: Sequence,
    pub proofs: Proofs,
    pub signer: Signer,
}

impl MsgTimeout {
    pub fn new(
        packet: Packet,
        next_sequence_recv: Sequence,
        proofs: Proofs,
        signer: Signer,
    ) -> MsgTimeout {
        Self {
            packet,
            next_sequence_recv,
            proofs,
            signer,
        }
    }
}

impl Msg for MsgTimeout {
    type ValidationError = Error;
    type Raw = RawMsgTimeout;

    fn route(&self) -> String {
        crate::keys::ROUTER_KEY.to_string()
    }

    fn type_url(&self) -> String {
        TYPE_URL.to_string()
    }
}

impl Protobuf<RawMsgTimeout> for MsgTimeout {}

impl TryFrom<RawMsgTimeout> for MsgTimeout {
    type Error = Error;

    fn try_from(raw_msg: RawMsgTimeout) -> Result<Self, Self::Error> {
        let proofs = Proofs::new(
            raw_msg.proof_unreceived.into(),
            None,
            None,
            None,
            raw_msg
                .proof_height
                .ok_or_else(Error::missing_height)?
                .into(),
        )
        .map_err(Error::invalid_proof)?;

        // TODO: Domain type verification for the next sequence: this should probably be > 0.

        Ok(MsgTimeout {
            packet: raw_msg
                .packet
                .ok_or_else(Error::missing_packet)?
                .try_into()?,
            next_sequence_recv: Sequence::from(raw_msg.next_sequence_recv),
            signer: raw_msg.signer.into(),
            proofs,
        })
    }
}

impl From<MsgTimeout> for RawMsgTimeout {
    fn from(domain_msg: MsgTimeout) -> Self {
        RawMsgTimeout {
            packet: Some(domain_msg.packet.into()),
            proof_unreceived: domain_msg.proofs.object_proof().clone().into(),
            proof_height: Some(domain_msg.proofs.height().into()),
            next_sequence_recv: domain_msg.next_sequence_recv.into(),
            signer: domain_msg.signer.to_string(),
        }
    }
}

#[cfg(test)]
pub mod test_util {
    use ibc_proto::ibc::core::channel::v1::MsgTimeout as RawMsgTimeout;
    use ibc_proto::ibc::core::client::v1::Height as RawHeight;

    use crate::ics04_channel::packet::test_utils::get_dummy_raw_packet;
    use crate::test_utils::{get_dummy_bech32_account, get_dummy_proof};

    /// Returns a dummy `RawMsgTimeout`, for testing only!
    /// The `height` parametrizes both the proof height as well as the timeout height.
    pub fn get_dummy_raw_msg_timeout(height: u64, timeout_timestamp: u64) -> RawMsgTimeout {
        RawMsgTimeout {
            packet: Some(get_dummy_raw_packet(height, timeout_timestamp)),
            proof_unreceived: get_dummy_proof(),
            proof_height: Some(RawHeight {
                revision_number: 0,
                revision_height: height,
            }),
            next_sequence_recv: 1,
            signer: get_dummy_bech32_account(),
        }
    }
}

#[cfg(test)]
mod test {
    use crate::prelude::*;
    use core::convert::{TryFrom, TryInto};
    use test_env_log::test;

    use ibc_proto::ibc::core::channel::v1::MsgTimeout as RawMsgTimeout;

    use crate::ics04_channel::error::Error;
    use crate::ics04_channel::msgs::timeout::test_util::get_dummy_raw_msg_timeout;
    use crate::ics04_channel::msgs::timeout::MsgTimeout;

    #[test]
    fn msg_timeout_try_from_raw() {
        struct Test {
            name: String,
            raw: RawMsgTimeout,
            want_pass: bool,
        }

        let height = 50;
        let timeout_timestamp = 0;
        let default_raw_msg = get_dummy_raw_msg_timeout(height, timeout_timestamp);

        let tests: Vec<Test> = vec![
            Test {
                name: "Good parameters".to_string(),
                raw: default_raw_msg.clone(),
                want_pass: true,
            },
            Test {
                name: "Missing packet".to_string(),
                raw: RawMsgTimeout {
                    packet: None,
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Missing proof".to_string(),
                raw: RawMsgTimeout {
                    proof_unreceived: Vec::new(),
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Missing proof height".to_string(),
                raw: RawMsgTimeout {
                    proof_height: None,
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Empty signer".to_string(),
                raw: RawMsgTimeout {
                    signer: "".to_string(),
                    ..default_raw_msg
                },
                want_pass: true,
            },
        ];

        for test in tests {
            let res_msg: Result<MsgTimeout, Error> = test.raw.clone().try_into();

            assert_eq!(
                res_msg.is_ok(),
                test.want_pass,
                "MsgTimeout::try_from failed for test {} \nraw message: {:?} with error: {:?}",
                test.name,
                test.raw,
                res_msg.err()
            );
        }
    }

    #[test]
    fn to_and_from() {
        let raw = get_dummy_raw_msg_timeout(15, 0);
        let msg = MsgTimeout::try_from(raw.clone()).unwrap();
        let raw_back = RawMsgTimeout::from(msg.clone());
        let msg_back = MsgTimeout::try_from(raw_back.clone()).unwrap();
        assert_eq!(raw, raw_back);
        assert_eq!(msg, msg_back);
    }
}
