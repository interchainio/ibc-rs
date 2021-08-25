use crate::ics04_channel::channel::{validate_version, ChannelEnd};
use crate::ics04_channel::error::Error as ChannelError;
use crate::ics24_host::error::ValidationError;
use crate::ics24_host::identifier::{ChannelId, PortId};
use crate::prelude::*;
use crate::proofs::Proofs;
use crate::signer::Signer;
use crate::tx_msg::Msg;

use ibc_proto::ibc::core::channel::v1::MsgChannelOpenTry as RawMsgChannelOpenTry;
use tendermint_proto::Protobuf;

use core::convert::{TryFrom, TryInto};
use core::str::FromStr;

pub const TYPE_URL: &str = "/ibc.core.channel.v1.MsgChannelOpenTry";

///
/// Message definition for the second step in the channel open handshake (`ChanOpenTry` datagram).
///
#[derive(Clone, Debug, PartialEq)]
pub struct MsgChannelOpenTry {
    pub port_id: PortId,
    pub previous_channel_id: Option<ChannelId>,
    pub channel: ChannelEnd,
    pub counterparty_version: String, // TODO(romac): newtype this
    pub proofs: Proofs,
    pub signer: Signer,
}

impl MsgChannelOpenTry {
    pub fn new(
        port_id: PortId,
        previous_channel_id: Option<ChannelId>,
        channel: ChannelEnd,
        counterparty_version: String,
        proofs: Proofs,
        signer: Signer,
    ) -> Self {
        Self {
            port_id,
            previous_channel_id,
            channel,
            counterparty_version,
            proofs,
            signer,
        }
    }

    /// Getter: borrow the `port_id` from this message.
    pub fn port_id(&self) -> &PortId {
        &self.port_id
    }
    pub fn previous_channel_id(&self) -> &Option<ChannelId> {
        &self.previous_channel_id
    }
    pub fn counterparty_version(&self) -> &String {
        &self.counterparty_version
    }
    pub fn channel(&self) -> &ChannelEnd {
        &self.channel
    }
    pub fn proofs(&self) -> &Proofs {
        &self.proofs
    }
}
impl Msg for MsgChannelOpenTry {
    type ValidationError = ChannelError;
    type Raw = RawMsgChannelOpenTry;

    fn route(&self) -> String {
        crate::keys::ROUTER_KEY.to_string()
    }

    fn type_url(&self) -> String {
        TYPE_URL.to_string()
    }

    fn validate_basic(&self) -> Result<(), ValidationError> {
        match self.channel().counterparty().channel_id() {
            None => Err(ValidationError::invalid_counterparty_channel_id()),
            Some(_c) => Ok(()),
        }
    }
}

impl Protobuf<RawMsgChannelOpenTry> for MsgChannelOpenTry {}

impl TryFrom<RawMsgChannelOpenTry> for MsgChannelOpenTry {
    type Error = ChannelError;

    fn try_from(raw_msg: RawMsgChannelOpenTry) -> Result<Self, Self::Error> {
        let proofs = Proofs::new(
            raw_msg.proof_init.into(),
            None,
            None,
            None,
            raw_msg
                .proof_height
                .ok_or_else(ChannelError::missing_height)?
                .into(),
        )
        .map_err(ChannelError::invalid_proof)?;

        let previous_channel_id = Some(raw_msg.previous_channel_id)
            .filter(|x| !x.is_empty())
            .map(|v| FromStr::from_str(v.as_str()))
            .transpose()
            .map_err(ChannelError::identifier)?;

        let msg = MsgChannelOpenTry {
            port_id: raw_msg.port_id.parse().map_err(ChannelError::identifier)?,
            previous_channel_id,
            channel: raw_msg
                .channel
                .ok_or_else(ChannelError::missing_channel)?
                .try_into()?,
            counterparty_version: validate_version(raw_msg.counterparty_version)?,
            proofs,
            signer: raw_msg.signer.into(),
        };

        msg.validate_basic()
            .map_err(ChannelError::invalid_counterparty_channel_id)?;

        Ok(msg)
    }
}

impl From<MsgChannelOpenTry> for RawMsgChannelOpenTry {
    fn from(domain_msg: MsgChannelOpenTry) -> Self {
        RawMsgChannelOpenTry {
            port_id: domain_msg.port_id.to_string(),
            previous_channel_id: domain_msg
                .previous_channel_id
                .map_or_else(|| "".to_string(), |v| v.as_str().to_string()),
            channel: Some(domain_msg.channel.into()),
            counterparty_version: domain_msg.counterparty_version,
            proof_init: domain_msg.proofs.object_proof().clone().into(),
            proof_height: Some(domain_msg.proofs.height().into()),
            signer: domain_msg.signer.to_string(),
        }
    }
}

#[cfg(test)]
pub mod test_util {
    use crate::prelude::*;
    use ibc_proto::ibc::core::channel::v1::MsgChannelOpenTry as RawMsgChannelOpenTry;

    use crate::ics04_channel::channel::test_util::get_dummy_raw_channel_end;
    use crate::ics24_host::identifier::{ChannelId, PortId};
    use crate::test_utils::{get_dummy_bech32_account, get_dummy_proof};
    use ibc_proto::ibc::core::client::v1::Height;

    /// Returns a dummy `RawMsgChannelOpenTry`, for testing only!
    pub fn get_dummy_raw_msg_chan_open_try(proof_height: u64) -> RawMsgChannelOpenTry {
        RawMsgChannelOpenTry {
            port_id: PortId::default().to_string(),
            previous_channel_id: ChannelId::default().to_string(),
            channel: Some(get_dummy_raw_channel_end()),
            counterparty_version: "".to_string(),
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
    use crate::ics04_channel::msgs::chan_open_try::test_util::get_dummy_raw_msg_chan_open_try;
    use crate::ics04_channel::msgs::chan_open_try::MsgChannelOpenTry;
    use crate::prelude::*;
    use core::convert::TryFrom;
    use ibc_proto::ibc::core::channel::v1::MsgChannelOpenTry as RawMsgChannelOpenTry;
    use ibc_proto::ibc::core::client::v1::Height;
    use test_env_log::test;

    #[test]
    fn channel_open_try_from_raw() {
        struct Test {
            name: String,
            raw: RawMsgChannelOpenTry,
            want_pass: bool,
        }

        let proof_height = 10;
        let default_raw_msg = get_dummy_raw_msg_chan_open_try(proof_height);

        let tests: Vec<Test> = vec![
            Test {
                name: "Good parameters".to_string(),
                raw: default_raw_msg.clone(),
                want_pass: true,
            },
            Test {
                name: "Correct port".to_string(),
                raw: RawMsgChannelOpenTry {
                    port_id: "p34".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: true,
            },
            Test {
                name: "Bad port, name too short".to_string(),
                raw: RawMsgChannelOpenTry {
                    port_id: "p".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Bad port, name too long".to_string(),
                raw: RawMsgChannelOpenTry {
                    port_id: "abcdefghijasdfasdfasdfasdfasdfasdfasdfasdfasdfasdfadgasgasdfasdfasdfasdfaklmnopqrstu".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Correct channel identifier".to_string(),
                raw: RawMsgChannelOpenTry {
                    previous_channel_id: "channelid34".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: true,
            },
            Test {
                name: "Bad channel, name too short".to_string(),
                raw: RawMsgChannelOpenTry {
                    previous_channel_id: "chshort".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Bad channel, name too long".to_string(),
                raw: RawMsgChannelOpenTry {
                    previous_channel_id: "abcdefghijkasdfasdfasdfasgdasdgasdfasdfadflmnoasdasdasdfasdfasdfasdfadadgadgadsfpqrstu".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Empty counterparty version (valid choice)".to_string(),
                raw: RawMsgChannelOpenTry {
                    counterparty_version: " ".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: true,
            },
            Test {
                name: "Arbitrary counterparty version (valid choice)".to_string(),
                raw: RawMsgChannelOpenTry {
                    counterparty_version: "anyversion".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: true,
            },
            Test {
                name: "Bad proof height, height = 0".to_string(),
                raw: RawMsgChannelOpenTry {
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
                raw: RawMsgChannelOpenTry {
                    proof_height: None,
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Missing proof init (object proof)".to_string(),
                raw: RawMsgChannelOpenTry {
                    proof_init: Vec::new(),
                    ..default_raw_msg
                },
                want_pass: false,
            },
        ]
            .into_iter()
            .collect();

        for test in tests {
            let res_msg = MsgChannelOpenTry::try_from(test.raw.clone());

            assert_eq!(
                test.want_pass,
                res_msg.is_ok(),
                "MsgChanOpenTry::try_from failed for test {}, \nraw msg {:?} with error {:?}",
                test.name,
                test.raw,
                res_msg.err(),
            );
        }
    }

    #[test]
    fn to_and_from() {
        let raw = get_dummy_raw_msg_chan_open_try(10);
        let msg = MsgChannelOpenTry::try_from(raw.clone()).unwrap();
        let raw_back = RawMsgChannelOpenTry::from(msg.clone());
        let msg_back = MsgChannelOpenTry::try_from(raw_back.clone()).unwrap();
        assert_eq!(raw, raw_back);
        assert_eq!(msg, msg_back);
    }
}
