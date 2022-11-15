use std::fmt::{Display, Formatter, Result};

use crate::tests::relayer_mock::base::types::{height::Height, packet::PacketKey};
use crate::tests::relayer_mock::contexts::chain::{ChainState, ClientId};

#[derive(Debug)]
pub enum Message {
    SendPacket(ClientId, Height, Height, PacketKey),
    AckPacket(ClientId, Height, PacketKey),
    TimeoutPacket(ClientId, Height, PacketKey),
    UpdateClient(ClientId, Height, ChainState),
}

impl Display for Message {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            Self::SendPacket(from, h, ch, p) => {
                write!(f, "{}|SendPacket:{}-{}: {}", from, h, ch, p)
            }
            Self::AckPacket(from, h, p) => write!(f, "{}|AckPacket:{}: {}", from, h, p),
            Self::TimeoutPacket(from, h, p) => write!(f, "{}|TimeoutPacket:{}: {}", from, h, p),
            Self::UpdateClient(from, h, s) => write!(f, "{}|UpdateClient:{}: {:?}", from, h, s),
        }
    }
}
