use async_trait::async_trait;

use crate::base::chain::traits::types::HasIbcPacketTypes;
use crate::base::chain::types::aliases::WriteAcknowledgementEvent;
use crate::std_prelude::*;

#[async_trait]
pub trait CanBuildAckPacketMessage<Counterparty>: HasIbcPacketTypes<Counterparty>
where
    Counterparty: HasIbcPacketTypes<
        Self,
        IncomingPacket = Self::OutgoingPacket,
        OutgoingPacket = Self::IncomingPacket,
    >,
{
    async fn build_ack_packet_message(
        &self,
        height: &Self::Height,
        packet: &Self::IncomingPacket,
        ack: &WriteAcknowledgementEvent,
    ) -> Result<Counterparty::Message, Self::Error>;
}
