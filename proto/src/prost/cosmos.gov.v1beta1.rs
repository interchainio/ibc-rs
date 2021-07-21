/// TextProposal defines a standard text proposal whose changes need to be
/// manually updated in case of approval.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TextProposal {
    #[prost(string, tag = "1")]
    pub title: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub description: ::prost::alloc::string::String,
}
/// Deposit defines an amount deposited by an account address to an active
/// proposal.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Deposit {
    #[prost(uint64, tag = "1")]
    pub proposal_id: u64,
    #[prost(string, tag = "2")]
    pub depositor: ::prost::alloc::string::String,
    #[prost(message, repeated, tag = "3")]
    pub amount: ::prost::alloc::vec::Vec<super::super::base::v1beta1::Coin>,
}
/// Proposal defines the core field members of a governance proposal.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Proposal {
    #[prost(uint64, tag = "1")]
    pub proposal_id: u64,
    #[prost(message, optional, tag = "2")]
    pub content: ::core::option::Option<::prost_types::Any>,
    #[prost(enumeration = "ProposalStatus", tag = "3")]
    pub status: i32,
    #[prost(message, optional, tag = "4")]
    pub final_tally_result: ::core::option::Option<TallyResult>,
    #[prost(message, optional, tag = "5")]
    pub submit_time: ::core::option::Option<::prost_types::Timestamp>,
    #[prost(message, optional, tag = "6")]
    pub deposit_end_time: ::core::option::Option<::prost_types::Timestamp>,
    #[prost(message, repeated, tag = "7")]
    pub total_deposit: ::prost::alloc::vec::Vec<super::super::base::v1beta1::Coin>,
    #[prost(message, optional, tag = "8")]
    pub voting_start_time: ::core::option::Option<::prost_types::Timestamp>,
    #[prost(message, optional, tag = "9")]
    pub voting_end_time: ::core::option::Option<::prost_types::Timestamp>,
}
/// TallyResult defines a standard tally for a governance proposal.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TallyResult {
    #[prost(string, tag = "1")]
    pub yes: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub abstain: ::prost::alloc::string::String,
    #[prost(string, tag = "3")]
    pub no: ::prost::alloc::string::String,
    #[prost(string, tag = "4")]
    pub no_with_veto: ::prost::alloc::string::String,
}
/// Vote defines a vote on a governance proposal.
/// A Vote consists of a proposal ID, the voter, and the vote option.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Vote {
    #[prost(uint64, tag = "1")]
    pub proposal_id: u64,
    #[prost(string, tag = "2")]
    pub voter: ::prost::alloc::string::String,
    #[prost(enumeration = "VoteOption", tag = "3")]
    pub option: i32,
}
/// DepositParams defines the params for deposits on governance proposals.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DepositParams {
    ///  Minimum deposit for a proposal to enter voting period.
    #[prost(message, repeated, tag = "1")]
    pub min_deposit: ::prost::alloc::vec::Vec<super::super::base::v1beta1::Coin>,
    ///  Maximum period for Atom holders to deposit on a proposal. Initial value: 2
    ///  months.
    #[prost(message, optional, tag = "2")]
    pub max_deposit_period: ::core::option::Option<::prost_types::Duration>,
}
/// VotingParams defines the params for voting on governance proposals.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct VotingParams {
    ///  Length of the voting period.
    #[prost(message, optional, tag = "1")]
    pub voting_period: ::core::option::Option<::prost_types::Duration>,
}
/// TallyParams defines the params for tallying votes on governance proposals.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TallyParams {
    ///  Minimum percentage of total stake needed to vote for a result to be
    ///  considered valid.
    #[prost(bytes = "vec", tag = "1")]
    pub quorum: ::prost::alloc::vec::Vec<u8>,
    ///  Minimum proportion of Yes votes for proposal to pass. Default value: 0.5.
    #[prost(bytes = "vec", tag = "2")]
    pub threshold: ::prost::alloc::vec::Vec<u8>,
    ///  Minimum value of Veto votes to Total votes ratio for proposal to be
    ///  vetoed. Default value: 1/3.
    #[prost(bytes = "vec", tag = "3")]
    pub veto_threshold: ::prost::alloc::vec::Vec<u8>,
}
/// VoteOption enumerates the valid vote options for a given governance proposal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum VoteOption {
    /// VOTE_OPTION_UNSPECIFIED defines a no-op vote option.
    Unspecified = 0,
    /// VOTE_OPTION_YES defines a yes vote option.
    Yes = 1,
    /// VOTE_OPTION_ABSTAIN defines an abstain vote option.
    Abstain = 2,
    /// VOTE_OPTION_NO defines a no vote option.
    No = 3,
    /// VOTE_OPTION_NO_WITH_VETO defines a no with veto vote option.
    NoWithVeto = 4,
}
/// ProposalStatus enumerates the valid statuses of a proposal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum ProposalStatus {
    /// PROPOSAL_STATUS_UNSPECIFIED defines the default propopsal status.
    Unspecified = 0,
    /// PROPOSAL_STATUS_DEPOSIT_PERIOD defines a proposal status during the deposit
    /// period.
    DepositPeriod = 1,
    /// PROPOSAL_STATUS_VOTING_PERIOD defines a proposal status during the voting
    /// period.
    VotingPeriod = 2,
    /// PROPOSAL_STATUS_PASSED defines a proposal status of a proposal that has
    /// passed.
    Passed = 3,
    /// PROPOSAL_STATUS_REJECTED defines a proposal status of a proposal that has
    /// been rejected.
    Rejected = 4,
    /// PROPOSAL_STATUS_FAILED defines a proposal status of a proposal that has
    /// failed.
    Failed = 5,
}
/// MsgSubmitProposal defines an sdk.Msg type that supports submitting arbitrary
/// proposal Content.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgSubmitProposal {
    #[prost(message, optional, tag = "1")]
    pub content: ::core::option::Option<::prost_types::Any>,
    #[prost(message, repeated, tag = "2")]
    pub initial_deposit: ::prost::alloc::vec::Vec<super::super::base::v1beta1::Coin>,
    #[prost(string, tag = "3")]
    pub proposer: ::prost::alloc::string::String,
}
/// MsgSubmitProposalResponse defines the Msg/SubmitProposal response type.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgSubmitProposalResponse {
    #[prost(uint64, tag = "1")]
    pub proposal_id: u64,
}
/// MsgVote defines a message to cast a vote.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgVote {
    #[prost(uint64, tag = "1")]
    pub proposal_id: u64,
    #[prost(string, tag = "2")]
    pub voter: ::prost::alloc::string::String,
    #[prost(enumeration = "VoteOption", tag = "3")]
    pub option: i32,
}
/// MsgVoteResponse defines the Msg/Vote response type.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgVoteResponse {}
/// MsgDeposit defines a message to submit a deposit to an existing proposal.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgDeposit {
    #[prost(uint64, tag = "1")]
    pub proposal_id: u64,
    #[prost(string, tag = "2")]
    pub depositor: ::prost::alloc::string::String,
    #[prost(message, repeated, tag = "3")]
    pub amount: ::prost::alloc::vec::Vec<super::super::base::v1beta1::Coin>,
}
/// MsgDepositResponse defines the Msg/Deposit response type.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgDepositResponse {}
#[doc = r" Generated client implementations."]
pub mod msg_client {
    #![allow(unused_variables, dead_code, missing_docs)]
    use tonic::codegen::*;
    #[doc = " Msg defines the bank Msg service."]
    #[derive(Debug, Clone)]
    pub struct MsgClient<T> {
        inner: tonic::client::Grpc<T>,
    }
    impl MsgClient<tonic::transport::Channel> {
        #[doc = r" Attempt to create a new client by connecting to a given endpoint."]
        pub async fn connect<D>(dst: D) -> Result<Self, tonic::transport::Error>
        where
            D: std::convert::TryInto<tonic::transport::Endpoint>,
            D::Error: Into<StdError>,
        {
            let conn = tonic::transport::Endpoint::new(dst)?.connect().await?;
            Ok(Self::new(conn))
        }
    }
    impl<T> MsgClient<T>
    where
        T: tonic::client::GrpcService<tonic::body::BoxBody>,
        T::ResponseBody: Body + Send + Sync + 'static,
        T::Error: Into<StdError>,
        <T::ResponseBody as Body>::Error: Into<StdError> + Send,
    {
        pub fn new(inner: T) -> Self {
            let inner = tonic::client::Grpc::new(inner);
            Self { inner }
        }
        pub fn with_interceptor<F>(inner: T, interceptor: F) -> MsgClient<InterceptedService<T, F>>
        where
            F: FnMut(tonic::Request<()>) -> Result<tonic::Request<()>, tonic::Status>,
            T: Service<
                http::Request<tonic::body::BoxBody>,
                Response = http::Response<
                    <T as tonic::client::GrpcService<tonic::body::BoxBody>>::ResponseBody,
                >,
            >,
            <T as Service<http::Request<tonic::body::BoxBody>>>::Error:
                Into<StdError> + Send + Sync,
        {
            MsgClient::new(InterceptedService::new(inner, interceptor))
        }
        #[doc = r" Compress requests with `gzip`."]
        #[doc = r""]
        #[doc = r" This requires the server to support it otherwise it might respond with an"]
        #[doc = r" error."]
        pub fn send_gzip(mut self) -> Self {
            self.inner = self.inner.send_gzip();
            self
        }
        #[doc = r" Enable decompressing responses with `gzip`."]
        pub fn accept_gzip(mut self) -> Self {
            self.inner = self.inner.accept_gzip();
            self
        }
        #[doc = " SubmitProposal defines a method to create new proposal given a content."]
        pub async fn submit_proposal(
            &mut self,
            request: impl tonic::IntoRequest<super::MsgSubmitProposal>,
        ) -> Result<tonic::Response<super::MsgSubmitProposalResponse>, tonic::Status> {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path =
                http::uri::PathAndQuery::from_static("/cosmos.gov.v1beta1.Msg/SubmitProposal");
            self.inner.unary(request.into_request(), path, codec).await
        }
        #[doc = " Vote defines a method to add a vote on a specific proposal."]
        pub async fn vote(
            &mut self,
            request: impl tonic::IntoRequest<super::MsgVote>,
        ) -> Result<tonic::Response<super::MsgVoteResponse>, tonic::Status> {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static("/cosmos.gov.v1beta1.Msg/Vote");
            self.inner.unary(request.into_request(), path, codec).await
        }
        #[doc = " Deposit defines a method to add deposit on a specific proposal."]
        pub async fn deposit(
            &mut self,
            request: impl tonic::IntoRequest<super::MsgDeposit>,
        ) -> Result<tonic::Response<super::MsgDepositResponse>, tonic::Status> {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static("/cosmos.gov.v1beta1.Msg/Deposit");
            self.inner.unary(request.into_request(), path, codec).await
        }
    }
}
/// QueryProposalRequest is the request type for the Query/Proposal RPC method.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryProposalRequest {
    /// proposal_id defines the unique id of the proposal.
    #[prost(uint64, tag = "1")]
    pub proposal_id: u64,
}
/// QueryProposalResponse is the response type for the Query/Proposal RPC method.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryProposalResponse {
    #[prost(message, optional, tag = "1")]
    pub proposal: ::core::option::Option<Proposal>,
}
/// QueryProposalsRequest is the request type for the Query/Proposals RPC method.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryProposalsRequest {
    /// proposal_status defines the status of the proposals.
    #[prost(enumeration = "ProposalStatus", tag = "1")]
    pub proposal_status: i32,
    /// voter defines the voter address for the proposals.
    #[prost(string, tag = "2")]
    pub voter: ::prost::alloc::string::String,
    /// depositor defines the deposit addresses from the proposals.
    #[prost(string, tag = "3")]
    pub depositor: ::prost::alloc::string::String,
    /// pagination defines an optional pagination for the request.
    #[prost(message, optional, tag = "4")]
    pub pagination: ::core::option::Option<super::super::base::query::v1beta1::PageRequest>,
}
/// QueryProposalsResponse is the response type for the Query/Proposals RPC
/// method.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryProposalsResponse {
    #[prost(message, repeated, tag = "1")]
    pub proposals: ::prost::alloc::vec::Vec<Proposal>,
    /// pagination defines the pagination in the response.
    #[prost(message, optional, tag = "2")]
    pub pagination: ::core::option::Option<super::super::base::query::v1beta1::PageResponse>,
}
/// QueryVoteRequest is the request type for the Query/Vote RPC method.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryVoteRequest {
    /// proposal_id defines the unique id of the proposal.
    #[prost(uint64, tag = "1")]
    pub proposal_id: u64,
    /// voter defines the oter address for the proposals.
    #[prost(string, tag = "2")]
    pub voter: ::prost::alloc::string::String,
}
/// QueryVoteResponse is the response type for the Query/Vote RPC method.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryVoteResponse {
    /// vote defined the queried vote.
    #[prost(message, optional, tag = "1")]
    pub vote: ::core::option::Option<Vote>,
}
/// QueryVotesRequest is the request type for the Query/Votes RPC method.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryVotesRequest {
    /// proposal_id defines the unique id of the proposal.
    #[prost(uint64, tag = "1")]
    pub proposal_id: u64,
    /// pagination defines an optional pagination for the request.
    #[prost(message, optional, tag = "2")]
    pub pagination: ::core::option::Option<super::super::base::query::v1beta1::PageRequest>,
}
/// QueryVotesResponse is the response type for the Query/Votes RPC method.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryVotesResponse {
    /// votes defined the queried votes.
    #[prost(message, repeated, tag = "1")]
    pub votes: ::prost::alloc::vec::Vec<Vote>,
    /// pagination defines the pagination in the response.
    #[prost(message, optional, tag = "2")]
    pub pagination: ::core::option::Option<super::super::base::query::v1beta1::PageResponse>,
}
/// QueryParamsRequest is the request type for the Query/Params RPC method.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryParamsRequest {
    /// params_type defines which parameters to query for, can be one of "voting",
    /// "tallying" or "deposit".
    #[prost(string, tag = "1")]
    pub params_type: ::prost::alloc::string::String,
}
/// QueryParamsResponse is the response type for the Query/Params RPC method.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryParamsResponse {
    /// voting_params defines the parameters related to voting.
    #[prost(message, optional, tag = "1")]
    pub voting_params: ::core::option::Option<VotingParams>,
    /// deposit_params defines the parameters related to deposit.
    #[prost(message, optional, tag = "2")]
    pub deposit_params: ::core::option::Option<DepositParams>,
    /// tally_params defines the parameters related to tally.
    #[prost(message, optional, tag = "3")]
    pub tally_params: ::core::option::Option<TallyParams>,
}
/// QueryDepositRequest is the request type for the Query/Deposit RPC method.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryDepositRequest {
    /// proposal_id defines the unique id of the proposal.
    #[prost(uint64, tag = "1")]
    pub proposal_id: u64,
    /// depositor defines the deposit addresses from the proposals.
    #[prost(string, tag = "2")]
    pub depositor: ::prost::alloc::string::String,
}
/// QueryDepositResponse is the response type for the Query/Deposit RPC method.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryDepositResponse {
    /// deposit defines the requested deposit.
    #[prost(message, optional, tag = "1")]
    pub deposit: ::core::option::Option<Deposit>,
}
/// QueryDepositsRequest is the request type for the Query/Deposits RPC method.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryDepositsRequest {
    /// proposal_id defines the unique id of the proposal.
    #[prost(uint64, tag = "1")]
    pub proposal_id: u64,
    /// pagination defines an optional pagination for the request.
    #[prost(message, optional, tag = "2")]
    pub pagination: ::core::option::Option<super::super::base::query::v1beta1::PageRequest>,
}
/// QueryDepositsResponse is the response type for the Query/Deposits RPC method.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryDepositsResponse {
    #[prost(message, repeated, tag = "1")]
    pub deposits: ::prost::alloc::vec::Vec<Deposit>,
    /// pagination defines the pagination in the response.
    #[prost(message, optional, tag = "2")]
    pub pagination: ::core::option::Option<super::super::base::query::v1beta1::PageResponse>,
}
/// QueryTallyResultRequest is the request type for the Query/Tally RPC method.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryTallyResultRequest {
    /// proposal_id defines the unique id of the proposal.
    #[prost(uint64, tag = "1")]
    pub proposal_id: u64,
}
/// QueryTallyResultResponse is the response type for the Query/Tally RPC method.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryTallyResultResponse {
    /// tally defines the requested tally.
    #[prost(message, optional, tag = "1")]
    pub tally: ::core::option::Option<TallyResult>,
}
#[doc = r" Generated client implementations."]
pub mod query_client {
    #![allow(unused_variables, dead_code, missing_docs)]
    use tonic::codegen::*;
    #[doc = " Query defines the gRPC querier service for gov module"]
    #[derive(Debug, Clone)]
    pub struct QueryClient<T> {
        inner: tonic::client::Grpc<T>,
    }
    impl QueryClient<tonic::transport::Channel> {
        #[doc = r" Attempt to create a new client by connecting to a given endpoint."]
        pub async fn connect<D>(dst: D) -> Result<Self, tonic::transport::Error>
        where
            D: std::convert::TryInto<tonic::transport::Endpoint>,
            D::Error: Into<StdError>,
        {
            let conn = tonic::transport::Endpoint::new(dst)?.connect().await?;
            Ok(Self::new(conn))
        }
    }
    impl<T> QueryClient<T>
    where
        T: tonic::client::GrpcService<tonic::body::BoxBody>,
        T::ResponseBody: Body + Send + Sync + 'static,
        T::Error: Into<StdError>,
        <T::ResponseBody as Body>::Error: Into<StdError> + Send,
    {
        pub fn new(inner: T) -> Self {
            let inner = tonic::client::Grpc::new(inner);
            Self { inner }
        }
        pub fn with_interceptor<F>(
            inner: T,
            interceptor: F,
        ) -> QueryClient<InterceptedService<T, F>>
        where
            F: FnMut(tonic::Request<()>) -> Result<tonic::Request<()>, tonic::Status>,
            T: Service<
                http::Request<tonic::body::BoxBody>,
                Response = http::Response<
                    <T as tonic::client::GrpcService<tonic::body::BoxBody>>::ResponseBody,
                >,
            >,
            <T as Service<http::Request<tonic::body::BoxBody>>>::Error:
                Into<StdError> + Send + Sync,
        {
            QueryClient::new(InterceptedService::new(inner, interceptor))
        }
        #[doc = r" Compress requests with `gzip`."]
        #[doc = r""]
        #[doc = r" This requires the server to support it otherwise it might respond with an"]
        #[doc = r" error."]
        pub fn send_gzip(mut self) -> Self {
            self.inner = self.inner.send_gzip();
            self
        }
        #[doc = r" Enable decompressing responses with `gzip`."]
        pub fn accept_gzip(mut self) -> Self {
            self.inner = self.inner.accept_gzip();
            self
        }
        #[doc = " Proposal queries proposal details based on ProposalID."]
        pub async fn proposal(
            &mut self,
            request: impl tonic::IntoRequest<super::QueryProposalRequest>,
        ) -> Result<tonic::Response<super::QueryProposalResponse>, tonic::Status> {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static("/cosmos.gov.v1beta1.Query/Proposal");
            self.inner.unary(request.into_request(), path, codec).await
        }
        #[doc = " Proposals queries all proposals based on given status."]
        pub async fn proposals(
            &mut self,
            request: impl tonic::IntoRequest<super::QueryProposalsRequest>,
        ) -> Result<tonic::Response<super::QueryProposalsResponse>, tonic::Status> {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static("/cosmos.gov.v1beta1.Query/Proposals");
            self.inner.unary(request.into_request(), path, codec).await
        }
        #[doc = " Vote queries voted information based on proposalID, voterAddr."]
        pub async fn vote(
            &mut self,
            request: impl tonic::IntoRequest<super::QueryVoteRequest>,
        ) -> Result<tonic::Response<super::QueryVoteResponse>, tonic::Status> {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static("/cosmos.gov.v1beta1.Query/Vote");
            self.inner.unary(request.into_request(), path, codec).await
        }
        #[doc = " Votes queries votes of a given proposal."]
        pub async fn votes(
            &mut self,
            request: impl tonic::IntoRequest<super::QueryVotesRequest>,
        ) -> Result<tonic::Response<super::QueryVotesResponse>, tonic::Status> {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static("/cosmos.gov.v1beta1.Query/Votes");
            self.inner.unary(request.into_request(), path, codec).await
        }
        #[doc = " Params queries all parameters of the gov module."]
        pub async fn params(
            &mut self,
            request: impl tonic::IntoRequest<super::QueryParamsRequest>,
        ) -> Result<tonic::Response<super::QueryParamsResponse>, tonic::Status> {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static("/cosmos.gov.v1beta1.Query/Params");
            self.inner.unary(request.into_request(), path, codec).await
        }
        #[doc = " Deposit queries single deposit information based proposalID, depositAddr."]
        pub async fn deposit(
            &mut self,
            request: impl tonic::IntoRequest<super::QueryDepositRequest>,
        ) -> Result<tonic::Response<super::QueryDepositResponse>, tonic::Status> {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static("/cosmos.gov.v1beta1.Query/Deposit");
            self.inner.unary(request.into_request(), path, codec).await
        }
        #[doc = " Deposits queries all deposits of a single proposal."]
        pub async fn deposits(
            &mut self,
            request: impl tonic::IntoRequest<super::QueryDepositsRequest>,
        ) -> Result<tonic::Response<super::QueryDepositsResponse>, tonic::Status> {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static("/cosmos.gov.v1beta1.Query/Deposits");
            self.inner.unary(request.into_request(), path, codec).await
        }
        #[doc = " TallyResult queries the tally of a proposal vote."]
        pub async fn tally_result(
            &mut self,
            request: impl tonic::IntoRequest<super::QueryTallyResultRequest>,
        ) -> Result<tonic::Response<super::QueryTallyResultResponse>, tonic::Status> {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path =
                http::uri::PathAndQuery::from_static("/cosmos.gov.v1beta1.Query/TallyResult");
            self.inner.unary(request.into_request(), path, codec).await
        }
    }
}
/// GenesisState defines the gov module's genesis state.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GenesisState {
    /// starting_proposal_id is the ID of the starting proposal.
    #[prost(uint64, tag = "1")]
    pub starting_proposal_id: u64,
    /// deposits defines all the deposits present at genesis.
    #[prost(message, repeated, tag = "2")]
    pub deposits: ::prost::alloc::vec::Vec<Deposit>,
    /// votes defines all the votes present at genesis.
    #[prost(message, repeated, tag = "3")]
    pub votes: ::prost::alloc::vec::Vec<Vote>,
    /// proposals defines all the proposals present at genesis.
    #[prost(message, repeated, tag = "4")]
    pub proposals: ::prost::alloc::vec::Vec<Proposal>,
    /// params defines all the paramaters of related to deposit.
    #[prost(message, optional, tag = "5")]
    pub deposit_params: ::core::option::Option<DepositParams>,
    /// params defines all the paramaters of related to voting.
    #[prost(message, optional, tag = "6")]
    pub voting_params: ::core::option::Option<VotingParams>,
    /// params defines all the paramaters of related to tally.
    #[prost(message, optional, tag = "7")]
    pub tally_params: ::core::option::Option<TallyParams>,
}
