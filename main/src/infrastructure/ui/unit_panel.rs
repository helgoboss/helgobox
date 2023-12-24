use crate::infrastructure::ui::{
    bindings::root, HeaderPanel, IndependentPanelManager, MappingRowsPanel,
    SharedIndependentPanelManager, SharedMainState,
};

use reaper_high::Reaper;

use std::cell::RefCell;

use crate::application::{
    get_virtual_fx_label, get_virtual_track_label, Affected, CompartmentProp, InstanceModel,
    SessionProp, SessionUi, VirtualFxType, WeakInstanceModel,
};
use crate::base::when;
use crate::domain::ui_util::format_tags_as_csv;
use crate::domain::{
    Compartment, InfoEvent, MappingId, MappingMatchedEvent, ProjectionFeedbackValue,
    QualifiedMappingId, TargetControlEvent, TargetValueChangedEvent,
};
use crate::infrastructure::plugin::{BackboneShell, InstanceParamContainer};
use crate::infrastructure::server::http::{
    send_projection_feedback_to_subscribed_clients, send_updated_controller_routing,
};
use crate::infrastructure::ui::util::{header_panel_height, parse_tags_from_csv};
use base::SoundPlayer;
use helgoboss_allocator::undesired_allocation_count;
use rxrust::prelude::*;
use std::rc::{Rc, Weak};
use std::sync;
use swell_ui::{DialogUnits, Point, SharedView, View, ViewContext, Window};

/// Just the old term as alias for easier class search.
type _MainPanel = UnitPanel;

/// The complete ReaLearn panel containing everything.
#[derive(Debug)]
pub struct UnitPanel {
    view: ViewContext,
    session: WeakInstanceModel,
    header_panel: SharedView<HeaderPanel>,
    mapping_rows_panel: SharedView<MappingRowsPanel>,
    panel_manager: SharedIndependentPanelManager,
    success_sound_player: Option<SoundPlayer>,
    state: SharedMainState,
}

impl UnitPanel {
    pub fn new(
        session: WeakInstanceModel,
        plugin_parameters: sync::Weak<InstanceParamContainer>,
    ) -> SharedView<Self> {
        let panel_manager = IndependentPanelManager::new(session.clone());
        let panel_manager = Rc::new(RefCell::new(panel_manager));
        let state = SharedMainState::default();
        let main_panel = Self {
            view: Default::default(),
            state: state.clone(),
            session: session.clone(),
            header_panel: HeaderPanel::new(
                session.clone(),
                state.clone(),
                plugin_parameters,
                Rc::downgrade(&panel_manager),
            )
            .into(),
            mapping_rows_panel: MappingRowsPanel::new(
                session,
                Rc::downgrade(&panel_manager),
                state,
                Point::new(DialogUnits(0), header_panel_height()),
            )
            .into(),
            panel_manager: panel_manager.clone(),
            success_sound_player: {
                let mut sound_player = SoundPlayer::new();
                if let Some(path_to_file) = BackboneShell::realearn_high_click_sound_path() {
                    if sound_player.load_file(path_to_file).is_ok() {
                        Some(sound_player)
                    } else {
                        None
                    }
                } else {
                    None
                }
            },
        };
        let main_panel = Rc::new(main_panel);
        panel_manager
            .borrow_mut()
            .set_main_panel(Rc::downgrade(&main_panel));
        // If the plug-in window is currently open, open the sub panels as well. Now we are talking!
        if let Some(window) = main_panel.view.window() {
            main_panel.open_sub_panels(window);
            main_panel.invalidate_all_controls();
            main_panel.register_session_listeners();
        }
        main_panel
    }

    pub fn state(&self) -> &SharedMainState {
        &self.state
    }

    pub fn force_scroll_to_mapping(&self, id: QualifiedMappingId) {
        self.mapping_rows_panel.force_scroll_to_mapping(id);
    }

    pub fn edit_mapping(&self, compartment: Compartment, mapping_id: MappingId) {
        self.mapping_rows_panel
            .edit_mapping(compartment, mapping_id);
    }

    pub fn show_pot_browser(&self) {
        self.header_panel.show_pot_browser();
    }

    fn open_sub_panels(&self, window: Window) {
        self.header_panel.clone().open(window);
        self.mapping_rows_panel.clone().open(window);
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
            let mut text =
                format!("Track: {instance_track_label:.20} | FX: {instance_fx_label:.30}");
            let fx_type = VirtualFxType::from_virtual_fx(&instance_fx.fx);
            if fx_type.requires_fx_chain() {
                let instance_fx_track_label = get_virtual_track_label(
                    &instance_fx.track_descriptor.track,
                    compartment,
                    session.extended_context(),
                );
                let _ = write!(&mut text, " (on track {instance_fx_track_label:.15})");
            }
            let control_and_feedback_state = instance_state.global_control_and_feedback_state();
            if !control_and_feedback_state.control_active {
                text.push_str(" | CONTROL off");
            }
            if !control_and_feedback_state.feedback_active {
                text.push_str(" | FEEDBACK off");
            }
            if cfg!(debug_assertions) {
                let _ = write!(&mut text, " | rt-allocs: {}", undesired_allocation_count());
            }
            let label = self.view.require_control(root::ID_MAIN_PANEL_STATUS_2_TEXT);
            label.disable();
            label.set_text(text.as_str());
        });
    }

    fn do_with_session<R>(&self, f: impl FnOnce(&InstanceModel) -> R) -> Result<R, &'static str> {
        match self.session.upgrade() {
            None => Err("session not available anymore"),
            Some(session) => Ok(f(&session.borrow())),
        }
    }

    fn do_with_session_mut<R>(
        &self,
        f: impl FnOnce(&mut InstanceModel) -> R,
    ) -> Result<R, &'static str> {
        match self.session.upgrade() {
            None => Err("session not available anymore"),
            Some(session) => Ok(f(&mut session.borrow_mut())),
        }
    }

    fn invalidate_version_text(&self) {
        self.view
            .require_control(root::ID_MAIN_PANEL_VERSION_TEXT)
            .set_text(format!(
                "ReaLearn {}",
                BackboneShell::detailed_version_label()
            ));
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
        self.panel_manager
            .borrow()
            .handle_changed_target_value(event);
    }

    fn handle_matched_mapping(&self, event: MappingMatchedEvent) {
        self.panel_manager.borrow().handle_matched_mapping(event);
        if self.is_open() {
            self.mapping_rows_panel.handle_matched_mapping(event);
        }
    }

    #[cfg(feature = "playtime")]
    pub fn app_instance(&self) -> crate::infrastructure::ui::SharedAppInstance {
        self.panel_manager.borrow().app_instance().clone()
    }

    #[cfg(feature = "playtime")]
    pub fn start_show_or_hide_app_instance(&self) {
        self.panel_manager
            .borrow()
            .start_show_or_hide_app_instance();
    }

    fn handle_target_control_event(&self, event: TargetControlEvent) {
        self.panel_manager
            .borrow()
            .handle_target_control_event(event);
    }

    fn handle_affected(
        self: SharedView<Self>,
        affected: Affected<SessionProp>,
        initiator: Option<u32>,
    ) {
        self.panel_manager
            .borrow()
            .handle_affected(&affected, initiator);
        self.mapping_rows_panel
            .handle_affected(&affected, initiator);
        self.header_panel.handle_affected(&affected, initiator);
        self.handle_affected_own(affected);
    }

    fn handle_info_event(self: SharedView<Self>, event: &InfoEvent) {
        match event {
            InfoEvent::UndesiredAllocationCountChanged => {
                self.invalidate_status_2_text();
            }
        }
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

    fn handle_changed_parameters(&self, session: &InstanceModel) {
        self.panel_manager
            .borrow()
            .handle_changed_parameters(session);
    }

    fn celebrate_success(&self) {
        if let Some(s) = &self.success_sound_player {
            let _ = s.play();
        }
    }

    fn handle_changed_midi_devices(&self) {
        self.header_panel.handle_changed_midi_devices();
    }

    fn handle_changed_conditions(&self) {
        self.panel_manager.borrow().handle_changed_conditions();
        if self.is_open() {
            self.mapping_rows_panel.handle_changed_conditions();
        }
    }

    fn edit_instance_data(&self) -> Result<(), &'static str> {
        let (initial_session_id, initial_tags_as_csv) = self.do_with_session(|session| {
            (
                session.id().to_owned(),
                format_tags_as_csv(session.tags.get_ref()),
            )
        })?;
        let initial_csv = format!("{initial_session_id}|{initial_tags_as_csv}");
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
            if BackboneShell::get().has_session(&new_session_id) {
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

impl View for UnitPanel {
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

    fn button_clicked(self: SharedView<Self>, resource_id: u32) {
        match resource_id {
            root::IDC_EDIT_TAGS_BUTTON => {
                self.edit_instance_data().unwrap();
            }
            _ => {}
        }
    }
}

impl SessionUi for Weak<UnitPanel> {
    fn show_mapping(&self, compartment: Compartment, mapping_id: MappingId) {
        upgrade_panel(self).edit_mapping(compartment, mapping_id);
    }

    fn show_pot_browser(&self) {
        upgrade_panel(self).show_pot_browser();
    }

    fn target_value_changed(&self, event: TargetValueChangedEvent) {
        upgrade_panel(self).handle_changed_target_value(event);
    }

    fn parameters_changed(&self, session: &InstanceModel) {
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

    fn send_projection_feedback(&self, session: &InstanceModel, value: ProjectionFeedbackValue) {
        let _ = send_projection_feedback_to_subscribed_clients(session.id(), value);
    }

    #[cfg(feature = "playtime")]
    fn clip_matrix_changed(
        &self,
        session: &InstanceModel,
        matrix: &playtime_clip_engine::base::Matrix,
        events: &[playtime_clip_engine::base::ClipMatrixEvent],
        is_poll: bool,
    ) {
        BackboneShell::get().clip_engine_hub().clip_matrix_changed(
            session.id(),
            matrix,
            events,
            is_poll,
            session.processor_context().project(),
        );
    }

    #[cfg(feature = "playtime")]
    fn process_control_surface_change_event_for_clip_engine(
        &self,
        session: &InstanceModel,
        matrix: &playtime_clip_engine::base::Matrix,
        events: &[reaper_high::ChangeEvent],
    ) {
        BackboneShell::get()
            .clip_engine_hub()
            .send_occasional_matrix_updates_caused_by_reaper(session.id(), matrix, events);
    }

    fn mapping_matched(&self, event: MappingMatchedEvent) {
        upgrade_panel(self).handle_matched_mapping(event);
    }

    fn target_controlled(&self, event: TargetControlEvent) {
        upgrade_panel(self).handle_target_control_event(event);
    }

    fn handle_affected(
        &self,
        session: &InstanceModel,
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

    fn handle_info_event(&self, event: &InfoEvent) {
        upgrade_panel(self).handle_info_event(event);
    }
}

fn upgrade_panel(panel: &Weak<UnitPanel>) -> Rc<UnitPanel> {
    panel.upgrade().expect("main panel not existing anymore")
}
