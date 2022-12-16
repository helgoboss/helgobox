use crate::domain::RealearnClipMatrix;
use crate::infrastructure::plugin::App;
use crate::infrastructure::server::grpc::WithSessionId;
use crate::infrastructure::ui::get_proto_time_signature;
use futures::{Stream, StreamExt};
use playtime_clip_engine::base::{ClipId, ClipSlotPos};
use playtime_clip_engine::proto::{
    clip_engine_server, occasional_matrix_update, occasional_track_update,
    qualified_occasional_slot_update, ArrangementPlayState, Empty, FullClipId, FullColumnId,
    FullRowId, FullSlotId, GetContinuousColumnUpdatesReply, GetContinuousColumnUpdatesRequest,
    GetContinuousMatrixUpdatesReply, GetContinuousMatrixUpdatesRequest,
    GetContinuousSlotUpdatesReply, GetContinuousSlotUpdatesRequest, GetOccasionalClipUpdatesReply,
    GetOccasionalClipUpdatesRequest, GetOccasionalMatrixUpdatesReply,
    GetOccasionalMatrixUpdatesRequest, GetOccasionalSlotUpdatesReply,
    GetOccasionalSlotUpdatesRequest, GetOccasionalTrackUpdatesReply,
    GetOccasionalTrackUpdatesRequest, OccasionalMatrixUpdate, OccasionalTrackUpdate,
    QualifiedOccasionalSlotUpdate, QualifiedOccasionalTrackUpdate, SetClipNameRequest,
    SetColumnVolumeRequest, SetMatrixPanRequest, SetMatrixTempoRequest, SetMatrixVolumeRequest,
    SlotPlayState, SlotPos, TrackColor, TrackInput, TrackInputMonitoring, TriggerColumnAction,
    TriggerColumnRequest, TriggerMatrixAction, TriggerMatrixRequest, TriggerRowAction,
    TriggerRowRequest, TriggerSlotAction, TriggerSlotRequest,
};
use playtime_clip_engine::rt::ColumnPlayClipOptions;
use reaper_high::{GroupingBehavior, Guid, OrCurrentProject, Pan, Reaper, Tempo, Track, Volume};
use reaper_medium::{Bpm, CommandId, Db, GangBehavior, ReaperPanValue, UndoBehavior};
use std::collections::HashMap;
use std::pin::Pin;
use std::str::FromStr;
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
            request.into_inner().matrix_id,
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
            request.into_inner().matrix_id,
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
            .with_clip_matrix(&request.get_ref().matrix_id, |matrix| {
                matrix
                    .all_slots()
                    .map(|slot| {
                        let play_state = slot.value().clip_play_state().unwrap_or_default();
                        let proto_play_state = SlotPlayState::from_engine(play_state.get());
                        let coordinates = SlotPos {
                            column_index: slot.column_index() as u32,
                            row_index: slot.value().index() as u32,
                        };
                        QualifiedOccasionalSlotUpdate {
                            slot_pos: Some(coordinates),
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
            request.into_inner().matrix_id,
            receiver,
            |slot_updates| GetOccasionalSlotUpdatesReply { slot_updates },
            Some(initial_reply).into_iter(),
        )
    }

    type GetOccasionalClipUpdatesStream =
        SyncBoxStream<'static, Result<GetOccasionalClipUpdatesReply, Status>>;

    async fn get_occasional_clip_updates(
        &self,
        request: Request<GetOccasionalClipUpdatesRequest>,
    ) -> Result<Response<Self::GetOccasionalClipUpdatesStream>, Status> {
        todo!()
    }

    type GetContinuousSlotUpdatesStream =
        SyncBoxStream<'static, Result<GetContinuousSlotUpdatesReply, Status>>;

    async fn get_continuous_slot_updates(
        &self,
        request: Request<GetContinuousSlotUpdatesRequest>,
    ) -> Result<Response<Self::GetContinuousSlotUpdatesStream>, Status> {
        let receiver = App::get().continuous_slot_update_sender().subscribe();
        stream_by_session_id(
            request.into_inner().matrix_id,
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
                &request.get_ref().matrix_id,
                |matrix| -> Result<_, &'static str> {
                    let project = matrix.permanent_project().or_current_project();
                    let master_track = project.master_track()?;
                    let matrix_json = serde_json::to_string(&matrix.save())
                        .map_err(|_| "couldn't serialize clip matrix")?;
                    let updates = [
                        Update::PersistentState(matrix_json),
                        Update::Volume(master_track.volume().db().get()),
                        Update::Pan(master_track.pan().reaper_value().get()),
                        Update::Tempo(project.tempo().bpm().get()),
                        Update::TimeSignature(get_proto_time_signature(project)),
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
            request.into_inner().matrix_id,
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
            .with_clip_matrix(&request.get_ref().matrix_id, |matrix| {
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
            request.into_inner().matrix_id,
            receiver,
            |track_updates| GetOccasionalTrackUpdatesReply { track_updates },
            Some(initial_reply).into_iter(),
        )
    }

    async fn trigger_slot(
        &self,
        request: Request<TriggerSlotRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let action = TriggerSlotAction::from_i32(req.action)
            .ok_or(Status::invalid_argument("unknown trigger slot action"))?;
        handle_slot_command(&req.slot_id, |matrix, slot_pos| match action {
            TriggerSlotAction::Play => matrix.play_clip(slot_pos, ColumnPlayClipOptions::default()),
            TriggerSlotAction::Stop => matrix.stop_clip(slot_pos, None),
            TriggerSlotAction::Record => matrix.record_clip(slot_pos),
        })
    }

    async fn set_clip_name(
        &self,
        request: Request<SetClipNameRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        handle_clip_command(&req.clip_id, |matrix, clip_id| {
            matrix.set_clip_name(clip_id, req.name)
        })
    }

    async fn trigger_matrix(
        &self,
        request: Request<TriggerMatrixRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let action: TriggerMatrixAction = TriggerMatrixAction::from_i32(req.action)
            .ok_or(Status::invalid_argument("unknown trigger matrix action"))?;
        handle_matrix_command(&req.matrix_id, |matrix| {
            let project = matrix.permanent_project().or_current_project();
            match action {
                TriggerMatrixAction::ArrangementTogglePlayStop => {
                    if project.is_playing() {
                        project.stop();
                    } else {
                        project.play();
                    }
                    Ok(())
                }
                TriggerMatrixAction::StopAllClips => {
                    matrix.stop();
                    Ok(())
                }
                TriggerMatrixAction::ArrangementPlay => {
                    project.play();
                    Ok(())
                }
                TriggerMatrixAction::ArrangementStop => {
                    project.stop();
                    Ok(())
                }
                TriggerMatrixAction::ArrangementPause => {
                    project.pause();
                    Ok(())
                }
                TriggerMatrixAction::ArrangementStartRecording => {
                    Reaper::get().enable_record_in_current_project();
                    Ok(())
                }
                TriggerMatrixAction::ArrangementStopRecording => {
                    Reaper::get().disable_record_in_current_project();
                    Ok(())
                }
                TriggerMatrixAction::Undo => matrix.undo(),
                TriggerMatrixAction::Redo => matrix.redo(),
                TriggerMatrixAction::ToggleClick => Reaper::get()
                    .main_section()
                    .action_by_command_id(CommandId::new(40364))
                    .invoke_as_trigger(Some(project))
                    .map_err(|e| e.message()),
            }
        })
    }

    async fn trigger_column(
        &self,
        request: Request<TriggerColumnRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let action = TriggerColumnAction::from_i32(req.action)
            .ok_or(Status::invalid_argument("unknown trigger column action"))?;
        handle_column_command(&req.column_id, |matrix, column_index| match action {
            TriggerColumnAction::Stop => matrix.stop_column(column_index),
        })
    }

    async fn trigger_row(
        &self,
        request: Request<TriggerRowRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let action = TriggerRowAction::from_i32(req.action)
            .ok_or(Status::invalid_argument("unknown trigger row action"))?;
        handle_row_command(&req.row_id, |matrix, row_index| match action {
            TriggerRowAction::Play => {
                matrix.play_row(row_index);
                Ok(())
            }
        })
    }

    async fn set_matrix_tempo(
        &self,
        request: Request<SetMatrixTempoRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let bpm = Bpm::try_from(req.bpm).map_err(|e| Status::invalid_argument(e.as_ref()))?;
        handle_matrix_command(&req.matrix_id, |matrix| {
            let project = matrix.permanent_project().or_current_project();
            project
                .set_tempo(Tempo::from_bpm(bpm), UndoBehavior::OmitUndoPoint)
                .map_err(|e| e.message())
        })
    }

    async fn set_matrix_volume(
        &self,
        request: Request<SetMatrixVolumeRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let db = Db::try_from(req.db).map_err(|e| Status::invalid_argument(e.as_ref()))?;
        handle_matrix_command(&req.matrix_id, |matrix| {
            let project = matrix.permanent_project().or_current_project();
            project.master_track()?.set_volume(
                Volume::from_db(db),
                GangBehavior::DenyGang,
                GroupingBehavior::PreventGrouping,
            );
            Ok(())
        })
    }

    async fn set_matrix_pan(
        &self,
        request: Request<SetMatrixPanRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let pan =
            ReaperPanValue::try_from(req.pan).map_err(|e| Status::invalid_argument(e.as_ref()))?;
        handle_matrix_command(&req.matrix_id, |matrix| {
            let project = matrix.permanent_project().or_current_project();
            project.master_track()?.set_pan(
                Pan::from_reaper_value(pan),
                GangBehavior::DenyGang,
                GroupingBehavior::PreventGrouping,
            );
            Ok(())
        })
    }

    async fn set_column_volume(
        &self,
        request: Request<SetColumnVolumeRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let db = Db::try_from(req.db).map_err(|e| Status::invalid_argument(e.as_ref()))?;
        handle_column_command(&req.column_id, |matrix, column_index| {
            let column = matrix.column(column_index)?;
            let track = column.playback_track()?;
            track.set_volume(
                Volume::from_db(db),
                GangBehavior::DenyGang,
                GroupingBehavior::PreventGrouping,
            );
            Ok(())
        })
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

fn handle_matrix_command(
    matrix_id: &str,
    handler: impl FnOnce(&mut RealearnClipMatrix) -> Result<(), &'static str>,
) -> Result<Response<Empty>, Status> {
    App::get()
        .with_clip_matrix_mut(matrix_id, handler)
        .map_err(Status::unknown)?
        .map_err(Status::not_found)?;
    Ok(Response::new(Empty {}))
}

fn handle_column_command(
    full_column_id: &Option<FullColumnId>,
    handler: impl FnOnce(&mut RealearnClipMatrix, usize) -> Result<(), &'static str>,
) -> Result<Response<Empty>, Status> {
    let full_column_id = full_column_id
        .as_ref()
        .ok_or_else(|| Status::invalid_argument("need full column ID"))?;
    let column_index = full_column_id.column_index as usize;
    handle_matrix_command(&full_column_id.matrix_id, |matrix| {
        handler(matrix, column_index)
    })
}

fn handle_row_command(
    full_row_id: &Option<FullRowId>,
    handler: impl FnOnce(&mut RealearnClipMatrix, usize) -> Result<(), &'static str>,
) -> Result<Response<Empty>, Status> {
    let full_row_id = full_row_id
        .as_ref()
        .ok_or_else(|| Status::invalid_argument("need full row ID"))?;
    let row_index = full_row_id.row_index as usize;
    handle_matrix_command(&full_row_id.matrix_id, |matrix| handler(matrix, row_index))
}

fn handle_slot_command(
    full_slot_id: &Option<FullSlotId>,
    handler: impl FnOnce(&mut RealearnClipMatrix, ClipSlotPos) -> Result<(), &'static str>,
) -> Result<Response<Empty>, Status> {
    let full_slot_id = full_slot_id
        .as_ref()
        .ok_or_else(|| Status::invalid_argument("need full slot ID"))?;
    let slot_pos = convert_slot_pos_to_engine(&full_slot_id.slot_pos)?;
    handle_matrix_command(&full_slot_id.matrix_id, |matrix| handler(matrix, slot_pos))
}

fn handle_clip_command(
    full_clip_id: &Option<FullClipId>,
    handler: impl FnOnce(&mut RealearnClipMatrix, ClipId) -> Result<(), &'static str>,
) -> Result<Response<Empty>, Status> {
    let full_clip_id = full_clip_id
        .as_ref()
        .ok_or_else(|| Status::invalid_argument("need full clip ID"))?;
    let clip_id = ClipId::from_str(&full_clip_id.clip_id).map_err(Status::invalid_argument)?;
    handle_matrix_command(&full_clip_id.matrix_id, |matrix| handler(matrix, clip_id))
}

fn convert_slot_pos_to_engine(pos: &Option<SlotPos>) -> Result<ClipSlotPos, Status> {
    let pos = pos
        .as_ref()
        .ok_or_else(|| Status::invalid_argument("need slot pos"))?
        .to_engine();
    Ok(pos)
}
