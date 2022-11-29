use crate::infrastructure::ui::{
    bindings::root, util, HeaderPanel, IndependentPanelManager, MappingRowsPanel,
    SharedIndependentPanelManager, SharedMainState,
};

use lazycell::LazyCell;
use reaper_high::{AvailablePanValue, ChangeEvent, Guid, OrCurrentProject, Reaper, Track};

use slog::debug;
use std::cell::{Cell, RefCell};

use crate::application::{
    get_virtual_fx_label, get_virtual_track_label, Affected, CompartmentProp, Session, SessionProp,
    SessionUi, VirtualFxType, WeakSession,
};
use crate::base::when;
use crate::domain::ui_util::format_tags_as_csv;
use crate::domain::{
    Compartment, MappingId, MappingMatchedEvent, PanExt, ProjectionFeedbackValue,
    QualifiedMappingId, RealearnClipMatrix, SoundPlayer, TargetControlEvent,
    TargetValueChangedEvent,
};
use crate::infrastructure::plugin::{App, RealearnPluginParameters};
use crate::infrastructure::server::grpc::{
    ContinuousColumnUpdateBatch, ContinuousMatrixUpdateBatch, ContinuousSlotUpdateBatch,
    OccasionalMatrixUpdateBatch, OccasionalSlotUpdateBatch, OccasionalTrackUpdateBatch,
};
use crate::infrastructure::server::http::{
    send_projection_feedback_to_subscribed_clients, send_updated_controller_routing,
};
use crate::infrastructure::ui::util::{header_panel_height, parse_tags_from_csv};
use playtime_api::persistence::EvenQuantization;
use playtime_clip_engine::base::ClipMatrixEvent;
use playtime_clip_engine::proto::{
    occasional_matrix_update, occasional_track_update, qualified_occasional_slot_update,
    ArrangementPlayState, ContinuousClipUpdate, ContinuousColumnUpdate, ContinuousMatrixUpdate,
    ContinuousSlotUpdate, OccasionalMatrixUpdate, OccasionalTrackUpdate,
    QualifiedContinuousSlotUpdate, QualifiedOccasionalSlotUpdate, QualifiedOccasionalTrackUpdate,
    SlotCoordinates, SlotPlayState, TrackInput, TrackInputMonitoring,
};
use playtime_clip_engine::rt::{ClipChangeEvent, QualifiedClipChangeEvent};
use playtime_clip_engine::{clip_timeline, Laziness, Timeline};
use reaper_medium::TrackAttributeKey;
use rxrust::prelude::*;
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::sync;
use swell_ui::{DialogUnits, Dimensions, Pixels, Point, SharedView, View, ViewContext, Window};

/// The complete ReaLearn panel containing everything.
// TODO-low Maybe call this SessionPanel
#[derive(Debug)]
pub struct MainPanel {
    view: ViewContext,
    active_data: LazyCell<ActiveData>,
    dimensions: Cell<Option<Dimensions<Pixels>>>,
    state: SharedMainState,
    plugin_parameters: sync::Weak<RealearnPluginParameters>,
}

#[derive(Debug)]
struct ActiveData {
    session: WeakSession,
    header_panel: SharedView<HeaderPanel>,
    mapping_rows_panel: SharedView<MappingRowsPanel>,
    panel_manager: SharedIndependentPanelManager,
    success_sound_player: Option<SoundPlayer>,
}

impl ActiveData {
    fn do_with_session<R>(&self, f: impl FnOnce(&Session) -> R) -> Result<R, &'static str> {
        match self.session.upgrade() {
            None => Err("session not available anymore"),
            Some(session) => Ok(f(&session.borrow())),
        }
    }
}

impl MainPanel {
    pub fn new(plugin_parameters: sync::Weak<RealearnPluginParameters>) -> Self {
        Self {
            view: Default::default(),
            active_data: LazyCell::new(),
            dimensions: None.into(),
            state: Default::default(),
            plugin_parameters,
        }
    }

    pub fn state(&self) -> &SharedMainState {
        &self.state
    }

    pub fn notify_session_is_available(self: Rc<Self>, session: WeakSession) {
        // Finally, the session is available. First, save its reference and create sub panels.
        let panel_manager = IndependentPanelManager::new(session.clone(), Rc::downgrade(&self));
        let panel_manager = Rc::new(RefCell::new(panel_manager));
        let active_data = ActiveData {
            session: session.clone(),
            header_panel: HeaderPanel::new(
                session.clone(),
                self.state.clone(),
                self.plugin_parameters.clone(),
                Rc::downgrade(&panel_manager),
            )
            .into(),
            mapping_rows_panel: MappingRowsPanel::new(
                session,
                Rc::downgrade(&panel_manager),
                self.state.clone(),
                Point::new(DialogUnits(0), header_panel_height()),
            )
            .into(),
            panel_manager,
            success_sound_player: {
                let mut sound_player = SoundPlayer::new();
                let path_to_file = App::realearn_sound_dir_path().join("high-click.mp3");
                if sound_player.load_file(&path_to_file).is_ok() {
                    Some(sound_player)
                } else {
                    None
                }
            },
        };
        self.active_data.fill(active_data).unwrap();
        // If the plug-in window is currently open, open the sub panels as well. Now we are talking!
        if let Some(window) = self.view.window() {
            self.open_sub_panels(window);
            self.invalidate_all_controls();
            self.register_session_listeners();
        }
    }

    pub fn dimensions(&self) -> Dimensions<Pixels> {
        self.dimensions
            .get()
            .unwrap_or_else(|| util::main_panel_dimensions().in_pixels())
    }

    pub fn open_with_resize(self: SharedView<Self>, parent_window: Window) {
        #[cfg(target_family = "windows")]
        {
            // On Windows, the first time opening the dialog window is just to determine the best
            // dimensions based on HiDPI settings.
            // TODO-low If we skip this, the dimensions would be saved. Wouldn't that be better?
            //  I guess if there are multiple screens, keeping this line is better. Then it's a
            //  matter of reopening the GUI to improve scaling. Test it!
            self.dimensions.replace(None);
        }
        self.open(parent_window)
    }

    pub fn force_scroll_to_mapping(&self, id: QualifiedMappingId) {
        if let Some(data) = self.active_data.borrow() {
            data.mapping_rows_panel.force_scroll_to_mapping(id);
        }
    }

    pub fn edit_mapping(&self, compartment: Compartment, mapping_id: MappingId) {
        if let Some(data) = self.active_data.borrow() {
            data.mapping_rows_panel
                .edit_mapping(compartment, mapping_id);
        }
    }

    fn open_sub_panels(&self, window: Window) {
        if let Some(data) = self.active_data.borrow() {
            data.header_panel.clone().open(window);
            data.mapping_rows_panel.clone().open(window);
        }
    }

    fn invalidate_status_1_text(&self) {
        use std::fmt::Write;
        let _ = self.do_with_session(|session| {
            let state = self.state.borrow();
            let scroll_status = state.scroll_status.get_ref();
            let tags = session.tags.get_ref();
            let mut text = format!(
                "Showing mappings {} to {} of {} | Session ID: {}",
                scroll_status.from_pos,
                scroll_status.to_pos,
                scroll_status.item_count,
                session.id()
            );
            if !tags.is_empty() {
                let _ = write!(&mut text, " | Instance tags: {}", format_tags_as_csv(tags));
            }
            self.view
                .require_control(root::ID_MAIN_PANEL_STATUS_1_TEXT)
                .set_text(text.as_str());
        });
    }

    fn invalidate_status_2_text(&self) {
        use std::fmt::Write;
        let _ = self.do_with_session(|session| {
            let instance_state = session.instance_state().borrow();
            let instance_track = instance_state.instance_track_descriptor();
            let compartment = Compartment::Main;
            let instance_track_label = get_virtual_track_label(
                &instance_track.track,
                compartment,
                session.extended_context(),
            );
            let instance_fx = instance_state.instance_fx_descriptor();
            let instance_fx_label =
                get_virtual_fx_label(instance_fx, compartment, session.extended_context());
            let mut text = format!(
                "Track: {:.20} | FX: {:.30}",
                instance_track_label, instance_fx_label
            );
            let fx_type = VirtualFxType::from_virtual_fx(&instance_fx.fx);
            if fx_type.requires_fx_chain() {
                let instance_fx_track_label = get_virtual_track_label(
                    &instance_fx.track_descriptor.track,
                    compartment,
                    session.extended_context(),
                );
                let _ = write!(&mut text, " (on track {:.15})", instance_fx_track_label);
            }
            let control_and_feedback_state = instance_state.global_control_and_feedback_state();
            if !control_and_feedback_state.control_active {
                text.push_str(" | CONTROL off");
            }
            if !control_and_feedback_state.feedback_active {
                text.push_str(" | FEEDBACK off");
            }
            let label = self.view.require_control(root::ID_MAIN_PANEL_STATUS_2_TEXT);
            label.disable();
            label.set_text(text.as_str());
        });
    }

    fn do_with_session<R>(&self, f: impl FnOnce(&Session) -> R) -> Result<R, &'static str> {
        match self.active_data.borrow() {
            None => Err("session not available"),
            Some(active_data) => active_data.do_with_session(f),
        }
    }

    fn do_with_session_mut<R>(&self, f: impl FnOnce(&mut Session) -> R) -> Result<R, &'static str> {
        if let Some(data) = self.active_data.borrow() {
            if let Some(session) = data.session.upgrade() {
                return Ok(f(&mut session.borrow_mut()));
            }
        }
        Err("session not available")
    }

    fn invalidate_version_text(&self) {
        self.view
            .require_control(root::ID_MAIN_PANEL_VERSION_TEXT)
            .set_text(format!("ReaLearn {}", App::detailed_version_label()));
    }

    fn invalidate_all_controls(&self) {
        self.invalidate_version_text();
        self.invalidate_status_1_text();
        self.invalidate_status_2_text();
    }

    fn register_listeners(self: SharedView<Self>) {
        let state = self.state.borrow();
        self.when(state.scroll_status.changed(), |view| {
            view.invalidate_status_1_text();
        });
        self.register_session_listeners();
    }

    fn register_session_listeners(self: &SharedView<Self>) {
        let _ = self.do_with_session(|session| {
            self.when(session.everything_changed(), |view| {
                view.invalidate_all_controls();
            });
            self.when(session.tags.changed().merge(session.id.changed()), |view| {
                view.invalidate_status_1_text();
            });
            let instance_state = session.instance_state().borrow();
            self.when(
                instance_state.global_control_and_feedback_state_changed(),
                |view| {
                    view.invalidate_status_2_text();
                },
            );
        });
    }

    fn handle_changed_target_value(&self, event: TargetValueChangedEvent) {
        if let Some(data) = self.active_data.borrow() {
            data.panel_manager
                .borrow()
                .handle_changed_target_value(event);
        }
    }

    fn handle_matched_mapping(&self, event: MappingMatchedEvent) {
        if let Some(data) = self.active_data.borrow() {
            data.panel_manager.borrow().handle_matched_mapping(event);
            if self.is_open() {
                data.mapping_rows_panel.handle_matched_mapping(event);
            }
        }
    }

    fn handle_target_control_event(&self, event: TargetControlEvent) {
        if let Some(data) = self.active_data.borrow() {
            data.panel_manager
                .borrow()
                .handle_target_control_event(event);
        }
    }

    fn handle_affected(
        self: SharedView<Self>,
        affected: Affected<SessionProp>,
        initiator: Option<u32>,
    ) {
        if let Some(data) = self.active_data.borrow() {
            data.panel_manager
                .borrow()
                .handle_affected(&affected, initiator);
            data.mapping_rows_panel
                .handle_affected(&affected, initiator);
            data.header_panel.handle_affected(&affected, initiator);
        }
        self.handle_affected_own(affected);
    }

    fn handle_affected_own(self: SharedView<Self>, affected: Affected<SessionProp>) {
        use Affected::*;
        use SessionProp::*;
        if !self.is_open() {
            return;
        }
        match affected {
            One(InstanceTrack | InstanceFx) | Multiple => {
                self.invalidate_status_2_text();
            }
            _ => {}
        }
    }

    fn handle_changed_parameters(&self, session: &Session) {
        if let Some(data) = self.active_data.borrow() {
            data.panel_manager
                .borrow()
                .handle_changed_parameters(session);
        }
    }

    fn celebrate_success(&self) {
        if let Some(data) = self.active_data.borrow() {
            if let Some(s) = &data.success_sound_player {
                let _ = s.play();
            }
        }
    }

    fn handle_changed_midi_devices(&self) {
        if let Some(data) = self.active_data.borrow() {
            data.header_panel.handle_changed_midi_devices();
        }
    }

    fn handle_changed_conditions(&self) {
        if let Some(data) = self.active_data.borrow() {
            data.panel_manager.borrow().handle_changed_conditions();
            if self.is_open() {
                data.mapping_rows_panel.handle_changed_conditions();
            }
        }
    }

    fn edit_instance_data(&self) -> Result<(), &'static str> {
        let (initial_session_id, initial_tags_as_csv) = self.do_with_session(|session| {
            (
                session.id().to_owned(),
                format_tags_as_csv(session.tags.get_ref()),
            )
        })?;
        let initial_csv = format!("{}|{}", initial_session_id, initial_tags_as_csv);
        // Show UI
        let csv_result = Reaper::get().medium_reaper().get_user_inputs(
            "ReaLearn",
            2,
            "Session ID,Tags,separator=|,extrawidth=200",
            initial_csv,
            512,
        );
        // Check if cancelled
        let csv = match csv_result {
            // Cancelled
            None => return Ok(()),
            Some(csv) => csv,
        };
        // Parse result CSV
        let split: Vec<_> = csv.to_str().split('|').collect();
        let (session_id, tags_as_csv) = match split.as_slice() {
            [session_id, tags_as_csv] => (session_id, tags_as_csv),
            _ => return Err("couldn't split result"),
        };
        // Take care of tags
        if tags_as_csv != &initial_tags_as_csv {
            let tags = parse_tags_from_csv(tags_as_csv);
            self.do_with_session_mut(|session| {
                session.tags.set(tags);
            })?;
        }
        // Take care of session ID
        // TODO-low Introduce a SessionId newtype.
        let new_session_id = session_id.replace(|ch| !nanoid::alphabet::SAFE.contains(&ch), "");
        if !new_session_id.is_empty() && new_session_id != initial_session_id {
            if App::get().has_session(&new_session_id) {
                self.view.require_window().alert(
                    "ReaLearn",
                    "There's another open ReaLearn session which already has this session ID!",
                );
                return Ok(());
            }
            self.do_with_session_mut(|session| {
                session.id.set(new_session_id);
            })?;
        }
        Ok(())
    }

    fn when(
        self: &SharedView<Self>,
        event: impl LocalObservable<'static, Item = (), Err = ()> + 'static,
        reaction: impl Fn(SharedView<Self>) + 'static + Copy,
    ) {
        when(event.take_until(self.view.closed()))
            .with(Rc::downgrade(self))
            .do_async(move |panel, _| reaction(panel));
    }
}

impl View for MainPanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_MAIN_PANEL
    }

    fn view_context(&self) -> &ViewContext {
        &self.view
    }

    fn opened(self: SharedView<Self>, window: Window) -> bool {
        #[cfg(target_family = "windows")]
        if self.dimensions.get().is_none() {
            // The dialog has been opened by user request but the optimal dimensions have not yet
            // been figured out. Figure them out now.
            self.dimensions.replace(Some(
                window.convert_to_pixels(util::main_panel_dimensions()),
            ));
            // Close and reopen window, this time with `dimensions()` returning the optimal size to
            // the host.
            let parent_window = window.parent().expect("must have parent");
            window.destroy();
            self.open(parent_window);
            return false;
        }
        // Optimal dimensions have been calculated and window has been reopened. Now add sub panels!
        self.open_sub_panels(window);
        self.invalidate_all_controls();
        self.register_listeners();
        true
    }

    #[allow(clippy::single_match)]
    fn button_clicked(self: SharedView<Self>, resource_id: u32) {
        match resource_id {
            root::IDC_EDIT_TAGS_BUTTON => {
                self.edit_instance_data().unwrap();
            }
            _ => {}
        }
    }
}

impl SessionUi for Weak<MainPanel> {
    fn show_mapping(&self, compartment: Compartment, mapping_id: MappingId) {
        upgrade_panel(self).edit_mapping(compartment, mapping_id);
    }

    fn target_value_changed(&self, event: TargetValueChangedEvent) {
        upgrade_panel(self).handle_changed_target_value(event);
    }

    fn parameters_changed(&self, session: &Session) {
        upgrade_panel(self).handle_changed_parameters(session);
    }

    fn midi_devices_changed(&self) {
        upgrade_panel(self).handle_changed_midi_devices();
    }

    fn conditions_changed(&self) {
        upgrade_panel(self).handle_changed_conditions();
    }

    fn celebrate_success(&self) {
        upgrade_panel(self).celebrate_success();
    }

    fn send_projection_feedback(&self, session: &Session, value: ProjectionFeedbackValue) {
        let _ = send_projection_feedback_to_subscribed_clients(session.id(), value);
    }

    fn clip_matrix_changed(
        &self,
        session: &Session,
        matrix: &RealearnClipMatrix,
        events: &[ClipMatrixEvent],
        is_poll: bool,
    ) {
        send_occasional_matrix_updates(session, matrix, events);
        send_occasional_slot_updates(session, matrix, events);
        send_continuous_slot_updates(session, events);
        if is_poll {
            send_continuous_matrix_updates(session);
            send_continuous_column_updates(session, matrix);
        }
    }

    fn process_control_surface_change_event_for_clip_engine(
        &self,
        session: &Session,
        matrix: &RealearnClipMatrix,
        event: &ChangeEvent,
    ) {
        send_occasional_matrix_track_updates(session, matrix, event);
    }

    fn mapping_matched(&self, event: MappingMatchedEvent) {
        upgrade_panel(self).handle_matched_mapping(event);
    }

    fn target_controlled(&self, event: TargetControlEvent) {
        upgrade_panel(self).handle_target_control_event(event);
    }

    #[allow(clippy::single_match)]
    fn handle_affected(
        &self,
        session: &Session,
        affected: Affected<SessionProp>,
        initiator: Option<u32>,
    ) {
        // Update secondary GUIs (e.g. Projection)
        use Affected::*;
        use CompartmentProp::*;
        use SessionProp::*;
        match &affected {
            One(InCompartment(_, One(InMapping(_, _)))) => {
                let _ = send_updated_controller_routing(session);
            }
            _ => {}
        }
        // Update primary GUI
        upgrade_panel(self).handle_affected(affected, initiator);
    }
}

fn send_occasional_matrix_updates(
    session: &Session,
    matrix: &RealearnClipMatrix,
    events: &[ClipMatrixEvent],
) {
    let sender = App::get().occasional_matrix_update_sender();
    if sender.receiver_count() == 0 {
        return;
    }
    // TODO-high-playtime-performance Push persistent matrix state only once (even if many events)
    let updates: Vec<_> = events
        .iter()
        .filter_map(|event| {
            if let ClipMatrixEvent::AllClipsChanged = event {
                let json = serde_json::to_string(&matrix.save())
                    .expect("couldn't serialize clip matrix as JSON");
                Some(OccasionalMatrixUpdate {
                    update: Some(occasional_matrix_update::Update::PersistentState(json)),
                })
            } else {
                None
            }
        })
        .collect();
    if !updates.is_empty() {
        let batch_event = OccasionalMatrixUpdateBatch {
            session_id: session.id().to_owned(),
            value: updates,
        };
        let _ = sender.send(batch_event);
    }
}

fn send_occasional_slot_updates(
    session: &Session,
    matrix: &RealearnClipMatrix,
    events: &[ClipMatrixEvent],
) {
    let sender = App::get().occasional_slot_update_sender();
    if sender.receiver_count() == 0 {
        return;
    }
    let updates: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            ClipMatrixEvent::ClipChanged(QualifiedClipChangeEvent {
                slot_coordinates,
                event,
            }) => {
                use ClipChangeEvent::*;
                let update = match event {
                    PlayState(play_state) => qualified_occasional_slot_update::Update::PlayState(
                        SlotPlayState::from_engine(play_state.get()).into(),
                    ),
                    ClipVolume(_) | ClipLooped(_) | Removed | RecordingFinished => {
                        let slot = matrix.slot(*slot_coordinates)?;
                        let api_slot = slot.save(session.processor_context().project()).unwrap_or(
                            playtime_api::persistence::Slot {
                                row: slot_coordinates.row(),
                                clip: None,
                            },
                        );
                        let json = serde_json::to_string(&api_slot).unwrap();
                        qualified_occasional_slot_update::Update::PersistentState(json)
                    }
                    ClipPosition { .. } => return None,
                };
                Some(QualifiedOccasionalSlotUpdate {
                    slot_coordinates: Some(SlotCoordinates::from_engine(*slot_coordinates)),
                    update: Some(update),
                })
            }
            _ => None,
        })
        .collect();
    if !updates.is_empty() {
        let batch_event = OccasionalSlotUpdateBatch {
            session_id: session.id().to_owned(),
            value: updates,
        };
        let _ = sender.send(batch_event);
    }
}

fn send_occasional_matrix_track_updates(
    session: &Session,
    matrix: &RealearnClipMatrix,
    event: &ChangeEvent,
) {
    use occasional_track_update::Update;
    enum R {
        Matrix(occasional_matrix_update::Update),
        Track(QualifiedOccasionalTrackUpdate),
    }
    let matrix_update_sender = App::get().occasional_matrix_update_sender();
    let track_update_sender = App::get().occasional_track_update_sender();
    if matrix_update_sender.receiver_count() == 0 && track_update_sender.receiver_count() == 0 {
        return;
    }
    fn track_update(
        matrix: &RealearnClipMatrix,
        track: &Track,
        create_update: impl FnOnce() -> Update,
    ) -> Option<R> {
        if matrix.uses_playback_track(track) {
            Some(R::Track(QualifiedOccasionalTrackUpdate {
                track_id: track.guid().to_string_without_braces(),
                track_updates: vec![OccasionalTrackUpdate {
                    update: Some(create_update()),
                }],
            }))
        } else {
            None
        }
    }
    let update: Option<R> = match event {
        ChangeEvent::TrackVolumeChanged(e) => {
            track_update(matrix, &e.track, || Update::Volume(e.new_value.get()))
        }
        ChangeEvent::TrackPanChanged(e) => track_update(matrix, &e.track, || {
            let api_value = match e.new_value {
                AvailablePanValue::Complete(v) => v.main_pan().get(),
                AvailablePanValue::Incomplete(v) => v.get(),
            };
            Update::Pan(api_value)
        }),
        ChangeEvent::TrackNameChanged(e) => track_update(matrix, &e.track, || {
            Update::Name(e.track.name().unwrap_or_default().into_string())
        }),
        ChangeEvent::TrackInputChanged(e) => track_update(matrix, &e.track, || {
            Update::Input(TrackInput::from_engine(e.new_value))
        }),
        ChangeEvent::TrackInputMonitoringChanged(e) => track_update(matrix, &e.track, || {
            Update::InputMonitoring(TrackInputMonitoring::from_engine(e.new_value).into())
        }),
        ChangeEvent::TrackArmChanged(e) => {
            track_update(matrix, &e.track, || Update::Armed(e.new_value))
        }
        ChangeEvent::TrackMuteChanged(e) => {
            track_update(matrix, &e.track, || Update::Mute(e.new_value))
        }
        ChangeEvent::TrackSoloChanged(e) => {
            track_update(matrix, &e.track, || Update::Solo(e.new_value))
        }
        ChangeEvent::TrackSelectedChanged(e) => {
            track_update(matrix, &e.track, || Update::Selected(e.new_value))
        }
        ChangeEvent::MasterTempoChanged(e) => Some(R::Matrix(
            occasional_matrix_update::Update::Tempo(e.new_value.get()),
        )),
        ChangeEvent::PlayStateChanged(e) => Some(R::Matrix(
            occasional_matrix_update::Update::ArrangementPlayState(
                ArrangementPlayState::from_engine(e.new_value).into(),
            ),
        )),
        _ => None,
    };
    if let Some(update) = update {
        match update {
            R::Matrix(u) => {
                let _ = matrix_update_sender.send(OccasionalMatrixUpdateBatch {
                    session_id: session.id().to_owned(),
                    value: vec![OccasionalMatrixUpdate { update: Some(u) }],
                });
            }
            R::Track(u) => {
                let _ = track_update_sender.send(OccasionalTrackUpdateBatch {
                    session_id: session.id().to_owned(),
                    value: vec![u],
                });
            }
        }
    }
}

fn send_continuous_slot_updates(session: &Session, events: &[ClipMatrixEvent]) {
    let sender = App::get().continuous_slot_update_sender();
    if sender.receiver_count() == 0 {
        return;
    }
    let updates: Vec<_> = events
        .iter()
        .filter_map(|event| {
            if let ClipMatrixEvent::ClipChanged(QualifiedClipChangeEvent {
                slot_coordinates,
                event:
                    ClipChangeEvent::ClipPosition {
                        proportional,
                        seconds,
                    },
            }) = event
            {
                Some(QualifiedContinuousSlotUpdate {
                    slot_coordinates: Some(SlotCoordinates::from_engine(*slot_coordinates)),
                    update: Some(ContinuousSlotUpdate {
                        clip_update: Some(ContinuousClipUpdate {
                            proportional_position: proportional.get(),
                            position_in_seconds: seconds.get(),
                            peak: 0.0,
                        }),
                    }),
                })
            } else {
                None
            }
        })
        .collect();
    if !updates.is_empty() {
        let batch_event = ContinuousSlotUpdateBatch {
            session_id: session.id().to_owned(),
            value: updates,
        };
        let _ = sender.send(batch_event);
    }
}

fn send_continuous_matrix_updates(session: &Session) {
    let sender = App::get().continuous_matrix_update_sender();
    if sender.receiver_count() == 0 {
        return;
    }
    let project = session.processor_context().project();
    let timeline = clip_timeline(project, false);
    let pos = timeline.cursor_pos();
    let bar_quantization = EvenQuantization::ONE_BAR;
    let next_bar = timeline.next_quantized_pos_at(pos, bar_quantization, Laziness::EagerForNextPos);
    // TODO-high-playtime CONTINUE We are mainly interested in beats relative to the bar in order to get a
    //  typical position display and a useful visual metronome!
    let full_beats = timeline.full_beats_at_pos(pos);
    let batch_event = ContinuousMatrixUpdateBatch {
        session_id: session.id().to_owned(),
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

fn send_continuous_column_updates(session: &Session, matrix: &RealearnClipMatrix) {
    let sender = App::get().continuous_column_update_sender();
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
            session_id: session.id().to_owned(),
            value: column_updates,
        };
        let _ = sender.send(batch_event);
    }
}

fn upgrade_panel(panel: &Weak<MainPanel>) -> Rc<MainPanel> {
    panel.upgrade().expect("main panel not existing anymore")
}

impl Drop for MainPanel {
    fn drop(&mut self) {
        debug!(Reaper::get().logger(), "Dropping main panel...");
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
    (0..channel_count)
        .map(|ch| {
            let volume = unsafe { reaper.track_get_peak_info(track, ch as u32 + 1024) };
            volume.get()
        })
        .collect()
}
