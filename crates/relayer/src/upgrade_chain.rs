//! Chain upgrade plans for triggering IBC-breaking upgrades.

use core::time::Duration;
use std::ops::Add;

use bytes::BufMut;
use flex_error::define_error;

use tendermint::Hash as TxHash;

use ibc_proto::cosmos::gov::v1::MsgSubmitProposal;
use ibc_proto::cosmos::gov::v1beta1::MsgSubmitProposal as LegacyMsgSubmitProposal;
use ibc_proto::cosmos::upgrade::v1beta1::Plan;
use ibc_proto::google::protobuf::Any;
use ibc_proto::ibc::core::client::v1::MsgIbcSoftwareUpgrade;
use ibc_relayer_types::clients::ics07_tendermint::client_state::UpgradeOptions;
use ibc_relayer_types::core::ics02_client::client_state::UpgradableClientState;
use ibc_relayer_types::core::ics24_host::identifier::{ChainId, ClientId};
use ibc_relayer_types::{downcast, Height};
use tracing::warn;

use crate::chain::handle::ChainHandle;
use crate::chain::requests::{IncludeProof, QueryClientStateRequest, QueryHeight};
use crate::chain::tracking::TrackedMsgs;
use crate::client_state::AnyClientState;
use crate::error::Error;

/// Extracted `UpgradeProposal` since ibc-go removed it from
/// the proto definitions, https://github.com/cosmos/ibc-go/pull/6782
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct UpgradeProposal {
    #[prost(string, tag = "1")]
    pub title: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub description: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "3")]
    pub plan: ::core::option::Option<ibc_proto::cosmos::upgrade::v1beta1::Plan>,
    /// An UpgradedClientState must be provided to perform an IBC breaking upgrade.
    /// This will make the chain commit to the correct upgraded (self) client state
    /// before the upgrade occurs, so that connecting chains can verify that the
    /// new upgraded client is valid by verifying a proof on the previous version
    /// of the chain. This will allow IBC connections to persist smoothly across
    /// planned chain upgrades
    #[prost(message, optional, tag = "4")]
    pub upgraded_client_state: ::core::option::Option<ibc_proto::google::protobuf::Any>,
}

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
    pub gov_account: String,
}

pub fn build_and_send_ibc_upgrade_proposal(
    dst_chain: impl ChainHandle, // the chain which will undergo an upgrade
    src_chain: impl ChainHandle, // the source chain; supplies a client state for building the upgrade plan
    opts: &UpgradePlanOptions,
) -> Result<TxHash, UpgradeChainError> {
    let any_msg = if requires_legacy_upgrade_proposal(dst_chain.clone()) {
        build_legacy_upgrade_proposal(dst_chain.clone(), src_chain, opts)
    } else {
        build_upgrade_proposal(dst_chain.clone(), src_chain, opts)
    }?;

    // Can't use send_messages_and_wait_commit because no IBC events
    // corresponding to the transaction can be recognized to confirm the
    // upgrade.
    // https://github.com/informalsystems/hermes/issues/1288#issuecomment-1066884163

    let responses = dst_chain
        .send_messages_and_wait_check_tx(TrackedMsgs::new_single(any_msg, "upgrade"))
        .map_err(|e| UpgradeChainError::submit(dst_chain.id(), e))?;

    Ok(responses[0].hash)
}

/// Looks at the ibc-go version to determine if the legacy `UpgradeProposal` message
/// or if the newer `MsgIBCSoftwareUpdate` message should be used to upgrade the chain.
/// If the ibc-go version returned isn't reliable, a deprecated version, then the version
/// of Cosmos SDK is used, if any. If there is no SDK version, we assume that the legacy upgrade is required.
pub fn requires_legacy_upgrade_proposal(dst_chain: impl ChainHandle) -> bool {
    let Ok(version_specs) = dst_chain.version_specs() else {
        warn!("failed to get version specs, assuming legacy upgrade proposal is required");
        return true;
    };

    let sdk_before_50 = version_specs
        .cosmos_sdk
        .as_ref()
        .map(|s| s.minor < 50)
        .unwrap_or(true);

    match version_specs.ibc_go {
        None => sdk_before_50,
        Some(ibc_version) => {
            // Some ibc-go simapps return unreliable ibc-go versions, such as simapp v8.0.0
            // returns version v1.0.0. So if the ibc-go version matches which is not maintained
            // anymore, use the Cosmos SDK version to determine if the legacy upgrade proposal
            // has to be used
            if ibc_version.major < 4 {
                sdk_before_50
            } else {
                ibc_version.major < 8
            }
        }
    }
}

/// Ibc-go versions up to v7.x.x use the deprecated `UpgradeProposal` to upgrade a chain
fn build_legacy_upgrade_proposal(
    dst_chain: impl ChainHandle, // the chain which will undergo an upgrade
    src_chain: impl ChainHandle, // the source chain; supplies a client state for building the upgrade plan
    opts: &UpgradePlanOptions,
) -> Result<Any, UpgradeChainError> {
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

    let proposal = Proposal::Legacy(proposal);

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

    let msg = LegacyMsgSubmitProposal {
        content: Some(any_proposal),
        initial_deposit: vec![coins],
        proposer: proposer.to_string(),
    };

    let mut buf_msg = Vec::new();
    prost::Message::encode(&msg, &mut buf_msg).unwrap();
    Ok(Any {
        type_url: "/cosmos.gov.v1beta1.MsgSubmitProposal".to_string(),
        value: buf_msg,
    })
}

/// Since ibc-go version to v8.x.x `MsgIbcSoftwareUpgrade` is used to upgrade a chain
fn build_upgrade_proposal(
    dst_chain: impl ChainHandle, // the chain which will undergo an upgrade
    src_chain: impl ChainHandle, // the source chain; supplies a client state for building the upgrade plan
    opts: &UpgradePlanOptions,
) -> Result<Any, UpgradeChainError> {
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

    let proposal = MsgIbcSoftwareUpgrade {
        plan: Some(Plan {
            name: opts.upgrade_plan_name.clone(),
            height: plan_height.revision_height() as i64,
            info: "".to_string(),
            ..Default::default() // deprecated fields - time & upgraded_client_state
        }),
        upgraded_client_state: Some(Any::from(AnyClientState::from(client_state))),
        signer: opts.gov_account.clone(),
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
        messages: vec![any_proposal],
        initial_deposit: vec![coins],
        proposer: proposer.to_string(),
        metadata: "".to_string(),
        title: "proposal 0".to_string(),
        summary: "upgrade the chain software and unbonding period".to_string(),
    };

    let mut buf_msg = Vec::new();
    prost::Message::encode(&msg, &mut buf_msg).unwrap();
    Ok(Any {
        type_url: "/cosmos.gov.v1.MsgSubmitProposal".to_string(),
        value: buf_msg,
    })
}

enum Proposal {
    Default(MsgIbcSoftwareUpgrade),
    Legacy(UpgradeProposal),
}

impl Proposal {
    fn encode(&self, buf: &mut impl BufMut) {
        match self {
            Proposal::Default(p) => prost::Message::encode(p, buf),
            Proposal::Legacy(p) => prost::Message::encode(p, buf),
        }
        .unwrap()
    }

    fn type_url(&self) -> String {
        match self {
            Proposal::Default(_) => "/ibc.core.client.v1.MsgIBCSoftwareUpgrade",
            Proposal::Legacy(_) => "/ibc.core.client.v1.UpgradeProposal",
        }
        .to_owned()
    }
}
