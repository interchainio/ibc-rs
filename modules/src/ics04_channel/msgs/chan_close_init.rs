use crate::prelude::*;
use core::convert::TryFrom;
use tendermint_proto::Protobuf;

use ibc_proto::ibc::core::channel::v1::MsgChannelCloseInit as RawMsgChannelCloseInit;

use crate::ics04_channel::error::Error;
use crate::ics24_host::identifier::{ChannelId, PortId};
use crate::signer::Signer;
use crate::tx_msg::Msg;

pub const TYPE_URL: &str = "/ibc.core.channel.v1.MsgChannelCloseInit";

///
/// Message definition for the first step in the channel close handshake (`ChanCloseInit` datagram).
///
#[derive(Clone, Debug, PartialEq)]
pub struct MsgChannelCloseInit {
    pub port_id: PortId,
    pub channel_id: ChannelId,
    pub signer: Signer,
}

impl MsgChannelCloseInit {
    pub fn new(port_id: PortId, channel_id: ChannelId, signer: Signer) -> Self {
        Self {
            port_id,
            channel_id,
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
}

impl Msg for MsgChannelCloseInit {
    type ValidationError = Error;
    type Raw = RawMsgChannelCloseInit;

    fn route(&self) -> String {
        crate::keys::ROUTER_KEY.to_string()
    }

    fn type_url(&self) -> String {
        TYPE_URL.to_string()
    }
}

impl Protobuf<RawMsgChannelCloseInit> for MsgChannelCloseInit {}

impl TryFrom<RawMsgChannelCloseInit> for MsgChannelCloseInit {
    type Error = Error;

    fn try_from(raw_msg: RawMsgChannelCloseInit) -> Result<Self, Self::Error> {
        Ok(MsgChannelCloseInit {
            port_id: raw_msg.port_id.parse().map_err(Error::identifier)?,
            channel_id: raw_msg.channel_id.parse().map_err(Error::identifier)?,
            signer: raw_msg.signer.into(),
        })
    }
}

impl From<MsgChannelCloseInit> for RawMsgChannelCloseInit {
    fn from(domain_msg: MsgChannelCloseInit) -> Self {
        RawMsgChannelCloseInit {
            port_id: domain_msg.port_id.to_string(),
            channel_id: domain_msg.channel_id.to_string(),
            signer: domain_msg.signer.to_string(),
        }
    }
}

#[cfg(test)]
pub mod test_util {
    use crate::prelude::*;
    use ibc_proto::ibc::core::channel::v1::MsgChannelCloseInit as RawMsgChannelCloseInit;

    use crate::ics24_host::identifier::{ChannelId, PortId};
    use crate::test_utils::get_dummy_bech32_account;

    /// Returns a dummy `RawMsgChannelCloseInit`, for testing only!
    pub fn get_dummy_raw_msg_chan_close_init() -> RawMsgChannelCloseInit {
        RawMsgChannelCloseInit {
            port_id: PortId::default().to_string(),
            channel_id: ChannelId::default().to_string(),
            signer: get_dummy_bech32_account(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::prelude::*;
    use core::convert::TryFrom;
    use test_env_log::test;

    use ibc_proto::ibc::core::channel::v1::MsgChannelCloseInit as RawMsgChannelCloseInit;

    use crate::ics04_channel::msgs::chan_close_init::test_util::get_dummy_raw_msg_chan_close_init;
    use crate::ics04_channel::msgs::chan_close_init::MsgChannelCloseInit;

    #[test]
    fn parse_channel_close_init_msg() {
        struct Test {
            name: String,
            raw: RawMsgChannelCloseInit,
            want_pass: bool,
        }

        let default_raw_msg = get_dummy_raw_msg_chan_close_init();

        let tests: Vec<Test> = vec![
            Test {
                name: "Good parameters".to_string(),
                raw: default_raw_msg.clone(),
                want_pass: true,
            },
            Test {
                name: "Correct port".to_string(),
                raw: RawMsgChannelCloseInit {
                    port_id: "p34".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: true,
            },
            Test {
                name: "Bad port, name too short".to_string(),
                raw: RawMsgChannelCloseInit {
                    port_id: "p".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Bad port, name too long".to_string(),
                raw: RawMsgChannelCloseInit {
                    port_id: "abcdefsdfasdfasdfasdfasdfasdfadsfasdgafsgadfasdfasdfasdfsdfasdfaghijklmnopqrstu".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Correct channel identifier".to_string(),
                raw: RawMsgChannelCloseInit {
                    channel_id: "channelid34".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: true,
            },
            Test {
                name: "Bad channel, name too short".to_string(),
                raw: RawMsgChannelCloseInit {
                    channel_id: "chshort".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Bad channel, name too long".to_string(),
                raw: RawMsgChannelCloseInit {
                    channel_id: "abcdeasdfasdfasdfasdfasdfasdfasdfasdfdgasdfasdfasdfghijklmnopqrstu".to_string(),
                    ..default_raw_msg
                },
                want_pass: false,
            },
        ]
            .into_iter()
            .collect();

        for test in tests {
            let msg = MsgChannelCloseInit::try_from(test.raw.clone());

            assert_eq!(
                test.want_pass,
                msg.is_ok(),
                "MsgChanCloseInit::try_from failed for test {}, \nmsg {:?} with error {:?}",
                test.name,
                test.raw,
                msg.err(),
            );
        }
    }

    #[test]
    fn to_and_from() {
        let raw = get_dummy_raw_msg_chan_close_init();
        let msg = MsgChannelCloseInit::try_from(raw.clone()).unwrap();
        let raw_back = RawMsgChannelCloseInit::from(msg.clone());
        let msg_back = MsgChannelCloseInit::try_from(raw_back.clone()).unwrap();
        assert_eq!(raw, raw_back);
        assert_eq!(msg, msg_back);
    }
}
