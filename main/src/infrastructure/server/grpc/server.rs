use crate::infrastructure::server::services::RealearnServices;
use std::net::SocketAddr;

pub async fn start_grpc_server(
    address: SocketAddr,
    services: RealearnServices,
) -> Result<(), tonic::transport::Error> {
    #[cfg(feature = "playtime")]
    {
        tonic::transport::Server::builder()
            .layer(crate::infrastructure::server::layers::MainThreadLayer)
            .add_service(services.playtime_service)
            .serve(address)
            .await
    }
    #[cfg(not(feature = "playtime"))]
    {
        let _ = (address, services);
        Ok(())
    }
}
