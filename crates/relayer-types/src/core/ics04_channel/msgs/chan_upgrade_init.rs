use crate::core::ics04_channel::upgrade_fields::UpgradeFields;

use ibc_proto::ibc::core::channel::v1::MsgChannelUpgradeInit as RawMsgChannelUpgradeInit;
use ibc_proto::Protobuf;

use crate::core::ics04_channel::error::Error;
use crate::core::ics24_host::identifier::{ChannelId, PortId};
use crate::signer::Signer;
use crate::tx_msg::Msg;

pub const TYPE_URL: &str = "/ibc.core.channel.v1.MsgChannelUpgradeInit";

/// Message definition for the first step in the channel
/// upgrade handshake (`ChanUpgradeInit` datagram).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MsgChannelUpgradeInit {
    pub port_id: PortId,
    pub channel_id: ChannelId,
    pub fields: UpgradeFields,
    pub signer: Signer,
}

impl MsgChannelUpgradeInit {
    pub fn new(
        port_id: PortId,
        channel_id: ChannelId,
        fields: UpgradeFields,
        signer: Signer,
    ) -> Self {
        Self {
            port_id,
            channel_id,
            fields,
            signer,
        }
    }
}

impl Msg for MsgChannelUpgradeInit {
    type ValidationError = Error;
    type Raw = RawMsgChannelUpgradeInit;

    fn route(&self) -> String {
        crate::keys::ROUTER_KEY.to_string()
    }

    fn type_url(&self) -> String {
        TYPE_URL.to_string()
    }
}

impl Protobuf<RawMsgChannelUpgradeInit> for MsgChannelUpgradeInit {}

impl TryFrom<RawMsgChannelUpgradeInit> for MsgChannelUpgradeInit {
    type Error = Error;

    fn try_from(raw_msg: RawMsgChannelUpgradeInit) -> Result<Self, Self::Error> {
        let raw_fields = raw_msg.fields.ok_or(Error::missing_upgrade_fields())?;
        let fields = UpgradeFields::try_from(raw_fields)?;

        Ok(MsgChannelUpgradeInit {
            port_id: raw_msg.port_id.parse().map_err(Error::identifier)?,
            channel_id: raw_msg.channel_id.parse().map_err(Error::identifier)?,
            signer: raw_msg.signer.parse().map_err(Error::signer)?,
            fields,
        })
    }
}

impl From<MsgChannelUpgradeInit> for RawMsgChannelUpgradeInit {
    fn from(domain_msg: MsgChannelUpgradeInit) -> Self {
        Self {
            port_id: domain_msg.port_id.to_string(),
            channel_id: domain_msg.channel_id.to_string(),
            signer: domain_msg.signer.to_string(),
            fields: Some(domain_msg.fields.into()),
        }
    }
}

#[cfg(test)]
pub mod test_util {
    use ibc_proto::ibc::core::channel::v1::MsgChannelUpgradeInit as RawMsgChannelUpgradeInit;

    use crate::core::ics04_channel::upgrade_fields::test_util::get_dummy_upgrade_fields;
    use crate::core::ics24_host::identifier::{ChannelId, PortId};
    use crate::test_utils::get_dummy_bech32_account;

    /// Returns a dummy `RawMsgChannelUpgradeInit`, for testing only!
    pub fn get_dummy_raw_msg_chan_upgrade_init() -> RawMsgChannelUpgradeInit {
        RawMsgChannelUpgradeInit {
            port_id: PortId::default().to_string(),
            channel_id: ChannelId::default().to_string(),
            signer: get_dummy_bech32_account(),
            fields: Some(get_dummy_upgrade_fields()),
        }
    }
}

#[cfg(test)]
mod tests {
    use test_log::test;

    use ibc_proto::ibc::core::channel::v1::MsgChannelUpgradeInit as RawMsgChannelUpgradeInit;

    use crate::core::ics04_channel::msgs::chan_upgrade_init::test_util::get_dummy_raw_msg_chan_upgrade_init;
    use crate::core::ics04_channel::msgs::chan_upgrade_init::MsgChannelUpgradeInit;

    #[test]
    fn parse_channel_upgrade_init_msg() {
        struct Test {
            name: String,
            raw: RawMsgChannelUpgradeInit,
            want_pass: bool,
        }

        let default_raw_msg = get_dummy_raw_msg_chan_upgrade_init();

        let tests: Vec<Test> = vec![
            Test {
                name: "Good parameters".to_string(),
                raw: default_raw_msg.clone(),
                want_pass: true,
            },
            Test {
                name: "Correct port ID".to_string(),
                raw: RawMsgChannelUpgradeInit {
                    port_id: "p36".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: true,
            },
            Test {
                name: "Port too short".to_string(),
                raw: RawMsgChannelUpgradeInit {
                    port_id: "p".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Port too long".to_string(),
                raw: RawMsgChannelUpgradeInit {
                    port_id: "abcdefsdfasdfasdfasdfasdfasdfadsfasdgafsgadfasdfasdfasdfsdfasdfaghijklmnopqrstuabcdefsdfasdfasdfasdfasdfasdfadsfasdgafsgadfasdfasdfasdfsdfasdfaghijklmnopqrstu".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Correct channel ID".to_string(),
                raw: RawMsgChannelUpgradeInit {
                    channel_id: "channel-2".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: true,
            },
            Test {
                name: "Channel name too short".to_string(),
                raw: RawMsgChannelUpgradeInit {
                    channel_id: "c".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
            Test {
                name: "Channel name too long".to_string(),
                raw: RawMsgChannelUpgradeInit {
                    channel_id: "channel-128391283791827398127398791283912837918273981273987912839".to_string(),
                    ..default_raw_msg.clone()
                },
                want_pass: false,
            },
        ]
        .into_iter()
        .collect();

        for test in tests {
            let res = MsgChannelUpgradeInit::try_from(test.raw.clone());

            assert_eq!(
                test.want_pass,
                res.is_ok(),
                "MsgChannelUpgradeInit::try_from failed for test {}, \nraw msg {:?} with err {:?}",
                test.name,
                test.raw,
                res.err()
            );
        }
    }

    #[test]
    fn to_and_from() {
        let raw = get_dummy_raw_msg_chan_upgrade_init();
        let msg = MsgChannelUpgradeInit::try_from(raw.clone()).unwrap();
        let raw_back = RawMsgChannelUpgradeInit::from(msg.clone());
        let msg_back = MsgChannelUpgradeInit::try_from(raw_back.clone()).unwrap();
        assert_eq!(raw, raw_back);
        assert_eq!(msg, msg_back);
    }
}
