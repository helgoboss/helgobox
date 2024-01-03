use crate::infrastructure::plugin::BackboneShell;
use crate::infrastructure::proto::helgobox_service_server::HelgoboxServiceServer;
use crate::infrastructure::proto::{ClipEngineHub, GrpcClipEngineService, MatrixProvider};
use playtime_clip_engine::base::Matrix;

pub type PlaytimeService = HelgoboxServiceServer<GrpcClipEngineService<AppMatrixProvider>>;

pub fn create_playtime_service(hub: &ClipEngineHub) -> PlaytimeService {
    hub.create_service(AppMatrixProvider)
}

#[derive(Clone, Debug)]
pub struct AppMatrixProvider;

impl MatrixProvider for AppMatrixProvider {
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
