use crate::prelude::*;
use core::convert::TryFrom;

use tendermint_proto::Protobuf;

use ibc_proto::ibc::core::channel::v1::MsgChannelCloseConfirm as RawMsgChannelCloseConfirm;

use crate::ics04_channel::error::Error;
use crate::ics24_host::identifier::{ChannelId, PortId};
use crate::proofs::Proofs;
use crate::signer::Signer;
use crate::tx_msg::Msg;

pub const TYPE_URL: &str = "/ibc.core.channel.v1.MsgChannelCloseConfirm";

///
/// Message definition for the second step in the channel close handshake (the `ChanCloseConfirm`
/// datagram).
///
#[derive(Clone, Debug, PartialEq)]
pub struct MsgChannelCloseConfirm {
    pub port_id: PortId,
    pub channel_id: ChannelId,
    pub proofs: Proofs,
    pub signer: Signer,
}

impl MsgChannelCloseConfirm {
    pub fn new(port_id: PortId, channel_id: ChannelId, proofs: Proofs, signer: Signer) -> Self {
        Self {
            port_id,
            channel_id,
            proofs,
            signer,
        }
    }

    /// Getter: borrow the `port_id` from this message.
    pub fn port_id(&self) -> &PortId {
        &self.port_id
    }
    pub fn channel_id(&self) -> &ChannelId {
        &self.channel_id
    }
    pub fn proofs(&self) -> &Proofs {
        &self.proofs
    }
}

impl Msg for MsgChannelCloseConfirm {
    type ValidationError = Error;
    type Raw = RawMsgChannelCloseConfirm;

    fn route(&self) -> String {
        crate::keys::ROUTER_KEY.to_string()
    }

    fn type_url(&self) -> String {
        TYPE_URL.to_string()
    }
}

impl Protobuf<RawMsgChannelCloseConfirm> for MsgChannelCloseConfirm {}

impl TryFrom<RawMsgChannelCloseConfirm> for MsgChannelCloseConfirm {
    type Error = Error;

    fn try_from(raw_msg: RawMsgChannelCloseConfirm) -> Result<Self, Self::Error> {
        let proofs = Proofs::new(
            raw_msg.proof_init.into(),
            None,
            None,
            None,
            raw_msg
                .proof_height
                .ok_or_else(Error::missing_height)?
                .into(),
        )
        .map_err(Error::invalid_proof)?;

        Ok(MsgChannelCloseConfirm {
            port_id: raw_msg.port_id.parse().map_err(Error::identifier)?,
            channel_id: raw_msg.channel_id.parse().map_err(Error::identifier)?,
            proofs,
            signer: raw_msg.signer.into(),
        })
    }
}

impl From<MsgChannelCloseConfirm> for RawMsgChannelCloseConfirm {
    fn from(domain_msg: MsgChannelCloseConfirm) -> Self {
        RawMsgChannelCloseConfirm {
            port_id: domain_msg.port_id.to_string(),
            channel_id: domain_msg.channel_id.to_string(),
            proof_init: domain_msg.proofs.object_proof().clone().into(),
            proof_height: Some(domain_msg.proofs.height().into()),
            signer: domain_msg.signer.to_string(),
        }
    }
}

#[cfg(test)]
pub mod test_util {
    use crate::prelude::*;
    use ibc_proto::ibc::core::channel::v1::MsgChannelCloseConfirm as RawMsgChannelCloseConfirm;
    use ibc_proto::ibc::core::client::v1::Height;

    use crate::ics24_host::identifier::{ChannelId, PortId};
    use crate::test_utils::{get_dummy_bech32_account, get_dummy_proof};

    /// Returns a dummy `RawMsgChannelCloseConfirm`, for testing only!
    pub fn get_dummy_raw_msg_chan_close_confirm(proof_height: u64) -> RawMsgChannelCloseConfirm {
        RawMsgChannelCloseConfirm {
            port_id: PortId::default().to_string(),
            channel_id: ChannelId::default().to_string(),
            proof_init: get_dummy_proof(),
            proof_height: Some(Height {
                revision_number: 0,
                revision_height: proof_height,
            }),
            signer: get_dummy_bech32_account(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::prelude::*;
    use core::convert::TryFrom;

    use ibc_proto::ibc::core::channel::v1::MsgChannelCloseConfirm as RawMsgChannelCloseConfirm;
    use ibc_proto::ibc::core::client::v1::Height;

    use crate::ics04_channel::msgs::chan_close_confirm::test_util::get_dummy_raw_msg_chan_close_confirm;
    use crate::ics04_channel::msgs::chan_close_confirm::MsgChannelCloseConfirm;

    #[test]
    fn parse_channel_close_confirm_msg() {
        struct Test {
            name: String,
            raw: RawMsgChannelCloseConfirm,
            want_pass: bool,
        }

        let proof_height = 10;
        let default_raw_msg = get_dummy_raw_msg_chan_close_confirm(proof_height);

        let tests: Vec<Test> = vec![
            Test {
                name: "Good parameters".to_string(),
                raw: default_raw_msg.clone(),
                want_pass: true,
            },
            Test {
                name: "Correct port".to_string(),
                raw: RawMsgChannelCloseConfirm {
                    port_id: "p34".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: true,
            },
            Test {
                name: "Bad port, name too short".to_string(),
                raw: RawMsgChannelCloseConfirm {
                    port_id: "p".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Bad port, name too long".to_string(),
                raw: RawMsgChannelCloseConfirm {
                    port_id:
                        "abcdefghijklmnsdfasdfasdfasdfasdgafgadsfasdfasdfasdasfdasdfsadfopqrstu"
                            .to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Correct channel identifier".to_string(),
                raw: RawMsgChannelCloseConfirm {
                    channel_id: "channelid34".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: true,
            },
            Test {
                name: "Bad channel, name too short".to_string(),
                raw: RawMsgChannelCloseConfirm {
                    channel_id: "chshort".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Bad channel, name too long".to_string(),
                raw: RawMsgChannelCloseConfirm {
                    channel_id:
                        "abcdefghiasdfadsfasdfgdfsadfasdasdfasdasdfasddsfasdfasdjklmnopqrstu"
                            .to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Bad proof height, height = 0".to_string(),
                raw: RawMsgChannelCloseConfirm {
                    proof_height: Some(Height {
                        revision_number: 0,
                        revision_height: 0,
                    }),
                    ..default_raw_msg
                },
                want_pass: false,
            },
        ]
        .into_iter()
        .collect();

        for test in tests {
            let msg = MsgChannelCloseConfirm::try_from(test.raw.clone());

            assert_eq!(
                test.want_pass,
                msg.is_ok(),
                "MsgChanCloseConfirm::try_from raw failed for test {}, \nraw msg {:?} with error {:?}",
                test.name,
                test.raw,
                msg.err(),
            );
        }
    }

    #[test]
    fn to_and_from() {
        let raw = get_dummy_raw_msg_chan_close_confirm(19);
        let msg = MsgChannelCloseConfirm::try_from(raw.clone()).unwrap();
        let raw_back = RawMsgChannelCloseConfirm::from(msg.clone());
        let msg_back = MsgChannelCloseConfirm::try_from(raw_back.clone()).unwrap();
        assert_eq!(raw, raw_back);
        assert_eq!(msg, msg_back);
    }
}
