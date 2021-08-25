use crate::ics04_channel::channel::ChannelEnd;
use crate::ics04_channel::error::Error;
use crate::ics24_host::identifier::PortId;
use crate::prelude::*;
use crate::signer::Signer;
use crate::tx_msg::Msg;

use ibc_proto::ibc::core::channel::v1::MsgChannelOpenInit as RawMsgChannelOpenInit;
use tendermint_proto::Protobuf;

use core::convert::{TryFrom, TryInto};

pub const TYPE_URL: &str = "/ibc.core.channel.v1.MsgChannelOpenInit";

///
/// Message definition for the first step in the channel open handshake (`ChanOpenInit` datagram).
///
#[derive(Clone, Debug, PartialEq)]
pub struct MsgChannelOpenInit {
    pub port_id: PortId,
    pub channel: ChannelEnd,
    pub signer: Signer,
}

impl MsgChannelOpenInit {
    pub fn new(port_id: PortId, channel: ChannelEnd, signer: Signer) -> Self {
        Self {
            port_id,
            channel,
            signer,
        }
    }

    /// Getter: borrow the `port_id` from this message.
    pub fn port_id(&self) -> &PortId {
        &self.port_id
    }

    /// Getter: borrow the `channelEnd` from this message.
    pub fn channel(&self) -> &ChannelEnd {
        &self.channel
    }
}

impl Msg for MsgChannelOpenInit {
    type ValidationError = Error;
    type Raw = RawMsgChannelOpenInit;

    fn route(&self) -> String {
        crate::keys::ROUTER_KEY.to_string()
    }

    fn type_url(&self) -> String {
        TYPE_URL.to_string()
    }
}

impl Protobuf<RawMsgChannelOpenInit> for MsgChannelOpenInit {}

impl TryFrom<RawMsgChannelOpenInit> for MsgChannelOpenInit {
    type Error = Error;

    fn try_from(raw_msg: RawMsgChannelOpenInit) -> Result<Self, Self::Error> {
        Ok(MsgChannelOpenInit {
            port_id: raw_msg.port_id.parse().map_err(Error::identifier)?,
            channel: raw_msg
                .channel
                .ok_or_else(Error::missing_channel)?
                .try_into()?,
            signer: raw_msg.signer.into(),
        })
    }
}

impl From<MsgChannelOpenInit> for RawMsgChannelOpenInit {
    fn from(domain_msg: MsgChannelOpenInit) -> Self {
        RawMsgChannelOpenInit {
            port_id: domain_msg.port_id.to_string(),
            channel: Some(domain_msg.channel.into()),
            signer: domain_msg.signer.to_string(),
        }
    }
}

#[cfg(test)]
pub mod test_util {
    use crate::prelude::*;
    use ibc_proto::ibc::core::channel::v1::MsgChannelOpenInit as RawMsgChannelOpenInit;

    use crate::ics04_channel::channel::test_util::get_dummy_raw_channel_end;
    use crate::ics24_host::identifier::PortId;
    use crate::test_utils::get_dummy_bech32_account;

    /// Returns a dummy `RawMsgChannelOpenInit`, for testing only!
    pub fn get_dummy_raw_msg_chan_open_init() -> RawMsgChannelOpenInit {
        RawMsgChannelOpenInit {
            port_id: PortId::default().to_string(),
            channel: Some(get_dummy_raw_channel_end()),
            signer: get_dummy_bech32_account(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::ics04_channel::msgs::chan_open_init::test_util::get_dummy_raw_msg_chan_open_init;
    use crate::ics04_channel::msgs::chan_open_init::MsgChannelOpenInit;
    use crate::prelude::*;
    use core::convert::TryFrom;
    use ibc_proto::ibc::core::channel::v1::MsgChannelOpenInit as RawMsgChannelOpenInit;
    use test_env_log::test;

    #[test]
    fn channel_open_init_from_raw() {
        struct Test {
            name: String,
            raw: RawMsgChannelOpenInit,
            want_pass: bool,
        }

        let default_raw_msg = get_dummy_raw_msg_chan_open_init();

        let tests: Vec<Test> = vec![
            Test {
                name: "Good parameters".to_string(),
                raw: default_raw_msg.clone(),
                want_pass: true,
            },
            Test {
                name: "Incorrect port identifier, slash (separator) prohibited".to_string(),
                raw: RawMsgChannelOpenInit {
                    port_id: "p34/".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Missing channel".to_string(),
                raw: RawMsgChannelOpenInit {
                    channel: None,
                    ..default_raw_msg
                },
                want_pass: false,
            },
        ]
        .into_iter()
        .collect();

        for test in tests {
            let res_msg = MsgChannelOpenInit::try_from(test.raw.clone());

            assert_eq!(
                test.want_pass,
                res_msg.is_ok(),
                "MsgChanOpenInit::try_from failed for test {}, \nraw msg {:?} with error {:?}",
                test.name,
                test.raw,
                res_msg.err(),
            );
        }
    }

    #[test]
    fn to_and_from() {
        let raw = get_dummy_raw_msg_chan_open_init();
        let msg = MsgChannelOpenInit::try_from(raw.clone()).unwrap();
        let raw_back = RawMsgChannelOpenInit::from(msg.clone());
        let msg_back = MsgChannelOpenInit::try_from(raw_back.clone()).unwrap();
        assert_eq!(raw, raw_back);
        assert_eq!(msg, msg_back);
    }
}
