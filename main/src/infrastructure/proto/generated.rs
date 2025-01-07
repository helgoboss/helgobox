/// Only necessary for in-process communication (without gRPC).
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Request {
    /// This should be the instance ID of the app instance sending this request.
    /// Although the instance ID is encoded in many of the actual requests below, having the
    /// instance ID here at one place makes it easier for us to send error messages back to the correct app instance.
    #[prost(uint32, tag = "1")]
    pub instance_id: u32,
    #[prost(oneof = "request::Value", tags = "2, 3")]
    pub value: ::core::option::Option<request::Value>,
}
/// Nested message and enum types in `Request`.
pub mod request {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Value {
        #[prost(message, tag = "2")]
        CommandRequest(super::CommandRequest),
        #[prost(message, tag = "3")]
        QueryRequest(super::QueryRequest),
    }
}
/// Only necessary for in-process communication (without gRPC).
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Reply {
    #[prost(oneof = "reply::Value", tags = "1, 2")]
    pub value: ::core::option::Option<reply::Value>,
}
/// Nested message and enum types in `Reply`.
pub mod reply {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Value {
        #[prost(message, tag = "1")]
        EventReply(super::EventReply),
        #[prost(message, tag = "2")]
        QueryReply(super::QueryReply),
    }
}
/// Should contain all possible *command* requests (without return value) from above service.
///
/// Only necessary for in-process communication (without gRPC).
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CommandRequest {
    #[prost(
        oneof = "command_request::Value",
        tags = "1, 54, 3, 4, 5, 52, 6, 7, 8, 9, 10, 11, 50, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 37, 53, 40, 25, 26, 27, 34, 28, 29, 31, 32, 33, 35, 36, 38, 39, 41, 42, 43, 44, 51, 45, 46, 47, 48, 49"
    )]
    pub value: ::core::option::Option<command_request::Value>,
}
/// Nested message and enum types in `CommandRequest`.
pub mod command_request {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Value {
        /// Embedding
        #[prost(message, tag = "1")]
        NotifyAppIsReady(super::NotifyAppIsReadyRequest),
        /// This is necessary on Windows so that REAPER doesn't execute global shortcuts when the
        /// App is in text entry mode.
        #[prost(message, tag = "54")]
        SetAppIsInTextEntryMode(super::SetAppIsInTextEntryModeRequest),
        /// Normal commands
        #[prost(message, tag = "3")]
        TriggerMatrix(super::TriggerMatrixRequest),
        #[prost(message, tag = "4")]
        SetMatrixSettings(super::SetMatrixSettingsRequest),
        #[prost(message, tag = "5")]
        SetMatrixTempo(super::SetMatrixTempoRequest),
        #[prost(message, tag = "52")]
        SetMatrixPlayRate(super::SetMatrixPlayRateRequest),
        #[prost(message, tag = "6")]
        SetMatrixVolume(super::SetMatrixVolumeRequest),
        #[prost(message, tag = "7")]
        SetMatrixPan(super::SetMatrixPanRequest),
        #[prost(message, tag = "8")]
        TriggerColumn(super::TriggerColumnRequest),
        #[prost(message, tag = "9")]
        SetColumnSettings(super::SetColumnSettingsRequest),
        #[prost(message, tag = "10")]
        SetTrackVolume(super::SetTrackVolumeRequest),
        #[prost(message, tag = "11")]
        SetTrackPan(super::SetTrackPanRequest),
        #[prost(message, tag = "50")]
        OpenTrackFx(super::OpenTrackFxRequest),
        #[prost(message, tag = "12")]
        SetColumnTrack(super::SetColumnTrackRequest),
        #[prost(message, tag = "13")]
        DragColumn(super::DragColumnRequest),
        #[prost(message, tag = "14")]
        SetTrackName(super::SetTrackNameRequest),
        #[prost(message, tag = "15")]
        SetTrackInput(super::SetTrackInputRequest),
        #[prost(message, tag = "16")]
        SetTrackInputMonitoring(super::SetTrackInputMonitoringRequest),
        #[prost(message, tag = "17")]
        TriggerRow(super::TriggerRowRequest),
        #[prost(message, tag = "18")]
        SetRowData(super::SetRowDataRequest),
        #[prost(message, tag = "19")]
        DragRow(super::DragRowRequest),
        #[prost(message, tag = "20")]
        TriggerSlot(super::TriggerSlotRequest),
        #[prost(message, tag = "21")]
        DragSlot(super::DragSlotRequest),
        #[prost(message, tag = "22")]
        TriggerClip(super::TriggerClipRequest),
        #[prost(message, tag = "23")]
        SetClipName(super::SetClipNameRequest),
        #[prost(message, tag = "24")]
        SetClipData(super::SetClipDataRequest),
        /// Event re-subscription commands (only for occasional aggregate events, the rest will be sent anyway)
        #[prost(message, tag = "37")]
        GetOccasionalGlobalUpdates(super::GetOccasionalGlobalUpdatesRequest),
        #[prost(message, tag = "53")]
        GetOccasionalPlaytimeEngineUpdates(
            super::GetOccasionalPlaytimeEngineUpdatesRequest,
        ),
        #[prost(message, tag = "40")]
        GetOccasionalInstanceUpdates(super::GetOccasionalInstanceUpdatesRequest),
        #[prost(message, tag = "25")]
        GetOccasionalMatrixUpdates(super::GetOccasionalMatrixUpdatesRequest),
        #[prost(message, tag = "26")]
        GetOccasionalTrackUpdates(super::GetOccasionalTrackUpdatesRequest),
        #[prost(message, tag = "27")]
        GetOccasionalSlotUpdates(super::GetOccasionalSlotUpdatesRequest),
        #[prost(message, tag = "34")]
        GetOccasionalClipUpdates(super::GetOccasionalClipUpdatesRequest),
        #[prost(message, tag = "28")]
        TriggerTrack(super::TriggerTrackRequest),
        #[prost(message, tag = "29")]
        SetMatrixTimeSignature(super::SetMatrixTimeSignatureRequest),
        #[prost(message, tag = "31")]
        SetTrackColor(super::SetTrackColorRequest),
        #[prost(message, tag = "32")]
        DragClip(super::DragClipRequest),
        #[prost(message, tag = "33")]
        ImportFiles(super::ImportFilesRequest),
        #[prost(message, tag = "35")]
        TriggerSequence(super::TriggerSequenceRequest),
        #[prost(message, tag = "36")]
        SetSequenceInfo(super::SetSequenceInfoRequest),
        #[prost(message, tag = "38")]
        SaveController(super::SaveControllerRequest),
        #[prost(message, tag = "39")]
        DeleteController(super::DeleteControllerRequest),
        #[prost(message, tag = "41")]
        SetInstanceSettings(super::SetInstanceSettingsRequest),
        #[prost(message, tag = "42")]
        AddLicense(super::AddLicenseRequest),
        #[prost(message, tag = "43")]
        SetAppSettings(super::SetAppSettingsRequest),
        #[prost(message, tag = "44")]
        SaveCustomCompartmentData(super::SaveCustomCompartmentDataRequest),
        #[prost(message, tag = "51")]
        SetCustomInstanceData(super::SetCustomInstanceDataRequest),
        #[prost(message, tag = "45")]
        GetOccasionalUnitUpdates(super::GetOccasionalUnitUpdatesRequest),
        #[prost(message, tag = "46")]
        TriggerGlobal(super::TriggerGlobalRequest),
        #[prost(message, tag = "47")]
        InsertColumns(super::InsertColumnsRequest),
        #[prost(message, tag = "48")]
        TriggerInstance(super::TriggerInstanceRequest),
        #[prost(message, tag = "49")]
        SetPlaytimeEngineSettings(super::SetPlaytimeEngineSettingsRequest),
    }
}
/// Envelope for queries.
///
/// Only necessary for in-process communication (without gRPC).
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryRequest {
    /// / Second part of composite ID for being able to associate replies with requests.
    /// / The first part is the instance ID. The second part is specific to queries, it's not
    /// / necessary for commands.
    #[prost(uint32, tag = "1")]
    pub id: u32,
    #[prost(message, optional, tag = "2")]
    pub query: ::core::option::Option<Query>,
}
/// Should contain all possible *query* requests (with return value) from above service.
///
/// Only necessary for in-process communication (without gRPC).
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Query {
    #[prost(oneof = "query::Value", tags = "1, 2, 3, 4, 5, 6, 7, 8")]
    pub value: ::core::option::Option<query::Value>,
}
/// Nested message and enum types in `Query`.
pub mod query {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Value {
        #[prost(message, tag = "1")]
        ProveAuthenticity(super::ProveAuthenticityRequest),
        #[prost(message, tag = "2")]
        GetClipDetail(super::GetClipDetailRequest),
        #[prost(message, tag = "3")]
        GetProjectDir(super::GetProjectDirRequest),
        #[prost(message, tag = "4")]
        GetHostInfo(super::GetHostInfoRequest),
        #[prost(message, tag = "5")]
        GetArrangementInfo(super::GetArrangementInfoRequest),
        #[prost(message, tag = "6")]
        GetAppSettings(super::GetAppSettingsRequest),
        #[prost(message, tag = "7")]
        GetCompartmentData(super::GetCompartmentDataRequest),
        #[prost(message, tag = "8")]
        GetCustomInstanceData(super::GetCustomInstanceDataRequest),
    }
}
/// Envelope for query results.
///
/// Only necessary for in-process communication (without gRPC).
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryReply {
    /// / ID for being able to associate replies with requests.
    #[prost(uint32, tag = "1")]
    pub id: u32,
    #[prost(message, optional, tag = "2")]
    pub result: ::core::option::Option<QueryResult>,
}
/// Should contain all possible *query* results (with return value) from above service.
///
/// Only necessary for in-process communication (without gRPC).
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryResult {
    #[prost(oneof = "query_result::Value", tags = "1, 2, 3, 4, 5, 6, 7, 8, 9")]
    pub value: ::core::option::Option<query_result::Value>,
}
/// Nested message and enum types in `QueryResult`.
pub mod query_result {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Value {
        #[prost(string, tag = "1")]
        Error(::prost::alloc::string::String),
        #[prost(message, tag = "2")]
        ProveAuthenticityReply(super::ProveAuthenticityReply),
        #[prost(message, tag = "3")]
        GetClipDetailReply(super::GetClipDetailReply),
        #[prost(message, tag = "4")]
        GetProjectDirReply(super::GetProjectDirReply),
        #[prost(message, tag = "5")]
        GetHostInfoReply(super::GetHostInfoReply),
        #[prost(message, tag = "6")]
        GetArrangementInfoReply(super::GetArrangementInfoReply),
        #[prost(message, tag = "7")]
        GetAppSettingsReply(super::GetAppSettingsReply),
        #[prost(message, tag = "8")]
        GetCompartmentDataReply(super::GetCompartmentDataReply),
        #[prost(message, tag = "9")]
        GetCustomInstanceDataReply(super::GetCustomInstanceDataReply),
    }
}
/// Should contain all possible *event* replies from above service.
///
/// Only necessary for in-process communication (without gRPC).
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct EventReply {
    #[prost(
        oneof = "event_reply::Value",
        tags = "1, 15, 13, 14, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12"
    )]
    pub value: ::core::option::Option<event_reply::Value>,
}
/// Nested message and enum types in `EventReply`.
pub mod event_reply {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Value {
        /// Normal events
        #[prost(message, tag = "1")]
        OccasionalGlobalUpdatesReply(super::GetOccasionalGlobalUpdatesReply),
        #[prost(message, tag = "15")]
        OccasionalPlaytimeEngineUpdatesReply(
            super::GetOccasionalPlaytimeEngineUpdatesReply,
        ),
        #[prost(message, tag = "13")]
        OccasionalInstanceUpdatesReply(super::GetOccasionalInstanceUpdatesReply),
        #[prost(message, tag = "14")]
        OccasionalUnitUpdatesReply(super::GetOccasionalUnitUpdatesReply),
        #[prost(message, tag = "2")]
        OccasionalMatrixUpdatesReply(super::GetOccasionalMatrixUpdatesReply),
        #[prost(message, tag = "3")]
        ContinuousMatrixUpdatesReply(super::GetContinuousMatrixUpdatesReply),
        #[prost(message, tag = "4")]
        OccasionalColumnUpdatesReply(super::GetOccasionalColumnUpdatesReply),
        #[prost(message, tag = "5")]
        ContinuousColumnUpdatesReply(super::GetContinuousColumnUpdatesReply),
        #[prost(message, tag = "6")]
        OccasionalTrackUpdatesReply(super::GetOccasionalTrackUpdatesReply),
        #[prost(message, tag = "7")]
        OccasionalRowUpdatesReply(super::GetOccasionalRowUpdatesReply),
        #[prost(message, tag = "8")]
        OccasionalSlotUpdatesReply(super::GetOccasionalSlotUpdatesReply),
        #[prost(message, tag = "9")]
        ContinuousSlotUpdatesReply(super::GetContinuousSlotUpdatesReply),
        #[prost(message, tag = "10")]
        OccasionalClipUpdatesReply(super::GetOccasionalClipUpdatesReply),
        /// Embedding
        /// This should be interpreted as an instruction to refetch all state again.
        #[prost(message, tag = "11")]
        Reset(super::Empty),
        /// A generic error message for the user used for returning errors that occur when processing a command
        /// (and ideally should not occur). When using gRPC, this is done using the typical gRPC error status method instead.
        #[prost(string, tag = "12")]
        ErrorMessage(::prost::alloc::string::String),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FullColumnAddress {
    #[prost(uint32, tag = "1")]
    pub matrix_id: u32,
    #[prost(uint32, tag = "2")]
    pub column_index: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FullTrackAddress {
    #[prost(uint32, tag = "1")]
    pub matrix_id: u32,
    #[prost(string, tag = "2")]
    pub track_id: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FullRowAddress {
    #[prost(uint32, tag = "1")]
    pub matrix_id: u32,
    #[prost(uint32, tag = "2")]
    pub row_index: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FullSlotAddress {
    #[prost(uint32, tag = "1")]
    pub matrix_id: u32,
    #[prost(message, optional, tag = "2")]
    pub slot_address: ::core::option::Option<SlotAddress>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FullSequenceId {
    #[prost(uint32, tag = "1")]
    pub matrix_id: u32,
    #[prost(string, tag = "2")]
    pub sequence_id: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FullClipId {
    #[prost(uint32, tag = "1")]
    pub matrix_id: u32,
    #[prost(string, tag = "2")]
    pub clip_id: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FullClipAddress {
    #[prost(uint32, tag = "1")]
    pub matrix_id: u32,
    #[prost(message, optional, tag = "2")]
    pub clip_address: ::core::option::Option<ClipAddress>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ClipAddress {
    #[prost(message, optional, tag = "1")]
    pub slot_address: ::core::option::Option<SlotAddress>,
    #[prost(uint32, tag = "2")]
    pub clip_index: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CellAddress {
    #[prost(uint32, optional, tag = "1")]
    pub column_index: ::core::option::Option<u32>,
    #[prost(uint32, optional, tag = "2")]
    pub row_index: ::core::option::Option<u32>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SlotAddress {
    #[prost(uint32, tag = "1")]
    pub column_index: u32,
    #[prost(uint32, tag = "2")]
    pub row_index: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SetMatrixTempoRequest {
    #[prost(uint32, tag = "1")]
    pub matrix_id: u32,
    #[prost(double, tag = "2")]
    pub bpm: f64,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SetMatrixPlayRateRequest {
    #[prost(uint32, tag = "1")]
    pub matrix_id: u32,
    /// Unit: Playback speed factor
    #[prost(double, tag = "2")]
    pub play_rate: f64,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SetMatrixTimeSignatureRequest {
    #[prost(uint32, tag = "1")]
    pub matrix_id: u32,
    #[prost(message, optional, tag = "2")]
    pub time_signature: ::core::option::Option<TimeSignature>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SetMatrixVolumeRequest {
    #[prost(uint32, tag = "1")]
    pub matrix_id: u32,
    #[prost(enumeration = "MatrixVolumeKind", tag = "2")]
    pub kind: i32,
    #[prost(double, tag = "3")]
    pub db: f64,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SetMatrixPanRequest {
    #[prost(uint32, tag = "1")]
    pub matrix_id: u32,
    #[prost(double, tag = "2")]
    pub pan: f64,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SetTrackVolumeRequest {
    #[prost(message, optional, tag = "1")]
    pub track_address: ::core::option::Option<FullTrackAddress>,
    #[prost(double, tag = "2")]
    pub db: f64,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SetTrackNameRequest {
    #[prost(message, optional, tag = "1")]
    pub track_address: ::core::option::Option<FullTrackAddress>,
    #[prost(string, tag = "2")]
    pub name: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SetTrackColorRequest {
    #[prost(message, optional, tag = "1")]
    pub track_address: ::core::option::Option<FullTrackAddress>,
    #[prost(message, optional, tag = "2")]
    pub color: ::core::option::Option<RgbColor>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SetTrackPanRequest {
    #[prost(message, optional, tag = "1")]
    pub track_address: ::core::option::Option<FullTrackAddress>,
    #[prost(double, tag = "2")]
    pub pan: f64,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct OpenTrackFxRequest {
    #[prost(message, optional, tag = "1")]
    pub track_address: ::core::option::Option<FullTrackAddress>,
    #[prost(uint32, tag = "2")]
    pub fx_index: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SetColumnTrackRequest {
    #[prost(message, optional, tag = "1")]
    pub column_address: ::core::option::Option<FullColumnAddress>,
    #[prost(string, optional, tag = "2")]
    pub track_id: ::core::option::Option<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SetTrackInputMonitoringRequest {
    #[prost(message, optional, tag = "1")]
    pub track_address: ::core::option::Option<FullTrackAddress>,
    #[prost(enumeration = "TrackInputMonitoring", tag = "2")]
    pub input_monitoring: i32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SetTrackInputRequest {
    #[prost(message, optional, tag = "1")]
    pub track_address: ::core::option::Option<FullTrackAddress>,
    #[prost(message, optional, tag = "2")]
    pub input: ::core::option::Option<TrackInput>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Empty {}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AddLicenseRequest {
    #[prost(string, tag = "1")]
    pub license_key: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SaveControllerRequest {
    /// Controller as JSON
    #[prost(string, tag = "1")]
    pub controller: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DeleteControllerRequest {
    #[prost(string, tag = "1")]
    pub controller_id: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TriggerGlobalRequest {
    #[prost(enumeration = "TriggerGlobalAction", tag = "1")]
    pub action: i32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TriggerInstanceRequest {
    #[prost(uint32, tag = "1")]
    pub instance_id: u32,
    #[prost(enumeration = "TriggerInstanceAction", tag = "2")]
    pub action: i32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TriggerMatrixRequest {
    #[prost(uint32, tag = "1")]
    pub matrix_id: u32,
    #[prost(enumeration = "TriggerMatrixAction", tag = "2")]
    pub action: i32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SetMatrixSettingsRequest {
    #[prost(uint32, tag = "1")]
    pub matrix_id: u32,
    /// Matrix settings as JSON
    #[prost(string, tag = "2")]
    pub settings: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NotifyAppIsReadyRequest {
    #[prost(uint32, tag = "1")]
    pub matrix_id: u32,
    #[prost(uint64, tag = "2")]
    pub app_callback_address: u64,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SetAppIsInTextEntryModeRequest {
    #[prost(uint32, tag = "1")]
    pub matrix_id: u32,
    #[prost(bool, tag = "2")]
    pub is_in_text_entry_mode: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetHostInfoRequest {}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetHostInfoReply {
    #[prost(string, tag = "1")]
    pub public_version: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub api_version: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetAppSettingsRequest {}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetCustomInstanceDataReply {
    /// Custom instance data as JSON
    #[prost(string, optional, tag = "1")]
    pub data: ::core::option::Option<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetAppSettingsReply {
    /// App settings as JSON
    #[prost(string, optional, tag = "1")]
    pub app_settings: ::core::option::Option<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SetAppSettingsRequest {
    /// App settings as JSON
    #[prost(string, tag = "1")]
    pub app_settings: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SetPlaytimeEngineSettingsRequest {
    /// Settings as JSON
    #[prost(string, tag = "1")]
    pub settings: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ProveAuthenticityRequest {
    #[prost(bytes = "vec", tag = "1")]
    pub challenge: ::prost::alloc::vec::Vec<u8>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetProjectDirRequest {
    #[prost(uint32, tag = "1")]
    pub matrix_id: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetArrangementInfoRequest {
    #[prost(uint32, tag = "1")]
    pub matrix_id: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ProveAuthenticityReply {
    #[prost(bytes = "vec", tag = "1")]
    pub signature: ::prost::alloc::vec::Vec<u8>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetProjectDirReply {
    #[prost(string, tag = "1")]
    pub project_dir: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetArrangementInfoReply {
    #[prost(bool, tag = "1")]
    pub clean: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TrackList {
    #[prost(message, repeated, tag = "1")]
    pub tracks: ::prost::alloc::vec::Vec<TrackInList>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TriggerTrackRequest {
    #[prost(message, optional, tag = "1")]
    pub track_address: ::core::option::Option<FullTrackAddress>,
    #[prost(enumeration = "TriggerTrackAction", tag = "2")]
    pub action: i32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TriggerColumnRequest {
    #[prost(message, optional, tag = "1")]
    pub column_address: ::core::option::Option<FullColumnAddress>,
    #[prost(enumeration = "TriggerColumnAction", tag = "2")]
    pub action: i32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct InsertColumnsRequest {
    #[prost(message, optional, tag = "1")]
    pub column_address: ::core::option::Option<FullColumnAddress>,
    #[prost(uint32, tag = "2")]
    pub count: u32,
    #[prost(enumeration = "ColumnKind", tag = "3")]
    pub kind: i32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SetColumnSettingsRequest {
    #[prost(message, optional, tag = "1")]
    pub column_address: ::core::option::Option<FullColumnAddress>,
    /// Column settings as JSON
    #[prost(string, tag = "2")]
    pub settings: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SetInstanceSettingsRequest {
    #[prost(uint32, tag = "1")]
    pub instance_id: u32,
    /// Instance settings as JSON
    #[prost(string, tag = "2")]
    pub settings: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetCustomInstanceDataRequest {
    #[prost(uint32, tag = "1")]
    pub instance_id: u32,
    /// The custom-data key, e.g. "app".
    #[prost(string, tag = "2")]
    pub custom_key: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SetCustomInstanceDataRequest {
    #[prost(uint32, tag = "1")]
    pub instance_id: u32,
    /// The custom-data key, e.g. "app".
    #[prost(string, tag = "2")]
    pub custom_key: ::prost::alloc::string::String,
    /// Custom instance data as JSON.
    #[prost(string, tag = "3")]
    pub custom_data: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SaveCustomCompartmentDataRequest {
    #[prost(message, optional, tag = "1")]
    pub compartment_id: ::core::option::Option<FullCompartmentId>,
    /// The custom-data key, e.g. "companion" or "playtime".
    #[prost(string, tag = "2")]
    pub custom_key: ::prost::alloc::string::String,
    /// Custom compartment data as JSON.
    #[prost(string, tag = "3")]
    pub custom_data: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FullCompartmentId {
    #[prost(uint32, tag = "1")]
    pub instance_id: u32,
    #[prost(uint32, tag = "2")]
    pub unit_id: u32,
    #[prost(enumeration = "Compartment", tag = "3")]
    pub compartment: i32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TriggerRowRequest {
    #[prost(message, optional, tag = "1")]
    pub row_address: ::core::option::Option<FullRowAddress>,
    #[prost(enumeration = "TriggerRowAction", tag = "2")]
    pub action: i32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SetRowDataRequest {
    #[prost(message, optional, tag = "1")]
    pub row_address: ::core::option::Option<FullRowAddress>,
    /// Row data as JSON
    #[prost(string, tag = "2")]
    pub data: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DragColumnRequest {
    #[prost(uint32, tag = "1")]
    pub matrix_id: u32,
    #[prost(uint32, tag = "2")]
    pub source_column_index: u32,
    #[prost(uint32, tag = "3")]
    pub destination_column_index: u32,
    #[prost(enumeration = "DragColumnAction", tag = "4")]
    pub action: i32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DragRowRequest {
    #[prost(uint32, tag = "1")]
    pub matrix_id: u32,
    #[prost(uint32, tag = "2")]
    pub source_row_index: u32,
    #[prost(uint32, tag = "3")]
    pub destination_row_index: u32,
    #[prost(enumeration = "DragRowAction", tag = "4")]
    pub action: i32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TriggerSlotRequest {
    #[prost(message, optional, tag = "1")]
    pub slot_address: ::core::option::Option<FullSlotAddress>,
    #[prost(enumeration = "TriggerSlotAction", tag = "2")]
    pub action: i32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TriggerClipRequest {
    #[prost(message, optional, tag = "1")]
    pub clip_address: ::core::option::Option<FullClipAddress>,
    #[prost(enumeration = "TriggerClipAction", tag = "2")]
    pub action: i32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TriggerSequenceRequest {
    #[prost(message, optional, tag = "1")]
    pub sequence_id: ::core::option::Option<FullSequenceId>,
    #[prost(enumeration = "TriggerSequenceAction", tag = "2")]
    pub action: i32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DragSlotRequest {
    #[prost(uint32, tag = "1")]
    pub matrix_id: u32,
    #[prost(message, optional, tag = "2")]
    pub source_slot_address: ::core::option::Option<SlotAddress>,
    #[prost(message, optional, tag = "3")]
    pub destination_slot_address: ::core::option::Option<SlotAddress>,
    #[prost(enumeration = "DragSlotAction", tag = "4")]
    pub action: i32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ImportFilesRequest {
    #[prost(message, optional, tag = "1")]
    pub slot_address: ::core::option::Option<FullSlotAddress>,
    #[prost(string, repeated, tag = "2")]
    pub files: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DragClipRequest {
    #[prost(uint32, tag = "1")]
    pub matrix_id: u32,
    #[prost(message, optional, tag = "2")]
    pub source_clip_address: ::core::option::Option<ClipAddress>,
    #[prost(message, optional, tag = "3")]
    pub destination_slot_address: ::core::option::Option<SlotAddress>,
    #[prost(enumeration = "DragClipAction", tag = "4")]
    pub action: i32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SetClipNameRequest {
    #[prost(message, optional, tag = "1")]
    pub clip_address: ::core::option::Option<FullClipAddress>,
    #[prost(string, optional, tag = "2")]
    pub name: ::core::option::Option<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SetClipDataRequest {
    #[prost(message, optional, tag = "1")]
    pub clip_address: ::core::option::Option<FullClipAddress>,
    /// Clip data as JSON
    #[prost(string, tag = "2")]
    pub data: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SetSequenceInfoRequest {
    #[prost(message, optional, tag = "1")]
    pub sequence_id: ::core::option::Option<FullSequenceId>,
    /// Sequence info as JSON
    #[prost(string, tag = "2")]
    pub data: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetCompartmentDataRequest {
    #[prost(message, optional, tag = "1")]
    pub compartment_id: ::core::option::Option<FullCompartmentId>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetClipDetailRequest {
    #[prost(message, optional, tag = "1")]
    pub clip_id: ::core::option::Option<FullClipId>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetCompartmentDataReply {
    /// Compartment data as JSON
    #[prost(string, tag = "1")]
    pub data: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetClipDetailReply {
    #[prost(bytes = "vec", optional, tag = "1")]
    pub rea_peaks: ::core::option::Option<::prost::alloc::vec::Vec<u8>>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetOccasionalGlobalUpdatesRequest {}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetOccasionalPlaytimeEngineUpdatesRequest {}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetOccasionalInstanceUpdatesRequest {
    #[prost(uint32, tag = "1")]
    pub instance_id: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetOccasionalUnitUpdatesRequest {
    #[prost(uint32, tag = "1")]
    pub instance_id: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetOccasionalMatrixUpdatesRequest {
    #[prost(uint32, tag = "1")]
    pub matrix_id: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetOccasionalTrackUpdatesRequest {
    #[prost(uint32, tag = "1")]
    pub matrix_id: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetOccasionalSlotUpdatesRequest {
    #[prost(uint32, tag = "1")]
    pub matrix_id: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetOccasionalClipUpdatesRequest {
    #[prost(uint32, tag = "1")]
    pub matrix_id: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetContinuousMatrixUpdatesRequest {
    #[prost(uint32, tag = "1")]
    pub matrix_id: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetOccasionalColumnUpdatesRequest {
    #[prost(uint32, tag = "1")]
    pub matrix_id: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetOccasionalRowUpdatesRequest {
    #[prost(uint32, tag = "1")]
    pub matrix_id: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetContinuousColumnUpdatesRequest {
    #[prost(uint32, tag = "1")]
    pub matrix_id: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetContinuousSlotUpdatesRequest {
    #[prost(uint32, tag = "1")]
    pub matrix_id: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetOccasionalGlobalUpdatesReply {
    /// For each global updated property
    #[prost(message, repeated, tag = "1")]
    pub global_updates: ::prost::alloc::vec::Vec<OccasionalGlobalUpdate>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetOccasionalPlaytimeEngineUpdatesReply {
    #[prost(message, repeated, tag = "1")]
    pub updates: ::prost::alloc::vec::Vec<OccasionalPlaytimeEngineUpdate>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetOccasionalInstanceUpdatesReply {
    /// For each updated instance property
    #[prost(message, repeated, tag = "1")]
    pub instance_updates: ::prost::alloc::vec::Vec<OccasionalInstanceUpdate>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct OccasionalInstanceUpdate {
    #[prost(oneof = "occasional_instance_update::Update", tags = "1, 2, 3, 4, 5")]
    pub update: ::core::option::Option<occasional_instance_update::Update>,
}
/// Nested message and enum types in `OccasionalInstanceUpdate`.
pub mod occasional_instance_update {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Update {
        /// Settings data as JSON.
        #[prost(string, tag = "1")]
        Settings(::prost::alloc::string::String),
        /// The set of units within the instance has changed (added or removed).
        #[prost(message, tag = "2")]
        Units(super::Units),
        /// Everything within the instance has changed (e.g. instance data load).
        #[prost(bool, tag = "3")]
        EverythingHasChanged(bool),
        /// Info event as JSON. Usually shortly displayed as toast or snack bar.
        #[prost(string, tag = "4")]
        InfoEvent(::prost::alloc::string::String),
        /// Warnings to be displayed permanently.
        #[prost(message, tag = "5")]
        Warnings(super::Warnings),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Warnings {
    #[prost(message, repeated, tag = "1")]
    pub warnings: ::prost::alloc::vec::Vec<Warning>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Warning {
    #[prost(enumeration = "Severity", tag = "1")]
    pub severity: i32,
    #[prost(string, tag = "2")]
    pub message: ::prost::alloc::string::String,
    #[prost(enumeration = "Scope", optional, tag = "3")]
    pub scope: ::core::option::Option<i32>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetOccasionalUnitUpdatesReply {
    /// For each updated unit property
    #[prost(message, repeated, tag = "1")]
    pub unit_updates: ::prost::alloc::vec::Vec<QualifiedOccasionalUnitUpdate>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QualifiedOccasionalUnitUpdate {
    #[prost(uint32, tag = "1")]
    pub unit_id: u32,
    #[prost(oneof = "qualified_occasional_unit_update::Update", tags = "2, 3")]
    pub update: ::core::option::Option<qualified_occasional_unit_update::Update>,
}
/// Nested message and enum types in `QualifiedOccasionalUnitUpdate`.
pub mod qualified_occasional_unit_update {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Update {
        /// Everything within the unit has changed (e.g. unit data load or replacing compartment).
        #[prost(bool, tag = "2")]
        EverythingHasChanged(bool),
        /// Updated controller routing (for projection).
        #[prost(string, tag = "3")]
        ControllerRouting(::prost::alloc::string::String),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Units {
    #[prost(message, repeated, tag = "1")]
    pub units: ::prost::alloc::vec::Vec<Unit>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Unit {
    #[prost(uint32, tag = "1")]
    pub id: u32,
    #[prost(string, tag = "2")]
    pub key: ::prost::alloc::string::String,
    #[prost(string, optional, tag = "3")]
    pub name: ::core::option::Option<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetOccasionalMatrixUpdatesReply {
    /// For each updated matrix property
    #[prost(message, repeated, tag = "1")]
    pub matrix_updates: ::prost::alloc::vec::Vec<OccasionalMatrixUpdate>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QualifiedOccasionalColumnUpdate {
    #[prost(uint32, tag = "1")]
    pub column_index: u32,
    #[prost(oneof = "qualified_occasional_column_update::Update", tags = "2")]
    pub update: ::core::option::Option<qualified_occasional_column_update::Update>,
}
/// Nested message and enum types in `QualifiedOccasionalColumnUpdate`.
pub mod qualified_occasional_column_update {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Update {
        /// Column settings as JSON
        #[prost(string, tag = "2")]
        Settings(::prost::alloc::string::String),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QualifiedOccasionalRowUpdate {
    #[prost(uint32, tag = "1")]
    pub row_index: u32,
    #[prost(oneof = "qualified_occasional_row_update::Update", tags = "2")]
    pub update: ::core::option::Option<qualified_occasional_row_update::Update>,
}
/// Nested message and enum types in `QualifiedOccasionalRowUpdate`.
pub mod qualified_occasional_row_update {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Update {
        /// Row data as JSON
        #[prost(string, tag = "2")]
        Data(::prost::alloc::string::String),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetOccasionalTrackUpdatesReply {
    /// For each updated column track
    #[prost(message, repeated, tag = "1")]
    pub track_updates: ::prost::alloc::vec::Vec<QualifiedOccasionalTrackUpdate>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetOccasionalSlotUpdatesReply {
    /// For each updated slot AND slot property
    #[prost(message, repeated, tag = "1")]
    pub slot_updates: ::prost::alloc::vec::Vec<QualifiedOccasionalSlotUpdate>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetOccasionalClipUpdatesReply {
    /// For each updated clip AND clip property
    #[prost(message, repeated, tag = "1")]
    pub clip_updates: ::prost::alloc::vec::Vec<QualifiedOccasionalClipUpdate>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetContinuousMatrixUpdatesReply {
    #[prost(message, optional, tag = "1")]
    pub matrix_update: ::core::option::Option<ContinuousMatrixUpdate>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetOccasionalColumnUpdatesReply {
    /// For each updated column property
    #[prost(message, repeated, tag = "1")]
    pub column_updates: ::prost::alloc::vec::Vec<QualifiedOccasionalColumnUpdate>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetOccasionalRowUpdatesReply {
    /// For each updated row property
    #[prost(message, repeated, tag = "1")]
    pub row_updates: ::prost::alloc::vec::Vec<QualifiedOccasionalRowUpdate>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetContinuousColumnUpdatesReply {
    /// For each column
    #[prost(message, repeated, tag = "1")]
    pub column_updates: ::prost::alloc::vec::Vec<ContinuousColumnUpdate>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetContinuousSlotUpdatesReply {
    /// For each updated slot
    #[prost(message, repeated, tag = "1")]
    pub slot_updates: ::prost::alloc::vec::Vec<QualifiedContinuousSlotUpdate>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ContinuousMatrixUpdate {
    #[prost(double, tag = "1")]
    pub second: f64,
    #[prost(sint32, tag = "2")]
    pub bar: i32,
    #[prost(double, tag = "3")]
    pub beat: f64,
    /// Numbers between 0 and 1.
    #[prost(double, repeated, tag = "4")]
    pub peaks: ::prost::alloc::vec::Vec<f64>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ContinuousColumnUpdate {
    /// Numbers between 0 and 1.
    #[prost(double, repeated, tag = "1")]
    pub peaks: ::prost::alloc::vec::Vec<f64>,
    /// / Current play lookahead in seconds (due to PDC primarily).
    #[prost(double, tag = "2")]
    pub play_lookahead: f64,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QualifiedContinuousSlotUpdate {
    #[prost(message, optional, tag = "1")]
    pub slot_address: ::core::option::Option<SlotAddress>,
    #[prost(message, optional, tag = "2")]
    pub update: ::core::option::Option<ContinuousSlotUpdate>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QualifiedOccasionalTrackUpdate {
    #[prost(string, tag = "1")]
    pub track_id: ::prost::alloc::string::String,
    /// For each updated track property
    #[prost(message, repeated, tag = "2")]
    pub track_updates: ::prost::alloc::vec::Vec<OccasionalTrackUpdate>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct OccasionalPlaytimeEngineUpdate {
    #[prost(oneof = "occasional_playtime_engine_update::Update", tags = "1, 2, 3")]
    pub update: ::core::option::Option<occasional_playtime_engine_update::Update>,
}
/// Nested message and enum types in `OccasionalPlaytimeEngineUpdate`.
pub mod occasional_playtime_engine_update {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Update {
        /// Engine settings as JSON
        #[prost(string, tag = "1")]
        EngineSettings(::prost::alloc::string::String),
        /// Contains the license state of Playtime.
        #[prost(message, tag = "2")]
        LicenseState(super::LicenseState),
        /// Engine stats.
        #[prost(message, tag = "3")]
        EngineStats(super::PlaytimeEngineStats),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PlaytimeEngineStats {
    #[prost(uint32, tag = "1")]
    pub min_buffered_blocks: u32,
    #[prost(uint32, tag = "2")]
    pub avg_buffered_blocks: u32,
    #[prost(uint32, tag = "3")]
    pub max_buffered_blocks: u32,
    #[prost(uint32, tag = "4")]
    pub future_size_in_blocks: u32,
    #[prost(uint32, tag = "6")]
    pub num_pre_buffer_fallbacks: u32,
    #[prost(uint32, tag = "5")]
    pub num_pre_buffer_misses: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct OccasionalGlobalUpdate {
    #[prost(
        oneof = "occasional_global_update::Update",
        tags = "1, 2, 3, 4, 5, 6, 7, 8, 10, 11, 12, 13, 14, 15"
    )]
    pub update: ::core::option::Option<occasional_global_update::Update>,
}
/// Nested message and enum types in `OccasionalGlobalUpdate`.
pub mod occasional_global_update {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Update {
        /// Global controller config as JSON
        #[prost(string, tag = "1")]
        ControllerConfig(::prost::alloc::string::String),
        /// MIDI input devices (= REAPER MIDI input devices)
        #[prost(message, tag = "2")]
        MidiInputDevices(super::MidiInputDevices),
        /// MIDI output devices (= REAPER MIDI output devices)
        #[prost(message, tag = "3")]
        MidiOutputDevices(super::MidiOutputDevices),
        /// Controller presets as JSON
        #[prost(string, tag = "4")]
        ControllerPresets(::prost::alloc::string::String),
        /// Main presets as JSON
        #[prost(string, tag = "5")]
        MainPresets(::prost::alloc::string::String),
        /// Audio input channels (= REAPER hardware input channels)
        #[prost(message, tag = "6")]
        AudioInputChannels(super::AudioInputChannels),
        /// Info event as JSON.
        #[prost(string, tag = "7")]
        InfoEvent(::prost::alloc::string::String),
        /// License info as JSON.
        #[prost(string, tag = "8")]
        LicenseInfo(::prost::alloc::string::String),
        /// Arrangement play state (= REAPER transport play state)
        #[prost(enumeration = "super::ArrangementPlayState", tag = "10")]
        ArrangementPlayState(i32),
        /// Requests the app to change the location (route).
        #[prost(string, tag = "11")]
        GoToLocation(::prost::alloc::string::String),
        /// Color scheme of the host.
        #[prost(message, tag = "12")]
        HostColorScheme(super::HostColorScheme),
        /// Resample modes.
        #[prost(message, tag = "13")]
        ResampleModes(super::ResampleModes),
        /// Pitch-shift modes.
        #[prost(message, tag = "14")]
        PitchShiftModes(super::PitchShiftModes),
        /// Helgobox instances.
        #[prost(message, tag = "15")]
        Instances(super::HelgoboxInstances),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct HostColorScheme {
    #[prost(map = "string, message", tag = "1")]
    pub colors: ::std::collections::HashMap<::prost::alloc::string::String, RgbColor>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LicenseState {
    /// If there's a valid license, this contains the license payload as JSON, otherwise not set.
    #[prost(string, optional, tag = "1")]
    pub license_payload: ::core::option::Option<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct OccasionalMatrixUpdate {
    #[prost(
        oneof = "occasional_matrix_update::Update",
        tags = "1, 2, 3, 27, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26"
    )]
    pub update: ::core::option::Option<occasional_matrix_update::Update>,
}
/// Nested message and enum types in `OccasionalMatrixUpdate`.
pub mod occasional_matrix_update {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Update {
        /// Matrix volume (= REAPER master track volume)
        #[prost(double, tag = "1")]
        MasterVolume(f64),
        /// Matrix pan (= REAPER master track pan)
        #[prost(double, tag = "2")]
        Pan(f64),
        /// Matrix tempo (= REAPER master tempo)
        #[prost(double, tag = "3")]
        Tempo(f64),
        /// Matrix play rate (= REAPER master play rate)
        /// Unit: Playback speed factor
        #[prost(double, tag = "27")]
        PlayRate(f64),
        /// Complete persistent data of the matrix has changed, including topology and other settings!
        /// This contains the complete persistent matrix as JSON.
        ///
        /// This is ONLY sent as initial event. Whenever "everything_has_changed" is fired, this
        /// should probably be refetched by the client by resubscribing to occasional matrix updates.
        #[prost(string, tag = "7")]
        CompletePersistentData(::prost::alloc::string::String),
        /// Playtime matrix history state
        #[prost(message, tag = "8")]
        HistoryState(super::HistoryState),
        /// Click on/off
        #[prost(bool, tag = "9")]
        ClickEnabled(bool),
        /// Time signature (= REAPER master time signature)
        #[prost(message, tag = "10")]
        TimeSignature(super::TimeSignature),
        /// Settings data as JSON.
        #[prost(string, tag = "11")]
        Settings(::prost::alloc::string::String),
        /// Matrix mute on/off
        #[prost(bool, tag = "12")]
        Mute(bool),
        /// List of all tracks
        #[prost(message, tag = "13")]
        TrackList(super::TrackList),
        /// Whether matrix exists at all
        #[prost(bool, tag = "14")]
        MatrixExists(bool),
        /// Whether silence mode is active (= Playtime is stopped)
        #[prost(bool, tag = "15")]
        SilenceMode(bool),
        /// Sequencer play state (= transport state of matrix sequencer)
        #[prost(enumeration = "super::SequencerPlayState", tag = "16")]
        SequencerPlayState(i32),
        /// Sequencer as JSON.
        #[prost(string, tag = "17")]
        Sequencer(::prost::alloc::string::String),
        /// Info event as JSON.
        #[prost(string, tag = "18")]
        InfoEvent(::prost::alloc::string::String),
        /// Simple mappings as JSON.
        #[prost(string, tag = "19")]
        SimpleMappingContainer(::prost::alloc::string::String),
        /// Learn state
        #[prost(message, tag = "20")]
        LearnState(super::LearnState),
        /// Active cell
        #[prost(message, tag = "21")]
        ActiveCell(super::CellAddress),
        /// Control units as JSON.
        #[prost(string, tag = "22")]
        ControlUnitConfig(::prost::alloc::string::String),
        /// Metronome volume
        #[prost(double, tag = "23")]
        ClickVolume(f64),
        /// Volume of the temp-tap sound
        #[prost(double, tag = "24")]
        TempoTapVolume(f64),
        /// This is true if Playtime attempted to load a non-empty matrix but ultimately didn't
        /// load it because it's not licensed.
        #[prost(bool, tag = "25")]
        HasUnloadedContent(bool),
        /// This is sent whenever the complete matrix has changed but NOT as initial event. The client can decide itself
        /// which streams to resubscribe to. Previously we sent complete_persistent_data instead and the
        /// client decided what to resubscribe. However, this is not just sent on changes but also
        /// initially, which is a problem if the client wants to resubscribe to occasional matrix updates
        /// itself - he would run into a infinite loop.
        #[prost(bool, tag = "26")]
        EverythingHasChanged(bool),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LearnState {
    /// If not set, it means that no target is learning.
    #[prost(string, optional, tag = "1")]
    pub simple_mapping_target: ::core::option::Option<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct HistoryState {
    #[prost(string, tag = "1")]
    pub undo_label: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub redo_label: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TimeSignature {
    #[prost(uint32, tag = "1")]
    pub numerator: u32,
    #[prost(uint32, tag = "2")]
    pub denominator: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct OccasionalTrackUpdate {
    #[prost(
        oneof = "occasional_track_update::Update",
        tags = "1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11"
    )]
    pub update: ::core::option::Option<occasional_track_update::Update>,
}
/// Nested message and enum types in `OccasionalTrackUpdate`.
pub mod occasional_track_update {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Update {
        /// Track name
        #[prost(string, tag = "1")]
        Name(::prost::alloc::string::String),
        /// Track color
        #[prost(message, tag = "2")]
        Color(super::RgbColor),
        /// Track recording input
        #[prost(message, tag = "3")]
        Input(super::TrackInput),
        /// Track record-arm on/off
        #[prost(bool, tag = "4")]
        Armed(bool),
        /// Track recording input monitoring setting
        #[prost(enumeration = "super::TrackInputMonitoring", tag = "5")]
        InputMonitoring(i32),
        /// Track mute on/off
        #[prost(bool, tag = "6")]
        Mute(bool),
        /// Track solo on/off
        #[prost(bool, tag = "7")]
        Solo(bool),
        /// Track selected or not
        #[prost(bool, tag = "8")]
        Selected(bool),
        /// Track volume
        #[prost(double, tag = "9")]
        Volume(f64),
        /// Track pan
        #[prost(double, tag = "10")]
        Pan(f64),
        /// Normal FX chain
        #[prost(message, tag = "11")]
        NormalFxChain(super::FxChain),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FxChain {
    #[prost(message, repeated, tag = "1")]
    pub fxs: ::prost::alloc::vec::Vec<Fx>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Fx {
    #[prost(string, tag = "1")]
    pub name: ::prost::alloc::string::String,
    #[prost(bool, tag = "2")]
    pub instrument: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RgbColor {
    #[prost(int32, optional, tag = "1")]
    pub color: ::core::option::Option<i32>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TrackInput {
    #[prost(oneof = "track_input::Input", tags = "1, 2, 4, 5, 3")]
    pub input: ::core::option::Option<track_input::Input>,
}
/// Nested message and enum types in `TrackInput`.
pub mod track_input {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Input {
        #[prost(uint32, tag = "1")]
        Mono(u32),
        #[prost(uint32, tag = "2")]
        Stereo(u32),
        #[prost(uint32, tag = "4")]
        LoopbackMono(u32),
        #[prost(uint32, tag = "5")]
        LoopbackStereo(u32),
        #[prost(message, tag = "3")]
        Midi(super::TrackMidiInput),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TrackMidiInput {
    #[prost(uint32, optional, tag = "1")]
    pub device: ::core::option::Option<u32>,
    #[prost(uint32, optional, tag = "2")]
    pub channel: ::core::option::Option<u32>,
    #[prost(uint32, optional, tag = "3")]
    pub destination_channel: ::core::option::Option<u32>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct HelgoboxInstances {
    #[prost(message, repeated, tag = "1")]
    pub instances: ::prost::alloc::vec::Vec<HelgoboxInstance>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct HelgoboxInstance {
    #[prost(message, optional, tag = "1")]
    pub data: ::core::option::Option<HelgoboxInstanceData>,
    #[prost(message, optional, tag = "2")]
    pub fx: ::core::option::Option<FxLocation>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct HelgoboxInstanceData {
    /// Instance ID (generated, immutable during runtime, not persistent)
    #[prost(uint32, tag = "1")]
    pub id: u32,
    /// Instance key (user-defined, therefore mutable, persistent)
    #[prost(string, tag = "2")]
    pub key: ::prost::alloc::string::String,
    /// Instance tags
    #[prost(string, repeated, tag = "3")]
    pub tags: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FxLocation {
    /// Location of the containing FX chain.
    #[prost(message, optional, tag = "1")]
    pub fx_chain: ::core::option::Option<FxChainLocation>,
    /// Unique ID of the FX.
    #[prost(string, tag = "2")]
    pub id: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FxChainLocation {
    #[prost(oneof = "fx_chain_location::Location", tags = "1, 2")]
    pub location: ::core::option::Option<fx_chain_location::Location>,
}
/// Nested message and enum types in `FxChainLocation`.
pub mod fx_chain_location {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Location {
        #[prost(message, tag = "1")]
        TrackFx(super::TrackFxChainLocation),
        #[prost(message, tag = "2")]
        MonitoringFx(super::Empty),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TrackFxChainLocation {
    /// Location of the containing track.
    #[prost(message, optional, tag = "1")]
    pub track: ::core::option::Option<TrackLocation>,
    /// Whether it's the normal or input FX chain.
    #[prost(bool, tag = "2")]
    pub input_fx: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TrackLocation {
    /// Location of the containing project.
    #[prost(message, optional, tag = "1")]
    pub project: ::core::option::Option<ProjectLocation>,
    /// Unique ID of the track.
    #[prost(string, tag = "2")]
    pub id: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ProjectLocation {
    /// Index within the currently open project tabs.
    #[prost(uint32, tag = "1")]
    pub index: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MidiInputDevices {
    #[prost(message, repeated, tag = "1")]
    pub devices: ::prost::alloc::vec::Vec<MidiInputDevice>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MidiInputDevice {
    #[prost(uint32, tag = "1")]
    pub id: u32,
    #[prost(string, tag = "2")]
    pub name: ::prost::alloc::string::String,
    #[prost(enumeration = "MidiDeviceStatus", tag = "3")]
    pub status: i32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MidiOutputDevices {
    #[prost(message, repeated, tag = "1")]
    pub devices: ::prost::alloc::vec::Vec<MidiOutputDevice>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MidiOutputDevice {
    #[prost(uint32, tag = "1")]
    pub id: u32,
    #[prost(string, tag = "2")]
    pub name: ::prost::alloc::string::String,
    #[prost(enumeration = "MidiDeviceStatus", tag = "3")]
    pub status: i32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TrackInList {
    #[prost(string, tag = "1")]
    pub id: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub name: ::prost::alloc::string::String,
    #[prost(uint32, tag = "3")]
    pub level: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AudioInputChannels {
    #[prost(message, repeated, tag = "1")]
    pub channels: ::prost::alloc::vec::Vec<AudioInputChannel>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AudioInputChannel {
    #[prost(uint32, tag = "1")]
    pub index: u32,
    #[prost(string, tag = "2")]
    pub name: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ResampleModes {
    #[prost(message, repeated, tag = "1")]
    pub modes: ::prost::alloc::vec::Vec<ResampleMode>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ResampleMode {
    #[prost(uint32, tag = "1")]
    pub index: u32,
    #[prost(string, tag = "2")]
    pub name: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PitchShiftModes {
    #[prost(message, repeated, tag = "1")]
    pub modes: ::prost::alloc::vec::Vec<PitchShiftMode>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PitchShiftMode {
    #[prost(uint32, tag = "1")]
    pub index: u32,
    #[prost(string, tag = "2")]
    pub name: ::prost::alloc::string::String,
    #[prost(message, repeated, tag = "3")]
    pub sub_modes: ::prost::alloc::vec::Vec<PitchShiftSubMode>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PitchShiftSubMode {
    #[prost(uint32, tag = "1")]
    pub index: u32,
    #[prost(string, tag = "2")]
    pub name: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QualifiedOccasionalSlotUpdate {
    #[prost(message, optional, tag = "1")]
    pub slot_address: ::core::option::Option<SlotAddress>,
    #[prost(oneof = "qualified_occasional_slot_update::Update", tags = "2, 3")]
    pub update: ::core::option::Option<qualified_occasional_slot_update::Update>,
}
/// Nested message and enum types in `QualifiedOccasionalSlotUpdate`.
pub mod qualified_occasional_slot_update {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Update {
        /// Slot play state
        #[prost(enumeration = "super::SlotPlayState", tag = "2")]
        PlayState(i32),
        /// The complete persistent data of this slot has changed, that's mainly the
        /// list of clips and their contents. This contains the complete slot as JSON.
        #[prost(string, tag = "3")]
        CompletePersistentData(::prost::alloc::string::String),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QualifiedOccasionalClipUpdate {
    #[prost(message, optional, tag = "1")]
    pub clip_address: ::core::option::Option<ClipAddress>,
    #[prost(oneof = "qualified_occasional_clip_update::Update", tags = "2, 3")]
    pub update: ::core::option::Option<qualified_occasional_clip_update::Update>,
}
/// Nested message and enum types in `QualifiedOccasionalClipUpdate`.
pub mod qualified_occasional_clip_update {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Update {
        /// The complete persistent data of this clip has changed, e.g. its name.
        /// This contains the complete clip as JSON.
        #[prost(string, tag = "2")]
        CompletePersistentData(::prost::alloc::string::String),
        #[prost(message, tag = "3")]
        ContentInfo(super::ClipContentInfo),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ClipContentInfo {
    #[prost(oneof = "clip_content_info::Info", tags = "1, 2")]
    pub info: ::core::option::Option<clip_content_info::Info>,
}
/// Nested message and enum types in `ClipContentInfo`.
pub mod clip_content_info {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Info {
        #[prost(message, tag = "1")]
        Midi(super::MidiClipContentInfo),
        #[prost(message, tag = "2")]
        Audio(super::AudioClipContentInfo),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MidiClipContentInfo {
    #[prost(bool, tag = "1")]
    pub quantized: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AudioClipContentInfo {
    #[prost(bool, tag = "1")]
    pub online: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ContinuousSlotUpdate {
    /// For each clip in the slot
    #[prost(message, repeated, tag = "1")]
    pub clip_update: ::prost::alloc::vec::Vec<ContinuousClipUpdate>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ContinuousClipUpdate {
    /// Number between 0 and 1, to be interpreted as percentage within the playable portion (e.g. within section bounds).
    ///
    /// Perfect for a simple clip progress visualization.
    #[prost(double, tag = "1")]
    pub proportional_position: f64,
    /// Position within the playable portion in seconds.
    ///
    /// This will be negative during the count-in phase, which makes it suitable for building a count-down.
    #[prost(double, tag = "2")]
    pub position_in_seconds: f64,
    /// Position within the inner-most source in frames (sample-rate dependent).
    ///
    /// If the section length exceeds the source end, this number can be larger than the source length.
    ///
    /// Suitable for indicating the position in a waveform editor.
    #[prost(int64, tag = "3")]
    pub source_position_in_frames: i64,
    /// Like `source_position_in_frames` but driven by calculations instead of the actual position.
    ///
    /// Primarily a tool for debugging, to check whether actual position doesn't deviate too
    /// much from the hypothetical one.
    #[prost(int64, tag = "4")]
    pub hypothetical_source_position_in_frames: i64,
    /// Number between 0 and 1.
    #[prost(double, tag = "5")]
    pub peak: f64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum MatrixVolumeKind {
    Master = 0,
    Click = 1,
    TempoTap = 2,
}
impl MatrixVolumeKind {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            MatrixVolumeKind::Master => "MATRIX_VOLUME_KIND_MASTER",
            MatrixVolumeKind::Click => "MATRIX_VOLUME_KIND_CLICK",
            MatrixVolumeKind::TempoTap => "MATRIX_VOLUME_KIND_TEMPO_TAP",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "MATRIX_VOLUME_KIND_MASTER" => Some(Self::Master),
            "MATRIX_VOLUME_KIND_CLICK" => Some(Self::Click),
            "MATRIX_VOLUME_KIND_TEMPO_TAP" => Some(Self::TempoTap),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum TriggerInstanceAction {
    ShowHelgoboxPlugin = 0,
    ArrangementTogglePlayStop = 1,
    ArrangementPlay = 2,
    ArrangementStop = 3,
    ArrangementPause = 4,
    ArrangementStartRecording = 5,
    ArrangementStopRecording = 6,
    CloseApp = 7,
    HideApp = 8,
    SaveProject = 9,
}
impl TriggerInstanceAction {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            TriggerInstanceAction::ShowHelgoboxPlugin => {
                "TRIGGER_INSTANCE_ACTION_SHOW_HELGOBOX_PLUGIN"
            }
            TriggerInstanceAction::ArrangementTogglePlayStop => {
                "TRIGGER_INSTANCE_ACTION_ARRANGEMENT_TOGGLE_PLAY_STOP"
            }
            TriggerInstanceAction::ArrangementPlay => {
                "TRIGGER_INSTANCE_ACTION_ARRANGEMENT_PLAY"
            }
            TriggerInstanceAction::ArrangementStop => {
                "TRIGGER_INSTANCE_ACTION_ARRANGEMENT_STOP"
            }
            TriggerInstanceAction::ArrangementPause => {
                "TRIGGER_INSTANCE_ACTION_ARRANGEMENT_PAUSE"
            }
            TriggerInstanceAction::ArrangementStartRecording => {
                "TRIGGER_INSTANCE_ACTION_ARRANGEMENT_START_RECORDING"
            }
            TriggerInstanceAction::ArrangementStopRecording => {
                "TRIGGER_INSTANCE_ACTION_ARRANGEMENT_STOP_RECORDING"
            }
            TriggerInstanceAction::CloseApp => "TRIGGER_INSTANCE_ACTION_CLOSE_APP",
            TriggerInstanceAction::HideApp => "TRIGGER_INSTANCE_ACTION_HIDE_APP",
            TriggerInstanceAction::SaveProject => "TRIGGER_INSTANCE_ACTION_SAVE_PROJECT",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "TRIGGER_INSTANCE_ACTION_SHOW_HELGOBOX_PLUGIN" => {
                Some(Self::ShowHelgoboxPlugin)
            }
            "TRIGGER_INSTANCE_ACTION_ARRANGEMENT_TOGGLE_PLAY_STOP" => {
                Some(Self::ArrangementTogglePlayStop)
            }
            "TRIGGER_INSTANCE_ACTION_ARRANGEMENT_PLAY" => Some(Self::ArrangementPlay),
            "TRIGGER_INSTANCE_ACTION_ARRANGEMENT_STOP" => Some(Self::ArrangementStop),
            "TRIGGER_INSTANCE_ACTION_ARRANGEMENT_PAUSE" => Some(Self::ArrangementPause),
            "TRIGGER_INSTANCE_ACTION_ARRANGEMENT_START_RECORDING" => {
                Some(Self::ArrangementStartRecording)
            }
            "TRIGGER_INSTANCE_ACTION_ARRANGEMENT_STOP_RECORDING" => {
                Some(Self::ArrangementStopRecording)
            }
            "TRIGGER_INSTANCE_ACTION_CLOSE_APP" => Some(Self::CloseApp),
            "TRIGGER_INSTANCE_ACTION_HIDE_APP" => Some(Self::HideApp),
            "TRIGGER_INSTANCE_ACTION_SAVE_PROJECT" => Some(Self::SaveProject),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum TriggerMatrixAction {
    StopAllClips = 0,
    Undo = 7,
    Redo = 8,
    ToggleClick = 9,
    Panic = 10,
    ToggleMute = 11,
    CreateMatrix = 12,
    ShowMasterFx = 13,
    ShowMasterRouting = 14,
    TapTempo = 15,
    EnterSilenceMode = 38,
    ToggleSilenceMode = 16,
    PlayAllIgnitedClips = 17,
    SequencerRecord = 18,
    SequencerPlay = 19,
    SequencerStop = 20,
    SequencerWriteToArrangementAtStart = 21,
    SequencerWriteToArrangementAtEnd = 41,
    SequencerWriteToArrangementAtCursor = 42,
    SequencerCleanArrangement = 22,
    TriggerSmartRecord = 25,
    ToggleLearnSimpleMappingTrigger = 23,
    ToggleLearnSimpleMappingSmartRecord = 26,
    ToggleLearnSimpleMappingEnterSilenceModeOrPlayIgnited = 27,
    ToggleLearnSimpleMappingSequencerRecordOnOffState = 28,
    ToggleLearnSimpleMappingSequencerPlayOnOffState = 29,
    ToggleLearnSimpleMappingTapTempo = 35,
    RemoveSimpleMappingTrigger = 24,
    RemoveSimpleMappingSmartRecord = 30,
    RemoveSimpleMappingEnterSilenceModeOrPlayIgnited = 31,
    RemoveSimpleMappingSequencerRecordOnOffState = 32,
    RemoveSimpleMappingSequencerPlayOnOffState = 33,
    RemoveSimpleMappingTapTempo = 36,
    Activate = 34,
    /// Brings all clips online again.
    ReloadAllClips = 37,
    ExportToClipboard = 39,
    ExportToArrangement = 40,
}
impl TriggerMatrixAction {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            TriggerMatrixAction::StopAllClips => "TRIGGER_MATRIX_ACTION_STOP_ALL_CLIPS",
            TriggerMatrixAction::Undo => "TRIGGER_MATRIX_ACTION_UNDO",
            TriggerMatrixAction::Redo => "TRIGGER_MATRIX_ACTION_REDO",
            TriggerMatrixAction::ToggleClick => "TRIGGER_MATRIX_ACTION_TOGGLE_CLICK",
            TriggerMatrixAction::Panic => "TRIGGER_MATRIX_ACTION_PANIC",
            TriggerMatrixAction::ToggleMute => "TRIGGER_MATRIX_ACTION_TOGGLE_MUTE",
            TriggerMatrixAction::CreateMatrix => "TRIGGER_MATRIX_ACTION_CREATE_MATRIX",
            TriggerMatrixAction::ShowMasterFx => "TRIGGER_MATRIX_ACTION_SHOW_MASTER_FX",
            TriggerMatrixAction::ShowMasterRouting => {
                "TRIGGER_MATRIX_ACTION_SHOW_MASTER_ROUTING"
            }
            TriggerMatrixAction::TapTempo => "TRIGGER_MATRIX_ACTION_TAP_TEMPO",
            TriggerMatrixAction::EnterSilenceMode => {
                "TRIGGER_MATRIX_ACTION_ENTER_SILENCE_MODE"
            }
            TriggerMatrixAction::ToggleSilenceMode => {
                "TRIGGER_MATRIX_ACTION_TOGGLE_SILENCE_MODE"
            }
            TriggerMatrixAction::PlayAllIgnitedClips => {
                "TRIGGER_MATRIX_ACTION_PLAY_ALL_IGNITED_CLIPS"
            }
            TriggerMatrixAction::SequencerRecord => {
                "TRIGGER_MATRIX_ACTION_SEQUENCER_RECORD"
            }
            TriggerMatrixAction::SequencerPlay => "TRIGGER_MATRIX_ACTION_SEQUENCER_PLAY",
            TriggerMatrixAction::SequencerStop => "TRIGGER_MATRIX_ACTION_SEQUENCER_STOP",
            TriggerMatrixAction::SequencerWriteToArrangementAtStart => {
                "TRIGGER_MATRIX_ACTION_SEQUENCER_WRITE_TO_ARRANGEMENT_AT_START"
            }
            TriggerMatrixAction::SequencerWriteToArrangementAtEnd => {
                "TRIGGER_MATRIX_ACTION_SEQUENCER_WRITE_TO_ARRANGEMENT_AT_END"
            }
            TriggerMatrixAction::SequencerWriteToArrangementAtCursor => {
                "TRIGGER_MATRIX_ACTION_SEQUENCER_WRITE_TO_ARRANGEMENT_AT_CURSOR"
            }
            TriggerMatrixAction::SequencerCleanArrangement => {
                "TRIGGER_MATRIX_ACTION_SEQUENCER_CLEAN_ARRANGEMENT"
            }
            TriggerMatrixAction::TriggerSmartRecord => {
                "TRIGGER_MATRIX_ACTION_TRIGGER_SMART_RECORD"
            }
            TriggerMatrixAction::ToggleLearnSimpleMappingTrigger => {
                "TRIGGER_MATRIX_ACTION_TOGGLE_LEARN_SIMPLE_MAPPING_TRIGGER"
            }
            TriggerMatrixAction::ToggleLearnSimpleMappingSmartRecord => {
                "TRIGGER_MATRIX_ACTION_TOGGLE_LEARN_SIMPLE_MAPPING_SMART_RECORD"
            }
            TriggerMatrixAction::ToggleLearnSimpleMappingEnterSilenceModeOrPlayIgnited => {
                "TRIGGER_MATRIX_ACTION_TOGGLE_LEARN_SIMPLE_MAPPING_ENTER_SILENCE_MODE_OR_PLAY_IGNITED"
            }
            TriggerMatrixAction::ToggleLearnSimpleMappingSequencerRecordOnOffState => {
                "TRIGGER_MATRIX_ACTION_TOGGLE_LEARN_SIMPLE_MAPPING_SEQUENCER_RECORD_ON_OFF_STATE"
            }
            TriggerMatrixAction::ToggleLearnSimpleMappingSequencerPlayOnOffState => {
                "TRIGGER_MATRIX_ACTION_TOGGLE_LEARN_SIMPLE_MAPPING_SEQUENCER_PLAY_ON_OFF_STATE"
            }
            TriggerMatrixAction::ToggleLearnSimpleMappingTapTempo => {
                "TRIGGER_MATRIX_ACTION_TOGGLE_LEARN_SIMPLE_MAPPING_TAP_TEMPO"
            }
            TriggerMatrixAction::RemoveSimpleMappingTrigger => {
                "TRIGGER_MATRIX_ACTION_REMOVE_SIMPLE_MAPPING_TRIGGER"
            }
            TriggerMatrixAction::RemoveSimpleMappingSmartRecord => {
                "TRIGGER_MATRIX_ACTION_REMOVE_SIMPLE_MAPPING_SMART_RECORD"
            }
            TriggerMatrixAction::RemoveSimpleMappingEnterSilenceModeOrPlayIgnited => {
                "TRIGGER_MATRIX_ACTION_REMOVE_SIMPLE_MAPPING_ENTER_SILENCE_MODE_OR_PLAY_IGNITED"
            }
            TriggerMatrixAction::RemoveSimpleMappingSequencerRecordOnOffState => {
                "TRIGGER_MATRIX_ACTION_REMOVE_SIMPLE_MAPPING_SEQUENCER_RECORD_ON_OFF_STATE"
            }
            TriggerMatrixAction::RemoveSimpleMappingSequencerPlayOnOffState => {
                "TRIGGER_MATRIX_ACTION_REMOVE_SIMPLE_MAPPING_SEQUENCER_PLAY_ON_OFF_STATE"
            }
            TriggerMatrixAction::RemoveSimpleMappingTapTempo => {
                "TRIGGER_MATRIX_ACTION_REMOVE_SIMPLE_MAPPING_TAP_TEMPO"
            }
            TriggerMatrixAction::Activate => "TRIGGER_MATRIX_ACTION_ACTIVATE",
            TriggerMatrixAction::ReloadAllClips => {
                "TRIGGER_MATRIX_ACTION_RELOAD_ALL_CLIPS"
            }
            TriggerMatrixAction::ExportToClipboard => {
                "TRIGGER_MATRIX_ACTION_EXPORT_TO_CLIPBOARD"
            }
            TriggerMatrixAction::ExportToArrangement => {
                "TRIGGER_MATRIX_ACTION_EXPORT_TO_ARRANGEMENT"
            }
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "TRIGGER_MATRIX_ACTION_STOP_ALL_CLIPS" => Some(Self::StopAllClips),
            "TRIGGER_MATRIX_ACTION_UNDO" => Some(Self::Undo),
            "TRIGGER_MATRIX_ACTION_REDO" => Some(Self::Redo),
            "TRIGGER_MATRIX_ACTION_TOGGLE_CLICK" => Some(Self::ToggleClick),
            "TRIGGER_MATRIX_ACTION_PANIC" => Some(Self::Panic),
            "TRIGGER_MATRIX_ACTION_TOGGLE_MUTE" => Some(Self::ToggleMute),
            "TRIGGER_MATRIX_ACTION_CREATE_MATRIX" => Some(Self::CreateMatrix),
            "TRIGGER_MATRIX_ACTION_SHOW_MASTER_FX" => Some(Self::ShowMasterFx),
            "TRIGGER_MATRIX_ACTION_SHOW_MASTER_ROUTING" => Some(Self::ShowMasterRouting),
            "TRIGGER_MATRIX_ACTION_TAP_TEMPO" => Some(Self::TapTempo),
            "TRIGGER_MATRIX_ACTION_ENTER_SILENCE_MODE" => Some(Self::EnterSilenceMode),
            "TRIGGER_MATRIX_ACTION_TOGGLE_SILENCE_MODE" => Some(Self::ToggleSilenceMode),
            "TRIGGER_MATRIX_ACTION_PLAY_ALL_IGNITED_CLIPS" => {
                Some(Self::PlayAllIgnitedClips)
            }
            "TRIGGER_MATRIX_ACTION_SEQUENCER_RECORD" => Some(Self::SequencerRecord),
            "TRIGGER_MATRIX_ACTION_SEQUENCER_PLAY" => Some(Self::SequencerPlay),
            "TRIGGER_MATRIX_ACTION_SEQUENCER_STOP" => Some(Self::SequencerStop),
            "TRIGGER_MATRIX_ACTION_SEQUENCER_WRITE_TO_ARRANGEMENT_AT_START" => {
                Some(Self::SequencerWriteToArrangementAtStart)
            }
            "TRIGGER_MATRIX_ACTION_SEQUENCER_WRITE_TO_ARRANGEMENT_AT_END" => {
                Some(Self::SequencerWriteToArrangementAtEnd)
            }
            "TRIGGER_MATRIX_ACTION_SEQUENCER_WRITE_TO_ARRANGEMENT_AT_CURSOR" => {
                Some(Self::SequencerWriteToArrangementAtCursor)
            }
            "TRIGGER_MATRIX_ACTION_SEQUENCER_CLEAN_ARRANGEMENT" => {
                Some(Self::SequencerCleanArrangement)
            }
            "TRIGGER_MATRIX_ACTION_TRIGGER_SMART_RECORD" => {
                Some(Self::TriggerSmartRecord)
            }
            "TRIGGER_MATRIX_ACTION_TOGGLE_LEARN_SIMPLE_MAPPING_TRIGGER" => {
                Some(Self::ToggleLearnSimpleMappingTrigger)
            }
            "TRIGGER_MATRIX_ACTION_TOGGLE_LEARN_SIMPLE_MAPPING_SMART_RECORD" => {
                Some(Self::ToggleLearnSimpleMappingSmartRecord)
            }
            "TRIGGER_MATRIX_ACTION_TOGGLE_LEARN_SIMPLE_MAPPING_ENTER_SILENCE_MODE_OR_PLAY_IGNITED" => {
                Some(Self::ToggleLearnSimpleMappingEnterSilenceModeOrPlayIgnited)
            }
            "TRIGGER_MATRIX_ACTION_TOGGLE_LEARN_SIMPLE_MAPPING_SEQUENCER_RECORD_ON_OFF_STATE" => {
                Some(Self::ToggleLearnSimpleMappingSequencerRecordOnOffState)
            }
            "TRIGGER_MATRIX_ACTION_TOGGLE_LEARN_SIMPLE_MAPPING_SEQUENCER_PLAY_ON_OFF_STATE" => {
                Some(Self::ToggleLearnSimpleMappingSequencerPlayOnOffState)
            }
            "TRIGGER_MATRIX_ACTION_TOGGLE_LEARN_SIMPLE_MAPPING_TAP_TEMPO" => {
                Some(Self::ToggleLearnSimpleMappingTapTempo)
            }
            "TRIGGER_MATRIX_ACTION_REMOVE_SIMPLE_MAPPING_TRIGGER" => {
                Some(Self::RemoveSimpleMappingTrigger)
            }
            "TRIGGER_MATRIX_ACTION_REMOVE_SIMPLE_MAPPING_SMART_RECORD" => {
                Some(Self::RemoveSimpleMappingSmartRecord)
            }
            "TRIGGER_MATRIX_ACTION_REMOVE_SIMPLE_MAPPING_ENTER_SILENCE_MODE_OR_PLAY_IGNITED" => {
                Some(Self::RemoveSimpleMappingEnterSilenceModeOrPlayIgnited)
            }
            "TRIGGER_MATRIX_ACTION_REMOVE_SIMPLE_MAPPING_SEQUENCER_RECORD_ON_OFF_STATE" => {
                Some(Self::RemoveSimpleMappingSequencerRecordOnOffState)
            }
            "TRIGGER_MATRIX_ACTION_REMOVE_SIMPLE_MAPPING_SEQUENCER_PLAY_ON_OFF_STATE" => {
                Some(Self::RemoveSimpleMappingSequencerPlayOnOffState)
            }
            "TRIGGER_MATRIX_ACTION_REMOVE_SIMPLE_MAPPING_TAP_TEMPO" => {
                Some(Self::RemoveSimpleMappingTapTempo)
            }
            "TRIGGER_MATRIX_ACTION_ACTIVATE" => Some(Self::Activate),
            "TRIGGER_MATRIX_ACTION_RELOAD_ALL_CLIPS" => Some(Self::ReloadAllClips),
            "TRIGGER_MATRIX_ACTION_EXPORT_TO_CLIPBOARD" => Some(Self::ExportToClipboard),
            "TRIGGER_MATRIX_ACTION_EXPORT_TO_ARRANGEMENT" => {
                Some(Self::ExportToArrangement)
            }
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum TriggerGlobalAction {
    FocusHost = 0,
    MidiPanic = 1,
}
impl TriggerGlobalAction {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            TriggerGlobalAction::FocusHost => "TRIGGER_GLOBAL_ACTION_FOCUS_HOST",
            TriggerGlobalAction::MidiPanic => "TRIGGER_GLOBAL_ACTION_MIDI_PANIC",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "TRIGGER_GLOBAL_ACTION_FOCUS_HOST" => Some(Self::FocusHost),
            "TRIGGER_GLOBAL_ACTION_MIDI_PANIC" => Some(Self::MidiPanic),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum ColumnKind {
    Audio = 0,
    Midi = 1,
}
impl ColumnKind {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            ColumnKind::Audio => "COLUMN_KIND_AUDIO",
            ColumnKind::Midi => "COLUMN_KIND_MIDI",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "COLUMN_KIND_AUDIO" => Some(Self::Audio),
            "COLUMN_KIND_MIDI" => Some(Self::Midi),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum Compartment {
    Controller = 0,
    Main = 1,
}
impl Compartment {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            Compartment::Controller => "COMPARTMENT_CONTROLLER",
            Compartment::Main => "COMPARTMENT_MAIN",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "COMPARTMENT_CONTROLLER" => Some(Self::Controller),
            "COMPARTMENT_MAIN" => Some(Self::Main),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum TriggerTrackAction {
    ToggleMute = 0,
    ToggleSolo = 1,
    ToggleArm = 2,
    ShowFx = 3,
    ShowRouting = 4,
    ToggleLearnInput = 5,
}
impl TriggerTrackAction {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            TriggerTrackAction::ToggleMute => "TRIGGER_TRACK_ACTION_TOGGLE_MUTE",
            TriggerTrackAction::ToggleSolo => "TRIGGER_TRACK_ACTION_TOGGLE_SOLO",
            TriggerTrackAction::ToggleArm => "TRIGGER_TRACK_ACTION_TOGGLE_ARM",
            TriggerTrackAction::ShowFx => "TRIGGER_TRACK_ACTION_SHOW_FX",
            TriggerTrackAction::ShowRouting => "TRIGGER_TRACK_ACTION_SHOW_ROUTING",
            TriggerTrackAction::ToggleLearnInput => {
                "TRIGGER_TRACK_ACTION_TOGGLE_LEARN_INPUT"
            }
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "TRIGGER_TRACK_ACTION_TOGGLE_MUTE" => Some(Self::ToggleMute),
            "TRIGGER_TRACK_ACTION_TOGGLE_SOLO" => Some(Self::ToggleSolo),
            "TRIGGER_TRACK_ACTION_TOGGLE_ARM" => Some(Self::ToggleArm),
            "TRIGGER_TRACK_ACTION_SHOW_FX" => Some(Self::ShowFx),
            "TRIGGER_TRACK_ACTION_SHOW_ROUTING" => Some(Self::ShowRouting),
            "TRIGGER_TRACK_ACTION_TOGGLE_LEARN_INPUT" => Some(Self::ToggleLearnInput),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum TriggerColumnAction {
    Stop = 0,
    Remove = 1,
    RemoveWithoutTrack = 11,
    Duplicate = 2,
    Insert = 3,
    InsertForEachSelectedTrack = 10,
    Panic = 4,
    ToggleLearnSimpleMapping = 5,
    RemoveSimpleMapping = 6,
    Activate = 7,
    ExportToClipboard = 8,
    ExportToArrangement = 9,
}
impl TriggerColumnAction {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            TriggerColumnAction::Stop => "TRIGGER_COLUMN_ACTION_STOP",
            TriggerColumnAction::Remove => "TRIGGER_COLUMN_ACTION_REMOVE",
            TriggerColumnAction::RemoveWithoutTrack => {
                "TRIGGER_COLUMN_ACTION_REMOVE_WITHOUT_TRACK"
            }
            TriggerColumnAction::Duplicate => "TRIGGER_COLUMN_ACTION_DUPLICATE",
            TriggerColumnAction::Insert => "TRIGGER_COLUMN_ACTION_INSERT",
            TriggerColumnAction::InsertForEachSelectedTrack => {
                "TRIGGER_COLUMN_ACTION_INSERT_FOR_EACH_SELECTED_TRACK"
            }
            TriggerColumnAction::Panic => "TRIGGER_COLUMN_ACTION_PANIC",
            TriggerColumnAction::ToggleLearnSimpleMapping => {
                "TRIGGER_COLUMN_ACTION_TOGGLE_LEARN_SIMPLE_MAPPING"
            }
            TriggerColumnAction::RemoveSimpleMapping => {
                "TRIGGER_COLUMN_ACTION_REMOVE_SIMPLE_MAPPING"
            }
            TriggerColumnAction::Activate => "TRIGGER_COLUMN_ACTION_ACTIVATE",
            TriggerColumnAction::ExportToClipboard => {
                "TRIGGER_COLUMN_ACTION_EXPORT_TO_CLIPBOARD"
            }
            TriggerColumnAction::ExportToArrangement => {
                "TRIGGER_COLUMN_ACTION_EXPORT_TO_ARRANGEMENT"
            }
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "TRIGGER_COLUMN_ACTION_STOP" => Some(Self::Stop),
            "TRIGGER_COLUMN_ACTION_REMOVE" => Some(Self::Remove),
            "TRIGGER_COLUMN_ACTION_REMOVE_WITHOUT_TRACK" => {
                Some(Self::RemoveWithoutTrack)
            }
            "TRIGGER_COLUMN_ACTION_DUPLICATE" => Some(Self::Duplicate),
            "TRIGGER_COLUMN_ACTION_INSERT" => Some(Self::Insert),
            "TRIGGER_COLUMN_ACTION_INSERT_FOR_EACH_SELECTED_TRACK" => {
                Some(Self::InsertForEachSelectedTrack)
            }
            "TRIGGER_COLUMN_ACTION_PANIC" => Some(Self::Panic),
            "TRIGGER_COLUMN_ACTION_TOGGLE_LEARN_SIMPLE_MAPPING" => {
                Some(Self::ToggleLearnSimpleMapping)
            }
            "TRIGGER_COLUMN_ACTION_REMOVE_SIMPLE_MAPPING" => {
                Some(Self::RemoveSimpleMapping)
            }
            "TRIGGER_COLUMN_ACTION_ACTIVATE" => Some(Self::Activate),
            "TRIGGER_COLUMN_ACTION_EXPORT_TO_CLIPBOARD" => Some(Self::ExportToClipboard),
            "TRIGGER_COLUMN_ACTION_EXPORT_TO_ARRANGEMENT" => {
                Some(Self::ExportToArrangement)
            }
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum TriggerRowAction {
    Play = 0,
    Clear = 1,
    Copy = 2,
    Cut = 3,
    Paste = 4,
    Remove = 5,
    Duplicate = 6,
    Insert = 7,
    ToggleLearnSimpleMapping = 9,
    RemoveSimpleMapping = 10,
    BuildSceneFromPlayingSlots = 11,
    Activate = 12,
    ExportToArrangement = 13,
}
impl TriggerRowAction {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            TriggerRowAction::Play => "TRIGGER_ROW_ACTION_PLAY",
            TriggerRowAction::Clear => "TRIGGER_ROW_ACTION_CLEAR",
            TriggerRowAction::Copy => "TRIGGER_ROW_ACTION_COPY",
            TriggerRowAction::Cut => "TRIGGER_ROW_ACTION_CUT",
            TriggerRowAction::Paste => "TRIGGER_ROW_ACTION_PASTE",
            TriggerRowAction::Remove => "TRIGGER_ROW_ACTION_REMOVE",
            TriggerRowAction::Duplicate => "TRIGGER_ROW_ACTION_DUPLICATE",
            TriggerRowAction::Insert => "TRIGGER_ROW_ACTION_INSERT",
            TriggerRowAction::ToggleLearnSimpleMapping => {
                "TRIGGER_ROW_ACTION_TOGGLE_LEARN_SIMPLE_MAPPING"
            }
            TriggerRowAction::RemoveSimpleMapping => {
                "TRIGGER_ROW_ACTION_REMOVE_SIMPLE_MAPPING"
            }
            TriggerRowAction::BuildSceneFromPlayingSlots => {
                "TRIGGER_ROW_ACTION_BUILD_SCENE_FROM_PLAYING_SLOTS"
            }
            TriggerRowAction::Activate => "TRIGGER_ROW_ACTION_ACTIVATE",
            TriggerRowAction::ExportToArrangement => {
                "TRIGGER_ROW_ACTION_EXPORT_TO_ARRANGEMENT"
            }
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "TRIGGER_ROW_ACTION_PLAY" => Some(Self::Play),
            "TRIGGER_ROW_ACTION_CLEAR" => Some(Self::Clear),
            "TRIGGER_ROW_ACTION_COPY" => Some(Self::Copy),
            "TRIGGER_ROW_ACTION_CUT" => Some(Self::Cut),
            "TRIGGER_ROW_ACTION_PASTE" => Some(Self::Paste),
            "TRIGGER_ROW_ACTION_REMOVE" => Some(Self::Remove),
            "TRIGGER_ROW_ACTION_DUPLICATE" => Some(Self::Duplicate),
            "TRIGGER_ROW_ACTION_INSERT" => Some(Self::Insert),
            "TRIGGER_ROW_ACTION_TOGGLE_LEARN_SIMPLE_MAPPING" => {
                Some(Self::ToggleLearnSimpleMapping)
            }
            "TRIGGER_ROW_ACTION_REMOVE_SIMPLE_MAPPING" => Some(Self::RemoveSimpleMapping),
            "TRIGGER_ROW_ACTION_BUILD_SCENE_FROM_PLAYING_SLOTS" => {
                Some(Self::BuildSceneFromPlayingSlots)
            }
            "TRIGGER_ROW_ACTION_ACTIVATE" => Some(Self::Activate),
            "TRIGGER_ROW_ACTION_EXPORT_TO_ARRANGEMENT" => Some(Self::ExportToArrangement),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum DragColumnAction {
    Reorder = 0,
}
impl DragColumnAction {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            DragColumnAction::Reorder => "DRAG_COLUMN_ACTION_REORDER",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "DRAG_COLUMN_ACTION_REORDER" => Some(Self::Reorder),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum DragRowAction {
    MoveContent = 0,
    CopyContent = 1,
    Reorder = 2,
}
impl DragRowAction {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            DragRowAction::MoveContent => "DRAG_ROW_ACTION_MOVE_CONTENT",
            DragRowAction::CopyContent => "DRAG_ROW_ACTION_COPY_CONTENT",
            DragRowAction::Reorder => "DRAG_ROW_ACTION_REORDER",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "DRAG_ROW_ACTION_MOVE_CONTENT" => Some(Self::MoveContent),
            "DRAG_ROW_ACTION_COPY_CONTENT" => Some(Self::CopyContent),
            "DRAG_ROW_ACTION_REORDER" => Some(Self::Reorder),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum TriggerSlotAction {
    Play = 0,
    Stop = 1,
    Record = 2,
    Clear = 4,
    Copy = 5,
    Cut = 6,
    Paste = 7,
    ImportSelectedItems = 8,
    Panic = 9,
    CreateEmptyMidiClip = 10,
    ToggleLearnSimpleMapping = 11,
    RemoveSimpleMapping = 12,
    TriggerOn = 13,
    TriggerOff = 14,
    Activate = 15,
    ExportToClipboard = 16,
    ExportToArrangement = 17,
}
impl TriggerSlotAction {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            TriggerSlotAction::Play => "TRIGGER_SLOT_ACTION_PLAY",
            TriggerSlotAction::Stop => "TRIGGER_SLOT_ACTION_STOP",
            TriggerSlotAction::Record => "TRIGGER_SLOT_ACTION_RECORD",
            TriggerSlotAction::Clear => "TRIGGER_SLOT_ACTION_CLEAR",
            TriggerSlotAction::Copy => "TRIGGER_SLOT_ACTION_COPY",
            TriggerSlotAction::Cut => "TRIGGER_SLOT_ACTION_CUT",
            TriggerSlotAction::Paste => "TRIGGER_SLOT_ACTION_PASTE",
            TriggerSlotAction::ImportSelectedItems => {
                "TRIGGER_SLOT_ACTION_IMPORT_SELECTED_ITEMS"
            }
            TriggerSlotAction::Panic => "TRIGGER_SLOT_ACTION_PANIC",
            TriggerSlotAction::CreateEmptyMidiClip => {
                "TRIGGER_SLOT_ACTION_CREATE_EMPTY_MIDI_CLIP"
            }
            TriggerSlotAction::ToggleLearnSimpleMapping => {
                "TRIGGER_SLOT_ACTION_TOGGLE_LEARN_SIMPLE_MAPPING"
            }
            TriggerSlotAction::RemoveSimpleMapping => {
                "TRIGGER_SLOT_ACTION_REMOVE_SIMPLE_MAPPING"
            }
            TriggerSlotAction::TriggerOn => "TRIGGER_SLOT_ACTION_TRIGGER_ON",
            TriggerSlotAction::TriggerOff => "TRIGGER_SLOT_ACTION_TRIGGER_OFF",
            TriggerSlotAction::Activate => "TRIGGER_SLOT_ACTION_ACTIVATE",
            TriggerSlotAction::ExportToClipboard => {
                "TRIGGER_SLOT_ACTION_EXPORT_TO_CLIPBOARD"
            }
            TriggerSlotAction::ExportToArrangement => {
                "TRIGGER_SLOT_ACTION_EXPORT_TO_ARRANGEMENT"
            }
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "TRIGGER_SLOT_ACTION_PLAY" => Some(Self::Play),
            "TRIGGER_SLOT_ACTION_STOP" => Some(Self::Stop),
            "TRIGGER_SLOT_ACTION_RECORD" => Some(Self::Record),
            "TRIGGER_SLOT_ACTION_CLEAR" => Some(Self::Clear),
            "TRIGGER_SLOT_ACTION_COPY" => Some(Self::Copy),
            "TRIGGER_SLOT_ACTION_CUT" => Some(Self::Cut),
            "TRIGGER_SLOT_ACTION_PASTE" => Some(Self::Paste),
            "TRIGGER_SLOT_ACTION_IMPORT_SELECTED_ITEMS" => {
                Some(Self::ImportSelectedItems)
            }
            "TRIGGER_SLOT_ACTION_PANIC" => Some(Self::Panic),
            "TRIGGER_SLOT_ACTION_CREATE_EMPTY_MIDI_CLIP" => {
                Some(Self::CreateEmptyMidiClip)
            }
            "TRIGGER_SLOT_ACTION_TOGGLE_LEARN_SIMPLE_MAPPING" => {
                Some(Self::ToggleLearnSimpleMapping)
            }
            "TRIGGER_SLOT_ACTION_REMOVE_SIMPLE_MAPPING" => {
                Some(Self::RemoveSimpleMapping)
            }
            "TRIGGER_SLOT_ACTION_TRIGGER_ON" => Some(Self::TriggerOn),
            "TRIGGER_SLOT_ACTION_TRIGGER_OFF" => Some(Self::TriggerOff),
            "TRIGGER_SLOT_ACTION_ACTIVATE" => Some(Self::Activate),
            "TRIGGER_SLOT_ACTION_EXPORT_TO_CLIPBOARD" => Some(Self::ExportToClipboard),
            "TRIGGER_SLOT_ACTION_EXPORT_TO_ARRANGEMENT" => {
                Some(Self::ExportToArrangement)
            }
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum TriggerClipAction {
    MidiOverdub = 0,
    Edit = 1,
    Remove = 2,
    Promote = 3,
    OpenInMediaExplorer = 4,
    Quantize = 5,
    Unquantize = 6,
    ExportToClipboard = 7,
    ExportToArrangement = 8,
    ToggleMidiOverdub = 9,
}
impl TriggerClipAction {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            TriggerClipAction::MidiOverdub => "TRIGGER_CLIP_ACTION_MIDI_OVERDUB",
            TriggerClipAction::Edit => "TRIGGER_CLIP_ACTION_EDIT",
            TriggerClipAction::Remove => "TRIGGER_CLIP_ACTION_REMOVE",
            TriggerClipAction::Promote => "TRIGGER_CLIP_ACTION_PROMOTE",
            TriggerClipAction::OpenInMediaExplorer => {
                "TRIGGER_CLIP_ACTION_OPEN_IN_MEDIA_EXPLORER"
            }
            TriggerClipAction::Quantize => "TRIGGER_CLIP_ACTION_QUANTIZE",
            TriggerClipAction::Unquantize => "TRIGGER_CLIP_ACTION_UNQUANTIZE",
            TriggerClipAction::ExportToClipboard => {
                "TRIGGER_CLIP_ACTION_EXPORT_TO_CLIPBOARD"
            }
            TriggerClipAction::ExportToArrangement => {
                "TRIGGER_CLIP_ACTION_EXPORT_TO_ARRANGEMENT"
            }
            TriggerClipAction::ToggleMidiOverdub => {
                "TRIGGER_CLIP_ACTION_TOGGLE_MIDI_OVERDUB"
            }
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "TRIGGER_CLIP_ACTION_MIDI_OVERDUB" => Some(Self::MidiOverdub),
            "TRIGGER_CLIP_ACTION_EDIT" => Some(Self::Edit),
            "TRIGGER_CLIP_ACTION_REMOVE" => Some(Self::Remove),
            "TRIGGER_CLIP_ACTION_PROMOTE" => Some(Self::Promote),
            "TRIGGER_CLIP_ACTION_OPEN_IN_MEDIA_EXPLORER" => {
                Some(Self::OpenInMediaExplorer)
            }
            "TRIGGER_CLIP_ACTION_QUANTIZE" => Some(Self::Quantize),
            "TRIGGER_CLIP_ACTION_UNQUANTIZE" => Some(Self::Unquantize),
            "TRIGGER_CLIP_ACTION_EXPORT_TO_CLIPBOARD" => Some(Self::ExportToClipboard),
            "TRIGGER_CLIP_ACTION_EXPORT_TO_ARRANGEMENT" => {
                Some(Self::ExportToArrangement)
            }
            "TRIGGER_CLIP_ACTION_TOGGLE_MIDI_OVERDUB" => Some(Self::ToggleMidiOverdub),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum TriggerSequenceAction {
    Activate = 0,
    Remove = 1,
}
impl TriggerSequenceAction {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            TriggerSequenceAction::Activate => "TRIGGER_SEQUENCE_ACTION_ACTIVATE",
            TriggerSequenceAction::Remove => "TRIGGER_SEQUENCE_ACTION_REMOVE",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "TRIGGER_SEQUENCE_ACTION_ACTIVATE" => Some(Self::Activate),
            "TRIGGER_SEQUENCE_ACTION_REMOVE" => Some(Self::Remove),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum DragSlotAction {
    Move = 0,
    Copy = 1,
}
impl DragSlotAction {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            DragSlotAction::Move => "DRAG_SLOT_ACTION_MOVE",
            DragSlotAction::Copy => "DRAG_SLOT_ACTION_COPY",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "DRAG_SLOT_ACTION_MOVE" => Some(Self::Move),
            "DRAG_SLOT_ACTION_COPY" => Some(Self::Copy),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum DragClipAction {
    Move = 0,
}
impl DragClipAction {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            DragClipAction::Move => "DRAG_CLIP_ACTION_MOVE",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "DRAG_CLIP_ACTION_MOVE" => Some(Self::Move),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum Scope {
    Realearn = 0,
    Playtime = 1,
}
impl Scope {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            Scope::Realearn => "SCOPE_REALEARN",
            Scope::Playtime => "SCOPE_PLAYTIME",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "SCOPE_REALEARN" => Some(Self::Realearn),
            "SCOPE_PLAYTIME" => Some(Self::Playtime),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum Severity {
    Low = 0,
    Medium = 1,
    High = 2,
}
impl Severity {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            Severity::Low => "SEVERITY_LOW",
            Severity::Medium => "SEVERITY_MEDIUM",
            Severity::High => "SEVERITY_HIGH",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "SEVERITY_LOW" => Some(Self::Low),
            "SEVERITY_MEDIUM" => Some(Self::Medium),
            "SEVERITY_HIGH" => Some(Self::High),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum TrackInputMonitoring {
    Unknown = 0,
    Off = 1,
    On = 2,
    Auto = 3,
}
impl TrackInputMonitoring {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            TrackInputMonitoring::Unknown => "TRACK_INPUT_MONITORING_UNKNOWN",
            TrackInputMonitoring::Off => "TRACK_INPUT_MONITORING_OFF",
            TrackInputMonitoring::On => "TRACK_INPUT_MONITORING_ON",
            TrackInputMonitoring::Auto => "TRACK_INPUT_MONITORING_AUTO",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "TRACK_INPUT_MONITORING_UNKNOWN" => Some(Self::Unknown),
            "TRACK_INPUT_MONITORING_OFF" => Some(Self::Off),
            "TRACK_INPUT_MONITORING_ON" => Some(Self::On),
            "TRACK_INPUT_MONITORING_AUTO" => Some(Self::Auto),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum SlotPlayState {
    Unknown = 0,
    Stopped = 1,
    Ignited = 2,
    ScheduledForPlayStart = 3,
    Playing = 4,
    Paused = 5,
    ScheduledForPlayRestart = 6,
    ScheduledForPlayStop = 7,
    ScheduledForRecordingStart = 8,
    Recording = 9,
    ScheduledForRecordingStop = 10,
}
impl SlotPlayState {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            SlotPlayState::Unknown => "SLOT_PLAY_STATE_UNKNOWN",
            SlotPlayState::Stopped => "SLOT_PLAY_STATE_STOPPED",
            SlotPlayState::Ignited => "SLOT_PLAY_STATE_IGNITED",
            SlotPlayState::ScheduledForPlayStart => {
                "SLOT_PLAY_STATE_SCHEDULED_FOR_PLAY_START"
            }
            SlotPlayState::Playing => "SLOT_PLAY_STATE_PLAYING",
            SlotPlayState::Paused => "SLOT_PLAY_STATE_PAUSED",
            SlotPlayState::ScheduledForPlayRestart => {
                "SLOT_PLAY_STATE_SCHEDULED_FOR_PLAY_RESTART"
            }
            SlotPlayState::ScheduledForPlayStop => {
                "SLOT_PLAY_STATE_SCHEDULED_FOR_PLAY_STOP"
            }
            SlotPlayState::ScheduledForRecordingStart => {
                "SLOT_PLAY_STATE_SCHEDULED_FOR_RECORDING_START"
            }
            SlotPlayState::Recording => "SLOT_PLAY_STATE_RECORDING",
            SlotPlayState::ScheduledForRecordingStop => {
                "SLOT_PLAY_STATE_SCHEDULED_FOR_RECORDING_STOP"
            }
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "SLOT_PLAY_STATE_UNKNOWN" => Some(Self::Unknown),
            "SLOT_PLAY_STATE_STOPPED" => Some(Self::Stopped),
            "SLOT_PLAY_STATE_IGNITED" => Some(Self::Ignited),
            "SLOT_PLAY_STATE_SCHEDULED_FOR_PLAY_START" => {
                Some(Self::ScheduledForPlayStart)
            }
            "SLOT_PLAY_STATE_PLAYING" => Some(Self::Playing),
            "SLOT_PLAY_STATE_PAUSED" => Some(Self::Paused),
            "SLOT_PLAY_STATE_SCHEDULED_FOR_PLAY_RESTART" => {
                Some(Self::ScheduledForPlayRestart)
            }
            "SLOT_PLAY_STATE_SCHEDULED_FOR_PLAY_STOP" => Some(Self::ScheduledForPlayStop),
            "SLOT_PLAY_STATE_SCHEDULED_FOR_RECORDING_START" => {
                Some(Self::ScheduledForRecordingStart)
            }
            "SLOT_PLAY_STATE_RECORDING" => Some(Self::Recording),
            "SLOT_PLAY_STATE_SCHEDULED_FOR_RECORDING_STOP" => {
                Some(Self::ScheduledForRecordingStop)
            }
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum MidiDeviceStatus {
    Disconnected = 0,
    ConnectedButDisabled = 1,
    Connected = 2,
}
impl MidiDeviceStatus {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            MidiDeviceStatus::Disconnected => "MIDI_DEVICE_STATUS_DISCONNECTED",
            MidiDeviceStatus::ConnectedButDisabled => {
                "MIDI_DEVICE_STATUS_CONNECTED_BUT_DISABLED"
            }
            MidiDeviceStatus::Connected => "MIDI_DEVICE_STATUS_CONNECTED",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "MIDI_DEVICE_STATUS_DISCONNECTED" => Some(Self::Disconnected),
            "MIDI_DEVICE_STATUS_CONNECTED_BUT_DISABLED" => {
                Some(Self::ConnectedButDisabled)
            }
            "MIDI_DEVICE_STATUS_CONNECTED" => Some(Self::Connected),
            _ => None,
        }
    }
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
impl ArrangementPlayState {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            ArrangementPlayState::Unknown => "ARRANGEMENT_PLAY_STATE_UNKNOWN",
            ArrangementPlayState::Stopped => "ARRANGEMENT_PLAY_STATE_STOPPED",
            ArrangementPlayState::Playing => "ARRANGEMENT_PLAY_STATE_PLAYING",
            ArrangementPlayState::PlayingPaused => {
                "ARRANGEMENT_PLAY_STATE_PLAYING_PAUSED"
            }
            ArrangementPlayState::Recording => "ARRANGEMENT_PLAY_STATE_RECORDING",
            ArrangementPlayState::RecordingPaused => {
                "ARRANGEMENT_PLAY_STATE_RECORDING_PAUSED"
            }
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "ARRANGEMENT_PLAY_STATE_UNKNOWN" => Some(Self::Unknown),
            "ARRANGEMENT_PLAY_STATE_STOPPED" => Some(Self::Stopped),
            "ARRANGEMENT_PLAY_STATE_PLAYING" => Some(Self::Playing),
            "ARRANGEMENT_PLAY_STATE_PLAYING_PAUSED" => Some(Self::PlayingPaused),
            "ARRANGEMENT_PLAY_STATE_RECORDING" => Some(Self::Recording),
            "ARRANGEMENT_PLAY_STATE_RECORDING_PAUSED" => Some(Self::RecordingPaused),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum SequencerPlayState {
    Unknown = 0,
    Stopped = 1,
    Playing = 2,
    Recording = 3,
}
impl SequencerPlayState {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            SequencerPlayState::Unknown => "SEQUENCER_PLAY_STATE_UNKNOWN",
            SequencerPlayState::Stopped => "SEQUENCER_PLAY_STATE_STOPPED",
            SequencerPlayState::Playing => "SEQUENCER_PLAY_STATE_PLAYING",
            SequencerPlayState::Recording => "SEQUENCER_PLAY_STATE_RECORDING",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "SEQUENCER_PLAY_STATE_UNKNOWN" => Some(Self::Unknown),
            "SEQUENCER_PLAY_STATE_STOPPED" => Some(Self::Stopped),
            "SEQUENCER_PLAY_STATE_PLAYING" => Some(Self::Playing),
            "SEQUENCER_PLAY_STATE_RECORDING" => Some(Self::Recording),
            _ => None,
        }
    }
}
/// Generated server implementations.
pub mod helgobox_service_server {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::*;
    /// Generated trait containing gRPC methods that should be implemented for use with HelgoboxServiceServer.
    #[async_trait]
    pub trait HelgoboxService: Send + Sync + 'static {
        /// General global queries
        async fn get_host_info(
            &self,
            request: tonic::Request<super::GetHostInfoRequest>,
        ) -> std::result::Result<
            tonic::Response<super::GetHostInfoReply>,
            tonic::Status,
        >;
        async fn prove_authenticity(
            &self,
            request: tonic::Request<super::ProveAuthenticityRequest>,
        ) -> std::result::Result<
            tonic::Response<super::ProveAuthenticityReply>,
            tonic::Status,
        >;
        async fn get_app_settings(
            &self,
            request: tonic::Request<super::GetAppSettingsRequest>,
        ) -> std::result::Result<
            tonic::Response<super::GetAppSettingsReply>,
            tonic::Status,
        >;
        /// General instance queries
        async fn get_custom_instance_data(
            &self,
            request: tonic::Request<super::GetCustomInstanceDataRequest>,
        ) -> std::result::Result<
            tonic::Response<super::GetCustomInstanceDataReply>,
            tonic::Status,
        >;
        /// Playtime matrix queries
        async fn get_project_dir(
            &self,
            request: tonic::Request<super::GetProjectDirRequest>,
        ) -> std::result::Result<
            tonic::Response<super::GetProjectDirReply>,
            tonic::Status,
        >;
        async fn get_arrangement_info(
            &self,
            request: tonic::Request<super::GetArrangementInfoRequest>,
        ) -> std::result::Result<
            tonic::Response<super::GetArrangementInfoReply>,
            tonic::Status,
        >;
        /// Playtime clip queries
        async fn get_clip_detail(
            &self,
            request: tonic::Request<super::GetClipDetailRequest>,
        ) -> std::result::Result<
            tonic::Response<super::GetClipDetailReply>,
            tonic::Status,
        >;
        /// ReaLearn compartment queries
        async fn get_compartment_data(
            &self,
            request: tonic::Request<super::GetCompartmentDataRequest>,
        ) -> std::result::Result<
            tonic::Response<super::GetCompartmentDataReply>,
            tonic::Status,
        >;
        /// General global commands
        async fn trigger_global(
            &self,
            request: tonic::Request<super::TriggerGlobalRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        async fn set_app_settings(
            &self,
            request: tonic::Request<super::SetAppSettingsRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        async fn add_license(
            &self,
            request: tonic::Request<super::AddLicenseRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        async fn save_controller(
            &self,
            request: tonic::Request<super::SaveControllerRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        async fn delete_controller(
            &self,
            request: tonic::Request<super::DeleteControllerRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        /// Playtime global commands
        async fn set_playtime_engine_settings(
            &self,
            request: tonic::Request<super::SetPlaytimeEngineSettingsRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        /// General instance commands
        async fn trigger_instance(
            &self,
            request: tonic::Request<super::TriggerInstanceRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        async fn set_instance_settings(
            &self,
            request: tonic::Request<super::SetInstanceSettingsRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        async fn set_custom_instance_data(
            &self,
            request: tonic::Request<super::SetCustomInstanceDataRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        /// Playtime matrix commands
        async fn trigger_matrix(
            &self,
            request: tonic::Request<super::TriggerMatrixRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        async fn set_matrix_settings(
            &self,
            request: tonic::Request<super::SetMatrixSettingsRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        async fn set_matrix_tempo(
            &self,
            request: tonic::Request<super::SetMatrixTempoRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        async fn set_matrix_time_signature(
            &self,
            request: tonic::Request<super::SetMatrixTimeSignatureRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        async fn set_matrix_play_rate(
            &self,
            request: tonic::Request<super::SetMatrixPlayRateRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        async fn set_matrix_volume(
            &self,
            request: tonic::Request<super::SetMatrixVolumeRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        async fn set_matrix_pan(
            &self,
            request: tonic::Request<super::SetMatrixPanRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        /// Playtime column commands
        async fn trigger_column(
            &self,
            request: tonic::Request<super::TriggerColumnRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        async fn insert_columns(
            &self,
            request: tonic::Request<super::InsertColumnsRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        async fn set_column_settings(
            &self,
            request: tonic::Request<super::SetColumnSettingsRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        async fn set_column_track(
            &self,
            request: tonic::Request<super::SetColumnTrackRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        async fn drag_column(
            &self,
            request: tonic::Request<super::DragColumnRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        /// Playtime track commands
        async fn trigger_track(
            &self,
            request: tonic::Request<super::TriggerTrackRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        async fn set_track_name(
            &self,
            request: tonic::Request<super::SetTrackNameRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        async fn set_track_color(
            &self,
            request: tonic::Request<super::SetTrackColorRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        async fn set_track_input(
            &self,
            request: tonic::Request<super::SetTrackInputRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        async fn set_track_input_monitoring(
            &self,
            request: tonic::Request<super::SetTrackInputMonitoringRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        async fn set_track_volume(
            &self,
            request: tonic::Request<super::SetTrackVolumeRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        async fn set_track_pan(
            &self,
            request: tonic::Request<super::SetTrackPanRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        async fn open_track_fx(
            &self,
            request: tonic::Request<super::OpenTrackFxRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        /// Playtime row commands
        async fn trigger_row(
            &self,
            request: tonic::Request<super::TriggerRowRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        async fn set_row_data(
            &self,
            request: tonic::Request<super::SetRowDataRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        async fn drag_row(
            &self,
            request: tonic::Request<super::DragRowRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        /// Playtime slot commands
        async fn trigger_slot(
            &self,
            request: tonic::Request<super::TriggerSlotRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        async fn drag_slot(
            &self,
            request: tonic::Request<super::DragSlotRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        async fn import_files(
            &self,
            request: tonic::Request<super::ImportFilesRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        /// Playtime clip commands
        async fn trigger_clip(
            &self,
            request: tonic::Request<super::TriggerClipRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        async fn set_clip_name(
            &self,
            request: tonic::Request<super::SetClipNameRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        async fn set_clip_data(
            &self,
            request: tonic::Request<super::SetClipDataRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        async fn drag_clip(
            &self,
            request: tonic::Request<super::DragClipRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        /// Playtime sequence commands
        async fn trigger_sequence(
            &self,
            request: tonic::Request<super::TriggerSequenceRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        async fn set_sequence_info(
            &self,
            request: tonic::Request<super::SetSequenceInfoRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        /// ReaLearn compartment commands
        async fn save_custom_compartment_data(
            &self,
            request: tonic::Request<super::SaveCustomCompartmentDataRequest>,
        ) -> std::result::Result<tonic::Response<super::Empty>, tonic::Status>;
        /// Server streaming response type for the GetOccasionalGlobalUpdates method.
        type GetOccasionalGlobalUpdatesStream: tonic::codegen::tokio_stream::Stream<
                Item = std::result::Result<
                    super::GetOccasionalGlobalUpdatesReply,
                    tonic::Status,
                >,
            >
            + Send
            + 'static;
        /// General global events
        async fn get_occasional_global_updates(
            &self,
            request: tonic::Request<super::GetOccasionalGlobalUpdatesRequest>,
        ) -> std::result::Result<
            tonic::Response<Self::GetOccasionalGlobalUpdatesStream>,
            tonic::Status,
        >;
        /// Server streaming response type for the GetOccasionalPlaytimeEngineUpdates method.
        type GetOccasionalPlaytimeEngineUpdatesStream: tonic::codegen::tokio_stream::Stream<
                Item = std::result::Result<
                    super::GetOccasionalPlaytimeEngineUpdatesReply,
                    tonic::Status,
                >,
            >
            + Send
            + 'static;
        /// Playtime global events
        async fn get_occasional_playtime_engine_updates(
            &self,
            request: tonic::Request<super::GetOccasionalPlaytimeEngineUpdatesRequest>,
        ) -> std::result::Result<
            tonic::Response<Self::GetOccasionalPlaytimeEngineUpdatesStream>,
            tonic::Status,
        >;
        /// Server streaming response type for the GetOccasionalInstanceUpdates method.
        type GetOccasionalInstanceUpdatesStream: tonic::codegen::tokio_stream::Stream<
                Item = std::result::Result<
                    super::GetOccasionalInstanceUpdatesReply,
                    tonic::Status,
                >,
            >
            + Send
            + 'static;
        /// General instance events
        async fn get_occasional_instance_updates(
            &self,
            request: tonic::Request<super::GetOccasionalInstanceUpdatesRequest>,
        ) -> std::result::Result<
            tonic::Response<Self::GetOccasionalInstanceUpdatesStream>,
            tonic::Status,
        >;
        /// Server streaming response type for the GetOccasionalUnitUpdates method.
        type GetOccasionalUnitUpdatesStream: tonic::codegen::tokio_stream::Stream<
                Item = std::result::Result<
                    super::GetOccasionalUnitUpdatesReply,
                    tonic::Status,
                >,
            >
            + Send
            + 'static;
        async fn get_occasional_unit_updates(
            &self,
            request: tonic::Request<super::GetOccasionalUnitUpdatesRequest>,
        ) -> std::result::Result<
            tonic::Response<Self::GetOccasionalUnitUpdatesStream>,
            tonic::Status,
        >;
        /// Server streaming response type for the GetOccasionalMatrixUpdates method.
        type GetOccasionalMatrixUpdatesStream: tonic::codegen::tokio_stream::Stream<
                Item = std::result::Result<
                    super::GetOccasionalMatrixUpdatesReply,
                    tonic::Status,
                >,
            >
            + Send
            + 'static;
        /// Playtime matrix events
        async fn get_occasional_matrix_updates(
            &self,
            request: tonic::Request<super::GetOccasionalMatrixUpdatesRequest>,
        ) -> std::result::Result<
            tonic::Response<Self::GetOccasionalMatrixUpdatesStream>,
            tonic::Status,
        >;
        /// Server streaming response type for the GetContinuousMatrixUpdates method.
        type GetContinuousMatrixUpdatesStream: tonic::codegen::tokio_stream::Stream<
                Item = std::result::Result<
                    super::GetContinuousMatrixUpdatesReply,
                    tonic::Status,
                >,
            >
            + Send
            + 'static;
        async fn get_continuous_matrix_updates(
            &self,
            request: tonic::Request<super::GetContinuousMatrixUpdatesRequest>,
        ) -> std::result::Result<
            tonic::Response<Self::GetContinuousMatrixUpdatesStream>,
            tonic::Status,
        >;
        /// Server streaming response type for the GetOccasionalColumnUpdates method.
        type GetOccasionalColumnUpdatesStream: tonic::codegen::tokio_stream::Stream<
                Item = std::result::Result<
                    super::GetOccasionalColumnUpdatesReply,
                    tonic::Status,
                >,
            >
            + Send
            + 'static;
        /// Playtime column events
        async fn get_occasional_column_updates(
            &self,
            request: tonic::Request<super::GetOccasionalColumnUpdatesRequest>,
        ) -> std::result::Result<
            tonic::Response<Self::GetOccasionalColumnUpdatesStream>,
            tonic::Status,
        >;
        /// Server streaming response type for the GetContinuousColumnUpdates method.
        type GetContinuousColumnUpdatesStream: tonic::codegen::tokio_stream::Stream<
                Item = std::result::Result<
                    super::GetContinuousColumnUpdatesReply,
                    tonic::Status,
                >,
            >
            + Send
            + 'static;
        async fn get_continuous_column_updates(
            &self,
            request: tonic::Request<super::GetContinuousColumnUpdatesRequest>,
        ) -> std::result::Result<
            tonic::Response<Self::GetContinuousColumnUpdatesStream>,
            tonic::Status,
        >;
        /// Server streaming response type for the GetOccasionalTrackUpdates method.
        type GetOccasionalTrackUpdatesStream: tonic::codegen::tokio_stream::Stream<
                Item = std::result::Result<
                    super::GetOccasionalTrackUpdatesReply,
                    tonic::Status,
                >,
            >
            + Send
            + 'static;
        /// Playtime track events
        async fn get_occasional_track_updates(
            &self,
            request: tonic::Request<super::GetOccasionalTrackUpdatesRequest>,
        ) -> std::result::Result<
            tonic::Response<Self::GetOccasionalTrackUpdatesStream>,
            tonic::Status,
        >;
        /// Server streaming response type for the GetOccasionalRowUpdates method.
        type GetOccasionalRowUpdatesStream: tonic::codegen::tokio_stream::Stream<
                Item = std::result::Result<
                    super::GetOccasionalRowUpdatesReply,
                    tonic::Status,
                >,
            >
            + Send
            + 'static;
        /// Playtime row events
        async fn get_occasional_row_updates(
            &self,
            request: tonic::Request<super::GetOccasionalRowUpdatesRequest>,
        ) -> std::result::Result<
            tonic::Response<Self::GetOccasionalRowUpdatesStream>,
            tonic::Status,
        >;
        /// Server streaming response type for the GetOccasionalSlotUpdates method.
        type GetOccasionalSlotUpdatesStream: tonic::codegen::tokio_stream::Stream<
                Item = std::result::Result<
                    super::GetOccasionalSlotUpdatesReply,
                    tonic::Status,
                >,
            >
            + Send
            + 'static;
        /// Playtime slot events
        async fn get_occasional_slot_updates(
            &self,
            request: tonic::Request<super::GetOccasionalSlotUpdatesRequest>,
        ) -> std::result::Result<
            tonic::Response<Self::GetOccasionalSlotUpdatesStream>,
            tonic::Status,
        >;
        /// Server streaming response type for the GetContinuousSlotUpdates method.
        type GetContinuousSlotUpdatesStream: tonic::codegen::tokio_stream::Stream<
                Item = std::result::Result<
                    super::GetContinuousSlotUpdatesReply,
                    tonic::Status,
                >,
            >
            + Send
            + 'static;
        async fn get_continuous_slot_updates(
            &self,
            request: tonic::Request<super::GetContinuousSlotUpdatesRequest>,
        ) -> std::result::Result<
            tonic::Response<Self::GetContinuousSlotUpdatesStream>,
            tonic::Status,
        >;
        /// Server streaming response type for the GetOccasionalClipUpdates method.
        type GetOccasionalClipUpdatesStream: tonic::codegen::tokio_stream::Stream<
                Item = std::result::Result<
                    super::GetOccasionalClipUpdatesReply,
                    tonic::Status,
                >,
            >
            + Send
            + 'static;
        /// Playtime clip events
        async fn get_occasional_clip_updates(
            &self,
            request: tonic::Request<super::GetOccasionalClipUpdatesRequest>,
        ) -> std::result::Result<
            tonic::Response<Self::GetOccasionalClipUpdatesStream>,
            tonic::Status,
        >;
    }
    #[derive(Debug)]
    pub struct HelgoboxServiceServer<T: HelgoboxService> {
        inner: _Inner<T>,
        accept_compression_encodings: EnabledCompressionEncodings,
        send_compression_encodings: EnabledCompressionEncodings,
        max_decoding_message_size: Option<usize>,
        max_encoding_message_size: Option<usize>,
    }
    struct _Inner<T>(Arc<T>);
    impl<T: HelgoboxService> HelgoboxServiceServer<T> {
        pub fn new(inner: T) -> Self {
            Self::from_arc(Arc::new(inner))
        }
        pub fn from_arc(inner: Arc<T>) -> Self {
            let inner = _Inner(inner);
            Self {
                inner,
                accept_compression_encodings: Default::default(),
                send_compression_encodings: Default::default(),
                max_decoding_message_size: None,
                max_encoding_message_size: None,
            }
        }
        pub fn with_interceptor<F>(
            inner: T,
            interceptor: F,
        ) -> InterceptedService<Self, F>
        where
            F: tonic::service::Interceptor,
        {
            InterceptedService::new(Self::new(inner), interceptor)
        }
        /// Enable decompressing requests with the given encoding.
        #[must_use]
        pub fn accept_compressed(mut self, encoding: CompressionEncoding) -> Self {
            self.accept_compression_encodings.enable(encoding);
            self
        }
        /// Compress responses with the given encoding, if the client supports it.
        #[must_use]
        pub fn send_compressed(mut self, encoding: CompressionEncoding) -> Self {
            self.send_compression_encodings.enable(encoding);
            self
        }
        /// Limits the maximum size of a decoded message.
        ///
        /// Default: `4MB`
        #[must_use]
        pub fn max_decoding_message_size(mut self, limit: usize) -> Self {
            self.max_decoding_message_size = Some(limit);
            self
        }
        /// Limits the maximum size of an encoded message.
        ///
        /// Default: `usize::MAX`
        #[must_use]
        pub fn max_encoding_message_size(mut self, limit: usize) -> Self {
            self.max_encoding_message_size = Some(limit);
            self
        }
    }
    impl<T, B> tonic::codegen::Service<http::Request<B>> for HelgoboxServiceServer<T>
    where
        T: HelgoboxService,
        B: Body + Send + 'static,
        B::Error: Into<StdError> + Send + 'static,
    {
        type Response = http::Response<tonic::body::BoxBody>;
        type Error = std::convert::Infallible;
        type Future = BoxFuture<Self::Response, Self::Error>;
        fn poll_ready(
            &mut self,
            _cx: &mut Context<'_>,
        ) -> Poll<std::result::Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
        fn call(&mut self, req: http::Request<B>) -> Self::Future {
            let inner = self.inner.clone();
            match req.uri().path() {
                "/generated.HelgoboxService/GetHostInfo" => {
                    #[allow(non_camel_case_types)]
                    struct GetHostInfoSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::GetHostInfoRequest>
                    for GetHostInfoSvc<T> {
                        type Response = super::GetHostInfoReply;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::GetHostInfoRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::get_host_info(&inner, request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = GetHostInfoSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/ProveAuthenticity" => {
                    #[allow(non_camel_case_types)]
                    struct ProveAuthenticitySvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::ProveAuthenticityRequest>
                    for ProveAuthenticitySvc<T> {
                        type Response = super::ProveAuthenticityReply;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::ProveAuthenticityRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::prove_authenticity(&inner, request)
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = ProveAuthenticitySvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/GetAppSettings" => {
                    #[allow(non_camel_case_types)]
                    struct GetAppSettingsSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::GetAppSettingsRequest>
                    for GetAppSettingsSvc<T> {
                        type Response = super::GetAppSettingsReply;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::GetAppSettingsRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::get_app_settings(&inner, request)
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = GetAppSettingsSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/GetCustomInstanceData" => {
                    #[allow(non_camel_case_types)]
                    struct GetCustomInstanceDataSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::GetCustomInstanceDataRequest>
                    for GetCustomInstanceDataSvc<T> {
                        type Response = super::GetCustomInstanceDataReply;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::GetCustomInstanceDataRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::get_custom_instance_data(
                                        &inner,
                                        request,
                                    )
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = GetCustomInstanceDataSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/GetProjectDir" => {
                    #[allow(non_camel_case_types)]
                    struct GetProjectDirSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::GetProjectDirRequest>
                    for GetProjectDirSvc<T> {
                        type Response = super::GetProjectDirReply;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::GetProjectDirRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::get_project_dir(&inner, request)
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = GetProjectDirSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/GetArrangementInfo" => {
                    #[allow(non_camel_case_types)]
                    struct GetArrangementInfoSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::GetArrangementInfoRequest>
                    for GetArrangementInfoSvc<T> {
                        type Response = super::GetArrangementInfoReply;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::GetArrangementInfoRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::get_arrangement_info(
                                        &inner,
                                        request,
                                    )
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = GetArrangementInfoSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/GetClipDetail" => {
                    #[allow(non_camel_case_types)]
                    struct GetClipDetailSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::GetClipDetailRequest>
                    for GetClipDetailSvc<T> {
                        type Response = super::GetClipDetailReply;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::GetClipDetailRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::get_clip_detail(&inner, request)
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = GetClipDetailSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/GetCompartmentData" => {
                    #[allow(non_camel_case_types)]
                    struct GetCompartmentDataSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::GetCompartmentDataRequest>
                    for GetCompartmentDataSvc<T> {
                        type Response = super::GetCompartmentDataReply;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::GetCompartmentDataRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::get_compartment_data(
                                        &inner,
                                        request,
                                    )
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = GetCompartmentDataSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/TriggerGlobal" => {
                    #[allow(non_camel_case_types)]
                    struct TriggerGlobalSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::TriggerGlobalRequest>
                    for TriggerGlobalSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::TriggerGlobalRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::trigger_global(&inner, request)
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = TriggerGlobalSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/SetAppSettings" => {
                    #[allow(non_camel_case_types)]
                    struct SetAppSettingsSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::SetAppSettingsRequest>
                    for SetAppSettingsSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::SetAppSettingsRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::set_app_settings(&inner, request)
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = SetAppSettingsSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/AddLicense" => {
                    #[allow(non_camel_case_types)]
                    struct AddLicenseSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::AddLicenseRequest>
                    for AddLicenseSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::AddLicenseRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::add_license(&inner, request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = AddLicenseSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/SaveController" => {
                    #[allow(non_camel_case_types)]
                    struct SaveControllerSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::SaveControllerRequest>
                    for SaveControllerSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::SaveControllerRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::save_controller(&inner, request)
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = SaveControllerSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/DeleteController" => {
                    #[allow(non_camel_case_types)]
                    struct DeleteControllerSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::DeleteControllerRequest>
                    for DeleteControllerSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::DeleteControllerRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::delete_controller(&inner, request)
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = DeleteControllerSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/SetPlaytimeEngineSettings" => {
                    #[allow(non_camel_case_types)]
                    struct SetPlaytimeEngineSettingsSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<
                        super::SetPlaytimeEngineSettingsRequest,
                    > for SetPlaytimeEngineSettingsSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<
                                super::SetPlaytimeEngineSettingsRequest,
                            >,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::set_playtime_engine_settings(
                                        &inner,
                                        request,
                                    )
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = SetPlaytimeEngineSettingsSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/TriggerInstance" => {
                    #[allow(non_camel_case_types)]
                    struct TriggerInstanceSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::TriggerInstanceRequest>
                    for TriggerInstanceSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::TriggerInstanceRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::trigger_instance(&inner, request)
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = TriggerInstanceSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/SetInstanceSettings" => {
                    #[allow(non_camel_case_types)]
                    struct SetInstanceSettingsSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::SetInstanceSettingsRequest>
                    for SetInstanceSettingsSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::SetInstanceSettingsRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::set_instance_settings(
                                        &inner,
                                        request,
                                    )
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = SetInstanceSettingsSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/SetCustomInstanceData" => {
                    #[allow(non_camel_case_types)]
                    struct SetCustomInstanceDataSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::SetCustomInstanceDataRequest>
                    for SetCustomInstanceDataSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::SetCustomInstanceDataRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::set_custom_instance_data(
                                        &inner,
                                        request,
                                    )
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = SetCustomInstanceDataSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/TriggerMatrix" => {
                    #[allow(non_camel_case_types)]
                    struct TriggerMatrixSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::TriggerMatrixRequest>
                    for TriggerMatrixSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::TriggerMatrixRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::trigger_matrix(&inner, request)
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = TriggerMatrixSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/SetMatrixSettings" => {
                    #[allow(non_camel_case_types)]
                    struct SetMatrixSettingsSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::SetMatrixSettingsRequest>
                    for SetMatrixSettingsSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::SetMatrixSettingsRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::set_matrix_settings(&inner, request)
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = SetMatrixSettingsSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/SetMatrixTempo" => {
                    #[allow(non_camel_case_types)]
                    struct SetMatrixTempoSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::SetMatrixTempoRequest>
                    for SetMatrixTempoSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::SetMatrixTempoRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::set_matrix_tempo(&inner, request)
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = SetMatrixTempoSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/SetMatrixTimeSignature" => {
                    #[allow(non_camel_case_types)]
                    struct SetMatrixTimeSignatureSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::SetMatrixTimeSignatureRequest>
                    for SetMatrixTimeSignatureSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::SetMatrixTimeSignatureRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::set_matrix_time_signature(
                                        &inner,
                                        request,
                                    )
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = SetMatrixTimeSignatureSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/SetMatrixPlayRate" => {
                    #[allow(non_camel_case_types)]
                    struct SetMatrixPlayRateSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::SetMatrixPlayRateRequest>
                    for SetMatrixPlayRateSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::SetMatrixPlayRateRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::set_matrix_play_rate(
                                        &inner,
                                        request,
                                    )
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = SetMatrixPlayRateSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/SetMatrixVolume" => {
                    #[allow(non_camel_case_types)]
                    struct SetMatrixVolumeSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::SetMatrixVolumeRequest>
                    for SetMatrixVolumeSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::SetMatrixVolumeRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::set_matrix_volume(&inner, request)
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = SetMatrixVolumeSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/SetMatrixPan" => {
                    #[allow(non_camel_case_types)]
                    struct SetMatrixPanSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::SetMatrixPanRequest>
                    for SetMatrixPanSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::SetMatrixPanRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::set_matrix_pan(&inner, request)
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = SetMatrixPanSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/TriggerColumn" => {
                    #[allow(non_camel_case_types)]
                    struct TriggerColumnSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::TriggerColumnRequest>
                    for TriggerColumnSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::TriggerColumnRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::trigger_column(&inner, request)
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = TriggerColumnSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/InsertColumns" => {
                    #[allow(non_camel_case_types)]
                    struct InsertColumnsSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::InsertColumnsRequest>
                    for InsertColumnsSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::InsertColumnsRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::insert_columns(&inner, request)
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = InsertColumnsSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/SetColumnSettings" => {
                    #[allow(non_camel_case_types)]
                    struct SetColumnSettingsSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::SetColumnSettingsRequest>
                    for SetColumnSettingsSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::SetColumnSettingsRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::set_column_settings(&inner, request)
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = SetColumnSettingsSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/SetColumnTrack" => {
                    #[allow(non_camel_case_types)]
                    struct SetColumnTrackSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::SetColumnTrackRequest>
                    for SetColumnTrackSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::SetColumnTrackRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::set_column_track(&inner, request)
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = SetColumnTrackSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/DragColumn" => {
                    #[allow(non_camel_case_types)]
                    struct DragColumnSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::DragColumnRequest>
                    for DragColumnSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::DragColumnRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::drag_column(&inner, request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = DragColumnSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/TriggerTrack" => {
                    #[allow(non_camel_case_types)]
                    struct TriggerTrackSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::TriggerTrackRequest>
                    for TriggerTrackSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::TriggerTrackRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::trigger_track(&inner, request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = TriggerTrackSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/SetTrackName" => {
                    #[allow(non_camel_case_types)]
                    struct SetTrackNameSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::SetTrackNameRequest>
                    for SetTrackNameSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::SetTrackNameRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::set_track_name(&inner, request)
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = SetTrackNameSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/SetTrackColor" => {
                    #[allow(non_camel_case_types)]
                    struct SetTrackColorSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::SetTrackColorRequest>
                    for SetTrackColorSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::SetTrackColorRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::set_track_color(&inner, request)
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = SetTrackColorSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/SetTrackInput" => {
                    #[allow(non_camel_case_types)]
                    struct SetTrackInputSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::SetTrackInputRequest>
                    for SetTrackInputSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::SetTrackInputRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::set_track_input(&inner, request)
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = SetTrackInputSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/SetTrackInputMonitoring" => {
                    #[allow(non_camel_case_types)]
                    struct SetTrackInputMonitoringSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::SetTrackInputMonitoringRequest>
                    for SetTrackInputMonitoringSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<
                                super::SetTrackInputMonitoringRequest,
                            >,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::set_track_input_monitoring(
                                        &inner,
                                        request,
                                    )
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = SetTrackInputMonitoringSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/SetTrackVolume" => {
                    #[allow(non_camel_case_types)]
                    struct SetTrackVolumeSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::SetTrackVolumeRequest>
                    for SetTrackVolumeSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::SetTrackVolumeRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::set_track_volume(&inner, request)
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = SetTrackVolumeSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/SetTrackPan" => {
                    #[allow(non_camel_case_types)]
                    struct SetTrackPanSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::SetTrackPanRequest>
                    for SetTrackPanSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::SetTrackPanRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::set_track_pan(&inner, request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = SetTrackPanSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/OpenTrackFx" => {
                    #[allow(non_camel_case_types)]
                    struct OpenTrackFxSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::OpenTrackFxRequest>
                    for OpenTrackFxSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::OpenTrackFxRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::open_track_fx(&inner, request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = OpenTrackFxSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/TriggerRow" => {
                    #[allow(non_camel_case_types)]
                    struct TriggerRowSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::TriggerRowRequest>
                    for TriggerRowSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::TriggerRowRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::trigger_row(&inner, request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = TriggerRowSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/SetRowData" => {
                    #[allow(non_camel_case_types)]
                    struct SetRowDataSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::SetRowDataRequest>
                    for SetRowDataSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::SetRowDataRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::set_row_data(&inner, request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = SetRowDataSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/DragRow" => {
                    #[allow(non_camel_case_types)]
                    struct DragRowSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::DragRowRequest>
                    for DragRowSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::DragRowRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::drag_row(&inner, request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = DragRowSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/TriggerSlot" => {
                    #[allow(non_camel_case_types)]
                    struct TriggerSlotSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::TriggerSlotRequest>
                    for TriggerSlotSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::TriggerSlotRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::trigger_slot(&inner, request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = TriggerSlotSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/DragSlot" => {
                    #[allow(non_camel_case_types)]
                    struct DragSlotSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::DragSlotRequest>
                    for DragSlotSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::DragSlotRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::drag_slot(&inner, request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = DragSlotSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/ImportFiles" => {
                    #[allow(non_camel_case_types)]
                    struct ImportFilesSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::ImportFilesRequest>
                    for ImportFilesSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::ImportFilesRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::import_files(&inner, request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = ImportFilesSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/TriggerClip" => {
                    #[allow(non_camel_case_types)]
                    struct TriggerClipSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::TriggerClipRequest>
                    for TriggerClipSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::TriggerClipRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::trigger_clip(&inner, request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = TriggerClipSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/SetClipName" => {
                    #[allow(non_camel_case_types)]
                    struct SetClipNameSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::SetClipNameRequest>
                    for SetClipNameSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::SetClipNameRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::set_clip_name(&inner, request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = SetClipNameSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/SetClipData" => {
                    #[allow(non_camel_case_types)]
                    struct SetClipDataSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::SetClipDataRequest>
                    for SetClipDataSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::SetClipDataRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::set_clip_data(&inner, request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = SetClipDataSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/DragClip" => {
                    #[allow(non_camel_case_types)]
                    struct DragClipSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::DragClipRequest>
                    for DragClipSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::DragClipRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::drag_clip(&inner, request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = DragClipSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/TriggerSequence" => {
                    #[allow(non_camel_case_types)]
                    struct TriggerSequenceSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::TriggerSequenceRequest>
                    for TriggerSequenceSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::TriggerSequenceRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::trigger_sequence(&inner, request)
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = TriggerSequenceSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/SetSequenceInfo" => {
                    #[allow(non_camel_case_types)]
                    struct SetSequenceInfoSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<super::SetSequenceInfoRequest>
                    for SetSequenceInfoSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::SetSequenceInfoRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::set_sequence_info(&inner, request)
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = SetSequenceInfoSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/SaveCustomCompartmentData" => {
                    #[allow(non_camel_case_types)]
                    struct SaveCustomCompartmentDataSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::UnaryService<
                        super::SaveCustomCompartmentDataRequest,
                    > for SaveCustomCompartmentDataSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<
                                super::SaveCustomCompartmentDataRequest,
                            >,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::save_custom_compartment_data(
                                        &inner,
                                        request,
                                    )
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = SaveCustomCompartmentDataSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/GetOccasionalGlobalUpdates" => {
                    #[allow(non_camel_case_types)]
                    struct GetOccasionalGlobalUpdatesSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::ServerStreamingService<
                        super::GetOccasionalGlobalUpdatesRequest,
                    > for GetOccasionalGlobalUpdatesSvc<T> {
                        type Response = super::GetOccasionalGlobalUpdatesReply;
                        type ResponseStream = T::GetOccasionalGlobalUpdatesStream;
                        type Future = BoxFuture<
                            tonic::Response<Self::ResponseStream>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<
                                super::GetOccasionalGlobalUpdatesRequest,
                            >,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::get_occasional_global_updates(
                                        &inner,
                                        request,
                                    )
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = GetOccasionalGlobalUpdatesSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.server_streaming(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/GetOccasionalPlaytimeEngineUpdates" => {
                    #[allow(non_camel_case_types)]
                    struct GetOccasionalPlaytimeEngineUpdatesSvc<T: HelgoboxService>(
                        pub Arc<T>,
                    );
                    impl<
                        T: HelgoboxService,
                    > tonic::server::ServerStreamingService<
                        super::GetOccasionalPlaytimeEngineUpdatesRequest,
                    > for GetOccasionalPlaytimeEngineUpdatesSvc<T> {
                        type Response = super::GetOccasionalPlaytimeEngineUpdatesReply;
                        type ResponseStream = T::GetOccasionalPlaytimeEngineUpdatesStream;
                        type Future = BoxFuture<
                            tonic::Response<Self::ResponseStream>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<
                                super::GetOccasionalPlaytimeEngineUpdatesRequest,
                            >,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::get_occasional_playtime_engine_updates(
                                        &inner,
                                        request,
                                    )
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = GetOccasionalPlaytimeEngineUpdatesSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.server_streaming(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/GetOccasionalInstanceUpdates" => {
                    #[allow(non_camel_case_types)]
                    struct GetOccasionalInstanceUpdatesSvc<T: HelgoboxService>(
                        pub Arc<T>,
                    );
                    impl<
                        T: HelgoboxService,
                    > tonic::server::ServerStreamingService<
                        super::GetOccasionalInstanceUpdatesRequest,
                    > for GetOccasionalInstanceUpdatesSvc<T> {
                        type Response = super::GetOccasionalInstanceUpdatesReply;
                        type ResponseStream = T::GetOccasionalInstanceUpdatesStream;
                        type Future = BoxFuture<
                            tonic::Response<Self::ResponseStream>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<
                                super::GetOccasionalInstanceUpdatesRequest,
                            >,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::get_occasional_instance_updates(
                                        &inner,
                                        request,
                                    )
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = GetOccasionalInstanceUpdatesSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.server_streaming(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/GetOccasionalUnitUpdates" => {
                    #[allow(non_camel_case_types)]
                    struct GetOccasionalUnitUpdatesSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::ServerStreamingService<
                        super::GetOccasionalUnitUpdatesRequest,
                    > for GetOccasionalUnitUpdatesSvc<T> {
                        type Response = super::GetOccasionalUnitUpdatesReply;
                        type ResponseStream = T::GetOccasionalUnitUpdatesStream;
                        type Future = BoxFuture<
                            tonic::Response<Self::ResponseStream>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<
                                super::GetOccasionalUnitUpdatesRequest,
                            >,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::get_occasional_unit_updates(
                                        &inner,
                                        request,
                                    )
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = GetOccasionalUnitUpdatesSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.server_streaming(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/GetOccasionalMatrixUpdates" => {
                    #[allow(non_camel_case_types)]
                    struct GetOccasionalMatrixUpdatesSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::ServerStreamingService<
                        super::GetOccasionalMatrixUpdatesRequest,
                    > for GetOccasionalMatrixUpdatesSvc<T> {
                        type Response = super::GetOccasionalMatrixUpdatesReply;
                        type ResponseStream = T::GetOccasionalMatrixUpdatesStream;
                        type Future = BoxFuture<
                            tonic::Response<Self::ResponseStream>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<
                                super::GetOccasionalMatrixUpdatesRequest,
                            >,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::get_occasional_matrix_updates(
                                        &inner,
                                        request,
                                    )
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = GetOccasionalMatrixUpdatesSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.server_streaming(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/GetContinuousMatrixUpdates" => {
                    #[allow(non_camel_case_types)]
                    struct GetContinuousMatrixUpdatesSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::ServerStreamingService<
                        super::GetContinuousMatrixUpdatesRequest,
                    > for GetContinuousMatrixUpdatesSvc<T> {
                        type Response = super::GetContinuousMatrixUpdatesReply;
                        type ResponseStream = T::GetContinuousMatrixUpdatesStream;
                        type Future = BoxFuture<
                            tonic::Response<Self::ResponseStream>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<
                                super::GetContinuousMatrixUpdatesRequest,
                            >,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::get_continuous_matrix_updates(
                                        &inner,
                                        request,
                                    )
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = GetContinuousMatrixUpdatesSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.server_streaming(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/GetOccasionalColumnUpdates" => {
                    #[allow(non_camel_case_types)]
                    struct GetOccasionalColumnUpdatesSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::ServerStreamingService<
                        super::GetOccasionalColumnUpdatesRequest,
                    > for GetOccasionalColumnUpdatesSvc<T> {
                        type Response = super::GetOccasionalColumnUpdatesReply;
                        type ResponseStream = T::GetOccasionalColumnUpdatesStream;
                        type Future = BoxFuture<
                            tonic::Response<Self::ResponseStream>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<
                                super::GetOccasionalColumnUpdatesRequest,
                            >,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::get_occasional_column_updates(
                                        &inner,
                                        request,
                                    )
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = GetOccasionalColumnUpdatesSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.server_streaming(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/GetContinuousColumnUpdates" => {
                    #[allow(non_camel_case_types)]
                    struct GetContinuousColumnUpdatesSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::ServerStreamingService<
                        super::GetContinuousColumnUpdatesRequest,
                    > for GetContinuousColumnUpdatesSvc<T> {
                        type Response = super::GetContinuousColumnUpdatesReply;
                        type ResponseStream = T::GetContinuousColumnUpdatesStream;
                        type Future = BoxFuture<
                            tonic::Response<Self::ResponseStream>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<
                                super::GetContinuousColumnUpdatesRequest,
                            >,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::get_continuous_column_updates(
                                        &inner,
                                        request,
                                    )
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = GetContinuousColumnUpdatesSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.server_streaming(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/GetOccasionalTrackUpdates" => {
                    #[allow(non_camel_case_types)]
                    struct GetOccasionalTrackUpdatesSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::ServerStreamingService<
                        super::GetOccasionalTrackUpdatesRequest,
                    > for GetOccasionalTrackUpdatesSvc<T> {
                        type Response = super::GetOccasionalTrackUpdatesReply;
                        type ResponseStream = T::GetOccasionalTrackUpdatesStream;
                        type Future = BoxFuture<
                            tonic::Response<Self::ResponseStream>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<
                                super::GetOccasionalTrackUpdatesRequest,
                            >,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::get_occasional_track_updates(
                                        &inner,
                                        request,
                                    )
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = GetOccasionalTrackUpdatesSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.server_streaming(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/GetOccasionalRowUpdates" => {
                    #[allow(non_camel_case_types)]
                    struct GetOccasionalRowUpdatesSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::ServerStreamingService<
                        super::GetOccasionalRowUpdatesRequest,
                    > for GetOccasionalRowUpdatesSvc<T> {
                        type Response = super::GetOccasionalRowUpdatesReply;
                        type ResponseStream = T::GetOccasionalRowUpdatesStream;
                        type Future = BoxFuture<
                            tonic::Response<Self::ResponseStream>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<
                                super::GetOccasionalRowUpdatesRequest,
                            >,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::get_occasional_row_updates(
                                        &inner,
                                        request,
                                    )
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = GetOccasionalRowUpdatesSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.server_streaming(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/GetOccasionalSlotUpdates" => {
                    #[allow(non_camel_case_types)]
                    struct GetOccasionalSlotUpdatesSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::ServerStreamingService<
                        super::GetOccasionalSlotUpdatesRequest,
                    > for GetOccasionalSlotUpdatesSvc<T> {
                        type Response = super::GetOccasionalSlotUpdatesReply;
                        type ResponseStream = T::GetOccasionalSlotUpdatesStream;
                        type Future = BoxFuture<
                            tonic::Response<Self::ResponseStream>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<
                                super::GetOccasionalSlotUpdatesRequest,
                            >,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::get_occasional_slot_updates(
                                        &inner,
                                        request,
                                    )
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = GetOccasionalSlotUpdatesSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.server_streaming(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/GetContinuousSlotUpdates" => {
                    #[allow(non_camel_case_types)]
                    struct GetContinuousSlotUpdatesSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::ServerStreamingService<
                        super::GetContinuousSlotUpdatesRequest,
                    > for GetContinuousSlotUpdatesSvc<T> {
                        type Response = super::GetContinuousSlotUpdatesReply;
                        type ResponseStream = T::GetContinuousSlotUpdatesStream;
                        type Future = BoxFuture<
                            tonic::Response<Self::ResponseStream>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<
                                super::GetContinuousSlotUpdatesRequest,
                            >,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::get_continuous_slot_updates(
                                        &inner,
                                        request,
                                    )
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = GetContinuousSlotUpdatesSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.server_streaming(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/generated.HelgoboxService/GetOccasionalClipUpdates" => {
                    #[allow(non_camel_case_types)]
                    struct GetOccasionalClipUpdatesSvc<T: HelgoboxService>(pub Arc<T>);
                    impl<
                        T: HelgoboxService,
                    > tonic::server::ServerStreamingService<
                        super::GetOccasionalClipUpdatesRequest,
                    > for GetOccasionalClipUpdatesSvc<T> {
                        type Response = super::GetOccasionalClipUpdatesReply;
                        type ResponseStream = T::GetOccasionalClipUpdatesStream;
                        type Future = BoxFuture<
                            tonic::Response<Self::ResponseStream>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<
                                super::GetOccasionalClipUpdatesRequest,
                            >,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as HelgoboxService>::get_occasional_clip_updates(
                                        &inner,
                                        request,
                                    )
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = GetOccasionalClipUpdatesSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.server_streaming(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                _ => {
                    Box::pin(async move {
                        Ok(
                            http::Response::builder()
                                .status(200)
                                .header("grpc-status", "12")
                                .header("content-type", "application/grpc")
                                .body(empty_body())
                                .unwrap(),
                        )
                    })
                }
            }
        }
    }
    impl<T: HelgoboxService> Clone for HelgoboxServiceServer<T> {
        fn clone(&self) -> Self {
            let inner = self.inner.clone();
            Self {
                inner,
                accept_compression_encodings: self.accept_compression_encodings,
                send_compression_encodings: self.send_compression_encodings,
                max_decoding_message_size: self.max_decoding_message_size,
                max_encoding_message_size: self.max_encoding_message_size,
            }
        }
    }
    impl<T: HelgoboxService> Clone for _Inner<T> {
        fn clone(&self) -> Self {
            Self(Arc::clone(&self.0))
        }
    }
    impl<T: std::fmt::Debug> std::fmt::Debug for _Inner<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{:?}", self.0)
        }
    }
    impl<T: HelgoboxService> tonic::server::NamedService for HelgoboxServiceServer<T> {
        const NAME: &'static str = "generated.HelgoboxService";
    }
}
