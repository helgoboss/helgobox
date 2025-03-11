use crate::infrastructure::ui::{
    bindings::root, HeaderPanel, IndependentPanelManager, MappingRowsPanel,
    SharedIndependentPanelManager, SharedMainState,
};

use reaper_high::Reaper;

use crate::application::{
    get_virtual_fx_label, get_virtual_track_label, Affected, CompartmentProp, SessionCommand,
    SessionProp, SessionUi, UnitModel, VirtualFxType, WeakUnitModel,
};
use crate::base::when;
use crate::domain::ui_util::format_tags_as_csv;
use crate::domain::{
    CompartmentKind, InstanceId, InternalInfoEvent, MappingId, MappingMatchedEvent,
    ProjectionFeedbackValue, QualifiedMappingId, SourceFeedbackEvent, TargetControlEvent,
    TargetValueChangedEvent,
};
use crate::infrastructure::plugin::{update_auto_units_async, BackboneShell};
use crate::infrastructure::server::http::{
    send_projection_feedback_to_subscribed_clients, send_sessions_to_subscribed_clients,
    send_updated_controller_routing,
};
use crate::infrastructure::ui::instance_panel::InstancePanel;
use crate::infrastructure::ui::util::{header_panel_height, parse_tags_from_csv};
use anyhow::Context;
use base::SoundPlayer;
use helgobox_allocator::undesired_allocation_count;
use helgobox_api::runtime::InstanceInfoEvent;
use rxrust::prelude::*;
use semver::Version;
use std::cell::RefCell;
use std::fmt;
use std::fmt::Write;
use std::rc::{Rc, Weak};
use swell_ui::{DialogUnits, Point, SharedView, View, ViewContext, WeakView, Window};

/// Just the old term as alias for easier class search.
type _MainPanel = UnitPanel;

/// The complete ReaLearn panel containing everything.
#[derive(Debug)]
pub struct UnitPanel {
    instance_id: InstanceId,
    view: ViewContext,
    unit_model: WeakUnitModel,
    instance_panel: WeakView<InstancePanel>,
    header_panel: SharedView<HeaderPanel>,
    mapping_rows_panel: SharedView<MappingRowsPanel>,
    panel_manager: SharedIndependentPanelManager,
    success_sound_player: Option<SoundPlayer>,
    state: SharedMainState,
}

impl UnitPanel {
    pub fn new(
        instance_id: InstanceId,
        session: WeakUnitModel,
        instance_panel: WeakView<InstancePanel>,
    ) -> SharedView<Self> {
        let panel_manager = IndependentPanelManager::new(session.clone());
        let panel_manager = Rc::new(RefCell::new(panel_manager));
        let state = SharedMainState::default();
        let main_panel = Self {
            instance_id,
            view: Default::default(),
            state: state.clone(),
            unit_model: session.clone(),
            instance_panel: instance_panel.clone(),
            header_panel: HeaderPanel::new(
                session.clone(),
                state.clone(),
                Rc::downgrade(&panel_manager),
                instance_panel,
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

    pub fn edit_mapping(&self, compartment: CompartmentKind, mapping_id: MappingId) {
        self.mapping_rows_panel
            .edit_mapping(compartment, mapping_id);
    }

    pub fn show_pot_browser(&self) {
        self.header_panel.show_pot_browser();
    }

    pub fn header_panel(&self) -> SharedView<HeaderPanel> {
        self.header_panel.clone()
    }

    fn open_sub_panels(&self, window: Window) {
        self.header_panel.clone().open(window);
        self.mapping_rows_panel.clone().open(window);
    }

    fn invalidate_status_1_text(&self) {
        use std::fmt::Write;
        let _ = self.do_with_session(|unit_model| {
            let state = self.state.borrow();
            let scroll_status = state.scroll_status.get_ref();
            let tags = unit_model.tags.get_ref();
            let instance_id = unit_model.instance_id();
            let unit_key = unit_model.unit_key();
            let mut text = format!(
                "Showing mappings {} to {} of {} | Instance ID: {instance_id} | Unit key: {unit_key}",
                scroll_status.from_pos,
                scroll_status.to_pos,
                scroll_status.item_count,
            );
            if !tags.is_empty() {
                let _ = write!(&mut text, " | Unit tags: {}", format_tags_as_csv(tags));
            }
            self.view
                .require_control(root::ID_MAIN_PANEL_STATUS_1_TEXT)
                .set_text(text.as_str());
        });
    }

    fn invalidate_status_2_text(&self) {
        use std::fmt::Write;
        let _ = self.do_with_session(|session| {
            let instance_state = session.unit().borrow();
            let instance_track = instance_state.instance_track_descriptor();
            let compartment = CompartmentKind::Main;
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

    fn do_with_session<R>(&self, f: impl FnOnce(&UnitModel) -> R) -> Result<R, &'static str> {
        match self.unit_model.upgrade() {
            None => Err("session not available anymore"),
            Some(session) => Ok(f(&session.borrow())),
        }
    }

    fn do_with_session_mut<R>(
        &self,
        f: impl FnOnce(&mut UnitModel) -> R,
    ) -> Result<R, &'static str> {
        match self.unit_model.upgrade() {
            None => Err("session not available anymore"),
            Some(session) => Ok(f(&mut session.borrow_mut())),
        }
    }

    fn invalidate_all_controls(&self) {
        self.invalidate_unit_button().unwrap();
        let _ = self.invalidate_version_text();
        self.invalidate_status_1_text();
        self.invalidate_status_2_text();
    }

    pub fn notify_units_changed(&self) {
        if self.view.window().is_some() {
            self.invalidate_unit_button().unwrap();
        }
    }

    fn invalidate_unit_button(&self) -> anyhow::Result<()> {
        let instance_panel = self.instance_panel();
        let instance_shell = instance_panel.shell()?;
        let unit_count = instance_shell.additional_unit_count() + 1;
        let unit_id = instance_panel.displayed_unit_id();
        let (index, unit_model) = instance_shell
            .find_unit_index_and_model_by_id(unit_id)
            .context("unit not found")?;
        let unit_model = unit_model.try_borrow().context("borrow unit model")?;
        let label = build_unit_label(&unit_model, index, Some(unit_count));
        self.view
            .require_control(root::IDC_UNIT_BUTTON)
            .set_text(label);
        Ok(())
    }

    fn invalidate_version_text(&self) -> anyhow::Result<()> {
        let mut text = String::new();
        text.write_str("Helgobox ")?;
        text.write_str(BackboneShell::detailed_version_label())?;
        if let Some(remote_config) = BackboneShell::remote_config() {
            if report_new_version(
                BackboneShell::version(),
                &remote_config.plugin.latest_version,
            ) {
                text.write_str(" [UPDATE AVAILABLE]")?;
            }
        }
        self.view
            .require_control(root::ID_MAIN_PANEL_VERSION_TEXT)
            .set_text(text);
        Ok(())
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
            let instance_state = session.unit().borrow();
            self.when(
                instance_state.global_control_and_feedback_state_changed(),
                |view| {
                    view.invalidate_status_2_text();
                },
            );
        });
    }

    pub fn panel_manager(&self) -> &SharedIndependentPanelManager {
        &self.panel_manager
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

    fn handle_target_control_event(&self, event: TargetControlEvent) {
        self.panel_manager
            .borrow()
            .handle_target_control_event(event);
    }

    fn handle_source_feedback_event(&self, event: SourceFeedbackEvent) {
        self.panel_manager
            .borrow()
            .handle_source_feedback_event(event);
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

    fn handle_internal_info_event(self: SharedView<Self>, event: &InternalInfoEvent) {
        if !self.is_open() {
            return;
        }
        match event {
            InternalInfoEvent::UndesiredAllocationCountChanged => {
                self.invalidate_status_2_text();
            }
        }
    }

    fn handle_external_info_event(self: SharedView<Self>, event: InstanceInfoEvent) {
        BackboneShell::get()
            .proto_hub()
            .notify_about_instance_info_event(self.instance_id, event);
    }

    fn handle_affected_own(self: SharedView<Self>, affected: Affected<SessionProp>) {
        use Affected::*;
        use SessionProp::*;
        // Handle even if closed
        match affected {
            One(UnitName) => {
                if let Ok(shell) = self.instance_panel().shell() {
                    BackboneShell::get()
                        .proto_hub()
                        .notify_instance_units_changed(&shell);
                }
            }
            One(UnitKey) => {
                // Actually, the instances are only affected if the main unit key is changed, but
                // so what.
                BackboneShell::get().proto_hub().notify_instances_changed();
                send_sessions_to_subscribed_clients();
            }
            _ => {}
        }
        // Handle only if open
        if !self.is_open() {
            return;
        }
        match affected {
            One(InstanceTrack | InstanceFx) => {
                self.invalidate_status_2_text();
            }
            One(UnitName) => {
                let _ = self.invalidate_unit_button();
            }
            One(UnitKey) => {
                self.invalidate_status_1_text();
            }
            Multiple => {
                self.invalidate_all_controls();
            }
            _ => {}
        }
    }

    fn handle_changed_parameters(&self, session: &UnitModel) {
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

    fn open_unit_popup_menu(self: SharedView<Self>) {
        self.instance_panel().open_unit_popup_menu();
    }

    fn instance_panel(&self) -> SharedView<InstancePanel> {
        self.instance_panel
            .upgrade()
            .expect("instance panel doesn't exist anymore")
    }

    fn edit_unit_data(&self) -> Result<(), &'static str> {
        let (initial_key, initial_name, initial_tags_as_csv) = self.do_with_session(|session| {
            (
                session.unit_key().to_owned(),
                session.name().map(|n| n.to_string()).unwrap_or_default(),
                format_tags_as_csv(session.tags.get_ref()),
            )
        })?;
        let initial_csv = format!("{initial_key}|{initial_name}|{initial_tags_as_csv}");
        // Show UI
        let csv_result = Reaper::get().medium_reaper().get_user_inputs(
            "ReaLearn",
            3,
            "Unit Key,Unit Name,Tags,separator=|,extrawidth=200",
            initial_csv,
            1024,
        );
        // Check if cancelled
        let csv = match csv_result {
            // Cancelled
            None => return Ok(()),
            Some(csv) => csv,
        };
        // Parse result CSV
        let split: Vec<_> = csv.to_str().split('|').collect();
        let (unit_key, unit_name, tags_as_csv) = match split.as_slice() {
            [unit_key, unit_name, tags_as_csv] => (unit_key, unit_name, tags_as_csv),
            _ => return Err("couldn't split result"),
        };
        // Take care of tags
        if tags_as_csv != &initial_tags_as_csv {
            let tags = parse_tags_from_csv(tags_as_csv);
            self.do_with_session_mut(|session| {
                session.tags.set(tags);
            })?;
        }
        // Take care of unit name
        let unit_name = if unit_name.trim().is_empty() {
            None
        } else {
            Some(unit_name.to_string())
        };
        self.do_with_session_mut(|session| {
            session.change_with_notification(
                SessionCommand::SetUnitName(unit_name),
                None,
                self.unit_model.clone(),
            );
        })?;
        // Take care of unit key
        // TODO-low Introduce a SessionId newtype.
        let new_unit_key = unit_key.replace(|ch| !nanoid::alphabet::SAFE.contains(&ch), "");
        if !new_unit_key.is_empty() && new_unit_key != initial_key {
            if BackboneShell::get().has_unit_with_key(&new_unit_key) {
                self.view.require_window().alert(
                    "ReaLearn",
                    "There's another open ReaLearn unit which already has this unit key!",
                );
                return Ok(());
            }
            self.do_with_session_mut(|session| {
                session.change_with_notification(
                    SessionCommand::SetUnitKey(new_unit_key),
                    None,
                    self.unit_model.clone(),
                );
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
        self.open_sub_panels(window);
        self.invalidate_all_controls();
        self.register_listeners();
        true
    }

    fn button_clicked(self: SharedView<Self>, resource_id: u32) {
        match resource_id {
            root::IDC_UNIT_BUTTON => {
                // Yes, putting this button into the instance panel would make more sense logically
                // but since the unit panel completely covers the instance panel, the button would
                // be unusable.
                self.open_unit_popup_menu();
            }
            root::IDC_EDIT_TAGS_BUTTON => {
                self.edit_unit_data().unwrap();
            }
            _ => {}
        }
    }
}

impl SessionUi for Weak<UnitPanel> {
    fn show_mapping(&self, compartment: CompartmentKind, mapping_id: MappingId) {
        upgrade_panel(self).edit_mapping(compartment, mapping_id);
    }

    fn show_pot_browser(&self) {
        upgrade_panel(self).show_pot_browser();
    }

    fn target_value_changed(&self, event: TargetValueChangedEvent) {
        upgrade_panel(self).handle_changed_target_value(event);
    }

    fn parameters_changed(&self, session: &UnitModel) {
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

    fn send_projection_feedback(&self, session: &UnitModel, value: ProjectionFeedbackValue) {
        let _ = send_projection_feedback_to_subscribed_clients(session.unit_key(), value);
    }

    fn mapping_matched(&self, event: MappingMatchedEvent) {
        upgrade_panel(self).handle_matched_mapping(event);
    }

    fn handle_target_control(&self, event: TargetControlEvent) {
        upgrade_panel(self).handle_target_control_event(event);
    }

    fn handle_source_feedback(&self, event: SourceFeedbackEvent) {
        upgrade_panel(self).handle_source_feedback_event(event);
    }

    fn handle_affected(
        &self,
        session: &UnitModel,
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

    fn handle_internal_info_event(&self, event: &InternalInfoEvent) {
        upgrade_panel(self).handle_internal_info_event(event);
    }

    fn handle_external_info_event(&self, event: InstanceInfoEvent) {
        upgrade_panel(self).handle_external_info_event(event);
    }

    fn handle_everything_changed(&self, unit_model: &UnitModel) {
        BackboneShell::get()
            .proto_hub()
            .notify_everything_in_unit_has_changed(unit_model.instance_id(), unit_model.unit_id());
    }

    fn handle_global_control_and_feedback_state_changed(&self) {
        update_auto_units_async();
    }
}

fn upgrade_panel(panel: &Weak<UnitPanel>) -> Rc<UnitPanel> {
    panel.upgrade().expect("unit panel not existing anymore")
}

pub fn build_unit_label(
    unit_model: &UnitModel,
    index: Option<usize>,
    count: Option<usize>,
) -> String {
    build_unit_label_internal(unit_model, index, count).unwrap_or_default()
}

/// A given count means that we build the button label, in which case the total number of units will be displayed, too.
fn build_unit_label_internal(
    unit_model: &UnitModel,
    index: Option<usize>,
    count: Option<usize>,
) -> Result<String, fmt::Error> {
    use std::fmt::Write;
    let mut s = String::new();
    let pos = index.map(|i| i + 2).unwrap_or(1);
    // Unit with position
    write!(&mut s, "Unit {pos}")?;
    // Total unit count, only if button label
    if let Some(c) = count {
        // This is for a button label. We want to display the total unit count.
        write!(&mut s, "/{c}")?;
    }
    // Indicate which one is an auto unit
    if unit_model.auto_unit().is_some() {
        write!(&mut s, " (auto)")?;
    }
    let label = unit_model.name_or_key();
    write!(&mut s, ": {label}")?;
    Ok(s)
}

fn report_new_version(current: &Version, latest: &Version) -> bool {
    if current >= latest {
        return false;
    }
    if !latest.pre.is_empty() && current.pre.is_empty() {
        return false;
    }
    true
}
