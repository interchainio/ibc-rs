use crate::ics04_channel::error::Error;
use crate::ics24_host::identifier::{ChannelId, PortId};
use crate::prelude::*;
use crate::proofs::Proofs;
use crate::signer::Signer;
use crate::tx_msg::Msg;

use core::convert::TryFrom;
use ibc_proto::ibc::core::channel::v1::MsgChannelOpenConfirm as RawMsgChannelOpenConfirm;
use tendermint_proto::Protobuf;

pub const TYPE_URL: &str = "/ibc.core.channel.v1.MsgChannelOpenConfirm";

///
/// Message definition for the fourth step in the channel open handshake (`ChanOpenConfirm`
/// datagram).
///
#[derive(Clone, Debug, PartialEq)]
pub struct MsgChannelOpenConfirm {
    pub port_id: PortId,
    pub channel_id: ChannelId,
    pub proofs: Proofs,
    pub signer: Signer,
}

impl MsgChannelOpenConfirm {
    pub fn new(port_id: PortId, channel_id: ChannelId, proofs: Proofs, signer: Signer) -> Self {
        Self {
            port_id,
            channel_id,
            proofs,
            signer,
        }
    }
}

impl MsgChannelOpenConfirm {
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

impl Msg for MsgChannelOpenConfirm {
    type ValidationError = Error;
    type Raw = RawMsgChannelOpenConfirm;

    fn route(&self) -> String {
        crate::keys::ROUTER_KEY.to_string()
    }

    fn type_url(&self) -> String {
        TYPE_URL.to_string()
    }
}

impl Protobuf<RawMsgChannelOpenConfirm> for MsgChannelOpenConfirm {}

impl TryFrom<RawMsgChannelOpenConfirm> for MsgChannelOpenConfirm {
    type Error = Error;

    fn try_from(raw_msg: RawMsgChannelOpenConfirm) -> Result<Self, Self::Error> {
        let proofs = Proofs::new(
            raw_msg.proof_ack.into(),
            None,
            None,
            None,
            raw_msg
                .proof_height
                .ok_or_else(Error::missing_height)?
                .into(),
        )
        .map_err(Error::invalid_proof)?;

        Ok(MsgChannelOpenConfirm {
            port_id: raw_msg.port_id.parse().map_err(Error::identifier)?,
            channel_id: raw_msg.channel_id.parse().map_err(Error::identifier)?,
            proofs,
            signer: raw_msg.signer.into(),
        })
    }
}

impl From<MsgChannelOpenConfirm> for RawMsgChannelOpenConfirm {
    fn from(domain_msg: MsgChannelOpenConfirm) -> Self {
        RawMsgChannelOpenConfirm {
            port_id: domain_msg.port_id.to_string(),
            channel_id: domain_msg.channel_id.to_string(),
            proof_ack: domain_msg.proofs.object_proof().clone().into(),
            proof_height: Some(domain_msg.proofs.height().into()),
            signer: domain_msg.signer.to_string(),
        }
    }
}

#[cfg(test)]
pub mod test_util {
    use crate::prelude::*;
    use ibc_proto::ibc::core::channel::v1::MsgChannelOpenConfirm as RawMsgChannelOpenConfirm;

    use crate::ics24_host::identifier::{ChannelId, PortId};
    use crate::test_utils::{get_dummy_bech32_account, get_dummy_proof};
    use ibc_proto::ibc::core::client::v1::Height;

    /// Returns a dummy `RawMsgChannelOpenConfirm`, for testing only!
    pub fn get_dummy_raw_msg_chan_open_confirm(proof_height: u64) -> RawMsgChannelOpenConfirm {
        RawMsgChannelOpenConfirm {
            port_id: PortId::default().to_string(),
            channel_id: ChannelId::default().to_string(),
            proof_ack: get_dummy_proof(),
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
    use ibc_proto::ibc::core::channel::v1::MsgChannelOpenConfirm as RawMsgChannelOpenConfirm;
    use test_env_log::test;

    use crate::ics04_channel::msgs::chan_open_confirm::test_util::get_dummy_raw_msg_chan_open_confirm;
    use crate::ics04_channel::msgs::chan_open_confirm::MsgChannelOpenConfirm;
    use core::convert::TryFrom;
    use ibc_proto::ibc::core::client::v1::Height;

    #[test]
    fn parse_channel_open_confirm_msg() {
        struct Test {
            name: String,
            raw: RawMsgChannelOpenConfirm,
            want_pass: bool,
        }

        let proof_height = 78;
        let default_raw_msg = get_dummy_raw_msg_chan_open_confirm(proof_height);

        let tests: Vec<Test> = vec![
            Test {
                name: "Good parameters".to_string(),
                raw: default_raw_msg.clone(),
                want_pass: true,
            },
            Test {
                name: "Correct port".to_string(),
                raw: RawMsgChannelOpenConfirm {
                    port_id: "p34".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: true,
            },
            Test {
                name: "Bad port, name too short".to_string(),
                raw: RawMsgChannelOpenConfirm {
                    port_id: "p".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Bad port, name too long".to_string(),
                raw: RawMsgChannelOpenConfirm {
                    port_id: "abcdesdfasdsdffasdfasdfasfasdgasdfgasdfasdfasdfasdfasdfasdffghijklmnopqrstu".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Correct channel identifier".to_string(),
                raw: RawMsgChannelOpenConfirm {
                    channel_id: "channelid34".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: true,
            },
            Test {
                name: "Bad channel, name too short".to_string(),
                raw: RawMsgChannelOpenConfirm {
                    channel_id: "chshort".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Bad channel, name too long".to_string(),
                raw: RawMsgChannelOpenConfirm {
                    channel_id: "abcdefghijklmnoasdfasdfasdfasdfasdgsdghasdfasdfasdfasdfadsfasgdasdfasdfasfdpqrstu".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Bad proof height, height = 0".to_string(),
                raw: RawMsgChannelOpenConfirm {
                    proof_height: Some(Height {
                        revision_number: 0,
                        revision_height: 0,
                    }),
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Missing object proof".to_string(),
                raw: RawMsgChannelOpenConfirm {
                    proof_ack: Vec::new(),
                    ..default_raw_msg
                },
                want_pass: false,
            },
        ]
            .into_iter()
            .collect();

        for test in tests {
            let res_msg = MsgChannelOpenConfirm::try_from(test.raw.clone());

            assert_eq!(
                test.want_pass,
                res_msg.is_ok(),
                "MsgChanOpenConfirm::try_from failed for test {}, \nraw msg {:?} with error {:?}",
                test.name,
                test.raw,
                res_msg.err(),
            );
        }
    }

    #[test]
    fn to_and_from() {
        let raw = get_dummy_raw_msg_chan_open_confirm(19);
        let msg = MsgChannelOpenConfirm::try_from(raw.clone()).unwrap();
        let raw_back = RawMsgChannelOpenConfirm::from(msg.clone());
        let msg_back = MsgChannelOpenConfirm::try_from(raw_back.clone()).unwrap();
        assert_eq!(raw, raw_back);
        assert_eq!(msg, msg_back);
    }
}
