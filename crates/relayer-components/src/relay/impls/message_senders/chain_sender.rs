use async_trait::async_trait;

use crate::chain::traits::message_sender::CanSendMessages;
use crate::chain::traits::types::ibc::HasIbcChainTypes;
use crate::relay::traits::ibc_message_sender::IbcMessageSender;
use crate::relay::traits::target::ChainTarget;
use crate::relay::traits::types::HasRelayChains;
use crate::std_prelude::*;

pub struct SendIbcMessagesToChain;

#[async_trait]
impl<Relay, Target, TargetChain> IbcMessageSender<Relay, Target> for SendIbcMessagesToChain
where
    Relay: HasRelayChains,
    Target: ChainTarget<Relay, TargetChain = TargetChain>,
    TargetChain: CanSendMessages,
    TargetChain: HasIbcChainTypes<Target::CounterpartyChain>,
{
    async fn send_messages(
        relay: &Relay,
        messages: Vec<TargetChain::Message>,
    ) -> Result<Vec<Vec<TargetChain::Event>>, Relay::Error> {
        let events = Target::target_chain(relay)
            .send_messages(messages)
            .await
            .map_err(Target::target_chain_error)?;

        Ok(events)
    }
}
