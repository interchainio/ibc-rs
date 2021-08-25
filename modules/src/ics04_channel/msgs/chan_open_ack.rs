use crate::ics04_channel::channel::validate_version;
use crate::ics04_channel::error::Error;
use crate::ics24_host::identifier::{ChannelId, PortId};
use crate::prelude::*;
use crate::proofs::Proofs;
use crate::signer::Signer;
use crate::tx_msg::Msg;

use core::convert::TryFrom;

use ibc_proto::ibc::core::channel::v1::MsgChannelOpenAck as RawMsgChannelOpenAck;
use tendermint_proto::Protobuf;

pub const TYPE_URL: &str = "/ibc.core.channel.v1.MsgChannelOpenAck";

///
/// Message definition for the third step in the channel open handshake (`ChanOpenAck` datagram).
///
#[derive(Clone, Debug, PartialEq)]
pub struct MsgChannelOpenAck {
    pub port_id: PortId,
    pub channel_id: ChannelId,
    pub counterparty_channel_id: ChannelId,
    pub counterparty_version: String, // FIXME(romac): Introduce newtype for versions
    pub proofs: Proofs,
    pub signer: Signer,
}

impl MsgChannelOpenAck {
    pub fn new(
        port_id: PortId,
        channel_id: ChannelId,
        counterparty_channel_id: ChannelId,
        counterparty_version: String,
        proofs: Proofs,
        signer: Signer,
    ) -> Self {
        Self {
            port_id,
            channel_id,
            counterparty_channel_id,
            counterparty_version,
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

    pub fn counterparty_channel_id(&self) -> &ChannelId {
        &self.counterparty_channel_id
    }

    pub fn counterparty_version(&self) -> &String {
        &self.counterparty_version
    }

    pub fn proofs(&self) -> &Proofs {
        &self.proofs
    }
}

impl Msg for MsgChannelOpenAck {
    type ValidationError = Error;
    type Raw = RawMsgChannelOpenAck;

    fn route(&self) -> String {
        crate::keys::ROUTER_KEY.to_string()
    }

    fn type_url(&self) -> String {
        TYPE_URL.to_string()
    }
}

impl Protobuf<RawMsgChannelOpenAck> for MsgChannelOpenAck {}

impl TryFrom<RawMsgChannelOpenAck> for MsgChannelOpenAck {
    type Error = Error;

    fn try_from(raw_msg: RawMsgChannelOpenAck) -> Result<Self, Self::Error> {
        let proofs = Proofs::new(
            raw_msg.proof_try.into(),
            None,
            None,
            None,
            raw_msg
                .proof_height
                .ok_or_else(Error::missing_height)?
                .into(),
        )
        .map_err(Error::invalid_proof)?;

        Ok(MsgChannelOpenAck {
            port_id: raw_msg.port_id.parse().map_err(Error::identifier)?,
            channel_id: raw_msg.channel_id.parse().map_err(Error::identifier)?,
            counterparty_channel_id: raw_msg
                .counterparty_channel_id
                .parse()
                .map_err(Error::identifier)?,
            counterparty_version: validate_version(raw_msg.counterparty_version)?,
            proofs,
            signer: raw_msg.signer.into(),
        })
    }
}

impl From<MsgChannelOpenAck> for RawMsgChannelOpenAck {
    fn from(domain_msg: MsgChannelOpenAck) -> Self {
        RawMsgChannelOpenAck {
            port_id: domain_msg.port_id.to_string(),
            channel_id: domain_msg.channel_id.to_string(),
            counterparty_channel_id: domain_msg.counterparty_channel_id.to_string(),
            counterparty_version: domain_msg.counterparty_version.to_string(),
            proof_try: domain_msg.proofs.object_proof().clone().into(),
            proof_height: Some(domain_msg.proofs.height().into()),
            signer: domain_msg.signer.to_string(),
        }
    }
}

#[cfg(test)]
pub mod test_util {
    use crate::prelude::*;
    use ibc_proto::ibc::core::channel::v1::MsgChannelOpenAck as RawMsgChannelOpenAck;

    use crate::ics24_host::identifier::{ChannelId, PortId};
    use crate::test_utils::{get_dummy_bech32_account, get_dummy_proof};
    use ibc_proto::ibc::core::client::v1::Height;

    /// Returns a dummy `RawMsgChannelOpenAck`, for testing only!
    pub fn get_dummy_raw_msg_chan_open_ack(proof_height: u64) -> RawMsgChannelOpenAck {
        RawMsgChannelOpenAck {
            port_id: PortId::default().to_string(),
            channel_id: ChannelId::default().to_string(),
            counterparty_channel_id: ChannelId::default().to_string(),
            counterparty_version: "".to_string(),
            proof_try: get_dummy_proof(),
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
    use ibc_proto::ibc::core::channel::v1::MsgChannelOpenAck as RawMsgChannelOpenAck;
    use test_env_log::test;

    use crate::ics04_channel::msgs::chan_open_ack::test_util::get_dummy_raw_msg_chan_open_ack;
    use crate::ics04_channel::msgs::chan_open_ack::MsgChannelOpenAck;
    use core::convert::TryFrom;
    use ibc_proto::ibc::core::client::v1::Height;

    #[test]
    fn parse_channel_open_ack_msg() {
        struct Test {
            name: String,
            raw: RawMsgChannelOpenAck,
            want_pass: bool,
        }

        let proof_height = 20;
        let default_raw_msg = get_dummy_raw_msg_chan_open_ack(proof_height);

        let tests: Vec<Test> = vec![
            Test {
                name: "Good parameters".to_string(),
                raw: default_raw_msg.clone(),
                want_pass: true,
            },
            Test {
                name: "Correct port identifier".to_string(),
                raw: RawMsgChannelOpenAck {
                    port_id: "p34".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: true,
            },
            Test {
                name: "Bad port, name too short".to_string(),
                raw: RawMsgChannelOpenAck {
                    port_id: "p".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Bad port, name too long".to_string(),
                raw: RawMsgChannelOpenAck {
                    port_id: "abcdezdfDfsdfgfddsfsfdsdfdfvxcvzxcvsgdfsdfwefwvsdfdsfdasgagadgsadgsdffghijklmnopqrstu".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Correct channel identifier".to_string(),
                raw: RawMsgChannelOpenAck {
                    channel_id: "channelid34".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: true,
            },
            Test {
                name: "Bad channel, name too short".to_string(),
                raw: RawMsgChannelOpenAck {
                    channel_id: "chshort".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Bad channel, name too long".to_string(),
                raw: RawMsgChannelOpenAck {
                    channel_id: "abcdefghsdfasdfasfdasfdwewefsdfasdfasdfasdfasdfasdfsfdijklmnopqrstu".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "[Counterparty] Correct channel identifier".to_string(),
                raw: RawMsgChannelOpenAck {
                    counterparty_channel_id: "channelid34".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: true,
            },
            Test {
                name: "[Counterparty] Bad channel, name too short".to_string(),
                raw: RawMsgChannelOpenAck {
                    counterparty_channel_id: "chshort".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "[Counterparty] Bad channel, name too long".to_string(),
                raw: RawMsgChannelOpenAck {
                    counterparty_channel_id: "abcdefghsdfasdfasfdasfdwewefsdfasdfasdfasdfasdfasdfsfdijklmnopqrstu".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Empty counterparty version (allowed)".to_string(),
                raw: RawMsgChannelOpenAck {
                    counterparty_version: " ".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: true,
            },
            Test {
                name: "Arbitrary counterparty version (allowed)".to_string(),
                raw: RawMsgChannelOpenAck {
                    counterparty_version: "v1.1.23-alpha".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: true,
            },
            Test {
                name: "Bad proof height, height = 0".to_string(),
                raw: RawMsgChannelOpenAck {
                    proof_height: Some(Height {
                        revision_number: 0,
                        revision_height: 0,
                    }),
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Missing proof height".to_string(),
                raw: RawMsgChannelOpenAck {
                    proof_height: None,
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Missing proof try (object proof)".to_string(),
                raw: RawMsgChannelOpenAck {
                    proof_try: Vec::new(),
                    ..default_raw_msg
                },
                want_pass: false,
            },
        ]
            .into_iter()
            .collect();

        for test in tests {
            let res_msg = MsgChannelOpenAck::try_from(test.raw.clone());

            assert_eq!(
                test.want_pass,
                res_msg.is_ok(),
                "MsgChanOpenAck::try_from raw failed for test {}, \nraw msg {:?} with error {:?}",
                test.name,
                test.raw,
                res_msg.err(),
            );
        }
    }

    #[test]
    fn to_and_from() {
        let raw = get_dummy_raw_msg_chan_open_ack(100);
        let msg = MsgChannelOpenAck::try_from(raw.clone()).unwrap();
        let raw_back = RawMsgChannelOpenAck::from(msg.clone());
        let msg_back = MsgChannelOpenAck::try_from(raw_back.clone()).unwrap();
        assert_eq!(raw, raw_back);
        assert_eq!(msg, msg_back);
    }
}
