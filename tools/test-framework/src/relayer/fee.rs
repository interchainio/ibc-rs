use core::time::Duration;
use http::uri::Uri;
use ibc::applications::ics29_fee::msgs::pay_packet::build_pay_packet_message;
use ibc::applications::ics29_fee::msgs::pay_packet_async::build_pay_packet_fee_async_message;
use ibc::applications::ics29_fee::msgs::register_counterparty::build_register_counterparty_address_message;
use ibc::applications::ics29_fee::packet_fee::IdentifiedPacketFees;
use ibc::core::ics04_channel::packet::Sequence;
use ibc::events::IbcEvent;
use ibc_relayer::chain::cosmos::query::fee::{
    query_counterparty_address as raw_query_counterparty_address,
    query_incentivized_packets as raw_query_incentivized_packets,
};
use ibc_relayer::chain::cosmos::types::config::TxConfig;

use crate::error::{handle_generic_error, Error};
use crate::ibc::token::TaggedTokenRef;
use crate::relayer::transfer::build_transfer_message;
use crate::relayer::tx::simple_send_tx;
use crate::types::id::{TaggedChannelIdRef, TaggedPortIdRef};
use crate::types::tagged::{DualTagged, MonoTagged};
use crate::types::wallet::{Wallet, WalletAddress};

pub async fn ibc_token_transfer_with_fee<SrcChain, DstChain>(
    tx_config: &MonoTagged<SrcChain, &TxConfig>,
    port_id: &TaggedPortIdRef<'_, SrcChain, DstChain>,
    channel_id: &TaggedChannelIdRef<'_, SrcChain, DstChain>,
    sender: &MonoTagged<SrcChain, &Wallet>,
    recipient: &MonoTagged<DstChain, &WalletAddress>,
    send_amount: &TaggedTokenRef<'_, SrcChain>,
    receive_fee: &TaggedTokenRef<'_, SrcChain>,
    ack_fee: &TaggedTokenRef<'_, SrcChain>,
    timeout_fee: &TaggedTokenRef<'_, SrcChain>,
    timeout: Duration,
) -> Result<Vec<IbcEvent>, Error> {
    let transfer_message =
        build_transfer_message(port_id, channel_id, sender, recipient, send_amount, timeout)?;

    let pay_message = build_pay_packet_message(
        port_id.value(),
        channel_id.value(),
        &sender
            .value()
            .address
            .0
            .parse()
            .map_err(handle_generic_error)?,
        vec![receive_fee.value().as_coin()],
        vec![ack_fee.value().as_coin()],
        vec![timeout_fee.value().as_coin()],
    )
    .map_err(handle_generic_error)?;

    let messages = vec![pay_message, transfer_message];

    let events = simple_send_tx(tx_config.value(), &sender.value().key, messages).await?;

    Ok(events)
}

pub async fn pay_packet_fee<Chain, Counterparty>(
    tx_config: &MonoTagged<Chain, &TxConfig>,
    port_id: &TaggedPortIdRef<'_, Chain, Counterparty>,
    channel_id: &TaggedChannelIdRef<'_, Chain, Counterparty>,
    sequence: &DualTagged<Chain, Counterparty, Sequence>,
    payer: &MonoTagged<Chain, &Wallet>,
    receive_fee: &TaggedTokenRef<'_, Chain>,
    ack_fee: &TaggedTokenRef<'_, Chain>,
    timeout_fee: &TaggedTokenRef<'_, Chain>,
) -> Result<(), Error> {
    let message = build_pay_packet_fee_async_message(
        port_id.value(),
        channel_id.value(),
        *sequence.value(),
        &payer
            .value()
            .address
            .0
            .parse()
            .map_err(handle_generic_error)?,
        vec![receive_fee.value().as_coin()],
        vec![ack_fee.value().as_coin()],
        vec![timeout_fee.value().as_coin()],
    )
    .map_err(handle_generic_error)?;

    simple_send_tx(tx_config.value(), &payer.value().key, vec![message]).await?;

    Ok(())
}

pub async fn register_counterparty_address<Chain, Counterparty>(
    tx_config: &MonoTagged<Chain, &TxConfig>,
    wallet: &MonoTagged<Chain, &Wallet>,
    counterparty_address: &MonoTagged<Counterparty, &WalletAddress>,
    channel_id: &TaggedChannelIdRef<'_, Chain, Counterparty>,
) -> Result<(), Error> {
    let message = build_register_counterparty_address_message(
        &wallet
            .value()
            .address
            .0
            .parse()
            .map_err(handle_generic_error)?,
        &counterparty_address
            .value()
            .0
            .parse()
            .map_err(handle_generic_error)?,
        channel_id.value(),
    )
    .map_err(handle_generic_error)?;

    let messages = vec![message];

    simple_send_tx(tx_config.value(), &wallet.value().key, messages).await?;

    Ok(())
}

pub async fn query_counterparty_address<Chain, Counterparty>(
    grpc_address: &Uri,
    channel_id: &TaggedChannelIdRef<'_, Chain, Counterparty>,
    address: &MonoTagged<Chain, &WalletAddress>,
) -> Result<Option<MonoTagged<Counterparty, WalletAddress>>, Error> {
    let counterparty_address = raw_query_counterparty_address(
        grpc_address,
        channel_id.value(),
        &address.value().0.parse().map_err(handle_generic_error)?,
    )
    .await
    .map_err(handle_generic_error)?;

    Ok(counterparty_address.map(|address| MonoTagged::new(WalletAddress(address))))
}

pub async fn query_incentivized_packets<Chain, Counterparty>(
    grpc_address: &Uri,
    channel_id: &TaggedChannelIdRef<'_, Chain, Counterparty>,
    port_id: &TaggedPortIdRef<'_, Chain, Counterparty>,
) -> Result<Vec<IdentifiedPacketFees>, Error> {
    raw_query_incentivized_packets(grpc_address, channel_id.value(), port_id.value())
        .await
        .map_err(handle_generic_error)
}
