use crate::infrastructure::server::grpc::handlers::RealearnClipEngine;
use crate::infrastructure::server::layers::MainThreadLayer;
use playtime_clip_engine::proto::clip_engine_server::ClipEngineServer;
use playtime_clip_engine::proto::{
    ContinuousColumnUpdate, ContinuousMatrixUpdate, OccasionalMatrixUpdate,
    QualifiedContinuousSlotUpdate, QualifiedOccasionalClipUpdate, QualifiedOccasionalSlotUpdate,
    QualifiedOccasionalTrackUpdate,
};
use std::net::SocketAddr;
use tonic::transport::Server;

pub async fn start_grpc_server(address: SocketAddr) -> Result<(), tonic::transport::Error> {
    let clip_engine = RealearnClipEngine::default();
    Server::builder()
        .layer(MainThreadLayer)
        .add_service(ClipEngineServer::new(clip_engine))
        .serve(address)
        .await
}

#[derive(Clone)]
pub struct WithSessionId<T> {
    pub session_id: String,
    pub value: T,
}

pub type OccasionalMatrixUpdateBatch = WithSessionId<Vec<OccasionalMatrixUpdate>>;
pub type OccasionalTrackUpdateBatch = WithSessionId<Vec<QualifiedOccasionalTrackUpdate>>;
pub type OccasionalSlotUpdateBatch = WithSessionId<Vec<QualifiedOccasionalSlotUpdate>>;
pub type OccasionalClipUpdateBatch = WithSessionId<Vec<QualifiedOccasionalClipUpdate>>;
pub type ContinuousMatrixUpdateBatch = WithSessionId<ContinuousMatrixUpdate>;
pub type ContinuousColumnUpdateBatch = WithSessionId<Vec<ContinuousColumnUpdate>>;
pub type ContinuousSlotUpdateBatch = WithSessionId<Vec<QualifiedContinuousSlotUpdate>>;
