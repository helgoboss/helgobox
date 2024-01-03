use crate::infrastructure::plugin::BackboneShell;
use crate::infrastructure::proto::helgobox_service_server::HelgoboxServiceServer;
use crate::infrastructure::proto::{HelgoboxServiceImpl, MatrixProvider, ProtoHub};
use playtime_clip_engine::base::Matrix;

pub type DefaultHelgoboxServiceServer =
    HelgoboxServiceServer<HelgoboxServiceImpl<BackboneMatrixProvider>>;

pub fn create_server(hub: &ProtoHub) -> DefaultHelgoboxServiceServer {
    hub.create_service(BackboneMatrixProvider)
}

#[derive(Clone, Debug)]
pub struct BackboneMatrixProvider;

impl MatrixProvider for BackboneMatrixProvider {
    fn with_matrix<R>(
        &self,
        clip_matrix_id: &str,
        f: impl FnOnce(&Matrix) -> R,
    ) -> anyhow::Result<R> {
        BackboneShell::get().with_clip_matrix(clip_matrix_id, f)
    }

    fn with_matrix_mut<R>(
        &self,
        clip_matrix_id: &str,
        f: impl FnOnce(&mut Matrix) -> R,
    ) -> anyhow::Result<R> {
        BackboneShell::get().with_clip_matrix_mut(clip_matrix_id, f)
    }

    fn create_matrix(&self, clip_matrix_id: &str) -> anyhow::Result<()> {
        BackboneShell::get().create_clip_matrix(clip_matrix_id)
    }
}
