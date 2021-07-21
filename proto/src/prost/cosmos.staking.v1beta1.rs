/// HistoricalInfo contains header and validator information for a given block.
/// It is stored as part of staking module's state, which persists the `n` most
/// recent HistoricalInfo
/// (`n` is set by the staking module's `historical_entries` parameter).
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct HistoricalInfo {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<::tendermint_proto::types::Header>,
    #[prost(message, repeated, tag = "2")]
    pub valset: ::prost::alloc::vec::Vec<Validator>,
}
/// CommissionRates defines the initial commission rates to be used for creating
/// a validator.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CommissionRates {
    /// rate is the commission rate charged to delegators, as a fraction.
    #[prost(string, tag = "1")]
    pub rate: ::prost::alloc::string::String,
    /// max_rate defines the maximum commission rate which validator can ever charge, as a fraction.
    #[prost(string, tag = "2")]
    pub max_rate: ::prost::alloc::string::String,
    /// max_change_rate defines the maximum daily increase of the validator commission, as a fraction.
    #[prost(string, tag = "3")]
    pub max_change_rate: ::prost::alloc::string::String,
}
/// Commission defines commission parameters for a given validator.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Commission {
    /// commission_rates defines the initial commission rates to be used for creating a validator.
    #[prost(message, optional, tag = "1")]
    pub commission_rates: ::core::option::Option<CommissionRates>,
    /// update_time is the last time the commission rate was changed.
    #[prost(message, optional, tag = "2")]
    pub update_time: ::core::option::Option<::prost_types::Timestamp>,
}
/// Description defines a validator description.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Description {
    /// moniker defines a human-readable name for the validator.
    #[prost(string, tag = "1")]
    pub moniker: ::prost::alloc::string::String,
    /// identity defines an optional identity signature (ex. UPort or Keybase).
    #[prost(string, tag = "2")]
    pub identity: ::prost::alloc::string::String,
    /// website defines an optional website link.
    #[prost(string, tag = "3")]
    pub website: ::prost::alloc::string::String,
    /// security_contact defines an optional email for security contact.
    #[prost(string, tag = "4")]
    pub security_contact: ::prost::alloc::string::String,
    /// details define other optional details.
    #[prost(string, tag = "5")]
    pub details: ::prost::alloc::string::String,
}
/// Validator defines a validator, together with the total amount of the
/// Validator's bond shares and their exchange rate to coins. Slashing results in
/// a decrease in the exchange rate, allowing correct calculation of future
/// undelegations without iterating over delegators. When coins are delegated to
/// this validator, the validator is credited with a delegation whose number of
/// bond shares is based on the amount of coins delegated divided by the current
/// exchange rate. Voting power can be calculated as total bonded shares
/// multiplied by exchange rate.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Validator {
    /// operator_address defines the address of the validator's operator; bech encoded in JSON.
    #[prost(string, tag = "1")]
    pub operator_address: ::prost::alloc::string::String,
    /// consensus_pubkey is the consensus public key of the validator, as a Protobuf Any.
    #[prost(message, optional, tag = "2")]
    pub consensus_pubkey: ::core::option::Option<::prost_types::Any>,
    /// jailed defined whether the validator has been jailed from bonded status or not.
    #[prost(bool, tag = "3")]
    pub jailed: bool,
    /// status is the validator status (bonded/unbonding/unbonded).
    #[prost(enumeration = "BondStatus", tag = "4")]
    pub status: i32,
    /// tokens define the delegated tokens (incl. self-delegation).
    #[prost(string, tag = "5")]
    pub tokens: ::prost::alloc::string::String,
    /// delegator_shares defines total shares issued to a validator's delegators.
    #[prost(string, tag = "6")]
    pub delegator_shares: ::prost::alloc::string::String,
    /// description defines the description terms for the validator.
    #[prost(message, optional, tag = "7")]
    pub description: ::core::option::Option<Description>,
    /// unbonding_height defines, if unbonding, the height at which this validator has begun unbonding.
    #[prost(int64, tag = "8")]
    pub unbonding_height: i64,
    /// unbonding_time defines, if unbonding, the min time for the validator to complete unbonding.
    #[prost(message, optional, tag = "9")]
    pub unbonding_time: ::core::option::Option<::prost_types::Timestamp>,
    /// commission defines the commission parameters.
    #[prost(message, optional, tag = "10")]
    pub commission: ::core::option::Option<Commission>,
    /// min_self_delegation is the validator's self declared minimum self delegation.
    #[prost(string, tag = "11")]
    pub min_self_delegation: ::prost::alloc::string::String,
}
/// ValAddresses defines a repeated set of validator addresses.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ValAddresses {
    #[prost(string, repeated, tag = "1")]
    pub addresses: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}
/// DVPair is struct that just has a delegator-validator pair with no other data.
/// It is intended to be used as a marshalable pointer. For example, a DVPair can
/// be used to construct the key to getting an UnbondingDelegation from state.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DvPair {
    #[prost(string, tag = "1")]
    pub delegator_address: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub validator_address: ::prost::alloc::string::String,
}
/// DVPairs defines an array of DVPair objects.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DvPairs {
    #[prost(message, repeated, tag = "1")]
    pub pairs: ::prost::alloc::vec::Vec<DvPair>,
}
/// DVVTriplet is struct that just has a delegator-validator-validator triplet
/// with no other data. It is intended to be used as a marshalable pointer. For
/// example, a DVVTriplet can be used to construct the key to getting a
/// Redelegation from state.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DvvTriplet {
    #[prost(string, tag = "1")]
    pub delegator_address: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub validator_src_address: ::prost::alloc::string::String,
    #[prost(string, tag = "3")]
    pub validator_dst_address: ::prost::alloc::string::String,
}
/// DVVTriplets defines an array of DVVTriplet objects.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DvvTriplets {
    #[prost(message, repeated, tag = "1")]
    pub triplets: ::prost::alloc::vec::Vec<DvvTriplet>,
}
/// Delegation represents the bond with tokens held by an account. It is
/// owned by one delegator, and is associated with the voting power of one
/// validator.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Delegation {
    /// delegator_address is the bech32-encoded address of the delegator.
    #[prost(string, tag = "1")]
    pub delegator_address: ::prost::alloc::string::String,
    /// validator_address is the bech32-encoded address of the validator.
    #[prost(string, tag = "2")]
    pub validator_address: ::prost::alloc::string::String,
    /// shares define the delegation shares received.
    #[prost(string, tag = "3")]
    pub shares: ::prost::alloc::string::String,
}
/// UnbondingDelegation stores all of a single delegator's unbonding bonds
/// for a single validator in an time-ordered list.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct UnbondingDelegation {
    /// delegator_address is the bech32-encoded address of the delegator.
    #[prost(string, tag = "1")]
    pub delegator_address: ::prost::alloc::string::String,
    /// validator_address is the bech32-encoded address of the validator.
    #[prost(string, tag = "2")]
    pub validator_address: ::prost::alloc::string::String,
    /// entries are the unbonding delegation entries.
    ///
    /// unbonding delegation entries
    #[prost(message, repeated, tag = "3")]
    pub entries: ::prost::alloc::vec::Vec<UnbondingDelegationEntry>,
}
/// UnbondingDelegationEntry defines an unbonding object with relevant metadata.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct UnbondingDelegationEntry {
    /// creation_height is the height which the unbonding took place.
    #[prost(int64, tag = "1")]
    pub creation_height: i64,
    /// completion_time is the unix time for unbonding completion.
    #[prost(message, optional, tag = "2")]
    pub completion_time: ::core::option::Option<::prost_types::Timestamp>,
    /// initial_balance defines the tokens initially scheduled to receive at completion.
    #[prost(string, tag = "3")]
    pub initial_balance: ::prost::alloc::string::String,
    /// balance defines the tokens to receive at completion.
    #[prost(string, tag = "4")]
    pub balance: ::prost::alloc::string::String,
}
/// RedelegationEntry defines a redelegation object with relevant metadata.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RedelegationEntry {
    /// creation_height  defines the height which the redelegation took place.
    #[prost(int64, tag = "1")]
    pub creation_height: i64,
    /// completion_time defines the unix time for redelegation completion.
    #[prost(message, optional, tag = "2")]
    pub completion_time: ::core::option::Option<::prost_types::Timestamp>,
    /// initial_balance defines the initial balance when redelegation started.
    #[prost(string, tag = "3")]
    pub initial_balance: ::prost::alloc::string::String,
    /// shares_dst is the amount of destination-validator shares created by redelegation.
    #[prost(string, tag = "4")]
    pub shares_dst: ::prost::alloc::string::String,
}
/// Redelegation contains the list of a particular delegator's redelegating bonds
/// from a particular source validator to a particular destination validator.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Redelegation {
    /// delegator_address is the bech32-encoded address of the delegator.
    #[prost(string, tag = "1")]
    pub delegator_address: ::prost::alloc::string::String,
    /// validator_src_address is the validator redelegation source operator address.
    #[prost(string, tag = "2")]
    pub validator_src_address: ::prost::alloc::string::String,
    /// validator_dst_address is the validator redelegation destination operator address.
    #[prost(string, tag = "3")]
    pub validator_dst_address: ::prost::alloc::string::String,
    /// entries are the redelegation entries.
    ///
    /// redelegation entries
    #[prost(message, repeated, tag = "4")]
    pub entries: ::prost::alloc::vec::Vec<RedelegationEntry>,
}
/// Params defines the parameters for the staking module.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Params {
    /// unbonding_time is the time duration of unbonding.
    #[prost(message, optional, tag = "1")]
    pub unbonding_time: ::core::option::Option<::prost_types::Duration>,
    /// max_validators is the maximum number of validators.
    #[prost(uint32, tag = "2")]
    pub max_validators: u32,
    /// max_entries is the max entries for either unbonding delegation or redelegation (per pair/trio).
    #[prost(uint32, tag = "3")]
    pub max_entries: u32,
    /// historical_entries is the number of historical entries to persist.
    #[prost(uint32, tag = "4")]
    pub historical_entries: u32,
    /// bond_denom defines the bondable coin denomination.
    #[prost(string, tag = "5")]
    pub bond_denom: ::prost::alloc::string::String,
}
/// DelegationResponse is equivalent to Delegation except that it contains a
/// balance in addition to shares which is more suitable for client responses.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DelegationResponse {
    #[prost(message, optional, tag = "1")]
    pub delegation: ::core::option::Option<Delegation>,
    #[prost(message, optional, tag = "2")]
    pub balance: ::core::option::Option<super::super::base::v1beta1::Coin>,
}
/// RedelegationEntryResponse is equivalent to a RedelegationEntry except that it
/// contains a balance in addition to shares which is more suitable for client
/// responses.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RedelegationEntryResponse {
    #[prost(message, optional, tag = "1")]
    pub redelegation_entry: ::core::option::Option<RedelegationEntry>,
    #[prost(string, tag = "4")]
    pub balance: ::prost::alloc::string::String,
}
/// RedelegationResponse is equivalent to a Redelegation except that its entries
/// contain a balance in addition to shares which is more suitable for client
/// responses.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RedelegationResponse {
    #[prost(message, optional, tag = "1")]
    pub redelegation: ::core::option::Option<Redelegation>,
    #[prost(message, repeated, tag = "2")]
    pub entries: ::prost::alloc::vec::Vec<RedelegationEntryResponse>,
}
/// Pool is used for tracking bonded and not-bonded token supply of the bond
/// denomination.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Pool {
    #[prost(string, tag = "1")]
    pub not_bonded_tokens: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub bonded_tokens: ::prost::alloc::string::String,
}
/// BondStatus is the status of a validator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum BondStatus {
    /// UNSPECIFIED defines an invalid validator status.
    Unspecified = 0,
    /// UNBONDED defines a validator that is not bonded.
    Unbonded = 1,
    /// UNBONDING defines a validator that is unbonding.
    Unbonding = 2,
    /// BONDED defines a validator that is bonded.
    Bonded = 3,
}
/// MsgCreateValidator defines a SDK message for creating a new validator.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgCreateValidator {
    #[prost(message, optional, tag = "1")]
    pub description: ::core::option::Option<Description>,
    #[prost(message, optional, tag = "2")]
    pub commission: ::core::option::Option<CommissionRates>,
    #[prost(string, tag = "3")]
    pub min_self_delegation: ::prost::alloc::string::String,
    #[prost(string, tag = "4")]
    pub delegator_address: ::prost::alloc::string::String,
    #[prost(string, tag = "5")]
    pub validator_address: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "6")]
    pub pubkey: ::core::option::Option<::prost_types::Any>,
    #[prost(message, optional, tag = "7")]
    pub value: ::core::option::Option<super::super::base::v1beta1::Coin>,
}
/// MsgCreateValidatorResponse defines the Msg/CreateValidator response type.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgCreateValidatorResponse {}
/// MsgEditValidator defines a SDK message for editing an existing validator.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgEditValidator {
    #[prost(message, optional, tag = "1")]
    pub description: ::core::option::Option<Description>,
    #[prost(string, tag = "2")]
    pub validator_address: ::prost::alloc::string::String,
    /// We pass a reference to the new commission rate and min self delegation as
    /// it's not mandatory to update. If not updated, the deserialized rate will be
    /// zero with no way to distinguish if an update was intended.
    /// REF: #2373
    #[prost(string, tag = "3")]
    pub commission_rate: ::prost::alloc::string::String,
    #[prost(string, tag = "4")]
    pub min_self_delegation: ::prost::alloc::string::String,
}
/// MsgEditValidatorResponse defines the Msg/EditValidator response type.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgEditValidatorResponse {}
/// MsgDelegate defines a SDK message for performing a delegation of coins
/// from a delegator to a validator.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgDelegate {
    #[prost(string, tag = "1")]
    pub delegator_address: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub validator_address: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "3")]
    pub amount: ::core::option::Option<super::super::base::v1beta1::Coin>,
}
/// MsgDelegateResponse defines the Msg/Delegate response type.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgDelegateResponse {}
/// MsgBeginRedelegate defines a SDK message for performing a redelegation
/// of coins from a delegator and source validator to a destination validator.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgBeginRedelegate {
    #[prost(string, tag = "1")]
    pub delegator_address: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub validator_src_address: ::prost::alloc::string::String,
    #[prost(string, tag = "3")]
    pub validator_dst_address: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "4")]
    pub amount: ::core::option::Option<super::super::base::v1beta1::Coin>,
}
/// MsgBeginRedelegateResponse defines the Msg/BeginRedelegate response type.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgBeginRedelegateResponse {
    #[prost(message, optional, tag = "1")]
    pub completion_time: ::core::option::Option<::prost_types::Timestamp>,
}
/// MsgUndelegate defines a SDK message for performing an undelegation from a
/// delegate and a validator.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgUndelegate {
    #[prost(string, tag = "1")]
    pub delegator_address: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub validator_address: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "3")]
    pub amount: ::core::option::Option<super::super::base::v1beta1::Coin>,
}
/// MsgUndelegateResponse defines the Msg/Undelegate response type.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgUndelegateResponse {
    #[prost(message, optional, tag = "1")]
    pub completion_time: ::core::option::Option<::prost_types::Timestamp>,
}
#[doc = r" Generated client implementations."]
pub mod msg_client {
    #![allow(unused_variables, dead_code, missing_docs)]
    use tonic::codegen::*;
    #[doc = " Msg defines the staking Msg service."]
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
        #[doc = " CreateValidator defines a method for creating a new validator."]
        pub async fn create_validator(
            &mut self,
            request: impl tonic::IntoRequest<super::MsgCreateValidator>,
        ) -> Result<tonic::Response<super::MsgCreateValidatorResponse>, tonic::Status> {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path =
                http::uri::PathAndQuery::from_static("/cosmos.staking.v1beta1.Msg/CreateValidator");
            self.inner.unary(request.into_request(), path, codec).await
        }
        #[doc = " EditValidator defines a method for editing an existing validator."]
        pub async fn edit_validator(
            &mut self,
            request: impl tonic::IntoRequest<super::MsgEditValidator>,
        ) -> Result<tonic::Response<super::MsgEditValidatorResponse>, tonic::Status> {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path =
                http::uri::PathAndQuery::from_static("/cosmos.staking.v1beta1.Msg/EditValidator");
            self.inner.unary(request.into_request(), path, codec).await
        }
        #[doc = " Delegate defines a method for performing a delegation of coins"]
        #[doc = " from a delegator to a validator."]
        pub async fn delegate(
            &mut self,
            request: impl tonic::IntoRequest<super::MsgDelegate>,
        ) -> Result<tonic::Response<super::MsgDelegateResponse>, tonic::Status> {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static("/cosmos.staking.v1beta1.Msg/Delegate");
            self.inner.unary(request.into_request(), path, codec).await
        }
        #[doc = " BeginRedelegate defines a method for performing a redelegation"]
        #[doc = " of coins from a delegator and source validator to a destination validator."]
        pub async fn begin_redelegate(
            &mut self,
            request: impl tonic::IntoRequest<super::MsgBeginRedelegate>,
        ) -> Result<tonic::Response<super::MsgBeginRedelegateResponse>, tonic::Status> {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path =
                http::uri::PathAndQuery::from_static("/cosmos.staking.v1beta1.Msg/BeginRedelegate");
            self.inner.unary(request.into_request(), path, codec).await
        }
        #[doc = " Undelegate defines a method for performing an undelegation from a"]
        #[doc = " delegate and a validator."]
        pub async fn undelegate(
            &mut self,
            request: impl tonic::IntoRequest<super::MsgUndelegate>,
        ) -> Result<tonic::Response<super::MsgUndelegateResponse>, tonic::Status> {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path =
                http::uri::PathAndQuery::from_static("/cosmos.staking.v1beta1.Msg/Undelegate");
            self.inner.unary(request.into_request(), path, codec).await
        }
    }
}
/// QueryValidatorsRequest is request type for Query/Validators RPC method.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryValidatorsRequest {
    /// status enables to query for validators matching a given status.
    #[prost(string, tag = "1")]
    pub status: ::prost::alloc::string::String,
    /// pagination defines an optional pagination for the request.
    #[prost(message, optional, tag = "2")]
    pub pagination: ::core::option::Option<super::super::base::query::v1beta1::PageRequest>,
}
/// QueryValidatorsResponse is response type for the Query/Validators RPC method
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryValidatorsResponse {
    /// validators contains all the queried validators.
    #[prost(message, repeated, tag = "1")]
    pub validators: ::prost::alloc::vec::Vec<Validator>,
    /// pagination defines the pagination in the response.
    #[prost(message, optional, tag = "2")]
    pub pagination: ::core::option::Option<super::super::base::query::v1beta1::PageResponse>,
}
/// QueryValidatorRequest is response type for the Query/Validator RPC method
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryValidatorRequest {
    /// validator_addr defines the validator address to query for.
    #[prost(string, tag = "1")]
    pub validator_addr: ::prost::alloc::string::String,
}
/// QueryValidatorResponse is response type for the Query/Validator RPC method
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryValidatorResponse {
    /// validator defines the the validator info.
    #[prost(message, optional, tag = "1")]
    pub validator: ::core::option::Option<Validator>,
}
/// QueryValidatorDelegationsRequest is request type for the
/// Query/ValidatorDelegations RPC method
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryValidatorDelegationsRequest {
    /// validator_addr defines the validator address to query for.
    #[prost(string, tag = "1")]
    pub validator_addr: ::prost::alloc::string::String,
    /// pagination defines an optional pagination for the request.
    #[prost(message, optional, tag = "2")]
    pub pagination: ::core::option::Option<super::super::base::query::v1beta1::PageRequest>,
}
/// QueryValidatorDelegationsResponse is response type for the
/// Query/ValidatorDelegations RPC method
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryValidatorDelegationsResponse {
    #[prost(message, repeated, tag = "1")]
    pub delegation_responses: ::prost::alloc::vec::Vec<DelegationResponse>,
    /// pagination defines the pagination in the response.
    #[prost(message, optional, tag = "2")]
    pub pagination: ::core::option::Option<super::super::base::query::v1beta1::PageResponse>,
}
/// QueryValidatorUnbondingDelegationsRequest is required type for the
/// Query/ValidatorUnbondingDelegations RPC method
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryValidatorUnbondingDelegationsRequest {
    /// validator_addr defines the validator address to query for.
    #[prost(string, tag = "1")]
    pub validator_addr: ::prost::alloc::string::String,
    /// pagination defines an optional pagination for the request.
    #[prost(message, optional, tag = "2")]
    pub pagination: ::core::option::Option<super::super::base::query::v1beta1::PageRequest>,
}
/// QueryValidatorUnbondingDelegationsResponse is response type for the
/// Query/ValidatorUnbondingDelegations RPC method.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryValidatorUnbondingDelegationsResponse {
    #[prost(message, repeated, tag = "1")]
    pub unbonding_responses: ::prost::alloc::vec::Vec<UnbondingDelegation>,
    /// pagination defines the pagination in the response.
    #[prost(message, optional, tag = "2")]
    pub pagination: ::core::option::Option<super::super::base::query::v1beta1::PageResponse>,
}
/// QueryDelegationRequest is request type for the Query/Delegation RPC method.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryDelegationRequest {
    /// delegator_addr defines the delegator address to query for.
    #[prost(string, tag = "1")]
    pub delegator_addr: ::prost::alloc::string::String,
    /// validator_addr defines the validator address to query for.
    #[prost(string, tag = "2")]
    pub validator_addr: ::prost::alloc::string::String,
}
/// QueryDelegationResponse is response type for the Query/Delegation RPC method.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryDelegationResponse {
    /// delegation_responses defines the delegation info of a delegation.
    #[prost(message, optional, tag = "1")]
    pub delegation_response: ::core::option::Option<DelegationResponse>,
}
/// QueryUnbondingDelegationRequest is request type for the
/// Query/UnbondingDelegation RPC method.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryUnbondingDelegationRequest {
    /// delegator_addr defines the delegator address to query for.
    #[prost(string, tag = "1")]
    pub delegator_addr: ::prost::alloc::string::String,
    /// validator_addr defines the validator address to query for.
    #[prost(string, tag = "2")]
    pub validator_addr: ::prost::alloc::string::String,
}
/// QueryDelegationResponse is response type for the Query/UnbondingDelegation
/// RPC method.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryUnbondingDelegationResponse {
    /// unbond defines the unbonding information of a delegation.
    #[prost(message, optional, tag = "1")]
    pub unbond: ::core::option::Option<UnbondingDelegation>,
}
/// QueryDelegatorDelegationsRequest is request type for the
/// Query/DelegatorDelegations RPC method.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryDelegatorDelegationsRequest {
    /// delegator_addr defines the delegator address to query for.
    #[prost(string, tag = "1")]
    pub delegator_addr: ::prost::alloc::string::String,
    /// pagination defines an optional pagination for the request.
    #[prost(message, optional, tag = "2")]
    pub pagination: ::core::option::Option<super::super::base::query::v1beta1::PageRequest>,
}
/// QueryDelegatorDelegationsResponse is response type for the
/// Query/DelegatorDelegations RPC method.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryDelegatorDelegationsResponse {
    /// delegation_responses defines all the delegations' info of a delegator.
    #[prost(message, repeated, tag = "1")]
    pub delegation_responses: ::prost::alloc::vec::Vec<DelegationResponse>,
    /// pagination defines the pagination in the response.
    #[prost(message, optional, tag = "2")]
    pub pagination: ::core::option::Option<super::super::base::query::v1beta1::PageResponse>,
}
/// QueryDelegatorUnbondingDelegationsRequest is request type for the
/// Query/DelegatorUnbondingDelegations RPC method.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryDelegatorUnbondingDelegationsRequest {
    /// delegator_addr defines the delegator address to query for.
    #[prost(string, tag = "1")]
    pub delegator_addr: ::prost::alloc::string::String,
    /// pagination defines an optional pagination for the request.
    #[prost(message, optional, tag = "2")]
    pub pagination: ::core::option::Option<super::super::base::query::v1beta1::PageRequest>,
}
/// QueryUnbondingDelegatorDelegationsResponse is response type for the
/// Query/UnbondingDelegatorDelegations RPC method.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryDelegatorUnbondingDelegationsResponse {
    #[prost(message, repeated, tag = "1")]
    pub unbonding_responses: ::prost::alloc::vec::Vec<UnbondingDelegation>,
    /// pagination defines the pagination in the response.
    #[prost(message, optional, tag = "2")]
    pub pagination: ::core::option::Option<super::super::base::query::v1beta1::PageResponse>,
}
/// QueryRedelegationsRequest is request type for the Query/Redelegations RPC
/// method.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryRedelegationsRequest {
    /// delegator_addr defines the delegator address to query for.
    #[prost(string, tag = "1")]
    pub delegator_addr: ::prost::alloc::string::String,
    /// src_validator_addr defines the validator address to redelegate from.
    #[prost(string, tag = "2")]
    pub src_validator_addr: ::prost::alloc::string::String,
    /// dst_validator_addr defines the validator address to redelegate to.
    #[prost(string, tag = "3")]
    pub dst_validator_addr: ::prost::alloc::string::String,
    /// pagination defines an optional pagination for the request.
    #[prost(message, optional, tag = "4")]
    pub pagination: ::core::option::Option<super::super::base::query::v1beta1::PageRequest>,
}
/// QueryRedelegationsResponse is response type for the Query/Redelegations RPC
/// method.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryRedelegationsResponse {
    #[prost(message, repeated, tag = "1")]
    pub redelegation_responses: ::prost::alloc::vec::Vec<RedelegationResponse>,
    /// pagination defines the pagination in the response.
    #[prost(message, optional, tag = "2")]
    pub pagination: ::core::option::Option<super::super::base::query::v1beta1::PageResponse>,
}
/// QueryDelegatorValidatorsRequest is request type for the
/// Query/DelegatorValidators RPC method.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryDelegatorValidatorsRequest {
    /// delegator_addr defines the delegator address to query for.
    #[prost(string, tag = "1")]
    pub delegator_addr: ::prost::alloc::string::String,
    /// pagination defines an optional pagination for the request.
    #[prost(message, optional, tag = "2")]
    pub pagination: ::core::option::Option<super::super::base::query::v1beta1::PageRequest>,
}
/// QueryDelegatorValidatorsResponse is response type for the
/// Query/DelegatorValidators RPC method.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryDelegatorValidatorsResponse {
    /// validators defines the the validators' info of a delegator.
    #[prost(message, repeated, tag = "1")]
    pub validators: ::prost::alloc::vec::Vec<Validator>,
    /// pagination defines the pagination in the response.
    #[prost(message, optional, tag = "2")]
    pub pagination: ::core::option::Option<super::super::base::query::v1beta1::PageResponse>,
}
/// QueryDelegatorValidatorRequest is request type for the
/// Query/DelegatorValidator RPC method.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryDelegatorValidatorRequest {
    /// delegator_addr defines the delegator address to query for.
    #[prost(string, tag = "1")]
    pub delegator_addr: ::prost::alloc::string::String,
    /// validator_addr defines the validator address to query for.
    #[prost(string, tag = "2")]
    pub validator_addr: ::prost::alloc::string::String,
}
/// QueryDelegatorValidatorResponse response type for the
/// Query/DelegatorValidator RPC method.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryDelegatorValidatorResponse {
    /// validator defines the the validator info.
    #[prost(message, optional, tag = "1")]
    pub validator: ::core::option::Option<Validator>,
}
/// QueryHistoricalInfoRequest is request type for the Query/HistoricalInfo RPC
/// method.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryHistoricalInfoRequest {
    /// height defines at which height to query the historical info.
    #[prost(int64, tag = "1")]
    pub height: i64,
}
/// QueryHistoricalInfoResponse is response type for the Query/HistoricalInfo RPC
/// method.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryHistoricalInfoResponse {
    /// hist defines the historical info at the given height.
    #[prost(message, optional, tag = "1")]
    pub hist: ::core::option::Option<HistoricalInfo>,
}
/// QueryPoolRequest is request type for the Query/Pool RPC method.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryPoolRequest {}
/// QueryPoolResponse is response type for the Query/Pool RPC method.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryPoolResponse {
    /// pool defines the pool info.
    #[prost(message, optional, tag = "1")]
    pub pool: ::core::option::Option<Pool>,
}
/// QueryParamsRequest is request type for the Query/Params RPC method.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryParamsRequest {}
/// QueryParamsResponse is response type for the Query/Params RPC method.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryParamsResponse {
    /// params holds all the parameters of this module.
    #[prost(message, optional, tag = "1")]
    pub params: ::core::option::Option<Params>,
}
#[doc = r" Generated client implementations."]
pub mod query_client {
    #![allow(unused_variables, dead_code, missing_docs)]
    use tonic::codegen::*;
    #[doc = " Query defines the gRPC querier service."]
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
        #[doc = " Validators queries all validators that match the given status."]
        pub async fn validators(
            &mut self,
            request: impl tonic::IntoRequest<super::QueryValidatorsRequest>,
        ) -> Result<tonic::Response<super::QueryValidatorsResponse>, tonic::Status> {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path =
                http::uri::PathAndQuery::from_static("/cosmos.staking.v1beta1.Query/Validators");
            self.inner.unary(request.into_request(), path, codec).await
        }
        #[doc = " Validator queries validator info for given validator address."]
        pub async fn validator(
            &mut self,
            request: impl tonic::IntoRequest<super::QueryValidatorRequest>,
        ) -> Result<tonic::Response<super::QueryValidatorResponse>, tonic::Status> {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path =
                http::uri::PathAndQuery::from_static("/cosmos.staking.v1beta1.Query/Validator");
            self.inner.unary(request.into_request(), path, codec).await
        }
        #[doc = " ValidatorDelegations queries delegate info for given validator."]
        pub async fn validator_delegations(
            &mut self,
            request: impl tonic::IntoRequest<super::QueryValidatorDelegationsRequest>,
        ) -> Result<tonic::Response<super::QueryValidatorDelegationsResponse>, tonic::Status>
        {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/cosmos.staking.v1beta1.Query/ValidatorDelegations",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
        #[doc = " ValidatorUnbondingDelegations queries unbonding delegations of a validator."]
        pub async fn validator_unbonding_delegations(
            &mut self,
            request: impl tonic::IntoRequest<super::QueryValidatorUnbondingDelegationsRequest>,
        ) -> Result<tonic::Response<super::QueryValidatorUnbondingDelegationsResponse>, tonic::Status>
        {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/cosmos.staking.v1beta1.Query/ValidatorUnbondingDelegations",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
        #[doc = " Delegation queries delegate info for given validator delegator pair."]
        pub async fn delegation(
            &mut self,
            request: impl tonic::IntoRequest<super::QueryDelegationRequest>,
        ) -> Result<tonic::Response<super::QueryDelegationResponse>, tonic::Status> {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path =
                http::uri::PathAndQuery::from_static("/cosmos.staking.v1beta1.Query/Delegation");
            self.inner.unary(request.into_request(), path, codec).await
        }
        #[doc = " UnbondingDelegation queries unbonding info for given validator delegator"]
        #[doc = " pair."]
        pub async fn unbonding_delegation(
            &mut self,
            request: impl tonic::IntoRequest<super::QueryUnbondingDelegationRequest>,
        ) -> Result<tonic::Response<super::QueryUnbondingDelegationResponse>, tonic::Status>
        {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/cosmos.staking.v1beta1.Query/UnbondingDelegation",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
        #[doc = " DelegatorDelegations queries all delegations of a given delegator address."]
        pub async fn delegator_delegations(
            &mut self,
            request: impl tonic::IntoRequest<super::QueryDelegatorDelegationsRequest>,
        ) -> Result<tonic::Response<super::QueryDelegatorDelegationsResponse>, tonic::Status>
        {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/cosmos.staking.v1beta1.Query/DelegatorDelegations",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
        #[doc = " DelegatorUnbondingDelegations queries all unbonding delegations of a given"]
        #[doc = " delegator address."]
        pub async fn delegator_unbonding_delegations(
            &mut self,
            request: impl tonic::IntoRequest<super::QueryDelegatorUnbondingDelegationsRequest>,
        ) -> Result<tonic::Response<super::QueryDelegatorUnbondingDelegationsResponse>, tonic::Status>
        {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/cosmos.staking.v1beta1.Query/DelegatorUnbondingDelegations",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
        #[doc = " Redelegations queries redelegations of given address."]
        pub async fn redelegations(
            &mut self,
            request: impl tonic::IntoRequest<super::QueryRedelegationsRequest>,
        ) -> Result<tonic::Response<super::QueryRedelegationsResponse>, tonic::Status> {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path =
                http::uri::PathAndQuery::from_static("/cosmos.staking.v1beta1.Query/Redelegations");
            self.inner.unary(request.into_request(), path, codec).await
        }
        #[doc = " DelegatorValidators queries all validators info for given delegator"]
        #[doc = " address."]
        pub async fn delegator_validators(
            &mut self,
            request: impl tonic::IntoRequest<super::QueryDelegatorValidatorsRequest>,
        ) -> Result<tonic::Response<super::QueryDelegatorValidatorsResponse>, tonic::Status>
        {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/cosmos.staking.v1beta1.Query/DelegatorValidators",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
        #[doc = " DelegatorValidator queries validator info for given delegator validator"]
        #[doc = " pair."]
        pub async fn delegator_validator(
            &mut self,
            request: impl tonic::IntoRequest<super::QueryDelegatorValidatorRequest>,
        ) -> Result<tonic::Response<super::QueryDelegatorValidatorResponse>, tonic::Status>
        {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/cosmos.staking.v1beta1.Query/DelegatorValidator",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
        #[doc = " HistoricalInfo queries the historical info for given height."]
        pub async fn historical_info(
            &mut self,
            request: impl tonic::IntoRequest<super::QueryHistoricalInfoRequest>,
        ) -> Result<tonic::Response<super::QueryHistoricalInfoResponse>, tonic::Status> {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/cosmos.staking.v1beta1.Query/HistoricalInfo",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
        #[doc = " Pool queries the pool info."]
        pub async fn pool(
            &mut self,
            request: impl tonic::IntoRequest<super::QueryPoolRequest>,
        ) -> Result<tonic::Response<super::QueryPoolResponse>, tonic::Status> {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static("/cosmos.staking.v1beta1.Query/Pool");
            self.inner.unary(request.into_request(), path, codec).await
        }
        #[doc = " Parameters queries the staking parameters."]
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
            let path = http::uri::PathAndQuery::from_static("/cosmos.staking.v1beta1.Query/Params");
            self.inner.unary(request.into_request(), path, codec).await
        }
    }
}
/// GenesisState defines the staking module's genesis state.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GenesisState {
    /// params defines all the paramaters of related to deposit.
    #[prost(message, optional, tag = "1")]
    pub params: ::core::option::Option<Params>,
    /// last_total_power tracks the total amounts of bonded tokens recorded during
    /// the previous end block.
    #[prost(bytes = "vec", tag = "2")]
    pub last_total_power: ::prost::alloc::vec::Vec<u8>,
    /// last_validator_powers is a special index that provides a historical list
    /// of the last-block's bonded validators.
    #[prost(message, repeated, tag = "3")]
    pub last_validator_powers: ::prost::alloc::vec::Vec<LastValidatorPower>,
    /// delegations defines the validator set at genesis.
    #[prost(message, repeated, tag = "4")]
    pub validators: ::prost::alloc::vec::Vec<Validator>,
    /// delegations defines the delegations active at genesis.
    #[prost(message, repeated, tag = "5")]
    pub delegations: ::prost::alloc::vec::Vec<Delegation>,
    /// unbonding_delegations defines the unbonding delegations active at genesis.
    #[prost(message, repeated, tag = "6")]
    pub unbonding_delegations: ::prost::alloc::vec::Vec<UnbondingDelegation>,
    /// redelegations defines the redelegations active at genesis.
    #[prost(message, repeated, tag = "7")]
    pub redelegations: ::prost::alloc::vec::Vec<Redelegation>,
    #[prost(bool, tag = "8")]
    pub exported: bool,
}
/// LastValidatorPower required for validator set update logic.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LastValidatorPower {
    /// address is the address of the validator.
    #[prost(string, tag = "1")]
    pub address: ::prost::alloc::string::String,
    /// power defines the power of the validator.
    #[prost(int64, tag = "2")]
    pub power: i64,
}
