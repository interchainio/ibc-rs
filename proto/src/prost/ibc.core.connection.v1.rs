// ICS03 - Connection Data Structures as defined in
// https://github.com/cosmos/ics/tree/master/spec/ics-003-connection-semantics#data-structures

/// ConnectionEnd defines a stateful object on a chain connected to another
/// separate one.
/// NOTE: there must only be 2 defined ConnectionEnds to establish
/// a connection between two chains.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ConnectionEnd {
    /// client associated with this connection.
    #[prost(string, tag = "1")]
    pub client_id: ::prost::alloc::string::String,
    /// IBC version which can be utilised to determine encodings or protocols for
    /// channels or packets utilising this connection.
    #[prost(message, repeated, tag = "2")]
    pub versions: ::prost::alloc::vec::Vec<Version>,
    /// current state of the connection end.
    #[prost(enumeration = "State", tag = "3")]
    pub state: i32,
    /// counterparty chain associated with this connection.
    #[prost(message, optional, tag = "4")]
    pub counterparty: ::core::option::Option<Counterparty>,
    /// delay period that must pass before a consensus state can be used for packet-verification
    /// NOTE: delay period logic is only implemented by some clients.
    #[prost(uint64, tag = "5")]
    pub delay_period: u64,
}
/// IdentifiedConnection defines a connection with additional connection
/// identifier field.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct IdentifiedConnection {
    /// connection identifier.
    #[prost(string, tag = "1")]
    pub id: ::prost::alloc::string::String,
    /// client associated with this connection.
    #[prost(string, tag = "2")]
    pub client_id: ::prost::alloc::string::String,
    /// IBC version which can be utilised to determine encodings or protocols for
    /// channels or packets utilising this connection
    #[prost(message, repeated, tag = "3")]
    pub versions: ::prost::alloc::vec::Vec<Version>,
    /// current state of the connection end.
    #[prost(enumeration = "State", tag = "4")]
    pub state: i32,
    /// counterparty chain associated with this connection.
    #[prost(message, optional, tag = "5")]
    pub counterparty: ::core::option::Option<Counterparty>,
    /// delay period associated with this connection.
    #[prost(uint64, tag = "6")]
    pub delay_period: u64,
}
/// Counterparty defines the counterparty chain associated with a connection end.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Counterparty {
    /// identifies the client on the counterparty chain associated with a given
    /// connection.
    #[prost(string, tag = "1")]
    pub client_id: ::prost::alloc::string::String,
    /// identifies the connection end on the counterparty chain associated with a
    /// given connection.
    #[prost(string, tag = "2")]
    pub connection_id: ::prost::alloc::string::String,
    /// commitment merkle prefix of the counterparty chain.
    #[prost(message, optional, tag = "3")]
    pub prefix: ::core::option::Option<super::super::commitment::v1::MerklePrefix>,
}
/// ClientPaths define all the connection paths for a client state.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ClientPaths {
    /// list of connection paths
    #[prost(string, repeated, tag = "1")]
    pub paths: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}
/// ConnectionPaths define all the connection paths for a given client state.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ConnectionPaths {
    /// client state unique identifier
    #[prost(string, tag = "1")]
    pub client_id: ::prost::alloc::string::String,
    /// list of connection paths
    #[prost(string, repeated, tag = "2")]
    pub paths: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}
/// Version defines the versioning scheme used to negotiate the IBC verison in
/// the connection handshake.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Version {
    /// unique version identifier
    #[prost(string, tag = "1")]
    pub identifier: ::prost::alloc::string::String,
    /// list of features compatible with the specified identifier
    #[prost(string, repeated, tag = "2")]
    pub features: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}
/// State defines if a connection is in one of the following states:
/// INIT, TRYOPEN, OPEN or UNINITIALIZED.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum State {
    /// Default State
    UninitializedUnspecified = 0,
    /// A connection end has just started the opening handshake.
    Init = 1,
    /// A connection end has acknowledged the handshake step on the counterparty
    /// chain.
    Tryopen = 2,
    /// A connection end has completed the handshake.
    Open = 3,
}
/// GenesisState defines the ibc connection submodule's genesis state.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GenesisState {
    #[prost(message, repeated, tag = "1")]
    pub connections: ::prost::alloc::vec::Vec<IdentifiedConnection>,
    #[prost(message, repeated, tag = "2")]
    pub client_connection_paths: ::prost::alloc::vec::Vec<ConnectionPaths>,
    /// the sequence for the next generated connection identifier
    #[prost(uint64, tag = "3")]
    pub next_connection_sequence: u64,
}
/// MsgConnectionOpenInit defines the msg sent by an account on Chain A to
/// initialize a connection with Chain B.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgConnectionOpenInit {
    #[prost(string, tag = "1")]
    pub client_id: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "2")]
    pub counterparty: ::core::option::Option<Counterparty>,
    #[prost(message, optional, tag = "3")]
    pub version: ::core::option::Option<Version>,
    #[prost(uint64, tag = "4")]
    pub delay_period: u64,
    #[prost(string, tag = "5")]
    pub signer: ::prost::alloc::string::String,
}
/// MsgConnectionOpenInitResponse defines the Msg/ConnectionOpenInit response type.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgConnectionOpenInitResponse {}
/// MsgConnectionOpenTry defines a msg sent by a Relayer to try to open a
/// connection on Chain B.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgConnectionOpenTry {
    #[prost(string, tag = "1")]
    pub client_id: ::prost::alloc::string::String,
    /// in the case of crossing hello's, when both chains call OpenInit, we need the connection identifier
    /// of the previous connection in state INIT
    #[prost(string, tag = "2")]
    pub previous_connection_id: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "3")]
    pub client_state: ::core::option::Option<::prost_types::Any>,
    #[prost(message, optional, tag = "4")]
    pub counterparty: ::core::option::Option<Counterparty>,
    #[prost(uint64, tag = "5")]
    pub delay_period: u64,
    #[prost(message, repeated, tag = "6")]
    pub counterparty_versions: ::prost::alloc::vec::Vec<Version>,
    #[prost(message, optional, tag = "7")]
    pub proof_height: ::core::option::Option<super::super::client::v1::Height>,
    /// proof of the initialization the connection on Chain A: `UNITIALIZED ->
    /// INIT`
    #[prost(bytes = "vec", tag = "8")]
    pub proof_init: ::prost::alloc::vec::Vec<u8>,
    /// proof of client state included in message
    #[prost(bytes = "vec", tag = "9")]
    pub proof_client: ::prost::alloc::vec::Vec<u8>,
    /// proof of client consensus state
    #[prost(bytes = "vec", tag = "10")]
    pub proof_consensus: ::prost::alloc::vec::Vec<u8>,
    #[prost(message, optional, tag = "11")]
    pub consensus_height: ::core::option::Option<super::super::client::v1::Height>,
    #[prost(string, tag = "12")]
    pub signer: ::prost::alloc::string::String,
}
/// MsgConnectionOpenTryResponse defines the Msg/ConnectionOpenTry response type.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgConnectionOpenTryResponse {}
/// MsgConnectionOpenAck defines a msg sent by a Relayer to Chain A to
/// acknowledge the change of connection state to TRYOPEN on Chain B.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgConnectionOpenAck {
    #[prost(string, tag = "1")]
    pub connection_id: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub counterparty_connection_id: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "3")]
    pub version: ::core::option::Option<Version>,
    #[prost(message, optional, tag = "4")]
    pub client_state: ::core::option::Option<::prost_types::Any>,
    #[prost(message, optional, tag = "5")]
    pub proof_height: ::core::option::Option<super::super::client::v1::Height>,
    /// proof of the initialization the connection on Chain B: `UNITIALIZED ->
    /// TRYOPEN`
    #[prost(bytes = "vec", tag = "6")]
    pub proof_try: ::prost::alloc::vec::Vec<u8>,
    /// proof of client state included in message
    #[prost(bytes = "vec", tag = "7")]
    pub proof_client: ::prost::alloc::vec::Vec<u8>,
    /// proof of client consensus state
    #[prost(bytes = "vec", tag = "8")]
    pub proof_consensus: ::prost::alloc::vec::Vec<u8>,
    #[prost(message, optional, tag = "9")]
    pub consensus_height: ::core::option::Option<super::super::client::v1::Height>,
    #[prost(string, tag = "10")]
    pub signer: ::prost::alloc::string::String,
}
/// MsgConnectionOpenAckResponse defines the Msg/ConnectionOpenAck response type.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgConnectionOpenAckResponse {}
/// MsgConnectionOpenConfirm defines a msg sent by a Relayer to Chain B to
/// acknowledge the change of connection state to OPEN on Chain A.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgConnectionOpenConfirm {
    #[prost(string, tag = "1")]
    pub connection_id: ::prost::alloc::string::String,
    /// proof for the change of the connection state on Chain A: `INIT -> OPEN`
    #[prost(bytes = "vec", tag = "2")]
    pub proof_ack: ::prost::alloc::vec::Vec<u8>,
    #[prost(message, optional, tag = "3")]
    pub proof_height: ::core::option::Option<super::super::client::v1::Height>,
    #[prost(string, tag = "4")]
    pub signer: ::prost::alloc::string::String,
}
/// MsgConnectionOpenConfirmResponse defines the Msg/ConnectionOpenConfirm response type.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgConnectionOpenConfirmResponse {}
#[doc = r" Generated client implementations."]
pub mod msg_client {
    #![allow(unused_variables, dead_code, missing_docs)]
    use tonic::codegen::*;
    #[doc = " Msg defines the ibc/connection Msg service."]
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
        #[doc = " ConnectionOpenInit defines a rpc handler method for MsgConnectionOpenInit."]
        pub async fn connection_open_init(
            &mut self,
            request: impl tonic::IntoRequest<super::MsgConnectionOpenInit>,
        ) -> Result<tonic::Response<super::MsgConnectionOpenInitResponse>, tonic::Status> {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/ibc.core.connection.v1.Msg/ConnectionOpenInit",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
        #[doc = " ConnectionOpenTry defines a rpc handler method for MsgConnectionOpenTry."]
        pub async fn connection_open_try(
            &mut self,
            request: impl tonic::IntoRequest<super::MsgConnectionOpenTry>,
        ) -> Result<tonic::Response<super::MsgConnectionOpenTryResponse>, tonic::Status> {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/ibc.core.connection.v1.Msg/ConnectionOpenTry",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
        #[doc = " ConnectionOpenAck defines a rpc handler method for MsgConnectionOpenAck."]
        pub async fn connection_open_ack(
            &mut self,
            request: impl tonic::IntoRequest<super::MsgConnectionOpenAck>,
        ) -> Result<tonic::Response<super::MsgConnectionOpenAckResponse>, tonic::Status> {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/ibc.core.connection.v1.Msg/ConnectionOpenAck",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
        #[doc = " ConnectionOpenConfirm defines a rpc handler method for MsgConnectionOpenConfirm."]
        pub async fn connection_open_confirm(
            &mut self,
            request: impl tonic::IntoRequest<super::MsgConnectionOpenConfirm>,
        ) -> Result<tonic::Response<super::MsgConnectionOpenConfirmResponse>, tonic::Status>
        {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/ibc.core.connection.v1.Msg/ConnectionOpenConfirm",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
    }
}
/// QueryConnectionRequest is the request type for the Query/Connection RPC
/// method
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryConnectionRequest {
    /// connection unique identifier
    #[prost(string, tag = "1")]
    pub connection_id: ::prost::alloc::string::String,
}
/// QueryConnectionResponse is the response type for the Query/Connection RPC
/// method. Besides the connection end, it includes a proof and the height from
/// which the proof was retrieved.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryConnectionResponse {
    /// connection associated with the request identifier
    #[prost(message, optional, tag = "1")]
    pub connection: ::core::option::Option<ConnectionEnd>,
    /// merkle proof of existence
    #[prost(bytes = "vec", tag = "2")]
    pub proof: ::prost::alloc::vec::Vec<u8>,
    /// height at which the proof was retrieved
    #[prost(message, optional, tag = "3")]
    pub proof_height: ::core::option::Option<super::super::client::v1::Height>,
}
/// QueryConnectionsRequest is the request type for the Query/Connections RPC
/// method
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryConnectionsRequest {
    #[prost(message, optional, tag = "1")]
    pub pagination: ::core::option::Option<
        super::super::super::super::cosmos::base::query::v1beta1::PageRequest,
    >,
}
/// QueryConnectionsResponse is the response type for the Query/Connections RPC
/// method.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryConnectionsResponse {
    /// list of stored connections of the chain.
    #[prost(message, repeated, tag = "1")]
    pub connections: ::prost::alloc::vec::Vec<IdentifiedConnection>,
    /// pagination response
    #[prost(message, optional, tag = "2")]
    pub pagination: ::core::option::Option<
        super::super::super::super::cosmos::base::query::v1beta1::PageResponse,
    >,
    /// query block height
    #[prost(message, optional, tag = "3")]
    pub height: ::core::option::Option<super::super::client::v1::Height>,
}
/// QueryClientConnectionsRequest is the request type for the
/// Query/ClientConnections RPC method
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryClientConnectionsRequest {
    /// client identifier associated with a connection
    #[prost(string, tag = "1")]
    pub client_id: ::prost::alloc::string::String,
}
/// QueryClientConnectionsResponse is the response type for the
/// Query/ClientConnections RPC method
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryClientConnectionsResponse {
    /// slice of all the connection paths associated with a client.
    #[prost(string, repeated, tag = "1")]
    pub connection_paths: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
    /// merkle proof of existence
    #[prost(bytes = "vec", tag = "2")]
    pub proof: ::prost::alloc::vec::Vec<u8>,
    /// height at which the proof was generated
    #[prost(message, optional, tag = "3")]
    pub proof_height: ::core::option::Option<super::super::client::v1::Height>,
}
/// QueryConnectionClientStateRequest is the request type for the
/// Query/ConnectionClientState RPC method
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryConnectionClientStateRequest {
    /// connection identifier
    #[prost(string, tag = "1")]
    pub connection_id: ::prost::alloc::string::String,
}
/// QueryConnectionClientStateResponse is the response type for the
/// Query/ConnectionClientState RPC method
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryConnectionClientStateResponse {
    /// client state associated with the channel
    #[prost(message, optional, tag = "1")]
    pub identified_client_state:
        ::core::option::Option<super::super::client::v1::IdentifiedClientState>,
    /// merkle proof of existence
    #[prost(bytes = "vec", tag = "2")]
    pub proof: ::prost::alloc::vec::Vec<u8>,
    /// height at which the proof was retrieved
    #[prost(message, optional, tag = "3")]
    pub proof_height: ::core::option::Option<super::super::client::v1::Height>,
}
/// QueryConnectionConsensusStateRequest is the request type for the
/// Query/ConnectionConsensusState RPC method
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryConnectionConsensusStateRequest {
    /// connection identifier
    #[prost(string, tag = "1")]
    pub connection_id: ::prost::alloc::string::String,
    #[prost(uint64, tag = "2")]
    pub revision_number: u64,
    #[prost(uint64, tag = "3")]
    pub revision_height: u64,
}
/// QueryConnectionConsensusStateResponse is the response type for the
/// Query/ConnectionConsensusState RPC method
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryConnectionConsensusStateResponse {
    /// consensus state associated with the channel
    #[prost(message, optional, tag = "1")]
    pub consensus_state: ::core::option::Option<::prost_types::Any>,
    /// client ID associated with the consensus state
    #[prost(string, tag = "2")]
    pub client_id: ::prost::alloc::string::String,
    /// merkle proof of existence
    #[prost(bytes = "vec", tag = "3")]
    pub proof: ::prost::alloc::vec::Vec<u8>,
    /// height at which the proof was retrieved
    #[prost(message, optional, tag = "4")]
    pub proof_height: ::core::option::Option<super::super::client::v1::Height>,
}
#[doc = r" Generated client implementations."]
pub mod query_client {
    #![allow(unused_variables, dead_code, missing_docs)]
    use tonic::codegen::*;
    #[doc = " Query provides defines the gRPC querier service"]
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
        #[doc = " Connection queries an IBC connection end."]
        pub async fn connection(
            &mut self,
            request: impl tonic::IntoRequest<super::QueryConnectionRequest>,
        ) -> Result<tonic::Response<super::QueryConnectionResponse>, tonic::Status> {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path =
                http::uri::PathAndQuery::from_static("/ibc.core.connection.v1.Query/Connection");
            self.inner.unary(request.into_request(), path, codec).await
        }
        #[doc = " Connections queries all the IBC connections of a chain."]
        pub async fn connections(
            &mut self,
            request: impl tonic::IntoRequest<super::QueryConnectionsRequest>,
        ) -> Result<tonic::Response<super::QueryConnectionsResponse>, tonic::Status> {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path =
                http::uri::PathAndQuery::from_static("/ibc.core.connection.v1.Query/Connections");
            self.inner.unary(request.into_request(), path, codec).await
        }
        #[doc = " ClientConnections queries the connection paths associated with a client"]
        #[doc = " state."]
        pub async fn client_connections(
            &mut self,
            request: impl tonic::IntoRequest<super::QueryClientConnectionsRequest>,
        ) -> Result<tonic::Response<super::QueryClientConnectionsResponse>, tonic::Status> {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/ibc.core.connection.v1.Query/ClientConnections",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
        #[doc = " ConnectionClientState queries the client state associated with the"]
        #[doc = " connection."]
        pub async fn connection_client_state(
            &mut self,
            request: impl tonic::IntoRequest<super::QueryConnectionClientStateRequest>,
        ) -> Result<tonic::Response<super::QueryConnectionClientStateResponse>, tonic::Status>
        {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/ibc.core.connection.v1.Query/ConnectionClientState",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
        #[doc = " ConnectionConsensusState queries the consensus state associated with the"]
        #[doc = " connection."]
        pub async fn connection_consensus_state(
            &mut self,
            request: impl tonic::IntoRequest<super::QueryConnectionConsensusStateRequest>,
        ) -> Result<tonic::Response<super::QueryConnectionConsensusStateResponse>, tonic::Status>
        {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/ibc.core.connection.v1.Query/ConnectionConsensusState",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
    }
}
