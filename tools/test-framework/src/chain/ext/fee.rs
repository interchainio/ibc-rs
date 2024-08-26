use core::time::Duration;
use eyre::eyre;

use ibc_relayer::chain::handle::ChainHandle;
use ibc_relayer::event::IbcEventWithHeight;
use ibc_relayer_types::applications::ics29_fee::packet_fee::IdentifiedPacketFees;
use ibc_relayer_types::core::ics04_channel::packet::Sequence;

use crate::chain::driver::ChainDriver;
use crate::chain::tagged::TaggedChainDriverExt;
use crate::error::Error;
use crate::ibc::token::TaggedTokenRef;
use crate::prelude::ConnectedChannel;
use crate::relayer::fee::{
    ibc_token_transfer_with_fee, pay_packet_fee, query_counterparty_payee,
    query_incentivized_packets, register_counterparty_payee, register_payee,
};
use crate::types::id::{TaggedChannelIdRef, TaggedPortIdRef};
use crate::types::tagged::*;
use crate::types::wallet::{Wallet, WalletAddress};

pub trait ChainFeeMethodsExt<Chain: ChainHandle> {
    fn ibc_token_transfer_with_fee<Counterparty: ChainHandle>(
        &self,
        channel: &ConnectedChannel<Chain, Counterparty>,
        sender: &MonoTagged<Chain, &Wallet>,
        recipient: &MonoTagged<Counterparty, &WalletAddress>,
        send_amount: &Vec<TaggedTokenRef<'_, Chain>>,
        receive_fee: &TaggedTokenRef<'_, Chain>,
        ack_fee: &TaggedTokenRef<'_, Chain>,
        timeout_fee: &TaggedTokenRef<'_, Chain>,
        timeout: Duration,
    ) -> Result<Vec<IbcEventWithHeight>, Error>;

    fn pay_packet_fee<Counterparty>(
        &self,
        port_id: &TaggedPortIdRef<'_, Chain, Counterparty>,
        channel_id: &TaggedChannelIdRef<'_, Chain, Counterparty>,
        sequence: &DualTagged<Chain, Counterparty, Sequence>,
        payer: &MonoTagged<Chain, &Wallet>,
        receive_fee: &TaggedTokenRef<'_, Chain>,
        ack_fee: &TaggedTokenRef<'_, Chain>,
        timeout_fee: &TaggedTokenRef<'_, Chain>,
    ) -> Result<Vec<IbcEventWithHeight>, Error>;

    fn register_counterparty_payee<Counterparty>(
        &self,
        wallet: &MonoTagged<Chain, &Wallet>,
        counterparty_payee: &MonoTagged<Counterparty, &WalletAddress>,
        channel_id: &TaggedChannelIdRef<'_, Chain, Counterparty>,
        port_id: &TaggedPortIdRef<'_, Chain, Counterparty>,
    ) -> Result<(), Error>;

    fn register_payee<Counterparty>(
        &self,
        wallet: &MonoTagged<Chain, &Wallet>,
        payee: &MonoTagged<Chain, &WalletAddress>,
        channel_id: &TaggedChannelIdRef<'_, Chain, Counterparty>,
        port_id: &TaggedPortIdRef<'_, Chain, Counterparty>,
    ) -> Result<(), Error>;

    fn query_counterparty_payee<Counterparty>(
        &self,
        channel_id: &TaggedChannelIdRef<'_, Chain, Counterparty>,
        address: &MonoTagged<Chain, &WalletAddress>,
    ) -> Result<Option<MonoTagged<Counterparty, WalletAddress>>, Error>;

    fn query_incentivized_packets<Counterparty>(
        &self,
        channel_id: &TaggedChannelIdRef<'_, Chain, Counterparty>,
        port_id: &TaggedPortIdRef<'_, Chain, Counterparty>,
    ) -> Result<Vec<IdentifiedPacketFees>, Error>;
}

impl<'a, Chain: ChainHandle + Send> ChainFeeMethodsExt<Chain>
    for MonoTagged<Chain, &'a ChainDriver>
{
    fn ibc_token_transfer_with_fee<Counterparty: ChainHandle>(
        &self,
        channel: &ConnectedChannel<Chain, Counterparty>,
        sender: &MonoTagged<Chain, &Wallet>,
        recipient: &MonoTagged<Counterparty, &WalletAddress>,
        send_amount: &Vec<TaggedTokenRef<'_, Chain>>,
        receive_fee: &TaggedTokenRef<'_, Chain>,
        ack_fee: &TaggedTokenRef<'_, Chain>,
        timeout_fee: &TaggedTokenRef<'_, Chain>,
        timeout: Duration,
    ) -> Result<Vec<IbcEventWithHeight>, Error> {
        let port_id = channel.port_a.clone();
        let channel_id = channel.channel_id_a.clone();
        let channel_version = channel.channel.dst_version().ok_or_else(|| {
            Error::generic(eyre!(
                "failed to retrieve channel version for channel `{:#?}`",
                channel.channel.dst_channel_id()
            ))
        })?;

        let rpc_client = self.rpc_client()?;
        self.value().runtime.block_on(ibc_token_transfer_with_fee(
            rpc_client.as_ref(),
            &self.tx_config(),
            &port_id.as_ref(),
            &channel_id.as_ref(),
            channel_version,
            sender,
            recipient,
            send_amount,
            receive_fee,
            ack_fee,
            timeout_fee,
            timeout,
        ))
    }

    fn pay_packet_fee<Counterparty>(
        &self,
        port_id: &TaggedPortIdRef<'_, Chain, Counterparty>,
        channel_id: &TaggedChannelIdRef<'_, Chain, Counterparty>,
        sequence: &DualTagged<Chain, Counterparty, Sequence>,
        payer: &MonoTagged<Chain, &Wallet>,
        receive_fee: &TaggedTokenRef<'_, Chain>,
        ack_fee: &TaggedTokenRef<'_, Chain>,
        timeout_fee: &TaggedTokenRef<'_, Chain>,
    ) -> Result<Vec<IbcEventWithHeight>, Error> {
        let rpc_client = self.rpc_client()?;
        self.value().runtime.block_on(pay_packet_fee(
            rpc_client.as_ref(),
            &self.tx_config(),
            port_id,
            channel_id,
            sequence,
            payer,
            receive_fee,
            ack_fee,
            timeout_fee,
        ))
    }

    fn register_counterparty_payee<Counterparty>(
        &self,
        wallet: &MonoTagged<Chain, &Wallet>,
        counterparty_payee: &MonoTagged<Counterparty, &WalletAddress>,
        channel_id: &TaggedChannelIdRef<'_, Chain, Counterparty>,
        port_id: &TaggedPortIdRef<'_, Chain, Counterparty>,
    ) -> Result<(), Error> {
        let rpc_client = self.rpc_client()?;
        self.value().runtime.block_on(register_counterparty_payee(
            rpc_client.as_ref(),
            &self.tx_config(),
            wallet,
            counterparty_payee,
            channel_id,
            port_id,
        ))
    }

    fn register_payee<Counterparty>(
        &self,
        wallet: &MonoTagged<Chain, &Wallet>,
        payee: &MonoTagged<Chain, &WalletAddress>,
        channel_id: &TaggedChannelIdRef<'_, Chain, Counterparty>,
        port_id: &TaggedPortIdRef<'_, Chain, Counterparty>,
    ) -> Result<(), Error> {
        let rpc_client = self.rpc_client()?;
        self.value().runtime.block_on(register_payee(
            rpc_client.as_ref(),
            &self.tx_config(),
            wallet,
            payee,
            channel_id,
            port_id,
        ))
    }

    fn query_counterparty_payee<Counterparty>(
        &self,
        channel_id: &TaggedChannelIdRef<'_, Chain, Counterparty>,
        address: &MonoTagged<Chain, &WalletAddress>,
    ) -> Result<Option<MonoTagged<Counterparty, WalletAddress>>, Error> {
        self.value().runtime.block_on(query_counterparty_payee(
            &self.tx_config().value().grpc_address,
            channel_id,
            address,
        ))
    }

    fn query_incentivized_packets<Counterparty>(
        &self,
        channel_id: &TaggedChannelIdRef<'_, Chain, Counterparty>,
        port_id: &TaggedPortIdRef<'_, Chain, Counterparty>,
    ) -> Result<Vec<IdentifiedPacketFees>, Error> {
        self.value().runtime.block_on(query_incentivized_packets(
            &self.tx_config().value().grpc_address,
            channel_id,
            port_id,
        ))
    }
}
