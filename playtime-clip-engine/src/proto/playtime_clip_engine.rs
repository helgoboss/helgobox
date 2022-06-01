#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetContinuousMatrixUpdatesRequest {
    #[prost(string, tag = "1")]
    pub clip_matrix_id: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetContinuousColumnUpdatesRequest {
    #[prost(string, tag = "1")]
    pub clip_matrix_id: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetOccasionalSlotUpdatesRequest {
    #[prost(string, tag = "1")]
    pub clip_matrix_id: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetContinuousSlotUpdatesRequest {
    #[prost(string, tag = "1")]
    pub clip_matrix_id: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetContinuousMatrixUpdatesReply {
    #[prost(message, optional, tag = "1")]
    pub matrix_update: ::core::option::Option<ContinuousMatrixUpdate>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetContinuousColumnUpdatesReply {
    #[prost(message, repeated, tag = "1")]
    pub column_updates: ::prost::alloc::vec::Vec<ContinuousColumnUpdate>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetOccasionalSlotUpdatesReply {
    #[prost(message, repeated, tag = "1")]
    pub slot_updates: ::prost::alloc::vec::Vec<QualifiedOccasionalSlotUpdate>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetContinuousSlotUpdatesReply {
    #[prost(message, repeated, tag = "1")]
    pub slot_updates: ::prost::alloc::vec::Vec<QualifiedContinuousSlotUpdate>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ContinuousMatrixUpdate {
    #[prost(double, tag = "1")]
    pub second: f64,
    #[prost(sint32, tag = "2")]
    pub bar: i32,
    #[prost(double, tag = "3")]
    pub beat: f64,
    #[prost(double, repeated, tag = "4")]
    pub peaks: ::prost::alloc::vec::Vec<f64>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ContinuousColumnUpdate {
    #[prost(double, repeated, tag = "1")]
    pub peaks: ::prost::alloc::vec::Vec<f64>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QualifiedContinuousSlotUpdate {
    #[prost(message, optional, tag = "1")]
    pub slot_coordinates: ::core::option::Option<SlotCoordinates>,
    #[prost(message, optional, tag = "2")]
    pub update: ::core::option::Option<ContinuousSlotUpdate>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QualifiedOccasionalSlotUpdate {
    #[prost(message, optional, tag = "1")]
    pub slot_coordinates: ::core::option::Option<SlotCoordinates>,
    #[prost(oneof = "qualified_occasional_slot_update::Update", tags = "2")]
    pub update: ::core::option::Option<qualified_occasional_slot_update::Update>,
}
/// Nested message and enum types in `QualifiedOccasionalSlotUpdate`.
pub mod qualified_occasional_slot_update {
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Update {
        #[prost(enumeration = "super::SlotPlayState", tag = "2")]
        PlayState(i32),
    }
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ContinuousSlotUpdate {
    #[prost(message, repeated, tag = "2")]
    pub clip_updates: ::prost::alloc::vec::Vec<ContinuousClipUpdate>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ContinuousClipUpdate {
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum SlotPlayState {
    Stopped = 0,
    ScheduledForPlayStart = 1,
    Playing = 2,
    Paused = 3,
    ScheduledForPlayStop = 4,
    ScheduledForRecordingStart = 5,
    Recording = 6,
    ScheduledForRecordingStop = 7,
}
#[doc = r" Generated server implementations."]
pub mod clip_engine_server {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::*;
    #[doc = "Generated trait containing gRPC methods that should be implemented for use with ClipEngineServer."]
    #[async_trait]
    pub trait ClipEngine: Send + Sync + 'static {
        #[doc = "Server streaming response type for the GetContinuousMatrixUpdates method."]
        type GetContinuousMatrixUpdatesStream: futures_core::Stream<
                Item = Result<super::GetContinuousMatrixUpdatesReply, tonic::Status>,
            > + Send
            + Sync
            + 'static;
        async fn get_continuous_matrix_updates(
            &self,
            request: tonic::Request<super::GetContinuousMatrixUpdatesRequest>,
        ) -> Result<tonic::Response<Self::GetContinuousMatrixUpdatesStream>, tonic::Status>;
        #[doc = "Server streaming response type for the GetContinuousColumnUpdates method."]
        type GetContinuousColumnUpdatesStream: futures_core::Stream<
                Item = Result<super::GetContinuousColumnUpdatesReply, tonic::Status>,
            > + Send
            + Sync
            + 'static;
        async fn get_continuous_column_updates(
            &self,
            request: tonic::Request<super::GetContinuousColumnUpdatesRequest>,
        ) -> Result<tonic::Response<Self::GetContinuousColumnUpdatesStream>, tonic::Status>;
        #[doc = "Server streaming response type for the GetOccasionalSlotUpdates method."]
        type GetOccasionalSlotUpdatesStream: futures_core::Stream<Item = Result<super::GetOccasionalSlotUpdatesReply, tonic::Status>>
            + Send
            + Sync
            + 'static;
        async fn get_occasional_slot_updates(
            &self,
            request: tonic::Request<super::GetOccasionalSlotUpdatesRequest>,
        ) -> Result<tonic::Response<Self::GetOccasionalSlotUpdatesStream>, tonic::Status>;
        #[doc = "Server streaming response type for the GetContinuousSlotUpdates method."]
        type GetContinuousSlotUpdatesStream: futures_core::Stream<Item = Result<super::GetContinuousSlotUpdatesReply, tonic::Status>>
            + Send
            + Sync
            + 'static;
        async fn get_continuous_slot_updates(
            &self,
            request: tonic::Request<super::GetContinuousSlotUpdatesRequest>,
        ) -> Result<tonic::Response<Self::GetContinuousSlotUpdatesStream>, tonic::Status>;
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
                "/playtime.clip_engine.ClipEngine/GetContinuousMatrixUpdates" => {
                    #[allow(non_camel_case_types)]
                    struct GetContinuousMatrixUpdatesSvc<T: ClipEngine>(pub Arc<T>);
                    impl<T: ClipEngine>
                        tonic::server::ServerStreamingService<
                            super::GetContinuousMatrixUpdatesRequest,
                        > for GetContinuousMatrixUpdatesSvc<T>
                    {
                        type Response = super::GetContinuousMatrixUpdatesReply;
                        type ResponseStream = T::GetContinuousMatrixUpdatesStream;
                        type Future =
                            BoxFuture<tonic::Response<Self::ResponseStream>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::GetContinuousMatrixUpdatesRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move {
                                (*inner).get_continuous_matrix_updates(request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = GetContinuousMatrixUpdatesSvc(inner);
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
                "/playtime.clip_engine.ClipEngine/GetContinuousColumnUpdates" => {
                    #[allow(non_camel_case_types)]
                    struct GetContinuousColumnUpdatesSvc<T: ClipEngine>(pub Arc<T>);
                    impl<T: ClipEngine>
                        tonic::server::ServerStreamingService<
                            super::GetContinuousColumnUpdatesRequest,
                        > for GetContinuousColumnUpdatesSvc<T>
                    {
                        type Response = super::GetContinuousColumnUpdatesReply;
                        type ResponseStream = T::GetContinuousColumnUpdatesStream;
                        type Future =
                            BoxFuture<tonic::Response<Self::ResponseStream>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::GetContinuousColumnUpdatesRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move {
                                (*inner).get_continuous_column_updates(request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = GetContinuousColumnUpdatesSvc(inner);
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
                "/playtime.clip_engine.ClipEngine/GetOccasionalSlotUpdates" => {
                    #[allow(non_camel_case_types)]
                    struct GetOccasionalSlotUpdatesSvc<T: ClipEngine>(pub Arc<T>);
                    impl<T: ClipEngine>
                        tonic::server::ServerStreamingService<
                            super::GetOccasionalSlotUpdatesRequest,
                        > for GetOccasionalSlotUpdatesSvc<T>
                    {
                        type Response = super::GetOccasionalSlotUpdatesReply;
                        type ResponseStream = T::GetOccasionalSlotUpdatesStream;
                        type Future =
                            BoxFuture<tonic::Response<Self::ResponseStream>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::GetOccasionalSlotUpdatesRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut =
                                async move { (*inner).get_occasional_slot_updates(request).await };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = GetOccasionalSlotUpdatesSvc(inner);
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
                "/playtime.clip_engine.ClipEngine/GetContinuousSlotUpdates" => {
                    #[allow(non_camel_case_types)]
                    struct GetContinuousSlotUpdatesSvc<T: ClipEngine>(pub Arc<T>);
                    impl<T: ClipEngine>
                        tonic::server::ServerStreamingService<
                            super::GetContinuousSlotUpdatesRequest,
                        > for GetContinuousSlotUpdatesSvc<T>
                    {
                        type Response = super::GetContinuousSlotUpdatesReply;
                        type ResponseStream = T::GetContinuousSlotUpdatesStream;
                        type Future =
                            BoxFuture<tonic::Response<Self::ResponseStream>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::GetContinuousSlotUpdatesRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut =
                                async move { (*inner).get_continuous_slot_updates(request).await };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = GetContinuousSlotUpdatesSvc(inner);
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
