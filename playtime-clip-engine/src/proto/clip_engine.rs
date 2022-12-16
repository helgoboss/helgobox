#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SetMatrixTempoRequest {
    #[prost(string, tag = "1")]
    pub clip_matrix_id: ::prost::alloc::string::String,
    #[prost(double, tag = "2")]
    pub bpm: f64,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SetMatrixVolumeRequest {
    #[prost(string, tag = "1")]
    pub clip_matrix_id: ::prost::alloc::string::String,
    #[prost(double, tag = "2")]
    pub db: f64,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SetMatrixPanRequest {
    #[prost(string, tag = "1")]
    pub clip_matrix_id: ::prost::alloc::string::String,
    #[prost(double, tag = "2")]
    pub pan: f64,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SetColumnVolumeRequest {
    #[prost(string, tag = "1")]
    pub clip_matrix_id: ::prost::alloc::string::String,
    #[prost(uint32, tag = "2")]
    pub column_index: u32,
    #[prost(double, tag = "3")]
    pub db: f64,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Empty {}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TriggerMatrixRequest {
    #[prost(string, tag = "1")]
    pub clip_matrix_id: ::prost::alloc::string::String,
    #[prost(enumeration = "TriggerMatrixAction", tag = "2")]
    pub action: i32,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TriggerColumnRequest {
    #[prost(string, tag = "1")]
    pub clip_matrix_id: ::prost::alloc::string::String,
    #[prost(uint32, tag = "2")]
    pub column: u32,
    #[prost(enumeration = "TriggerColumnAction", tag = "3")]
    pub action: i32,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TriggerRowRequest {
    #[prost(string, tag = "1")]
    pub clip_matrix_id: ::prost::alloc::string::String,
    #[prost(uint32, tag = "2")]
    pub row: u32,
    #[prost(enumeration = "TriggerRowAction", tag = "3")]
    pub action: i32,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TriggerSlotRequest {
    #[prost(string, tag = "1")]
    pub clip_matrix_id: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "2")]
    pub slot_coordinates: ::core::option::Option<SlotCoordinates>,
    #[prost(enumeration = "TriggerSlotAction", tag = "3")]
    pub action: i32,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SetClipNameRequest {
    #[prost(string, tag = "1")]
    pub clip_matrix_id: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "2")]
    pub slot_coordinates: ::core::option::Option<SlotCoordinates>,
    #[prost(string, optional, tag = "3")]
    pub name: ::core::option::Option<::prost::alloc::string::String>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetOccasionalMatrixUpdatesRequest {
    #[prost(string, tag = "1")]
    pub clip_matrix_id: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetOccasionalTrackUpdatesRequest {
    #[prost(string, tag = "1")]
    pub clip_matrix_id: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetOccasionalSlotUpdatesRequest {
    #[prost(string, tag = "1")]
    pub clip_matrix_id: ::prost::alloc::string::String,
}
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
pub struct GetContinuousSlotUpdatesRequest {
    #[prost(string, tag = "1")]
    pub clip_matrix_id: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetOccasionalMatrixUpdatesReply {
    /// For each updated matrix property
    #[prost(message, repeated, tag = "1")]
    pub matrix_updates: ::prost::alloc::vec::Vec<OccasionalMatrixUpdate>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetOccasionalTrackUpdatesReply {
    /// For each updated column track
    #[prost(message, repeated, tag = "1")]
    pub track_updates: ::prost::alloc::vec::Vec<QualifiedOccasionalTrackUpdate>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetOccasionalSlotUpdatesReply {
    /// For each updated slot AND property
    #[prost(message, repeated, tag = "1")]
    pub slot_updates: ::prost::alloc::vec::Vec<QualifiedOccasionalSlotUpdate>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetContinuousMatrixUpdatesReply {
    #[prost(message, optional, tag = "1")]
    pub matrix_update: ::core::option::Option<ContinuousMatrixUpdate>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetContinuousColumnUpdatesReply {
    /// For each column
    #[prost(message, repeated, tag = "1")]
    pub column_updates: ::prost::alloc::vec::Vec<ContinuousColumnUpdate>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetContinuousSlotUpdatesReply {
    /// For each updated slot
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
pub struct QualifiedOccasionalTrackUpdate {
    #[prost(string, tag = "1")]
    pub track_id: ::prost::alloc::string::String,
    /// For each updated track property
    #[prost(message, repeated, tag = "2")]
    pub track_updates: ::prost::alloc::vec::Vec<OccasionalTrackUpdate>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct OccasionalMatrixUpdate {
    #[prost(
        oneof = "occasional_matrix_update::Update",
        tags = "1, 2, 3, 4, 5, 6, 7, 8, 9, 10"
    )]
    pub update: ::core::option::Option<occasional_matrix_update::Update>,
}
/// Nested message and enum types in `OccasionalMatrixUpdate`.
pub mod occasional_matrix_update {
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Update {
        #[prost(double, tag = "1")]
        Volume(f64),
        #[prost(double, tag = "2")]
        Pan(f64),
        #[prost(double, tag = "3")]
        Tempo(f64),
        #[prost(enumeration = "super::ArrangementPlayState", tag = "4")]
        ArrangementPlayState(i32),
        #[prost(message, tag = "5")]
        MidiInputDevices(super::MidiInputDevices),
        #[prost(message, tag = "6")]
        AudioInputChannels(super::AudioInputChannels),
        #[prost(string, tag = "7")]
        PersistentState(::prost::alloc::string::String),
        #[prost(message, tag = "8")]
        HistoryState(super::HistoryState),
        #[prost(bool, tag = "9")]
        ClickEnabled(bool),
        #[prost(message, tag = "10")]
        TimeSignature(super::TimeSignature),
    }
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct HistoryState {
    #[prost(string, tag = "1")]
    pub undo_label: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub redo_label: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TimeSignature {
    #[prost(uint32, tag = "1")]
    pub numerator: u32,
    #[prost(uint32, tag = "2")]
    pub denominator: u32,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct OccasionalTrackUpdate {
    #[prost(
        oneof = "occasional_track_update::Update",
        tags = "1, 2, 3, 4, 5, 6, 7, 8, 9, 10"
    )]
    pub update: ::core::option::Option<occasional_track_update::Update>,
}
/// Nested message and enum types in `OccasionalTrackUpdate`.
pub mod occasional_track_update {
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Update {
        #[prost(string, tag = "1")]
        Name(::prost::alloc::string::String),
        #[prost(message, tag = "2")]
        Color(super::TrackColor),
        #[prost(message, tag = "3")]
        Input(super::TrackInput),
        #[prost(bool, tag = "4")]
        Armed(bool),
        #[prost(enumeration = "super::TrackInputMonitoring", tag = "5")]
        InputMonitoring(i32),
        #[prost(bool, tag = "6")]
        Mute(bool),
        #[prost(bool, tag = "7")]
        Solo(bool),
        #[prost(bool, tag = "8")]
        Selected(bool),
        #[prost(double, tag = "9")]
        Volume(f64),
        #[prost(double, tag = "10")]
        Pan(f64),
    }
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TrackColor {
    #[prost(int32, optional, tag = "1")]
    pub color: ::core::option::Option<i32>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TrackInput {
    #[prost(oneof = "track_input::Input", tags = "1, 2, 3")]
    pub input: ::core::option::Option<track_input::Input>,
}
/// Nested message and enum types in `TrackInput`.
pub mod track_input {
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Input {
        #[prost(uint32, tag = "1")]
        Mono(u32),
        #[prost(uint32, tag = "2")]
        Stereo(u32),
        #[prost(message, tag = "3")]
        Midi(super::TrackMidiInput),
    }
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TrackMidiInput {
    #[prost(uint32, optional, tag = "1")]
    pub device: ::core::option::Option<u32>,
    #[prost(uint32, optional, tag = "2")]
    pub channel: ::core::option::Option<u32>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MidiInputDevices {
    #[prost(message, repeated, tag = "1")]
    pub devices: ::prost::alloc::vec::Vec<MidiInputDevice>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MidiInputDevice {
    #[prost(uint32, tag = "1")]
    pub id: u32,
    #[prost(string, tag = "2")]
    pub name: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AudioInputChannels {
    #[prost(message, repeated, tag = "1")]
    pub channels: ::prost::alloc::vec::Vec<AudioInputChannel>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AudioInputChannel {
    #[prost(uint32, tag = "1")]
    pub index: u32,
    #[prost(string, tag = "2")]
    pub name: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QualifiedOccasionalSlotUpdate {
    #[prost(message, optional, tag = "1")]
    pub slot_coordinates: ::core::option::Option<SlotCoordinates>,
    #[prost(oneof = "qualified_occasional_slot_update::Update", tags = "2, 3")]
    pub update: ::core::option::Option<qualified_occasional_slot_update::Update>,
}
/// Nested message and enum types in `QualifiedOccasionalSlotUpdate`.
pub mod qualified_occasional_slot_update {
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Update {
        #[prost(enumeration = "super::SlotPlayState", tag = "2")]
        PlayState(i32),
        #[prost(string, tag = "3")]
        PersistentState(::prost::alloc::string::String),
    }
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ContinuousSlotUpdate {
    #[prost(message, optional, tag = "1")]
    pub clip_update: ::core::option::Option<ContinuousClipUpdate>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ContinuousClipUpdate {
    /// Number between 0 and 1, interpretable as percentage.
    #[prost(double, tag = "1")]
    pub proportional_position: f64,
    #[prost(double, tag = "2")]
    pub position_in_seconds: f64,
    #[prost(double, tag = "3")]
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
pub enum TriggerMatrixAction {
    ArrangementTogglePlayStop = 0,
    StopAllClips = 1,
    ArrangementPlay = 2,
    ArrangementStop = 3,
    ArrangementPause = 4,
    ArrangementStartRecording = 5,
    ArrangementStopRecording = 6,
    Undo = 7,
    Redo = 8,
    ToggleClick = 9,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum TriggerColumnAction {
    Stop = 0,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum TriggerRowAction {
    Play = 0,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum TriggerSlotAction {
    Play = 0,
    Stop = 1,
    Record = 2,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum TrackInputMonitoring {
    Unknown = 0,
    Off = 1,
    Normal = 2,
    TapeStyle = 3,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum SlotPlayState {
    Unknown = 0,
    Stopped = 1,
    ScheduledForPlayStart = 2,
    Playing = 3,
    Paused = 4,
    ScheduledForPlayStop = 5,
    ScheduledForRecordingStart = 6,
    Recording = 7,
    ScheduledForRecordingStop = 8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum ArrangementPlayState {
    Unknown = 0,
    Stopped = 1,
    Playing = 2,
    PlayingPaused = 3,
    Recording = 4,
    RecordingPaused = 5,
}
#[doc = r" Generated server implementations."]
pub mod clip_engine_server {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::*;
    #[doc = "Generated trait containing gRPC methods that should be implemented for use with ClipEngineServer."]
    #[async_trait]
    pub trait ClipEngine: Send + Sync + 'static {
        #[doc = " Matrix commands"]
        async fn trigger_matrix(
            &self,
            request: tonic::Request<super::TriggerMatrixRequest>,
        ) -> Result<tonic::Response<super::Empty>, tonic::Status>;
        async fn set_matrix_tempo(
            &self,
            request: tonic::Request<super::SetMatrixTempoRequest>,
        ) -> Result<tonic::Response<super::Empty>, tonic::Status>;
        async fn set_matrix_volume(
            &self,
            request: tonic::Request<super::SetMatrixVolumeRequest>,
        ) -> Result<tonic::Response<super::Empty>, tonic::Status>;
        async fn set_matrix_pan(
            &self,
            request: tonic::Request<super::SetMatrixPanRequest>,
        ) -> Result<tonic::Response<super::Empty>, tonic::Status>;
        #[doc = " Column commands"]
        async fn trigger_column(
            &self,
            request: tonic::Request<super::TriggerColumnRequest>,
        ) -> Result<tonic::Response<super::Empty>, tonic::Status>;
        async fn set_column_volume(
            &self,
            request: tonic::Request<super::SetColumnVolumeRequest>,
        ) -> Result<tonic::Response<super::Empty>, tonic::Status>;
        #[doc = " Row commands"]
        async fn trigger_row(
            &self,
            request: tonic::Request<super::TriggerRowRequest>,
        ) -> Result<tonic::Response<super::Empty>, tonic::Status>;
        #[doc = " Slot commands"]
        async fn trigger_slot(
            &self,
            request: tonic::Request<super::TriggerSlotRequest>,
        ) -> Result<tonic::Response<super::Empty>, tonic::Status>;
        async fn set_clip_name(
            &self,
            request: tonic::Request<super::SetClipNameRequest>,
        ) -> Result<tonic::Response<super::Empty>, tonic::Status>;
        #[doc = "Server streaming response type for the GetOccasionalMatrixUpdates method."]
        type GetOccasionalMatrixUpdatesStream: futures_core::Stream<
                Item = Result<super::GetOccasionalMatrixUpdatesReply, tonic::Status>,
            > + Send
            + Sync
            + 'static;
        #[doc = " Occasional events"]
        async fn get_occasional_matrix_updates(
            &self,
            request: tonic::Request<super::GetOccasionalMatrixUpdatesRequest>,
        ) -> Result<tonic::Response<Self::GetOccasionalMatrixUpdatesStream>, tonic::Status>;
        #[doc = "Server streaming response type for the GetOccasionalTrackUpdates method."]
        type GetOccasionalTrackUpdatesStream: futures_core::Stream<
                Item = Result<super::GetOccasionalTrackUpdatesReply, tonic::Status>,
            > + Send
            + Sync
            + 'static;
        async fn get_occasional_track_updates(
            &self,
            request: tonic::Request<super::GetOccasionalTrackUpdatesRequest>,
        ) -> Result<tonic::Response<Self::GetOccasionalTrackUpdatesStream>, tonic::Status>;
        #[doc = "Server streaming response type for the GetOccasionalSlotUpdates method."]
        type GetOccasionalSlotUpdatesStream: futures_core::Stream<Item = Result<super::GetOccasionalSlotUpdatesReply, tonic::Status>>
            + Send
            + Sync
            + 'static;
        async fn get_occasional_slot_updates(
            &self,
            request: tonic::Request<super::GetOccasionalSlotUpdatesRequest>,
        ) -> Result<tonic::Response<Self::GetOccasionalSlotUpdatesStream>, tonic::Status>;
        #[doc = "Server streaming response type for the GetContinuousMatrixUpdates method."]
        type GetContinuousMatrixUpdatesStream: futures_core::Stream<
                Item = Result<super::GetContinuousMatrixUpdatesReply, tonic::Status>,
            > + Send
            + Sync
            + 'static;
        #[doc = " Continuous events"]
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
                "/playtime.clip_engine.ClipEngine/TriggerMatrix" => {
                    #[allow(non_camel_case_types)]
                    struct TriggerMatrixSvc<T: ClipEngine>(pub Arc<T>);
                    impl<T: ClipEngine> tonic::server::UnaryService<super::TriggerMatrixRequest>
                        for TriggerMatrixSvc<T>
                    {
                        type Response = super::Empty;
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::TriggerMatrixRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { (*inner).trigger_matrix(request).await };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = TriggerMatrixSvc(inner);
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
                "/playtime.clip_engine.ClipEngine/SetMatrixTempo" => {
                    #[allow(non_camel_case_types)]
                    struct SetMatrixTempoSvc<T: ClipEngine>(pub Arc<T>);
                    impl<T: ClipEngine> tonic::server::UnaryService<super::SetMatrixTempoRequest>
                        for SetMatrixTempoSvc<T>
                    {
                        type Response = super::Empty;
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::SetMatrixTempoRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { (*inner).set_matrix_tempo(request).await };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = SetMatrixTempoSvc(inner);
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
                "/playtime.clip_engine.ClipEngine/SetMatrixVolume" => {
                    #[allow(non_camel_case_types)]
                    struct SetMatrixVolumeSvc<T: ClipEngine>(pub Arc<T>);
                    impl<T: ClipEngine> tonic::server::UnaryService<super::SetMatrixVolumeRequest>
                        for SetMatrixVolumeSvc<T>
                    {
                        type Response = super::Empty;
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::SetMatrixVolumeRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { (*inner).set_matrix_volume(request).await };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = SetMatrixVolumeSvc(inner);
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
                "/playtime.clip_engine.ClipEngine/SetMatrixPan" => {
                    #[allow(non_camel_case_types)]
                    struct SetMatrixPanSvc<T: ClipEngine>(pub Arc<T>);
                    impl<T: ClipEngine> tonic::server::UnaryService<super::SetMatrixPanRequest> for SetMatrixPanSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::SetMatrixPanRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { (*inner).set_matrix_pan(request).await };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = SetMatrixPanSvc(inner);
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
                "/playtime.clip_engine.ClipEngine/TriggerColumn" => {
                    #[allow(non_camel_case_types)]
                    struct TriggerColumnSvc<T: ClipEngine>(pub Arc<T>);
                    impl<T: ClipEngine> tonic::server::UnaryService<super::TriggerColumnRequest>
                        for TriggerColumnSvc<T>
                    {
                        type Response = super::Empty;
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::TriggerColumnRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { (*inner).trigger_column(request).await };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = TriggerColumnSvc(inner);
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
                "/playtime.clip_engine.ClipEngine/SetColumnVolume" => {
                    #[allow(non_camel_case_types)]
                    struct SetColumnVolumeSvc<T: ClipEngine>(pub Arc<T>);
                    impl<T: ClipEngine> tonic::server::UnaryService<super::SetColumnVolumeRequest>
                        for SetColumnVolumeSvc<T>
                    {
                        type Response = super::Empty;
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::SetColumnVolumeRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { (*inner).set_column_volume(request).await };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = SetColumnVolumeSvc(inner);
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
                "/playtime.clip_engine.ClipEngine/TriggerRow" => {
                    #[allow(non_camel_case_types)]
                    struct TriggerRowSvc<T: ClipEngine>(pub Arc<T>);
                    impl<T: ClipEngine> tonic::server::UnaryService<super::TriggerRowRequest> for TriggerRowSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::TriggerRowRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { (*inner).trigger_row(request).await };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = TriggerRowSvc(inner);
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
                "/playtime.clip_engine.ClipEngine/TriggerSlot" => {
                    #[allow(non_camel_case_types)]
                    struct TriggerSlotSvc<T: ClipEngine>(pub Arc<T>);
                    impl<T: ClipEngine> tonic::server::UnaryService<super::TriggerSlotRequest> for TriggerSlotSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::TriggerSlotRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { (*inner).trigger_slot(request).await };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = TriggerSlotSvc(inner);
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
                "/playtime.clip_engine.ClipEngine/SetClipName" => {
                    #[allow(non_camel_case_types)]
                    struct SetClipNameSvc<T: ClipEngine>(pub Arc<T>);
                    impl<T: ClipEngine> tonic::server::UnaryService<super::SetClipNameRequest> for SetClipNameSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::SetClipNameRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { (*inner).set_clip_name(request).await };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = SetClipNameSvc(inner);
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
                "/playtime.clip_engine.ClipEngine/GetOccasionalMatrixUpdates" => {
                    #[allow(non_camel_case_types)]
                    struct GetOccasionalMatrixUpdatesSvc<T: ClipEngine>(pub Arc<T>);
                    impl<T: ClipEngine>
                        tonic::server::ServerStreamingService<
                            super::GetOccasionalMatrixUpdatesRequest,
                        > for GetOccasionalMatrixUpdatesSvc<T>
                    {
                        type Response = super::GetOccasionalMatrixUpdatesReply;
                        type ResponseStream = T::GetOccasionalMatrixUpdatesStream;
                        type Future =
                            BoxFuture<tonic::Response<Self::ResponseStream>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::GetOccasionalMatrixUpdatesRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move {
                                (*inner).get_occasional_matrix_updates(request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = GetOccasionalMatrixUpdatesSvc(inner);
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
                "/playtime.clip_engine.ClipEngine/GetOccasionalTrackUpdates" => {
                    #[allow(non_camel_case_types)]
                    struct GetOccasionalTrackUpdatesSvc<T: ClipEngine>(pub Arc<T>);
                    impl<T: ClipEngine>
                        tonic::server::ServerStreamingService<
                            super::GetOccasionalTrackUpdatesRequest,
                        > for GetOccasionalTrackUpdatesSvc<T>
                    {
                        type Response = super::GetOccasionalTrackUpdatesReply;
                        type ResponseStream = T::GetOccasionalTrackUpdatesStream;
                        type Future =
                            BoxFuture<tonic::Response<Self::ResponseStream>, tonic::Status>;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::GetOccasionalTrackUpdatesRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut =
                                async move { (*inner).get_occasional_track_updates(request).await };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = GetOccasionalTrackUpdatesSvc(inner);
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
