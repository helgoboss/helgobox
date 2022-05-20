#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DoubleRequest {}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DoubleReply {
    #[prost(double, tag = "1")]
    pub value: f64,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetClipMatrixRequest {
    #[prost(string, tag = "1")]
    pub session_id: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetClipMatrixReply {
    #[prost(string, tag = "1")]
    pub value: ::prost::alloc::string::String,
}
#[doc = r" Generated server implementations."]
pub mod greeter_server {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::*;
    #[doc = "Generated trait containing gRPC methods that should be implemented for use with GreeterServer."]
    #[async_trait]
    pub trait Greeter: Send + Sync + 'static {
        async fn get_clip_matrix(
            &self,
            request: tonic::Request<super::GetClipMatrixRequest>,
        ) -> Result<tonic::Response<super::GetClipMatrixReply>, tonic::Status>;
        #[doc = "Server streaming response type for the StreamExperiment method."]
        type StreamExperimentStream: futures_core::Stream<Item = Result<super::DoubleReply, tonic::Status>>
            + Send
            + Sync
            + 'static;
        async fn stream_experiment(
            &self,
            request: tonic::Request<super::DoubleRequest>,
        ) -> Result<tonic::Response<Self::StreamExperimentStream>, tonic::Status>;
    }
    #[derive(Debug)]
    pub struct GreeterServer<T: Greeter> {
        inner: _Inner<T>,
        accept_compression_encodings: (),
        send_compression_encodings: (),
    }
    struct _Inner<T>(Arc<T>);
    impl<T: Greeter> GreeterServer<T> {
        pub fn new(inner: T) -> Self {
            let inner = Arc::new(inner);
            let inner = _Inner(inner);
            Self {
                inner,
                accept_compression_encodings: Default::default(),
                send_compression_encodings: Default::default(),
            }
        }
        pub fn with_interceptor<F>(inner: T, interceptor: F) -> InterceptedService<Self, F>
        where
            F: tonic::service::Interceptor,
        {
            InterceptedService::new(Self::new(inner), interceptor)
        }
    }
    impl<T, B> tonic::codegen::Service<http::Request<B>> for GreeterServer<T>
    where
        T: Greeter,
        B: Body + Send + Sync + 'static,
        B::Error: Into<StdError> + Send + 'static,
    {
        type Response = http::Response<tonic::body::BoxBody>;
        type Error = Never;
        type Future = BoxFuture<Self::Response, Self::Error>;
        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
        fn call(&mut self, req: http::Request<B>) -> Self::Future {
            let inner = self.inner.clone();
            match req.uri().path() {
                "/realearn.Greeter/GetClipMatrix" => {
                    #[allow(non_camel_case_types)]
                    struct GetClipMatrixSvc<T: Greeter>(pub Arc<T>);
                    impl<T: Greeter> tonic::server::UnaryService<super::GetClipMatrixRequest> for GetClipMatrixSvc<T> {
                        type Response = super::GetClipMatrixReply;
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::GetClipMatrixRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { (*inner).get_clip_matrix(request).await };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = GetClipMatrixSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec).apply_compression_config(
                            accept_compression_encodings,
                            send_compression_encodings,
                        );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/realearn.Greeter/StreamExperiment" => {
                    #[allow(non_camel_case_types)]
                    struct StreamExperimentSvc<T: Greeter>(pub Arc<T>);
                    impl<T: Greeter> tonic::server::ServerStreamingService<super::DoubleRequest>
                        for StreamExperimentSvc<T>
                    {
                        type Response = super::DoubleReply;
                        type ResponseStream = T::StreamExperimentStream;
                        type Future =
                            BoxFuture<tonic::Response<Self::ResponseStream>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::DoubleRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { (*inner).stream_experiment(request).await };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = StreamExperimentSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec).apply_compression_config(
                            accept_compression_encodings,
                            send_compression_encodings,
                        );
                        let res = grpc.server_streaming(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                _ => Box::pin(async move {
                    Ok(http::Response::builder()
                        .status(200)
                        .header("grpc-status", "12")
                        .header("content-type", "application/grpc")
                        .body(empty_body())
                        .unwrap())
                }),
            }
        }
    }
    impl<T: Greeter> Clone for GreeterServer<T> {
        fn clone(&self) -> Self {
            let inner = self.inner.clone();
            Self {
                inner,
                accept_compression_encodings: self.accept_compression_encodings,
                send_compression_encodings: self.send_compression_encodings,
            }
        }
    }
    impl<T: Greeter> Clone for _Inner<T> {
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }
    impl<T: std::fmt::Debug> std::fmt::Debug for _Inner<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{:?}", self.0)
        }
    }
    impl<T: Greeter> tonic::transport::NamedService for GreeterServer<T> {
        const NAME: &'static str = "realearn.Greeter";
    }
}
