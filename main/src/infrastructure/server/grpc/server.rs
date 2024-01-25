use crate::infrastructure::server::services::Services;
use std::net::SocketAddr;

pub async fn start_grpc_server(
    address: SocketAddr,
    services: Services,
) -> Result<(), tonic::transport::Error> {
    tonic::transport::Server::builder()
        .layer(crate::infrastructure::server::layers::MainThreadLayer)
        .add_service(services.helgobox_service)
        .serve(address)
        .await
}
