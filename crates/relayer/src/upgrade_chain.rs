//! Chain upgrade plans for triggering IBC-breaking upgrades.

use core::time::Duration;
use std::ops::Add;

use bytes::BufMut;
use flex_error::define_error;

use tendermint::Hash as TxHash;

use ibc_proto::cosmos::gov::v1beta1::MsgSubmitProposal;
use ibc_proto::cosmos::upgrade::v1beta1::Plan;
use ibc_proto::google::protobuf::Any;
use ibc_proto::ibc::core::client::v1::UpgradeProposal;
use ibc_relayer_types::clients::ics07_tendermint::client_state::UpgradeOptions;
use ibc_relayer_types::core::ics02_client::client_state::UpgradableClientState;
use ibc_relayer_types::core::ics24_host::identifier::{ChainId, ClientId};
use ibc_relayer_types::{downcast, Height};

use crate::chain::handle::ChainHandle;
use crate::chain::requests::{IncludeProof, QueryClientStateRequest, QueryHeight};
use crate::chain::tracking::TrackedMsgs;
use crate::client_state::AnyClientState;
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
                format!("failed while submitting the Transfer message to chain {0}", e.chain_id)
            },

        TxResponse
            { event: String }
            |e| {
                format!("tx response event consists of an error: {}", e.event)
            },

        TendermintOnly
            |_| { "only Tendermint clients can be upgraded" },

        UpgradeHeightRevision
            { revision: u64 }
            |r| {
                format!("invalid upgrade height revision: {r}")
            }
    }
}

#[derive(Clone, Debug)]
pub struct UpgradePlanOptions {
    pub src_client_id: ClientId,
    pub amount: u64,
    pub denom: String,
    pub height_offset: u64,
    pub upgraded_chain_id: ChainId,
    pub upgraded_unbonding_period: Option<Duration>,
    pub upgrade_plan_name: String,
}

pub fn build_and_send_ibc_upgrade_proposal(
    dst_chain: impl ChainHandle, // the chain which will undergo an upgrade
    src_chain: impl ChainHandle, // the source chain; supplies a client state for building the upgrade plan
    opts: &UpgradePlanOptions,
) -> Result<TxHash, UpgradeChainError> {
    let plan_height = dst_chain
        .query_latest_height() // FIXME(romac): Use query_chain_latest_height once added to ChainHandle
        .map_err(UpgradeChainError::query)?
        .add(opts.height_offset);

    let upgraded_client_latest_height =
        if dst_chain.id().version() == opts.upgraded_chain_id.version() {
            plan_height.increment()
        } else {
            Height::new(opts.upgraded_chain_id.version(), 1).map_err(|_| {
                UpgradeChainError::upgrade_height_revision(opts.upgraded_chain_id.version())
            })?
        };

    let (client_state, _) = src_chain
        .query_client_state(
            QueryClientStateRequest {
                client_id: opts.src_client_id.clone(),
                height: QueryHeight::Latest,
            },
            IncludeProof::No,
        )
        .map_err(UpgradeChainError::query)?;

    let mut client_state = downcast!(client_state => AnyClientState::Tendermint)
        .ok_or_else(UpgradeChainError::tendermint_only)?;

    // Retain the old unbonding period in case the user did not specify a new one
    let upgrade_options = UpgradeOptions {
        unbonding_period: opts
            .upgraded_unbonding_period
            .unwrap_or(client_state.unbonding_period),
    };

    client_state.upgrade(
        upgraded_client_latest_height,
        upgrade_options,
        opts.upgraded_chain_id.clone(),
    );

    let proposal = UpgradeProposal {
        title: "proposal 0".to_string(),
        description: "upgrade the chain software and unbonding period".to_string(),
        upgraded_client_state: Some(Any::from(AnyClientState::from(client_state))),
        plan: Some(Plan {
            name: opts.upgrade_plan_name.clone(),
            height: plan_height.revision_height() as i64,
            info: "".to_string(),
            ..Default::default() // deprecated fields - time & upgraded_client_state
        }),
    };

    let proposal = Proposal::Default(proposal);

    let mut buf_proposal = Vec::new();
    proposal.encode(&mut buf_proposal);
    let any_proposal = Any {
        type_url: proposal.type_url(),
        value: buf_proposal,
    };

    // build the msg submit proposal
    let proposer = dst_chain.get_signer().map_err(UpgradeChainError::key)?;

    let coins = ibc_proto::cosmos::base::v1beta1::Coin {
        denom: opts.denom.clone(),
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

    // Can't use send_messages_and_wait_commit because no IBC events
    // corresponding to the transaction can be recognized to confirm the
    // upgrade.
    // https://github.com/informalsystems/hermes/issues/1288#issuecomment-1066884163

    let responses = dst_chain
        .send_messages_and_wait_check_tx(TrackedMsgs::new_single(any_msg, "upgrade"))
        .map_err(|e| UpgradeChainError::submit(dst_chain.id(), e))?;

    Ok(responses[0].hash)
}

enum Proposal {
    Default(UpgradeProposal),
}

impl Proposal {
    fn encode(&self, buf: &mut impl BufMut) {
        match self {
            Proposal::Default(p) => prost::Message::encode(p, buf),
        }
        .unwrap()
    }

    fn type_url(&self) -> String {
        match self {
            Proposal::Default(_) => "/ibc.core.client.v1.UpgradeProposal",
        }
        .to_owned()
    }
}
