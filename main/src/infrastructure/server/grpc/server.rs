use crate::infrastructure::server::layers::MainThreadLayer;
use playtime_clip_engine::proto::clip_engine_server::{ClipEngine, ClipEngineServer};
use std::net::SocketAddr;
use tonic::transport::Server;

pub async fn start_grpc_server(
    address: SocketAddr,
    service: impl ClipEngine,
) -> Result<(), tonic::transport::Error> {
    Server::builder()
        .layer(MainThreadLayer)
        .add_service(ClipEngineServer::new(service))
        .serve(address)
        .await
}
