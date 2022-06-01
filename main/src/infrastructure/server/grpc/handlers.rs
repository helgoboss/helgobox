use crate::infrastructure::plugin::App;
use crate::infrastructure::server::grpc::WithSessionId;
use futures::{Stream, StreamExt};
use playtime_clip_engine::proto::{
    clip_engine_server, GetContinuousColumnUpdatesReply, GetContinuousColumnUpdatesRequest,
    GetContinuousMatrixUpdatesReply, GetContinuousMatrixUpdatesRequest,
    GetContinuousSlotUpdatesReply, GetContinuousSlotUpdatesRequest, GetOccasionalSlotUpdatesReply,
    GetOccasionalSlotUpdatesRequest,
};
use std::future;
use std::pin::Pin;
use tokio_stream::wrappers::BroadcastStream;
use tonic::{Request, Response, Status};

#[derive(Debug, Default)]
pub struct RealearnClipEngine {}

#[tonic::async_trait]
impl clip_engine_server::ClipEngine for RealearnClipEngine {
    type GetContinuousMatrixUpdatesStream =
        SyncBoxStream<'static, Result<GetContinuousMatrixUpdatesReply, Status>>;
    type GetContinuousColumnUpdatesStream =
        SyncBoxStream<'static, Result<GetContinuousColumnUpdatesReply, Status>>;
    type GetContinuousSlotUpdatesStream =
        SyncBoxStream<'static, Result<GetContinuousSlotUpdatesReply, Status>>;
    type GetOccasionalSlotUpdatesStream =
        SyncBoxStream<'static, Result<GetOccasionalSlotUpdatesReply, Status>>;

    async fn get_continuous_slot_updates(
        &self,
        request: Request<GetContinuousSlotUpdatesRequest>,
    ) -> Result<Response<Self::GetContinuousSlotUpdatesStream>, Status> {
        let receiver = App::get().continuous_slot_update_sender().subscribe();
        let receiver_stream = BroadcastStream::new(receiver).filter_map(move |value| {
            let res = match value {
                Err(e) => Some(Err(Status::unknown(e.to_string()))),
                Ok(WithSessionId { session_id, value })
                    if &session_id == &request.get_ref().clip_matrix_id =>
                {
                    Some(Ok(GetContinuousSlotUpdatesReply {
                        slot_updates: value,
                    }))
                }
                _ => None,
            };
            future::ready(res)
        });
        Ok(Response::new(Box::pin(receiver_stream)))
    }

    async fn get_continuous_matrix_updates(
        &self,
        request: Request<GetContinuousMatrixUpdatesRequest>,
    ) -> Result<Response<Self::GetContinuousMatrixUpdatesStream>, Status> {
        let receiver = App::get().continuous_matrix_update_sender().subscribe();
        stream_by_session_id(
            request.into_inner().clip_matrix_id,
            receiver,
            |matrix_update| GetContinuousMatrixUpdatesReply {
                matrix_update: Some(matrix_update),
            },
        )
    }

    async fn get_continuous_column_updates(
        &self,
        request: Request<GetContinuousColumnUpdatesRequest>,
    ) -> Result<Response<Self::GetContinuousColumnUpdatesStream>, Status> {
        let receiver = App::get().continuous_column_update_sender().subscribe();
        stream_by_session_id(
            request.into_inner().clip_matrix_id,
            receiver,
            |column_updates| GetContinuousColumnUpdatesReply { column_updates },
        )
    }

    async fn get_occasional_slot_updates(
        &self,
        request: Request<GetOccasionalSlotUpdatesRequest>,
    ) -> Result<Response<Self::GetOccasionalSlotUpdatesStream>, Status> {
        let receiver = App::get().occasional_slot_update_sender().subscribe();
        stream_by_session_id(
            request.into_inner().clip_matrix_id,
            receiver,
            |slot_updates| GetOccasionalSlotUpdatesReply { slot_updates },
        )
    }
}

type SyncBoxStream<'a, T> = Pin<Box<dyn Stream<Item = T> + Send + Sync + 'a>>;

fn stream_by_session_id<T, R, F>(
    requested_clip_matrix_id: String,
    receiver: tokio::sync::broadcast::Receiver<WithSessionId<T>>,
    create_result: F,
) -> Result<Response<SyncBoxStream<'static, Result<R, Status>>>, Status>
where
    T: Clone + Send + 'static,
    R: Send + Sync + 'static,
    F: Fn(T) -> R + Send + Sync + 'static,
{
    let receiver_stream = BroadcastStream::new(receiver).filter_map(move |value| {
        let res = match value {
            Err(e) => Some(Err(Status::unknown(e.to_string()))),
            Ok(WithSessionId { session_id, value }) if &session_id == &requested_clip_matrix_id => {
                Some(Ok(create_result(value)))
            }
            _ => None,
        };
        future::ready(res)
    });
    Ok(Response::new(Box::pin(receiver_stream)))
}
