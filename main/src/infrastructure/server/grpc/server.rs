use crate::infrastructure::server::grpc::handlers::MyGreeter;
use crate::infrastructure::server::grpc::proto::greeter_server::GreeterServer;
use std::net::SocketAddr;
use tokio::sync::broadcast;
use tonic::transport::Server;

pub async fn start_grpc_server(
    address: SocketAddr,
    mut shutdown_receiver: broadcast::Receiver<()>,
) -> Result<(), tonic::transport::Error> {
    let greeter = MyGreeter::default();
    Server::builder()
        .add_service(GreeterServer::new(greeter))
        .serve_with_shutdown(
            address,
            async move { shutdown_receiver.recv().await.unwrap() },
        )
        .await
}
