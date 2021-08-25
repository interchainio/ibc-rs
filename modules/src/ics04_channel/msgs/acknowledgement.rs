use crate::prelude::*;
use core::convert::{TryFrom, TryInto};

use tendermint_proto::Protobuf;

use ibc_proto::ibc::core::channel::v1::MsgAcknowledgement as RawMsgAcknowledgement;

use crate::ics04_channel::error::Error;
use crate::ics04_channel::packet::Packet;
use crate::proofs::Proofs;
use crate::signer::Signer;
use crate::tx_msg::Msg;

pub const TYPE_URL: &str = "/ibc.core.channel.v1.MsgAcknowledgement";

///
/// Message definition for packet acknowledgements.
///
#[derive(Clone, Debug, PartialEq)]
pub struct MsgAcknowledgement {
    pub packet: Packet,
    pub acknowledgement: Vec<u8>, // TODO(romac): Introduce a newtype for this
    pub proofs: Proofs,
    pub signer: Signer,
}

impl MsgAcknowledgement {
    pub fn new(
        packet: Packet,
        acknowledgement: Vec<u8>,
        proofs: Proofs,
        signer: Signer,
    ) -> MsgAcknowledgement {
        Self {
            packet,
            acknowledgement,
            proofs,
            signer,
        }
    }

    pub fn acknowledgement(&self) -> &Vec<u8> {
        &self.acknowledgement
    }

    pub fn proofs(&self) -> &Proofs {
        &self.proofs
    }
}

impl Msg for MsgAcknowledgement {
    type ValidationError = Error;
    type Raw = RawMsgAcknowledgement;

    fn route(&self) -> String {
        crate::keys::ROUTER_KEY.to_string()
    }

    fn type_url(&self) -> String {
        TYPE_URL.to_string()
    }
}

impl Protobuf<RawMsgAcknowledgement> for MsgAcknowledgement {}

impl TryFrom<RawMsgAcknowledgement> for MsgAcknowledgement {
    type Error = Error;

    fn try_from(raw_msg: RawMsgAcknowledgement) -> Result<Self, Self::Error> {
        let proofs = Proofs::new(
            raw_msg.proof_acked.into(),
            None,
            None,
            None,
            raw_msg
                .proof_height
                .ok_or_else(Error::missing_height)?
                .into(),
        )
        .map_err(Error::invalid_proof)?;

        Ok(MsgAcknowledgement {
            packet: raw_msg
                .packet
                .ok_or_else(Error::missing_packet)?
                .try_into()?,
            acknowledgement: raw_msg.acknowledgement,
            signer: raw_msg.signer.into(),
            proofs,
        })
    }
}

impl From<MsgAcknowledgement> for RawMsgAcknowledgement {
    fn from(domain_msg: MsgAcknowledgement) -> Self {
        RawMsgAcknowledgement {
            packet: Some(domain_msg.packet.into()),
            acknowledgement: domain_msg.acknowledgement,
            signer: domain_msg.signer.to_string(),
            proof_height: Some(domain_msg.proofs.height().into()),
            proof_acked: domain_msg.proofs.object_proof().clone().into(),
        }
    }
}

#[cfg(test)]
pub mod test_util {
    use ibc_proto::ibc::core::channel::v1::MsgAcknowledgement as RawMsgAcknowledgement;
    use ibc_proto::ibc::core::client::v1::Height as RawHeight;

    use crate::ics04_channel::packet::test_utils::get_dummy_raw_packet;
    use crate::test_utils::{get_dummy_bech32_account, get_dummy_proof};

    /// Returns a dummy `RawMsgAcknowledgement`, for testing only!
    /// The `height` parametrizes both the proof height as well as the timeout height.
    pub fn get_dummy_raw_msg_acknowledgement(height: u64) -> RawMsgAcknowledgement {
        RawMsgAcknowledgement {
            packet: Some(get_dummy_raw_packet(height, 1)),
            acknowledgement: get_dummy_proof(),
            proof_acked: get_dummy_proof(),
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
    use core::convert::TryInto;
    use test_env_log::test;

    use ibc_proto::ibc::core::channel::v1::MsgAcknowledgement as RawMsgAcknowledgement;

    use crate::ics04_channel::error::Error;
    use crate::ics04_channel::msgs::acknowledgement::test_util::get_dummy_raw_msg_acknowledgement;
    use crate::ics04_channel::msgs::acknowledgement::MsgAcknowledgement;

    #[test]
    fn msg_acknowledgment_try_from_raw() {
        struct Test {
            name: String,
            raw: RawMsgAcknowledgement,
            want_pass: bool,
        }

        let height = 50;
        let default_raw_msg = get_dummy_raw_msg_acknowledgement(height);

        let tests: Vec<Test> = vec![
            Test {
                name: "Good parameters".to_string(),
                raw: default_raw_msg.clone(),
                want_pass: true,
            },
            Test {
                name: "Missing packet".to_string(),
                raw: RawMsgAcknowledgement {
                    packet: None,
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Missing proof height".to_string(),
                raw: RawMsgAcknowledgement {
                    proof_height: None,
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Empty signer".to_string(),
                raw: RawMsgAcknowledgement {
                    signer: "".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: true,
            },
            Test {
                name: "Empty proof acked".to_string(),
                raw: RawMsgAcknowledgement {
                    proof_acked: Vec::new(),
                    ..default_raw_msg
                },
                want_pass: false,
            },
        ];

        for test in tests {
            let res_msg: Result<MsgAcknowledgement, Error> = test.raw.clone().try_into();

            assert_eq!(
                res_msg.is_ok(),
                test.want_pass,
                "MsgAcknowledgement::try_from failed for test {} \nraw message: {:?} with error: {:?}",
                test.name,
                test.raw,
                res_msg.err()
            );
        }
    }
}
