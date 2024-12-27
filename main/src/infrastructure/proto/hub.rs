use reaper_high::ChangeEvent;

use helgobox_api::runtime::{GlobalInfoEvent, InstanceInfoEvent};

use crate::application::UnitModel;
use crate::domain::{InstanceId, UnitId};
use crate::infrastructure::data::{
    ControllerManager, FileBasedControllerPresetManager, FileBasedMainPresetManager, LicenseManager,
};
use crate::infrastructure::plugin::InstanceShell;
use crate::infrastructure::proto::helgobox_service_server::HelgoboxServiceServer;
use crate::infrastructure::proto::{
    occasional_global_update, occasional_instance_update, qualified_occasional_unit_update,
    HelgoboxServiceImpl, OccasionalGlobalUpdate, OccasionalInstanceUpdate,
    OccasionalInstanceUpdateBatch, OccasionalUnitUpdateBatch, ProtoRequestHandler, ProtoSenders,
    QualifiedOccasionalUnitUpdate,
};

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

    pub fn create_service(&self) -> HelgoboxServiceServer<HelgoboxServiceImpl> {
        HelgoboxServiceServer::new(HelgoboxServiceImpl::new(
            ProtoRequestHandler,
            self.senders.clone(),
        ))
    }

    pub fn notify_about_instance_info_event(
        &self,
        instance_id: InstanceId,
        info_event: InstanceInfoEvent,
    ) {
        self.send_occasional_instance_updates(instance_id, || {
            [occasional_instance_update::Update::info_event(info_event)]
        });
    }

    pub fn notify_about_global_info_event(&self, info_event: GlobalInfoEvent) {
        self.send_occasional_global_updates(|| {
            [occasional_global_update::Update::info_event(info_event)]
        });
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

    pub fn notify_licenses_changed(&self, license_manager: &LicenseManager) {
        self.send_occasional_global_updates(|| {
            [occasional_global_update::Update::license_info(
                license_manager,
            )]
        });
        #[cfg(feature = "playtime")]
        self.send_occasional_playtime_engine_updates(|| {
            [crate::infrastructure::proto::occasional_playtime_engine_update::Update::playtime_license_state()]
        });
    }

    pub fn notify_controller_routing_changed(&self, unit_model: &UnitModel) {
        let unit_id = unit_model.unit_id();
        self.send_occasional_unit_updates(unit_model.instance_id(), || {
            [QualifiedOccasionalUnitUpdate {
                unit_id: unit_id.into(),
                update: Some(
                    qualified_occasional_unit_update::Update::controller_routing(unit_model),
                ),
            }]
        })
    }

    pub fn notify_controller_config_changed(&self, controller_manager: &ControllerManager) {
        self.send_occasional_global_updates(|| {
            [occasional_global_update::Update::controller_config(
                controller_manager,
            )]
        });
    }

    pub fn send_global_events_caused_by_reaper_change_events(&self, change_events: &[ChangeEvent]) {
        self.send_occasional_global_updates(|| {
            change_events.iter().filter_map(|event| match event {
                ChangeEvent::PlayStateChanged(e) => Some(
                    occasional_global_update::Update::arrangement_play_state(e.new_value),
                ),
                _ => None,
            })
        })
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

    fn send_occasional_instance_updates<F, I>(&self, instance_id: InstanceId, create_updates: F)
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
            instance_id,
            value: wrapped_updates.collect(),
        };
        let _ = sender.send(batch_event);
    }

    fn send_occasional_unit_updates<F, I>(&self, instance_id: InstanceId, create_updates: F)
    where
        F: FnOnce() -> I,
        I: IntoIterator<Item = QualifiedOccasionalUnitUpdate>,
    {
        let sender = &self.senders.occasional_unit_update_sender;
        if sender.receiver_count() == 0 {
            return;
        }
        let batch_event = OccasionalUnitUpdateBatch {
            instance_id,
            value: create_updates().into_iter().collect(),
        };
        let _ = sender.send(batch_event);
    }

    pub fn notify_instance_units_changed(&self, instance_shell: &InstanceShell) {
        self.send_occasional_instance_updates(instance_shell.instance_id(), || {
            [occasional_instance_update::Update::units(instance_shell)]
        });
    }

    pub fn notify_instance_settings_changed(&self, instance_shell: &InstanceShell) {
        self.send_occasional_instance_updates(instance_shell.instance_id(), || {
            [occasional_instance_update::Update::settings(instance_shell)]
        });
    }

    pub fn notify_everything_in_instance_has_changed(&self, instance_id: InstanceId) {
        self.send_occasional_instance_updates(instance_id, || {
            [occasional_instance_update::Update::EverythingHasChanged(
                true,
            )]
        });
    }

    pub fn notify_everything_in_unit_has_changed(&self, instance_id: InstanceId, unit_id: UnitId) {
        self.send_occasional_unit_updates(instance_id, || {
            [QualifiedOccasionalUnitUpdate {
                unit_id: unit_id.into(),
                update: Some(qualified_occasional_unit_update::Update::EverythingHasChanged(true)),
            }]
        });
    }
}

#[cfg(feature = "playtime")]
mod playtime_impl {
    use std::collections::HashMap;

    use reaper_high::{AvailablePanValue, ChangeEvent, Guid, PanExt, Project, Track};
    use reaper_medium::Db;

    use base::hash_util::NonCryptoHashMap;
    use base::peak_util;
    use playtime_api::persistence::EvenQuantization;
    use playtime_clip_engine::{
        base::{ClipMatrixEvent, Matrix, PlaytimeTrackInputProps},
        clip_timeline,
        rt::{
            ClipChangeEvent, QualifiedClipChangeEvent, QualifiedSlotChangeEvent, SlotChangeEvent,
        },
        Laziness, Timeline,
    };

    use crate::domain::InstanceId;
    use crate::infrastructure::proto;
    use crate::infrastructure::proto::senders::{
        ContinuousColumnUpdateBatch, ContinuousMatrixUpdateBatch, ContinuousSlotUpdateBatch,
        OccasionalClipUpdateBatch, OccasionalColumnUpdateBatch, OccasionalMatrixUpdateBatch,
        OccasionalRowUpdateBatch, OccasionalSlotUpdateBatch, OccasionalTrackUpdateBatch,
    };
    use crate::infrastructure::proto::{
        occasional_matrix_update, occasional_track_update, qualified_occasional_clip_update,
        qualified_occasional_column_update, qualified_occasional_row_update,
        qualified_occasional_slot_update, ContinuousColumnUpdate, ContinuousMatrixUpdate,
        ContinuousSlotUpdate, OccasionalMatrixUpdate, OccasionalTrackUpdate,
        QualifiedContinuousSlotUpdate, QualifiedOccasionalClipUpdate,
        QualifiedOccasionalColumnUpdate, QualifiedOccasionalRowUpdate,
        QualifiedOccasionalSlotUpdate, QualifiedOccasionalTrackUpdate, SlotAddress,
    };
    use crate::infrastructure::proto::{
        occasional_playtime_engine_update, OccasionalPlaytimeEngineUpdate, ProtoHub,
    };

    impl ProtoHub {
        pub fn notify_engine_stats_changed(&self) {
            self.send_occasional_playtime_engine_updates(|| {
                [occasional_playtime_engine_update::Update::engine_stats()]
            });
        }

        pub fn notify_playtime_settings_changed(&self) {
            self.send_occasional_playtime_engine_updates(|| {
                [occasional_playtime_engine_update::Update::engine_settings()]
            });
        }

        pub fn send_occasional_playtime_engine_updates<F, I>(&self, create_updates: F)
        where
            F: FnOnce() -> I,
            I: IntoIterator<Item = occasional_playtime_engine_update::Update>,
        {
            let sender = &self.senders.occasional_playtime_engine_update_sender;
            if sender.receiver_count() == 0 {
                return;
            }
            let wrapped_updates = create_updates()
                .into_iter()
                .map(|u| OccasionalPlaytimeEngineUpdate { update: Some(u) });
            let _ = sender.send(wrapped_updates.collect());
        }

        pub fn notify_clip_matrix_changed(
            &self,
            matrix_id: InstanceId,
            matrix: &Matrix,
            events: &[ClipMatrixEvent],
            is_poll: bool,
        ) {
            let project = matrix.project();
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

        fn send_occasional_matrix_updates_caused_by_matrix(
            &self,
            matrix_id: InstanceId,
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
                let changed_update = OccasionalMatrixUpdate {
                    update: Some(occasional_matrix_update::Update::EverythingHasChanged(true)),
                };
                updates.push(changed_update);
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
                    ClipMatrixEvent::ClickVolumeChanged => Some(OccasionalMatrixUpdate {
                        update: Some(occasional_matrix_update::Update::click_volume(matrix)),
                    }),
                    ClipMatrixEvent::TempoTapVolumeChanged => Some(OccasionalMatrixUpdate {
                        update: Some(occasional_matrix_update::Update::tempo_tap_volume(matrix)),
                    }),
                    ClipMatrixEvent::SilenceModeChanged => Some(OccasionalMatrixUpdate {
                        update: Some(occasional_matrix_update::Update::silence_mode(matrix)),
                    }),
                    ClipMatrixEvent::ActiveCellChanged => Some(OccasionalMatrixUpdate {
                        update: Some(occasional_matrix_update::Update::active_cell(matrix)),
                    }),
                    ClipMatrixEvent::ControlUnitsChanged => Some(OccasionalMatrixUpdate {
                        update: Some(occasional_matrix_update::Update::control_unit_config(
                            matrix,
                        )),
                    }),
                    ClipMatrixEvent::TimeSignatureChanged => Some(OccasionalMatrixUpdate {
                        update: Some(occasional_matrix_update::Update::time_signature(
                            matrix.project(),
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
                        update: Some(occasional_matrix_update::Update::info_event(evt.clone())),
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
                    instance_id: matrix_id,
                    value: updates,
                };
                let _ = sender.send(batch_event);
            }
        }

        fn send_occasional_track_updates_caused_by_matrix(
            &self,
            matrix_id: InstanceId,
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
                        occasional_track_update::Update::input_monitoring(
                            input_props.input_monitoring,
                        ),
                    );
                    let input_update =
                        track_update(track, occasional_track_update::Update::input(track));
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
                    instance_id: matrix_id,
                    value: updates,
                };
                let _ = sender.send(batch_event);
            }
        }

        fn send_occasional_column_updates(
            &self,
            matrix_id: InstanceId,
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
                        let update = qualified_occasional_column_update::Update::settings(
                            matrix,
                            *column_index,
                        )
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
                    instance_id: matrix_id,
                    value: updates,
                };
                let _ = sender.send(batch_event);
            }
        }

        fn send_occasional_row_updates(
            &self,
            matrix_id: InstanceId,
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
                            qualified_occasional_row_update::Update::data(matrix, *row_index)
                                .ok()?;
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
                    instance_id: matrix_id,
                    value: updates,
                };
                let _ = sender.send(batch_event);
            }
        }

        fn send_occasional_slot_updates(
            &self,
            matrix_id: InstanceId,
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
                    instance_id: matrix_id,
                    value: updates,
                };
                let _ = sender.send(batch_event);
            }
        }

        fn send_occasional_clip_updates(
            &self,
            matrix_id: InstanceId,
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
                        let employment = matrix.find_clip_employment(*clip_address)?;
                        let update = match event {
                            Everything | Volume(_) | Looped(_) => {
                                qualified_occasional_clip_update::Update::complete_persistent_data(
                                    matrix,
                                    &employment.clip,
                                )
                                .ok()?
                            }
                            Content => {
                                qualified_occasional_clip_update::Update::content_info(employment)
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
                    instance_id: matrix_id,
                    value: updates,
                };
                let _ = sender.send(batch_event);
            }
        }

        pub fn send_occasional_matrix_updates_caused_by_reaper(
            &self,
            matrix_id: InstanceId,
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
            if matrix_update_sender.receiver_count() == 0
                && track_update_sender.receiver_count() == 0
            {
                return;
            }
            fn track_normal_fx_chain_update(track: &Track) -> R {
                track_update(track, || Update::normal_fx_chain(track))
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
                let update = occasional_matrix_update::Update::track_list(matrix.project());
                let _ = matrix_update_sender.send(OccasionalMatrixUpdateBatch {
                    instance_id: matrix_id,
                    value: vec![OccasionalMatrixUpdate {
                        update: Some(update),
                    }],
                });
            }
            // Handle single events
            for event in events {
                let update: Option<R> = match event {
                    ChangeEvent::TrackVolumeChanged(e) => {
                        let db = e.new_value.to_db_ex(Db::MINUS_INF);
                        if e.track.is_master_track() {
                            Some(R::Matrix(occasional_matrix_update::Update::master_volume(
                                db,
                            )))
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
                    ChangeEvent::FxAdded(e) => e.fx.track().map(track_normal_fx_chain_update),
                    ChangeEvent::FxRemoved(e) => e.fx.track().map(track_normal_fx_chain_update),
                    ChangeEvent::FxReordered(e) => Some(track_normal_fx_chain_update(&e.track)),
                    ChangeEvent::TrackNameChanged(e) => {
                        Some(track_update(&e.track, || Update::name(&e.track)))
                    }
                    ChangeEvent::TrackInputChanged(e) => {
                        column_track_update(matrix, &e.track, || Update::input(&e.track))
                    }
                    ChangeEvent::TrackInputMonitoringChanged(e) => {
                        column_track_update(matrix, &e.track, || {
                            let input_props = PlaytimeTrackInputProps::from_reaper_track(&e.track);
                            Update::input_monitoring(input_props.input_monitoring)
                        })
                    }
                    ChangeEvent::TrackArmChanged(e) => {
                        column_track_update(matrix, &e.track, || {
                            let input_props = PlaytimeTrackInputProps::from_reaper_track(&e.track);
                            Update::armed(input_props.armed)
                        })
                    }
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
                    ChangeEvent::MasterTempoChanged(_) => {
                        // It's important not to take the given tempo value because REAPER doesn't really support
                        // project-specific tempo notifications. The change could come from any project. We query
                        // our matrix explicitly (and it might just as well turn out that the tempo for that matrix
                        // hasn't changed at all, but that's okay).
                        Some(R::Matrix(occasional_matrix_update::Update::tempo(matrix)))
                    }
                    ChangeEvent::MasterPlayRateChanged(_) => {
                        // It's the same with the play rate
                        Some(R::Matrix(occasional_matrix_update::Update::play_rate(
                            matrix,
                        )))
                    }
                    _ => None,
                };
                if let Some(update) = update {
                    match update {
                        R::Matrix(u) => {
                            let _ = matrix_update_sender.send(OccasionalMatrixUpdateBatch {
                                instance_id: matrix_id,
                                value: vec![OccasionalMatrixUpdate { update: Some(u) }],
                            });
                        }
                        R::Track(u) => {
                            let _ = track_update_sender.send(OccasionalTrackUpdateBatch {
                                instance_id: matrix_id,
                                value: vec![u],
                            });
                        }
                    }
                }
            }
        }

        fn send_continuous_slot_updates(&self, matrix_id: InstanceId, events: &[ClipMatrixEvent]) {
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
                    instance_id: matrix_id,
                    value: updates,
                };
                let _ = sender.send(batch_event);
            }
        }

        fn send_continuous_matrix_updates(&self, matrix_id: InstanceId, project: Project) {
            let sender = &self.senders.continuous_matrix_update_sender;
            if sender.receiver_count() == 0 {
                return;
            }
            let timeline = clip_timeline(project, false);
            let pos = timeline.cursor_pos().get();
            let bar_quantization = EvenQuantization::ONE_BAR;
            let next_bar = timeline.next_quantized_pos_at(
                pos,
                bar_quantization,
                Laziness::SuperEagerForNextPos,
            );
            // TODO-high-playtime-refactoring We are mainly interested in beats relative to the bar in order to get a
            //  typical position display and a useful visual metronome. It's not super urgent because
            //  at least with the steady timeline, the app can derive a good metronome view because
            //  of the way the steady timeline is constructed. However, it gets wrong as soon as we
            //  use the REAPER timeline *and* use tempo markers. But this needs more attention anyway.
            let full_beats = timeline.full_beats_at_pos(pos);
            let batch_event = ContinuousMatrixUpdateBatch {
                instance_id: matrix_id,
                value: ContinuousMatrixUpdate {
                    second: pos.get(),
                    bar: (next_bar.position() - 1) as i32,
                    beat: full_beats.get(),
                    peaks: project
                        .master_track()
                        .map(|t| get_track_peaks(&t))
                        .unwrap_or_default(),
                },
            };
            let _ = sender.send(batch_event);
        }

        fn send_continuous_column_updates(&self, matrix_id: InstanceId, matrix: &Matrix) {
            let sender = &self.senders.continuous_column_update_sender;
            if sender.receiver_count() == 0 {
                return;
            }
            let column_update_by_track_guid: NonCryptoHashMap<Guid, ContinuousColumnUpdate> =
                HashMap::with_capacity_and_hasher(matrix.column_count(), Default::default());
            let column_updates: Vec<_> = matrix
                .columns()
                .map(|column| {
                    if let Ok(track) = column.playback_track() {
                        if let Some(existing_update) = column_update_by_track_guid.get(track.guid())
                        {
                            // We have already collected the update for this column's playback track.
                            existing_update.clone()
                        } else {
                            // We haven't yet collected the update for this column's playback track.
                            ContinuousColumnUpdate {
                                peaks: get_track_peaks(track),
                                play_lookahead: column.play_lookahead().get().get(),
                            }
                        }
                    } else {
                        ContinuousColumnUpdate {
                            peaks: vec![],
                            play_lookahead: 0.0,
                        }
                    }
                })
                .collect();
            if !column_updates.is_empty() {
                let batch_event = ContinuousColumnUpdateBatch {
                    instance_id: matrix_id,
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
        let Ok(track) = track.raw() else {
            return vec![];
        };
        peak_util::get_track_peaks(track)
            .map(|vol| vol.to_db_ex(Db::MINUS_INF).get())
            .collect()
    }
}
