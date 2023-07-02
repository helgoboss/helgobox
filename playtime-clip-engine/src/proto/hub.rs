use crate::base::{ClipMatrixEvent, Matrix};
use crate::proto::clip_engine_server::ClipEngineServer;
use crate::proto::senders::{
    ClipEngineSenders, ContinuousColumnUpdateBatch, ContinuousMatrixUpdateBatch,
    ContinuousSlotUpdateBatch, OccasionalClipUpdateBatch, OccasionalColumnUpdateBatch,
    OccasionalMatrixUpdateBatch, OccasionalRowUpdateBatch, OccasionalSlotUpdateBatch,
    OccasionalTrackUpdateBatch,
};
use crate::proto::{
    occasional_matrix_update, occasional_track_update, qualified_occasional_clip_update,
    qualified_occasional_column_update, qualified_occasional_row_update,
    qualified_occasional_slot_update, ClipEngineService, ContinuousClipUpdate,
    ContinuousColumnUpdate, ContinuousMatrixUpdate, ContinuousSlotUpdate, MatrixProvider,
    OccasionalMatrixUpdate, OccasionalTrackUpdate, QualifiedContinuousSlotUpdate,
    QualifiedOccasionalClipUpdate, QualifiedOccasionalColumnUpdate, QualifiedOccasionalRowUpdate,
    QualifiedOccasionalSlotUpdate, QualifiedOccasionalTrackUpdate, SlotAddress,
};
use crate::rt::{
    ClipChangeEvent, QualifiedClipChangeEvent, QualifiedSlotChangeEvent, SlotChangeEvent,
};
use crate::{clip_timeline, proto, Laziness, Timeline};
use playtime_api::persistence::EvenQuantization;
use reaper_high::{
    AvailablePanValue, ChangeEvent, Guid, OrCurrentProject, PanExt, Project, Reaper, Track, Volume,
};
use reaper_medium::TrackAttributeKey;
use std::collections::HashMap;

#[derive(Debug)]
pub struct ClipEngineHub {
    senders: ClipEngineSenders,
}

impl Default for ClipEngineHub {
    fn default() -> Self {
        Self::new()
    }
}

impl ClipEngineHub {
    pub fn new() -> Self {
        Self {
            senders: ClipEngineSenders::new(),
        }
    }

    pub fn create_service<P: MatrixProvider>(
        &self,
        matrix_provider: P,
    ) -> ClipEngineServer<ClipEngineService<P>> {
        ClipEngineServer::new(ClipEngineService::new(
            matrix_provider,
            self.senders.clone(),
        ))
    }

    pub fn clip_matrix_changed(
        &self,
        matrix_id: &str,
        matrix: &Matrix,
        events: &[ClipMatrixEvent],
        is_poll: bool,
        project: Option<Project>,
    ) {
        self.send_occasional_matrix_updates_caused_by_matrix(matrix_id, matrix, events);
        self.send_occasional_column_updates(matrix_id, matrix, events);
        self.send_occasional_row_updates(matrix_id, matrix, events);
        self.send_occasional_slot_updates(matrix_id, matrix, events);
        self.send_occasional_clip_updates(matrix_id, matrix, events);
        self.send_continuous_slot_updates(matrix_id, events);
        if is_poll {
            self.send_continuous_matrix_updates(matrix_id, project);
            self.send_continuous_column_updates(matrix_id, matrix);
        }
    }

    fn send_occasional_matrix_updates_caused_by_matrix(
        &self,
        matrix_id: &str,
        matrix: &Matrix,
        events: &[ClipMatrixEvent],
    ) {
        let sender = &self.senders.occasional_matrix_update_sender;
        if sender.receiver_count() == 0 {
            return;
        }
        // TODO-high-clip-engine-performance Push persistent matrix state only once (even if many events)
        let updates: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                ClipMatrixEvent::EverythingChanged => Some(OccasionalMatrixUpdate {
                    update: Some(occasional_matrix_update::Update::complete_persistent_data(
                        matrix,
                    )),
                }),
                ClipMatrixEvent::MatrixSettingsChanged => Some(OccasionalMatrixUpdate {
                    update: Some(occasional_matrix_update::Update::settings(matrix)),
                }),
                ClipMatrixEvent::HistoryChanged => Some(OccasionalMatrixUpdate {
                    update: Some(occasional_matrix_update::Update::history_state(matrix)),
                }),
                _ => None,
            })
            .collect();
        if !updates.is_empty() {
            let batch_event = OccasionalMatrixUpdateBatch {
                session_id: matrix_id.to_string(),
                value: updates,
            };
            let _ = sender.send(batch_event);
        }
    }

    fn send_occasional_column_updates(
        &self,
        matrix_id: &str,
        matrix: &Matrix,
        events: &[ClipMatrixEvent],
    ) {
        let sender = &self.senders.occasional_column_update_sender;
        if sender.receiver_count() == 0 {
            return;
        }
        let updates: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                ClipMatrixEvent::ColumnSettingsChanged(column_index) => {
                    let update =
                        qualified_occasional_column_update::Update::settings(matrix, *column_index)
                            .ok()?;
                    Some(QualifiedOccasionalColumnUpdate {
                        column_index: *column_index as _,
                        update: Some(update),
                    })
                }
                _ => None,
            })
            .collect();
        if !updates.is_empty() {
            let batch_event = OccasionalColumnUpdateBatch {
                session_id: matrix_id.to_string(),
                value: updates,
            };
            let _ = sender.send(batch_event);
        }
    }
    fn send_occasional_row_updates(
        &self,
        matrix_id: &str,
        matrix: &Matrix,
        events: &[ClipMatrixEvent],
    ) {
        let sender = &self.senders.occasional_row_update_sender;
        if sender.receiver_count() == 0 {
            return;
        }
        let updates: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                ClipMatrixEvent::RowChanged(row_index) => {
                    let update =
                        qualified_occasional_row_update::Update::data(matrix, *row_index).ok()?;
                    Some(QualifiedOccasionalRowUpdate {
                        row_index: *row_index as _,
                        update: Some(update),
                    })
                }
                _ => None,
            })
            .collect();
        if !updates.is_empty() {
            let batch_event = OccasionalRowUpdateBatch {
                session_id: matrix_id.to_string(),
                value: updates,
            };
            let _ = sender.send(batch_event);
        }
    }

    fn send_occasional_slot_updates(
        &self,
        matrix_id: &str,
        matrix: &Matrix,
        events: &[ClipMatrixEvent],
    ) {
        let sender = &self.senders.occasional_slot_update_sender;
        if sender.receiver_count() == 0 {
            return;
        }
        let updates: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                ClipMatrixEvent::SlotChanged(QualifiedSlotChangeEvent {
                    slot_address: slot_coordinates,
                    event,
                }) => {
                    use SlotChangeEvent::*;
                    let update = match event {
                        PlayState(play_state) => {
                            qualified_occasional_slot_update::Update::play_state(*play_state)
                        }
                        Clips(_) => {
                            let slot = matrix.find_slot(*slot_coordinates)?;
                            qualified_occasional_slot_update::Update::complete_persistent_data(
                                matrix, slot,
                            )
                        }
                        Continuous { .. } => return None,
                    };
                    Some(QualifiedOccasionalSlotUpdate {
                        slot_address: Some(SlotAddress::from_engine(*slot_coordinates)),
                        update: Some(update),
                    })
                }
                _ => None,
            })
            .collect();
        if !updates.is_empty() {
            let batch_event = OccasionalSlotUpdateBatch {
                session_id: matrix_id.to_string(),
                value: updates,
            };
            let _ = sender.send(batch_event);
        }
    }

    fn send_occasional_clip_updates(
        &self,
        matrix_id: &str,
        matrix: &Matrix,
        events: &[ClipMatrixEvent],
    ) {
        let sender = &self.senders.occasional_clip_update_sender;
        if sender.receiver_count() == 0 {
            return;
        }
        let updates: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                ClipMatrixEvent::ClipChanged(QualifiedClipChangeEvent {
                    clip_address,
                    event,
                }) => {
                    use ClipChangeEvent::*;
                    let update = match event {
                        Everything | Volume(_) | Looped(_) => {
                            let clip = matrix.find_clip(*clip_address)?;
                            qualified_occasional_clip_update::Update::complete_persistent_data(
                                matrix, clip,
                            )
                            .ok()?
                        }
                    };
                    Some(QualifiedOccasionalClipUpdate {
                        clip_address: Some(proto::ClipAddress::from_engine(*clip_address)),
                        update: Some(update),
                    })
                }
                _ => None,
            })
            .collect();
        if !updates.is_empty() {
            let batch_event = OccasionalClipUpdateBatch {
                session_id: matrix_id.to_string(),
                value: updates,
            };
            let _ = sender.send(batch_event);
        }
    }

    pub fn send_occasional_matrix_updates_caused_by_reaper(
        &self,
        matrix_id: &str,
        matrix: &Matrix,
        event: &ChangeEvent,
    ) {
        use occasional_track_update::Update;
        enum R {
            Matrix(occasional_matrix_update::Update),
            Track(QualifiedOccasionalTrackUpdate),
        }
        let matrix_update_sender = &self.senders.occasional_matrix_update_sender;
        let track_update_sender = &self.senders.occasional_track_update_sender;
        if matrix_update_sender.receiver_count() == 0 && track_update_sender.receiver_count() == 0 {
            return;
        }
        fn track_update(track: &Track, create_update: impl FnOnce() -> Update) -> R {
            R::Track(QualifiedOccasionalTrackUpdate {
                track_id: track.guid().to_string_without_braces(),
                track_updates: vec![OccasionalTrackUpdate {
                    update: Some(create_update()),
                }],
            })
        }
        fn column_track_update(
            matrix: &Matrix,
            track: &Track,
            create_update: impl FnOnce() -> Update,
        ) -> Option<R> {
            if matrix.uses_playback_track(track) {
                Some(track_update(track, create_update))
            } else {
                None
            }
        }
        let update: Option<R> = match event {
            ChangeEvent::TrackVolumeChanged(e) => {
                let db = Volume::from_reaper_value(e.new_value).db();
                if e.track.is_master_track() {
                    Some(R::Matrix(occasional_matrix_update::Update::volume(db)))
                } else {
                    column_track_update(matrix, &e.track, || Update::volume(db))
                }
            }
            ChangeEvent::TrackPanChanged(e) => {
                let val = match e.new_value {
                    AvailablePanValue::Complete(v) => v.main_pan(),
                    AvailablePanValue::Incomplete(v) => v,
                };
                if e.track.is_master_track() {
                    Some(R::Matrix(occasional_matrix_update::Update::pan(val)))
                } else {
                    column_track_update(matrix, &e.track, || Update::pan(val))
                }
            }
            ChangeEvent::TrackNameChanged(e) => {
                Some(track_update(&e.track, || Update::name(&e.track)))
            }
            ChangeEvent::TrackInputChanged(e) => {
                column_track_update(matrix, &e.track, || Update::input(e.new_value))
            }
            ChangeEvent::TrackInputMonitoringChanged(e) => {
                column_track_update(matrix, &e.track, || Update::input_monitoring(e.new_value))
            }
            ChangeEvent::TrackArmChanged(e) => {
                column_track_update(matrix, &e.track, || Update::armed(e.new_value))
            }
            ChangeEvent::TrackMuteChanged(e) => {
                column_track_update(matrix, &e.track, || Update::mute(e.new_value))
            }
            ChangeEvent::TrackSoloChanged(e) => {
                column_track_update(matrix, &e.track, || Update::solo(e.new_value))
            }
            ChangeEvent::TrackSelectedChanged(e) => {
                column_track_update(matrix, &e.track, || Update::selected(e.new_value))
            }
            ChangeEvent::MasterTempoChanged(e) => {
                // TODO-high-clip-engine Also notify correctly about time signature changes. Looks like
                //  MasterTempoChanged event doesn't fire in that case :(
                Some(R::Matrix(occasional_matrix_update::Update::tempo(
                    e.new_value,
                )))
            }
            ChangeEvent::PlayStateChanged(e) => Some(R::Matrix(
                occasional_matrix_update::Update::arrangement_play_state(e.new_value),
            )),
            _ => None,
        };
        if let Some(update) = update {
            match update {
                R::Matrix(u) => {
                    let _ = matrix_update_sender.send(OccasionalMatrixUpdateBatch {
                        session_id: matrix_id.to_string(),
                        value: vec![OccasionalMatrixUpdate { update: Some(u) }],
                    });
                }
                R::Track(u) => {
                    let _ = track_update_sender.send(OccasionalTrackUpdateBatch {
                        session_id: matrix_id.to_string(),
                        value: vec![u],
                    });
                }
            }
        }
    }

    fn send_continuous_slot_updates(&self, matrix_id: &str, events: &[ClipMatrixEvent]) {
        let sender = &self.senders.continuous_slot_update_sender;
        if sender.receiver_count() == 0 {
            return;
        }
        let updates: Vec<_> = events
            .iter()
            .filter_map(|event| {
                if let ClipMatrixEvent::SlotChanged(QualifiedSlotChangeEvent {
                    slot_address: slot_coordinates,
                    event:
                        SlotChangeEvent::Continuous {
                            proportional,
                            seconds,
                            peak,
                        },
                }) = event
                {
                    Some(QualifiedContinuousSlotUpdate {
                        slot_address: Some(SlotAddress::from_engine(*slot_coordinates)),
                        update: Some(ContinuousSlotUpdate {
                            // TODO-high-clip-engine Send for each clip
                            clip_update: vec![
                                (ContinuousClipUpdate {
                                    proportional_position: proportional.get(),
                                    position_in_seconds: seconds.get(),
                                    peak: peak.get(),
                                }),
                            ],
                        }),
                    })
                } else {
                    None
                }
            })
            .collect();
        if !updates.is_empty() {
            let batch_event = ContinuousSlotUpdateBatch {
                session_id: matrix_id.to_string(),
                value: updates,
            };
            let _ = sender.send(batch_event);
        }
    }

    fn send_continuous_matrix_updates(&self, matrix_id: &str, project: Option<Project>) {
        let sender = &self.senders.continuous_matrix_update_sender;
        if sender.receiver_count() == 0 {
            return;
        }
        let timeline = clip_timeline(project, false);
        let pos = timeline.cursor_pos();
        let bar_quantization = EvenQuantization::ONE_BAR;
        let next_bar =
            timeline.next_quantized_pos_at(pos, bar_quantization, Laziness::EagerForNextPos);
        // TODO-high-clip-engine CONTINUE We are mainly interested in beats relative to the bar in order to get a
        //  typical position display and a useful visual metronome!
        let full_beats = timeline.full_beats_at_pos(pos);
        let batch_event = ContinuousMatrixUpdateBatch {
            session_id: matrix_id.to_string(),
            value: ContinuousMatrixUpdate {
                second: pos.get(),
                bar: (next_bar.position() - 1) as i32,
                beat: full_beats.get(),
                peaks: project
                    .or_current_project()
                    .master_track()
                    .map(|t| get_track_peaks(&t))
                    .unwrap_or_default(),
            },
        };
        let _ = sender.send(batch_event);
    }

    fn send_continuous_column_updates(&self, matrix_id: &str, matrix: &Matrix) {
        let sender = &self.senders.continuous_column_update_sender;
        if sender.receiver_count() == 0 {
            return;
        }
        let column_update_by_track_guid: HashMap<Guid, ContinuousColumnUpdate> =
            HashMap::with_capacity(matrix.column_count());
        let column_updates: Vec<_> = matrix
            .all_columns()
            .map(|column| {
                if let Ok(track) = column.playback_track() {
                    if let Some(existing_update) = column_update_by_track_guid.get(track.guid()) {
                        // We have already collected the update for this column's playback track.
                        existing_update.clone()
                    } else {
                        // We haven't yet collected the update for this column's playback track.
                        ContinuousColumnUpdate {
                            peaks: get_track_peaks(track),
                        }
                    }
                } else {
                    ContinuousColumnUpdate { peaks: vec![] }
                }
            })
            .collect();
        if !column_updates.is_empty() {
            let batch_event = ContinuousColumnUpdateBatch {
                session_id: matrix_id.to_string(),
                value: column_updates,
            };
            let _ = sender.send(batch_event);
        }
    }
}

fn get_track_peaks(track: &Track) -> Vec<f64> {
    let reaper = Reaper::get().medium_reaper();
    let track = track.raw();
    let channel_count =
        unsafe { reaper.get_media_track_info_value(track, TrackAttributeKey::Nchan) as i32 };
    if channel_count <= 0 {
        return vec![];
    }
    // TODO-high-clip-engine CONTINUE Apply same fix as in #560 (check I_VUMODE to know whether to query volume or peaks)
    // TODO-high-clip-engine CONTINUE Respect solo (same as a recent ReaLearn issue)
    (0..channel_count)
        .map(|ch| {
            let volume = unsafe { reaper.track_get_peak_info(track, ch as u32) };
            volume.get()
        })
        .collect()
}
