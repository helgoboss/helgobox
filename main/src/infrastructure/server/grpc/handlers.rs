use crate::infrastructure::plugin::App;
use crate::infrastructure::server::grpc::GrpcEvent;
use futures::{Stream, StreamExt};
use playtime_clip_engine::proto::{
    clip_engine_server, GetContinuousMatrixStateUpdatesReply,
    GetContinuousMatrixStateUpdatesRequest, GetContinuousSlotStateUpdatesReply,
    GetContinuousSlotStateUpdatesRequest, GetContinuousTrackStateUpdatesReply,
    GetContinuousTrackStateUpdatesRequest,
};
use std::pin::Pin;
use tokio_stream::wrappers::BroadcastStream;
use tonic::{Request, Response, Status};

#[derive(Debug, Default)]
pub struct RealearnClipEngine {}

#[tonic::async_trait]
impl clip_engine_server::ClipEngine for RealearnClipEngine {
    type GetContinuousMatrixStateUpdatesStream =
        SyncBoxStream<'static, Result<GetContinuousMatrixStateUpdatesReply, Status>>;
    type GetContinuousTrackStateUpdatesStream =
        SyncBoxStream<'static, Result<GetContinuousTrackStateUpdatesReply, Status>>;
    type GetContinuousSlotStateUpdatesStream =
        SyncBoxStream<'static, Result<GetContinuousSlotStateUpdatesReply, Status>>;

    async fn get_continuous_slot_state_updates(
        &self,
        request: Request<GetContinuousSlotStateUpdatesRequest>,
    ) -> Result<Response<Self::GetContinuousSlotStateUpdatesStream>, Status> {
        let receiver = App::get().grpc_sender().subscribe();
        let requested_clip_matrix_id = request.into_inner().clip_matrix_id;
        let receiver_stream = BroadcastStream::new(receiver).filter_map(move |value| {
            // TODO-high This shouldn't be necessary!
            let requested_clip_matrix_id = requested_clip_matrix_id.clone();
            async move {
                match value {
                    Err(e) => Some(Err(Status::unknown(e.to_string()))),
                    Ok(GrpcEvent {
                        session_id,
                        payload,
                    }) if &session_id == &requested_clip_matrix_id => {
                        Some(Ok(GetContinuousSlotStateUpdatesReply {
                            slot_states: payload,
                        }))
                    }
                    _ => None,
                }
            }
        });
        Ok(Response::new(Box::pin(receiver_stream)))
    }

    async fn get_continuous_matrix_state_updates(
        &self,
        request: Request<GetContinuousMatrixStateUpdatesRequest>,
    ) -> Result<Response<Self::GetContinuousMatrixStateUpdatesStream>, Status> {
        todo!()
    }

    async fn get_continuous_track_state_updates(
        &self,
        request: Request<GetContinuousTrackStateUpdatesRequest>,
    ) -> Result<Response<Self::GetContinuousTrackStateUpdatesStream>, Status> {
        todo!()
    }
}

type SyncBoxStream<'a, T> = Pin<Box<dyn Stream<Item = T> + Send + Sync + 'a>>;
