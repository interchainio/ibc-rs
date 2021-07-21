/// ListAllInterfacesRequest is the request type of the ListAllInterfaces RPC.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ListAllInterfacesRequest {}
/// ListAllInterfacesResponse is the response type of the ListAllInterfaces RPC.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ListAllInterfacesResponse {
    /// interface_names is an array of all the registered interfaces.
    #[prost(string, repeated, tag = "1")]
    pub interface_names: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}
/// ListImplementationsRequest is the request type of the ListImplementations
/// RPC.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ListImplementationsRequest {
    /// interface_name defines the interface to query the implementations for.
    #[prost(string, tag = "1")]
    pub interface_name: ::prost::alloc::string::String,
}
/// ListImplementationsResponse is the response type of the ListImplementations
/// RPC.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ListImplementationsResponse {
    #[prost(string, repeated, tag = "1")]
    pub implementation_message_names: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}
#[doc = r" Generated client implementations."]
pub mod reflection_service_client {
    #![allow(unused_variables, dead_code, missing_docs)]
    use tonic::codegen::*;
    #[doc = " ReflectionService defines a service for interface reflection."]
    #[derive(Debug, Clone)]
    pub struct ReflectionServiceClient<T> {
        inner: tonic::client::Grpc<T>,
    }
    impl ReflectionServiceClient<tonic::transport::Channel> {
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
    impl<T> ReflectionServiceClient<T>
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
        ) -> ReflectionServiceClient<InterceptedService<T, F>>
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
            ReflectionServiceClient::new(InterceptedService::new(inner, interceptor))
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
        #[doc = " ListAllInterfaces lists all the interfaces registered in the interface"]
        #[doc = " registry."]
        pub async fn list_all_interfaces(
            &mut self,
            request: impl tonic::IntoRequest<super::ListAllInterfacesRequest>,
        ) -> Result<tonic::Response<super::ListAllInterfacesResponse>, tonic::Status> {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/cosmos.base.reflection.v1beta1.ReflectionService/ListAllInterfaces",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
        #[doc = " ListImplementations list all the concrete types that implement a given"]
        #[doc = " interface."]
        pub async fn list_implementations(
            &mut self,
            request: impl tonic::IntoRequest<super::ListImplementationsRequest>,
        ) -> Result<tonic::Response<super::ListImplementationsResponse>, tonic::Status> {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/cosmos.base.reflection.v1beta1.ReflectionService/ListImplementations",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
    }
}
