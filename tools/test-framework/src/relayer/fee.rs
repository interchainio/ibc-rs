use core::time::Duration;
use http::uri::Uri;
use ibc_relayer::chain::cosmos::query::fee::{
    query_counterparty_payee as raw_query_counterparty_payee,
    query_incentivized_packets as raw_query_incentivized_packets,
};
use ibc_relayer::chain::cosmos::tx::simple_send_tx;
use ibc_relayer::chain::cosmos::types::config::TxConfig;
use ibc_relayer::channel::version::Version;
use ibc_relayer::event::IbcEventWithHeight;
use ibc_relayer_types::applications::ics29_fee::msgs::pay_packet::build_pay_packet_message;
use ibc_relayer_types::applications::ics29_fee::msgs::pay_packet_async::build_pay_packet_fee_async_message;
use ibc_relayer_types::applications::ics29_fee::msgs::register_payee::{
    build_register_counterparty_payee_message, build_register_payee_message,
};
use ibc_relayer_types::applications::ics29_fee::packet_fee::IdentifiedPacketFees;
use ibc_relayer_types::core::ics04_channel::packet::Sequence;
use tendermint_rpc::HttpClient;

use crate::error::{handle_generic_error, Error};
use crate::ibc::token::{TaggedTokenExt, TaggedTokenRef};
use crate::relayer::transfer::build_transfer_message;
use crate::types::id::{TaggedChannelIdRef, TaggedPortIdRef};
use crate::types::tagged::{DualTagged, MonoTagged};
use crate::types::wallet::{Wallet, WalletAddress};

pub async fn ibc_token_transfer_with_fee<SrcChain, DstChain>(
    rpc_client: MonoTagged<SrcChain, &HttpClient>,
    tx_config: &MonoTagged<SrcChain, &TxConfig>,
    port_id: &TaggedPortIdRef<'_, SrcChain, DstChain>,
    channel_id: &TaggedChannelIdRef<'_, SrcChain, DstChain>,
    channel_version: &Version,
    sender: &MonoTagged<SrcChain, &Wallet>,
    recipient: &MonoTagged<DstChain, &WalletAddress>,
    send_amount: &Vec<TaggedTokenRef<'_, SrcChain>>,
    receive_fee: &TaggedTokenRef<'_, SrcChain>,
    ack_fee: &TaggedTokenRef<'_, SrcChain>,
    timeout_fee: &TaggedTokenRef<'_, SrcChain>,
    timeout: Duration,
) -> Result<Vec<IbcEventWithHeight>, Error> {
    let transfer_message = build_transfer_message(
        port_id,
        channel_id,
        channel_version,
        sender,
        recipient,
        send_amount,
        timeout,
        None,
    )?;

    let pay_message = build_pay_packet_message(
        port_id.value(),
        channel_id.value(),
        &sender
            .value()
            .address
            .0
            .parse()
            .map_err(handle_generic_error)?,
        vec![receive_fee.as_coin()],
        vec![ack_fee.as_coin()],
        vec![timeout_fee.as_coin()],
    )
    .map_err(handle_generic_error)?;

    let messages = vec![pay_message, transfer_message];

    let events = simple_send_tx(
        rpc_client.value(),
        tx_config.value(),
        &sender.value().key,
        messages,
    )
    .await?;

    Ok(events)
}

pub async fn pay_packet_fee<Chain, Counterparty>(
    rpc_client: MonoTagged<Chain, &HttpClient>,
    tx_config: &MonoTagged<Chain, &TxConfig>,
    port_id: &TaggedPortIdRef<'_, Chain, Counterparty>,
    channel_id: &TaggedChannelIdRef<'_, Chain, Counterparty>,
    sequence: &DualTagged<Chain, Counterparty, Sequence>,
    payer: &MonoTagged<Chain, &Wallet>,
    receive_fee: &TaggedTokenRef<'_, Chain>,
    ack_fee: &TaggedTokenRef<'_, Chain>,
    timeout_fee: &TaggedTokenRef<'_, Chain>,
) -> Result<Vec<IbcEventWithHeight>, Error> {
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
        vec![receive_fee.as_coin()],
        vec![ack_fee.as_coin()],
        vec![timeout_fee.as_coin()],
    )
    .map_err(handle_generic_error)?;

    let events = simple_send_tx(
        rpc_client.value(),
        tx_config.value(),
        &payer.value().key,
        vec![message],
    )
    .await
    .map_err(Error::relayer)?;

    Ok(events)
}

pub async fn register_counterparty_payee<Chain, Counterparty>(
    rpc_client: MonoTagged<Chain, &HttpClient>,
    tx_config: &MonoTagged<Chain, &TxConfig>,
    wallet: &MonoTagged<Chain, &Wallet>,
    counterparty_payee: &MonoTagged<Counterparty, &WalletAddress>,
    channel_id: &TaggedChannelIdRef<'_, Chain, Counterparty>,
    port_id: &TaggedPortIdRef<'_, Chain, Counterparty>,
) -> Result<(), Error> {
    let message = build_register_counterparty_payee_message(
        &wallet
            .value()
            .address
            .0
            .parse()
            .map_err(handle_generic_error)?,
        &counterparty_payee
            .value()
            .0
            .parse()
            .map_err(handle_generic_error)?,
        channel_id.value(),
        port_id.value(),
    )
    .map_err(handle_generic_error)?;

    let messages = vec![message];

    simple_send_tx(
        rpc_client.value(),
        tx_config.value(),
        &wallet.value().key,
        messages,
    )
    .await?;

    Ok(())
}

pub async fn register_payee<Chain, Counterparty>(
    rpc_client: MonoTagged<Chain, &HttpClient>,
    tx_config: &MonoTagged<Chain, &TxConfig>,
    wallet: &MonoTagged<Chain, &Wallet>,
    payee: &MonoTagged<Chain, &WalletAddress>,
    channel_id: &TaggedChannelIdRef<'_, Chain, Counterparty>,
    port_id: &TaggedPortIdRef<'_, Chain, Counterparty>,
) -> Result<(), Error> {
    let message = build_register_payee_message(
        &wallet
            .value()
            .address
            .0
            .parse()
            .map_err(handle_generic_error)?,
        &payee.value().0.parse().map_err(handle_generic_error)?,
        channel_id.value(),
        port_id.value(),
    )
    .map_err(handle_generic_error)?;

    let messages = vec![message];

    simple_send_tx(
        rpc_client.value(),
        tx_config.value(),
        &wallet.value().key,
        messages,
    )
    .await?;

    Ok(())
}

pub async fn query_counterparty_payee<Chain, Counterparty>(
    grpc_address: &Uri,
    channel_id: &TaggedChannelIdRef<'_, Chain, Counterparty>,
    address: &MonoTagged<Chain, &WalletAddress>,
) -> Result<Option<MonoTagged<Counterparty, WalletAddress>>, Error> {
    let counterparty_payee = raw_query_counterparty_payee(
        grpc_address,
        channel_id.value(),
        &address.value().0.parse().map_err(handle_generic_error)?,
    )
    .await
    .map_err(handle_generic_error)?;

    Ok(counterparty_payee.map(|address| MonoTagged::new(WalletAddress(address))))
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
