use crate::infrastructure::data::{
    ControllerManager, FileBasedControllerPresetManager, FileBasedMainPresetManager,
};
use crate::infrastructure::plugin::InstanceShell;
use crate::infrastructure::proto;
use crate::infrastructure::proto::helgobox_service_server::HelgoboxServiceServer;
use crate::infrastructure::proto::senders::{
    ContinuousColumnUpdateBatch, ContinuousMatrixUpdateBatch, ContinuousSlotUpdateBatch,
    OccasionalClipUpdateBatch, OccasionalColumnUpdateBatch, OccasionalMatrixUpdateBatch,
    OccasionalRowUpdateBatch, OccasionalSlotUpdateBatch, OccasionalTrackUpdateBatch, ProtoSenders,
};
use crate::infrastructure::proto::{
    occasional_global_update, occasional_instance_update, occasional_matrix_update,
    occasional_track_update, qualified_occasional_clip_update, qualified_occasional_column_update,
    qualified_occasional_row_update, qualified_occasional_slot_update, ContinuousColumnUpdate,
    ContinuousMatrixUpdate, ContinuousSlotUpdate, HelgoboxServiceImpl, MatrixProvider,
    OccasionalGlobalUpdate, OccasionalInstanceUpdate, OccasionalInstanceUpdateBatch,
    OccasionalMatrixUpdate, OccasionalTrackUpdate, ProtoRequestHandler,
    QualifiedContinuousSlotUpdate, QualifiedOccasionalClipUpdate, QualifiedOccasionalColumnUpdate,
    QualifiedOccasionalRowUpdate, QualifiedOccasionalSlotUpdate, QualifiedOccasionalTrackUpdate,
    SlotAddress,
};
use base::peak_util;
use playtime_api::persistence::EvenQuantization;
use playtime_clip_engine::base::{ClipMatrixEvent, Matrix, PlaytimeTrackInputProps};
use playtime_clip_engine::rt::{
    ClipChangeEvent, QualifiedClipChangeEvent, QualifiedSlotChangeEvent, SlotChangeEvent,
};
use playtime_clip_engine::{clip_timeline, Laziness, Timeline};
use reaper_high::{
    AvailablePanValue, ChangeEvent, Guid, OrCurrentProject, PanExt, Project, Track, Volume,
};
use std::collections::HashMap;

#[derive(Debug)]
pub struct ProtoHub {
    senders: ProtoSenders,
}

impl Default for ProtoHub {
    fn default() -> Self {
        Self::new()
    }
}

impl ProtoHub {
    pub fn new() -> Self {
        Self {
            senders: ProtoSenders::new(),
        }
    }

    pub fn senders(&self) -> &ProtoSenders {
        &self.senders
    }

    pub fn create_service<P: MatrixProvider + Clone>(
        &self,
        matrix_provider: P,
    ) -> HelgoboxServiceServer<HelgoboxServiceImpl<P>> {
        HelgoboxServiceServer::new(HelgoboxServiceImpl::new(
            matrix_provider.clone(),
            ProtoRequestHandler::new(matrix_provider),
            self.senders.clone(),
        ))
    }

    pub fn notify_midi_input_devices_changed(&self) {
        self.send_occasional_global_updates(|| {
            [occasional_global_update::Update::midi_input_devices()]
        });
    }

    pub fn notify_midi_output_devices_changed(&self) {
        self.send_occasional_global_updates(|| {
            [occasional_global_update::Update::midi_output_devices()]
        });
    }

    pub fn notify_controller_presets_changed(
        &self,
        preset_manager: &FileBasedControllerPresetManager,
    ) {
        self.send_occasional_global_updates(|| {
            [occasional_global_update::Update::controller_presets(
                preset_manager,
            )]
        });
    }

    pub fn notify_main_presets_changed(&self, preset_manager: &FileBasedMainPresetManager) {
        self.send_occasional_global_updates(|| {
            [occasional_global_update::Update::main_presets(
                preset_manager,
            )]
        });
    }

    pub fn notify_controller_config_changed(&self, controller_manager: &ControllerManager) {
        self.send_occasional_global_updates(|| {
            [occasional_global_update::Update::controller_config(
                controller_manager,
            )]
        });
    }

    fn send_occasional_global_updates<F, I>(&self, create_updates: F)
    where
        F: FnOnce() -> I,
        I: IntoIterator<Item = occasional_global_update::Update>,
    {
        let sender = &self.senders.occasional_global_update_sender;
        if sender.receiver_count() == 0 {
            return;
        }
        let wrapped_updates = create_updates()
            .into_iter()
            .map(|u| OccasionalGlobalUpdate { update: Some(u) });
        let _ = sender.send(wrapped_updates.collect());
    }

    fn send_occasional_instance_updates<F, I>(&self, instance_id: &str, create_updates: F)
    where
        F: FnOnce() -> I,
        I: IntoIterator<Item = occasional_instance_update::Update>,
    {
        let sender = &self.senders.occasional_instance_update_sender;
        if sender.receiver_count() == 0 {
            return;
        }
        let wrapped_updates = create_updates()
            .into_iter()
            .map(|u| OccasionalInstanceUpdate { update: Some(u) });
        let batch_event = OccasionalInstanceUpdateBatch {
            session_id: instance_id.to_string(),
            value: wrapped_updates.collect(),
        };
        let _ = sender.send(batch_event);
    }

    pub fn notify_clip_matrix_changed(
        &self,
        matrix_id: &str,
        matrix: &Matrix,
        events: &[ClipMatrixEvent],
        is_poll: bool,
        project: Option<Project>,
    ) {
        self.send_occasional_matrix_updates_caused_by_matrix(matrix_id, matrix, events);
        self.send_occasional_track_updates_caused_by_matrix(matrix_id, events);
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

    pub fn notify_instance_settings_changed(&self, instance_shell: &InstanceShell) {
        self.send_occasional_instance_updates(&instance_shell.instance_id().to_string(), || {
            [occasional_instance_update::Update::settings(instance_shell)]
        });
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
        let mut updates = vec![];
        let everything_changed = events
            .iter()
            .any(|e| matches!(e, ClipMatrixEvent::EverythingChanged));
        if everything_changed {
            let exists_update = OccasionalMatrixUpdate {
                update: Some(occasional_matrix_update::Update::MatrixExists(true)),
            };
            let complete_update = OccasionalMatrixUpdate {
                update: Some(occasional_matrix_update::Update::complete_persistent_data(
                    matrix,
                )),
            };
            updates.push(exists_update);
            updates.push(complete_update);
        } else {
            // Misc updates
            let misc_updates = events.iter().flat_map(|event| match event {
                ClipMatrixEvent::MatrixSettingsChanged => Some(OccasionalMatrixUpdate {
                    update: Some(occasional_matrix_update::Update::settings(matrix)),
                }),
                ClipMatrixEvent::HistoryChanged => Some(OccasionalMatrixUpdate {
                    update: Some(occasional_matrix_update::Update::history_state(matrix)),
                }),
                ClipMatrixEvent::ClickEnabledChanged => Some(OccasionalMatrixUpdate {
                    update: Some(occasional_matrix_update::Update::click_enabled(matrix)),
                }),
                ClipMatrixEvent::SilenceModeChanged => Some(OccasionalMatrixUpdate {
                    update: Some(occasional_matrix_update::Update::silence_mode(matrix)),
                }),
                ClipMatrixEvent::ActiveSlotChanged => Some(OccasionalMatrixUpdate {
                    update: Some(occasional_matrix_update::Update::active_slot(matrix)),
                }),
                ClipMatrixEvent::TimeSignatureChanged => Some(OccasionalMatrixUpdate {
                    update: Some(occasional_matrix_update::Update::time_signature(
                        matrix.temporary_project(),
                    )),
                }),
                ClipMatrixEvent::SequencerPlayStateChanged => Some(OccasionalMatrixUpdate {
                    update: Some(occasional_matrix_update::Update::sequencer_play_state(
                        matrix.sequencer().status(),
                    )),
                }),
                ClipMatrixEvent::SequencerChanged => Some(OccasionalMatrixUpdate {
                    update: Some(occasional_matrix_update::Update::sequencer(
                        matrix.sequencer(),
                    )),
                }),
                ClipMatrixEvent::Info(evt) => Some(OccasionalMatrixUpdate {
                    update: Some(occasional_matrix_update::Update::info_event(*evt)),
                }),
                ClipMatrixEvent::SimpleMappingsChanged => Some(OccasionalMatrixUpdate {
                    update: Some(occasional_matrix_update::Update::simple_mappings(matrix)),
                }),
                ClipMatrixEvent::LearningTargetChanged => Some(OccasionalMatrixUpdate {
                    update: Some(occasional_matrix_update::Update::learn_state(matrix)),
                }),
                _ => None,
            });
            updates.extend(misc_updates);
        }
        // Send all updates
        if !updates.is_empty() {
            let batch_event = OccasionalMatrixUpdateBatch {
                session_id: matrix_id.to_string(),
                value: updates,
            };
            let _ = sender.send(batch_event);
        }
    }

    fn send_occasional_track_updates_caused_by_matrix(
        &self,
        matrix_id: &str,
        events: &[ClipMatrixEvent],
    ) {
        let sender = &self.senders.occasional_track_update_sender;
        if sender.receiver_count() == 0 {
            return;
        }
        fn track_update(
            track: &Track,
            update: occasional_track_update::Update,
        ) -> QualifiedOccasionalTrackUpdate {
            QualifiedOccasionalTrackUpdate {
                track_id: track.guid().to_string_without_braces(),
                track_updates: vec![OccasionalTrackUpdate {
                    update: Some(update),
                }],
            }
        }
        let mut updates = vec![];
        for e in events {
            if let ClipMatrixEvent::TrackChanged(track) = e {
                let input_props = PlaytimeTrackInputProps::from_reaper_track(track);
                let arm_update = track_update(
                    track,
                    occasional_track_update::Update::armed(input_props.armed),
                );
                let input_monitoring_update = track_update(
                    track,
                    occasional_track_update::Update::input_monitoring(input_props.input_monitoring),
                );
                let input_update = track_update(
                    track,
                    occasional_track_update::Update::input(track.recording_input()),
                );
                let color_update =
                    track_update(track, occasional_track_update::Update::color(track));
                updates.push(arm_update);
                updates.push(input_monitoring_update);
                updates.push(input_update);
                updates.push(color_update);
            }
        }
        // Send all updates
        if !updates.is_empty() {
            let batch_event = OccasionalTrackUpdateBatch {
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
                            qualified_occasional_clip_update::Update::complete_persistent_data(clip)
                                .ok()?
                        }
                        Content => {
                            let clip = matrix.find_clip(*clip_address)?;
                            qualified_occasional_clip_update::Update::content_info(clip)
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
        events: &[ChangeEvent],
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
        // Handle batches of events
        let track_list_might_have_changed = events.iter().any(|evt| {
            matches!(
                evt,
                ChangeEvent::TrackAdded(_)
                    | ChangeEvent::TrackRemoved(_)
                    | ChangeEvent::TracksReordered(_)
            )
        });
        if track_list_might_have_changed {
            let update = occasional_matrix_update::Update::track_list(matrix.temporary_project());
            let _ = matrix_update_sender.send(OccasionalMatrixUpdateBatch {
                session_id: matrix_id.to_string(),
                value: vec![OccasionalMatrixUpdate {
                    update: Some(update),
                }],
            });
        }
        // Handle single events
        for event in events {
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
                    column_track_update(matrix, &e.track, || {
                        let input_props = PlaytimeTrackInputProps::from_reaper_track(&e.track);
                        Update::input_monitoring(input_props.input_monitoring)
                    })
                }
                ChangeEvent::TrackArmChanged(e) => column_track_update(matrix, &e.track, || {
                    let input_props = PlaytimeTrackInputProps::from_reaper_track(&e.track);
                    Update::armed(input_props.armed)
                }),
                ChangeEvent::TrackMuteChanged(e) => {
                    if e.track.is_master_track() {
                        Some(R::Matrix(occasional_matrix_update::Update::mute(
                            e.new_value,
                        )))
                    } else {
                        column_track_update(matrix, &e.track, || Update::mute(e.new_value))
                    }
                }
                ChangeEvent::TrackSoloChanged(e) => {
                    column_track_update(matrix, &e.track, || Update::solo(e.new_value))
                }
                ChangeEvent::TrackSelectedChanged(e) => {
                    column_track_update(matrix, &e.track, || Update::selected(e.new_value))
                }
                ChangeEvent::PlayStateChanged(e) => Some(R::Matrix(
                    occasional_matrix_update::Update::arrangement_play_state(e.new_value),
                )),
                ChangeEvent::MasterTempoChanged(e) => Some(R::Matrix(
                    occasional_matrix_update::Update::tempo(e.new_value),
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
                    event: SlotChangeEvent::Continuous(clip_events),
                }) = event
                {
                    Some(QualifiedContinuousSlotUpdate {
                        slot_address: Some(SlotAddress::from_engine(*slot_coordinates)),
                        update: Some(ContinuousSlotUpdate::from_engine(clip_events)),
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
        // TODO-high-ms4 We are mainly interested in beats relative to the bar in order to get a
        //  typical position display and a useful visual metronome. It's not super urgent because
        //  at least with the steady timeline, the app can derive a good metronome view because
        //  of the way the steady timeline is constructed. However, it gets wrong as soon as we
        //  use the REAPER timeline *and* use tempo markers. But this needs more attention anyway.
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
            .columns()
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
    if peak_util::peaks_should_be_hidden(track) {
        return vec![];
    }
    peak_util::get_track_peaks(track.raw())
        .map(|vol| Volume::from_reaper_value(vol).db().get())
        .collect()
}