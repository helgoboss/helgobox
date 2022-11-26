use crate::infrastructure::plugin::App;
use crate::infrastructure::server::grpc::WithSessionId;
use futures::{Stream, StreamExt};
use playtime_clip_engine::proto::{
    clip_engine_server, occasional_matrix_update, occasional_track_update,
    qualified_occasional_slot_update, ArrangementPlayState, GetContinuousColumnUpdatesReply,
    GetContinuousColumnUpdatesRequest, GetContinuousMatrixUpdatesReply,
    GetContinuousMatrixUpdatesRequest, GetContinuousSlotUpdatesReply,
    GetContinuousSlotUpdatesRequest, GetOccasionalMatrixUpdatesReply,
    GetOccasionalMatrixUpdatesRequest, GetOccasionalSlotUpdatesReply,
    GetOccasionalSlotUpdatesRequest, GetOccasionalTrackUpdatesReply,
    GetOccasionalTrackUpdatesRequest, OccasionalMatrixUpdate, OccasionalTrackUpdate,
    QualifiedOccasionalSlotUpdate, QualifiedOccasionalTrackUpdate, SlotCoordinates, SlotPlayState,
    TrackColor, TrackInput, TrackInputMonitoring, TriggerSlotAction, TriggerSlotReply,
    TriggerSlotRequest,
};
use playtime_clip_engine::rt::ColumnPlayClipOptions;
use reaper_high::{Guid, OrCurrentProject, Track};
use std::collections::HashMap;
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
        let initial_slot_updates = App::get()
            .with_clip_matrix(&request.get_ref().clip_matrix_id, |matrix| {
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
            .map_err(Status::not_found)?;
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

    type GetOccasionalMatrixUpdatesStream =
        SyncBoxStream<'static, Result<GetOccasionalMatrixUpdatesReply, Status>>;

    async fn get_occasional_matrix_updates(
        &self,
        request: Request<GetOccasionalMatrixUpdatesRequest>,
    ) -> Result<Response<Self::GetOccasionalMatrixUpdatesStream>, Status> {
        use occasional_matrix_update::Update;
        let initial_matrix_updates = App::get()
            .with_clip_matrix(
                &request.get_ref().clip_matrix_id,
                |matrix| -> Result<_, &'static str> {
                    let project = matrix.permanent_project().or_current_project();
                    let master_track = project.master_track()?;
                    let updates = [
                        Update::Volume(master_track.volume().db().get()),
                        Update::Pan(master_track.pan().reaper_value().get()),
                        Update::Tempo(project.tempo().bpm().get()),
                        Update::ArrangementPlayState(
                            ArrangementPlayState::from_engine(project.play_state()).into(),
                        ),
                    ];
                    let updates: Vec<_> = updates
                        .into_iter()
                        .map(|u| OccasionalMatrixUpdate { update: Some(u) })
                        .collect();
                    Ok(updates)
                },
            )
            .map_err(Status::not_found)?
            .map_err(Status::unknown)?;
        let initial_reply = GetOccasionalMatrixUpdatesReply {
            matrix_updates: initial_matrix_updates,
        };
        let receiver = App::get().occasional_matrix_update_sender().subscribe();
        stream_by_session_id(
            request.into_inner().clip_matrix_id,
            receiver,
            |matrix_updates| GetOccasionalMatrixUpdatesReply { matrix_updates },
            Some(initial_reply).into_iter(),
        )
    }

    type GetOccasionalTrackUpdatesStream =
        SyncBoxStream<'static, Result<GetOccasionalTrackUpdatesReply, Status>>;

    async fn get_occasional_track_updates(
        &self,
        request: Request<GetOccasionalTrackUpdatesRequest>,
    ) -> Result<Response<Self::GetOccasionalTrackUpdatesStream>, Status> {
        let initial_track_updates = App::get()
            .with_clip_matrix(&request.get_ref().clip_matrix_id, |matrix| {
                let track_by_guid: HashMap<Guid, Track> = matrix
                    .all_columns()
                    .flat_map(|column| {
                        column
                            .playback_track()
                            .into_iter()
                            .cloned()
                            .chain(column.effective_recording_track().into_iter())
                    })
                    .map(|track| (*track.guid(), track))
                    .collect();
                track_by_guid
                    .into_iter()
                    .map(|(guid, track)| {
                        use occasional_track_update::Update;
                        QualifiedOccasionalTrackUpdate {
                            track_id: guid.to_string_without_braces(),
                            track_updates: [
                                Update::Name(track.name().unwrap_or_default().into_string()),
                                Update::Color(TrackColor::from_engine(track.custom_color())),
                                Update::Input(TrackInput::from_engine(track.recording_input())),
                                Update::Armed(track.is_armed(false)),
                                Update::InputMonitoring(
                                    TrackInputMonitoring::from_engine(
                                        track.input_monitoring_mode(),
                                    )
                                    .into(),
                                ),
                                Update::Mute(track.is_muted()),
                                Update::Solo(track.is_solo()),
                                Update::Selected(track.is_selected()),
                                Update::Volume(track.volume().db().get()),
                                Update::Pan(track.pan().reaper_value().get()),
                            ]
                            .into_iter()
                            .map(|update| OccasionalTrackUpdate {
                                update: Some(update),
                            })
                            .collect(),
                        }
                    })
                    .collect()
            })
            .map_err(Status::not_found)?;
        let initial_reply = GetOccasionalTrackUpdatesReply {
            track_updates: initial_track_updates,
        };
        let receiver = App::get().occasional_track_update_sender().subscribe();
        stream_by_session_id(
            request.into_inner().clip_matrix_id,
            receiver,
            |track_updates| GetOccasionalTrackUpdatesReply { track_updates },
            Some(initial_reply).into_iter(),
        )
    }

    async fn trigger_slot(
        &self,
        request: Request<TriggerSlotRequest>,
    ) -> Result<Response<TriggerSlotReply>, Status> {
        let req = request.get_ref();
        let slot_coordinates = req
            .slot_coordinates
            .as_ref()
            .ok_or(Status::invalid_argument("need slot coordinates"))?
            .to_engine();
        let action = TriggerSlotAction::from_i32(req.action)
            .ok_or(Status::invalid_argument("unknown trigger slot action"))?;
        let result = App::get().with_clip_matrix_mut(&req.clip_matrix_id, |matrix| match action {
            TriggerSlotAction::Play => {
                matrix.play_clip(slot_coordinates, ColumnPlayClipOptions::default())
            }
            TriggerSlotAction::Stop => matrix.stop_clip(slot_coordinates, None),
            TriggerSlotAction::Record => matrix.record_clip(slot_coordinates),
        });
        result
            .map_err(Status::not_found)?
            .map_err(Status::unknown)?;
        Ok(Response::new(TriggerSlotReply {}))
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
            Ok(WithSessionId { session_id, value }) if session_id == requested_clip_matrix_id => {
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
