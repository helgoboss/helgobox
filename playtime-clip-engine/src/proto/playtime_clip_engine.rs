#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetContinuousMatrixStateUpdatesRequest {
    #[prost(string, tag = "1")]
    pub clip_matrix_id: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetContinuousTrackStateUpdatesRequest {
    #[prost(string, tag = "1")]
    pub clip_matrix_id: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetContinuousSlotStateUpdatesRequest {
    #[prost(string, tag = "1")]
    pub clip_matrix_id: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetContinuousMatrixStateUpdatesReply {
    #[prost(message, optional, tag = "1")]
    pub matrix_states: ::core::option::Option<ContinuousMatrixState>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetContinuousTrackStateUpdatesReply {
    #[prost(map = "string, message", tag = "1")]
    pub track_states:
        ::std::collections::HashMap<::prost::alloc::string::String, ContinuousTrackState>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetContinuousSlotStateUpdatesReply {
    #[prost(message, repeated, tag = "1")]
    pub slot_states: ::prost::alloc::vec::Vec<QualifiedContinuousSlotState>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ContinuousMatrixState {
    #[prost(double, tag = "1")]
    pub second: f64,
    #[prost(sint32, tag = "2")]
    pub bar: i32,
    #[prost(double, tag = "3")]
    pub beat: f64,
    #[prost(double, tag = "4")]
    pub peak: f64,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ContinuousTrackState {
    #[prost(double, tag = "1")]
    pub peak: f64,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QualifiedContinuousSlotState {
    #[prost(message, optional, tag = "1")]
    pub slot_coordinates: ::core::option::Option<SlotCoordinates>,
    #[prost(message, optional, tag = "2")]
    pub state: ::core::option::Option<ContinuousSlotState>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ContinuousSlotState {
    #[prost(message, repeated, tag = "2")]
    pub clip_states: ::prost::alloc::vec::Vec<ContinuousClipState>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ContinuousClipState {
    #[prost(double, tag = "1")]
    pub position: f64,
    #[prost(double, tag = "2")]
    pub peak: f64,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SlotCoordinates {
    #[prost(uint32, tag = "1")]
    pub column: u32,
    #[prost(uint32, tag = "2")]
    pub row: u32,
}
#[doc = r" Generated server implementations."]
pub mod clip_engine_server {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::*;
    #[doc = "Generated trait containing gRPC methods that should be implemented for use with ClipEngineServer."]
    #[async_trait]
    pub trait ClipEngine: Send + Sync + 'static {
        #[doc = "Server streaming response type for the GetContinuousMatrixStateUpdates method."]
        type GetContinuousMatrixStateUpdatesStream: futures_core::Stream<
                Item = Result<super::GetContinuousMatrixStateUpdatesReply, tonic::Status>,
            > + Send
            + Sync
            + 'static;
        async fn get_continuous_matrix_state_updates(
            &self,
            request: tonic::Request<super::GetContinuousMatrixStateUpdatesRequest>,
        ) -> Result<tonic::Response<Self::GetContinuousMatrixStateUpdatesStream>, tonic::Status>;
        #[doc = "Server streaming response type for the GetContinuousTrackStateUpdates method."]
        type GetContinuousTrackStateUpdatesStream: futures_core::Stream<
                Item = Result<super::GetContinuousTrackStateUpdatesReply, tonic::Status>,
            > + Send
            + Sync
            + 'static;
        async fn get_continuous_track_state_updates(
            &self,
            request: tonic::Request<super::GetContinuousTrackStateUpdatesRequest>,
        ) -> Result<tonic::Response<Self::GetContinuousTrackStateUpdatesStream>, tonic::Status>;
        #[doc = "Server streaming response type for the GetContinuousSlotStateUpdates method."]
        type GetContinuousSlotStateUpdatesStream: futures_core::Stream<
                Item = Result<super::GetContinuousSlotStateUpdatesReply, tonic::Status>,
            > + Send
            + Sync
            + 'static;
        async fn get_continuous_slot_state_updates(
            &self,
            request: tonic::Request<super::GetContinuousSlotStateUpdatesRequest>,
        ) -> Result<tonic::Response<Self::GetContinuousSlotStateUpdatesStream>, tonic::Status>;
    }
    #[derive(Debug)]
    pub struct ClipEngineServer<T: ClipEngine> {
        inner: _Inner<T>,
        accept_compression_encodings: (),
        send_compression_encodings: (),
    }
    struct _Inner<T>(Arc<T>);
    impl<T: ClipEngine> ClipEngineServer<T> {
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
    impl<T, B> tonic::codegen::Service<http::Request<B>> for ClipEngineServer<T>
    where
        T: ClipEngine,
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
                "/playtime.clip_engine.ClipEngine/GetContinuousMatrixStateUpdates" => {
                    #[allow(non_camel_case_types)]
                    struct GetContinuousMatrixStateUpdatesSvc<T: ClipEngine>(pub Arc<T>);
                    impl<T: ClipEngine>
                        tonic::server::ServerStreamingService<
                            super::GetContinuousMatrixStateUpdatesRequest,
                        > for GetContinuousMatrixStateUpdatesSvc<T>
                    {
                        type Response = super::GetContinuousMatrixStateUpdatesReply;
                        type ResponseStream = T::GetContinuousMatrixStateUpdatesStream;
                        type Future =
                            BoxFuture<tonic::Response<Self::ResponseStream>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::GetContinuousMatrixStateUpdatesRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move {
                                (*inner).get_continuous_matrix_state_updates(request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = GetContinuousMatrixStateUpdatesSvc(inner);
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
                "/playtime.clip_engine.ClipEngine/GetContinuousTrackStateUpdates" => {
                    #[allow(non_camel_case_types)]
                    struct GetContinuousTrackStateUpdatesSvc<T: ClipEngine>(pub Arc<T>);
                    impl<T: ClipEngine>
                        tonic::server::ServerStreamingService<
                            super::GetContinuousTrackStateUpdatesRequest,
                        > for GetContinuousTrackStateUpdatesSvc<T>
                    {
                        type Response = super::GetContinuousTrackStateUpdatesReply;
                        type ResponseStream = T::GetContinuousTrackStateUpdatesStream;
                        type Future =
                            BoxFuture<tonic::Response<Self::ResponseStream>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::GetContinuousTrackStateUpdatesRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move {
                                (*inner).get_continuous_track_state_updates(request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = GetContinuousTrackStateUpdatesSvc(inner);
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
                "/playtime.clip_engine.ClipEngine/GetContinuousSlotStateUpdates" => {
                    #[allow(non_camel_case_types)]
                    struct GetContinuousSlotStateUpdatesSvc<T: ClipEngine>(pub Arc<T>);
                    impl<T: ClipEngine>
                        tonic::server::ServerStreamingService<
                            super::GetContinuousSlotStateUpdatesRequest,
                        > for GetContinuousSlotStateUpdatesSvc<T>
                    {
                        type Response = super::GetContinuousSlotStateUpdatesReply;
                        type ResponseStream = T::GetContinuousSlotStateUpdatesStream;
                        type Future =
                            BoxFuture<tonic::Response<Self::ResponseStream>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::GetContinuousSlotStateUpdatesRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move {
                                (*inner).get_continuous_slot_state_updates(request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = GetContinuousSlotStateUpdatesSvc(inner);
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
    impl<T: ClipEngine> Clone for ClipEngineServer<T> {
        fn clone(&self) -> Self {
            let inner = self.inner.clone();
            Self {
                inner,
                accept_compression_encodings: self.accept_compression_encodings,
                send_compression_encodings: self.send_compression_encodings,
            }
        }
    }
    impl<T: ClipEngine> Clone for _Inner<T> {
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }
    impl<T: std::fmt::Debug> std::fmt::Debug for _Inner<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{:?}", self.0)
        }
    }
    impl<T: ClipEngine> tonic::transport::NamedService for ClipEngineServer<T> {
        const NAME: &'static str = "playtime.clip_engine.ClipEngine";
    }
}
