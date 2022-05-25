use crate::infrastructure::plugin::App;
use crate::infrastructure::server::grpc::proto::{
    clip_engine_server, GetClipPositionUpdatesReply, GetClipPositionUpdatesRequest,
};
use crate::infrastructure::server::grpc::GrpcClipPositionsUpdateEvent;
use futures::{Stream, StreamExt};
use std::pin::Pin;
use tokio_stream::wrappers::BroadcastStream;
use tonic::{Request, Response, Status};

#[derive(Debug, Default)]
pub struct MyClipEngine {}

#[tonic::async_trait]
impl clip_engine_server::ClipEngine for MyClipEngine {
    type GetClipPositionUpdatesStream =
        SyncBoxStream<'static, Result<GetClipPositionUpdatesReply, Status>>;

    async fn get_clip_position_updates(
        &self,
        request: Request<GetClipPositionUpdatesRequest>,
    ) -> Result<Response<Self::GetClipPositionUpdatesStream>, Status> {
        let receiver = App::get()
            .grpc_clip_positions_update_event_sender()
            .subscribe();
        let request_session_id = request.into_inner().session_id;
        let receiver_stream = BroadcastStream::new(receiver).filter_map(move |value| {
            let request_session_id = request_session_id.clone();
            async move {
                match value {
                    Err(e) => Some(Err(Status::unknown(e.to_string()))),
                    Ok(GrpcClipPositionsUpdateEvent {
                        session_id,
                        updates,
                    }) if &session_id == &request_session_id => {
                        Some(Ok(GetClipPositionUpdatesReply {
                            clip_position_updates: updates,
                        }))
                    }
                    _ => None,
                }
            }
        });
        Ok(Response::new(Box::pin(receiver_stream)))
    }
}

type SyncBoxStream<'a, T> = Pin<Box<dyn Stream<Item = T> + Send + Sync + 'a>>;
