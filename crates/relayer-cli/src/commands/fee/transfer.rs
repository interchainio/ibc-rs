use core::time::Duration;

use abscissa_core::{clap::Parser, config::Override, FrameworkError, FrameworkErrorKind};
use eyre::eyre;

use ibc_relayer::{
    chain::handle::ChainHandle,
    config::Config,
    transfer::{build_transfer_messages, send_messages, TransferOptions},
};
use ibc_relayer_types::{
    applications::{
        ics29_fee::msgs::pay_packet::build_pay_packet_message,
        transfer::{Amount, Coin},
    },
    core::ics24_host::identifier::{ChainId, ChannelId, PortId},
};

use crate::{
    cli_utils::{check_can_send_on_channel, ChainHandlePair},
    conclude::{exit_with_unrecoverable_error, Output},
    prelude::*,
};

#[derive(Clone, Command, Debug, Parser, PartialEq, Eq)]
pub struct FeeTransferCmd {
    #[clap(
        long = "dst-chain",
        required = true,
        value_name = "DST_CHAIN_ID",
        help_heading = "FLAGS",
        help = "Identifier of the destination chain"
    )]
    dst_chain_id: ChainId,

    #[clap(
        long = "src-chain",
        required = true,
        value_name = "SRC_CHAIN_ID",
        help_heading = "FLAGS",
        help = "Identifier of the source chain"
    )]
    src_chain_id: ChainId,

    #[clap(
        long = "src-port",
        required = true,
        value_name = "SRC_PORT_ID",
        help_heading = "FLAGS",
        help = "Identifier of the source port"
    )]
    src_port_id: PortId,

    #[clap(
        long = "src-channel",
        visible_alias = "src-chan",
        required = true,
        value_name = "SRC_CHANNEL_ID",
        help_heading = "FLAGS",
        help = "Identifier of the source channel"
    )]
    src_channel_id: ChannelId,

    #[clap(
        long = "amount",
        required = true,
        value_name = "AMOUNT",
        help_heading = "FLAGS",
        help = "Amount of coins (samoleans, by default) to send (e.g. `100000`)"
    )]
    amount: Amount,

    #[clap(
        long = "denom",
        value_name = "DENOM",
        help = "Denomination of the coins to send. Default: samoleans",
        default_value = "samoleans"
    )]
    denom: String,

    #[clap(
        long = "recipient",
        value_name = "RECIPIENT",
        help = "The account address on the destination chain which will receive the tokens. If omitted, the relayer's wallet on the destination chain will be used"
    )]
    recipient: Option<String>,

    #[clap(
        long = "number-msgs",
        value_name = "NUMBER_MSGS",
        help = "Number of messages to send"
    )]
    number_msgs: Option<usize>,

    #[clap(
        long = "key-name",
        value_name = "KEY_NAME",
        help = "Use the given signing key name (default: `key_name` config)"
    )]
    key_name: Option<String>,

    #[clap(
        long = "timeout-height-offset",
        default_value = "0",
        value_name = "TIMEOUT_HEIGHT_OFFSET",
        help = "Timeout in number of blocks since current. Default: 0"
    )]
    timeout_height_offset: u64,

    #[clap(
        long = "timeout-seconds",
        default_value = "0",
        value_name = "TIMEOUT_SECONDS",
        help = "Timeout in seconds since current. Default: 0"
    )]
    timeout_seconds: u64,

    #[clap(
        long = "receive-fee",
        value_name = "RECEIVE_FEE",
        help = "Fee to pay for the Recv message. Default: 0",
        default_value = "0"
    )]
    receive_fee: Amount,

    #[clap(
        long = "ack-fee",
        value_name = "ACK_FEE",
        help = "Fee to pay for the Ack message. Default: 0",
        default_value = "0"
    )]
    ack_fee: Amount,

    #[clap(
        long = "timeout-fee",
        value_name = "TIMEOUT_FEE",
        help = "Fee to pay for the Timeout message. Default: 0",
        default_value = "0"
    )]
    timeout_fee: Amount,

    #[clap(
        long = "memo",
        value_name = "MEMO",
        help = "Optional memo included in the transfer"
    )]
    memo: Option<String>,
}

impl Override<Config> for FeeTransferCmd {
    fn override_config(&self, mut config: Config) -> Result<Config, FrameworkError> {
        let src_chain_config = config.find_chain_mut(&self.src_chain_id).ok_or_else(|| {
            FrameworkErrorKind::ComponentError.context(format!(
                "missing configuration for source chain '{}'",
                self.src_chain_id
            ))
        })?;

        if let Some(ref key_name) = self.key_name {
            src_chain_config.set_key_name(key_name.to_string());
        }

        Ok(config)
    }
}

impl FeeTransferCmd {
    fn validate_options(&self, config: &Config) -> eyre::Result<FeeTransferOptions> {
        config.find_chain(&self.src_chain_id).ok_or_else(|| {
            eyre!(
                "missing configuration for source chain '{}'",
                self.src_chain_id
            )
        })?;

        config.find_chain(&self.dst_chain_id).ok_or_else(|| {
            eyre!(
                "missing configuration for destination chain '{}'",
                self.dst_chain_id
            )
        })?;

        let denom = self.denom.clone();

        let number_msgs = self.number_msgs.unwrap_or(1);
        if number_msgs == 0 {
            return Err(eyre!("number of messages should be greater than zero"));
        }

        let opts = FeeTransferOptions {
            src_chain_id: self.src_chain_id.clone(),
            dst_chain_id: self.dst_chain_id.clone(),
            src_port_id: self.src_port_id.clone(),
            src_channel_id: self.src_channel_id.clone(),
            amount: self.amount,
            denom,
            receiver: self.recipient.clone(),
            timeout_height_offset: self.timeout_height_offset,
            timeout_duration: Duration::from_secs(self.timeout_seconds),
            number_msgs,
            receive_fee: self.receive_fee,
            ack_fee: self.ack_fee,
            timeout_fee: self.timeout_fee,
            memo: self.memo.clone(),
        };

        Ok(opts)
    }
}

impl Runnable for FeeTransferCmd {
    fn run(&self) {
        let config = app_config();

        let opts = match self.validate_options(&config) {
            Err(err) => Output::error(err).exit(),
            Ok(result) => result,
        };

        let chains = ChainHandlePair::spawn(&config, &self.src_chain_id, &self.dst_chain_id)
            .unwrap_or_else(exit_with_unrecoverable_error);

        fee_transfer(chains, opts).unwrap_or_else(exit_with_unrecoverable_error);
    }
}

#[derive(Clone, Debug)]
pub struct FeeTransferOptions {
    pub src_chain_id: ChainId,
    pub dst_chain_id: ChainId,
    pub src_port_id: PortId,
    pub src_channel_id: ChannelId,
    pub amount: Amount,
    pub denom: String,
    pub receiver: Option<String>,
    pub timeout_height_offset: u64,
    pub timeout_duration: Duration,
    pub number_msgs: usize,
    pub receive_fee: Amount,
    pub ack_fee: Amount,
    pub timeout_fee: Amount,
    pub memo: Option<String>,
}

impl From<FeeTransferOptions> for TransferOptions {
    fn from(f: FeeTransferOptions) -> Self {
        let tokens = vec![(f.amount, f.denom)];
        TransferOptions {
            src_port_id: f.src_port_id,
            src_channel_id: f.src_channel_id,
            tokens,
            receiver: f.receiver,
            timeout_height_offset: f.timeout_height_offset,
            timeout_duration: f.timeout_duration,
            number_msgs: f.number_msgs,
            memo: f.memo,
        }
    }
}

fn fee_transfer(chains: ChainHandlePair, opts: FeeTransferOptions) -> Result<(), eyre::Report> {
    check_can_send_on_channel(
        &chains.src,
        &opts.src_channel_id,
        &opts.src_port_id,
        &opts.dst_chain_id,
    )?;

    let sender = chains.src.get_signer()?;
    let receive_fee = Coin::new(opts.denom.clone(), opts.receive_fee);
    let ack_fee = Coin::new(opts.denom.clone(), opts.ack_fee);
    let timeout_fee = Coin::new(opts.denom.clone(), opts.timeout_fee);

    let pay_message = build_pay_packet_message(
        &opts.src_port_id,
        &opts.src_channel_id,
        &sender,
        vec![receive_fee],
        vec![ack_fee],
        vec![timeout_fee],
    )?;

    // Recommended by ICS29 https://github.com/cosmos/ibc/blob/main/spec/app/ics-029-fee-payment/README.md:
    // "it is recommended that fee middleware require their messages to be placed before the send packet message and escrow
    // fees for the next sequence on the given channel."
    // In addition, a fee message is sent for every transfer message.
    let mut msgs = vec![];

    let transfer = build_transfer_messages(&chains.src, &chains.dst, &opts.into())?;
    for t in transfer {
        msgs.push(pay_message.clone());
        msgs.push(t);
    }

    let res = send_messages(&chains.src, msgs);

    match res {
        Ok(ev) => Output::success(ev).exit(),
        Err(e) => Output::error(e).exit(),
    }
}

#[cfg(test)]
mod tests {

    use super::FeeTransferCmd;

    use abscissa_core::clap::Parser;
    use std::str::FromStr;

    use ibc_relayer_types::{
        applications::transfer::Amount,
        core::ics24_host::identifier::{ChainId, ChannelId, PortId},
    };

    #[test]
    fn test_fee_transfer_required_only() {
        assert_eq!(
            FeeTransferCmd {
                dst_chain_id: ChainId::from_string("chain_b"),
                src_chain_id: ChainId::from_string("chain_a"),
                src_port_id: PortId::from_str("port_a").unwrap(),
                src_channel_id: ChannelId::from_str("channel_a").unwrap(),
                amount: Amount::from(1000u64),
                denom: "samoleans".to_owned(),
                recipient: None,
                number_msgs: None,
                key_name: None,
                timeout_height_offset: 0,
                timeout_seconds: 0,
                receive_fee: Amount::from(0u64),
                ack_fee: Amount::from(0u64),
                timeout_fee: Amount::from(0u64),
                memo: None,
            },
            FeeTransferCmd::parse_from([
                "test",
                "--dst-chain",
                "chain_b",
                "--src-chain",
                "chain_a",
                "--src-port",
                "port_a",
                "--src-channel",
                "channel_a",
                "--amount",
                "1000"
            ])
        )
    }

    #[test]
    fn test_fee_transfer_channel_alias() {
        assert_eq!(
            FeeTransferCmd {
                dst_chain_id: ChainId::from_string("chain_b"),
                src_chain_id: ChainId::from_string("chain_a"),
                src_port_id: PortId::from_str("port_a").unwrap(),
                src_channel_id: ChannelId::from_str("channel_a").unwrap(),
                amount: Amount::from(1000u64),
                denom: "samoleans".to_owned(),
                recipient: None,
                number_msgs: None,
                key_name: None,
                timeout_height_offset: 0,
                timeout_seconds: 0,
                receive_fee: Amount::from(0u64),
                ack_fee: Amount::from(0u64),
                timeout_fee: Amount::from(0u64),
                memo: None,
            },
            FeeTransferCmd::parse_from([
                "test",
                "--dst-chain",
                "chain_b",
                "--src-chain",
                "chain_a",
                "--src-port",
                "port_a",
                "--src-chan",
                "channel_a",
                "--amount",
                "1000"
            ])
        )
    }

    #[test]
    fn test_fee_transfer_denom() {
        assert_eq!(
            FeeTransferCmd {
                dst_chain_id: ChainId::from_string("chain_b"),
                src_chain_id: ChainId::from_string("chain_a"),
                src_port_id: PortId::from_str("port_a").unwrap(),
                src_channel_id: ChannelId::from_str("channel_a").unwrap(),
                amount: Amount::from(1000u64),
                denom: "stake".to_owned(),
                recipient: None,
                number_msgs: None,
                key_name: None,
                timeout_height_offset: 0,
                timeout_seconds: 0,
                receive_fee: Amount::from(0u64),
                ack_fee: Amount::from(0u64),
                timeout_fee: Amount::from(0u64),
                memo: None,
            },
            FeeTransferCmd::parse_from([
                "test",
                "--dst-chain",
                "chain_b",
                "--src-chain",
                "chain_a",
                "--src-port",
                "port_a",
                "--src-channel",
                "channel_a",
                "--amount",
                "1000",
                "--denom",
                "stake"
            ])
        )
    }

    #[test]
    fn test_fee_transfer_recipient() {
        assert_eq!(
            FeeTransferCmd {
                dst_chain_id: ChainId::from_string("chain_b"),
                src_chain_id: ChainId::from_string("chain_a"),
                src_port_id: PortId::from_str("port_a").unwrap(),
                src_channel_id: ChannelId::from_str("channel_a").unwrap(),
                amount: Amount::from(1000u64),
                denom: "samoleans".to_owned(),
                recipient: Some("other_recipient".to_owned()),
                number_msgs: None,
                key_name: None,
                timeout_height_offset: 0,
                timeout_seconds: 0,
                receive_fee: Amount::from(0u64),
                ack_fee: Amount::from(0u64),
                timeout_fee: Amount::from(0u64),
                memo: None,
            },
            FeeTransferCmd::parse_from([
                "test",
                "--dst-chain",
                "chain_b",
                "--src-chain",
                "chain_a",
                "--src-port",
                "port_a",
                "--src-channel",
                "channel_a",
                "--amount",
                "1000",
                "--recipient",
                "other_recipient"
            ])
        )
    }

    #[test]
    fn test_fee_transfer_number_msgs() {
        assert_eq!(
            FeeTransferCmd {
                dst_chain_id: ChainId::from_string("chain_b"),
                src_chain_id: ChainId::from_string("chain_a"),
                src_port_id: PortId::from_str("port_a").unwrap(),
                src_channel_id: ChannelId::from_str("channel_a").unwrap(),
                amount: Amount::from(1000u64),
                denom: "samoleans".to_owned(),
                recipient: None,
                number_msgs: Some(10),
                key_name: None,
                timeout_height_offset: 0,
                timeout_seconds: 0,
                receive_fee: Amount::from(0u64),
                ack_fee: Amount::from(0u64),
                timeout_fee: Amount::from(0u64),
                memo: None,
            },
            FeeTransferCmd::parse_from([
                "test",
                "--dst-chain",
                "chain_b",
                "--src-chain",
                "chain_a",
                "--src-port",
                "port_a",
                "--src-channel",
                "channel_a",
                "--amount",
                "1000",
                "--number-msgs",
                "10"
            ])
        )
    }

    #[test]
    fn test_fee_transfer_key_name() {
        assert_eq!(
            FeeTransferCmd {
                dst_chain_id: ChainId::from_string("chain_b"),
                src_chain_id: ChainId::from_string("chain_a"),
                src_port_id: PortId::from_str("port_a").unwrap(),
                src_channel_id: ChannelId::from_str("channel_a").unwrap(),
                amount: Amount::from(1000u64),
                denom: "samoleans".to_owned(),
                recipient: None,
                number_msgs: None,
                key_name: Some("other_wallet".to_owned()),
                timeout_height_offset: 0,
                timeout_seconds: 0,
                receive_fee: Amount::from(0u64),
                ack_fee: Amount::from(0u64),
                timeout_fee: Amount::from(0u64),
                memo: None,
            },
            FeeTransferCmd::parse_from([
                "test",
                "--dst-chain",
                "chain_b",
                "--src-chain",
                "chain_a",
                "--src-port",
                "port_a",
                "--src-channel",
                "channel_a",
                "--amount",
                "1000",
                "--key-name",
                "other_wallet"
            ])
        )
    }

    #[test]
    fn test_fee_transfer_timeout_heightoffset() {
        assert_eq!(
            FeeTransferCmd {
                dst_chain_id: ChainId::from_string("chain_b"),
                src_chain_id: ChainId::from_string("chain_a"),
                src_port_id: PortId::from_str("port_a").unwrap(),
                src_channel_id: ChannelId::from_str("channel_a").unwrap(),
                amount: Amount::from(1000u64),
                denom: "samoleans".to_owned(),
                recipient: None,
                number_msgs: None,
                key_name: None,
                timeout_height_offset: 42,
                timeout_seconds: 0,
                receive_fee: Amount::from(0u64),
                ack_fee: Amount::from(0u64),
                timeout_fee: Amount::from(0u64),
                memo: None,
            },
            FeeTransferCmd::parse_from([
                "test",
                "--dst-chain",
                "chain_b",
                "--src-chain",
                "chain_a",
                "--src-port",
                "port_a",
                "--src-channel",
                "channel_a",
                "--amount",
                "1000",
                "--timeout-height-offset",
                "42"
            ])
        )
    }

    #[test]
    fn test_fee_transfer_timeout_seconds() {
        assert_eq!(
            FeeTransferCmd {
                dst_chain_id: ChainId::from_string("chain_b"),
                src_chain_id: ChainId::from_string("chain_a"),
                src_port_id: PortId::from_str("port_a").unwrap(),
                src_channel_id: ChannelId::from_str("channel_a").unwrap(),
                amount: Amount::from(1000u64),
                denom: "samoleans".to_owned(),
                recipient: None,
                number_msgs: None,
                key_name: None,
                timeout_height_offset: 0,
                timeout_seconds: 21,
                receive_fee: Amount::from(0u64),
                ack_fee: Amount::from(0u64),
                timeout_fee: Amount::from(0u64),
                memo: None,
            },
            FeeTransferCmd::parse_from([
                "test",
                "--dst-chain",
                "chain_b",
                "--src-chain",
                "chain_a",
                "--src-port",
                "port_a",
                "--src-channel",
                "channel_a",
                "--amount",
                "1000",
                "--timeout-seconds",
                "21"
            ])
        )
    }

    #[test]
    fn test_fee_transfer_receive_fee() {
        assert_eq!(
            FeeTransferCmd {
                dst_chain_id: ChainId::from_string("chain_b"),
                src_chain_id: ChainId::from_string("chain_a"),
                src_port_id: PortId::from_str("port_a").unwrap(),
                src_channel_id: ChannelId::from_str("channel_a").unwrap(),
                amount: Amount::from(1000u64),
                denom: "samoleans".to_owned(),
                recipient: None,
                number_msgs: None,
                key_name: None,
                timeout_height_offset: 0,
                timeout_seconds: 0,
                receive_fee: Amount::from(51u64),
                ack_fee: Amount::from(0u64),
                timeout_fee: Amount::from(0u64),
                memo: None,
            },
            FeeTransferCmd::parse_from([
                "test",
                "--dst-chain",
                "chain_b",
                "--src-chain",
                "chain_a",
                "--src-port",
                "port_a",
                "--src-channel",
                "channel_a",
                "--amount",
                "1000",
                "--receive-fee",
                "51"
            ])
        )
    }
    #[test]
    fn test_fee_transfer_ack_fee() {
        assert_eq!(
            FeeTransferCmd {
                dst_chain_id: ChainId::from_string("chain_b"),
                src_chain_id: ChainId::from_string("chain_a"),
                src_port_id: PortId::from_str("port_a").unwrap(),
                src_channel_id: ChannelId::from_str("channel_a").unwrap(),
                amount: Amount::from(1000u64),
                denom: "samoleans".to_owned(),
                recipient: None,
                number_msgs: None,
                key_name: None,
                timeout_height_offset: 0,
                timeout_seconds: 0,
                receive_fee: Amount::from(0u64),
                ack_fee: Amount::from(52u64),
                timeout_fee: Amount::from(0u64),
                memo: None,
            },
            FeeTransferCmd::parse_from([
                "test",
                "--dst-chain",
                "chain_b",
                "--src-chain",
                "chain_a",
                "--src-port",
                "port_a",
                "--src-channel",
                "channel_a",
                "--amount",
                "1000",
                "--ack-fee",
                "52"
            ])
        )
    }

    #[test]
    fn test_fee_transfer_timeout_fee() {
        assert_eq!(
            FeeTransferCmd {
                dst_chain_id: ChainId::from_string("chain_b"),
                src_chain_id: ChainId::from_string("chain_a"),
                src_port_id: PortId::from_str("port_a").unwrap(),
                src_channel_id: ChannelId::from_str("channel_a").unwrap(),
                amount: Amount::from(1000u64),
                denom: "samoleans".to_owned(),
                recipient: None,
                number_msgs: None,
                key_name: None,
                timeout_height_offset: 0,
                timeout_seconds: 0,
                receive_fee: Amount::from(0u64),
                ack_fee: Amount::from(0u64),
                timeout_fee: Amount::from(53u64),
                memo: None,
            },
            FeeTransferCmd::parse_from([
                "test",
                "--dst-chain",
                "chain_b",
                "--src-chain",
                "chain_a",
                "--src-port",
                "port_a",
                "--src-channel",
                "channel_a",
                "--amount",
                "1000",
                "--timeout-fee",
                "53"
            ])
        )
    }

    #[test]
    fn test_fee_transfer_memo() {
        assert_eq!(
            FeeTransferCmd {
                dst_chain_id: ChainId::from_string("chain_b"),
                src_chain_id: ChainId::from_string("chain_a"),
                src_port_id: PortId::from_str("port_a").unwrap(),
                src_channel_id: ChannelId::from_str("channel_a").unwrap(),
                amount: Amount::from(1000u64),
                denom: "samoleans".to_owned(),
                recipient: None,
                number_msgs: None,
                key_name: None,
                timeout_height_offset: 0,
                timeout_seconds: 0,
                receive_fee: Amount::from(0u64),
                ack_fee: Amount::from(0u64),
                timeout_fee: Amount::from(0u64),
                memo: Some("test memo".to_owned()),
            },
            FeeTransferCmd::parse_from([
                "test",
                "--dst-chain",
                "chain_b",
                "--src-chain",
                "chain_a",
                "--src-port",
                "port_a",
                "--src-channel",
                "channel_a",
                "--amount",
                "1000",
                "--memo",
                "test memo"
            ])
        )
    }

    #[test]
    fn test_fee_transfer_no_amount() {
        assert!(FeeTransferCmd::try_parse_from([
            "test",
            "--dst-chain",
            "chain_b",
            "--src-chain",
            "chain_a",
            "--src-port",
            "port_a",
            "--src-channel",
            "channel_a"
        ])
        .is_err())
    }

    #[test]
    fn test_fee_transfer_no_src_channel() {
        assert!(FeeTransferCmd::try_parse_from([
            "test",
            "--dst-chain",
            "chain_b",
            "--src-chain",
            "chain_a",
            "--src-port",
            "port_a",
            "--amount",
            "1000"
        ])
        .is_err())
    }

    #[test]
    fn test_fee_transfer_no_src_port() {
        assert!(FeeTransferCmd::try_parse_from([
            "test",
            "--dst-chain",
            "chain_b",
            "--src-chain",
            "chain_a",
            "--src-channel",
            "channel_a",
            "--amount",
            "1000"
        ])
        .is_err())
    }

    #[test]
    fn test_fee_transfer_no_src_chain() {
        assert!(FeeTransferCmd::try_parse_from([
            "test",
            "--dst-chain",
            "chain_b",
            "--src-port",
            "port_a",
            "--src-channel",
            "channel_a",
            "--amount",
            "1000"
        ])
        .is_err())
    }

    #[test]
    fn test_fee_transfer_no_dst_chain() {
        assert!(FeeTransferCmd::try_parse_from([
            "test",
            "--src-chain",
            "chain_a",
            "--src-port",
            "port_a",
            "--src-channel",
            "channel_a",
            "--amount",
            "1000"
        ])
        .is_err())
    }
}
