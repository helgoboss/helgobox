use crate::domain::BackboneState;
use crate::infrastructure::plugin::App;
use crate::infrastructure::server::grpc::WithSessionId;
use futures::{Stream, StreamExt};
use playtime_clip_engine::proto::{
    clip_engine_server, qualified_occasional_slot_update, GetContinuousColumnUpdatesReply,
    GetContinuousColumnUpdatesRequest, GetContinuousMatrixUpdatesReply,
    GetContinuousMatrixUpdatesRequest, GetContinuousSlotUpdatesReply,
    GetContinuousSlotUpdatesRequest, GetOccasionalSlotUpdatesReply,
    GetOccasionalSlotUpdatesRequest, QualifiedOccasionalSlotUpdate, SlotCoordinates, SlotPlayState,
};
use std::pin::Pin;
use std::{future, iter};
use tokio_stream::wrappers::BroadcastStream;
use tonic::{Request, Response, Status};

#[derive(Debug, Default)]
pub struct RealearnClipEngine {}

#[tonic::async_trait]
impl clip_engine_server::ClipEngine for RealearnClipEngine {
    type GetContinuousMatrixUpdatesStream =
        SyncBoxStream<'static, Result<GetContinuousMatrixUpdatesReply, Status>>;

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
            iter::empty(),
        )
    }
    type GetContinuousColumnUpdatesStream =
        SyncBoxStream<'static, Result<GetContinuousColumnUpdatesReply, Status>>;
    async fn get_continuous_column_updates(
        &self,
        request: Request<GetContinuousColumnUpdatesRequest>,
    ) -> Result<Response<Self::GetContinuousColumnUpdatesStream>, Status> {
        let receiver = App::get().continuous_column_update_sender().subscribe();
        stream_by_session_id(
            request.into_inner().clip_matrix_id,
            receiver,
            |column_updates| GetContinuousColumnUpdatesReply { column_updates },
            iter::empty(),
        )
    }

    type GetOccasionalSlotUpdatesStream =
        SyncBoxStream<'static, Result<GetOccasionalSlotUpdatesReply, Status>>;

    async fn get_occasional_slot_updates(
        &self,
        request: Request<GetOccasionalSlotUpdatesRequest>,
    ) -> Result<Response<Self::GetOccasionalSlotUpdatesStream>, Status> {
        // Initial
        let session = App::get()
            .find_session_by_id(&request.get_ref().clip_matrix_id)
            .ok_or_else(session_not_found)?;
        let session = session.borrow();
        let instance_state = session.instance_state();
        let initial_slot_updates = BackboneState::get()
            .with_clip_matrix(instance_state, |matrix| {
                matrix
                    .all_slots()
                    .map(|slot| {
                        let play_state = slot.value().clip_play_state().unwrap_or_default();
                        let proto_play_state = SlotPlayState::from_engine(play_state.get());
                        let coordinates = SlotCoordinates {
                            column: slot.column_index() as u32,
                            row: slot.value().index() as u32,
                        };
                        QualifiedOccasionalSlotUpdate {
                            slot_coordinates: Some(coordinates),
                            update: Some(qualified_occasional_slot_update::Update::PlayState(
                                proto_play_state.into(),
                            )),
                        }
                    })
                    .collect()
            })
            .map_err(|e| Status::not_found(e))?;
        let initial_reply = GetOccasionalSlotUpdatesReply {
            slot_updates: initial_slot_updates,
        };
        // On change
        let receiver = App::get().occasional_slot_update_sender().subscribe();
        stream_by_session_id(
            request.into_inner().clip_matrix_id,
            receiver,
            |slot_updates| GetOccasionalSlotUpdatesReply { slot_updates },
            Some(initial_reply).into_iter(),
        )
    }

    type GetContinuousSlotUpdatesStream =
        SyncBoxStream<'static, Result<GetContinuousSlotUpdatesReply, Status>>;

    async fn get_continuous_slot_updates(
        &self,
        request: Request<GetContinuousSlotUpdatesRequest>,
    ) -> Result<Response<Self::GetContinuousSlotUpdatesStream>, Status> {
        let receiver = App::get().continuous_slot_update_sender().subscribe();
        stream_by_session_id(
            request.into_inner().clip_matrix_id,
            receiver,
            |slot_updates| GetContinuousSlotUpdatesReply { slot_updates },
            iter::empty(),
        )
    }
}

type SyncBoxStream<'a, T> = Pin<Box<dyn Stream<Item = T> + Send + Sync + 'a>>;

fn stream_by_session_id<T, R, F, I>(
    requested_clip_matrix_id: String,
    receiver: tokio::sync::broadcast::Receiver<WithSessionId<T>>,
    create_result: F,
    initial: I,
) -> Result<Response<SyncBoxStream<'static, Result<R, Status>>>, Status>
where
    T: Clone + Send + 'static,
    R: Send + Sync + 'static,
    F: Fn(T) -> R + Send + Sync + 'static,
    I: Iterator<Item = R> + Send + Sync + 'static,
{
    let initial_stream = futures::stream::iter(initial.map(|r| Ok(r)));
    let receiver_stream = BroadcastStream::new(receiver).filter_map(move |value| {
        let res = match value {
            // Error
            Err(e) => Some(Err(Status::unknown(e.to_string()))),
            // Clip matrix ID matches
            Ok(WithSessionId { session_id, value }) if &session_id == &requested_clip_matrix_id => {
                Some(Ok(create_result(value)))
            }
            // Clip matrix ID doesn't match
            _ => None,
        };
        future::ready(res)
    });
    Ok(Response::new(Box::pin(
        initial_stream.chain(receiver_stream),
    )))
}

fn session_not_found() -> Status {
    Status::not_found("session not found")
}
