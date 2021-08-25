//! Chain upgrade plans for triggering IBC-breaking upgrades.
#![allow(deprecated)] // TODO(hu55a1n1): remove this when we don't need legacy upgrade support

use core::time::Duration;

use bytes::BufMut;
use flex_error::define_error;
use prost_types::Any;

use ibc::ics02_client::client_state::AnyClientState;
use ibc::ics02_client::height::Height;
use ibc::ics24_host::identifier::{ChainId, ClientId};
use ibc::{events::IbcEvent, ics07_tendermint::client_state::ClientState};
use ibc_proto::cosmos::gov::v1beta1::MsgSubmitProposal;
use ibc_proto::cosmos::upgrade::v1beta1::{Plan, SoftwareUpgradeProposal};
use ibc_proto::ibc::core::client::v1::UpgradeProposal;

use crate::chain::{ChainEndpoint, CosmosSdkChain};
use crate::config::ChainConfig;
use crate::error::Error;

define_error! {
    UpgradeChainError {
        Query
            [ Error ]
            |_| { "error during a query" },

        Key
            [ Error ]
            |_| { "key error" },

        Submit
            { chain_id: ChainId }
            [ Error ]
            |e| {
                format!("failed while submitting the Transfer message to chain {0}",
                    e.chain_id)
            },

        TxResponse
            { event: String }
            |e| {
                format!("tx response event consists of an error: {}",
                    e.event)
            },

    }
}

#[derive(Clone, Debug)]
pub struct UpgradePlanOptions {
    pub src_chain_config: ChainConfig,
    pub dst_chain_config: ChainConfig,
    pub src_client_id: ClientId,
    pub amount: u64,
    pub height_offset: u64,
    pub upgraded_chain_id: ChainId,
    pub upgraded_unbonding_period: Option<Duration>,
    pub upgrade_plan_name: String,
    pub legacy: bool,
}

pub fn build_and_send_ibc_upgrade_proposal(
    mut dst_chain: CosmosSdkChain, // the chain which will undergo an upgrade
    src_chain: CosmosSdkChain, // the source chain; supplies a client state for building the upgrade plan
    opts: &UpgradePlanOptions,
) -> Result<Vec<IbcEvent>, UpgradeChainError> {
    let upgrade_height = dst_chain
        .query_latest_height()
        .map_err(UpgradeChainError::query)?
        .add(opts.height_offset);

    let client_state = src_chain
        .query_client_state(&opts.src_client_id, Height::zero())
        .map_err(UpgradeChainError::query)?;

    // Retain the old unbonding period in case the user did not specify a new one
    let upgraded_unbonding_period = opts
        .upgraded_unbonding_period
        .unwrap_or(client_state.unbonding_period);

    let mut upgraded_client_state = ClientState::zero_custom_fields(client_state);
    upgraded_client_state.latest_height = upgrade_height.increment();
    upgraded_client_state.unbonding_period = upgraded_unbonding_period;
    upgraded_client_state.chain_id = opts.upgraded_chain_id.clone();

    let raw_client_state = AnyClientState::Tendermint(upgraded_client_state);
    let proposal = UpgradeProposal {
        title: "proposal 0".to_string(),
        description: "upgrade the chain software and unbonding period".to_string(),
        upgraded_client_state: Some(Any::from(raw_client_state)),
        plan: Some(Plan {
            name: opts.upgrade_plan_name.clone(),
            height: upgrade_height.revision_height as i64,
            info: "".to_string(),
            ..Default::default() // deprecated fields - time & upgraded_client_state
        }),
    };

    let proposal = if opts.legacy {
        Proposal::Legacy(proposal.into())
    } else {
        Proposal::Default(proposal)
    };

    let mut buf_proposal = Vec::new();
    proposal.encode(&mut buf_proposal);
    let any_proposal = Any {
        type_url: proposal.type_url(),
        value: buf_proposal,
    };

    // build the msg submit proposal
    let proposer = dst_chain.get_signer().map_err(UpgradeChainError::key)?;

    let coins = ibc_proto::cosmos::base::v1beta1::Coin {
        denom: "stake".to_string(),
        amount: opts.amount.to_string(),
    };

    let msg = MsgSubmitProposal {
        content: Some(any_proposal),
        initial_deposit: vec![coins],
        proposer: proposer.to_string(),
    };

    let mut buf_msg = Vec::new();
    prost::Message::encode(&msg, &mut buf_msg).unwrap();
    let any_msg = Any {
        type_url: "/cosmos.gov.v1beta1.MsgSubmitProposal".to_string(),
        value: buf_msg,
    };

    let events = dst_chain
        .send_messages_and_wait_commit(vec![any_msg])
        .map_err(|e| UpgradeChainError::submit(dst_chain.id().clone(), e))?;

    // Check if the chain rejected the transaction
    let result = events.iter().find_map(|event| match event {
        IbcEvent::ChainError(reason) => Some(reason.clone()),
        _ => None,
    });

    match result {
        None => Ok(events),
        Some(reason) => Err(UpgradeChainError::tx_response(reason)),
    }
}

enum Proposal {
    Default(UpgradeProposal),
    Legacy(LegacyProposal),
}

impl Proposal {
    fn encode(&self, buf: &mut impl BufMut) {
        match self {
            Proposal::Default(p) => prost::Message::encode(p, buf),
            Proposal::Legacy(p) => prost::Message::encode(&p.0, buf),
        }
        .unwrap()
    }

    fn type_url(&self) -> String {
        match self {
            Proposal::Default(_) => "/ibc.core.client.v1.UpgradeProposal",
            Proposal::Legacy(_) => "/cosmos.upgrade.v1beta1.SoftwareUpgradeProposal",
        }
        .to_owned()
    }
}

struct LegacyProposal(SoftwareUpgradeProposal);

impl From<UpgradeProposal> for LegacyProposal {
    fn from(v: UpgradeProposal) -> Self {
        let plan = {
            if let Some(plan) = v.plan {
                Some(Plan {
                    name: plan.name,
                    height: plan.height,
                    info: plan.info,
                    time: None,
                    upgraded_client_state: v.upgraded_client_state,
                })
            } else {
                None
            }
        };
        Self(SoftwareUpgradeProposal {
            title: v.title,
            description: v.description,
            plan,
        })
    }
}
