#[doc = r" Generated client implementations."]
pub mod abci_application_client {
    #![allow(unused_variables, dead_code, missing_docs)]
    use tonic::codegen::*;
    #[derive(Debug, Clone)]
    pub struct AbciApplicationClient<T> {
        inner: tonic::client::Grpc<T>,
    }
    impl AbciApplicationClient<tonic::transport::Channel> {
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
    impl<T> AbciApplicationClient<T>
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
        ) -> AbciApplicationClient<InterceptedService<T, F>>
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
            AbciApplicationClient::new(InterceptedService::new(inner, interceptor))
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
        pub async fn echo(
            &mut self,
            request: impl tonic::IntoRequest<::tendermint_proto::abci::RequestEcho>,
        ) -> Result<tonic::Response<::tendermint_proto::abci::ResponseEcho>, tonic::Status>
        {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path =
                http::uri::PathAndQuery::from_static("/tendermint.abci.ABCIApplication/Echo");
            self.inner.unary(request.into_request(), path, codec).await
        }
        pub async fn flush(
            &mut self,
            request: impl tonic::IntoRequest<::tendermint_proto::abci::RequestFlush>,
        ) -> Result<tonic::Response<::tendermint_proto::abci::ResponseFlush>, tonic::Status>
        {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path =
                http::uri::PathAndQuery::from_static("/tendermint.abci.ABCIApplication/Flush");
            self.inner.unary(request.into_request(), path, codec).await
        }
        pub async fn info(
            &mut self,
            request: impl tonic::IntoRequest<::tendermint_proto::abci::RequestInfo>,
        ) -> Result<tonic::Response<::tendermint_proto::abci::ResponseInfo>, tonic::Status>
        {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path =
                http::uri::PathAndQuery::from_static("/tendermint.abci.ABCIApplication/Info");
            self.inner.unary(request.into_request(), path, codec).await
        }
        pub async fn set_option(
            &mut self,
            request: impl tonic::IntoRequest<::tendermint_proto::abci::RequestSetOption>,
        ) -> Result<tonic::Response<::tendermint_proto::abci::ResponseSetOption>, tonic::Status>
        {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path =
                http::uri::PathAndQuery::from_static("/tendermint.abci.ABCIApplication/SetOption");
            self.inner.unary(request.into_request(), path, codec).await
        }
        pub async fn deliver_tx(
            &mut self,
            request: impl tonic::IntoRequest<::tendermint_proto::abci::RequestDeliverTx>,
        ) -> Result<tonic::Response<::tendermint_proto::abci::ResponseDeliverTx>, tonic::Status>
        {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path =
                http::uri::PathAndQuery::from_static("/tendermint.abci.ABCIApplication/DeliverTx");
            self.inner.unary(request.into_request(), path, codec).await
        }
        pub async fn check_tx(
            &mut self,
            request: impl tonic::IntoRequest<::tendermint_proto::abci::RequestCheckTx>,
        ) -> Result<tonic::Response<::tendermint_proto::abci::ResponseCheckTx>, tonic::Status>
        {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path =
                http::uri::PathAndQuery::from_static("/tendermint.abci.ABCIApplication/CheckTx");
            self.inner.unary(request.into_request(), path, codec).await
        }
        pub async fn query(
            &mut self,
            request: impl tonic::IntoRequest<::tendermint_proto::abci::RequestQuery>,
        ) -> Result<tonic::Response<::tendermint_proto::abci::ResponseQuery>, tonic::Status>
        {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path =
                http::uri::PathAndQuery::from_static("/tendermint.abci.ABCIApplication/Query");
            self.inner.unary(request.into_request(), path, codec).await
        }
        pub async fn commit(
            &mut self,
            request: impl tonic::IntoRequest<::tendermint_proto::abci::RequestCommit>,
        ) -> Result<tonic::Response<::tendermint_proto::abci::ResponseCommit>, tonic::Status>
        {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path =
                http::uri::PathAndQuery::from_static("/tendermint.abci.ABCIApplication/Commit");
            self.inner.unary(request.into_request(), path, codec).await
        }
        pub async fn init_chain(
            &mut self,
            request: impl tonic::IntoRequest<::tendermint_proto::abci::RequestInitChain>,
        ) -> Result<tonic::Response<::tendermint_proto::abci::ResponseInitChain>, tonic::Status>
        {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path =
                http::uri::PathAndQuery::from_static("/tendermint.abci.ABCIApplication/InitChain");
            self.inner.unary(request.into_request(), path, codec).await
        }
        pub async fn begin_block(
            &mut self,
            request: impl tonic::IntoRequest<::tendermint_proto::abci::RequestBeginBlock>,
        ) -> Result<tonic::Response<::tendermint_proto::abci::ResponseBeginBlock>, tonic::Status>
        {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path =
                http::uri::PathAndQuery::from_static("/tendermint.abci.ABCIApplication/BeginBlock");
            self.inner.unary(request.into_request(), path, codec).await
        }
        pub async fn end_block(
            &mut self,
            request: impl tonic::IntoRequest<::tendermint_proto::abci::RequestEndBlock>,
        ) -> Result<tonic::Response<::tendermint_proto::abci::ResponseEndBlock>, tonic::Status>
        {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path =
                http::uri::PathAndQuery::from_static("/tendermint.abci.ABCIApplication/EndBlock");
            self.inner.unary(request.into_request(), path, codec).await
        }
        pub async fn list_snapshots(
            &mut self,
            request: impl tonic::IntoRequest<::tendermint_proto::abci::RequestListSnapshots>,
        ) -> Result<tonic::Response<::tendermint_proto::abci::ResponseListSnapshots>, tonic::Status>
        {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/tendermint.abci.ABCIApplication/ListSnapshots",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
        pub async fn offer_snapshot(
            &mut self,
            request: impl tonic::IntoRequest<::tendermint_proto::abci::RequestOfferSnapshot>,
        ) -> Result<tonic::Response<::tendermint_proto::abci::ResponseOfferSnapshot>, tonic::Status>
        {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/tendermint.abci.ABCIApplication/OfferSnapshot",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
        pub async fn load_snapshot_chunk(
            &mut self,
            request: impl tonic::IntoRequest<::tendermint_proto::abci::RequestLoadSnapshotChunk>,
        ) -> Result<
            tonic::Response<::tendermint_proto::abci::ResponseLoadSnapshotChunk>,
            tonic::Status,
        > {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/tendermint.abci.ABCIApplication/LoadSnapshotChunk",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
        pub async fn apply_snapshot_chunk(
            &mut self,
            request: impl tonic::IntoRequest<::tendermint_proto::abci::RequestApplySnapshotChunk>,
        ) -> Result<
            tonic::Response<::tendermint_proto::abci::ResponseApplySnapshotChunk>,
            tonic::Status,
        > {
            self.inner.ready().await.map_err(|e| {
                tonic::Status::new(
                    tonic::Code::Unknown,
                    format!("Service was not ready: {}", e.into()),
                )
            })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/tendermint.abci.ABCIApplication/ApplySnapshotChunk",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
    }
}
