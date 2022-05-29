use crate::infrastructure::server::grpc::handlers::RealearnClipEngine;
use crate::infrastructure::server::layers::MainThreadLayer;
use playtime_clip_engine::proto::clip_engine_server::ClipEngineServer;
use playtime_clip_engine::proto::QualifiedContinuousSlotState;
use std::net::SocketAddr;
use tokio::sync::broadcast;
use tonic::transport::Server;

pub async fn start_grpc_server(
    address: SocketAddr,
    mut shutdown_receiver: broadcast::Receiver<()>,
) -> Result<(), tonic::transport::Error> {
    let clip_engine = RealearnClipEngine::default();
    Server::builder()
        .layer(MainThreadLayer)
        .add_service(ClipEngineServer::new(clip_engine))
        .serve_with_shutdown(
            address,
            async move { shutdown_receiver.recv().await.unwrap() },
        )
        .await
}

#[derive(Clone)]
pub struct GrpcEvent {
    pub session_id: String,
    pub payload: Vec<QualifiedContinuousSlotState>,
}
