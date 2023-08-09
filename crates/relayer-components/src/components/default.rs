use core::marker::PhantomData;

use crate::relay::impls::auto_relayers::concurrent_bidirectional::ConcurrentBidirectionalRelayer;
use crate::relay::impls::auto_relayers::concurrent_event::ConcurrentEventSubscriptionRelayer;
use crate::relay::impls::client::update::BuildUpdateClientMessages;
use crate::relay::impls::messages::skip_update_client::SkipUpdateClient;
use crate::relay::impls::messages::wait_update_client::WaitUpdateClient;
use crate::relay::impls::packet_relayers::general::filter_relayer::FilterRelayer;
use crate::relay::impls::packet_relayers::general::full_relay::FullCycleRelayer;
use crate::relay::impls::packet_relayers::general::lock::LockPacketRelayer;
use crate::relay::impls::packet_relayers::general::log::LoggerRelayer;
use crate::relay::impls::packet_relayers::receive::base_receive_packet::BaseReceivePacketRelayer;
use crate::relay::impls::packet_relayers::receive::skip_received_packet::SkipReceivedPacketRelayer;
use crate::std_prelude::*;

pub struct DefaultComponents<BaseComponents>(pub PhantomData<BaseComponents>);

crate::derive_chain_status_querier!(DefaultComponents<BaseComponents>, BaseComponents);

crate::derive_consensus_state_querier!(DefaultComponents<BaseComponents>, BaseComponents);

crate::derive_update_client_message_builder!(
    DefaultComponents<BaseComponents>,
    SkipUpdateClient<WaitUpdateClient<BuildUpdateClientMessages>>,
);

crate::derive_packet_relayer!(
    DefaultComponents<BaseComponents>,
    LockPacketRelayer<LoggerRelayer<FilterRelayer<FullCycleRelayer>>>,
);

crate::derive_receive_packet_relayer!(
    DefaultComponents<BaseComponents>,
    SkipReceivedPacketRelayer<BaseReceivePacketRelayer>,
);

crate::derive_auto_relayer!(
    DefaultComponents<BaseComponents>,
    ConcurrentBidirectionalRelayer<ConcurrentEventSubscriptionRelayer>,
);
