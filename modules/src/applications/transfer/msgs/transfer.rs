//! This is the definition of a transfer messages that an application submits to a chain.

use crate::prelude::*;

use ibc_proto::cosmos::base::v1beta1::Coin;
use ibc_proto::google::protobuf::Any;
use ibc_proto::ibc::applications::transfer::v1::MsgTransfer as RawMsgTransfer;
use tendermint_proto::Protobuf;

use crate::applications::transfer::error::Error;
use crate::core::ics02_client::height::Height;
use crate::core::ics24_host::identifier::{ChannelId, PortId};
use crate::signer::Signer;
use crate::timestamp::Timestamp;
use crate::tx_msg::Msg;

pub const TYPE_URL: &str = "/ibc.applications.transfer.v1.MsgTransfer";

/// Message used to build an ICS20 token transfer packet.
///
/// Note that this message is not a packet yet, as it lacks the proper sequence
/// number, and destination port/channel. This is by design. The sender of the
/// packet, which might be the user of a command line application, should only
/// have to specify the information related to the transfer of the token, and
/// let the library figure out how to build the packet properly.
#[derive(Clone, Debug, PartialEq)]
pub struct MsgTransfer<C = Coin> {
    /// the port on which the packet will be sent
    pub source_port: PortId,
    /// the channel by which the packet will be sent
    pub source_channel: ChannelId,
    /// the tokens to be transferred
    pub token: C,
    /// the sender address
    pub sender: Signer,
    /// the recipient address on the destination chain
    pub receiver: Signer,
    /// Timeout height relative to the current block height.
    /// The timeout is disabled when set to None.
    pub timeout_height: Option<Height>,
    /// Timeout timestamp relative to the current block timestamp.
    /// The timeout is disabled when set to 0.
    pub timeout_timestamp: Timestamp,
}

impl Msg for MsgTransfer {
    type ValidationError = Error;
    type Raw = RawMsgTransfer;

    fn route(&self) -> String {
        crate::keys::ROUTER_KEY.to_string()
    }

    fn type_url(&self) -> String {
        TYPE_URL.to_string()
    }
}

impl TryFrom<RawMsgTransfer> for MsgTransfer {
    type Error = Error;

    fn try_from(raw_msg: RawMsgTransfer) -> Result<Self, Self::Error> {
        let timeout_timestamp = Timestamp::from_nanoseconds(raw_msg.timeout_timestamp)
            .map_err(|_| Error::invalid_packet_timeout_timestamp(raw_msg.timeout_timestamp))?;

        let timeout_height: Option<Height> = raw_msg
            .timeout_height
            .map(|raw_height| raw_height.try_into())
            .transpose()
            .map_err(|e| {
                Error::invalid_packet_timeout_height(format!("invalid timeout height {}", e))
            })?;

        Ok(MsgTransfer {
            source_port: raw_msg
                .source_port
                .parse()
                .map_err(|e| Error::invalid_port_id(raw_msg.source_port.clone(), e))?,
            source_channel: raw_msg
                .source_channel
                .parse()
                .map_err(|e| Error::invalid_channel_id(raw_msg.source_channel.clone(), e))?,
            token: raw_msg.token.ok_or_else(Error::invalid_token)?,
            sender: raw_msg.sender.parse().map_err(Error::signer)?,
            receiver: raw_msg.receiver.parse().map_err(Error::signer)?,
            timeout_height,
            timeout_timestamp,
        })
    }
}

impl From<MsgTransfer> for RawMsgTransfer {
    fn from(domain_msg: MsgTransfer) -> Self {
        RawMsgTransfer {
            source_port: domain_msg.source_port.to_string(),
            source_channel: domain_msg.source_channel.to_string(),
            token: Some(domain_msg.token),
            sender: domain_msg.sender.to_string(),
            receiver: domain_msg.receiver.to_string(),
            timeout_height: domain_msg.timeout_height.map(|height| height.into()),
            timeout_timestamp: domain_msg.timeout_timestamp.nanoseconds(),
        }
    }
}

impl Protobuf<RawMsgTransfer> for MsgTransfer {}

impl TryFrom<Any> for MsgTransfer {
    type Error = Error;

    fn try_from(raw: Any) -> Result<Self, Self::Error> {
        match raw.type_url.as_str() {
            TYPE_URL => MsgTransfer::decode_vec(&raw.value).map_err(Error::decode_raw_msg),
            _ => Err(Error::unknown_msg_type(raw.type_url)),
        }
    }
}

impl From<MsgTransfer> for Any {
    fn from(msg: MsgTransfer) -> Self {
        Self {
            type_url: TYPE_URL.to_string(),
            value: msg
                .encode_vec()
                .expect("encoding to `Any` from `MsgTranfer`"),
        }
    }
}

#[cfg(test)]
pub mod test_util {
    use core::ops::Add;
    use core::time::Duration;

    use super::MsgTransfer;
    use crate::bigint::U256;
    use crate::signer::Signer;
    use crate::{
        applications::transfer::{BaseCoin, PrefixedCoin},
        core::ics24_host::identifier::{ChannelId, PortId},
        test_utils::get_dummy_bech32_account,
        timestamp::Timestamp,
        Height,
    };

    // Returns a dummy ICS20 `MsgTransfer`, for testing only!
    pub fn get_dummy_msg_transfer(height: u64) -> MsgTransfer<PrefixedCoin> {
        let address: Signer = get_dummy_bech32_account().as_str().parse().unwrap();
        MsgTransfer {
            source_port: PortId::default(),
            source_channel: ChannelId::default(),
            token: BaseCoin {
                denom: "uatom".parse().unwrap(),
                amount: U256::from(10).into(),
            }
            .into(),
            sender: address.clone(),
            receiver: address,
            timeout_timestamp: Timestamp::now().add(Duration::from_secs(10)).unwrap(),
            timeout_height: Some(Height {
                revision_number: 0,
                revision_height: height,
            }),
        }
    }
}
