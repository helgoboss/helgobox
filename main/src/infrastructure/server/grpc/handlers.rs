use crate::domain::RealearnClipMatrix;
use crate::infrastructure::plugin::App;
use crate::infrastructure::server::grpc::WithSessionId;
use base::future_util;
use futures::{FutureExt, Stream, StreamExt};
use playtime_clip_engine::base::{ClipAddress, ClipSlotAddress};
use playtime_clip_engine::proto;
use playtime_clip_engine::proto::{
    clip_engine_server, occasional_matrix_update, occasional_track_update,
    qualified_occasional_slot_update, Empty, FullClipAddress, FullColumnAddress, FullRowAddress,
    FullSlotAddress, GetContinuousColumnUpdatesReply, GetContinuousColumnUpdatesRequest,
    GetContinuousMatrixUpdatesReply, GetContinuousMatrixUpdatesRequest,
    GetContinuousSlotUpdatesReply, GetContinuousSlotUpdatesRequest, GetOccasionalClipUpdatesReply,
    GetOccasionalClipUpdatesRequest, GetOccasionalMatrixUpdatesReply,
    GetOccasionalMatrixUpdatesRequest, GetOccasionalSlotUpdatesReply,
    GetOccasionalSlotUpdatesRequest, GetOccasionalTrackUpdatesReply,
    GetOccasionalTrackUpdatesRequest, OccasionalMatrixUpdate, OccasionalTrackUpdate,
    QualifiedOccasionalSlotUpdate, QualifiedOccasionalTrackUpdate, SetClipDataRequest,
    SetClipNameRequest, SetColumnPanRequest, SetColumnVolumeRequest, SetMatrixPanRequest,
    SetMatrixTempoRequest, SetMatrixVolumeRequest, SlotAddress, TriggerColumnAction,
    TriggerColumnRequest, TriggerMatrixAction, TriggerMatrixRequest, TriggerRowAction,
    TriggerRowRequest, TriggerSlotAction, TriggerSlotRequest,
};
use playtime_clip_engine::rt::ColumnPlayClipOptions;
use reaper_high::{GroupingBehavior, Guid, OrCurrentProject, Pan, Reaper, Tempo, Track, Volume};
use reaper_medium::{Bpm, CommandId, Db, GangBehavior, ReaperPanValue, SoloMode, UndoBehavior};
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
                        let play_state = slot.value().play_state().unwrap_or_default();
                        let address = SlotAddress {
                            column_index: slot.column_index() as u32,
                            row_index: slot.value().index() as u32,
                        };
                        QualifiedOccasionalSlotUpdate {
                            slot_address: Some(address),
                            update: Some(qualified_occasional_slot_update::Update::play_state(
                                play_state,
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
        let receiver = App::get().occasional_clip_update_sender().subscribe();
        stream_by_session_id(
            request.into_inner().matrix_id,
            receiver,
            |clip_updates| GetOccasionalClipUpdatesReply { clip_updates },
            iter::empty(),
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
                    let updates = [
                        Update::volume(master_track.volume().db()),
                        Update::pan(master_track.pan().reaper_value()),
                        Update::tempo(project.tempo().bpm()),
                        Update::arrangement_play_state(project.play_state()),
                        // TODO MIDI input devices
                        // TODO audio input channels
                        Update::complete_persistent_data(matrix),
                        Update::history_state(matrix),
                        // TODO click enabled
                        Update::time_signature(project),
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
                                Update::name(&track),
                                Update::color(&track),
                                Update::input(track.recording_input()),
                                Update::armed(track.is_armed(false)),
                                Update::input_monitoring(track.input_monitoring_mode()),
                                Update::mute(track.is_muted()),
                                Update::solo(track.is_solo()),
                                Update::selected(track.is_selected()),
                                Update::volume(track.volume().db()),
                                Update::pan(track.pan().reaper_value()),
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
            .ok_or_else(|| Status::invalid_argument("unknown trigger slot action"))?;
        handle_slot_command(&req.slot_address, |matrix, slot_address| match action {
            TriggerSlotAction::Play => {
                matrix.play_slot(slot_address, ColumnPlayClipOptions::default())
            }
            TriggerSlotAction::Stop => matrix.stop_slot(slot_address, None),
            TriggerSlotAction::Record => matrix.record_slot(slot_address),
            TriggerSlotAction::StartEditing => matrix.start_editing_slot(slot_address),
        })
    }

    async fn set_clip_name(
        &self,
        request: Request<SetClipNameRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        handle_clip_command(&req.clip_address, |matrix, clip_address| {
            matrix.set_clip_name(clip_address, req.name)
        })
    }

    async fn set_clip_data(
        &self,
        request: Request<SetClipDataRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let clip =
            serde_json::from_str(&req.data).map_err(|e| Status::invalid_argument(e.to_string()))?;
        handle_clip_command(&req.clip_address, |matrix, clip_address| {
            matrix.set_clip_data(clip_address, clip)
        })
    }

    async fn trigger_matrix(
        &self,
        request: Request<TriggerMatrixRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let action: TriggerMatrixAction = TriggerMatrixAction::from_i32(req.action)
            .ok_or_else(|| Status::invalid_argument("unknown trigger matrix action"))?;
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
            .ok_or_else(|| Status::invalid_argument("unknown trigger column action"))?;
        handle_column_command(&req.column_address, |matrix, column_index| match action {
            TriggerColumnAction::Stop => matrix.stop_column(column_index),
            TriggerColumnAction::ToggleMute => {
                let column = matrix.get_column(column_index)?;
                let track = column.playback_track()?;
                track.set_mute(
                    !track.is_muted(),
                    GangBehavior::DenyGang,
                    GroupingBehavior::PreventGrouping,
                );
                Ok(())
            }
            TriggerColumnAction::ToggleSolo => {
                let column = matrix.get_column(column_index)?;
                let track = column.playback_track()?;
                let new_solo_mode = if track.is_solo() {
                    SoloMode::Off
                } else {
                    SoloMode::SoloInPlace
                };
                track.set_solo_mode(new_solo_mode);
                Ok(())
            }
            TriggerColumnAction::ToggleArm => {
                let column = matrix.get_column(column_index)?;
                let track = column.playback_track()?;
                track.set_armed(
                    !track.is_armed(false),
                    GangBehavior::DenyGang,
                    GroupingBehavior::PreventGrouping,
                );
                Ok(())
            }
        })
    }

    async fn trigger_row(
        &self,
        request: Request<TriggerRowRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let action = TriggerRowAction::from_i32(req.action)
            .ok_or_else(|| Status::invalid_argument("unknown trigger row action"))?;
        handle_row_command(&req.row_address, |matrix, row_index| match action {
            TriggerRowAction::Play => {
                matrix.play_scene(row_index);
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
        handle_column_command(&req.column_address, |matrix, column_index| {
            let column = matrix.get_column(column_index)?;
            let track = column.playback_track()?;
            track.set_volume(
                Volume::from_db(db),
                GangBehavior::DenyGang,
                GroupingBehavior::PreventGrouping,
            );
            Ok(())
        })
    }

    async fn set_column_pan(
        &self,
        request: Request<SetColumnPanRequest>,
    ) -> Result<Response<Empty>, Status> {
        let req = request.into_inner();
        let pan = ReaperPanValue::new(req.pan.clamp(-1.0, 1.0));
        handle_column_command(&req.column_address, |matrix, column_index| {
            let column = matrix.get_column(column_index)?;
            let track = column.playback_track()?;
            track.set_pan(
                Pan::from_reaper_value(pan),
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
    // Stream that waits 1 millisecond and emits nothing
    // This is done to (hopefully) prevent the following client-side Dart error, which otherwise
    // would occur sporadically when attempting to connect:
    // [ERROR:flutter/runtime/dart_vm_initializer.cc(41)] Unhandled Exception: gRPC Error (code: 2, codeName: UNKNOWN, message: HTTP/2 error: Connection error: Connection is being forcefully terminated. (errorCode: 1), details: null, rawResponse: null, trailers: {})
    let wait_one_milli = future_util::millis(1)
        .map(|_| Err(Status::unknown("skipped")))
        .into_stream()
        .skip(1);
    // Stream for sending the initial state
    let initial_stream = futures::stream::iter(initial.map(|r| Ok(r)));
    // Stream for sending occasional updates
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
        wait_one_milli.chain(initial_stream).chain(receiver_stream),
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
    full_column_id: &Option<FullColumnAddress>,
    handler: impl FnOnce(&mut RealearnClipMatrix, usize) -> Result<(), &'static str>,
) -> Result<Response<Empty>, Status> {
    let full_column_id = full_column_id
        .as_ref()
        .ok_or_else(|| Status::invalid_argument("need full column address"))?;
    let column_index = full_column_id.column_index as usize;
    handle_matrix_command(&full_column_id.matrix_id, |matrix| {
        handler(matrix, column_index)
    })
}

fn handle_row_command(
    full_row_id: &Option<FullRowAddress>,
    handler: impl FnOnce(&mut RealearnClipMatrix, usize) -> Result<(), &'static str>,
) -> Result<Response<Empty>, Status> {
    let full_row_id = full_row_id
        .as_ref()
        .ok_or_else(|| Status::invalid_argument("need full row address"))?;
    let row_index = full_row_id.row_index as usize;
    handle_matrix_command(&full_row_id.matrix_id, |matrix| handler(matrix, row_index))
}

fn handle_slot_command(
    full_slot_address: &Option<FullSlotAddress>,
    handler: impl FnOnce(&mut RealearnClipMatrix, ClipSlotAddress) -> Result<(), &'static str>,
) -> Result<Response<Empty>, Status> {
    let full_slot_address = full_slot_address
        .as_ref()
        .ok_or_else(|| Status::invalid_argument("need full slot address"))?;
    let slot_addr = convert_slot_address_to_engine(&full_slot_address.slot_address)?;
    handle_matrix_command(&full_slot_address.matrix_id, |matrix| {
        handler(matrix, slot_addr)
    })
}

fn handle_clip_command(
    full_clip_address: &Option<FullClipAddress>,
    handler: impl FnOnce(&mut RealearnClipMatrix, ClipAddress) -> Result<(), &'static str>,
) -> Result<Response<Empty>, Status> {
    let full_clip_address = full_clip_address
        .as_ref()
        .ok_or_else(|| Status::invalid_argument("need full clip address"))?;
    let clip_addr = convert_clip_address_to_engine(&full_clip_address.clip_address)?;
    handle_matrix_command(&full_clip_address.matrix_id, |matrix| {
        handler(matrix, clip_addr)
    })
}

fn convert_slot_address_to_engine(addr: &Option<SlotAddress>) -> Result<ClipSlotAddress, Status> {
    let addr = addr
        .as_ref()
        .ok_or_else(|| Status::invalid_argument("need slot address"))?
        .to_engine();
    Ok(addr)
}

fn convert_clip_address_to_engine(
    addr: &Option<proto::ClipAddress>,
) -> Result<ClipAddress, Status> {
    let addr = addr
        .as_ref()
        .ok_or_else(|| Status::invalid_argument("need clip address"))?
        .to_engine()
        .map_err(Status::invalid_argument)?;
    Ok(addr)
}
