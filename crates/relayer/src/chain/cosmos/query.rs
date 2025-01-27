use ibc_proto::cosmos::base::tendermint::v1beta1::GetNodeInfoResponse;
use ibc_relayer_types::core::ics04_channel::packet::Sequence;
use ibc_relayer_types::core::ics23_commitment::merkle::{
    convert_tm_to_ics_merkle_proof, MerkleProof,
};
use ibc_relayer_types::core::ics24_host::identifier::ChainId;
use prost::Message;
use tendermint::block::Height;
use tendermint_rpc::query::Query;
use tendermint_rpc::{Client, HttpClient, Url};

use crate::chain::cosmos::version::Specs;
use crate::chain::requests::QueryHeight;
use crate::chain::requests::{QueryClientEventRequest, QueryPacketEventDataRequest, QueryTxHash};
use crate::error::Error;

pub mod account;
pub mod balance;
pub mod connection;
pub mod consensus_state;
pub mod custom;
pub mod denom_trace;
pub mod fee;
pub mod status;
pub mod tx;

/// Generic query response type
#[derive(Clone, Debug, PartialEq)]
pub struct QueryResponse {
    pub value: Vec<u8>,
    pub proof: Option<MerkleProof>,
    pub height: Height,
}

pub fn packet_query(request: &QueryPacketEventDataRequest, seq: Sequence) -> Query {
    Query::eq(
        format!("{}.packet_sequence", request.event_id.as_str()),
        seq.to_string(),
    )
}

pub fn header_query(request: &QueryClientEventRequest) -> Query {
    Query::eq(
        format!("{}.client_id", request.event_id.as_str()),
        request.client_id.to_string(),
    )
    .and_eq(
        format!("{}.consensus_height", request.event_id.as_str()),
        format!(
            "{}-{}",
            request.consensus_height.revision_number(),
            request.consensus_height.revision_height()
        ),
    )
}

pub fn tx_hash_query(request: &QueryTxHash) -> Query {
    Query::eq("tx.hash", request.0.to_string())
}

/// Perform a generic `abci_query`, and return the corresponding deserialized response data.
pub async fn abci_query(
    rpc_client: &HttpClient,
    rpc_address: &Url,
    path: String,
    data: String,
    height: Height,
    prove: bool,
) -> Result<QueryResponse, Error> {
    let height = if height.value() == 0 {
        None
    } else {
        Some(height)
    };

    // Use the Tendermint-rs RPC client to do the query.
    let response = rpc_client
        .abci_query(Some(path), data.into_bytes(), height, prove)
        .await
        .map_err(|e| Error::rpc(rpc_address.clone(), e))?;

    if !response.code.is_ok() {
        // Fail with response log.
        return Err(Error::abci_query(response));
    }

    if prove && response.proof.is_none() {
        // Fail due to empty proof
        return Err(Error::empty_response_proof());
    }

    let proof = response
        .proof
        .map(|p| convert_tm_to_ics_merkle_proof(&p))
        .transpose()
        .map_err(Error::ics23)?;

    let response = QueryResponse {
        value: response.value,
        height: response.height,
        proof,
    };

    Ok(response)
}

/// Queries the chain to obtain the version information.
pub async fn fetch_version_specs(
    chain_id: &ChainId,
    rpc_client: &HttpClient,
    rpc_addr: &Url,
) -> Result<Specs, Error> {
    let query_response = abci_query(
        rpc_client,
        rpc_addr,
        "/cosmos.base.tendermint.v1beta1.Service/GetNodeInfo".to_owned(),
        "".to_owned(),
        QueryHeight::Latest.into(),
        false,
    )
    .await?;
    let node_info_response =
        GetNodeInfoResponse::decode(query_response.value.as_ref()).map_err(|e| {
            Error::protobuf_decode(
                "cosmos.base.tendermint.v1beta1.Service/GetNodeInfo".to_owned(),
                e,
            )
        })?;

    let version = node_info_response.application_version.ok_or_else(|| {
        Error::fetch_version_invalid_version_response(
            chain_id.clone(),
            rpc_addr.to_string(),
            "tendermint::GetNodeInfoRequest".to_string(),
        )
    })?;

    // Parse the raw version info into a domain-type `version::Specs`
    version
        .try_into()
        .map_err(|e| Error::fetch_version_parsing(chain_id.clone(), rpc_addr.to_string(), e))
}
