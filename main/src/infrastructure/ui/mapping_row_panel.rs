use crate::application::{
    Affected, CompartmentProp, MappingCommand, MappingModel, MappingProp, SessionProp,
    SharedMapping, SharedUnitModel, SourceCategory, TargetCategory, TargetModelFormatMultiLine,
    UnitModel, WeakUnitModel,
};
use crate::base::when;
use crate::domain::{CompartmentKind, GroupId, GroupKey, MappingId, QualifiedMappingId};

use crate::domain::ui_util::format_tags_as_csv;
use crate::infrastructure::api::convert::from_data::ConversionStyle;
use crate::infrastructure::data::{
    ActivationConditionData, MappingModelData, ModeModelData, SourceModelData, TargetModelData,
};
use crate::infrastructure::plugin::BackboneShell;
use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::bindings::root::{
    IDC_MAPPING_ROW_ENABLED_CHECK_BOX, ID_MAPPING_ROW_CONTROL_CHECK_BOX,
    ID_MAPPING_ROW_FEEDBACK_CHECK_BOX,
};
use crate::infrastructure::ui::color_panel::{ColorPanel, ColorPanelDesc};
use crate::infrastructure::ui::dialog_util::add_group_via_dialog;
use crate::infrastructure::ui::util::{
    colors, mapping_row_panel_height, symbols, view, GLOBAL_SCALING,
};
use crate::infrastructure::ui::{
    copy_text_to_clipboard, deserialize_api_object_from_lua, deserialize_data_object_from_json,
    get_text_from_clipboard, serialize_data_object, DataObject, IndependentPanelManager,
    MappingPanel, SerializationFormat, SharedMainState,
};
use anyhow::Context;
use core::iter;
use helgobox_api::persistence::{ApiObject, Envelope};
use reaper_medium::Hbrush;
use rxrust::prelude::*;
use std::cell::{Ref, RefCell};
use std::error::Error;
use std::ops::Deref;
use std::rc::{Rc, Weak};
use std::time::Duration;
use swell_ui::{DeviceContext, DialogUnits, Pixels, Point, SharedView, View, ViewContext, Window};
use tracing::debug;

pub type SharedIndependentPanelManager = Rc<RefCell<IndependentPanelManager>>;

/// Panel containing the summary data of one mapping and buttons such as "Remove".
#[derive(Debug)]
pub struct MappingRowPanel {
    view: ViewContext,
    session: WeakUnitModel,
    main_state: SharedMainState,
    mapping_color_panel: SharedView<ColorPanel>,
    source_color_panel: SharedView<ColorPanel>,
    target_color_panel: SharedView<ColorPanel>,
    row_index: u32,
    // We use virtual scrolling to be able to show a large number of rows without any
    // performance issues. That means there's a fixed number of mapping rows, and they just
    // display different mappings depending on the current scroll position. If there are fewer
    // mappings than the fixed number, some rows remain unused. In this case, their mapping is
    // `None`, which will make the row hide itself.
    mapping: RefCell<Option<SharedMapping>>,
    // Fires when a mapping is about to change.
    party_is_over_subject: RefCell<LocalSubject<'static, (), ()>>,
    panel_manager: Weak<RefCell<IndependentPanelManager>>,
}

impl MappingRowPanel {
    pub fn new(
        session: WeakUnitModel,
        row_index: u32,
        panel_manager: Weak<RefCell<IndependentPanelManager>>,
        main_state: SharedMainState,
    ) -> MappingRowPanel {
        MappingRowPanel {
            view: Default::default(),
            session,
            main_state,
            mapping_color_panel: SharedView::new(ColorPanel::new(build_mapping_color_panel_desc())),
            source_color_panel: SharedView::new(ColorPanel::new(build_source_color_panel_desc())),
            target_color_panel: SharedView::new(ColorPanel::new(build_target_color_panel_desc())),
            row_index,
            party_is_over_subject: Default::default(),
            mapping: None.into(),
            panel_manager,
        }
    }

    pub fn handle_affected(&self, affected: &Affected<SessionProp>, _initiator: Option<u32>) {
        // If the reaction can't be displayed anymore because the mapping is not filled anymore,
        // so what.
        use Affected::*;
        use CompartmentProp::*;
        use SessionProp::*;
        self.with_mapping(|_, m| {
            match affected {
                One(InCompartment(compartment, One(InGroup(_, _))))
                    if *compartment == m.compartment() =>
                {
                    // Refresh to display potentially new inherited tags.
                    self.invalidate_name_labels(m);
                }
                One(InCompartment(compartment, One(InMapping(mapping_id, affected))))
                    if *compartment == m.compartment() && *mapping_id == m.id() =>
                {
                    match affected {
                        Multiple => {
                            self.invalidate_all_controls(m);
                        }
                        One(prop) => {
                            use MappingProp as P;
                            match prop {
                                P::Name | P::Tags => {
                                    self.invalidate_name_labels(m);
                                }
                                P::IsEnabled => {
                                    self.invalidate_enabled_check_box(m);
                                }
                                P::ControlIsEnabled => {
                                    self.invalidate_control_check_box(m);
                                }
                                P::FeedbackIsEnabled => {
                                    self.invalidate_feedback_check_box(m);
                                }
                                P::InSource(_) => {
                                    self.invalidate_source_label(m);
                                }
                                P::InTarget(_) => {
                                    self.invalidate_name_labels(m);
                                    self.invalidate_target_label(m);
                                }
                                _ => {}
                            }
                        }
                    }
                }
                _ => {}
            }
        });
    }

    pub fn handle_matched_mapping(&self) {
        self.source_match_indicator_control().enable();
        self.view
            .require_window()
            .set_timer(SOURCE_MATCH_INDICATOR_TIMER_ID, Duration::from_millis(50));
    }

    pub fn handle_changed_conditions(&self) {
        self.with_mapping(|p, m| {
            p.invalidate_name_labels(m);
            p.invalidate_target_label(m);
        });
    }

    fn source_match_indicator_control(&self) -> Window {
        self.view
            .require_control(root::IDC_MAPPING_ROW_MATCHED_INDICATOR_TEXT)
    }

    pub fn mapping_id(&self) -> Option<MappingId> {
        let mapping = self.optional_mapping()?;
        let mapping = mapping.borrow();
        Some(mapping.id())
    }

    pub fn set_mapping(self: &SharedView<Self>, mapping: Option<SharedMapping>) {
        self.party_is_over_subject.borrow_mut().next(());
        match &mapping {
            None => self.view.require_window().hide(),
            Some(m) => {
                self.view.require_window().show();
                self.invalidate_all_controls(&m.borrow());
                self.register_listeners();
            }
        }
        self.mapping.replace(mapping);
    }

    fn invalidate_all_controls(&self, mapping: &MappingModel) {
        self.invalidate_name_labels(mapping);
        self.invalidate_source_label(mapping);
        self.invalidate_target_label(mapping);
        self.invalidate_learn_source_button(mapping);
        self.invalidate_learn_target_button(mapping);
        self.invalidate_enabled_check_box(mapping);
        self.invalidate_control_check_box(mapping);
        self.invalidate_feedback_check_box(mapping);
        self.invalidate_on_indicator(mapping);
        self.invalidate_button_enabled_states();
    }

    fn invalidate_name_labels(&self, mapping: &MappingModel) {
        let main_state = self.main_state.borrow();
        // Left label
        self.view
            .require_window()
            .require_control(root::ID_MAPPING_ROW_MAPPING_LABEL)
            .set_text(mapping.effective_name());
        // Initialize right label with tags
        let session = self.session();
        let session = session.borrow();
        let group_id = mapping.group_id();
        let compartment = main_state.active_compartment.get();
        let group = session.find_group_by_id_including_default_group(compartment, group_id);
        let mut right_label = if let Some(g) = group {
            // Group present. Merge group tags with mapping tags.
            let g = g.borrow();
            format_tags_as_csv(g.tags().iter().chain(mapping.tags()))
        } else {
            // Group not present. Use mapping tags only.
            format_tags_as_csv(mapping.tags())
        };
        // Add group name to right label if all groups are shown.
        if main_state
            .displayed_group_for_active_compartment()
            .is_none()
        {
            let group_label = if let Some(g) = group {
                g.borrow().effective_name().to_owned()
            } else {
                "<group not present>".to_owned()
            };
            if !right_label.is_empty() {
                right_label += " | ";
            }
            right_label += &group_label;
        };
        self.view
            .require_window()
            .require_control(root::ID_MAPPING_ROW_GROUP_LABEL)
            .set_text(right_label);
    }

    fn session(&self) -> SharedUnitModel {
        self.session.upgrade().expect("session gone")
    }

    fn invalidate_source_label(&self, mapping: &MappingModel) {
        let plain_label = mapping.source_model.to_string();
        let rich_label = if mapping.source_model.category() == SourceCategory::Virtual {
            let session = self.session();
            let session = session.borrow();
            let controller_mappings = session.mappings(CompartmentKind::Controller);
            let mappings: Vec<_> = controller_mappings
                .filter(|m| {
                    let m = m.borrow();
                    m.target_model.category() == TargetCategory::Virtual
                        && m.target_model.create_control_element()
                            == mapping.source_model.create_control_element()
                })
                .collect();
            if mappings.is_empty() {
                plain_label
            } else {
                let first_mapping = mappings[0].borrow();
                if first_mapping.name().is_empty() {
                    plain_label
                } else {
                    let first_mapping_name = first_mapping.effective_name();
                    if mappings.len() == 1 {
                        format!("{plain_label}\n({first_mapping_name})")
                    } else {
                        format!(
                            "{}\n({} + {})",
                            plain_label,
                            first_mapping_name,
                            mappings.len() - 1
                        )
                    }
                }
            }
        } else {
            plain_label
        };
        self.view
            .require_window()
            .require_control(root::ID_MAPPING_ROW_SOURCE_LABEL_TEXT)
            .set_text(rich_label);
    }

    fn invalidate_target_label(&self, mapping: &MappingModel) {
        let session = self.session();
        let session = session.borrow();
        if !session
            .processor_context()
            .project_or_current_project()
            .is_available()
        {
            // Prevent error on project close
            return;
        }
        let target_model_string =
            TargetModelFormatMultiLine::new(&mapping.target_model, &session, mapping.compartment())
                .to_string();
        self.view
            .require_window()
            .require_control(root::ID_MAPPING_ROW_TARGET_LABEL_TEXT)
            .set_text(target_model_string);
    }

    fn invalidate_learn_source_button(&self, mapping: &MappingModel) {
        let text = if self
            .session()
            .borrow()
            .unit()
            .borrow()
            .mapping_is_learning_source(mapping.qualified_id())
        {
            "Stop"
        } else {
            "Learn source"
        };
        self.view
            .require_control(root::ID_MAPPING_ROW_LEARN_SOURCE_BUTTON)
            .set_text(text);
    }

    fn invalidate_learn_target_button(&self, mapping: &MappingModel) {
        let text = if self
            .session()
            .borrow()
            .unit()
            .borrow()
            .mapping_is_learning_target(mapping.qualified_id())
        {
            "Stop"
        } else {
            "Learn target"
        };
        self.view
            .require_control(root::ID_MAPPING_ROW_LEARN_TARGET_BUTTON)
            .set_text(text);
    }

    fn init_symbol_controls(&self) {
        self.view
            .require_control(root::ID_MAPPING_ROW_CONTROL_CHECK_BOX)
            .set_text(symbols::arrow_right_symbol().to_string());
        self.view
            .require_control(root::ID_MAPPING_ROW_FEEDBACK_CHECK_BOX)
            .set_text(symbols::arrow_left_symbol().to_string());
        self.view
            .require_control(root::ID_UP_BUTTON)
            .set_text(symbols::arrow_up_symbol().to_string());
        self.view
            .require_control(root::ID_DOWN_BUTTON)
            .set_text(symbols::arrow_down_symbol().to_string());
        let indicator = self
            .view
            .require_control(root::IDC_MAPPING_ROW_MATCHED_INDICATOR_TEXT);
        indicator.set_text(symbols::indicator_symbol().to_string());
        indicator.disable();
    }

    fn invalidate_enabled_check_box(&self, mapping: &MappingModel) {
        self.view
            .require_control(root::IDC_MAPPING_ROW_ENABLED_CHECK_BOX)
            .set_checked(mapping.is_enabled());
    }

    fn invalidate_control_check_box(&self, mapping: &MappingModel) {
        self.view
            .require_control(root::ID_MAPPING_ROW_CONTROL_CHECK_BOX)
            .set_checked(mapping.control_is_enabled());
    }

    fn invalidate_feedback_check_box(&self, mapping: &MappingModel) {
        self.view
            .require_control(root::ID_MAPPING_ROW_FEEDBACK_CHECK_BOX)
            .set_checked(mapping.feedback_is_enabled());
    }

    fn invalidate_on_indicator(&self, mapping: &MappingModel) {
        let is_on = self
            .session()
            .borrow()
            .mapping_is_on(mapping.qualified_id());
        self.view
            .require_control(root::ID_MAPPING_ROW_MAPPING_LABEL)
            .set_enabled(is_on);
        self.view
            .require_control(root::ID_MAPPING_ROW_SOURCE_LABEL_TEXT)
            .set_enabled(is_on);
        self.view
            .require_control(root::ID_MAPPING_ROW_TARGET_LABEL_TEXT)
            .set_enabled(is_on);
        self.view
            .require_control(root::ID_MAPPING_ROW_GROUP_LABEL)
            .set_enabled(is_on);
    }

    fn mappings_are_read_only(&self) -> bool {
        self.session()
            .borrow()
            .mappings_are_read_only(self.active_compartment())
    }

    fn invalidate_button_enabled_states(&self) {
        let enabled = !self.mappings_are_read_only();
        let buttons = [
            root::ID_UP_BUTTON,
            root::ID_DOWN_BUTTON,
            root::ID_MAPPING_ROW_CONTROL_CHECK_BOX,
            root::ID_MAPPING_ROW_FEEDBACK_CHECK_BOX,
            root::ID_MAPPING_ROW_EDIT_BUTTON,
            root::ID_MAPPING_ROW_DUPLICATE_BUTTON,
            root::ID_MAPPING_ROW_REMOVE_BUTTON,
            root::ID_MAPPING_ROW_LEARN_SOURCE_BUTTON,
            root::ID_MAPPING_ROW_LEARN_TARGET_BUTTON,
        ];
        for b in buttons.iter() {
            self.view.require_control(*b).set_enabled(enabled);
        }
    }

    fn register_listeners(self: &SharedView<Self>) {
        let session = self.session();
        let session = session.borrow();
        let instance_state = session.unit().borrow();
        self.when(
            instance_state.mapping_which_learns_source().changed(),
            |view| {
                view.with_mapping(Self::invalidate_learn_source_button);
            },
        );
        self.when(
            instance_state.mapping_which_learns_target().changed(),
            |view| {
                view.with_mapping(Self::invalidate_learn_target_button);
            },
        );
        self.when(instance_state.on_mappings_changed(), |view| {
            view.with_mapping(Self::invalidate_on_indicator);
        });
        self.when(
            session
                .auto_load_mode
                .changed()
                .merge(session.learn_many_state_changed()),
            |view| {
                view.invalidate_button_enabled_states();
            },
        );
    }

    fn with_mapping(&self, use_mapping: impl Fn(&Self, &MappingModel)) {
        let mapping = self.mapping.borrow();
        if let Some(m) = mapping.as_ref() {
            use_mapping(self, m.borrow().deref())
        }
    }

    fn closed_or_mapping_will_change(
        &self,
    ) -> impl LocalObservable<'static, Item = (), Err = ()> + 'static {
        self.view
            .closed()
            .merge(self.party_is_over_subject.borrow().clone())
    }

    fn require_mapping(&self) -> Ref<SharedMapping> {
        Ref::map(self.mapping.borrow(), |m| m.as_ref().unwrap())
    }

    fn optional_mapping(&self) -> Option<SharedMapping> {
        self.mapping.clone().into_inner()
    }

    fn get_qualified_mapping_id(&self) -> anyhow::Result<QualifiedMappingId> {
        let qualified_id = self
            .mapping
            .borrow()
            .as_ref()
            .context("row mapping not available")?
            .borrow()
            .qualified_id();
        Ok(qualified_id)
    }

    fn edit_mapping(&self) -> SharedView<MappingPanel> {
        self.main_state.borrow_mut().stop_filter_learning();
        self.panel_manager()
            .borrow_mut()
            .edit_mapping(self.require_mapping().deref())
    }

    fn panel_manager(&self) -> SharedIndependentPanelManager {
        self.panel_manager.upgrade().expect("panel manager gone")
    }

    fn move_mapping_within_list(&self, increment: isize) -> Result<(), &'static str> {
        // When we route keyboard input to ReaLearn and press space, it presses the "Up" button,
        // even if we don't display the rows. Don't know why, but suppress a panic here.
        let mapping = self.optional_mapping().ok_or("row has no mapping")?;
        let within_same_group = self
            .main_state
            .borrow()
            .displayed_group_for_active_compartment()
            .is_some();
        let _ = self.session().borrow_mut().move_mapping_within_list(
            self.active_compartment(),
            mapping.borrow().id(),
            within_same_group,
            increment,
        );
        Ok(())
    }

    fn active_compartment(&self) -> CompartmentKind {
        self.main_state.borrow().active_compartment.get()
    }

    fn remove_mapping(&self) -> anyhow::Result<()> {
        let user_confirmed = self
            .view
            .require_window()
            .confirm("ReaLearn", "Do you really want to remove this mapping?");
        if !user_confirmed {
            return Ok(());
        }
        self.session()
            .borrow_mut()
            .remove_mapping(self.get_qualified_mapping_id()?);
        Ok(())
    }

    fn duplicate_mapping(&self) -> anyhow::Result<()> {
        self.session()
            .borrow_mut()
            .duplicate_mapping(self.get_qualified_mapping_id()?)
    }

    fn toggle_learn_source(&self) -> anyhow::Result<()> {
        let mapping_id = self.get_qualified_mapping_id()?;
        let shared_session = self.session();
        shared_session
            .borrow_mut()
            .toggle_learning_source(self.session.clone(), mapping_id)?;
        Ok(())
    }

    fn toggle_learn_target(&self) -> anyhow::Result<()> {
        let mapping_id = self.get_qualified_mapping_id()?;
        let shared_session = self.session();
        shared_session
            .borrow_mut()
            .toggle_learning_target(self.session.clone(), mapping_id);
        Ok(())
    }

    fn update_is_enabled(&self) {
        let checked = self
            .view
            .require_control(IDC_MAPPING_ROW_ENABLED_CHECK_BOX)
            .is_checked();
        self.change_mapping(MappingCommand::SetIsEnabled(checked));
    }

    fn update_control_is_enabled(&self) {
        let checked = self
            .view
            .require_control(ID_MAPPING_ROW_CONTROL_CHECK_BOX)
            .is_checked();
        self.change_mapping(MappingCommand::SetControlIsEnabled(checked));
    }

    fn update_feedback_is_enabled(&self) {
        let checked = self
            .view
            .require_control(ID_MAPPING_ROW_FEEDBACK_CHECK_BOX)
            .is_checked();
        self.change_mapping(MappingCommand::SetFeedbackIsEnabled(checked));
    }

    fn change_mapping(&self, cmd: MappingCommand) {
        let mapping = self.require_mapping();
        let mut mapping = mapping.borrow_mut();
        UnitModel::change_mapping_from_ui_simple(self.session.clone(), &mut mapping, cmd, None);
    }

    fn notify_user_on_error(&self, result: Result<(), Box<dyn Error>>) {
        if let Err(e) = result {
            self.view.require_window().alert("ReaLearn", e.to_string());
        }
    }

    fn paste_from_lua_replace(&self, text: &str) -> Result<(), Box<dyn Error>> {
        let active_compartment = self.active_compartment();
        let api_object = deserialize_api_object_from_lua(text, active_compartment)?;
        if !matches!(api_object, ApiObject::Mapping(Envelope { value: _, .. })) {
            return Err("There's more than one mapping in the clipboard.".into());
        }
        let data_object = {
            let session = self.session();
            let session = session.borrow();
            let compartment_in_session = session.compartment_in_session(active_compartment);
            DataObject::try_from_api_object(api_object, &compartment_in_session)?
        };
        paste_data_object_in_place(data_object, self.session(), self.mapping_triple()?)?;
        Ok(())
    }

    fn paste_from_lua_insert_below(&self, text: &str) -> Result<(), Box<dyn Error>> {
        let active_compartment = self.active_compartment();
        let api_object = deserialize_api_object_from_lua(text, active_compartment)?;
        let api_mappings = api_object
            .into_mappings()
            .ok_or("Can only insert a list of mappings.")?;
        let data_mappings = {
            let session = self.session();
            let session = session.borrow();
            let compartment_in_session = session.compartment_in_session(active_compartment);
            DataObject::try_from_api_mappings(api_mappings.value, &compartment_in_session)?
        };
        let triple = self.mapping_triple()?;
        paste_mappings(
            Envelope::new(api_mappings.version, data_mappings),
            self.session(),
            triple.compartment,
            Some(triple.mapping_id),
            triple.group_id,
        )
    }

    fn mapping_triple(&self) -> Result<MappingTriple, &'static str> {
        let mapping = self.mapping.borrow();
        let mapping = mapping.as_ref().ok_or("row contains no mapping")?;
        let mapping = mapping.borrow();
        let triple = MappingTriple {
            compartment: mapping.compartment(),
            mapping_id: mapping.id(),
            group_id: mapping.group_id(),
        };
        Ok(triple)
    }

    fn open_context_menu(&self, location: Point<Pixels>) -> Result<(), &'static str> {
        enum MenuAction {
            None,
            PasteObjectInPlace(DataObject),
            PasteMappings(Envelope<Vec<MappingModelData>>),
            CopyPart(ObjectType),
            MoveMappingToGroup(Option<GroupId>),
            CopyMappingAsLua(ConversionStyle),
            PasteFromLuaReplace(String),
            PasteFromLuaInsertBelow(String),
            LogDebugInfo,
        }
        impl Default for MenuAction {
            fn default() -> Self {
                Self::None
            }
        }
        let pure_menu = {
            use swell_ui::menu_tree::*;
            let shared_session = self.session();
            let session = shared_session.borrow();
            let mapping = self.mapping.borrow();
            let mapping = mapping.as_ref().ok_or("row contains no mapping")?;
            let mapping = mapping.borrow();
            let compartment = mapping.compartment();
            let text_from_clipboard = get_text_from_clipboard();
            let data_object_from_clipboard = text_from_clipboard
                .as_ref()
                .and_then(|text| deserialize_data_object_from_json(text).ok());
            let clipboard_could_contain_lua =
                text_from_clipboard.is_some() && data_object_from_clipboard.is_none();
            let text_from_clipboard_clone = text_from_clipboard.clone();
            let data_object_from_clipboard_clone = data_object_from_clipboard.clone();
            let group_id = mapping.group_id();
            let entries = vec![
                item("Copy", MenuAction::CopyPart(ObjectType::Mapping)),
                {
                    let desc = match data_object_from_clipboard {
                        Some(DataObject::Mapping(Envelope { value: m, version })) => Some((
                            format!("Paste mapping \"{}\" (replace)", &m.name),
                            DataObject::Mapping(Envelope { value: m, version }),
                        )),
                        Some(DataObject::Source(Envelope { value: s, version })) => Some((
                            format!("Paste source ({})", s.category),
                            DataObject::Source(Envelope { value: s, version }),
                        )),
                        Some(DataObject::Glue(Envelope { value: m, version })) => Some((
                            "Paste glue".to_owned(),
                            DataObject::Glue(Envelope { value: m, version }),
                        )),
                        Some(DataObject::Target(Envelope { value: t, version })) => Some((
                            format!("Paste target ({})", t.category),
                            DataObject::Target(Envelope { value: t, version }),
                        )),
                        Some(DataObject::ActivationCondition(Envelope { value: t, version })) => {
                            Some((
                                format!("Paste activation condition ({})", t.activation_type),
                                DataObject::ActivationCondition(Envelope { value: t, version }),
                            ))
                        }
                        _ => None,
                    };
                    if let Some((label, obj)) = desc {
                        item(label, MenuAction::PasteObjectInPlace(obj))
                    } else {
                        disabled_item("Paste (replace)")
                    }
                },
                {
                    let desc = match data_object_from_clipboard_clone {
                        Some(DataObject::Mapping(Envelope { value: m, version })) => Some((
                            format!("Paste mapping \"{}\" (insert below)", &m.name),
                            Envelope::new(version, vec![*m]),
                        )),
                        Some(DataObject::Mappings(Envelope {
                            value: vec,
                            version,
                        })) => Some((
                            format!("Paste {} mappings below", vec.len()),
                            Envelope::new(version, vec),
                        )),
                        _ => None,
                    };
                    if let Some((label, datas)) = desc {
                        item(label, MenuAction::PasteMappings(datas))
                    } else {
                        disabled_item("Paste (insert below)")
                    }
                },
                menu(
                    "Copy part",
                    vec![
                        item(
                            "Copy activation condition",
                            MenuAction::CopyPart(ObjectType::ActivationCondition),
                        ),
                        item("Copy source", MenuAction::CopyPart(ObjectType::Source)),
                        item("Copy glue", MenuAction::CopyPart(ObjectType::Glue)),
                        item("Copy target", MenuAction::CopyPart(ObjectType::Target)),
                    ],
                ),
                menu(
                    "Move to group",
                    iter::once(item("<New group>", MenuAction::MoveMappingToGroup(None)))
                        .chain(session.groups_sorted(compartment).map(move |g| {
                            let g = g.borrow();
                            let g_id = g.id();
                            item_with_opts(
                                g.to_string(),
                                ItemOpts {
                                    enabled: group_id != g_id,
                                    checked: false,
                                },
                                MenuAction::MoveMappingToGroup(Some(g_id)),
                            )
                        }))
                        .collect(),
                ),
                menu(
                    "Advanced",
                    vec![
                        item(
                            "Copy as Lua",
                            MenuAction::CopyMappingAsLua(ConversionStyle::Minimal),
                        ),
                        item(
                            "Copy as Lua (include default values)",
                            MenuAction::CopyMappingAsLua(ConversionStyle::IncludeDefaultValues),
                        ),
                        item_with_opts(
                            "Paste from Lua (replace)",
                            ItemOpts {
                                enabled: clipboard_could_contain_lua,
                                checked: false,
                            },
                            MenuAction::PasteFromLuaReplace(
                                text_from_clipboard.unwrap_or_default(),
                            ),
                        ),
                        item_with_opts(
                            "Paste from Lua (insert below)",
                            ItemOpts {
                                enabled: clipboard_could_contain_lua,
                                checked: false,
                            },
                            MenuAction::PasteFromLuaInsertBelow(
                                text_from_clipboard_clone.unwrap_or_default(),
                            ),
                        ),
                        item("Log debug info (now)", MenuAction::LogDebugInfo),
                    ],
                ),
            ];
            anonymous_menu(entries)
        };
        let result = self
            .view
            .require_window()
            .open_popup_menu(pure_menu, location)
            .ok_or("no entry selected")?;
        let triple = self.mapping_triple()?;
        match result {
            MenuAction::None => {}
            MenuAction::PasteObjectInPlace(obj) => {
                let _ = paste_data_object_in_place(obj, self.session(), triple);
            }
            MenuAction::PasteFromLuaReplace(text) => {
                self.notify_user_on_error(self.paste_from_lua_replace(&text));
            }
            MenuAction::PasteMappings(datas) => {
                let result = paste_mappings(
                    datas,
                    self.session(),
                    triple.compartment,
                    Some(triple.mapping_id),
                    triple.group_id,
                );
                self.notify_user_on_error(result);
            }
            MenuAction::PasteFromLuaInsertBelow(text) => {
                self.notify_user_on_error(self.paste_from_lua_insert_below(&text));
            }
            MenuAction::CopyPart(obj_type) => {
                copy_mapping_object(
                    self.session(),
                    triple.compartment,
                    triple.mapping_id,
                    obj_type,
                    SerializationFormat::JsonDataObject,
                )
                .unwrap();
            }
            MenuAction::CopyMappingAsLua(style) => {
                copy_mapping_object(
                    self.session(),
                    triple.compartment,
                    triple.mapping_id,
                    ObjectType::Mapping,
                    SerializationFormat::LuaApiObject(style),
                )
                .unwrap();
            }
            MenuAction::MoveMappingToGroup(group_id) => {
                let _ = move_mapping_to_group(
                    self.session(),
                    triple.compartment,
                    triple.mapping_id,
                    group_id,
                );
            }
            MenuAction::LogDebugInfo => {
                let _ = self
                    .session()
                    .borrow()
                    .log_mapping(triple.compartment, triple.mapping_id);
            }
        }
        Ok(())
    }

    fn when(
        self: &SharedView<Self>,
        event: impl LocalObservable<'static, Item = (), Err = ()> + 'static,
        reaction: impl Fn(SharedView<Self>) + 'static + Copy,
    ) {
        when(event.take_until(self.closed_or_mapping_will_change()))
            .with(Rc::downgrade(self))
            .do_sync(move |panel, _| reaction(panel));
    }
}

impl View for MappingRowPanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_MAPPING_ROW_PANEL
    }

    fn view_context(&self) -> &ViewContext {
        &self.view
    }

    fn opened(self: SharedView<Self>, window: Window) -> bool {
        window.hide();
        if cfg!(unix) {
            self.source_color_panel.clone().open(window);
            self.target_color_panel.clone().open(window);
            // Must be the last because we want it below the others
            self.mapping_color_panel.clone().open(window);
        }
        window.move_to_dialog_units(Point::new(
            DialogUnits(0),
            mapping_row_panel_height() * self.row_index,
        ));
        self.init_symbol_controls();
        false
    }

    fn erase_background(self: SharedView<Self>, device_context: DeviceContext) -> bool {
        if cfg!(unix) {
            // On macOS/Linux we use color panels as real child windows.
            return false;
        }
        if !BackboneShell::get().config().background_colors_enabled() {
            return false;
        }
        let window = self.view.require_window();
        // Must be the first because we want it below the others
        self.mapping_color_panel
            .paint_manually(device_context, window);
        self.source_color_panel
            .paint_manually(device_context, window);
        self.target_color_panel
            .paint_manually(device_context, window);
        true
    }

    fn control_color_static(
        self: SharedView<Self>,
        device_context: DeviceContext,
        window: Window,
    ) -> Option<Hbrush> {
        if cfg!(target_os = "macos") {
            // On macOS, we fortunately don't need to do this nonsense. And it wouldn't be possible
            // anyway because SWELL macOS can't distinguish between different child controls.
            return None;
        }
        if !BackboneShell::get().config().background_colors_enabled() {
            return None;
        }
        device_context.set_bk_mode_to_transparent();
        let color_pair = match window.resource_id() {
            root::ID_MAPPING_ROW_SOURCE_LABEL_TEXT => colors::source(),
            root::ID_MAPPING_ROW_TARGET_LABEL_TEXT => colors::target(),
            root::ID_MAPPING_ROW_MAPPING_LABEL | root::ID_MAPPING_ROW_GROUP_LABEL => {
                colors::show_background()
            }
            _ => colors::mapping(),
        };
        view::get_brush_for_color_pair(color_pair)
    }

    fn button_clicked(self: SharedView<Self>, resource_id: u32) {
        match resource_id {
            root::IDC_MAPPING_ROW_ENABLED_CHECK_BOX => self.update_is_enabled(),
            root::ID_MAPPING_ROW_EDIT_BUTTON => {
                self.edit_mapping();
            }
            root::ID_UP_BUTTON => {
                let _ = self.move_mapping_within_list(-1);
            }
            root::ID_DOWN_BUTTON => {
                let _ = self.move_mapping_within_list(1);
            }
            root::ID_MAPPING_ROW_REMOVE_BUTTON => {
                let _ = self.remove_mapping();
            }
            root::ID_MAPPING_ROW_DUPLICATE_BUTTON => {
                let _ = self.duplicate_mapping();
            }
            root::ID_MAPPING_ROW_LEARN_SOURCE_BUTTON => {
                let _ = self.toggle_learn_source();
            }
            root::ID_MAPPING_ROW_LEARN_TARGET_BUTTON => {
                let _ = self.toggle_learn_target();
            }
            root::ID_MAPPING_ROW_CONTROL_CHECK_BOX => self.update_control_is_enabled(),
            root::ID_MAPPING_ROW_FEEDBACK_CHECK_BOX => self.update_feedback_is_enabled(),
            _ => {}
        }
    }

    fn context_menu_wanted(self: SharedView<Self>, location: Point<Pixels>) -> bool {
        let _ = self.open_context_menu(location);
        true
    }

    fn timer(&self, id: usize) -> bool {
        if id == SOURCE_MATCH_INDICATOR_TIMER_ID {
            self.view
                .require_window()
                .kill_timer(SOURCE_MATCH_INDICATOR_TIMER_ID);
            self.source_match_indicator_control().disable();
            true
        } else {
            false
        }
    }
}

impl Drop for MappingRowPanel {
    fn drop(&mut self) {
        debug!("Dropping mapping row panel...");
    }
}

fn move_mapping_to_group(
    session: SharedUnitModel,
    compartment: CompartmentKind,
    mapping_id: MappingId,
    group_id: Option<GroupId>,
) -> Result<(), &'static str> {
    let cloned_session = session.clone();
    let group_id = group_id
        .or_else(move || add_group_via_dialog(cloned_session, compartment).ok())
        .ok_or("no group selected")?;
    session.borrow_mut().move_mappings_to_group(
        compartment,
        &[mapping_id],
        group_id,
        Rc::downgrade(&session),
    )?;
    Ok(())
}

fn copy_mapping_object(
    session: SharedUnitModel,
    compartment: CompartmentKind,
    mapping_id: MappingId,
    object_type: ObjectType,
    format: SerializationFormat,
) -> Result<(), Box<dyn Error>> {
    let session = session.borrow();
    let mapping = session
        .find_mapping_by_id(compartment, mapping_id)
        .ok_or("mapping not found")?;
    use ObjectType::*;
    let mapping = mapping.borrow();
    let compartment_in_session = session.compartment_in_session(compartment);
    let data_object = match object_type {
        Mapping => DataObject::Mapping(BackboneShell::create_envelope(Box::new(
            MappingModelData::from_model(&mapping, &compartment_in_session),
        ))),
        Source => DataObject::Source(BackboneShell::create_envelope(Box::new(
            SourceModelData::from_model(&mapping.source_model),
        ))),
        Glue => DataObject::Glue(BackboneShell::create_envelope(Box::new(
            ModeModelData::from_model(&mapping.mode_model),
        ))),
        Target => DataObject::Target(BackboneShell::create_envelope(Box::new(
            TargetModelData::from_model(&mapping.target_model, &compartment_in_session),
        ))),
        ActivationCondition => DataObject::ActivationCondition(BackboneShell::create_envelope(
            Box::new(ActivationConditionData::from_model(
                &mapping.activation_condition_model,
                &compartment_in_session,
            )),
        )),
    };
    let text = serialize_data_object(data_object, format)?;
    copy_text_to_clipboard(text);
    Ok(())
}

enum ObjectType {
    Mapping,
    Source,
    Glue,
    Target,
    ActivationCondition,
}

fn paste_data_object_in_place(
    data_object: DataObject,
    shared_session: SharedUnitModel,
    triple: MappingTriple,
) -> Result<(), &'static str> {
    let mut session = shared_session.borrow_mut();
    let mapping = session
        .find_mapping_by_id(triple.compartment, triple.mapping_id)
        .ok_or("mapping not found")?
        .clone();
    BackboneShell::warn_if_envelope_version_higher(data_object.version());
    let mut mapping = mapping.borrow_mut();
    match data_object {
        DataObject::Mapping(Envelope {
            value: mut m,
            version,
        }) => {
            m.group_id = {
                if triple.group_id.is_default() {
                    GroupKey::default()
                } else {
                    let group = session
                        .find_group_by_id(triple.compartment, triple.group_id)
                        .ok_or("couldn't find group")?;
                    group.borrow().key().clone()
                }
            };
            let conversion_context = session.compartment_in_session(mapping.compartment());
            // TODO-medium It would simplify things if we would just translate this into a new model
            //  and then call a Session method to completely replace a model by its ID. Same with
            //  other data object types.
            m.apply_to_model(
                &mut mapping,
                &conversion_context,
                Some(session.extended_context()),
                version.as_ref(),
            )?;
        }
        DataObject::Source(Envelope { value: s, .. }) => {
            s.apply_to_model(&mut mapping.source_model, triple.compartment);
        }
        DataObject::Glue(Envelope { value: m, .. }) => {
            m.apply_to_model(&mut mapping.mode_model);
        }
        DataObject::Target(Envelope { value: t, .. }) => {
            let compartment_in_session = session.compartment_in_session(triple.compartment);
            t.apply_to_model(
                &mut mapping.target_model,
                triple.compartment,
                session.extended_context(),
                &compartment_in_session,
            )?;
        }
        DataObject::ActivationCondition(Envelope { value: c, .. }) => {
            let compartment_in_session = session.compartment_in_session(triple.compartment);
            c.apply_to_model(
                &mut mapping.activation_condition_model,
                &compartment_in_session,
            );
        }
        _ => return Err("can only paste mapping, source, mode and target in place"),
    };
    session.notify_mapping_has_changed(mapping.qualified_id(), Rc::downgrade(&shared_session));
    Ok(())
}

/// If `below_mapping_id` not given, it's added at the end.
// https://github.com/rust-lang/rust-clippy/issues/6066
#[allow(clippy::needless_collect)]
pub fn paste_mappings(
    mapping_datas: Envelope<Vec<MappingModelData>>,
    session: SharedUnitModel,
    compartment: CompartmentKind,
    below_mapping_id: Option<MappingId>,
    group_id: GroupId,
) -> Result<(), Box<dyn Error>> {
    let mut session = session.borrow_mut();
    let index = if let Some(id) = below_mapping_id {
        session
            .find_mapping_and_index_by_id(compartment, id)
            .ok_or("mapping not found")?
            .0
    } else {
        session.mapping_count(compartment)
    };
    let group_key = {
        if group_id.is_default() {
            GroupKey::default()
        } else {
            let group = session
                .find_group_by_id(compartment, group_id)
                .ok_or("couldn't find group")?;
            let group = group.borrow();
            group.key().clone()
        }
    };
    let new_mappings: Result<Vec<_>, _> = mapping_datas
        .value
        .into_iter()
        .map(|mut data| {
            data.id = None;
            data.group_id = group_key.clone();
            data.to_model(
                compartment,
                &session.compartment_in_session(compartment),
                Some(session.extended_context()),
                mapping_datas.version.as_ref(),
            )
        })
        .collect();
    session.insert_mappings_at(compartment, index + 1, new_mappings?.into_iter());
    Ok(())
}

const SOURCE_MATCH_INDICATOR_TIMER_ID: usize = 571;

struct MappingTriple {
    compartment: CompartmentKind,
    mapping_id: MappingId,
    group_id: GroupId,
}

fn build_mapping_color_panel_desc() -> ColorPanelDesc {
    ColorPanelDesc {
        x: 0,
        y: 0,
        width: 460,
        height: COLOR_PANEL_HEIGHT,
        color_pair: colors::mapping(),
        scaling: GLOBAL_SCALING,
    }
}

fn build_source_color_panel_desc() -> ColorPanelDesc {
    ColorPanelDesc {
        x: 43,
        y: 0,
        width: 94,
        height: COLOR_PANEL_HEIGHT,
        color_pair: colors::source(),
        scaling: GLOBAL_SCALING,
    }
}

fn build_target_color_panel_desc() -> ColorPanelDesc {
    ColorPanelDesc {
        x: 161,
        y: 0,
        width: 182,
        height: COLOR_PANEL_HEIGHT,
        color_pair: colors::target(),
        scaling: GLOBAL_SCALING,
    }
}

const COLOR_PANEL_HEIGHT: u32 = 48;
