use prost::Message;

use ibc_proto::ibc::applications::transfer::v1::DenomTrace as RawDenomTrace;
use tendermint_rpc::Client;
use tendermint_rpc::HttpClient;

use crate::denom::DenomTrace;
use crate::error::Error;

/// Extracted `QueryDenomTraceRequest` since ibc-go removed it from
/// the proto definitions, https://github.com/cosmos/ibc-go/pull/6433
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryDenomTraceRequest {
    /// hash (in hex format) or denom (full denom with ibc prefix) of the denomination trace information.
    #[prost(string, tag = "1")]
    pub hash: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryDenomTraceResponse {
    /// denom_trace returns the requested denomination trace information.
    #[prost(message, optional, tag = "1")]
    pub denom_trace: ::core::option::Option<RawDenomTrace>,
}
impl ::prost::Name for QueryDenomTraceResponse {
    const NAME: &'static str = "QueryDenomTraceResponse";
    const PACKAGE: &'static str = "ibc.applications.transfer.v1";
    fn full_name() -> ::prost::alloc::string::String {
        "ibc.applications.transfer.v1.QueryDenomTraceResponse".into()
    }
    fn type_url() -> ::prost::alloc::string::String {
        "/ibc.applications.transfer.v1.QueryDenomTraceResponse".into()
    }
}

// Uses the GRPC client to retrieve the denom trace for a specific hash
pub async fn query_denom_trace(client: &HttpClient, hash: &str) -> Result<DenomTrace, Error> {
    let request = QueryDenomTraceRequest {
        hash: hash.to_string(),
    };
    let path = "/ibc.applications.transfer.v1.Query/DenomTrace".to_owned();

    let data = prost::Message::encode_to_vec(&request);

    let response = client
        .abci_query(Some(path.clone()), data, None, false)
        .await
        .map_err(|e| Error::failed_abci_query(path, e))?;

    let result = QueryDenomTraceResponse::decode(response.value.as_slice())
        .map_err(|e| Error::protobuf_decode("QueryDenomTraceResponse".to_owned(), e))?;

    let denom_trace = result
        .denom_trace
        .ok_or_else(|| Error::empty_denom_trace(hash.to_owned()))?;

    Ok(DenomTrace {
        path: denom_trace.path,
        base_denom: denom_trace.base_denom,
    })
}
