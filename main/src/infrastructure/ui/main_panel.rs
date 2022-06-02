use crate::infrastructure::ui::{
    bindings::root, dialog_util, util, HeaderPanel, IndependentPanelManager, MappingRowsPanel,
    SharedIndependentPanelManager, SharedMainState,
};

use lazycell::LazyCell;
use reaper_high::{Guid, OrCurrentProject, Reaper, Track};

use slog::debug;
use std::cell::{Cell, RefCell};

use crate::application::{Affected, CompartmentProp, Session, SessionProp, SessionUi, WeakSession};
use crate::base::when;
use crate::domain::{
    Compartment, MappingId, MappingMatchedEvent, ProjectionFeedbackValue, RealearnClipMatrix,
    TargetValueChangedEvent,
};
use crate::infrastructure::plugin::{App, RealearnPluginParameters};
use crate::infrastructure::server::grpc::{
    ContinuousColumnUpdateBatch, ContinuousMatrixUpdateBatch, ContinuousSlotUpdateBatch,
    OccasionalSlotUpdateBatch,
};
use crate::infrastructure::server::http::{
    send_projection_feedback_to_subscribed_clients, send_updated_controller_routing,
};
use crate::infrastructure::ui::util::{format_tags_as_csv, parse_tags_from_csv};
use playtime_api::persistence::EvenQuantization;
use playtime_clip_engine::main::ClipMatrixEvent;
use playtime_clip_engine::proto::{
    qualified_occasional_slot_update, ContinuousClipUpdate, ContinuousColumnUpdate,
    ContinuousMatrixUpdate, ContinuousSlotUpdate, QualifiedContinuousSlotUpdate,
    QualifiedOccasionalSlotUpdate, SlotCoordinates, SlotPlayState,
};
use playtime_clip_engine::rt::{ClipChangeEvent, QualifiedClipChangeEvent};
use playtime_clip_engine::{clip_timeline, Laziness, Timeline};
use reaper_medium::TrackAttributeKey;
use rxrust::prelude::*;
use std::borrow::Cow;
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
                Point::new(DialogUnits(0), DialogUnits(124)),
            )
            .into(),
            panel_manager,
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
            .unwrap_or_else(|| util::MAIN_PANEL_DIMENSIONS.in_pixels())
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

    pub fn force_scroll_to_mapping(&self, mapping_id: MappingId) {
        if let Some(data) = self.active_data.borrow() {
            data.mapping_rows_panel.force_scroll_to_mapping(mapping_id);
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

    fn invalidate_status_text(&self) {
        self.do_with_session(|session| {
            let state = self.state.borrow();
            let status_msg = state.status_msg.get_ref();
            let tags = session.tags.get_ref();
            let text = if tags.is_empty() {
                Cow::Borrowed(status_msg)
            } else {
                Cow::Owned(format!(
                    "{} | Tags: {}",
                    status_msg,
                    format_tags_as_csv(tags)
                ))
            };
            self.view
                .require_control(root::ID_MAIN_PANEL_STATUS_TEXT)
                .set_text(text.as_str());
        });
    }

    fn do_with_session<R>(&self, f: impl FnOnce(&Session) -> R) -> Option<R> {
        if let Some(data) = self.active_data.borrow() {
            if let Some(session) = data.session.upgrade() {
                return Some(f(&session.borrow()));
            }
        }
        None
    }

    fn do_with_session_mut<R>(&self, f: impl FnOnce(&mut Session) -> R) -> Option<R> {
        if let Some(data) = self.active_data.borrow() {
            if let Some(session) = data.session.upgrade() {
                return Some(f(&mut session.borrow_mut()));
            }
        }
        None
    }

    fn invalidate_version_text(&self) {
        self.view
            .require_control(root::ID_MAIN_PANEL_VERSION_TEXT)
            .set_text(format!("ReaLearn {}", App::detailed_version_label()));
    }

    fn invalidate_all_controls(&self) {
        self.invalidate_version_text();
        self.invalidate_status_text();
    }

    fn register_listeners(self: SharedView<Self>) {
        let state = self.state.borrow();
        self.when(state.status_msg.changed(), |view| {
            view.invalidate_status_text();
        });
        self.register_session_listeners();
    }

    fn register_session_listeners(self: &SharedView<Self>) {
        self.do_with_session(|session| {
            self.when(session.everything_changed(), |view| {
                view.invalidate_all_controls();
            });
            self.when(session.tags.changed(), |view| {
                view.invalidate_status_text();
            });
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
    }

    fn handle_changed_parameters(&self, session: &Session) {
        if let Some(data) = self.active_data.borrow() {
            data.panel_manager
                .borrow()
                .handle_changed_parameters(session);
        }
    }

    fn edit_tags(&self) {
        let initial_csv = self
            .do_with_session(|session| format_tags_as_csv(session.tags.get_ref()))
            .unwrap_or_default();
        let new_tag_string = match dialog_util::prompt_for("Tags", &initial_csv) {
            None => return,
            Some(s) => s,
        };
        let tags = parse_tags_from_csv(&new_tag_string);
        self.do_with_session_mut(|session| {
            session.tags.set(tags);
        });
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
            self.dimensions
                .replace(Some(window.convert_to_pixels(util::MAIN_PANEL_DIMENSIONS)));
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
            root::IDC_EDIT_TAGS_BUTTON => self.edit_tags(),
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

    fn send_projection_feedback(&self, session: &Session, value: ProjectionFeedbackValue) {
        let _ = send_projection_feedback_to_subscribed_clients(session.id(), value);
    }

    fn clip_matrix_polled(
        &self,
        session: &Session,
        matrix: &RealearnClipMatrix,
        events: &[ClipMatrixEvent],
    ) {
        send_occasional_slot_updates(session, events);
        send_continuous_slot_updates(session, events);
        send_continuous_matrix_updates(session);
        send_continuous_column_updates(session, matrix);
    }

    fn mapping_matched(&self, event: MappingMatchedEvent) {
        upgrade_panel(self).handle_matched_mapping(event);
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

fn send_occasional_slot_updates(session: &Session, events: &[ClipMatrixEvent]) {
    let sender = App::get().occasional_slot_update_sender();
    if sender.receiver_count() == 0 {
        return;
    }
    let updates: Vec<_> = events
        .iter()
        .filter_map(|event| {
            if let ClipMatrixEvent::ClipChanged(QualifiedClipChangeEvent {
                slot_coordinates,
                event: ClipChangeEvent::PlayState(play_state),
            }) = event
            {
                Some(QualifiedOccasionalSlotUpdate {
                    slot_coordinates: Some(SlotCoordinates::from_engine(*slot_coordinates)),
                    update: Some(qualified_occasional_slot_update::Update::PlayState(
                        SlotPlayState::from_engine(play_state.get()).into(),
                    )),
                })
            } else {
                None
            }
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
                event: ClipChangeEvent::ClipPosition(pos),
            }) = event
            {
                Some(QualifiedContinuousSlotUpdate {
                    slot_coordinates: Some(SlotCoordinates::from_engine(*slot_coordinates)),
                    update: Some(ContinuousSlotUpdate {
                        clip_updates: vec![ContinuousClipUpdate {
                            position: pos.get(),
                            peak: 0.0,
                        }],
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
    // TODO-high CONTINUE We are mainly interested in beats relative to the bar in order to get a
    //  typical position display and a useful visual metronome!
    let full_beats = timeline.full_beats_at_pos(pos);
    let batch_event = ContinuousMatrixUpdateBatch {
        session_id: session.id().to_owned(),
        value: ContinuousMatrixUpdate {
            second: pos.get(),
            bar: (next_bar.position() - 1) as i32,
            beat: full_beats.get(),
            peaks: get_track_peaks(&project.or_current_project().master_track()),
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
                    let update = ContinuousColumnUpdate {
                        peaks: get_track_peaks(&track),
                    };
                    update
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
    // TODO-high Apply same fix as in #560 (check I_VUMODE to know whether to query volume or peaks)
    (0..channel_count)
        .map(|ch| {
            let volume = unsafe { reaper.track_get_peak_info(track, ch as u32 + 1024) };
            volume.get()
        })
        .collect()
}
