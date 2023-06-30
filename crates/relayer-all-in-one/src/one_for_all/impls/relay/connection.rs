use async_trait::async_trait;

use ibc_relayer_components::relay::impls::connection::open_init::{
    InitializeConnection, InjectMissingConnectionInitEventError,
};
use ibc_relayer_components::relay::impls::connection::open_try::{
    InjectMissingConnectionTryEventError, RelayConnectionOpenTry,
};
use ibc_relayer_components::relay::traits::connection::open_init::{
    CanInitConnection, ConnectionInitializer,
};
use ibc_relayer_components::relay::traits::connection::open_try::{
    CanRelayConnectionOpenTry, ConnectionOpenTryRelayer,
};

use crate::one_for_all::traits::chain::{OfaChain, OfaIbcChain};
use crate::one_for_all::traits::relay::OfaRelay;
use crate::one_for_all::types::relay::OfaRelayWrapper;
use crate::std_prelude::*;

impl<Relay> InjectMissingConnectionInitEventError for OfaRelayWrapper<Relay>
where
    Relay: OfaRelay,
{
    fn missing_connection_init_event_error(&self) -> Relay::Error {
        self.relay.missing_connection_init_event_error()
    }
}

impl<Relay> InjectMissingConnectionTryEventError for OfaRelayWrapper<Relay>
where
    Relay: OfaRelay,
{
    fn missing_connection_try_event_error(
        &self,
        src_connection_id: &<Relay::SrcChain as OfaChain>::ConnectionId,
    ) -> Relay::Error {
        self.relay
            .missing_connection_try_event_error(src_connection_id)
    }
}

#[async_trait]
impl<Relay> CanInitConnection for OfaRelayWrapper<Relay>
where
    Relay: OfaRelay,
{
    async fn init_connection(
        &self,
        init_connection_options: &<Relay::SrcChain as OfaIbcChain<Relay::DstChain>>::InitConnectionOptions,
    ) -> Result<<Relay::SrcChain as OfaChain>::ConnectionId, Self::Error> {
        InitializeConnection::init_connection(self, init_connection_options).await
    }
}

#[async_trait]
impl<Relay> CanRelayConnectionOpenTry for OfaRelayWrapper<Relay>
where
    Relay: OfaRelay,
{
    async fn relay_connection_open_try(
        &self,
        src_connection_id: &<Relay::SrcChain as OfaChain>::ConnectionId,
    ) -> Result<<Relay::DstChain as OfaChain>::ConnectionId, Self::Error> {
        RelayConnectionOpenTry::relay_connection_open_try(self, src_connection_id).await
    }
}
