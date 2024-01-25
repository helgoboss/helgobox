use crate::infrastructure::proto::helgobox_service_server::HelgoboxServiceServer;
use crate::infrastructure::proto::{HelgoboxServiceImpl, ProtoHub};

pub type DefaultHelgoboxServiceServer = HelgoboxServiceServer<HelgoboxServiceImpl>;

pub fn create_server(hub: &ProtoHub) -> DefaultHelgoboxServiceServer {
    hub.create_service()
}
