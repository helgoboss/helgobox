use crate::infrastructure::plugin::BackboneShell;
use crate::infrastructure::proto;
use crate::infrastructure::proto::{
    DragClipAction, DragClipRequest, DragColumnAction, DragColumnRequest, DragRowAction,
    DragRowRequest, DragSlotAction, DragSlotRequest, Empty, FullClipAddress, FullColumnAddress,
    FullRowAddress, FullSequenceId, FullSlotAddress, FullTrackAddress, GetArrangementInfoReply,
    GetArrangementInfoRequest, GetClipDetailReply, GetClipDetailRequest, GetProjectDirReply,
    GetProjectDirRequest, ImportFilesRequest, ProveAuthenticityReply, ProveAuthenticityRequest,
    SetClipDataRequest, SetClipNameRequest, SetColumnSettingsRequest, SetColumnTrackRequest,
    SetMatrixPanRequest, SetMatrixSettingsRequest, SetMatrixTempoRequest,
    SetMatrixTimeSignatureRequest, SetMatrixVolumeRequest, SetRowDataRequest,
    SetSequenceInfoRequest, SetTrackColorRequest, SetTrackInputMonitoringRequest,
    SetTrackInputRequest, SetTrackNameRequest, SetTrackPanRequest, SetTrackVolumeRequest,
    TriggerClipAction, TriggerClipRequest, TriggerColumnAction, TriggerColumnRequest,
    TriggerMatrixAction, TriggerMatrixRequest, TriggerRowAction, TriggerRowRequest,
    TriggerSequenceAction, TriggerSequenceRequest, TriggerSlotAction, TriggerSlotRequest,
    TriggerTrackAction, TriggerTrackRequest,
};
use base::future_util;
use base::tracing_util::ok_or_log_as_warn;
use playtime_api::persistence::{
    ColumnAddress, MatrixSequenceId, RowAddress, SlotAddress, TrackId,
};
use playtime_api::runtime::SimpleMappingTarget;

use helgoboss_learn::UnitValue;
#[cfg(feature = "playtime")]
use playtime_clip_engine::{
    base::ClipAddress, base::Matrix, rt::ColumnPlaySlotOptions, ClipEngine,
};
use reaper_high::{GroupingBehavior, Guid, OrCurrentProject, Pan, Reaper, Track};
use reaper_medium::{Bpm, Db, GangBehavior, ReaperPanValue, SoloMode};
use tonic::{Response, Status};

#[derive(Debug)]
pub struct PlaytimeProtoRequestHandler;

impl PlaytimeProtoRequestHandler {
    pub fn trigger_slot(&self, req: TriggerSlotRequest) -> Result<Response<Empty>, Status> {
        let action = TriggerSlotAction::try_from(req.action)
            .map_err(|_| Status::invalid_argument("unknown trigger slot action"))?;
        self.handle_slot_command(&req.slot_address, |matrix, slot_address| match action {
            TriggerSlotAction::Play => matrix.play_slot(
                slot_address,
                ColumnPlaySlotOptions {
                    velocity: Some(UnitValue::MAX),
                    stop_column_if_slot_empty: false,
                    start_timing: None,
                },
            ),
            TriggerSlotAction::Stop => matrix.stop_slot(slot_address, None),
            TriggerSlotAction::Record => matrix.record_slot(slot_address),
            TriggerSlotAction::Clear => matrix.clear_slot(slot_address),
            TriggerSlotAction::Copy => matrix.copy_slot(slot_address),
            TriggerSlotAction::Cut => matrix.cut_slot(slot_address),
            TriggerSlotAction::Paste => matrix.paste_slot(slot_address),
            TriggerSlotAction::ImportSelectedItems => matrix.import_selected_items(slot_address),
            TriggerSlotAction::Panic => matrix.panic_slot(slot_address),
            TriggerSlotAction::CreateEmptyMidiClip => {
                matrix.create_empty_midi_clip_in_slot(slot_address)
            }
            TriggerSlotAction::ToggleLearnSimpleMapping => {
                matrix
                    .toggle_learn_source_by_target(SimpleMappingTarget::TriggerSlot(slot_address));
                Ok(())
            }
            TriggerSlotAction::RemoveSimpleMapping => {
                matrix.remove_mapping_by_target(SimpleMappingTarget::TriggerSlot(slot_address));
                Ok(())
            }
            TriggerSlotAction::TriggerOn => matrix.trigger_slot(slot_address, UnitValue::MAX),
            TriggerSlotAction::TriggerOff => matrix.trigger_slot(slot_address, UnitValue::MIN),
            TriggerSlotAction::Activate => matrix.activate_slot(slot_address),
        })
    }

    pub fn import_files(&self, req: ImportFilesRequest) -> Result<Response<Empty>, Status> {
        self.handle_slot_command(&req.slot_address, |matrix, slot_address| {
            matrix.import_files(slot_address, req.files)
        })
    }

    pub fn trigger_clip(&self, req: TriggerClipRequest) -> Result<Response<Empty>, Status> {
        let action = TriggerClipAction::try_from(req.action)
            .map_err(|_| Status::invalid_argument("unknown trigger clip action"))?;
        self.handle_clip_command(&req.clip_address, |matrix, clip_address| match action {
            TriggerClipAction::MidiOverdub => matrix.midi_overdub_clip(clip_address),
            TriggerClipAction::Edit => matrix.start_editing_clip(clip_address),
            TriggerClipAction::Remove => matrix.remove_clip_from_slot(clip_address),
            TriggerClipAction::Promote => matrix.promote_clip_within_slot(clip_address),
            TriggerClipAction::Quantize => matrix.quantize_clip(clip_address),
            TriggerClipAction::Unquantize => matrix.unquantize_clip(clip_address),
            TriggerClipAction::OpenInMediaExplorer => {
                matrix.open_clip_source_in_media_explorer(clip_address)
            }
            TriggerClipAction::ExportToClipboard => matrix.export_clip_to_clipboard(clip_address),
        })
    }

    pub fn drag_slot(&self, req: DragSlotRequest) -> Result<Response<Empty>, Status> {
        let action = DragSlotAction::try_from(req.action)
            .map_err(|_| Status::invalid_argument("unknown drag slot action"))?;
        let source_slot_address = convert_slot_address_to_engine(&req.source_slot_address)?;
        let dest_slot_address = convert_slot_address_to_engine(&req.destination_slot_address)?;
        self.handle_matrix_command(&req.matrix_id, |matrix| match action {
            DragSlotAction::Move => matrix.move_slot_to(source_slot_address, dest_slot_address),
            DragSlotAction::Copy => matrix.copy_slot_to(source_slot_address, dest_slot_address),
        })
    }

    pub fn drag_clip(&self, req: DragClipRequest) -> Result<Response<Empty>, Status> {
        let action = DragClipAction::try_from(req.action)
            .map_err(|_| Status::invalid_argument("unknown drag clip action"))?;
        let source_clip_address = convert_clip_address_to_engine(&req.source_clip_address)?;
        let dest_slot_address = convert_slot_address_to_engine(&req.destination_slot_address)?;
        self.handle_matrix_command(&req.matrix_id, |matrix| match action {
            DragClipAction::Move => matrix.move_clip_to(source_clip_address, dest_slot_address),
        })
    }

    pub fn drag_row(&self, req: DragRowRequest) -> Result<Response<Empty>, Status> {
        let action = DragRowAction::try_from(req.action)
            .map_err(|_| Status::invalid_argument("unknown drag row action"))?;
        self.handle_matrix_command(&req.matrix_id, |matrix| match action {
            DragRowAction::MoveContent => matrix
                .move_scene_content_to(req.source_row_index as _, req.destination_row_index as _),
            DragRowAction::CopyContent => matrix
                .copy_scene_content_to(req.source_row_index as _, req.destination_row_index as _),
            DragRowAction::Reorder => {
                matrix.reorder_rows(req.source_row_index as _, req.destination_row_index as _)
            }
        })
    }

    pub fn drag_column(&self, req: DragColumnRequest) -> Result<Response<Empty>, Status> {
        let action = DragColumnAction::try_from(req.action)
            .map_err(|_| Status::invalid_argument("unknown drag column action"))?;
        self.handle_matrix_command(&req.matrix_id, |matrix| match action {
            DragColumnAction::Reorder => matrix.reorder_columns(
                req.source_column_index as _,
                req.destination_column_index as _,
            ),
        })
    }

    pub fn set_track_name(&self, req: SetTrackNameRequest) -> Result<Response<Empty>, Status> {
        self.handle_track_command(&req.track_address, |_matrix, track| {
            track.set_name(req.name);
            Ok(())
        })
    }

    pub fn set_track_color(&self, req: SetTrackColorRequest) -> Result<Response<Empty>, Status> {
        self.handle_track_command(&req.track_address, |matrix, track| {
            let color = req.color.and_then(|tc| tc.to_engine());
            matrix.set_track_color(track, color);
            Ok(())
        })
    }

    pub fn set_clip_name(&self, req: SetClipNameRequest) -> Result<Response<Empty>, Status> {
        self.handle_clip_command(&req.clip_address, |matrix, clip_address| {
            matrix.set_clip_name(clip_address, req.name)
        })
    }

    pub fn set_clip_data(&self, req: SetClipDataRequest) -> Result<Response<Empty>, Status> {
        let clip =
            serde_json::from_str(&req.data).map_err(|e| Status::invalid_argument(e.to_string()))?;
        self.handle_clip_command(&req.clip_address, |matrix, clip_address| {
            matrix.set_clip_data(clip_address, clip)
        })
    }

    pub fn trigger_sequence(&self, req: TriggerSequenceRequest) -> Result<Response<Empty>, Status> {
        let action: TriggerSequenceAction = TriggerSequenceAction::try_from(req.action)
            .map_err(|_| Status::invalid_argument("unknown trigger sequence action"))?;
        self.handle_sequence_command(req.sequence_id, |matrix, seq_id| match action {
            TriggerSequenceAction::Activate => matrix.activate_sequence(seq_id),
            TriggerSequenceAction::Remove => matrix.remove_sequence(seq_id),
        })
    }

    pub fn set_sequence_info(
        &self,
        req: SetSequenceInfoRequest,
    ) -> Result<Response<Empty>, Status> {
        self.handle_sequence_command(req.sequence_id, |matrix, seq_id| {
            let sequence_info = serde_json::from_str(&req.data)
                .map_err(|e| Status::invalid_argument(e.to_string()))?;
            matrix.set_sequence_info(seq_id, sequence_info)
        })
    }

    pub fn trigger_matrix(&self, req: TriggerMatrixRequest) -> Result<Response<Empty>, Status> {
        let action: TriggerMatrixAction = TriggerMatrixAction::try_from(req.action)
            .map_err(|_| Status::invalid_argument("unknown trigger matrix action"))?;
        if action == TriggerMatrixAction::CreateMatrix {
            self.create_matrix(&req.matrix_id)
                .map_err(|e| Status::not_found(e.to_string()))?;
            return Ok(Response::new(Empty {}));
        }
        self.handle_matrix_command(&req.matrix_id, |matrix| {
            let project = matrix.permanent_project().or_current_project();
            match action {
                TriggerMatrixAction::CreateMatrix => {
                    unreachable!("matrix creation handled above")
                }
                TriggerMatrixAction::SequencerCleanArrangement => matrix.clean_arrangement(),
                TriggerMatrixAction::SequencerWriteToArrangement => {
                    matrix.write_active_sequence_to_arrangement()
                }
                TriggerMatrixAction::ArrangementTogglePlayStop => {
                    if project.is_playing() {
                        project.stop();
                    } else {
                        project.play();
                    }
                    Ok(())
                }
                TriggerMatrixAction::SequencerPlay => {
                    matrix.play_active_sequence()?;
                    Ok(())
                }
                TriggerMatrixAction::SequencerRecord => {
                    matrix.record_new_sequence();
                    Ok(())
                }
                TriggerMatrixAction::SequencerStop => {
                    matrix.stop_sequencer();
                    Ok(())
                }
                TriggerMatrixAction::ToggleSilenceMode => {
                    matrix.toggle_silence_mode();
                    Ok(())
                }
                TriggerMatrixAction::PlayAllIgnitedClips => {
                    matrix.play_all_ignited();
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
                TriggerMatrixAction::ToggleClick => {
                    matrix.toggle_click();
                    Ok(())
                }
                TriggerMatrixAction::Panic => {
                    matrix.panic();
                    Ok(())
                }
                TriggerMatrixAction::ToggleMute => {
                    matrix.toggle_mute();
                    Ok(())
                }
                TriggerMatrixAction::ShowMasterFx => {
                    matrix.show_master_fx();
                    Ok(())
                }
                TriggerMatrixAction::ShowMasterRouting => {
                    matrix.show_master_routing();
                    Ok(())
                }
                TriggerMatrixAction::TapTempo => {
                    matrix.tap_tempo();
                    Ok(())
                }
                TriggerMatrixAction::ToggleLearnSimpleMapping => {
                    matrix.toggle_learn_source_by_target(SimpleMappingTarget::TriggerMatrix);
                    Ok(())
                }
                TriggerMatrixAction::RemoveSimpleMapping => {
                    matrix.toggle_learn_source_by_target(SimpleMappingTarget::TriggerMatrix);
                    Ok(())
                }
                TriggerMatrixAction::TriggerSmartRecord => matrix.trigger_smart_record(),
            }
        })
    }

    pub fn set_matrix_settings(
        &self,
        req: SetMatrixSettingsRequest,
    ) -> Result<Response<Empty>, Status> {
        let matrix_settings = serde_json::from_str(&req.settings)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
        self.handle_matrix_command(&req.matrix_id, |matrix| {
            matrix.set_settings(matrix_settings)
        })
    }

    pub fn trigger_column(&self, req: TriggerColumnRequest) -> Result<Response<Empty>, Status> {
        let action = TriggerColumnAction::try_from(req.action)
            .map_err(|_| Status::invalid_argument("unknown trigger column action"))?;
        self.handle_column_command(&req.column_address, |matrix, column_index| match action {
            TriggerColumnAction::Stop => matrix.stop_column(column_index, None),
            TriggerColumnAction::Remove => matrix.remove_column(column_index),
            TriggerColumnAction::Duplicate => matrix.duplicate_column(column_index),
            TriggerColumnAction::Insert => matrix.insert_column(column_index),
            TriggerColumnAction::Panic => matrix.panic_column(column_index),
            TriggerColumnAction::ToggleLearnSimpleMapping => {
                matrix.toggle_learn_source_by_target(SimpleMappingTarget::TriggerColumn(
                    ColumnAddress {
                        index: column_index,
                    },
                ));
                Ok(())
            }
            TriggerColumnAction::RemoveSimpleMapping => {
                matrix.remove_mapping_by_target(SimpleMappingTarget::TriggerColumn(
                    ColumnAddress {
                        index: column_index,
                    },
                ));
                Ok(())
            }
        })
    }

    pub fn trigger_track(&self, req: TriggerTrackRequest) -> Result<Response<Empty>, Status> {
        let action = TriggerTrackAction::try_from(req.action)
            .map_err(|_| Status::invalid_argument("unknown trigger track action"))?;
        self.handle_track_command(&req.track_address, |matrix, track| match action {
            TriggerTrackAction::ToggleMute => {
                track.set_mute(
                    !track.is_muted(),
                    GangBehavior::DenyGang,
                    GroupingBehavior::PreventGrouping,
                );
                Ok(())
            }
            TriggerTrackAction::ToggleSolo => {
                let new_solo_mode = if track.is_solo() {
                    SoloMode::Off
                } else {
                    SoloMode::SoloInPlace
                };
                track.set_solo_mode(new_solo_mode);
                Ok(())
            }
            TriggerTrackAction::ToggleArm => {
                matrix.toggle_track_armed(track);
                Ok(())
            }
            TriggerTrackAction::ShowFx => {
                matrix.show_track_fx(track);
                Ok(())
            }
            TriggerTrackAction::ShowRouting => {
                matrix.show_track_routing(&track);
                Ok(())
            }
        })
    }

    pub fn set_column_settings(
        &self,
        req: SetColumnSettingsRequest,
    ) -> Result<Response<Empty>, Status> {
        let column_settings = serde_json::from_str(&req.settings)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
        self.handle_column_command(&req.column_address, |matrix, column_index| {
            matrix.set_column_settings(column_index, column_settings)
        })
    }

    pub fn trigger_row(&self, req: TriggerRowRequest) -> Result<Response<Empty>, Status> {
        let action = TriggerRowAction::try_from(req.action)
            .map_err(|_| Status::invalid_argument("unknown trigger row action"))?;
        self.handle_row_command(&req.row_address, |matrix, row_index| match action {
            TriggerRowAction::Play => {
                matrix.play_scene(row_index);
                Ok(())
            }
            TriggerRowAction::Clear => matrix.clear_scene(row_index),
            TriggerRowAction::Copy => matrix.copy_scene(row_index),
            TriggerRowAction::Cut => matrix.cut_scene(row_index),
            TriggerRowAction::Paste => matrix.paste_scene(row_index),
            TriggerRowAction::Remove => matrix.remove_row(row_index),
            TriggerRowAction::Duplicate => matrix.duplicate_row(row_index),
            TriggerRowAction::Insert => matrix.insert_row(row_index),
            TriggerRowAction::Panic => matrix.panic_row(row_index),
            TriggerRowAction::ToggleLearnSimpleMapping => {
                matrix.toggle_learn_source_by_target(SimpleMappingTarget::TriggerRow(RowAddress {
                    index: row_index,
                }));
                Ok(())
            }
            TriggerRowAction::RemoveSimpleMapping => {
                matrix.remove_mapping_by_target(SimpleMappingTarget::TriggerRow(RowAddress {
                    index: row_index,
                }));
                Ok(())
            }
        })
    }

    pub fn set_row_data(&self, req: SetRowDataRequest) -> Result<Response<Empty>, Status> {
        let row_data =
            serde_json::from_str(&req.data).map_err(|e| Status::invalid_argument(e.to_string()))?;
        self.handle_row_command(&req.row_address, |matrix, row_index| {
            matrix.set_row_data(row_index, row_data)
        })
    }

    pub fn set_matrix_tempo(&self, req: SetMatrixTempoRequest) -> Result<Response<Empty>, Status> {
        let bpm = Bpm::try_from(req.bpm).map_err(|e| Status::invalid_argument(e.to_string()))?;
        self.handle_matrix_command(&req.matrix_id, |matrix| {
            matrix.set_tempo(bpm);
            Ok(())
        })
    }

    pub fn set_matrix_time_signature(
        &self,
        req: SetMatrixTimeSignatureRequest,
    ) -> Result<Response<Empty>, Status> {
        let time_sig = req
            .time_signature
            .ok_or_else(|| Status::invalid_argument("no time signature given"))?
            .to_engine()
            .map_err(Status::invalid_argument)?;
        self.handle_matrix_command(&req.matrix_id, |matrix| {
            matrix.set_time_signature(time_sig);
            Ok(())
        })
    }

    pub fn set_matrix_volume(
        &self,
        req: SetMatrixVolumeRequest,
    ) -> Result<Response<Empty>, Status> {
        let db = Db::try_from(req.db).map_err(|e| Status::invalid_argument(e.to_string()))?;
        self.handle_matrix_command(&req.matrix_id, |matrix| {
            let project = matrix.permanent_project().or_current_project();
            project.master_track()?.set_volume(
                db.to_linear_volume_value(),
                GangBehavior::DenyGang,
                GroupingBehavior::PreventGrouping,
            );
            Ok(())
        })
    }

    pub fn set_matrix_pan(&self, req: SetMatrixPanRequest) -> Result<Response<Empty>, Status> {
        let pan = ReaperPanValue::try_from(req.pan)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
        self.handle_matrix_command(&req.matrix_id, |matrix| {
            let project = matrix.permanent_project().or_current_project();
            project.master_track()?.set_pan(
                Pan::from_reaper_value(pan),
                GangBehavior::DenyGang,
                GroupingBehavior::PreventGrouping,
            );
            Ok(())
        })
    }

    pub fn set_track_volume(&self, req: SetTrackVolumeRequest) -> Result<Response<Empty>, Status> {
        let db = Db::try_from(req.db).map_err(|e| Status::invalid_argument(e.to_string()))?;
        self.handle_track_command(&req.track_address, |_matrix, track| {
            track.set_volume(
                db.to_linear_volume_value(),
                GangBehavior::DenyGang,
                GroupingBehavior::PreventGrouping,
            );
            Ok(())
        })
    }

    pub fn set_track_pan(&self, req: SetTrackPanRequest) -> Result<Response<Empty>, Status> {
        let pan = ReaperPanValue::new_panic(req.pan.clamp(-1.0, 1.0));
        self.handle_track_command(&req.track_address, |_matrix, track| {
            track.set_pan(
                Pan::from_reaper_value(pan),
                GangBehavior::DenyGang,
                GroupingBehavior::PreventGrouping,
            );
            Ok(())
        })
    }

    pub async fn set_column_track(
        &self,
        req: SetColumnTrackRequest,
    ) -> Result<Response<Empty>, Status> {
        // We shouldn't just change the column track directly, otherwise we get abrupt clicks
        // (audio) and hanging notes (MIDI). The following is a dirty but efficient solution to
        // prevent this.
        // Immediately stop everything in that column (gracefully)
        self.handle_column_internal(&req.column_address, |matrix, column_index| {
            matrix.get_column(column_index)?.panic(true);
            Ok(())
        })?;
        // Make sure to wait long enough until fade outs and stuff finished
        future_util::millis(50).await;
        // Finally change column track
        self.handle_column_command(&req.column_address, |matrix, column_index| {
            let track_id = req.track_id.map(TrackId::new);
            matrix.set_column_playback_track(column_index, track_id.as_ref())?;
            Ok(())
        })
    }

    pub fn set_track_input_monitoring(
        &self,
        req: SetTrackInputMonitoringRequest,
    ) -> Result<Response<Empty>, Status> {
        self.handle_track_command(&req.track_address, |matrix, track| {
            matrix.set_track_input_monitoring(track, req.input_monitoring().to_engine());
            Ok(())
        })
    }

    pub fn set_track_input(&self, req: SetTrackInputRequest) -> Result<Response<Empty>, Status> {
        self.handle_track_command(&req.track_address, |matrix, track| {
            matrix.set_track_input(track, req.input.and_then(|i| i.to_engine()));
            Ok(())
        })
    }

    pub async fn get_clip_detail(
        &self,
        req: GetClipDetailRequest,
    ) -> Result<Response<GetClipDetailReply>, Status> {
        let peak_file_future =
            self.handle_clip_internal(&req.clip_address, |matrix, clip_address| {
                let clip = matrix.get_clip(clip_address)?;
                let peak_file_future = clip.peak_file_contents(matrix.permanent_project())?;
                Ok(peak_file_future)
            })?;
        let reply = GetClipDetailReply {
            rea_peaks: ok_or_log_as_warn(peak_file_future.await),
        };
        Ok(Response::new(reply))
    }

    pub async fn prove_authenticity(
        &self,
        req: ProveAuthenticityRequest,
    ) -> Result<Response<ProveAuthenticityReply>, Status> {
        let signature = ClipEngine::prove_authenticity(&req.challenge)
            .ok_or_else(|| Status::unknown("authenticity proof failed"))?;
        Ok(Response::new(ProveAuthenticityReply { signature }))
    }

    pub async fn get_project_dir(
        &self,
        req: GetProjectDirRequest,
    ) -> Result<Response<GetProjectDirReply>, Status> {
        let project_dir = self.handle_matrix_internal(&req.matrix_id, |matrix| {
            let project = matrix.temporary_project();
            let project_dir = project
                .directory()
                .unwrap_or_else(|| project.recording_path());
            Ok(project_dir)
        })?;
        let reply = GetProjectDirReply {
            project_dir: project_dir.to_string_lossy().to_string(),
        };
        Ok(Response::new(reply))
    }

    pub async fn get_arrangement_info(
        &self,
        req: GetArrangementInfoRequest,
    ) -> Result<Response<GetArrangementInfoReply>, Status> {
        let clean = self
            .handle_matrix_internal(&req.matrix_id, |matrix| Ok(matrix.arrangement_is_clean()))?;
        let reply = GetArrangementInfoReply { clean };
        Ok(Response::new(reply))
    }

    fn handle_matrix_command(
        &self,
        matrix_id: &str,
        handler: impl FnOnce(&mut Matrix) -> anyhow::Result<()>,
    ) -> Result<Response<Empty>, Status> {
        self.handle_matrix_internal(matrix_id, handler)?;
        Ok(Response::new(Empty {}))
    }

    fn handle_matrix_internal<R>(
        &self,
        matrix_id: &str,
        handler: impl FnOnce(&mut Matrix) -> anyhow::Result<R>,
    ) -> Result<R, Status> {
        let r = self
            .with_matrix_mut(matrix_id, handler)
            .map_err(|e| Status::not_found(format!("{e:#}")))?
            .map_err(|e| Status::unknown(format!("{e:#}")))?;
        Ok(r)
    }

    fn handle_column_command(
        &self,
        full_column_id: &Option<FullColumnAddress>,
        handler: impl FnOnce(&mut Matrix, usize) -> anyhow::Result<()>,
    ) -> Result<Response<Empty>, Status> {
        self.handle_column_internal(full_column_id, handler)?;
        Ok(Response::new(Empty {}))
    }

    fn handle_column_internal<R>(
        &self,
        full_column_id: &Option<FullColumnAddress>,
        handler: impl FnOnce(&mut Matrix, usize) -> anyhow::Result<R>,
    ) -> Result<R, Status> {
        let full_column_id = full_column_id
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("need full column address"))?;
        let column_index = full_column_id.column_index as usize;
        self.handle_matrix_internal(&full_column_id.matrix_id, |matrix| {
            handler(matrix, column_index)
        })
    }

    fn handle_sequence_command(
        &self,
        full_sequence_id: Option<FullSequenceId>,
        handler: impl FnOnce(&mut Matrix, MatrixSequenceId) -> anyhow::Result<()>,
    ) -> Result<Response<Empty>, Status> {
        self.handle_sequence_internal(full_sequence_id, handler)?;
        Ok(Response::new(Empty {}))
    }

    fn handle_sequence_internal<R>(
        &self,
        full_sequence_id: Option<FullSequenceId>,
        handler: impl FnOnce(&mut Matrix, MatrixSequenceId) -> anyhow::Result<R>,
    ) -> Result<R, Status> {
        let full_sequence_id =
            full_sequence_id.ok_or_else(|| Status::invalid_argument("need full sequence ID"))?;
        let sequence_id = MatrixSequenceId::new(full_sequence_id.sequence_id);
        self.handle_matrix_internal(&full_sequence_id.matrix_id, |matrix| {
            handler(matrix, sequence_id)
        })
    }

    fn handle_row_command(
        &self,
        full_row_id: &Option<FullRowAddress>,
        handler: impl FnOnce(&mut Matrix, usize) -> anyhow::Result<()>,
    ) -> Result<Response<Empty>, Status> {
        let full_row_id = full_row_id
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("need full row address"))?;
        let row_index = full_row_id.row_index as usize;
        self.handle_matrix_command(&full_row_id.matrix_id, |matrix| handler(matrix, row_index))
    }

    fn handle_slot_command(
        &self,
        full_slot_address: &Option<FullSlotAddress>,
        handler: impl FnOnce(&mut Matrix, SlotAddress) -> anyhow::Result<()>,
    ) -> Result<Response<Empty>, Status> {
        let full_slot_address = full_slot_address
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("need full slot address"))?;
        let slot_addr = convert_slot_address_to_engine(&full_slot_address.slot_address)?;
        self.handle_matrix_command(&full_slot_address.matrix_id, |matrix| {
            handler(matrix, slot_addr)
        })
    }

    fn handle_clip_command(
        &self,
        full_clip_address: &Option<FullClipAddress>,
        handler: impl FnOnce(&mut Matrix, ClipAddress) -> anyhow::Result<()>,
    ) -> Result<Response<Empty>, Status> {
        self.handle_clip_internal(full_clip_address, handler)?;
        Ok(Response::new(Empty {}))
    }

    pub(crate) fn handle_clip_internal<R>(
        &self,
        full_clip_address: &Option<FullClipAddress>,
        handler: impl FnOnce(&mut Matrix, ClipAddress) -> anyhow::Result<R>,
    ) -> Result<R, Status> {
        let full_clip_address = full_clip_address
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("need full clip address"))?;
        let clip_addr = convert_clip_address_to_engine(&full_clip_address.clip_address)?;
        self.handle_matrix_internal(&full_clip_address.matrix_id, |matrix| {
            handler(matrix, clip_addr)
        })
    }

    fn handle_track_command(
        &self,
        full_track_address: &Option<FullTrackAddress>,
        handler: impl FnOnce(&Matrix, Track) -> anyhow::Result<()>,
    ) -> Result<Response<Empty>, Status> {
        let full_track_address = full_track_address
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("need full track address"))?;
        self.handle_track_internal(full_track_address, handler)?;
        Ok(Response::new(Empty {}))
    }

    fn handle_track_internal<R>(
        &self,
        track_address: &FullTrackAddress,
        handler: impl FnOnce(&Matrix, Track) -> anyhow::Result<R>,
    ) -> Result<R, Status> {
        self.handle_matrix_internal(&track_address.matrix_id, |matrix| {
            let guid = Guid::from_string_without_braces(&track_address.track_id)
                .map_err(anyhow::Error::msg)?;
            let track = matrix.temporary_project().track_by_guid(&guid)?;
            handler(matrix, track)
        })
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

fn convert_slot_address_to_engine(
    addr: &Option<proto::SlotAddress>,
) -> Result<SlotAddress, Status> {
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
