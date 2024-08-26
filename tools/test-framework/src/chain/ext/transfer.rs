use core::time::Duration;
use eyre::eyre;

use ibc_relayer::chain::handle::ChainHandle;
use ibc_relayer_types::core::ics02_client::height::Height;
use ibc_relayer_types::core::ics04_channel::packet::Packet;
use ibc_relayer_types::core::ics24_host::identifier::{ChannelId, PortId};

use crate::chain::cli::transfer::{local_transfer_token, transfer_from_chain};
use crate::chain::driver::ChainDriver;
use crate::chain::tagged::TaggedChainDriverExt;
use crate::error::Error;
use crate::ibc::token::TaggedTokenRef;
use crate::prelude::ConnectedChannel;
use crate::relayer::transfer::{batched_ibc_token_transfer, ibc_token_transfer};
use crate::types::tagged::*;
use crate::types::wallet::{Wallet, WalletAddress};

pub trait ChainTransferMethodsExt<Chain: ChainHandle> {
    /**
       Submits an IBC token transfer transaction to `Chain` to any other
       `Counterparty` chain.

       The following parameters are accepted:

       - A `PortId` on `Chain` that corresponds to a channel connected to
         `Counterparty`.

       - A `ChannelId` on `Chain` that corresponds to a channel connected to
         `Counterparty`.

       - The [`Wallet`] of the sender on `Chain`.

       - The [`WalletAddress`] address of the recipient on `Counterparty`.

       - The denomination of the amount on `Chain`.

       - The transfer amount.
    */
    fn ibc_transfer_token<Counterparty: ChainHandle>(
        &self,
        channel: &ConnectedChannel<Chain, Counterparty>,
        sender: &MonoTagged<Chain, &Wallet>,
        recipient: &MonoTagged<Counterparty, &WalletAddress>,
        token: &Vec<TaggedTokenRef<Chain>>,
    ) -> Result<Packet, Error>;

    fn ibc_transfer_token_with_memo_and_timeout<Counterparty: ChainHandle>(
        &self,
        channel: &ConnectedChannel<Chain, Counterparty>,
        sender: &MonoTagged<Chain, &Wallet>,
        recipient: &MonoTagged<Counterparty, &WalletAddress>,
        tokens: &Vec<TaggedTokenRef<Chain>>,
        memo: Option<String>,
        timeout: Option<Duration>,
    ) -> Result<Packet, Error>;

    fn ibc_transfer_token_multiple<Counterparty: ChainHandle>(
        &self,
        channel: &ConnectedChannel<Chain, Counterparty>,
        sender: &MonoTagged<Chain, &Wallet>,
        recipient: &MonoTagged<Counterparty, &WalletAddress>,
        tokens: &Vec<TaggedTokenRef<Chain>>,
        num_msgs: usize,
        memo: Option<String>,
    ) -> Result<(), Error>;

    fn local_transfer_token(
        &self,
        sender: &MonoTagged<Chain, &Wallet>,
        recipient: &MonoTagged<Chain, &WalletAddress>,
        token: &TaggedTokenRef<Chain>,
        fees: &TaggedTokenRef<Chain>,
    ) -> Result<(), Error>;

    fn transfer_from_chain<Counterparty>(
        &self,
        sender: &MonoTagged<Chain, &Wallet>,
        recipient: &MonoTagged<Counterparty, &WalletAddress>,
        port: &PortId,
        channel: &ChannelId,
        token: &TaggedTokenRef<Chain>,
        fees: &TaggedTokenRef<Chain>,
        timeout_height: &Height,
    ) -> Result<(), Error>;
}

impl<'a, Chain: ChainHandle + Send> ChainTransferMethodsExt<Chain>
    for MonoTagged<Chain, &'a ChainDriver>
{
    fn ibc_transfer_token<Counterparty: ChainHandle>(
        &self,
        channel: &ConnectedChannel<Chain, Counterparty>,
        sender: &MonoTagged<Chain, &Wallet>,
        recipient: &MonoTagged<Counterparty, &WalletAddress>,
        token: &Vec<TaggedTokenRef<Chain>>,
    ) -> Result<Packet, Error> {
        let port_id = channel.port_a.clone();
        let channel_id = channel.channel_id_a.clone();
        let channel_version = channel.channel.dst_version().ok_or_else(|| {
            Error::generic(eyre!(
                "failed to retrieve channel version for channel `{:#?}`",
                channel.channel.dst_channel_id()
            ))
        })?;

        let rpc_client = self.rpc_client()?;
        self.value().runtime.block_on(ibc_token_transfer(
            rpc_client.as_ref(),
            &self.tx_config(),
            &port_id.as_ref(),
            &channel_id.as_ref(),
            channel_version,
            sender,
            recipient,
            token,
            None,
            None,
        ))
    }

    fn ibc_transfer_token_with_memo_and_timeout<Counterparty: ChainHandle>(
        &self,
        channel: &ConnectedChannel<Chain, Counterparty>,
        sender: &MonoTagged<Chain, &Wallet>,
        recipient: &MonoTagged<Counterparty, &WalletAddress>,
        tokens: &Vec<TaggedTokenRef<Chain>>,
        memo: Option<String>,
        timeout: Option<Duration>,
    ) -> Result<Packet, Error> {
        let port_id = channel.port_a.clone();
        let channel_id = channel.channel_id_a.clone();
        let channel_version = channel.channel.dst_version().ok_or_else(|| {
            Error::generic(eyre!(
                "failed to retrieve channel version for channel `{:#?}`",
                channel.channel.dst_channel_id()
            ))
        })?;

        let rpc_client = self.rpc_client()?;
        self.value().runtime.block_on(ibc_token_transfer(
            rpc_client.as_ref(),
            &self.tx_config(),
            &port_id.as_ref(),
            &channel_id.as_ref(),
            channel_version,
            sender,
            recipient,
            tokens,
            memo,
            timeout,
        ))
    }

    fn ibc_transfer_token_multiple<Counterparty: ChainHandle>(
        &self,
        channel: &ConnectedChannel<Chain, Counterparty>,
        sender: &MonoTagged<Chain, &Wallet>,
        recipient: &MonoTagged<Counterparty, &WalletAddress>,
        tokens: &Vec<TaggedTokenRef<Chain>>,
        num_msgs: usize,
        memo: Option<String>,
    ) -> Result<(), Error> {
        let port_id = channel.port_a.clone();
        let channel_id = channel.channel_id_a.clone();
        let channel_version = channel.channel.dst_version().ok_or_else(|| {
            Error::generic(eyre!(
                "failed to retrieve channel version for channel `{:#?}`",
                channel.channel.dst_channel_id()
            ))
        })?;

        let rpc_client = self.rpc_client()?;
        self.value().runtime.block_on(batched_ibc_token_transfer(
            rpc_client.as_ref(),
            &self.tx_config(),
            &port_id.as_ref(),
            &channel_id.as_ref(),
            channel_version,
            sender,
            recipient,
            tokens,
            num_msgs,
            memo,
        ))
    }

    fn local_transfer_token(
        &self,
        sender: &MonoTagged<Chain, &Wallet>,
        recipient: &MonoTagged<Chain, &WalletAddress>,
        token: &TaggedTokenRef<Chain>,
        fees: &TaggedTokenRef<Chain>,
    ) -> Result<(), Error> {
        let driver = *self.value();
        local_transfer_token(
            driver.chain_id.as_str(),
            &driver.command_path,
            &driver.home_path,
            &driver.rpc_listen_address(),
            sender.value().address.as_str(),
            recipient.value().as_str(),
            &token.value().to_string(),
            &fees.value().to_string(),
        )
    }

    fn transfer_from_chain<Counterparty>(
        &self,
        sender: &MonoTagged<Chain, &Wallet>,
        recipient: &MonoTagged<Counterparty, &WalletAddress>,
        port: &PortId,
        channel: &ChannelId,
        token: &TaggedTokenRef<Chain>,
        fees: &TaggedTokenRef<Chain>,
        timeout_height: &Height,
    ) -> Result<(), Error> {
        let driver = *self.value();
        let timeout_height_str = timeout_height.revision_height() + 100;
        transfer_from_chain(
            driver.chain_id.as_str(),
            &driver.command_path,
            &driver.home_path,
            &driver.rpc_listen_address(),
            sender.value().address.as_str(),
            port.as_ref(),
            channel.as_ref(),
            recipient.value().as_str(),
            &token.value().to_string(),
            &fees.value().to_string(),
            &timeout_height_str.to_string(),
        )
    }
}
