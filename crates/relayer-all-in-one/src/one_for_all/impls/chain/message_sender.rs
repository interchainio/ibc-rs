use async_trait::async_trait;
use ibc_relayer_components::chain::traits::message_sender::CanSendMessages;

use crate::one_for_all::traits::chain::OfaChain;
use crate::one_for_all::types::chain::OfaChainWrapper;
use crate::std_prelude::*;

#[async_trait]
impl<Chain> CanSendMessages for OfaChainWrapper<Chain>
where
    Chain: OfaChain,
{
    async fn send_messages(
        &self,
        messages: Vec<Chain::Message>,
    ) -> Result<Vec<Vec<Chain::Event>>, Chain::Error> {
        let events = self.chain.send_messages(messages).await?;
        Ok(events)
    }
}
